//! Kition's collaboration members (engine contract 2, Kition phase 3f Task 2): the workbook as a
//! KDM replica.
//!
//! The engine keeps its own `Session` (the command bridge to the replica,
//! `kition_sheet_engine::kdm_bridge`), not a `kition_kit::session::KitSession` directly, so
//! `kition_kit::export_kit_session!` does not apply here; these members are added by hand with
//! the exact JS names and shapes `kition-sdk`'s `session::web` gives
//! (`crates/kition-kit/src/session/web.rs`), as Kition Slide's are, so Kition's host code is the
//! same for every kit. The wasm bindings live in [`crate::web`].
//!
//! Workbooks embed pictures (images on a sheet, pictures in cells) in the replica, so
//! [`SheetDocument::blobs`], [`SheetDocument::add_blobs`] and [`SheetDocument::blobs_since`] move
//! real content, encoded the same way Kition Slide's and Photo's do:
//! `(64-char ASCII-hex sha256, u64 LE length, bytes) * N`.

use std::collections::HashMap;
use std::sync::Arc;

use kition_kit::KitDocument;
use kition_sheet_engine::{Damage, MemoryBlobs, Origin, Session, Version};
use serde_json::{Value, json};

use crate::{SheetDocument, engine_error};

impl SheetDocument {
    /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
    /// capability: `.xlsx` cannot carry its own replica): the workbook is the replica's, so it
    /// keeps its KDM identity across saves. The host vouches that the replica was saved with these
    /// bytes; when they disagree (an older save, a workbook edited outside the replica) the engine
    /// records the file on top as one import. A replica that does not open (damaged, a newer
    /// schema) is ignored and the file opens as [`SheetDocument::open`] opens it.
    ///
    /// Empty `bytes` mean there is no file yet, only the replica (a host opening a shared workbook
    /// for the first time from its sync room's snapshot, to save it as a new file): the workbook
    /// is the replica's alone.
    pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        let restored = (|| -> Result<Session, String> {
            let blobs = decode_blobs(blobs)?;
            let mut session = Session::new();
            session.open_with_replica(name, bytes, snapshot, blobs).map_err(engine_error)?;
            // Titled as `open` titles it.
            let title = std::path::Path::new(name).file_stem().map_or_else(|| "Book".to_owned(), |stem| stem.to_string_lossy().into_owned());
            session.doc_mut().map_err(engine_error)?.title = title;
            Ok(session)
        })();
        match restored {
            Ok(session) => Self::from_session(session),
            Err(_) => Self::open(name, bytes),
        }
    }

    /// Whether the workbook has made its replica yet: it makes one lazily, on the first shared
    /// edit or when asked for it ([`SheetDocument::version`] and the rest).
    pub fn has_replica(&self) -> bool {
        self.session.active().is_some_and(|state| state.kdm().is_some())
    }

    /// The replica's own document id (a `kition-sdk` `DocId`, stable across saves and across
    /// opening another replica of the same workbook, never the file's name): Kit UI's generic
    /// runtime reads this for the Kit UI protocol's `replica/info` message. `None` (`undefined`
    /// in JS, through [`crate::web`]) before the workbook has made a replica
    /// ([`SheetDocument::has_replica`]).
    pub fn doc_id(&self) -> Option<String> {
        self.session.doc_id()
    }

    /// The workbook's replica version.
    pub fn version(&mut self) -> Result<Vec<u8>, String> {
        Ok(self.session.version().map_err(engine_error)?.encode())
    }

    /// The workbook's changes since `since` (empty bytes: every change).
    pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        self.session.export_changes(&since).map_err(engine_error)
    }

    /// Imports a peer's (or stored) changes; add their blobs first with
    /// [`SheetDocument::add_blobs`]. Answers what went stale on screen.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, String> {
        self.session.import_changes(bytes).map_err(engine_error)
    }

    /// The workbook's replica as a snapshot, for a peer to open as a new replica of the same
    /// workbook ([`SheetDocument::open`]'ing the saved bytes does not: `.xlsx` carries no
    /// replica).
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        self.session.snapshot().map_err(engine_error)
    }

    /// Undoes the person's last step on the workbook's replica. `done` is `false`, not an error,
    /// when there is nothing to undo.
    pub fn undo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_undo();
        if done {
            self.execute("edit.undo", json!({ "scope": "person" }))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn redo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_redo();
        if done {
            self.execute("edit.redo", json!({ "scope": "person" }))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    /// Whether the person has a step to undo (engine rule E14a: the person's).
    pub fn can_undo(&self) -> bool {
        self.session.active().is_some_and(|state| state.undo.iter().any(|u| u.scope == kition_sheet_engine::UndoScope::Person))
    }

    pub fn can_redo(&self) -> bool {
        self.session.active().is_some_and(|state| state.redo.iter().any(|u| u.scope == kition_sheet_engine::UndoScope::Person))
    }

    /// Runs a command as `origin` (a person, an agent): the replica records it as theirs, and it
    /// undoes as this replica's own step. Contract 1's
    /// [`SheetDocument::execute`](kition_kit::KitDocument::execute) is this with the local
    /// person (`command.runLine` included).
    pub fn execute_as(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        let previous = self.session.set_origin(Some(origin.clone()));
        let result = self.execute(command, params);
        self.session.set_origin(previous);
        result
    }

    /// Every blob the workbook's replica references or can still bring back, as one buffer a peer
    /// feeds straight into [`SheetDocument::add_blobs`].
    pub fn blobs(&mut self) -> Result<Vec<u8>, String> {
        let blobs = self.session.blobs().map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// Adds a peer's blobs (from their [`SheetDocument::blobs`]): kept until the next
    /// [`SheetDocument::import_changes`], which a host calls right after.
    pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), String> {
        let blobs = decode_blobs(bytes)?;
        self.session.add_blobs(&blobs).map_err(engine_error)
    }

    /// The blobs the workbook's replica has newly needed since `since` (empty bytes: every blob
    /// the current state references), encoded the same way as [`SheetDocument::blobs`]: for live
    /// sync, so one change ships only the blobs it needs rather than the whole store.
    pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        let blobs = self.session.blobs_since(&since).map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// The replicated state, the replica made first if there is none yet.
    fn replicated_state(&mut self) -> Result<Value, String> {
        self.session.version().map_err(engine_error)?;
        let kdm = self.session.kdm().ok_or("the workbook has no replica")?;
        kdm.replica().state_json().map_err(engine_error)
    }

    /// Takes a draft's changes (from a replica forked from this one) in as one undo step of the
    /// person (`mergeChanges`, as `kition-sdk`'s `export_kit_session!` answers it): the
    /// engine's workbook follows, and one Undo takes the whole draft back. Add the draft's blobs
    /// first. Answers `{"damage", "summary", "scope"}`, `summary` the `StateDiff` of the replicated
    /// state before and after, `scope` the undo scope the replica recorded the step in (`"person"`,
    /// also for an agent's draft; `null` for none). A refused merge changes nothing and fails with
    /// the KDM's message.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Value, String> {
        let before = self.replicated_state()?;
        let (scope, damage) = self.session.merge_changes(bytes, origin).map_err(engine_error)?;
        let after = self.replicated_state()?;
        Ok(json!({
            "damage": damage,
            "summary": kition_kit::kdm::diff::diff_states(&before, &after).to_json(),
            "scope": scope.map(kition_kit::kdm::UndoScope::as_str),
        }))
    }

    /// What merging a draft's changes would change in the workbook now, as `StateDiff` JSON, the
    /// merge's repair included (`SheetSession::compare_changes`), so the preview is what accepting
    /// the draft gives; nothing changes. Refuses what [`SheetDocument::merge_changes`] refuses.
    pub fn compare_changes(&self, bytes: &[u8]) -> Result<Value, String> {
        let kdm = self.session.kdm().ok_or("the workbook has no replica, so no draft of it")?;
        Ok(kdm.compare_changes(bytes).map_err(engine_error)?.to_json())
    }

    /// Writes the state a snapshot holds (a checkpoint of this workbook, the Kit UI protocol's
    /// `replica/restore`) over the workbook by object id, as one undo step of `origin`'s scope:
    /// the engine's workbook follows. Add the blobs it references first. Nothing changes when the
    /// states are equal; another document's snapshot is refused.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage, String> {
        self.session.restore_state(snapshot, origin).map_err(engine_error)
    }

    /// An agent's reply (JSON text) with every object id it holds as a value or a key named as
    /// the tool interface names it (its tool id, KitObjectModel OM9); unchanged without a replica
    /// or when the schema declares no tool ids. The workbook keeps its own ids.
    ///
    /// The editor asks this for every agent reply (each `ai/inspect`, `document/check` and
    /// `ai/run`): the tool ids are read for the objects the reply names alone
    /// (`ToolIds::show_reply`, through `Replica::tool_ids_of`), not for the whole workbook, whose
    /// hundreds of thousands of cells would take a walk of every object each time. Only a reply
    /// naming an object the SDK cannot name on its own (one named by where it is held) takes the
    /// whole workbook's, read once per version.
    pub fn model_reply(&self, value: &str) -> Result<String, String> {
        let value: Value = serde_json::from_str(value).map_err(|error| format!("the reply is not JSON: {error}"))?;
        let Some(kdm) = self.session.kdm() else { return Ok(value.to_string()) };
        Ok(kition_kit::kdm::tool_ids::ToolIds::show_reply(kdm.replica(), &value).to_string())
    }

    /// What changed since the version `since` (empty bytes: since the replica began), a receipt as
    /// `kition-sdk`'s `changesSince` answers it: `{"changes": StateDiff}`, or `{"changes": null,
    /// "unavailable": "<why>"}` when the replica's log does not hold `since` (or there is no
    /// replica yet).
    pub fn changes_since(&self, since: &[u8]) -> Value {
        let Some(kdm) = self.session.kdm() else { return json!({ "changes": null, "unavailable": "the workbook has no replica yet" }) };
        let replica = kdm.replica();
        let since = if since.is_empty() { Ok(Version::empty()) } else { Version::decode(since) };
        match since.and_then(|since| replica.changes_since(&since)) {
            Ok(changes) => json!({ "changes": changes.to_recorded_json(replica.schema()) }),
            Err(error) => json!({ "changes": null, "unavailable": error.to_string() }),
        }
    }

    /// `model/describe` (KitObjectModel 1.0) over the change exchange, as the headless kit serves it.
    pub fn model_describe(&mut self) -> Result<Value, String> {
        kition_kit::session::model::Exchange::new().describe(self).map_err(|error| error.to_json().to_string())
    }

    /// `model/batch` over the change exchange; a failure's message is the error's JSON
    /// (`{"code", "index", "message"}`).
    pub fn model_batch(&mut self, request: &Value) -> Result<Value, String> {
        kition_kit::session::model::Exchange::new().batch(self, request, false).map_err(|error| error.to_json().to_string())
    }

    /// How many times the workbook left its replica because it refused a write or an undo (a
    /// workbook it cannot hold, or a bug). The workbook keeps working, undoing from its own
    /// history; the next shared edit makes a fresh replica, which collaborators must resync (its
    /// full [`SheetDocument::snapshot`]). Hosts watch this.
    pub fn kdm_resets(&self) -> u32 {
        u32::try_from(self.session.kdm_resets()).unwrap_or(u32::MAX)
    }
}

