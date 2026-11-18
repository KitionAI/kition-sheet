//! A command refuses a parameter its schema does not name, saying which, instead of running
//! without it (engine rule E7a, kit requirement AX15).
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

fn book() -> SheetDocument {
    SheetDocument::create("Book.xlsx", 64, 36).unwrap()
}

#[test]
fn every_command_refuses_a_parameter_it_does_not_take() {
    if let Err(problems) = kition_kit::testing::check_unknown_params(book, &[]) {
        panic!("{} commands:\n{}", problems.len(), problems.join("\n"));
    }
}

#[test]
fn a_misspelled_parameter_names_the_one_it_resembles_and_changes_nothing() {
    let mut doc = book();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "1" })).unwrap();
    let before = doc.save(None).unwrap();
    let error = doc.execute("home.bold", json!({ "rnage": "A1" })).unwrap_err();
    assert!(error.contains("`rnage`") && error.contains("did you mean `range`"), "{error}");
    assert_eq!(doc.save(None).unwrap(), before);
    // The target helpers' other spellings stay valid calls.
    doc.execute("home.bold", json!({ "cell": "A1", "sheet": 0 })).unwrap();
}
