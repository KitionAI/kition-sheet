//! Cell entry, selection, undo/redo, clipboard, clear, fill, find & replace.

use kition_sheet_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value};
use kition_sheet_model::{Cell, Formula, StyleId};
use serde_json::{Value as Json, json};

use super::*;
use crate::selection::{current_region, jump};
use crate::{Clipboard, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "cell.set",
            "Enter Cell",
            [],
            None,
            "{cell?: \"B2\", name?: \"static single-cell defined name, instead of cell\", input: \"text, number or =formula\", sheet?, array?: bool, runs?: [{text, font}] (a text's rich text runs, as cell.get answers them), expand?: bool (an entry right under or right of a table makes it larger; default true, false for what Insert put in: a function, a symbol), widen?: bool (as typing does: a column of the default width widens so the date or number entered shows in full instead of #### or scientific notation; default false)}",
            has_doc,
            cell_set
        ),
        cmd!(
            "cell.formatText",
            "Format Characters",
            [],
            None,
            "{cell?: \"B2\", sheet?, start?: 0, end?: (characters, end exclusive; the whole text by default), bold?, italic?, strike?, underline?: true|\"double\"|\"none\", vertAlign?: superscript|subscript|baseline, size?, name?, color?: \"#C00000\"|{theme, tint}|\"auto\", toggle?: bold|italic|underline|strike|superscript|subscript} → {cell, start, end, runs}: formats part of a text cell's characters (rich text)",
            has_doc,
            cell_format_text
        ),
        cmd!(
            "range.setValues",
            "Set Values",
            [],
            None,
            "{range?: \"A1\", values: [[...]] (rows of numbers/strings/bools/null; strings starting with = are formulas)}",
            has_doc,
            range_set_values
        ),
        cmd!(
            "range.fill",
            "Fill Range With Input",
            [],
            Some("Cmd+Enter"),
            "{range?, input}: enters the same input in every cell of the range (default: the selection); a formula is written for the range's first cell (for the active cell when the selection is filled, like Cmd+Enter) and its relative references move per cell",
            has_doc,
            range_fill
        ),
        cmd!(noundo "select.set", "Select", [], None, "{range: \"A1:B2,D4\" | cell, active?: \"A1\", sheet?}", has_doc, selection_set),
        cmd!(noundo "select.move", "Move Selection", [], None, "{dr, dc, extend?: bool, jump?: bool (Ctrl+arrow), page?: bool, to?: rowStart|sheetStart|lastCell (Home, Ctrl+Home, Ctrl+End: instead of dr and dc)}", has_doc, selection_move),
        cmd!(noundo "select.next", "Next Cell in Selection", [], None, "{forward?: true, byRow?: false} (Enter/Tab inside a selection)", has_doc, selection_next),
        cmd!(noundo "edit.selectAll", "Select All", ["Edit"], Some("Cmd+A"), "{} (current region first, then the whole sheet)", has_doc, select_all),
        cmd!(noundo "select.currentRegion", "Select Current Region", [], Some("Ctrl+Shift+8"), "{}", has_doc, select_region),
        cmd!(noundo "select.row", "Select Entire Row", [], Some("Shift+Space"), "{}", has_doc, select_row),
        cmd!(noundo "select.column", "Select Entire Column", [], Some("Ctrl+Space"), "{}", has_doc, select_col),
        cmd!(noundo "edit.goTo", "Go To…", ["Home", "Editing", "Find & Select"], Some("Ctrl+G"), "{reference: \"B5\" | \"Sheet2!A1:C3\" | name}", has_doc, go_to),
        cmd!(noundo "edit.goToSpecial", "Go To Special…", ["Home", "Editing", "Find & Select"], None, "{kind: blanks|constants|formulas|comments|lastCell|currentRegion|visible|conditionalFormats|dataValidation|errors|numbers|text}", has_doc, go_to_special),
        cmd!(noundo "edit.undo", "Undo", ["Edit"], Some("Cmd+Z"), r#"{steps?: 1, scope?: "person"|"agent"}"#, can_undo, undo),
        cmd!(noundo "edit.redo", "Redo", ["Edit"], Some("Cmd+Y"), r#"{steps?: 1, scope?: "person"|"agent"}"#, can_redo, redo),
        cmd!(noundo "edit.copy", "Copy", ["Home", "Clipboard"], Some("Cmd+C"), "{range?, html?: bool} → {range, text, html?} (html: a styled HTML table, only when requested and small enough)", has_doc, copy),
        cmd!(noundo "edit.cut", "Cut", ["Home", "Clipboard"], Some("Cmd+X"), "{range?, html?: bool} → {range, text, html?} (html: a styled HTML table, only when requested and small enough)", has_doc, cut),
        cmd!(
            "edit.paste",
            "Paste",
            ["Home", "Clipboard"],
            Some("Cmd+V"),
            "{at?: \"C3\", text?: \"tab-separated text from the system clipboard\", html?: \"the HTML beside it, whose first table (merged cells, line breaks in a cell, formatting, column widths) is pasted instead\"}",
            has_doc,
            paste
        ),
        cmd!(
            "edit.pasteSpecial",
            "Paste Special…",
            ["Home", "Clipboard"],
            Some("Ctrl+Cmd+V"),
            "{what: all|formulas|values|formats|comments|validation|allExceptBorders|columnWidths|formulasAndNumberFormats|valuesAndNumberFormats, operation?: none|add|subtract|multiply|divide, skipBlanks?, transpose?, link?}",
            has_clipboard,
            paste_special
        ),
        cmd!(noundo "edit.clearClipboard", "Cancel Copy", [], Some("Escape"), "{}", has_doc, clear_clipboard),
        cmd!("edit.clearAll", "Clear All", ["Home", "Editing", "Clear"], None, "{range?}", has_doc, |s, p| clear(s, p, "all")),
        cmd!("edit.clearFormats", "Clear Formats", ["Home", "Editing", "Clear"], None, "{range?}", has_doc, |s, p| clear(s, p, "formats")),
        cmd!("edit.clearContents", "Clear Contents", ["Home", "Editing", "Clear"], Some("Delete"), "{range?}", has_doc, |s, p| clear(
            s, p, "contents"
        )),
        cmd!("edit.clearComments", "Clear Comments and Notes", ["Home", "Editing", "Clear"], None, "{range?}", has_doc, |s, p| clear(
            s, p, "comments"
        )),
        cmd!("edit.clearHyperlinks", "Clear Hyperlinks", ["Home", "Editing", "Clear"], None, "{range?}", has_doc, |s, p| clear(s, p, "hyperlinks")),
        cmd!("edit.fillDown", "Fill Down", ["Home", "Editing", "Fill"], Some("Cmd+D"), "{range?}", has_doc, |s, p| fill_dir(s, p, 1, 0)),
        cmd!("edit.fillRight", "Fill Right", ["Home", "Editing", "Fill"], Some("Cmd+R"), "{range?}", has_doc, |s, p| fill_dir(s, p, 0, 1)),
        cmd!("edit.fillUp", "Fill Up", ["Home", "Editing", "Fill"], None, "{range?}", has_doc, |s, p| fill_dir(s, p, -1, 0)),
        cmd!("edit.fillLeft", "Fill Left", ["Home", "Editing", "Fill"], None, "{range?}", has_doc, |s, p| fill_dir(s, p, 0, -1)),
        cmd!(
            "edit.autoFill",
            "AutoFill",
            [],
            None,
            "{source: \"A1:A2\", target: \"A1:A10\", mode?: series|extend|copy|formats|values}: the fill-handle drag; extend also counts a single number on",
            has_doc,
            auto_fill
        ),
        cmd!(
            "edit.fillSeries",
            "Series…",
            ["Home", "Editing", "Fill"],
            None,
            "{range?, direction?: columns|rows, type?: linear|growth|date|autofill, step?: 1, stop?, dateUnit?: day|weekday|month|year}",
            has_doc,
            fill_series
        ),
        cmd!("edit.smartFill", "Smart fill", ["Home", "Editing", "Fill"], None, "{range?}", has_doc, smart_fill),
        cmd!(noundo "edit.find", "Find…", ["Home", "Editing", "Find & Select"], Some("Cmd+F"), "{what, matchCase?, wholeCell?, lookIn?: formulas|values, byColumns?, all?: bool, within?: sheet|workbook, backward?: bool}", has_doc, find),
        cmd!(
            "edit.replace",
            "Replace…",
            ["Home", "Editing", "Find & Select"],
            Some("Ctrl+H"),
            "{what, with, matchCase?, wholeCell?, all?: true, within?: sheet|workbook}",
            has_doc,
            replace
        ),
        cmd!(
            "edit.formatPainter",
            "Format Painter",
            ["Home", "Clipboard"],
            None,
            "{sticky?: bool, apply?: \"C3:D4\", cancel?: bool} → {active, sticky}: picks up the selection's formats (sticky: until cancelled, as a double-click on the button does); called again without `sticky` it puts them down, as a second click on the button does; {apply} paints them on a range (whole rows or columns too) on the active sheet; {cancel} puts them down",
            has_doc,
            format_painter
        ),
        cmd!(noundo "edit.beginEdit", "Edit Cell", [], Some("F2"), "{}", has_doc, begin_edit),
        cmd!(
            "edit.moveCells",
            "Move Cells",
            [],
            None,
            "{from?: \"B2:C4\" (default: the selection; whole rows \"3:4\" or columns \"C:D\" move as rows or columns), to: \"E5\" (the block's new top-left cell; a row or a column for whole lines), copy?: bool (a copy, as a drag with Cmd/Ctrl held), insert?: \"down\"|\"right\" (the cells at `to` make room, as a drag with Shift held: down moves them down, right moves them right; whole rows and columns always make room as rows or columns)} → {range}: a drag of the selection's border (WPS, Excel): the cells move with their values, formulas, formats, merged areas, notes, links, conditional formats, validation and tables, and formulas anywhere that refer to them follow",
            has_doc,
            move_cells
        ),
    ]
}

// ---------------------------------------------------------------- entry

/// What typing `input` into a cell produces (constant or formula, plus an automatic number
/// format). Inherits the destination's cell, row or column style, including its number
/// format, before parsing. Errors when the formula can't be parsed.
pub fn input_to_cell(input: &str, sheet: usize, at: CellRef, wb: &mut kition_sheet_model::Workbook) -> Result<Option<Cell>> {
    let mut style = wb.sheet(sheet).map(|sh| sh.style_id(at)).unwrap_or_default();
    // A checkbox cell takes TRUE or FALSE in any case (Find and Replace, an agent, a paste) and
    // stays a checkbox; anything else replaces the checkbox.
    if wb.styles.get(style).checkbox {
        for (word, b) in [("true", true), ("false", false)] {
            if input.trim().eq_ignore_ascii_case(word) {
                return Ok(Some(Cell { value: Value::Bool(b), formula: None, style, rich: None }));
            }
        }
        style = wb.styles.with_checkbox(style, false);
    }
    if input.is_empty() {
        let c = Cell { value: Value::Empty, formula: None, style, rich: None };
        return Ok(if c.is_blank() { None } else { Some(c) });
    }
    let fmt_is_text = wb.styles.get(style).num_fmt.as_str() == "@";
    if fmt_is_text {
        return Ok(Some(Cell { value: Value::text(input), formula: None, style, rich: None }));
    }
    let is_formula = input.starts_with('=')
        || (input.len() > 1
            && (input.starts_with('+') || input.starts_with('-'))
            && kition_sheet_core::parse::parse_number_text(input).is_none()
            && input.chars().nth(1).is_some_and(|c| c.is_ascii_alphabetic() || c == '('));
    if is_formula {
        let body = input.strip_prefix('=').unwrap_or(input);
        // Excel closes missing parentheses for you.
        let mut text = body.to_string();
        let mut parsed = kition_sheet_formula::parse(&text);
        // When closing parentheses does not help, the reason is the formula's own (`=1+`: its end),
        // not that of the last parenthesis tried.
        let mut reason = None;
        for _ in 0..8 {
            if parsed.is_ok() {
                break;
            }
            if reason.is_none() {
                reason = parsed.as_ref().err().map(ToString::to_string);
            }
            text.push(')');
            parsed = kition_sheet_formula::parse(&text);
        }
        let expr =
            parsed.map_err(|e| EngineError::Other(format!("There's a problem with this formula: {}", reason.unwrap_or_else(|| e.to_string()))))?;
        let expr = wb.name_call_case(expr);
        let mut cell = Cell::formula(Formula::from_expr(expr));
        cell.style = style;
        // Formulas whose result is a date/time get a format from functions like TODAY().
        if wb.styles.get(style).num_fmt.as_str() == "General" {
            let f = cell.formula.as_ref().map(|f| f.text.to_ascii_uppercase()).unwrap_or_default();
            let auto = if f.starts_with("TODAY(")
                || f.starts_with("DATE(")
                || f.starts_with("EDATE(")
                || f.starts_with("EOMONTH(")
                || f.starts_with("WORKDAY(")
            {
                Some("m/d/yyyy")
            } else if f.starts_with("NOW(") {
                Some("m/d/yyyy h:mm")
            } else if f.starts_with("TIME(") {
                Some("h:mm AM/PM")
            } else {
                None
            };
            if let Some(code) = auto {
                cell.style = wb.styles.derive(style, |s| s.num_fmt = kition_sheet_model::NumFmt::new(code));
            }
        }
        return Ok(Some(cell));
    }
    let parsed = kition_sheet_core::parse::parse_input(input, wb.date_system);
    if let Some(code) = parsed.format {
        let cur = wb.styles.get(style).num_fmt.as_str().to_string();
        if cur == "General" {
            style = wb.styles.derive(style, |s| s.num_fmt = kition_sheet_model::NumFmt::new(code));
        }
    }
    // Wrap text automatically when the input has line breaks (Alt+Enter).
    if input.contains('\n') && !wb.styles.get(style).align.wrap {
        style = wb.styles.derive(style, |s| s.align.wrap = true);
    }
    Ok(Some(Cell { value: parsed.value, formula: None, style, rich: None }))
}

