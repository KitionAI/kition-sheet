//! C8-sheet-read: shared edit consequences still read inputs beyond the direct target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::{Value, json};

fn check(result: Result<Value, Value>, related: bool, name: &str, failures: &mut Vec<String>) {
    if related {
        if result.as_ref().err().map(|error| &error["error"]["code"]) != Some(&json!("conflict")) {
            failures.push(format!("{name}: changed input must conflict: {result:?}"));
        }
    } else if result.as_ref().ok().map(|answer| &answer["saved"]) != Some(&json!(true)) {
        failures.push(format!("{name}: unrelated change must apply: {result:?}"));
    }
}

#[tokio::test]
async fn row_resize_holds_connector_endpoints_used_by_shared_rerouting() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":220,"y":100,"w":80,"h":40}})),
            ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":380,"y":160,"w":80,"h":40}})),
            ("drawing.connect", json!({"from":1,"to":2})),
        ]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("drawing.transform", json!({"id":1,"dx":30}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"unrelated"}))]);
        }
        let before = fixture.bytes();
        let result = fixture.run(basis, "row.setHeight", json!({"rows":"1:1","height":44})).await;
        let refused = result.as_ref().err().map(|error| &error["error"]["code"]) == Some(&json!("conflict"));
        check(result, related, "row resize rerouting", &mut failures);
        if related && refused {
            assert_eq!(fixture.bytes(), before, "refusal preserves the colleague's endpoint and connector");
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn workbook_protection_only_holds_workbook_state() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":220,"y":100,"w":80,"h":40}})),
            ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":380,"y":160,"w":80,"h":40}})),
            ("drawing.connect", json!({"from":1,"to":2})),
        ]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("workbook.protect", json!({"on":true}))]);
        } else {
            fixture.colleague(&[("drawing.transform", json!({"id":1,"dx":30}))]);
        }
        check(fixture.run(basis, "workbook.protect", json!({"on":false})).await, related, "workbook protection dispatch", &mut failures);
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn header_rename_holds_cross_sheet_structured_formulas_it_rewrites() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("range.setValues", json!({"range":"A1","values":[["Region","Amount"],["East",2],["West",3]]})),
            ("table.insert", json!({"range":"A1:B3","header":true,"name":"Sales"})),
            ("cell.set", json!({"cell":"Sheet2!D10","input":"=SUM(Sales[Region])"})),
        ]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(
            "cell.set",
            if related { json!({"cell":"Sheet2!D10","input":"=SUM(Sales[Region])+1"}) } else { json!({"cell":"Sheet2!D11","input":"unrelated"}) },
        )]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "cell.set", json!({"cell":"A1","input":"Area"})).await;
        let refused = result.as_ref().err().map(|error| &error["error"]["code"]) == Some(&json!("conflict"));
        check(result, related, "header rename structured formulas", &mut failures);
        if related && refused {
            assert_eq!(fixture.bytes(), before, "refusal preserves the colleague's formula");
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn header_rename_only_holds_formulas_using_the_renamed_column() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("range.setValues", json!({"range":"A1","values":[["Region","Amount"],["East",2],["West",3]]})),
            ("table.insert", json!({"range":"A1:B3","header":true,"name":"Sales"})),
            ("cell.set", json!({"cell":"Sheet2!D10","input":"=SUM(Sales[Region])"})),
            ("cell.set", json!({"cell":"Sheet2!D11","input":"=SUM(Sales[Amount])"})),
        ]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(
            "cell.set",
            if related {
                json!({"cell":"Sheet2!D10","input":"=SUM(Sales[Region])+1"})
            } else {
                json!({"cell":"Sheet2!D11","input":"=SUM(Sales[Amount])+1"})
            },
        )]);
        check(fixture.run(basis, "cell.set", json!({"cell":"A1","input":"Area"})).await, related, "header column references", &mut failures);
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn structural_name_edit_allows_unchanged_table_formula_inputs() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("range.setValues", json!({"range":"A1","values":[["Region","Amount"],["East",2],["West",3]]})),
            ("table.insert", json!({"range":"A1:B3","header":true,"name":"Sales"})),
            ("cell.set", json!({"cell":"Sheet2!D10","input":"=SUM(Sales[Region])"})),
            ("name.define", json!({"name":"Result","refersTo":"=40"})),
        ]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("name.define", json!({"name":"Result","refersTo":"=41"}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"=SUM(Sales[Region])+1"}))]);
        }
        check(
            fixture.run(basis, "name.define", json!({"name":"Result","refersTo":"=42"})).await,
            related,
            "structural header synchronization",
            &mut failures,
        );
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn sheet_rename_holds_cross_sheet_formulas_it_rewrites() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"=Sheet1!B2"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(
            "cell.set",
            if related { json!({"cell":"Sheet2!D10","input":"=Sheet1!B2+1"}) } else { json!({"cell":"Sheet2!D11","input":"unrelated"}) },
        )]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "sheet.rename", json!({"sheet":"Sheet1","name":"Renamed"})).await;
        let refused = result.as_ref().err().map(|error| &error["error"]["code"]) == Some(&json!("conflict"));
        check(result, related, "sheet rename references", &mut failures);
        if related && refused {
            assert_eq!(fixture.bytes(), before, "refusal preserves the colleague's formula");
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn sheet_rename_holds_internal_link_rewrite_inputs_and_allows_external_links() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("link.insert", json!({"cell":"Sheet2!E10","target":"#Sheet1!A1","text":"inside"})),
            ("link.insert", json!({"cell":"Sheet2!E11","target":"https://example.org/old","text":"outside"})),
        ]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(
            "link.insert",
            if related {
                json!({"cell":"Sheet2!E10","target":"#Sheet1!A2","text":"inside"})
            } else {
                json!({"cell":"Sheet2!E11","target":"https://example.org/new","text":"outside"})
            },
        )]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "sheet.rename", json!({"sheet":"Sheet1","name":"Renamed"})).await;
        let refused = result.as_ref().err().map(|error| &error["error"]["code"]) == Some(&json!("conflict"));
        check(result, related, "sheet rename internal links", &mut failures);
        if related && refused {
            assert_eq!(fixture.bytes(), before, "refusal preserves the colleague's internal link");
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
