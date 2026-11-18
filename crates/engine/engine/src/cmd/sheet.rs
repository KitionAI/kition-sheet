//! Worksheets and structure: insert/delete rows, columns and cells; sheet tabs.

use std::sync::Arc;

use kition_sheet_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use kition_sheet_formula::adjust::{Axis, Edit};
use kition_sheet_model::{Anchor, AnchorMode, Sheet, StyleId, Visibility};
use serde_json::{Value as Json, json};

use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "home.insertRows",
            "Insert Sheet Rows",
            ["Home", "Cells", "Insert"],
            Some("Ctrl+Shift+="),
            "{rows?: \"3:5\", count?, below?: bool}",
            has_doc,
            |s, p| insert_lines(s, p, Axis::Rows)
        ),
        cmd!(
            "home.insertColumns",
            "Insert Sheet Columns",
            ["Home", "Cells", "Insert"],
            None,
            "{cols?: \"B:C\", count?, right?: bool}",
            has_doc,
            |s, p| insert_lines(s, p, Axis::Cols)
        ),
        cmd!("home.deleteRows", "Delete Sheet Rows", ["Home", "Cells", "Delete"], Some("Cmd+-"), "{rows?}", has_doc, |s, p| delete_lines(
            s,
            p,
            Axis::Rows
        )),
        cmd!("home.deleteColumns", "Delete Sheet Columns", ["Home", "Cells", "Delete"], None, "{cols?}", has_doc, |s, p| delete_lines(
            s,
            p,
            Axis::Cols
        )),
        cmd!("home.insertCells", "Insert Cells…", ["Home", "Cells", "Insert"], None, "{range?, shift: right|down|row|column}", has_doc, insert_cells),
        cmd!("home.deleteCells", "Delete Cells…", ["Home", "Cells", "Delete"], None, "{range?, shift: left|up|row|column}", has_doc, delete_cells),
        cmd!("home.insertSheet", "Insert Sheet", ["Home", "Cells", "Insert"], Some("Shift+F11"), "{name?, before?: index}", has_doc, insert_sheet),
        cmd!("home.deleteSheet", "Delete Sheet", ["Home", "Cells", "Delete"], None, "{sheet?}", has_doc, delete_sheet),
        cmd!("sheet.rename", "Rename Sheet", ["Home", "Cells", "Format"], None, "{sheet?, name}", has_doc, rename_sheet),
        cmd!(
            "sheet.move",
            "Move or Copy Sheet…",
            ["Home", "Cells", "Format"],
            None,
            "{sheet?, to: index, copy?: bool} → index (and a copy's name)",
            has_doc,
            move_sheet
        ),
        cmd!("sheet.tabColor", "Tab Color", ["Home", "Cells", "Format"], None, "{sheet?, color: \"#FF0000\"|\"none\"}", has_doc, tab_color),
        cmd!("sheet.hide", "Hide Sheet", ["Home", "Cells", "Format", "Hide & Unhide"], None, "{sheet?}", has_doc, hide_sheet),
        cmd!(
            "sheet.unhide",
            "Unhide Sheet…",
            ["Home", "Cells", "Format", "Hide & Unhide"],
            None,
            "{sheet, activate?: bool (default true)}",
            has_doc,
            unhide_sheet
        ),
        cmd!(noundo "sheet.activate", "Activate Sheet", [], None, "{sheet: index | name}", has_doc, activate),
        cmd!(noundo "sheet.next", "Next Sheet", [], Some("Ctrl+PageDown"), "{}", has_doc, |s, _| step_sheet(s, 1)),
        cmd!(noundo "sheet.previous", "Previous Sheet", [], Some("Ctrl+PageUp"), "{}", has_doc, |s, _| step_sheet(s, -1)),
    ]
}

/// Most lines one menu insert adds (WPS's count box takes up to 1000 rows and 100 columns).
pub const MAX_INSERT_ROWS: u32 = 1000;
pub const MAX_INSERT_COLS: u32 = 100;

/// The blocks of lines an insert or delete works on, each `(first line, count)`, from the bottom
/// (right) up so that each edit leaves the lines of the next one where they were.
///
/// A `rows`/`cols`/`range` param names one block (`"3:5"`, `"B"`); without one, every area of the
/// selection is a block (rows/columns it covers, overlapping areas joined). `count` replaces each
/// block's own size; `below` (rows) or `right` (columns) inserts after the block instead of before.
/// With the whole sheet selected there are no rows or columns to insert or delete beside it.
fn line_blocks(s: &Session, p: &Json, axis: Axis, insert: bool) -> Result<Vec<(u32, u32)>> {
    let key = if axis == Axis::Rows { "rows" } else { "cols" };
    let ranges = match str_param(p, key).or_else(|| str_param(p, "range")) {
        // A single row number or column letter (`"3"`, `"B"`) stands for that whole line.
        Some(t) => {
            let t = split_sheet(t).1;
            vec![RangeRef::parse(t).or_else(|| RangeRef::parse(&format!("{t}:{t}"))).ok_or_else(|| bad(key, format!("not a range: {t}")))?]
        }
        None => {
            let ranges = s.doc()?.selection.ranges.clone();
            if ranges.iter().any(|r| r.is_full_rows() && r.is_full_cols()) {
                return Err(EngineError::Other("This can't be done with the whole sheet selected.".into()));
            }
            ranges
        }
    };
    let span = |r: &RangeRef| if axis == Axis::Rows { (r.start.row, r.end.row) } else { (r.start.col, r.end.col) };
    let limit = if axis == Axis::Rows { MAX_ROWS } else { MAX_COLS };
    let after = bool_param(p, if axis == Axis::Rows { "below" } else { "right" }).unwrap_or(false);
    let count = u32_param(p, "count");
    if count == Some(0) {
        return Err(bad("count", "insert or delete at least one line"));
    }
    // Join the areas' lines into runs.
    let mut spans: Vec<(u32, u32)> = ranges.iter().map(span).collect();
    spans.sort_unstable();
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for (a, b) in spans {
        match runs.last_mut() {
            Some(last) if a <= last.1.saturating_add(1) && !insert => last.1 = last.1.max(b),
            Some(last) if insert && a <= last.1 => last.1 = last.1.max(b),
            _ => runs.push((a, b)),
        }
    }
    let mut blocks: Vec<(u32, u32)> = runs
        .into_iter()
        .map(|(a, b)| {
            let n = count.unwrap_or(b - a + 1);
            if insert && after { (b.saturating_add(1), n) } else { (a, n) }
        })
        .filter(|&(at, _)| at < limit)
        .collect();
    if blocks.is_empty() {
        return Err(bad(key, "no rows or columns to change there"));
    }
    blocks.sort_unstable_by_key(|b| std::cmp::Reverse(b.0));
    Ok(blocks)
}

/// Bottom-right line of an anchor (the `to` marker of a two-cell anchor) and the offset of that
/// corner inside it, in points. Measured against `sh`'s current geometry.
fn anchor_corner(sh: &Sheet, a: &Anchor) -> (u32, u32, f32, f32) {
    let x2 = sh.col_left(a.cell.col) + a.dx as f64 + a.width.max(0.0) as f64;
    let y2 = sh.row_top(a.cell.row) + a.dy as f64 + a.height.max(0.0) as f64;
    let tc = sh.col_at(x2);
    let tr = sh.row_at(y2);
    (tc, tr, (x2 - sh.col_left(tc)) as f32, (y2 - sh.row_top(tr)) as f32)
}

