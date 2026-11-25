//! Object defaults and metadata dependencies found by the independent handler reading.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise, window},
};
use kition_kit::{DerivedValues, Premise, Under};
use kition_office_core::{ink::InkStroke, ops::DrawingOp};
use kition_sheet_engine::model::{Chart, ChartKind, ShapeKind};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn number(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|number| u32::try_from(number).ok()).or_else(|| value.as_str()?.parse().ok())
}

fn stored_json(workbook: &kition_sheet_engine::model::Workbook, key: &str) -> Value {
    workbook
        .names
        .iter()
        .find(|definition| definition.name == key)
        .and_then(|definition| match kition_sheet_engine::formula::parse(&definition.formula).ok()? {
            kition_sheet_engine::formula::Expr::Text(text) => serde_json::from_str(&text).ok(),
            _ => None,
        })
        .unwrap_or_else(|| json!({}))
}

pub(crate) fn scalar_chart_premise(tool: String, command: &str, params: &Value, chart: &Chart) -> Option<Premise> {
    // same-field decision: hold the observed value each assignment replaces.
    let mut positions = vec!["sheet"];
    let fields = match command {
        "chart.move" => {
            positions.push("frame");
            Vec::new()
        }
        "chart.addElement" => match params.get("element").and_then(Value::as_str).unwrap_or("title") {
            "title" => vec!["parts.title"],
            "legend" => vec!["parts.legend"],
            "dataLabels" => vec!["parts.data_labels"],
            "gridlines" => vec!["parts.gridlines"],
            "axisTitles" => vec!["parts.x_title", "parts.y_title"],
            "smooth" => vec!["parts.series"],
            _ => return None,
        },
        "chart.quickLayout" => vec!["parts.legend", "parts.data_labels", "parts.gridlines", "parts.x_title", "parts.y_title"],
        "chart.formatSelection" => vec!["parts.series"],
        "chart.shapeFill" if params.get("kind").and_then(Value::as_str) != Some("shape") => vec!["parts.series"],
        "chart.changeColors" => {
            let mut fields = vec!["parts.kind", "parts.series"];
            if matches!(chart.kind, ChartKind::Pie | ChartKind::Doughnut) {
                fields.push("parts.point_colors");
            }
            fields
        }
        "chart.selectData" | "chart.switchRowColumn" => vec!["parts.series", "parts.source", "parts.by_rows"],
        "chart.set" | "chart.changeType" => {
            let mut fields = Vec::new();
            for (parameter, field) in [
                ("type", "parts.kind"),
                ("totals", "parts.totals"),
                ("title", "parts.title"),
                ("legend", "parts.legend"),
                ("dataLabels", "parts.data_labels"),
                ("gridlines", "parts.gridlines"),
                ("style", "parts.style"),
                ("xTitle", "parts.x_title"),
                ("yTitle", "parts.y_title"),
            ] {
                if params.get(parameter).is_some() {
                    fields.push(field);
                }
            }
            if params.get("totals").is_some_and(|value| !value.is_null()) && params.get("type").and_then(Value::as_str).is_none() {
                fields.push("parts.kind");
            }
            if ["at", "width", "height", "dx", "dy"].iter().any(|parameter| params.get(*parameter).is_some()) {
                // A frame assigned by this write is replacement content. Keep
                // sheet relocation positional; compare the assigned anchor as changed.
                fields.push("~assignedFrame");
            }
            fields
        }
        _ => return None,
    };
    Some(premise(tool, fields).positions(positions))
}

