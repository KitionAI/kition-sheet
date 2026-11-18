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

/// Two people write into the same new column at once (D1 and D2: both replicas pad the columns
/// with the same derived column object). One undoing their edit must not delete the column, and
/// with it the other's cell, for everyone.
#[test]
fn undoing_a_cell_in_a_column_both_made_keeps_the_other_cell() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    let (wb, e) = edited(&a, |wb| {
        put(wb, 0, "D1", "from a");
        vec![]
    });
    a.record(&Origin::person("user:a"), &wb, &e).unwrap();
    let (wb, e) = edited(&b, |wb| {
        put(wb, 0, "D2", "from b");
        vec![]
    });
    b.record(&Origin::person("user:b"), &wb, &e).unwrap();
    send(&a, &mut b);
    send(&b, &mut a);
    for s in [&a, &b] {
        assert_eq!(input(s.workbook(), 0, "D1"), "from a");
        assert_eq!(input(s.workbook(), 0, "D2"), "from b");
    }

    assert!(b.undo().unwrap().0, "b's edit is an undo step");
    send(&b, &mut a);
    send(&a, &mut b);
    for (who, s) in [("a", &a), ("b", &b)] {
        assert_eq!(input(s.workbook(), 0, "D1"), "from a", "{who}: a's cell survives b's undo");
        assert_eq!(input(s.workbook(), 0, "D2"), "", "{who}: b's own cell is undone");
    }
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    assert!(a.can_undo(), "a's step is still there");
    assert!(a.undo().unwrap().0);
    assert_eq!(input(a.workbook(), 0, "D1"), "");
    assert!(a.redo().unwrap().0);
    assert_eq!(input(a.workbook(), 0, "D1"), "from a");
}

/// Sends both ways until `a` and `b` hold the same changes (an import may write: a lost cell's
/// text typed again).
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

fn type_into(s: &mut SheetSession, who: &str, a1: &str, text: &str) {
    let (wb, e) = edited(s, |wb| {
        put(wb, 0, a1, text);
        vec![]
    });
    assert_eq!(s.record(&Origin::person(format!("user:{who}")), &wb, &e).unwrap(), 1);
}

/// Two people type into the same empty cell at once: each creates a `Cell` object of their own for
/// it, and the merge gives the cell's entry to one; the session that sees both records the other
/// (`merged`), so both texts show; each person's undo then reverts only their own typing (not the
/// step before it), for everyone, and redo brings it back.
#[test]
fn typing_into_one_empty_cell_on_both_keeps_both_texts_and_undo_reverts_only_the_typing() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    // A step of each before the race, which their undo of the race's typing must not revert.
    type_into(&mut a, "a", "C1", "a before");
    type_into(&mut b, "b", "C2", "b before");
    exchange(&mut a, &mut b);

    type_into(&mut a, "a", "B3", "abc");
    type_into(&mut b, "b", "B3", "xyz");
    exchange(&mut a, &mut b);
    let merged = input(a.workbook(), 0, "B3");
    assert!(merged == "abcxyz" || merged == "xyzabc", "both texts survive: {merged:?}");
    assert_eq!(input(b.workbook(), 0, "B3"), merged);

    assert!(b.undo().unwrap().0);
    assert_eq!(input(b.workbook(), 0, "B3"), "abc", "b's undo reverts b's typing");
    assert_eq!(input(b.workbook(), 0, "C2"), "b before", "and not b's step before it");
    exchange(&mut a, &mut b);
    assert_eq!(input(a.workbook(), 0, "B3"), "abc");
    assert!(b.redo().unwrap().0);
    exchange(&mut a, &mut b);
    let again = input(a.workbook(), 0, "B3");
    assert!(again == "abcxyz" || again == "xyzabc", "b's redo types it again: {again:?}");

    assert!(a.undo().unwrap().0);
    assert_eq!(input(a.workbook(), 0, "B3"), "xyz", "a's undo reverts a's typing");
    assert_eq!(input(a.workbook(), 0, "C1"), "a before", "and not a's step before it");
    exchange(&mut a, &mut b);
    for s in [&a, &b] {
        assert_eq!(input(s.workbook(), 0, "B3"), "xyz");
        assert_eq!(input(s.workbook(), 0, "C1"), "a before");
        assert_eq!(input(s.workbook(), 0, "C2"), "b before");
    }

    // Both undo everything: the workbook is the base again, the emptied cells no cells.
    while a.can_undo() {
        assert!(a.undo().unwrap().0);
    }
    while b.can_undo() {
        assert!(b.undo().unwrap().0);
    }
    exchange(&mut a, &mut b);
    for s in [&a, &b] {
        assert!(s.workbook().sheets[0].cells.get(CellRef::new(2, 1)).is_none(), "an emptied cell is no cell");
        assert!(s.holds(&base()).unwrap(), "the workbook is the base again");
    }
}

/// Three people typing into one empty cell at once keep all three texts.
#[test]
fn typing_into_one_empty_cell_on_three_keeps_all_texts() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    let mut c = peer(&a);
    type_into(&mut a, "a", "E5", "one");
    type_into(&mut b, "b", "E5", "two");
    type_into(&mut c, "c", "E5", "three");
    for _ in 0..3 {
        exchange(&mut a, &mut b);
        exchange(&mut b, &mut c);
        exchange(&mut a, &mut c);
    }
    let merged = input(a.workbook(), 0, "E5");
    for text in ["one", "two", "three"] {
        assert!(merged.contains(text), "{text} survives: {merged:?}");
    }
    assert_eq!(merged.len(), "onetwothree".len(), "each once: {merged:?}");
    assert_eq!(input(c.workbook(), 0, "E5"), merged);
}

