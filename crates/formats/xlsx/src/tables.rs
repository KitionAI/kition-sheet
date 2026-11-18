//! Fixed lookup tables: built-in number formats, the legacy indexed colour palette, paper sizes,
//! functions that need the `_xlfn.` prefix in files, and unit conversions.

/// Built-in number format codes (en-US) by id. Ids not listed (23–36 are East Asian
/// locale formats) read as General.
const BUILTIN_FORMATS: [(u32, &str); 36] = [
    (0, "General"),
    (1, "0"),
    (2, "0.00"),
    (3, "#,##0"),
    (4, "#,##0.00"),
    (5, "\"$\"#,##0_);\\(\"$\"#,##0\\)"),
    (6, "\"$\"#,##0_);[Red]\\(\"$\"#,##0\\)"),
    (7, "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)"),
    (8, "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)"),
    (9, "0%"),
    (10, "0.00%"),
    (11, "0.00E+00"),
    (12, "# ?/?"),
    (13, "# ??/??"),
    (14, "m/d/yyyy"),
    (15, "d-mmm-yy"),
    (16, "d-mmm"),
    (17, "mmm-yy"),
    (18, "h:mm AM/PM"),
    (19, "h:mm:ss AM/PM"),
    (20, "h:mm"),
    (21, "h:mm:ss"),
    (22, "m/d/yyyy h:mm"),
    (37, "#,##0 ;(#,##0)"),
    (38, "#,##0 ;[Red](#,##0)"),
    (39, "#,##0.00;(#,##0.00)"),
    (40, "#,##0.00;[Red](#,##0.00)"),
    (41, "_(* #,##0_);_(* \\(#,##0\\);_(* \"-\"_);_(@_)"),
    (42, "_(\"$\"* #,##0_);_(\"$\"* \\(#,##0\\);_(\"$\"* \"-\"_);_(@_)"),
    (43, "_(* #,##0.00_);_(* \\(#,##0.00\\);_(* \"-\"??_);_(@_)"),
    (44, "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)"),
    (45, "mm:ss"),
    (46, "[h]:mm:ss"),
    (47, "mmss.0"),
    (48, "##0.0E+0"),
    (49, "@"),
];

pub fn builtin_format(id: u32) -> Option<&'static str> {
    BUILTIN_FORMATS.iter().find(|(i, _)| *i == id).map(|(_, c)| *c)
}

/// Id to write for a format code when it is a locale-independent built-in (currency formats 5–8
/// are written as custom formats because their meaning depends on the reader's locale).
pub fn builtin_id(code: &str) -> Option<u32> {
    BUILTIN_FORMATS.iter().find(|(i, c)| *c == code && !(5..=8).contains(i)).map(|(i, _)| *i)
}

/// The 64-entry legacy palette (`indexed` colours), RRGGBB.
pub const INDEXED: [u32; 64] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, // 0-7
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, // 8-15
    0x800000, 0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, // 16-23
    0x9999FF, 0x993366, 0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, // 24-31
    0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, // 32-39
    0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, // 40-47
    0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600, 0x666699, 0x969696, // 48-55
    0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399, 0x333333, // 56-63
];

/// Paper sizes (`pageSetup paperSize`) we name.
const PAPERS: [(u32, &str); 14] = [
    (1, "Letter"),
    (3, "Tabloid"),
    (4, "Ledger"),
    (5, "Legal"),
    (6, "Statement"),
    (7, "Executive"),
    (8, "A3"),
    (9, "A4"),
    (11, "A5"),
    (12, "B4"),
    (13, "B5"),
    (20, "Envelope #10"),
    (27, "Envelope DL"),
    (34, "Envelope B5"),
];

pub fn paper_name(id: u32) -> Option<&'static str> {
    PAPERS.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

pub fn paper_id(name: &str) -> Option<u32> {
    PAPERS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name.trim())).map(|(i, _)| *i)
}

