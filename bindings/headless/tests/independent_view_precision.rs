//! C8-sheet-read: lifecycle and spelling premises follow the values actually used.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn supported_new_workbook_does_not_hold_or_replace_the_old_files_cell_contents() {
    let mut failures = Vec::new();
    for address in ["Sheet1!B2", "Sheet2!Z20"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":address,"input":"colleague"}))]);
        let before = f.bytes();
        let answer = f.run(basis, "file.new", json!({})).await;
        match answer {
            Ok(output) => {
                assert!(output["completed"][0]["result"]["index"].as_u64().is_some_and(|index| index >= 1), "{output}");
                assert_eq!(f.bytes(), before, "a clean new workbook has not been saved over the old file");
            }
            Err(error) => failures.push(format!("{address} is unrelated to a separate new workbook: {error}")),
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_without_a_save_does_not_hold_or_replace_old_cell_contents() {
    // Closing is terminal: completion must not require inspecting a closed workbook.
    let mut failures = Vec::new();
    for address in ["Sheet1!B2", "Sheet2!Z20"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":address,"input":"colleague"}))]);
        let before = f.bytes();
        let answer = f.run(basis, "file.close", json!({"force":true})).await;
        match answer {
            Ok(output) => {
                assert_eq!(output["completed"][0]["result"]["closed"], true, "{output}");
                assert_eq!(f.bytes(), before, "closing without saving keeps the outside writer's file");
            }
            Err(error) => failures.push(format!("{address} is unrelated to a close without saving: {error}")),
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_unoffered_open_keeps_its_exact_disabled_result_after_other_cell_edits() {
    let mut failures = Vec::new();
    for address in ["Sheet1!B2", "Sheet2!Z20"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":address,"input":"colleague"}))]);
        let before = f.bytes();
        let error = f.run(basis, "file.open", json!({"name":"Other.xlsx","base64":""})).await.expect_err("file commands are not offered to agents");
        let message = "step 0 (`file.open`) failed: command disabled: commands that name files are not available to the agent; the host opens and saves documents";
        if error["error"]["code"] != "command-failed"
            || error["error"]["message"] != message
            || error["completed"] != json!([])
            || error["saved"] != false
        {
            failures.push(format!("{address}: file.open must return the exact host-file refusal without running: {error}"));
        }
        assert_eq!(f.bytes(), before);
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_single_cell_spelling_holds_the_named_source_and_allows_another_rows_change() {
    let mut failures = Vec::new();
    for qualified in [false, true] {
        for related in [true, false] {
            let f = Fixture::new();
            let target = if qualified { "Sheet2!C3" } else { "C3" };
            f.colleague(&[("cell.set", json!({"cell":target,"input":"teh cost"}))]);
            let basis = f.basis().await;
            let changed = if related { target } else { "Sheet1!Z20" };
            f.colleague(&[("cell.set", json!({"cell":changed,"input":"colleague"}))]);
            let before = f.bytes();
            let answer = f.run(basis, "spelling.change", json!({"cell":target,"word":"teh","to":"the"})).await;
            if related {
                if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                    failures.push(format!("{target}: the source changed: {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                failures.push(format!("{target}: Z20 is outside the source and fitted row: {error}"));
            } else {
                let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
                assert_eq!(saved.execute("cell.get", json!({"cell":target})).unwrap()["value"], "the cost");
                assert_eq!(saved.execute("cell.get", json!({"cell":"Sheet1!Z20"})).unwrap()["value"], "colleague");
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_spelling_replacement_that_becomes_a_formula_holds_its_precedent() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"cell":"C3","input":"teh"})), ("cell.set", json!({"cell":"E10","input":"5"}))]);
        let basis = f.basis().await;
        let changed = if related { "E10" } else { "Z20" };
        f.colleague(&[("cell.set", json!({"cell":changed,"input":"9"}))]);
        let before = f.bytes();
        let answer = f.run(basis, "spelling.change", json!({"cell":"C3","word":"teh","to":"=E10+1"})).await;
        if related {
            if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                failures.push(format!("the replacement reads changed E10: {answer:?}"));
            } else {
                assert_eq!(f.bytes(), before);
            }
        } else if let Err(error) = answer {
            failures.push(format!("Z20 is unrelated to the composed formula: {error}"));
        } else {
            let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            assert_eq!(saved.execute("cell.get", json!({"cell":"C3"})).unwrap()["value"], 6.0);
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn supported_close_handler_returns_success_before_the_sdk_requests_a_description() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let result = doc.execute_as("file.close", json!({"force":true}), &kition_sheet_engine::Origin::agent("c8-reader")).unwrap();
    assert_eq!(result, json!({"closed":true}));
    assert_eq!(doc.inspect().unwrap_err(), "no workbook open");
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_all_spelling_holds_source_membership_but_allows_other_sheet_inputs_and_unmatched_styles() {
    let mut failures = Vec::new();
    for qualified in [false, true] {
        for change in [
            "source",
            "new source",
            "deleted source",
            "other sheet",
            "unmatched style",
            "unmatched input",
            "new unmatched input",
            "deleted unmatched",
        ] {
            let f = Fixture::new();
            let sheet = if qualified { "Sheet2" } else { "Sheet1" };
            let other = if qualified { "Sheet1" } else { "Sheet2" };
            f.colleague(&[
                ("cell.set", json!({"cell":format!("{sheet}!C3"),"input":"teh cost"})),
                ("cell.set", json!({"cell":format!("{sheet}!D6"),"input":"teh price"})),
                ("cell.set", json!({"cell":format!("{sheet}!A1"),"input":"other"})),
            ]);
            let basis = f.basis().await;
            let related = matches!(change, "source" | "new source" | "deleted source");
            match change {
                "source" => f.colleague(&[("cell.set", json!({"cell":format!("{sheet}!C3"),"input":"colleague"}))]),
                "new source" => f.colleague(&[("cell.set", json!({"cell":format!("{sheet}!M19"),"input":"teh new"}))]),
                "deleted source" => f.colleague(&[("edit.clearAll", json!({"range":format!("{sheet}!C3")}))]),
                "other sheet" => f.colleague(&[("cell.set", json!({"cell":format!("{other}!Z20"),"input":"colleague"}))]),
                "unmatched style" => f.colleague(&[("format.color", json!({"range":format!("{sheet}!A1"),"color":"#123456"}))]),
                "unmatched input" => f.colleague(&[("cell.set", json!({"cell":format!("{sheet}!A1"),"input":"ordinary"}))]),
                "new unmatched input" => f.colleague(&[("cell.set", json!({"cell":format!("{sheet}!M19"),"input":"ordinary"}))]),
                "deleted unmatched" => f.colleague(&[("edit.clearAll", json!({"range":format!("{sheet}!A1")}))]),
                _ => panic!("unknown fixture change"),
            }
            let before = f.bytes();
            let mut params = json!({"word":"teh","to":"the","all":true});
            if qualified {
                params["cell"] = json!("Sheet2!C3");
            }
            let answer = f.run(basis, "spelling.change", params).await;
            if related {
                if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                    failures.push(format!("{sheet} {change}: source scan changed: {answer:?}"));
                }
                assert_eq!(f.bytes(), before);
            } else if let Err(error) = answer {
                failures.push(format!("{sheet} {change}: unrelated input must apply: {error}"));
            } else {
                let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
                for (cell, wanted) in [("C3", "the cost"), ("D6", "the price")] {
                    assert_eq!(saved.execute("cell.get", json!({"cell":format!("{sheet}!{cell}")})).unwrap()["value"], wanted);
                }
                if change == "other sheet" {
                    assert_eq!(saved.execute("cell.get", json!({"cell":format!("{other}!Z20")})).unwrap()["value"], "colleague");
                }
                if matches!(change, "unmatched input" | "new unmatched input") {
                    let cell = if change == "unmatched input" { "A1" } else { "M19" };
                    assert_eq!(saved.execute("cell.get", json!({"cell":format!("{sheet}!{cell}")})).unwrap()["value"], "ordinary");
                }
                if change == "deleted unmatched" {
                    assert!(saved.execute("cell.get", json!({"cell":format!("{sheet}!A1")})).unwrap()["value"].is_null());
                }
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_all_spelling_holds_formula_sources_in_another_sheet() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"cell":"C3","input":"teh"})), ("cell.set", json!({"cell":"Sheet2!E10","input":"5"}))]);
        let basis = f.basis().await;
        let changed = if related { "Sheet2!E10" } else { "Sheet2!Z20" };
        f.colleague(&[("cell.set", json!({"cell":changed,"input":"9"}))]);
        let before = f.bytes();
        let answer = f.run(basis, "spelling.change", json!({"word":"teh","to":"=Sheet2!E10+1","all":true})).await;
        if related {
            if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                failures.push(format!("the replacement reads changed Sheet2!E10: {answer:?}"));
            }
            assert_eq!(f.bytes(), before);
        } else if let Err(error) = answer {
            failures.push(format!("Sheet2!Z20 is unrelated to the replacement formula: {error}"));
        } else {
            let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            assert_eq!(saved.execute("cell.get", json!({"cell":"C3"})).unwrap()["value"], 6.0);
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_all_spelling_preserves_wildcard_matching_and_malformed_formula_fallback() {
    let mut failures = Vec::new();
    for (word, to, wanted) in [("t?h", "the", "teh"), ("teh", "=(", "=(")] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"cell":"C3","input":"teh"}))]);
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":"Sheet2!Z20","input":"colleague"}))]);
        let answer = f.run(basis, "spelling.change", json!({"word":word,"to":to,"all":true})).await;
        match answer {
            Err(error) => failures.push(format!("{word:?} to {to:?}: other-sheet input must apply: {error}")),
            Ok(output) => {
                assert_eq!(output["completed"][0]["result"]["replaced"], 1, "{output}");
                let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
                assert_eq!(saved.execute("cell.get", json!({"cell":"C3"})).unwrap()["value"], wanted);
                assert_eq!(saved.execute("cell.get", json!({"cell":"Sheet2!Z20"})).unwrap()["value"], "colleague");
            }
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_cell_tool_id_spelling_targets_its_named_sheet_and_holds_that_source() {
    let mut failures = Vec::new();
    for change in ["none", "named source", "active sheet"] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"cell":"Sheet1!C3","input":"teh active"})), ("cell.set", json!({"cell":"Sheet2!C3","input":"teh named"}))]);
        let basis = f.basis().await;
        if change == "named source" {
            f.colleague(&[("cell.set", json!({"cell":"Sheet2!C3","input":"colleague named"}))]);
        } else if change == "active sheet" {
            f.colleague(&[("cell.set", json!({"cell":"Sheet1!C3","input":"colleague active"}))]);
        }
        let before = f.bytes();
        let answer = f.run(basis, "spelling.change", json!({"cell":"cell:2!C3","word":"teh","to":"the"})).await;
        if change == "named source" {
            if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                failures.push(format!("cell:2!C3 changed after observation: {answer:?}"));
            }
            assert_eq!(f.bytes(), before);
        } else if let Err(error) = answer {
            failures.push(format!("{change}: valid cell:2!C3 must apply to Sheet2: {error}"));
        } else {
            let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            let named = saved.execute("cell.get", json!({"cell":"Sheet2!C3"})).unwrap()["value"].clone();
            let active = saved.execute("cell.get", json!({"cell":"Sheet1!C3"})).unwrap()["value"].clone();
            let expected_active = if change == "active sheet" { "colleague active" } else { "teh active" };
            if named != "the named" || active != expected_active {
                failures.push(format!("{change}: cell:2!C3 changed wrong sheet: Sheet2={named}, Sheet1={active}"));
            }
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn supported_all_spelling_holds_covering_header_membership_and_allows_unrelated_table_creation() {
    let mut failures = Vec::new();
    for related in [true, false] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"cell":"C3","input":"teh cost"})), ("cell.set", json!({"cell":"D6","input":"teh price"}))]);
        let basis = f.basis().await;
        let range = if related { "C3:D7" } else { "Q15:R17" };
        f.colleague(&[("table.insert", json!({"range":range,"header":true,"name":"ColleagueTable"}))]);
        let before = f.bytes();
        let answer = f.run(basis, "spelling.change", json!({"word":"teh","to":"the","all":true})).await;
        if related {
            if !answer.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
                failures.push(format!("C3 became a table header after observation: {answer:?}"));
            }
            assert_eq!(f.bytes(), before);
        } else if let Err(error) = answer {
            failures.push(format!("Q15:R17 is outside every spelling match: {error}"));
        } else {
            let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            for (cell, wanted) in [("C3", "the cost"), ("D6", "the price")] {
                assert_eq!(saved.execute("cell.get", json!({"cell":cell})).unwrap()["value"], wanted);
            }
            let (workbook, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
            assert!(workbook.sheets[0].tables.iter().any(|table| table.name == "ColleagueTable" && table.range.a1() == "Q15:R17"));
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
