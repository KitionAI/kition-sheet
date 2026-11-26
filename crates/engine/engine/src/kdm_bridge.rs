//! The command bridge to each workbook's KDM replica (Kition phase 3f, Task 2), the design of
//! Kition Slide's, Film's and Photo's.
//!
//! A document's replica ([`kdm_model::SheetSession`]) is its shared source of truth;
//! [`DocState::wb`] is the engine's own view of it. Commands run exactly as before on the
//! in-memory [`Workbook`]; at the end of every [`Session::execute`] (a command run by another
//! included) the bridge writes what changed since the replica last saw it ([`Link::synced`]) as
//! one transaction, with the structural edits the command logged
//! ([`DocState::take_structural_edits`]: taken after every command, so each edit's sheet index is
//! the workbook's as the command left it). An undoable command's commands are part of its one
//! undo step ([`Session::execute`]); their transactions are counted for it ([`Link::pending`])
//! and given to the step it pushes, found by a push counter ([`Link::pushed`], not the stack's
//! length: at the limit a push drops the oldest step). A change that pushed no step (view state,
//! `formula.calculateNow`, the watch list) is written as the system's: never an undo step, and
//! the redo steps survive. The panic guard's rollback after commands that already wrote is
//! written too, as part of the step below it.
//!
//! Undo is per person: each history step remembers how many replica undo steps it took
//! ([`crate::HistoryEntry::steps`]); undo and redo step the replica that many times and take the
//! replica's result (with every collaborator's edit since, recalculated: formula results are
//! derived) as the workbook, not the engine's own snapshot; the active sheet follows the
//! snapshot's by name. The selection, each sheet's remembered selection, the clipboard, the edit
//! mode, the active sheet, and each sheet's zoom, active cell and scroll position are this
//! person's own: never written ([`kdm_model::Known`] keeps them across the replica's reads).
//! After an import the selection follows its sheet and its rows and columns by object (a row
//! inserted above moves it down; a deleted one leaves it on the line now in its place), and each
//! sheet's remembered selection follows its sheet ([`DocState::follow_kdm`]).
//!
//! A replica that refuses a write or an undo is left ([`Session::kdm_resets`]): the document keeps
//! working from its own history snapshots, and the next edit makes a fresh replica.

use std::collections::HashMap;
use std::sync::Arc;

use kdm_model::SheetSession;
use kdm_model::kition_kit::kdm::{ObjectId, Origin, UndoScope, Version};
use kdm_model::kition_kit::session::{Damage, MemoryBlobs};
use kition_sheet_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use kition_sheet_model::{StructuralEdit, Workbook};

use crate::{DocState, EngineError, HistoryEntry, Result, Selection, Session};

/// The origin of the local person's commands (used when nothing else was set with
/// [`Session::execute_as`]/[`Session::set_origin`]).
pub fn local_person() -> Origin {
    Origin::person("person:local")
}

fn kdm_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Other(format!("workbook replica: {error}"))
}

fn no_replica() -> EngineError {
    EngineError::Other("the workbook has no replica and none could be made".into())
}

/// One document's bridge state.
#[derive(Debug)]
pub struct Link {
    /// The replica, made lazily (the first edit or passthrough); `None` again after a reset.
    pub(crate) kdm: Option<SheetSession>,
    /// The workbook as last written to (or read from) the replica.
    pub(crate) synced: Arc<Workbook>,
    pub(crate) resets: u64,
    /// How many undo steps [`DocState::push_undo`] ever pushed (only grows).
    pub(crate) pushed: u64,
    /// [`Link::pushed`] when the bridge last looked.
    pub(crate) mark: u64,
    /// Replica steps the commands an undoable command ran have written: they belong to the step
    /// it pushes when it ends.
    pub(crate) pending: u32,
}

impl Link {
    pub(crate) fn new(wb: &Arc<Workbook>) -> Self {
        Self { kdm: None, synced: wb.clone(), resets: 0, pushed: 0, mark: 0, pending: 0 }
    }
}

/// Where the selection was, by object, before an import: the sheets' objects, the active sheet,
/// and its row and column objects.
pub(crate) struct Place {
    sheets: Vec<ObjectId>,
    active: usize,
    rows: Vec<ObjectId>,
    cols: Vec<ObjectId>,
}

