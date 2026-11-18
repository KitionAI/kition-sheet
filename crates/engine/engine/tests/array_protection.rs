//! C8-sheet-read: protection must be checked on every array-entry destination.

use kition_sheet_engine::Session;
use serde_json::json;

#[test]
fn array_entry_checks_the_selection_instead_of_an_unrelated_cell_parameter() {
    for locked in ["A1", "A2"] {
        let mut session = Session::new();
        session.new_workbook();
        session.execute("range.setValues", json!({"range":"A1","values":[[11],[22]]})).unwrap();
        session.execute("home.lockCell", json!({"range":"A1:B2","on":false})).unwrap();
        session.execute("home.lockCell", json!({"range":locked,"on":true})).unwrap();
        session.execute("review.protectSheet", json!({})).unwrap();
        session.execute("selection.set", json!({"range":"A1:A2"})).unwrap();
        let result = session.execute("cell.set", json!({"cell":"B2","input":"=7","array":true}));
        assert!(result.is_err(), "locked destination {locked} accepted array entry: {result:?}");
        assert_eq!(session.execute("cell.get", json!({"cell":"A1"})).unwrap()["value"], json!(11.0));
        assert_eq!(session.execute("cell.get", json!({"cell":"A2"})).unwrap()["value"], json!(22.0));
    }
}

#[test]
fn array_entry_allows_an_unlocked_selection_when_the_named_cell_is_locked() {
    let mut session = Session::new();
    session.new_workbook();
    session.execute("home.lockCell", json!({"range":"A1:A2","on":false})).unwrap();
    session.execute("review.protectSheet", json!({})).unwrap();
    session.execute("selection.set", json!({"range":"A1:A2"})).unwrap();
    session.execute("cell.set", json!({"cell":"B2","input":"=7","array":true})).unwrap();
    assert_eq!(session.execute("cell.get", json!({"cell":"A1"})).unwrap()["value"], json!(7.0));
}
