//! # kition-sheet-kit
//!
//! The Sheet engine for Kition's in-page editor, on the Kition SDK. [`SheetDocument`] implements
//! [`KitDocument`] for a Kition Sheet workbook (Excel `.xlsx`) and [`KitTools`] for working in its
//! grid: a click selects a cell, a drag a range, a header a column or row; typing enters the
//! active cell, Enter and Tab commit and move, arrows move; the selection and the cell being
//! typed come back as engine overlays. The `command.runLine` command enters text into the active
//! cell, for the host's bar (a formula bar) and agents.
//!
//! Document pixels are the active sheet at 1:1 as the engine lays it out (column widths and row
//! heights in points, one pixel each), with the row and column headings at the top and left,
//! over the used range and a margin of empty cells. The upstream editor drew its grid with egui
//! (since removed); the cells are painted here ([`draw`]), from the same
//! styles and number formats.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod acts;
mod bar;
mod check;
mod clear_premises;
mod collab;
mod comments;
mod computed;
mod conditional_read_premises;
mod context;
mod controls;
mod copying;
mod data_read_premises;
mod describe;
mod draw;
mod drawn;
mod edit;
mod edit_read_premises;
mod files;
mod fill_read_premises;
mod filling;
mod format_premises;
mod formula;
mod gesture;
mod icons;
mod ids;

mod axis_premises;
mod moving;
mod named_premises;
mod object_read_premises;
mod objects;
mod outline_premises;
mod params;
mod place;
mod point;
mod pointing;
mod primitive_read_premises;
mod references;
mod registry_read_premises;
mod render;
mod resize;
mod scheme;
mod shared_edit_premises;
mod style_premises;
mod table_lookup_premises;
mod table_premises;
mod validation;
mod view;
mod view_read_premises;

use draw::{Canvas, Faces, Line, Rect, Rgba};
use edit::Edit;
use gesture::Drag;
use image::ImageEncoder as _;
use kition_kit::raster::{self, Frame};
use kition_kit::tools::{KitTools, PointerAction, ToolInfo, ToolMods, ToolOverlay, ToolPointer, ToolRequest, ToolResponse};
use kition_kit::{KitCommand, KitDocument, MAX_RENDER_SIDE};
use kition_sheet_engine::core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value, col_to_letters};
use kition_sheet_engine::model::{
    BorderStyle, Color, Font, HAlign, PatternType, RichText, RunColor, RunFont, Sheet, TextRun, Underline, VAlign, VertAlign, Workbook,
};
use kition_sheet_engine::{DocState, Session};
use serde_json::{Value as Json, json};

/// Width of the row headings, in document pixels.
pub const HEADER_W: f64 = 46.0;
/// Height of the column headings, in document pixels.
pub const HEADER_H: f64 = 21.0;
/// One outline level's slot in the outline gutter (Excel's outline bar), in document pixels.
pub const OUTLINE_SLOT: f64 = 16.0;
/// Empty columns and rows shown past the used range.
const MORE_COLS: u32 = 6;
const MORE_ROWS: u32 = 30;
/// At least this many columns and rows, so a new sheet fills a window.
const MIN_COLS: u32 = 26;
const MIN_ROWS: u32 = 100;
/// Text more than this many columns past the area drawn is not looked for.
const OVERFLOW_COLS: u32 = 20;

pub(crate) const WHITE: Rgba = [255, 255, 255, 255];
pub(crate) const GRIDLINE: Rgba = [225, 225, 228, 255];
pub(crate) const HEADING: Rgba = [245, 245, 247, 255];
pub(crate) const HEADING_LINE: Rgba = [210, 210, 214, 255];
pub(crate) const HEADING_TEXT: Rgba = [90, 90, 96, 255];
/// The lines under frozen rows and right of frozen columns (upstream `grid.rs`).
pub(crate) const FREEZE_LINE: Rgba = [150, 150, 150, 255];
/// The sheet's brand colour (`ui.brand.light` in `kit.json`); the grid is always drawn light.
const ACCENT: [u8; 4] = [26, 134, 65, 255];
const SELECTION: [u8; 4] = [26, 134, 65, 36];
const HEADING_SELECTED: [u8; 4] = [26, 134, 65, 56];
pub(crate) const EDITING_TEXT: [u8; 4] = [20, 20, 20, 255];
/// The freeze preview's line and tint (the Freeze Panes menu points at a choice).
const FREEZE_PREVIEW: [u8; 4] = [26, 134, 65, 255];
const FREEZE_PREVIEW_TINT: [u8; 4] = [26, 134, 65, 28];

/// The highlight of View › 高亮行列 in `#RRGGBB`: light colours nearly as they are, darker ones
/// fainter, so the cells' text stays readable under it.
fn focus_rgba(hex: &str) -> [u8; 4] {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0xFBE9C6);
    let [r, g, b] = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
    let light = u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 >= 200_000;
    [r, g, b, if light { 200 } else { 96 }]
}

/// Characters selected in the cell being typed.
const TEXT_SELECTION: [u8; 4] = [26, 134, 65, 72];

/// The pointer over the grid while the format painter is up: a white cross with a small brush
/// beside it (Kition Sheet's own drawing), falling back to the copy cursor.
const PAINTER_CURSOR: &str = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24'%3E%3Cpath d='M7 1v12M1 7h12' stroke='%23000' stroke-width='4'/%3E%3Cpath d='M7 1v12M1 7h12' stroke='%23fff' stroke-width='2'/%3E%3Crect x='14' y='11' width='8' height='5' rx='1' fill='%23fff' stroke='%23000'/%3E%3Cpath d='M18 16v6' stroke='%23000' stroke-width='3' stroke-linecap='round'/%3E%3C/svg%3E\") 7 7, copy";

/// One open Kition Sheet workbook.
pub struct SheetDocument {
    session: Session,
    computed: std::cell::RefCell<computed::History>,
    faces: Faces,
    /// The text being typed into the active cell, before it is entered.
    editing: Option<Edit>,
    /// Canvas reach requested by the visible viewport (document pixels), kept out of the file.
    viewport: (f64, f64),
    /// How many times the cell being typed changed: part of the revision, so the page repaints
    /// it (the editor is drawn with the cells, in its runs' fonts).
    edits: u64,
    /// The host's color scheme the editor's grid is drawn in (`editor.scheme`): a view concern,
    /// never saved, never a change of the workbook ([`scheme`]).
    scheme: scheme::Scheme,
    /// How many times the scheme changed: part of the revision, so the page repaints the grid.
    schemes: u64,
    /// How many of another's steps (an agent's, the host's) changed what the grid draws without an
    /// undo step (a recalculation, a view or typing state the editor keeps): part of the revision,
    /// so the page repaints them, as it skips repainting a step that moves neither the revision
    /// nor `inspect()` (kition-sdk `agentSkipsUnchangedPaint`, [`drawn`]).
    drawn: u64,
    /// The pointer is over the cell being typed (or selecting its characters): the text cursor.
    text_hover: bool,
    /// The gesture a press began, until its release ends it.
    drag: Drag,
    /// The text of the shape being typed (`objects`).
    shape_edit: Option<objects::ShapeEdit>,
    /// Drawing objects added to the selection with Shift (for Group), besides the selected one.
    also_selected: Vec<u32>,
    /// The pointer is on the line after a row's heading (`true`) or a column's.
    edge_hover: Option<bool>,
    /// The reference being pointed at while a formula is typed (`pointing`).
    point: Option<pointing::Pointing>,
    /// The sheet and cell of the formula being typed while another sheet is shown to point at.
    edit_home: Option<(usize, CellRef)>,
    /// The function chosen in the function list.
    hint_pick: usize,
    /// How many times View › Zoom set the zoom: the editor zooms the view to it each time.
    zoom_asked: u64,
    /// The last dialog a command asked the editor to open (menu-style invocations without the
    /// values they need: Format Cells, Sort…): how many were asked, its name, its values, and the
    /// command that asked.
    dialog_asked: (u64, Option<(String, Json, String)>),
    /// The rows and columns a freeze the Freeze Panes menu points at would freeze (a preview).
    freeze_preview: Option<(u32, u32)>,
    /// Where the view is scrolled to (document points of its top-left, `editor.viewport`): a
    /// freeze starts at the cell shown there.
    view_at: (f64, f64),
    /// The last notice for the person (`controls`): how many there were, and its text.
    notice: (u64, Option<String>),
    /// The cell being typed began with what Insert put in (Insert Function, a symbol): entered,
    /// it does not make a table beside it larger (TS-C60), as typing does.
    edit_inserted: bool,
    /// The pointer is over the fill handle (`filling`).
    fill_hover: bool,
    /// The pointer is over the selection's border: with Cmd/Ctrl held (a copy) or not.
    move_hover: Option<bool>,
    /// What was typed is held by its cell's data validation (`validation`): the rule's error style
    /// and where Enter (Tab, an arrow) would have moved once it goes in.
    validation_held: Option<validation::Held>,
    /// The run of Tabs Enter goes back from (WPS W-B3); session state, never saved.
    tab_run: Option<validation::TabRun>,
    /// Entering what is typed skips the cell's data validation (a warning or a note answered Yes).
    validation_skip: bool,
    /// The text of the person's copy the grid marks (`copying`): its marching ants, Enter pastes it.
    copy_mark: Option<String>,
    /// The text of the person's copy that ended (Escape, Enter's paste, typing): a paste of it
    /// pastes nothing, as in WPS, while the system clipboard keeps it for other applications.
    copy_spent: Option<String>,
    /// A press on the selection's border (`moving`): where, the selection then, and the side, so a
    /// double-click there jumps that way even when the press's click selected the cell beside it.
    border_press: Option<moving::BorderPress>,
}

/// What [`SheetDocument::paint_sheet`] draws besides the cells.
#[derive(Clone, Copy)]
struct Paint {
    /// A picture of the sheet (File › Export as Picture): no gridlines, headings, filter buttons,
    /// freeze lines, note marks or cell being typed.
    plain: bool,
    /// The editor's own state: the cell or shape being typed.
    editor: bool,
    /// The row and column headings.
    headings: bool,
    /// The grid's own colours: the host's scheme in the editor ([`scheme`]), light in every
    /// picture of the workbook.
    palette: &'static scheme::Palette,
}

/// The editor's state in [`KitDocument::inspect`], as JSON Pointers: what the editor shows of the
/// active sheet (its name, used range, view settings, frozen panes and zoom), the selection and the
/// active cell with what it holds and its formatting, the cell or shape being typed and the
/// formula being pointed at, the selected drawing objects and table, and the editor's own notices
/// and dialogs. It describes the editing session, not the workbook, so a headless host leaves it
/// out of what an agent reads (`kition_headless::KitInfo::volatile`); every sheet's own facts are
/// in `/workbook` instead.
pub const EDITOR_STATE: &[&str] = &[
    "/sheet",
    "/used",
    "/active",
    "/selection",
    "/stats",
    "/lines",
    "/content",
    "/editing",
    "/editText",
    "/editCaret",
    "/editAnchor",
    "/pointing",
    "/gridlines",
    "/gridColor",
    "/headings",
    "/freeze",
    "/freezeShift",
    "/freezePreview",
    "/activeRect",
    "/zoom",
    "/zoomAsked",
    "/editCell",
    "/formulaHint",
    "/formulaText",
    "/formulaReferences",
    "/table",
    "/shape",
    "/shapes",
    "/chart",
    "/shapeEditing",
    "/escape",
    "/formatPainter",
    "/drawTool",
    "/copied",
    "/style",
    "/tableInfo",
    "/dialog",
    "/validation",
    "/defaultSize",
    "/notice",
    "/checkbox",
];

/// `file` commands that only produce bytes or files from the workbook, leaving it as it is.
const FILE_OUTPUTS: &[&str] =
    &["file.export", "file.saveBytes", "file.exportCsv", "file.exportHtml", "file.exportPdf", "file.print", "file.printPreview", "file.inspect"];

/// Commands that leave the active sheet or change the workbook's sheets (the sheet tabs').
const SHEET_TAB_COMMANDS: &[&str] = &[
    "sheet.activate",
    "sheet.next",
    "sheet.previous",
    "sheet.move",
    "sheet.rename",
    "sheet.hide",
    "sheet.unhide",
    "sheet.tabColor",
    "sheet.insert",
    "sheet.delete",
];

/// Commands that insert or delete rows, columns or cells: what is being typed goes into its cell
/// first (WPS greys them while a cell is typed; the editor's menus and agents reach them anyway).
const STRUCTURE_COMMANDS: &[&str] = &["row.insert", "column.insert", "row.delete", "column.delete", "cell.insert", "cell.delete"];

/// Undoable commands that only change the view (the zoom is not part of the replica).
const VIEW_ONLY: &[&str] = &["view.zoom", "view.zoomToSelection"];

/// Whether running a command changes the workbook (the catalog's `mutates`): its edits (the
/// engine's undoable commands, including the sheet view settings a workbook saves, such as frozen
/// panes and gridlines), undo and redo, a second window on the workbook, and the `file` commands
/// that replace, close, or save it. Selection, navigation, the zoom, and queries do not.
fn changes_document(id: &str, undoable: bool) -> bool {
    (undoable && !VIEW_ONLY.contains(&id))
        || matches!(id, "edit.undo" | "edit.redo" | "view.newWindow" | "view.customViews" | "view.split" | "formula.watchWindow" | "script.record")
        || (id.starts_with("file.") && !FILE_OUTPUTS.contains(&id))
}

fn engine_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn rgb(color: [u8; 3]) -> Rgba {
    [color[0], color[1], color[2], 255]
}

fn resolve(color: Color, wb: &Workbook) -> Option<Rgba> {
    color.resolve(&wb.theme).map(rgb)
}

impl SheetDocument {
    /// The parameter schema the catalogue offers agents for `command`.
    fn command_schema(&self, command: &str) -> Option<Json> {
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Json>> = std::sync::OnceLock::new();
        SCHEMAS.get_or_init(|| self.commands().into_iter().filter_map(|command| Some((command.id, command.schema?))).collect()).get(command).cloned()
    }

    /// [`KitDocument::execute`]'s work: the tool ids, and an agent's command that names what it
    /// acts on run apart from the editor's state.
    fn execute_step(&mut self, command: &str, params: Json) -> Result<Json, String> {
        self.refuse_unknown(command, &params)?;
        let mut params = params;
        let schema = self.command_schema(command);
        let implied = if self.session.active().is_some() { self.take_tool_ids(command, &mut params, schema.as_ref())? } else { None };
        let agent = kition_kit::dispatch::role() == Some(kition_kit::dispatch::Role::Agent);
        if !agent || context::moves_the_editor(command) || !Self::names_target(&params) {
            return self.run_command(command, params);
        }
        let kept = self.editor_snapshot();
        let active = self.doc_state()?.wb.active_sheet;
        if let Some(index) = self.command_target_sheet(command, &params, implied)
            && index != active
        {
            self.session.show_sheet(index, None).map_err(engine_error)?;
        }
        let result = self.run_command(command, params);
        if let Some(kept) = kept {
            // Fails only when the command closed the workbook, which leaves nothing to put back.
            let _ = self.put_back(&kept);
        }
        result
    }

