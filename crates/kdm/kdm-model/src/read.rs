//! Replica -> `Workbook`. First every sheet's lines (rows and columns, deleted ones included, each
//! object read once at its first place in the list), so every token can be resolved; then the
//! content, every position and formula rendered with the current positions (see `codec`). A
//! list entry whose object is gone is skipped; a cell, comment or link keyed by a deleted row or
//! column is dropped (G5). Formula results are not stored: formula cells read with no value, for
//! the engine to recalculate.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use kition_kit::kdm::{KdmError, ObjectId, Replica};
use kition_kit::session::{BlobRef, BlobSource};
use kition_sheet_core::Value as CellValue;
use kition_sheet_core::{CellRef, MAX_COLS, MAX_ROWS};
use kition_sheet_model::{Cell, CellPicture, DefinedName, LineInfo, Sheet, StyleId, StyleTable, Visibility, Workbook};
use serde_json::Value;

use crate::cells::{self, OddTexts};
use crate::codec::{Decode, decode_formula, decode_input, decode_range};
use crate::features::{from_json, read_features, read_hyperlink, read_image, read_line_info, read_sheet_props, read_workbook_props, to_json};
use crate::ids::{Ids, Known, SheetIds};
use crate::lines::{Axis, LineAt, Lines};
use crate::write::style_key;
use crate::{KdmResult, kdm_err, root_id};

/// Reads the whole workbook. The per-person state (active sheet, zoom, active cell, scroll) is
/// the default.
pub fn read_document(replica: &Replica, blobs: &dyn BlobSource) -> KdmResult<(Workbook, Ids)> {
    read_document_known(replica, blobs, &Known::default())
}

/// Reads the whole workbook, giving each sheet `known` remembers its per-person state back.
pub fn read_document_known(replica: &Replica, blobs: &dyn BlobSource, known: &Known) -> KdmResult<(Workbook, Ids)> {
    let (mut wb, ids) = Reader::read(replica, blobs)?;
    known.restore(&mut wb, &ids);
    Ok((wb, ids))
}

/// A sheet's rows or columns as `replica` holds them, deleted ones included.
pub(crate) fn sheet_lines(replica: &Replica, sheet: &ObjectId, axis: Axis) -> KdmResult<Lines> {
    let mut order = Vec::new();
    for object in replica.list(sheet, axis.field())? {
        if !replica.exists(&object) || replica.object_type(&object).ok().as_deref() != Some(axis.ty()) {
            continue;
        }
        let gone = replica.get(&object, "gone").unwrap_or(Value::Null) == Value::Bool(true);
        order.push((object, gone));
    }
    Ok(Lines::new(order))
}

struct ReadSheet {
    object: ObjectId,
    name: String,
    rows: Lines,
    cols: Lines,
}

/// One cell as read: its view, its picture, its `Cell` object and that
/// object's text when it is not what the view's cell would write (`cells::OddTexts`).
struct ReadCell {
    cell: Cell,
    picture: Option<Arc<CellPicture>>,
    object: Option<ObjectId>,
    odd_text: Option<String>,
}

struct Reader<'r> {
    replica: &'r Replica,
    blobs: &'r dyn BlobSource,
    sheets: Vec<ReadSheet>,
    by_object: HashMap<String, usize>,
    styles: HashMap<String, StyleId>,
    pictures: HashMap<String, Arc<CellPicture>>,
}

impl Decode for Reader<'_> {
    fn sheet_of_object(&self, object: &str) -> Option<usize> {
        self.by_object.get(object).copied()
    }

    fn sheet_name(&self, sheet: usize) -> Option<&str> {
        self.sheets.get(sheet).map(|s| s.name.as_str())
    }

    fn line_at(&self, sheet: usize, axis: Axis, token: &str) -> LineAt {
        match self.sheets.get(sheet) {
            Some(s) => match axis {
                Axis::Rows => s.rows.at(token),
                Axis::Cols => s.cols.at(token),
            },
            None => LineAt::Missing,
        }
    }

    fn style_id(&self, key: &str) -> StyleId {
        self.styles.get(key).copied().unwrap_or_default()
    }
}

/// `name` made unique among `used` (lower case) the way a person would: `Name (2)`, `Name (3)`…,
/// within Excel's 31 characters.
pub(crate) fn unique_name(name: &str, used: &HashSet<String>) -> String {
    let base: String = if name.trim().is_empty() { "Sheet".to_owned() } else { name.chars().take(31).collect() };
    if !used.contains(&base.to_ascii_lowercase()) {
        return base;
    }
    for n in 2u32.. {
        let suffix = format!(" ({n})");
        let keep = 31usize.saturating_sub(suffix.chars().count());
        let candidate = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
        if !used.contains(&candidate.to_ascii_lowercase()) {
            return candidate;
        }
    }
    base
}

