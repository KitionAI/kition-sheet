//! Fonts in the style part: a font's character set and theme font reference, and the workbook's
//! own colours (a custom indexed palette, the recently used colours), read and written back.

use std::io::Read;

use kition_sheet_core::CellRef;
use kition_sheet_model::{Color, Palette, RunColor, Workbook};

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const STYLES_REL: &str =
    r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;

/// A style part (font 0 the default font): font 1 the theme's body font, font 2 a Chinese face with its character set,
/// a palette whose index 10 is not red, and two recently used colours.
const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="4"><font><sz val="11"/><name val="Calibri"/></font><font><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/><charset val="134"/><scheme val="minor"/></font><font><b/><sz val="12"/><color indexed="10"/><name val="SimSun"/><family val="3"/><charset val="134"/></font><font><sz val="18"/><name val="Cambria"/><scheme val="major"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellXfs count="4"><xf fontId="0"/><xf fontId="1" applyFont="1"/><xf fontId="2" applyFont="1"/><xf fontId="3" applyFont="1"/></cellXfs><colors><indexedColors><rgbColor rgb="FF000000"/><rgbColor rgb="FFFFFFFF"/><rgbColor rgb="FFFF0000"/><rgbColor rgb="FF00FF00"/><rgbColor rgb="FF0000FF"/><rgbColor rgb="FFFFFF00"/><rgbColor rgb="FFFF00FF"/><rgbColor rgb="FF00FFFF"/><rgbColor rgb="FF000000"/><rgbColor rgb="FFFFFFFF"/><rgbColor rgb="FF123456"/></indexedColors><mruColors><color rgb="FF00B050"/><color theme="4" tint="-0.249977111117893"/></mruColors></colors></styleSheet>"#;

fn package() -> Vec<u8> {
    minimal(
        r#"<sheetData><row r="1"><c r="A1" s="1" t="inlineStr"><is><t>body</t></is></c><c r="B1" s="2" t="inlineStr"><is><t>face</t></is></c><c r="C1" s="3" t="inlineStr"><is><t>title</t></is></c></row></sheetData>"#,
        &[("xl/styles.xml", STYLES)],
        STYLES_REL,
        "",
    )
}

fn styles_part(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name("xl/styles.xml").unwrap().read_to_string(&mut out).unwrap();
    out
}

fn font_at(wb: &Workbook, a1: &str) -> kition_sheet_model::Font {
    let sheet = wb.sheet(0).unwrap();
    let cell = sheet.cell(CellRef::parse(a1).unwrap()).unwrap();
    wb.styles.get(cell.style).font.clone()
}

