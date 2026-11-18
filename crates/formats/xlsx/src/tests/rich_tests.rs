//! Rich text in cells: runs of shared strings and inline strings are read with every run
//! property, written back as shared strings with runs (one item per distinct rich text), and read
//! again unchanged.

use std::io::Read;
use std::sync::Arc;

use kition_sheet_core::{CellRef, Value};
use kition_sheet_model::{Cell, RichText, RunColor, RunFont, StyleId, TextRun, Underline, VertAlign, Workbook};

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const SST_REL: &str = r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>"#;

fn shared_strings(items: &str) -> String {
    format!(r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{items}</sst>"#)
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut out).unwrap();
    out
}

fn runs(cell: &Cell) -> Vec<TextRun> {
    cell.runs().map(|r| r.runs.clone()).unwrap_or_default()
}

/// A run setting every property the format has.
fn every_property() -> RunFont {
    RunFont {
        name: Some("Cambria".into()),
        size: Some(10.5),
        bold: Some(true),
        italic: Some(false),
        underline: Some(Underline::Double),
        strike: Some(true),
        color: Some(RunColor { theme: Some(4), tint: Some(-0.249977111117893), ..RunColor::default() }),
        vert: Some(VertAlign::Superscript),
        family: Some(1),
        charset: Some(134),
        scheme: Some("major".into()),
        outline: Some(true),
        shadow: Some(true),
        condense: Some(false),
        extend: Some(true),
    }
}

#[test]
fn shared_and_inline_rich_strings_are_read_with_every_property() {
    let items = shared_strings(concat!(
        r#"<si><t>plain</t></si>"#,
        r#"<si><r><t xml:space="preserve">Total </t></r><r><rPr><b/><sz val="11"/><color rgb="FFFF0000"/><rFont val="Calibri"/><family val="2"/><scheme val="minor"/></rPr><t>due</t></r></si>"#,
        r#"<si><r><rPr><b/><i val="0"/><strike/><condense val="0"/><extend/><outline/><shadow/><u val="double"/><vertAlign val="superscript"/><sz val="10.5"/><color theme="4" tint="-0.249977111117893"/><rFont val="Cambria"/><family val="1"/><charset val="134"/><scheme val="major"/></rPr><t>x</t></r><r><rPr><color indexed="10"/><u/></rPr><t>_x0041_</t></r><rPh sb="0" eb="1"><t>ignored</t></rPh></si>"#,
        r#"<si><r><t>no</t></r><r><t> marks</t></r></si>"#,
    ));
    let body = concat!(
        r#"<sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>2</v></c><c r="D1" t="s"><v>3</v></c>"#,
        r#"<c r="E1" t="inlineStr"><is><r><rPr><i/></rPr><t>in</t></r><r><t>line</t></r></is></c><c r="F1" t="s"><v>1</v></c></row></sheetData>"#
    );
    let bytes = minimal(body, &[("xl/sharedStrings.xml", &items)], SST_REL, "");
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let sheet = &wb.sheets[0];
    let cell = |a1: &str| sheet.cell(CellRef::parse(a1).unwrap()).unwrap().clone();

    assert_eq!(cell("A1").rich, None);
    assert_eq!(cell("A1").value, Value::text("plain"));

    let b1 = cell("B1");
    assert_eq!(b1.value, Value::text("Total due"));
    let due = RunFont {
        bold: Some(true),
        size: Some(11.0),
        color: Some(RunColor { rgb: Some("FFFF0000".into()), ..RunColor::default() }),
        name: Some("Calibri".into()),
        family: Some(2),
        scheme: Some("minor".into()),
        ..RunFont::default()
    };
    assert_eq!(runs(&b1), vec![TextRun::new("Total ", RunFont::default()), TextRun::new("due", due)]);
    assert!(Arc::ptr_eq(b1.rich.as_ref().unwrap(), cell("F1").rich.as_ref().unwrap()), "cells naming one item share its runs");

    let c1 = cell("C1");
    assert_eq!(c1.value, Value::text("xA"), "escapes decoded, phonetic runs left out");
    let indexed =
        RunFont { color: Some(RunColor { indexed: Some(10), ..RunColor::default() }), underline: Some(Underline::Single), ..RunFont::default() };
    assert_eq!(runs(&c1), vec![TextRun::new("x", every_property()), TextRun::new("A", indexed)]);

    assert_eq!(cell("D1").rich, None, "runs that set nothing are plain text");
    assert_eq!(cell("D1").value, Value::text("no marks"));

    let e1 = cell("E1");
    assert_eq!(e1.value, Value::text("inline"));
    assert_eq!(runs(&e1), vec![TextRun::new("in", RunFont { italic: Some(true), ..RunFont::default() }), TextRun::new("line", RunFont::default())]);
}

