//! Explicit call context (kition-sdk `KitDocument::editor_state`, `set_context`; Kit Process
//! Protocol section 7a; task card C4/C5/C6-sheet): the editor state a command may read besides its
//! parameters, taken and put back without an edit, set for one call from what the call says it
//! acts on, and kept out of an agent's commands that name what they act on.
//!
//! A call's **part** is a sheet: `{"sheet": 3}` (its id), `{"sheet": "sheet:3"}` (its tool id) or
//! `{"sheet": "Data"}` (its name; names are unique in a workbook, compared without case). Never a
//! position: positions change when sheets move.
//!
//! A call's **`on`** is either cells or drawing objects, never both:
//! - cells: ranges in A1 notation (`"B2:D10"`, `"A1"`, `"C:C"`, `"3:5"`), each optionally with its
//!   sheet (`"Data!B2:D10"`, `"'My Sheet'!A1"`), which must be the call's part when it has one. They
//!   become the selection on that sheet, in the order given; the active cell is the first cell of
//!   the last range. Without a part or a sheet in the range, a workbook of more than one sheet
//!   refuses them: an address means something only on its sheet.
//! - drawing objects: their ids as the inspect tool lists them, `"drawing:5"`, or the number
//!   (`5`, `"5"`). A drawing object names its sheet; they must all be on one sheet, the call's part
//!   when it has one. The first becomes the selected object, the others are added to it.
//!
//! An item that is neither, a mixed list, a range out of the sheet's bounds, and a sheet or object
//! the workbook lacks are refused, saying what is valid, and nothing changes. Spreadsheets have no
//! time: `time` is refused as not available.

use kition_kit::CallContext;
use kition_sheet_engine::core::{CellRef, RangeRef};
use kition_sheet_engine::model::Workbook;
use kition_sheet_engine::{EditorState, Selection};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::ids::{self, ToolId};
use crate::{SheetDocument, engine_error};

/// What the binding keeps of the editor besides the engine's state: drawing objects added to the
/// selection with Shift.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub engine: EditorState,
    pub also_selected: Vec<u32>,
}

/// Commands whose work is moving the editor: they keep their effect for an agent too.
pub(crate) fn moves_the_editor(command: &str) -> bool {
    command.starts_with("selection.") && command != "select.stats"
        || command.starts_with("view.")
        || matches!(
            command,
            "sheet.activate"
                | "sheet.next"
                | "sheet.previous"
                | "edit.selectAll"
                | "edit.goTo"
                | "edit.goToSpecial"
                | "edit.find"
                | "window.activate"
        )
}

/// The parameters that name what a command acts on: a sheet, cells, drawing objects.
const NAMING: [&str; 8] = ["sheet", "range", "cell", "rows", "cols", "id", "ids", "chart"];

/// One `on` item.
enum On {
    Range(Option<String>, RangeRef),
    Drawing(u32),
}

fn on_item(item: &Json) -> Result<On, String> {
    let text = match item {
        Json::Number(number) => {
            return number.as_u64().and_then(|n| u32::try_from(n).ok()).map(On::Drawing).ok_or_else(|| {
                format!("`on` holds {item}: a drawing object's id is a whole number (or its tool id `drawing:<id>`), as sheet_inspect lists it")
            });
        }
        Json::String(text) => text.trim(),
        other => return Err(format!("`on` holds {other}: an item is a range in A1 notation (`B2:D10`) or a drawing object's id (`drawing:5`)")),
    };
    match ToolId::parse(text) {
        Some(ToolId::Drawing(id)) => return Ok(On::Drawing(id)),
        Some(ToolId::Sheet(_)) => {
            return Err(format!(
                "`on` holds `{text}`, a sheet: a sheet is the call's part (the run's `target` or the step's `at.part`, `{{\"sheet\": …}}`), and `on` names cells or drawing objects on it"
            ));
        }
        None => {}
    }
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        return text.parse::<u32>().map(On::Drawing).map_err(|_| format!("`on` holds `{text}`, which is no drawing object's id"));
    }
    let (sheet, body) = match text.rsplit_once('!') {
        Some((sheet, body)) => (Some(sheet.trim_matches('\'').replace("''", "'")), body),
        None => (None, text),
    };
    let range = RangeRef::parse(body).ok_or_else(|| {
        format!(
            "`on` holds `{text}`, which is neither a range in A1 notation within the sheet (columns A to XFD, rows 1 to 1048576; `B2:D10`, `Data!A1`) nor a drawing object's id (`drawing:5`)"
        )
    })?;
    Ok(On::Range(sheet, range))
}

