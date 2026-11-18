//! Regression: grouped rows and columns get Excel's outline gutter
//! beside the headings, a 16 px slot per level plus one, with the group's bracket, a - (or +,
//! once collapsed) button at its summary row or column that collapses and expands it, and the
//! level buttons 1, 2, … in the corner; the cells move over to make room.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::tools::{KitTools, PointerAction, ToolMods, ToolPointer};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

const HEADER_W: f64 = 46.0;
const HEADER_H: f64 = 21.0;
const SLOT: f64 = 16.0;
/// A new sheet's column width and row height (WPS's 69 × 19, module M10).
const COL: f64 = kition_sheet_engine::model::DEFAULT_COL_WIDTH as f64;
const ROW: f64 = kition_sheet_engine::model::DEFAULT_ROW_HEIGHT as f64;

fn run(doc: &mut SheetDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn click(doc: &mut SheetDocument, x: f64, y: f64) {
    for action in [PointerAction::Down, PointerAction::Up] {
        doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }).unwrap();
    }
}

/// Dark pixels in the document area.
fn ink(doc: &SheetDocument, x: f64, y: f64, w: u32, h: u32) -> usize {
    doc.render(x, y, 1.0, w, h).unwrap().as_chunks::<4>().0.iter().filter(|p| p[0] < 150 && p[1] < 150 && p[2] < 150).count()
}

fn grouped() -> SheetDocument {
    let mut doc = SheetDocument::create("Outline.xlsx", 0, 0).unwrap();
    for (cell, input) in [("A1", "Region"), ("A2", "North"), ("A3", "South"), ("A4", "West"), ("A5", "Total")] {
        run(&mut doc, "cell.set", json!({ "cell": cell, "input": input }));
    }
    doc
}

#[test]
fn grouped_rows_get_a_gutter_with_buttons() {
    let mut doc = grouped();
    let (width, height) = (doc.width(), doc.height());
    run(&mut doc, "data.group", json!({ "rows": "2:4" }));
    // One level: two slots (the group's and the detail's) left of the row headings.
    assert_eq!(doc.width(), width + (2.0 * SLOT) as u32, "the cells move over");
    assert_eq!(doc.height(), height, "no column groups, no gutter above");
    let gutter = 2.0 * SLOT;
    let row_top = |row: f64| HEADER_H + ROW * row;
    // The bracket along rows 2 to 4 and the button at row 5, in the first slot.
    assert!(ink(&doc, 0.0, row_top(1.0) + 4.0, SLOT as u32, 50) > 20, "the bracket");
    assert!(ink(&doc, 0.0, row_top(4.0), SLOT as u32, 20) > 15, "the - button");
    assert_eq!(ink(&doc, SLOT, row_top(1.0), SLOT as u32, 80), 0, "the detail slot is empty");
    // The level buttons in the corner.
    assert!(ink(&doc, 0.0, 0.0, gutter as u32, HEADER_H as u32) > 20, "level buttons 1 and 2");

    // Clicking a cell still selects it where it is drawn.
    click(&mut doc, gutter + HEADER_W + 32.0, row_top(2.0) + 10.0);
    assert_eq!(doc.inspect().unwrap()["active"], json!("A3"));

    // The - button collapses the group, the + expands it.
    click(&mut doc, SLOT / 2.0, row_top(4.0) + 10.0);
    assert_eq!(doc.height(), height - (3.0 * ROW) as u32, "rows 2 to 4 hidden");
    click(&mut doc, SLOT / 2.0, row_top(1.0) + 10.0);
    assert_eq!(doc.height(), height, "the + at the summary row (now just under row 1) shows them");
    // Level 1 shows only the summaries; level 2 everything.
    click(&mut doc, SLOT / 2.0, HEADER_H / 2.0);
    assert_eq!(doc.height(), height - (3.0 * ROW) as u32);
    click(&mut doc, SLOT * 1.5, HEADER_H / 2.0);
    assert_eq!(doc.height(), height);
}

#[test]
fn grouped_columns_get_a_gutter_above() {
    let mut doc = grouped();
    let (width, height) = (doc.width(), doc.height());
    run(&mut doc, "data.group", json!({ "cols": "B:C" }));
    assert_eq!(doc.height(), height + (2.0 * SLOT) as u32);
    assert_eq!(doc.width(), width);
    let col_left = |col: f64| HEADER_W + COL * col;
    assert!(ink(&doc, col_left(1.0) + 4.0, 0.0, 120, SLOT as u32) > 20, "the bracket over B and C");
    // The - button over column D collapses B and C.
    click(&mut doc, col_left(3.0) + 32.0, SLOT / 2.0);
    assert_eq!(doc.width(), width - (2.0 * COL) as u32);
}
