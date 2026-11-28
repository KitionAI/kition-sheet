//! The copy the person sees, as WPS shows it (W-F1, W-F3, W-F6, W-F8): after Cmd+C (Cmd+X) the
//! copied cells' frame is a dashed one; pasting leaves it there, so the cells paste again, unless
//! they were cut (a cut pastes once). Enter on the target pastes once and ends the copy; Escape
//! ends it, typing in a cell ends it, and so does a change to the copied cells. Ending it leaves
//! the system clipboard as it is (WPS empties it: plan section 2, "not copied"), but a Cmd+V of
//! that same text then pastes nothing, as in WPS.
//!
//! Only the person's copy shows: an agent's `edit.copy` fills the same clipboard and leaves the
//! grid and the person's Enter alone. Session state, never saved.

use std::sync::Arc;

use super::*;

/// How many copied cells are compared with what they hold now before giving up and taking them
/// as they were (a whole column's copy compares its stored cells only).
const COMPARED_CELLS: usize = 20_000;

/// The copy the grid marks: its sheet (index), its cells, whether they were cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Copied {
    pub(crate) sheet: usize,
    pub(crate) range: RangeRef,
    pub(crate) cut: bool,
}

impl SheetDocument {
    /// The person's copy while it is still marked: the clipboard holds it, from this workbook, and
    /// its cells hold what they held when copied.
    pub(crate) fn copy_live(&self) -> Option<Copied> {
        let text = self.copy_mark.as_ref()?;
        let clip = self.session.clipboard.as_ref()?;
        let doc = self.session.doc().ok()?;
        if clip.text != *text || clip.doc_uid != doc.uid || !unchanged(&clip.wb, &doc.wb, clip.sheet, clip.range) {
            return None;
        }
        Some(Copied { sheet: clip.sheet, range: clip.range, cut: clip.cut })
    }

    /// The person copied (cut): the grid marks it.
    pub(crate) fn copy_marked(&mut self) {
        self.copy_mark = self.session.clipboard.as_ref().map(|clip| clip.text.clone());
        self.copy_spent = None;
        self.edits += 1;
    }

    /// Ends the person's copy (Escape, Enter's paste, typing, the copied cells changed): its mark
    /// goes and the clipboard within the workbook empties; the system clipboard keeps its text, but
    /// a paste of that text is the copy that ended. An agent's copy stays.
    pub(crate) fn end_copy(&mut self) {
        let Some(text) = self.copy_mark.take() else { return };
        if self.session.clipboard.as_ref().is_some_and(|clip| clip.text == text) {
            self.session.clipboard = None;
        }
        self.copy_spent = Some(text);
        self.edits += 1;
    }

    /// Whether a person's paste of `params` would paste the copy that ended (Escape, Enter's paste,
    /// a cut pasted once, typing): WPS pastes nothing then. A copy whose cells changed ends here.
    pub(crate) fn paste_ended(&mut self, params: &Json) -> bool {
        if self.copy_mark.is_some() && self.copy_live().is_none() {
            self.end_copy();
        }
        if self.copy_mark.is_some() {
            return false;
        }
        let trim = |text: &str| text.trim_end_matches(['\r', '\n']).to_owned();
        match (params.get("text").and_then(Json::as_str), self.copy_spent.as_deref()) {
            (Some(text), Some(spent)) => trim(text) == trim(spent),
            _ => false,
        }
    }

    /// Enter on the target, the copy still marked: pastes it once and ends it (WPS W-F6).
    /// Answers whether it did.
    pub(crate) fn enter_pastes(&mut self) -> Result<bool, String> {
        if self.copy_live().is_none() {
            return Ok(false);
        }
        self.tab_run = None;
        self.session.execute("edit.paste", json!({})).map_err(engine_error)?;
        self.end_copy();
        Ok(true)
    }

    /// The marching ants: a dashed frame round the copied cells, on their sheet.
    pub(crate) fn copy_frame(&self) -> Option<RangeRef> {
        let copied = self.copy_live()?;
        let shown = self.session.doc().ok()?.wb.active_sheet;
        (copied.sheet == shown).then_some(copied.range)
    }
}

/// Whether `range` of sheet `sheet` holds in `now` what it held in `then`: the same workbook, or
/// the same cells (values, formulas, formatting) in it.
fn unchanged(then: &Arc<Workbook>, now: &Arc<Workbook>, sheet: usize, range: RangeRef) -> bool {
    if Arc::ptr_eq(then, now) {
        return true;
    }
    let (Some(old), Some(new)) = (then.sheet(sheet), now.sheet(sheet)) else { return false };
    if old.name != new.name {
        return false;
    }
    let mut count = 0usize;
    for (cell, held) in old.cells.iter_range(range) {
        count += 1;
        if count > COMPARED_CELLS {
            return true;
        }
        if new.cell(cell) != Some(held) {
            return false;
        }
    }
    new.cells.iter_range(range).take(count + 1).count() == count
}