impl<'r> Reader<'r> {
    fn read(replica: &'r Replica, blobs: &'r dyn BlobSource) -> KdmResult<(Workbook, Ids)> {
        let root = root_id();
        if !replica.exists(&root) {
            return Err(KdmError::NotFound(root.to_string()));
        }
        let mut r = Reader::new(replica, blobs);
        let mut wb = Workbook::new();
        wb.sheets.clear();
        read_workbook_props(&replica.get(&root, "props")?, &mut wb);
        if let Some(theme) = from_json(replica.get(&root, "theme")?) {
            wb.theme = theme;
        }
        if let Some(lists) = from_json(replica.get(&root, "customLists")?) {
            wb.custom_lists = lists;
        }
        if let Some(styles) = from_json(replica.get(&root, "cellStyles")?) {
            wb.cell_styles = styles;
        }
        let (table, keys) = r.read_styles()?;
        wb.styles = table;

        r.read_sheets()?;

        let mut ids = Ids { sheets: Vec::with_capacity(r.sheets.len()), names: Vec::new(), styles: keys };
        for k in 0..r.sheets.len() {
            let (sheet, sid) = r.sheet(k)?;
            wb.sheets.push(Arc::new(sheet));
            ids.sheets.push(sid);
        }
        if wb.sheets.is_empty() {
            // Every sheet was deleted (concurrently): the engine needs one. The repair creates it
            // in the replica; until then the view has one of its own (written as new).
            wb.sheets.push(Arc::new(Sheet::new("Sheet1")));
            ids.sheets.push(SheetIds::new(ObjectId::from_string("absent-sheet")));
        }
        // A sheet without an id, or with one a sheet before it has (made concurrently), gets one.
        wb.number_sheets();
        if !wb.sheets.iter().any(|s| s.visibility == Visibility::Visible)
            && let Some(first) = wb.sheet_mut(0)
        {
            first.visibility = Visibility::Visible;
        }

        // Defined names (one per name and scope: a name defined twice concurrently is read once).
        let mut seen = HashSet::new();
        let mut defined: HashSet<(String, Option<usize>)> = HashSet::new();
        for object in replica.list(&root, "names")? {
            if !replica.exists(&object) || !seen.insert(object.clone()) || replica.object_type(&object).ok().as_deref() != Some("Name") {
                continue;
            }
            let name = replica.get(&object, "name")?.as_str().unwrap_or("").to_owned();
            let stored = replica.get(&object, "formula")?.as_str().unwrap_or("").to_owned();
            let scope = match replica.get(&object, "scope")? {
                Value::String(s) => r.by_object.get(&s).copied(),
                _ => None,
            };
            if !defined.insert((name.to_lowercase(), scope)) {
                continue;
            }
            let props = replica.get(&object, "props")?;
            wb.names.push(DefinedName {
                name,
                scope,
                formula: decode_formula(&r, &stored, None),
                comment: props.get("comment").and_then(Value::as_str).unwrap_or("").to_owned(),
                hidden: props.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            });
            ids.names.push(object);
        }

        wb.apply_name_call_case();
        dedupe_local_ids(&mut wb);
        Ok((wb, ids))
    }

    fn new(replica: &'r Replica, blobs: &'r dyn BlobSource) -> Self {
        Reader { replica, blobs, sheets: Vec::new(), by_object: HashMap::new(), styles: HashMap::new(), pictures: HashMap::new() }
    }

    /// The sheets and their lines, read first: every token resolves against them.
    fn read_sheets(&mut self) -> KdmResult<()> {
        let replica = self.replica;
        let mut seen = HashSet::new();
        let mut used = HashSet::new();
        for object in replica.list(&root_id(), "sheets")? {
            if !replica.exists(&object) || !seen.insert(object.clone()) || replica.object_type(&object).ok().as_deref() != Some("Sheet") {
                continue;
            }
            let name = unique_name(replica.get(&object, "name")?.as_str().unwrap_or(""), &used);
            used.insert(name.to_ascii_lowercase());
            let rows = self.lines(&object, Axis::Rows)?;
            let cols = self.lines(&object, Axis::Cols)?;
            self.by_object.insert(object.as_str().to_owned(), self.sheets.len());
            self.sheets.push(ReadSheet { object, name, rows, cols });
        }
        Ok(())
    }

    /// The sheets and their lines as the view holds them (`wb`, `ids`): every sheet the replica
    /// lists, in order, with the view's name; a line the view does not hold live is a deleted one,
    /// or for a sheet of `changed`, one of `new` (lines made since) at the end of its list. Answers
    /// each sheet's new rows and columns; `None` when the replica's sheets or lines are not the
    /// view's.
    #[allow(clippy::type_complexity)]
    fn view_sheets(
        &mut self,
        wb: &Workbook,
        ids: &Ids,
        changed: &HashSet<usize>,
        new: &HashSet<ObjectId>,
    ) -> KdmResult<Option<Vec<[Vec<ObjectId>; 2]>>> {
        let replica = self.replica;
        let mut seen = HashSet::new();
        let listed: Vec<ObjectId> = replica
            .list(&root_id(), "sheets")?
            .into_iter()
            .filter(|o| replica.exists(o) && seen.insert(o.clone()) && replica.object_type(o).ok().as_deref() == Some("Sheet"))
            .collect();
        if listed.len() != ids.sheets.len() || listed.len() != wb.sheets.len() {
            return Ok(None);
        }
        let none = HashSet::new();
        let mut appended = Vec::with_capacity(listed.len());
        for (k, (object, (sid, sheet))) in listed.into_iter().zip(ids.sheets.iter().zip(&wb.sheets)).enumerate() {
            if object != sid.object {
                return Ok(None);
            }
            let new = if changed.contains(&k) { new } else { &none };
            let (Some((rows, new_rows)), Some((cols, new_cols))) =
                (self.view_lines(&object, Axis::Rows, &sid.rows, new)?, self.view_lines(&object, Axis::Cols, &sid.cols, new)?)
            else {
                return Ok(None);
            };
            self.by_object.insert(object.as_str().to_owned(), k);
            self.sheets.push(ReadSheet { object, name: sheet.name.clone(), rows, cols });
            appended.push([new_rows, new_cols]);
        }
        Ok(Some(appended))
    }

