//! Checkbox cells (Insert › Checkbox, module M8, cases INSC-C50 to INSC-C330): what inserting one
//! does to a cell, a range, a merged area and an array formula; ticking and clearing; TRUE and
//! FALSE typed or replaced in; the checkbox as the cell's content (Clear Contents takes it, Clear
//! Formats, the format painter and Paste Special › Formats leave it, copy and cut carry it,
//! inserted lines and cells merged away don't get it); sort, remove duplicates, links refused;
//! saved and reopened. And a table's header row with pictures (TS-C76).

use kition_sheet_engine::Session;
use kition_sheet_engine::core::CellRef;
use kition_sheet_engine::io::{open_bytes, save_bytes};
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn get(s: &mut Session, cell: &str) -> Json {
    run(s, "cell.get", json!({"cell": cell}))
}

fn value(s: &mut Session, cell: &str) -> Json {
    get(s, cell)["value"].clone()
}

/// Whether `cell` shows a checkbox.
fn boxed(s: &Session, cell: &str) -> bool {
    let d = s.doc().unwrap();
    kition_sheet_engine::cmd::extra::is_checkbox(&d.wb, d.wb.active_sheet, CellRef::parse(cell).unwrap())
}

/// Whether `cell`'s style carries a checkbox (whatever it holds).
fn styled(s: &Session, cell: &str) -> bool {
    let d = s.doc().unwrap();
    let sheet = d.wb.active().unwrap();
    d.wb.styles.get(sheet.style_id(CellRef::parse(cell).unwrap())).checkbox
}

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

#[test]
fn inserting_replaces_what_cells_hold_and_keeps_true_and_false() {
    let mut s = session();
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "已完成"}));
    run(&mut s, "cell.set", json!({"cell": "A2", "input": "=1+1"}));
    run(&mut s, "cell.set", json!({"cell": "A3", "input": "TRUE"}));
    run(&mut s, "image.insert", json!({"at": "A4", "base64": PNG, "mime": "image/png", "placement": "cell"}));
    run(&mut s, "checkbox.insert", json!({"range": "A1:A5"}));
    for cell in ["A1", "A2", "A3", "A4", "A5"] {
        assert!(boxed(&s, cell), "{cell}");
    }
    assert_eq!([value(&mut s, "A1"), value(&mut s, "A2"), value(&mut s, "A3")], [json!(false), json!(false), json!(true)]);
    assert_eq!(get(&mut s, "A2")["formula"], Json::Null, "the formula goes");
    assert_eq!(get(&mut s, "A4")["picture"], Json::Null, "the picture goes");
    assert_eq!(get(&mut s, "A1")["style"]["align"]["h"], json!("Center"));
    assert!(!boxed(&s, "B1") && !boxed(&s, "A6"), "only the range");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(value(&mut s, "A1"), json!("已完成"), "one undo step");
    assert!(!styled(&s, "A1"));
}

#[test]
fn a_merged_area_takes_one_and_array_formulas_refuse() {
    let mut s = session();
    run(&mut s, "cell.merge", json!({"range": "A1:B2"}));
    run(&mut s, "checkbox.insert", json!({"range": "A1:B2"}));
    assert!(boxed(&s, "A1"));
    assert!(!styled(&s, "B2"), "the merged area's other cells stay as they were");
    assert_eq!(get(&mut s, "A1")["merge"], json!("A1:B2"));
    run(&mut s, "range.setValues", json!({"range": "D2", "values": [[1], [2]]}));
    run(&mut s, "select.set", json!({"range": "C2:C3"}));
    run(&mut s, "cell.set", json!({"cell": "C2", "input": "=D2:D3*2", "array": true}));
    for range in ["C2", "C3", "C1:C3"] {
        let error = s.execute("checkbox.insert", json!({"range": range})).unwrap_err().to_string();
        assert!(error.contains("part of an array"), "{range}: {error}");
    }
    assert_eq!(value(&mut s, "C3"), json!(4.0));
    run(&mut s, "checkbox.insert", json!({"range": "C1"}));
    assert!(boxed(&s, "C1"), "next to the array");
}

