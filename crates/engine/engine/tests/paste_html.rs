//! Pasting another spreadsheet's table from the system clipboard: its HTML keeps the merged cells,
//! the line breaks in a cell and the formatting that its tab-separated text loses (a cell with a
//! line break, unquoted there, split into rows).

use kition_sheet_core::{CellRef, RangeRef, Value};
use kition_sheet_engine::Session;
use kition_sheet_model::{BorderStyle, Color, HAlign, VAlign};
use serde_json::json;

/// A two-part sheet as a web spreadsheet puts it on the clipboard: a title merged across the
/// table, a category merged down three rows, cells with line breaks, and a wide first column.
const HTML: &str = "<meta charset='utf-8'><style>td{white-space:nowrap;vertical-align:middle}.head{font-weight:700;background:#e8e6f4;text-align:center}\
    .box{border:1px solid #000000}</style><table style=\"font-size:13px\"><colgroup><col width=\"153\"><col width=\"230\"><col width=\"288\"></colgroup>\
    <tbody><tr><td class=\"head box\" colspan=\"3\" style=\"font-size:16pt\">Scenarios</td></tr>\
    <tr><td class=\"head box\">Kind</td><td class=\"head box\">Steps</td><td class=\"head box\">Metric</td></tr>\
    <tr><td class=\"box\" rowspan=\"3\" style=\"text-align:center\">Operations</td><td class=\"box\">1. Close every process<br>2. Start the app<br>3. See the home screen</td><td class=\"box\">Start time</td></tr>\
    <tr><td class=\"box\"><div>1. Open the file</div><div>2. See all of it</div></td><td class=\"box\">Load time</td></tr>\
    <tr><td class=\"box\">Save</td><td class=\"box\">12</td></tr></tbody></table>";

/// The same table's plain text, as that spreadsheet writes it (line breaks unquoted).
const TEXT: &str = "Scenarios\t\t\nKind\tSteps\tMetric\nOperations\t1. Close every process\n2. Start the app\n3. See the home screen\tStart time\n\t1. Open the file\n2. See all of it\tLoad time\n\tSave\t12\n";

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn at(cell: &str) -> CellRef {
    CellRef::parse(cell).unwrap()
}

#[test]
fn an_html_table_keeps_merges_line_breaks_and_formatting() {
    let mut s = session();
    s.execute("selection.set", json!({"range": "B2"})).unwrap();
    let answer = s.execute("edit.paste", json!({"text": TEXT, "html": HTML})).unwrap();
    assert_eq!((answer["rows"].as_u64(), answer["cols"].as_u64()), (Some(5), Some(3)));

    let doc = s.doc().unwrap();
    let wb = &doc.wb;
    let sh = wb.sheet(wb.active_sheet).unwrap();
    let value = |cell: &str| sh.value(at(cell));
    assert_eq!(value("B2"), Value::text("Scenarios"));
    assert_eq!(value("B4"), Value::text("Operations"));
    assert_eq!(value("C4"), Value::text("1. Close every process\n2. Start the app\n3. See the home screen"), "a line break stays in its cell");
    assert_eq!(value("C5"), Value::text("1. Open the file\n2. See all of it"));
    assert_eq!(value("D5"), Value::text("Load time"), "the next row is the next row");
    assert_eq!(value("C6"), Value::text("Save"), "the cell beside a merge moves past it");
    assert_eq!(value("D6"), Value::Number(12.0), "numbers stay numbers");
    assert_eq!(value("B7"), Value::Empty, "nothing spills below the table");

    let mut merges = sh.merges.clone();
    merges.sort_by_key(|m| (m.start.row, m.start.col));
    assert_eq!(merges, vec![RangeRef::parse("B2:D2").unwrap(), RangeRef::parse("B4:B6").unwrap()]);

    let style = |cell: &str| wb.styles.get(sh.style_id(at(cell)));
    let title = style("B2");
    assert!(title.font.bold);
    assert_eq!(title.font.size, 16.0);
    assert_eq!(title.fill.fg, Color::Rgb(0xe8e6f4));
    assert_eq!(title.align.h, HAlign::Center);
    assert_eq!(title.borders.top.style, BorderStyle::Thin);
    assert_eq!(style("D2").borders.right.style, BorderStyle::Thin, "the cells a merge covers carry its borders");
    let steps = style("C4");
    assert!(steps.align.wrap, "a cell with line breaks wraps");
    assert_eq!(steps.align.v, VAlign::Center);
    assert_eq!(steps.font.size, 10.0, "the table's 13px");
    assert!(!style("D4").align.wrap);
    assert_eq!(style("B4").align.h, HAlign::Center);

    assert_eq!(sh.col_width(1), 153.0);
    assert_eq!(sh.col_width(3), 288.0);
    assert!(sh.row_height(3) > sh.row_height(2) * 2.0, "the row of three lines grows to show them");
    assert_eq!(doc.selection.ranges, vec![RangeRef::parse("B2:D6").unwrap()]);
}

#[test]
fn a_pasted_table_replaces_the_merges_under_it_and_undoes_in_one_step() {
    let mut s = session();
    s.execute("cell.set", json!({"cell": "C3", "input": "old"})).unwrap();
    s.execute("home.mergeCells", json!({"range": "C3:E4"})).unwrap();
    s.execute("edit.paste", json!({"at": "B2", "text": TEXT, "html": HTML})).unwrap();
    {
        let wb = &s.doc().unwrap().wb;
        let sh = wb.sheet(wb.active_sheet).unwrap();
        assert!(!sh.merges.contains(&RangeRef::parse("C3:E4").unwrap()), "the old merge under the table is gone");
        assert_eq!(sh.value(at("C3")), Value::text("Steps"));
    }
    s.execute("edit.undo", json!({})).unwrap();
    let wb = &s.doc().unwrap().wb;
    let sh = wb.sheet(wb.active_sheet).unwrap();
    assert_eq!(sh.merges, vec![RangeRef::parse("C3:E4").unwrap()]);
    assert_eq!(sh.value(at("C3")), Value::text("old"));
    assert_eq!(sh.value(at("B2")), Value::Empty);
}

#[test]
fn html_without_a_table_and_our_own_copy_paste_as_before() {
    let mut s = session();
    s.execute("selection.set", json!({"range": "A1"})).unwrap();
    s.execute("edit.paste", json!({"text": "a\tb\n", "html": "<p>a b</p>"})).unwrap();
    {
        let wb = &s.doc().unwrap().wb;
        let sh = wb.sheet(wb.active_sheet).unwrap();
        assert_eq!((sh.value(at("A1")), sh.value(at("B1"))), (Value::text("a"), Value::text("b")));
    }
    // A copy of our own cells pastes from our clipboard (formulas and all), not its HTML.
    s.execute("cell.set", json!({"cell": "D1", "input": "=1+1"})).unwrap();
    let copied = s.execute("edit.copy", json!({"range": "D1", "html": true})).unwrap();
    s.execute("edit.paste", json!({"at": "D3", "text": copied["text"], "html": copied["html"]})).unwrap();
    let wb = &s.doc().unwrap().wb;
    let sh = wb.sheet(wb.active_sheet).unwrap();
    assert!(sh.cell(at("D3")).and_then(|c| c.formula.as_ref()).is_some(), "the formula came along");
}
