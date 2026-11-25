//! Moving cells by dragging the selection's border, as WPS for Mac and Excel do (module M10,
//! CLN-X1 to CLN-X6): a move keeps what belongs to the cells and the formulas that refer to them
//! follow; Cmd/Ctrl copies; Shift makes room and closes up behind; whole rows and columns move as
//! lines; one undo step.

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

fn text(s: &Session, cell: &str) -> String {
    let wb = &s.doc().unwrap().wb;
    match wb.active().unwrap().value(CellRef::parse(cell).unwrap()) {
        Value::Text(t) => t.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Empty => String::new(),
        other => format!("{other:?}"),
    }
}

fn formula(s: &Session, cell: &str) -> String {
    s.doc()
        .unwrap()
        .wb
        .active()
        .unwrap()
        .cell(CellRef::parse(cell).unwrap())
        .and_then(|c| c.formula.as_ref().map(|f| f.text.clone()))
        .unwrap_or_default()
}

fn sheet(s: &Session) -> &kition_sheet_engine::model::Sheet {
    s.doc().unwrap().wb.active().unwrap()
}

#[test]
fn a_move_takes_the_cells_and_formulas_follow_them() {
    let mut s = session();
    set(&mut s, "A1", "1");
    set(&mut s, "A2", "2");
    set(&mut s, "B1", "=A1+A2");
    set(&mut s, "C1", "=SUM(A1:A2)");
    run(&mut s, "format.bold", json!({"range": "A1"}));
    let answer = run(&mut s, "edit.moveCells", json!({"from": "A1:A2", "to": "D4"}));
    assert_eq!(answer["range"], json!("D4:D5"));
    assert_eq!((text(&s, "A1"), text(&s, "A2")), (String::new(), String::new()), "the place left is empty");
    assert_eq!((text(&s, "D4"), text(&s, "D5")), ("1".into(), "2".into()));
    assert_eq!(formula(&s, "B1"), "D4+D5", "a formula that read the cells follows them");
    assert_eq!(formula(&s, "C1"), "SUM(D4:D5)");
    assert_eq!(text(&s, "B1"), "3");
    let wb = &s.doc().unwrap().wb;
    let style = sheet(&s).cell(CellRef::parse("D4").unwrap()).unwrap().style;
    assert!(wb.styles.get(style).font.bold, "the formats go with the cells");
    assert_eq!(s.doc().unwrap().selection.a1(), "D4:D5", "the block lands selected");
    // One step undoes the whole move.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!((text(&s, "A1"), text(&s, "D4"), formula(&s, "B1")), ("1".into(), String::new(), "A1+A2".into()));
}

#[test]
fn merged_areas_notes_validation_and_conditional_formats_go_with_the_block() {
    let mut s = session();
    set(&mut s, "A1", "合并");
    run(&mut s, "cell.merge", json!({"range": "A1:B1"}));
    run(&mut s, "note.add", json!({"cell": "A2", "text": "备注"}));
    run(&mut s, "data.validation", json!({"range": "A2", "type": "list", "formula1": "\"是,否\""}));
    run(
        &mut s,
        "conditionalFormat.apply",
        json!({"range": "B2", "rule": {"type": "cellIs", "operator": "greaterThan", "value": "5", "preset": "lightRedFill"}}),
    );
    run(&mut s, "edit.moveCells", json!({"from": "A1:B2", "to": "C5"}));
    let sh = sheet(&s);
    assert!(sh.merges.contains(&RangeRef::parse("C5:D5").unwrap()), "{:?}", sh.merges);
    assert!(!sh.merges.contains(&RangeRef::parse("A1:B1").unwrap()), "the old merged area is gone");
    assert!(sh.comments.contains_key(&CellRef::parse("C6").unwrap()) && !sh.comments.contains_key(&CellRef::parse("A2").unwrap()));
    assert!(
        sh.validations.iter().any(|v| v.ranges.contains(&RangeRef::parse("C6").unwrap())),
        "{:?}",
        sh.validations.iter().map(|v| v.ranges.clone()).collect::<Vec<_>>()
    );
    assert!(sh.cond_formats.iter().any(|c| c.ranges.contains(&RangeRef::parse("D6").unwrap())));
}

#[test]
fn a_copy_leaves_the_block_and_pastes_it_again() {
    let mut s = session();
    set(&mut s, "A1", "x");
    set(&mut s, "A2", "=A1&\"y\"");
    run(&mut s, "edit.moveCells", json!({"from": "A1:A2", "to": "C1", "copy": true}));
    assert_eq!((text(&s, "A1"), text(&s, "C1")), ("x".into(), "x".into()));
    assert_eq!(formula(&s, "C2"), "C1&\"y\"", "a copied formula's relative references move");
    assert_eq!(formula(&s, "A2"), "A1&\"y\"");
}