#[test]
fn ticking_typing_and_replacing() {
    let mut s = session();
    run(&mut s, "checkbox.insert", json!({"range": "A1:A3"}));
    run(&mut s, "cell.set", json!({"cell": "B1", "input": "TRUE"}));
    assert_eq!(run(&mut s, "cell.toggleCheckbox", json!({"cell": "A2"}))["toggled"], json!(1));
    assert_eq!([value(&mut s, "A1"), value(&mut s, "A2")], [json!(false), json!(true)]);
    // The selection: its checkboxes toggle, other cells are left alone.
    run(&mut s, "select.set", json!({"range": "A1:B1"}));
    run(&mut s, "cell.toggleCheckbox", json!({}));
    assert_eq!([value(&mut s, "A1"), value(&mut s, "B1")], [json!(true), json!(true)]);
    assert!(!boxed(&s, "B1"));
    // TRUE and FALSE in any case keep the checkbox; anything else replaces it.
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "false"}));
    assert!(boxed(&s, "A1"));
    assert_eq!(value(&mut s, "A1"), json!(false));
    run(&mut s, "edit.replace", json!({"what": "false", "with": "true", "all": true}));
    assert_eq!(value(&mut s, "A1"), json!(true));
    assert!(boxed(&s, "A1"));
    run(&mut s, "format.numberFormat", json!({"range": "A3", "format": "Text"}));
    run(&mut s, "cell.set", json!({"cell": "A3", "input": "TRUE"}));
    assert!(boxed(&s, "A3"), "a Text format keeps it a checkbox (INSC-C71, INSC-C281)");
    run(&mut s, "cell.set", json!({"cell": "A3", "input": "已完成"}));
    assert!(!styled(&s, "A3"));
    assert_eq!(value(&mut s, "A3"), json!("已完成"));
}

#[test]
fn a_checkbox_is_content_not_format() {
    let mut s = session();
    run(&mut s, "checkbox.insert", json!({"range": "A1:A4"}));
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "TRUE"}));
    run(&mut s, "format.fillColor", json!({"range": "A1:A4", "color": "#FFFF00"}));
    // Clear Formats leaves it; Clear Contents and Clear All take it.
    run(&mut s, "edit.clearFormats", json!({"range": "A1"}));
    assert!(boxed(&s, "A1"));
    assert_eq!(value(&mut s, "A1"), json!(true));
    assert_eq!(get(&mut s, "A1")["style"]["fill"]["pattern"], json!("None"));
    run(&mut s, "edit.clearContents", json!({"range": "A2"}));
    assert!(!styled(&s, "A2"));
    assert_eq!(value(&mut s, "A2"), Json::Null);
    assert_eq!(get(&mut s, "A2")["style"]["fill"]["pattern"], json!("Solid"), "the fill stays");
    run(&mut s, "edit.clearAll", json!({"range": "A3"}));
    assert!(!styled(&s, "A3"));
    // Paste Special › Formats: the target keeps its own (none brought, none taken).
    run(&mut s, "edit.copy", json!({"range": "A1"}));
    run(&mut s, "select.set", json!({"range": "C1"}));
    run(&mut s, "edit.pasteSpecial", json!({"what": "formats"}));
    assert!(!styled(&s, "C1"));
    run(&mut s, "cell.set", json!({"cell": "D1", "input": "x"}));
    run(&mut s, "format.bold", json!({"range": "D1", "on": true}));
    run(&mut s, "edit.copy", json!({"range": "D1"}));
    run(&mut s, "select.set", json!({"range": "A4"}));
    run(&mut s, "edit.pasteSpecial", json!({"what": "formats"}));
    assert!(boxed(&s, "A4"));
    assert_eq!(get(&mut s, "A4")["style"]["font"]["bold"], json!(true));
    // The format painter: the same both ways.
    run(&mut s, "select.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({}));
    run(&mut s, "edit.formatPainter", json!({"apply": "E1"}));
    assert!(!styled(&s, "E1"));
    run(&mut s, "select.set", json!({"range": "F9"}));
    run(&mut s, "edit.formatPainter", json!({}));
    run(&mut s, "edit.formatPainter", json!({"apply": "A1"}));
    assert!(boxed(&s, "A1"));
    assert_eq!(value(&mut s, "A1"), json!(true));
    // Copy and cut carry it.
    run(&mut s, "edit.copy", json!({"range": "A1"}));
    run(&mut s, "edit.paste", json!({"at": "G1"}));
    assert!(boxed(&s, "G1"));
    assert_eq!(value(&mut s, "G1"), json!(true));
    run(&mut s, "edit.cut", json!({"range": "G1"}));
    run(&mut s, "edit.paste", json!({"at": "H1"}));
    assert!(boxed(&s, "H1") && !styled(&s, "G1"));
    // Paste Special › Values: TRUE, no checkbox.
    run(&mut s, "edit.copy", json!({"range": "A1"}));
    run(&mut s, "select.set", json!({"range": "I1"}));
    run(&mut s, "edit.pasteSpecial", json!({"what": "values"}));
    assert_eq!(value(&mut s, "I1"), json!(true));
    assert!(!styled(&s, "I1"));
}

