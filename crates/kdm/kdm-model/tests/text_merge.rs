//! Character-level merges of cell text (plan K5) and the migration of 0.3 replicas (K6): two
//! replicas typing into one text cell at different places and at the same place keep both;
//! concurrent edits of one formula or one number keep one whole edit (never a mix); typing next
//! to a concurrent row insert lands in the cell that moved; a peer's marks on a cell's text stay
//! through the bridge's edits and grow by their `expand`; text typed concurrently with a formula
//! stays out of the view and later edits still write; typing into a cell cleared at the same time
//! survives the clear; a 0.3 replica reads, is rewritten as `Cell` objects on its first write (no undo
//! step), and two sessions migrating it concurrently converge; a peer on schema version 1 is
//! refused a new replica.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, input, insert_rows, peer, put, sync};
use kdm_model::{SheetModel, SheetSession};
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema};
use kition_kit::session::{KitError, KitModel, MemoryBlobs, TextRun};
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

// ---------------------------------------------------------------- 0.3 replicas (K6)

/// The schema 0.3 replicas were written under: version 1, without `Cell`.
fn schema_v1() -> std::sync::Arc<Schema> {
    let mut raw: serde_json::Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap_or_else(|e| panic!("schema: {e}"));
    raw["schemaVersion"] = json!(1);
    raw["types"].as_object_mut().unwrap_or_else(|| panic!("types")).remove("Cell");
    // Version 1 stores inline cell registers, not references to Cell objects.
    raw["types"]["Sheet"]["fields"]["cells"].as_object_mut().unwrap_or_else(|| panic!("cells field")).remove("of");
    std::sync::Arc::new(Schema::parse(&raw.to_string()).unwrap_or_else(|e| panic!("v1 schema: {e}")))
}

/// A replica as Kition Sheet 0.3 wrote it: one sheet `Data`, two rows and two columns, cells as
/// `{input, style?}` registers (a text, a number, a formula, a quoted text).
fn replica_v1() -> Vec<u8> {
    let actor = ActorId::from_u64(7).unwrap_or_else(|_| panic!("actor"));
    let (replica, ()) = Replica::create_with(schema_v1(), DocId::new(), actor, &Origin::system("create"), |tx| {
        let root = ObjectId::from_string("workbook");
        tx.create_with_id("Workbook", &root, serde_json::Map::new())?;
        let sheet = tx.create("Sheet", [("name".to_owned(), json!("Data"))].into_iter().collect())?;
        tx.list_insert(&root, "sheets", 0, &sheet)?;
        let mut rows = Vec::new();
        let mut cols = Vec::new();
        for k in 0..2 {
            let row = tx.create("Row", serde_json::Map::new())?;
            tx.list_insert(&sheet, "rows", k, &row)?;
            rows.push(row);
            let col = tx.create("Col", serde_json::Map::new())?;
            tx.list_insert(&sheet, "cols", k, &col)?;
            cols.push(col);
        }
        let key = |r: usize, c: usize| format!("{}:{}", rows[r].as_str(), cols[c].as_str());
        tx.map_set(&sheet, "cells", &key(0, 0), json!({ "input": "Hello" }))?;
        tx.map_set(&sheet, "cells", &key(0, 1), json!({ "input": "42.0" }))?;
        tx.map_set(&sheet, "cells", &key(1, 0), json!({ "input": "=1+2" }))?;
        tx.map_set(&sheet, "cells", &key(1, 1), json!({ "input": "'12" }))?;
        Ok(())
    })
    .unwrap_or_else(|e| panic!("v1 replica: {e}"));
    replica.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"))
}

fn check_v1_content(wb: &Workbook) {
    let v = |a1: &str| wb.sheets[0].input_text(at(a1));
    assert_eq!((v("A1"), v("B1"), v("A2")), ("Hello".into(), "42".into(), "=1+2".into()));
    assert!(matches!(wb.sheets[0].cell(at("B2")).map(|c| &c.value), Some(Value::Text(t)) if &**t == "12"), "the quoted 12 is text");
}

