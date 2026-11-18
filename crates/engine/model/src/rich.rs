//! Rich text in cells: a text cell's characters in runs, each run with the font properties it
//! sets itself (SpreadsheetML `<r><rPr>` in a shared or inline string). A property a run does not
//! set comes from the cell's style, as in Excel.
//!
//! A cell keeps its runs next to its value ([`Cell::rich`](crate::Cell)): the value stays the
//! plain text (what formulas, sorting and search see), and the runs say how its characters look.
//! [`RichText::new`] keeps runs in one canonical form (no empty run, adjacent runs with the same
//! font merged, no runs at all when none sets anything), so a text read from a file, from a
//! replica and back compares equal.

use serde::{Deserialize, Serialize};

use crate::style::{Font, Underline, VertAlign};

/// A run's colour as a file names it: an explicit ARGB value, a theme slot, a legacy palette
/// index, or automatic, with an optional tint. Kept as written so a saved file says the same.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunColor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
    /// `AARRGGBB` (or `RRGGBB`) hex, as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<f64>,
}

impl RunColor {
    /// The model colour it resolves to (`indexed` through the legacy palette); `None` for
    /// automatic or nothing usable.
    pub fn to_color(&self) -> Option<crate::Color> {
        let tint = || self.tint.map_or(0, |t| (t.clamp(-1.0, 1.0) * 1000.0).round() as i16);
        if let Some(rgb) = self.rgb.as_deref().and_then(crate::Color::from_hex) {
            return Some(rgb);
        }
        if let Some(theme) = self.theme {
            return Some(crate::Color::Theme(u8::try_from(theme.min(11)).unwrap_or(11), tint()));
        }
        let index = usize::try_from(self.indexed?).ok()?;
        INDEXED.get(index).map(|v| crate::Color::Rgb(*v))
    }
}

/// The legacy 64-colour palette of `indexed` colours (ECMA-376 Part 1, 18.8.27). Index 64 is the
/// system foreground (automatic), which has no entry.
const INDEXED: [u32; 64] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00,
    0xFF00FF, 0x00FFFF, 0x800000, 0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366, 0xFFFFCC, 0xCCFFFF,
    0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF,
    0xCCFFCC, 0xFFFF99, 0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600, 0x666699, 0x969696,
    0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399, 0x333333,
];

/// The font properties one run sets (`None`: not set, the cell's style decides).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RunFont {
    /// `rFont`: the face name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `sz`, in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<Underline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<RunColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vert: Option<VertAlign>,
    /// `family`: the font family class (1 roman, 2 swiss, 3 modern, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<u32>,
    /// `charset`: the character set the face is used for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<u32>,
    /// `scheme`: `major` or `minor` when the run follows the theme's fonts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condense: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extend: Option<bool>,
}

impl RunFont {
    /// Whether the run sets nothing.
    pub fn is_empty(&self) -> bool {
        *self == RunFont::default()
    }

    /// `base` (the cell's font) with what this run sets on top.
    pub fn resolve(&self, base: &Font) -> Font {
        let mut f = base.clone();
        if let Some(name) = &self.name {
            f.name = name.clone();
        }
        if let Some(size) = self.size.filter(|s| s.is_finite() && *s > 0.0) {
            f.size = size as f32;
        }
        if let Some(v) = self.bold {
            f.bold = v;
        }
        if let Some(v) = self.italic {
            f.italic = v;
        }
        if let Some(v) = self.underline {
            f.underline = v;
        }
        if let Some(v) = self.strike {
            f.strike = v;
        }
        if let Some(c) = self.color.as_ref().and_then(RunColor::to_color) {
            f.color = c;
        }
        if let Some(v) = self.vert {
            f.vert = v;
        }
        f
    }

    /// Forgets what the run sets for every property in which `before` and `after` (a cell's font
    /// before and after a format command) differ: formatting the whole cell formats every run,
    /// as in Excel.
    pub fn clear_changed(&mut self, before: &Font, after: &Font) {
        if before.name != after.name {
            self.name = None;
            self.family = None;
            self.charset = None;
            self.scheme = None;
        }
        if before.size != after.size {
            self.size = None;
        }
        if before.bold != after.bold {
            self.bold = None;
        }
        if before.italic != after.italic {
            self.italic = None;
        }
        if before.underline != after.underline {
            self.underline = None;
        }
        if before.strike != after.strike {
            self.strike = None;
        }
        if before.color != after.color {
            self.color = None;
        }
        if before.vert != after.vert {
            self.vert = None;
        }
    }
}

