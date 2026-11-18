//! Excel 97-2003 workbooks (`.xls`, BIFF8), read from Microsoft's public [MS-XLS] specification:
//! <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xls/>.
//!
//! The `Workbook` stream of the compound file is a sequence of records: the workbook globals
//! (fonts, number formats, cell formats, the palette, shared strings, sheets, names, links), then
//! one substream per sheet (rows, columns, cells, merged ranges, hyperlinks, notes, the window
//! and the panes). Cell values, formulas (decoded from their tokens back to text), styles,
//! column widths, row heights, merges, frozen panes, hidden rows, columns and sheets, hyperlinks,
//! notes and defined names are read; charts, pictures, data validation, conditional formats and
//! macros are not (a warning says so). Every read is bounds-checked and every loop bounded: a
//! damaged file gives an error or what could be read, never a crash.

use std::collections::HashMap;
use std::sync::Arc;

use kition_sheet_core::{CellError, CellRef, DateSystem, MAX_COLS, MAX_ROWS, RangeRef, Value};
use kition_sheet_model::{
    Alignment, BorderLine, BorderStyle, Borders, Cell, Color, Comment, DefinedName, Fill, Font, Formula, HAlign, Hyperlink, LineInfo, NumFmt,
    PatternType, Sheet, Style, StyleId, Underline, VAlign, VertAlign, Visibility, Workbook,
};

use crate::{IoError, ReadReport};

const MAX_CELLS: usize = 5_000_000;
const MAX_STRINGS: usize = 2_000_000;
const MAX_TEXT: usize = 128 * 1024 * 1024;
const MAX_SHEETS: usize = 1024;
const MAX_RECORDS: usize = 20_000_000;
/// Most tokens one formula may have (bounds the decoder on hostile input).
const MAX_TOKENS: usize = 10_000;

fn invalid(msg: impl Into<String>) -> IoError {
    IoError::Format(format!("XLS: {}", msg.into()))
}

fn u8_at(d: &[u8], at: usize) -> Option<u8> {
    d.get(at).copied()
}
fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*d.get(at)?, *d.get(at + 1)?]))
}
fn i16_at(d: &[u8], at: usize) -> Option<i16> {
    u16_at(d, at).map(|v| v as i16)
}
fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*d.get(at)?, *d.get(at + 1)?, *d.get(at + 2)?, *d.get(at + 3)?]))
}
fn f64_at(d: &[u8], at: usize) -> Option<f64> {
    let b: [u8; 8] = d.get(at..at + 8)?.try_into().ok()?;
    Some(f64::from_le_bytes(b))
}

/// One record, with its CONTINUE records appended; `breaks` are where each continuation began
/// (strings split across records restate their character width there).
struct Record {
    kind: u16,
    data: Vec<u8>,
    breaks: Vec<usize>,
}

/// The records from `at` in the stream, until the substream's EOF (or the end of the stream).
fn records(stream: &[u8], mut at: usize) -> Vec<Record> {
    let mut out: Vec<Record> = Vec::new();
    while out.len() < MAX_RECORDS {
        let (Some(kind), Some(len)) = (u16_at(stream, at), u16_at(stream, at + 2)) else { break };
        let len = usize::from(len);
        let Some(data) = stream.get(at + 4..at + 4 + len) else { break };
        at += 4 + len;
        if kind == 0x003C
            && let Some(last) = out.last_mut()
        {
            last.breaks.push(last.data.len());
            last.data.extend_from_slice(data);
            continue;
        }
        out.push(Record { kind, data: data.to_vec(), breaks: Vec::new() });
        if kind == 0x000A {
            break;
        }
    }
    out
}

/// Reads strings from a record whose continuations restate the character width.
struct Strings<'a> {
    data: &'a [u8],
    breaks: &'a [usize],
    at: usize,
}

impl Strings<'_> {
    fn u8(&mut self) -> Option<u8> {
        let v = u8_at(self.data, self.at)?;
        self.at += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        let v = u16_at(self.data, self.at)?;
        self.at += 2;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        let v = u32_at(self.data, self.at)?;
        self.at += 4;
        Some(v)
    }
    /// `count` characters, one or two bytes each; at a continuation the width is restated.
    fn chars(&mut self, count: usize, mut high: bool) -> Option<String> {
        let mut units: Vec<u16> = Vec::with_capacity(count.min(65_536));
        for _ in 0..count {
            if self.breaks.contains(&self.at) && !units.is_empty() {
                high = self.u8()? & 1 != 0;
            }
            if high {
                units.push(self.u16()?);
            } else {
                units.push(u16::from(self.u8()?));
            }
        }
        Some(String::from_utf16_lossy(&units))
    }
    /// An XLUnicodeRichExtendedString (the shared string table's items).
    fn rich_string(&mut self) -> Option<String> {
        let count = usize::from(self.u16()?);
        let flags = self.u8()?;
        let runs = if flags & 0x08 != 0 { usize::from(self.u16()?) } else { 0 };
        let ext = if flags & 0x04 != 0 { self.u32()? as usize } else { 0 };
        let text = self.chars(count, flags & 1 != 0)?;
        self.at = self.at.checked_add(runs * 4)?.checked_add(ext)?;
        Some(text)
    }
}

/// A ShortXLUnicodeString (one-byte length) at `at`; answers the text and the bytes it took.
fn short_string(d: &[u8], at: usize) -> Option<(String, usize)> {
    let count = usize::from(u8_at(d, at)?);
    let high = u8_at(d, at + 1)? & 1 != 0;
    let mut s = Strings { data: d, breaks: &[], at: at + 2 };
    let text = s.chars(count, high)?;
    Some((text, s.at - at))
}

/// An XLUnicodeString (two-byte length) at `at`.
fn long_string(d: &[u8], at: usize) -> Option<(String, usize)> {
    let count = usize::from(u16_at(d, at)?);
    let high = u8_at(d, at + 2)? & 1 != 0;
    let mut s = Strings { data: d, breaks: &[], at: at + 3 };
    let text = s.chars(count, high)?;
    Some((text, s.at - at))
}

/// An RK number: an integer or the top 30 bits of a double, optionally ÷ 100.
fn rk(v: u32) -> f64 {
    let n = if v & 2 != 0 { f64::from((v as i32) >> 2) } else { f64::from_bits(u64::from(v & 0xFFFF_FFFC) << 32) };
    if v & 1 != 0 { n / 100.0 } else { n }
}

fn error(code: u8) -> CellError {
    match code {
        0x00 => CellError::Null,
        0x07 => CellError::Div0,
        0x0F => CellError::Value,
        0x17 => CellError::Ref,
        0x1D => CellError::Name,
        0x24 => CellError::Num,
        _ => CellError::NA,
    }
}

fn error_text(code: u8) -> &'static str {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        _ => "#N/A",
    }
}

/// A font as the FONT record has it.
struct XlsFont {
    font: Font,
}

/// A cell format (XF) decoded to a model style.
struct Globals {
    fonts: Vec<XlsFont>,
    formats: HashMap<u16, String>,
    xfs: Vec<Style>,
    palette: Vec<u32>,
    strings: Vec<Arc<str>>,
    sheets: Vec<(String, usize, Visibility)>,
    names: Vec<(String, Option<usize>, Vec<u8>, bool)>,
    /// EXTERNSHEET entries: (supporting link, first sheet, last sheet).
    xti: Vec<(u16, i16, i16)>,
    /// Supporting links: whether each is this workbook, and the names an add-in link defines.
    supbooks: Vec<(bool, Vec<String>)>,
    date_1904: bool,
    active: usize,
}

fn color(icv: u16, palette: &[u32]) -> Color {
    match icv {
        0..=63 => palette.get(usize::from(icv)).map_or(Color::Auto, |c| Color::Rgb(*c)),
        _ => Color::Auto,
    }
}

fn border(style: u16, icv: u16, palette: &[u32]) -> BorderLine {
    let style = match style & 0xF {
        1 => BorderStyle::Thin,
        2 => BorderStyle::Medium,
        3 => BorderStyle::Dashed,
        4 => BorderStyle::Dotted,
        5 => BorderStyle::Thick,
        6 => BorderStyle::Double,
        7 => BorderStyle::Hair,
        8 => BorderStyle::MediumDashed,
        9 => BorderStyle::DashDot,
        10 => BorderStyle::MediumDashDot,
        11 => BorderStyle::DashDotDot,
        12 => BorderStyle::MediumDashDotDot,
        13 => BorderStyle::SlantDashDot,
        _ => return BorderLine::default(),
    };
    BorderLine { style, color: color(icv, palette) }
}