/// `new` (what an edit of a cell's text makes) with the runs of the rich text cell it replaces
/// (`old`): the characters the edit keeps keep their run, and typed ones take the run at the caret
/// ([`RichText::retext`](kition_sheet_model::RichText::retext)).
/// Refuses a change to a locked cell on a protected sheet.
fn check_unlocked(s: &Session, sheet: usize, at: CellRef) -> Result<()> {
    let protected =
        s.doc()?.wb.sheet(sheet).is_some_and(|sh| sh.is_protected() && s.doc().is_ok_and(|d| d.wb.styles.get(sh.style_id(at)).protection.locked));
    if protected {
        return Err(EngineError::Other("The cell or chart you're trying to change is on a protected sheet.".into()));
    }
    Ok(())
}

/// A new text cell with the runs an editor gave for it (ignored unless they spell its text).
fn with_runs(new: Option<Cell>, runs: &[kition_sheet_model::TextRun]) -> Option<Cell> {
    let mut new = new?;
    if let (None, Value::Text(text)) = (&new.formula, &new.value) {
        new.rich = kition_sheet_model::RichText::new(runs.iter().cloned()).filter(|r| r.spells(text)).map(std::sync::Arc::new);
    }
    Some(new)
}

/// Formats characters of a text cell: `cell.formatText` (see [`crate::text_format`]).
fn cell_format_text(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = match cell_param(p, "cell") {
        Some(c) => c,
        None => s.doc()?.selection.active,
    };
    check_unlocked(s, sheet, at)?;
    let (text, runs, base) = {
        let d = s.doc()?;
        let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
        let cell = sh.cells.get(at).filter(|c| c.formula.is_none());
        let Some((Value::Text(text), cell)) = cell.map(|c| (&c.value, c)) else {
            return Err(EngineError::Other("Only the characters of a text cell can be formatted.".into()));
        };
        (text.to_string(), cell.runs().cloned(), d.wb.styles.get(cell.style).font.clone())
    };
    let len = text.chars().count();
    let index = |key: &str, default: usize| p.get(key).and_then(Json::as_u64).map_or(default, |v| usize::try_from(v).unwrap_or(usize::MAX).min(len));
    let (start, end) = (index("start", 0), index("end", len));
    if start >= end {
        return Err(bad("cell.formatText", "no characters are selected (`start` must be before `end`)"));
    }
    let new = crate::text_format::format_runs(runs.as_ref(), &text, &base, start, end, p)?;
    edit(s, |cx| {
        if let Some(cell) = cx.sheet_mut(sheet)?.cells.get_mut(at) {
            cell.rich = new.clone().map(std::sync::Arc::new);
        }
        cx.touch(sheet, at);
        Ok(Json::Null)
    })?;
    Ok(json!({"cell": at.a1(), "start": start, "end": end, "runs": new.map(|r| r.runs)}))
}

pub(crate) fn keep_runs(old: Option<&Cell>, new: Option<Cell>) -> Option<Cell> {
    let mut new = new?;
    if let (Some(rich), None, Value::Text(text)) = (old.and_then(Cell::runs), &new.formula, &new.value) {
        new.rich = rich.retext(text).map(std::sync::Arc::new);
    }
    Some(new)
}

fn cell_set(s: &mut Session, p: &Json) -> Result<Json> {
    let context = target_sheet(s, p)?;
    let array = bool_param(p, "array").unwrap_or(false);
    let (sheet, at) = if let Some(name) = str_param(p, "name") {
        if p.get("cell").is_some() || array {
            return Err(bad("cell.set", "`name` cannot be combined with `cell` or array entry"));
        }
        let resolved = super::formulas::named_cell(&s.doc()?.wb, context, name)?;
        (resolved.sheet, resolved.at)
    } else {
        let at = if array { s.doc()?.selection.current().start } else { cell_param(p, "cell").unwrap_or(s.doc()?.selection.active) };
        (context, at)
    };
    let input =
        str_param(p, "input").or_else(|| str_param(p, "value")).map(str::to_string).or_else(|| p.get("value").map(json_to_input)).unwrap_or_default();
    let runs = match p.get("runs") {
        None | Some(Json::Null) => None,
        Some(v) => Some(serde_json::from_value::<Vec<kition_sheet_model::TextRun>>(v.clone()).map_err(|e| bad("cell.set", format!("`runs`: {e}")))?),
    };
    if array {
        let range = s.doc()?.selection.current();
        if range.count() > 1_000_000 {
            return Err(bad("cell.set", "array entry exceeds 1000000 cells"));
        }
        for destination in range.iter() {
            check_unlocked(s, sheet, destination)?;
        }
    } else {
        check_unlocked(s, sheet, at)?;
    }
    s.expand_tables = bool_param(p, "expand").unwrap_or(true);
    let widen = bool_param(p, "widen").unwrap_or(false) && !array;
    edit(s, |cx| {
        if array {
            // Legacy Ctrl+Shift+Enter array formula over the selection.
            let range = cx.sel.current();
            let mut cell = input_to_cell(&input, sheet, range.start, &mut cx.wb)?.unwrap_or_default();
            if let Some(f) = cell.formula.as_mut() {
                std::sync::Arc::make_mut(f).array = Some(range);
            }
            let sh = cx.sheet_mut(sheet)?;
            // The array takes the whole range: other cells in it lose their contents.
            for c in range.iter().skip(1).take(1_000_000) {
                if let Some(old) = sh.cells.get(c).filter(|x| !x.value.is_empty() || x.formula.is_some()) {
                    let style = old.style;
                    sh.set_cell(c, Cell { style, ..Cell::default() });
                }
            }
            sh.set_cell(range.start, cell);
            cx.touch(sheet, range.start);
            return Ok(Json::Null);
        }
        let cell = input_to_cell(&input, sheet, at, &mut cx.wb)?;
        let sh = cx.sheet_mut(sheet)?;
        let cell = match &runs {
            Some(runs) => with_runs(cell, runs),
            None => keep_runs(sh.cells.get(at), cell),
        };
        match cell {
            Some(c) => sh.set_cell(at, c),
            None => {
                sh.remove_cell(at);
            }
        }
        cx.touch(sheet, at);
        // Typed, a date or a number too wide for its column widens the column in the same step.
        if widen && let Some(width) = cx.wb.sheet(sheet).and_then(|sh| entry_width(&cx.wb, sh, at)) {
            cx.sheet_mut(sheet)?.cols.entry(at.col).or_default().size = Some(width);
        }
        Ok(Json::Null)
    })?;
    let v = s.doc()?.wb.sheet(sheet).map(|sh| sh.value(at)).unwrap_or_default();
    Ok(json!({"cell": at.a1(), "value": v.display()}))
}

/// The width (pixels) a column of the default width takes so the number just entered at `at`
/// shows in full, as typing a date does in WPS (W-E6) and the whole part of a General number of
/// up to 11 digits in Excel (WPS untested); `None` when it already fits, when the cell holds text or a formula, when
/// the column's width was set (by a person or the file), or when the cell is merged, turned,
/// wrapped or shrunk to fit. Measured as the grid draws a number: the text and the cell's padding.
fn entry_width(wb: &kition_sheet_model::Workbook, sheet: &kition_sheet_model::Sheet, at: CellRef) -> Option<f32> {
    let cell = sheet.cell(at)?;
    if cell.formula.is_some() || sheet.show_formulas {
        return None;
    }
    let Value::Number(n) = cell.value else { return None };
    if sheet.cols.get(&at.col).is_some_and(|line| line.size.is_some() || line.hidden) || sheet.merge_at(at).is_some_and(|m| !m.is_single()) {
        return None;
    }
    let style = wb.styles.get(sheet.style_id(at));
    if style.align.rotation != 0 || style.align.wrap || style.align.shrink {
        return None;
    }
    let text = if style.num_fmt.as_str() == "General" {
        // General shows fewer decimals in a narrow column; only the whole part widens it, and
        // more than 11 digits stay in scientific notation at the column's width (Excel).
        if !n.is_finite() || n.abs() >= 1e11 {
            return None;
        }
        kition_sheet_numfmt::format_general_fit(n.trunc(), 11)?
    } else {
        crate::display::format(&cell.value, style.num_fmt.as_str(), wb).text
    };
    // The grid's padding (2.5 px a side) and a pixel, and a little room for the face drawn.
    let needed = (crate::metrics::text_width(&text, &style.font) + f32::from(style.align.indent) * 9.0 + 9.0).ceil();
    (needed > sheet.col_width(at.col)).then_some(needed.min(1785.0))
}

