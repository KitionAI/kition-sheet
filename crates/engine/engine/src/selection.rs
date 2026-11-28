//! Cell selection: one or more ranges with an active cell.

use kition_sheet_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use kition_sheet_model::Sheet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// The active (white) cell.
    pub active: CellRef,
    /// Where a Shift-extension is anchored.
    pub anchor: CellRef,
    /// Selected areas; the last one contains the active cell. Never empty.
    pub ranges: Vec<RangeRef>,
}

impl Default for Selection {
    fn default() -> Self {
        Selection::at(CellRef::default())
    }
}

impl Selection {
    pub fn at(c: CellRef) -> Selection {
        Selection { active: c, anchor: c, ranges: vec![RangeRef::cell(c)] }
    }
    pub fn range(r: RangeRef) -> Selection {
        Selection { active: r.start, anchor: r.start, ranges: vec![r] }
    }
    /// The current (last) area.
    pub fn current(&self) -> RangeRef {
        self.ranges.last().copied().unwrap_or(RangeRef::cell(self.active))
    }
    /// Bounding range of all areas.
    pub fn bounds(&self) -> RangeRef {
        let mut it = self.ranges.iter();
        let first = it.next().copied().unwrap_or(RangeRef::cell(self.active));
        it.fold(first, |a, b| a.union(b))
    }
    pub fn contains(&self, c: CellRef) -> bool {
        self.ranges.iter().any(|r| r.contains(c))
    }
    pub fn is_single_cell(&self) -> bool {
        self.ranges.len() == 1 && self.current().is_single()
    }
    /// Extends the current area from the anchor to `to` (Shift+click / Shift+arrows).
    pub fn extend_to(&mut self, to: CellRef) {
        let r = RangeRef::new(self.anchor, to);
        if let Some(last) = self.ranges.last_mut() {
            *last = r;
        } else {
            self.ranges.push(r);
        }
    }
    /// The A1 text of all areas (`A1:B2,D4`).
    pub fn a1(&self) -> String {
        self.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(",")
    }
    /// Expands areas so merged cells are included whole. Whole rows or columns (a heading's) stay
    /// as they are, as in WPS: deleting row 1 under a merge of rows 1 and 2 deletes row 1 only.
    pub fn expand_merges(&mut self, sheet: &Sheet) {
        for r in self.ranges.iter_mut().filter(|r| !r.is_full_rows() && !r.is_full_cols()) {
            let mut cur = *r;
            loop {
                let mut grown = cur;
                for m in &sheet.merges {
                    if m.intersects(&grown) {
                        grown = grown.union(m);
                    }
                }
                if grown == cur {
                    break;
                }
                cur = grown;
            }
            *r = cur;
        }
    }
    /// Moves the active cell within the selection (Enter/Tab inside a multi-cell selection), the
    /// selection unchanged: Enter goes down each column first (`by_row` false), Tab along each row
    /// first, Shift backwards. Past an area's last cell it goes on in the next area (Excel), from the
    /// last back to the first. Hidden rows and columns and the cells a merge covers are passed over
    /// (`sheet`); a merge is visited at its top-left cell.
    pub fn cycle(&mut self, sheet: Option<&Sheet>, forward: bool, by_row: bool) {
        if self.is_single_cell() {
            return;
        }
        let count = self.ranges.len();
        let Some(mut index) = self.ranges.iter().rposition(|r| r.contains(self.active)) else {
            self.active = self.current().start;
            return;
        };
        let passed =
            |c: CellRef| sheet.is_some_and(|s| s.is_row_hidden(c.row) || s.is_col_hidden(c.col) || s.merge_at(c).is_some_and(|m| m.start != c));
        let mut at = self.active;
        // Bounded: a whole column of hidden rows is not walked to its end.
        for _ in 0..100_000 {
            let Some(area) = self.ranges.get(index).copied() else { break };
            match step_within(area, at, forward, by_row) {
                Some(next) => at = next,
                None => {
                    index = if forward { (index + 1) % count } else { (index + count - 1) % count };
                    let Some(next) = self.ranges.get(index) else { break };
                    at = if forward { next.start } else { next.end };
                }
            }
            if !passed(at) {
                break;
            }
        }
        self.active = at;
    }
}

