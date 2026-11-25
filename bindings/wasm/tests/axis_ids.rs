//! C8-sheet-2: blank axes retain identity across an independent saved-file read.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::{KitDocument, session::model::Exchange};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn axes(doc: &mut SheetDocument, field: &str) -> Vec<Value> {
    Exchange::new().batch(doc, &json!({"load":[{"target":"sheet:1","fields":[field]}]}), true).unwrap()["loaded"][0]["fields"][field]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn inserted_blank_rows_and_columns_keep_distinct_saved_identities() {
    for (field, command, params) in [("rows", "row.insert", json!({"rows":"2:2"})), ("cols", "column.insert", json!({"cols":"B:B"}))] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("cell.set", json!({"cell":"D12","input":"end"})).unwrap();
        let before = axes(&mut doc, field);
        doc.execute(command, params).unwrap();
        let mut saved = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let after = axes(&mut saved, field);
        assert_eq!(after.len(), before.len() + 1);
        assert_eq!(before[1], after[2], "the old blank axis moved; a new blank cannot replace it");
        assert_ne!(before[1], after[1]);
        assert!(after.iter().all(|id| id.as_str().unwrap().starts_with(if field == "rows" { "row:" } else { "col:" })));
        assert_eq!(after.iter().map(Value::to_string).collect::<std::collections::BTreeSet<_>>().len(), after.len());
    }
}
