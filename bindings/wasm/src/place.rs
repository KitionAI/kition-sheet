//! Premises use the same normalized ownership paths as receipts. A1 addresses hold two
//! positional windows, including empty destinations; a property write holds only its fields.
use std::collections::{BTreeMap, BTreeSet};

use kition_kit::{Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, Premise, Under, Within};
use kition_office_core::ink::InkStroke;
use kition_sheet_engine::core::RangeRef;
use kition_sheet_engine::formula::{Expr, ast::SheetSel};
use kition_sheet_engine::{
    cmd::ink,
    model::{ShapeKind, Sheet},
};
use serde_json::{Value, json};

use crate::{SheetDocument, ids};

// Evaluating a Struct node resolves its table/column area without dereferencing cells.
struct TableReferenceHost<'a>(&'a kition_sheet_engine::model::Workbook);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SourceFacts {
    visibility: bool,
    width: bool,
    sheet_name: bool,
    format: bool,
}

impl Default for SourceFacts {
    fn default() -> Self {
        Self { visibility: false, width: false, sheet_name: false, format: true }
    }
}

impl kition_sheet_engine::calc::eval::Host for TableReferenceHost<'_> {
    fn workbook(&self) -> &kition_sheet_engine::model::Workbook {
        self.0
    }
    fn cell_value(&mut self, _: usize, _: kition_sheet_engine::core::CellRef) -> kition_sheet_engine::core::Value {
        kition_sheet_engine::core::Value::Empty
    }
    fn now_serial(&self) -> f64 {
        0.0
    }
    fn random(&mut self) -> f64 {
        0.0
    }
}

fn source_expr(range: RangeRef) -> Expr {
    use kition_sheet_engine::formula::{Anchor, RefKind, Reference};
    let anchor = |at: kition_sheet_engine::core::CellRef| Anchor { row: at.row, col: at.col, row_abs: true, col_abs: true };
    Expr::Ref(Reference { sheet: SheetSel::Current, kind: RefKind::Range(anchor(range.start), anchor(range.end)) })
}

fn ink_values(sheet: &Sheet) -> Vec<Value> {
    sheet
        .shapes
        .iter()
        .filter(|shape| shape.kind == ShapeKind::Ink)
        .map(|shape| json!({"id":format!("drawing:{}",shape.id),"stroke":ink::stroke_of(sheet,shape)}))
        .collect()
}

fn ink_hits(params: &Value, values: &[Value]) -> Result<Vec<Value>, String> {
    let gesture = kition_office_core::ink::parse_eraser(params, ink::ERASER_RADIUS).map_err(|e| e.to_string())?;
    let strokes: Vec<InkStroke> =
        values.iter().map(|v| serde_json::from_value(v["stroke"].clone()).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    Ok(gesture.hits(&strokes).into_iter().filter_map(|n| values.get(n).map(|v| v["id"].clone())).collect())
}

pub(super) fn premise(id: impl Into<Value>, fields: impl IntoIterator<Item = impl Into<String>>) -> Premise {
    Premise::on(id).fields(fields).positions(Vec::<String>::new())
}

pub(super) fn window(under: &mut Under, parent: &str, field: &str, from: usize, to: usize) {
    under.within.push(Within { parent: json!(parent), field: field.into(), from, to, time: None });
}

pub(super) fn area(text: &str) -> Option<RangeRef> {
    let body = text.rsplit_once('!').map_or(text, |(_, body)| body);
    RangeRef::parse(body).or_else(|| RangeRef::parse(&format!("{body}:{body}")))
}

fn number(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|n| u32::try_from(n).ok()).or_else(|| value.as_str()?.strip_prefix("drawing:").unwrap_or(value.as_str()?).parse().ok())
}

fn transform_extent(wb: &kition_sheet_engine::model::Workbook, si: usize, ids: &[u32], params: &Value) -> Option<(u32, u32)> {
    use kition_office_core::ops::DrawingOp;
    use kition_sheet_engine::cmd::{chart_spec, drawing};
    let sheet = wb.sheet(si)?;
    if sheet.shapes.iter().any(|shape| shape.connection.is_some()) {
        return None;
    }
    let op = DrawingOp::parse("drawing.transform", params).ok()?;
    let mut extent = (0, 0);
    for id in ids {
        let members: Vec<_> = sheet.shapes.iter().filter(|shape| shape.groups.contains(id)).collect();
        let before = if let Some(shape) = sheet.shapes.iter().find(|shape| shape.id == *id) {
            drawing::to_drawing(sheet, shape)
        } else if let Some(chart) = sheet.charts.iter().find(|chart| chart.id == *id) {
            chart_spec::to_drawing(wb, si, sheet, chart)
        } else {
            drawing::group_object(sheet, *id)?
        };
        let mut after = before.clone();
        op.apply(&mut after).ok()?;
        let mut points = vec![(before.frame.x, before.frame.y), (after.frame.x, after.frame.y)];
        let (b, a) = (before.frame, after.frame);
        for member in members {
            let frame = drawing::to_drawing(sheet, member).frame;
            points.extend([(frame.x, frame.y), (a.x + (frame.x - b.x) * a.w / b.w.max(1e-6), a.y + (frame.y - b.y) * a.h / b.h.max(1e-6))]);
        }
        for (x, y) in points {
            extent.0 = extent.0.max(sheet.row_at(y.max(0.0)));
            extent.1 = extent.1.max(sheet.col_at(x.max(0.0)));
        }
    }
    Some(extent)
}

fn alignment_extent(sheet: &Sheet, ids: &[u32], command: &str, params: &Value) -> Option<(u32, u32)> {
    if sheet.shapes.iter().any(|shape| shape.connection.is_some()) {
        return None;
    }
    let anchors: Vec<_> = match params.get("kind").and_then(Value::as_str).unwrap_or("shape") {
        "chart" => sheet.charts.iter().map(|object| (object.id, &object.anchor)).collect(),
        "image" => sheet.images.iter().map(|object| (object.id, &object.anchor)).collect(),
        _ => sheet.shapes.iter().map(|object| (object.id, &object.anchor)).collect(),
    };
    let geometry: Vec<_> = anchors
        .into_iter()
        .filter(|(id, _)| ids.contains(id))
        .map(|(_, anchor)| (anchor, sheet.col_left(anchor.cell.col) + f64::from(anchor.dx), sheet.row_top(anchor.cell.row) + f64::from(anchor.dy)))
        .collect();
    let left = geometry.iter().map(|(_, x, _)| *x).fold(f64::INFINITY, f64::min);
    let right = geometry.iter().map(|(anchor, x, _)| x + f64::from(anchor.width)).fold(f64::NEG_INFINITY, f64::max);
    let top = geometry.iter().map(|(_, _, y)| *y).fold(f64::INFINITY, f64::min);
    let bottom = geometry.iter().map(|(anchor, _, y)| y + f64::from(anchor.height)).fold(f64::NEG_INFINITY, f64::max);
    let how = if command == "arrange.group" { "left" } else { params.get("align").and_then(Value::as_str).unwrap_or("left") };
    let mut extent = (0, 0);
    for (anchor, mut x, mut y) in geometry {
        // The handler converts only the result's top-left coordinate back to an anchor.
        match how {
            "left" => x = left,
            "right" => x = right - f64::from(anchor.width),
            "center" => x = (left + right) / 2.0 - f64::from(anchor.width) / 2.0,
            "top" => y = top,
            "bottom" => y = bottom - f64::from(anchor.height),
            _ => y = (top + bottom) / 2.0 - f64::from(anchor.height) / 2.0,
        }
        extent.0 = extent.0.max(anchor.cell.row).max(sheet.row_at(y));
        extent.1 = extent.1.max(anchor.cell.col).max(sheet.col_at(x));
    }
    Some(extent)
}

fn property_fields(command: &str, params: &Value) -> Option<Vec<String>> {
    let fields: &[&str] = match command {
        "review.protectSheet" | "review.unprotectSheet" => &["props.protection"],
        "view.gridlines" => &["props.showGridlines"],
        "view.headings" => &["props.showHeadings"],
        "view.showZeros" => &["props.showZeros"],
        "view.rightToLeft" => &["props.rightToLeft"],
        "view.gridlineColor" => &["props.gridColor"],
        "home.defaultWidth" => &["props.defaultColWidth"],
        "pageLayout.orientation" => &["props.print.orientation"],
        "pageLayout.size" => &["props.print.paper"],
        "pageLayout.margins" => &["props.print.margins"],
        "pageLayout.scaleToFit" => {
            let scale = params.get("scale").and_then(Value::as_f64).is_some();
            let mut fields = Vec::new();
            if scale {
                fields.push("props.print.scale".to_owned());
            }
            if scale || params.get("width").is_some() || params.get("height").is_some() {
                fields.push("props.print.fit_to".to_owned());
            }
            return Some(fields);
        }
        "pageLayout.printGridlines" => &["props.print.gridlines"],
        "pageLayout.printHeadings" => &["props.print.headings"],
        "pageLayout.headerFooter" => {
            return Some(
                ["header", "footer"].into_iter().filter(|field| params.get(*field).is_some()).map(|field| format!("props.print.{field}")).collect(),
            );
        }
        _ => return None,
    };
    Some(fields.iter().map(|field| (*field).to_owned()).collect())
}

