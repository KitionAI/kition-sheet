//! C8-sheet-read: second-file writers change tab-name, visibility and style-default inputs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use serde_json::{Value, json};

fn result_kind(answer: &Result<Value, Value>) -> String {
    match answer {
        Ok(value) => format!("stale write applied: {value}"),
        Err(error) if error["error"]["code"] == "conflict" => "premise conflict".into(),
        Err(error) => format!("handler/kit error after guard: {error}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sheet_commands_hold_other_tabs_names_and_visibility() {
    let mut failures = Vec::new();
    for (command, params, other_command, other_params) in [
        // The observed next default is Sheet3. The colleague occupies it and frees Sheet2.
        ("sheet.insert", json!({}), "sheet.rename", json!({"sheet":"Sheet2","name":"Sheet3"})),
        ("sheet.rename", json!({"sheet":"Sheet1","name":"Renamed"}), "sheet.rename", json!({"sheet":"Sheet2","name":"Renamed"})),
        ("sheet.move", json!({"sheet":"Sheet1","to":2,"copy":true}), "sheet.rename", json!({"sheet":"Sheet2","name":"Sheet1(2)"})),
        ("sheet.delete", json!({"sheet":"Sheet1"}), "sheet.hide", json!({"sheet":"Sheet2"})),
        ("sheet.hide", json!({"sheet":"Sheet1"}), "sheet.hide", json!({"sheet":"Sheet2"})),
        ("sheet.move", json!({"sheet":"Sheet1","to":2,"copy":true}), "cell.set", json!({"cell":"Sheet1!B2","input":"source changed"})),
        ("sheet.delete", json!({"sheet":"Sheet1"}), "cell.set", json!({"cell":"Sheet1!B2","input":"source changed"})),
    ] {
        for related in [true, false] {
            let fixture = Fixture::new();
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[(other_command, other_params.clone())]);
            } else {
                fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"unrelated constant"}))]);
            }
            let before = fixture.bytes();
            let answer = fixture.run(basis, command, params.clone()).await;
            if related {
                if answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                    assert_eq!(fixture.bytes(), before, "refusal must preserve the colleague's file");
                } else {
                    failures.push(format!("{command} {params}: other-tab input changed; {}", result_kind(&answer)));
                }
            } else if let Err(error) = answer {
                failures.push(format!("{command} {params}: another sheet's constant must apply: {error}"));
            }
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn builtin_title_style_holds_the_theme_font_without_holding_other_sheet_cells() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("theme.fonts", json!({"major":"Kition Mono"}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"unrelated constant"}))]);
        }
        let before = fixture.bytes();
        let answer = fixture.run(basis, "style.apply", json!({"range":"B2","name":"Title"})).await;
        if related {
            if answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                assert_eq!(fixture.bytes(), before, "refusal must preserve the colleague's file");
            } else {
                failures.push(format!("Title's major font changed; {}", result_kind(&answer)));
            }
        } else if let Err(error) = answer {
            failures.push(format!("Title's theme source is unrelated to a second-sheet constant: {error}"));
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