pub(crate) fn json_to_input(v: &Json) -> String {
    match v {
        Json::Null => String::new(),
        Json::Bool(b) => {
            if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        Json::Number(n) => n.to_string(),
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn range_set_values(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let start = target_range(s, p)?.start;
    let Some(rows) = p.get("values").and_then(Json::as_array) else { return Err(bad("range.setValues", "missing `values` (array of rows)")) };
    if rows.len() > MAX_ROWS as usize {
        return Err(bad("range.setValues", "too many rows"));
    }
    let mut count = 0;
    edit(s, |cx| {
        for (ri, row) in rows.iter().enumerate() {
            let cols: Vec<&Json> = match row.as_array() {
                Some(a) => a.iter().collect(),
                None => vec![row],
            };
            for (ci, v) in cols.iter().enumerate() {
                let Some(at) = start.offset(ri as i64, ci as i64) else { continue };
                let input = json_to_input(v);
                let cell = match v {
                    Json::Number(n) => Some(Cell {
                        value: Value::number(n.as_f64().unwrap_or(0.0)),
                        formula: None,
                        style: cx.wb.sheet(sheet).map(|sh| sh.style_id(at)).unwrap_or_default(),

                        rich: None,
                    }),
                    Json::Bool(b) => Some(Cell {
                        value: Value::Bool(*b),
                        formula: None,
                        style: cx.wb.sheet(sheet).map(|sh| sh.style_id(at)).unwrap_or_default(),
                        rich: None,
                    }),
                    _ => input_to_cell(&input, sheet, at, &mut cx.wb)?,
                };
                let sh = cx.sheet_mut(sheet)?;
                match cell {
                    Some(c) => sh.set_cell(at, c),
                    None => {
                        sh.remove_cell(at);
                    }
                }
                cx.touch(sheet, at);
                count += 1;
            }
        }
        Ok(())
    })?;
    Ok(json!({"cells": count}))
}

fn range_fill(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let ranges = target_ranges(s, p)?;
    let input = str_param(p, "input").unwrap_or("").to_string();
    // The input is written for the range's first cell when a range is given, else (Ctrl+Enter)
    // for the active cell; relative references move from there.
    let given = p.get("range").is_some() || p.get("cell").is_some();
    let origin = match ranges.first() {
        Some(r) if given => r.start,
        _ => s.doc()?.selection.active,
    };
    edit(s, |cx| {
        let base = input_to_cell(&input, sheet, origin, &mut cx.wb)?;
        for r in &ranges {
            if r.count() > 2_000_000 {
                return Err(bad("range.fill", "range too large"));
            }
            for at in r.iter() {
                let mut cell = base.clone().unwrap_or_default();
                if let Some(f) = &cell.formula
                    && let Some(e) = f.expr()
                {
                    let shifted =
                        kition_sheet_formula::adjust::shift_relative(e, at.row as i64 - origin.row as i64, at.col as i64 - origin.col as i64);
                    cell.formula = Some(std::sync::Arc::new(Formula::from_expr(shifted)));
                }
                cell.style = cx.wb.sheet(sheet).and_then(|sh| sh.cell(at)).map(|c| c.style).unwrap_or(cell.style);
                cx.sheet_mut(sheet)?.set_cell(at, cell);
                cx.touch(sheet, at);
            }
        }
        Ok(Json::Null)
    })
}

// ---------------------------------------------------------------- selection

fn selection_set(s: &mut Session, p: &Json) -> Result<Json> {
    if p.get("sheet").is_some() {
        let i = target_sheet(s, p)?;
        s.doc_mut()?.wb_switch_sheet(i);
    } else if let Some(r) = str_param(p, "range").or_else(|| str_param(p, "cell"))
        && let (Some(_), _) = split_sheet(r)
    {
        let i = target_sheet(s, p)?;
        s.doc_mut()?.wb_switch_sheet(i);
    }
    let ranges = target_ranges(s, p)?;
    let d = s.doc_mut()?;
    let active = cell_param(p, "active").or_else(|| ranges.last().map(|r| r.start)).unwrap_or_default();
    let mut sel = crate::Selection { active, anchor: ranges.last().map(|r| r.start).unwrap_or(active), ranges };
    if let Some(sh) = d.wb.active() {
        sel.expand_merges(sh);
    }
    d.selection = sel;
    Ok(json!({"selection": d.selection.a1(), "active": d.selection.active.a1()}))
}

impl crate::DocState {
    /// Switches the active sheet, remembering each sheet's selection.
    pub fn wb_switch_sheet(&mut self, i: usize) {
        if i >= self.wb.sheets.len() || i == self.wb.active_sheet {
            return;
        }
        let cur = self.wb.active_sheet;
        if self.sheet_selections.len() < self.wb.sheets.len() {
            self.sheet_selections.resize(self.wb.sheets.len(), crate::Selection::default());
        }
        if let Some(slot) = self.sheet_selections.get_mut(cur) {
            *slot = self.selection.clone();
        }
        // The active sheet is view state: change it without making the workbook dirty, and
        // without recording it in the replica (the replica holds no active sheet): neither makes
        // a replica (a viewer's first switch would read as an edit) nor records a step, which
        // settles every cell's position and took a second on a sheet of 200,000 cells.
        let dirty = self.is_dirty();
        let in_sync = std::sync::Arc::ptr_eq(&self.link.synced, &self.wb);
        let wb = std::sync::Arc::make_mut(&mut self.wb);
        wb.active_sheet = i;
        if !dirty {
            self.saved = self.wb.clone();
        }
        if in_sync {
            self.link.synced = self.wb.clone();
        }
        self.selection = self.sheet_selections.get(i).cloned().unwrap_or_default();
    }
}

fn selection_move(s: &mut Session, p: &Json) -> Result<Json> {
    let dr = p.get("dr").and_then(Json::as_i64).unwrap_or(0);
    let dc = p.get("dc").and_then(Json::as_i64).unwrap_or(0);
    let extend = bool_param(p, "extend").unwrap_or(false);
    let jmp = bool_param(p, "jump").unwrap_or(false);
    let place = match str_param(p, "to") {
        Some(name) => Some(crate::selection::Place::parse(name).ok_or_else(|| bad("select.move", "`to` is rowStart, sheetStart or lastCell"))?),
        None => None,
    };
    let d = s.doc_mut()?;
    let Some(sheet) = d.wb.active() else { return Err(EngineError::NoDocument) };
    let sel = &mut d.selection;
    // Extension moves the far corner, not the active cell.
    let from = if extend {
        let r = sel.current();
        let far_row = if sel.anchor.row == r.start.row { r.end.row } else { r.start.row };
        let far_col = if sel.anchor.col == r.start.col { r.end.col } else { r.start.col };
        CellRef::new(far_row, far_col)
    } else {
        sel.active
    };
    // Skip hidden rows/columns and step over merged areas.
    let mut to = if let Some(place) = place {
        crate::Selection::place(sheet, from, place)
    } else if jmp {
        jump(sheet, from, dr.signum(), dc.signum())
    } else {
        let mut t = from.offset_clamped(dr, dc);
        let mut guard = 0;
        while (sheet.is_row_hidden(t.row) || sheet.is_col_hidden(t.col)) && guard < 100_000 {
            let n = t.offset_clamped(dr.signum(), dc.signum());
            if n == t {
                break;
            }
            t = n;
            guard += 1;
        }
        if !extend && let Some(m) = sheet.merge_at(from) {
            // Leaving a merged cell moves from its edge.
            if dr > 0 {
                t = CellRef::new((m.end.row + dr as u32).min(MAX_ROWS - 1), from.col);
            } else if dc > 0 {
                t = CellRef::new(from.row, (m.end.col + dc as u32).min(MAX_COLS - 1));
            }
        }
        t
    };
    if let Some(m) = sheet.merge_at(to)
        && !extend
    {
        to = m.start;
    }
    if extend {
        sel.extend_to(to);
        sel.expand_merges(sheet);
    } else {
        *sel = crate::Selection::at(to);
        if let Some(m) = sheet.merge_at(to) {
            sel.ranges = vec![m];
        }
    }
    Ok(json!({"selection": sel.a1(), "active": sel.active.a1()}))
}

fn selection_next(s: &mut Session, p: &Json) -> Result<Json> {
    let fwd = bool_param(p, "forward").unwrap_or(true);
    let by_row = bool_param(p, "byRow").unwrap_or(false);
    let d = s.doc_mut()?;
    let sheet = d.wb.active();
    d.selection.cycle(sheet, fwd, by_row);
    Ok(json!({"active": d.selection.active.a1()}))
}

fn select_all(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc_mut()?;
    let Some(sheet) = d.wb.active() else { return Err(EngineError::NoDocument) };
    let region = current_region(sheet, d.selection.active);
    let all = RangeRef::all();
    let r = if region.is_single() || d.selection.current() == region { all } else { region };
    let active = d.selection.active;
    d.selection = crate::Selection { active, anchor: r.start, ranges: vec![r] };
    Ok(json!({"selection": d.selection.a1()}))
}

fn select_region(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc_mut()?;
    let Some(sheet) = d.wb.active() else { return Err(EngineError::NoDocument) };
    let r = current_region(sheet, d.selection.active);
    let active = d.selection.active;
    d.selection = crate::Selection { active, anchor: r.start, ranges: vec![r] };
    Ok(json!({"selection": d.selection.a1()}))
}

fn select_row(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc_mut()?;
    let r = d.selection.current();
    let active = d.selection.active;
    d.selection = crate::Selection { active, anchor: CellRef::new(r.start.row, 0), ranges: vec![RangeRef::rows(r.start.row, r.end.row)] };
    Ok(json!({"selection": d.selection.a1()}))
}

fn select_col(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc_mut()?;
    let r = d.selection.current();
    let active = d.selection.active;
    d.selection = crate::Selection { active, anchor: CellRef::new(0, r.start.col), ranges: vec![RangeRef::cols(r.start.col, r.end.col)] };
    Ok(json!({"selection": d.selection.a1()}))
}

fn go_to(s: &mut Session, p: &Json) -> Result<Json> {
    let Some(reference) = str_param(p, "reference").or_else(|| str_param(p, "range")) else { return Err(bad("edit.goTo", "missing `reference`")) };
    // A defined name or table?
    let d = s.doc()?;
    let resolved = if RangeRef::parse(split_sheet(reference).1).is_none() {
        let sheet = d.wb.active_sheet;
        if let Some(n) = d.wb.name(reference, sheet) {
            n.formula.clone()
        } else if let Some((si, ti)) = d.wb.table(reference) {
            // As in Excel, a table's name stands for its data, without the header and totals rows.
            let t = d.wb.sheet(si).and_then(|sh| sh.tables.get(ti)).map(|t| t.data_range().unwrap_or(t.range).a1()).unwrap_or_default();
            format!("'{}'!{}", d.wb.sheet(si).map(|s| s.name.clone()).unwrap_or_default(), t)
        } else {
            return Err(EngineError::Other("Reference isn't valid.".into()));
        }
    } else {
        reference.to_string()
    };
    let resolved = resolved.replace('$', "");
    selection_set(s, &json!({"range": resolved}))
}

fn go_to_special(s: &mut Session, p: &Json) -> Result<Json> {
    let kind = str_param(p, "kind").unwrap_or("blanks");
    let d = s.doc()?;
    let Some(sheet) = d.wb.active() else { return Err(EngineError::NoDocument) };
    let scope = if d.selection.is_single_cell() { sheet.used_range().unwrap_or(RangeRef::cell(d.selection.active)) } else { d.selection.bounds() };
    if kind == "lastCell" {
        let c = sheet.used_range().map(|r| r.end).unwrap_or_default();
        return selection_set(s, &json!({"cell": c.a1()}));
    }
    if kind == "currentRegion" {
        return select_region(s, &json!({}));
    }
    let mut hits: Vec<CellRef> = Vec::new();
    if scope.count() > 4_000_000 {
        return Err(bad("edit.goToSpecial", "range too large"));
    }
    for c in scope.iter() {
        let cell = sheet.cell(c);
        let v = sheet.value(c);
        let ok = match kind {
            "blanks" => v.is_empty(),
            "constants" => cell.is_some_and(|x| x.formula.is_none() && !x.value.is_empty()),
            "formulas" => cell.is_some_and(|x| x.formula.is_some()),
            "comments" | "notes" => sheet.comments.contains_key(&c),
            "errors" => v.is_error(),
            "numbers" => v.is_number(),
            "text" => v.is_text(),
            "visible" => !sheet.is_row_hidden(c.row) && !sheet.is_col_hidden(c.col),
            "conditionalFormats" => sheet.cond_formats.iter().any(|cf| cf.ranges.iter().any(|r| r.contains(c))),
            "dataValidation" => sheet.validations.iter().any(|dv| dv.ranges.iter().any(|r| r.contains(c))),
            _ => return Err(bad("edit.goToSpecial", format!("unknown kind `{kind}`"))),
        };
        if ok {
            hits.push(c);
        }
    }
    if hits.is_empty() {
        return Err(EngineError::Other("No cells were found.".into()));
    }
    let ranges = coalesce(&hits);
    let d = s.doc_mut()?;
    d.selection = crate::Selection { active: hits[0], anchor: hits[0], ranges };
    Ok(json!({"selection": d.selection.a1(), "count": hits.len()}))
}

/// Merges cells into vertical runs per column (a compact multi-area selection).
fn coalesce(cells: &[CellRef]) -> Vec<RangeRef> {
    let mut sorted = cells.to_vec();
    sorted.sort_by_key(|c| (c.col, c.row));
    let mut out: Vec<RangeRef> = Vec::new();
    for c in sorted {
        if let Some(last) = out.last_mut()
            && last.start.col == c.col
            && last.end.col == c.col
            && last.end.row + 1 == c.row
        {
            last.end.row = c.row;
            continue;
        }
        out.push(RangeRef::cell(c));
    }
    if out.len() > 2000 {
        out.truncate(2000);
    }
    out
}

// ---------------------------------------------------------------- history

fn undo(s: &mut Session, p: &Json) -> Result<Json> {
    step_history(s, p, false)
}

fn redo(s: &mut Session, p: &Json) -> Result<Json> {
    step_history(s, p, true)
}

/// Undo or redo `steps` steps of one undo scope (Kition: on a workbook with a replica, the replica
/// steps back, or forward, the transactions the steps took and the workbook becomes its result,
/// which keeps every collaborator's edit since; `crate::kdm_bridge`). The scope is the one `scope`
/// names, else that of who runs the command (engine rule E14a, KitModel M12a); a step under the
/// other scope's later ones is taken back through the replica, keeping those. A workbook undone
/// back to its saved content is clean again.
fn step_history(s: &mut Session, p: &Json, redo: bool) -> Result<Json> {
    if s.capturing() {
        // The running command's own step is not on the history yet: undoing below it would take
        // the replica's wrong steps.
        return Err(EngineError::Other("Undo and Redo can't run inside another command.".into()));
    }
    let steps = u32_param(p, "steps").unwrap_or(1).max(1);
    let origin = s.origin();
    let scope = kdm_model::kition_kit::session::history_scope(p, &origin).map_err(|e| EngineError::Other(e.to_string()))?;
    let d = s.doc_mut()?;
    d.sync_kdm(&origin, false);
    let mut label = String::new();
    for _ in 0..steps {
        match step_once(d, redo, scope) {
            Some(l) => label = l,
            None => break,
        }
    }
    d.calc.rebuild(&d.wb);
    d.revision += 1;
    Ok(if redo { json!({"redone": label}) } else { json!({"undone": label}) })
}

/// One step of `scope`: the history's own snapshot when it is on top, else the replica's. `None`
/// when there is none to take.
fn step_once(d: &mut crate::DocState, redo: bool, scope: crate::UndoScope) -> Option<String> {
    let from = if redo { &d.redo } else { &d.undo };
    let i = from.iter().rposition(|u| u.scope == scope)?;
    if i + 1 < from.len() {
        // The other scope has steps after this one: only the replica can take this one back and
        // keep theirs.
        let steps = from.get(i)?.steps;
        let before = d.wb.clone();
        let (made, beyond) = d.replay_kdm(steps, !redo, scope)?;
        let e = if redo { d.redo.remove(i) } else { d.undo.remove(i) };
        let cur = crate::HistoryEntry { label: e.label.clone(), wb: before, selection: d.selection.clone(), steps: made, scope };
        if redo {
            d.undo.push(cur);
            crate::kdm_bridge::absorb(&mut d.redo, scope, beyond);
        } else {
            d.redo.push(cur);
            crate::kdm_bridge::absorb(&mut d.undo, scope, beyond);
        }
        d.trim_kdm();
        return Some(e.label);
    }
    let (from, to) = if redo { (&mut d.redo, &mut d.undo) } else { (&mut d.undo, &mut d.redo) };
    let e = from.pop()?;
    to.push(crate::HistoryEntry { label: e.label.clone(), wb: d.wb.clone(), selection: d.selection.clone(), steps: e.steps, scope });
    let reference = e.wb.clone();
    d.wb = e.wb;
    d.selection = e.selection;
    d.step_kdm(e.steps, !redo, scope);
    if std::sync::Arc::ptr_eq(&reference, &d.saved)
        && !std::sync::Arc::ptr_eq(&d.wb, &reference)
        && kdm_model::normalize(&d.wb) == kdm_model::normalize(&reference)
    {
        d.saved = d.wb.clone();
    }
    Some(e.label)
}

// ---------------------------------------------------------------- clipboard

fn copy_impl(s: &mut Session, p: &Json, cut: bool) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let ranges = target_ranges(s, p)?;
    if ranges.len() > 1 {
        return Err(EngineError::Other("This action won't work on multiple selections.".into()));
    }
    let range = ranges.first().copied().unwrap_or_default();
    let d = s.doc()?;
    let Some(sh) = d.wb.sheet(sheet) else { return Err(EngineError::NoDocument) };
    let range = if range.is_full_cols() || range.is_full_rows() {
        let used = sh.used_range().unwrap_or(RangeRef::cell(range.start));
        range.intersection(&used.union(&RangeRef::cell(range.start))).unwrap_or(range)
    } else {
        range
    };
    let text = crate::display::range_text(&d.wb, sheet, range);
    // The HTML rendering is for the host clipboard (UI); it can reach 4 MB, so agents and
    // scripts get it only when they ask for it.
    let html = if p.get("html").and_then(Json::as_bool) == Some(true) {
        crate::io::range_html(&d.wb, sheet, range, &text).map(|html| match clip_payload(&d.wb, sheet, range) {
            // Kition Sheet's own copy of the cells rides along, for another workbook's paste.
            Some(payload) => html.replacen("<table ", &format!("<table {CLIP_ATTR}=\"{payload}\" "), 1),
            None => html,
        })
    } else {
        None
    };
    let clip = Clipboard { wb: d.wb.clone(), sheet, range, cut, doc_uid: d.uid, text: text.clone() };
    s.clipboard = Some(clip);
    let mut result = json!({"range": range.a1(), "text": text});
    if let Some(html) = html {
        result["html"] = Json::String(html);
    }
    Ok(result)
}

/// The attribute of the clipboard's HTML table that carries Kition Sheet's own copy of the cells.
const CLIP_ATTR: &str = "data-kition-sheet";
/// Most bytes that copy may take (encoded); larger copies keep only the HTML.
const CLIP_PAYLOAD_MAX: usize = 4 * 1024 * 1024;

/// The copied cells as a workbook of their own (values, formulas, formats, checkboxes, merges,
/// notes, links, column widths and row heights, in their places), as base64 JSON: pasting it into
/// another workbook gives what pasting within one does, which the HTML table cannot carry.
fn clip_payload(wb: &Workbook, sheet: usize, range: RangeRef) -> Option<String> {
    let src = wb.sheet(sheet)?;
    if range.count() > 100_000 {
        return None;
    }
    let mut out = Workbook::new();
    out.theme = wb.theme.clone();
    out.date_system = wb.date_system;
    let mut sh = kition_sheet_model::Sheet::new(src.name.clone());
    let mut styles = std::collections::HashMap::new();
    for (c, cell) in src.cells.iter_range(range) {
        let style = *styles.entry(cell.style).or_insert_with(|| out.styles.intern(wb.styles.get(cell.style).clone()));
        let mut copy = cell.clone();
        copy.style = style;
        sh.cells.set(c, copy);
    }
    sh.merges = src.merges.iter().filter(|m| range.contains(m.start)).copied().collect();
    sh.comments = src.comments.iter().filter(|(c, _)| range.contains(**c)).map(|(c, v)| (*c, v.clone())).collect();
    sh.hyperlinks = src.hyperlinks.iter().filter(|(c, _)| range.contains(**c)).map(|(c, v)| (*c, v.clone())).collect();
    sh.cols = src.cols.range(range.start.col..=range.end.col).map(|(k, v)| (*k, kition_sheet_model::LineInfo { style: None, ..*v })).collect();
    sh.rows = src.rows.range(range.start.row..=range.end.row).map(|(k, v)| (*k, kition_sheet_model::LineInfo { style: None, ..*v })).collect();
    sh.default_col_width = src.default_col_width;
    sh.default_row_height = src.default_row_height;
    out.sheets = vec![std::sync::Arc::new(sh)];
    let json = serde_json::to_vec(&serde_json::json!({ "range": range.a1(), "workbook": out })).ok()?;
    let encoded = crate::io::base64_encode(&json);
    (encoded.len() <= CLIP_PAYLOAD_MAX).then_some(encoded)
}

/// The copy Kition Sheet put in the clipboard's HTML ([`clip_payload`]), as a clipboard to paste.
fn clip_from_html(html: &str, text: &str) -> Option<Clipboard> {
    let start = html.find(&format!("{CLIP_ATTR}=\""))? + CLIP_ATTR.len() + 2;
    let end = start + html.get(start..)?.find('"')?;
    let bytes = crate::io::base64_decode(html.get(start..end)?)?;
    let value: Json = serde_json::from_slice(&bytes).ok()?;
    let range = RangeRef::parse(value.get("range")?.as_str()?)?;
    let mut wb: Workbook = serde_json::from_value(value.get("workbook")?.clone()).ok()?;
    wb.styles.rebuild_index();
    if wb.sheets.is_empty() {
        return None;
    }
    // Another workbook's copy: never a move, whatever the source did.
    Some(Clipboard { wb: std::sync::Arc::new(wb), sheet: 0, range, cut: false, doc_uid: u64::MAX, text: text.to_owned() })
}

fn copy(s: &mut Session, p: &Json) -> Result<Json> {
    copy_impl(s, p, false)
}
fn cut(s: &mut Session, p: &Json) -> Result<Json> {
    copy_impl(s, p, true)
}
fn clear_clipboard(s: &mut Session, _: &Json) -> Result<Json> {
    s.clipboard = None;
    s.format_painter = None;
    ok()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum What {
    All,
    Formulas,
    Values,
    Formats,
    Comments,
    Validation,
    AllExceptBorders,
    ColumnWidths,
    FormulasAndNumberFormats,
    ValuesAndNumberFormats,
}

fn paste(s: &mut Session, p: &Json) -> Result<Json> {
    // Entries pasted right under or right of a table make it larger, as typed ones do.
    let r = paste_entries(s, p);
    s.expand_tables = false;
    r
}

fn paste_entries(s: &mut Session, p: &Json) -> Result<Json> {
    s.expand_tables = true;
    // External text (from the system clipboard) when it differs from ours; its HTML table first,
    // which keeps what the tab-separated text loses.
    let text = str_param(p, "text");
    let external =
        text.is_none_or(|text| s.clipboard.as_ref().is_none_or(|c| c.text.trim_end_matches(['\r', '\n']) != text.trim_end_matches(['\r', '\n'])));
    if external {
        // Another Kition Sheet workbook's copy: pasted as a copy within one is.
        if let Some(clip) = str_param(p, "html").and_then(|html| clip_from_html(html, text.unwrap_or_default())) {
            s.clipboard = Some(clip);
            return paste_special(s, &json!({"what": "all", "at": p.get("at").cloned().unwrap_or(Json::Null)}));
        }
        if let Some(table) = str_param(p, "html").and_then(crate::html_table::parse).filter(|t| !t.cells.is_empty()) {
            return paste_table(s, p, &table);
        }
        if let Some(text) = text {
            return paste_text(s, p, text);
        }
    }
    if s.clipboard.is_none() {
        return Err(EngineError::Other("Nothing to paste.".into()));
    }
    paste_special(s, &json!({"what": "all", "at": p.get("at").cloned().unwrap_or(Json::Null)}))
}

fn paste_text(s: &mut Session, p: &Json, text: &str) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = cell_param(p, "at").unwrap_or(s.doc()?.selection.current().start);
    let rows: Vec<Vec<String>> = parse_tsv(text);
    let h = rows.len();
    let w = rows.iter().map(Vec::len).max().unwrap_or(0);
    edit(s, |cx| {
        for (ri, row) in rows.iter().enumerate() {
            for (ci, v) in row.iter().enumerate() {
                let Some(c) = at.offset(ri as i64, ci as i64) else { continue };
                let cell = input_to_cell(v, sheet, c, &mut cx.wb).unwrap_or_else(|_| Some(Cell::value(Value::text(v.as_str()))));
                let sh = cx.sheet_mut(sheet)?;
                match cell {
                    Some(cell) => sh.set_cell(c, cell),
                    None => {
                        sh.remove_cell(c);
                    }
                }
                cx.touch(sheet, c);
            }
        }
        if h > 0 && w > 0 {
            let end = at.offset_clamped(h as i64 - 1, w as i64 - 1);
            *cx.sel = crate::Selection { active: at, anchor: at, ranges: vec![RangeRef::new(at, end)] };
        }
        Ok(())
    })?;
    Ok(json!({"rows": h, "cols": w}))
}

/// An HTML table at the active cell (or `at`): its cells with their formatting, its merged cells,
/// and the column widths it declares. The cells a merge covers take its formatting, so its
/// borders go round the whole merge.
fn paste_table(s: &mut Session, p: &Json, table: &crate::html_table::HtmlTable) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = cell_param(p, "at").unwrap_or(s.doc()?.selection.current().start);
    let area = RangeRef::new(at, at.offset_clamped(table.rows.saturating_sub(1) as i64, table.cols.saturating_sub(1) as i64));
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.merges.retain(|m| !m.intersects(&area));
        for html in &table.cells {
            let Some(c) = at.offset(html.row as i64, html.col as i64) else { continue };
            let style = cx.wb.styles.derive(StyleId::DEFAULT, |st| apply_look(st, &html.look));
            let span = RangeRef::new(c, c.offset_clamped(html.rows.saturating_sub(1) as i64, html.cols.saturating_sub(1) as i64));
            let sh = cx.sheet_mut(sheet)?;
            if !span.is_single() {
                sh.merges.push(span);
            }
            let blank = Cell { value: Value::Empty, formula: None, style, rich: None };
            let mut covered = Vec::new();
            for row in span.start.row..=span.end.row {
                for col in span.start.col..=span.end.col {
                    let cell = CellRef::new(row, col);
                    if blank.is_blank() {
                        sh.remove_cell(cell);
                    } else {
                        sh.set_cell(cell, blank.clone());
                    }
                    covered.push((sheet, cell));
                }
            }
            cx.changed.extend(covered);
            if html.text.is_empty() {
                continue;
            }
            let cell = input_to_cell(&html.text, sheet, c, &mut cx.wb)
                .unwrap_or_else(|_| Some(Cell { value: Value::text(html.text.as_str()), formula: None, style, rich: None }));
            if let Some(cell) = cell {
                cx.sheet_mut(sheet)?.set_cell(c, cell);
            }
        }
        let sh = cx.sheet_mut(sheet)?;
        for (i, width) in table.widths.iter().enumerate() {
            if let (Some(width), Some(col)) = (width, at.col.checked_add(i as u32).filter(|c| *c < MAX_COLS)) {
                let line = sh.cols.entry(col).or_default();
                line.size = Some(width.clamp(1.0, 1800.0).round());
                line.hidden = false;
            }
        }
        *cx.sel = crate::Selection { active: at, anchor: at, ranges: vec![area] };
        Ok(())
    })?;
    Ok(json!({"rows": table.rows, "cols": table.cols, "merges": table.cells.iter().filter(|c| c.rows > 1 || c.cols > 1).count()}))
}

