//! C8-sheet-read: independently rewritten files exercise source reads found in handlers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test]
async fn sort_key_outside_rectangle_refuses_changed_source_and_allows_unrelated_cell() {
    for (key, orientation, changed, unrelated) in
        [(json!({"row":20,"order":"asc"}), "columns", "B20", "Z20"), (json!({"column":"Z","order":"asc"}), "rows", "Z3", "Y20")]
    {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("cell.set", json!({"cell":"A20","input":"3"})),
                ("cell.set", json!({"cell":"B20","input":"2"})),
                ("cell.set", json!({"cell":"C20","input":"1"})),
                ("cell.set", json!({"cell":"Z2","input":"3"})),
                ("cell.set", json!({"cell":"Z3","input":"2"})),
                ("cell.set", json!({"cell":"Z4","input":"1"})),
            ]);
            let basis = fixture.basis().await;
            fixture.colleague(&[("cell.set", json!({"cell":if related {changed} else {unrelated},"input":"0"}))]);
            let before = fixture.bytes();
            let answer = fixture.run(basis, "data.sort", json!({"range":"A2:C4","header":false,"orientation":orientation,"keys":[key]})).await;
            if related {
                assert!(answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict"), "a changed external key must refuse: {answer:?}");
                assert_eq!(fixture.bytes(), before, "refusal must preserve the colleague's file");
            } else {
                assert!(answer.is_ok(), "an unrelated key-column row must apply: {answer:?}");
            }
            fixture.shutdown().await;
        }
    }
}
