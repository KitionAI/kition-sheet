//! C8-sheet-pin: conditional targets and public SDK contracts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn headless_logical_axis_history_includes_all_declared_derived_facts() {
    use kition_kits::Caller;
    let f = Fixture::new();
    let basis = f.basis().await;
    let view = f.kits.open(Caller::Agent, "Book.xlsx").await.unwrap();
    let values = f.kits.model(Caller::Agent, "kit.sheet", &view.document_id, "model.derived", Default::default()).await.unwrap();
    let objects = values["objects"].as_array().unwrap();
    let old = objects.iter().find(|object| object["id"] == "sheet:1").unwrap()["values"]["axisOrder"].clone();
    assert!(old.is_object(), "{values}");
    f.colleague(&[("cell.set", json!({"cell":"Z20","input":"outside"}))]);
    f.basis().await;
    let params = json!({"cell":"F5","input":"agent"});
    let under = f
        .kits
        .context(
            Caller::Agent,
            "kit.sheet",
            &view.document_id,
            "context.under",
            json!({"command":"cell.set","params":params}).as_object().unwrap().clone(),
        )
        .await
        .unwrap()
        .unwrap();
    let covered = f
        .kits
        .context(
            Caller::Agent,
            "kit.sheet",
            &view.document_id,
            "context.covered",
            json!({"command":"cell.set","params":params,"objects":[{"id":"sheet:1","before":{"~axisOrder":old}}]}).as_object().unwrap().clone(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(covered["under"], under["objects"], "headless historical coverage: {covered}");
    f.run(basis, "cell.set", params).await.unwrap();
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn raw_formula_assignment_holds_its_sources() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("cell.set", json!({"cell":"B2","input":"999"}))]);
    let before = f.bytes();
    let answer = f.call("batch", json!({"basis":basis,"ops":[{"op":"set","target":"cell:1!B7","field":"input","value":"=B2*2"}]})).await;
    let error = answer.expect_err("raw formula assignment must hold the same sources as cell.set");
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(f.bytes(), before);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dependent_steps_plan_from_their_own_created_sheet_and_selection() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("cell.set", json!({"cell":"Z20","input":"outside"}))]);
    f.call(
        "run",
        json!({"basis":basis,"steps":[
            {"command":"sheet.insert","params":{"name":"Planned"}},
            {"command":"select.set","params":{"range":"H8"}},
            {"command":"cell.set","params":{"input":"agent"}}
        ]}),
    )
    .await
    .unwrap();
    let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
    assert_eq!(saved.execute("cell.get", json!({"cell":"Planned!H8"})).unwrap()["value"], "agent");
    assert_eq!(saved.execute("cell.get", json!({"cell":"Sheet1!Z20"})).unwrap()["value"], "outside");
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reread_keeps_agent_undo_and_outside_cell_values() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.run(basis, "cell.set", json!({"cell":"H8","input":"agent"})).await.unwrap();
    f.colleague(&[("cell.set", json!({"cell":"Z20","input":"outside"}))]);
    // Reread imports the outside difference while retaining the agent's history.
    let basis = f.basis().await;
    f.run(basis, "edit.undo", json!({})).await.unwrap();
    let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
    assert_eq!(saved.execute("cell.get", json!({"cell":"H8"})).unwrap()["value"], serde_json::Value::Null);
    assert_eq!(saved.execute("cell.get", json!({"cell":"Z20"})).unwrap()["value"], "outside");
    f.shutdown().await;
}