impl DocState {
    /// The document's replica, if it has made one.
    pub fn kdm(&self) -> Option<&SheetSession> {
        self.link.kdm.as_ref()
    }

    /// How many times the document left its replica (a write or undo the replica refused).
    pub fn kdm_resets(&self) -> u64 {
        self.link.resets
    }

    /// Writes what the workbook changed since the replica last saw it, at the end of a command
    /// (see the module docs). `captured`: the command ran inside an undoable one, whose step its
    /// writes belong to.
    pub(crate) fn sync_kdm(&mut self, origin: &Origin, captured: bool) {
        let edits = self.take_structural_edits();
        let pushed = self.link.pushed != self.link.mark;
        self.link.mark = self.link.pushed;
        let changed = !Arc::ptr_eq(&self.link.synced, &self.wb);
        if captured {
            if changed && let Some(steps) = self.record_kdm(origin, &edits) {
                self.link.pending = self.link.pending.saturating_add(steps);
            }
            return;
        }
        if !changed && !pushed && self.link.pending == 0 {
            return;
        }
        let pending = std::mem::take(&mut self.link.pending);
        if pushed {
            let steps = if changed { self.record_kdm(origin, &edits).map_or(0, |n| pending.saturating_add(n)) } else { pending };
            if let Some(top) = self.undo.last_mut() {
                top.steps = steps;
            }
        } else if pending == 0 {
            // No undo step: the system's, so it never undoes and the redo steps survive.
            let _ = self.record_kdm(&Origin::system("view"), &edits);
        } else {
            // A command rolled back after the commands it ran had written: those writes and the
            // rollback net nothing, and undo with the step below; they cleared the replica's redo
            // steps, so the history's go too.
            let steps = if changed { self.record_kdm(origin, &edits).map_or(0, |n| pending.saturating_add(n)) } else { pending };
            if let Some(top) = self.undo.last_mut() {
                top.steps = top.steps.saturating_add(steps);
            }
            let scope = UndoScope::of(origin.kind).unwrap_or(UndoScope::Person);
            self.redo.retain(|u| u.scope != scope);
        }
        self.trim_kdm();
    }

    /// Writes the workbook on top of the replica (made now, baselined on [`Link::synced`], if
    /// there is none) and answers the undo steps that took; `None` when the replica refused and
    /// was left.
    fn record_kdm(&mut self, origin: &Origin, edits: &[StructuralEdit]) -> Option<u32> {
        let after = self.wb.clone();
        if self.link.kdm.is_none() {
            match SheetSession::create(&self.link.synced) {
                Ok(kdm) => self.link.kdm = Some(kdm),
                Err(error) => {
                    log::warn!("the workbook can't be held in a replica: {error}");
                    self.link.synced = after;
                    return Some(0);
                }
            }
        }
        let kdm = self.link.kdm.as_mut()?;
        match kdm.record(origin, &after, edits) {
            Ok(steps) => {
                self.link.synced = after;
                Some(steps)
            }
            Err(error) => {
                log::warn!("the workbook replica refused an edit: {error}");
                self.detach_kdm();
                None
            }
        }
    }

    /// The replica, made now from the current workbook if there is none yet (anything not written
    /// yet is written first).
    pub(crate) fn ensure_kdm(&mut self, origin: &Origin) -> Option<&mut SheetSession> {
        self.sync_kdm(origin, false);
        if self.link.kdm.is_none() {
            match SheetSession::create(&self.wb) {
                Ok(kdm) => {
                    self.link.kdm = Some(kdm);
                    self.link.synced = self.wb.clone();
                }
                Err(error) => {
                    log::warn!("the workbook can't be held in a replica: {error}");
                    return None;
                }
            }
        }
        self.link.kdm.as_mut()
    }

