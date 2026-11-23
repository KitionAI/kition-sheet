//! Exact rectangle coverage without expanding whole-column rules into individual cells.
//! A rectangle is partitioned into aligned power-of-two row and column blocks. A destination
//! holds every block that intersects it, so new, removed and moved covering rules are visible.
use std::collections::BTreeMap;

use kition_kit::{
    kdm::{ObjectId, Tx},
    session::sha256_hex,
};
use kition_sheet_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use kition_sheet_model::Sheet;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{KdmResult, kdm_err};

const MAX_FACTS: usize = 100_000;
const ROW_BITS: u32 = 20;
const COL_BITS: u32 = 14;

fn partition(mut first: u32, last: u32, bits: u32) -> Vec<(u32, u32)> {
    let mut blocks = Vec::new();
    while first <= last {
        let mut level = first.trailing_zeros().min(bits);
        while (1u32 << level) > last - first + 1 {
            level = level.saturating_sub(1);
        }
        blocks.push((level, first >> level));
        first = first.saturating_add(1u32 << level);
    }
    blocks
}

fn key(row: (u32, u32), col: (u32, u32)) -> String {
    format!("r{}_{}c{}_{}", row.0, row.1, col.0, col.1)
}

fn digest(value: &impl Serialize) -> KdmResult<String> {
    let bytes = serde_json::to_vec(value).map_err(kdm_err)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(kdm_err("a constraint exceeds the premise size limit"));
    }
    Ok(sha256_hex(&bytes))
}

pub(crate) fn facts(sheet: &Sheet) -> KdmResult<Value> {
    let mut facts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut assignments = 0usize;
    let mut add = |range: RangeRef, value: &str| -> KdmResult<()> {
        if range.start.row >= MAX_ROWS || range.start.col >= MAX_COLS {
            return Err(kdm_err("constraint starts outside the sheet"));
        }
        let rows = partition(range.start.row, range.end.row.min(MAX_ROWS - 1), ROW_BITS);
        let cols = partition(range.start.col, range.end.col.min(MAX_COLS - 1), COL_BITS);
        for row in rows {
            for &col in &cols {
                facts.entry(key(row, col)).or_default().push(value.to_owned());
                assignments = assignments.saturating_add(1);
                if facts.len() > MAX_FACTS || assignments > MAX_FACTS * 10 {
                    return Err(kdm_err("constraints exceed the premise fact limit"));
                }
            }
        }
        Ok(())
    };
    for range in &sheet.merges {
        add(*range, &digest(&json!({"merge":range}))?)?;
    }
    for table in &sheet.tables {
        let value = digest(&json!({"table":table}))?;
        add(table.range, &value)?;
        // Entry can grow a table directly beneath it or to its right, excluding the corner.
        if !table.totals_row && table.range.end.row < MAX_ROWS - 1 {
            let row = table.range.end.row + 1;
            add(RangeRef::new(CellRef::new(row, table.range.start.col), CellRef::new(row, table.range.end.col)), &value)?;
        }
        if table.range.end.col < MAX_COLS - 1 {
            let col = table.range.end.col + 1;
            add(RangeRef::new(CellRef::new(table.range.start.row, col), CellRef::new(table.range.end.row, col)), &value)?;
        }
    }
    for validation in &sheet.validations {
        let value = digest(&json!({"validation":validation}))?;
        for range in &validation.ranges {
            add(*range, &value)?;
        }
    }
    let mut result = serde_json::Map::new();
    for (key, mut values) in facts {
        values.sort();
        values.dedup();
        result.insert(key, json!(digest(&values)?));
    }
    Ok(Value::Object(result))
}

pub(crate) fn write(tx: &mut Tx<'_>, object: &ObjectId, sheet: &Sheet) -> KdmResult<bool> {
    let old: BTreeMap<_, _> = tx.map_entries(object, "constraints")?.into_iter().collect();
    let facts = facts(sheet)?;
    let Some(wanted) = facts.as_object() else { return Ok(false) };
    let mut changed = false;
    for key in old.keys() {
        if !wanted.contains_key(key) {
            tx.map_delete(object, "constraints", key)?;
            changed = true;
        }
    }
    for (key, value) in wanted {
        if old.get(key) != Some(value) {
            tx.map_set(object, "constraints", key, value.clone())?;
            changed = true;
        }
    }
    Ok(changed)
}

/// Field paths of every possible covering block. Large destinations use a bounded aggregate.
pub fn constraint_fields(range: RangeRef) -> Vec<String> {
    let range = RangeRef::new(
        CellRef::new(range.start.row.min(MAX_ROWS - 1), range.start.col.min(MAX_COLS - 1)),
        CellRef::new(range.end.row.min(MAX_ROWS - 1), range.end.col.min(MAX_COLS - 1)),
    );
    let intersecting = |first: u32, last: u32, bits: u32| {
        (0..=bits).flat_map(move |level| (first >> level..=last >> level).map(move |block| (level, block))).collect::<Vec<_>>()
    };
    let row_count = (0..=ROW_BITS).map(|level| u64::from((range.end.row >> level) - (range.start.row >> level) + 1)).sum::<u64>();
    let col_count = (0..=COL_BITS).map(|level| u64::from((range.end.col >> level) - (range.start.col >> level) + 1)).sum::<u64>();
    if row_count * col_count > MAX_FACTS as u64 {
        return vec!["constraints".into()];
    }
    let rows = intersecting(range.start.row, range.end.row, ROW_BITS);
    let cols = intersecting(range.start.col, range.end.col, COL_BITS);
    rows.into_iter().flat_map(|row| cols.iter().map(move |&col| format!("constraints.{}", key(row, col)))).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn indexed_coverage_matches_rectangles_and_bounds_whole_column_work() {
        let mut sheet = Sheet::new("Data");
        sheet.merges = [(1, 2, 8, 5), (0, 10, 18, 15), (12, 0, 17, 2)]
            .map(|(r, c, end_r, end_c)| RangeRef::new(CellRef::new(r, c), CellRef::new(end_r, end_c)))
            .to_vec();
        let index = facts(&sheet).unwrap();
        for row in 0..24 {
            for col in 0..20 {
                let cell = CellRef::new(row, col);
                let found = constraint_fields(RangeRef::cell(cell)).iter().any(|path| index.get(path.trim_start_matches("constraints.")).is_some());
                assert_eq!(found, sheet.merges.iter().any(|range| range.contains(cell)), "{cell}");
            }
        }
        sheet.merges = vec![RangeRef::new(CellRef::new(0, 1), CellRef::new(MAX_ROWS - 1, 1))];
        let index = facts(&sheet).unwrap();
        assert_eq!(index.as_object().unwrap().len(), 1, "whole-column coverage needs one block");
        assert_eq!(constraint_fields(RangeRef::cell(CellRef::new(MAX_ROWS - 1, MAX_COLS - 1))).len(), 315);
        assert_eq!(constraint_fields(RangeRef::new(CellRef::default(), CellRef::new(MAX_ROWS - 1, MAX_COLS - 1))), ["constraints"]);
    }
}
