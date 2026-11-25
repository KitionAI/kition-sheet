//! Cell text through the KDM (plan K5): a peer's marks on a cell's text stay through the
//! bridge's edits and grow by their `expand`; bold and italic marked on overlapping ranges both
//! apply; a cleared cell nobody types into is no cell.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, input, peer, put, sync};
use kition_kit::kdm::{ObjectId, Origin};
use kition_kit::session::{KitError, TextRun};
use kition_sheet_core::Value;
use kition_sheet_model::{Cell, Workbook};
use serde_json::json;

/// `Data`: a few text cells, a number and a formula.
fn base() -> Workbook {
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.name = "Data".into();
    }
    put(&mut wb, 0, "A1", "Hello world");
    put(&mut wb, 0, "A2", "Budget");
    put(&mut wb, 0, "B1", "120");
    put(&mut wb, 0, "B2", "=B1*2");
    put(&mut wb, 0, "A5", "below");
    wb
}

fn pair() -> (SharedBlobs, Session, Session) {
    let blobs = SharedBlobs::default();
    let a = first(&base(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().workbook, &b.model().workbook);
    assert_workbook_valid(&a.model().workbook);
    assert_replica_clean(a.replica());
}

fn data(s: &Session, a1: &str) -> String {
    input(&s.model().workbook, 0, a1)
}

/// Sets `a1` to the text `text` exactly (no number or formula parsing), keeping its style and,
/// as the engine's `cell.set` does, its rich text runs (`RichText::retext`).
fn put_text(wb: &mut Workbook, a1: &str, text: &str) {
    let sh = wb.sheet_mut(0).unwrap_or_else(|| panic!("sheet"));
    let c = at(a1);
    let style = sh.cells.get(c).map(|x| x.style).unwrap_or_default();
    let rich = sh.cells.get(c).and_then(Cell::runs).and_then(|r| r.retext(text)).map(std::sync::Arc::new);
    sh.set_cell(c, Cell { value: Value::Text(text.into()), formula: None, style, rich });
}

/// The `Cell` object of `a1` on the first sheet.
fn cell_object(s: &Session, a1: &str) -> ObjectId {
    let c = at(a1);
    let ids = s.model().ids();
    let sheet = &ids.sheets[0];
    let key = format!("{}:{}", sheet.rows[c.row as usize].as_str(), sheet.cols[c.col as usize].as_str());
    let v = s.replica().map_get(&sheet.object, "cells", &key).unwrap_or_else(|e| panic!("cells: {e}"));
    ObjectId::from_string(v.as_str().unwrap_or_else(|| panic!("{a1} names a Cell object, not {v}")))
}

#[test]
fn a_peers_marks_stay_through_edits_and_grow_by_expand() {
    let (blobs, mut a, mut b) = pair();
    let object = cell_object(&a, "A1");
    // A peer marks "Hello" bold.
    a.record(&Origin::person("user:rich"), |tx| tx.text_mark(&object, "text", 0, 5, "bold", json!(true)).map_err(KitError::from))
        .unwrap_or_else(|e| panic!("mark: {e}"));
    sync(&mut a, &mut b);
    // The other types right after "Hello" and at the end: the bold run grows by `expand: after`
    // where it ends, the rest stays plain.
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "A1", "Hello, world.");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let runs = a.replica().text_delta(&object, "text").unwrap_or_else(|e| panic!("delta: {e}"));
    assert_eq!(runs, vec![TextRun::new("Hello,").with("bold", json!(true)), TextRun::new(" world.")]);
    assert_eq!(data(&a, "A1"), "Hello, world.", "the view reads the text");
    let bold = kition_sheet_model::RunFont { bold: Some(true), ..Default::default() };
    let view = a.model().workbook.sheets[0].cell(at("A1")).and_then(Cell::runs).map(|r| r.runs.clone());
    assert_eq!(view, Some(vec![kition_sheet_model::TextRun::new("Hello,", bold), kition_sheet_model::TextRun::new(" world.", Default::default())]));
}

#[test]
fn bold_and_italic_on_overlapping_ranges_both_apply() {
    let (_blobs, mut a, mut b) = pair();
    let object = cell_object(&a, "A1");
    a.record(&Origin::person("user:a"), |tx| tx.text_mark(&object, "text", 0, 7, "bold", json!(true)).map_err(KitError::from))
        .unwrap_or_else(|e| panic!("bold: {e}"));
    a.record(&Origin::person("user:b"), |tx| tx.text_mark(&object, "text", 4, 11, "italic", json!(true)).map_err(KitError::from))
        .unwrap_or_else(|e| panic!("italic: {e}"));
    sync(&mut a, &mut b);
    assert_eq!(a.state_hash().ok(), b.state_hash().ok());
    let runs = a.replica().text_delta(&object, "text").unwrap_or_else(|e| panic!("delta: {e}"));
    assert_eq!(
        runs,
        vec![
            TextRun::new("Hell").with("bold", json!(true)),
            TextRun::new("o w").with("bold", json!(true)).with("italic", json!(true)),
            TextRun::new("orld").with("italic", json!(true)),
        ]
    );
}

/// A cleared cell nobody types into is no cell, on both sides, and saves as such.
#[test]
fn a_cleared_cell_is_no_cell() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.remove_cell(at("B2"));
            s.remove_cell(at("A1"));
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert!(s.model().workbook.sheets[0].cells.get(at("A1")).is_none());
        assert!(s.model().workbook.sheets[0].cells.get(at("B2")).is_none());
        let fresh = kdm_model::read_document(s.replica(), &blobs).unwrap_or_else(|e| panic!("read: {e}")).0;
        assert_same("a fresh read", &s.model().workbook, &fresh);
    }
    let mut expected = base();
    if let Some(s) = expected.sheet_mut(0) {
        s.remove_cell(at("B2"));
        s.remove_cell(at("A1"));
    }
    assert_same("the workbook", &a.model().workbook, &expected);
}
