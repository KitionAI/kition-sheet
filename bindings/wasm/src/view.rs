//! The grid as it is shown: with frozen panes taken from a scrolled view (WPS, Excel), the frozen
//! panes start at the sheet view's top-left cell, and the rows above it and the columns left of
//! it are out of reach until the panes are unfrozen. Positions here are in document points from
//! the first row and column shown (before the headings); rows and columns out of reach are
//! zero-sized at the start.

use kition_sheet_engine::model::Sheet;

/// The first row and column the grid shows: the frozen panes' top-left on each frozen axis.
pub fn origin(sheet: &Sheet) -> (u32, u32) {
    match sheet.freeze {
        Some((rows, cols)) => (if rows > 0 { sheet.view_top_left.row } else { 0 }, if cols > 0 { sheet.view_top_left.col } else { 0 }),
        None => (0, 0),
    }
}

/// The frozen rows and columns' end (the first row and column of the scrolling pane).
pub fn split(sheet: &Sheet) -> (u32, u32) {
    let (rows, cols) = sheet.freeze.unwrap_or((0, 0));
    let (r, c) = origin(sheet);
    (r.saturating_add(rows), c.saturating_add(cols))
}

pub fn top(sheet: &Sheet, row: u32) -> f64 {
    let (r, _) = origin(sheet);
    sheet.row_top(row.max(r)) - sheet.row_top(r)
}

pub fn left(sheet: &Sheet, col: u32) -> f64 {
    let (_, c) = origin(sheet);
    sheet.col_left(col.max(c)) - sheet.col_left(c)
}

pub fn row_at(sheet: &Sheet, y: f64) -> u32 {
    let (r, _) = origin(sheet);
    sheet.row_at(y.max(0.0) + sheet.row_top(r)).max(r)
}

pub fn col_at(sheet: &Sheet, x: f64) -> u32 {
    let (_, c) = origin(sheet);
    sheet.col_at(x.max(0.0) + sheet.col_left(c)).max(c)
}

pub fn row_h(sheet: &Sheet, row: u32) -> f32 {
    if row < origin(sheet).0 { 0.0 } else { sheet.row_height(row) }
}

pub fn col_w(sheet: &Sheet, col: u32) -> f32 {
    if col < origin(sheet).1 { 0.0 } else { sheet.col_width(col) }
}
