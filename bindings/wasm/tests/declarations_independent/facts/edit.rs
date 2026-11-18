//! Projections of ENTRY, FIT, STYLE, HEADERS and STRUCTURE in edit_format.rs.
use super::model::{Context, Facts, area};
use crate::paths::Reading;
use kition_sheet_engine::core::{CellRef, RangeRef};
use serde_json::{Value, json};

pub(super) fn expected(row: &Reading, params: &Value, cx: &Context) -> Facts {
    let mut out = Facts::default();
    let si = cx.sheet(params);
    let sid = cx.sid(si);
    let range = cx.range(params);
    let command = row.command;
    if matches!(command, "edit.undo" | "edit.redo") {
        cx.whole(&mut out);
        return out;
    }
    if command == "home.formatCells" && params.as_object().unwrap().is_empty() {
        return out;
    }
    cx.lookup(&mut out, params, si);
    match command {
        "cell.set" | "range.setValues" | "range.fill" => entry(&mut out, command, params, cx, si),
        "cell.formatText" => {
            let at = params.get("cell").and_then(Value::as_str).and_then(area).map_or(cx.active_cell, |r| r.start);
            cx.address(&mut out, si, RangeRef::cell(at));
            cx.inherited_style(&mut out, si, at);
            out.add(&cx.cell(si, at), ["input", "text", "array"]);
            out.add(&sid, ["props.protection"]);
            cx.fit(&mut out, si, RangeRef::cell(at));
            cx.shared(&mut out, si, Some(RangeRef::cell(at)), false);
        }
        "edit.clearAll" | "edit.clearFormats" | "edit.clearContents" | "edit.clearComments" | "edit.clearHyperlinks" => {
            clear(&mut out, command, params, cx, si)
        }
        "edit.fillDown" | "edit.fillRight" | "edit.fillUp" | "edit.fillLeft" => fill(&mut out, command, params, cx, si),
        "home.borders" | "home.drawBorder" => borders(&mut out, params, cx, si),
        "home.mergeCells" | "home.mergeAcross" | "home.mergeCenter" | "home.unmergeCells" => merge(&mut out, command, params, cx, si),
        "home.formatCells" if params.get("edges").is_some() || params.get("merge").is_some() => {
            if params.get("style").is_some() {
                style(&mut out, command, params, cx, si);
            }
            for edge in params.get("edges").and_then(Value::as_array).into_iter().flatten() {
                let mut p = params.clone();
                p["preset"] = edge.get("preset").cloned().unwrap_or(json!("all"));
                borders(&mut out, &p, cx, si);
            }
            if let Some(on) = params.get("merge").and_then(Value::as_bool) {
                merge(&mut out, if on { "home.mergeCells" } else { "home.unmergeCells" }, params, cx, si);
            }
        }
        "home.rowHeight"
        | "home.columnWidth"
        | "home.hideRows"
        | "home.hideColumns"
        | "home.unhideRows"
        | "home.unhideColumns"
        | "home.autofitRowHeight"
        | "home.autofitColumnWidth" => lines(&mut out, command, params, cx, si),
        "home.defaultWidth" => {
            out.add(&sid, ["props.defaultColWidth"]);
            cx.shared(&mut out, si, None, false);
        }
        "home.insertRows" | "home.insertColumns" | "home.deleteRows" | "home.deleteColumns" => structure(&mut out, command, params, cx, si),
        "home.insertSheet" | "home.deleteSheet" | "sheet.rename" | "sheet.move" | "sheet.tabColor" | "sheet.hide" | "sheet.unhide" => {
            sheets(&mut out, command, params, cx, si)
        }
        "edit.paste" | "edit.pasteSpecial" => paste(&mut out, command, params, cx, si),
        "edit.autoFill" | "edit.fillSeries" | "edit.smartFill" | "edit.replace" | "edit.formatPainter" | "edit.moveCells" | "home.insertCells"
        | "home.deleteCells" => complex(&mut out, command, params, cx, si),
        _ if command.starts_with("home.") || command == "cell.formatText" => style(&mut out, command, params, cx, si),
        _ => panic!("unprojected edit path {command} {params}"),
    }
    // Keep the independently recorded span visible in debugging, even on an
    // empty destination: no declaration controls which range is projected.
    assert!(range.start.row <= range.end.row);
    out
}

