//! Renaming a sheet must not manufacture edits to all of its cell descriptions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::session::model::ReplicaPort;
use kition_kit::{KitDocument, session::model::Exchange};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn cell(doc: &mut SheetDocument) -> Value {
    Exchange::new().batch(doc, &json!({"load":[{"target":"cell:1!B7"}]}), true).unwrap()["loaded"][0].clone()
}

#[test]
fn sheet_rename_has_no_cell_label_effects() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
    let before = cell(&mut doc);
    let receipt = Exchange::new().batch(&mut doc, &json!({"ops":[{"op":"set","target":"sheet:1","field":"name","value":"Renamed"}]}), false).unwrap();
    assert_eq!(receipt["results"].as_array().unwrap().len(), 1, "{receipt}");
    assert_eq!(cell(&mut doc), before);
    let bytes = doc.save(None).unwrap();
    let mut reopened = SheetDocument::open("Book.xlsx", &bytes).unwrap();
    assert_eq!(cell(&mut reopened), before);
}

#[test]
fn cells_keep_their_owning_map_and_images_opt_out_of_sampling() {
    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let schema = doc.model_schema();
    let sheet = &schema.types["Sheet"];
    assert!(sheet.fields["cells"].owns);
    assert_eq!(sheet.fields["cells"].of.as_deref(), Some("Cell"));
    assert_eq!(sheet.fields["images"].sample, Some(false));
    assert_eq!(schema.types["Cell"].fields["text"].label.as_deref(), Some("cell text"));
}

#[test]
fn typed_owning_maps_accept_cells_and_reject_other_object_types() {
    let text = include_str!("../../../crates/kdm/kdm-model/model/schema.json");
    let mut schema: Value = serde_json::from_str(text).unwrap();
    schema["types"]["Sheet"]["fields"]["cells"]["of"] = json!("Cell");
    let parsed = kition_kit::kdm::Schema::parse(&schema.to_string()).unwrap();
    assert_eq!(parsed.types["Sheet"].fields["cells"].of.as_deref(), Some("Cell"));
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
    let before = cell(&mut doc);
    let bad =
        Exchange::new().batch(&mut doc, &json!({"ops":[{"op":"map.set","target":"sheet:1","field":"cells","key":"B7","value":"sheet:1"}]}), false);
    assert!(bad.is_err(), "an owning cell entry cannot hold a Sheet");
    assert_eq!(cell(&mut doc), before);
}

#[test]
fn cell_owning_map_replies_translate_raw_ids() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
    let answer = Exchange::new().batch(&mut doc, &json!({"load":[{"target":"sheet:1","fields":["cells"]}]}), true).unwrap();
    let cells = answer["loaded"][0]["fields"]["cells"].as_object().unwrap();
    assert_eq!(cells.len(), 1);
    assert_eq!(cells.values().next().unwrap(), "cell:1!B7");
    assert_eq!(cell(&mut doc)["id"], "cell:1!B7");
}
