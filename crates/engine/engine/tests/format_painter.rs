//! The format painter and the font controls as WPS for Mac has them (module M6, cases FP-C… and
//! FONT-C…): once and sticky painting, what is painted (cell styles and merged areas, not values,
//! formulas, conditional formats, validation, links or sizes), the source repeated over a larger
//! range and cut to a smaller one, whole rows and columns, other sheets, one undo step; WPS's font
//! size list for Increase and Decrease Font Size; filtered rows left alone; frozen panes that
//! start at a scrolled view's top-left (carried from M5).

use kition_sheet_engine::Session;
use kition_sheet_engine::core::CellRef;
use kition_sheet_engine::io::{open_bytes, save_bytes};
use kition_sheet_engine::model::{Color, Style, Underline};
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn style(s: &Session, cell: &str) -> Style {
    let d = s.doc().unwrap();
    let sheet = d.wb.active().unwrap();
    d.wb.styles.get(sheet.style_id(CellRef::parse(cell).unwrap())).clone()
}

fn disabled(s: &Session, id: &str) -> bool {
    s.commands().iter().any(|c| c.id == id && !c.enabled)
}

fn value(s: &mut Session, cell: &str) -> Json {
    run(s, "cell.get", json!({"cell": cell}))["value"].clone()
}

/// A1 in 楷体, red, bold, on yellow, with a 0.00% format.
fn styled_a1(s: &mut Session) {
    run(s, "cell.set", json!({"cell": "A1", "input": "0.5"}));
    run(s, "home.fontName", json!({"range": "A1", "name": "楷体"}));
    run(s, "home.fontColor", json!({"range": "A1", "color": "#FF0000"}));
    run(s, "home.bold", json!({"range": "A1", "on": true}));
    run(s, "home.fillColor", json!({"range": "A1", "color": "#FFFF00"}));
    run(s, "home.numberFormat", json!({"range": "A1", "format": "0.00%"}));
}

fn painted_like_a1(s: &Session, cell: &str) -> bool {
    let (a, b) = (style(s, "A1"), style(s, cell));
    a.font == b.font && a.fill == b.fill && a.num_fmt == b.num_fmt
}

#[test]
fn a_click_paints_once_a_double_click_keeps_painting() {
    let mut s = session();
    styled_a1(&mut s);
    run(&mut s, "selection.set", json!({"range": "A1"}));
    let up = run(&mut s, "edit.formatPainter", json!({}));
    assert_eq!(up, json!({"active": true, "sticky": false}), "FP-C6: a click picks up the format");
    run(&mut s, "edit.formatPainter", json!({"apply": "B1"}));
    assert!(painted_like_a1(&s, "B1"), "FP-C6: B1 takes A1's format");
    assert!(s.format_painter.is_none(), "FP-C12: one paint puts the painter down");
    assert!(s.execute("edit.formatPainter", json!({"apply": "C1"})).is_err(), "FP-C12: C1 is not painted");
    assert!(!painted_like_a1(&s, "C1"));

    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({"sticky": true}));
    for cell in ["B2", "C2", "D2"] {
        run(&mut s, "edit.formatPainter", json!({"apply": cell}));
    }
    assert!(["B2", "C2", "D2"].iter().all(|c| painted_like_a1(&s, c)), "FP-C7/C13: sticky paints each cell");
    assert!(s.format_painter.is_some());
    let down = run(&mut s, "edit.formatPainter", json!({}));
    assert_eq!(down["active"], json!(false), "FP-C8: a second click puts it down");
    run(&mut s, "edit.formatPainter", json!({"sticky": true}));
    run(&mut s, "edit.formatPainter", json!({"cancel": true}));
    assert!(s.format_painter.is_none(), "Esc puts it down");
}