/// The next cell of `area` after `at` in reading order (along rows when `by_row`, else down the
/// columns), backwards unless `forward`; `None` past its last (first) cell.
fn step_within(area: RangeRef, at: CellRef, forward: bool, by_row: bool) -> Option<CellRef> {
    let (start, end) = (area.start, area.end);
    // The axis walked first and the one it wraps onto.
    let (inner, inner_lo, inner_hi, outer, outer_lo, outer_hi) =
        if by_row { (at.col, start.col, end.col, at.row, start.row, end.row) } else { (at.row, start.row, end.row, at.col, start.col, end.col) };
    let (inner, outer) = if forward {
        if inner < inner_hi {
            (inner + 1, outer)
        } else if outer < outer_hi {
            (inner_lo, outer + 1)
        } else {
            return None;
        }
    } else if inner > inner_lo {
        (inner - 1, outer)
    } else if outer > outer_lo {
        (inner_hi, outer - 1)
    } else {
        return None;
    };
    Some(if by_row { CellRef::new(outer, inner) } else { CellRef::new(inner, outer) })
}

impl Selection {
    /// Where Home, Cmd+Home and Cmd+End go from `from` (`select.move {to}`; WPS W-B9, W-B11): the
    /// row's first column, the sheet's first cell, or the used range's bottom-right cell. With
    /// frozen panes the first cell is the scrolling pane's top-left (Excel); rows and columns the
    /// panes leave out of reach, and hidden ones, are passed over.
    pub fn place(sheet: &Sheet, from: CellRef, to: Place) -> CellRef {
        let (top, left) = match sheet.freeze {
            Some((rows, cols)) => (if rows > 0 { sheet.view_top_left.row } else { 0 }, if cols > 0 { sheet.view_top_left.col } else { 0 }),
            None => (0, 0),
        };
        let (rows, cols) = sheet.freeze.unwrap_or((0, 0));
        let visible_col = |mut c: u32| {
            let mut guard = 0;
            while sheet.is_col_hidden(c) && c + 1 < MAX_COLS && guard < 16_384 {
                c += 1;
                guard += 1;
            }
            c
        };
        let visible_row = |mut r: u32| {
            let mut guard = 0;
            while sheet.is_row_hidden(r) && r + 1 < MAX_ROWS && guard < 100_000 {
                r += 1;
                guard += 1;
            }
            r
        };
        match to {
            Place::RowStart => CellRef::new(from.row, visible_col(left)),
            Place::SheetStart => {
                CellRef::new(visible_row(top.saturating_add(rows).min(MAX_ROWS - 1)), visible_col(left.saturating_add(cols).min(MAX_COLS - 1)))
            }
            Place::LastCell => sheet.used_range().map_or(CellRef::new(visible_row(top), visible_col(left)), |used| used.end),
        }
    }
}

/// Where `select.move {to}` goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Home: the row's first column.
    RowStart,
    /// Cmd+Home (Ctrl+Home): the sheet's first cell.
    SheetStart,
    /// Cmd+End (Ctrl+End): the used range's bottom-right cell.
    LastCell,
}

impl Place {
    pub fn parse(name: &str) -> Option<Place> {
        match name {
            "rowStart" => Some(Place::RowStart),
            "sheetStart" => Some(Place::SheetStart),
            "lastCell" => Some(Place::LastCell),
            _ => None,
        }
    }
}

