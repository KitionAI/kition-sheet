//! Charts as `Chart` objects: each chart's anchor is its own register and each of its other
//! members (type, title, legend, series, style…) an entry of its `parts` map, so two people
//! changing different charts of a sheet, or different members of one chart, all keep their change
//! (`KitDrawing` DR14).
//!
//! As a `Shape` does (`crate::shapes`), a `Chart` names its sheet rather than being listed by it,
//! and a sheet's charts are the objects naming it in `z` order (ties by object id). Series ranges
//! and the source range are position tokens, so a row inserted above the data moves the chart's
//! ranges with it.

use kition_kit::kdm::{ObjectId, Replica, Tx};
use kition_sheet_model::Chart;
use serde_json::{Map, Value as Json, json};

use crate::KdmResult;
use crate::codec::{Decode, Encode};
use crate::features::{anchor_value, chart_decoded, chart_encoded, from_json, read_anchor, to_json};

/// The object type.
pub(crate) const CHART: &str = "Chart";

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
