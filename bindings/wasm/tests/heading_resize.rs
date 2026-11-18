//! Columns and rows resize by the lines between their headings, as in Excel: the pointer there
//! shows the resize cursor, a drag sets the new width or height (on every selected whole column
//! or row the line belongs to), and a double-click fits the column to its contents. A drag there
//! used to select the column instead.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::tools::{KitTools, PointerAction, ToolMods, ToolOverlay, ToolPointer, ToolResponse};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

const HEADER_W: f64 = 46.0;
const HEADER_H: f64 = 21.0;
/// A new sheet's column width and row height, in pixels (WPS for Mac's, module M10).
const COL: f64 = kition_sheet_engine::model::DEFAULT_COL_WIDTH as f64;
const ROW: f64 = kition_sheet_engine::model::DEFAULT_ROW_HEIGHT as f64;

fn run(doc: &mut SheetDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn pointer(doc: &mut SheetDocument, action: PointerAction, x: f64, y: f64) -> ToolResponse {
    doc.pointer(&ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }).unwrap()
}

fn drag(doc: &mut SheetDocument, from: (f64, f64), to: (f64, f64)) -> ToolResponse {
    pointer(doc, PointerAction::Down, from.0, from.1);
    let during = pointer(doc, PointerAction::Drag, to.0, to.1);
    pointer(doc, PointerAction::Up, to.0, to.1);
    during
}

fn active(doc: &mut SheetDocument, x: f64, y: f64) -> Value {
    pointer(doc, PointerAction::Down, x, y);
    pointer(doc, PointerAction::Up, x, y);
    doc.inspect().unwrap()["active"].clone()
}

fn sheet() -> SheetDocument {
    SheetDocument::create("Resize.xlsx", 0, 0).unwrap()
}

#[test]
fn dragging_the_line_after_a_column_heading_sets_its_width() {
    let mut doc = sheet();
    let a_edge = HEADER_W + COL;
    // Hovering the line shows the resize cursor; a heading's middle does not.
    assert_eq!(pointer(&mut doc, PointerAction::Move, a_edge - 2.0, HEADER_H / 2.0).cursor, "ew-resize");
    assert_eq!(pointer(&mut doc, PointerAction::Move, a_edge + 2.0, HEADER_H / 2.0).cursor, "ew-resize", "just past it too");
    assert_eq!(pointer(&mut doc, PointerAction::Move, HEADER_W + COL / 2.0, HEADER_H / 2.0).cursor, "cell");

    let width = doc.width();
    let during = drag(&mut doc, (a_edge, HEADER_H / 2.0), (a_edge + 86.0, HEADER_H / 2.0));
    assert_eq!(during.cursor, "ew-resize");
    assert!(during.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { dashed: true, .. })), "a guide line shows where the column will end");
    assert_eq!(doc.width(), width + 86, "column A is 86 pixels wider");
    assert_eq!(active(&mut doc, HEADER_W + COL + 40.0, HEADER_H + 5.0), json!("A1"), "the cells follow the new width");
    assert_ne!(doc.inspect().unwrap()["selection"], json!("A:A"), "the drag did not select the column");

    // One undo step brings it back.
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(doc.width(), width);
}

#[test]
fn a_resize_sets_every_selected_whole_column_and_rows_resize_too() {
    let mut doc = sheet();
    run(&mut doc, "selection.set", json!({ "range": "B:D" }));
    let width = doc.width();
    // The line after C, one of the selected columns, narrowed by 24.
    let c_edge = HEADER_W + 3.0 * COL;
    drag(&mut doc, (c_edge, HEADER_H / 2.0), (c_edge - 24.0, HEADER_H / 2.0));
    assert_eq!(doc.width(), width - 3 * 24, "B, C and D are each 24 pixels narrower");

    // Rows: the line under row 2's heading, 30 pixels lower.
    let height = doc.height();
    let row2_edge = HEADER_H + 2.0 * ROW;
    assert_eq!(pointer(&mut doc, PointerAction::Move, HEADER_W / 2.0, row2_edge + 1.0).cursor, "ns-resize");
    drag(&mut doc, (HEADER_W / 2.0, row2_edge), (HEADER_W / 2.0, row2_edge + 30.0));
    assert_eq!(doc.height(), height + 30);
    assert_eq!(active(&mut doc, HEADER_W + 10.0, HEADER_H + ROW + 40.0), json!("A2"), "row 2 is 30 pixels higher");
}

#[test]
fn double_clicking_the_line_fits_the_column_and_a_click_without_moving_changes_nothing() {
    let mut doc = sheet();
    run(&mut doc, "cell.set", json!({ "cell": "A1", "input": "A heading much wider than one column" }));
    let width = doc.width();
    let a_edge = HEADER_W + COL;
    pointer(&mut doc, PointerAction::Down, a_edge, HEADER_H / 2.0);
    pointer(&mut doc, PointerAction::Up, a_edge, HEADER_H / 2.0);
    assert_eq!(doc.width(), width, "a press and release without moving leaves the width");
    pointer(&mut doc, PointerAction::DoubleClick, a_edge, HEADER_H / 2.0);
    assert!(doc.width() > width + 100, "column A fits its text: {} -> {}", width, doc.width());
}
