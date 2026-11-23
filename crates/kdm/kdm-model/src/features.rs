//! The `json` registers of the workbook, a sheet, a name and an image: what they hold, with every
//! position and formula in id form (see `codec`), and back.
//!
//! - Workbook `props`: date system, calculation settings, document properties, structure
//!   protection, the file's own colours (`palette`), the `styles` key of the file's own Normal
//!   style (`defaultStyle`: the style unstyled cells take; Kition's default when absent), the
//!   file's workbook window (`view`), the versions that wrote it (`fileVersion`), and which named
//!   cell styles are Excel's built-in ones (`cellStyleInfo`, by name).
//! - Sheet `props`: default sizes, freeze panes (by count), tab colour, visibility, display
//!   switches, protection (the password hash as a string: it does not fit a JSON number), print
//!   settings (area, titles and breaks as line tokens), and the info of rows and columns below the
//!   materialised part of the sheet (`farRows`, `farCols`, by index).
//! - A sheet's feature objects, each in its own list of the sheet so that two people's edits of
//!   different ones (or of different fields of one) both stay: `Merge`, `CondFormat`,
//!   `Validation`, `Table` (with its `TableColumn`s), `Sparkline` and `Pivot`; and its filter, the
//!   `autofilter` register. Every range and anchor is a position token, every formula in id form.
//! - A link's target in the workbook, in id form.
//!
//! Not stored (G4): the sheet's zoom, active cell and scroll position.

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_sheet_core::RangeRef;
use kition_sheet_model::{
    AutoFilter, CfRule, CfValueKind, Chart, CondFormat, Hyperlink, Image, LineInfo, PivotTable, PrintSettings, Sheet, SheetProtection, Sparkline,
    Table, TableColumn, TotalsFn, Validation, Workbook,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value as Json, json};

use crate::codec::{
    Decode, Encode, decode_cell, decode_formula, decode_line, decode_range, decode_text, encode_cell, encode_formula, encode_range, encode_text,
};
use crate::ids::FeatureIds;
use crate::lines::{Axis, Use, span};
use crate::{KdmResult, kdm_err};

pub(crate) fn to_json(v: &impl Serialize) -> KdmResult<Json> {
    serde_json::to_value(v).map_err(kdm_err)
}

pub(crate) fn from_json<T: DeserializeOwned>(v: Json) -> Option<T> {
    serde_json::from_value(v).ok()
}

fn field<T: DeserializeOwned>(v: &Json, key: &str) -> Option<T> {
    v.get(key).cloned().and_then(from_json)
}

fn set(v: &mut Json, key: &str, value: Json) {
    if let Some(map) = v.as_object_mut() {
        map.insert(key.to_owned(), value);
    }
}

// ---------------------------------------------------------------- workbook

/// The workbook's `props`; `default_style` is the `styles` key of a file's own Normal style.
pub(crate) fn workbook_props(wb: &Workbook, default_style: Option<&str>) -> KdmResult<Json> {
    let mut v = json!({
        "dateSystem": to_json(&wb.date_system)?,
        "calc": to_json(&wb.calc)?,
        "docProps": to_json(&wb.props)?,
        "protectedStructure": wb.protected_structure,
    });
    // Additive: a reader that does not know it leaves it out.
    if let Some(palette) = &wb.palette {
        set(&mut v, "palette", to_json(palette)?);
    }
    if !wb.cell_style_info.is_empty() {
        set(&mut v, "cellStyleInfo", to_json(&wb.cell_style_info)?);
    }
    if let Some(view) = &wb.view {
        set(&mut v, "view", to_json(view)?);
    }
    if let Some(version) = &wb.file_version {
        set(&mut v, "fileVersion", to_json(version)?);
    }
    if !wb.table_styles.is_empty() {
        set(&mut v, "tableStyles", to_json(&wb.table_styles)?);
    }
    if !wb.extensions.is_empty() {
        set(&mut v, "extensions", to_json(&wb.extensions)?);
    }
    if let Some(key) = default_style {
        set(&mut v, "defaultStyle", Json::String(key.to_owned()));
    }
    Ok(v)
}

pub(crate) fn read_workbook_props(v: &Json, wb: &mut Workbook) {
    if let Some(x) = field(v, "dateSystem") {
        wb.date_system = x;
    }
    if let Some(x) = field(v, "calc") {
        wb.calc = x;
    }
    if let Some(x) = field(v, "docProps") {
        wb.props = x;
    }
    if let Some(x) = field(v, "protectedStructure") {
        wb.protected_structure = x;
    }
    wb.palette = field(v, "palette");
    wb.cell_style_info = field(v, "cellStyleInfo").unwrap_or_default();
    wb.view = field(v, "view");
    wb.file_version = field(v, "fileVersion");
    wb.table_styles = field(v, "tableStyles").unwrap_or_default();
    wb.extensions = field(v, "extensions").unwrap_or_default();
}

