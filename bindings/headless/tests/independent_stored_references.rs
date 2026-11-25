//! C8-sheet-read: stored references keep address intent without evaluating source values.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test]
async fn stored_name_and_validation_references_do_not_hold_source_values() {
    let mut failures = Vec::new();
    for (command, initial, changed, write) in [
        (
            "name.define",
            json!({"name":"Result","refersTo":"=Sheet2!$A$1"}),
            json!({"name":"Result","refersTo":"=Sheet2!$A$2"}),
            json!({"name":"Result","refersTo":"=Sheet2!$A$1"}),
        ),
        (
            "data.validation",
            json!({"range":"B2","type":"whole","formula1":"1","formula2":"3"}),
            json!({"range":"B2","type":"whole","formula1":"1","formula2":"4"}),
            json!({"range":"B2","type":"list","formula1":"Sheet2!A1:A3"}),
        ),
    ] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[(command, initial.clone())]);
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[(command, changed.clone())]);
            } else {
                fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!A1","input":"changed source value"}))]);
            }
            let before = fixture.bytes();
            let result = fixture.run(basis, command, write.clone()).await;
            if related {
                if result.as_ref().err().map(|error| &error["error"]["code"]) == Some(&json!("conflict")) {
                    assert_eq!(fixture.bytes(), before, "refusal preserves the changed definition");
                } else {
                    failures.push(format!("{command}: changed definition must conflict: {result:?}"));
                }
            } else if let Err(error) = result {
                failures.push(format!("{command}: referenced cell values are stored, not evaluated: {error}"));
            }
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
