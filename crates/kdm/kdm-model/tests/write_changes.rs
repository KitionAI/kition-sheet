//! `write_changes` writes only what differs: one `Cell` object for a cell edit (a text cell's
//! characters edited in place); three `Row` objects and their cells for a three-row insert in a
//! 10,000-row sheet (no formula below rewritten);
//! tombstones for a delete; one register for a feature; nothing for an unchanged view.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{SharedBlobs, at, delete_rows, insert_rows, new_replica, person, put, rich};
use kdm_model::{Ids, StructuralEdit, read_document, write_changes};
use kition_kit::kdm::{ObjectId, Replica};
use kition_sheet_model::{LineInfo, Workbook};
use serde_json::Value;

fn write(replica: &mut Replica, before: &Workbook, after: &Workbook, edits: &[StructuralEdit], ids: &mut Ids, blobs: &SharedBlobs) -> bool {
    let mut sink = blobs.clone();
    replica.transact(&person("t"), |tx| write_changes(tx, before, after, edits, ids, &mut sink)).unwrap_or_else(|error| panic!("write: {error}"))
}

fn cells(replica: &Replica, sheet: &ObjectId) -> BTreeMap<String, Value> {
    replica.map_entries(sheet, "cells").unwrap_or_else(|error| panic!("cells: {error}")).into_iter().collect()
}

#[test]
fn editing_one_cell_writes_its_cell_object_only() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let sheet = ids.sheets[0].object.clone();
    let was = cells(&replica, &sheet);

    let mut after = before.clone();
    put(&mut after, 0, "B3", "8.5");
    assert!(write(&mut replica, &before, &after, &[], &mut ids, &blobs));
    let key = format!("{}:{}", ids.sheets[0].rows[2].as_str(), ids.sheets[0].cols[1].as_str());
    let object = was.get(&key).and_then(Value::as_str).map(ObjectId::from_string).unwrap_or_else(|| panic!("B3 has a Cell object"));
    assert_eq!(replica.take_changed(), BTreeSet::from([object.clone()]), "only B3's Cell changed, not the sheet's map");
    assert_eq!(cells(&replica, &sheet), was, "the map still names the same objects");
    assert_eq!(replica.get(&object, "input").ok().as_ref().and_then(Value::as_str), Some("8.5"));
    assert_eq!(replica.text(&object, "text").unwrap_or_default(), "", "a number is no text");

    // Nothing more to write.
    assert!(!write(&mut replica, &after, &after, &[], &mut ids, &blobs));
}

#[test]
fn editing_a_text_cell_edits_its_characters() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let sheet = ids.sheets[0].object.clone();
    let key = format!("{}:{}", ids.sheets[0].rows[2].as_str(), ids.sheets[0].cols[0].as_str());
    let object = cells(&replica, &sheet).get(&key).and_then(Value::as_str).map(ObjectId::from_string).unwrap_or_else(|| panic!("A3"));
    assert_eq!(replica.text(&object, "text").unwrap_or_default(), "Pears");
    assert!(replica.get(&object, "input").unwrap_or(Value::Null).is_null(), "text is not in the register");

    let mut after = before.clone();
    put(&mut after, 0, "A3", "Green pears");
    assert!(write(&mut replica, &before, &after, &[], &mut ids, &blobs));
    assert_eq!(replica.text(&object, "text").unwrap_or_default(), "Green pears", "the same object, its text edited");
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(common::input(&read, 0, "A3"), "Green pears");

    // Text to a formula and back: the register takes the formula, the text empties.
    let mut formula = after.clone();
    put(&mut formula, 0, "A3", "=B3*2");
    assert!(write(&mut replica, &after, &formula, &[], &mut ids, &blobs));
    assert_eq!(replica.text(&object, "text").unwrap_or_default(), "");
    assert!(replica.get(&object, "input").ok().as_ref().and_then(Value::as_str).is_some_and(|f| f.starts_with('=')));
    let mut text = formula.clone();
    put(&mut text, 0, "A3", "Pears again");
    assert!(write(&mut replica, &formula, &text, &[], &mut ids, &blobs));
    assert_eq!(replica.text(&object, "text").unwrap_or_default(), "Pears again");
    assert!(replica.get(&object, "input").unwrap_or(Value::Null).is_null());
}