// ---------------------------------------------------------------- lines

/// A row's or column's info, its style as a `styles` key.
pub(crate) fn line_info<E: Encode + ?Sized>(e: &E, info: &LineInfo) -> KdmResult<Json> {
    let mut v = to_json(info)?;
    if info.size.is_none()
        && let Some(fields) = v.as_object_mut()
    {
        fields.remove("size");
    }
    set(&mut v, "style", info.style.and_then(|s| e.style_key(s)).map_or(Json::Null, Json::String));
    Ok(v)
}

pub(crate) fn read_line_info<D: Decode + ?Sized>(d: &D, v: &Json) -> Option<LineInfo> {
    if !v.is_object() {
        return None;
    }
    let style = v.get("style").and_then(Json::as_str).map(|k| d.style_id(k));
    let mut v = v.clone();
    set(&mut v, "style", Json::Null);
    let mut info: LineInfo = from_json(v)?;
    info.style = style;
    Some(info)
}

// ---------------------------------------------------------------- sheet props

fn protection_value(p: &SheetProtection) -> KdmResult<Json> {
    let mut v = to_json(p)?;
    if let Some(hash) = p.password_hash {
        set(&mut v, "password_hash", Json::String(hash.to_string()));
    }
    Ok(v)
}

fn read_protection(v: &Json) -> Option<SheetProtection> {
    if !v.is_object() {
        return None;
    }
    let hash = v.get("password_hash").and_then(Json::as_str).and_then(|s| s.parse::<u64>().ok());
    let mut v = v.clone();
    set(&mut v, "password_hash", Json::Null);
    let mut p: SheetProtection = from_json(v)?;
    p.password_hash = hash;
    Some(p)
}

fn print_value<E: Encode + ?Sized>(e: &mut E, s: usize, p: &PrintSettings) -> KdmResult<Json> {
    let mut v = to_json(p)?;
    let area = match p.print_area {
        Some(r) => Json::String(encode_range(e, s, r, Use::Ref)?),
        None => Json::Null,
    };
    set(&mut v, "print_area", area);
    for (key, titles, axis) in [("title_rows", p.title_rows, Axis::Rows), ("title_cols", p.title_cols, Axis::Cols)] {
        let t = match titles {
            Some((a, b)) => json!([e.line(s, axis, a, Use::Ref)?, e.line(s, axis, b, Use::Ref)?]),
            None => Json::Null,
        };
        set(&mut v, key, t);
    }
    for (key, breaks, axis) in [("row_breaks", &p.row_breaks, Axis::Rows), ("col_breaks", &p.col_breaks, Axis::Cols)] {
        let tokens = breaks.iter().map(|b| e.line(s, axis, *b, Use::Ref)).collect::<KdmResult<Vec<String>>>()?;
        set(&mut v, key, json!(tokens));
    }
    Ok(v)
}

fn read_print<D: Decode + ?Sized>(d: &D, s: usize, v: &Json) -> Option<PrintSettings> {
    if !v.is_object() {
        return None;
    }
    let mut v = v.clone();
    let area = v.get("print_area").and_then(Json::as_str).and_then(|t| decode_range(d, s, t));
    set(&mut v, "print_area", Json::Null);
    let mut titles = [None, None];
    for (k, (key, axis)) in [("title_rows", Axis::Rows), ("title_cols", Axis::Cols)].into_iter().enumerate() {
        let pair = v.get(key).and_then(Json::as_array).and_then(|a| Some((a.first()?.as_str()?.to_owned(), a.get(1)?.as_str()?.to_owned())));
        if let Some((a, b)) = pair
            && let Some(slot) = titles.get_mut(k)
        {
            *slot = span(d.line_at(s, axis, &a), d.line_at(s, axis, &b), axis.max());
        }
        set(&mut v, key, Json::Null);
    }
    let mut breaks: [Vec<u32>; 2] = [Vec::new(), Vec::new()];
    for (k, (key, axis)) in [("row_breaks", Axis::Rows), ("col_breaks", Axis::Cols)].into_iter().enumerate() {
        if let (Some(items), Some(slot)) = (v.get(key).and_then(Json::as_array), breaks.get_mut(k)) {
            *slot = items.iter().filter_map(Json::as_str).filter_map(|t| decode_line(d, s, axis, t)).collect();
        }
        set(&mut v, key, json!([]));
    }
    let mut p: PrintSettings = from_json(v)?;
    p.print_area = area;
    let [rows, cols] = titles;
    p.title_rows = rows;
    p.title_cols = cols;
    let [rb, cb] = breaks;
    p.row_breaks = rb;
    p.col_breaks = cb;
    Some(p)
}