#[test]
fn a_0_3_replica_reads_and_is_migrated_on_its_first_write() {
    let bytes = replica_v1();
    let mut s = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open: {e}"));
    assert!(!s.migrated());
    check_v1_content(s.workbook());

    let mut after = s.workbook().clone();
    put_text(&mut after, "A1", "Hello there");
    assert_eq!(s.record(&Origin::person("user:a"), &after, &[]).unwrap_or_else(|e| panic!("record: {e}")), 1, "one undo step: the edit");
    assert!(s.migrated());
    let sheet = s.model().ids().sheets[0].object.clone();
    for (key, v) in s.replica().map_entries(&sheet, "cells").unwrap_or_default() {
        let id = ObjectId::from_string(v.as_str().unwrap_or_else(|| panic!("{key} is still a register: {v}")));
        assert_eq!(s.replica().object_type(&id).ok().as_deref(), Some("Cell"));
    }
    let (read, _) = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("read: {e}"));
    assert_eq!(read.sheets[0].input_text(at("A1")), "Hello there");

    // The migration raised the document's minimum schema version (to 4: its shapes and charts are
    // objects too): a 0.3 reader (schema version 1) refuses the migrated replica, and refuses its
    // changes without taking any of them.
    assert_eq!(s.replica().min_schema_version(), 4);
    let actor = ActorId::from_u64(11).unwrap_or_else(|_| panic!("actor"));
    let migrated = s.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    assert!(
        matches!(Replica::load(schema_v1(), &migrated, actor), Err(KdmError::NewerSchema { found: 4, reads: 1 })),
        "a 0.3 kit refuses the migrated replica"
    );
    let mut old = Replica::load(schema_v1(), &bytes, actor).unwrap_or_else(|e| panic!("a 0.3 kit reads the 0.3 replica: {e}"));
    let before = old.state_hash().unwrap_or_else(|e| panic!("hash: {e}"));
    let changes = s.export_changes(&old.version()).unwrap_or_else(|e| panic!("export: {e}"));
    assert!(matches!(old.import_changes(&changes), Err(KdmError::NewerSchema { found: 4, reads: 1 })), "a 0.3 kit refuses the migration");
    assert_eq!(old.state_hash().ok(), Some(before), "the refused import changed nothing");
    assert_eq!(old.min_schema_version(), 1);

    // Undo takes back the edit, not the migration.
    assert!(s.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    assert!(!s.can_undo(), "the migration is no undo step");
    check_v1_content(s.workbook());
    for (_, v) in s.replica().map_entries(&sheet, "cells").unwrap_or_default() {
        assert!(v.is_string(), "the cells stay objects after the undo");
    }
}

#[test]
fn two_sessions_migrating_one_0_3_replica_converge() {
    let bytes = replica_v1();
    let mut a = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open a: {e}"));
    let mut b = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open b: {e}"));
    let mut wa = a.workbook().clone();
    put_text(&mut wa, "A1", "Hello from a");
    a.record(&Origin::person("user:a"), &wa, &[]).unwrap_or_else(|e| panic!("record a: {e}"));
    let mut wb = b.workbook().clone();
    put_text(&mut wb, "A1", "Hi, Hello");
    b.record(&Origin::person("user:b"), &wb, &[]).unwrap_or_else(|e| panic!("record b: {e}"));
    for _ in 0..4 {
        let since = b.version();
        b.import_changes(&a.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("import: {e}"));
        let since = a.version();
        a.import_changes(&b.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("import: {e}"));
    }
    assert_eq!(a.state_hash().ok(), b.state_hash().ok());
    assert_same("migrated twice", a.workbook(), b.workbook());
    // Both migrations created the same objects (ids derived from the cells' keys), so the cell
    // holds one of the two edits, whole, not a mix of two objects.
    let v = a.workbook().sheets[0].input_text(at("A1"));
    assert!(v == "Hello from a" || v == "Hi, Hello", "{v}");
    assert_eq!(a.replica().objects_of_type("Cell").len(), 4, "one object per cell");
}