    /// Leaves the replica: every step's replica steps were the dropped replica's, and a fresh
    /// one never holds them.
    pub(crate) fn detach_kdm(&mut self) {
        self.link.kdm = None;
        self.link.resets = self.link.resets.saturating_add(1);
        self.link.synced = self.wb.clone();
        self.link.pending = 0;
        self.link.mark = self.link.pushed;
        for e in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            e.steps = 0;
        }
    }

    /// Lets the replica forget the steps the history can no longer take, and their blobs.
    pub(crate) fn trim_kdm(&mut self) {
        let sum = |v: &[HistoryEntry]| v.iter().map(|e| e.steps as usize).sum::<usize>();
        let undo = sum(&self.undo).saturating_add(self.link.pending as usize);
        let redo = sum(&self.redo);
        if let Some(kdm) = self.link.kdm.as_mut() {
            kdm.retain_steps(undo, redo);
        }
    }

    /// After undo or redo set the workbook to the step's own snapshot: steps the replica too
    /// (`steps` times) and takes its view (with every collaborator's edit since) instead, on the
    /// snapshot's active sheet. Without a replica the snapshot stays.
    ///
    /// The replica passes over a step whose objects are gone since (say an agent deleted the sheet
    /// a person's step edited) and goes on to the one below in the same call (`UndoOutcome`): so
    /// the calls may take back fewer of this step's replica steps than they took off the stack, or
    /// reach below it into the scope's next history steps. The history step, now on the other
    /// stack, counts the replica steps the calls left there, and the history steps below whose
    /// replica steps went with it leave their stack ([`absorb`]). A replica with nothing left to
    /// take stays as it is; only a replica that refuses is left.
    pub(crate) fn step_kdm(&mut self, steps: u32, undo: bool, scope: UndoScope) {
        if steps > 0
            && let Some(kdm) = self.link.kdm.as_mut()
        {
            let Some((made, beyond)) = take_steps(kdm, steps, undo, scope) else {
                log::warn!("the workbook replica refused an undo or redo");
                self.detach_kdm();
                return;
            };
            let (from, to) = if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
            if let Some(e) = to.iter_mut().rev().find(|e| e.scope == scope) {
                e.steps = made;
            }
            absorb(from, scope, beyond);
        }
        if let Some(kdm) = self.link.kdm.as_mut() {
            let mut wb = kdm.workbook().clone();
            let active = self.wb.active().and_then(|sheet| wb.sheet_index(&sheet.name));
            if let Some(i) = active
                && i != wb.active_sheet
            {
                wb.active_sheet = i;
                // This person's own: the view remembers it, and nothing is written.
                if let Err(error) = kdm.record(&Origin::system("view"), &wb, &[]) {
                    log::warn!("the workbook replica refused the active sheet: {error}");
                }
            }
            self.wb = Arc::new(wb);
        }
        self.link.synced = self.wb.clone();
        self.link.mark = self.link.pushed;
        self.trim_kdm();
    }

    /// A step under the other scope's later ones: the replica undoes (or redoes) the step's
    /// `steps` transactions of `scope` and the workbook becomes its view, with the other scope's
    /// later edits and every collaborator's. `None`, changing nothing, when the replica does not
    /// hold them; else the replica steps the calls left on the other stack, and how many they
    /// took off beyond this step's (passed over, [`DocState::step_kdm`]), for the caller, which
    /// moves the step to the other stack, gives the steps beyond to [`absorb`], then trims
    /// ([`DocState::trim_kdm`]).
    pub(crate) fn replay_kdm(&mut self, steps: u32, undo: bool, scope: UndoScope) -> Option<(u32, usize)> {
        let kdm = self.link.kdm.as_mut()?;
        let (available, can) = if undo { (kdm.steps().0, kdm.can_undo_in(scope)) } else { (kdm.steps().1, kdm.can_redo_in(scope)) };
        if steps == 0 || !can || usize::try_from(steps).unwrap_or(usize::MAX) > available {
            return None;
        }
        let Some(taken) = take_steps(kdm, steps, undo, scope) else {
            log::warn!("the workbook replica refused an undo or redo");
            self.detach_kdm();
            return None;
        };
        let mut wb = kdm.workbook().clone();
        let active = self.wb.active().and_then(|sheet| wb.sheet_index(&sheet.name));
        if let Some(i) = active
            && i != wb.active_sheet
        {
            wb.active_sheet = i;
            // This person's own: the view remembers it, and nothing is written.
            if let Err(error) = kdm.record(&Origin::system("view"), &wb, &[]) {
                log::warn!("the workbook replica refused the active sheet: {error}");
            }
        }
        self.wb = Arc::new(wb);
        self.link.synced = self.wb.clone();
        self.link.mark = self.link.pushed;
        Some(taken)
    }

    /// Where the selection is, by object (the replica's view is the workbook as of the last sync).
    fn place(&self) -> Option<Place> {
        let ids = self.link.kdm.as_ref()?.model().ids();
        let active = self.wb.active_sheet;
        let sheet = ids.sheets.get(active);
        Some(Place {
            sheets: ids.sheets.iter().map(|s| s.object.clone()).collect(),
            active,
            rows: sheet.map(|s| s.rows.to_vec()).unwrap_or_default(),
            cols: sheet.map(|s| s.cols.to_vec()).unwrap_or_default(),
        })
    }

    /// After an import: the workbook follows the replica (its view, recalculated), and the
    /// selection follows its sheet, rows and columns; each sheet's remembered selection follows
    /// its sheet.
    pub(crate) fn follow_kdm(&mut self, before: Option<Place>) {
        let Some(kdm) = self.link.kdm.as_ref() else { return };
        let wb = Arc::new(kdm.workbook().clone());
        let ids = kdm.model().ids();
        if let Some(before) = before {
            let mut memories = vec![Selection::default(); wb.sheets.len()];
            for (i, object) in before.sheets.iter().enumerate() {
                if let (Some(j), Some(sel)) = (ids.sheet_of(object), self.sheet_selections.get(i))
                    && let Some(slot) = memories.get_mut(j)
                {
                    *slot = sel.clone();
                }
            }
            let same_sheet = before.sheets.get(before.active).and_then(|o| ids.sheet_of(o)) == Some(wb.active_sheet);
            match ids.sheets.get(wb.active_sheet) {
                Some(after) if same_sheet => follow_lines(&mut self.selection, (&before.rows, &after.rows), (&before.cols, &after.cols)),
                _ if same_sheet => {}
                _ => self.selection = memories.get(wb.active_sheet).cloned().unwrap_or_default(),
            }
            self.sheet_selections = memories;
        }
        self.wb = wb;
        self.link.synced = self.wb.clone();
        self.calc.rebuild(&self.wb);
        self.revision += 1;
    }
}

