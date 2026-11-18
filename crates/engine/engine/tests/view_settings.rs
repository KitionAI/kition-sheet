//! Frozen panes, gridlines and the highlight of the selection's rows and columns as WPS for Mac
//! has them (module M5, cases FRZ-C…, GRID-C…, HL-C…): WPS's freeze choices for the active cell
//! (each replaces the frozen panes; none on the first row or column), undo, the gridline colour
//! saved with the sheet, and the highlight as the person's view, never the workbook's.

use kition_sheet_engine::Session;
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

fn freeze(s: &Session) -> Option<(u32, u32)> {
    s.doc().unwrap().wb.active().unwrap().freeze
}

#[test]
fn each_freeze_choice_replaces_the_frozen_panes() {
    let mut s = session();
    run(&mut s, "selection.set", json!({"range": "D6"}));
    run(&mut s, "view.freezeRows", json!({}));
    assert_eq!(freeze(&s), Some((5, 0)), "冻结至第 5 行: the rows above the active cell");
    run(&mut s, "view.freezeColumns", json!({}));
    assert_eq!(freeze(&s), Some((0, 3)), "冻结至第 C 列 replaces the rows (WPS)");
    run(&mut s, "view.freezePanes", json!({"cell": "D6"}));
    assert_eq!(freeze(&s), Some((5, 3)));
    run(&mut s, "view.freezeTopRow", json!({}));
    assert_eq!(freeze(&s), Some((1, 0)));
    run(&mut s, "view.freezeColumns", json!({"cell": "G2"}));
    assert_eq!(freeze(&s), Some((0, 6)));
    run(&mut s, "view.unfreezePanes", json!({}));
    assert_eq!(freeze(&s), None);
}

#[test]
fn nothing_to_freeze_above_or_left_of_the_first_cell() {
    let mut s = session();
    run(&mut s, "selection.set", json!({"range": "A1"}));
    assert!(s.execute("view.freezeRows", json!({})).is_err());
    assert!(s.execute("view.freezeColumns", json!({})).is_err());
    run(&mut s, "selection.set", json!({"range": "A5"}));
    assert!(s.execute("view.freezeColumns", json!({})).is_err(), "column A has no columns left of it");
    run(&mut s, "view.freezeRows", json!({}));
    assert_eq!(freeze(&s), Some((4, 0)));
}

#[test]
fn freezing_is_undone_and_redone() {
    let mut s = session();
    run(&mut s, "view.freezeRows", json!({"cell": "C6"}));
    run(&mut s, "view.freezeRows", json!({"cell": "C21"}));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(freeze(&s), Some((5, 0)));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(freeze(&s), None);
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(freeze(&s), Some((20, 0)));
}

#[test]
fn the_gridline_colour_is_the_sheets_and_saved_with_it() {
    let mut s = session();
    run(&mut s, "view.gridlineColor", json!({"color": "#3366ff"}));
    let wb = s.doc().unwrap().wb.clone();
    assert_eq!(wb.active().unwrap().grid_color, Some(0x3366FF));
    let bytes = save_bytes(&wb, "Grid.xlsx").unwrap();
    let (back, _) = open_bytes("Grid.xlsx", &bytes).unwrap();
    assert_eq!(back.active().unwrap().grid_color, Some(0x3366FF), "colorId 48 of the legacy palette");
    // A colour outside the palette is saved as the nearest of its colours.
    run(&mut s, "view.gridlineColor", json!({"color": "#3367FE"}));
    let bytes = save_bytes(&s.doc().unwrap().wb, "Grid.xlsx").unwrap();
    assert_eq!(open_bytes("Grid.xlsx", &bytes).unwrap().0.active().unwrap().grid_color, Some(0x3366FF));
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "view.gridlineColor", json!({"color": "auto"}));
    assert_eq!(s.doc().unwrap().wb.active().unwrap().grid_color, None);
    assert!(s.execute("view.gridlineColor", json!({"color": "blue-ish"})).is_err());
    assert!(s.execute("view.gridlineColor", json!({})).is_err());
}

#[test]
fn the_highlight_is_the_persons_and_never_the_workbooks() {
    let mut s = session();
    let undo = s.doc().unwrap().undo.len();
    assert!(!s.prefs.focus_cell);
    assert_eq!(run(&mut s, "view.focusCell", json!({})), json!({"on": true, "color": "#FBE9C6"}));
    // A colour alone keeps it as it is (on here, off below).
    assert_eq!(run(&mut s, "view.focusCell", json!({"color": "#3d7fe0"})), json!({"on": true, "color": "#3D7FE0"}));
    run(&mut s, "view.focusCell", json!({"on": false}));
    assert_eq!(run(&mut s, "view.focusCell", json!({"color": "#E8A33D"})), json!({"on": false, "color": "#E8A33D"}));
    assert!(s.execute("view.focusCell", json!({"color": "red"})).is_err());
    let d = s.doc().unwrap();
    assert!(!d.is_dirty(), "turning it on and choosing colours changes nothing in the workbook");
    assert_eq!(d.undo.len(), undo);
}