/// Functions introduced after Excel 2007 that files store with the `_xlfn.` prefix. Entries with
/// `_xlws.` get `_xlfn._xlws.`.
const FUTURE_FUNCTIONS: &[&str] = &[
    "ACOT",
    "ACOTH",
    "AGGREGATE",
    "ARABIC",
    "ARRAYTOTEXT",
    "BASE",
    "BETA.DIST",
    "BETA.INV",
    "BINOM.DIST",
    "BINOM.DIST.RANGE",
    "BINOM.INV",
    "BITAND",
    "BITLSHIFT",
    "BITOR",
    "BITRSHIFT",
    "BITXOR",
    "BYCOL",
    "BYROW",
    "CEILING.MATH",
    "CEILING.PRECISE",
    "CHISQ.DIST",
    "CHISQ.DIST.RT",
    "CHISQ.INV",
    "CHISQ.INV.RT",
    "CHISQ.TEST",
    "CHOOSECOLS",
    "CHOOSEROWS",
    "COMBINA",
    "CONCAT",
    "CONFIDENCE.NORM",
    "CONFIDENCE.T",
    "COT",
    "COTH",
    "COVARIANCE.P",
    "COVARIANCE.S",
    "CSC",
    "CSCH",
    "DAYS",
    "DECIMAL",
    "DROP",
    "ERF.PRECISE",
    "ERFC.PRECISE",
    "EXPAND",
    "EXPON.DIST",
    "F.DIST",
    "F.DIST.RT",
    "F.INV",
    "F.INV.RT",
    "F.TEST",
    "FIELDVALUE",
    "FILTERXML",
    "FLOOR.MATH",
    "FLOOR.PRECISE",
    "FORECAST.ETS",
    "FORECAST.ETS.CONFINT",
    "FORECAST.ETS.SEASONALITY",
    "FORECAST.ETS.STAT",
    "FORECAST.LINEAR",
    "FORMULATEXT",
    "GAMMA",
    "GAMMA.DIST",
    "GAMMA.INV",
    "GAMMALN.PRECISE",
    "GAUSS",
    "GROUPBY",
    "HSTACK",
    "HYPGEOM.DIST",
    "IFNA",
    "IFS",
    "IMAGE",
    "IMCOSH",
    "IMCOT",
    "IMCSC",
    "IMCSCH",
    "IMSEC",
    "IMSECH",
    "IMSINH",
    "IMTAN",
    "ISFORMULA",
    "ISOMITTED",
    "ISOWEEKNUM",
    "LAMBDA",
    "LET",
    "LOGNORM.DIST",
    "LOGNORM.INV",
    "MAKEARRAY",
    "MAP",
    "MAXIFS",
    "MINIFS",
    "MODE.MULT",
    "MODE.SNGL",
    "MUNIT",
    "NEGBINOM.DIST",
    "NETWORKDAYS.INTL",
    "NORM.DIST",
    "NORM.INV",
    "NORM.S.DIST",
    "NORM.S.INV",
    "NUMBERVALUE",
    "PDURATION",
    "PERCENTILE.EXC",
    "PERCENTILE.INC",
    "PERCENTRANK.EXC",
    "PERCENTRANK.INC",
    "PERMUTATIONA",
    "PHI",
    "PIVOTBY",
    "POISSON.DIST",
    "QUARTILE.EXC",
    "QUARTILE.INC",
    "RANDARRAY",
    "RANK.AVG",
    "RANK.EQ",
    "REDUCE",
    "RRI",
    "SCAN",
    "SEC",
    "SECH",
    "SEQUENCE",
    "SHEET",
    "SHEETS",
    "SINGLE",
    "SKEW.P",
    "SORTBY",
    "STDEV.P",
    "STDEV.S",
    "SWITCH",
    "T.DIST",
    "T.DIST.2T",
    "T.DIST.RT",
    "T.INV",
    "T.INV.2T",
    "T.TEST",
    "TAKE",
    "TEXTAFTER",
    "TEXTBEFORE",
    "TEXTJOIN",
    "TEXTSPLIT",
    "TOCOL",
    "TOROW",
    "UNICHAR",
    "UNICODE",
    "UNIQUE",
    "VALUETOTEXT",
    "VAR.P",
    "VAR.S",
    "VSTACK",
    "WEBSERVICE",
    "WEIBULL.DIST",
    "WORKDAY.INTL",
    "WRAPCOLS",
    "WRAPROWS",
    "XLOOKUP",
    "XMATCH",
    "XOR",
    "Z.TEST",
    "_xlws.FILTER",
    "_xlws.SORT",
];

