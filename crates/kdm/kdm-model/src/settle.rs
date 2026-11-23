//! Canonicalize direct object-model writes before the SDK takes its receipt.
use std::collections::HashSet;

use kition_kit::kdm::{ObjectId, Tx};
use kition_sheet_core::CellRef;
use kition_sheet_model::{Sheet, StyleId};
use serde_json::{Value, json};

use crate::codec::{Decode, Encode, OPEN, encode_formula, literal, parse_literal};
use crate::lines::{Axis, LineAt, Lines, Use, pad_id};
use crate::{KdmResult, root_id};

/// Tool addresses are a projection of content and axis order. Keep them out of the incremental
/// content diff, and commit their consequences in the session's same transaction.
pub(crate) fn project_positions(tx: &mut Tx<'_>, workbook: &kition_sheet_model::Workbook, ids: &crate::Ids) -> KdmResult<()> {
    for (sheet, ids) in workbook.sheets.iter().zip(&ids.sheets) {
        for axis in Axis::BOTH {
            for (index, object) in ids.axis(axis).iter().enumerate() {
                if tx.get(object, "index")? != json!(index) {
                    tx.set(object, "index", json!(index))?;
                }
            }
        }
        let rows: std::collections::HashMap<_, _> = ids.rows.iter().enumerate().map(|(index, id)| (id.as_str(), index as u32)).collect();
        let cols: std::collections::HashMap<_, _> = ids.cols.iter().enumerate().map(|(index, id)| (id.as_str(), index as u32)).collect();
        for (key, value) in tx.map_entries(&ids.object, "cells")? {
            let Some(object) = value.as_str().map(ObjectId::from_string).filter(|object| tx.exists(object)) else { continue };
            let Some(at) = key.match_indices(':').find_map(|(i, _)| Some(CellRef::new(*rows.get(key.get(..i)?)?, *cols.get(key.get(i + 1..)?)?)))
            else {
                continue;
            };
            for (field, value) in [("address", json!(format!("{}!{at}", sheet.sheet_id))), ("label", json!(format!("sheet:{}!{at}", sheet.sheet_id)))]
            {
                if tx.get(&object, field)? != value {
                    tx.set(&object, field, value)?;
                }
            }
        }
    }
    Ok(())
}

struct Inputs<'a, 'tx> {
    tx: &'a mut Tx<'tx>,
    sheets: Vec<(ObjectId, String, [Vec<ObjectId>; 2])>,
    reserved: HashSet<ObjectId>,
}

impl Decode for Inputs<'_, '_> {
    fn sheet_of_object(&self, object: &str) -> Option<usize> {
        self.sheets.iter().position(|(id, _, _)| id.as_str() == object)
    }
    fn sheet_name(&self, sheet: usize) -> Option<&str> {
        self.sheets.get(sheet).map(|(_, name, _)| name.as_str())
    }
    fn line_at(&self, sheet: usize, axis: Axis, token: &str) -> LineAt {
        let Some((object, _, _)) = self.sheets.get(sheet) else { return LineAt::Missing };
        let order = self
            .tx
            .list_items(object, axis.field())
            .unwrap_or_default()
            .into_iter()
            .filter(|id| self.tx.exists(id))
            .map(|id| {
                let gone = self.tx.get(&id, "gone").ok() == Some(json!(true));
                (id, gone)
            })
            .collect();
        Lines::new(order).at(token)
    }
    fn style_id(&self, _key: &str) -> StyleId {
        StyleId::DEFAULT
    }
}

