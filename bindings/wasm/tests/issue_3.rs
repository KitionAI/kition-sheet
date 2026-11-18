//! The grid draws what Excel draws: sparklines (and lists them in
//! `document.inspect`), checkboxes, strikethrough, rotated text, note and comment indicators,
//! AutoFilter buttons and the outline's group brackets; the frozen-pane line stays under charts;
//! right-aligned text wider than its cell spills into empty cells on its left.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

const HEADER_W: f64 = 46.0;
const HEADER_H: f64 = 21.0;
/// Default cell size in document pixels (a new sheet's: WPS's 69 × 19, module M10).
const CELL_W: f64 = kition_sheet_engine::model::DEFAULT_COL_WIDTH as f64;
const CELL_H: f64 = kition_sheet_engine::model::DEFAULT_ROW_HEIGHT as f64;
/// The same in whole pixels, for a shot's coordinates.
const W: usize = CELL_W as usize;
const H: usize = CELL_H as usize;

fn new_doc() -> SheetDocument {
    SheetDocument::create("Grid.xlsx", 0, 0).unwrap()
}

fn run(doc: &mut SheetDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// The pixels of the area (document pixels, at scale 1) as RGBA rows.
struct Shot {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Shot {
    fn take(doc: &SheetDocument, x: f64, y: f64, w: u32, h: u32) -> Shot {
        Shot { w: w as usize, h: h as usize, px: doc.render(x, y, 1.0, w, h).unwrap() }
    }

    /// The area of cell (`col`, `row`) from 0, `cols` × `rows` cells.
    fn cells(doc: &SheetDocument, col: u32, row: u32, cols: u32, rows: u32) -> Shot {
        Shot::take(
            doc,
            HEADER_W + CELL_W * f64::from(col),
            HEADER_H + CELL_H * f64::from(row),
            (CELL_W * f64::from(cols)) as u32,
            (CELL_H * f64::from(rows)) as u32,
        )
    }

    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }

    /// Pixels at least this dark (all channels below `level`) in the box.
    fn dark(&self, x0: usize, y0: usize, x1: usize, y1: usize, level: u8) -> usize {
        let mut n = 0;
        for y in y0..y1.min(self.h) {
            for x in x0..x1.min(self.w) {
                let p = self.at(x, y);
                if p[0] < level && p[1] < level && p[2] < level {
                    n += 1;
                }
            }
        }
        n
    }

    /// The bounding box of the dark pixels: (width, height).
    fn ink(&self, level: u8) -> (usize, usize) {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                let p = self.at(x, y);
                if p[0] < level && p[1] < level && p[2] < level {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        if x0 > x1 { (0, 0) } else { (x1 - x0 + 1, y1 - y0 + 1) }
    }
}

#[test]
fn strikethrough_is_drawn() {
    let mut plain = new_doc();
    run(&mut plain, "cell.set", json!({ "cell": "B2", "input": "MMMM" }));
    let mut struck = new_doc();
    run(&mut struck, "cell.set", json!({ "cell": "B2", "input": "MMMM" }));
    run(&mut struck, "home.strikethrough", json!({ "range": "B2" }));
    let (a, b) = (Shot::cells(&plain, 1, 1, 1, 1), Shot::cells(&struck, 1, 1, 1, 1));
    assert!(b.dark(0, 0, W, H, 120) > a.dark(0, 0, W, H, 120) + 10, "a strikethrough adds a line across the text");
}

#[test]
fn rotated_text_is_drawn_rotated() {
    let mut doc = new_doc();
    run(&mut doc, "cell.set", json!({ "cell": "B2", "input": "Rotated" }));
    run(&mut doc, "home.rowHeight", json!({ "rows": "2", "height": 80 }));
    let flat = Shot::take(&doc, HEADER_W + CELL_W, HEADER_H + CELL_H, W as u32, 80).ink(140);
    run(&mut doc, "home.orientation", json!({ "range": "B2", "angle": "up" }));
    let up = Shot::take(&doc, HEADER_W + CELL_W, HEADER_H + CELL_H, W as u32, 80).ink(140);
    assert!(flat.0 > flat.1, "flat text is wider than tall: {flat:?}");
    assert!(up.1 > up.0, "text turned up is taller than wide: {up:?}");
    run(&mut doc, "home.orientation", json!({ "range": "B2", "angle": "vertical" }));
    let stacked = Shot::take(&doc, HEADER_W + CELL_W, HEADER_H + CELL_H, W as u32, 80).ink(140);
    assert!(stacked.1 > stacked.0 * 2, "stacked letters run down: {stacked:?}");
}

#[test]
fn checkboxes_are_drawn_with_their_value() {
    let mut doc = new_doc();
    run(&mut doc, "insert.checkbox", json!({ "range": "B2:B3" }));
    run(&mut doc, "cell.set", json!({ "cell": "B3", "input": "TRUE" }));
    let empty = Shot::cells(&doc, 1, 1, 1, 1);
    let checked = Shot::cells(&doc, 1, 2, 1, 1);
    assert!(empty.dark(0, 0, W, H, 160) > 20, "an empty box is drawn");
    assert_ne!(empty.px, checked.px, "a checked box differs from an empty one");
}

#[test]
fn sparklines_are_drawn_and_inspected() {
    let mut doc = new_doc();
    run(&mut doc, "range.setValues", json!({ "range": "A1", "values": [[1, 5, 2, 8, 3, 9]] }));
    let before = Shot::cells(&doc, 6, 0, 1, 1);
    run(&mut doc, "insert.sparkline", json!({ "range": "A1:F1", "location": "G1" }));
    let after = Shot::cells(&doc, 6, 0, 1, 1);
    assert_ne!(before.px, after.px, "the sparkline draws in G1");
    let inspected = run(&mut doc, "document.inspect", json!({}));
    assert_eq!(inspected["sheets"][0]["sparklines"], json!([{ "cell": "G1", "source": "A1:F1", "type": "line" }]), "{inspected}");
}

#[test]
fn notes_and_comments_show_an_indicator() {
    let mut doc = new_doc();
    run(&mut doc, "review.newNote", json!({ "cell": "B2", "text": "Check this" }));
    let shot = Shot::cells(&doc, 1, 1, 1, 1);
    let corner = shot.at(W - 3, 2);
    assert!(corner[0] > 180 && corner[1] < 90 && corner[2] < 90, "a red mark in the top-right corner: {corner:?}");
    let other = Shot::cells(&doc, 2, 1, 1, 1).at(W - 3, 2);
    assert!(other[0] > 200 && other[1] > 200, "no mark without a note: {other:?}");
}

#[test]
fn autofilter_headers_show_buttons() {
    let mut doc = new_doc();
    run(&mut doc, "range.setValues", json!({ "range": "A1", "values": [["Name", "Qty"], ["a", 1], ["b", 2]] }));
    let before = Shot::cells(&doc, 0, 0, 2, 1);
    run(&mut doc, "data.filter", json!({ "range": "A1:B3" }));
    let after = Shot::cells(&doc, 0, 0, 2, 1);
    // A button at the right of each header cell.
    for col in 0..2 {
        let x0 = col * W + 46;
        assert!(after.dark(x0, 2, x0 + 17, 18, 200) > before.dark(x0, 2, x0 + 17, 18, 200) + 8, "a button in header {col}");
    }
}

#[test]
fn grouped_rows_show_their_bracket() {
    let mut doc = new_doc();
    run(&mut doc, "range.setValues", json!({ "range": "A1", "values": [[1], [2], [3], [4]] }));
    let before = Shot::take(&doc, 0.0, HEADER_H, HEADER_W as u32, (CELL_H * 5.0) as u32);
    run(&mut doc, "data.group", json!({ "rows": "2:3" }));
    let after = Shot::take(&doc, 0.0, HEADER_H, HEADER_W as u32, (CELL_H * 5.0) as u32);
    assert_ne!(before.px, after.px, "the row headings show the group");
}

#[test]
fn the_freeze_line_stays_under_charts() {
    let mut doc = new_doc();
    run(&mut doc, "range.setValues", json!({ "range": "A1", "values": [["M", "V"], ["a", 1], ["b", 2]] }));
    run(&mut doc, "view.freezePanes", json!({ "cell": "A4" }));
    run(&mut doc, "insert.chart", json!({ "range": "A1:B3", "type": "column", "at": "D2", "width": 300, "height": 200 }));
    // The freeze line runs under row 3 (y = 21 + 60); inside the chart's card (from x = 46 + 192)
    // it is hidden.
    let shot = Shot::take(&doc, HEADER_W + 3.0 * CELL_W + 40.0, HEADER_H + 3.0 * CELL_H - 3.0, 60, 6);
    for y in 0..6 {
        for x in 0..60 {
            assert_ne!(shot.at(x, y), [150, 150, 150, 255], "the freeze line crosses the chart at ({x}, {y})");
        }
    }
}

#[test]
fn right_aligned_text_spills_left() {
    let mut doc = new_doc();
    run(&mut doc, "cell.set", json!({ "cell": "C1", "input": "A rather long label that is right aligned" }));
    run(&mut doc, "home.alignRight", json!({ "range": "C1" }));
    let left = Shot::cells(&doc, 1, 0, 1, 1);
    assert!(left.dark(0, 0, W, H, 140) > 20, "the text runs into B1");
}
