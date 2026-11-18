//! Opening, exporting and copying workbooks against the golden WPS cases (module M9: 工作簿导入,
//! 工作簿导出, 文件预览, 保存模板): files that are not spreadsheets, empty or damaged say so; a web
//! page named .xls opens as its table; rows without a height of their own are sized as WPS sizes
//! them; templates are written as templates; the File tab's exports answer their bytes; and a copy
//! taken from one workbook pastes into another with its formats, checkboxes and merges.

use kition_sheet_engine::Session;
use kition_sheet_engine::core::CellRef;
use kition_sheet_engine::io::{open_bytes, save_bytes};
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

/// A part of a saved package, as text.
fn part(bytes: &[u8], name: &str) -> String {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut text).unwrap();
    text
}

fn error(name: &str, bytes: &[u8]) -> String {
    match open_bytes(name, bytes) {
        Ok(_) => panic!("{name} opened"),
        Err(e) => e.to_string(),
    }
}

/// IMP-C74, C75, C77, C64: an empty file, a damaged workbook and a file that is something else
/// are refused with what they are.
#[test]
fn files_that_are_not_workbooks_say_why() {
    assert!(error("empty.xlsx", &[]).contains("is empty (0 bytes)"));
    assert!(error("broken.xlsx", b"PK\x03\x04 broken zip").contains("is damaged and can't be opened"));
    assert!(error("picture.xlsx", &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0]).contains("is a PNG picture, not a spreadsheet"));
    assert!(error("paper.xlsx", b"%PDF-1.7\n1 0 obj").contains("is a PDF document, not a spreadsheet"));
}

/// IMP-C60: many systems export "Excel" files that are web pages named .xls; the page's table
/// opens, merged cells and all.
#[test]
fn a_web_page_named_xls_opens_as_its_table() {
    let page = "<html><body><table><tr><td colspan=\"2\"><b>资产</b></td></tr><tr><td>主机</td><td>12</td></tr></table></body></html>";
    let (wb, _) = open_bytes("核查结果.xls", page.as_bytes()).unwrap();
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.name, "核查结果");
    assert_eq!(sheet.value(CellRef::parse("A1").unwrap()).as_text().map(str::to_owned), Some("资产".to_owned()));
    assert_eq!(sheet.value(CellRef::parse("B2").unwrap()).as_f64(), Some(12.0));
    assert!(!sheet.merges.is_empty());
}

/// IMP-C30 (WPS for Mac): rows whose height the last application only worked out (no
/// `customHeight`) are sized again from their wrapped text, rows set by hand keep theirs, and a
/// height that is no whole number of pixels is cut to whole points.
#[test]
fn opened_rows_are_sized_as_wps_sizes_them() {
    let mut s = session();
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "第一行\n第二行\n第三行\n第四行"}));
    run(&mut s, "home.wrapText", json!({"range": "A1"}));
    run(&mut s, "home.rowHeight", json!({"rows": "3", "height": 40}));
    let bytes = save_bytes(&s.doc().unwrap().wb, "rows.xlsx").unwrap();
    let (wb, _) = open_bytes("rows.xlsx", &bytes).unwrap();
    let sheet = wb.sheet(0).unwrap();
    // Four lines of 11 pt text: 4 × 16.775 pt, up to 68 pt.
    assert_eq!(sheet.row_height(0), 68.0 * 4.0 / 3.0);
    assert_eq!(sheet.row_height(2), 40.0);
    assert_eq!(sheet.row_height(1), sheet.default_row_height);
}

/// TPL-C5: a workbook saved as a template is a template package; EXP-C5, C9: the File tab's
/// exports answer their bytes (a copy of the workbook, a template).
#[test]
fn templates_and_workbook_copies_export() {
    let mut s = session();
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "模板内容"}));
    let template = save_bytes(&s.doc().unwrap().wb, "A.xltx").unwrap();
    assert!(part(&template, "[Content_Types].xml").contains("spreadsheetml.template.main+xml"), "an .xltx is written with the template's main part");
    let workbook = save_bytes(&s.doc().unwrap().wb, "A.xlsx").unwrap();
    assert!(part(&workbook, "[Content_Types].xml").contains("spreadsheetml.sheet.main+xml"));
    for (format, mime) in [
        ("xltx", "application/vnd.openxmlformats-officedocument.spreadsheetml.template"),
        ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    ] {
        let answer = run(&mut s, "file.export", json!({"format": format}));
        assert_eq!((answer["extension"].as_str(), answer["mime"].as_str()), (Some(format), Some(mime)));
        let bytes = kition_sheet_engine::io::base64_decode(answer["data"].as_str().unwrap()).unwrap();
        let (back, _) = open_bytes(&format!("x.{format}"), &bytes).unwrap();
        assert_eq!(back.sheet(0).unwrap().value(CellRef::parse("A1").unwrap()).as_text().map(str::to_owned), Some("模板内容".to_owned()));
    }
    // Exporting changes nothing.
    assert!(!s.doc().unwrap().is_dirty() || s.doc().unwrap().undo.len() == 1);
}

