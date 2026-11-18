//! `drawing.*`: the Kit Harness Protocol's `KitDrawing 1.0` commands over the active sheet's
//! shapes and text boxes, through the drawing model Kition's kits share (`kition-office-core`). A
//! shape's frame is resolved from its cell anchor in sheet points; a changed frame is written
//! back as a cell and an offset, keeping the anchor's mode (move and size with cells, move only,
//! absolute). Only the members an operation changed are written back.
//!
//! Charts (`KitDrawing 1.1`) are drawing objects too, described and changed through `chart_spec`:
//! they share the sheet's object numbering with shapes, so an id names one or the other.

use kition_office_core::model::{self as dm, DrawingObject};
use kition_office_core::ops::{DrawingError, DrawingOp};
use kition_office_core::{Xfrm, pt_to_emu};
use kition_sheet_core::CellRef;
use kition_sheet_model::{Anchor, AnchorMode, Color, CustomGeom, CustomPath, PathFill, PresetGeom, Shape, ShapeKind, Sheet, Workbook};
use serde_json::{Value as Json, json};

use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "drawing.describe", "Describe Drawing Objects", [], None, "{ids?} (default: the selected object, else every shape on the sheet)", has_doc, describe),
        cmd!(
            "drawing.insert",
            "Insert Drawing Object",
            [],
            None,
            "{kind?: shape|textBox|chart, geometry?: {preset, adjust?}, frame?: {x,y,w,h,rot?,flipH?,flipV?} (sheet points), placement?: {kind: cell, mode?}, fill?, stroke?, text?, chart?: {kind?, title?, legend?, dataLabels?, gridlines?, xTitle?, yTitle?} (a chart of the selected cells, or of `range`), range?}",
            has_doc,
            insert
        ),
        cmd!("drawing.delete", "Delete Drawing Objects", [], None, "{ids}", has_doc, |s, p| apply(s, "drawing.delete", p)),
        cmd!("drawing.transform", "Transform Drawing Objects", [], None, "{ids, frame? | dx?, dy?, rotateBy?, flipH?, flipV?}", has_doc, |s, p| {
            apply(s, "drawing.transform", p)
        }),
        cmd!("drawing.format", "Format Drawing Objects", [], None, "{ids, fill?, stroke?, geometry?}", has_doc, |s, p| apply(s, "drawing.format", p)),
        cmd!("drawing.text", "Set Drawing Object Text", [], None, "{id, text? | paragraphs?}", has_doc, |s, p| apply(s, "drawing.text", p)),
        cmd!("drawing.reconnect", "Reconnect Connector", [], None, "{id, end: start|end, to?: id, site?: n, point?: [x, y]}", has_doc, |s, p| apply(
            s,
            "drawing.reconnect",
            p
        )),
        cmd!(
            "drawing.connect",
            "Connect Drawing Objects",
            [],
            None,
            "{from, to, fromSite?, toSite?, geometry?: {preset: straightConnector1|bentConnector3|curvedConnector3}}",
            has_doc,
            connect
        ),
        cmd!("drawing.group", "Group Drawing Objects", [], None, "{ids} (two or more shapes or groups on the sheet, not in a group)", has_doc, group),
        cmd!("drawing.ungroup", "Ungroup Drawing Objects", [], None, "{ids} (groups)", has_doc, ungroup),
        cmd!(
            "drawing.arrange",
            "Arrange Drawing Objects",
            [],
            None,
            "{ids, order: front|back|forward|backward} (charts stay above shapes)",
            has_doc,
            arrange
        ),
        cmd!(
            "drawing.chart",
            "Change Chart",
            [],
            None,
            "{id, kind?, title?: text|null, legend?: top|bottom|left|right|null, dataLabels?, gridlines?, xTitle?, yTitle?, series?: [{name, values, color?}] (colours only: the data are the cells)}",
            has_doc,
            |s, p| apply(s, "drawing.chart", p)
        ),
    ]
}

/// Longest text a shape holds, as a cell does.
const MAX_TEXT: usize = 32_767;

fn fail(cmd: &str, e: DrawingError) -> EngineError {
    let property = e.property.as_deref().map(|p| format!(" ({p})")).unwrap_or_default();
    bad(cmd, format!("{}{property}: {}", e.code, e.message))
}

/// Shapes and text boxes, and ink strokes (`KitDrawing 1.2`); icons are not drawing objects.
fn is_drawing(sh: &Shape) -> bool {
    !matches!(sh.kind, ShapeKind::Icon)
}

