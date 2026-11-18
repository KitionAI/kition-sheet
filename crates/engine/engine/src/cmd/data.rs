//! Data tab: sort, filter, remove duplicates, text to columns, data validation, outline.

use std::cmp::Ordering;
use std::sync::Arc;

use kition_sheet_core::{CellRef, RangeRef, Value, sort_compare};
use kition_sheet_model::*;
use serde_json::{Value as Json, json};

use super::*;
use crate::selection::current_region;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("data.sortAscending", "Sort A to Z", ["Data", "Sort & Filter"], None, "{range?, column?: \"B\"}", has_doc, |s, p| quick_sort(
            s, p, true
        )),
        cmd!("data.sortDescending", "Sort Z to A", ["Data", "Sort & Filter"], None, "{range?, column?}", has_doc, |s, p| quick_sort(s, p, false)),
        cmd!(
            "data.sort",
            "Sort…",
            ["Data", "Sort & Filter"],
            None,
            "{range?, header?: bool, keys: [{column: \"B\", order: asc|desc, by?: values|cellColor|fontColor, customList?: [..]}], orientation?: rows|columns, matchCase?}",
            has_doc,
            sort
        ),
        cmd!(
            "data.filter",
            "Filter",
            ["Data", "Sort & Filter"],
            Some("Cmd+Shift+F"),
            "{range?} toggles AutoFilter on the current region",
            has_doc,
            toggle_filter
        ),
        cmd!(
            "data.filterBy",
            "Filter Column",
            [],
            None,
            "{column: \"B\" (or 0-based offset), cell?: \"B3\" (a filter's header cell: which filter, the sheet's or a table's, and the column), table?: name (a table's own filter), values?: [..], blanks?: bool, custom?: {op: \"=\"|\"<>\"|\">\"|\">=\"|\"<\"|\"<=\"|beginsWith|endsWith|contains|notContains, value: \"10\", op2?, value2?, and?}, top?: {count, bottom?, percent?}, aboveAverage?: bool, clear?: bool}",
            has_doc,
            filter_by
        ),
        cmd!(
            "data.clearFilter",
            "Clear",
            ["Data", "Sort & Filter"],
            None,
            "{cell?: \"B3\" | table?: name (a table's filter; default the sheet's, or the active cell's table when the sheet has none)}",
            has_doc,
            clear_filter
        ),
        cmd!(
            query "data.filterValues",
            "Filter Values",
            [],
            None,
            "{cell?: \"B3\" (a filter's header cell; default the active cell)} → {range, table, column, header, kind: text|number|date, values: [{text, count, checked}], blanks: {count, checked}|null, criterion, filtered: [columns]}: what a filter button's list shows",
            has_doc,
            filter_values_cmd
        ),
        cmd!("data.reapply", "Reapply", ["Data", "Sort & Filter"], None, "{}", has_doc, reapply),
        cmd!(
            "data.removeDuplicates",
            "Remove Duplicates",
            ["Data", "Data Tools"],
            None,
            "{range?, columns?: [\"A\",\"C\"], header?: bool}",
            has_doc,
            remove_duplicates
        ),
        cmd!(
            "data.textToColumns",
            "Text to Columns",
            ["Data", "Data Tools"],
            None,
            "{range?: str (an A1 range of one column; default the selection), delimiters?: [\",\", \"\t\", \" \", \";\"] (each one character; tab is \"\t\"), other?: str (one more delimiter character, e.g. \"|\"), fixedWidths?: [int] (column widths in characters, instead of delimiters), treatConsecutive?: bool, textQualifier?: str (one character that quotes a field; default the double quote), destination?: \"B1\" (the top-left cell of the result; default the range start)}",
            has_doc,
            text_to_columns
        ),
        cmd!(
            "data.validation",
            "Data Validation…",
            ["Data", "Data Tools"],
            None,
            "{range?, type: any|whole|decimal|list|date|time|textLength|custom, operator?: between|notBetween|equal|notEqual|greater|less|greaterOrEqual|lessOrEqual, formula1, formula2?, ignoreBlank?, dropdown?, showInput?: bool (default true), inputTitle?, inputMessage?, showError?: bool (default true), errorStyle?: stop|warning|information, errorTitle?, errorMessage?, clear?: bool}",
            has_doc,
            validation
        ),
        cmd!(query "data.validate", "Check Value Against Validation", [], None, "{cell?, input} → {ok, message?}", has_doc, validate_cmd),
        cmd!(noundo "data.circleInvalid", "Circle Invalid Data", ["Data", "Data Tools", "Data Validation"], None, "{} → invalid cells", has_doc, circle_invalid),
        cmd!("data.group", "Group", ["Data", "Outline"], Some("Cmd+Shift+K"), "{rows?: \"2:5\" | cols?: \"B:C\"}", has_doc, |s, p| group(s, p, true)),
        cmd!("data.ungroup", "Ungroup", ["Data", "Outline"], Some("Cmd+Shift+J"), "{rows? | cols?}", has_doc, |s, p| group(s, p, false)),
        cmd!("data.hideDetail", "Hide Detail", ["Data", "Outline"], None, "{rows? | cols?}", has_doc, |s, p| detail(s, p, true)),
        cmd!("data.showDetail", "Show Detail", ["Data", "Outline"], None, "{rows? | cols?}", has_doc, |s, p| detail(s, p, false)),
        cmd!(
            "data.subtotal",
            "Subtotal",
            ["Data", "Outline"],
            None,
            "{range?, groupBy: \"A\", function?: sum|count|average|max|min|product, columns: [\"C\"], header?: true}",
            has_doc,
            subtotal
        ),
        cmd!("data.smartFill", "Smart fill", ["Data", "Data Tools"], None, "{range?}", has_doc, |s, p| s.execute("edit.smartFill", p.clone())),
    ]
}

fn col_param(v: Option<&Json>, base: u32) -> Option<u32> {
    match v? {
        Json::String(s) => kition_sheet_core::letters_to_col(s.trim_start_matches('$')),
        Json::Number(n) => n.as_u64().map(|o| base + o as u32),
        _ => None,
    }
}

/// The range a data command works on: the selection when it's a block, else the current
/// region (trimmed to the used range for whole columns).
fn data_range(s: &Session, p: &Json) -> Result<RangeRef> {
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    if p.get("range").is_some() {
        return target_range(s, p);
    }
    if let Some(t) = sh.table_at(d.selection.active) {
        return Ok(t.data_range().map(|dr| RangeRef::new(t.range.start, dr.end)).unwrap_or(t.range));
    }
    let sel = d.selection.current();
    if !sel.is_single() {
        // An empty sheet has no data to work on: just the active cell.
        let Some(used) = sh.used_range() else { return Ok(RangeRef::cell(d.selection.active)) };
        return Ok(sel.intersection(&used).unwrap_or(sel));
    }
    Ok(current_region(sh, d.selection.active))
}

/// Excel's header guess: the first row is text over a column of numbers, or is bold.
fn guess_header(wb: &Workbook, sh: &Sheet, r: RangeRef) -> bool {
    if let Some(t) = sh.table_at(r.start) {
        return t.header_row;
    }
    if r.height() < 2 {
        return false;
    }
    let mut votes = 0i32;
    for c in r.start.col..=r.end.col {
        let a = sh.value(CellRef::new(r.start.row, c));
        let b = sh.value(CellRef::new(r.start.row + 1, c));
        if a.is_text() && !b.is_text() && !b.is_empty() {
            votes += 1;
        }
        if wb.styles.get(sh.style_id(CellRef::new(r.start.row, c))).font.bold
            != wb.styles.get(sh.style_id(CellRef::new(r.start.row + 1, c))).font.bold
        {
            votes += 1;
        }
        if a.is_text() && b.is_text() {
            votes -= 0;
        }
    }
    votes > 0
}

#[derive(Clone)]
struct SortKey {
    col: u32,
    desc: bool,
    by: String,
    list: Option<Vec<String>>,
}