/// Undoes (or redoes) `steps` replica steps of `scope`: calls the replica until the steps it took
/// off its stack (passed over or taken back, `UndoOutcome`) reach `steps`, or it has none left.
/// Answers how many it took back and how many it took off beyond `steps`; `None` when it refused.
fn take_steps(kdm: &mut SheetSession, steps: u32, undo: bool, scope: UndoScope) -> Option<(u32, usize)> {
    let want = steps as usize;
    let (mut consumed, mut made) = (0usize, 0u32);
    while consumed < want {
        let outcome = match if undo { kdm.undo_in(scope) } else { kdm.redo_in(scope) } {
            Ok((outcome, _)) => outcome,
            Err(_) => return None,
        };
        if outcome.steps() == 0 {
            break;
        }
        consumed += outcome.steps();
        made += u32::from(outcome.is_done());
    }
    Some((made, consumed.saturating_sub(want)))
}

/// Replica steps of `scope` taken off beyond a history step's own (the replica passed over its
/// step and went on below): the history steps of `scope` below, from the top, whose replica steps
/// went with it leave `stack`, and one only partly taken keeps the rest.
pub(crate) fn absorb(stack: &mut Vec<HistoryEntry>, scope: UndoScope, mut beyond: usize) {
    while beyond > 0 {
        let Some(i) = stack.iter().rposition(|e| e.scope == scope && e.steps > 0) else { break };
        let Some(below) = stack.get(i).map(|e| e.steps as usize) else { break };
        if below <= beyond {
            stack.remove(i);
            beyond -= below;
        } else {
            if let Some(e) = stack.get_mut(i) {
                e.steps -= u32::try_from(beyond).unwrap_or(0);
            }
            beyond = 0;
        }
    }
}

