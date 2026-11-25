//! The private fact holder (an `AxisFacts` object, tool id `axis:all`, named by the workbook's
//! `axisFacts`) is part of the workbook from its first replica: undo and redo of every edit keep
//! it, and a replica following the workbook on one line of changes holds the same state after each
//! (case regressions/issue-c8-sheet-pin-fact-holder-undo).

use kdm_model::SheetModel;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::KitModel;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

const NAME: &str = "Facts.xlsx";

fn run(doc: &mut SheetDocument, command: &str, params: Value) {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"));
}

/// The replica's objects, by id.
fn objects(doc: &mut SheetDocument) -> Value {
    let snapshot = doc.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    let replica = Replica::load(SheetModel::schema(), &snapshot, ActorId::random()).unwrap_or_else(|error| panic!("load: {error}"));
    replica.state_json().unwrap_or_else(|error| panic!("state: {error}"))["objects"].clone()
}

/// The object the workbook names as its fact holder, with its id.
fn holder(objects: &Value) -> (String, Value) {
    let id =
        objects["workbook"]["axisFacts"].as_str().unwrap_or_else(|| panic!("the workbook names a fact holder: {}", objects["workbook"])).to_owned();
    let object = objects[id.as_str()].clone();
    (id, object)
}

/// Brings `follower` to `doc`'s version through `doc`'s changes since its own.
fn follow(doc: &mut SheetDocument, follower: &mut SheetDocument) {
    let since = follower.version().unwrap_or_else(|error| panic!("version: {error}"));
    follower.add_blobs(&doc.blobs_since(&since).unwrap_or_else(|error| panic!("blobs: {error}"))).unwrap_or_else(|error| panic!("add: {error}"));
    let changes = doc.export_changes(&since).unwrap_or_else(|error| panic!("export: {error}"));
    follower.import_changes(&changes).unwrap_or_else(|error| panic!("import: {error}"));
}

#[test]
fn undo_redo_and_a_follower_keep_the_private_fact_holder() {
    let mut doc = SheetDocument::create(NAME, 0, 0).unwrap_or_else(|error| panic!("create: {error}"));
    // The replica first, so every edit below is one of its steps.
    doc.version().unwrap_or_else(|error| panic!("version: {error}"));
    let mut follower = SheetDocument::open_with_replica(NAME, &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
    let first = holder(&objects(&mut doc));
    assert_eq!((first.1["$type"].clone(), first.1["key"].clone()), (json!("AxisFacts"), json!("all")), "{first:?}");
    let check = |doc: &mut SheetDocument, follower: &mut SheetDocument, what: &str| {
        follow(doc, follower);
        let (mine, theirs) = (objects(doc), objects(follower));
        assert_eq!(holder(&mine), first, "{what}: the same holder");
        assert!(mine == theirs, "{what}: the follower holds the same state");
    };
    let edits = [
        ("range.setValues", json!({ "range": "A1", "values": [[1, 2], [3, 4], [5, 6]] })),
        ("row.insert", json!({ "rows": "2:2" })),
        ("cell.set", json!({ "cell": "B5", "input": "7" })),
        ("column.insert", json!({ "cols": "B:B" })),
        ("format.bold", json!({ "range": "A1:C1", "on": true })),
        ("row.delete", json!({ "rows": "1" })),
    ];
    for (command, params) in &edits {
        run(&mut doc, command, params.clone());
        check(&mut doc, &mut follower, command);
    }
    let mut undone = 0;
    while doc.can_undo() {
        assert!(doc.undo().unwrap_or_else(|error| panic!("undo: {error}")).0);
        undone += 1;
        check(&mut doc, &mut follower, &format!("undo {undone}"));
    }
    assert!(undone >= edits.len(), "every edit undone: {undone}");
    let mut redone = 0;
    while doc.can_redo() {
        assert!(doc.redo().unwrap_or_else(|error| panic!("redo: {error}")).0);
        redone += 1;
        check(&mut doc, &mut follower, &format!("redo {redone}"));
    }
    assert_eq!(redone, undone, "every step redone");
}
