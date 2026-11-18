//! A rich text cell's runs as marks on its `Cell`'s `text` field (plan K5).
//!
//! Each property a run sets is one mark key (schema `Cell.text.marks`), so two people formatting
//! different properties or different characters of one cell both keep theirs (rule M19):
//! `bold`, `italic`, `strike` (booleans), `underline` (`single`, `double`, `singleAccounting`,
//! `doubleAccounting`, `none`), `font` (the face name), `size` (points), `color` (`{rgb?, theme?,
//! indexed?, tint?, auto?}` as the file names it), `vertAlign` (`superscript`, `subscript`,
//! `baseline`), `family` and `charset` (numbers), `scheme` (`major`, `minor`), and `extra` for
//! the rest (`{outline?, shadow?, condense?, extend?}`). Every key expands `after`: typing at the
//! end of a run continues it, as typing at a caret does.
//!
//! `family`, `charset`, `scheme` and `extra` came after 0.4.x, which declares the first eight
//! keys only: such a reader drops the four from what it reads (the KDM reads declared marks only)
//! and keeps them in the replica, and its edits, which write characters, leave them on the
//! characters they cover. So the keys are additive: the schema version is unchanged.

use std::collections::BTreeMap;

use kition_kit::session::TextRun as Marked;
use kition_sheet_model::{Cell, RichText, RunColor, RunFont, TextRun, Underline, VertAlign};
use serde_json::{Map, Value, json};

/// The marks a run's font sets.
pub(crate) fn font_marks(f: &RunFont) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut put = |key: &str, v: Option<Value>| {
        if let Some(v) = v {
            out.insert(key.to_owned(), v);
        }
    };
    put("bold", f.bold.map(Value::Bool));
    put("italic", f.italic.map(Value::Bool));
    put("strike", f.strike.map(Value::Bool));
    put("underline", f.underline.map(|u| json!(underline_name(u))));
    put("font", f.name.clone().map(Value::String));
    put("size", f.size.filter(|s| s.is_finite()).map(|s| json!(s)));
    put("color", f.color.as_ref().map(color_value).filter(|c| c.as_object().is_some_and(|m| !m.is_empty())));
    put("vertAlign", f.vert.map(|v| json!(vert_name(v))));
    put("family", f.family.map(|v| json!(v)));
    put("charset", f.charset.map(|v| json!(v)));
    put("scheme", f.scheme.clone().map(Value::String));
    let mut extra = Map::new();
    for (key, v) in [("outline", f.outline), ("shadow", f.shadow), ("condense", f.condense), ("extend", f.extend)] {
        if let Some(v) = v {
            extra.insert(key.to_owned(), Value::Bool(v));
        }
    }
    if !extra.is_empty() {
        out.insert("extra".to_owned(), Value::Object(extra));
    }
    out
}

/// The font a run's marks set; a mark whose value this version cannot read is left out.
pub(crate) fn marks_font(marks: &BTreeMap<String, Value>) -> RunFont {
    let flag = |key: &str| marks.get(key).and_then(Value::as_bool);
    let string = |key: &str| marks.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| marks.get(key).and_then(Value::as_u64).and_then(|v| u32::try_from(v).ok());
    let extra = |key: &str| marks.get("extra").and_then(|e| e.get(key)).and_then(Value::as_bool);
    RunFont {
        name: string("font"),
        size: marks.get("size").and_then(Value::as_f64).filter(|s| s.is_finite()),
        bold: flag("bold"),
        italic: flag("italic"),
        underline: marks.get("underline").and_then(Value::as_str).and_then(underline_of),
        strike: flag("strike"),
        color: marks.get("color").and_then(color_of),
        vert: marks.get("vertAlign").and_then(Value::as_str).and_then(vert_of),
        family: number("family"),
        charset: number("charset"),
        scheme: string("scheme"),
        outline: extra("outline"),
        shadow: extra("shadow"),
        condense: extra("condense"),
        extend: extra("extend"),
    }
}

fn underline_name(u: Underline) -> &'static str {
    match u {
        Underline::None => "none",
        Underline::Single => "single",
        Underline::Double => "double",
        Underline::SingleAccounting => "singleAccounting",
        Underline::DoubleAccounting => "doubleAccounting",
    }
}

