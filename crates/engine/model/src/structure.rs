//! Structural edits (Kition): what an engine command did to the grid's structure, as opposed to
//! the content it changed.
//!
//! The engine records one [`StructuralEdit`] wherever it shifts a sheet's cells and features and
//! rewrites the workbook's formulas for an insert, a delete or a move (`cmd/sheet.rs`,
//! `cmd/edit.rs`). Kition's KDM bridge (`kdm-model`) reads them after each command to turn row
//! and column inserts and deletes into list edits of the replica's stable row and column objects,
//! so an insert writes the new rows only, not every row below.

pub use kition_sheet_formula::adjust::{Axis, Edit};

/// One structural edit: `edit` on the sheet at index `sheet` when the command ran (for
/// [`Edit::MoveToSheet`], the source sheet).
#[derive(Clone, Debug, PartialEq)]
pub struct StructuralEdit {
    pub sheet: usize,
    pub edit: Edit,
}

impl StructuralEdit {
    pub fn new(sheet: usize, edit: Edit) -> StructuralEdit {
        StructuralEdit { sheet, edit }
    }
}