/// A sheet's `props` register. `far` are the infos of the rows and of the columns kept by index.
pub(crate) fn sheet_props<E: Encode + ?Sized>(e: &mut E, s: usize, sh: &Sheet, far: [&[(u32, Json)]; 2]) -> KdmResult<Json> {
    let protection = match &sh.protection {
        Some(p) => protection_value(p)?,
        None => Json::Null,
    };
    let print = print_value(e, s, &sh.print)?;
    let [far_rows, far_cols] = far;
    let mut v = json!({
        "defaultColWidth": sh.default_col_width,
        "defaultRowHeight": sh.default_row_height,
        "freeze": to_json(&sh.freeze)?,
        "tabColor": to_json(&sh.tab_color)?,
        "visibility": to_json(&sh.visibility)?,
        "showGridlines": sh.show_gridlines,
        "showHeadings": sh.show_headings,
        "showFormulas": sh.show_formulas,
        "showZeros": sh.show_zeros,
        "rightToLeft": sh.right_to_left,
        "protection": protection,
        "print": print,
        "farRows": far_rows.iter().map(|(i, v)| json!([i, v])).collect::<Vec<_>>(),
        "farCols": far_cols.iter().map(|(i, v)| json!([i, v])).collect::<Vec<_>>(),
    });
    // Additive: a reader that does not know it leaves it out.
    if let Some(levels) = sh.outline_levels {
        set(&mut v, "outlineLevels", json!([levels.0, levels.1]));
    }
    if let Some(color) = sh.grid_color {
        set(&mut v, "gridColor", json!(color));
    }
    Ok(v)
}

/// Reads a sheet's `props` into `sh` (a fresh sheet: what is missing keeps its default).
pub(crate) fn read_sheet_props<D: Decode + ?Sized>(d: &D, s: usize, v: &Json, sh: &mut Sheet) {
    if !v.is_object() {
        return;
    }
    if let Some(x) = field(v, "defaultColWidth") {
        sh.default_col_width = x;
    }
    if let Some(x) = field(v, "defaultRowHeight") {
        sh.default_row_height = x;
    }
    if let Some(x) = field(v, "freeze") {
        sh.freeze = x;
    }
    if let Some(x) = field(v, "tabColor") {
        sh.tab_color = x;
    }
    if let Some(x) = field(v, "visibility") {
        sh.visibility = x;
    }
    for (key, slot) in [
        ("showGridlines", &mut sh.show_gridlines),
        ("showHeadings", &mut sh.show_headings),
        ("showFormulas", &mut sh.show_formulas),
        ("showZeros", &mut sh.show_zeros),
        ("rightToLeft", &mut sh.right_to_left),
    ] {
        if let Some(x) = v.get(key).and_then(Json::as_bool) {
            *slot = x;
        }
    }
    sh.outline_levels = field(v, "outlineLevels");
    sh.grid_color = v.get("gridColor").and_then(Json::as_u64).and_then(|c| u32::try_from(c).ok()).map(|c| c & 0xFF_FFFF);
    sh.protection = v.get("protection").and_then(read_protection);
    if let Some(p) = v.get("print").and_then(|p| read_print(d, s, p)) {
        sh.print = p;
    }
    for (key, axis) in [("farRows", Axis::Rows), ("farCols", Axis::Cols)] {
        let Some(items) = v.get(key).and_then(Json::as_array) else { continue };
        for item in items {
            let Some(i) = item.get(0).and_then(Json::as_u64).and_then(|i| u32::try_from(i).ok()).filter(|i| *i < axis.max()) else { continue };
            let Some(info) = item.get(1).and_then(|v| read_line_info(d, v)) else { continue };
            match axis {
                Axis::Rows => sh.rows.insert(i, info),
                Axis::Cols => sh.cols.insert(i, info),
            };
        }
    }
}

// ---------------------------------------------------------------- features

