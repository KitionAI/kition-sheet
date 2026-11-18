//! What a saved file keeps of the file it was opened from, found by the canonical fidelity
//! comparison (`tests/fidelity.rs`): number cells holding text, file links and link texts, a
//! frozen sheet's scrolled top-left cell, what cell styles leave out, extension lists, picture
//! extensions, the filter range's name, `fullCalcOnLoad`, the file's own table styles, reserved
//! outline levels, and other applications' attributes on a filter. Synthetic packages only.

use std::io::Read;

use kition_sheet_core::{CellRef, Value};
use kition_sheet_model::{BorderStyle, Color, StyleParts, Workbook};

use super::{make_zip, minimal};
use crate::{read_xlsx, write_xlsx};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut out).unwrap();
    out
}

/// Reads, writes and reads again; the second workbook must equal the first.
fn round_trip(bytes: &[u8]) -> (Workbook, Vec<u8>) {
    let first = read_xlsx(bytes).unwrap().0;
    let written = write_xlsx(&first).unwrap();
    let second = read_xlsx(&written).unwrap().0;
    assert_eq!(serde_json::to_value(&second).unwrap(), serde_json::to_value(&first).unwrap());
    (first, written)
}

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

/// A one-sheet package with a styles part.
fn with_styles(sheet_body: &str, styles: &str, wb_extra: &str) -> Vec<u8> {
    let styles = format!(
        r#"<?xml version="1.0"?><styleSheet xmlns="{MAIN}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main">{styles}</styleSheet>"#
    );
    minimal(
        sheet_body,
        &[("xl/styles.xml", &styles)],
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        wb_extra,
    )
}

#[test]
fn a_number_cell_holding_text_keeps_it_as_text() {
    let bytes = minimal(r#"<sheetData><row r="1"><c r="A1"><v>.</v></c><c r="B1"><v>2.5</v></c></row></sheetData>"#, &[], "", "");
    let (wb, written) = round_trip(&bytes);
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.value(at("A1")), Value::text("."));
    assert_eq!(sheet.value(at("B1")), Value::Number(2.5));
    assert!(part(&written, "xl/sharedStrings.xml").contains("<t>.</t>"));
}

