//! Package-level details a saved file keeps or writes the way Excel does: the workbook window
//! (`workbookView`), the versions that wrote the file (`fileVersion`), the extended properties'
//! summary of the worksheets (`docProps/app.xml`), the rows' column spans, and which named cell
//! styles are Excel's built-in ones.

use std::io::Read;

use kition_sheet_model::{CellStyleInfo, FileVersion, Sheet, Workbook, WorkbookView};

use super::{CT, ROOT_RELS, make_zip, minimal};
use crate::{read_xlsx, write_xlsx};

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut out).unwrap();
    out
}

/// Three sheets (the second one's part missing, so it is skipped on read), a window with every
/// attribute, and a `fileVersion`.
fn three_sheets() -> Vec<u8> {
    let wb = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><fileVersion appName="xl" lastEdited="7" lowestEdited="6" rupBuild="27425" codeName="{00000000-0000-0000-0000-000000000001}"/><workbookPr/><bookViews><workbookView visibility="visible" minimized="0" showHorizontalScroll="1" showVerticalScroll="0" showSheetTabs="1" xWindow="-120" yWindow="-120" windowWidth="29040" windowHeight="15720" tabRatio="745" firstSheet="1" activeTab="2" autoFilterDateGrouping="0"/></bookViews><sheets><sheet name="One" sheetId="1" r:id="rId1"/><sheet name="Gone" sheetId="2" r:id="rId9"/><sheet name="Three" sheetId="3" r:id="rId3"/></sheets></workbook>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet3.xml"/></Relationships>"#;
    let sheet = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#;
    make_zip(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/sheet3.xml", sheet.as_bytes()),
    ])
}

