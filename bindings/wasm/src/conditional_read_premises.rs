//! Rule operations hold identities, overlap decisions, and the ranges they remove.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise, window},
};
use kition_kit::{Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, Under};
use kition_sheet_engine::core::RangeRef;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn bounded_ranges(sheet: &kition_sheet_engine::model::Sheet) -> Option<Vec<Vec<RangeRef>>> {
    let count = sheet.cond_formats.iter().try_fold(0usize, |count, rule| count.checked_add(rule.ranges.len()))?;
    (count <= 100_000 && sheet.cond_formats.len() <= 100_000).then(|| sheet.cond_formats.iter().map(|rule| rule.ranges.clone()).collect())
}

fn row_areas(rules: &[Vec<RangeRef>]) -> Option<BTreeMap<u32, Vec<(usize, &[RangeRef])>>> {
    let mut by_row = BTreeMap::<u32, Vec<_>>::new();
    let mut count = 0usize;
    for (index, areas) in rules.iter().enumerate() {
        let starts: BTreeSet<_> = areas.iter().map(|range| range.start.row).collect();
        count = count.checked_add(areas.len().checked_mul(starts.len())?)?;
        if count > 100_000 {
            return None;
        }
        for row in starts {
            by_row.entry(row).or_default().push((index, areas.as_slice()));
        }
    }
    Some(by_row)
}

fn targets(params: &Value, fallback: &[RangeRef]) -> Option<Vec<RangeRef>> {
    if let Some(text) = params.get("range").or_else(|| params.get("cell")).and_then(Value::as_str) {
        let body = text.rsplit_once('!').map_or(text, |(_, body)| body);
        body.split(',').map(area).collect()
    } else {
        Some(fallback.to_vec())
    }
}

fn intersections(rules: &[Vec<RangeRef>], targets: &[RangeRef]) -> Option<Vec<Vec<bool>>> {
    let areas = rules.iter().try_fold(0usize, |count, rule| count.checked_add(rule.len()))?;
    if rules.len() > 100_000 || areas > 100_000 || targets.len() > 100_000 || areas.checked_mul(targets.len())? > 100_000 {
        return None;
    }
    Some(rules.iter().map(|ranges| ranges.iter().map(|range| targets.iter().any(|target| target.intersects(range))).collect()).collect())
}

fn before_intersections(value: &Value, targets: &[RangeRef]) -> Option<Vec<Vec<bool>>> {
    let rows = value.as_array().filter(|rows| rows.len() <= 100_000)?;
    let areas = rows.iter().try_fold(0usize, |count, row| count.checked_add(row.as_array()?.len()))?;
    if areas > 100_000 || targets.len() > 100_000 || areas.checked_mul(targets.len())? > 100_000 {
        return None;
    }
    let ranges: Vec<Vec<RangeRef>> = serde_json::from_value(value.clone()).ok()?;
    intersections(&ranges, targets)
}

