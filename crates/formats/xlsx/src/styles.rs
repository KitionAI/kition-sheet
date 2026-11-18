//! `styles.xml`: reading cell formats and differential formats into model styles, and writing
//! the workbook's style table.

use std::collections::HashMap;
use std::fmt::Write as _;

use kition_sheet_model::style::Protection;
use kition_sheet_model::{
    Alignment, BorderLine, BorderStyle, Borders, Color, Fill, Font, HAlign, NumFmt, Palette, PatternType, Style, Underline, VAlign, VertAlign,
    Workbook,
};

use crate::tables::{INDEXED, builtin_format, builtin_id};
use crate::xml::{El, esc_attr, num};

// ---------------------------------------------------------------- reading

/// Styles read from a file.
#[derive(Debug, Default)]
pub struct StylesIn {
    /// Cell formats (`cellXfs`) in file order.
    pub xfs: Vec<Style>,
    /// Differential formats (`dxfs`) in file order.
    pub dxfs: Vec<Style>,
    pub palette: Vec<u32>,
    /// The file's own colours (`<colors>`), kept for writing back.
    pub colors: Option<Palette>,
    /// Named cell styles (`cellStyles` with their `cellStyleXfs`).
    pub named: Vec<(String, Style)>,
    /// What the file says of those named styles beyond their formats (built-in ones), by name.
    pub named_info: std::collections::BTreeMap<String, kition_sheet_model::CellStyleInfo>,
    /// Custom number formats (`numFmts`) by id.
    pub num_fmts: HashMap<u32, String>,
    /// The file's table styles (`tableStyles`).
    pub table_styles: kition_sheet_model::TableStyles,
    /// Each cell format's feature property bag complement (`xfpb:xfComplement i`), in file order:
    /// what Excel's cell controls (checkboxes) hang on ([`checkbox_complements`]).
    pub xf_complements: Vec<Option<u32>>,
}

/// The `uri` of a cell format's extension naming its feature property bag complement.
pub const XF_COMPLEMENT_EXT: &str = "{C7286773-470A-42A8-94C5-96B5CB345126}";
/// The feature property bag part's namespace, relationship type and content type.
pub const NS_FPB: &str = "http://schemas.microsoft.com/office/spreadsheetml/2022/featurepropertybag";
pub const REL_FPB: &str = "http://schemas.microsoft.com/office/2022/11/relationships/FeaturePropertyBag";
pub const CT_FPB: &str = "application/vnd.ms-excel.featurepropertybag+xml";

/// The feature property bags Kition writes when a workbook has checkboxes: a Checkbox bag (0), the
/// cell controls holding it (1), a cell format complement holding those (2), and the list cell
/// formats index into (`xfComplement i="0"` → bag 2).
pub const FPB_CHECKBOX: &str = concat!(
    "<bag type=\"Checkbox\"/>",
    "<bag type=\"XFControls\"><bagId k=\"CellControl\">0</bagId></bag>",
    "<bag type=\"XFComplement\"><bagId k=\"XFControls\">1</bagId></bag>",
    "<bag type=\"XFComplements\" extRef=\"XFComplementsMapperExtRef\"><a k=\"MappedFeaturePropertyBags\"><bagId>2</bagId></a></bag>",
);

/// The complement indices (`xfComplement i`) of a feature property bag part that lead to a
/// checkbox: the `XFComplements` list's entry `i` names a bag whose bags (followed a few levels)
/// include one of type `Checkbox`.
pub fn checkbox_complements(root: &El) -> std::collections::HashSet<u32> {
    let bags: Vec<&El> = root.kids("bag").take(10_000).collect();
    let ids = |bag: &El| -> Vec<usize> {
        let mut out = Vec::new();
        for kid in bag.children.iter().take(1000) {
            if kid.name == "bagId" {
                out.extend(kid.text.trim().parse::<usize>().ok());
            }
            for inner in kid.kids("bagId").take(1000) {
                out.extend(inner.text.trim().parse::<usize>().ok());
            }
        }
        out
    };
    // Whether `at` is, or leads within a few steps to, a Checkbox bag.
    fn leads(bags: &[&El], ids: &dyn Fn(&El) -> Vec<usize>, at: usize, depth: u8) -> bool {
        let Some(bag) = bags.get(at) else { return false };
        if bag.attr("type") == Some("Checkbox") {
            return true;
        }
        depth < 6 && ids(bag).into_iter().any(|next| next != at && leads(bags, ids, next, depth + 1))
    }
    let mut out = std::collections::HashSet::new();
    for list in bags.iter().filter(|b| b.attr("type") == Some("XFComplements")) {
        let mapped: Vec<usize> = list
            .kids("a")
            .filter(|a| a.attr("k") == Some("MappedFeaturePropertyBags"))
            .flat_map(|a| a.kids("bagId").take(10_000).filter_map(|b| b.text.trim().parse::<usize>().ok()))
            .collect();
        for (i, bag) in mapped.into_iter().enumerate() {
            if leads(&bags, &ids, bag, 0) {
                out.insert(i as u32);
            }
        }
    }
    out
}

