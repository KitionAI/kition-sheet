//! The fill handle, as WPS works it (W-E1–W-E7): a small square at the selection's bottom-right
//! corner. Over it the pointer is a thin cross; a drag from it shows a dashed frame round the
//! cells it would fill, along one direction only (the one the pointer has gone further in, down
//! when even), and the release fills them with one `edit.autoFill`: the series goes on (a single
//! number counts on, 5, 6, 7) or, with Cmd (Ctrl) held at the release, the source is copied (W-E2).
//! Dragged back into the selection, the cells passed over are shaded and the release clears them
//! (Excel's). A double-click fills down as far as the data in the column beside it goes (left
//! first, then right). Escape drops the drag. The workbook does not change until the release, one
//! undo step.

use super::*;

/// How far from the selection's corner the handle is grabbed, in screen pixels either way: 9 × 9.
const HANDLE_REACH: f64 = 4.5;
/// The handle's side, in screen pixels, and its white edge round it.
const HANDLE_SIZE: f32 = 6.0;
const HANDLE_EDGE: f32 = 8.0;
/// The frame of the cells a drag would fill: the grid's dark grey.
const FILL_FRAME: [u8; 4] = [64, 64, 64, 255];
/// The shade over cells a drag back into the selection would clear.
const CLEAR_SHADE: [u8; 4] = [96, 96, 96, 72];

/// A drag of the fill handle under way.
pub(crate) struct FillDrag {
    /// The selection the handle belongs to.
    source: RangeRef,
    /// What the release would do now.
    aim: Aim,
}

/// What a fill handle's release does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Aim {
    /// Nothing: the pointer is on the source's last cell.
    Stay,
    /// Fill this range (the source and the cells beyond it).
    Fill(RangeRef),
    /// Clear these cells (dragged back into the source).
    Clear(RangeRef),
}

impl SheetDocument {
    /// The range the fill handle extends: the selection's one area (a merged cell's whole area),
    /// unless something is being typed, pointed at or painted, or a drawing object is selected;
    /// none for whole rows or columns, or past the grid.
    pub(crate) fn fill_source(&self) -> Option<RangeRef> {
        if self.editing.is_some() || self.point.is_some() || self.session.format_painter.is_some() || self.session.selected_shape.is_some() {
            return None;
        }
        let st = self.session.active()?;
        if st.selection.ranges.len() != 1 {
            return None;
        }
        let range = st.selection.current();
        if range.is_full_rows() || range.is_full_cols() {
            return None;
        }
        let (_, sheet) = self.sheet()?;
        let range = if range.is_single() { sheet.merge_at(range.start).unwrap_or(range) } else { range };
        let (cols, rows) = self.extent();
        (range.end.row < rows && range.end.col < cols).then_some(range)
    }

    /// The handle's centre: `range`'s bottom-right corner, in document pixels.
    fn fill_corner(&self, range: RangeRef) -> Option<(f64, f64)> {
        let (_, sheet) = self.sheet()?;
        let (_, _, x1, y1) = Self::cell_rect(sheet, range);
        Some((x1, y1))
    }

    /// Whether (`x`, `y`) is on the fill handle: within 9 × 9 screen pixels round the corner, among
    /// the cells, and in the pane the handle is drawn in (with frozen panes, never across the
    /// freeze line).
    pub(crate) fn on_fill_handle(&self, x: f64, y: f64, zoom: f64) -> bool {
        let (Some(range), Some((_, sheet))) = (self.fill_source(), self.sheet()) else { return false };
        let Some((cx, cy)) = self.fill_corner(range) else { return false };
        let reach = HANDLE_REACH / zoom.max(0.05);
        if (x - cx).abs() > reach || (y - cy).abs() > reach || x < header_w(sheet) || y < header_h(sheet) {
            return false;
        }
        let (rows, cols) = sheet.freeze.unwrap_or((0, 0));
        let (split_row, split_col) = view::split(sheet);
        let frozen_y = header_h(sheet) + view::top(sheet, split_row);
        let frozen_x = header_w(sheet) + view::left(sheet, split_col);
        (rows == 0 || (range.end.row < split_row) == (y < frozen_y)) && (cols == 0 || (range.end.col < split_col) == (x < frozen_x))
    }

