//! Projection of data_insert.rs's sources, destinations, registry and trial rows.
use super::model::{Context, Facts, area};
use crate::paths::Reading;
use kition_sheet_engine::core::{CellRef, RangeRef};
use serde_json::{Value, json};

pub(super) fn expected(row: &Reading, params: &Value, cx: &Context) -> Facts {
    let mut out = Facts::default();
    let command = row.command;
    let si = cx.sheet(params);
    let sid = cx.sid(si);
    if command.starts_with("chart.") {
        super::objects::chart(&mut out, command, params, cx);
        return out;
    }
    if command.starts_with("pivot.")
        || matches!(command, "insert.pivotTable" | "insert.recommendedPivotTables" | "table.summarizePivot" | "data.refreshAll")
    {
        pivot(&mut out, command, params, cx);
        return out;
    }
    if row.holds == "none" || row.holds == "none beyond addressing" {
        return out;
    }
    if command.starts_with("table.") {
        table(&mut out, command, params, cx);
        return out;
    }
    match command {
        "data.validation" => {
            cx.lookup(&mut out, params, si);
            cx.address(&mut out, si, cx.range(params));
            // data.rs::validation subtracts only overlapping rule ranges.
            cx.constraints(&mut out, si, cx.range(params));
            cx.shared(&mut out, si, None, false);
        }
        "data.group" | "data.ungroup" | "data.hideDetail" | "data.showDetail" => outline(&mut out, command, params, cx),
        "data.filter" | "data.filterBy" | "data.clearFilter" | "data.reapply" => filter(&mut out, command, params, cx),
        "data.sortAscending" | "data.sortDescending" | "data.sort" | "data.removeDuplicates" | "data.subtotal" => {
            let range = data_range(cx, params);
            cx.source(&mut out, cx.active, range, true);
            out.add(&cx.sid(cx.active), ["tables", "autofilter", "~sortObjects"]);
            cx.constraints(&mut out, cx.active, range);
            if command != "data.sort" || params.get("keys").is_some() {
                cx.entry(&mut out, cx.active, range, false);
                for row in range.start.row..=range.end.row {
                    cx.line(&mut out, cx.active, true, row, &["info"]);
                }
            }
            if command == "data.subtotal" {
                out.add("workbook", ["names"]);
                cx.shared(&mut out, cx.active, None, true);
            }
        }
        "data.textToColumns" => {
            let source = cx.range(params);
            cx.lookup(&mut out, params, si);
            let source = RangeRef::new(source.start, CellRef::new(source.end.row, source.start.col));
            cx.source(&mut out, si, source, false);
            let start = params.get("destination").and_then(Value::as_str).and_then(area).map_or(source.start, |range| range.start);
            // The retained fixtures contain single-field text. Splitting reads
            // the source strings to compute the output width before entry.
            let target = RangeRef::new(start, start.offset(source.height() as i64 - 1, 0).unwrap());
            cx.entry(&mut out, si, target, true);
        }
        "data.smartFill" => {
            let mut delegated = Reading {
                family: row.family,
                line: row.line,
                command: "edit.smartFill",
                params: params.clone(),
                holds: row.holds,
                reads: row.reads,
                note: row.note,
            };
            out = super::edit::expected(&delegated, params, cx);
            delegated.command = command;
        }
        "insert.table" | "home.formatAsTable" => insert_table(&mut out, command, params, cx),
        "insert.chart" | "insert.recommendedCharts" => {
            let range = if params.get("range").is_some() { cx.range(params) } else { data_range(cx, params) };
            cx.lookup(&mut out, params, si);
            out.add(&sid, ["name"]);
            cx.source(&mut out, si, range, true);
            out.add("workbook", ["~nextObjectId"]);
            cx.shared(&mut out, si, None, false);
        }
        "insert.picture" if params.get("placement").and_then(Value::as_str) == Some("cell") => {
            let at = params.get("at").and_then(Value::as_str).and_then(area).map_or(cx.active_cell, |range| range.start);
            cx.entry(&mut out, cx.active, RangeRef::cell(at), false);
        }
        "insert.picture" | "insert.shape" | "insert.textBox" | "insert.icons" => {
            let at = params.get("at").and_then(Value::as_str).and_then(area).map_or(cx.active_cell, |range| range.start);
            super::objects::insertion(&mut out, cx, cx.active, at, false);
        }
        "insert.artText" => {
            super::objects::insertion(&mut out, cx, cx.active, cx.active_cell, true);
            cx.geometry(&mut out, cx.active, cx.active_cell.row, cx.active_cell.col);
        }
        "shape.setText" | "object.delete" | "object.move" | "object.setAnchorMode" => {
            let id = params["id"].as_u64().unwrap();
            let tool = format!("drawing:{id}");
            match command {
                "shape.setText" => {
                    out.add(&tool, ["look.kind", "text"]);
                    out.add(&cx.sid(cx.active), ["props.protection"]);
                }
                "object.delete" => {
                    out.add(&tool, ["*"]);
                    out.position(&tool, &["*"]);
                }
                "object.move" => {
                    out.add(&tool, ["look.groups", "look.connection"]);
                    out.position(&tool, &["sheet", "frame"]);
                    let at = params.get("at").and_then(Value::as_str).and_then(area).map(|range| range.start).unwrap();
                    cx.address(&mut out, cx.active, RangeRef::cell(at));
                    cx.geometry(&mut out, cx.active, at.row, at.col);
                }
                _ => {
                    out.add(&tool, ["frame.anchor.mode"]);
                    out.position(&tool, &["sheet"]);
                }
            }
            cx.shared(&mut out, cx.active, None, false);
        }
        "insert.sparkline" => {
            let range = params.get("location").and_then(Value::as_str).and_then(area).unwrap();
            cx.address(&mut out, cx.active, range);
            out.add(&cx.sid(cx.active), ["sparklines"]);
            cx.shared(&mut out, cx.active, None, false);
        }
        "insert.link" => link(&mut out, params, cx),
        "review.newNote" | "review.newComment" | "review.replyComment" | "review.resolveComment" | "review.deleteComment" | "review.showNote" => {
            comments(&mut out, command, params, cx)
        }
        "insert.symbol" => {
            cx.entry(&mut out, cx.active, RangeRef::cell(cx.active_cell), true);
            cx.source(&mut out, cx.active, RangeRef::cell(cx.active_cell), false);
        }
        "home.conditionalFormat" | "home.clearRules" | "home.manageRules" => conditional(&mut out, command, params, cx),
        "formulas.autoSum" => autosum(&mut out, params, cx),
        "formulas.insertFunction" => checkbox_enabled(&mut out, cx),
        "formulas.defineName" | "formulas.deleteName" => names(&mut out, command, params, cx),
        "formulas.createFromSelection" => {
            let range = cx.range(params);
            cx.source(&mut out, cx.active, RangeRef::new(range.start, CellRef::new(range.start.row, range.end.col)), false);
            out.add(&cx.sid(cx.active), ["name"]);
            out.add("workbook", ["names"]);
            cx.shared(&mut out, cx.active, None, true);
        }
        "formulas.showFormulas" => {
            out.add(&cx.sid(cx.active), ["props.showFormulas"]);
            cx.shared(&mut out, cx.active, None, false);
        }
        "formulas.calculateNow" | "formulas.calculateSheet" => calculation(&mut out, cx),
        "formulas.calculationOptions" => {
            for (key, field) in [("mode", "mode"), ("iterative", "iterative"), ("maxIterations", "max_iterations"), ("maxChange", "max_change")] {
                if params.get(key).is_some() || key == "mode" {
                    out.add("workbook", [format!("props.calc.{field}")]);
                }
            }
            // calc_options touches workbook settings; only header sync can
            // change sheets (formulas.rs:515; cmd/mod.rs:282).
            out.add("workbook", ["~headerSync"]);
        }
        "data.goalSeek"
        | "data.dataTable"
        | "data.scenarioAdd"
        | "data.scenarioShow"
        | "data.scenarioDelete"
        | "data.scenarioSummary"
        | "data.advancedFilter"
        | "data.consolidate" => trials(&mut out, command, params, cx),
        _ => panic!("unprojected data row {command} {params}"),
    }
    out
}

