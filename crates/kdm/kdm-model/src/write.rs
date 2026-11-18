//! `Workbook` -> replica: a full write (a new replica, an import) and an incremental diff (the
//! engine command bridge) that writes only what changed. A full write is a diff from nothing.
//!
//! **Sheets** have no ids in the engine: they are paired with the view's by `Arc` identity (an
//! untouched sheet), then by name, then, between sheets so paired, in order (a renamed sheet).
//! **Structural edits** (G3, the engine's log) are applied first: an insert of `n` rows or columns
//! creates `n` line objects in the sheet's list, a delete marks the lines `gone` (see `lines`).
//! Then, with the shifted index -> object maps, cells, comments and links are diffed by key:
//! after a structural edit the whole sheet is compared with what the replica holds (keys are
//! line objects, so the cells below an insert keep their keys and values and are not written);
//! otherwise only the cells the engine's copy-on-write store says changed are. Formulas are
//! written in id form, so a formula the insert moved has the same stored form and is not
//! written either. Line info, the sheet's `props` and `features`, images, names, styles and the
//! workbook's registers follow.
//!
//! **Objects other replicas may create too** (padding lines and `Cell` objects, whose ids are
//! derived from their place) are logged as [`Created`]: `SheetSession::record` creates them
//! ahead of a person's write, in a `system` transaction, so undoing that person's step never
//! deletes an object a collaborator created at the same time and wrote into.
//!
//! A register or map entry is written only when its new value differs from what the replica
//! holds. A cell that has a `Cell` object keeps it: its registers are set where they differ and
//! its `text` gets the character edits from the view's old text to the new one (see `cells`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::{BlobSink, TextRun, sha256_hex, write_rich_text, write_text};
use kition_sheet_core::CellRef;
use kition_sheet_formula::adjust::Edit;
use kition_sheet_model::{Cell, CellPicture, Sheet, StructuralEdit, StyleId, Workbook};
use serde_json::{Map, Value, json};

use crate::cells::{self, Stored, ViewText};
use crate::codec::{Encode, encode_formula, encode_input, encode_range};
use crate::features::{image_props, line_info, name_props, sheet_features, sheet_props, to_json, workbook_props};
use crate::ids::{Ids, SheetIds};
use crate::lines::{Axis, Use, pad_id};
use crate::list::{longest_increasing, sync_list};
use crate::{KdmResult, root_id};

/// More lines than this in one insert are not applied as list inserts: the sheet is then written
/// by position, as without a structural log.
const MAX_INSERT: u32 = 1 << 16;

/// Writes a whole workbook into a fresh replica's creating transaction. Answers the `Workbook`
/// object's id.
pub fn write_document(tx: &mut Tx<'_>, wb: &Workbook, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<ObjectId> {
    let root = root_id();
    tx.create_with_id("Workbook", &root, Map::new())?;
    *ids = Ids::default();
    Writer::new(tx, blobs).write(None, wb, &[], ids)?;
    Ok(root)
}

/// Writes only what differs between `before` (the workbook `ids` describe) and `after`, and leaves
/// `ids` describing `after`. `edits` are the structural edits that led from `before` to `after`
/// (the engine's log for the command, `DocState::take_structural_edits`; empty when unknown: the
/// write is then by position, correct but larger). Answers whether it wrote anything.
pub fn write_changes(
    tx: &mut Tx<'_>,
    before: &Workbook,
    after: &Workbook,
    edits: &[StructuralEdit],
    ids: &mut Ids,
    blobs: &mut dyn BlobSink,
) -> KdmResult<bool> {
    write_changes_logged(tx, before, after, edits, ids, blobs).map(|(wrote, _)| wrote)
}

/// An object [`write_changes`] created under a derived id, in order.
#[derive(Clone, Debug)]
pub(crate) enum Created {
    /// A padding line, appended to `sheet`'s list of `axis`.
    Line { sheet: ObjectId, axis: Axis, id: ObjectId },
    /// A `Cell` object named by `key` in `sheet`'s `cells` map.
    Cell { sheet: ObjectId, key: String, id: ObjectId },
}

/// [`write_changes`], also answering the objects it created under derived ids ([`Created`]).
pub(crate) fn write_changes_logged(
    tx: &mut Tx<'_>,
    before: &Workbook,
    after: &Workbook,
    edits: &[StructuralEdit],
    ids: &mut Ids,
    blobs: &mut dyn BlobSink,
) -> KdmResult<(bool, Vec<Created>)> {
    let mut writer = Writer::new(tx, blobs);
    writer.write(Some(before), after, edits, ids)?;
    Ok((writer.wrote, writer.created))
}

/// Creates the objects of `created` that can be made on their own, empty: padding lines of
/// existing sheets (appended to their lists) and `Cell` objects whose key names lines that exist
/// (or `#n` indexes) and is not taken, each under its own id (`cells::ahead_id`) with `by` (the
/// creating session's token) and its `key` (see `cells`).
/// Answers the lines added, by sheet and axis, in order.
pub(crate) fn create_ahead(tx: &mut Tx<'_>, created: &[Created], by: &str) -> KdmResult<Vec<(ObjectId, Axis, ObjectId)>> {
    let mut lines = Vec::new();
    // Each list's length, read once (a write may pad thousands of lines).
    let mut lengths: HashMap<(ObjectId, Axis), usize> = HashMap::new();
    for c in created {
        match c {
            Created::Line { sheet, axis, id } => {
                if !tx.exists(sheet) || tx.exists(id) {
                    continue;
                }
                tx.create_with_id(axis.ty(), id, [("key".into(), json!(id.as_str()))].into_iter().collect())?;
                let at = match lengths.get_mut(&(sheet.clone(), *axis)) {
                    Some(len) => len,
                    None => lengths.entry((sheet.clone(), *axis)).or_insert(tx.list_items(sheet, axis.field())?.len()),
                };
                tx.list_insert(sheet, axis.field(), *at, id)?;
                *at += 1;
                lines.push((sheet.clone(), *axis, id.clone()));
            }
            Created::Cell { sheet, key, id } => {
                // This session's own object (`cells::ahead_id`): another session typing into the
                // cell at once makes another, and both texts stay.
                let id = &cells::ahead_id(id, by);
                let line_ok = |token: &str| token.starts_with('#') || tx.exists(&ObjectId::from_string(token));
                let resolvable =
                    key.match_indices(':').any(|(i, _)| matches!((key.get(..i), key.get(i + 1..)), (Some(r), Some(c)) if line_ok(r) && line_ok(c)));
                if !tx.exists(sheet) || tx.exists(id) || !resolvable || !tx.map_get(sheet, "cells", key)?.is_null() {
                    continue;
                }
                let fields: Map<String, Value> = [("by".to_owned(), json!(by)), ("key".to_owned(), json!(key))].into_iter().collect();
                tx.create_with_id(cells::CELL, id, fields)?;
                tx.map_set(sheet, "cells", key, json!(id.as_str()))?;
            }
        }
    }
    Ok(lines)
}

/// The `styles` key of a style: a hash of its content.
pub(crate) fn style_key(style: &Value) -> String {
    let hash = sha256_hex(style.to_string().as_bytes());
    hash.get(..24).unwrap_or(&hash).to_owned()
}

/// The `cells` (and `comments`, `hyperlinks`) key of a position: `<row>:<col>`, each a line token.
pub(crate) fn cell_key(row: &str, col: &str) -> String {
    format!("{row}:{col}")
}

struct Writer<'w, 'tx> {
    tx: &'w mut Tx<'tx>,
    blobs: &'w mut dyn BlobSink,
    wrote: bool,
    /// `after`'s sheets by lower-case name.
    names: HashMap<String, usize>,
    /// `after`'s sheets' objects, their lines as they are being written.
    sheets: Vec<SheetIds>,
    /// Each sheet's replica lists (rows, cols) with every entry, read when first needed.
    raw: Vec<[Option<Vec<ObjectId>>; 2]>,
    restored_lines: Vec<[Vec<String>; 2]>,
    restoring: bool,
    /// `after`'s style table: each style's key.
    styles: Vec<String>,
    /// Blob references of the cell pictures written so far, by picture.
    pictures: HashMap<*const CellPicture, Value>,
    /// The objects created under derived ids so far.
    created: Vec<Created>,
}

impl Encode for Writer<'_, '_> {
    fn sheet_index(&self, name: &str) -> Option<usize> {
        self.names.get(&name.to_ascii_lowercase()).copied()
    }

