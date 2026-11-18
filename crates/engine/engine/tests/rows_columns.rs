//! Inserting and deleting rows and columns as WPS for Mac does (module M4, cases INS-C… and
//! DEL-C…): counts and sides, several areas at once, the formats new lines take, frozen panes,
//! conditional formats, validation, hidden and grouped lines, merged cells, filtered rows, the
//! selection afterwards, and undo.

use kition_sheet_core::{CellRef, RangeRef, Value};
use kition_sheet_engine::Session;
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn set(s: &mut Session, cell: &str, input: &str) {
    run(s, "cell.set", json!({"cell": cell, "input": input}));
}

fn select(s: &mut Session, range: &str) {
    run(s, "selection.set", json!({"range": range}));
}

fn text(s: &Session, cell: &str) -> String {
    let wb = &s.doc().unwrap().wb;
    match wb.active().unwrap().value(CellRef::parse(cell).unwrap()) {
        Value::Text(t) => t.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Empty => String::new(),
        other => format!("{other:?}"),
    }
}

fn bold(s: &Session, cell: &str) -> bool {
    let wb = &s.doc().unwrap().wb;
    let style = wb.active().unwrap().cell(CellRef::parse(cell).unwrap()).map(|c| c.style).unwrap_or_default();
    wb.styles.get(style).font.bold
}

fn sheet(s: &Session) -> &kition_sheet_engine::model::Sheet {
    s.doc().unwrap().wb.active().unwrap()
}

/// Column A holds `A1`..`A12`, row 1 holds `A1`..`H1` (each cell its own address).
fn grid(s: &mut Session) {
    for r in 1..=12 {
        set(s, &format!("A{r}"), &format!("A{r}"));
    }
    for c in "BCDEFGH".chars() {
        set(s, &format!("{c}1"), &format!("{c}1"));
        set(s, &format!("{c}3"), &format!("{c}3"));
    }
}

#[test]
fn rows_go_above_or_below_the_selection_with_its_count_or_another() {
    let mut s = session();
    grid(&mut s);
    // INS-C7: one row above B3's row.
    select(&mut s, "B3");
    run(&mut s, "home.insertRows", json!({}));
    assert_eq!(text(&s, "A4"), "A3");
    assert_eq!(text(&s, "A3"), "");
    // The new row is selected (WPS).
    assert_eq!(s.doc().unwrap().selection.a1(), "B3");
    run(&mut s, "edit.undo", json!({}));
    // INS-C8: five above A5.
    select(&mut s, "A5");
    run(&mut s, "home.insertRows", json!({"count": 5}));
    assert_eq!(text(&s, "A10"), "A5");
    run(&mut s, "edit.undo", json!({}));
    // INS-C14/C15: below A5, the rows after it move.
    select(&mut s, "A5");
    run(&mut s, "home.insertRows", json!({"count": 5, "below": true}));
    assert_eq!(text(&s, "A5"), "A5");
    assert_eq!(text(&s, "A11"), "A6");
    assert_eq!(s.doc().unwrap().selection.a1(), "A6:A10");
    run(&mut s, "edit.undo", json!({}));
    // INS-C9/C16: a block of cells takes its rows' count unless told otherwise.
    select(&mut s, "A4:C6");
    run(&mut s, "home.insertRows", json!({}));
    assert_eq!(text(&s, "A7"), "A4");
    run(&mut s, "edit.undo", json!({}));
    select(&mut s, "A4:C6");
    run(&mut s, "home.insertRows", json!({"count": 1, "below": true}));
    assert_eq!(text(&s, "A6"), "A6");
    assert_eq!(text(&s, "A8"), "A7");
}

#[test]
fn columns_go_left_or_right() {
    let mut s = session();
    grid(&mut s);
    // INS-C21/C22.
    select(&mut s, "C3");
    run(&mut s, "home.insertColumns", json!({"count": 5}));
    assert_eq!(text(&s, "H3"), "C3");
    run(&mut s, "edit.undo", json!({}));
    // INS-C28/C29: right of C, D moves.
    select(&mut s, "C3");
    run(&mut s, "home.insertColumns", json!({"count": 5, "right": true}));
    assert_eq!(text(&s, "C3"), "C3");
    assert_eq!(text(&s, "I3"), "D3");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s, "D3"), "D3");
    // INS-C46 redo.
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(text(&s, "I3"), "D3");
}

