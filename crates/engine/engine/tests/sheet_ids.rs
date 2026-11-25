//! A sheet's id is stable (task card C4/C5/C6-sheet, one id space): it is the workbook's own
//! number for the sheet (`sheetId` in an `.xlsx`), kept through a rename, a move, a save and a
//! reopen, so an agent that read it once can name the sheet by it whatever else changed. A new
//! sheet, and a copy, get a number no other sheet of the workbook has.

use kition_sheet_engine::Session;
use kition_sheet_engine::io::{open_bytes, save_bytes};
use kition_sheet_engine::model::{Sheet, Workbook};
use serde_json::{Value as Json, json};

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn ids(s: &Session) -> Vec<(String, u32)> {
    s.doc().unwrap().wb.sheets.iter().map(|sheet| (sheet.name.clone(), sheet.sheet_id)).collect()
}

fn three_sheets() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    run(&mut s, "sheet.insert", json!({ "name": "Data" }));
    run(&mut s, "sheet.insert", json!({ "name": "Notes" }));
    s
}

#[test]
fn every_sheet_has_a_distinct_id_from_1() {
    let s = three_sheets();
    let mut numbers: Vec<u32> = ids(&s).into_iter().map(|(_, id)| id).collect();
    assert!(numbers.iter().all(|id| *id >= 1), "{:?}", ids(&s));
    numbers.sort_unstable();
    numbers.dedup();
    assert_eq!(numbers.len(), 3, "{:?}", ids(&s));
}

#[test]
fn a_sheet_keeps_its_id_through_rename_move_save_and_reopen() {
    let mut s = three_sheets();
    let before = ids(&s);
    let data = before.iter().find(|(name, _)| name == "Data").map(|(_, id)| *id).unwrap();
    run(&mut s, "sheet.rename", json!({ "sheet": "Data", "name": "Figures" }));
    let index = s.doc().unwrap().wb.sheet_index("Figures").unwrap();
    run(&mut s, "sheet.move", json!({ "sheet": index, "to": 0 }));
    let wb = &s.doc().unwrap().wb;
    assert_eq!((wb.sheets[0].name.as_str(), wb.sheets[0].sheet_id), ("Figures", data));
    assert_eq!(wb.sheet_by_id(data), Some(0));
    let bytes = save_bytes(wb, "book.xlsx").unwrap();
    let (reopened, _) = open_bytes("book.xlsx", &bytes).unwrap();
    let after: Vec<(String, u32)> = reopened.sheets.iter().map(|sheet| (sheet.name.clone(), sheet.sheet_id)).collect();
    assert_eq!(after, ids(&s), "the saved file's sheets keep their ids");
}

#[test]
fn a_copy_gets_an_id_of_its_own_and_the_original_keeps_its_own() {
    let mut s = three_sheets();
    let before = ids(&s);
    let index = s.doc().unwrap().wb.sheet_index("Data").unwrap();
    run(&mut s, "sheet.move", json!({ "sheet": index, "to": 0, "copy": true }));
    let after = ids(&s);
    for (name, id) in &before {
        assert!(after.contains(&(name.clone(), *id)), "{name} keeps {id}: {after:?}");
    }
    let copy = after.iter().find(|(name, _)| !before.iter().any(|(b, _)| b == name)).unwrap();
    assert!(!before.iter().any(|(_, id)| *id == copy.1), "the copy {copy:?} has an id of its own: {after:?}");
}

#[test]
fn a_files_sheet_ids_are_kept() {
    let mut wb = Workbook::new();
    wb.sheets.push(std::sync::Arc::new(Sheet::new("Second")));
    wb.number_sheets();
    std::sync::Arc::make_mut(&mut wb.sheets[0]).sheet_id = 3;
    std::sync::Arc::make_mut(&mut wb.sheets[1]).sheet_id = 7;
    let bytes = save_bytes(&wb, "ids.xlsx").unwrap();
    let workbook_xml = {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut text = String::new();
        zip.by_name("xl/workbook.xml").unwrap().read_to_string(&mut text).unwrap();
        text
    };
    assert!(workbook_xml.contains("sheetId=\"3\"") && workbook_xml.contains("sheetId=\"7\""), "{workbook_xml}");
    let (reopened, _) = open_bytes("ids.xlsx", &bytes).unwrap();
    assert_eq!(reopened.sheets.iter().map(|sheet| sheet.sheet_id).collect::<Vec<_>>(), vec![3, 7]);
    // A new sheet in the reopened workbook takes the next number free.
    let mut s = Session::new();
    s.add_document(kition_sheet_engine::DocState::new(reopened, None, "ids".into()));
    run(&mut s, "sheet.insert", json!({ "name": "Third" }));
    let third = s.doc().unwrap().wb.sheets.iter().find(|sheet| sheet.name == "Third").map(|sheet| sheet.sheet_id).unwrap();
    assert_eq!(third, 8);
}

#[test]
fn numbering_gives_unnumbered_and_repeated_sheets_fresh_ids() {
    let mut wb = Workbook::new();
    wb.sheets.push(std::sync::Arc::new(Sheet::new("B")));
    wb.sheets.push(std::sync::Arc::new(Sheet::new("C")));
    std::sync::Arc::make_mut(&mut wb.sheets[0]).sheet_id = 4;
    std::sync::Arc::make_mut(&mut wb.sheets[1]).sheet_id = 4;
    wb.number_sheets();
    assert_eq!(wb.sheets.iter().map(|sheet| sheet.sheet_id).collect::<Vec<_>>(), vec![4, 5, 6]);
}
