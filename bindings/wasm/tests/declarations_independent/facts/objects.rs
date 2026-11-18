//! Stored projections of the independently recorded object/extra paths.
use super::model::{Context, Facts, area};
use crate::paths::Reading;
use kition_sheet_engine::core::{CellRef, RangeRef};
use serde_json::{Value, json};

pub(super) fn expected(row: &Reading, params: &Value, cx: &Context) -> Facts {
    let mut out = Facts::default();
    let si = cx.active;
    let sheet = &cx.wb.sheets[si];
    let sid = cx.sid(si);
    let command = row.command;
    if command.starts_with("chart.") {
        chart(&mut out, command, params, cx);
        return out;
    }
    if row.holds == "none" {
        return out;
    }
    if command.starts_with("drawing.") || command.starts_with("arrange.") || command == "draw.inkToShape" {
        drawing(&mut out, command, params, cx);
        return out;
    }
    match command {
        "view.customViews" => {
            out.add("workbook", ["~storedViews"]);
            if params.get("save").is_some() {
                out.window("workbook", "sheets", si, si + 1);
                out.add(&sid, ["~viewZoom", "props.freeze"]);
                cx.address(&mut out, si, cx.selected);
            } else if params.get("show").is_some() {
                out.window("workbook", "sheets", si, si + 1);
                out.add(&sid, ["~viewZoom", "props.freeze", "~freezeOrigin", "features.merges"]);
                cx.address(&mut out, si, RangeRef::cell(CellRef::new(2, 1)));
            }
        }
        "view.split" => {
            let mut freeze = Reading {
                family: row.family,
                line: row.line,
                command: "view.freezePanes",
                params: json!({}),
                holds: "sheet{props.freeze,~freezeOrigin}",
                reads: "part,selection",
                note: row.note,
            };
            if sheet.freeze.is_some() {
                freeze.command = "view.unfreezePanes";
            }
            out = super::view::expected(&freeze, &json!({}), cx);
        }
        "view.newWindow" => cx.whole(&mut out),
        "automate.newScript" | "automate.deleteScript" => out.add("workbook", ["~storedScripts"]),
        "automate.runScript" => {
            out.add("workbook", ["~storedScripts"]);
            // extra.rs::run_script executes stored engine commands in order.
            let stored = cx.wb.names.iter().find(|name| name.name == "_sc_scripts").unwrap();
            let text = stored.formula.strip_prefix('"').and_then(|text| text.strip_suffix('"')).unwrap().replace("\"\"", "\"");
            let scripts: Value = serde_json::from_str(&text).unwrap();
            for step in scripts[params["name"].as_str().unwrap()].as_array().unwrap() {
                let nested = step["command"].as_str().unwrap();
                assert_eq!(nested, "cell.set", "add an independent projection for this script step");
                cx.entry(&mut out, si, area(step["params"]["cell"].as_str().unwrap()).unwrap(), true);
            }
        }
        "automate.recordActions" if params.get("stop").is_some() => out.add("workbook", ["~storedScripts"]),
        "automate.recordActions" => {}
        "formulas.watchWindow" => out.add("workbook", ["~storedWatch"]),
        "pageLayout.themeColors" | "pageLayout.themeFonts" => {
            out.add("workbook", ["theme.name", "theme.colors"]);
            if command == "pageLayout.themeFonts" {
                for key in ["major", "minor"] {
                    if params.get(key).is_some() {
                        out.add("workbook", [format!("theme.{key}_font")]);
                    }
                }
            }
        }
        "pageLayout.background" if params.get("clear").and_then(Value::as_bool) == Some(true) => {
            out.add(&sid, ["images"]);
            for image in &sheet.images {
                out.add(&format!("drawing:{}", image.id), ["*"]);
                out.position(&format!("drawing:{}", image.id), &["*"]);
            }
            cx.shared(&mut out, si, None, false);
        }
        "pageLayout.background" => insertion(&mut out, cx, si, CellRef::default(), false),
        "draw.stroke" => {
            let point = params["points"][0].as_array().unwrap();
            cx.geometry(&mut out, si, sheet.row_at(point[1].as_f64().unwrap()), sheet.col_at(point[0].as_f64().unwrap()));
            out.add("workbook", ["~nextObjectId"]);
            cx.shared(&mut out, si, None, false);
        }
        "insert.checkbox" | "cell.toggleCheckbox" => {
            let owner = cx.sheet(params);
            let range = cx.range(params);
            cx.lookup(&mut out, params, owner);
            if command == "insert.checkbox" {
                cx.entry(&mut out, owner, range, false);
            } else {
                // extra.rs:1042 only mutates materialized boolean inputs.
                cx.address(&mut out, owner, range);
                cx.constraints(&mut out, owner, range);
                for at in range.iter() {
                    out.add(&cx.cell(owner, at), ["input", "text", "style"]);
                }
                cx.fit(&mut out, owner, range);
                cx.shared(&mut out, owner, Some(range), false);
            }
        }
        "home.drawBorder" => {
            let owner = cx.sheet(params);
            cx.lookup(&mut out, params, owner);
            super::edit::borders(&mut out, params, cx, owner);
        }
        "home.newCellStyle" => {
            let at = params.get("fromCell").and_then(Value::as_str).and_then(area).map_or(cx.active_cell, |range| range.start);
            cx.address(&mut out, si, RangeRef::cell(at));
            cx.inherited_style(&mut out, si, at);
            out.add("workbook", [format!("cellStyleDefinitions.{}", kdm_model::cell_style_name_key(params["name"].as_str().unwrap()))]);
        }
        "table.resize" => {
            let table = sheet
                .tables
                .iter()
                .find(|table| params.get("table").and_then(Value::as_str).is_some_and(|name| table.name.eq_ignore_ascii_case(name)))
                .or_else(|| sheet.table_at(cx.active_cell))
                .unwrap();
            out.add(&sid, [format!("features.tables[name={}]", json!(table.name)), "~arrays".into()]);
            if params["range"].as_str().is_some_and(|text| text.contains('!')) {
                out.add(&sid, ["name"]);
            }
            let range = cx.range(params);
            cx.address(&mut out, si, range);
            cx.constraints(&mut out, si, range);
            // extra.rs::table_resize compares Value::display header text.
            cx.source(&mut out, si, RangeRef::new(range.start, CellRef::new(range.start.row, range.end.col)), false);
            cx.shared(&mut out, si, None, true);
        }
        "table.removeDuplicates" => {
            let range = params.get("range").and_then(Value::as_str).and_then(area).unwrap_or_else(|| sheet.tables[0].range);
            cx.source(&mut out, si, range, true);
            cx.entry(&mut out, si, range, false);
            // data.rs::remove_duplicates copies body cells; it does not move
            // merges/arrays or other objects, nor write the header row.
            out.0.retain(|fact| fact.field != "~headerReferences");
        }
        "data.fromTextCsv" | "data.getData" | "data.fromTableRange" => {
            out.add("workbook", ["~tabs", "~tableNames", "~nextObjectId", "props.dateSystem", "props.defaultStyle", "props.protectedStructure"]);
            out.window("workbook", "sheets", si + 1, si + 1);
            for owner in 0..cx.wb.sheets.len() {
                out.add(&cx.sid(owner), ["name"]);
            }
            if command == "data.fromTableRange" {
                out.add(&sid, ["name"]);
                cx.source(&mut out, si, cx.range(params), true);
            }
            cx.shared(&mut out, si, None, true);
        }
        _ => panic!("unprojected object row {command} {params}"),
    }
    out
}

