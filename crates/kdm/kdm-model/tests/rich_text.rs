//! Rich text cells through the KDM (plan K5): a cell's runs are marks on its `text` field, one key
//! per run property. A rich cell reads back with every property; two people formatting different
//! runs of one cell both keep theirs; typing into a rich cell merges by character and typed
//! characters take the run at the caret; replacing the text with plain text unmarks it; and a
//! 0.4.x peer, whose schema lacks the newer mark keys (`family`, `charset`, `scheme`, `extra`),
//! reads the cell with the marks it knows and its typing leaves the others in place.

mod common;

use std::sync::Arc;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, peer, rich_runs, sync};
use kition_kit::kdm::{ActorId, ObjectId, Origin, Replica, Schema};
use kition_kit::session::{KitModel, TextRun as Marked};
use kition_sheet_core::Value;
use kition_sheet_model::{Cell, RichText, RunColor, RunFont, StyleId, TextRun, Workbook};
use serde_json::json;

fn base() -> Workbook {
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.set_cell(at("A1"), Cell::rich_text(rich_runs(), StyleId::DEFAULT));
        s.set_cell(at("A2"), Cell::value(Value::text("plain")));
    }
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

fn cell(s: &Session, a1: &str) -> Cell {
    s.model().workbook.sheets[0].cell(at(a1)).cloned().unwrap_or_else(|| panic!("no cell {a1}"))
}

fn runs(s: &Session, a1: &str) -> Vec<TextRun> {
    cell(s, a1).runs().map(|r| r.runs.clone()).unwrap_or_default()
}

fn font(runs: &[TextRun], at: usize) -> RunFont {
    runs.get(at).map(|r| r.font.clone()).unwrap_or_default()
}

/// Sets `a1`'s runs: `f` edits a copy of the cell's runs.
fn restyle(wb: &mut Workbook, a1: &str, f: impl FnOnce(&mut Vec<TextRun>)) {
    let sheet = wb.sheet_mut(0).unwrap_or_else(|| panic!("sheet"));
    let c = sheet.cells.get_mut(at(a1)).unwrap_or_else(|| panic!("cell {a1}"));
    let mut runs = c.rich.as_ref().map(|r| r.runs.clone()).unwrap_or_else(|| vec![TextRun::new(c.value.display(), RunFont::default())]);
    f(&mut runs);
    c.rich = RichText::new(runs).map(Arc::new);
}

/// Types `text` over `a1` as the engine's `cell.set` does: the runs follow the edit.
fn type_text(wb: &mut Workbook, a1: &str, text: &str) {
    let sheet = wb.sheet_mut(0).unwrap_or_else(|| panic!("sheet"));
    let c = sheet.cells.get_mut(at(a1)).unwrap_or_else(|| panic!("cell {a1}"));
    c.rich = c.runs().and_then(|r| r.retext(text)).map(Arc::new);
    c.value = Value::text(text);
}

/// Splits run `k` at character `at`, so the part after can be formatted on its own.
fn split(runs: &mut Vec<TextRun>, k: usize, at: usize) {
    let run = runs.remove(k);
    let (head, tail): (String, String) = (run.text.chars().take(at).collect(), run.text.chars().skip(at).collect());
    runs.insert(k, TextRun::new(tail, run.font.clone()));
    runs.insert(k, TextRun::new(head, run.font));
}

#[test]
fn a_rich_cell_reads_back_with_every_property() {
    let blobs = SharedBlobs::default();
    let s = first(&common::rich(), &blobs);
    let (read, _) = kdm_model::read_document(s.replica(), &blobs).unwrap_or_else(|e| panic!("read: {e}"));
    assert_same("rich text round trip", &common::rich(), &read);
    let c = read.sheets[0].cell(at("E6")).unwrap_or_else(|| panic!("E6"));
    assert_eq!(c.runs(), Some(&rich_runs()));

    // In the replica: one mark per property, the newer ones included.
    let s = first(&base(), &blobs);
    let marked = s.replica().text_delta(&object_of(&s, "A1"), "text").unwrap_or_else(|e| panic!("delta: {e}"));
    assert_eq!(marked.len(), 3);
    let keys: Vec<&str> = marked[0].attributes.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        ["bold", "charset", "color", "extra", "family", "font", "italic", "scheme", "size", "strike", "underline", "vertAlign"],
        "every property of the first run"
    );
    assert_eq!(marked[0].attributes.get("extra"), Some(&json!({ "outline": true, "shadow": false, "condense": true, "extend": false })));
    assert!(marked[2].attributes.is_empty(), "the plain run");
}