#[test]
fn a_fonts_character_set_and_theme_font_reference_are_kept() {
    let wb = read_xlsx(&package()).unwrap().0;
    let body = font_at(&wb, "A1");
    assert_eq!((body.charset, body.scheme.as_deref()), (Some(134), Some("minor")));
    let face = font_at(&wb, "B1");
    assert_eq!((face.name.as_str(), face.charset, face.scheme.as_deref()), ("SimSun", Some(134), None));
    // `indexed` resolves through the file's palette, not the built-in one (index 10 is red there).
    assert_eq!(face.color, Color::Rgb(0x123456));
    let title = font_at(&wb, "C1");
    assert_eq!((title.charset, title.scheme.as_deref()), (None, Some("major")));

    let written = write_xlsx(&wb).unwrap();
    let xml = styles_part(&written);
    assert!(xml.contains(r#"<name val="SimSun"/><family val="2"/><charset val="134"/></font>"#), "{xml}");
    assert!(xml.contains(r#"<charset val="134"/><scheme val="minor"/>"#), "{xml}");
    assert!(xml.contains(r#"<name val="Cambria"/><family val="2"/><scheme val="major"/>"#), "{xml}");
    let back = read_xlsx(&written).unwrap().0;
    for a1 in ["A1", "B1", "C1"] {
        assert_eq!(font_at(&back, a1), font_at(&wb, a1), "{a1}");
    }
}

#[test]
fn choosing_a_face_forgets_the_old_ones_character_set_and_theme_reference() {
    let wb = read_xlsx(&package()).unwrap().0;
    let mut font = font_at(&wb, "A1");
    font.set_name("Arial");
    assert_eq!((font.name.as_str(), font.charset, font.scheme), ("Arial", None, None));
}

#[test]
fn a_workbooks_own_palette_and_recent_colours_are_kept() {
    let wb = read_xlsx(&package()).unwrap().0;
    let palette = wb.palette.clone().unwrap();
    assert_eq!(palette.indexed.len(), 11);
    assert_eq!(palette.indexed[10], 0x123456);
    assert_eq!(
        palette.recent,
        vec![
            RunColor { rgb: Some("FF00B050".into()), ..RunColor::default() },
            RunColor { theme: Some(4), tint: Some(-0.249977111117893), ..RunColor::default() },
        ]
    );

    let written = write_xlsx(&wb).unwrap();
    let xml = styles_part(&written);
    assert!(xml.contains(r#"<rgbColor rgb="FF123456"/></indexedColors><mruColors><color rgb="FF00B050"/><color theme="4" tint="-0.249977111117893"/></mruColors></colors></styleSheet>"#), "{xml}");
    assert_eq!(read_xlsx(&written).unwrap().0.palette, wb.palette);

    // A workbook without its own colours writes none.
    let plain = write_xlsx(&Workbook::new()).unwrap();
    assert!(!styles_part(&plain).contains("<colors>"));
    assert_eq!(read_xlsx(&plain).unwrap().0.palette, None::<Palette>);
}

#[test]
fn a_files_normal_style_is_the_default_for_unstyled_cells() {
    // xf 0 (Normal): Arial 10, top-left, wrapped. A1 names it, B1 has no style, C1 is bold.
    let styles = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="10"/><name val="Arial"/><charset val="204"/></font><font><b/><sz val="10"/><name val="Arial"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellXfs count="2"><xf fontId="0" applyAlignment="1"><alignment horizontal="left" vertical="top" wrapText="1"/></xf><xf fontId="1" applyFont="1"/></cellXfs></styleSheet>"#;
    let bytes = minimal(
        r#"<sheetData><row r="1"><c r="A1" s="0" t="inlineStr"><is><t>named</t></is></c><c r="B1" t="inlineStr"><is><t>plain</t></is></c><c r="C1" s="1" t="inlineStr"><is><t>bold</t></is></c></row></sheetData>"#,
        &[("xl/styles.xml", styles)],
        STYLES_REL,
        "",
    );
    let wb = read_xlsx(&bytes).unwrap().0;
    let normal = wb.styles.get(kition_sheet_model::StyleId::DEFAULT).clone();
    assert_eq!((normal.font.name.as_str(), normal.font.size, normal.font.charset), ("Arial", 10.0, Some(204)));
    assert!(normal.align.wrap);
    for a1 in ["A1", "B1"] {
        assert_eq!(wb.sheet(0).unwrap().cell(CellRef::parse(a1).unwrap()).unwrap().style, kition_sheet_model::StyleId::DEFAULT, "{a1}");
    }
    assert!(font_at(&wb, "C1").bold);
    // Written back as the first cell format and the first font; read again the same.
    let written = write_xlsx(&wb).unwrap();
    let xml = styles_part(&written);
    assert!(
        xml.contains(r#"<fonts count="2"><font><sz val="10"/><color theme="1"/><name val="Arial"/><family val="2"/><charset val="204"/></font>"#),
        "{xml}"
    );
    assert!(xml.contains(r#"<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment horizontal="left" vertical="top" wrapText="1"/></xf>"#), "{xml}");
    let back = read_xlsx(&written).unwrap().0;
    assert_eq!(back.styles.get(kition_sheet_model::StyleId::DEFAULT), &normal);
    assert_eq!(font_at(&back, "C1"), font_at(&wb, "C1"));
}
