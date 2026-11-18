//! View tab (sheet view state that's saved with the file) and Page Layout settings.

use kition_sheet_core::{CellRef, RangeRef};
use kition_sheet_model::Orientation;
use serde_json::{Value as Json, json};

use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "view.freezePanes",
            "Freeze Panes",
            ["View", "Window", "Freeze Panes"],
            None,
            "{cell?: \"B2\" (rows above and columns left of it freeze), top?: \"A11\" (the view's top-left cell: the panes start there, rows above and columns left of it stay out of view, as WPS freezes a scrolled view)}",
            has_doc,
            freeze_panes
        ),
        cmd!("view.freezeTopRow", "Freeze Top Row", ["View", "Window", "Freeze Panes"], None, "{top?: \"A11\"}", has_doc, |s, p| set_freeze_at(
            s,
            Some((1, 0)),
            top_param(p)
        )),
        cmd!("view.freezeFirstColumn", "Freeze First Column", ["View", "Window", "Freeze Panes"], None, "{top?: \"D1\"}", has_doc, |s, p| {
            set_freeze_at(s, Some((0, 1)), top_param(p))
        }),
        cmd!(
            "view.freezeRows",
            "Freeze to Current Row",
            ["View", "Window", "Freeze Panes"],
            None,
            "{cell?: \"C5\", top?: \"A11\"} (the rows above it freeze, no columns; WPS's 冻结至第 4 行)",
            has_doc,
            |s, p| freeze_lines(s, p, true)
        ),
        cmd!(
            "view.freezeColumns",
            "Freeze to Current Column",
            ["View", "Window", "Freeze Panes"],
            None,
            "{cell?: \"C5\", top?: \"A11\"} (the columns left of it freeze, no rows; WPS's 冻结至第 B 列)",
            has_doc,
            |s, p| freeze_lines(s, p, false)
        ),
        cmd!("view.unfreezePanes", "Unfreeze Panes", ["View", "Window", "Freeze Panes"], None, "{}", has_doc, |s, _| set_freeze(s, None)),
        cmd!("view.gridlines", "Gridlines", ["View", "Show"], None, "{on?}", has_doc, |s, p| sheet_flag(s, p, "gridlines")),
        cmd!(
            "view.gridlineColor",
            "Gridline Color",
            ["View", "Show"],
            None,
            "{color: \"#RRGGBB\" | \"auto\"} (the active sheet's gridlines; saved with it)",
            has_doc,
            gridline_color
        ),
        cmd!(
            noundo "view.focusCell",
            "Focus Cell",
            ["View", "Show"],
            None,
            "{on?, color?: \"#RRGGBB\"} (without `on` or `color`: toggles) → {on, color}; highlights the selection's rows and columns (WPS's 高亮行列, the person's view, not saved with the workbook)",
            always,
            focus_cell
        ),
        cmd!("view.headings", "Headings", ["View", "Show"], None, "{on?}", has_doc, |s, p| sheet_flag(s, p, "headings")),
        cmd!(
            noundo "view.formulaBar",
            "Formula Bar",
            ["View", "Show"],
            None,
            "{on?} (without `on`: toggles) → {on}; the person's view, not saved with the workbook",
            always,
            formula_bar
        ),
        cmd!("view.showZeros", "Show Zeros", [], None, "{on?}", has_doc, |s, p| sheet_flag(s, p, "zeros")),
        cmd!("view.rightToLeft", "Sheet Right-to-Left", ["Page Layout", "Sheet Options"], None, "{on?}", has_doc, |s, p| sheet_flag(s, p, "rtl")),
        cmd!("view.zoom", "Zoom", ["View", "Zoom"], None, "{percent: 10..400}", has_doc, zoom),
        cmd!("view.zoomToSelection", "Zoom to Selection", ["View", "Zoom"], None, "{viewWidth?, viewHeight?}", has_doc, zoom_selection),
        cmd!("pageLayout.orientation", "Orientation", ["Page Layout", "Page Setup"], None, "{orientation: portrait|landscape}", has_doc, orientation),
        cmd!("pageLayout.size", "Size", ["Page Layout", "Page Setup"], None, "{paper: Letter|Legal|A4|A3|A5|Tabloid|Executive}", has_doc, paper),
        cmd!(
            "pageLayout.margins",
            "Margins",
            ["Page Layout", "Page Setup"],
            None,
            "{preset?: normal|wide|narrow, left?, right?, top?, bottom?, header?, footer? (inches)}",
            has_doc,
            margins
        ),
        cmd!(
            "pageLayout.printArea",
            "Set Print Area",
            ["Page Layout", "Page Setup", "Print Area"],
            None,
            "{range?, clear?: bool}",
            has_doc,
            print_area
        ),
        cmd!(
            "pageLayout.printTitles",
            "Print Titles",
            ["Page Layout", "Page Setup"],
            None,
            "{rows?: \"1:1\", cols?: \"A:A\"}",
            has_doc,
            print_titles
        ),
        cmd!(
            "pageLayout.scaleToFit",
            "Scale to Fit",
            ["Page Layout", "Scale to Fit"],
            None,
            "{scale?: 100, width?: pages, height?: pages}",
            has_doc,
            scale_fit
        ),
        cmd!("pageLayout.printGridlines", "Print Gridlines", ["Page Layout", "Sheet Options"], None, "{on?}", has_doc, |s, p| sheet_flag(
            s,
            p,
            "printGridlines"
        )),
        cmd!("pageLayout.printHeadings", "Print Headings", ["Page Layout", "Sheet Options"], None, "{on?}", has_doc, |s, p| sheet_flag(
            s,
            p,
            "printHeadings"
        )),
        cmd!(
            "pageLayout.breaks",
            "Breaks",
            ["Page Layout", "Page Setup"],
            None,
            "{insert?: bool, remove?: bool, reset?: bool, cell?}",
            has_doc,
            breaks
        ),
        cmd!(
            "pageLayout.headerFooter",
            "Header & Footer",
            ["Insert", "Text"],
            None,
            "{header?: \"&C&P\", footer?: \"&CPage &P of &N\"}",
            has_doc,
            header_footer
        ),
        cmd!(
            "pageLayout.theme",
            "Themes",
            ["Page Layout", "Themes"],
            None,
            "{name: Classic|Slate|Meadow|Ember|Ocean|Orchid|Graphite, colors?: [12 hex], majorFont?, minorFont?}",
            has_doc,
            theme
        ),
    ]
}

