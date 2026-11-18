//! Charts as `Chart` objects (schema version 4): each chart's anchor is its own register and each
//! of its other members (type, title, legend, series, style…) an entry of its `parts` map, so two
//! people changing different charts of a sheet, or different members of one chart, all keep their
//! change (`KitDrawing` DR14). Before version 4 a sheet's charts rode in its `features` register as
//! one JSON value; such a replica still reads, and [`migrate`] moves its charts into objects.
//!
//! As a `Shape` does (`crate::shapes`), a `Chart` names its sheet rather than being listed by it,
//! and a sheet's charts are the objects naming it in `z` order (ties by object id). Series ranges
//! and the source range are stored as the `features` register stored them: position tokens, so a
//! row inserted above the data moves the chart's ranges with it.

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_kit::session::sha256_hex;
use kition_sheet_model::Chart;
use serde_json::{Map, Value as Json, json};

use crate::KdmResult;
use crate::codec::{Decode, Encode};
use crate::features::{anchor_value, chart_decoded, chart_encoded, from_json, read_anchor, to_json};

/// The object type.
pub(crate) const CHART: &str = "Chart";
/// The key of a sheet's charts in its `features` before schema version 4.
pub(crate) const CHARTS: &str = "charts";
/// The schema version whose charts are `Chart` objects: a migrated replica requires it (a version
/// 3 reader would find no charts in the sheet's `features`).
pub(crate) const CHART_OBJECTS: u32 = 4;

/// The chart's anchor, as its `frame` register holds it.
pub(crate) fn frame<E: Encode + ?Sized>(e: &mut E, s: usize, c: &Chart) -> KdmResult<Json> {
    Ok(json!({ "anchor": anchor_value(e, s, &c.anchor)? }))
}

/// The chart's other members (ranges as position tokens), by name.
pub(crate) fn parts<E: Encode + ?Sized>(e: &mut E, s: usize, c: &Chart) -> KdmResult<Map<String, Json>> {
    let mut v = to_json(&chart_encoded(e, s, c)?)?;
    let mut map = v.as_object_mut().map(std::mem::take).unwrap_or_default();
    map.remove("anchor");
    Ok(map)
}

/// A chart from its object's fields; `None` when its members do not read.
pub(crate) fn read<D: Decode + ?Sized>(d: &D, s: usize, frame: &Json, parts: Map<String, Json>) -> Option<Chart> {
    let mut v = parts;
    let anchor = frame.get("anchor").and_then(|a| read_anchor(d, s, a)).unwrap_or_default();
    v.insert("anchor".into(), to_json(&anchor).ok()?);
    from_json::<Chart>(Json::Object(v)).map(|c| chart_decoded(d, s, c))
}

/// Makes `object`'s `parts` hold `now`, writing only the entries that differ. Answers whether it
/// wrote.
pub(crate) fn write_parts(tx: &mut Tx<'_>, object: &ObjectId, now: &Map<String, Json>) -> KdmResult<bool> {
    let have: Map<String, Json> = tx.map_entries(object, "parts")?.into_iter().collect();
    let mut wrote = false;
    for (k, v) in now {
        if have.get(k) != Some(v) {
            tx.map_set(object, "parts", k, v.clone())?;
            wrote = true;
        }
    }
    for k in have.keys().filter(|k| !now.contains_key(*k)) {
        tx.map_delete(object, "parts", k)?;
        wrote = true;
    }
    Ok(wrote)
}

/// The live `Chart` objects of sheet `sheet`, bottom to top.
pub(crate) fn of_sheet(replica: &Replica, sheet: &ObjectId) -> Vec<ObjectId> {
    let mut found: Vec<(f64, ObjectId)> = replica
        .objects_of_type(CHART)
        .into_iter()
        .filter(|id| replica.get(id, "sheet").ok().as_ref().and_then(Json::as_str) == Some(sheet.as_str()))
        .map(|id| (replica.get(&id, "z").ok().and_then(|z| z.as_f64()).filter(|z| z.is_finite()).unwrap_or(0.0), id))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));
    found.into_iter().map(|(_, id)| id).collect()
}

/// The id the migration gives the `index`-th chart of `sheet`'s `features`: the same in every
/// replica, so two people migrating one replica at once make the same objects.
fn migrated_id(sheet: &ObjectId, index: usize) -> ObjectId {
    const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let hash = sha256_hex(format!("{}/chart/{index}", sheet.as_str()).as_bytes());
    let mut n = hash.get(..24).and_then(|h| u128::from_str_radix(h, 16).ok()).unwrap_or_default();
    let mut id = [b'0'; 18];
    id[0] = b'c';
    for digit in id[1..].iter_mut().rev() {
        *digit = DIGITS[(n % 62) as usize];
        n /= 62;
    }
    ObjectId::from_string(String::from_utf8_lossy(&id).into_owned())
}

/// Whether the replica keeps charts in its sheets' `features` (it does not require
/// [`CHART_OBJECTS`]).
pub(crate) fn needs_migration(replica: &Replica) -> bool {
    replica.exists(&crate::root_id()) && replica.min_schema_version() < CHART_OBJECTS
}

/// Moves every sheet's charts from its `features` register into `Chart` objects and raises the
/// document's minimum schema version to [`CHART_OBJECTS`] in the same transaction (a `system`
/// one), so an older reader refuses the replica instead of showing it without its charts. The
/// charts' stored form (ranges and anchors as position tokens) carries over unchanged.
pub(crate) fn migrate(tx: &mut Tx<'_>) -> KdmResult<bool> {
    let root = crate::root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    let mut wrote = tx.raise_schema_version(CHART_OBJECTS)?;
    for sheet in tx.list_items(&root, "sheets")? {
        if tx.object_type(&sheet).ok().as_deref() != Some("Sheet") {
            continue;
        }
        let mut features = tx.get(&sheet, "features")?;
        let Some(legacy) = features.as_object_mut().and_then(|f| f.remove(CHARTS)) else { continue };
        for (index, item) in legacy.as_array().into_iter().flatten().take(100_000).enumerate() {
            let Some(map) = item.as_object() else { continue };
            let mut parts = map.clone();
            let anchor = parts.remove("anchor").unwrap_or(Json::Null);
            let mut fields = Map::new();
            fields.insert("sheet".into(), json!(sheet.as_str()));
            fields.insert("z".into(), json!(index));
            fields.insert("frame".into(), json!({ "anchor": anchor }));
            let object = match tx.create_with_id(CHART, &migrated_id(&sheet, index), fields.clone()) {
                Ok(id) => id,
                Err(_) => tx.create(CHART, fields)?,
            };
            for (k, v) in parts {
                tx.map_set(&object, "parts", &k, v)?;
            }
        }
        tx.set(&sheet, "features", features)?;
        wrote = true;
    }
    Ok(wrote)
}
