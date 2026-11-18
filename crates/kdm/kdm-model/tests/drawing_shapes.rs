//! Drawing shapes through the replica, as `Shape` objects (schema version 3): groups, preset and
//! custom geometry, rotation and mirroring, transparency, gradients, pens, kept text bodies and
//! style references, and one-cell anchors reach a peer; concurrent changes to different shapes,
//! and to one shape's text, frame and each of its other members, all survive (`KitDrawing 1.0`
//! DR14); a
//! replica that kept its shapes in the sheet's `features` (schema version 2) reads and migrates.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, peer, sync};
use kdm_model::SheetSession;
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema};
use kition_kit::session::MemoryBlobs;
use kition_sheet_model::{
    Anchor, AnchorMode, Color, CustomGeom, CustomPath, Gradient, GradientStop, Outline, PathFill, PresetGeom, Shape, ShapeKind, Workbook,
};
use serde_json::json;

fn shapes() -> Vec<Shape> {
    let anchor = |a1: &str| Anchor { cell: at(a1), dx: 2.0, dy: 3.0, width: 80.0, height: 40.0, mode: AnchorMode::MoveOnly, one_cell: true };
    vec![
        Shape {
            id: 10,
            kind: ShapeKind::TextBox,
            anchor: anchor("C3"),
            fill: Color::Theme(4, 400),
            line: Color::Rgb(0xB4C6E7),
            text: "Title".into(),
            groups: vec![1],
            fill_alpha: Some(35_000),
            outline: Outline { width: Some(9525), dash: Some("solid".into()), cap: Some("flat".into()), alpha: Some(80_000), ..Default::default() },
            text_body: Some(
                "<xdr:txBody><a:bodyPr anchor=\"ctr\"/><a:lstStyle/><a:p><a:r><a:rPr sz=\"1400\" b=\"1\"/><a:t>Title</a:t></a:r></a:p></xdr:txBody>"
                    .into(),
            ),
            ..Default::default()
        },
        Shape {
            id: 11,
            kind: ShapeKind::Rectangle,
            anchor: anchor("E3"),
            line: Color::Rgb(0),
            groups: vec![1, 2],
            rot: 5_400_000,
            flip_v: true,
            geom: Some(CustomGeom {
                paths: vec![CustomPath { w: 200.0, h: 100.0, d: "M 0 100 L 100 0 L 200 100 Z".into(), fill: PathFill::None, stroke: true }],
                source: Some("<a:custGeom><a:avLst/><a:gdLst><a:gd name=\"mid\" fmla=\"*/ w 1 2\"/></a:gdLst></a:custGeom>".into()),
            }),
            ..Default::default()
        },
        Shape {
            id: 12,
            kind: ShapeKind::Rectangle,
            anchor: anchor("E8"),
            fill: Color::Rgb(0x4472C4),
            preset: Some(PresetGeom { name: "star5".into(), adj: vec![("adj".into(), "val 19098".into())] }),
            gradient: Some(Gradient {
                stops: vec![
                    GradientStop { pos: 0, color: Color::Rgb(0x4472C4), alpha: None },
                    GradientStop { pos: 100_000, color: Color::Theme(1, 0), alpha: Some(0) },
                ],
                angle: 5_400_000,
                scaled: Some(true),
                ..Default::default()
            }),
            style_refs: Some("<xdr:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"/></a:lnRef></xdr:style>".into()),
            ..Default::default()
        },
    ]
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().workbook, &b.model().workbook);
    assert_workbook_valid(&a.model().workbook);
    assert_replica_clean(a.replica());
}

#[test]
fn a_shapes_groups_geometry_transparency_and_text_body_reach_the_peer_and_concurrent_changes_converge() {
    let blobs = SharedBlobs::default();
    let mut a = first(&Workbook::new(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes = shapes();
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(b.model().workbook.sheet(0).map(|s| s.shapes.clone()), Some(shapes()));

    // Both change the new fields at once: the replicas agree on one outcome.
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].fill_alpha = Some(10_000);
            s.shapes[1].groups = vec![1];
        }
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[2].gradient = None;
            s.shapes[0].outline.width = Some(28_575);
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let after = b.model().workbook.sheet(0).map(|s| s.shapes.clone()).unwrap_or_default();
    assert_eq!(after.len(), 3);
    // Changes to different shapes both stay, and so do changes to two members of one shape
    // (each its own entry of the shape's `look`).
    assert_eq!(after[1].groups, vec![1], "A's change of shape 11 stays");
    assert!(after[2].gradient.is_none(), "B's change of shape 12 stays");
    assert_eq!((after[0].fill_alpha, after[0].outline.width), (Some(10_000), Some(28_575)), "{:?}", after[0]);

    // Undo on each side converges too.
    a.undo().unwrap_or_else(|e| panic!("undo: {e}"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
}

#[test]
fn concurrent_text_frame_and_formatting_of_one_shape_all_stay() {
    let blobs = SharedBlobs::default();
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.shapes = shapes();
    }
    let mut a = first(&wb, &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].text = "Title, typed by A".into();
            s.shapes[1].anchor.dx = 40.0;
        }
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].fill = Color::Rgb(0x00FF00);
            s.shapes[0].text = "My Title".into();
            s.shapes[1].line = Color::Rgb(0xFF0000);
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let after = a.model().workbook.sheet(0).map(|s| s.shapes.clone()).unwrap_or_default();
    // Text against formatting, and a move against an outline: both sides of each.
    assert_eq!(after[0].fill, Color::Rgb(0x00FF00));
    assert_eq!(after[1].anchor.dx, 40.0);
    assert_eq!(after[1].line, Color::Rgb(0xFF0000));
    // Both people's typing in one shape's text stays, character by character.
    assert!(after[0].text.contains("typed by A") && after[0].text.starts_with("My"), "{}", after[0].text);
}