    /// A sheet's lines given the view's live ones (`live`) and the lines made since (`new`): only
    /// the others are read, each of which must be a deleted line, and new lines may only follow
    /// the view's. Answers the lines and the new ones at their end; `None` when the list does not
    /// hold the view's lines so.
    fn view_lines(&self, sheet: &ObjectId, axis: Axis, live: &[ObjectId], new: &HashSet<ObjectId>) -> KdmResult<Option<(Lines, Vec<ObjectId>)>> {
        let known: HashSet<&ObjectId> = live.iter().collect();
        let mut order = Vec::new();
        let mut added = Vec::new();
        for object in self.replica.list(sheet, axis.field())? {
            if known.contains(&object) {
                if !added.is_empty() {
                    return Ok(None);
                }
                order.push((object, false));
                continue;
            }
            if !self.replica.exists(&object) || self.replica.object_type(&object).ok().as_deref() != Some(axis.ty()) {
                continue;
            }
            let gone = self.replica.get(&object, "gone").unwrap_or(Value::Null) == Value::Bool(true);
            match (gone, new.contains(&object)) {
                (true, false) if added.is_empty() => order.push((object, true)),
                (false, true) => {
                    if !added.contains(&object) {
                        added.push(object.clone());
                    }
                    order.push((object, false));
                }
                _ => return Ok(None),
            }
        }
        let lines = Lines::new(order);
        let expected = live.len() + added.len();
        let holds = lines.live.len() == expected && lines.live.get(..live.len()) == Some(live) && lines.live.get(live.len()..) == Some(&added[..]);
        Ok(holds.then_some((lines, added)))
    }

    /// The style table, in the order the styles were written (`order`, then key); the default
    /// style is always 0. Answers each style's key.
    fn read_styles(&mut self) -> KdmResult<(StyleTable, Vec<String>)> {
        let root = root_id();
        let mut entries: Vec<(f64, String, kition_sheet_model::Style)> = self
            .replica
            .map_entries(&root, "styles")?
            .into_iter()
            .filter_map(|(key, v)| {
                let style = from_json(v.get("style")?.clone())?;
                Some((v.get("order").and_then(Value::as_f64).unwrap_or(f64::MAX), key, style))
            })
            .collect();
        entries.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut table = StyleTable::default();
        let mut keys = vec![style_key(&to_json(&kition_sheet_model::Style::default())?)];
        self.styles.insert(keys.first().cloned().unwrap_or_default(), StyleId::DEFAULT);
        // A file's own Normal style, when the workbook's props name one, is the default.
        let props = self.replica.get(&root, "props")?;
        if let Some(at) = props.get("defaultStyle").and_then(Value::as_str).and_then(|wanted| entries.iter().position(|(_, key, _)| key == wanted)) {
            let (_, key, style) = entries.remove(at);
            table.set_default(style);
            self.styles.insert(key.clone(), StyleId::DEFAULT);
            if let Some(first) = keys.first_mut() {
                *first = key;
            }
        }
        for (_, key, style) in entries {
            let id = table.intern(style);
            if id.0 as usize == keys.len() {
                keys.push(key.clone());
            }
            self.styles.insert(key, id);
        }
        Ok((table, keys))
    }

    /// A sheet's rows or columns, deleted ones included.
    fn lines(&self, sheet: &ObjectId, axis: Axis) -> KdmResult<Lines> {
        sheet_lines(self.replica, sheet, axis)
    }

    /// The position of a `cells` (or `comments`, `hyperlinks`) key of sheet `k`: `None` when its
    /// row or column is deleted or unknown.
    fn position(&self, k: usize, key: &str) -> Option<CellRef> {
        let s = self.sheets.get(k)?;
        for (i, _) in key.match_indices(':') {
            let (Some(row), Some(col)) = (key.get(..i), key.get(i + 1..)) else { continue };
            if let (Some(r), Some(c)) = (s.rows.index(row), s.cols.index(col))
                && r < MAX_ROWS
                && c < MAX_COLS
            {
                return Some(CellRef::new(r, c));
            }
        }
        None
    }