/// Encodes blobs for one wasm call's worth of blob transfer
/// ([`SheetDocument::blobs`]/[`SheetDocument::add_blobs`]): `(64-byte ASCII-hex hash, u64 LE
/// length, bytes) * N`. Every hash the KDM model produces is a 64-char hex SHA-256, so the hash
/// field needs no length prefix of its own.
fn encode_blobs(blobs: &MemoryBlobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, data) in &blobs.0 {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Reverses [`encode_blobs`]; truncated or malformed input is an error, never a panic.
fn decode_blobs(bytes: &[u8]) -> Result<MemoryBlobs, String> {
    let mut map = HashMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let hash_end = i.checked_add(64).ok_or("blob bundle: overflow")?;
        let hash = bytes.get(i..hash_end).ok_or("truncated blob bundle: hash")?;
        let hash = std::str::from_utf8(hash).map_err(|_| "blob bundle: hash is not ASCII".to_owned())?.to_owned();
        let len_end = hash_end.checked_add(8).ok_or("blob bundle: overflow")?;
        let len_bytes: [u8; 8] =
            bytes.get(hash_end..len_end).ok_or("truncated blob bundle: length")?.try_into().map_err(|_| "blob bundle: length".to_owned())?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| "blob bundle: length too large".to_owned())?;
        let data_end = len_end.checked_add(len).ok_or("blob bundle: overflow")?;
        let data = bytes.get(len_end..data_end).ok_or("truncated blob bundle: data")?;
        map.insert(hash, Arc::from(data));
        i = data_end;
    }
    Ok(MemoryBlobs(map))
}

