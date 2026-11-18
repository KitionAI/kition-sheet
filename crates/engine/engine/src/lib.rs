//! The Kition Sheet engine façade.
//!
//! Every user-visible action is a command with a stable id (`cell.set`, `home.bold`,
//! `data.sortAscending`…) and JSON parameters. The kit's editor (`web/`) and the headless kit
//! executable both go through [`Session::execute`] via `KitTools`, so every action is scriptable
//! and journaled.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod catalog;
pub mod cf;
pub mod cmd;
pub mod context;
pub mod display;
pub mod fill;
pub(crate) mod html_table;
pub mod io;
pub mod kdm_bridge;
pub mod metrics;
pub mod pivot;
pub mod sample;
pub mod selection;
pub mod shape_text;
pub mod tables;
pub mod text_format;
pub mod text_layout;

use std::sync::Arc;

use kition_sheet_calc::Calc;
use kition_sheet_core::{CellRef, RangeRef};
use kition_sheet_model::Workbook;
use serde::Serialize;
use serde_json::Value as Json;

pub use cmd::{CommandInfo, CommandSpec, command_specs, find_command};
pub use context::EditorState;
pub use kdm_model::kition_kit::kdm::{Origin, UndoScope, Version};
pub use kdm_model::kition_kit::session::{Damage, MemoryBlobs};
pub use kition_sheet_calc as calc;
pub use kition_sheet_core as core;
pub use kition_sheet_formula as formula;
pub use kition_sheet_model as model;
pub use kition_sheet_numfmt as numfmt;
pub use kition_sheet_pdf as pdf;
pub use selection::Selection;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("no workbook open")]
    NoDocument,
    #[error("{0}")]
    Other(String),
    #[error("internal error in `{0}` (the workbook was kept as it was): {1}")]
    Internal(String, String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub label: String,
    pub wb: Arc<Workbook>,
    pub selection: Selection,
    /// How many of the workbook's KDM replica undo steps this step took ([`kdm_bridge`]): 0 for
    /// a step that changed nothing the replica keeps, or that was taken before a reset.
    pub steps: u32,
    /// Whose history the step is in (KitModel M12a): the person's, or the agents' working for
    /// them, as the origin of the command that made it.
    pub scope: UndoScope,
}

/// The clipboard: a block of cells (with formulas and styles) or marching-ants reference.
#[derive(Clone, Debug)]
pub struct Clipboard {
    /// Source workbook snapshot, sheet and range.
    pub wb: Arc<Workbook>,
    pub sheet: usize,
    pub range: RangeRef,
    pub cut: bool,
    /// Document uid of the source (a cut only moves within the same workbook).
    pub doc_uid: u64,
    /// Plain text (tab-separated) for the system clipboard.
    pub text: String,
}

/// In-cell edit mode as shown in the status bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub enum Mode {
    #[default]
    Ready,
    Enter,
    Edit,
    Point,
}

#[derive(Debug)]
pub struct DocState {
    pub wb: Arc<Workbook>,
    pub calc: Calc,
    pub selection: Selection,
    pub undo: Vec<HistoryEntry>,
    pub redo: Vec<HistoryEntry>,
    pub path: Option<String>,
    pub saved: Arc<Workbook>,
    pub revision: u64,
    pub uid: u64,
    pub title: String,
    /// Per-sheet selection memory.
    pub sheet_selections: Vec<Selection>,
    /// Structural edits committed since the last [`DocState::take_structural_edits`] (Kition).
    structure: Vec<kition_sheet_model::StructuralEdit>,
    /// The workbook's KDM replica and what it last saw ([`kdm_bridge`]).
    pub(crate) link: kdm_bridge::Link,
}

static NEXT_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
const UNDO_LIMIT: usize = 100;

