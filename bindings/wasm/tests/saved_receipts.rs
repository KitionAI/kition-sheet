//! C8-land-13-sheet: every receipt describes the model that the saved XLSX reopens as.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::{
    KitDocument,
    kdm::{ActorId, Replica, reread::diff_read_again},
    session::model::{Exchange, ReplicaPort},
};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

fn state(doc: &mut SheetDocument) -> Value {
    let bytes = doc.model_export(&[]).unwrap();
    let raw = Replica::load(doc.model_schema(), &bytes, ActorId::random()).unwrap().state_json().unwrap();
    let loads: Vec<_> = raw["objects"].as_object().unwrap().keys().map(|id| json!({"target":id})).collect();
    let loaded = Exchange::new().batch(doc, &json!({"load":loads}), true).unwrap();
    let objects: serde_json::Map<_, _> = loaded["loaded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|object| {
            let mut fields = object["fields"].as_object().unwrap().clone();
            fields.insert("$type".into(), object["type"].clone());
            (object["id"].as_str().unwrap().to_owned(), Value::Object(fields))
        })
        .collect();
    json!({"objects":objects})
}

#[test]
fn in_memory_equals_saved_on_the_conformance_file_and_drawing_fixture() {
    let mut problems = Vec::new();
    let basic = include_bytes!("../../../conformance/basic.xlsx");
    for (command, params) in [
        ("cell.set", json!({"cell":"B7","input":"42"})),
        ("home.bold", json!({"range":"B7","on":true})),
        ("sheet.rename", json!({"name":"Renamed"})),
        ("home.insertRows", json!({"rows":"2:2"})),
        ("drawing.insert", json!({"kind":"textBox","text":"A note","frame":{"x":12.123456,"y":24.654321,"w":50.123456,"h":30.654321}})),
    ] {
        let mut doc = SheetDocument::open("Book.xlsx", basic).unwrap();
        doc.execute(command, params).unwrap();
        let memory = state(&mut doc);
        let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let saved = state(&mut reopened);
        let difference = diff_read_again(&doc.model_schema(), &memory, &saved).to_json();
        if difference != json!({"created":[],"changed":[],"moved":[],"deleted":[]}) {
            problems.push(format!("{command}: {difference}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_conformance_workbook_keeps_write_family_receipts_on_save() {
    let mut problems = Vec::new();
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "xlsx"))
        .collect();
    files.sort();
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        for (command, params) in [
            ("cell.set", json!({"cell":"B7","input":"42"})),
            ("cell.set", json!({"cell":"B7","input":"=SUM(B2:B6)"})),
            ("range.setValues", json!({"range":"X1:Y3","values":[["Item","Amount"],["First",12],["Second",24]]})),
            ("home.bold", json!({"range":"B7","on":true})),
            ("home.numberFormat", json!({"range":"B7","format":"0.000"})),
            ("cell.formatText", json!({"cell":"X2","start":0,"end":3,"bold":true})),
            ("edit.fillDown", json!({"range":"X2:Y4"})),
            ("edit.moveCells", json!({"from":"X2:Y3","to":"X5"})),
            ("edit.replace", json!({"what":"First","with":"Updated","all":true})),
            ("edit.paste", json!({"at":"X5","text":"Label\tValue\nNew\t30"})),
            ("data.sort", json!({"range":"X1:Y3","header":true,"keys":[{"column":"Y","order":"desc"}]})),
            ("data.filter", json!({"range":"X1:Y3"})),
            ("data.removeDuplicates", json!({"range":"X1:Y3","header":true})),
            ("data.textToColumns", json!({"range":"X2:X3","delimiters":["i"],"destination":"X5"})),
            ("data.subtotal", json!({"range":"X1:Y3","groupBy":"X","columns":["Y"]})),
            ("data.consolidate", json!({"sources":["X1:Y3"],"destination":"X10","topRow":true,"leftColumn":true})),
            ("data.scenarioAdd", json!({"name":"Receipt scenario","changing":"Y2:Y3","values":[25,30]})),
            ("data.getData", json!({"text":"Item,Amount\nFirst,12\nSecond,24"})),
            ("file.properties", json!({"title":"Receipt title","author":"Kition"})),
            ("formulas.calculationOptions", json!({"mode":"manual","iterative":true,"maxIterations":200,"maxChange":0.01})),
            ("home.newCellStyle", json!({"name":"ReceiptStyle","fromCell":"X2"})),
            ("table.newStyle", json!({"name":"ReceiptTableStyle","elements":{"headerRow":{"bold":true}}})),
            ("sheet.rename", json!({"name":"Renamed"})),
            ("home.insertSheet", json!({"name":"Added"})),
            ("home.insertRows", json!({"rows":"2:2"})),
            ("home.insertColumns", json!({"cols":"B:B"})),
            ("home.deleteRows", json!({"rows":"2:2"})),
            ("home.deleteColumns", json!({"cols":"B:B"})),
            ("home.insertCells", json!({"range":"X2:Y2","shift":"down"})),
            ("home.deleteCells", json!({"range":"X2:Y2","shift":"up"})),
            ("range.fill", json!({"range":"X5:Y6","input":"=B2+1"})),
            ("edit.clearContents", json!({"range":"X1:Y3"})),
            ("home.rowHeight", json!({"rows":"7:7","height":19.123456})),
            ("home.columnWidth", json!({"cols":"B:B","width":14.123456})),
            ("home.autofitRowHeight", json!({"rows":"2:3"})),
            ("home.autofitColumnWidth", json!({"cols":"X:Y"})),
            ("home.hideRows", json!({"rows":"2:3"})),
            ("home.hideColumns", json!({"cols":"X:Y"})),
            ("home.mergeCells", json!({"range":"X10:Y11"})),
            ("home.conditionalFormat", json!({"range":"Y2:Y3","rule":{"type":"cellIs","operator":"greaterThan","value":10,"preset":"greenFill"}})),
            ("formulas.defineName", json!({"name":"ReceiptName","refersTo":"=B7"})),
            ("data.validation", json!({"range":"X5:Y6","type":"whole","formula1":"1","formula2":"10"})),
            ("data.group", json!({"rows":"5:6"})),
            ("review.protectSheet", json!({})),
            ("review.protectWorkbook", json!({"on":true})),
            ("review.newNote", json!({"cell":"X5","text":"Receipt note","author":"Kition"})),
            ("review.newComment", json!({"cell":"X5","text":"Receipt comment","author":"Kition"})),
            ("insert.link", json!({"cell":"X5","target":"https://kition.ai","text":"Kition"})),
            ("review.changeSpelling", json!({"cell":"X2","word":"First","to":"Updated"})),
            ("pageLayout.pageSetup", json!({"printArea":"X1:Y5","gridlines":true,"titleRows":"1:1","header":"&LReceipt"})),
            ("pageLayout.themeFonts", json!({"major":"Arial","minor":"Arial"})),
            ("view.gridlines", json!({"on":false})),
            ("view.customViews", json!({"save":"Receipt view"})),
            ("view.split", json!({})),
            ("formulas.watchWindow", json!({"add":"Data!B7"})),
            ("automate.newScript", json!({"name":"Receipt script","commands":[{"command":"cell.set","params":{"cell":"X20","input":"7"}}]})),
            ("automate.runScript", json!({"name":"Receipt script"})),
            ("automate.deleteScript", json!({"name":"Receipt script"})),
            ("automate.recordActions", json!({"stop":true,"name":"Receipt recording"})),
            ("insert.sparkline", json!({"range":"Y2:Y3","location":"Z2","type":"line","markers":true})),
            ("insert.checkbox", json!({"range":"Y2:Y3"})),
            ("insert.table", json!({"range":"X1:Y3","header":true,"name":"ReceiptTable"})),
            ("insert.pivotTable", json!({"source":"X1:Y3","destination":"X10","rows":["Item"],"values":["Amount"]})),
            ("insert.chart", json!({"range":"X1:Y3","type":"column"})),
            ("drawing.insert", json!({"kind":"textBox","text":"A note","frame":{"x":12.123456,"y":24.654321,"w":50.123456,"h":30.654321}})),
        ] {
            let mut doc = SheetDocument::open("Book.xlsx", &bytes).unwrap();
            doc.execute("range.setValues", json!({"range":"X1:Y3","values":[["Item","Amount"],["First",12],["Second",24]]})).unwrap();
            if matches!(command, "automate.runScript" | "automate.deleteScript") {
                doc.execute(
                    "automate.newScript",
                    json!({"name":"Receipt script","commands":[{"command":"cell.set","params":{"cell":"X20","input":"7"}}]}),
                )
                .unwrap();
            }
            if command == "automate.recordActions" {
                doc.execute(command, json!({})).unwrap();
                doc.execute("cell.set", json!({"cell":"X20","input":"7"})).unwrap();
            }
            if let Err(error) = doc.execute(command, params) {
                problems.push(format!("{} {command}: {error}", path.display()));
                continue;
            }
            doc.model_export(&[]).unwrap_or_else(|error| panic!("{} {command}: {error}", path.display()));
            let memory = state(&mut doc);
            let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
            let difference = diff_read_again(&doc.model_schema(), &memory, &state(&mut reopened)).to_json();
            if difference != json!({"created":[],"changed":[],"moved":[],"deleted":[]}) {
                problems.push(format!("{} {command}: {difference}", path.display()));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn persisted_auxiliary_commands_declare_writes_and_survive_save() {
    for (command, params) in [
        ("view.customViews", json!({"save":"Receipt view"})),
        ("view.split", json!({})),
        ("formulas.watchWindow", json!({"add":"Sheet1!B7"})),
        ("automate.recordActions", json!({"stop":true,"name":"Receipt recording"})),
    ] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        assert!(doc.commands().iter().find(|spec| spec.id == command).unwrap().mutates, "{command} writes saved workbook data");
        doc.execute("selection.set", json!({"range":"B7"})).unwrap();
        if command == "automate.recordActions" {
            doc.execute(command, json!({})).unwrap();
            doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
        }
        let before = state(&mut doc);
        doc.execute(command, params).unwrap();
        let memory = state(&mut doc);
        assert_ne!(memory, before, "{command} must report its saved change");
        let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let difference = diff_read_again(&doc.model_schema(), &memory, &state(&mut reopened)).to_json();
        assert_eq!(difference, json!({"created":[],"changed":[],"moved":[],"deleted":[]}), "{command}: {difference}");
    }
}

#[test]
fn direct_cell_inputs_are_canonical_before_the_receipt() {
    for input in ["42", "=SUM(B2:B9)", "Receipt text"] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
        Exchange::new().batch(&mut doc, &json!({"ops":[{"op":"set","target":"cell:1!B7","field":"input","value":input}]}), false).unwrap();
        let memory = state(&mut doc);
        let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let difference = diff_read_again(&doc.model_schema(), &memory, &state(&mut reopened)).to_json();
        assert_eq!(difference, json!({"created":[],"changed":[],"moved":[],"deleted":[]}), "{input}: {difference}");
    }
}

#[test]
fn defined_name_identity_survives_rename_and_reopen() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("formulas.defineName", json!({"name":"ReceiptRate","refersTo":"=0.2"})).unwrap();
    let memory = state(&mut doc);
    let id = memory["objects"].as_object().unwrap().iter().find(|(_, value)| value["$type"] == "Name").unwrap().0.to_owned();
    assert!(id.starts_with("name:"), "a defined name needs a stable tool id: {id}");
    Exchange::new().batch(&mut doc, &json!({"ops":[{"op":"set","target":id,"field":"name","value":"ReceiptTax"}]}), false).unwrap();
    let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    let saved = state(&mut reopened);
    assert_eq!(saved["objects"][&id]["name"], "ReceiptTax");
}

#[test]
fn direct_axis_and_sheet_writes_are_canonical_before_the_receipt() {
    let mut problems = Vec::new();
    for op in [
        json!({"op":"list.move","target":"sheet:1","field":"rows","from":3,"to":2}),
        json!({"op":"create","type":"Row","in":"sheet:1.rows","at":1,"fields":{}}),
        json!({"op":"create","type":"Col","in":"sheet:1.cols","at":1,"fields":{}}),
        json!({"op":"create","type":"Sheet","in":"root.sheets","at":1,"fields":{"name":"Added"}}),
        json!({"op":"set","target":"$row1","field":"gone","value":true}),
        json!({"op":"delete","target":"$col1"}),
        json!({"op":"set","target":"sheet:1","field":"name","value":"Notes"}),
    ] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("range.setValues", json!({"range":"A1:B4","values":[["Item","Value"],["First",1],["Second",2],["Third",3]]})).unwrap();
        doc.execute("home.insertSheet", json!({"name":"Notes","before":9})).unwrap();
        doc.execute("sheet.activate", json!({"sheet":0})).unwrap();
        let mut op = op;
        for (token, ty) in [("$row1", "Row"), ("$col1", "Col")] {
            if op["target"] == token {
                let before = state(&mut doc);
                let id =
                    before["objects"].as_object().unwrap().iter().find(|(_, value)| value["$type"] == ty && value["index"] == 1).unwrap().0.clone();
                op["target"] = json!(id);
            }
        }
        Exchange::new().batch(&mut doc, &json!({"ops":[op.clone()]}), false).unwrap();
        let memory = state(&mut doc);
        let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let difference = diff_read_again(&doc.model_schema(), &memory, &state(&mut reopened)).to_json();
        if difference != json!({"created":[],"changed":[],"moved":[],"deleted":[]}) {
            problems.push(format!("{op}: {difference}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn an_image_without_content_is_refused_before_a_receipt() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let before = state(&mut doc);
    let result = Exchange::new().batch(&mut doc, &json!({"ops":[{"op":"create","type":"Image","in":"sheet:1.images","at":0,"fields":{}}]}), false);
    assert!(result.is_err(), "an image without bytes cannot survive save: {result:?}");
    assert_eq!(state(&mut doc), before);
}

#[test]
fn direct_name_creation_and_last_drawing_axis_deletion_survive_save() {
    let mut problems = Vec::new();
    for kind in ["name", "Row", "Col"] {
        let mut doc = fixture(1);
        let memory = state(&mut doc);
        let op = if kind == "name" {
            json!({"op":"create","type":"Name","in":"root.names","at":0,"fields":{"name":"Rate","formula":"0.2"}})
        } else {
            let id = memory["objects"].as_object().unwrap().iter().find(|(_, value)| value["$type"] == kind).unwrap().0.clone();
            json!({"op":"delete","target":id})
        };
        Exchange::new().batch(&mut doc, &json!({"ops":[op]}), false).unwrap();
        let memory = state(&mut doc);
        let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let difference = diff_read_again(&doc.model_schema(), &memory, &state(&mut reopened)).to_json();
        if difference != json!({"created":[],"changed":[],"moved":[],"deleted":[]}) {
            problems.push(format!("{kind}: {difference}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn direct_fractional_dimensions_are_quantized_before_the_receipt() {
    let mut problems = Vec::new();
    for (target, field, value) in [
        ("drawing:1", "frame.anchor.dx", json!(12.123456789)),
        ("drawing:1", "frame.anchor.dy", json!(7.987654321)),
        ("drawing:1", "frame.anchor.width", json!(54.123456789)),
        ("sheet:1", "props.defaultColWidth", json!(54.123456789)),
        ("sheet:1", "props.defaultRowHeight", json!(19.123456789)),
        ("$row", "info", json!({"size":19.123456789,"hidden":false,"custom":true,"outline":0,"collapsed":false,"style":null})),
        ("$col", "info", json!({"size":54.123456789,"hidden":false,"custom":false,"outline":0,"collapsed":false,"style":null})),
    ] {
        let mut doc = fixture(1);
        let memory = state(&mut doc);
        let target = if let Some(axis) = target.strip_prefix('$') {
            let ty = if axis == "row" { "Row" } else { "Col" };
            memory["objects"].as_object().unwrap().iter().find(|(_, value)| value["$type"] == ty).unwrap().0.clone()
        } else {
            target.to_owned()
        };
        Exchange::new().batch(&mut doc, &json!({"ops":[{"op":"set","target":target,"field":field,"value":value}]}), false).unwrap();
        let memory = state(&mut doc);
        let mut reopened = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let difference = diff_read_again(&doc.model_schema(), &memory, &state(&mut reopened)).to_json();
        if difference != json!({"created":[],"changed":[],"moved":[],"deleted":[]}) {
            problems.push(format!("{field}: {difference}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn fixture(count: usize) -> SheetDocument {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    for n in 0..count {
        doc.execute("drawing.insert", json!({"kind":"shape","frame":{"x":n * 60,"y":10,"w":50,"h":30}})).unwrap();
    }
    // Start from precisely what XLSX stores.
    SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap()
}

fn receipt(doc: &mut SheetDocument, command: &str, params: Value) -> Value {
    let before = state(doc);
    doc.execute(command, params).unwrap();
    let after = state(doc);
    let receipt = diff_read_again(&doc.model_schema(), &before, &after).to_json();
    let mut saved = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
    let difference = diff_read_again(&doc.model_schema(), &after, &state(&mut saved)).to_json();
    assert_eq!(difference, json!({"created":[],"changed":[],"moved":[],"deleted":[]}), "{command}: {difference}");
    receipt
}

#[test]
fn sparse_metadata_clear_receipts_match_saved_reopen() {
    for command in ["edit.clearAll", "edit.clearFormats", "edit.clearComments", "edit.clearHyperlinks"] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("cell.set", json!({"cell":"A1","input":"kept"})).unwrap();
        doc.execute("cell.set", json!({"cell":"B7","input":"linked"})).unwrap();
        doc.execute("insert.link", json!({"cell":"B7","target":"https://example.org"})).unwrap();
        doc.execute("review.newNote", json!({"cell":"Z20","text":"sparse"})).unwrap();
        doc.execute("home.mergeAcross", json!({"range":"Z20:AA21"})).unwrap();
        let mut doc = SheetDocument::open("Book.xlsx", &doc.save(None).unwrap()).unwrap();
        let changes = receipt(&mut doc, command, json!({"range":"B7,Z20:AA21"}));
        assert!(!changes["changed"].as_array().unwrap().is_empty() || !changes["deleted"].as_array().unwrap().is_empty(), "{command}: {changes}");
        assert_eq!(doc.execute("cell.get", json!({"cell":"A1"})).unwrap()["value"], "kept");
    }
}

#[test]
fn receipt_bytes_for_properties_moves_groups_and_many_objects() {
    measurements(receipt);
}

#[test]
fn receipt_bytes_even_when_the_saved_invariant_reports_a_defect() {
    measurements(|doc, command, params| {
        let before = state(doc);
        doc.execute(command, params).unwrap();
        diff_read_again(&doc.model_schema(), &before, &state(doc)).to_json()
    });
}

fn measurements(receipt: fn(&mut SheetDocument, &str, Value) -> Value) {
    let mut doc = fixture(1);
    for (label, command, params) in
        [("one property", "drawing.format", json!({"ids":[1],"fill":"#C00000"})), ("one move", "drawing.transform", json!({"ids":[1],"dx":1.25}))]
    {
        let changes = receipt(&mut doc, command, params);
        println!("receipt bytes {label}: {}", changes.to_string().len());
    }
    for count in [5, 200] {
        let mut doc = fixture(count);
        let ids: Vec<_> = (1..=count).collect();
        doc.execute("drawing.group", json!({"ids":ids})).unwrap();
        let group =
            doc.inspect().unwrap()["workbook"]["sheets"][0]["drawings"].as_array().unwrap().iter().find(|o| o["kind"] == "group").unwrap()["toolId"]
                .clone();
        let changes = receipt(&mut doc, "drawing.transform", json!({"ids":[group],"dx":1.25}));
        println!("receipt bytes parent with {count} children: {}", changes.to_string().len());
    }
    let mut doc = fixture(200);
    let ids: Vec<_> = (1..=200).collect();
    let changes = receipt(&mut doc, "drawing.transform", json!({"ids":ids,"dx":1.25}));
    println!("receipt bytes same offset on 200: {}", changes.to_string().len());
}

#[test]
fn owning_map_queries_name_their_references_by_tool_id() {
    let mut before = fixture(1);
    let answer = Exchange::new().batch(&mut before, &json!({"load":[{"target":"sheet:1","fields":["shapes"]}]}), true).unwrap();
    let shapes = answer["loaded"][0]["fields"]["shapes"].as_object().unwrap();
    assert_eq!(shapes.len(), 1);
    assert!(shapes.values().all(|value| value == "drawing:1"), "{answer}");
    assert!(state(&mut before)["objects"].get("drawing:1").is_some());
}

#[test]
fn saved_identity_references_do_not_churn_when_cells_are_omitted() {
    let mut before = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    before.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
    let bytes = before.save(None).unwrap();
    let mut a = state(&mut before);
    let mut after = SheetDocument::open("Book.xlsx", &bytes).unwrap();
    let mut b = state(&mut after);
    // kition-env::snapshot::named_ids follows list/tree/ref, but not owning maps.
    // Removing only their omitted Cell objects reproduces its otherwise unchanged reads.
    for state in [&mut a, &mut b] {
        state["objects"].as_object_mut().unwrap().retain(|_, object| object["$type"] != "Cell");
    }
    let difference = diff_read_again(&before.model_schema(), &a, &b).to_json();
    assert_eq!(difference, json!({"created":[],"changed":[],"moved":[],"deleted":[]}));
}
