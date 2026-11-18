//! `sheet_check` against its catalogue (`agent/checks.json`, KHP A38): every fixture in
//! `conformance/` (written by `examples/checks.rs`) reports its code after the XLSX round trip, every
//! code it reports is catalogued, and `basic.xlsx` reports nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::Value;

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn check(fixture: &str) -> Vec<kition_kit::diagnostics::Diagnostic> {
    let bytes = std::fs::read(repo().join("conformance").join(fixture)).unwrap_or_else(|error| panic!("{fixture}: {error}"));
    SheetDocument::open(fixture, &bytes).unwrap_or_else(|error| panic!("{fixture}: {error}")).check()
}

#[test]
fn every_fixture_reports_its_code() {
    let catalogue: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(repo().join("agent/checks.json")).unwrap()).unwrap();
    let codes: Vec<&str> = catalogue.iter().map(|entry| entry["code"].as_str().unwrap()).collect();
    let mut failures = Vec::new();
    for entry in &catalogue {
        let (code, fixture) = (entry["code"].as_str().unwrap(), entry["fixture"].as_str().unwrap());
        let found = check(fixture);
        if !found.iter().any(|d| d.code == code && serde_json::to_value(d.severity).unwrap() == entry["severity"]) {
            failures.push(format!("{fixture} does not report {code}: {:?}", found.iter().map(|d| &d.code).collect::<Vec<_>>()));
        }
        for d in &found {
            if !codes.contains(&d.code.as_str()) {
                failures.push(format!("{fixture} reports {} outside the catalogue", d.code));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_basic_fixture_is_clean() {
    let found: Vec<String> = check("basic.xlsx").into_iter().map(|d| d.code).collect();
    assert_eq!(found, Vec::<String>::new());
}