#[test]
fn file_links_stay_file_links_and_links_keep_their_text() {
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/hyperlink" Target="docs/report.xlsx" TargetMode="External"/><Relationship Id="rId2" Type="{REL}/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>"#
    );
    let bytes = minimal(
        r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>report</t></is></c></row></sheetData><hyperlinks><hyperlink ref="A1" r:id="rId1" display="the report"/><hyperlink ref="A2" r:id="rId2" display="https://example.com/"/><hyperlink ref="A3" location="Data!B2" display="Go to B2"/><hyperlink ref="A4" location="Data!C3" display="Data!C3"/></hyperlinks>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", &rels)],
        "",
        "",
    );
    let (wb, written) = round_trip(&bytes);
    let links = &wb.sheets[0].hyperlinks;
    let file = &links[&at("A1")];
    assert_eq!((file.target.as_str(), file.external, file.display.as_deref()), ("docs/report.xlsx", true, Some("the report")));
    // A URL is external by its form; its text is kept.
    let url = &links[&at("A2")];
    assert_eq!((url.external, url.display.as_deref()), (false, Some("https://example.com/")));
    assert!(url.is_external());
    assert_eq!(links[&at("A3")].display.as_deref(), Some("Go to B2"));
    // An in-workbook link whose text is its location reads as having no text of its own.
    assert_eq!(links[&at("A4")].display, None);
    let srels = part(&written, "xl/worksheets/_rels/sheet1.xml.rels");
    assert!(srels.contains(r#"Target="docs/report.xlsx" TargetMode="External""#), "{srels}");
    let sheet = part(&written, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"display="the report""#) && sheet.contains(r#"display="Go to B2""#), "{sheet}");
    assert!(!sheet.contains(r#"location="docs/report.xlsx""#), "{sheet}");
}

#[test]
fn a_frozen_sheet_keeps_its_scrolled_top_left_cell() {
    let bytes = minimal(
        r#"<sheetViews><sheetView topLeftCell="D1" workbookViewId="0"><pane ySplit="2" topLeftCell="A3" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><sheetData/>"#,
        &[],
        "",
        "",
    );
    let (wb, written) = round_trip(&bytes);
    assert_eq!((wb.sheets[0].view_top_left, wb.sheets[0].freeze), (at("D1"), Some((2, 0))));
    assert!(part(&written, "xl/worksheets/sheet1.xml").contains(r#"topLeftCell="D1""#));
}

#[test]
fn cell_styles_keep_the_formats_they_leave_out() {
    let styles = r#"<numFmts count="1"><numFmt numFmtId="164" formatCode="0.0"/></numFmts><fonts count="1"><font><sz val="11"/><name val="Arial"/></font></fonts><fills count="2"><fill><patternFill/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" applyFont="0" applyFill="0" applyBorder="0" applyAlignment="0" applyProtection="0"><alignment vertical="center"/></xf></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><cellStyles count="2"><cellStyle name="Normal" xfId="0" builtinId="0"/><cellStyle name="One decimal" xfId="1"/></cellStyles>"#;
    let (wb, written) = round_trip(&with_styles("<sheetData/>", styles, ""));
    let excludes = wb.cell_style_info.get("One decimal").map(|i| i.excludes);
    assert_eq!(excludes, Some(StyleParts { font: true, fill: true, border: true, alignment: true, protection: true, number_format: false }));
    let out = part(&written, "xl/styles.xml");
    assert!(out.contains(r#"applyFont="0" applyFill="0" applyBorder="0" applyAlignment="0" applyProtection="0""#), "{out}");
}

#[test]
fn extension_lists_are_kept_as_written_unless_they_name_parts() {
    // The calculation features' namespace is declared on the root, not on the `ext`.
    let wb_extra = r#"<calcPr calcId="191029" fullCalcOnLoad="1"/><extLst><ext uri="{B58B0392-4F1F-4190-BB64-5DF3571DCE5F}"><xcalcf:calcFeatures><xcalcf:feature name="microsoft.com:RD"/></xcalcf:calcFeatures></ext><ext uri="{876F7934-8845-4945-9796-88D515C7AA90}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:pivotCaches><pivotCache cacheId="1" r:id="rId7"/></x14:pivotCaches></ext></extLst>"#;
    let styles = r#"<fonts count="1"><font><sz val="11"/></font></fonts><cellXfs count="1"><xf/></cellXfs><extLst><ext uri="{EB79DEF2-80B8-43e5-95BD-54CBDDF9020C}"><x14:slicerStyles defaultSlicerStyle="SlicerStyleLight1"/></ext></extLst>"#;
    let bytes = with_styles("<sheetData/>", styles, "");
    // `minimal` puts `wb_extra` before the sheets; this package needs it after them.
    let wb_xml = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{MAIN}" xmlns:r="{REL}" xmlns:xcalcf="http://schemas.microsoft.com/office/spreadsheetml/2018/calcfeatures"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets>{wb_extra}</workbook>"#
    );
    let bytes = replace_part(&bytes, "xl/workbook.xml", wb_xml.as_bytes());
    let (wb, written) = round_trip(&bytes);
    assert_eq!(wb.extensions.workbook.len(), 1, "the extension naming a pivot cache part is left out");
    assert!(wb.calc.full_calc_on_load);
    let out = part(&written, "xl/workbook.xml");
    assert!(
        out.contains(r#"<extLst><ext xmlns:xcalcf="http://schemas.microsoft.com/office/spreadsheetml/2018/calcfeatures" uri="{B58B0392-4F1F-4190-BB64-5DF3571DCE5F}"><xcalcf:calcFeatures>"#),
        "{out}"
    );
    assert!(out.contains(r#"fullCalcOnLoad="1""#) && !out.contains("pivotCaches"), "{out}");
    let styles_out = part(&written, "xl/styles.xml");
    assert!(
        styles_out.contains(r#"<extLst><ext xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" uri="{EB79DEF2-80B8-43e5-95BD-54CBDDF9020C}"><x14:slicerStyles defaultSlicerStyle="SlicerStyleLight1"/></ext></extLst></styleSheet>"#),
        "{styles_out}"
    );
}

/// `bytes` with one part replaced.
fn replace_part(bytes: &[u8], name: &str, data: &[u8]) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        let part_name = entry.name().to_owned();
        parts.push((part_name.clone(), if part_name == name { data.to_vec() } else { content }));
    }
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    make_zip(&refs)
}

#[test]
fn pictures_keep_their_extensions() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\nfakeimagedata";
    let drawing = r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main"><xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>5</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"><a:extLst><a:ext uri="{28A0092B-C50C-407E-A947-70E740481C1C}"><a14:useLocalDpi val="0"/></a:ext></a:extLst></a:blip><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#;
    let drels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/image" Target="../media/image1.png"/></Relationships>"#
    );
    let srels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
    let base = minimal(r#"<sheetData/><drawing r:id="rId1"/>"#, &[("xl/worksheets/_rels/sheet1.xml.rels", &srels)], "", "");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(base)).unwrap();
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        parts.push((entry.name().to_owned(), content));
    }
    parts.push(("xl/drawings/drawing1.xml".into(), drawing.as_bytes().to_vec()));
    parts.push(("xl/drawings/_rels/drawing1.xml.rels".into(), drels.into_bytes()));
    parts.push(("xl/media/image1.png".into(), png.to_vec()));
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    let (wb, written) = round_trip(&make_zip(&refs));
    let image = &wb.sheets[0].images[0];
    assert_eq!(
        image.blip_ext.as_deref(),
        Some(concat!(
            r#"<a:ext xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main""#,
            r#" uri="{28A0092B-C50C-407E-A947-70E740481C1C}"><a14:useLocalDpi val="0"/></a:ext>"#
        ))
    );
    assert!(part(&written, "xl/drawings/drawing1.xml").contains("<a:blip r:embed=\"rId1\"><a:extLst><a:ext "));
}

#[test]
fn the_filter_range_is_named_as_excel_names_it_and_other_attributes_are_kept() {
    let bytes = minimal(
        r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><autoFilter xmlns:v="urn:example:vendor" ref="A1:C5" v:followRange="0"/>"#,
        &[],
        "",
        "",
    );
    let (wb, written) = round_trip(&bytes);
    let filter = wb.sheets[0].autofilter.as_ref().unwrap();
    assert_eq!(filter.extra.len(), 1);
    assert_eq!(
        (filter.extra[0].name.as_str(), filter.extra[0].ns.as_str(), filter.extra[0].value.as_str()),
        ("v:followRange", "urn:example:vendor", "0")
    );
    assert!(wb.names.is_empty(), "the filter's name is not a defined name of the model");
    let out = part(&written, "xl/workbook.xml");
    assert!(out.contains(r#"<definedName name="_xlnm._FilterDatabase" localSheetId="0" hidden="1">Data!$A$1:$C$5</definedName>"#), "{out}");
    assert!(part(&written, "xl/worksheets/sheet1.xml").contains(r#"xmlns:v="urn:example:vendor" v:followRange="0""#));
}

#[test]
fn the_file_table_styles_are_kept_with_their_formats() {
    let styles = r#"<fonts count="1"><font><sz val="11"/></font></fonts><cellXfs count="1"><xf/></cellXfs><dxfs count="2"><dxf><font><color theme="1"/></font><border><left style="thin"><color theme="4"/></left><horizontal style="thin"><color theme="4" tint="0.4"/></horizontal></border></dxf><dxf><font><b/></font></dxf></dxfs><tableStyles count="1" defaultTableStyle="TableStyleMedium9" defaultPivotStyle="PivotStyleLight16"><tableStyle name="Team Blue" pivot="0" count="2" xmlns:xr9="http://schemas.microsoft.com/office/spreadsheetml/2016/revision9" xr9:uid="{00000000-0000-0000-0000-000000000009}"><tableStyleElement type="wholeTable" dxfId="0"/><tableStyleElement type="firstRowStripe" size="2" dxfId="1"/></tableStyle></tableStyles>"#;
    let (wb, written) = round_trip(&with_styles("<sheetData/>", styles, ""));
    let ts = &wb.table_styles;
    assert_eq!(ts.default_table.as_deref(), Some("TableStyleMedium9"));
    assert_eq!(ts.default_pivot, None, "Excel's default is not stored");
    let style = &ts.styles[0];
    assert_eq!(
        (style.name.as_str(), style.pivot, style.table, style.uid.as_deref()),
        ("Team Blue", Some(false), None, Some("{00000000-0000-0000-0000-000000000009}"))
    );
    let whole = &style.elements[0];
    assert_eq!(whole.kind, "wholeTable");
    assert_eq!(whole.style.borders.horizontal.style, BorderStyle::Thin);
    // A differential format's text colour 1 sets the colour.
    assert_eq!(whole.style.font.color, Color::Theme(1, 0));
    assert_eq!((style.elements[1].kind.as_str(), style.elements[1].size, style.elements[1].style.font.bold), ("firstRowStripe", Some(2), true));
    let out = part(&written, "xl/styles.xml");
    assert!(out.contains(r#"<tableStyles count="1" defaultTableStyle="TableStyleMedium9" defaultPivotStyle="PivotStyleLight16"><tableStyle name="Team Blue" pivot="0" count="2""#), "{out}");
    assert!(out.contains(r#"<horizontal style="thin">"#) && out.contains(r#"<font><color theme="1"/></font>"#), "{out}");
}

#[test]
fn the_outline_levels_a_file_reserves_are_kept() {
    let bytes = minimal(r#"<sheetFormatPr defaultRowHeight="15" outlineLevelCol="3"/><sheetData/>"#, &[], "", "");
    let (wb, written) = round_trip(&bytes);
    assert_eq!(wb.sheets[0].outline_levels, Some((0, 3)));
    assert!(part(&written, "xl/worksheets/sheet1.xml").contains(r#"outlineLevelCol="3""#));
}