fn quick_sort(s: &mut Session, p: &Json, asc: bool) -> Result<Json> {
    let r = data_range(s, p)?;
    let col = col_param(p.get("column"), r.start.col).unwrap_or(s.doc()?.selection.active.col).clamp(r.start.col, r.end.col);
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let header = guess_header(&d.wb, sh, r);
    do_sort(s, r, header, vec![SortKey { col, desc: !asc, by: "values".into(), list: None }], false, false)
}

fn sort(s: &mut Session, p: &Json) -> Result<Json> {
    let r = data_range(s, p)?;
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let header = bool_param(p, "header").unwrap_or_else(|| guess_header(&d.wb, sh, r));
    let by_cols = str_param(p, "orientation") == Some("columns");
    let case = bool_param(p, "matchCase").unwrap_or(false);
    if by_cols {
        for key in p.get("keys").and_then(Json::as_array).into_iter().flatten() {
            if let Some(row) = key.get("row").and_then(Json::as_u64)
                && !(1..=u64::from(kition_sheet_core::MAX_ROWS)).contains(&row)
            {
                return Err(bad("data.sort", "a sort key row must be between 1 and 1048576"));
            }
        }
    }
    let keys: Vec<SortKey> = p
        .get("keys")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|k| {
                    let col = if by_cols {
                        k.get("row")
                            .and_then(Json::as_u64)
                            .and_then(|row| u32::try_from(row).ok())
                            .and_then(|row| row.checked_sub(1))
                            .or_else(|| col_param(k.get("column"), r.start.row))?
                    } else {
                        col_param(k.get("column"), r.start.col)?
                    };
                    Some(SortKey {
                        col,
                        desc: k.get("order").and_then(Json::as_str) == Some("desc"),
                        by: k.get("by").and_then(Json::as_str).unwrap_or("values").to_string(),
                        list: k.get("customList").and_then(Json::as_array).map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if keys.is_empty() {
        s.ui_requests.push(crate::UiRequest::Dialog("sort".into(), json!({"range": r.a1(), "header": header})));
        return ok();
    }
    if keys.len() > 64 {
        return Err(bad("data.sort", "too many sort levels (64 max)"));
    }
    do_sort(s, r, header, keys, by_cols, case)
}

fn do_sort(s: &mut Session, r: RangeRef, header: bool, keys: Vec<SortKey>, by_cols: bool, case: bool) -> Result<Json> {
    let body = if by_cols {
        if header && r.width() > 1 { RangeRef::new(CellRef::new(r.start.row, r.start.col + 1), r.end) } else { r }
    } else if header && r.height() > 1 {
        RangeRef::new(CellRef::new(r.start.row + 1, r.start.col), r.end)
    } else {
        r
    };
    if body.count() > 20_000_000 {
        return Err(bad("data.sort", "range too large"));
    }
    let sheet = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        let Some(sh) = cx.wb.sheet(sheet) else { return Ok(Json::Null) };
        if sh.merges.iter().any(|m| m.intersects(&body) && !m.is_single()) {
            return Err(EngineError::Other("To do this, all the merged cells need to be the same size.".into()));
        }
        let n = if by_cols { body.width() } else { body.height() };
        let line = |i: u32, k: u32| if by_cols { CellRef::new(k, body.start.col + i) } else { CellRef::new(body.start.row + i, k) };
        let theme = cx.wb.theme.clone();
        let styles = &cx.wb.styles;
        let key_val = |i: u32, k: &SortKey| -> (Value, u32) {
            let c = line(i, k.col);
            let v = sh.value(c);
            let color = match k.by.as_str() {
                "cellColor" => {
                    styles.get(sh.style_id(c)).fill.fg.resolve(&theme).map(|[r, g, b]| u32::from_be_bytes([0, r, g, b])).unwrap_or(u32::MAX)
                }
                "fontColor" => {
                    styles.get(sh.style_id(c)).font.color.resolve(&theme).map(|[r, g, b]| u32::from_be_bytes([0, r, g, b])).unwrap_or(u32::MAX)
                }
                _ => 0,
            };
            (v, color)
        };
        let mut order: Vec<u32> = (0..n).collect();
        let keyed: Vec<Vec<(Value, u32)>> = (0..n).map(|i| keys.iter().map(|k| key_val(i, k)).collect()).collect();
        order.sort_by(|&a, &b| {
            for (ki, k) in keys.iter().enumerate() {
                let (va, ca) = &keyed[a as usize][ki];
                let (vb, cb) = &keyed[b as usize][ki];
                let o = if k.by != "values" {
                    ca.cmp(cb)
                } else if let Some(list) = &k.list {
                    let pos = |v: &Value| v.as_text().and_then(|t| list.iter().position(|x| x.eq_ignore_ascii_case(t))).unwrap_or(usize::MAX);
                    pos(va).cmp(&pos(vb))
                } else if case && va.is_text() && vb.is_text() {
                    va.as_text().unwrap_or("").cmp(vb.as_text().unwrap_or(""))
                } else {
                    sort_compare(va, vb)
                };
                // Blanks always sort last, whatever the order.
                let o = if k.desc && !(va.is_empty() || vb.is_empty()) { o.reverse() } else { o };
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        });
        if order.iter().enumerate().all(|(i, &o)| i as u32 == o) {
            return Ok(Json::Null);
        }
        // Move whole lines (cells with formulas adjusted for relative references).
        let lanes: Vec<u32> = if by_cols { (body.start.row..=body.end.row).collect() } else { (body.start.col..=body.end.col).collect() };
        let mut moved: Vec<(CellRef, Option<Cell>)> = Vec::with_capacity(body.count() as usize);
        let mut pictures = Vec::new();
        for (dst_i, &src_i) in order.iter().enumerate() {
            for &k in &lanes {
                let src = line(src_i, k);
                let dst = line(dst_i as u32, k);
                let mut cell = sh.cell(src).cloned();
                if let Some(c) = cell.as_mut()
                    && let Some(f) = &c.formula
                    && let Some(e) = f.expr()
                {
                    let shifted = kition_sheet_formula::adjust::shift_relative(e, dst.row as i64 - src.row as i64, dst.col as i64 - src.col as i64);
                    c.formula = Some(Arc::new(Formula::from_expr(shifted)));
                }
                if let Some(picture) = sh.cell_pictures.get(&src) {
                    pictures.push((dst, picture.clone()));
                }
                moved.push((dst, cell));
            }
        }
        // Row heights travel with rows.
        let heights: Vec<Option<LineInfo>> =
            if by_cols { vec![] } else { order.iter().map(|&i| sh.rows.get(&(body.start.row + i)).copied()).collect() };
        // Floating objects anchored inside the sorted body travel with their lines, unless they
        // are pinned absolutely ("Don't move or size with cells", `editAs="absolute"`). Reordering
        // is the inverse of `order`: `dst_of[src] = dst`.
        let dst_of: Vec<u32> = {
            let mut v = vec![0u32; order.len()];
            for (dst_i, &src_i) in order.iter().enumerate() {
                if let Some(slot) = v.get_mut(src_i as usize) {
                    *slot = dst_i as u32;
                }
            }
            v
        };
        // The object's own geometry (its top-left anchor cell, offset and size) is read from the
        // pre-sort `sh`; a two-cell object's bottom-right cell is derived from its size here so it
        // can be re-fitted to the lines it spans after the sort.
        let objs: Vec<Anchor> =
            sh.images.iter().map(|o| o.anchor).chain(sh.charts.iter().map(|o| o.anchor)).chain(sh.shapes.iter().map(|o| o.anchor)).collect();
        let descs: Vec<(u32, u32, f32, f32)> = objs
            .iter()
            .map(|a| {
                let x2 = sh.col_left(a.cell.col) + a.dx as f64 + a.width.max(0.0) as f64;
                let y2 = sh.row_top(a.cell.row) + a.dy as f64 + a.height.max(0.0) as f64;
                let tc = sh.col_at(x2);
                let tr = sh.row_at(y2);
                (tc, tr, (x2 - sh.col_left(tc)) as f32, (y2 - sh.row_top(tr)) as f32)
            })
            .collect();
        let shm = cx.sheet_mut(sheet)?;
        for (c, cell) in &moved {
            match cell {
                Some(x) => shm.set_cell(*c, x.clone()),
                None => {
                    shm.remove_cell(*c);
                }
            }
        }
        shm.cell_pictures.extend(pictures);
        for (i, h) in heights.into_iter().enumerate() {
            let row = body.start.row + i as u32;
            match h {
                Some(info) => {
                    shm.rows.insert(row, LineInfo { hidden: shm.rows.get(&row).is_some_and(|x| x.hidden), ..info });
                }
                None => {
                    if let Some(e) = shm.rows.get_mut(&row) {
                        e.size = None;
                    }
                }
            }
        }
        // Re-anchor floating objects now that the row heights are final. `objs` was collected as
        // images ++ charts ++ shapes, so the same index maps straight onto each vec.
        // Only objects anchored inside the sorted block move: within its lines on the sort axis
        // and within its span on the other axis (a picture beside the block stays put).
        let (b0, b1) = if by_cols { (body.start.col, body.end.col) } else { (body.start.row, body.end.row) };
        let (o0, o1) = if by_cols { (body.start.row, body.end.row) } else { (body.start.col, body.end.col) };
        let ni = shm.images.len();
        let nch = shm.charts.len();
        for (i, (a, desc)) in objs.into_iter().zip(descs).enumerate() {
            let other = if by_cols { a.cell.row } else { a.cell.col };
            if other < o0 || other > o1 {
                continue;
            }
            let na = sorted_anchor(shm, a, desc, by_cols, b0, b1, &dst_of);
            if na != a {
                if i < ni {
                    if let Some(o) = shm.images.get_mut(i) {
                        o.anchor = na;
                    }
                } else if i < ni + nch {
                    if let Some(o) = shm.charts.get_mut(i - ni) {
                        o.anchor = na;
                    }
                } else if let Some(o) = shm.shapes.get_mut(i - ni - nch) {
                    o.anchor = na;
                }
            }
        }
        for (c, _) in moved {
            cx.changed.push((sheet, c));
        }
        // A filter shows the rows its criteria let through, wherever the sort put them.
        reapply_criteria(&mut cx.wb, sheet);
        cx.structural = true;
        Ok(Json::Null)
    })?;
    Ok(json!({"sorted": body.a1()}))
}

/// Re-anchor one floating object for a sort. `bottom` is the object's bottom-right line and the
/// offset of that corner inside it, measured before the sort. `map` (via `dst_of`) translates a
/// line index on the moving axis and is the identity outside `b0..=b1`. `sh` is the post-sort
/// grid (row heights already reordered), so re-fitting uses the heights the object lands on.
/// Objects pinned absolutely ("Don't move or size with cells") never move; "move but don't size"
/// keeps its size; "move and size" is re-fitted when its whole extent stays inside the body.
fn sorted_anchor(sh: &Sheet, a: Anchor, bottom: (u32, u32, f32, f32), by_cols: bool, b0: u32, b1: u32, dst_of: &[u32]) -> Anchor {
    if a.mode == AnchorMode::Absolute {
        return a;
    }
    let (tc, tr, tdx, tdy) = bottom;
    let map = |v: u32| -> u32 { if v >= b0 && v <= b1 { dst_of.get((v - b0) as usize).map_or(v, |&d| b0 + d) } else { v } };
    let (axis_start, axis_end) = if by_cols { (a.cell.col, tc) } else { (a.cell.row, tr) };
    if axis_start < b0 || axis_start > b1 {
        return a;
    }
    let (nr, nc) = if by_cols { (a.cell.row, map(a.cell.col)) } else { (map(a.cell.row), a.cell.col) };
    if a.mode == AnchorMode::MoveAndSize && axis_end >= b0 && axis_end <= b1 {
        let (nbr, nbc) = if by_cols { (tr, map(tc)) } else { (map(tr), tc) };
        let x1 = sh.col_left(nc) + a.dx as f64;
        let y1 = sh.row_top(nr) + a.dy as f64;
        let x2 = sh.col_left(nbc) + tdx as f64;
        let y2 = sh.row_top(nbr) + tdy as f64;
        Anchor { cell: CellRef::new(nr, nc), dx: a.dx, dy: a.dy, width: (x2 - x1).max(0.0) as f32, height: (y2 - y1).max(0.0) as f32, ..a }
    } else {
        Anchor { cell: CellRef::new(nr, nc), ..a }
    }
}

// ---------------------------------------------------------------- filter

fn toggle_filter(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = s.doc()?.wb.active_sheet;
    let has = s.doc()?.wb.sheet(sheet).is_some_and(|sh| sh.autofilter.is_some());
    let in_table =
        s.doc()?.wb.sheet(sheet).and_then(|sh| sh.table_at(s.doc().map(|d| d.selection.active).unwrap_or_default()).map(|t| t.name.clone()));
    if let Some(name) = in_table {
        return edit(s, |cx| {
            let sh = cx.sheet_mut(sheet)?;
            if let Some(t) = sh.tables.iter_mut().find(|t| t.name == name) {
                t.filter_button = !t.filter_button;
            }
            Ok(Json::Null)
        });
    }
    if has {
        return edit(s, |cx| {
            let sh = cx.sheet_mut(sheet)?;
            sh.autofilter = None;
            for info in sh.rows.values_mut() {
                info.hidden = false;
            }
            cx.structural = true;
            Ok(json!({"filter": false}))
        });
    }
    let r = data_range(s, p)?;
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.autofilter = Some(AutoFilter { range: r, criteria: vec![], extra: vec![] });
        Ok(json!({"filter": true, "range": r.a1()}))
    })
}

