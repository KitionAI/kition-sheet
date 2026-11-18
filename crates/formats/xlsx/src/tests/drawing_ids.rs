//! Drawing objects keep the file's ids (`cNvPr id`, task card C4/C5/C6-sheet): an agent that read
//! a shape's or a chart's id once names it by the same id after a save and a reopen. Ids repeat
//! across a file's drawing parts (each sheet's drawing counts its own); in the workbook they are
//! unique, so a repeated one gets a new id, which the next save writes and keeps.

use std::sync::Arc;

use kition_sheet_core::CellRef;
use kition_sheet_model::{Shape, ShapeKind, Sheet, Workbook};

use crate::{read_xlsx, write_xlsx};

fn shape(id: u32, at: &str) -> Shape {
    Shape {
        id,
        kind: ShapeKind::Rectangle,
        anchor: kition_sheet_model::Anchor { cell: CellRef::parse(at).unwrap(), width: 90.0, height: 30.0, ..Default::default() },
        ..Default::default()
    }
}

fn shape_ids(wb: &Workbook) -> Vec<Vec<u32>> {
    wb.sheets.iter().map(|s| s.shapes.iter().map(|sh| sh.id).collect()).collect()
}

#[test]
fn a_files_drawing_ids_are_kept() {
    let anchor = |col: u32, id: u32| {
        format!(
            r#"<xdr:twoCellAnchor><xdr:from><xdr:col>{col}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:sp><xdr:nvSpPr><xdr:cNvPr id="{id}" name="s{id}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>"#,
            col + 2
        )
    };
    let bytes = super::drawing_tests::with_drawing(&(anchor(1, 5) + &anchor(4, 9)));
    let (wb, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(shape_ids(&wb), vec![vec![5, 9]]);
}

#[test]
fn drawing_ids_survive_save_and_reopen() {
    let mut wb = Workbook::new();
    wb.sheet_mut(0).unwrap().shapes = vec![shape(7, "B2"), shape(12, "E2")];
    let saved = write_xlsx(&wb).unwrap();
    let drawing = super::drawing_tests::part(&saved, "xl/drawings/drawing1.xml");
    assert!(drawing.contains("cNvPr id=\"7\"") && drawing.contains("cNvPr id=\"12\""), "{drawing}");
    let (back, _) = read_xlsx(&saved).unwrap();
    assert_eq!(shape_ids(&back), vec![vec![7, 12]]);
}

#[test]
fn ids_repeated_across_sheets_become_distinct_and_then_stay() {
    // Each sheet's drawing part numbers its shapes from 2, as Excel writes them.
    let mut wb = Workbook::new();
    wb.sheet_mut(0).unwrap().shapes = vec![shape(2, "B2")];
    let mut second = Sheet::new("Sheet2");
    second.shapes = vec![shape(2, "B2"), shape(3, "E2")];
    wb.sheets.push(Arc::new(second));
    let saved = write_xlsx(&wb).unwrap();
    let (first, _) = read_xlsx(&saved).unwrap();
    let ids: Vec<u32> = shape_ids(&first).concat();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "unique in the workbook: {ids:?}");
    assert_eq!(shape_ids(&first)[0], vec![2], "the first holder keeps its id");
    let (again, _) = read_xlsx(&write_xlsx(&first).unwrap()).unwrap();
    assert_eq!(shape_ids(&again), shape_ids(&first), "a second save and reopen keeps them");
}