#[test]
fn deleting_takes_the_selected_rows_or_columns_and_every_area() {
    let mut s = session();
    grid(&mut s);
    // DEL-C8: A3:E5 deletes rows 3 to 5.
    select(&mut s, "A3:E5");
    run(&mut s, "home.deleteRows", json!({}));
    assert_eq!(text(&s, "A3"), "A6");
    run(&mut s, "edit.undo", json!({}));
    // DEL-C14: A3:C5 deletes columns A to C.
    select(&mut s, "A3:C5");
    run(&mut s, "home.deleteColumns", json!({}));
    assert_eq!(text(&s, "A1"), "D1");
    run(&mut s, "edit.undo", json!({}));
    // Several areas (Ctrl-click rows 3 and 5): both go, in one step.
    run(&mut s, "selection.set", json!({"range": "3:3,5:5"}));
    run(&mut s, "home.deleteRows", json!({}));
    assert_eq!(text(&s, "A3"), "A4");
    assert_eq!(text(&s, "A4"), "A6");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s, "A5"), "A5");
}

#[test]
fn the_whole_sheet_selected_has_no_rows_or_columns_to_insert_or_delete() {
    let mut s = session();
    grid(&mut s);
    select(&mut s, "A1:XFD1048576");
    for command in ["home.deleteRows", "home.deleteColumns", "home.insertRows", "home.insertColumns"] {
        assert!(s.execute(command, json!({})).is_err(), "{command}");
    }
    assert_eq!(text(&s, "A12"), "A12");
}

#[test]
fn new_lines_take_the_formats_of_the_line_before_above_and_after_below() {
    let mut s = session();
    set(&mut s, "A2", "x");
    set(&mut s, "A4", "y");
    run(&mut s, "home.bold", json!({"range": "A2", "on": true}));
    run(&mut s, "home.bold", json!({"range": "A4", "on": true}));
    // Above row 3: the new row 3 takes row 2's format.
    select(&mut s, "A3");
    run(&mut s, "home.insertRows", json!({}));
    assert!(bold(&s, "A3"));
    assert_eq!(text(&s, "A3"), "");
    run(&mut s, "edit.undo", json!({}));
    // Below row 2: the new row 3 takes the row after's (row 3, plain), not row 2's.
    select(&mut s, "A2");
    run(&mut s, "home.insertRows", json!({"below": true}));
    assert!(!bold(&s, "A3"));
    run(&mut s, "edit.undo", json!({}));
    // Below row 3: the new row 4 takes old row 4's bold.
    select(&mut s, "A3");
    run(&mut s, "home.insertRows", json!({"below": true}));
    assert!(bold(&s, "A4"));
    assert_eq!(text(&s, "A5"), "y");
    run(&mut s, "edit.undo", json!({}));
    // Above row 1 there is nothing to copy (INS-C71 as WPS has it).
    run(&mut s, "home.bold", json!({"range": "A1", "on": true}));
    select(&mut s, "A1");
    run(&mut s, "home.insertRows", json!({}));
    assert!(!bold(&s, "A1"));
    assert!(bold(&s, "A2"));
    // Columns: left of B takes A's; right of A takes B's.
    run(&mut s, "edit.undo", json!({}));
    select(&mut s, "B1");
    run(&mut s, "home.insertColumns", json!({}));
    assert!(bold(&s, "B1"));
}

#[test]
fn links_validation_merges_and_pictures_stay_where_they_were() {
    let mut s = session();
    set(&mut s, "A2", "9");
    run(&mut s, "insert.link", json!({"cell": "A2", "target": "https://example.com"}));
    run(&mut s, "data.validation", json!({"range": "A2", "type": "whole", "operator": "between", "formula1": "8", "formula2": "12"}));
    run(&mut s, "home.mergeCells", json!({"range": "D7:D8"}));
    // INS-C89: rows 2 and 3, two below: no validation on rows 4 and 5.
    select(&mut s, "2:3");
    run(&mut s, "home.insertRows", json!({"count": 2, "below": true}));
    let sh = sheet(&s);
    assert_eq!(sh.validations.len(), 1);
    assert_eq!(sh.validations[0].ranges, vec![RangeRef::parse("A2").unwrap()]);
    assert_eq!(sh.hyperlinks.len(), 1);
    assert_eq!(sh.merges, vec![RangeRef::parse("D9:D10").unwrap()]);
    run(&mut s, "edit.undo", json!({}));
    // INS-C90: rows 1 and 2, two above: the rule moves to A4, the new rows have none.
    select(&mut s, "1:2");
    run(&mut s, "home.insertRows", json!({}));
    let sh = sheet(&s);
    assert_eq!(sh.validations[0].ranges, vec![RangeRef::parse("A4").unwrap()]);
    // INS-C76: a column inserted beside a merged column has no merge.
    run(&mut s, "edit.undo", json!({}));
    select(&mut s, "D:D");
    run(&mut s, "home.insertColumns", json!({"right": true}));
    assert_eq!(sheet(&s).merges, vec![RangeRef::parse("D7:D8").unwrap()]);
}

