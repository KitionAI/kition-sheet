//! C8-sheet-same-field: clears hold the rule areas they remove.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod independent_common;

use independent_common::Fixture;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn clear_rules_refuses_changed_rule_ranges_and_accepts_unrelated_shifts() {
    for (range, command, rows, necessary) in [
        // same-field decision: the shifted range belongs to the removed rule.
        ("A7:B8", "home.insertRows", "Sheet1!2", true),
        ("A8:B9", "home.insertRows", "Sheet1!2", true),
        ("A9:B10", "home.deleteRows", "Sheet1!2", true),
        ("A7:B8", "home.insertRows", "Sheet1!12", false),
    ] {
        let fixture = Fixture::new();
        fixture.colleague(&[("home.conditionalFormat", json!({"range":format!("Sheet1!{range}"),"rule":{"type":"blanks"}}))]);
        let basis = fixture.basis().await;
        fixture.colleague(&[(command, json!({"rows":rows}))]);
        let outside = fixture.bytes();
        let result = fixture.run(basis, "home.clearRules", json!({"range":"Sheet1!A7:B8"})).await;
        if necessary {
            assert!(result.as_ref().is_err_and(|error| error.to_string().contains("conflict")), "{range}/{command}: {result:?}");
            if range == "A7:B8" {
                assert!(result.as_ref().unwrap_err()["conflicts"].to_string().contains("changed"));
            }
            assert_eq!(fixture.bytes(), outside, "a refused clear preserves the outside writer's file");
        } else {
            result.unwrap_or_else(|error| panic!("{range}/{command}: unchanged intersection must apply: {error}"));
            let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
            let sheet = saved.sheet(0).unwrap();
            assert!(sheet.cond_formats.is_empty());
            let source = if rows.ends_with("!2") { "B8" } else { "B7" };
            assert_eq!(sheet.value(kition_sheet_engine::core::CellRef::parse(source).unwrap()).display(), "7");
        }
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn clearing_one_rule_preserves_another_rules_shifted_range() {
    // same-field decision: the other rule's ranges are not removed.
    let fixture = Fixture::new();
    fixture.colleague(&[
        ("home.conditionalFormat", json!({"range":"Sheet1!D9:E10","rule":{"type":"blanks"}})),
        ("home.conditionalFormat", json!({"range":"Sheet1!A2:B3","rule":{"type":"blanks"}})),
    ]);
    let basis = fixture.basis().await;
    fixture.colleague(&[("home.insertRows", json!({"rows":"Sheet1!8"}))]);
    fixture.run(basis, "home.clearRules", json!({"range":"Sheet1!A2:B3"})).await.expect("an unremoved rule's range is unrelated");
    let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &fixture.bytes()).unwrap();
    let rules = &saved.sheet(0).unwrap().cond_formats;
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].ranges[0].a1(), "D10:E11");
    fixture.shutdown().await;
}