#[test]
fn rich_text_is_written_as_shared_strings_with_runs_and_reads_back() {
    let rich = RichText::new([
        TextRun::new(" lead", RunFont { bold: Some(true), ..RunFont::default() }),
        TextRun::new("<&>", every_property()),
        TextRun::new("tail ", RunFont::default()),
    ])
    .unwrap();
    let text = rich.text();
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).unwrap();
    let other = RichText::new([TextRun::new(text.clone(), RunFont { italic: Some(true), ..RunFont::default() })]).unwrap();
    sheet.set_cell(CellRef::new(0, 0), Cell::rich_text(rich.clone(), StyleId::DEFAULT));
    sheet.set_cell(CellRef::new(1, 0), Cell::rich_text(rich.clone(), StyleId::DEFAULT));
    sheet.set_cell(CellRef::new(2, 0), Cell::value(Value::text(text.as_str())));
    sheet.set_cell(CellRef::new(3, 0), Cell::rich_text(other.clone(), StyleId::DEFAULT));
    // Runs that no longer spell the cell's text are not written.
    let mut stale = Cell::value(Value::text("changed"));
    stale.rich = Some(Arc::new(rich.clone()));
    sheet.set_cell(CellRef::new(4, 0), stale);

    let bytes = write_xlsx(&wb).unwrap();
    let sst = part(&bytes, "xl/sharedStrings.xml");
    assert_eq!(sst.matches("<si>").count(), 4, "one item per distinct text and runs: {sst}");
    assert!(sst.contains(r#"uniqueCount="4""#) && sst.contains(r#"count="5""#), "{sst}");
    assert!(sst.contains(r#"<r><rPr><b/></rPr><t xml:space="preserve"> lead</t></r>"#), "{sst}");
    assert!(sst.contains(r#"<t xml:space="preserve">&lt;&amp;&gt;</t>"#), "{sst}");

    let (back, _) = read_xlsx(&bytes).unwrap();
    let sheet = &back.sheets[0];
    let cell = |r: u32| sheet.cell(CellRef::new(r, 0)).unwrap().clone();
    assert_eq!(cell(0).runs(), Some(&rich));
    assert_eq!(cell(1).runs(), Some(&rich));
    assert_eq!(cell(2).rich, None);
    assert_eq!(cell(2).value, Value::text(text.as_str()));
    assert_eq!(cell(3).runs(), Some(&other));
    assert_eq!(cell(4).rich, None);
    assert_eq!(cell(4).value, Value::text("changed"));

    // And once more: the written file reads to the same model.
    let again = write_xlsx(&back).unwrap();
    assert_eq!(part(&again, "xl/sharedStrings.xml"), sst);
}

#[test]
fn a_formula_cell_never_takes_runs() {
    let items = shared_strings(r#"<si><r><rPr><b/></rPr><t>x</t></r></si>"#);
    let body = r#"<sheetData><row r="1"><c r="A1" t="s"><f>"x"</f><v>0</v></c></row></sheetData>"#;
    let bytes = minimal(body, &[("xl/sharedStrings.xml", &items)], SST_REL, "");
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let a1 = wb.sheets[0].cell(CellRef::new(0, 0)).unwrap();
    assert!(a1.formula.is_some());
    assert_eq!(a1.rich, None);
}
