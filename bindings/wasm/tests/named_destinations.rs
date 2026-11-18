//! Cell entry through a static defined name uses that name's destination.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[test]
fn cell_entry_through_a_defined_name_reaches_its_cell_and_survives_save() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("sheet.rename", json!({"name":"Data"})).unwrap();
    doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
    doc.execute("formulas.defineName", json!({"name":"Destination","refersTo":"=Data!$B$7"})).unwrap();
    doc.execute("cell.set", json!({"name":"Destination","input":"42"})).unwrap();
    let loaded = Exchange::new().batch(&mut doc, &json!({"load":[{"target":"workbook","fields":["names"]}]}), true).unwrap();
    let id = loaded["loaded"][0]["fields"]["names"][0].as_str().unwrap();
    assert!(id.starts_with("name:"), "{loaded}");
    doc.execute("cell.set", json!({"name":id,"input":"43"})).unwrap();
    doc.execute("cell.set", json!({"name":"Destination","input":"42"})).unwrap();
    assert_eq!(doc.execute("cell.get", json!({"cell":"B7"})).unwrap()["value"], 42.0);
    assert_eq!(doc.execute("cell.get", json!({"cell":"A1"})).unwrap()["value"], serde_json::Value::Null);
    let bytes = doc.save(None).unwrap();
    let mut saved = SheetDocument::open("Book.xlsx", &bytes).unwrap();
    assert_eq!(saved.execute("cell.get", json!({"cell":"B7"})).unwrap()["value"], 42.0);
}

#[test]
fn named_entry_resolves_aliases_and_rejects_ambiguous_or_dynamic_destinations() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("formulas.defineName", json!({"name":"Destination","refersTo":"=$B$7"})).unwrap();
    doc.execute("formulas.defineName", json!({"name":"Alias","refersTo":"=Destination"})).unwrap();
    doc.execute("cell.set", json!({"name":"Alias","input":"42"})).unwrap();
    assert_eq!(doc.execute("cell.get", json!({"cell":"B7"})).unwrap()["value"], 42.0);
    for formula in ["=Destination", "=B7:C8", "=OFFSET(B7,1,0)"] {
        doc.execute("formulas.defineName", json!({"name":"Destination","refersTo":formula})).unwrap();
        assert!(doc.execute("cell.set", json!({"name":"Destination","input":"99"})).is_err(), "{formula}");
        assert_eq!(doc.execute("cell.get", json!({"cell":"B7"})).unwrap()["value"], 42.0);
    }
    assert!(doc.execute("cell.set", json!({"name":"Missing","input":"99"})).is_err());
    assert!(doc.execute("cell.set", json!({"name":"Alias","cell":"A1","input":"99"})).is_err());
}