/// A cell format's complement index (`<extLst><ext uri=…><xfpb:xfComplement i="0"/>`).
fn xf_complement(xf: &El) -> Option<u32> {
    xf.child("extLst")?.kids("ext").find(|e| e.attr("uri") == Some(XF_COMPLEMENT_EXT))?.child("xfComplement")?.attr_u32("i")
}

pub fn read_color(e: &El, palette: &[u32]) -> Color {
    if e.flag("auto", false) {
        return Color::Auto;
    }
    if let Some(rgb) = e.attr("rgb") {
        return Color::from_hex(rgb).unwrap_or(Color::Auto);
    }
    if let Some(t) = e.attr_u32("theme") {
        if t > 11 {
            return Color::Auto;
        }
        let tint = e.attr_f64("tint").map(|v| (v * 1000.0).round().clamp(-1000.0, 1000.0) as i16).unwrap_or(0);
        return Color::Theme(t as u8, tint);
    }
    if let Some(i) = e.attr_u32("indexed") {
        return palette.get(i as usize).map(|&c| Color::Rgb(c)).unwrap_or(Color::Auto);
    }
    Color::Auto
}

fn opt_color(e: Option<&El>, palette: &[u32]) -> Color {
    e.map(|e| read_color(e, palette)).unwrap_or(Color::Auto)
}

/// `<b/>`-style boolean property elements: present means true unless `val` says otherwise.
fn bool_prop(e: &El, name: &str) -> bool {
    e.child(name).is_some_and(|c| c.flag("val", true))
}

pub fn read_font(e: &El, palette: &[u32]) -> Font {
    let mut f = Font::default();
    if let Some(n) = e.child("name").or_else(|| e.child("rFont")).and_then(|n| n.attr("val")) {
        f.name = n.to_string();
    }
    if let Some(sz) = e.child("sz").and_then(|s| s.attr_f64("val")) {
        f.size = sz.clamp(1.0, 409.0) as f32;
    }
    f.bold = bool_prop(e, "b");
    f.italic = bool_prop(e, "i");
    f.strike = bool_prop(e, "strike");
    if let Some(u) = e.child("u") {
        f.underline = match u.attr("val").unwrap_or("single") {
            "double" => Underline::Double,
            "singleAccounting" => Underline::SingleAccounting,
            "doubleAccounting" => Underline::DoubleAccounting,
            "none" => Underline::None,
            _ => Underline::Single,
        };
    }
    if let Some(v) = e.child_val("vertAlign") {
        f.vert = match v {
            "superscript" => VertAlign::Superscript,
            "subscript" => VertAlign::Subscript,
            _ => VertAlign::Baseline,
        };
    }
    f.charset = e.child("charset").and_then(|c| c.attr_u32("val"));
    f.scheme = e.child_val("scheme").map(str::to_owned);
    f.color = opt_color(e.child("color"), palette);
    // Text 1 (dk1) is what "automatic" font colour means in practice.
    if f.color == Color::Theme(1, 0) {
        f.color = Color::Auto;
    }
    f
}

fn pattern_from(s: &str) -> PatternType {
    match s {
        "solid" => PatternType::Solid,
        "mediumGray" => PatternType::Gray50,
        "darkGray" => PatternType::Gray75,
        "lightGray" => PatternType::Gray25,
        "gray125" => PatternType::Gray125,
        "gray0625" => PatternType::Gray0625,
        "darkHorizontal" => PatternType::DarkHorizontal,
        "darkVertical" => PatternType::DarkVertical,
        "darkDown" => PatternType::DarkDown,
        "darkUp" => PatternType::DarkUp,
        "darkGrid" => PatternType::DarkGrid,
        "darkTrellis" => PatternType::DarkTrellis,
        "lightHorizontal" => PatternType::LightHorizontal,
        "lightVertical" => PatternType::LightVertical,
        "lightDown" => PatternType::LightDown,
        "lightUp" => PatternType::LightUp,
        "lightGrid" => PatternType::LightGrid,
        "lightTrellis" => PatternType::LightTrellis,
        _ => PatternType::None,
    }
}

fn pattern_name(p: PatternType) -> &'static str {
    match p {
        PatternType::None => "none",
        PatternType::Solid => "solid",
        PatternType::Gray50 => "mediumGray",
        PatternType::Gray75 => "darkGray",
        PatternType::Gray25 => "lightGray",
        PatternType::Gray125 => "gray125",
        PatternType::Gray0625 => "gray0625",
        PatternType::DarkHorizontal => "darkHorizontal",
        PatternType::DarkVertical => "darkVertical",
        PatternType::DarkDown => "darkDown",
        PatternType::DarkUp => "darkUp",
        PatternType::DarkGrid => "darkGrid",
        PatternType::DarkTrellis => "darkTrellis",
        PatternType::LightHorizontal => "lightHorizontal",
        PatternType::LightVertical => "lightVertical",
        PatternType::LightDown => "lightDown",
        PatternType::LightUp => "lightUp",
        PatternType::LightGrid => "lightGrid",
        PatternType::LightTrellis => "lightTrellis",
    }
}

