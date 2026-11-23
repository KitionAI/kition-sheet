//! Drawing shapes as `Shape` objects: each shape's frame, text and other properties are fields of
//! their own, so two people's concurrent edits to different shapes, or to one shape's text and its
//! fill, all survive a merge (`KitDrawing 1.0` DR14).
//!
//! A shape names its sheet (`sheet`); readers order those references by `z` (ties by object
//! id). The sheet also maintains a `shapes` ownership map for SDK traversal and receipts.
//!
//! - `frame`: the anchor (its cell a position token), rotation and mirroring;
//! - `text`: the shape's text, merged character by character;
//! - `look`: everything else of the shape (kind, colours, geometry, outline, kept XML), one map
//!   entry per member, so two people changing two different ones of the same shape at once both
//!   keep theirs;
//! - `z`: its place in the sheet's stacking order.

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_sheet_model::Shape;
use serde_json::{Map, Value as Json, json};

use crate::KdmResult;
use crate::codec::{Decode, Encode};
use crate::features::{anchor_value, from_json, read_anchor, to_json};

/// The object type.
pub(crate) const SHAPE: &str = "Shape";

/// The members of a serialized [`Shape`] that are fields of their own.
const OWN: [&str; 5] = ["anchor", "rot", "flip_h", "flip_v", "text"];

pub(crate) fn frame<E: Encode + ?Sized>(e: &mut E, s: usize, sh: &Shape) -> KdmResult<Json> {
    Ok(json!({ "anchor": anchor_value(e, s, &sh.anchor)?, "rot": sh.rot, "flipH": sh.flip_h, "flipV": sh.flip_v }))
}

pub(crate) fn props(sh: &Shape) -> KdmResult<Json> {
    let mut v = to_json(sh)?;
    if let Some(map) = v.as_object_mut() {
        for key in OWN {
            map.remove(key);
        }
    }
    Ok(v)
}

/// Writes a shape's other members into its `look` map: those that changed since `before` (all
/// of them for a new shape). `true` when it wrote.
pub(crate) fn write_look(tx: &mut Tx, object: &ObjectId, before: Option<&Json>, after: &Json) -> KdmResult<bool> {
    let (was, now) = (before.and_then(Json::as_object), after.as_object());
    let mut wrote = false;
    for (k, v) in now.into_iter().flatten() {
        if was.and_then(|w| w.get(k)) != Some(v) {
            tx.map_set(object, "look", k, v.clone())?;
            wrote = true;
        }
    }
    for k in was.into_iter().flatten().map(|(k, _)| k).filter(|k| now.is_none_or(|n| !n.contains_key(*k))) {
        tx.map_delete(object, "look", k)?;
        wrote = true;
    }
    Ok(wrote)
}

/// A shape's other members: its `look` map's entries.
pub(crate) fn look_of(replica: &Replica, object: &ObjectId) -> KdmResult<Json> {
    Ok(Json::Object(replica.map_entries(object, "look")?.into_iter().collect()))
}

/// A shape from its object's fields; `None` when its properties do not read.
pub(crate) fn read<D: Decode + ?Sized>(d: &D, s: usize, frame: &Json, props: &Json, text: String) -> Option<Shape> {
    let mut v: Map<String, Json> = props.as_object().cloned().unwrap_or_default();
    let anchor = frame.get("anchor").and_then(|a| read_anchor(d, s, a)).unwrap_or_default();
    v.insert("anchor".into(), to_json(&anchor).ok()?);
    v.insert("rot".into(), frame.get("rot").cloned().unwrap_or(json!(0)));
    v.insert("flip_h".into(), frame.get("flipH").cloned().unwrap_or(json!(false)));
    v.insert("flip_v".into(), frame.get("flipV").cloned().unwrap_or(json!(false)));
    v.insert("text".into(), Json::String(text));
    from_json(Json::Object(v))
}

/// The live `Shape` objects of sheet `sheet`, bottom to top.
pub(crate) fn of_sheet(replica: &Replica, sheet: &ObjectId) -> Vec<ObjectId> {
    let mut found: Vec<(f64, ObjectId)> = replica
        .objects_of_type(SHAPE)
        .into_iter()
        .filter(|id| replica.get(id, "sheet").ok().as_ref().and_then(Json::as_str) == Some(sheet.as_str()))
        .map(|id| (replica.get(&id, "z").ok().and_then(|z| z.as_f64()).filter(|z| z.is_finite()).unwrap_or(0.0), id))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));
    found.into_iter().map(|(_, id)| id).collect()
}

/// Makes the text field of `object` read `now`, given that it reads `was`: only the changed middle
/// is deleted and inserted, so concurrent typing elsewhere in the text stays.
pub(crate) fn write_text(tx: &mut Tx<'_>, object: &ObjectId, was: &str, now: &str) -> KdmResult<bool> {
    if was == now {
        return Ok(false);
    }
    let a: Vec<char> = was.chars().collect();
    let b: Vec<char> = now.chars().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    let removed = a.len() - prefix - suffix;
    if removed > 0 {
        tx.text_delete(object, "text", prefix, removed)?;
    }
    let inserted: String = b[prefix..b.len() - suffix].iter().collect();
    if !inserted.is_empty() {
        tx.text_insert(object, "text", prefix, &inserted)?;
    }
    Ok(true)
}
