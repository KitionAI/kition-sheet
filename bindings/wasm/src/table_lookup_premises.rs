//! Explicit table names depend on duplicate predicates and overlapping extents.
use crate::{SheetDocument, place::premise};
use kition_kit::{Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, Under};
use kition_sheet_engine::core::RangeRef;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

struct Lookup {
    name: String,
    area: Option<(u32, RangeRef)>,
}

fn name_key(name: &str) -> String {
    kition_kit::session::sha256_hex(name.to_ascii_lowercase().as_bytes())
}

impl Lookup {
    fn matches(&self, name: &str, sheet: u32, range: RangeRef) -> bool {
        self.name == name || self.area.is_some_and(|(owner, area)| owner == sheet && area.intersects(&range))
    }
}

impl SheetDocument {
    fn table_lookups(&self) -> Result<Option<BTreeMap<String, Lookup>>, String> {
        let state = self.doc_state()?;
        let Some(kdm) = state.kdm() else { return Ok(None) };
        let replica = kdm.replica();
        let mut result = BTreeMap::new();
        let objects = replica.objects_of_type("Table");
        if objects.len().saturating_add(state.wb.names.len()) > 100_000 {
            return Ok(None);
        }
        let indexed: BTreeSet<_> =
            objects.iter().filter_map(|object| replica.get(object, "key").ok().and_then(|value| value.as_str().map(str::to_owned))).collect();
        for sheet in &state.wb.sheets {
            for table in &sheet.tables {
                let key = format!("{}:{}", sheet.sheet_id, table.id);
                if result.len() >= 100_000 || !indexed.contains(&key) {
                    return Ok(None);
                }
                result.insert(format!("table:{key}"), Lookup { name: name_key(&table.name), area: Some((sheet.sheet_id, table.range)) });
            }
        }
        for (index, name) in state.wb.names.iter().enumerate() {
            let Some(raw) = kdm.model().ids().names.get(index) else { return Ok(None) };
            if result.len() >= 100_000 {
                return Ok(None);
            }
            result.insert(format!("name:{raw}"), Lookup { name: name_key(&name.name), area: None });
        }
        Ok(Some(result))
    }