#[test]
fn conditional_formats_grow_over_lines_inserted_after_them() {
    let mut s = session();
    let rule = |range: &str| json!({"range": range, "rule": {"type": "expression", "formula": "TRUE", "preset": "lightRedFill"}});
    run(&mut s, "home.conditionalFormat", rule("A5"));
    run(&mut s, "home.conditionalFormat", rule("B5"));
    // INS-C67: column A, one to the right: A and B have rule 1, C rule 2.
    select(&mut s, "A:A");
    run(&mut s, "home.insertColumns", json!({"right": true}));
    let ranges: Vec<String> = sheet(&s).cond_formats.iter().map(|cf| cf.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(",")).collect();
    assert!(ranges.contains(&"A5:B5".to_string()) && ranges.contains(&"C5".to_string()), "{ranges:?}");
    run(&mut s, "edit.undo", json!({}));
    // INS-C69: a row below row 5 takes its rule.
    select(&mut s, "A5");
    run(&mut s, "home.insertRows", json!({"below": true}));
    assert!(sheet(&s).cond_formats.iter().any(|cf| cf.ranges == vec![RangeRef::parse("A5:A6").unwrap()]));
    run(&mut s, "edit.undo", json!({}));
    // Above row 5 (the rule's own row): the rule moves down, the new row has none.
    select(&mut s, "A5");
    run(&mut s, "home.insertRows", json!({}));
    assert!(sheet(&s).cond_formats.iter().any(|cf| cf.ranges == vec![RangeRef::parse("A6").unwrap()]));
    // INS-C68: A1:A200 with a row inserted at 100 becomes A1:A201.
    let mut s = session();
    run(&mut s, "home.conditionalFormat", rule("A1:A200"));
    select(&mut s, "100:100");
    run(&mut s, "home.insertRows", json!({}));
    assert_eq!(sheet(&s).cond_formats[0].ranges, vec![RangeRef::parse("A1:A201").unwrap()]);
    // DEL-C40: deleting another column leaves it alone.
    select(&mut s, "D:D");
    run(&mut s, "home.deleteColumns", json!({}));
    assert_eq!(sheet(&s).cond_formats[0].ranges, vec![RangeRef::parse("A1:A201").unwrap()]);
}

#[test]
fn frozen_panes_grow_and_shrink_but_stay_frozen() {
    let mut s = session();
    run(&mut s, "view.freezePanes", json!({"cell": "B2"}));
    // INS-C85: above the frozen row 1 → two frozen rows; INS-C86: below it → still one.
    select(&mut s, "1:1");
    run(&mut s, "home.insertRows", json!({}));
    assert_eq!(sheet(&s).freeze, Some((2, 1)));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "home.insertRows", json!({"below": true}));
    assert_eq!(sheet(&s).freeze, Some((1, 1)));
    run(&mut s, "edit.undo", json!({}));
    // INS-C87/C88: columns likewise.
    select(&mut s, "A:A");
    run(&mut s, "home.insertColumns", json!({}));
    assert_eq!(sheet(&s).freeze, Some((1, 2)));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "home.insertColumns", json!({"right": true}));
    assert_eq!(sheet(&s).freeze, Some((1, 1)));
    run(&mut s, "edit.undo", json!({}));
    // DEL-C35: deleting the frozen row 1 and column A keeps one of each frozen.
    select(&mut s, "1:1");
    run(&mut s, "home.deleteRows", json!({}));
    select(&mut s, "A:A");
    run(&mut s, "home.deleteColumns", json!({}));
    assert_eq!(sheet(&s).freeze, Some((1, 1)));
    // Three frozen rows less the first are two.
    let mut s = session();
    run(&mut s, "view.freezePanes", json!({"cell": "A4"}));
    select(&mut s, "1:1");
    run(&mut s, "home.deleteRows", json!({}));
    assert_eq!(sheet(&s).freeze, Some((2, 0)));
}

