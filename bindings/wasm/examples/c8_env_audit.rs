//! Classify every diagnostic emitted by the pinned environment's receipt audit.
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};

use serde_json::{Value, json};

fn count(counts: &mut BTreeMap<String, usize>, key: impl Into<String>) {
    *counts.entry(key.into()).or_default() += 1;
}

fn cause(event: &Value) -> String {
    let ops: Vec<_> = event["ops"].as_array().into_iter().flatten().collect();
    let op = ops.first().copied().unwrap_or(&Value::Null);
    let field = op["field"].as_str().unwrap_or("");
    let family = if event["reason"] == "not-replayable" {
        if op["type"] == "Name" { "replay-created-name-identity" } else { "replay-created-axis-identity" }
    } else if event["reason"] == "new-error" {
        if op["op"] == "delete" { "axis-deletion-ref-error" } else { "axis-move-circular-reference" }
    } else if event["reason"] == "refused" && op["target"].as_str().is_some_and(|target| target.starts_with("style:")) {
        "readonly-style-index-deletion"
    } else if event["reason"] == "instruction-incomplete" {
        match op["op"].as_str() {
            Some("create") => "axis-creation-index-and-cell-addresses",
            Some("delete") => "axis-deletion-index-and-cell-addresses",
            Some("list.move" | "list.remove") => "axis-move-index-and-cell-addresses",
            _ if field.ends_with("gone") => "axis-deletion-consequences",
            _ if field == "name" => "rename-cell-labels",
            _ => "multiple-stored-consequences",
        }
    } else if op["type"] == "Image" {
        "image-without-content"
    } else if op["type"] == "Sheet" {
        "new-sheet-identity-and-defaults"
    } else if op["type"] == "Name" {
        "new-name-defaults"
    } else if op["type"] == "Row" || op["type"] == "Col" {
        "new-axis-identity-and-index"
    } else if op["op"] == "list.move" && ["rows", "cols"].contains(&field) {
        "axis-move-index-and-cell-addresses"
    } else if op["op"] == "delete" || field.ends_with("gone") {
        "axis-deletion-and-cell-map-cleanup"
    } else if field == "name" {
        "sheet-rename-and-labels"
    } else if matches!(field, "props.defaultColWidth" | "props.defaultRowHeight") {
        "dimension-entry-normalization"
    } else {
        "other"
    };
    let edits: Vec<_> =
        event["reread"]["changed"].as_array().into_iter().flatten().flat_map(|change| change["edits"].as_array().into_iter().flatten()).collect();
    let churn = edits.iter().any(|edit| {
        edit["field"] == "cells" && {
            let keys = |value: &Value| value.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>());
            keys(&edit["before"]) == keys(&edit["after"]) && edit["before"] != edit["after"]
        }
    });
    let without_cell_maps = |diff: &Value| {
        let mut result = diff.clone();
        if let Some(changes) = result["changed"].as_array_mut() {
            for change in changes.iter_mut() {
                for key in ["fields", "edits"] {
                    if let Some(items) = change[key].as_array_mut() {
                        items.retain(|item| if key == "fields" { item != "cells" } else { item["field"] != "cells" });
                    }
                }
            }
            changes.retain(|change| change["fields"].as_array().is_some_and(|fields| !fields.is_empty()));
        }
        result
    };
    if churn && without_cell_maps(&event["receipt"]) == without_cell_maps(&event["reread"]) {
        "cell-reference-churn".into()
    } else if churn {
        format!("cell-reference-churn+{family}")
    } else {
        family.into()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut reasons = BTreeMap::new();
    let mut causes = BTreeMap::new();
    let mut kinds = BTreeMap::new();
    let mut tasks = BTreeMap::new();
    let mut events = Vec::new();
    let mut giveups = 0;
    for path in std::env::args().skip(1) {
        for line in BufReader::new(std::fs::File::open(path)?).lines() {
            let line = line?;
            if let Some(body) = line.strip_prefix("SHEET_AUDIT ") {
                let event: Value = serde_json::from_str(body)?;
                let diagnosis = cause(&event);
                count(&mut causes, format!("{}:{diagnosis}", event["reason"].as_str().unwrap_or("unknown")));
                events.push(json!({"copy":event["copy"],"kind":event["kind"],"reason":event["reason"],"cause":diagnosis,"ops":event["ops"]}));
            } else if let Some(body) = line.strip_prefix("kit-env: gave up task ") {
                let event: Value = serde_json::from_str(body)?;
                giveups += 1;
                for attempt in event["resampled"].as_array().into_iter().flatten() {
                    count(&mut reasons, attempt["reason"].as_str().unwrap_or("unknown"));
                    count(
                        &mut kinds,
                        format!("{}:{}", attempt["reason"].as_str().unwrap_or("unknown"), attempt["kind"].as_str().unwrap_or("unknown")),
                    );
                }
            } else if line.starts_with('{') {
                let task: Value = serde_json::from_str(&line)?;
                count(&mut tasks, task["kind"].as_str().unwrap_or("unknown"));
                for attempt in task["resampled"].as_array().into_iter().flatten() {
                    count(&mut reasons, attempt["reason"].as_str().unwrap_or("unknown"));
                    count(
                        &mut kinds,
                        format!("{}:{}", attempt["reason"].as_str().unwrap_or("unknown"), attempt["kind"].as_str().unwrap_or("unknown")),
                    );
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"tasks":tasks,"giveups":giveups,"discards":reasons,"discard_kinds":kinds,"audited_causes":causes,"audited_events":events})
        )?
    );
    Ok(())
}
