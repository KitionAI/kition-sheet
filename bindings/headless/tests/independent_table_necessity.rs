//! C8-sheet-read: explicit table names read a duplicate predicate, not every name.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn table_creation_holds_matching_names_and_overlaps_but_accepts_unrelated_tables_or_names() {
    for change in ["unrelated table", "duplicate table", "overlap", "duplicate name", "unrelated name"] {
        let fixture = Fixture::new();
        let basis = fixture.basis().await;
        match change {
            "unrelated table" => fixture.colleague(&[("insert.table", json!({"range":"Sheet2!A1:B3","header":true,"name":"Other"}))]),
            "duplicate table" => fixture.colleague(&[("insert.table", json!({"range":"Sheet2!A1:B3","header":true,"name":"newsales"}))]),
            "overlap" => fixture.colleague(&[("insert.table", json!({"range":"Sheet1!A3:B5","header":true,"name":"Other"}))]),
            "duplicate name" => fixture.colleague(&[("formulas.defineName", json!({"name":"newsales","refersTo":"=42"}))]),
            _ => fixture.colleague(&[("formulas.defineName", json!({"name":"Different","refersTo":"=42"}))]),
        }
        let outside = fixture.bytes();
        let result = fixture.run(basis, "insert.table", json!({"range":"Sheet1!A2:B4","header":true,"name":"NewSales"})).await;
        if matches!(change, "duplicate table" | "duplicate name" | "overlap") {
            assert!(result.as_ref().is_err_and(|error| error.to_string().contains("conflict")), "{change}: {result:?}");
            assert_eq!(fixture.bytes(), outside, "a refused insert preserves the outside writer's file");
        } else {
            result.unwrap_or_else(|error| panic!("{change}: unrelated allocation must apply: {error}"));
            let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            assert_eq!(saved.table("NewSales").map(|(si, ti)| saved.sheets[si].tables[ti].range.a1()), Some("A2:B4".into()));
            if change == "unrelated table" {
                assert!(saved.table("Other").is_some_and(|(si, ti)| si == 1 && saved.sheets[si].tables[ti].range.a1() == "A1:B3"));
            } else {
                assert!(saved.names.iter().any(|name| name.name == "Different" && name.formula == "42"));
            }
        }
        fixture.shutdown().await;
    }
}
