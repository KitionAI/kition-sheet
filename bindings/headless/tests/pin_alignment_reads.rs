//! C8-sheet-pin: alignment holds geometry through anchor conversion, not shape edges.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
// The native headless fixture deliberately backdates an outside file's mtime.
#[allow(clippy::disallowed_methods)]
mod common;
use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[test]
fn alignment_compares_newly_materialized_column_sizes_against_implicit_defaults() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"D12","input":"source"})).unwrap();
    for frame in [json!({"x":128,"y":40,"w":300,"h":240}), json!({"x":256,"y":80,"w":60,"h":40})] {
        doc.execute("drawing.insert", json!({"kind":"shape","frame":frame})).unwrap();
    }
    doc.version().unwrap();
    let before = doc.derived(&[json!("axis:all")], &["contents".into()]).unwrap();
    let before = before.iter().find(|value| value.id == "axis:all").unwrap().values["contents"].clone();
    let mut outside = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    outside.execute("column.setWidth", json!({"cols":"F","width":40})).unwrap();
    doc.reconcile_reread(&SheetDocument::open("Book.xlsx", &outside.save(None).unwrap()).unwrap()).unwrap();
    let under = doc.under_step("arrange.align", &json!({"kind":"shape","ids":[1,2],"align":"right"}), &Default::default()).unwrap().unwrap();
    let changed = under.content.iter().any(|content| {
        content.region["axis"].as_str().is_some_and(|id| id.starts_with("col:"))
            && doc
                .content_at(&kition_kit::ContentAsk { premise: content.clone(), before: before.clone(), step: Default::default() }, &NoBlobs)
                .unwrap()
                == Some(true)
    });
    assert!(changed, "a newly materialized column must compare against its implicit size: {under:?}");
}

struct NoBlobs;
impl kition_kit::HistoricalBlobs for NoBlobs {
    fn read(&self, _hash: &str, _offset: usize, _length: usize) -> Result<Vec<u8>, String> {
        Err("alignment geometry reads no blobs".into())
    }
}

#[tokio::test]
async fn alignment_ignores_axes_beyond_anchor_conversions_and_retains_source_geometry() {
    let mut failures = Vec::new();
    for command in ["arrange.align", "arrange.group"] {
        for change in ["tail row size", "tail row deleted", "tail col size", "tail col deleted", "shape fill", "source row", "source frame"] {
            let f = Fixture::new();
            f.colleague(&[
                ("drawing.insert", json!({"kind":"shape","frame":{"x":128,"y":40,"w":300,"h":240}})),
                ("drawing.insert", json!({"kind":"shape","frame":{"x":256,"y":40,"w":300,"h":240}})),
                ("object.setAnchorMode", json!({"kind":"shape","id":1,"mode":"absolute"})),
                ("object.setAnchorMode", json!({"kind":"shape","id":2,"mode":"absolute"})),
            ]);
            let (observed, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
            let basis = f.basis().await;
            f.colleague(&[match change {
                "tail row size" => ("row.setHeight", json!({"rows":"10","height":44})),
                "tail row deleted" => ("row.delete", json!({"rows":"10"})),
                "tail col size" => ("column.setWidth", json!({"cols":"H","width":99})),
                "tail col deleted" => ("column.delete", json!({"cols":"H"})),
                "shape fill" => ("chart.shapeFill", json!({"kind":"shape","id":1,"color":"#123456"})),
                "source row" => ("row.setHeight", json!({"rows":"1","height":44})),
                "source frame" => ("drawing.transform", json!({"id":1,"dx":20})),
                _ => panic!("unknown change"),
            }]);
            let before = f.bytes();
            if change.starts_with("tail") {
                let (outside, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &before).unwrap();
                assert_eq!(outside.sheets[0].shapes, observed.sheets[0].shapes, "{change} must preserve the alignment operands");
            }
            let params =
                if command == "arrange.group" { json!({"kind":"shape","ids":[1,2]}) } else { json!({"kind":"shape","ids":[1,2],"align":"right"}) };
            let result = f.run(basis, command, params.clone()).await;
            if matches!(change, "source row" | "source frame") {
                let error = result.expect_err("changed alignment geometry must refuse");
                assert_eq!(error["error"]["code"], "conflict", "{command}, {change}: {error}");
                assert_eq!(f.bytes(), before, "refusal must not write");
            } else if let Err(error) = result {
                failures.push(format!("{command}, {change}: unrelated alignment input refused: {error}"));
            } else {
                let mut expected = SheetDocument::open("Book.xlsx", &before).unwrap();
                expected.execute(command, params).unwrap();
                let (expected, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &expected.save(None).unwrap()).unwrap();
                let (actual, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
                assert_eq!(
                    actual.sheets[0].shapes, expected.sheets[0].shapes,
                    "{command}, {change}: shape geometry or content differs from a fresh write"
                );
                assert_eq!(actual.sheets[0].rows, expected.sheets[0].rows);
                assert_eq!(actual.sheets[0].cols, expected.sheets[0].cols);
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn alignment_holds_result_anchor_prefixes_beyond_the_source_anchors() {
    for how in ["right", "bottom"] {
        let f = Fixture::new();
        f.colleague(&[
            ("drawing.insert", json!({"kind":"shape","frame":{"x":128,"y":40,"w":300,"h":240}})),
            ("drawing.insert", json!({"kind":"shape","frame":{"x":256,"y":80,"w":60,"h":40}})),
            ("object.setAnchorMode", json!({"kind":"shape","id":1,"mode":"absolute"})),
            ("object.setAnchorMode", json!({"kind":"shape","id":2,"mode":"absolute"})),
        ]);
        let (observed, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
        let basis = f.basis().await;
        f.colleague(&[if how == "right" {
            ("column.setWidth", json!({"cols":"F","width":40}))
        } else {
            ("row.setHeight", json!({"rows":"10","height":10}))
        }]);
        let before = f.bytes();
        let (outside, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &before).unwrap();
        assert_eq!(outside.sheets[0].shapes, observed.sheets[0].shapes, "the source frames must stay unchanged");
        let error = f.run(basis, "arrange.align", json!({"kind":"shape","ids":[1,2],"align":how})).await.expect_err("result geometry must be held");
        assert_eq!(error["error"]["code"], "conflict", "{how}: {error}");
        assert!(error.to_string().contains("changed"), "{how}: {error}");
        assert_eq!(f.bytes(), before, "refusal must not write");
        f.shutdown().await;
    }
}