fn formula(out: &mut Facts, cx: &Context, si: usize, input: &str) {
    if input.starts_with('=') {
        let expr = kition_sheet_engine::formula::parse(input).unwrap();
        cx.expression(out, si, &expr, 0);
    }
}

fn entry(out: &mut Facts, command: &str, params: &Value, cx: &Context, mut si: usize) {
    let mut range = cx.range(params);
    if command == "cell.set" {
        range = RangeRef::cell(params.get("cell").and_then(Value::as_str).and_then(area).map(|range| range.start).unwrap_or(cx.active_cell));
        if params.get("array").and_then(Value::as_bool) == Some(true) {
            range = cx.selected;
        }
        if let Some(name) = params.get("name").and_then(Value::as_str) {
            let target = kition_sheet_engine::cmd::formulas::named_cell(&cx.wb, si, name).unwrap();
            si = target.sheet;
            range = RangeRef::cell(target.at);
            out.0.clear();
            out.add("workbook", [format!("names[name={}]", json!(name))]);
            for (i, _) in cx.wb.names.iter().enumerate().filter(|(_, definition)| definition.name.eq_ignore_ascii_case(name)) {
                out.add(&format!("name:{}", cx.ids.names[i]), ["name", "scope", "formula", "props"]);
            }
            // A static name follows persistent axes, rather than a literal A1
            // window. edit.rs::cell_set -> formulas.rs::named_cell resolves the
            // cell; the stored row layout expands its old entry and fitted row.
            out.add(&cx.axis(si, true, range.start.row), ["info", "~layout"]);
            out.add(&cx.axis(si, false, range.start.col), ["info.style"]);
            out.add(&cx.sid(si), ["props.protection", "features.merges", "features.tables", "features.validations"]);
            out.add("workbook", ["props.defaultStyle", "props.dateSystem"]);
            return;
        }
    }
    if command == "range.setValues" {
        for (r, row) in params["values"].as_array().unwrap().iter().enumerate() {
            let cells = row.as_array().map_or_else(|| vec![row], |row| row.iter().collect::<Vec<_>>());
            if cells.is_empty() {
                continue;
            }
            let first = range.start.offset(r as i64, 0).unwrap();
            let last = first.offset(0, cells.len() as i64 - 1).unwrap();
            cx.entry(out, si, RangeRef::new(first, last), cells.iter().any(|cell| cell.is_string()));
            for (c, cell) in cells.into_iter().enumerate() {
                if let Some(input) = cell.as_str() {
                    formula(out, cx, si, input);
                }
                let _ = c;
            }
        }
    } else {
        cx.entry(out, si, range, true);
        if let Some(input) = params.get("input").and_then(Value::as_str) {
            formula(out, cx, si, input);
            if command == "range.fill" && range.count() > 1 && input.starts_with('=') {
                let expr = kition_sheet_engine::formula::parse(input).unwrap();
                for at in range.iter().skip(1) {
                    let translated = kition_sheet_engine::formula::adjust::shift_relative(
                        expr.clone(),
                        at.row as i64 - range.start.row as i64,
                        at.col as i64 - range.start.col as i64,
                    );
                    cx.expression(out, si, &translated, 0);
                }
            }
        }
    }
}

