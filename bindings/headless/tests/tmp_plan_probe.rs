//! TEMPORARY (plain model round 5): prints what the kit answers to a premise plan after a reread,
//! to find why pin_adoption's dependent steps meet "Editor state unavailable". Deleted once known.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
mod common;

use common::Fixture;
use kition_kits::Caller;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn probe_plan_after_a_reread() {
    for outside in [false, true] {
        let f = Fixture::new();
        let _ = f.basis().await;
        if outside {
            f.colleague(&[("cell.set", json!({"cell":"Z20","input":"outside"}))]);
            let _ = f.basis().await;
        }
        let view = f.kits.open(Caller::Agent, "Book.xlsx").await.unwrap();
        let params = json!({"action":"begin","steps":[{"command":"home.insertSheet","params":{"name":"Planned"}}],"step":1});
        let answer = f.kits.context(Caller::Agent, "kit.sheet", &view.document_id, "plan", params.as_object().unwrap().clone()).await;
        println!("PROBE outside={outside}: {answer:?}");
        f.shutdown().await;
    }
    panic!("PROBE: see the lines above");
}
