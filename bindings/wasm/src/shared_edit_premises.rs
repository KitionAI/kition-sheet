//! Inputs read by shared edit consequences after the command's direct operation.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise},
};
use kition_kit::{DerivedValues, Under};
use kition_sheet_engine::{
    core::{CellRef, RangeRef},
    formula::{Expr, ast::SheetSel},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const LIMIT: usize = 100_000;

fn digest(value: &impl serde::Serialize) -> Result<Value, String> {
    Ok(json!(kition_kit::session::sha256_hex(&serde_json::to_vec(value).map_err(|error| error.to_string())?)))
}

fn store(found: &mut Vec<DerivedValues>, id: Value, name: &str, value: Value) {
    if let Some(existing) = found.iter_mut().find(|existing| existing.id == id) {
        existing.values.insert(name.into(), value);
    } else {
        found.push(DerivedValues { id, values: [(name.into(), value)].into_iter().collect() });
    }
}

fn too_many_cells(wb: &kition_sheet_engine::model::Workbook) -> bool {
    wb.sheets.iter().try_fold(0usize, |count, sheet| count.checked_add(sheet.cells.len()).filter(|count| *count <= LIMIT)).is_none()
}

impl SheetDocument {
    pub(crate) fn hold_header_sync_reads(&self, under: &mut Under) -> Result<(), String> {
        let mut supported = Vec::new();
        self.shared_edit_values(&[json!("workbook")], &["headerSync".into()], &mut supported)?;
        if supported.iter().any(|value| value.values.contains_key("headerSync")) {
            under.premises.push(premise("workbook", ["~headerSync"]));
        } else {
            *under = self.document_premises()?;
        }
        Ok(())
    }

    pub(crate) fn hold_shared_edit_reads(&self, under: &mut Under, si: usize, ranges: &[RangeRef], structural: bool) -> Result<(), String> {
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(()) };
        let sid = json!(ids::sheet_tool_id(sheet));
        let mut supported = Vec::new();
        self.shared_edit_values(std::slice::from_ref(&sid), &["editConnectors".into()], &mut supported)?;
        if !supported.iter().any(|value| value.id == sid && value.values.contains_key("editConnectors")) {
            *under = self.document_premises()?;
            return Ok(());
        }
        under.premises.push(premise(sid, ["~editConnectors"]));
        if structural {
            self.hold_header_sync_reads(under)?;
        }
        let mut headers = BTreeSet::new();
        for table in sheet.tables.iter().filter(|table| table.header_row) {
            let header = RangeRef::new(table.range.start, CellRef::new(table.range.start.row, table.range.end.col));
            if !ranges.iter().any(|range| range.intersects(&header)) {
                continue;
            }
            for column in 0..table.columns.len().min(table.range.width() as usize) {
                let Some(at) = table.range.start.offset(0, column as i64) else { continue };
                if ranges.iter().any(|range| range.contains(at)) {
                    if headers.len() >= LIMIT {
                        *under = self.document_premises()?;
                        return Ok(());
                    }
                    let Some(raw) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)).and_then(|axes| axes.cols.get(at.col as usize))
                    else {
                        *under = self.document_premises()?;
                        return Ok(());
                    };
                    headers.insert(format!("col:{raw}"));
                }
            }
        }
        if !headers.is_empty() {
            let requested: Vec<_> = headers.iter().map(|id| json!(id)).collect();
            let mut values = Vec::new();
            self.table_formula_values(&requested, &["headerReferences".into()], &mut values)?;
            let supported: BTreeSet<_> =
                values.iter().filter(|value| value.values.contains_key("headerReferences")).filter_map(|value| value.id.as_str()).collect();
            if headers.iter().any(|header| !supported.contains(header.as_str())) {
                *under = self.document_premises()?;
                return Ok(());
            }
            for header in headers {
                under.premises.push(premise(header, ["~headerReferences"]));
            }
        }
        Ok(())
    }

    pub(crate) fn shared_edit_premises(&self, command: &str, given: &Value, under: Option<Under>) -> Result<Option<Under>, String> {
        let Some(mut under) = under else { return Ok(None) };
        if under.premises.iter().any(|premise| premise.id == "workbook" && premise.fields.is_none()) {
            return Ok(Some(under));
        }
        let mut params = given.clone();
        let implied = self.take_tool_ids(command, &mut params, self.command_schema(command).as_ref())?;
        let state = self.doc_state()?;
        let mut si = self.command_target_sheet(command, &params, implied).unwrap_or(state.wb.active_sheet);
        if command == "cell.set"
            && let Some(name) = params.get("name").and_then(Value::as_str)
            && let Ok(target) = kition_sheet_engine::cmd::formulas::named_cell(&state.wb, si, name)
        {
            si = target.sheet;
        }
        if command == "edit.paste"
            && params.get("html").is_none()
            && self.session.clipboard.is_none()
            && let Some(text) = params.get("text").and_then(Value::as_str)
        {
            let at = params.get("at").and_then(Value::as_str).and_then(area).map(|range| range.start).unwrap_or(state.selection.current().start);
            return self.shared_edit_premises(
                "range.setValues",
                &json!({"sheet":si,"range":at.a1(),"values":kition_sheet_engine::cmd::edit::parse_tsv(text)}),
                Some(under),
            );
        }
        if command == "formulas.calculationOptions" && params.get("mode").and_then(Value::as_str) == Some("automatic") {
            self.hold_header_sync_reads(&mut under)?;
            merged(&mut under);
            return Ok(Some(under));
        }
        let content = matches!(
            command,
            "cell.set"
                | "range.setValues"
                | "range.fill"
                | "edit.clearAll"
                | "edit.clearContents"
                | "home.mergeCells"
                | "home.mergeCenter"
                | "data.sort"
                | "data.sortAscending"
                | "data.sortDescending"
                | "insert.checkbox"
        );
        if command == "review.changeSpelling" {
            let Some(replacements) = self.spelling_replacements(&params, si)? else { return self.document_premises().map(Some) };
            let ranges: Vec<_> = replacements.into_iter().map(|(at, _)| RangeRef::cell(at)).collect();
            if !ranges.is_empty() {
                self.hold_shared_edit_reads(&mut under, si, &ranges, false)?;
            }
            merged(&mut under);
            return Ok(Some(under));
        }
        let structural = matches!(
            command,
            "home.insertRows"
                | "home.insertColumns"
                | "home.deleteRows"
                | "home.deleteColumns"
                | "home.deleteSheet"
                | "home.insertSheet"
                | "sheet.rename"
                | "sheet.move"
                | "home.hideRows"
                | "home.hideColumns"
                | "home.unhideRows"
                | "home.unhideColumns"
                | "table.headerRow"
                | "table.totalRow"
                | "table.totalFunction"
                | "table.rename"
                | "table.resize"
                | "table.removeDuplicates"
                | "table.convertToRange"
                | "formulas.defineName"
                | "formulas.deleteName"
                | "data.hideDetail"
                | "data.showDetail"
                | "data.sort"
                | "data.sortAscending"
                | "data.sortDescending"
                | "data.filter"
                | "data.filterBy"
                | "data.clearFilter"
                | "data.reapply"
                | "insert.table"
                | "home.formatAsTable"
        );
        if matches!(command, "formulas.defineName" | "formulas.deleteName") {
            self.hold_header_sync_reads(&mut under)?;
            merged(&mut under);
            return Ok(Some(under));
        }
        if matches!(command, "home.insertSheet" | "home.deleteSheet" | "sheet.move") {
            self.hold_header_sync_reads(&mut under)?;
            let count = state.wb.sheets.len();
            let copied = params.get("copy").and_then(Value::as_bool) == Some(true);
            let destination = params
                .get(if command == "home.insertSheet" { "before" } else { "to" })
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or_else(|| state.wb.active_sheet.saturating_add(1))
                .min(if command == "sheet.move" && !copied { count.saturating_sub(1) } else { count });
            let (first, last) = match command {
                "home.insertSheet" => (destination, count),
                "home.deleteSheet" => (si, count),
                _ if copied => (destination, count),
                _ => (si.min(destination), si.max(destination).saturating_add(1)),
            };
            for owner in first..last {
                self.hold_shared_edit_reads(&mut under, owner, &[], false)?;
            }
            merged(&mut under);
            return Ok(Some(under));
        }
        let conditional = command == "home.conditionalFormat" && params.get("rule").is_some()
            || command == "home.clearRules"
            || command == "home.manageRules" && ["delete", "moveUp", "moveDown"].iter().any(|key| params.get(*key).is_some());
        let target_sheet = content
            || conditional
            || matches!(
                command,
                "cell.formatText"
                    | "edit.clearFormats"
                    | "edit.clearComments"
                    | "edit.clearHyperlinks"
                    | "home.bold"
                    | "home.italic"
                    | "home.underline"
                    | "home.doubleUnderline"
                    | "home.strikethrough"
                    | "home.superscript"
                    | "home.subscript"
                    | "home.fontName"
                    | "home.fontSize"
                    | "home.increaseFontSize"
                    | "home.decreaseFontSize"
                    | "home.fontColor"
                    | "home.fillColor"
                    | "home.borders"
                    | "home.drawBorder"
                    | "home.alignLeft"
                    | "home.alignCenter"
                    | "home.alignRight"
                    | "home.justify"
                    | "home.alignTop"
                    | "home.alignMiddle"
                    | "home.alignBottom"
                    | "home.wrapText"
                    | "home.shrinkToFit"
                    | "home.increaseIndent"
                    | "home.decreaseIndent"
                    | "home.orientation"
                    | "home.mergeAcross"
                    | "home.unmergeCells"
                    | "home.numberFormat"
                    | "home.accounting"
                    | "home.percent"
                    | "home.comma"
                    | "home.increaseDecimal"
                    | "home.decreaseDecimal"
                    | "home.cellStyle"
                    | "home.lockCell"
                    | "home.rowHeight"
                    | "home.columnWidth"
                    | "home.autofitRowHeight"
                    | "home.autofitColumnWidth"
                    | "home.defaultWidth"
                    | "home.hideRows"
                    | "home.hideColumns"
                    | "home.unhideRows"
                    | "home.unhideColumns"
                    | "home.insertRows"
                    | "home.insertColumns"
                    | "home.deleteRows"
                    | "home.deleteColumns"
                    | "home.formatAsTable"
                    | "sheet.rename"
                    | "sheet.tabColor"
                    | "sheet.hide"
                    | "table.headerRow"
                    | "table.totalRow"
                    | "table.totalFunction"
                    | "table.rename"
                    | "table.resize"
                    | "table.removeDuplicates"
                    | "table.convertToRange"
                    | "table.bandedRows"
                    | "table.bandedColumns"
                    | "table.filterButton"
                    | "table.firstColumn"
                    | "table.lastColumn"
                    | "table.style"
                    | "review.protectSheet"
                    | "review.unprotectSheet"
                    | "review.newNote"
                    | "review.newComment"
                    | "review.replyComment"
                    | "review.resolveComment"
                    | "review.deleteComment"
                    | "review.showNote"
                    | "data.validation"
                    | "data.group"
                    | "data.ungroup"
                    | "data.hideDetail"
                    | "data.showDetail"
                    | "data.filter"
                    | "data.filterBy"
                    | "data.clearFilter"
                    | "data.reapply"
                    | "insert.table"
                    | "insert.chart"
                    | "insert.recommendedCharts"
                    | "insert.picture"
                    | "insert.shape"
                    | "insert.textBox"
                    | "insert.artText"
                    | "insert.icons"
                    | "insert.link"
                    | "drawing.insert"
                    | "drawing.delete"
                    | "drawing.transform"
                    | "drawing.format"
                    | "drawing.text"
                    | "drawing.chart"
                    | "drawing.reconnect"
                    | "drawing.connect"
                    | "drawing.group"
                    | "drawing.ungroup"
                    | "drawing.arrange"
                    | "drawing.ink"
                    | "drawing.erase"
                    | "draw.stroke"
                    | "draw.inkToShape"
                    | "chart.set"
                    | "chart.delete"
                    | "chart.switchRowColumn"
                    | "chart.addElement"
                    | "chart.changeColors"
                    | "chart.changeType"
                    | "chart.formatSelection"
                    | "chart.quickLayout"
                    | "chart.selectData"
                    | "chart.move"
                    | "chart.shapeFill"
                    | "chart.shapeOutline"
                    | "arrange.align"
                    | "arrange.bringForward"
                    | "arrange.sendBackward"
                    | "arrange.group"
                    | "arrange.rotate"
                    | "shape.setText"
                    | "object.delete"
                    | "object.move"
                    | "object.setAnchorMode"
            )
            || command == "home.formatCells" && (params.get("style").is_some() || params.get("edges").is_some())
            || command == "sheet.unhide" && params.get("sheet").is_some();
        if !target_sheet {
            return Ok(Some(under));
        }
        if too_many_cells(&state.wb) {
            return self.document_premises().map(Some);
        }
        let mut ranges = Vec::new();
        if content {
            ranges = params
                .get("range")
                .or_else(|| params.get("cell"))
                .and_then(Value::as_str)
                .map(|text| text.rsplit_once('!').map_or(text, |(_, body)| body).split(',').filter_map(area).collect())
                .unwrap_or_else(|| state.selection.ranges.clone());
            if command == "cell.set" {
                let at = params
                    .get("name")
                    .and_then(Value::as_str)
                    .and_then(|name| kition_sheet_engine::cmd::formulas::named_cell(&state.wb, si, name).ok())
                    .map(|target| target.at)
                    .or_else(|| params.get("cell").and_then(Value::as_str).and_then(area).map(|range| range.start))
                    .unwrap_or(state.selection.active);
                if params.get("array").and_then(Value::as_bool) != Some(true) {
                    ranges = vec![RangeRef::cell(at)];
                }
            } else if command == "range.setValues"
                && let Some(values) = params.get("values").and_then(Value::as_array)
            {
                let start = ranges.first().map_or(state.selection.current().start, |range| range.start);
                ranges = values
                    .iter()
                    .enumerate()
                    .filter_map(|(row, value)| {
                        let width = value.as_array().map_or(1, Vec::len);
                        Some(RangeRef::new(start.offset(row as i64, 0)?, start.offset(row as i64, width.checked_sub(1)? as i64)?))
                    })
                    .collect();
            }
        }
        self.hold_shared_edit_reads(&mut under, si, &ranges, structural)?;
        if command == "chart.move"
            && let Some(destination) =
                params.get("sheet").and_then(Value::as_str).and_then(|name| state.wb.sheet_index(name)).filter(|owner| *owner != si)
        {
            self.hold_shared_edit_reads(&mut under, destination, &[], false)?;
        }
        if command == "sheet.rename" {
            let Some(sheet) = state.wb.sheet(si) else { return Ok(Some(under)) };
            let mut supported = Vec::new();
            self.shared_edit_values(&[json!("workbook")], &["sheetReferences".into()], &mut supported)?;
            if !supported.iter().any(|value| value.values.contains_key("sheetReferences")) {
                return self.document_premises().map(Some);
            }
            under.premises.push(premise("workbook", [format!("~sheetReferences.{}", sheet.sheet_id)]));
            for (owner, _) in state.wb.sheets.iter().enumerate() {
                self.hold_shared_edit_reads(&mut under, owner, &[], false)?;
            }
        }
        if under.premises.len() > LIMIT {
            return self.document_premises().map(Some);
        }
        merged(&mut under);
        Ok(Some(under))
    }

    pub(crate) fn shared_edit_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        let connectors = names.is_empty() || names.iter().any(|name| name == "editConnectors");
        let references = names.is_empty() || names.iter().any(|name| name == "sheetReferences");
        let headers = names.is_empty() || names.iter().any(|name| name == "headerSync");
        if !connectors && !references && !headers {
            return Ok(());
        }
        let state = self.doc_state()?;
        if too_many_cells(&state.wb) {
            return Ok(());
        }
        if connectors {
            for (si, sheet) in state.wb.sheets.iter().enumerate() {
                let sid = json!(ids::sheet_tool_id(sheet));
                if !requested.is_empty() && !requested.contains(&sid) {
                    continue;
                }
                if sheet.shapes.len() > LIMIT {
                    continue;
                }
                let mut inputs = Vec::new();
                let mut endpoints = BTreeSet::new();
                let mut max_row = 0;
                let mut max_col = 0;
                for shape in sheet.shapes.iter().filter(|shape| shape.connection.is_some()) {
                    inputs.push(json!([shape.id, shape.anchor, shape.rot, shape.flip_h, shape.flip_v, shape.connection]));
                    if let Some(connection) = &shape.connection {
                        for end in [connection.start, connection.end].into_iter().flatten() {
                            endpoints.insert(end.id);
                        }
                    }
                    max_row = max_row.max(shape.anchor.cell.row);
                    max_col = max_col.max(shape.anchor.cell.col);
                }
                for shape in sheet.shapes.iter().filter(|shape| endpoints.contains(&shape.id)) {
                    let object = kition_sheet_engine::cmd::drawing::to_drawing(sheet, shape);
                    inputs.push(json!([shape.id, object.kind, object.frame, object.geometry]));
                    max_row = max_row.max(shape.anchor.cell.row);
                    max_col = max_col.max(shape.anchor.cell.col);
                    for point in kition_office_core::connect::sites(&object) {
                        max_row = max_row.max(sheet.row_at(point.y.max(0.0)));
                        max_col = max_col.max(sheet.col_at(point.x.max(0.0)));
                    }
                }
                let value = if inputs.is_empty() {
                    json!([])
                } else {
                    if max_row as usize >= LIMIT || max_col as usize >= LIMIT {
                        continue;
                    }
                    let axes = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si));
                    let rows: Vec<_> = (0..=max_row)
                        .map(|row| {
                            json!([
                                row,
                                axes.and_then(|axes| axes.rows.get(row as usize)).map(|raw| raw.as_str()),
                                sheet.rows.get(&row).map(|info| (info.size, info.hidden))
                            ])
                        })
                        .collect();
                    let cols: Vec<_> = (0..=max_col)
                        .map(|col| {
                            json!([
                                col,
                                axes.and_then(|axes| axes.cols.get(col as usize)).map(|raw| raw.as_str()),
                                sheet.cols.get(&col).map(|info| (info.size, info.hidden))
                            ])
                        })
                        .collect();
                    json!([inputs, rows, cols, sheet.default_row_height, sheet.default_col_width])
                };
                store(found, sid, "editConnectors", digest(&value)?);
            }
        }
        if references && (requested.is_empty() || requested.contains(&json!("workbook"))) {
            let mut sources: BTreeMap<u32, Vec<Value>> = state.wb.sheets.iter().map(|sheet| (sheet.sheet_id, Vec::new())).collect();
            let mut count = 0usize;
            let mut record = |where_: Value, text: &str| {
                count = count.saturating_add(1);
                if count > LIMIT {
                    return;
                }
                let Ok(expr) = kition_sheet_engine::formula::parse(text.trim_start_matches(['=', '#'])) else { return };
                let mut targets = BTreeSet::new();
                expr.walk(&mut |expr| {
                    if let Expr::Ref(reference) = expr {
                        let names: Vec<_> = match &reference.sheet {
                            SheetSel::Named(name) => vec![name],
                            SheetSel::Span(first, last) => vec![first, last],
                            SheetSel::Current => Vec::new(),
                        };
                        for name in names {
                            if let Some(sheet) = state.wb.sheet_index(name).and_then(|index| state.wb.sheet(index)) {
                                targets.insert(sheet.sheet_id);
                            }
                        }
                    }
                });
                for target in targets {
                    sources.entry(target).or_default().push(json!([where_, text]));
                }
            };
            for sheet in &state.wb.sheets {
                for (at, cell) in sheet.cells.iter() {
                    if let Some(formula) = &cell.formula {
                        record(json!([sheet.sheet_id, "cell", at]), &formula.text);
                    }
                }
                let mut copy = sheet.as_ref().clone();
                let mut index = 0;
                kition_sheet_engine::cmd::sheet::for_each_feature_formula(&mut copy, |formula| {
                    record(json!([sheet.sheet_id, "feature", index]), formula);
                    index += 1;
                });
                for (at, link) in &sheet.hyperlinks {
                    if !link.is_external() {
                        record(json!([sheet.sheet_id, "link", at]), &link.target);
                    }
                }
            }
            for (index, name) in state.wb.names.iter().enumerate() {
                record(json!(["name", index]), &name.formula);
            }
            if count > LIMIT {
                return Ok(());
            }
            let values: serde_json::Map<String, Value> =
                sources.into_iter().map(|(id, sources)| Ok((id.to_string(), digest(&sources)?))).collect::<Result<_, String>>()?;
            store(found, json!("workbook"), "sheetReferences", Value::Object(values));
        }
        if headers && (requested.is_empty() || requested.contains(&json!("workbook"))) {
            let mut inputs = Vec::new();
            let mut stale = Vec::new();
            let mut count = 0usize;
            for (si, sheet) in state.wb.sheets.iter().enumerate() {
                for table in sheet.tables.iter().filter(|table| table.header_row) {
                    for (column, old) in table.columns.iter().enumerate() {
                        count = count.saturating_add(1);
                        if count > LIMIT {
                            return Ok(());
                        }
                        let Some(at) = table.range.start.offset(0, column as i64).filter(|at| table.range.contains(*at)) else { continue };
                        let Some((text, is_text)) = kition_sheet_engine::tables::header_text(&state.wb, si, at) else { continue };
                        if text == old.name && is_text {
                            continue;
                        }
                        let column_names: Vec<_> = table.columns.iter().map(|column| &column.name).collect();
                        inputs.push(json!([sheet.sheet_id, table.id, table.name, table.range, column_names, at, text, is_text, sheet.cell(at)]));
                        stale.push((json!(format!("cell:{}!{at}", sheet.sheet_id)), json!(ids::sheet_tool_id(sheet))));
                    }
                }
            }
            if !stale.is_empty() {
                let mut values = Vec::new();
                self.table_formula_values(&[], &["headerReferences".into()], &mut values)?;
                for (header, sheet) in stale {
                    let Some(references) = values.iter().find(|value| value.id == header).and_then(|value| value.values.get("headerReferences"))
                    else {
                        return Ok(());
                    };
                    let mut connectors = Vec::new();
                    self.shared_edit_values(std::slice::from_ref(&sheet), &["editConnectors".into()], &mut connectors)?;
                    let Some(connectors) = connectors.iter().find(|value| value.id == sheet).and_then(|value| value.values.get("editConnectors"))
                    else {
                        return Ok(());
                    };
                    inputs.push(json!([header, references, connectors]));
                }
            }
            store(found, json!("workbook"), "headerSync", digest(&inputs)?);
        }
        Ok(())
    }
}