fn style(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let range = cx.range(params);
    cx.address(out, si, range);
    out.add(&cx.sid(si), ["props.protection", "features.autofilter"]);
    out.add("workbook", ["props.dateSystem"]);
    let points: Vec<_> = if range.is_full_cols() || range.is_full_rows() {
        let rows = range.is_full_rows();
        let (first, last, field) = if rows { (range.start.row, range.end.row, "rows") } else { (range.start.col, range.end.col, "cols") };
        for i in first..=last {
            out.add(&cx.axis(si, rows, i), ["info.style"]);
            out.position(&cx.axis(si, rows, i), &["index"]);
        }
        out.window(&cx.sid(si), field, first as usize, last as usize + 1);
        cx.wb.sheets[si].cells.iter().filter(|(at, _)| range.contains(*at)).map(|(at, _)| at).collect()
    } else {
        range.iter().collect()
    };
    for at in points {
        cx.inherited_style(out, si, at);
        out.add(&cx.cell(si, at), ["text"]);
    }
    let fitted =
        if range.is_full_cols() || range.is_full_rows() { range.intersection(&cx.wb.sheets[si].used_range().unwrap()).unwrap() } else { range };
    cx.fit(out, si, fitted);
    let active_style =
        matches!(command, "home.increaseFontSize" | "home.decreaseFontSize" | "home.increaseDecimal" | "home.decreaseDecimal" | "home.formatCells")
            || params.get("on").is_none()
                && matches!(
                    command,
                    "home.alignLeft" | "home.alignCenter" | "home.alignRight" | "home.justify" | "home.underline" | "home.doubleUnderline"
                )
            || params.get("on").is_none()
                && matches!(
                    command,
                    "home.bold"
                        | "home.italic"
                        | "home.strikethrough"
                        | "home.superscript"
                        | "home.subscript"
                        | "home.wrapText"
                        | "home.shrinkToFit"
                        | "home.lockCell"
                );
    if active_style {
        cx.address(out, cx.active, RangeRef::cell(cx.active_cell));
        cx.inherited_style(out, cx.active, cx.active_cell);
    }
    if matches!(command, "home.increaseDecimal" | "home.decreaseDecimal") {
        cx.source(out, cx.active, RangeRef::cell(cx.active_cell), true);
        // format.rs::decimals reads computed value, not array metadata.
        out.0.retain(|fact| !(fact.object == cx.cell(cx.active, cx.active_cell) && fact.field == "array"));
    }
    if command == "home.cellStyle" {
        let name = params["name"].as_str().unwrap();
        out.add("workbook", [format!("cellStyleDefinitions.{}", kdm_model::cell_style_name_key(name))]);
    }
    cx.shared(out, si, None, false);
}

pub(super) fn borders(out: &mut Facts, params: &Value, cx: &Context, si: usize) {
    let range = cx.range(params);
    let mut points: std::collections::BTreeSet<_> = range.iter().take(100_000).collect();
    if params.get("preset").and_then(Value::as_str) == Some("none") {
        for at in range.iter() {
            for (boundary, dr, dc) in [
                (at.row == range.start.row, -1, 0),
                (at.row == range.end.row, 1, 0),
                (at.col == range.start.col, 0, -1),
                (at.col == range.end.col, 0, 1),
            ] {
                if boundary && let Some(at) = at.offset(dr, dc) {
                    points.insert(at);
                }
            }
        }
    }
    for at in points {
        cx.address(out, si, RangeRef::cell(at));
        out.add(&cx.cell(si, at), ["style"]);
        out.add(&cx.axis(si, true, at.row), ["~style"]);
        out.add(&cx.axis(si, false, at.col), ["~style"]);
    }
    out.add(&cx.sid(si), ["props.protection"]);
    out.add("workbook", ["props.defaultStyle"]);
    cx.shared(out, si, None, false);
}

fn merge(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let range = cx.range(params);
    cx.address(out, si, range);
    cx.constraints(out, si, range);
    out.add(&cx.sid(si), ["props.protection"]);
    out.add("workbook", ["props.defaultStyle"]);
    if !matches!(command, "home.unmergeCells" | "home.mergeAcross") {
        cx.entry(out, si, range, false);
        out.add(&cx.sid(si), ["features.autofilter"]);
    }
    cx.shared(out, si, None, false);
}

