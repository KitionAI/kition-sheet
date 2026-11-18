//! Rows and columns as stable objects (G1, G3).
//!
//! **The sparse representation.** A sheet's `rows` (and `cols`) list holds one `Row` object per
//! row *from the top of the sheet down to the last row that has content* (a cell, row info, a
//! merge, a table…) or that a formula or feature names: the list is dense from index 0 and the
//! rows below its end are implicit (they exist, empty, with no object yet). A row's index is its
//! place in the list, counting live rows only, so:
//!
//! - inserting `n` rows at `i` is `n` new `Row` objects and `n` list inserts; no row below is
//!   rewritten, and every cell, formula and feature naming a row below still names the same row
//!   object, now further down;
//! - deleting rows marks their objects `gone` (a tombstone) and leaves them in the list. Their
//!   cells are dropped, a formula naming one of them reads `#REF!`, and a range whose first or
//!   last row is gone still knows where that row was, so it shrinks to the rows left (Excel's
//!   rule), even when the delete was concurrent;
//! - writing below the list's end appends the rows down to the one written ("padding"). Padding
//!   objects get ids derived from the sheet and their place in the list, so two people typing
//!   below the data at once create the *same* row objects, and the replica's list (read once per
//!   object, at its first place) lines their cells up again.
//!
//! The choice against the alternatives: a list of only the materialised rows with an `index`
//! register rewrites every row below on an insert, and a list with "gap" counts turns two
//! concurrent inserts into one gap count's last-writer-wins; both lose the property that a row
//! insert touches only the new rows. A dense list is O(rows in use) objects, so rows are only
//! materialised below [`Axis::limit`] (content) and [`Axis::ref_limit`] (references to empty rows):
//! anything further down (a cell at row 900,000, `A1:A1048576`'s end) is stored by index (`#n`),
//! which keeps its meaning until the next structural edit, when the engine rewrites it.

use std::collections::HashMap;

use kition_kit::kdm::ObjectId;
use kition_kit::session::sha256_hex;
use kition_sheet_core::{MAX_COLS, MAX_ROWS};

/// One axis of a sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Axis {
    Rows,
    Cols,
}

impl Axis {
    pub(crate) const BOTH: [Axis; 2] = [Axis::Rows, Axis::Cols];

    /// The sheet's list field.
    pub(crate) fn field(self) -> &'static str {
        match self {
            Axis::Rows => "rows",
            Axis::Cols => "cols",
        }
    }

    /// The object type of a line.
    pub(crate) fn ty(self) -> &'static str {
        match self {
            Axis::Rows => "Row",
            Axis::Cols => "Col",
        }
    }

    /// Lines in a sheet.
    pub(crate) fn max(self) -> u32 {
        match self {
            Axis::Rows => MAX_ROWS,
            Axis::Cols => MAX_COLS,
        }
    }

    /// Content (cells, line info, merges, tables) materialises lines above this index.
    pub(crate) fn limit(self) -> u32 {
        match self {
            Axis::Rows => 1 << 18,
            Axis::Cols => MAX_COLS,
        }
    }

    /// A reference to an empty line (in a formula or a feature) materialises lines above this.
    pub(crate) fn ref_limit(self) -> u32 {
        match self {
            Axis::Rows => 1 << 16,
            Axis::Cols => 1 << 10,
        }
    }

    pub(crate) fn of(axis: kition_sheet_formula::adjust::Axis) -> Axis {
        match axis {
            kition_sheet_formula::adjust::Axis::Rows => Axis::Rows,
            kition_sheet_formula::adjust::Axis::Cols => Axis::Cols,
        }
    }

    pub(crate) fn slot(self) -> usize {
        match self {
            Axis::Rows => 0,
            Axis::Cols => 1,
        }
    }
}

/// Why a line is materialised (how far down an index may grow the list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Use {
    /// Cells, line info, merges, tables, autofilters, array formula ranges.
    Content,
    /// Formula references and other features.
    Ref,
}

/// The id of the padding line at `raw` (its place in the replica's list, entries of every kind
/// counted) of `sheet`'s `axis`: replicas padding the same list from the same state make the
/// same objects.
pub(crate) fn pad_id(sheet: &ObjectId, axis: Axis, raw: usize) -> ObjectId {
    let hash = sha256_hex(format!("{}/{}/{raw}", sheet.as_str(), axis.field()).as_bytes());
    ObjectId::from_string(format!("p{}", hash.get(..32).unwrap_or(&hash)))
}

/// Where a stored line token points in a [`Lines`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineAt {
    /// A live line at this index.
    Live(u32),
    /// A deleted line; the next live line has this index (the lines below the list are implicit,
    /// so there always is one).
    Gone(u32),
    /// A line by index (`#n`).
    Index(u32),
    /// An object this sheet's list does not hold, or a malformed token.
    Missing,
}

/// One axis of a sheet as read: every line in list order, deleted ones included.
#[derive(Clone, Debug, Default)]
pub(crate) struct Lines {
    /// For each line in list order: whether it is gone, and the live lines before it.
    entries: Vec<(bool, u32)>,
    /// Each line's place in `entries`, by its id (looked up by token without an allocation).
    pos: HashMap<String, usize>,
    /// The live lines, in order: view index -> object.
    pub(crate) live: Vec<ObjectId>,
}

