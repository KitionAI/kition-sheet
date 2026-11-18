//! C8-sheet-read-2: the selection traversal budget is shared by all ranges.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::Session;
use serde_json::json;

fn fixture() -> Session {
    let mut session = Session::new();
    session.new_workbook();
    for cell in ["A1", "C1", "C2"] {
        session.execute("insert.checkbox", json!({"range":cell})).unwrap();
    }
    session
}

#[test]
fn checkbox_selection_caps_the_total_before_filtering_or_collecting() {
    let mut session = fixture();
    session.execute("selection.set", json!({"range":"A1:A100000,C1:C100000"})).unwrap();
    let output = session.execute("cell.toggleCheckbox", json!({})).unwrap();
    assert_eq!(output["toggled"], 1, "empty coordinates consume the traversal budget too");
    assert_eq!(session.execute("cell.get", json!({"cell":"A1"})).unwrap()["value"], true);
    assert_eq!(session.execute("cell.get", json!({"cell":"C1"})).unwrap()["value"], false);
}

#[test]
fn checkbox_selection_uses_the_remaining_budget_in_the_next_range() {
    let mut session = fixture();
    session.execute("selection.set", json!({"range":"A1:A99999,C1:C2"})).unwrap();
    let output = session.execute("cell.toggleCheckbox", json!({})).unwrap();
    assert_eq!(output["toggled"], 2);
    assert_eq!(session.execute("cell.get", json!({"cell":"A1"})).unwrap()["value"], true);
    assert_eq!(session.execute("cell.get", json!({"cell":"C1"})).unwrap()["value"], true);
    assert_eq!(session.execute("cell.get", json!({"cell":"C2"})).unwrap()["value"], false);
}

#[test]
fn an_explicit_checkbox_cell_bypasses_the_selection_budget() {
    let mut session = fixture();
    session.execute("selection.set", json!({"range":"A1:A100000,C1:C100000"})).unwrap();
    assert_eq!(session.execute("cell.toggleCheckbox", json!({"cell":"C2"})).unwrap()["toggled"], 1);
    assert_eq!(session.execute("cell.get", json!({"cell":"C2"})).unwrap()["value"], true);
    assert_eq!(session.execute("cell.get", json!({"cell":"A1"})).unwrap()["value"], false);
}