fn pattern(fls: u32) -> PatternType {
    match fls {
        0 => PatternType::None,
        1 => PatternType::Solid,
        2 => PatternType::Gray50,
        3 => PatternType::Gray75,
        4 => PatternType::Gray25,
        5 => PatternType::DarkHorizontal,
        6 => PatternType::DarkVertical,
        7 => PatternType::DarkDown,
        8 => PatternType::DarkUp,
        9 => PatternType::DarkGrid,
        10 => PatternType::DarkTrellis,
        11 => PatternType::LightHorizontal,
        12 => PatternType::LightVertical,
        13 => PatternType::LightDown,
        14 => PatternType::LightUp,
        15 => PatternType::LightGrid,
        16 => PatternType::LightTrellis,
        17 => PatternType::Gray125,
        18 => PatternType::Gray0625,
        _ => PatternType::None,
    }
}

/// The FONT record (2.4.122).
fn read_font(d: &[u8], palette: &[u32]) -> Option<XlsFont> {
    let height = u16_at(d, 0)?;
    let flags = u16_at(d, 2)?;
    let icv = u16_at(d, 4)?;
    let weight = u16_at(d, 6)?;
    let script = u16_at(d, 8)?;
    let underline = u8_at(d, 10)?;
    let charset = u8_at(d, 12)?;
    let (name, _) = short_string(d, 14)?;
    Some(XlsFont {
        font: Font {
            name,
            size: (f32::from(height) / 20.0).clamp(1.0, 409.0),
            bold: weight >= 700,
            italic: flags & 0x02 != 0,
            strike: flags & 0x08 != 0,
            underline: match underline {
                1 => Underline::Single,
                2 => Underline::Double,
                0x21 => Underline::SingleAccounting,
                0x22 => Underline::DoubleAccounting,
                _ => Underline::None,
            },
            vert: match script {
                1 => VertAlign::Superscript,
                2 => VertAlign::Subscript,
                _ => VertAlign::Baseline,
            },
            color: if icv == 0x7FFF || icv == 8 { Color::Auto } else { color(icv, palette) },
            charset: (charset != 0 && charset != 1).then_some(u32::from(charset)),
            scheme: None,
        },
    })
}

/// The XF record (2.4.353): a cell's font, number format, alignment, borders and fill.
fn read_xf(d: &[u8], g: &Globals) -> Option<Style> {
    let ifnt = usize::from(u16_at(d, 0)?);
    let ifmt = u16_at(d, 2)?;
    let flags = u16_at(d, 4)?;
    let align = u8_at(d, 6)?;
    let trot = u8_at(d, 7)?;
    let indent = u8_at(d, 8)?;
    let lines = u16_at(d, 10)?;
    let colors = u16_at(d, 12)?;
    let more = u32_at(d, 14)?;
    let fill = u16_at(d, 18)?;
    // Font index 4 does not exist in BIFF: the fourth font record is font 5.
    let font_index = if ifnt >= 4 { ifnt - 1 } else { ifnt };
    let font = g.fonts.get(font_index).map(|f| f.font.clone()).unwrap_or_default();
    let code = g
        .formats
        .get(&ifmt)
        .cloned()
        .or_else(|| crate::tables::builtin_format(u32::from(ifmt)).map(str::to_owned))
        .unwrap_or_else(|| "General".into());
    let rotation = match trot {
        0..=90 => i16::from(trot),
        91..=180 => 90 - i16::from(trot),
        255 => 255,
        _ => 0,
    };
    let p = &g.palette;
    let fls = (more >> 26) & 0x3F;
    let fore = fill & 0x7F;
    let back = (fill >> 7) & 0x7F;
    Some(Style {
        font,
        fill: Fill {
            pattern: pattern(fls),
            fg: if fls == 0 { Color::Auto } else { color(fore, p) },
            bg: if fls <= 1 || back == 0x41 || back >= 64 { Color::Auto } else { color(back, p) },
        },
        borders: Borders {
            left: border(lines, colors & 0x7F, p),
            right: border(lines >> 4, (colors >> 7) & 0x7F, p),
            top: border(lines >> 8, (more & 0x7F) as u16, p),
            bottom: border(lines >> 12, ((more >> 7) & 0x7F) as u16, p),
            diag_down: if colors & 0x4000 != 0 {
                border(((more >> 21) & 0xF) as u16, ((more >> 14) & 0x7F) as u16, p)
            } else {
                BorderLine::default()
            },
            diag_up: if colors & 0x8000 != 0 { border(((more >> 21) & 0xF) as u16, ((more >> 14) & 0x7F) as u16, p) } else { BorderLine::default() },
            ..Borders::default()
        },
        align: Alignment {
            h: match align & 0x7 {
                1 => HAlign::Left,
                2 => HAlign::Center,
                3 => HAlign::Right,
                4 => HAlign::Fill,
                5 => HAlign::Justify,
                6 => HAlign::CenterAcross,
                7 => HAlign::Distributed,
                _ => HAlign::General,
            },
            v: match (align >> 4) & 0x7 {
                0 => VAlign::Top,
                1 => VAlign::Center,
                3 => VAlign::Justify,
                4 => VAlign::Distributed,
                _ => VAlign::Bottom,
            },
            wrap: align & 0x08 != 0,
            shrink: indent & 0x10 != 0,
            indent: indent & 0x0F,
            rotation,
        },
        num_fmt: NumFmt::new(&code),
        protection: kition_sheet_model::style::Protection { locked: flags & 1 != 0, hidden: flags & 2 != 0 },
        checkbox: false,
    })
}