/// One run: characters and the font properties they set.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "RunFont::is_empty")]
    pub font: RunFont,
}

impl TextRun {
    pub fn new(text: impl Into<String>, font: RunFont) -> TextRun {
        TextRun { text: text.into(), font }
    }
}

/// A rich text cell's runs, in canonical form (see the module docs).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RichText {
    pub runs: Vec<TextRun>,
}

impl RichText {
    /// The runs in canonical form: empty runs dropped, adjacent runs with equal fonts merged.
    /// `None` when no run sets anything (the text is plain).
    pub fn new(runs: impl IntoIterator<Item = TextRun>) -> Option<RichText> {
        let mut out: Vec<TextRun> = Vec::new();
        for run in runs {
            if run.text.is_empty() {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.font == run.font => last.text.push_str(&run.text),
                _ => out.push(run),
            }
        }
        out.iter().any(|r| !r.font.is_empty()).then_some(RichText { runs: out })
    }

    /// The plain text: every run's characters in order.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Whether the runs spell exactly `text`.
    pub fn spells(&self, text: &str) -> bool {
        let mut rest = text;
        for run in &self.runs {
            match rest.strip_prefix(run.text.as_str()) {
                Some(r) => rest = r,
                None => return false,
            }
        }
        rest.is_empty()
    }

    /// The runs after the text is edited to `new`: characters the edit keeps (the longest common
    /// subsequence of the old and new text, after their common start and end) keep their run's
    /// font, and typed characters take the font of the character before them (the first
    /// character's at the start), as typing at a caret does. `None` when the result sets nothing.
    pub fn retext(&self, new: &str) -> Option<RichText> {
        let old: Vec<(char, usize)> = self.runs.iter().enumerate().flat_map(|(k, r)| r.text.chars().map(move |c| (c, k))).collect();
        let new_chars: Vec<char> = new.chars().collect();
        let old_chars: Vec<char> = old.iter().map(|(c, _)| *c).collect();
        let first = old.first().map_or(0, |(_, k)| *k);
        let mut runs: Vec<TextRun> = Vec::new();
        let mut last = first;
        for (c, kept) in new_chars.iter().zip(keep(&old_chars, &new_chars)) {
            let k = kept.and_then(|i| old.get(i)).map_or(last, |(_, k)| *k);
            last = k;
            let font = self.runs.get(k).map(|r| &r.font);
            match runs.last_mut() {
                Some(run) if Some(&run.font) == font => run.text.push(*c),
                _ => runs.push(TextRun { text: c.to_string(), font: font.cloned().unwrap_or_default() }),
            }
        }
        RichText::new(runs)
    }

    /// The runs of `text` after `f` changes the font of its characters `from..to` (character
    /// offsets, clamped to the text): `runs` are the text's runs now (`None`, or runs that do not
    /// spell `text`: plain). `None` when the result sets nothing.
    pub fn format(runs: Option<&RichText>, text: &str, from: usize, to: usize, f: impl Fn(&mut RunFont)) -> Option<RichText> {
        let plain = [TextRun::new(text, RunFont::default())];
        let base = match runs.filter(|r| r.spells(text)) {
            Some(r) => r.runs.as_slice(),
            None => &plain,
        };
        let mut out = Vec::with_capacity(base.len() + 2);
        let mut at = 0;
        for run in base {
            let len = run.text.chars().count();
            let (start, end) = (from.clamp(at, at + len) - at, to.clamp(at, at + len) - at);
            let piece = |a: usize, b: usize| run.text.chars().skip(a).take(b.saturating_sub(a)).collect::<String>();
            if start >= end {
                out.push(run.clone());
            } else {
                out.push(TextRun::new(piece(0, start), run.font.clone()));
                let mut font = run.font.clone();
                f(&mut font);
                out.push(TextRun::new(piece(start, end), font));
                out.push(TextRun::new(piece(end, len), run.font.clone()));
            }
            at += len;
        }
        RichText::new(out)
    }

