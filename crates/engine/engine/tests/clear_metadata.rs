//! Sparse metadata remains clearable outside the extent of stored cells.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_sheet_engine::{Session, core::CellRef, model::Hyperlink};
use serde_json::json;

#[test]
fn clear_metadata_does_not_clip_to_the_used_cell_range() {
    for command in ["edit.clearComments", "edit.clearHyperlinks", "edit.clearFormats", "edit.clearAll"] {
        let mut session = Session::new();
        session.new_workbook();
        session.execute("cell.set", json!({"cell":"A1","input":"kept"})).unwrap();
        session.execute("note.add", json!({"cell":"Z20","text":"outside cell extent"})).unwrap();
        session.execute("cell.mergeAcross", json!({"range":"Z20:AA21"})).unwrap();
        let at = CellRef::parse("Z20").unwrap();
        std::sync::Arc::make_mut(&mut session.doc_mut().unwrap().wb)
            .sheet_mut(0)
            .unwrap()
            .hyperlinks
            .insert(at, Hyperlink::new("https://example.org".into(), None));
        assert_eq!(session.doc().unwrap().wb.sheets[0].used_range().unwrap().a1(), "A1");
        session.execute(command, json!({"range":"Z20:AA21"})).unwrap();
        let sheet = &session.doc().unwrap().wb.sheets[0];
        assert_eq!(sheet.comments.contains_key(&at), !matches!(command, "edit.clearComments" | "edit.clearAll"), "{command}");
        assert_eq!(sheet.hyperlinks.contains_key(&at), !matches!(command, "edit.clearHyperlinks" | "edit.clearAll"), "{command}");
        assert_eq!(sheet.merges.is_empty(), matches!(command, "edit.clearFormats" | "edit.clearAll"), "{command}");
        assert_eq!(sheet.input_text(CellRef::new(0, 0)), "kept", "{command}");
    }
}