    fn run_command(&mut self, command: &str, params: Json) -> Result<Json, String> {
        // The agent's preview: a function of the workbook and the call, before anything below
        // touches the cell being typed.
        if command == "file.render" {
            return self.render_view(&params);
        }
        if let Some(answer) = self.comment_command(command, &params) {
            return answer;
        }
        if command == "editor.viewport" {
            let extent = |key: &str| params.get(key).and_then(Json::as_f64).filter(|n| n.is_finite() && *n >= 0.0).unwrap_or(0.0);
            self.viewport = (extent("right"), extent("bottom"));
            if params.get("x").is_some() || params.get("y").is_some() {
                self.view_at = (extent("x"), extent("y"));
            }
            return Ok(json!({ "width": self.width(), "height": self.height() }));
        }
        if command == "editor.wording" {
            // The editor's language for names the engine makes up: a Chinese editor's tables name
            // empty headers 列1, 列2 (WPS), others Column1.
            if let Some(prefix) =
                params.get("columnPrefix").and_then(Json::as_str).map(str::trim).filter(|p| !p.is_empty() && p.chars().count() <= 32)
            {
                self.session.prefs.column_prefix = prefix.to_owned();
            }
            return Ok(json!({ "columnPrefix": self.session.prefs.column_prefix }));
        }
        if command == "editor.scheme" {
            // The host's color scheme: the grid is drawn in its palette; nothing moves, nothing is
            // saved, the workbook is unchanged.
            let name = params.get("scheme").and_then(Json::as_str).ok_or("invalid-params: editor.scheme needs `scheme`, light or dark")?;
            let scheme = scheme::Scheme::parse(name).ok_or_else(|| format!("invalid-params: the scheme is light or dark, not `{name}`"))?;
            let changed = scheme != self.scheme;
            if changed {
                self.scheme = scheme;
                self.schemes = self.schemes.saturating_add(1);
            }
            return Ok(json!({ "changed": changed, "scheme": scheme.name() }));
        }
        if command == "editor.formula" {
            return self.bar_command(&params);
        }
        if command == "file.export" && params.get("format").and_then(Json::as_str).is_some_and(|f| f.eq_ignore_ascii_case("png")) {
            return self.export_picture(&params);
        }
        if command == "editor.cycleReference" {
            // Without text: the cell being typed (F4 reaches the kit's own key handler, not the tool).
            if params.get("text").is_none() {
                self.cycle_reference();
                let (text, caret) =
                    self.editing.as_ref().map_or((String::new(), 0), |e| (e.text(), e.chars().iter().take(e.caret).map(|c| c.len_utf16()).sum()));
                return Ok(json!({ "text": text, "start": caret, "end": caret }));
            }
            return Ok(Self::cycle_reference_command(&params));
        }
        if command == "editor.acceptFunction" {
            return self.accept_function_command(&params);
        }
        if command == "editor.contextMenu" {
            return self.context_menu(&params);
        }
        if command == "editor.validation" {
            return self.validation_answer(&params);
        }
        if command == "editor.insertText" {
            let text = params.get("text").and_then(Json::as_str).unwrap_or_default().to_owned();
            // `typing`: only into what is being typed (Shift+Space, decided in the editor's queue
            // after the keys typed before it), else nothing.
            if params.get("typing").and_then(Json::as_bool) == Some(true) && self.editing.is_none() {
                return Ok(json!({ "editing": false }));
            }
            return self.insert_text(&text, params.get("append").and_then(Json::as_bool) == Some(true));
        }
        let person = self.session.undo_scope() == kition_sheet_engine::UndoScope::Person;
        // A person's paste of the copy that ended pastes nothing (WPS: Cmd+V does nothing after
        // Escape, Enter's paste or a cut pasted once).
        if person && command == "edit.paste" && self.paste_ended(&params) {
            return Ok(json!({ "pasted": null }));
        }
        if person && command == "edit.clearClipboard" {
            self.end_copy();
        }
        // Insert › Symbol in the editor: at the caret of what is typed, else after the active
        // cell's text with the cell open (WPS). An agent's goes straight into the cell.
        if command == "symbol.insert"
            && self.session.undo_scope() == kition_sheet_engine::UndoScope::Person
            && let Some(symbol) = params.get("char").and_then(Json::as_str).filter(|c| !c.is_empty())
        {
            return self.insert_text(symbol, true);
        }
        if command == "editor.freezePreview" {
            let count = |key: &str| params.get(key).and_then(Json::as_u64).map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX).min(MAX_ROWS - 1));
            let (rows, cols) = (count("rows"), count("cols").min(MAX_COLS - 1));
            self.freeze_preview = (rows > 0 || cols > 0).then_some((rows, cols));
            self.edits += 1;
            return Ok(json!({ "preview": self.freeze_preview.map(|(r, c)| [r, c]) }));
        }
        // A freeze chosen ends its preview.
        if command.starts_with("view.freeze") || command == "view.unfreezePanes" {
            self.freeze_preview = None;
        }
        // A freeze starts at the view's top-left cell (WPS, Excel): rows scrolled off above and
        // columns scrolled off left stay out of view while the panes are frozen.
        let mut params = params;
        if command.starts_with("view.freeze")
            && params.get("top").is_none()
            && let Some(top) = self.view_top_cell()
            && top != CellRef::default()
            && let Some(map) = params.as_object_mut()
        {
            map.insert("top".into(), json!(top.a1()));
        }
        if STRUCTURE_COMMANDS.contains(&command) && self.editing.is_some() {
            self.commit_or_drop();
            self.edits += 1;
        }
        // A font command with shapes selected (or one being typed) formats their text.
        if self.editing.is_none()
            && let Some(answer) = self.shape_font(command, &params)
        {
            return answer;
        }
        // A font command while characters of the cell being typed are selected formats them.
        if self.editing.is_some()
            && let Some(format) = edit::format_params(command, &params)
        {
            let base = self.active_font();
            if let Some(edit) = self.editing.as_mut()
                && edit.format(&base, &format)?
            {
                self.edits += 1;
                return Ok(json!({ "editing": true, "format": format }));
            }
        }
        if command == "command.runLine" {
            // The formula bar: enters the text into the active cell and moves down, as Enter does.
            let text = params.get("text").and_then(Json::as_str).unwrap_or_default().to_owned();
            if self.editing.take().is_some() {
                self.edits += 1;
            }
            let cell = self.active().a1();
            self.session.execute("cell.set", json!({ "cell": cell, "input": text })).map_err(engine_error)?;
            self.step_after(validation::Step::Enter(false))?;
            return Ok(json!({ "cell": cell }));
        }
        // Leaving the sheet, or changing the sheets, enters what is being typed into its cell
        // first, as clicking another sheet's tab does: the text stays on the sheet it was typed on.
        // Text the cell refuses is dropped rather than carried to the next sheet.
        // A formula typed where a reference can go stays open instead: the next sheet's cells can be
        // pointed at (WPS, Excel), and Enter puts it in its own cell.
        let pointing = SHEET_TAB_COMMANDS.contains(&command) && self.keep_formula_across_sheets(command);
        if !pointing && SHEET_TAB_COMMANDS.contains(&command) && self.editing.is_some() {
            self.commit_or_drop();
        }
        let result = self.session.execute(command, params).map_err(engine_error);
        if result.is_ok() && matches!(command, "view.zoom" | "view.zoomToSelection") {
            self.zoom_asked += 1;
        }
        // F2 (`edit.beginEdit`) and Insert Function with a function chosen type in the active cell;
        // a dialog a command asks for (a menu item run without its values) is the editor's to open.
        for request in self.session.take_ui_requests() {
            match request {
                kition_sheet_engine::UiRequest::EditCell(text) => self.begin_edit(text.as_deref()),
                // Programmatic calls (an agent's) never open dialogs.
                kition_sheet_engine::UiRequest::Dialog(name, values) if self.session.undo_scope() == kition_sheet_engine::UndoScope::Person => {
                    self.dialog_asked = (self.dialog_asked.0 + 1, Some((name, values, command.to_string())));
                    // A new revision, so the editor reads the request even when nothing else changed.
                    self.edits += 1;
                }
                _ => {}
            }
        }
        result
    }

    /// Refuses a parameter `command` does not take (a typo or a guessed name) before it runs,
    /// naming the one it resembles, instead of running without it (engine rule E7a). The schemas
    /// are the ones the catalogue offers agents, read once.
    fn refuse_unknown(&self, command: &str, params: &Json) -> Result<(), String> {
        if command == "chart.shapeFill" && params.get("kind").and_then(Json::as_str) == Some("shape") && params.get("chart").is_some() {
            return Err("chart.shapeFill: kind=shape takes id; chart/on cannot name a shape".into());
        }
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Json>> = std::sync::OnceLock::new();
        let schemas = SCHEMAS.get_or_init(|| self.commands().into_iter().filter_map(|command| Some((command.id, command.schema?))).collect());
        schemas.get(command).map_or(Ok(()), |schema| kition_kit::schema::refuse_unknown(command, schema, params))
    }

    fn from_session(mut session: Session) -> Result<Self, String> {
        // A process a host runs (kition-headless) owns no file: the engine reads files only
        // through the host and writes none by path (engine contract E22, KHP A29).
        session.set_hosted(kition_kit::host::hosting().files_only);
        let faces = Faces::load().ok_or("the cell fonts could not be read")?;
        Ok(Self {
            session,
            computed: Default::default(),
            faces,
            editing: None,
            viewport: (0.0, 0.0),
            edits: 0,
            scheme: scheme::Scheme::Light,
            schemes: 0,
            drawn: 0,
            text_hover: false,
            drag: Drag::None,
            shape_edit: None,
            also_selected: Vec::new(),
            edge_hover: None,
            point: None,
            edit_home: None,
            hint_pick: 0,
            zoom_asked: 0,
            dialog_asked: (0, None),
            freeze_preview: None,
            view_at: (0.0, 0.0),
            notice: (0, None),
            edit_inserted: false,
            fill_hover: false,
            move_hover: None,
            validation_held: None,
            tab_run: None,
            validation_skip: false,
            copy_mark: None,
            copy_spent: None,
            border_press: None,
        })
    }

    fn doc_state(&self) -> Result<&DocState, String> {
        self.session.doc().map_err(engine_error)
    }

    fn sheet(&self) -> Option<(&Workbook, &Sheet)> {
        let st = self.session.active()?;
        Some((&st.wb, st.wb.active()?))
    }

    /// Columns and rows on the canvas: the used range and a margin.
    /// Columns and rows of `sheet` laid out without an editor: its used range and a margin.
    fn sheet_extent(sheet: &Sheet) -> (u32, u32) {
        let used = sheet.used_range();
        let cols = used.map_or(0, |r| r.end.col + 1).saturating_add(MORE_COLS).clamp(MIN_COLS, MAX_COLS);
        let rows = used.map_or(0, |r| r.end.row + 1).saturating_add(MORE_ROWS).clamp(MIN_ROWS, MAX_ROWS);
        (cols, rows)
    }

    fn extent(&self) -> (u32, u32) {
        let used = self.sheet().and_then(|(_, sheet)| sheet.used_range());
        let active = self.active();
        let visible = self
            .sheet()
            .map(|(_, sheet)| {
                (
                    view::col_at(sheet, (self.viewport.0 - header_w(sheet)).max(0.0)) + 1,
                    view::row_at(sheet, (self.viewport.1 - header_h(sheet)).max(0.0)) + 1,
                )
            })
            .unwrap_or_default();
        let cols = used.map_or(0, |r| r.end.col + 1).max(active.col + 1).max(visible.0).saturating_add(MORE_COLS).clamp(MIN_COLS, MAX_COLS);
        let rows = used.map_or(0, |r| r.end.row + 1).max(active.row + 1).max(visible.1).saturating_add(MORE_ROWS).clamp(MIN_ROWS, MAX_ROWS);
        (cols, rows)
    }

    fn formula_text(&self) -> String {
        self.editing.as_ref().map(Edit::text).unwrap_or_default()
    }

    fn formula_references(&self) -> Vec<formula::Reference> {
        let shown = self.sheet().map_or("", |(_, sheet)| sheet.name.as_str());
        // A formula typed for a cell on another sheet: its plain references are that sheet's.
        let home = match self.edit_home {
            Some((sheet, _)) => self.doc_state().ok().and_then(|d| d.wb.sheet(sheet)).map_or(shown, |s| s.name.as_str()),
            _ => shown,
        };
        formula::references(&self.formula_text(), home, shown)
    }

    fn col_x(sheet: &Sheet, col: u32) -> f64 {
        header_w(sheet) + view::left(sheet, col)
    }

    fn row_y(sheet: &Sheet, row: u32) -> f64 {
        header_h(sheet) + view::top(sheet, row)
    }

    /// The canvas rectangle of a cell, or of the merged area it is in.
    fn cell_rect(sheet: &Sheet, range: RangeRef) -> (f64, f64, f64, f64) {
        (
            Self::col_x(sheet, range.start.col),
            Self::row_y(sheet, range.start.row),
            Self::col_x(sheet, range.end.col + 1),
            Self::row_y(sheet, range.end.row + 1),
        )
    }

    /// The `data.showDetail` / `data.hideDetail` range of lines `start..=end`.
    fn outline_range(rows: bool, start: u32, end: u32) -> Json {
        if rows {
            json!({ "rows": format!("{}:{}", start + 1, end + 1) })
        } else {
            json!({ "cols": format!("{}:{}", col_to_letters(start), col_to_letters(end)) })
        }
    }

    /// A group's + or - button: expands or collapses it.
    fn toggle_group(&mut self, rows: bool, start: u32, end: u32, hidden: bool) -> Result<(), String> {
        let command = if hidden { "data.showDetail" } else { "data.hideDetail" };
        self.session.execute(command, Self::outline_range(rows, start, end)).map(|_| ()).map_err(engine_error)
    }

    /// A level button: the groups under `level` are shown, the others collapsed.
    fn show_level(&mut self, rows: bool, level: u8) -> Result<(), String> {
        let Some((_, sheet)) = self.sheet() else { return Ok(()) };
        let lines = if rows { &sheet.rows } else { &sheet.cols };
        let deepest = outline_levels(lines);
        let mut steps = Vec::new();
        for l in 1..=deepest {
            for (start, end, hidden) in outline_groups(lines, l) {
                let hide = l >= level;
                if hide != hidden {
                    steps.push((hide, start, end));
                }
            }
        }
        // Shallow groups are shown before deeper ones are collapsed inside them.
        steps.sort_by_key(|(hide, ..)| *hide);
        for (hide, start, end) in steps {
            let command = if hide { "data.hideDetail" } else { "data.showDetail" };
            self.session.execute(command, Self::outline_range(rows, start, end)).map_err(engine_error)?;
        }
        Ok(())
    }

    /// Excel's outline bar: each group's bracket and its - (+ once collapsed) button at its
    /// summary line, and the level buttons 1, 2, … in the corner.
    fn paint_outline(
        &self,
        canvas: &mut Canvas,
        sheet: &Sheet,
        palette: &scheme::Palette,
        (x, y, scale): (f64, f64, f64),
        (r0, r1, c0, c1): (u32, u32, u32, u32),
        (grid_right, grid_bottom): (f64, f64),
    ) {
        let to_x = |dx: f64| (dx - x) * scale;
        let to_y = |dy: f64| (dy - y) * scale;
        let (gutter_w, gutter_h) = (row_gutter(sheet), col_gutter(sheet));
        let line = scale.clamp(1.0, 2.0).round();
        let half = 5.5 * scale;
        canvas.fill(Rect { x0: to_x(0.0), y0: to_y(0.0), x1: to_x(gutter_w), y1: grid_bottom }, palette.background);
        canvas.fill(Rect { x0: to_x(0.0), y0: to_y(0.0), x1: grid_right, y1: to_y(gutter_h) }, palette.background);
        let size = (9.0 * 96.0 / 72.0 * scale) as f32;
        // A button: a box with a minus, a plus, or a level number.
        let button = |canvas: &mut Canvas, cx: f64, cy: f64, mark: Option<bool>, label: Option<u8>| {
            let (bx0, by0, bx1, by1) = ((cx - half).round(), (cy - half).round(), (cx + half).round(), (cy + half).round());
            canvas.fill(Rect { x0: bx0, y0: by0, x1: bx1, y1: by1 }, palette.background);
            canvas.hline(bx0, bx1, by0, line, palette.heading_text);
            canvas.hline(bx0, bx1, by1 - line, line, palette.heading_text);
            canvas.vline(bx0, by0, by1, line, palette.heading_text);
            canvas.vline(bx1 - line, by0, by1, line, palette.heading_text);
            if let Some(plus) = mark {
                canvas.hline((cx - half * 0.55).round(), (cx + half * 0.55).round(), cy.round(), line, palette.heading_text);
                if plus {
                    canvas.vline(cx.round(), (cy - half * 0.55).round(), (cy + half * 0.55).round(), line, palette.heading_text);
                }
            }
            if let Some(n) = label {
                let label = n.to_string();
                let text = Line { font: &self.faces.sans, size, text: &label };
                let (ascent, descent) = text.metrics();
                text.draw(
                    canvas,
                    cx - text.width() / 2.0,
                    cy + (ascent - descent) / 2.0,
                    Rect { x0: bx0, y0: by0, x1: bx1, y1: by1 },
                    palette.heading_text,
                    false,
                );
            }
        };
        let rows = outline_levels(&sheet.rows);
        for level in 1..=rows {
            let cx = to_x(OUTLINE_SLOT * (f64::from(level) - 0.5));
            for (start, end, hidden) in outline_groups(&sheet.rows, level) {
                if end + 1 < r0 || start > r1 + 1 {
                    continue;
                }
                let summary = end + 1;
                let cy = to_y((Self::row_y(sheet, summary) + Self::row_y(sheet, summary + 1)) / 2.0);
                if !hidden {
                    let top = to_y(Self::row_y(sheet, start)) + 2.0 * scale;
                    canvas.vline(cx.round(), top, cy - half, line, palette.heading_text);
                    canvas.hline(cx.round(), cx + 4.0 * scale, top.round(), line, palette.heading_text);
                }
                button(canvas, cx, cy, Some(hidden), None);
            }
        }
        if rows > 0 {
            for level in 1..=rows + 1 {
                button(canvas, to_x(OUTLINE_SLOT * (f64::from(level) - 0.5)), to_y(gutter_h + HEADER_H / 2.0), None, Some(level));
            }
        }
        let cols = outline_levels(&sheet.cols);
        for level in 1..=cols {
            let cy = to_y(OUTLINE_SLOT * (f64::from(level) - 0.5));
            for (start, end, hidden) in outline_groups(&sheet.cols, level) {
                if end + 1 < c0 || start > c1 + 1 {
                    continue;
                }
                let summary = end + 1;
                let cx = to_x((Self::col_x(sheet, summary) + Self::col_x(sheet, summary + 1)) / 2.0);
                if !hidden {
                    let left = to_x(Self::col_x(sheet, start)) + 2.0 * scale;
                    canvas.hline(left, cx - half, cy.round(), line, palette.heading_text);
                    canvas.vline(left.round(), cy, cy + 4.0 * scale, line, palette.heading_text);
                }
                button(canvas, cx, cy, Some(hidden), None);
            }
        }
        if cols > 0 {
            for level in 1..=cols + 1 {
                button(canvas, to_x(gutter_w + HEADER_W / 2.0), to_y(OUTLINE_SLOT * (f64::from(level) - 0.5)), None, Some(level));
            }
        }
    }

    fn active(&self) -> CellRef {
        self.session.active().map(|st| st.selection.active).unwrap_or_default()
    }

    /// What the active cell holds, as it is typed: its formula, else its text.
    fn content(&self, cell: CellRef) -> String {
        let Some((wb, sheet)) = self.sheet() else { return String::new() };
        match sheet.cell(cell).and_then(|c| c.formula.as_ref()) {
            Some(formula) => format!("={}", formula.text),
            None => kition_sheet_engine::display::cell_text(wb, sheet, cell),
        }
    }

    /// Straight RGBA of the canvas area at (`x`, `y`) at `scale` pixels per document pixel.
    fn paint(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Vec<u8> {
        self.paint_with(x, y, scale, width, height, false)
    }

    /// [`Self::paint`]; `plain` draws the sheet as a picture of it (File › Export as Picture):
    /// no gridlines, headings, filter buttons, freeze lines, note marks or cell being typed.
    fn paint_with(&self, x: f64, y: f64, scale: f64, width: u32, height: u32, plain: bool) -> Vec<u8> {
        let background = if plain { WHITE } else { self.scheme.palette().background };
        let Some(st) = self.session.active() else { return Canvas::new(width, height, background).px };
        let si = st.wb.active_sheet;
        let Some(sheet) = st.wb.active() else { return Canvas::new(width, height, background).px };
        // The editor's grid in the host's scheme; a plain picture of the sheet is always light.
        let palette = if plain { &scheme::Palette::LIGHT } else { self.scheme.palette() };
        let paint = Paint { plain, editor: true, headings: !plain && sheet.show_headings, palette };
        self.paint_sheet(&st.wb, si, sheet, self.extent(), (x, y, scale, width, height), paint)
    }

    /// Sheet `si` of `wb` (`sheet`, which may be a copy of it seen from its first row and column)
    /// over its first `cols` and `rows`, as [`Self::paint_with`] draws the active one; `paint` says
    /// whether the editor's own state (the cell or shape being typed) and the headings are drawn.
    fn paint_sheet(&self, wb: &Workbook, si: usize, sheet: &Sheet, (cols, rows): (u32, u32), at: (f64, f64, f64, u32, u32), paint: Paint) -> Vec<u8> {
        let (x, y, scale, width, height) = at;
        let plain = paint.plain;
        let palette = paint.palette;
        let mut canvas = Canvas::new(width, height, palette.background);
        let (cols, rows) = (cols.max(1), rows.max(1));
        let to_x = |dx: f64| (dx - x) * scale;
        let to_y = |dy: f64| (dy - y) * scale;
        let (x1, y1) = (x + f64::from(width) / scale, y + f64::from(height) / scale);
        let (head_w, head_h) = (header_w(sheet), header_h(sheet));
        let c0 = view::col_at(sheet, (x - head_w).max(0.0)).min(cols - 1);
        let c1 = view::col_at(sheet, (x1 - head_w).max(0.0)).min(cols - 1);
        let r0 = view::row_at(sheet, (y - head_h).max(0.0)).min(rows - 1);
        let r1 = view::row_at(sheet, (y1 - head_h).max(0.0)).min(rows - 1);
        let (grid_right, grid_bottom) = (to_x(Self::col_x(sheet, c1 + 1)), to_y(Self::row_y(sheet, r1 + 1)));
        let line = scale.clamp(1.0, 2.0).round();
        let area = RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1));
        let merges: Vec<RangeRef> = sheet.merges.iter().filter(|m| m.intersects(&area)).copied().collect();
        let merge_of = |c: CellRef| merges.iter().find(|m| m.contains(c)).copied();
        // Conditional formats, evaluated as Kition Sheet's grid does (upstream `grid.rs` `look`).
        let mut conditional = kition_sheet_engine::cf::CfCache::new();

        // Gridlines, in the sheet's gridline colour when it has one.
        let gridline = sheet.grid_color.map_or(palette.gridline, |c| [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
        if sheet.show_gridlines && !plain {
            for c in c0..=c1 + 1 {
                if c <= c1 && view::col_w(sheet, c) == 0.0 {
                    continue;
                }
                canvas.vline(to_x(Self::col_x(sheet, c)).round(), to_y(head_h), grid_bottom, line, gridline);
            }
            for r in r0..=r1 + 1 {
                if r <= r1 && view::row_h(sheet, r) == 0.0 {
                    continue;
                }
                canvas.hline(to_x(head_w), grid_right, to_y(Self::row_y(sheet, r)).round(), line, gridline);
            }
        }

        // Each cell (or merged area, once): fill, borders, text.
        for r in r0..=r1 {
            if view::row_h(sheet, r) == 0.0 {
                continue;
            }
            for c in c0..=c1 {
                if view::col_w(sheet, c) == 0.0 {
                    continue;
                }
                let cell = CellRef::new(r, c);
                let range = match merge_of(cell) {
                    // A merged area is drawn from its first cell in the area drawn.
                    Some(m) if !(r == r0.max(m.start.row) && c == c0.max(m.start.col)) => continue,
                    Some(m) => m,
                    None => RangeRef::new(cell, cell),
                };
                let anchor = range.start;
                let mut style = wb.styles.get(sheet.style_id(anchor)).clone();
                if let Some(table) = sheet.table_at(anchor) {
                    style = kition_sheet_engine::tables::table_cell_style(wb, si, table, anchor);
                }
                let look = conditional.look(wb, si, anchor);
                if let Some(look) = &look {
                    apply_conditional(&mut style, look);
                }
                let (dx0, dy0, dx1, dy1) = Self::cell_rect(sheet, range);
                let rect = Rect { x0: to_x(dx0), y0: to_y(dy0), x1: to_x(dx1), y1: to_y(dy1) };
                let scale_fill = look.as_ref().and_then(|look| look.scale_fill).map(rgb);
                let fill = scale_fill.or(match style.fill.pattern {
                    PatternType::None => None,
                    PatternType::Solid => resolve(style.fill.fg, wb),
                    // Patterns: a blend of foreground and background, as Kition Sheet shows them.
                    _ => {
                        let fg = style.fill.fg.resolve(&wb.theme).unwrap_or([0, 0, 0]);
                        let bg = style.fill.bg.resolve(&wb.theme).unwrap_or([255, 255, 255]);
                        Some(rgb([0, 1, 2].map(|i| ((u16::from(fg[i]) + u16::from(bg[i]) * 3) / 4) as u8)))
                    }
                });
                if let Some(fill) = fill {
                    canvas.fill(Rect { x0: rect.x0 + line, y0: rect.y0 + line, ..rect }, fill);
                } else if range.start != range.end {
                    // No gridlines inside a merged area.
                    canvas.fill(Rect { x0: rect.x0 + line, y0: rect.y0 + line, ..rect }, palette.background);
                }
                for (border, edge) in [
                    (style.borders.top, (rect.x0, rect.y0, rect.x1, rect.y0)),
                    (style.borders.bottom, (rect.x0, rect.y1, rect.x1, rect.y1)),
                    (style.borders.left, (rect.x0, rect.y0, rect.x0, rect.y1)),
                    (style.borders.right, (rect.x1, rect.y0, rect.x1, rect.y1)),
                ] {
                    let width = match border.style {
                        BorderStyle::None => continue,
                        BorderStyle::Medium | BorderStyle::MediumDashed | BorderStyle::MediumDashDot | BorderStyle::Double => 2.0,
                        BorderStyle::Thick => 3.0,
                        _ => 1.0,
                    } * scale.clamp(1.0, 2.0);
                    let color = resolve(border.color, wb).unwrap_or(palette.border);
                    let (ax, ay, bx, by) = edge;
                    if ay == by {
                        canvas.hline(ax, bx + width / 2.0, ay.round() - (width / 2.0).floor(), width.round(), color);
                    } else {
                        canvas.vline(ax.round() - (width / 2.0).floor(), ay, by + width / 2.0, width.round(), color);
                    }
                }
                let mut text_rect = rect;
                if let Some(look) = &look {
                    if let Some((fraction, color, gradient)) = look.bar {
                        paint_data_bar(&mut canvas, rect, scale, f64::from(fraction), color, gradient, palette.background);
                    }
                    if let Some((set, index)) = &look.icon {
                        paint_icon(&mut canvas, set, *index, rect, scale);
                        text_rect.x0 += 15.0 * scale;
                    }
                }
                if !look.as_ref().is_some_and(|look| look.hide_value) {
                    self.paint_text(
                        &mut canvas,
                        wb,
                        sheet,
                        anchor,
                        &style,
                        text_rect,
                        scale,
                        (palette.auto_text(fill), palette),
                        (c0, c1),
                        &|c| sheet.value_ref(c).is_none_or(Value::is_empty) && merge_of(c).is_none(),
                        &|col| to_x(Self::col_x(sheet, col)),
                    );
                }
                // A note's (red) or a comment's (purple) mark in the top-right corner.
                if let Some(comment) = sheet.comments.get(&anchor).filter(|_| !plain) {
                    paint_comment_mark(&mut canvas, rect, scale, comment.threaded, palette);
                }
                // An AutoFilter's or a table's filter button at the right of a header cell.
                if let Some(filtered) = filter_button(sheet, anchor).filter(|_| !plain) {
                    paint_filter_button(&mut canvas, rect, scale, filtered, palette);
                }
            }
        }

        // Sparklines, in their cells.
        for spark in &sheet.sparklines {
            let c = spark.cell;
            if c.row < r0 || c.row > r1 || c.col < c0 || c.col > c1 || view::row_h(sheet, c.row) == 0.0 || view::col_w(sheet, c.col) == 0.0 {
                continue;
            }
            let (dx0, dy0, dx1, dy1) = Self::cell_rect(sheet, RangeRef::new(c, c));
            paint_sparkline(&mut canvas, wb, si, spark, Rect { x0: to_x(dx0), y0: to_y(dy0), x1: to_x(dx1), y1: to_y(dy1) }, palette);
        }

        // Freeze lines, just inside the frozen panes, so they stay put with them; under the
        // drawing objects, as Excel draws them.
        // Two pixels wide at 1:1, so a view zoomed out (an exact reduction of 1:1) still shows them.
        let (frozen_rows, frozen_cols) = if plain { (0, 0) } else { sheet.freeze.unwrap_or((0, 0)) };
        let (split_row, split_col) = view::split(sheet);
        let split = 2.0 * line;
        if frozen_rows > 0 {
            canvas.hline(to_x(head_w), grid_right, to_y(Self::row_y(sheet, split_row)).round() - split, split, palette.freeze_line);
        }
        if frozen_cols > 0 {
            canvas.vline(to_x(Self::col_x(sheet, split_col)).round() - split, to_y(head_h), grid_bottom, split, palette.freeze_line);
        }

        // Shapes, over the cells: groups' members each in their own place.
        let bounds = Rect { x0: 0.0, y0: 0.0, x1: f64::from(width), y1: f64::from(height) };
        for shape in &sheet.shapes {
            // The shape being typed shows what was typed so far.
            let typed;
            let shape = match self.shape_edit.as_ref().filter(|_| paint.editor) {
                Some(edit) if edit.id == shape.id => {
                    typed = kition_sheet_engine::model::Shape { text: edit.text.clone(), text_body: None, ..shape.clone() };
                    &typed
                }
                _ => shape,
            };
            let anchor = &shape.anchor;
            let (sx, sy) = (Self::col_x(sheet, anchor.cell.col) + f64::from(anchor.dx), Self::row_y(sheet, anchor.cell.row) + f64::from(anchor.dy));
            self.paint_shape(&mut canvas, wb, shape, (to_x(sx), to_y(sy)), scale, bounds);
        }

        // Smart graphics kept from the file.
        for diagram in &sheet.diagrams {
            let anchor = &diagram.anchor;
            let (dx, dy) = (Self::col_x(sheet, anchor.cell.col) + f64::from(anchor.dx), Self::row_y(sheet, anchor.cell.row) + f64::from(anchor.dy));
            let card = Rect { x0: to_x(dx), y0: to_y(dy), x1: to_x(dx + f64::from(anchor.width)), y1: to_y(dy + f64::from(anchor.height)) };
            self.paint_diagram(&mut canvas, wb, diagram, card, scale, bounds);
        }

        // Form controls kept from the file (check boxes, buttons…), where they sit.
        for control in &sheet.form_controls {
            self.paint_form_control(&mut canvas, sheet, control, (&to_x, &to_y), scale, palette);
        }

        // Charts, over the cells (upstream `chartview.rs` `paint_objects`).
        for chart in &sheet.charts {
            let anchor = &chart.anchor;
            let (cx, cy) = (Self::col_x(sheet, anchor.cell.col) + f64::from(anchor.dx), Self::row_y(sheet, anchor.cell.row) + f64::from(anchor.dy));
            let card = Rect { x0: to_x(cx), y0: to_y(cy), x1: to_x(cx + f64::from(anchor.width)), y1: to_y(cy + f64::from(anchor.height.max(1.0))) };
            if card.intersect(Rect { x0: 0.0, y0: 0.0, x1: f64::from(width), y1: f64::from(height) }).is_empty() {
                continue;
            }
            self.paint_chart(&mut canvas, wb, si, chart, card, scale, palette);
        }

        // The cell being typed, over the cells: its runs, the selected characters and the caret.
        if let (Some(b), Some(edit)) = (self.edit_box(x, y, scale).filter(|_| !plain && paint.editor), self.editing.as_ref()) {
            canvas.fill(b.rect, palette.background);
            let (from, to) = edit.selection();
            for line in &b.lines {
                let (top, under) = (line.baseline - b.ascent, line.baseline + b.descent);
                let end = line.first + line.stops.len().saturating_sub(1);
                let (a, z) = (from.max(line.first), to.min(end));
                if from < to && a <= z && (a < z || to > end) {
                    let right = if to > end { line.x(end) + 4.0 * scale } else { line.x(z) };
                    canvas.fill(Rect { x0: b.start + line.x(a), y0: top, x1: b.start + right, y1: under }, TEXT_SELECTION);
                }
                Segment::draw_all(b.segments.get(line.segments.clone()).unwrap_or_default(), &mut canvas, b.start, line.baseline, b.rect);
            }
            if let Some(line) = b.line_of(edit.caret) {
                let (top, under) = (line.baseline - b.ascent, line.baseline + b.descent);
                canvas.vline((b.start + line.x(edit.caret)).round(), top, under, scale.clamp(1.0, 2.0).round(), palette.editing_text);
            }
        }

        // The headings, after the outline gutter (when rows or columns are grouped).
        let (gutter_w, gutter_h) = (row_gutter(sheet), col_gutter(sheet));
        if paint.headings {
            let (top, left) = (to_y(gutter_h), to_x(gutter_w));
            let (band_h, band_w) = (to_y(head_h), to_x(head_w));
            canvas.fill(Rect { x0: left.max(band_w), y0: top, x1: grid_right, y1: band_h }, palette.heading);
            canvas.fill(Rect { x0: left, y0: band_h.max(top), x1: band_w, y1: grid_bottom }, palette.heading);
            let size = (11.0 * 96.0 / 72.0 * scale) as f32;
            for c in c0..=c1 {
                let (a, b) = (to_x(Self::col_x(sheet, c)), to_x(Self::col_x(sheet, c + 1)));
                canvas.vline(a.round(), top, band_h, line, palette.heading_line);
                let label = col_to_letters(c);
                let text = Line { font: &self.faces.sans, size, text: &label };
                let (ascent, descent) = text.metrics();
                // Whole pixels: a label lands on the same pixels whichever tile draws it.
                let baseline = ((top + band_h) / 2.0 + (ascent - descent) / 2.0).round();
                text.draw(
                    &mut canvas,
                    ((a + b) / 2.0 - text.width() / 2.0).round(),
                    baseline,
                    Rect { x0: a, y0: top, x1: b, y1: band_h },
                    palette.heading_text,
                    false,
                );
            }
            for r in r0..=r1 {
                let (a, b) = (to_y(Self::row_y(sheet, r)), to_y(Self::row_y(sheet, r + 1)));
                canvas.hline(left, band_w, a.round(), line, palette.heading_line);
                let label = (r + 1).to_string();
                let text = Line { font: &self.faces.sans, size, text: &label };
                let (ascent, descent) = text.metrics();
                let baseline = ((a + b) / 2.0 + (ascent - descent) / 2.0).round();
                text.draw(
                    &mut canvas,
                    ((left + band_w) / 2.0 - text.width() / 2.0).round(),
                    baseline,
                    Rect { x0: left, y0: a, x1: band_w, y1: b },
                    palette.heading_text,
                    false,
                );
            }
            canvas.hline(left, grid_right, band_h.round() - line, line, palette.heading_line);
            canvas.vline(band_w.round() - line, top, grid_bottom, line, palette.heading_line);
            canvas.fill(Rect { x0: left, y0: top, x1: band_w, y1: band_h }, palette.heading);
        }
        if (gutter_w > 0.0 || gutter_h > 0.0) && !plain {
            self.paint_outline(&mut canvas, sheet, palette, (x, y, scale), (r0, r1, c0, c1), (grid_right, grid_bottom));
        }
        canvas.px
    }

    /// A form control as WPS and Excel draw it: a check box or option button with its caption, a
    /// button, a group box's frame, a label's text, or the box of a list, a drop-down or a spinner.
    fn paint_form_control(
        &self,
        canvas: &mut Canvas,
        sheet: &Sheet,
        c: &kition_sheet_engine::model::FormControl,
        (to_x, to_y): (&dyn Fn(f64) -> f64, &dyn Fn(f64) -> f64),
        scale: f64,
        palette: &scheme::Palette,
    ) {
        let (from, fx, fy) = c.from;
        let (to, tx, ty) = c.to;
        let rect = Rect {
            x0: to_x(Self::col_x(sheet, from.col) + f64::from(fx)),
            y0: to_y(Self::row_y(sheet, from.row) + f64::from(fy)),
            x1: to_x(Self::col_x(sheet, to.col) + f64::from(tx)),
            y1: to_y(Self::row_y(sheet, to.row) + f64::from(ty)),
        };
        if rect.x1 <= rect.x0 || rect.y1 <= rect.y0 || rect.intersect(Rect { x0: 0.0, y0: 0.0, x1: canvas.w as f64, y1: canvas.h as f64 }).is_empty()
        {
            return;
        }
        // The control's own colours are the editor's: they follow the host's scheme.
        let (edge, ink) = (palette.control_edge, palette.control_ink);
        let line = scale.clamp(1.0, 2.0).round();
        let frame = |canvas: &mut Canvas, r: Rect| {
            canvas.hline(r.x0, r.x1, r.y0, line, edge);
            canvas.hline(r.x0, r.x1, r.y1 - line, line, edge);
            canvas.vline(r.x0, r.y0, r.y1, line, edge);
            canvas.vline(r.x1 - line, r.y0, r.y1, line, edge);
        };
        let size = (11.0 * 96.0 / 72.0 * scale) as f32;
        let caption = |canvas: &mut Canvas, x: f64, clip: Rect, centred: bool| {
            let text = Line { font: &self.faces.sans, size, text: c.text.lines().next().unwrap_or_default() };
            let (ascent, descent) = text.metrics();
            let width = text.width();
            let x = if centred { (clip.x0 + clip.x1) / 2.0 - width / 2.0 } else { x };
            text.draw(canvas, x, (clip.y0 + clip.y1) / 2.0 + (ascent - descent) / 2.0, clip, ink, false);
        };
        match c.kind.as_str() {
            "Checkbox" | "Radio" => {
                let side = (12.0 * scale).min(rect.y1 - rect.y0);
                let top = (rect.y0 + rect.y1) / 2.0 - side / 2.0;
                let mark = Rect { x0: rect.x0 + 2.0 * scale, y0: top, x1: rect.x0 + 2.0 * scale + side, y1: top + side };
                canvas.fill(mark, palette.control_inside);
                frame(canvas, mark);
                if c.checked {
                    let inset = (side * 0.28).round();
                    canvas.fill(Rect { x0: mark.x0 + inset, y0: mark.y0 + inset, x1: mark.x1 - inset, y1: mark.y1 - inset }, ink);
                }
                caption(canvas, mark.x1 + 4.0 * scale, rect, false);
            }
            "Button" => {
                canvas.fill(rect, palette.control_face);
                frame(canvas, rect);
                caption(canvas, rect.x0, rect, true);
            }
            "GBox" => {
                frame(canvas, rect);
                caption(canvas, rect.x0 + 6.0 * scale, Rect { y1: rect.y0 + 16.0 * scale, ..rect }, false);
            }
            "Label" => caption(canvas, rect.x0 + 2.0 * scale, rect, false),
            _ => {
                canvas.fill(rect, palette.control_inside);
                frame(canvas, rect);
            }
        }
    }

    /// A cell's value, formatted and aligned as Kition Sheet does (upstream `grid.rs`
    /// `paint_quadrant` and `display_text`): numbers right and `###` when too narrow; text wider
    /// than its cell runs on into empty neighbours (right of left-aligned text, left of
    /// right-aligned text, both ways from centred text); a checkbox cell draws its box; turned
    /// text (`format.orientation`) is drawn turned, vertical text stacked.
    #[allow(clippy::too_many_arguments)]
    fn paint_text(
        &self,
        canvas: &mut Canvas,
        wb: &Workbook,
        sheet: &Sheet,
        cell: CellRef,
        style: &kition_sheet_engine::model::Style,
        rect: Rect,
        scale: f64,
        (auto, palette): (Rgba, &scheme::Palette),
        (first_col, last_col): (u32, u32),
        empty: &dyn Fn(CellRef) -> bool,
        col_x: &dyn Fn(u32) -> f64,
    ) {
        let value = sheet.value(cell);
        if style.checkbox
            && let Value::Bool(checked) = value
        {
            paint_checkbox(canvas, rect, scale, style, checked, resolve(style.font.color, wb), palette);
            return;
        }
        if value.is_empty() {
            return;
        }
        if !sheet.show_zeros && value.as_f64() == Some(0.0) {
            return;
        }
        let picked = self.faces.pick(&style.font.name, style.font.bold, style.font.italic);
        let font = &picked.face;
        let size = (f64::from(style.font.size) * 96.0 / 72.0 * scale) as f32;
        let pad = 2.5 * scale;
        let char_w = Line { font, size, text: "0" }.width().max(1.0);
        let avail = rect.x1 - rect.x0 - 2.0 * pad;
        let code = style.num_fmt.0.as_ref();
        let (mut text, color, numeric) = match (&value, code) {
            _ if sheet.show_formulas && sheet.cell(cell).is_some_and(|c| c.formula.is_some()) => {
                (sheet.cell(cell).and_then(|c| c.formula.as_ref()).map(|f| format!("={}", f.text)).unwrap_or_default(), None, false)
            }
            (Value::Number(n), "General") => {
                let fit = ((avail - 1.0) / char_w).floor().max(1.0) as usize;
                (kition_sheet_engine::numfmt::format_general_fit(*n, fit.min(11)).unwrap_or_else(|| "#".repeat(fit)), None, true)
            }
            _ => {
                let formatted = kition_sheet_engine::display::format(&value, code, wb);
                (formatted.text, formatted.color.map(format_color), formatted.numeric)
            }
        };
        let color = color.or_else(|| resolve(style.font.color, wb)).unwrap_or(auto);
        let numeric = numeric || matches!(value, Value::Number(_));
        if style.align.rotation != 0 {
            self.paint_turned(canvas, &text, style, rect, scale, color);
            return;
        }
        // Rich text runs, drawn while the cell shows its text as it is (not a formula, not
        // reformatted by a number format's text section).
        let rich = match &value {
            Value::Text(raw) if !sheet.show_formulas && text == **raw => {
                sheet.cell(cell).and_then(kition_sheet_engine::model::Cell::runs).map(|runs| Runs { runs, base: &style.font, color, scale, wb })
            }
            _ => None,
        };
        if style.align.wrap && !numeric {
            self.paint_wrapped(canvas, &text, font, size, style, rect, scale, color, rich.as_ref());
            return;
        }
        text = text.lines().next().unwrap_or_default().to_owned();
        if text.is_empty() {
            return;
        }
        let hashes;
        let segments = match &rich {
            Some(rich) => rich.segments(&self.faces, 0, text.chars().count()),
            None => {
                let mut shown = text.as_str();
                if numeric && (Line { font, size, text: shown }).width() > avail {
                    hashes = "#".repeat((avail / char_w).floor().max(1.0) as usize);
                    shown = &hashes;
                }
                vec![Segment {
                    text: shown.to_owned(),
                    face: font.clone(),
                    size,
                    rise: 0.0,
                    color,
                    bold: picked.bold,
                    heavy: style.font.bold,
                    italic: picked.italic,
                    underline: style.font.underline,
                    strike: style.font.strike,
                }]
            }
        };
        let (width, _, _) = Segment::measure(&segments);
        let halign = match style.align.h {
            HAlign::General if numeric => HAlign::Right,
            HAlign::General if rich.is_none() && matches!(value, Value::Bool(_) | Value::Error(_)) => HAlign::Center,
            HAlign::General => HAlign::Left,
            other => other,
        };
        let indent = f64::from(style.align.indent) * 9.0 * scale;
        let x = match halign {
            HAlign::Center | HAlign::CenterAcross => (rect.x0 + rect.x1) / 2.0 - width / 2.0,
            HAlign::Right => rect.x1 - pad - width - indent,
            _ => rect.x0 + pad + indent,
        };
        // One line in WPS's line box (its pitch, its baseline from the top).
        let (pitch, first) = line_box(style.font.size, scale);
        let baseline = match style.align.v {
            VAlign::Top => rect.y0 + first,
            VAlign::Center | VAlign::Justify | VAlign::Distributed => (rect.y0 + rect.y1) / 2.0 - pitch / 2.0 + first,
            VAlign::Bottom => rect.y1 - pitch + first,
        };
        // Text runs on into empty cells beside it: right of left-aligned text, left of
        // right-aligned text, both ways from centred text (numbers never do).
        let mut clip = rect;
        if !numeric && width > avail {
            let (right, left) = match halign {
                HAlign::Left | HAlign::General => (true, false),
                HAlign::Right => (false, true),
                HAlign::Center | HAlign::CenterAcross => (true, true),
                _ => (false, false),
            };
            if right {
                let mut col = cell.col + 1;
                while clip.x1 < x + width + pad && col < MAX_COLS && col <= last_col + OVERFLOW_COLS && empty(CellRef::new(cell.row, col)) {
                    clip.x1 = col_x(col + 1);
                    col += 1;
                }
            }
            if left {
                let mut col = cell.col;
                while clip.x0 > x - pad && col > 0 && col + OVERFLOW_COLS >= first_col && empty(CellRef::new(cell.row, col - 1)) {
                    col -= 1;
                    clip.x0 = col_x(col);
                }
            }
            // Over the gridlines the text passes.
            let background = resolve_fill_or_white(style, wb);
            if clip.x1 > rect.x1 {
                canvas.fill(Rect { x0: rect.x1 - scale, y0: rect.y0 + scale, x1: clip.x1 - scale, y1: rect.y1 }, background);
            }
            if clip.x0 < rect.x0 {
                canvas.fill(Rect { x0: clip.x0 + scale, y0: rect.y0 + scale, x1: rect.x0 + scale, y1: rect.y1 }, background);
            }
        }
        Segment::draw_all(&segments, canvas, x, baseline, clip);
    }

    /// Text turned by the cell's orientation (degrees counterclockwise, -90..=90), or stacked
    /// one character under another (vertical, 255), inside its cell.
    #[allow(clippy::too_many_arguments)]
    fn paint_turned(&self, canvas: &mut Canvas, text: &str, style: &kition_sheet_engine::model::Style, rect: Rect, scale: f64, color: Rgba) {
        use kition_office_core::kurbo::{Affine, BezPath, Point};
        let picked = self.faces.pick(&style.font.name, style.font.bold, style.font.italic);
        let size = (f64::from(style.font.size) * 96.0 / 72.0 * scale) as f32;
        let text = text.lines().next().unwrap_or_default();
        let pad = 2.5 * scale;
        let (ascent, descent) = Line { font: &picked.face, size, text: "Hg" }.metrics();
        let mut path = BezPath::new();
        let (bw, bh);
        if style.align.rotation == 255 {
            // Stacked: each character centred on its own line.
            let step = (ascent + descent) * 1.05;
            for (i, c) in text.chars().take(200).enumerate() {
                let mut buf = [0u8; 4];
                let one = Line { font: &picked.face, size, text: c.encode_utf8(&mut buf) };
                path.extend(one.outline(-one.width() / 2.0, ascent + step * i as f64, picked.italic));
            }
            let count = text.chars().take(200).count() as f64;
            (bw, bh) = (f64::from(size), step * count);
            // Origin: the top centre of the stack.
            let x = match style.align.h {
                HAlign::Left => rect.x0 + pad + bw / 2.0,
                HAlign::Right => rect.x1 - pad - bw / 2.0,
                _ => (rect.x0 + rect.x1) / 2.0,
            };
            let y = match style.align.v {
                VAlign::Top => rect.y0 + pad,
                VAlign::Center | VAlign::Justify | VAlign::Distributed => (rect.y0 + rect.y1) / 2.0 - bh / 2.0,
                VAlign::Bottom => rect.y1 - pad - bh,
            };
            path.apply_affine(Affine::translate((x, y)));
        } else {
            let line = Line { font: &picked.face, size, text };
            let width = line.width();
            // The text on its baseline from the origin, turned about the origin.
            path = line.outline(0.0, 0.0, picked.italic);
            let angle = -f64::from(style.align.rotation.clamp(-90, 90)).to_radians();
            let turn = Affine::rotate(angle);
            let corners =
                [Point::new(0.0, -ascent), Point::new(width, -ascent), Point::new(width, descent), Point::new(0.0, descent)].map(|p| turn * p);
            let (min_x, max_x) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
            let (min_y, max_y) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
            (bw, bh) = (max_x - min_x, max_y - min_y);
            let left = match style.align.h {
                HAlign::Right => rect.x1 - pad - bw,
                HAlign::Center | HAlign::CenterAcross => (rect.x0 + rect.x1) / 2.0 - bw / 2.0,
                // Text turned up starts at the bottom left, as Excel sets it.
                _ => rect.x0 + pad,
            };
            let top = match style.align.v {
                VAlign::Top => rect.y0 + pad,
                VAlign::Center | VAlign::Justify | VAlign::Distributed => (rect.y0 + rect.y1) / 2.0 - bh / 2.0,
                VAlign::Bottom => rect.y1 - pad - bh,
            };
            path.apply_affine(Affine::translate((left - min_x, top - min_y)) * turn);
        }
        let _ = (bw, bh);
        let area = rect.intersect(Rect { x0: 0.0, y0: 0.0, x1: canvas.w as f64, y1: canvas.h as f64 });
        if area.is_empty() {
            return;
        }
        let (x0, y0) = (area.x0.floor(), area.y0.floor());
        let (iw, ih) = ((area.x1.ceil() - x0) as u32, (area.y1.ceil() - y0) as u32);
        if u64::from(iw) * u64::from(ih) > 16 * 1024 * 1024 {
            return;
        }
        path.apply_affine(Affine::translate((-x0, -y0)));
        let mut contours: Vec<Vec<[f32; 2]>> = Vec::new();
        kition_office_core::kurbo::flatten(&path, 0.2, |el| match el {
            kition_office_core::kurbo::PathEl::MoveTo(p) => contours.push(vec![[p.x as f32, p.y as f32]]),
            kition_office_core::kurbo::PathEl::LineTo(p) => {
                if let Some(c) = contours.last_mut() {
                    c.push([p.x as f32, p.y as f32]);
                }
            }
            _ => {}
        });
        let prims = [kition_sheet_chart::Prim::Path { contours, paint: kition_sheet_chart::Paint::Solid(color) }];
        let image = kition_sheet_chart::rasterize(&prims, iw, ih, [255, 255, 255, 0]);
        canvas.image(&image, iw as usize, ih as usize, x0, y0, area);
    }

    /// Text that wraps within its cell (Wrap Text): words onto as many lines as the width allows,
    /// any character for scripts without spaces, aligned as the cell's text is.
    #[allow(clippy::too_many_arguments)]
    fn paint_wrapped(
        &self,
        canvas: &mut Canvas,
        text: &str,
        font: &ab_glyph::FontArc,
        size: f32,
        style: &kition_sheet_engine::model::Style,
        rect: Rect,
        scale: f64,
        color: Rgba,
        rich: Option<&Runs<'_>>,
    ) {
        // What the face leaves to synthesise.
        let picked = self.faces.pick(&style.font.name, style.font.bold, style.font.italic);
        // The engine's line breaks (the ones row auto-fit counts), at WPS's line pitch.
        let width = ((rect.x1 - rect.x0) / scale - f64::from(kition_sheet_engine::text_layout::CELL_PADDING_PX)).max(1.0);
        let lines = kition_sheet_engine::text_layout::wrap_lines(text, &style.font, width as f32);
        let pad = 2.5 * scale;
        let (pitch, first_baseline) = line_box(style.font.size, scale);
        let block = pitch * lines.len() as f64;
        // Text taller than its cell starts at the top and is cut at the bottom (WPS).
        let top = match style.align.v {
            _ if block > rect.y1 - rect.y0 => rect.y0,
            VAlign::Top => rect.y0,
            VAlign::Center | VAlign::Justify | VAlign::Distributed => (rect.y0 + rect.y1) / 2.0 - block / 2.0,
            VAlign::Bottom => rect.y1 - block,
        };
        // Where each line starts in the text (in characters), for the runs it shows.
        let mut cursor = 0;
        for (index, text_line) in lines.iter().enumerate() {
            let start = match text.get(cursor..).and_then(|rest| rest.find(text_line.as_str())) {
                Some(at) => cursor + at,
                None => cursor,
            };
            let first = text.get(..start).map_or(0, |head| head.chars().count());
            cursor = start + text_line.len();
            let baseline = top + first_baseline + pitch * index as f64;
            let x_for = |width: f64| match style.align.h {
                HAlign::Center | HAlign::CenterAcross => (rect.x0 + rect.x1) / 2.0 - width / 2.0,
                HAlign::Right => rect.x1 - pad - width,
                _ => rect.x0 + pad,
            };
            if let Some(rich) = rich {
                let segments = rich.segments(&self.faces, first, first + text_line.chars().count());
                let (width, _, _) = Segment::measure(&segments);
                Segment::draw_all(&segments, canvas, x_for(width), baseline, rect);
                continue;
            }
            let segment = Segment {
                text: text_line.clone(),
                face: font.clone(),
                size,
                rise: 0.0,
                color,
                bold: picked.bold,
                heavy: style.font.bold,
                italic: picked.italic,
                underline: style.font.underline,
                strike: style.font.strike,
            };
            let width = segment.line().width();
            Segment::draw_all(&[segment], canvas, x_for(width), baseline, rect);
        }
    }

    /// One chart as Kition Sheet draws it: a white card, the chart's shapes rasterised at this
    /// scale, and its labels in the cell faces (rotated labels, such as vertical axis titles, are
    /// left out).
    #[allow(clippy::too_many_arguments)]
    fn paint_chart(
        &self,
        canvas: &mut Canvas,
        wb: &Workbook,
        si: usize,
        chart: &kition_sheet_engine::model::Chart,
        card: Rect,
        scale: f64,
        palette: &scheme::Palette,
    ) {
        let (w, h) = ((card.x1 - card.x0).round().max(1.0), (card.y1 - card.y0).round().max(1.0));
        if w * h > 32.0 * 1024.0 * 1024.0 {
            return;
        }
        canvas.fill(Rect { x0: card.x0, y0: card.y0 + 1.5 * scale, x1: card.x1, y1: card.y1 + 1.5 * scale }, [0, 0, 0, 18]);
        let data = kition_sheet_chart::resolve(wb, si, chart);
        let measure = FaceMeasure { faces: &self.faces, name: &wb.theme.minor_font };
        // The chart's automatic colours (its area, text, gridlines, axes) in the editor's scheme;
        // its series keep the file's.
        let prims = kition_sheet_chart::render_in(chart, &data, chart.anchor.width, chart.anchor.height, &measure, &palette.chart);
        let s = scale as f32;
        let scaled: Vec<kition_sheet_chart::Prim> = prims.iter().map(|prim| scale_prim(prim, s)).collect();
        let image = kition_sheet_chart::rasterize(&scaled, w as u32, h as u32, palette.chart.background);
        canvas.image(&image, w as usize, h as usize, card.x0, card.y0, card);
        self.paint_texts(canvas, wb, &scaled, (card.x0, card.y0), card);
    }

    /// The text primitives of a chart or diagram, at `at` (canvas pixels) and within `clip`, in the
    /// theme's body font as Excel sets chart text.
    fn paint_texts(&self, canvas: &mut Canvas, wb: &Workbook, prims: &[kition_sheet_chart::Prim], at: (f64, f64), clip: Rect) {
        for prim in prims {
            let kition_sheet_chart::Prim::Text { x, y, text, size, color, bold, align, valign, rotation } = prim else { continue };
            if rotation.abs() > 0.01 {
                continue;
            }
            let picked = self.faces.pick(&wb.theme.minor_font, *bold, false);
            let line = Line { font: &picked.face, size: *size, text };
            let (ascent, descent) = line.metrics();
            let width = line.width();
            let left = f64::from(*x)
                - match align {
                    kition_sheet_chart::HAlign::Left => 0.0,
                    kition_sheet_chart::HAlign::Center => width / 2.0,
                    kition_sheet_chart::HAlign::Right => width,
                };
            let top = f64::from(*y)
                - match valign {
                    kition_sheet_chart::VAlign::Top => 0.0,
                    kition_sheet_chart::VAlign::Middle => (ascent + descent) / 2.0,
                    kition_sheet_chart::VAlign::Bottom => ascent + descent,
                };
            line.draw(canvas, at.0 + left, at.1 + top + ascent, clip, *color, picked.bold);
        }
    }

    /// A smart graphic in its `card` (canvas pixels): its kept drawing rasterised over the part of
    /// the canvas it covers, then its text (with the prompts of empty nodes, as an editor shows them).
    fn paint_diagram(&self, canvas: &mut Canvas, wb: &Workbook, diagram: &kition_sheet_engine::model::Diagram, card: Rect, scale: f64, bounds: Rect) {
        let area = card.intersect(bounds);
        if area.is_empty() {
            return;
        }
        let drawing = kition_sheet_chart::render_diagram(diagram, &wb.theme, diagram.anchor.width, diagram.anchor.height, true);
        let (x0, y0) = (area.x0.floor(), area.y0.floor());
        let (iw, ih) = ((area.x1.ceil() - x0) as u32, (area.y1.ceil() - y0) as u32);
        let prims: Vec<kition_sheet_chart::Prim> =
            drawing.prims.iter().map(|p| p.scaled(scale as f32).translated((card.x0 - x0) as f32, (card.y0 - y0) as f32)).collect();
        if u64::from(iw) * u64::from(ih) <= 16 * 1024 * 1024 {
            let image = kition_sheet_chart::rasterize(&prims, iw, ih, [255, 255, 255, 0]);
            canvas.image(&image, iw as usize, ih as usize, x0, y0, area);
        }
        self.paint_texts(canvas, wb, &prims, (x0, y0), area);
    }

    /// One shape with its top left at `at` (canvas pixels): its outline and fill rasterised over
    /// the part of the canvas it covers (with their transparency), then its text in the cell faces.
    fn paint_shape(&self, canvas: &mut Canvas, wb: &Workbook, shape: &kition_sheet_engine::model::Shape, at: (f64, f64), scale: f64, bounds: Rect) {
        let (w, h) = (f64::from(shape.anchor.width.max(0.0)), f64::from(shape.anchor.height.max(0.0)));
        // Turned shapes and pens reach past the box.
        let pad = 8.0 + w.max(h) * 0.5;
        let reach = Rect { x0: at.0 - pad * scale, y0: at.1 - pad * scale, x1: at.0 + (w + pad) * scale, y1: at.1 + (h + pad) * scale };
        let area = reach.intersect(bounds);
        if area.is_empty() {
            return;
        }
        let (x0, y0) = (area.x0.floor(), area.y0.floor());
        let (iw, ih) = ((area.x1.ceil() - x0) as u32, (area.y1.ceil() - y0) as u32);
        let mut prims: Vec<kition_sheet_chart::Prim> = kition_sheet_chart::render_shape(shape, &wb.theme, w as f32, h as f32)
            .iter()
            .map(|p| p.scaled(scale as f32).translated((at.0 - x0) as f32, (at.1 - y0) as f32))
            .collect();
        let text = kition_sheet_engine::shape_text::shape_text(shape);
        // Art text: the text's outlines, bent by its warp, drawn with the shape (in the face the
        // engine prints them in).
        let warp = text.warp.as_ref().filter(|(name, _)| kition_office_core::warp::is_warp(name));
        if let Some((preset, adjust)) = warp {
            let measure =
                |_: &kition_sheet_engine::shape_text::ShapeTextRun, size: f32, s: &str| Line { font: &self.faces.sans, size, text: s }.width() as f32;
            let pieces = kition_sheet_engine::shape_text::layout(&text, w as f32, h as f32, &measure);
            let runs: Vec<(Rgba, kition_office_core::kurbo::BezPath)> = pieces
                .iter()
                .take(5000)
                .map(|piece| {
                    let color = kition_sheet_chart::shape_rgba(piece.run.color, piece.run.alpha, &wb.theme).unwrap_or([0, 0, 0, 255]);
                    let line = Line { font: &self.faces.sans, size: piece.size, text: &piece.text };
                    (color, line.outline(f64::from(piece.x), f64::from(piece.baseline), piece.run.italic))
                })
                .collect();
            let px = 96.0 / 72.0;
            let [il, it, ir, ib] = text.insets.map(|v| f64::from(v) * px);
            let margins = (il, it, (w - il - ir).max(1.0), (h - it - ib).max(1.0));
            let place = kition_office_core::kurbo::Affine::translate(((at.0 - x0), (at.1 - y0))) * kition_office_core::kurbo::Affine::scale(scale);
            prims.extend(warped_text(&runs, margins, preset, adjust, place));
        }
        if u64::from(iw) * u64::from(ih) <= 16 * 1024 * 1024 {
            let image = kition_sheet_chart::rasterize(&prims, iw, ih, [255, 255, 255, 0]);
            canvas.image(&image, iw as usize, ih as usize, x0, y0, area);
        }
        if warp.is_some() {
            return;
        }
        let measure = |run: &kition_sheet_engine::shape_text::ShapeTextRun, size: f32, s: &str| {
            Line { font: &self.shape_face(&wb.theme, run).face, size, text: s }.width() as f32
        };
        for piece in kition_sheet_engine::shape_text::layout(&text, w as f32, h as f32, &measure).iter().take(5000) {
            let color = kition_sheet_chart::shape_rgba(piece.run.color, piece.run.alpha, &wb.theme).unwrap_or([0, 0, 0, 255]);
            let size = piece.size * scale as f32;
            let picked = self.shape_face(&wb.theme, piece.run);
            let line = Line { font: &picked.face, size, text: &piece.text };
            let (px, py) = (at.0 + f64::from(piece.x) * scale, at.1 + f64::from(piece.baseline) * scale);
            line.draw_styled(canvas, px, py, bounds, color, picked.bold, picked.italic);
            let thick = (f64::from(size) / 16.0).max(1.0);
            if piece.run.underline {
                canvas.hline(px, px + line.width(), (py + f64::from(size) * 0.12).round(), thick.round(), color);
            }
            if piece.run.strike {
                canvas.hline(px, px + line.width(), (py - f64::from(size) * 0.3).round(), thick.round(), color);
            }
        }
    }

    /// The face a shape's text run is drawn in: its font (a theme font named as `+mn-lt` or
    /// `+mj-lt` is the theme's), else the theme's body font, as Excel sets it.
    fn shape_face(&self, theme: &kition_sheet_engine::model::Theme, run: &kition_sheet_engine::shape_text::ShapeTextRun) -> draw::Picked {
        let name = match run.font.as_deref() {
            Some(theme_font) if theme_font.starts_with("+mj") => theme.major_font.as_str(),
            Some(name) if !name.starts_with('+') => name,
            _ => theme.minor_font.as_str(),
        };
        self.faces.pick(name, run.bold, run.italic)
    }

    /// The canvas cell under (`x`, `y`) in the grid, or the heading row and column there.
    fn hit(&self, x: f64, y: f64) -> Hit {
        let Some((_, sheet)) = self.sheet() else { return Hit::Cell(CellRef::default()) };
        let (cols, rows) = self.extent();
        if let Some(hit) = outline_hit(sheet, x, y, (cols, rows)) {
            return hit;
        }
        let (head_w, head_h) = (header_w(sheet), header_h(sheet));
        let col = view::col_at(sheet, (x - head_w).max(0.0)).min(cols - 1);
        let row = view::row_at(sheet, (y - head_h).max(0.0)).min(rows - 1);
        match (x < head_w, y < head_h) {
            (true, true) => Hit::All,
            (false, true) => Hit::Column(col),
            (true, false) => Hit::Row(row),
            (false, false) => Hit::Cell(CellRef::new(row, col)),
        }
    }

    /// A right-click on the grid at document point (`x`, `y`) (`editor.contextMenu`): what is
    /// being typed goes in, and the cell, row or column heading pressed is selected unless it is
    /// already in the selection (WPS, Excel), so the menu acts on it. Answers which menu shows:
    /// `cell`, `rows`, `cols`, `all`, or `null` (an outline button, the outline gutter).
    /// File › Export as Picture (WPS 输出为图片): the active sheet's cells (its print area, or
    /// `range`, else its used range and objects) as a PNG, drawn as the grid draws them without
    /// gridlines or headings, on a white margin. `scale` 1 to 4 (default 1: 96 dpi).
    fn export_picture(&self, params: &Json) -> Result<Json, String> {
        const MARGIN: f64 = 20.0;
        let st = self.session.active().ok_or("No workbook is open.")?;
        let wb = &st.wb;
        let scale = params.get("scale").and_then(Json::as_f64).filter(|v| v.is_finite()).unwrap_or(1.0).clamp(1.0, 4.0);
        let given = params.get("range").and_then(Json::as_str).map(str::trim).filter(|r| !r.is_empty());
        // The sheet the range names (`Notes!A1:B2`), else the active one.
        let si = match given.and_then(|r| r.rsplit_once('!')) {
            Some((name, _)) => {
                let name = name.trim_matches('\'').replace("''", "'");
                wb.sheet_index(&name).ok_or_else(|| format!("the workbook has no sheet named `{name}`"))?
            }
            None => wb.active_sheet,
        };
        let sheet = wb.sheet(si).ok_or("No workbook is open.")?;
        let range = match given {
            Some(r) => RangeRef::parse(r.rsplit('!').next().unwrap_or(r)).ok_or_else(|| format!("not a range: `{r}`"))?,
            None => kition_sheet_engine::cmd::print::print_range(sheet).unwrap_or(RangeRef::cell(CellRef::new(0, 0))),
        };
        // The active sheet as the editor lays it out; another as its used range does.
        let extent = if si == wb.active_sheet { self.extent() } else { Self::sheet_extent(sheet) };
        let (x0, y0) = (Self::col_x(sheet, range.start.col), Self::row_y(sheet, range.start.row));
        let (x1, y1) = (Self::col_x(sheet, range.end.col + 1), Self::row_y(sheet, range.end.row + 1));
        let w = ((x1 - x0 + 2.0 * MARGIN) * scale).ceil().max(1.0);
        let h = ((y1 - y0 + 2.0 * MARGIN) * scale).ceil().max(1.0);
        // Pictures larger than this are refused rather than allocated (400 megapixels).
        if w * h > 400_000_000.0 {
            return Err("The sheet is too large to export as one picture; export a smaller range.".into());
        }
        let (w, h) = (w as u32, h as u32);
        let mut image = vec![255u8; (w as usize) * (h as usize) * 4];
        let tile = MAX_RENDER_SIDE.min(4096);
        let mut ty = 0;
        while ty < h {
            let th = tile.min(h - ty);
            let mut tx = 0;
            while tx < w {
                let tw = tile.min(w - tx);
                let at = (x0 - MARGIN + f64::from(tx) / scale, y0 - MARGIN + f64::from(ty) / scale, scale, tw, th);
                let paint = Paint { plain: true, editor: false, headings: false, palette: &scheme::Palette::LIGHT };
                let px = self.paint_sheet(wb, si, sheet, extent, at, paint);
                for row in 0..th {
                    let from = (row as usize) * (tw as usize) * 4;
                    let to = (((ty + row) as usize) * (w as usize) + tx as usize) * 4;
                    if let (Some(src), Some(dst)) = (px.get(from..from + tw as usize * 4), image.get_mut(to..to + tw as usize * 4)) {
                        dst.copy_from_slice(src);
                    }
                }
                tx += tw;
            }
            ty += th;
        }
        // The margin is the page's white, not the sheet's.
        let edge = (MARGIN * scale) as u32;
        for y in 0..h {
            for x in 0..w {
                if x < edge || y < edge || x + edge > w || y + edge > h {
                    let at = ((y as usize) * (w as usize) + x as usize) * 4;
                    if let Some(px) = image.get_mut(at..at + 4) {
                        px.copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
            }
        }
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(&image, w, h, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
        Ok(json!({
            "format": "png", "extension": "png", "mime": "image/png", "bytes": png.len(), "width": w, "height": h,
            "range": range.a1(), "data": kition_sheet_engine::io::base64_encode(&png),
        }))
    }

    fn context_menu(&mut self, params: &Json) -> Result<Json, String> {
        let at = |key: &str| params.get(key).and_then(Json::as_f64).filter(|v| v.is_finite()).unwrap_or(0.0);
        if self.editing.is_some() {
            if self.commit().is_err() {
                self.editing = None;
                self.edit_home = None;
            }
            self.edits += 1;
        }
        let selection = self.session.active().map(|st| st.selection.clone()).unwrap_or_default();
        let target = match self.hit(at("x"), at("y")) {
            Hit::Cell(cell) => {
                if !selection.contains(cell) {
                    self.select(RangeRef::new(cell, cell), cell)?;
                }
                "cell"
            }
            Hit::Row(row) => {
                if !selection.ranges.iter().any(|r| r.is_full_rows() && r.start.row <= row && row <= r.end.row) {
                    self.select(RangeRef::new(CellRef::new(row, 0), CellRef::new(row, MAX_COLS - 1)), CellRef::new(row, 0))?;
                }
                "rows"
            }
            Hit::Column(col) => {
                if !selection.ranges.iter().any(|r| r.is_full_cols() && r.start.col <= col && col <= r.end.col) {
                    self.select(RangeRef::new(CellRef::new(0, col), CellRef::new(MAX_ROWS - 1, col)), CellRef::new(0, col))?;
                }
                "cols"
            }
            Hit::All => {
                self.select(RangeRef::all(), self.active())?;
                "all"
            }
            _ => return Ok(json!({ "target": null })),
        };
        Ok(json!({ "target": target }))
    }

    /// The cell at the view's top-left: the first row and column wholly in view (the frozen
    /// panes' own top-left while they are frozen).
    fn view_top_cell(&self) -> Option<CellRef> {
        let (_, sheet) = self.sheet()?;
        if sheet.freeze.is_some() {
            let (row, col) = view::origin(sheet);
            return Some(CellRef::new(row, col));
        }
        let (x, y) = self.view_at;
        let first = |at: u32, start: f64, find: &dyn Fn(u32) -> f64| if find(at) < start - 1.0 { at.saturating_add(1) } else { at };
        let col = first(view::col_at(sheet, x), x, &|c| view::left(sheet, c));
        let row = first(view::row_at(sheet, y), y, &|r| view::top(sheet, r));
        Some(CellRef::new(row.min(MAX_ROWS - 1), col.min(MAX_COLS - 1)))
    }

    fn select(&mut self, range: RangeRef, active: CellRef) -> Result<(), String> {
        self.session.execute("select.set", json!({ "range": range.a1(), "active": active.a1() })).map(|_| ()).map_err(engine_error)
    }

    /// Adds `area` to the selection with `active` as its active cell and anchor (Cmd+click).
    fn add_area(&mut self, area: RangeRef, active: CellRef) -> Result<(), String> {
        let mut ranges = self.session.active().map(|st| st.selection.a1()).unwrap_or_default();
        if !ranges.is_empty() {
            ranges.push(',');
        }
        ranges.push_str(&area.a1());
        self.session.execute("select.set", json!({ "range": ranges, "active": active.a1() })).map_err(engine_error)?;
        if let Ok(st) = self.session.doc_mut() {
            st.selection.anchor = active;
        }
        Ok(())
    }

    /// Cmd+click on a selected cell (WPS W-A6): the cell (its merge) leaves the selection, the
    /// areas around it splitting into the pieces left; the active cell stays if it is still
    /// selected, else the last piece's first cell is. Answers whether the press was on a selected
    /// cell (the only selected cell stays selected).
    fn deselect(&mut self, cell: CellRef) -> Result<bool, String> {
        let Some((selection, hole)) = self
            .session
            .active()
            .zip(self.sheet())
            .map(|(st, (_, sheet))| (st.selection.clone(), sheet.merge_at(cell).unwrap_or(RangeRef::cell(cell))))
        else {
            return Ok(false);
        };
        if !selection.contains(cell) {
            return Ok(false);
        }
        let mut ranges: Vec<RangeRef> = Vec::new();
        for area in &selection.ranges {
            if area.intersects(&hole) {
                ranges.extend(without(*area, hole));
            } else {
                ranges.push(*area);
            }
        }
        if ranges.is_empty() {
            return Ok(true);
        }
        let active = if ranges.iter().any(|r| r.contains(selection.active)) { selection.active } else { ranges.last().map_or(cell, |r| r.start) };
        // The area with the active cell goes last, as the selection keeps it.
        if let Some(at) = ranges.iter().rposition(|r| r.contains(active)) {
            let area = ranges.remove(at);
            ranges.push(area);
        }
        let text = ranges.iter().map(RangeRef::a1).collect::<Vec<_>>().join(",");
        self.session.execute("select.set", json!({ "range": text, "active": active.a1() })).map_err(engine_error)?;
        Ok(true)
    }

    /// Enters the text being typed into the active cell, with its runs.
    fn commit(&mut self) -> Result<(), String> {
        self.point = None;
        self.hint_pick = 0;
        if let Some(edit) = self.editing.take() {
            self.edits += 1;
            // A formula finished on another sheet (pointing at its cells) goes into its own cell,
            // and that sheet shows again.
            let (sheet, cell) = self.edit_home();
            // Data validation (WPS, Excel): a typed value the cell's rule refuses stays in the cell
            // being typed, with the rule's message; a warning or a note lets it in and says so.
            // A warning or a note answered Yes lets it in (`validation`).
            let text = edit.text();
            if !text.starts_with('=')
                && !self.validation_skip
                && let Some((wb, _)) = self.sheet()
                && let Some((rule, message)) = kition_sheet_engine::cmd::data::check_validation(wb, sheet, cell, &text)
            {
                self.editing = Some(edit);
                self.hold_for_validation(&rule, message, cell);
                return Ok(());
            }
            self.validation_held = None;
            let circular = |doc: &Self| doc.session.doc().ok().and_then(|d| d.wb.sheet(sheet)).is_some_and(|sh| sh.circular.contains(&cell));
            let was_circular = circular(self);
            let params = json!({ "sheet": sheet, "cell": cell.a1(), "input": text, "runs": edit.runs_param(), "expand": !std::mem::take(&mut self.edit_inserted), "widen": true });
            let entered = self.session.execute("cell.set", params).map(|_| ()).map_err(engine_error);
            // A formula that makes a new circular reference is warned about once (WPS, Excel), unless
            // iterative calculation is on; the cells show 0 meanwhile.
            if entered.is_ok() && !was_circular && circular(self) && self.session.doc().is_ok_and(|d| !d.wb.calc.iterative) {
                self.dialog_asked = (self.dialog_asked.0 + 1, Some(("circularReference".into(), json!({ "cell": cell.a1() }), "cell.set".into())));
                self.edits += 1;
            }
            self.go_home()?;
            entered?;
        }
        self.edit_home = None;
        Ok(())
    }

    /// The active cell's style, its font toggles as the selected characters of the cell being
    /// typed have them.
    fn typing_style(&self) -> kition_sheet_engine::model::Style {
        let Some((wb, sheet)) = self.sheet() else { return kition_sheet_engine::model::Style::default() };
        let mut style = wb.styles.get(sheet.style_id(self.active())).clone();
        if let Some(edit) = &self.editing {
            let base = style.font.clone();
            // The face, size and colour the selected characters share (no face: they differ).
            let fonts = edit.selected_fonts(&base);
            if let Some(first) = fonts.first() {
                style.font.name = if fonts.iter().all(|f| f.name == first.name) { first.name.clone() } else { String::new() };
                style.font.size = first.size;
                style.font.color = first.color;
            }
            style.font.bold = edit.every(&base, |f| f.bold);
            style.font.italic = edit.every(&base, |f| f.italic);
            style.font.strike = edit.every(&base, |f| f.strike);
            if !edit.every(&base, |f| f.underline != Underline::None) {
                style.font.underline = Underline::None;
            } else if style.font.underline == Underline::None {
                style.font.underline = Underline::Single;
            }
        }
        style
    }

    /// Starts editing `cell` in place (F2, a double-click): its formula or text, with its runs.
    fn edit_in_place(&mut self, cell: CellRef) {
        if self.refuse_checkbox(cell) {
            return;
        }
        self.edit_inserted = false;
        self.end_copy();
        let text = self.content(cell);
        let runs = self.sheet().and_then(|(_, sheet)| sheet.cell(cell)).and_then(kition_sheet_engine::model::Cell::runs).cloned();
        self.editing = Some(Edit::new(&text, runs.as_ref(), true));
        self.edits += 1;
    }

    /// The font of the active cell (of the merged area it is in).
    fn active_font(&self) -> Font {
        let Some((wb, sheet)) = self.sheet() else { return Font::default() };
        let active = self.active();
        let anchor = sheet.merges.iter().find(|m| m.contains(active)).map_or(active, |m| m.start);
        wb.styles.get(sheet.style_id(anchor)).font.clone()
    }

    /// Where the cell being typed is drawn at `scale` with the view's top-left at (`x`, `y`): its
    /// box (the cell, wider when the text is), the text's start and baseline, and where each
    /// caret position is from the start.
    fn edit_box(&self, x: f64, y: f64, scale: f64) -> Option<EditBox> {
        let edit = self.editing.as_ref()?;
        // Pointing at another sheet's cells: the cell being typed is on its own sheet.
        if self.editing_elsewhere() {
            return None;
        }
        let (wb, sheet) = self.sheet()?;
        let active = self.active();
        let merged = sheet.merges.iter().find(|m| m.contains(active)).copied().unwrap_or(RangeRef::new(active, active));
        let (dx0, dy0, dx1, dy1) = Self::cell_rect(sheet, merged);
        let style = wb.styles.get(sheet.style_id(merged.start));
        let color = resolve(style.font.color, wb).unwrap_or(self.scheme.palette().editing_text);
        let text = edit.text();
        let rich = if text.starts_with('=') {
            formula::rich_text(&text, &formula::references(&text, &sheet.name, &sheet.name))
        } else {
            edit.rich().unwrap_or_else(|| RichText { runs: vec![TextRun::new(text, RunFont::default())] })
        };
        let runs = Runs { runs: &rich, base: &style.font, color, scale, wb };
        let size = (f64::from(style.font.size) * 96.0 / 72.0 * scale) as f32;
        let base = self.faces.pick(&style.font.name, style.font.bold, style.font.italic).face;
        let (mut ascent, mut descent) = Line { font: &base, size, text: "" }.metrics();
        // A line break (Alt+Enter) starts another line.
        let chars = edit.chars();
        let mut bounds = Vec::new();
        let mut first = 0;
        for (i, c) in chars.iter().enumerate() {
            if *c == '\n' {
                bounds.push((first, i));
                first = i + 1;
            }
        }
        bounds.push((first, chars.len()));
        let mut segments = Vec::new();
        let mut lines = Vec::with_capacity(bounds.len());
        let mut widest: f64 = 0.0;
        for (from, to) in bounds {
            let line = runs.segments(&self.faces, from, to);
            let mut stops = Vec::with_capacity(to - from + 1);
            stops.push(0.0);
            let mut pen = 0.0;
            for segment in &line {
                let mut buffer = [0; 4];
                for c in segment.text.chars() {
                    pen += Line { font: &segment.face, size: segment.size, text: c.encode_utf8(&mut buffer) }.width();
                    stops.push(pen);
                }
            }
            let (_, a, d) = Segment::measure(&line);
            (ascent, descent, widest) = (ascent.max(a), descent.max(d), widest.max(pen));
            let at = segments.len();
            segments.extend(line);
            lines.push(EditLine { first: from, stops, segments: at..segments.len(), baseline: 0.0 });
        }
        let pad = 2.5 * scale;
        let step = ascent + descent + scale;
        let count = lines.len() as f64;
        let (x0, y0, x1, y1) = ((dx0 - x) * scale, (dy0 - y) * scale, (dx1 - x) * scale, (dy1 - y) * scale);
        let rect = Rect { x0, y0, x1: x1.max(x0 + widest + 2.0 * pad + 2.0 * scale), y1: y1.max(y0 + count * step + 2.0 * scale) };
        // The lines sit centred up and down in the box (at its top in a top-aligned cell).
        let block = count * (ascent + descent) + (count - 1.0) * scale;
        let top = match style.align.v {
            VAlign::Top => rect.y0 + scale,
            _ => (rect.y0 + rect.y1) / 2.0 - block / 2.0,
        };
        for (k, line) in lines.iter_mut().enumerate() {
            line.baseline = top + ascent + k as f64 * step;
        }
        Some(EditBox { rect, start: x0 + pad, ascent, descent, segments, lines })
    }

    /// The caret position nearest document point (`x`, `y`) in the cell being typed; `None` off
    /// its box unless `anywhere`.
    fn edit_index(&self, x: f64, y: f64, anywhere: bool) -> Option<usize> {
        let b = self.edit_box(0.0, 0.0, 1.0)?;
        let inside = x >= b.rect.x0 && x < b.rect.x1 && y >= b.rect.y0 && y < b.rect.y1;
        if !anywhere && !inside {
            return None;
        }
        let middle = |line: &EditLine| line.baseline - (b.ascent - b.descent) / 2.0;
        let line = b.lines.iter().min_by(|p, q| (middle(p) - y).abs().total_cmp(&(middle(q) - y).abs()))?;
        line.stops
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, c)| (b.start + **a - x).abs().total_cmp(&(b.start + **c - x).abs()))
            .map(|(i, _)| line.first + i)
    }

    fn step(&mut self, dr: i64, dc: i64, extend: bool, jump: bool) -> Result<(), String> {
        self.session.execute("select.move", json!({ "dr": dr, "dc": dc, "extend": extend, "jump": jump })).map(|_| ()).map_err(engine_error)
    }

    /// The selection, the active cell, the headings of the selected columns and rows, and the
    /// cell being typed.
    fn overlays(&self) -> Vec<ToolOverlay> {
        let mut out = Vec::new();
        let (Some(st), Some((_, sheet))) = (self.session.active(), self.sheet()) else { return out };
        let quad = |(x0, y0, x1, y1): (f64, f64, f64, f64)| [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let (cols, rows) = self.extent();
        for reference in self.formula_references().iter().filter(|r| r.local) {
            let range = reference.range;
            let end = CellRef::new(range.end.row.min(rows - 1), range.end.col.min(cols - 1));
            if end.row < range.start.row || end.col < range.start.col {
                continue;
            }
            let range = sheet.merge_at(range.start).filter(|_| range.is_single()).unwrap_or(RangeRef::new(range.start, end));
            let (x0, y0, x1, y1) = Self::cell_rect(sheet, range);
            let [r, g, b, _] = reference.color;
            out.push(ToolOverlay::Highlight { quad: quad((x0, y0, x1, y1)), color: [r, g, b, 24] });
            out.push(ToolOverlay::Path {
                d: format!("M{x0:.1},{y0:.1} L{x1:.1},{y0:.1} L{x1:.1},{y1:.1} L{x0:.1},{y1:.1} Z"),
                color: reference.color,
                width: 2.0,
                dashed: false,
            });
        }
        // The range pointed at for the formula being typed: a dashed frame, as WPS draws it.
        if let Some(p) = self.point.filter(|p| self.session.doc().is_ok_and(|d| d.wb.active_sheet == p.sheet)) {
            let range = RangeRef::new(p.anchor, p.end);
            let end = CellRef::new(range.end.row.min(rows - 1), range.end.col.min(cols - 1));
            if end.row >= range.start.row && end.col >= range.start.col {
                let (x0, y0, x1, y1) = Self::cell_rect(sheet, RangeRef::new(range.start, end));
                out.push(ToolOverlay::Path {
                    d: format!("M{x0:.1},{y0:.1} L{x1:.1},{y0:.1} L{x1:.1},{y1:.1} L{x0:.1},{y1:.1} Z"),
                    color: ACCENT,
                    width: 2.0,
                    dashed: true,
                });
            }
        }
        self.focus_overlays(sheet, st.selection.current(), (cols, rows), &mut out);
        let several = st.selection.ranges.len() > 1;
        let active = st.selection.active;
        let merged = sheet.merges.iter().find(|m| m.contains(active)).copied().unwrap_or(RangeRef::new(active, active));
        let frame = |(x0, y0, x1, y1): (f64, f64, f64, f64), dashed: bool| ToolOverlay::Path {
            d: format!("M{x0:.1},{y0:.1} L{x1:.1},{y0:.1} L{x1:.1},{y1:.1} L{x0:.1},{y1:.1} Z"),
            color: ACCENT,
            width: 2.0,
            dashed,
        };
        // The person's copy: its marching ants take the place of its area's frame (WPS W-F1).
        let copied = self.copy_frame();
        // An area of one cell (or one merged cell) is the active cell's frame, drawn below.
        let one_cell = |range: &RangeRef| range.is_single() || sheet.merge_at(range.start) == Some(*range);
        let active_rect = Self::cell_rect(sheet, merged);
        for range in &st.selection.ranges {
            let clamped = RangeRef::new(range.start, CellRef::new(range.end.row.min(rows - 1), range.end.col.min(cols - 1)));
            let (x0, y0, x1, y1) = Self::cell_rect(sheet, clamped);
            // An area is tinted but for the active cell, which stays white (WPS W-A3); a one-cell
            // area among several is tinted too, unless it is the active cell (B6).
            if !one_cell(range) && range.contains(active) {
                let (ax0, ay0, ax1, ay1) = active_rect;
                for part in [(x0, y0, x1, ay0), (x0, ay1, x1, y1), (x0, ay0, ax0, ay1), (ax1, ay0, x1, ay1)] {
                    if part.2 > part.0 && part.3 > part.1 {
                        out.push(ToolOverlay::Highlight { quad: quad(part), color: SELECTION });
                    }
                }
            } else if !one_cell(range) || (several && !range.contains(active)) {
                out.push(ToolOverlay::Highlight { quad: quad((x0, y0, x1, y1)), color: SELECTION });
            }
            out.push(ToolOverlay::Highlight { quad: quad((x0, col_gutter(sheet), x1, header_h(sheet))), color: HEADING_SELECTED });
            out.push(ToolOverlay::Highlight { quad: quad((row_gutter(sheet), y0, header_w(sheet), y1)), color: HEADING_SELECTED });
            // Every area has its 2 px frame (WPS W-A1, walkthrough B3).
            if Some(*range) != copied && !(one_cell(range) && range.contains(active)) {
                out.push(frame((x0, y0, x1, y1), false));
            }
        }
        if let Some(range) = copied {
            let end = CellRef::new(range.end.row.min(rows - 1), range.end.col.min(cols - 1));
            if end.row >= range.start.row && end.col >= range.start.col {
                out.push(frame(Self::cell_rect(sheet, RangeRef::new(range.start, end)), true));
            }
        }
        // The cell being typed is drawn with the cells (see `paint`), in its runs' fonts; the
        // frame goes round its box. Inside an area of several cells the active cell has no frame
        // of its own (WPS), nor where the ants go round it.
        let in_area = st.selection.ranges.iter().any(|range| !one_cell(range) && range.contains(active));
        match self.edit_box(0.0, 0.0, 1.0) {
            Some(b) => out.push(frame((b.rect.x0, b.rect.y0, b.rect.x1, b.rect.y1), false)),
            None if !in_area && copied != Some(merged) => out.push(frame(active_rect, false)),
            None => {}
        }
        self.object_overlays(&mut out);
        self.ink_overlays(&mut out);
        self.resize_overlays(&mut out);
        self.fill_overlays(&mut out);
        self.move_overlays(&mut out);
        self.freeze_preview_overlays(sheet, &mut out);
        out
    }

    /// View › 高亮行列 (the person's): the rows and the columns of the selection's current area,
    /// across the grid, tinted in the chosen colour; the area itself shows as selected. An
    /// overlay, so it stays out of the document's pixels and follows each pane of frozen panes.
    fn focus_overlays(&self, sheet: &Sheet, area: RangeRef, (cols, rows): (u32, u32), out: &mut Vec<ToolOverlay>) {
        if !self.session.prefs.focus_cell {
            return;
        }
        let color = focus_rgba(&self.session.prefs.focus_color);
        let quad = |(x0, y0, x1, y1): (f64, f64, f64, f64)| [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let end = CellRef::new(area.end.row.min(rows - 1), area.end.col.min(cols - 1));
        let (x0, y0, x1, y1) = Self::cell_rect(sheet, RangeRef::new(area.start, end));
        let (left, top) = (header_w(sheet), header_h(sheet));
        let (right, bottom) = (Self::col_x(sheet, cols), Self::row_y(sheet, rows));
        for part in [(left, y0, x0, y1), (x1, y0, right, y1), (x0, top, x1, y0), (x0, y1, x1, bottom)] {
            if part.2 > part.0 && part.3 > part.1 {
                out.push(ToolOverlay::Highlight { quad: quad(part), color });
            }
        }
    }

    /// The freeze the Freeze Panes menu points at: the rows and columns it would freeze tinted and
    /// the line it would draw, as WPS previews it.
    fn freeze_preview_overlays(&self, sheet: &Sheet, out: &mut Vec<ToolOverlay>) {
        let Some((rows, cols)) = self.freeze_preview else { return };
        let quad = |(x0, y0, x1, y1): (f64, f64, f64, f64)| [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let (extent_cols, extent_rows) = self.extent();
        let (left, top) = (header_w(sheet), header_h(sheet));
        let (right, bottom) = (Self::col_x(sheet, extent_cols.max(cols)), Self::row_y(sheet, extent_rows.max(rows)));
        let (fx, fy) = (Self::col_x(sheet, cols), Self::row_y(sheet, rows));
        if rows > 0 {
            out.push(ToolOverlay::Highlight { quad: quad((left, top, right, fy)), color: FREEZE_PREVIEW_TINT });
            out.push(ToolOverlay::Path { d: format!("M{left:.1},{fy:.1} L{right:.1},{fy:.1}"), color: FREEZE_PREVIEW, width: 2.0, dashed: false });
        }
        if cols > 0 {
            let from = if rows > 0 { fy } else { top };
            out.push(ToolOverlay::Highlight { quad: quad((left, from, fx, bottom)), color: FREEZE_PREVIEW_TINT });
            out.push(ToolOverlay::Path { d: format!("M{fx:.1},{top:.1} L{fx:.1},{bottom:.1}"), color: FREEZE_PREVIEW, width: 2.0, dashed: false });
        }
    }

    fn respond(&self) -> ToolResponse {
        // The bar above the grid shows the active cell and what it holds.
        let active = self.active();
        let prompt = match (&self.editing, self.move_status()) {
            (_, Some(target)) => target,
            (Some(edit), None) => format!("{}: {}", active.a1(), edit.text()),
            (None, None) => format!("{}: {}", active.a1(), self.content(active)),
        };
        ToolResponse {
            requests: vec![ToolRequest::Status { text: prompt }],
            overlays: self.overlays(),
            cursor: self
                .resize_cursor()
                .or_else(|| ((self.fill_hover && self.fill_source().is_some()) || matches!(self.drag, Drag::Fill(_))).then_some("crosshair"))
                .or_else(|| self.move_cursor())
                .or_else(|| (self.text_hover && self.editing.is_some()).then_some("text"))
                .unwrap_or(if self.session.format_painter.is_some() { PAINTER_CURSOR } else { "cell" })
                .into(),
            // A spreadsheet always takes typing (it enters the active cell) and its keys.
            busy: true,
            wants_text: true,
        }
    }
}

/// The deepest outline level of `lines` (rows or columns), at most Excel's 7.
fn outline_levels(lines: &std::collections::BTreeMap<u32, kition_sheet_engine::model::LineInfo>) -> u8 {
    lines.values().map(|l| l.outline).max().unwrap_or(0).min(7)
}

/// The outline gutter left of the row headings: a slot per row level and one for the level
/// buttons, as Excel's outline bar; none without grouped rows.
fn row_gutter(sheet: &Sheet) -> f64 {
    match outline_levels(&sheet.rows) {
        0 => 0.0,
        levels => OUTLINE_SLOT * f64::from(levels + 1),
    }
}

/// The outline gutter above the column headings (see [`row_gutter`]).
fn col_gutter(sheet: &Sheet) -> f64 {
    match outline_levels(&sheet.cols) {
        0 => 0.0,
        levels => OUTLINE_SLOT * f64::from(levels + 1),
    }
}

/// Where the cells start across: the outline gutter and the row headings.
fn header_w(sheet: &Sheet) -> f64 {
    row_gutter(sheet) + HEADER_W
}

/// Where the cells start down: the outline gutter and the column headings.
fn header_h(sheet: &Sheet) -> f64 {
    col_gutter(sheet) + HEADER_H
}

/// The groups of `lines` at outline `level`: each run of lines at that level or deeper, and
/// whether all its lines are hidden (collapsed).
fn outline_groups(lines: &std::collections::BTreeMap<u32, kition_sheet_engine::model::LineInfo>, level: u8) -> Vec<(u32, u32, bool)> {
    let mut out: Vec<(u32, u32, bool)> = Vec::new();
    for (&i, info) in lines.iter().filter(|(_, l)| l.outline >= level) {
        match out.last_mut() {
            Some((_, end, hidden)) if *end + 1 == i => {
                *end = i;
                *hidden &= info.hidden;
            }
            _ => out.push((i, i, info.hidden)),
        }
    }
    out
}

/// The outline gutter's button under (`x`, `y`): a group's + or - at its summary line (the line
/// after the group, Excel's default "summary below"), or a level button in the corner.
fn outline_hit(sheet: &Sheet, x: f64, y: f64, (cols, rows): (u32, u32)) -> Option<Hit> {
    let (gutter_w, gutter_h) = (row_gutter(sheet), col_gutter(sheet));
    let (head_w, head_h) = (header_w(sheet), header_h(sheet));
    if x < gutter_w && y < head_h {
        // Row level buttons, in the corner beside the column headings.
        let level = (x / OUTLINE_SLOT) as u8 + 1;
        return Some(if y >= gutter_h { Hit::Level { rows: true, level } } else { Hit::Gutter });
    }
    if y < gutter_h && x < head_w {
        let level = (y / OUTLINE_SLOT) as u8 + 1;
        return Some(if x >= gutter_w { Hit::Level { rows: false, level } } else { Hit::Gutter });
    }
    if x < gutter_w {
        let level = (x / OUTLINE_SLOT) as u8 + 1;
        let row = view::row_at(sheet, (y - head_h).max(0.0)).min(rows.saturating_sub(1));
        return Some(
            outline_groups(&sheet.rows, level)
                .into_iter()
                .find(|(_, end, _)| *end + 1 == row)
                .map_or(Hit::Gutter, |(start, end, hidden)| Hit::Group { rows: true, start, end, hidden }),
        );
    }
    if y < gutter_h {
        let level = (y / OUTLINE_SLOT) as u8 + 1;
        let col = view::col_at(sheet, (x - head_w).max(0.0)).min(cols.saturating_sub(1));
        return Some(
            outline_groups(&sheet.cols, level)
                .into_iter()
                .find(|(_, end, _)| *end + 1 == col)
                .map_or(Hit::Gutter, |(start, end, hidden)| Hit::Group { rows: false, start, end, hidden }),
        );
    }
    None
}

#[derive(Clone, Copy)]
enum Hit {
    Cell(CellRef),
    Column(u32),
    Row(u32),
    All,
    /// An outline group's + or - button: rows (else columns) `start..=end`, hidden or not.
    Group {
        rows: bool,
        start: u32,
        end: u32,
        hidden: bool,
    },
    /// An outline level button: show the rows (else columns) of levels under `level`.
    Level {
        rows: bool,
        level: u8,
    },
    /// Elsewhere in the outline gutter.
    Gutter,
}

/// A rich text cell's runs, with what drawing them needs: the cell's font (what a run does not
/// set), its text colour (a number format's or the font's), the scale and the workbook (theme
/// colours).
struct Runs<'a> {
    runs: &'a RichText,
    base: &'a Font,
    color: Rgba,
    scale: f64,
    wb: &'a Workbook,
}

impl Runs<'_> {
    /// The characters `from..to` of the text as segments, one per run they cross.
    fn segments(&self, faces: &Faces, from: usize, to: usize) -> Vec<Segment> {
        let mut out = Vec::new();
        let mut at = 0;
        for run in &self.runs.runs {
            let len = run.text.chars().count();
            let (start, end) = (from.max(at), to.min(at + len));
            if start < end {
                let text: String = run.text.chars().skip(start - at).take(end - start).collect();
                let font = run.font.resolve(self.base);
                let picked = faces.pick(&font.name, font.bold, font.italic);
                let full = (f64::from(font.size) * 96.0 / 72.0 * self.scale) as f32;
                // Superscript and subscript: smaller, raised or lowered.
                let (size, rise) = match font.vert {
                    VertAlign::Superscript => (full * 0.65, -f64::from(full) * 0.35),
                    VertAlign::Subscript => (full * 0.65, f64::from(full) * 0.12),
                    VertAlign::Baseline => (full, 0.0),
                };
                let color = match run.font.color.as_ref().and_then(RunColor::to_color) {
                    Some(c) => resolve(c, self.wb).unwrap_or(self.color),
                    None => self.color,
                };
                out.push(Segment {
                    text,
                    face: picked.face,
                    size,
                    rise,
                    color,
                    bold: picked.bold,
                    heavy: font.bold,
                    italic: picked.italic,
                    underline: font.underline,
                    strike: font.strike,
                });
            }
            at += len;
            if at >= to {
                break;
            }
        }
        out
    }
}

/// One stretch of a line drawn in one font (a rich text run, or part of one).
struct Segment {
    text: String,
    face: ab_glyph::FontArc,
    size: f32,
    /// Baseline offset in pixels (negative: raised).
    rise: f64,
    color: Rgba,
    /// Emboldened, the face having no bold (see `Line::draw_styled`).
    bold: bool,
    /// Bold text: characters a fallback face draws are emboldened (see `Line::draw_weighted`).
    heavy: bool,
    /// Slanted, the face having no italic.
    italic: bool,
    underline: Underline,
    strike: bool,
}

/// Where the cell being typed is drawn (see `SheetDocument::edit_box`).
struct EditBox {
    rect: Rect,
    /// Where the lines start.
    start: f64,
    ascent: f64,
    descent: f64,
    /// Every line's segments, line after line.
    segments: Vec<Segment>,
    /// Its lines (a line break starts another), top to bottom.
    lines: Vec<EditLine>,
}

/// A line of the cell being typed.
struct EditLine {
    /// Its first character in the text.
    first: usize,
    /// Each caret position's distance from the start (one more than it has characters).
    stops: Vec<f64>,
    /// Its segments in `EditBox::segments`.
    segments: std::ops::Range<usize>,
    baseline: f64,
}

impl EditLine {
    /// Caret position `i` (of the whole text) from the start of the line.
    fn x(&self, i: usize) -> f64 {
        self.stops.get(i.saturating_sub(self.first)).or(self.stops.last()).copied().unwrap_or(0.0)
    }
}

impl EditBox {
    /// The line caret position `i` is on.
    fn line_of(&self, i: usize) -> Option<&EditLine> {
        self.lines.iter().rev().find(|line| line.first <= i).or(self.lines.first())
    }
}

impl Segment {
    fn line(&self) -> Line<'_> {
        Line { font: &self.face, size: self.size, text: &self.text }
    }

    /// The segments' total width, and the largest ascent and descent.
    fn measure(segments: &[Segment]) -> (f64, f64, f64) {
        segments.iter().fold((0.0, 0.0, 0.0), |(w, a, d), s| {
            let line = s.line();
            let (ascent, descent) = line.metrics();
            (w + line.width(), f64::max(a, ascent - s.rise), f64::max(d, descent + s.rise))
        })
    }

    /// Draws the segments one after another from `x`, on the baseline `y`, clipped to `clip`,
    /// with their underlines and strikethroughs.
    fn draw_all(segments: &[Segment], canvas: &mut Canvas, x: f64, y: f64, clip: Rect) {
        let mut pen = x;
        for s in segments {
            let line = s.line();
            let width = line.width();
            let baseline = y + s.rise;
            line.draw_weighted(canvas, pen, baseline, clip, s.color, s.bold, s.heavy, s.italic);
            let thick = (f64::from(s.size) / 16.0).round().max(1.0);
            let (x0, x1) = (pen.max(clip.x0), (pen + width).min(clip.x1));
            let mut rule = |top: f64| {
                if x0 < x1 && top >= clip.y0 && top + thick <= clip.y1 {
                    canvas.hline(x0, x1, top, thick, s.color);
                }
            };
            match s.underline {
                Underline::None => {}
                Underline::Single | Underline::SingleAccounting => rule((baseline + thick).round()),
                Underline::Double | Underline::DoubleAccounting => {
                    rule((baseline + thick).round());
                    rule((baseline + 3.0 * thick).round());
                }
            }
            if s.strike {
                rule((baseline - line.metrics().0 * 0.3).round());
            }
            pen += width;
        }
    }
}

/// A line of cell text in a font of `size` points at `scale`: the distance between baselines
/// and the baseline's depth below the line's top, in canvas pixels (WPS's line box,
/// `units::line_pitch_pt`).
fn line_box(size: f32, scale: f64) -> (f64, f64) {
    use kition_sheet_engine::model::units;
    let px = units::PX_PER_PT * scale;
    (units::line_pitch_pt(size) * px, units::first_baseline_pt(size) * px)
}

fn resolve_fill_or_white(style: &kition_sheet_engine::model::Style, wb: &Workbook) -> Rgba {
    match style.fill.pattern {
        PatternType::Solid => resolve(style.fill.fg, wb).unwrap_or(WHITE),
        _ => WHITE,
    }
}

fn format_color(color: kition_sheet_engine::numfmt::FormatColor) -> Rgba {
    use kition_sheet_engine::numfmt::FormatColor as F;
    match color {
        F::Black | F::Indexed(_) => [0, 0, 0, 255],
        F::Blue => [0, 0, 255, 255],
        F::Cyan => [0, 255, 255, 255],
        F::Green => [0, 128, 0, 255],
        F::Magenta => [255, 0, 255, 255],
        F::Red => [255, 0, 0, 255],
        F::White => [255, 255, 255, 255],
        F::Yellow => [255, 255, 0, 255],
    }
}

impl KitDocument for SheetDocument {
    fn batch_settler(&self) -> Option<kition_kit::BatchSettler> {
        let axes: Vec<_> = self
            .session
            .active()
            .and_then(|state| state.kdm())
            .into_iter()
            .flat_map(|kdm| &kdm.model().ids().sheets)
            .flat_map(|sheet| sheet.rows.iter().chain(&sheet.cols))
            .cloned()
            .collect();
        Some(Box::new(move |tx| kdm_model::settle_inputs(tx, &axes).map_err(|error| error.to_string())))
    }

    /// What is wrong with the workbook (KHP A21): see [`check`].
    fn check(&self) -> Vec<kition_kit::diagnostics::Diagnostic> {
        self.session.active().map(|st| check::check(&st.wb, &self.faces)).unwrap_or_default()
    }

    /// Opens an Excel workbook (and the other formats Kition Sheet reads).
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        let (wb, _warnings) = kition_sheet_engine::io::open_bytes(name, bytes).map_err(engine_error)?;
        let title = std::path::Path::new(name).file_stem().map_or_else(|| "Book".to_owned(), |stem| stem.to_string_lossy().into_owned());
        let mut session = Session::new();
        session.add_document(DocState::new(wb, None, title));
        Self::from_session(session)
    }

    /// A new workbook.
    fn create(_name: &str, _width: u32, _height: u32) -> Result<Self, String> {
        let mut session = Session::new();
        session.new_workbook();
        Self::from_session(session)
    }

    fn width(&self) -> u32 {
        let (cols, _) = self.extent();
        self.sheet().map_or(1, |(_, sheet)| Self::col_x(sheet, cols).ceil() as u32)
    }

    fn height(&self) -> u32 {
        let (_, rows) = self.extent();
        self.sheet().map_or(1, |(_, sheet)| Self::row_y(sheet, rows).ceil() as u32)
    }

    /// The headings stay in view, and the frozen rows and columns with them.
    fn pinned(&self) -> (u32, u32) {
        let Some((_, sheet)) = self.sheet() else { return (0, 0) };
        let (rows, cols) = view::split(sheet);
        (Self::col_x(sheet, cols).round() as u32, Self::row_y(sheet, rows).round() as u32)
    }

    /// The workbook's revision, and the changes of the cell being typed (drawn with the cells), of
    /// the scheme, and of what another's step changed in the drawing without an undo step.
    fn revision(&self) -> u64 {
        self.session.active().map_or(0, |st| st.revision).saturating_add(self.edits).saturating_add(self.schemes).saturating_add(self.drawn)
    }

    fn dirty(&self) -> bool {
        self.session.active().is_some_and(DocState::is_dirty)
    }

    /// Drawn at the zoom at 1:1 and above, so text stays sharp; below, the exact reduction of 1:1.
    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        if scale < 1.0 {
            let (x0, y0) = (x.floor() as i32, y.floor() as i32);
            let span_x = ((f64::from(width) / scale).ceil() as i32).max(1);
            let span_y = ((f64::from(height) / scale).ceil() as i32).max(1);
            return self.render_area(x0, y0, x0.saturating_add(span_x), y0.saturating_add(span_y), width, height);
        }
        Ok(self.paint(x, y, scale, width, height))
    }

    fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let w = u32::try_from(x1.saturating_sub(x0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let h = u32::try_from(y1.saturating_sub(y0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let full = self.paint(f64::from(x0), f64::from(y0), 1.0, w, h);
        Ok(raster::render_area(Frame { rgba: &full, width: w, height: h }, 0, 0, w as i32, h as i32, width, height))
    }

    /// Runs `command`. Tool ids in its parameters (`sheet:<id>`, `drawing:<id>`) become the kit's
    /// own names and numbers first. An agent's command that names what it acts on (a sheet, a
    /// range or cell, drawing objects) acts on that sheet and leaves the active sheet, the
    /// selection and the selected objects as they were, also when it fails; only the commands
    /// whose work is moving the editor ([`context::moves_the_editor`]: `sheet.activate`,
    /// `select.set`, `view.*`, …) keep their effect. A person's commands run as they always did.
    /// Another's command that changes what the grid draws moves the revision ([`drawn`]).
    fn execute(&mut self, command: &str, params: Json) -> Result<Json, String> {
        let before = drawn::Drawn::before(self);
        let result = self.execute_step(command, params);
        // The person's copy (Cmd+C, Cmd+X, the menus): the grid marks it (`copying`).
        let person = self.session.undo_scope() == kition_sheet_engine::UndoScope::Person;
        if result.is_ok() && matches!(command, "edit.copy" | "edit.cut") && person {
            self.copy_marked();
        }
        if result.is_ok() && command == "edit.paste" && person {
            self.after_paste();
        }
        if let Some(before) = before
            && !before.same(self)
        {
            self.drawn = self.drawn.saturating_add(1);
        }
        result
    }

    /// A host owns every file (engine contract E22, KHP A29, section 10.6): the session reads a
    /// file a command names only through the host, writes none by path, and reads no system word
    /// list.
    fn hosted(&mut self, hosting: &kition_kit::host::Hosting) {
        if hosting.files_only {
            self.session.set_hosted(true);
        }
    }

    /// The current state of the editor that is not the workbook ([`context::Snapshot`]: the
    /// active sheet by id, the selection there and on the other sheets, the selected drawing
    /// objects, the editing and view modes, the pen), opaque to hosts.
    fn editor_state(&self) -> Json {
        self.editor_snapshot().and_then(|state| serde_json::to_value(state).ok()).unwrap_or(Json::Null)
    }

    /// Puts back what [`KitDocument::editor_state`] answered; no edit. A sheet or object the run
    /// deleted is dropped.
    fn restore_editor_state(&mut self, saved: &Json) -> Result<(), String> {
        if saved.is_null() {
            return Ok(());
        }
        let state: context::Snapshot =
            serde_json::from_value(saved.clone()).map_err(|error| format!("this is not the editor state this kit answered: {error}"))?;
        self.put_back(&state)
    }

    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        let wb = &fresh.doc_state()?.wb;
        if let Some(saved) = self.editor_snapshot() {
            if ids::sheet_index(wb, saved.engine.sheet).is_none() {
                return Err("deleted active sheet".into());
            }
            for id in saved.engine.drawing.iter().chain(&saved.also_selected) {
                if ids::drawing_sheet(wb, *id).is_none() {
                    return Err(format!("deleted selected drawing:{id}"));
                }
            }
            if saved.engine.sheet_selections.iter().any(|(id, _)| ids::sheet_index(wb, *id).is_none()) {
                return Err("deleted sheet with a retained selection".into());
            }
        }
        self.session.reconcile_workbook(wb).map_err(engine_error)?;
        Ok(true)
    }

    fn fork_for_premises(&self) -> Result<Self, String> {
        let state = self.doc_state()?;
        let kdm = state.kdm().ok_or("premise planning needs an observed replica")?;
        let mut session = Session::new();
        session.open_with_replica("Book.xlsx", &[], &kdm.snapshot().map_err(engine_error)?, kdm.blobs().clone()).map_err(engine_error)?;
        let mut copy = Self::from_session(session)?;
        copy.restore_editor_state(&self.editor_state())?;
        copy.session.clipboard = self.session.clipboard.clone();
        copy.session.format_painter = self.session.format_painter.clone();
        copy.session.set_hosted(true);
        Ok(copy)
    }

    /// Sets the sheet a call acts in (`part`) and the cells or drawing objects it acts on there
    /// (`on`), as [`context`] describes; refuses what the workbook lacks and `time`. No edit.
    fn set_context(&mut self, context: &kition_kit::CallContext) -> Result<(), String> {
        self.set_call_context(context)
    }

    fn under(&self, command: &str, params: &Json) -> Result<Option<kition_kit::Under>, String> {
        let under = self.shared_edit_premises(command, params, self.call_premises(command, params)?)?;
        let state = self.doc_state()?;
        let objects = state.kdm().map(|kdm| kdm.model().ids());
        Ok(under.map(|mut under| {
            place::feature_object_premises(&state.wb, objects, &mut under);
            under
        }))
    }

    fn under_step(&self, command: &str, params: &Json, _step: &kition_kit::PremiseStep) -> Result<Option<kition_kit::Under>, String> {
        // The engine describes logical coordinate windows. The SDK's stored-list
        // guard needs historical coverage because empty axes materialize lazily.
        self.under(command, params)?.map(|under| self.logical_axis_premises(under)).transpose()
    }

    fn covered(&self, command: &str, params: &Json, objects: &[kition_kit::CoveredAsk]) -> Result<Option<kition_kit::CoveredAt>, String> {
        self.logical_axis_covered(command, params, objects)
    }

    fn content_at(&self, ask: &kition_kit::ContentAsk, _history: &dyn kition_kit::HistoricalBlobs) -> Result<Option<bool>, String> {
        self.axis_content_at(ask)
    }

    fn derived(&self, ids: &[Json], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
        self.premise_derived(ids, names)
    }

    fn carried(&self, ids: &[Json], changes: &[Json]) -> Result<Vec<kition_kit::Carried>, String> {
        self.computed_carried(ids, changes)
    }

    /// The workbook: its sheets, the active sheet's used range, the selection, the active cell
    /// and what it holds, and the history.
    fn inspect(&self) -> Result<Json, String> {
        let st = self.doc_state()?;
        let sheet = st.wb.active().ok_or("the workbook has no sheets")?;
        let active = st.selection.active;
        let area = st.selection.current();
        let active_rect = {
            let merged = sheet.merges.iter().find(|m| m.contains(active)).copied().unwrap_or(RangeRef::new(active, active));
            let (x0, y0, x1, y1) = Self::cell_rect(sheet, merged);
            [x0, y0, x1, y1]
        };
        let table_info = sheet.table_at(active).map(|t| {
            json!({
                "name": t.name,
                "range": t.range.a1(),
                "style": t.style,
                "header": t.header_row,
                "totals": t.totals_row,
                "bandedRows": t.banded_rows,
                "bandedCols": t.banded_cols,
                "firstCol": t.first_col,
                "lastCol": t.last_col,
                "filter": t.filter_button,
            })
        });
        let dialog = self
            .dialog_asked
            .1
            .as_ref()
            .map(|(name, values, command)| json!({ "seq": self.dialog_asked.0, "name": name, "params": values, "command": command }));
        let freeze_shift = {
            let (r, c) = view::origin(sheet);
            [sheet.col_left(c), sheet.row_top(r)]
        };
        let mut out = json!({
            "title": st.title,
            "sheets": st.wb.sheets.iter().map(|sheet| sheet.name.clone()).collect::<Vec<_>>(),
            // What the sheet tabs show of each sheet, in tab order: hidden sheets have no tab but
            // are listed, a tab colour, and a lock on a protected sheet.
            "tabs": st.wb.sheets.iter().map(|sheet| json!({
                "name": sheet.name,
                "hidden": sheet.visibility != kition_sheet_engine::model::Visibility::Visible,
                "color": sheet.tab_color.and_then(|color| color.hex(&st.wb.theme)),
                "protected": sheet.protection.is_some(),
            })).collect::<Vec<_>>(),
            "sheet": sheet.name,
            "used": sheet.used_range().map(|range| range.a1()),
            "active": active.a1(),
            "selection": st.selection.a1(),
            // The status bar's sum, average and count, for a selection of more than one cell.
            "stats": (st.selection.ranges.iter().fold(0u64, |cells, range| cells.saturating_add(range.count())) > 1)
                .then(|| kition_sheet_engine::display::stats(sheet, &st.selection.ranges)),
            // The current area's size and whether it is whole rows or columns: the insert and delete
            // menus' counts and choices.
            "lines": json!({ "rows": area.height(), "cols": area.width(), "wholeRows": area.is_full_rows(), "wholeCols": area.is_full_cols() }),
            "content": self.content(active),
            "editing": self.editing.is_some(),
            "editText": self.editing.as_ref().map(Edit::text),
            // The caret and the other end of the selection in it (UTF-16), and whether a reference
            // was just pointed at (the formula bar takes the keyboard back to go on typing).
            "editCaret": self.edit_offsets().map(|(caret, _)| caret),
            "editAnchor": self.edit_offsets().map(|(_, anchor)| anchor),
            "pointing": self.point.is_some(),
            // The person's View › Show settings: the formula bar, and the sheet's gridlines and headings.
            "formulaBar": self.session.prefs.formula_bar,
            "gridlines": sheet.show_gridlines,
            "gridColor": sheet.grid_color.map(|c| format!("#{c:06X}")),
            "headings": sheet.show_headings,
            // View › 高亮行列 (the person's): on or off, and its colour.
            "focusCell": self.session.prefs.focus_cell,
            "focusColor": self.session.prefs.focus_color,
            // The frozen rows and columns, and the freeze the Freeze Panes menu previews.
            "freeze": sheet.freeze.unwrap_or((0, 0)),
            // How far the grid is shifted by frozen panes that start at a scrolled view's top-left
            // (document points, x and y): the rows above and columns left of it are out of view.
            "freezeShift": freeze_shift,
            "freezePreview": self.freeze_preview,
            // The active cell's rectangle in document pixels (the editor keeps it in view).
            "activeRect": active_rect,
            // The sheet's zoom (View › Zoom), and how many times it was set: the view follows each time.
            "zoom": sheet.zoom,
            "zoomAsked": self.zoom_asked,
            // Where the formula being typed goes, when another sheet is shown to point at its cells.
            "editCell": self.edit_home.map(|(sheet, cell)| json!({ "sheet": st.wb.sheet(sheet).map(|s| s.name.clone()), "cell": cell.a1() })),
            // The function list and argument hint for the cell being typed, and the document point
            // under the cell editor where they show.
            "formulaHint": self.cell_hint().map(|(hint, at)| {
                let mut out = pointing::hint_json(&hint, self.editing.as_ref().map(Edit::chars).unwrap_or_default(), self.hint_pick);
                out["at"] = json!(at);
                out
            }),
            "formulaText": self.formula_text(),
            "formulaReferences": self.formula_references(),
            // The table the active cell is in, for the editor's Table Design tab, and its options.
            "table": sheet.table_at(active).map(|table| table.name.clone()),
            // The selected drawing object, for the editor's Shape Format and Chart Design tabs.
            // A shape, or a group (whose key is in its shapes' `groups`).
            "shape": self.session.selected_shape.filter(|id| sheet.shapes.iter().any(|s| s.id == *id || s.groups.first() == Some(id))).map(|id| id.to_string()),
            // Every selected drawing object: the selected one, then those added with Shift.
            "shapes": self.session.selected_shape.into_iter().chain(self.also_selected.iter().copied()).map(|id| id.to_string()).collect::<Vec<_>>(),
            "chart": self.session.selected_shape.filter(|id| sheet.charts.iter().any(|c| c.id == *id)).map(|id| id.to_string()),
            "shapeEditing": self.shape_edit.is_some(),
            // Whether Escape has something to cancel: typing in a cell or a shape, a gesture, a
            // shape chosen to draw, or a selected object. The editor gives Escape to the select
            // tool only then, so an Escape with nothing to cancel is left to the host (in Focus it
            // leaves Focus).
            "escape": self.escape_cancels(),
            // The format painter: up once (one paint), sticky (until put down), or down (null).
            "formatPainter": self.session.format_painter.as_ref().map(|p| if p.3 { "sticky" } else { "once" }),
            // The active cell's formatting, for the editor's ribbon toggles and pickers (while it is
            // typed, the selected characters' bold, italic, underline and strikethrough).
            "style": serde_json::to_value(self.typing_style()).unwrap_or(Json::Null),
            // The person's (engine rule E14a).
            "canUndo": st.undo.iter().any(|u| u.scope == kition_sheet_engine::UndoScope::Person),
            "canRedo": st.redo.iter().any(|u| u.scope == kition_sheet_engine::UndoScope::Person),
        });
        // The active table's options (the Table Design tab), and the last dialog a command asked
        // for: how many were asked (the editor opens each once), its name and values, the command.
        out["tableInfo"] = table_info.unwrap_or(Json::Null);
        // The Draw tab's pen or eraser, when one is up (a drag draws instead of selecting): the
        // editor does not scroll at the grid's edge for it.
        // The person's copy the grid marks with marching ants (`copying`): Escape ends it, Enter
        // pastes it once.
        out["copied"] = self.copy_live().map_or(Json::Null, |copied| {
            let name = self.session.doc().ok().and_then(|d| d.wb.sheet(copied.sheet).map(|s| s.name.clone()));
            json!({ "sheet": name, "range": copied.range.a1(), "cut": copied.cut })
        });
        out["drawTool"] = json!(Some(self.session.draw_tool.as_str()).filter(|t| !t.is_empty() && *t != "select"));
        out["dialog"] = dialog.unwrap_or(Json::Null);
        // The last notice for the person (the editor shows each once), and whether the active
        // cell shows a checkbox (not typed in: the formula bar stays shut, Insert Link greys).
        // The active cell's data validation: its list's entries for the dropdown arrow, its input
        // message, and the rule for the Data Validation dialog.
        out["validation"] = self.validation_info();
        // The sheet's default column width and row height (document pixels): the grid's pitch
        // where no line has its own size.
        out["defaultSize"] = json!([sheet.default_col_width, sheet.default_row_height]);
        out["notice"] = self.notice.1.as_ref().map_or(Json::Null, |text| json!({ "seq": self.notice.0, "text": text }));
        out["checkbox"] = json!(self.shows_checkbox(active));
        // Every sheet with its id, name and position, and its drawing objects by id: the
        // workbook's, whatever sheet is active (the editor's state above is left out for agents).
        out["workbook"] = ids::workbook_json(&st.wb);
        Ok(out)
    }

    fn commands(&self) -> Vec<KitCommand> {
        let mut commands: Vec<kition_kit::KitCommand> = {
            let mut commands: Vec<KitCommand> = self
                .session
                .commands()
                .into_iter()
                .map(|info| {
                    let spec = kition_sheet_engine::find_command(info.id);
                    KitCommand {
                        id: info.id.to_owned(),
                        title: info.label.trim_end_matches('…').to_owned(),
                        menu: (!info.menu.is_empty()).then(|| info.menu.iter().map(|part| (*part).to_owned()).collect()),
                        shortcut: info.shortcut.map(str::to_owned),
                        params: Some(info.params.to_owned()),
                        mutates: changes_document(info.id, spec.is_some_and(|spec| spec.undoable)),
                        enabled: info.enabled,
                        ..Default::default()
                    }
                    .with_kit_schema()
                })
                .collect();
            commands.push(
                KitCommand {
                    id: "file.render".into(),
                    title: "Render Sheet Range".into(),
                    params: Some("{target?: {sheet?: string, range?: string}, scale?: number, maxSize?: number}".into()),
                    mutates: false,
                    enabled: true,
                    ..Default::default()
                }
                .with_kit_schema(),
            );
            commands.push(
                KitCommand {
                    id: "command.runLine".into(),
                    title: "Enter in Active Cell".into(),
                    params: Some("{text: string}".into()),
                    mutates: true,
                    enabled: true,
                    ..Default::default()
                }
                .with_kit_schema(),
            );
            for (id, title, params) in [
                ("editor.viewport", "Set Visible Grid Extent", "{right: number, bottom: number, x?: number, y?: number}"),
                ("editor.wording", "Set the Editor's Wording", "{columnPrefix?: string}"),
                ("editor.scheme", "Draw the Grid in the Host's Color Scheme", "{scheme: string}"),
                ("editor.formula", "Type in the Formula Bar", "{text?: string, start?: number, end?: number, enter?: string, peek?: boolean}"),
                ("editor.cycleReference", "Cycle Reference Type (F4)", "{text?: string, start?: number, end?: number}"),
                ("editor.acceptFunction", "Insert Function From List", "{name?: string}"),
                ("editor.contextMenu", "Right-Click the Grid", "{x: number, y: number}"),
                ("editor.insertText", "Insert Text at the Caret", "{text: string, append?: boolean, typing?: boolean}"),
                ("editor.freezePreview", "Preview a Freeze", "{rows?: number, cols?: number}"),
                ("editor.validation", "Answer a Data Validation Message", "{answer: string}"),
                ("editor.commentAnchor", "Comment Anchor of Selection", "{}"),
                ("editor.commentPlaces", "Find Comment Anchors", "{anchors: array}"),
            ] {
                commands.push(
                    KitCommand { id: id.into(), title: title.into(), params: Some(params.into()), enabled: true, ..Default::default() }
                        .with_kit_schema(),
                );
            }
            commands
        };
        for command in &mut commands {
            if let Some(schema) = command.schema.as_mut() {
                params::complete(&command.id, schema);
            }
        }
        kition_kit::files::declare(&mut commands, files::FILES);
        describe::describe(&mut commands);
        // What each command reads besides its parameters, and the parameter `on` fills.
        for command in &mut commands {
            command.reads = acts::reads_any(&command.id);
            command.targets = acts::targets(&command.id, command.schema.as_ref());
        }
        references::declare(&mut commands);
        commands
    }

    fn disabled_commands(&self) -> Vec<String> {
        kition_sheet_engine::command_specs().iter().filter(|spec| (spec.enabled)(&self.session).is_err()).map(|spec| spec.id.to_owned()).collect()
    }

    /// The workbook as Excel `.xlsx`, or in the format `name`'s extension asks for.
    fn save(&self, name: Option<&str>) -> Result<Vec<u8>, String> {
        let state = self.doc_state()?;
        let mut wb = (*state.wb).clone();
        if let Some(kdm) = state.kdm() {
            wb.identities.names = kdm.model().ids().names.iter().map(ToString::to_string).collect();
            for (sheet, ids) in wb.sheets.iter_mut().zip(&kdm.model().ids().sheets) {
                let sheet = std::sync::Arc::make_mut(sheet);
                let rows: std::collections::HashMap<_, _> = ids.rows.iter().enumerate().map(|(index, id)| (id.as_str(), index as u32)).collect();
                let cols: std::collections::HashMap<_, _> = ids.cols.iter().enumerate().map(|(index, id)| (id.as_str(), index as u32)).collect();
                sheet.identities = kition_sheet_engine::model::SheetIdentities {
                    object: ids.object.to_string(),
                    axes: [ids.rows.iter().map(ToString::to_string).collect(), ids.cols.iter().map(ToString::to_string).collect()],
                    tombstones: Default::default(),
                    cells: Default::default(),
                    tables: sheet.tables.iter().map(|table| table.id).collect(),
                    pivots: sheet.pivots.iter().map(|pivot| pivot.id).collect(),
                    drawings: sheet
                        .shapes
                        .iter()
                        .map(|shape| shape.id)
                        .zip(&ids.shapes)
                        .chain(sheet.charts.iter().map(|chart| chart.id).zip(&ids.charts))
                        .chain(sheet.images.iter().map(|image| image.id).zip(&ids.images))
                        .map(|(id, object)| (id, object.to_string()))
                        .collect(),
                };
                for (axis, field) in ["rows", "cols"].iter().enumerate() {
                    for (index, object) in kdm.replica().list(&ids.object, field).map_err(|error| error.to_string())?.iter().enumerate() {
                        if kdm.replica().get(object, "gone").ok() == Some(json!(true))
                            && let Some(gone) = sheet.identities.tombstones.get_mut(axis)
                        {
                            gone.insert(index, object.to_string());
                        }
                    }
                }
                for (key, value) in kdm.replica().map_entries(&ids.object, "cells").map_err(|e| e.to_string())? {
                    let Some(object) = value.as_str() else { continue };
                    let Some(at) =
                        key.match_indices(':').find_map(|(i, _)| Some(CellRef::new(*rows.get(key.get(..i)?)?, *cols.get(key.get(i + 1..)?)?)))
                    else {
                        continue;
                    };
                    let id = kition_kit::kdm::ObjectId::from_string(object);
                    let field = |name| kdm.replica().get(&id, name).ok().and_then(|value| value.as_str().map(str::to_owned));
                    sheet
                        .identities
                        .cells
                        .insert(at.a1(), kition_sheet_engine::model::CellIdentity { object: object.to_owned(), by: field("by"), key: field("key") });
                }
            }
        }
        kition_sheet_engine::io::save_bytes(&wb, name.unwrap_or("Book.xlsx")).map_err(engine_error)
    }

    fn mark_saved(&mut self) {
        if let Ok(st) = self.session.doc_mut() {
            st.saved = st.wb.clone();
        }
    }
}

impl KitTools for SheetDocument {
    fn tools(&self) -> Vec<ToolInfo> {
        vec![ToolInfo { id: "select".into(), label: "Select".into(), shortcut: None, icon: "tool-selection".into(), group: 0 }]
    }

    fn tool(&self) -> String {
        "select".into()
    }

    fn select_tool(&mut self, id: &str, _zoom: f64) -> Result<ToolResponse, String> {
        if id != "select" {
            return Err(format!("no tool {id}"));
        }
        Ok(self.respond())
    }

    fn pointer(&mut self, event: &ToolPointer) -> Result<ToolResponse, String> {
        let hit = self.hit(event.x, event.y);
        // In the cell being typed: a press places the caret (Shift extends), a drag selects, a
        // double-click selects a word.
        let typing_at = self.edit_index(event.x, event.y, false);
        self.text_hover = typing_at.is_some() || matches!(self.drag, Drag::Text);
        // A gesture under way takes the moves and the release; a press without a release before
        // it drops the gesture and begins anew.
        if !matches!(self.drag, Drag::None) {
            match event.action {
                PointerAction::Drag | PointerAction::Move => {
                    self.drag_to(event, hit)?;
                    return Ok(self.respond());
                }
                PointerAction::Up => {
                    self.release(event)?;
                    return Ok(self.respond());
                }
                PointerAction::Down | PointerAction::DoubleClick => self.drag = Drag::None,
            }
        }
        // A press goes to the first that takes it, in this order: the cell being typed, the lines
        // between headings, drawing objects (over the cells), the fill handle, the selection's
        // border, a filter button, then the cells and headings. Hovering shows the cursor of the one under it.
        let in_cell_text = typing_at.is_some() && matches!(event.action, PointerAction::Down | PointerAction::DoubleClick);
        if !in_cell_text && self.resize_press(event)? {
            return Ok(self.respond());
        }
        if !in_cell_text && self.object_press(event)? {
            return Ok(self.respond());
        }
        // The fill handle comes before the border round it (WPS, Excel).
        if !in_cell_text && self.fill_press(event)? {
            return Ok(self.respond());
        }
        // The selection's border moves it (WPS, Excel); a filter button opens its list.
        if !in_cell_text && self.move_press(event)? {
            return Ok(self.respond());
        }
        if !in_cell_text && event.action == PointerAction::Down && !event.mods.shift && self.filter_button_press(event.x, event.y) {
            return Ok(self.respond());
        }
        match event.action {
            PointerAction::Down if typing_at.is_some() => {
                if let (Some(edit), Some(at)) = (self.editing.as_mut(), typing_at) {
                    edit.in_place = true;
                    edit.move_to(at, event.mods.shift);
                    // A click into the text ends pointing.
                    self.point = None;
                    self.drag = Drag::Text;
                    self.edits += 1;
                }
            }
            // Typing a formula where a reference can go: the cell (or heading) pressed is pointed at.
            PointerAction::Down if self.point_press(hit, event.mods.shift) => {}
            PointerAction::Down => self.select_press(event, hit)?,
            PointerAction::DoubleClick => {
                if let (Some(edit), Some(at)) = (self.editing.as_mut(), typing_at) {
                    edit.select_word(at);
                    self.edits += 1;
                } else if let Hit::Cell(cell) = hit {
                    self.select(RangeRef::new(cell, cell), cell)?;
                    // A checkbox cell is not typed in (WPS): a double-click leaves it.
                    if !self.shows_checkbox(cell) {
                        self.edit_in_place(cell);
                    }
                }
            }
            _ => {}
        }
        Ok(self.respond())
    }

    fn key(&mut self, key: &str, mods: ToolMods, _zoom: f64) -> Result<ToolResponse, String> {
        self.border_press = None;
        // Escape drops a gesture under way (the editor also sends it for a pointer taken away).
        if key == "Escape" && self.escape_drag() {
            return Ok(self.respond());
        }
        if self.editing.is_none() && self.object_key(key, mods)? {
            return Ok(self.respond());
        }
        let jump = mods.command || mods.control;
        let arrow = match key {
            "ArrowUp" => Some((-1, 0)),
            "ArrowDown" => Some((1, 0)),
            "ArrowLeft" => Some((0, -1)),
            "ArrowRight" => Some((0, 1)),
            _ => None,
        };
        if self.editing.is_some() {
            // The function list takes Up, Down and Tab while it shows.
            if matches!(key, "ArrowDown" | "ArrowUp" | "Tab") && !mods.shift && self.point.is_none() && self.hint_key(key) {
                return Ok(self.respond());
            }
            if key == "F4" {
                self.cycle_reference();
                return Ok(self.respond());
            }
            if let Some((dr, dc)) = arrow
                && self.point_step(dr, dc, mods.shift)
            {
                return Ok(self.respond());
            }
            if key == "Escape" {
                self.editing = None;
                self.point = None;
                self.hint_pick = 0;
                self.edits += 1;
                self.go_home()?;
                return Ok(self.respond());
            }
        }
        if let Some(edit) = self.editing.as_mut() {
            self.edits += 1;
            // Any other key ends pointing: the reference stays as it is in the text.
            self.point = None;
            match key {
                // Alt+Enter (Option+Return): a line break in the cell.
                "Enter" if mods.alt => edit.insert("\n"),
                // Cmd+Enter (Ctrl+Enter elsewhere) fills the selection with it (WPS W-C10).
                "Enter" if (mods.command || mods.control) && !mods.shift => self.fill_selection()?,
                "Enter" => self.enter_and_step(validation::Step::Enter(mods.shift))?,
                "Tab" => self.enter_and_step(validation::Step::Tab(mods.shift))?,
                "Escape" => self.editing = None,
                "Backspace" => edit.delete(false),
                "Delete" => edit.delete(true),
                // Edit mode: arrows, Home and End move the caret (Shift selects).
                "ArrowLeft" | "ArrowRight" if edit.in_place => edit.step(key == "ArrowRight", mods.shift),
                "Home" if edit.in_place => edit.move_to(0, mods.shift),
                "End" if edit.in_place => {
                    let end = edit.len();
                    edit.move_to(end, mods.shift);
                }
                _ => {
                    if let Some((dr, dc)) = arrow {
                        // Arrows enter the cell and move, as in Excel's Enter mode.
                        self.enter_and_step(validation::Step::By(dr, dc))?;
                    }
                }
            }
            return Ok(self.respond());
        }
        if key == "Escape" && self.session.format_painter.is_some() {
            self.session.format_painter = None;
            self.edits += 1;
            return Ok(self.respond());
        }
        // Then the copy: Escape ends it (its marching ants go; the system clipboard keeps its
        // text), and Enter on the target pastes it once and ends it (WPS W-F6).
        if key == "Escape" && self.copy_live().is_some() {
            self.session.execute("edit.clearClipboard", json!({})).map_err(engine_error)?;
            self.end_copy();
            return Ok(self.respond());
        }
        if key == "Enter" && !mods.shift && !mods.alt && !mods.command && !mods.control && self.enter_pastes()? {
            return Ok(self.respond());
        }
        // Alt+↓ (Option+↓) on a cell whose validation offers a list opens it (WPS, Excel).
        if key == "ArrowDown" && mods.alt && !mods.shift && self.validation_info().get("list").is_some_and(|list| list.is_array()) {
            self.dialog_asked =
                (self.dialog_asked.0 + 1, Some(("validationList".into(), json!({ "cell": self.active().a1() }), "editor.validation".into())));
            self.edits += 1;
            return Ok(self.respond());
        }
        if key != "Tab" && key != "Enter" {
            self.tab_run = None;
        }
        match key {
            "Enter" => self.step_after(validation::Step::Enter(mods.shift))?,
            "Tab" => self.step_after(validation::Step::Tab(mods.shift))?,
            "Delete" | "Backspace" => self.session.execute("edit.clearContents", json!({})).map(|_| ()).map_err(engine_error)?,
            // Home goes to the row's first column, Cmd+Home (Ctrl+Home) to the first cell, Cmd+End
            // to the used range's last; Shift extends; End alone does nothing (WPS W-B9–W-B11).
            "Home" | "End" if key == "Home" || mods.command || mods.control => {
                let to = match (key, mods.command || mods.control) {
                    ("Home", false) => "rowStart",
                    ("Home", true) => "sheetStart",
                    _ => "lastCell",
                };
                self.session.execute("select.move", json!({ "to": to, "extend": mods.shift })).map(|_| ()).map_err(engine_error)?;
            }
            "F2" => self.edit_in_place(self.active()),
            "PageDown" => self
                .session
                .execute("select.move", json!({ "dr": 1, "dc": 0, "page": true, "extend": mods.shift }))
                .map(|_| ())
                .map_err(engine_error)?,
            "PageUp" => self
                .session
                .execute("select.move", json!({ "dr": -1, "dc": 0, "page": true, "extend": mods.shift }))
                .map(|_| ())
                .map_err(engine_error)?,
            _ => {
                if let Some((dr, dc)) = arrow {
                    self.step(dr, dc, mods.shift, jump)?;
                }
            }
        }
        Ok(self.respond())
    }

    /// Typing replaces the active cell's content, as in Excel (in edit mode, the selection).
    fn text(&mut self, text: &str, _zoom: f64) -> Result<ToolResponse, String> {
        if self.editing.is_none() && text.chars().all(|c| !c.is_control()) && self.object_text(text) {
            return Ok(self.respond());
        }
        // A checkbox cell: Space ticks or clears the selection's checkboxes; anything else typed
        // is refused (WPS: delete the checkbox first).
        if self.editing.is_none() && self.shows_checkbox(self.active()) {
            if text == " " {
                self.session.execute("cell.toggleCheckbox", json!({})).map_err(engine_error)?;
            } else if text.chars().all(|c| !c.is_control()) {
                self.notify(kition_sheet_engine::cmd::extra::CHECKBOX_LOCKED);
            }
            return Ok(self.respond());
        }
        if text.chars().all(|c| !c.is_control()) {
            if self.editing.is_none() {
                self.edit_inserted = false;
                // Typing in a cell ends the copy (Excel).
                self.end_copy();
            }
            self.editing.get_or_insert_with(|| Edit::new("", None, false)).insert(text);
            self.edits += 1;
            self.point = None;
            self.hint_pick = 0;
        }
        Ok(self.respond())
    }

    fn state(&mut self, _x: f64, _y: f64, _mods: ToolMods, _zoom: f64) -> ToolResponse {
        self.respond()
    }
}

/// A conditional format's look over a cell's style, as upstream `grid.rs` `look` merges it: only
/// the parts the rule sets.
fn apply_conditional(style: &mut kition_sheet_engine::model::Style, look: &kition_sheet_engine::cf::CfLook) {
    use kition_sheet_engine::model::Underline;
    let Some(rule) = &look.style else { return };
    if rule.fill.pattern != PatternType::None {
        style.fill = rule.fill;
    }
    if rule.font.color != Color::Auto {
        style.font.color = rule.font.color;
    }
    style.font.bold |= rule.font.bold;
    style.font.italic |= rule.font.italic;
    style.font.strike |= rule.font.strike;
    if rule.font.underline != Underline::None {
        style.font.underline = rule.font.underline;
    }
    for (target, source) in [
        (&mut style.borders.left, rule.borders.left),
        (&mut style.borders.right, rule.borders.right),
        (&mut style.borders.top, rule.borders.top),
        (&mut style.borders.bottom, rule.borders.bottom),
    ] {
        if !source.is_none() {
            *target = source;
        }
    }
}

/// A data bar: `fraction` of the cell's width, solid or fading to the grid's background (white;
/// the dark grid's in the dark scheme) (upstream `grid.rs`). The bar's colour is the rule's.
fn paint_data_bar(canvas: &mut Canvas, cell: Rect, scale: f64, fraction: f64, color: [u8; 3], gradient: bool, toward: Rgba) {
    let bar = Rect {
        x0: cell.x0 + 2.0 * scale,
        y0: cell.y0 + 2.0 * scale,
        x1: cell.x0 + 2.0 * scale + (cell.x1 - cell.x0 - 4.0 * scale) * fraction.clamp(0.0, 1.0),
        y1: cell.y1 - 2.0 * scale,
    };
    if bar.is_empty() {
        return;
    }
    if !gradient {
        canvas.fill(bar, rgb(color));
        return;
    }
    let (x0, x1) = (bar.x0.round(), bar.x1.round());
    let span = (x1 - x0).max(1.0);
    let mut x = x0;
    while x < x1 {
        let t = (x - x0) / span;
        let shade: [u8; 3] = std::array::from_fn(|i| {
            let (from, to) = (f64::from(color.get(i).copied().unwrap_or(0)), f64::from(toward.get(i).copied().unwrap_or(255)));
            (from + (to - from) * 0.75 * t).round() as u8
        });
        canvas.fill(Rect { x0: x, x1: x + 1.0, ..bar }, rgb(shade));
        x += 1.0;
    }
    let edge = rgb(color);
    canvas.fill(Rect { x1: bar.x0 + scale.max(1.0), ..bar }, edge);
}

/// Where a checkbox cell's box is in its cell (canvas pixels at `scale`): as tall as the cell's
/// text (so a bigger font makes a bigger box), placed as the cell's alignment places text
/// (centred by default).
pub(crate) fn checkbox_rect(cell: Rect, scale: f64, style: &kition_sheet_engine::model::Style) -> Rect {
    let side = (f64::from(style.font.size) * 96.0 / 72.0 * scale * 0.9).clamp(6.0 * scale, (cell.y1 - cell.y0 - 2.0 * scale).max(1.0)).round();
    let pad = 3.0 * scale;
    let x = match style.align.h {
        HAlign::Left => cell.x0 + pad,
        HAlign::Right => cell.x1 - pad - side,
        _ => ((cell.x0 + cell.x1) / 2.0 - side / 2.0).round(),
    };
    let y = match style.align.v {
        VAlign::Top => cell.y0 + pad,
        VAlign::Center => ((cell.y0 + cell.y1) / 2.0 - side / 2.0).round(),
        _ => cell.y1 - pad - side,
    };
    Rect { x0: x, y0: y, x1: x + side, y1: y + side }
}

/// A checkbox cell's box: empty, a grey outline on the grid's background (white, as WPS; the dark
/// grid's in the dark scheme; the cell's fill stays outside it); ticked, filled grey (dark grey on
/// the light grid, light grey on the dark) with a tick in the background's colour. A font colour of
/// the cell's own colours it; bold, italic and the lines through or under text don't touch it.
fn paint_checkbox(
    canvas: &mut Canvas,
    cell: Rect,
    scale: f64,
    style: &kition_sheet_engine::model::Style,
    checked: bool,
    own: Option<Rgba>,
    palette: &scheme::Palette,
) {
    let own = own.filter(|_| style.font.color != Color::Auto);
    let stroke = scale.clamp(1.0, 2.0).round();
    let r = checkbox_rect(cell, scale, style);
    let b = r.intersect(cell);
    if b.is_empty() {
        return;
    }
    let side = r.x1 - r.x0;
    if checked {
        canvas.fill(b, own.unwrap_or(palette.checkbox_ticked));
        let s = side as f32;
        let tick = kition_sheet_chart::Prim::Line {
            pts: vec![[s * 0.22, s * 0.52], [s * 0.42, s * 0.72], [s * 0.78, s * 0.3]],
            color: palette.checkbox_tick,
            width: (s * 0.13).max(1.2),
            dash: false,
        };
        let px = side.ceil() as u32 + 1;
        let image = kition_sheet_chart::rasterize(&[tick], px, px, [255, 255, 255, 0]);
        canvas.image(&image, px as usize, px as usize, r.x0, r.y0, cell);
    } else {
        let color = own.unwrap_or(palette.checkbox_empty);
        canvas.fill(b, palette.checkbox_inside);
        canvas.hline(b.x0, b.x1, b.y0, stroke, color);
        canvas.hline(b.x0, b.x1, b.y1 - stroke, stroke, color);
        canvas.vline(b.x0, b.y0, b.y1, stroke, color);
        canvas.vline(b.x1 - stroke, b.y0, b.y1, stroke, color);
    }
}

/// The mark of a cell with a note (a red triangle) or a threaded comment (a purple one) in its
/// top-right corner, lighter on the dark grid.
fn paint_comment_mark(canvas: &mut Canvas, cell: Rect, scale: f64, threaded: bool, palette: &scheme::Palette) {
    let color: Rgba = if threaded { palette.comment_mark } else { palette.note_mark };
    let side = (6.0 * scale).round();
    for i in 0..side as i64 {
        // Row i of the triangle: from the right edge leftwards, narrowing downwards.
        let y = cell.y0 + 1.0 + i as f64;
        let w = side - i as f64;
        canvas.fill(Rect { x0: cell.x1 - w, y0: y, x1: cell.x1, y1: y + 1.0 }.intersect(cell), color);
    }
}

/// The filter button of a header cell: whether a filter's or a table's header holds it, and
/// whether its column is filtered.
fn filter_button(sheet: &Sheet, cell: CellRef) -> Option<bool> {
    if let Some(af) = &sheet.autofilter
        && af.range.start.row == cell.row
        && (af.range.start.col..=af.range.end.col).contains(&cell.col)
    {
        let offset = cell.col - af.range.start.col;
        return Some(af.criteria.iter().any(|(c, _)| *c == offset));
    }
    sheet
        .tables
        .iter()
        .any(|t| t.header_row && t.filter_button && t.range.start.row == cell.row && (t.range.start.col..=t.range.end.col).contains(&cell.col))
        .then_some(false)
}

/// A filter button: a raised square at the right of the cell with a down arrow, and a funnel
/// beside it when the column is filtered.
fn paint_filter_button(canvas: &mut Canvas, cell: Rect, scale: f64, filtered: bool, palette: &scheme::Palette) {
    let side = (15.0 * scale).min(cell.y1 - cell.y0 - 2.0 * scale).max(4.0).round();
    let x1 = cell.x1 - 2.0 * scale;
    let b = Rect { x0: x1 - side, y0: (cell.y1 - 2.0 * scale - side).max(cell.y0 + scale), x1, y1: cell.y1 - 2.0 * scale };
    let b = b.intersect(cell);
    if b.is_empty() {
        return;
    }
    let (face, edge, mark) = (palette.filter_face, palette.filter_edge, palette.filter_mark);
    let stroke = scale.clamp(1.0, 2.0).round();
    canvas.fill(b, face);
    canvas.hline(b.x0, b.x1, b.y0, stroke, edge);
    canvas.hline(b.x0, b.x1, b.y1 - stroke, stroke, edge);
    canvas.vline(b.x0, b.y0, b.y1, stroke, edge);
    canvas.vline(b.x1 - stroke, b.y0, b.y1, stroke, edge);
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    let (cx, cy) = (b.x0 + w * if filtered { 0.68 } else { 0.5 }, b.y0 + h * 0.52);
    // The down arrow: rows narrowing to a point.
    let half = (w * 0.22).max(1.5).round();
    for i in 0..half as i64 {
        let y = (cy - half / 2.0).round() + i as f64;
        let r = half - i as f64;
        canvas.fill(Rect { x0: cx - r, y0: y, x1: cx + r, y1: y + 1.0 }, mark);
    }
    if filtered {
        // A funnel: a wide top narrowing to a stem.
        let fx = b.x0 + w * 0.3;
        let top = b.y0 + h * 0.28;
        let fw = (w * 0.18).max(1.5).round();
        for i in 0..(fw as i64) {
            let r = fw - i as f64;
            canvas.fill(Rect { x0: fx - r, y0: top + i as f64, x1: fx + r, y1: top + i as f64 + 1.0 }, mark);
        }
        canvas.fill(Rect { x0: fx - stroke / 2.0, y0: top + fw, x1: fx + stroke / 2.0 + 0.5, y1: top + fw + h * 0.3 }, mark);
    }
}

/// A sparkline drawn in its cell, from its source's values now.
fn paint_sparkline(
    canvas: &mut Canvas,
    wb: &Workbook,
    si: usize,
    spark: &kition_sheet_engine::model::Sparkline,
    cell: Rect,
    palette: &scheme::Palette,
) {
    let area = cell.intersect(Rect { x0: 0.0, y0: 0.0, x1: canvas.w as f64, y1: canvas.h as f64 });
    if area.is_empty() {
        return;
    }
    let values: Vec<Option<f64>> = match kition_sheet_engine::calc::evaluate(wb, si, spark.cell, &spark.source) {
        Value::Array(a) => a.iter().take(10_000).map(Value::as_f64).collect(),
        v => vec![v.as_f64()],
    };
    // The file's colour, else the automatic one (lighter on the dark grid).
    let [r, g, b] = spark.color.resolve(&wb.theme).unwrap_or(palette.sparkline);
    let (w, h) = ((cell.x1 - cell.x0) as f32, (cell.y1 - cell.y0) as f32);
    let (x0, y0) = (cell.x0.floor(), cell.y0.floor());
    let (iw, ih) = ((cell.x1.ceil() - x0).max(1.0) as u32, (cell.y1.ceil() - y0).max(1.0) as u32);
    if u64::from(iw) * u64::from(ih) > 16 * 1024 * 1024 {
        return;
    }
    let prims: Vec<kition_sheet_chart::Prim> = kition_sheet_chart::render_sparkline(spark.kind, &values, [r, g, b, 255], spark.markers, w, h)
        .iter()
        .map(|p| p.translated((cell.x0 - x0) as f32, (cell.y0 - y0) as f32))
        .collect();
    let image = kition_sheet_chart::rasterize(&prims, iw, ih, [255, 255, 255, 0]);
    canvas.image(&image, iw as usize, ih as usize, x0, y0, area);
}

/// An icon set's icon at the left of a cell, centred on its height (`icons`).
fn paint_icon(canvas: &mut Canvas, set: &str, index: usize, cell: Rect, scale: f64) {
    let size = 12.0 * scale;
    let side = size.ceil() as u32 + 2;
    let (x, y) = ((cell.x0 + 2.0 * scale).floor(), ((cell.y0 + cell.y1) / 2.0 - size / 2.0).floor());
    let prims = icons::icon(set, index, 1.0, 1.0, size as f32);
    let image = kition_sheet_chart::rasterize(&prims, side, side, [255, 255, 255, 0]);
    canvas.image(&image, side as usize, side as usize, x - 1.0, y - 1.0, cell);
}

/// Text widths for chart layout, in the face for the font `name` (see `Faces::pick`).
struct FaceMeasure<'a> {
    faces: &'a Faces,
    name: &'a str,
}

