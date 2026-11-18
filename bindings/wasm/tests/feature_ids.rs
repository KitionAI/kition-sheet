//! Feature objects are discoverable by stable tool ids through rename and saved reread.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::{KitDocument, session::model::Exchange};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn load(doc: &mut SheetDocument, target: &str) -> Value {
    Exchange::new().batch(doc, &json!({"load":[{"target":target}]}), true).unwrap()["loaded"][0].clone()
}

#[test]
fn feature_identity_types_declare_their_read_only_contract() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let schema: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    for ty in ["Group", "Table", "Pivot", "Style"] {
        assert_eq!(schema["types"][ty]["readOnly"], true, "{ty} is an identity index whose owning list refuses direct writes");
    }
    let style = load(&mut doc, "workbook")["fields"]["styleObjects"][0].clone();
    let before = doc.save(None).unwrap();
    let error = Exchange::new().batch(&mut doc, &json!({"ops":[{"op":"delete","target":style}]}), false).unwrap_err();
    assert_eq!(error.code, kition_kit::session::model::ModelErrorCode::ReadOnly);
    assert_eq!(doc.save(None).unwrap(), before);
}

#[test]
fn feature_types_and_objects_have_persistent_tool_ids() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({"range":"A1:B3","values":[["Item","Amount"],["First",12],["Second",24]]})).unwrap();
    doc.execute("insert.table", json!({"range":"A1:B3","name":"Sales","header":true})).unwrap();
    doc.execute("insert.pivotTable", json!({"source":"A1:B3","destination":"D1","rows":["Item"],"values":["Amount"]})).unwrap();
    let mut members = Vec::new();
    for x in [10, 40] {
        members.push(doc.execute("drawing.insert", json!({"kind":"shape","frame":{"x":x,"y":20,"w":20,"h":20}})).unwrap()["id"].clone());
    }
    let group = doc.execute("drawing.group", json!({"ids":members})).unwrap()["id"].as_str().unwrap().to_owned();
    let schema: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    for ty in ["Group", "Table", "Pivot", "Style"] {
        assert!(schema["types"][ty]["toolId"].is_object(), "missing tool id: {ty}");
    }
    let owner = load(&mut doc, "sheet:1");
    let table = owner["fields"]["tableObjects"][0].as_str().expect("table identity index").to_owned();
    let pivot = owner["fields"]["pivotObjects"][0].as_str().expect("pivot identity index").to_owned();
    assert!(table.starts_with("table:1:"), "{owner}");
    assert!(pivot.starts_with("pivot:1:"), "{owner}");
    for (target, ty) in [(table, "Table"), (pivot, "Pivot"), (format!("drawing:{group}"), "Group")] {
        let before = load(&mut doc, &target);
        assert_eq!(before["type"], ty);
        if ty == "Table" {
            doc.execute("table.rename", json!({"table":target,"name":"Revenue"})).unwrap();
            assert_eq!(load(&mut doc, &target)["fields"]["name"], "Revenue");
        }
        doc.execute("sheet.rename", json!({"name":"Renamed"})).unwrap();
        let mut saved = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let after = load(&mut saved, &target);
        assert_eq!(before["id"], after["id"]);
        assert_eq!(after["type"], ty);
    }
    let styles = load(&mut doc, "workbook")["fields"]["styleObjects"].as_array().unwrap().clone();
    assert!(!styles.is_empty());
    let mut saved = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    for id in styles {
        let id = id.as_str().unwrap();
        assert!(id.starts_with("style:"));
        assert_eq!(load(&mut saved, id)["type"], "Style");
    }
}

#[test]
fn named_style_tool_ids_survive_redefinition_rename_and_reopen() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("table.newStyle", json!({"name":"First","elements":{"headerRow":{"bold":true}}})).unwrap();
    doc.execute("home.newCellStyle", json!({"name":"Entry","fromCell":"A1"})).unwrap();
    let named = |doc: &mut SheetDocument, name: &str| {
        load(doc, "workbook")["fields"]["styleObjects"]
            .as_array()
            .unwrap()
            .clone()
            .into_iter()
            .filter_map(|id| id.as_str().map(str::to_owned))
            .find(|id| load(doc, id)["fields"]["name"] == name)
            .unwrap_or_else(|| panic!("no identity for named style {name}"))
    };
    let table = named(&mut doc, "First");
    let cell = named(&mut doc, "Entry");
    doc.execute("table.newStyle", json!({"name":"Renamed","replace":table,"elements":{"headerRow":{"italic":true}}})).unwrap();
    doc.execute("home.bold", json!({"range":"A1","on":true})).unwrap();
    doc.execute("home.newCellStyle", json!({"name":"Entry","fromCell":"A1"})).unwrap();
    assert_eq!(named(&mut doc, "Renamed"), table);
    assert_eq!(named(&mut doc, "Entry"), cell);
    doc.execute("home.cellStyle", json!({"name":cell,"range":"B1"})).unwrap();
    let mut saved = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    assert_eq!(named(&mut saved, "Renamed"), table);
    assert_eq!(named(&mut saved, "Entry"), cell);
    assert_eq!(load(&mut saved, &cell)["fields"]["value"]["font"]["bold"], true);
}

#[test]
fn distinct_non_ascii_style_names_do_not_share_a_tool_id() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    for name in ["\u{c4}", "\u{e4}"] {
        doc.execute("home.newCellStyle", json!({"name":name})).unwrap();
    }
    let styles = load(&mut doc, "workbook")["fields"]["styleObjects"].as_array().unwrap().clone();
    let names: Vec<_> =
        styles.into_iter().filter_map(|id| id.as_str().map(str::to_owned)).map(|id| load(&mut doc, &id)["fields"]["name"].clone()).collect();
    for name in ["\u{c4}", "\u{e4}"] {
        assert!(names.contains(&json!(name)), "style identity missing for {name}: {names:?}");
    }
}
