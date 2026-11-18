//! Cells as `Cell` objects (schema version 2, plan K5 and K6).
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
//! - `style`, `array` and `picture` registers, as the 0.3 entry's fields;
//! - `by`, a register only a `Cell` created ahead of a person's write has: the creating
//!   session's token, which is also the end of the object's id ([`ahead_id`]: the cell's derived
//!   id, `-`, the token). Two sessions typing into one empty cell at once so create two objects,
//!   and the map entry names one of them (the *kept* one);
//! - `key`, also only on a `Cell` created ahead: its key in the map, so a peer reading the new
//!   cell finds its place without reading the whole map (rule P1), and a session finds the other
//!   object of such a race (its key's entry names another object);
//! - `merged`, only on a `Cell` created ahead that kept its entry in such a race: the other
//!   objects, by id. The cell's text is its own text and then theirs, in that order, every
//!   replica reading the same; a write diffs each object's part on its own, so typing in any of
//!   them merges by character and each person's undo reverts their own typing. Its other
//!   registers are the kept object's (an input that is not text, a formula or a number, is a
//!   whole value: the other's is not read). Sessions record `merged` (`SheetSession`) as they see
//!   the race, after an import and when opening a replica merged elsewhere, with the other's
//!   style when the kept one has none. Cells written in bulk (an import, a paste into a full write) have no `by`,
//!   `key` or `merged`: on a large workbook it would be most of the snapshot again.
//!
//! **Clearing** a cell empties its `Cell` (its text deleted by character, its registers unset)
//! and keeps the object and its entry ([`clear`]): what another person types into the cell at
//! the same time survives the clear, as typing does next to any concurrent text delete. An empty
//! `Cell` reads as no cell.
//!
//! When `input` is set, it is the cell's content, whatever `text` holds (text typed concurrently
//! with a formula stays in `text`, unread, until the cell is text again).
//!
//! **0.3 replicas** (schema version 1) hold each cell as one register `{input, style?, array?,
//! picture?}` in the map. [`read_entry`] reads both forms; [`migrate`] rewrites every register
//! entry as a `Cell` object, in one `system` transaction before a session's first write.

use std::collections::HashMap;

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_kit::session::{TextRun, diff_text, sha256_hex};
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
    /// The `Cell` objects whose texts make up `text`, each with its own (the kept object first,
    /// then the ones `merged` names); empty when only one object holds the text.
    pub parts: Vec<(ObjectId, String)>,
    /// `text` as runs with their marks, when any run has one (empty: plain text).
    pub runs: Vec<TextRun>,
}

impl Stored {
    /// A stored 0.3 input string split into the register and the text.
    pub fn from_input(input: &str) -> Stored {
        let (input, text) = split_input(input);
        Stored { input, text, ..Stored::default() }
    }