    fn sheet_object(&self, sheet: usize) -> Option<String> {
        self.sheets.get(sheet).map(|s| s.object.as_str().to_owned())
    }

    fn line(&mut self, sheet: usize, axis: Axis, index: u32, purpose: Use) -> KdmResult<String> {
        let Some(lines) = self.sheets.get(sheet).map(|s| s.axis(axis)) else { return Ok(format!("#{index}")) };
        if let Some(object) = lines.get(index as usize) {
            return Ok(object.as_str().to_owned());
        }
        let limit = match purpose {
            Use::Content => axis.limit(),
            Use::Ref => axis.ref_limit(),
        };
        if index >= limit {
            return Ok(format!("#{index}"));
        }
        self.extend(sheet, axis, index as usize + 1)?;
        Ok(self.sheets.get(sheet).and_then(|s| s.axis(axis).get(index as usize)).map_or_else(|| format!("#{index}"), |o| o.as_str().to_owned()))
    }

    fn style_key(&self, id: StyleId) -> Option<String> {
        self.styles.get(id.0 as usize).cloned()
    }
}

impl<'w, 'tx> Writer<'w, 'tx> {
    fn new(tx: &'w mut Tx<'tx>, blobs: &'w mut dyn BlobSink) -> Self {
        Writer {
            tx,
            blobs,
            wrote: false,
            names: HashMap::new(),
            sheets: Vec::new(),
            raw: Vec::new(),
            restored_lines: Vec::new(),
            restoring: false,
            styles: Vec::new(),
            pictures: HashMap::new(),
            created: Vec::new(),
        }
    }

    fn set(&mut self, object: &ObjectId, field: &str, v: Value) -> KdmResult<()> {
        if !same_json(&self.tx.get(object, field)?, &v) {
            self.tx.set(object, field, v)?;
            self.wrote = true;
        }
        Ok(())
    }