    fn bytes(&self, blob: &Value, what: &str) -> KdmResult<Vec<u8>> {
        if blob.is_null() {
            return Ok(Vec::new());
        }
        let blob: BlobRef = serde_json::from_value(blob.clone()).map_err(kdm_err)?;
        let bytes = self.blobs.get(&blob.hash).ok_or_else(|| kdm_err(format!("missing blob {} ({what})", blob.hash)))?;
        Ok(bytes.to_vec())
    }

    fn picture(&mut self, v: &Value) -> KdmResult<Option<Arc<CellPicture>>> {
        let Some(blob) = v.get("blob") else { return Ok(None) };
        let hash = blob.get("hash").and_then(Value::as_str).unwrap_or("").to_owned();
        let mime = v.get("mime").and_then(Value::as_str).unwrap_or("image/png").to_owned();
        let alt = v.get("alt").and_then(Value::as_str).unwrap_or("").to_owned();
        let cache = format!("{hash}\u{0}{mime}\u{0}{alt}");
        if let Some(p) = self.pictures.get(&cache) {
            return Ok(Some(p.clone()));
        }
        let data = self.bytes(blob, "cell picture")?;
        let p = Arc::new(CellPicture { data, mime, alt });
        self.pictures.insert(cache, p.clone());
        Ok(Some(p))
    }

    /// One `cells` entry of sheet `k`: a `Cell` object (see `cells`).
    /// `None`: the entry names no live `Cell`.
    fn cell(&mut self, k: usize, v: &Value) -> KdmResult<Option<ReadCell>> {
        let Some((object, stored)) = cells::read_entry(self.replica, v) else { return Ok(None) };
        let object = Some(object);
        let (value, formula) = match &stored.input {
            Some(input) => decode_input(&*self, input, k),
            None if stored.text.is_empty() => (CellValue::Empty, None),
            None => (CellValue::Text(Arc::from(stored.text.as_str())), None),
        };
        let style = stored.style.as_deref().map(|s| self.style_id(s)).unwrap_or_default();
        let array = stored.array.as_deref().and_then(|t| decode_range(&*self, k, t));
        let formula = formula.map(|mut f| {
            f.array = array;
            Arc::new(f)
        });
        // What the writer takes the field to hold: its view's text (`cells::view_text`).
        let written: &str = match (&formula, &value) {
            (None, CellValue::Text(t)) => t,
            _ => "",
        };
        let odd_text = (object.is_some() && stored.text != written).then(|| stored.text.clone());
        let picture = stored.picture.as_ref().map(|p| self.picture(p)).transpose()?.flatten();
        // A text's runs (marks this version reads), when they spell it.
        let rich = match (&formula, &value) {
            (None, CellValue::Text(t)) if !stored.runs.is_empty() => crate::marks::rich_of(&stored.runs).filter(|r| r.spells(t)).map(Arc::new),
            _ => None,
        };
        Ok(Some(ReadCell { cell: Cell { value, formula, style, rich }, picture, object, odd_text }))
    }