/// The fields of a `Comment` a premise on all of it holds.
const COMMENT_FIELDS: [&str; 6] = ["author", "text", "replies", "threaded", "resolved", "visible"];

/// The cell a `comments` key in tool ids (`row:<id>:col:<id>`) names on sheet `si`.
fn comment_place(objects: Option<&kdm_model::Ids>, si: usize, key: &str) -> Option<kition_sheet_engine::core::CellRef> {
    let (row, col) = key.strip_prefix("row:")?.split_once(":col:")?;
    let sheet = objects?.sheets.get(si)?;
    let r = sheet.rows.iter().position(|o| o.as_str() == row)?;
    let c = sheet.cols.iter().position(|o| o.as_str() == col)?;
    Some(kition_sheet_engine::core::CellRef::new(u32::try_from(r).ok()?, u32::try_from(c).ok()?))
}

/// The fields of a `Table` or `Pivot` object a premise on all of it holds.
const FEATURE_OBJECT_FIELDS: [&str; 5] = ["key", "name", "range", "props", "columns"];
const PIVOT_OBJECT_FIELDS: [&str; 6] = ["key", "name", "anchor", "source", "lastRange", "props"];

/// What a premise on a sheet's `tables` or `pivots` field (a list, or an item named by
/// `[name=…]` and maybe a path inside it) holds of the `Table` and `Pivot` objects: they have tool
/// ids (`table:<key>`, `pivot:<key>`), so others' changes of them are told under those ids, not
/// under the sheet's list. Each such premise gets one on the objects it names: every table (or
/// pivot table) of the sheet for the whole list, the named one for an item, with the path inside.
pub(super) fn feature_object_premises(wb: &kition_sheet_engine::model::Workbook, objects: Option<&kdm_model::Ids>, under: &mut Under) {
    let mut more = Vec::new();
    for p in &under.premises {
        let Some((si, sheet)) = p.id.as_str().and_then(|id| wb.sheets.iter().enumerate().find(|(_, s)| ids::sheet_tool_id(s) == id)) else {
            continue;
        };
        for field in p.fields.iter().flatten() {
            // A cell's comment (`comments.row:<id>:col:<id>`) is a `Comment` with its own tool id.
            if let Some(at) = field.strip_prefix("comments.").and_then(|key| comment_place(objects, si, key))
                && sheet.comments.contains_key(&at)
            {
                more.push(premise(format!("comment:{}!{at}", sheet.sheet_id), COMMENT_FIELDS));
            }
            for (list, prefix, all) in [("tables", "table", &FEATURE_OBJECT_FIELDS[..]), ("pivots", "pivot", &PIVOT_OBJECT_FIELDS[..])] {
                let named: Vec<(String, u32)> = if list == "tables" {
                    sheet.tables.iter().map(|t| (t.name.clone(), t.id)).collect()
                } else {
                    sheet.pivots.iter().map(|p| (p.name.clone(), p.id)).collect()
                };
                let tool = |id: u32| format!("{prefix}:{}:{id}", sheet.sheet_id);
                let Some(rest) = field.strip_prefix(list) else { continue };
                if rest.is_empty() || rest.starts_with('.') {
                    for (_, id) in &named {
                        more.push(premise(tool(*id), all.iter().copied()));
                    }
                } else if let Some(selector) = rest.strip_prefix("[name=") {
                    let mut values = serde_json::Deserializer::from_str(selector).into_iter::<Value>();
                    let Some(Ok(Value::String(name))) = values.next() else { continue };
                    let tail = selector.get(values.byte_offset()..).unwrap_or("");
                    let inside = tail.strip_prefix(']').unwrap_or("").strip_prefix('.').unwrap_or("");
                    for (_, id) in named.iter().filter(|(n, _)| *n == name) {
                        if inside.is_empty() {
                            more.push(premise(tool(*id), all.iter().copied()));
                        } else {
                            more.push(premise(tool(*id), [inside.to_owned()]));
                        }
                    }
                }
            }
        }
    }
    if !more.is_empty() {
        under.premises.extend(more);
        merged(under);
    }
}

/// Joins repeated dependencies; no later premise may accidentally erase an earlier one.
pub(super) fn merged(under: &mut Under) {
    let mut all: BTreeMap<String, Premise> = BTreeMap::new();
    for mut p in std::mem::take(&mut under.premises) {
        let key = p.id.to_string();
        if let Some(previous) = all.get_mut(&key) {
            for (before, after) in [(&mut previous.fields, p.fields.take()), (&mut previous.positions, p.positions.take())] {
                match (before.as_mut(), after) {
                    (Some(before), Some(after)) => {
                        before.extend(after);
                        before.sort();
                        before.dedup();
                    }
                    (_, None) => *before = None,
                    _ => {}
                }
            }
        } else {
            all.insert(key, p);
        }
    }
    under.premises = all.into_values().collect();
    under.objects.sort_by_key(Value::to_string);
    under.objects.dedup();
}

impl SheetDocument {
    pub(crate) fn document_premises(&self) -> Result<Under, String> {
        let state = self.doc_state()?;
        let mut under = Under::default();
        if let Some(kdm) = state.kdm() {
            let replica = kdm.replica();
            let names = kition_kit::kdm::tool_ids::ToolIds::of_replica(replica);
            for ty in replica.schema().types.keys() {
                for object in replica.objects_of_type(ty) {
                    if under.premises.len() >= 100_000 {
                        return Err("document dependencies exceed the premise limit".into());
                    }
                    let id = names.show(&object);
                    under.objects.push(json!(id));
                    under.premises.push(Premise::on(id));
                }
            }
        } else {
            under.premises.push(Premise::on("workbook"));
            for sheet in &state.wb.sheets {
                under.premises.push(Premise::on(ids::sheet_tool_id(sheet)));
                for (at, _) in sheet.cells.iter() {
                    under.premises.push(Premise::on(format!("cell:{}!{at}", sheet.sheet_id)));
                }
            }
        }
        Ok(under)
    }