pub(super) fn insertion(out: &mut Facts, cx: &Context, si: usize, at: CellRef, protection: bool) {
    cx.address(out, si, RangeRef::cell(at));
    out.add("workbook", ["~nextObjectId"]);
    if protection {
        out.add(&cx.sid(si), ["props.protection"]);
    }
    cx.shared(out, si, None, false);
}

pub(super) fn chart(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let si = cx.active;
    let sheet = &cx.wb.sheets[si];
    let chart = params
        .get("chart")
        .and_then(Value::as_u64)
        .and_then(|id| sheet.charts.iter().find(|chart| chart.id == id as u32))
        .or_else(|| sheet.charts.last())
        .unwrap();
    let id = format!("drawing:{}", chart.id);
    if command == "chart.shapeFill" && params.get("kind").and_then(Value::as_str) == Some("shape") {
        let shape = sheet.shapes.iter().find(|shape| Some(shape.id as u64) == params.get("id").and_then(Value::as_u64)).unwrap();
        out.add(&format!("drawing:{}", shape.id), ["look.fill"]);
        cx.shared(out, si, None, false);
        return;
    }
    if command == "chart.shapeOutline" {
        let shape = sheet.shapes.iter().find(|shape| Some(shape.id as u64) == params.get("id").and_then(Value::as_u64)).unwrap();
        out.add(&format!("drawing:{}", shape.id), ["look.line"]);
        cx.shared(out, si, None, false);
        return;
    }
    let fields: Vec<String> = match command {
        "chart.addElement" => match params["element"].as_str().unwrap_or("title") {
            "axisTitles" => vec!["parts.x_title", "parts.y_title"],
            "dataLabels" => vec!["parts.data_labels"],
            "smooth" => vec!["parts.series"],
            "title" => vec!["parts.title"],
            "legend" => vec!["parts.legend"],
            "gridlines" => vec!["parts.gridlines"],
            other => panic!("chart element {other}"),
        }
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "chart.quickLayout" => ["parts.legend", "parts.data_labels", "parts.gridlines", "parts.x_title", "parts.y_title"].map(str::to_owned).to_vec(),
        "chart.formatSelection" | "chart.shapeFill" => vec!["parts.series".into()],
        "chart.changeColors" => {
            let mut fields = vec!["parts.kind".into(), "parts.series".into()];
            if matches!(chart.kind, kition_sheet_engine::model::ChartKind::Pie | kition_sheet_engine::model::ChartKind::Doughnut) {
                fields.push("parts.point_colors".into());
            }
            fields
        }
        "chart.selectData" | "chart.switchRowColumn" => vec!["parts.series".into(), "parts.source".into(), "parts.by_rows".into()],
        "chart.move" => Vec::new(),
        "chart.delete" => vec!["*".into()],
        "chart.set" | "chart.changeType" => {
            let mut fields: Vec<_> = [
                ("type", "kind"),
                ("totals", "totals"),
                ("title", "title"),
                ("legend", "legend"),
                ("dataLabels", "data_labels"),
                ("gridlines", "gridlines"),
                ("style", "style"),
                ("xTitle", "x_title"),
                ("yTitle", "y_title"),
            ]
            .into_iter()
            .filter(|(parameter, _)| params.get(*parameter).is_some())
            .map(|(_, field)| format!("parts.{field}"))
            .collect();
            if params.get("totals").is_some() && params.get("type").is_none() {
                fields.push("parts.kind".into());
            }
            if ["at", "width", "height", "dx", "dy"].iter().any(|key| params.get(*key).is_some()) {
                fields.push("~assignedFrame".into());
            }
            fields
        }
        _ => panic!("unprojected chart {command}"),
    };
    out.add(&id, fields);
    out.position(
        &id,
        if command == "chart.delete" {
            &["*"]
        } else if command == "chart.move" {
            &["sheet", "frame"]
        } else {
            &["sheet"]
        },
    );
    if command == "chart.move" {
        out.position(&id, &["z"]);
        let destination = params.get("sheet").and_then(Value::as_str).and_then(|name| cx.wb.sheet_index(name)).unwrap_or(si);
        if params.get("sheet").is_some() {
            out.add(&cx.sid(destination), ["name"]);
        }
        if let Some(range) = params.get("at").and_then(Value::as_str).and_then(area) {
            cx.address(out, destination, range);
        }
        cx.shared(out, destination, None, false);
    }
    if matches!(command, "chart.selectData" | "chart.switchRowColumn") {
        let text = params.get("range").and_then(Value::as_str).or(chart.source.as_deref()).unwrap();
        chart_source(out, cx, si, text);
    } else if command == "chart.changeColors" && params.get("palette").is_some() {
        chart_values(out, cx, si, chart);
    }
    if let Some(range) = params.get("at").and_then(Value::as_str).and_then(area)
        && command != "chart.move"
    {
        cx.address(out, si, range);
    }
    cx.shared(out, si, None, false);
}

