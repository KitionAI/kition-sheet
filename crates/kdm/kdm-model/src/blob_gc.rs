//! Which blobs a [`crate::SheetSession`] still needs, so the rest can be dropped (ported from
//! Kition Slide's and Photo's `kdm-model`, same owner and licence; only [`object_blobs`] is
//! Sheet's own).
//!
//! A blob is kept while the replica's current state references it, while an undo or redo step
//! this session can still take might bring it back, or while it waits to be imported (blobs a
//! peer sent before its changes). Everything else (the bytes of a picture deleted and its undo
//! step trimmed from history) is garbage: without this a long session keeps every picture it
//! ever held.
//!
//! `kit.sheet` references blobs in two places: an `Image`'s `blob` field, and a `Cell`'s
//! `picture` (a picture in a cell, `{blob, mime, alt}`; one register, read whenever the cell
//! changes).
//!
//! The current state's references are counted per object, updated from the objects each
//! transaction, undo, redo or import changed, so nothing rescans the whole document. Each undo
//! step keeps the blobs its transaction displaced (to undo it) and added (to redo it after an
//! undo, even if a collaborator overwrote them meanwhile); a redo step keeps the same for the undo
//! that made it.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use kition_kit::kdm::{ObjectId, Replica};
use kition_kit::session::MemoryBlobs;
use serde_json::Value;

/// The blob hashes one replica operation stopped and started referencing.
#[derive(Default)]
pub(crate) struct Moved {
    pub displaced: HashSet<String>,
    pub added: HashSet<String>,
}

impl Moved {
    fn all(self) -> HashSet<String> {
        let mut all = self.displaced;
        all.extend(self.added);
        all
    }
}

#[derive(Default)]
pub(crate) struct BlobIndex {
    by_object: HashMap<ObjectId, Vec<String>>,
    refs: HashMap<String, u32>,
    /// What each undo step can bring back, oldest first.
    undo: VecDeque<HashSet<String>>,
    /// What each redo step can bring back, the next redo last.
    redo: Vec<HashSet<String>>,
    /// Blobs added for changes not imported yet.
    pending: HashSet<String>,
}

/// Every `{"hash": …, "mime": …}` (a blob reference) inside `value`.
fn collect(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let (Some(Value::String(hash)), Some(Value::String(_))) = (map.get("hash"), map.get("mime")) {
                out.push(hash.clone());
            }
            for item in map.values() {
                collect(item, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect(item, out)),
        _ => {}
    }
}

/// The blobs object `id` references (none if it no longer exists): an `Image`'s `blob`, a
/// `Cell`'s `picture`.
fn object_blobs(replica: &Replica, id: &ObjectId) -> Vec<String> {
    let mut out = Vec::new();
    if !replica.exists(id) {
        return out;
    }
    match replica.object_type(id).as_deref() {
        Ok("Image") => {
            if let Ok(value) = replica.get(id, "blob") {
                collect(&value, &mut out);
            }
        }
        Ok("Cell") => {
            if let Ok(value) = replica.get(id, "picture") {
                collect(&value, &mut out);
            }
        }
        _ => {}
    }
    out
}

impl BlobIndex {
    /// Indexes a freshly loaded or created replica: its sheets (the workbook's `sheets` list),
    /// their images (each sheet's `images` list), and the `Cell` objects of the sheets `cells`
    /// says may hold cell pictures (see the module docs). Rule P1: nothing here walks every
    /// object or every cell of a large workbook (`objects_of_type` reads each object's type; the
    /// view's own read is the one pass over the cells). An object no list names is not shown,
    /// and is indexed when a change names it again.
    pub fn build(replica: &Replica, cells: &dyn Fn(&ObjectId) -> bool) -> Self {
        let mut index = Self::default();
        let mut all = BTreeSet::new();
        for sheet in replica.list(&crate::root_id(), "sheets").unwrap_or_default() {
            all.extend(replica.list(&sheet, "images").unwrap_or_default());
            if cells(&sheet) {
                let entries = replica.map_entries(&sheet, "cells").unwrap_or_default();
                all.extend(entries.iter().filter_map(|(_, v)| v.as_str()).map(ObjectId::from_string));
            }
            all.insert(sheet);
        }
        index.update(replica, &all);
        index
    }