/// A sheet of 10,000 rows: values in A and B, a formula in C naming its own row and the row
/// below, row info every 100 rows.
fn big() -> Workbook {
    let mut wb = Workbook::new();
    for r in 1..=10_000u32 {
        put(&mut wb, 0, &format!("A{r}"), &format!("{r}"));
        put(&mut wb, 0, &format!("B{r}"), &format!("item {r}"));
        put(&mut wb, 0, &format!("C{r}"), &format!("=A{r}*2+A{}", r + 1));
    }
    let sheet = wb.sheet_mut(0).unwrap_or_else(|| panic!("sheet"));
    for r in (0..10_000).step_by(100) {
        sheet.rows.insert(r, LineInfo { size: Some(24.0), custom: true, ..Default::default() });
    }
    wb
}

#[test]
fn inserting_three_rows_in_a_10000_row_sheet_writes_three_rows_and_their_cells() {
    let blobs = SharedBlobs::default();
    let before = big();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let sheet = ids.sheets[0].object.clone();
    let rows_before = replica.list(&sheet, "rows").unwrap_or_default();
    assert_eq!(rows_before.len(), 10_001, "rows down to the last one a formula names");
    let was = cells(&replica, &sheet);

    // The engine inserts rows 5001:5003 and the person types in them.
    let mut after = before.clone();
    let edit = insert_rows(&mut after, 0, 5000, 3);
    for r in 5001..=5003 {
        put(&mut after, 0, &format!("A{r}"), "new");
    }
    assert_eq!(common::input(&after, 0, "C5004"), "=A5004*2+A5005", "the engine moved the formulas below");
    let mut sink = blobs.clone();
    let mut elapsed = std::time::Duration::ZERO;
    replica
        .transact(&person("t"), |tx| {
            #[allow(clippy::disallowed_methods)]
            let start = std::time::Instant::now();
            let wrote = write_changes(tx, &before, &after, &[edit], &mut ids, &mut sink);
            elapsed = start.elapsed();
            wrote
        })
        .unwrap_or_else(|error| panic!("write: {error}"));
    eprintln!("a 3-row insert in a 10,000-row sheet: {elapsed:?}");

    let changed = replica.take_changed();
    let new_rows: BTreeSet<ObjectId> = ids.sheets[0].rows[5000..5003].iter().cloned().collect();
    let now = cells(&replica, &sheet);
    let new_cells: BTreeSet<ObjectId> =
        now.iter().filter(|(k, _)| !was.contains_key(*k)).filter_map(|(_, v)| v.as_str().map(ObjectId::from_string)).collect();
    assert_eq!(new_cells.len(), 3, "the three typed cells have objects");
    let mut expected = new_rows.clone();
    expected.insert(sheet.clone());
    expected.extend(new_cells);
    assert_eq!(changed, expected, "the sheet (its rows list and cells map), the three new rows and their cells");

    let mut rows_expected = rows_before.clone();
    for (k, row) in new_rows.iter().enumerate() {
        rows_expected.insert(5000 + k, ids.sheets[0].rows[5000 + k].clone());
        assert!(new_rows.contains(row));
    }
    assert_eq!(replica.list(&sheet, "rows").unwrap_or_default(), rows_expected, "three list inserts; the rows below keep their objects");

    let added: Vec<&String> = now.keys().filter(|k| !was.contains_key(*k)).collect();
    assert_eq!(added.len(), 3, "the three typed cells: {added:?}");
    assert!(added.iter().all(|k| new_rows.iter().any(|r| k.starts_with(&format!("{}:", r.as_str())))));
    for (key, v) in &was {
        assert_eq!(now.get(key), Some(v), "{key} (a cell below the insert, formulas included) was not rewritten");
    }

    // The replica reads back as the engine's view.
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    common::assert_same("after the insert", &after, &read);
    assert_eq!(common::input(&read, 0, "C9003"), "=A9003*2+A9004");
}