/// A pasted HTML cell's formatting over the workbook's default style.
fn apply_look(st: &mut kition_sheet_model::Style, look: &crate::html_table::Look) {
    use kition_sheet_model::{Fill, Underline};
    if let Some(bold) = look.bold {
        st.font.bold = bold;
    }
    if let Some(italic) = look.italic {
        st.font.italic = italic;
    }
    if let Some(underline) = look.underline {
        st.font.underline = if underline { Underline::Single } else { Underline::None };
    }
    if let Some(strike) = look.strike {
        st.font.strike = strike;
    }
    if let Some(color) = look.color {
        st.font.color = color;
    }
    if let Some(size) = look.size {
        st.font.size = size;
    }
    if let Some(font) = &look.font {
        st.font.set_name(font.clone());
    }
    if let Some(fill) = look.fill {
        st.fill = Fill::solid(fill);
    }
    if let Some(h) = look.h {
        st.align.h = h;
    }
    if let Some(v) = look.v {
        st.align.v = v;
    }
    if let Some(wrap) = look.wrap {
        st.align.wrap = wrap;
    }
    let [left, right, top, bottom] = look.borders;
    for (line, side) in [(left, &mut st.borders.left), (right, &mut st.borders.right), (top, &mut st.borders.top), (bottom, &mut st.borders.bottom)] {
        if let Some(line) = line {
            *side = line;
        }
    }
}