impl DocState {
    pub fn new(wb: Workbook, path: Option<String>, title: String) -> DocState {
        DocState::build(wb, path, title, true)
    }
    /// A document on `wb` whose formula results are already calculated (a replica's view).
    pub(crate) fn calculated(wb: Workbook, path: Option<String>, title: String) -> DocState {
        DocState::build(wb, path, title, false)
    }
    fn build(mut wb: Workbook, path: Option<String>, title: String, recalc: bool) -> DocState {
        // Every sheet has its id from the start (task card C4/C5/C6-sheet).
        wb.number_sheets();
        kition_sheet_xlsx::normalize_file_units(&mut wb);
        let mut calc = Calc::new();
        if recalc {
            calc.recalc_all(&mut wb);
        } else {
            calc.rebuild(&wb);
        }
        let wb = Arc::new(wb);
        let sel = Selection::at(wb.active().map(|s| s.view_active).unwrap_or_default());
        DocState {
            saved: wb.clone(),
            calc,
            selection: sel,
            undo: vec![],
            redo: vec![],
            path,
            revision: 1,
            uid: NEXT_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            title,
            sheet_selections: vec![],
            structure: vec![],
            link: kdm_bridge::Link::new(&wb),
            wb,
        }
    }
    /// Pushes a new undo step (clearing its own scope's redo steps, never the other scope's;
    /// dropping the oldest past the limit) and
    /// counts it for the KDM bridge ([`kdm_bridge`]), which finds the step a command pushed by
    /// the count: at the limit a push leaves the stack's length unchanged.
    pub(crate) fn push_undo(&mut self, entry: HistoryEntry) {
        let scope = entry.scope;
        self.undo.push(entry);
        let excess = self.undo.len().saturating_sub(UNDO_LIMIT);
        self.undo.drain(..excess);
        self.redo.retain(|u| u.scope != scope);
        self.link.pushed = self.link.pushed.saturating_add(1);
    }
    /// The structural edits (row and column inserts and deletes, cell inserts and deletes, cut
    /// and paste moves) the commands committed since the last
    /// [`take_structural_edits`](Self::take_structural_edits), in order (Kition). Each names the
    /// sheet by its index when the edit ran. Kition's KDM bridge takes them after every command,
    /// so a sheet index always means the workbook the command left. Undo and redo of the engine's
    /// own history, and opening a file, record none: they replace the workbook.
    pub fn structural_edits(&self) -> &[kition_sheet_model::StructuralEdit] {
        &self.structure
    }
    /// Hands over and clears the log of [`structural_edits`](Self::structural_edits).
    pub fn take_structural_edits(&mut self) -> Vec<kition_sheet_model::StructuralEdit> {
        std::mem::take(&mut self.structure)
    }
    pub fn is_dirty(&self) -> bool {
        !Arc::ptr_eq(&self.wb, &self.saved)
    }
    pub fn sheet_index(&self) -> usize {
        self.wb.active_sheet
    }
    pub fn display_title(&self) -> String {
        self.path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.title.clone())
    }
}

/// Engine-side preferences.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// After pressing Enter, move the selection: "down", "right", "up", "left", "none".
    pub enter_direction: String,
    pub autocomplete: bool,
    pub user_name: String,
    pub default_font: String,
    pub default_font_size: f32,
    pub sheets_in_new_workbook: usize,
    pub r1c1: bool,
    /// Words added with Add to Dictionary.
    pub user_dictionary: Vec<String>,
    /// Whether the formula bar shows (View › Show › Formula Bar): the person's, not the workbook's.
    pub formula_bar: bool,
    /// Whether the selection's rows and columns are highlighted (View › 高亮行列), and in which
    /// colour (`#RRGGBB`): the person's, not the workbook's.
    pub focus_cell: bool,
    pub focus_color: String,
    /// The generic name of a table column whose header is empty, numbered (`Column1`; `列1` in a
    /// Chinese editor, which says so through `editor.wording`).
    pub column_prefix: String,
}

/// The highlight's colour until the person picks another (a light amber, as WPS's).
pub const DEFAULT_FOCUS_COLOR: &str = "#FBE9C6";

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            enter_direction: "down".into(),
            autocomplete: true,
            user_name: "Kition Sheet User".into(),
            default_font: kition_sheet_model::DEFAULT_FONT.into(),
            default_font_size: kition_sheet_model::DEFAULT_FONT_SIZE,
            sheets_in_new_workbook: 1,
            r1c1: false,
            user_dictionary: vec![],
            formula_bar: true,
            focus_cell: false,
            column_prefix: "Column".into(),
            focus_color: DEFAULT_FOCUS_COLOR.into(),
        }
    }
}

/// Requests from commands to the UI (open a dialog, show a message…).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum UiRequest {
    Dialog(String, Json),
    Message(String),
    /// Start editing the active cell (F2) with optional initial text.
    EditCell(Option<String>),
    OpenUrl(String),
}

