//! Cells as `Cell` objects (plan K5).
//!
//! A sheet's `cells` map names each cell's `Cell` object by its key (`<rowObject>:<colObject>`):
//! the map value is the object's id. A `Cell` has
//! - `text`, a KDM `text` field: the cell's value when it is text (stored as typed, no `'`
//!   quoting), so concurrent typing in one cell merges by character. Its marks are the runs of a
//!   rich text cell (see `marks`): a write diffs the view's runs against the new ones
//!   (`kition_kit::session::write_rich_text`), so two people formatting different runs of one
//!   cell both keep theirs, and typed characters take the run at the caret (`expand: after`);
//! - `input`, a register: everything that is not text, as one value (a formula in id form behind
//!   `=`, a number, a boolean, an error, or `'` for an empty text). **Formulas stay a register**:
//!   the id form names rows and columns by object ids inside `⟦…⟧` tokens, and a character merge
//!   of two edits of one formula could cut a token (an id half from each side), which would read
//!   as neither person's reference; a number merged by character would be a number nobody typed.
//!   So two people editing one formula or number concurrently keep one of the edits, as before;
//! - `style`, `array` and `picture` registers;
//! - `by`, a register only a `Cell` created ahead of a person's write has: the creating
//!   session's token, which is also the end of the object's id ([`ahead_id`]: the cell's derived
//!   id, `-`, the token);
//! - `key`, also only on a `Cell` created ahead: its key in the map, so a reader of the new cell
//!   finds its place without reading the whole map (rule P1). Cells written in bulk (an import, a
//!   paste into a full write) have no `by` or `key`: on a large workbook it would be most of the
//!   snapshot again.
//!
//! One cell's entry names one object. A draft that types into a new cell the person typed into
//! too sets the cell's entry to another object: the merge refuses it (data model 6.6, rule D4).
//!
//! **Clearing** a cell empties its `Cell` (its text deleted by character, its registers unset)
//! and keeps the object and its entry ([`clear`]): what another person types into the cell at
//! the same time survives the clear, as typing does next to any concurrent text delete. An empty
//! `Cell` reads as no cell.
//!
//! When `input` is set, it is the cell's content, whatever `text` holds (text typed concurrently
//! with a formula stays in `text`, unread, until the cell is text again).

use std::collections::HashMap;

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_kit::session::{TextRun, sha256_hex};
use kition_sheet_core::Value as CellValue;
use kition_sheet_model::Cell;
use serde_json::{Map, Value, json};

use crate::KdmResult;
use crate::codec::parse_literal;

pub(crate) const CELL: &str = "Cell";
pub(crate) const TEXT: &str = "text";

/// What one cell stores, in either form.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Stored {
    /// The `input` register: a formula (`=…`, id form) or a literal that is not text.
    pub input: Option<String>,
    /// The `text` field: the cell's text value (empty when `input` is set).
    pub text: String,
    pub style: Option<String>,
    pub array: Option<String>,
    pub picture: Option<Value>,
    /// `text` as runs with their marks, when any run has one (empty: plain text).
    pub runs: Vec<TextRun>,
}

impl Stored {
    /// A stored input string (`codec::encode_input`) split into the register and the text.
    pub fn from_input(input: &str) -> Stored {
        let (input, text) = split_input(input);
        Stored { input, text, ..Stored::default() }
    }

    /// The cell's input as one string, the form `codec::decode_input` reads.
    #[cfg(test)]
    pub fn joined(&self) -> String {
        match &self.input {
            Some(input) => input.clone(),
            None if self.text.is_empty() => String::new(),
            None => crate::codec::literal(&CellValue::Text(std::sync::Arc::from(self.text.as_str()))),
        }
    }

    /// Whether nothing is stored (no entry).
    pub fn is_empty(&self) -> bool {
        self.input.as_deref().is_none_or(str::is_empty)
            && self.text.is_empty()
            && self.style.is_none()
            && self.array.is_none()
            && self.picture.is_none()
    }

    /// The registers a new `Cell` object is created with.
    fn registers(&self) -> Map<String, Value> {
        let mut fields = Map::new();
        for (name, v) in [
            ("input", self.input.clone().map(Value::String)),
            ("style", self.style.clone().map(Value::String)),
            ("array", self.array.clone().map(Value::String)),
            ("picture", self.picture.clone()),
        ] {
            if let Some(v) = v {
                fields.insert(name.to_owned(), v);
            }
        }
        fields
    }
}

/// A stored input (`=formula` or a literal) as the `input` register and the `text` field.
pub(crate) fn split_input(input: &str) -> (Option<String>, String) {
    if input.starts_with('=') {
        return (Some(input.to_owned()), String::new());
    }
    match parse_literal(input) {
        CellValue::Empty => (None, String::new()),
        CellValue::Text(t) if !t.is_empty() => (None, t.to_string()),
        _ => (Some(input.to_owned()), String::new()),
    }
}

