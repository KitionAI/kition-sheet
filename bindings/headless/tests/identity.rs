//! `document.identity` and `document.fork` (Kit Process Protocol section 5.6, KitModel M7a)
//! through the real `kition-sheet` process: an `.xlsx` is written from the workbook and embeds no
//! replica. Its `xl/kition/identities.json` part keeps object ids for the next open, but no
//! document id and no history, so the file carries no document id and opening it makes a new
//! replica each time. A host learns that without parsing the file, and a fork of a workbook is the
//! same bytes, with no id.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kit_host::{HeadlessKit, LaunchOptions};
use kition_sheet_kit::SheetDocument;
use serde_json::json;

const LIMIT: Duration = Duration::from_secs(30);

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-sheet-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

async fn launch(dir: &Path) -> HeadlessKit {
    let mut options = LaunchOptions::new(env!("CARGO_BIN_EXE_kition-sheet"));
    options.workspace = Some(dir.to_path_buf());
    HeadlessKit::launch(options).await.unwrap()
}

/// A workbook the kit saved: a label, a number and a formula.
fn workbook() -> Vec<u8> {
    let mut doc = SheetDocument::create("Budget.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "Rent" })).unwrap();
    doc.execute("cell.set", json!({ "cell": "B1", "input": "1200" })).unwrap();
    doc.execute("cell.set", json!({ "cell": "C1", "input": "=B1*2" })).unwrap();
    doc.save(Some("Budget.xlsx")).unwrap()
}

#[tokio::test]
async fn a_workbook_file_embeds_no_replica_so_it_carries_no_identity_and_forks_unchanged() {
    let dir = workspace("plain");
    let bytes = workbook();
    std::fs::write(dir.join("Budget.xlsx"), &bytes).unwrap();
    let kit = launch(&dir).await;
    assert!(kit.serves("document.identity"), "{}", kit.capabilities());

    // A workbook, a byte copy of it and an empty file (made for the kit before it saved one)
    // carry no identity: the file holds the workbook, not its replica.
    std::fs::copy(dir.join("Budget.xlsx"), dir.join("Copy.xlsx")).unwrap();
    std::fs::write(dir.join("New.xlsx"), b"").unwrap();
    for path in ["Budget.xlsx", "Copy.xlsx", "New.xlsx"] {
        assert_eq!(kit.identity(path, LIMIT).await.unwrap().doc_id, None, "{path}");
    }

    // A fork is the same bytes, with no id, and leaves the source as it was.
    let fork = kit.fork("Budget.xlsx", "Fork.xlsx", LIMIT).await.unwrap();
    assert_eq!(fork.doc_id, None);
    assert_eq!(fork.name, "Fork.xlsx");
    assert!(!fork.version.is_empty());
    assert_eq!(std::fs::read(dir.join("Fork.xlsx")).unwrap(), bytes, "a file without a replica forks unchanged");
    assert_eq!(std::fs::read(dir.join("Budget.xlsx")).unwrap(), bytes, "the source is unchanged");
    let mut opened = SheetDocument::open("Fork.xlsx", &std::fs::read(dir.join("Fork.xlsx")).unwrap()).unwrap();
    assert_eq!(opened.execute("cell.get", json!({ "cell": "C1" })).unwrap()["value"], json!(2400.0));
    let empty = kit.fork("New.xlsx", "NewCopy.xlsx", LIMIT).await.unwrap();
    assert_eq!(empty.doc_id, None);
    assert_eq!(std::fs::read(dir.join("NewCopy.xlsx")).unwrap(), b"");

    // A fork never replaces a file.
    std::fs::write(dir.join("Taken.xlsx"), b"someone else's file").unwrap();
    let taken = kit.fork("Budget.xlsx", "Taken.xlsx", LIMIT).await.unwrap_err();
    assert_eq!(taken.reason(), Some("file_conflict"), "{taken}");
    assert_eq!(std::fs::read(dir.join("Taken.xlsx")).unwrap(), b"someone else's file", "the existing file is kept");
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}
