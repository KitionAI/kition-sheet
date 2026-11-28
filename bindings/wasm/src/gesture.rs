//! The grid's gestures under the select tool: a press decides which one it begins (in the order
//! `pointer` tries them), and from then on its moves and its release go to that one, here, until
//! it ends. A gesture changes nothing in the workbook while it runs; the release runs its one
//! command. Escape (and a pointer the system took away, which the editor reports as Escape) drops a
//! gesture whose release would change the workbook, leaving no undo step.

use super::*;

/// The one gesture a press began, until its release ends it (or Escape, or a press that comes
/// without a release before it). Two never run at once.
#[derive(Default)]
pub(crate) enum Drag {
    #[default]
    None,
    /// Cells, or whole rows and columns from a heading, being selected; with the format painter up
    /// (`paint`), what the release selects is painted.
    Select { paint: bool },
    /// Characters of the cell being typed being selected.
    Text,
    /// Cells being pointed at for the formula being typed (`pointing`).
    Point,
    /// A column width or row height (`resize`).
    Resize(resize::Resize),
    /// The selection's block, by its border (`moving`).
    Move(moving::MoveDrag),
    /// A drawing object's move, resize, turn or adjust handle, or a shape being drawn (`objects`).
    Object(objects::ObjectDrag),
    /// A pen stroke or an eraser's path (the Draw tab, `objects`).
    Ink(objects::InkDrag),
    /// The fill handle (`filling`).
    Fill(filling::FillDrag),
}

impl Drag {
    /// Whether Escape drops it: a gesture whose release would change the workbook.
    pub(crate) fn cancels(&self) -> bool {
        matches!(self, Drag::Resize(_) | Drag::Move(_) | Drag::Object(_) | Drag::Ink(_) | Drag::Fill(_))
    }
}

impl SheetDocument {
    /// A press on the cells or the headings: what is being typed goes in first (staying where it
    /// is when its cell's validation holds it), then the press selects (Shift extends, Cmd/Ctrl
    /// adds or takes out) and a drag from it selects more; an outline button acts at once.
    pub(crate) fn select_press(&mut self, event: &ToolPointer, hit: Hit) -> Result<(), String> {
        self.tab_run = None;
        // A formula typed for another sheet's cell goes in, and its sheet shows again.
        let elsewhere = self.editing_elsewhere();
        self.commit()?;
        // What its cell's validation holds stays there until the person answers.
        if elsewhere || self.editing.is_some() {
            return Ok(());
        }
        // With the format painter up, the cells (rows, columns) pressed and dragged over are
        // painted when the button goes up.
        let paint = self.session.format_painter.is_some() && matches!(hit, Hit::Cell(_) | Hit::Row(_) | Hit::Column(_) | Hit::All);
        self.drag = Drag::Select { paint };
        let adds = (event.mods.command || event.mods.control) && !paint;
        // A press on a checkbox's box ticks or clears it (beside the box: only selects).
        let ticked = if paint || event.mods.shift || event.mods.command || event.mods.control { None } else { self.checkbox_at(event.x, event.y) };
        match hit {
            Hit::Cell(cell) if event.mods.shift => {
                let anchor = self.session.active().map(|st| st.selection.anchor).unwrap_or(cell);
                self.select(RangeRef::new(anchor, cell), anchor)?;
            }
            // Cmd+click (Ctrl+click) on a selected cell takes it out of the selection (WPS W-A6);
            // the press is not a drag.
            Hit::Cell(cell) if adds && self.deselect(cell)? => self.drag = Drag::None,
            // Cmd+click (Ctrl+click) adds a cell to the selection, a drag from it a range (WPS,
            // Excel); it becomes the active cell.
            Hit::Cell(cell) if adds => {
                let mut ranges = self.session.active().map(|st| st.selection.a1()).unwrap_or_default();
                if !ranges.is_empty() {
                    ranges.push(',');
                }
                ranges.push_str(&cell.a1());
                self.session.execute("select.set", json!({ "range": ranges, "active": cell.a1() })).map_err(engine_error)?;
                if let Ok(st) = self.session.doc_mut() {
                    st.selection.anchor = cell;
                }
            }
            Hit::Cell(cell) => {
                self.select(RangeRef::new(cell, cell), cell)?;
                if let Some(at) = ticked {
                    self.toggle_checkbox(at)?;
                }
            }
            // Shift+click on a heading extends from the selection's anchor to that column or row.
            Hit::Column(col) if event.mods.shift => {
                let anchor = self.session.active().map(|st| st.selection.anchor).unwrap_or_default();
                self.select(RangeRef::cols(anchor.col.min(col), anchor.col.max(col)), CellRef::new(0, anchor.col))?;
            }
            Hit::Row(row) if event.mods.shift => {
                let anchor = self.session.active().map(|st| st.selection.anchor).unwrap_or_default();
                self.select(RangeRef::rows(anchor.row.min(row), anchor.row.max(row)), CellRef::new(anchor.row, 0))?;
            }
            // Cmd+click (Ctrl+click) on a heading adds its column or row, a drag from it more (WPS
            // W-A6; walkthrough B8).
            Hit::Column(col) if adds => self.add_area(RangeRef::cols(col, col), CellRef::new(0, col))?,
            Hit::Row(row) if adds => self.add_area(RangeRef::rows(row, row), CellRef::new(row, 0))?,
            Hit::Column(col) => self.select(RangeRef::new(CellRef::new(0, col), CellRef::new(MAX_ROWS - 1, col)), CellRef::new(0, col))?,
            Hit::Row(row) => self.select(RangeRef::new(CellRef::new(row, 0), CellRef::new(row, MAX_COLS - 1)), CellRef::new(row, 0))?,
            // The corner selects the whole sheet (Cmd+A first takes the region around the active cell).
            Hit::All => self.select(RangeRef::all(), self.active())?,
            Hit::Group { rows, start, end, hidden } => {
                self.drag = Drag::None;
                self.toggle_group(rows, start, end, hidden)?;
            }
            Hit::Level { rows, level } => {
                self.drag = Drag::None;
                self.show_level(rows, level)?;
            }
            Hit::Gutter => self.drag = Drag::None,
        }
        Ok(())
    }