#[test]
fn shift_makes_room_and_closes_up_behind() {
    let mut s = session();
    for (i, v) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        set(&mut s, &format!("A{}", i + 1), v);
    }
    set(&mut s, "B1", "=A2");
    // a b c d e: b moves to between d and e.
    let answer = run(&mut s, "edit.moveCells", json!({"from": "A2", "to": "A5", "insert": "down"}));
    let column: Vec<String> = (1..=5).map(|r| text(&s, &format!("A{r}"))).collect();
    assert_eq!(column, ["a", "c", "d", "b", "e"]);
    assert_eq!(answer["range"], json!("A4"));
    assert_eq!(formula(&s, "B1"), "A4", "the reference follows b");
    // And back up: b goes in above c.
    run(&mut s, "edit.moveCells", json!({"from": "A4", "to": "A2", "insert": "down"}));
    let column: Vec<String> = (1..=5).map(|r| text(&s, &format!("A{r}"))).collect();
    assert_eq!(column, ["a", "b", "c", "d", "e"]);
    // Right: the cells in the row make room.
    set(&mut s, "D1", "q");
    run(&mut s, "edit.moveCells", json!({"from": "D1", "to": "A1", "insert": "right"}));
    assert_eq!((text(&s, "A1"), text(&s, "B1"), text(&s, "D1")), ("q".into(), "a".into(), String::new()));
}

#[test]
fn whole_rows_and_columns_move_as_lines() {
    let mut s = session();
    for r in 1..=4 {
        set(&mut s, &format!("A{r}"), &format!("r{r}"));
    }
    run(&mut s, "row.setHeight", json!({"rows": "2:2", "height": 40}));
    set(&mut s, "C1", "=A2");
    // Row 2 goes in under row 4.
    let answer = run(&mut s, "edit.moveCells", json!({"from": "2:2", "to": "A5", "insert": "down"}));
    assert_eq!(answer["range"], json!("4:4"));
    let column: Vec<String> = (1..=4).map(|r| text(&s, &format!("A{r}"))).collect();
    assert_eq!(column, ["r1", "r3", "r4", "r2"]);
    assert_eq!(sheet(&s).row_height(3), 40.0, "the row keeps its height");
    assert_eq!(formula(&s, "C1"), "A4");
    // A column dropped over another (no Shift) replaces it.
    set(&mut s, "E1", "e");
    run(&mut s, "edit.moveCells", json!({"from": "E:E", "to": "G1"}));
    assert_eq!((text(&s, "E1"), text(&s, "G1")), (String::new(), "e".into()));
    assert_eq!(s.doc().unwrap().selection.a1(), "G:G");
}

#[test]
fn a_move_refuses_what_it_cannot_do() {
    let mut s = session();
    set(&mut s, "A1", "1");
    assert!(s.execute("edit.moveCells", json!({"from": "A1"})).is_err(), "`to` is needed");
    assert!(s.execute("edit.moveCells", json!({"from": "A1:B2", "to": "XFD1"})).is_err(), "past the sheet's edge");
    assert!(s.execute("edit.moveCells", json!({"from": "A1", "to": "B1", "insert": "up"})).is_err());
    // Onto itself: nothing changes.
    run(&mut s, "edit.moveCells", json!({"from": "A1", "to": "A1"}));
    assert_eq!(text(&s, "A1"), "1");
}

#[test]
fn a_new_sheet_has_wps_default_column_width_and_row_height() {
    let mut s = session();
    let sh = sheet(&s);
    assert_eq!((sh.default_col_width, sh.default_row_height), (69.0, 19.0), "WPS for Mac: 51.75 pt by 14.25 pt");
    run(&mut s, "sheet.insert", json!({"name": "Two"}));
    let sh = sheet(&s);
    assert_eq!((sh.col_width(3), sh.row_height(3)), (69.0, 19.0));
    // Saved and opened again, the sizes stay.
    let bytes = kition_sheet_engine::io::save_bytes(&s.doc().unwrap().wb, "Sizes.xlsx").unwrap();
    let (wb, _) = kition_sheet_engine::io::open_bytes("Sizes.xlsx", &bytes).unwrap();
    let sh = wb.sheet(0).unwrap();
    assert!(
        (sh.default_col_width - 69.0).abs() < 0.01 && (sh.default_row_height - 19.0).abs() < 0.01,
        "{} {}",
        sh.default_col_width,
        sh.default_row_height
    );
}