fn data_range(cx: &Context, params: &Value) -> RangeRef {
    if let Some(range) = params.get("range").and_then(Value::as_str).and_then(area) {
        return range;
    }
    if !cx.selected.is_single() {
        return cx.selected;
    }
    cx.wb.sheets[cx.active]
        .table_at(cx.active_cell)
        .map(|table| table.range)
        .unwrap_or_else(|| kition_sheet_engine::selection::current_region(&cx.wb.sheets[cx.active], cx.active_cell))
}

fn outline(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let rows = params.get("cols").is_none();
    let range = params.get(if rows { "rows" } else { "cols" }).and_then(Value::as_str).and_then(area).unwrap_or(cx.selected);
    let (mut lo, mut hi) = if rows { (range.start.row, range.end.row) } else { (range.start.col, range.end.col) };
    let lines = if rows { &cx.wb.sheets[cx.active].rows } else { &cx.wb.sheets[cx.active].cols };
    let detail = matches!(command, "data.hideDetail" | "data.showDetail");
    let level = (lo..=hi).filter_map(|i| lines.get(&i).map(|info| info.outline)).max().unwrap_or(0);
    if detail && level > 0 {
        while lo > 0 && lines.get(&(lo - 1)).is_some_and(|info| info.outline >= level) {
            lo -= 1;
        }
        while lines.get(&(hi + 1)).is_some_and(|info| info.outline >= level) {
            hi += 1;
        }
    }
    let first = if detail && level > 0 { lo.saturating_sub(1) } else { lo };
    let last = if detail && level > 0 { hi + 1 } else { hi };
    out.window(&cx.sid(cx.active), if rows { "rows" } else { "cols" }, first as usize, last as usize + 1);
    for i in first..=last {
        let mut fields = vec!["~outline"];
        if detail && level > 0 && (lo..=hi).contains(&i) || command == "data.ungroup" && lines.get(&i).is_none_or(|info| info.outline <= 1) {
            fields.push("~hidden");
        }
        cx.line(out, cx.active, rows, i, &fields);
    }
    out.add(&cx.sid(cx.active), ["props.protection"]);
    cx.shared(out, cx.active, None, false);
}

