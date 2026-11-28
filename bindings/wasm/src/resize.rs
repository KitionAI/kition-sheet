//! Column widths and row heights by the headings, as in Excel: the pointer over the line after a
//! column's heading (below a row's) shows the resize cursor, a drag there moves a guide line and
//! sets the new size when it ends (on every selected whole column or row the line belongs to),
//! and a double-click fits the column or row to its contents.

use super::*;

/// How near the line between two headings a pointer resizes, in screen pixels.
const EDGE_SLOP: f64 = 4.0;
/// Excel's largest column width (255 characters) and row height (409 points), in pixels.
const MAX_WIDTH: f64 = 1800.0;
const MAX_HEIGHT: f64 = 546.0;

/// A resize under way.
pub(crate) struct Resize {
    rows: bool,
    line: u32,
    /// Where the line starts (its left or top edge), in document pixels.
    start: f64,
    /// Its size before and during the drag.
    from: f64,
    size: f64,
}

impl SheetDocument {
    /// The column (else row) whose far edge is under (`x`, `y`) in the headings: the line a drag
    /// there resizes.
    pub(crate) fn heading_edge(&self, x: f64, y: f64, zoom: f64) -> Option<(bool, u32)> {
        let (_, sheet) = self.sheet()?;
        let (cols, rows) = self.extent();
        let (head_w, head_h) = (header_w(sheet), header_h(sheet));
        let slop = EDGE_SLOP / zoom.max(0.05);
        let near = |at: f64, count: u32, start: &dyn Fn(u32) -> f64, size: &dyn Fn(u32) -> f64, find: &dyn Fn(f64) -> u32| {
            let line = find(at.max(0.0)).min(count.saturating_sub(1));
            let begin = start(line);
            if begin + size(line) - at <= slop {
                return Some(line);
            }
            // Just past the line before: that line's edge, skipping the hidden ones it hides.
            (at - begin <= slop).then(|| (0..line).rev().find(|l| size(*l) > 0.0)).flatten()
        };
        if y >= col_gutter(sheet) && y < head_h && x >= head_w {
            let line = near(x - head_w, cols, &|c| view::left(sheet, c), &|c| view::col_w(sheet, c) as f64, &|at| view::col_at(sheet, at));
            return line.map(|line| (false, line));
        }
        if x >= row_gutter(sheet) && x < head_w && y >= head_h {
            let line = near(y - head_h, rows, &|r| view::top(sheet, r), &|r| view::row_h(sheet, r) as f64, &|at| view::row_at(sheet, at));
            return line.map(|line| (true, line));
        }
        None
    }

    /// A press (or a double-click) on the line after a column's heading (below a row's) begins a
    /// resize (fits the column or row); hovering there shows the resize cursor. Answers whether
    /// the event was the headings' line's.
    pub(crate) fn resize_press(&mut self, event: &ToolPointer) -> Result<bool, String> {
        let Some((rows, line)) = self.heading_edge(event.x, event.y, event.zoom) else {
            self.edge_hover = None;
            return Ok(false);
        };
        self.edge_hover = Some(rows);
        match event.action {
            PointerAction::Down => {
                self.commit()?;
                let Some((_, sheet)) = self.sheet() else { return Ok(false) };
                let (start, size) = if rows {
                    (header_h(sheet) + view::top(sheet, line), view::row_h(sheet, line) as f64)
                } else {
                    (header_w(sheet) + view::left(sheet, line), view::col_w(sheet, line) as f64)
                };
                self.drag = Drag::Resize(Resize { rows, line, start, from: size, size });
                Ok(true)
            }
            PointerAction::DoubleClick => {
                let lines = self.resize_lines(rows, line);
                let (command, params) =
                    if rows { ("row.autofitHeight", json!({ "rows": lines })) } else { ("column.autofitWidth", json!({ "cols": lines })) };
                self.session.execute(command, params).map_err(engine_error)?;
                Ok(true)
            }
            // Hovering shows the cursor; the rest is the grid's.
            _ => Ok(false),
        }
    }

    /// The guide line follows the pointer.
    pub(crate) fn resize_drag_to(&mut self, event: &ToolPointer) {
        if let Drag::Resize(resize) = &mut self.drag {
            let at = if resize.rows { event.y } else { event.x };
            resize.size = (at - resize.start).clamp(0.0, if resize.rows { MAX_HEIGHT } else { MAX_WIDTH }).round();
        }
    }

    /// The release sets the new size, on every selected whole column or row the line belongs to.
    pub(crate) fn resize_release(&mut self, resize: Resize) -> Result<(), String> {
        if (resize.size - resize.from).abs() < 1.0 {
            return Ok(());
        }
        let lines = self.resize_lines(resize.rows, resize.line);
        let (command, params) = if resize.rows {
            ("row.setHeight", json!({ "rows": lines, "height": resize.size }))
        } else {
            ("column.setWidth", json!({ "cols": lines, "width": resize.size }))
        };
        self.session.execute(command, params).map(|_| ()).map_err(engine_error)
    }

    /// The columns (else rows) a resize of `line` sets: the selected whole ones when `line` is
    /// one of them, as in Excel, else `line` alone (`"C:E,G"`, `"3:3"`).
    fn resize_lines(&self, rows: bool, line: u32) -> String {
        let whole = |r: &RangeRef| if rows { r.is_full_rows() } else { r.is_full_cols() };
        let span = |r: &RangeRef| if rows { (r.start.row, r.end.row) } else { (r.start.col, r.end.col) };
        let selected: Vec<(u32, u32)> =
            self.session.active().map(|st| st.selection.ranges.iter().filter(|r| whole(r)).map(span).collect()).unwrap_or_default();
        let spans = if selected.iter().any(|(a, b)| (*a..=*b).contains(&line)) { selected } else { vec![(line, line)] };
        let name = |n: u32| if rows { (n as u64 + 1).to_string() } else { col_to_letters(n) };
        spans.iter().map(|(a, b)| format!("{}:{}", name(*a), name(*b))).collect::<Vec<_>>().join(",")
    }

    /// The cursor over the headings' lines and during a resize.
    pub(crate) fn resize_cursor(&self) -> Option<&'static str> {
        let resizing = match &self.drag {
            Drag::Resize(resize) => Some(resize.rows),
            _ => None,
        };
        match resizing.or(self.edge_hover) {
            Some(true) => Some("ns-resize"),
            Some(false) => Some("ew-resize"),
            None => None,
        }
    }

    /// The guide line where the resized column or row will end, across the grid.
    pub(crate) fn resize_overlays(&self, out: &mut Vec<ToolOverlay>) {
        let (Drag::Resize(resize), Some((_, sheet))) = (&self.drag, self.sheet()) else { return };
        let (cols, rows) = self.extent();
        let at = resize.start + resize.size;
        let d = if resize.rows {
            format!("M0,{at:.1} L{:.1},{at:.1}", header_w(sheet) + view::left(sheet, cols))
        } else {
            format!("M{at:.1},0 L{at:.1},{:.1}", header_h(sheet) + view::top(sheet, rows))
        };
        out.push(ToolOverlay::Path { d, color: ACCENT, width: 1.0, dashed: true });
    }
}
