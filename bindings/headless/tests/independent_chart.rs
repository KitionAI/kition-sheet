//! A chart's source and destination parameters do not select the chart being edited.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn charts() -> Fixture {
    let fixture = Fixture::new();
    fixture.colleague(&[
        ("insert.chart", json!({"range":"A1:D12"})),
        ("sheet.activate", json!({"sheet":"Sheet2"})),
        ("range.setValues", json!({"range":"A1","values":[["Name","Value"],["A",1],["B",2]]})),
        ("insert.chart", json!({"range":"A1:B3"})),
        ("sheet.activate", json!({"sheet":"Sheet1"})),
    ]);
    fixture
}

async fn run(fixture: &Fixture, basis: Value, command: &str, params: Value) -> Result<Value, Value> {
    fixture.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":command,"params":params}]})).await
}

#[tokio::test]
async fn chart_move_destination_does_not_replace_the_chart_target() {
    for explicit in [false, true] {
        for related in [true, false] {
            let fixture = charts();
            let basis = fixture.basis().await;
            if related {
                // The move reads the retained frame. Title and series changes
                // are independently covered by the field-precision matrix.
                fixture.colleague(&[("chart.set", json!({"chart":1,"width":333}))]);
            } else {
                fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"unrelated"}))]);
            }
            let mut params = json!({"sheet":"Sheet2","at":"H5"});
            if explicit {
                params["chart"] = json!("drawing:1");
            }
            let before = fixture.bytes();
            let answer = run(&fixture, basis, "chart.move", params).await;
            if related {
                assert_eq!(answer.as_ref().err().map(|error| &error["error"]["code"]), Some(&json!("conflict")), "{answer:?}");
                assert_eq!(fixture.bytes(), before);
            } else {
                answer.expect("unrelated cell must allow moving the source chart");
                let reopened = SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
                let workbook: Value = serde_json::from_slice(&reopened.save(Some("view.json")).unwrap()).unwrap();
                assert_eq!(workbook["sheets"][0]["charts"].as_array().map(Vec::len), Some(0), "the source chart must leave Sheet1");
                let mut ids: Vec<_> = workbook["sheets"][1]["charts"].as_array().unwrap().iter().map(|chart| chart["id"].as_u64().unwrap()).collect();
                ids.sort_unstable();
                assert_eq!(ids, [1, 2], "destination chart 2 must remain a different object");
            }
            fixture.shutdown().await;
        }
    }
}

#[tokio::test]
async fn chart_data_range_holds_the_source_without_replacing_the_chart_target() {
    for related in [true, false] {
        let fixture = charts();
        let basis = fixture.basis().await;
        fixture.colleague(&[("cell.set", json!({"cell":if related { "Sheet2!B2" } else { "Sheet2!D10" },"input":"99"}))]);
        let before = fixture.bytes();
        let answer = run(&fixture, basis, "chart.selectData", json!({"chart":"drawing:1","range":"Sheet2!A1:B3"})).await;
        if related {
            assert_eq!(answer.as_ref().err().map(|error| &error["error"]["code"]), Some(&json!("conflict")), "{answer:?}");
            assert_eq!(fixture.bytes(), before);
        } else {
            let answer = answer.expect("unrelated cell must allow changing chart source");
            assert_eq!(answer["completed"][0]["result"]["chart"], 1);
        }
        fixture.shutdown().await;
    }
}