impl Encode for Inputs<'_, '_> {
    fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets.iter().position(|(_, stored, _)| stored.eq_ignore_ascii_case(name))
    }
    fn sheet_object(&self, sheet: usize) -> Option<String> {
        self.sheets.get(sheet).map(|(id, _, _)| id.to_string())
    }
    fn style_key(&self, _id: StyleId) -> Option<String> {
        None
    }
    fn line(&mut self, sheet: usize, axis: Axis, index: u32, _purpose: Use) -> KdmResult<String> {
        let Some((object, _, axes)) = self.sheets.get_mut(sheet) else { return Ok(format!("#{index}")) };
        let Some(lines) = axes.get_mut(axis.slot()) else { return Ok(format!("#{index}")) };
        if let Some(id) = lines.get(index as usize) {
            return Ok(id.to_string());
        }
        if index >= axis.ref_limit() {
            return Ok(format!("#{index}"));
        }
        let mut raw = self.tx.list_items(object, axis.field())?.len();
        while lines.len() <= index as usize {
            let candidate = pad_id(object, axis, raw);
            let id = if self.reserved.contains(&candidate) && !self.tx.exists(&candidate) { ObjectId::new() } else { candidate };
            if !self.tx.exists(&id) {
                self.tx.create_with_id(
                    axis.ty(),
                    &id,
                    [("key".into(), json!(id.as_str())), ("index".into(), json!(lines.len()))].into_iter().collect(),
                )?;
            }
            self.tx.list_insert(object, axis.field(), raw, &id)?;
            lines.push(id);
            raw += 1;
        }
        Ok(lines.get(index as usize).map_or_else(|| format!("#{index}"), ToString::to_string))
    }
}