#[test]
fn an_art_text_warp_a_connector_end_and_a_move_all_stay() {
    let blobs = SharedBlobs::default();
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.shapes = shapes();
    }
    let mut a = first(&wb, &blobs);
    let mut b = peer(&a, 2, &blobs);
    let warped = "<xdr:txBody><a:bodyPr anchor=\"ctr\"><a:prstTxWarp prst=\"textArchUp\"><a:avLst/></a:prstTxWarp></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr sz=\"1400\" b=\"1\"/><a:t>Title</a:t></a:r></a:p></xdr:txBody>";
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].text_body = Some(warped.into());
            s.shapes[1].connection =
                Some(kition_sheet_model::ShapeConnection { start: Some(kition_sheet_model::ConnectionEnd { id: 12, site: 1 }), end: None });
        }
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].anchor.dx = 55.0;
            s.shapes[0].fill = Color::Rgb(0x112233);
            s.shapes[1].line = Color::Rgb(0x00AA00);
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let after = b.model().workbook.sheet(0).map(|s| s.shapes.clone()).unwrap_or_default();
    // A's warp (in the text body) against B's move and fill of that shape; A's glued end
    // against B's pen on the connector: each member is its own entry, so all stay.
    assert_eq!((after[0].text_body.as_deref(), after[0].anchor.dx, after[0].fill), (Some(warped), 55.0, Color::Rgb(0x112233)));
    assert_eq!((after[1].connection.as_ref().and_then(|c| c.start).map(|e| e.id), after[1].line), (Some(12), Color::Rgb(0x00AA00)));
}

#[test]
fn deleting_a_shape_against_an_edit_deletes_it() {
    let blobs = SharedBlobs::default();
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.shapes = shapes();
    }
    let mut a = first(&wb, &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes.remove(1);
        }
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[1].fill = Color::Rgb(0x123456);
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let ids: Vec<u32> = a.model().workbook.sheet(0).map(|s| s.shapes.iter().map(|x| x.id).collect()).unwrap_or_default();
    assert_eq!(ids, vec![10, 12]);
}

/// The schema replicas were written under before shapes were objects: version 2, without `Shape`.
fn schema_v2() -> std::sync::Arc<Schema> {
    let mut raw: serde_json::Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap_or_else(|e| panic!("schema: {e}"));
    raw["schemaVersion"] = json!(2);
    raw["types"].as_object_mut().unwrap_or_else(|| panic!("types")).remove("Shape");
    raw["types"].as_object_mut().unwrap_or_else(|| panic!("types")).remove("Chart");
    raw["types"]["Sheet"]["fields"].as_object_mut().unwrap_or_else(|| panic!("Sheet fields")).remove("shapes");
    std::sync::Arc::new(Schema::parse(&raw.to_string()).unwrap_or_else(|e| panic!("v2 schema: {e}")))
}