#[test]
fn deleting_rows_marks_them_gone_and_drops_their_cells() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let sheet = ids.sheets[0].object.clone();
    let doomed = [ids.sheets[0].rows[2].clone(), ids.sheets[0].rows[3].clone()];
    let rows_before = replica.list(&sheet, "rows").unwrap_or_default();

    let mut after = before.clone();
    let edit = delete_rows(&mut after, 0, 2, 2);
    assert!(write(&mut replica, &before, &after, &[edit], &mut ids, &blobs));
    for row in &doomed {
        assert_eq!(replica.get(row, "gone").unwrap_or(Value::Null), Value::Bool(true), "a deleted row is a tombstone");
        assert!(!ids.sheets[0].rows.contains(row));
    }
    assert_eq!(replica.list(&sheet, "rows").unwrap_or_default(), rows_before, "deleted rows stay in the list");
    let keys = cells(&replica, &sheet);
    assert!(keys.keys().all(|k| doomed.iter().all(|r| !k.starts_with(&format!("{}:", r.as_str())))), "their cells are gone");
    // Every sheet's cells are live (the Summary sheet has its own).
    let all: Vec<Value> = ids.sheets.iter().flat_map(|s| cells(&replica, &s.object).into_values()).collect();
    let live: BTreeSet<&str> = all.iter().filter_map(Value::as_str).collect();
    for cell in replica.objects_of_type("Cell") {
        assert!(live.contains(cell.as_str()), "the deleted rows' Cell objects went with their entries: {cell}");
    }
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    common::assert_same("after the delete", &after, &read);
    assert_eq!(common::input(&read, 0, "A3"), "Figs", "the rows below moved up");
}

#[test]
fn changing_a_merge_writes_the_features_register_only() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let sheet = ids.sheets[0].object.clone();
    let mut after = before.clone();
    after.sheet_mut(0).unwrap_or_else(|| panic!("Data")).merges.push(common::range("E20:F21"));
    assert!(write(&mut replica, &before, &after, &[], &mut ids, &blobs));
    let changed = replica.take_changed();
    // The merge's rows were padded on (rows 20 and 21 had no objects): those are new objects.
    assert!(changed.contains(&sheet));
    for o in &changed {
        let ty = replica.object_type(o).unwrap_or_default();
        assert!(ty == "Sheet" || ty == "Row", "only the sheet and new rows changed, not {ty}");
    }
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert!(read.sheets[0].merges.contains(&common::range("E20:F21")));
}

#[test]
fn a_new_style_is_one_styles_entry() {
    let blobs = SharedBlobs::default();
    let before = rich();
    let (mut replica, mut ids) = new_replica(&before, &blobs);
    replica.take_changed();
    let root = ObjectId::from_string("workbook");
    let styles_before = replica.map_entries(&root, "styles").unwrap_or_default().len();
    let mut after = before.clone();
    let italic = after.styles.derive(kition_sheet_model::StyleId::DEFAULT, |s| s.font.italic = true);
    if let Some(c) = after.sheet_mut(0).and_then(|s| s.cells.get_mut(at("A2"))) {
        c.style = italic;
    }
    assert!(write(&mut replica, &before, &after, &[], &mut ids, &blobs));
    assert_eq!(replica.map_entries(&root, "styles").unwrap_or_default().len(), styles_before + 1);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    let c = read.sheets[0].cell(at("A2")).unwrap_or_else(|| panic!("A2"));
    assert!(read.styles.get(c.style).font.italic);
}
