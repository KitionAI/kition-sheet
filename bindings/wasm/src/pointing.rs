//! The cell editor's formula entry (`point`): pointing at cells, on this sheet or another one,
//! inserts their references; F4 cycles a reference's `$`; the function list and the argument hint
//! follow the caret. WPS's behaviour, which Excel shares.

use kition_sheet_engine::core::{CellRef, MAX_COLS, MAX_ROWS};
use serde_json::{Value as Json, json};

use crate::{SheetDocument, engine_error, point};

/// The reference being pointed at: on which sheet, its anchor and moving corner, whether whole
/// columns (`Some(true)`) or rows were taken from a heading, and its characters in the text.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pointing {
    pub sheet: usize,
    pub anchor: CellRef,
    pub end: CellRef,
    pub whole: Option<bool>,
    pub from: usize,
    pub to: usize,
}

impl SheetDocument {
    fn active_sheet_index(&self) -> usize {
        self.session.doc().map_or(0, |d| d.wb.active_sheet)
    }

    /// The sheet and cell the formula being typed belongs to.
    pub(crate) fn edit_home(&self) -> (usize, CellRef) {
        self.edit_home.unwrap_or((self.active_sheet_index(), self.active()))
    }

    /// Whether the cell being typed is on another sheet than the one shown (the person went to
    /// another sheet to point at its cells).
    pub(crate) fn editing_elsewhere(&self) -> bool {
        self.edit_home.is_some_and(|(sheet, _)| sheet != self.active_sheet_index())
    }

    /// Whether pointing at a cell now inserts its reference: a formula is being typed and the
    /// caret is where a reference can go, or right after the reference just pointed at.
    pub(crate) fn pointing_ready(&self) -> bool {
        let Some(edit) = self.editing.as_ref() else { return false };
        let (from, to) = edit.selection();
        if from != to {
            // The reference pointed at may be selected (AutoSum's guess): pointing replaces it.
            return self.point.is_some_and(|p| (p.from, p.to) == (from, to) && p.to == edit.caret);
        }
        self.point.is_some_and(|p| p.to == edit.caret) || point::can_point(edit.chars(), edit.caret)
    }

    /// Points at `anchor`..`end` on the sheet shown: the reference goes in at the caret, or in
    /// place of the one pointed at before.
    pub(crate) fn point_at(&mut self, anchor: CellRef, end: CellRef, whole: Option<bool>) {
        let shown = self.active_sheet_index();
        let home = self.edit_home().0;
        let sheet_name = (shown != home).then(|| self.session.doc().ok().and_then(|d| d.wb.sheet(shown).map(|s| s.name.clone()))).flatten();
        let text = point::range_text(sheet_name.as_deref(), anchor, end, whole);
        let Some(edit) = self.editing.as_mut() else { return };
        let (from, to) = match self.point {
            Some(p) if p.to == edit.caret => (p.from, p.to),
            _ => (edit.caret, edit.caret),
        };
        edit.replace(from, to, &text);
        self.point = Some(Pointing { sheet: shown, anchor, end, whole, from, to: from + text.chars().count() });
        self.edits += 1;
    }

    /// A press in the grid while a formula is being typed where a reference can go: points at the
    /// cell (Shift: from the reference pointed at before), or at whole columns or rows.
    pub(crate) fn point_press(&mut self, hit: crate::Hit, shift: bool) -> bool {
        if !self.pointing_ready() {
            return false;
        }
        let previous = self.point.filter(|p| p.sheet == self.active_sheet_index() && shift);
        match hit {
            crate::Hit::Cell(cell) => {
                let anchor = previous.map_or(cell, |p| p.anchor);
                self.point_at(anchor, cell, None);
            }
            crate::Hit::Column(col) => self.point_at(CellRef::new(0, col), CellRef::new(MAX_ROWS - 1, col), Some(true)),
            crate::Hit::Row(row) => self.point_at(CellRef::new(row, 0), CellRef::new(row, MAX_COLS - 1), Some(false)),
            _ => return false,
        }
        self.drag = crate::Drag::Point;
        true
    }

