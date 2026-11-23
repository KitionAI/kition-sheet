//! Formatting holds assigned styles and fitted-row height contributions.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise, window},
};
use kition_kit::{DerivedValues, Under};
use kition_sheet_engine::core::{CellRef, MAX_COLS};
use serde_json::{Value, json};

fn extend(under: &mut Under, other: Under) {
    under.objects.extend(other.objects);
    under.premises.extend(other.premises);
    under.within.extend(other.within);
}

fn style_command(command: &str, params: &Value) -> bool {
    match command {
        "home.bold"
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
        | "home.numberFormat"
        | "home.accounting"
        | "home.percent"
        | "home.comma"
        | "home.increaseDecimal"
        | "home.decreaseDecimal"
        | "home.lockCell"
        | "cell.formatText" => true,
        "home.formatCells" => params.get("edges").is_none() && params.get("merge").is_none(),
        _ => false,
    }
}

impl SheetDocument {
    fn border_merge_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        let border = matches!(command, "home.borders" | "home.drawBorder");
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let ranges = params
            .get("range")
            .or_else(|| params.get("cell"))
            .and_then(Value::as_str)
            .map(|text| text.split(',').filter_map(area).collect::<Vec<_>>())
            .unwrap_or_else(|| state.selection.ranges.clone());
        if ranges.is_empty()
            || ranges.iter().try_fold(0u64, |n, range| n.checked_add(range.count())).is_none_or(|count| count > 100_000)
            || ranges.iter().any(|range| range.is_full_rows() || range.is_full_cols())
        {
            return Ok(None);
        }
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
        let sid = ids::sheet_tool_id(sheet);
        let unmerge = command == "home.unmergeCells" || command == "home.mergeCenter" && ranges.iter().all(|range| sheet.merges.contains(range));
        let content = !border && !unmerge && command != "home.mergeAcross";
        let mut under = if content {
            let mut params = params.clone();
            params["on"] = json!(true);
            let Some(mut under) = self.format_premises("home.bold", &params, si)? else { return Ok(None) };
            for premise in &mut under.premises {
                if let Some(fields) = &mut premise.fields {
                    for field in fields.iter_mut().filter(|field| field.as_str() == "~layout") {
                        *field = "~fit".into();
                    }
                }
            }
            under
        } else {
            Under::default()
        };
        under.objects.push(json!(sid));
        let mut fields = vec!["props.protection".to_owned()];
        if content {
            fields.push("props.defaultRowHeight".into());
        }
        if params.get("sheet").is_some_and(Value::is_string)
            || ["range", "cell"].iter().any(|key| params.get(key).and_then(Value::as_str).is_some_and(|text| text.contains('!')))
        {
            fields.push("name".into());
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64) {
            window(&mut under, "workbook", "sheets", index as usize, index as usize + 1);
        }
        let mut styled = std::collections::BTreeSet::new();
        for range in ranges {
            window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
            window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
            if border {
                for at in range.iter() {
                    styled.insert(at);
                    if command == "home.borders" && params.get("preset").and_then(Value::as_str) == Some("none") {
                        for (edge, dr, dc) in [
                            (at.row == range.start.row, -1, 0),
                            (at.row == range.end.row, 1, 0),
                            (at.col == range.start.col, 0, -1),
                            (at.col == range.end.col, 0, 1),
                        ] {
                            if edge && let Some(neighbor) = at.offset(dr, dc) {
                                styled.insert(neighbor);
                            }
                        }
                    }
                }
            } else if state.kdm().is_some_and(|kdm| kdm.replica().map_entries(&axes.object, "constraints").is_ok()) {
                fields.extend(kdm_model::constraint_fields(range));
            } else {
                fields.extend(["merges", "tables"].map(str::to_owned));
            }
        }
        for at in styled {
            window(&mut under, &sid, "rows", at.row as usize, at.row as usize + 1);
            window(&mut under, &sid, "cols", at.col as usize, at.col as usize + 1);
            let id = format!("cell:{}!{at}", sheet.sheet_id);
            under.objects.push(json!(id));
            under.premises.push(premise(id, ["style"]));
            for (kind, index, lines) in [("row", at.row, &axes.rows), ("col", at.col, &axes.cols)] {
                if let Some(raw) = lines.get(index as usize) {
                    under.premises.push(premise(format!("{kind}:{raw}"), ["~style"]).positions(["index"]));
                } else {
                    fields.push(format!("{kind}s.{index}.info.style"));
                }
            }
        }
        under.premises.push(premise(sid, fields));
        under.premises.push(premise("workbook", ["props.defaultStyle"]));
        merged(&mut under);
        Ok(Some(under))
    }

    fn autofit_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        let rows = command == "home.autofitRowHeight";
        let spans = kition_sheet_engine::cmd::format::line_targets(&self.session, params, rows).map_err(crate::engine_error)?;
        if spans.iter().try_fold(0u64, |count, (first, last)| count.checked_add(u64::from(last - first) + 1)).is_none_or(|count| count > 100_000) {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
        let lines = if rows { &axes.rows } else { &axes.cols };
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        let mut fields = vec!["props.protection"];
        if params.get(if rows { "rows" } else { "cols" }).and_then(Value::as_str).is_some_and(|text| text.contains('!')) {
            fields.push("name");
        }
        under.premises.push(premise(sid.clone(), fields));
        under.premises.push(premise("workbook", ["props.defaultStyle", "props.dateSystem"]));
        for (first, last) in spans {
            if lines.get(last as usize).is_none() {
                return Ok(None);
            }
            window(&mut under, &sid, if rows { "rows" } else { "cols" }, first as usize, last as usize + 1);
            for raw in lines.iter().skip(first as usize).take((last - first) as usize + 1) {
                under.premises.push(premise(format!("{}:{raw}", if rows { "row" } else { "col" }), ["info", "~layout"]).positions(["index"]));
            }
        }
        merged(&mut under);
        Ok(Some(under))
    }

    pub(crate) fn entry_row_premises(&self, under: &mut Under, si: usize, range: kition_sheet_engine::core::RangeRef) {
        if let Ok(state) = self.doc_state()
            && let Some(sheet) = state.wb.sheet(si)
            && let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si))
        {
            under.premises.push(premise(ids::sheet_tool_id(sheet), ["props.defaultRowHeight"]));
            for raw in axes.rows.iter().skip(range.start.row as usize).take(range.height() as usize) {
                under.premises.push(premise(format!("row:{raw}"), ["~fit", "info.size", "info.custom"]).positions(["index"]));
            }
        }
    }

    pub(crate) fn row_layout_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        if !names.is_empty() && !names.iter().any(|name| matches!(name.as_str(), "layout" | "fit" | "style" | "outline" | "hidden")) {
            return Ok(());
        }
        let state = self.doc_state()?;
        let Some(kdm) = state.kdm() else { return Ok(()) };
        for (si, axes) in kdm.model().ids().sheets.iter().enumerate() {
            let Some(sheet) = state.wb.sheet(si) else { continue };
            for (row, raw) in axes.rows.iter().enumerate() {
                let id = json!(format!("row:{raw}"));
                if !requested.is_empty() && !requested.contains(&id) {
                    continue;
                }
                let row = row as u32;
                let cells: Vec<_> = sheet
                    .cells
                    .row(row, 0, MAX_COLS - 1)
                    .map(|(col, cell)| {
                        let at = CellRef::new(row, col);
                        json!([col, cell, state.wb.styles.get(sheet.style_id(at)), sheet.col_width(col), sheet.merge_at(at)])
                    })
                    .collect();
                let pictures: Vec<_> = sheet
                    .cell_pictures
                    .iter()
                    .filter(|(at, _)| at.row == row)
                    .map(|(at, picture)| json!([at.col, picture.mime, picture.alt, kition_kit::session::sha256_hex(&picture.data)]))
                    .collect();
                let value = json!([cells, pictures, sheet.rows.get(&row), sheet.default_row_height, state.wb.styles.get(Default::default())]);
                let digest = kition_kit::session::sha256_hex(&serde_json::to_vec(&value).map_err(|error| error.to_string())?);
                // Height fitting uses base font size, wrapped display text and explicit
                // line breaks. Plain small cells, including newly styled empty cells,
                // contribute only the default height.
                let normal = state.wb.styles.get(Default::default()).font.size;
                let fit: Vec<_> = sheet
                    .cells
                    .row(row, 0, MAX_COLS - 1)
                    .filter_map(|(col, cell)| {
                        let at = CellRef::new(row, col);
                        let style = state.wb.styles.get(cell.style);
                        let text = kition_sheet_engine::display::cell_text(&state.wb, sheet, at);
                        let multiline = text.lines().count() > 1;
                        if style.font.size <= normal && !style.align.wrap && !multiline {
                            return None;
                        }
                        if cell.value.is_empty() && cell.formula.is_none() && style.font.size <= normal {
                            return None;
                        }
                        Some(if style.align.wrap {
                            json!([col, style.font, text, sheet.col_width(col), sheet.merge_at(at)])
                        } else {
                            json!([col, style.font.size, text.lines().count().max(1), sheet.merge_at(at)])
                        })
                    })
                    .collect();
                let fit = kition_kit::session::sha256_hex(&serde_json::to_vec(&fit).map_err(|error| error.to_string())?);
                let style = sheet.rows.get(&row).and_then(|info| info.style).map(|id| state.wb.styles.get(id));
                found.push(DerivedValues {
                    id,
                    values: [
                        ("layout".into(), json!(digest)),
                        ("fit".into(), json!(fit)),
                        ("style".into(), json!(style)),
                        ("outline".into(), json!(sheet.rows.get(&row).map_or(0, |info| info.outline))),
                        ("hidden".into(), json!(sheet.is_row_hidden(row))),
                    ]
                    .into_iter()
                    .collect(),
                });
            }
            if requested.is_empty() || requested.iter().any(|id| id.as_str().is_some_and(|id| id.starts_with("col:"))) {
                let mut columns = std::collections::BTreeMap::new();
                for (at, cell) in sheet.cells.iter() {
                    columns.entry(at.col).or_insert_with(Vec::new).push((at, cell));
                }
                for (col, raw) in axes.cols.iter().enumerate() {
                    let id = json!(format!("col:{raw}"));
                    if !requested.is_empty() && !requested.contains(&id) {
                        continue;
                    }
                    let col = col as u32;
                    let cells: Vec<_> = columns
                        .get(&col)
                        .into_iter()
                        .flatten()
                        .map(|(at, cell)| json!([at.row, cell, state.wb.styles.get(sheet.style_id(*at)), sheet.merge_at(*at)]))
                        .collect();
                    let value = json!([cells, sheet.cols.get(&col), state.wb.styles.get(Default::default())]);
                    let digest = kition_kit::session::sha256_hex(&serde_json::to_vec(&value).map_err(|error| error.to_string())?);
                    let style = sheet.cols.get(&col).and_then(|info| info.style).map(|id| state.wb.styles.get(id));
                    found.push(DerivedValues {
                        id,
                        values: [
                            ("layout".into(), json!(digest)),
                            ("style".into(), json!(style)),
                            ("outline".into(), json!(sheet.cols.get(&col).map_or(0, |info| info.outline))),
                            ("hidden".into(), json!(sheet.cols.get(&col).is_some_and(|info| info.hidden))),
                        ]
                        .into_iter()
                        .collect(),
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn format_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if matches!(command, "home.borders" | "home.drawBorder" | "home.mergeCells" | "home.mergeCenter" | "home.mergeAcross" | "home.unmergeCells") {
            return self.border_merge_premises(command, params, si);
        }
        if command == "home.formatCells" && (params.get("edges").is_some() || params.get("merge").is_some()) {
            let mut base = params.clone();
            if let Some(object) = base.as_object_mut() {
                object.remove("edges");
                object.remove("merge");
            }
            let mut under = Under::default();
            if params.get("style").is_some() {
                let Some(style) = self.format_premises(command, &base, si)? else { return Ok(None) };
                extend(&mut under, style);
            }
            for edge in params.get("edges").and_then(Value::as_array).into_iter().flatten() {
                let mut params = base.clone();
                for key in ["preset", "style", "color"] {
                    if let Some(value) = edge.get(key) {
                        params[key] = value.clone();
                    }
                }
                let Some(edge) = self.border_merge_premises("home.borders", &params, si)? else { return Ok(None) };
                extend(&mut under, edge);
            }
            if let Some(merge) = params.get("merge").and_then(Value::as_bool) {
                let Some(merge) = self.border_merge_premises(if merge { "home.mergeCells" } else { "home.unmergeCells" }, &base, si)? else {
                    return Ok(None);
                };
                extend(&mut under, merge);
            }
            merged(&mut under);
            return Ok(Some(under));
        }
        if matches!(command, "home.autofitRowHeight" | "home.autofitColumnWidth") {
            return self.autofit_premises(command, params, si);
        }
        if !style_command(command, params) {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let sheet = state.wb.sheet(si).ok_or("no sheet")?;
        let ranges = if command == "cell.formatText" {
            vec![
                params
                    .get("cell")
                    .and_then(Value::as_str)
                    .and_then(area)
                    .filter(|range| range.is_single())
                    .unwrap_or_else(|| kition_sheet_engine::core::RangeRef::cell(state.selection.active)),
            ]
        } else {
            params
                .get("range")
                .or_else(|| params.get("cell"))
                .and_then(Value::as_str)
                .map(|text| text.split(',').filter_map(area).collect::<Vec<_>>())
                .unwrap_or_else(|| state.selection.ranges.clone())
        };
        if ranges.iter().try_fold(0u64, |n, r| n.checked_add(r.count())).is_none_or(|n| n > 100_000)
            || ranges.iter().any(|r| r.is_full_rows() || r.is_full_cols())
        {
            return Ok(None);
        }
        let Some(kdm) = state.kdm() else { return Ok(None) };
        let Some(axes) = kdm.model().ids().sheets.get(si) else { return Ok(None) };
        if ranges.iter().any(|r| axes.rows.get(r.end.row as usize).is_none()) {
            return Ok(None);
        }
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        let mut fields = vec!["props.protection".to_owned(), "autofilter".into()];
        if command != "home.formatCells" || params.get("style").is_some() {
            fields.push("props.defaultRowHeight".into());
        }
        if params.get("sheet").is_some_and(Value::is_string)
            || ["range", "cell"].iter().any(|key| params.get(key).and_then(Value::as_str).is_some_and(|text| text.contains('!')))
        {
            fields.push("name".into());
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64) {
            window(&mut under, "workbook", "sheets", index as usize, index as usize + 1);
        }
        let mut styles = std::collections::BTreeSet::new();
        for range in &ranges {
            window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
            window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
            for row in range.start.row..=range.end.row {
                if let Some(raw) = axes.rows.get(row as usize) {
                    let layout = if command == "home.bold" && !range.is_single() { "~layout" } else { "~fit" };
                    under.premises.push(premise(format!("row:{raw}"), [layout, "info"]).positions(["index"]));
                }
            }
            for cell in range.iter() {
                if (command != "home.formatCells" || params.get("style").is_some()) && sheet.style_id(cell).0 != 0 {
                    let Some(key) = kdm.model().ids().styles.get(sheet.style_id(cell).0 as usize) else { return Ok(None) };
                    styles.insert(format!("styles.{key}.style"));
                }
                let id = format!("cell:{}!{cell}", sheet.sheet_id);
                under.objects.push(json!(id));
                under.premises.push(premise(id, ["input", "text", "style", "array", "picture"]));
                if let Some(col) = axes.cols.get(cell.col as usize) {
                    under.premises.push(premise(format!("col:{col}"), ["info.style"]).positions(["index"]));
                } else {
                    fields.push(format!("cols.{}.info.style", cell.col));
                }
            }
        }
        // Toggles and decimal/font stepping read the active cell even for an explicit range.
        let toggles_active = matches!(
            command,
            "home.bold"
                | "home.italic"
                | "home.underline"
                | "home.doubleUnderline"
                | "home.strikethrough"
                | "home.superscript"
                | "home.subscript"
                | "home.wrapText"
                | "home.shrinkToFit"
                | "home.lockCell"
                | "home.alignLeft"
                | "home.alignCenter"
                | "home.alignRight"
                | "home.justify"
        );
        if toggles_active && params.get("on").is_none()
            || matches!(command, "home.increaseFontSize" | "home.decreaseFontSize" | "home.increaseDecimal" | "home.decreaseDecimal")
            || command == "home.formatCells" && params.get("style").is_some()
        {
            let active = state.selection.active;
            if sheet.style_id(active).0 != 0 {
                let Some(key) = kdm.model().ids().styles.get(sheet.style_id(active).0 as usize) else { return Ok(None) };
                styles.insert(format!("styles.{key}.style"));
            }
            under.premises.push(premise(format!("cell:{}!{active}", sheet.sheet_id), ["style", "input", "text"]));
            if let Some(row) = axes.rows.get(active.row as usize) {
                under.premises.push(premise(format!("row:{row}"), ["info.style"]).positions(["index"]));
            }
            if let Some(col) = axes.cols.get(active.col as usize) {
                under.premises.push(premise(format!("col:{col}"), ["info.style"]).positions(["index"]));
            }
        }
        under.premises.push(premise(sid, fields));
        under.premises.push(premise("workbook", ["props.defaultStyle", "props.dateSystem"]));
        under.premises.push(premise("workbook", styles));
        merged(&mut under);
        Ok(Some(under))
    }
}
