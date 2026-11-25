//! Small handler defects found during the C8-sheet-read independent reading.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::{DocState, Session};
use serde_json::{Value, json};

fn run(session: &mut Session, command: &str, params: Value) -> Value {
    session.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn reopen(session: &Session) -> Session {
    let bytes = kition_sheet_engine::io::save_bytes(&session.doc().unwrap().wb, "Book.xlsx").unwrap();
    let (workbook, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &bytes).unwrap();
    let mut opened = Session::new();
    opened.add_document(DocState::new(workbook, None, "Book".into()));
    opened
}

#[test]
fn custom_view_show_restores_saved_frozen_or_unfrozen_panes_after_reopen() {
    let mut session = Session::new();
    session.new_workbook();
    run(&mut session, "view.freezePanes", json!({"cell":"C4"}));
    run(&mut session, "view.customViews", json!({"save":"Frozen"}));
    run(&mut session, "view.unfreezePanes", json!({}));
    run(&mut session, "view.customViews", json!({"save":"Unfrozen"}));
    run(&mut session, "view.freezePanes", json!({"cell":"B2"}));
    let mut session = reopen(&session);

    run(&mut session, "view.customViews", json!({"show":"Frozen"}));
    assert_eq!(session.doc().unwrap().wb.active().unwrap().freeze, Some((3, 2)), "show restores the saved pane counts");
    run(&mut session, "view.customViews", json!({"show":"Unfrozen"}));
    assert_eq!(session.doc().unwrap().wb.active().unwrap().freeze, None, "a saved unfrozen view clears the current freeze");
}

#[test]
fn arrange_rotate_honors_flip_without_swapping_the_frame_and_can_toggle_it_back() {
    let mut session = Session::new();
    session.new_workbook();
    run(&mut session, "shape.insert", json!({"kind":"triangle","at":"C3","width":120,"height":60}));
    let original = session.doc().unwrap().wb.active().unwrap().shapes[0].clone();
    for (flip, horizontal, vertical) in [("horizontal", true, false), ("vertical", true, true), ("horizontal", false, true)] {
        run(&mut session, "arrange.rotate", json!({"kind":"shape","id":original.id,"flip":flip}));
        let shape = &session.doc().unwrap().wb.active().unwrap().shapes[0];
        assert_eq!(shape.anchor, original.anchor, "a {flip} flip retains the shape's frame");
        assert_eq!((shape.flip_h, shape.flip_v), (horizontal, vertical));
    }
    let mut session = reopen(&session);
    let saved = &session.doc().unwrap().wb.active().unwrap().shapes[0];
    assert_eq!((saved.flip_h, saved.flip_v), (false, true), "the flip survives XLSX save and reopen");
    run(&mut session, "arrange.rotate", json!({"kind":"shape","id":original.id}));
    let shape = &session.doc().unwrap().wb.active().unwrap().shapes[0];
    assert_eq!((shape.anchor.width, shape.anchor.height), (60.0, 120.0), "the default turn still swaps width and height");
    assert_eq!((shape.flip_h, shape.flip_v), (false, true));
}

#[test]
fn table_range_import_reads_the_named_source_sheet_and_keeps_its_name() {
    for params in [json!({"range":"Sheet2!A1:B3"}), json!({"range":"Sheet2!A1"}), json!({"sheet":"Sheet2","range":"A1:B3"})] {
        let mut session = Session::new();
        session.new_workbook();
        run(&mut session, "range.setValues", json!({"range":"A1","values":[["Item","Amount"],["left one",1],["left two",2]]}));
        run(&mut session, "sheet.insert", json!({"name":"Sheet2","before":9}));
        run(&mut session, "range.setValues", json!({"range":"A1","values":[["Item","Amount"],["right one",100],["right two",200]]}));
        run(&mut session, "sheet.activate", json!({"sheet":"Sheet1"}));

        let result = run(&mut session, "data.fromTableRange", params.clone());
        assert_eq!(result["sheet"], json!("Sheet2 (2)"), "{params}: the imported sheet name comes from the resolved source");
        assert_eq!((result["rows"].clone(), result["cols"].clone()), (json!(3), json!(2)));
        let saved = reopen(&session);
        let wb = &saved.doc().unwrap().wb;
        assert_eq!(wb.sheets.len(), 3);
        for (sheet, label, amount) in [("Sheet1", "left one", 1.0), ("Sheet2", "right one", 100.0), ("Sheet2 (2)", "right one", 100.0)] {
            let source = wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap();
            assert_eq!(source.value(kition_sheet_engine::core::CellRef::new(1, 0)).display(), label);
            assert_eq!(source.value(kition_sheet_engine::core::CellRef::new(1, 1)), kition_sheet_engine::core::Value::Number(amount));
        }
    }
}
