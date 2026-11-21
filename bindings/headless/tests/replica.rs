//! `kition-sheet` opens a sync room's state with no file, as a server's headless kit pool does
//! (`model.open`, `model.import`, `model.export`, `model.save`; kition-sdk `docs/platform/reference/object-model.md`).

use kition_headless::{Headless, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::{ErrorReason, KitBackend};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn kit() -> Headless<SheetDocument> {
    Headless::with_replica_port(KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_sheet_kit::EDITOR_STATE).unwrap())
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for k in 0..4 {
            out.push(if k <= chunk.len() { char::from(BASE64[((n >> (18 - 6 * k)) & 63) as usize]) } else { '=' });
        }
    }
    out
}

fn decode(text: &str) -> Vec<u8> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = BASE64.iter().position(|known| *known == byte).expect("base64");
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    out
}

fn cell(doc: &mut SheetDocument, a1: &str) -> Value {
    doc.execute("cell.get", json!({ "cell": a1 })).unwrap()["value"].clone()
}

/// The room's workbook, as a collaborator holds it.
fn peer() -> SheetDocument {
    let mut doc = SheetDocument::create("Budget.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "Rent" })).unwrap();
    doc.version().unwrap();
    doc
}

#[test]
fn opens_a_room_without_a_file_and_imports_exports_and_saves_it() {
    let mut kit = kit();
    let features = kit.capabilities()["features"].clone();
    for feature in ["model.open", "model.import", "model.export", "model.save"] {
        assert!(features.as_array().unwrap().iter().any(|listed| listed == feature), "{feature} in {features}");
    }

    let mut peer = peer();
    let (snapshot, blobs) = (peer.snapshot().unwrap(), peer.blobs().unwrap());
    let opened = kit.model("model.open", json!({ "name": "Budget.xlsx", "snapshot": encode(&snapshot), "blobs": encode(&blobs) })).unwrap();
    let id = opened["documentId"].as_str().unwrap().to_owned();
    assert_eq!(opened["format"], "xlsx");
    assert_eq!(opened["title"], "Budget.xlsx");

    // A collaborator's change comes in.
    let since = peer.version().unwrap();
    peer.execute("cell.set", json!({ "cell": "B1", "input": "1200" })).unwrap();
    let change = peer.export_changes(&since).unwrap();
    kit.model("model.import", json!({ "documentId": id, "changes": encode(&change) })).unwrap();

    // The kit edits on top of it, and the collaborator takes what it exports.
    let batch = json!({ "documentId": id, "ops": [{ "op": "command", "id": "cell.set", "params": { "cell": "C1", "input": "=B1*2" } }] });
    kit.model("model.batch", batch).unwrap();
    let exported = kit.model("model.export", json!({ "documentId": id, "since": encode(&peer.version().unwrap()) })).unwrap();
    peer.import_changes(&decode(exported["changes"].as_str().unwrap())).unwrap();
    assert_eq!(cell(&mut peer, "C1"), json!(2400.0));
    let everything = kit.model("model.export", json!({ "documentId": id })).unwrap();
    let mut joined = SheetDocument::open_with_replica("Budget.xlsx", &[], &decode(everything["changes"].as_str().unwrap()), &[]).unwrap();
    assert_eq!(joined.version().unwrap(), peer.version().unwrap(), "every change the document has");

    let saved = kit.model("model.save", json!({ "documentId": id })).unwrap();
    assert_eq!((saved["name"].as_str(), saved["format"].as_str()), (Some("Budget.xlsx"), Some("xlsx")));
    let mut file = SheetDocument::open("Budget.xlsx", &decode(saved["data"].as_str().unwrap())).unwrap();
    assert_eq!((cell(&mut file, "A1"), cell(&mut file, "B1"), cell(&mut file, "C1")), (json!("Rent"), json!(1200.0), json!(2400.0)));
}

#[test]
fn never_opens_an_empty_workbook_for_a_replica_it_cannot_read() {
    let mut kit = kit();
    for (snapshot, blobs) in [(&b"not a replica"[..], &[][..]), (&b""[..], &[][..])] {
        let error = kit.model("model.open", json!({ "name": "Budget.xlsx", "snapshot": encode(snapshot), "blobs": encode(blobs) })).unwrap_err();
        assert_eq!(error.code, ErrorReason::UnsupportedFormat.code(), "{error:?}");
    }
    let snapshot = peer().snapshot().unwrap();
    let error = kit.model("model.open", json!({ "name": "Budget.xlsx", "snapshot": encode(&snapshot), "blobs": encode(&[1, 2, 3]) })).unwrap_err();
    assert_eq!(error.code, ErrorReason::UnsupportedFormat.code(), "damaged blobs: {error:?}");
}

/// Changes the workbook cannot take (here, from a newer build whose schema this kit does not
/// read) are refused, and the document is closed.
#[test]
fn refuses_changes_it_cannot_take_and_closes_the_document() {
    use kition_kit::kdm::{ActorId, Origin, Replica, Schema};
    use std::sync::Arc;

    let mut kit = kit();
    let mut peer = peer();
    let snapshot = peer.snapshot().unwrap();
    let opened = kit.model("model.open", json!({ "name": "Budget.xlsx", "snapshot": encode(&snapshot) })).unwrap();
    let id = opened["documentId"].as_str().unwrap().to_owned();

    let mut raw: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    let newer = raw["schemaVersion"].as_u64().unwrap() + 1;
    raw["schemaVersion"] = json!(newer);
    let mut newer_peer = Replica::load(Arc::new(Schema::parse(&raw.to_string()).unwrap()), &snapshot, ActorId::from_u64(77).unwrap()).unwrap();
    let since = newer_peer.version();
    newer_peer.transact(&Origin::system("migrate"), |tx| tx.raise_schema_version(u32::try_from(newer).unwrap()).map(|_| ())).unwrap();
    let refused = newer_peer.export_changes(&since).unwrap();

    let error = kit.model("model.import", json!({ "documentId": id, "changes": encode(&refused) })).unwrap_err();
    assert_eq!(error.code, ErrorReason::InvalidParams.code(), "{error:?}");
    assert_eq!(error.data.as_ref().unwrap()["refused"], true);
    let closed = kit.model("model.export", json!({ "documentId": id })).unwrap_err();
    assert_eq!(closed.code, ErrorReason::DocumentNotFound.code());
}