    /// The cell's input as one string, the 0.3 form `codec::decode_input` reads.
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
/// create one cell concurrently (two migrations of one 0.3 replica, two people typing into one
/// empty cell) create one object, not two that one map entry chooses between.
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

/// The register of a `Cell` created ahead naming the other objects of a race (see the module docs).
pub(crate) const MERGED: &str = "merged";

/// The id of a `Cell` created ahead of a person's write by the session whose token is `by`: the
/// cell's derived id ([`cell_id`]), `-`, the token. Sessions creating one cell at once make
/// distinct objects, and keep both texts (see the module docs).
/// Whether the `Cell` object `id` holds nothing: no text and no other content.
pub(crate) fn is_empty_object(replica: &Replica, id: &ObjectId) -> bool {
    let unset = |field: &str| replica.get(id, field).map_or(true, |v| v.is_null());
    replica.text(id, TEXT).map_or(true, |text| text.is_empty()) && unset("input") && unset("picture") && unset("array")
}

pub(crate) fn ahead_id(derived: &ObjectId, by: &str) -> ObjectId {
    ObjectId::from_string(format!("{}-{by}", derived.as_str()))
}

/// Whether `id` is a `Cell` created ahead ([`ahead_id`]), the only cells that can hold `merged`.
pub(crate) fn is_ahead(id: &str) -> bool {
    id.len() > 19 && id.starts_with('c') && id.as_bytes().get(18) == Some(&b'-')
}

/// The `Cell` object a map value names, if it is one and exists.
pub(crate) fn object_of(tx: &Tx<'_>, value: &Value) -> Option<ObjectId> {
    let id = ObjectId::from_string(value.as_str()?);
    (tx.exists(&id) && tx.object_type(&id).ok().as_deref() == Some(CELL)).then_some(id)
}

/// A 0.3 register entry (`{input, style?, array?, picture?}`) as what it stores.
pub(crate) fn from_register(v: &Value) -> Option<Stored> {
    let map = v.as_object()?;
    let mut stored = Stored::from_input(map.get("input").and_then(Value::as_str).unwrap_or(""));
    stored.style = map.get("style").and_then(Value::as_str).map(str::to_owned);
    stored.array = map.get("array").and_then(Value::as_str).map(str::to_owned);
    stored.picture = map.get("picture").filter(|p| !p.is_null()).cloned();
    Some(stored)
}

/// Creates the `Cell` object of `key` in `sheet`'s `cells` map, holding `stored`, and names it in
/// the map. Answers its id, and whether it is the derived one ([`cell_id`]: another replica may
/// create the same object).
pub(crate) fn create(tx: &mut Tx<'_>, sheet: &ObjectId, key: &str, stored: &Stored) -> KdmResult<(ObjectId, bool)> {
    let fields = stored.registers();
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
/// text is deleted and its registers unset; any other entry (a 0.3 register, an entry naming no
/// live object) is removed. Answers whether it wrote anything.
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
    for part in std::iter::once(id.clone()).chain(merged_of(tx, &id)) {
        let len = tx.text_len(&part, TEXT)?;
        if len > 0 {
            tx.text_delete(&part, TEXT, 0, len)?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// For a `Cell` created ahead (`id`) whose key's entry names another live `Cell` (it lost a race
/// on one new cell, see the module docs): the sheet (by its place in `sheets`) and that object.
pub(crate) fn holder_of(replica: &Replica, sheets: &[ObjectId], id: &ObjectId) -> Option<(usize, ObjectId)> {
    if !is_ahead(id.as_str()) {
        return None;
    }
    let key = replica.get(id, "key").ok()?.as_str()?.to_owned();
    let derived = id.as_str().get(..18)?;
    let k = sheets.iter().position(|sheet| cell_id(sheet, &key).as_str() == derived)?;
    let named = ObjectId::from_string(replica.map_get(sheets.get(k)?, "cells", &key).ok()?.as_str()?);
    (named != *id && replica.object_type(&named).ok().as_deref() == Some(CELL)).then_some((k, named))
}

/// A cell's new text `new` split over the objects whose texts `olds` made up its old one (see the
/// module docs): kept characters stay in their object's part, inserted ones join the part of the
/// character before them (the first part at the start).
pub(crate) fn split_parts(olds: &[String], new: &str) -> Vec<String> {
    let old: String = olds.concat();
    let mut owner_old: Vec<usize> = Vec::with_capacity(old.chars().count());
    for (k, part) in olds.iter().enumerate() {
        owner_old.extend(std::iter::repeat_n(k, part.chars().count()));
    }
    let mut owner_new: Vec<usize> = Vec::with_capacity(new.chars().count());
    let mut at = 0;
    for edit in diff_text(&old, new) {
        owner_new.extend(owner_old.get(at..edit.index).unwrap_or(&[]).iter().copied());
        let last = owner_new.last().copied().unwrap_or(0);
        owner_new.extend(std::iter::repeat_n(last, edit.insert.chars().count()));
        at = edit.index + edit.delete;
    }
    owner_new.extend(owner_old.get(at..).unwrap_or(&[]).iter().copied());
    let mut out = vec![String::new(); olds.len().max(1)];
    for (ch, k) in new.chars().zip(owner_new) {
        if let Some(part) = out.get_mut(k) {
            part.push(ch);
        }
    }
    out
}

/// The live `Cell` objects `id`'s `merged` names (none unless `id` is a cell created ahead).
pub(crate) fn merged_of(tx: &Tx<'_>, id: &ObjectId) -> Vec<ObjectId> {
    if !is_ahead(id.as_str()) {
        return Vec::new();
    }
    let v = tx.get(id, MERGED).unwrap_or(Value::Null);
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ObjectId::from_string)
        .filter(|o| o != id && tx.object_type(o).ok().as_deref() == Some(CELL))
        .collect()
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

/// The schema version whose cells are `Cell` objects: a migrated replica requires it (a 0.3
/// reader, on version 1, would read the objects' ids as cells with no input).
pub(crate) const CELL_OBJECTS: u32 = 2;

/// Rewrites every 0.3 register entry of every sheet's `cells` map as a `Cell` object (K6), and
/// raises the document's minimum schema version to [`CELL_OBJECTS`] in the same transaction (which
/// must be a `system` one), so a 0.3 reader refuses the migrated replica instead of misreading it.
/// Answers whether it wrote anything; a replica in the object form is left as it is.
pub(crate) fn migrate(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let root = crate::root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    let mut wrote = tx.raise_schema_version(CELL_OBJECTS)?;
    for sheet in tx.list_items(&root, "sheets")? {
        if tx.object_type(&sheet).ok().as_deref() != Some("Sheet") {
            continue;
        }
        let mut legacy: Vec<(String, Stored)> =
            tx.map_entries(&sheet, "cells")?.into_iter().filter_map(|(key, v)| Some((key, from_register(&v)?))).collect();
        legacy.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, stored) in legacy {
            create(tx, &sheet, &key, &stored)?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// Whether the replica is not yet in the object form: it does not require schema version
/// [`CELL_OBJECTS`] (a 0.3 replica, even one with no cells). A migrated replica can still get a
/// register entry, merged from a 0.3 peer's edit made before it saw the migration; that reads
/// as any 0.3 entry does, and its next write makes it a `Cell` ([`create`]), so no scan of
/// every cell looks for one.
pub(crate) fn needs_migration(replica: &Replica) -> bool {
    replica.exists(&crate::root_id()) && replica.min_schema_version() < CELL_OBJECTS
}

/// A cell's stored form as the replica holds it: a `Cell` object or a 0.3 register. `None`: the
/// entry names no live `Cell` (and is not a register).
pub(crate) fn read_entry(replica: &Replica, v: &Value) -> Option<(Option<ObjectId>, Stored)> {
    if let Some(stored) = from_register(v) {
        return Some((None, stored));
    }
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
    let mut stored = Stored {
        input,
        text,
        style: string("style"),
        array,
        picture: replica.get(&id, "picture").ok().filter(|p| !p.is_null()),
        parts: Vec::new(),
        runs,
    };
    // A cell created ahead may hold the texts of the other objects of a race (see the module docs).
    if is_ahead(id.as_str())
        && let Some(others) = replica.get(&id, MERGED).ok().as_ref().and_then(Value::as_array)
    {
        let mut parts = vec![(id.clone(), stored.text.clone())];
        for other in others.iter().filter_map(Value::as_str).map(ObjectId::from_string) {
            if other == id || parts.iter().any(|(o, _)| *o == other) || replica.object_type(&other).ok().as_deref() != Some(CELL) {
                continue;
            }
            let (text, runs) = text_runs(replica, &other);
            if !runs.is_empty() || !stored.runs.is_empty() {
                if stored.runs.is_empty() {
                    stored.runs.push(TextRun::new(stored.text.clone()));
                }
                if runs.is_empty() {
                    stored.runs.push(TextRun::new(text.clone()));
                }
                stored.runs.extend(runs);
            }
            stored.text.push_str(&text);
            parts.push((other, text));
        }
        if parts.len() > 1 {
            stored.parts = parts;
        }
    }
    Some((Some(id), stored))
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
