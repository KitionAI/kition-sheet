//! Outline levels discover detail boundaries; cell values are unrelated.
use crate::{
    SheetDocument, ids,
    place::{merged, premise, window},
};
use kition_kit::Under;
use serde_json::{Value, json};

impl SheetDocument {
    pub(crate) fn outline_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if !matches!(command, "data.group" | "data.ungroup" | "data.hideDetail" | "data.showDetail") {
            return Ok(None);
        }
        let (rows, first, last) = kition_sheet_engine::cmd::data::outline_lines(&self.session, params).map_err(crate::engine_error)?;
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
        let (map, lines, kind) = if rows { (&sheet.rows, &axes.rows, "row") } else { (&sheet.cols, &axes.cols, "col") };
        if last.saturating_sub(first) >= 100_000 || lines.get(last as usize).is_none() {
            return Ok(None);
        }
        let detail = matches!(command, "data.hideDetail" | "data.showDetail");
        let level = (first..=last).filter_map(|index| map.get(&index).map(|info| info.outline)).max().unwrap_or(0);
        let (mut lo, mut hi) = (first, last);
        if detail && level > 0 {
            while lo > 0 && map.get(&(lo - 1)).is_some_and(|info| info.outline >= level) {
                lo -= 1;
                if hi.saturating_sub(lo) >= 100_000 {
                    return Ok(None);
                }
            }
            while hi.checked_add(1).and_then(|next| map.get(&next)).is_some_and(|info| info.outline >= level) {
                hi = hi.saturating_add(1);
                if hi.saturating_sub(lo) >= 100_000 {
                    return Ok(None);
                }
            }
        }
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        under.premises.push(premise(sid.clone(), ["props.protection"]));
        let boundary = detail && level > 0;
        let start = if boundary { lo.saturating_sub(1) } else { first };
        let end = if boundary { hi.saturating_add(1) } else { last };
        window(&mut under, &sid, if rows { "rows" } else { "cols" }, start as usize, end as usize + 1);
        for index in start..=end {
            let Some(raw) = lines.get(index as usize) else {
                // A new level beyond materialized axes changes the boundary window.
                continue;
            };
            let hidden = if detail {
                level > 0 && index >= lo && index <= hi
            } else {
                command == "data.ungroup" && map.get(&index).is_none_or(|info| info.outline <= 1)
            };
            let mut fields = vec!["~outline"];
            if hidden {
                fields.push("~hidden");
            }
            under.premises.push(premise(format!("{kind}:{raw}"), fields).positions(["index"]));
        }
        merged(&mut under);
        Ok(Some(under))
    }
}