/// A filter on the sheet: its own AutoFilter, or a table's header buttons. `range` runs from the
/// header row down to the last row filtered (a table's totals row excluded).
#[derive(Clone, Debug)]
pub struct FilterScope {
    pub range: RangeRef,
    pub criteria: Vec<(u32, FilterCriterion)>,
    /// The table's name, for a table's filter.
    pub table: Option<String>,
}

/// The sheet's filters: its AutoFilter, then each table with a header row and filter buttons.
pub fn filter_scopes(sh: &Sheet) -> Vec<FilterScope> {
    let mut out: Vec<FilterScope> =
        sh.autofilter.iter().map(|af| FilterScope { range: af.range, criteria: af.criteria.clone(), table: None }).collect();
    for t in sh.tables.iter().filter(|t| t.header_row && t.filter_button) {
        let end = if t.totals_row { t.range.end.row.saturating_sub(1).max(t.range.start.row) } else { t.range.end.row };
        out.push(FilterScope {
            range: RangeRef::new(t.range.start, CellRef::new(end, t.range.end.col)),
            criteria: t.criteria.clone(),
            table: Some(t.name.clone()),
        });
    }
    out
}

/// The filter whose header row holds `cell` (a filter button's cell), else the one around it.
pub fn filter_scope_at(sh: &Sheet, cell: CellRef) -> Option<FilterScope> {
    let scopes = filter_scopes(sh);
    let header = |f: &FilterScope| f.range.start.row == cell.row && (f.range.start.col..=f.range.end.col).contains(&cell.col);
    scopes.iter().find(|f| header(f)).or_else(|| scopes.iter().find(|f| f.range.contains(cell))).cloned()
}

