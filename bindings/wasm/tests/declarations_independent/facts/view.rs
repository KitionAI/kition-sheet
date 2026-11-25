//! Full stored-fact projection of view_file's retained rows, using their actual
//! path fixture. Binding routing is applied before the engine handler runs.
use super::model::{Context, Facts, area};
use crate::paths::Reading;
use kition_sheet_engine::core::{CellRef, RangeRef};
use serde_json::Value;

pub(super) fn expected(row: &Reading, params: &Value, cx: &Context) -> Facts {
    let mut out = Facts::default();
    let si = if row.command.starts_with("view.") { cx.active } else { cx.sheet(params) };
    let sid = cx.sid(si);
    if row.holds == "whole workbook" {
        cx.whole(&mut out);
        return out;
    }
    if row.holds == "none" {
        return out;
    }
    if let Some(rest) = row.holds.strip_prefix("root{").or_else(|| row.holds.strip_prefix("sheet{")) {
        let object = if row.holds.starts_with("root{") { "workbook" } else { &sid };
        let fields = rest.split('}').next().unwrap();
        if row.command != "file.properties" || !params.as_object().unwrap().is_empty() {
            out.add(object, fields.split(',').map(|field| field.replace("props.properties", "props.docProps")));
        } else {
            out.add(object, Vec::<String>::new());
        }
    }
    if !row.command.starts_with("view.") {
        cx.lookup(&mut out, params, si);
    }
    let address = |out: &mut Facts, range| cx.address(out, si, range);
    let at = |key: &str| params.get(key).and_then(Value::as_str).and_then(area).map(|range| range.start);
    match row.command {
        "sheet.protect" | "sheet.unprotect" => out.add(&sid, ["~editConnectors"]),
        "view.freezePanes" | "view.freezeTopRow" | "view.freezeFirstColumn" | "view.freezeRows" | "view.freezeColumns" => {
            if row.command == "view.freezePanes" && params.get("cell").is_none() && cx.wb.sheets[si].freeze.is_some() {
                return out;
            }
            let point = at("cell").unwrap_or(cx.selected.start);
            let origin = at("top").unwrap_or_default();
            address(&mut out, RangeRef::cell(origin));
            if matches!(row.command, "view.freezePanes" | "view.freezeRows") {
                out.window(&sid, "rows", point.row as usize, point.row as usize + 1);
                out.position(&cx.axis(si, true, point.row), &["index"]);
            }
            if matches!(row.command, "view.freezePanes" | "view.freezeColumns") {
                out.window(&sid, "cols", point.col as usize, point.col as usize + 1);
                out.position(&cx.axis(si, false, point.col), &["index"]);
            }
        }
        "page.printArea" if params.get("clear").and_then(Value::as_bool) != Some(true) => address(&mut out, cx.range(params)),
        "page.printTitles" => {
            for (key, rows) in [("rows", true), ("cols", false)] {
                if let Some(range) = params.get(key).and_then(Value::as_str).and_then(area) {
                    let (first, last) = if rows { (range.start.row, range.end.row) } else { (range.start.col, range.end.col) };
                    out.window(&sid, key, first as usize, last as usize + 1);
                    for i in first..=last {
                        out.position(&cx.axis(si, rows, i), &["index"]);
                    }
                }
            }
        }
        "page.breaks" if params.get("reset").and_then(Value::as_bool) != Some(true) => {
            address(&mut out, RangeRef::cell(at("cell").unwrap_or(cx.selected.start)))
        }
        "page.pageSetup" => {
            for (key, rows) in [("titleRows", true), ("titleCols", false)] {
                if let Some(range) = params.get(key).and_then(Value::as_str).and_then(area) {
                    let (first, last, field) = if rows { (range.start.row, range.end.row, "rows") } else { (range.start.col, range.end.col, "cols") };
                    out.window(&sid, field, first as usize, last as usize + 1);
                    for i in first..=last {
                        out.position(&cx.axis(si, rows, i), &["index"]);
                    }
                }
            }
            for (key, rows) in [("rowBreaks", true), ("colBreaks", false)] {
                for i in params.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).map(|i| i as u32 - 1) {
                    out.window(&sid, if rows { "rows" } else { "cols" }, i as usize, i as usize + 1);
                    out.position(&cx.axis(si, rows, i), &["index"]);
                }
            }
            if let Some(range) = params.get("printArea").and_then(Value::as_str).and_then(area) {
                address(&mut out, range);
            }
        }
        "spelling.change" => spelling(&mut out, params, cx, si),
        _ => {}
    }
    out
}

fn spelling(out: &mut Facts, params: &Value, cx: &Context, si: usize) {
    let all = params.get("all").and_then(Value::as_bool) == Some(true);
    let word = params["word"].as_str().unwrap();
    let sid = cx.sid(si);
    let points: Vec<_> = if all {
        // edit.rs::replace iterates stored input strings, then set_cell/touch.
        // Unlike cell_set it never calls check_array/validation/merge lookup.
        // The original prose's broad "constraints" term is retained, but its
        // executable projection is the header/protection tail actually read.
        out.add(&sid, Vec::<String>::new());
        out.fact("call", "covers", "input");
        out.fact("call", "covers", "~spellingInputs");
        cx.wb.sheets[si].cells.iter().filter(|(_, cell)| cell.input_text().contains(word)).map(|(at, _)| at).collect()
    } else {
        vec![params.get("cell").and_then(Value::as_str).and_then(area).map(|range| range.start).unwrap_or(CellRef::new(2, 2))]
    };
    for at in points {
        cx.address(out, si, RangeRef::cell(at));
        out.add(&cx.cell(si, at), ["input", "text", "style", "array", "picture"]);
        if all {
            out.add(&cx.axis(si, true, at.row), ["~spellingHeader"]);
        } else {
            out.add(&sid, kdm_model::constraint_fields(RangeRef::cell(at)));
        }
        out.add(&sid, ["props.protection", "props.defaultRowHeight", "~editConnectors"]);
        out.add("workbook", ["props.defaultStyle", "props.dateSystem"]);
        out.add(&cx.axis(si, true, at.row), ["info.style", "info.size", "info.custom", "~fit"]);
        out.add(&cx.axis(si, false, at.col), ["info.style"]);
    }
}