/// The object model (`KitObjectModel 1.0`) over the document's change exchange: `kition-sheet` serves
/// `model.describe` and `model.batch` headless through `kition_kit::session::model::Exchange`.
impl kition_kit::session::model::ReplicaPort for SheetDocument {
    fn model_schema(&self) -> std::sync::Arc<kition_kit::kdm::Schema> {
        <kdm_model::SheetModel as kition_kit::session::KitModel>::schema()
    }

    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.export_changes(since)
    }

    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.import_changes(bytes).map(|_| ())
    }

    fn model_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.execute_as(command, params, origin)
    }

    fn model_commands(&self) -> Vec<kition_kit::KitCommand> {
        self.commands()
    }

    /// A shared workbook with no file yet (a server's headless kit opening a sync room): the
    /// replica alone, failing rather than falling back to an empty workbook.
    fn model_open_replica(name: &str, snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        let blobs = decode_blobs(blobs)?;
        let mut session = Session::new();
        session.open_with_replica(name, &[], snapshot, blobs).map_err(engine_error)?;
        let title = std::path::Path::new(name).file_stem().map_or_else(|| "Book".to_owned(), |stem| stem.to_string_lossy().into_owned());
        session.doc_mut().map_err(engine_error)?.title = title;
        Self::from_session(session)
    }
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use serde_json::json;

    use super::*;

    fn fixture() -> Vec<u8> {
        let mut doc = SheetDocument::create("Budget.xlsx", 0, 0).unwrap();
        for (cell, input) in [("A1", "Item"), ("B1", "Cost"), ("A2", "Rent"), ("B2", "1200"), ("A3", "Food"), ("B3", "450"), ("B4", "=SUM(B2:B3)")] {
            doc.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
        }
        doc.save(None).unwrap()
    }

    fn extra_edit(doc: &mut SheetDocument) {
        doc.execute("cell.set", json!({ "cell": "C2", "input": "=B2*12" })).unwrap();
        doc.execute("format.bold", json!({ "range": "A1:B1", "on": true })).unwrap();
    }

    fn cell(doc: &mut SheetDocument, a1: &str) -> Value {
        doc.execute("cell.get", json!({ "cell": a1 })).unwrap()
    }

    fn picture() -> String {
        // A 1x1 PNG.
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==".to_owned()
    }

    #[test]
    fn version_and_snapshot_force_a_replica_lazily() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        assert!(!doc.has_replica(), "opening an .xlsx plainly makes no replica");
        let version = doc.version().unwrap();
        assert!(doc.has_replica());
        assert_eq!(doc.version().unwrap(), version, "asking twice makes no further replica");
        let snap = doc.snapshot().unwrap();
        assert!(!snap.is_empty());
    }

    #[test]
    fn undo_redo_report_done_and_damage() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        assert!(!doc.can_undo());
        doc.execute("cell.set", json!({ "cell": "B2", "input": "1300" })).unwrap();
        assert!(doc.can_undo());
        let (done, damage) = doc.undo().unwrap();
        assert!(done);
        assert_ne!(damage, Damage::None);
        assert_eq!(cell(&mut doc, "B2")["value"], json!(1200.0));
        assert_eq!(cell(&mut doc, "B4")["value"], json!(1650.0), "the undo's result is recalculated");
        assert!(!doc.can_undo());
        assert!(doc.can_redo());
        let (done, _) = doc.redo().unwrap();
        assert!(done);
        assert_eq!(cell(&mut doc, "B2")["value"], json!(1300.0));
        // Undoing with nothing to undo is a normal `false`, not an error.
        doc.undo().unwrap();
        let (done, damage) = doc.undo().unwrap();
        assert!(!done);
        assert_eq!(damage, Damage::None);
    }

    #[test]
    fn execute_as_records_the_given_origin() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let origin = Origin::person("user:agent-1");
        doc.execute_as("cell.set", json!({ "cell": "D1", "input": "agent" }), &origin).unwrap();
        assert!(doc.can_undo());
        doc.execute_as("command.runLine", json!({ "text": "typed" }), &origin).unwrap();
        assert_eq!(doc.session.origin(), kition_sheet_engine::kdm_bridge::local_person(), "the origin is restored after the call");
    }

    /// `docId` is `None` before any replica exists, and the replica's own id (not the file's
    /// `name`, and stable across a save/reopen with that same replica) once one does.
    #[test]
    fn doc_id_is_none_until_a_replica_exists_then_the_replicas_own_id() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        assert_eq!(doc.doc_id(), None, "no replica yet");
        doc.version().unwrap(); // makes the replica lazily
        let id = doc.doc_id().unwrap_or_else(|| panic!("doc_id should be set once a replica exists"));
        assert!(!id.is_empty());
        assert_eq!(doc.doc_id().as_deref(), Some(id.as_str()), "stable across further asks");

        let bytes = doc.save(None).unwrap();
        let snapshot = doc.snapshot().unwrap();
        let blobs = doc.blobs().unwrap();
        let reopened = SheetDocument::open_with_replica("Budget.xlsx", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(reopened.doc_id(), Some(id), "the reopened replica keeps the same document id");
    }

    /// With no file yet (empty bytes), the replica alone opens: a peer's full change export
    /// becomes the same workbook, same identity, and it saves as a new `.xlsx`.
    #[test]
    fn a_replica_opens_without_a_file() {
        let mut a = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        extra_edit(&mut a);
        let changes = a.export_changes(&[]).unwrap();
        let mut b = SheetDocument::open_with_replica("Shared.xlsx", &[], &changes, &a.blobs().unwrap()).unwrap();
        assert_eq!(b.doc_id(), a.doc_id());
        assert_eq!(b.version().unwrap(), a.version().unwrap());
        assert!(!b.dirty());
        assert_eq!(cell(&mut b, "C2")["value"], json!(14400.0), "the view is recalculated");
        assert!(SheetDocument::open("Shared.xlsx", &b.save(None).unwrap()).is_ok());
        assert!(SheetDocument::open_with_replica("Shared.xlsx", &[], b"not a replica", &[]).is_err());
    }

    #[test]
    fn blobs_round_trip_through_encode_and_decode() {
        let mut a = MemoryBlobs::default();
        a.0.insert("a".repeat(64), Arc::from(b"hello".as_slice()));
        a.0.insert("b".repeat(64), Arc::from(b"".as_slice()));
        let bytes = encode_blobs(&a);
        let b = decode_blobs(&bytes).unwrap();
        assert_eq!(b.0.len(), 2);
        assert_eq!(b.0.get(&"a".repeat(64)).map(|v| v.as_ref()), Some(b"hello".as_ref()));
        assert_eq!(b.0.get(&"b".repeat(64)).map(|v| v.as_ref()), Some(b"".as_ref()));
    }

    #[test]
    fn decode_blobs_rejects_truncated_input_without_panicking() {
        assert!(decode_blobs(&[1, 2, 3]).is_err());
        assert!(decode_blobs(&"a".repeat(64).into_bytes()).is_err(), "hash with no length");
        let mut malformed = "a".repeat(64).into_bytes();
        malformed.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_blobs(&malformed).is_err(), "a length claiming more data than exists");
    }

    /// An `.xlsx` saved with its replica reopens as that replica: same version, same rendered
    /// pixels, not dirty, and it takes new edits (which undo) at once.
    #[test]
    fn a_workbook_reopens_with_its_stored_replica() {
        let mut a = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        let version = a.version().unwrap();

        let mut b = SheetDocument::open_with_replica("Budget.xlsx", &bytes, &snapshot, &blobs).unwrap();
        assert!(b.has_replica());
        assert_eq!(b.version().unwrap(), version, "the file is what the replica saves: nothing recorded");
        assert_eq!(b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap(), a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap());
        assert!(!b.dirty());
        assert!(!b.can_undo());
        extra_edit(&mut b);
        b.execute("cell.set", json!({ "cell": "A5", "input": "more" })).unwrap();
        assert!(b.dirty());
        assert_ne!(b.version().unwrap(), version);
        assert!(b.undo().unwrap().0);
        assert!(SheetDocument::open("Budget.xlsx", &b.save(None).unwrap()).is_ok(), "it still saves as .xlsx");
    }

    /// A file changed outside its stored replica wins: it opens with the file's content, recorded
    /// onto the replica (same identity), clean and with nothing to undo.
    #[test]
    fn a_file_changed_elsewhere_is_recorded_onto_its_replica() {
        let mut a = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        let mut other = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        other.execute("cell.set", json!({ "cell": "A2", "input": "Mortgage" })).unwrap();
        let changed = other.save(None).unwrap();
        let mut b = SheetDocument::open_with_replica("Budget.xlsx", &changed, &snapshot, &blobs).unwrap();
        assert_eq!(b.doc_id(), a.doc_id());
        assert_eq!(cell(&mut b, "A2")["value"], json!("Mortgage"));
        assert!(!b.dirty());
        assert!(!b.can_undo());
        assert_ne!(b.version().unwrap(), a.version().unwrap(), "the file was recorded");
    }

    /// A damaged (or missing) stored replica never stops the file from opening: it opens plainly,
    /// never as a blank workbook.
    #[test]
    fn a_damaged_stored_replica_opens_the_file_plainly() {
        let bytes = fixture();
        for (snapshot, blobs) in [(&b"not a snapshot"[..], &[][..]), (&[][..], &[1u8, 2, 3][..])] {
            let mut doc = SheetDocument::open_with_replica("Budget.xlsx", &bytes, snapshot, blobs).unwrap();
            assert!(!doc.has_replica(), "the file opened plainly");
            assert_eq!(cell(&mut doc, "A2")["value"], json!("Rent"));
        }
        assert!(SheetDocument::open_with_replica("Budget.xlsx", b"not a workbook", b"not a snapshot", &[]).is_err());
    }

    /// `a` saves with its replica; `b` opens with it, so both start from the same replica
    /// identity. `a` edits further (a row insert, a picture in a cell);
    /// `b.importChanges(a.exportChanges(b.version()))` brings `b` to the same replicated state as
    /// `a`, through the public SDK surface alone.
    #[test]
    fn a_document_follows_another_through_export_and_import_changes() {
        let mut a = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        extra_edit(&mut a);
        let bytes = a.save(None).unwrap();
        let snapshot = a.snapshot().unwrap();
        let blobs = a.blobs().unwrap();

        let mut b = SheetDocument::open_with_replica("Budget.xlsx", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(b.version().unwrap(), a.version().unwrap(), "b starts from a's own replica");

        a.execute("row.insert", json!({ "rows": "2:2" })).unwrap();
        a.execute("image.insert", json!({ "placement": "cell", "at": "D2", "base64": picture(), "alt": "dot" })).unwrap();

        let since = b.version().unwrap();
        let changes = a.export_changes(&since).unwrap();
        b.add_blobs(&a.blobs_since(&since).unwrap()).unwrap();
        b.import_changes(&changes).unwrap();

        assert_eq!(a.version().unwrap(), b.version().unwrap());
        assert_eq!(cell(&mut b, "B5")["formula"], json!("=SUM(B3:B4)"), "the total still sums the same rows");
        assert_eq!(cell(&mut b, "D2")["type"], json!("picture"), "the picture's blob came with its change");
        assert_eq!(
            a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap(),
            b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap(),
            "both replicas render the same workbook"
        );
    }

    /// A value of the first sheet, read without moving the person.
    fn value(doc: &mut SheetDocument, a1: &str) -> Value {
        cell(doc, a1)["value"].clone()
    }

    /// The replicated objects and the engine's own workbook (id-blind), to compare whole states.
    fn whole(doc: &mut SheetDocument) -> (Value, Value) {
        let objects = doc.replicated_state().unwrap()["objects"].clone();
        let wb = doc
            .session
            .active()
            .map(|state| serde_json::to_value(kdm_model::normalize(&state.wb)).unwrap())
            .unwrap_or_else(|| panic!("an active workbook"));
        (objects, wb)
    }

    /// The names the workbook defines, sorted.
    fn names(doc: &mut SheetDocument) -> Vec<String> {
        let listed = doc.execute("name.manage", json!({})).unwrap();
        let mut names: Vec<String> = listed.as_array().into_iter().flatten().filter_map(|n| n["name"].as_str().map(str::to_owned)).collect();
        names.sort();
        names
    }

    /// A draft (a replica forked from the workbook's) merges in as one step of the person's Undo:
    /// the comparison beforehand names what the merge does and changes nothing, the engine's
    /// workbook follows the merge, an edit made here meanwhile stays (a name defined on each side:
    /// both are in the workbook's list of names), Undo gives back the exact state before (the
    /// replica's objects and the engine's workbook) and Redo the merged one, and a refused merge
    /// changes nothing.
    #[test]
    fn a_draft_merges_in_as_one_undo_step_and_the_workbook_follows() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = SheetDocument::open_with_replica("Budget.xlsx", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        draft.execute("cell.set", json!({ "cell": "B3", "input": "500" })).unwrap();
        draft.execute("name.define", json!({ "name": "Rate", "refersTo": "=Sheet1!$B$3" })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        doc.execute("cell.set", json!({ "cell": "A2", "input": "Rent and bills" })).unwrap();
        doc.execute("name.define", json!({ "name": "Tax", "refersTo": "=Sheet1!$B$2" })).unwrap();
        let version = doc.version().unwrap();
        let before = whole(&mut doc);
        let preview = doc.compare_changes(&changes).unwrap();
        assert_eq!(doc.version().unwrap(), version, "comparing changes nothing");
        let origin = Origin::person("person:local");
        let answer = doc.merge_changes(&changes, &origin).unwrap();
        assert_eq!(answer["summary"], preview, "the comparison named what the merge did: {answer}");
        assert_eq!(answer["scope"], json!("person"), "{answer}");
        assert_ne!(answer["damage"], json!({ "kind": "none" }), "{answer}");
        assert_eq!(value(&mut doc, "B3"), json!(500.0), "the engine's workbook follows the merge");
        assert_eq!(value(&mut doc, "B4"), json!(1700.0), "and is recalculated");
        assert_eq!(value(&mut doc, "A2"), json!("Rent and bills"), "the edit made here stays");
        assert_eq!(names(&mut doc), ["Rate", "Tax"], "both sides' names are in the list");
        let merged = whole(&mut doc);
        assert!(doc.undo().unwrap().0);
        assert_eq!(value(&mut doc, "B3"), json!(450.0), "one undo takes the whole draft back");
        assert_eq!(value(&mut doc, "A2"), json!("Rent and bills"), "and only the draft");
        assert!(whole(&mut doc) == before, "the undo gives back the exact state before the merge");
        assert_eq!(names(&mut doc), ["Tax"]);
        assert!(doc.redo().unwrap().0);
        assert!(whole(&mut doc) == merged, "the redo gives back the exact merged state");
        let at = doc.version().unwrap();
        assert!(doc.merge_changes(b"not changes", &origin).is_err());
        assert_eq!(doc.version().unwrap(), at, "a refused merge changes nothing");
    }

    /// KitModel M12a: a draft an agent made is one undo step of the person who accepts it (the
    /// host merges with the agent's origin): the person's Undo takes it back, the agent's history
    /// holds nothing, and the replica's histories say the same.
    #[test]
    fn an_agents_draft_merges_as_the_persons_undo_step() {
        use kition_kit::kdm::UndoScope;
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = SheetDocument::open_with_replica("Budget.xlsx", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        draft.execute("cell.set", json!({ "cell": "C2", "input": "=B2*12" })).unwrap();
        let changes = draft.export_changes(&base).unwrap();
        let before = whole(&mut doc);
        let agent = Origin::agent("user:agent");
        let answer = doc.merge_changes(&changes, &agent).unwrap();
        assert_eq!(answer["scope"], json!("person"), "the replica answers the person's scope: {answer}");
        assert_eq!(value(&mut doc, "C2"), json!(14400.0));
        let scopes =
            |doc: &SheetDocument| doc.session.active().map(|state| state.undo.iter().map(|u| u.scope).collect::<Vec<_>>()).unwrap_or_default();
        assert_eq!(scopes(&doc), [UndoScope::Person], "the engine's history holds the merge as the person's step");
        let replica = doc.session.kdm().unwrap().replica();
        assert!(replica.can_undo_in(UndoScope::Person) && !replica.can_undo_in(UndoScope::Agent), "the replica's history agrees");
        assert!(doc.undo().unwrap().0, "the person's Undo takes the draft back");
        assert!(whole(&mut doc) == before, "exactly");
    }

    /// An earlier state (a checkpoint's snapshot) is written over the workbook by object id as one
    /// undo step: the replica holds that state again, with the same ids; Undo gives back the exact
    /// state before the restore (the replica's objects and the engine's workbook), and Redo the
    /// restored one.
    #[test]
    fn a_restored_state_keeps_the_ids_and_undoes_as_one_step() {
        use kition_kit::kdm::{ActorId, Replica};
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        doc.version().unwrap();
        let checkpoint = doc.snapshot().unwrap();
        doc.execute("cell.set", json!({ "cell": "B2", "input": "1300" })).unwrap();
        doc.execute("name.define", json!({ "name": "Tax", "refersTo": "=Sheet1!$B$2" })).unwrap();
        let before = whole(&mut doc);
        let origin = Origin::person("person:local");
        let damage = doc.restore_state(&checkpoint, &origin).unwrap();
        assert_ne!(damage, Damage::None);
        assert_eq!(value(&mut doc, "B2"), json!(1200.0), "the engine's workbook follows the restore");
        let then = Replica::load(<kdm_model::SheetModel as kition_kit::session::KitModel>::schema(), &checkpoint, ActorId::random())
            .unwrap()
            .state_json()
            .unwrap();
        assert_eq!(doc.replicated_state().unwrap()["objects"], then["objects"], "the checkpoint's objects, under their ids");
        assert!(names(&mut doc).is_empty(), "the name defined since goes");
        let restored = whole(&mut doc);
        let at = doc.version().unwrap();
        assert_eq!(doc.restore_state(&checkpoint, &origin).unwrap(), Damage::None, "the same state again changes nothing");
        assert_eq!(doc.version().unwrap(), at);
        assert!(doc.undo().unwrap().0);
        assert_eq!(value(&mut doc, "B2"), json!(1300.0), "one undo takes the restore back");
        assert!(whole(&mut doc) == before, "the undo gives back the exact state before the restore");
        assert!(doc.redo().unwrap().0);
        assert!(whole(&mut doc) == restored, "the redo gives back the exact restored state");
        let mut other = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let other = other.snapshot().unwrap();
        assert!(doc.restore_state(&other, &origin).is_err(), "another document's state is refused");
    }

    /// An agent's reply names objects by their tool ids; text that merely holds an id stays.
    #[test]
    fn a_reply_names_objects_by_their_tool_ids() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let state = doc.replicated_state().unwrap();
        let sheet = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, object)| object["$type"] == "Sheet")
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| panic!("a sheet object: {state}"));
        let shown: Value =
            serde_json::from_str(&doc.model_reply(&json!({ "target": sheet, "note": format!("about {sheet}") }).to_string()).unwrap()).unwrap();
        assert!(shown["target"].as_str().is_some_and(|id| id.starts_with("sheet:")), "{shown}");
        assert_eq!(shown["note"], json!(format!("about {sheet}")), "text that holds an id stays");
        // Naming each object alone gives the tool id the whole workbook's ids give it.
        let every: Vec<String> = state["objects"].as_object().unwrap().keys().cloned().collect();
        let reply = json!({ "objects": every });
        let alone: Value = serde_json::from_str(&doc.model_reply(&reply.to_string()).unwrap()).unwrap();
        let kdm = doc.session.kdm().unwrap();
        assert_eq!(alone, kition_kit::kdm::tool_ids::ToolIds::of_replica(kdm.replica()).show_json(&reply), "the same tool ids");
        let plain = json!({ "summary": "Budget", "cells": ["A1", "B2"], "sheet": 1 });
        assert_eq!(serde_json::from_str::<Value>(&doc.model_reply(&plain.to_string()).unwrap()).unwrap(), plain, "a reply naming no object stays");
        // A sheet made since is named too.
        doc.execute("sheet.insert", json!({ "name": "Second" })).unwrap();
        let state = doc.replicated_state().unwrap();
        let second = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(id, object)| object["$type"] == "Sheet" && **id != sheet)
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| panic!("the second sheet: {state}"));
        let shown: Value = serde_json::from_str(&doc.model_reply(&json!({ "target": second }).to_string()).unwrap()).unwrap();
        assert!(shown["target"].as_str().is_some_and(|id| id.starts_with("sheet:")), "{shown}");
        assert!(doc.model_reply("not json").is_err());
    }

    /// A reply naming one cell of a workbook of thousands names it as the whole workbook's tool ids
    /// do, under `target` and under a member ending in `Id`, and a reply naming no object comes back as it is (the editor
    /// sends every `ai/inspect`, `document/check` and `ai/run` reply through `modelReply`).
    #[test]
    fn a_reply_naming_one_cell_of_many_names_it_as_the_whole_workbook_does() {
        let mut doc = SheetDocument::create("Many.xlsx", 0, 0).unwrap();
        let rows: Vec<Vec<Value>> = (0..400).map(|r| (0..10).map(|c| json!(r * 10 + c)).collect()).collect();
        doc.execute("range.setValues", json!({ "range": "A1:J400", "values": rows })).unwrap();
        let state = doc.replicated_state().unwrap();
        let cells: Vec<String> =
            state["objects"].as_object().unwrap().iter().filter(|(_, object)| object["$type"] == "Cell").map(|(id, _)| id.clone()).collect();
        assert_eq!(cells.len(), 4000, "one object per cell");
        let kdm = doc.session.kdm().unwrap();
        let every = kition_kit::kdm::tool_ids::ToolIds::of_replica(kdm.replica());
        for id in [&cells[0], &cells[1999], &cells[3999]] {
            // A cell's id is the kit's own (not a UUID): a reply names it under `target` or a
            // member ending in `Id` (kition-sdk 68dd08b); elsewhere it is text.
            let reply = json!({ "target": id, "cells": [{ "cellId": id }], "note": format!("about {id}"), "cell": id });
            let shown: Value = serde_json::from_str(&doc.model_reply(&reply.to_string()).unwrap()).unwrap();
            assert_eq!(shown, every.show_reply_json(&reply), "the whole workbook's tool ids");
            assert!(shown["target"].as_str().is_some_and(|text| text.starts_with("cell:")), "{shown}");
            assert!(shown["cells"][0]["cellId"].as_str().is_some_and(|text| text.starts_with("cell:")), "{shown}");
            assert_eq!(shown["note"], json!(format!("about {id}")), "text that holds an id stays");
            assert_eq!(shown["cell"], json!(id), "a chosen id under another member stays");
        }
        let plain = json!({ "summary": "Many", "cells": ["A1", "J400"], "count": 4000 });
        assert_eq!(serde_json::from_str::<Value>(&doc.model_reply(&plain.to_string()).unwrap()).unwrap(), plain, "a reply naming no object stays");
    }

    /// A receipt names what changed since a version; a version the log does not hold says so
    /// instead of answering nothing changed.
    #[test]
    fn changes_since_a_version_name_the_cell_written() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let since = doc.version().unwrap();
        doc.execute("cell.set", json!({ "cell": "B2", "input": "1300" })).unwrap();
        let answer = doc.changes_since(&since);
        assert!(answer["changes"].is_object(), "{answer}");
        assert!(answer["changes"].to_string().contains("1300"), "{answer}");
        let now = doc.version().unwrap();
        let none = doc.changes_since(&now);
        assert!(none["changes"].is_object() && none["unavailable"].is_null(), "{none}");
        let refused = doc.changes_since(b"not a version");
        assert!(refused["changes"].is_null() && refused["unavailable"].is_string(), "{refused}");
    }

    /// `modelDescribe` and `modelBatch` serve the object model over the change exchange: a
    /// command in a batch reaches the engine, and a malformed batch is refused with its code.
    #[test]
    fn describes_and_batches_the_object_model() {
        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let described = doc.model_describe().unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0", "{described}");
        let before = doc.version().unwrap();
        let command = json!({ "ops": [{ "op": "command", "id": "cell.set", "params": { "cell": "D1", "input": "agent" } }] });
        doc.model_batch(&command).unwrap();
        assert_ne!(doc.version().unwrap(), before, "the command ran in the engine");
        assert_eq!(value(&mut doc, "D1"), json!("agent"));
        let refused = doc.model_batch(&json!({ "ops": [{ "op": "no such op" }] })).unwrap_err();
        assert!(refused.contains("\"code\""), "{refused}");
    }

    /// Changes from a newer build (one whose migration raised the document's minimum schema
    /// version past this kit's) are refused with a message naming both versions, and leave the
    /// workbook as it was and still editable.
    #[test]
    fn changes_needing_a_newer_schema_are_refused_clearly() {
        use kition_kit::kdm::{ActorId, Replica, Schema};

        let mut a = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let snapshot = a.snapshot().unwrap();
        let mut raw: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
        let newer = raw["schemaVersion"].as_u64().unwrap() + 1;
        raw["schemaVersion"] = json!(newer);
        let schema = Arc::new(Schema::parse(&raw.to_string()).unwrap());
        let mut peer = Replica::load(schema, &snapshot, ActorId::from_u64(77).unwrap()).unwrap();
        let since = peer.version();
        peer.transact(&Origin::system("migrate"), |tx| tx.raise_schema_version(u32::try_from(newer).unwrap()).map(|_| ())).unwrap();
        let changes = peer.export_changes(&since).unwrap();

        let version = a.version().unwrap();
        let error = a.import_changes(&changes).unwrap_err();
        assert!(error.contains(&format!("needs schema version {newer}")) && error.contains(&format!("up to {}", newer - 1)), "{error}");
        assert_eq!(a.version().unwrap(), version, "nothing was imported");
        assert_eq!(cell(&mut a, "A2")["value"], json!("Rent"));
        a.execute("cell.set", json!({ "cell": "A2", "input": "Rent and bills" })).unwrap();
        assert_eq!(cell(&mut a, "A2")["value"], json!("Rent and bills"), "the workbook is still editable");
    }
    /// `kition-sheet` serves the object model through `ReplicaPort`: a command and a model edit in
    /// batches reach the engine, and a read-only batch refuses writes.
    #[test]
    fn serves_the_object_model_over_its_change_exchange() {
        use kition_kit::session::model::{Exchange, ModelErrorCode};
        use serde_json::{Value, json};

        let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
        let mut exchange = Exchange::new();
        let described = exchange.describe(&mut doc).unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "cell.set"));

        let command = json!({ "ops": [{ "op": "command", "id": "cell.set", "params": { "cell": "D1", "input": "agent" } }] });
        assert_eq!(exchange.batch(&mut doc, &command, true).unwrap_err().code, ModelErrorCode::ReadOnly);
        let before = doc.version().unwrap();
        exchange.batch(&mut doc, &command, false).unwrap();
        assert_ne!(doc.version().unwrap(), before, "the command ran in the engine");

        let load = |exchange: &mut Exchange, doc: &mut SheetDocument, target: &str, field: &str| -> Value {
            let answer = exchange.batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": [field] }] }), false).unwrap();
            answer["loaded"][0]["fields"][field].clone()
        };
        let sheets = load(&mut exchange, &mut doc, "root", "sheets");
        let sheet = sheets[0].as_str().unwrap().to_owned();
        let rename = json!({ "ops": [{ "op": "set", "target": sheet, "field": "name", "value": "Scripted" }] });
        exchange.batch(&mut doc, &rename, false).unwrap();
        assert_eq!(load(&mut Exchange::new(), &mut doc, &sheet, "name"), "Scripted");
    }
}
