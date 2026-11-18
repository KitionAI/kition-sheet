//! `file.export`: the File tab's Export buttons and the agent's `sheet_export` take the file's
//! bytes from the engine as base64 and hand them to the host, which writes the new file (Kit UI
//! protocol `host/write` mode `export`, KHP A41). The engine writes nothing and the workbook stays
//! as it was.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn book() -> SheetDocument {
    let mut doc = SheetDocument::create("Sales.xlsx", 64, 36).unwrap();
    for (cell, input) in [("A1", "Region"), ("B1", "Sales"), ("A2", "North"), ("B2", "12"), ("A3", "South <&>"), ("B3", "=B2*2")] {
        doc.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
    }
    doc.execute("home.insertSheet", json!({ "name": "Second" })).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": "Second" })).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "second" })).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": "Sheet1" })).unwrap();
    doc
}

fn bytes(answer: &Value) -> Vec<u8> {
    kition_sheet_engine::io::base64_decode(answer["data"].as_str().unwrap()).unwrap()
}

#[test]
fn exports_the_workbook_as_pdf_bytes_without_changing_it() {
    let mut doc = book();
    let (revision, dirty, saved) = (doc.revision(), doc.dirty(), doc.save(None).unwrap());
    let answer = doc.execute("file.export", json!({ "format": "pdf" })).unwrap();
    let pdf = bytes(&answer);
    assert!(pdf.starts_with(b"%PDF-"), "{:?}", &pdf[..8.min(pdf.len())]);
    assert_eq!(
        (answer["format"].as_str(), answer["extension"].as_str(), answer["mime"].as_str()),
        (Some("pdf"), Some("pdf"), Some("application/pdf"))
    );
    assert_eq!(answer["bytes"], pdf.len());
    assert_eq!(answer["pages"], 1);
    assert!(answer.get("path").is_none(), "the host names and writes the file");
    let all = doc.execute("file.export", json!({ "format": "pdf", "sheets": "all" })).unwrap();
    assert_eq!(all["pages"], 2, "every sheet with something on it");
    assert_eq!((doc.revision(), doc.dirty()), (revision, dirty));
    assert_eq!(doc.save(None).unwrap(), saved);
}

#[test]
fn exports_a_sheet_as_csv_and_as_a_web_page() {
    let mut doc = book();
    let (revision, saved) = (doc.revision(), doc.save(None).unwrap());
    let csv = doc.execute("file.export", json!({ "format": "csv" })).unwrap();
    assert_eq!((csv["extension"].as_str(), csv["mime"].as_str()), (Some("csv"), Some("text/csv")));
    let text = String::from_utf8(bytes(&csv)).unwrap();
    assert!(text.starts_with("Region,Sales") && text.contains("North,12") && text.contains("24"), "{text}");
    let second = String::from_utf8(bytes(&doc.execute("file.export", json!({ "format": "csv", "sheet": "Second" })).unwrap())).unwrap();
    assert!(second.starts_with("second"), "{second}");
    let html = doc.execute("file.export", json!({ "format": "html" })).unwrap();
    assert_eq!((html["extension"].as_str(), html["mime"].as_str()), (Some("html"), Some("text/html")));
    let page = String::from_utf8(bytes(&html)).unwrap();
    assert!(page.contains("<table") && page.contains("South &lt;&amp;&gt;"), "{page}");
    assert_eq!(doc.revision(), revision);
    assert_eq!(doc.save(None).unwrap(), saved);
}

#[test]
fn refuses_a_format_or_sheet_it_cannot_write() {
    let mut doc = book();
    let error = doc.execute("file.export", json!({ "format": "xlsb" })).unwrap_err();
    assert!(error.contains("pdf, csv or html"), "{error}");
    assert!(doc.execute("file.export", json!({})).is_err(), "a format is required");
    assert!(doc.execute("file.export", json!({ "format": "csv", "sheet": 9 })).is_err());
    assert!(doc.execute("file.export", json!({ "format": "pdf", "path": "/tmp/x.pdf" })).is_err(), "no file system");
}

/// kit.json declares what `file.export` writes, for the host's export check and the agent's
/// `sheet_export` (`exports`, `agent.export`).
#[test]
fn the_manifest_declares_the_exports() {
    let manifest: Value = serde_json::from_str(include_str!("../../../kit.json")).unwrap();
    let exports: Vec<(&str, &str, &str)> = manifest["exports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["extension"].as_str().unwrap(), entry["mime"].as_str().unwrap(), entry["command"].as_str().unwrap()))
        .collect();
    assert_eq!(
        exports,
        [
            ("pdf", "application/pdf", "file.export"),
            ("csv", "text/csv", "file.export"),
            ("html", "text/html", "file.export"),
            // File › Export as Picture and Save as Template (module M9).
            ("png", "image/png", "file.export"),
            ("xltx", "application/vnd.openxmlformats-officedocument.spreadsheetml.template", "file.export"),
        ]
    );
    assert_eq!(manifest["agent"]["export"], json!({ "command": "file.export", "data": "data" }));
    let mut doc = book();
    for (extension, mime, command) in exports {
        let answer = doc.execute(command, json!({ "format": extension })).unwrap();
        assert_eq!((answer["extension"].as_str(), answer["mime"].as_str()), (Some(extension), Some(mime)));
        assert!(!bytes(&answer).is_empty());
    }
}