/// Tab-separated text with Excel-style quoting.
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut at_start = true;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => {
                quoted = true;
                at_start = false;
            }
            '\t' => {
                row.push(std::mem::take(&mut field));
                at_start = true;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                at_start = true;
            }
            _ => {
                field.push(c);
                at_start = false;
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn paste_special(s: &mut Session, p: &Json) -> Result<Json> {
    let Some(clip) = s.clipboard.clone() else { return Err(EngineError::Other("Nothing to paste.".into())) };
    let what = match str_param(p, "what").unwrap_or("all") {
        "all" => What::All,
        "formulas" => What::Formulas,
        "values" => What::Values,
        "formats" => What::Formats,
        "comments" | "notes" => What::Comments,
        "validation" => What::Validation,
        "allExceptBorders" => What::AllExceptBorders,
        "columnWidths" => What::ColumnWidths,
        "formulasAndNumberFormats" => What::FormulasAndNumberFormats,
        "valuesAndNumberFormats" => What::ValuesAndNumberFormats,
        other => return Err(bad("edit.pasteSpecial", format!("unknown `what`: {other}"))),
    };
    let op = str_param(p, "operation").unwrap_or("none").to_string();
    let skip_blanks = bool_param(p, "skipBlanks").unwrap_or(false);
    let transpose = bool_param(p, "transpose").unwrap_or(false);
    let link = bool_param(p, "link").unwrap_or(false);
    let dest_sheet = target_sheet(s, p)?;
    let sel = s.doc()?.selection.current();
    let at = cell_param(p, "at").unwrap_or(sel.start);
    let src = clip.range;
    let (h, w) = if transpose { (src.width(), src.height()) } else { (src.height(), src.width()) };
    // Tile the copy over a larger destination that is a multiple of its size.
    let tile = sel.height() % h == 0
        && sel.width() % w == 0
        && !sel.is_single()
        && cell_param(p, "at").is_none()
        && !sel.is_full_cols()
        && !sel.is_full_rows()
        && sel.count() <= 4_000_000;
    let (tiles_r, tiles_c) = if tile { (sel.height() / h, sel.width() / w) } else { (1, 1) };
    let same_doc = s.doc()?.uid == clip.doc_uid;
    let cut_move = clip.cut && same_doc && what == What::All;
    let src_wb = clip.wb.clone();
    let Some(src_sheet) = src_wb.sheet(clip.sheet) else { return Err(EngineError::Other("The copied sheet no longer exists.".into())) };
    let src_sheet_name = src_sheet.name.clone();
    edit(s, |cx| {
        // Styles from another workbook must be re-interned.
        let mut style_map = std::collections::HashMap::new();
        let mut map_style = |id: StyleId, wb: &mut kition_sheet_model::Workbook| -> StyleId {
            if same_doc {
                return id;
            }
            *style_map.entry(id).or_insert_with(|| wb.styles.intern(src_wb.styles.get(id).clone()))
        };
        let mut moved_formulas = Vec::new();
        let mut moved_pictures = std::collections::BTreeMap::new();
        if cut_move {
            // Remove the source first (contents and formats), and fix references to it.
            let source = cx.sheet_mut(clip.sheet)?;
            moved_pictures = source.cell_pictures.iter().filter(|(c, _)| src.contains(**c)).map(|(c, p)| (*c, p.clone())).collect();
            let taken = source.take_cells(src);
            for (c, _) in &taken {
                cx.touch(clip.sheet, *c);
            }
            moved_formulas = taken;
        }
        for tr in 0..tiles_r {
            for tc in 0..tiles_c {
                for r in 0..src.height() {
                    for c in 0..src.width() {
                        let sc = CellRef::new(src.start.row + r, src.start.col + c);
                        let (dr, dc) = if transpose { (c, r) } else { (r, c) };
                        let Some(dest) = at.offset((tr * h + dr) as i64, (tc * w + dc) as i64) else { continue };
                        let src_cell = if cut_move {
                            moved_formulas.iter().find(|(k, _)| *k == sc).map(|(_, v)| v.clone())
                        } else {
                            src_sheet.cell(sc).cloned()
                        };
                        let src_val = src_sheet.value(sc);
                        let src_picture = if cut_move { moved_pictures.get(&sc) } else { src_sheet.cell_pictures.get(&sc) }.cloned();
                        if skip_blanks && src_picture.is_none() && src_val.is_empty() && src_cell.as_ref().is_none_or(|x| x.formula.is_none()) {
                            continue;
                        }
                        let old = cx.wb.sheet(dest_sheet).and_then(|sh| sh.cell(dest)).cloned().unwrap_or_default();
                        let old_picture = cx.wb.sheet(dest_sheet).and_then(|sh| sh.cell_pictures.get(&dest)).cloned();
                        let mut new_picture = old_picture.clone();
                        let mut new = old.clone();
                        let shift = |cell: &Cell| -> Option<std::sync::Arc<Formula>> {
                            let f = cell.formula.as_ref()?;
                            let e = f.expr()?;
                            if cut_move {
                                // A moved formula still reads cells on its original sheet. Qualify
                                // those references before the workbook-wide move adjustment below.
                                let e = if dest_sheet != clip.sheet {
                                    e.map(&mut |x| match x {
                                        kition_sheet_formula::Expr::Ref(mut r) if r.sheet == kition_sheet_formula::SheetSel::Current => {
                                            r.sheet = kition_sheet_formula::SheetSel::Named(src_sheet_name.clone());
                                            kition_sheet_formula::Expr::Ref(r)
                                        }
                                        other => other,
                                    })
                                } else {
                                    e
                                };
                                let mut moved = Formula::from_expr(e);
                                moved.array = f.array;
                                return Some(std::sync::Arc::new(moved));
                            }
                            let moved =
                                kition_sheet_formula::adjust::shift_relative(e, dest.row as i64 - sc.row as i64, dest.col as i64 - sc.col as i64);
                            Some(std::sync::Arc::new(Formula::from_expr(moved)))
                        };
                        if link {
                            let sheet_prefix = if dest_sheet == clip.sheet && same_doc {
                                String::new()
                            } else {
                                format!("{}!", kition_sheet_formula::quote_sheet(&src_sheet_name))
                            };
                            new.formula = Some(std::sync::Arc::new(Formula::new(&format!("={sheet_prefix}{}", sc.a1()))));
                            new.value = Value::Empty;
                            new_picture = None;
                        } else {
                            let sc_cell = src_cell.clone().unwrap_or_default();
                            match what {
                                What::All | What::AllExceptBorders => {
                                    new_picture = src_picture.clone();
                                    new.formula = shift(&sc_cell);
                                    new.value = if new.formula.is_some() { Value::Empty } else { sc_cell.value.clone() };
                                    let st = map_style(sc_cell.style, &mut cx.wb);
                                    new.style = if what == What::AllExceptBorders {
                                        let borders = cx.wb.styles.get(old.style).borders;
                                        cx.wb.styles.derive(st, |s| s.borders = borders)
                                    } else {
                                        st
                                    };
                                }
                                What::Formulas | What::FormulasAndNumberFormats => {
                                    new_picture = src_picture.clone();
                                    new.formula = shift(&sc_cell);
                                    new.value = if new.formula.is_some() { Value::Empty } else { sc_cell.value.clone() };
                                    if what == What::FormulasAndNumberFormats {
                                        let nf = src_wb.styles.get(sc_cell.style).num_fmt.clone();
                                        new.style = cx.wb.styles.derive(old.style, |s| s.num_fmt = nf);
                                    }
                                }
                                What::Values | What::ValuesAndNumberFormats => {
                                    new_picture = src_picture.clone();
                                    new.formula = None;
                                    new.value = src_val.clone();
                                    if what == What::ValuesAndNumberFormats {
                                        let nf = src_wb.styles.get(sc_cell.style).num_fmt.clone();
                                        new.style = cx.wb.styles.derive(old.style, |s| s.num_fmt = nf);
                                    }
                                }
                                // Formats only: a checkbox stays the target's (it is content).
                                What::Formats => {
                                    let st = map_style(sc_cell.style, &mut cx.wb);
                                    new.style = cx.wb.styles.keep_checkbox(old.style, st);
                                }
                                What::Comments | What::Validation | What::ColumnWidths => {}
                            }
                            // Operations combine numbers with what is already there.
                            if op != "none"
                                && matches!(what, What::All | What::Values | What::ValuesAndNumberFormats | What::Formulas)
                                && new.formula.is_none()
                            {
                                if old_picture.is_some() || new_picture.is_some() {
                                    new_picture = None;
                                    new.value = Value::Error(kition_sheet_core::CellError::Value);
                                }
                                let a = old.value.clone();
                                let b = new.value.clone();
                                // Numbers stored as text take part (multiplying by 1 turns them into
                                // numbers); other text and errors stay as they are.
                                if let (Ok(x), Ok(y)) = (a.to_number(), b.to_number())
                                    && (a.is_number() || a.is_empty() || a.is_text())
                                    && (b.is_number() || b.is_empty())
                                {
                                    new.value = match op.as_str() {
                                        "add" => Value::number(x + y),
                                        "subtract" => Value::number(x - y),
                                        "multiply" => Value::number(x * y),
                                        "divide" => {
                                            if y == 0.0 {
                                                Value::Error(kition_sheet_core::CellError::Div0)
                                            } else {
                                                Value::number(x / y)
                                            }
                                        }
                                        _ => b.clone(),
                                    };
                                } else if (a.is_text() || a.is_error()) && (b.is_number() || b.is_empty()) {
                                    new.value = a;
                                }
                                // A formula already there is kept and combined with the pasted number.
                                if let Some(f) = &old.formula
                                    && let Ok(y) = b.to_number()
                                    && (b.is_number() || b.is_empty())
                                    && let Some(sym) = match op.as_str() {
                                        "add" => Some('+'),
                                        "subtract" => Some('-'),
                                        "multiply" => Some('*'),
                                        "divide" => Some('/'),
                                        _ => None,
                                    }
                                {
                                    let text = format!("=({}){sym}{}", f.text, kition_sheet_core::number_to_text(y));
                                    new.formula = Some(std::sync::Arc::new(Formula::new(&text)));
                                    new.value = Value::Empty;
                                }
                            }
                        }
                        match what {
                            What::Comments => {
                                if let Some(cm) = src_sheet.comments.get(&sc).cloned() {
                                    cx.sheet_mut(dest_sheet)?.comments.insert(dest, cm);
                                }
                            }
                            What::ColumnWidths => {
                                if let Some(info) = src_sheet.cols.get(&sc.col).copied() {
                                    cx.sheet_mut(dest_sheet)?.cols.insert(dest.col, info);
                                }
                            }
                            What::All => {
                                if let Some(cm) = src_sheet.comments.get(&sc).cloned() {
                                    cx.sheet_mut(dest_sheet)?.comments.insert(dest, cm);
                                }
                                cx.sheet_mut(dest_sheet)?.set_cell(dest, new);
                            }
                            _ => cx.sheet_mut(dest_sheet)?.set_cell(dest, new),
                        }
                        if let Some(picture) = new_picture {
                            cx.sheet_mut(dest_sheet)?.cell_pictures.insert(dest, picture);
                        }
                        cx.touch(dest_sheet, dest);
                    }
                }
            }
        }
        // Merged areas inside the copied block travel with it.
        if matches!(what, What::All | What::Formats) && !transpose {
            let merges: Vec<RangeRef> = src_sheet.merges.iter().filter(|m| src.contains_range(m)).copied().collect();
            for m in merges {
                let dr = at.row as i64 - src.start.row as i64;
                let dc = at.col as i64 - src.start.col as i64;
                if let (Some(a), Some(b)) = (m.start.offset(dr, dc), m.end.offset(dr, dc)) {
                    let nm = RangeRef::new(a, b);
                    let sh = cx.sheet_mut(dest_sheet)?;
                    sh.merges.retain(|x| !x.intersects(&nm));
                    sh.merges.push(nm);
                }
            }
        }
        if cut_move {
            // References elsewhere to the moved block now point to its new place.
            let dest_name = cx.wb.sheet(dest_sheet).map(|s| s.name.clone()).unwrap_or_default();
            let e = if dest_sheet == clip.sheet {
                kition_sheet_formula::adjust::Edit::Move { from: src, to_row: at.row, to_col: at.col }
            } else {
                kition_sheet_formula::adjust::Edit::MoveToSheet { from: src, to_sheet: dest_name, to_row: at.row, to_col: at.col }
            };
            rewrite_all_formulas(&mut cx.wb, &src_sheet_name, &e);
            cx.log_structure(clip.sheet, &e);
            cx.structural = true;
        }
        let end = at.offset_clamped((tiles_r * h) as i64 - 1, (tiles_c * w) as i64 - 1);
        *cx.sel = crate::Selection { active: at, anchor: at, ranges: vec![RangeRef::new(at, end)] };
        Ok(())
    })?;
    if cut_move {
        s.clipboard = None;
    }
    Ok(json!({"pasted": s.doc()?.selection.a1()}))
}

/// Applies a reference adjustment to every cell formula and defined name in the workbook.
pub(crate) fn rewrite_all_formulas(wb: &mut kition_sheet_model::Workbook, target: &str, e: &kition_sheet_formula::adjust::Edit) {
    for si in 0..wb.sheets.len() {
        let host = wb.sheets.get(si).map(|s| s.name.clone()).unwrap_or_default();
        let keys: Vec<(CellRef, std::sync::Arc<Formula>)> =
            wb.sheets.get(si).map(|s| s.cells.iter().filter_map(|(c, cell)| cell.formula.clone().map(|f| (c, f))).collect()).unwrap_or_default();
        if keys.is_empty() {
            continue;
        }
        let Some(sheet) = wb.sheet_mut(si) else { continue };
        for (c, f) in keys {
            let Some(expr) = f.expr() else { continue };
            let new = kition_sheet_formula::adjust::adjust(expr, &host, target, e);
            let text = kition_sheet_formula::print(&new);
            if text != f.text
                && let Some(cell) = sheet.cells.get_mut(c)
            {
                let mut nf = Formula::from_expr(new);
                nf.array = f.array;
                cell.formula = Some(std::sync::Arc::new(nf));
            }
        }
    }
    for n in wb.names.iter_mut() {
        if let Ok(expr) = kition_sheet_formula::parse(&n.formula) {
            // Names are workbook-level: unqualified refs don't occur; qualified ones adjust.
            let new = kition_sheet_formula::adjust::adjust(expr, "\u{0}", target, e);
            n.formula = kition_sheet_formula::print(&new);
        }
    }
}

// ---------------------------------------------------------------- clear

fn clear(s: &mut Session, p: &Json, what: &str) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let ranges = target_ranges(s, p)?;
    edit(s, |cx| {
        // A whole table cleared of its formats loses its style, cleared of everything it goes
        // (Clear All, as in Excel); part of a table keeps the table's look.
        if matches!(what, "all" | "formats") {
            let sh = cx.sheet_mut(sheet)?;
            let whole = |t: &kition_sheet_model::Table| ranges.iter().any(|r| r.contains_range(&t.range));
            if what == "all" {
                if sh.tables.iter().any(whole) {
                    sh.tables.retain(|t| !whole(t));
                    cx.structural = true;
                }
            } else {
                for t in sh.tables.iter_mut().filter(|t| whole(t)) {
                    t.style.clear();
                }
            }
        }
        for r in &ranges {
            let sh = cx.sheet_mut(sheet)?;
            if matches!(what, "all" | "comments") {
                sh.comments.retain(|c, _| !r.contains(*c));
            }
            if matches!(what, "all" | "hyperlinks") {
                sh.hyperlinks.retain(|c, _| !r.contains(*c));
            }
            if matches!(what, "all" | "formats") {
                sh.merges.retain(|m| !r.contains_range(m));
            }
            if matches!(what, "comments" | "hyperlinks") {
                continue;
            }
            let r = match cx.wb.sheet(sheet).and_then(|sh| sh.used_range()) {
                Some(u) => match r.intersection(&u) {
                    Some(x) => x,
                    None => continue,
                },
                None => continue,
            };
            let cells: Vec<(CellRef, Cell)> =
                cx.wb.sheet(sheet).map(|sh| sh.cells.iter_range(r).map(|(c, x)| (c, x.clone())).collect()).unwrap_or_default();
            // A checkbox is the cell's content: Clear Contents takes it, Clear Formats leaves it.
            let mut kept = std::collections::HashMap::new();
            for (_, cell) in &cells {
                if !kept.contains_key(&cell.style) && cx.wb.styles.get(cell.style).checkbox {
                    let new = match what {
                        "contents" => cx.wb.styles.with_checkbox(cell.style, false),
                        _ => cx.wb.styles.with_checkbox(StyleId::DEFAULT, true),
                    };
                    kept.insert(cell.style, new);
                }
            }
            let sh = cx.sheet_mut(sheet)?;
            for (c, mut cell) in cells {
                match what {
                    "all" => {
                        sh.remove_cell(c);
                    }
                    "contents" => {
                        cell.value = Value::Empty;
                        cell.formula = None;
                        cell.style = kept.get(&cell.style).copied().unwrap_or(cell.style);
                        sh.set_cell(c, cell);
                    }
                    "formats" => {
                        cell.style = kept.get(&cell.style).copied().unwrap_or(StyleId::DEFAULT);
                        sh.cells.set(c, cell);
                    }
                    _ => {}
                }
            }
            for c in r.iter().take(1_000_000) {
                cx.changed.push((sheet, c));
            }
        }
        Ok(Json::Null)
    })
}