/// Reads the workbook globals substream.
fn read_globals(recs: &[Record], report: &mut ReadReport) -> Result<Globals, IoError> {
    let mut g = Globals {
        fonts: Vec::new(),
        formats: HashMap::new(),
        xfs: Vec::new(),
        palette: crate::tables::INDEXED.to_vec(),
        strings: Vec::new(),
        sheets: Vec::new(),
        names: Vec::new(),
        xti: Vec::new(),
        supbooks: Vec::new(),
        date_1904: false,
        active: 0,
    };
    // The palette comes before the formats in the file, but fonts may come before it: keep the
    // raw records and decode once everything is read.
    let mut font_recs = Vec::new();
    let mut xf_recs = Vec::new();
    let mut text = 0usize;
    for r in recs {
        let d = &r.data;
        match r.kind {
            0x0809 if u16_at(d, 0).is_some_and(|v| v != 0x0600) => {
                return Err(invalid("only Excel 97-2003 (BIFF8) workbooks are read; this is an older Excel 5/95 file"));
            }
            0x0022 => g.date_1904 = u16_at(d, 0) == Some(1),
            0x0031 => font_recs.push(d.clone()),
            0x041E => {
                if let (Some(id), Some((code, _))) = (u16_at(d, 0), long_string(d, 2)) {
                    g.formats.insert(id, code);
                }
            }
            0x00E0 => xf_recs.push(d.clone()),
            0x0092 => {
                let count = usize::from(u16_at(d, 0).unwrap_or(0)).min(56);
                for i in 0..count {
                    let at = 2 + i * 4;
                    if let (Some(r), Some(gr), Some(b)) = (u8_at(d, at), u8_at(d, at + 1), u8_at(d, at + 2))
                        && let Some(slot) = g.palette.get_mut(8 + i)
                    {
                        *slot = (u32::from(r) << 16) | (u32::from(gr) << 8) | u32::from(b);
                    }
                }
            }
            0x003D => g.active = usize::from(u16_at(d, 10).unwrap_or(0)),
            0x0085 => {
                if g.sheets.len() >= MAX_SHEETS {
                    continue;
                }
                let (Some(pos), Some(state), Some(kind)) = (u32_at(d, 0), u8_at(d, 4), u8_at(d, 5)) else { continue };
                let Some((name, _)) = short_string(d, 6) else { continue };
                if kind != 0 {
                    report.warn(format!("sheet \"{name}\" is a chart or macro sheet and was skipped"));
                    continue;
                }
                let vis = match state & 3 {
                    1 => Visibility::Hidden,
                    2 => Visibility::VeryHidden,
                    _ => Visibility::Visible,
                };
                g.sheets.push((name, pos as usize, vis));
            }
            0x00FC => {
                let unique = u32_at(d, 4).unwrap_or(0) as usize;
                let mut s = Strings { data: d, breaks: &r.breaks, at: 8 };
                for _ in 0..unique.min(MAX_STRINGS) {
                    let Some(item) = s.rich_string() else { break };
                    text = text.saturating_add(item.len());
                    if text > MAX_TEXT {
                        return Err(IoError::TooLarge("XLS shared strings exceed 128 MiB".into()));
                    }
                    g.strings.push(Arc::from(item.as_str()));
                }
                if g.strings.len() < unique.min(MAX_STRINGS) {
                    report.warn(format!("the shared string table is damaged; kept {} of {unique} strings", g.strings.len()));
                }
            }
            0x01AE => {
                // SUPBOOK: this workbook (virtPath 0x0401), an add-in (0x3A01) or another file.
                let marker = u16_at(d, 2).unwrap_or(0);
                g.supbooks.push((marker == 0x0401, Vec::new()));
            }
            0x0023 => {
                // EXTERNNAME: an add-in function's name (IFERROR and the other newer functions
                // an older format stores as add-in names).
                if let Some((name, _)) = short_string(d, 6)
                    && let Some(book) = g.supbooks.last_mut()
                {
                    book.1.push(name.strip_prefix("_xlfn.").unwrap_or(&name).to_owned());
                }
            }
            0x0017 => {
                let count = usize::from(u16_at(d, 0).unwrap_or(0));
                for i in 0..count.min(65_535) {
                    let at = 2 + i * 6;
                    let (Some(book), Some(first), Some(last)) = (u16_at(d, at), i16_at(d, at + 2), i16_at(d, at + 4)) else { break };
                    g.xti.push((book, first, last));
                }
            }
            0x0018 => {
                let (Some(flags), Some(len), Some(cce), Some(itab)) = (u16_at(d, 0), u8_at(d, 3), u16_at(d, 4), u16_at(d, 8)) else { continue };
                let high = u8_at(d, 14).unwrap_or(0) & 1 != 0;
                let mut s = Strings { data: d, breaks: &[], at: 15 };
                let Some(mut name) = s.chars(usize::from(len), high) else { continue };
                if flags & 0x20 != 0 {
                    name = match name.chars().next().map(u32::from) {
                        Some(0x06) => "_xlnm.Print_Area".into(),
                        Some(0x07) => "_xlnm.Print_Titles".into(),
                        Some(0x0D) => "_xlnm._FilterDatabase".into(),
                        _ => continue,
                    };
                }
                let rgce = d.get(s.at..s.at + usize::from(cce)).unwrap_or_default().to_vec();
                g.names.push((name, (itab > 0).then(|| usize::from(itab) - 1), rgce, flags & 1 != 0));
            }
            _ => {}
        }
    }
    g.fonts = font_recs.iter().filter_map(|d| read_font(d, &g.palette)).collect();
    let xfs: Vec<Style> = xf_recs.iter().map(|d| read_xf(d, &g).unwrap_or_default()).collect();
    g.xfs = xfs;
    Ok(g)
}

// ------------------------------------------------------------------ formulas