/// A copy taken in one workbook pastes into another as within one: formats, checkboxes, merges
/// and notes come along (the clipboard's HTML carries Kition Sheet's own copy of the cells).
#[test]
fn a_copy_pastes_into_another_workbook_with_its_formats() {
    let mut a = session();
    run(&mut a, "cell.set", json!({"cell": "A1", "input": "标题"}));
    run(&mut a, "home.bold", json!({"range": "A1"}));
    run(&mut a, "home.fillColor", json!({"range": "A1", "color": "#FFFF00"}));
    run(&mut a, "home.mergeCells", json!({"range": "A1:B1"}));
    run(&mut a, "cell.set", json!({"cell": "A2", "input": "TRUE"}));
    run(&mut a, "insert.checkbox", json!({"range": "A2"}));
    let copied = run(&mut a, "edit.copy", json!({"range": "A1:B2", "html": true}));
    let (text, html) = (copied["text"].as_str().unwrap().to_owned(), copied["html"].as_str().unwrap().to_owned());
    assert!(html.contains("data-kition-sheet=\""));

    let mut b = session();
    run(&mut b, "selection.set", json!({"range": "C3"}));
    run(&mut b, "edit.paste", json!({"text": text, "html": html}));
    let d = b.doc().unwrap();
    let sheet = d.wb.active().unwrap();
    let style = d.wb.styles.get(sheet.style_id(CellRef::parse("C3").unwrap()));
    assert!(style.font.bold);
    assert_eq!(style.fill.fg, kition_sheet_engine::model::Color::Rgb(0xFFFF00));
    assert!(sheet.merges.iter().any(|m| m.a1() == "C3:D3"));
    assert!(kition_sheet_engine::cmd::extra::is_checkbox(&d.wb, d.wb.active_sheet, CellRef::parse("C4").unwrap()));
    // HTML from elsewhere still pastes as a table.
    let mut c = session();
    run(&mut c, "edit.paste", json!({"text": "x", "html": "<table><tr><td><b>x</b></td></tr></table>"}));
    assert_eq!(c.doc().unwrap().wb.active().unwrap().value(CellRef::parse("A1").unwrap()).as_text().map(str::to_owned), Some("x".to_owned()));
}

/// EXP-C93, EXP-C10: CSV holds what the cells show (WPS and Excel), not the stored numbers: a
/// date as a date, a percentage as one, thousands separators quoted.
#[test]
fn csv_exports_what_cells_show() {
    let mut s = session();
    run(&mut s, "range.setValues", json!({"range": "A1", "values": [[46295, 0.125, 1234.5]]}));
    run(&mut s, "home.numberFormat", json!({"range": "A1", "format": "yyyy-mm-dd"}));
    run(&mut s, "home.numberFormat", json!({"range": "B1", "format": "0.0%"}));
    run(&mut s, "home.numberFormat", json!({"range": "C1", "format": "#,##0.00"}));
    let answer = run(&mut s, "file.export", json!({"format": "csv"}));
    let csv = String::from_utf8(kition_sheet_engine::io::base64_decode(answer["data"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(csv, "2026-09-30,12.5%,\"1,234.50\"\r\n");
}

/// EXP-C6: sheets print on A4 unless they say otherwise (WPS; Excel outside North America), and
/// the workbook's Chinese text is in the PDF when the host has given the engine a font for it.
#[test]
fn pdfs_are_a4_by_default() {
    let mut s = session();
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "x"}));
    let answer = run(&mut s, "file.export", json!({"format": "pdf"}));
    let pdf = kition_sheet_engine::io::base64_decode(answer["data"].as_str().unwrap()).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/MediaBox [0 0 595.28 841.89]"), "an A4 page");
}

/// EXP-C69 (module M10): CSV for Excel for Windows carries a UTF-8 byte-order mark when asked (the
/// editor's Export as CSV asks), as Excel's and WPS's 「CSV UTF-8」 do; without it, it has none.
#[test]
fn csv_can_carry_a_byte_order_mark() {
    let mut s = session();
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "中文"}));
    let with = run(&mut s, "file.export", json!({"format": "csv", "bom": true}));
    let bytes = kition_sheet_engine::io::base64_decode(with["data"].as_str().unwrap()).unwrap();
    assert_eq!(bytes.get(..3), Some(&[0xEF, 0xBB, 0xBF][..]));
    assert_eq!(String::from_utf8(bytes[3..].to_vec()).unwrap(), "中文\r\n");
    let without = run(&mut s, "file.export", json!({"format": "csv"}));
    let bytes = kition_sheet_engine::io::base64_decode(without["data"].as_str().unwrap()).unwrap();
    assert!(bytes.starts_with("中文".as_bytes()));
    // Opened again, the mark is not part of A1.
    let (wb, _) = kition_sheet_engine::io::open_bytes("back.csv", &[&[0xEF, 0xBB, 0xBF][..], "中文\r\n".as_bytes()].concat()).unwrap();
    assert_eq!(wb.sheet(0).unwrap().value(CellRef::parse("A1").unwrap()).as_text(), Some("中文"));
}