/// Repositions one floating object's anchor for a structural insert/delete, honouring its
/// [`AnchorMode`]: absolute objects ("Don't move or size with cells") stay put; "move but don't
/// size" follows its cell and keeps its size; "move and size" is re-fitted to the lines it lands
/// on. `corner` is the pre-edit bottom-right line/offset (from [`anchor_corner`]); a line deleted
/// out from under an edge collapses that edge onto `at`. `sh` is the post-edit grid, used to
/// measure the re-fitted size.
fn shift_anchor(sh: &Sheet, a: Anchor, corner: (u32, u32, f32, f32), axis: Axis, at: u32, map_line: &impl Fn(u32) -> Option<u32>) -> Anchor {
    if a.mode == AnchorMode::Absolute {
        return a;
    }
    let main = if axis == Axis::Rows { a.cell.row } else { a.cell.col };
    // A deleted anchor line collapses onto the line that takes its place.
    let nmain = map_line(main).unwrap_or(at);
    let (nr, nc) = if axis == Axis::Rows { (nmain, a.cell.col) } else { (a.cell.row, nmain) };
    if a.mode == AnchorMode::MoveOnly {
        return Anchor { cell: CellRef::new(nr, nc), ..a };
    }
    // Move and size: re-fit to the lines the object lands on. A far edge whose line was deleted
    // collapses onto the near cell, shrinking the object.
    let (tc, tr, tdx, tdy) = corner;
    let far = if axis == Axis::Rows { tr } else { tc };
    let nfar = map_line(far).unwrap_or(nmain);
    let (nbr, nbc) = if axis == Axis::Rows { (nfar, tc) } else { (tr, nfar) };
    let x1 = sh.col_left(nc) + a.dx as f64;
    let y1 = sh.row_top(nr) + a.dy as f64;
    let xn = sh.col_left(nbc) + tdx as f64;
    let yn = sh.row_top(nbr) + tdy as f64;
    Anchor { cell: CellRef::new(nr, nc), dx: a.dx, dy: a.dy, width: (xn - x1).max(0.0) as f32, height: (yn - y1).max(0.0) as f32, ..a }
}

/// Shifts everything that has a position on the sheet for an insert/delete.
fn shift_sheet_features(sh: &mut Sheet, axis: Axis, at: u32, count: u32, insert: bool) {
    let map_line = |v: u32| -> Option<u32> {
        if insert {
            if v >= at { v.checked_add(count) } else { Some(v) }
        } else if v < at {
            Some(v)
        } else if v >= at + count {
            Some(v - count)
        } else {
            None
        }
    };
    let e = if insert { Edit::Insert { axis, at, count } } else { Edit::Delete { axis, at, count } };
    let map_range = |r: RangeRef| map_range(r, &e);
    let map_cell = |c: CellRef| -> Option<CellRef> {
        match axis {
            Axis::Rows => map_line(c.row).map(|r| CellRef::new(r, c.col)),
            Axis::Cols => map_line(c.col).map(|x| CellRef::new(c.row, x)),
        }
    };
    // Floating objects: snapshot their bottom-right corners against the pre-shift geometry, so a
    // re-fit after the shift measures against the grid the object actually lands on.
    let obj_corners: Vec<(u32, u32, f32, f32)> = sh
        .charts
        .iter()
        .map(|o| &o.anchor)
        .chain(sh.images.iter().map(|o| &o.anchor))
        .chain(sh.shapes.iter().map(|o| &o.anchor))
        .chain(sh.diagrams.iter().map(|o| &o.anchor))
        .map(|a| anchor_corner(sh, a))
        .collect();
    // Line info.
    let lines = if axis == Axis::Rows { &mut sh.rows } else { &mut sh.cols };
    let old = std::mem::take(lines);
    for (k, v) in old {
        if let Some(n) = map_line(k)
            && n < if axis == Axis::Rows { MAX_ROWS } else { MAX_COLS }
        {
            lines.insert(n, v);
        }
    }
    sh.merges = sh.merges.iter().filter_map(|m| map_range(*m)).filter(|m| !m.is_single()).collect();
    for cf in sh.cond_formats.iter_mut() {
        cf.ranges = cf.ranges.iter().filter_map(|r| map_range(*r)).collect();
    }
    sh.cond_formats.retain(|cf| !cf.ranges.is_empty());
    for dv in sh.validations.iter_mut() {
        dv.ranges = dv.ranges.iter().filter_map(|r| map_range(*r)).collect();
    }
    sh.validations.retain(|dv| !dv.ranges.is_empty());
    for t in sh.tables.iter_mut() {
        if let Some(r) = map_range(t.range) {
            // Columns inserted inside a table become new table columns.
            if axis == Axis::Cols && insert && at > t.range.start.col && at <= t.range.end.col {
                let idx = (at - t.range.start.col) as usize;
                for k in 0..count {
                    let name = format!("Column{}", t.columns.len() + 1);
                    t.columns.insert(
                        (idx + k as usize).min(t.columns.len()),
                        kition_sheet_model::TableColumn { name, totals: Default::default(), totals_label: None, formula: None },
                    );
                }
            }
            if axis == Axis::Cols && !insert {
                let lo = at.max(t.range.start.col);
                let hi = (at + count).min(t.range.end.col + 1);
                if hi > lo {
                    let a = (lo - t.range.start.col) as usize;
                    let b = (hi - t.range.start.col) as usize;
                    if b <= t.columns.len() {
                        t.columns.drain(a..b);
                    }
                }
            }
            t.range = r;
        } else {
            t.range = RangeRef::default();
        }
    }
    sh.tables.retain(|t| t.range != RangeRef::default() && !t.columns.is_empty());
    if let Some(af) = sh.autofilter.as_mut() {
        match map_range(af.range) {
            Some(r) => af.range = r,
            None => sh.autofilter = None,
        }
    }
    sh.cell_pictures =
        std::mem::take(&mut sh.cell_pictures).into_iter().filter_map(|(c, v)| map_cell(c).filter(CellRef::is_valid).map(|c| (c, v))).collect();
    sh.comments = std::mem::take(&mut sh.comments).into_iter().filter_map(|(c, v)| map_cell(c).map(|c| (c, v))).collect();
    sh.hyperlinks = std::mem::take(&mut sh.hyperlinks).into_iter().filter_map(|(c, v)| map_cell(c).map(|c| (c, v))).collect();
    // Floating objects follow their anchor lines per their [`AnchorMode`]. Corners were snapshotted
    // as charts ++ images ++ shapes, matching the three slices below. New anchors are computed
    // against the post-shift grid first (the re-fit needs its sizes), then written back.
    let anchored = |ui: usize, a: Anchor| -> Anchor { obj_corners.get(ui).map_or(a, |&corner| shift_anchor(sh, a, corner, axis, at, &map_line)) };
    let new_charts: Vec<Anchor> = sh.charts.iter().enumerate().map(|(i, o)| anchored(i, o.anchor)).collect();
    let new_images: Vec<Anchor> = sh.images.iter().enumerate().map(|(i, o)| anchored(new_charts.len() + i, o.anchor)).collect();
    let base = new_charts.len() + new_images.len();
    let new_shapes: Vec<Anchor> = sh.shapes.iter().enumerate().map(|(i, o)| anchored(base + i, o.anchor)).collect();
    let base = base + new_shapes.len();
    let new_diagrams: Vec<Anchor> = sh.diagrams.iter().enumerate().map(|(i, o)| anchored(base + i, o.anchor)).collect();
    for (o, a) in sh.charts.iter_mut().zip(new_charts) {
        o.anchor = a;
    }
    for (o, a) in sh.images.iter_mut().zip(new_images) {
        o.anchor = a;
    }
    for (o, a) in sh.shapes.iter_mut().zip(new_shapes) {
        o.anchor = a;
    }
    for (o, a) in sh.diagrams.iter_mut().zip(new_diagrams) {
        o.anchor = a;
    }
    for sp in sh.sparklines.iter_mut() {
        if let Some(c) = map_cell(sp.cell) {
            sp.cell = c;
        }
    }
    if let Some(pa) = sh.print.print_area {
        sh.print.print_area = map_range(pa);
    }
    // Frozen panes (WPS, Excel): lines inserted inside the frozen part freeze with it; deleting
    // frozen lines shrinks it, but never to nothing (the freeze stays on its first line).
    if let Some((rows, cols)) = sh.freeze {
        let frozen = if axis == Axis::Rows { rows } else { cols };
        let next = if frozen == 0 || at >= frozen {
            frozen
        } else if insert {
            frozen.saturating_add(count).min(if axis == Axis::Rows { MAX_ROWS - 1 } else { MAX_COLS - 1 })
        } else {
            (frozen - (frozen.min(at.saturating_add(count)) - at)).max(1)
        };
        sh.freeze = Some(if axis == Axis::Rows { (next, cols) } else { (rows, next) });
    }
}