    pub(crate) fn premise_derived(&self, ids: &[Value], names: &[String]) -> Result<Vec<DerivedValues>, String> {
        let mut found = Vec::new();
        if ids.is_empty() || ids.contains(&json!("workbook")) {
            let mut values = serde_json::Map::new();
            if names.is_empty() || names.iter().any(|name| name == "computed") {
                values.insert("computed".into(), json!(self.computed_digest()?));
            }
            if names.is_empty() || names.iter().any(|name| name == "tabs") {
                let tabs: Vec<_> = self.doc_state()?.wb.sheets.iter().map(|sheet| sheet.sheet_id).collect();
                let digest = kition_kit::session::sha256_hex(&serde_json::to_vec(&tabs).map_err(|e| e.to_string())?);
                values.insert("tabs".into(), json!(digest));
            }
            self.table_dependency_values(names, &mut values)?;
            if !values.is_empty() {
                found.push(DerivedValues { id: json!("workbook"), values });
            }
        }
        for sheet in &self.doc_state()?.wb.sheets {
            let id = json!(ids::sheet_tool_id(sheet));
            if !ids.is_empty() && !ids.contains(&id) {
                continue;
            }
            let mut values = serde_json::Map::new();
            if names.is_empty() || names.iter().any(|name| name == "freezeOrigin") {
                values.insert("freezeOrigin".into(), json!(crate::view::origin(sheet)));
            }
            if names.is_empty() || names.iter().any(|name| name == "ink") {
                values.insert("ink".into(), json!(ink_values(sheet)));
            }
            if names.is_empty() || names.iter().any(|name| name == "arrays") {
                let arrays: Vec<_> = sheet.cells.iter().filter_map(|(at, cell)| Some((at, cell.formula.as_ref()?.array?))).collect();
                values.insert("arrays".into(), json!(kition_kit::session::sha256_hex(&serde_json::to_vec(&arrays).map_err(|e| e.to_string())?)));
            }
            if names.is_empty() || names.iter().any(|name| name == "filterScopes") {
                let scopes: Vec<_> =
                    kition_sheet_engine::cmd::data::filter_scopes(sheet).into_iter().map(|scope| (scope.range, scope.table)).collect();
                values
                    .insert("filterScopes".into(), json!(kition_kit::session::sha256_hex(&serde_json::to_vec(&scopes).map_err(|e| e.to_string())?)));
            }
            if names.is_empty() || names.iter().any(|name| name == "hiddenRows" || name == "hiddenCount") {
                let hidden: Vec<_> = sheet.rows.iter().filter(|(_, info)| info.hidden).map(|(row, _)| row).collect();
                values.insert("hiddenCount".into(), json!(hidden.len()));
                values.insert("hiddenRows".into(), json!(kition_kit::session::sha256_hex(&serde_json::to_vec(&hidden).map_err(|e| e.to_string())?)));
            }
            if names.is_empty() || names.iter().any(|name| name == "sortObjects") {
                let anchors: Vec<_> = sheet
                    .images
                    .iter()
                    .map(|object| (object.id, object.anchor))
                    .chain(sheet.charts.iter().map(|object| (object.id, object.anchor)))
                    .chain(sheet.shapes.iter().map(|object| (object.id, object.anchor)))
                    .filter(|(_, anchor)| anchor.mode != kition_sheet_engine::model::AnchorMode::Absolute)
                    .collect();
                values
                    .insert("sortObjects".into(), json!(kition_kit::session::sha256_hex(&serde_json::to_vec(&anchors).map_err(|e| e.to_string())?)));
            }
            if !values.is_empty() {
                found.push(DerivedValues { id, values });
            }
        }
        self.row_layout_values(ids, names, &mut found)?;
        self.edit_read_values(ids, names, &mut found)?;
        self.object_read_values(ids, names, &mut found)?;
        self.view_read_values(ids, names, &mut found)?;
        self.conditional_read_values(ids, names, &mut found)?;
        self.table_lookup_values(ids, names, &mut found)?;
        self.shared_edit_values(ids, names, &mut found)?;
        self.table_formula_values(ids, names, &mut found)?;
        self.axis_values(ids, names, &mut found)?;
        let mut slots = BTreeMap::<String, usize>::new();
        let mut unique: Vec<DerivedValues> = Vec::new();
        for object in found {
            if let Some(previous) = slots.get(&object.id.to_string()).and_then(|slot| unique.get_mut(*slot)) {
                previous.values.extend(object.values);
            } else {
                slots.insert(object.id.to_string(), unique.len());
                unique.push(object);
            }
        }
        Ok(unique)
    }

