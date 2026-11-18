//! C8-sheet-read: independent writers change the actual inputs of editing commands.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn cross_sheet_font_step_holds_the_remembered_active_cell() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"cell":"Sheet2!A1","input":"source"})), ("cell.set", json!({"cell":"Sheet2!B2","input":"destination"}))]);
        f.call("run", json!({"steps":[{"command":"selection.set","params":{"range":"Sheet1!C5"}}]})).await.unwrap();
        let basis = f.basis().await;
        let range = if related { "Sheet2!A1" } else { "Sheet2!D10" };
        f.colleague(&[("home.fontSize", json!({"range":range,"size":24}))]);
        let before = f.bytes();
        let answer = f.run(basis, "home.increaseFontSize", json!({"range":"Sheet2!B2"})).await;
        if related {
            if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                failures.push(format!("remembered A1 changed, stale font step must refuse: {answer:?}"));
            } else {
                assert_eq!(f.bytes(), before);
            }
        } else if let Err(error) = answer {
            failures.push(format!("D10 is unrelated to active A1 and destination B2: {error}"));
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn inserted_rows_hold_the_neighbour_formats_they_inherit() {
    let mut failures = Vec::new();
    for (command, params, source, unrelated) in [
        ("home.insertRows", json!({"rows":"5:5"}), "B4", "B10"),
        ("home.insertRows", json!({"rows":"5:5"}), "E4", "B10"),
        ("home.insertRows", json!({"rows":"5:5","below":true}), "B6", "B10"),
        ("home.insertColumns", json!({"cols":"C:C"}), "B4", "D10"),
        ("home.insertColumns", json!({"cols":"C:C","right":true}), "D4", "A10"),
    ] {
        for related in [true, false] {
            let f = Fixture::new();
            let basis = f.basis().await;
            let range = format!("Sheet1!{}", if related { source } else { unrelated });
            f.colleague(&[("home.fontSize", json!({"range":range,"size":24}))]);
            let before = f.bytes();
            let answer = f.run(basis, command, params.clone()).await;
            if related {
                if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    failures.push(format!("{command} {params}: inherited {source} style changed, insert must refuse: {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                failures.push(format!("{command} {params}: {unrelated} style is unrelated to inherited {source}: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cell_entries_hold_the_other_cells_that_determine_row_height() {
    let mut failures = Vec::new();
    for (command, params) in [
        ("cell.set", json!({"cell":"B7","input":"changed"})),
        ("range.setValues", json!({"range":"B7","values":[["changed"]]})),
        ("range.fill", json!({"range":"B7","input":"changed"})),
        ("edit.clearContents", json!({"range":"B7"})),
    ] {
        for related in [true, false] {
            let f = Fixture::new();
            // The destination masks C7's contribution until this call removes its ten lines.
            f.colleague(&[("cell.set", json!({"cell":"Sheet1!B7","input":"a\nb\nc\nd\ne\nf\ng\nh\ni\nj"}))]);
            let basis = f.basis().await;
            let range = if related { "Sheet1!C7" } else { "Sheet1!C10" };
            f.colleague(&[("home.fontSize", json!({"range":range,"size":40}))]);
            let before = f.bytes();
            let answer = f.run(basis, command, params.clone()).await;
            if related {
                if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    failures.push(format!("{command}: fitted-row C7 changed, entry must refuse: {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                failures.push(format!("{command}: C10 is outside fitted row 7: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn sheet_structure_commands_declare_workbook_protection() {
    let mut failures = Vec::new();
    for (command, params) in [
        ("home.insertSheet", json!({"name":"Added"})),
        ("home.deleteSheet", json!({"sheet":"Sheet2"})),
        ("sheet.move", json!({"sheet":"Sheet2","to":0})),
    ] {
        for related in [true, false] {
            let f = Fixture::new();
            let basis = f.basis().await;
            if related {
                f.colleague(&[("review.protectWorkbook", json!({"on":true}))]);
            } else {
                f.colleague(&[("cell.set", json!({"cell":"Sheet1!D10","input":"unrelated"}))]);
            }
            let before = f.bytes();
            let answer = f.run(basis, command, params.clone()).await;
            if related {
                if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    failures.push(format!("{command}: changed workbook protection must conflict before dispatch: {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                failures.push(format!("{command}: unrelated first-sheet constant refused: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn inserted_rows_hold_the_conditional_format_boundary_they_extend() {
    let mut failures = Vec::new();
    for (command, params, boundary, unrelated) in [
        ("home.insertRows", json!({"rows":"5:5"}), "Sheet1!B2:B4", "Sheet1!B8:B10"),
        ("home.insertRows", json!({"rows":"5:5","below":true}), "Sheet1!B2:B5", "Sheet1!B8:B10"),
        ("home.insertColumns", json!({"cols":"C:C"}), "Sheet1!A2:B4", "Sheet1!D8:F10"),
        ("home.insertColumns", json!({"cols":"C:C","right":true}), "Sheet1!A2:C4", "Sheet1!D8:F10"),
    ] {
        for related in [true, false] {
            let f = Fixture::new();
            let basis = f.basis().await;
            let range = if related { boundary } else { unrelated };
            f.colleague(&[("home.conditionalFormat", json!({"range":range,"rule":{"type":"expression","formula":"1=1"}}))]);
            let before = f.bytes();
            let answer = f.run(basis, command, params.clone()).await;
            if related {
                if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                    failures.push(format!("{command} {params}: new CF at {boundary} is extended by insertion: {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                failures.push(format!("{command} {params}: CF at {unrelated} does not cross the inherited boundary: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