/// The filter a call names: `table`, else the one whose header holds `cell`, else the sheet's
/// AutoFilter, else the table around the active cell.
pub fn named_scope(s: &Session, p: &Json) -> Result<Option<FilterScope>> {
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    if let Some(name) = str_param(p, "table") {
        let found = filter_scopes(sh).into_iter().find(|f| f.table.as_deref().is_some_and(|t| t.eq_ignore_ascii_case(name)));
        return found.map(Some).ok_or_else(|| bad("data.filterBy", format!("no table `{name}` with filter buttons")));
    }
    if let Some(cell) = cell_param(p, "cell") {
        return Ok(filter_scope_at(sh, cell));
    }
    if let Some(af) = &sh.autofilter {
        return Ok(Some(FilterScope { range: af.range, criteria: af.criteria.clone(), table: None }));
    }
    Ok(sh.table_at(d.selection.active).and_then(|t| filter_scope_at(sh, t.range.start)))
}

fn filter_by(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = s.doc()?.wb.active_sheet;
    let scope = match named_scope(s, p)? {
        Some(scope) => scope,
        None => {
            toggle_filter(s, &json!({}))?;
            let sh = s.doc()?.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
            let af = sh.autofilter.clone().ok_or_else(|| EngineError::Other("no filter".into()))?;
            FilterScope { range: af.range, criteria: af.criteria, table: None }
        }
    };
    let col = col_param(p.get("column"), scope.range.start.col)
        .or_else(|| cell_param(p, "cell").map(|c| c.col))
        .ok_or_else(|| bad("data.filterBy", "missing `column`"))?;
    if col < scope.range.start.col || col > scope.range.end.col {
        return Err(bad("data.filterBy", "column outside the filter range"));
    }
    let off = col - scope.range.start.col;
    let crit = if bool_param(p, "clear").unwrap_or(false) {
        None
    } else if let Some(vals) = p.get("values").and_then(Json::as_array) {
        Some(FilterCriterion::Values {
            values: vals.iter().map(super::edit::json_to_input).collect(),
            blanks: bool_param(p, "blanks").unwrap_or(false),
        })
    } else if let Some(c) = p.get("custom") {
        let a = (c.get("op").and_then(Json::as_str).unwrap_or("=").to_string(), c.get("value").map(super::edit::json_to_input).unwrap_or_default());
        let b = c.get("op2").and_then(Json::as_str).map(|o| (o.to_string(), c.get("value2").map(super::edit::json_to_input).unwrap_or_default()));
        Some(FilterCriterion::Custom { a, b, and: c.get("and").and_then(Json::as_bool).unwrap_or(true) })
    } else if let Some(t) = p.get("top") {
        Some(FilterCriterion::Top10 {
            bottom: t.get("bottom").and_then(Json::as_bool).unwrap_or(false),
            percent: t.get("percent").and_then(Json::as_bool).unwrap_or(false),
            count: t.get("count").and_then(Json::as_u64).unwrap_or(10).min(u64::from(u32::MAX)) as u32,
        })
    } else {
        bool_param(p, "aboveAverage").map(FilterCriterion::AboveAverage)
    };
    let table = scope.table.clone();
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        let criteria = match &table {
            Some(name) => sh.tables.iter_mut().find(|t| &t.name == name).map(|t| &mut t.criteria),
            None => sh.autofilter.as_mut().map(|a| &mut a.criteria),
        };
        if let Some(criteria) = criteria {
            criteria.retain(|(o, _)| *o != off);
            if let Some(c) = crit.clone() {
                criteria.push((off, c));
                criteria.sort_by_key(|(o, _)| *o);
            }
        }
        match &table {
            Some(name) => apply_table_filter(&mut cx.wb, sheet, name),
            None => apply_filter(&mut cx.wb, sheet),
        }
        cx.structural = true;
        Ok(Json::Null)
    })?;
    let hidden = s.doc()?.wb.sheet(sheet).map(|sh| sh.rows.values().filter(|r| r.hidden).count()).unwrap_or(0);
    Ok(json!({"hiddenRows": hidden}))
}

/// Does a value pass a custom comparison (`>`, `=*abc*`, `<>x`…)?
fn custom_ok(v: &Value, text: &str, op: &str, crit: &str) -> bool {
    let num = kition_sheet_core::parse::parse_number_text(crit);
    let cmp = match (v.as_f64(), num) {
        (Some(a), Some(b)) => a.partial_cmp(&b),
        _ => Some(kition_sheet_core::compare_text(text, crit)),
    };
    let wild = || super::edit::wildcard(&text.to_lowercase(), &crit.to_lowercase());
    match op {
        "=" => {
            if crit.contains(['*', '?']) {
                wild()
            } else {
                cmp == Some(Ordering::Equal)
            }
        }
        "<>" => {
            if crit.contains(['*', '?']) {
                !wild()
            } else {
                cmp != Some(Ordering::Equal)
            }
        }
        ">" => cmp == Some(Ordering::Greater),
        "<" => cmp == Some(Ordering::Less),
        ">=" => matches!(cmp, Some(Ordering::Greater | Ordering::Equal)),
        "<=" => matches!(cmp, Some(Ordering::Less | Ordering::Equal)),
        "beginsWith" => text.to_lowercase().starts_with(&crit.to_lowercase()),
        "endsWith" => text.to_lowercase().ends_with(&crit.to_lowercase()),
        "contains" => text.to_lowercase().contains(&crit.to_lowercase()),
        "notContains" => !text.to_lowercase().contains(&crit.to_lowercase()),
        _ => true,
    }
}

/// Which rows of a filter's range fail its criteria: (row, hidden) for every row below its header.
fn filtered_rows(wb: &Workbook, sh: &Sheet, r: RangeRef, criteria: &[(u32, FilterCriterion)]) -> Vec<(u32, bool)> {
    let mut hide = Vec::new();
    // Precompute per-column numbers for top10/average.
    let mut col_stats: std::collections::HashMap<u32, (Vec<f64>, f64)> = std::collections::HashMap::new();
    for (off, c) in criteria {
        if matches!(c, FilterCriterion::Top10 { .. } | FilterCriterion::AboveAverage(_)) {
            let col = r.start.col + off;
            let nums: Vec<f64> = (r.start.row + 1..=r.end.row).filter_map(|row| sh.value(CellRef::new(row, col)).as_f64()).collect();
            let avg = if nums.is_empty() { 0.0 } else { nums.iter().sum::<f64>() / nums.len() as f64 };
            col_stats.insert(*off, (nums, avg));
        }
    }
    for row in r.start.row + 1..=r.end.row {
        let mut visible = true;
        for (off, crit) in criteria {
            let c = CellRef::new(row, r.start.col + off);
            let v = sh.value(c);
            let text = crate::display::cell_text(wb, sh, c);
            let ok = match crit {
                FilterCriterion::Values { values, blanks } => {
                    if v.is_empty() {
                        *blanks
                    } else {
                        values.iter().any(|x| x.eq_ignore_ascii_case(&text) || x.eq_ignore_ascii_case(&v.display()))
                    }
                }
                FilterCriterion::Custom { a, b, and } => {
                    let x = custom_ok(&v, &text, &a.0, &a.1);
                    match b {
                        Some(b) => {
                            let y = custom_ok(&v, &text, &b.0, &b.1);
                            if *and { x && y } else { x || y }
                        }
                        None => x,
                    }
                }
                FilterCriterion::Top10 { bottom, percent, count } => match (v.as_f64(), col_stats.get(off)) {
                    (Some(n), Some((nums, _))) => {
                        let mut sorted = nums.clone();
                        sorted.sort_by(|a, b| if *bottom { a.partial_cmp(b) } else { b.partial_cmp(a) }.unwrap_or(Ordering::Equal));
                        let k = if *percent { ((sorted.len() as f64) * (*count as f64) / 100.0).ceil() as usize } else { *count as usize };
                        let k = k.clamp(1, sorted.len().max(1));
                        let thr = sorted.get(k - 1).copied().unwrap_or(n);
                        if *bottom { n <= thr } else { n >= thr }
                    }
                    _ => false,
                },
                FilterCriterion::AboveAverage(above) => match (v.as_f64(), col_stats.get(off)) {
                    (Some(n), Some((_, avg))) => {
                        if *above {
                            n > *avg
                        } else {
                            n < *avg
                        }
                    }
                    _ => false,
                },
                FilterCriterion::FillColor(col) => wb.styles.get(sh.style_id(c)).fill.fg == *col,
                FilterCriterion::FontColor(col) => wb.styles.get(sh.style_id(c)).font.color == *col,
            };
            if !ok {
                visible = false;
                break;
            }
        }
        hide.push((row, !visible));
    }
    hide
}