/// The sheet a call's part names (`{"sheet": id | "sheet:<id>" | name}`), by index.
pub(crate) fn part_sheet(wb: &Workbook, part: &Json) -> Result<usize, String> {
    let listed = "the sheets' ids, tool ids and names are in sheet_inspect's `workbook.sheets`";
    let Some(fields) = part.as_object() else { return Err(format!("the part {part} is not `{{\"sheet\": id}}`: {listed}")) };
    if let Some(other) = fields.keys().find(|key| *key != "sheet") {
        return Err(format!("the part has `{other}`, which names no part of a workbook: a part is `{{\"sheet\": id}}`; {listed}"));
    }
    match fields.get("sheet") {
        Some(Json::Number(number)) => number
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .and_then(|id| ids::sheet_index(wb, id))
            .ok_or_else(|| format!("no sheet has id {number} (a sheet's id, not its position): {listed}")),
        Some(Json::String(text)) => match ToolId::parse(text) {
            Some(ToolId::Sheet(id)) => ids::sheet_index(wb, id).ok_or_else(|| format!("no sheet has the tool id `{text}`: {listed}")),
            Some(ToolId::Drawing(_)) => Err(format!("`{text}` is a drawing object, not a sheet: {listed}")),
            None => wb.sheet_index(text).ok_or_else(|| format!("the workbook has no sheet named `{text}`: {listed}")),
        },
        _ => Err(format!("the part {part} names no sheet: a part is `{{\"sheet\": id}}`; {listed}")),
    }
}

impl SheetDocument {
    pub(crate) fn editor_snapshot(&self) -> Option<Snapshot> {
        self.session.editor_state().map(|engine| Snapshot { engine, also_selected: self.also_selected.clone() })
    }

    pub(crate) fn put_back(&mut self, snapshot: &Snapshot) -> Result<(), String> {
        self.session.restore_editor_state(&snapshot.engine).map_err(engine_error)?;
        let selected = self.session.selected_shape;
        let wb = self.doc_state()?.wb.clone();
        self.also_selected =
            snapshot.also_selected.iter().copied().filter(|id| selected.is_some() && ids::drawing_sheet(&wb, *id).is_some()).collect();
        Ok(())
    }

    /// Sets the sheet a call acts in and what it acts on there (module docs). Everything is checked
    /// before anything changes; nothing of it is an edit.
    pub(crate) fn set_call_context(&mut self, context: &CallContext) -> Result<(), String> {
        if context.time.is_some() {
            return Err("a workbook has no timeline: setting the current time (`at.time`) is not available in this kit".into());
        }
        let wb = self.doc_state()?.wb.clone();
        let part = context.part.as_ref().map(|part| part_sheet(&wb, part)).transpose()?;
        let items = context.on.iter().map(on_item).collect::<Result<Vec<_>, _>>()?;
        let ranges = items.iter().filter(|item| matches!(item, On::Range(..))).count();
        if ranges > 0 && ranges < items.len() {
            return Err(
                "`on` mixes cells and drawing objects: a selection is cells or drawing objects, so give one kind per step (cells as A1 ranges, objects as `drawing:<id>`)".into(),
            );
        }
        let name = |index: usize| wb.sheet(index).map_or_else(String::new, |sheet| sheet.name.clone());
        // The sheet `on` names, which must agree with the part.
        let mut named: Option<usize> = None;
        let mut agree = |index: usize, what: &str| -> Result<(), String> {
            match (part, named) {
                (Some(part), _) if part != index => Err(format!(
                    "{what} is on sheet `{}`, and the call's part is sheet `{}`: give it on the part's sheet, or leave the part out",
                    name(index),
                    name(part)
                )),
                (_, Some(other)) if other != index => {
                    Err(format!("`on` names sheets `{}` and `{}`: one step acts on one sheet; give them in separate steps", name(other), name(index)))
                }
                _ => {
                    named = Some(index);
                    Ok(())
                }
            }
        };
        let mut cells: Vec<RangeRef> = Vec::new();
        let mut drawings: Vec<u32> = Vec::new();
        for item in &items {
            match item {
                On::Range(Some(sheet), range) => {
                    let index = wb.sheet_index(sheet).ok_or_else(|| {
                        format!("`on` names sheet `{sheet}`, which the workbook lacks: the sheets' names are in sheet_inspect's `workbook.sheets`")
                    })?;
                    agree(index, &format!("`{sheet}!{}`", range.a1()))?;
                    cells.push(*range);
                }
                On::Range(None, range) => cells.push(*range),
                On::Drawing(id) => {
                    let (index, _) = ids::drawing_sheet(&wb, *id).ok_or_else(|| {
                        format!("the workbook has no drawing object `drawing:{id}`: drawing objects' ids are in sheet_inspect's `workbook.sheets[].drawings`")
                    })?;
                    agree(index, &format!("`drawing:{id}`"))?;
                    if !drawings.contains(id) {
                        drawings.push(*id);
                    }
                }
            }
        }
        let sheet = part.or(named);
        if !cells.is_empty() && sheet.is_none() && wb.sheets.len() > 1 {
            return Err(format!(
                "`on` names cells, and an address means something only on its sheet, and this workbook has {} sheets: give the sheet as the run's `target` or the step's `at.part` (`{{\"sheet\": id}}`), or write it in the range (`Data!B2:D10`)",
                wb.sheets.len()
            ));
        }
        // Checked; now set.
        let before = self.editor_snapshot();
        if let Some(index) = sheet
            && index != wb.active_sheet
        {
            self.session.show_sheet(index, None).map_err(engine_error)?;
            self.session.selected_shape = None;
            self.also_selected.clear();
        }
        if !cells.is_empty() {
            let last = cells.last().copied().unwrap_or_default();
            let active: CellRef = last.start;
            let selection = Selection { active, anchor: active, ranges: cells };
            match self.session.doc_mut() {
                Ok(d) => d.selection = selection,
                Err(error) => {
                    if let Some(before) = before {
                        let _ = self.put_back(&before);
                    }
                    return Err(engine_error(error));
                }
            }
            self.session.selected_shape = None;
            self.also_selected.clear();
        }
        if let Some((first, rest)) = drawings.split_first() {
            self.session.selected_shape = Some(*first);
            self.also_selected = rest.to_vec();
        }
        Ok(())
    }

