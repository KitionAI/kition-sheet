//! The formula bar (`editor.formula`): what is typed there is the cell being typed, as in WPS and
//! Excel. The cell shows it as it is typed, pointing at cells (on this sheet or another) inserts
//! their references at the bar's caret, and Enter, Tab, ✓, Escape and ✕ end it as they end typing
//! in the cell. Offsets are the browser's UTF-16 units.

use serde_json::{Value as Json, json};

use crate::edit::Edit;
use crate::validation::Step;
use crate::{SheetDocument, point, pointing};

/// Characters in the first `units` UTF-16 units of `text`.
fn chars_in(text: &str, units: usize) -> usize {
    let mut seen = 0;
    let mut count = 0;
    for c in text.chars() {
        if seen >= units {
            break;
        }
        seen += c.len_utf16();
        count += 1;
    }
    count
}

/// UTF-16 units of the first `chars` characters.
fn units_in(chars: &[char], count: usize) -> usize {
    chars.iter().take(count).map(|c| c.len_utf16()).sum()
}

impl SheetDocument {
    /// `editor.formula`:
    /// - `{text, start?, end?}`: the bar's text and its selection (default: the caret at the end).
    ///   Typing starts the active cell's edit with what it holds, so the cell shows the typing.
    /// - `{enter: down|up|right|left|stay, text?}`: enters it as Enter, Shift+Enter, Tab,
    ///   Shift+Tab or ✓ do (on another sheet: back in its own cell and sheet).
    /// - `{peek: true}`: changes nothing.
    /// - `{}`: drops it, as Escape and ✕ do.
    ///
    /// Answers the cell being typed (`editing`, `text`, `start`, `end`), the references in it and
    /// the function list and argument hint at its caret.
    pub(crate) fn bar_command(&mut self, params: &Json) -> Result<Json, String> {
        let text = params.get("text").and_then(Json::as_str);
        if let Some(text) = text {
            let units = text.encode_utf16().count();
            let at = |key: &str| params.get(key).and_then(Json::as_u64).map_or(units, |n| (n as usize).min(units));
            self.bar_text(text, at("start"), at("end"));
        }
        if let Some(direction) = params.get("enter").and_then(Json::as_str) {
            let step = match direction {
                "down" => Step::Enter(false),
                "up" => Step::Enter(true),
                "right" => Step::Tab(false),
                "left" => Step::Tab(true),
                _ => Step::By(0, 0),
            };
            // With nothing typed, Enter and Tab still move, as in the grid (round a selection,
            // back to where a run of Tabs began); what the cell's validation holds stays until
            // the person answers.
            if self.editing.is_some() {
                self.enter_and_step(step)?;
            } else {
                self.step_after(step)?;
            }
        } else if text.is_none() && params.get("peek").and_then(Json::as_bool) != Some(true) {
            self.cancel_edit()?;
        }
        Ok(self.bar_state())
    }

    /// Drops the cell being typed (Escape), back on its own sheet.
    pub(crate) fn cancel_edit(&mut self) -> Result<(), String> {
        if self.editing.take().is_some() {
            self.edits += 1;
        }
        self.point = None;
        self.hint_pick = 0;
        self.go_home()
    }

    /// The bar's text and selection become the cell being typed's: only the characters that
    /// changed are retyped, so the others keep their own fonts.
    fn bar_text(&mut self, text: &str, start: usize, end: usize) {
        if self.editing.is_none() {
            self.edit_in_place(self.active());
            self.point = None;
            self.hint_pick = 0;
        }
        let Some(edit) = self.editing.as_mut() else { return };
        let new: Vec<char> = text.chars().collect();
        let old = edit.chars().to_vec();
        if old != new {
            let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
            let room = old.len().min(new.len()) - prefix;
            let suffix = old.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
            let typed: String = new.get(prefix..new.len() - suffix).unwrap_or_default().iter().collect();
            let gone = old.len() - suffix;
            edit.replace(prefix, gone, &typed);
            // Typing in the bar ends pointing, as typing in the cell does.
            self.point = None;
            self.hint_pick = 0;
        }
        edit.move_to(chars_in(text, start), false);
        edit.move_to(chars_in(text, end), true);
        self.edits += 1;
    }

    /// The cell being typed as the bar shows it.
    pub(crate) fn bar_state(&self) -> Json {
        let references = self.formula_references();
        match self.editing.as_ref() {
            Some(edit) => {
                let chars = edit.chars();
                let (anchor, caret) = (units_in(chars, edit.anchor), units_in(chars, edit.caret));
                let hint = point::hint(chars, edit.caret);
                let hint = (!hint.functions.is_empty() || hint.call.is_some()).then(|| pointing::hint_json(&hint, chars, self.hint_pick));
                json!({
                    "editing": true,
                    "pointing": self.point.is_some(),
                    "text": edit.text(),
                    "start": anchor.min(caret),
                    "end": anchor.max(caret),
                    "references": references,
                    "hint": hint,
                })
            }
            None => json!({ "editing": false, "pointing": false, "text": null, "start": 0, "end": 0, "references": references, "hint": null }),
        }
    }

    /// A command asked to type in the active cell (`UiRequest::EditCell`): F2 edits it in place;
    /// a function chosen in Insert Function goes in at the caret of the formula being typed, or
    /// starts one (`=SUM(`).
    pub(crate) fn begin_edit(&mut self, text: Option<&str>) {
        // A checkbox cell is not typed in (Insert Function, F2): WPS says why.
        if self.editing.is_none() && self.refuse_checkbox(self.active()) {
            return;
        }
        let Some(text) = text else {
            match self.editing.as_mut() {
                None => self.edit_in_place(self.active()),
                // F2 while typing switches between Enter mode and Edit mode (WPS W-C4).
                Some(edit) => {
                    edit.in_place = !edit.in_place;
                    self.edits += 1;
                }
            }
            return;
        };
        // Insert Function's formula is inserted, not typed (TS-C60): it doesn't make a table larger.
        if self.editing.is_none() {
            self.edit_inserted = true;
        }
        let formula = self.editing.as_ref().is_some_and(|e| e.chars().first() == Some(&'='));
        match (self.editing.as_mut(), text.strip_prefix('=')) {
            (Some(edit), Some(rest)) if formula => edit.insert(rest),
            _ => {
                let mut edit = Edit::new(text, None, true);
                // `=SUM()`: the caret between the parentheses.
                if text.ends_with("()") {
                    edit.move_to(edit.len().saturating_sub(1), false);
                }
                self.editing = Some(edit);
            }
        }
        self.point = None;
        self.hint_pick = 0;
        self.edits += 1;
    }

    /// The caret and the other end of the selection of the cell being typed (UTF-16).
    pub(crate) fn edit_offsets(&self) -> Option<(usize, usize)> {
        self.editing.as_ref().map(|edit| (units_in(edit.chars(), edit.caret), units_in(edit.chars(), edit.anchor)))
    }
}