/// Hides the rows of a filter's range that fail its criteria and shows the others.
fn apply_scope(wb: &mut Workbook, sheet: usize, scope: &FilterScope) {
    let Some(sh) = wb.sheet(sheet) else { return };
    let hide = filtered_rows(wb, sh, scope.range, &scope.criteria);
    let Some(shm) = wb.sheet_mut(sheet) else { return };
    for (row, h) in hide {
        if h {
            shm.rows.entry(row).or_default().hidden = true;
        } else if let Some(e) = shm.rows.get_mut(&row) {
            e.hidden = false;
        }
    }
}

/// Hides rows of the autofilter range that fail any criterion, and applies the tables' filters
/// that have criteria.
pub(crate) fn apply_filter(wb: &mut Workbook, sheet: usize) {
    let Some(sh) = wb.sheet(sheet) else { return };
    for scope in filter_scopes(sh) {
        if scope.table.is_none() || !scope.criteria.is_empty() {
            apply_scope(wb, sheet, &scope);
        }
    }
}

/// Applies the filters that have criteria again (after the cells under them changed: a sort).
pub(crate) fn reapply_criteria(wb: &mut Workbook, sheet: usize) {
    let Some(sh) = wb.sheet(sheet) else { return };
    for scope in filter_scopes(sh).into_iter().filter(|f| !f.criteria.is_empty()) {
        apply_scope(wb, sheet, &scope);
    }
}

/// Applies one table's filter (its rows all show again when it has no criteria).
fn apply_table_filter(wb: &mut Workbook, sheet: usize, name: &str) {
    let Some(sh) = wb.sheet(sheet) else { return };
    if let Some(scope) = filter_scopes(sh).into_iter().find(|f| f.table.as_deref() == Some(name)) {
        apply_scope(wb, sheet, &scope);
    }
}

fn clear_filter(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = s.doc()?.wb.active_sheet;
    // A table's filter when the call names it or the active cell is in a filtered table and the
    // sheet has no filter of its own; else the sheet's.
    let table = match named_scope(s, p)? {
        Some(FilterScope { table: Some(name), .. })
            if p.get("table").is_some() || p.get("cell").is_some() || s.doc()?.wb.sheet(sheet).is_some_and(|sh| sh.autofilter.is_none()) =>
        {
            Some(name)
        }
        _ => None,
    };
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        match &table {
            Some(name) => {
                if let Some(t) = sh.tables.iter_mut().find(|t| &t.name == name) {
                    t.criteria.clear();
                }
                apply_table_filter(&mut cx.wb, sheet, name);
            }
            None => {
                if let Some(a) = sh.autofilter.as_mut() {
                    a.criteria.clear();
                }
                apply_filter(&mut cx.wb, sheet);
            }
        }
        cx.structural = true;
        Ok(Json::Null)
    })
}

fn reapply(s: &mut Session, _: &Json) -> Result<Json> {
    let sheet = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        apply_filter(&mut cx.wb, sheet);
        cx.structural = true;
        Ok(Json::Null)
    })
}

/// Most distinct values a filter's list offers (WPS and Excel list 10,000).
const FILTER_VALUES_MAX: usize = 10_000;

/// `data.filterValues`: what a filter button's list shows for its column.
fn filter_values_cmd(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let cell = cell_param(p, "cell").unwrap_or(d.selection.active);
    let Some(scope) = filter_scope_at(sh, cell) else { return Err(EngineError::Other("There's no filter here.".into())) };
    let col = cell.col.clamp(scope.range.start.col, scope.range.end.col);
    Ok(column_values(&d.wb, sh, &scope, col))
}

/// The distinct values of a filter's column, in sort order, with how many rows hold each and
/// whether its criterion lets them show; the blanks apart; and the column's criterion.
pub fn column_values(wb: &Workbook, sh: &Sheet, scope: &FilterScope, col: u32) -> Json {
    let off = col - scope.range.start.col;
    let selected: Option<&FilterCriterion> = scope.criteria.iter().find(|(o, _)| *o == off).map(|(_, c)| c);
    let mut seen: std::collections::BTreeMap<(u8, i64, String), (String, u32)> = std::collections::BTreeMap::new();
    let (mut blanks, mut numbers, mut texts, mut dates) = (0u32, 0u32, 0u32, 0u32);
    let last = scope.range.end.row.min(scope.range.start.row.saturating_add(1_000_000));
    for row in scope.range.start.row + 1..=last {
        let c = CellRef::new(row, col);
        let v = sh.value(c);
        if v.is_empty() {
            blanks += 1;
            continue;
        }
        if v.is_number() {
            numbers += 1;
            if crate::display::is_date_cell(wb, sh, c) {
                dates += 1;
            }
        } else {
            texts += 1;
        }
        let text = crate::display::cell_text(wb, sh, c);
        if seen.len() >= FILTER_VALUES_MAX && !seen.contains_key(&(sort_key(&v).0, sort_key(&v).1, sort_key(&v).2.clone())) {
            continue;
        }
        let key = sort_key(&v);
        let key = (key.0, key.1, format!("{}\u{0}{text}", key.2));
        seen.entry(key).or_insert_with(|| (text, 0)).1 += 1;
    }
    let shows = |text: &str| match selected {
        Some(FilterCriterion::Values { values, .. }) => values.iter().any(|x| x.eq_ignore_ascii_case(text)),
        _ => true,
    };
    let values: Vec<Json> = seen.into_values().map(|(text, count)| json!({ "text": text, "count": count, "checked": shows(&text) })).collect();
    let blanks = (blanks > 0).then(|| {
        let checked = match selected {
            Some(FilterCriterion::Values { blanks, .. }) => *blanks,
            _ => true,
        };
        json!({ "count": blanks, "checked": checked })
    });
    let kind = if dates > 0 && dates * 2 >= numbers + texts {
        "date"
    } else if numbers > texts {
        "number"
    } else {
        "text"
    };
    let criterion = selected.map(|c| match c {
        FilterCriterion::Values { values, blanks } => json!({ "type": "values", "values": values, "blanks": blanks }),
        FilterCriterion::Custom { a, b, and } => json!({ "type": "custom", "op": a.0, "value": a.1, "op2": b.as_ref().map(|b| b.0.clone()), "value2": b.as_ref().map(|b| b.1.clone()), "and": and }),
        FilterCriterion::Top10 { bottom, percent, count } => json!({ "type": "top", "bottom": bottom, "percent": percent, "count": count }),
        FilterCriterion::AboveAverage(above) => json!({ "type": "average", "above": above }),
        FilterCriterion::FillColor(_) | FilterCriterion::FontColor(_) => json!({ "type": "color" }),
    });
    let header = sh.value(CellRef::new(scope.range.start.row, col));
    json!({
        "range": scope.range.a1(),
        "table": scope.table,
        "column": kition_sheet_core::col_to_letters(col),
        "header": header.display(),
        "kind": kind,
        "values": values,
        "blanks": blanks,
        "criterion": criterion,
        "filtered": scope.criteria.iter().map(|(o, _)| kition_sheet_core::col_to_letters(scope.range.start.col + o)).collect::<Vec<_>>(),
    })
}