#[test]
fn hidden_and_grouped_lines() {
    let mut s = session();
    for r in 1..=8 {
        set(&mut s, &format!("A{r}"), &format!("A{r}"));
    }
    run(&mut s, "home.hideRows", json!({"rows": "5:5"}));
    let hidden = |s: &Session| -> Vec<u32> { sheet(s).rows.iter().filter(|(_, i)| i.hidden).map(|(r, _)| r + 1).collect() };
    // INS-C77: below row 4 → row 6 hidden. INS-C78: above row 6 → row 5 stays hidden.
    select(&mut s, "4:4");
    run(&mut s, "home.insertRows", json!({"below": true}));
    assert_eq!(hidden(&s), vec![6]);
    run(&mut s, "edit.undo", json!({}));
    select(&mut s, "6:6");
    run(&mut s, "home.insertRows", json!({}));
    assert_eq!(hidden(&s), vec![5]);
    run(&mut s, "edit.undo", json!({}));
    // INS-C79/C80: rows 4–6, three above → row 8; three below → row 5.
    select(&mut s, "4:6");
    run(&mut s, "home.insertRows", json!({}));
    assert_eq!(hidden(&s), vec![8]);
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "home.insertRows", json!({"below": true}));
    assert_eq!(hidden(&s), vec![5]);
    run(&mut s, "edit.undo", json!({}));
    // DEL-C31: rows 1–3 with row 2 hidden all go.
    run(&mut s, "home.unhideRows", json!({"rows": "5:5"}));
    run(&mut s, "home.hideRows", json!({"rows": "2:2"}));
    select(&mut s, "1:3");
    run(&mut s, "home.deleteRows", json!({}));
    assert_eq!(text(&s, "A1"), "A4");
    assert!(hidden(&s).is_empty());
    // A row inserted inside a group joins it.
    let mut s = session();
    run(&mut s, "data.group", json!({"rows": "3:5"}));
    select(&mut s, "A4");
    run(&mut s, "home.insertRows", json!({}));
    assert_eq!(sheet(&s).rows.get(&3).map(|i| i.outline), Some(1));
    assert_eq!(sheet(&s).rows.get(&5).map(|i| i.outline), Some(1));
}

#[test]
fn deleting_filtered_rows_keeps_the_rows_the_filter_hides() {
    let mut s = session();
    for (r, v) in ["k", "a", "b", "a", "b", "a", "b"].iter().enumerate() {
        set(&mut s, &format!("A{}", r + 1), v);
    }
    select(&mut s, "A1:A7");
    run(&mut s, "data.filter", json!({}));
    run(&mut s, "data.filterBy", json!({"column": "A", "values": ["a"]}));
    // DEL-C34 (as WPS for Mac has it): the rows in view go, the hidden ones stay hidden, the
    // filter stays on.
    select(&mut s, "2:6");
    run(&mut s, "home.deleteRows", json!({}));
    let sh = sheet(&s);
    assert_eq!((text(&s, "A1"), text(&s, "A2"), text(&s, "A3"), text(&s, "A4")), ("k".into(), "b".into(), "b".into(), "b".into()));
    assert!(sh.autofilter.as_ref().is_some_and(|af| !af.criteria.is_empty() && af.range == RangeRef::parse("A1:A4").unwrap()));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(text(&s, "A6"), "a");
}

#[test]
fn merged_cells_formulas_and_charts_follow_a_delete() {
    let mut s = session();
    run(&mut s, "home.mergeCells", json!({"range": "A1:B2"}));
    // DEL-C36: deleting row 1 leaves A1:B1 merged; column A instead leaves A1:A2.
    select(&mut s, "1:1");
    run(&mut s, "home.deleteRows", json!({}));
    assert_eq!(sheet(&s).merges, vec![RangeRef::parse("A1:B1").unwrap()]);
    run(&mut s, "edit.undo", json!({}));
    select(&mut s, "A:A");
    run(&mut s, "home.deleteColumns", json!({}));
    assert_eq!(sheet(&s).merges, vec![RangeRef::parse("A1:A2").unwrap()]);
    // DEL-C37: =SUM(B2:B10) shrinks with row 5.
    let mut s = session();
    for r in 2..=10 {
        set(&mut s, &format!("B{r}"), &r.to_string());
    }
    set(&mut s, "B1", "=SUM(B2:B10)");
    select(&mut s, "5:5");
    run(&mut s, "home.deleteRows", json!({}));
    let f = sheet(&s).cell(CellRef::parse("B1").unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone()).unwrap_or_default();
    assert_eq!(f.trim_start_matches('='), "SUM(B2:B9)");
    assert_eq!(text(&s, "B1"), "49");
}

#[test]
fn counts_are_checked() {
    let mut s = session();
    assert!(s.execute("home.insertRows", json!({"count": 0})).is_err());
    // A count past the sheet's end is refused, not wrapped.
    assert!(s.execute("home.insertRows", json!({"rows": "1048576", "below": true})).is_err());
    assert!(s.execute("home.insertColumns", json!({"cols": "A", "count": 4_000_000_000u64})).is_err());
}
