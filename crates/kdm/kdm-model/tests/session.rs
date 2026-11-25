//! `SheetSession`: the engine bridge's replica (phase 3f Task 2): `Send`, record/undo/redo with
//! the command's structural edits and the exchange with a peer, cell pictures and images shipped
//! through `blobs_since`, and their blobs garbage-collected once no reachable undo step needs them.

mod common;

use std::sync::Arc;

use common::{assert_same, input, insert_rows, person, put};
use kdm_model::SheetSession;
use kition_kit::kdm::{Origin, Version};
use kition_kit::session::MemoryBlobs;
use kition_sheet_core::CellRef;
use kition_sheet_model::{CellPicture, StructuralEdit, Workbook};

fn base() -> Workbook {
    let mut wb = Workbook::new();
    for r in 1..=4 {
        put(&mut wb, 0, &format!("A{r}"), &format!("{}", r * 10));
    }
    put(&mut wb, 0, "B1", "=A3*2");
    wb
}

fn send(from: &SheetSession, to: &mut SheetSession) {
    let since = to.version();
    to.add_blobs(&MemoryBlobs(from.blobs_since(&since).into_iter().collect()));
    to.import_changes(&from.export_changes(&since).unwrap()).unwrap();
}

fn peer(of: &SheetSession) -> SheetSession {
    SheetSession::open_snapshot(&of.snapshot().unwrap(), MemoryBlobs(of.blobs_since(&Version::empty()).into_iter().collect())).unwrap()
}

fn edited(s: &SheetSession, f: impl FnOnce(&mut Workbook) -> Vec<StructuralEdit>) -> (Workbook, Vec<StructuralEdit>) {
    let mut wb = s.workbook().clone();
    let edits = f(&mut wb);
    (wb, edits)
}

fn picture(bytes: &[u8]) -> Arc<CellPicture> {
    Arc::new(CellPicture { data: bytes.to_vec(), mime: "image/png".into(), alt: "dot".into() })
}

#[test]
fn a_session_is_send() {
    fn send_able<T: Send>() {}
    send_able::<SheetSession>();
}

#[test]
fn record_undo_redo_and_a_peer_agree() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    assert_same("opened", a.workbook(), b.workbook());

    let (after, edits) = edited(&a, |wb| {
        put(wb, 0, "A2", "25");
        vec![]
    });
    assert_eq!(a.record(&person("a"), &after, &edits).unwrap(), 1, "one undo step");
    assert_eq!(a.record(&person("a"), &after, &[]).unwrap(), 0, "nothing changed, no step");
    send(&a, &mut b);
    assert_eq!(input(b.workbook(), 0, "A2"), "25");
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());

    assert!(a.undo().unwrap().0);
    assert_eq!(input(a.workbook(), 0, "A2"), "20");
    assert!(a.redo().unwrap().0);
    assert_eq!(input(a.workbook(), 0, "A2"), "25");
    assert!(!a.redo().unwrap().0, "nothing more to redo");
}

#[test]
fn a_row_insert_moves_the_formula_with_its_row_in_a_peer() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    let (after, edits) = edited(&a, |wb| vec![insert_rows(wb, 0, 1, 2)]);
    a.record(&person("a"), &after, &edits).unwrap();
    send(&a, &mut b);
    assert_eq!(input(b.workbook(), 0, "A5"), "30");
    assert_eq!(input(b.workbook(), 0, "B1"), "=A5*2", "the formula still names the same row");
    let value = b.workbook().sheet(0).map(|s| s.value(CellRef::new(0, 1)).display());
    assert_eq!(value.as_deref(), Some("60"), "the peer's view is recalculated");
    assert_same("after the insert", a.workbook(), b.workbook());
}

#[test]
fn per_person_view_state_writes_nothing_and_survives_an_import() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    let version = a.version();
    let (after, _) = edited(&a, |wb| {
        if let Some(sheet) = wb.sheet_mut(0) {
            sheet.zoom = 150;
            sheet.view_active = CellRef::new(3, 0);
        }
        vec![]
    });
    assert_eq!(a.record(&person("a"), &after, &[]).unwrap(), 0);
    assert_eq!(a.version(), version, "zoom and the active cell are this person's own");
    let (edit, _) = edited(&b, |wb| {
        put(wb, 0, "C1", "remote");
        vec![]
    });
    b.record(&person("b"), &edit, &[]).unwrap();
    send(&b, &mut a);
    let sheet = a.workbook().sheet(0).unwrap();
    assert_eq!((sheet.zoom, sheet.view_active), (150, CellRef::new(3, 0)), "the import keeps this person's view");
    assert_eq!(input(a.workbook(), 0, "C1"), "remote");
}

