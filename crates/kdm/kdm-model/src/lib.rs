//! Bridges Kition Sheet's [`Workbook`] ([`kition_sheet_model`]) to a `kit.sheet` KDM replica
//! (phase 3f).
//!
//! [`SheetModel`] implements [`KitModel`]: the engine's materialised view of a replica, the same
//! [`Workbook`] the rest of the engine already knows. Sheets, defined names, rows, columns and
//! images become KDM objects with their own identity, and every cell is its own map entry keyed by
//! its row and column objects, so concurrent edits of different cells, a row insert and an edit
//! below it, and edits of different sheets merge; [`write_document`] and [`write_changes`] are the
//! engine command bridge's entry points: writing a whole workbook in one creating transaction, or
//! only what a command changed.
//!
//! What lives where (`model/schema.json`, decision G1):
//! - the workbook's `props` (date system, calculation, document properties, structure
//!   protection), `theme`, `customLists` and `cellStyles` are `json` registers; `styles` maps each
//!   style's content hash to the style, so cells name styles by content and styles made
//!   concurrently are all kept;
//! - a `Name` has its name, its scope (a `ref` to a `Sheet`), its formula in id form and `props`;
//! - a `Sheet` has its name and `props` register, its `rows` and `cols` lists of line objects
//!   (see `lines`: dense from the top, deleted lines kept as tombstones), its `cells`, `comments`
//!   and `hyperlinks` maps keyed `<rowObject>:<colObject>` (each comment a `Comment` object, see
//!   `comments`), its feature objects (`Merge`, `CondFormat`, `Validation`, `Table`, `Sparkline`,
//!   `Pivot`, each in a list of its own, and the `autofilter` register: see `features`), and its
//!   `images`, each an `Image` with `props` and a content-addressed `blob`;
//! - a cell's entry names its `Cell` object (plan K5; see `cells`): its text
//!   value in a `text` field (concurrent typing in one cell merges by character; a rich text
//!   cell's runs are its marks, see `marks`), everything else in an `input` register (a formula in id form, see
//!   `codec`, or a number, boolean or error: formulas are not merged by character, so an id
//!   token is never cut), its style's key, a legacy array formula's range, a picture's blob.
//!   Formula results are never stored: the engine recalculates them (G1).
//!   The plan's `media` list is not needed: image and cell picture bytes are blob fields, which
//!   are content-addressed (one copy per content), so no `Media` type exists.
//!
//! **Formulas (G2)** name sheets, rows and columns by object, so a row insert changes no formula
//! in the replica. **Structural edits (G3)** come from the engine's log ([`StructuralEdit`]):
//! [`write_changes`] applies them as list edits first, so inserting `n` rows writes `n` row
//! objects and the cells in them, not the sheet below. **Per-person state (G4)** (the active
//! sheet, a sheet's zoom, active cell and scroll position) is never written; [`Known`] keeps it
//! across reads. **Repair (G5)**: [`KitModel::validate`] drops what a merge left dangling
//! (`validate`); references to deleted lines read `#REF!` and ranges shrink as they are read.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod anchors;
mod blob_gc;
mod cells;
mod charts;
mod codec;
mod comments;
mod compare;
mod constraints;
mod feature_ids;
mod features;
mod ids;
mod lines;
mod list;
mod marks;
mod read;
mod reread;
mod session;
mod settle;
mod shapes;
mod validate;
mod write;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema, Tx};
use kition_kit::session::{BlobSink, BlobSource, Damage, KitError, KitModel};
use kition_sheet_model::Workbook;

pub use anchors::{comment_anchor, find_comment_anchor};
pub use compare::normalize;
pub use constraints::constraint_fields;
pub use feature_ids::{cell_style_name_key, table_style_uid};
pub use ids::{FeatureIds, Ids, Known, LineIds, SheetIds};
pub use kition_sheet_model::StructuralEdit;
pub use read::{read_document, read_document_known};
pub use session::SheetSession;
pub use settle::settle_inputs;
pub use write::{write_changes, write_document};

/// Private workbook-lifetime holder for normalized logical-axis settings.
pub(crate) fn axis_facts_id() -> ObjectId {
    ObjectId::from_string("axis-facts")
}

/// Re-exported so downstream crates (the engine command bridge) share this crate's exact pin.
pub use kition_kit;

type Result<T> = kition_kit::session::Result<T>;
pub(crate) type KdmResult<T> = std::result::Result<T, KdmError>;

const SCHEMA: &str = include_str!("../model/schema.json");

/// The `Workbook` object's id: one per replica, minted by [`write_document`] with
/// [`Tx::create_with_id`], so every reader finds it without an index.
pub(crate) fn root_id() -> ObjectId {
    ObjectId::from_string("workbook")
}

