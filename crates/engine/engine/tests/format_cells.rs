//! Format Cells (Home › Format › Format Cells…, Cmd/Ctrl+1, the cell menu), as the editor's dialog
//! runs it (module M7, cases FC-X…): formats, border presets and Merge Cells in one undo step; run
//! from a menu without them, it asks the editor for its dialog.

use kition_sheet_engine::core::CellRef;
use kition_sheet_engine::model::{BorderStyle, Color, HAlign};
use kition_sheet_engine::{Session, UiRequest};
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

#[test]
fn format_cells_applies_formats_borders_and_merge_in_one_step() {
    let mut s = session();
    s.execute("range.setValues", json!({"range": "B2", "values": [[1234.5, "x"], [2, 3]]})).unwrap();
    s.execute("select.set", json!({"range": "B2:C3"})).unwrap();
    s.execute(
        "format.cells",
        json!({
            "style": {"numFmt": "#,##0.00", "font": {"bold": true, "color": "#C00000"}, "align": {"h": "Center"}, "fill": "#FFF2CC"},
            "edges": [{"preset": "outside", "style": "medium", "color": "#1F4E79"}, {"preset": "inside", "style": "thin"}],
        }),
    )
    .unwrap();
    let d = s.doc().unwrap();
    let sheet = d.wb.active().unwrap();
    let b2 = d.wb.styles.get(sheet.style_id(CellRef::parse("B2").unwrap())).clone();
    assert_eq!(b2.num_fmt.as_str(), "#,##0.00", "FC-X2");
    assert!(b2.font.bold && b2.font.color == Color::Rgb(0xC00000), "FC-X4");
    assert_eq!(b2.align.h, HAlign::Center, "FC-X3");
    assert_eq!(b2.borders.top.style, BorderStyle::Medium, "FC-X5: the outline");
    assert_eq!(b2.borders.right.style, BorderStyle::Thin, "FC-X5: inside");
    assert_eq!(b2.fill.fg, Color::Rgb(0xFFF2CC), "FC-X6");
    let undo = d.undo.len();
    s.execute("format.cells", json!({"range": "E5:F6", "merge": true})).unwrap();
    assert_eq!(s.doc().unwrap().wb.active().unwrap().merges.len(), 1, "FC-X3: Merge Cells");
    assert_eq!(s.doc().unwrap().undo.len(), undo + 1);
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    let d = s.doc().unwrap();
    let b2 = d.wb.styles.get(d.wb.active().unwrap().style_id(CellRef::parse("B2").unwrap())).clone();
    assert!(!b2.font.bold && b2.borders.top.style == BorderStyle::None, "one undo step takes all of it back");
    // From a menu without values, the editor is asked for the dialog.
    s.take_ui_requests();
    s.execute("format.cells", json!({})).unwrap();
    assert_eq!(s.take_ui_requests(), vec![UiRequest::Dialog("formatCells".into(), json!({}))], "FC-X1");
}