#[test]
fn a_schema_version_1_peer_is_refused_a_new_replica() {
    let s = SheetSession::create(&base()).unwrap_or_else(|e| panic!("create: {e}"));
    let snapshot = s.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    let actor = ActorId::from_u64(9).unwrap_or_else(|_| panic!("actor"));
    assert!(Replica::load(schema_v1(), &snapshot, actor).is_err(), "a 0.3 kit refuses a schema version 2 replica");
    assert!(Replica::load(SheetModel::schema(), &snapshot, actor).is_ok());
}

/// The `Cell` schema Kition Sheet 0.4.0 and 0.4.1 read: version 2, without `by`, `key` and
/// `merged`.
fn schema_0_4_1() -> std::sync::Arc<Schema> {
    let mut raw: serde_json::Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap_or_else(|e| panic!("schema: {e}"));
    let fields = raw["types"]["Cell"]["fields"].as_object_mut().unwrap_or_else(|| panic!("Cell fields"));
    assert!(fields.remove("by").is_some() && fields.remove("key").is_some() && fields.remove("merged").is_some());
    std::sync::Arc::new(Schema::parse(&raw.to_string()).unwrap_or_else(|e| panic!("0.4.1 schema: {e}")))
}

/// A 0.4.1 peer (no `by`, `key` or `merged` in its schema, same schema version) takes what this
/// version writes and reads it as this version does (cells created ahead with `by` and `key`, a
/// cleared cell kept as an empty object), but for a cell two people typed into at once: it shows
/// the text of the object holding the cell's entry, not the other's (`merged`, which it does not
/// know; nothing is lost, and it reads both once it is updated).
#[test]
fn a_0_4_1_peer_reads_the_same() {
    let mut a = SheetSession::create(&base()).unwrap_or_else(|e| panic!("create: {e}"));
    let blobs = MemoryBlobs(a.blobs_since(&kition_kit::kdm::Version::empty()).into_iter().collect());
    let mut b = SheetSession::open_snapshot(&a.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}")), blobs).unwrap_or_else(|e| panic!("open: {e}"));
    let actor = ActorId::from_u64(41).unwrap_or_else(|_| panic!("actor"));
    let mut old =
        Replica::load(schema_0_4_1(), &a.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}")), actor).unwrap_or_else(|e| panic!("load: {e}"));
    let type_into = |s: &mut SheetSession, f: &dyn Fn(&mut Workbook)| {
        let mut wb = s.workbook().clone();
        f(&mut wb);
        s.record(&Origin::person("user:test"), &wb, &[]).unwrap_or_else(|e| panic!("record: {e}"));
    };
    type_into(&mut a, &|wb| put_text(wb, "D7", "from a"));
    type_into(&mut b, &|wb| put_text(wb, "D7", "from b"));
    type_into(&mut a, &|wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.remove_cell(at("A1"));
        }
    });
    for _ in 0..3 {
        let since = b.version();
        b.import_changes(&a.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("import: {e}"));
        let since = a.version();
        a.import_changes(&b.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("import: {e}"));
    }
    assert_eq!(a.state_hash().ok(), b.state_hash().ok());
    let merged = input(a.workbook(), 0, "D7");
    assert!(merged == "from afrom b" || merged == "from bfrom a", "both texts: {merged:?}");
    old.import_changes(&a.export_changes(&old.version()).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("old import: {e}"));
    let (read, _) = kdm_model::read_document(&old, a.blobs()).unwrap_or_else(|e| panic!("old read: {e}"));
    let shown = input(&read, 0, "D7");
    assert!(shown == "from a" || shown == "from b", "the kept object's text: {shown:?}");
    assert!(merged.starts_with(&shown), "{shown:?} is the kept part of {merged:?}");
    let mut expected = a.workbook().clone();
    put(&mut expected, 0, "D7", &shown);
    assert_same("a 0.4.1 read", &expected, &read);
}