impl kition_sheet_chart::Measure for FaceMeasure<'_> {
    fn text_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let picked = self.faces.pick(self.name, bold, false);
        let width = Line { font: &picked.face, size, text }.width() as f32;
        // A synthesised bold is drawn a little wider than its face.
        if picked.bold { width * 1.05 } else { width }
    }
}

/// A chart primitive at `s` times its size (chart points to output pixels).
fn scale_prim(prim: &kition_sheet_chart::Prim, s: f32) -> kition_sheet_chart::Prim {
    prim.scaled(s)
}

kition_kit::export_kit_document!(SheetDocument as "SheetDocument");
kition_kit::export_kit_tools!(SheetDocument as "SheetDocument");

/// Adds a font file's face for cell text the built-in faces do not cover, such as Chinese in a
/// web build, which has no system fonts to fall back on (standard E9). Returns the faces added.
pub use describe::NO_OUTPUT;

pub fn add_font(bytes: &[u8]) -> usize {
    // The PDF export embeds (a subset of) it for the characters the standard PDF fonts lack.
    kition_sheet_engine::pdf::add_font(bytes);
    draw::add_fallback(bytes.to_vec())
}

/// Sheet's additions to the exported module, for its embedded editor.
#[cfg(target_arch = "wasm32")]
pub mod web {
    use kition_kit::wasm_bindgen::prelude::*;
    use serde_json::json;

