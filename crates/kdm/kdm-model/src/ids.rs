//! [`Ids`]: which replica object each piece of the view is, so [`crate::write_changes`] can find
//! the objects a command changed. Sheets, names, rows, columns and images have no ids in the
//! engine: their objects are kept here parallel to the view, rebuilt by every read and every
//! write.
//!
//! [`Known`] keeps what each person has of their own (G4): the active sheet, and each sheet's zoom,
//! active cell and scroll position. They are never written to the replica; a read with
//! [`crate::read_document_known`] gives them back to the sheets they belonged to.

use std::collections::HashMap;

use kition_kit::kdm::ObjectId;
use kition_sheet_core::CellRef;
use kition_sheet_model::Workbook;

/// One sheet's objects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetIds {
    pub object: ObjectId,
    /// The live `Row` objects, by row index (the rows below the list's end have none yet).
    pub rows: Vec<ObjectId>,
    /// The live `Col` objects, by column index.
    pub cols: Vec<ObjectId>,
    /// The `Image` objects, parallel to the sheet's `images`.
    pub images: Vec<ObjectId>,
    /// The `Shape` objects, parallel to the sheet's `shapes` (empty while a replica older than
    /// schema version 3 keeps them in `features`).
    pub shapes: Vec<ObjectId>,
    /// The `Chart` objects, parallel to the sheet's `charts` (empty while a replica older than
    /// schema version 4 keeps them in `features`).
    pub charts: Vec<ObjectId>,
    /// The `Cell` objects whose `text` differs from what their view's cell writes, with the text
    /// they hold (see `cells`; usually none): the writer diffs a cell's new text against it.
    pub texts: HashMap<ObjectId, String>,
    /// The `Cell` objects whose cell's text is made up of several objects' (a race on one new
    /// cell, see `cells`), with each part's object and text, the cell's own object first.
    pub parts: HashMap<ObjectId, Vec<(ObjectId, String)>>,
}

impl SheetIds {
    pub(crate) fn new(object: ObjectId) -> SheetIds {
        SheetIds { object, ..SheetIds::default() }
    }

    /// The row (`cols == false`) or column objects.
    pub fn lines(&self, cols: bool) -> &[ObjectId] {
        if cols { &self.cols } else { &self.rows }
    }

    pub(crate) fn lines_mut(&mut self, axis: crate::lines::Axis) -> &mut Vec<ObjectId> {
        match axis {
            crate::lines::Axis::Rows => &mut self.rows,
            crate::lines::Axis::Cols => &mut self.cols,
        }
    }

    pub(crate) fn axis(&self, axis: crate::lines::Axis) -> &[ObjectId] {
        match axis {
            crate::lines::Axis::Rows => &self.rows,
            crate::lines::Axis::Cols => &self.cols,
        }
    }
}

/// The objects of a view: `sheets` is parallel to `Workbook::sheets`, `names` to
/// `Workbook::names`, `styles` to the style table (each style's key in the `styles` map, the hash
/// of its content).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ids {
    pub sheets: Vec<SheetIds>,
    pub names: Vec<ObjectId>,
    pub styles: Vec<String>,
}

impl Ids {
    /// The index of the sheet whose object is `object`.
    pub fn sheet_of(&self, object: &ObjectId) -> Option<usize> {
        self.sheets.iter().position(|s| s.object == *object)
    }

    /// The object of row `row` of sheet `sheet`, if it has one.
    pub fn row(&self, sheet: usize, row: u32) -> Option<&ObjectId> {
        self.sheets.get(sheet)?.rows.get(row as usize)
    }

    /// The object of column `col` of sheet `sheet`, if it has one.
    pub fn col(&self, sheet: usize, col: u32) -> Option<&ObjectId> {
        self.sheets.get(sheet)?.cols.get(col as usize)
    }
}

/// One person's view of a sheet (never replicated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct View {
    pub zoom: u16,
    pub view_active: CellRef,
    pub view_top_left: CellRef,
}

/// The per-person state of a view (G4), by sheet object. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct Known {
    pub(crate) active: Option<ObjectId>,
    pub(crate) views: HashMap<ObjectId, View>,
}

impl Known {
    /// Remembers `wb`'s per-person state, its sheets' objects being `ids`.
    pub fn remember(&mut self, wb: &Workbook, ids: &Ids) {
        for (sheet, sid) in wb.sheets.iter().zip(&ids.sheets) {
            self.views.insert(sid.object.clone(), View { zoom: sheet.zoom, view_active: sheet.view_active, view_top_left: sheet.view_top_left });
        }
        if let Some(sid) = ids.sheets.get(wb.active_sheet) {
            self.active = Some(sid.object.clone());
        }
    }

    /// Gives `wb` (read with `ids`) the per-person state remembered for its sheets.
    pub(crate) fn restore(&self, wb: &mut Workbook, ids: &Ids) {
        for (k, sid) in ids.sheets.iter().enumerate() {
            let Some(view) = self.views.get(&sid.object) else { continue };
            if let Some(sheet) = wb.sheet_mut(k) {
                sheet.zoom = view.zoom;
                sheet.view_active = view.view_active;
                sheet.view_top_left = view.view_top_left;
            }
        }
        if let Some(k) = self.active.as_ref().and_then(|a| ids.sheet_of(a)) {
            wb.active_sheet = k;
        }
        wb.active_sheet = wb.active_sheet.min(wb.sheets.len().saturating_sub(1));
    }
}
