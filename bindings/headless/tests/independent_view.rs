//! C8-sheet-read: independently rewritten files, derived from blind view rows.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn active_only_property_holds_the_sheet_the_handler_actually_writes() {
    for (command, params, changed) in [
        ("view.gridlines", json!({}), json!({"on":false})),
        ("page.orientation", json!({"orientation":"landscape"}), json!({"orientation":"landscape"})),
        ("page.headerFooter", json!({"header":"agent"}), json!({"header":"colleague"})),
    ] {
        for related in [true, false] {
            let f = Fixture::new();
            let basis = f.basis().await;
            let mut change = changed.clone();
            if !related {
                f.colleague(&[
                    ("sheet.activate", json!({"sheet":"Sheet2"})),
                    (command, change.take()),
                    ("sheet.activate", json!({"sheet":"Sheet1"})),
                ]);
            } else {
                f.colleague(&[(command, change)]);
            }
            let before = f.bytes();
            let result = f.run(basis, command, params.clone()).await;
            if related {
                let error = result.expect_err(command);
                assert!(error.to_string().contains("conflict"), "{command}: {error}");
                assert_eq!(f.bytes(), before);
            } else {
                result.unwrap_or_else(|e| panic!("{command} unrelated: {e}"));
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn qualified_print_area_holds_the_sheet_selected_by_agent_dispatch() {
    for related in [true, false] {
        let f = Fixture::new();
        f.colleague(&[("range.setValues", json!({"range":"Sheet2!A1","values":vec![vec![0; 4]; 12]}))]);
        let basis = f.basis().await;
        let change = ("page.printArea", json!({"range":"A1:B2"}));
        if related {
            f.colleague(&[("sheet.activate", json!({"sheet":"Sheet2"})), change, ("sheet.activate", json!({"sheet":"Sheet1"}))]);
        } else {
            f.colleague(&[change]);
        }
        let before = f.bytes();
        let result = f.run(basis, "page.printArea", json!({"range":"Sheet2!C3:D4"})).await;
        if related {
            let error = result.expect_err("the named sheet print area changed");
            assert!(error.to_string().contains("conflict"), "{error}");
            assert_eq!(f.bytes(), before);
        } else {
            result.unwrap_or_else(|error| panic!("the other sheet must not conflict: {error}"));
            let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            let active = saved.execute("page.pageSetup", json!({"sheet":"Sheet1"})).unwrap();
            let other = saved.execute("page.pageSetup", json!({"sheet":"Sheet2"})).unwrap();
            assert_eq!(active["print_area"], json!({"start":{"row":0,"col":0},"end":{"row":1,"col":1}}));
            assert_eq!(other["print_area"], json!({"start":{"row":2,"col":2},"end":{"row":3,"col":3}}));
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn view_coordinate_parsing_holds_only_the_positions_the_handler_uses() {
    for (command, params, insertion) in
        [("view.freezeRows", json!({"cell":"D8:E9"}), "9:9"), ("page.printTitles", json!({"rows":"Sheet2!3:4"}), "2:2")]
    {
        let f = Fixture::new();
        if command == "page.printTitles" {
            f.colleague(&[
                ("sheet.activate", json!({"sheet":"Sheet2"})),
                ("page.printTitles", json!({"rows":"2:3"})),
                ("sheet.activate", json!({"sheet":"Sheet1"})),
            ]);
        }
        let basis = f.basis().await;
        f.colleague(&[("row.insert", json!({"rows":insertion}))]);
        let mut step = json!({"command":command,"params":params});
        if command == "view.freezeRows" {
            step["on"] = json!(["Sheet1!C5"]);
        }
        f.call("run", json!({"basis":basis,"steps":[step]}))
            .await
            .unwrap_or_else(|error| panic!("{command}: ignored coordinates must not conflict: {error}"));
        let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
        if command == "view.freezeRows" {
            assert_eq!(saved.inspect().unwrap()["freeze"], json!([4, 0]));
        } else {
            let print = saved.execute("page.pageSetup", json!({"sheet":"Sheet2"})).unwrap();
            assert!(print["title_rows"].is_null());
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn zero_scroll_freeze_origin_does_not_read_leading_line_geometry() {
    for explicit_top in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("row.hide", json!({"rows":"1:1"}))]);
        let params = if explicit_top { json!({"cell":"C5","top":"A1"}) } else { json!({"cell":"C5"}) };
        let result = f.run(basis, "view.freezeRows", params).await;
        result.unwrap_or_else(|error| panic!("zero-scroll or explicit top does not read line visibility: {error}"));
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn default_freeze_origin_refuses_a_changed_persisted_origin() {
    for (related, explicit_top) in [(true, false), (false, false), (true, true)] {
        let f = Fixture::new();
        f.colleague(&[("view.freezeRows", json!({"cell":"A5","top":"A1"}))]);
        let basis = f.basis().await;
        if related {
            f.colleague(&[("view.freezeRows", json!({"cell":"A6","top":"A2"}))]);
        } else {
            f.colleague(&[("cell.set", json!({"cell":"Z20","input":"unrelated"}))]);
        }
        let before = f.bytes();
        let params = if explicit_top { json!({"cell":"C8","top":"A1"}) } else { json!({"cell":"C8"}) };
        let result = f.run(basis, "view.freezeRows", params).await;
        // same-field decision: an explicit top still replaces the saved origin.
        if related {
            let error = result.expect_err("changing only the frozen view origin changes the default top");
            assert_eq!(error["error"]["code"], "conflict");
            assert!(error["conflicts"].to_string().contains("changed"), "{error}");
            assert_eq!(f.bytes(), before);
        } else {
            result.unwrap_or_else(|error| panic!("unread origin or unrelated cell must not conflict: {error}"));
            let saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            assert_eq!(saved.inspect().unwrap()["freeze"], json!([7, 0]));
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn positional_view_writes_refuse_shifted_addresses_and_allow_other_cells() {
    for (command, params) in [
        ("view.freezeRows", json!({"cell":"C5"})),
        ("page.printArea", json!({"range":"C3:D4"})),
        ("page.printTitles", json!({"rows":"3:4"})),
        ("page.breaks", json!({"cell":"C5"})),
    ] {
        for related in [true, false] {
            let f = Fixture::new();
            let basis = f.basis().await;
            if related {
                f.colleague(&[("row.insert", json!({"rows":"2:2"}))]);
            } else {
                f.colleague(&[("cell.set", json!({"cell":"Z20","input":"unrelated"}))]);
            }
            let before = f.bytes();
            let result = f.run(basis, command, params.clone()).await;
            if related {
                let error = result.expect_err(command);
                assert!(error.to_string().contains("conflict"), "{command}: {error}");
                assert_eq!(f.bytes(), before);
            } else {
                result.unwrap_or_else(|e| panic!("{command} unrelated: {e}"));
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn view_addresses_refuse_a_new_line_at_the_address() {
    for rows in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        let (insert, key, at, command) =
            if rows { ("row.insert", "rows", "5:5", "view.freezeRows") } else { ("column.insert", "cols", "C:C", "view.freezeColumns") };
        f.colleague(&[(insert, json!({(key):at}))]);
        let before = f.bytes();
        let error = f.run(basis, command, json!({"cell":"C5"})).await.expect_err("the address now names a newly inserted line");
        assert!(error.to_string().contains("conflict"), "{error}");
        assert_eq!(f.bytes(), before);
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn view_addresses_allow_a_new_line_after_the_address() {
    for rows in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        let (insert, key, at, command, freeze) = if rows {
            ("row.insert", "rows", "6:6", "view.freezeRows", json!([4, 0]))
        } else {
            ("column.insert", "cols", "D:D", "view.freezeColumns", json!([0, 2]))
        };
        f.colleague(&[(insert, json!({(key):at}))]);
        f.run(basis, command, json!({"cell":"C5"})).await.unwrap_or_else(|error| panic!("the line after the address must not conflict: {error}"));
        let saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
        assert_eq!(saved.inspect().unwrap()["freeze"], freeze);
        f.shutdown().await;
    }
}

#[tokio::test]
async fn fit_width_holds_fit_settings_without_holding_preserved_scale() {
    for related in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        if related {
            f.colleague(&[("page.scaleToFit", json!({"width":3}))]);
        } else {
            f.colleague(&[("page.pageSetup", json!({"scale":90}))]);
        }
        let before = f.bytes();
        let answer = f.run(basis, "page.scaleToFit", json!({"width":2})).await;
        if related {
            assert_eq!(answer.as_ref().err().map(|error| &error["error"]["code"]), Some(&json!("conflict")), "{answer:?}");
            assert_eq!(f.bytes(), before);
        } else {
            answer.expect("changing preserved scale is unrelated to setting fit width");
            let reopened = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            let workbook: serde_json::Value = serde_json::from_slice(&reopened.save(Some("view.json")).unwrap()).unwrap();
            assert_eq!(workbook["sheets"][0]["print"]["scale"], 90);
            assert_eq!(workbook["sheets"][0]["print"]["fit_to"], json!([2, 0]));
        }
        f.shutdown().await;
    }
}
