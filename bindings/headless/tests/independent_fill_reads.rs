//! C8-sheet-read: bounded constant-source fills hold their copied edge and destinations.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test]
async fn directional_fills_hold_source_cells_without_holding_unrelated_sheet_cells() {
    let mut wrong = Vec::new();
    for (command, range, source, destination) in [
        ("edit.fillDown", "B2:C4", "B2", "B4"),
        ("edit.fillRight", "B2:D3", "B2", "D2"),
        ("edit.fillUp", "B2:C4", "B4", "B2"),
        ("edit.fillLeft", "B2:D3", "D2", "B2"),
        // The handler expands a one-row/one-column forward fill to its neighbour.
        ("edit.fillDown", "B3:C3", "B2", "B3"),
        ("edit.fillRight", "C2:C3", "B2", "C2"),
    ] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[("cell.set", json!({"cell":source,"input":"111"}))]);
            let basis = fixture.basis().await;
            fixture.colleague(&[("cell.set", json!({"cell":if related {source} else {"Z20"},"input":if related {"701"} else {"999"}}))]);
            let before = fixture.bytes();
            let result = fixture.run(basis, command, json!({"range":range})).await;
            if related {
                if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                    wrong.push(format!("{command} {range}: copied source change must conflict: {result:?}"));
                }
                assert_eq!(fixture.bytes(), before, "a refused fill preserves the colleague file");
            } else if result.as_ref().ok().and_then(|output| output["saved"].as_bool()) != Some(true) {
                wrong.push(format!("{command} {range}: unrelated cell must permit fill: {result:?}"));
            } else {
                let mut after = SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
                assert_eq!(after.execute("cell.get", json!({"cell":destination})).unwrap()["value"], 111.0);
                assert_eq!(after.execute("cell.get", json!({"cell":"Z20"})).unwrap()["value"], 999.0);
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn constant_fill_holds_source_style_and_other_destination_row_height_inputs() {
    let mut wrong = Vec::new();
    for read in ["source style", "destination fit"] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("cell.set", json!({"cell":"B2","input":"copied\ntext"})),
                ("format.fontSize", json!({"range":"B2","size":20})),
                ("cell.set", json!({"cell":"H3","input":"height input"})),
            ]);
            let basis = fixture.basis().await;
            let range = if !related {
                "Z20"
            } else if read == "source style" {
                "B2"
            } else {
                "H3"
            };
            fixture.colleague(&[("format.fontSize", json!({"range":range,"size":40}))]);
            let before = fixture.bytes();
            let result = fixture.run(basis, "edit.fillDown", json!({"range":"B2:B4"})).await;
            if related {
                if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                    wrong.push(format!("changed {read} must conflict: {result:?}"));
                }
                assert_eq!(fixture.bytes(), before);
            } else if result.as_ref().ok().and_then(|output| output["saved"].as_bool()) != Some(true) {
                wrong.push(format!("unrelated style change must allow fill checking {read}: {result:?}"));
            } else {
                let mut after = SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
                let copied = after.execute("cell.get", json!({"cell":"B3"})).unwrap();
                assert_eq!(copied["value"], "copied\ntext");
                assert_eq!(copied["style"]["font"]["size"], 20.0);
                assert_eq!(after.execute("cell.get", json!({"cell":"Z20"})).unwrap()["style"]["font"]["size"], 40.0);
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
