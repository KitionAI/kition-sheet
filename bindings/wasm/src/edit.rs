//! The text being typed into the active cell: its characters, each one's own font (rich text),
//! the caret and the selection. Typed characters take the font of the character before the caret
//! (the first character's at the start), as `RichText::retext` does for an edit made elsewhere;
//! formatting the selection changes its characters' fonts (`text_format`), and entering the cell
//! gives `cell.set` the runs.

use kition_sheet_engine::model::{Font, RichText, RunFont, TextRun};
use kition_sheet_engine::text_format;
use serde_json::Value as Json;

pub struct Edit {
    chars: Vec<char>,
    /// Each character's own font (`RunFont::default()`: the cell's).
    fonts: Vec<RunFont>,
    /// The caret, and the other end of the selection, in characters.
    pub caret: usize,
    pub anchor: usize,
    /// Edit mode (F2, a double-click, a click into the text): arrows move the caret. Otherwise
    /// (typing over the cell) arrows enter it and move, as in Excel's Enter mode.
    pub in_place: bool,
}

impl Edit {
    /// Editing `text` with its `runs` (used when they spell it), the caret at the end.
    pub fn new(text: &str, runs: Option<&RichText>, in_place: bool) -> Edit {
        let chars: Vec<char> = text.chars().collect();
        let fonts = match runs.filter(|r| r.spells(text)) {
            Some(runs) => runs.runs.iter().flat_map(|r| r.text.chars().map(move |_| r.font.clone())).collect(),
            None => vec![RunFont::default(); chars.len()],
        };
        let end = chars.len();
        Edit { chars, fonts, caret: end, anchor: end, in_place }
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// The characters typed so far.
    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    /// Types `text` over characters `from..to`, the caret after it.
    pub fn replace(&mut self, from: usize, to: usize, text: &str) {
        let len = self.chars.len();
        self.anchor = from.min(len);
        self.caret = to.min(len);
        self.insert(text);
    }

    /// The whole text replaced by `text` (a formula: in the cell's font), the caret at `caret`.
    pub fn set_text(&mut self, text: &str, caret: usize) {
        self.chars = text.chars().collect();
        self.fonts = vec![RunFont::default(); self.chars.len()];
        self.move_to(caret, false);
    }

    /// The runs of the text (`None`: plain).
    pub fn rich(&self) -> Option<RichText> {
        RichText::new(self.chars.iter().zip(&self.fonts).map(|(c, f)| TextRun::new(c.to_string(), f.clone())))
    }

    /// The runs as `cell.set` takes them (none for plain text, so the cell's old runs go).
    pub fn runs_param(&self) -> Json {
        self.rich().and_then(|r| serde_json::to_value(r.runs).ok()).unwrap_or_else(|| Json::Array(Vec::new()))
    }

    /// The selection, start first.
    pub fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    fn delete_selection(&mut self) -> bool {
        let (from, to) = self.selection();
        let (from, to) = (from.min(self.chars.len()), to.min(self.chars.len()));
        if from == to {
            return false;
        }
        self.chars.drain(from..to);
        self.fonts.drain(from..to);
        self.caret = from;
        self.anchor = from;
        true
    }

    /// Types `text` over the selection.
    pub fn insert(&mut self, text: &str) {
        self.delete_selection();
        let at = self.caret.min(self.chars.len());
        let font = self.fonts.get(at.wrapping_sub(1)).or_else(|| self.fonts.first()).cloned().unwrap_or_default();
        let typed: Vec<char> = text.chars().collect();
        let n = typed.len();
        self.chars.splice(at..at, typed);
        self.fonts.splice(at..at, std::iter::repeat_n(font, n));
        self.caret = at + n;
        self.anchor = self.caret;
    }

    /// Backspace (`forward`: Delete): the selection, else the character before (after) the caret.
    pub fn delete(&mut self, forward: bool) {
        if self.delete_selection() {
            return;
        }
        let at = self.caret.min(self.chars.len());
        let gone = if forward { (at < self.chars.len()).then_some(at) } else { at.checked_sub(1) };
        if let Some(i) = gone {
            self.chars.remove(i);
            self.fonts.remove(i);
            self.caret = i;
            self.anchor = i;
        }
    }

    /// Moves the caret to `to` (clamped); `extend` keeps the selection's other end.
    pub fn move_to(&mut self, to: usize, extend: bool) {
        self.caret = to.min(self.chars.len());
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// One character left or right; without `extend`, a selection collapses to that side.
    pub fn step(&mut self, right: bool, extend: bool) {
        let (from, to) = self.selection();
        let target = match (right, extend || from == to) {
            (false, true) => self.caret.saturating_sub(1),
            (true, true) => self.caret + 1,
            (false, false) => from,
            (true, false) => to,
        };
        self.move_to(target, extend);
    }

    /// Selects the word around character `at` (letters and digits; else that one character).
    pub fn select_word(&mut self, at: usize) {
        let at = at.min(self.chars.len().saturating_sub(1));
        let word = |i: usize| self.chars.get(i).is_some_and(|c| c.is_alphanumeric());
        if !word(at) {
            self.anchor = at;
            self.caret = (at + 1).min(self.chars.len());
            return;
        }
        let start = (0..=at).rev().take_while(|&i| word(i)).last().unwrap_or(at);
        let end = (at..self.chars.len()).take_while(|&i| word(i)).last().map_or(at, |i| i + 1);
        self.anchor = start;
        self.caret = end;
    }

    /// Formats the selected characters as `cell.formatText`'s parameters say (`base`: the cell's
    /// font). `Ok(false)` when nothing is selected or the text is a formula: the format is the
    /// cell's to take.
    pub fn format(&mut self, base: &Font, params: &Json) -> Result<bool, String> {
        let (from, to) = self.selection();
        if from == to || self.chars.first() == Some(&'=') {
            return Ok(false);
        }
        let text = self.text();
        let rich = self.rich();
        let new = text_format::format_runs(rich.as_ref(), &text, base, from, to, params).map_err(|e| e.to_string())?;
        self.fonts = match new {
            Some(runs) => runs.runs.iter().flat_map(|r| r.text.chars().map(move |_| r.font.clone())).collect(),
            None => vec![RunFont::default(); self.chars.len()],
        };
        Ok(true)
    }

    /// The fonts (over `base`) of the selected characters, or of the character before the caret
    /// (the first one at the start): what the ribbon's font, size and colour show while typing.
    pub fn selected_fonts(&self, base: &Font) -> Vec<Font> {
        let (from, to) = self.selection();
        let range = if from < to {
            from..to
        } else if from > 0 {
            from - 1..from
        } else {
            0..1
        };
        let fonts: Vec<Font> = self
            .fonts
            .get(range.start.min(self.fonts.len())..range.end.min(self.fonts.len()))
            .unwrap_or_default()
            .iter()
            .map(|f| f.resolve(base))
            .collect();
        if fonts.is_empty() { vec![base.clone()] } else { fonts }
    }

    /// Whether `test` holds for every selected character's font over `base` (the character before
    /// a caret).
    pub fn every(&self, base: &Font, test: impl Fn(&Font) -> bool) -> bool {
        let (from, to) = self.selection();
        RichText::every(self.rich().as_ref(), &self.text(), base, from, to, test)
    }
}

/// `cell.formatText` parameters for a font command run while characters are selected, or `None`
/// for a command that is not one.
pub fn format_params(command: &str, params: &Json) -> Option<Json> {
    let on = params.get("on").and_then(Json::as_bool);
    let toggle = |name: &str, key: &str| match on {
        Some(v) => serde_json::json!({ key: v }),
        None => serde_json::json!({ "toggle": name }),
    };
    Some(match command {
        "home.bold" => toggle("bold", "bold"),
        "home.italic" => toggle("italic", "italic"),
        "home.strikethrough" => toggle("strike", "strike"),
        "home.underline" => match (on, params.get("style").and_then(Json::as_str)) {
            (Some(false), _) => serde_json::json!({ "underline": "none" }),
            (_, Some(style)) => serde_json::json!({ "underline": style }),
            (Some(true), None) => serde_json::json!({ "underline": "single" }),
            (None, None) => serde_json::json!({ "toggle": "underline" }),
        },
        "home.superscript" | "home.subscript" => {
            let name = command.trim_start_matches("home.");
            match on {
                Some(v) => serde_json::json!({ "vertAlign": if v { name } else { "baseline" } }),
                None => serde_json::json!({ "toggle": name }),
            }
        }
        "home.fontColor" => serde_json::json!({ "color": params.get("color")? }),
        "home.fontSize" => serde_json::json!({ "size": params.get("size")? }),
        "home.fontName" => serde_json::json!({ "name": params.get("name")? }),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typing_deleting_and_moving_keep_each_characters_font() {
        let bold = RunFont { bold: Some(true), ..RunFont::default() };
        let runs = RichText::new([TextRun::new("ab", bold.clone()), TextRun::new("cd", RunFont::default())]);
        let mut e = Edit::new("abcd", runs.as_ref(), true);
        e.move_to(2, false);
        e.insert("X");
        assert_eq!(e.text(), "abXcd");
        assert_eq!(e.rich().map(|r| r.runs[0].text.clone()), Some("abX".into()), "typing takes the run before the caret");
        e.step(false, true);
        e.step(false, true);
        assert_eq!(e.selection(), (1, 3));
        e.delete(false);
        assert_eq!((e.text().as_str(), e.caret), ("acd", 1));
        e.delete(true);
        assert_eq!(e.text(), "ad");
        e.move_to(0, false);
        e.delete(false);
        assert_eq!(e.text(), "ad", "nothing before the start");
        e.select_word(1);
        assert_eq!(e.selection(), (0, 2));
        e.insert("hello world");
        e.select_word(8);
        assert_eq!(e.selection(), (6, 11));
        e.step(false, false);
        assert_eq!(e.selection(), (6, 6), "a selection collapses to its start");
    }

    #[test]
    fn formats_the_selection_only() {
        let mut e = Edit::new("Hello", None, true);
        assert!(!e.format(&Font::default(), &json!({"toggle": "bold"})).unwrap(), "no selection: not handled");
        e.move_to(1, false);
        e.move_to(3, true);
        assert!(e.format(&Font::default(), &json!({"toggle": "bold"})).unwrap());
        assert!(e.every(&Font::default(), |f| f.bold));
        let runs = e.rich().unwrap().runs;
        assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["H", "el", "lo"]);
        assert_eq!(e.runs_param()[1]["font"]["bold"], json!(true));
        assert!(e.format(&Font::default(), &json!({"toggle": "bold"})).unwrap());
        assert_eq!(e.runs_param(), json!([]), "plain again");
        let mut formula = Edit::new("=A1", None, true);
        formula.move_to(0, true);
        assert!(!formula.format(&Font::default(), &json!({"bold": true})).unwrap());
    }

    #[test]
    fn font_commands_become_character_formats() {
        assert_eq!(format_params("home.bold", &json!({})), Some(json!({"toggle": "bold"})));
        assert_eq!(format_params("home.italic", &json!({"on": false})), Some(json!({"italic": false})));
        assert_eq!(format_params("home.underline", &json!({"style": "double"})), Some(json!({"underline": "double"})));
        assert_eq!(format_params("home.fontColor", &json!({"color": "#FF0000"})), Some(json!({"color": "#FF0000"})));
        assert_eq!(format_params("home.fontSize", &json!({"size": 14})), Some(json!({"size": 14})));
        assert_eq!(format_params("home.fontSize", &json!({})), None);
        assert_eq!(format_params("home.superscript", &json!({"on": true})), Some(json!({"vertAlign": "superscript"})));
        assert_eq!(format_params("home.subscript", &json!({})), Some(json!({"toggle": "subscript"})));
        assert_eq!(format_params("home.fillColor", &json!({"color": "#FF0000"})), None);
    }
}