    fn sheet(&mut self, k: usize) -> KdmResult<(Sheet, SheetIds)> {
        let replica = self.replica;
        let (object, name) = match self.sheets.get(k) {
            Some(s) => (s.object.clone(), s.name.clone()),
            None => return Err(kdm_err("no such sheet")),
        };
        let mut sheet = Sheet::new(name);
        // The sheet's id (its tool id `sheet:<id>`); a replica without one is numbered below.
        sheet.sheet_id = replica.get(&object, "sheetId")?.as_u64().and_then(|id| u32::try_from(id).ok()).unwrap_or(0);
        read_sheet_props(&*self, k, &replica.get(&object, "props")?, &mut sheet);
        let features = read_features(replica, &*self, k, &object, &mut sheet);

        // Line info, by the lines' places.
        for axis in Axis::BOTH {
            let live: Vec<ObjectId> =
                self.sheets.get(k).map(|s| if axis == Axis::Rows { s.rows.live.clone() } else { s.cols.live.clone() }).unwrap_or_default();
            for (i, line) in live.iter().enumerate() {
                let Ok(i) = u32::try_from(i) else { break };
                if i >= axis.max() {
                    break;
                }
                let v = replica.get(line, "info").unwrap_or(Value::Null);
                if v.is_null() {
                    continue;
                }
                if let Some(info) = read_line_info(&*self, &v) {
                    match axis {
                        Axis::Rows => sheet.rows.insert(i, info),
                        Axis::Cols => sheet.cols.insert(i, info),
                    };
                }
            }
        }

        // Cells.
        let mut texts = OddTexts::new();
        for (key, v) in replica.map_entries(&object, "cells")? {
            let Some(at) = self.position(k, &key) else { continue };
            let Some(read) = self.cell(k, &v)? else { continue };
            if let (Some(id), Some(text)) = (read.object, read.odd_text) {
                texts.insert(id, text);
            }
            if let Some(p) = read.picture {
                sheet.cell_pictures.insert(at, p);
            }
            sheet.cells.set(at, read.cell);
        }
        for (key, v) in replica.map_entries(&object, "comments")? {
            if let (Some(at), Some(c)) = (self.position(k, &key), crate::comments::read(replica, &v)) {
                sheet.comments.insert(at, c);
            }
        }
        for (key, v) in replica.map_entries(&object, "hyperlinks")? {
            if let (Some(at), Some(h)) = (self.position(k, &key), read_hyperlink(&*self, k, v)) {
                sheet.hyperlinks.insert(at, h);
            }
        }

        // Images.
        let mut images = Vec::new();
        let mut seen = HashSet::new();
        for image in replica.list(&object, "images")? {
            if !replica.exists(&image) || !seen.insert(image.clone()) || replica.object_type(&image).ok().as_deref() != Some("Image") {
                continue;
            }
            let data = self.bytes(&replica.get(&image, "blob")?, "image")?;
            sheet.images.push(read_image(&*self, k, &replica.get(&image, "props")?, data));
            images.push(image);
        }

        // Shapes and charts: the `Shape` and `Chart` objects naming the sheet.
        let mut shapes = Vec::new();
        let mut seen = HashSet::new();
        for shape in crate::shapes::of_sheet(replica, &object) {
            if !replica.exists(&shape) || !seen.insert(shape.clone()) || replica.object_type(&shape).ok().as_deref() != Some(crate::shapes::SHAPE) {
                continue;
            }
            let text = replica.text(&shape, "text").unwrap_or_default();
            if let Some(s) = crate::shapes::read(&*self, k, &replica.get(&shape, "frame")?, &crate::shapes::look_of(replica, &shape)?, text) {
                sheet.shapes.push(s);
                shapes.push(shape);
            }
        }
        let mut charts = Vec::new();
        let mut seen = HashSet::new();
        for chart in crate::charts::of_sheet(replica, &object) {
            if !replica.exists(&chart) || !seen.insert(chart.clone()) || replica.object_type(&chart).ok().as_deref() != Some(crate::charts::CHART) {
                continue;
            }
            let members = replica.map_entries(&chart, "parts")?.into_iter().collect();
            if let Some(c) = crate::charts::read(&*self, k, &replica.get(&chart, "frame")?, members) {
                sheet.charts.push(c);
                charts.push(chart);
            }
        }

        let (rows, cols) = match self.sheets.get(k) {
            Some(s) => (s.rows.live.clone(), s.cols.live.clone()),
            None => (Vec::new(), Vec::new()),
        };
        Ok((sheet, SheetIds { object, rows: rows.into(), cols: cols.into(), images, shapes, charts, features, texts }))
    }
}

/// A hash of everything sheet `object` holds but its cells and lines: its name, `props`, filter,
/// which feature objects it lists, comments, hyperlinks, images, shapes and charts (a change
/// inside a feature object, a comment or a drawing changes that object, which the caller tells). Two equal shapes, before and after a change of the
/// sheet's object, mean only its `cells` map and lines changed ([`reread_cells`] checks the lines).
pub(crate) fn sheet_shape(replica: &Replica, object: &ObjectId) -> KdmResult<u64> {
    let mut h = DefaultHasher::new();
    for field in ["name", "props", "autofilter"] {
        replica.get(object, field)?.to_string().hash(&mut h);
    }
    for (field, _) in crate::features::FEATURE_LISTS {
        for id in replica.list(object, field)? {
            id.as_str().hash(&mut h);
        }
        0xfeu8.hash(&mut h);
    }
    for field in ["comments", "hyperlinks"] {
        for (key, v) in replica.map_entries(object, field)? {
            (key, v.to_string()).hash(&mut h);
        }
        0xffu8.hash(&mut h);
    }
    for id in replica.list(object, "images")? {
        id.as_str().hash(&mut h);
    }
    for id in crate::shapes::of_sheet(replica, object) {
        id.as_str().hash(&mut h);
    }
    for id in crate::charts::of_sheet(replica, object) {
        id.as_str().hash(&mut h);
    }
    Ok(h.finish())
}

/// Where each `Cell` object of a sheet's `cells` map is in the view, per sheet (`None`: not known,
/// found by reading the sheet's map when needed). Kept by [`crate::SheetModel`] while the sheet's
/// lines do not change but for new ones at the end, and its map only gains or loses entries with
/// their objects (`delta_places`).
pub(crate) type CellIndex = Vec<Option<HashMap<ObjectId, CellRef>>>;

/// What one sheet's cells need after a change: the cells to (re)write, with their pictures and
/// objects, and the places to clear.
#[derive(Default)]
struct CellPatch {
    set: Vec<(CellRef, ReadCell)>,
    clear: Vec<CellRef>,
    /// Every object the sheet's `cells` map names, when its odd texts need trimming to them.
    named: Option<HashSet<ObjectId>>,
    /// New lines at the end of the rows and the columns, with their line info.
    appended: [Vec<(ObjectId, Option<LineInfo>)>; 2],
}

