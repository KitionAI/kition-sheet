//! File differences import atomically into the retained model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[test]
fn premise_fork_retains_the_format_painter_snapshot() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B2","input":"destination"})).unwrap();
    doc.execute("format.bold", json!({"range":"A1","on":true})).unwrap();
    doc.execute("select.set", json!({"range":"A1"})).unwrap();
    doc.execute("edit.formatPainter", json!({"sticky":true})).unwrap();
    doc.version().unwrap();
    let mut fork = doc.fork_for_premises().unwrap();
    for target in [&mut doc, &mut fork] {
        target.execute("edit.formatPainter", json!({"apply":"B2"})).unwrap();
        let (wb, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &target.save(None).unwrap()).unwrap();
        let at = kition_sheet_engine::core::CellRef::new(1, 1);
        assert!(wb.styles.get(wb.sheets[0].style_id(at)).font.bold, "the retained painter source must format B2");
    }
    assert_eq!(doc.save(Some("Book.json")).unwrap(), fork.save(Some("Book.json")).unwrap());
}

#[test]
fn axis_content_regions_compare_stored_and_derived_fields() {
    for (command, params) in [("row.setHeight", json!({"rows":"3","height":40})), ("row.autofitHeight", json!({"rows":"3"}))] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("cell.set", json!({"cell":"B3","input":"before"})).unwrap();
        doc.version().unwrap();
        let held = doc.under_step(command, &params, &Default::default()).unwrap().unwrap();
        let content = held.content.iter().find(|content| content.region["axis"].as_str().is_some_and(|id| id.starts_with("row:"))).unwrap();
        let before = doc.derived(std::slice::from_ref(&content.id), &["contents".into()]).unwrap();
        let before = before.iter().find(|value| value.id == content.id).unwrap().values["contents"].clone();
        let complete = doc.derived(&[], &[]).unwrap();
        assert_eq!(
            before,
            complete.iter().find(|value| value.id == content.id).unwrap().values["contents"],
            "targeted and complete axis facts agree"
        );
        if command == "row.setHeight" {
            doc.execute("row.setHeight", json!({"rows":"3","height":60})).unwrap();
        } else {
            doc.execute("cell.set", json!({"cell":"B3","input":"after\nnew line"})).unwrap();
        }
        assert_eq!(
            doc.content_at(&kition_kit::ContentAsk { premise: content.clone(), before, step: Default::default() }, &NoBlobs).unwrap(),
            Some(true),
            "{command}: {content:?}"
        );
    }
}

struct NoBlobs;
impl kition_kit::HistoricalBlobs for NoBlobs {
    fn read(&self, _hash: &str, _offset: usize, _length: usize) -> Result<Vec<u8>, String> {
        Err("axis facts do not read blobs".into())
    }
}

#[test]
fn historical_logical_coverage_does_not_turn_padding_into_a_relocation() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B3","input":"7"})).unwrap();
    doc.version().unwrap();
    let all = doc.derived(&[], &[]).unwrap();
    let unique: std::collections::BTreeSet<_> = all.iter().map(|value| value.id.to_string()).collect();
    assert_eq!(unique.len(), all.len(), "each object's derived facts must occupy one record");
    assert!(all.iter().find(|value| value.id == "sheet:1").unwrap().values.contains_key("axisOrder"));
    let values = doc.derived(&[json!("sheet:1")], &["axisOrder".into()]).unwrap();
    let before = values.iter().find(|value| value.id == "sheet:1").unwrap().values["axisOrder"].clone();
    assert_eq!(before, json!({"rows":{},"cols":{}}), "padding is the implicit identity order");
    let mut fresh = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    fresh.execute("cell.set", json!({"cell":"Z20","input":"outside"})).unwrap();
    let fresh = SheetDocument::open("Book.xlsx", &fresh.save(None).unwrap()).unwrap();
    doc.reconcile_reread(&fresh).unwrap();
    let params = json!({"cell":"H5","input":"agent"});
    let now = doc.under_step("cell.set", &params, &Default::default()).unwrap().unwrap();
    let covered = doc
        .covered(
            "cell.set",
            &params,
            &[kition_kit::CoveredAsk {
                id: json!("sheet:1"),
                before: serde_json::Map::from_iter([("~axisOrder".into(), before)]),
                ..Default::default()
            }],
        )
        .unwrap()
        .unwrap();
    assert_eq!(covered.under, now.objects);
}

#[test]
fn reread_imports_new_referenced_objects() {
    for (command, params) in [
        ("table.insert", json!({"range":"Z20:AA22","name":"ReceiptTable","header":true})),
        ("format.bold", json!({"range":"B7","on":true})),
        ("name.define", json!({"name":"Rate","refersTo":"0.2"})),
    ] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
        doc.version().unwrap();
        let mut fresh = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        fresh.execute(command, params).unwrap();
        let fresh = SheetDocument::open("Book.xlsx", &fresh.save(None).unwrap()).unwrap();
        assert!(doc.reconcile_reread(&fresh).unwrap_or_else(|error| panic!("{command}: {error}")));
        let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        assert_eq!(saved.sheets[0].value(kition_sheet_engine::core::CellRef::new(6, 1)), kition_sheet_engine::core::Value::Number(7.0));
        match command {
            "table.insert" => assert_eq!(saved.sheets[0].tables[0].name, "ReceiptTable"),
            "format.bold" => assert!(saved.styles.get(saved.sheets[0].style_id(kition_sheet_engine::core::CellRef::new(6, 1))).font.bold),
            "name.define" => assert!(saved.names.iter().any(|name| name.name == "Rate")),
            _ => panic!("unreviewed fixture"),
        }
    }
}

#[test]
fn reread_refuses_a_deleted_active_sheet_without_mutating_the_retained_document() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("sheet.insert", json!({"name":"Active"})).unwrap();
    doc.version().unwrap();
    let mut fresh = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    fresh.execute("sheet.delete", json!({"sheet":"Active"})).unwrap();
    let fresh = SheetDocument::open("Book.xlsx", &fresh.save(None).unwrap()).unwrap();
    let before = doc.save(Some("Book.json")).unwrap();
    assert_eq!(doc.reconcile_reread(&fresh).unwrap_err(), "deleted active sheet");
    assert_eq!(doc.save(Some("Book.json")).unwrap(), before);
}