fn filter(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let si = cx.active;
    let sheet = &cx.wb.sheets[si];
    let sid = cx.sid(si);
    let table = params
        .get("table")
        .and_then(Value::as_str)
        .and_then(|name| sheet.tables.iter().find(|table| table.name.eq_ignore_ascii_case(name)))
        .or_else(|| sheet.table_at(params.get("cell").and_then(Value::as_str).and_then(area).map_or(cx.active_cell, |r| r.start)));
    out.add(&sid, ["autofilter", "~filterScopes", "props.protection"]);
    if let Some(table) = table {
        out.add(&sid, [format!("tables[name={}]", json!(table.name))]);
        if command == "data.filter" {
            cx.constraints(out, si, RangeRef::cell(cx.active_cell));
        } else {
            cx.address(out, si, table.range);
            if command == "data.filterBy" {
                cx.source(out, si, table.range, true);
                out.add(&sid, ["~hiddenCount"]);
            }
            for row in table.range.start.row + 1..=table.range.end.row {
                cx.line(out, si, true, row, &["info.hidden"]);
            }
        }
    }
    cx.shared(out, si, None, false);
}

fn table(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    if matches!(command, "table.newStyle" | "table.deleteStyle" | "table.setDefaultStyle") {
        let name = params.get("replace").or_else(|| params.get("name")).and_then(Value::as_str).unwrap();
        out.add("workbook", [format!("props.tableStyles.custom[name={} ]", json!(name)), "props.tableStyles.default_table".into()]);
        if command != "table.setDefaultStyle" {
            // insert.rs::new/delete_table_style only touch sheets using the
            // replaced style; structural edits still synchronize headers.
            out.add("workbook", ["~headerSync"]);
            if command == "table.deleteStyle" || params.get("replace").is_some() {
                out.add("workbook", ["~tabs"]);
                for (owner, sheet) in cx.wb.sheets.iter().enumerate() {
                    out.add(&cx.sid(owner), ["tables"]);
                    if sheet.tables.iter().any(|table| table.style.eq_ignore_ascii_case(name)) {
                        cx.shared(out, owner, None, false);
                    }
                }
            }
        }
        return;
    }
    let (si, ti) = params
        .get("table")
        .and_then(Value::as_str)
        .and_then(|name| cx.wb.table(name))
        .unwrap_or_else(|| (cx.active, cx.wb.sheets[cx.active].tables.iter().position(|table| table.range.contains(cx.active_cell)).unwrap()));
    let table = &cx.wb.sheets[si].tables[ti];
    let sid = cx.sid(si);
    let field = format!("tables[name={}]", json!(table.name));
    out.add(&sid, [field.clone()]);
    match command {
        "table.headerRow" => {
            let on = params.get("on").and_then(Value::as_bool).unwrap_or(!table.header_row);
            if !on {
                // insert.rs::header_row retains column definitions; the shared
                // structural tail handles pre-existing stale header names.
                out.0.retain(|fact| fact.field != field);
                let value = serde_json::to_value(table).unwrap();
                out.add(&sid, value.as_object().unwrap().keys().filter(|key| key.as_str() != "columns").map(|key| format!("{field}.{key}")));
            }
            if on != table.header_row {
                let row = if on { table.range.start.row - 1 } else { table.range.start.row };
                let range = RangeRef::new(CellRef::new(row, table.range.start.col), CellRef::new(row, table.range.end.col));
                table_entry(out, cx, si, range);
                cx.shared(out, si, None, true);
            }
        }
        "table.totalRow" | "table.totalFunction" => {
            let on = params.get("on").and_then(Value::as_bool).unwrap_or(!table.totals_row);
            if command == "table.totalFunction" && !table.totals_row {
                out.add(&sid, ["props.protection"]);
                cx.shared(out, si, None, true);
                return;
            }
            let row = if command == "table.totalFunction" || !on { table.range.end.row } else { table.range.end.row + 1 };
            let range = RangeRef::new(CellRef::new(row, table.range.start.col), CellRef::new(row, table.range.end.col));
            table_entry(out, cx, si, range);
            // SUBTOTAL reads the table body, excluding its header row.
            if on {
                cx.source(
                    out,
                    si,
                    RangeRef::new(CellRef::new(table.range.start.row + u32::from(table.header_row), table.range.start.col), table.range.end),
                    true,
                );
            }
            cx.shared(out, si, None, true);
        }
        "table.rename" => {
            out.add("workbook", ["~tableNames"]);
            out.add(&format!("table:{}:{}", cx.wb.sheets[si].sheet_id, table.id), ["~formulaInputs"]);
            cx.shared(out, si, None, true);
        }
        "table.convertToRange" => {
            cx.address(out, si, table.range);
            for at in table.range.iter() {
                cx.inherited_style(out, si, at);
            }
            out.add("workbook", ["props.tableStyles", "theme"]);
            for row in table.range.start.row..=table.range.end.row {
                cx.line(out, si, true, row, &["info.hidden"]);
            }
            for col in table.range.start.col..=table.range.end.col {
                cx.line(out, si, false, col, &["info.hidden"]);
            }
            cx.fit(out, si, table.range);
            cx.shared(out, si, None, false);
        }
        "table.bandedRows" | "table.bandedColumns" | "table.filterButton" | "table.firstColumn" | "table.lastColumn" | "table.style" => {
            cx.shared(out, si, None, false)
        }
        _ => panic!("unprojected table {command}"),
    }
}