// ---------------------------------------------------------------- fill

fn fill_dir(s: &mut Session, p: &Json, dr: i64, dc: i64) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let mut r = target_range(s, p)?;
    // Whole rows or columns fill only where the sheet has content (nothing on an empty sheet).
    if r.is_full_cols() || r.is_full_rows() {
        match s.doc()?.wb.sheet(sheet).and_then(|sh| sh.used_range()).and_then(|u| r.intersection(&u)) {
            Some(u) => r = u,
            None => return Ok(Json::Null),
        }
    }
    if r.count() > 5_000_000 {
        return Err(bad("edit.fill", "range too large"));
    }
    // With one row selected, Fill Down copies from the row above (Excel behaviour).
    if dr == 1 && r.height() == 1 {
        r.start.row = r.start.row.saturating_sub(1);
    }
    if dc == 1 && r.width() == 1 {
        r.start.col = r.start.col.saturating_sub(1);
    }
    let src = if dr == 1 {
        RangeRef::new(r.start, CellRef::new(r.start.row, r.end.col))
    } else if dr == -1 {
        RangeRef::new(CellRef::new(r.end.row, r.start.col), r.end)
    } else if dc == 1 {
        RangeRef::new(r.start, CellRef::new(r.end.row, r.start.col))
    } else {
        RangeRef::new(CellRef::new(r.start.row, r.end.col), r.end)
    };
    edit(s, |cx| {
        crate::fill::fill(cx, sheet, src, r, crate::fill::FillMode::Copy)?;
        Ok(Json::Null)
    })
}

fn auto_fill(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let src = str_param(p, "source").and_then(RangeRef::parse).map_or_else(|| target_range(s, p), Ok)?;
    let Some(target) = str_param(p, "target").and_then(RangeRef::parse) else { return Err(bad("edit.autoFill", "missing `target`")) };
    let mode = match str_param(p, "mode").unwrap_or("series") {
        "extend" => crate::fill::FillMode::Extend,
        "copy" => crate::fill::FillMode::Copy,
        "formats" => crate::fill::FillMode::Formats,
        "values" => crate::fill::FillMode::ValuesOnly,
        _ => crate::fill::FillMode::Series,
    };
    if target.count() > 5_000_000 || src.count() > 5_000_000 {
        return Err(bad("edit.autoFill", "range too large"));
    }
    edit(s, |cx| {
        crate::fill::fill(cx, sheet, src, target, mode)?;
        *cx.sel = crate::Selection { active: src.start, anchor: src.start, ranges: vec![target.union(&src)] };
        Ok(Json::Null)
    })
}

fn fill_series(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let r = target_range(s, p)?;
    let rows = str_param(p, "direction").map(|d| d == "rows").unwrap_or(r.width() > r.height());
    let kind = str_param(p, "type").unwrap_or("linear").to_string();
    let step = f64_param(p, "step").unwrap_or(1.0);
    let stop = f64_param(p, "stop");
    let unit = str_param(p, "dateUnit").unwrap_or("day").to_string();
    edit(s, |cx| {
        crate::fill::series(cx, sheet, r, rows, &kind, step, stop, &unit)?;
        Ok(Json::Null)
    })
}

fn smart_fill(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let r = target_range(s, p)?;
    let n = edit(s, |cx| crate::fill::smart_fill(cx, sheet, r.start))?;
    if n == 0 {
        return Err(EngineError::Other(
            "We looked at all the data next to your selection and didn't see a pattern for filling in values for you.".into(),
        ));
    }
    Ok(json!({"filled": n}))
}

// ---------------------------------------------------------------- find & replace

pub fn matches(hay: &str, what: &str, case: bool, whole: bool) -> bool {
    let (h, w) = if case { (hay.to_string(), what.to_string()) } else { (hay.to_lowercase(), what.to_lowercase()) };
    let has_wild = w.contains(['*', '?']);
    if has_wild {
        let pat = if whole { w } else { format!("*{w}*") };
        return wildcard(&h, &pat);
    }
    if whole { h == w } else { h.contains(&w) }
}