    /// A move of the pointer during the gesture under way.
    pub(crate) fn drag_to(&mut self, event: &ToolPointer, hit: Hit) -> Result<(), String> {
        let dragged = event.action == PointerAction::Drag;
        match &self.drag {
            Drag::None => {}
            Drag::Select { .. } if dragged => self.select_to(event, hit),
            // Only a caret that moved repaints: the drag reports far more often than it crosses a
            // character, and each repaint draws the whole view again.
            Drag::Text if dragged => {
                if let Some(at) = self.edit_index(event.x, event.y, true)
                    && let Some(edit) = self.editing.as_mut()
                    && at != edit.caret
                {
                    edit.move_to(at, true);
                    self.edits += 1;
                }
            }
            Drag::Point if dragged => self.point_drag_to(hit),
            Drag::Resize(_) => self.resize_drag_to(event),
            Drag::Move(_) => self.move_drag_to(event),
            Drag::Object(_) if dragged => self.object_drag_to(event),
            Drag::Ink(_) if dragged => self.ink_drag_to(event),
            Drag::Fill(_) => self.fill_drag_to(event),
            _ => {}
        }
        Ok(())
    }

    /// The release of the gesture under way: its one command, if it has one.
    pub(crate) fn release(&mut self, event: &ToolPointer) -> Result<(), String> {
        match std::mem::take(&mut self.drag) {
            Drag::None | Drag::Text | Drag::Point => Ok(()),
            Drag::Select { paint } => {
                if paint && self.session.format_painter.is_some() {
                    let range = self.session.active().map(|st| st.selection.current()).unwrap_or_default();
                    self.session.execute("edit.formatPainter", json!({ "apply": range.a1() })).map_err(engine_error)?;
                }
                Ok(())
            }
            Drag::Resize(resize) => self.resize_release(resize),
            Drag::Move(drag) => self.move_release(drag, event),
            Drag::Object(drag) => self.object_release(drag),
            Drag::Ink(drag) => self.ink_release(drag),
            Drag::Fill(drag) => self.fill_release(drag, event.mods),
        }
    }

    /// Escape during a gesture: one that would change the workbook is dropped, and the Escape is
    /// spent (`true`); a selection, a caret or a reference being dragged just ends, and Escape goes
    /// on to do what it does (end the typing, put the format painter down).
    pub(crate) fn escape_drag(&mut self) -> bool {
        let drag = std::mem::take(&mut self.drag);
        if !drag.cancels() {
            return false;
        }
        if matches!(drag, Drag::Object(_)) {
            self.session.draw_shape = None;
        }
        self.move_hover = None;
        self.edits += 1;
        true
    }

    /// A drag of cells, or of whole rows or columns from a heading, to `hit`. A drag of cells that
    /// reaches the headings (or the outline gutter) goes to the nearest cell, as the editor holds
    /// a pointer past the grid's edge at the edge while it scrolls (`web/src/edgeScroll.ts`).
    fn select_to(&mut self, event: &ToolPointer, hit: Hit) {
        let near = self.cell_near(event.x, event.y);
        let Ok(st) = self.session.doc_mut() else { return };
        let current = st.selection.current();
        let anchor = st.selection.anchor;
        let to = match hit {
            Hit::Cell(cell) => Some(cell),
            Hit::Row(row) => Some(CellRef::new(row, anchor.col)),
            Hit::Column(col) => Some(CellRef::new(anchor.row, col)),
            _ => None,
        };
        // A drag that began on a heading selects whole rows or columns.
        let lines = match (current.is_full_rows(), current.is_full_cols(), to) {
            (true, false, Some(to)) => Some(RangeRef::rows(anchor.row.min(to.row), anchor.row.max(to.row))),
            (false, true, Some(to)) => Some(RangeRef::cols(anchor.col.min(to.col), anchor.col.max(to.col))),
            _ => None,
        };
        match (lines, hit) {
            (Some(range), _) => {
                if let Some(last) = st.selection.ranges.last_mut() {
                    *last = range;
                }
            }
            (None, _) => st.selection.extend_to(near),
        }
    }
}