/// A `<fill>`. In differential formats a solid fill's colour is the *background* colour and the
/// pattern type may be omitted.
pub fn read_fill(e: &El, palette: &[u32], dxf: bool) -> Fill {
    if let Some(p) = e.child("patternFill") {
        let fg = opt_color(p.child("fgColor"), palette);
        let bg = opt_color(p.child("bgColor"), palette);
        let pt = p.attr("patternType");
        if dxf && (pt.is_none() || pt == Some("solid")) {
            let c = if bg != Color::Auto { bg } else { fg };
            if c == Color::Auto && pt.is_none() {
                return Fill::default();
            }
            return Fill::solid(c);
        }
        let pattern = pattern_from(pt.unwrap_or("none"));
        if pattern == PatternType::None {
            return Fill::default();
        }
        return Fill { pattern, fg, bg };
    }
    if let Some(g) = e.child("gradientFill") {
        // Approximated by the first stop's colour.
        if let Some(c) = g.kids("stop").next().and_then(|s| s.child("color")) {
            return Fill::solid(read_color(c, palette));
        }
    }
    Fill::default()
}

fn border_style(s: &str) -> BorderStyle {
    match s {
        "thin" => BorderStyle::Thin,
        "medium" => BorderStyle::Medium,
        "thick" => BorderStyle::Thick,
        "dashed" => BorderStyle::Dashed,
        "dotted" => BorderStyle::Dotted,
        "double" => BorderStyle::Double,
        "hair" => BorderStyle::Hair,
        "mediumDashed" => BorderStyle::MediumDashed,
        "dashDot" => BorderStyle::DashDot,
        "mediumDashDot" => BorderStyle::MediumDashDot,
        "dashDotDot" => BorderStyle::DashDotDot,
        "mediumDashDotDot" => BorderStyle::MediumDashDotDot,
        "slantDashDot" => BorderStyle::SlantDashDot,
        _ => BorderStyle::None,
    }
}

fn border_style_name(s: BorderStyle) -> &'static str {
    match s {
        BorderStyle::None => "none",
        BorderStyle::Thin => "thin",
        BorderStyle::Medium => "medium",
        BorderStyle::Thick => "thick",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Dotted => "dotted",
        BorderStyle::Double => "double",
        BorderStyle::Hair => "hair",
        BorderStyle::MediumDashed => "mediumDashed",
        BorderStyle::DashDot => "dashDot",
        BorderStyle::MediumDashDot => "mediumDashDot",
        BorderStyle::DashDotDot => "dashDotDot",
        BorderStyle::MediumDashDotDot => "mediumDashDotDot",
        BorderStyle::SlantDashDot => "slantDashDot",
    }
}

fn read_line(e: Option<&El>, palette: &[u32]) -> BorderLine {
    let Some(e) = e else { return BorderLine::default() };
    let style = border_style(e.attr("style").unwrap_or("none"));
    if style == BorderStyle::None {
        return BorderLine::default();
    }
    BorderLine { style, color: opt_color(e.child("color"), palette) }
}

pub fn read_border(e: &El, palette: &[u32]) -> Borders {
    let diag = read_line(e.child("diagonal"), palette);
    Borders {
        left: read_line(e.child("left").or_else(|| e.child("start")), palette),
        right: read_line(e.child("right").or_else(|| e.child("end")), palette),
        top: read_line(e.child("top"), palette),
        bottom: read_line(e.child("bottom"), palette),
        diag_down: if e.flag("diagonalDown", false) { diag } else { BorderLine::default() },
        diag_up: if e.flag("diagonalUp", false) { diag } else { BorderLine::default() },
        vertical: read_line(e.child("vertical"), palette),
        horizontal: read_line(e.child("horizontal"), palette),
    }
}

fn read_alignment(a: &El) -> Alignment {
    let mut al = Alignment {
        h: match a.attr("horizontal").unwrap_or("general") {
            "left" => HAlign::Left,
            "center" => HAlign::Center,
            "right" => HAlign::Right,
            "fill" => HAlign::Fill,
            "justify" => HAlign::Justify,
            "centerContinuous" => HAlign::CenterAcross,
            "distributed" => HAlign::Distributed,
            _ => HAlign::General,
        },
        v: match a.attr("vertical").unwrap_or("bottom") {
            "top" => VAlign::Top,
            "center" => VAlign::Center,
            "justify" => VAlign::Justify,
            "distributed" => VAlign::Distributed,
            _ => VAlign::Bottom,
        },
        wrap: a.flag("wrapText", false),
        shrink: a.flag("shrinkToFit", false),
        indent: a.attr_u32("indent").unwrap_or(0).min(250) as u8,
        rotation: 0,
    };
    if let Some(r) = a.attr_i64("textRotation") {
        al.rotation = match r {
            0..=90 => r as i16,
            91..=180 => -((r - 90) as i16),
            255 => 255,
            _ => 0,
        };
    }
    al
}

fn read_protection(p: &El) -> Protection {
    Protection { locked: p.flag("locked", true), hidden: p.flag("hidden", false) }
}

fn num_fmt_code(id: u32, custom: &HashMap<u32, String>) -> NumFmt {
    if let Some(c) = custom.get(&id) {
        return NumFmt::new(c);
    }
    NumFmt::new(builtin_format(id).unwrap_or("General"))
}