/// The view's top-left cell a freeze starts at (`top`), else the sheet's first cell.
fn top_param(p: &Json) -> CellRef {
    cell_param(p, "top").unwrap_or_default()
}

/// The frozen rows and columns above and left of `at`, counted from the view's top-left `top`
/// (WPS and Excel freeze a scrolled view from its top-left); an axis where `at` is above or left
/// of `top` counts from the sheet's start.
fn counted(at: CellRef, top: CellRef) -> ((u32, u32), CellRef) {
    let row = if at.row > top.row { top.row } else { 0 };
    let col = if at.col > top.col { top.col } else { 0 };
    ((at.row - row, at.col - col), CellRef::new(row, col))
}

fn freeze_panes(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    if sh.freeze.is_some() && p.get("cell").is_none() {
        return set_freeze(s, None);
    }
    let at = cell_param(p, "cell").unwrap_or(d.selection.active);
    let ((rows, cols), top) = counted(at, top_param(p));
    set_freeze_at(s, if rows == 0 && cols == 0 { None } else { Some((rows, cols)) }, top)
}

/// Freezes the rows above the active cell (or `cell`), or the columns left of it, alone (WPS's
/// 冻结至第 4 行 and 冻结至第 B 列 replace any frozen panes).
fn freeze_lines(s: &mut Session, p: &Json, rows: bool) -> Result<Json> {
    let id = if rows { "view.freezeRows" } else { "view.freezeColumns" };
    let at = cell_param(p, "cell").unwrap_or(s.doc()?.selection.active);
    let ((r, c), top) = counted(at, top_param(p));
    let f = if rows { (r, 0) } else { (0, c) };
    if f == (0, 0) {
        return Err(bad(
            id,
            if rows { "there are no rows above the active cell to freeze" } else { "there are no columns left of the active cell to freeze" },
        ));
    }
    set_freeze_at(s, Some(f), top)
}

fn set_freeze(s: &mut Session, f: Option<(u32, u32)>) -> Result<Json> {
    edit(s, |cx| {
        let si = cx.sheet_index();
        cx.sheet_mut(si)?.freeze = f;
        Ok(json!({"freeze": f}))
    })
}