fn table_entry(out: &mut Facts, cx: &Context, si: usize, range: RangeRef) {
    cx.address(out, si, range);
    out.add(&cx.sid(si), ["props.protection"]);
    for at in range.iter() {
        out.add(&cx.cell(si, at), ["input", "text", "array", "picture"]);
        cx.inherited_style(out, si, at);
    }
    out.add("workbook", ["props.dateSystem"]);
}

fn insert_table(out: &mut Facts, _command: &str, params: &Value, cx: &Context) {
    let si = cx.sheet(params);
    let range = if params.get("range").is_some() { cx.range(params) } else { data_range(cx, params) };
    cx.lookup(out, params, si);
    out.add(&cx.sid(cx.active), ["props.protection"]);
    out.add(&cx.sid(si), ["props.protection", "~arrays"]);
    cx.address(out, si, range);
    cx.constraints(out, si, range);
    let header = RangeRef::new(range.start, CellRef::new(range.start.row, range.end.col));
    cx.entry(out, si, header, false);
    if params.get("name").is_some() {
        for field in ["~lookupName", "name", "range"] {
            out.fact("call", "covers", field);
        }
    } else {
        out.add("workbook", ["~tableNames", "names"]);
    }
    if params.get("style").is_none() {
        out.add("workbook", ["props.tableStyles.default_table"]);
    }
    if params.get("mode").and_then(Value::as_str) == Some("format") {
        cx.source(out, si, range, true);
        out.add("workbook", ["props.tableStyles", "theme"]);
    }
    cx.shared(out, si, None, true);
}

fn checkbox_enabled(out: &mut Facts, cx: &Context) {
    let range = RangeRef::cell(cx.active_cell);
    cx.address(out, cx.active, range);
    cx.constraints(out, cx.active, range);
    // extra.rs::is_checkbox uses the materialized cell's style, then the
    // workbook default, rather than row/column inherited styles.
    out.add(&cx.cell(cx.active, cx.active_cell), ["style"]);
    if cx.wb.sheets[cx.active].cell(cx.active_cell).is_none_or(|cell| cell.style.0 == 0) {
        out.add("workbook", ["props.defaultStyle"]);
    }
}

