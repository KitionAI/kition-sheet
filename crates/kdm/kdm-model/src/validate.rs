//! Deterministic repair after a merge, an undo or a redo ([`kition_kit::session::KitModel::validate`],
//! G5): state only, so every replica writes the same repair for the same merged state.
//!
//! - List entries naming deleted objects, and repeated entries (two replicas padding a sheet with
//!   the same line objects), are removed; the first entry stays, where the reader reads it.
//! - A name scoped to a deleted sheet becomes a workbook name.
//! - A cell, comment or link keyed by a deleted (`gone`) or unknown row or column is removed
//!   (a cell with its `Cell` object), on every sheet whose cells or lines changed. (Formulas naming a deleted line read `#REF!`,
//!   and feature ranges shrink to their live lines, as they are read: neither needs a repair.)
//! - A workbook left without sheets gets one, under an id derived from the state, so replicas
//!   repairing the same state create the same object.
//!
//! A change of only cells and sheets (`Cell` objects, sheets, lines those sheets list, and deleted
//! objects) can leave nothing dangling but in the sheets it changed (a cell typed into a row deleted
//! concurrently): only those are looked at (rule P1: merging one cell's typing must not read every
//! line of every sheet). The workbook's lists and names change only with the workbook, another
//! object or a deleted one, which looks at everything.

use std::collections::{BTreeSet, HashSet};

use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::sha256_hex;
use serde_json::{Map, Value, json};

use crate::{Ids, KdmResult, root_id};

pub(crate) fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    validate_known(tx, changed, None)
}

/// [`validate`], given the view's objects from before the change (`view`, a session's): a sheet
/// whose lists of lines are still the view's live lines, when no line changed or went, has no
/// line to clean and none gone, which is then known without reading every line (rule P1: one
/// cell typed into a sheet of 10,000 rows). The repair is the same as without `view`.
pub(crate) fn validate_known(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>, view: Option<&Ids>) -> KdmResult<bool> {
    let root = root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    // A sheet deleted concurrently with an edit of its cells is no change of only cells: its
    // list entry and the names scoped to it are repaired too.
    let listed = tx.list_items(&root, "sheets")?;
    let cells_only = sheets_only(tx, changed)? && listed.iter().all(|sheet| tx.exists(sheet));
    let mut wrote = false;
    if !cells_only {
        wrote |= clean_list(tx, &root, "sheets")?;
        wrote |= clean_list(tx, &root, "names")?;
    }
    let sheets = tx.list_items(&root, "sheets")?;

    let names = if cells_only { Vec::new() } else { tx.list_items(&root, "names")? };
    for name in names {
        if tx.object_type(&name).ok().as_deref() != Some("Name") {
            continue;
        }
        if let Value::String(scope) = tx.get(&name, "scope")?
            && !tx.exists(&ObjectId::from_string(scope))
        {
            tx.set(&name, "scope", Value::Null)?;
            wrote = true;
        }
    }

    // No object was deleted: a sheet whose lists are the view's live lines (none of them changed)
    // and then new lines, each listed once and live, holds no gone, deleted or repeated line.
    let lines_known = view.is_some() && changed.iter().all(|id| tx.exists(id));
    let new_lines: HashSet<&ObjectId> = if lines_known {
        changed.iter().filter(|id| tx.object_type(id).is_ok_and(|ty| ty == "Row" || ty == "Col")).collect()
    } else {
        HashSet::new()
    };
    for sheet in &sheets {
        if cells_only && !changed.contains(sheet) {
            continue;
        }
        if lines_known
            && let Some(sid) = view.and_then(|ids| ids.sheets.iter().find(|s| s.object == *sheet))
            && view_lines_and_new(tx, sheet, "rows", &sid.rows, &new_lines)?
            && view_lines_and_new(tx, sheet, "cols", &sid.cols, &new_lines)?
        {
            wrote |= clean_list(tx, sheet, "images")?;
            continue;
        }
        let mut lines_changed = false;
        let mut any_gone = false;
        // Each axis's lines (`clean_list`'s cleaning, with one read of each line: rule P1) and
        // which are gone.
        let mut lines: [Vec<(ObjectId, bool)>; 2] = [Vec::new(), Vec::new()];
        for (k, field) in ["rows", "cols"].into_iter().enumerate() {
            let items = tx.list_items(sheet, field)?;
            let mut seen = HashSet::with_capacity(items.len());
            let mut doomed = Vec::new();
            let mut kept = Vec::with_capacity(items.len());
            for (i, line) in items.iter().enumerate() {
                let gone = match tx.get(line, "gone") {
                    Ok(v) => v == Value::Bool(true),
                    Err(_) if !tx.exists(line) => {
                        doomed.push(i);
                        continue;
                    }
                    Err(_) => false,
                };
                if !seen.insert(line) {
                    doomed.push(i);
                    continue;
                }
                lines_changed |= changed.contains(line);
                any_gone |= gone;
                kept.push((line.clone(), gone));
            }
            for &index in doomed.iter().rev() {
                tx.list_remove(sheet, field, index)?;
            }
            wrote |= !doomed.is_empty();
            if let Some(slot) = lines.get_mut(k) {
                *slot = kept;
            }
        }
        wrote |= clean_list(tx, sheet, "images")?;
        if !any_gone || !(lines_changed || changed.contains(sheet)) {
            continue;
        }
        let [rows, cols] =
            lines.map(|axis| axis.into_iter().filter(|(_, gone)| !gone).map(|(line, _)| line.as_str().to_owned()).collect::<HashSet<String>>());
        let (rows, cols) = (&rows, &cols);
        for field in ["cells", "comments", "hyperlinks"] {
            for (key, v) in tx.map_entries(sheet, field)? {
                if !valid_key(&key, rows, cols) {
                    // A cell's `Cell` object goes with its entry.
                    if field == "cells"
                        && let Some(cell) = crate::cells::object_of(tx, &v)
                    {
                        tx.delete(&cell)?;
                    }
                    tx.map_delete(sheet, field, &key)?;
                    wrote = true;
                }
            }
        }
    }

    if sheets.is_empty() {
        wrote |= ensure_sheet(tx, &root)?;
    }
    Ok(wrote)
}

