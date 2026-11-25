//! Reference parameters audited against cmd/{drawing,ink,insert,extra,pivot,sheet,formulas}.rs.
//! Numeric `sheet` values are tab indexes, never persistent ids. Tables and pivots are held
//! JSON members, so their resolvers are places; `place` holds their actual sheet paths.
use kition_kit::KitCommand;

const DRAWING: &[&str] = &["Shape", "Chart", "Image", "Group"];
const ANCHOR: &[&str] = &["Shape", "Chart", "Image"];
const SHAPE: &[&str] = &["Shape"];
const CHART: &[&str] = &["Chart"];
const SHAPE_GROUP: &[&str] = &["Shape", "Group"];
const GROUP: &[&str] = &["Group"];
const SHARED_DRAWING: &[&str] = &["Shape", "Chart", "Group"];
const SHAPE_CHART: &[&str] = &["Shape", "Chart"];

fn reference_schema(node: &mut serde_json::Value) {
    if node.get("type").and_then(serde_json::Value::as_str) == Some("array") {
        if let Some(items) = node.get_mut("items") {
            reference_schema(items);
        }
    } else if node.get("type").and_then(serde_json::Value::as_str) == Some("integer")
        && let Some(node) = node.as_object_mut()
    {
        node.remove("type");
        node.insert("oneOf".into(), serde_json::json!([{"type":"integer"},{"type":"string"}]));
    }
}

pub(crate) fn drawing_parameters(command: &str) -> &'static [&'static str] {
    match command {
        "drawing.describe" | "drawing.group" | "drawing.ungroup" | "drawing.arrange" | "arrange.align" | "arrange.group" => &["ids"],
        "drawing.transform" | "drawing.format" | "drawing.delete" | "drawing.chart" => &["ids", "id"],
        "drawing.text"
        | "shape.setText"
        | "object.move"
        | "object.delete"
        | "object.setAnchorMode"
        | "arrange.bringForward"
        | "arrange.sendBackward"
        | "arrange.rotate"
        | "ink.toShape"
        | "chart.shapeOutline" => &["id"],
        "drawing.reconnect" => &["id", "to"],
        "drawing.connect" => &["from", "to"],
        "chart.shapeFill" => &["id", "chart"],
        "chart.set"
        | "chart.addElement"
        | "chart.changeColors"
        | "chart.changeType"
        | "chart.delete"
        | "chart.formatSelection"
        | "chart.move"
        | "chart.quickLayout"
        | "chart.selectData"
        | "chart.switchRowColumn" => &["chart"],
        _ => &[],
    }
}

fn types(command: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match command {
        "drawing.connect" => &[("from", SHAPE), ("to", SHAPE)],
        "drawing.reconnect" => &[("id", SHAPE), ("to", SHAPE)],
        "drawing.text" | "shape.setText" | "arrange.rotate" | "chart.shapeOutline" | "ink.toShape" => &[("id", SHAPE)],
        "drawing.chart" => &[("ids", CHART), ("id", CHART)],
        "chart.shapeFill" => &[("id", SHAPE), ("chart", CHART)],
        "drawing.group" => &[("ids", SHAPE_GROUP)],
        "drawing.ungroup" => &[("ids", GROUP)],
        "drawing.arrange" => &[("ids", SHAPE_CHART)],
        "object.delete" | "object.move" | "object.setAnchorMode" => &[("id", ANCHOR)],
        "drawing.delete" | "drawing.format" | "drawing.transform" => &[("ids", SHARED_DRAWING), ("id", SHARED_DRAWING)],
        "arrange.align" | "arrange.group" => &[("ids", ANCHOR)],
        "arrange.bringForward" | "arrange.sendBackward" => &[("id", ANCHOR)],
        id if id.starts_with("chart.") => &[("chart", CHART)],
        _ => &[("ids", DRAWING), ("id", DRAWING)],
    }
}

/// Whether the engine's resolver takes a sheet (as a name, numeric tab index or tool id).
pub(crate) fn sheet_parameter(key: &str) -> bool {
    key == "sheet" || key == "scope"
}

