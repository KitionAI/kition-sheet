//! C8-sheet-read: ink creation and conversion respect sheet drawing protection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::Session;
use serde_json::{Value, json};

fn calls() -> [(&'static str, Value); 3] {
    [
        ("draw.stroke", json!({"points":[[10,10],[80,40]],"color":"#123456","width":2,"tool":"pen"})),
        ("drawing.ink", json!({"strokes":[{"points":[[10,10],[80,40]],"color":"#123456","width":2,"tool":"pen"}]})),
        ("draw.inkToShape", json!({"id":1})),
    ]
}

fn with_stroke() -> Session {
    let mut session = Session::new();
    session.new_workbook();
    session.execute("draw.stroke", json!({"points":[[100,100],[140,120]],"color":"#654321","width":2,"tool":"pen"})).unwrap();
    session
}

#[test]
fn ink_writes_refuse_protected_sheets_and_leave_existing_shapes_unchanged() {
    let mut wrong = Vec::new();
    for (command, params) in calls() {
        let mut session = with_stroke();
        session.execute("review.protectSheet", json!({})).unwrap();
        let before = session.doc().unwrap().wb.active().unwrap().shapes.clone();
        let result = session.execute(command, params);
        if result.is_ok() {
            wrong.push(format!("{command}: protected sheet accepted an ink write"));
        }
        if session.doc().unwrap().wb.active().unwrap().shapes != before {
            wrong.push(format!("{command}: protected sheet shapes changed"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn ink_writes_still_apply_on_unprotected_sheets() {
    for (command, params) in calls() {
        let mut session = with_stroke();
        let before = session.doc().unwrap().wb.active().unwrap().shapes.clone();
        session.execute(command, params).unwrap();
        assert_ne!(session.doc().unwrap().wb.active().unwrap().shapes, before, "{command} must change the unprotected sheet");
    }
}
