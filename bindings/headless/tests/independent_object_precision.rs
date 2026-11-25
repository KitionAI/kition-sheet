//! C8-sheet-read: object helpers hold their stored inputs without freezing unrelated cells.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use kition_kit::KitDocument;
use serde_json::{Value, json};

#[tokio::test]
async fn custom_view_save_holds_the_zoom_value_it_records() {
    let fixture = Fixture::new();
    let basis = fixture.basis().await;
    fixture.colleague_rewrite(|workbook| workbook.sheet_mut(0).unwrap().zoom = 150);
    let before = fixture.bytes();
    let (workbook, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &before).unwrap();
    assert_eq!(workbook.sheet(0).unwrap().zoom, 150, "fixture must save the outside zoom change");
    let result = fixture.run(basis, "view.customViews", json!({"save":"Work"})).await;
    if result.is_ok() {
        let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
        eprintln!("custom view after outside zoom change: {:?}", saved.names.iter().find(|name| name.name == "_sc_custom_views"));
    }
    assert_eq!(result.as_ref().err().and_then(|error| error["error"]["code"].as_str()), Some("conflict"), "{result:?}");
    assert_eq!(fixture.bytes(), before);
    fixture.shutdown().await;
}

#[tokio::test]
async fn ink_conversion_holds_the_chosen_stroke_and_last_ink_default() {
    let mut wrong = Vec::new();
    for explicit in [true, false] {
        for change in ["points", "lastInk", "unrelated"] {
            if explicit && change == "lastInk" {
                continue;
            }
            let fixture = Fixture::new();
            fixture.colleague(&[
                ("ink.stroke", json!({"points":[[20,20],[60,60]],"color":"#123456"})),
                ("shape.insert", json!({"kind":"rectangle","at":"C3"})),
            ]);
            let basis = fixture.basis().await;
            match change {
                "points" => fixture.colleague_rewrite(|workbook| {
                    workbook.sheet_mut(0).unwrap().shapes.first_mut().unwrap().text = "0,0;40,0;40,40;0,40;0,0".into();
                }),
                "lastInk" => fixture.colleague(&[("ink.stroke", json!({"points":[[100,100],[140,140]]}))]),
                _ => fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]),
            }
            let before = fixture.bytes();
            let result = fixture.run(basis, "ink.toShape", if explicit { json!({"id":1}) } else { json!({}) }).await;
            if change == "unrelated" {
                if result.is_err() {
                    wrong.push(format!("explicit={explicit}: unrelated cell must allow ink conversion: {result:?}"));
                } else {
                    let (workbook, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
                    assert_ne!(workbook.sheet(0).unwrap().shapes[0].kind, kition_sheet_engine::model::ShapeKind::Ink);
                    assert_eq!(workbook.sheet(1).unwrap().value(kition_sheet_engine::core::CellRef::default()).display(), "unrelated");
                }
            } else {
                if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                    wrong.push(format!("explicit={explicit}, {change}: changed conversion input must conflict: {result:?}"));
                }
                assert_eq!(fixture.bytes(), before);
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn drawing_transform_holds_the_kind_that_selects_its_supported_operations() {
    let mut wrong = Vec::new();
    for related in [true, false] {
        let fixture = Fixture::new();
        fixture.colleague(&[("drawing.insert", json!({"kind":"shape","geometry":{"preset":"rect"},"frame":{"x":100,"y":100,"w":80,"h":40}}))]);
        let basis = fixture.basis().await;
        if related {
            fixture.colleague_rewrite(|workbook| {
                let shape = workbook.sheet_mut(0).unwrap().shapes.first_mut().unwrap();
                shape.kind = kition_sheet_engine::model::ShapeKind::Ink;
                shape.text = "0,0;80,40".into();
            });
            let (workbook, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            assert_eq!(
                workbook.sheet(0).unwrap().shapes[0].kind,
                kition_sheet_engine::model::ShapeKind::Ink,
                "fixture must preserve the outside kind change"
            );
        } else {
            fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
        }
        let bytes = fixture.bytes();
        let result = fixture.run(basis, "drawing.transform", json!({"id":1,"rotateBy":90})).await;
        if related {
            if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                wrong.push(format!("changed kind must conflict before dispatching its new transform behavior: {result:?}"));
            }
            if fixture.bytes() != bytes {
                wrong.push("changed-kind transform changed the outside writer's file".into());
            }
        } else {
            assert_eq!(
                result.as_ref().ok().and_then(|value| value["saved"].as_bool()),
                Some(true),
                "unrelated cell must allow the valid shape rotation: {result:?}"
            );
        }
        fixture.shutdown().await;
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn object_helper_holds_follow_their_stored_name_theme_or_pane_inputs() {
    let mut wrong = Vec::new();
    let rows: [(&str, Value, Vec<(&str, Value)>, Option<(&str, Value)>); 8] = [
        ("script.record", json!({}), vec![], None),
        ("formula.useInFormula", json!({"name":"Revenue"}), vec![], None),
        ("theme.effects", json!({"name":"Simple"}), vec![], None),
        ("theme.colors", json!({"name":"Meadow"}), vec![], Some(("theme.colors", json!({"name":"Ember"})))),
        (
            "formula.watchWindow",
            json!({"add":"A3"}),
            vec![("formula.watchWindow", json!({"add":"A2"}))],
            Some(("formula.watchWindow", json!({"add":"A4"}))),
        ),
        (
            "view.customViews",
            json!({"save":"Work"}),
            vec![("view.customViews", json!({"save":"Existing"}))],
            Some(("view.customViews", json!({"save":"Another"}))),
        ),
        (
            "view.customViews",
            json!({"show":"Existing"}),
            vec![("view.customViews", json!({"save":"Existing"}))],
            Some(("view.customViews", json!({"delete":"Existing"}))),
        ),
        ("view.split", json!({}), vec![], Some(("view.freezePanes", json!({"cell":"A3"})))),
    ];
    for (command, params, setup, related_call) in rows {
        for related in [true, false] {
            if related && related_call.is_none() {
                continue;
            }
            let fixture = Fixture::new();
            fixture.colleague(&setup);
            let basis = fixture.basis().await;
            if related {
                let (command, params) = related_call.as_ref().unwrap();
                fixture.colleague(&[(*command, params.clone())]);
            } else {
                fixture.colleague(&[("cell.set", json!({"sheet":"Sheet2","cell":"A1","input":"unrelated"}))]);
            }
            let bytes = fixture.bytes();
            let result = fixture.run(basis, command, params.clone()).await;
            if related {
                if result.as_ref().err().and_then(|error| error["error"]["code"].as_str()) != Some("conflict") {
                    wrong.push(format!("{command} {params}: changed input must conflict: {result:?}"));
                }
                assert_eq!(fixture.bytes(), bytes, "{command}: a conflict must preserve the outside writer's file");
            } else if result.is_err() {
                wrong.push(format!("{command} {params}: unrelated cell must apply: {result:?}"));
            } else {
                let mut after = kition_sheet_kit::SheetDocument::open("Book.xlsx", &fixture.bytes()).unwrap();
                assert_eq!(after.execute("cell.get", json!({"sheet":"Sheet2","cell":"A1"})).unwrap()["value"], "unrelated");
            }
            fixture.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn theme_colors_keeps_the_outside_writers_fonts() {
    let fixture = Fixture::new();
    let basis = fixture.basis().await;
    fixture.colleague(&[("theme.fonts", json!({"major":"Kition Serif","minor":"Kition Mono"}))]);
    let result = fixture.run(basis, "theme.colors", json!({"name":"Meadow"})).await;
    assert!(result.is_ok(), "a color-scheme write preserves the outside writer's fonts: {result:?}");
    let (workbook, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
    assert_eq!(workbook.theme.major_font, "Kition Serif");
    assert_eq!(workbook.theme.minor_font, "Kition Mono");
    fixture.shutdown().await;
}
