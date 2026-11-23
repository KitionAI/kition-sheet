//! Read-only identity indexes: a sheet's shape groups (`Group`, by the groups its shapes name)
//! and the workbook's styles (`Style`, immutable ones by their content hash, named cell styles by
//! name, table styles by their native UID).
use std::collections::{BTreeMap, BTreeSet};

use kition_kit::kdm::{ObjectId, Tx};
use kition_sheet_model::{Sheet, Style, TableStyleDef, TableStyles};
use serde_json::{Map, Value, json};

use crate::{KdmResult, list::sync_list, root_id};

fn sync(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, ty: &str, values: Vec<(String, Map<String, Value>)>) -> KdmResult<bool> {
    let before = tx.list_items(owner, field)?;
    let mut changed = false;
    let mut wanted = Vec::with_capacity(values.len());
    for (_, fields) in values {
        let existing = before.iter().find(|id| tx.exists(id) && tx.get(id, "key").ok().as_ref() == fields.get("key"));
        let object = if let Some(object) = existing {
            for (field, value) in fields {
                if tx.get(object, &field)? != value {
                    tx.set(object, &field, value)?;
                    changed = true;
                }
            }
            object.clone()
        } else {
            changed = true;
            tx.create(ty, fields)?
        };
        wanted.push(object);
    }
    changed |= sync_list(tx, owner, field, &wanted)? > 0;
    for object in before {
        if !wanted.contains(&object) && tx.exists(&object) {
            tx.delete(&object)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// Writes the `Group` objects of `sheet`'s shapes. Answers whether it wrote.
pub(crate) fn sheet(tx: &mut Tx<'_>, owner: &ObjectId, sheet: &Sheet) -> KdmResult<bool> {
    let mut groups: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for shape in &sheet.shapes {
        for &group in &shape.groups {
            groups.entry(group).or_default().insert(shape.id);
        }
    }
    let groups = groups
        .into_iter()
        .map(|(id, members)| {
            (
                id.to_string(),
                [
                    ("key".into(), json!(id)),
                    ("sheet".into(), json!(owner.as_str())),
                    ("members".into(), json!(members.into_iter().map(|id| format!("drawing:{id}")).collect::<Vec<_>>())),
                ]
                .into_iter()
                .collect(),
            )
        })
        .collect();
    sync(tx, owner, "groups", "Group", groups)
}

/// Imported table styles without a native uid receive a deterministic initial identity.
/// The modify command stores this uid before changing the definition's name.
pub fn table_style_uid(style: &TableStyleDef) -> String {
    style.uid.clone().unwrap_or_else(|| {
        let hash = kition_kit::session::sha256_hex(style.name.to_ascii_lowercase().as_bytes());
        let part = |from, to| hash.get(from..to).unwrap_or_default();
        format!("{{{}-{}-{}-{}-{}}}", part(0, 8), part(8, 12), part(12, 16), part(16, 20), part(20, 32))
    })
}

/// A case-insensitive named cell-style index key.
pub fn cell_style_name_key(name: &str) -> String {
    kition_kit::session::sha256_hex(name.to_ascii_lowercase().as_bytes())
}

pub(crate) fn styles(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let root = root_id();
    let mut values: Vec<_> = tx
        .map_entries(&root, "styles")?
        .into_iter()
        .map(|(key, value)| {
            (key.clone(), [("key".into(), json!(key)), ("value".into(), value.get("style").cloned().unwrap_or(Value::Null))].into_iter().collect())
        })
        .collect();
    let cell_styles: Vec<(String, Style)> = crate::features::from_json(tx.get(&root, "cellStyles")?).unwrap_or_default();
    let table_styles: TableStyles = crate::features::from_json(tx.get(&root, "props")?["tableStyles"].clone()).unwrap_or_default();
    for (name, style) in &cell_styles {
        let key = format!("cell:{}", crate::write::style_key(&json!(name.to_ascii_lowercase())));
        values.push((
            key.clone(),
            [
                ("key".into(), json!(key)),
                ("name".into(), json!(name)),
                ("kind".into(), json!("cell")),
                ("value".into(), crate::features::to_json(style)?),
            ]
            .into_iter()
            .collect(),
        ));
    }
    for style in &table_styles.styles {
        let key = format!("table:{}", table_style_uid(style));
        values.push((
            key.clone(),
            [
                ("key".into(), json!(key)),
                ("name".into(), json!(style.name)),
                ("kind".into(), json!("table")),
                ("value".into(), crate::features::to_json(style)?),
            ]
            .into_iter()
            .collect(),
        ));
    }
    let mut changed = sync(tx, &root, "styleObjects", "Style", values)?;
    let old: BTreeMap<_, _> = tx.map_entries(&root, "cellStyleDefinitions")?.into_iter().collect();
    let props = tx.get(&root, "props")?;
    let mut wanted = BTreeMap::new();
    for (name, style) in cell_styles {
        let info = props["cellStyleInfo"].as_object().and_then(|infos| infos.iter().find(|(stored, _)| stored.eq_ignore_ascii_case(&name)));
        let value = serde_json::to_vec(&(name.clone(), style, info)).map_err(crate::kdm_err)?;
        wanted.insert(cell_style_name_key(&name), json!(kition_kit::session::sha256_hex(&value)));
    }
    for key in old.keys().filter(|key| !wanted.contains_key(*key)) {
        tx.map_delete(&root, "cellStyleDefinitions", key)?;
        changed = true;
    }
    for (key, value) in wanted {
        if old.get(&key) != Some(&value) {
            tx.map_set(&root, "cellStyleDefinitions", &key, value)?;
            changed = true;
        }
    }
    Ok(changed)
}
