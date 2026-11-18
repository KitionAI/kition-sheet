//! Read-only commands for agents: inspect the workbook, read ranges, cell details.

use kition_sheet_core::{CellRef, RangeRef, Value};
use serde_json::{Value as Json, json};

use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "document.inspect", "Inspect Workbook", [], None, "{} → sheets, selection, names, tables, charts, dirty, history", has_doc, inspect),
        cmd!(query "sheet.read", "Read Range", [], None, "{range?: \"A1:D10\" (default: used range), sheet?, formulas?: bool, formatted?: bool} → rows of values", has_doc, read),
        cmd!(query "cell.get", "Get Cell", [], None, "{cell?, sheet?} → value, formula, display text, style, comment, link", has_doc, cell_get),
        cmd!(query "selection.stats", "Selection Statistics", [], None, "{} → count, sum, average, min, max", has_doc, sel_stats),
        cmd!(query "history.list", "Undo History", [], None, "{} → undo/redo labels", has_doc, history),
        cmd!(query "app.commands", "List Commands", [], None, "{search?} → ids, labels, params", always, list_commands),
    ]
}

pub fn value_json(v: &Value) -> Json {
    match v {
        Value::Empty => Json::Null,
        Value::Number(n) => json!(n),
        Value::Text(t) => json!(t.as_ref()),
        Value::Bool(b) => json!(b),
        Value::Error(e) => json!({"error": e.as_str()}),
        Value::Array(a) => {
            let rows: Vec<Json> =
                (0..a.rows).map(|r| Json::Array((0..a.cols).map(|c| value_json(&a.get(r, c).cloned().unwrap_or_default())).collect())).collect();
            Json::Array(rows)
        }
    }
}

