//! C8-sheet-read-2: source checks independent of the reviewed declaration tables.
use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn copied_window_keeps_the_source_active_sheet() {
    for name in ["Sheet1", "Sheet2"] {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("sheet.insert", json!({"name":"Sheet2"})).unwrap();
        doc.execute("sheet.activate", json!({"sheet":name})).unwrap();
        doc.execute("view.newWindow", json!({})).unwrap();
        assert_eq!(doc.inspect().unwrap()["sheet"], name);
        // extra.rs::new_window copies wb.active_sheet into the new DocState.
        let command = doc.commands().into_iter().find(|command| command.id == "view.newWindow").unwrap();
        let states: BTreeSet<_> = command.reads.iter().map(|state| state.as_str()).collect();
        assert_eq!(states, BTreeSet::from(["part", "selection"]));
    }
}

#[test]
fn spelling_selection_does_not_choose_any_replacement() {
    for all in [false, true] {
        let mut saved = Vec::new();
        for selected in ["A1", "Z20"] {
            let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
            doc.execute("cell.set", json!({"cell":"C3","input":"teh"})).unwrap();
            doc.execute("select.set", json!({"range":selected})).unwrap();
            doc.execute("spelling.change", json!({"cell":"C3","word":"teh","to":"the","all":all})).unwrap();
            assert_eq!(doc.execute("cell.get", json!({"cell":"C3"})).unwrap()["value"], "the");
            let mut workbook: serde_json::Value = serde_json::from_slice(&doc.save(Some("Book.json")).unwrap()).unwrap();
            workbook.as_object_mut().unwrap().remove("identities");
            for sheet in workbook["sheets"].as_array_mut().unwrap() {
                sheet.as_object_mut().unwrap().remove("identities");
            }
            saved.push(workbook);
            // spelling.rs::change fixes all=true in its edit.replace delegation;
            // its other branch supplies cell.set with an explicit cell.
            let command = doc.commands().into_iter().find(|command| command.id == "spelling.change").unwrap();
            let states: BTreeSet<_> = command.reads.iter().map(|state| state.as_str()).collect();
            assert_eq!(states, BTreeSet::from(["part"]));
        }
        assert_eq!(saved[0], saved[1]);
    }
}

#[test]
fn legacy_object_operations_do_not_resolve_group_keys() {
    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    for id in ["object.delete", "object.move", "object.setAnchorMode"] {
        // insert.rs::{delete_object,move_object,set_anchor_mode} search concrete
        // image/chart/shape lists, without drawing.rs's group expansion.
        let command = doc.commands().into_iter().find(|command| command.id == id).unwrap();
        let types: BTreeSet<_> = command.object_types.iter().find(|(parameter, _)| *parameter == "id").unwrap().1.iter().copied().collect();
        assert_eq!(types, BTreeSet::from(["Shape", "Chart", "Image"]), "{id}");
    }
}
