//! A wrapper's fixed destination does not inherit its delegate's selection default.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::{Session, core::CellRef};
use serde_json::json;

#[test]
fn background_picture_is_independent_of_the_current_selection() {
    for selection in ["C5", "H12:J14"] {
        let mut session = Session::new();
        session.new_workbook();
        session.execute("select.set", json!({"range":selection})).unwrap();
        // Synthetic bytes are accepted by the engine's floating-picture branch.
        // This tests the engine handler directly: the public background schema
        // accepts a host path, and does not expose the base64 engine parameter.
        session.execute("page.background", json!({"base64":"AA=="})).unwrap();
        let image = session.doc().unwrap().wb.active().unwrap().images.first().unwrap();
        assert_eq!(image.anchor.cell, CellRef::new(0, 0));
        assert_eq!(image.alt, "Sheet background");
        assert_eq!(session.doc().unwrap().selection.current().a1(), selection);
    }
}