/// Brings `wb`'s cells up to the replica after a change of only cells (rule P1: a remote edit, an
/// undo or a redo of one cell must not read every cell of a large workbook again): `sheets` are
/// the sheets whose `cells` map or lines changed (and nothing else of theirs: [`sheet_shape`]),
/// `objects` the `Cell` objects and `lines` the `Row` and `Col` objects that changed, each of which
/// must be a new line at the end of a changed sheet's rows or columns (typing below or right of
/// the data). A changed sheet's entries are matched against the view (new places read, places no
/// entry names cleared); a changed object elsewhere is read where `index` places it. Every other
/// cell, and everything else of the workbook, is the view's already. Formula results are left for
/// the caller to recalculate. Answers `false`, having changed nothing, when the sheets or their
/// lines are not the view's but for such new lines (a full read is needed).
pub(crate) fn reread_cells(
    replica: &Replica,
    blobs: &dyn BlobSource,
    wb: &mut Workbook,
    ids: &mut Ids,
    index: &mut CellIndex,
    sheets: &HashSet<usize>,
    objects: &HashSet<ObjectId>,
    lines: &HashSet<ObjectId>,
) -> KdmResult<bool> {
    let mut r = Reader::new(replica, blobs);
    r.read_styles()?;
    let Some(appended) = r.view_sheets(wb, ids, sheets, lines)? else { return Ok(false) };
    if appended.iter().map(|[rows, cols]| rows.len() + cols.len()).sum::<usize>() != lines.len() {
        return Ok(false);
    }
    // A formula that names a line the view did not have reads `#REF!`: with new lines, it may name
    // one of them now (two people made the same line), which only a full read finds.
    let any_ref = || wb.sheets.iter().any(|s| s.cells.iter().any(|(_, c)| c.formula.as_ref().is_some_and(|f| f.text.contains("#REF!"))));
    if !lines.is_empty() && any_ref() {
        return Ok(false);
    }
    index.resize(wb.sheets.len(), None);
    let mut patches: Vec<(usize, CellPatch)> = Vec::new();
    let mut found: HashSet<&ObjectId> = HashSet::new();

    // The changed sheets: by the changed objects where the index tells every one's place
    // ([`delta_places`]), otherwise every entry against the view.
    let mut whole: Vec<usize> = sheets.iter().copied().collect();
    whole.sort_unstable();
    let delta = delta_places(replica, &r, index, &whole, objects)?;
    for &k in &whole {
        let object = r.sheets[k].object.clone();
        let view = &wb.sheets[k];
        let mut patch = CellPatch::default();
        for (axis, new) in Axis::BOTH.into_iter().zip(&appended[k]) {
            for line in new {
                let info = replica.get(line, "info").unwrap_or(Value::Null);
                let info = if info.is_null() { None } else { read_line_info(&r, &info) };
                patch.appended[axis.slot()].push((line.clone(), info));
            }
        }
        // A sheet that changed with no changed object of its own changed its map some other way:
        // read in full.
        if let Some(delta) = delta.as_ref().filter(|d| d.values().any(|(j, _)| *j == k)) {
            let Some(places) = index.get_mut(k).and_then(Option::as_mut) else { return Ok(false) };
            for (id, change) in delta.iter().filter(|(_, (j, _))| *j == k) {
                found.insert(objects.get(id).unwrap_or(id));
                match change.1 {
                    Some(at) => {
                        places.insert(id.clone(), at);
                        match r.cell(k, &Value::String(id.as_str().to_owned()))? {
                            Some(cell) => patch.set.push((at, cell)),
                            None => patch.clear.push(at),
                        }
                    }
                    None => {
                        if let Some(at) = places.remove(id) {
                            patch.clear.push(at);
                        }
                        ids.sheets[k].texts.remove(id);
                    }
                }
            }
            // A cleared place another object now holds is that object's.
            let set: HashSet<CellRef> = patch.set.iter().map(|(at, _)| *at).collect();
            patch.clear.retain(|at| !set.contains(at));
            patches.push((k, patch));
            continue;
        }
        let entries = replica.map_entries(&object, "cells")?;
        let mut places = HashMap::with_capacity(entries.len());
        let mut present: HashSet<CellRef> = HashSet::with_capacity(entries.len());
        for (key, v) in &entries {
            let Some(at) = r.position(k, key) else { continue };
            present.insert(at);
            let named = v.as_str().map(ObjectId::from_string);
            let changed = named.as_ref().and_then(|id| objects.get(id));
            if let Some(id) = changed {
                found.insert(id);
            }
            // An entry naming no object, a changed object, or a place the view has no cell at.
            let read = named.is_none() || changed.is_some() || (!view.cells.has(at) && !view.cell_pictures.contains_key(&at));
            if let Some(id) = named {
                places.insert(id, at);
            }
            if read {
                match r.cell(k, v)? {
                    Some(cell) => patch.set.push((at, cell)),
                    None => patch.clear.push(at),
                }
            }
        }
        patch.clear.extend(view.cells.iter().map(|(at, _)| at).filter(|at| !present.contains(at)));
        patch.clear.extend(view.cell_pictures.keys().filter(|at| !present.contains(*at)).copied());
        if !ids.sheets[k].texts.is_empty() {
            patch.named = Some(places.keys().cloned().collect());
        }
        index[k] = Some(places);
        patches.push((k, patch));
    }

    // Changed objects of the other sheets, where the index places them.
    let mut rest: Vec<ObjectId> = objects.iter().filter(|o| !found.contains(o)).cloned().collect();
    rest.sort_unstable();
    rest.dedup();
    for k in (0..wb.sheets.len()).filter(|k| !sheets.contains(k)) {
        if rest.is_empty() {
            break;
        }
        if index[k].is_none() {
            // Deleted objects build the index too: a cleared cell's entry goes with its object,
            // so its sheet changed (above), but an entry a merge kept while its object went (an
            // undo deleting a new cell while a peer wrote its entry) still names it, and that
            // cell is no cell any more. Rare: a sheet's map is read for it only then.
            let object = r.sheets[k].object.clone();
            let mut places = HashMap::new();
            for (key, v) in replica.map_entries(&object, "cells")? {
                if let (Some(id), Some(at)) = (v.as_str(), r.position(k, &key)) {
                    places.insert(ObjectId::from_string(id), at);
                }
            }
            index[k] = Some(places);
        }
        let Some(places) = index[k].as_ref() else { continue };
        let mut patch = CellPatch::default();
        let mut left = Vec::new();
        for id in rest {
            match places.get(&id) {
                Some(&at) => match r.cell(k, &Value::String(id.as_str().to_owned()))? {
                    Some(cell) => patch.set.push((at, cell)),
                    None => patch.clear.push(at),
                },
                None => left.push(id),
            }
        }
        rest = left;
        if !patch.set.is_empty() || !patch.clear.is_empty() {
            patches.push((k, patch));
        }
    }
    // An object no sheet names (an entry a merge gave to another object) changes no cell.

    for (k, mut patch) in patches {
        let (Some(sheet), Some(sid)) = (wb.sheet_mut(k), ids.sheets.get_mut(k)) else { continue };
        for axis in Axis::BOTH {
            let [rows, cols] = &mut patch.appended;
            let (new, live, infos) = match axis {
                Axis::Rows => (rows, &mut sid.rows, &mut sheet.rows),
                Axis::Cols => (cols, &mut sid.cols, &mut sheet.cols),
            };
            for (line, info) in new.drain(..) {
                if let (Some(info), Ok(i)) = (info, u32::try_from(live.len()))
                    && i < axis.max()
                {
                    infos.insert(i, info);
                }
                live.push(line);
            }
        }
        for at in patch.clear {
            sheet.cells.remove(at);
            sheet.cell_pictures.remove(&at);
        }
        if let Some(named) = patch.named {
            sid.texts.retain(|id, _| named.contains(id));
        }
        for (at, read) in patch.set {
            if let Some(id) = read.object {
                match read.odd_text {
                    Some(text) => sid.texts.insert(id, text),
                    None => sid.texts.remove(&id),
                };
            }
            match read.picture {
                Some(p) => sheet.cell_pictures.insert(at, p),
                None => sheet.cell_pictures.remove(&at),
            };
            sheet.cells.set(at, read.cell);
        }
    }
    wb.apply_name_call_case();
    Ok(true)
}

