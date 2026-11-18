//! Spelling history uses stable row projections, including numeric source text.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "independent_common/mod.rs"]
mod common;
use common::Fixture;
use kition_kit::KitDocument;
use serde_json::json;

#[tokio::test]
async fn numeric_spelling_still_holds_old_and_new_matching_inputs() {
    for change in ["matching", "new matching", "deleted matching", "unmatched"] {
        let fixture = Fixture::new();
        fixture.colleague(&[("cell.set", json!({"cell":"C3","input":"12"}))]);
        let basis = fixture.basis().await;
        let (cell, input) = match change {
            "matching" => ("C3", "24"),
            "new matching" => ("D6", "12"),
            _ => ("D6", "24"),
        };
        if change == "deleted matching" {
            fixture.colleague(&[("edit.clearAll", json!({"range":"C3"}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":cell,"input":input}))]);
        }
        let before = fixture.bytes();
        let result = fixture.run(basis, "review.changeSpelling", json!({"word":"12","to":"13","all":true})).await;
        if change == "unmatched" {
            assert_eq!(result.as_ref().ok().map(|value| &value["saved"]), Some(&json!(true)), "{change}: {result:?}");
            let mut saved = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
            assert_eq!(saved.execute("cell.get", json!({"cell":"C3"})).unwrap()["value"], 13.0);
            assert_eq!(saved.execute("cell.get", json!({"cell":"D6"})).unwrap()["value"], 24.0);
        } else {
            assert_eq!(result.as_ref().err().map(|value| &value["error"]["code"]), Some(&json!("conflict")), "{change}: {result:?}");
            assert_eq!(fixture.bytes(), before);
        }
        fixture.shutdown().await;
    }
}

#[test]
fn spelling_text_vectors_live_on_stable_rows_while_numbers_keep_their_input_register() {
    let mut doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({"range":"C3","values":[[12,"teh"]]})).unwrap();
    doc.snapshot().unwrap();
    let projected = doc.derived(&[], &["spellingInputs".into(), "spellingHeader".into()]).unwrap();
    assert!(projected.iter().all(|entry| entry.id.as_str().is_some_and(|id| id.starts_with("row:"))), "spelling must emit no Cell derived values");
    let row = projected.iter().find(|entry| entry.values.get("spellingInputs") == Some(&json!([[3, "teh"]]))).unwrap();
    let identity = row.id.clone();
    assert!(row.values["spellingHeader"].as_str().is_some_and(|digest| digest.len() == 64));
    assert_eq!(doc.execute("cell.get", json!({"cell":"C3"})).unwrap()["value"], 12.0, "numeric input remains in its existing cell register");

    doc.execute("home.insertRows", json!({"rows":"1:1"})).unwrap();
    doc.snapshot().unwrap();
    let projected = doc.derived(std::slice::from_ref(&identity), &["spellingInputs".into()]).unwrap();
    let moved = projected.iter().find(|entry| entry.id == identity).unwrap();
    assert_eq!(moved.values["spellingInputs"], json!([[3, "teh"]]), "a row's text vector follows its stable row identity");
}

#[tokio::test]
async fn same_row_unmatched_input_changes_preserve_numeric_and_text_spelling_edits() {
    for (word, replacement) in [("teh", "the"), ("12", "13")] {
        let fixture = Fixture::new();
        fixture.colleague(&[("cell.set", json!({"cell":"C3","input":word})), ("cell.set", json!({"cell":"D3","input":"ordinary"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[("cell.set", json!({"cell":"D3","input":"colleague"}))]);
        let result = fixture.run(basis, "review.changeSpelling", json!({"word":word,"to":replacement,"all":true})).await;
        assert_eq!(
            result.as_ref().ok().map(|value| &value["saved"]),
            Some(&json!(true)),
            "an unmatched input in the source row must apply: {result:?}"
        );
        let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
        let sheet = saved.sheet(0).unwrap();
        assert_eq!(sheet.value(kition_sheet_engine::core::CellRef::parse("C3").unwrap()).display(), replacement);
        assert_eq!(sheet.value(kition_sheet_engine::core::CellRef::parse("D3").unwrap()).display(), "colleague");
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn deleting_a_matching_source_row_or_column_refuses_spelling_without_cell_derived_fields() {
    for (command, params) in [("home.deleteRows", json!({"rows":"3:3"})), ("home.deleteColumns", json!({"cols":"C:C"}))] {
        let fixture = Fixture::new();
        fixture.colleague(&[("cell.set", json!({"cell":"C3","input":"teh"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(command, params)]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "review.changeSpelling", json!({"word":"teh","to":"the","all":true})).await;
        assert_eq!(
            result.as_ref().err().map(|value| &value["error"]["code"]),
            Some(&json!("conflict")),
            "deleting the source must conflict: {result:?}"
        );
        assert_eq!(fixture.bytes(), before, "refusal preserves the outside deletion");
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn spelling_history_combines_numeric_registers_with_text_row_snapshots() {
    for (initial, changed, word, replacement, related) in [
        ("12", "ordinary", "12", "13", true),
        ("ordinary", "12", "12", "13", true),
        ("teh", "24", "teh", "the", true),
        ("24", "teh", "teh", "the", true),
        ("24", "ordinary", "teh", "the", false),
        ("ordinary", "24", "teh", "the", false),
    ] {
        let fixture = Fixture::new();
        fixture.colleague(&[("cell.set", json!({"cell":"C3","input":initial})), ("cell.set", json!({"cell":"D6","input":"teh"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[("cell.set", json!({"cell":"C3","input":changed}))]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "review.changeSpelling", json!({"word":word,"to":replacement,"all":true})).await;
        if related {
            assert_eq!(
                result.as_ref().err().map(|value| &value["error"]["code"]),
                Some(&json!("conflict")),
                "{initial} to {changed} changes spelling membership for {word}: {result:?}"
            );
            assert_eq!(fixture.bytes(), before);
        } else {
            assert_eq!(result.as_ref().ok().map(|value| &value["saved"]), Some(&json!(true)), "{initial} to {changed} is unmatched: {result:?}");
            let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            let sheet = saved.sheet(0).unwrap();
            assert_eq!(sheet.value(kition_sheet_engine::core::CellRef::parse("C3").unwrap()).display(), changed);
            assert_eq!(sheet.value(kition_sheet_engine::core::CellRef::parse("D6").unwrap()).display(), "the");
        }
        fixture.shutdown().await;
    }
}
