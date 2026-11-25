//! C8-sheet-read-2: creation reads an anchor and allocator, not unrelated cells.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use kition_kit::KitDocument;
use serde_json::json;

#[tokio::test]
async fn anchored_creation_holds_its_gap_and_allocator_but_allows_unrelated_cells() {
    let mut wrong = Vec::new();
    for command in ["shape.insert", "icon.insert", "textBox.insert"] {
        for change in ["anchor", "allocator", "unrelated"] {
            let fixture = Fixture::new();
            let basis = fixture.basis().await;
            match change {
                "anchor" => fixture.colleague(&[("row.insert", json!({"rows":"1:1"}))]),
                "allocator" => fixture.colleague(&[("shape.insert", json!({"at":"Sheet2!A1"}))]),
                _ => fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!A1","input":"colleague"}))]),
            }
            let outside = fixture.bytes();
            let result = fixture.run(basis, command, json!({"at":"C5"})).await;
            if change == "unrelated" {
                if !result.as_ref().is_ok_and(|answer| answer["saved"] == true) {
                    wrong.push(format!("{command}: unrelated cell must apply: {result:?}"));
                } else {
                    let mut saved = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
                    assert_eq!(saved.execute("cell.get", json!({"cell":"Sheet2!A1"})).unwrap()["value"], "colleague");
                    let (book, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
                    let sheet = book.sheet(0).unwrap();
                    let anchor = sheet.shapes.first().map(|shape| &shape.anchor).or_else(|| sheet.images.first().map(|image| &image.anchor)).unwrap();
                    assert_eq!(anchor.cell.a1(), "C5");
                }
            } else {
                if !result.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                    wrong.push(format!("{command}/{change}: changed prerequisite must conflict: {result:?}"));
                }
                assert_eq!(fixture.bytes(), outside, "{command}/{change}: refusal preserves outside bytes");
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn insert_function_holds_checkbox_eligibility_but_allows_unrelated_cells() {
    let mut wrong = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        fixture.colleague(&[if related {
            ("checkbox.insert", json!({"range":"C5"}))
        } else {
            ("cell.set", json!({"cell":"Sheet2!A1","input":"colleague"}))
        }]);
        let outside = fixture.bytes();
        let answer = fixture.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":"formula.insertFunction","on":["C5"],"params":{"name":"SUM"}}]})).await;
        if related {
            if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                wrong.push(format!("changed enabled prerequisite must conflict: {answer:?}"));
            }
            assert_eq!(fixture.bytes(), outside);
        } else if answer.is_err() {
            wrong.push(format!("unrelated cell must allow the formula request: {answer:?}"));
        } else {
            assert_eq!(fixture.bytes(), outside, "a formula request does not change the saved workbook");
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
