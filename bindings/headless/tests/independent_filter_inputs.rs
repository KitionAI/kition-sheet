//! C8-sheet-read: a current empty filter must not erase an observed criterion premise.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn reapply_holds_first_table_criterion_added_or_removed_outside_the_selection() {
    let mut failures = Vec::new();
    for removed in [false, true] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[("table.insert", json!({"range":"A1:D12","header":true,"name":"Sales"}))]);
            if removed {
                fixture.colleague(&[("data.filterBy", json!({"table":"Sales","column":"B","values":[2]}))]);
            }
            fixture.call("run", json!({"steps":[{"command":"select.set","params":{"range":"E20"}}]})).await.unwrap();
            let basis = fixture.basis().await;
            if related {
                if removed {
                    fixture.colleague(&[("data.clearFilter", json!({"table":"Sales"}))]);
                } else {
                    fixture.colleague(&[("data.filterBy", json!({"table":"Sales","column":"B","values":[2]}))]);
                }
            } else {
                fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"unrelated constant"}))]);
            }
            let before = fixture.bytes();
            let answer = fixture.run(basis, "data.reapply", json!({})).await;
            if related {
                if answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                    assert_eq!(fixture.bytes(), before, "refusal must preserve the colleague's file");
                } else {
                    let kind = if removed { "removed" } else { "added" };
                    let outcome = match answer {
                        Ok(value) => format!("stale write applied: {value}"),
                        Err(error) => format!("handler/kit error after guard: {error}"),
                    };
                    failures.push(format!("first Sales criterion {kind}, active E20 is outside filters: {outcome}"));
                }
            } else if let Err(error) = answer {
                failures.push(format!("unrelated Sheet2 cell must apply with removed={removed}: {error}"));
            }
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