fn read_xf(xf: &El, fonts: &[Font], fills: &[Fill], borders: &[Borders], fmts: &HashMap<u32, String>) -> Style {
    let mut s = Style::default();
    if let Some(f) = xf.attr_u32("fontId").and_then(|i| fonts.get(i as usize)) {
        s.font = f.clone();
    } else if let Some(f) = fonts.first() {
        s.font = f.clone();
    }
    if let Some(f) = xf.attr_u32("fillId").and_then(|i| fills.get(i as usize)) {
        s.fill = *f;
    }
    if let Some(b) = xf.attr_u32("borderId").and_then(|i| borders.get(i as usize)) {
        s.borders = *b;
    }
    s.num_fmt = num_fmt_code(xf.attr_u32("numFmtId").unwrap_or(0), fmts);
    if let Some(a) = xf.child("alignment") {
        s.align = read_alignment(a);
    }
    if let Some(p) = xf.child("protection") {
        s.protection = read_protection(p);
    }
    s
}

pub fn read_styles(root: &El) -> StylesIn {
    let mut palette: Vec<u32> = INDEXED.to_vec();
    let mut colors = Palette::default();
    if let Some(ix) = root.path(&["colors", "indexedColors"]) {
        let custom: Vec<u32> = ix
            .kids("rgbColor")
            .take(INDEXED.len())
            .map(|c| match read_color(c, &INDEXED) {
                Color::Rgb(v) => v,
                _ => 0,
            })
            .collect();
        if !custom.is_empty() {
            palette = custom.clone();
            colors.indexed = custom;
        }
    }
    if let Some(mru) = root.path(&["colors", "mruColors"]) {
        colors.recent = mru.kids("color").take(64).map(crate::rich::read_run_color).collect();
    }
    let colors = (colors != Palette::default()).then_some(colors);
    let mut fmts = HashMap::new();
    if let Some(n) = root.child("numFmts") {
        for f in n.kids("numFmt") {
            if let (Some(id), Some(code)) = (f.attr_u32("numFmtId"), f.attr("formatCode")) {
                fmts.insert(id, code.to_string());
            }
        }
    }
    let fonts: Vec<Font> = root.child("fonts").map(|f| f.kids("font").map(|e| read_font(e, &palette)).collect()).unwrap_or_default();
    let fills: Vec<Fill> = root.child("fills").map(|f| f.kids("fill").map(|e| read_fill(e, &palette, false)).collect()).unwrap_or_default();
    let borders: Vec<Borders> = root.child("borders").map(|f| f.kids("border").map(|e| read_border(e, &palette)).collect()).unwrap_or_default();
    let xfs = root.child("cellXfs").map(|x| x.kids("xf").map(|xf| read_xf(xf, &fonts, &fills, &borders, &fmts)).collect()).unwrap_or_default();
    let xf_complements = root.child("cellXfs").map(|x| x.kids("xf").map(xf_complement).collect()).unwrap_or_default();
    let style_xfs: Vec<Style> =
        root.child("cellStyleXfs").map(|x| x.kids("xf").map(|xf| read_xf(xf, &fonts, &fills, &borders, &fmts)).collect()).unwrap_or_default();
    let style_xf_els: Vec<&El> = root.child("cellStyleXfs").map(|x| x.kids("xf").collect()).unwrap_or_default();
    let mut named = Vec::new();
    let mut named_info = std::collections::BTreeMap::new();
    if let Some(cs) = root.child("cellStyles") {
        for c in cs.kids("cellStyle") {
            // The Normal style (built-in 0, whatever the file's language calls it) is the
            // workbook's default style, read from the first cell format.
            let builtin = c.attr_u32("builtinId");
            if let (Some(name), Some(s)) = (c.attr("name"), c.attr_u32("xfId").and_then(|i| style_xfs.get(i as usize)))
                && name != "Normal"
                && builtin != Some(0)
            {
                named.push((name.to_string(), s.clone()));
                let xf = c.attr_u32("xfId").and_then(|i| style_xf_els.get(i as usize));
                let off = |flag: &str| xf.and_then(|x| x.attr_bool(flag)) == Some(false);
                let info = kition_sheet_model::CellStyleInfo {
                    builtin,
                    level: builtin.and(c.attr_u32("iLevel")),
                    custom_builtin: builtin.is_some() && c.flag("customBuiltin", false),
                    hidden: c.flag("hidden", false),
                    excludes: kition_sheet_model::StyleParts {
                        number_format: off("applyNumberFormat"),
                        font: off("applyFont"),
                        fill: off("applyFill"),
                        border: off("applyBorder"),
                        alignment: off("applyAlignment"),
                        protection: off("applyProtection"),
                    },
                };
                if info != kition_sheet_model::CellStyleInfo::default() {
                    named_info.insert(name.to_string(), info);
                }
            }
        }
    }
    let dxfs: Vec<Style> = root
        .child("dxfs")
        .map(|d| {
            d.kids("dxf")
                .map(|x| {
                    let mut s = Style::default();
                    if let Some(f) = x.child("font") {
                        let explicit = f.child("color").map(|c| read_color(c, &palette));
                        let mut f = read_font(f, &palette);
                        // A differential format's text colour 1 sets the colour; it is not
                        // "automatic" (which leaves the cell's colour).
                        if let Some(c @ Color::Theme(1, 0)) = explicit {
                            f.color = c;
                        }
                        // Differential fonts rarely name a face; keep the default face and size.
                        s.font = Font { name: s.font.name.clone(), size: s.font.size, charset: None, scheme: None, ..f };
                    }
                    if let Some(n) = x.child("numFmt") {
                        if let Some(code) = n.attr("formatCode") {
                            s.num_fmt = NumFmt::new(code);
                        } else if let Some(id) = n.attr_u32("numFmtId") {
                            s.num_fmt = num_fmt_code(id, &fmts);
                        }
                    }
                    if let Some(f) = x.child("fill") {
                        s.fill = read_fill(f, &palette, true);
                    }
                    if let Some(b) = x.child("border") {
                        s.borders = read_border(b, &palette);
                    }
                    if let Some(a) = x.child("alignment") {
                        s.align = read_alignment(a);
                    }
                    if let Some(p) = x.child("protection") {
                        s.protection = read_protection(p);
                    }
                    s
                })
                .collect()
        })
        .unwrap_or_default();
    let table_styles = read_table_styles(root, &dxfs);
    StylesIn { xfs, dxfs, palette, colors, named, named_info, num_fmts: fmts, table_styles, xf_complements }
}