/// Declarations are cached per catalogue shape, rather than leaking slices on every inspect.
pub(crate) fn declare(commands: &mut [KitCommand]) {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Entry = (&'static [&'static str], &'static [&'static str], &'static [(&'static str, &'static [&'static str])]);
    static CACHE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() else { return };
    for command in commands {
        let schema = command.schema.as_ref();
        let has = |key: &str| schema.is_some_and(|schema| schema.get(key).is_some());
        let has_sheet = has("sheet") && !matches!(command.id.as_str(), "data.fromTextCsv" | "data.getData" | "conditionalFormat.clearRules");
        let (objects, places, object_types) = cache.entry(command.id.clone()).or_insert_with(|| {
            let objects: Vec<_> = drawing_parameters(&command.id).iter().copied().filter(|key| has(key) && Some(*key) != command.targets).collect();
            let mut places: Vec<_> = ["sheet", "scope", "table", "pivot", "cell", "range", "rows", "cols"]
                .into_iter()
                .filter(|key| has(key) && (*key != "sheet" || has_sheet))
                .collect();
            places.retain(|key| match command.id.as_str() {
                "edit.undo" | "edit.redo" => *key != "scope",
                "pivot.insert" => !matches!(*key, "rows" | "cols"),
                "table.summarizePivot" => !matches!(*key, "rows" | "cols"),
                "pivot.grandTotals" => !matches!(*key, "rows" | "cols"),
                _ => true,
            });
            match command.id.as_str() {
                "edit.paste" | "edit.pasteSpecial" if has("at") => places.push("at"),
                "sheet.insert" => places.push("before"),
                "sheet.move" => places.push("to"),
                "edit.moveCells" => places.extend(["from", "to"]),
                "drawing.erase" => places.extend(["path", "x", "y"]),
                "name.define" => places.extend(["name", "refersTo"]),
                "name.delete" | "style.apply" | "table.deleteStyle" | "table.setDefaultStyle" => places.push("name"),
                "style.add" => places.extend(["name", "fromCell"]),
                "cell.set" => places.push("name"),
                "view.freezePanes" | "view.freezeRows" | "view.freezeColumns" | "view.freezeTopRow" | "view.freezeFirstColumn" => places.push("top"),
                "page.pageSetup" => places.extend(["printArea", "titleRows", "titleCols", "rowBreaks", "colBreaks"]),
                "edit.autoFill" => places.extend(["source", "target"]),
                "edit.formatPainter" => places.push("apply"),
                "data.sortAscending" | "data.sortDescending" | "data.filterBy" | "table.totalFunction" => places.push("column"),
                "data.sort" => places.push("keys"),
                "data.removeDuplicates" => places.push("columns"),
                "table.removeDuplicates" => places.push("columns"),
                "ink.stroke" => places.push("points"),
                "drawing.ink" => places.push("strokes"),
                "drawing.insert" => places.push("frame"),
                "drawing.reconnect" => places.push("point"),
                "spelling.change" => places.push("word"),
                "data.subtotal" => places.extend(["groupBy", "columns"]),
                "data.textToColumns" => places.push("destination"),
                "table.formatAs" => places.push("style"),
                "table.newStyle" => places.push("replace"),
                "chart.insert" | "chart.recommended" | "chart.set" | "chart.move" | "image.insert" | "shape.insert" | "icon.insert"
                | "textBox.insert" | "object.move" => places.push("at"),
                "sparkline.insert" => places.push("location"),
                "link.insert" => places.push("target"),
                "conditionalFormat.manageRules" => places.extend(["delete", "moveUp", "moveDown"]),
                "pivot.insert" | "pivot.recommended" => places.extend(["source", "range", "destination", "location"]),
                "table.summarizePivot" => places.extend(["destination", "location"]),
                "pivot.changeSource" => places.push("source"),
                "data.goalSeek" => places.extend(["set", "changing"]),
                "data.scenarioAdd" => places.extend(["name", "changing"]),
                "data.scenarioShow" | "data.scenarioDelete" => places.push("name"),
                "data.scenarioSummary" => places.push("resultCells"),
                "data.dataTable" => places.extend(["rowInput", "columnInput"]),
                "data.advancedFilter" => places.extend(["criteria", "copyTo"]),
                "data.consolidate" => places.extend(["sources", "destination"]),
                _ => {}
            }
            let mut seen = std::collections::BTreeSet::new();
            places.retain(|key| has(key.split('.').next().unwrap_or_default().trim_end_matches("[]")) && seen.insert(*key));
            let mut object_types = if drawing_parameters(&command.id).is_empty() { Vec::new() } else { types(&command.id).to_vec() };
            object_types.retain(|(key, _)| has(key));
            if has_sheet {
                object_types.push(("sheet", &["Sheet"]));
            }
            if command.id == "cell.set" {
                object_types.push(("name", &["Name"]));
            }
            (Box::leak(objects.into_boxed_slice()), Box::leak(places.into_boxed_slice()), Box::leak(object_types.into_boxed_slice()))
        });
        command.objects = objects;
        command.places = places;
        command.object_types = object_types;
        if let Some(schema) = command.schema.as_mut() {
            for key in drawing_parameters(&command.id) {
                if let Some(node) = schema.get_mut(*key) {
                    reference_schema(node);
                }
            }
        }
        if has_sheet {
            command.name_params = if command.id == "cell.set" { &[("sheet", "name"), ("name", "name")] } else { &[("sheet", "name")] };
        }
    }
}
