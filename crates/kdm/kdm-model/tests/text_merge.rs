//! Character-level merges of cell text (plan K5): two
//! replicas typing into one text cell at different places and at the same place keep both;
//! concurrent edits of one formula or one number keep one whole edit (never a mix); typing next
//! to a concurrent row insert lands in the cell that moved; a peer's marks on a cell's text stay
//! through the bridge's edits and grow by their `expand`; text typed concurrently with a formula
//! stays out of the view and later edits still write; typing into a cell cleared at the same time
//! survives the clear.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, input, insert_rows, peer, put, sync};
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
fn typing_at_different_places_of_one_cell_keeps_both() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        put_text(wb, "A1", "Hello brave world");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "A1", "Hello world!");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(data(&a, "A1"), "Hello brave world!");
}

#[test]
fn typing_at_the_same_place_of_one_cell_keeps_both() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        put_text(wb, "A2", "Budget 2026");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "A2", "Budget draft");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let merged = data(&a, "A2");
    assert!(merged == "Budget 2026 draft" || merged == "Budget draft 2026", "both insertions survive, in one order: {merged}");
}

#[test]
fn concurrent_formula_edits_keep_one_whole_formula() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        put(wb, 0, "B2", "=B1*3");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        put(wb, 0, "B2", "=SUM(B1,A9)");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert!(matches!(data(&a, "B2").as_str(), "=B1*3" | "=SUM(B1,A9)"), "never a mix of the two: {}", data(&a, "B2"));
}

#[test]
fn concurrent_number_edits_keep_one_number() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        put(wb, 0, "B1", "130");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        put(wb, 0, "B1", "125");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert!(matches!(data(&a, "B1").as_str(), "130" | "125"), "a number is one value: {}", data(&a, "B1"));
}

#[test]
fn typing_in_a_cell_while_rows_are_inserted_above_it() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| vec![insert_rows(wb, 0, 0, 2)]);
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "A5", "below, edited");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(data(&a, "A7"), "below, edited", "the cell moved down with its row and kept the edit");
    assert_eq!(data(&a, "A3"), "Hello world");
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
    b.record(&Origin::person("user:b"), |tx| tx.text_mark(&object, "text", 4, 11, "italic", json!(true)).map_err(KitError::from))
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

#[test]
fn text_typed_while_a_formula_is_set_stays_unread_and_later_edits_write() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        put(wb, 0, "A2", "=B1+1");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "A2", "Budget!");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    // The formula is the cell (its register is set); the text typed beside it is kept unread.
    assert_eq!(data(&a, "A2"), "=B1+1");
    let object = cell_object(&a, "A2");
    assert_eq!(a.replica().text(&object, "text").unwrap_or_default(), "!", "B's insertion outlived A's deletion of the old text");
    // Both can type text into it again: the writer diffs against what the field holds.
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "A2", "Plan");
        vec![]
    });
    edit(&mut a, &blobs, |wb| {
        put(wb, 0, "B1", "7");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(data(&a, "A2"), "Plan");
}

/// Clearing empties the cell's object and keeps it: typing into the cell at the same time
/// survives the clear (the text the clear saw is deleted, what the other person added stays).
#[test]
fn typing_into_a_cell_cleared_at_the_same_time_survives() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.remove_cell(at("A2"));
        }
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "A2", "Budget plan");
        vec![]
    });
    let object = cell_object(&a, "A2");
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(data(&a, "A2"), " plan", "the typing survives the clear");
    // Typing there again writes into the same object.
    edit(&mut a, &blobs, |wb| {
        put_text(wb, "A2", "again");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(data(&b, "A2"), "again");
    assert_eq!(cell_object(&b, "A2"), object, "one object all along");
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

/// The SDK's generic session (`KitSession`) neither creates cells ahead nor records races: two
/// creations of one cell's object keep one object and its text, and no orphan (`SheetSession`
/// keeps both texts: `tests/session.rs`).
#[test]
fn typing_into_one_empty_cell_on_both_keeps_one_object() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        put_text(wb, "C3", "from a");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        put_text(wb, "C3", "from b");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert!(matches!(data(&a, "C3").as_str(), "from a" | "from b"), "one creation wins: {}", data(&a, "C3"));
    let referenced: std::collections::BTreeSet<String> = a
        .replica()
        .map_entries(&a.model().ids().sheets[0].object, "cells")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v)| v.as_str().map(str::to_owned))
        .collect();
    for cell in a.replica().objects_of_type("Cell") {
        assert!(referenced.contains(cell.as_str()), "no orphan Cell object: {cell}");
    }
}