fn kit_err(error: impl std::fmt::Display) -> KitError {
    KitError::new(error.to_string())
}

pub(crate) fn kdm_err(error: impl std::fmt::Display) -> KdmError {
    KdmError::Engine(error.to_string())
}

fn ext_of(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.contains('.') { lower.rsplit('.').next().unwrap_or("").to_owned() } else { String::new() }
}

/// A CSV opens as a sheet named after the file (like Excel; the engine's `io::open_bytes`).
fn rename_first_sheet(wb: &mut Workbook, name: &str) {
    let stem = std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("Sheet1");
    let clean: String = stem.chars().filter(|c| !matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']')).take(31).collect();
    let clean = clean.trim_matches('\'').to_string();
    if !clean.is_empty()
        && let Some(sh) = wb.sheet_mut(0)
    {
        sh.name = clean;
    }
}

/// A file's workbook, as [`KitModel::import`] reads it (the engine's `io::open_bytes`): an Excel
/// package (`.xlsx`, `.xlsm`, `.xltx`, `.xltm`, or any zip by content), an `.xlsb` or `.ods`
/// (by content or extension), the engine's JSON, or CSV/TSV text.
pub fn decode(name: &str, bytes: &[u8]) -> Result<Workbook> {
    use kition_sheet_xlsx::Format;
    let ext = ext_of(name);
    let sniffed = kition_sheet_xlsx::sniff(bytes);
    match sniffed {
        Format::Encrypted => return Err(kit_err(format!("{name} is password-protected"))),
        Format::LegacyBinary => return Err(kit_err(format!("{name} is an Excel 97-2003 (.xls) or other legacy binary file"))),
        _ => {}
    }
    if sniffed == Format::Ods || (ext == "ods" && sniffed != Format::Xlsx && sniffed != Format::Xlsb) {
        return kition_sheet_xlsx::read_ods(bytes).map(|(wb, _)| wb).map_err(kit_err);
    }
    if sniffed == Format::Xlsb || (ext == "xlsb" && sniffed != Format::Xlsx) {
        return kition_sheet_xlsx::read_xlsb(bytes).map(|(wb, _)| wb).map_err(kit_err);
    }
    if sniffed == Format::Xlsx || matches!(ext.as_str(), "xlsx" | "xlsm" | "xltx" | "xltm") {
        return kition_sheet_xlsx::read_xlsx(bytes).map(|(wb, _)| wb).map_err(kit_err);
    }
    match ext.as_str() {
        "json" | "scjson" => {
            let mut wb: Workbook = serde_json::from_slice(bytes).map_err(kit_err)?;
            wb.styles.rebuild_index();
            if wb.sheets.is_empty() {
                wb = Workbook::new();
            }
            Ok(wb)
        }
        "tsv" | "tab" | "txt" => {
            let opts = kition_sheet_xlsx::CsvOptions { delimiter: b'\t', ..Default::default() };
            let mut wb = kition_sheet_xlsx::read_csv(bytes, &opts).map_err(kit_err)?;
            rename_first_sheet(&mut wb, name);
            Ok(wb)
        }
        _ => {
            let opts = kition_sheet_xlsx::CsvOptions { delimiter: 0, ..Default::default() };
            let mut wb = kition_sheet_xlsx::read_csv(bytes, &opts).map_err(kit_err)?;
            rename_first_sheet(&mut wb, name);
            Ok(wb)
        }
    }
}

/// The workbook as file bytes in `name`'s format: an Excel package (`.xlsx`, `.xlsm`…, and any
/// name with no other format), the engine's JSON, or the active sheet as CSV/TSV.
pub fn encode(name: &str, wb: &Workbook) -> Result<Vec<u8>> {
    match ext_of(name).as_str() {
        "json" | "scjson" => serde_json::to_vec_pretty(wb).map_err(kit_err),
        "csv" => Ok(wb.sheet(wb.active_sheet).map(|sh| kition_sheet_xlsx::write_csv(sh, wb, b',')).unwrap_or_default()),
        "tsv" | "tab" | "txt" => Ok(wb.sheet(wb.active_sheet).map(|sh| kition_sheet_xlsx::write_csv(sh, wb, b'\t')).unwrap_or_default()),
        "xlsb" | "ods" => Err(kit_err(format!("{name}: this format is imported only; save as .xlsx"))),
        _ => kition_sheet_xlsx::write_xlsx(wb).map_err(kit_err),
    }
}

