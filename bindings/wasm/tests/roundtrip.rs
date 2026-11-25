//! Core editing survives save and reopen: each test edits a new workbook only through engine
//! commands (as the editor and the agent do), checks the edit took, saves it as .xlsx, opens the
//! saved bytes again and compares the whole workbook before and after.
//!
//! The comparison is the engine's own workbook model (`common::view`: values, formulas, styles,
//! number formats, column widths and row heights, merges, names, sheets, freeze panes), normalised
//! by `kdm_model::normalize` and without the per-person view state (zoom, active cell, scroll).

mod common;

use common::{run, view};
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

const NAME: &str = "Round trip.xlsx";

fn exec(doc: &mut SheetDocument, command: &str, params: Value) -> Value {
    run(doc, command, params).unwrap_or_else(|error| panic!("{error}"))
}

fn new_workbook() -> SheetDocument {
    SheetDocument::create(NAME, 0, 0).unwrap_or_else(|error| panic!("{error}"))
}

fn cell(doc: &mut SheetDocument, cell: &str) -> Value {
    exec(doc, "cell.get", json!({ "cell": cell }))
}

fn model(doc: &mut SheetDocument) -> Value {
    serde_json::to_value(view(doc).unwrap_or_else(|error| panic!("view: {error}"))).unwrap_or_else(|error| panic!("{error}"))
}

/// Saves `doc` as .xlsx, opens the bytes again and asserts the workbook did not change.
fn assert_survives_save_and_reopen(doc: &mut SheetDocument) {
    let before = model(doc);
    let bytes = doc.save(Some(NAME)).unwrap_or_else(|error| panic!("save: {error}"));
    let mut reopened = SheetDocument::open(NAME, &bytes).unwrap_or_else(|error| panic!("reopen: {error}"));
    let after = model(&mut reopened);
    let mut changes = Vec::new();
    differences(&before, &after, "", &mut changes);
    assert!(changes.is_empty(), "the workbook changed across save and reopen:\n{}", changes.join("\n"));
}

/// Each JSON pointer where `before` and `after` differ, with both values.
fn differences(before: &Value, after: &Value, at: &str, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys().filter(|key| !a.contains_key(*key))) {
                differences(a.get(key).unwrap_or(&Value::Null), b.get(key).unwrap_or(&Value::Null), &format!("{at}/{key}"), out);
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                differences(x, y, &format!("{at}/{index}"), out);
            }
        }
        _ if before != after => out.push(format!("{at}: {before} -> {after}")),
        _ => {}
    }
}

