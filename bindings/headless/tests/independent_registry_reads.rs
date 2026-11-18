//! C8-sheet-read: table-style registry and formula-view writes avoid unrelated cell holds.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use kition_sheet_engine::core::CellRef;
use serde_json::json;

#[tokio::test]
async fn table_style_writes_hold_selected_registry_inputs_without_holding_cells() {
    let mut wrong = Vec::new();
    for command in ["table.newStyle", "table.deleteStyle", "table.setDefaultStyle"] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("table.newStyle", json!({"name":"OldStyle","default":true,"elements":{"wholeTable":{"bold":true}}})),
                ("insert.table", json!({"range":"A1:C4","name":"Sales","header":true,"style":"OldStyle"})),
            ]);
            let basis = fixture.basis().await;
            fixture.colleague(&[if related {
                ("table.deleteStyle", json!({"name":"OldStyle"}))
            } else {
                ("cell.set", json!({"cell":"Z20","input":"999"}))
            }]);
            let before = fixture.bytes();
            let params = if command == "table.newStyle" {
                json!({"name":"Renamed","replace":"OldStyle","elements":{"wholeTable":{"italic":true}}})
            } else {
                json!({"name":"OldStyle"})
            };
            let result = fixture.run(basis, command, params).await;
            if related {
                if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                    wrong.push(format!("{command}: deleted selected registry entry must conflict: {result:?}"));
                }
                assert_eq!(fixture.bytes(), before);
            } else if result.as_ref().ok().and_then(|output| output["saved"].as_bool()) != Some(true) {
                wrong.push(format!("{command}: unrelated cell must permit registry write: {result:?}"));
            } else {
                let (after, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
                let sheet = after.sheet(0).unwrap();
                assert_eq!(sheet.value(CellRef::new(19, 25)).display(), "999");
                match command {
                    "table.newStyle" => {
                        assert!(after.table_styles.styles.iter().any(|style| style.name == "Renamed"));
                        assert!(!after.table_styles.styles.iter().any(|style| style.name == "OldStyle"));
                        assert_eq!(sheet.tables.first().unwrap().style, "Renamed");
                    }
                    "table.deleteStyle" => {
                        assert!(!after.table_styles.styles.iter().any(|style| style.name == "OldStyle"));
                        assert_ne!(sheet.tables.first().unwrap().style, "OldStyle");
                    }
                    _ => assert_eq!(after.table_styles.default_table.as_deref(), Some("OldStyle")),
                }
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn showing_formulas_holds_its_flag_without_holding_sheet_cells() {
    let mut wrong = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        fixture.colleague(&[if related { ("formulas.showFormulas", json!({"on":true})) } else { ("cell.set", json!({"cell":"Z20","input":"999"})) }]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "formulas.showFormulas", json!({})).await;
        if related {
            if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                wrong.push(format!("changed formula-view flag must conflict: {result:?}"));
            }
            assert_eq!(fixture.bytes(), before);
        } else if result.as_ref().ok().and_then(|output| output["saved"].as_bool()) != Some(true) {
            wrong.push(format!("unrelated cell must permit formula view change: {result:?}"));
        } else {
            let (after, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            let sheet = after.sheet(0).unwrap();
            assert!(sheet.show_formulas);
            assert_eq!(sheet.value(CellRef::new(19, 25)).display(), "999");
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn creating_an_unused_table_style_holds_uniqueness_without_holding_cells() {
    let mut wrong = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        fixture.colleague(&[if related {
            ("table.newStyle", json!({"name":"Fresh","elements":{}}))
        } else {
            ("cell.set", json!({"cell":"Z20","input":"999"}))
        }]);
        let before = fixture.bytes();
        let result = fixture.run(basis, "table.newStyle", json!({"name":"Fresh","elements":{}})).await;
        if related {
            if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                wrong.push(format!("taken creation name must conflict: {result:?}"));
            }
            assert_eq!(fixture.bytes(), before);
        } else if result.as_ref().ok().and_then(|output| output["saved"].as_bool()) != Some(true) {
            wrong.push(format!("unrelated cell must permit creation of unused style: {result:?}"));
        } else {
            let (after, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            assert!(after.table_styles.styles.iter().any(|style| style.name == "Fresh"));
            assert_eq!(after.sheet(0).unwrap().value(CellRef::new(19, 25)).display(), "999");
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