fn object_ids(p: &Json) -> std::result::Result<Option<Vec<u32>>, DrawingError> {
    let list: Vec<&Json> = match (p.get("ids"), p.get("id")) {
        (Some(Json::Array(a)), _) => a.iter().collect(),
        (None, Some(v)) => vec![v],
        (None, None) => return Ok(None),
        _ => return Err(DrawingError::invalid("`ids` is a list of ids")),
    };
    list.into_iter()
        .map(|v| {
            let text = match v {
                Json::String(s) => s.clone(),
                Json::Number(n) => n.to_string(),
                _ => return Err(DrawingError::invalid("an id is a string")),
            };
            text.parse::<u32>().map_err(|_| DrawingError::not_found(&text))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map(Some)
}

/// The active sheet's drawing objects, bottom to top: its shapes, then its charts (drawn over them).
pub fn objects(wb: &Workbook) -> Vec<DrawingObject> {
    let si = wb.active_sheet;
    let Some(sheet) = wb.active() else { return Vec::new() };
    // A group stands where its first member is, in the members' place.
    let mut out = Vec::new();
    let mut groups_seen = Vec::new();
    for sh in sheet.shapes.iter().filter(|sh| is_drawing(sh)) {
        match sh.groups.first() {
            None => out.push(to_drawing(sheet, sh)),
            Some(&key) if !groups_seen.contains(&key) => {
                groups_seen.push(key);
                if let Some(g) = group_object(sheet, key) {
                    out.push(g);
                }
            }
            Some(_) => {}
        }
    }
    out.extend(sheet.charts.iter().map(|c| chart_spec::to_drawing(wb, si, sheet, c)));
    out
}

/// The shapes in group `key` (at any depth), in their order.
fn group_shapes(sheet: &Sheet, key: u32) -> Vec<&Shape> {
    sheet.shapes.iter().filter(|sh| sh.groups.contains(&key)).collect()
}

/// Is `id` a group's key on `sheet` (and not a shape's id)?
fn is_group(sheet: &Sheet, id: u32) -> bool {
    !sheet.shapes.iter().any(|sh| sh.id == id) && sheet.shapes.iter().any(|sh| sh.groups.contains(&id))
}

/// Group `key`, described (`KitDrawing 1.4`, DR22): its frame bounds its members, which are its
/// shapes and its nested groups, by id.
pub fn group_object(sheet: &Sheet, key: u32) -> Option<DrawingObject> {
    let shapes = group_shapes(sheet, key);
    let bounds = shapes.iter().map(|sh| to_drawing(sheet, sh).frame.bounds()).reduce(|a, b| a.union(b))?;
    let mut members: Vec<String> = Vec::new();
    for sh in &shapes {
        let depth = sh.groups.iter().position(|k| *k == key)?;
        let member = sh.groups.get(depth + 1).copied().unwrap_or(sh.id).to_string();
        if !members.contains(&member) {
            members.push(member);
        }
    }
    let mut placement = dm::Placement { kind: dm::PlacementKind::Cell, ..Default::default() };
    placement.extra.insert("mode".into(), json!("moveOnly"));
    Some(DrawingObject {
        id: key.to_string(),
        kind: dm::ObjectKind::Group,
        frame: Xfrm::new(bounds.x0, bounds.y0, bounds.width(), bounds.height()),
        placement,
        members: Some(members),
        ..Default::default()
    })
}

/// Object `id` on the sheet, described: a shape (a group's member among them), a group, or a chart.
fn object_of(wb: &Workbook, id: u32) -> Option<DrawingObject> {
    let si = wb.active_sheet;
    let sheet = wb.active()?;
    if let Some(sh) = sheet.shapes.iter().find(|sh| sh.id == id && is_drawing(sh)) {
        return Some(to_drawing(sheet, sh));
    }
    if is_group(sheet, id) {
        return group_object(sheet, id);
    }
    sheet.charts.iter().find(|c| c.id == id).map(|c| chart_spec::to_drawing(wb, si, sheet, c))
}

fn describe(s: &mut Session, p: &Json) -> Result<Json> {
    let d = s.doc()?;
    let all = objects(&d.wb);
    let ids = match object_ids(p).map_err(|e| fail("drawing.describe", e))? {
        Some(ids) => ids,
        None => match s.selected_shape {
            Some(id) => vec![id],
            None => all.iter().filter_map(|o| o.id.parse().ok()).collect(),
        },
    };
    let mut objects = Vec::new();
    for id in ids {
        let text = id.to_string();
        let o = all
            .iter()
            .find(|o| o.id == text)
            .cloned()
            .or_else(|| object_of(&d.wb, id))
            .ok_or_else(|| fail("drawing.describe", DrawingError::not_found(&text)))?;
        objects.push(o);
    }
    let mut out = json!({ "objects": objects });
    if objects.len() > 1 {
        out["selection"] = dm::describe_many(&objects);
    }
    Ok(out)
}

pub(crate) fn insert(s: &mut Session, p: &Json) -> Result<Json> {
    let DrawingOp::Insert(o) = DrawingOp::parse("drawing.insert", p).map_err(|e| fail("drawing.insert", e))? else {
        return Err(bad("drawing.insert", "not an insert"));
    };
    let mut o = *o;
    if p.get("placement").is_none() {
        o.placement.kind = dm::PlacementKind::Cell;
    }
    if o.placement.kind != dm::PlacementKind::Cell {
        return Err(fail("drawing.insert", DrawingError::unsupported("placement.kind", "objects on a sheet are anchored to cells")));
    }
    if o.kind == dm::ObjectKind::Chart {
        return insert_chart(s, p, &o);
    }
    if o.kind == dm::ObjectKind::Ink {
        return Err(fail("drawing.insert", DrawingError::unsupported("kind", "ink is drawn with drawing.ink")));
    }
    let d = s.doc()?;
    let sheet_index = d.wb.active_sheet;
    let sheet = d.wb.active().ok_or(EngineError::NoDocument)?;
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    if p.get("frame").is_none() {
        // At the active cell, the size `insert.shape` gives.
        let at = d.selection.active;
        let (w, h) = if o.kind == dm::ObjectKind::TextBox { (144.0, 48.0) } else { (144.0, 96.0) };
        o.frame = Xfrm::new(sheet.col_left(at.col), sheet.row_top(at.row), w, h);
    }
    let id = next_object_id(&d.wb)?;
    let text_box = o.kind == dm::ObjectKind::TextBox;
    let mut shape = Shape {
        id,
        kind: if text_box { ShapeKind::TextBox } else { ShapeKind::Rectangle },
        // A text box is white with a grey pen, a shape the theme's accent, as `insert.shape` makes them.
        fill: if text_box { Color::rgb(255, 255, 255) } else { Color::Theme(4, 0) },
        line: if text_box { Color::rgb(0x80, 0x80, 0x80) } else { Color::Theme(4, -250) },
        // A shape's text is white, as in a shape Excel inserts.
        style_refs: (!text_box).then(|| kition_sheet_xlsx::DEFAULT_SHAPE_STYLE.to_string()),
        ..Default::default()
    };
    if let Some(mode) = o.placement.extra.get("mode").and_then(Json::as_str) {
        shape.anchor.mode =
            anchor_mode(mode).ok_or_else(|| fail("drawing.insert", DrawingError::invalid("mode is moveAndSize, moveOnly or absolute")))?;
    }
    let base = DrawingObject { text: None, geometry: dm::Geometry::preset("rect"), ..to_drawing(sheet, &shape) };
    // What the command leaves out keeps the new shape's defaults.
    if p.get("fill").is_none() {
        o.fill.clone_from(&base.fill);
    }
    if p.get("stroke").is_none() {
        o.stroke.clone_from(&base.stroke);
    }
    write_back(sheet, &mut shape, &base, &o).map_err(|e| fail("drawing.insert", e))?;
    edit(s, |cx| {
        cx.sheet_mut(sheet_index)?.shapes.push(shape);
        Ok(())
    })?;
    s.selected_shape = Some(id);
    Ok(json!({ "id": id.to_string() }))
}

/// `drawing.insert` of a chart: Insert › Chart of the selected cells (or `range`), then the members
/// given. Its data are the cells', so categories and series are refused (DR16).
fn insert_chart(s: &mut Session, p: &Json, o: &DrawingObject) -> Result<Json> {
    let given = p.get("chart").cloned().unwrap_or_else(|| json!({}));
    for key in ["categories", "series"] {
        if given.get(key).is_some() {
            return Err(fail(
                "drawing.insert",
                DrawingError::unsupported(&format!("chart.{key}"), "a chart shows the sheet's cells: select them, or give `range`"),
            ));
        }
    }
    let spec = o.chart.clone().unwrap_or_default();
    let (t, sub) = chart_spec::insert_type(&spec.kind)
        .ok_or_else(|| fail("drawing.insert", DrawingError::unsupported("chart.kind", "insert a chart of a type the shared controls offer")))?;
    let mut params = json!({ "type": t, "subtype": sub });
    if let Some(range) = p.get("range") {
        params["range"] = range.clone();
    }
    if let Some(title) = given.get("title").and_then(Json::as_str) {
        params["title"] = json!(title);
    }
    if p.get("frame").is_some() {
        params["width"] = json!(o.frame.w);
        params["height"] = json!(o.frame.h);
    }
    let made = s.execute("insert.chart", params)?;
    let id = made.get("chart").and_then(Json::as_u64).ok_or_else(|| bad("drawing.insert", "the new chart was not made"))?.to_string();
    // The members given beyond the type and title, and the place, as `drawing.chart` and
    // `drawing.transform` set them (one undo step with the insert: commands run inside it push none).
    let rest: serde_json::Map<String, Json> = given
        .as_object()
        .map(|m| m.iter().filter(|(k, _)| !["kind", "title"].contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    if !rest.is_empty() {
        let mut change = Json::Object(rest);
        change["id"] = json!(id);
        apply(s, "drawing.chart", &change)?;
    }
    if let Some(f) = p.get("frame").filter(|f| f.get("x").is_some() || f.get("y").is_some()) {
        apply(
            s,
            "drawing.transform",
            &json!({ "ids": [id], "frame": { "x": f.get("x").cloned().unwrap_or(json!(0)), "y": f.get("y").cloned().unwrap_or(json!(0)), "w": o.frame.w, "h": o.frame.h } }),
        )?;
    }
    s.selected_shape = id.parse().ok();
    Ok(json!({ "id": id }))
}

fn apply(s: &mut Session, cmd: &str, p: &Json) -> Result<Json> {
    let op = DrawingOp::parse(cmd, p).map_err(|e| fail(cmd, e))?;
    let ids: Vec<u32> = op.ids().iter().map(|id| id.parse::<u32>().map_err(|_| fail(cmd, DrawingError::not_found(id)))).collect::<Result<_>>()?;
    let d = s.doc()?;
    let sheet_index = d.wb.active_sheet;
    let sheet = d.wb.active().ok_or(EngineError::NoDocument)?;
    // Sheet protection locks drawing objects (there is no "edit objects" allowance yet).
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    let mut changes = Vec::new();
    let mut charts = Vec::new();
    for id in &ids {
        // A group: deleted with its members, moved and sized by carrying them (DR22).
        if is_group(sheet, *id) {
            if matches!(op, DrawingOp::Delete(_)) {
                continue;
            }
            let before = group_object(sheet, *id).ok_or_else(|| fail(cmd, DrawingError::not_found(&id.to_string())))?;
            let mut after = before.clone();
            op.apply(&mut after).map_err(|e| fail(cmd, e))?;
            let (b, a) = (before.frame, after.frame);
            if (a.rot, a.flip_h, a.flip_v) != (b.rot, b.flip_h, b.flip_v) {
                return Err(fail(cmd, DrawingError::unsupported("frame.rot", "a sheet's group moves and resizes; its members turn one by one")));
            }
            if (DrawingObject { frame: b, ..after.clone() }) != before {
                return Err(fail(cmd, DrawingError::unsupported("kind", "a group's members are formatted one by one")));
            }
            kition_office_core::ops::check_frame(&a).map_err(|e| fail(cmd, e))?;
            let (sx, sy) = (a.w / b.w.max(1e-6), a.h / b.h.max(1e-6));
            for sh in group_shapes(sheet, *id) {
                let mb = to_drawing(sheet, sh);
                let mut ma = mb.clone();
                ma.frame.x = a.x + (mb.frame.x - b.x) * sx;
                ma.frame.y = a.y + (mb.frame.y - b.y) * sy;
                ma.frame.w = (mb.frame.w * sx).max(1.0);
                ma.frame.h = (mb.frame.h * sy).max(1.0);
                let mut shape = sh.clone();
                write_back(sheet, &mut shape, &mb, &ma).map_err(|e| fail(cmd, e))?;
                changes.push(shape);
            }
            continue;
        }
        if let Some(c) = sheet.charts.iter().find(|c| c.id == *id) {
            let before = chart_spec::to_drawing(&d.wb, sheet_index, sheet, c);
            let mut after = before.clone();
            op.apply(&mut after).map_err(|e| fail(cmd, e))?;
            let mut chart = c.clone();
            chart_spec::write_back(sheet, &mut chart, &before, &after).map_err(|e| fail(cmd, e))?;
            charts.push(chart);
            continue;
        }
        if matches!(op, DrawingOp::Chart { .. }) {
            return Err(fail(cmd, DrawingError::unsupported("kind", format!("object {id} is not a chart"))));
        }
        let sh = sheet.shapes.iter().find(|sh| sh.id == *id).ok_or_else(|| fail(cmd, DrawingError::not_found(&id.to_string())))?;
        if !is_drawing(sh) {
            return Err(fail(cmd, DrawingError::unsupported("kind", format!("object {id} is an icon or an ink stroke"))));
        }
        let before = to_drawing(sheet, sh);
        let mut after = before.clone();
        op.apply(&mut after).map_err(|e| fail(cmd, e))?;
        if after.frame != before.frame && !sh.groups.is_empty() {
            return Err(fail(cmd, DrawingError::unsupported("frame", format!("object {id} is in a group: move the group, or ungroup it"))));
        }
        let mut shape = sh.clone();
        write_back(sheet, &mut shape, &before, &after).map_err(|e| fail(cmd, e))?;
        changes.push(shape);
    }
    let delete = matches!(op, DrawingOp::Delete(_));
    let frames: Vec<Json> = changes
        .iter()
        .map(|sh| json!(to_drawing(sheet, sh).frame))
        .chain(charts.iter().map(|c| json!(chart_spec::to_drawing(&d.wb, sheet_index, sheet, c).frame)))
        .collect();
    edit(s, |cx| {
        let sheet = cx.sheet_mut(sheet_index)?;
        if delete {
            sheet.shapes.retain(|sh| !ids.contains(&sh.id) && !sh.groups.iter().any(|k| ids.contains(k)));
            sheet.charts.retain(|c| !ids.contains(&c.id));
            return Ok(());
        }
        for shape in &changes {
            if let Some(slot) = sheet.shapes.iter_mut().find(|sh| sh.id == shape.id) {
                *slot = shape.clone();
            }
        }
        reroute_connectors(sheet);
        for chart in &charts {
            if let Some(slot) = sheet.charts.iter_mut().find(|c| c.id == chart.id) {
                *slot = chart.clone();
            }
        }
        Ok(())
    })?;
    if delete {
        if s.selected_shape.is_some_and(|id| ids.contains(&id)) {
            s.selected_shape = None;
        }
        return Ok(json!({ "deleted": ids.len() }));
    }
    Ok(json!({ "frames": frames }))
}

/// `drawing.connect` (`KitDrawing 1.4`, DR25), one undo step: a connector line glued to two
/// shapes' sites (named, or the nearest pair), which follows them.
fn connect(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "drawing.connect";
    let c = kition_office_core::connect::Connect::parse(p).map_err(|e| fail(CMD, e))?;
    let d = s.doc()?;
    let sheet_index = d.wb.active_sheet;
    let sheet = d.wb.active().ok_or(EngineError::NoDocument)?;
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    let end_of = |text: &str| -> Result<(u32, DrawingObject)> {
        let id: u32 = text.parse().map_err(|_| fail(CMD, DrawingError::not_found(text)))?;
        let sh = sheet.shapes.iter().find(|sh| sh.id == id && is_drawing(sh)).ok_or_else(|| {
            let what = if sheet.charts.iter().any(|ch| ch.id == id) || is_group(sheet, id) {
                DrawingError::unsupported("from", format!("object {id} is not a shape"))
            } else {
                DrawingError::not_found(text)
            };
            fail(CMD, what)
        })?;
        Ok((id, to_drawing(sheet, sh)))
    };
    let ((from, a), (to, b)) = (end_of(&c.from)?, end_of(&c.to)?);
    let (i, j, p0, p1) = c.sites(&a, &b).map_err(|e| fail(CMD, e))?;
    let id = next_object_id(&d.wb)?;
    let mut shape = Shape { id, kind: ShapeKind::Line, line: Color::Theme(4, 0), ..Default::default() };
    let base = to_drawing(sheet, &shape);
    let after = DrawingObject { frame: kition_office_core::connect::line_frame(p0, p1), geometry: dm::Geometry::preset(&c.preset), ..base.clone() };
    write_back(sheet, &mut shape, &base, &after).map_err(|e| fail(CMD, e))?;
    shape.connection = Some(kition_sheet_model::ShapeConnection {
        start: Some(kition_sheet_model::ConnectionEnd { id: from, site: i as u32 }),
        end: Some(kition_sheet_model::ConnectionEnd { id: to, site: j as u32 }),
    });
    edit(s, |cx| {
        cx.sheet_mut(sheet_index)?.shapes.push(shape);
        Ok(())
    })?;
    s.selected_shape = Some(id);
    Ok(json!({ "id": id.to_string() }))
}

/// `drawing.group` (`KitDrawing 1.4`, DR22), one undo step: the shapes, and the shapes of the
/// groups named, gain a new outermost group, its key a fresh object id. Charts are not grouped.
fn group(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "drawing.group";
    let named = kition_office_core::arrange::group_ids(p, 2).map_err(|e| fail(CMD, e))?;
    let d = s.doc()?;
    let sheet_index = d.wb.active_sheet;
    let sheet = d.wb.active().ok_or(EngineError::NoDocument)?;
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    let mut ids: Vec<u32> = Vec::new();
    for text in &named {
        let id: u32 = text.parse().map_err(|_| fail(CMD, DrawingError::not_found(text)))?;
        if sheet.charts.iter().any(|c| c.id == id) {
            return Err(fail(CMD, DrawingError::unsupported("kind", format!("object {id} is a chart: a sheet's groups hold shapes"))));
        }
        let top = if is_group(sheet, id) {
            group_shapes(sheet, id).first().is_some_and(|sh| sh.groups.first() == Some(&id))
        } else {
            match sheet.shapes.iter().find(|sh| sh.id == id) {
                None => return Err(fail(CMD, DrawingError::not_found(text))),
                Some(sh) => sh.groups.is_empty(),
            }
        };
        if !top {
            return Err(fail(CMD, DrawingError::unsupported("ids", format!("object {id} is in a group: ungroup it first"))));
        }
        ids.push(id);
    }
    let key = next_object_id(&d.wb)?;
    edit(s, |cx| {
        let sheet = cx.sheet_mut(sheet_index)?;
        for sh in &mut sheet.shapes {
            if ids.contains(&sh.id) || sh.groups.first().is_some_and(|k| ids.contains(k)) {
                sh.groups.insert(0, key);
            }
        }
        Ok(())
    })?;
    s.selected_shape = Some(key);
    Ok(json!({ "id": key.to_string() }))
}

/// `drawing.ungroup` (DR22), one undo step: each group named loses its key from its shapes;
/// answers its members (shapes and nested groups).
fn ungroup(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "drawing.ungroup";
    let named = kition_office_core::arrange::group_ids(p, 1).map_err(|e| fail(CMD, e))?;
    let d = s.doc()?;
    let sheet_index = d.wb.active_sheet;
    let sheet = d.wb.active().ok_or(EngineError::NoDocument)?;
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    let mut keys = Vec::new();
    let mut members: Vec<String> = Vec::new();
    for text in &named {
        let id: u32 = text.parse().map_err(|_| fail(CMD, DrawingError::not_found(text)))?;
        let g = if is_group(sheet, id) { group_object(sheet, id) } else { None };
        let Some(g) = g else { return Err(fail(CMD, DrawingError::unsupported("kind", format!("object {id} is not a group")))) };
        members.extend(g.members.unwrap_or_default());
        keys.push(id);
    }
    edit(s, |cx| {
        let sheet = cx.sheet_mut(sheet_index)?;
        for sh in &mut sheet.shapes {
            sh.groups.retain(|k| !keys.contains(k));
        }
        Ok(())
    })?;
    s.selected_shape = None;
    Ok(json!({ "ids": members }))
}

/// `drawing.arrange` (`KitDrawing 1.4`, DR21), one undo step. A sheet keeps its charts above
/// its shapes (as it draws and saves them), so shapes move among the shapes and charts among the
/// charts: Bring to Front puts a shape over the other shapes, under the charts.
fn arrange(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "drawing.arrange";
    let (named, order) = kition_office_core::arrange::Order::parse(p).map_err(|e| fail(CMD, e))?;
    let d = s.doc()?;
    let sheet_index = d.wb.active_sheet;
    let sheet = d.wb.active().ok_or(EngineError::NoDocument)?;
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    let shape_stack: Vec<String> = sheet.shapes.iter().map(|sh| sh.id.to_string()).collect();
    let chart_stack: Vec<String> = sheet.charts.iter().map(|c| c.id.to_string()).collect();
    if let Some(missing) = named.iter().find(|id| !shape_stack.contains(id) && !chart_stack.contains(id)) {
        return Err(fail(CMD, DrawingError::not_found(missing)));
    }
    let (shapes, charts): (Vec<String>, Vec<String>) = named.into_iter().partition(|id| shape_stack.contains(id));
    let shape_order = if shapes.is_empty() {
        shape_stack.clone()
    } else {
        kition_office_core::arrange::arrange(&shape_stack, &shapes, order).map_err(|e| fail(CMD, e))?
    };
    let chart_order = if charts.is_empty() {
        chart_stack.clone()
    } else {
        kition_office_core::arrange::arrange(&chart_stack, &charts, order).map_err(|e| fail(CMD, e))?
    };
    if shape_order == shape_stack && chart_order == chart_stack {
        return Ok(json!({}));
    }
    edit(s, |cx| {
        let sheet = cx.sheet_mut(sheet_index)?;
        let rank = |order: &[String], id: u32| order.iter().position(|x| *x == id.to_string()).unwrap_or(usize::MAX);
        sheet.shapes.sort_by_key(|sh| rank(&shape_order, sh.id));
        sheet.charts.sort_by_key(|c| rank(&chart_order, c.id));
        Ok(())
    })?;
    Ok(json!({}))
}

/// A Sheet shape, described: its frame in sheet points from its anchor.
pub fn to_drawing(sheet: &Sheet, sh: &Shape) -> DrawingObject {
    if sh.kind == ShapeKind::Ink {
        let a = &sh.anchor;
        let frame = Xfrm::new(
            sheet.col_left(a.cell.col) + f64::from(a.dx),
            sheet.row_top(a.cell.row) + f64::from(a.dy),
            f64::from(a.width.max(0.0)),
            f64::from(a.height.max(0.0)),
        );
        let mut placement = dm::Placement { kind: dm::PlacementKind::Cell, ..Default::default() };
        placement.extra.insert("mode".into(), json!(mode_name(a.mode)));
        placement.extra.insert("cell".into(), json!(a.cell.to_string()));
        return DrawingObject {
            id: sh.id.to_string(),
            kind: dm::ObjectKind::Ink,
            frame,
            placement,
            ink: Some(vec![super::ink::stroke_of(sheet, sh)]),
            ..Default::default()
        };
    }
    let a = &sh.anchor;
    let frame = Xfrm {
        x: sheet.col_left(a.cell.col) + f64::from(a.dx),
        y: sheet.row_top(a.cell.row) + f64::from(a.dy),
        w: f64::from(a.width.max(0.0)),
        h: f64::from(a.height.max(0.0)),
        rot: (f64::from(sh.rot) / 60_000.0).rem_euclid(360.0),
        flip_h: sh.flip_h,
        flip_v: sh.flip_v,
    };
    let mut placement = dm::Placement { kind: dm::PlacementKind::Cell, ..Default::default() };
    placement.extra.insert("mode".into(), json!(mode_name(a.mode)));
    placement.extra.insert("cell".into(), json!(a.cell.to_string()));
    let text_box = sh.kind == ShapeKind::TextBox;
    DrawingObject {
        id: sh.id.to_string(),
        kind: if text_box { dm::ObjectKind::TextBox } else { dm::ObjectKind::Shape },
        frame,
        placement,
        geometry: geometry_of(sh),
        fill: Some(match (&sh.gradient, sh.fill) {
            (Some(g), _) => dm::Fill::Gradient(dm::Gradient {
                stops: g
                    .stops
                    .iter()
                    .map(|st| dm::GradientStop { pos: f64::from(st.pos.min(100_000)) / 100_000.0, color: color_of(st.color, st.alpha) })
                    .collect(),
                angle: f64::from(g.angle) / 60_000.0,
                path: g.path.clone(),
            }),
            (None, Color::Auto) => dm::Fill::None,
            (None, c) => dm::Fill::Solid(color_of(c, sh.fill_alpha)),
        }),
        stroke: Some(match sh.line {
            Color::Auto => dm::Stroke { none: true, ..Default::default() },
            c => dm::Stroke {
                none: false,
                color: Some(color_of(c, sh.outline.alpha)),
                width: Some(sh.outline.width.map_or(0.75, |e| f64::from(e) / 12_700.0)),
                dash: sh.outline.dash.clone(),
            },
        }),
        text: Some(dm::TextBody { anchor: if text_box { dm::TextAnchor::Top } else { dm::TextAnchor::Middle }, ..rich_text(sh) }),
        chart: None,
        ink: None,
        members: None,
        effects: sh.effects.as_ref().and_then(|e| {
            let out = dm::Effects {
                shadow: e.shadow.as_ref().map(|s| dm::Shadow {
                    color: color_of(s.color, s.alpha),
                    blur: f64::from(s.blur),
                    distance: f64::from(s.distance),
                    angle: f64::from(s.angle),
                }),
                glow: e.glow.as_ref().map(|g| dm::Glow { color: color_of(g.color, g.alpha), size: f64::from(g.size) }),
            };
            (!out.is_empty()).then_some(out)
        }),
        connection: sh.connection.as_ref().map(|c| kition_office_core::connect::Connection {
            start: c.start.map(|e| kition_office_core::connect::End { id: e.id.to_string(), site: e.site as usize }),
            end: c.end.map(|e| kition_office_core::connect::End { id: e.id.to_string(), site: e.site as usize }),
        }),
    }
}

/// Connectors glued to shapes on `sheet` moved to their ends' sites (`KitDrawing 1.4`, DR25):
/// after anything that moves a shape, in the same change. An end whose shape is gone stays where
/// it is, free.
pub fn reroute_connectors(sheet: &mut Sheet) {
    if !sheet.shapes.iter().any(|sh| sh.connection.is_some()) {
        return;
    }
    let objects: Vec<DrawingObject> = sheet.shapes.iter().map(|sh| to_drawing(sheet, sh)).collect();
    let mut moves = Vec::new();
    for (i, sh) in sheet.shapes.iter().enumerate() {
        if sh.connection.is_none() {
            continue;
        }
        let conn = objects[i].connection.clone().unwrap_or_default();
        if let Some(f) = kition_office_core::connect::reroute(&objects[i], &conn, &objects)
            && f != objects[i].frame
        {
            let mut shape = sh.clone();
            let after = DrawingObject { frame: f, ..objects[i].clone() };
            if write_back(sheet, &mut shape, &objects[i], &after).is_ok() {
                moves.push((i, shape));
            }
        }
    }
    for (i, shape) in moves {
        if let Some(slot) = sheet.shapes.get_mut(i) {
            *slot = shape;
        }
    }
}

/// A shape's text as the shared description's (DR8): from its text body when it has one that is
/// still its text (the runs' looks that differ from the default: 11 pt, regular, black), else
/// its plain text.
fn rich_text(sh: &Shape) -> dm::TextBody {
    let kept = sh.text_body.is_some() && sh.text.chars().any(|c| !c.is_whitespace());
    let drawn = crate::shape_text::shape_text(sh);
    let warp = drawn.warp.as_ref().map(|(preset, adj)| dm::Warp { preset: preset.clone(), adjust: adj.iter().cloned().collect() });
    if !kept || drawn.paras.iter().flat_map(|p| &p.runs).map(|r| r.text.as_str()).collect::<String>() != sh.text.replace('\n', "") {
        return dm::TextBody { warp, ..dm::TextBody::plain(&sh.text) };
    }
    let paragraphs = drawn
        .paras
        .iter()
        .map(|p| dm::Paragraph {
            align: match p.align {
                crate::shape_text::TextPlace::Middle => Some(dm::Align::Center),
                crate::shape_text::TextPlace::End => Some(dm::Align::Right),
                crate::shape_text::TextPlace::Start => None,
            },
            runs: p
                .runs
                .iter()
                .map(|r| dm::Run {
                    text: r.text.clone(),
                    font: r.font.clone(),
                    size: (r.size != 11.0).then_some(f64::from(r.size)),
                    bold: r.bold.then_some(true),
                    italic: r.italic.then_some(true),
                    underline: r.underline.then_some(true),
                    color: (r.color != Color::Rgb(0) || r.alpha.is_some()).then(|| color_of(r.color, r.alpha)),
                })
                .collect(),
        })
        .collect();
    dm::TextBody { paragraphs, warp, ..dm::TextBody::plain("") }
}

/// Text for XML.
fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A DrawingML colour element for a sheet colour.
fn color_xml(c: Color, alpha: Option<u32>) -> String {
    let alpha = alpha.map(|a| format!("<a:alpha val=\"{a}\"/>")).unwrap_or_default();
    match c {
        Color::Theme(i, tint) => {
            let k = i32::from(tint.clamp(-1000, 1000));
            let mods = if k < 0 {
                format!("<a:lumMod val=\"{}\"/>", (1000 + k) * 100)
            } else if k > 0 {
                format!("<a:lumMod val=\"{}\"/><a:lumOff val=\"{}\"/>", (1000 - k) * 100, k * 100)
            } else {
                String::new()
            };
            format!("<a:schemeClr val=\"{}\">{mods}{alpha}</a:schemeClr>", SLOTS.get(usize::from(i)).copied().unwrap_or("tx1"))
        }
        Color::Rgb(v) => format!("<a:srgbClr val=\"{:06X}\">{alpha}</a:srgbClr>", v & 0xFF_FFFF),
        _ => format!("<a:srgbClr val=\"000000\">{alpha}</a:srgbClr>"),
    }
}

/// The text body (`xdr:txBody`) of the shared description's text: a paragraph per paragraph, a
/// run per run, with their alignment and looks; drawn and saved as the shape's text.
fn text_body_xml(t: &dm::TextBody, text_box: bool) -> std::result::Result<String, DrawingError> {
    let anchor = if text_box { "t" } else { "ctr" };
    let body_pr = format!("vertOverflow=\"clip\" wrap=\"square\" rtlCol=\"0\" anchor=\"{anchor}\"");
    let mut x = match t.warp.as_ref().filter(|w| w.preset.chars().all(|c| c.is_ascii_alphanumeric())) {
        Some(w) => {
            let gds: String = w
                .adjust
                .iter()
                .filter(|(n, v)| n.chars().all(|c| c.is_ascii_alphanumeric()) && v.is_finite())
                .map(|(n, v)| format!("<a:gd name=\"{n}\" fmla=\"val {}\"/>", v.round() as i64))
                .collect();
            format!(
                "<xdr:txBody><a:bodyPr {body_pr}><a:prstTxWarp prst=\"{}\"><a:avLst>{gds}</a:avLst></a:prstTxWarp></a:bodyPr><a:lstStyle/>",
                w.preset
            )
        }
        None => format!("<xdr:txBody><a:bodyPr {body_pr}/><a:lstStyle/>"),
    };
    for p in &t.paragraphs {
        x.push_str("<a:p>");
        if let Some(a) = p.align {
            let algn = match a {
                dm::Align::Left => "l",
                dm::Align::Center => "ctr",
                dm::Align::Right => "r",
                dm::Align::Justify => "just",
            };
            x.push_str(&format!("<a:pPr algn=\"{algn}\"/>"));
        }
        for r in &p.runs {
            if r.text.is_empty() {
                continue;
            }
            let mut attrs = String::from(" lang=\"en-US\"");
            if let Some(sz) = r.size {
                attrs.push_str(&format!(" sz=\"{}\"", (sz.clamp(1.0, 4000.0) * 100.0).round() as i64));
            }
            if r.bold == Some(true) {
                attrs.push_str(" b=\"1\"");
            }
            if r.italic == Some(true) {
                attrs.push_str(" i=\"1\"");
            }
            if r.underline == Some(true) {
                attrs.push_str(" u=\"sng\"");
            }
            let mut inner = String::new();
            if let Some(c) = &r.color {
                let (c, alpha) = color_back(c)?;
                inner.push_str(&format!("<a:solidFill>{}</a:solidFill>", color_xml(c, alpha)));
            }
            if let Some(f) = &r.font {
                inner.push_str(&format!("<a:latin typeface=\"{}\"/>", esc(f)));
            }
            x.push_str(&format!("<a:r><a:rPr{attrs}>{inner}</a:rPr><a:t>{}</a:t></a:r>", esc(&r.text)));
        }
        x.push_str("</a:p>");
    }
    x.push_str("</xdr:txBody>");
    Ok(x)
}

pub(super) fn mode_name(m: AnchorMode) -> &'static str {
    match m {
        AnchorMode::MoveAndSize => "moveAndSize",
        AnchorMode::MoveOnly => "moveOnly",
        AnchorMode::Absolute => "absolute",
    }
}

pub(super) fn anchor_mode(name: &str) -> Option<AnchorMode> {
    match name {
        "moveAndSize" => Some(AnchorMode::MoveAndSize),
        "moveOnly" => Some(AnchorMode::MoveOnly),
        "absolute" => Some(AnchorMode::Absolute),
        _ => None,
    }
}

/// The preset a shape's kind draws.
fn kind_preset(kind: ShapeKind) -> &'static str {
    match kind {
        ShapeKind::RoundedRectangle => "roundRect",
        ShapeKind::Ellipse => "ellipse",
        ShapeKind::Triangle => "triangle",
        ShapeKind::Line | ShapeKind::Arrow => "line",
        _ => "rect",
    }
}

fn geometry_of(sh: &Shape) -> dm::Geometry {
    if let Some(g) = sh.geom.as_ref().filter(|g| !g.paths.is_empty()) {
        return dm::Geometry::Custom {
            paths: g
                .paths
                .iter()
                .map(|p| dm::CustomPath { w: p.w, h: p.h, d: p.d.clone(), filled: p.fill != PathFill::None, stroked: p.stroke })
                .collect(),
        };
    }
    match &sh.preset {
        Some(p) => dm::Geometry::Preset {
            preset: p.name.clone(),
            adjust: p
                .adj
                .iter()
                .filter_map(|(name, formula)| {
                    Some((name.clone(), formula.strip_prefix("val ")?.trim().parse::<f64>().ok().filter(|v| v.is_finite())?))
                })
                .collect(),
        },
        None => dm::Geometry::preset(kind_preset(sh.kind)),
    }
}

/// Theme indexes as Sheet keeps them, by DrawingML slot.
const SLOTS: [&str; 12] = ["bg1", "tx1", "bg2", "tx2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];

pub(super) fn color_of(c: Color, alpha: Option<u32>) -> dm::Color {
    let mut out = match c {
        Color::Rgb(v) => dm::Color::rgb(dm::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)),
        Color::Theme(i, tint) => {
            let mut t = dm::Color::theme(SLOTS.get(usize::from(i)).copied().unwrap_or("tx1"));
            // A tint in thousandths: darker is a luminance factor, lighter a factor and an offset.
            let k = i32::from(tint.clamp(-1000, 1000));
            if k < 0 {
                t.mods.push(("lumMod".into(), (1000 + k) * 100));
            } else if k > 0 {
                t.mods.push(("lumMod".into(), (1000 - k) * 100));
                t.mods.push(("lumOff".into(), k * 100));
            }
            t
        }
        Color::Auto => dm::Color::theme("tx1"),
    };
    out.alpha = alpha.filter(|a| *a < 100_000).map(|a| f64::from(a) / 100_000.0);
    out
}

pub(super) fn color_back(c: &dm::Color) -> std::result::Result<(Color, Option<u32>), DrawingError> {
    let color = match &c.base {
        dm::ColorBase::Rgb { rgb } => {
            if !c.mods.is_empty() {
                return Err(DrawingError::unsupported("color.mods", "a sheet colour given as RGB takes no transforms"));
            }
            Color::rgb(rgb.0, rgb.1, rgb.2)
        }
        dm::ColorBase::Theme { theme } => {
            let slot = match theme.as_str() {
                "lt1" => "bg1",
                "dk1" => "tx1",
                "lt2" => "bg2",
                "dk2" => "tx2",
                other => other,
            };
            let i = SLOTS.iter().position(|s| *s == slot).ok_or_else(|| DrawingError::invalid(format!("unknown theme color `{theme}`")))?;
            let modv = |n: &str| c.mods.iter().find(|(m, _)| m == n).map(|(_, v)| *v);
            if c.mods.iter().any(|(m, _)| m != "lumMod" && m != "lumOff") {
                return Err(DrawingError::unsupported("color.mods", "a sheet theme colour takes a tint (lumMod, lumOff) only"));
            }
            let tint = match (modv("lumMod"), modv("lumOff")) {
                (None, None) => 0,
                (Some(m), None) => (m / 100 - 1000).clamp(-1000, 0),
                (Some(_), Some(o)) | (None, Some(o)) => (o / 100).clamp(0, 1000),
            };
            Color::Theme(u8::try_from(i).unwrap_or(1), i16::try_from(tint).unwrap_or(0))
        }
    };
    let alpha = c.alpha.map(|a| (a.clamp(0.0, 1.0) * 100_000.0).round() as u32).filter(|a| *a < 100_000);
    Ok((color, alpha))
}

/// Writes the members that differ between `before` and `after` into `sh`.
fn write_back(sheet: &Sheet, sh: &mut Shape, before: &DrawingObject, after: &DrawingObject) -> std::result::Result<(), DrawingError> {
    // Ink moves; its points are its own, so it neither resizes nor turns, and its look is its pen's.
    if sh.kind == ShapeKind::Ink {
        let (a, b) = (&after.frame, &before.frame);
        if (a.w, a.h, a.rot, a.flip_h, a.flip_v) != (b.w, b.h, b.rot, b.flip_h, b.flip_v) {
            return Err(DrawingError::unsupported("frame", "ink moves; it does not resize or turn"));
        }
        for (property, changed) in [
            ("fill", after.fill != before.fill),
            ("stroke", after.stroke != before.stroke),
            ("geometry", after.geometry != before.geometry),
            ("text", after.text != before.text),
        ] {
            if changed {
                return Err(DrawingError::unsupported(property, "ink keeps its pen's look"));
            }
        }
    }
    // A connector's glued ends (`drawing.reconnect`): to shapes on the sheet.
    if after.connection != before.connection {
        let end = |e: &Option<kition_office_core::connect::End>| -> std::result::Result<Option<kition_sheet_model::ConnectionEnd>, DrawingError> {
            let Some(e) = e else { return Ok(None) };
            let id = e.id.parse::<u32>().ok().filter(|id| sheet.shapes.iter().any(|s| s.id == *id)).ok_or_else(|| DrawingError::not_found(&e.id))?;
            Ok(Some(kition_sheet_model::ConnectionEnd { id, site: u32::try_from(e.site).unwrap_or(0) }))
        };
        let c = after.connection.clone().unwrap_or_default();
        let (start, end) = (end(&c.start)?, end(&c.end)?);
        sh.connection = (start.is_some() || end.is_some()).then_some(kition_sheet_model::ShapeConnection { start, end });
    }
    if after.frame != before.frame {
        let f = &after.frame;
        kition_office_core::ops::check_frame(f)?;
        let (x, y) = (f.x.max(0.0), f.y.max(0.0));
        let cell = CellRef::new(sheet.row_at(y), sheet.col_at(x));
        sh.anchor = Anchor {
            cell,
            dx: (x - sheet.col_left(cell.col)) as f32,
            dy: (y - sheet.row_top(cell.row)) as f32,
            width: f.w as f32,
            height: f.h as f32,
            ..sh.anchor
        };
        sh.rot = (f.rot.rem_euclid(360.0) * 60_000.0).round() as i32;
        sh.flip_h = f.flip_h;
        sh.flip_v = f.flip_v;
    }
    if after.placement != before.placement
        && let Some(mode) = after.placement.extra.get("mode").and_then(Json::as_str)
    {
        sh.anchor.mode = anchor_mode(mode).ok_or_else(|| DrawingError::invalid("mode is moveAndSize, moveOnly or absolute"))?;
    }
    if after.geometry != before.geometry {
        match &after.geometry {
            dm::Geometry::Preset { preset, adjust } => {
                sh.geom = None;
                let own = kind_preset(sh.kind) == preset && adjust.is_empty() && sh.kind != ShapeKind::Arrow;
                sh.preset = (!own).then(|| PresetGeom {
                    name: preset.clone(),
                    adj: adjust.iter().map(|(n, v)| (n.clone(), format!("val {}", v.round() as i64))).collect(),
                });
            }
            dm::Geometry::Custom { paths } => {
                sh.preset = None;
                sh.geom = Some(CustomGeom {
                    paths: paths
                        .iter()
                        .map(|p| CustomPath {
                            w: p.w,
                            h: p.h,
                            d: p.d.clone(),
                            fill: if p.filled { PathFill::Norm } else { PathFill::None },
                            stroke: p.stroked,
                        })
                        .collect(),
                    source: None,
                });
            }
        }
    }
    if after.effects != before.effects {
        let e = after.effects.clone().unwrap_or_default();
        let shadow = e
            .shadow
            .map(|s| {
                let (color, alpha) = color_back(&s.color)?;
                Ok::<_, DrawingError>(kition_sheet_model::ShapeShadow {
                    color,
                    alpha,
                    blur: s.blur as f32,
                    distance: s.distance as f32,
                    angle: s.angle as f32,
                })
            })
            .transpose()?;
        let glow = e
            .glow
            .map(|g| {
                let (color, alpha) = color_back(&g.color)?;
                Ok::<_, DrawingError>(kition_sheet_model::ShapeGlow { color, alpha, size: g.size as f32 })
            })
            .transpose()?;
        sh.effects = (shadow.is_some() || glow.is_some()).then_some(kition_sheet_model::ShapeEffects { shadow, glow });
    }
    if after.fill != before.fill {
        match &after.fill {
            Some(dm::Fill::None) | None => {
                sh.fill = Color::Auto;
                sh.fill_alpha = None;
                sh.gradient = None;
            }
            Some(dm::Fill::Solid(c)) => {
                (sh.fill, sh.fill_alpha) = color_back(c)?;
                sh.gradient = None;
            }
            Some(dm::Fill::Gradient(g)) => {
                let stops = g
                    .stops
                    .iter()
                    .map(|st| {
                        let (color, alpha) = color_back(&st.color)?;
                        Ok(kition_sheet_model::GradientStop { pos: (st.pos * 100_000.0).round() as u32, color, alpha })
                    })
                    .collect::<std::result::Result<Vec<_>, DrawingError>>()?;
                // The first colour stands in for readers that do not know gradients.
                if let Some(first) = stops.first() {
                    (sh.fill, sh.fill_alpha) = (first.color, first.alpha);
                }
                sh.gradient = Some(kition_sheet_model::Gradient {
                    stops,
                    angle: (g.angle * 60_000.0).round() as i32,
                    path: g.path.clone(),
                    rot_with_shape: Some(true),
                    ..Default::default()
                });
            }
            Some(dm::Fill::Other(kind)) => return Err(DrawingError::unsupported("fill", format!("a {kind} fill is not set by command"))),
        }
    }
    if after.stroke != before.stroke
        && let Some(st) = &after.stroke
    {
        if st.none {
            sh.line = Color::Auto;
        } else {
            if let Some(c) = &st.color {
                (sh.line, sh.outline.alpha) = color_back(c)?;
            } else if sh.line == Color::Auto {
                sh.line = Color::Theme(4, -250);
            }
            if let Some(w) = st.width {
                sh.outline.width = u32::try_from(pt_to_emu(w)).ok();
            }
            if st.dash.is_some() {
                sh.outline.dash.clone_from(&st.dash);
            }
        }
    }
    if after.text != before.text
        && let Some(t) = &after.text
    {
        // A sheet shape keeps plain text and, for its looks, a text body that is drawn and saved
        // while it is still that text: formatted text writes one.
        // A warp (art text, DR26) lives in the text body too: setting or removing one writes it.
        let warp_changed = t.warp != before.text.as_ref().and_then(|b| b.warp.clone());
        if let Some(w) = &t.warp
            && !kition_office_core::warp::is_warp(&w.preset)
        {
            return Err(DrawingError::unsupported("warp", format!("no text warp `{}`", w.preset)));
        }
        let formatted = warp_changed
            || t.warp.is_some()
            || t.paragraphs.iter().any(|p| p.align.is_some())
            || t.paragraphs.iter().flat_map(|p| &p.runs).any(|r| {
                r.font.is_some()
                    || r.size.is_some()
                    || r.bold == Some(true)
                    || r.italic == Some(true)
                    || r.underline == Some(true)
                    || r.color.is_some()
            });
        let text = t.text();
        if text.chars().nth(MAX_TEXT).is_some() {
            return Err(DrawingError::invalid("text is longer than 32,767 characters"));
        }
        // Plain text (typed in the shape) keeps the body's looks: the characters it keeps keep
        // their runs, typed ones take the look where they were typed.
        if formatted {
            sh.text_body = Some(text_body_xml(t, sh.kind == ShapeKind::TextBox)?);
        } else if text != sh.text
            && let Some(body) = sh.text_body.as_deref().and_then(|b| kition_sheet_xlsx::retext_body(b, &text))
        {
            sh.text_body = Some(body);
        }
        sh.text = text;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.new_workbook();
        s
    }

    fn obj(s: &mut Session, id: &str) -> DrawingObject {
        let d = s.execute("drawing.describe", json!({ "ids": [id] })).unwrap();
        serde_json::from_value(d["objects"][0].clone()).unwrap()
    }

    #[test]
    fn a_new_shape_has_white_text_and_a_text_box_black_as_in_excel() {
        let mut s = session();
        s.execute("drawing.insert", json!({ "text": "Shape" })).unwrap();
        s.execute("drawing.insert", json!({ "kind": "textBox", "text": "Box", "frame": { "x": 300, "y": 40, "w": 120, "h": 40 } })).unwrap();
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "d.xlsx", "base64": b64})).unwrap();
        let shapes = &back.doc().unwrap().wb.sheet(0).unwrap().shapes;
        let colors: Vec<Color> = shapes.iter().map(|sh| crate::shape_text::shape_text(sh).paras[0].runs[0].color).collect();
        assert_eq!(colors, [Color::Theme(0, 0), Color::Rgb(0)]);
    }

    #[test]
    fn insert_edit_undo_and_reopen_as_xlsx() {
        let mut s = session();
        let id = s
            .execute("drawing.insert", json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 100, "y": 50, "w": 120, "h": 60 }, "fill": { "color": "#5645D4" }, "stroke": { "color": "accent2", "width": 2 }, "text": "Q3" }),
            )
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let o = obj(&mut s, &id);
        assert_eq!((o.frame.x, o.frame.y, o.frame.w), (100.0, 50.0, 120.0));
        assert_eq!(o.geometry, dm::Geometry::preset("ellipse"));
        assert_eq!(o.fill, Some(dm::Fill::Solid(dm::Color::rgb(dm::Rgb(0x56, 0x45, 0xD4)))));
        assert_eq!(o.placement.kind, dm::PlacementKind::Cell);
        // Left out, the fill and pen are a new shape's: the theme's accent.
        let plain = s.execute("drawing.insert", json!({})).unwrap()["id"].as_str().unwrap().to_string();
        let p = obj(&mut s, &plain);
        assert_eq!(p.fill, Some(dm::Fill::Solid(dm::Color::theme("accent1"))));
        assert!(!p.stroke.unwrap().none);
        s.execute("drawing.delete", json!({ "ids": [plain] })).unwrap();

        s.execute("drawing.transform", json!({ "ids": [id], "dx": 30, "dy": 10, "rotateBy": 45 })).unwrap();
        s.execute(
            "drawing.format",
            json!({ "ids": [id], "fill": { "color": { "theme": "accent1", "mods": [["lumMod", 75000]] } }, "geometry": { "preset": "star5" } }),
        )
        .unwrap();
        s.execute("drawing.text", json!({ "id": id, "text": "Q4" })).unwrap();
        let o = obj(&mut s, &id);
        assert!((o.frame.x - 130.0).abs() < 1e-3 && (o.frame.y - 60.0).abs() < 1e-3 && (o.frame.rot - 45.0).abs() < 1e-9, "{:?}", o.frame);
        assert_eq!(o.geometry, dm::Geometry::preset("star5"));
        assert_eq!(o.fill, Some(dm::Fill::Solid(dm::Color { mods: vec![("lumMod".into(), 75000)], ..dm::Color::theme("accent1") })));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).text.unwrap().text(), "Q3");
        s.execute("edit.redo", json!({})).unwrap();

        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "d.xlsx", "base64": b64})).unwrap();
        let o = obj(&mut back, &id);
        assert!((o.frame.x - 130.0).abs() < 0.1 && (o.frame.rot - 45.0).abs() < 1e-6, "{:?}", o.frame);
        assert_eq!(o.geometry, dm::Geometry::preset("star5"));
        assert_eq!(o.text.unwrap().text(), "Q4");
        back.execute("drawing.format", json!({ "ids": [id], "stroke": { "none": true } })).unwrap();
        assert!(obj(&mut back, &id).stroke.unwrap().none);
    }

    #[test]
    fn charts_show_their_cells_change_by_property_undo_and_reopen_as_xlsx() {
        let mut s = session();
        // The chart's cells, and cells next to them it does not show.
        for (cell, input) in [
            ("A1", ""),
            ("B1", "East"),
            ("C1", "West"),
            ("A2", "Q1"),
            ("B2", "10"),
            ("C2", "4"),
            ("A3", "Q2"),
            ("B3", "20"),
            ("C3", "6"),
            ("A4", "Q3"),
            ("B4", "99"),
            ("D1", "North"),
            ("D2", "7"),
        ] {
            s.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
        }
        let id = s
            .execute("drawing.insert", json!({ "kind": "chart", "range": "A1:C3", "chart": { "kind": "line", "title": "Sales", "legend": "top" }, "frame": { "x": 300, "y": 20, "w": 360, "h": 220 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let o = obj(&mut s, &id);
        assert_eq!((o.kind, o.frame.x, o.frame.w), (dm::ObjectKind::Chart, 300.0, 360.0));
        let c = o.chart.unwrap();
        assert_eq!(
            (c.kind, c.title.as_deref(), c.legend),
            (kition_office_core::chart::ChartKind::Line, Some("Sales"), Some(kition_office_core::chart::LegendPos::Top))
        );
        assert_eq!(c.categories, ["Q1", "Q2"]);
        assert_eq!((c.series[0].name.as_str(), c.series[0].values.clone()), ("East", vec![Some(10.0), Some(20.0)]));
        // The chart shows the cells: change one, and the description follows.
        s.execute("cell.set", json!({ "cell": "B3", "input": "25" })).unwrap();
        assert_eq!(obj(&mut s, &id).chart.unwrap().series[0].values[1], Some(25.0));

        s.execute("drawing.chart", json!({ "id": id, "kind": "column", "dataLabels": true, "yTitle": "USD", "series": [{ "name": "East", "values": [10, 25], "color": "#FF0000" }, { "name": "West", "values": [4, 6] }] })).unwrap();
        let c = obj(&mut s, &id).chart.unwrap();
        assert_eq!((c.kind, c.data_labels, c.y_title.as_deref()), (kition_office_core::chart::ChartKind::Column, true, Some("USD")));
        assert_eq!(c.series[0].color, Some(dm::Color::rgb(dm::Rgb(255, 0, 0))));
        // Its data are the cells': a change to them is refused, and changes nothing.
        let e = s.execute("drawing.chart", json!({ "id": id, "title": "No", "categories": ["X", "Y"] })).unwrap_err();
        assert!(e.to_string().contains("unsupported") && e.to_string().contains("chart.categories"), "{e}");
        assert_eq!(obj(&mut s, &id).chart.unwrap().title.as_deref(), Some("Sales"));
        let e = s.execute("drawing.chart", json!({ "id": id, "series": [{ "name": "East", "values": [1, 2] }] })).unwrap_err();
        assert!(e.to_string().contains("chart.series"), "{e}");
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).chart.unwrap().kind, kition_office_core::chart::ChartKind::Line);
        s.execute("edit.redo", json!({})).unwrap();
        // It moves and resizes; it does not turn.
        s.execute("drawing.transform", json!({ "ids": [id], "dx": 40 })).unwrap();
        assert!((obj(&mut s, &id).frame.x - 340.0).abs() < 0.01);
        assert!(s.execute("drawing.transform", json!({ "ids": [id], "rotateBy": 90 })).unwrap_err().to_string().contains("unsupported"));

        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "c.xlsx", "base64": b64})).unwrap();
        let objects = back.execute("drawing.describe", json!({})).unwrap()["objects"].as_array().unwrap().clone();
        let o: DrawingObject = serde_json::from_value(objects.iter().find(|o| o["kind"] == "chart").unwrap().clone()).unwrap();
        let c = o.chart.unwrap();
        assert_eq!(
            (c.kind, c.data_labels, c.y_title.as_deref(), c.series[0].values.clone()),
            (kition_office_core::chart::ChartKind::Column, true, Some("USD"), vec![Some(10.0), Some(25.0)])
        );
        back.execute("drawing.chart", json!({ "id": o.id, "legend": null })).unwrap();
        assert_eq!(obj(&mut back, &o.id).chart.unwrap().legend, None);
        // A series' smoothing is the chart's.
        back.execute("drawing.chart", json!({ "id": o.id, "kind": "line", "seriesFormat": [{ "index": 1, "smooth": true }] })).unwrap();
        assert!(obj(&mut back, &o.id).chart.unwrap().series[1].smooth);
        assert!(back.doc().unwrap().wb.sheets[0].charts[0].series[1].smooth);
        // Axes, label positions, line widths and markers, in one step each, undone, and saved.
        back.execute(
            "drawing.chart",
            json!({ "id": o.id, "yAxis": { "min": 0, "max": 50, "major": 10, "numberFormat": "0.0" }, "xAxis": { "hidden": true }, "dataLabelPosition": "b", "seriesFormat": [{ "index": 0, "marker": "square", "lineWidth": 4 }] }),
        )
        .unwrap();
        let set = |s: &mut Session| obj(s, &o.id).chart.unwrap();
        let c = set(&mut back);
        assert_eq!((c.y_axis.max, c.y_axis.major, c.y_axis.number_format.as_deref(), c.x_axis.hidden), (Some(50.0), Some(10.0), Some("0.0"), true));
        assert_eq!((c.data_label_position.as_deref(), c.series[0].marker.as_deref(), c.series[0].line_width), (Some("b"), Some("square"), Some(4.0)));
        back.execute("edit.undo", json!({})).unwrap();
        assert_eq!(set(&mut back).y_axis.max, None);
        back.execute("edit.redo", json!({})).unwrap();
        let b64 = back.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut again = Session::new();
        again.execute("file.open", json!({"name": "c.xlsx", "base64": b64})).unwrap();
        let r = super::objects(&again.doc().unwrap().wb).into_iter().find_map(|o| o.chart).unwrap();
        assert_eq!(
            (r.y_axis, r.x_axis.hidden, r.data_label_position, r.series[0].marker.clone()),
            (c.y_axis, true, c.data_label_position, c.series[0].marker.clone())
        );
        // A position the type does not take, and bounds on a category axis, are refused.
        for (change, says) in [(json!({ "dataLabelPosition": "outEnd" }), "invalid"), (json!({ "xAxis": { "min": 1 } }), "unsupported")] {
            let mut p = change.clone();
            p["id"] = json!(o.id);
            let e = back.execute("drawing.chart", p).unwrap_err();
            assert!(e.to_string().contains(says), "{change}: {e}");
        }
        // A type that does not take the position goes back to its own.
        back.execute("drawing.chart", json!({ "id": o.id, "kind": "column" })).unwrap();
        assert_eq!(set(&mut back).data_label_position, None);
        back.execute("drawing.delete", json!({ "ids": [o.id] })).unwrap();
        assert!(back.execute("drawing.describe", json!({})).unwrap()["objects"].as_array().unwrap().iter().all(|o| o["kind"] != "chart"));
    }

    #[test]
    fn shapes_and_charts_move_in_their_stacks_in_one_step_and_reopen() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str, x: f64| -> String {
            s.execute("drawing.insert", json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": 40, "w": 80, "h": 60 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b, c) = (ins(&mut s, "rect", 40.0), ins(&mut s, "ellipse", 60.0), ins(&mut s, "triangle", 80.0));
        s.execute("cell.set", json!({ "cell": "A1", "input": "1" })).unwrap();
        let chart =
            s.execute("drawing.insert", json!({ "kind": "chart", "range": "A1", "frame": { "x": 300, "y": 40, "w": 200, "h": 120 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
        let order = |s: &mut Session| -> Vec<String> { objects(&s.doc().unwrap().wb).into_iter().map(|o| o.id).collect() };
        assert_eq!(order(&mut s), [a.clone(), b.clone(), c.clone(), chart.clone()]);
        // Bring to Front: over the other shapes, under the chart.
        s.execute("drawing.arrange", json!({ "ids": [a], "order": "front" })).unwrap();
        assert_eq!(order(&mut s), [b.clone(), c.clone(), a.clone(), chart.clone()]);
        s.execute("drawing.arrange", json!({ "ids": [c, chart], "order": "back" })).unwrap();
        assert_eq!(order(&mut s), [c.clone(), b.clone(), a.clone(), chart.clone()]);
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(order(&mut s), [b.clone(), c.clone(), a.clone(), chart.clone()]);
        assert!(s.execute("drawing.arrange", json!({ "ids": ["999"], "order": "front" })).is_err());
        // Saved bottom to top as it is drawn: the rectangle over the triangle, the chart over all.
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "z.xlsx", "base64": b64})).unwrap();
        let presets: Vec<String> = objects(&back.doc().unwrap().wb)
            .iter()
            .map(|o| match &o.geometry {
                dm::Geometry::Preset { preset, .. } if o.kind != dm::ObjectKind::Chart => preset.clone(),
                _ => "chart".into(),
            })
            .collect();
        assert_eq!(presets, ["ellipse", "triangle", "rect", "chart"]);
    }

    #[test]
    fn gradient_fills_format_describe_undo_and_reopen() {
        let mut s = session();
        let id = s.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": 60, "y": 40, "w": 160, "h": 80 } })).unwrap()
            ["id"]
            .as_str()
            .unwrap()
            .to_string();
        let gradient = json!({ "gradient": { "stops": [{ "pos": 0, "color": "#204080" }, { "pos": 1, "color": "#C7CFDF" }], "angle": 90 } });
        s.execute("drawing.format", json!({ "ids": [id], "fill": gradient })).unwrap();
        let expected: dm::Fill = serde_json::from_value(gradient).unwrap();
        assert_eq!(obj(&mut s, &id).fill.as_ref(), Some(&expected));
        s.execute("drawing.format", json!({ "ids": [id], "fill": { "gradient": { "stops": [{ "pos": 0, "color": "#FFFFFF" }, { "pos": 1, "color": "#000000" }], "path": "circle" } } })).unwrap();
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).fill.as_ref(), Some(&expected));
        // Saved as .xlsx and reopened.
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "g.xlsx", "base64": b64})).unwrap();
        let o = objects(&back.doc().unwrap().wb).into_iter().find(|o| o.geometry == dm::Geometry::preset("rect")).unwrap();
        assert_eq!(o.fill, Some(expected));
    }

    #[test]
    fn shadows_and_glows_format_describe_draw_and_reopen() {
        let mut s = session();
        let id = s
            .execute(
                "drawing.insert",
                json!({ "geometry": { "preset": "rect" }, "frame": { "x": 60, "y": 40, "w": 160, "h": 80 }, "fill": { "color": "#204080" } }),
            )
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        s.execute(
            "drawing.format",
            json!({ "ids": [id], "effects": { "shadow": { "color": { "rgb": "#000000", "alpha": 0.4 }, "blur": 4, "distance": 3, "angle": 45 }, "glow": { "color": { "rgb": "#5645D4", "alpha": 0.6 }, "size": 6 } } }),
        )
        .unwrap();
        let e = obj(&mut s, &id).effects.unwrap();
        let shadow = e.shadow.clone().unwrap();
        assert_eq!((shadow.blur, shadow.distance, shadow.angle, shadow.color.alpha), (4.0, 3.0, 45.0, Some(0.4)));
        assert_eq!(e.glow.as_ref().map(|g| (g.size, g.color.alpha)), Some((6.0, Some(0.6))));
        // Drawn under the shape: the glow's pens and the shadow's copies come before its fill.
        let sheet = s.doc().unwrap().wb.active().unwrap().clone();
        let shape = sheet.shapes.iter().find(|x| x.id.to_string() == id).unwrap();
        let prims = kition_sheet_chart::render_shape(shape, &s.doc().unwrap().wb.theme, 160.0, 80.0);
        let fill_at = prims
            .iter()
            .position(|p| matches!(p, kition_sheet_chart::Prim::Path { paint: kition_sheet_chart::Paint::Solid([0x20, 0x40, 0x80, 255]), .. }))
            .unwrap();
        assert!(fill_at > 8, "shadow and glow first: {fill_at}");
        s.execute("drawing.format", json!({ "ids": [id], "effects": { "shadow": null } })).unwrap();
        assert!(obj(&mut s, &id).effects.is_some_and(|e| e.shadow.is_none() && e.glow.is_some()));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).effects, Some(e.clone()));
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "e.xlsx", "base64": b64})).unwrap();
        let o = objects(&back.doc().unwrap().wb).into_iter().find(|o| o.geometry == dm::Geometry::preset("rect")).unwrap();
        assert_eq!(o.effects, Some(e));
    }

    #[test]
    fn shape_text_takes_runs_and_alignment_draws_and_reopens() {
        let mut s = session();
        let id = s
            .execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": 60, "y": 40, "w": 200, "h": 100 }, "text": "x" }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let paragraphs = json!([
            { "align": "center", "runs": [{ "text": "Q3 " }, { "text": "results", "bold": true, "size": 18, "color": "#C00000" }] },
            { "runs": [{ "text": "up 12%", "italic": true, "underline": true, "font": "Arial" }] }
        ]);
        s.execute("drawing.text", json!({ "id": id, "paragraphs": paragraphs })).unwrap();
        let check = |t: dm::TextBody| {
            assert_eq!(t.text(), "Q3 results\nup 12%");
            assert_eq!(t.paragraphs[0].align, Some(dm::Align::Center));
            let r = &t.paragraphs[0].runs[1];
            assert_eq!((r.bold, r.size, r.color.clone()), (Some(true), Some(18.0), Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0)))));
            let r = &t.paragraphs[1].runs[0];
            assert_eq!((r.italic, r.underline, r.font.as_deref()), (Some(true), Some(true), Some("Arial")));
        };
        check(obj(&mut s, &id).text.unwrap());
        // It is drawn so: the bold run at 18 pt, in the middle.
        let sheet = s.doc().unwrap().wb.active().unwrap().clone();
        let shape = sheet.shapes.iter().find(|x| x.id.to_string() == id).unwrap();
        let drawn = crate::shape_text::shape_text(shape);
        assert_eq!(
            (drawn.paras[0].align, drawn.paras[0].runs[1].size, drawn.paras[0].runs[1].bold),
            (crate::shape_text::TextPlace::Middle, 18.0, true)
        );
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).text.unwrap().text(), "x");
        s.execute("edit.redo", json!({})).unwrap();
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "t.xlsx", "base64": b64})).unwrap();
        let o = objects(&back.doc().unwrap().wb).into_iter().find(|o| o.geometry == dm::Geometry::preset("rect")).unwrap();
        check(o.text.unwrap());
    }

    #[test]
    fn shapes_group_move_and_resize_together_ungroup_and_reopen() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str, x: f64| -> String {
            s.execute("drawing.insert", json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": 40, "w": 80, "h": 60 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b, c) = (ins(&mut s, "rect", 60.0), ins(&mut s, "ellipse", 200.0), ins(&mut s, "triangle", 400.0));
        let g = s.execute("drawing.group", json!({ "ids": [a, b] })).unwrap()["id"].as_str().unwrap().to_string();
        let group = obj(&mut s, &g);
        assert_eq!(group.kind, dm::ObjectKind::Group);
        assert_eq!(group.members.as_deref(), Some(&[a.clone(), b.clone()][..]));
        assert!((group.frame.x - 60.0).abs() < 0.5 && (group.frame.w - 220.0).abs() < 0.5, "{:?}", group.frame);
        // The sheet's objects: the group in its members' place, and the triangle.
        let ids: Vec<String> = objects(&s.doc().unwrap().wb).into_iter().map(|o| o.id).collect();
        assert_eq!(ids, [g.clone(), c.clone()]);
        assert_eq!(obj(&mut s, &a).frame.x, 60.0, "a member is still described");
        // Moving and sizing the group carries its members.
        s.execute("drawing.transform", json!({ "ids": [g], "dx": 30 })).unwrap();
        assert!((obj(&mut s, &b).frame.x - 230.0).abs() < 0.5);
        let f = obj(&mut s, &g).frame;
        s.execute("drawing.transform", json!({ "ids": [g], "frame": { "x": f.x, "y": f.y, "w": f.w * 2.0, "h": f.h } })).unwrap();
        let m = obj(&mut s, &b).frame;
        assert!((m.x - (f.x + 140.0 * 2.0)).abs() < 1.0 && (m.w - 160.0).abs() < 1.0, "{m:?}");
        // Refusals: a turn, a look, a member moved alone, a member grouped again.
        assert!(s.execute("drawing.transform", json!({ "ids": [g], "rotateBy": 90 })).unwrap_err().to_string().contains("unsupported"));
        assert!(s.execute("drawing.format", json!({ "ids": [g], "fill": { "color": "#FF0000" } })).unwrap_err().to_string().contains("unsupported"));
        assert!(s.execute("drawing.transform", json!({ "ids": [a], "dx": 5 })).unwrap_err().to_string().contains("unsupported"));
        assert!(s.execute("drawing.group", json!({ "ids": [a, c] })).unwrap_err().to_string().contains("unsupported"));
        // Saved and reopened, the group is there with its two members.
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "g.xlsx", "base64": b64})).unwrap();
        let reopened: Vec<DrawingObject> = objects(&back.doc().unwrap().wb).into_iter().filter(|o| o.kind == dm::ObjectKind::Group).collect();
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened[0].members.as_ref().map(Vec::len), Some(2));
        // Ungroup: the members again, one step.
        let r = s.execute("drawing.ungroup", json!({ "ids": [g] })).unwrap();
        assert_eq!(r["ids"], json!([a, b]));
        assert_eq!(objects(&s.doc().unwrap().wb).len(), 3);
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &g).kind, dm::ObjectKind::Group);
        // Deleting the group deletes its members.
        s.execute("drawing.delete", json!({ "ids": [g] })).unwrap();
        let left: Vec<String> = objects(&s.doc().unwrap().wb).into_iter().map(|o| o.id).collect();
        assert_eq!(left, [c]);
    }

    #[test]
    fn a_connector_joins_two_shapes_follows_them_and_reopens() {
        let mut s = session();
        let ins = |s: &mut Session, x: f64, y: f64| -> String {
            s.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": y, "w": 100, "h": 50 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b) = (ins(&mut s, 50.0, 40.0), ins(&mut s, 400.0, 240.0));
        let c = s.execute("drawing.connect", json!({ "from": a, "to": b, "geometry": { "preset": "bentConnector3" } })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let o = obj(&mut s, &c);
        let conn = o.connection.clone().unwrap();
        assert_eq!((conn.start.as_ref().unwrap().id.as_str(), conn.end.as_ref().unwrap().id.as_str()), (a.as_str(), b.as_str()));
        assert_eq!(o.geometry, dm::Geometry::preset("bentConnector3"));
        let site = |s: &mut Session, id: &str, k: usize| kition_office_core::connect::sites(&obj(s, id))[k];
        let near = |p: kition_office_core::Point, q: kition_office_core::Point| (p - q).hypot() < 1.0;
        let (p0, p1) = kition_office_core::connect::line_ends(&o.frame);
        let (sa, sb) = (conn.start.as_ref().unwrap().site, conn.end.as_ref().unwrap().site);
        assert!(near(p0, site(&mut s, &a, sa)) && near(p1, site(&mut s, &b, sb)), "{p0:?} {p1:?}");
        // b moves: the end follows in the same step; undo puts both back.
        s.execute("drawing.transform", json!({ "ids": [b], "dx": -200, "dy": 100 })).unwrap();
        let (q0, q1) = kition_office_core::connect::line_ends(&obj(&mut s, &c).frame);
        assert!(near(q0, p0) && near(q1, site(&mut s, &b, sb)), "{q0:?} {q1:?}");
        s.execute("edit.undo", json!({})).unwrap();
        let (r0, r1) = kition_office_core::connect::line_ends(&obj(&mut s, &c).frame);
        assert!(near(r0, p0) && near(r1, p1), "{r0:?} {r1:?}");
        // Saved and reopened, it is a connector glued to the same shapes.
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let bytes = kition_sheet_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("xl/drawings/drawing1.xml").unwrap(), &mut xml).unwrap();
        assert!(xml.contains("<xdr:cxnSp") && xml.contains("<a:stCxn id=") && !xml.contains("{{cxn:"), "{xml}");
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "c.xlsx", "base64": b64})).unwrap();
        let objects = objects(&back.doc().unwrap().wb);
        let connector = objects.iter().find(|o| o.connection.is_some()).unwrap();
        let ends = connector.connection.clone().unwrap();
        let ids: Vec<&str> = objects.iter().filter(|o| o.geometry == dm::Geometry::preset("rect")).map(|o| o.id.as_str()).collect();
        assert_eq!(vec![ends.start.unwrap().id.as_str(), ends.end.unwrap().id.as_str()], ids);
        assert!(s.execute("drawing.connect", json!({ "from": a, "to": a })).is_err());
    }

    #[test]
    fn art_text_warps_keep_the_text_save_and_come_off() {
        let mut s = session();
        let id = s
            .execute(
                "drawing.insert",
                json!({ "kind": "textBox", "paragraphs": [{ "runs": [{ "text": "Art", "bold": true }, { "text": " text" }] }] }),
            )
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        s.execute("drawing.text", json!({ "id": id, "warp": { "preset": "textArchUp", "adjust": { "adj": 10800000 } } })).unwrap();
        let t = obj(&mut s, &id).text.unwrap();
        let w = t.warp.clone().unwrap();
        assert_eq!(
            (t.text().as_str(), t.paragraphs[0].runs[0].bold, w.preset.as_str(), w.adjust["adj"]),
            ("Art text", Some(true), "textArchUp", 10800000.0)
        );
        // Saved and reopened, the warp and its adjust value are kept.
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "w.xlsx", "base64": b64})).unwrap();
        let again = objects(&back.doc().unwrap().wb).into_iter().find_map(|o| o.text.and_then(|t| t.warp)).unwrap();
        assert_eq!(again, w);
        // Removed: the text stays; undo bends it again.
        s.execute("drawing.text", json!({ "id": id, "warp": null })).unwrap();
        let t = obj(&mut s, &id).text.unwrap();
        assert_eq!((t.warp.clone(), t.text().as_str()), (None, "Art text"));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &id).text.unwrap().warp.unwrap().preset, "textArchUp");
        assert!(s.execute("drawing.text", json!({ "id": id, "warp": "textSpiral" })).is_err());
    }

    #[test]
    fn connectors_follow_shapes_moved_by_other_edits() {
        let mut s = session();
        let ins = |s: &mut Session, x: f64, y: f64| -> String {
            s.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": y, "w": 80, "h": 40 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b) = (ins(&mut s, 50.0, 200.0), ins(&mut s, 400.0, 300.0));
        let c = s.execute("drawing.connect", json!({ "from": a, "to": b })).unwrap()["id"].as_str().unwrap().to_string();
        let glued = |s: &mut Session| {
            let conn = obj(s, &c);
            let end = conn.connection.as_ref().unwrap().end.clone().unwrap();
            let site = kition_office_core::connect::sites(&obj(s, &end.id))[end.site];
            let (_, p1) = kition_office_core::connect::line_ends(&conn.frame);
            (p1 - site).hypot() < 1.0
        };
        let before = obj(&mut s, &c).frame;
        // The editor's own Move Object (what a drag runs) moves b; the end follows.
        s.execute("object.move", json!({ "kind": "shape", "id": b.parse::<u32>().unwrap(), "dx": 30, "dy": 60 })).unwrap();
        assert!(glued(&mut s) && obj(&mut s, &c).frame != before);
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(obj(&mut s, &c).frame, before);
        // Rows inserted between a and b push b down, and the connector's end with it.
        s.execute("home.insertRows", json!({ "rows": "13:16" })).unwrap();
        assert!(glued(&mut s) && obj(&mut s, &c).frame.h > before.h, "{:?} {before:?}", obj(&mut s, &c).frame);
    }

    #[test]
    fn a_plain_line_glued_at_both_ends_is_a_connector_that_saves() {
        let mut s = session();
        let ins = |s: &mut Session, preset: &str, x: f64, y: f64| -> String {
            s.execute("drawing.insert", json!({ "geometry": { "preset": preset }, "frame": { "x": x, "y": y, "w": 80, "h": 40 } })).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let (a, b, line) = (ins(&mut s, "rect", 40.0, 40.0), ins(&mut s, "rect", 300.0, 240.0), ins(&mut s, "line", 150.0, 150.0));
        s.execute("drawing.reconnect", json!({ "id": line, "end": "start", "to": a, "site": 3 })).unwrap();
        s.execute("drawing.reconnect", json!({ "id": line, "end": "end", "to": b, "site": 1 })).unwrap();
        let o = obj(&mut s, &line);
        let conn = o.connection.clone().unwrap();
        assert_eq!((conn.start.unwrap().id, conn.end.unwrap().id), (a.clone(), b.clone()));
        let (p0, p1) = kition_office_core::connect::line_ends(&o.frame);
        assert!((p0 - kition_office_core::connect::sites(&obj(&mut s, &a))[3]).hypot() < 1.0, "{p0:?}");
        assert!((p1 - kition_office_core::connect::sites(&obj(&mut s, &b))[1]).hypot() < 1.0, "{p1:?}");
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "l.xlsx", "base64": b64})).unwrap();
        assert!(objects(&back.doc().unwrap().wb).iter().any(|o| o.connection.as_ref().is_some_and(|c| c.start.is_some() && c.end.is_some())));
    }

    #[test]
    fn a_chart_series_becomes_a_line_on_the_secondary_axis_and_back() {
        use kition_sheet_model::ChartKind;
        let mut s = session();
        for (cell, input) in [
            ("A1", "Month"),
            ("B1", "Sales"),
            ("C1", "Share"),
            ("A2", "Jan"),
            ("B2", "100"),
            ("C2", "0.2"),
            ("A3", "Feb"),
            ("B3", "300"),
            ("C3", "0.5"),
        ] {
            s.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
        }
        let id = s
            .execute(
                "drawing.insert",
                json!({ "kind": "chart", "range": "A1:C3", "chart": { "kind": "column" }, "frame": { "x": 300, "y": 20, "w": 360, "h": 220 } }),
            )
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        s.execute("drawing.chart", json!({ "id": id, "seriesFormat": [{ "index": 1, "kind": "line", "secondary": true }] })).unwrap();
        let c = obj(&mut s, &id).chart.unwrap();
        assert_eq!(
            (c.kind.clone(), c.series[1].kind.clone(), c.series[1].secondary, c.series[0].secondary),
            (kition_office_core::chart::ChartKind::Column, Some(kition_office_core::chart::ChartKind::Line), true, false)
        );
        let model = s.doc().unwrap().wb.sheet(0).unwrap().charts[0].clone();
        assert_eq!((model.kind, model.series[1].kind, model.series[1].secondary), (ChartKind::Combo, Some(ChartKind::Line), true));
        // Saved and reopened, it is still a combination on two axes.
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "k.xlsx", "base64": b64})).unwrap();
        let again = objects(&back.doc().unwrap().wb).into_iter().find_map(|o| o.chart).unwrap();
        assert_eq!((again.series[1].kind.clone(), again.series[1].secondary), (Some(kition_office_core::chart::ChartKind::Line), true));
        // One type again: no longer a combination.
        s.execute("drawing.chart", json!({ "id": id, "seriesFormat": [{ "index": 1, "kind": null, "secondary": false }] })).unwrap();
        let model = s.doc().unwrap().wb.sheet(0).unwrap().charts[0].clone();
        assert_eq!((model.kind, model.series[1].kind, model.series[1].secondary), (ChartKind::ColumnClustered, None, false));
        assert!(s.execute("drawing.chart", json!({ "id": id, "seriesFormat": [{ "index": 1, "kind": "pie" }] })).is_err());
    }

    #[test]
    fn chart_trendlines_and_error_bars_set_draw_and_reopen() {
        let mut s = session();
        for (cell, input) in [
            ("A1", "Month"),
            ("B1", "Sales"),
            ("A2", "Jan"),
            ("B2", "1"),
            ("A3", "Feb"),
            ("B3", "3"),
            ("A4", "Mar"),
            ("B4", "2"),
            ("A5", "Apr"),
            ("B5", "5"),
        ] {
            s.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
        }
        let id = s
            .execute(
                "drawing.insert",
                json!({ "kind": "chart", "range": "A1:B5", "chart": { "kind": "column" }, "frame": { "x": 300, "y": 20, "w": 360, "h": 220 } }),
            )
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let lines = |s: &Session| {
            let doc = s.doc().unwrap();
            let sheet = doc.wb.sheet(0).unwrap();
            let chart = &sheet.charts[0];
            let data = kition_sheet_chart::resolve(&doc.wb, 0, chart);
            kition_sheet_chart::render(chart, &data, 360.0, 220.0, &kition_sheet_chart::ApproxMeasure)
                .iter()
                .filter(|p| matches!(p, kition_sheet_chart::Prim::Line { width, .. } if *width == 1.5 || *width == 0.75))
                .count()
        };
        let before = lines(&s);
        s.execute("drawing.chart", json!({ "id": id, "seriesFormat": [{ "index": 0, "trendline": "linear", "errorBars": { "kind": "stdErr" } }] }))
            .unwrap();
        let c = obj(&mut s, &id).chart.unwrap();
        assert_eq!(
            (c.series[0].trendline.as_ref().unwrap().kind.as_str(), c.series[0].error_bars.as_ref().unwrap().kind.as_str()),
            ("linear", "stdErr")
        );
        // One trendline, and an error bar with its two caps on each of the four columns.
        assert_eq!(lines(&s), before + 1 + 4 * 3);
        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "t.xlsx", "base64": b64})).unwrap();
        let again = objects(&back.doc().unwrap().wb).into_iter().find_map(|o| o.chart).unwrap();
        assert_eq!(
            (again.series[0].trendline.clone(), again.series[0].error_bars.clone()),
            (c.series[0].trendline.clone(), c.series[0].error_bars.clone())
        );
        s.execute("drawing.chart", json!({ "id": id, "seriesFormat": [{ "index": 0, "trendline": null, "errorBars": null }] })).unwrap();
        assert_eq!(lines(&s), before);
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(lines(&s), before + 13);
    }

    #[test]
    fn insert_art_text_makes_a_drawing_object_with_its_warp() {
        let mut s = session();
        let id = s.execute("insert.artText", json!({ "text": "Kition", "warp": "textArchUp" })).unwrap()["id"].as_str().unwrap().to_string();
        let o = obj(&mut s, &id);
        let t = o.text.clone().unwrap();
        assert_eq!(
            (o.kind, t.text().as_str(), t.paragraphs[0].runs[0].size, t.warp.unwrap().preset.as_str()),
            (dm::ObjectKind::TextBox, "Kition", Some(36.0), "textArchUp")
        );
        assert_eq!(o.fill, Some(dm::Fill::None));
        let flat = s.execute("insert.artText", json!({})).unwrap()["id"].as_str().unwrap().to_string();
        assert_eq!(obj(&mut s, &flat).text.unwrap().warp, None);
    }

    #[test]
    fn a_shape_moves_with_its_rows() {
        let mut s = session();
        let id = s.execute("drawing.insert", json!({ "frame": { "x": 70, "y": 45, "w": 50, "h": 20 } })).unwrap()["id"].as_str().unwrap().to_string();
        let before = obj(&mut s, &id).frame;
        s.execute("home.insertRows", json!({ "rows": "1:2" })).unwrap();
        let after = obj(&mut s, &id).frame;
        assert!(after.y > before.y + 1.0, "inserting rows above moves the shape down: {before:?} → {after:?}");
        assert_eq!(after.x, before.x);
    }

    #[test]
    fn failures_change_nothing_and_protection_refuses() {
        let mut s = session();
        let id = s.execute("drawing.insert", json!({})).unwrap()["id"].as_str().unwrap().to_string();
        let before = s.doc().unwrap().wb.clone();
        let e = s.execute("drawing.format", json!({ "ids": [id], "fill": { "color": { "rgb": "#000000", "mods": [["tint", 5]] } } })).unwrap_err();
        assert!(e.to_string().contains("unsupported (color.mods)"), "{e}");
        let e = s.execute("drawing.transform", json!({ "ids": [id, "4242"], "dx": 1 })).unwrap_err();
        assert!(e.to_string().contains("not-found"), "{e}");
        assert_eq!(*s.doc().unwrap().wb, *before);
        // A run colour a sheet cannot hold fails the text, which stays.
        let e = s
            .execute("drawing.text", json!({ "id": id, "paragraphs": [{ "runs": [{ "text": "x", "bold": true, "color": { "rgb": "#000000", "mods": [["tint", 5]] } }] }] }))
            .unwrap_err();
        assert!(e.to_string().contains("unsupported (color.mods)"), "{e}");
        assert_eq!(*s.doc().unwrap().wb, *before);
        s.execute("drawing.delete", json!({ "ids": [id] })).unwrap();
        assert!(s.doc().unwrap().wb.active().unwrap().shapes.is_empty());
    }

    #[test]
    fn theme_tints_round_trip() {
        for tint in [-500i16, 0, 250, 600] {
            let c = color_of(Color::Theme(5, tint), None);
            assert_eq!(color_back(&c).unwrap(), (Color::Theme(5, tint), None), "tint {tint}");
        }
    }
}