    /// Re-reads the references of the `changed` objects.
    ///
    /// An object can list the same hash more than once (one picture in two cells): `before` and
    /// `now` are counted as multisets, and only a hash's *net* change in how many times this one
    /// object lists it touches `refs`, so a hash an object keeps listing never drops to a
    /// transient zero and looks "added" again.
    pub fn update(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>) -> Moved {
        let mut dropped = Vec::new();
        let mut gained = Vec::new();
        for id in changed {
            let now = object_blobs(replica, id);
            let before = if now.is_empty() { self.by_object.remove(id) } else { self.by_object.insert(id.clone(), now.clone()) }.unwrap_or_default();
            let mut before_counts: HashMap<&str, i64> = HashMap::new();
            for hash in &before {
                *before_counts.entry(hash.as_str()).or_insert(0) += 1;
            }
            let mut now_counts: HashMap<&str, i64> = HashMap::new();
            for hash in &now {
                *now_counts.entry(hash.as_str()).or_insert(0) += 1;
            }
            let mut hashes: BTreeSet<&str> = before_counts.keys().copied().collect();
            hashes.extend(now_counts.keys().copied());
            for hash in hashes {
                let delta = now_counts.get(hash).copied().unwrap_or(0) - before_counts.get(hash).copied().unwrap_or(0);
                if delta > 0 {
                    let count = self.refs.entry(hash.to_owned()).or_insert(0);
                    if *count == 0 {
                        gained.push(hash.to_owned());
                    }
                    *count = count.saturating_add(u32::try_from(delta).unwrap_or(u32::MAX));
                } else if delta < 0
                    && let Some(count) = self.refs.get_mut(hash)
                {
                    *count = count.saturating_sub(u32::try_from(-delta).unwrap_or(u32::MAX));
                    if *count == 0 {
                        self.refs.remove(hash);
                        dropped.push(hash.to_owned());
                    }
                }
            }
        }
        let mut moved = Moved::default();
        for hash in dropped {
            if !self.refs.contains_key(&hash) {
                moved.displaced.insert(hash);
            }
        }
        for hash in gained {
            if !moved.displaced.remove(&hash) {
                moved.added.insert(hash);
            }
        }
        moved
    }

    /// A transaction that became a new undo step.
    pub fn push_step(&mut self, moved: Moved) {
        self.undo.push_back(moved.all());
        self.redo.clear();
    }

    pub fn undone(&mut self, moved: Moved) {
        self.undo.pop_back();
        self.redo.push(moved.all());
    }

    pub fn redone(&mut self, moved: Moved) {
        self.redo.pop();
        self.undo.push_back(moved.all());
    }

    pub fn undo_steps(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_steps(&self) -> usize {
        self.redo.len()
    }

    /// Every hash the current state references (not the extra copies undo, redo or a pending
    /// import keep around): what a peer starting from nothing needs.
    pub fn referenced(&self) -> Vec<String> {
        self.refs.keys().cloned().collect()
    }

    /// Keeps only the newest `undo` undo steps and the next `redo` redo steps: the older ones
    /// can no longer be taken, so their blobs can go.
    pub fn retain(&mut self, undo: usize, redo: usize) {
        while self.undo.len() > undo {
            self.undo.pop_front();
        }
        if self.redo.len() > redo {
            self.redo.drain(..self.redo.len() - redo);
        }
    }

    pub fn protect(&mut self, hashes: impl IntoIterator<Item = String>) {
        self.pending.extend(hashes);
    }

    pub fn imported(&mut self) {
        self.pending.clear();
    }

    /// Drops every blob nothing above needs.
    pub fn collect_garbage(&self, blobs: &mut MemoryBlobs) {
        blobs.0.retain(|hash, _| {
            self.refs.contains_key(hash)
                || self.pending.contains(hash)
                || self.undo.iter().any(|s| s.contains(hash))
                || self.redo.iter().any(|s| s.contains(hash))
        });
    }
}