/// Upper bound of the table styles and elements read from a file.
const MAX_TABLE_STYLES: usize = 1024;

/// `tableStyles`: the defaults it names and the styles the file defines, each element's format
/// taken from the differential formats.
fn read_table_styles(root: &El, dxfs: &[Style]) -> kition_sheet_model::TableStyles {
    let Some(t) = root.child("tableStyles") else { return Default::default() };
    let styles = t
        .kids("tableStyle")
        .take(MAX_TABLE_STYLES)
        .filter_map(|s| {
            let name = s.attr("name")?.to_string();
            let elements = s
                .kids("tableStyleElement")
                .take(64)
                .filter_map(|e| {
                    Some(kition_sheet_model::TableStyleElement {
                        kind: e.attr("type")?.to_string(),
                        size: e.attr_u32("size"),
                        style: e.attr_u32("dxfId").and_then(|i| dxfs.get(i as usize)).cloned().unwrap_or_default(),
                    })
                })
                .collect();
            Some(kition_sheet_model::TableStyleDef {
                name,
                table: s.attr_bool("table"),
                pivot: s.attr_bool("pivot"),
                uid: s.attr("uid").map(str::to_string),
                elements,
            })
        })
        .collect();
    kition_sheet_model::TableStyles {
        default_table: t.attr("defaultTableStyle").filter(|n| *n != "TableStyleMedium2").map(str::to_string),
        default_pivot: t.attr("defaultPivotStyle").filter(|n| *n != "PivotStyleLight16").map(str::to_string),
        styles,
    }
}

// ---------------------------------------------------------------- writing

pub fn color_attrs(c: &Color) -> String {
    match *c {
        Color::Auto => "auto=\"1\"".into(),
        Color::Rgb(v) => format!("rgb=\"FF{:06X}\"", v & 0xFFFFFF),
        Color::Theme(i, t) => {
            if t == 0 {
                format!("theme=\"{i}\"")
            } else {
                format!("theme=\"{i}\" tint=\"{}\"", num(t as f64 / 1000.0))
            }
        }
    }
}

fn font_xml(f: &Font, dxf: bool) -> String {
    let mut s = String::from("<font>");
    if f.bold {
        s.push_str("<b/>");
    }
    if f.italic {
        s.push_str("<i/>");
    }
    if f.strike {
        s.push_str("<strike/>");
    }
    match f.underline {
        Underline::None => {}
        Underline::Single => s.push_str("<u/>"),
        Underline::Double => s.push_str("<u val=\"double\"/>"),
        Underline::SingleAccounting => s.push_str("<u val=\"singleAccounting\"/>"),
        Underline::DoubleAccounting => s.push_str("<u val=\"doubleAccounting\"/>"),
    }
    match f.vert {
        VertAlign::Baseline => {}
        VertAlign::Superscript => s.push_str("<vertAlign val=\"superscript\"/>"),
        VertAlign::Subscript => s.push_str("<vertAlign val=\"subscript\"/>"),
    }
    if !dxf {
        let _ = write!(s, "<sz val=\"{}\"/>", num(f.size as f64));
    }
    if f.color != Color::Auto {
        let _ = write!(s, "<color {}/>", color_attrs(&f.color));
    } else if !dxf {
        s.push_str("<color theme=\"1\"/>");
    }
    if !dxf {
        let _ = write!(s, "<name val=\"{}\"/><family val=\"2\"/>", esc_attr(&f.name));
        if let Some(v) = f.charset {
            let _ = write!(s, "<charset val=\"{v}\"/>");
        }
        if let Some(v) = &f.scheme {
            let _ = write!(s, "<scheme val=\"{}\"/>", esc_attr(v));
        }
    }
    s.push_str("</font>");
    s
}

