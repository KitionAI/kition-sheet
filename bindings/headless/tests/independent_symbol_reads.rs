//! C8-sheet-read-2: symbol insertion composes an entry at the call's active cell.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use kition_kit::KitDocument;
use serde_json::json;

#[tokio::test]
async fn symbol_entry_holds_its_composed_input_and_formula_sources_but_allows_another_sheet() {
    let mut failures = Vec::new();
    for formula in [false, true] {
        for change in ["destination", "source", "unrelated"] {
            let fixture = Fixture::new();
            fixture.colleague(&[("cell.set", json!({"cell":"C5","input":if formula {"=B1"} else {"old"}}))]);
            let basis = fixture.basis().await;
            let changed_cell = match change {
                "destination" => "C5",
                "source" if formula => "C1",
                "source" => "C5",
                _ => "Sheet2!A1",
            };
            fixture.colleague(&[("cell.set", json!({"cell":changed_cell,"input":"42"}))]);
            let outside = fixture.bytes();
            let result = fixture.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":"symbol.insert","on":["C5"],"params":{"char":if formula {"+C1"} else {"€"}}}]})).await;
            if change == "unrelated" {
                if let Err(error) = result {
                    failures.push(format!("formula={formula}: unrelated sheet must apply: {error}"));
                } else {
                    let mut saved = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
                    let value = saved.execute("cell.get", json!({"cell":"C5"})).unwrap();
                    assert_eq!(value["value"], if formula { json!(3.0) } else { json!("old€") });
                    assert_eq!(value["formula"], if formula { json!("=B1+C1") } else { serde_json::Value::Null });
                    assert_eq!(saved.execute("cell.get", json!({"cell":"Sheet2!A1"})).unwrap()["value"], 42.0);
                }
            } else {
                if !result.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    failures.push(format!("formula={formula} {change}: a necessary change must conflict: {result:?}"));
                }
                assert_eq!(fixture.bytes(), outside, "a refusal preserves the independent writer's file");
            }
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