    /// Whether `test` holds for the font of every character `from..to` of `text` (`runs` as for
    /// [`RichText::format`]), each run resolved over `base` (the cell's font). An empty range
    /// asks about the character before `from` (the one typing there would take after).
    pub fn every(runs: Option<&RichText>, text: &str, base: &Font, from: usize, to: usize, test: impl Fn(&Font) -> bool) -> bool {
        let Some(runs) = runs.filter(|r| r.spells(text)) else { return test(base) };
        let (from, to) = if from < to { (from, to) } else { (from.saturating_sub(1), from.saturating_sub(1) + 1) };
        let mut at = 0;
        for run in &runs.runs {
            let len = run.text.chars().count();
            if at < to && from < at + len && !test(&run.font.resolve(base)) {
                return false;
            }
            at += len;
        }
        true
    }

    /// The runs with every property `before` and `after` differ in cleared ([`RunFont::clear_changed`]).
    /// `None` when nothing is left set.
    pub fn clear_changed(&self, before: &Font, after: &Font) -> Option<RichText> {
        RichText::new(self.runs.iter().map(|r| {
            let mut font = r.font.clone();
            font.clear_changed(before, after);
            TextRun { text: r.text.clone(), font }
        }))
    }
}

/// The most cells [`keep`] compares the changed middles of two texts in (their lengths'
/// product); beyond it the middle counts as replaced.
const DIFF_CELLS: usize = 1 << 20;

