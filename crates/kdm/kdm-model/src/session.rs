//! [`SheetSession`]: one workbook's replica, its [`SheetModel`] view and its blobs, for the
//! engine's command bridge (Kition phase 3f, Task 2).
//!
//! It is `kition_kit::session::KitSession<SheetModel>`'s flow (transactions, per-actor undo, the
//! change exchange, the post-merge repair through [`KitModel::validate`], the view patched through
//! [`KitModel::apply`]) on concrete parts, as Kition Slide's `SlideSession` and Photo's
//! `PhotoSession` do: the engine must enumerate, ship and garbage-collect the picture blobs, which
//! `KitSession`'s boxed blob store does not allow. Edits are diff-bridged: the engine changes its
//! own [`Workbook`], and [`SheetSession::record`] writes what changed as one transaction
//! ([`write_changes`], with the command's structural edits), then adopts it as the view. The view
//! after an undo, a redo or an import is recalculated ([`SheetModel`] stores no formula results)
//! and keeps this person's own view state ([`crate::Known`]). Everything here is `Send`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, OriginKind, Replica, Tx, UndoScope, Version};
use kition_kit::session::{BlobSource, Damage, KitModel, MemoryBlobs};
use kition_sheet_model::{StructuralEdit, Workbook};
use serde_json::{Value, json};

use crate::blob_gc::{BlobIndex, Moved};
use crate::cells;
use crate::write::{Created, create_ahead, write_changes_logged};
use crate::{Ids, Result, SheetModel, kit_err, normalize, read_document, write_changes, write_document};

/// One open workbook as a KDM replica and the engine's view of it. See the module docs.
pub struct SheetSession {
    replica: Replica,
    model: SheetModel,
    blobs: MemoryBlobs,
    /// Which blobs are still needed ([`crate::blob_gc`]); the rest are dropped after each change.
    index: BlobIndex,
    /// The hashes each record, import, undo and redo started referencing, with the version right
    /// after it, for [`SheetSession::blobs_since`] (Kition Photo's blob log: chronological, so
    /// `since` cleanly splits it into "already has" and "needs"; trimmed with the store).
    blob_log: Vec<(Version, Vec<String>)>,
    /// This session's token, the `by` of the `Cell` objects it creates ahead, and the end of their
    /// ids (see `cells`).
    token: String,
    /// The `Cell` objects this session created ahead of a person's write: an import that gives
    /// one's entry to another session's object (both typed into one empty cell at once) makes
    /// [`SheetSession::import_changes`] record the race (`cells::RACED`), so the cell shows both.
    mine: HashSet<ObjectId>,
    /// Every `Cell` created ahead this session knows of (any session's), with its `key`: an
    /// import that only re-points a cell's entry (an undo elsewhere) can make a race of two of
    /// them without changing either, so such an import checks every key two of them share.
    ahead: BTreeMap<ObjectId, String>,
    /// Whether the view could not follow the replica's last change (a picture whose blob has not
    /// arrived yet): the next import, undo, redo, or [`SheetSession::retry_view`] reads it whole.
    stale: bool,
}

/// A session's token for `by`: its actor, unique to the replica.
fn token_of(replica: &Replica) -> String {
    format!("{:x}", replica.actor().get())
}

impl std::fmt::Debug for SheetSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetSession").field("doc", &self.replica.doc_id()).field("actor", &self.replica.actor()).finish_non_exhaustive()
    }
}

/// Whether the sheet `id` may hold cell pictures in `model` (its view's sheet has some). A sheet
/// the view does not hold, or a view that may be stale (`exact` false), is read regardless.
fn may_hold_pictures(model: &SheetModel, exact: bool, id: &ObjectId) -> bool {
    if !exact {
        return true;
    }
    model.ids().sheet_of(id).and_then(|k| model.workbook.sheets.get(k)).is_none_or(|sheet| !sheet.cell_pictures.is_empty())
}

