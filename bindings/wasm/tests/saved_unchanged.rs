//! A workbook saved twice without a change in between writes the same bytes, and the file opened
//! again (no stored replica: a new replica of the file) saves those bytes again and keeps the
//! object ids of its sheets, rows, columns and cells (the file's `xl/kition/identities.json`).
//! Plan 2026-11-24, plain model, section 9.3.

use std::collections::{BTreeMap, BTreeSet};

use kdm_model::SheetModel;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::KitModel;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

const NAME: &str = "Unchanged.xlsx";

fn run(doc: &mut SheetDocument, command: &str, params: serde_json::Value) {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"));
}

/// The ids of the replica's objects of each kind that the file keeps.
fn ids(doc: &mut SheetDocument) -> BTreeMap<&'static str, BTreeSet<String>> {
    doc.version().unwrap_or_else(|error| panic!("version: {error}"));
    let snapshot = doc.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    let replica = Replica::load(SheetModel::schema(), &snapshot, ActorId::random()).unwrap_or_else(|error| panic!("load: {error}"));
    ["Sheet", "Row", "Col", "Cell"]
        .into_iter()
        .map(|ty| (ty, replica.objects_of_type(ty).into_iter().map(|id| id.as_str().to_owned()).collect()))
        .collect()
}

#[test]
fn an_unchanged_workbook_saves_the_same_bytes_and_reopens_with_the_same_object_ids() {
    let mut doc = SheetDocument::create(NAME, 0, 0).unwrap_or_else(|error| panic!("create: {error}"));
    for (cell, input) in [("A1", "Item"), ("B1", "Cost"), ("A2", "Rent"), ("B2", "1200"), ("A3", "Food"), ("B3", "450"), ("B4", "=SUM(B2:B3)")] {
        run(&mut doc, "cell.set", json!({ "cell": cell, "input": input }));
    }
    run(&mut doc, "format.bold", json!({ "range": "A1:B1", "on": true }));
    run(&mut doc, "row.insert", json!({ "rows": "2:2" }));
    run(&mut doc, "cell.set", json!({ "cell": "A2", "input": "Inserted" }));
    run(&mut doc, "sheet.insert", json!({ "name": "Notes" }));
    run(&mut doc, "cell.set", json!({ "cell": "C3", "input": "on the second sheet" }));

    let saved = doc.save(Some(NAME)).unwrap_or_else(|error| panic!("save: {error}"));
    assert_eq!(
        doc.save(Some(NAME)).unwrap_or_else(|error| panic!("save again: {error}")),
        saved,
        "saving again without a change writes the same bytes"
    );
    let before = ids(&mut doc);
    assert_eq!(before["Sheet"].len(), 2, "{before:?}");
    assert!(before["Cell"].len() >= 9, "{before:?}");

    let mut reopened = SheetDocument::open(NAME, &saved).unwrap_or_else(|error| panic!("reopen: {error}"));
    assert_eq!(
        reopened.save(Some(NAME)).unwrap_or_else(|error| panic!("save the reopened: {error}")),
        saved,
        "the reopened workbook, unchanged, saves the same bytes"
    );
    assert_eq!(ids(&mut reopened), before, "the reopened workbook's objects keep their ids");
}