/// The name to write for a function (`XLOOKUP` → `_xlfn.XLOOKUP`, `FILTER` → `_xlfn._xlws.FILTER`).
pub fn file_function_name(name: &str) -> Option<String> {
    let u = name.to_ascii_uppercase();
    for f in FUTURE_FUNCTIONS {
        if let Some(ws) = f.strip_prefix("_xlws.") {
            if ws == u {
                return Some(format!("_xlfn._xlws.{u}"));
            }
        } else if *f == u {
            return Some(format!("_xlfn.{u}"));
        }
    }
    None
}

// ---------------------------------------------------------------- units

/// Column width as stored in a file (characters) → model width (px), with `char_px` pixels per
/// character (the workbook's character unit, `kition_sheet_model::units`); to 0.01 px.
pub fn col_width_to_px(w: f64, char_px: f64) -> f32 {
    if !w.is_finite() || w <= 0.0 || !char_px.is_finite() || char_px <= 0.0 {
        return 0.0;
    }
    ((w.min(255.0) * char_px * 100.0).round() / 100.0) as f32
}

/// Model width (px) → stored column width (characters), to 1e-6 of a character: reads back as
/// the same width.
pub fn px_to_col_width(px: f32, char_px: f64) -> f64 {
    let char_px = if char_px.is_finite() && char_px > 0.0 { char_px } else { 7.0 };
    let w = (f64::from(px.max(0.0)) / char_px).min(255.0);
    (w * 1e6).round() / 1e6
}

/// Row height in typographic points → model px.
pub fn pt_to_px(pt: f64) -> f32 {
    if !pt.is_finite() || pt <= 0.0 { 0.0 } else { (pt.min(409.5) * 4.0 / 3.0) as f32 }
}

/// Model px → row height in points, to 1e-6 pt: fine enough that every f32 height reads back
/// exactly ([`pt_to_px`]), and short enough not to write the f32's binary noise.
pub fn px_to_pt(px: f32) -> f64 {
    ((px.max(0.0) as f64) * 0.75 * 1e6).round() / 1e6
}

/// EMU per px at 96 dpi.
pub const EMU_PER_PX: f64 = 9525.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// Widths in WPS for Mac's character unit (6.4 pt for Calibri 11, module M9 D1; Excel for
    /// Windows' 7-pixel unit with padding gave 64 px for 9.140625): every width reads back as
    /// written.
    #[test]
    fn widths() {
        let calibri = kition_sheet_model::units::char_width_px("Calibri", 11.0);
        assert_eq!(col_width_to_px(9.140625, calibri), 78.0);
        assert_eq!(col_width_to_px(7.5, calibri), 64.0);
        assert_eq!(px_to_col_width(64.0, calibri), 7.5);
        for char_px in [calibri, kition_sheet_model::units::char_width_px("宋体", 11.0), 7.0] {
            for quarter in 0..1600 {
                let px = quarter as f32 / 4.0;
                assert_eq!(col_width_to_px(px_to_col_width(px, char_px), char_px), px, "px {px} at {char_px}");
            }
        }
        assert_eq!(pt_to_px(15.0), 20.0);
        assert_eq!(px_to_pt(20.0), 15.0);
    }

    /// A row height the engine works out (from a font size, say) is not on a 0.01 pt grid; written
    /// as `ht` and read back it must be the same height (cases/regressions/issue-row-height-precision).
    #[test]
    fn row_heights_read_back_as_written() {
        for px in [32.727_272_f32, 20.0, 15.5, 21.333_334, 545.999_94] {
            let written: f64 = crate::xml::num(px_to_pt(px)).parse().unwrap();
            assert_eq!(pt_to_px(written), px, "px {px} written as {written}");
        }
    }

    #[test]
    fn functions() {
        assert_eq!(file_function_name("xlookup").as_deref(), Some("_xlfn.XLOOKUP"));
        assert_eq!(file_function_name("FILTER").as_deref(), Some("_xlfn._xlws.FILTER"));
        assert_eq!(file_function_name("SUM"), None);
        assert_eq!(builtin_format(14), Some("m/d/yyyy"));
        assert_eq!(builtin_id("0.00%"), Some(10));
        assert_eq!(builtin_id(builtin_format(5).unwrap()), None);
        assert_eq!(paper_name(9), Some("A4"));
        assert_eq!(paper_id("a4"), Some(9));
    }
}