pub(super) fn chart_source(out: &mut Facts, cx: &Context, si: usize, text: &str) {
    let owner = text.rsplit_once('!').and_then(|(name, _)| cx.wb.sheet_index(name.trim_matches('\''))).unwrap_or(si);
    let range = area(text).unwrap();
    out.add(&cx.sid(owner), ["name"]);
    cx.source(out, owner, range, true);
}

fn chart_values(out: &mut Facts, cx: &Context, si: usize, chart: &kition_sheet_engine::model::Chart) {
    for series in &chart.series {
        for text in std::iter::once(&series.values).chain(series.categories.as_ref()) {
            if !text.is_empty() {
                let expr = kition_sheet_engine::formula::parse(text).unwrap();
                cx.expression(out, si, &expr, 0);
            }
        }
    }
    out.add("workbook", ["props.dateSystem", "props.defaultStyle", "theme"]);
}

fn drawing(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let si = cx.active;
    let sheet = &cx.wb.sheets[si];
    let sid = cx.sid(si);
    if command == "drawing.ink" {
        for stroke in params["strokes"].as_array().unwrap() {
            let point = &stroke["points"][0];
            cx.geometry(out, si, sheet.row_at(point[1].as_f64().unwrap()), sheet.col_at(point[0].as_f64().unwrap()));
        }
        out.add("workbook", ["~nextObjectId"]);
        cx.shared(out, si, None, false);
        return;
    }
    if command == "drawing.erase" {
        out.add(&sid, ["props.protection"]);
        out.fact("call", "covers", "~ink");
        return;
    }
    if command == "drawing.insert" {
        if params["kind"] == "chart" {
            chart_source(out, cx, si, params["range"].as_str().unwrap());
        } else {
            let at = params
                .get("frame")
                .map_or(cx.active_cell, |frame| CellRef::new(sheet.row_at(frame["y"].as_f64().unwrap()), sheet.col_at(frame["x"].as_f64().unwrap())));
            cx.geometry(out, si, at.row, at.col);
        }
        out.add("workbook", ["~nextObjectId"]);
        out.add(&sid, ["props.protection"]);
        cx.shared(out, si, None, false);
        return;
    }
    let numeric = |value: &Value| value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok())).unwrap() as u32;
    let mut ids: Vec<u32> = params.get("ids").and_then(Value::as_array).map(|ids| ids.iter().map(numeric).collect()).unwrap_or_default();
    for key in ["id", "from", "to"] {
        if let Some(id) = params.get(key) {
            ids.push(numeric(id));
        }
    }
    for id in ids.clone() {
        for shape in sheet.shapes.iter().filter(|shape| shape.groups.contains(&id)) {
            ids.push(shape.id);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    let mut furthest = CellRef::default();
    for id in ids {
        let tool = format!("drawing:{id}");
        if let Some(chart) = sheet.charts.iter().find(|chart| chart.id == id) {
            out.add(&tool, ["parts"]);
            out.position(&tool, &["sheet", "frame"]);
            chart_values(out, cx, si, chart);
            continue;
        }
        let Some(shape) = sheet.shapes.iter().find(|shape| shape.id == id) else { continue };
        furthest.row = furthest.row.max(shape.anchor.cell.row);
        furthest.col = furthest.col.max(shape.anchor.cell.col);
        let fields: Vec<&str> = match command {
            "drawing.text" => vec!["text", "look.text_body", "look.kind"],
            "drawing.format" => {
                let mut fields = vec!["look.kind"];
                if params.get("fill").is_some() {
                    fields.extend(["look.fill", "look.fill_alpha", "look.gradient"]);
                }
                if params.get("stroke").is_some() {
                    fields.extend(["look.line", "look.outline"]);
                }
                if params.get("effects").is_some() {
                    fields.push("look.effects");
                }
                fields
            }
            "drawing.transform" => vec!["look.groups", "look.connection"],
            "drawing.connect" => vec!["look.kind", "look.preset", "look.geom"],
            "drawing.reconnect" if params.get("to").map(numeric) == Some(id) => vec!["look.kind", "look.preset", "look.geom"],
            "drawing.reconnect" => vec!["look.kind", "look.connection"],
            "drawing.group" | "drawing.ungroup" => vec!["look.kind", "look.groups"],
            "draw.inkToShape" => vec!["look.kind", "look.fill", "look.line", "text"],
            "arrange.rotate" | "arrange.align" | "arrange.group" | "arrange.bringForward" | "arrange.sendBackward" | "drawing.arrange" => Vec::new(),
            "drawing.delete" => vec!["*"],
            _ => panic!("unprojected drawing {command}"),
        };
        out.add(&tool, fields);
        if command == "drawing.delete" {
            out.position(&tool, &["*"]);
        } else {
            out.position(&tool, &["sheet"]);
            if !matches!(command, "drawing.format" | "drawing.text") {
                out.position(&tool, &["frame"]);
            }
            if command.contains("Forward") || command.contains("Backward") || command == "drawing.arrange" {
                out.position(&tool, &["z"]);
            }
        }
    }
    if matches!(command, "drawing.transform" | "drawing.connect" | "drawing.reconnect" | "arrange.align" | "arrange.group") {
        if let Some(frame) = params.get("frame") {
            furthest.row = furthest.row.max(sheet.row_at(frame["y"].as_f64().unwrap()));
            furthest.col = furthest.col.max(sheet.col_at(frame["x"].as_f64().unwrap()));
        }
        cx.geometry(out, si, furthest.row, furthest.col);
    }
    if matches!(command, "drawing.connect" | "drawing.group") {
        out.add("workbook", ["~nextObjectId"]);
    }
    // extra.rs::with_shape (legacy rotation) does not check protection.
    if command != "arrange.rotate" {
        out.add(&sid, ["props.protection"]);
    }
    cx.shared(out, si, None, false);
}