fn object_of(s: &Session, a1: &str) -> ObjectId {
    let c = at(a1);
    let ids = s.model().ids();
    let sheet = &ids.sheets[0];
    let key = format!("{}:{}", sheet.rows[c.row as usize].as_str(), sheet.cols[c.col as usize].as_str());
    let v = s.replica().map_get(&sheet.object, "cells", &key).unwrap_or_else(|e| panic!("cells: {e}"));
    ObjectId::from_string(v.as_str().unwrap_or_else(|| panic!("{a1} names a Cell object, not {v}")))
}

#[test]
fn two_people_formatting_different_runs_of_one_cell_both_apply() {
    let (blobs, mut a, mut b) = pair();
    // A makes "Total" bold (the first run, which said not bold); B colours "now" blue.
    edit(&mut a, &blobs, |wb| {
        restyle(wb, "A1", |runs| {
            split(runs, 0, 5);
            runs[0].font.bold = Some(true);
        });
        vec![]
    });
    let blue = RunColor { rgb: Some("FF0000FF".into()), ..RunColor::default() };
    edit(&mut b, &blobs, |wb| {
        restyle(wb, "A1", |runs| {
            split(runs, 2, 1);
            runs[3].font.color = Some(blue.clone());
        });
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let got = runs(&a, "A1");
    assert_eq!(got.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["Total", " ", "due", " ", "now"]);
    let first = rich_runs().runs[0].font.clone();
    assert_eq!(font(&got, 0), RunFont { bold: Some(true), ..first.clone() }, "A's bold");
    assert_eq!(font(&got, 1), first, "the rest of the first run as it was");
    assert_eq!(font(&got, 2), rich_runs().runs[1].font, "B left the middle run alone");
    assert_eq!(font(&got, 4), RunFont { color: Some(blue), ..RunFont::default() }, "B's colour");
    assert_eq!(cell(&a, "A1").value, Value::text("Total due now"));
}

#[test]
fn two_people_formatting_one_run_differently_keep_both_properties() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        restyle(wb, "A1", |runs| runs[1].font.italic = Some(true));
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        restyle(wb, "A1", |runs| runs[1].font.strike = Some(true));
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let got = runs(&a, "A1");
    assert_eq!(font(&got, 1), RunFont { italic: Some(true), strike: Some(true), ..rich_runs().runs[1].font.clone() });
}

#[test]
fn typing_into_a_rich_cell_merges_by_character_and_takes_the_run_at_the_caret() {
    let (blobs, mut a, mut b) = pair();
    // A types after "due" (the red run's end); B types at the end of the plain run.
    edit(&mut a, &blobs, |wb| {
        type_text(wb, "A1", "Total due!! now");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        type_text(wb, "A1", "Total due now, please");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(cell(&a, "A1").value, Value::text("Total due!! now, please"), "both typed");
    let got = runs(&a, "A1");
    let expected = rich_runs();
    assert_eq!(
        got,
        vec![
            TextRun::new("Total ", expected.runs[0].font.clone()),
            TextRun::new("due!!", expected.runs[1].font.clone()),
            TextRun::new(" now, please", RunFont::default()),
        ]
    );
}

#[test]
fn typing_at_the_same_place_of_a_rich_cell_keeps_both() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        type_text(wb, "A1", "Total amount due now");
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        type_text(wb, "A1", "Total sum due now");
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let text = cell(&a, "A1").value.display();
    assert!(text == "Total amount sum due now" || text == "Total sum amount due now", "{text}");
    // Both insertions follow "Total ": the first run's font.
    let got = runs(&a, "A1");
    assert!(got[0].text.ends_with("sum ") || got[0].text.ends_with("amount "), "{got:?}");
    assert_eq!(font(&got, 0), rich_runs().runs[0].font);
}

#[test]
fn plain_text_over_a_rich_cell_unmarks_it_and_new_runs_mark_a_plain_cell() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.set_cell(at("A1"), Cell::value(Value::text("Total due now")));
        }
        restyle(wb, "A2", |runs| {
            split(runs, 0, 2);
            runs[0].font.underline = Some(kition_sheet_model::Underline::Single);
        });
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(cell(&b, "A1").rich, None, "the text is plain again");
    assert_eq!(
        runs(&b, "A2"),
        vec![
            TextRun::new("pl", RunFont { underline: Some(kition_sheet_model::Underline::Single), ..RunFont::default() }),
            TextRun::new("ain", RunFont::default()),
        ]
    );
    let delta = b.replica().text_delta(&object_of(&b, "A1"), "text").unwrap_or_else(|e| panic!("delta: {e}"));
    assert_eq!(delta, vec![Marked::new("Total due now")], "no mark left behind");
}

