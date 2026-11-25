//! `range.fill {range, input}` writes the input as for the range's first
//! cell (relative references move from there, as Excel's Ctrl+Enter moves them from the active
//! cell of the selection it fills), and `row.setHeight` / `column.setWidth` take a single row
//! or column (`"9"`, `"C"`) and lists (`"2:3,5"`).

use kition_sheet_core::{CellRef, Value};
use kition_sheet_engine::Session;
use serde_json::json;

fn workbook() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn v(s: &Session, a: &str) -> Value {
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse(a).unwrap())
}

fn formula(s: &Session, a: &str) -> String {
    s.doc().unwrap().wb.active().unwrap().cell(CellRef::parse(a).unwrap()).unwrap().formula.as_ref().unwrap().text.clone()
}

#[test]
fn fill_with_a_range_moves_references_from_its_first_cell() {
    let mut s = workbook();
    s.execute("cell.set", json!({"cell": "A10", "input": "1"})).unwrap();
    // The active cell is A1, outside the range.
    s.execute("range.fill", json!({"range": "A11:A69", "input": "=A10+1"})).unwrap();
    assert_eq!(formula(&s, "A11"), "A10+1");
    assert_eq!(formula(&s, "A69"), "A68+1");
    assert_eq!(v(&s, "A69"), Value::Number(60.0));
}

#[test]
fn fill_of_the_selection_moves_references_from_the_active_cell() {
    let mut s = workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1], [2], [3]]})).unwrap();
    s.execute("select.set", json!({"range": "B1:B3", "active": "B2"})).unwrap();
    s.execute("range.fill", json!({"input": "=A2*10"})).unwrap();
    assert_eq!([v(&s, "B1"), v(&s, "B2"), v(&s, "B3")], [Value::Number(10.0), Value::Number(20.0), Value::Number(30.0)]);
}

fn row_height(s: &Session, row: u32) -> Option<f32> {
    s.doc().unwrap().wb.active().unwrap().rows.get(&(row - 1)).and_then(|r| r.size)
}

fn col_width(s: &Session, col: u32) -> Option<f32> {
    s.doc().unwrap().wb.active().unwrap().cols.get(&col).and_then(|c| c.size)
}

#[test]
fn row_height_takes_a_single_row_and_lists() {
    let mut s = workbook();
    s.execute("row.setHeight", json!({"rows": "9", "height": 30})).unwrap();
    assert_eq!(row_height(&s, 9), Some(30.0));
    assert_eq!(row_height(&s, 8), None);
    s.execute("row.setHeight", json!({"rows": "2:3,5", "height": 12})).unwrap();
    assert_eq!([row_height(&s, 2), row_height(&s, 3), row_height(&s, 4), row_height(&s, 5)], [Some(12.0), Some(12.0), None, Some(12.0)]);
    s.execute("row.setHeight", json!({"rows": 7, "height": 40})).unwrap();
    assert_eq!(row_height(&s, 7), Some(40.0));
    assert!(s.execute("row.setHeight", json!({"rows": "abc", "height": 20})).is_err());
    assert!(s.execute("column.setWidth", json!({"cols": "12", "width": 20})).is_err());
}

#[test]
fn column_width_takes_a_single_column() {
    let mut s = workbook();
    s.execute("column.setWidth", json!({"cols": "C", "width": 100})).unwrap();
    assert_eq!(col_width(&s, 2), Some(100.0));
    assert_eq!(col_width(&s, 3), None);
}