impl SheetSession {
    /// A new replica holding `workbook`, written in its creating transaction (not an undo step).
    /// The view is `workbook` itself.
    pub fn create(workbook: &Workbook) -> Result<Self> {
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::default();
        let (mut replica, ()) = Replica::create_with(SheetModel::schema(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
            write_document(tx, workbook, &mut ids, &mut blobs).map(|_| ())
        })
        .map_err(kit_err)?;
        replica.take_changed();
        let mut model = SheetModel::from_parts(workbook.clone(), ids);
        model.note_shapes(&replica, None);
        let index = BlobIndex::build(&replica, &|id| may_hold_pictures(&model, true, id));
        let token = token_of(&replica);
        Ok(Self { replica, model, blobs, index, blob_log: Vec::new(), token, mine: HashSet::new(), ahead: BTreeMap::new(), stale: false })
    }

    /// Opens a snapshot (or a full change export) of another replica as a new replica of a fresh
    /// actor. `blobs` must hold the blobs it references. The view is recalculated.
    pub fn open_snapshot(bytes: &[u8], blobs: MemoryBlobs) -> Result<Self> {
        let mut replica = Replica::load(SheetModel::schema(), bytes, ActorId::random()).map_err(kit_err)?;
        replica.take_changed();
        let (model, named_ahead) = SheetModel::load_races(&replica, &blobs)?;
        let index = BlobIndex::build(&replica, &|id| may_hold_pictures(&model, true, id));
        let token = token_of(&replica);
        let mut session =
            Self { replica, model, blobs, index, blob_log: Vec::new(), token, mine: HashSet::new(), ahead: BTreeMap::new(), stale: false };
        // Best effort: a replica merged elsewhere may hold races no session recorded yet. The kept
        // side of one is always a `Cell` created ahead that an entry names (`record_races` records
        // no other), so the scan of every `Cell` runs only when the read met such an entry: a
        // workbook written in bulk (an import, a file opened) has none and opens without it.
        if named_ahead {
            let found: Vec<ObjectId> = session.replica.objects_of_type(cells::CELL).into_iter().filter(|o| cells::is_ahead(o.as_str())).collect();
            session.note_ahead(found.iter());
            let _ = session.record_races(None, &[]);
        }
        session.index.collect_garbage(&mut session.blobs);
        Ok(session)
    }

    /// The view: the workbook as of the last record, undo, redo or import.
    pub fn workbook(&self) -> &Workbook {
        &self.model.workbook
    }

    pub fn model(&self) -> &SheetModel {
        &self.model
    }

    /// Imports a file without rebuilding this replica or either undo history.
    pub fn reconcile_file(&mut self, workbook: &Workbook) -> Result<Damage> {
        let fresh = Self::create(workbook)?;
        let schema = SheetModel::schema();
        let before = self.replica.state_json().map_err(kit_err)?;
        let after = fresh.replica.state_json().map_err(kit_err)?;
        self.replica
            .transact(&Origin::new(OriginKind::Import, "file:reread"), |tx| crate::reread::reconcile(tx, &schema, &before, &after))
            .map_err(kit_err)?;
        self.add_blobs(&fresh.blobs);
        self.settle().0
    }

    /// Writes what differs between the view and `after` as one transaction of `origin`, then
    /// makes `after` the view. `edits` are the structural edits the engine logged on the way from
    /// the view to `after` (`DocState::take_structural_edits`; empty when unknown: the write is
    /// then by position, correct but larger). Answers how many undo steps that made: 1, or 0 when
    /// nothing replicated changed or `origin` is a `system`/`import` one (never an undo step). On
    /// failure nothing is written and the view is unchanged.
    pub fn record(&mut self, origin: &Origin, after: &Workbook, edits: &[StructuralEdit]) -> Result<u32> {
        self.record_marked(origin, after, edits, &mut |_| {})
    }