/// Distinct display values of a filter column (for the dropdown list).
pub fn filter_values(wb: &Workbook, sheet: usize, col: u32) -> Vec<(String, bool)> {
    let Some(sh) = wb.sheet(sheet) else { return vec![] };
    let Some(scope) = filter_scopes(sh).into_iter().find(|f| (f.range.start.col..=f.range.end.col).contains(&col)) else { return vec![] };
    let answer = column_values(wb, sh, &scope, col);
    let mut out: Vec<(String, bool)> = answer["values"]
        .as_array()
        .map(|values| values.iter().map(|v| (v["text"].as_str().unwrap_or_default().to_owned(), v["checked"].as_bool().unwrap_or(true))).collect())
        .unwrap_or_default();
    if let Some(blanks) = answer["blanks"].as_object() {
        out.push(("(Blanks)".into(), blanks.get("checked").and_then(Json::as_bool).unwrap_or(true)));
    }
    out
}

fn sort_key(v: &Value) -> (u8, i64, String) {
    match v {
        Value::Number(n) => (0, (n * 1e6) as i64, String::new()),
        Value::Text(t) => (1, 0, t.to_lowercase()),
        Value::Bool(b) => (2, *b as i64, String::new()),
        Value::Empty => (9, 0, String::new()),
        _ => (3, 0, v.display()),
    }
}

// ---------------------------------------------------------------- tools

fn remove_duplicates(s: &mut Session, p: &Json) -> Result<Json> {
    let r = data_range(s, p)?;
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    // Rows and columns past the data are blank: only the part with data is compared (whole
    // columns compared a million blank rows and reported them as removed duplicates).
    let r = sh.used_range().and_then(|u| r.intersection(&u)).unwrap_or(RangeRef::cell(r.start));
    let header = bool_param(p, "header").unwrap_or_else(|| guess_header(&d.wb, sh, r));
    let cols: Vec<u32> = p
        .get("columns")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|c| col_param(Some(c), r.start.col)).collect())
        .unwrap_or_else(|| (r.start.col..=r.end.col).collect());
    let sheet = d.wb.active_sheet;
    let start = r.start.row + header as u32;
    let mut removed = 0;
    edit(s, |cx| {
        let Some(sh) = cx.wb.sheet(sheet) else { return Ok(()) };
        let mut seen = std::collections::HashSet::new();
        let mut keep: Vec<u32> = Vec::new();
        for row in start..=r.end.row {
            let key: Vec<String> = cols.iter().map(|c| sh.value(CellRef::new(row, *c)).display().to_lowercase()).collect();
            if seen.insert(key) {
                keep.push(row);
            } else {
                removed += 1;
            }
        }
        let pictures: Vec<_> = keep
            .iter()
            .enumerate()
            .flat_map(|(i, row)| {
                (r.start.col..=r.end.col)
                    .filter_map(move |col| sh.cell_pictures.get(&CellRef::new(*row, col)).map(|p| (CellRef::new(start + i as u32, col), p.clone())))
            })
            .collect();
        let rows: Vec<Vec<Option<Cell>>> =
            keep.iter().map(|row| (r.start.col..=r.end.col).map(|c| sh.cell(CellRef::new(*row, c)).cloned()).collect()).collect();
        let shm = cx.sheet_mut(sheet)?;
        for row in start..=r.end.row {
            for c in r.start.col..=r.end.col {
                shm.remove_cell(CellRef::new(row, c));
            }
        }
        for (i, cells) in rows.into_iter().enumerate() {
            for (j, cell) in cells.into_iter().enumerate() {
                if let Some(cell) = cell {
                    shm.set_cell(CellRef::new(start + i as u32, r.start.col + j as u32), cell);
                }
            }
        }
        shm.cell_pictures.extend(pictures);
        for c in RangeRef::new(CellRef::new(start, r.start.col), r.end).iter() {
            cx.changed.push((sheet, c));
        }
        Ok(())
    })?;
    let remaining = (r.end.row + 1 - start) as usize - removed;
    Ok(json!({"removed": removed, "remaining": remaining}))
}

fn text_to_columns(s: &mut Session, p: &Json) -> Result<Json> {
    let r = target_range(s, p)?;
    let sheet = target_sheet(s, p)?;
    let mut delims: Vec<char> = p
        .get("delimiters")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|d| d.as_str().and_then(|x| x.chars().next())).collect())
        .unwrap_or_else(|| vec!['\t']);
    if let Some(o) = str_param(p, "other").and_then(|o| o.chars().next()) {
        delims.push(o);
    }
    let fixed: Option<Vec<usize>> =
        p.get("fixedWidths").and_then(Json::as_array).map(|a| a.iter().filter_map(|x| x.as_u64().map(|v| v as usize)).collect());
    let consecutive = bool_param(p, "treatConsecutive").unwrap_or(false);
    let quote = str_param(p, "textQualifier").and_then(|q| q.chars().next()).unwrap_or('"');
    let dest = cell_param(p, "destination").unwrap_or(r.start);
    edit(s, |cx| {
        let Some(sh) = cx.wb.sheet(sheet) else { return Ok(()) };
        let mut out: Vec<(CellRef, String)> = Vec::new();
        for (i, row) in (r.start.row..=r.end.row).enumerate() {
            let text = sh.value(CellRef::new(row, r.start.col)).display();
            let parts: Vec<String> = match &fixed {
                Some(w) => {
                    let chars: Vec<char> = text.chars().collect();
                    let mut cuts = vec![0];
                    cuts.extend(w.iter().copied().filter(|x| *x < chars.len()));
                    cuts.push(chars.len());
                    cuts.windows(2)
                        .map(|p| chars.get(p[0]..p[1]).map(|s| s.iter().collect::<String>().trim().to_string()).unwrap_or_default())
                        .collect()
                }
                None => split_delimited(&text, &delims, consecutive, quote),
            };
            for (j, part) in parts.into_iter().enumerate() {
                out.push((CellRef::new(dest.row + i as u32, dest.col + j as u32), part));
            }
        }
        for (c, t) in out {
            let cell = super::edit::input_to_cell(&t, sheet, c, &mut cx.wb).unwrap_or(None);
            let shm = cx.sheet_mut(sheet)?;
            match cell {
                Some(x) => shm.set_cell(c, x),
                None => {
                    shm.remove_cell(c);
                }
            }
            cx.changed.push((sheet, c));
        }
        Ok(())
    })?;
    ok()
}

fn split_delimited(text: &str, delims: &[char], consecutive: bool, quote: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut last_delim = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == quote {
            if in_q && chars.peek() == Some(&quote) {
                cur.push(quote);
                chars.next();
            } else {
                in_q = !in_q;
            }
            continue;
        }
        if !in_q && delims.contains(&c) {
            if consecutive && last_delim {
                continue;
            }
            out.push(std::mem::take(&mut cur));
            last_delim = true;
            continue;
        }
        last_delim = false;
        cur.push(c);
    }
    out.push(cur);
    out
}

// ---------------------------------------------------------------- validation

/// A comparison operator: Excel's names (`greaterThan`, `lessThanOrEqual`, …, as files store
/// them) or the short ones (`greater`, `lessOrEqual`, …); anything else is refused.
pub(crate) fn operator_param(command: &str, given: Option<&str>, default: CfOperator) -> Result<CfOperator> {
    Ok(match given {
        None => default,
        Some("between") => CfOperator::Between,
        Some("notBetween") => CfOperator::NotBetween,
        Some("equal") => CfOperator::Equal,
        Some("notEqual") => CfOperator::NotEqual,
        Some("greater" | "greaterThan") => CfOperator::Greater,
        Some("less" | "lessThan") => CfOperator::Less,
        Some("greaterOrEqual" | "greaterThanOrEqual") => CfOperator::GreaterOrEqual,
        Some("lessOrEqual" | "lessThanOrEqual") => CfOperator::LessOrEqual,
        Some(other) => {
            return Err(bad(
                command,
                format!(
                    "unknown operator `{other}`: use between, notBetween, equal, notEqual, greaterThan, lessThan, greaterThanOrEqual or lessThanOrEqual"
                ),
            ));
        }
    })
}

