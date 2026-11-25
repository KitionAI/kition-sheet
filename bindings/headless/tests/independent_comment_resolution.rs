//! C8-sheet-read: deletion resolves the supplied cell before falling back to selection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn delete_comment_holds_the_supplied_cell_instead_of_the_selection() {
    let mut failures = Vec::new();
    for sheet in ["Sheet1", "Sheet2"] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("comment.add", json!({"cell":format!("{sheet}!A1"),"text":"selected thread","author":"Owner"})),
                ("comment.add", json!({"cell":format!("{sheet}!B2"),"text":"other thread","author":"Owner"})),
            ]);
            fixture.call("run", json!({"steps":[{"command":"select.set","params":{"range":"Sheet1!A1"}}]})).await.unwrap();
            let basis = fixture.basis().await;
            let changed = format!("{sheet}!{}", if related { "B2" } else { "A1" });
            fixture.colleague(&[("comment.reply", json!({"cell":changed,"text":"colleague reply","author":"Colleague"}))]);
            let before = fixture.bytes();
            let supplied = if sheet == "Sheet1" { "B2" } else { "Sheet2!B2" };
            let answer = fixture.run(basis, "comment.delete", json!({"cell":supplied})).await;
            if related {
                if answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                    assert_eq!(fixture.bytes(), before, "refusal must preserve the colleague's reply");
                } else {
                    failures.push(format!("named {supplied} thread changed while selection remains A1: {answer:?}"));
                }
            } else if let Err(error) = answer {
                failures.push(format!("selected {sheet}!A1 thread is unrelated to named {supplied}: {error}"));
            } else {
                assert_ne!(fixture.bytes(), before, "allowed deletion must remove the named thread");
            }
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