/// Gives lines `at..at + count` (just inserted) their formats, as WPS does: inserted above (left)
/// they take the line before's, inserted below (right) the line after's; nothing on the first
/// line. Cell formats, the line's own format and size; not its values, merges, validation, links
/// or hidden state. Outline levels follow the group the lines land in (the lower of their
/// neighbours'); conditional formats covering the line before grow over the new lines.
/// New lines take the format of the line beside them but not its checkboxes (a checkbox is
/// content: WPS's new rows and columns are blank).
fn unbox_inserted(cx: &mut Ctx<'_>, sheet: usize, axis: Axis, at: u32, count: u32) -> Result<()> {
    let limit = if axis == Axis::Rows { MAX_ROWS } else { MAX_COLS };
    let last = at.saturating_add(count).min(limit).saturating_sub(1);
    let block = if axis == Axis::Rows { RangeRef::rows(at, last) } else { RangeRef::cols(at, last) };
    let Some(sh) = cx.wb.sheet(sheet) else { return Ok(()) };
    let styles = &cx.wb.styles;
    let cells: Vec<(CellRef, StyleId)> =
        sh.cells.iter_range(block).filter(|(_, c)| styles.get(c.style).checkbox).map(|(r, c)| (r, c.style)).collect();
    let infos = if axis == Axis::Rows { &sh.rows } else { &sh.cols };
    let lines: Vec<(u32, StyleId)> =
        infos.range(at..=last).filter_map(|(l, i)| i.style.filter(|st| styles.get(*st).checkbox).map(|st| (*l, st))).collect();
    for (c, st) in cells {
        let st = cx.wb.styles.with_checkbox(st, false);
        cx.sheet_mut(sheet)?.set_style(c, st);
    }
    for (l, st) in lines {
        let st = cx.wb.styles.with_checkbox(st, false);
        let sh = cx.sheet_mut(sheet)?;
        let infos = if axis == Axis::Rows { &mut sh.rows } else { &mut sh.cols };
        if let Some(info) = infos.get_mut(&l) {
            info.style = Some(st);
        }
    }
    Ok(())
}

fn format_inserted(sh: &mut Sheet, axis: Axis, at: u32, count: u32, from_after: bool) {
    let limit = if axis == Axis::Rows { MAX_ROWS } else { MAX_COLS };
    let end = at.saturating_add(count).min(limit);
    let source = if from_after { Some(end).filter(|&l| l < limit) } else { at.checked_sub(1) };
    let infos = if axis == Axis::Rows { &sh.rows } else { &sh.cols };
    let level = |l: Option<u32>| l.and_then(|l| infos.get(&l)).map_or(0, |i| i.outline);
    let outline = level(at.checked_sub(1)).min(level(Some(end)));
    let base = source.and_then(|l| infos.get(&l)).copied();
    if base.is_some() || outline > 0 {
        let info = kition_sheet_model::LineInfo { hidden: false, outline, collapsed: false, ..base.unwrap_or_default() };
        let map = if axis == Axis::Rows { &mut sh.rows } else { &mut sh.cols };
        for i in at..end {
            map.insert(i, info);
        }
    }
    if let Some(src) = source {
        let block = if axis == Axis::Rows { RangeRef::rows(src, src) } else { RangeRef::cols(src, src) };
        let styled: Vec<(u32, kition_sheet_model::StyleId)> = sh
            .cells
            .iter_range(block)
            .filter(|(_, c)| c.style != kition_sheet_model::StyleId::DEFAULT)
            .map(|(r, c)| (if axis == Axis::Rows { r.col } else { r.row }, c.style))
            .collect();
        // A line formatted across thousands of cells inserted a thousand times stays bounded.
        if (styled.len() as u64).saturating_mul((end - at) as u64) <= 2_000_000 {
            for i in at..end {
                for &(cross, style) in &styled {
                    let c = if axis == Axis::Rows { CellRef::new(i, cross) } else { CellRef::new(cross, i) };
                    sh.cells.set(c, kition_sheet_model::Cell { value: kition_sheet_core::Value::Empty, formula: None, style, rich: None });
                }
            }
        }
    }
    if let Some(before) = at.checked_sub(1) {
        for cf in sh.cond_formats.iter_mut() {
            for r in cf.ranges.iter_mut() {
                match axis {
                    Axis::Rows if r.end.row == before => r.end.row = end - 1,
                    Axis::Cols if r.end.col == before => r.end.col = end - 1,
                    _ => {}
                }
            }
        }
    }
}

/// Rewrites every formula reference for a structural edit on `target`: the cells' formulas,
/// defined names, and the formulas beside the cells on every sheet (chart series, conditional
/// formats, validations, sparklines, pivot table sources, tables' calculated columns).
fn rewrite(wb: &mut kition_sheet_model::Workbook, target: &str, e: &Edit) {
    super::edit::rewrite_all_formulas(wb, target, e);
    for si in 0..wb.sheets.len() {
        let host = wb.sheets.get(si).map(|s| s.name.clone()).unwrap_or_default();
        let Some(sh) = wb.sheet_mut(si) else { continue };
        adjust_feature_formulas(sh, |x| kition_sheet_formula::adjust::adjust(x, &host, target, e));
    }
}

/// Inserting or deleting cells moves the copied block away from the stored source range, so, like
/// Excel, it cancels copy/cut mode for this workbook (a later paste would take the wrong cells).
fn cancel_copy_mode(s: &mut Session) {
    let uid = s.doc().map(|d| d.uid).ok();
    if s.clipboard.as_ref().is_some_and(|c| Some(c.doc_uid) == uid) {
        s.clipboard = None;
    }
}