#[test]
fn the_workbook_window_and_file_version_are_kept() {
    let wb = read_xlsx(&three_sheets()).unwrap().0;
    assert_eq!(wb.sheets.len(), 2);
    assert_eq!(wb.active_sheet, 1);
    assert_eq!(
        wb.view,
        Some(WorkbookView {
            x: Some(-120),
            y: Some(-120),
            width: Some(29040),
            height: Some(15720),
            tab_ratio: Some(745),
            // The file's second tab was skipped: the first tab shown is the next one read.
            first_sheet: Some(1),
            visibility: Some("visible".into()),
            minimized: Some(false),
            show_horizontal_scroll: Some(true),
            show_vertical_scroll: Some(false),
            show_sheet_tabs: Some(true),
            auto_filter_date_grouping: Some(false),
        })
    );
    assert_eq!(
        wb.file_version,
        Some(FileVersion {
            app_name: "xl".into(),
            last_edited: "7".into(),
            lowest_edited: "6".into(),
            rup_build: "27425".into(),
            code_name: "{00000000-0000-0000-0000-000000000001}".into(),
        })
    );

    let written = write_xlsx(&wb).unwrap();
    let xml = part(&written, "xl/workbook.xml");
    assert!(
        xml.contains(r#"<fileVersion appName="xl" lastEdited="7" lowestEdited="6" rupBuild="27425" codeName="{00000000-0000-0000-0000-000000000001}"/><workbookPr/>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<workbookView visibility="visible" minimized="0" showHorizontalScroll="1" showVerticalScroll="0" showSheetTabs="1" xWindow="-120" yWindow="-120" windowWidth="29040" windowHeight="15720" tabRatio="745" firstSheet="1" activeTab="1" autoFilterDateGrouping="0"/>"#),
        "{xml}"
    );
    let back = read_xlsx(&written).unwrap().0;
    assert_eq!((back.view, back.file_version), (wb.view, wb.file_version));
}

#[test]
fn the_first_tab_shown_never_passes_the_active_one_or_the_last_sheet() {
    let mut wb = read_xlsx(&three_sheets()).unwrap().0;
    wb.active_sheet = 0;
    let xml = part(&write_xlsx(&wb).unwrap(), "xl/workbook.xml");
    assert!(xml.contains(r#" firstSheet="0" activeTab="0""#), "{xml}");
    wb.view.as_mut().unwrap().first_sheet = Some(5);
    let xml = part(&write_xlsx(&wb).unwrap(), "xl/workbook.xml");
    assert!(!xml.contains("firstSheet"), "{xml}");
}

#[test]
fn a_workbook_without_a_window_or_version_writes_only_the_active_tab() {
    let wb = read_xlsx(&minimal(r#"<sheetData/>"#, &[], "", "")).unwrap().0;
    assert_eq!((wb.view.as_ref(), wb.file_version.as_ref()), (None, None));
    let xml = part(&write_xlsx(&wb).unwrap(), "xl/workbook.xml");
    assert!(xml.contains(r#"<bookViews><workbookView activeTab="0"/></bookViews>"#), "{xml}");
    assert!(!xml.contains("fileVersion"), "{xml}");
    // An empty `fileVersion` and a window naming only its active tab are not kept.
    let wb = read_xlsx(&minimal(r#"<sheetData/>"#, &[], "", r#"<fileVersion/><bookViews><workbookView activeTab="0"/></bookViews>"#)).unwrap().0;
    assert_eq!((wb.view, wb.file_version), (None, None));
}

#[test]
fn the_extended_properties_list_the_worksheets() {
    let mut wb = Workbook::new();
    wb.sheets.push(std::sync::Arc::new(Sheet::new("R&D")));
    let xml = part(&write_xlsx(&wb).unwrap(), "docProps/app.xml");
    assert!(
        xml.contains(r#"<HeadingPairs><vt:vector size="2" baseType="variant"><vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>2</vt:i4></vt:variant></vt:vector></HeadingPairs><TitlesOfParts><vt:vector size="2" baseType="lpstr"><vt:lpstr>Sheet1</vt:lpstr><vt:lpstr>R&amp;D</vt:lpstr></vt:vector></TitlesOfParts>"#),
        "{xml}"
    );
}

#[test]
fn rows_carry_the_column_span_of_their_block_of_sixteen() {
    // Rows 1 and 3 share the first block (columns B..E); row 17 starts the next (column C only);
    // row 18 has only a height, in the second block.
    let wb = read_xlsx(&minimal(
        r#"<sheetData><row r="1"><c r="B1"><v>1</v></c><c r="C1"><v>2</v></c></row><row r="3"><c r="E3"><v>3</v></c></row><row r="17"><c r="C17"><v>4</v></c></row><row r="18" ht="30" customHeight="1"/></sheetData>"#,
        &[],
        "",
        "",
    ))
    .unwrap()
    .0;
    let xml = part(&write_xlsx(&wb).unwrap(), "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<row r="1" spans="2:5">"#), "{xml}");
    assert!(xml.contains(r#"<row r="3" spans="2:5">"#), "{xml}");
    assert!(xml.contains(r#"<row r="17" spans="3:3">"#), "{xml}");
    assert!(xml.contains(r#"<row r="18" spans="3:3" ht="30" customHeight="1"/>"#), "{xml}");
}

/// A style part whose Normal style has another language's name, with two built-in styles (one
/// changed from the built-in format, one an outline level style hidden from the gallery) and a
/// style of the file's own.
const NAMED_STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="4"><xf fontId="0"/><xf fontId="1"/><xf fontId="1"/><xf fontId="0"/></cellStyleXfs><cellXfs count="1"><xf fontId="0" xfId="0"/></cellXfs><cellStyles count="4"><cellStyle name="Standard" xfId="0" builtinId="0"/><cellStyle name="Gut" xfId="1" builtinId="26" customBuiltin="1"/><cellStyle name="RowLevel_2" xfId="2" builtinId="1" iLevel="1" hidden="1"/><cellStyle name="Totals" xfId="3"/></cellStyles></styleSheet>"#;

fn named_styles() -> Workbook {
    let rel = r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;
    read_xlsx(&minimal(r#"<sheetData/>"#, &[("xl/styles.xml", NAMED_STYLES)], rel, "")).unwrap().0
}

#[test]
fn built_in_cell_styles_keep_their_ids_whatever_the_files_language_names_them() {
    let wb = named_styles();
    // The Normal style is the workbook's default style, not a named style of its own.
    let names: Vec<&str> = wb.cell_styles.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Gut", "RowLevel_2", "Totals"]);
    assert_eq!(
        wb.cell_style_info.get("Gut"),
        Some(&CellStyleInfo { builtin: Some(26), level: None, custom_builtin: true, hidden: false, ..Default::default() })
    );
    assert_eq!(
        wb.cell_style_info.get("RowLevel_2"),
        Some(&CellStyleInfo { builtin: Some(1), level: Some(1), custom_builtin: false, hidden: true, ..Default::default() })
    );
    assert_eq!(wb.cell_style_info.get("Totals"), None);

    let written = write_xlsx(&wb).unwrap();
    let xml = part(&written, "xl/styles.xml");
    assert!(
        xml.contains(r#"<cellStyle name="Normal" xfId="0" builtinId="0"/><cellStyle name="Gut" xfId="1" builtinId="26" customBuiltin="1"/><cellStyle name="RowLevel_2" xfId="2" builtinId="1" iLevel="1" hidden="1"/><cellStyle name="Totals" xfId="3"/></cellStyles>"#),
        "{xml}"
    );
    let back = read_xlsx(&written).unwrap().0;
    assert_eq!((&back.cell_styles, &back.cell_style_info), (&wb.cell_styles, &wb.cell_style_info));
}