/// A replica as Kition Sheet 0.5 wrote it: one sheet whose `features` hold two shapes.
fn replica_v2() -> Vec<u8> {
    let actor = ActorId::from_u64(7).unwrap_or_else(|_| panic!("actor"));
    let (replica, ()) = Replica::create_with(schema_v2(), DocId::new(), actor, &Origin::system("create"), |tx| {
        let root = ObjectId::from_string("workbook");
        tx.create_with_id("Workbook", &root, serde_json::Map::new())?;
        let sheet = tx.create("Sheet", [("name".to_owned(), json!("Data"))].into_iter().collect())?;
        tx.list_insert(&root, "sheets", 0, &sheet)?;
        let mut lines = Vec::new();
        for (field, k) in [("rows", 0), ("rows", 1), ("cols", 0), ("cols", 1)] {
            let line = tx.create(if field == "rows" { "Row" } else { "Col" }, serde_json::Map::new())?;
            tx.list_insert(&sheet, field, k, &line)?;
            lines.push(line);
        }
        let cell = format!("{},{}", lines[1].as_str(), lines[3].as_str());
        let anchor = json!({ "cell": cell, "dx": 2.0, "dy": 3.0, "width": 80.0, "height": 40.0, "mode": "MoveOnly" });
        let features = json!({ "shapes": [
            { "id": 10, "kind": "TextBox", "anchor": anchor, "fill": { "Rgb": 0xFFFFFF }, "line": "Auto", "text": "Kept", "rot": 5_400_000 },
            { "id": 11, "kind": "Ellipse", "anchor": anchor, "fill": { "Theme": [4, 0] }, "line": "Auto", "text": "", "flip_h": true },
        ] });
        tx.set(&sheet, "features", features)?;
        Ok(())
    })
    .unwrap_or_else(|e| panic!("v2 replica: {e}"));
    replica.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"))
}

#[test]
fn a_version_2_replica_reads_its_shapes_and_moves_them_into_objects_on_its_first_write() {
    let bytes = replica_v2();
    let mut s = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open: {e}"));
    assert!(!s.migrated());
    let check = |wb: &Workbook| {
        let shapes = &wb.sheets[0].shapes;
        assert_eq!(shapes.iter().map(|x| x.id).collect::<Vec<_>>(), vec![10, 11]);
        assert_eq!((shapes[0].text.as_str(), shapes[0].rot, shapes[0].anchor.cell), ("Kept", 5_400_000, at("B2")));
        assert!(shapes[1].flip_h && shapes[1].kind == ShapeKind::Ellipse);
    };
    check(s.workbook());

    // The first write migrates: the shapes become objects, the view reads the same.
    let mut after = s.workbook().clone();
    if let Some(sheet) = after.sheet_mut(0) {
        sheet.shapes[1].text = "New".into();
    }
    assert_eq!(s.record(&Origin::person("user:a"), &after, &[]).unwrap_or_else(|e| panic!("record: {e}")), 1, "one undo step: the edit");
    assert!(s.migrated());
    assert_eq!(s.replica().min_schema_version(), 4, "shapes and charts are objects now");
    let sheet = s.model().ids().sheets[0].object.clone();
    assert_eq!(s.replica().objects_of_type("Shape").len(), 2);
    assert!(s.replica().get(&sheet, "features").unwrap_or_default().get("shapes").is_none(), "no shapes left in `features`");
    let (read, _) = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("read: {e}"));
    assert_eq!(read.sheets[0].shapes[1].text, "New");
    assert_eq!(read.sheets[0].shapes[0].text, "Kept");

    // A version 2 kit refuses the migrated replica rather than showing it without its shapes.
    let actor = ActorId::from_u64(11).unwrap_or_else(|_| panic!("actor"));
    let migrated = s.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    assert!(matches!(Replica::load(schema_v2(), &migrated, actor), Err(KdmError::NewerSchema { found: 4, reads: 2 })));

    // Undo takes back the edit, not the migration.
    assert!(s.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    check(s.workbook());
    assert_eq!(s.replica().objects_of_type("Shape").len(), 2, "the shapes stay objects after the undo");
}

#[test]
fn two_people_migrating_one_replica_at_once_converge_on_the_same_shapes() {
    let bytes = replica_v2();
    let mut a = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open a: {e}"));
    let mut b = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open b: {e}"));
    for (s, text) in [(&mut a, "from a"), (&mut b, "from b")] {
        let mut after = s.workbook().clone();
        if let Some(sheet) = after.sheet_mut(0) {
            sheet.shapes[1].text = text.into();
        }
        s.record(&Origin::person("user:x"), &after, &[]).unwrap_or_else(|e| panic!("record: {e}"));
    }
    let to_b = a.export_changes(&b.version()).unwrap_or_else(|e| panic!("export: {e}"));
    let to_a = b.export_changes(&a.version()).unwrap_or_else(|e| panic!("export: {e}"));
    b.import_changes(&to_b).unwrap_or_else(|e| panic!("import b: {e}"));
    a.import_changes(&to_a).unwrap_or_else(|e| panic!("import a: {e}"));
    assert_eq!(a.replica().objects_of_type("Shape").len(), 2, "the same two objects, not four");
    assert_eq!(a.workbook().sheets[0].shapes, b.workbook().sheets[0].shapes);
    // Both made the same object at once, so one creation's text is the object's: a known limit for
    // text typed into one shape by two people who each migrated the same old replica offline.
    let text = &a.workbook().sheets[0].shapes[1].text;
    assert!(text == "from a" || text == "from b", "one side's text, never a mix: {text}");
}
