//! Imported rule priorities must not escape through the panic recovery guard.
#![allow(clippy::unwrap_used, clippy::panic)]
use kition_sheet_engine::{EngineError, Session};
use serde_json::json;

#[test]
fn exhausted_conditional_format_priority_returns_an_error_without_a_panic() {
    let mut session = Session::new();
    session.new_workbook();
    let params = json!({"range":"A1:B2","rule":{"type":"cellIs","value":3}});
    session.execute("conditionalFormat.apply", params.clone()).unwrap();
    std::sync::Arc::make_mut(&mut session.doc_mut().unwrap().wb).sheet_mut(0).unwrap().cond_formats[0].priority = u32::MAX - 1;
    session.execute("conditionalFormat.apply", params.clone()).unwrap();
    let before = session.doc().unwrap().wb.clone();
    assert_eq!(before.sheets[0].cond_formats[1].priority, u32::MAX);
    let result = session.execute("conditionalFormat.apply", params);
    assert!(
        matches!(result, Err(EngineError::BadParams { ref cmd, ref msg }) if cmd == "conditionalFormat.apply" && msg.contains("priority")),
        "{result:?}"
    );
    assert_eq!(session.doc().unwrap().wb.sheets[0].cond_formats, before.sheets[0].cond_formats);
}