fn clear(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let range = cx.range(params);
    if matches!(command, "edit.clearComments" | "edit.clearHyperlinks") {
        let field = if command == "edit.clearComments" { "comments" } else { "hyperlinks" };
        for at in range.iter().take(100_000) {
            out.add(
                &cx.sid(si),
                [format!("{field}.row:{}:col:{}", cx.ids.sheets[si].rows[at.row as usize], cx.ids.sheets[si].cols[at.col as usize])],
            );
        }
    } else if command == "edit.clearContents" {
        cx.entry(out, si, range, false);
    } else {
        style(out, command, params, cx, si);
        if command == "edit.clearFormats" {
            // edit.rs::clear reads only checkbox on the overwritten style.
            // Other old style members are not consulted by this branch.
            let definitions: Vec<_> = out
                .0
                .iter()
                .filter(|fact| fact.object == "workbook" && fact.field.starts_with("styles.") && fact.field.ends_with(".style"))
                .cloned()
                .collect();
            for mut fact in definitions {
                out.0.remove(&fact);
                fact.field.push_str(".checkbox");
                out.0.insert(fact);
            }
        }
        if command == "edit.clearAll" {
            for field in ["comments", "hyperlinks"] {
                for at in range.iter().take(100_000) {
                    out.add(
                        &cx.sid(si),
                        [format!("{field}.row:{}:col:{}", cx.ids.sheets[si].rows[at.row as usize], cx.ids.sheets[si].cols[at.col as usize])],
                    );
                }
            }
        }
    }
    cx.shared(out, si, Some(range), false);
}

fn fill(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let mut range = cx.range(params);
    match command {
        "edit.fillDown" if range.height() == 1 => range.start.row -= 1,
        "edit.fillRight" if range.width() == 1 => range.start.col -= 1,
        _ => {}
    }
    cx.address(out, si, range);
    let source = match command {
        "edit.fillDown" => RangeRef::new(range.start, CellRef::new(range.start.row, range.end.col)),
        "edit.fillUp" => RangeRef::new(CellRef::new(range.end.row, range.start.col), range.end),
        "edit.fillRight" => RangeRef::new(range.start, CellRef::new(range.end.row, range.start.col)),
        _ => RangeRef::new(CellRef::new(range.start.row, range.end.col), range.end),
    };
    cx.source(out, si, source, false);
    for at in source.iter() {
        out.add(&cx.cell(si, at), ["style", "picture"]);
        let style = cx.wb.sheets[si].cell(at).map(|cell| cell.style).unwrap_or_default();
        out.add("workbook", [format!("styles.{}.style", cx.ids.styles[style.0 as usize])]);
    }
    let mut destination = range;
    match command {
        "edit.fillDown" => destination.start.row += 1,
        "edit.fillUp" => destination.end.row -= 1,
        "edit.fillRight" => destination.start.col += 1,
        _ => destination.end.col -= 1,
    }
    cx.constraints(out, si, destination);
    for at in destination.iter() {
        out.add(&cx.cell(si, at), ["input", "text", "style", "array", "picture"]);
    }
    out.add(&cx.sid(si), ["props.protection"]);
    out.add("workbook", ["props.defaultStyle", "props.dateSystem"]);
    cx.fit(out, si, destination);
    cx.shared(out, si, Some(destination), false);
}

fn lines(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let rows = command.ends_with("Rows") || command.ends_with("RowHeight") || command == "home.rowHeight";
    let field = if rows { "rows" } else { "cols" };
    let range = params.get(field).and_then(Value::as_str).and_then(area).unwrap_or(cx.selected);
    let (mut first, mut last) = if rows { (range.start.row, range.end.row) } else { (range.start.col, range.end.col) };
    if command.starts_with("home.unhide") && first == last {
        first = first.saturating_sub(1);
        last += 1;
    }
    out.window(&cx.sid(si), field, first as usize, last as usize + 1);
    for i in first..=last {
        let id = cx.axis(si, rows, i);
        out.position(&id, &["index"]);
        let fields: &[&str] = match command {
            "home.rowHeight" => &["info.size", "info.hidden", "info.custom"],
            "home.columnWidth" => &["info.size", "info.hidden"],
            "home.hideRows" | "home.hideColumns" => &["info.hidden"],
            "home.unhideRows" | "home.unhideColumns" => &["info.hidden", "info.size"],
            _ => &["info", "~layout"],
        };
        out.add(&id, fields.iter().copied());
    }
    if command.starts_with("home.autofit") {
        out.add("workbook", ["props.defaultStyle", "props.dateSystem"]);
        out.add(&cx.sid(si), ["props.protection"]);
    }
    cx.shared(out, si, None, command.contains("hide"));
}