/// Where each changed `Cell` object of `objects` is now in the changed sheets `whole`, when that is
/// known without reading their maps: by sheet, the place (`None`: no place any more, a deleted
/// object). Every changed sheet must have an index; an object it places keeps its place (a
/// deleted one loses it), and one it does not is a new object created ahead of a person's write,
/// whose `key` the sheet's map names it under. `None` when any changed object is neither (a new
/// object without `key`, made by a full write or an older build): the maps are read.
///
/// A changed sheet's map changes this way only (rule P1: one cell typed by a peer must not read a
/// large sheet's every entry): writers add an entry with a new object, never remove one but with
/// its object (a cell is emptied, `cells::clear`), and a repair that removes entries changes
/// lines.
fn delta_places(
    replica: &Replica,
    r: &Reader<'_>,
    index: &CellIndex,
    whole: &[usize],
    objects: &HashSet<ObjectId>,
) -> KdmResult<Option<HashMap<ObjectId, (usize, Option<CellRef>)>>> {
    if whole.is_empty() || whole.iter().any(|&k| !matches!(index.get(k), Some(Some(_)))) {
        return Ok(None);
    }
    let mut out = HashMap::new();
    let mut ids: Vec<&ObjectId> = objects.iter().collect();
    ids.sort_unstable();
    for id in ids {
        let known = whole.iter().find_map(|&k| index.get(k)?.as_ref()?.get(id).map(|at| (k, *at)));
        let live = replica.object_type(id).is_ok_and(|ty| ty == cells::CELL);
        if let Some((k, at)) = known {
            out.insert(id.clone(), (k, live.then_some(at)));
            continue;
        }
        if !live {
            // Deleted, never in these sheets' views (or another sheet's: the caller looks there).
            if replica.exists(id) {
                return Ok(None);
            }
            continue;
        }
        // In an unchanged sheet's index: the caller reads it there.
        if index.iter().enumerate().any(|(k, places)| !whole.contains(&k) && places.as_ref().is_some_and(|p| p.contains_key(id))) {
            continue;
        }
        let Some(key) = replica.get(id, "key").ok().and_then(|v| v.as_str().map(str::to_owned)) else { return Ok(None) };
        let mut placed = false;
        for &k in whole {
            let named = replica.map_get(&r.sheets[k].object, "cells", &key)?;
            if named.as_str() == Some(id.as_str()) {
                if let Some(at) = r.position(k, &key) {
                    out.insert(id.clone(), (k, Some(at)));
                }
                placed = true;
                break;
            }
        }
        // Not named in these sheets (another sheet's index is not built): the maps are read.
        if !placed {
            return Ok(None);
        }
    }
    Ok(Some(out))
}