fn fill_xml(f: &Fill, dxf: bool) -> String {
    if f.pattern == PatternType::None {
        return "<fill><patternFill patternType=\"none\"/></fill>".into();
    }
    if dxf && f.pattern == PatternType::Solid {
        let c = color_attrs(&f.fg);
        return format!("<fill><patternFill patternType=\"solid\"><fgColor {c}/><bgColor {c}/></patternFill></fill>");
    }
    let mut s = format!("<fill><patternFill patternType=\"{}\">", pattern_name(f.pattern));
    if f.fg != Color::Auto {
        let _ = write!(s, "<fgColor {}/>", color_attrs(&f.fg));
    } else if f.pattern != PatternType::Gray125 {
        s.push_str("<fgColor indexed=\"64\"/>");
    }
    if f.bg != Color::Auto {
        let _ = write!(s, "<bgColor {}/>", color_attrs(&f.bg));
    } else {
        s.push_str("<bgColor indexed=\"64\"/>");
    }
    s.push_str("</patternFill></fill>");
    s
}

fn line_xml(tag: &str, l: &BorderLine) -> String {
    if l.is_none() {
        return format!("<{tag}/>");
    }
    let color = if l.color == Color::Auto { "<color indexed=\"64\"/>".to_string() } else { format!("<color {}/>", color_attrs(&l.color)) };
    format!("<{tag} style=\"{}\">{color}</{tag}>", border_style_name(l.style))
}

fn border_xml(b: &Borders) -> String {
    let mut s = String::from("<border");
    if !b.diag_up.is_none() {
        s.push_str(" diagonalUp=\"1\"");
    }
    if !b.diag_down.is_none() {
        s.push_str(" diagonalDown=\"1\"");
    }
    s.push('>');
    s.push_str(&line_xml("left", &b.left));
    s.push_str(&line_xml("right", &b.right));
    s.push_str(&line_xml("top", &b.top));
    s.push_str(&line_xml("bottom", &b.bottom));
    let diag = if !b.diag_down.is_none() { b.diag_down } else { b.diag_up };
    s.push_str(&line_xml("diagonal", &diag));
    for (name, line) in [("vertical", &b.vertical), ("horizontal", &b.horizontal)] {
        if !line.is_none() {
            s.push_str(&line_xml(name, line));
        }
    }
    s.push_str("</border>");
    s
}

fn alignment_xml(a: &Alignment) -> Option<String> {
    if *a == Alignment::default() {
        return None;
    }
    let mut s = String::from("<alignment");
    let h = match a.h {
        HAlign::General => None,
        HAlign::Left => Some("left"),
        HAlign::Center => Some("center"),
        HAlign::Right => Some("right"),
        HAlign::Fill => Some("fill"),
        HAlign::Justify => Some("justify"),
        HAlign::CenterAcross => Some("centerContinuous"),
        HAlign::Distributed => Some("distributed"),
    };
    if let Some(h) = h {
        let _ = write!(s, " horizontal=\"{h}\"");
    }
    let v = match a.v {
        VAlign::Bottom => None,
        VAlign::Top => Some("top"),
        VAlign::Center => Some("center"),
        VAlign::Justify => Some("justify"),
        VAlign::Distributed => Some("distributed"),
    };
    if let Some(v) = v {
        let _ = write!(s, " vertical=\"{v}\"");
    }
    let rot = match a.rotation {
        255 => 255,
        r if (0..=90).contains(&r) => r,
        r if (-90..0).contains(&r) => 90 - r,
        _ => 0,
    };
    if rot != 0 {
        let _ = write!(s, " textRotation=\"{rot}\"");
    }
    if a.wrap {
        s.push_str(" wrapText=\"1\"");
    }
    if a.indent > 0 {
        let _ = write!(s, " indent=\"{}\"", a.indent);
    }
    if a.shrink {
        s.push_str(" shrinkToFit=\"1\"");
    }
    s.push_str("/>");
    Some(s)
}

fn protection_xml(p: &Protection) -> Option<String> {
    if *p == Protection::default() {
        return None;
    }
    Some(format!("<protection locked=\"{}\" hidden=\"{}\"/>", p.locked as u8, p.hidden as u8))
}

/// Interns `items` into a deduplicated list, returning the index.
fn intern<T: Clone + Eq + std::hash::Hash>(list: &mut Vec<T>, map: &mut HashMap<T, u32>, item: &T) -> u32 {
    if let Some(&i) = map.get(item) {
        return i;
    }
    let i = list.len() as u32;
    list.push(item.clone());
    map.insert(item.clone(), i);
    i
}

/// Number format ids assigned while writing.
struct NumFmts {
    custom: Vec<(u32, String)>,
    map: HashMap<String, u32>,
}

impl NumFmts {
    fn id(&mut self, code: &str) -> u32 {
        if let Some(id) = builtin_id(code) {
            return id;
        }
        if let Some(&id) = self.map.get(code) {
            return id;
        }
        let id = 164 + self.custom.len() as u32;
        self.custom.push((id, code.to_string()));
        self.map.insert(code.to_string(), id);
        id
    }
}

