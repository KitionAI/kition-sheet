//! Import a file's canonical model into the retained replica, preserving identities and history.
use kition_kit::kdm::{FieldKind, KdmError, ObjectId, Schema, Tx, reread::pair};
use kition_kit::session::{TextRun, write_rich_text, write_text};
use serde_json::{Map, Value};

use crate::{KdmResult, list::sync_list};

pub(crate) fn reconcile(tx: &mut Tx<'_>, schema: &Schema, before: &Value, fresh: &Value) -> KdmResult<()> {
    // Persisted identities win; the SDK pairs objects whose file format minted fresh IDs.
    let after = pair(schema, fresh, before).renamed(schema, fresh);
    let objects = after.get("objects").and_then(Value::as_object).ok_or_else(|| invalid("missing file objects"))?;
    let old = before.get("objects").and_then(Value::as_object).ok_or_else(|| invalid("missing retained objects"))?;
    if objects.len().saturating_add(old.len()) > 1_000_000 {
        return Err(invalid("too many objects to reconcile"));
    }
    let mut pending: Vec<_> = objects.iter().filter(|(id, _)| !tx.exists(&ObjectId::from_string(*id))).collect();
    while !pending.is_empty() {
        let count = pending.len();
        let mut remaining = Vec::new();
        for (id, object) in pending {
            let ty = object.get("$type").and_then(Value::as_str).ok_or_else(|| invalid("missing object type"))?;
            let definition = schema.types.get(ty).ok_or_else(|| invalid("unknown object type"))?;
            let mut fields = Map::new();
            let mut ready = true;
            for (field, definition) in &definition.fields {
                if !definition.kind.is_container() && !definition.optional {
                    let value = object.get(field).cloned().unwrap_or(Value::Null);
                    if definition.kind == FieldKind::Ref && value.as_str().is_some_and(|target| !tx.exists(&ObjectId::from_string(target))) {
                        ready = false;
                    }
                    fields.insert(field.clone(), value);
                }
            }
            if ready {
                tx.create_with_id(ty, &ObjectId::from_string(id), fields)?;
            } else {
                remaining.push((id, object));
            }
        }
        if remaining.len() == count {
            return Err(invalid("unresolved required references in new objects"));
        }
        pending = remaining;
    }
    for (id, object) in objects {
        let id = ObjectId::from_string(id);
        let ty = object.get("$type").and_then(Value::as_str).ok_or_else(|| invalid("missing object type"))?;
        let definition = schema.types.get(ty).ok_or_else(|| invalid("unknown object type"))?;
        for (field, definition) in &definition.fields {
            let wanted = object.get(field).cloned().unwrap_or(Value::Null);
            match definition.kind {
                FieldKind::List => {
                    let wanted: Vec<_> = wanted.as_array().into_iter().flatten().filter_map(Value::as_str).map(ObjectId::from_string).collect();
                    sync_list(tx, &id, field, &wanted)?;
                }
                FieldKind::Map => {
                    let entries = wanted.as_object();
                    for (key, _) in tx.map_entries(&id, field)? {
                        if entries.is_none_or(|entries| !entries.contains_key(&key)) {
                            tx.map_delete(&id, field, &key)?;
                        }
                    }
                    for (key, value) in entries.into_iter().flatten() {
                        if tx.map_get(&id, field, key)? != *value {
                            tx.map_set(&id, field, key, value.clone())?;
                        }
                    }
                }
                FieldKind::Text => {
                    let previous = old.get(id.as_str()).and_then(|object| object.get(field)).cloned().unwrap_or(Value::Null);
                    if let Some(text) = wanted.as_str() {
                        write_text(tx, &id, field, previous.as_str().unwrap_or_default(), text).map_err(|error| invalid(&error.to_string()))?;
                    } else {
                        let runs: Vec<TextRun> = if wanted.is_null() {
                            Vec::new()
                        } else {
                            serde_json::from_value(wanted).map_err(|error| invalid(&error.to_string()))?
                        };
                        let previous: Vec<TextRun> = if previous.is_null() {
                            Vec::new()
                        } else {
                            serde_json::from_value(previous).map_err(|error| invalid(&error.to_string()))?
                        };
                        write_rich_text(tx, &id, field, &previous, &runs).map_err(|error| invalid(&error.to_string()))?;
                    }
                }
                FieldKind::Tree => return Err(invalid("sheet file models do not contain trees")),
                _ => {
                    if tx.get(&id, field)? != wanted {
                        tx.set(&id, field, wanted)?;
                    }
                }
            }
        }
    }
    for id in old.keys().filter(|id| !objects.contains_key(*id)) {
        tx.delete(&ObjectId::from_string(id))?;
    }
    Ok(())
}

fn invalid(message: &str) -> KdmError {
    KdmError::Engine(format!("cannot reconcile sheet file: {message}"))
}
