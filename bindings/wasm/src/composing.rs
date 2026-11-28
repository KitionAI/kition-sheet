//! What an input method composes in the cell being typed, as WPS shows it (W-T1–W-T4; walkthrough
//! D13): the pinyin (or kana, or hangul) not yet chosen shows in the cell at the caret, underlined,
//! and the cell's box grows with it; the first of it starts typing in the cell as a typed character
//! does (what the cell held is replaced). It is session state: never the edit's text, never the
//! workbook, never a change (only the cell's repaint). The chosen text comes as typed text, which
//! ends the composition; an empty composition (Escape in the input method) takes it away and the
//! cell stays open.
//!
//! The editor sends it (`editor.preedit`) from the composition events of the text field the input
//! method writes in, and puts that field at the caret (`inspect().caretRect`), so the candidate
//! window opens beside the cell.

use super::*;

/// The longest composition shown, in characters.
const LONGEST: usize = 1000;

/// A composition under way: its text and the input method's cursor in it (characters).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Preedit {
    pub(crate) text: String,
    pub(crate) cursor: usize,
}

impl SheetDocument {
    /// `editor.preedit {text, cursor?}`: shows `text` composing at the caret of the cell being
    /// typed, starting to type in the active cell if nothing is (as a typed character does);
    /// empty, it takes the composition away. A drawing object's text, a checkbox cell and a cell on
    /// another sheet being pointed from show none.
    pub(crate) fn preedit_command(&mut self, params: &Json) -> Result<Json, String> {
        let text: String = params.get("text").and_then(Json::as_str).unwrap_or_default().chars().take(LONGEST).collect();
        if text.chars().any(char::is_control) {
            return Err("editor.preedit: `text` has no control characters".into());
        }
        if text.is_empty() {
            if self.preedit.take().is_some() {
                self.edits += 1;
            }
            return Ok(json!({ "editing": self.editing.is_some() }));
        }
        if self.editing.is_none() {
            let cell = self.active();
            if self.shape_edit.is_some() || self.session.selected_shape.is_some() || self.shows_checkbox(cell) {
                return Ok(json!({ "editing": false }));
            }
            // The first of a composition types over the cell, as a typed character does.
            self.edit_inserted = false;
            self.end_copy();
            self.editing = Some(Edit::new("", None, false));
            self.point = None;
            self.hint_pick = 0;
        }
        if self.editing_elsewhere() {
            return Ok(json!({ "editing": true }));
        }
        let length = text.chars().count();
        let cursor = params.get("cursor").and_then(Json::as_u64).map_or(length, |c| usize::try_from(c).unwrap_or(length).min(length));
        self.preedit = Some(Preedit { text, cursor });
        self.edits += 1;
        Ok(json!({ "editing": true }))
    }

    /// The cell being typed as it shows: with the composition at the caret (over the selected
    /// characters, as typing would be), the caret at the input method's cursor; and the
    /// composition's characters.
    pub(crate) fn shown_edit(&self) -> Option<(std::borrow::Cow<'_, Edit>, Option<(usize, usize)>)> {
        let edit = self.editing.as_ref()?;
        let Some(preedit) = self.preedit.as_ref() else { return Some((std::borrow::Cow::Borrowed(edit), None)) };
        let mut shown = edit.clone();
        let at = shown.selection().0;
        shown.insert(&preedit.text);
        let end = shown.caret;
        shown.move_to(at + preedit.cursor, false);
        Some((std::borrow::Cow::Owned(shown), Some((at, end))))
    }

    /// The caret of the cell being typed (at the composition's cursor while composing), in
    /// document points: where the input method's candidate window goes.
    pub(crate) fn caret_rect(&self) -> Option<[f64; 4]> {
        let (edit, _) = self.shown_edit()?;
        let b = self.edit_box(0.0, 0.0, 1.0)?;
        let line = b.line_of(edit.caret)?;
        let x = b.start + line.x(edit.caret);
        Some([x, line.baseline - b.ascent, x + 1.0, line.baseline + b.descent])
    }

    /// The composition as the editor shows it beside the cell's text: its text and where it goes
    /// in the text (UTF-16 units, as the formula bar counts).
    pub(crate) fn preedit_info(&self) -> Json {
        let (Some(edit), Some(preedit)) = (self.editing.as_ref(), self.preedit.as_ref()) else { return Json::Null };
        let at = edit.selection().0;
        let units: usize = edit.chars().iter().take(at).map(|c| c.len_utf16()).sum();
        json!({ "text": preedit.text, "at": units })
    }
}