#[derive(Default)]
pub struct Session {
    docs: Vec<DocState>,
    active: usize,
    pub clipboard: Option<Clipboard>,
    pub prefs: Prefs,
    pub mode: Mode,
    pub journal: Vec<(String, Json)>,
    pub ui_requests: Vec<UiRequest>,
    /// Format painter source style (sticky when double-clicked).
    pub format_painter: Option<(Arc<Workbook>, usize, RangeRef, bool)>,
    /// Find state (last search).
    pub last_find: Option<Json>,
    /// Workbook view: normal, pageLayout or pageBreakPreview.
    pub view_mode: String,
    /// Journal position where action recording started.
    pub recording: Option<usize>,
    /// Windows hidden with View › Hide.
    pub hidden_windows: Vec<usize>,
    /// Draw tab tool: "" (select), "pen" or "eraser"; pen colour and width.
    pub draw_tool: String,
    pub draw_color: String,
    pub draw_width: f32,
    /// A shape chosen in Insert › Shapes to draw with the next drag on the sheet (its DrawingML
    /// preset), until it is drawn or Escape.
    pub draw_shape: Option<String>,
    /// The drawing object (shape or text box) selected on the active sheet, by id: the select
    /// tool's object selection, which `drawing.describe` reads when given no ids.
    pub selected_shape: Option<u32>,
    /// Who the commands act for on the replicas ([`kdm_bridge`]); `None`: the local person.
    origin: Option<Origin>,
    /// An undoable command is running: the commands it runs itself are part of its one undo
    /// step (they push none of their own) and their replica writes are counted for it.
    capturing: bool,
    /// The next edit types or pastes entries: a table they land right under or right of grows to
    /// take them (`tables::auto_expand`).
    pub(crate) expand_tables: bool,
    /// A host owns every file ([`Session::set_hosted`]).
    hosted: bool,
}

/// The largest file a command reads by path.
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024 * 1024;

impl Session {
    pub fn new() -> Session {
        Session::default()
    }

    /// Says that a host owns every file of this session (KHP A29, A42, A58, section 10.6; engine
    /// contract E22): a file a command names by path is read only through the host
    /// (`kition_kit::host`, which resolves it inside the workspace and refuses the rest), no file
    /// is written by path (the host saves and exports workbooks), and nothing of the machine is
    /// read (the system word list). A session is hosted as well whenever its process is
    /// (`kition_kit::host::hosting`).
    pub fn set_hosted(&mut self, hosted: bool) {
        self.hosted = hosted;
    }

    /// Whether a host owns every file of this session ([`Session::set_hosted`]).
    pub fn hosted(&self) -> bool {
        self.hosted || kdm_model::kition_kit::host::hosting().files_only
    }

    /// The bytes of the file a command names by `path`: lent by the host when the session is
    /// hosted, else read from disk.
    pub(crate) fn read_path(&self, path: &str) -> Result<Vec<u8>> {
        if !self.hosted() {
            return io::read_file(path);
        }
        let size = kdm_model::kition_kit::host::size(path).map_err(EngineError::Other)?;
        if size > MAX_READ_BYTES {
            return Err(EngineError::Other("files larger than 2 GB aren't supported".into()));
        }
        kdm_model::kition_kit::host::read(path).map_err(EngineError::Other)
    }

    /// Writes `bytes` to the file a command names by `path`; refused when the session is hosted,
    /// whose host writes every file.
    pub(crate) fn write_path(&self, path: &str, bytes: &[u8]) -> Result<()> {
        if self.hosted() {
            return Err(EngineError::Other(format!("{path}: the host writes this workbook's files; nothing is written by path")));
        }
        io::write_file(path, bytes)
    }
    pub fn documents(&self) -> &[DocState] {
        &self.docs
    }
    pub fn active_index(&self) -> usize {
        self.active
    }
    pub fn set_active(&mut self, i: usize) {
        if i < self.docs.len() {
            self.active = i;
        }
    }
    pub fn active(&self) -> Option<&DocState> {
        self.docs.get(self.active)
    }
    pub fn active_mut(&mut self) -> Option<&mut DocState> {
        self.docs.get_mut(self.active)
    }
    pub fn doc(&self) -> Result<&DocState> {
        self.active().ok_or(EngineError::NoDocument)
    }
    pub fn doc_mut(&mut self) -> Result<&mut DocState> {
        self.active_mut().ok_or(EngineError::NoDocument)
    }
    pub fn add_document(&mut self, d: DocState) -> usize {
        self.docs.push(d);
        self.active = self.docs.len() - 1;
        self.active
    }
    pub fn close_document(&mut self, i: usize) {
        if i < self.docs.len() {
            self.docs.remove(i);
            if self.active >= self.docs.len() {
                self.active = self.docs.len().saturating_sub(1);
            }
        }
    }
    /// A new blank workbook titled `BookN`.
    pub fn new_workbook(&mut self) -> usize {
        let n = (1..).find(|n| !self.docs.iter().any(|d| d.title == format!("Book{n}") && d.path.is_none())).unwrap_or(1);
        let mut wb = Workbook::new();
        for _ in 1..self.prefs.sheets_in_new_workbook.clamp(1, 255) {
            let name = wb.next_sheet_name();
            wb.sheets.push(Arc::new(kition_sheet_model::Sheet::new(name)));
        }
        for sheet in &mut wb.sheets {
            Arc::make_mut(sheet).identities.object = kdm_model::kition_kit::kdm::ObjectId::new().to_string();
        }
        self.add_document(DocState::new(wb, None, format!("Book{n}")))
    }