/// `*`/`?` wildcard match with `~` escapes.
pub(crate) fn wildcard(text: &str, pat: &str) -> bool {
    let t: Vec<char> = text.chars().collect();
    let mut p: Vec<(char, bool)> = Vec::new();
    let mut it = pat.chars();
    while let Some(c) = it.next() {
        if c == '~' {
            if let Some(n) = it.next() {
                p.push((n, true));
            }
        } else {
            p.push((c, false));
        }
    }
    let (mut ti, mut pi) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == ('?', false) || (p[pi].0 == t[ti] && p[pi] != ('*', false))) {
            ti += 1;
            pi += 1;
        } else if pi < p.len() && p[pi] == ('*', false) {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(sp) = star {
            pi = sp + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == ('*', false) {
        pi += 1;
    }
    pi == p.len()
}

fn find(s: &mut Session, p: &Json) -> Result<Json> {
    let what = str_param(p, "what").unwrap_or("").to_string();
    if what.is_empty() {
        return Err(bad("edit.find", "missing `what`"));
    }
    let case = bool_param(p, "matchCase").unwrap_or(false);
    let whole = bool_param(p, "wholeCell").unwrap_or(false);
    let values = str_param(p, "lookIn") == Some("values");
    let by_cols = bool_param(p, "byColumns").unwrap_or(false);
    let all = bool_param(p, "all").unwrap_or(false);
    let workbook = str_param(p, "within") == Some("workbook");
    s.last_find = Some(p.clone());
    let d = s.doc()?;
    let sheets: Vec<usize> = if workbook { (0..d.wb.sheets.len()).collect() } else { vec![d.wb.active_sheet] };
    let mut hits: Vec<(usize, CellRef, String)> = Vec::new();
    for si in sheets {
        let Some(sh) = d.wb.sheet(si) else { continue };
        let mut cells: Vec<(CellRef, String)> = sh
            .cells
            .iter()
            .filter_map(|(c, cell)| {
                let text = if values || cell.formula.is_none() || crate::display::formula_hidden(&d.wb, sh, c) {
                    crate::display::cell_text(&d.wb, sh, c)
                } else {
                    cell.input_text()
                };
                matches(&text, &what, case, whole).then_some((c, text))
            })
            .collect();
        if by_cols {
            cells.sort_by_key(|(c, _)| (c.col, c.row));
        }
        hits.extend(cells.into_iter().map(|(c, t)| (si, c, t)));
    }
    if hits.is_empty() {
        return Err(EngineError::Other(format!("We couldn't find what you were looking for: {what}")));
    }
    if all {
        let list: Vec<Json> =
            hits.iter().map(|(si, c, t)| json!({"sheet": d.wb.sheet(*si).map(|s| s.name.clone()), "cell": c.a1(), "value": t})).collect();
        return Ok(json!({"count": list.len(), "results": list}));
    }
    // The next hit after the active cell (`backward`: the last one before it, Find Previous),
    // round to the first (last) past the end.
    let cur = (d.wb.active_sheet, d.selection.active);
    let key = |si: usize, c: CellRef| if by_cols { (si, c.col, c.row) } else { (si, c.row, c.col) };
    let next = if bool_param(p, "backward").unwrap_or(false) {
        hits.iter().rev().find(|(si, c, _)| key(*si, *c) < key(cur.0, cur.1)).or_else(|| hits.last()).cloned()
    } else {
        hits.iter().find(|(si, c, _)| key(*si, *c) > key(cur.0, cur.1)).or_else(|| hits.first()).cloned()
    };
    let Some((si, c, t)) = next else { return Err(EngineError::Other("not found".into())) };
    let dd = s.doc_mut()?;
    dd.wb_switch_sheet(si);
    dd.selection = crate::Selection::at(c);
    Ok(json!({"cell": c.a1(), "sheet": si, "value": t, "count": hits.len()}))
}

fn replace(s: &mut Session, p: &Json) -> Result<Json> {
    let what = str_param(p, "what").unwrap_or("").to_string();
    let with = str_param(p, "with").unwrap_or("").to_string();
    if what.is_empty() {
        return Err(bad("edit.replace", "missing `what`"));
    }
    let case = bool_param(p, "matchCase").unwrap_or(false);
    let whole = bool_param(p, "wholeCell").unwrap_or(false);
    let all = bool_param(p, "all").unwrap_or(true);
    let workbook = str_param(p, "within") == Some("workbook");
    let active = s.doc()?.selection.active;
    // The matches, worked out first: with none (nothing matches, or Replace's active cell does
    // not), the workbook and the undo history stay as they were.
    let d = s.doc()?;
    let sheets: Vec<usize> = if workbook { (0..d.wb.sheets.len()).collect() } else { vec![d.wb.active_sheet] };
    let mut planned: Vec<(usize, CellRef, String)> = Vec::new();
    for si in sheets {
        let Some(sh) = d.wb.sheet(si) else { continue };
        for (c, _) in sh.cells.iter() {
            if !all && c != active {
                continue;
            }
            let text = sh.input_text(c);
            if !matches(&text, &what, case, whole) {
                continue;
            }
            let new = if whole {
                with.clone()
            } else if case {
                text.replace(&what, &with)
            } else {
                replace_ci(&text, &what, &with)
            };
            planned.push((si, c, new));
        }
    }
    if planned.is_empty() {
        return Ok(json!({"replaced": 0}));
    }
    let n = edit(s, |cx| {
        let mut n = 0;
        for (si, c, new) in planned {
            let cell = input_to_cell(&new, si, c, &mut cx.wb).unwrap_or_else(|_| Some(Cell::value(Value::text(new.as_str()))));
            let sh = cx.sheet_mut(si)?;
            let cell = keep_runs(sh.cells.get(c), cell);
            match cell {
                Some(cell) => sh.set_cell(c, cell),
                None => {
                    sh.remove_cell(c);
                }
            }
            cx.touch(si, c);
            n += 1;
        }
        Ok(n)
    })?;
    Ok(json!({"replaced": n}))
}

fn replace_ci(text: &str, what: &str, with: &str) -> String {
    let lower = text.to_lowercase();
    let w = what.to_lowercase();
    if lower.len() != text.len() || w.is_empty() {
        return text.replace(what, with);
    }
    let mut out = String::new();
    let mut i = 0;
    while let Some(pos) = lower.get(i..).and_then(|s| s.find(&w)) {
        out.push_str(text.get(i..i + pos).unwrap_or(""));
        out.push_str(with);
        i += pos + w.len();
    }
    out.push_str(text.get(i..).unwrap_or(""));
    out
}

// ---------------------------------------------------------------- format painter

fn painter_state(s: &Session) -> Json {
    json!({"active": s.format_painter.is_some(), "sticky": s.format_painter.as_ref().is_some_and(|p| p.3)})
}

/// The format painter (Home › Clipboard): picks up the formats of the selection (its cell styles
/// and the merged areas in it), and paints them on the range clicked or dragged over, as WPS and
/// Excel do: a single cell takes the source's size, a larger range repeats the source over it, a
/// smaller one takes its part. Values, formulas, comments, links, conditional formats, validation,
/// row heights and column widths stay as they are. Once (a click on the button) the painter is put
/// down after one paint; sticky (a double-click) it stays up until cancelled.
fn format_painter(s: &mut Session, p: &Json) -> Result<Json> {
    if bool_param(p, "cancel") == Some(true) {
        s.format_painter = None;
        return Ok(painter_state(s));
    }
    if let Some(apply) = p.get("apply") {
        let dest = apply.as_str().and_then(RangeRef::parse).ok_or_else(|| bad("edit.formatPainter", "`apply` is a range such as \"C3:D4\""))?;
        let Some((wb, sheet, src, sticky)) = s.format_painter.clone() else { return Err(EngineError::Other("Pick up a format first.".into())) };
        paint_formats(s, &wb, sheet, src, dest)?;
        if !sticky {
            s.format_painter = None;
        }
        return Ok(painter_state(s));
    }
    let sticky = bool_param(p, "sticky");
    // A second click on the button puts the painter down.
    if s.format_painter.is_some() && sticky.is_none() {
        s.format_painter = None;
        return Ok(painter_state(s));
    }
    let d = s.doc()?;
    let mut src = d.selection.current();
    // A merged area is picked up whole.
    if let Some(sh) = d.wb.active() {
        loop {
            let grown = sh.merges.iter().filter(|m| m.intersects(&src)).fold(src, |r, m| r.union(m));
            if grown == src {
                break;
            }
            src = grown;
        }
    }
    s.format_painter = Some((d.wb.clone(), d.wb.active_sheet, src, sticky.unwrap_or(false)));
    Ok(painter_state(s))
}

/// The most cells one paint restyles cell by cell.
const PAINT_CELLS: u64 = 2_000_000;

fn paint_formats(s: &mut Session, wb: &std::sync::Arc<kition_sheet_model::Workbook>, sheet: usize, src: RangeRef, dest: RangeRef) -> Result<()> {
    let dest_sheet = s.doc()?.wb.active_sheet;
    if s.doc()?.wb.sheet(dest_sheet).is_some_and(|sh| sh.protection.as_ref().is_some_and(|pr| !pr.format_cells)) {
        return Err(EngineError::Other("The cell or chart you're trying to change is on a protected sheet.".into()));
    }
    let same = std::sync::Arc::ptr_eq(wb, &s.doc()?.wb) || s.doc()?.wb.styles == wb.styles;
    let Some(src_sheet) = wb.sheet(sheet).cloned() else { return Err(EngineError::Other("The sheet the format was picked up on is gone.".into())) };
    let (h, w) = (src.height().max(1), src.width().max(1));
    let lines = dest.is_full_rows() || dest.is_full_cols();
    // A single cell (or the merged area clicked) takes the source's size.
    let dest = if lines {
        dest
    } else {
        let merged = s.doc()?.wb.sheet(dest_sheet).and_then(|sh| sh.merge_at(dest.start)).filter(|m| *m == dest);
        if dest.is_single() || merged.is_some() { RangeRef::new(dest.start, dest.start.offset_clamped(h as i64 - 1, w as i64 - 1)) } else { dest }
    };
    if !lines && dest.count() > PAINT_CELLS {
        return Err(bad("edit.formatPainter", "the range is too large to paint cell by cell"));
    }
    let source_of = move |c: CellRef| {
        CellRef::new(src.start.row + (c.row.saturating_sub(dest.start.row)) % h, src.start.col + (c.col.saturating_sub(dest.start.col)) % w)
    };
    edit(s, |cx| {
        let mut styles: std::collections::HashMap<StyleId, StyleId> = std::collections::HashMap::new();
        let mut map = |cx: &mut Ctx<'_>, st: StyleId| -> StyleId {
            if same {
                return st;
            }
            *styles.entry(st).or_insert_with(|| cx.wb.styles.intern(wb.styles.get(st).clone()))
        };
        let restyle = |cx: &mut Ctx<'_>, c: CellRef, new: StyleId| -> Result<()> {
            let old = cx.wb.sheet(dest_sheet).map(|sh| sh.style_id(c)).unwrap_or_default();
            // A checkbox is the cell's content: the painter neither brings one nor takes one away.
            let new = cx.wb.styles.keep_checkbox(old, new);
            if old != new {
                crate::cmd::format::restyle_runs(cx, dest_sheet, c, old, new)?;
                cx.sheet_mut(dest_sheet)?.set_style(c, new);
                cx.touch(dest_sheet, c);
            }
            Ok(())
        };
        if lines {
            // Whole rows or columns: the lines' own format, and the cells already in them.
            let rows = dest.is_full_rows() && !dest.is_full_cols();
            let (first, last) = if rows { (dest.start.row, dest.end.row) } else { (dest.start.col, dest.end.col) };
            for line in first..=last {
                let from = if rows { CellRef::new(line, src.start.col) } else { CellRef::new(src.start.row, line) };
                let src_cell = if rows {
                    CellRef::new(src.start.row + (line - first) % h, from.col)
                } else {
                    CellRef::new(from.row, src.start.col + (line - first) % w)
                };
                let st = map(cx, src_sheet.style_id(src_cell));
                let st = cx.wb.styles.with_checkbox(st, false);
                let sh = cx.sheet_mut(dest_sheet)?;
                let infos = if rows { &mut sh.rows } else { &mut sh.cols };
                infos.entry(line).or_default().style = Some(st);
            }
            let cells: Vec<CellRef> = cx.wb.sheet(dest_sheet).map(|sh| sh.cells.iter_range(dest).map(|(c, _)| c).collect()).unwrap_or_default();
            for c in cells {
                let st = map(cx, src_sheet.style_id(source_of(c)));
                restyle(cx, c, st)?;
            }
        } else {
            // Merged areas in the way come apart; the source's merged areas are repeated.
            let sh = cx.sheet_mut(dest_sheet)?;
            sh.merges.retain(|m| !m.intersects(&dest));
            for c in dest.iter() {
                if cx.wb.sheet(dest_sheet).is_some_and(|sh| crate::cmd::format::filtered_out(sh, c.row)) {
                    continue;
                }
                // A cell of a table gives the look its table style shows (WPS, Excel): its fill,
                // colours and borders are painted as the target's own format.
                let from = source_of(c);
                let st = match src_sheet.table_at(from) {
                    Some(t) => cx.wb.styles.intern(crate::tables::table_cell_style(wb, sheet, t, from)),
                    None => map(cx, src_sheet.style_id(from)),
                };
                restyle(cx, c, st)?;
            }
            let mut merges = Vec::new();
            for m in src_sheet.merges.iter().filter(|m| src.contains(m.start) && src.contains(m.end)) {
                let (dr, dc) = (m.start.row - src.start.row, m.start.col - src.start.col);
                let mut row = dest.start.row;
                while row <= dest.end.row {
                    let mut col = dest.start.col;
                    while col <= dest.end.col {
                        let start = CellRef::new(row + dr, col + dc);
                        let end = start.offset_clamped(i64::from(m.height()) - 1, i64::from(m.width()) - 1);
                        let placed = RangeRef::new(start, end);
                        if dest.contains(placed.start) && dest.contains(placed.end) {
                            merges.push(placed);
                        }
                        col = col.saturating_add(w);
                        if col == u32::MAX {
                            break;
                        }
                    }
                    row = row.saturating_add(h);
                    if row == u32::MAX {
                        break;
                    }
                }
            }
            cx.sheet_mut(dest_sheet)?.merges.extend(merges);
            if dest.height() <= 100_000 {
                cx.fit_rows.extend((dest.start.row..=dest.end.row).map(|row| (dest_sheet, row)));
            }
        }
        cx.structural = true;
        *cx.sel = crate::Selection::range(dest);
        Ok(())
    })
}

/// Most cells a drag moves at once (whole rows and columns count their used cells only).
const MOVE_MAX_CELLS: u64 = 4_000_000;

/// A drag of the selection's border (WPS, Excel), as one step: a move is a cut and a paste at
/// `to` (formulas elsewhere follow the cells), with the block's merged areas, notes, links,
/// conditional formats, validation and tables moved along; `copy` pastes a copy instead; `insert`
/// first makes room at `to` (Insert Cut Cells): the cells there move down or right, and the cells
/// left behind close up. Whole rows and columns keep their heights and widths.
fn move_cells(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "edit.moveCells";
    let sheet = s.doc()?.wb.active_sheet;
    if target_sheet(s, p)? != sheet {
        return Err(bad(CMD, "cells move on the active sheet"));
    }
    let from = match str_param(p, "from") {
        Some(text) => RangeRef::parse(split_sheet(text).1).ok_or_else(|| bad(CMD, format!("not a range: `{text}`")))?,
        None => {
            let d = s.doc()?;
            if d.selection.ranges.len() > 1 {
                return Err(EngineError::Other("This action won't work on multiple selections.".into()));
            }
            d.selection.current()
        }
    };
    let rows = from.is_full_rows() && !from.is_full_cols();
    let cols = from.is_full_cols() && !from.is_full_rows();
    if from.is_full_rows() && from.is_full_cols() {
        return Err(bad(CMD, "the whole sheet can't be moved"));
    }
    let to = str_param(p, "to").ok_or_else(|| bad(CMD, "missing `to`"))?;
    let to = split_sheet(to).1;
    let to = CellRef::parse(to).or_else(|| RangeRef::parse(to).map(|r| r.start)).ok_or_else(|| bad(CMD, format!("not a cell: `{to}`")))?;
    let to = if rows {
        CellRef::new(to.row, 0)
    } else if cols {
        CellRef::new(0, to.col)
    } else {
        to
    };
    let copy = bool_param(p, "copy").unwrap_or(false);
    let insert = match str_param(p, "insert") {
        None => None,
        Some("down") => Some(kition_sheet_formula::adjust::Axis::Rows),
        Some("right") => Some(kition_sheet_formula::adjust::Axis::Cols),
        Some(other) => return Err(bad(CMD, format!("`insert` is down or right, not `{other}`"))),
    };
    // Whole lines make room as lines.
    let insert = match insert {
        Some(_) if rows => Some(kition_sheet_formula::adjust::Axis::Rows),
        Some(_) if cols => Some(kition_sheet_formula::adjust::Axis::Cols),
        other => other,
    };
    let (h, w) = (from.height(), from.width());
    if to.offset(i64::from(h) - 1, i64::from(w) - 1).is_none() {
        return Err(EngineError::Other("The cells can't be moved past the edge of the sheet.".into()));
    }
    let used = s.doc()?.wb.sheet(sheet).and_then(|sh| sh.used_range());
    let cells = if rows || cols { used.and_then(|u| u.intersection(&from)).map_or(0, |r| r.count()) } else { from.count() };
    if cells > MOVE_MAX_CELLS {
        return Err(EngineError::Other("The selection is too large to move.".into()));
    }
    // Nothing moves onto itself.
    if to == from.start && insert.is_none() {
        return Ok(json!({"range": from.a1()}));
    }
    let landed = match insert {
        None => {
            move_block(s, sheet, from, to, copy, rows, cols)?;
            RangeRef::new(to, to.offset_clamped(i64::from(h) - 1, i64::from(w) - 1))
        }
        Some(axis) => insert_block(s, sheet, from, to, copy, axis, rows, cols)?,
    };
    // The block lands selected; a drag ends copy mode (Excel).
    let range = if rows {
        RangeRef::rows(landed.start.row, landed.end.row)
    } else if cols {
        RangeRef::cols(landed.start.col, landed.end.col)
    } else {
        landed
    };
    s.clipboard = None;
    if let Ok(d) = s.doc_mut() {
        d.selection = crate::Selection { active: range.start, anchor: range.start, ranges: vec![range] };
    }
    Ok(json!({"range": range.a1()}))
}