    /// The cell under (`x`, `y`), or the nearest cell when it is over a heading.
    pub(crate) fn cell_near(&self, x: f64, y: f64) -> CellRef {
        let Some((_, sheet)) = self.sheet() else { return CellRef::default() };
        match self.hit(x.max(header_w(sheet)), y.max(header_h(sheet))) {
            Hit::Cell(cell) => cell,
            _ => CellRef::default(),
        }
    }

    /// A press on the fill handle begins a fill drag; a double-click on it fills down beside the
    /// data next to it; hovering shows the thin cross. Answers whether the event was the handle's.
    pub(crate) fn fill_press(&mut self, event: &ToolPointer) -> Result<bool, String> {
        let on = self.on_fill_handle(event.x, event.y, event.zoom);
        self.fill_hover = on;
        let Some(source) = self.fill_source().filter(|_| on) else { return Ok(false) };
        match event.action {
            PointerAction::Down => {
                self.tab_run = None;
                self.drag = Drag::Fill(FillDrag { source, aim: Aim::Stay });
                Ok(true)
            }
            PointerAction::DoubleClick => {
                self.fill_beside(source)?;
                Ok(true)
            }
            // Hovering shows the cursor; the rest is the grid's.
            _ => Ok(false),
        }
    }

    /// The cells the drag would fill (or clear) follow the pointer.
    pub(crate) fn fill_drag_to(&mut self, event: &ToolPointer) {
        let cell = self.cell_near(event.x, event.y);
        if let Drag::Fill(drag) = &mut self.drag {
            drag.aim = fill_aim(drag.source, cell);
        }
    }

    /// The release fills (Cmd/Ctrl held: copies) or clears what the drag shows, in one command.
    pub(crate) fn fill_release(&mut self, drag: FillDrag, mods: ToolMods) -> Result<(), String> {
        // The handle moves to the filled block's corner.
        self.fill_hover = false;
        match drag.aim {
            Aim::Stay => Ok(()),
            Aim::Fill(target) => {
                let mode = if mods.command || mods.control { "copy" } else { "extend" };
                let params = json!({ "source": drag.source.a1(), "target": target.a1(), "mode": mode });
                self.session.execute("edit.autoFill", params).map(|_| ()).map_err(engine_error)
            }
            Aim::Clear(cleared) => {
                self.session.execute("edit.clearContents", json!({ "range": cleared.a1() })).map_err(engine_error)?;
                let kept = if cleared.start.row > drag.source.start.row {
                    RangeRef::new(drag.source.start, CellRef::new(cleared.start.row - 1, drag.source.end.col))
                } else {
                    RangeRef::new(drag.source.start, CellRef::new(drag.source.end.row, cleared.start.col.saturating_sub(1)))
                };
                self.select(kept, drag.source.start)
            }
        }
    }

    /// A double-click on the handle: the selection fills down to the last row of the data in the
    /// column left of it (or, with nothing there, right of it), from the row under it; nothing when
    /// the cell beside the first row below is empty.
    fn fill_beside(&mut self, source: RangeRef) -> Result<(), String> {
        let Some((_, sheet)) = self.sheet() else { return Ok(()) };
        let below = source.end.row.saturating_add(1);
        let last_row = sheet.used_range().map_or(0, |used| used.end.row);
        if below >= MAX_ROWS || below > last_row {
            return Ok(());
        }
        let holds = |row: u32, col: u32| sheet.cell(CellRef::new(row, col)).is_some_and(|c| !c.value.is_empty() || c.formula.is_some());
        let run_end = |col: u32| {
            if !holds(below, col) {
                return None;
            }
            let mut row = below;
            while row < last_row && holds(row + 1, col) {
                row += 1;
            }
            Some(row)
        };
        let left = source.start.col.checked_sub(1).and_then(run_end);
        let right = || source.end.col.checked_add(1).filter(|c| *c < MAX_COLS).and_then(run_end);
        let Some(end) = left.or_else(right) else { return Ok(()) };
        let target = RangeRef::new(source.start, CellRef::new(end, source.end.col));
        let params = json!({ "source": source.a1(), "target": target.a1(), "mode": "extend" });
        self.session.execute("edit.autoFill", params).map(|_| ()).map_err(engine_error)
    }