fn insert_lines(s: &mut Session, p: &Json, axis: Axis) -> Result<Json> {
    let blocks = line_blocks(s, p, axis, true)?;
    let sheet = target_sheet(s, p)?;
    let limit = if axis == Axis::Rows { MAX_ROWS } else { MAX_COLS };
    let used = s.doc()?.wb.sheet(sheet).and_then(|sh| sh.used_range());
    for &(at, count) in &blocks {
        check_lines_protection(s, p, axis, at, count, true)?;
        // Refuse to push data off the sheet.
        if let Some(u) = used {
            let last = if axis == Axis::Rows { u.end.row } else { u.end.col };
            if last >= at && last as u64 + count as u64 >= limit as u64 {
                return Err(EngineError::Other("To prevent possible loss of data, Excel cannot shift nonblank cells off of the worksheet.".into()));
            }
        }
        if at as u64 + count as u64 > limit as u64 {
            return Err(bad(if axis == Axis::Rows { "rows" } else { "cols" }, "the sheet has no room for that many lines"));
        }
    }
    let from_after = bool_param(p, if axis == Axis::Rows { "below" } else { "right" }).unwrap_or(false);
    let explicit = p.get(if axis == Axis::Rows { "rows" } else { "cols" }).is_some() || p.get("range").is_some();
    let out = edit(s, |cx| {
        let name = cx.wb.sheet(sheet).map(|s| s.name.clone()).unwrap_or_default();
        cx.protection_checked = true;
        for &(at, count) in &blocks {
            {
                let sh = cx.sheet_mut(sheet)?;
                match axis {
                    Axis::Rows => sh.cells.shift_rows(at, count as i64),
                    Axis::Cols => sh.cells.shift_cols(at, count as i64),
                }
                shift_sheet_features(sh, axis, at, count, true);
                format_inserted(sh, axis, at, count, from_after);
            }
            unbox_inserted(cx, sheet, axis, at, count)?;
            let e = Edit::Insert { axis, at, count };
            rewrite(&mut cx.wb, &name, &e);
            cx.log_structure(sheet, &e);
        }
        // The new lines are selected (WPS), across the columns (rows) the selection had.
        if let [(at, count)] = blocks.as_slice()
            && !explicit
            && cx.wb.active_sheet == sheet
        {
            let r = cx.sel.current();
            let last = at.saturating_add(count - 1).min(limit - 1);
            let n = match axis {
                Axis::Rows => RangeRef::new(CellRef::new(*at, r.start.col), CellRef::new(last, r.end.col)),
                Axis::Cols => RangeRef::new(CellRef::new(r.start.row, *at), CellRef::new(r.end.row, last)),
            };
            *cx.sel = crate::Selection::range(n);
        }
        cx.structural = true;
        Ok(Json::Null)
    })?;
    cancel_copy_mode(s);
    Ok(out)
}

/// The lines of `at..at + count` a delete takes: with a filter applied to the sheet, the rows it
/// hides inside its range stay (WPS, Excel delete only the rows in view), as runs from the bottom.
fn deleted_runs(sh: &Sheet, axis: Axis, at: u32, count: u32) -> Vec<(u32, u32)> {
    let end = at.saturating_add(count).min(if axis == Axis::Rows { MAX_ROWS } else { MAX_COLS });
    let kept = |l: u32| {
        axis == Axis::Rows
            && sh.autofilter.as_ref().is_some_and(|af| !af.criteria.is_empty() && l > af.range.start.row && l <= af.range.end.row)
            && sh.rows.get(&l).is_some_and(|i| i.hidden)
    };
    let mut runs: Vec<(u32, u32)> = Vec::new();
    if !(at..end).any(kept) {
        return vec![(at, end - at)];
    }
    for l in at..end {
        if kept(l) {
            continue;
        }
        match runs.last_mut() {
            Some((a, n)) if *a + *n == l => *n += 1,
            _ => runs.push((l, 1)),
        }
    }
    runs.reverse();
    runs
}

fn delete_lines(s: &mut Session, p: &Json, axis: Axis) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let blocks = line_blocks(s, p, axis, false)?;
    let runs: Vec<(u32, u32)> = {
        let d = s.doc()?;
        let sh = d.wb.sheet(sheet).ok_or_else(|| bad("sheet", "no such sheet"))?;
        blocks.iter().flat_map(|&(at, count)| deleted_runs(sh, axis, at, count)).collect()
    };
    for &(at, count) in &runs {
        check_lines_protection(s, p, axis, at, count, false)?;
    }
    let out = edit(s, |cx| {
        let name = cx.wb.sheet(sheet).map(|s| s.name.clone()).unwrap_or_default();
        cx.protection_checked = true;
        for &(at, count) in &runs {
            {
                let sh = cx.sheet_mut(sheet)?;
                let block = match axis {
                    Axis::Rows => RangeRef::rows(at, (at + count - 1).min(MAX_ROWS - 1)),
                    Axis::Cols => RangeRef::cols(at, (at + count - 1).min(MAX_COLS - 1)),
                };
                sh.take_cells(block);
                match axis {
                    Axis::Rows => sh.cells.shift_rows(at + count, -(count as i64)),
                    Axis::Cols => sh.cells.shift_cols(at + count, -(count as i64)),
                }
                shift_sheet_features(sh, axis, at, count, false);
            }
            let e = Edit::Delete { axis, at, count };
            rewrite(&mut cx.wb, &name, &e);
            cx.log_structure(sheet, &e);
        }
        cx.structural = true;
        Ok(Json::Null)
    })?;
    cancel_copy_mode(s);
    Ok(out)
}

fn insert_cells(s: &mut Session, p: &Json) -> Result<Json> {
    let shift = str_param(p, "shift").unwrap_or("down");
    let r = target_range(s, p)?;
    let sheet = target_sheet(s, p)?;
    match shift {
        "row" | "entireRow" => return insert_lines(s, &json!({"sheet":sheet,"rows": format!("{}:{}", r.start.row + 1, r.end.row + 1)}), Axis::Rows),
        "column" | "entireColumn" => return insert_lines(s, &json!({"sheet":sheet,"cols": RangeRef::cols(r.start.col, r.end.col).a1()}), Axis::Cols),
        _ => {}
    }
    let e = Edit::InsertCells { axis: if shift == "right" { Axis::Cols } else { Axis::Rows }, range: r };
    let right = shift == "right";
    let out = edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        if right {
            sh.cells.shift_cols_in_rows(r.start.row, r.end.row, r.start.col, r.width() as i64);
        } else {
            sh.cells.shift_rows_in_cols(r.start.col, r.end.col, r.start.row, r.height() as i64);
        }
        shift_cell_features(sh, &e);
        let name = cx.wb.sheet(sheet).map(|s| s.name.clone()).unwrap_or_default();
        rewrite(&mut cx.wb, &name, &e);
        cx.log_structure(sheet, &e);
        cx.structural = true;
        Ok(Json::Null)
    })?;
    cancel_copy_mode(s);
    Ok(out)
}

fn delete_cells(s: &mut Session, p: &Json) -> Result<Json> {
    let shift = str_param(p, "shift").unwrap_or("up");
    let r = target_range(s, p)?;
    let sheet = target_sheet(s, p)?;
    match shift {
        "row" | "entireRow" => return delete_lines(s, &json!({"sheet":sheet,"rows": format!("{}:{}", r.start.row + 1, r.end.row + 1)}), Axis::Rows),
        "column" | "entireColumn" => return delete_lines(s, &json!({"sheet":sheet,"cols": RangeRef::cols(r.start.col, r.end.col).a1()}), Axis::Cols),
        _ => {}
    }
    let e = Edit::DeleteCells { axis: if shift == "left" { Axis::Cols } else { Axis::Rows }, range: r };
    let left = shift == "left";
    let out = edit(s, |cx| {
        let name = cx.wb.sheet(sheet).map(|s| s.name.clone()).unwrap_or_default();
        {
            let sh = cx.sheet_mut(sheet)?;
            sh.take_cells(r);
            if left {
                sh.cells.shift_cols_in_rows(r.start.row, r.end.row, r.end.col + 1, -(r.width() as i64));
            } else {
                sh.cells.shift_rows_in_cols(r.start.col, r.end.col, r.end.row + 1, -(r.height() as i64));
            }
            shift_cell_features(sh, &e);
        }
        // References into the deleted cells become #REF!; the rest move.
        rewrite(&mut cx.wb, &name, &e);
        cx.log_structure(sheet, &e);
        cx.structural = true;
        Ok(Json::Null)
    })?;
    cancel_copy_mode(s);
    Ok(out)
}

