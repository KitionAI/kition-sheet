//! C8-sheet-read: copied drawing objects keep their internal endpoint relationships.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::Session;
use serde_json::json;

#[test]
fn copied_sheet_connectors_reference_the_copied_shapes() {
    let mut session = Session::new();
    session.new_workbook();
    for x in [10, 180] {
        session.execute("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":x,"y":20,"w":80,"h":40}})).unwrap();
    }
    session.execute("drawing.connect", json!({"from":1,"to":2})).unwrap();
    session.execute("sheet.move", json!({"sheet":"Sheet1","to":1,"copy":true})).unwrap();
    let copied = session.doc().unwrap().wb.active().unwrap();
    let connection = copied.shapes.iter().find_map(|shape| shape.connection.as_ref()).unwrap();
    let endpoints: Vec<_> = copied.shapes.iter().filter(|shape| shape.connection.is_none()).map(|shape| shape.id).collect();
    assert_eq!(endpoints.len(), 2);
    assert_eq!(connection.start.unwrap().id, endpoints[0], "the copied start must follow the copied first shape");
    assert_eq!(connection.end.unwrap().id, endpoints[1], "the copied end must follow the copied second shape");
    let connector = copied.shapes.iter().find(|shape| shape.connection.is_some()).unwrap().id;
    let before = kition_sheet_engine::cmd::drawing::to_drawing(copied, copied.shapes.iter().find(|shape| shape.id == connector).unwrap()).frame;
    session.execute("drawing.transform", json!({"id":endpoints[0],"dx":30})).unwrap();
    let copied = session.doc().unwrap().wb.active().unwrap();
    let after = kition_sheet_engine::cmd::drawing::to_drawing(copied, copied.shapes.iter().find(|shape| shape.id == connector).unwrap()).frame;
    assert_ne!(after, before, "the copied connector must reroute when its copied endpoint moves");
    let source = &session.doc().unwrap().wb.sheets[0];
    assert_eq!(source.shapes.iter().find_map(|shape| shape.connection.as_ref()).unwrap().start.unwrap().id, 1);
}