/// Built-in functions by their index (Ftab, 2.5.198.17): name, and the argument count of the
/// fixed-arity ones (`None` for those that take a variable number).
fn function(index: u16) -> Option<(&'static str, Option<u8>)> {
    let f = match index {
        0 => ("COUNT", None),
        1 => ("IF", None),
        2 => ("ISNA", Some(1)),
        3 => ("ISERROR", Some(1)),
        4 => ("SUM", None),
        5 => ("AVERAGE", None),
        6 => ("MIN", None),
        7 => ("MAX", None),
        8 => ("ROW", None),
        9 => ("COLUMN", None),
        10 => ("NA", Some(0)),
        11 => ("NPV", None),
        12 => ("STDEV", None),
        13 => ("DOLLAR", None),
        14 => ("FIXED", None),
        15 => ("SIN", Some(1)),
        16 => ("COS", Some(1)),
        17 => ("TAN", Some(1)),
        18 => ("ATAN", Some(1)),
        19 => ("PI", Some(0)),
        20 => ("SQRT", Some(1)),
        21 => ("EXP", Some(1)),
        22 => ("LN", Some(1)),
        23 => ("LOG10", Some(1)),
        24 => ("ABS", Some(1)),
        25 => ("INT", Some(1)),
        26 => ("SIGN", Some(1)),
        27 => ("ROUND", Some(2)),
        28 => ("LOOKUP", None),
        29 => ("INDEX", None),
        30 => ("REPT", Some(2)),
        31 => ("MID", Some(3)),
        32 => ("LEN", Some(1)),
        33 => ("VALUE", Some(1)),
        34 => ("TRUE", Some(0)),
        35 => ("FALSE", Some(0)),
        36 => ("AND", None),
        37 => ("OR", None),
        38 => ("NOT", Some(1)),
        39 => ("MOD", Some(2)),
        40 => ("DCOUNT", Some(3)),
        41 => ("DSUM", Some(3)),
        42 => ("DAVERAGE", Some(3)),
        43 => ("DMIN", Some(3)),
        44 => ("DMAX", Some(3)),
        45 => ("DSTDEV", Some(3)),
        46 => ("VAR", None),
        47 => ("DVAR", Some(3)),
        48 => ("TEXT", Some(2)),
        49 => ("LINEST", None),
        50 => ("TREND", None),
        51 => ("LOGEST", None),
        52 => ("GROWTH", None),
        56 => ("PV", None),
        57 => ("FV", None),
        58 => ("NPER", None),
        59 => ("PMT", None),
        60 => ("RATE", None),
        61 => ("MIRR", Some(3)),
        62 => ("IRR", None),
        63 => ("RAND", Some(0)),
        64 => ("MATCH", None),
        65 => ("DATE", Some(3)),
        66 => ("TIME", Some(3)),
        67 => ("DAY", Some(1)),
        68 => ("MONTH", Some(1)),
        69 => ("YEAR", Some(1)),
        70 => ("WEEKDAY", None),
        71 => ("HOUR", Some(1)),
        72 => ("MINUTE", Some(1)),
        73 => ("SECOND", Some(1)),
        74 => ("NOW", Some(0)),
        75 => ("AREAS", Some(1)),
        76 => ("ROWS", Some(1)),
        77 => ("COLUMNS", Some(1)),
        78 => ("OFFSET", None),
        82 => ("SEARCH", None),
        83 => ("TRANSPOSE", Some(1)),
        86 => ("TYPE", Some(1)),
        97 => ("ATAN2", Some(2)),
        98 => ("ASIN", Some(1)),
        99 => ("ACOS", Some(1)),
        100 => ("CHOOSE", None),
        101 => ("HLOOKUP", None),
        102 => ("VLOOKUP", None),
        105 => ("ISREF", Some(1)),
        109 => ("LOG", None),
        111 => ("CHAR", Some(1)),
        112 => ("LOWER", Some(1)),
        113 => ("UPPER", Some(1)),
        114 => ("PROPER", Some(1)),
        115 => ("LEFT", None),
        116 => ("RIGHT", None),
        117 => ("EXACT", Some(2)),
        118 => ("TRIM", Some(1)),
        119 => ("REPLACE", Some(4)),
        120 => ("SUBSTITUTE", None),
        121 => ("CODE", Some(1)),
        124 => ("FIND", None),
        125 => ("CELL", None),
        126 => ("ISERR", Some(1)),
        127 => ("ISTEXT", Some(1)),
        128 => ("ISNUMBER", Some(1)),
        129 => ("ISBLANK", Some(1)),
        130 => ("T", Some(1)),
        131 => ("N", Some(1)),
        140 => ("DATEVALUE", Some(1)),
        141 => ("TIMEVALUE", Some(1)),
        142 => ("SLN", Some(3)),
        143 => ("SYD", Some(4)),
        144 => ("DDB", None),
        148 => ("INDIRECT", None),
        162 => ("CLEAN", Some(1)),
        163 => ("MDETERM", Some(1)),
        164 => ("MINVERSE", Some(1)),
        165 => ("MMULT", Some(2)),
        167 => ("IPMT", None),
        168 => ("PPMT", None),
        169 => ("COUNTA", None),
        183 => ("PRODUCT", None),
        184 => ("FACT", Some(1)),
        189 => ("DPRODUCT", Some(3)),
        190 => ("ISNONTEXT", Some(1)),
        193 => ("STDEVP", None),
        194 => ("VARP", None),
        195 => ("DSTDEVP", Some(3)),
        196 => ("DVARP", Some(3)),
        197 => ("TRUNC", None),
        198 => ("ISLOGICAL", Some(1)),
        199 => ("DCOUNTA", Some(3)),
        204 => ("USDOLLAR", None),
        205 => ("FINDB", None),
        206 => ("SEARCHB", None),
        207 => ("REPLACEB", Some(4)),
        208 => ("LEFTB", None),
        209 => ("RIGHTB", None),
        210 => ("MIDB", Some(3)),
        211 => ("LENB", Some(1)),
        212 => ("ROUNDUP", Some(2)),
        213 => ("ROUNDDOWN", Some(2)),
        214 => ("ASC", Some(1)),
        215 => ("DBCS", Some(1)),
        216 => ("RANK", None),
        219 => ("ADDRESS", None),
        220 => ("DAYS360", None),
        221 => ("TODAY", Some(0)),
        222 => ("VDB", None),
        227 => ("MEDIAN", None),
        228 => ("SUMPRODUCT", None),
        229 => ("SINH", Some(1)),
        230 => ("COSH", Some(1)),
        231 => ("TANH", Some(1)),
        232 => ("ASINH", Some(1)),
        233 => ("ACOSH", Some(1)),
        234 => ("ATANH", Some(1)),
        235 => ("DGET", Some(3)),
        244 => ("INFO", Some(1)),
        247 => ("DB", None),
        252 => ("FREQUENCY", Some(2)),
        261 => ("ERROR.TYPE", Some(1)),
        269 => ("AVEDEV", None),
        270 => ("BETADIST", None),
        271 => ("GAMMALN", Some(1)),
        272 => ("BETAINV", None),
        273 => ("BINOMDIST", Some(4)),
        274 => ("CHIDIST", Some(2)),
        275 => ("CHIINV", Some(2)),
        276 => ("COMBIN", Some(2)),
        277 => ("CONFIDENCE", Some(3)),
        278 => ("CRITBINOM", Some(3)),
        279 => ("EVEN", Some(1)),
        280 => ("EXPONDIST", Some(3)),
        281 => ("FDIST", Some(3)),
        282 => ("FINV", Some(3)),
        283 => ("FISHER", Some(1)),
        284 => ("FISHERINV", Some(1)),
        285 => ("FLOOR", Some(2)),
        286 => ("GAMMADIST", Some(4)),
        287 => ("GAMMAINV", Some(3)),
        288 => ("CEILING", Some(2)),
        289 => ("HYPGEOMDIST", Some(4)),
        290 => ("LOGNORMDIST", Some(3)),
        291 => ("LOGINV", Some(3)),
        292 => ("NEGBINOMDIST", Some(3)),
        293 => ("NORMDIST", Some(4)),
        294 => ("NORMSDIST", Some(1)),
        295 => ("NORMINV", Some(3)),
        296 => ("NORMSINV", Some(1)),
        297 => ("STANDARDIZE", Some(3)),
        298 => ("ODD", Some(1)),
        299 => ("PERMUT", Some(2)),
        300 => ("POISSON", Some(3)),
        301 => ("TDIST", Some(3)),
        302 => ("WEIBULL", Some(4)),
        303 => ("SUMXMY2", Some(2)),
        304 => ("SUMX2MY2", Some(2)),
        305 => ("SUMX2PY2", Some(2)),
        306 => ("CHITEST", Some(2)),
        307 => ("CORREL", Some(2)),
        308 => ("COVAR", Some(2)),
        309 => ("FORECAST", Some(3)),
        310 => ("FTEST", Some(2)),
        311 => ("INTERCEPT", Some(2)),
        312 => ("PEARSON", Some(2)),
        313 => ("RSQ", Some(2)),
        314 => ("STEYX", Some(2)),
        315 => ("SLOPE", Some(2)),
        316 => ("TTEST", Some(4)),
        317 => ("PROB", None),
        318 => ("DEVSQ", None),
        319 => ("GEOMEAN", None),
        320 => ("HARMEAN", None),
        321 => ("SUMSQ", None),
        322 => ("KURT", None),
        323 => ("SKEW", None),
        324 => ("ZTEST", None),
        325 => ("LARGE", Some(2)),
        326 => ("SMALL", Some(2)),
        327 => ("QUARTILE", Some(2)),
        328 => ("PERCENTILE", Some(2)),
        329 => ("PERCENTRANK", None),
        330 => ("MODE", None),
        331 => ("TRIMMEAN", Some(2)),
        332 => ("TINV", Some(2)),
        336 => ("CONCATENATE", None),
        337 => ("POWER", Some(2)),
        342 => ("RADIANS", Some(1)),
        343 => ("DEGREES", Some(1)),
        344 => ("SUBTOTAL", None),
        345 => ("SUMIF", None),
        346 => ("COUNTIF", Some(2)),
        347 => ("COUNTBLANK", Some(1)),
        350 => ("ISPMT", Some(4)),
        351 => ("DATEDIF", Some(3)),
        354 => ("ROMAN", None),
        358 => ("GETPIVOTDATA", None),
        359 => ("HYPERLINK", None),
        360 => ("PHONETIC", Some(1)),
        361 => ("AVERAGEA", None),
        362 => ("MAXA", None),
        363 => ("MINA", None),
        364 => ("STDEVPA", None),
        365 => ("VARPA", None),
        366 => ("STDEVA", None),
        367 => ("VARA", None),
        368 => ("BAHTTEXT", Some(1)),
        _ => return None,
    };
    Some(f)
}

/// Where a formula is (for relative references in shared formulas) and what it may name.
struct FormulaCx<'a> {
    g: &'a Globals,
    row: u32,
    col: u32,
}