    pub(crate) fn premise_covered(&self, command: &str, params: &Value, objects: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
        if let Some(covered) = self.table_insert_covered(command, params, objects)? {
            return Ok(Some(covered));
        }
        if let Some(covered) = self.conditional_covered(command, params, objects)? {
            return Ok(Some(covered));
        }
        if let Some(covered) = self.view_covered(command, params, objects)? {
            return Ok(Some(covered));
        }
        if command != "drawing.erase" {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let sheet = state.wb.active().ok_or("no active sheet")?;
        let sid = json!(ids::sheet_tool_id(sheet));
        let now = ink_values(sheet);
        let old = objects.iter().find(|ask| ask.id == sid).and_then(|ask| ask.before.get("~ink")).and_then(Value::as_array).unwrap_or(&now);
        let under = ink_hits(params, old)?;
        let current = ink_hits(params, &now)?;
        let touched = old.iter().chain(&now).any(|value| {
            let id = &value["id"];
            (under.contains(id) || current.contains(id)) && old.iter().find(|v| &v["id"] == id) != now.iter().find(|v| &v["id"] == id)
        });
        let covered = objects
            .iter()
            .map(|ask| Covered {
                id: ask.id.clone(),
                covered: if ask.unknown.iter().any(|field| field == "~ink" || field == "$was") {
                    None
                } else if ask.id == sid {
                    Some(touched)
                } else {
                    Some(under.contains(&ask.id))
                },
                ..Default::default()
            })
            .collect();
        Ok(Some(CoveredAt { under, covered }))
    }

    pub(crate) fn call_premises(&self, command: &str, given: &Value) -> Result<Option<Under>, String> {
        let mut params = given.clone();
        let schema = self.command_schema(command);
        let implied = self.take_tool_ids(command, &mut params, schema.as_ref())?;
        let state = self.doc_state()?;
        let wb = &state.wb;
        let si = self.command_target_sheet(command, &params, implied).unwrap_or(wb.active_sheet);
        let sheet = wb.sheet(si).ok_or("the call names no sheet")?;
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        let mut pending = Vec::new();

        if command == "selection.set" {
            let ranges = if let Some(text) = params.get("range").or_else(|| params.get("cell")).and_then(Value::as_str) {
                text.rsplit_once('!')
                    .map_or(text, |(_, body)| body)
                    .split(',')
                    .map(|part| RangeRef::parse(part).ok_or_else(|| format!("not a range: {part}")))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                state.selection.ranges.clone()
            };
            let mut fields = Vec::new();
            for range in ranges {
                window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
                fields.extend(kdm_model::constraint_fields(range));
            }
            if params.get("sheet").is_some_and(Value::is_string)
                || ["range", "cell"].iter().any(|key| params.get(key).and_then(Value::as_str).is_some_and(|text| text.contains('!')))
            {
                fields.push("name".into());
            }
            under.objects.push(json!(sid));
            under.premises.push(premise(sid, fields));
            return Ok(Some(under));
        }

        if matches!(command, "drawing.connect" | "drawing.group") {
            under.premises.push(premise("workbook", ["~nextObjectId"]));
        }

        if let Some(under) = self.object_read_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.view_read_premises(command, &params, si)? {
            return Ok(Some(under));
        }

        if command == "cell.set" && params.get("name").is_some() {
            return match self.named_cell_premises(&params, si)? {
                Some(under) => Ok(Some(under)),
                None => self.document_premises().map(Some),
            };
        }

        if command == "edit.paste"
            && params.get("html").is_none()
            && self.session.clipboard.is_none()
            && let Some(text) = params.get("text").and_then(Value::as_str)
        {
            let at = params.get("at").and_then(Value::as_str).and_then(area).map(|range| range.start).unwrap_or(state.selection.current().start);
            let values = kition_sheet_engine::cmd::edit::parse_tsv(text);
            let mut entry = json!({"sheet":si,"range":at.a1(),"values":values});
            if let Some(value) = params.get("sheet") {
                entry["sheet"] = value.clone();
            }
            let mut under = self.call_premises("range.setValues", &entry)?.unwrap_or_default();
            let height = values.len();
            if let Some(end) = at.offset(height.saturating_sub(1) as i64, 0) {
                self.entry_row_premises(&mut under, si, RangeRef::new(at, end));
            }
            merged(&mut under);
            return Ok(Some(under));
        }

        let workbook_fields = match command {
            "review.protectWorkbook" => Some(vec!["props.protectedStructure".to_owned()]),
            "file.properties" => Some(
                ["title", "subject", "author", "company", "keywords", "description"]
                    .into_iter()
                    .filter(|key| params.get(*key).is_some())
                    .map(|key| format!("props.docProps.{key}"))
                    .collect(),
            ),
            "formulas.calculationOptions" => {
                let mut fields = vec!["props.calc.mode".to_owned()];
                for (parameter, field) in [("iterative", "iterative"), ("maxIterations", "max_iterations"), ("maxChange", "max_change")] {
                    if params.get(parameter).is_some() {
                        fields.push(format!("props.calc.{field}"));
                    }
                }
                Some(fields)
            }
            _ => None,
        };
        if let Some(fields) = workbook_fields {
            under.objects.push(json!("workbook"));
            under.premises.push(premise("workbook", fields));
            return Ok(Some(under));
        }

        if let Some(under) = self.clear_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.format_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.cell_style_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.outline_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.table_control_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.conditional_read_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.fill_read_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.primitive_read_premises(command, &params, si)? {
            return Ok(Some(under));
        }
        if let Some(under) = self.registry_read_premises(command, &params, si)? {
            return Ok(Some(under));
        }

        if matches!(command, "home.conditionalFormat" | "home.clearRules" | "home.manageRules") {
            // Adding and reordering rules rewrites priorities; clearing rewrites membership.
            let mut fields = vec!["condFormats"];
            if params.get("range").and_then(Value::as_str).is_some_and(|range| range.contains('!'))
                || params.get("sheet").is_some_and(Value::is_string)
            {
                fields.push("name");
            }
            under.objects.push(json!(sid));
            under.premises.push(premise(sid.clone(), fields));
            let mut ranges: Vec<_> = sheet.cond_formats.iter().flat_map(|rule| rule.ranges.iter().copied()).collect();
            if command == "home.conditionalFormat" || command == "home.clearRules" && params.get("sheet").and_then(Value::as_bool) != Some(true) {
                if let Some(text) = params.get("range").and_then(Value::as_str) {
                    let body = text.rsplit_once('!').map_or(text, |(_, body)| body);
                    for part in body.split(',') {
                        let Some(range) = area(part) else { return self.document_premises().map(Some) };
                        ranges.push(range);
                    }
                } else {
                    ranges.extend(state.selection.ranges.iter().copied());
                }
            }
            if ranges.len() > 100_000 {
                return self.document_premises().map(Some);
            }
            let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return self.document_premises().map(Some) };
            let mut lines = std::collections::BTreeSet::new();
            for range in ranges {
                window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
                for (kind, first, last, ids) in
                    [("row", range.start.row, range.end.row, &axes.rows), ("col", range.start.col, range.end.col, &axes.cols)]
                {
                    if last.saturating_sub(first) >= 100_000 || ids.get(last as usize).is_none() {
                        return self.document_premises().map(Some);
                    }
                    for raw in ids.iter().skip(first as usize).take((last - first) as usize + 1) {
                        lines.insert(format!("{kind}:{raw}"));
                        if lines.len() > 100_000 {
                            return self.document_premises().map(Some);
                        }
                    }
                }
            }
            for id in lines {
                under.premises.push(premise(id, Vec::<String>::new()).positions(["index"]));
            }
            merged(&mut under);
            return Ok(Some(under));
        }

        if matches!(
            command,
            "table.bandedRows" | "table.bandedColumns" | "table.firstColumn" | "table.lastColumn" | "table.filterButton" | "table.style"
        ) {
            let target = params.get("table").and_then(Value::as_str).and_then(|name| wb.table(name)).or_else(|| {
                params
                    .get("table")
                    .is_none()
                    .then(|| sheet.tables.iter().position(|table| table.range.contains(state.selection.active)).map(|index| (si, index)))
                    .flatten()
            });
            let Some((owner, table)) =
                target.and_then(|(owner, index)| wb.sheet(owner).and_then(|sheet| sheet.tables.get(index).map(|table| (sheet, table))))
            else {
                return self.document_premises().map(Some);
            };
            let id = ids::sheet_tool_id(owner);
            under.objects.push(json!(id));
            under.premises.push(premise(id, [format!("tables[name={}]", json!(table.name))]));
            return Ok(Some(under));
        }

        if command == "drawing.erase" {
            under.objects = ink_hits(&params, &ink_values(sheet))?;
            for id in &under.objects {
                under.premises.push(premise(id.clone(), Vec::<String>::new()));
            }
            under.premises.push(premise(sid, ["props.protection"]));
            under.covers = Some(Covers { fields: vec!["~ink".into()] });
            return Ok(Some(under));
        }

        let entry = matches!(command, "cell.set" | "range.setValues" | "range.fill" | "edit.clearContents");
        let sort = matches!(command, "data.sort" | "data.sortAscending" | "data.sortDescending") && params.get("range").is_some();
        let filter = matches!(command, "data.filter" | "data.filterBy" | "data.clearFilter" | "data.reapply");
        let validation = command == "data.validation";
        let name_edit = matches!(command, "formulas.defineName" | "formulas.deleteName") && params.get("name").is_some();
        let table_resize = command == "table.resize";
        let lines = matches!(command, "home.insertRows" | "home.insertColumns" | "home.deleteRows" | "home.deleteColumns");
        let sheet_property = matches!(
            command,
            "sheet.rename" | "sheet.tabColor" | "sheet.hide" | "sheet.unhide" | "sheet.move" | "home.insertSheet" | "home.deleteSheet"
        );
        let drawing = !crate::references::drawing_parameters(command).is_empty();
        let properties = property_fields(command, &params);
        let comments = matches!(
            command,
            "review.newNote" | "review.newComment" | "review.replyComment" | "review.resolveComment" | "review.deleteComment" | "review.showNote"
        );
        let fixed_axes = matches!(
            command,
            "home.rowHeight" | "home.columnWidth" | "home.hideRows" | "home.hideColumns" | "home.unhideRows" | "home.unhideColumns"
        );
        if !entry
            && !sort
            && !filter
            && !validation
            && !name_edit
            && !table_resize
            && !lines
            && !sheet_property
            && !drawing
            && properties.is_none()
            && !comments
            && !fixed_axes
        {
            return self.document_premises().map(Some);
        }

        // The numeric `sheet` resolver is a tab position, unlike a context's sheet id.
        if let Some(index) = params.get("sheet").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()) {
            window(&mut under, "workbook", "sheets", index, index.saturating_add(1));
        }
        let named = params.get("sheet").is_some_and(Value::is_string)
            && !given.get("sheet").and_then(Value::as_str).is_some_and(|s| s.starts_with("sheet:"))
            || ["range", "cell", "rows", "cols"].iter().any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|s| s.contains('!')));
        let mut sheet_fields = Vec::<String>::new();
        if command == "chart.move" {
            let destination = params.get("sheet").and_then(Value::as_str).and_then(|name| wb.sheet_index(name)).unwrap_or(si);
            if let Some(target) = wb.sheet(destination) {
                let target_id = ids::sheet_tool_id(target);
                let mut fields = vec!["props.protection"];
                if params.get("sheet").is_some() {
                    fields.push("name");
                }
                under.premises.push(premise(target_id.clone(), fields));
                if let Some(at) = params.get("at").and_then(Value::as_str).and_then(|text| {
                    let body = text.rsplit_once('!').map_or(text, |(_, body)| body);
                    kition_sheet_engine::core::CellRef::parse(body)
                }) {
                    window(&mut under, &target_id, "rows", at.row as usize, at.row as usize + 1);
                    window(&mut under, &target_id, "cols", at.col as usize, at.col as usize + 1);
                }
            }
            under.premises.push(premise(sid.clone(), ["~chartStack"]));
        }
        if matches!(command, "chart.set" | "chart.changeType")
            && let Some(at) =
                params.get("at").and_then(Value::as_str).and_then(|text| text.rsplit('!').next()).and_then(kition_sheet_engine::core::CellRef::parse)
        {
            window(&mut under, &sid, "rows", at.row as usize, at.row as usize + 1);
            window(&mut under, &sid, "cols", at.col as usize, at.col as usize + 1);
        }
        if let Some(fields) = &properties {
            sheet_fields.extend(fields.iter().cloned());
            under.objects.push(json!(sid));
        }
        if named {
            sheet_fields.push("name".into());
        }

        if filter {
            use kition_sheet_engine::cmd::data::{filter_scopes, named_scope};
            let scopes = filter_scopes(sheet);
            let selected = named_scope(&self.session, &params).map_err(crate::engine_error)?;
            sheet_fields.extend(["props.protection", "autofilter", "~filterScopes"].map(str::to_owned));
            let mut applied = Vec::new();
            if command == "data.filter" {
                let active = state.selection.active;
                sheet_fields.extend(kdm_model::constraint_fields(RangeRef::cell(active)));
                if let Some(table) = sheet.table_at(active) {
                    sheet_fields.push(format!("tables[name={}]", json!(table.name)));
                } else if sheet.autofilter.is_some() {
                    sheet_fields.push("~hiddenRows".into());
                } else if let Some(range) = params.get("range").and_then(Value::as_str).and_then(area) {
                    if range.count() > 100_000 {
                        return self.document_premises().map(Some);
                    }
                    window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                    window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
                } else {
                    return self.document_premises().map(Some);
                }
            } else {
                let table = selected.as_ref().and_then(|scope| scope.table.as_ref());
                let selected_table = table.is_some()
                    && (command == "data.filterBy" || params.get("table").is_some() || params.get("cell").is_some() || sheet.autofilter.is_none());
                if command == "data.filterBy" {
                    sheet_fields.push("~hiddenCount".into());
                    if selected.is_none() {
                        return self.document_premises().map(Some);
                    }
                }
                for scope in scopes {
                    if selected_table && command != "data.reapply" && scope.table.as_ref() != table {
                        continue;
                    }
                    if !selected_table && scope.table.is_some() && scope.criteria.is_empty() {
                        continue;
                    }
                    if let Some(name) = &scope.table {
                        sheet_fields.push(format!("tables[name={}]", json!(name)));
                    }
                    // Sheet filters also reapply nonempty table criteria. Clearing the
                    // selected scope writes visibility but does not read its entries.
                    let cleared = command == "data.clearFilter" && (selected_table || scope.table.is_none());
                    let read = !cleared && (scope.table.is_none() || !scope.criteria.is_empty() || command == "data.filterBy");
                    if scope.range.count() > 100_000 {
                        return self.document_premises().map(Some);
                    }
                    if read {
                        pending.push((si, scope.range.start, source_expr(scope.range), SourceFacts::default()));
                    }
                    applied.push(scope.range);
                }
                for range in applied {
                    window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                    window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
                    if let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) {
                        for raw in axes.rows.iter().skip(range.start.row as usize + 1).take(range.height().saturating_sub(1) as usize) {
                            under.premises.push(premise(format!("row:{raw}"), ["info.hidden"]).positions(["index"]));
                        }
                    }
                }
            }
            under.objects.push(json!(sid));
        }

        if name_edit {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let scope = match params.get("scope").and_then(Value::as_str) {
                Some("Workbook") => Some(None),
                Some(name) => Some(wb.sheet_index(name)),
                None if command == "formulas.defineName" => Some(None),
                _ => None,
            };
            let mut matched = false;
            for (index, definition) in wb
                .names
                .iter()
                .enumerate()
                .filter(|(_, definition)| definition.name.eq_ignore_ascii_case(name) && scope.is_none_or(|scope| scope == definition.scope))
            {
                matched = true;
                under.premises.push(premise("workbook", [format!("names[name={}]", json!(definition.name))]));
                if let Some(raw) = state.kdm().and_then(|kdm| kdm.model().ids().names.get(index)) {
                    let id = format!("name:{raw}");
                    under.objects.push(json!(id));
                    under.premises.push(premise(id, ["name", "scope", "formula", "props"]));
                }
            }
            if !matched {
                under.premises.push(premise("workbook", ["names"]));
            }
            if command == "formulas.defineName" && params.get("refersTo").and_then(Value::as_str).is_none() {
                sheet_fields.push("name".into());
                let range = state.selection.current();
                window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
            }
            if let Some(Some(scope)) = scope
                && let Some(sheet) = wb.sheet(scope)
            {
                under.premises.push(premise(ids::sheet_tool_id(sheet), ["name"]));
            }
        }

        if validation || table_resize {
            let ranges = params
                .get("range")
                .and_then(Value::as_str)
                .map(|text| text.split(',').filter_map(area).collect::<Vec<_>>())
                .unwrap_or_else(|| state.selection.ranges.clone());
            if ranges.iter().try_fold(0u64, |n, range| n.checked_add(range.count())).is_none_or(|n| n > 100_000) {
                return self.document_premises().map(Some);
            }
            for range in &ranges {
                sheet_fields.extend(kdm_model::constraint_fields(*range));
                window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
                if table_resize {
                    pending.push((
                        si,
                        range.start,
                        source_expr(RangeRef::new(range.start, kition_sheet_engine::core::CellRef::new(range.start.row, range.end.col))),
                        SourceFacts::default(),
                    ));
                }
            }
            if table_resize {
                let table = params
                    .get("table")
                    .and_then(Value::as_str)
                    .and_then(|name| sheet.tables.iter().find(|table| table.name.eq_ignore_ascii_case(name)))
                    .or_else(|| params.get("table").is_none().then(|| sheet.table_at(state.selection.active)).flatten());
                let Some(table) = table else { return self.document_premises().map(Some) };
                sheet_fields.extend([format!("tables[name={}]", json!(table.name)), "~arrays".into()]);
            }
            sheet_fields.push("props.protection".into());
            under.objects.push(json!(sid));
        }

        if fixed_axes {
            let rows = matches!(command, "home.rowHeight" | "home.hideRows" | "home.unhideRows");
            let unhide = command.starts_with("home.unhide");
            let spans = kition_sheet_engine::cmd::format::line_targets(&self.session, &params, rows).map_err(crate::engine_error)?;
            let spans: Vec<_> = spans
                .into_iter()
                .map(|(first, last)| if unhide && first == last { (first.saturating_sub(1), last.saturating_add(1)) } else { (first, last) })
                .collect();
            if spans.iter().try_fold(0u64, |count, (first, last)| count.checked_add(u64::from(last - first) + 1)).is_none_or(|n| n > 100_000) {
                return self.document_premises().map(Some);
            }
            let fields: &[&str] = match command {
                "home.rowHeight" => &["info.size", "info.hidden", "info.custom"],
                "home.columnWidth" | "home.unhideRows" | "home.unhideColumns" => &["info.size", "info.hidden"],
                _ => &["info.hidden"],
            };
            let axes = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si));
            for (first, last) in spans {
                window(&mut under, &sid, if rows { "rows" } else { "cols" }, first as usize, last as usize + 1);
                for index in first..=last {
                    let Some(line) = axes.and_then(|axes| if rows { axes.rows.get(index as usize) } else { axes.cols.get(index as usize) }) else {
                        return self.document_premises().map(Some);
                    };
                    let target = format!("{}:{line}", if rows { "row" } else { "col" });
                    if !line.as_str().starts_with('p') || line.as_str().len() != 33 {
                        under.objects.push(json!(target));
                    }
                    under.premises.push(premise(target, fields.iter().copied()).positions(["index"]));
                }
            }
            under.objects.push(json!(sid));
        }

        if comments {
            let ranges = if command == "review.deleteComment" {
                params
                    .get("range")
                    .or_else(|| params.get("cell"))
                    .and_then(Value::as_str)
                    .map(|text| text.rsplit_once('!').map_or(text, |(_, body)| body).split(',').filter_map(area).collect::<Vec<_>>())
                    .unwrap_or_else(|| state.selection.ranges.clone())
            } else {
                let at = params
                    .get("cell")
                    .and_then(Value::as_str)
                    .and_then(area)
                    .filter(RangeRef::is_single)
                    .map(|range| range.start)
                    .unwrap_or(state.selection.active);
                vec![RangeRef::new(at, at)]
            };
            if ranges.iter().try_fold(0u64, |count, range| count.checked_add(range.count())).is_none_or(|count| count > 100_000) {
                return self.document_premises().map(Some);
            }
            let axes = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si));
            for range in ranges {
                window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
                for at in range.iter() {
                    let Some((row, col)) = axes.and_then(|ids| Some((ids.rows.get(at.row as usize)?, ids.cols.get(at.col as usize)?))) else {
                        return self.document_premises().map(Some);
                    };
                    sheet_fields.push(format!("comments.row:{row}:col:{col}"));
                }
            }
            under.objects.push(json!(sid));
        }

        match command {
            "sheet.rename" => sheet_fields.push("name".into()),
            "sheet.tabColor" => sheet_fields.push("props.tabColor".into()),
            "sheet.hide" | "sheet.unhide" => sheet_fields.push("props.visibility".into()),
            "sheet.move" => {
                let destination = params.get("to").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(si);
                window(&mut under, "workbook", "sheets", destination, destination);
                under.premises.push(premise(sid.clone(), Vec::<String>::new()).positions(["sheets"]));
            }
            "home.insertSheet" => {
                let at = params
                    .get("before")
                    .and_then(Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(si.saturating_add(1))
                    .min(wb.sheets.len());
                window(&mut under, "workbook", "sheets", at, at);
            }
            "home.deleteSheet" => under.premises.push(Premise::on(sid.clone())),
            _ => {}
        }

        let is_lines = matches!(command, "home.insertRows" | "home.insertColumns" | "home.deleteRows" | "home.deleteColumns");
        if is_lines {
            let cols = command.ends_with("Columns");
            let field = if cols { "cols" } else { "rows" };
            let ranges: Vec<_> = params
                .get(field)
                .or_else(|| params.get("range"))
                .and_then(Value::as_str)
                .and_then(area)
                .map(|a| vec![a])
                .unwrap_or_else(|| state.selection.ranges.clone());
            for range in ranges {
                let (first, last) = if cols { (range.start.col, range.end.col) } else { (range.start.row, range.end.row) };
                let insert = command.starts_with("home.insert");
                let after = params.get(if cols { "right" } else { "below" }).and_then(Value::as_bool).unwrap_or(false);
                let at = if insert && after { last.saturating_add(1) } else { first } as usize;
                let count = params
                    .get("count")
                    .and_then(Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(last.saturating_sub(first).saturating_add(1) as usize);
                window(&mut under, &sid, field, at, if insert { at } else { at.saturating_add(count) });
            }
            sheet_fields.push("props.protection".into());
            under.objects.push(json!(sid));
        }

        let drawing_params = crate::references::drawing_parameters(command);
        if !drawing_params.is_empty() {
            let mut targets: Vec<u32> = drawing_params
                .iter()
                .flat_map(|key| params.get(*key))
                .flat_map(|value| value.as_array().cloned().unwrap_or_else(|| vec![value.clone()]))
                .filter_map(|value| number(&value))
                .collect();
            if targets.is_empty() {
                if drawing_params.contains(&"chart") {
                    targets.extend(sheet.charts.last().map(|chart| chart.id));
                    sheet_fields.push("charts".into());
                } else {
                    targets.extend(self.session.selected_shape);
                }
            }
            let extent = match command {
                "drawing.transform" => transform_extent(wb, si, &targets, &params),
                "arrange.align" | "arrange.group" => alignment_extent(sheet, &targets, command, &params),
                _ => None,
            };
            let axes = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si));
            let precise_geometry = extent.zip(axes).filter(|((row, col), axes)| {
                *row < 100_000
                    && *col < 100_000
                    && (matches!(command, "arrange.align" | "arrange.group")
                        || axes.rows.get(*row as usize).is_some() && axes.cols.get(*col as usize).is_some())
            });
            if let Some(((row, col), axes)) = precise_geometry {
                for (field, end, lines) in [("rows", row, &axes.rows), ("cols", col, &axes.cols)] {
                    window(&mut under, &sid, field, 0, end as usize + 1);
                    for line in lines.iter().take(end as usize + 1) {
                        let target = format!("{}:{line}", if field == "rows" { "row" } else { "col" });
                        under.premises.push(premise(target, ["info.size", "info.hidden"]).positions(["index"]));
                    }
                }
            }
            for id in targets {
                let tool_id = format!("drawing:{id}");
                let follows_owner = drawing_params
                    .iter()
                    .flat_map(|key| given.get(*key))
                    .flat_map(|value| value.as_array().cloned().unwrap_or_else(|| vec![value.clone()]))
                    .any(|value| value.as_str() == Some(tool_id.as_str()));
                if let Some(chart) = sheet.charts.iter().find(|chart| chart.id == id) {
                    let reads_series = command.starts_with("drawing.")
                        || command == "chart.switchRowColumn"
                        || command == "chart.changeColors"
                            && matches!(chart.kind, kition_sheet_engine::model::ChartKind::Pie | kition_sheet_engine::model::ChartKind::Doughnut);
                    let mut sources = Vec::new();
                    if command == "chart.selectData" {
                        sources.extend(params.get("range").and_then(Value::as_str).map(str::to_owned));
                    } else if command == "chart.switchRowColumn" {
                        sources.extend(
                            chart.source.clone().or_else(|| kition_sheet_engine::cmd::insert::series_block(&chart.series).map(|(range, _)| range)),
                        );
                    }
                    if reads_series {
                        for series in &chart.series {
                            sources.extend(
                                [
                                    Some(&series.values),
                                    series.name.as_ref().filter(|name| name.trim().starts_with('=') || name.contains('!')),
                                    series.categories.as_ref(),
                                    series.bubble_sizes.as_ref(),
                                ]
                                .into_iter()
                                .flatten()
                                .cloned(),
                            );
                        }
                    }
                    for source in sources {
                        if let Ok(expr) = kition_sheet_engine::formula::parse(&source) {
                            pending.push((si, Default::default(), expr, SourceFacts::default()));
                        } else {
                            return self.document_premises().map(Some);
                        }
                    }
                    if reads_series || command == "chart.selectData" {
                        under.premises.push(premise("workbook", ["props.dateSystem", "props.defaultStyle", "theme"]));
                    }
                }
                let shapes: Vec<_> = sheet.shapes.iter().filter(|sh| sh.id == id || sh.groups.contains(&id)).collect();
                let members: Vec<_> = if shapes.is_empty() { vec![id] } else { shapes.iter().map(|sh| sh.id).collect() };
                for member in members {
                    let tool = format!("drawing:{member}");
                    let mut p = match command {
                        "drawing.text" => premise(tool, ["text", "look.text_body", "look.kind"]),
                        "shape.setText" => premise(tool, ["text", "look.kind"]),
                        "object.setAnchorMode" => premise(tool, ["~anchorMode"]).positions(["sheet"]),
                        "drawing.transform" | "object.move" => premise(tool, ["look.groups", "look.connection"]).positions(["frame", "sheet"]),
                        "drawing.format" => {
                            let mut fields = vec!["look.kind"];
                            if params.get("fill").is_some() {
                                fields.extend(["look.fill", "look.fill_alpha", "look.gradient"]);
                            }
                            if params.get("stroke").is_some() {
                                fields.extend(["look.line", "look.outline"]);
                            }
                            if params.get("geometry").is_some() {
                                fields.extend(["look.preset", "look.geom"]);
                            }
                            if params.get("effects").is_some() {
                                fields.push("look.effects");
                            }
                            premise(tool, fields)
                        }
                        "drawing.connect" => premise(tool, ["look.kind", "look.preset", "look.geom"]).positions(["frame", "sheet"]),
                        "chart.shapeOutline" => premise(tool, ["look.line"]),
                        "arrange.rotate" | "arrange.align" | "arrange.group" => {
                            if sheet.images.iter().any(|image| image.id == member) {
                                premise(tool, ["props.anchor"]).positions(["sheet"])
                            } else {
                                premise(tool, Vec::<String>::new()).positions(["frame", "sheet"])
                            }
                        }
                        _ => sheet
                            .charts
                            .iter()
                            .find(|chart| chart.id == member)
                            .and_then(|chart| crate::object_read_premises::scalar_chart_premise(tool.clone(), command, &params, chart))
                            .unwrap_or_else(|| Premise::on(tool)),
                    };
                    if matches!(command, "drawing.text" | "drawing.format") && !follows_owner {
                        p = p.positions(["sheet"]);
                    }
                    if command == "drawing.reconnect" {
                        p = p.fields(["look.connection", "look.kind"]).positions(["frame", "sheet"]);
                    }
                    under.premises.push(p);
                    if member != id {
                        under.objects.push(json!(format!("drawing:{member}")));
                    }
                }
            }
            // Every drawing command gates on the sheet's protection.
            sheet_fields.push("props.protection".into());
            let reroutes =
                matches!(command, "drawing.format" | "drawing.text" | "drawing.chart") && sheet.shapes.iter().any(|shape| shape.connection.is_some());
            if matches!(
                command,
                "drawing.transform" | "object.move" | "arrange.rotate" | "drawing.connect" | "drawing.reconnect" | "arrange.align" | "arrange.group"
            ) || reroutes
            {
                sheet_fields.extend(["props.defaultColWidth", "props.defaultRowHeight"].map(str::to_owned));
                if precise_geometry.is_none() {
                    if let Some(axes) = axes.filter(|axes| {
                        axes.rows
                            .len()
                            .checked_add(axes.cols.len())
                            .and_then(|count| count.checked_add(under.premises.len()))
                            .is_some_and(|n| n <= 100_000)
                    }) {
                        // Owning lists alone do not hold each line's size or visibility.
                        sheet_fields.extend(["rows", "cols"].map(str::to_owned));
                        for (kind, lines) in [("row", &axes.rows), ("col", &axes.cols)] {
                            for line in lines {
                                under.premises.push(premise(format!("{kind}:{line}"), ["info.size", "info.hidden"]).positions(["index"]));
                            }
                        }
                    } else {
                        // Keep traversing chart sources even before a replica exists.
                        let fallback = self.document_premises()?;
                        under.premises.extend(fallback.premises);
                    }
                }
            }
            // Shared drawing operations reroute connectors after applying the requested edit.
            if command.starts_with("drawing.")
                && !matches!(command, "drawing.delete" | "drawing.group" | "drawing.ungroup" | "drawing.arrange" | "drawing.describe")
            {
                for shape in sheet.shapes.iter().filter(|shape| shape.connection.is_some()) {
                    under.premises.push(
                        premise(format!("drawing:{}", shape.id), ["look.connection", "look.geom", "look.preset"]).positions(["frame", "sheet"]),
                    );
                    if let Some(connection) = &shape.connection {
                        for end in [connection.start.as_ref(), connection.end.as_ref()].into_iter().flatten() {
                            under.premises.push(premise(format!("drawing:{}", end.id), ["look.geom", "look.preset"]).positions(["frame", "sheet"]));
                        }
                    }
                }
            }
        }

        // Names of tables and pivots resolve inside JSON arrays, not the drawing id space.
        for (parameter, field) in [("table", "tables"), ("pivot", "pivots")] {
            if let Some(value) = params.get(parameter) {
                for candidate in &wb.sheets {
                    let list: Vec<(usize, bool)> = if parameter == "table" {
                        candidate
                            .tables
                            .iter()
                            .enumerate()
                            .map(|(i, t)| (i, value.as_str().is_some_and(|s| t.name.eq_ignore_ascii_case(s))))
                            .collect()
                    } else {
                        candidate
                            .pivots
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                (i, value.as_str().is_some_and(|s| p.name.eq_ignore_ascii_case(s)) || value.as_u64() == Some(u64::from(p.id)))
                            })
                            .collect()
                    };
                    if let Some((index, _)) = list.iter().find(|(_, hit)| *hit) {
                        let owner = ids::sheet_tool_id(candidate);
                        under.objects.push(json!(owner));
                        let name = if parameter == "table" {
                            candidate.tables.get(*index).map(|t| t.name.clone())
                        } else {
                            candidate.pivots.get(*index).map(|p| p.name.clone())
                        };
                        under.premises.push(premise(owner, [format!("{field}[name={}]", json!(name.unwrap_or_default()))]));
                        break;
                    }
                }
            }
        }

        let has_cells = entry || sort;
        if has_cells {
            let mut ranges = params
                .get("range")
                .or_else(|| params.get("cell"))
                .and_then(Value::as_str)
                .map(|s| s.split(',').filter_map(area).collect::<Vec<_>>())
                .unwrap_or_else(|| state.selection.ranges.clone());
            if command == "cell.set" && params.get("array").and_then(Value::as_bool) != Some(true) {
                let at = params.get("cell").and_then(Value::as_str).and_then(area).map(|a| a.start).unwrap_or(state.selection.active);
                ranges = vec![RangeRef::new(at, at)];
            } else if command == "cell.set" {
                ranges = state.selection.ranges.clone();
            }
            if command == "range.setValues"
                && let Some(values) = params.get("values").and_then(Value::as_array)
            {
                let start = ranges.first().map(|r| r.start).unwrap_or_default();
                ranges = values
                    .iter()
                    .enumerate()
                    .filter_map(|(row, values)| {
                        let width = values.as_array().map_or(1, Vec::len);
                        let first = start.offset(row as i64, 0)?;
                        let last = start.offset(row as i64, width.checked_sub(1)? as i64)?;
                        Some(RangeRef::new(first, last))
                    })
                    .collect();
            }
            let formula_at = ranges.first().map(|range| range.start).unwrap_or(state.selection.active);
            if sort {
                sheet_fields.extend(["autofilter", "~sortObjects"].map(str::to_owned));
                under.premises.push(premise("workbook", ["theme"]));
                for scope in kition_sheet_engine::cmd::data::filter_scopes(sheet).into_iter().filter(|scope| !scope.criteria.is_empty()) {
                    pending.push((si, scope.range.start, source_expr(scope.range), SourceFacts { visibility: true, ..SourceFacts::default() }));
                }
                for range in &ranges {
                    pending.push((si, range.start, source_expr(*range), SourceFacts::default()));
                    if command == "data.sort" {
                        for source in self.independent_sort_key_sources(&params, si, *range)? {
                            pending.push((si, source.start, source_expr(source), SourceFacts::default()));
                        }
                    }
                    self.entry_row_premises(&mut under, si, *range);
                    if let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) {
                        // Sort carries complete row metadata along with the cells.
                        for raw in axes.rows.iter().skip(range.start.row as usize).take(range.height() as usize) {
                            under.premises.push(premise(format!("row:{raw}"), ["info"]).positions(["index"]));
                        }
                    }
                }
                // Every table filter can be reapplied, including one outside the sort block.
                sheet_fields.push("tables".into());
            }
            let total = ranges.iter().try_fold(0u64, |n, r| n.checked_add(r.count()));
            if total.is_none_or(|n| n > 100_000) {
                return self.document_premises().map(Some);
            } else {
                for range in &ranges {
                    if entry {
                        self.entry_row_premises(&mut under, si, *range);
                    }
                    if state.kdm().is_none_or(|kdm| {
                        kdm.model().ids().sheets.get(si).is_some_and(|ids| kdm.replica().map_entries(&ids.object, "constraints").is_ok())
                    }) {
                        sheet_fields.extend(kdm_model::constraint_fields(*range));
                    } else {
                        sheet_fields.extend(["merges", "tables", "validations"].map(str::to_owned));
                    }
                    window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
                    window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
                    for cell in range.iter() {
                        let axes = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si));
                        let row = axes.and_then(|ids| ids.rows.get(cell.row as usize)).map(|id| format!("row:{id}"));
                        let col = axes.and_then(|ids| ids.cols.get(cell.col as usize)).map(|id| format!("col:{id}"));
                        // Deterministic padding materializes already-existing logical axes;
                        // only explicitly inserted axes are new objects under an address.
                        under.objects.extend(
                            row.iter()
                                .chain(col.iter())
                                .filter(|id| !id.split_once(':').is_some_and(|(_, raw)| raw.starts_with('p') && raw.len() == 33))
                                .map(|id| json!(id)),
                        );
                        let target = format!("cell:{}!{cell}", sheet.sheet_id);
                        under.objects.push(json!(target));
                        under.premises.push(premise(target, ["input", "text", "style", "array", "picture"]));
                        for (id, field, index) in [(row, "rows", cell.row), (col, "cols", cell.col)] {
                            if let Some(id) = id {
                                under.premises.push(premise(id, ["info.style"]).positions(["index"]));
                            } else {
                                sheet_fields.push(format!("{field}.{index}.info.style"));
                            }
                        }
                    }
                }
            }
            sheet_fields.push("props.protection".into());
            under.premises.push(premise("workbook", ["props.dateSystem", "props.defaultStyle"]));
            // Formula text is composed against observed values and coordinate windows.
            let mut inputs = Vec::new();
            if command == "range.setValues" {
                for (row, values) in params.get("values").and_then(Value::as_array).into_iter().flatten().enumerate() {
                    let columns = values.as_array().map_or_else(|| vec![values], |values| values.iter().collect());
                    for (col, value) in columns.into_iter().enumerate() {
                        if let Some(input) = value.as_str()
                            && let Some(at) = formula_at.offset(row as i64, col as i64)
                        {
                            inputs.push((input, at));
                        }
                    }
                }
            } else if matches!(command, "cell.set" | "range.fill")
                && let Some(input) = params
                    .get("input")
                    .and_then(Value::as_str)
                    .or_else(|| (command == "cell.set").then(|| params.get("value").and_then(Value::as_str)).flatten())
            {
                let at = if command == "range.fill" && params.get("range").is_none() && params.get("cell").is_none() {
                    state.selection.active
                } else {
                    formula_at
                };
                inputs.push((input, at));
            }
            let mut preview = wb.as_ref().clone();
            for (input, at) in inputs {
                // Reuse entry parsing for signed formulas, repaired syntax and text formats.
                let parsed = kition_sheet_engine::cmd::edit::input_to_cell(input, si, at, &mut preview).map_err(crate::engine_error)?;
                let Some(expr) = parsed.and_then(|cell| cell.formula.as_ref().and_then(|formula| formula.expr())) else { continue };
                if command == "range.fill" {
                    if total.is_none_or(|count| count > 10_000) {
                        return self.document_premises().map(Some);
                    }
                    for destination in ranges.iter().flat_map(RangeRef::iter) {
                        let shifted = kition_sheet_engine::formula::adjust::shift_relative(
                            expr.clone(),
                            i64::from(destination.row) - i64::from(at.row),
                            i64::from(destination.col) - i64::from(at.col),
                        );
                        pending.push((si, destination, shifted, SourceFacts { format: false, ..Default::default() }));
                    }
                } else {
                    pending.push((si, at, expr, SourceFacts { format: false, ..Default::default() }));
                }
            }
            under.objects.push(json!(sid));
        }
        if !pending.is_empty() {
            if filter {
                // Filter text uses cell_text, including the workbook's Normal
                // number format and date serial interpretation.
                under.premises.push(premise("workbook", ["props.dateSystem", "props.defaultStyle"]));
            }
            let mut visited_names = BTreeSet::new();
            let mut visited_cells = BTreeSet::new();
            let mut depth = 0usize;
            while let Some((context_index, origin, expr, mut facts)) = pending.pop() {
                depth += 1;
                if depth > 10_000 {
                    return Err("formula dependencies exceed the premise limit".into());
                }
                if let Some(context_sheet) = wb.sheet(context_index) {
                    let mut conservative = false;
                    let mut tab_order = false;
                    let mut tab_names = false;
                    let mut implicit_cell = false;
                    expr.walk(&mut |node| {
                        if let Expr::Call(name, args) = node {
                            match name.as_str() {
                                "SUBTOTAL" => {
                                    facts.format = true;
                                    facts.visibility |= !matches!(args.first(), Some(Expr::Number(code)) if *code <= 100.0);
                                }
                                "AGGREGATE" => {
                                    facts.visibility |= match args.get(1) {
                                        Some(Expr::Number(options)) => matches!(*options as u32, 1 | 3 | 5 | 7),
                                        _ => true,
                                    };
                                }
                                "CELL" => {
                                    facts.format = true;
                                    match args.first() {
                                        Some(Expr::Text(kind)) => {
                                            facts.width |= kind.eq_ignore_ascii_case("width");
                                            facts.sheet_name |= kind.eq_ignore_ascii_case("sheetname");
                                        }
                                        _ => {
                                            facts.width = true;
                                            facts.sheet_name = true;
                                        }
                                    }
                                    implicit_cell |= args.len() < 2;
                                }
                                "SHEET" | "SHEETS" => {
                                    tab_order = true;
                                    tab_names |= name == "SHEET" && args.first().is_some_and(|arg| !matches!(arg, Expr::Ref(_)));
                                }
                                _ => {}
                            }
                        }
                    });
                    if tab_order {
                        under.premises.push(premise("workbook", ["~tabs"]));
                    }
                    if tab_names {
                        for source in &wb.sheets {
                            under.premises.push(premise(ids::sheet_tool_id(source), ["name"]));
                        }
                    }
                    if implicit_cell {
                        pending.push((context_index, origin, source_expr(RangeRef::cell(origin)), facts));
                    }
                    expr.walk(&mut |node| match node {
                        Expr::Ref(reference) => {
                            if matches!(reference.sheet, SheetSel::Span(_, _)) {
                                under.premises.push(premise("workbook", ["~tabs"]));
                            }
                            let owners: Vec<_> = match &reference.sheet {
                                SheetSel::Current => vec![(context_index, context_sheet)],
                                SheetSel::Named(name) => wb
                                    .sheets
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, sh)| sh.name.eq_ignore_ascii_case(name))
                                    .map(|(i, s)| (i, s.as_ref()))
                                    .collect(),
                                SheetSel::Span(_, _) => wb.sheets.iter().enumerate().map(|(i, s)| (i, s.as_ref())).collect(),
                            };
                            if owners.is_empty() {
                                conservative = true;
                                return;
                            }
                            let range = reference.range();
                            for (index, owner) in owners {
                                let id = ids::sheet_tool_id(owner);
                                under.objects.push(json!(id));
                                if !matches!(reference.sheet, SheetSel::Current) {
                                    under.premises.push(premise(id.clone(), ["name"]));
                                }
                                if facts.sheet_name {
                                    under.premises.push(premise(id.clone(), ["name"]));
                                }
                                if facts.width {
                                    under.premises.push(premise(id.clone(), ["props.defaultColWidth"]));
                                }
                                window(&mut under, &id, "rows", range.start.row as usize, range.end.row as usize + 1);
                                window(&mut under, &id, "cols", range.start.col as usize, range.end.col as usize + 1);
                                if let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(index)) {
                                    for (field, start, end, lines) in
                                        [("rows", range.start.row, range.end.row, &axes.rows), ("cols", range.start.col, range.end.col, &axes.cols)]
                                    {
                                        for line in lines.iter().skip(start as usize).take((end - start) as usize + 1) {
                                            let target = format!("{}:{line}", if field == "rows" { "row" } else { "col" });
                                            if !line.as_str().starts_with('p') || line.as_str().len() != 33 {
                                                under.objects.push(json!(target));
                                            }
                                            let mut fields = if facts.format { vec!["info.style"] } else { Vec::new() };
                                            if field == "rows" && facts.visibility {
                                                fields.push("info.hidden");
                                            }
                                            if field == "cols" && facts.width {
                                                fields.extend(["info.size", "info.hidden"]);
                                            }
                                            under.premises.push(premise(target, fields).positions(["index"]));
                                        }
                                    }
                                }
                                if range.count() <= 100_000 {
                                    for at in range.iter() {
                                        let target = format!("cell:{}!{at}", owner.sheet_id);
                                        under.objects.push(json!(target));
                                        let mut fields = vec!["input", "text", "array"];
                                        if facts.format {
                                            fields.push("style");
                                        }
                                        under.premises.push(premise(target, fields));
                                    }
                                } else {
                                    under.premises.push(premise(id, ["cells"]));
                                    if let Some(kdm) = state.kdm() {
                                        let prefix = format!("{}!", owner.sheet_id);
                                        for object in kdm.replica().objects_of_type("Cell") {
                                            if let Some(address) =
                                                kdm.replica().get(&object, "address").ok().and_then(|v| v.as_str().map(str::to_owned))
                                                && let Some(at) = address.strip_prefix(&prefix).and_then(kition_sheet_engine::core::CellRef::parse)
                                                && range.contains(at)
                                            {
                                                let target = format!("cell:{address}");
                                                under.objects.push(json!(target));
                                                under.premises.push(premise(target, ["input", "text", "style", "array"]));
                                            }
                                        }
                                    }
                                }
                                for (at, cell) in owner.cells.iter().filter(|(at, _)| range.contains(*at)) {
                                    if visited_cells.insert((index, at, facts))
                                        && let Some(expr) = cell.formula.as_ref().and_then(|formula| formula.expr())
                                    {
                                        pending.push((index, at, expr, facts));
                                    }
                                }
                            }
                        }
                        Expr::Name(name) => {
                            let definition = wb
                                .names
                                .iter()
                                .find(|n| n.name.eq_ignore_ascii_case(name) && n.scope == Some(context_index))
                                .or_else(|| wb.names.iter().find(|n| n.name.eq_ignore_ascii_case(name) && n.scope.is_none()));
                            let spelling = definition.map_or(name.as_str(), |n| n.name.as_str());
                            under.premises.push(premise(
                                "workbook",
                                [if definition.is_some() { format!("names[name={}]", json!(spelling)) } else { "names".into() }],
                            ));
                            if let Some(definition) = definition {
                                if let Some(object) = wb
                                    .names
                                    .iter()
                                    .position(|name| std::ptr::eq(name, definition))
                                    .and_then(|index| state.kdm()?.model().ids().names.get(index))
                                {
                                    let target = format!("name:{object}");
                                    under.objects.push(json!(target));
                                    under.premises.push(premise(target, ["name", "scope", "formula", "props"]));
                                }
                                if visited_names.insert((definition.scope, spelling.to_ascii_lowercase(), facts))
                                    && let Ok(expr) = kition_sheet_engine::formula::parse(&definition.formula)
                                {
                                    pending.push((definition.scope.unwrap_or(context_index), origin, expr, facts));
                                }
                            }
                        }
                        Expr::Struct(spec) => {
                            if spec.specifiers.contains(&kition_sheet_engine::formula::StructItem::ThisRow) {
                                conservative = true;
                                return;
                            }
                            let mut host = TableReferenceHost(wb);
                            let resolved = kition_sheet_engine::calc::eval::Evaluator::new(&mut host, context_index, origin).eval(node);
                            if let kition_sheet_engine::calc::eval::Ev::R(areas) = resolved {
                                for area in areas {
                                    let Some(owner) = wb.sheet(area.sheet) else {
                                        conservative = true;
                                        continue;
                                    };
                                    let table = owner.tables.iter().find(|table| {
                                        if spec.table.is_empty() {
                                            table.range.contains(origin)
                                        } else {
                                            table.name.eq_ignore_ascii_case(&spec.table)
                                        }
                                    });
                                    if let Some(table) = table {
                                        under.premises.push(premise(ids::sheet_tool_id(owner), [format!("tables[name={}]", json!(table.name))]));
                                        pending.push((area.sheet, origin, source_expr(area.range), facts));
                                    } else {
                                        conservative = true;
                                    }
                                }
                            } else {
                                conservative = true;
                            }
                        }
                        Expr::Call(name, _) if matches!(name.as_str(), "INDIRECT" | "OFFSET" | "SUMIF" | "AVERAGEIF") => conservative = true,
                        // These constructs can produce source areas larger than their literal
                        // references, or resolve a user-defined function through a name.
                        Expr::Call(name, _) if wb.names.iter().any(|defined| defined.name.eq_ignore_ascii_case(name)) => conservative = true,
                        Expr::Binary(kition_sheet_engine::formula::BinOp::Range, _, _) => conservative = true,
                        _ => {}
                    });
                    if conservative {
                        return self.document_premises().map(Some);
                    }
                }
            }
        }
        // By name, index or A1, the sheet itself must still exist. Per-field holds prevent an
        // unrelated change elsewhere on the sheet from turning it into a whole-sheet hold.
        if has_cells
            || filter
            || name_edit
            || validation
            || table_resize
            || is_lines
            || comments
            || fixed_axes
            || !drawing_params.is_empty()
            || properties.is_some()
            || params.get("sheet").is_some()
            || command.starts_with("sheet.")
        {
            under.premises.push(premise(sid.clone(), sheet_fields));
            if params.get("sheet").is_some() || command.starts_with("sheet.") {
                under.objects.push(json!(sid));
            }
        }
        if !self.add_edit_read_premises(command, &params, si, &mut under)? {
            return self.document_premises().map(Some);
        }
        merged(&mut under);
        Ok((!under.premises.is_empty() || !under.objects.is_empty() || !under.within.is_empty()).then_some(under))
    }
}
