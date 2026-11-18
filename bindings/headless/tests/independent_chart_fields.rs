//! C8-sheet-read: scalar chart changes hold their fields without freezing other chart fields.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use kition_kit::KitDocument;
use serde_json::json;

#[tokio::test]
async fn chart_move_holds_its_frame_and_preserves_outside_title_or_series_edits() {
    let mut wrong = Vec::new();
    for change in ["frame", "title", "series"] {
        let fixture = Fixture::new();
        fixture.colleague(&[("insert.chart", json!({"range":"A1:C4","type":"column","title":"Original"}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[match change {
            "frame" => ("chart.set", json!({"chart":1,"width":333})),
            "title" => ("chart.set", json!({"chart":1,"title":"Colleague"})),
            _ => ("chart.formatSelection", json!({"chart":1,"series":0,"color":"#654321"})),
        }]);
        let before = fixture.bytes();
        let mut prior = kition_sheet_kit::SheetDocument::open("Book.xlsx", &before).unwrap();
        let prior = prior.execute("drawing.describe", json!({"ids":[1]})).unwrap()["objects"][0]["chart"].clone();
        let result = fixture.run(basis, "chart.move", json!({"chart":1,"sheet":"Sheet2","at":"B2"})).await;
        if change == "frame" {
            if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                wrong.push(format!("changed retained frame must conflict: {result:?}"));
            }
            assert_eq!(fixture.bytes(), before);
        } else if result.as_ref().ok().and_then(|value| value["saved"].as_bool()) != Some(true) {
            wrong.push(format!("outside {change} edit must allow moving the latest chart: {result:?}"));
        } else {
            let mut after = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
            after.execute("sheet.activate", json!({"sheet":"Sheet2"})).unwrap();
            let after = after.execute("drawing.describe", json!({"ids":[1]})).unwrap()["objects"][0]["chart"].clone();
            assert_eq!(after[change], prior[change], "the moved chart must preserve the outside {change} edit");
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn chart_shape_fill_on_targets_the_named_chart_instead_of_the_last_chart() {
    let mut wrong = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[
            ("insert.chart", json!({"range":"A1:C4","type":"column","title":"First"})),
            ("insert.chart", json!({"range":"A1:C4","type":"line","title":"Last"})),
        ]);
        let basis = fixture.basis().await;
        fixture.colleague(&[("chart.formatSelection", json!({"chart":if related {1} else {2},"series":0,"color":"#654321"}))]);
        let bytes = fixture.bytes();
        let result =
            fixture.call("run", json!({"basis":basis,"steps":[{"command":"chart.shapeFill","on":["drawing:1"],"params":{"color":"#123456"}}]})).await;
        if related {
            if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                wrong.push(format!("changed named chart must conflict: {result:?}"));
            }
            assert_eq!(fixture.bytes(), bytes, "a conflict must preserve the outside writer's file");
        } else if result.as_ref().ok().and_then(|value| value["saved"].as_bool()) != Some(true) {
            wrong.push(format!("changed other chart must allow the named chart's fill: {result:?}"));
        } else {
            let mut after = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
            let charts = after.execute("drawing.describe", json!({"ids":[1,2]})).unwrap();
            assert_eq!(charts["objects"][0]["chart"]["series"][0]["color"]["rgb"], "#123456");
            assert_eq!(charts["objects"][1]["chart"]["series"][0]["color"]["rgb"], "#654321");
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn chart_move_holds_destination_protection_without_holding_destination_cells() {
    let mut wrong = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[("insert.chart", json!({"range":"A1:C4","type":"column","title":"Original"}))]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[
                ("sheet.activate", json!({"sheet":"Sheet2"})),
                ("review.protectSheet", json!({})),
                ("sheet.activate", json!({"sheet":"Sheet1"})),
            ]);
        } else {
            fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
        }
        let bytes = fixture.bytes();
        let result = fixture.run(basis, "chart.move", json!({"chart":1,"sheet":"Sheet2","at":"B2"})).await;
        if related {
            if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                wrong.push(format!("changed destination protection must conflict before execution: {result:?}"));
            }
            assert_eq!(fixture.bytes(), bytes, "a refused move must preserve the outside writer's file");
        } else if result.as_ref().ok().and_then(|value| value["saved"].as_bool()) != Some(true) {
            wrong.push(format!("unrelated destination cell must allow the move: {result:?}"));
        } else {
            let mut after = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
            assert_eq!(after.execute("cell.get", json!({"sheet":"Sheet2","cell":"A1"})).unwrap()["value"], "unrelated");
            let workbook = after.inspect().unwrap();
            assert!(workbook["workbook"]["sheets"][0]["drawings"].as_array().unwrap().is_empty());
            assert_eq!(workbook["workbook"]["sheets"][1]["drawings"][0]["toolId"], "drawing:1");
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn chart_fields_conflict_only_with_the_fields_or_sources_the_operation_uses() {
    let mut wrong = Vec::new();
    for (command, params, related_command, related_params, unrelated_params) in [
        (
            "chart.addElement",
            json!({"chart":1,"element":"legend","on":false}),
            "chart.delete",
            json!({"chart":1}),
            json!({"chart":1,"title":"Unrelated"}),
        ),
        (
            "chart.addElement",
            json!({"chart":1,"element":"title"}),
            "chart.set",
            json!({"chart":1,"title":"Related"}),
            json!({"chart":1,"legend":"top"}),
        ),
        ("chart.quickLayout", json!({"chart":1,"layout":2}), "chart.delete", json!({"chart":1}), json!({"chart":1,"title":"Unrelated"})),
        ("chart.changeType", json!({"chart":1,"type":"line"}), "chart.delete", json!({"chart":1}), json!({"chart":1,"title":"Unrelated"})),
        (
            "chart.formatSelection",
            json!({"chart":1,"series":0,"color":"#123456"}),
            "chart.formatSelection",
            json!({"chart":1,"series":0,"color":"#654321"}),
            json!({"chart":1,"title":"Unrelated"}),
        ),
        (
            "chart.shapeFill",
            json!({"chart":1,"series":0,"color":"#123456"}),
            "chart.formatSelection",
            json!({"chart":1,"series":0,"color":"#654321"}),
            json!({"chart":1,"title":"Unrelated"}),
        ),
        (
            "chart.changeColors",
            json!({"chart":1,"colors":["#123456"]}),
            "chart.formatSelection",
            json!({"chart":1,"series":0,"color":"#654321"}),
            json!({"chart":1,"title":"Unrelated"}),
        ),
        (
            "chart.selectData",
            json!({"chart":1,"range":"A1:C4"}),
            "cell.set",
            json!({"cell":"B2","input":"77"}),
            json!({"chart":1,"title":"Unrelated"}),
        ),
        ("chart.switchRowColumn", json!({"chart":1}), "cell.set", json!({"cell":"B2","input":"77"}), json!({"chart":1,"title":"Unrelated"})),
        ("chart.set", json!({"chart":1,"title":"Updated"}), "chart.delete", json!({"chart":1}), json!({"chart":1,"legend":"top"})),
    ] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[("insert.chart", json!({"range":"A1:C4","type":"column","title":"Original"}))]);
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[(related_command, related_params.clone())]);
            } else {
                fixture.colleague(&[("chart.set", unrelated_params.clone())]);
            }
            let bytes = fixture.bytes();
            let mut before = kition_sheet_kit::SheetDocument::open("Book.xlsx", &bytes).unwrap();
            let prior = if related {
                serde_json::Value::Null
            } else {
                before.execute("drawing.describe", json!({"ids":[1]})).unwrap()["objects"][0]["chart"].clone()
            };
            let result = fixture.run(basis, command, params.clone()).await;
            let status = match &result {
                Ok(value) => format!("saved={}", value["saved"]),
                Err(error) => format!("error={}", error["error"]["code"]),
            };
            if related {
                if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                    wrong.push(format!("{command} {params}: related field/source must conflict, {status}"));
                }
                if fixture.bytes() != bytes {
                    wrong.push(format!("{command} {params}: conflicting write changed the file"));
                }
            } else if result.as_ref().ok().and_then(|value| value["saved"].as_bool()) != Some(true) {
                wrong.push(format!("{command} {params}: unrelated chart field must apply, {status}"));
            } else {
                let mut after = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
                let chart = after.execute("drawing.describe", json!({"ids":[1]})).unwrap()["objects"][0]["chart"].clone();
                let unrelated = if unrelated_params.get("title").is_some() { "title" } else { "legend" };
                assert_eq!(chart[unrelated], prior[unrelated], "{command} must preserve the outside writer's {unrelated}");
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