fn col_letters(col: u32) -> String {
    let mut s = Vec::new();
    let mut c = col + 1;
    while c > 0 {
        let r = ((c - 1) % 26) as u8;
        s.push(b'A' + r);
        c = (c - 1) / 26;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// A sheet name as a formula writes it (quoted when it needs to be).
fn sheet_prefix(name: &str) -> String {
    let plain = name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if plain { format!("{name}!") } else { format!("'{}'!", name.replace('\'', "''")) }
}

impl FormulaCx<'_> {
    /// A cell reference: `rw`, and the column word with its relative flags (bit 14 column, bit 15 row).
    /// `offsets`: relative parts are offsets from the formula's cell (shared formulas).
    fn cell(&self, rw: u16, colw: u16, offsets: bool) -> Option<String> {
        let (col_rel, row_rel) = (colw & 0x4000 != 0, colw & 0x8000 != 0);
        let row = if offsets && row_rel { i64::from(self.row) + i64::from(rw as i16) } else { i64::from(rw) };
        let col = if offsets && col_rel { i64::from(self.col) + i64::from((colw & 0xFF) as u8 as i8) } else { i64::from(colw & 0x3FFF) };
        // Shared formulas wrap around the grid (65,536 rows, 256 columns in BIFF8).
        let row = row.rem_euclid(65_536);
        let col = col.rem_euclid(256);
        Some(format!("{}{}{}{}", if col_rel { "" } else { "$" }, col_letters(col as u32), if row_rel { "" } else { "$" }, row + 1))
    }

    fn area(&self, d: &[u8], at: usize, offsets: bool) -> Option<String> {
        let (r1, r2, c1, c2) = (u16_at(d, at)?, u16_at(d, at + 2)?, u16_at(d, at + 4)?, u16_at(d, at + 6)?);
        // Whole columns and whole rows.
        if !offsets && r1 == 0 && r2 == 0xFFFF {
            let c = |w: u16| format!("{}{}", if w & 0x4000 != 0 { "" } else { "$" }, col_letters(u32::from(w & 0x3FFF)));
            return Some(format!("{}:{}", c(c1), c(c2)));
        }
        if !offsets && c1 & 0x3FFF == 0 && c2 & 0x3FFF == 0xFF {
            let r = |rw: u16, w: u16| format!("{}{}", if w & 0x8000 != 0 { "" } else { "$" }, u32::from(rw) + 1);
            return Some(format!("{}:{}", r(r1, c1), r(r2, c2)));
        }
        Some(format!("{}:{}", self.cell(r1, c1, offsets)?, self.cell(r2, c2, offsets)?))
    }

    /// The sheet (or sheets) an EXTERNSHEET entry names.
    fn sheets(&self, ixti: u16) -> Option<String> {
        let (book, first, last) = *self.g.xti.get(usize::from(ixti))?;
        let internal = self.g.supbooks.get(usize::from(book)).is_none_or(|b| b.0);
        if !internal || first < 0 {
            return None;
        }
        let name = |i: i16| self.g.sheets.get(i as usize).map(|s| s.0.clone());
        let a = name(first)?;
        if last != first
            && let Some(b) = name(last)
        {
            return Some(sheet_prefix(&format!("{a}:{b}")));
        }
        Some(sheet_prefix(&a))
    }

    fn extern_name(&self, ixti: u16, iname: u16) -> Option<String> {
        let (book, _, _) = *self.g.xti.get(usize::from(ixti))?;
        self.g.supbooks.get(usize::from(book))?.1.get(usize::from(iname).checked_sub(1)?).cloned()
    }

    /// Decodes a formula's tokens (`rgce`, with `extra` the data after them) to its text.
    fn decode(&self, rgce: &[u8], extra: &[u8], offsets: bool) -> Option<String> {
        let mut stack: Vec<String> = Vec::new();
        let mut at = 0usize;
        let mut extra_at = 0usize;
        let mut tokens = 0;
        let pop2 = |stack: &mut Vec<String>| -> Option<(String, String)> {
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some((a, b))
        };
        while at < rgce.len() {
            tokens += 1;
            if tokens > MAX_TOKENS {
                return None;
            }
            let ptg = *rgce.get(at)?;
            at += 1;
            let base = if ptg >= 0x20 { (ptg & 0x1F) | 0x20 } else { ptg };
            match base {
                0x01 | 0x02 => return None,
                0x03..=0x11 => {
                    let op = match ptg {
                        0x03 => "+",
                        0x04 => "-",
                        0x05 => "*",
                        0x06 => "/",
                        0x07 => "^",
                        0x08 => "&",
                        0x09 => "<",
                        0x0A => "<=",
                        0x0B => "=",
                        0x0C => ">=",
                        0x0D => ">",
                        0x0E => "<>",
                        0x0F => " ",
                        0x10 => ",",
                        _ => ":",
                    };
                    let (a, b) = pop2(&mut stack)?;
                    stack.push(if ptg == 0x10 { format!("({a},{b})") } else { format!("{a}{op}{b}") });
                }
                0x12 => {
                    let a = stack.pop()?;
                    stack.push(format!("+{a}"));
                }
                0x13 => {
                    let a = stack.pop()?;
                    stack.push(format!("-{a}"));
                }
                0x14 => {
                    let a = stack.pop()?;
                    stack.push(format!("{a}%"));
                }
                0x15 => {
                    let a = stack.pop()?;
                    stack.push(format!("({a})"));
                }
                0x16 => stack.push(String::new()),
                0x17 => {
                    let (s, len) = short_string(rgce, at)?;
                    at += len;
                    stack.push(format!("\"{}\"", s.replace('"', "\"\"")));
                }
                0x18 => return None,
                0x19 => {
                    let kind = *rgce.get(at)?;
                    let data = u16_at(rgce, at + 1)?;
                    at += 3;
                    if kind & 0x04 != 0 {
                        at += (usize::from(data) + 1) * 2;
                    } else if kind & 0x10 != 0 {
                        let a = stack.pop()?;
                        stack.push(format!("SUM({a})"));
                    }
                }
                0x1C => {
                    stack.push(error_text(*rgce.get(at)?).into());
                    at += 1;
                }
                0x1D => {
                    stack.push(if *rgce.get(at)? != 0 { "TRUE" } else { "FALSE" }.into());
                    at += 1;
                }
                0x1E => {
                    stack.push(u16_at(rgce, at)?.to_string());
                    at += 2;
                }
                0x1F => {
                    let v = f64_at(rgce, at)?;
                    at += 8;
                    stack.push(if v.is_finite() { format!("{v}") } else { "0".into() });
                }
                0x20 => {
                    at += 7;
                    // The array's values are in the data after the tokens.
                    let cols = usize::from(*extra.get(extra_at)?) + 1;
                    let rows = usize::from(u16_at(extra, extra_at + 1)?) + 1;
                    extra_at += 3;
                    let mut out = String::from("{");
                    for r in 0..rows.min(4096) {
                        if r > 0 {
                            out.push(';');
                        }
                        for c in 0..cols.min(256) {
                            if c > 0 {
                                out.push(',');
                            }
                            let kind = *extra.get(extra_at)?;
                            extra_at += 1;
                            match kind {
                                0x01 => {
                                    out.push_str(&format!("{}", f64_at(extra, extra_at)?));
                                    extra_at += 8;
                                }
                                0x02 => {
                                    let (s, len) = long_string(extra, extra_at)?;
                                    extra_at += len;
                                    out.push_str(&format!("\"{}\"", s.replace('"', "\"\"")));
                                }
                                0x04 => {
                                    out.push_str(if *extra.get(extra_at)? != 0 { "TRUE" } else { "FALSE" });
                                    extra_at += 8;
                                }
                                0x10 => {
                                    out.push_str(error_text(*extra.get(extra_at)?));
                                    extra_at += 8;
                                }
                                _ => extra_at += 8,
                            }
                        }
                    }
                    out.push('}');
                    stack.push(out);
                }
                0x21 | 0x22 => {
                    let (count, index) = if base == 0x21 {
                        let index = u16_at(rgce, at)?;
                        at += 2;
                        (usize::from(function(index)?.1?), index)
                    } else {
                        let count = usize::from(*rgce.get(at)? & 0x7F);
                        let index = u16_at(rgce, at + 1)? & 0x7FFF;
                        at += 3;
                        (count, index)
                    };
                    if stack.len() < count {
                        return None;
                    }
                    let mut args = stack.split_off(stack.len() - count);
                    let name = if index == 255 {
                        // A user-defined or add-in function: its name is the first argument.
                        if args.is_empty() {
                            return None;
                        }
                        args.remove(0)
                    } else {
                        function(index)?.0.to_owned()
                    };
                    stack.push(format!("{name}({})", args.join(",")));
                }
                0x23 => {
                    let index = u32_at(rgce, at)? as usize;
                    at += 4;
                    stack.push(self.g.names.get(index.checked_sub(1)?)?.0.clone());
                }
                0x24 | 0x2C => {
                    let (rw, colw) = (u16_at(rgce, at)?, u16_at(rgce, at + 2)?);
                    at += 4;
                    stack.push(self.cell(rw, colw, base == 0x2C)?);
                }
                0x25 | 0x2D => {
                    stack.push(self.area(rgce, at, base == 0x2D)?);
                    at += 8;
                }
                0x26..=0x28 => at += 6,
                0x29 | 0x2E => at += 2,
                0x2A => {
                    at += 4;
                    stack.push("#REF!".into());
                }
                0x2B => {
                    at += 8;
                    stack.push("#REF!".into());
                }
                0x39 => {
                    let (ixti, iname) = (u16_at(rgce, at)?, u16_at(rgce, at + 2)?);
                    at += 6;
                    stack.push(self.extern_name(ixti, iname).unwrap_or_else(|| "#NAME?".into()));
                }
                0x3A => {
                    let (ixti, rw, colw) = (u16_at(rgce, at)?, u16_at(rgce, at + 2)?, u16_at(rgce, at + 4)?);
                    at += 6;
                    let sheet = self.sheets(ixti).unwrap_or_else(|| "#REF!".into());
                    stack.push(format!("{sheet}{}", self.cell(rw, colw, offsets)?));
                }
                0x3B => {
                    let ixti = u16_at(rgce, at)?;
                    let sheet = self.sheets(ixti).unwrap_or_else(|| "#REF!".into());
                    stack.push(format!("{sheet}{}", self.area(rgce, at + 2, offsets)?));
                    at += 10;
                }
                0x3C => {
                    at += 6;
                    stack.push("#REF!".into());
                }
                0x3D => {
                    at += 10;
                    stack.push("#REF!".into());
                }
                _ => return None,
            }
        }
        if stack.len() == 1 { stack.pop() } else { None }
    }
}

// ------------------------------------------------------------------ sheets

#[derive(Default)]
struct Pending {
    /// Cells whose formula is a shared or array formula anchored elsewhere: (cell, anchor).
    shared_uses: Vec<(CellRef, CellRef)>,
    /// Shared formulas by their anchor: the range they cover and their tokens.
    shared: HashMap<CellRef, (RangeRef, Vec<u8>, Vec<u8>)>,
    arrays: Vec<(RangeRef, Vec<u8>, Vec<u8>)>,
    /// Notes' texts by their drawing object id, and the notes (cell, object id, author).
    texts: HashMap<u16, String>,
    notes: Vec<(CellRef, u16, String)>,
}

/// The tokens of a CellParsedFormula at `at`: (rgce, the data after it).
fn parsed_formula(d: &[u8], at: usize) -> Option<(Vec<u8>, Vec<u8>)> {
    let cce = usize::from(u16_at(d, at)?);
    let rgce = d.get(at + 2..at + 2 + cce)?.to_vec();
    let extra = d.get(at + 2 + cce..).unwrap_or_default().to_vec();
    Some((rgce, extra))
}

struct SheetReader<'a> {
    g: &'a Globals,
    xf_ids: &'a [StyleId],
    char_px: f64,
    cells: &'a mut usize,
}

