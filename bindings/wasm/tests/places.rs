//! C8-land-13-sheet: receipt paths, rather than engine member names, are premises.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::{KitDocument, Premise};
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[test]
fn cells_are_owned_and_drawing_frames_are_positions() {
    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    use kition_kit::session::model::ReplicaPort;
    let schema = doc.model_schema();
    let sheet = schema.types.get("Sheet").unwrap();
    assert!(sheet.fields.get("cells").unwrap().owns);
    assert_eq!(sheet.fields.get("cells").unwrap().kind, kition_kit::kdm::FieldKind::Map);
    assert!(schema.types.contains_key("Cell"));
    for ty in ["Shape", "Chart"] {
        assert!(schema.types.get(ty).unwrap().fields.get("frame").unwrap().position);
    }
    for field in ["shapes", "charts"] {
        let field = sheet.fields.get(field).expect("drawings must be reachable through an owning sheet map");
        assert!(field.owns);
        assert_eq!(field.kind, kition_kit::kdm::FieldKind::Map);
    }
}

#[test]
fn b7_holds_its_entry_and_both_positional_axes() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
    let under = doc.under("cell.set", &json!({"cell":"B7","input":"42"})).unwrap().unwrap();
    assert!(under.objects.contains(&json!("sheet:1")), "{under:?}");
    assert!(under.premises.iter().any(|p| p.id == "cell:1!B7" && p.fields.as_ref().unwrap().contains(&"input".into())), "{under:?}");
    assert!(under.premises.iter().filter(|p| p.id == "sheet:1").all(|p| !p.fields.as_ref().unwrap().iter().any(|f| f.starts_with("cells"))));
    for (field, from, to) in [("rows", 6, 7), ("cols", 1, 2)] {
        assert!(under.within.iter().any(|w| w.field == field && w.from == from && w.to == to), "{under:?}");
    }
}

#[test]
fn drawing_text_holds_text_not_frame_and_transform_holds_frame() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("drawing.insert", json!({"kind":"textBox","text":"before"})).unwrap();
    let under = doc.under("drawing.text", &json!({"id":"drawing:1","text":"after"})).unwrap().unwrap();
    assert!(
        under.premises.contains(&Premise::on("drawing:1").fields(["text", "look.text_body", "look.kind"]).positions(Vec::<String>::new())),
        "{under:?}"
    );
    let under = doc.under("drawing.transform", &json!({"ids":[1],"dx":1})).unwrap().unwrap();
    assert!(under.premises.iter().any(|p| p.id == "drawing:1" && p.positions.as_ref().unwrap().contains(&"frame".to_owned())), "{under:?}");
}

#[test]
fn eraser_names_only_the_strokes_under_its_region() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("drawing.ink", json!({"strokes":[{"points":[[10,100],[90,100]],"width":2},{"points":[[200,100],[290,100]],"width":2}]})).unwrap();
    let params = json!({"x":50,"y":100});
    let under = doc.under("drawing.erase", &params).unwrap().unwrap();
    assert_eq!(under.objects, json!(["drawing:1"]).as_array().unwrap().clone());
    assert!(under.covers.is_some(), "point lookup needs historical coverage");
    let values = doc.derived(&[json!("sheet:1")], &["ink".into()]).unwrap();
    assert_eq!(values[0].values["ink"].as_array().unwrap().len(), 2);
}

#[test]
fn formula_entry_holds_the_position_of_the_reference_it_will_store() {
    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let under = doc.under("cell.set", &json!({"cell":"A1","input":"=B7"})).unwrap().unwrap();
    assert!(under.within.iter().any(|w| w.field == "rows" && w.from == 6 && w.to == 7), "{under:?}");
    assert!(under.within.iter().any(|w| w.field == "cols" && w.from == 1 && w.to == 2), "{under:?}");
}