/// The engine's view of a `kit.sheet` replica: the same [`Workbook`] the rest of the engine knows
/// (recalculated: the replica stores no formula results), plus the objects [`write_changes`]
/// needs to find each piece again and each person's own view state ([`Known`]).
pub struct SheetModel {
    pub workbook: Workbook,
    ids: Ids,
    known: Known,
    /// Each sheet's [`read::sheet_shape`] as the view holds it, for telling a change of only its
    /// cells ([`KitModel::apply`] then reads just those). A sheet with none is read in full.
    shapes: HashMap<ObjectId, u64>,
    /// Where each sheet's `Cell` objects are, as far as known ([`read::CellIndex`]).
    cell_index: read::CellIndex,
}

impl SheetModel {
    /// A view built by the caller itself: `workbook` as written by [`write_document`] or
    /// [`write_changes`], with the ids they filled.
    pub fn from_parts(workbook: Workbook, ids: Ids) -> Self {
        let mut known = Known::default();
        known.remember(&workbook, &ids);
        Self { workbook, ids, known, shapes: HashMap::new(), cell_index: Vec::new() }
    }

    /// Replaces the view after the caller wrote `workbook` to the replica itself (a diff bridge's
    /// [`write_changes`]), with the ids that write left.
    pub fn adopt(&mut self, workbook: Workbook, ids: Ids) {
        self.known.remember(&workbook, &ids);
        self.workbook = workbook;
        self.ids = ids;
        // What the write changed is not known here: the next change reads the whole view.
        self.shapes.clear();
        self.cell_index.clear();
    }

    /// [`SheetModel::adopt`] for a write whose changed objects are known (`changed`, the
    /// replica's): the shapes of the sheets it did not change stay.
    pub(crate) fn adopt_written(&mut self, replica: &Replica, workbook: Workbook, ids: Ids, changed: &BTreeSet<ObjectId>) {
        let shapes = std::mem::take(&mut self.shapes);
        let mut index = std::mem::take(&mut self.cell_index);
        let sheets_before: Vec<ObjectId> = self.ids.sheets.iter().map(|s| s.object.clone()).collect();
        self.adopt(workbook, ids);
        self.shapes = shapes;
        self.note_shapes(replica, Some(changed));
        // The cell index stays for the sheets whose map and lines the write left as they were: a
        // write of only existing cells' objects. Anything else (a line, a sheet added) drops it.
        let same_sheets = sheets_before.iter().eq(self.ids.sheets.iter().map(|s| &s.object));
        let only_cells = same_sheets
            && changed.len() <= 256
            && changed.iter().all(|id| self.ids.sheet_of(id).is_some() || replica.object_type(id).is_ok_and(|ty| ty == cells::CELL));
        if only_cells {
            for id in changed {
                if let Some(slot) = self.ids.sheet_of(id).and_then(|k| index.get_mut(k)) {
                    *slot = None;
                }
            }
            self.cell_index = index;
        }
    }

    /// Records the shape of every sheet in `changed` (all of them for `None`) or without one.
    pub(crate) fn note_shapes(&mut self, replica: &Replica, changed: Option<&BTreeSet<ObjectId>>) {
        let mut shapes = HashMap::with_capacity(self.ids.sheets.len());
        for sheet in &self.ids.sheets {
            let known = self.shapes.get(&sheet.object).filter(|_| changed.is_some_and(|c| !c.contains(&sheet.object)));
            if let Some(shape) = known.copied().or_else(|| read::sheet_shape(replica, &sheet.object).ok()) {
                shapes.insert(sheet.object.clone(), shape);
            }
        }
        self.shapes = shapes;
    }