/// `r` minus `cut`, as up to four non-overlapping rectangles.
fn subtract_range(r: RangeRef, cut: &RangeRef) -> Vec<RangeRef> {
    let Some(i) = r.intersection(cut) else { return vec![r] };
    let mut out = Vec::new();
    if i.start.row > r.start.row {
        out.push(RangeRef::new(r.start, CellRef::new(i.start.row - 1, r.end.col)));
    }
    if i.end.row < r.end.row {
        out.push(RangeRef::new(CellRef::new(i.end.row + 1, r.start.col), r.end));
    }
    if i.start.col > r.start.col {
        out.push(RangeRef::new(CellRef::new(i.start.row, r.start.col), CellRef::new(i.end.row, i.start.col - 1)));
    }
    if i.end.col < r.end.col {
        out.push(RangeRef::new(CellRef::new(i.start.row, i.end.col + 1), CellRef::new(i.end.row, r.end.col)));
    }
    out
}

/// Removes the cells of `cuts` from every validation, keeping the rest of each rule's ranges.
fn remove_validation_cells(validations: &mut Vec<Validation>, cuts: &[RangeRef]) {
    for dv in validations.iter_mut() {
        for cut in cuts {
            dv.ranges = dv.ranges.iter().flat_map(|r| subtract_range(*r, cut)).collect();
        }
    }
    validations.retain(|d| !d.ranges.is_empty());
}

fn validation(s: &mut Session, p: &Json) -> Result<Json> {
    let ranges = target_ranges(s, p)?;
    let sheet = target_sheet(s, p)?;
    if bool_param(p, "clear").unwrap_or(false) {
        return edit(s, |cx| {
            let sh = cx.sheet_mut(sheet)?;
            remove_validation_cells(&mut sh.validations, &ranges);
            Ok(Json::Null)
        });
    }
    let Some(kind) = str_param(p, "type") else {
        s.ui_requests.push(crate::UiRequest::Dialog("dataValidation".into(), json!({})));
        return ok();
    };
    let kind = match kind {
        "whole" => ValidationKind::Whole,
        "decimal" => ValidationKind::Decimal,
        "list" => ValidationKind::List,
        "date" => ValidationKind::Date,
        "time" => ValidationKind::Time,
        "textLength" => ValidationKind::TextLength,
        "custom" => ValidationKind::Custom,
        _ => ValidationKind::Any,
    };
    let dv = Validation {
        ranges: ranges.clone(),
        kind,
        op: operator_param("data.validation", str_param(p, "operator"), CfOperator::Between)?,
        f1: str_param(p, "formula1").unwrap_or("").trim_start_matches('=').to_string(),
        f2: str_param(p, "formula2").map(|f| f.trim_start_matches('=').to_string()),
        allow_blank: bool_param(p, "ignoreBlank").unwrap_or(true),
        in_cell_dropdown: bool_param(p, "dropdown").unwrap_or(true),
        input_title: str_param(p, "inputTitle").unwrap_or("").into(),
        input_message: str_param(p, "inputMessage").unwrap_or("").into(),
        show_input: bool_param(p, "showInput").unwrap_or(true),
        error_title: str_param(p, "errorTitle").unwrap_or("").into(),
        error_message: str_param(p, "errorMessage").unwrap_or("").into(),
        error_style: match str_param(p, "errorStyle") {
            Some("warning") => ErrorStyle::Warning,
            Some("information") => ErrorStyle::Information,
            _ => ErrorStyle::Stop,
        },
        show_error: bool_param(p, "showError").unwrap_or(true),
    };
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        remove_validation_cells(&mut sh.validations, &ranges);
        sh.validations.push(dv.clone());
        Ok(Json::Null)
    })
}

/// List entries of a list validation (literal or from a range).
pub fn list_items(wb: &Workbook, sheet: usize, dv: &Validation) -> Vec<String> {
    let f = dv.f1.trim();
    if f.starts_with('"')
        || (!f.contains('!')
            && kition_sheet_formula::parse(f)
                .map(|e| !matches!(e, kition_sheet_formula::Expr::Ref(_) | kition_sheet_formula::Expr::Name(_)))
                .unwrap_or(true))
    {
        return f.trim_matches('"').split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    }
    let at = dv.ranges.first().map(|r| r.start).unwrap_or_default();
    match kition_sheet_calc::evaluate(wb, sheet, at, f) {
        Value::Array(a) => a.data.iter().filter(|v| !v.is_empty()).map(Value::display).collect(),
        v if !v.is_empty() && !v.is_error() => vec![v.display()],
        _ => vec![],
    }
}

/// Checks typed input against the validation of a cell. `Ok(None)` = valid.
pub fn check_validation(wb: &Workbook, sheet: usize, at: CellRef, input: &str) -> Option<(Validation, String)> {
    let sh = wb.sheet(sheet)?;
    let dv = sh.validations.iter().find(|d| d.ranges.iter().any(|r| r.contains(at)))?;
    if dv.kind == ValidationKind::Any || !dv.show_error {
        return None;
    }
    if input.is_empty() && dv.allow_blank {
        return None;
    }
    let v = kition_sheet_core::parse::parse_input(input, wb.date_system).value;
    let num = |f: &str| kition_sheet_calc::evaluate(wb, sheet, at, f).to_number().ok();
    let ok = match dv.kind {
        ValidationKind::List => list_items(wb, sheet, dv).iter().any(|x| x.eq_ignore_ascii_case(input)),
        ValidationKind::Custom => kition_sheet_calc::evaluate(wb, sheet, at, &dv.f1).to_bool().unwrap_or(false),
        ValidationKind::TextLength => {
            let n = input.chars().count() as f64;
            compare_op(dv.op, n, num(&dv.f1), dv.f2.as_deref().and_then(num))
        }
        _ => match v.as_f64() {
            Some(n) => {
                if dv.kind == ValidationKind::Whole && n.fract() != 0.0 {
                    false
                } else {
                    compare_op(dv.op, n, num(&dv.f1), dv.f2.as_deref().and_then(num))
                }
            }
            None => false,
        },
    };
    if ok {
        return None;
    }
    let msg = if dv.error_message.is_empty() {
        "This value doesn't match the data validation restrictions defined for this cell.".to_string()
    } else {
        dv.error_message.clone()
    };
    Some((dv.clone(), msg))
}

fn compare_op(op: CfOperator, n: f64, a: Option<f64>, b: Option<f64>) -> bool {
    let Some(a) = a else { return false };
    match op {
        CfOperator::Between => b.is_some_and(|b| n >= a.min(b) && n <= a.max(b)),
        CfOperator::NotBetween => b.is_some_and(|b| n < a.min(b) || n > a.max(b)),
        CfOperator::Equal => n == a,
        CfOperator::NotEqual => n != a,
        CfOperator::Greater => n > a,
        CfOperator::Less => n < a,
        CfOperator::GreaterOrEqual => n >= a,
        CfOperator::LessOrEqual => n <= a,
    }
}

fn validate_cmd(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let at = cell_param(p, "cell").unwrap_or(d.selection.active);
    let input = str_param(p, "input").unwrap_or("");
    match check_validation(&d.wb, d.wb.active_sheet, at, input) {
        None => Ok(json!({"ok": true})),
        Some((dv, msg)) => Ok(json!({"ok": false, "message": msg, "title": dv.error_title, "style": format!("{:?}", dv.error_style)})),
    }
}

fn circle_invalid(s: &mut Session, _: &Json) -> Result<Json> {
    let d = s.doc()?;
    let sheet = d.wb.active_sheet;
    let Some(sh) = d.wb.sheet(sheet) else { return Err(EngineError::NoDocument) };
    let mut bad_cells = Vec::new();
    for dv in &sh.validations {
        for r in &dv.ranges {
            let r = sh.used_range().and_then(|u| r.intersection(&u));
            let Some(r) = r else { continue };
            for c in r.iter().take(100_000) {
                let text = sh.input_text(c);
                if !text.is_empty() && check_validation(&d.wb, sheet, c, &text).is_some() {
                    bad_cells.push(c.a1());
                }
            }
        }
    }
    Ok(json!({"invalid": bad_cells}))
}

