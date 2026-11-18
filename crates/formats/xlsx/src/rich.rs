//! Rich text in string items (`<si>` of the shared strings, a cell's inline `<is>`): runs
//! `<r><rPr>…</rPr><t>…</t></r>` read into [`RichText`] and written back.

use std::fmt::Write as _;
use std::sync::Arc;

use kition_sheet_model::{RichText, RunColor, RunFont, TextRun, Underline, VertAlign};

use crate::xml::{self, El, esc, esc_attr, num};

/// A string item's text, and its runs when it has rich text (runs that set something).
pub fn read_item(e: &El) -> (Arc<str>, Option<Arc<RichText>>) {
    if !e.children.iter().any(|c| c.name == "r") {
        return (Arc::from(xml::rich_text(e)), None);
    }
    let mut runs = Vec::new();
    for c in &e.children {
        match c.name.as_str() {
            "t" => runs.push(TextRun::new(xml::decode_xstring(&c.text), RunFont::default())),
            "r" => {
                let text: String = c.kids("t").map(|t| t.text.as_str()).collect();
                let font = c.child("rPr").map(read_font).unwrap_or_default();
                runs.push(TextRun::new(xml::decode_xstring(&text), font));
            }
            _ => {}
        }
    }
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    (Arc::from(text), RichText::new(runs).map(Arc::new))
}

fn read_font(rpr: &El) -> RunFont {
    let mut f = RunFont::default();
    for p in &rpr.children {
        let val = p.attr("val");
        let flag = || p.attr_bool("val").or(Some(true));
        match p.name.as_str() {
            "rFont" => f.name = val.map(str::to_owned),
            "sz" => f.size = p.attr_f64("val"),
            "b" => f.bold = flag(),
            "i" => f.italic = flag(),
            "strike" => f.strike = flag(),
            "outline" => f.outline = flag(),
            "shadow" => f.shadow = flag(),
            "condense" => f.condense = flag(),
            "extend" => f.extend = flag(),
            "u" => {
                f.underline = Some(match val.unwrap_or("single") {
                    "double" => Underline::Double,
                    "singleAccounting" => Underline::SingleAccounting,
                    "doubleAccounting" => Underline::DoubleAccounting,
                    "none" => Underline::None,
                    _ => Underline::Single,
                })
            }
            "vertAlign" => {
                f.vert = match val {
                    Some("superscript") => Some(VertAlign::Superscript),
                    Some("subscript") => Some(VertAlign::Subscript),
                    Some("baseline") => Some(VertAlign::Baseline),
                    _ => None,
                }
            }
            "family" => f.family = p.attr_u32("val"),
            "charset" => f.charset = p.attr_u32("val"),
            "scheme" => f.scheme = val.map(str::to_owned),
            "color" => f.color = Some(read_run_color(p)),
            _ => {}
        }
    }
    f
}

/// A `<color>` as written.
pub fn read_run_color(e: &El) -> RunColor {
    RunColor {
        auto: e.attr_bool("auto"),
        rgb: e.attr("rgb").map(str::to_owned),
        theme: e.attr_u32("theme"),
        indexed: e.attr_u32("indexed"),
        tint: e.attr_f64("tint"),
    }
}

/// `<color …/>` saying what `c` says.
pub fn write_run_color(out: &mut String, c: &RunColor) {
    out.push_str("<color");
    if let Some(v) = c.auto {
        let _ = write!(out, " auto=\"{}\"", u8::from(v));
    }
    if let Some(v) = &c.rgb {
        let _ = write!(out, " rgb=\"{}\"", esc_attr(v));
    }
    if let Some(v) = c.theme {
        let _ = write!(out, " theme=\"{v}\"");
    }
    if let Some(v) = c.indexed {
        let _ = write!(out, " indexed=\"{v}\"");
    }
    if let Some(v) = c.tint {
        let _ = write!(out, " tint=\"{}\"", num(v));
    }
    out.push_str("/>");
}

/// The children of a string item for `text`: its runs when `rich` spells it, else one `<t>`.
pub fn write_item(out: &mut String, text: &str, rich: Option<&RichText>) {
    let Some(rich) = rich.filter(|r| r.spells(text)) else {
        out.push_str(&xml::t_el(text));
        return;
    };
    for run in &rich.runs {
        out.push_str("<r>");
        if !run.font.is_empty() {
            write_font(out, &run.font);
        }
        // Excel keeps every run's spaces: a run boundary often falls next to one.
        let _ = write!(out, "<t xml:space=\"preserve\">{}</t></r>", esc(&xml::encode_xstring(&run.text)));
    }
}

/// `<rPr>`, its elements in the order Excel writes them.
fn write_font(out: &mut String, f: &RunFont) {
    out.push_str("<rPr>");
    let flag = |out: &mut String, name: &str, v: Option<bool>| match v {
        Some(true) => {
            let _ = write!(out, "<{name}/>");
        }
        Some(false) => {
            let _ = write!(out, "<{name} val=\"0\"/>");
        }
        None => {}
    };
    flag(out, "b", f.bold);
    flag(out, "i", f.italic);
    flag(out, "strike", f.strike);
    flag(out, "condense", f.condense);
    flag(out, "extend", f.extend);
    flag(out, "outline", f.outline);
    flag(out, "shadow", f.shadow);
    if let Some(u) = f.underline {
        let val = match u {
            Underline::Single => None,
            Underline::Double => Some("double"),
            Underline::SingleAccounting => Some("singleAccounting"),
            Underline::DoubleAccounting => Some("doubleAccounting"),
            Underline::None => Some("none"),
        };
        match val {
            Some(v) => {
                let _ = write!(out, "<u val=\"{v}\"/>");
            }
            None => out.push_str("<u/>"),
        }
    }
    if let Some(v) = f.vert {
        let val = match v {
            VertAlign::Superscript => "superscript",
            VertAlign::Subscript => "subscript",
            VertAlign::Baseline => "baseline",
        };
        let _ = write!(out, "<vertAlign val=\"{val}\"/>");
    }
    if let Some(v) = f.size.filter(|v| v.is_finite()) {
        let _ = write!(out, "<sz val=\"{}\"/>", num(v));
    }
    if let Some(c) = &f.color {
        write_run_color(out, c);
    }
    if let Some(name) = &f.name {
        let _ = write!(out, "<rFont val=\"{}\"/>", esc_attr(name));
    }
    if let Some(v) = f.family {
        let _ = write!(out, "<family val=\"{v}\"/>");
    }
    if let Some(v) = f.charset {
        let _ = write!(out, "<charset val=\"{v}\"/>");
    }
    if let Some(s) = &f.scheme {
        let _ = write!(out, "<scheme val=\"{}\"/>", esc_attr(s));
    }
    out.push_str("</rPr>");
}