fn structure(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let rows = command.ends_with("Rows");
    let field = if rows { "rows" } else { "cols" };
    let range = params.get(field).or_else(|| params.get("range")).and_then(Value::as_str).and_then(area).unwrap_or(cx.selected);
    let (first, last) = if rows { (range.start.row, range.end.row) } else { (range.start.col, range.end.col) };
    let insert = command.contains("insert");
    let after = params.get(if rows { "below" } else { "right" }).and_then(Value::as_bool) == Some(true);
    let at = if after { last + 1 } else { first };
    out.window(&cx.sid(si), field, at as usize, if insert { at } else { last + 1 } as usize);
    if insert {
        let source = if after { Some(at) } else { at.checked_sub(1) };
        if let Some(source) = source {
            out.add(&cx.axis(si, rows, source), ["~insertFormat"]);
            out.position(&cx.axis(si, rows, source), &["index"]);
            out.window(&cx.sid(si), field, source as usize, source as usize + 1);
        }
        for neighbor in [at.checked_sub(1), Some(at)].into_iter().flatten() {
            out.add(&cx.axis(si, rows, neighbor), ["~outline"]);
            out.position(&cx.axis(si, rows, neighbor), &["index"]);
        }
        if let Some(before) = at.checked_sub(1) {
            out.add(&cx.axis(si, rows, before), ["~insertBoundary"]);
        }
    }
    out.add(&cx.sid(si), ["props.protection"]);
    cx.shared(out, si, None, true);
}

fn sheets(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let sid = cx.sid(si);
    let copy = command == "sheet.move" && params.get("copy").and_then(Value::as_bool) == Some(true);
    if matches!(command, "home.insertSheet" | "home.deleteSheet" | "sheet.move") {
        out.add("workbook", ["props.protectedStructure"]);
    }
    if matches!(command, "home.insertSheet" | "sheet.rename") || copy {
        out.add("workbook", ["~tabs"]);
        for i in 0..cx.wb.sheets.len() {
            out.add(&cx.sid(i), ["name"]);
        }
    }
    if matches!(command, "home.deleteSheet" | "sheet.hide") {
        out.add("workbook", ["~tabs"]);
        for i in 0..cx.wb.sheets.len() {
            out.add(&cx.sid(i), ["props.visibility"]);
        }
    }
    if copy || command == "home.deleteSheet" {
        out.add(&sid, ["*"]);
        out.position(&sid, &["*"]);
        for (id, (owner, _)) in &cx.ownership {
            if *owner == sid {
                out.add(id, ["*"]);
                if copy {
                    out.position(id, &["*"]);
                }
            }
        }
        for (at, _) in cx.wb.sheets[si].cells.iter() {
            let id = cx.cell(si, at);
            out.add(&id, ["*"]);
            if copy {
                out.position(&id, &["*"]);
            }
        }
    }
    if command == "sheet.rename" {
        out.add("workbook", [format!("~sheetReferences.{}", cx.wb.sheets[si].sheet_id)]);
    }
    if copy {
        out.add("workbook", ["~nextObjectId"]);
    }
    if command == "sheet.move" {
        out.position(&sid, &["sheets"]);
        let position = params["to"].as_u64().unwrap() as usize;
        out.window("workbook", "sheets", position, position);
    }
    match command {
        "sheet.tabColor" => out.add(&sid, ["props.tabColor"]),
        "sheet.hide" | "sheet.unhide" => out.add(&sid, ["props.visibility"]),
        _ => {}
    }
    if command == "sheet.unhide" && params.as_object().unwrap().is_empty() {
        return;
    }
    if command == "home.insertSheet" {
        out.add("workbook", ["~headerSync"]);
        let at = params.get("before").and_then(Value::as_u64).map_or(cx.active + 1, |at| (at as usize).min(cx.wb.sheets.len()));
        out.window("workbook", "sheets", at, at);
    } else {
        cx.shared(out, si, None, matches!(command, "home.deleteSheet" | "sheet.rename" | "sheet.move"));
    }
}