/// Freezes `f` rows and columns from the view's top-left cell `top` (the sheet view's top-left
/// cell, `topLeftCell` in the file): the rows above it and the columns left of it are out of view
/// while the panes are frozen.
fn set_freeze_at(s: &mut Session, f: Option<(u32, u32)>, top: CellRef) -> Result<Json> {
    let top = CellRef::new(top.row.min(kition_sheet_core::MAX_ROWS - 1), top.col.min(kition_sheet_core::MAX_COLS - 1));
    let f = f.map(|(r, c)| (r.min(kition_sheet_core::MAX_ROWS - 1 - top.row), c.min(kition_sheet_core::MAX_COLS - 1 - top.col)));
    edit(s, |cx| {
        let si = cx.sheet_index();
        let sh = cx.sheet_mut(si)?;
        sh.freeze = f;
        sh.view_top_left = top;
        Ok(json!({"freeze": f, "top": top.a1()}))
    })
}

fn sheet_flag(s: &mut Session, p: &Json, which: &str) -> Result<Json> {
    edit(s, |cx| {
        let si = cx.sheet_index();
        let sh = cx.sheet_mut(si)?;
        let slot = match which {
            "gridlines" => &mut sh.show_gridlines,
            "headings" => &mut sh.show_headings,
            "zeros" => &mut sh.show_zeros,
            "rtl" => &mut sh.right_to_left,
            "printGridlines" => &mut sh.print.gridlines,
            _ => &mut sh.print.headings,
        };
        *slot = bool_param(p, "on").unwrap_or(!*slot);
        Ok(json!({"on": *slot}))
    })
}

/// The active sheet's gridline colour (`auto`: the automatic grey).
fn gridline_color(s: &mut Session, p: &Json) -> Result<Json> {
    let text = str_param(p, "color").ok_or_else(|| bad("view.gridlineColor", "missing `color`"))?;
    let color = if text.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(hex_color(text).ok_or_else(|| bad("view.gridlineColor", format!("not a colour: `{text}`")))?)
    };
    edit(s, |cx| {
        let si = cx.sheet_index();
        cx.sheet_mut(si)?.grid_color = color;
        Ok(json!({"color": color.map(|c| format!("#{c:06X}"))}))
    })
}

/// `#RRGGBB` (or `RRGGBB`) as 0xRRGGBB.
fn hex_color(text: &str) -> Option<u32> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| u32::from_str_radix(hex, 16).ok()).flatten()
}

/// Highlights the rows and columns of the selection (WPS's 高亮行列, Excel's Focus Cell) in a
/// colour. Like the formula bar it is the person's view: the workbook does not change.
fn focus_cell(s: &mut Session, p: &Json) -> Result<Json> {
    if let Some(text) = str_param(p, "color") {
        let color = hex_color(text).ok_or_else(|| bad("view.focusCell", format!("not a colour: `{text}`")))?;
        s.prefs.focus_color = format!("#{color:06X}");
    }
    s.prefs.focus_cell = match bool_param(p, "on") {
        Some(on) => on,
        // A colour alone keeps it as it is; nothing toggles it.
        None if p.get("color").is_some() => s.prefs.focus_cell,
        None => !s.prefs.focus_cell,
    };
    Ok(json!({"on": s.prefs.focus_cell, "color": s.prefs.focus_color}))
}

/// Shows or hides the formula bar. Like Excel's and WPS's, it is the person's view of every
/// workbook: the workbook does not change (no undo step, nothing to save).
fn formula_bar(s: &mut Session, p: &Json) -> Result<Json> {
    s.prefs.formula_bar = bool_param(p, "on").unwrap_or(!s.prefs.formula_bar);
    Ok(json!({"on": s.prefs.formula_bar}))
}

fn zoom(s: &mut Session, p: &Json) -> Result<Json> {
    let z = f64_param(p, "percent").ok_or_else(|| bad("view.zoom", "missing `percent`"))?.clamp(10.0, 400.0) as u16;
    let d = s.doc_mut()?;
    let dirty = d.is_dirty();
    let wb = std::sync::Arc::make_mut(&mut d.wb);
    let si = wb.active_sheet;
    if let Some(sh) = wb.sheet_mut(si) {
        sh.zoom = z;
    }
    if !dirty {
        d.saved = d.wb.clone();
    }
    Ok(json!({"zoom": z}))
}

