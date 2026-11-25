//! C8-sheet-same-field: same-field decision protects replaced values.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use kition_kit::KitDocument;
use serde_json::json;

#[tokio::test]
async fn replaced_chart_fields_and_preserved_defaults_refuse_outside_changes() {
    let mut wrong = Vec::new();
    for (command, params, other_command, other_params, required) in [
        ("chart.addElement", json!({"chart":1,"element":"legend","on":false}), "chart.set", json!({"chart":1,"legend":"top"}), true),
        ("chart.addElement", json!({"chart":1,"element":"dataLabels","on":false}), "chart.set", json!({"chart":1,"dataLabels":true}), true),
        ("chart.addElement", json!({"chart":1,"element":"gridlines","on":true}), "chart.set", json!({"chart":1,"gridlines":false}), true),
        ("chart.addElement", json!({"chart":1,"element":"title","on":false}), "chart.set", json!({"chart":1,"title":"Outside"}), true),
        ("chart.addElement", json!({"chart":1,"element":"axisTitles","on":false}), "chart.set", json!({"chart":1,"xTitle":"Outside"}), true),
        ("chart.quickLayout", json!({"chart":1,"layout":2}), "chart.set", json!({"chart":1,"legend":"none"}), true),
        ("chart.changeType", json!({"chart":1,"type":"line"}), "chart.set", json!({"chart":1,"type":"area"}), true),
        ("chart.selectData", json!({"chart":1,"range":"A1:C4"}), "chart.selectData", json!({"chart":1,"range":"A1:B4"}), true),
        ("chart.set", json!({"chart":1,"title":"Updated"}), "chart.set", json!({"chart":1,"title":"Outside"}), true),
        ("chart.set", json!({"chart":1,"legend":"bottom"}), "chart.set", json!({"chart":1,"legend":"none"}), true),
        ("chart.addElement", json!({"chart":1,"element":"title"}), "chart.set", json!({"chart":1,"title":"Outside"}), true),
        ("chart.addElement", json!({"chart":1,"element":"axisTitles"}), "chart.set", json!({"chart":1,"xTitle":"Outside"}), true),
        ("chart.quickLayout", json!({"chart":1,"layout":5}), "chart.set", json!({"chart":1,"xTitle":"Outside"}), true),
        ("chart.selectData", json!({"chart":1,"range":"A1:C4"}), "cell.set", json!({"cell":"B2","input":"77"}), true),
    ] {
        let fixture = Fixture::new();
        fixture.colleague(&[("chart.insert", json!({"range":"A1:C4","type":"column","title":"Original"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(other_command, other_params.clone())]);
        let outside = fixture.bytes();
        let result = fixture.run(basis, command, params.clone()).await;
        // same-field decision: replacing a value holds the observed value.
        if required {
            if !result.as_ref().is_err_and(|error| error["error"]["code"] == "conflict" && error["conflicts"].to_string().contains("changed")) {
                wrong.push(format!("{command} {params}: changed read must refuse: {result:?}"));
            }
            match &result {
                Err(_) => assert_eq!(fixture.bytes(), outside, "refusal must preserve the outside file"),
                Ok(_) if fixture.bytes() != outside => wrong.push(format!("WRONG_WRITE {command} {params}: overwrote colleague value")),
                Ok(_) => {}
            }
        } else if let Err(error) = result {
            wrong.push(format!("{command} {params}: replaced field must apply: {}", error["error"]["code"]));
        } else {
            let mut expected = kition_sheet_kit::SheetDocument::open("Book.xlsx", &outside).unwrap();
            expected.execute(command, params.clone()).unwrap();
            let mut expected = kition_sheet_kit::SheetDocument::open("Book.xlsx", &expected.save(None).unwrap()).unwrap();
            let expected = expected.execute("drawing.describe", json!({"ids":[1]})).unwrap();
            let mut actual = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
            let actual = actual.execute("drawing.describe", json!({"ids":[1]})).unwrap();
            assert_eq!(actual["objects"][0]["chart"], expected["objects"][0]["chart"], "{command} {params}");
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn shape_colors_replace_the_previous_color_but_still_name_the_observed_shape() {
    let mut wrong = Vec::new();
    for command in ["chart.shapeFill", "chart.shapeOutline"] {
        for explicit in [true, false] {
            for change in ["identity", "same color", "other color", "other shape"] {
                let fixture = Fixture::new();
                fixture.colleague(&[("shape.insert", json!({"at":"B2"})), ("shape.insert", json!({"at":"F5"}))]);
                let basis = fixture.basis().await;
                if change == "identity" && explicit {
                    fixture.colleague(&[("object.delete", json!({"id":2}))]);
                } else if change == "identity" {
                    fixture.colleague(&[("arrange.bringForward", json!({"kind":"shape","id":1,"toFront":true}))]);
                } else {
                    let other_command = if change == "other color" {
                        if command == "chart.shapeFill" { "chart.shapeOutline" } else { "chart.shapeFill" }
                    } else {
                        command
                    };
                    fixture.colleague(&[(other_command, json!({"kind":"shape","id":if change == "other shape" {1} else {2},"color":"#123456"}))]);
                }
                let outside = fixture.bytes();
                let mut params = json!({"kind":"shape","color":"#ABCDEF"});
                if explicit {
                    params["id"] = json!(2);
                }
                let result = fixture.run(basis, command, params).await;
                // same-field decision; another property/shape stays applicable.
                if change == "identity" || change == "same color" {
                    if !result.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                        wrong.push(format!("{command}/{explicit}: changed shape identity must refuse: {result:?}"));
                    }
                    if change == "same color" && !result.as_ref().is_err_and(|error| error["conflicts"].to_string().contains("changed")) {
                        wrong.push(format!("{command}/{explicit}: expected changed"));
                    }
                    match &result {
                        Err(_) => assert_eq!(fixture.bytes(), outside, "refusal must preserve the outside color"),
                        Ok(_) if fixture.bytes() != outside => wrong.push(format!("WRONG_WRITE {command}/{explicit}: overwrote colleague color")),
                        Ok(_) => {}
                    }
                } else if result.is_err() {
                    wrong.push(format!("{command}/{explicit}: {change} must apply"));
                } else {
                    let (book, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
                    let shape = book.sheet(0).unwrap().shapes.iter().find(|shape| shape.id == 2).unwrap();
                    let color = if command == "chart.shapeFill" { shape.fill } else { shape.line };
                    assert_eq!(color, kition_sheet_engine::model::Color::rgb(0xAB, 0xCD, 0xEF));
                }
                fixture.shutdown().await;
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