#[test]
fn a_cell_picture_ships_its_blob_and_lives_while_its_undo_step_does() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    assert!(a.blobs().0.is_empty());
    let since = b.version();
    let (after, _) = edited(&a, |wb| {
        if let Some(sheet) = wb.sheet_mut(0) {
            sheet.cell_pictures.insert(CellRef::new(5, 0), picture(&[0x89, b'P', b'N', b'G', 1, 2, 3]));
        }
        vec![]
    });
    a.record(&person("a"), &after, &[]).unwrap();
    assert_eq!(a.blobs_since(&since).len(), 1, "only the new picture's bytes");
    send(&a, &mut b);
    assert_eq!(b.workbook().sheet(0).map(|s| s.cell_pictures.len()), Some(1), "the peer has the picture");
    assert!(a.blobs_since(&a.version()).is_empty(), "an up-to-date peer needs nothing");

    let (after, _) = edited(&a, |wb| {
        if let Some(sheet) = wb.sheet_mut(0) {
            sheet.cell_pictures.clear();
        }
        vec![]
    });
    a.record(&person("a"), &after, &[]).unwrap();
    assert_eq!(a.blobs().0.len(), 1, "the undo step can bring it back");
    assert!(a.undo().unwrap().0);
    assert_eq!(a.workbook().sheet(0).map(|s| s.cell_pictures.len()), Some(1), "undo brought the picture back");
    assert!(a.redo().unwrap().0);
    a.retain_steps(0, 0);
    assert!(a.blobs().0.is_empty(), "no reachable step needs it any more");
}

/// Opening a snapshot indexes the blobs its sheets' images and cell pictures reference (through
/// the sheets' lists and the cells of the sheets that have pictures, not a walk of every object),
/// so the open's garbage collection keeps every one, and dropping one later frees only it.
#[test]
fn an_opened_snapshot_keeps_its_images_and_cell_pictures() {
    let mut wb = common::rich();
    for (k, byte) in [(0, 8), (1, 9)] {
        if let Some(sheet) = wb.sheet_mut(k) {
            sheet.cell_pictures.insert(CellRef::new(3, 2), picture(&[0x89, b'P', b'N', b'G', byte, byte]));
        }
    }
    let a = SheetSession::create(&wb).unwrap();
    let all = a.blobs().0.len();
    assert_eq!(all, 3, "the Data sheet's image and cell picture, and the Summary sheet's cell picture");
    let mut b = peer(&a);
    assert_eq!(b.blobs().0.len(), all, "the open kept every referenced blob");
    assert_eq!(b.workbook().sheet(1).map(|s| s.cell_pictures.len()), Some(1));

    let (after, _) = edited(&b, |wb| {
        if let Some(sheet) = wb.sheet_mut(1) {
            sheet.cell_pictures.clear();
        }
        vec![]
    });
    b.record(&person("b"), &after, &[]).unwrap();
    b.retain_steps(0, 0);
    assert_eq!(b.blobs().0.len(), all - 1, "only the removed picture's blob went");
}

#[test]
fn holds_compares_content_only() {
    let a = SheetSession::create(&base()).unwrap();
    let mut same = base();
    if let Some(sheet) = same.sheet_mut(0) {
        sheet.zoom = 200;
    }
    assert!(a.holds(&same).unwrap(), "zoom is not content");
    let mut other = base();
    put(&mut other, 0, "A1", "11");
    assert!(!a.holds(&other).unwrap());
}

/// Sends both ways until `a` and `b` hold the same changes (an import may write: a repair).
fn exchange(a: &mut SheetSession, b: &mut SheetSession) {
    for _ in 0..4 {
        send(a, b);
        send(b, a);
        if a.version() == b.version() {
            break;
        }
    }
    assert_eq!(a.version(), b.version(), "the exchange settles");
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    for s in [&*a, &*b] {
        let fresh = kdm_model::read_document(s.replica(), s.blobs()).unwrap().0;
        assert_same("a fresh read", s.workbook(), &fresh);
    }
}

