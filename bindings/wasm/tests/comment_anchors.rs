//! Comment anchors for a host that keeps comment threads itself (Kit UI protocol 2.6 section 7k):
//! `editor.commentAnchor` names the selection by the replica's objects, and `editor.commentPlaces`
//! finds it again after rows and columns are inserted and deleted, the sheet is renamed or moved,
//! and on another replica of the workbook. Neither changes the workbook.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

/// Values in A1:D6, so rows 1 to 6 and columns A to D have objects.
fn workbook() -> SheetDocument {
    let mut doc = SheetDocument::create("Comments.xlsx", 0, 0).unwrap();
    for (row, values) in [
        ["Item", "Q1", "Q2", "Q3"],
        ["Rent", "1200", "1200", "1250"],
        ["Food", "450", "470", "430"],
        ["Travel", "90", "300", "120"],
        ["Other", "40", "35", "60"],
        ["Total", "=SUM(B2:B5)", "=SUM(C2:C5)", "=SUM(D2:D5)"],
    ]
    .iter()
    .enumerate()
    {
        for (col, input) in values.iter().enumerate() {
            let cell = format!("{}{}", ['A', 'B', 'C', 'D'][col], row + 1);
            doc.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
        }
    }
    doc.mark_saved();
    doc
}

fn run(doc: &mut SheetDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// The anchor of `range` (selected, `active` its first cell).
fn anchor(doc: &mut SheetDocument, range: &str) -> Value {
    let active = range.split(':').next().unwrap();
    run(doc, "selection.set", json!({ "range": range, "active": active }));
    run(doc, "editor.commentAnchor", json!({}))["anchor"].clone()
}

/// Where each anchor is now: `Sheet!Range`, or `None` when it is gone.
fn places(doc: &mut SheetDocument, anchors: &[&Value]) -> Vec<Option<String>> {
    let answer = run(doc, "editor.commentPlaces", json!({ "anchors": anchors }));
    answer["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|place| (place["found"] == json!(true)).then(|| format!("{}!{}", place["sheet"].as_str().unwrap(), place["range"].as_str().unwrap())))
        .collect()
}

fn place(doc: &mut SheetDocument, anchor: &Value) -> Option<String> {
    places(doc, &[anchor]).remove(0)
}

#[test]
fn an_anchor_names_its_sheet_rows_and_columns_and_says_where_it_is() {
    let mut doc = workbook();
    let cell = anchor(&mut doc, "B3");
    assert_eq!(cell["ids"].as_array().unwrap().len(), 3, "a cell: its sheet, row and column");
    assert_eq!(cell["label"], "Sheet1!B3");
    let range = anchor(&mut doc, "B2:D5");
    assert_eq!(range["ids"].as_array().unwrap().len(), 5, "a range: its sheet and both corners' rows and columns");
    assert_eq!(range["label"], "Sheet1!B2:D5");
    assert_eq!(place(&mut doc, &cell).as_deref(), Some("Sheet1!B3"));
    assert_eq!(place(&mut doc, &range).as_deref(), Some("Sheet1!B2:D5"));
    let answer = run(&mut doc, "editor.commentPlaces", json!({ "anchors": [cell] }));
    let rect: Vec<f64> = serde_json::from_value(answer["places"][0]["rect"].clone()).unwrap();
    // Default sizes (a new sheet's, WPS's 69 x 19) after the row and column headings (46 x 21).
    let (w, h) = (f64::from(kition_sheet_engine::model::DEFAULT_COL_WIDTH), f64::from(kition_sheet_engine::model::DEFAULT_ROW_HEIGHT));
    assert_eq!(rect, vec![46.0 + w, 21.0 + 2.0 * h, w, h], "the cell's rectangle on the active sheet's canvas");
    assert!(!doc.dirty(), "anchoring and finding change nothing");
}

#[test]
fn anchors_follow_inserted_rows_and_columns() {
    let mut doc = workbook();
    let cell = anchor(&mut doc, "C3");
    let range = anchor(&mut doc, "B2:C4");
    run(&mut doc, "home.insertRows", json!({ "rows": "1:2" }));
    assert_eq!(places(&mut doc, &[&cell, &range]), vec![Some("Sheet1!C5".to_owned()), Some("Sheet1!B4:C6".to_owned())]);
    run(&mut doc, "home.insertColumns", json!({ "cols": "B:B" }));
    assert_eq!(places(&mut doc, &[&cell, &range]), vec![Some("Sheet1!D5".to_owned()), Some("Sheet1!C4:D6".to_owned())]);
    // Inside the range: it grows; below and right of it: nothing moves.
    run(&mut doc, "home.insertRows", json!({ "rows": "5:5" }));
    run(&mut doc, "home.insertColumns", json!({ "cols": "Z:Z" }));
    assert_eq!(places(&mut doc, &[&cell, &range]), vec![Some("Sheet1!D6".to_owned()), Some("Sheet1!C4:D7".to_owned())]);
}

#[test]
fn anchors_follow_deleted_rows_and_columns_and_go_with_their_cells() {
    let mut doc = workbook();
    let cell = anchor(&mut doc, "C4");
    let range = anchor(&mut doc, "B2:D5");
    let first_row = anchor(&mut doc, "A2");
    run(&mut doc, "home.deleteRows", json!({ "rows": "1:1" }));
    run(&mut doc, "home.deleteColumns", json!({ "cols": "A:A" }));
    assert_eq!(places(&mut doc, &[&cell, &range, &first_row]), vec![Some("Sheet1!B3".to_owned()), Some("Sheet1!A1:C4".to_owned()), None]);
    // The range's first row deleted: it shrinks to the rows left, as Excel's ranges do.
    run(&mut doc, "home.deleteRows", json!({ "rows": "1:1" }));
    assert_eq!(places(&mut doc, &[&cell, &range]), vec![Some("Sheet1!B2".to_owned()), Some("Sheet1!A1:C3".to_owned())]);
    // The cell's own row deleted: its anchor is gone, as an Excel comment goes with its cell.
    run(&mut doc, "home.deleteRows", json!({ "rows": "2:2" }));
    assert_eq!(places(&mut doc, &[&cell, &range]), vec![None, Some("Sheet1!A1:C2".to_owned())]);
    // Undo brings the row back, and the anchor with it.
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(place(&mut doc, &cell).as_deref(), Some("Sheet1!B2"));
}

#[test]
fn an_empty_cell_below_the_data_moves_with_rows_inserted_above_the_data() {
    let mut doc = workbook();
    let below = anchor(&mut doc, "F40");
    let columns = anchor(&mut doc, "C:D");
    assert_eq!(place(&mut doc, &below).as_deref(), Some("Sheet1!F40"));
    run(&mut doc, "home.insertRows", json!({ "rows": "2:4" }));
    run(&mut doc, "home.insertColumns", json!({ "cols": "A:A" }));
    assert_eq!(places(&mut doc, &[&below, &columns]), vec![Some("Sheet1!G43".to_owned()), Some("Sheet1!D:E".to_owned())]);
    run(&mut doc, "home.deleteRows", json!({ "rows": "1:1" }));
    assert_eq!(place(&mut doc, &below).as_deref(), Some("Sheet1!G42"));
}

#[test]
fn anchors_keep_their_sheet_through_renames_moves_and_new_sheets() {
    let mut doc = workbook();
    let cell = anchor(&mut doc, "B2");
    run(&mut doc, "sheet.rename", json!({ "name": "Budget 2026" }));
    assert_eq!(place(&mut doc, &cell).as_deref(), Some("Budget 2026!B2"));
    assert_eq!(anchor(&mut doc, "B2")["label"], "'Budget 2026'!B2");
    run(&mut doc, "home.insertSheet", json!({ "name": "Notes", "before": 0 }));
    let answer = run(&mut doc, "editor.commentPlaces", json!({ "anchors": [cell] }));
    let found = &answer["places"][0];
    assert_eq!((found["found"].clone(), found["sheet"].clone(), found["range"].clone()), (json!(true), json!("Budget 2026"), json!("B2")));
    assert_eq!(found["active"], json!(false), "the new sheet is the active one");
    assert!(found.get("rect").is_none(), "a rectangle only on the active sheet");
    run(&mut doc, "sheet.move", json!({ "sheet": "Budget 2026", "to": 0 }));
    run(&mut doc, "sheet.rename", json!({ "sheet": "Budget 2026", "name": "Plan" }));
    assert_eq!(place(&mut doc, &cell).as_deref(), Some("Plan!B2"), "moved and renamed again");
    run(&mut doc, "home.deleteSheet", json!({ "sheet": "Plan" }));
    assert_eq!(place(&mut doc, &cell), None, "its sheet deleted");
}

#[test]
fn another_replica_finds_the_anchor_after_its_own_edits() {
    let mut mine = workbook();
    let cell = anchor(&mut mine, "C3");
    let snapshot = mine.snapshot().unwrap();
    let blobs = mine.blobs().unwrap();
    let mut theirs = SheetDocument::open_with_replica("Comments.xlsx", &[], &snapshot, &blobs).unwrap();
    assert_eq!(place(&mut theirs, &cell).as_deref(), Some("Sheet1!C3"));
    let since = theirs.version().unwrap();
    run(&mut theirs, "home.insertRows", json!({ "rows": "1:1" }));
    run(&mut theirs, "home.insertColumns", json!({ "cols": "A:A" }));
    assert_eq!(place(&mut theirs, &cell).as_deref(), Some("Sheet1!D4"));
    let changes = theirs.export_changes(&since).unwrap();
    mine.import_changes(&changes).unwrap();
    assert_eq!(place(&mut mine, &cell).as_deref(), Some("Sheet1!D4"), "the edit arrived");
}

#[test]
fn malformed_or_foreign_anchors_are_not_found() {
    let mut doc = workbook();
    let other = anchor(&mut workbook(), "B2");
    let answer =
        run(&mut doc, "editor.commentPlaces", json!({ "anchors": [other, { "ids": [] }, { "ids": [1, 2, 3] }, { "ids": ["a", "b"] }, "B2", {}] }));
    let found: Vec<Value> = answer["places"].as_array().unwrap().iter().map(|place| place["found"].clone()).collect();
    assert_eq!(found, vec![json!(false); 6]);
    assert_eq!(run(&mut doc, "editor.commentPlaces", json!({})), json!({ "places": [] }));
    assert!(doc.execute("editor.commentPlaces", json!({ "anchor": [] })).is_err(), "an unknown parameter is refused");
}