fn insert_sheet(s: &mut Session, p: &Json) -> Result<Json> {
    if s.doc()?.wb.protected_structure {
        return Err(EngineError::Other("The workbook is protected and cannot be changed.".into()));
    }
    let name = match str_param(p, "name") {
        Some(n) => n.to_string(),
        None => s.doc()?.wb.next_sheet_name(),
    };
    s.doc()?.wb.check_sheet_name(&name, None).map_err(EngineError::Other)?;
    let d = s.doc_mut()?;
    let before = p.get("before").and_then(Json::as_u64).map(|v| v as usize);
    // The sheet that was in view remembers its selection for when it shows again.
    let shown = d.wb.active_sheet;
    if d.sheet_selections.len() < d.wb.sheets.len() {
        d.sheet_selections.resize(d.wb.sheets.len(), crate::Selection::default());
    }
    if let Some(slot) = d.sheet_selections.get_mut(shown) {
        *slot = d.selection.clone();
    }
    let idx = edit_doc(d, |cx| {
        // Excel inserts before the active sheet; the "+" button appends after it.
        let at = before.unwrap_or(cx.wb.active_sheet + 1).min(cx.wb.sheets.len());
        cx.wb.sheets.insert(at, Arc::new(Sheet::new(name.clone())));
        // Names scoped to later sheets shift.
        for n in cx.wb.names.iter_mut() {
            if let Some(sc) = n.scope.as_mut()
                && *sc >= at
            {
                *sc += 1;
            }
        }
        cx.wb.active_sheet = at;
        *cx.sel = crate::Selection::default();
        cx.structural = true;
        Ok(at)
    })?;
    d.sheet_selections.insert(idx.min(d.sheet_selections.len()), crate::Selection::default());
    Ok(json!({"index": idx, "name": name}))
}

fn edit_doc<R>(d: &mut crate::DocState, f: impl FnOnce(&mut Ctx) -> Result<R>) -> Result<R> {
    super::commit(d, f)
}

/// Sheet protection for inserting or deleting rows/columns: the protection must allow it, and
/// deleted lines must not hold locked cells.
fn check_lines_protection(s: &Session, p: &Json, axis: Axis, at: u32, count: u32, insert: bool) -> Result<()> {
    let sheet = target_sheet(s, p)?;
    let wb = &s.doc()?.wb;
    let Some(sh) = wb.sheet(sheet) else { return Ok(()) };
    let Some(pr) = &sh.protection else { return Ok(()) };
    let allowed = match (axis, insert) {
        (Axis::Rows, true) => pr.insert_rows,
        (Axis::Rows, false) => pr.delete_rows,
        (Axis::Cols, true) => pr.insert_columns,
        (Axis::Cols, false) => pr.delete_columns,
    };
    let locked = |st: kition_sheet_model::StyleId| wb.styles.get(st).protection.locked;
    let refuse = || EngineError::Other(PROTECTED.into());
    if !allowed {
        return Err(refuse());
    }
    if insert {
        return Ok(());
    }
    let end = at.saturating_add(count - 1).min(if axis == Axis::Rows { MAX_ROWS - 1 } else { MAX_COLS - 1 });
    let infos = if axis == Axis::Rows { &sh.rows } else { &sh.cols };
    if (at..=end).any(|i| locked(infos.get(&i).and_then(|x| x.style).unwrap_or_default())) {
        return Err(refuse());
    }
    let block = if axis == Axis::Rows { RangeRef::rows(at, end) } else { RangeRef::cols(at, end) };
    if sh.cells.iter_range(block).any(|(_, cell)| locked(cell.style)) {
        return Err(refuse());
    }
    Ok(())
}

fn delete_sheet(s: &mut Session, p: &Json) -> Result<Json> {
    let i = target_sheet(s, p)?;
    let d = s.doc()?;
    if d.wb.protected_structure {
        return Err(EngineError::Other("The workbook is protected and cannot be changed.".into()));
    }
    // The last visible sheet stays; a hidden sheet can go as long as a visible one is left.
    let visible = |sh: &Arc<Sheet>| sh.visibility == Visibility::Visible;
    if d.wb.sheets.get(i).is_some_and(visible) && d.wb.sheets.iter().filter(|sh| visible(sh)).count() <= 1 {
        return Err(EngineError::Other("A workbook must contain at least one visible worksheet.".into()));
    }
    let name = d.wb.sheet(i).map(|s| s.name.clone()).unwrap_or_default();
    let order: Vec<String> = d.wb.sheets.iter().map(|s| s.name.clone()).collect();
    let was_active = d.wb.active_sheet == i;
    let d = s.doc_mut()?;
    edit_doc(d, |cx| {
        cx.wb.sheets.remove(i);
        for si in 0..cx.wb.sheets.len() {
            let keys: Vec<(CellRef, Arc<kition_sheet_model::Formula>)> =
                cx.wb.sheets.get(si).map(|s| s.cells.iter().filter_map(|(c, x)| x.formula.clone().map(|f| (c, f))).collect()).unwrap_or_default();
            let Some(sh) = cx.wb.sheet_mut(si) else { continue };
            for (c, f) in keys {
                if let Some(e) = f.expr() {
                    let ne = kition_sheet_formula::adjust::delete_sheet(e.clone(), &name, &order);
                    if ne != e
                        && let Some(cell) = sh.cells.get_mut(c)
                    {
                        cell.formula = Some(rewritten(&f, ne));
                    }
                }
            }
        }
        // Charts, conditional formats, validations, sparklines, pivot tables and tables' calculated
        // columns that refer to the sheet turn #REF! like the cells' formulas.
        for si in 0..cx.wb.sheets.len() {
            if let Some(sh) = cx.wb.sheet_mut(si) {
                adjust_feature_formulas(sh, |e| kition_sheet_formula::adjust::delete_sheet(e, &name, &order));
            }
        }
        cx.wb.names.retain(|n| n.scope != Some(i));
        for n in cx.wb.names.iter_mut() {
            if let Some(sc) = n.scope.as_mut()
                && *sc > i
            {
                *sc -= 1;
            }
            // Names referring to the sheet become #REF!.
            if let Ok(e) = kition_sheet_formula::parse(&n.formula) {
                let ne = kition_sheet_formula::adjust::delete_sheet(e.clone(), &name, &order);
                if ne != e {
                    n.formula = kition_sheet_formula::print(&ne);
                }
            }
        }
        if cx.wb.active_sheet > i {
            cx.wb.active_sheet -= 1;
        } else if was_active {
            // The deleted sheet was in view: the next visible sheet on its right shows, else the
            // nearest on its left.
            let shows = |j: &usize| cx.wb.sheet(*j).is_some_and(|sh| sh.visibility == Visibility::Visible);
            let next = (i..cx.wb.sheets.len()).find(shows).or_else(|| (0..i).rev().find(shows)).unwrap_or(0);
            cx.wb.active_sheet = next;
        }
        cx.structural = true;
        Ok(())
    })?;
    if i < d.sheet_selections.len() {
        d.sheet_selections.remove(i);
    }
    if was_active {
        // The sheet now in view comes back with the selection it remembers.
        d.selection = d.sheet_selections.get(d.wb.active_sheet).cloned().unwrap_or_default();
    }
    ok()
}

