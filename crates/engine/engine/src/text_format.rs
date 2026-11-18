//! Formatting part of a text cell's characters (rich text): what `cell.formatText` and the
//! in-cell editor of the kit's web build do to a selection of characters.
//!
//! The format is given as command parameters: `bold`, `italic`, `strike` (booleans), `underline`
//! (`true`, `false` or an Excel underline style), `vertAlign` (`superscript`, `subscript`,
//! `baseline`), `size` (points, 1–409), `name` (a face), `color` (`"#C00000"`, `{theme, tint}`, or
//! `"auto"`: the cell's colour), and `toggle` (`bold`, `italic`, `underline`, `strike`,
//! `superscript`, `subscript`: on unless every selected character has it already, as the Bold
//! button does). A property set to what the cell's own font says is left to the cell, so turning
//! bold on and off again leaves plain text.

use kition_sheet_model::{Color, Font, RichText, RunColor, RunFont, Underline, VertAlign};
use serde_json::Value as Json;

use crate::cmd::format::color_param;
use crate::{EngineError, Result};

/// The keys of a format; a request without any of them formats nothing.
pub const FORMAT_KEYS: &[&str] = &["bold", "italic", "strike", "underline", "vertAlign", "size", "name", "color", "toggle"];

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: "cell.formatText".into(), msg: msg.into() }
}

/// One property change, resolved against the selection.
enum Change {
    Bold(bool),
    Italic(bool),
    Strike(bool),
    Underline(Underline),
    Vert(VertAlign),
    Size(f64),
    Name(String),
    Color(Color),
}

fn underline_of(v: &Json) -> Option<Underline> {
    match v {
        Json::Bool(true) => Some(Underline::Single),
        Json::Bool(false) => Some(Underline::None),
        Json::String(s) => match s.as_str() {
            "single" => Some(Underline::Single),
            "double" => Some(Underline::Double),
            "singleAccounting" => Some(Underline::SingleAccounting),
            "doubleAccounting" => Some(Underline::DoubleAccounting),
            "none" => Some(Underline::None),
            _ => None,
        },
        _ => None,
    }
}

/// The changes `p` asks for, a toggle resolved against the characters `from..to`.
fn changes(runs: Option<&RichText>, text: &str, base: &Font, from: usize, to: usize, p: &Json) -> Result<Vec<Change>> {
    let mut out = Vec::new();
    let every = |test: &dyn Fn(&Font) -> bool| RichText::every(runs, text, base, from, to, test);
    if let Some(toggle) = p.get("toggle") {
        let Some(name) = toggle.as_str() else { return Err(bad("`toggle` is a property name")) };
        out.push(match name {
            "bold" => Change::Bold(!every(&|f| f.bold)),
            "italic" => Change::Italic(!every(&|f| f.italic)),
            "strike" | "strikethrough" => Change::Strike(!every(&|f| f.strike)),
            "underline" => Change::Underline(if every(&|f| f.underline != Underline::None) { Underline::None } else { Underline::Single }),
            "superscript" => Change::Vert(if every(&|f| f.vert == VertAlign::Superscript) { VertAlign::Baseline } else { VertAlign::Superscript }),
            "subscript" => Change::Vert(if every(&|f| f.vert == VertAlign::Subscript) { VertAlign::Baseline } else { VertAlign::Subscript }),
            other => return Err(bad(format!("cannot toggle `{other}`"))),
        });
    }
    for (key, make) in [("bold", Change::Bold as fn(bool) -> Change), ("italic", Change::Italic), ("strike", Change::Strike)] {
        if let Some(v) = p.get(key) {
            out.push(make(v.as_bool().ok_or_else(|| bad(format!("`{key}` is true or false")))?));
        }
    }
    if let Some(v) = p.get("underline") {
        out.push(Change::Underline(underline_of(v).ok_or_else(|| bad("`underline` is true, false or an underline style"))?));
    }
    if let Some(v) = p.get("vertAlign") {
        out.push(Change::Vert(match v.as_str() {
            Some("superscript") => VertAlign::Superscript,
            Some("subscript") => VertAlign::Subscript,
            Some("baseline") => VertAlign::Baseline,
            _ => return Err(bad("`vertAlign` is superscript, subscript or baseline")),
        }));
    }
    if let Some(v) = p.get("size") {
        let size = v.as_f64().filter(|s| (1.0..=409.0).contains(s)).ok_or_else(|| bad("`size` is a number of points from 1 to 409"))?;
        out.push(Change::Size((size * 2.0).round() / 2.0));
    }
    if let Some(v) = p.get("name") {
        let name = v.as_str().map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 255).ok_or_else(|| bad("`name` is a font name"))?;
        out.push(Change::Name(name.to_owned()));
    }
    if let Some(v) = p.get("color") {
        out.push(Change::Color(color_param(Some(v)).ok_or_else(|| bad("`color` is \"#RRGGBB\", {theme, tint} or \"auto\""))?));
    }
    if out.is_empty() {
        return Err(bad(format!("nothing to format: give one of {}", FORMAT_KEYS.join(", "))));
    }
    Ok(out)
}

