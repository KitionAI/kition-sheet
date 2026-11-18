//! Kition's optional identity part carries no workbook content or replica history.
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use kition_sheet_model::{SheetIdentities, Workbook};
use serde::{Deserialize, Serialize};

use crate::{
    IoError,
    package::Package,
    write::{Out, Rels},
};

const PART: &str = "xl/kition/identities.json";
const REL: &str = "https://kition.ai/sheet/identities";

#[derive(Serialize, Deserialize)]
struct FileIdentities {
    sheets: BTreeMap<u32, SheetIdentities>,
    names: Vec<String>,
}

pub(crate) fn write(wb: &Workbook, out: &mut Out, rels: &mut Rels) -> Result<(), IoError> {
    let sheets: BTreeMap<_, _> = wb.sheets.iter().filter(|s| !s.identities.object.is_empty()).map(|s| (s.sheet_id, s.identities.clone())).collect();
    if !sheets.is_empty() {
        let bytes = serde_json::to_vec(&FileIdentities { sheets, names: wb.identities.names.clone() }).map_err(|e| IoError::Format(e.to_string()))?;
        rels.add_uri(REL, "kition/identities.json");
        out.part(PART, Some("application/vnd.kition.sheet.identities+json"), bytes);
    }
    Ok(())
}

pub(crate) fn read(wb: &mut Workbook, pkg: &mut Package<'_>) -> Result<(), IoError> {
    let Some(bytes) = pkg.read_limited(PART, 16 * 1024 * 1024)? else { return Ok(()) };
    let FileIdentities { sheets, names } =
        serde_json::from_slice::<FileIdentities>(&bytes).map_err(|e| IoError::Format(format!("invalid identities: {e}")))?;
    let mut seen = HashSet::new();
    for id in &names {
        if id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') || !seen.insert(id) {
            return Err(IoError::Format("invalid or duplicate defined name identity".into()));
        }
    }
    wb.identities.names = names.clone();
    let mut table_ids = HashSet::new();
    let mut pivot_ids = HashSet::new();
    for identity in sheets.values() {
        let [row, col] = &identity.axes;
        if row.len() > 1 << 18 || col.len() > 16_384 {
            return Err(IoError::TooLarge("axis identities exceed the materialized grid".into()));
        }
        for id in std::iter::once(&identity.object)
            .chain(row)
            .chain(col)
            .chain(identity.tombstones.iter().flat_map(|axis| axis.values()))
            .chain(identity.cells.values().map(|cell| &cell.object))
            .chain(identity.drawings.values())
        {
            if id.is_empty()
                || id.len() > 128
                || !id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
                || !seen.insert(id)
            {
                return Err(IoError::Format("invalid or duplicate axis identity".into()));
            }
        }
        if identity.cells.keys().any(|address| kition_sheet_core::CellRef::parse(address).is_none()) {
            return Err(IoError::Format("invalid cell identity address".into()));
        }
        if identity
            .cells
            .values()
            .any(|cell| cell.by.as_ref().is_some_and(|by| by.len() > 128) || cell.key.as_ref().is_some_and(|key| key.len() > 512))
        {
            return Err(IoError::Format("invalid cell provenance".into()));
        }
    }
    for sheet in &mut wb.sheets {
        if let Some(ids) = sheets.get(&sheet.sheet_id) {
            let sheet = Arc::make_mut(sheet);
            for (stored, count, seen) in [(&ids.tables, sheet.tables.len(), &mut table_ids), (&ids.pivots, sheet.pivots.len(), &mut pivot_ids)] {
                if !stored.is_empty() && (stored.len() != count || stored.iter().any(|id| *id == 0 || !seen.insert(*id))) {
                    return Err(IoError::Format("invalid table or pivot identities".into()));
                }
            }
            for (table, id) in sheet.tables.iter_mut().zip(&ids.tables) {
                table.id = *id;
            }
            for (pivot, id) in sheet.pivots.iter_mut().zip(&ids.pivots) {
                pivot.id = *id;
            }
            sheet.identities = ids.clone();
        }
    }
    Ok(())
}