    pub fn commands(&self) -> Vec<CommandInfo> {
        command_specs().iter().map(|c| c.info(self)).collect()
    }

    /// Runs a command by id. Undoable commands record a history step when the workbook changed:
    /// one for everything the command did, the commands it runs itself included (Kition: a
    /// command run by an undoable one pushes no step of its own). Panics inside commands are
    /// caught and reported; the workbook is restored. Afterwards what the command changed is
    /// written to the document's KDM replica ([`kdm_bridge`]).
    pub fn execute(&mut self, id: &str, params: Json) -> Result<Json> {
        let captured = self.capturing;
        let result = self.execute_command(id, params, captured);
        self.capturing = captured;
        self.sync_all_kdm(captured);
        result
    }

    fn execute_command(&mut self, id: &str, params: Json, captured: bool) -> Result<Json> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        if let Err(why) = (spec.enabled)(self) {
            return Err(EngineError::Disabled(id.to_string(), why));
        }
        let before = self.active().map(|d| (d.uid, d.wb.clone(), d.selection.clone()));
        let logged: Vec<(u64, usize)> = self.docs.iter().map(|d| (d.uid, d.structure.len())).collect();
        let params = if params.is_null() { Json::Object(Default::default()) } else { params };
        if spec.undoable {
            self.capturing = true;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (spec.run)(self, &params)));
        self.capturing = captured;
        let result = match result {
            Ok(r) => r,
            Err(p) => {
                let msg =
                    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into());
                // Restore the workbook as it was.
                if let Some((uid, wb, sel)) = &before
                    && let Some(d) = self.docs.iter_mut().find(|d| d.uid == *uid)
                {
                    d.wb = wb.clone();
                    d.selection = sel.clone();
                    d.calc.rebuild(&d.wb);
                }
                // The edits the restored workbook no longer holds leave the structural log too.
                for d in self.docs.iter_mut() {
                    if let Some((_, len)) = logged.iter().find(|(uid, _)| *uid == d.uid) {
                        d.structure.truncate(*len);
                    }
                }
                return Err(EngineError::Internal(id.to_string(), msg));
            }
        };
        if result.is_ok()
            && let Some((uid, old, _)) = &before
            && let Some(state) = self.docs.iter_mut().find(|state| state.uid == *uid)
            && !Arc::ptr_eq(&state.wb, old)
        {
            let mut normalized = (*state.wb).clone();
            if kition_sheet_xlsx::normalize_file_units(&mut normalized) {
                state.wb = Arc::new(normalized);
            }
        }
        if spec.journal && result.is_ok() {
            self.journal.push((id.to_string(), params.clone()));
            if self.journal.len() > 10_000 {
                self.journal.drain(..5_000);
            }
        }
        let scope = self.undo_scope();
        if spec.undoable
            && !captured
            && let Some((uid, wb, sel)) = before
            && let Some(d) = self.docs.iter_mut().find(|d| d.uid == uid)
            && !Arc::ptr_eq(&d.wb, &wb)
        {
            d.push_undo(HistoryEntry { label: spec.label.trim_end_matches('…').to_string(), wb, selection: sel, steps: 0, scope });
            d.revision += 1;
        }
        result
    }

    pub fn run(&mut self, id: &str, params: Json) -> std::result::Result<Json, String> {
        self.execute(id, params).map_err(|e| e.to_string())
    }

    pub fn take_ui_requests(&mut self) -> Vec<UiRequest> {
        std::mem::take(&mut self.ui_requests)
    }

    /// Runs a command for a caller with no UI (MCP, CLI): queued UI requests are dropped, and a command that only
    /// queued a file dialog because it got no file (`file.open`, `file.saveAs`, `file.save` on a workbook with no
    /// file yet, `data.getData`) is an error instead of a silent success.
    pub fn execute_headless(&mut self, id: &str, params: Json) -> Result<Json> {
        let r = self.execute(id, params);
        let requests = self.take_ui_requests();
        let asked_for_file = requests.iter().any(|q| matches!(q, UiRequest::Dialog(name, _) if name == "open" || name == "saveAs"));
        if r.is_ok() && asked_for_file {
            let what = match id {
                "file.open" => "`path` or `base64`",
                "data.getData" | "data.fromTextCsv" => "`path` or `text`",
                _ => "`path`",
            };
            let msg = format!("missing {what}: programmatic calls never open dialogs");
            return Err(EngineError::BadParams { cmd: id.to_string(), msg });
        }
        r
    }
}

/// Convenience: the active workbook's active sheet.
pub fn active_sheet(s: &Session) -> Option<&kition_sheet_model::Sheet> {
    s.active().and_then(|d| d.wb.active())
}

pub fn cell(r: u32, c: u32) -> CellRef {
    CellRef::new(r, c)
}

#[cfg(test)]
mod tests;