    /// The view after a change of only cells (`changed`: `Cell` objects, deleted objects, sheets
    /// whose shape is unchanged, and new lines at the end of theirs): those cells read again,
    /// recalculated. `false`, the view unchanged, when the change is anything else (a full read
    /// is needed).
    fn apply_cells(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>, blobs: &dyn BlobSource) -> bool {
        let mut sheets = HashSet::new();
        let mut objects = HashSet::new();
        let mut lines = HashSet::new();
        for id in changed {
            if let Some(k) = self.ids.sheet_of(id) {
                sheets.insert(k);
                continue;
            }
            match replica.object_type(id) {
                Ok(ty) if ty == cells::CELL => {
                    objects.insert(id.clone());
                }
                Ok(ty) if ty == "Row" || ty == "Col" => {
                    lines.insert(id.clone());
                }
                // Deleted (a cleared cell's object): its entry's removal changed its sheet. Read
                // as a cell too: an entry may still name it (a merge kept the entry, not the
                // object), and that cell is no cell any more.
                Err(_) if !replica.exists(id) && *id != root_id() => {
                    objects.insert(id.clone());
                }
                _ => return false,
            }
        }
        // A deleted object the view holds other than a cell (a line, a name, an image, a sheet: an
        // undo of the step that made it, which may not change what listed it) needs a full read.
        let deleted: HashSet<&ObjectId> = objects.iter().filter(|id| !replica.exists(id)).collect();
        if !deleted.is_empty()
            && (self.ids.names.iter().any(|name| deleted.contains(name))
                || self.ids.sheets.iter().any(|s| {
                    deleted.contains(&s.object)
                        || s.rows.iter().chain(&s.cols).chain(&s.images).chain(&s.shapes).chain(&s.charts).any(|object| deleted.contains(object))
                }))
        {
            return false;
        }
        let mut shapes = Vec::with_capacity(sheets.len());
        for &k in &sheets {
            let Some(object) = self.ids.sheets.get(k).map(|s| s.object.clone()) else { return false };
            match (self.shapes.get(&object), read::sheet_shape(replica, &object)) {
                (Some(before), Ok(now)) if *before == now => shapes.push((object, now)),
                _ => return false,
            }
        }
        let mut workbook = self.workbook.clone();
        let mut ids = self.ids.clone();
        // Taken: a full reload (when this fails) starts a new one anyway.
        let mut index = std::mem::take(&mut self.cell_index);
        match read::reread_cells(replica, blobs, &mut workbook, &mut ids, &mut index, &sheets, &objects, &lines) {
            Ok(true) => {}
            _ => return false,
        }
        self.cell_index = index;
        recalc(&mut workbook);
        self.workbook = workbook;
        self.ids = ids;
        self.shapes.extend(shapes);
        true
    }

    /// The objects of the view, current as of the last `load`/`apply`/`adopt`. A diff bridge
    /// clones them into its own working copy for a [`write_changes`] transaction, then hands them
    /// back through [`SheetModel::adopt`].
    pub fn ids(&self) -> &Ids {
        &self.ids
    }

    /// The per-person state the view keeps across reloads, for [`read_document_known`].
    pub fn known(&self) -> &Known {
        &self.known
    }

    fn reload(&mut self, replica: &Replica, blobs: &dyn BlobSource) -> Result<()> {
        self.known.remember(&self.workbook, &self.ids);
        let (mut workbook, ids) = read_document_known(replica, blobs, &self.known).map_err(kit_err)?;
        recalc(&mut workbook);
        self.workbook = workbook;
        self.ids = ids;
        self.note_shapes(replica, None);
        self.cell_index.clear();
        Ok(())
    }
}

/// Recalculates every formula (results are not stored in the replica).
fn recalc(wb: &mut Workbook) {
    kition_sheet_calc::Calc::new().recalc_all(wb);
}

impl std::fmt::Debug for SheetModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetModel").field("sheets", &self.workbook.sheets.len()).field("names", &self.workbook.names.len()).finish_non_exhaustive()
    }
}

impl KitModel for SheetModel {
    fn schema() -> Arc<Schema> {
        #[allow(clippy::expect_used)]
        Arc::new(Schema::parse(SCHEMA).expect("model/schema.json is a fixed, tested file"))
    }

    fn load(replica: &Replica, blobs: &dyn BlobSource) -> Result<Self> {
        let (mut workbook, ids) = read_document(replica, blobs).map_err(kit_err)?;
        recalc(&mut workbook);
        let mut model = Self::from_parts(workbook, ids);
        model.note_shapes(replica, None);
        Ok(model)
    }

    /// Reads again only the cells that changed when nothing else did (rule P1); otherwise reloads
    /// the whole view. Either way each person's own state stays (G4).
    fn apply(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>, blobs: &dyn BlobSource) -> Result<Damage> {
        if !self.apply_cells(replica, changed, blobs) {
            self.reload(replica, blobs)?;
        }
        Ok(Damage::Full)
    }

    fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> Result<bool> {
        validate::validate(tx, changed).map_err(kit_err)
    }

    fn import(name: &str, bytes: &[u8], blobs: &mut dyn BlobSink, actor: ActorId) -> Result<Replica> {
        let workbook = decode(name, bytes)?;
        let mut ids = Ids::default();
        let (replica, ()) = Replica::create_with(Self::schema(), DocId::new(), actor, &Origin::system("import"), |tx| {
            write_document(tx, &workbook, &mut ids, blobs)?;
            Ok(())
        })
        .map_err(kit_err)?;
        Ok(replica)
    }

    fn export(&self, _replica: &Replica, name: &str, _blobs: &dyn BlobSource) -> Result<Vec<u8>> {
        encode(name, &self.workbook)
    }
}