impl Lines {
    /// Lines from the list's objects (each read once, at its first place) and whether each is gone.
    pub(crate) fn new(order: Vec<(ObjectId, bool)>) -> Lines {
        let mut lines = Lines::default();
        for (object, gone) in order {
            if lines.pos.contains_key(object.as_str()) {
                continue;
            }
            let before = u32::try_from(lines.live.len()).unwrap_or(u32::MAX);
            lines.pos.insert(object.as_str().to_owned(), lines.entries.len());
            lines.entries.push((gone, before));
            if !gone {
                lines.live.push(object);
            }
        }
        lines
    }

    /// Where `token` (an object id, or `#n`) points.
    pub(crate) fn at(&self, token: &str) -> LineAt {
        if let Some(n) = token.strip_prefix('#') {
            return match n.parse::<u32>() {
                Ok(n) => LineAt::Index(n),
                Err(_) => LineAt::Missing,
            };
        }
        match self.pos.get(token).and_then(|p| self.entries.get(*p)) {
            Some((false, before)) => LineAt::Live(*before),
            Some((true, before)) => LineAt::Gone(*before),
            None => LineAt::Missing,
        }
    }

    /// The view index of a live line's token (`#n` included).
    pub(crate) fn index(&self, token: &str) -> Option<u32> {
        match self.at(token) {
            LineAt::Live(i) | LineAt::Index(i) => Some(i),
            _ => None,
        }
    }
}

/// Resolves two ends of a span on one axis (`a`, `b`, in either order) to view indexes: a gone
/// end moves inwards to the nearest live line (the first end to the line after it, the last to
/// the line before it). `None` when an end is missing or nothing is left of the span. `max`
/// clamps indexes.
pub(crate) fn span(a: LineAt, b: LineAt, max: u32) -> Option<(u32, u32)> {
    // A sort key between live lines: a gone line sits just before the live line after it.
    let key = |at: LineAt| -> Option<i64> {
        match at {
            LineAt::Live(i) | LineAt::Index(i) => Some(2 * i64::from(i)),
            LineAt::Gone(next) => Some(2 * i64::from(next) - 1),
            LineAt::Missing => None,
        }
    };
    let (ka, kb) = (key(a)?, key(b)?);
    let low = |at: LineAt| match at {
        LineAt::Live(i) | LineAt::Index(i) | LineAt::Gone(i) => Some(i),
        LineAt::Missing => None,
    };
    let high = |at: LineAt| match at {
        LineAt::Live(i) | LineAt::Index(i) => Some(i),
        LineAt::Gone(next) => next.checked_sub(1),
        LineAt::Missing => None,
    };
    let (va, vb) = if ka <= kb { (low(a)?, high(b)?) } else { (high(a)?, low(b)?) };
    let lo = va.min(vb);
    let last = max.saturating_sub(1);
    if (ka <= kb && va > vb) || (ka > kb && vb > va) || lo > last {
        return None;
    }
    Some((va.min(last), vb.min(last)))
}

/// One end of a line: a gone line resolves to the next live one.
pub(crate) fn line(at: LineAt, max: u32) -> Option<u32> {
    match at {
        LineAt::Live(i) | LineAt::Index(i) | LineAt::Gone(i) => Some(i.min(max.saturating_sub(1))),
        LineAt::Missing => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(spec: &[(&str, bool)]) -> Lines {
        Lines::new(spec.iter().map(|(id, gone)| (ObjectId::from_string(*id), *gone)).collect())
    }

    #[test]
    fn gone_lines_keep_their_place() {
        let l = lines(&[("a", false), ("b", true), ("c", false), ("a", false), ("d", true)]);
        assert_eq!(l.live.len(), 2);
        assert_eq!(l.at("a"), LineAt::Live(0));
        assert_eq!(l.at("b"), LineAt::Gone(1));
        assert_eq!(l.at("c"), LineAt::Live(1));
        assert_eq!(l.at("d"), LineAt::Gone(2));
        assert_eq!(l.at("#7"), LineAt::Index(7));
        assert_eq!(l.at("zz"), LineAt::Missing);
    }

    #[test]
    fn spans_shrink_to_live_lines() {
        // Lines: 0 live, gone, 1 live, 2 live, gone.
        assert_eq!(span(LineAt::Gone(1), LineAt::Live(2), MAX_ROWS), Some((1, 2)));
        assert_eq!(span(LineAt::Live(0), LineAt::Gone(3), MAX_ROWS), Some((0, 2)));
        assert_eq!(span(LineAt::Live(2), LineAt::Gone(1), MAX_ROWS), Some((2, 1)));
        assert_eq!(span(LineAt::Gone(1), LineAt::Gone(1), MAX_ROWS), None);
        assert_eq!(span(LineAt::Gone(0), LineAt::Gone(0), MAX_ROWS), None);
        assert_eq!(span(LineAt::Live(0), LineAt::Missing, MAX_ROWS), None);
        assert_eq!(span(LineAt::Index(3), LineAt::Index(MAX_ROWS + 5), MAX_ROWS), Some((3, MAX_ROWS - 1)));
    }

    #[test]
    fn pad_ids_are_deterministic_object_ids() {
        let sheet = ObjectId::from_string("s1");
        assert_eq!(pad_id(&sheet, Axis::Rows, 7), pad_id(&sheet, Axis::Rows, 7));
        assert_ne!(pad_id(&sheet, Axis::Rows, 7), pad_id(&sheet, Axis::Cols, 7));
        assert_eq!(pad_id(&sheet, Axis::Rows, 7).as_str().len(), 33);
    }
}
