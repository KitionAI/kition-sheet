//! C8-sheet-2: formula sources include metadata that changes their results.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::{KitDocument, Under};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn has(under: &Under, id: impl Fn(&Value) -> bool, field: &str) -> bool {
    under.premises.iter().any(|p| id(&p.id) && p.fields.as_ref().is_none_or(|fields| fields.iter().any(|name| name == field)))
}

fn sources(formula: &str) -> Under {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B9","input":"9"})).unwrap();
    doc.under("cell.set", &json!({"cell":"E1","input":formula})).unwrap().unwrap()
}

#[test]
fn visibility_sensitive_aggregates_hold_source_row_visibility() {
    for formula in ["=SUBTOTAL(109,B2:B9)", "=AGGREGATE(9,1,B2:B9)"] {
        let under = sources(formula);
        assert!(has(&under, |id| id.as_str().is_some_and(|id| id.starts_with("row:")), "info.hidden"), "{formula}: {under:?}");
        assert!(!under.premises.iter().any(|p| p.fields.is_none()), "bounded aggregate sources must stay precise: {under:?}");
    }
    for formula in ["=SUM(B2:B9)", "=SUBTOTAL(9,B2:B9)", "=AGGREGATE(9,0,B2:B9)"] {
        let under = sources(formula);
        assert!(!has(&under, |id| id.as_str().is_some_and(|id| id.starts_with("row:")), "info.hidden"), "{formula}: {under:?}");
    }
}

#[test]
fn cell_and_sheet_information_functions_hold_the_metadata_they_read() {
    let under = sources("=CELL(\"width\",B7)");
    assert!(has(&under, |id| id == "sheet:1", "props.defaultColWidth"), "{under:?}");
    assert!(has(&under, |id| id.as_str().is_some_and(|id| id.starts_with("col:")), "info.size"), "{under:?}");
    for formula in ["=SHEET()", "=SHEET(\"Sheet1\")", "=SHEETS()"] {
        let under = sources(formula);
        assert!(has(&under, |id| id == "workbook", "~tabs"), "{formula}: {under:?}");
    }
    let under = sources("=CELL(\"sheetname\",B7)");
    assert!(has(&under, |id| id == "sheet:1", "name"), "{under:?}");
}

#[test]
fn a_shared_name_retains_each_callers_metadata_dependencies() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    for (cell, input) in [("B7", "7"), ("D4", "=CELL(\"width\",Source)"), ("P4", "=SUM(Source)")] {
        doc.execute("cell.set", json!({"cell":cell,"input":input})).unwrap();
    }
    doc.execute("formulas.defineName", json!({"name":"Source","refersTo":"=Sheet1!B7"})).unwrap();
    let under = doc.under("cell.set", &json!({"cell":"E1","input":"=D4+P4"})).unwrap().unwrap();
    let width_axes = under.premises.iter().filter(|p| {
        p.id.as_str().is_some_and(|id| id.starts_with("col:"))
            && p.fields.as_ref().is_some_and(|fields| fields.iter().any(|field| field == "info.size"))
    });
    assert_eq!(width_axes.count(), 1, "the name used by CELL must still hold column B's width");
}

#[test]
fn three_dimensional_sources_hold_tab_order() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("home.insertSheet", json!({"name":"Notes","before":1})).unwrap();
    let under = doc.under("cell.set", &json!({"sheet":"Sheet1","cell":"E1","input":"=SUM(Sheet1:Notes!B2:B9)"})).unwrap().unwrap();
    assert!(has(&under, |id| id == "workbook", "~tabs"), "moving a tab changes a three-dimensional source");
}
