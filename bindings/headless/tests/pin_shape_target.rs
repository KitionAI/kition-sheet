//! C8-sheet-pin: a conditional shape target never writes the fallback shape.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn shape_fill_on_changes_the_named_shape_or_refuses_without_writing() {
    for explicit in [false, true] {
        let f = Fixture::new();
        f.colleague(&[
            ("shape.insert", json!({"kind":"rect","at":"F2","fill":"#112233"})),
            ("shape.insert", json!({"kind":"rect","at":"J2","fill":"#445566"})),
        ]);
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":"Z20","input":"outside"}))]);
        let before = f.bytes();
        let step = if explicit {
            json!({"command":"chart.shapeFill","params":{"kind":"shape","id":1,"color":"#abcdef"}})
        } else {
            json!({"command":"chart.shapeFill","on":[1],"params":{"kind":"shape","color":"#abcdef"}})
        };
        let answer = f.call("run", json!({"basis":basis,"steps":[step]})).await;
        match answer {
            Ok(_) => {
                let (book, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
                let shapes = &book.sheets[0].shapes;
                assert_eq!(shapes[0].fill.hex(&book.theme).as_deref(), Some("#ABCDEF"), "on must never edit the fallback shape");
                assert_eq!(shapes[1].fill.hex(&book.theme).as_deref(), Some("#445566"));
                let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
                assert_eq!(saved.execute("cell.get", json!({"cell":"Z20"})).unwrap()["value"], "outside");
            }
            Err(error) => {
                assert!(!explicit, "an explicit id must work: {error}");
                assert_eq!(error["error"]["code"], "command-failed", "{error}");
                assert_eq!(error["completed"], json!([]), "the ambiguous command must refuse before a write");
                assert!(error["error"]["message"].as_str().unwrap().contains("kind=shape takes id"), "{error}");
                assert_eq!(f.bytes(), before);
            }
        }
        f.shutdown().await;
    }
}