/// Ctrl+arrow: jump to the edge of the current data region (Excel's End mode).
pub fn jump(sheet: &Sheet, from: CellRef, dr: i64, dc: i64) -> CellRef {
    if dr == 0 && dc == 0 {
        // No direction: nowhere to go (the search below would never advance).
        return from;
    }
    let filled = |c: CellRef| sheet.value_ref(c).is_some_and(|v| !v.is_empty());
    let Some(mut cur) = from.offset(dr, dc) else { return from };
    let here = filled(from);
    let next = filled(cur);
    if here && next {
        // Move to the last filled cell of the run.
        while let Some(n) = cur.offset(dr, dc) {
            if !filled(n) {
                break;
            }
            cur = n;
        }
        return cur;
    }
    // Move to the next filled cell, or the sheet edge. Use the used range to avoid walking a
    // million empty cells.
    let used = sheet.used_range();
    loop {
        if filled(cur) {
            return cur;
        }
        let beyond = match used {
            Some(u) => {
                (dr > 0 && cur.row > u.end.row)
                    || (dc > 0 && cur.col > u.end.col)
                    || (dr < 0 && cur.row < u.start.row)
                    || (dc < 0 && cur.col < u.start.col)
            }
            None => true,
        };
        if beyond {
            let r = if dr > 0 {
                MAX_ROWS - 1
            } else if dr < 0 {
                0
            } else {
                cur.row
            };
            let c = if dc > 0 {
                MAX_COLS - 1
            } else if dc < 0 {
                0
            } else {
                cur.col
            };
            return CellRef::new(r, c);
        }
        match cur.offset(dr, dc) {
            Some(n) => cur = n,
            None => return cur,
        }
    }
}