    /// Writes the tool ids in `params` as the kit's own: a sheet's (`sheet:<id>`) as its name, a
    /// drawing object's (`drawing:<id>`) as its number, in the type the command's parameter takes.
    /// The one place tool ids become the kit's numbers for commands.
    /// Answers the sheet the drawing objects' tool ids name, which the command then acts in;
    /// objects of two sheets in one call are refused.
    pub(crate) fn take_tool_ids(&self, command: &str, params: &mut Json, schema: Option<&Json>) -> Result<Option<usize>, String> {
        let Some(map) = params.as_object_mut() else { return Ok(None) };
        let wb = &self.doc_state()?.wb;
        let mut implied: Option<(usize, String)> = None;
        for (key, value) in map.iter_mut() {
            if command == "spelling.change"
                && key == "cell"
                && let Some(text) = value.as_str().filter(|text| text.starts_with("cell:"))
            {
                let body = text.strip_prefix("cell:").ok_or_else(|| format!("invalid cell tool id: {text}"))?;
                let (sheet, address) = body.split_once('!').ok_or_else(|| format!("invalid cell tool id: {text}"))?;
                let sheet = sheet.parse::<u32>().map_err(|_| format!("invalid cell tool id: {text}"))?;
                let at = CellRef::parse(address).ok_or_else(|| format!("invalid cell tool id: {text}"))?;
                let index = ids::sheet_index(wb, sheet).ok_or_else(|| format!("no sheet for {text}"))?;
                implied = Some((index, text.to_owned()));
                *value = Json::from(at.a1());
                continue;
            }
            if command == "cell.set"
                && key == "name"
                && let Some(text) = value.as_str().filter(|text| text.starts_with("name:"))
            {
                let kdm = self.doc_state()?.kdm().ok_or("name tool ids require an object model")?;
                let names = kition_kit::kdm::tool_ids::ToolIds::of_replica(kdm.replica());
                let raw = names.resolve(text, |id| kdm.replica().exists(id)).map_err(|_| format!("no defined name has the tool id {text}"))?;
                let definition = kdm
                    .model()
                    .ids()
                    .names
                    .iter()
                    .position(|id| id == &raw)
                    .and_then(|index| wb.names.get(index))
                    .ok_or_else(|| format!("{text} is not a defined name"))?;
                if let Some(sheet) = definition.scope {
                    implied = Some((sheet, text.to_owned()));
                }
                *value = Json::from(definition.name.clone());
                continue;
            }
            let style_kind = match (command, key.as_str()) {
                ("style.apply", "name") => Some("cell"),
                ("table.newStyle", "replace") | ("table.deleteStyle" | "table.setDefaultStyle", "name") | ("table.style", "style") => Some("table"),
                _ => None,
            };
            if let Some(kind) = style_kind
                && let Some(text) = value.as_str().filter(|text| text.starts_with("style:"))
            {
                let kdm = self.doc_state()?.kdm().ok_or("style tool ids require an object model")?;
                let replica = kdm.replica();
                let names = kition_kit::kdm::tool_ids::ToolIds::of_replica(replica);
                let object = names.resolve(text, |id| replica.exists(id)).map_err(|_| format!("no named style has the tool id {text}"))?;
                if replica.object_type(&object).ok().as_deref() != Some("Style")
                    || replica.get(&object, "kind").map_err(|error| error.to_string())?.as_str() != Some(kind)
                {
                    return Err(format!("{text} is not a named {kind} style"));
                }
                *value = replica.get(&object, "name").map_err(|error| error.to_string())?;
                continue;
            }
            if matches!(key.as_str(), "table" | "pivot")
                && let Some(text) = value.as_str()
                && let Some(body) = text.strip_prefix(&format!("{key}:"))
            {
                let (sheet, native) = body.split_once(':').ok_or_else(|| format!("invalid {key} tool id: {text}"))?;
                let sheet = sheet.parse::<u32>().map_err(|error| error.to_string())?;
                let native = native.parse::<u32>().map_err(|error| error.to_string())?;
                let index = ids::sheet_index(wb, sheet).ok_or_else(|| format!("no sheet for {text}"))?;
                let owner = wb.sheet(index).ok_or_else(|| format!("no sheet for {text}"))?;
                let resolved = if key == "table" {
                    owner.tables.iter().find(|table| table.id == native).map(|table| Json::from(table.name.clone()))
                } else {
                    owner.pivots.iter().find(|pivot| pivot.id == native).map(|pivot| Json::from(pivot.id))
                }
                .ok_or_else(|| format!("no object has the tool id {text}"))?;
                if implied.as_ref().is_some_and(|(first, _)| *first != index) {
                    return Err("one call cannot target feature objects on different sheets".into());
                }
                implied = Some((index, text.to_owned()));
                *value = resolved;
                continue;
            }
            let drawing = crate::references::drawing_parameters(command).contains(&key.as_str());
            let sheet = crate::references::sheet_parameter(key) && schema.is_some_and(|schema| schema.get(key.as_str()).is_some());
            if !drawing && !sheet {
                continue;
            }
            let items: Vec<&mut Json> = match value {
                Json::Array(items) => items.iter_mut().collect(),
                other => vec![other],
            };
            for item in items {
                let Some(text) = item.as_str() else { continue };
                match ToolId::parse(text) {
                    Some(ToolId::Sheet(id)) if sheet => {
                        let index = ids::sheet_index(wb, id).ok_or_else(|| {
                            format!("{command}: no sheet has the tool id `{text}`: the sheets' ids are in sheet_inspect's `workbook.sheets`")
                        })?;
                        *item = Json::from(wb.sheet(index).map(|sheet| sheet.name.clone()).unwrap_or_default());
                    }
                    Some(ToolId::Drawing(id)) if drawing => {
                        if let Some((index, _)) = ids::drawing_sheet(wb, id) {
                            match &implied {
                                Some((first, named)) if *first != index => {
                                    return Err(format!(
                                        "{command}: `{named}` and `{text}` are drawing objects of two sheets: one call acts on one sheet's objects; give them in separate steps"
                                    ));
                                }
                                Some(_) => {}
                                None => implied = Some((index, text.to_owned())),
                            }
                        }
                        *item = Json::from(id);
                    }
                    _ => {}
                }
            }
        }
        Ok(implied.map(|(index, _)| index))
    }