#[test]
fn painting_keeps_values_formulas_and_the_cells_other_features() {
    let mut s = session();
    styled_a1(&mut s);
    run(&mut s, "cell.set", json!({"cell": "B1", "input": "=SUM(A1:A1)"}));
    run(&mut s, "cell.set", json!({"cell": "C1", "input": "text"}));
    run(&mut s, "insert.link", json!({"cell": "C1", "target": "https://example.com"}));
    run(&mut s, "data.validation", json!({"range": "D1", "type": "list", "formula1": "\"甲,乙\""}));
    run(
        &mut s,
        "home.conditionalFormat",
        json!({"range": "E1", "rule": {"type": "cellIs", "operator": "greaterThan", "value": "5", "style": {"fill": "#00FF00"}}}),
    );
    run(&mut s, "home.rowHeight", json!({"rows": "1", "height": 30}));
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({"sticky": true}));
    run(&mut s, "edit.formatPainter", json!({"apply": "B1:E1"}));
    assert!(["B1", "C1", "D1", "E1"].iter().all(|c| painted_like_a1(&s, c)));
    let b1 = run(&mut s, "cell.get", json!({"cell": "B1"}));
    assert_eq!(b1["formula"], json!("=SUM(A1:A1)"), "FP-C26: the formula stays");
    assert_eq!(b1["value"], json!(0.5));
    assert_eq!(value(&mut s, "C1"), json!("text"));
    let wb = &s.doc().unwrap().wb;
    let sheet = wb.active().unwrap();
    assert_eq!(sheet.hyperlinks.len(), 1, "FP-C30: the link stays");
    assert_eq!(sheet.validations.len(), 1, "FP-C32: validation is not painted over");
    assert_eq!(sheet.cond_formats.len(), 1, "FP-C33: the conditional format stays, none is copied");
    assert_eq!(sheet.rows.get(&0).and_then(|r| r.size), Some(30.0), "FP-C10/C20: row heights stay");
}

#[test]
fn a_single_cell_takes_the_sources_size_and_ranges_repeat_or_cut_it() {
    let mut s = session();
    // A1:C3 with a different fill in each cell.
    let colours = ["#110000", "#220000", "#330000", "#001100", "#002200", "#003300", "#000011", "#000022", "#000033"];
    for (i, colour) in colours.iter().enumerate() {
        let cell = format!("{}{}", ["A", "B", "C"][i % 3], i / 3 + 1);
        run(&mut s, "home.fillColor", json!({"range": cell, "color": colour}));
    }
    let fill = |s: &Session, cell: &str| style(s, cell).fill.fg;
    run(&mut s, "selection.set", json!({"range": "A1:C3"}));
    run(&mut s, "edit.formatPainter", json!({"sticky": true}));
    run(&mut s, "edit.formatPainter", json!({"apply": "E1"}));
    assert_eq!(fill(&s, "G3"), fill(&s, "C3"), "FP-C11: a click paints the source's size");
    assert_eq!(s.doc().unwrap().selection.a1(), "E1:G3");
    run(&mut s, "edit.formatPainter", json!({"apply": "E5:F6"}));
    assert_eq!((fill(&s, "E5"), fill(&s, "F6")), (fill(&s, "A1"), fill(&s, "B2")), "FP-C38: a smaller range takes its part");
    assert_eq!(fill(&s, "G5"), Color::Auto, "FP-C38: nothing past it");
    run(&mut s, "edit.formatPainter", json!({"apply": "I1:K6"}));
    assert_eq!((fill(&s, "I4"), fill(&s, "K6")), (fill(&s, "A1"), fill(&s, "C3")), "FP-C39: a larger range repeats it");
    assert_eq!(fill(&s, "L1"), Color::Auto);
}

#[test]
fn merged_areas_are_painted_and_keep_the_targets_content() {
    let mut s = session();
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "title"}));
    run(&mut s, "home.mergeCenter", json!({"range": "A1:C1"}));
    run(&mut s, "home.bold", json!({"range": "A1:C1", "on": true}));
    for (cell, text) in [("B3", "one"), ("C3", "two"), ("D3", "three")] {
        run(&mut s, "cell.set", json!({"cell": cell, "input": text}));
    }
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({}));
    run(&mut s, "edit.formatPainter", json!({"apply": "B3"}));
    let merges: Vec<String> = s.doc().unwrap().wb.active().unwrap().merges.iter().map(|m| m.a1()).collect();
    assert!(merges.contains(&"B3:D3".to_string()), "FP-C27: the merged area is painted: {merges:?}");
    assert!(style(&s, "B3").font.bold);
    assert_eq!(
        [value(&mut s, "B3"), value(&mut s, "C3"), value(&mut s, "D3")],
        [json!("one"), json!("two"), json!("three")],
        "FP-C27: no content is lost"
    );
}

#[test]
fn whole_rows_and_columns_are_painted_without_resizing_them() {
    let mut s = session();
    run(&mut s, "home.bold", json!({"range": "A1", "on": true}));
    run(&mut s, "home.fillColor", json!({"range": "A1", "color": "#FFFF00"}));
    run(&mut s, "cell.set", json!({"cell": "H5", "input": "x"}));
    run(&mut s, "home.columnWidth", json!({"cols": "H", "width": 120}));
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({"sticky": true}));
    run(&mut s, "edit.formatPainter", json!({"apply": "5:5"}));
    run(&mut s, "edit.formatPainter", json!({"apply": "J:J"}));
    for cell in ["H5", "Z5", "J1", "J900"] {
        assert!(style(&s, cell).font.bold && style(&s, cell).fill == style(&s, "A1").fill, "FP-C23: {cell} is painted");
    }
    assert!(!style(&s, "H6").font.bold);
    assert_eq!(value(&mut s, "H5"), json!("x"));
    assert_eq!(s.doc().unwrap().wb.active().unwrap().cols.get(&7).and_then(|c| c.size), Some(120.0), "FP-C23: widths stay");
}

