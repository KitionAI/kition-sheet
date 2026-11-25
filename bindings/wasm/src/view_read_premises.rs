//! View, lifecycle and spelling premises follow the handler's chosen path.
use crate::SheetDocument;
use crate::place::{area, merged, premise, window};
use kition_kit::{Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, Under};
use kition_sheet_engine::core::{CellRef, RangeRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn spelling_headers(sheet: &kition_sheet_engine::model::Sheet) -> Result<Option<BTreeMap<u32, Value>>, String> {
    if sheet.tables.len() > 100_000 {
        return Ok(None);
    }
    let mut headers: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
    let mut count = 0usize;
    for table in sheet.tables.iter().filter(|table| table.header_row) {
        let width = table.range.end.col.saturating_sub(table.range.start.col).saturating_add(1) as usize;
        let Some(next) = count.checked_add(width).and_then(|count| count.checked_add(table.columns.len())).filter(|count| *count <= 100_000) else {
            return Ok(None);
        };
        count = next;
        let names: Vec<_> = table.columns.iter().map(|column| column.name.as_str()).collect();
        headers.entry(table.range.start.row).or_default().push(json!([table.id, table.name, table.range, names]));
    }
    headers
        .into_iter()
        .map(|(row, values)| {
            let bytes = serde_json::to_vec(&values).map_err(|error| error.to_string())?;
            Ok((row, json!(kition_kit::session::sha256_hex(&bytes))))
        })
        .collect::<Result<_, String>>()
        .map(Some)
}

fn spelling_inputs(sheet: &kition_sheet_engine::model::Sheet, include_numbers: bool) -> Option<BTreeMap<u32, Vec<(u32, String)>>> {
    if sheet.cells.len() > 100_000 {
        return None;
    }
    let mut rows: BTreeMap<u32, Vec<(u32, String)>> = BTreeMap::new();
    for (at, cell) in sheet.cells.iter() {
        if !include_numbers && cell.formula.is_none() && matches!(cell.value, kition_sheet_engine::core::Value::Number(_)) {
            continue;
        }
        rows.entry(at.row).or_default().push((at.col, sheet.input_text(at)));
    }
    Some(rows)
}

impl SheetDocument {
    pub(crate) fn view_covered(&self, command: &str, given: &Value, objects: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
        if command != "spelling.change" || given.get("all").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }
        let mut params = given.clone();
        let implied = self.take_tool_ids(command, &mut params, self.command_schema(command).as_ref())?;
        let Some(word) = params.get("word").and_then(Value::as_str) else { return Ok(None) };
        let state = self.doc_state()?;
        let si = self.command_target_sheet(command, &params, implied).unwrap_or(state.wb.active_sheet);
        let sheet = state.wb.sheet(si).ok_or("no spelling sheet")?;
        let Some(current_rows) = spelling_inputs(sheet, true) else { return Ok(None) };
        let Some(current_text_rows) = spelling_inputs(sheet, false) else { return Ok(None) };
        let numeric_now: BTreeSet<_> = sheet
            .cells
            .iter()
            .filter(|(_, cell)| cell.formula.is_none() && matches!(cell.value, kition_sheet_engine::core::Value::Number(_)))
            .map(|(at, _)| at)
            .collect();
        let Some(kdm) = state.kdm() else { return Ok(None) };
        let mut positions = BTreeMap::new();
        for (owner, axes) in kdm.model().ids().sheets.iter().enumerate() {
            for (row, raw) in axes.rows.iter().enumerate() {
                if positions.len() >= 100_000 {
                    return Ok(None);
                }
                positions.insert(format!("row:{raw}"), (owner, row as u32));
            }
        }
        let mut before: BTreeMap<CellRef, String> =
            current_rows.iter().flat_map(|(row, inputs)| inputs.iter().map(move |(col, input)| (CellRef::new(*row, *col), input.clone()))).collect();
        let hits = |source: &BTreeMap<CellRef, String>| -> BTreeSet<CellRef> {
            source.iter().filter(|(_, text)| kition_sheet_engine::cmd::edit::matches(text, word, true, false)).map(|(at, _)| *at).collect()
        };
        let now = hits(&before);
        let mut unknown = BTreeSet::new();
        let mut cleared = BTreeSet::new();
        let mut created_rows = BTreeSet::new();
        let mut restored = Vec::new();
        let mut old_positions = BTreeMap::new();
        for ask in objects {
            let Some(id) = ask.id.as_str().filter(|id| id.starts_with("row:")) else { continue };
            let Some((owner, current)) = positions.get(id).copied() else {
                // A deleted row has no current owner; the pinned journal may not
                // retain enough ownership or input history to reconstruct it.
                unknown.insert(id.to_owned());
                continue;
            };
            if owner != si {
                continue;
            }
            cleared.insert(current);
            if ask.created {
                created_rows.insert(current);
                continue;
            }
            let old = ask.before.get("index").and_then(Value::as_u64).and_then(|row| u32::try_from(row).ok()).unwrap_or(current);
            old_positions.insert(id.to_owned(), old);
            if ask.deleted || old != current || ask.unknown.iter().any(|field| matches!(field.as_str(), "index" | "~spellingInputs" | "$was")) {
                unknown.insert(id.to_owned());
                continue;
            }
            let inputs = match ask.before.get("~spellingInputs") {
                Some(value) => match serde_json::from_value::<Vec<(u32, String)>>(value.clone()) {
                    Ok(inputs) if inputs.len() <= 100_000 && inputs.iter().all(|(col, _)| *col < kition_sheet_engine::core::MAX_COLS) => inputs,
                    _ => {
                        unknown.insert(id.to_owned());
                        continue;
                    }
                },
                // A position-only change leaves this row's input vector intact.
                None => current_text_rows.get(&current).cloned().unwrap_or_default(),
            };
            if old >= kition_sheet_engine::core::MAX_ROWS {
                unknown.insert(id.to_owned());
                continue;
            }
            restored.push((old, inputs));
        }
        // Row snapshots cover nonnumeric inputs. Keep current numeric literals
        // until their existing Cell.input history below supplies the old value.
        before.retain(|at, _| !created_rows.contains(&at.row) && (!cleared.contains(&at.row) || numeric_now.contains(at)));
        let mut historical_text = BTreeSet::new();
        for (row, inputs) in restored {
            for (col, input) in inputs {
                let at = CellRef::new(row, col);
                historical_text.insert(at);
                before.insert(at, input);
            }
        }
        let prefix = format!("cell:{}!", sheet.sheet_id);
        for ask in objects {
            let Some(id) = ask.id.as_str() else { continue };
            let Some(at) = id.strip_prefix(&prefix).and_then(CellRef::parse) else { continue };
            if ask.created {
                before.remove(&at);
                continue;
            }
            if ask.before.get("address").and_then(Value::as_str).is_some_and(|old| Some(old) != id.strip_prefix("cell:")) {
                // Moving positional Cell tool IDs cannot establish a numeric
                // source's old coordinate without a complete historical row.
                unknown.insert(id.to_owned());
                continue;
            }
            if let Some(number) = ask.before.get("input").and_then(Value::as_str).and_then(|input| input.parse::<f64>().ok()) {
                before.insert(at, kition_sheet_engine::core::Value::Number(number).display());
            } else if (ask.deleted || ask.unknown.iter().any(|field| matches!(field.as_str(), "input" | "$was"))) && !historical_text.contains(&at) {
                // Deleted numeric cells may have no retained input register.
                // Preserve the refusal instead of inventing their old value.
                unknown.insert(id.to_owned());
            }
        }
        let then = hits(&before);
        let under = then.iter().map(|at| json!(format!("{prefix}{at}"))).collect();
        let covered = objects
            .iter()
            .map(|ask| {
                let id = ask.id.as_str().unwrap_or_default();
                let covered = if unknown.contains(id) {
                    None
                } else if let Some((owner, current)) = positions.get(id) {
                    Some(
                        *owner == si
                            && (now.iter().any(|at| at.row == *current)
                                || old_positions.get(id).is_some_and(|old| then.iter().any(|at| at.row == *old))),
                    )
                } else if let Some(at) = id.strip_prefix(&prefix).and_then(CellRef::parse) {
                    Some(then.contains(&at) || now.contains(&at))
                } else {
                    Some(false)
                };
                Covered { id: ask.id.clone(), covered, ..Default::default() }
            })
            .collect();
        Ok(Some(CoveredAt { under, covered }))
    }

    pub(crate) fn spelling_replacements(&self, params: &Value, si: usize) -> Result<Option<Vec<(CellRef, String)>>, String> {
        let (Some(word), Some(to)) = (params.get("word").and_then(Value::as_str), params.get("to").and_then(Value::as_str)) else {
            return Ok(None);
        };
        let state = self.doc_state()?;
        let sheet = state.wb.sheet(si).ok_or("no spelling sheet")?;
        if params.get("all").and_then(Value::as_bool) == Some(true) {
            if word.is_empty() {
                return Ok(Some(Vec::new()));
            }
            let source: Vec<_> = sheet.cells.iter().take(100_001).map(|(at, _)| (at, sheet.input_text(at))).collect();
            if source.len() > 100_000 {
                return Ok(None);
            }
            Ok(Some(
                source
                    .into_iter()
                    .filter(|(_, text)| kition_sheet_engine::cmd::edit::matches(text, word, true, false))
                    .map(|(at, text)| (at, text.replace(word, to)))
                    .collect(),
            ))
        } else {
            let Some(at) = params.get("cell").and_then(Value::as_str).and_then(|cell| cell.rsplit('!').next()).and_then(CellRef::parse) else {
                return Ok(None);
            };
            let text = sheet.cell(at).map(|cell| cell.input_text()).unwrap_or_default();
            Ok(Some(vec![(at, text.replacen(word, to, 1))]))
        }
    }

    pub(crate) fn view_read_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        let input = names.is_empty() || names.iter().any(|name| name == "spellingInputs");
        let header = names.is_empty() || names.iter().any(|name| name == "spellingHeader");
        if !input && !header {
            return Ok(());
        }
        let state = self.doc_state()?;
        let Some(kdm) = state.kdm() else { return Ok(()) };
        let mut indices: BTreeMap<_, _> = found.iter().enumerate().map(|(index, previous)| (previous.id.to_string(), index)).collect();
        let requested: BTreeSet<_> = requested.iter().filter_map(Value::as_str).collect();
        let empty_header = json!(kition_kit::session::sha256_hex(b"[]"));
        for (si, sheet) in state.wb.sheets.iter().enumerate() {
            let Some(inputs) = spelling_inputs(sheet, false) else { continue };
            let headers = if header { spelling_headers(sheet)? } else { None };
            let Some(axes) = kdm.model().ids().sheets.get(si) else { continue };
            if axes.rows.len() > 100_000 {
                continue;
            }
            for (row, raw) in axes.rows.iter().enumerate() {
                let id = format!("row:{raw}");
                if !requested.is_empty() && !requested.contains(id.as_str()) {
                    continue;
                }
                let id = json!(id);
                let mut values = serde_json::Map::new();
                if input {
                    values.insert("spellingInputs".into(), json!(inputs.get(&(row as u32)).cloned().unwrap_or_default()));
                }
                if let Some(headers) = &headers {
                    values.insert("spellingHeader".into(), headers.get(&(row as u32)).cloned().unwrap_or_else(|| empty_header.clone()));
                }
                if values.is_empty() {
                    continue;
                }
                if let Some(previous) = indices.get(&id.to_string()).and_then(|index| found.get_mut(*index)) {
                    previous.values.extend(values);
                } else {
                    indices.insert(id.to_string(), found.len());
                    found.push(DerivedValues { id, values });
                }
            }
        }
        Ok(())
    }

    fn spelling_entry_premises(&self, si: usize, at: CellRef, input: &str) -> Result<Option<Under>, String> {
        let entry = json!({"sheet":si,"cell":at.a1(),"input":input});
        let Some(mut under) = self.call_premises("cell.set", &entry)? else { return Ok(None) };
        // The delegated handler enters the resolved sheet. Its synthetic index
        // is not a tab-position dependency of the spelling call.
        if let Some(index) = under.within.iter().position(|window| {
            window.parent == json!("workbook") && window.field == "sheets" && window.from == si && window.to == si.saturating_add(1)
        }) {
            under.within.remove(index);
        }
        Ok(Some(under))
    }

    fn view_address(&self, under: &mut Under, si: usize, rows: Option<(u32, u32)>, cols: Option<(u32, u32)>) -> Option<()> {
        let state = self.doc_state().ok()?;
        let sheet = state.wb.sheet(si)?;
        let sid = crate::ids::sheet_tool_id(sheet);
        let axes = state.kdm()?.model().ids().sheets.get(si)?;
        for (field, prefix, span, ids) in [("rows", "row", rows, &axes.rows), ("cols", "col", cols, &axes.cols)] {
            let Some((first, last)) = span else { continue };
            if last.saturating_sub(first) >= 100_000 || ids.get(last as usize).is_none() {
                return None;
            }
            window(under, &sid, field, first as usize, last as usize + 1);
            for raw in ids.iter().skip(first as usize).take((last - first) as usize + 1) {
                under.premises.push(premise(format!("{prefix}:{raw}"), Vec::<String>::new()).positions(["index"]));
            }
        }
        Some(())
    }

    pub(crate) fn view_read_premises(&self, command: &str, params: &Value, resolved: usize) -> Result<Option<Under>, String> {
        if matches!(command, "file.new" | "file.open") || command == "file.close" && params.get("force").and_then(Value::as_bool) == Some(true) {
            return Ok(Some(Under::default()));
        }
        let state = self.doc_state()?;
        let si = if command.starts_with("view.") { state.wb.active_sheet } else { resolved };
        let sheet = state.wb.sheet(si).ok_or("no view sheet")?;
        let sid = crate::ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        let mut fields: Vec<String> = Vec::new();
        let mut spans = Vec::new();
        let point = |key: &str| params.get(key).and_then(Value::as_str).and_then(|text| text.rsplit('!').next()).and_then(CellRef::parse);
        let rectangle = |r: RangeRef| (Some((r.start.row, r.end.row)), Some((r.start.col, r.end.col)));
        match command {
            "spelling.change" if params.get("all").and_then(Value::as_bool) == Some(true) => {
                if params.get("word").and_then(Value::as_str) == Some("") {
                    return Ok(Some(under));
                }
                let Some(replacements) = self.spelling_replacements(params, si)? else { return Ok(None) };
                if spelling_headers(sheet)?.is_none() {
                    return Ok(None);
                }
                under.covers = Some(Covers { fields: vec!["input".into(), "~spellingInputs".into()] });
                for (at, replacement) in replacements {
                    let cell_id = format!("cell:{}!{at}", sheet.sheet_id);
                    under.objects.push(json!(cell_id));
                    let Some(row) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)).and_then(|axes| axes.rows.get(at.row as usize))
                    else {
                        return Ok(None);
                    };
                    under.premises.push(premise(format!("row:{row}"), ["~spellingHeader"]));
                    let mut preview = state.wb.as_ref().clone();
                    // Replace saves malformed formulas as plain default-style
                    // text. Entry's parser must not reject this handler path.
                    let input = if kition_sheet_engine::cmd::edit::input_to_cell(&replacement, si, at, &mut preview).is_err() {
                        format!("'{replacement}")
                    } else {
                        replacement
                    };
                    let Some(mut entry) = self.spelling_entry_premises(si, at, &input)? else { return Ok(None) };
                    for p in entry.premises.iter_mut().filter(|p| p.id == sid) {
                        if let Some(fields) = &mut p.fields {
                            fields.retain(|field| {
                                field != "constraints" && !field.starts_with("constraints.") && !matches!(field.as_str(), "merges" | "validations")
                            });
                        }
                    }
                    under.premises.extend(entry.premises);
                    under.within.extend(entry.within);
                    if under.premises.len() > 100_000 {
                        return Ok(None);
                    }
                }
                if params.get("cell").and_then(Value::as_str).is_some_and(|cell| cell.contains('!')) {
                    under.premises.push(premise(sid, ["name"]));
                }
                merged(&mut under);
                return Ok(Some(under));
            }
            "spelling.change" if params.get("all").and_then(Value::as_bool) != Some(true) => {
                let (Some(at), Some(word), Some(to)) =
                    (point("cell"), params.get("word").and_then(Value::as_str), params.get("to").and_then(Value::as_str))
                else {
                    return Ok(None);
                };
                let text = sheet.cell(at).map(|cell| cell.input_text()).unwrap_or_default();
                let Some(mut under) = self.spelling_entry_premises(si, at, &text.replacen(word, to, 1))? else { return Ok(None) };
                if params.get("cell").and_then(Value::as_str).is_some_and(|cell| cell.contains('!')) {
                    under.premises.push(premise(sid, ["name"]));
                }
                merged(&mut under);
                return Ok(Some(under));
            }
            "theme.set" => {
                under.objects.push(json!("workbook"));
                under.premises.push(premise("workbook", ["theme"]));
                return Ok(Some(under));
            }
            "view.freezePanes" | "view.freezeRows" | "view.freezeColumns" | "view.freezeTopRow" | "view.freezeFirstColumn" | "view.unfreezePanes" => {
                // same-field decision: even explicit top replaces
                // the observed frozen origin; clearing panes removes that view.
                fields.extend(["props.freeze", "~freezeOrigin"].map(str::to_owned));
                let unfreeze =
                    command == "view.unfreezePanes" || command == "view.freezePanes" && sheet.freeze.is_some() && params.get("cell").is_none();
                if !unfreeze {
                    let top = if params.get("top").is_some() { point("top") } else { self.view_top_cell() }.unwrap_or_default();
                    spans.push(rectangle(RangeRef::cell(top)));
                    if matches!(command, "view.freezePanes" | "view.freezeRows" | "view.freezeColumns") {
                        let at = point("cell").unwrap_or(state.selection.active);
                        spans.push((
                            (command != "view.freezeColumns").then_some((at.row, at.row)),
                            (command != "view.freezeRows").then_some((at.col, at.col)),
                        ));
                    }
                    if params.get("top").is_none() && self.view_at != (0.0, 0.0) && sheet.freeze.is_none() {
                        fields.extend(["props.defaultRowHeight", "props.defaultColWidth"].map(str::to_owned));
                        if let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) {
                            for (prefix, ids, end) in [("row", &axes.rows, top.row), ("col", &axes.cols, top.col)] {
                                for raw in ids.iter().take(end as usize + 1) {
                                    under.premises.push(premise(format!("{prefix}:{raw}"), ["info.size", "info.hidden"]));
                                }
                            }
                        }
                    }
                }
            }
            "page.printArea" => {
                fields.push("props.print.print_area".into());
                if params.get("clear").and_then(Value::as_bool) != Some(true) {
                    let r = params
                        .get("range")
                        .or_else(|| params.get("cell"))
                        .and_then(Value::as_str)
                        .and_then(|text| text.rsplit('!').next())
                        .and_then(|body| body.split(',').next())
                        .and_then(RangeRef::parse)
                        .unwrap_or(state.selection.current());
                    spans.push(rectangle(r));
                }
            }
            "page.printTitles" => {
                fields.extend(["props.print.title_rows", "props.print.title_cols"].map(str::to_owned));
                if let Some(r) = params.get("rows").and_then(Value::as_str).and_then(RangeRef::parse) {
                    spans.push((Some((r.start.row, r.end.row)), None));
                }
                if let Some(r) = params.get("cols").and_then(Value::as_str).and_then(RangeRef::parse) {
                    spans.push((None, Some((r.start.col, r.end.col))));
                }
            }
            "page.breaks" => {
                fields.extend(["props.print.row_breaks", "props.print.col_breaks"].map(str::to_owned));
                if params.get("reset").and_then(Value::as_bool) != Some(true) {
                    spans.push(rectangle(RangeRef::cell(point("cell").unwrap_or(state.selection.active))));
                }
            }
            "page.pageSetup" => {
                for (parameter, field) in [
                    ("orientation", "orientation"),
                    ("paper", "paper"),
                    ("scale", "scale"),
                    ("centerH", "center_h"),
                    ("centerV", "center_v"),
                    ("gridlines", "gridlines"),
                    ("headings", "headings"),
                    ("header", "header"),
                    ("footer", "footer"),
                    ("printArea", "print_area"),
                    ("titleRows", "title_rows"),
                    ("titleCols", "title_cols"),
                    ("rowBreaks", "row_breaks"),
                    ("colBreaks", "col_breaks"),
                ] {
                    if params.get(parameter).is_some() {
                        fields.push(format!("props.print.{field}"));
                    }
                }
                if ["scale", "fitWidth", "fitHeight", "fitToPage"].iter().any(|key| params.get(*key).is_some()) {
                    fields.push("props.print.fit_to".into());
                }
                if params.get("margins").is_some()
                    || ["left", "right", "top", "bottom", "header", "footer"].iter().any(|key| params.get(*key).is_some_and(Value::is_number))
                {
                    fields.push("props.print.margins".into());
                }
                for key in ["printArea", "titleRows", "titleCols"] {
                    if let Some(r) = params.get(key).and_then(Value::as_str).and_then(area) {
                        spans.push((
                            (key != "titleCols").then_some((r.start.row, r.end.row)),
                            (key != "titleRows").then_some((r.start.col, r.end.col)),
                        ));
                    }
                }
                for value in params.get("rowBreaks").and_then(Value::as_array).into_iter().flatten() {
                    if let Some(row) = value
                        .as_u64()
                        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
                        .and_then(|n| u32::try_from(n).ok())
                        .filter(|n| *n >= 2 && *n <= kition_sheet_engine::core::MAX_ROWS)
                    {
                        spans.push((Some((row - 1, row - 1)), None));
                    }
                }
                for value in params.get("colBreaks").and_then(Value::as_array).into_iter().flatten() {
                    let col =
                        value.as_str().and_then(|s| kition_sheet_engine::core::letters_to_col(s.trim().trim_start_matches('$'))).or_else(|| {
                            value
                                .as_u64()
                                .and_then(|n| u32::try_from(n).ok())
                                .filter(|n| *n >= 2 && *n <= kition_sheet_engine::core::MAX_COLS)
                                .map(|n| n - 1)
                        });
                    if let Some(col) = col.filter(|n| *n >= 1) {
                        spans.push((None, Some((col, col))));
                    }
                }
            }
            _ => return Ok(None),
        }
        if !command.starts_with("view.") {
            if params.get("sheet").is_some_and(Value::is_string)
                || ["range", "cell", "rows", "cols"].iter().any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|s| s.contains('!')))
            {
                fields.push("name".into());
            }
            if let Some(index) = params.get("sheet").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()) {
                window(&mut under, "workbook", "sheets", index, index.saturating_add(1));
            }
        }
        for (rows, cols) in spans {
            if self.view_address(&mut under, si, rows, cols).is_none() {
                return Ok(None);
            }
        }
        under.objects.push(json!(sid));
        under.premises.push(premise(sid, fields));
        merged(&mut under);
        Ok(Some(under))
    }
}