impl SheetDocument {
    pub(crate) fn conditional_read_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        if !names.is_empty() && !names.iter().any(|name| matches!(name.as_str(), "conditionalRanges" | "conditionalAreas")) {
            return Ok(());
        }
        let requested: BTreeSet<_> = requested.iter().filter_map(Value::as_str).collect();
        let mut slots: BTreeMap<_, _> =
            found.iter().enumerate().filter_map(|(index, value)| value.id.as_str().map(|id| (id.to_owned(), index))).collect();
        let state = self.doc_state()?;
        let capture =
            (requested.is_empty() || requested.contains("axis:all")) && (names.is_empty() || names.iter().any(|name| name == "conditionalRanges"));
        let mut coverage = serde_json::Map::new();
        let mut coverage_size = 0usize;
        for (si, sheet) in state.wb.sheets.iter().enumerate() {
            let tool = ids::sheet_tool_id(sheet);
            let Some(ranges) = bounded_ranges(sheet) else { continue };
            if capture {
                coverage_size = coverage_size.saturating_add(1).saturating_add(ranges.iter().map(Vec::len).sum::<usize>());
                if coverage_size > 100_000 {
                    return Err("conditional coverage exceeds the premise limit".into());
                }
                coverage.insert(tool.clone(), json!(ranges));
            }
            let mut produced = Vec::new();
            if (requested.is_empty() || requested.contains(tool.as_str()))
                && (names.is_empty() || names.iter().any(|name| name == "conditionalRanges"))
            {
                produced.push((tool, "conditionalRanges", json!(ranges)));
            }
            if names.is_empty() || names.iter().any(|name| name == "conditionalAreas") {
                let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { continue };
                let Some(by_row) = row_areas(&ranges) else { continue };
                for (row, areas) in by_row {
                    let Some(raw) = axes.rows.get(row as usize) else { continue };
                    let id = format!("row:{raw}");
                    if requested.is_empty() || requested.contains(id.as_str()) {
                        produced.push((id, "conditionalAreas", json!(areas)));
                    }
                }
            }
            for (tool, name, value) in produced {
                if let Some(previous) = slots.get(&tool).and_then(|index| found.get_mut(*index)) {
                    previous.values.insert(name.into(), value);
                } else {
                    slots.insert(tool.clone(), found.len());
                    found.push(DerivedValues { id: json!(tool), values: [(name.into(), value)].into_iter().collect() });
                }
            }
        }
        if capture {
            // A separate holder keeps coverage history independent of stored
            // sheet edits when the SDK journals private facts separately.
            if let Some(existing) = found.iter_mut().find(|value| value.id == "axis:all") {
                existing.values.insert("conditionalRanges".into(), Value::Object(coverage));
            } else {
                found.push(DerivedValues {
                    id: json!("axis:all"),
                    values: [("conditionalRanges".into(), Value::Object(coverage))].into_iter().collect(),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn conditional_read_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if !matches!(command, "home.manageRules" | "home.clearRules") {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(ranges) = bounded_ranges(sheet) else { return Ok(None) };
        let sid = ids::sheet_tool_id(sheet);
        let mut fields = vec!["condFormats"];
        if params.get("sheet").is_some_and(Value::is_string)
            || ["range", "cell"].iter().any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|text| text.contains('!')))
        {
            fields.push("name");
        }
        let mut under = Under::default();
        under.objects.push(json!(sid));
        under.premises.push(premise(sid.clone(), fields));
        if let Some(index) = params.get("sheet").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) {
            window(&mut under, "workbook", "sheets", index, index.saturating_add(1));
            // `covers` replaces window processing for range clears; a numeric sheet
            // still reads the sheet's tab position.
            under.premises.push(premise(sid, Vec::<String>::new()).positions(["sheets"]));
        }
        if command == "home.clearRules" && params.get("sheet").and_then(Value::as_bool) != Some(true) {
            let Some(targets) = targets(params, &state.selection.ranges) else { return Ok(None) };
            if targets.len() > 100_000 || intersections(&ranges, &targets).is_none() || row_areas(&ranges).is_none() {
                return Ok(None);
            }
            under.covers = Some(Covers { fields: vec!["~conditionalRanges".into()] });
            // same-field decision: removal holds each selected rule's
            // numeric ranges, even when a shift leaves its intersection unchanged.
            // Encoded KDM endpoints alone follow row identities through that shift.
            let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
            let starts: BTreeSet<_> = sheet
                .cond_formats
                .iter()
                .filter(|rule| rule.ranges.iter().any(|range| targets.iter().any(|target| target.intersects(range))))
                .flat_map(|rule| rule.ranges.iter().map(|range| range.start.row))
                .collect();
            for row in starts {
                let Some(raw) = axes.rows.get(row as usize) else { return Ok(None) };
                under.premises.push(premise(format!("row:{raw}"), ["~conditionalAreas"]).positions(Vec::<String>::new()));
            }
        } else if command == "home.clearRules" {
            // The whole-sheet clear replaces every rule area.
            under.premises.push(premise(ids::sheet_tool_id(sheet), ["~conditionalRanges"]));
        }
        merged(&mut under);
        Ok(Some(under))
    }

    pub(crate) fn conditional_covered(&self, command: &str, given: &Value, objects: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
        if command != "home.clearRules" || given.get("sheet").and_then(Value::as_bool) == Some(true) {
            return Ok(None);
        }
        let mut params = given.clone();
        let implied = self.take_tool_ids(command, &mut params, self.command_schema(command).as_ref())?;
        let state = self.doc_state()?;
        let si = self.command_target_sheet(command, &params, implied).unwrap_or(state.wb.active_sheet);
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(now) = bounded_ranges(sheet) else { return Ok(None) };
        let Some(targets) = targets(&params, &state.selection.ranges).filter(|targets| targets.len() <= 100_000) else { return Ok(None) };
        let sid = json!(ids::sheet_tool_id(sheet));
        let Some(decisions) = intersections(&now, &targets) else { return Ok(None) };
        let covered = objects
            .iter()
            .map(|ask| {
                let coverage_holder = ask.id == "axis:all";
                let before = ask
                    .before
                    .get("~conditionalRanges")
                    .and_then(|before| if coverage_holder { before.get(ids::sheet_tool_id(sheet)) } else { Some(before) });
                let covered = if ask.id != sid && !coverage_holder {
                    Some(false)
                } else if ask.created || ask.deleted || ask.unknown.iter().any(|field| matches!(field.as_str(), "~conditionalRanges" | "$was")) {
                    None
                } else if let Some(before) = before {
                    before_intersections(before, &targets).filter(|before| *before == decisions).map(|_| false)
                    // The pinned SDK has no rule-area objects to name when an
                    // intersection changes. Refuse as unknown rather than pretend
                    // the sheet itself moved into or out of the call's place.
                } else if coverage_holder {
                    None
                } else {
                    Some(false)
                };
                Covered { id: ask.id.clone(), covered, ..Default::default() }
            })
            .collect();
        Ok(Some(CoveredAt { under: vec![sid], covered }))
    }
}