/// Applies `f` to every formula string of a conditional format's rule.
fn rule_strings(rule: &mut CfRule, f: &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()> {
    fn kind(k: &mut CfValueKind, f: &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()> {
        if let CfValueKind::Formula(s) = k {
            f(s)?;
        }
        Ok(())
    }
    match rule {
        CfRule::CellIs { a, b, .. } => {
            f(a)?;
            if let Some(b) = b {
                f(b)?;
            }
        }
        CfRule::Expression { formula, .. } => f(formula)?,
        CfRule::ColorScale { stops } => {
            for (k, _) in stops.iter_mut() {
                kind(k, f)?;
            }
        }
        CfRule::DataBar { min, max, .. } => {
            kind(min, f)?;
            kind(max, f)?;
        }
        CfRule::IconSet { thresholds, .. } => {
            for k in thresholds.iter_mut() {
                kind(k, f)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validation_strings(v: &mut Validation, f: &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()> {
    f(&mut v.f1)?;
    if let Some(f2) = v.f2.as_mut() {
        f(f2)?;
    }
    Ok(())
}

fn chart_strings(c: &mut Chart, f: &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()> {
    for s in c.series.iter_mut() {
        f(&mut s.values)?;
        for slot in [s.name.as_mut(), s.categories.as_mut(), s.bubble_sizes.as_mut()].into_iter().flatten() {
            f(slot)?;
        }
    }
    if let Some(source) = c.source.as_mut() {
        f(source)?;
    }
    Ok(())
}

/// A chart with its ranges as position tokens, as stored.
pub(crate) fn chart_encoded<E: Encode + ?Sized>(e: &mut E, s: usize, c: &Chart) -> KdmResult<Chart> {
    encoded(e, s, c, chart_strings)
}

/// A stored chart with its ranges read back.
pub(crate) fn chart_decoded<D: Decode + ?Sized>(d: &D, s: usize, c: Chart) -> Chart {
    decoded(d, s, c, chart_strings)
}

/// Encodes every string `strings` visits.
fn encoded<E: Encode + ?Sized, T: Clone>(
    e: &mut E,
    s: usize,
    item: &T,
    strings: fn(&mut T, &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()>,
) -> KdmResult<T> {
    let mut item = item.clone();
    strings(&mut item, &mut |t: &mut String| {
        *t = encode_text(e, t, Some(s))?;
        Ok(())
    })?;
    Ok(item)
}

/// Decodes every string `strings` visits.
fn decoded<D: Decode + ?Sized, T>(
    d: &D,
    s: usize,
    mut item: T,
    strings: fn(&mut T, &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()>,
) -> T {
    let _ = strings(&mut item, &mut |t: &mut String| {
        *t = decode_text(d, t, Some(s));
        Ok(())
    });
    item
}

fn ranges<E: Encode + ?Sized>(e: &mut E, s: usize, ranges: &[RangeRef], purpose: Use) -> KdmResult<Json> {
    Ok(Json::Array(ranges.iter().map(|r| encode_range(e, s, *r, purpose).map(Json::String)).collect::<KdmResult<Vec<_>>>()?))
}

/// An object anchor with row and column references as separate JSON values. SDK reread pairing
/// can then translate each id without interpreting text or an embedded position-token grammar.
pub(crate) fn anchor_value<E: Encode + ?Sized>(e: &mut E, s: usize, a: &kition_sheet_model::Anchor) -> KdmResult<Json> {
    let mut v = to_json(a)?;
    set(&mut v, "cell", json!([e.line(s, Axis::Rows, a.cell.row, Use::Ref)?, e.line(s, Axis::Cols, a.cell.col, Use::Ref)?]));
    Ok(v)
}

/// An object anchor back; a cell that is gone (a row deleted concurrently) is the sheet's corner.
pub(crate) fn read_anchor<D: Decode + ?Sized>(d: &D, s: usize, v: &Json) -> Option<kition_sheet_model::Anchor> {
    let cell = v
        .get("cell")
        .and_then(Json::as_array)
        .and_then(|parts| {
            Some(kition_sheet_core::CellRef::new(
                decode_line(d, s, Axis::Rows, parts.first()?.as_str()?)?,
                decode_line(d, s, Axis::Cols, parts.get(1)?.as_str()?)?,
            ))
        })
        .or_else(|| v.get("cell").and_then(Json::as_str).and_then(|token| decode_cell(d, s, token)))
        .unwrap_or_default();
    let mut v = v.clone();
    set(&mut v, "cell", to_json(&cell).ok()?);
    from_json(v)
}

// ---------------------------------------------------------------- feature objects

/// What a reader needs of the store a sheet's feature objects are in: a replica, or a
/// transaction's view of one.
pub(crate) trait Store {
    /// A register field's value (`null` when the object or the field has none).
    fn field(&self, id: &ObjectId, field: &str) -> Json;
    /// A list field's items, each live object once, in order.
    fn items(&self, id: &ObjectId, field: &str) -> Vec<ObjectId>;
    /// Whether `id` is a live object of type `ty`.
    fn is(&self, id: &ObjectId, ty: &str) -> bool;
}

impl Store for Replica {
    fn field(&self, id: &ObjectId, field: &str) -> Json {
        self.get(id, field).unwrap_or(Json::Null)
    }
    fn items(&self, id: &ObjectId, field: &str) -> Vec<ObjectId> {
        let mut seen = std::collections::HashSet::new();
        self.list(id, field).unwrap_or_default().into_iter().filter(|o| self.exists(o) && seen.insert(o.clone())).collect()
    }
    fn is(&self, id: &ObjectId, ty: &str) -> bool {
        self.object_type(id).ok().as_deref() == Some(ty)
    }
}

impl Store for Tx<'_> {
    fn field(&self, id: &ObjectId, field: &str) -> Json {
        self.get(id, field).unwrap_or(Json::Null)
    }
    fn items(&self, id: &ObjectId, field: &str) -> Vec<ObjectId> {
        let mut seen = std::collections::HashSet::new();
        self.list_items(id, field).unwrap_or_default().into_iter().filter(|o| self.exists(o) && seen.insert(o.clone())).collect()
    }
    fn is(&self, id: &ObjectId, ty: &str) -> bool {
        self.object_type(id).ok().as_deref() == Some(ty)
    }
}

/// The object types of a sheet's features and the `Sheet` list fields holding them.
pub(crate) const MERGE: &str = "Merge";
pub(crate) const COND_FORMAT: &str = "CondFormat";
pub(crate) const VALIDATION: &str = "Validation";
pub(crate) const TABLE: &str = "Table";
pub(crate) const TABLE_COLUMN: &str = "TableColumn";
pub(crate) const SPARKLINE: &str = "Sparkline";
pub(crate) const PIVOT: &str = "Pivot";
/// Every list of feature objects a sheet has, with its object type.
pub(crate) const FEATURE_LISTS: [(&str, &str); 6] =
    [("merges", MERGE), ("condFormats", COND_FORMAT), ("validations", VALIDATION), ("tables", TABLE), ("sparklines", SPARKLINE), ("pivots", PIVOT)];

/// `v` as an object's fields, without the members `drop`.
fn members(v: Json, drop: &[&str]) -> Map<String, Json> {
    let mut map = match v {
        Json::Object(map) => map,
        _ => Map::new(),
    };
    for key in drop {
        map.remove(*key);
    }
    map
}

fn object_of(fields: Vec<(&str, Json)>) -> Map<String, Json> {
    fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

/// A merged area's `Merge` fields: its range as a position token.
pub(crate) fn merge_fields<E: Encode + ?Sized>(e: &mut E, s: usize, r: RangeRef) -> KdmResult<Map<String, Json>> {
    Ok(object_of(vec![("range", Json::String(encode_range(e, s, r, Use::Content)?))]))
}

/// A conditional format's `CondFormat` fields: its ranges as position tokens, its rule with every
/// formula in id form, its priority and whether it stops the rules after it.
pub(crate) fn cond_format_fields<E: Encode + ?Sized>(e: &mut E, s: usize, cf: &CondFormat) -> KdmResult<Map<String, Json>> {
    let mut rule = cf.rule.clone();
    rule_strings(&mut rule, &mut |t: &mut String| {
        *t = encode_text(e, t, Some(s))?;
        Ok(())
    })?;
    Ok(object_of(vec![
        ("ranges", ranges(e, s, &cf.ranges, Use::Ref)?),
        ("rule", to_json(&rule)?),
        ("priority", json!(cf.priority)),
        ("stopIfTrue", json!(cf.stop_if_true)),
    ]))
}

/// A data validation's `Validation` fields: its ranges as position tokens, and the rest of it
/// (its formulas in id form) as its `rule`.
pub(crate) fn validation_fields<E: Encode + ?Sized>(e: &mut E, s: usize, dv: &Validation) -> KdmResult<Map<String, Json>> {
    let rule = members(to_json(&encoded(e, s, dv, validation_strings)?)?, &["ranges"]);
    Ok(object_of(vec![("ranges", ranges(e, s, &dv.ranges, Use::Ref)?), ("rule", Json::Object(rule))]))
}

/// A sparkline's `Sparkline` fields: its cell as a position token, its source in id form, the rest
/// as its `props`.
pub(crate) fn sparkline_fields<E: Encode + ?Sized>(e: &mut E, s: usize, sp: &Sparkline) -> KdmResult<Map<String, Json>> {
    let source = encode_text(e, &sp.source, Some(s))?;
    let props = members(to_json(sp)?, &["cell", "source"]);
    Ok(object_of(vec![
        ("cell", Json::String(encode_cell(e, s, sp.cell, Use::Ref)?)),
        ("source", Json::String(source)),
        ("props", Json::Object(props)),
    ]))
}

/// A table's or pivot table's `key` (its tool id `table:<key>`, `pivot:<key>`): its sheet's id and
/// its own.
pub(crate) fn feature_key(sheet_id: u32, id: u32) -> String {
    format!("{sheet_id}:{id}")
}

/// The local id at the end of a `key` ([`feature_key`]).
fn id_of_key(key: &Json) -> Option<u32> {
    key.as_str()?.rsplit(':').next()?.parse().ok()
}

/// A table's `Table` fields: its key, name, range (a position token) and the rest of it but its
/// columns (`TableColumn` objects, [`table_column_fields`]) as its `props`.
pub(crate) fn table_fields<E: Encode + ?Sized>(e: &mut E, s: usize, sheet_id: u32, t: &Table) -> KdmResult<Map<String, Json>> {
    let props = members(to_json(t)?, &["id", "name", "range", "columns"]);
    Ok(object_of(vec![
        ("key", Json::String(feature_key(sheet_id, t.id))),
        ("name", Json::String(t.name.clone())),
        ("range", Json::String(encode_range(e, s, t.range, Use::Content)?)),
        ("props", Json::Object(props)),
    ]))
}

/// A table column's `TableColumn` fields: its name, and the rest of it (its calculated formula and
/// custom totals label in id form) as its `props`.
pub(crate) fn table_column_fields<E: Encode + ?Sized>(e: &mut E, s: usize, c: &TableColumn) -> KdmResult<Map<String, Json>> {
    let mut c = c.clone();
    if let Some(formula) = c.formula.as_mut() {
        *formula = encode_text(e, formula, Some(s))?;
    }
    if c.totals == TotalsFn::Custom
        && let Some(label) = c.totals_label.as_mut()
    {
        *label = encode_text(e, label, Some(s))?;
    }
    let name = Json::String(c.name.clone());
    Ok(object_of(vec![("name", name), ("props", Json::Object(members(to_json(&c)?, &["name"])))]))
}

/// A pivot table's `Pivot` fields: its key, name, anchor cell and last range (position tokens),
/// its source in id form, and the rest of it as its `props`.
pub(crate) fn pivot_fields<E: Encode + ?Sized>(e: &mut E, s: usize, sheet_id: u32, p: &PivotTable) -> KdmResult<Map<String, Json>> {
    let last = match p.last_range {
        Some(r) => Json::String(encode_range(e, s, r, Use::Ref)?),
        None => Json::Null,
    };
    let props = members(to_json(p)?, &["id", "name", "source", "anchor", "last_range"]);
    Ok(object_of(vec![
        ("key", Json::String(feature_key(sheet_id, p.id))),
        ("name", Json::String(p.name.clone())),
        ("anchor", Json::String(encode_cell(e, s, p.anchor, Use::Ref)?)),
        ("source", Json::String(encode_text(e, &p.source, Some(s))?)),
        ("lastRange", last),
        ("props", Json::Object(props)),
    ]))
}

/// A sheet's filter, as its `autofilter` register holds it (its range a position token).
pub(crate) fn autofilter_value<E: Encode + ?Sized>(e: &mut E, s: usize, af: Option<&AutoFilter>) -> KdmResult<Json> {
    let Some(af) = af else { return Ok(Json::Null) };
    let mut v = to_json(af)?;
    set(&mut v, "range", Json::String(encode_range(e, s, af.range, Use::Content)?));
    Ok(v)
}

/// Whether a range token was written for a single cell (both ends of each axis the same line).
fn written_single(token: &str) -> bool {
    let Some((rows, cols)) = token.split_once(',') else { return false };
    let same = |axis: &str| axis.split_once(';').is_some_and(|(a, b)| a == b);
    same(rows) && same(cols)
}

/// A list of range tokens decoded, the ranges left empty dropped; `None` when no range is left.
fn read_ranges<D: Decode + ?Sized>(d: &D, s: usize, v: &Json) -> Option<Vec<RangeRef>> {
    let decoded: Vec<RangeRef> = v.as_array().into_iter().flatten().filter_map(Json::as_str).filter_map(|t| decode_range(d, s, t)).collect();
    (!decoded.is_empty()).then_some(decoded)
}

/// A string field's text.
fn text_of<S: Store + ?Sized>(store: &S, id: &ObjectId, field: &str) -> Option<String> {
    store.field(id, field).as_str().map(str::to_owned)
}

/// A merged area back. One that rows or columns deleted concurrently shrank to one cell is no
/// merge any more (the engine's rule); one that was written as one cell is kept as it was.
fn read_merge<D: Decode + ?Sized, S: Store + ?Sized>(store: &S, d: &D, s: usize, id: &ObjectId) -> Option<RangeRef> {
    let token = text_of(store, id, "range")?;
    decode_range(d, s, &token).filter(|m| !m.is_single() || written_single(&token))
}

fn read_cond_format<D: Decode + ?Sized, S: Store + ?Sized>(store: &S, d: &D, s: usize, id: &ObjectId) -> Option<CondFormat> {
    let ranges = read_ranges(d, s, &store.field(id, "ranges"))?;
    let mut rule: CfRule = from_json(store.field(id, "rule"))?;
    let _ = rule_strings(&mut rule, &mut |t: &mut String| {
        *t = decode_text(d, t, Some(s));
        Ok(())
    });
    let priority = store.field(id, "priority").as_u64().and_then(|p| u32::try_from(p).ok()).unwrap_or(0);
    let stop_if_true = store.field(id, "stopIfTrue").as_bool().unwrap_or(false);
    Some(CondFormat { ranges, rule, priority, stop_if_true })
}

fn read_validation<D: Decode + ?Sized, S: Store + ?Sized>(store: &S, d: &D, s: usize, id: &ObjectId) -> Option<Validation> {
    let ranges = read_ranges(d, s, &store.field(id, "ranges"))?;
    let mut rule = store.field(id, "rule");
    set(&mut rule, "ranges", to_json(&ranges).ok()?);
    from_json::<Validation>(rule).map(|dv| decoded(d, s, dv, validation_strings))
}

fn read_sparkline<D: Decode + ?Sized, S: Store + ?Sized>(store: &S, d: &D, s: usize, id: &ObjectId) -> Option<Sparkline> {
    let cell = decode_cell(d, s, &text_of(store, id, "cell")?)?;
    let source = decode_text(d, &text_of(store, id, "source").unwrap_or_default(), Some(s));
    let mut v = store.field(id, "props");
    set(&mut v, "cell", to_json(&cell).ok()?);
    set(&mut v, "source", Json::String(source));
    from_json(v)
}

fn read_table<D: Decode + ?Sized, S: Store + ?Sized>(store: &S, d: &D, s: usize, id: &ObjectId) -> Option<Table> {
    let range = decode_range(d, s, &text_of(store, id, "range")?)?;
    let mut columns = Vec::new();
    for column in store.items(id, "columns") {
        if !store.is(&column, TABLE_COLUMN) {
            continue;
        }
        let mut v = store.field(&column, "props");
        set(&mut v, "name", Json::String(text_of(store, &column, "name").unwrap_or_default()));
        let Some(mut c) = from_json::<TableColumn>(v) else { continue };
        if let Some(formula) = c.formula.as_mut() {
            *formula = decode_text(d, formula, Some(s));
        }
        if c.totals == TotalsFn::Custom
            && let Some(label) = c.totals_label.as_mut()
        {
            *label = decode_text(d, label, Some(s));
        }
        columns.push(c);
    }
    let mut v = store.field(id, "props");
    set(&mut v, "id", json!(id_of_key(&store.field(id, "key")).unwrap_or(0)));
    set(&mut v, "name", Json::String(text_of(store, id, "name").unwrap_or_default()));
    set(&mut v, "range", to_json(&range).ok()?);
    set(&mut v, "columns", to_json(&columns).ok()?);
    from_json(v)
}

fn read_pivot<D: Decode + ?Sized, S: Store + ?Sized>(store: &S, d: &D, s: usize, id: &ObjectId) -> Option<PivotTable> {
    let anchor = text_of(store, id, "anchor").and_then(|t| decode_cell(d, s, &t)).unwrap_or_default();
    let last = text_of(store, id, "lastRange").and_then(|t| decode_range(d, s, &t));
    let mut v = store.field(id, "props");
    set(&mut v, "id", json!(id_of_key(&store.field(id, "key")).unwrap_or(0)));
    set(&mut v, "name", Json::String(text_of(store, id, "name").unwrap_or_default()));
    set(&mut v, "anchor", to_json(&anchor).ok()?);
    set(&mut v, "source", Json::String(decode_text(d, &text_of(store, id, "source").unwrap_or_default(), Some(s))));
    set(&mut v, "last_range", to_json(&last).ok()?);
    from_json(v)
}

/// Reads sheet `sheet`'s feature objects and its filter into `sh`, in list order, and answers
/// the objects of what it read. What does not read (or has nothing left on the grid) is skipped,
/// as is a merged area or a table overlapping one read before it (made concurrently by two
/// people): the engine keeps them apart.
pub(crate) fn read_features<D: Decode + ?Sized, S: Store + ?Sized>(store: &S, d: &D, s: usize, sheet: &ObjectId, sh: &mut Sheet) -> FeatureIds {
    let of = |field: &str, ty: &str| -> Vec<ObjectId> { store.items(sheet, field).into_iter().filter(|o| store.is(o, ty)).collect() };
    let mut ids = FeatureIds::default();
    for id in of("merges", MERGE) {
        if let Some(m) = read_merge(store, d, s, &id)
            && !sh.merges.iter().any(|x| x.intersects(&m))
        {
            sh.merges.push(m);
            ids.merges.push(id);
        }
    }
    for id in of("condFormats", COND_FORMAT) {
        if let Some(cf) = read_cond_format(store, d, s, &id) {
            sh.cond_formats.push(cf);
            ids.cond_formats.push(id);
        }
    }
    for id in of("validations", VALIDATION) {
        if let Some(dv) = read_validation(store, d, s, &id) {
            sh.validations.push(dv);
            ids.validations.push(id);
        }
    }
    for id in of("tables", TABLE) {
        if let Some(t) = read_table(store, d, s, &id)
            && !sh.tables.iter().any(|x| x.range.intersects(&t.range))
        {
            sh.tables.push(t);
            ids.tables.push(id);
        }
    }
    sh.autofilter = Some(store.field(sheet, "autofilter")).filter(|v| v.is_object()).and_then(|item| {
        let r = item.get("range").and_then(Json::as_str).and_then(|t| decode_range(d, s, t))?;
        let mut item = item.clone();
        set(&mut item, "range", to_json(&r).ok()?);
        from_json::<AutoFilter>(item)
    });
    for id in of("sparklines", SPARKLINE) {
        if let Some(sp) = read_sparkline(store, d, s, &id) {
            sh.sparklines.push(sp);
            ids.sparklines.push(id);
        }
    }
    for id in of("pivots", PIVOT) {
        if let Some(p) = read_pivot(store, d, s, &id) {
            sh.pivots.push(p);
            ids.pivots.push(id);
        }
    }
    ids
}

// ---------------------------------------------------------------- links

/// A link's value in its sheet's `hyperlinks` map: a target in the workbook (a cell or range,
/// `Sheet2!A1`) in id form (`codec`), so it follows rows, columns and sheets moved or renamed.
pub(crate) fn hyperlink_value<E: Encode + ?Sized>(e: &mut E, s: usize, h: &Hyperlink) -> KdmResult<Json> {
    let mut v = to_json(h)?;
    if !h.is_external() {
        // A place may be written behind a `#` (`#Sheet2!A1`), as a file's location is.
        let (hash, place) = h.target.strip_prefix('#').map_or(("", h.target.as_str()), |place| ("#", place));
        set(&mut v, "target", Json::String(format!("{hash}{}", encode_formula(e, place, None, Some(s))?)));
    }
    Ok(v)
}

/// A link back from its `hyperlinks` entry.
pub(crate) fn read_hyperlink<D: Decode + ?Sized>(d: &D, s: usize, v: Json) -> Option<Hyperlink> {
    let mut h: Hyperlink = from_json(v)?;
    if !h.is_external() {
        let (hash, place) = h.target.strip_prefix('#').map_or(("", h.target.as_str()), |place| ("#", place));
        h.target = format!("{hash}{}", decode_formula(d, place, Some(s)));
    }
    Some(h)
}

// ---------------------------------------------------------------- images and names

/// An image's `props`: everything but its bytes (the `blob` field).
pub(crate) fn image_props<E: Encode + ?Sized>(e: &mut E, s: usize, img: &Image) -> KdmResult<Json> {
    let mut v = json!({ "id": img.id, "anchor": anchor_value(e, s, &img.anchor)?, "mime": img.mime, "alt": img.alt });
    // Additive: a reader that does not know it leaves it out.
    if let Some(ext) = &img.blip_ext {
        set(&mut v, "blipExt", Json::String(ext.clone()));
    }
    Ok(v)
}

pub(crate) fn read_image<D: Decode + ?Sized>(d: &D, s: usize, v: &Json, data: Vec<u8>) -> Image {
    Image {
        id: field(v, "id").unwrap_or(0),
        anchor: v.get("anchor").and_then(|a| read_anchor(d, s, a)).unwrap_or_default(),
        data,
        mime: field(v, "mime").unwrap_or_else(|| "image/png".to_owned()),
        alt: field(v, "alt").unwrap_or_default(),
        blip_ext: field(v, "blipExt"),
    }
}

/// A defined name's `props`: its comment and whether it is hidden.
pub(crate) fn name_props(n: &kition_sheet_model::DefinedName) -> Json {
    json!({ "comment": n.comment, "hidden": n.hidden })
}