#[test]
fn merged_away_cells_and_inserted_lines_get_none() {
    let mut s = session();
    run(&mut s, "checkbox.insert", json!({"range": "A1:B1"}));
    run(&mut s, "cell.merge", json!({"range": "A1:B1"}));
    run(&mut s, "cell.unmerge", json!({"range": "A1:B1"}));
    assert!(boxed(&s, "A1"));
    assert!(!styled(&s, "B1"), "INSC-C272");
    run(&mut s, "checkbox.insert", json!({"range": "D1:D3"}));
    run(&mut s, "row.insert", json!({"rows": "2:2"}));
    run(&mut s, "row.insert", json!({"rows": "5", "below": true}));
    run(&mut s, "column.insert", json!({"cols": "E"}));
    assert!(!styled(&s, "D2"), "the new row takes the format above, not its checkbox (INSC-C284)");
    assert!(boxed(&s, "D3") && boxed(&s, "D4"));
    assert!(!styled(&s, "E3"), "nor the new column");
    // Fill copies it, as Excel's does.
    run(&mut s, "select.set", json!({"range": "A1:A3"}));
    run(&mut s, "edit.fillDown", json!({}));
    assert!(boxed(&s, "A3"));
}

#[test]
fn sort_duplicates_and_links() {
    let mut s = session();
    run(&mut s, "checkbox.insert", json!({"range": "A1:A4"}));
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "TRUE"}));
    run(&mut s, "cell.set", json!({"cell": "A3", "input": "TRUE"}));
    run(&mut s, "range.setValues", json!({"range": "B1", "values": [["一"], ["二"], ["三"], ["四"]]}));
    run(&mut s, "data.sortAscending", json!({"range": "A1:B4", "column": "A"}));
    assert_eq!([value(&mut s, "B1"), value(&mut s, "B3")], [json!("二"), json!("一")]);
    assert!(boxed(&s, "A1") && boxed(&s, "A4"));
    run(&mut s, "data.removeDuplicates", json!({"range": "A1:A4", "header": false}));
    assert_eq!([value(&mut s, "A1"), value(&mut s, "A2")], [json!(false), json!(true)]);
    assert!(!boxed(&s, "A3"), "INSC-C277: two left");
    // A link is refused on a checkbox; Insert Link and Insert Function are greyed there.
    run(&mut s, "select.set", json!({"range": "A1"}));
    assert!(s.commands().iter().any(|c| c.id == "link.insert" && !c.enabled));
    assert!(s.commands().iter().any(|c| c.id == "formula.insertFunction" && !c.enabled));
    run(&mut s, "select.set", json!({"range": "C1"}));
    assert!(s.execute("link.insert", json!({"cell": "A1", "target": "https://example.com"})).is_err());
    run(&mut s, "link.insert", json!({"cell": "C1", "target": "https://example.com"}));
}

#[test]
fn checkboxes_are_saved_and_reopened() {
    let mut s = session();
    run(&mut s, "checkbox.insert", json!({"range": "A1:A3"}));
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "TRUE"}));
    let bytes = save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
    let (wb, _) = open_bytes("Book.xlsx", &bytes).unwrap();
    let mut back = session();
    run(&mut back, "file.open", json!({"name": "Book.xlsx", "base64": kition_sheet_engine::io::base64_encode(&bytes)}));
    assert!(boxed(&back, "A1") && boxed(&back, "A2") && boxed(&back, "A3"));
    assert_eq!(value(&mut back, "A1"), json!(true));
    assert!(wb.styles.iter().any(|(_, st)| st.checkbox));
}

/// TS-C76: Convert to a table with pictures in the header row is refused until they may go; then
/// they go and the header is named.
#[test]
fn a_tables_header_row_holds_no_pictures() {
    let mut s = session();
    run(&mut s, "range.setValues", json!({"range": "A1", "values": [["", "b"], [1, 2], [3, 4]]}));
    run(&mut s, "image.insert", json!({"at": "A1", "base64": PNG, "mime": "image/png", "placement": "cell"}));
    let refused =
        s.execute("table.formatAs", json!({"range": "A1:B3", "mode": "table", "header": true, "columnPrefix": "列"})).unwrap_err().to_string();
    assert!(refused.contains("header row can't hold pictures"), "{refused}");
    run(&mut s, "table.formatAs", json!({"range": "A1:B3", "mode": "table", "header": true, "columnPrefix": "列", "clearPictures": true}));
    assert_eq!(get(&mut s, "A1")["picture"], Json::Null);
    assert_eq!(value(&mut s, "A1"), json!("列1"));
    run(&mut s, "edit.undo", json!({}));
    assert_ne!(get(&mut s, "A1")["picture"], Json::Null, "one undo step brings the picture back");
}