    /// The handle at the selection's corner, and during a drag the dashed frame round the cells it
    /// would fill (or the shade over those it would clear and the frame round what stays).
    pub(crate) fn fill_overlays(&self, out: &mut Vec<ToolOverlay>) {
        let Some((_, sheet)) = self.sheet() else { return };
        let frame = |range: RangeRef| {
            let (x0, y0, x1, y1) = Self::cell_rect(sheet, range);
            ToolOverlay::Path {
                d: format!("M{x0:.1},{y0:.1} L{x1:.1},{y0:.1} L{x1:.1},{y1:.1} L{x0:.1},{y1:.1} Z"),
                color: FILL_FRAME,
                width: 1.5,
                dashed: true,
            }
        };
        if let Drag::Fill(drag) = &self.drag {
            match drag.aim {
                Aim::Stay => {}
                Aim::Fill(target) => out.push(frame(target)),
                Aim::Clear(cleared) => {
                    let (x0, y0, x1, y1) = Self::cell_rect(sheet, cleared);
                    out.push(ToolOverlay::Highlight { quad: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], color: CLEAR_SHADE });
                    out.push(frame(drag.source));
                }
            }
        }
        if let Some((x, y)) = self.fill_source().and_then(|range| self.fill_corner(range)) {
            out.push(ToolOverlay::Anchor { x, y, color: WHITE, filled: true, size: HANDLE_EDGE });
            out.push(ToolOverlay::Anchor { x, y, color: ACCENT, filled: true, size: HANDLE_SIZE });
        }
    }
}

/// Where a fill handle dragged from `source` to `cell` aims: beyond the source along the one
/// direction it has gone further in (down or up when even), or back inside it to clear the rows
/// below (the columns right of) the pointer's.
pub(crate) fn fill_aim(source: RangeRef, cell: CellRef) -> Aim {
    let (row, col) = (i64::from(cell.row), i64::from(cell.col));
    let below = row - i64::from(source.end.row);
    let above = i64::from(source.start.row) - row;
    let right = col - i64::from(source.end.col);
    let left = i64::from(source.start.col) - col;
    let rows_out = below.max(above).max(0);
    let cols_out = right.max(left).max(0);
    if rows_out == 0 && cols_out == 0 {
        let rows_in = i64::from(source.end.row) - row;
        let cols_in = i64::from(source.end.col) - col;
        return if rows_in == 0 && cols_in == 0 {
            Aim::Stay
        } else if rows_in >= cols_in {
            Aim::Clear(RangeRef::new(CellRef::new(cell.row + 1, source.start.col), source.end))
        } else {
            Aim::Clear(RangeRef::new(CellRef::new(source.start.row, cell.col + 1), source.end))
        };
    }
    Aim::Fill(if rows_out >= cols_out {
        if below > 0 {
            RangeRef::new(source.start, CellRef::new(cell.row, source.end.col))
        } else {
            RangeRef::new(CellRef::new(cell.row, source.start.col), source.end)
        }
    } else if right > 0 {
        RangeRef::new(source.start, CellRef::new(source.end.row, cell.col))
    } else {
        RangeRef::new(CellRef::new(source.start.row, cell.col), source.end)
    })
}