    /// Dragging after a pointing press widens the reference.
    pub(crate) fn point_drag_to(&mut self, hit: crate::Hit) {
        let Some(p) = self.point else { return };
        let target = match hit {
            crate::Hit::Cell(c) => c,
            crate::Hit::Column(col) => CellRef::new(0, col),
            crate::Hit::Row(row) => CellRef::new(row, 0),
            _ => return,
        };
        match p.whole {
            None => self.point_at(p.anchor, target, None),
            Some(true) => self.point_at(p.anchor, CellRef::new(MAX_ROWS - 1, target.col), Some(true)),
            Some(false) => self.point_at(p.anchor, CellRef::new(target.row, MAX_COLS - 1), Some(false)),
        }
    }

    /// An arrow key in Enter mode while a formula is being typed where a reference can go: points
    /// at the cell next to the one pointed at (at first, next to the cell being typed); Shift
    /// widens the reference.
    pub(crate) fn point_step(&mut self, dr: i64, dc: i64, shift: bool) -> bool {
        let in_place = self.editing.as_ref().is_some_and(|e| e.in_place);
        if in_place || !self.pointing_ready() {
            return false;
        }
        let shown = self.active_sheet_index();
        let start = match self.point.filter(|p| p.sheet == shown && p.whole.is_none()) {
            Some(p) => p,
            None => {
                let from = if self.edit_home().0 == shown { self.edit_home().1 } else { self.active() };
                Pointing { sheet: shown, anchor: from, end: from, whole: None, from: 0, to: 0 }
            }
        };
        let moved = |c: CellRef| {
            CellRef::new(
                (i64::from(c.row) + dr).clamp(0, i64::from(MAX_ROWS) - 1) as u32,
                (i64::from(c.col) + dc).clamp(0, i64::from(MAX_COLS) - 1) as u32,
            )
        };
        if self.point.is_some() && shift {
            self.point_at(start.anchor, moved(start.end), None);
        } else {
            let at = moved(start.end);
            self.point_at(at, at, None);
        }
        true
    }

    /// F4 in the cell editor: the reference at the caret cycles through `$A$1`, `A$1`, `$A1`, `A1`.
    pub(crate) fn cycle_reference(&mut self) -> bool {
        let Some(edit) = self.editing.as_mut() else { return false };
        let (from, to) = edit.selection();
        let Some((text, _, end)) = point::cycle_reference(edit.chars(), from, to) else { return false };
        edit.set_text(&text, end);
        self.point = None;
        self.edits += 1;
        true
    }

    /// The function list and argument hint for the cell being typed, with where to show them
    /// (the document point under the cell editor).
    pub(crate) fn cell_hint(&self) -> Option<(point::Hint, [f64; 2])> {
        let edit = self.editing.as_ref()?;
        let b = self.edit_box(0.0, 0.0, 1.0)?;
        let hint = point::hint(edit.chars(), edit.caret);
        (!hint.functions.is_empty() || hint.call.is_some()).then_some((hint, [b.rect.x0, b.rect.y1]))
    }

    /// Up and Down choose in the function list, Tab takes the chosen function.
    pub(crate) fn hint_key(&mut self, key: &str) -> bool {
        let Some(edit) = self.editing.as_ref() else { return false };
        let hint = point::hint(edit.chars(), edit.caret);
        let (Some(prefix), false) = (hint.prefix, hint.functions.is_empty()) else { return false };
        match key {
            "ArrowDown" => self.hint_pick = (self.hint_pick + 1).min(hint.functions.len() - 1),
            "ArrowUp" => self.hint_pick = self.hint_pick.saturating_sub(1),
            "Tab" => {
                let name = hint.functions.get(self.hint_pick).or(hint.functions.first()).map(|f| f.name.clone()).unwrap_or_default();
                self.accept_function(prefix, &name);
            }
            _ => return false,
        }
        self.edits += 1;
        true
    }