/// What a view's cell writes in its `text` field: its text value (nothing for a formula, a
/// number…) and, for a rich text cell, its runs with their marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ViewText {
    pub text: String,
    /// Empty for plain text.
    pub runs: Vec<TextRun>,
}

/// The `text` a view's cell writes (see [`ViewText`]).
pub(crate) fn view_text(cell: Option<&Cell>) -> ViewText {
    match cell {
        Some(c @ Cell { formula: None, value: CellValue::Text(t), .. }) => ViewText { text: t.to_string(), runs: crate::marks::cell_runs(Some(c)) },
        _ => ViewText::default(),
    }
}

/// Sets the marks of `runs` on `id`'s text, which holds their characters unmarked.
pub(crate) fn mark(tx: &mut Tx<'_>, id: &ObjectId, runs: &[TextRun]) -> KdmResult<()> {
    let mut at = 0;
    for run in runs {
        let end = at + run.insert.chars().count();
        for (key, value) in &run.attributes {
            tx.text_mark(id, TEXT, at, end, key, value.clone())?;
        }
        at = end;
    }
    Ok(())
}

/// The id a cell's object is created under: derived from its sheet and key, so replicas that
/// create one cell concurrently create one object, not two that one map entry chooses between.
///
/// The id is `c` and 96 bits of the hash in base 62 (18 characters): every cell stores its id
/// twice (its object's key and its map entry), so on a workbook of 100,000 cells each character
/// is about 200 KB of snapshot (rule P1). 96 bits keep a collision between two cells of one
/// workbook out of reach (about 10⁻¹⁷ for a million cells).
pub(crate) fn cell_id(sheet: &ObjectId, key: &str) -> ObjectId {
    const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let hash = sha256_hex(format!("{}/{key}", sheet.as_str()).as_bytes());
    let mut n = hash.get(..24).and_then(|h| u128::from_str_radix(h, 16).ok()).unwrap_or_default();
    let mut id = [b'0'; 18];
    id[0] = b'c';
    for digit in id[1..].iter_mut().rev() {
        *digit = DIGITS[(n % 62) as usize];
        n /= 62;
    }
    ObjectId::from_string(String::from_utf8_lossy(&id).into_owned())
}

/// The id of a `Cell` created ahead of a person's write by the session whose token is `by`: the
/// cell's derived id ([`cell_id`]), `-`, the token.
pub(crate) fn ahead_id(derived: &ObjectId, by: &str) -> ObjectId {
    ObjectId::from_string(format!("{}-{by}", derived.as_str()))
}

/// The `Cell` object a map value names, if it is one and exists.
pub(crate) fn object_of(tx: &Tx<'_>, value: &Value) -> Option<ObjectId> {
    let id = ObjectId::from_string(value.as_str()?);
    (tx.exists(&id) && tx.object_type(&id).ok().as_deref() == Some(CELL)).then_some(id)
}

/// Creates the `Cell` object of `key` in `sheet`'s `cells` map, holding `stored`, and names it in
/// the map. `address`: its tool address (`<sheet id>!A1`), written with its `label`, when the
/// writer knows it (else positions are projected later). Answers its id, and whether it is the
/// derived one ([`cell_id`]: another replica may create the same object).
pub(crate) fn create(tx: &mut Tx<'_>, sheet: &ObjectId, key: &str, stored: &Stored, address: Option<String>) -> KdmResult<(ObjectId, bool)> {
    let mut fields = stored.registers();
    if let Some(address) = address {
        fields.insert("label".to_owned(), json!(format!("sheet:{address}")));
        fields.insert("address".to_owned(), Value::String(address));
    }
    let (id, derived) = match tx.create_with_id(CELL, &cell_id(sheet, key), fields.clone()) {
        Ok(id) => (id, true),
        // That id was used before (the cell was cleared): any id will do.
        Err(_) => (tx.create(CELL, fields)?, false),
    };
    if !stored.text.is_empty() {
        tx.text_insert(&id, TEXT, 0, &stored.text)?;
        mark(tx, &id, &stored.runs)?;
    }
    tx.map_set(sheet, "cells", key, json!(id.as_str()))?;
    Ok((id, derived))
}

/// Empties the cell `key` of `sheet` (see the module docs): a `Cell` object keeps its entry, its
/// text is deleted and its registers unset; an entry naming no live object is removed. Answers
/// whether it wrote anything.
pub(crate) fn clear(tx: &mut Tx<'_>, sheet: &ObjectId, key: &str) -> KdmResult<bool> {
    let v = tx.map_get(sheet, "cells", key)?;
    if v.is_null() {
        return Ok(false);
    }
    let Some(id) = object_of(tx, &v) else {
        tx.map_delete(sheet, "cells", key)?;
        return Ok(true);
    };
    let mut wrote = false;
    for field in ["input", "style", "array", "picture"] {
        if !tx.get(&id, field)?.is_null() {
            tx.set(&id, field, Value::Null)?;
            wrote = true;
        }
    }
    let len = tx.text_len(&id, TEXT)?;
    if len > 0 {
        tx.text_delete(&id, TEXT, 0, len)?;
        wrote = true;
    }
    Ok(wrote)
}

