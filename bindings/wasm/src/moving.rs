//! Moving cells by dragging the selection's border, as WPS and Excel do: over the border the
//! pointer is the move cursor; a drag shows where the block will land (a dashed frame, or with
//! Shift held the line where it goes in between the cells) and drops it there with
//! `edit.moveCells`: a move, a copy with Cmd held (WPS W-P2; Option is a plain move, W-P3; on a
//! Mac a Ctrl press is the system's secondary click, elsewhere Ctrl is the main modifier and
//! copies). Whole selected rows and columns move as rows and columns. A double-click on the
//! border goes to the end of the data that way (W-P6), as Cmd with the arrow does. Dropping onto cells that hold something asks first: the
//! editor's 「是否替换目标单元格内容？」 (`dialog` `moveCells`).
//!
//! A filter button's press opens the editor's filter list (`dialog` `filterMenu`) instead of
//! selecting.

use super::*;

/// How near the selection's border a pointer grabs it, in screen pixels either side.
const BORDER_SLOP: f64 = 3.0;

/// A drag of the selection's border under way.
pub(crate) struct MoveDrag {
    /// The block being moved (whole rows or columns when they are selected).
    from: RangeRef,
    /// The pointer's cell at the press, from the block's top-left.
    grab: (i64, i64),
    /// Where it was pressed (document pixels): a press released there is a click.
    press: (f64, f64),
    /// Where the block would land now.
    target: Option<Target>,
}

/// A press on the selection's border, kept for a double-click there: its click selects the cell
/// pressed (perhaps the one beside the selection), and the double-click jumps from the selection
/// as it was.
pub(crate) struct BorderPress {
    at: (f64, f64),
    range: RangeRef,
    active: CellRef,
    /// The way the side pressed faces: down, up, left or right.
    toward: (i64, i64),
}

/// Whether a press copies the block: Cmd (the main modifier; kit-editor gives Ctrl as Cmd too).
fn copies(mods: ToolMods) -> bool {
    mods.command || mods.control
}

/// Where a dragged block lands: its new top-left, whether it is a copy, and with Shift where it
/// goes in (`Some(true)`: between rows, the cells below moving down; `Some(false)`: between
/// columns, the cells right of it moving right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Target {
    to: CellRef,
    copy: bool,
    insert: Option<bool>,
}

impl SheetDocument {
    /// The selection a drag of its border would move: its one area (a merged cell's whole area),
    /// unless something is being typed, pointed at or painted.
    fn movable(&self) -> Option<RangeRef> {
        if self.editing.is_some() || self.point.is_some() || self.session.format_painter.is_some() || self.session.selected_shape.is_some() {
            return None;
        }
        let st = self.session.active()?;
        if st.selection.ranges.len() != 1 {
            return None;
        }
        let range = st.selection.current();
        if range.is_full_rows() && range.is_full_cols() {
            return None;
        }
        let (_, sheet) = self.sheet()?;
        Some(if range.is_single() { sheet.merge_at(range.start).unwrap_or(range) } else { range })
    }

    /// The selection's rectangle on the canvas (document pixels), within the grid's extent.
    fn movable_rect(&self, range: RangeRef) -> Option<(f64, f64, f64, f64)> {
        let (_, sheet) = self.sheet()?;
        let (cols, rows) = self.extent();
        let end = CellRef::new(range.end.row.min(rows.saturating_sub(1)), range.end.col.min(cols.saturating_sub(1)));
        Some(Self::cell_rect(sheet, RangeRef::new(range.start, end)))
    }

    /// Whether (`x`, `y`) is on the border of the selection, among the cells.
    pub(crate) fn on_selection_border(&self, x: f64, y: f64, zoom: f64) -> bool {
        let (Some(range), Some((_, sheet))) = (self.movable(), self.sheet()) else { return false };
        if x < header_w(sheet) || y < header_h(sheet) {
            return false;
        }
        let Some((x0, y0, x1, y1)) = self.movable_rect(range) else { return false };
        let slop = BORDER_SLOP / zoom.max(0.05);
        let along_x = x >= x0 - slop && x <= x1 + slop;
        let along_y = y >= y0 - slop && y <= y1 + slop;
        let near_side = (x - x0).abs() <= slop || (x - x1).abs() <= slop;
        let near_edge = (y - y0).abs() <= slop || (y - y1).abs() <= slop;
        (near_side && along_y) || (near_edge && along_x)
    }