fn paste(out: &mut Facts, _command: &str, params: &Value, cx: &Context, si: usize) {
    let range = cx.range(params);
    if let Some(text) = params.get("text").and_then(Value::as_str) {
        let values = kition_sheet_engine::cmd::edit::parse_tsv(text);
        let end = range
            .start
            .offset(values.len().saturating_sub(1) as i64, values.iter().map(Vec::len).max().unwrap_or(1).saturating_sub(1) as i64)
            .unwrap();
        cx.entry(out, si, RangeRef::new(range.start, end), true);
    } else {
        let destination = RangeRef::new(range.start, range.start.offset(1, 1).unwrap());
        cx.entry(out, si, destination, false);
        if params.get("what").and_then(Value::as_str) == Some("columnWidths") {
            for c in destination.start.col..=destination.end.col {
                out.add(&cx.axis(si, false, c), ["info"]);
            }
        }
    }
}

fn complex(out: &mut Facts, command: &str, params: &Value, cx: &Context, si: usize) {
    let range = cx.range(params);
    match command {
        "edit.formatPainter" if params.get("cancel").is_some() => {}
        "edit.formatPainter" => {
            let target = params.get("apply").and_then(Value::as_str).and_then(area).unwrap_or(cx.selected);
            cx.entry(out, si, target, false);
            out.add("workbook", ["theme", "cellStyles"]);
        }
        "edit.replace" => {
            let all = params.get("all").and_then(Value::as_bool).unwrap_or(true);
            let sheets: Vec<_> =
                if params.get("within").and_then(Value::as_str) == Some("workbook") { (0..cx.wb.sheets.len()).collect() } else { vec![cx.active] };
            for owner in sheets {
                for (at, cell) in cx.wb.sheets[owner].cells.iter() {
                    if !all && at != cx.active_cell {
                        continue;
                    }
                    out.add(&cx.cell(owner, at), ["input"]);
                    if cell.input_text().contains(params["what"].as_str().unwrap()) {
                        cx.entry(out, owner, RangeRef::cell(at), true);
                    }
                }
            }
        }
        "edit.autoFill" => {
            let source = params.get("source").and_then(Value::as_str).and_then(area).unwrap_or(cx.selected);
            let target = params.get("target").and_then(Value::as_str).and_then(area).unwrap();
            cx.source(out, si, source, true);
            cx.entry(out, si, target, false);
            out.add("workbook", ["customLists", "props.dateSystem"]);
        }
        "edit.fillSeries" => {
            cx.source(out, si, RangeRef::cell(range.start), true);
            cx.entry(out, si, range, false);
            out.add("workbook", ["props.dateSystem"]);
        }
        "edit.smartFill" => {
            let block = RangeRef::parse("B1:C4").unwrap();
            cx.source(out, si, block, true);
            cx.entry(out, si, block, false);
        }
        "edit.moveCells" => {
            let source = params.get("from").and_then(Value::as_str).and_then(area).unwrap_or(cx.selected);
            let target = params.get("to").and_then(Value::as_str).and_then(area).unwrap();
            let source = if source.count() > 100_000 { cx.wb.sheets[si].used_range().unwrap() } else { source };
            cx.source(out, si, source, true);
            cx.entry(
                out,
                si,
                RangeRef::new(target.start, target.start.offset(source.height() as i64 - 1, source.width() as i64 - 1).unwrap()),
                false,
            );
        }
        "home.insertCells" | "home.deleteCells" => {
            cx.entry(out, si, range, false);
            out.add("workbook", ["names"]);
            cx.shared(out, si, None, true);
        }
        _ => panic!("unprojected complex edit {command}"),
    }
}