impl SheetReader<'_> {
    fn style(&self, ixfe: u16) -> StyleId {
        self.xf_ids.get(usize::from(ixfe)).copied().unwrap_or(StyleId::DEFAULT)
    }

    fn put(&mut self, sheet: &mut Sheet, row: u16, col: u16, value: Value, ixfe: u16) {
        if u32::from(row) >= MAX_ROWS || u32::from(col) >= MAX_COLS || *self.cells >= MAX_CELLS {
            return;
        }
        *self.cells += 1;
        let mut cell = Cell::value(value);
        cell.style = self.style(ixfe);
        sheet.cells.set(CellRef::new(u32::from(row), u32::from(col)), cell);
    }

    fn read(&mut self, name: &str, recs: &[Record], report: &mut ReadReport) -> Sheet {
        let mut sheet = Sheet::new(name);
        sheet.default_row_height = crate::FILE_DEFAULT_ROW_PX;
        sheet.default_col_width = crate::FILE_DEFAULT_COL_PX;
        let mut pending = Pending::default();
        let mut last_formula: Option<CellRef> = None;
        let mut last_object: Option<u16> = None;
        let mut frozen = false;
        let mut skipped = std::collections::BTreeSet::new();
        for r in recs {
            let d = &r.data;
            match r.kind {
                0x0225 => {
                    if let (Some(flags), Some(twips)) = (u16_at(d, 0), u16_at(d, 2))
                        && flags & 1 != 0
                        && twips > 0
                    {
                        sheet.default_row_height = crate::tables::pt_to_px(f64::from(twips) / 20.0);
                    }
                }
                0x0055 => {
                    if let Some(chars) = u16_at(d, 0) {
                        sheet.default_col_width = ((f64::from(chars) + 0.43) * self.char_px) as f32;
                    }
                }
                0x007D => {
                    let (Some(first), Some(last), Some(width), Some(ixfe), Some(flags)) =
                        (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u16_at(d, 6), u16_at(d, 8))
                    else {
                        continue;
                    };
                    let mut info = LineInfo {
                        size: Some(crate::tables::col_width_to_px(f64::from(width) / 256.0, self.char_px)),
                        hidden: flags & 1 != 0,
                        outline: ((flags >> 8) & 7) as u8,
                        collapsed: flags & 0x1000 != 0,
                        ..LineInfo::default()
                    };
                    let st = self.style(ixfe);
                    if st != StyleId::DEFAULT {
                        info.style = Some(st);
                    }
                    for c in u32::from(first)..=u32::from(last).min(MAX_COLS - 1).min(u32::from(first) + 16_384) {
                        sheet.cols.insert(c, info);
                    }
                }
                0x0208 => {
                    let (Some(row), Some(height), Some(flags), Some(xf)) = (u16_at(d, 0), u16_at(d, 6), u16_at(d, 12), u16_at(d, 14)) else {
                        continue;
                    };
                    let mut info = LineInfo {
                        hidden: flags & 0x20 != 0,
                        outline: (flags & 7) as u8,
                        collapsed: flags & 0x10 != 0,
                        custom: flags & 0x40 != 0,
                        ..LineInfo::default()
                    };
                    let twips = height & 0x7FFF;
                    if twips > 0 {
                        info.size = Some(crate::tables::pt_to_px(kition_sheet_model::units::file_row_height_pt(f64::from(twips) / 20.0)));
                    }
                    if flags & 0x80 != 0 {
                        let st = self.style(xf & 0x0FFF);
                        if st != StyleId::DEFAULT {
                            info.style = Some(st);
                        }
                    }
                    if info != LineInfo::default() {
                        sheet.rows.insert(u32::from(row), info);
                    }
                }
                0x0203 => {
                    if let (Some(row), Some(col), Some(xf), Some(v)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), f64_at(d, 6)) {
                        self.put(&mut sheet, row, col, if v.is_finite() { Value::Number(v) } else { Value::Error(CellError::Num) }, xf);
                    }
                }
                0x027E => {
                    if let (Some(row), Some(col), Some(xf), Some(v)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u32_at(d, 6)) {
                        self.put(&mut sheet, row, col, Value::Number(rk(v)), xf);
                    }
                }
                0x00BD => {
                    let (Some(row), Some(first)) = (u16_at(d, 0), u16_at(d, 2)) else { continue };
                    let count = d.len().saturating_sub(6) / 6;
                    for i in 0..count {
                        let at = 4 + i * 6;
                        if let (Some(xf), Some(v)) = (u16_at(d, at), u32_at(d, at + 2)) {
                            self.put(&mut sheet, row, first.saturating_add(i as u16), Value::Number(rk(v)), xf);
                        }
                    }
                }
                0x00FD => {
                    if let (Some(row), Some(col), Some(xf), Some(i)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u32_at(d, 6)) {
                        let text = self.g.strings.get(i as usize).cloned().unwrap_or_else(|| Arc::from(""));
                        self.put(&mut sheet, row, col, Value::Text(text), xf);
                    }
                }
                0x0204 => {
                    if let (Some(row), Some(col), Some(xf), Some((text, _))) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), long_string(d, 6)) {
                        self.put(&mut sheet, row, col, Value::Text(Arc::from(text.as_str())), xf);
                    }
                }
                0x0205 => {
                    if let (Some(row), Some(col), Some(xf), Some(v), Some(is_error)) =
                        (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u8_at(d, 6), u8_at(d, 7))
                    {
                        self.put(&mut sheet, row, col, if is_error != 0 { Value::Error(error(v)) } else { Value::Bool(v != 0) }, xf);
                    }
                }
                0x0201 => {
                    if let (Some(row), Some(col), Some(xf)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4))
                        && self.style(xf) != StyleId::DEFAULT
                    {
                        self.put(&mut sheet, row, col, Value::Empty, xf);
                    }
                }
                0x00BE => {
                    let (Some(row), Some(first)) = (u16_at(d, 0), u16_at(d, 2)) else { continue };
                    let count = d.len().saturating_sub(6) / 2;
                    for i in 0..count {
                        if let Some(xf) = u16_at(d, 4 + i * 2)
                            && self.style(xf) != StyleId::DEFAULT
                        {
                            self.put(&mut sheet, row, first.saturating_add(i as u16), Value::Empty, xf);
                        }
                    }
                }
                0x0006 => {
                    let (Some(row), Some(col), Some(xf)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4)) else { continue };
                    let Some(raw) = d.get(6..14) else { continue };
                    let value = if raw.get(6..8) == Some(&[0xFF, 0xFF]) {
                        match raw.first() {
                            Some(1) => Value::Bool(raw.get(2).is_some_and(|b| *b != 0)),
                            Some(2) => Value::Error(error(raw.get(2).copied().unwrap_or(0x2A))),
                            // A string result follows in a STRING record; an empty string otherwise.
                            _ => Value::Text(Arc::from("")),
                        }
                    } else {
                        let v = f64_at(raw, 0).unwrap_or(0.0);
                        if v.is_finite() { Value::Number(v) } else { Value::Error(CellError::Num) }
                    };
                    self.put(&mut sheet, row, col, value, xf);
                    let here = CellRef::new(u32::from(row), u32::from(col));
                    last_formula = Some(here);
                    let Some((rgce, extra)) = parsed_formula(d, 20) else { continue };
                    if rgce.first() == Some(&0x01) {
                        if let (Some(ar), Some(ac)) = (u16_at(&rgce, 1), u16_at(&rgce, 3)) {
                            pending.shared_uses.push((here, CellRef::new(u32::from(ar), u32::from(ac))));
                        }
                        continue;
                    }
                    let cx = FormulaCx { g: self.g, row: here.row, col: here.col };
                    match cx.decode(&rgce, &extra, false) {
                        Some(text) => set_formula(&mut sheet, here, &text, None),
                        None => {
                            skipped.insert(format!("{}{}", col_letters(here.col), here.row + 1));
                        }
                    }
                }
                0x0207 => {
                    if let (Some(at), Some((text, _))) = (last_formula, long_string(d, 0))
                        && let Some(cell) = sheet.cells.get(at).cloned()
                    {
                        sheet.cells.set(at, Cell { value: Value::Text(Arc::from(text.as_str())), ..cell });
                    }
                }
                0x04BC => {
                    let (Some(r1), Some(r2), Some(c1), Some(c2)) = (u16_at(d, 0), u16_at(d, 2), u8_at(d, 4), u8_at(d, 5)) else { continue };
                    let range = RangeRef::new(CellRef::new(u32::from(r1), u32::from(c1)), CellRef::new(u32::from(r2), u32::from(c2)));
                    if let Some((rgce, extra)) = parsed_formula(d, 8) {
                        pending.shared.insert(range.start, (range, rgce, extra));
                    }
                }
                0x0221 => {
                    let (Some(r1), Some(r2), Some(c1), Some(c2)) = (u16_at(d, 0), u16_at(d, 2), u8_at(d, 4), u8_at(d, 5)) else { continue };
                    let range = RangeRef::new(CellRef::new(u32::from(r1), u32::from(c1)), CellRef::new(u32::from(r2), u32::from(c2)));
                    if let Some((rgce, extra)) = parsed_formula(d, 12) {
                        pending.arrays.push((range, rgce, extra));
                    }
                }
                0x00E5 => {
                    let count = usize::from(u16_at(d, 0).unwrap_or(0));
                    for i in 0..count.min(65_535) {
                        let at = 2 + i * 8;
                        let (Some(r1), Some(r2), Some(c1), Some(c2)) = (u16_at(d, at), u16_at(d, at + 2), u16_at(d, at + 4), u16_at(d, at + 6))
                        else {
                            break;
                        };
                        let m = RangeRef::new(CellRef::new(u32::from(r1), u32::from(c1)), CellRef::new(u32::from(r2), u32::from(c2)));
                        if !m.is_single() && u32::from(r2) < MAX_ROWS && u32::from(c2) < MAX_COLS {
                            sheet.merges.push(m);
                        }
                    }
                }
                0x01B8 => read_hyperlink(d, &mut sheet),
                0x023E => {
                    if let Some(flags) = u16_at(d, 0) {
                        sheet.show_gridlines = flags & 0x02 != 0;
                        sheet.show_headings = flags & 0x04 != 0;
                        frozen = flags & 0x08 != 0;
                        sheet.show_zeros = flags & 0x10 != 0;
                        sheet.right_to_left = flags & 0x40 != 0;
                    }
                }
                0x0041 => {
                    if frozen
                        && let (Some(x), Some(y)) = (u16_at(d, 0), u16_at(d, 2))
                        && (x > 0 || y > 0)
                    {
                        sheet.freeze = Some((u32::from(y), u32::from(x)));
                    }
                }
                0x00A0 => {
                    if let (Some(n), Some(den)) = (u16_at(d, 0), u16_at(d, 2))
                        && den > 0
                    {
                        sheet.zoom = (u32::from(n) * 100 / u32::from(den)).clamp(10, 400) as u16;
                    }
                }
                0x0862 => {
                    if let Some(icv) = u32_at(d, 16) {
                        let icv = (icv & 0x7F) as u16;
                        if icv < 64 {
                            sheet.tab_color = Some(color(icv, &self.g.palette));
                        }
                    }
                }
                0x005D => {
                    // OBJ: the first sub-record (ftCmo) names the object's id.
                    last_object = u16_at(d, 6);
                }
                0x01B6 => {
                    // TXO: a text box's (a note's) text, in the continuation after the record.
                    if let (Some(id), Some(len), Some(&start)) = (last_object, u16_at(d, 10), r.breaks.first()) {
                        let high = u8_at(d, start).unwrap_or(0) & 1 != 0;
                        let mut s = Strings { data: d, breaks: &r.breaks[1..], at: start + 1 };
                        if let Some(text) = s.chars(usize::from(len), high) {
                            pending.texts.insert(id, text.replace('\r', ""));
                        }
                    }
                }
                0x001C => {
                    if let (Some(row), Some(col), Some(id)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 6)) {
                        let author = long_string(d, 8).map(|(a, _)| a).unwrap_or_default();
                        pending.notes.push((CellRef::new(u32::from(row), u32::from(col)), id, author));
                    }
                }
                0x01B2 | 0x01BE => {
                    report.warn(format!("sheet \"{name}\": data validation is not imported from .xls files"));
                }
                0x01B0 | 0x01B1 => {
                    report.warn(format!("sheet \"{name}\": conditional formatting is not imported from .xls files"));
                }
                0x00EC => {
                    report.warn(format!("sheet \"{name}\": charts, pictures and shapes are not imported from .xls files"));
                }
                _ => {}
            }
        }
        for (at, anchor) in pending.shared_uses {
            if let Some((range, rgce, extra)) = pending.shared.get(&anchor)
                && range.contains(at)
            {
                let cx = FormulaCx { g: self.g, row: at.row, col: at.col };
                match cx.decode(rgce, extra, true) {
                    Some(text) => set_formula(&mut sheet, at, &text, None),
                    None => {
                        skipped.insert(format!("{}{}", col_letters(at.col), at.row + 1));
                    }
                }
            } else if let Some((range, rgce, extra)) = pending.arrays.iter().find(|(r, _, _)| r.start == anchor)
                && at == range.start
            {
                let cx = FormulaCx { g: self.g, row: at.row, col: at.col };
                if let Some(text) = cx.decode(rgce, extra, false) {
                    set_formula(&mut sheet, at, &text, Some(*range));
                }
            }
        }
        for (at, id, author) in pending.notes {
            if let Some(text) = pending.texts.get(&id) {
                sheet
                    .comments
                    .insert(at, Comment { author, text: text.clone(), replies: Vec::new(), threaded: false, resolved: false, visible: false });
            }
        }
        if !skipped.is_empty() {
            let list: Vec<String> = skipped.iter().take(5).cloned().collect();
            report.warn(format!(
                "sheet \"{name}\": {} formulas could not be read and keep their saved results ({}{})",
                skipped.len(),
                list.join(", "),
                if skipped.len() > 5 { ", …" } else { "" }
            ));
        }
        sheet
    }
}

