//! Round trips: a workbook written to a replica reads back with the same content (style ids,
//! formula results and per-person state aside), for a workbook exercising every replicated kind
//! of content (directly and through `.xlsx`), the web editor's sample, and (when
//! `KITION_SHEET_CORPUS=<dir>` is set) up to 40 `.xlsx`/`.xlsm` files of a local corpus. Each
//! also checks that writing the read view over itself writes nothing.

mod common;

use std::sync::Arc;

use common::{SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, new_replica, rich};
use kdm_model::{Known, SheetModel, decode, encode, read_document, read_document_known, write_changes};
use kition_kit::session::{KitModel, KitSession};
use kition_sheet_model::{Sheet, Workbook};

/// A copy of `wb` sharing no sheet `Arc`, so every sheet is compared.
fn deep(wb: &Workbook) -> Workbook {
    let mut copy = wb.clone();
    copy.sheets = wb.sheets.iter().map(|s| Arc::new(Sheet::clone(s))).collect();
    copy
}

/// `wb` through `import`'s path (write_document in a creating transaction) and back. Also checks
/// that diffing the read view against a deep copy of itself writes nothing.
fn round_trip(wb: &Workbook) -> Workbook {
    let blobs = SharedBlobs::default();
    let (mut replica, _) = new_replica(wb, &blobs);
    replica.take_changed();
    assert_replica_clean(&replica);
    let (read, mut ids) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_workbook_valid(&read);
    let copy = deep(&read);
    let mut sink = blobs.clone();
    let wrote = replica
        .transact(&common::person("t"), |tx| write_changes(tx, &read, &copy, &[], &mut ids, &mut sink))
        .unwrap_or_else(|error| panic!("write_changes: {error}"));
    let changed: Vec<String> = replica.take_changed().iter().map(|o| format!("{} {}", replica.object_type(o).unwrap_or_default(), o)).collect();
    assert!(!wrote, "an unchanged view wrote changes: {changed:?}");
    read
}

#[test]
fn rich_workbook_round_trips() {
    let wb = rich();
    let read = round_trip(&wb);
    assert_same("rich", &wb, &read);
    // Spot checks of what the comparison covers.
    let data = read.sheet(0).unwrap_or_else(|| panic!("Data"));
    assert_eq!(data.input_text(common::at("C8")), "=Summary!B2*2");
    assert_eq!(data.input_text(common::at("C11")), "=SUM(A1:A1048576)");
    assert_eq!(data.input_text(common::at("A400001")), "far below", "content below the materialised rows");
    assert_eq!(data.rows.get(&500_000).and_then(|i| i.size), Some(9.5), "row info below the materialised rows");
    assert_eq!(data.protection.as_ref().and_then(|p| p.password_hash), Some(kition_sheet_model::password_hash("pw")), "a hash beyond 2^53");
    assert_eq!(read.names.iter().find(|n| n.name == "Local").and_then(|n| n.scope), Some(1));
    assert!(data.cell_pictures.contains_key(&common::at("J2")));
}

#[test]
fn per_person_state_is_not_written_but_kept_on_read() {
    let wb = rich();
    let blobs = SharedBlobs::default();
    let (replica, ids) = new_replica(&wb, &blobs);
    let (read, _) = read_document(&replica, &blobs).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(read.sheets[0].zoom, 100, "zoom is not in the replica");
    assert_eq!(read.sheets[0].view_active, Default::default());
    let mut known = Known::default();
    let mut mine = wb.clone();
    mine.active_sheet = 1;
    known.remember(&mine, &ids);
    let (read, _) = read_document_known(&replica, &blobs, &known).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!((read.active_sheet, read.sheets[0].zoom, read.sheets[0].view_active), (1, 130, common::at("B3")));
}

#[test]
fn rich_workbook_round_trips_through_xlsx() {
    let bytes = encode("Rich.xlsx", &rich()).unwrap_or_else(|error| panic!("encode: {error}"));
    let original = decode("Rich.xlsx", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    assert!(original.sheets.len() == 2 && !original.names.is_empty());
    assert_same("rich.xlsx", &original, &round_trip(&original));
}

#[test]
fn sample_round_trips() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../web/public/sample/Sample.xlsx"))
        .unwrap_or_else(|error| panic!("sample: {error}"));
    let original = decode("Sample.xlsx", &bytes).unwrap_or_else(|error| panic!("decode: {error}"));
    assert!(original.sheets.iter().any(|s| !s.cells.is_empty()));
    assert_same("Sample.xlsx", &original, &round_trip(&original));
}

#[test]
fn import_and_export_through_a_session() {
    let bytes = encode("Rich.xlsx", &rich()).unwrap_or_else(|error| panic!("encode: {error}"));
    let session: KitSession<SheetModel> =
        KitSession::import("Rich.xlsx", &bytes, Box::new(SharedBlobs::default()), common::actor(7)).unwrap_or_else(|error| panic!("import: {error}"));
    // Formula results are recalculated on load: Data!C8 is Summary!B2*2, Summary!B2 is 0+12.
    let data = session.model().workbook.sheet(0).unwrap_or_else(|| panic!("Data"));
    assert_eq!(data.value(common::at("C8")), kition_sheet_core::Value::Number(24.0));
    let exported = session.export("Rich.xlsx").unwrap_or_else(|error| panic!("export: {error}"));
    let again = decode("Rich.xlsx", &exported).unwrap_or_else(|error| panic!("decode: {error}"));
    assert_same("export", &decode("Rich.xlsx", &bytes).unwrap_or_else(|error| panic!("decode: {error}")), &again);
    let csv = session.export("Rich.csv").unwrap_or_else(|error| panic!("csv export: {error}"));
    assert!(String::from_utf8_lossy(&csv).contains("Apples"));
}

#[test]
fn other_formats_import() {
    let wb = decode("Notes.csv", b"a,b\n1,2\n").unwrap_or_else(|error| panic!("csv: {error}"));
    assert_eq!(wb.sheets[0].name, "Notes");
    assert_eq!(wb.sheets[0].input_text(common::at("B2")), "2");
    let json = encode("book.json", &rich()).unwrap_or_else(|error| panic!("json: {error}"));
    let back = decode("book.json", &json).unwrap_or_else(|error| panic!("json: {error}"));
    assert_same("json", &rich(), &back);
    assert!(decode("legacy.xls", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0]).is_err());
}

#[test]
fn schema_parses() {
    assert_eq!(SheetModel::schema().kit, "kit.sheet");
}

/// `KITION_SHEET_CORPUS=<dir>`: up to 40 `.xlsx`/`.xlsm` files there, each imported, written, read
/// back and compared. Skipped when unset.
#[test]
fn xlsx_corpus_round_trips() {
    let Ok(dir) = std::env::var("KITION_SHEET_CORPUS") else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx") || ext.eq_ignore_ascii_case("xlsm")))
        .collect();
    files.sort();
    files.truncate(40);
    let mut checked = 0;
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
        let Ok(original) = decode(&name, &bytes) else {
            eprintln!("{name}: not importable, skipped");
            continue;
        };
        assert_same(&name, &original, &round_trip(&original));
        checked += 1;
    }
    eprintln!("corpus: {checked} of {} files round-tripped", files.len());
}
