//! Named cell styles hold their chosen definition, not the workbook's other styles.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise, window},
};
use kition_kit::Under;
use serde_json::{Value, json};

impl SheetDocument {
    pub(crate) fn cell_style_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if !matches!(command, "style.apply" | "style.add") {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let wb = &state.wb;
        if !state
            .kdm()
            .is_some_and(|kdm| kdm.replica().map_entries(&kition_kit::kdm::ObjectId::from_string("workbook"), "cellStyleDefinitions").is_ok())
        {
            return Ok(None);
        }
        let name = params.get("name").and_then(Value::as_str).unwrap_or("Normal");
        let mut under = if command == "style.apply" {
            let Some(under) = self.format_premises("format.fontName", params, si)? else { return Ok(None) };
            under
        } else {
            let Some(sheet) = wb.sheet(si) else { return Ok(None) };
            let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
            let at = params
                .get("fromCell")
                .and_then(Value::as_str)
                .and_then(area)
                .filter(|range| range.is_single())
                .map_or(state.selection.active, |range| range.start);
            let (Some(row), Some(col)) = (axes.rows.get(at.row as usize), axes.cols.get(at.col as usize)) else { return Ok(None) };
            let sid = ids::sheet_tool_id(sheet);
            let mut under = Under::default();
            under.objects.push(json!(sid));
            under.premises.push(premise(sid.clone(), Vec::<String>::new()));
            window(&mut under, &sid, "rows", at.row as usize, at.row as usize + 1);
            window(&mut under, &sid, "cols", at.col as usize, at.col as usize + 1);
            under.premises.push(premise(format!("cell:{}!{at}", sheet.sheet_id), ["style"]));
            under.premises.push(premise(format!("row:{row}"), ["~style"]).positions(["index"]));
            under.premises.push(premise(format!("col:{col}"), ["~style"]).positions(["index"]));
            under.premises.push(premise("workbook", ["props.defaultStyle"]));
            under
        };
        let chosen = kdm_model::cell_style_name_key(name);
        under.premises.push(premise("workbook", [format!("cellStyleDefinitions.{chosen}")]));
        if command == "style.apply" && name == "Title" && !wb.cell_styles.iter().any(|(stored, _)| stored.eq_ignore_ascii_case(name)) {
            under.premises.push(premise("workbook", ["theme.major_font"]));
        }
        merged(&mut under);
        Ok(Some(under))
    }
}
