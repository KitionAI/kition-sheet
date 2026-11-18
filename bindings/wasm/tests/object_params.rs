//! C8-land-13-sheet: declarations follow the engine resolvers, including untyped schemas.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::{KitCommand, KitDocument};
use kition_sheet_kit::SheetDocument;
use serde_json::json;

fn commands() -> Vec<KitCommand> {
    SheetDocument::create("Book.xlsx", 0, 0).unwrap().commands()
}

#[test]
fn integer_ids_are_declared_to_the_sdk() {
    let commands = commands();
    kition_kit::testing::check_id_params(&commands, &[]).unwrap();
    let marked: usize = commands
        .iter()
        .map(|command| command.objects.len() + usize::from(command.targets.is_some_and(|target| matches!(target, "id" | "ids" | "chart"))))
        .sum();
    println!("object parameters marked: {marked}; SDK scanner allowed: 0");
    let mut damaged = commands.clone();
    let connect = damaged.iter_mut().find(|c| c.id == "drawing.connect").unwrap();
    connect.objects = &[];
    assert!(kition_kit::testing::check_id_params(&damaged, &[]).is_err(), "the scanner must catch a removed endpoint declaration");
}

#[test]
fn object_declarations_do_not_overwrite_target_roles() {
    for command in commands() {
        if let Some(target) = command.targets {
            assert!(!command.objects.contains(&target), "{}.{} loses its targets role", command.id, target);
        }
    }
}

#[test]
fn drawing_resolvers_have_typed_declarations_even_when_the_schema_is_untyped() {
    for command in commands() {
        let schema = command.schema.as_ref().unwrap();
        for key in ["id", "ids", "chart", "from", "to"] {
            let drawing = (command.id.starts_with("drawing.") && key != "chart")
                || ((command.id.starts_with("chart.")
                    || command.id.starts_with("arrange.")
                    || command.id.starts_with("object.")
                    || command.id == "shape.setText")
                    && matches!(key, "id" | "ids" | "chart"));
            if !drawing || schema.get(key).is_none() {
                continue;
            }
            assert!(command.targets == Some(key) || command.objects.contains(&key), "{}.{key} is unmarked", command.id);
            let types =
                command.object_types.iter().find(|(parameter, _)| *parameter == key).unwrap_or_else(|| panic!("{}.{key} is untyped", command.id)).1;
            assert!(!types.is_empty(), "{}.{key}", command.id);
        }
    }
}

/// Names embedded in JSON are places resolved by `under`, rather than fictitious KDM ids.
#[test]
fn name_resolvers_are_declared() {
    let mut marked = 0;
    let mut places = 0;
    for command in commands() {
        let schema = command.schema.as_ref().unwrap();
        for key in ["sheet", "table", "pivot", "scope"] {
            if schema.get(key).is_some() {
                if key == "scope" && matches!(command.id.as_str(), "edit.undo" | "edit.redo") {
                    assert!(!command.places.contains(&key), "{}.scope selects the history owner", command.id);
                    assert!(!command.object_types.iter().any(|(parameter, _)| *parameter == key));
                    assert!(!command.name_params.iter().any(|(parameter, _)| *parameter == key));
                    continue;
                }
                // Reviewed handlers use these sheet parameters as a new name or a bool.
                if key == "sheet" && matches!(command.id.as_str(), "data.fromTextCsv" | "data.getData" | "home.clearRules") {
                    assert!(!command.places.contains(&key), "{}.sheet is literal", command.id);
                    assert!(
                        !command.name_params.iter().any(|(parameter, _)| *parameter == key),
                        "{}.sheet must not resolve an existing name",
                        command.id
                    );
                    assert!(!command.object_types.iter().any(|(parameter, _)| *parameter == key), "{}.sheet must not resolve a Sheet id", command.id);
                    continue;
                }
                if command.name_params.iter().any(|(parameter, _)| *parameter == key) {
                    marked += 1;
                } else {
                    places += 1;
                }
                assert!(
                    command.places.contains(&key) || command.name_params.iter().any(|(parameter, _)| *parameter == key),
                    "{}.{key} names an existing noun without a declaration",
                    command.id
                );
            }
        }
    }
    println!("name resolver parameters: typed {marked}; declared places {}; scanner allowed 0", places + 4);
    for (id, key) in [("formulas.deleteName", "name"), ("home.cellStyle", "name"), ("table.deleteStyle", "name"), ("table.setDefaultStyle", "name")] {
        let command = commands().into_iter().find(|command| command.id == id).unwrap();
        assert!(command.places.contains(&key), "{id}.{key}");
    }
}

#[test]
fn literal_text_that_spells_a_tool_id_stays_literal() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    for text in ["drawing:999", "sheet:999"] {
        doc.execute("cell.set", json!({"cell":"A1", "input":text})).unwrap();
        let cell = doc.execute("cell.get", json!({"cell":"A1"})).unwrap();
        assert_eq!(cell["value"], text, "{cell}");
    }
}

#[test]
fn numeric_only_handlers_accept_drawing_tool_ids() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("insert.textBox", json!({"text":"before"})).unwrap();
    let id = doc.inspect().unwrap()["workbook"]["sheets"][0]["drawings"][0]["toolId"].clone();
    doc.execute("shape.setText", json!({"id":id,"text":"after"})).unwrap();
    doc.execute("range.setValues", json!({"range":"A1", "values":[["Name","Value"],["A",1]]})).unwrap();
    doc.execute("insert.chart", json!({"range":"A1:B2"})).unwrap();
    let drawings = doc.inspect().unwrap()["workbook"]["sheets"][0]["drawings"].as_array().unwrap().clone();
    let chart = drawings.last().unwrap()["toolId"].clone();
    doc.execute("chart.selectData", json!({"chart":chart,"range":"A1:B2"})).unwrap();
}
