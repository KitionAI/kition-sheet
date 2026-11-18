//! Drawing shapes as `Shape` objects (schema version 3): each shape's frame, text and other
//! properties are fields of their own, so two people's concurrent edits to different shapes, or to
//! one shape's text and its fill, all survive a merge (`KitDrawing 1.0` DR14). Before version 3 a
//! sheet's shapes rode in its `features` register as one JSON value, so any two concurrent shape
//! edits on a sheet kept only one; such a replica still reads, and [`migrate`] moves its shapes
//! into objects.
//!
//! A shape names its sheet (`sheet`); readers order those references by `z` (ties by object
//! id). The sheet also maintains a `shapes` ownership map for SDK traversal and receipts.
//!
//! - `frame`: the anchor (its cell a position token), rotation and mirroring;
//! - `text`: the shape's text, merged character by character;
//! - `look` (schema version 5): everything else of the shape (kind, colours, geometry, outline,
//!   kept XML), one map entry per member, so two people changing two different ones of the
//!   same shape at once both keep theirs; before version 5 these were one `props` register,
//!   which still reads and moves into `look` on the shape's next write;
//! - `z`: its place in the sheet's stacking order.

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_kit::session::sha256_hex;
use kition_sheet_model::Shape;
use serde_json::{Map, Value as Json, json};

use crate::KdmResult;
use crate::codec::{Decode, Encode};
use crate::features::{anchor_value, from_json, read_anchor, to_json};

/// The object type.
pub(crate) const SHAPE: &str = "Shape";
/// The key of a sheet's shapes in its `features` before schema version 3.
pub(crate) const SHAPES: &str = "shapes";
/// The schema version whose shapes are `Shape` objects: a migrated replica requires it (a 0.5
/// reader, on version 2, would find no shapes in the sheet's `features`).
pub(crate) const SHAPE_OBJECTS: u32 = 3;

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
/// of them for a new shape, or one still kept in the `props` register, which is then cleared).
/// `true` when it wrote.
pub(crate) fn write_look(tx: &mut Tx, object: &ObjectId, before: Option<&Json>, after: &Json) -> KdmResult<bool> {
    let legacy = tx.get(object, "props")?;
    let before = if legacy.is_null() { before } else { None };
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
    if !legacy.is_null() {
        tx.set(object, "props", Json::Null)?;
        wrote = true;
    }
    Ok(wrote)
}

/// A shape's other members: its `props` register (before schema version 5) under its `look`
/// map's entries.
pub(crate) fn look_of(replica: &Replica, object: &ObjectId) -> KdmResult<Json> {
    let mut v: Map<String, Json> = replica.get(object, "props")?.as_object().cloned().unwrap_or_default();
    for (k, x) in replica.map_entries(object, "look").unwrap_or_default() {
        v.insert(k, x);
    }
    Ok(Json::Object(v))
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

/// The id the migration gives the `index`-th shape of `sheet`'s `features`: the same in every
/// replica, so two people migrating one replica at once make the same objects. (Text each of them
/// types into one migrated shape before they sync is then one creation's: one side's text stays.)
fn migrated_id(sheet: &ObjectId, index: usize) -> ObjectId {
    const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let hash = sha256_hex(format!("{}/shape/{index}", sheet.as_str()).as_bytes());
    let mut n = hash.get(..24).and_then(|h| u128::from_str_radix(h, 16).ok()).unwrap_or_default();
    let mut id = [b'0'; 18];
    id[0] = b's';
    for digit in id[1..].iter_mut().rev() {
        *digit = DIGITS[(n % 62) as usize];
        n /= 62;
    }
    ObjectId::from_string(String::from_utf8_lossy(&id).into_owned())
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

/// Whether the replica keeps shapes in its sheets' `features` (it does not require
/// [`SHAPE_OBJECTS`]).
pub(crate) fn needs_migration(replica: &Replica) -> bool {
    replica.exists(&crate::root_id()) && replica.min_schema_version() < SHAPE_OBJECTS
}

/// Moves every sheet's shapes from its `features` register into `Shape` objects and raises the
/// document's minimum schema version to [`SHAPE_OBJECTS`] in the same transaction (a `system`
/// one), so an older reader refuses the replica instead of showing it without its shapes. The
/// shapes' stored form (anchors as position tokens) carries over unchanged.
pub(crate) fn migrate(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let root = crate::root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    let mut wrote = tx.raise_schema_version(SHAPE_OBJECTS)?;
    for sheet in tx.list_items(&root, "sheets")? {
        if tx.object_type(&sheet).ok().as_deref() != Some("Sheet") {
            continue;
        }
        let mut features = tx.get(&sheet, "features")?;
        let Some(legacy) = features.as_object_mut().and_then(|f| f.remove(SHAPES)) else { continue };
        for (index, item) in legacy.as_array().into_iter().flatten().take(100_000).enumerate() {
            let Some(map) = item.as_object() else { continue };
            let frame = json!({
                "anchor": map.get("anchor").cloned().unwrap_or(Json::Null),
                "rot": map.get("rot").cloned().unwrap_or(json!(0)),
                "flipH": map.get("flip_h").cloned().unwrap_or(json!(false)),
                "flipV": map.get("flip_v").cloned().unwrap_or(json!(false)),
            });
            let mut props = map.clone();
            for key in OWN {
                props.remove(key);
            }
            let text = map.get("text").and_then(Json::as_str).unwrap_or("").to_owned();
            let mut fields = Map::new();
            fields.insert("sheet".into(), json!(sheet.as_str()));
            fields.insert("z".into(), json!(index));
            fields.insert("frame".into(), frame);
            fields.insert("props".into(), Json::Object(props));
            let object = match tx.create_with_id(SHAPE, &migrated_id(&sheet, index), fields.clone()) {
                Ok(id) => id,
                Err(_) => tx.create(SHAPE, fields)?,
            };
            if !text.is_empty() {
                tx.text_insert(&object, "text", 0, &text)?;
            }
        }
        tx.set(&sheet, "features", features)?;
        wrote = true;
    }
    Ok(wrote)
}
