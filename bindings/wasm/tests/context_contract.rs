//! C8-land-13-sheet: the complete binding editor-state list, across both selection kinds.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::{CallContext, KitDocument};
use kition_sheet_kit::{EDITOR_STATE, SheetDocument};
use serde_json::json;

#[test]
fn the_full_editor_state_list_passes_the_sdk_contract_for_cells_drawings_and_sheets() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("drawing.insert", json!({"kind":"shape"})).unwrap();
    doc.execute("home.insertSheet", json!({"name":"Other", "before":9})).unwrap();
    doc.execute("sheet.activate", json!({"sheet":0})).unwrap();
    doc.execute("selection.set", json!({"range":"G9:H10"})).unwrap();
    let kept = doc.editor_state();
    for (part, on) in [
        (json!({"sheet":"sheet:1"}), vec![json!("A1:B2")]),
        (json!({"sheet":"sheet:1"}), vec![json!("drawing:1")]),
        (json!({"sheet":"Other"}), vec![json!("C3:D4")]),
    ] {
        let context = CallContext { part: Some(part), on, time: None };
        kition_kit::testing::check_context(&mut doc, &context, EDITOR_STATE).unwrap();
        assert_eq!(doc.editor_state(), kept);
    }
}