#[test]
fn entered_values_and_formulas_survive_save_and_reopen() {
    let mut doc = new_workbook();
    exec(
        &mut doc,
        "range.setValues",
        json!({ "range": "A1", "values": [
            ["Item", "Qty", "Price", "Total", "Paid"],
            ["Tea", 2, 3.5, "=B2*C2", true],
            ["Coffee", 3, 4.25, "=B3*C3", false],
            ["Sum", "=SUM(B2:B3)", null, "=SUM(D2:D3)", null],
        ] }),
    );
    exec(&mut doc, "cell.set", json!({ "cell": "F1", "input": "Note" }));
    exec(&mut doc, "cell.set", json!({ "cell": "F2", "input": "=IF(E2,\"settled\",\"open\")" }));
    exec(&mut doc, "cell.set", json!({ "cell": "F3", "input": "=CONCAT(A3,\" x\",B3)" }));
    assert_eq!(cell(&mut doc, "D2")["value"], json!(7.0));
    assert_eq!(cell(&mut doc, "D4")["value"], json!(19.75));
    assert_eq!(cell(&mut doc, "B4")["value"], json!(5.0));
    assert_eq!(cell(&mut doc, "F2")["value"], json!("settled"));
    assert_eq!(cell(&mut doc, "F3")["value"], json!("Coffee x3"));
    assert_eq!(cell(&mut doc, "E3")["value"], json!(false));
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn font_fill_and_border_formatting_survives_save_and_reopen() {
    let mut doc = new_workbook();
    exec(
        &mut doc,
        "range.setValues",
        json!({ "range": "A1", "values": [["Bold", "Italic", "Under", "Strike", "Font", "Size", "Red", "Filled", "Boxed"]] }),
    );
    exec(&mut doc, "format.bold", json!({ "range": "A1", "on": true }));
    exec(&mut doc, "format.italic", json!({ "range": "B1", "on": true }));
    exec(&mut doc, "format.underline", json!({ "range": "C1", "on": true, "style": "double" }));
    exec(&mut doc, "format.strikethrough", json!({ "range": "D1", "on": true }));
    exec(&mut doc, "format.fontName", json!({ "range": "E1", "name": "Georgia" }));
    exec(&mut doc, "format.fontSize", json!({ "range": "F1", "size": 18 }));
    exec(&mut doc, "format.color", json!({ "range": "G1", "color": "#C00000" }));
    exec(&mut doc, "format.fillColor", json!({ "range": "H1", "color": "#FFFF00" }));
    exec(&mut doc, "format.borders", json!({ "range": "I1", "preset": "thickOutside" }));
    let font = |doc: &mut SheetDocument, at: &str| cell(doc, at)["style"]["font"].clone();
    assert_eq!(font(&mut doc, "A1")["bold"], json!(true));
    assert_eq!(font(&mut doc, "B1")["italic"], json!(true));
    assert_ne!(font(&mut doc, "C1")["underline"], json!("None"));
    assert_eq!(font(&mut doc, "D1")["strike"], json!(true));
    assert_eq!(font(&mut doc, "E1")["name"], json!("Georgia"));
    assert_eq!(font(&mut doc, "F1")["size"], json!(18.0));
    assert_ne!(font(&mut doc, "G1")["color"], json!("Auto"));
    assert_eq!(cell(&mut doc, "H1")["style"]["fill"]["pattern"], json!("Solid"));
    assert_ne!(cell(&mut doc, "I1")["style"]["borders"]["top"]["style"], json!("None"));
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn alignment_and_merges_survive_save_and_reopen() {
    let mut doc = new_workbook();
    exec(
        &mut doc,
        "range.setValues",
        json!({ "range": "A1", "values": [["Centred title", null, null], ["Right", "Top", "A long text that wraps inside its cell"], ["Indented", "Turned", null]] }),
    );
    exec(&mut doc, "cell.mergeCenter", json!({ "range": "A1:C1" }));
    exec(&mut doc, "format.alignRight", json!({ "range": "A2" }));
    exec(&mut doc, "format.alignTop", json!({ "range": "B2" }));
    exec(&mut doc, "format.wrapText", json!({ "range": "C2", "on": true }));
    exec(&mut doc, "format.increaseIndent", json!({ "range": "A3" }));
    exec(&mut doc, "format.orientation", json!({ "range": "B3", "angle": 45 }));
    let align = |doc: &mut SheetDocument, at: &str| cell(doc, at)["style"]["align"].clone();
    assert!(cell(&mut doc, "A1")["merge"].is_string() || cell(&mut doc, "A1")["merge"].is_object(), "{}", cell(&mut doc, "A1"));
    assert_eq!(align(&mut doc, "A2")["h"], json!("Right"));
    assert_eq!(align(&mut doc, "B2")["v"], json!("Top"));
    assert_eq!(align(&mut doc, "C2")["wrap"], json!(true));
    assert_eq!(align(&mut doc, "A3")["indent"], json!(1));
    assert_eq!(align(&mut doc, "B3")["rotation"], json!(45));
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn number_formats_survive_save_and_reopen() {
    let mut doc = new_workbook();
    exec(&mut doc, "range.setValues", json!({ "range": "A1", "values": [[1234.5, 0.256, 1234567.891, 42.1, 45000]] }));
    exec(&mut doc, "format.accounting", json!({ "range": "A1", "symbol": "$" }));
    exec(&mut doc, "format.percent", json!({ "range": "B1" }));
    exec(&mut doc, "format.comma", json!({ "range": "C1" }));
    exec(&mut doc, "format.numberFormat", json!({ "range": "D1", "format": "0.000" }));
    exec(&mut doc, "format.numberFormat", json!({ "range": "E1", "format": "yyyy-mm-dd" }));
    let text = |doc: &mut SheetDocument, at: &str| cell(doc, at)["text"].clone();
    assert_eq!(text(&mut doc, "B1"), json!("26%"));
    assert_eq!(text(&mut doc, "D1"), json!("42.100"));
    assert_eq!(text(&mut doc, "E1"), json!("2023-03-15"));
    assert!(text(&mut doc, "C1").as_str().unwrap().contains("1,234,567"), "{}", text(&mut doc, "C1"));
    assert!(text(&mut doc, "A1").as_str().unwrap().contains('$'), "{}", text(&mut doc, "A1"));
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn row_and_column_changes_survive_save_and_reopen() {
    let mut doc = new_workbook();
    exec(&mut doc, "range.setValues", json!({ "range": "A1", "values": [["a", "b", "c", "d"], [1, 2, 3, 4], [5, 6, 7, 8]] }));
    exec(&mut doc, "column.setWidth", json!({ "cols": "A:A", "width": 120 }));
    exec(&mut doc, "row.setHeight", json!({ "rows": "1:1", "height": 32 }));
    exec(&mut doc, "row.insert", json!({ "rows": "2:2" }));
    exec(&mut doc, "column.insert", json!({ "cols": "B:B" }));
    exec(&mut doc, "column.hide", json!({ "cols": "E:E" }));
    assert_eq!(cell(&mut doc, "A3")["value"], json!(1.0), "a row was inserted above the first data row");
    assert_eq!(cell(&mut doc, "C1")["value"], json!("b"), "a column was inserted before b");
    assert!(cell(&mut doc, "B2")["value"].is_null());
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn sheet_management_survives_save_and_reopen() {
    let mut doc = new_workbook();
    exec(&mut doc, "range.setValues", json!({ "range": "A1", "values": [["first"]] }));
    exec(&mut doc, "sheet.insert", json!({ "name": "Data" }));
    exec(&mut doc, "range.setValues", json!({ "range": "Data!A1", "values": [["second"]] }));
    exec(&mut doc, "sheet.insert", json!({ "name": "Archive" }));
    exec(&mut doc, "sheet.rename", json!({ "sheet": 0, "name": "Summary" }));
    exec(&mut doc, "sheet.tabColor", json!({ "sheet": "Data", "color": "#FF0000" }));
    exec(&mut doc, "sheet.move", json!({ "sheet": "Archive", "to": 0 }));
    exec(&mut doc, "sheet.hide", json!({ "sheet": "Archive" }));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    let sheets: Vec<(Value, Value)> =
        inspect["sheets"].as_array().unwrap().iter().map(|sheet| (sheet["name"].clone(), sheet["visibility"].clone())).collect();
    assert_eq!(sheets.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(), [json!("Archive"), json!("Summary"), json!("Data")], "{inspect}");
    assert_ne!(sheets[0].1, json!("Visible"), "{inspect}");
    assert!(inspect["sheets"][2]["tabColor"].is_string() || inspect["sheets"][2]["tabColor"].is_object(), "{inspect}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn defined_names_survive_save_and_reopen() {
    let mut doc = new_workbook();
    exec(&mut doc, "range.setValues", json!({ "range": "A1", "values": [[10], [20], [30]] }));
    exec(&mut doc, "name.define", json!({ "name": "Amounts", "refersTo": "=Sheet1!$A$1:$A$3" }));
    exec(&mut doc, "cell.set", json!({ "cell": "B1", "input": "=SUM(Amounts)" }));
    assert_eq!(cell(&mut doc, "B1")["value"], json!(60.0));
    let names = exec(&mut doc, "name.manage", json!({}));
    assert!(names.to_string().contains("Amounts"), "{names}");
    assert_survives_save_and_reopen(&mut doc);
}

#[test]
fn frozen_panes_and_gridlines_survive_save_and_reopen() {
    let mut doc = new_workbook();
    exec(&mut doc, "range.setValues", json!({ "range": "A1", "values": [["Name", "Q1", "Q2"], ["North", 1, 2]] }));
    exec(&mut doc, "view.freezePanes", json!({ "cell": "B2" }));
    exec(&mut doc, "view.gridlines", json!({ "on": false }));
    let inspect = exec(&mut doc, "file.inspect", json!({}));
    assert_eq!(inspect["sheets"][0]["freeze"], json!([1, 1]), "{inspect}");
    assert_survives_save_and_reopen(&mut doc);
}
