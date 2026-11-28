//! The text being typed into the active cell: its characters, each one's own font (rich text),
//! the caret and the selection. Typed characters take the font of the character before the caret
//! (the first character's at the start), as `RichText::retext` does for an edit made elsewhere;
//! formatting the selection changes its characters' fonts (`text_format`), and entering the cell
//! gives `cell.set` the runs.

use kition_sheet_engine::model::{Font, RichText, RunFont, TextRun};
use kition_sheet_engine::text_format;
use serde_json::Value as Json;

#[derive(Clone, PartialEq)]
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
    /// The text and fonts the typing started from: Cmd+Z while typing goes back to them (WPS W-H5).
    initial: (Vec<char>, Vec<RunFont>),
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
        let initial = (chars.clone(), fonts.clone());
        Edit { chars, fonts, caret: end, anchor: end, in_place, initial }
    }

    /// Takes back everything typed since the typing started (Cmd+Z while typing, WPS W-H5): the
    /// text it started from, the caret at its end, still typing. Answers whether anything changed.
    pub fn undo_typing(&mut self) -> bool {
        let changed = self.chars != self.initial.0 || self.fonts != self.initial.1;
        self.chars = self.initial.0.clone();
        self.fonts = self.initial.1.clone();
        let end = self.chars.len();
        self.caret = end;
        self.anchor = end;
        changed
    }

    /// Selects all the text (Cmd+A, a triple-click).
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.chars.len();
    }

    /// The selected text.
    pub fn selected_text(&self) -> String {
        let (from, to) = self.selection();
        self.chars.get(from.min(self.chars.len())..to.min(self.chars.len())).unwrap_or_default().iter().collect()
    }

    /// Takes the selected text out (a cut), answering it.
    pub fn cut(&mut self) -> String {
        let text = self.selected_text();
        self.delete_selection();
        text
    }

    /// The start and the end (before its line break) of the line of text caret position `at` is on
    /// (lines are the cell's line breaks).
    pub fn line_bounds(&self, at: usize) -> (usize, usize) {
        let at = at.min(self.chars.len());
        let start = self.chars.get(..at).and_then(|before| before.iter().rposition(|c| *c == '\n')).map_or(0, |i| i + 1);
        let end = self.chars.get(at..).and_then(|after| after.iter().position(|c| *c == '\n')).map_or(self.chars.len(), |i| at + i);
        (start, end)
    }

    /// The caret position a line up (`up`) or down from the caret, as far along its line as it can
    /// go; `None` on the first (last) line.
    pub fn line_step(&self, up: bool) -> Option<usize> {
        let (start, end) = self.line_bounds(self.caret);
        let along = self.caret.min(self.chars.len()) - start;
        if up {
            let previous = start.checked_sub(1)?;
            let (p_start, p_end) = self.line_bounds(previous);
            Some((p_start + along).min(p_end))
        } else {
            if end >= self.chars.len() {
                return None;
            }
            let (n_start, n_end) = self.line_bounds(end + 1);
            Some((n_start + along).min(n_end))
        }
    }

    /// The caret position a word to the left (right) of the caret, as Option+arrow moves on a Mac:
    /// to the start of the word before (the end of the word after), over spaces and punctuation.
    pub fn word_stop(&self, right: bool) -> usize {
        let word = |c: &char| c.is_alphanumeric();
        let len = self.chars.len();
        let mut at = self.caret.min(len);
        if right {
            while at < len && !self.chars.get(at).is_some_and(word) {
                at += 1;
            }
            while at < len && self.chars.get(at).is_some_and(word) {
                at += 1;
            }
        } else {
            while at > 0 && !self.chars.get(at - 1).is_some_and(word) {
                at -= 1;
            }
            while at > 0 && self.chars.get(at - 1).is_some_and(word) {
                at -= 1;
            }
        }
        at
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
        "format.bold" => toggle("bold", "bold"),
        "format.italic" => toggle("italic", "italic"),
        "format.strikethrough" => toggle("strike", "strike"),
        "format.underline" => match (on, params.get("style").and_then(Json::as_str)) {
            (Some(false), _) => serde_json::json!({ "underline": "none" }),
            (_, Some(style)) => serde_json::json!({ "underline": style }),
            (Some(true), None) => serde_json::json!({ "underline": "single" }),
            (None, None) => serde_json::json!({ "toggle": "underline" }),
        },
        "format.superscript" | "format.subscript" => {
            let name = command.trim_start_matches("format.");
            match on {
                Some(v) => serde_json::json!({ "vertAlign": if v { name } else { "baseline" } }),
                None => serde_json::json!({ "toggle": name }),
            }
        }
        "format.color" => serde_json::json!({ "color": params.get("color")? }),
        "format.fontSize" => serde_json::json!({ "size": params.get("size")? }),
        "format.fontName" => serde_json::json!({ "name": params.get("name")? }),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn words_lines_select_all_cut_and_undo_typing() {
        let mut e = Edit::new("harbour north", None, true);
        assert_eq!(e.word_stop(false), 8);
        e.move_to(8, false);
        assert_eq!(e.word_stop(false), 0);
        e.move_to(7, false);
        assert_eq!(e.word_stop(true), 13);
        e.move_to(0, false);
        assert_eq!(e.word_stop(true), 7);
        e.select_all();
        assert_eq!(e.selection(), (0, 13));
        e.move_to(3, false);
        e.move_to(8, true);
        assert_eq!(e.selected_text(), "bour ");
        assert_eq!(e.cut(), "bour ");
        assert_eq!(e.text(), "harnorth");
        e.insert("X");
        assert!(e.undo_typing());
        assert_eq!(e.text(), "harbour north");
        assert_eq!(e.selection(), (13, 13));
        assert!(!e.undo_typing(), "nothing more to take back");

        let mut lines = Edit::new("one\ntwo more\nx", None, true);
        lines.move_to(6, false);
        assert_eq!(lines.line_bounds(6), (4, 12));
        assert_eq!(lines.line_step(true), Some(2), "up: as far along the shorter line as it goes");
        assert_eq!(lines.line_step(false), Some(14));
        lines.move_to(1, false);
        assert_eq!(lines.line_step(true), None, "the first line has nothing above");
        lines.move_to(14, false);
        assert_eq!(lines.line_step(false), None, "the last line has nothing below");
    }

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
        assert_eq!(format_params("format.bold", &json!({})), Some(json!({"toggle": "bold"})));
        assert_eq!(format_params("format.italic", &json!({"on": false})), Some(json!({"italic": false})));
        assert_eq!(format_params("format.underline", &json!({"style": "double"})), Some(json!({"underline": "double"})));
        assert_eq!(format_params("format.color", &json!({"color": "#FF0000"})), Some(json!({"color": "#FF0000"})));
        assert_eq!(format_params("format.fontSize", &json!({"size": 14})), Some(json!({"size": 14})));
        assert_eq!(format_params("format.fontSize", &json!({})), None);
        assert_eq!(format_params("format.superscript", &json!({"on": true})), Some(json!({"vertAlign": "superscript"})));
        assert_eq!(format_params("format.subscript", &json!({})), Some(json!({"toggle": "subscript"})));
        assert_eq!(format_params("format.fillColor", &json!({"color": "#FF0000"})), None);
    }
}