fn underline_of(s: &str) -> Option<Underline> {
    Some(match s {
        "none" => Underline::None,
        "single" => Underline::Single,
        "double" => Underline::Double,
        "singleAccounting" => Underline::SingleAccounting,
        "doubleAccounting" => Underline::DoubleAccounting,
        _ => return None,
    })
}

fn vert_name(v: VertAlign) -> &'static str {
    match v {
        VertAlign::Baseline => "baseline",
        VertAlign::Superscript => "superscript",
        VertAlign::Subscript => "subscript",
    }
}

fn vert_of(s: &str) -> Option<VertAlign> {
    Some(match s {
        "baseline" => VertAlign::Baseline,
        "superscript" => VertAlign::Superscript,
        "subscript" => VertAlign::Subscript,
        _ => return None,
    })
}

fn color_value(c: &RunColor) -> Value {
    let mut m = Map::new();
    if let Some(v) = c.auto {
        m.insert("auto".into(), Value::Bool(v));
    }
    if let Some(v) = &c.rgb {
        m.insert("rgb".into(), Value::String(v.clone()));
    }
    if let Some(v) = c.theme {
        m.insert("theme".into(), json!(v));
    }
    if let Some(v) = c.indexed {
        m.insert("indexed".into(), json!(v));
    }
    if let Some(v) = c.tint.filter(|t| t.is_finite()) {
        m.insert("tint".into(), json!(v));
    }
    Value::Object(m)
}

fn color_of(v: &Value) -> Option<RunColor> {
    let m = v.as_object()?;
    let number = |key: &str| m.get(key).and_then(Value::as_u64).and_then(|v| u32::try_from(v).ok());
    let c = RunColor {
        auto: m.get("auto").and_then(Value::as_bool),
        rgb: m.get("rgb").and_then(Value::as_str).map(str::to_owned),
        theme: number("theme"),
        indexed: number("indexed"),
        tint: m.get("tint").and_then(Value::as_f64).filter(|t| t.is_finite()),
    };
    (c != RunColor::default()).then_some(c)
}

/// A cell's runs as marked runs: empty unless it is a rich text cell ([`Cell::runs`]).
pub(crate) fn cell_runs(cell: Option<&Cell>) -> Vec<Marked> {
    let Some(rich) = cell.and_then(Cell::runs) else { return Vec::new() };
    rich.runs.iter().map(|r| Marked { insert: r.text.clone(), attributes: font_marks(&r.font) }).collect()
}

/// A text field's marked runs as rich text (`None`: no run sets anything this version reads).
pub(crate) fn rich_of(runs: &[Marked]) -> Option<RichText> {
    if runs.iter().all(|r| r.attributes.is_empty()) {
        return None;
    }
    RichText::new(runs.iter().map(|r| TextRun::new(r.insert.clone(), marks_font(&r.attributes))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_property_is_a_mark_and_reads_back() {
        let f = RunFont {
            name: Some("Cambria".into()),
            size: Some(10.5),
            bold: Some(true),
            italic: Some(false),
            underline: Some(Underline::DoubleAccounting),
            strike: Some(true),
            color: Some(RunColor { theme: Some(4), tint: Some(-0.249977111117893), ..RunColor::default() }),
            vert: Some(VertAlign::Subscript),
            family: Some(2),
            charset: Some(134),
            scheme: Some("minor".into()),
            outline: Some(true),
            shadow: Some(false),
            condense: None,
            extend: Some(true),
        };
        let marks = font_marks(&f);
        assert_eq!(marks.len(), 12, "{marks:?}");
        assert_eq!(marks.get("extra"), Some(&json!({ "outline": true, "shadow": false, "extend": true })));
        assert_eq!(marks_font(&marks), f);
        assert!(font_marks(&RunFont::default()).is_empty());
        // Values this version cannot read are left out.
        let odd: BTreeMap<String, Value> =
            [("bold".to_owned(), json!("yes")), ("underline".to_owned(), json!("wavy")), ("italic".to_owned(), json!(true))].into_iter().collect();
        assert_eq!(marks_font(&odd), RunFont { italic: Some(true), ..RunFont::default() });
    }
}