fn link(out: &mut Facts, params: &Value, cx: &Context) {
    checkbox_enabled(out, cx);
    let si = cx.sheet(params);
    let range = cx.range(params);
    cx.lookup(out, params, si);
    cx.address(out, si, range);
    map_entry(out, cx, si, "hyperlinks", range.start);
    if params.get("remove").and_then(Value::as_bool) != Some(true) {
        cx.entry(out, si, range, true);
    }
    cx.shared(out, si, Some(range), false);
}

fn map_entry(out: &mut Facts, cx: &Context, si: usize, field: &str, at: CellRef) {
    out.add(&cx.sid(si), [format!("{field}.row:{}:col:{}", cx.ids.sheets[si].rows[at.row as usize], cx.ids.sheets[si].cols[at.col as usize])]);
}

fn comments(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let si = cx.sheet(params);
    cx.lookup(out, params, si);
    let range = if command == "review.deleteComment" {
        cx.selected
    } else {
        RangeRef::cell(params.get("cell").and_then(Value::as_str).and_then(area).map_or(cx.active_cell, |r| r.start))
    };
    cx.address(out, si, range);
    for at in range.iter() {
        map_entry(out, cx, si, "comments", at);
    }
    cx.shared(out, si, None, false);
}

fn conditional(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let si = cx.sheet(params);
    cx.lookup(out, params, si);
    out.add(&cx.sid(si), ["condFormats"]);
    if command == "home.conditionalFormat" {
        cx.address(out, si, cx.range(params));
    }
    if command == "home.clearRules" {
        out.fact("call", "covers", "~conditionalRanges");
        for rule in &cx.wb.sheets[si].cond_formats {
            if rule.ranges.iter().any(|range| range.intersects(&cx.range(params))) {
                for range in &rule.ranges {
                    cx.line(out, si, true, range.start.row, &["~conditionalAreas"]);
                }
            }
        }
    }
    cx.shared(out, si, None, false);
}

fn autosum(out: &mut Facts, params: &Value, cx: &Context) {
    let range = cx.range(params);
    if !range.is_single() {
        cx.source(out, cx.active, range, false);
        let start = CellRef::new(range.end.row, range.start.col);
        cx.entry(out, cx.active, RangeRef::new(start, range.end), true);
    } else {
        let scan = RangeRef::new(CellRef::new(0, range.start.col), range.end);
        cx.source(out, cx.active, scan, false);
        if params.get("enter").and_then(Value::as_bool) != Some(false) {
            cx.entry(out, cx.active, range, true);
        }
    }
}

fn names(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let name = params["name"].as_str().unwrap();
    let matches: Vec<_> = cx.wb.names.iter().enumerate().filter(|(_, definition)| definition.name.eq_ignore_ascii_case(name)).collect();
    if matches.is_empty() {
        out.add("workbook", ["names"]);
    }
    for (i, definition) in matches {
        out.add("workbook", [format!("names[name={}]", json!(definition.name))]);
        out.add(&format!("name:{}", cx.ids.names[i]), ["name", "scope", "formula", "props"]);
    }
    if let Some(si) = params.get("scope").and_then(Value::as_str).and_then(|name| cx.wb.sheet_index(name)) {
        out.add(&cx.sid(si), ["name"]);
    }
    if command == "formulas.defineName" && params.get("refersTo").is_none() {
        out.add(&cx.sid(cx.active), ["name"]);
        cx.address(out, cx.active, cx.selected);
    }
    // Defining/deleting a name is structural but touches no sheet directly.
    out.add("workbook", ["~headerSync"]);
}

fn calculation(out: &mut Facts, cx: &Context) {
    out.add("workbook", ["names", "props.calc", "props.dateSystem"]);
    for (si, sheet) in cx.wb.sheets.iter().enumerate() {
        out.add(&cx.sid(si), ["name", "tables", "validations"]);
        for (at, _) in sheet.cells.iter() {
            cx.source(out, si, RangeRef::cell(at), false);
        }
    }
}