/// Moves (or copies) `from` so its top-left is at `to`, overwriting what is there.
fn move_block(s: &mut Session, sheet: usize, from: RangeRef, to: CellRef, copy: bool, rows: bool, cols: bool) -> Result<()> {
    let delta = (i64::from(to.row) - i64::from(from.start.row), i64::from(to.col) - i64::from(from.start.col));
    let lines: Vec<(u32, Option<kition_sheet_model::LineInfo>)> = {
        let sh = s.doc()?.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
        if rows {
            (from.start.row..=from.end.row).map(|r| (r, sh.rows.get(&r).copied())).collect()
        } else if cols {
            (from.start.col..=from.end.col).map(|c| (c, sh.cols.get(&c).copied())).collect()
        } else {
            vec![]
        }
    };
    if !copy {
        // What belongs to the block's cells goes with it: merged areas, notes, links, conditional
        // formats, validation, tables (the ranges wholly inside it).
        let e = kition_sheet_formula::adjust::Edit::Move { from, to_row: to.row, to_col: to.col };
        edit(s, |cx| {
            super::sheet::shift_cell_features(cx.sheet_mut(sheet)?, &e);
            Ok(())
        })?;
    }
    let saved = s.clipboard.take();
    let pasted = copy_impl(s, &json!({ "range": from.a1() }), !copy).and_then(|_| paste_special(s, &json!({ "what": "all", "at": to.a1() })));
    s.clipboard = saved;
    pasted?;
    // Whole rows and columns take their heights (widths) and formats with them.
    if rows || cols {
        edit(s, |cx| {
            let sh = cx.sheet_mut(sheet)?;
            let map = if rows { &mut sh.rows } else { &mut sh.cols };
            let shift = if rows { delta.0 } else { delta.1 };
            if !copy {
                for (line, _) in &lines {
                    map.remove(line);
                }
            }
            for (line, info) in &lines {
                let Some(dest) = u32::try_from(i64::from(*line) + shift).ok() else { continue };
                match info {
                    Some(info) => {
                        map.insert(dest, *info);
                    }
                    None => {
                        map.remove(&dest);
                    }
                }
            }
            cx.structural = true;
            Ok(())
        })?;
    }
    Ok(())
}

/// Shift-drag (Insert Cut Cells): makes room at `to` along `axis`, moves (or copies) the block
/// there, and for a move closes up the place it left. Answers where the block landed.
#[allow(clippy::too_many_arguments)]
fn insert_block(
    s: &mut Session,
    sheet: usize,
    from: RangeRef,
    to: CellRef,
    copy: bool,
    axis: kition_sheet_formula::adjust::Axis,
    rows: bool,
    cols: bool,
) -> Result<RangeRef> {
    use kition_sheet_formula::adjust::Axis;
    let (h, w) = (from.height(), from.width());
    let down = axis == Axis::Rows;
    // The room made at `to`: the block's size, in the destination's columns (rows).
    let room = RangeRef::new(to, to.offset_clamped(i64::from(h) - 1, i64::from(w) - 1));
    // Cells in the room's band at or past `to` move by the block's size; the source too when it
    // lies there. A source only partly in the band would be split.
    let (band_a, band_b, src_a, src_b) = if down {
        (room.start.col, room.end.col, from.start.col, from.end.col)
    } else {
        (room.start.row, room.end.row, from.start.row, from.end.row)
    };
    let in_band = rows || cols || (src_a >= band_a && src_b <= band_b);
    let overlaps = src_b >= band_a && src_a <= band_b;
    let past = if down { from.start.row >= to.row } else { from.start.col >= to.col };
    if !rows && !cols && overlaps && !in_band && (past || !copy) {
        return Err(EngineError::Other("That command cannot be used on overlapping selections.".into()));
    }
    // Dropping the block where it already is changes nothing.
    let aligned = if down { room.start.col == from.start.col } else { room.start.row == from.start.row };
    let (line, first, last) = if down { (to.row, from.start.row, from.end.row) } else { (to.col, from.start.col, from.end.col) };
    if !copy && in_band && aligned && line >= first && line <= last.saturating_add(1) {
        return Ok(from);
    }
    if rows {
        s.execute("row.insert", json!({ "rows": format!("{}:{}", to.row + 1, to.row + h) }))?;
    } else if cols {
        s.execute("column.insert", json!({ "cols": RangeRef::cols(to.col, to.col + w - 1).a1() }))?;
    } else {
        s.execute("cell.insert", json!({ "range": room.a1(), "shift": if down { "down" } else { "right" } }))?;
    }
    let moved = if in_band && past {
        let (dr, dc) = if down { (i64::from(h), 0) } else { (0, i64::from(w)) };
        match (from.start.offset(dr, dc), from.end.offset(dr, dc)) {
            (Some(a), Some(b)) => RangeRef::new(a, b),
            _ => return Err(EngineError::Other("The cells can't be moved past the edge of the sheet.".into())),
        }
    } else {
        from
    };
    let (moved, room) = if rows {
        (RangeRef::rows(moved.start.row, moved.end.row), RangeRef::rows(room.start.row, room.end.row))
    } else if cols {
        (RangeRef::cols(moved.start.col, moved.end.col), RangeRef::cols(room.start.col, room.end.col))
    } else {
        (moved, room)
    };
    move_block(s, sheet, moved, room.start, copy, rows, cols)?;
    if copy {
        return Ok(room);
    }
    // The place the block left closes up.
    if rows {
        s.execute("row.delete", json!({ "rows": format!("{}:{}", moved.start.row + 1, moved.end.row + 1) }))?;
    } else if cols {
        s.execute("column.delete", json!({ "cols": RangeRef::cols(moved.start.col, moved.end.col).a1() }))?;
    } else {
        s.execute("cell.delete", json!({ "range": moved.a1(), "shift": if down { "up" } else { "left" } }))?;
    }
    // The room shifts back when the closed-up place was before it in the band.
    let before = in_band && if down { moved.end.row < room.start.row } else { moved.end.col < room.start.col };
    Ok(if before {
        let (dr, dc) = if down { (-i64::from(h), 0) } else { (0, -i64::from(w)) };
        RangeRef::new(room.start.offset_clamped(dr, dc), room.end.offset_clamped(dr, dc))
    } else {
        room
    })
}

fn begin_edit(s: &mut Session, _: &Json) -> Result<Json> {
    s.ui_requests.push(crate::UiRequest::EditCell(None));
    ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_array_entry() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "B2", "values": [[1], [2]]})).unwrap();
        s.execute("cell.set", json!({"cell": "C4", "input": "old"})).unwrap();
        s.execute("select.set", json!({"range": "C2:C4"})).unwrap();
        s.execute("cell.set", json!({"cell": "C2", "input": "=B2:B3*2", "array": true})).unwrap();
        let get = |s: &mut Session, a: &str| s.execute("cell.get", json!({"cell": a})).unwrap();
        assert_eq!(get(&mut s, "C3")["value"], json!(4.0));
        // Past the result Excel shows #N/A; the old content of the range is replaced.
        assert_eq!(get(&mut s, "C4")["text"], "#N/A");
        let bar = s.doc().unwrap().wb.active().unwrap().cell(CellRef::parse("C2").unwrap()).map(|c| c.bar_text());
        assert_eq!(bar.as_deref(), Some("{=B2:B3*2}"));
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("hello", "h*o"));
        assert!(wildcard("hello", "h?llo"));
        assert!(!wildcard("hello", "h?lo"));
        assert!(wildcard("a*b", "a~*b"));
        assert!(!wildcard("axb", "a~*b"));
        assert!(wildcard("", "*"));
    }

    #[test]
    fn tsv() {
        assert_eq!(parse_tsv("a\tb\n1\t2\n"), vec![vec!["a", "b"], vec!["1", "2"]]);
        assert_eq!(parse_tsv("\"x\ty\"\tz"), vec![vec!["x\ty", "z"]]);
        assert_eq!(parse_tsv("\"he said \"\"hi\"\"\""), vec![vec!["he said \"hi\""]]);
    }

    #[test]
    fn paste_special_operations_keep_text_errors_and_formulas() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute(
            "range.setValues",
            json!({"range": "A1", "values": [["'123", "Total", "=NA()", "=B3*2", 5], [1, null, null, null, null], [null, 4, null, null, null]]}),
        )
        .unwrap();
        let get = |s: &mut Session, a: &str| s.execute("cell.get", json!({"cell": a})).unwrap();
        assert_eq!(get(&mut s, "A1")["value"], "123");
        s.execute("edit.copy", json!({"range": "A2"})).unwrap();
        s.execute("select.set", json!({"range": "A1:E1"})).unwrap();
        s.execute("edit.pasteSpecial", json!({"what": "values", "operation": "multiply"})).unwrap();
        assert_eq!(get(&mut s, "A1")["value"], json!(123.0));
        assert_eq!(get(&mut s, "B1")["value"], "Total");
        assert_eq!(get(&mut s, "C1")["value"]["error"], "#N/A");
        assert_eq!(get(&mut s, "D1")["formula"], "=(B3*2)*1");
        assert_eq!(get(&mut s, "D1")["value"], json!(8.0));
        assert_eq!(get(&mut s, "E1")["value"], json!(5.0));
    }

    /// Typing a date into a column of the default width widens it (WPS W-E6; walkthrough D10),
    /// in the same undo step as the entry; a set width, text, a long number and an entry without
    /// `widen` (an agent's, a paste) leave the column as it is.
    #[test]
    fn a_typed_date_widens_its_column_in_the_same_step() {
        let mut s = Session::new();
        s.new_workbook();
        let width = |s: &Session, col: u32| s.doc().unwrap().wb.active().unwrap().col_width(col);
        let default = width(&s, 0);
        s.execute("cell.set", json!({"cell": "A1", "input": "2026-11-27", "widen": true})).unwrap();
        assert!(width(&s, 0) > default, "the date's column widened: {}", width(&s, 0));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(width(&s, 0), default, "one undo takes the value and the width back");
        assert_eq!(s.execute("cell.get", json!({"cell": "A1"})).unwrap()["value"], Json::Null);
        s.execute("cell.set", json!({"cell": "B1", "input": "2026-11-27"})).unwrap();
        assert_eq!(width(&s, 1), default, "without widen the column stays");
        s.execute("cell.set", json!({"cell": "C1", "input": "12345678901", "widen": true})).unwrap();
        assert!(width(&s, 2) > default, "11 digits widen");
        s.execute("cell.set", json!({"cell": "D1", "input": "123456789012", "widen": true})).unwrap();
        assert_eq!(width(&s, 3), default, "12 digits stay in scientific notation");
        s.execute("cell.set", json!({"cell": "E1", "input": "1234.56789", "widen": true})).unwrap();
        assert_eq!(width(&s, 4), default, "decimals are shown fewer, not widened");
        s.execute("cell.set", json!({"cell": "F1", "input": "a long piece of text", "widen": true})).unwrap();
        assert_eq!(width(&s, 5), default, "text overflows instead");
        s.execute("column.setWidth", json!({"cols": "G", "width": 40})).unwrap();
        let set = width(&s, 6);
        s.execute("cell.set", json!({"cell": "G1", "input": "2026-11-27", "widen": true})).unwrap();
        assert_eq!(width(&s, 6), set, "a width that was set stays");
        s.execute("cell.set", json!({"cell": "H1", "input": "5", "widen": true})).unwrap();
        assert_eq!(width(&s, 7), default, "what fits changes nothing");
    }

    /// Home, Cmd+Home and Cmd+End (WPS W-B9, W-B11; walkthrough C6), Shift extending.
    #[test]
    fn select_move_goes_to_the_row_start_the_first_and_the_last_cell() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "B2", "values": [[1, 2], [3, 4]]})).unwrap();
        s.execute("cell.set", json!({"cell": "J61", "input": "x"})).unwrap();
        s.execute("select.set", json!({"range": "G7"})).unwrap();
        let moved = |s: &mut Session, p: Json| s.execute("select.move", p).unwrap();
        assert_eq!(moved(&mut s, json!({"to": "rowStart"}))["selection"], "A7");
        assert_eq!(moved(&mut s, json!({"to": "lastCell"}))["selection"], "J61");
        assert_eq!(moved(&mut s, json!({"to": "sheetStart"}))["selection"], "A1");
        s.execute("select.set", json!({"range": "G7"})).unwrap();
        assert_eq!(moved(&mut s, json!({"to": "rowStart", "extend": true}))["selection"], "A7:G7");
        s.execute("select.set", json!({"range": "B3"})).unwrap();
        let r = moved(&mut s, json!({"to": "lastCell", "extend": true}));
        assert_eq!((r["selection"].as_str(), r["active"].as_str()), (Some("B3:J61"), Some("B3")));
        assert_eq!(moved(&mut s, json!({"to": "sheetStart", "extend": true}))["selection"], "A1:B3");
        assert!(s.execute("select.move", json!({"to": "end"})).is_err());
    }

    #[test]
    fn replace_case_insensitive() {
        assert_eq!(replace_ci("Apple apple APPLE", "apple", "pear"), "pear pear pear");
    }
}
