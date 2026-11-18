//! .xls reading, on workbooks built here record by record (BIFF8 in a compound file), as the
//! repository never holds files written by other applications.

use super::*;

fn rec(kind: u16, data: &[u8]) -> Vec<u8> {
    let mut out = kind.to_le_bytes().to_vec();
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// A ShortXLUnicodeString (compressed when it is Latin-1, else UTF-16).
fn short(text: &str) -> Vec<u8> {
    let wide = text.chars().any(|c| u32::from(c) > 0xFF);
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = vec![units.len() as u8, u8::from(wide)];
    for u in units {
        if wide {
            out.extend_from_slice(&u.to_le_bytes());
        } else {
            out.push(u as u8);
        }
    }
    out
}

fn long(text: &str) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = (units.len() as u16).to_le_bytes().to_vec();
    out.push(1);
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn bof(dt: u16) -> Vec<u8> {
    let mut d = 0x0600u16.to_le_bytes().to_vec();
    d.extend_from_slice(&dt.to_le_bytes());
    d.extend_from_slice(&[0; 12]);
    rec(0x0809, &d)
}

fn font(height: u16, bold: bool, name: &str) -> Vec<u8> {
    let mut d = height.to_le_bytes().to_vec();
    d.extend_from_slice(&[0, 0, 0xFF, 0x7F]);
    d.extend_from_slice(&(if bold { 700u16 } else { 400 }).to_le_bytes());
    d.extend_from_slice(&[0, 0, 0, 0, 134, 0]);
    d.extend_from_slice(&short(name));
    rec(0x0031, &d)
}

/// An XF: font, number format, alignment byte, and a solid fill colour index (0 = none).
fn xf(font: u16, fmt: u16, align: u8, fill: u16, cell: bool) -> Vec<u8> {
    let mut d = font.to_le_bytes().to_vec();
    d.extend_from_slice(&fmt.to_le_bytes());
    d.extend_from_slice(&(if cell { 0x0001u16 } else { 0xFFF5 }).to_le_bytes());
    d.extend_from_slice(&[align, 0, 0, 0]);
    // Thin borders all round, black.
    d.extend_from_slice(&0x1111u16.to_le_bytes());
    d.extend_from_slice(&((8u16) | (8 << 7)).to_le_bytes());
    let fls: u32 = if fill > 0 { 1 } else { 0 };
    d.extend_from_slice(&((8u32) | (8 << 7) | (fls << 26)).to_le_bytes());
    d.extend_from_slice(&(fill | (0x41 << 7)).to_le_bytes());
    rec(0x00E0, &d)
}

fn cell(kind: u16, row: u16, col: u16, xf: u16, rest: &[u8]) -> Vec<u8> {
    let mut d = row.to_le_bytes().to_vec();
    d.extend_from_slice(&col.to_le_bytes());
    d.extend_from_slice(&xf.to_le_bytes());
    d.extend_from_slice(rest);
    rec(kind, &d)
}

fn formula(row: u16, col: u16, value: f64, rgce: &[u8]) -> Vec<u8> {
    let mut rest = value.to_le_bytes().to_vec();
    rest.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    rest.extend_from_slice(&(rgce.len() as u16).to_le_bytes());
    rest.extend_from_slice(rgce);
    cell(0x0006, row, col, 15, &rest)
}

/// A compound file with one stream, in regular sectors (512 bytes).
pub(crate) fn compound(name: &str, stream: &[u8]) -> Vec<u8> {
    let mut stream = stream.to_vec();
    stream.resize(stream.len().max(4096).div_ceil(512) * 512, 0);
    let sectors = stream.len() / 512;
    assert!(sectors + 2 <= 128);
    let mut b = vec![0u8; 512 * 3];
    b[..8].copy_from_slice(&crate::cfb::SIGNATURE);
    b[24..26].copy_from_slice(&0x3Eu16.to_le_bytes());
    b[26..28].copy_from_slice(&3u16.to_le_bytes());
    b[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    b[30..32].copy_from_slice(&9u16.to_le_bytes());
    b[32..34].copy_from_slice(&6u16.to_le_bytes());
    b[44..48].copy_from_slice(&1u32.to_le_bytes());
    b[48..52].copy_from_slice(&1u32.to_le_bytes());
    b[56..60].copy_from_slice(&4096u32.to_le_bytes());
    b[60..64].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    b[68..72].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    for i in 0..109 {
        b[76 + i * 4..80 + i * 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }
    b[76..80].copy_from_slice(&0u32.to_le_bytes());
    let fat = 512;
    for i in 0..128 {
        b[fat + i * 4..fat + i * 4 + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }
    b[fat..fat + 4].copy_from_slice(&0xFFFF_FFFDu32.to_le_bytes());
    b[fat + 4..fat + 8].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    for s in 0..sectors {
        let next = if s + 1 == sectors { 0xFFFF_FFFE } else { (s + 3) as u32 };
        b[fat + (s + 2) * 4..fat + (s + 2) * 4 + 4].copy_from_slice(&next.to_le_bytes());
    }
    for (k, (entry, kind, start, size)) in [("Root Entry", 5u8, 0xFFFF_FFFEu32, 0u32), (name, 2, 2, stream.len() as u32)].into_iter().enumerate() {
        let e = 1024 + k * 128;
        let units: Vec<u16> = entry.encode_utf16().collect();
        for (i, u) in units.iter().enumerate() {
            b[e + i * 2..e + i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        b[e + 64..e + 66].copy_from_slice(&((units.len() as u16 + 1) * 2).to_le_bytes());
        b[e + 66] = kind;
        for at in [68, 72, 76] {
            b[e + at..e + at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        }
        if k == 0 {
            b[e + 76..e + 80].copy_from_slice(&1u32.to_le_bytes());
        }
        b[e + 116..e + 120].copy_from_slice(&start.to_le_bytes());
        b[e + 120..e + 124].copy_from_slice(&size.to_le_bytes());
    }
    b.extend_from_slice(&stream);
    b
}

/// A workbook of two sheets ("数据" and "Other"), with styles, strings split across a
/// CONTINUE record, numbers, formulas (one shared), a merge, frozen panes and a link.
fn sample() -> Vec<u8> {
    let mut globals = bof(0x0005);
    globals.extend(rec(0x0042, &1200u16.to_le_bytes()));
    globals.extend(rec(0x003D, &[0, 0, 0, 0, 0, 0, 0, 0, 0x38, 0, 1, 0, 0, 0, 1, 0, 0x58, 2]));
    globals.extend(font(220, false, "宋体"));
    globals.extend(font(220, true, "Arial"));
    globals.extend(font(220, false, "Arial"));
    globals.extend(font(220, false, "Arial"));
    globals.extend(font(320, true, "黑体"));
    let mut fmt = 164u16.to_le_bytes().to_vec();
    fmt.extend(long("0.0%"));
    globals.extend(rec(0x041E, &fmt));
    for _ in 0..15 {
        globals.extend(xf(0, 0, 0x20, 0, false));
    }
    globals.extend(xf(0, 0, 0x20, 0, true)); // 15: default cell format
    globals.extend(xf(1, 0, 0x2A, 13, true)); // 16: bold Arial, centred, wrapped, yellow fill
    globals.extend(xf(5, 164, 0x20, 0, true)); // 17: 黑体 16 (the fifth font record is font 5), 0.0%
    let sheet_at = globals.len();
    let mut bs = 0u32.to_le_bytes().to_vec();
    bs.extend_from_slice(&[0, 0]);
    bs.extend(short("数据"));
    globals.extend(rec(0x0085, &bs));
    let mut bs2 = 0u32.to_le_bytes().to_vec();
    bs2.extend_from_slice(&[1, 0]);
    bs2.extend(short("Other"));
    globals.extend(rec(0x0085, &bs2));
    globals.extend(rec(0x01AE, &[2, 0, 1, 4]));
    let mut xti = 2u16.to_le_bytes().to_vec();
    xti.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0]);
    globals.extend(rec(0x0017, &xti));
    // NAME: Total = 数据!$B$1:$B$3 (Area3d to sheet 0).
    let mut name = vec![0, 0, 0, 5];
    let rgce = [0x3B, 0, 0, 0, 0, 2, 0, 1, 0, 1, 0];
    name.extend_from_slice(&(rgce.len() as u16).to_le_bytes());
    name.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0]);
    name.extend_from_slice(b"Total");
    name.extend_from_slice(&rgce);
    globals.extend(rec(0x0018, &name));
    // SST: "标题", "第一行文字" split across a CONTINUE (compressed → wide), "Hello".
    let strings = ["标题", "第一行文字", "Hello"];
    let mut sst = 3u32.to_le_bytes().to_vec();
    sst.extend_from_slice(&3u32.to_le_bytes());
    sst.extend_from_slice(&2u16.to_le_bytes());
    sst.push(1);
    for u in strings[0].encode_utf16() {
        sst.extend_from_slice(&u.to_le_bytes());
    }
    sst.extend_from_slice(&5u16.to_le_bytes());
    sst.push(1);
    let second: Vec<u16> = strings[1].encode_utf16().collect();
    for u in &second[..2] {
        sst.extend_from_slice(&u.to_le_bytes());
    }
    globals.extend(rec(0x00FC, &sst));
    let mut cont = vec![1u8];
    for u in &second[2..] {
        cont.extend_from_slice(&u.to_le_bytes());
    }
    cont.extend_from_slice(&5u16.to_le_bytes());
    cont.push(0);
    cont.extend_from_slice(b"Hello");
    globals.extend(rec(0x003C, &cont));
    globals.extend(rec(0x000A, &[]));

    let mut sheet = bof(0x0010);
    sheet.extend(rec(0x0225, &[0, 0, 0x2C, 0x01]));
    let mut colinfo = 0u16.to_le_bytes().to_vec();
    colinfo.extend_from_slice(&1u16.to_le_bytes());
    colinfo.extend_from_slice(&(20u16 * 256).to_le_bytes());
    colinfo.extend_from_slice(&15u16.to_le_bytes());
    colinfo.extend_from_slice(&[0, 0, 0, 0]);
    sheet.extend(rec(0x007D, &colinfo));
    let mut row = 0u16.to_le_bytes().to_vec();
    row.extend_from_slice(&[0, 0, 3, 0]);
    row.extend_from_slice(&(600u16).to_le_bytes());
    row.extend_from_slice(&[0, 0, 0, 0]);
    row.extend_from_slice(&0x0140u16.to_le_bytes());
    row.extend_from_slice(&15u16.to_le_bytes());
    sheet.extend(rec(0x0208, &row));
    sheet.extend(cell(0x00FD, 0, 0, 16, &0u32.to_le_bytes()));
    sheet.extend(cell(0x00FD, 1, 0, 15, &1u32.to_le_bytes()));
    sheet.extend(cell(0x0203, 0, 1, 15, &1.5f64.to_le_bytes()));
    // RK: the integer 2 (2 << 2 | fInt).
    sheet.extend(cell(0x027E, 1, 1, 17, &((2u32 << 2) | 2).to_le_bytes()));
    // MULRK: B3 = 0.25 ÷ 100 and C3 = 7.
    let mut mulrk = 2u16.to_le_bytes().to_vec();
    mulrk.extend_from_slice(&1u16.to_le_bytes());
    mulrk.extend_from_slice(&15u16.to_le_bytes());
    mulrk.extend_from_slice(&((0.25f64.to_bits() >> 32) as u32 | 1).to_le_bytes());
    mulrk.extend_from_slice(&15u16.to_le_bytes());
    mulrk.extend_from_slice(&((7u32 << 2) | 2).to_le_bytes());
    mulrk.extend_from_slice(&2u16.to_le_bytes());
    sheet.extend(rec(0x00BD, &mulrk));
    sheet.extend(cell(0x0205, 3, 0, 15, &[1, 0]));
    sheet.extend(cell(0x0205, 3, 1, 15, &[0x07, 1]));
    // D1 = SUM(B1:B3) via tAttrSum; D2 = IF(B1>1,"big","small") with a string result.
    sheet.extend(formula(0, 3, 3.75, &[0x25, 0, 0, 2, 0, 1, 0xC0, 1, 0xC0, 0x19, 0x10, 0, 0]));
    let mut if_rgce = vec![0x24, 0, 0, 1, 0xC0, 0x1E, 1, 0, 0x0D, 0x17, 3, 0];
    if_rgce.extend_from_slice(b"big");
    if_rgce.extend_from_slice(&[0x17, 5, 0]);
    if_rgce.extend_from_slice(b"small");
    if_rgce.extend_from_slice(&[0x42, 3, 1, 0]);
    let mut d2 = vec![0u8, 0, 0, 0, 0, 0, 0xFF, 0xFF];
    d2.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    d2.extend_from_slice(&(if_rgce.len() as u16).to_le_bytes());
    d2.extend_from_slice(&if_rgce);
    sheet.extend(cell(0x0006, 1, 3, 15, &d2));
    sheet.extend(rec(0x0207, &long("big")));
    // E1:E2 = B1*2, B2*2: a shared formula (RefN B1 relative), anchored at E1.
    sheet.extend(formula(0, 4, 3.0, &[0x01, 0, 0, 4, 0]));
    let mut shr = vec![0, 0, 1, 0, 4, 4, 0, 2];
    let shared = [0x2C, 0, 0, 0xFD, 0xC0, 0x1E, 2, 0, 0x05];
    shr.extend_from_slice(&(shared.len() as u16).to_le_bytes());
    shr.extend_from_slice(&shared);
    sheet.extend(rec(0x04BC, &shr));
    sheet.extend(formula(1, 4, 4.0, &[0x01, 0, 0, 4, 0]));
    // F1 = Other!A1 + Total
    sheet.extend(formula(0, 5, 0.0, &[0x3A, 1, 0, 0, 0, 0, 0xC0, 0x23, 1, 0, 0, 0, 0x03]));
    let mut merge = 1u16.to_le_bytes().to_vec();
    merge.extend_from_slice(&[5, 0, 6, 0, 0, 0, 1, 0]);
    sheet.extend(rec(0x00E5, &merge));
    sheet.extend(rec(0x023E, &[0x0C, 0x02, 0, 0, 0, 0, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    sheet.extend(rec(0x0041, &[1, 0, 1, 0, 1, 0, 1, 0, 3, 0]));
    // A link on A2 to a URL.
    let mut link = vec![1, 0, 1, 0, 0, 0, 0, 0];
    link.extend_from_slice(&[0xD0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B]);
    link.extend_from_slice(&2u32.to_le_bytes());
    link.extend_from_slice(&0x03u32.to_le_bytes());
    link.extend_from_slice(&[0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B]);
    let url: Vec<u8> = "https://example.com/\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
    link.extend_from_slice(&(url.len() as u32).to_le_bytes());
    link.extend_from_slice(&url);
    sheet.extend(rec(0x01B8, &link));
    sheet.extend(rec(0x000A, &[]));

    let mut other = bof(0x0010);
    other.extend(cell(0x0203, 0, 0, 15, &10.0f64.to_le_bytes()));
    other.extend(rec(0x000A, &[]));

    let mut stream = globals;
    let first = stream.len() as u32;
    stream.extend(sheet);
    let second = stream.len() as u32;
    stream.extend(other);
    // Patch the sheets' stream positions.
    let at = sheet_at + 4;
    stream[at..at + 4].copy_from_slice(&first.to_le_bytes());
    let at2 = at + 4 + 4 + 2 + short("数据").len();
    stream[at2..at2 + 4].copy_from_slice(&second.to_le_bytes());
    compound("Workbook", &stream)
}

fn text(sheet: &Sheet, a1: &str) -> String {
    match sheet.cells.get(CellRef::parse(a1).unwrap()).map(|c| c.value.clone()) {
        Some(Value::Text(t)) => t.to_string(),
        other => format!("{other:?}"),
    }
}

fn number(sheet: &Sheet, a1: &str) -> f64 {
    match sheet.cells.get(CellRef::parse(a1).unwrap()).map(|c| c.value.clone()) {
        Some(Value::Number(n)) => n,
        other => panic!("{a1}: {other:?}"),
    }
}

fn formula_text(sheet: &Sheet, a1: &str) -> Option<String> {
    sheet.cells.get(CellRef::parse(a1).unwrap()).and_then(|c| c.formula.as_ref().map(|f| f.text.clone()))
}

#[test]
fn reads_values_strings_and_sheets() {
    let bytes = sample();
    assert!(is_xls(&bytes));
    assert_eq!(crate::sniff(&bytes), crate::Format::Xls);
    let (wb, report) = read_xls(&bytes).unwrap();
    assert_eq!(wb.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["数据", "Other"]);
    assert_eq!(wb.sheets[1].visibility, Visibility::Hidden);
    let s = &wb.sheets[0];
    assert_eq!(text(s, "A1"), "标题");
    // A string split across a CONTINUE record, its width restated there.
    assert_eq!(text(s, "A2"), "第一行文字");
    assert_eq!(number(s, "B1"), 1.5);
    assert_eq!(number(s, "B2"), 2.0);
    assert_eq!(number(s, "B3"), 0.0025);
    assert_eq!(number(s, "C3"), 7.0);
    assert_eq!(s.cells.get(CellRef::parse("A4").unwrap()).map(|c| c.value.clone()), Some(Value::Bool(true)));
    assert_eq!(s.cells.get(CellRef::parse("B4").unwrap()).map(|c| c.value.clone()), Some(Value::Error(CellError::Div0)));
    assert_eq!(number(&wb.sheets[1], "A1"), 10.0);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn reads_formulas_with_their_results() {
    let (wb, _) = read_xls(&sample()).unwrap();
    let s = &wb.sheets[0];
    assert_eq!(formula_text(s, "D1").as_deref(), Some("SUM(B1:B3)"));
    assert_eq!(number(s, "D1"), 3.75);
    assert_eq!(formula_text(s, "D2").as_deref(), Some("IF(B1>1,\"big\",\"small\")"));
    assert_eq!(text(s, "D2"), "big");
    // The shared formula, relative to each of its cells.
    assert_eq!(formula_text(s, "E1").as_deref(), Some("B1*2"));
    assert_eq!(formula_text(s, "E2").as_deref(), Some("B2*2"));
    assert_eq!(formula_text(s, "F1").as_deref(), Some("Other!A1+Total"));
    assert_eq!(wb.names.iter().map(|n| (n.name.as_str(), n.formula.as_str())).collect::<Vec<_>>(), [("Total", "数据!$B$1:$B$3")]);
}

#[test]
fn reads_styles_sizes_merges_panes_and_links() {
    let (wb, _) = read_xls(&sample()).unwrap();
    let s = &wb.sheets[0];
    let st = wb.styles.get(s.cells.get(CellRef::parse("A1").unwrap()).unwrap().style);
    assert!(st.font.bold && st.font.name == "Arial");
    assert_eq!(st.align.h, HAlign::Center);
    assert!(st.align.wrap);
    assert_eq!(st.fill.pattern, PatternType::Solid);
    assert_eq!(st.fill.fg, Color::Rgb(0xFFFF00));
    assert_eq!(st.borders.left.style, BorderStyle::Thin);
    let pct = wb.styles.get(s.cells.get(CellRef::parse("B2").unwrap()).unwrap().style);
    assert_eq!(pct.num_fmt.as_str(), "0.0%");
    assert_eq!(pct.font.name, "黑体");
    assert_eq!(pct.font.size, 16.0);
    // The default font is 宋体 11: column widths in its character unit.
    let unit = kition_sheet_model::units::char_width_px("宋体", 11.0);
    assert_eq!(s.col_width(0), crate::tables::col_width_to_px(20.0, unit));
    assert_eq!(s.rows.get(&0).and_then(|r| r.size), Some(40.0));
    assert!(s.rows.get(&0).is_some_and(|r| r.custom));
    assert_eq!(s.merges, vec![RangeRef::parse("A6:B7").unwrap()]);
    assert_eq!(s.freeze, Some((1, 1)));
    assert!(!s.show_gridlines);
    assert_eq!(s.hyperlinks.get(&CellRef::parse("A2").unwrap()).map(|h| h.target.as_str()), Some("https://example.com/"));
}

/// Damaged files give an error or what could be read, never a crash.
#[test]
fn hostile_files_never_panic() {
    let bytes = sample();
    assert!(read_xls(&bytes[..600]).is_err());
    for cut in (0..bytes.len()).step_by(97) {
        let _ = read_xls(&bytes[..cut]);
    }
    for at in (1536..bytes.len().min(1536 + 1400)).step_by(7) {
        for v in [0u8, 0xFF, 0x7F] {
            let mut b = bytes.clone();
            b[at] = v;
            let _ = read_xls(&b);
        }
    }
    // A compound file without a workbook stream is not a spreadsheet.
    assert!(read_xls(&compound("WordDocument", &[0; 100])).is_err());
    assert!(!is_xls(&compound("WordDocument", &[0; 100])));
}