    /// The side of the selection's border under (`x`, `y`), as the way it faces.
    fn border_side(&self, x: f64, y: f64) -> Option<(i64, i64)> {
        let (x0, y0, x1, y1) = self.movable_rect(self.movable()?)?;
        [((x - x0).abs(), (0, -1)), ((x - x1).abs(), (0, 1)), ((y - y0).abs(), (-1, 0)), ((y - y1).abs(), (1, 0))]
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, toward)| toward)
    }

    /// A double-click on the selection's border: the active cell goes to the end of the data
    /// that way (WPS W-P6), from the selection as it was before the double-click's first click.
    fn border_double_click(&mut self, event: &ToolPointer) -> Result<bool, String> {
        let near = |press: &BorderPress| {
            let slop = (BORDER_SLOP + 2.0) / event.zoom.max(0.05);
            (press.at.0 - event.x).abs() <= slop && (press.at.1 - event.y).abs() <= slop
        };
        let Some(press) = self.border_press.take().filter(near) else { return Ok(false) };
        self.select(press.range, press.active)?;
        self.step(press.toward.0, press.toward.1, false, true)?;
        self.edits += 1;
        Ok(true)
    }

    /// A press on the selection's border begins moving it; hovering there shows the move cursor
    /// (the copy cursor with Cmd held). Answers whether the press was the border's.
    pub(crate) fn move_press(&mut self, event: &ToolPointer) -> Result<bool, String> {
        if event.action == PointerAction::DoubleClick && self.border_double_click(event)? {
            return Ok(true);
        }
        let copy = copies(event.mods);
        let on = self.on_selection_border(event.x, event.y, event.zoom);
        self.move_hover = on.then_some(copy);
        if event.action == PointerAction::Down && !on {
            self.border_press = None;
        }
        if !on || event.action != PointerAction::Down {
            // Hovering shows the cursor; the rest is the grid's.
            return Ok(false);
        }
        let Some(from) = self.movable() else { return Ok(false) };
        let cell = match self.hit(event.x, event.y) {
            Hit::Cell(cell) => cell,
            _ => return Ok(false),
        };
        // The pointer's cell, clamped into the block (the border is shared with the cells round it).
        let inside = CellRef::new(cell.row.clamp(from.start.row, from.end.row), cell.col.clamp(from.start.col, from.end.col));
        let grab = (i64::from(inside.row) - i64::from(from.start.row), i64::from(inside.col) - i64::from(from.start.col));
        // The second press of a double-click keeps the first's selection and side.
        let again = self.border_press.as_ref().is_some_and(|press| {
            let slop = (BORDER_SLOP + 2.0) / event.zoom.max(0.05);
            (press.at.0 - event.x).abs() <= slop && (press.at.1 - event.y).abs() <= slop
        });
        if !again && let (Some(toward), Some(st)) = (self.border_side(event.x, event.y), self.session.active()) {
            self.border_press = Some(BorderPress { at: (event.x, event.y), range: from, active: st.selection.active, toward });
        }
        self.drag = Drag::Move(MoveDrag { from, grab, press: (event.x, event.y), target: None });
        self.edits += 1;
        Ok(true)
    }

    /// The landing place follows the pointer.
    pub(crate) fn move_drag_to(&mut self, event: &ToolPointer) {
        let copy = copies(event.mods);
        let target = match &self.drag {
            Drag::Move(drag) => self.move_target(drag, event),
            _ => return,
        };
        if let Drag::Move(drag) = &mut self.drag
            && target.is_some()
        {
            drag.target = target;
        }
        self.move_hover = Some(copy);
        self.edits += 1;
    }

    /// The release drops the block where it lands; pressed and released in place, it is a click
    /// on the cell there.
    pub(crate) fn move_release(&mut self, drag: MoveDrag, event: &ToolPointer) -> Result<(), String> {
        self.move_hover = None;
        self.edits += 1;
        let moved = (event.x - drag.press.0).abs() > 2.0 || (event.y - drag.press.1).abs() > 2.0;
        let target = self.move_target(&drag, event).or(drag.target);
        match target {
            Some(target) if moved => self.drop_block(drag.from, target),
            _ => {
                if let Hit::Cell(cell) = self.hit(drag.press.0, drag.press.1) {
                    self.select(RangeRef::new(cell, cell), cell)?;
                }
                Ok(())
            }
        }
    }

    /// Where the dragged block would land with the pointer at `event`.
    fn move_target(&self, drag: &MoveDrag, event: &ToolPointer) -> Option<Target> {
        let (_, sheet) = self.sheet()?;
        let from = drag.from;
        let (rows, cols) = (from.is_full_rows(), from.is_full_cols());
        let cell = match self.hit(event.x, event.y) {
            Hit::Cell(cell) => cell,
            Hit::Row(row) => CellRef::new(row, 0),
            Hit::Column(col) => CellRef::new(0, col),
            _ => return drag.target,
        };
        let copy = copies(event.mods);
        let (h, w) = (i64::from(from.height()), i64::from(from.width()));
        let max_row = i64::from(MAX_ROWS) - h;
        let max_col = i64::from(MAX_COLS) - w;
        let place = |row: i64, col: i64| CellRef::new(row.clamp(0, max_row.max(0)) as u32, col.clamp(0, max_col.max(0)) as u32);
        if event.mods.shift {
            // Between the cells: the boundary of the pointer's cell nearest it.
            let (x0, y0, x1, y1) = Self::cell_rect(sheet, RangeRef::cell(cell));
            let (fx, fy) = ((event.x - x0) / (x1 - x0).max(1.0), (event.y - y0) / (y1 - y0).max(1.0));
            let to_edge_h = fy.min(1.0 - fy) * (y1 - y0);
            let to_edge_v = fx.min(1.0 - fx) * (x1 - x0);
            let down = if rows {
                true
            } else if cols {
                false
            } else {
                to_edge_h <= to_edge_v
            };
            return Some(if down {
                let row = i64::from(cell.row) + i64::from(fy > 0.5);
                let col = if rows { 0 } else { i64::from(cell.col) - drag.grab.1 };
                Target { to: place(row.min(i64::from(MAX_ROWS) - 1), col), copy, insert: Some(true) }
            } else {
                let col = i64::from(cell.col) + i64::from(fx > 0.5);
                let row = if cols { 0 } else { i64::from(cell.row) - drag.grab.0 };
                Target { to: place(row, col.min(i64::from(MAX_COLS) - 1)), copy, insert: Some(false) }
            });
        }
        let row = if cols { 0 } else { i64::from(cell.row) - drag.grab.0 };
        let col = if rows { 0 } else { i64::from(cell.col) - drag.grab.1 };
        Some(Target { to: place(row, col), copy, insert: None })
    }

    /// Drops the block: straight in, or after asking when it would cover cells that hold something.
    fn drop_block(&mut self, from: RangeRef, target: Target) -> Result<(), String> {
        if target.to == from.start && target.insert.is_none() {
            return Ok(());
        }
        let params = json!({
            "from": from.a1(),
            "to": target.to.a1(),
            "copy": target.copy,
            "insert": target.insert.map(|down| if down { "down" } else { "right" }),
        });
        if target.insert.is_none() && self.covers_data(from, target) {
            self.dialog_asked = (self.dialog_asked.0 + 1, Some(("moveCells".into(), params, "edit.moveCells".into())));
            self.edits += 1;
            return Ok(());
        }
        self.session.execute("edit.moveCells", params).map(|_| ()).map_err(engine_error)
    }

    /// Whether the cells a block would land on hold something (outside the block itself, for a move).
    fn covers_data(&self, from: RangeRef, target: Target) -> bool {
        let Some((_, sheet)) = self.sheet() else { return false };
        let end = target.to.offset_clamped(i64::from(from.height()) - 1, i64::from(from.width()) - 1);
        let dest = RangeRef::new(target.to, end);
        sheet.cells.iter_range(dest).any(|(c, cell)| (target.copy || !from.contains(c)) && (!cell.value.is_empty() || cell.formula.is_some()))
    }

    /// The cursor over the selection's border and while it is dragged.
    pub(crate) fn move_cursor(&self) -> Option<&'static str> {
        let moving = matches!(self.drag, Drag::Move(_)).then_some(false);
        match self.move_hover.or(moving) {
            Some(true) => Some("copy"),
            Some(false) => Some("move"),
            None => None,
        }
    }

    /// Where the dragged block would land: a dashed frame, or with Shift a thick line where it
    /// goes in between the cells.
    pub(crate) fn move_overlays(&self, out: &mut Vec<ToolOverlay>) {
        let Drag::Move(drag) = &self.drag else { return };
        let Some(target) = drag.target else { return };
        let from = drag.from;
        let end = target.to.offset_clamped(i64::from(from.height()) - 1, i64::from(from.width()) - 1);
        let Some((x0, y0, x1, y1)) = self.movable_rect(RangeRef::new(target.to, end)) else { return };
        let d = match target.insert {
            Some(true) => format!("M{x0:.1},{y0:.1} L{x1:.1},{y0:.1}"),
            Some(false) => format!("M{x0:.1},{y0:.1} L{x0:.1},{y1:.1}"),
            None => format!("M{x0:.1},{y0:.1} L{x1:.1},{y0:.1} L{x1:.1},{y1:.1} L{x0:.1},{y1:.1} Z"),
        };
        let width = if target.insert.is_some() { 3.0 } else { 2.0 };
        out.push(ToolOverlay::Path { d, color: MOVE_FRAME, width, dashed: target.insert.is_none() });
    }

    /// The range the dragged block would land on (the status bar shows it), if one is dragged.
    pub(crate) fn move_status(&self) -> Option<String> {
        let Drag::Move(drag) = &self.drag else { return None };
        let target = drag.target?;
        let end = target.to.offset_clamped(i64::from(drag.from.height()) - 1, i64::from(drag.from.width()) - 1);
        let range = RangeRef::new(target.to, end);
        Some(if drag.from.is_full_rows() {
            format!("{}:{}", range.start.row + 1, range.end.row + 1)
        } else if drag.from.is_full_cols() {
            RangeRef::cols(range.start.col, range.end.col).a1()
        } else {
            range.a1()
        })
    }

    /// A press on a filter button (a filter's or a table's header): the editor's filter list for
    /// that column opens (`dialog` `filterMenu`), with the button's rectangle to show it under.
    pub(crate) fn filter_button_press(&mut self, x: f64, y: f64) -> bool {
        let Some((_, sheet)) = self.sheet() else { return false };
        let Hit::Cell(cell) = self.hit(x, y) else { return false };
        if filter_button(sheet, cell).is_none() {
            return false;
        }
        let area = sheet.merge_at(cell).unwrap_or(RangeRef::cell(cell));
        let (cx0, cy0, cx1, cy1) = Self::cell_rect(sheet, area);
        let side = 15.0f64.min(cy1 - cy0 - 2.0).max(4.0).round();
        let (bx1, by1) = (cx1 - 2.0, cy1 - 2.0);
        let (bx0, by0) = (bx1 - side, (by1 - side).max(cy0 + 1.0));
        if x < bx0 || x > bx1 + 1.0 || y < by0 - 1.0 || y > by1 + 1.0 {
            return false;
        }
        if self.commit().is_err() || self.editing.is_some() {
            return true;
        }
        let params = json!({ "cell": cell.a1(), "rect": [bx0, by0, bx1, by1], "cellRect": [cx0, cy0, cx1, cy1] });
        self.dialog_asked = (self.dialog_asked.0 + 1, Some(("filterMenu".into(), params, "data.filterBy".into())));
        self.edits += 1;
        true
    }
}

/// The frame of a dragged block's landing place: the grid's dark grey.
const MOVE_FRAME: [u8; 4] = [64, 64, 64, 255];