/// A session that typed into an empty cell and closed before it synced still keeps its text when
/// another session typed into the same cell at once: the race is recorded by whichever session
/// sees both objects (here the other person's, importing the closed session's stored changes),
/// not retyped by the closed one.
#[test]
fn a_session_closed_before_syncing_keeps_its_text_typed_into_a_cell_another_typed_into() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    let since = a.version();
    type_into(&mut a, "a", "B7", "typed by a");
    // A's changes are stored, and A's session closes.
    let stored = a.export_changes(&since).unwrap();
    drop(a);
    // B made more changes first, so its claim on the cell is the later one and keeps the entry:
    // the closed session's object is the one that loses it.
    for r in 1..=12 {
        type_into(&mut b, "b", &format!("H{r}"), "more");
    }
    type_into(&mut b, "b", "B7", "typed by b");
    b.import_changes(&stored).unwrap();
    let merged = input(b.workbook(), 0, "B7");
    assert!(merged == "typed by atyped by b" || merged == "typed by btyped by a", "both texts: {merged:?}");
    let fresh = kdm_model::read_document(b.replica(), b.blobs()).unwrap().0;
    assert_same("a fresh read", b.workbook(), &fresh);
    // A reopened session (any replica) reads the same.
    let c = peer(&b);
    assert_eq!(input(c.workbook(), 0, "B7"), merged);
}

/// A replica merged with no session running (a store importing both people's changes) shows both
/// texts once a session opens it: opening records the race.
#[test]
fn a_replica_merged_without_a_session_shows_both_texts_when_opened() {
    let a0 = SheetSession::create(&base()).unwrap();
    let mut a = peer(&a0);
    let mut b = peer(&a0);
    let since = a.version();
    type_into(&mut a, "a", "C9", "one");
    type_into(&mut b, "b", "C9", "two");
    let snapshot = a0.snapshot().unwrap();
    let mut store = kition_kit::kdm::Replica::load(
        <kdm_model::SheetModel as kition_kit::session::KitModel>::schema(),
        &snapshot,
        kition_kit::kdm::ActorId::random(),
    )
    .unwrap();
    store.import_changes(&a.export_changes(&since).unwrap()).unwrap();
    store.import_changes(&b.export_changes(&since).unwrap()).unwrap();
    let c = SheetSession::open_snapshot(&store.snapshot().unwrap(), MemoryBlobs(a0.blobs_since(&Version::empty()).into_iter().collect())).unwrap();
    let merged = input(c.workbook(), 0, "C9");
    assert!(merged == "onetwo" || merged == "twoone", "both texts: {merged:?}");
    let fresh = kdm_model::read_document(c.replica(), c.blobs()).unwrap().0;
    assert_same("the record is in the replica", c.workbook(), &fresh);
    assert_eq!(c.steps(), (0, 0), "recording the race is no undo step");
}

/// A third person typing into the object that lost the race (it was the cell's object when they
/// saw it) keeps their text too, and later edits of the cell write each object's part.
#[test]
fn a_third_typist_into_the_losing_object_keeps_their_text() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    let mut c = peer(&a);
    type_into(&mut a, "a", "D8", "aaa");
    // B's claim on the cell is the later one: A's object (which C types into) loses the entry.
    for r in 1..=12 {
        type_into(&mut b, "b", &format!("H{r}"), "more");
    }
    type_into(&mut b, "b", "D8", "bbb");
    // C sees only A's cell, and types after A's text.
    send(&a, &mut c);
    type_into(&mut c, "c", "D8", "aaa+ccc");
    for _ in 0..3 {
        exchange(&mut a, &mut b);
        exchange(&mut b, &mut c);
        exchange(&mut a, &mut c);
    }
    let merged = input(a.workbook(), 0, "D8");
    for text in ["aaa+ccc", "bbb"] {
        assert!(merged.contains(text), "{text} survives: {merged:?}");
    }
    assert_eq!(merged.len(), "aaa+cccbbb".len(), "each once: {merged:?}");
    for s in [&b, &c] {
        assert_eq!(input(s.workbook(), 0, "D8"), merged);
    }
    // Editing the whole cell afterwards writes each object's part: everyone reads the edit.
    type_into(&mut b, "b", "D8", "replaced");
    for _ in 0..2 {
        exchange(&mut a, &mut b);
        exchange(&mut b, &mut c);
    }
    for s in [&a, &b, &c] {
        assert_eq!(input(s.workbook(), 0, "D8"), "replaced");
    }
}

/// A person who lost the race with a style on their new cell keeps it when the winner's has none.
#[test]
fn a_lost_new_cells_style_is_kept() {
    let mut a = SheetSession::create(&base()).unwrap();
    let mut b = peer(&a);
    type_into(&mut a, "a", "D4", "plain");
    let (wb, e) = edited(&b, |wb| {
        put(wb, 0, "D4", "bold");
        let mut bold = kition_sheet_model::Style::default();
        bold.font.bold = true;
        let style = wb.styles.intern(bold);
        let sheet = wb.sheet_mut(0).unwrap();
        let mut cell = sheet.cells.get(CellRef::new(3, 3)).cloned().unwrap();
        cell.style = style;
        sheet.set_cell(CellRef::new(3, 3), cell);
        vec![]
    });
    b.record(&Origin::person("user:b"), &wb, &e).unwrap();
    exchange(&mut a, &mut b);
    let merged = input(a.workbook(), 0, "D4");
    assert!(merged == "plainbold" || merged == "boldplain", "both texts survive: {merged:?}");
    let style = a.workbook().sheets[0].cells.get(CellRef::new(3, 3)).unwrap().style;
    assert!(a.workbook().styles.get(style).font.bold, "the styled cell's style is kept");
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