    pub(crate) fn table_lookup_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        if !names.is_empty() && !names.iter().any(|name| name == "lookupName") {
            return Ok(());
        }
        let Some(lookups) = self.table_lookups()? else { return Ok(()) };
        let requested: BTreeSet<_> = requested.iter().filter_map(Value::as_str).collect();
        let mut slots: BTreeMap<_, _> = found
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                value.id.as_str().filter(|id| id.starts_with("table:") || id.starts_with("name:")).map(|id| (id.to_owned(), index))
            })
            .collect();
        for (tool, lookup) in lookups {
            if !requested.is_empty() && !requested.contains(tool.as_str()) {
                continue;
            }
            let id = json!(tool);
            let values = [("lookupName".to_owned(), json!(lookup.name))].into_iter().collect();
            if let Some(previous) = slots.get(&tool).and_then(|index| found.get_mut(*index)) {
                previous.values.extend(values);
            } else {
                slots.insert(tool, found.len());
                found.push(DerivedValues { id, values });
            }
        }
        Ok(())
    }

    pub(crate) fn table_lookup_premises(&self, under: &mut Under, params: &Value, si: usize, range: RangeRef) -> Result<bool, String> {
        let Some(name) = params.get("name").and_then(Value::as_str) else { return Ok(false) };
        let name = name_key(name);
        let Some(lookups) = self.table_lookups()? else { return Ok(false) };
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(false) };
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(false) };
        if axes.rows.get(range.end.row as usize).is_none() || axes.cols.get(range.end.col as usize).is_none() {
            return Ok(false);
        }
        for (id, _) in lookups.into_iter().filter(|(_, lookup)| lookup.matches(&name, sheet.sheet_id, range)) {
            under.objects.push(json!(id));
            under.premises.push(premise(id, ["~lookupName"]));
        }
        // Covers supersedes list windows. Keep the explicitly addressed target
        // rows/columns as positional premises while the duplicate query uses covers.
        for (kind, first, last, lines) in [("row", range.start.row, range.end.row, &axes.rows), ("col", range.start.col, range.end.col, &axes.cols)] {
            for raw in lines.iter().skip(first as usize).take(last.saturating_sub(first).saturating_add(1) as usize) {
                under.premises.push(premise(format!("{kind}:{raw}"), Vec::<String>::new()).positions(["index"]));
            }
        }
        under.covers = Some(Covers { fields: vec!["~lookupName".into(), "name".into(), "range".into()] });
        Ok(true)
    }

    pub(crate) fn table_insert_covered(&self, command: &str, given: &Value, objects: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
        if command != "insert.table" || given.get("name").and_then(Value::as_str).is_none() {
            return Ok(None);
        }
        let mut params = given.clone();
        let implied = self.take_tool_ids(command, &mut params, self.command_schema(command).as_ref())?;
        let state = self.doc_state()?;
        let si = self.command_target_sheet(command, &params, implied).unwrap_or(state.wb.active_sheet);
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(range) = params.get("range").and_then(Value::as_str).and_then(crate::place::area) else { return Ok(None) };
        let Some(name) = params.get("name").and_then(Value::as_str) else { return Ok(None) };
        let name = name_key(name);
        let Some(lookups) = self.table_lookups()? else { return Ok(None) };
        let Some(current) = self.call_premises(command, &params)? else { return Ok(None) };
        if current.covers.is_none() {
            return Ok(None);
        }
        let mut before: BTreeSet<String> = current.objects.iter().filter_map(Value::as_str).map(str::to_owned).collect();
        let mut covered = Vec::with_capacity(objects.len());
        for ask in objects {
            let Some(id) = ask.id.as_str() else {
                covered.push(Covered { id: ask.id.clone(), covered: None, ..Default::default() });
                continue;
            };
            if ask.created {
                before.remove(id);
                covered.push(Covered { id: ask.id.clone(), covered: Some(false), ..Default::default() });
                continue;
            }
            let table = id.strip_prefix("table:").and_then(|key| key.split_once(':')).and_then(|(owner, _)| owner.parse::<u32>().ok());
            if table.is_none() && !id.starts_with("name:") {
                covered.push(Covered { id: ask.id.clone(), covered: Some(false), ..Default::default() });
                continue;
            }
            if ask.unknown.iter().any(|field| matches!(field.as_str(), "~lookupName" | "name" | "range" | "$was")) {
                covered.push(Covered { id: ask.id.clone(), covered: None, ..Default::default() });
                continue;
            }
            let current = lookups.get(id);
            let previous_name = ask
                .before
                .get("~lookupName")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| ask.before.get("name").and_then(Value::as_str).map(name_key))
                .or_else(|| current.map(|lookup| lookup.name.clone()));
            let previous_area = if let Some(owner) = table {
                ask.before
                    .get("range")
                    .and_then(Value::as_str)
                    .and_then(crate::place::area)
                    .map(|range| (owner, range))
                    .or_else(|| current.and_then(|lookup| lookup.area))
            } else {
                None
            };
            let Some(previous_name) = previous_name else {
                covered.push(Covered { id: ask.id.clone(), covered: None, ..Default::default() });
                continue;
            };
            if table.is_some() && previous_area.is_none() {
                covered.push(Covered { id: ask.id.clone(), covered: None, ..Default::default() });
                continue;
            }
            let previous = Lookup { name: previous_name, area: previous_area };
            let matched = previous.matches(&name, sheet.sheet_id, range);
            if matched {
                before.insert(id.to_owned());
            } else {
                before.remove(id);
            }
            covered.push(Covered {
                id: ask.id.clone(),
                covered: Some(matched || current.is_some_and(|lookup| lookup.matches(&name, sheet.sheet_id, range))),
                ..Default::default()
            });
        }
        Ok(Some(CoveredAt { under: before.into_iter().map(Value::String).collect(), covered }))
    }
}
