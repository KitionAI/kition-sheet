//! C8-sheet-read: table duplicate keys may name columns outside the table.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::json;

#[tokio::test]
async fn table_duplicate_removal_holds_external_comparison_keys() {
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[("table.insert", json!({"range":"A1:C4","name":"Data","header":true}))]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("cell.set", json!({"cell":"D3","input":"6"}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
        }
        let before = fixture.bytes();
        let result = fixture.run(basis, "table.removeDuplicates", json!({"table":"Data","columns":["D"]})).await;
        if related {
            assert_eq!(result.as_ref().err().map(|error| &error["error"]["code"]), Some(&json!("conflict")), "external key changed: {result:?}");
            assert_eq!(fixture.bytes(), before, "refusal must preserve the colleague's file");
        } else {
            assert_eq!(result.as_ref().ok().map(|output| &output["saved"]), Some(&json!(true)), "unrelated sheet cell must apply: {result:?}");
        }
        fixture.shutdown().await;
    }
}
