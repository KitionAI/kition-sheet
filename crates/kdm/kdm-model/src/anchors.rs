//! Comment anchors (Kit UI protocol 2.6 section 7k): a sheet's cell range named by objects, so a
//! host that keeps comment threads itself (Kition One) finds the range again after rows or columns
//! are inserted or deleted before it, and after its sheet is renamed or moved.
//!
//! An anchor's ids are the sheet's object, then the range's first row and column and, for more
//! than one cell, its last row and column: `[sheet, row, col]` or `[sheet, top, left, bottom,
//! right]`. A line is named by its `Row` or `Col` object (see `lines`); a line below the list's end
//! (an empty cell under or right of everything the sheet holds) by the last live line's object and
//! how far past it the line is, `<object>+<n>`, or `#n` when the list is empty; a whole axis
//! (selected columns or rows) by `*` at both ends.
//!
//! Found again, a range's deleted first or last line moves inwards to the nearest live one (the
//! range shrinks, Excel's rule for ranges) and a range with nothing left is gone; so is a single
//! cell whose row or column was deleted, as an Excel comment goes with its cell. Lines below the
//! list's end have no objects: an insert between the list's end and such a line does not move
//! the anchor (`<object>+<n>` keeps its distance from its object).

use kition_kit::kdm::{ObjectId, Replica};
use kition_sheet_core::{CellRef, RangeRef};

use crate::Ids;
use crate::lines::{Axis, LineAt, Lines, span};
use crate::read::sheet_lines;

/// The token of line `index` of `lines`.
fn token(lines: &Lines, index: u32) -> String {
    match lines.live.get(index as usize) {
        Some(object) => object.as_str().to_owned(),
        None => match lines.live.last() {
            // `index` is at or past the list's end, so the distance is at least 1.
            Some(last) => format!("{}+{}", last.as_str(), (index as usize).saturating_add(1).saturating_sub(lines.live.len())),
            None => format!("#{index}"),
        },
    }
}

/// Where a token points now.
fn at(lines: &Lines, token: &str) -> LineAt {
    let found = lines.at(token);
    if found != LineAt::Missing {
        return found;
    }
    let Some((base, n)) = token.rsplit_once('+') else { return LineAt::Missing };
    let Some(n) = n.parse::<u32>().ok().filter(|n| *n > 0) else { return LineAt::Missing };
    match lines.at(base) {
        LineAt::Live(i) => i.checked_add(n).map_or(LineAt::Missing, LineAt::Index),
        // The base line was deleted: the lines past it moved up by one.
        LineAt::Gone(next) => next.checked_add(n - 1).map_or(LineAt::Missing, LineAt::Index),
        _ => LineAt::Missing,
    }
}

/// One axis of a range: its two tokens (`*` for the whole axis).
fn tokens(lines: &Lines, axis: Axis, lo: u32, hi: u32) -> (String, String) {
    if lo == 0 && hi >= axis.max().saturating_sub(1) {
        return ("*".to_owned(), "*".to_owned());
    }
    (token(lines, lo), token(lines, hi))
}

/// One axis back: its first and last line now; `None` when nothing is left of it.
fn resolve(lines: &Lines, axis: Axis, a: &str, b: &str) -> Option<(u32, u32)> {
    match (a, b) {
        ("*", "*") => Some((0, axis.max().saturating_sub(1))),
        ("*", _) | (_, "*") => None,
        _ => {
            let (lo, hi) = span(at(lines, a), at(lines, b), axis.max())?;
            Some((lo.min(hi), lo.max(hi)))
        }
    }
}

/// The anchor ids of `range` on sheet `sheet` (its index in `ids`); `None` when the sheet has no
/// object or its lines cannot be read.
pub fn comment_anchor(replica: &Replica, ids: &Ids, sheet: usize, range: RangeRef) -> Option<Vec<String>> {
    let object = &ids.sheets.get(sheet)?.object;
    let rows = sheet_lines(replica, object, Axis::Rows).ok()?;
    let cols = sheet_lines(replica, object, Axis::Cols).ok()?;
    let (top, bottom) = tokens(&rows, Axis::Rows, range.start.row, range.end.row);
    let (left, right) = tokens(&cols, Axis::Cols, range.start.col, range.end.col);
    let mut anchor = vec![object.as_str().to_owned(), top, left];
    if !range.is_single() {
        anchor.extend([bottom, right]);
    }
    Some(anchor)
}

