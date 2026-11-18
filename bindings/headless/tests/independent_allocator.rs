//! C8-sheet-read: drawing creators consume one workbook-wide object-ID allocator.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::json;

#[tokio::test]
async fn connect_and_group_hold_global_allocation_without_holding_another_sheets_cells() {
    let mut wrong = Vec::new();
    for (command, params) in [("drawing.connect", json!({"from":1,"to":2})), ("drawing.group", json!({"ids":["1","2"]}))] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("drawing.insert", json!({"kind":"shape","frame":{"x":10,"y":10,"w":80,"h":40}})),
                ("drawing.insert", json!({"kind":"shape","frame":{"x":180,"y":20,"w":80,"h":40}})),
            ]);
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[
                    ("sheet.activate", json!({"sheet":"Sheet2"})),
                    ("drawing.insert", json!({"kind":"shape","frame":{"x":10,"y":10,"w":80,"h":40}})),
                    ("sheet.activate", json!({"sheet":"Sheet1"})),
                ]);
            } else {
                fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
            }
            let bytes = fixture.bytes();
            let result = fixture.run(basis, command, params.clone()).await;
            if related {
                if result.as_ref().err().map(|error| &error["error"]["code"]) != Some(&json!("conflict")) {
                    wrong.push(format!("{command}: changed global allocation must conflict: {result:?}"));
                }
                if fixture.bytes() != bytes {
                    wrong.push(format!("{command}: changed allocation was followed by a write"));
                }
            } else {
                if result.as_ref().ok().map(|output| &output["saved"]) != Some(&json!(true)) {
                    wrong.push(format!("{command}: unrelated sheet cells must apply: {result:?}"));
                }
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
