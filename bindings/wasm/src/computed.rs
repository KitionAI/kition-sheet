//! Formula caches are consequences, summarized separately from stored cell entries.
use std::collections::{BTreeMap, VecDeque};

use kition_kit::{Carried, Carries};
use kition_sheet_engine::core::Value as CellValue;
use serde_json::{Value, json};

use crate::{SheetDocument, ids};

#[derive(Default)]
pub(crate) struct History {
    sheets: BTreeMap<String, VecDeque<(String, BTreeMap<String, CellValue>)>>,
}

impl SheetDocument {
    pub(crate) fn computed_digest(&self) -> Result<String, String> {
        let state = self.doc_state()?;
        let mut values = BTreeMap::new();
        for (index, sheet) in state.wb.sheets.iter().enumerate() {
            let axes = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(index));
            for (at, cell) in sheet.cells.iter().filter(|(_, cell)| cell.formula.is_some()) {
                let row = axes.and_then(|a| a.rows.get(at.row as usize)).map_or_else(|| format!("#{}", at.row), ToString::to_string);
                let col = axes.and_then(|a| a.cols.get(at.col as usize)).map_or_else(|| format!("#{}", at.col), ToString::to_string);
                values.insert(format!("{}!{row}:{col}", ids::sheet_tool_id(sheet)), cell.value.clone());
            }
        }
        let digest = kition_kit::session::sha256_hex(&serde_json::to_vec(&values).map_err(|e| e.to_string())?);
        let mut history = self.computed.try_borrow_mut().map_err(|e| e.to_string())?;
        let snapshots = history.sheets.entry("workbook".into()).or_default();
        if snapshots.back().is_none_or(|(hash, _)| hash != &digest) {
            snapshots.push_back((digest.clone(), values));
            while snapshots.len() > 2 {
                snapshots.pop_front();
            }
        }
        Ok(digest)
    }

    pub(crate) fn computed_carried(&self, wanted: &[Value], changes: &[Value]) -> Result<Vec<Carried>, String> {
        let history = self.computed.try_borrow().map_err(|e| e.to_string())?;
        let mut found = Vec::new();
        for change in changes.iter().filter(|change| wanted.contains(&change["id"])) {
            let Some(id) = change["id"].as_str() else { continue };
            let Some(snapshots) = history.sheets.get(id) else { continue };
            for edit in change["edits"].as_array().into_iter().flatten().filter(|edit| edit["field"] == "~computed") {
                let old = snapshots.iter().find(|(hash, _)| edit["before"] == *hash).map(|(_, values)| values);
                let new = snapshots.iter().find(|(hash, _)| edit["after"] == *hash).map(|(_, values)| values);
                if let (Some(old), Some(new)) = (old, new) {
                    let count = new.iter().filter(|(key, value)| old.get(*key).is_some_and(|before| before != *value)).count() as u64;
                    if count > 0 {
                        found.push(Carried { id: json!(id), carries: Carries { count, what: "computed cell values".into() } });
                    }
                }
            }
        }
        Ok(found)
    }
}