/// Where the anchor `anchor` ([`comment_anchor`]) is now: its sheet's index in `ids` and its
/// range. `None` when its sheet is gone or nothing is left of its range.
pub fn find_comment_anchor(replica: &Replica, ids: &Ids, anchor: &[String]) -> Option<(usize, RangeRef)> {
    let (sheet, top, left, bottom, right) = match anchor {
        [sheet, row, col] => (sheet, row, col, row, col),
        [sheet, top, left, bottom, right] => (sheet, top, left, bottom, right),
        _ => return None,
    };
    let object = ObjectId::from_string(sheet.as_str());
    let index = ids.sheet_of(&object)?;
    let rows = sheet_lines(replica, &object, Axis::Rows).ok()?;
    let cols = sheet_lines(replica, &object, Axis::Cols).ok()?;
    let (r0, r1) = resolve(&rows, Axis::Rows, top, bottom)?;
    let (c0, c1) = resolve(&cols, Axis::Cols, left, right)?;
    Some((index, RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_sheet_core::{MAX_COLS, MAX_ROWS};

    fn lines(spec: &[(&str, bool)]) -> Lines {
        Lines::new(spec.iter().map(|(id, gone)| (ObjectId::from_string(*id), *gone)).collect())
    }

    #[test]
    fn a_line_is_its_object_and_one_past_the_list_counts_from_the_last_live_line() {
        let l = lines(&[("a", false), ("b", true), ("c", false)]);
        assert_eq!(token(&l, 0), "a");
        assert_eq!(token(&l, 1), "c");
        assert_eq!(token(&l, 2), "c+1");
        assert_eq!(token(&l, 9), "c+8");
        assert_eq!(token(&lines(&[]), 4), "#4");
    }

    #[test]
    fn tokens_follow_their_objects_and_lines_past_the_list_follow_the_last_one() {
        let l = lines(&[("a", false), ("c", false)]);
        let (a, past) = (token(&l, 0), token(&l, 4));
        // Two lines inserted before `a`; `c` deleted.
        let moved = lines(&[("n1", false), ("n2", false), ("a", false), ("c", true)]);
        assert_eq!(at(&moved, &a), LineAt::Live(2));
        assert_eq!(at(&moved, &past), LineAt::Index(5), "4 - 1 deleted + 2 inserted");
        assert_eq!(at(&moved, "zz+1"), LineAt::Missing);
        assert_eq!(at(&moved, "a+0"), LineAt::Missing);
        assert_eq!(at(&moved, "a+x"), LineAt::Missing);
        assert_eq!(at(&moved, "#3"), LineAt::Index(3));
    }

    #[test]
    fn spans_shrink_to_their_live_lines_and_whole_axes_stay_whole() {
        let l = lines(&[("a", false), ("b", true), ("c", false), ("d", false), ("e", true)]);
        assert_eq!(resolve(&l, Axis::Rows, "b", "d"), Some((1, 2)));
        assert_eq!(resolve(&l, Axis::Rows, "a", "e"), Some((0, 2)));
        assert_eq!(resolve(&l, Axis::Rows, "b", "b"), None, "a single deleted line is gone");
        assert_eq!(resolve(&l, Axis::Rows, "*", "*"), Some((0, MAX_ROWS - 1)));
        assert_eq!(resolve(&l, Axis::Cols, "*", "*"), Some((0, MAX_COLS - 1)));
        assert_eq!(resolve(&l, Axis::Rows, "*", "a"), None);
        assert_eq!(resolve(&l, Axis::Rows, "a", "missing"), None);
        assert_eq!(tokens(&l, Axis::Rows, 0, MAX_ROWS - 1), ("*".to_owned(), "*".to_owned()));
    }
}