/// Whether `sheet`'s list `field` is `view` (live lines none of which is in `new`) and then lines of
/// `new`, each once and not gone.
fn view_lines_and_new(tx: &Tx<'_>, sheet: &ObjectId, field: &str, view: &[ObjectId], new: &HashSet<&ObjectId>) -> KdmResult<bool> {
    let items = tx.list_items(sheet, field)?;
    let Some((prefix, added)) = items.split_at_checked(view.len()) else { return Ok(false) };
    if prefix != view || (!new.is_empty() && prefix.iter().any(|line| new.contains(line))) {
        return Ok(false);
    }
    let mut seen = HashSet::with_capacity(added.len());
    for line in added {
        if !new.contains(line) || !seen.insert(line) || tx.get(line, "gone")? == Value::Bool(true) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `changed` holds only `Cell` objects, sheets, lines a changed sheet lists, and deleted
/// objects (a writer removes what lists an object with the object, so its owner changed too).
fn sheets_only(tx: &Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    let mut lines = Vec::new();
    let mut sheets = Vec::new();
    for id in changed {
        match tx.object_type(id).ok().as_deref() {
            Some(crate::cells::CELL) => {}
            Some("Sheet") => sheets.push(id),
            Some("Row" | "Col") => lines.push(id),
            // Deleted (a cleared cell's object): whatever listed it changed with it.
            None if !tx.exists(id) && *id != root_id() => {}
            _ => return Ok(false),
        }
    }
    if lines.is_empty() {
        return Ok(true);
    }
    let mut listed = HashSet::new();
    for sheet in sheets {
        for field in ["rows", "cols"] {
            listed.extend(tx.list_items(sheet, field)?);
        }
    }
    Ok(lines.into_iter().all(|line| listed.contains(line)))
}

/// Whether a line token names a live line of `live` (or a line by index).
fn live_token(token: &str, live: &HashSet<String>) -> bool {
    match token.strip_prefix('#') {
        Some(n) => n.parse::<u32>().is_ok(),
        None => live.contains(token),
    }
}

/// Whether a cell key (`<row>:<col>`) names live lines.
fn valid_key(key: &str, rows: &HashSet<String>, cols: &HashSet<String>) -> bool {
    key.match_indices(':').any(|(i, _)| match (key.get(..i), key.get(i + 1..)) {
        (Some(r), Some(c)) => live_token(r, rows) && live_token(c, cols),
        _ => false,
    })
}

/// Lists a new sheet in a workbook that has none.
fn ensure_sheet(tx: &mut Tx<'_>, root: &ObjectId) -> KdmResult<bool> {
    let hash = sha256_hex(format!("{}/sheets/empty", root.as_str()).as_bytes());
    let id = ObjectId::from_string(format!("sheet-{}", hash.get(..40).unwrap_or(&hash)));
    let fields: Map<String, Value> = [("name".to_owned(), json!("Sheet1"))].into_iter().collect();
    let object = match tx.create_with_id("Sheet", &id, fields.clone()) {
        Ok(object) => object,
        // That id was used before (and deleted): any id will do.
        Err(_) => tx.create("Sheet", fields)?,
    };
    tx.list_insert(root, "sheets", 0, &object)?;
    Ok(true)
}

/// Removes list entries naming an object that no longer exists, and every entry after the first
/// of one object.
fn clean_list(tx: &mut Tx<'_>, owner: &ObjectId, field: &str) -> KdmResult<bool> {
    let items = tx.list_items(owner, field)?;
    let mut seen = HashSet::new();
    let doomed: Vec<usize> = items.iter().enumerate().filter(|(_, o)| !tx.exists(o) || !seen.insert((*o).clone())).map(|(i, _)| i).collect();
    for &index in doomed.iter().rev() {
        tx.list_remove(owner, field, index)?;
    }
    Ok(!doomed.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_name_live_lines() {
        let rows: HashSet<String> = ["r1".to_owned(), "r:2".to_owned()].into_iter().collect();
        let cols: HashSet<String> = ["c1".to_owned()].into_iter().collect();
        assert!(valid_key("r1:c1", &rows, &cols));
        assert!(valid_key("r:2:c1", &rows, &cols), "an id holding `:` still splits");
        assert!(valid_key("#900000:c1", &rows, &cols));
        assert!(!valid_key("gone:c1", &rows, &cols));
        assert!(!valid_key("r1", &rows, &cols));
    }
}
