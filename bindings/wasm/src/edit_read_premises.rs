//! Edit reads beyond the destination: workbook protection and inherited neighbour formats.
use crate::{
    SheetDocument, ids,
    place::{area, premise, window},
};
use kition_kit::{DerivedValues, Under};
use kition_sheet_engine::model::StyleId;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl SheetDocument {
    /// Returns false when a source lies beyond materialized axes and needs the caller's
    /// conservative fallback. All positions here are before the insertion.
    pub(crate) fn add_edit_read_premises(&self, command: &str, params: &Value, si: usize, under: &mut Under) -> Result<bool, String> {
        if matches!(command, "home.insertSheet" | "home.deleteSheet" | "sheet.move") {
            under.premises.push(premise("workbook", ["props.protectedStructure"]));
        }
        let copy = command == "sheet.move" && params.get("copy").and_then(Value::as_bool) == Some(true);
        let state = self.doc_state()?;
        if matches!(command, "home.insertSheet" | "sheet.rename") || copy {
            // The tab digest plus each existing name covers a newly added
            // sheet. Holding the owning list itself would include child edits
            // in the pinned SDK. Tab reordering remains conservative here.
            under.premises.push(premise("workbook", ["~tabs"]));
            for sheet in &state.wb.sheets {
                under.premises.push(premise(ids::sheet_tool_id(sheet), ["name"]));
            }
        }
        if matches!(command, "home.deleteSheet" | "sheet.hide") {
            under.premises.push(premise("workbook", ["~tabs"]));
            for sheet in &state.wb.sheets {
                under.premises.push(premise(ids::sheet_tool_id(sheet), ["props.visibility"]));
            }
        }
        if copy {
            let Some(sheet) = state.wb.sheet(si) else { return Ok(false) };
            under.premises.push(premise("workbook", ["~nextObjectId"]));
            if !sheet.tables.is_empty() {
                under.premises.push(premise("workbook", ["~tableNames", "names"]));
            }
            let Some(kdm) = state.kdm() else { return Ok(false) };
            let Some(axes) = kdm.model().ids().sheets.get(si) else { return Ok(false) };
            under.premises.push(kition_kit::Premise::on(ids::sheet_tool_id(sheet)));
            let names = kition_kit::kdm::tool_ids::ToolIds::of_replica(kdm.replica());
            for raw in axes.rows.iter().chain(&axes.cols).chain(&axes.images).chain(&axes.shapes).chain(&axes.charts) {
                if under.premises.len() >= 100_000 {
                    return Ok(false);
                }
                under.premises.push(kition_kit::Premise::on(names.show(raw)));
            }
            for (at, _) in sheet.cells.iter() {
                if under.premises.len() >= 100_000 {
                    return Ok(false);
                }
                under.premises.push(kition_kit::Premise::on(format!("cell:{}!{at}", sheet.sheet_id)));
            }
        }
        if !matches!(command, "home.insertRows" | "home.insertColumns") {
            return Ok(true);
        }
        let rows = command == "home.insertRows";
        let key = if rows { "rows" } else { "cols" };
        let after = params.get(if rows { "below" } else { "right" }).and_then(Value::as_bool).unwrap_or(false);
        let Some(sheet) = state.wb.sheet(si) else { return Ok(false) };
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(false) };
        let lines = if rows { &axes.rows } else { &axes.cols };
        let ranges = match params.get(key).or_else(|| params.get("range")).and_then(Value::as_str) {
            Some(text) => match area(text) {
                Some(range) => vec![range],
                None => return Ok(false),
            },
            None => state.selection.ranges.clone(),
        };
        let mut spans: Vec<_> =
            ranges.into_iter().map(|range| if rows { (range.start.row, range.end.row) } else { (range.start.col, range.end.col) }).collect();
        spans.sort_unstable();
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for (first, last) in spans {
            match runs.last_mut() {
                Some(previous) if first <= previous.1 => previous.1 = previous.1.max(last),
                _ => runs.push((first, last)),
            }
        }
        let sid = ids::sheet_tool_id(sheet);
        for (first, last) in runs {
            let at = if after { last.saturating_add(1) } else { first };
            let source = if after { Some(at) } else { at.checked_sub(1) };
            if let Some(source) = source {
                let Some(raw) = lines.get(source as usize) else { return Ok(false) };
                under.premises.push(premise(format!("{}:{raw}", if rows { "row" } else { "col" }), ["~insertFormat"]).positions(["index"]));
                window(under, &sid, key, source as usize, source as usize + 1);
            }
            // The new lines take the lower outline level of the two original neighbours.
            for neighbor in [at.checked_sub(1), Some(at)].into_iter().flatten() {
                let Some(raw) = lines.get(neighbor as usize) else { return Ok(false) };
                under.premises.push(premise(format!("{}:{raw}", if rows { "row" } else { "col" }), ["~outline"]).positions(["index"]));
            }
            if let Some(before) = at.checked_sub(1) {
                let Some(raw) = lines.get(before as usize) else { return Ok(false) };
                under.premises.push(premise(format!("{}:{raw}", if rows { "row" } else { "col" }), ["~insertBoundary"]).positions(["index"]));
            }
        }
        Ok(true)
    }

    pub(crate) fn edit_read_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        let formats = names.is_empty() || names.iter().any(|name| name == "insertFormat");
        let boundaries = names.is_empty() || names.iter().any(|name| name == "insertBoundary");
        if !formats && !boundaries {
            return Ok(());
        }
        let state = self.doc_state()?;
        let Some(kdm) = state.kdm() else { return Ok(()) };
        let mut slots: BTreeMap<_, _> = found.iter().enumerate().map(|(index, value)| (value.id.to_string(), index)).collect();
        for (si, axes) in kdm.model().ids().sheets.iter().enumerate() {
            let Some(sheet) = state.wb.sheet(si) else { continue };
            for (rows, lines) in [(true, &axes.rows), (false, &axes.cols)] {
                let mut cell_formats: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
                if formats {
                    for (at, cell) in sheet.cells.iter().filter(|(_, cell)| cell.style != StyleId::DEFAULT) {
                        let mut style = state.wb.styles.get(cell.style).clone();
                        style.checkbox = false;
                        cell_formats.entry(if rows { at.row } else { at.col }).or_default().push(json!([if rows { at.col } else { at.row }, style]));
                    }
                }
                let mut boundary_rules: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
                if boundaries {
                    for rule in &sheet.cond_formats {
                        let ends: BTreeSet<_> = rule.ranges.iter().map(|range| if rows { range.end.row } else { range.end.col }).collect();
                        let value = serde_json::to_value(rule).map_err(|error| error.to_string())?;
                        for end in ends {
                            boundary_rules.entry(end).or_default().push(value.clone());
                        }
                    }
                }
                for (index, raw) in lines.iter().enumerate() {
                    let id = json!(format!("{}:{raw}", if rows { "row" } else { "col" }));
                    if !requested.is_empty() && !requested.contains(&id) {
                        continue;
                    }
                    let line = index as u32;
                    let mut values = serde_json::Map::new();
                    if formats {
                        let info = if rows { sheet.rows.get(&line) } else { sheet.cols.get(&line) };
                        let cells = cell_formats.get(&line);
                        let style = info.and_then(|info| info.style).map(|id| {
                            let mut style = state.wb.styles.get(id).clone();
                            style.checkbox = false;
                            style
                        });
                        let value = json!([info.and_then(|info| info.size), info.is_some_and(|info| info.custom), style, cells]);
                        let digest = kition_kit::session::sha256_hex(&serde_json::to_vec(&value).map_err(|error| error.to_string())?);
                        values.insert("insertFormat".into(), json!(digest));
                    }
                    if boundaries {
                        let digest =
                            kition_kit::session::sha256_hex(&serde_json::to_vec(&boundary_rules.get(&line)).map_err(|error| error.to_string())?);
                        values.insert("insertBoundary".into(), json!(digest));
                    }
                    if let Some(existing) = slots.get(&id.to_string()).and_then(|index| found.get_mut(*index)) {
                        existing.values.extend(values);
                    } else {
                        slots.insert(id.to_string(), found.len());
                        found.push(DerivedValues { id, values });
                    }
                }
            }
        }
        Ok(())
    }
}