#[test]
fn the_painter_crosses_sheets_and_undoes_in_one_step() {
    let mut s = session();
    styled_a1(&mut s);
    run(&mut s, "home.insertSheet", json!({"name": "Two"}));
    run(&mut s, "sheet.activate", json!({"sheet": 0}));
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({}));
    run(&mut s, "sheet.activate", json!({"sheet": "Two"}));
    run(&mut s, "cell.set", json!({"cell": "B2", "input": "kept"}));
    assert!(s.format_painter.is_some(), "FP-C24: still up on another sheet");
    run(&mut s, "edit.formatPainter", json!({"apply": "B2"}));
    let b2 = style(&s, "B2");
    assert!(b2.font.bold && b2.font.name == "楷体" && b2.num_fmt.as_str() == "0.00%", "FP-C24");
    assert_eq!(value(&mut s, "B2"), json!("kept"));
    run(&mut s, "edit.undo", json!({}));
    assert!(!style(&s, "B2").font.bold, "FP-C45: one undo takes the paint back");
    assert_eq!(value(&mut s, "B2"), json!("kept"));
    run(&mut s, "edit.redo", json!({}));
    assert!(style(&s, "B2").font.bold, "FP-C45: redo paints it again");
}

#[test]
fn painting_over_rich_text_leaves_no_old_character_formats() {
    let mut s = session();
    run(&mut s, "home.fontName", json!({"range": "A1", "name": "楷体"}));
    run(&mut s, "cell.set", json!({"cell": "B1", "input": "字体样式测试"}));
    run(&mut s, "cell.formatText", json!({"cell": "B1", "start": 0, "end": 2, "name": "黑体", "underline": true}));
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({}));
    run(&mut s, "edit.formatPainter", json!({"apply": "B1"}));
    let b1 = run(&mut s, "cell.get", json!({"cell": "B1"}));
    let runs = b1["runs"].as_array().cloned().unwrap_or_default();
    assert!(runs.iter().all(|r| r["font"].get("name").is_none()), "FP-C22: the characters take the painted font: {runs:?}");
    assert_eq!(style(&s, "B1").font.name, "楷体");
}

#[test]
fn font_sizes_step_through_wps_list_and_stop_at_its_ends() {
    let mut s = session();
    run(&mut s, "home.fontSize", json!({"range": "A1", "size": 20}));
    assert_eq!(run(&mut s, "home.increaseFontSize", json!({}))["size"], json!(22.0), "FONT-C88: 20 → 22");
    assert_eq!(run(&mut s, "home.increaseFontSize", json!({}))["size"], json!(24.0), "FONT-C88: 22 → 24");
    run(&mut s, "home.fontSize", json!({"range": "A1", "size": 15}));
    assert_eq!(run(&mut s, "home.increaseFontSize", json!({}))["size"], json!(16.0), "FONT-C88: 15 → the next listed size");
    run(&mut s, "home.fontSize", json!({"range": "A1", "size": 72}));
    assert!(s.execute("home.increaseFontSize", json!({})).is_err(), "FONT-C83: no larger than 72");
    assert!(disabled(&s, "home.increaseFontSize"), "FONT-C83: greyed at 72");
    run(&mut s, "home.fontSize", json!({"range": "A1", "size": 6}));
    assert!(disabled(&s, "home.decreaseFontSize"), "FONT-C85: greyed at 6");
    assert_eq!(run(&mut s, "home.increaseFontSize", json!({}))["size"], json!(8.0));
    for bad in [0.0, 410.0, 500.0] {
        assert!(s.execute("home.fontSize", json!({"range": "A1", "size": bad})).is_err(), "FONT-C87: {bad}");
    }
    run(&mut s, "home.fontSize", json!({"range": "A1", "size": 200}));
    assert_eq!(style(&s, "A1").font.size, 200.0, "FONT-C86: any size from 1 to 409");
}