/// The contiguous data region around `c` (Ctrl+A / Ctrl+Shift+8, AutoSum, sort, tables).
pub fn current_region(sheet: &Sheet, c: CellRef) -> RangeRef {
    let filled = |c: CellRef| sheet.cell(c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some()) || sheet.spill.contains_key(&c);
    // Whether a row (or column) has a filled cell between two columns (rows), inclusive.
    let row_has = |row: u32, a: u32, b: u32| (a..=b).any(|col| filled(CellRef::new(row, col)));
    let col_has = |col: u32, a: u32, b: u32| (a..=b).any(|row| filled(CellRef::new(row, col)));
    let (mut r0, mut r1, mut c0, mut c1) = (c.row, c.row, c.col, c.col);
    // Grow one side at a time while the line next to it (diagonals included) has data. Each
    // pass scans only the lines next to the region, so tall regions take linear time.
    loop {
        let before = (r0, r1, c0, c1);
        let (lo_c, hi_c) = (c0.saturating_sub(1), (c1 + 1).min(MAX_COLS - 1));
        while r0 > 0 && row_has(r0 - 1, lo_c, hi_c) {
            r0 -= 1;
        }
        while r1 + 1 < MAX_ROWS && row_has(r1 + 1, lo_c, hi_c) {
            r1 += 1;
        }
        let (lo_r, hi_r) = (r0.saturating_sub(1), (r1 + 1).min(MAX_ROWS - 1));
        while c0 > 0 && col_has(c0 - 1, lo_r, hi_r) {
            c0 -= 1;
        }
        while c1 + 1 < MAX_COLS && col_has(c1 + 1, lo_r, hi_r) {
            c1 += 1;
        }
        if (r0, r1, c0, c1) == before {
            break;
        }
    }
    RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_sheet_core::Value;

    #[test]
    fn jumps() {
        let mut s = Sheet::new("S");
        for r in 0..5 {
            s.set_value(CellRef::new(r, 0), Value::Number(1.0));
        }
        s.set_value(CellRef::new(10, 0), Value::Number(1.0));
        assert_eq!(jump(&s, CellRef::new(0, 0), 1, 0), CellRef::new(4, 0));
        // Without a direction it stays put (it used to loop forever from a blank cell).
        assert_eq!(jump(&s, CellRef::new(7, 0), 0, 0), CellRef::new(7, 0));
        assert_eq!(jump(&s, CellRef::new(4, 0), 1, 0), CellRef::new(10, 0));
        assert_eq!(jump(&s, CellRef::new(10, 0), 1, 0), CellRef::new(MAX_ROWS - 1, 0));
        assert_eq!(jump(&s, CellRef::new(0, 0), 0, 1), CellRef::new(0, MAX_COLS - 1));
        assert_eq!(jump(&s, CellRef::new(0, 0), -1, 0), CellRef::new(0, 0));
    }

    #[test]
    fn regions() {
        let mut s = Sheet::new("S");
        for r in 2..5 {
            for c in 1..4 {
                s.set_value(CellRef::new(r, c), Value::Number(1.0));
            }
        }
        assert_eq!(current_region(&s, CellRef::new(3, 2)).a1(), "B3:D5");
        assert_eq!(current_region(&s, CellRef::new(5, 4)).a1(), "B3:E6");
        assert_eq!(current_region(&s, CellRef::new(20, 20)).a1(), "U21");
        // Taller than 10,000 rows: it used to stop growing after 10,000 steps.
        let mut s = Sheet::new("S");
        for r in 0..20_000 {
            s.set_value(CellRef::new(r, 0), Value::Number(r as f64));
            s.set_value(CellRef::new(r, 1), Value::Number(1.0));
        }
        assert_eq!(current_region(&s, CellRef::new(5, 0)).a1(), "A1:B20000");
    }

    #[test]
    fn cycling() {
        let mut sel = Selection::range(RangeRef::parse("A1:B2").unwrap());
        sel.cycle(None, true, false);
        assert_eq!(sel.active.a1(), "A2");
        sel.cycle(None, true, false);
        assert_eq!(sel.active.a1(), "B1");
        sel.cycle(None, true, true);
        assert_eq!(sel.active.a1(), "A2");
    }

    /// Enter and Tab in a selection (WPS W-B4): down each column first, along each row first,
    /// Shift backwards, the selection unchanged; areas one after another; merges at their
    /// top-left; hidden lines passed over.
    #[test]
    fn cycling_through_areas_merges_and_hidden_lines() {
        let order = |sel: &mut Selection, sheet: Option<&Sheet>, forward: bool, by_row: bool, n: usize| {
            (0..n)
                .map(|_| {
                    sel.cycle(sheet, forward, by_row);
                    sel.active.a1()
                })
                .collect::<Vec<_>>()
        };
        let mut sel = Selection::range(RangeRef::parse("B2:C4").unwrap());
        assert_eq!(order(&mut sel, None, true, false, 6), ["B3", "B4", "C2", "C3", "C4", "B2"]);
        assert_eq!(order(&mut sel, None, true, true, 6), ["C2", "B3", "C3", "B4", "C4", "B2"]);
        assert_eq!(order(&mut sel, None, false, true, 2), ["C4", "B4"]);
        assert_eq!(sel.a1(), "B2:C4");
        let mut sel = Selection {
            active: CellRef::parse("A1").unwrap(),
            anchor: CellRef::parse("A1").unwrap(),
            ranges: vec![RangeRef::parse("A1:A2").unwrap(), RangeRef::parse("C3").unwrap(), RangeRef::parse("E1:E2").unwrap()],
        };
        assert_eq!(order(&mut sel, None, true, false, 6), ["A2", "C3", "E1", "E2", "A1", "A2"]);
        assert_eq!(order(&mut sel, None, false, false, 2), ["A1", "E2"]);
        let mut sheet = Sheet::new("S");
        sheet.merges.push(RangeRef::parse("B3:C3").unwrap());
        sheet.rows.entry(3).or_default().hidden = true;
        let mut sel = Selection::range(RangeRef::parse("B2:C5").unwrap());
        assert_eq!(order(&mut sel, Some(&sheet), true, true, 5), ["C2", "B3", "B5", "C5", "B2"]);
    }

    #[test]
    fn home_and_end_places() {
        let mut sheet = Sheet::new("S");
        sheet.set_value(CellRef::parse("J61").unwrap(), Value::Number(1.0));
        sheet.set_value(CellRef::parse("C3").unwrap(), Value::Number(1.0));
        let g7 = CellRef::parse("G7").unwrap();
        assert_eq!(Selection::place(&sheet, g7, Place::RowStart).a1(), "A7");
        assert_eq!(Selection::place(&sheet, g7, Place::SheetStart).a1(), "A1");
        assert_eq!(Selection::place(&sheet, g7, Place::LastCell).a1(), "J61");
        // Frozen panes: the scrolling pane's top-left; a hidden first column is passed over.
        sheet.freeze = Some((1, 0));
        assert_eq!(Selection::place(&sheet, g7, Place::SheetStart).a1(), "A2");
        sheet.cols.entry(0).or_default().hidden = true;
        assert_eq!(Selection::place(&sheet, g7, Place::RowStart).a1(), "B7");
        assert_eq!(Place::parse("rowStart"), Some(Place::RowStart));
        assert_eq!(Place::parse("end"), None);
        assert_eq!(Selection::place(&Sheet::new("E"), g7, Place::LastCell).a1(), "A1");
    }
}