    /// [`SheetSession::record`], calling `mark` as each of its phases ends: `write` (the diff of
    /// the view against `after` written), `positions` (the lines' and cells' addresses projected),
    /// `commit` (the transaction applied), and `view` (the view adopted, the blobs updated). The
    /// performance tests time them (`tests/perf_scale.rs`); the engine records through `record`.
    #[doc(hidden)]
    pub fn record_marked(&mut self, origin: &Origin, after: &Workbook, edits: &[StructuralEdit], mark: &mut dyn FnMut(&'static str)) -> Result<u32> {
        let person = !matches!(origin.kind, OriginKind::System | OriginKind::Import);
        let mut ids = self.model.ids().clone();
        let mut before = self.replica.version();
        let (model, blobs) = (&self.model, &mut self.blobs);
        // A person's write that would create objects another replica may create too (padding
        // lines, `Cell` objects: derived ids) is not committed: those objects are made first, empty,
        // in a `system` transaction (not an undo step), and the write runs again on top of them.
        // Undoing the person's step then reverts only what they wrote into the objects, never the
        // objects a collaborator may have written into at the same time.
        let mut ahead = Vec::new();
        let result = self.replica.transact(origin, |tx| {
            let (_, created) = write_changes_logged(tx, &model.workbook, after, edits, &mut ids, blobs)?;
            mark("write");
            if person && !created.is_empty() {
                ahead = created;
                return Err(KdmError::Engine("objects to create ahead".into()));
            }
            crate::settle::project_positions(tx, (&model.workbook, model.ids()), after, &ids)?;
            mark("positions");
            Ok(())
        });
        mark("commit");
        let mut changed = self.replica.take_changed();
        let result = match result {
            Err(_) if !ahead.is_empty() => {
                let token = self.token.clone();
                let lines = self.replica.transact(&Origin::system("create"), |tx| create_ahead(tx, &ahead, &token)).map_err(kit_err)?;
                changed = self.replica.take_changed();
                for created in &ahead {
                    if let Created::Cell { id, .. } = created {
                        let id = cells::ahead_id(id, &token);
                        if self.replica.get(&id, "by").ok().as_ref().and_then(Value::as_str) == Some(token.as_str()) {
                            self.mine.insert(id);
                        }
                    }
                }
                let mut ahead_ids = self.model.ids().clone();
                for (sheet, axis, line) in lines {
                    if let Some(sid) = ahead_ids.sheets.iter_mut().find(|s| s.object == sheet) {
                        sid.lines_mut(axis).push(line);
                    }
                }
                let view = self.model.workbook.clone();
                self.model.adopt_written(&self.replica, view, ahead_ids, &changed);
                before = self.replica.version();
                ids = self.model.ids().clone();
                let (model, blobs) = (&self.model, &mut self.blobs);
                let result = self.replica.transact(origin, |tx| {
                    write_changes(tx, &model.workbook, after, edits, &mut ids, blobs)?;
                    crate::settle::project_positions(tx, (&model.workbook, model.ids()), after, &ids)
                });
                changed.extend(self.replica.take_changed());
                result
            }
            result => result,
        };
        if let Err(error) = result {
            self.index.collect_garbage(&mut self.blobs);
            return Err(kit_err(error));
        }
        self.model.adopt_written(&self.replica, after.clone(), ids, &changed);
        let moved = self.index.update(&self.replica, &changed);
        let wrote = self.replica.version() != before;
        let undoable = wrote && !matches!(origin.kind, OriginKind::System | OriginKind::Import);
        self.log_added(moved.added.iter().cloned());
        if undoable {
            self.index.push_step(moved);
        }
        self.index.collect_garbage(&mut self.blobs);
        mark("view");
        Ok(u32::from(undoable))
    }

    /// Whether the replica holds `workbook`'s content, compared id-blind ([`normalize`]) as
    /// [`read_document`] reads both back. Errs if `workbook` cannot be written to a replica.
    pub fn holds(&self, workbook: &Workbook) -> Result<bool> {
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::default();
        let (scratch, ()) = Replica::create_with(SheetModel::schema(), DocId::new(), ActorId::random(), &Origin::system("create"), |tx| {
            write_document(tx, workbook, &mut ids, &mut blobs).map(|_| ())
        })
        .map_err(kit_err)?;
        let (theirs, _) = read_document(&scratch, &blobs).map_err(kit_err)?;
        let (mine, _) = read_document(&self.replica, &self.blobs).map_err(kit_err)?;
        let value = |wb: &Workbook| serde_json::to_value(normalize(wb)).map_err(kit_err);
        Ok(value(&theirs)? == value(&mine)?)
    }

    /// Undoes this replica's last step of the person (never an import or a repair), repairs the
    /// result and patches the view. Answers whether there was one, and the damage.
    ///
    /// A step with nothing left to revert is taken with no change, answering `true`: the
    /// replica's undo skips a step whose writes all went with an object a merge dropped (one a
    /// 0.4.3 or older peer created under the same id at the same time), reverting the step below it
    /// instead, if any, or nothing.
    pub fn undo(&mut self) -> Result<(bool, Damage)> {
        self.undo_in(UndoScope::Person)
    }

    /// [`SheetSession::undo`] in `scope` (KitModel M12a): the person's and the agents' steps on
    /// this replica are separate histories.
    pub fn undo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        if self.index.undo_steps() == 0 {
            return Ok((false, Damage::None));
        }
        let done = self.replica.can_undo_in(scope) && self.replica.undo_in(scope).map_err(kit_err)?;
        let (damage, moved) = self.settle();
        if done {
            self.log_added(moved.added.iter().cloned());
        }
        self.index.undone(moved);
        self.index.collect_garbage(&mut self.blobs);
        Ok((true, damage?))
    }

