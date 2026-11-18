//! C8-sheet-read: month patterns use the same calendar for detection and generation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::Session;
use kition_sheet_engine::core::{CellRef, DateSystem, Value, date::serial_from_ymd};
use serde_json::json;

#[test]
fn month_autofill_uses_the_workbooks_date_system() {
    for system in [DateSystem::D1900, DateSystem::D1904] {
        let mut session = Session::new();
        session.new_workbook();
        std::sync::Arc::make_mut(&mut session.doc_mut().unwrap().wb).date_system = system;
        let serial = |month, day| serial_from_ymd(system, 2024, month, day).unwrap();
        session.execute("range.setValues", json!({"range":"A1","values":[[serial(2,1)],[serial(3,1)]]})).unwrap();
        session.execute("home.numberFormat", json!({"range":"A1:A3","format":"m/d/yyyy"})).unwrap();
        session.execute("edit.autoFill", json!({"source":"A1:A2","target":"A1:A3"})).unwrap();
        assert_eq!(session.doc().unwrap().wb.active().unwrap().value(CellRef::new(2, 0)), Value::number(serial(4, 1)), "{system:?}");
    }
}
