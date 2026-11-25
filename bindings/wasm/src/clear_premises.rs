//! Sparse metadata clears hold map entries; cell clears also hold styles and fitted rows.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise, window},
};
use kition_kit::Under;
use serde_json::{Value, json};

impl SheetDocument {
    pub(crate) fn clear_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if !matches!(command, "edit.clearAll" | "edit.clearFormats" | "edit.clearComments" | "edit.clearHyperlinks") {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let ranges = if let Some(text) = params.get("range").or_else(|| params.get("cell")).and_then(Value::as_str) {
            let body = text.rsplit_once('!').map_or(text, |(_, body)| body);
            let Some(ranges) = body.split(',').map(area).collect::<Option<Vec<_>>>() else { return Ok(None) };
            ranges
        } else {
            state.selection.ranges.clone()
        };
        if ranges.iter().try_fold(0u64, |count, range| count.checked_add(range.count())).is_none_or(|count| count > 100_000) {
            return Ok(None);
        }
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
        if ranges.iter().any(|range| axes.rows.get(range.end.row as usize).is_none() || axes.cols.get(range.end.col as usize).is_none()) {
            return Ok(None);
        }
        let cells = matches!(command, "edit.clearAll" | "edit.clearFormats");
        let mut under = if cells {
            let Some(mut under) = self.format_premises("format.fontName", params, si)? else { return Ok(None) };
            for premise in &mut under.premises {
                if premise.id.as_str() == Some("workbook") {
                    for fields in premise.fields.iter_mut() {
                        fields.retain(|field| !field.starts_with("styles.") || command == "edit.clearFormats");
                        for field in fields.iter_mut().filter(|field| field.starts_with("styles.")) {
                            field.push_str(".checkbox");
                        }
                    }
                }
            }
            under
        } else {
            Under::default()
        };
        let sid = ids::sheet_tool_id(sheet);
        under.objects.push(json!(sid));
        let mut fields = Vec::new();
        if params.get("sheet").is_some_and(Value::is_string)
            || ["range", "cell"].iter().any(|key| params.get(key).and_then(Value::as_str).is_some_and(|text| text.contains('!')))
        {
            fields.push("name".into());
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64) {
            window(&mut under, "workbook", "sheets", index as usize, index as usize + 1);
        }
        for range in ranges {
            window(&mut under, &sid, "rows", range.start.row as usize, range.end.row as usize + 1);
            window(&mut under, &sid, "cols", range.start.col as usize, range.end.col as usize + 1);
            if cells {
                if !state.kdm().is_some_and(|kdm| kdm.replica().map_entries(&axes.object, "constraints").is_ok()) {
                    return Ok(None);
                }
                fields.extend(kdm_model::constraint_fields(range));
            }
            for at in range.iter() {
                let (Some(row), Some(col)) = (axes.rows.get(at.row as usize), axes.cols.get(at.col as usize)) else { return Ok(None) };
                if matches!(command, "edit.clearAll" | "edit.clearComments") {
                    fields.push(format!("comments.row:{row}:col:{col}"));
                }
                if matches!(command, "edit.clearAll" | "edit.clearHyperlinks") {
                    fields.push(format!("hyperlinks.row:{row}:col:{col}"));
                }
            }
        }
        under.premises.push(premise(sid, fields));
        merged(&mut under);
        Ok(Some(under))
    }
}