/// A person's step is one undo step even when objects were created ahead of it (`system`): undo
/// and redo round-trip it, and nothing else is undoable.
#[test]
fn objects_created_ahead_are_not_an_undo_step() {
    let mut a = SheetSession::create(&base()).unwrap();
    let (wb, e) = edited(&a, |wb| {
        put(wb, 0, "F9", "far");
        vec![]
    });
    assert_eq!(a.record(&Origin::person("user:a"), &wb, &e).unwrap(), 1);
    assert_eq!(a.steps(), (1, 0));
    assert!(a.undo().unwrap().0);
    assert_eq!(input(a.workbook(), 0, "F9"), "");
    assert!(!a.can_undo(), "the objects made ahead are no undo step");
    assert!(a.redo().unwrap().0);
    assert_eq!(input(a.workbook(), 0, "F9"), "far");
    // Typing into the cell again writes into the same object.
    let (wb, e) = edited(&a, |wb| {
        put(wb, 0, "F9", "farther");
        vec![]
    });
    assert_eq!(a.record(&Origin::person("user:a"), &wb, &e).unwrap(), 1);
    let fresh = kdm_model::read_document(a.replica(), a.blobs()).unwrap().0;
    assert_same("a fresh read", a.workbook(), &fresh);
}

/// An image whose anchor row a delete cuts through stays where it was (the engine refits it), so
/// its stored anchor, which named the deleted row, is written again: the view, a fresh read and a
/// peer agree (an image past the data, the lines ending where the data does: a row insert above
/// it and a delete through it).
#[test]
fn an_image_whose_row_moves_or_goes_reads_where_the_view_has_it() {
    let mut wb = base();
    wb.sheet_mut(0).unwrap().images = vec![kition_sheet_model::Image {
        id: 6,
        anchor: kition_sheet_model::Anchor {
            cell: CellRef::new(14, 13),
            dx: 1.0,
            dy: 1.0,
            width: 30.0,
            height: 20.0,
            mode: Default::default(),
            one_cell: false,
        },
        data: common::PNG.to_vec(),
        mime: "image/png".into(),
        alt: "logo".into(),
        blip_ext: None,
    }];
    let mut a = SheetSession::create(&wb).unwrap();
    let mut b = peer(&a);
    let steps: [fn(&mut Workbook) -> Vec<StructuralEdit>; 3] =
        [|wb| vec![insert_rows(wb, 0, 2, 3)], |wb| vec![common::delete_rows(wb, 0, 15, 3)], |wb| vec![common::delete_rows(wb, 0, 1, 2)]];
    for step in steps {
        let (wb, e) = edited(&a, step);
        a.record(&Origin::person("user:a"), &wb, &e).unwrap();
        exchange(&mut a, &mut b);
        assert_same("the peer", a.workbook(), b.workbook());
    }
    assert_eq!(a.workbook().sheets[0].images[0].anchor.cell, CellRef::new(15, 13), "moved down 3, kept by the delete through it, up 2");
    while a.can_undo() {
        assert!(a.undo().unwrap().0);
        exchange(&mut a, &mut b);
    }
    assert_eq!(b.workbook().sheets[0].images[0].anchor.cell, CellRef::new(14, 13));
}

/// A draft (an agent's replica of the workbook) that typed into a new cell the person typed into
/// meanwhile names another `Cell` object for the cell's entry (each session creates its own
/// object ahead): the merge is refused (data model 6.6, rule D4) and changes nothing, so one cell's
/// entry never has two objects to choose between.
#[test]
fn a_draft_typing_into_a_new_cell_the_person_typed_into_is_refused() {
    use kition_kit::kdm::MergeAt;
    let mut a = SheetSession::create(&base()).unwrap();
    let at = a.version();
    let mut d = peer(&a);
    let (wb, e) = edited(&d, |wb| {
        put(wb, 0, "B3", "the draft's");
        vec![]
    });
    d.record(&Origin::agent("user:agent"), &wb, &e).unwrap();
    let (wb, e) = edited(&a, |wb| {
        put(wb, 0, "B3", "mine");
        vec![]
    });
    a.record(&Origin::person("user:a"), &wb, &e).unwrap();
    let changes = d.export_changes(&at).unwrap();
    let ids = &a.model().ids().sheets[0];
    let key = format!("{}:{}", ids.rows[2].as_str(), ids.cols[1].as_str());
    let conflicts = a.replica().merge_conflicts(&changes).unwrap();
    assert!(
        conflicts
            .iter()
            .any(|c| c.rule == "D4" && c.object == ids.object && c.field.as_deref() == Some("cells") && c.at == Some(MergeAt::Key(key.clone()))),
        "the cell's entry collides: {conflicts:?}"
    );
    let version = a.version();
    assert!(a.merge_changes(&changes, &Origin::person("user:a")).is_err(), "the merge is refused");
    assert_eq!(a.version(), version, "and changes nothing");
    assert_eq!(input(a.workbook(), 0, "B3"), "mine");
}
