//! Registry and formula-view writes do not read unrelated cell contents.
//!
//! Table-style definitions still share a conservative registry hold. Replacing
//! or deleting a style also holds table definitions to discover every user; that
//! table-level precision can be narrowed separately without exposing cell data.
use crate::{
    SheetDocument, ids,
    place::{merged, premise},
};
use kition_kit::Under;
use serde_json::{Value, json};

impl SheetDocument {
    pub(crate) fn registry_read_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if command == "formulas.showFormulas" {
            let state = self.doc_state()?;
            let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
            let sid = ids::sheet_tool_id(sheet);
            let mut under = Under::default();
            under.objects.push(json!(sid));
            under.premises.push(premise(sid, ["props.showFormulas"]));
            // The direct edit changes this sheet even when an explicit `on`
            // repeats the current flag, so its connectors are rerouted.
            self.hold_shared_edit_reads(&mut under, si, &[], false)?;
            merged(&mut under);
            return Ok(Some(under));
        }
        if !matches!(command, "table.newStyle" | "table.deleteStyle" | "table.setDefaultStyle") {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let mut under = Under::default();
        under.objects.push(json!("workbook"));
        // Names, chosen definition/UID, default and fallback resolution live in
        // this register. An unrelated registry change can still refuse the call.
        under.premises.push(premise("workbook", ["props.tableStyles"]));
        if command != "table.setDefaultStyle" {
            // Both handlers mark the edit structural, even creating an unused
            // style. They synchronize every stale header before recalculation.
            self.hold_header_sync_reads(&mut under)?;
            let replaced =
                if command == "table.newStyle" { params.get("replace").and_then(Value::as_str) } else { params.get("name").and_then(Value::as_str) };
            if let Some(replaced) = replaced {
                // Every sheet is searched for tables using the old name. The
                // membership hold also detects a new sheet containing such a user.
                under.premises.push(premise("workbook", ["~tabs"]));
                for (owner, sheet) in state.wb.sheets.iter().enumerate() {
                    under.premises.push(premise(ids::sheet_tool_id(sheet), ["features.tables"]));
                    if sheet.tables.iter().any(|table| table.style.eq_ignore_ascii_case(replaced)) {
                        self.hold_shared_edit_reads(&mut under, owner, &[], false)?;
                    }
                }
            }
        }
        merged(&mut under);
        Ok(Some(under))
    }
}
