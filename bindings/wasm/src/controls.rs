//! Checkbox cells and inserted text in the editor (module M8, as WPS for Mac and Excel have them):
//! - a checkbox cell (Insert › Checkbox) is ticked or cleared by a click on its box or by Space; a
//!   click beside the box only selects the cell; it is not typed in (typing, a double-click, F2,
//!   the formula bar and Insert Function leave it as it is, with WPS's notice
//!   「复选框不可直接编辑，请删除后再插入」);
//! - text inserted by the editor (`editor.insertText`; Insert › Symbol, a date) goes in at the
//!   caret of what is being typed (the cell, a shape's text); with nothing typed, a symbol goes
//!   after the active cell's text and leaves the cell open (WPS), a date takes the cell's place.
//!
//! The notices go to the editor through `inspect().notice` (`{seq, text}`), once each.

use kition_sheet_engine::cmd::extra::{CHECKBOX_LOCKED, is_checkbox};
use kition_sheet_engine::core::{CellRef, RangeRef};
use serde_json::{Value as Json, json};

use crate::draw::Rect;
use crate::edit::Edit;
use crate::{Hit, SheetDocument, checkbox_rect};

impl SheetDocument {
    /// Tells the person something (the editor shows it once).
    pub(crate) fn notify(&mut self, text: &str) {
        self.notice = (self.notice.0 + 1, Some(text.to_owned()));
        self.edits += 1;
    }

    /// Whether `cell` (of the active sheet) shows a checkbox.
    pub(crate) fn shows_checkbox(&self, cell: CellRef) -> bool {
        self.session.doc().is_ok_and(|d| is_checkbox(&d.wb, d.wb.active_sheet, cell))
    }

    /// Refuses typing in `cell` when it shows a checkbox, saying why; `true` when refused.
    pub(crate) fn refuse_checkbox(&mut self, cell: CellRef) -> bool {
        let refused = self.shows_checkbox(cell);
        if refused {
            self.notify(CHECKBOX_LOCKED);
        }
        refused
    }

    /// The checkbox cell (a merged area's first cell) whose box is under document point (`x`, `y`).
    pub(crate) fn checkbox_at(&self, x: f64, y: f64) -> Option<CellRef> {
        let Hit::Cell(cell) = self.hit(x, y) else { return None };
        let (wb, sheet) = self.sheet()?;
        let area = sheet.merge_at(cell).unwrap_or(RangeRef::new(cell, cell));
        if !self.shows_checkbox(area.start) {
            return None;
        }
        let (x0, y0, x1, y1) = Self::cell_rect(sheet, area);
        let r = checkbox_rect(Rect { x0, y0, x1, y1 }, 1.0, wb.styles.get(sheet.style_id(area.start)));
        (x >= r.x0 - 1.0 && x <= r.x1 + 1.0 && y >= r.y0 - 1.0 && y <= r.y1 + 1.0).then_some(area.start)
    }

    /// Ticks or clears the checkbox of `cell` (a click on its box, Space).
    pub(crate) fn toggle_checkbox(&mut self, cell: CellRef) -> Result<(), String> {
        self.session.execute("cell.toggleCheckbox", json!({ "cell": cell.a1() })).map(|_| ()).map_err(crate::engine_error)
    }

    /// `editor.insertText {text, append?}`: `text` typed at the caret of the cell or shape being
    /// typed; else after the selected shape's text; else in the active cell — after what it holds,
    /// the cell left open (`append`: a symbol, as WPS does), or in its place (a date, Ctrl+;). A
    /// checkbox cell refuses it.
    pub(crate) fn insert_text(&mut self, text: &str, append: bool) -> Result<Json, String> {
        if text.is_empty() || text.chars().count() > 256 || text.chars().any(char::is_control) {
            return Err("editor.insertText: `text` is 1 to 256 characters, without control characters".into());
        }
        if let Some(edit) = self.editing.as_mut() {
            edit.insert(text);
            self.point = None;
            self.hint_pick = 0;
            self.edits += 1;
            return Ok(json!({ "editing": true }));
        }
        if self.object_insert(text) {
            return Ok(json!({ "shape": true }));
        }
        let cell = self.active();
        if self.refuse_checkbox(cell) {
            return Ok(json!({ "refused": CHECKBOX_LOCKED }));
        }
        if append {
            self.edit_in_place(cell);
            // A symbol is inserted, not typed: entered, it leaves a table beside it as it is.
            self.edit_inserted = true;
        } else {
            self.editing = Some(Edit::new("", None, false));
            self.edit_inserted = false;
        }
        if let Some(edit) = self.editing.as_mut() {
            edit.insert(text);
        }
        self.point = None;
        self.hint_pick = 0;
        self.edits += 1;
        Ok(json!({ "editing": true }))
    }
}