fn inspect(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc()?;
    let wb = &d.wb;
    let sheets: Vec<Json> = wb
        .sheets
        .iter()
        .map(|sh| {
            json!({
                "name": sh.name,
                "visibility": format!("{:?}", sh.visibility),
                "usedRange": sh.used_range().map(|r| r.a1()),
                "cells": sh.cells.len(),
                "freeze": sh.freeze,
                "merges": sh.merges.iter().map(|m| m.a1()).collect::<Vec<_>>(),
                "tables": sh.tables.iter().map(|t| json!({"name": t.name, "range": t.range.a1(), "style": t.style, "totals": t.totals_row, "columns": t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                "charts": sh.charts.iter().map(|c| json!({"id": c.id, "kind": format!("{:?}", c.kind), "title": c.title, "at": c.anchor.cell.a1(), "series": c.series.len(), "values": c.series.iter().map(|x| x.values.clone()).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                "images": sh.images.len(),
                "shapes": sh.shapes.len(),
                "conditionalFormats": sh.cond_formats.len(),
                "validations": sh.validations.len(),
                "comments": sh.comments.len(),
                "sparklines": sh.sparklines.iter().map(|sp| {
                    let (owner, body) = split_sheet(&sp.source);
                    let source = if owner.as_deref().is_some_and(|name| name.eq_ignore_ascii_case(&sh.name)) { body } else { &sp.source };
                    json!({"cell": sp.cell.a1(), "source": source, "type": match sp.kind {
                        kition_sheet_model::SparklineKind::Line => "line",
                        kition_sheet_model::SparklineKind::Column => "column",
                        kition_sheet_model::SparklineKind::WinLoss => "winLoss",
                    }})
                }).collect::<Vec<_>>(),
                "autofilter": sh.autofilter.as_ref().map(|a| a.range.a1()),
                // The filter's columns with a condition (offsets in its range).
                "filteredColumns": sh.autofilter.as_ref().map(|a| a.criteria.iter().map(|(c, _)| *c).collect::<Vec<_>>()).unwrap_or_default(),
                // Hidden rows (numbers) and columns (letters), the first 1000 of each.
                "hiddenRows": sh.rows.iter().filter(|(_, i)| i.hidden).take(1000).map(|(r, _)| r + 1).collect::<Vec<_>>(),
                "hiddenColumns": sh.cols.iter().filter(|(_, i)| i.hidden).take(1000).map(|(c, _)| kition_sheet_core::col_to_letters(*c)).collect::<Vec<_>>(),
                // Where each conditional format rule and validation rule applies (`A1:A10 C2`).
                "conditionalFormatRanges": sh.cond_formats.iter().map(|cf| cf.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>(),
                "validationRanges": sh.validations.iter().map(|v| v.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>(),
                "protected": sh.protection.is_some(),
                "tabColor": sh.tab_color.and_then(|c| c.hex(&wb.theme)),
                "zoom": sh.zoom,
                "gridlines": sh.show_gridlines,
                "pivots": sh.pivots.len(),
                // Form controls kept from the file: their kind and caption.
                "formControls": sh.form_controls.iter().take(1000).map(|c| json!({"kind": c.kind, "text": c.text, "checked": c.checked})).collect::<Vec<_>>(),
                // Rows and columns with a size of their own (px; the first 1000 of each), and the defaults.
                "rowHeights": sh.rows.iter().filter_map(|(r, i)| i.size.map(|h| ((r + 1).to_string(), json!(h)))).take(1000).collect::<serde_json::Map<String, Json>>(),
                "colWidths": sh.cols.iter().filter_map(|(c, i)| i.size.map(|w| (kition_sheet_core::col_to_letters(*c), json!(w)))).take(1000).collect::<serde_json::Map<String, Json>>(),
                "defaultRowHeight": sh.default_row_height,
                "defaultColWidth": sh.default_col_width,
            })
        })
        .collect();
    Ok(json!({
        "title": d.display_title(),
        "path": d.path,
        "dirty": d.is_dirty(),
        "activeSheet": wb.active_sheet,
        "sheets": sheets,
        "selection": d.selection.a1(),
        "activeCell": d.selection.active.a1(),
        "names": wb.names.iter().map(|n| json!({"name": n.name, "refersTo": format!("={}", n.formula), "scope": n.scope})).collect::<Vec<_>>(),
        "calcMode": format!("{:?}", wb.calc.mode),
        "undo": d.undo.iter().rev().take(20).map(|e| e.label.clone()).collect::<Vec<_>>(),
        "redo": d.redo.iter().rev().take(20).map(|e| e.label.clone()).collect::<Vec<_>>(),
        "theme": wb.theme.name,
        "mode": format!("{:?}", s.mode),
    }))
}

fn read(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let d = s.doc()?;
    let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    let r = match str_param(p, "range") {
        Some(_) => target_range(s, p)?,
        None => match sh.used_range() {
            Some(r) => r,
            None => return Ok(json!({"range": null, "values": []})),
        },
    };
    let r = match sh.used_range() {
        Some(u) if r.is_full_cols() || r.is_full_rows() => r.intersection(&u).unwrap_or(RangeRef::cell(r.start)),
        _ => r,
    };
    if r.count() > 1_000_000 {
        return Err(bad("sheet.read", "range larger than 1,000,000 cells; read it in parts"));
    }
    let formulas = bool_param(p, "formulas").unwrap_or(false);
    let formatted = bool_param(p, "formatted").unwrap_or(false);
    let mut rows = Vec::with_capacity(r.height() as usize);
    for row in r.start.row..=r.end.row {
        let mut cols = Vec::with_capacity(r.width() as usize);
        for col in r.start.col..=r.end.col {
            let c = CellRef::new(row, col);
            let v = if formulas && let Some(f) = sh.cell(c).and_then(|x| x.formula.as_ref()) {
                json!(format!("={}", f.text))
            } else if formatted {
                json!(crate::display::cell_text(&d.wb, sh, c))
            } else {
                value_json(&sh.value(c))
            };
            cols.push(v);
        }
        rows.push(Json::Array(cols));
    }
    Ok(json!({"range": r.a1(), "sheet": sh.name, "values": rows}))
}

fn cell_get(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let d = s.doc()?;
    let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    let c = cell_param(p, "cell").unwrap_or(d.selection.active);
    let cell = sh.cell(c);
    let style = d.wb.styles.get(sh.style_id(c));
    Ok(json!({
        "cell": c.a1(),
        "value": value_json(&sh.value(c)),
        "type": if sh.cell_pictures.contains_key(&c) { "picture" } else { match sh.value(c) { Value::Empty => "empty", Value::Number(_) => "number", Value::Text(_) => "text", Value::Bool(_) => "boolean", Value::Error(_) => "error", Value::Array(_) => "array" } },
        "formula": cell.and_then(|x| x.formula.as_ref()).map(|f| format!("={}", f.text)),
        "text": crate::display::cell_text(&d.wb, sh, c),
        "input": sh.input_text(c),
        // A text cell's rich text runs (`cell.set` takes them back).
        "runs": cell.and_then(kition_sheet_model::Cell::runs).map(|r| &r.runs),
        "picture": sh.cell_pictures.get(&c).map(|picture| json!({"mime": picture.mime, "alt": picture.alt, "bytes": picture.data.len()})),
        "spilledFrom": sh.spill_ranges.iter().find(|(a, r)| **a != c && r.contains(c)).map(|(a, _)| a.a1()),
        "spill": sh.spill_ranges.get(&c).map(|r| r.a1()),
        "style": style,
        "merge": sh.merge_at(c).map(|m| m.a1()),
        "comment": sh.comments.get(&c),
        "hyperlink": sh.hyperlinks.get(&c),
    }))
}

fn sel_stats(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let st = crate::display::stats(sh, &d.selection.ranges);
    serde_json::to_value(st).map_err(|e| EngineError::Other(e.to_string()))
}

fn history(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc()?;
    Ok(
        json!({"undo": d.undo.iter().rev().map(|e| e.label.clone()).collect::<Vec<_>>(), "redo": d.redo.iter().rev().map(|e| e.label.clone()).collect::<Vec<_>>()}),
    )
}

fn list_commands(s: &mut Session, p: &Json) -> Result<Json> {
    let q = str_param(p, "search").map(str::to_ascii_lowercase);
    let v: Vec<Json> = s
        .commands()
        .into_iter()
        .filter(|c| q.as_ref().is_none_or(|q| c.id.to_ascii_lowercase().contains(q.as_str()) || c.label.to_ascii_lowercase().contains(q.as_str())))
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    Ok(Json::Array(v))
}