/// How one axis's lines moved: each line object's index before and after.
struct LineMap<'a> {
    before: &'a [ObjectId],
    after: HashMap<&'a ObjectId, usize>,
    /// The last line of `before` still there, with its index before and after.
    last: Option<(usize, usize)>,
    max: u32,
}

impl<'a> LineMap<'a> {
    /// `None` when nothing moved.
    fn new(before: &'a [ObjectId], after: &'a [ObjectId], max: u32) -> Option<Self> {
        if before == after {
            return None;
        }
        let after: HashMap<&ObjectId, usize> = after.iter().enumerate().map(|(i, o)| (o, i)).collect();
        let last = before.iter().enumerate().rev().find_map(|(i, o)| after.get(o).map(|&j| (i, j)));
        Some(Self { before, after, last, max })
    }

    /// Where line `i` is now: its object's index; for a deleted line, the line now in its place
    /// (the next one still there); past the lines that have objects, shifted as the last one was.
    fn map(&self, i: u32) -> u32 {
        let at = i as usize;
        let found = self.before.get(at..).and_then(|rest| rest.iter().find_map(|o| self.after.get(o).copied()));
        let moved = match (found, self.last) {
            (Some(j), _) => j as i64,
            (None, Some((k, j))) => j as i64 + (at as i64 - k as i64),
            (None, None) => at as i64,
        };
        u32::try_from(moved.clamp(0, i64::from(self.max) - 1)).unwrap_or(i)
    }
}

/// Moves `sel` along its rows and columns as an import moved them (`(before, after)` objects).
/// Whole rows and whole columns stay whole.
fn follow_lines(sel: &mut Selection, rows: (&[ObjectId], &[ObjectId]), cols: (&[ObjectId], &[ObjectId])) {
    let rows = LineMap::new(rows.0, rows.1, MAX_ROWS);
    let cols = LineMap::new(cols.0, cols.1, MAX_COLS);
    if rows.is_none() && cols.is_none() {
        return;
    }
    let row = |r: u32| rows.as_ref().map_or(r, |m| m.map(r));
    let col = |c: u32| cols.as_ref().map_or(c, |m| m.map(c));
    let cell = |c: CellRef| CellRef::new(row(c.row), col(c.col));
    sel.active = cell(sel.active);
    sel.anchor = cell(sel.anchor);
    for r in &mut sel.ranges {
        let (r0, r1) = if r.start.row == 0 && r.end.row == MAX_ROWS - 1 { (0, MAX_ROWS - 1) } else { (row(r.start.row), row(r.end.row)) };
        let (c0, c1) = if r.start.col == 0 && r.end.col == MAX_COLS - 1 { (0, MAX_COLS - 1) } else { (col(r.start.col), col(r.end.col)) };
        *r = RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1));
    }
    if sel.ranges.is_empty() {
        sel.ranges.push(RangeRef::cell(sel.active));
    }
    if let Some(last) = sel.ranges.last()
        && !last.contains(sel.active)
    {
        sel.active = last.start;
    }
}

impl Session {
    /// Imports a file difference without replacing this session's replica or undo histories.
    pub fn reconcile_workbook(&mut self, fresh: &Workbook) -> Result<()> {
        let st = self.doc_mut()?;
        let place = st.place();
        let kdm = st.link.kdm.as_mut().ok_or_else(no_replica)?;
        kdm.reconcile_file(fresh).map_err(kdm_error)?;
        st.follow_kdm(place);
        for sheet in &mut Arc::make_mut(&mut st.wb).sheets {
            if let Some(imported) = fresh.sheets.iter().find(|imported| imported.sheet_id == sheet.sheet_id) {
                let sheet = Arc::make_mut(sheet);
                sheet.view_top_left = imported.view_top_left;
                sheet.zoom = imported.zoom;
            }
        }
        st.link.synced = st.wb.clone();
        st.saved = st.wb.clone();
        Ok(())
    }

    /// The undo scope of who runs the next command ([`Session::origin`]): `edit.undo` and
    /// `edit.redo` work in it unless their `scope` parameter names another (engine rule E14a).
    pub fn undo_scope(&self) -> UndoScope {
        UndoScope::of(self.origin().kind).unwrap_or(UndoScope::Person)
    }