    /// Like [`SheetSession::undo`]: a step with nothing left to redo is taken with no change.
    pub fn redo(&mut self) -> Result<(bool, Damage)> {
        self.redo_in(UndoScope::Person)
    }

    /// [`SheetSession::redo`] in `scope`, like [`SheetSession::undo_in`].
    pub fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        if self.index.redo_steps() == 0 {
            return Ok((false, Damage::None));
        }
        let done = self.replica.can_redo_in(scope) && self.replica.redo_in(scope).map_err(kit_err)?;
        let (damage, moved) = self.settle();
        if done {
            self.log_added(moved.added.iter().cloned());
        }
        self.index.redone(moved);
        self.index.collect_garbage(&mut self.blobs);
        Ok((true, damage?))
    }

    /// Whether there is an undo step to take: one this session recorded, not trimmed by
    /// [`SheetSession::retain_steps`] (one the replica skips is still taken, see
    /// [`SheetSession::undo`]).
    pub fn can_undo(&self) -> bool {
        self.index.undo_steps() > 0
    }

    pub fn can_redo(&self) -> bool {
        self.index.redo_steps() > 0
    }

    /// Whether the replica holds a step of `scope` to undo (or redo), within the reachable steps.
    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.index.undo_steps() > 0 && self.replica.can_undo_in(scope)
    }

    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.index.redo_steps() > 0 && self.replica.can_redo_in(scope)
    }

    /// Keeps only the newest `undo` undo steps and the next `redo` redo steps reachable (the
    /// engine history's own) and drops the blobs only older steps needed: the replica has no
    /// limit of its own, so this is what keeps a long session's pictures bounded.
    pub fn retain_steps(&mut self, undo: usize, redo: usize) {
        self.index.retain(undo, redo);
        self.index.collect_garbage(&mut self.blobs);
        let blobs = &self.blobs;
        self.blob_log.retain(|(_, hashes)| hashes.iter().any(|hash| blobs.0.contains_key(hash)));
    }

    /// Undo and redo steps reachable, as [`SheetSession::retain_steps`] left them.
    pub fn steps(&self) -> (usize, usize) {
        (self.index.undo_steps(), self.index.redo_steps())
    }

    /// Imports remote or stored changes (their blobs added first, [`SheetSession::add_blobs`]),
    /// repairs the merged state and patches the view. An import whose repair or view load fails
    /// has still imported its changes: [`SheetSession::view_loaded`] tells whether the view
    /// followed.
    ///
    /// Two sessions that typed into one empty cell at once each made a `Cell` object of their own
    /// for it, and the merge gave the cell's entry to one of them: the session records the other in
    /// the kept one's `raced` (`cells`), so the cell shows both texts on every replica, and each
    /// person's undo reverts their own typing.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        self.replica.import_changes(bytes).map_err(kit_err)?;
        let (damage, moved) = self.settle();
        self.log_added(moved.added.iter().cloned());
        self.index.imported();
        self.index.collect_garbage(&mut self.blobs);
        damage
    }

    /// Records the races on one new cell `changed` shows (`None`: every `Cell` created ahead, when
    /// opening a replica merged elsewhere) as the kept objects' `raced`, in a `system`
    /// transaction (not an undo step). `shown`: objects the view showed as such parts before (one
    /// a concurrent record may have dropped). Answers whether it wrote.
    fn record_races(&mut self, changed: Option<&BTreeSet<ObjectId>>, shown: &[ObjectId]) -> Result<bool> {
        if let Some(changed) = changed {
            let mine: Vec<ObjectId> = self.mine.iter().cloned().collect();
            self.note_ahead(changed.iter().chain(&mine));
        }
        let replica = &self.replica;
        self.ahead.retain(|o, _| replica.exists(o));
        let sheet_changed = changed.is_some_and(|changed| changed.iter().any(|o| self.model.ids().sheet_of(o).is_some()));
        // The ahead cells of keys two of them share: the only ones an entry can make a race of.
        let shared = || {
            let mut by_key: HashMap<&str, Vec<&ObjectId>> = HashMap::new();
            for (o, key) in &self.ahead {
                by_key.entry(key.as_str()).or_default().push(o);
            }
            by_key.into_values().filter(|v| v.len() > 1).flatten().cloned().collect::<Vec<_>>()
        };
        let candidates: BTreeSet<ObjectId> = match changed {
            Some(changed) => {
                let direct = changed.iter().filter(|o| cells::is_ahead(o.as_str())).chain(&self.mine).chain(shown.iter()).cloned();
                if sheet_changed { direct.chain(shared()).collect() } else { direct.collect() }
            }
            None => shared().into_iter().collect(),
        };
        if candidates.is_empty() {
            return Ok(false);
        }
        let sheets: Vec<ObjectId> = self.model.ids().sheets.iter().map(|s| s.object.clone()).collect();
        let mut merged: BTreeMap<ObjectId, BTreeSet<String>> = BTreeMap::new();
        for id in &candidates {
            let Some((_, held)) = cells::holder_of(&self.replica, &sheets, id) else { continue };
            if !cells::is_ahead(held.as_str()) {
                continue;
            }
            // An empty part (its typing undone or cleared) adds nothing to the cell: recording
            // it would only make a replica reopened from a snapshot write what the live one never
            // did.
            if id != &held && cells::is_empty_object(&self.replica, id) {
                continue;
            }
            let set = merged.entry(held.clone()).or_insert_with(|| {
                let v = self.replica.get(&held, cells::RACED).unwrap_or(Value::Null);
                v.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
            });
            set.insert(id.as_str().to_owned());
        }
        let replica = &self.replica;
        let string = |id: &ObjectId, field: &str| replica.get(id, field).ok().filter(|v| v.is_string());
        // Each kept object's `raced`, and, where the kept one has no style, the style of an
        // object recorded now.
        let writes: Vec<(ObjectId, Value, Option<Value>)> = merged
            .into_iter()
            .filter_map(|(held, set)| {
                let v = json!(set.iter().collect::<Vec<_>>());
                let current = replica.get(&held, cells::RACED).unwrap_or(Value::Null);
                if current == v {
                    return None;
                }
                let recorded: BTreeSet<&str> = current.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                let style = match string(&held, "style") {
                    Some(_) => None,
                    None => set.iter().filter(|o| !recorded.contains(o.as_str())).find_map(|o| string(&ObjectId::from_string(o.as_str()), "style")),
                };
                Some((held, v, style))
            })
            .collect();
        if writes.is_empty() {
            return Ok(false);
        }
        let result = self.replica.transact(&Origin::system("repair"), |tx| {
            for (held, v, style) in &writes {
                tx.set(held, cells::RACED, v.clone())?;
                if let Some(style) = style {
                    tx.set(held, "style", style.clone())?;
                }
            }
            Ok(())
        });
        self.replica.take_changed();
        result.map_err(kit_err)?;
        // A race joins cells the view may hold apart (its kept and other objects, entries an
        // import re-pointed): read the view whole rather than patch it. Races are rare.
        let blobs: &dyn BlobSource = &self.blobs;
        self.model.reload(&self.replica, blobs)?;
        Ok(true)
    }

    /// Remembers the `Cell` objects created ahead among `ids`, with their keys.
    fn note_ahead<'a>(&mut self, ids: impl Iterator<Item = &'a ObjectId>) {
        for id in ids.filter(|o| cells::is_ahead(o.as_str())) {
            if self.ahead.contains_key(id) {
                continue;
            }
            if let Some(key) = self.replica.get(id, "key").ok().and_then(|v| v.as_str().map(str::to_owned)) {
                self.ahead.insert(id.clone(), key);
            }
        }
    }

    /// Whether the view matches the replica as it is now (its own read succeeds).
    pub fn view_loaded(&self) -> bool {
        read_document(&self.replica, &self.blobs).is_ok()
    }

    /// The blobs a peer at `since` needs to reach this session's current state (Kition Photo's
    /// `blobs_since`): the hashes logged for a version `since` does not include, still in the
    /// store. An empty `since` answers every blob the current state references (a creating
    /// transaction or a snapshot is never logged).
    pub fn blobs_since(&self, since: &Version) -> Vec<(String, Arc<[u8]>)> {
        if *since == Version::empty() {
            return self.index.referenced().into_iter().filter_map(|hash| self.blobs.0.get(&hash).cloned().map(|bytes| (hash, bytes))).collect();
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (version, hashes) in &self.blob_log {
            if since.includes(version) {
                continue;
            }
            for hash in hashes {
                if seen.insert(hash.clone())
                    && let Some(bytes) = self.blobs.0.get(hash)
                {
                    out.push((hash.clone(), bytes.clone()));
                }
            }
        }
        out
    }

    fn log_added(&mut self, added: impl IntoIterator<Item = String>) {
        let added: Vec<String> = added.into_iter().collect();
        if !added.is_empty() {
            self.blob_log.push((self.replica.version(), added));
        }
    }

    pub fn export_changes(&self, since: &Version) -> Result<Vec<u8>> {
        self.replica.export_changes(since).map_err(kit_err)
    }

    pub fn version(&self) -> Version {
        self.replica.version()
    }

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.replica.snapshot().map_err(kit_err)
    }

    pub fn state_hash(&self) -> Result<String> {
        self.replica.state_hash().map_err(kit_err)
    }

    pub fn doc_id(&self) -> DocId {
        self.replica.doc_id()
    }

    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    /// The comment anchor ids of `range` on sheet `sheet` ([`crate::comment_anchor`]).
    pub fn comment_anchor(&self, sheet: usize, range: kition_sheet_core::RangeRef) -> Option<Vec<String>> {
        crate::comment_anchor(&self.replica, self.model.ids(), sheet, range)
    }

    /// Where a comment anchor is now ([`crate::find_comment_anchor`]).
    pub fn find_comment_anchor(&self, anchor: &[String]) -> Option<(usize, kition_sheet_core::RangeRef)> {
        crate::find_comment_anchor(&self.replica, self.model.ids(), anchor)
    }

    /// The blobs the replica references or an undo, redo or pending import can still need.
    pub fn blobs(&self) -> &MemoryBlobs {
        &self.blobs
    }

    /// Whether the view could not follow the replica's last change; see [`SheetSession::retry_view`].
    pub fn view_stale(&self) -> bool {
        self.stale
    }

    /// Whether the replica references a blob this session does not hold (one on its way from a
    /// peer).
    pub fn missing_blobs(&self) -> bool {
        self.index.referenced().iter().any(|hash| !self.blobs.0.contains_key(hash))
    }

    /// Reads a stale view whole again (after [`SheetSession::add_blobs`] brought what it lacked).
    /// Answers whether the view now follows the replica again; `false` when it was not stale.
    pub fn retry_view(&mut self) -> Result<bool> {
        if !self.stale {
            return Ok(false);
        }
        let blobs: &dyn BlobSource = &self.blobs;
        self.model.reload(&self.replica, blobs)?;
        self.stale = false;
        Ok(true)
    }

    /// Adds blobs (from a peer, before importing its changes): kept until the next import.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) {
        for (hash, bytes) in &blobs.0 {
            self.blobs.0.entry(hash.clone()).or_insert_with(|| bytes.clone());
        }
        self.index.protect(blobs.0.keys().cloned());
    }

    /// As `KitSession`'s: repairs what changed in a `system` transaction (not an undo step), then
    /// patches the view with both; the view follows the replica even if the repair fails. Also
    /// answers which blobs moved (the index is updated either way, reading every changed sheet's
    /// cells when the view could not follow).
    fn settle(&mut self) -> (Result<Damage>, Moved) {
        // The view's parts of cells made up of several objects (before this change).
        let shown: Vec<ObjectId> = self.model.ids().sheets.iter().flat_map(|s| s.parts.values().flatten().map(|(o, _)| o.clone())).collect();
        let mut changed = self.replica.take_changed();
        let repaired = if changed.is_empty() {
            Ok(())
        } else {
            let mut kit_error = None;
            let view = self.model.ids();
            let result = self.replica.transact(&Origin::system("repair"), |tx: &mut Tx<'_>| {
                crate::validate::validate_known(tx, &changed, Some(view)).map(|_| ()).map_err(kit_err).map_err(|error| {
                    let message = error.to_string();
                    kit_error = Some(error);
                    KdmError::Engine(message)
                })
            });
            match (result, kit_error) {
                (_, Some(error)) => Err(error),
                (result, None) => result.map_err(kit_err),
            }
        };
        changed.extend(self.replica.take_changed());
        // A stale view (one change could not be applied) is read whole; patching it would miss
        // that change.
        let refreshed = if self.stale {
            let blobs: &dyn BlobSource = &self.blobs;
            self.model.reload(&self.replica, blobs).map(|()| Damage::Full)
        } else {
            self.refresh(&changed)
        };
        self.stale = refreshed.is_err();
        let moved = self.index.update(&self.replica, &changed);
        let damage = refreshed.and_then(|damage| repaired.map(|()| damage));
        let damage = match damage {
            Ok(damage) => self.record_races(Some(&changed), &shown).map(|recorded| if recorded { Damage::Full } else { damage }),
            failed => failed,
        };
        (damage, moved)
    }

    fn refresh(&mut self, changed: &BTreeSet<ObjectId>) -> Result<Damage> {
        if changed.is_empty() {
            return Ok(Damage::None);
        }
        let blobs: &dyn BlobSource = &self.blobs;
        self.model.apply(&self.replica, changed, blobs)
    }
}
