//! C8-sheet-read: scalar chart writes respect drawing protection on every touched sheet.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::{Session, model::Chart};
use serde_json::{Value, json};

fn fixture() -> Session {
    let mut session = Session::new();
    session.new_workbook();
    session.execute("range.setValues", json!({"range":"A1","values":[["Name","Value","Other"],["One",1,2],["Two",3,4]]})).unwrap();
    session.execute("insert.chart", json!({"range":"A1:C3","type":"column","title":"Original"})).unwrap();
    session.execute("home.insertSheet", json!({"name":"Sheet2"})).unwrap();
    session.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
    session
}

fn calls() -> [(&'static str, Value); 10] {
    [
        ("chart.addElement", json!({"chart":1,"element":"title","on":false})),
        ("chart.quickLayout", json!({"chart":1,"layout":2})),
        ("chart.changeColors", json!({"chart":1,"colors":["#123456"]})),
        ("chart.selectData", json!({"chart":1,"range":"B1:C3"})),
        ("chart.changeType", json!({"chart":1,"type":"line"})),
        ("chart.formatSelection", json!({"chart":1,"series":0,"color":"#123456"})),
        ("chart.shapeFill", json!({"chart":1,"series":0,"color":"#123456"})),
        ("chart.set", json!({"chart":1,"title":"Changed"})),
        ("chart.switchRowColumn", json!({"chart":1})),
        ("chart.move", json!({"chart":1,"sheet":"Sheet2","at":"B2"})),
    ]
}

fn charts(session: &Session) -> Vec<Vec<Chart>> {
    session.doc().unwrap().wb.sheets.iter().map(|sheet| sheet.charts.clone()).collect()
}

#[test]
fn scalar_chart_writes_refuse_protected_sources_without_changing_charts() {
    let mut wrong = Vec::new();
    for (command, params) in calls() {
        let mut session = fixture();
        session.execute("review.protectSheet", json!({})).unwrap();
        let before = charts(&session);
        if session.execute(command, params).is_ok() {
            wrong.push(format!("{command} accepted a protected source"));
        }
        if charts(&session) != before {
            wrong.push(format!("{command} changed charts on a protected source"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn moving_a_chart_refuses_a_protected_destination_without_changing_either_sheet() {
    let mut session = fixture();
    session.execute("sheet.activate", json!({"sheet":"Sheet2"})).unwrap();
    session.execute("review.protectSheet", json!({})).unwrap();
    session.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
    let before = charts(&session);
    let result = session.execute("chart.move", json!({"chart":1,"sheet":"Sheet2","at":"B2"}));
    assert!(result.is_err(), "protected chart destination accepted: {result:?}");
    assert_eq!(charts(&session), before, "a refused move must preserve charts on both sheets");
}

#[test]
fn scalar_chart_writes_still_apply_to_unprotected_sheets() {
    for (command, params) in calls() {
        let mut session = fixture();
        let before = charts(&session);
        session.execute(command, params).unwrap();
        assert_ne!(charts(&session), before, "{command} must change the unprotected chart");
    }
}
