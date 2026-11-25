//! C8-sheet-read: stale object reads are exercised through an actual second file writer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::json;

fn assert_guard(result: Result<serde_json::Value, serde_json::Value>, related: bool) {
    if related {
        assert_eq!(result.as_ref().err().map(|error| &error["error"]["code"]), Some(&json!("conflict")), "{result:?}");
    } else {
        assert!(result.is_ok(), "{result:?}");
    }
}

fn shapes() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":10,"y":10,"w":80,"h":40}})),
        ("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":180,"y":20,"w":80,"h":40}})),
    ]
}

#[tokio::test]
async fn omitted_shape_id_holds_last_shape_instead_of_selected_shape() {
    for command in ["chart.shapeOutline", "chart.shapeFill"] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&shapes());
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[("arrange.bringForward", json!({"kind":"shape","id":1,"toFront":true}))]);
            } else {
                fixture.colleague(&[("cell.set", json!({"cell":"A1","input":"unrelated"}))]);
            }
            let result = fixture.run(basis, command, json!({"kind":"shape","color":"#ABCDEF"})).await;
            assert_guard(result, related);
            fixture.shutdown().await;
        }
    }
}

#[tokio::test]
async fn reconnect_holds_the_geometry_that_gives_the_target_site() {
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&shapes());
        fixture.colleague(&[("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":300,"y":30,"w":80,"h":40}}))]);
        fixture.colleague(&[("drawing.connect", json!({"from":1,"to":2}))]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("drawing.format", json!({"id":3,"geometry":{"preset":"triangle"}}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":"A1","input":"unrelated"}))]);
        }
        let result = fixture.run(basis, "drawing.reconnect", json!({"id":4,"end":"end","to":3,"site":0})).await;
        assert_guard(result, related);
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn hidden_script_metadata_does_not_hold_unrelated_cell_values() {
    for (command, params) in [("script.new", json!({"name":"Work","commands":[]})), ("script.delete", json!({"name":"Existing"}))] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&[("script.new", json!({"name":"Existing","commands":[]}))]);
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[("script.new", json!({"name":"Colleague","commands":[]}))]);
            } else {
                fixture.colleague(&[("cell.set", json!({"cell":"A1","input":"unrelated"}))]);
            }
            let result = fixture.run(basis, command, params.clone()).await;
            assert_guard(result, related);
            fixture.shutdown().await;
        }
    }
}

#[tokio::test]
async fn theme_font_defaults_hold_the_theme_without_holding_cells() {
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("theme.fonts", json!({"major":"Kition Serif","minor":"Kition Sans"}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":"A1","input":"unrelated"}))]);
        }
        let result = fixture.run(basis, "theme.fonts", json!({"major":"Kition Mono"})).await;
        assert_guard(result, related);
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn ink_creation_holds_coordinate_geometry_without_holding_cells() {
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("column.setWidth", json!({"cols":"A:A","width":30}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"cell":"A1","input":"unrelated"}))]);
        }
        let result =
            fixture.run(basis, "drawing.ink", json!({"strokes":[{"points":[[100,100],[140,100]],"color":"#123456","width":2,"tool":"pen"}]})).await;
        assert_guard(result, related);
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn ink_creation_holds_protection_without_holding_another_sheets_cells() {
    let mut wrong = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("sheet.protect", json!({}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
        }
        let bytes = fixture.bytes();
        let result =
            fixture.run(basis, "drawing.ink", json!({"strokes":[{"points":[[100,100],[140,100]],"color":"#123456","width":2,"tool":"pen"}]})).await;
        if related {
            if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                wrong.push(format!("changed protection must conflict before execution: {result:?}"));
            }
            assert_eq!(fixture.bytes(), bytes, "a refused ink write must leave the outside writer's file intact");
        } else if result.as_ref().ok().and_then(|value| value["saved"].as_bool()) != Some(true) {
            wrong.push(format!("unrelated Sheet2 cells must allow ink creation: {result:?}"));
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn checkbox_creation_holds_destination_without_holding_another_sheet() {
    for related in [true, false] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("cell.set", json!({"cell":"B2","input":"TRUE"}))]);
        } else {
            fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
        }
        let result = fixture.run(basis, "checkbox.insert", json!({"range":"B2"})).await;
        assert_guard(result, related);
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn omitted_shape_id_holds_stacking_order_without_holding_another_shapes_fill() {
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&shapes());
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("arrange.bringForward", json!({"kind":"shape","id":1,"toFront":true}))]);
        } else {
            fixture.colleague(&[("drawing.format", json!({"id":1,"fill":{"color":"#123456"}}))]);
        }
        let bytes = fixture.bytes();
        let result = fixture.run(basis, "chart.shapeOutline", json!({"color":"#ABCDEF"})).await;
        assert_guard(result, related);
        if related {
            assert_eq!(fixture.bytes(), bytes, "a guard refusal must leave the outside writer's file intact");
        }
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn ink_creation_holds_group_ids_used_by_global_id_allocation() {
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&shapes());
        let basis = fixture.basis().await;
        if related {
            fixture.colleague(&[("drawing.group", json!({"ids":["1","2"]}))]);
        } else {
            fixture.colleague(&[("drawing.format", json!({"id":1,"fill":{"color":"#123456"}}))]);
        }
        let bytes = fixture.bytes();
        let result =
            fixture.run(basis, "drawing.ink", json!({"strokes":[{"points":[[100,100],[140,100]],"color":"#123456","width":2,"tool":"pen"}]})).await;
        assert_guard(result, related);
        if related {
            assert_eq!(fixture.bytes(), bytes, "a guard refusal must leave the outside writer's file intact");
        }
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn shape_fill_holds_shape_identity_without_holding_the_shapes_text() {
    for explicit in [true, false] {
        for related in [true, false] {
            let fixture = Fixture::new();
            fixture.colleague(&shapes());
            let basis = fixture.basis().await;
            if related {
                fixture.colleague(&[("object.delete", json!({"id":2}))]);
            } else {
                fixture.colleague(&[("drawing.text", json!({"id":2,"text":"unrelated"}))]);
            }
            let mut params = json!({"kind":"shape","color":"#ABCDEF"});
            if explicit {
                params["id"] = json!(2);
            }
            let result = fixture.run(basis, "chart.shapeFill", params).await;
            assert_guard(result, related);
            fixture.shutdown().await;
        }
    }
}
