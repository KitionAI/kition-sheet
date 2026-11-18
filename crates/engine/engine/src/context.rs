//! The editor's state a command may read besides its parameters (the active sheet, the selection
//! on it and the selections the other sheets keep, the selected drawing object, the editing mode,
//! the view mode and the Draw tab's pen), taken, set for one call and put back without an edit:
//! the revision stays, the workbook stays as clean or dirty as it was, nothing joins the undo
//! history. A host saves it before an agent's call and restores it afterwards, so no call depends
//! on or leaves behind what the person had active or selected (kition-sdk
//! `KitDocument::editor_state`, `set_context`; task card C4/C5/C6-sheet).
//!
//! Sheets are kept by their ids, so a state taken before a run that inserted, moved or deleted
//! sheets goes back to the same sheet (or, when the run deleted it, stays where it is).

use serde::{Deserialize, Serialize};

use crate::{Mode, Result, Selection, Session};

/// A snapshot of the editor's state ([`Session::editor_state`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorState {
    /// The active sheet's id (`Sheet::sheet_id`).
    pub sheet: u32,
    /// The selection on the active sheet.
    pub selection: Selection,
    /// The selection each other sheet keeps for when it is active again, by sheet id; a sheet not
    /// listed keeps the first cell.
    pub sheet_selections: Vec<(u32, Selection)>,
    /// The selected drawing object.
    pub drawing: Option<u32>,
    pub mode: Mode,
    pub view_mode: String,
    pub draw_tool: String,
    pub draw_color: String,
    pub draw_width: f32,
    pub draw_shape: Option<String>,
}

impl Session {
    /// The editor's state now; `None` without an open workbook.
    pub fn editor_state(&self) -> Option<EditorState> {
        let d = self.active()?;
        let sheet = d.wb.active()?.sheet_id;
        let sheet_selections = d
            .sheet_selections
            .iter()
            .enumerate()
            .filter(|(index, selection)| *index != d.wb.active_sheet && **selection != Selection::default())
            .filter_map(|(index, selection)| Some((d.wb.sheet(index)?.sheet_id, selection.clone())))
            .collect();
        Some(EditorState {
            sheet,
            selection: d.selection.clone(),
            sheet_selections,
            drawing: self.selected_shape,
            mode: self.mode,
            view_mode: self.view_mode.clone(),
            draw_tool: self.draw_tool.clone(),
            draw_color: self.draw_color.clone(),
            draw_width: self.draw_width,
            draw_shape: self.draw_shape.clone(),
        })
    }

    /// Makes sheet `index` the active one, with `selection` on it, as no edit: the workbook stays
    /// as clean or dirty as it was and the revision and the undo history stay. An index the
    /// workbook lacks leaves the active sheet as it is.
    pub fn show_sheet(&mut self, index: usize, selection: Option<Selection>) -> Result<()> {
        let d = self.doc_mut()?;
        if index < d.wb.sheets.len() {
            d.wb_switch_sheet(index);
        }
        if let Some(selection) = selection {
            d.selection = selection;
        }
        Ok(())
    }

    /// Puts back a state [`Session::editor_state`] answered. It is no edit. A sheet the workbook no
    /// longer has (the run deleted it) is dropped: the active sheet stays where it is, and the
    /// selection there is kept. A drawing object it no longer has is no longer selected.
    pub fn restore_editor_state(&mut self, state: &EditorState) -> Result<()> {
        let d = self.doc_mut()?;
        let index = d.wb.sheets.iter().position(|sheet| sheet.sheet_id == state.sheet);
        if let Some(index) = index {
            d.wb_switch_sheet(index);
            d.selection = state.selection.clone();
        }
        let count = d.wb.sheets.len();
        if d.sheet_selections.len() < count {
            d.sheet_selections.resize(count, Selection::default());
        }
        let active = d.wb.active_sheet;
        for (index, slot) in d.sheet_selections.iter_mut().enumerate().filter(|(index, _)| *index != active) {
            let id = d.wb.sheets.get(index).map(|sheet| sheet.sheet_id);
            *slot = state.sheet_selections.iter().find(|(kept, _)| Some(*kept) == id).map(|(_, selection)| selection.clone()).unwrap_or_default();
        }
        let exists = |id: u32| {
            d.wb.sheets.iter().any(|sheet| {
                sheet.charts.iter().any(|chart| chart.id == id)
                    || sheet.images.iter().any(|image| image.id == id)
                    || sheet.shapes.iter().any(|shape| shape.id == id || shape.groups.first() == Some(&id))
            })
        };
        let drawing = state.drawing.filter(|id| exists(*id));
        self.selected_shape = drawing;
        self.mode = state.mode;
        self.view_mode = state.view_mode.clone();
        self.draw_tool = state.draw_tool.clone();
        self.draw_color = state.draw_color.clone();
        self.draw_width = state.draw_width;
        self.draw_shape = state.draw_shape.clone();
        Ok(())
    }
}