    /// Who the running (or next) command acts for: the origin of [`Session::execute_as`], or the
    /// local person.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(local_person)
    }

    /// Sets who the following commands act for (`None`: the local person); answers the previous
    /// setting, to put back.
    pub fn set_origin(&mut self, origin: Option<Origin>) -> Option<Origin> {
        std::mem::replace(&mut self.origin, origin)
    }

    /// Whether an undoable command is running (the commands it runs are part of its step).
    pub(crate) fn capturing(&self) -> bool {
        self.capturing
    }

    /// [`Session::execute`] as `origin` (a person, an agent): the replica records the edit as
    /// theirs, and it undoes as this replica's own step.
    pub fn execute_as(&mut self, id: &str, params: serde_json::Value, origin: &Origin) -> Result<serde_json::Value> {
        let previous = self.set_origin(Some(origin.clone()));
        let r = self.execute(id, params);
        self.set_origin(previous);
        r
    }

    /// After a command: replicates every document's changes. A replica whose write panicked is
    /// left (a reset), never the session.
    pub(crate) fn sync_all_kdm(&mut self, captured: bool) {
        let origin = self.origin();
        for st in &mut self.docs {
            let synced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| st.sync_kdm(&origin, captured)));
            if synced.is_err() {
                st.detach_kdm();
            }
        }
    }

    /// The active document's replica, if it has made one (never makes one).
    pub fn kdm(&self) -> Option<&SheetSession> {
        self.active()?.kdm()
    }

    /// How many times the active document left its replica. Hosts watch this: the next edit's
    /// fresh replica is a new KDM document collaborators must join from its snapshot.
    pub fn kdm_resets(&self) -> u64 {
        self.active().map_or(0, DocState::kdm_resets)
    }

    /// The active document's KDM identity, `None` before it has made a replica.
    pub fn doc_id(&self) -> Option<String> {
        Some(self.kdm()?.doc_id().to_string())
    }

    fn active_kdm(&mut self) -> Result<&mut SheetSession> {
        let origin = self.origin();
        self.doc_mut()?.ensure_kdm(&origin).ok_or_else(no_replica)
    }

    /// The active document's replica version.
    pub fn version(&mut self) -> Result<Version> {
        Ok(self.active_kdm()?.version())
    }

    /// The active document's replica as a snapshot, for a peer ([`Session::open_snapshot`]).
    pub fn snapshot(&mut self) -> Result<Vec<u8>> {
        self.active_kdm()?.snapshot().map_err(kdm_error)
    }

    /// SHA-256 of the active document's replicated state: equal on replicas that converged.
    pub fn state_hash(&mut self) -> Result<String> {
        self.active_kdm()?.state_hash().map_err(kdm_error)
    }

    /// The active document's changes since `since` (`Version::empty()` for all of them).
    pub fn export_changes(&mut self, since: &Version) -> Result<Vec<u8>> {
        self.active_kdm()?.export_changes(since).map_err(kdm_error)
    }

    /// Every blob the active document's replica references or can still bring back.
    pub fn blobs(&mut self) -> Result<MemoryBlobs> {
        Ok(self.active_kdm()?.blobs().clone())
    }

    /// Adds a peer's blobs (before importing its changes): kept until the next import.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) -> Result<()> {
        let origin = self.origin();
        let st = self.doc_mut()?;
        st.ensure_kdm(&origin).ok_or_else(no_replica)?;
        let place = st.place();
        let kdm = st.link.kdm.as_mut().ok_or_else(no_replica)?;
        kdm.add_blobs(blobs);
        // A view left stale by changes that arrived before their blobs follows the replica now.
        if kdm.view_stale() && kdm.retry_view().unwrap_or(false) {
            st.follow_kdm(place);
        }
        Ok(())
    }

    /// The blobs a peer at `since` needs (`Version::empty()`: every blob the current state
    /// references), so a live change ships only its own pictures.
    pub fn blobs_since(&mut self, since: &Version) -> Result<MemoryBlobs> {
        Ok(MemoryBlobs(self.active_kdm()?.blobs_since(since).into_iter().collect()))
    }

    /// Imports a collaborator's (or stored) changes into the active document's replica (their
    /// blobs added first, [`Session::add_blobs`]). Local edits not written yet are written first.
    /// The workbook (recalculated) and the selection follow the replica; one whose view could not
    /// be loaded from it leaves it (a reset) rather than diff against it later.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        let origin = self.origin();
        let st = self.doc_mut()?;
        st.ensure_kdm(&origin).ok_or_else(no_replica)?;
        let place = st.place();
        let kdm = st.link.kdm.as_mut().ok_or_else(no_replica)?;
        let before = kdm.version();
        let result = kdm.import_changes(bytes);
        let changed = kdm.version() != before;
        let follow = result.is_ok() || kdm.view_loaded();
        if changed && follow {
            st.follow_kdm(place);
            let mut canonical = (*st.wb).clone();
            if kition_sheet_xlsx::normalize_file_units(&mut canonical) {
                st.wb = std::sync::Arc::new(canonical);
                st.sync_kdm(&Origin::system("file units"), false);
            }
        } else if changed && kdm.view_stale() && kdm.missing_blobs() {
            // The changes reference blobs still on their way: the replica keeps them and the view
            // stays as it was until `add_blobs` brings what it lacks (architecture section 16,
            // "missing blobs arrive late without corrupting the document").
            st.trim_kdm();
            return Ok(Damage::None);
        } else if changed {
            st.detach_kdm();
        }
        st.trim_kdm();
        result.map_err(kdm_error)
    }

    /// Takes a draft's changes (made on a replica forked from this one, `mergeChanges`) into the
    /// active workbook as one undo step of the person who accepts it, even when `origin` is an
    /// agent's (KitModel M12a: the replica records it in the person's history too), as a command's
    /// edit is: local edits not written yet are written first, the workbook (recalculated) and its
    /// selection follow the replica, and one Undo takes the whole draft back. Add the draft's blobs
    /// first ([`Session::add_blobs`]). A refused merge changes nothing. Answers the undo scope of
    /// the step (`None`: the draft changed nothing) and the damage.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<(Option<UndoScope>, Damage)> {
        self.take_in("Merge Draft", |kdm| kdm.merge_changes(bytes, origin))
    }

    /// Writes the state a snapshot holds (a checkpoint of this workbook, `replica/restore`) over
    /// the active workbook by object id, as one undo step of `origin`'s scope, taken in as a
    /// merged draft is ([`Session::merge_changes`]). Nothing changes when the states are equal.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage> {
        self.take_in("Restore Version", |kdm| kdm.restore_state(snapshot, origin)).map(|(_, damage)| damage)
    }

    /// One transaction `write` makes on the active workbook's replica, taken into the workbook as
    /// a command's step: the workbook follows the replica, and the history gets the step (its undo
    /// steps the replica's) when the transaction is one.
    fn take_in(
        &mut self,
        label: &str,
        write: impl FnOnce(&mut SheetSession) -> kdm_model::kition_kit::session::Result<(Option<UndoScope>, Damage)>,
    ) -> Result<(Option<UndoScope>, Damage)> {
        let now = self.origin();
        let st = self.doc_mut()?;
        st.sync_kdm(&now, false);
        st.ensure_kdm(&now).ok_or_else(no_replica)?;
        let place = st.place();
        let (before, selection) = (st.wb.clone(), st.selection.clone());
        let kdm = st.link.kdm.as_mut().ok_or_else(no_replica)?;
        let version = kdm.version();
        let result = write(kdm);
        let changed = kdm.version() != version;
        let follow = changed && (result.is_ok() || kdm.view_loaded());
        if follow {
            st.follow_kdm(place);
        } else if changed {
            st.detach_kdm();
        }
        // The step goes in the scope the replica recorded it in (a merged draft: the person's,
        // even when an agent made it).
        if follow && let Ok((Some(scope), _)) = &result {
            st.push_undo(HistoryEntry { label: label.to_owned(), wb: before, selection, steps: 1, scope: *scope });
            // The step is written already: the bridge has nothing of it left to record.
            st.link.mark = st.link.pushed;
        }
        st.trim_kdm();
        result.map_err(kdm_error)
    }

    /// A title no open untitled workbook has (`BookN`).
    fn next_untitled(&self) -> String {
        let n = (1..).find(|n| !self.docs.iter().any(|d| d.title == format!("Book{n}") && d.path.is_none())).unwrap_or(1);
        format!("Book{n}")
    }

    /// Opens another replica's snapshot (with the blobs it references) as a new document, a
    /// replica of the same KDM document under a fresh actor, and makes it active: clean, with an
    /// empty history.
    pub fn open_snapshot(&mut self, bytes: &[u8], blobs: MemoryBlobs) -> Result<usize> {
        let kdm = SheetSession::open_snapshot(bytes, blobs).map_err(kdm_error)?;
        let title = self.next_untitled();
        let mut st = DocState::calculated(kdm.workbook().clone(), None, title);
        st.link.kdm = Some(kdm);
        Ok(self.add_document(st))
    }

    /// [`Session::open_snapshot`] for a host that keeps a document's replica beside its saved
    /// file `name`. The file is what the person saved: when `file` is not empty and the replica
    /// does not hold its content (id-blind), the file wins, recorded onto the replica as one
    /// `import` transaction (not an undo step) under the replica's own identity, so collaborators
    /// get it as ordinary changes. Either way the file's own view state (its active sheet, each
    /// sheet's zoom, active cell and scroll position) is this person's. The document opens clean.
    pub fn open_with_replica(&mut self, name: &str, file: &[u8], snapshot: &[u8], blobs: MemoryBlobs) -> Result<usize> {
        let loaded = if file.is_empty() { None } else { Some(crate::io::open_bytes(name, file)?.0) };
        let index = self.open_snapshot(snapshot, blobs)?;
        let st = self.doc_mut()?;
        if let Some(mut wb) = loaded {
            let kdm = st.link.kdm.as_mut().ok_or_else(no_replica)?;
            if file_matches(kdm, name, &wb)? {
                let mut view = kdm.workbook().clone();
                take_view_state(&wb, &mut view);
                kdm.record(&Origin::system("view"), &view, &[]).map_err(kdm_error)?;
                st.wb = Arc::new(view);
            } else {
                st.calc.recalc_all(&mut wb);
                kdm.record(&Origin::system("import"), &wb, &[]).map_err(kdm_error)?;
                st.wb = Arc::new(wb);
            }
            st.calc.rebuild(&st.wb);
            st.saved = st.wb.clone();
            st.link.synced = st.wb.clone();
            st.selection = Selection::at(st.wb.active().map(|s| s.view_active).unwrap_or_default());
            st.sheet_selections.clear();
            st.revision += 1;
        }
        Ok(index)
    }
}