fn rename_sheet(s: &mut Session, p: &Json) -> Result<Json> {
    let i = target_sheet(s, p)?;
    let Some(name) = str_param(p, "name").map(|n| n.trim().to_string()) else { return Err(bad("sheet.rename", "missing `name`")) };
    s.doc()?.wb.check_sheet_name(&name, Some(i)).map_err(EngineError::Other)?;
    if s.doc()?.wb.sheet(i).is_some_and(|sh| sh.name == name) {
        return ok();
    }
    edit(s, |cx| {
        let old = cx.wb.sheet(i).map(|s| s.name.clone()).unwrap_or_default();
        cx.sheet_mut(i)?.name = name.clone();
        for si in 0..cx.wb.sheets.len() {
            let keys: Vec<(CellRef, Arc<kition_sheet_model::Formula>)> =
                cx.wb.sheets.get(si).map(|s| s.cells.iter().filter_map(|(c, x)| x.formula.clone().map(|f| (c, f))).collect()).unwrap_or_default();
            let Some(sh) = cx.wb.sheet_mut(si) else { continue };
            for (c, f) in keys {
                if let Some(e) = f.expr() {
                    let ne = kition_sheet_formula::adjust::rename_sheet(e, &old, &name);
                    let text = kition_sheet_formula::print(&ne);
                    if text != f.text
                        && let Some(cell) = sh.cells.get_mut(c)
                    {
                        cell.formula = Some(rewritten(&f, ne));
                    }
                }
            }
            rename_sheet_features(sh, &old, &name);
        }
        for n in cx.wb.names.iter_mut() {
            if let Some(text) = renamed_refs(&n.formula, &old, &name) {
                n.formula = text;
            }
        }
        cx.structural = true;
        Ok(Json::Null)
    })
}

/// `text` (a formula, with or without its `=`, or a reference) with its references to sheet `old`
/// naming `new`; `None` when it refers to no such sheet or is no formula, so text that does not
/// change keeps its own spelling.
fn renamed_refs(text: &str, old: &str, new: &str) -> Option<String> {
    mapped_refs(text, |e| kition_sheet_formula::adjust::rename_sheet(e, old, new))
}

/// `text` (a formula, with or without its `=`, or a reference) mapped by `f`; `None` when it is
/// no formula or `f` leaves it as it was, so text that does not change keeps its own spelling.
fn mapped_refs(text: &str, f: impl Fn(kition_sheet_formula::Expr) -> kition_sheet_formula::Expr) -> Option<String> {
    let (eq, body) = match text.strip_prefix('=') {
        Some(body) => ("=", body),
        None => ("", text),
    };
    let e = kition_sheet_formula::parse(body).ok()?;
    let ne = f(e.clone());
    (ne != e).then(|| format!("{eq}{}", kition_sheet_formula::print(&ne)))
}

/// Every formula a sheet keeps beside its cells: chart series, conditional format and validation
/// formulas, sparkline and pivot table sources, and its tables' calculated columns and totals
/// formulas.
pub fn for_each_feature_formula(sh: &mut Sheet, mut f: impl FnMut(&mut String)) {
    for ch in sh.charts.iter_mut() {
        for se in ch.series.iter_mut() {
            f(&mut se.values);
            if let Some(t) = se.categories.as_mut() {
                f(t);
            }
            if let Some(t) = se.name.as_mut() {
                f(t);
            }
        }
    }
    for cf in sh.cond_formats.iter_mut() {
        match &mut cf.rule {
            kition_sheet_model::CfRule::CellIs { a, b, .. } => {
                f(a);
                if let Some(b) = b.as_mut() {
                    f(b);
                }
            }
            kition_sheet_model::CfRule::Expression { formula, .. } => f(formula),
            _ => {}
        }
    }
    for dv in sh.validations.iter_mut() {
        f(&mut dv.f1);
        if let Some(f2) = dv.f2.as_mut() {
            f(f2);
        }
    }
    for sp in sh.sparklines.iter_mut() {
        f(&mut sp.source);
    }
    for pt in sh.pivots.iter_mut() {
        f(&mut pt.source);
    }
    for t in sh.tables.iter_mut() {
        for c in t.columns.iter_mut() {
            if let Some(x) = c.formula.as_mut() {
                f(x);
            }
            // A totals label is a formula only when it starts with `=`.
            if let Some(x) = c.totals_label.as_mut().filter(|x| x.starts_with('=')) {
                f(x);
            }
        }
    }
}

/// Maps the references of every formula a sheet keeps beside its cells (see
/// [`for_each_feature_formula`]) with `map`.
pub(crate) fn adjust_feature_formulas(sh: &mut Sheet, map: impl Fn(kition_sheet_formula::Expr) -> kition_sheet_formula::Expr) {
    for_each_feature_formula(sh, |t| {
        if let Some(text) = mapped_refs(t, &map) {
            *t = text;
        }
    });
}

/// What else on a sheet names sheets in formulas or references, after sheet `old` became `new`:
/// the formulas beside its cells and links to places in the workbook.
fn rename_sheet_features(sh: &mut Sheet, old: &str, new: &str) {
    adjust_feature_formulas(sh, |e| kition_sheet_formula::adjust::rename_sheet(e, old, new));
    for link in sh.hyperlinks.values_mut() {
        if link.is_external() {
            continue;
        }
        let (hash, place) = match link.target.strip_prefix('#') {
            Some(place) => ("#", place),
            None => ("", link.target.as_str()),
        };
        if let Some(text) = renamed_refs(place, old, new) {
            link.target = format!("{hash}{text}");
        }
    }
}

/// Where each sheet goes when the sheet at `from` moves to `to` (the others close up behind it
/// and open before it).
fn moved_index(x: usize, from: usize, to: usize) -> usize {
    match x {
        x if x == from => to,
        x if from < x && x <= to => x - 1,
        x if to <= x && x < from => x + 1,
        x => x,
    }
}

/// `f` with the expression `e` (its text printed from it), still an array formula if it was one.
fn rewritten(f: &kition_sheet_model::Formula, e: kition_sheet_formula::Expr) -> Arc<kition_sheet_model::Formula> {
    Arc::new(kition_sheet_model::Formula { array: f.array, ..kition_sheet_model::Formula::from_expr(e) })
}

