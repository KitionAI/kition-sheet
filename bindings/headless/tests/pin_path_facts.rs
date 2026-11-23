//! C8-sheet-pin: behavior probes discovered by the full path comparison.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;
use common::Fixture;
use serde_json::{Value, json};

fn refused(result: &Result<Value, Value>, before: &[u8], fixture: &Fixture, label: &str, failures: &mut Vec<String>) {
    if !result.as_ref().is_err_and(|error| error["error"]["code"] == "conflict") {
        failures.push(format!("{label}: expected refusal, got {result:?}"));
    }
    if fixture.bytes() != before {
        failures.push(format!("WRONG_WRITE {label}"));
    }
}

#[tokio::test]
async fn chart_move_holds_the_stack_order_it_replaces() {
    let fixture = Fixture::new();
    for title in ["One", "Two", "Three"] {
        fixture.colleague(&[("insert.chart", json!({"range":"A1:C4","title":title}))]);
    }
    let basis = fixture.basis().await;
    fixture.colleague_rewrite(|wb| wb.sheet_mut(0).unwrap().charts.swap(0, 1));
    let before = fixture.bytes();
    let result = fixture.run(basis, "chart.move", json!({"chart":1})).await;
    let mut failures = Vec::new();
    refused(&result, &before, &fixture, "chart stack", &mut failures);
    fixture.shutdown().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn displayed_filter_values_hold_workbook_date_interpretation() {
    let fixture = Fixture::new();
    fixture.colleague(&[
        ("insert.table", json!({"range":"A1:D12","name":"Sales","header":true})),
        ("home.numberFormat", json!({"range":"B2:B12","code":"yyyy-mm-dd"})),
    ]);
    let basis = fixture.basis().await;
    fixture.colleague_rewrite(|wb| wb.date_system = kition_sheet_engine::core::DateSystem::D1904);
    let before = fixture.bytes();
    let result = fixture.run(basis, "data.filterBy", json!({"table":"Sales","column":"B","values":["1900-01-02"]})).await;
    let mut failures = Vec::new();
    refused(&result, &before, &fixture, "filter date system", &mut failures);
    fixture.shutdown().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn displayed_filter_values_hold_the_default_format() {
    let fixture = Fixture::new();
    fixture.colleague(&[("insert.table", json!({"range":"A1:D12","name":"Sales","header":true}))]);
    let basis = fixture.basis().await;
    fixture.colleague_rewrite(|wb| {
        let mut style = wb.styles.get(kition_sheet_engine::model::StyleId::DEFAULT).clone();
        style.num_fmt = kition_sheet_engine::model::NumFmt::new("0.00");
        wb.styles.set_default(style);
    });
    let before = fixture.bytes();
    let result = fixture.run(basis, "data.filterBy", json!({"table":"Sales","column":"B","values":["2.00"]})).await;
    let mut failures = Vec::new();
    refused(&result, &before, &fixture, "filter default format", &mut failures);
    fixture.shutdown().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn default_name_definition_holds_its_selection_address() {
    let fixture = Fixture::new();
    let basis = fixture.basis().await;
    fixture.colleague(&[("home.insertRows", json!({"rows":"2"}))]);
    let before = fixture.bytes();
    let result = fixture
        .call(
            "run",
            json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[
                {"command":"formulas.defineName","on":["C5"],"params":{"name":"Chosen"}}
            ]}),
        )
        .await;
    let mut failures = Vec::new();
    refused(&result, &before, &fixture, "default name address", &mut failures);
    assert!(result.as_ref().is_err_and(|error| error["conflicts"].to_string().contains("relocated")), "{result:?}");
    fixture.shutdown().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn chart_assignment_holds_the_destination_address() {
    let fixture = Fixture::new();
    fixture.colleague(&[
        ("insert.chart", json!({"range":"A1:C4","at":"H1"})),
        ("object.setAnchorMode", json!({"kind":"chart","id":1,"mode":"moveOnly"})),
    ]);
    let basis = fixture.basis().await;
    fixture.colleague(&[("home.insertRows", json!({"rows":"5"}))]);
    let before = fixture.bytes();
    let result = fixture.run(basis, "chart.set", json!({"chart":1,"at":"C10"})).await;
    let mut failures = Vec::new();
    refused(&result, &before, &fixture, "chart assignment address", &mut failures);
    fixture.shutdown().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn conditional_edits_hold_shared_connector_inputs() {
    let fixture = Fixture::new();
    fixture.colleague(&[
        ("drawing.insert", json!({"kind":"shape","frame":{"x":220,"y":100,"w":80,"h":40}})),
        ("drawing.insert", json!({"kind":"shape","frame":{"x":380,"y":160,"w":80,"h":40}})),
        ("drawing.connect", json!({"from":1,"to":2})),
    ]);
    let basis = fixture.basis().await;
    fixture.colleague(&[("drawing.transform", json!({"id":1,"dx":30}))]);
    let before = fixture.bytes();
    let result = fixture.run(basis, "home.conditionalFormat", json!({"range":"B2","rule":{"type":"blanks"}})).await;
    let mut failures = Vec::new();
    refused(&result, &before, &fixture, "conditional connector tail", &mut failures);
    fixture.shutdown().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn automatic_calculation_holds_header_synchronization_inputs() {
    let fixture = Fixture::new();
    fixture.colleague(&[("insert.table", json!({"range":"A1:D12","name":"Sales","header":true}))]);
    let basis = fixture.basis().await;
    // Reopening XLSX synchronizes header text to the stored column name. A raw
    // second caller's model write preserves the stale header this tail repairs.
    let outside = kition_kits::tools::call_as(
        &fixture.kits,
        "sheet_batch",
        &json!({"path":"Book.xlsx","ops":[{"op":"set","target":"sheet:1","field":"tables.0.columns.0.name","value":"Outside"}]}),
        kition_kits::tools::CallOptions { guard: kition_kits::tools::Guard::Off, ..Default::default() },
        "outside",
    )
    .await
    .unwrap();
    assert!(outside.is_ok(), "outside header write: {outside:?}");
    let before = fixture.bytes();
    let result = fixture.run(basis, "formulas.calculationOptions", json!({"mode":"automatic"})).await;
    let mut failures = Vec::new();
    refused(&result, &before, &fixture, "calculation header sync", &mut failures);
    fixture.shutdown().await;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn numeric_shape_writes_hold_the_owner_used_to_resolve_them() {
    let mut failures = Vec::new();
    for (command, params) in [("drawing.text", json!({"id":1,"text":"Agent"})), ("drawing.format", json!({"ids":[1],"fill":{"color":"#123456"}}))] {
        let fixture = Fixture::new();
        fixture.colleague(&[("drawing.insert", json!({"kind":"textBox","text":"Original","frame":{"x":20,"y":20,"w":80,"h":40}}))]);
        let basis = fixture.basis().await;
        fixture.colleague_rewrite(|wb| {
            let shape = wb.sheet_mut(0).unwrap().shapes.remove(0);
            wb.sheet_mut(1).unwrap().shapes.push(shape);
        });
        let before = fixture.bytes();
        let result = fixture.run(basis, command, params).await;
        refused(&result, &before, &fixture, command, &mut failures);
        fixture.shutdown().await;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