/// Removes `key` from `sheet`'s `cells` map, and deletes the `Cell` object it named. Answers
/// whether there was an entry.
pub(crate) fn remove(tx: &mut Tx<'_>, sheet: &ObjectId, key: &str) -> KdmResult<bool> {
    let v = tx.map_get(sheet, "cells", key)?;
    if v.is_null() {
        return Ok(false);
    }
    if let Some(id) = object_of(tx, &v) {
        tx.delete(&id)?;
    }
    tx.map_delete(sheet, "cells", key)?;
    Ok(true)
}

/// A cell's `Cell` object and what it stores. `None`: the entry names no live `Cell`.
pub(crate) fn read_entry(replica: &Replica, v: &Value) -> Option<(ObjectId, Stored)> {
    let id = ObjectId::from_string(v.as_str()?);
    // Each read is a lookup of the object (rule P1: a large workbook reads every cell on open):
    // `object_type` fails for a missing object, and only a formula has an array range.
    if replica.object_type(&id).ok().as_deref() != Some(CELL) {
        return None;
    }
    let string = |field: &str| replica.get(&id, field).ok().and_then(|v| v.as_str().map(str::to_owned));
    let input = string("input");
    let array = if input.as_deref().is_some_and(|i| i.starts_with('=')) { string("array") } else { None };
    let (text, runs) = text_runs(replica, &id);
    let stored = Stored { input, text, style: string("style"), array, picture: replica.get(&id, "picture").ok().filter(|p| !p.is_null()), runs };
    Some((id, stored))
}

/// A `Cell`'s text, and its runs when any has a mark (empty otherwise).
fn text_runs(replica: &Replica, id: &ObjectId) -> (String, Vec<TextRun>) {
    let runs = replica.text_delta(id, TEXT).unwrap_or_default();
    if runs.iter().all(|r| r.attributes.is_empty()) {
        // One run at most (adjacent runs differ in their marks).
        return (runs.into_iter().map(|r| r.insert).collect(), Vec::new());
    }
    (runs.iter().map(|r| r.insert.as_str()).collect(), runs)
}

/// The texts of a sheet's `Cell` objects that differ from what their view's cell would write
/// ([`view_text`]): a merge's text next to a formula, a text no cell value reads back as. The
/// writer diffs a cell's text against this, or against its view's, as the field's current text.
pub(crate) type OddTexts = HashMap<ObjectId, String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_split_into_register_and_text() {
        assert_eq!(split_input("=SUM(1,2)"), (Some("=SUM(1,2)".into()), String::new()));
        assert_eq!(split_input("3.5"), (Some("3.5".into()), String::new()));
        assert_eq!(split_input("TRUE"), (Some("TRUE".into()), String::new()));
        assert_eq!(split_input("#N/A"), (Some("#N/A".into()), String::new()));
        assert_eq!(split_input("'"), (Some("'".into()), String::new()), "an empty text keeps its quote");
        assert_eq!(split_input(""), (None, String::new()));
        assert_eq!(split_input("hello"), (None, "hello".into()));
        assert_eq!(split_input("'=not a formula"), (None, "=not a formula".into()), "text is stored unquoted");
        assert_eq!(split_input("'12"), (None, "12".into()));
        for input in ["=A1", "3.5", "TRUE", "'", "", "hello", "'=x", "'12"] {
            let (i, t) = split_input(input);
            let stored = Stored { input: i, text: t, ..Stored::default() };
            assert_eq!(parse_literal(&stored.joined()), parse_literal(input), "{input} joins back");
        }
    }

    #[test]
    fn cell_ids_are_valid_and_stable() {
        let sheet = ObjectId::from_string("0192f0aa-0000-7000-8000-000000000001");
        let a = cell_id(&sheet, "r1:c1");
        assert_eq!(a, cell_id(&sheet, "r1:c1"));
        assert_ne!(a, cell_id(&sheet, "r1:c2"));
        assert!(a.as_str().len() <= 128 && a.as_str().chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        assert_eq!(a.as_str().len(), 18, "short: every cell stores its id twice");
        // Distinct keys of one sheet get distinct ids.
        let ids: std::collections::HashSet<ObjectId> = (0..10_000).map(|k| cell_id(&sheet, &format!("r{k}:c1"))).collect();
        assert_eq!(ids.len(), 10_000);
    }
}