    use crate::__kition_kit_export::WebDocument;

    /// Adds a font file's face for cell text the built-in faces do not cover; returns how many.
    #[wasm_bindgen(js_name = addFont, wasm_bindgen = kition_kit::wasm_bindgen)]
    pub fn add_font(bytes: &[u8]) -> u32 {
        crate::add_font(bytes) as u32
    }

    fn error(message: String) -> JsError {
        JsError::new(&message)
    }

    // Kition phase 3f, Task 2: the collaboration members (engine contract 2). Shapes mirror
    // `kition-sdk`'s `session::web` (`export_kit_session!` does not apply here; see
    // `crate::collab`).
    #[wasm_bindgen(js_class = "SheetDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        pub fn version(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.version().map_err(error)
        }

        #[wasm_bindgen(js_name = exportChanges)]
        pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.export_changes(since).map_err(error)
        }

        #[wasm_bindgen(js_name = importChanges)]
        pub fn import_changes(&mut self, bytes: &[u8]) -> Result<String, JsError> {
            self.0.import_changes(bytes).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        pub fn snapshot(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.snapshot().map_err(error)
        }

        pub fn undo(&mut self) -> Result<String, JsError> {
            self.0.undo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        pub fn redo(&mut self) -> Result<String, JsError> {
            self.0.redo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        #[wasm_bindgen(js_name = canUndo)]
        pub fn can_undo(&self) -> bool {
            self.0.can_undo()
        }

        #[wasm_bindgen(js_name = canRedo)]
        pub fn can_redo(&self) -> bool {
            self.0.can_redo()
        }

        /// `origin` is the KDM's commit form, the JSON array `[kind, principal, label]`.
        #[wasm_bindgen(js_name = executeAs)]
        pub fn execute_as(&mut self, command: &str, params: &str, origin: &str) -> Result<String, JsError> {
            let params = kition_kit::parse_params(params).map_err(error)?;
            let origin =
                kition_sheet_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))?;
            self.0.execute_as(command, params, &origin).map(|value| value.to_string()).map_err(error)
        }

        /// Every blob the replica references or can still bring back, for a peer's `addBlobs`.
        pub fn blobs(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.blobs().map_err(error)
        }

        /// Adds a peer's blobs (from their `blobs()`): kept until the next `importChanges`.
        #[wasm_bindgen(js_name = addBlobs)]
        pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), JsError> {
            self.0.add_blobs(bytes).map_err(error)
        }

        /// The blobs newly needed since `since` (empty bytes: everything the current state
        /// references), encoded like `blobs()`: for live sync, so a change ships only the blobs
        /// it needs.
        #[wasm_bindgen(js_name = blobsSince)]
        pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.blobs_since(since).map_err(error)
        }

        #[wasm_bindgen(js_name = kdmResets)]
        pub fn kdm_resets(&self) -> u32 {
            self.0.kdm_resets()
        }

        /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
        /// capability); a replica that does not open is ignored and the file opens plainly.
        #[wasm_bindgen(js_name = openWithReplica)]
        pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<WebDocument, JsError> {
            crate::SheetDocument::open_with_replica(name, bytes, snapshot, blobs).map(Self).map_err(error)
        }

        /// Whether the workbook has made its replica yet (it makes one on the first shared edit).
        #[wasm_bindgen(getter, js_name = hasReplica)]
        pub fn has_replica(&self) -> bool {
            self.0.has_replica()
        }

        /// The replica's own document id, for the Kit UI protocol's `replica/info` message.
        /// `undefined` before the workbook has made a replica yet.
        #[wasm_bindgen(getter, js_name = docId)]
        pub fn doc_id(&self) -> Option<String> {
            self.0.doc_id()
        }

        /// A draft's changes taken in as one undo step, recorded with `origin` (`[kind, principal,
        /// label]`): `{"damage", "summary"}`. A refusal fails with the KDM's message.
        #[wasm_bindgen(js_name = mergeChanges)]
        pub fn merge_changes(&mut self, changes: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.merge_changes(changes, &origin).map(|answer| answer.to_string()).map_err(error)
        }

        /// What merging a draft's changes would change now, as `StateDiff` JSON; changes nothing.
        #[wasm_bindgen(js_name = compareChanges)]
        pub fn compare_changes(&self, changes: &[u8]) -> Result<String, JsError> {
            self.0.compare_changes(changes).map(|answer| answer.to_string()).map_err(error)
        }

        /// Writes the state a snapshot holds over the workbook as one change of `origin`; answers
        /// the damage JSON.
        #[wasm_bindgen(js_name = restoreState)]
        pub fn restore_state(&mut self, snapshot: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.restore_state(snapshot, &origin).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        /// An agent's reply (JSON text) with its object ids named as the tool interface names them.
        #[wasm_bindgen(js_name = modelReply)]
        pub fn model_reply(&self, value: &str) -> Result<String, JsError> {
            self.0.model_reply(value).map_err(error)
        }

        /// What changed since the version `since` (a receipt): `{"changes": StateDiff}`, or
        /// `{"changes": null, "unavailable"}` when the replica's log does not hold it.
        #[wasm_bindgen(js_name = changesSince)]
        pub fn changes_since(&self, since: &[u8]) -> String {
            self.0.changes_since(since).to_string()
        }

        /// `model/describe` as JSON text.
        #[wasm_bindgen(js_name = modelDescribe)]
        pub fn model_describe(&mut self) -> Result<String, JsError> {
            self.0.model_describe().map(|answer| answer.to_string()).map_err(error)
        }

        /// `model/batch`: the request and the answer as JSON text; a failure's message is the
        /// error's JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelBatch)]
        pub fn model_batch(&mut self, request: &str) -> Result<String, JsError> {
            let request: serde_json::Value = serde_json::from_str(request)
                .map_err(|e| error(json!({ "code": "invalid-op", "index": null, "message": format!("the batch is not JSON: {e}") }).to_string()))?;
            self.0.model_batch(&request).map(|answer| answer.to_string()).map_err(error)
        }
    }

    /// An origin in the KDM's commit form, the JSON array `[kind, principal, label]`.
    fn origin_of(origin: &str) -> Result<kition_sheet_engine::Origin, JsError> {
        kition_sheet_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))
    }
}