/// The `Cell` schema of Kition Sheet 0.4.x: no `family`, `charset`, `scheme` or `extra` marks,
/// the same schema version.
fn schema_0_4() -> Arc<Schema> {
    let mut raw: serde_json::Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap_or_else(|e| panic!("schema: {e}"));
    let marks = raw["types"]["Cell"]["fields"]["text"]["marks"].as_object_mut().unwrap_or_else(|| panic!("marks"));
    for key in ["family", "charset", "scheme", "extra"] {
        assert!(marks.remove(key).is_some(), "{key} is declared");
    }
    Arc::new(Schema::parse(&raw.to_string()).unwrap_or_else(|e| panic!("0.4 schema: {e}")))
}

#[test]
fn a_0_4_peer_reads_rich_cells_and_its_typing_keeps_the_newer_marks() {
    let (_blobs, a, _) = pair();
    let actor = ActorId::from_u64(44).unwrap_or_else(|_| panic!("actor"));
    let snapshot = a.replica().snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    let mut old = Replica::load(schema_0_4(), &snapshot, actor).unwrap_or_else(|e| panic!("a 0.4 kit opens the replica: {e}"));
    assert_eq!(old.min_schema_version(), a.replica().min_schema_version(), "no schema version raised");

    // It reads the marks it declares, and the text.
    let object = object_of(&a, "A1");
    let seen = old.text_delta(&object, "text").unwrap_or_else(|e| panic!("old delta: {e}"));
    assert_eq!(seen.iter().map(|r| r.insert.as_str()).collect::<String>(), "Total due now");
    assert!(seen[0].attributes.contains_key("bold") && !seen[0].attributes.contains_key("family"), "{seen:?}");
    let (read, _) = kdm_model::read_document(&old, a.blobs()).unwrap_or_else(|e| panic!("old read: {e}"));
    let first = read.sheets[0].cell(at("A1")).and_then(Cell::runs).map(|r| r.runs[0].font.clone()).unwrap_or_default();
    let known = RunFont {
        family: None,
        charset: None,
        scheme: None,
        outline: None,
        shadow: None,
        condense: None,
        extend: None,
        ..rich_runs().runs[0].font.clone()
    };
    assert_eq!(first, known, "the properties it knows");

    // It types into the first run (a 0.4 bridge writes characters, then its own marks).
    old.transact(&Origin::person("user:old"), |tx| tx.text_insert(&object, "text", 5, " sum")).unwrap_or_else(|e| panic!("old typing: {e}"));
    let mut new = Replica::load(kdm_model::SheetModel::schema(), &snapshot, ActorId::from_u64(45).unwrap_or_else(|_| panic!("actor")))
        .unwrap_or_else(|e| panic!("load: {e}"));
    new.import_changes(&old.export_changes(&new.version()).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("import: {e}"));
    let (read, _) = kdm_model::read_document(&new, a.blobs()).unwrap_or_else(|e| panic!("read: {e}"));
    let c = read.sheets[0].cell(at("A1")).unwrap_or_else(|| panic!("A1"));
    assert_eq!(c.value, Value::text("Total sum due now"));
    let got = c.runs().map(|r| r.runs.clone()).unwrap_or_default();
    assert_eq!(got[0], TextRun::new("Total sum ", rich_runs().runs[0].font.clone()), "the newer marks cover the old peer's typing too");
}
