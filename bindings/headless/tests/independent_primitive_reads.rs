//! C8-sheet-read: checkbox toggles and direct ink strokes read local primitive inputs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn check(answer: &Result<Value, Value>, related: bool, label: &str, failures: &mut Vec<String>) {
    let status = match answer {
        Ok(output) => format!("saved={}", output["saved"]),
        Err(error) => format!("error={}", error["error"]["code"]),
    };
    if related {
        if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
            failures.push(format!("{label}: changed primitive input must conflict: {status}"));
        }
    } else if answer.as_ref().ok().and_then(|answer| answer["saved"].as_bool()) != Some(true) {
        failures.push(format!("{label}: unrelated change must apply: {status}"));
    }
}

#[tokio::test]
async fn checkbox_toggle_holds_its_literal_input_and_allows_other_plain_cells() {
    let mut failures = Vec::new();
    for selection in [false, true] {
        for changed in ["B2", "D2", "Sheet2!D10"] {
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("checkbox.insert", json!({"range":"B2:C2"})),
                ("cell.set", json!({"cell":"B2","input":"TRUE"})),
                ("cell.set", json!({"cell":"C2","input":"TRUE"})),
            ]);
            let basis = fixture.basis().await;
            let related = changed == "B2";
            fixture.colleague(&[("cell.set", json!({"cell":changed,"input":if related { "FALSE" } else { "colleague" }}))]);
            let before = fixture.bytes();
            let answer = if selection {
                fixture.call("run", json!({"basis":basis,"steps":[{"command":"cell.toggleCheckbox","on":["Sheet1!B2:C2"],"params":{}}]})).await
            } else {
                fixture.run(basis, "cell.toggleCheckbox", json!({"cell":"B2"})).await
            };
            check(&answer, related, &format!("checkbox selection={selection}, outside {changed}"), &mut failures);
            if related {
                assert_eq!(fixture.bytes(), before, "a conflict preserves the outside checkbox value");
            } else if answer.is_ok() {
                let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
                let first = saved.sheet(0).unwrap();
                assert_eq!(first.value(kition_sheet_engine::core::CellRef::parse("B2").unwrap()).display(), "FALSE");
                assert_eq!(first.value(kition_sheet_engine::core::CellRef::parse("C2").unwrap()).display(), if selection { "FALSE" } else { "TRUE" });
                let (si, at) = if changed.contains('!') { (1, "D10") } else { (0, "D2") };
                assert_eq!(saved.sheet(si).unwrap().value(kition_sheet_engine::core::CellRef::parse(at).unwrap()).display(), "colleague");
            }
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn checkbox_toggle_holds_the_explicit_cells_merge_anchor() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("cell.merge", json!({"range":"B2:C2"})),
            ("checkbox.insert", json!({"range":"B2"})),
            ("cell.set", json!({"cell":"B2","input":"TRUE"})),
        ]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("cell.unmerge", json!({"range":"B2:C2"}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"colleague"}))]);
        }
        let before = fixture.bytes();
        let answer = fixture.run(basis, "cell.toggleCheckbox", json!({"cell":"C2"})).await;
        check(&answer, related, "checkbox merged C2", &mut failures);
        if related {
            assert_eq!(fixture.bytes(), before);
        } else if answer.is_ok() {
            let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            assert_eq!(saved.sheet(0).unwrap().value(kition_sheet_engine::core::CellRef::parse("B2").unwrap()).display(), "FALSE");
            assert_eq!(saved.sheet(1).unwrap().value(kition_sheet_engine::core::CellRef::parse("D10").unwrap()).display(), "colleague");
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn checkbox_toggle_refuses_a_boolean_created_in_an_observed_empty_inherited_checkbox() {
    let fixture = Fixture::new();
    fixture.colleague_rewrite(|workbook| {
        let style = workbook.styles.derive(kition_sheet_engine::model::StyleId::DEFAULT, |style| style.checkbox = true);
        workbook.sheet_mut(0).unwrap().rows.entry(19).or_default().style = Some(style);
    });
    let (observed, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
    let at = kition_sheet_engine::core::CellRef::parse("A20").unwrap();
    let sheet = observed.sheet(0).unwrap();
    assert!(sheet.cell(at).is_none(), "the observed address must have no materialized cell");
    assert!(observed.styles.get(sheet.style_id(at)).checkbox, "the empty address must inherit its row's checkbox style");
    let basis = fixture.basis().await;
    fixture.colleague(&[("cell.set", json!({"cell":"A20","input":"TRUE"}))]);
    let before = fixture.bytes();
    let answer = fixture.run(basis, "cell.toggleCheckbox", json!({"cell":"A20"})).await;
    assert!(answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict"), "a new boolean changes whether the empty cell qualifies");
    assert_eq!(fixture.bytes(), before, "refusal preserves the new outside boolean");
    fixture.shutdown().await;
}

#[tokio::test]
async fn direct_ink_holds_anchor_geometry_and_id_allocation_but_keeps_unrelated_changes() {
    let mut failures = Vec::new();
    for change in ["anchor row", "object allocation", "distant row", "same sheet cell", "other sheet cell"] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        match change {
            "anchor row" => fixture.colleague(&[("row.setHeight", json!({"rows":"1:1","height":44}))]),
            "object allocation" => fixture.colleague(&[("shape.insert", json!({"kind":"triangle","at":"Sheet2!B2"}))]),
            "distant row" => fixture.colleague(&[("row.setHeight", json!({"rows":"50:50","height":44}))]),
            "same sheet cell" => fixture.colleague(&[("cell.set", json!({"cell":"Z20","input":"colleague"}))]),
            _ => fixture.colleague(&[("cell.set", json!({"cell":"Sheet2!D10","input":"colleague"}))]),
        }
        let related = matches!(change, "anchor row" | "object allocation");
        let before = fixture.bytes();
        // An agent call restores personal modes afterwards. Set the pen in this
        // run so the following stroke, whose parameters omit it, reads that pen.
        let answer = fixture
            .call(
                "run",
                json!({"basis":basis,"steps":[
                    {"command":"ink.highlighter","params":{"color":"#336699","width":7}},
                    {"command":"ink.stroke","params":{"points":[[160,120],[240,160]]}}
                ]}),
            )
            .await;
        check(&answer, related, &format!("ink {change}"), &mut failures);
        if related {
            assert_eq!(fixture.bytes(), before);
        } else if answer.is_ok() {
            let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            let shape = saved.sheet(0).unwrap().shapes.last().unwrap();
            assert_eq!(shape.line, kition_sheet_engine::model::Color::rgb(0x33, 0x66, 0x99), "direct stroke keeps the current pen color");
            assert_eq!(kition_sheet_engine::cmd::draw::ink_width(shape), 7.0, "direct stroke keeps the current pen width");
            assert_eq!(kition_sheet_engine::cmd::draw::ink_tool(shape), "highlighter");
            if change == "distant row" {
                assert_eq!(saved.sheet(0).unwrap().rows.get(&49).unwrap().size, Some(44.0));
            } else {
                let (si, at) = if change == "same sheet cell" { (0, "Z20") } else { (1, "D10") };
                assert_eq!(saved.sheet(si).unwrap().value(kition_sheet_engine::core::CellRef::parse(at).unwrap()).display(), "colleague");
            }
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn primitive_defaults_follow_the_restored_editor_state_in_the_direct_binding() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("checkbox.insert", json!({"range":"B2:C2"})).unwrap();
    for cell in ["B2", "C2"] {
        doc.execute("cell.set", json!({"cell":cell,"input":"TRUE"})).unwrap();
    }
    doc.execute("select.set", json!({"range":"B2:C2"})).unwrap();
    doc.execute("ink.highlighter", json!({"color":"#336699","width":7})).unwrap();
    let personal = doc.editor_state();
    doc.execute("select.set", json!({"range":"D10"})).unwrap();
    doc.execute("ink.pen", json!({"color":"#123456","width":2})).unwrap();
    doc.restore_editor_state(&personal).unwrap();
    assert_eq!(doc.editor_state(), personal, "the kit restores selection and current pen without relying on cross-call state");

    let toggle = doc.execute("cell.toggleCheckbox", json!({})).unwrap();
    assert_eq!(toggle["toggled"], 2);
    doc.execute("ink.stroke", json!({"points":[[160,120],[240,160]]})).unwrap();
    let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    let sheet = saved.sheet(0).unwrap();
    for cell in ["B2", "C2"] {
        assert_eq!(sheet.value(kition_sheet_engine::core::CellRef::parse(cell).unwrap()).display(), "FALSE");
    }
    let shape = sheet.shapes.last().unwrap();
    assert_eq!(shape.line, kition_sheet_engine::model::Color::rgb(0x33, 0x66, 0x99));
    assert_eq!(kition_sheet_engine::cmd::draw::ink_width(shape), 7.0);
    assert_eq!(kition_sheet_engine::cmd::draw::ink_tool(shape), "highlighter");
}
