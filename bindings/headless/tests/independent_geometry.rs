//! C8-sheet-read: drawing coordinates read the sizes behind row/column identities.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::json;

#[tokio::test]
async fn drawing_coordinates_hold_axis_sizes_without_holding_another_sheets_cells() {
    let mut wrong = Vec::new();
    for (command, params) in [
        ("arrange.align", json!({"ids":[1,2],"align":"left"})),
        ("arrange.group", json!({"ids":[1,2]})),
        ("drawing.connect", json!({"from":1,"to":2})),
        ("drawing.transform", json!({"id":1,"dx":20})),
        ("drawing.format", json!({"id":1,"fill":{"color":"#123456"}})),
        ("drawing.text", json!({"id":1,"text":"updated"})),
    ] {
        for (size_command, initial, changed) in [
            ("column.setWidth", json!({"cols":"A:A","width":20}), json!({"cols":"A:A","width":30})),
            ("row.setHeight", json!({"rows":"1:1","height":24}), json!({"rows":"1:1","height":34})),
        ] {
            for related in [true, false] {
                let fixture = Fixture::new();
                fixture.colleague(&[
                    (size_command, initial.clone()),
                    ("drawing.insert", json!({"kind":"shape","frame":{"x":220,"y":100,"w":80,"h":40}})),
                    ("drawing.insert", json!({"kind":"shape","frame":{"x":380,"y":160,"w":80,"h":40}})),
                ]);
                if matches!(command, "drawing.transform" | "drawing.format" | "drawing.text") {
                    fixture.colleague(&[("drawing.connect", json!({"from":1,"to":2}))]);
                }
                let basis = fixture.basis().await;
                if related {
                    fixture.colleague(&[(size_command, changed.clone())]);
                } else {
                    fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
                }
                let bytes = fixture.bytes();
                let result = fixture.run(basis, command, params.clone()).await;
                if related {
                    if result.as_ref().err().map(|error| &error["error"]["code"]) != Some(&json!("conflict")) {
                        wrong.push(format!("{command}/{size_command}: changed axis size must conflict: {result:?}"));
                    }
                    if fixture.bytes() != bytes {
                        wrong.push(format!("{command}/{size_command}: stale coordinates were followed by a write"));
                    }
                } else if result.as_ref().ok().map(|output| &output["saved"]) != Some(&json!(true)) {
                    wrong.push(format!("{command}/{size_command}: unrelated sheet cells must apply: {result:?}"));
                }
                fixture.shutdown().await;
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
