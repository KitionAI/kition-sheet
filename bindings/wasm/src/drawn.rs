//! What the grid draws, against another's step (kition-sdk `agentSkipsUnchangedPaint`): the page
//! repaints after an agent's step (or the host's) only when the step moved the revision or
//! `inspect()`, so a step that changes what [`SheetDocument::render`] draws must move one of them.
//!
//! The canvas draws the active workbook's content on the person's active sheet around the person's
//! active cell (their editor state, which the page puts back after each of another's steps), the
//! visible extent the editor asked for (`editor.viewport`), the cell or shape being typed, and the
//! host's scheme. Undoable edits, typing and the scheme already move the revision; a step that
//! changes the rest without an undo step (`formula.calculateNow` recalculating `RAND()` and
//! `NOW()`, another workbook made active, `editor.viewport`, a step that enters or drops the
//! person's typing) moves it through [`SheetDocument::drawn`]. Only another's step is compared: the
//! person's own commands repaint the page in full, and the editor scrolls through
//! `editor.viewport`, which must not count as a change of the document.

use std::sync::Arc;

use kition_kit::KitDocument;
use kition_kit::kdm::OriginKind;
use kition_sheet_engine::core::CellRef;
use kition_sheet_engine::model::Workbook;

use crate::SheetDocument;
use crate::edit::Edit;
use crate::objects::ShapeEdit;

/// What the grid draws from, before another's step.
pub(crate) struct Drawn {
    revision: u64,
    /// The active workbook (its session id) and its content.
    doc: Option<(u64, Arc<Workbook>)>,
    viewport: (f64, f64),
    editing: Option<Edit>,
    edit_home: Option<(usize, CellRef)>,
    shape_edit: Option<ShapeEdit>,
}

impl Drawn {
    /// What `doc` draws from now, when the next command is another's (an agent's, the host's).
    pub(crate) fn before(doc: &SheetDocument) -> Option<Self> {
        (doc.session.origin().kind != OriginKind::Person).then(|| Self {
            revision: doc.revision(),
            doc: doc.session.active().map(|st| (st.uid, st.wb.clone())),
            viewport: doc.viewport,
            editing: doc.editing.clone(),
            edit_home: doc.edit_home,
            shape_edit: doc.shape_edit.clone(),
        })
    }

    /// Whether the step left the drawing as it was, or moved the revision itself.
    pub(crate) fn same(&self, doc: &SheetDocument) -> bool {
        if doc.revision() != self.revision {
            return true;
        }
        let content = match (&self.doc, doc.session.active()) {
            (None, None) => true,
            (Some((uid, before)), Some(st)) => *uid == st.uid && same_content(before, &st.wb),
            _ => false,
        };
        content
            && self.viewport == doc.viewport
            && self.editing == doc.editing
            && self.edit_home == doc.edit_home
            && self.shape_edit == doc.shape_edit
    }
}

/// Whether two states of a workbook hold the same content, whichever sheet is active (the active
/// sheet is editor state, which the page puts back after another's step). Cheap when the step left
/// the workbook or its sheets untouched; a sheet it touched is compared in full.
fn same_content(before: &Arc<Workbook>, after: &Arc<Workbook>) -> bool {
    if Arc::ptr_eq(before, after) {
        return true;
    }
    let rest = |wb: &Workbook| Workbook { sheets: Vec::new(), active_sheet: 0, ..wb.clone() };
    before.sheets.len() == after.sheets.len()
        && before.sheets.iter().zip(&after.sheets).all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
        && rest(before) == rest(after)
}
