//! C8-sheet-same-field: same-field decision.
//! An absolute assignment holds the observed value it replaces, including removal.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::{Value, json};

fn check(result: &Result<Value, Value>, before: &[u8], after: &[u8], label: &str, failures: &mut Vec<String>) {
    if !result.as_ref().is_err_and(|error| error["error"]["code"] == "conflict" && error["conflicts"].to_string().contains("changed")) {
        failures.push(format!("{label}: expected changed, got {result:?}"));
    }
    if before != after {
        failures.push(format!("WRONG_WRITE {label}: replaced colleague value"));
    }
}

#[tokio::test]
async fn scalar_chart_assignments_hold_every_replaced_field() {
    let mut failures = Vec::new();
    for (field, agent, colleague) in [
        ("title", json!("Agent"), json!("Colleague")),
        ("legend", json!("top"), json!("none")),
        ("dataLabels", json!(false), json!(true)),
        ("gridlines", json!(true), json!(false)),
        ("type", json!("line"), json!("area")),
        ("style", json!(3), json!(8)),
        ("xTitle", json!("Agent"), json!("Colleague")),
        ("yTitle", json!("Agent"), json!("Colleague")),
        ("totals", json!([0]), json!([1])),
        ("at", json!("C3"), json!("D4")),
        ("width", json!(400), json!(350)),
        ("height", json!(300), json!(250)),
        ("dx", json!(4), json!(8)),
        ("dy", json!(4), json!(8)),
    ] {
        let fixture = Fixture::new();
        fixture.colleague(&[("chart.insert", json!({"range":"A1:C4","type":if field == "totals" {"waterfall"} else {"column"},"title":"Original"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[("chart.set", json!({"chart":1,field:colleague}))]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "chart.set", json!({"chart":1,field:agent})).await;
        check(&result, &before, &fixture.bytes(), &format!("chart.set/{field}"), &mut failures);
        fixture.shutdown().await;
    }
    for layout in 1..=6 {
        for (field, value) in [
            ("legend", json!("none")),
            ("dataLabels", json!(true)),
            ("gridlines", json!(false)),
            ("xTitle", json!("Outside")),
            ("yTitle", json!("Outside")),
        ] {
            let fixture = Fixture::new();
            fixture.colleague(&[("chart.insert", json!({"range":"A1:C4","type":"column","title":"Original"}))]);
            let basis = fixture.basis().await;
            fixture.colleague(&[("chart.set", json!({"chart":1,field:value}))]);
            let before = fixture.bytes();
            let result = fixture.run(basis, "chart.quickLayout", json!({"chart":1,"layout":layout})).await;
            check(&result, &before, &fixture.bytes(), &format!("layout {layout}/{field}"), &mut failures);
            fixture.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn cell_name_style_table_rule_and_view_assignments_hold_the_replaced_values() {
    let mut failures = Vec::new();
    for (command, agent, other_command, other) in [
        ("cell.set", json!({"cell":"B2","input":"42"}), "cell.set", json!({"cell":"B2","input":"99"})),
        ("range.setValues", json!({"range":"B2","values":[[42]]}), "cell.set", json!({"cell":"B2","input":"99"})),
        ("range.fill", json!({"range":"B2:B3","input":"42"}), "cell.set", json!({"cell":"B3","input":"99"})),
        ("edit.clearContents", json!({"range":"B2"}), "cell.set", json!({"cell":"B2","input":"99"})),
        ("format.bold", json!({"range":"B2","on":false}), "format.bold", json!({"range":"B2","on":true})),
        ("format.fontSize", json!({"range":"B2","size":14}), "format.fontSize", json!({"range":"B2","size":24})),
        ("format.color", json!({"range":"B2","color":"#ABCDEF"}), "format.color", json!({"range":"B2","color":"#123456"})),
        ("format.numberFormat", json!({"range":"B2","code":"0.000"}), "format.numberFormat", json!({"range":"B2","code":"0.0"})),
        ("name.define", json!({"name":"Existing","refersTo":"=42"}), "name.define", json!({"name":"Existing","refersTo":"=99"})),
        ("name.delete", json!({"name":"Existing"}), "name.define", json!({"name":"Existing","refersTo":"=99"})),
        ("style.add", json!({"name":"ExistingStyle","fromCell":"B2"}), "style.add", json!({"name":"ExistingStyle","fromCell":"C2"})),
        (
            "table.newStyle",
            json!({"name":"ExistingTableStyle","replace":"ExistingTableStyle","elements":{"wholeTable":{"italic":true}}}),
            "table.newStyle",
            json!({"name":"ExistingTableStyle","replace":"ExistingTableStyle","elements":{"wholeTable":{"bold":true}}}),
        ),
        ("table.bandedRows", json!({"table":"Sales","on":true}), "table.bandedRows", json!({"table":"Sales","on":false})),
        ("table.bandedColumns", json!({"table":"Sales","on":false}), "table.bandedColumns", json!({"table":"Sales","on":true})),
        ("table.firstColumn", json!({"table":"Sales","on":false}), "table.firstColumn", json!({"table":"Sales","on":true})),
        ("table.lastColumn", json!({"table":"Sales","on":false}), "table.lastColumn", json!({"table":"Sales","on":true})),
        ("table.filterButton", json!({"table":"Sales","on":true}), "table.filterButton", json!({"table":"Sales","on":false})),
        (
            "data.validation",
            json!({"range":"B2","type":"whole","formula1":"1","formula2":"10"}),
            "data.validation",
            json!({"range":"B2","type":"whole","formula1":"2","formula2":"20"}),
        ),
        ("conditionalFormat.manageRules", json!({"delete":0}), "conditionalFormat.manageRules", json!({"moveDown":0})),
        ("view.gridlines", json!({"on":true}), "view.gridlines", json!({"on":false})),
        ("view.headings", json!({"on":true}), "view.headings", json!({"on":false})),
        ("view.zoom", json!({"percent":110}), "view.zoom", json!({"percent":150})),
        ("page.orientation", json!({"orientation":"portrait"}), "page.orientation", json!({"orientation":"landscape"})),
        ("page.headerFooter", json!({"header":"Agent"}), "page.headerFooter", json!({"header":"Colleague"})),
        ("page.scaleToFit", json!({"width":2}), "page.scaleToFit", json!({"width":3})),
        ("theme.fonts", json!({"major":"Kition Mono"}), "theme.fonts", json!({"major":"Kition Serif"})),
    ] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("name.define", json!({"name":"Existing","refersTo":"=1"})),
            ("format.fontSize", json!({"range":"C2","size":24})),
            ("style.add", json!({"name":"ExistingStyle","fromCell":"B2"})),
            ("table.newStyle", json!({"name":"ExistingTableStyle","elements":{}})),
            ("table.insert", json!({"range":"A1:C4","name":"Sales","header":true})),
            ("conditionalFormat.apply", json!({"range":"B2","rule":{"type":"blanks"}})),
            ("conditionalFormat.apply", json!({"range":"C3","rule":{"type":"blanks"}})),
        ]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(other_command, other)]);
        let before = fixture.bytes();
        let result = fixture.run(basis, command, agent.clone()).await;
        check(&result, &before, &fixture.bytes(), &format!("{command} {agent}"), &mut failures);
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn custom_view_restore_holds_the_zoom_and_panes_it_replaces() {
    let mut failures = Vec::new();
    for change in ["zoom", "panes", "unrelated"] {
        let fixture = Fixture::new();
        fixture.colleague(&[("view.customViews", json!({"save":"Work"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[match change {
            "zoom" => ("view.zoom", json!({"percent":150})),
            "panes" => ("view.freezeRows", json!({"cell":"A5"})),
            _ => ("cell.set", json!({"cell":"Z20","input":"colleague"})),
        }]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "view.customViews", json!({"show":"Work"})).await;
        if change == "unrelated" {
            if result.is_err() {
                failures.push(format!("custom view/{change}: {result:?}"));
            }
        } else {
            check(&result, &before, &fixture.bytes(), &format!("custom view/{change}"), &mut failures);
        }
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn object_deletion_holds_the_content_it_removes() {
    let mut failures = Vec::new();
    for command in ["chart.delete", "object.delete", "drawing.delete"] {
        let fixture = Fixture::new();
        fixture.colleague(&[("chart.insert", json!({"range":"A1:C4","type":"column","title":"Original"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[("chart.set", json!({"chart":1,"title":"Colleague"}))]);
        let before = fixture.bytes();
        let params = match command {
            "chart.delete" => json!({"chart":1}),
            "object.delete" => json!({"kind":"chart","id":1}),
            _ => json!({"ids":[1]}),
        };
        let result = fixture.run(basis, command, params).await;
        check(&result, &before, &fixture.bytes(), command, &mut failures);
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn ink_conversion_holds_the_fill_it_replaces() {
    // same-field decision: conversion assigns fill from the line color.
    let mut failures = Vec::new();
    for explicit in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[("ink.stroke", json!({"points":[[20,20],[60,60]],"color":"#654321"}))]);
        let basis = fixture.basis().await;
        // XLSX ink stores its rounded pen width in fill's red channel on reopen.
        // A color-only fill edit does not survive that format, so change the width.
        let (observed, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
        fixture.colleague_rewrite(|workbook| workbook.sheet_mut(0).unwrap().shapes[0].outline.width = Some(7 * 12_700));
        let (outside, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
        assert_ne!(
            outside.sheet(0).unwrap().shapes[0].fill,
            observed.sheet(0).unwrap().shapes[0].fill,
            "the outside field must actually change in the saved file"
        );
        let before = fixture.bytes();
        let params = if explicit { json!({"id":1}) } else { json!({}) };
        let result = fixture.run(basis, "ink.toShape", params).await;
        check(&result, &before, &fixture.bytes(), &format!("ink conversion fill/{explicit}"), &mut failures);
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn theme_font_assignment_preserves_the_other_font() {
    // same-field decision: the other font is not assigned.
    for (assigned, retained) in [("major", "minor"), ("minor", "major")] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        fixture.colleague(&[("theme.fonts", json!({retained:"Kition Serif"}))]);
        fixture.run(basis, "theme.fonts", json!({assigned:"Kition Mono"})).await.expect("the other font must stay applicable");
        let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
        assert_eq!(if assigned == "major" { &saved.theme.major_font } else { &saved.theme.minor_font }, "Kition Mono");
        assert_eq!(if retained == "major" { &saved.theme.major_font } else { &saved.theme.minor_font }, "Kition Serif");
        fixture.shutdown().await;
    }
}
