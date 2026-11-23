//! The identity part (`xl/kition/identities.json`) keeps a sheet's object ids only while its
//! worksheet part is the one they were saved beside, and the defined names' ids only while the
//! workbook part is (schema freeze, 2026-11-22): another program that changed a sheet under the
//! ids (a row inserted moves every row after it) makes Kition drop them rather than name the
//! wrong rows, cells or names.

use std::io::{Read, Write};
use std::sync::Arc;

use kition_sheet_model::{DefinedName, Sheet, SheetIdentities, Workbook};

use crate::{read_xlsx, write_xlsx};

fn identities(object: &str) -> SheetIdentities {
    SheetIdentities {
        object: object.into(),
        axes: [vec![format!("{object}-r0"), format!("{object}-r1")], vec![format!("{object}-c0")]],
        tombstones: [std::collections::BTreeMap::from([(4, format!("{object}-gone"))]), Default::default()],
        ..Default::default()
    }
}

fn book() -> Workbook {
    let mut wb = Workbook::new();
    wb.sheets.push(Arc::new(Sheet::new("Second")));
    for (k, object) in ["sheet-a", "sheet-b"].into_iter().enumerate() {
        let sheet = wb.sheet_mut(k).unwrap();
        sheet.identities = identities(object);
        sheet.set_cell(kition_sheet_core::CellRef::new(0, 0), kition_sheet_model::Cell::value(kition_sheet_core::Value::Number(1.0)));
    }
    wb.number_sheets();
    wb.names = vec![DefinedName { name: "Total".into(), scope: None, formula: "Sheet1!$A$1".into(), comment: String::new(), hidden: false }];
    wb.identities.names = vec!["name-total".into()];
    wb
}

/// `bytes` with the part `name` passed through `change`, every other part as it was.
fn rewritten(bytes: &[u8], name: &str, change: impl Fn(String) -> String) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for k in 0..archive.len() {
        let mut file = archive.by_index(k).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        let part = file.name().to_owned();
        if part == name {
            data = change(String::from_utf8(data).unwrap()).into_bytes();
        }
        out.start_file(part, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn identities_survive_a_save_and_reopen() {
    let bytes = write_xlsx(&book()).unwrap();
    let (wb, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(wb.sheets[0].identities, identities("sheet-a"));
    assert_eq!(wb.sheets[1].identities, identities("sheet-b"));
    assert_eq!(wb.identities.names, vec!["name-total".to_owned()]);
}

#[test]
fn a_sheet_another_program_changed_loses_its_identities_and_the_others_keep_theirs() {
    let bytes = write_xlsx(&book()).unwrap();
    // Another program inserts a row above the first sheet's data: its rows are not the ones the
    // ids were saved for.
    let changed =
        rewritten(&bytes, "xl/worksheets/sheet1.xml", |xml| xml.replace("<c r=\"A1\"", "<c r=\"A2\"").replace("<row r=\"1\"", "<row r=\"2\""));
    assert_ne!(changed, bytes);
    let (wb, _) = read_xlsx(&changed).unwrap();
    assert_eq!(wb.sheets[0].identities, SheetIdentities::default(), "the changed sheet's ids are dropped");
    assert_eq!(wb.sheets[1].identities, identities("sheet-b"), "the unchanged sheet keeps its ids");
    assert_eq!(wb.identities.names, vec!["name-total".to_owned()], "the workbook part is unchanged");
}

#[test]
fn names_another_program_changed_lose_their_identities() {
    let bytes = write_xlsx(&book()).unwrap();
    let changed = rewritten(&bytes, "xl/workbook.xml", |xml| {
        xml.replace("<definedName name=\"Total\"", "<definedName name=\"Added\">Sheet1!$A$1</definedName><definedName name=\"Total\"")
    });
    let (wb, _) = read_xlsx(&changed).unwrap();
    assert!(wb.identities.names.is_empty(), "a name added before the others: their ids are dropped");
    assert_eq!(wb.sheets[0].identities, identities("sheet-a"));
}
