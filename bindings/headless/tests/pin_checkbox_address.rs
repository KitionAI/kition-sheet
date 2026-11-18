//! C8-sheet-pin: delegation must preserve the caller's sheet addressing mode.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
// The native headless fixture deliberately backdates an outside file's mtime.
#[allow(clippy::disallowed_methods)]
mod common;
use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test]
async fn named_checkbox_creation_ignores_other_sheet_insertion_but_holds_numeric_addresses() {
    let mut failures = Vec::new();
    for change in ["other sheet", "numeric sheet", "earlier row", "same field"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[match change {
            "other sheet" | "numeric sheet" => ("home.insertSheet", json!({"before":0,"name":"Outside"})),
            "earlier row" => ("home.insertRows", json!({"rows":"Sheet1!2"})),
            "same field" => ("cell.set", json!({"cell":"C5","input":"99"})),
            _ => panic!("unknown change"),
        }]);
        let before = f.bytes();
        let params = if change == "numeric sheet" { json!({"sheet":0,"range":"C5"}) } else { json!({"sheet":"Sheet1","range":"C5"}) };
        let result = f.run(basis, "insert.checkbox", params).await;
        if change == "other sheet" {
            if let Err(error) = result {
                failures.push(format!("inserting another sheet refused a named checkbox: {error}"));
            } else {
                let mut actual = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
                assert_eq!(actual.execute("cell.get", json!({"cell":"Sheet1!C5"})).unwrap()["value"], false);
                assert!(actual.execute("cell.get", json!({"cell":"Outside!C5"})).unwrap()["value"].is_null());
                let (wb, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
                let sheet = wb.sheets.iter().find(|sheet| sheet.name == "Sheet1").unwrap();
                let at = kition_sheet_engine::core::CellRef::parse("C5").unwrap();
                assert!(wb.styles.get(sheet.style_id(at)).checkbox);
            }
        } else {
            let error = result.expect_err("a changed address or destination must refuse");
            assert_eq!(error["error"]["code"], "conflict", "{change}: {error}");
            assert!(error.to_string().contains(if change == "same field" { "changed" } else { "relocated" }), "{change}: {error}");
            assert_eq!(f.bytes(), before, "refusal must not write");
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
