//! C8-sheet-pin: each hold reduction pairs an allowed change with a necessary refusal.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
// The native headless fixture deliberately backdates an outside file's mtime.
#[allow(clippy::disallowed_methods)]
mod common;
use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn conflict(result: Result<Value, Value>, before: &[u8], f: &Fixture) {
    let error = result.expect_err("necessary change must refuse");
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(f.bytes(), before, "refusal must not write");
}

#[tokio::test]
async fn identity_painter_pickup_ignores_deleted_unrelated_objects_but_holds_source_style() {
    let mut failures = Vec::new();
    for necessary in [false, true] {
        let f = Fixture::new();
        f.colleague(&[("drawing.insert", json!({"kind":"shape","frame":{"x":20,"y":20,"w":80,"h":40}}))]);
        let basis = f.basis().await;
        if necessary {
            f.colleague(&[("format.bold", json!({"range":"C5","on":true}))]);
        } else {
            f.colleague(&[("object.delete", json!({"kind":"shape","id":1}))]);
        }
        let before = f.bytes();
        let result = f
            .call(
                "run",
                json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":"edit.formatPainter","on":["C5"],"params":{"sticky":true}}]}),
            )
            .await;
        if necessary {
            conflict(result, &before, &f);
        } else if let Err(error) = result {
            failures.push(format!("unrelated shape deletion refused pickup: {error}"));
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn position_anchor_mode_ignores_geometry_but_holds_the_assigned_mode() {
    let mut failures = Vec::new();
    for necessary in [false, true] {
        let f = Fixture::new();
        f.colleague(&[("drawing.insert", json!({"kind":"shape","frame":{"x":20,"y":20,"w":80,"h":40}}))]);
        let basis = f.basis().await;
        f.colleague(&[if necessary {
            ("object.setAnchorMode", json!({"kind":"shape","id":1,"mode":"moveOnly"}))
        } else {
            ("object.move", json!({"kind":"shape","id":1,"at":"D8","width":130}))
        }]);
        let before = f.bytes();
        let result = f.run(basis, "object.setAnchorMode", json!({"kind":"shape","id":1,"mode":"absolute"})).await;
        if necessary {
            conflict(result, &before, &f);
        } else if let Err(error) = result {
            failures.push(format!("unrelated geometry refused mode assignment: {error}"));
        } else {
            let (wb, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
            let anchor = wb.sheets[0].shapes[0].anchor;
            assert_eq!(anchor.cell.a1(), "D8");
            assert_eq!(anchor.width, 130.0);
            assert_eq!(anchor.mode, kition_sheet_engine::model::AnchorMode::Absolute);
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn stored_formula_sources_ignore_formatting_but_hold_values_and_cell_format_queries() {
    let mut failures = Vec::new();
    for (formula, command, params, necessary) in [
        ("=B2*2", "format.numberFormat", json!({"range":"B2","code":"0.00"}), false),
        ("=B2*2", "cell.set", json!({"cell":"B2","input":"99"}), true),
        ("=CELL(\"format\",B2)", "format.numberFormat", json!({"range":"B2","code":"0.00"}), true),
        ("=CELL(\"protect\",B2)", "cell.lock", json!({"range":"B2","on":false}), true),
    ] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[(command, params)]);
        let before = f.bytes();
        let result = f.run(basis, "cell.set", json!({"cell":"G7","input":formula})).await;
        if necessary {
            conflict(result, &before, &f);
        } else if let Err(error) = result {
            failures.push(format!("unrelated source formatting refused formula: {error}"));
        } else {
            let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            assert_eq!(saved.execute("cell.get", json!({"cell":"G7"})).unwrap()["value"], 4.0);
            let (wb, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
            let at = kition_sheet_engine::core::CellRef::parse("B2").unwrap();
            assert_eq!(wb.styles.get(wb.sheets[0].style_id(at)).num_fmt.as_str(), "0.00");
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
