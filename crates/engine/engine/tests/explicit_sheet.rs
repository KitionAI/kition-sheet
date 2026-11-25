//! A size command given its sheet sets the rows or columns of that sheet, not of the active one
//! (task card C4/C5/C6-sheet: no command depends on the active sheet when it is told where to act).
//! `row.setHeight` and `column.setWidth` take `sheet` (a name or a position from 0) or a sheet in
//! their `rows` / `cols` (`Data!2:3`), as the commands that take a range do.

use kition_sheet_engine::Session;
use serde_json::{Value as Json, json};

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

/// `Sheet1` (active) and `Data`.
fn two_sheets() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    run(&mut s, "sheet.insert", json!({ "name": "Data", "before": 9 }));
    run(&mut s, "sheet.activate", json!({ "sheet": "Sheet1" }));
    s
}

fn row(s: &Session, sheet: usize, row: u32) -> Option<f32> {
    s.doc().unwrap().wb.sheet(sheet).unwrap().rows.get(&row).and_then(|info| info.size)
}

fn col(s: &Session, sheet: usize, col: u32) -> Option<f32> {
    s.doc().unwrap().wb.sheet(sheet).unwrap().cols.get(&col).and_then(|info| info.size)
}

#[test]
fn row_height_sets_the_rows_of_the_sheet_it_names() {
    let mut s = two_sheets();
    run(&mut s, "row.setHeight", json!({ "sheet": "Data", "rows": "2", "height": 30 }));
    assert_eq!((row(&s, 1, 1), row(&s, 0, 1)), (Some(30.0), None), "Data's row 2 is 30 pt; Sheet1's is untouched");
    run(&mut s, "row.setHeight", json!({ "rows": "Data!4:5", "height": 12 }));
    assert_eq!((row(&s, 1, 3), row(&s, 1, 4), row(&s, 0, 3)), (Some(12.0), Some(12.0), None));
    assert_eq!(s.doc().unwrap().wb.active_sheet, 0, "the active sheet stays");
}

#[test]
fn column_width_sets_the_columns_of_the_sheet_it_names() {
    let mut s = two_sheets();
    run(&mut s, "column.setWidth", json!({ "sheet": 1, "cols": "C", "width": 90 }));
    assert_eq!((col(&s, 1, 2), col(&s, 0, 2)), (Some(90.0), None));
    run(&mut s, "column.setWidth", json!({ "cols": "'Data'!E:F", "width": 40 }));
    assert_eq!((col(&s, 1, 4), col(&s, 1, 5), col(&s, 0, 4)), (Some(40.0), Some(40.0), None));
}

#[test]
fn a_sheet_the_workbook_lacks_is_refused() {
    let mut s = two_sheets();
    let error = s.execute("row.setHeight", json!({ "sheet": "Nowhere", "rows": "2", "height": 30 })).unwrap_err().to_string();
    assert!(error.contains("Nowhere"), "{error}");
    let error = s.execute("column.setWidth", json!({ "cols": "Nowhere!C", "width": 30 })).unwrap_err().to_string();
    assert!(error.contains("Nowhere"), "{error}");
}