    fn create(&mut self, ty: &str, pairs: Vec<(&str, Value)>) -> KdmResult<ObjectId> {
        self.wrote = true;
        let fields: Map<String, Value> = pairs.into_iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.to_owned(), v)).collect();
        self.tx.create(ty, fields)
    }

    fn create_restored(&mut self, ty: &str, pairs: Vec<(&str, Value)>, saved: Option<&String>) -> KdmResult<ObjectId> {
        if self.restoring
            && let Some(saved) = saved
        {
            let fields = pairs.iter().filter(|(_, value)| !value.is_null()).map(|(key, value)| ((*key).to_owned(), value.clone())).collect();
            let object = self.tx.create_with_id(ty, &ObjectId::from_string(saved.clone()), fields)?;
            self.wrote = true;
            return Ok(object);
        }
        self.create(ty, pairs)
    }

    fn delete(&mut self, object: &ObjectId) -> KdmResult<()> {
        if self.tx.exists(object) {
            self.tx.delete(object)?;
            self.wrote = true;
        }
        Ok(())
    }

    /// The replica's list of a sheet's lines, every entry (gone, repeated) included.
    fn raw_list(&mut self, sheet: usize, axis: Axis) -> KdmResult<&mut Vec<ObjectId>> {
        let object = self.sheets.get(sheet).map(|s| s.object.clone()).ok_or_else(|| crate::kdm_err("no such sheet"))?;
        let slot = self.raw.get_mut(sheet).and_then(|r| r.get_mut(axis.slot())).ok_or_else(|| crate::kdm_err("no such sheet"))?;
        if slot.is_none() {
            *slot = Some(self.tx.list_items(&object, axis.field())?);
        }
        slot.as_mut().ok_or_else(|| crate::kdm_err("lines not read"))
    }

    /// Pads a sheet's lines down to `len` live ones (see `lines`: the padding objects' ids come
    /// from their place in the list).
    fn extend(&mut self, sheet: usize, axis: Axis, len: usize) -> KdmResult<()> {
        let Some(object) = self.sheets.get(sheet).map(|s| s.object.clone()) else { return Ok(()) };
        let mut raw = std::mem::take(self.raw_list(sheet, axis)?);
        let mut added = Vec::new();
        let have = self.sheets.get(sheet).map_or(0, |s| s.axis(axis).len());
        for index in have..len {
            let at = raw.len();
            let wanted = self
                .restored_lines
                .get(sheet)
                .and_then(|axes| axes.get(axis.slot()))
                .and_then(|ids| ids.get(index))
                .map(|id| ObjectId::from_string(id.clone()))
                .unwrap_or_else(|| pad_id(&object, axis, at));
            let fields = [("key".into(), json!(wanted.as_str()))].into_iter().collect();
            let id = match self.tx.create_with_id(axis.ty(), &wanted, fields) {
                Ok(id) => {
                    self.created.push(Created::Line { sheet: object.clone(), axis, id: id.clone() });
                    id
                }
                // That id is taken (the list was shortened by a repair): any id will do.
                Err(_) => {
                    let id = self.tx.create(axis.ty(), Map::new())?;
                    self.tx.set(&id, "key", json!(id.as_str()))?;
                    id
                }
            };
            self.tx.list_insert(&object, axis.field(), at, &id)?;
            raw.push(id.clone());
            added.push(id);
        }
        if !added.is_empty() {
            self.wrote = true;
        }
        *self.raw_list(sheet, axis)? = raw;
        if let Some(s) = self.sheets.get_mut(sheet) {
            s.lines_mut(axis).extend(added);
        }
        Ok(())
    }

    /// Applies sheet `j`'s structural edits to its lines (see the module docs). Answers whether
    /// any line moved.
    fn apply_edits(&mut self, j: usize, edits: &[&Edit]) -> KdmResult<bool> {
        let Some(object) = self.sheets.get(j).map(|s| s.object.clone()) else { return Ok(false) };
        let mut moved = false;
        for edit in edits {
            match edit {
                Edit::Insert { axis, at, count } => {
                    let axis = Axis::of(*axis);
                    let lines = self.sheets.get(j).map(|s| s.axis(axis).to_vec()).unwrap_or_default();
                    let Some(anchor) = lines.get(*at as usize).cloned() else { continue };
                    if *count > MAX_INSERT {
                        // Too many to insert one by one: the rest of the log is not applied either
                        // (the sheet is then written by position).
                        return Ok(true);
                    }
                    let mut raw = std::mem::take(self.raw_list(j, axis)?);
                    let pos = raw.iter().position(|o| *o == anchor).unwrap_or(raw.len());
                    let mut new = Vec::with_capacity(*count as usize);
                    for k in 0..*count as usize {
                        let id = self.tx.create(axis.ty(), Map::new())?;
                        self.tx.set(&id, "key", json!(id.as_str()))?;
                        self.tx.list_insert(&object, axis.field(), pos + k, &id)?;
                        raw.insert(pos + k, id.clone());
                        new.push(id);
                    }
                    *self.raw_list(j, axis)? = raw;
                    if let Some(s) = self.sheets.get_mut(j) {
                        let lines = s.lines_mut(axis);
                        let at = (*at as usize).min(lines.len());
                        lines.splice(at..at, new);
                    }
                    self.wrote = true;
                    moved = true;
                }
                Edit::Delete { axis, at, count } => {
                    let axis = Axis::of(*axis);
                    let len = self.sheets.get(j).map_or(0, |s| s.axis(axis).len());
                    let from = (*at as usize).min(len);
                    let to = (*at as usize).saturating_add(*count as usize).min(len);
                    if from >= to {
                        continue;
                    }
                    let gone: Vec<ObjectId> = match self.sheets.get_mut(j) {
                        Some(s) => s.lines_mut(axis).drain(from..to).collect(),
                        None => Vec::new(),
                    };
                    for line in gone {
                        if self.tx.exists(&line) {
                            self.tx.set(&line, "gone", Value::Bool(true))?;
                            self.set(&line, "index", Value::Null)?;
                        }
                    }
                    self.wrote = true;
                    moved = true;
                }
                // Moves and cell shifts move content, not lines: the cell diff writes them.
                _ => {}
            }
        }
        Ok(moved)
    }

    fn write(&mut self, before: Option<&Workbook>, after: &Workbook, edits: &[StructuralEdit], ids: &mut Ids) -> KdmResult<()> {
        let root = root_id();
        self.set(&root, "key", json!("workbook"))?;
        let facts = crate::axis_facts_id();
        if !self.tx.exists(&facts) {
            self.tx.create_with_id("AxisFacts", &facts, [("key".into(), json!("all"))].into_iter().collect())?;
            self.wrote = true;
        }
        self.set(&root, "axisFacts", json!(facts.as_str()))?;
        self.restoring = before.is_none();

        // The workbook's registers; the props name the default style when it is a file's own.
        self.styles = self.write_styles(before, after, &ids.styles)?;
        let props_changed = before.is_none_or(|b| {
            b.date_system != after.date_system
                || b.calc != after.calc
                || b.props != after.props
                || b.protected_structure != after.protected_structure
                || b.palette != after.palette
                || b.view != after.view
                || b.cell_style_info != after.cell_style_info
                || b.file_version != after.file_version
                || b.table_styles != after.table_styles
                || b.extensions != after.extensions
                || b.styles.get(StyleId::DEFAULT) != after.styles.get(StyleId::DEFAULT)
        });
        if props_changed {
            let own = *after.styles.get(StyleId::DEFAULT) != kition_sheet_model::Style::default();
            let v = workbook_props(after, self.styles.first().filter(|_| own).map(String::as_str))?;
            self.set(&root, "props", v)?;
        }
        if before.is_none_or(|b| b.theme != after.theme) {
            let v = to_json(&after.theme)?;
            self.set(&root, "theme", v)?;
        }
        if before.is_none_or(|b| b.custom_lists != after.custom_lists) {
            let v = to_json(&after.custom_lists)?;
            self.set(&root, "customLists", v)?;
        }
        if before.is_none_or(|b| b.cell_styles != after.cell_styles) {
            let v = to_json(&after.cell_styles)?;
            self.set(&root, "cellStyles", v)?;
        }

        // Sheets: pair them with the view's, create the new ones, delete the ones gone.
        let old = std::mem::take(&mut ids.sheets);
        let before_sheets: &[Arc<Sheet>] = before.map_or(&[], |b| b.sheets.as_slice());
        let consistent = before_sheets.len() == old.len();
        let partner = if consistent { pair_sheets(before_sheets, &after.sheets) } else { vec![None; after.sheets.len()] };
        let mut kept = vec![false; old.len()];
        let mut fresh = Vec::with_capacity(after.sheets.len());
        self.sheets = Vec::with_capacity(after.sheets.len());
        let sheet_ids = after.numbered_sheet_ids();
        for (j, sheet) in after.sheets.iter().enumerate() {
            let paired = partner.get(j).copied().flatten().and_then(|i| Some((i, old.get(i)?)));
            match paired {
                Some((i, sid)) if self.tx.exists(&sid.object) => {
                    if let Some(k) = kept.get_mut(i) {
                        *k = true;
                    }
                    self.sheets.push(sid.clone());
                    fresh.push(false);
                }
                _ => {
                    let wanted = if sheet.identities.object.is_empty() {
                        format!("sheet:{}", sheet_ids.get(j).copied().unwrap_or(0))
                    } else {
                        sheet.identities.object.clone()
                    };
                    let object = self.create_restored("Sheet", vec![("name", json!(sheet.name))], Some(&wanted))?;
                    self.wrote = true;
                    self.sheets.push(SheetIds::new(object));
                    fresh.push(true);
                }
            }
        }
        for (i, sid) in old.iter().enumerate() {
            if !kept.get(i).copied().unwrap_or(false) {
                self.delete_sheet(sid)?;
            }
        }
        self.names = after.sheets.iter().enumerate().map(|(j, s)| (s.name.to_ascii_lowercase(), j)).collect();
        self.raw = vec![[None, None]; after.sheets.len()];
        self.restored_lines = after
            .sheets
            .iter()
            .enumerate()
            .map(
                |(j, sheet)| {
                    if self.restoring && fresh.get(j).copied().unwrap_or(false) { sheet.identities.axes.clone() } else { Default::default() }
                },
            )
            .collect();
        for j in 0..after.sheets.len() {
            for axis in Axis::BOTH {
                let count = self.restored_lines.get(j).and_then(|axes| axes.get(axis.slot())).map_or(0, Vec::len).min(axis.limit() as usize);
                if count > 0 {
                    self.extend(j, axis, count)?;
                }
            }
            if self.restoring
                && let Some(sheet) = after.sheets.get(j)
            {
                let object = self.sheets.get(j).map(|s| s.object.clone()).ok_or_else(|| crate::kdm_err("no such sheet"))?;
                for axis in Axis::BOTH {
                    if let Some(gone) = sheet.identities.tombstones.get(axis.slot()) {
                        for (index, saved) in gone {
                            let id = ObjectId::from_string(saved.clone());
                            self.tx.create_with_id(
                                axis.ty(),
                                &id,
                                [("key".into(), json!(saved)), ("gone".into(), json!(true))].into_iter().collect(),
                            )?;
                            self.tx.list_insert(&object, axis.field(), *index, &id)?;
                        }
                    }
                }
                if let Some(raw) = self.raw.get_mut(j) {
                    *raw = [None, None];
                }
                for (address, identity) in &sheet.identities.cells {
                    let at = CellRef::parse(address).ok_or_else(|| crate::kdm_err("invalid cell identity address"))?;
                    let key = self.key_of(j, at, true)?;
                    let fields = [
                        ("by".into(), identity.by.clone().map_or(Value::Null, Value::String)),
                        ("key".into(), identity.key.clone().map_or(Value::Null, Value::String)),
                    ]
                    .into_iter()
                    .filter(|(_, value)| !value.is_null())
                    .collect();
                    let id = self.tx.create_with_id(cells::CELL, &ObjectId::from_string(identity.object.clone()), fields)?;
                    self.tx.map_set(&object, "cells", &key, json!(id.as_str()))?;
                }
            }
        }

        // Structural edits first, so the content below is keyed by the lines it moved with.
        let mut restructured = vec![false; after.sheets.len()];
        for (j, slot) in restructured.iter_mut().enumerate() {
            if fresh.get(j).copied().unwrap_or(true) {
                continue;
            }
            let mine: Vec<&Edit> = edits.iter().filter(|e| e.sheet == j).map(|e| &e.edit).collect();
            if !mine.is_empty() {
                *slot = self.apply_edits(j, &mine)?;
            }
        }

        // Each sheet's content, with its id (as the workbook numbers it, for a view not numbered yet).
        for (j, sheet) in after.sheets.iter().enumerate() {
            let is_fresh = fresh.get(j).copied().unwrap_or(true);
            let was = if is_fresh { None } else { partner.get(j).copied().flatten().and_then(|i| Some((before_sheets.get(i)?, old.get(i)?))) };
            let restructured = restructured.get(j).copied().unwrap_or(false);
            self.write_sheet(j, was, sheet, sheet_ids.get(j).copied().unwrap_or(0), is_fresh, restructured)?;
            if before.is_none_or(|wb| {
                partner
                    .get(j)
                    .copied()
                    .flatten()
                    .and_then(|i| wb.sheets.get(i))
                    .is_none_or(|old| old.tables != sheet.tables || old.pivots != sheet.pivots || old.shapes != sheet.shapes)
            }) {
                let object = self.sheets.get(j).map(|ids| ids.object.clone()).ok_or_else(|| crate::kdm_err("missing feature owner"))?;
                self.wrote |= crate::feature_ids::sheet(self.tx, &object, sheet)?;
            }
            for axis in Axis::BOTH {
                let lines = self.sheets.get(j).map(|s| s.axis(axis).to_vec()).unwrap_or_default();
                for (index, id) in lines.iter().enumerate() {
                    if self.tx.get(id, "index")?.is_null() {
                        self.set(id, "index", json!(index))?;
                    }
                }
            }
        }

        // Names, then the sheets' order.
        let same_sheets = consistent && old.len() == self.sheets.len() && old.iter().zip(&self.sheets).all(|(a, b)| a.object == b.object);
        let names = if same_sheets && before.is_some_and(|b| b.names == after.names) && ids.names.len() == after.names.len() {
            std::mem::take(&mut ids.names)
        } else {
            let old_names = std::mem::take(&mut ids.names);
            self.write_names(before, after, &old, old_names)?
        };
        let order: Vec<ObjectId> = self.sheets.iter().map(|s| s.object.clone()).collect();
        self.wrote |= sync_list(self.tx, &root, "sheets", &order)? > 0;
        if before.is_none_or(|wb| {
            wb.styles != after.styles
                || wb.cell_styles != after.cell_styles
                || wb.cell_style_info != after.cell_style_info
                || wb.table_styles != after.table_styles
        }) {
            self.wrote |= crate::feature_ids::styles(self.tx)?;
        }

        *ids = Ids { sheets: std::mem::take(&mut self.sheets), names, styles: std::mem::take(&mut self.styles) };
        Ok(())
    }

    /// Every style of `after`'s table in the `styles` map (`{order, style}` under its key), the
    /// ones the replica lacks written; answers each style's key. Styles are never removed: a cell
    /// written concurrently may still name one.
    fn write_styles(&mut self, before: Option<&Workbook>, after: &Workbook, old: &[String]) -> KdmResult<Vec<String>> {
        let root = root_id();
        let mut keys = Vec::with_capacity(after.styles.len());
        for (id, style) in after.styles.iter() {
            let k = id.0 as usize;
            let reuse = before.filter(|b| k < b.styles.len() && b.styles.get(id) == style).and_then(|_| old.get(k)).cloned();
            let key = match reuse {
                Some(key) => key,
                None => {
                    let v = to_json(style)?;
                    let key = style_key(&v);
                    if self.tx.map_get(&root, "styles", &key)?.is_null() {
                        self.tx.map_set(&root, "styles", &key, json!({ "order": id.0, "style": v }))?;
                        self.wrote = true;
                    }
                    key
                }
            };
            keys.push(key);
        }
        Ok(keys)
    }

    /// Deletes a sheet's object and its lines, images and shapes.
    fn delete_sheet(&mut self, sid: &SheetIds) -> KdmResult<()> {
        if !self.tx.exists(&sid.object) {
            return Ok(());
        }
        for field in ["rows", "cols", "images"] {
            for item in self.tx.list_items(&sid.object, field)? {
                self.delete(&item)?;
            }
        }
        for object in sid.shapes.iter().chain(&sid.charts) {
            self.delete(object)?;
        }
        self.delete(&sid.object)
    }

    /// The key of cell `c` of sheet `j`; `materialise`: create its row and column objects if
    /// they have none yet.
    fn key_of(&mut self, j: usize, c: CellRef, materialise: bool) -> KdmResult<String> {
        if materialise {
            let row = self.line(j, Axis::Rows, c.row, Use::Content)?;
            let col = self.line(j, Axis::Cols, c.col, Use::Content)?;
            return Ok(cell_key(&row, &col));
        }
        let lines = self.sheets.get(j);
        let row = lines.and_then(|s| s.rows.get(c.row as usize)).map_or_else(|| format!("#{}", c.row), |o| o.as_str().to_owned());
        let col = lines.and_then(|s| s.cols.get(c.col as usize)).map_or_else(|| format!("#{}", c.col), |o| o.as_str().to_owned());
        Ok(cell_key(&row, &col))
    }

    fn picture_value(&mut self, p: &Arc<CellPicture>) -> KdmResult<Value> {
        if let Some(v) = self.pictures.get(&Arc::as_ptr(p)) {
            return Ok(v.clone());
        }
        let blob = to_json(&self.blobs.put(&p.data, &p.mime))?;
        let v = json!({ "blob": blob, "mime": p.mime, "alt": p.alt });
        self.pictures.insert(Arc::as_ptr(p), v.clone());
        Ok(v)
    }

    /// What a cell stores (`None`: nothing to store; see `cells`).
    fn cell_stored(&mut self, j: usize, sheet: &Sheet, c: CellRef) -> KdmResult<Option<Stored>> {
        // A picture whose cell holds nothing else is still stored (with an empty input).
        let empty = Cell::default();
        let cell = match sheet.cells.get(c) {
            Some(cell) => cell,
            None if sheet.cell_pictures.contains_key(&c) => &empty,
            None => return Ok(None),
        };
        let mut stored = Stored::from_input(&encode_input(self, cell, j)?);
        if !stored.text.is_empty() {
            stored.runs = crate::marks::cell_runs(Some(cell));
        }
        if cell.style != StyleId::DEFAULT {
            stored.style = self.style_key(cell.style);
        }
        if let Some(range) = cell.formula.as_ref().and_then(|f| f.array) {
            stored.array = Some(encode_range(self, j, range, Use::Content)?);
        }
        if let Some(p) = sheet.cell_pictures.get(&c) {
            stored.picture = Some(self.picture_value(p)?);
        }
        Ok((!stored.is_empty()).then_some(stored))
    }

    /// Writes cell `key` of sheet `j` (whose object is `object`) as `stored`. `current` is the
    /// map's value for `key`; `view_old` the text (and runs) the view's cell before this write
    /// wrote (`None`: unknown). An existing `Cell` keeps its object: its registers are set where
    /// they differ and its text is diffed by character (`kition_kit::session::write_text`, or
    /// `write_rich_text` with the runs' marks when either side has runs), so concurrent typing and
    /// formatting in the cell merge. Where several objects hold the cell's text (a race on one
    /// new cell, see `cells`), the parts' characters are written and the marks they hold stay.
    fn put_cell(&mut self, j: usize, object: &ObjectId, key: &str, current: &Value, stored: &Stored, view_old: Option<ViewText>) -> KdmResult<()> {
        let Some(id) = cells::object_of(self.tx, current) else {
            return self.create_cell(object, key, stored);
        };
        let text_of = |v: &Option<String>| v.clone().map_or(Value::Null, Value::String);
        self.set(&id, "input", text_of(&stored.input))?;
        self.set(&id, "style", text_of(&stored.style))?;
        self.set(&id, "array", text_of(&stored.array))?;
        self.set(&id, "picture", stored.picture.clone().unwrap_or(Value::Null))?;
        // Several objects' texts make up the cell's (a race on one new cell, see `cells`): each
        // object's part is written on its own.
        if let Some(parts) = self.sheets.get(j).and_then(|s| s.parts.get(&id)).cloned() {
            let olds: Vec<String> = parts.iter().map(|(_, text)| text.clone()).collect();
            let news = cells::split_parts(&olds, &stored.text);
            let mut written = Vec::with_capacity(parts.len());
            for ((object, old), new) in parts.into_iter().zip(news) {
                if self.tx.exists(&object) {
                    let len = self.tx.text_len(&object, cells::TEXT)?;
                    if old.chars().count() == len {
                        if old != new {
                            write_text(self.tx, &object, cells::TEXT, &old, &new).map_err(crate::kdm_err)?;
                            self.wrote = true;
                        }
                    } else {
                        // Not known (it should be): the part is replaced.
                        self.tx.text_delete(&object, cells::TEXT, 0, len)?;
                        self.tx.text_insert(&object, cells::TEXT, 0, &new)?;
                        self.wrote = true;
                    }
                }
                written.push((object, new));
            }
            if let Some(sheet) = self.sheets.get_mut(j) {
                sheet.texts.remove(&id);
                sheet.parts.insert(id, written);
            }
            return Ok(());
        }
        // The field's current text: what the reader saw differ from the view (its marks not
        // known: taken as none), or the view's.
        let odd = self.sheets.get_mut(j).and_then(|s| s.texts.remove(&id));
        let old = match odd {
            Some(text) => Some(ViewText { text, runs: Vec::new() }),
            None => view_old,
        };
        let len = self.tx.text_len(&id, cells::TEXT)?;
        match old.filter(|old| old.text.chars().count() == len) {
            Some(old) if old.runs.is_empty() && stored.runs.is_empty() => {
                if old.text != stored.text {
                    write_text(self.tx, &id, cells::TEXT, &old.text, &stored.text).map_err(crate::kdm_err)?;
                    self.wrote = true;
                }
            }
            Some(old) => {
                let as_runs = |text: &str, runs: &[TextRun]| if runs.is_empty() { vec![TextRun::new(text)] } else { runs.to_vec() };
                let (olds, news) = (as_runs(&old.text, &old.runs), as_runs(&stored.text, &stored.runs));
                if olds != news {
                    write_rich_text(self.tx, &id, cells::TEXT, &olds, &news).map_err(crate::kdm_err)?;
                    self.wrote = true;
                }
            }
            // Not known (it should be): the whole text is replaced.
            None => {
                if len > 0 {
                    self.tx.text_delete(&id, cells::TEXT, 0, len)?;
                    self.wrote = true;
                }
                if !stored.text.is_empty() {
                    self.tx.text_insert(&id, cells::TEXT, 0, &stored.text)?;
                    cells::mark(self.tx, &id, &stored.runs)?;
                    self.wrote = true;
                }
            }
        }
        Ok(())
    }

    /// Sheet `j`'s live line tokens, as written so far.
    fn live_tokens(&self, j: usize) -> LiveTokens {
        let set = |lines: &[ObjectId]| lines.iter().map(|o| o.as_str().to_owned()).collect();
        match self.sheets.get(j) {
            Some(s) => LiveTokens { rows: set(&s.rows), cols: set(&s.cols) },
            None => LiveTokens::default(),
        }
    }

    /// Creates a cell's `Cell` object (see [`cells::create`]), logging a derived id.
    fn create_cell(&mut self, sheet: &ObjectId, key: &str, stored: &Stored) -> KdmResult<()> {
        let (id, derived) = cells::create(self.tx, sheet, key, stored)?;
        if derived {
            self.created.push(Created::Cell { sheet: sheet.clone(), key: key.to_owned(), id });
        }
        self.wrote = true;
        Ok(())
    }

    /// Writes a sheet's `cells` map. `full`: `entries` are every cell the sheet holds, and the
    /// rest go; otherwise `entries` are the positions that changed (`None`: no cell any more).
    /// `fresh`: the map is new (empty). `was`: the sheet and its objects before this write.
    fn write_cells(
        &mut self,
        j: usize,
        full: bool,
        fresh: bool,
        entries: Vec<(CellRef, Option<Stored>)>,
        was: Option<(&Sheet, &SheetIds)>,
    ) -> KdmResult<()> {
        let Some(object) = self.sheets.get(j).map(|s| s.object.clone()) else { return Ok(()) };
        if full {
            let mut new: HashMap<String, Stored> = HashMap::with_capacity(entries.len());
            for (c, v) in entries {
                if let Some(v) = v {
                    new.insert(self.key_of(j, c, true)?, v);
                }
            }
            if !fresh {
                let before = was.map(|(sheet, sid)| (sheet, BeforeLines::new(sid)));
                let mut live: Option<LiveTokens> = None;
                for (key, current) in self.tx.map_entries(&object, "cells")? {
                    match new.remove(&key) {
                        Some(stored) => {
                            let view_old =
                                before.as_ref().and_then(|(sheet, lines)| lines.position(&key).map(|c| cells::view_text(sheet.cells.get(c))));
                            self.put_cell(j, &object, &key, &current, &stored, view_old)?;
                        }
                        // A cell at live lines is emptied, not removed (its object stays, so a
                        // collaborator typing into it keeps their edit, see `cells`); one whose
                        // row or column is gone goes with its object.
                        None if live.get_or_insert_with(|| self.live_tokens(j)).holds(&key) => {
                            self.wrote |= cells::clear(self.tx, &object, &key)?;
                        }
                        None => {
                            cells::remove(self.tx, &object, &key)?;
                            self.wrote = true;
                        }
                    }
                }
            }
            let mut rest: Vec<(String, Stored)> = new.into_iter().collect();
            rest.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, stored) in rest {
                self.create_cell(&object, &key, &stored)?;
            }
            return Ok(());
        }
        for (c, v) in entries {
            match v {
                Some(stored) => {
                    let key = self.key_of(j, c, true)?;
                    let current = self.tx.map_get(&object, "cells", &key)?;
                    let view_old = was.map(|(sheet, _)| cells::view_text(sheet.cells.get(c)));
                    self.put_cell(j, &object, &key, &current, &stored, view_old)?;
                }
                None => {
                    let key = self.key_of(j, c, false)?;
                    // The entry may also be under the position's index (a cell below the
                    // materialised part when it was written).
                    let by_index = cell_key(&format!("#{}", c.row), &format!("#{}", c.col));
                    // Emptied, not removed: typing into it at the same time survives (`cells`).
                    for key in [key.clone(), by_index] {
                        self.wrote |= cells::clear(self.tx, &object, &key)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Writes one of a sheet's maps keyed by cell. `full`: `entries` are every entry the map
    /// should hold, and whatever else it holds goes; otherwise `entries` are the positions that
    /// changed (`None`: no entry any more). `fresh`: the map is new (empty).
    fn write_map(&mut self, j: usize, field: &str, full: bool, fresh: bool, entries: Vec<(CellRef, Option<Value>)>) -> KdmResult<()> {
        let Some(object) = self.sheets.get(j).map(|s| s.object.clone()) else { return Ok(()) };
        if full {
            let mut new: HashMap<String, Value> = HashMap::with_capacity(entries.len());
            for (c, v) in entries {
                if let Some(v) = v {
                    new.insert(self.key_of(j, c, true)?, v);
                }
            }
            if !fresh {
                for (key, stored) in self.tx.map_entries(&object, field)? {
                    match new.remove(&key) {
                        Some(v) => {
                            if !same_json(&stored, &v) {
                                self.tx.map_set(&object, field, &key, v)?;
                                self.wrote = true;
                            }
                        }
                        None => {
                            self.tx.map_delete(&object, field, &key)?;
                            self.wrote = true;
                        }
                    }
                }
            }
            let mut rest: Vec<(String, Value)> = new.into_iter().collect();
            rest.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, v) in rest {
                self.tx.map_set(&object, field, &key, v)?;
                self.wrote = true;
            }
            return Ok(());
        }
        for (c, v) in entries {
            match v {
                Some(v) => {
                    let key = self.key_of(j, c, true)?;
                    if !same_json(&self.tx.map_get(&object, field, &key)?, &v) {
                        self.tx.map_set(&object, field, &key, v)?;
                        self.wrote = true;
                    }
                }
                None => {
                    let key = self.key_of(j, c, false)?;
                    // The entry may also be under the position's index (a cell below the
                    // materialised part when it was written).
                    let by_index = cell_key(&format!("#{}", c.row), &format!("#{}", c.col));
                    for key in [key.clone(), by_index] {
                        if !self.tx.map_get(&object, field, &key)?.is_null() {
                            self.tx.map_delete(&object, field, &key)?;
                            self.wrote = true;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Writes sheet `j` (`after`) over its view `was` (its sheet and objects before this
    /// command; `None`: a new sheet).
    fn write_sheet(
        &mut self,
        j: usize,
        was: Option<(&Arc<Sheet>, &SheetIds)>,
        after: &Arc<Sheet>,
        sheet_id: u32,
        fresh: bool,
        restructured: bool,
    ) -> KdmResult<()> {
        if let Some((b, _)) = was
            && Arc::ptr_eq(b, after)
            && !restructured
        {
            return Ok(());
        }
        let Some(object) = self.sheets.get(j).map(|s| s.object.clone()) else { return Ok(()) };
        let before: Option<&Sheet> = was.map(|(b, _)| b.as_ref());
        let full = before.is_none() || restructured;
        self.set(&object, "name", json!(after.name))?;
        // The sheet's id, read-only for agents: its tool id `sheet:<id>` (KitObjectModel OM9).
        if sheet_id != 0 {
            self.set(&object, "sheetId", json!(sheet_id))?;
        }

        // Line info (rows and columns below the materialised part go to `props`).
        let mut far: [Vec<(u32, Value)>; 2] = [Vec::new(), Vec::new()];
        for axis in Axis::BOTH {
            let (b_lines, a_lines) = match axis {
                Axis::Rows => (before.map(|b| &b.rows), &after.rows),
                Axis::Cols => (before.map(|b| &b.cols), &after.cols),
            };
            if !full && b_lines == Some(a_lines) {
                // Unchanged: only the info kept by index is wanted (for `props`).
                let live = self.sheets.get(j).map_or(0, |s| s.axis(axis).len());
                let threshold = u32::try_from(live).unwrap_or(u32::MAX).max(axis.limit());
                for (&i, info) in a_lines.range(threshold..) {
                    let v = line_info(self, info)?;
                    if let Some(slot) = far.get_mut(axis.slot()) {
                        slot.push((i, v));
                    }
                }
                continue;
            }
            let mut new: HashMap<ObjectId, Value> = HashMap::new();
            for (&i, info) in a_lines {
                let v = line_info(self, info)?;
                let token = self.line(j, axis, i, Use::Content)?;
                if token.starts_with('#') {
                    if let Some(slot) = far.get_mut(axis.slot()) {
                        slot.push((i, v));
                    }
                } else {
                    new.insert(ObjectId::from_string(token), v);
                }
            }
            // Lines that had info: before this command (by the view's objects) and now.
            let mut had: HashSet<ObjectId> = HashSet::new();
            if let (Some(b_lines), Some((_, sid))) = (b_lines, was) {
                for &i in b_lines.keys() {
                    if let Some(o) = sid.axis(axis).get(i as usize) {
                        had.insert(o.clone());
                    }
                }
            }
            let mut set: Vec<(ObjectId, Value)> = new.into_iter().collect();
            set.sort_by(|a, b| a.0.cmp(&b.0));
            for (line, v) in set {
                had.remove(&line);
                self.set(&line, "info", v)?;
            }
            let mut cleared: Vec<ObjectId> = had.into_iter().collect();
            cleared.sort();
            for line in cleared {
                // A deleted line's info is left as it is: nothing reads it.
                if self.tx.exists(&line) && self.tx.get(&line, "gone")? != Value::Bool(true) {
                    self.set(&line, "info", Value::Null)?;
                }
            }
        }

        // Cells.
        let positions: Vec<CellRef> = if full {
            let mut p: Vec<CellRef> = after.cells.iter().map(|(c, _)| c).collect();
            p.extend(after.cell_pictures.keys().filter(|c| after.cells.get(**c).is_none()).copied());
            p
        } else {
            let mut p: Vec<CellRef> = match before {
                // A formula cell whose value alone changed (a recalculation) stores the same.
                Some(b) => b.cells.diff(&after.cells).into_iter().filter(|c| !same_stored(b.cells.get(*c), after.cells.get(*c))).collect(),
                None => Vec::new(),
            };
            if let Some(b) = before
                && b.cell_pictures != after.cell_pictures
            {
                let keys: HashSet<CellRef> = b.cell_pictures.keys().chain(after.cell_pictures.keys()).copied().collect();
                p.extend(keys.into_iter().filter(|c| b.cell_pictures.get(c) != after.cell_pictures.get(c)));
            }
            p.sort_unstable();
            p.dedup();
            p
        };
        let mut entries = Vec::with_capacity(positions.len());
        for c in &positions {
            entries.push((*c, self.cell_stored(j, after, *c)?));
        }
        let was_cells = was.map(|(b, sid)| (b.as_ref(), sid));
        self.write_cells(j, full, fresh && (!self.restoring || after.identities.cells.is_empty()), entries, was_cells)?;
        let addressed: Vec<_> = if full || before.is_some_and(|b| b.name != after.name) {
            let lines = self.sheets.get(j).map(BeforeLines::new);
            self.tx
                .map_entries(&object, "cells")?
                .into_iter()
                .filter_map(|(key, value)| Some((lines.as_ref()?.position(&key)?, cells::object_of(self.tx, &value)?)))
                .collect()
        } else {
            let mut addressed = Vec::with_capacity(positions.len());
            for at in positions {
                let key = self.key_of(j, at, false)?;
                if let Some(id) = cells::object_of(self.tx, &self.tx.map_get(&object, "cells", &key)?) {
                    addressed.push((at, id));
                }
            }
            addressed
        };
        for (at, id) in addressed {
            if self.tx.get(&id, "address")?.is_null() {
                self.set(&id, "address", json!(format!("{sheet_id}!{at}")))?;
                self.set(&id, "label", json!(format!("sheet:{sheet_id}!{at}")))?;
            }
        }

        // Comments and links.
        for (field, changed) in [
            ("comments", before.is_none_or(|b| b.comments != after.comments)),
            ("hyperlinks", before.is_none_or(|b| b.hyperlinks != after.hyperlinks)),
        ] {
            if !full && !changed {
                continue;
            }
            let mut entries = Vec::new();
            let value_at = |c: &CellRef| -> KdmResult<Option<Value>> {
                Ok(match field {
                    "comments" => after.comments.get(c).map(to_json).transpose()?,
                    _ => after.hyperlinks.get(c).map(to_json).transpose()?,
                })
            };
            if full {
                let keys: Vec<CellRef> =
                    if field == "comments" { after.comments.keys().copied().collect() } else { after.hyperlinks.keys().copied().collect() };
                for c in keys {
                    entries.push((c, value_at(&c)?));
                }
            } else if let Some(b) = before {
                let mut keys: Vec<CellRef> = if field == "comments" {
                    let all: HashSet<CellRef> = b.comments.keys().chain(after.comments.keys()).copied().collect();
                    all.into_iter().filter(|c| b.comments.get(c) != after.comments.get(c)).collect()
                } else {
                    let all: HashSet<CellRef> = b.hyperlinks.keys().chain(after.hyperlinks.keys()).copied().collect();
                    all.into_iter().filter(|c| b.hyperlinks.get(c) != after.hyperlinks.get(c)).collect()
                };
                keys.sort_unstable();
                for c in keys {
                    entries.push((c, value_at(&c)?));
                }
            }
            self.write_map(j, field, full, fresh, entries)?;
        }

        // Props and features, after the cells (whose rows and columns now exist).
        let props_changed = before.is_none_or(|b| {
            full || b.default_col_width != after.default_col_width
                || b.default_row_height != after.default_row_height
                || b.freeze != after.freeze
                || b.tab_color != after.tab_color
                || b.visibility != after.visibility
                || b.show_gridlines != after.show_gridlines
                || b.grid_color != after.grid_color
                || b.show_headings != after.show_headings
                || b.show_formulas != after.show_formulas
                || b.show_zeros != after.show_zeros
                || b.right_to_left != after.right_to_left
                || b.outline_levels != after.outline_levels
                || b.protection != after.protection
                || b.print != after.print
                || b.rows != after.rows
                || b.cols != after.cols
        });
        if props_changed {
            let [far_rows, far_cols] = &far;
            let v = sheet_props(self, j, after, [far_rows.as_slice(), far_cols.as_slice()])?;
            self.set(&object, "props", v)?;
        }
        let features_changed = before.is_none_or(|b| {
            full || b.merges != after.merges
                || b.cond_formats != after.cond_formats
                || b.validations != after.validations
                || b.tables != after.tables
                || b.autofilter != after.autofilter
                || b.charts != after.charts
                || b.sparklines != after.sparklines
                || b.pivots != after.pivots
        });
        if features_changed {
            let v = sheet_features(self, j, after)?;
            self.set(&object, "features", v)?;
            self.wrote |= crate::constraints::write(self.tx, &object, after)?;
        }

        // Images: the same object while the image's id is the same.
        let images = self.write_images(j, was, after, restructured)?;
        if let Some(s) = self.sheets.get_mut(j) {
            s.images = images;
        }
        // Shapes, likewise.
        let shapes = self.write_shapes(j, was, after, restructured)?;
        self.write_drawing_ownership(&object, "shapes", &shapes)?;
        if let Some(s) = self.sheets.get_mut(j) {
            s.shapes = shapes;
        }
        // Charts, likewise.
        let charts = self.write_charts(j, was, after, restructured)?;
        self.write_drawing_ownership(&object, "charts", &charts)?;
        if let Some(s) = self.sheets.get_mut(j) {
            s.charts = charts;
        }
        Ok(())
    }

    fn write_drawing_ownership(&mut self, sheet: &ObjectId, field: &str, objects: &[ObjectId]) -> KdmResult<()> {
        let mut wanted = Vec::with_capacity(objects.len());
        for id in objects {
            let key = self
                .tx
                .get(id, "key")?
                .as_u64()
                .map(|key| format!("drawing:{key}"))
                .ok_or_else(|| crate::kdm_err("drawing ownership needs its persistent key"))?;
            wanted.push((key, json!(id.as_str())));
        }
        let current = self.tx.map_entries(sheet, field)?;
        for (key, _) in &current {
            if !wanted.iter().any(|(name, _)| name == key) {
                self.tx.map_delete(sheet, field, key)?;
                self.wrote = true;
            }
        }
        for (key, value) in wanted {
            if !current.iter().any(|(name, v)| name == &key && v == &value) {
                match self.tx.map_set(sheet, field, &key, value) {
                    // SDK 34dff06 cannot add a container to an existing object. Old replicas
                    // still discover drawings by their sheet ref; preserve that migration.
                    Err(kition_kit::kdm::KdmError::NotFound(path)) if self.tx.min_schema_version() < 6 && path == format!("{sheet}.{field}") => {
                        return Ok(());
                    }
                    result => result?,
                }
                self.wrote = true;
            }
        }
        Ok(())
    }

    /// Writes sheet `j`'s charts as `Chart` objects: the same object while the chart's id is the
    /// same, its anchor and each member written only where it differs. `restructured`: lines moved,
    /// so every chart's ranges are encoded again.
    fn write_charts(&mut self, j: usize, was: Option<(&Arc<Sheet>, &SheetIds)>, after: &Sheet, restructured: bool) -> KdmResult<Vec<ObjectId>> {
        let Some(sheet) = self.sheets.get(j).map(|s| s.object.clone()) else { return Ok(Vec::new()) };
        let (b_charts, old): (&[kition_sheet_model::Chart], &[ObjectId]) = match was {
            Some((b, sid)) if b.charts.len() == sid.charts.len() => (&b.charts, &sid.charts),
            _ => (&[], &[]),
        };
        if !restructured && b_charts == after.charts.as_slice() && old.len() == after.charts.len() {
            return Ok(old.to_vec());
        }
        let mut used = vec![false; old.len()];
        let mut out = Vec::with_capacity(after.charts.len());
        for (place, chart) in after.charts.iter().enumerate() {
            let found = b_charts.iter().enumerate().position(|(i, b)| !used.get(i).copied().unwrap_or(true) && b.id == chart.id);
            let frame = crate::charts::frame(self, j, chart)?;
            let parts = crate::charts::parts(self, j, chart)?;
            match found.and_then(|i| Some((i, old.get(i)?.clone()))) {
                Some((i, existing)) if self.tx.exists(&existing) => {
                    if let Some(u) = used.get_mut(i) {
                        *u = true;
                    }
                    self.set(&existing, "frame", frame)?;
                    self.set(&existing, "key", drawing_key(chart.id))?;
                    if i != place {
                        self.set(&existing, "z", json!(place))?;
                    }
                    self.wrote |= crate::charts::write_parts(self.tx, &existing, &parts)?;
                    out.push(existing);
                }
                _ => {
                    let created = self.create_restored(
                        crate::charts::CHART,
                        vec![("sheet", json!(sheet.as_str())), ("z", json!(place)), ("frame", frame), ("key", drawing_key(chart.id))],
                        after.identities.drawings.get(&chart.id),
                    )?;
                    crate::charts::write_parts(self.tx, &created, &parts)?;
                    // A version 3 reader would find no charts in the sheet's `features`.
                    self.tx.raise_schema_version(crate::charts::CHART_OBJECTS)?;
                    out.push(created);
                }
            }
        }
        for (i, o) in old.iter().enumerate() {
            if !used.get(i).copied().unwrap_or(false) {
                self.delete(o)?;
            }
        }
        Ok(out)
    }

    /// Writes sheet `j`'s shapes as `Shape` objects: the same object while the shape's id is the
    /// same, each field written only where it differs (the text by its changed middle, the stacking
    /// order only where a shape's place changed).
    fn write_shapes(&mut self, j: usize, was: Option<(&Arc<Sheet>, &SheetIds)>, after: &Sheet, restructured: bool) -> KdmResult<Vec<ObjectId>> {
        let Some(sheet) = self.sheets.get(j).map(|s| s.object.clone()) else { return Ok(Vec::new()) };
        let (b_shapes, old): (&[kition_sheet_model::Shape], &[ObjectId]) = match was {
            Some((b, sid)) if b.shapes.len() == sid.shapes.len() => (&b.shapes, &sid.shapes),
            _ => (&[], &[]),
        };
        if !restructured && b_shapes == after.shapes.as_slice() && old.len() == after.shapes.len() {
            return Ok(old.to_vec());
        }
        let mut used = vec![false; old.len()];
        let mut out = Vec::with_capacity(after.shapes.len());
        for (place, shape) in after.shapes.iter().enumerate() {
            let found = b_shapes.iter().enumerate().position(|(i, b)| !used.get(i).copied().unwrap_or(true) && b.id == shape.id);
            let frame = crate::shapes::frame(self, j, shape)?;
            let props = crate::shapes::props(shape)?;
            match found.and_then(|i| Some((i, old.get(i)?.clone(), b_shapes.get(i)?))) {
                Some((i, existing, b)) if self.tx.exists(&existing) => {
                    if let Some(u) = used.get_mut(i) {
                        *u = true;
                    }
                    self.set(&existing, "frame", frame)?;
                    self.set(&existing, "key", drawing_key(shape.id))?;
                    let was = crate::shapes::props(b)?;
                    self.wrote |= crate::shapes::write_look(self.tx, &existing, Some(&was), &props)?;
                    if i != place {
                        self.set(&existing, "z", json!(place))?;
                    }
                    // The view's text is the replica's as last read or written.
                    self.wrote |= crate::shapes::write_text(self.tx, &existing, &b.text, &shape.text)?;
                    out.push(existing);
                }
                _ => {
                    let created = self.create_restored(
                        crate::shapes::SHAPE,
                        vec![("sheet", json!(sheet.as_str())), ("z", json!(place)), ("frame", frame), ("key", drawing_key(shape.id))],
                        after.identities.drawings.get(&shape.id),
                    )?;
                    crate::shapes::write_look(self.tx, &created, None, &props)?;
                    crate::shapes::write_text(self.tx, &created, "", &shape.text)?;
                    out.push(created);
                }
            }
        }
        for (i, o) in old.iter().enumerate() {
            if !used.get(i).copied().unwrap_or(false) {
                self.delete(o)?;
            }
        }
        Ok(out)
    }

    /// Writes sheet `j`'s images. `restructured`: lines moved, so an unchanged image may name
    /// other lines than the view's (its anchor's line deleted, or past the lines it was written
    /// over): every image's props are encoded again and written where they differ.
    fn write_images(&mut self, j: usize, was: Option<(&Arc<Sheet>, &SheetIds)>, after: &Sheet, restructured: bool) -> KdmResult<Vec<ObjectId>> {
        let Some(object) = self.sheets.get(j).map(|s| s.object.clone()) else { return Ok(Vec::new()) };
        let (b_images, old): (&[kition_sheet_model::Image], &[ObjectId]) = match was {
            Some((b, sid)) if b.images.len() == sid.images.len() => (&b.images, &sid.images),
            _ => (&[], &[]),
        };
        if !restructured && !b_images.is_empty() && b_images == after.images.as_slice() {
            return Ok(old.to_vec());
        }
        let mut used = vec![false; old.len()];
        let mut out = Vec::with_capacity(after.images.len());
        for img in &after.images {
            let found = b_images.iter().enumerate().position(|(i, b)| !used.get(i).copied().unwrap_or(true) && b.id == img.id);
            let props = image_props(self, j, img)?;
            match found.and_then(|i| Some((i, old.get(i)?.clone(), b_images.get(i)?))) {
                Some((i, existing, b)) if self.tx.exists(&existing) => {
                    if let Some(u) = used.get_mut(i) {
                        *u = true;
                    }
                    self.set(&existing, "props", props)?;
                    self.set(&existing, "key", drawing_key(img.id))?;
                    if b.data != img.data || b.mime != img.mime {
                        let blob = to_json(&self.blobs.put(&img.data, &img.mime))?;
                        self.set(&existing, "blob", blob)?;
                    }
                    out.push(existing);
                }
                _ => {
                    let blob = to_json(&self.blobs.put(&img.data, &img.mime))?;
                    out.push(self.create_restored(
                        "Image",
                        vec![("props", props), ("blob", blob), ("key", drawing_key(img.id))],
                        after.identities.drawings.get(&img.id),
                    )?);
                }
            }
        }
        for (i, o) in old.iter().enumerate() {
            if !used.get(i).copied().unwrap_or(false) {
                self.delete(o)?;
            }
        }
        self.wrote |= sync_list(self.tx, &object, "images", &out)? > 0;
        Ok(out)
    }

    /// Writes the defined names: the same object while a name keeps its name and scope.
    fn write_names(&mut self, before: Option<&Workbook>, after: &Workbook, old_sheets: &[SheetIds], old: Vec<ObjectId>) -> KdmResult<Vec<ObjectId>> {
        let root = root_id();
        let before_names = before.map_or(&[][..], |b| b.names.as_slice());
        let consistent = before_names.len() == old.len();
        let scope_object = |scope: Option<usize>, sheets: &[SheetIds]| scope.and_then(|s| sheets.get(s)).map(|s| s.object.clone());
        let mut used = vec![false; old.len()];
        let mut out = Vec::with_capacity(after.names.len());
        for (index, n) in after.names.iter().enumerate() {
            let scope = scope_object(n.scope, &self.sheets);
            let found = if consistent {
                before_names.iter().enumerate().position(|(i, b)| {
                    !used.get(i).copied().unwrap_or(true) && b.name.eq_ignore_ascii_case(&n.name) && scope_object(b.scope, old_sheets) == scope
                })
            } else {
                None
            };
            let formula = encode_formula(self, &n.formula, None, None)?;
            let fields = |formula: &str, scope: &Option<ObjectId>| {
                vec![
                    ("name", json!(n.name)),
                    ("formula", json!(formula)),
                    ("scope", scope.as_ref().map_or(Value::Null, |s| Value::String(s.as_str().to_owned()))),
                    ("props", name_props(n)),
                ]
            };
            match found.and_then(|i| Some((i, old.get(i)?.clone()))) {
                Some((i, object)) if self.tx.exists(&object) => {
                    if let Some(u) = used.get_mut(i) {
                        *u = true;
                    }
                    for (field, v) in fields(&formula, &scope) {
                        self.set(&object, field, v)?;
                    }
                    out.push(object);
                }
                _ => {
                    let pairs = fields(&formula, &scope);
                    let object = self.create_restored("Name", pairs, after.identities.names.get(index))?;
                    self.set(&object, "key", json!(object.as_str()))?;
                    out.push(object);
                }
            }
        }
        for object in &out {
            self.set(object, "key", json!(object.as_str()))?;
        }
        for (i, o) in old.iter().enumerate() {
            if !used.get(i).copied().unwrap_or(false) {
                self.delete(o)?;
            }
        }
        self.wrote |= sync_list(self.tx, &root, "names", &out)? > 0;
        Ok(out)
    }
}

/// The places of a sheet's lines before a write, to find the view's cell of a `cells` key.
/// A sheet's live row and column tokens.
#[derive(Default)]
struct LiveTokens {
    rows: HashSet<String>,
    cols: HashSet<String>,
}

impl LiveTokens {
    /// Whether a cell key (`<row>:<col>`) names live lines (`#n` indexes are live).
    fn holds(&self, key: &str) -> bool {
        let live = |token: &str, set: &HashSet<String>| token.strip_prefix('#').map_or_else(|| set.contains(token), |n| n.parse::<u32>().is_ok());
        key.match_indices(':')
            .any(|(i, _)| matches!((key.get(..i), key.get(i + 1..)), (Some(r), Some(c)) if live(r, &self.rows) && live(c, &self.cols)))
    }
}

struct BeforeLines {
    rows: HashMap<String, u32>,
    cols: HashMap<String, u32>,
}

impl BeforeLines {
    fn new(sid: &SheetIds) -> BeforeLines {
        let places = |lines: &[ObjectId]| -> HashMap<String, u32> {
            lines.iter().enumerate().filter_map(|(i, o)| Some((o.as_str().to_owned(), u32::try_from(i).ok()?))).collect()
        };
        BeforeLines { rows: places(&sid.rows), cols: places(&sid.cols) }
    }

    fn index(map: &HashMap<String, u32>, token: &str) -> Option<u32> {
        match token.strip_prefix('#') {
            Some(n) => n.parse().ok(),
            None => map.get(token).copied(),
        }
    }

    /// The position `key` (`<row>:<col>`, either id may hold `:`) had.
    fn position(&self, key: &str) -> Option<CellRef> {
        key.match_indices(':').find_map(|(i, _)| {
            let row = Self::index(&self.rows, key.get(..i)?)?;
            let col = Self::index(&self.cols, key.get(i + 1..)?)?;
            Some(CellRef::new(row, col))
        })
    }
}

/// Whether two cells store the same entry (a formula's value is not stored).
/// A drawing object's id as its `key` (its tool id `drawing:<id>`, KitObjectModel OM9): the kit's
/// own number for the shape, chart or picture, unique in the workbook; none for an unnumbered one.
fn drawing_key(id: u32) -> Value {
    if id == 0 { Value::Null } else { json!(id) }
}

fn same_stored(a: Option<&kition_sheet_model::Cell>, b: Option<&kition_sheet_model::Cell>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => match (&a.formula, &b.formula) {
            (Some(x), Some(y)) => a.style == b.style && (Arc::ptr_eq(x, y) || (x.text == y.text && x.array == y.array)),
            _ => false,
        },
        _ => false,
    }
}

/// Whether two JSON values are equal, numbers by value: the replica keeps a whole float as an
/// integer (`36.0` reads back as `36`).
pub(crate) fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y || x.as_f64().zip(y.as_f64()).is_some_and(|(x, y)| x == y),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, p)| y.get(k).is_some_and(|q| same_json(p, q))),
        _ => a == b,
    }
}

/// For each `after` sheet, its `before` partner: the same `Arc` (untouched), then the same name,
/// then, between sheets so paired, the rest in order (a renamed or edited sheet).
pub(crate) fn pair_sheets(before: &[Arc<Sheet>], after: &[Arc<Sheet>]) -> Vec<Option<usize>> {
    let mut partner: Vec<Option<usize>> = vec![None; after.len()];
    let mut used = vec![false; before.len()];
    for (j, a) in after.iter().enumerate() {
        if let Some(i) = (0..before.len()).find(|&i| !used[i] && before.get(i).is_some_and(|b| Arc::ptr_eq(a, b))) {
            partner[j] = Some(i);
            used[i] = true;
        }
    }
    for (j, a) in after.iter().enumerate() {
        if partner[j].is_some() {
            continue;
        }
        if let Some(i) = (0..before.len()).find(|&i| !used[i] && before.get(i).is_some_and(|b| b.name.eq_ignore_ascii_case(&a.name))) {
            partner[j] = Some(i);
            used[i] = true;
        }
    }
    let matched: Vec<(usize, usize)> = partner.iter().enumerate().filter_map(|(j, i)| i.map(|i| (j, i))).collect();
    let order: Vec<usize> = matched.iter().map(|(_, i)| *i).collect();
    let stays: HashSet<usize> = longest_increasing(&order).into_iter().collect();
    let mut anchors: Vec<(usize, usize)> = matched.into_iter().filter(|(_, i)| stays.contains(i)).collect();
    anchors.push((after.len(), before.len()));
    let mut start = (0usize, 0usize);
    for (j_end, i_end) in anchors {
        let mut free: Vec<usize> = (start.1..i_end).filter(|&i| !used[i]).collect();
        free.reverse();
        for slot in partner.iter_mut().take(j_end).skip(start.0) {
            if slot.is_none()
                && let Some(i) = free.pop()
            {
                *slot = Some(i);
                used[i] = true;
            }
        }
        start = (j_end + 1, i_end + 1);
    }
    partner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheets_pair_by_identity_then_name_then_place() {
        let s = |n: &str| Arc::new(Sheet::new(n));
        let (a, b, c) = (s("A"), s("B"), s("C"));
        // Untouched, reordered.
        assert_eq!(pair_sheets(&[a.clone(), b.clone(), c.clone()], &[c.clone(), a.clone(), b.clone()]), vec![Some(2), Some(0), Some(1)]);
        // B renamed (a new Arc, a new name): it keeps its place.
        assert_eq!(pair_sheets(&[a.clone(), b.clone(), c.clone()], &[a.clone(), s("Renamed"), c.clone()]), vec![Some(0), Some(1), Some(2)]);
        // A new sheet at the end.
        assert_eq!(pair_sheets(std::slice::from_ref(&a), &[a.clone(), s("New")]), vec![Some(0), None]);
        // Edited (new Arc, same name).
        assert_eq!(pair_sheets(&[a.clone(), b.clone()], &[s("B"), s("A")]), vec![Some(1), Some(0)]);
    }

    #[test]
    fn style_keys_are_content_hashes() {
        let a = to_json(&kition_sheet_model::Style::default()).unwrap();
        assert_eq!(style_key(&a), style_key(&a.clone()));
        let mut bold = kition_sheet_model::Style::default();
        bold.font.bold = true;
        assert_ne!(style_key(&to_json(&bold).unwrap()), style_key(&a));
    }
}