fn pivot(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    let si = cx.active;
    let source = params.get("source").and_then(Value::as_str).unwrap_or("Sales");
    if command != "pivot.delete" {
        if let Some((owner, ti)) = cx.wb.table(source) {
            let table = &cx.wb.sheets[owner].tables[ti];
            out.add(&cx.sid(owner), [format!("tables[name={}]", json!(table.name))]);
            cx.source(out, owner, table.range, true);
        } else if let Some(range) = area(source) {
            let owner = source.rsplit_once('!').and_then(|(name, _)| cx.wb.sheet_index(name)).unwrap_or(si);
            out.add(&cx.sid(owner), ["name"]);
            cx.source(out, owner, range, true);
        }
    }
    for (owner, sheet) in cx.wb.sheets.iter().enumerate() {
        for pivot in &sheet.pivots {
            out.add(&cx.sid(owner), [format!("pivots[name={}]", json!(pivot.name))]);
            if let Some(range) = pivot.last_range {
                cx.entry(out, owner, range, false);
                for col in range.start.col..=range.end.col {
                    cx.line(out, owner, false, col, &["info.size", "info.hidden"]);
                }
            }
        }
    }
    if matches!(command, "insert.pivotTable" | "insert.recommendedPivotTables" | "table.summarizePivot") {
        if command == "insert.recommendedPivotTables" && params.get("apply").is_none() {
            return;
        }
        out.add("workbook", ["~nextObjectId"]);
        if let Some(destination) = params.get("destination").and_then(Value::as_str) {
            let owner = destination.rsplit_once('!').and_then(|(name, _)| cx.wb.sheet_index(name)).unwrap_or(si);
            cx.entry(out, owner, area(destination).unwrap(), false);
        } else {
            out.add("workbook", ["~tabs", "props.protectedStructure"]);
        }
    }
    cx.shared(out, si, None, true);
}

fn trials(out: &mut Facts, command: &str, params: &Value, cx: &Context) {
    match command {
        "data.scenarioAdd" | "data.scenarioDelete" | "data.scenarioShow" => {
            out.add("workbook", ["names"]);
            out.add(&cx.sid(cx.active), ["name"]);
            if command == "data.scenarioAdd" {
                let text = params["changing"].as_str().unwrap();
                let si = text.rsplit_once('!').and_then(|(name, _)| cx.wb.sheet_index(name)).unwrap_or(cx.active);
                out.add(&cx.sid(si), ["name"]);
                let range = area(text).unwrap();
                cx.address(out, si, range);
                if params.get("values").is_none() {
                    cx.source(out, si, range, false);
                }
            } else if command == "data.scenarioShow" {
                cx.entry(out, cx.active, RangeRef::parse("B2:B3").unwrap(), true);
            }
        }
        "data.consolidate" => {
            for source in params["sources"].as_array().unwrap() {
                let text = source.as_str().unwrap();
                let si = text.rsplit_once('!').and_then(|(name, _)| cx.wb.sheet_index(name)).unwrap_or(cx.active);
                cx.source(out, si, area(text).unwrap(), params.get("topRow").is_some());
                if text.contains('!') {
                    out.add(&cx.sid(si), ["name"]);
                }
            }
            let text = params.get("destination").and_then(Value::as_str);
            let si = text.and_then(|text| text.rsplit_once('!')).and_then(|(name, _)| cx.wb.sheet_index(name)).unwrap_or(cx.active);
            let start = text.and_then(area).map_or(cx.active_cell, |range| range.start);
            cx.entry(out, si, RangeRef::new(start, start.offset(4, 2).unwrap()), false);
        }
        "data.advancedFilter" => {
            for key in ["range", "criteria"] {
                if let Some(text) = params.get(key).and_then(Value::as_str) {
                    let si = text.rsplit_once('!').and_then(|(name, _)| cx.wb.sheet_index(name)).unwrap_or(cx.active);
                    cx.source(out, si, area(text).unwrap(), true);
                }
            }
            out.add(&cx.sid(cx.active), ["rows"]);
        }
        "data.goalSeek" | "data.dataTable" | "data.scenarioSummary" => {
            calculation(out, cx);
            if command == "data.goalSeek" {
                let text = params["changing"].as_str().unwrap();
                let si = text.rsplit_once('!').and_then(|(name, _)| cx.wb.sheet_index(name)).unwrap_or(cx.active);
                cx.entry(out, si, area(text).unwrap(), false);
            } else if command == "data.dataTable" {
                let range = cx.range(params);
                cx.entry(out, cx.sheet(params), range, false);
            } else {
                out.add("workbook", ["~tabs", "props.defaultStyle"]);
            }
        }
        _ => panic!("unprojected trial {command}"),
    }
    cx.shared(out, cx.active, None, true);
}