/// Gives a cell its formula, when the engine can parse it; else the cell keeps its saved result.
fn set_formula(sheet: &mut Sheet, at: CellRef, text: &str, array: Option<RangeRef>) {
    let mut formula = Formula::new(&crate::fmla::from_file(text));
    if formula.expr.is_none() {
        return;
    }
    formula.array = array;
    let cell = sheet.cells.get(at).cloned().unwrap_or_default();
    sheet.cells.set(at, Cell { formula: Some(Arc::new(formula)), ..cell });
}

/// A HyperlinkString (2.3.7.9): a length (characters, with the terminating null) and UTF-16.
fn hyperlink_string(d: &[u8], at: &mut usize) -> Option<String> {
    let len = u32_at(d, *at)? as usize;
    *at += 4;
    let bytes = d.get(*at..at.checked_add(len.checked_mul(2)?)?)?;
    *at += len * 2;
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).take_while(|u| *u != 0).collect();
    Some(String::from_utf16_lossy(&units))
}

/// The HLINK record (2.4.140): a URL, a place in the workbook, or both.
fn read_hyperlink(d: &[u8], sheet: &mut Sheet) {
    let (Some(r1), Some(r2), Some(c1), Some(c2), Some(flags)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), u16_at(d, 6), u32_at(d, 28)) else {
        return;
    };
    let mut at = 32;
    let mut display = None;
    if flags & 0x10 != 0 {
        display = hyperlink_string(d, &mut at);
    }
    if flags & 0x80 != 0 {
        let _ = hyperlink_string(d, &mut at);
    }
    let mut target = String::new();
    let mut external = false;
    if flags & 0x01 != 0 {
        if flags & 0x100 != 0 {
            target = hyperlink_string(d, &mut at).unwrap_or_default();
            external = true;
        } else {
            const URL: [u8; 16] = [0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B];
            const FILE: [u8; 16] = [0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46];
            let clsid = d.get(at..at + 16);
            at += 16;
            if clsid == Some(&URL[..]) {
                let len = u32_at(d, at).unwrap_or(0) as usize;
                at += 4;
                if let Some(bytes) = d.get(at..at + len) {
                    let units: Vec<u16> =
                        bytes.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
                    target = String::from_utf16_lossy(&units);
                    external = true;
                }
                at += len;
            } else if clsid == Some(&FILE[..]) {
                let ups = u16_at(d, at).unwrap_or(0);
                let ansi_len = u32_at(d, at + 2).unwrap_or(0) as usize;
                let ansi: String =
                    d.get(at + 6..at + 6 + ansi_len).unwrap_or_default().iter().take_while(|b| **b != 0).map(|b| char::from(*b)).collect();
                target = format!("{}{}", "../".repeat(usize::from(ups).min(32)), ansi);
                external = true;
                // The rest of the file moniker (its Unicode path) is skipped: the ANSI path names it.
                at = d.len();
            } else {
                return;
            }
        }
    }
    if flags & 0x08 != 0
        && let Some(location) = hyperlink_string(d, &mut at)
    {
        if target.is_empty() {
            target = location;
        } else {
            target = format!("{target}#{location}");
        }
    }
    if target.is_empty() {
        return;
    }
    for row in u32::from(r1)..=u32::from(r2).min(u32::from(r1) + 1000) {
        for col in u32::from(c1)..=u32::from(c2).min(u32::from(c1) + 256) {
            sheet.hyperlinks.insert(CellRef::new(row, col), Hyperlink { target: target.clone(), tooltip: None, display: display.clone(), external });
        }
    }
}