/// For each character of `new`, the index of the character of `old` it keeps, if any: the common
/// start and end, and between them a longest common subsequence (when small enough).
fn keep(old: &[char], new: &[char]) -> Vec<Option<usize>> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let rest = old.len().min(new.len()) - prefix;
    let suffix = old.iter().rev().zip(new.iter().rev()).take(rest).take_while(|(a, b)| a == b).count();
    let (a, b) = (&old[prefix..old.len() - suffix], &new[prefix..new.len() - suffix]);
    let mut out: Vec<Option<usize>> = (0..prefix).map(Some).collect();
    let (n, m) = (a.len(), b.len());
    if n > 0 && m > 0 && n.saturating_mul(m) <= DIFF_CELLS {
        // lcs[i * (m + 1) + j]: the longest common subsequence of a[i..] and b[j..].
        let mut lcs = vec![0u32; (n + 1) * (m + 1)];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i * (m + 1) + j] =
                    if a[i] == b[j] { lcs[(i + 1) * (m + 1) + j + 1] + 1 } else { lcs[(i + 1) * (m + 1) + j].max(lcs[i * (m + 1) + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while j < m {
            if i < n && a[i] == b[j] {
                out.push(Some(prefix + i));
                i += 1;
                j += 1;
            } else if i < n && lcs[(i + 1) * (m + 1) + j] >= lcs[i * (m + 1) + j + 1] {
                i += 1;
            } else {
                out.push(None);
                j += 1;
            }
        }
    } else {
        out.extend(std::iter::repeat_n(None, m));
    }
    out.extend((old.len() - suffix..old.len()).map(Some));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold() -> RunFont {
        RunFont { bold: Some(true), ..RunFont::default() }
    }

    fn red() -> RunFont {
        RunFont { color: Some(RunColor { rgb: Some("FFFF0000".into()), ..RunColor::default() }), ..RunFont::default() }
    }

    fn rich(runs: &[(&str, RunFont)]) -> RichText {
        RichText::new(runs.iter().map(|(t, f)| TextRun::new(*t, f.clone()))).unwrap_or_else(|| panic!("rich"))
    }

    #[test]
    fn runs_are_canonical() {
        let r = RichText::new([TextRun::new("a", bold()), TextRun::new("", red()), TextRun::new("b", bold()), TextRun::new("c", RunFont::default())]);
        assert_eq!(r, Some(RichText { runs: vec![TextRun::new("ab", bold()), TextRun::new("c", RunFont::default())] }));
        assert_eq!(RichText::new([TextRun::new("plain", RunFont::default())]), None, "runs setting nothing are plain text");
        assert!(rich(&[("Hello ", bold()), ("world", red())]).spells("Hello world"));
        assert!(!rich(&[("Hello ", bold()), ("world", red())]).spells("Hello world!"));
    }

    #[test]
    fn edits_keep_runs_and_typing_takes_the_run_at_the_caret() {
        let r = rich(&[("Hello", bold()), (" world", RunFont::default())]);
        // Typed after the bold run's last character: bold.
        assert_eq!(r.retext("Hello!! world"), Some(rich(&[("Hello!!", bold()), (" world", RunFont::default())])));
        // Typed inside the plain run: plain.
        assert_eq!(r.retext("Hello big world"), Some(rich(&[("Hello", bold()), (" big world", RunFont::default())])));
        // Typed at the start: the first run's.
        assert_eq!(r.retext(">Hello world"), Some(rich(&[(">Hello", bold()), (" world", RunFont::default())])));
        // Deleting the bold run leaves plain text.
        assert_eq!(r.retext(" world"), None);
        // Replacing a middle part.
        assert_eq!(r.retext("Help world"), Some(rich(&[("Help", bold()), (" world", RunFont::default())])));
        assert_eq!(r.retext("Hello world"), Some(r.clone()));
        assert_eq!(r.retext(""), None);
        // Two edits at once: each typed character takes its own caret's run.
        assert_eq!(r.retext("Hello, world."), Some(rich(&[("Hello,", bold()), (" world.", RunFont::default())])));
        let three = rich(&[("ab", bold()), ("cd", red()), ("ef", RunFont::default())]);
        assert_eq!(three.retext("aXbcYdeZf"), Some(rich(&[("aXb", bold()), ("cYd", red()), ("eZf", RunFont::default())])));
        assert_eq!(keep(&['a', 'b'], &['b', 'a', 'b']), vec![None, Some(0), Some(1)]);
    }

    #[test]
    fn formatting_characters_splits_and_merges_runs() {
        // Plain text: bold "ll".
        let r = RichText::format(None, "Hello", 2, 4, |f| f.bold = Some(true));
        assert_eq!(r, Some(rich(&[("He", RunFont::default()), ("ll", bold()), ("o", RunFont::default())])));
        // Across a run boundary, then over all of it: one run again.
        let red_tail = rich(&[("He", RunFont::default()), ("llo", red())]);
        let r = RichText::format(Some(&red_tail), "Hello", 1, 3, |f| f.bold = Some(true)).unwrap_or_else(|| panic!("rich"));
        let red_bold = RunFont { bold: Some(true), ..red() };
        assert_eq!(r, rich(&[("H", RunFont::default()), ("e", bold()), ("l", red_bold.clone()), ("lo", red())]));
        assert_eq!(RichText::format(Some(&r), "Hello", 0, 5, |f| f.bold = None), Some(red_tail.clone()));
        // Unsetting what was set leaves plain text; ranges are clamped; runs not spelling the
        // text are ignored.
        assert_eq!(RichText::format(Some(&rich(&[("ab", bold())])), "ab", 0, 99, |f| f.bold = None), None);
        assert_eq!(RichText::format(Some(&red_tail), "other", 5, 9, |f| f.bold = Some(true)), None);
        assert_eq!(RichText::format(None, "abc", 2, 1, |f| f.bold = Some(true)), None, "an empty range changes nothing");
        // Multibyte characters count as one.
        assert_eq!(
            RichText::format(None, "汉字ab", 1, 3, |f| f.italic = Some(true)),
            Some(rich(&[("汉", RunFont::default()), ("字a", RunFont { italic: Some(true), ..RunFont::default() }), ("b", RunFont::default())]))
        );

        let base = Font::default();
        let is_bold = |f: &Font| f.bold;
        assert!(RichText::every(Some(&r), "Hello", &base, 1, 3, is_bold));
        assert!(!RichText::every(Some(&r), "Hello", &base, 1, 4, is_bold));
        assert!(!RichText::every(None, "Hello", &base, 0, 2, is_bold));
        assert!(RichText::every(None, "Hello", &Font { bold: true, ..Font::default() }, 0, 2, is_bold), "the cell's font counts");
        assert!(RichText::every(Some(&r), "Hello", &base, 3, 3, is_bold), "a caret asks about the character before it");
    }

    #[test]
    fn formatting_the_cell_formats_every_run() {
        let both = RunFont { bold: Some(false), ..red() };
        let r = rich(&[("a", both), ("b", RunFont::default())]);
        let before = Font::default();
        let after = Font { bold: true, ..Font::default() };
        assert_eq!(r.clear_changed(&before, &after), Some(rich(&[("a", red()), ("b", RunFont::default())])));
        let resolved = red().resolve(&after);
        assert!(resolved.bold);
        assert_eq!(resolved.color, crate::Color::Rgb(0xFF0000));
        let indexed = RunFont { color: Some(RunColor { indexed: Some(10), ..RunColor::default() }), ..RunFont::default() };
        assert_eq!(indexed.resolve(&before).color, crate::Color::Rgb(0xFF0000));
    }
}