/// The name WPS gives a copy of sheet `base`: `base(2)`, or the next number free. A copy of a copy
/// counts on from the original's name (`Sheet1(2)` → `Sheet1(3)`, as `Sheet1 (2)` from Excel does),
/// and a long name is cut so the copy's keeps to 31 characters.
fn copy_name(wb: &kition_sheet_model::Workbook, base: &str) -> Result<String> {
    let stem = match base.strip_suffix(')').and_then(|b| b.rsplit_once('(')) {
        Some((head, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && !head.trim_end().is_empty() => head.trim_end(),
        _ => base,
    };
    for k in 2..100_000u32 {
        let tail = format!("({k})");
        let head: String = stem.chars().take(31usize.saturating_sub(tail.len())).collect();
        let name = format!("{}{tail}", head.trim_end());
        if wb.sheet_index(&name).is_none() {
            return Ok(name);
        }
    }
    Err(EngineError::Other("Too many copies of this sheet.".into()))
}

/// A name for the copy of table `name`: its name without the number it ends in, numbered from 2
/// (`Table1` → `Table2`, `Sales` → `Sales2`), free among the workbook's tables and names and `taken`.
fn table_copy_name(wb: &kition_sheet_model::Workbook, name: &str, taken: &[String]) -> String {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let stem = if stem.is_empty() { name } else { stem };
    let free = |n: &str| {
        wb.table(n).is_none() && !wb.names.iter().any(|d| d.name.eq_ignore_ascii_case(n)) && !taken.iter().any(|t| t.eq_ignore_ascii_case(n))
    };
    (2..100_000u32).map(|k| format!("{stem}{k}")).find(|n| free(n)).unwrap_or_else(|| format!("{name}_{}", taken.len() + 2))
}

/// A copy of `src` named `name`, as Excel and WPS make one: what refers to the sheet itself (its
/// formulas, conditional formats, validations, charts, sparklines, links) refers to the copy, and
/// references to other sheets stay; its tables get names of their own, which its formulas follow,
/// and its tables, charts, pictures and shapes ids of their own.
fn sheet_copy(wb: &kition_sheet_model::Workbook, src: &Sheet, name: &str) -> Result<Sheet> {
    let mut sh = src.clone();
    let base = src.name.clone();
    sh.name = name.to_string();
    // A copy is a sheet of its own: the workbook numbers it (`Workbook::number_sheets`).
    sh.sheet_id = 0;
    let mut next = wb.next_object_id();
    let mut id = || -> Result<u32> {
        let this = next.ok_or_else(object_id_exhausted)?;
        next = this.checked_add(1);
        Ok(this)
    };
    let mut tables: Vec<(String, String)> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for t in sh.tables.iter_mut() {
        t.id = id()?;
        let copy = table_copy_name(wb, &t.name, &taken);
        taken.push(copy.clone());
        tables.push((std::mem::replace(&mut t.name, copy.clone()), copy));
    }
    for c in sh.charts.iter_mut() {
        c.id = id()?;
    }
    for im in sh.images.iter_mut() {
        im.id = id()?;
    }
    for d in sh.diagrams.iter_mut() {
        d.id = id()?;
    }
    // A group's key is shared by its members: each key gets one new key.
    let mut keys = std::collections::HashMap::new();
    let mut shape_ids = std::collections::HashMap::new();
    for shape in sh.shapes.iter_mut() {
        let copied = id()?;
        shape_ids.insert(shape.id, copied);
        shape.id = copied;
        for key in shape.groups.iter_mut() {
            let copied = match keys.get(key) {
                Some(copied) => *copied,
                None => {
                    let copied = id()?;
                    keys.insert(*key, copied);
                    copied
                }
            };
            *key = copied;
        }
    }
    for shape in sh.shapes.iter_mut() {
        if let Some(connection) = &mut shape.connection {
            for endpoint in [&mut connection.start, &mut connection.end].into_iter().flatten() {
                if let Some(copied) = shape_ids.get(&endpoint.id) {
                    endpoint.id = *copied;
                }
            }
        }
    }
    let formulas: Vec<(CellRef, Arc<kition_sheet_model::Formula>)> = sh.cells.iter().filter_map(|(c, x)| x.formula.clone().map(|f| (c, f))).collect();
    for (c, f) in formulas {
        let Some(e) = f.expr() else { continue };
        let ne = kition_sheet_formula::adjust::copy_sheet_refs(e.clone(), &base, name, &tables);
        if ne != e
            && let Some(cell) = sh.cells.get_mut(c)
        {
            cell.formula = Some(rewritten(&f, ne));
        }
    }
    rename_sheet_features(&mut sh, &base, name);
    // Calculated columns name the copy's tables, like its cells' formulas.
    for_each_tables_formula(&mut sh, |t| {
        if let Some(text) = mapped_refs(t, |e| kition_sheet_formula::adjust::copy_sheet_refs(e, &base, name, &tables)) {
            *t = text;
        }
    });
    Ok(sh)
}

/// The calculated column and totals formulas of a sheet's tables.
fn for_each_tables_formula(sh: &mut Sheet, mut f: impl FnMut(&mut String)) {
    for t in sh.tables.iter_mut() {
        for c in t.columns.iter_mut() {
            if let Some(x) = c.formula.as_mut() {
                f(x);
            }
            if let Some(x) = c.totals_label.as_mut().filter(|x| x.starts_with('=')) {
                f(x);
            }
        }
    }
}

/// Moves a sheet to position `to` (counted after it leaves its place), or copies it there. The
/// active sheet stays the one it was (a moved sheet is not activated: dragging another sheet's tab
/// leaves the sheet in view), and each sheet's remembered selection goes with its sheet. A copy
/// (named as WPS names one: `Sheet1(2)`) becomes the active sheet, at A1.
fn move_sheet(s: &mut Session, p: &Json) -> Result<Json> {
    let i = target_sheet(s, p)?;
    let copy = bool_param(p, "copy").unwrap_or(false);
    let to = p.get("to").and_then(Json::as_u64).map(|v| v as usize).ok_or_else(|| bad("sheet.move", "missing `to`"))?;
    let n = s.doc()?.wb.sheets.len();
    if s.doc()?.wb.protected_structure {
        return Err(EngineError::Other("The workbook is protected and cannot be changed.".into()));
    }
    if !copy && to.min(n.saturating_sub(1)) == i {
        return Ok(json!({"index": i}));
    }
    let active_before = s.doc()?.wb.active_sheet;
    let mut copied = None;
    let at = edit(s, |cx| {
        let Some(src) = cx.wb.sheets.get(i).cloned() else { return Err(bad("sheet.move", "no such sheet")) };
        if copy {
            let base = src.name.clone();
            let name = copy_name(&cx.wb, &base)?;
            let sh = sheet_copy(&cx.wb, &src, &name)?;
            copied = Some(name.clone());
            let at = to.min(n);
            // The copy gets its own copies of the sheet's names, referring to the copy.
            let copies: Vec<kition_sheet_model::DefinedName> = cx
                .wb
                .names
                .iter()
                .filter(|nm| nm.scope == Some(i))
                .map(|nm| {
                    let formula = renamed_refs(&nm.formula, &base, &name).unwrap_or_else(|| nm.formula.clone());
                    kition_sheet_model::DefinedName { scope: Some(at), formula, ..nm.clone() }
                })
                .collect();
            for nm in cx.wb.names.iter_mut() {
                if let Some(sc) = nm.scope.as_mut()
                    && *sc >= at
                {
                    *sc += 1;
                }
            }
            cx.wb.names.extend(copies);
            cx.wb.sheets.insert(at, Arc::new(sh));
            cx.wb.active_sheet = at;
            cx.structural = true;
            Ok(at)
        } else {
            let sh = cx.wb.sheets.remove(i);
            let at = to.min(n - 1);
            cx.wb.sheets.insert(at, sh);
            cx.wb.active_sheet = moved_index(cx.wb.active_sheet, i, at);
            // Sheet-level names follow their sheet.
            for nm in cx.wb.names.iter_mut() {
                if let Some(sc) = nm.scope.as_mut() {
                    *sc = moved_index(*sc, i, at);
                }
            }
            cx.structural = true;
            Ok(at)
        }
    })?;
    let d = s.doc_mut()?;
    if d.sheet_selections.len() < n {
        d.sheet_selections.resize(n, crate::Selection::default());
    }
    if copy {
        // The copy opens at A1, as a new sheet does; the sheet that was active remembers its own.
        if let Some(slot) = d.sheet_selections.get_mut(active_before) {
            *slot = d.selection.clone();
        }
        d.sheet_selections.insert(at.min(d.sheet_selections.len()), crate::Selection::default());
        d.selection = crate::Selection::default();
    } else if i < d.sheet_selections.len() {
        let memory = d.sheet_selections.remove(i);
        d.sheet_selections.insert(at.min(d.sheet_selections.len()), memory);
    }
    Ok(match copied {
        Some(name) => json!({"index": at, "name": name}),
        None => json!({"index": at}),
    })
}

fn tab_color(s: &mut Session, p: &Json) -> Result<Json> {
    let i = target_sheet(s, p)?;
    let c = super::format::color_param(p.get("color"));
    edit(s, |cx| {
        cx.sheet_mut(i)?.tab_color = c.filter(|c| *c != kition_sheet_model::Color::Auto);
        Ok(Json::Null)
    })
}

/// Hides a sheet. Hiding the active sheet activates the next visible sheet on its right, else
/// the nearest on its left, as Excel does.
fn hide_sheet(s: &mut Session, p: &Json) -> Result<Json> {
    let i = target_sheet(s, p)?;
    if s.doc()?.wb.sheets.iter().filter(|s| s.visibility == Visibility::Visible).count() <= 1 {
        return Err(EngineError::Other("A workbook must contain at least one visible worksheet.".into()));
    }
    let before = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        cx.sheet_mut(i)?.visibility = Visibility::Hidden;
        if cx.wb.active_sheet == i {
            let visible = |j: &usize| cx.wb.sheet(*j).is_some_and(|s| s.visibility == Visibility::Visible);
            let next = (i + 1..cx.wb.sheets.len()).find(visible).or_else(|| (0..i).rev().find(visible)).unwrap_or(0);
            cx.wb.active_sheet = next;
        }
        Ok(Json::Null)
    })?;
    follow_active_sheet(s.doc_mut()?, before);
    ok()
}