/// A run's colour for a model colour (`None`: automatic, the cell's colour).
fn run_color(c: Color) -> Option<RunColor> {
    match c {
        Color::Auto => None,
        Color::Rgb(v) => Some(RunColor { rgb: Some(format!("FF{:06X}", v & 0xFF_FFFF)), ..RunColor::default() }),
        Color::Theme(i, tint) => {
            Some(RunColor { theme: Some(u32::from(i)), tint: (tint != 0).then(|| f64::from(tint) / 1000.0), ..RunColor::default() })
        }
    }
}

/// Sets `v` on a run, or leaves it to the cell when the cell's font says the same.
fn set<T: PartialEq>(slot: &mut Option<T>, v: T, cell: &T) {
    *slot = if v == *cell { None } else { Some(v) };
}

/// The runs of `text` after formatting its characters `from..to` as `p` says (see the module
/// docs); `runs` are its runs now and `base` the cell's font. `None` when the text is plain after.
pub fn format_runs(runs: Option<&RichText>, text: &str, base: &Font, from: usize, to: usize, p: &Json) -> Result<Option<RichText>> {
    let changes = changes(runs, text, base, from, to, p)?;
    Ok(RichText::format(runs, text, from, to, |font: &mut RunFont| {
        for change in &changes {
            match change {
                Change::Bold(v) => set(&mut font.bold, *v, &base.bold),
                Change::Italic(v) => set(&mut font.italic, *v, &base.italic),
                Change::Strike(v) => set(&mut font.strike, *v, &base.strike),
                Change::Underline(v) => set(&mut font.underline, *v, &base.underline),
                Change::Vert(v) => set(&mut font.vert, *v, &base.vert),
                Change::Size(v) => set(&mut font.size, *v, &f64::from(base.size)),
                Change::Name(v) => {
                    set(&mut font.name, v.clone(), &base.name);
                    font.family = None;
                    font.charset = None;
                    font.scheme = None;
                }
                Change::Color(c) => font.color = if *c == base.color { None } else { run_color(*c) },
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_sheet_model::TextRun;
    use serde_json::json;

    fn runs(r: &Option<RichText>) -> Vec<(String, RunFont)> {
        r.as_ref().map(|r| r.runs.iter().map(|TextRun { text, font }| (text.clone(), font.clone())).collect()).unwrap_or_default()
    }

    #[test]
    fn toggles_follow_the_selection_and_the_cells_font() {
        let base = Font::default();
        let bold = format_runs(None, "Hello", &base, 0, 2, &json!({"toggle": "bold"})).unwrap();
        assert_eq!(runs(&bold)[0], ("He".to_owned(), RunFont { bold: Some(true), ..RunFont::default() }));
        // Toggling again turns it off: plain text again.
        assert_eq!(format_runs(bold.as_ref(), "Hello", &base, 0, 2, &json!({"toggle": "bold"})).unwrap(), None);
        // Partly bold: the toggle turns the whole selection bold.
        let more = format_runs(bold.as_ref(), "Hello", &base, 1, 4, &json!({"toggle": "bold"})).unwrap();
        assert_eq!(runs(&more)[0], ("Hell".to_owned(), RunFont { bold: Some(true), ..RunFont::default() }));
        // In a bold cell, un-bolding characters writes bold off on them.
        let cell = Font { bold: true, ..Font::default() };
        let off = format_runs(None, "Hello", &cell, 0, 1, &json!({"toggle": "bold"})).unwrap();
        assert_eq!(runs(&off)[0], ("H".to_owned(), RunFont { bold: Some(false), ..RunFont::default() }));
    }

    #[test]
    fn sets_colour_size_face_and_underline() {
        let base = Font::default();
        let r =
            format_runs(None, "abc", &base, 1, 3, &json!({"color": "#C00000", "size": 14, "name": "Georgia", "underline": "double", "italic": true}))
                .unwrap();
        let font = &runs(&r)[1].1;
        assert_eq!(font.color.as_ref().and_then(|c| c.rgb.as_deref()), Some("FFC00000"));
        assert_eq!(
            (font.size, font.name.as_deref(), font.underline, font.italic),
            (Some(14.0), Some("Georgia"), Some(Underline::Double), Some(true))
        );
        // `auto` hands the colour back to the cell.
        let back = format_runs(r.as_ref(), "abc", &base, 0, 3, &json!({"color": "auto"})).unwrap();
        assert!(runs(&back).iter().all(|(_, f)| f.color.is_none()));
        let theme = format_runs(None, "abc", &base, 0, 1, &json!({"color": {"theme": 4, "tint": -0.25}})).unwrap();
        assert_eq!(runs(&theme)[0].1.color, Some(RunColor { theme: Some(4), tint: Some(-0.25), ..RunColor::default() }));
    }

    #[test]
    fn refuses_what_it_cannot_apply() {
        let base = Font::default();
        for p in [json!({}), json!({"toggle": "shadow"}), json!({"size": 0}), json!({"bold": "yes"}), json!({"color": "nope"}), json!({"name": ""})] {
            assert!(format_runs(None, "abc", &base, 0, 1, &p).is_err(), "{p}");
        }
    }
}
