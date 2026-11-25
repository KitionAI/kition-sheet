//! C8-sheet-pin: published SDK receipt and replacement contracts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
// The native headless fixture deliberately backdates an outside file's mtime.
#[allow(clippy::disallowed_methods)]
mod common;

use common::Fixture;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::{KitBackend, OpenRequest, protocol::Origin};
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test]
async fn explicit_null_content_regions_reach_the_kit_callback() {
    let f = Fixture::new();
    let view = f.kits.open(kition_kits::Caller::Agent, "Book.xlsx").await.unwrap();
    let answer = f
        .kits
        .context(
            kition_kits::Caller::Agent,
            "kit.sheet",
            &view.document_id,
            "context.content",
            json!({"id":"workbook","field":"~unknown","region":null,"before":null}).as_object().unwrap().clone(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(answer, json!({"changed":null}), "an unsupported region is unknown, not an invalid protocol request");
    f.shutdown().await;
}

#[tokio::test]
async fn private_derived_facts_stay_out_of_receipts_and_still_guard_sources() {
    let f = Fixture::new();
    let basis = f.basis().await;
    let output = f.run(basis, "cell.set", json!({"cell":"B7","input":"42"})).await.unwrap();
    for item in output["changes"]["changed"].as_array().unwrap() {
        for field in item["fields"].as_array().unwrap() {
            if let Some(name) = field.as_str().unwrap().strip_prefix('~') {
                assert!(matches!(name, "computed" | "hiddenCount" | "ink"), "private fact in public receipt: {output}");
            }
        }
    }
    let basis = f.basis().await;
    f.colleague(&[("cell.set", json!({"cell":"B7","input":"99"}))]);
    let before = f.bytes();
    let output = f.run(basis, "row.autofitHeight", json!({"rows":"7"})).await;
    assert!(output.as_ref().is_err_and(|error| error["error"]["code"] == "conflict"), "{output:?}");
    assert_eq!(f.bytes(), before);
    f.shutdown().await;
}

#[test]
fn replica_exports_and_saved_bytes_follow_a_replaced_document() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = SheetDocument::create("Old.xlsx", 0, 0).unwrap();
    old.execute("cell.set", json!({"cell":"A1","input":"old"})).unwrap();
    std::fs::write(dir.path().join("Old.xlsx"), old.save(None).unwrap()).unwrap();
    let folder = HostFolder::new(dir.path()).unwrap();
    let mut kit = Headless::<SheetDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_sheet_kit::EDITOR_STATE).unwrap(),
    );
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("Old.xlsx").unwrap(), read_only: false, create: None }).unwrap();
    let id = opened.id;
    let previous = kit.model("model.export", json!({"documentId":id})).unwrap();
    kit.execute(&id, "file.new", Default::default(), Origin::Agent).unwrap();
    let saved = kit.model("model.save", json!({"documentId":id})).unwrap();
    let exported = kit.model("model.export", json!({"documentId":id})).unwrap();
    assert!(saved["data"].as_str().is_some_and(|data| !data.is_empty()), "{saved}");
    assert!(exported["changes"].as_str().is_some_and(|data| !data.is_empty()), "{exported}");
    assert_ne!(exported["changes"], previous["changes"]);
    let read = kit.execute(&id, "cell.get", json!({"cell":"A1"}).as_object().unwrap().clone(), Origin::Agent).unwrap();
    assert_eq!(read.result.unwrap()["value"], serde_json::Value::Null);
}