impl SheetDocument {
    pub(crate) fn object_read_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        let allocation = names.is_empty() || names.iter().any(|name| name == "nextObjectId");
        let last_shape = names.is_empty() || names.iter().any(|name| name == "lastShape");
        let last_ink = names.is_empty() || names.iter().any(|name| name == "lastInk");
        let view_zoom = names.is_empty() || names.iter().any(|name| name == "viewZoom");
        let assigned_frame = names.is_empty() || names.iter().any(|name| name == "assignedFrame");
        let anchor_mode = names.is_empty() || names.iter().any(|name| name == "anchorMode");
        let chart_stack = names.is_empty() || names.iter().any(|name| name == "chartStack");
        let stashes = [("storedScripts", "_sc_scripts"), ("storedWatch", "_sc_watch"), ("storedViews", "_sc_custom_views")];
        if !allocation
            && !last_shape
            && !last_ink
            && !view_zoom
            && !assigned_frame
            && !anchor_mode
            && !chart_stack
            && !stashes.iter().any(|(name, _)| names.is_empty() || names.iter().any(|asked| asked == name))
        {
            return Ok(());
        }
        let state = self.doc_state()?;
        let mut values = Vec::new();
        if anchor_mode {
            for (count, (id, mode)) in state
                .wb
                .sheets
                .iter()
                .flat_map(|sheet| {
                    sheet
                        .shapes
                        .iter()
                        .map(|object| (object.id, object.anchor.mode))
                        .chain(sheet.charts.iter().map(|object| (object.id, object.anchor.mode)))
                        .chain(sheet.images.iter().map(|object| (object.id, object.anchor.mode)))
                })
                .enumerate()
            {
                if count >= 100_000 {
                    return Err("anchor mode metadata exceeds the premise limit".into());
                }
                values.push((json!(format!("drawing:{id}")), "anchorMode", serde_json::to_value(mode).map_err(|error| error.to_string())?));
            }
        }
        if chart_stack {
            for sheet in &state.wb.sheets {
                if sheet.charts.len() > 100_000 {
                    return Err("chart stack exceeds the premise limit".into());
                }
                values.push((json!(ids::sheet_tool_id(sheet)), "chartStack", json!(sheet.charts.iter().map(|chart| chart.id).collect::<Vec<_>>())));
            }
        }
        if allocation {
            let mut highest = 0u32;
            for sheet in &state.wb.sheets {
                for id in sheet
                    .charts
                    .iter()
                    .map(|object| object.id)
                    .chain(sheet.images.iter().map(|object| object.id))
                    .chain(sheet.shapes.iter().flat_map(|object| std::iter::once(object.id).chain(object.groups.iter().copied())))
                    .chain(sheet.tables.iter().map(|object| object.id))
                    .chain(sheet.diagrams.iter().map(|object| object.id))
                {
                    highest = highest.max(id);
                }
            }
            // Widen before adding so a hostile maximum ID cannot panic during guard evaluation.
            values.push((json!("workbook"), "nextObjectId", json!(u64::from(highest) + 1)));
        }
        if last_shape {
            for sheet in &state.wb.sheets {
                values.push((json!(ids::sheet_tool_id(sheet)), "lastShape", json!(sheet.shapes.last().map(|shape| shape.id))));
            }
        }
        if last_ink {
            for sheet in &state.wb.sheets {
                values.push((
                    json!(ids::sheet_tool_id(sheet)),
                    "lastInk",
                    json!(sheet.shapes.iter().rfind(|shape| shape.kind == ShapeKind::Ink).map(|shape| shape.id)),
                ));
            }
        }
        if view_zoom {
            for sheet in &state.wb.sheets {
                values.push((json!(ids::sheet_tool_id(sheet)), "viewZoom", json!(sheet.zoom)));
            }
        }
        if assigned_frame {
            for (count, chart) in state.wb.sheets.iter().flat_map(|sheet| sheet.charts.iter()).enumerate() {
                if count >= 100_000 {
                    return Err("chart frame metadata exceeds the premise limit".into());
                }
                values.push((
                    json!(format!("drawing:{}", chart.id)),
                    "assignedFrame",
                    serde_json::to_value(chart.anchor).map_err(|error| error.to_string())?,
                ));
            }
        }
        for (name, key) in stashes {
            if !names.is_empty() && !names.iter().any(|asked| asked == name) {
                continue;
            }
            let matching: Vec<_> = state.wb.names.iter().filter(|definition| definition.name == key).take(100_001).collect();
            if matching.len() > 100_000 {
                return Err("stored helper metadata exceeds the premise limit".into());
            }
            values.push((json!("workbook"), name, serde_json::to_value(matching).map_err(|error| error.to_string())?));
        }
        for (id, name, value) in values {
            if !requested.is_empty() && !requested.contains(&id) {
                continue;
            }
            if let Some(existing) = found.iter_mut().find(|existing| existing.id == id) {
                existing.values.insert(name.into(), value);
            } else {
                found.push(DerivedValues { id, values: [(name.into(), value)].into_iter().collect() });
            }
        }
        Ok(())
    }

    pub(crate) fn object_read_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if matches!(command, "formula.useInFormula" | "theme.effects") || command == "script.record" && self.session.recording.is_none() {
            return Ok(Some(Under::default()));
        }
        let stash = match command {
            "script.new" | "script.delete" | "script.record" => Some("~storedScripts"),
            "formula.watchWindow" => Some("~storedWatch"),
            "view.customViews" => Some("~storedViews"),
            _ => None,
        };
        if let Some(stash) = stash {
            let mut under = Under::default();
            under.objects.push(json!("workbook"));
            under.premises.push(premise("workbook", [stash]));
            if command == "view.customViews" {
                let state = self.doc_state()?;
                let save = params.get("save").and_then(Value::as_str).is_some();
                let stored = stored_json(&state.wb, "_sc_custom_views");
                let shown = (!save).then(|| params.get("show").and_then(Value::as_str).and_then(|name| stored.get(name))).flatten();
                let target = if save {
                    Some(state.wb.active_sheet)
                } else {
                    shown.and_then(|view| {
                        view.get("sheet").and_then(Value::as_u64).map_or(Some(state.wb.active_sheet), |number| usize::try_from(number).ok())
                    })
                };
                if let Some(target) = target {
                    let Some(sheet) = state.wb.sheet(target) else { return Ok(None) };
                    let sid = ids::sheet_tool_id(sheet);
                    window(&mut under, "workbook", "sheets", target, target.saturating_add(1));
                    let ranges = if save {
                        under.premises.push(premise(sid.clone(), ["~viewZoom", "props.freeze"]));
                        state.selection.ranges.clone()
                    } else {
                        let Some(text) = shown.and_then(|view| view.get("selection")).and_then(Value::as_str) else {
                            merged(&mut under);
                            return Ok(Some(under));
                        };
                        let Some(ranges) = text.split(',').map(area).collect::<Option<Vec<_>>>() else { return Ok(None) };
                        // same-field decision: restoring a view assigns its
                        // destination zoom and pane counts, even with explicit saved values.
                        under.premises.push(premise(sid.clone(), ["merges", "~viewZoom", "props.freeze", "~freezeOrigin"]));
                        ranges
                    };
                    let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(target)) else { return Ok(None) };
                    for range in ranges {
                        for (field, first, last, lines, kind) in
                            [("rows", range.start.row, range.end.row, &axes.rows, "row"), ("cols", range.start.col, range.end.col, &axes.cols, "col")]
                        {
                            if last.saturating_sub(first) >= 100_000 || lines.get(last as usize).is_none() {
                                return Ok(None);
                            }
                            window(&mut under, &sid, field, first as usize, last as usize + 1);
                            for line in lines.iter().skip(first as usize).take(last.saturating_sub(first) as usize + 1) {
                                under.premises.push(premise(format!("{kind}:{line}"), Vec::<String>::new()).positions(["index"]));
                            }
                        }
                    }
                }
            }
            merged(&mut under);
            return Ok(Some(under));
        }
        if matches!(command, "theme.colors" | "theme.fonts") {
            let mut under = Under::default();
            under.objects.push(json!("workbook"));
            // same-field decision: theme name/colors and supplied
            // fonts are replacement destinations as well as retained defaults.
            let mut fields = vec!["theme.name", "theme.colors"];
            if command == "theme.fonts" {
                if params.get("major").is_some_and(Value::is_string) {
                    fields.push("theme.major_font");
                }
                if params.get("minor").is_some_and(Value::is_string) {
                    fields.push("theme.minor_font");
                }
            }
            under.premises.push(premise("workbook", fields));
            return Ok(Some(under));
        }
        if command == "view.split" {
            let state = self.doc_state()?;
            let unfreeze = state.wb.active().is_some_and(|sheet| sheet.freeze.is_some());
            return self.call_premises(if unfreeze { "view.unfreezePanes" } else { "view.freezePanes" }, &json!({}));
        }
        if command == "ink.toShape" {
            let state = self.doc_state()?;
            let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
            let explicit = params.get("id").and_then(Value::as_u64).map(|id| id.min(u64::from(u32::MAX)) as u32);
            let chosen = match explicit {
                Some(id) => sheet.shapes.iter().find(|shape| shape.id == id && shape.kind == ShapeKind::Ink),
                None => sheet.shapes.iter().rfind(|shape| shape.kind == ShapeKind::Ink),
            };
            let Some(ink) = chosen else { return Ok(None) };
            let sid = ids::sheet_tool_id(sheet);
            let tool = format!("drawing:{}", ink.id);
            let mut under = Under::default();
            under.objects.push(json!(tool));
            // same-field decision: conversion also replaces the fill.
            under.premises.push(premise(tool, ["look.kind", "text", "look.line", "look.fill"]).positions(["frame", "sheet"]));
            under.premises.push(premise(sid.clone(), ["props.protection"]));
            if explicit.is_none() {
                under.premises.push(premise(sid, ["~lastInk"]));
            }
            merged(&mut under);
            return Ok(Some(under));
        }
        if command == "checkbox.insert" {
            let state = self.doc_state()?;
            let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
            let ranges = if let Some(text) = params.get("range").or_else(|| params.get("cell")).and_then(Value::as_str) {
                let Some(ranges) = text.rsplit('!').next().and_then(|body| body.split(',').map(area).collect::<Option<Vec<_>>>()) else {
                    return Ok(None);
                };
                ranges
            } else {
                state.selection.ranges.clone()
            };
            if ranges.iter().try_fold(0u64, |count, range| count.checked_add(range.count())).is_none_or(|count| count > 100_000) {
                return Ok(None);
            }
            let mut under = Under::default();
            for range in ranges {
                let mut entry = params.clone();
                let Some(parameters) = entry.as_object_mut() else { return Ok(None) };
                parameters.insert("range".into(), json!(range.a1()));
                let sheet_target = params.get("sheet").filter(|value| value.is_u64()).cloned().unwrap_or_else(|| json!(ids::sheet_tool_id(sheet)));
                parameters.insert("sheet".into(), sheet_target);
                parameters.remove("input");
                let next = self.call_premises("range.fill", &entry)?.unwrap_or_default();
                under.objects.extend(next.objects);
                under.premises.extend(next.premises);
                under.within.extend(next.within);
                self.entry_row_premises(&mut under, si, range);
            }
            if let Some(sheet) = state.wb.sheet(si) {
                let sid = ids::sheet_tool_id(sheet);
                under.premises.push(premise(sid.clone(), ["~arrays"]));
                if params.get("sheet").is_some_and(Value::is_string)
                    || ["range", "cell"].iter().any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|text| text.contains('!')))
                {
                    under.premises.push(premise(sid, ["name"]));
                }
            }
            merged(&mut under);
            return Ok(Some(under));
        }
        if command == "drawing.ink" {
            let default = InkStroke { tool: Default::default(), color: None, width: 2.0, points: Vec::new() };
            let Ok(strokes) = kition_office_core::ink::parse_strokes(params, &default) else { return Ok(None) };
            let state = self.doc_state()?;
            let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
            let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
            let mut row = 0;
            let mut col = 0;
            for stroke in strokes {
                let x = stroke.points.iter().map(|point| point[0]).fold(f64::INFINITY, f64::min).max(0.0);
                let y = stroke.points.iter().map(|point| point[1]).fold(f64::INFINITY, f64::min).max(0.0);
                row = row.max(sheet.row_at(y));
                col = col.max(sheet.col_at(x));
            }
            // A changed sheet reroutes its existing connectors, whose endpoints can lie
            // beyond the new stroke. Their stored objects are held below as well.
            if sheet.shapes.iter().any(|shape| shape.connection.is_some()) {
                for shape in &sheet.shapes {
                    row = row.max(shape.anchor.cell.row);
                    col = col.max(shape.anchor.cell.col);
                }
            }
            if row >= 100_000 || col >= 100_000 || axes.rows.get(row as usize).is_none() || axes.cols.get(col as usize).is_none() {
                return Ok(None);
            }
            let mut under = Under::default();
            let sid = ids::sheet_tool_id(sheet);
            under.objects.push(json!(sid));
            under.premises.push(premise(sid.clone(), ["props.protection", "props.defaultColWidth", "props.defaultRowHeight"]));
            for (field, last, lines, kind) in [("rows", row, &axes.rows, "row"), ("cols", col, &axes.cols, "col")] {
                window(&mut under, &sid, field, 0, last as usize + 1);
                for line in lines.iter().take(last as usize + 1) {
                    under.premises.push(premise(format!("{kind}:{line}"), ["info.size", "info.hidden"]).positions(["index"]));
                }
            }
            under.premises.push(premise("workbook", ["~nextObjectId"]));
            merged(&mut under);
            return Ok(Some(under));
        }
        let shape_property = matches!(command, "chart.shapeOutline" | "arrange.rotate")
            || command == "chart.shapeFill" && params.get("kind").and_then(Value::as_str) == Some("shape");
        if shape_property {
            let state = self.doc_state()?;
            let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
            let explicit = params.get("id").and_then(Value::as_u64).map(|id| id.min(u64::from(u32::MAX)) as u32);
            let shape = match explicit {
                Some(id) => sheet.shapes.iter().find(|shape| shape.id == id),
                None => sheet.shapes.last(),
            };
            let Some(shape) = shape else { return Ok(None) };
            let sid = ids::sheet_tool_id(sheet);
            let mut under = Under::default();
            under.objects.push(json!(format!("drawing:{}", shape.id)));
            if explicit.is_none() {
                under.premises.push(premise(sid.clone(), ["~lastShape"]));
            }
            if params.get("sheet").is_some_and(Value::is_string) {
                under.premises.push(premise(sid, ["name"]));
            }
            if let Some(index) = params.get("sheet").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) {
                window(&mut under, "workbook", "sheets", index, index.saturating_add(1));
            }
            let tool = format!("drawing:{}", shape.id);
            under.premises.push(match command {
                // same-field decision: the assigned color is held.
                "chart.shapeFill" => premise(tool, ["look.fill"]).positions(["sheet"]),
                "chart.shapeOutline" => premise(tool, ["look.line"]).positions(["sheet"]),
                _ => premise(tool, Vec::<String>::new()).positions(["frame", "sheet"]),
            });
            merged(&mut under);
            return Ok(Some(under));
        }
        if command != "drawing.reconnect" {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
        let Some(id) = params.get("id").and_then(number) else { return Ok(None) };
        let Ok(operation) = DrawingOp::parse(command, params) else { return Ok(None) };
        let mut objects: Vec<_> = sheet.shapes.iter().map(|shape| kition_sheet_engine::cmd::drawing::to_drawing(sheet, shape)).collect();
        let Some(index) = objects.iter().position(|object| object.id == id.to_string()) else { return Ok(None) };
        let Some(object) = objects.get_mut(index) else { return Ok(None) };
        let old_frame = object.frame;
        if operation.apply(object).is_err() {
            return Ok(None);
        }
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        under.premises.push(premise(sid.clone(), ["props.protection", "props.defaultColWidth", "props.defaultRowHeight"]));
        if params.get("sheet").is_some_and(Value::is_string) {
            under.premises.push(premise(sid.clone(), ["name"]));
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) {
            window(&mut under, "workbook", "sheets", index, index.saturating_add(1));
        }
        let mut endpoints = BTreeSet::new();
        let mut connectors = BTreeSet::from([id]);
        let mut points = vec![(old_frame.x, old_frame.y)];
        for object in &objects {
            let is_changed = object.id == id.to_string();
            if is_changed {
                points.push((object.frame.x, object.frame.y));
            }
            let Some(connection) = &object.connection else { continue };
            if let Ok(id) = object.id.parse::<u32>() {
                connectors.insert(id);
            }
            points.push((object.frame.x, object.frame.y));
            for end in [connection.start.as_ref(), connection.end.as_ref()].into_iter().flatten() {
                if let Ok(id) = end.id.parse::<u32>() {
                    endpoints.insert(id);
                }
            }
            if let Some(frame) = kition_office_core::connect::reroute(object, connection, &objects) {
                points.push((frame.x, frame.y));
            }
        }
        if let Some(target) = params.get("to").and_then(number) {
            endpoints.insert(target);
        }
        for id in connectors {
            under.premises.push(
                premise(format!("drawing:{id}"), ["look.kind", "look.groups", "look.connection", "look.preset", "look.geom"])
                    .positions(["frame", "sheet"]),
            );
        }
        for id in endpoints {
            under.premises.push(premise(format!("drawing:{id}"), ["look.kind", "look.preset", "look.geom"]).positions(["frame", "sheet"]));
            if let Some(object) = objects.iter().find(|object| object.id == id.to_string()) {
                points.push((object.frame.x, object.frame.y));
            }
        }
        let row = points.iter().map(|(_, y)| sheet.row_at(y.max(0.0))).max().unwrap_or(0);
        let col = points.iter().map(|(x, _)| sheet.col_at(x.max(0.0))).max().unwrap_or(0);
        if row >= 100_000 || col >= 100_000 || axes.rows.get(row as usize).is_none() || axes.cols.get(col as usize).is_none() {
            return Ok(None);
        }
        for (field, last, lines, kind) in [("rows", row, &axes.rows, "row"), ("cols", col, &axes.cols, "col")] {
            window(&mut under, &sid, field, 0, last as usize + 1);
            for line in lines.iter().take(last as usize + 1) {
                under.premises.push(premise(format!("{kind}:{line}"), ["info.size", "info.hidden"]).positions(["index"]));
            }
        }
        merged(&mut under);
        Ok(Some(under))
    }
}