/// What is left of `area` without `hole` (inside it): the rows above and below the hole across
/// the area, and the parts left and right of it on the hole's rows.
fn without(area: RangeRef, hole: RangeRef) -> Vec<RangeRef> {
    let Some(hole) = area.intersection(&hole) else { return vec![area] };
    let (a, h) = (area, hole);
    let mut out = Vec::new();
    if h.start.row > a.start.row {
        out.push(RangeRef::new(a.start, CellRef::new(h.start.row - 1, a.end.col)));
    }
    if h.start.col > a.start.col {
        out.push(RangeRef::new(CellRef::new(h.start.row, a.start.col), CellRef::new(h.end.row, h.start.col - 1)));
    }
    if h.end.col < a.end.col {
        out.push(RangeRef::new(CellRef::new(h.start.row, h.end.col + 1), CellRef::new(h.end.row, a.end.col)));
    }
    if h.end.row < a.end.row {
        out.push(RangeRef::new(CellRef::new(h.end.row + 1, a.start.col), a.end));
    }
    out
}

#[cfg(test)]
mod tests;

/// Art text (KitDrawing 1.3 DR26): the runs' bent outlines as filled paths
/// (`shape_text::warped_contours`).
fn warped_text(
    runs: &[(Rgba, kition_office_core::kurbo::BezPath)],
    margins: (f64, f64, f64, f64),
    preset: &str,
    adjust: &[(String, f64)],
    place: kition_office_core::kurbo::Affine,
) -> Vec<kition_sheet_chart::Prim> {
    kition_sheet_engine::shape_text::warped_contours(runs, margins, preset, adjust, place)
        .into_iter()
        .map(|(color, contours)| kition_sheet_chart::Prim::Path { contours, paint: kition_sheet_chart::Paint::Solid(color) })
        .collect()
}
