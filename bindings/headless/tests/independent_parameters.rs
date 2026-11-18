//! C8-sheet-read: similarly named parameters have different object-resolution roles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::json;

#[tokio::test]
async fn drawing_frames_accept_the_objects_used_by_the_engine() {
    for command in ["drawing.insert", "drawing.transform"] {
        let fixture = Fixture::new();
        fixture.colleague(&[("drawing.insert", json!({"kind":"shape","frame":{"x":10,"y":20,"w":80,"h":40}}))]);
        let basis = fixture.basis().await;
        let mut params = json!({"frame":{"x":120,"y":80,"w":100,"h":60}});
        if command == "drawing.transform" {
            params["ids"] = json!(["drawing:1"]);
        } else {
            params["kind"] = json!("shape");
        }
        let answer = fixture.run(basis, command, params).await;
        assert!(answer.as_ref().is_ok_and(|value| value["saved"] == true), "{command} frame object: {answer:?}");
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn file_import_commands_keep_their_disabled_result_and_do_not_write() {
    for (command, text) in [("data.fromTextCsv", "Name,Value\nOne,1"), ("data.getData", "[{\"Name\":\"One\",\"Value\":1}]")] {
        for name in ["Imported", "Sheet2"] {
            let fixture = Fixture::new();
            let basis = fixture.basis().await;
            let bytes = fixture.bytes();
            let error = fixture
                .run(basis, command, json!({"sheet":name,"text":text}))
                .await
                .expect_err("commands with typed file inputs are unavailable to agents");
            assert_eq!(error["error"]["code"], "command-failed");
            assert_eq!(
                error["error"]["message"],
                format!(
                    "step 0 (`{command}`) failed: command disabled: commands that name files are not available to the agent; the host opens and saves documents"
                )
            );
            assert_eq!(error["saved"], false);
            assert_eq!(error["completed"], json!([]));
            assert_eq!(fixture.bytes(), bytes);
            fixture.shutdown().await;
        }
    }
}

#[tokio::test]
async fn arrange_group_ids_follow_kind_for_chart_and_image_anchors() {
    let mut wrong = Vec::new();
    for kind in ["chart", "image"] {
        for related in [true, false] {
            let fixture = Fixture::new();
            if kind == "chart" {
                fixture.colleague(&[
                    ("insert.chart", json!({"range":"A1:C4","at":"B2","type":"column"})),
                    ("insert.chart", json!({"range":"A1:C4","at":"F5","type":"column"})),
                ]);
            } else {
                fixture.colleague(&[
                    ("insert.picture", json!({"base64":"AA==","at":"B2","width":80,"height":40})),
                    ("insert.picture", json!({"base64":"AA==","at":"F5","width":80,"height":40})),
                ]);
            }
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[("object.move", json!({"kind":kind,"id":1,"at":"C3"}))]);
            } else {
                fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
            }
            let bytes = fixture.bytes();
            let result = fixture.run(basis, "arrange.group", json!({"kind":kind,"ids":[1,2]})).await;
            if related {
                if result.as_ref().err().map(|error| &error["error"]["code"]) != Some(&json!("conflict")) {
                    wrong.push(format!("arrange.group/{kind}: changed anchors must conflict: {result:?}"));
                }
                if fixture.bytes() != bytes {
                    wrong.push(format!("arrange.group/{kind}: conflicting alignment wrote the file"));
                }
            } else if result.as_ref().ok().map(|output| &output["saved"]) != Some(&json!(true)) {
                wrong.push(format!("arrange.group/{kind}: matching typed ids and unrelated cells must apply: {result:?}"));
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
