//! A name follows its persistent target cell across positional edits.
use crate::{
    SheetDocument, ids,
    place::{merged, premise, window},
};
use kition_kit::Under;
use serde_json::{Value, json};

impl SheetDocument {
    pub(crate) fn named_cell_premises(&self, params: &Value, context: usize) -> Result<Option<Under>, String> {
        let state = self.doc_state()?;
        let wb = &state.wb;
        let Some(name) = params.get("name").and_then(Value::as_str) else { return Ok(None) };
        let Ok(resolved) = kition_sheet_engine::cmd::formulas::named_cell(wb, context, name) else { return Ok(None) };
        let Some(sheet) = wb.sheet(resolved.sheet) else { return Ok(None) };
        let Some(kdm) = state.kdm() else { return Ok(None) };
        let Some(axes) = kdm.model().ids().sheets.get(resolved.sheet) else { return Ok(None) };
        let (Some(row), Some(col)) = (axes.rows.get(resolved.at.row as usize), axes.cols.get(resolved.at.col as usize)) else { return Ok(None) };
        if sheet.cell(resolved.at).is_none() {
            return Ok(None);
        }
        let input = params.get("input").or_else(|| params.get("value")).and_then(Value::as_str).unwrap_or_default();
        let mut preview = wb.as_ref().clone();
        let parsed = kition_sheet_engine::cmd::edit::input_to_cell(input, resolved.sheet, resolved.at, &mut preview).map_err(crate::engine_error)?;
        if parsed.as_ref().is_some_and(|cell| cell.formula.is_some()) || params.get("array").and_then(Value::as_bool) == Some(true) {
            return Ok(None);
        }
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        // The fitted-row digest includes the destination's complete cell value and style.
        // Its persistent row id survives insertion without exposing an internal cell id.
        under.premises.push(premise(format!("row:{row}"), ["info", "~layout"]));
        under.premises.push(premise(format!("col:{col}"), ["info.style"]));
        under.premises.push(premise(sid, ["props.protection", "merges", "tables", "validations"]));
        under.premises.push(premise("workbook", ["props.defaultStyle", "props.dateSystem"]));
        for index in resolved.definitions {
            if let Some(definition) = wb.names.get(index) {
                under.premises.push(premise("workbook", [format!("names[name={}]", json!(definition.name))]));
            }
            if let Some(raw) = kdm.model().ids().names.get(index) {
                let id = format!("name:{raw}");
                under.objects.push(json!(id));
                under.premises.push(premise(id, ["name", "scope", "formula", "props"]));
            }
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64) {
            window(&mut under, "workbook", "sheets", index as usize, index as usize + 1);
        }
        merged(&mut under);
        Ok(Some(under))
    }
}
