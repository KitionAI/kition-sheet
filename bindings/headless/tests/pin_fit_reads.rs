//! C8-sheet-pin: fitting reads height contributions, not unrelated row contents.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "independent_common/mod.rs"]
// The native headless fixture deliberately backdates an outside file's mtime.
#[allow(clippy::disallowed_methods)]
mod common;
use common::Fixture;
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn conflict(result: Result<Value, Value>, before: &[u8], fixture: &Fixture) {
    let error = result.expect_err("changed fit input must refuse");
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(fixture.bytes(), before, "refusal must not write");
}

#[tokio::test]
async fn derived_fit_allows_plain_neighbor_changes_but_holds_tall_neighbors_and_target_styles() {
    let mut failures = Vec::new();
    for (command, params) in [
        ("format.bold", json!({"range":"C5","on":true})),
        ("format.color", json!({"range":"C5","color":"#123456"})),
        ("cell.formatText", json!({"cell":"C5","bold":true})),
        ("edit.clearFormats", json!({"range":"C5"})),
        ("edit.paste", json!({"at":"C5","text":"42"})),
        ("edit.fillDown", json!({"range":"C4:C5"})),
        ("data.sort", json!({"range":"C4:C5","header":false,"keys":[{"column":"C","order":"desc"}]})),
        ("checkbox.insert", json!({"range":"C5"})),
    ] {
        for change in ["plain value", "neighbor color", "tall neighbor", "target style"] {
            let f = Fixture::new();
            if command == "cell.formatText" {
                f.colleague(&[("cell.set", json!({"cell":"C5","input":"target text"}))]);
            }
            let basis = f.basis().await;
            f.colleague(&[match change {
                "plain value" => ("cell.set", json!({"cell":"D5","input":"99"})),
                "neighbor color" => ("format.color", json!({"range":"D5","color":"#CC0000"})),
                "tall neighbor" => ("format.fontSize", json!({"range":"D5","size":30})),
                "target style" => ("format.italic", json!({"range":"C5","on":true})),
                _ => panic!("unknown change"),
            }]);
            let before = f.bytes();
            let result = f.run(basis, command, params.clone()).await;
            if matches!(change, "tall neighbor" | "target style") {
                conflict(result, &before, &f);
            } else if let Err(error) = result {
                failures.push(format!("{command}, {change}: unrelated row fact refused: {error}"));
            } else {
                let mut expected = SheetDocument::open("Book.xlsx", &before).unwrap();
                expected.execute(command, params.clone()).unwrap();
                let mut actual = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
                for cell in ["C4", "C5", "D5"] {
                    assert_eq!(
                        actual.execute("cell.get", json!({"cell":cell})).unwrap(),
                        expected.execute("cell.get", json!({"cell":cell})).unwrap()
                    );
                }
                let (actual, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
                let (expected, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &expected.save(None).unwrap()).unwrap();
                assert_eq!(actual.sheets[0].rows.get(&4), expected.sheets[0].rows.get(&4), "{command}: row geometry differs from a fresh write");
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn derived_fit_holds_wrapping_width_multiline_height_and_custom_row_height() {
    for change in ["wrapped text", "wrapped width", "multiline", "row height", "default font"] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"cell":"D5","input":"wrapped source words"})), ("format.wrapText", json!({"range":"D5","on":true}))]);
        let basis = f.basis().await;
        match change {
            "wrapped text" => f.colleague(&[("cell.set", json!({"cell":"D5","input":"a longer wrapped source with many more words"}))]),
            "wrapped width" => f.colleague(&[("column.setWidth", json!({"cols":"D","width":40}))]),
            "multiline" => f.colleague(&[("cell.set", json!({"cell":"B5","input":"one\ntwo"}))]),
            "row height" => f.colleague(&[("row.setHeight", json!({"rows":"5","height":60}))]),
            "default font" => f.colleague_rewrite(|wb| {
                let mut style = wb.styles.get(Default::default()).clone();
                style.font.size = 16.0;
                wb.styles.set_default(style);
            }),
            _ => panic!("unknown change"),
        }
        let before = f.bytes();
        conflict(f.run(basis, "format.bold", json!({"range":"C5","on":true})).await, &before, &f);
        f.shutdown().await;
    }
}