/// Whether `file` (read from the file `name`) is what the replica holds: its content, or (`.xlsx`
/// and the other formats keep less than the replica, and lose nothing it holds twice) what the
/// replica's view saves as `name` and reads back, compared id-blind without formula results or
/// view state ([`kdm_model::normalize`]).
fn file_matches(kdm: &SheetSession, name: &str, file: &Workbook) -> Result<bool> {
    if kdm.holds(file).map_err(kdm_error)? {
        return Ok(true);
    }
    let saved = crate::io::save_bytes(kdm.workbook(), name)?;
    let (again, _) = crate::io::open_bytes(name, &saved)?;
    Ok(kdm_model::normalize(&again) == kdm_model::normalize(file))
}

/// Gives `to` the view state `from` has for the sheets both have by name: the active sheet, and
/// each sheet's zoom, active cell and scroll position.
fn take_view_state(from: &Workbook, to: &mut Workbook) {
    for sheet in &from.sheets {
        if let Some(i) = to.sheet_index(&sheet.name)
            && let Some(target) = to.sheet_mut(i)
        {
            target.zoom = sheet.zoom;
            target.view_active = sheet.view_active;
            target.view_top_left = sheet.view_top_left;
        }
    }
    if let Some(i) = from.active().and_then(|sheet| to.sheet_index(&sheet.name)) {
        to.active_sheet = i;
    }
}