/// Whether `bytes` is a compound file holding an Excel workbook stream.
pub fn is_xls(bytes: &[u8]) -> bool {
    crate::cfb::Compound::open(bytes).is_some_and(|cf| {
        cf.entries.iter().any(|e| e.kind != 1 && e.kind != 5 && (e.name.eq_ignore_ascii_case("Workbook") || e.name.eq_ignore_ascii_case("Book")))
    })
}

/// Reads an Excel 97-2003 workbook. Warnings name what was not imported.
pub fn read_xls(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    let cf = crate::cfb::Compound::open(bytes).ok_or_else(|| invalid("not a compound file"))?;
    let stream = match cf.read("Workbook") {
        Some(s) => s,
        None if cf.read("Book").is_some() => return Err(invalid("only Excel 97-2003 (BIFF8) workbooks are read; this is an older Excel 5/95 file")),
        None => return Err(invalid("this compound file holds no Excel workbook")),
    };
    let mut report = ReadReport::default();
    if cf.entries.iter().any(|e| e.name.eq_ignore_ascii_case("_VBA_PROJECT_CUR")) {
        report.warn("the workbook's macros are not imported");
    }
    let globals = records(&stream, 0);
    if globals.first().is_none_or(|r| r.kind != 0x0809) {
        return Err(invalid("the workbook stream does not start with a BOF record"));
    }
    if globals.iter().any(|r| r.kind == 0x002F) {
        return Err(IoError::Format(
            "This .xls workbook is password-protected. Kition Sheet can't open encrypted workbooks yet; remove the password and try again.".into(),
        ));
    }
    let g = read_globals(&globals, &mut report)?;
    let mut wb = Workbook::new();
    wb.sheets.clear();
    wb.date_system = if g.date_1904 { DateSystem::D1904 } else { DateSystem::D1900 };
    // Cell formats: the 16th is the default cell format (the first 15 are style formats).
    let default_xf = if g.xfs.len() > 15 { 15 } else { 0 };
    let mut styles = kition_sheet_model::StyleTable::default();
    if let Some(normal) = g.xfs.get(default_xf) {
        styles.set_default(normal.clone());
    }
    let xf_ids: Vec<StyleId> =
        g.xfs.iter().enumerate().map(|(i, s)| if i == default_xf { StyleId::DEFAULT } else { styles.intern(s.clone()) }).collect();
    let normal = styles.get(StyleId::DEFAULT).font.clone();
    let char_px = kition_sheet_model::units::char_width_px(&normal.name, normal.size);
    let auto_row_px = (kition_sheet_model::units::default_row_height_pt(normal.size) * kition_sheet_model::units::PX_PER_PT) as f32;
    let mut cells = 0usize;
    for (name, pos, visibility) in &g.sheets {
        let recs = records(&stream, *pos);
        let mut reader = SheetReader { g: &g, xf_ids: &xf_ids, char_px, cells: &mut cells };
        let mut sheet = reader.read(name, &recs, &mut report);
        let custom_default = recs.iter().any(|r| r.kind == 0x0225 && u16_at(&r.data, 0).is_some_and(|f| f & 1 != 0));
        if !custom_default {
            sheet.default_row_height = auto_row_px;
        }
        sheet.visibility = *visibility;
        wb.sheets.push(Arc::new(sheet));
        if cells >= MAX_CELLS {
            report.warn("the workbook has more than 5 million cells; the rest were not imported");
            break;
        }
    }
    wb.styles = styles;
    if wb.sheets.is_empty() {
        wb.sheets.push(Arc::new(Sheet::new("Sheet1")));
    }
    wb.active_sheet = g.active.min(wb.sheets.len() - 1);
    // Defined names (print areas and titles go to the sheets' print settings).
    for (name, scope, rgce, hidden) in &g.names {
        let cx = FormulaCx { g: &g, row: 0, col: 0 };
        let Some(text) = cx.decode(rgce, &[], false) else { continue };
        match name.as_str() {
            "_xlnm._FilterDatabase" => {}
            "_xlnm.Print_Area" => {
                if let Some(si) = scope
                    && let Some(range) = text.rsplit('!').next().and_then(|r| RangeRef::parse(&r.replace('$', "")))
                    && let Some(sh) = wb.sheet_mut(*si)
                {
                    sh.print.print_area = Some(range);
                }
            }
            "_xlnm.Print_Titles" => {
                if let Some(si) = scope
                    && let Some(sh) = wb.sheet_mut(*si)
                {
                    for part in text.trim_matches(|c| c == '(' || c == ')').split(',') {
                        let r = part.rsplit('!').next().unwrap_or("").replace('$', "");
                        if let Some((a, b)) = r.split_once(':') {
                            if let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) {
                                sh.print.title_rows = Some((a.saturating_sub(1), b.saturating_sub(1)));
                            } else if a.chars().all(|c| c.is_ascii_alphabetic()) {
                                let col = |s: &str| s.chars().fold(0u32, |n, c| n * 26 + (c.to_ascii_uppercase() as u32 - 64)).saturating_sub(1);
                                sh.print.title_cols = Some((col(a), col(b)));
                            }
                        }
                    }
                }
            }
            _ => wb.names.push(DefinedName { name: name.clone(), scope: *scope, formula: text, comment: String::new(), hidden: *hidden }),
        }
    }
    wb.number_sheets();
    Ok((wb, report))
}

#[cfg(test)]
pub(crate) mod tests;
