//! File › Export as Picture of a range on another sheet (`Sheet!A1:B2`) draws that sheet's cells,
//! not the active sheet's at the same address (task card C4/C5/C6-sheet: a command given its sheet
//! acts there), and refuses a sheet the workbook lacks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[test]
fn a_picture_of_another_sheets_range_draws_that_sheet() {
    let mut book = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    book.execute("range.setValues", json!({ "range": "A1", "values": [["active", "sheet"], [1, 2]] })).unwrap();
    book.execute("sheet.insert", json!({ "name": "Notes", "before": 9 })).unwrap();
    book.execute("range.setValues", json!({ "range": "A1", "values": [["other", "words"], [3, 4]] })).unwrap();
    let there = book.execute("file.export", json!({ "format": "png", "range": "A1:B2" })).unwrap();
    book.execute("sheet.activate", json!({ "sheet": 0 })).unwrap();
    let named = book.execute("file.export", json!({ "format": "png", "range": "Notes!A1:B2" })).unwrap();
    assert_eq!(named["data"], there["data"], "the picture of Notes!A1:B2 is Notes' cells");
    assert_eq!(book.inspect().unwrap()["sheet"], "Sheet1", "exporting moved nothing");
    let error = book.execute("file.export", json!({ "format": "png", "range": "Nowhere!A1:B2" })).unwrap_err();
    assert!(error.contains("Nowhere"), "{error}");
}