fn zoom_selection(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let r = d.selection.current();
    let w = (sh.col_left(r.end.col + 1) - sh.col_left(r.start.col)).max(1.0);
    let h = (sh.row_top(r.end.row + 1) - sh.row_top(r.start.row)).max(1.0);
    let vw = f64_param(p, "viewWidth").unwrap_or(1200.0);
    let vh = f64_param(p, "viewHeight").unwrap_or(700.0);
    let z = ((vw / w).min(vh / h) * 100.0).clamp(10.0, 400.0).floor();
    zoom(s, &json!({"percent": z}))
}

fn orientation(s: &mut Session, p: &Json) -> Result<Json> {
    let o = if str_param(p, "orientation") == Some("landscape") { Orientation::Landscape } else { Orientation::Portrait };
    edit(s, |cx| {
        let si = cx.sheet_index();
        cx.sheet_mut(si)?.print.orientation = o;
        Ok(Json::Null)
    })
}

fn paper(s: &mut Session, p: &Json) -> Result<Json> {
    let paper = str_param(p, "paper").unwrap_or("A4").to_string();
    edit(s, |cx| {
        let si = cx.sheet_index();
        cx.sheet_mut(si)?.print.paper = paper.clone();
        Ok(Json::Null)
    })
}

fn margins(s: &mut Session, p: &Json) -> Result<Json> {
    let preset = match str_param(p, "preset") {
        Some("wide") => Some([1.0, 1.0, 1.0, 1.0, 0.5, 0.5]),
        Some("narrow") => Some([0.25, 0.25, 0.75, 0.75, 0.3, 0.3]),
        Some("normal") => Some([0.7, 0.7, 0.75, 0.75, 0.3, 0.3]),
        _ => None,
    };
    edit(s, |cx| {
        let si = cx.sheet_index();
        let m = &mut cx.sheet_mut(si)?.print.margins;
        if let Some(pr) = preset {
            *m = pr;
        }
        for (i, k) in ["left", "right", "top", "bottom", "header", "footer"].iter().enumerate() {
            if let Some(v) = f64_param(p, k)
                && let Some(slot) = m.get_mut(i)
            {
                *slot = v.clamp(0.0, 20.0) as f32;
            }
        }
        Ok(Json::Null)
    })
}

fn print_area(s: &mut Session, p: &Json) -> Result<Json> {
    let clear = bool_param(p, "clear").unwrap_or(false);
    let r = target_range(s, p)?;
    edit(s, |cx| {
        let si = cx.sheet_index();
        cx.sheet_mut(si)?.print.print_area = if clear { None } else { Some(r) };
        Ok(Json::Null)
    })
}

fn print_titles(s: &mut Session, p: &Json) -> Result<Json> {
    let rows = str_param(p, "rows").and_then(RangeRef::parse).map(|r| (r.start.row, r.end.row));
    let cols = str_param(p, "cols").and_then(RangeRef::parse).map(|r| (r.start.col, r.end.col));
    edit(s, |cx| {
        let si = cx.sheet_index();
        let pr = &mut cx.sheet_mut(si)?.print;
        pr.title_rows = rows;
        pr.title_cols = cols;
        Ok(Json::Null)
    })
}

fn scale_fit(s: &mut Session, p: &Json) -> Result<Json> {
    edit(s, |cx| {
        let si = cx.sheet_index();
        let pr = &mut cx.sheet_mut(si)?.print;
        if let Some(sc) = f64_param(p, "scale") {
            pr.scale = sc.clamp(10.0, 400.0) as u16;
            pr.fit_to = None;
        }
        if p.get("width").is_some() || p.get("height").is_some() {
            pr.fit_to = Some((u32_param(p, "width").unwrap_or(0).min(32767) as u16, u32_param(p, "height").unwrap_or(0).min(32767) as u16));
        }
        Ok(Json::Null)
    })
}