/// Gives repeated local ids (made concurrently: two charts both numbered 5) fresh ones above every
/// id in use, and repeated table names fresh names, in reading order, so every replica reads the
/// same. Charts, images, shapes and tables share one numbering; pivot tables have their own.
fn dedupe_local_ids(wb: &mut Workbook) {
    let objects = |s: &Sheet| -> Vec<u32> {
        s.charts
            .iter()
            .map(|c| c.id)
            .chain(s.images.iter().map(|c| c.id))
            .chain(s.shapes.iter().map(|c| c.id))
            .chain(s.tables.iter().map(|t| t.id))
            .collect()
    };
    let (mut ids, mut pivots, mut tables) = (HashSet::new(), HashSet::new(), HashSet::new());
    let mut repeated = false;
    // Every id is recorded (no short cut at the first repeat), so the fresh ids start past all of
    // them and never take an id a later object already has.
    for s in &wb.sheets {
        for id in objects(s) {
            repeated |= !ids.insert(id);
        }
        for p in &s.pivots {
            repeated |= !pivots.insert(p.id);
        }
        for t in &s.tables {
            repeated |= !tables.insert(t.name.to_ascii_lowercase());
        }
    }
    if !repeated {
        return;
    }
    let mut next = ids.iter().copied().max().unwrap_or(0);
    let mut pivot_next = pivots.iter().copied().max().unwrap_or(0);
    let (mut ids, mut pivots, mut tables) = (HashSet::new(), HashSet::new(), HashSet::<String>::new());
    let fresh = |id: &mut u32, seen: &mut HashSet<u32>, next: &mut u32| {
        if !seen.insert(*id) {
            *next = next.saturating_add(1);
            *id = *next;
            seen.insert(*next);
        }
    };
    for k in 0..wb.sheets.len() {
        let Some(sheet) = wb.sheet_mut(k) else { continue };
        for c in sheet.charts.iter_mut() {
            fresh(&mut c.id, &mut ids, &mut next);
        }
        for c in sheet.images.iter_mut() {
            fresh(&mut c.id, &mut ids, &mut next);
        }
        for c in sheet.shapes.iter_mut() {
            fresh(&mut c.id, &mut ids, &mut next);
        }
        for t in sheet.tables.iter_mut() {
            fresh(&mut t.id, &mut ids, &mut next);
            if tables.contains(&t.name.to_ascii_lowercase()) {
                let base = t.name.clone();
                let mut n = 2u32;
                while tables.contains(&format!("{base}_{n}").to_ascii_lowercase()) {
                    n = n.saturating_add(1);
                }
                t.name = format!("{base}_{n}");
            }
            tables.insert(t.name.to_ascii_lowercase());
        }
        for p in sheet.pivots.iter_mut() {
            fresh(&mut p.id, &mut pivots, &mut pivot_next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_names_are_made_unique() {
        let used: HashSet<String> = ["data".to_owned(), "data (2)".to_owned()].into_iter().collect();
        assert_eq!(unique_name("Data", &used), "Data (3)");
        assert_eq!(unique_name("Other", &used), "Other");
        assert_eq!(unique_name("", &used), "Sheet");
        let long = "x".repeat(40);
        let used: HashSet<String> = [long.chars().take(31).collect::<String>()].into_iter().collect();
        assert_eq!(unique_name(&long, &used).chars().count(), 31);
    }

    #[test]
    fn repeated_local_ids_get_fresh_ones() {
        let mut wb = Workbook::new();
        let chart = |id: u32| kition_sheet_model::Chart {
            id,
            kind: Default::default(),
            anchor: Default::default(),
            title: None,
            series: vec![],
            legend: Default::default(),
            data_labels: false,
            gridlines: false,
            style: 0,
            x_title: None,
            y_title: None,
            source: None,
            by_rows: false,
            x_axis: Default::default(),
            y_axis: Default::default(),
            label_position: None,
            totals: vec![],
            point_colors: vec![],
        };
        if let Some(s) = wb.sheet_mut(0) {
            s.charts = vec![chart(5), chart(5), chart(7)];
        }
        dedupe_local_ids(&mut wb);
        let ids: Vec<u32> = wb.sheets[0].charts.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![5, 8, 7]);
    }
}