pub fn settle_inputs(tx: &mut Tx<'_>, reserved: &[ObjectId]) -> KdmResult<()> {
    let mut sheets = Vec::new();
    let objects: Vec<_> = tx.list_items(&root_id(), "sheets")?.into_iter().filter(|id| tx.exists(id)).collect();
    if objects.is_empty() {
        return Err(crate::kdm_err("a workbook must contain a sheet"));
    }
    let mut next_sheet = objects.iter().filter_map(|object| tx.get(object, "sheetId").ok()?.as_u64()).max().unwrap_or(0);
    let mut used_names = HashSet::new();
    let mut used_ids = HashSet::new();
    for object in objects {
        let stored_name = tx.get(&object, "name")?.as_str().unwrap_or_default().to_owned();
        let name = crate::read::unique_name(&stored_name, &used_names);
        used_names.insert(name.to_ascii_lowercase());
        if name != stored_name {
            tx.set(&object, "name", json!(name))?;
        }
        let id = tx.get(&object, "sheetId")?.as_u64().filter(|id| *id > 0 && *id <= u64::from(u32::MAX));
        if id.is_none_or(|id| !used_ids.insert(id)) {
            next_sheet = next_sheet.checked_add(1).filter(|id| *id <= u64::from(u32::MAX)).ok_or_else(|| crate::kdm_err("sheet ids exhausted"))?;
            used_ids.insert(next_sheet);
            tx.set(&object, "sheetId", json!(next_sheet))?;
        }
        let mut axes = [Vec::new(), Vec::new()];
        for axis in Axis::BOTH {
            for id in tx.list_items(&object, axis.field())? {
                if tx.exists(&id) {
                    if tx.get(&id, "key")?.as_str() != Some(id.as_str()) {
                        tx.set(&id, "key", json!(id.as_str()))?;
                    }
                    let gone = tx.get(&id, "gone")? == json!(true);
                    if let Some(lines) = axes.get_mut(axis.slot()) {
                        let index = if gone { Value::Null } else { json!(lines.len()) };
                        if tx.get(&id, "index")? != index {
                            tx.set(&id, "index", index)?;
                        }
                        if !gone {
                            lines.push(id);
                        }
                    }
                }
            }
        }
        for image in tx.list_items(&object, "images")? {
            if tx.exists(&image) && tx.get(&image, "blob")?.is_null() {
                return Err(crate::kdm_err("an image requires content bytes"));
            }
        }
        sheets.push((object, name, axes));
    }
    let mut inputs = Inputs { tx, sheets, reserved: reserved.iter().cloned().collect() };
    for host in 0..inputs.sheets.len() {
        let Some(object) = inputs.sheets.get(host).map(|(object, _, _)| object.clone()) else { continue };
        let sheet = Sheet::new(inputs.sheets.get(host).map(|(_, name, _)| name.as_str()).unwrap_or("Sheet"));
        if inputs.tx.get(&object, "props")?.is_null() {
            let props = crate::features::sheet_props(&mut inputs, host, &sheet, [&[], &[]])?;
            inputs.tx.set(&object, "props", props)?;
        }
        let mut constrained = sheet.clone();
        crate::features::read_features(&*inputs.tx, &inputs, host, &object, &mut constrained);
        crate::constraints::write(inputs.tx, &object, &constrained)?;
        for (_, value) in inputs.tx.map_entries(&object, "shapes").unwrap_or_default() {
            let Some(shape) = value.as_str().map(ObjectId::from_string).filter(|id| inputs.tx.exists(id)) else { continue };
            let look = Value::Object(inputs.tx.map_entries(&shape, "look")?.into_iter().collect());
            if let Some(shape) = crate::shapes::read(&inputs, host, &inputs.tx.get(&shape, "frame")?, &look, String::new()) {
                constrained.shapes.push(shape);
            }
        }
        crate::feature_ids::sheet(inputs.tx, &object, &constrained)?;
        for (field, property) in [("shapes", "frame"), ("charts", "frame"), ("images", "props")] {
            let objects: Vec<_> = if field == "images" {
                inputs.tx.list_items(&object, field)?
            } else {
                inputs
                    .tx
                    .map_entries(&object, field)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(_, value)| value.as_str().map(ObjectId::from_string))
                    .collect()
            };
            for drawing in objects {
                if !inputs.tx.exists(&drawing) {
                    continue;
                }
                let mut frame = inputs.tx.get(&drawing, property)?;
                if let Some(anchor) = frame.get("anchor").and_then(|anchor| crate::features::read_anchor(&inputs, host, anchor)) {
                    let canonical = crate::features::anchor_value(&mut inputs, host, &anchor)?;
                    if frame["anchor"] != canonical {
                        frame["anchor"] = canonical;
                        inputs.tx.set(&drawing, property, frame)?;
                    }
                }
            }
        }
        let sheet_id = inputs.tx.get(&object, "sheetId")?.as_u64().unwrap_or(0);
        for (key, value) in inputs.tx.map_entries(&object, "cells")? {
            let Some(id) = value.as_str().map(ObjectId::from_string) else { continue };
            let position = inputs.sheets.get(host).and_then(|(_, _, axes)| {
                let index = |axis: usize, token: &str| {
                    token
                        .strip_prefix('#')
                        .and_then(|index| index.parse::<u32>().ok())
                        .or_else(|| axes.get(axis)?.iter().position(|id| id.as_str() == token).and_then(|index| u32::try_from(index).ok()))
                };
                key.match_indices(':').find_map(|(at, _)| Some(CellRef::new(index(0, key.get(..at)?)?, index(1, key.get(at + 1..)?)?)))
            });
            let Some(position) = position.filter(|_| inputs.tx.exists(&id)) else {
                inputs.tx.map_delete(&object, "cells", &key)?;
                if inputs.tx.exists(&id) {
                    inputs.tx.delete(&id)?;
                }
                continue;
            };
            for (field, value) in [("address", json!(format!("{sheet_id}!{position}"))), ("label", json!(format!("sheet:{sheet_id}!{position}")))] {
                if inputs.tx.get(&id, field)? != value {
                    inputs.tx.set(&id, field, value)?;
                }
            }
            let value = inputs.tx.get(&id, "input")?;
            let Some(input) = value.as_str() else { continue };
            let canonical = match input.strip_prefix('=') {
                Some(body) if !body.contains(OPEN) => format!("={}", encode_formula(&mut inputs, body, None, Some(host))?),
                Some(_) => continue,
                None => {
                    if let kition_sheet_core::Value::Text(text) = parse_literal(input)
                        && !text.is_empty()
                    {
                        let len = inputs.tx.text_len(&id, "text")?;
                        if len > 0 {
                            inputs.tx.text_delete(&id, "text", 0, len)?;
                        }
                        inputs.tx.text_insert(&id, "text", 0, &text)?;
                        inputs.tx.set(&id, "input", serde_json::Value::Null)?;
                        continue;
                    }
                    literal(&parse_literal(input))
                }
            };
            if canonical != input {
                inputs.tx.set(&id, "input", json!(canonical))?;
            }
        }
    }
    for object in inputs.tx.list_items(&root_id(), "names")? {
        if !inputs.tx.exists(&object) {
            continue;
        }
        if inputs.tx.get(&object, "key")?.as_str() != Some(object.as_str()) {
            inputs.tx.set(&object, "key", json!(object.as_str()))?;
        }
        if inputs.tx.get(&object, "props")?.is_null() {
            inputs.tx.set(&object, "props", json!({"comment":"","hidden":false}))?;
        }
    }
    crate::feature_ids::styles(inputs.tx)?;
    Ok(())
}
