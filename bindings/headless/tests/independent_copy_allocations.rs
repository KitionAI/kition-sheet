//! C8-sheet-read: sheet copies allocate workbook-wide object IDs and table names.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test]
async fn sheet_copy_holds_global_object_ids_and_table_name_allocations() {
    let mut failures = Vec::new();
    for allocation in ["object", "table", "defined name"] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("insert.table", json!({"range":"A1:B3","header":true,"name":"Sales"})),
                ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":10,"y":20,"w":80,"h":40}})),
            ]);
            let basis = fixture.basis().await;
            if related && allocation == "table" {
                fixture.colleague(&[("insert.table", json!({"range":"Sheet2!A1:B3","header":true,"name":"Sales2"}))]);
            } else if related && allocation == "defined name" {
                fixture.colleague(&[("formulas.defineName", json!({"name":"Sales2","refersTo":"=Sheet2!$A$1"}))]);
            } else if related {
                fixture.colleague(&[
                    ("sheet.activate", json!({"sheet":"Sheet2"})),
                    ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":200,"y":20,"w":80,"h":40}})),
                    ("sheet.activate", json!({"sheet":"Sheet1"})),
                ]);
            } else {
                fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"unrelated"}))]);
            }
            let before = fixture.bytes();
            let result = fixture.run(basis, "sheet.move", json!({"sheet":"Sheet1","to":2,"copy":true})).await;
            if related {
                if result.as_ref().err().map(|error| &error["error"]["code"]) == Some(&json!("conflict")) {
                    assert_eq!(fixture.bytes(), before, "refusal preserves the colleague's allocation");
                } else {
                    failures.push(format!("{allocation} allocation: changed global allocation must conflict: {result:?}"));
                }
            } else if let Err(error) = result {
                failures.push(format!("{allocation} allocation: unrelated other-tab constant must apply: {error}"));
            }
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
