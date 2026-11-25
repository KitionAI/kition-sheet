//! C8-sheet-read: imported maximum object IDs fail explicitly before any write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::{
    EngineError, Session,
    model::{PivotTable, Shape},
};
use serde_json::{Value, json};

fn fixture() -> Session {
    let mut session = Session::new();
    session.new_workbook();
    session.execute("range.setValues", json!({"range":"A1","values":[["Item","Amount"],["One",1],["Two",2]]})).unwrap();
    for x in [10, 120] {
        session.execute("drawing.insert", json!({"kind":"shape","frame":{"x":x,"y":10,"w":50,"h":30},"geometry":{"preset":"rect"}})).unwrap();
    }
    session
}

fn add_imported_shape(session: &mut Session, id: u32) {
    std::sync::Arc::make_mut(&mut session.doc_mut().unwrap().wb).sheet_mut(0).unwrap().shapes.push(Shape { id, ..Default::default() });
}

fn assert_exhausted(session: &mut Session, command: &str, params: Value) {
    let before = serde_json::to_value(&*session.doc().unwrap().wb).unwrap();
    let result = session.execute(command, params.clone());
    assert!(
        matches!(&result, Err(EngineError::Other(message)) if message.contains("object ID space is exhausted")),
        "{command} {params} must return an explicit allocation error, got {result:?}"
    );
    assert_eq!(serde_json::to_value(&*session.doc().unwrap().wb).unwrap(), before, "{command} must leave the workbook unchanged");
}

#[test]
fn every_object_allocator_rejects_an_imported_maximum_id_without_a_caught_panic() {
    for (command, params) in [
        ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"}})),
        ("drawing.connect", json!({"from":"1","to":"2"})),
        ("drawing.group", json!({"ids":["1","2"]})),
        ("shape.insert", json!({"kind":"rectangle"})),
        ("textBox.insert", json!({"text":"Hello"})),
        ("artText.insert", json!({"text":"Hello"})),
        ("icon.insert", json!({"name":"check"})),
        ("ink.stroke", json!({"points":[[10,10],[20,20]]})),
        ("drawing.ink", json!({"strokes":[{"points":[[10,10],[20,20]]}]})),
        ("chart.insert", json!({"range":"A1:B3","type":"column"})),
        ("table.insert", json!({"range":"A1:B3","header":true,"name":"Data"})),
        (
            "image.insert",
            json!({"base64":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==","mime":"image/png"}),
        ),
        ("pivot.insert", json!({"source":"A1:B3","destination":"D5","rows":["Item"],"values":["Amount"]})),
        ("sheet.move", json!({"sheet":"Sheet1","to":1,"copy":true})),
    ] {
        let mut session = fixture();
        add_imported_shape(&mut session, u32::MAX);
        assert_exhausted(&mut session, command, params);
    }
}

#[test]
fn sheet_copy_rejects_exhaustion_after_the_last_available_id_without_duplicate_ids() {
    let mut session = fixture();
    add_imported_shape(&mut session, u32::MAX - 1);
    assert_exhausted(&mut session, "sheet.move", json!({"sheet":"Sheet1","to":1,"copy":true}));
}

#[test]
fn the_last_object_id_is_usable_once_then_allocation_refuses() {
    let mut session = fixture();
    add_imported_shape(&mut session, u32::MAX - 1);
    let result = session.execute("shape.insert", json!({"kind":"rectangle"})).unwrap();
    assert_eq!(result["shape"], json!(u32::MAX));
    assert_exhausted(&mut session, "shape.insert", json!({"kind":"rectangle"}));
}

#[test]
fn pivot_ids_do_not_saturate_and_reuse_an_imported_maximum_pivot_id() {
    let mut session = fixture();
    std::sync::Arc::make_mut(&mut session.doc_mut().unwrap().wb).sheet_mut(0).unwrap().pivots.push(PivotTable {
        id: u32::MAX,
        name: "Imported".into(),
        source: "Sheet1!A1:B3".into(),
        ..Default::default()
    });
    assert_exhausted(&mut session, "pivot.insert", json!({"source":"A1:B3","destination":"D5","rows":["Item"],"values":["Amount"]}));
}
