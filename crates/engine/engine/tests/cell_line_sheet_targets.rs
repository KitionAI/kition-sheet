//! C8-sheet-read: whole-line cell operations keep the explicitly resolved sheet.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::{Session, core::CellRef};
use serde_json::json;

#[test]
fn whole_line_cell_operations_keep_the_named_sheet() {
    let mut wrong = Vec::new();
    for (command, shift, expected) in [
        ("cell.insert", "row", ["", "", "target-a", "target-b"]),
        ("cell.insert", "column", ["", "target-a", "", "target-c"]),
        ("cell.delete", "row", ["target-c", "target-d", "", ""]),
        ("cell.delete", "column", ["target-b", "", "target-d", ""]),
    ] {
        for qualified in [false, true] {
            let mut session = Session::new();
            session.new_workbook();
            session.execute("range.setValues", json!({"range":"A1","values":[["keep-a","keep-b"],["keep-c","keep-d"]]})).unwrap();
            session.execute("sheet.insert", json!({"name":"Target"})).unwrap();
            session.execute("range.setValues", json!({"range":"A1","values":[["target-a","target-b"],["target-c","target-d"]]})).unwrap();
            session.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
            let mut params = json!({"range":"A1","shift":shift});
            if qualified {
                params["range"] = json!("Target!A1");
            } else {
                params["sheet"] = json!("Target");
            }
            session.execute(command, params).unwrap();
            for (sheet, wanted) in [("Sheet1", ["keep-a", "keep-b", "keep-c", "keep-d"]), ("Target", expected)] {
                let wb = &session.doc().unwrap().wb;
                let sheet = wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap();
                let actual: Vec<_> = ["A1", "B1", "A2", "B2"].into_iter().map(|at| sheet.input_text(CellRef::parse(at).unwrap())).collect();
                if actual != wanted {
                    wrong.push(format!("{command}/{shift}/qualified={qualified}/{}: {actual:?} != {wanted:?}", sheet.name));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