/// `styles.xml` for the workbook: one cell format per model style, in `StyleId` order (so a
/// cell's `s` attribute is its style id), plus the given differential formats.
/// `extra_fmts` are number formats used outside cell styles (pivot table value fields); the
/// returned map gives the id of every custom format written.
pub fn write_styles(wb: &Workbook, dxfs: &[Style], extra_fmts: &[String]) -> (String, HashMap<String, u32>) {
    let mut nf = NumFmts { custom: vec![], map: HashMap::new() };
    let mut fonts: Vec<Font> = vec![];
    let mut font_map = HashMap::new();
    let mut fills: Vec<Fill> = vec![Fill::default(), Fill { pattern: PatternType::Gray125, fg: Color::Auto, bg: Color::Auto }];
    let mut fill_map: HashMap<Fill, u32> = fills.iter().enumerate().map(|(i, f)| (*f, i as u32)).collect();
    let mut borders: Vec<Borders> = vec![Borders::default()];
    let mut border_map: HashMap<Borders, u32> = HashMap::from([(Borders::default(), 0)]);
    // The default style's font is font 0.
    intern(&mut fonts, &mut font_map, &wb.styles.get(kition_sheet_model::StyleId::DEFAULT).font);

    let mut xfs = String::new();
    let mut count = 0;
    for (_, st) in wb.styles.iter() {
        let font = intern(&mut fonts, &mut font_map, &st.font);
        let fill = intern(&mut fills, &mut fill_map, &st.fill);
        let border = intern(&mut borders, &mut border_map, &st.borders);
        let fmt = nf.id(st.num_fmt.as_str());
        let _ = write!(xfs, "<xf numFmtId=\"{fmt}\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\"");
        if fmt != 0 {
            xfs.push_str(" applyNumberFormat=\"1\"");
        }
        if font != 0 {
            xfs.push_str(" applyFont=\"1\"");
        }
        if fill != 0 {
            xfs.push_str(" applyFill=\"1\"");
        }
        if border != 0 {
            xfs.push_str(" applyBorder=\"1\"");
        }
        let al = alignment_xml(&st.align);
        let pr = protection_xml(&st.protection);
        if al.is_some() {
            xfs.push_str(" applyAlignment=\"1\"");
        }
        if pr.is_some() {
            xfs.push_str(" applyProtection=\"1\"");
        }
        if al.is_none() && pr.is_none() && !st.checkbox {
            xfs.push_str("/>");
        } else {
            xfs.push('>');
            xfs.push_str(&al.unwrap_or_default());
            xfs.push_str(&pr.unwrap_or_default());
            // A checkbox: the cell format's complement names the Checkbox bag (FPB_CHECKBOX).
            if st.checkbox {
                let _ = write!(xfs, "<extLst><ext uri=\"{XF_COMPLEMENT_EXT}\" xmlns:xfpb=\"{NS_FPB}\"><xfpb:xfComplement i=\"0\"/></ext></extLst>");
            }
            xfs.push_str("</xf>");
        }
        count += 1;
    }

    // Named cell styles.
    let mut style_xfs = String::from("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/>");
    let mut cell_styles = String::from("<cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/>");
    let mut named = 1;
    for (name, st) in &wb.cell_styles {
        if name.is_empty() || name == "Normal" {
            continue;
        }
        let font = intern(&mut fonts, &mut font_map, &st.font);
        let fill = intern(&mut fills, &mut fill_map, &st.fill);
        let border = intern(&mut borders, &mut border_map, &st.borders);
        let fmt = nf.id(st.num_fmt.as_str());
        let _ = write!(style_xfs, "<xf numFmtId=\"{fmt}\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\"");
        // The parts the style leaves out ("Style includes" cleared).
        if let Some(x) = wb.cell_style_info.get(name).map(|i| i.excludes) {
            for (off, flag) in [
                (x.number_format, "applyNumberFormat"),
                (x.font, "applyFont"),
                (x.fill, "applyFill"),
                (x.border, "applyBorder"),
                (x.alignment, "applyAlignment"),
                (x.protection, "applyProtection"),
            ] {
                if off {
                    let _ = write!(style_xfs, " {flag}=\"0\"");
                }
            }
        }
        let al = alignment_xml(&st.align);
        let pr = protection_xml(&st.protection);
        if al.is_none() && pr.is_none() {
            style_xfs.push_str("/>");
        } else {
            let _ = write!(style_xfs, ">{}{}</xf>", al.unwrap_or_default(), pr.unwrap_or_default());
        }
        let _ = write!(cell_styles, "<cellStyle name=\"{}\" xfId=\"{named}\"", esc_attr(name));
        // A built-in style keeps its id (so applications name it in their own language); the
        // workbook's Normal style is written above.
        let info = wb.cell_style_info.get(name);
        if let Some(id) = info.and_then(|i| i.builtin).filter(|&id| id != 0) {
            let _ = write!(cell_styles, " builtinId=\"{id}\"");
            if let Some(level) = info.and_then(|i| i.level).filter(|_| matches!(id, 1 | 2)) {
                let _ = write!(cell_styles, " iLevel=\"{}\"", level.min(6));
            }
        }
        if info.is_some_and(|i| i.hidden) {
            cell_styles.push_str(" hidden=\"1\"");
        }
        if info.is_some_and(|i| i.custom_builtin && i.builtin.is_some_and(|id| id != 0)) {
            cell_styles.push_str(" customBuiltin=\"1\"");
        }
        cell_styles.push_str("/>");
        named += 1;
    }

    // The file's own table styles, their formats after the given differential formats.
    let mut table_styles = String::new();
    let mut dxfs = dxfs.to_vec();
    let ts = &wb.table_styles;
    let _ = write!(
        table_styles,
        "<tableStyles count=\"{}\" defaultTableStyle=\"{}\" defaultPivotStyle=\"{}\"",
        ts.styles.len(),
        esc_attr(ts.default_table.as_deref().unwrap_or("TableStyleMedium2")),
        esc_attr(ts.default_pivot.as_deref().unwrap_or("PivotStyleLight16"))
    );
    if ts.styles.is_empty() {
        table_styles.push_str("/>");
    } else {
        table_styles.push('>');
        for t in &ts.styles {
            let _ = write!(table_styles, "<tableStyle name=\"{}\"", esc_attr(&t.name));
            for (flag, v) in [("pivot", t.pivot), ("table", t.table)] {
                if let Some(v) = v {
                    let _ = write!(table_styles, " {flag}=\"{}\"", u8::from(v));
                }
            }
            let _ = write!(table_styles, " count=\"{}\"", t.elements.len());
            if let Some(uid) = &t.uid {
                let _ = write!(
                    table_styles,
                    " xmlns:xr9=\"http://schemas.microsoft.com/office/spreadsheetml/2016/revision9\" xr9:uid=\"{}\"",
                    esc_attr(uid)
                );
            }
            table_styles.push('>');
            for e in &t.elements {
                let id = dxf_id(&mut dxfs, &e.style);
                let _ = write!(table_styles, "<tableStyleElement type=\"{}\"", esc_attr(&e.kind));
                if let Some(size) = e.size {
                    let _ = write!(table_styles, " size=\"{size}\"");
                }
                let _ = write!(table_styles, " dxfId=\"{id}\"/>");
            }
            table_styles.push_str("</tableStyle>");
        }
        table_styles.push_str("</tableStyles>");
    }

    let mut dx = String::new();
    for d in &dxfs {
        dx.push_str("<dxf>");
        let def = Style::default();
        if d.font.bold
            || d.font.italic
            || d.font.strike
            || d.font.underline != Underline::None
            || d.font.color != Color::Auto
            || d.font.vert != VertAlign::Baseline
        {
            dx.push_str(&font_xml(&d.font, true));
        }
        if d.num_fmt != def.num_fmt {
            let id = nf.id(d.num_fmt.as_str());
            let _ = write!(dx, "<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", esc_attr(d.num_fmt.as_str()));
        }
        if d.fill != def.fill {
            dx.push_str(&fill_xml(&d.fill, true));
        }
        if let Some(a) = alignment_xml(&d.align) {
            dx.push_str(&a);
        }
        if d.borders != def.borders {
            dx.push_str(&border_xml(&d.borders));
        }
        dx.push_str("</dxf>");
    }
    for f in extra_fmts {
        nf.id(f);
    }

    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    );
    if !nf.custom.is_empty() {
        let _ = write!(s, "<numFmts count=\"{}\">", nf.custom.len());
        for (id, code) in &nf.custom {
            let _ = write!(s, "<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", esc_attr(code));
        }
        s.push_str("</numFmts>");
    }
    let _ = write!(s, "<fonts count=\"{}\">", fonts.len());
    for f in &fonts {
        s.push_str(&font_xml(f, false));
    }
    let _ = write!(s, "</fonts><fills count=\"{}\">", fills.len());
    for f in &fills {
        s.push_str(&fill_xml(f, false));
    }
    let _ = write!(s, "</fills><borders count=\"{}\">", borders.len());
    for b in &borders {
        s.push_str(&border_xml(b));
    }
    let _ = write!(s, "</borders><cellStyleXfs count=\"{named}\">{style_xfs}</cellStyleXfs>");
    let _ = write!(s, "<cellXfs count=\"{count}\">{xfs}</cellXfs>");
    let _ = write!(s, "<cellStyles count=\"{named}\">{cell_styles}</cellStyles>");
    let _ = write!(s, "<dxfs count=\"{}\">{dx}</dxfs>", dxfs.len());
    s.push_str(&table_styles);
    if let Some(p) = wb.palette.as_ref().filter(|p| !p.indexed.is_empty() || !p.recent.is_empty()) {
        s.push_str("<colors>");
        if !p.indexed.is_empty() {
            s.push_str("<indexedColors>");
            for c in &p.indexed {
                let _ = write!(s, "<rgbColor rgb=\"FF{:06X}\"/>", c & 0xFF_FFFF);
            }
            s.push_str("</indexedColors>");
        }
        if !p.recent.is_empty() {
            s.push_str("<mruColors>");
            for c in &p.recent {
                crate::rich::write_run_color(&mut s, c);
            }
            s.push_str("</mruColors>");
        }
        s.push_str("</colors>");
    }
    if !wb.extensions.styles.is_empty() {
        let _ = write!(s, "<extLst>{}</extLst>", wb.extensions.styles.concat());
    }
    s.push_str("</styleSheet>");
    (s, nf.map)
}

/// Index of a differential format, adding it when new.
pub fn dxf_id(dxfs: &mut Vec<Style>, s: &Style) -> u32 {
    if let Some(i) = dxfs.iter().position(|d| d == s) {
        return i as u32;
    }
    dxfs.push(s.clone());
    (dxfs.len() - 1) as u32
}
