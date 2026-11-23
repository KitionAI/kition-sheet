//! Kition's optional identity part carries no workbook content or replica history: the object ids
//! of the sheets, their rows, columns, cells and drawings, and of the defined names, so a workbook
//! saved and opened again keeps them. Each sheet's ids are written with a fingerprint of the
//! worksheet part they were written beside, and the names' with one of the workbook part: when
//! another program changed a part (a row inserted moves every row after it), its fingerprint no
//! longer matches and its ids are dropped, so no id names the wrong row, cell or name.
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
    /// The fingerprint of the workbook part the names' ids were written beside.
    #[serde(default)]
    workbook: String,
    sheets: BTreeMap<u32, FileSheet>,
    names: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct FileSheet {
    /// The fingerprint of the worksheet part the sheet's ids were written beside.
    #[serde(default)]
    fingerprint: String,
    #[serde(flatten)]
    ids: SheetIdentities,
}

/// A part's fingerprint: its length and a 64-bit FNV-1a hash of its bytes (it tells a part
/// another program rewrote, not one made to collide).
fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}-{}", bytes.len())
}

/// Writes the identity part. `sheet_parts` are the worksheet parts written for `wb`'s sheets, in
/// order, and `workbook_part` the workbook part.
pub(crate) fn write(wb: &Workbook, out: &mut Out, rels: &mut Rels, sheet_parts: &[String], workbook_part: &[u8]) -> Result<(), IoError> {
    let mut sheets = BTreeMap::new();
    for (sheet, part) in wb.sheets.iter().zip(sheet_parts) {
        if sheet.identities.object.is_empty() {
            continue;
        }
        let Some(bytes) = out.part_bytes(part) else { continue };
        sheets.insert(sheet.sheet_id, FileSheet { fingerprint: fingerprint(bytes), ids: sheet.identities.clone() });
    }
    if !sheets.is_empty() {
        let file = FileIdentities { workbook: fingerprint(workbook_part), sheets, names: wb.identities.names.clone() };
        let bytes = serde_json::to_vec(&file).map_err(|e| IoError::Format(e.to_string()))?;
        rels.add_uri(REL, "kition/identities.json");
        out.part(PART, Some("application/vnd.kition.sheet.identities+json"), bytes);
    }
    Ok(())
}

/// Reads the identity part into `wb`: the ids of each sheet whose worksheet part (`sheet_parts`,
/// parallel to `wb.sheets`) still has the fingerprint they were written with, and the names' ids
/// while the workbook part (`workbook_part`) does. Ids another program's change made stale are
/// dropped (the objects get new ones).
pub(crate) fn read(wb: &mut Workbook, pkg: &mut Package<'_>, sheet_parts: &[Option<String>], workbook_part: &str) -> Result<(), IoError> {
    let Some(bytes) = pkg.read_limited(PART, 16 * 1024 * 1024)? else { return Ok(()) };
    let file = serde_json::from_slice::<FileIdentities>(&bytes).map_err(|e| IoError::Format(format!("invalid identities: {e}")))?;
    fn unchanged(pkg: &mut Package<'_>, part: &str, written: &str) -> Result<bool, IoError> {
        Ok(!written.is_empty() && pkg.read(part)?.is_some_and(|bytes| fingerprint(&bytes) == written))
    }
    let names = if unchanged(pkg, workbook_part, &file.workbook)? { file.names } else { Vec::new() };
    let mut sheets: BTreeMap<u32, SheetIdentities> = BTreeMap::new();
    for (sheet, part) in wb.sheets.iter().zip(sheet_parts) {
        let (Some(part), Some(entry)) = (part, file.sheets.get(&sheet.sheet_id)) else { continue };
        if !sheets.contains_key(&sheet.sheet_id) && unchanged(pkg, part, &entry.fingerprint)? {
            sheets.insert(sheet.sheet_id, entry.ids.clone());
        }
    }
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