    fn accept_function(&mut self, prefix: (usize, usize), name: &str) {
        let Some(edit) = self.editing.as_mut() else { return };
        let (text, caret) = point::accept(edit.chars(), prefix, name);
        edit.set_text(&text, caret);
        self.hint_pick = 0;
        self.point = None;
        self.edits += 1;
    }

    /// `editor.acceptFunction {name}`: a click in the function list.
    /// Without a name, the function chosen in the list (as Tab takes it).
    pub(crate) fn accept_function_command(&mut self, params: &Json) -> Result<Json, String> {
        let Some(edit) = self.editing.as_ref() else { return Ok(json!({ "editing": false, "text": null })) };
        let hint = point::hint(edit.chars(), edit.caret);
        let name = match params.get("name").and_then(Json::as_str) {
            Some(name) => name.to_ascii_uppercase(),
            None => match hint.functions.get(self.hint_pick).or(hint.functions.first()) {
                Some(f) => f.name.clone(),
                None => return Ok(json!({ "editing": true, "text": edit.text() })),
            },
        };
        let prefix = hint.prefix.unwrap_or((edit.caret, edit.caret));
        self.accept_function(prefix, &name);
        Ok(json!({ "editing": true, "text": self.editing.as_ref().map(crate::edit::Edit::text) }))
    }

    /// `editor.cycleReference {text, start, end}` (the formula bar's F4; UTF-16 offsets).
    pub(crate) fn cycle_reference_command(params: &Json) -> Json {
        let text = params.get("text").and_then(Json::as_str).unwrap_or_default();
        let units = |key: &str| params.get(key).and_then(Json::as_u64).unwrap_or(0) as usize;
        let chars: Vec<char> = text.chars().collect();
        let to_chars = |u: usize| String::from_utf16_lossy(&text.encode_utf16().take(u).collect::<Vec<u16>>()).chars().count();
        match point::cycle_reference(&chars, to_chars(units("start")), to_chars(units("end"))) {
            // The caret goes after the reference, as in the cell editor.
            Some((new, _, end)) => {
                let caret = new.chars().take(end).map(char::len_utf16).sum::<usize>();
                json!({ "text": new, "start": caret, "end": caret })
            }
            None => json!({ "text": text, "start": units("start"), "end": units("end") }),
        }
    }

    /// Before a sheet tab command while a formula is typed where a reference can go: the formula
    /// stays open on its cell, and the next sheet's cells can be pointed at (`true`).
    pub(crate) fn keep_formula_across_sheets(&mut self, command: &str) -> bool {
        if !matches!(command, "sheet.activate" | "sheet.next" | "sheet.previous") || !self.pointing_ready() {
            return false;
        }
        if self.edit_home.is_none() {
            self.edit_home = Some((self.active_sheet_index(), self.active()));
        }
        self.point = None;
        true
    }

    /// Back on the sheet the formula belongs to (after Enter or Escape on another sheet).
    pub(crate) fn go_home(&mut self) -> Result<(), String> {
        if let Some((sheet, _)) = self.edit_home.take()
            && sheet != self.active_sheet_index()
        {
            self.session.execute("sheet.activate", json!({ "sheet": sheet })).map_err(engine_error)?;
        }
        Ok(())
    }
}

/// The hint as the editor shows it: the functions (with the one chosen), the word they replace
/// (UTF-16) and the call with its bold argument.
pub(crate) fn hint_json(hint: &point::Hint, chars: &[char], pick: usize) -> Json {
    let utf16 = |c: usize| chars.iter().take(c).map(|ch| ch.len_utf16()).sum::<usize>();
    json!({
        "functions": hint.functions,
        "pick": pick.min(hint.functions.len().saturating_sub(1)),
        "replace": hint.prefix.map(|(a, b)| [utf16(a), utf16(b)]),
        "call": hint.call,
    })
}
