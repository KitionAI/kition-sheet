//! Measure actual headless receipts, including SDK footprint compaction.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::path::PathBuf;

use kition_kit::KitDocument;
use kition_kits::{KitDefinition, KitManager, tools};
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn property_offset_and_group_receipt_sizes() {
    for (children, form) in [(1, "property"), (1, "one"), (5, "parent"), (200, "parent"), (200, "all")] {
        let workspace = tempfile::tempdir().unwrap();
        let mut doc = SheetDocument::create("Size.xlsx", 0, 0).unwrap();
        let mut ids = Vec::new();
        for i in 0..children {
            let result = doc.execute("drawing.insert", json!({"kind":"shape","frame":{"x":(i%20)*20,"y":(i/20)*20,"w":10,"h":10}})).unwrap();
            ids.push(result["id"].clone());
        }
        let group = if form == "parent" { Some(doc.execute("drawing.group", json!({"ids":ids})).unwrap()["id"].clone()) } else { None };
        std::fs::write(workspace.path().join("Size.xlsx"), doc.save(None).unwrap()).unwrap();
        let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(workspace.path().to_path_buf()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-sheet"))))]);
        let (command, params) = match form {
            "property" => ("drawing.format", json!({"ids":[ids[0]],"fill":"#C00000"})),
            "one" => ("drawing.transform", json!({"ids":[ids[0]],"dx":5,"dy":3})),
            "parent" => ("drawing.transform", json!({"ids":[group.unwrap()],"dx":5,"dy":3})),
            _ => ("drawing.transform", json!({"ids":ids,"dx":5,"dy":3})),
        };
        let answer = tools::call(&kits, "sheet_run", &json!({"path":"Size.xlsx","steps":[{"command":command,"params":params}]}))
            .await
            .unwrap()
            .unwrap()
            .output;
        assert!(answer.get("error").is_none(), "{answer}");
        assert_ne!(answer["changes"], json!({"created":[],"changed":[],"moved":[],"deleted":[]}), "{answer}");
        println!("RECEIPT {form} children={children} changes={} response={}", answer["changes"].to_string().len(), answer.to_string().len());
        if children == 200 {
            println!("RECEIPT-DETAIL {form} {}", answer["changes"]);
        }
        kits.shutdown().await;
    }
}
