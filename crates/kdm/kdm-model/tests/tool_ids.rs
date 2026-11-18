//! Tool ids (KitObjectModel OM9, task card C4/C5/C6-sheet): the object model names a sheet
//! `sheet:<sheetId>` and a drawing object (a shape, a chart, a picture) `drawing:<id>`, the ids
//! the kit's inspect lists, so receipts, queries, batches and commands speak one id space. The ids
//! are written by the kit's own write path and survive a read, a rewrite, and a new replica made
//! from the saved file.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::{SharedBlobs, actor, new_replica, rich};
use kdm_model::{Ids, SheetModel, encode, read_document, write_changes};
use kition_kit::kdm::Replica;
use kition_kit::kdm::tool_ids::ToolIds;
use kition_kit::session::KitSession;
use kition_sheet_model::Workbook;

/// `rich()` with its sheets numbered 3, 7, 8, … (not their places).
fn numbered() -> Workbook {
    let mut wb = rich();
    for (k, sheet) in wb.sheets.iter_mut().enumerate() {
        Arc::make_mut(sheet).sheet_id = if k == 0 { 3 } else { 6 + k as u32 };
    }
    wb
}

/// Every tool id the object model renders for the sheets and the drawing objects of `ids`.
fn named(replica: &Replica, ids: &Ids) -> BTreeSet<String> {
    let tools = ToolIds::of_replica(replica);
    let mut out = BTreeSet::new();
    for sheet in &ids.sheets {
        for object in std::iter::once(&sheet.object).chain(&sheet.shapes).chain(&sheet.charts).chain(&sheet.images) {
            out.insert(tools.tool_id(object).unwrap_or_else(|| panic!("{object} has no tool id")).to_owned());
        }
    }
    out
}

/// What `wb` should be named by: its sheets' ids and its drawing objects' ids.
fn expected(wb: &Workbook) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for sheet in &wb.sheets {
        out.insert(format!("sheet:{}", sheet.sheet_id));
        for id in sheet.shapes.iter().map(|s| s.id).chain(sheet.charts.iter().map(|c| c.id)).chain(sheet.images.iter().map(|i| i.id)) {
            out.insert(format!("drawing:{id}"));
        }
    }
    out
}

#[test]
fn sheets_and_drawings_are_named_by_the_ids_the_kit_lists() {
    let wb = numbered();
    assert!(wb.sheets.iter().any(|s| !s.shapes.is_empty() && !s.charts.is_empty() && !s.images.is_empty()), "the fixture has drawings");
    let blobs = SharedBlobs::default();
    let (replica, ids) = new_replica(&wb, &blobs);
    assert_eq!(named(&replica, &ids), expected(&wb));
    assert!(named(&replica, &ids).contains("sheet:3"));
}

#[test]
fn the_ids_survive_a_read_a_rewrite_and_a_new_replica() {
    let wb = numbered();
    let blobs = SharedBlobs::default();
    let (mut replica, _) = new_replica(&wb, &blobs);
    let (read, mut ids) = read_document(&replica, &blobs).unwrap();
    assert_eq!(read.sheets.iter().map(|s| s.sheet_id).collect::<Vec<_>>(), wb.sheets.iter().map(|s| s.sheet_id).collect::<Vec<_>>());
    // A rename and a move write the sheets again; their ids stay.
    let mut after = read.clone();
    Arc::make_mut(&mut after.sheets[0]).name = "Renamed".into();
    let first = after.sheets.remove(0);
    after.sheets.push(first);
    let mut sink = blobs.clone();
    replica.transact(&common::person("t"), |tx| write_changes(tx, &read, &after, &[], &mut ids, &mut sink)).unwrap();
    assert_eq!(named(&replica, &ids), expected(&wb));
    // A new replica from the saved file names them the same.
    let bytes = encode("book.xlsx", &after).unwrap();
    let session: KitSession<SheetModel> = KitSession::import("book.xlsx", &bytes, Box::new(blobs.clone()), actor(9)).unwrap();
    let (again, ids) = read_document(session.replica(), &blobs).unwrap();
    assert_eq!(named(session.replica(), &ids), expected(&wb));
    assert_eq!(again.sheets.last().map(|s| (s.name.as_str(), s.sheet_id)), Some(("Renamed", 3)));
}
