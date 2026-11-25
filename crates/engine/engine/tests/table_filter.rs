//! A table's header buttons filter its rows, as the sheet's AutoFilter does (module M10, CLN-X7
//! to CLN-X10; M7's TS-C102): the column's values for the list, values and conditions, clearing,
//! sorting with a filter on, and the criteria kept in the .xlsx.

use kition_sheet_engine::Session;
use serde_json::{Value as Json, json};

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

/// A table on A1:B6: 名称 (苹果, 香蕉, 苹果, 橙子, empty) and 数量 (3, 8, 5, 1, 9).
fn fruit() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows = [["名称", "数量"], ["苹果", "3"], ["香蕉", "8"], ["苹果", "5"], ["橙子", "1"], ["", "9"]];
    for (r, row) in rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            if !value.is_empty() {
                let cell = format!("{}{}", ["A", "B"][c], r + 1);
                run(&mut s, "cell.set", json!({"cell": cell, "input": value}));
            }
        }
    }
    run(&mut s, "table.insert", json!({"range": "A1:B6", "header": true}));
    s
}

fn hidden(s: &Session) -> Vec<u32> {
    let sh = s.doc().unwrap().wb.active().unwrap();
    (1..=6).filter(|r| sh.is_row_hidden(r - 1)).collect()
}

#[test]
fn the_header_lists_the_column_values_with_counts() {
    let mut s = fruit();
    let answer = run(&mut s, "data.filterValues", json!({"cell": "A1"}));
    assert_eq!(answer["range"], json!("A1:B6"));
    assert!(answer["table"].is_string());
    assert_eq!(answer["kind"], json!("text"));
    let values: Vec<(String, u64)> =
        answer["values"].as_array().unwrap().iter().map(|v| (v["text"].as_str().unwrap().to_owned(), v["count"].as_u64().unwrap())).collect();
    assert_eq!(values, vec![("橙子".to_owned(), 1), ("苹果".to_owned(), 2), ("香蕉".to_owned(), 1)]);
    assert_eq!(answer["blanks"]["count"], json!(1));
    let numbers = run(&mut s, "data.filterValues", json!({"cell": "B1"}));
    assert_eq!(numbers["kind"], json!("number"));
    assert_eq!(numbers["values"][0]["text"], json!("1"), "numbers in numeric order");
}

#[test]
fn a_table_filters_by_values_and_conditions_and_clears() {
    let mut s = fruit();
    run(&mut s, "data.filterBy", json!({"cell": "A1", "values": ["苹果"]}));
    assert_eq!(hidden(&s), [3, 5, 6], "only the apples show");
    let table = &s.doc().unwrap().wb.active().unwrap().tables[0];
    assert_eq!(table.criteria.len(), 1, "the criterion is the table's own");
    assert!(s.doc().unwrap().wb.active().unwrap().autofilter.is_none(), "no sheet filter appears");
    let answer = run(&mut s, "data.filterValues", json!({"cell": "A1"}));
    let ticked: Vec<&str> =
        answer["values"].as_array().unwrap().iter().filter(|v| v["checked"] == json!(true)).map(|v| v["text"].as_str().unwrap()).collect();
    assert_eq!(ticked, ["苹果"]);
    // A number condition on the second column adds to the first.
    run(&mut s, "data.filterBy", json!({"cell": "B1", "custom": {"op": ">", "value": "4"}}));
    assert_eq!(hidden(&s), [2, 3, 5, 6], "apples above 4");
    // The first column's condition cleared: every row above 4.
    run(&mut s, "data.filterBy", json!({"cell": "A1", "clear": true}));
    assert_eq!(hidden(&s), [2, 5]);
    run(&mut s, "data.clearFilter", json!({"cell": "B1"}));
    assert!(hidden(&s).is_empty());
    // One step each: undo brings the last condition back.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(hidden(&s), [2, 5]);
}

#[test]
fn a_sort_keeps_the_filter_on_the_rows_it_lets_through() {
    let mut s = fruit();
    run(&mut s, "data.filterBy", json!({"cell": "B1", "custom": {"op": ">=", "value": "5"}}));
    assert_eq!(hidden(&s), [2, 5]);
    run(&mut s, "data.sort", json!({"range": "A1:B6", "header": true, "keys": [{"column": "B", "order": "asc"}]}));
    // Sorted 1 3 5 8 9: the rows holding 5, 8 and 9 show.
    assert_eq!(hidden(&s), [2, 3]);
}

#[test]
fn table_criteria_are_saved_in_the_xlsx() {
    let mut s = fruit();
    run(&mut s, "data.filterBy", json!({"cell": "A1", "values": ["香蕉", "橙子"]}));
    let bytes = kition_sheet_engine::io::save_bytes(&s.doc().unwrap().wb, "Fruit.xlsx").unwrap();
    let (wb, _) = kition_sheet_engine::io::open_bytes("Fruit.xlsx", &bytes).unwrap();
    let table = &wb.sheet(0).unwrap().tables[0];
    assert_eq!(table.criteria.len(), 1, "{:?}", table.criteria);
    assert!(table.filter_button);
}