fn breaks(s: &mut Session, p: &Json) -> Result<Json> {
    let at = cell_param(p, "cell").unwrap_or(s.doc()?.selection.active);
    let insert = bool_param(p, "insert").unwrap_or(false);
    let remove = bool_param(p, "remove").unwrap_or(false);
    let reset = bool_param(p, "reset").unwrap_or(false);
    edit(s, |cx| {
        let si = cx.sheet_index();
        let pr = &mut cx.sheet_mut(si)?.print;
        if reset {
            pr.row_breaks.clear();
            pr.col_breaks.clear();
        } else if insert || !remove {
            if at.row > 0 && !pr.row_breaks.contains(&at.row) {
                pr.row_breaks.push(at.row);
                pr.row_breaks.sort_unstable();
            }
            if at.col > 0 && !pr.col_breaks.contains(&at.col) {
                pr.col_breaks.push(at.col);
                pr.col_breaks.sort_unstable();
            }
        } else {
            pr.row_breaks.retain(|r| *r != at.row);
            pr.col_breaks.retain(|c| *c != at.col);
        }
        Ok(Json::Null)
    })
}

fn header_footer(s: &mut Session, p: &Json) -> Result<Json> {
    edit(s, |cx| {
        let si = cx.sheet_index();
        let pr = &mut cx.sheet_mut(si)?.print;
        if let Some(h) = str_param(p, "header") {
            pr.header = h.to_string();
        }
        if let Some(f) = str_param(p, "footer") {
            pr.footer = f.to_string();
        }
        Ok(Json::Null)
    })
}

/// Kition Sheet's own theme set (original palettes).
pub fn themes() -> Vec<(&'static str, [u32; 12])> {
    vec![
        ("Classic", [0xFFFFFF, 0x000000, 0xE8E8E8, 0x0E2841, 0x156082, 0xE97132, 0x196B24, 0x0F9ED5, 0xA02B93, 0x4EA72E, 0x467886, 0x96607D]),
        ("Slate", [0xFFFFFF, 0x1F2328, 0xEEF1F4, 0x3B4652, 0x4F6D8A, 0x8AA1B8, 0xC27C4C, 0x6F9A72, 0x9D6C9B, 0xD3B25A, 0x2F6FB0, 0x7A5C8F]),
        ("Meadow", [0xFFFFFF, 0x16241A, 0xEEF4EA, 0x2E4A33, 0x4D8B3F, 0x9BC53D, 0xE4B33A, 0x2E8C83, 0xC4643C, 0x7B6A9E, 0x2B7A57, 0x6E7F3A]),
        ("Ember", [0xFFFFFF, 0x2A1A14, 0xF6EEE8, 0x5A2E1E, 0xD9532B, 0xF2994A, 0xF2C94C, 0x8C3B2E, 0x4A6FA5, 0x6F8F4E, 0xB5472A, 0x8C6A5A]),
        ("Ocean", [0xFFFFFF, 0x0B1E2D, 0xE8F1F7, 0x123A5A, 0x1F78B4, 0x33A6CC, 0x52C7B8, 0x3B5BA5, 0xF2A65A, 0x8E6CB8, 0x1A6FA8, 0x6A7FB0]),
        ("Orchid", [0xFFFFFF, 0x24152B, 0xF3EDF6, 0x4B2C5E, 0x8E44AD, 0xC56CF0, 0xF78FB3, 0x3DC1D3, 0xF5CD79, 0x63CDDA, 0x7E3FA0, 0xB066B0]),
        ("Graphite", [0xFFFFFF, 0x111111, 0xEDEDED, 0x333333, 0x595959, 0x7F7F7F, 0xA5A5A5, 0x3C78D8, 0xCC4125, 0x6AA84F, 0x3C78D8, 0x8E7CC3]),
    ]
}

fn theme(s: &mut Session, p: &Json) -> Result<Json> {
    let name = str_param(p, "name").unwrap_or("Classic").to_string();
    let colors = p
        .get("colors")
        .and_then(Json::as_array)
        .and_then(|a| {
            let v: Vec<u32> = a.iter().filter_map(|c| c.as_str().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok())).collect();
            <[u32; 12]>::try_from(v).ok()
        })
        .or_else(|| themes().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(&name)).map(|(_, c)| c))
        .ok_or_else(|| bad("pageLayout.theme", format!("unknown theme `{name}`")))?;
    let major = str_param(p, "majorFont").map(str::to_string);
    let minor = str_param(p, "minorFont").map(str::to_string);
    edit(s, |cx| {
        cx.wb.theme.name = name.clone();
        cx.wb.theme.colors = colors;
        if let Some(m) = &major {
            cx.wb.theme.major_font = m.clone();
        }
        if let Some(m) = &minor {
            cx.wb.theme.minor_font = m.clone();
        }
        Ok(Json::Null)
    })
}

#[allow(dead_code)]
fn unused(_: CellRef) {}