    /// The sheet an agent's command names (`sheet`, or the sheet of its range, cell, rows or
    /// columns), by index, when the workbook has it.
    pub(crate) fn named_sheet(&self, params: &Json) -> Option<usize> {
        let wb = &self.doc_state().ok()?.wb;
        match params.get("sheet") {
            Some(Json::Number(number)) => return number.as_u64().and_then(|n| usize::try_from(n).ok()).filter(|n| *n < wb.sheets.len()),
            Some(Json::String(name)) => return wb.sheet_index(name),
            _ => {}
        }
        ["range", "cell", "rows", "cols"].iter().find_map(|key| {
            let text = params.get(*key)?.as_str()?;
            let (sheet, _) = text.rsplit_once('!')?;
            wb.sheet_index(&sheet.trim_matches('\'').replace("''", "'"))
        })
    }

    pub(crate) fn command_target_sheet(&self, command: &str, params: &Json, implied: Option<usize>) -> Option<usize> {
        // A move destination and a replacement data source name another sheet, not
        // the sheet owning the chart. Its explicit id or the current part selects it.
        if matches!(command, "chart.move" | "chart.selectData") { implied } else { self.named_sheet(params).or(implied) }
    }

    /// Whether an agent's command names what it acts on in its parameters.
    pub(crate) fn names_target(params: &Json) -> bool {
        NAMING.iter().any(|key| params.get(*key).is_some_and(|value| !value.is_null()))
    }
}