fn unhide_sheet(s: &mut Session, p: &Json) -> Result<Json> {
    if p.get("sheet").is_none() {
        let hidden: Vec<String> = s.doc()?.wb.sheets.iter().filter(|s| s.visibility != Visibility::Visible).map(|s| s.name.clone()).collect();
        if hidden.is_empty() {
            return Err(EngineError::Other("No sheets are hidden.".into()));
        }
        s.ui_requests.push(crate::UiRequest::Dialog("unhideSheet".into(), json!({"sheets": hidden})));
        return ok();
    }
    let i = target_sheet(s, p)?;
    // As in Excel the sheet shows; the sheet list's Unhide (`activate: false`) leaves the sheet in view.
    let activate = bool_param(p, "activate").unwrap_or(true);
    let before = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        cx.sheet_mut(i)?.visibility = Visibility::Visible;
        if activate {
            cx.wb.active_sheet = i;
        }
        Ok(Json::Null)
    })?;
    follow_active_sheet(s.doc_mut()?, before);
    ok()
}

/// After an edit changed the active sheet from `before`: `before` remembers the selection, and the
/// selection becomes the one the newly active sheet remembers.
fn follow_active_sheet(d: &mut crate::DocState, before: usize) {
    let after = d.wb.active_sheet;
    if after == before {
        return;
    }
    if d.sheet_selections.len() < d.wb.sheets.len() {
        d.sheet_selections.resize(d.wb.sheets.len(), crate::Selection::default());
    }
    if let Some(slot) = d.sheet_selections.get_mut(before) {
        *slot = d.selection.clone();
    }
    d.selection = d.sheet_selections.get(after).cloned().unwrap_or_default();
}

fn activate(s: &mut Session, p: &Json) -> Result<Json> {
    let i = target_sheet(s, p)?;
    let d = s.doc_mut()?;
    d.wb_switch_sheet(i);
    Ok(json!({"sheet": i, "name": d.wb.active().map(|s| s.name.clone())}))
}

fn step_sheet(s: &mut Session, dir: i64) -> Result<Json> {
    let d = s.doc()?;
    let n = d.wb.sheets.len() as i64;
    let mut i = d.wb.active_sheet as i64;
    for _ in 0..n {
        i = (i + dir).clamp(0, n - 1);
        if d.wb.sheet(i as usize).is_some_and(|s| s.visibility == Visibility::Visible) {
            break;
        }
    }
    s.doc_mut()?.wb_switch_sheet(i as usize);
    ok()
}

/// Where a range of the edited sheet goes after a structural edit (`None` = deleted).
fn map_range(r: RangeRef, e: &Edit) -> Option<RangeRef> {
    let expr = kition_sheet_formula::Expr::Ref(kition_sheet_formula::Reference {
        sheet: kition_sheet_formula::SheetSel::Current,
        kind: kition_sheet_formula::RefKind::Range(
            kition_sheet_formula::Anchor { row: r.start.row, col: r.start.col, row_abs: false, col_abs: false },
            kition_sheet_formula::Anchor { row: r.end.row, col: r.end.col, row_abs: false, col_abs: false },
        ),
    });
    match kition_sheet_formula::adjust::adjust(expr, "S", "S", e) {
        kition_sheet_formula::Expr::Ref(r) => Some(r.range()),
        _ => None,
    }
}

/// Shifts what Insert/Delete Cells moves along with the cells: merged areas, conditional
/// format, validation, table, filter and print-area ranges, notes, links and sparklines.
pub(crate) fn shift_cell_features(sh: &mut Sheet, e: &Edit) {
    let map_cell = |c: CellRef| map_range(RangeRef::cell(c), e).map(|r| r.start);
    sh.merges = sh.merges.iter().filter_map(|m| map_range(*m, e)).filter(|m| !m.is_single()).collect();
    for cf in sh.cond_formats.iter_mut() {
        cf.ranges = cf.ranges.iter().filter_map(|r| map_range(*r, e)).collect();
    }
    sh.cond_formats.retain(|cf| !cf.ranges.is_empty());
    for dv in sh.validations.iter_mut() {
        dv.ranges = dv.ranges.iter().filter_map(|r| map_range(*r, e)).collect();
    }
    sh.validations.retain(|dv| !dv.ranges.is_empty());
    for t in sh.tables.iter_mut() {
        t.range = map_range(t.range, e).unwrap_or_default();
    }
    sh.tables.retain(|t| t.range != RangeRef::default());
    if let Some(af) = sh.autofilter.as_mut() {
        match map_range(af.range, e) {
            Some(r) => af.range = r,
            None => sh.autofilter = None,
        }
    }
    sh.cell_pictures =
        std::mem::take(&mut sh.cell_pictures).into_iter().filter_map(|(c, v)| map_cell(c).filter(CellRef::is_valid).map(|c| (c, v))).collect();
    sh.comments = std::mem::take(&mut sh.comments).into_iter().filter_map(|(c, v)| map_cell(c).map(|c| (c, v))).collect();
    sh.hyperlinks = std::mem::take(&mut sh.hyperlinks).into_iter().filter_map(|(c, v)| map_cell(c).map(|c| (c, v))).collect();
    for sp in sh.sparklines.iter_mut() {
        if let Some(c) = map_cell(sp.cell) {
            sp.cell = c;
        }
    }
    if let Some(pa) = sh.print.print_area {
        sh.print.print_area = map_range(pa, e);
    }
}
