//! C8-sheet-pin: external table keys retain their own coordinate footprint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
// The native headless fixture deliberately backdates an outside file's mtime.
#[allow(clippy::disallowed_methods)]
mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test]
async fn external_duplicate_keys_hold_letter_and_relative_numeric_columns_without_shifting() {
    for columns in [json!(["E"]), json!([3])] {
        for related in [true, false] {
            let f = Fixture::new();
            f.colleague(&[("insert.table", json!({"range":"B1:D4","name":"Data","header":true}))]);
            let basis = f.basis().await;
            f.colleague(&[("cell.set", json!({"cell":if related {"E3"} else {"F3"},"input":"6"}))]);
            let before = f.bytes();
            let result = f.run(basis, "table.removeDuplicates", json!({"table":"Data","columns":columns})).await;
            if related {
                let error = result.expect_err("a changed external key must refuse");
                assert_eq!(error["error"]["code"], "conflict", "{error}");
                assert!(error.to_string().contains("changed"), "{error}");
                assert_eq!(f.bytes(), before, "refusal must not write");
            } else {
                assert!(result.as_ref().is_ok_and(|answer| answer["saved"] == true), "unrelated key column: {result:?}");
            }
            f.shutdown().await;
        }
    }
}