// ---------------------------------------------------------------- outline

pub fn outline_lines(s: &Session, p: &Json) -> Result<(bool, u32, u32)> {
    // One row or column may be given alone (`"3"`, `"B"`), as `home.insertRows` takes it.
    let span = |key: &str| str_param(p, key).map(|t| if t.contains(':') { t.to_string() } else { format!("{t}:{t}") });
    if let Some(r) = span("rows").as_deref().and_then(RangeRef::parse) {
        return Ok((true, r.start.row, r.end.row));
    }
    if let Some(r) = span("cols").as_deref().and_then(RangeRef::parse) {
        return Ok((false, r.start.col, r.end.col));
    }
    let r = s.doc()?.selection.current();
    if r.is_full_cols() && !r.is_full_rows() { Ok((false, r.start.col, r.end.col)) } else { Ok((true, r.start.row, r.end.row)) }
}

fn group(s: &mut Session, p: &Json, on: bool) -> Result<Json> {
    let (rows, a, b) = outline_lines(s, p)?;
    edit(s, |cx| {
        let si = cx.sheet_index();
        let sh = cx.sheet_mut(si)?;
        let map = if rows { &mut sh.rows } else { &mut sh.cols };
        for i in a..=b.min(a + 1_048_575) {
            let e = map.entry(i).or_default();
            e.outline = if on { (e.outline + 1).min(7) } else { e.outline.saturating_sub(1) };
            if !on && e.outline == 0 {
                e.hidden = false;
            }
        }
        Ok(Json::Null)
    })
}

fn detail(s: &mut Session, p: &Json, hide: bool) -> Result<Json> {
    let (rows, a, b) = outline_lines(s, p)?;
    edit(s, |cx| {
        let si = cx.sheet_index();
        let sh = cx.sheet_mut(si)?;
        let map = if rows { &mut sh.rows } else { &mut sh.cols };
        // Expand to the whole group around the selection.
        let level = (a..=b).filter_map(|i| map.get(&i).map(|e| e.outline)).max().unwrap_or(0);
        if level == 0 {
            return Ok(Json::Null);
        }
        let mut lo = a;
        while lo > 0 && map.get(&(lo - 1)).is_some_and(|e| e.outline >= level) {
            lo -= 1;
        }
        let mut hi = b;
        while map.get(&(hi + 1)).is_some_and(|e| e.outline >= level) {
            hi += 1;
        }
        for i in lo..=hi {
            if let Some(e) = map.get_mut(&i) {
                e.hidden = hide;
            }
        }
        cx.structural = true;
        Ok(Json::Null)
    })
}

fn subtotal(s: &mut Session, p: &Json) -> Result<Json> {
    let r = data_range(s, p)?;
    let group_col = col_param(p.get("groupBy"), r.start.col).unwrap_or(r.start.col);
    let func = str_param(p, "function").unwrap_or("sum");
    let code = match func {
        "count" => 3,
        "average" => 1,
        "max" => 4,
        "min" => 5,
        "product" => 6,
        _ => 9,
    };
    let cols: Vec<u32> = p
        .get("columns")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|c| col_param(Some(c), r.start.col)).collect())
        .unwrap_or_else(|| vec![r.end.col]);
    let label = match func {
        "count" => "Count",
        "average" => "Average",
        "max" => "Max",
        "min" => "Min",
        "product" => "Product",
        _ => "Total",
    };
    let sheet = s.doc()?.wb.active_sheet;
    // Group boundaries (header in the first row).
    let d = s.doc()?;
    let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    let mut groups: Vec<(u32, u32, String)> = Vec::new();
    let mut start = r.start.row + 1;
    for row in r.start.row + 1..=r.end.row + 1 {
        let cur = sh.value(CellRef::new(row, group_col)).display();
        let prev = sh.value(CellRef::new(start, group_col)).display();
        if row > r.end.row || cur != prev {
            groups.push((start, row - 1, prev));
            start = row;
        }
    }
    // Insert a total row after each group, bottom-up so indices stay valid.
    let mut inserted = 0;
    for (g0, g1, name) in groups.iter().rev() {
        s.execute("home.insertRows", json!({"rows": format!("{}:{}", g1 + 2, g1 + 2)}))?;
        let row = g1 + 1;
        let lbl = CellRef::new(row, group_col).a1();
        s.execute("cell.set", json!({"cell": lbl, "input": format!("{name} {label}")}))?;
        for c in &cols {
            let range = RangeRef::new(CellRef::new(*g0, *c), CellRef::new(*g1, *c)).a1();
            s.execute("cell.set", json!({"cell": CellRef::new(row, *c).a1(), "input": format!("=SUBTOTAL({code},{range})")}))?;
        }
        s.execute("home.bold", json!({"range": RangeRef::new(CellRef::new(row, r.start.col), CellRef::new(row, r.end.col)).a1(), "on": true}))?;
        s.execute("data.group", json!({"rows": format!("{}:{}", g0 + 1, g1 + 1)}))?;
        inserted += 1;
    }
    let last = r.end.row + inserted + 1;
    let lbl = CellRef::new(last, group_col).a1();
    s.execute("home.insertRows", json!({"rows": format!("{}:{}", last + 1, last + 1)}))?;
    s.execute("cell.set", json!({"cell": lbl, "input": format!("Grand {label}")}))?;
    for c in &cols {
        let range = RangeRef::new(CellRef::new(r.start.row + 1, *c), CellRef::new(last - 1, *c)).a1();
        s.execute("cell.set", json!({"cell": CellRef::new(last, *c).a1(), "input": format!("=SUBTOTAL({code},{range})")}))?;
    }
    Ok(json!({"groups": inserted}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_sort_key_row_is_a_parameter_error() {
        let mut session = Session::new();
        session.new_workbook();
        session.execute("range.setValues", json!({"range":"A1","values":[[3,2,1],[6,5,4]]})).unwrap();
        let before = session.doc().unwrap().wb.clone();
        let error = session.execute("data.sort", json!({"range":"A1:C2","header":false,"orientation":"columns","keys":[{"row":0}]}));
        assert!(matches!(error, Err(EngineError::BadParams { .. })), "invalid row must return a parameter error: {error:?}");
        assert!(std::sync::Arc::ptr_eq(&before, &session.doc().unwrap().wb));
    }

    #[test]
    fn partial_validation_edits_keep_the_rest_of_the_old_rule() {
        let mut s = Session::new();
        s.new_workbook();
        let rule = |range: &str, lo: &str, hi: &str| json!({"range": range, "type": "whole", "operator": "between", "formula1": lo, "formula2": hi});
        let ok = |s: &mut Session, cell: &str, input: &str| {
            s.execute("data.validate", json!({"cell": cell, "input": input})).unwrap()["ok"] == json!(true)
        };
        s.execute("data.validation", rule("B1:B3", "1", "10")).unwrap();
        s.execute("data.validation", json!({"range": "B2", "clear": true})).unwrap();
        assert!(ok(&mut s, "B2", "999"));
        assert!(!ok(&mut s, "B1", "999"));
        assert!(!ok(&mut s, "B3", "999"));
        s.execute("data.validation", rule("B2:B4", "100", "200")).unwrap();
        for cell in ["B2", "B3", "B4"] {
            assert!(ok(&mut s, cell, "150"), "{cell}");
            assert!(!ok(&mut s, cell, "5"), "{cell}");
        }
        assert!(ok(&mut s, "B1", "5"));
        assert!(!ok(&mut s, "B1", "150"));
    }

    #[test]
    fn split() {
        assert_eq!(split_delimited("a,b,,c", &[','], false, '"'), ["a", "b", "", "c"]);
        assert_eq!(split_delimited("a,b,,c", &[','], true, '"'), ["a", "b", "c"]);
        assert_eq!(split_delimited("\"x,y\",z", &[','], false, '"'), ["x,y", "z"]);
    }
}