#[test]
fn a_larger_font_on_whole_rows_or_columns_makes_their_rows_taller() {
    let mut s = session();
    run(&mut s, "range.setValues", json!({"range": "A3", "values": [["三"], ["四"], ["五"]]}));
    run(&mut s, "cell.set", json!({"cell": "D7", "input": "列"}));
    run(&mut s, "home.fontSize", json!({"range": "3:4", "size": 24}));
    let height = |s: &Session, row: u32| s.doc().unwrap().wb.active().unwrap().row_height(row);
    assert!(height(&s, 2) > 20.0 && height(&s, 3) > 20.0, "FONT-C91: rows 3 and 4 grow");
    assert_eq!(height(&s, 4), kition_sheet_engine::model::DEFAULT_ROW_HEIGHT, "FONT-C91: row 5 stays");
    run(&mut s, "home.fontSize", json!({"range": "D:D", "size": 36}));
    assert!(height(&s, 6) > 30.0, "FONT-C91: a row with a cell in column D grows");
}

#[test]
fn formatting_leaves_rows_a_filter_hides_and_formats_rows_hidden_by_hand() {
    let mut s = session();
    run(&mut s, "range.setValues", json!({"range": "A1", "values": [["k"], ["a"], ["b"], ["a"], ["b"]]}));
    run(&mut s, "data.filter", json!({"range": "A1:A5"}));
    run(&mut s, "data.filterBy", json!({"column": 0, "values": ["a"]}));
    run(&mut s, "home.bold", json!({"range": "A2:A5", "on": true}));
    assert!(style(&s, "A2").font.bold && style(&s, "A4").font.bold);
    assert!(!style(&s, "A3").font.bold && !style(&s, "A5").font.bold, "FONT-C125: filtered rows are left alone");

    let mut s = session();
    run(&mut s, "home.hideRows", json!({"rows": "3"}));
    run(&mut s, "home.bold", json!({"range": "A2:A4", "on": true}));
    assert!(style(&s, "A3").font.bold, "FONT-C126: rows hidden by hand are formatted");
}

#[test]
fn fonts_and_rich_text_survive_saving() {
    let mut s = session();
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "字体样式测试"}));
    run(&mut s, "home.fontName", json!({"range": "A1", "name": "微软雅黑"}));
    run(&mut s, "cell.formatText", json!({"cell": "A1", "start": 2, "end": 4, "name": "楷体", "bold": true, "color": "#FF0000", "strike": true}));
    run(&mut s, "home.underline", json!({"range": "B1", "on": true}));
    let bytes = save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
    let wb = open_bytes("Book.xlsx", &bytes).unwrap().0;
    let sheet = wb.active().unwrap();
    let a1 = CellRef::parse("A1").unwrap();
    assert_eq!(wb.styles.get(sheet.style_id(a1)).font.name, "微软雅黑", "FONT-C57/C59");
    let runs = sheet.cell(a1).and_then(|c| c.runs()).map(|r| r.runs.clone()).unwrap_or_default();
    let styled = runs.iter().find(|r| r.text == "样式").map(|r| r.font.clone()).unwrap_or_default();
    assert_eq!(
        (styled.name.as_deref(), styled.bold, styled.strike),
        (Some("楷体"), Some(true), Some(true)),
        "FONT-C59: rich text is not flattened: {runs:?}"
    );
    assert_eq!(wb.styles.get(sheet.style_id(CellRef::parse("B1").unwrap())).font.underline, Underline::Single);
}

#[test]
fn a_freeze_in_a_scrolled_view_starts_at_its_top_left() {
    let mut s = session();
    run(&mut s, "selection.set", json!({"range": "C15"}));
    run(&mut s, "view.freezeRows", json!({"top": "A11"}));
    let sheet = s.doc().unwrap().wb.active().unwrap().clone();
    assert_eq!(sheet.freeze, Some((4, 0)), "rows 11–14 freeze, counted from the view's top");
    assert_eq!(sheet.view_top_left.a1(), "A11", "rows 1–10 stay out of view");
    run(&mut s, "view.freezePanes", json!({"cell": "D20", "top": "B11"}));
    assert_eq!(s.doc().unwrap().wb.active().unwrap().freeze, Some((9, 2)));
    let bytes = save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
    let wb = open_bytes("Book.xlsx", &bytes).unwrap().0;
    let back = wb.active().unwrap();
    assert_eq!((back.freeze, back.view_top_left.a1()), (Some((9, 2)), "B11".to_string()), "the file keeps where the panes start");
    // Without the view's top-left (an agent) the panes start at the first cell.
    run(&mut s, "view.freezeTopRow", json!({}));
    let sheet = s.doc().unwrap().wb.active().unwrap().clone();
    assert_eq!((sheet.freeze, sheet.view_top_left.a1()), (Some((1, 0)), "A1".to_string()));
}
