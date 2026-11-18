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
//! - Sheet `features`: merges, conditional formats, validations, tables, the autofilter, charts,
//!   sparklines and pivot tables, each range and anchor a position token, each formula in id form.
//!   Shapes were here before schema version 3 and still read from here (`crate::shapes`).
//!
//! Not stored (G4): the sheet's zoom, active cell and scroll position.

use kition_sheet_core::RangeRef;
use kition_sheet_model::{
    AutoFilter, CfRule, CfValueKind, Chart, CondFormat, Image, LineInfo, PivotTable, PrintSettings, Shape, Sheet, SheetProtection, Sparkline, Table,
    TotalsFn, Validation, Workbook,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value as Json, json};

use crate::codec::{Decode, Encode, decode_cell, decode_line, decode_range, decode_text, encode_cell, encode_range, encode_text};
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

fn table_strings(t: &mut Table, f: &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()> {
    for c in t.columns.iter_mut() {
        if let Some(formula) = c.formula.as_mut() {
            f(formula)?;
        }
        // A totals label is a formula only for a custom total.
        if c.totals == TotalsFn::Custom
            && let Some(label) = c.totals_label.as_mut()
        {
            f(label)?;
        }
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

fn sparkline_strings(sp: &mut Sparkline, f: &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()> {
    f(&mut sp.source)
}

fn pivot_strings(p: &mut PivotTable, f: &mut dyn FnMut(&mut String) -> KdmResult<()>) -> KdmResult<()> {
    f(&mut p.source)
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

/// A sheet's `features` register: only the kinds it has.
pub(crate) fn sheet_features<E: Encode + ?Sized>(e: &mut E, s: usize, sh: &Sheet) -> KdmResult<Json> {
    let mut out = Map::new();
    if !sh.merges.is_empty() {
        out.insert("merges".into(), ranges(e, s, &sh.merges, Use::Content)?);
    }
    if !sh.cond_formats.is_empty() {
        let mut items = Vec::new();
        for cf in &sh.cond_formats {
            let mut item: CondFormat = cf.clone();
            rule_strings(&mut item.rule, &mut |t: &mut String| {
                *t = encode_text(e, t, Some(s))?;
                Ok(())
            })?;
            let mut v = to_json(&item)?;
            set(&mut v, "ranges", ranges(e, s, &cf.ranges, Use::Ref)?);
            items.push(v);
        }
        out.insert("condFormats".into(), Json::Array(items));
    }
    if !sh.validations.is_empty() {
        let mut items = Vec::new();
        for dv in &sh.validations {
            let mut v = to_json(&encoded(e, s, dv, validation_strings)?)?;
            set(&mut v, "ranges", ranges(e, s, &dv.ranges, Use::Ref)?);
            items.push(v);
        }
        out.insert("validations".into(), Json::Array(items));
    }
    if !sh.tables.is_empty() {
        let mut items = Vec::new();
        for t in &sh.tables {
            let mut v = to_json(&encoded(e, s, t, table_strings)?)?;
            set(&mut v, "range", Json::String(encode_range(e, s, t.range, Use::Content)?));
            items.push(v);
        }
        out.insert("tables".into(), Json::Array(items));
    }
    if let Some(af) = &sh.autofilter {
        let mut v = to_json(af)?;
        set(&mut v, "range", Json::String(encode_range(e, s, af.range, Use::Content)?));
        out.insert("autofilter".into(), v);
    }
    // Charts and shapes are objects of their own (`crate::charts`, `crate::shapes`), no longer
    // written here.
    if !sh.sparklines.is_empty() {
        let mut items = Vec::new();
        for sp in &sh.sparklines {
            let mut v = to_json(&encoded(e, s, sp, sparkline_strings)?)?;
            set(&mut v, "cell", Json::String(encode_cell(e, s, sp.cell, Use::Ref)?));
            items.push(v);
        }
        out.insert("sparklines".into(), Json::Array(items));
    }
    if !sh.pivots.is_empty() {
        let mut items = Vec::new();
        for p in &sh.pivots {
            let mut v = to_json(&encoded(e, s, p, pivot_strings)?)?;
            set(&mut v, "anchor", Json::String(encode_cell(e, s, p.anchor, Use::Ref)?));
            let last = match p.last_range {
                Some(r) => Json::String(encode_range(e, s, r, Use::Ref)?),
                None => Json::Null,
            };
            set(&mut v, "last_range", last);
            items.push(v);
        }
        out.insert("pivots".into(), Json::Array(items));
    }
    Ok(Json::Object(out))
}

/// Whether a range token was written for a single cell (both ends of each axis the same line).
fn written_single(token: &str) -> bool {
    let Some((rows, cols)) = token.split_once(',') else { return false };
    let same = |axis: &str| axis.split_once(';').is_some_and(|(a, b)| a == b);
    same(rows) && same(cols)
}

fn items<'v>(v: &'v Json, key: &str) -> impl Iterator<Item = &'v Json> {
    v.get(key).and_then(Json::as_array).into_iter().flatten()
}

/// `item` with its `key` (a list of range tokens) decoded, the ranges left empty dropped;
/// `None` when no range is left.
fn with_ranges<D: Decode + ?Sized>(d: &D, s: usize, item: &Json, key: &str) -> Option<Json> {
    let decoded: Vec<Json> =
        items(item, key).filter_map(Json::as_str).filter_map(|t| decode_range(d, s, t)).filter_map(|r| to_json(&r).ok()).collect();
    if decoded.is_empty() {
        return None;
    }
    let mut item = item.clone();
    set(&mut item, key, Json::Array(decoded));
    Some(item)
}

/// `item` with its `key` (one range token) decoded; `None` when nothing is left of the range.
fn with_range<D: Decode + ?Sized>(d: &D, s: usize, item: &Json, key: &str) -> Option<Json> {
    let r = item.get(key).and_then(Json::as_str).and_then(|t| decode_range(d, s, t))?;
    let mut item = item.clone();
    set(&mut item, key, to_json(&r).ok()?);
    Some(item)
}

fn with_anchor<D: Decode + ?Sized>(d: &D, s: usize, item: &Json) -> Option<Json> {
    let anchor = read_anchor(d, s, item.get("anchor")?)?;
    let mut item = item.clone();
    set(&mut item, "anchor", to_json(&anchor).ok()?);
    Some(item)
}

/// Reads a sheet's `features` into `sh`. What does not read (or has nothing left on the grid) is
/// skipped.
pub(crate) fn read_features<D: Decode + ?Sized>(d: &D, s: usize, v: &Json, sh: &mut Sheet) {
    // A merge that rows or columns deleted concurrently shrank to one cell is no merge any more
    // (the engine's rule); one that was written as one cell is kept as it was.
    sh.merges =
        items(v, "merges").filter_map(Json::as_str).filter_map(|t| decode_range(d, s, t).filter(|m| !m.is_single() || written_single(t))).collect();
    sh.cond_formats = items(v, "condFormats")
        .filter_map(|item| with_ranges(d, s, item, "ranges"))
        .filter_map(from_json::<CondFormat>)
        .map(|mut cf| {
            let _ = rule_strings(&mut cf.rule, &mut |t: &mut String| {
                *t = decode_text(d, t, Some(s));
                Ok(())
            });
            cf
        })
        .collect();
    sh.validations = items(v, "validations")
        .filter_map(|item| with_ranges(d, s, item, "ranges"))
        .filter_map(from_json::<Validation>)
        .map(|dv| decoded(d, s, dv, validation_strings))
        .collect();
    sh.tables = items(v, "tables")
        .filter_map(|item| with_range(d, s, item, "range"))
        .filter_map(from_json::<Table>)
        .map(|t| decoded(d, s, t, table_strings))
        .collect();
    sh.autofilter = v.get("autofilter").and_then(|item| with_range(d, s, item, "range")).and_then(from_json::<AutoFilter>);
    sh.charts = items(v, "charts")
        .filter_map(|item| with_anchor(d, s, item))
        .filter_map(from_json::<Chart>)
        .map(|c| decoded(d, s, c, chart_strings))
        .collect();
    sh.shapes = items(v, "shapes").filter_map(|item| with_anchor(d, s, item)).filter_map(from_json::<Shape>).collect();
    sh.sparklines = items(v, "sparklines")
        .filter_map(|item| {
            let cell = item.get("cell").and_then(Json::as_str).and_then(|t| decode_cell(d, s, t))?;
            let mut item = item.clone();
            set(&mut item, "cell", to_json(&cell).ok()?);
            Some(item)
        })
        .filter_map(from_json::<Sparkline>)
        .map(|sp| decoded(d, s, sp, sparkline_strings))
        .collect();
    sh.pivots = items(v, "pivots")
        .filter_map(|item| {
            let anchor = item.get("anchor").and_then(Json::as_str).and_then(|t| decode_cell(d, s, t)).unwrap_or_default();
            let last = item.get("last_range").and_then(Json::as_str).and_then(|t| decode_range(d, s, t));
            let mut item = item.clone();
            set(&mut item, "anchor", to_json(&anchor).ok()?);
            set(&mut item, "last_range", to_json(&last).ok()?);
            Some(item)
        })
        .filter_map(from_json::<PivotTable>)
        .map(|p| decoded(d, s, p, pivot_strings))
        .collect();
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
