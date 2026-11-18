//! Bounded directional copies of materialized constant source cells.
//!
//! Formula sources, whole-line ranges, absent source cells and unmaterialized
//! axes retain the existing document fallback. Copy mode reads the source Cell's
//! raw style, not an inherited source-row/column format, and copies cell pictures.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise, window},
};
use kition_kit::Under;
use kition_sheet_engine::core::{CellRef, RangeRef};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const LIMIT: u64 = 100_000;

impl SheetDocument {
    pub(crate) fn fill_read_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if !matches!(command, "edit.fillDown" | "edit.fillRight" | "edit.fillUp" | "edit.fillLeft") {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        // target_ranges parses every explicit area, then target_range takes its
        // first. Do not silently narrow a malformed trailing area.
        let Some(text) = params.get("range").or_else(|| params.get("cell")).and_then(Value::as_str) else { return Ok(None) };
        let body = text.rsplit_once('!').map_or(text, |(_, body)| body);
        let Some(ranges) = body.split(',').map(area).collect::<Option<Vec<_>>>() else { return Ok(None) };
        let Some(mut target) = ranges.first().copied() else { return Ok(None) };
        if target.is_full_cols() || target.is_full_rows() || target.count() > LIMIT {
            return Ok(None);
        }
        // The one-line forward cases copy from the preceding neighbour.
        if command == "edit.fillDown" && target.height() == 1 {
            target.start.row = target.start.row.saturating_sub(1);
        }
        if command == "edit.fillRight" && target.width() == 1 {
            target.start.col = target.start.col.saturating_sub(1);
        }
        if target.count() > LIMIT {
            return Ok(None);
        }
        let (source, destination) = match command {
            "edit.fillDown" if target.height() > 1 => (
                RangeRef::new(target.start, CellRef::new(target.start.row, target.end.col)),
                RangeRef::new(CellRef::new(target.start.row.saturating_add(1), target.start.col), target.end),
            ),
            "edit.fillRight" if target.width() > 1 => (
                RangeRef::new(target.start, CellRef::new(target.end.row, target.start.col)),
                RangeRef::new(CellRef::new(target.start.row, target.start.col.saturating_add(1)), target.end),
            ),
            "edit.fillUp" if target.height() > 1 => (
                RangeRef::new(CellRef::new(target.end.row, target.start.col), target.end),
                RangeRef::new(target.start, CellRef::new(target.end.row.saturating_sub(1), target.end.col)),
            ),
            "edit.fillLeft" if target.width() > 1 => (
                RangeRef::new(CellRef::new(target.start.row, target.end.col), target.end),
                RangeRef::new(target.start, CellRef::new(target.end.row, target.end.col.saturating_sub(1))),
            ),
            // Even a no-op Fill enters the shared edit machinery. Leave that
            // uncommon path on the established conservative declaration.
            _ => return Ok(None),
        };
        // Empty-source creation has the pinned SDK's unresolved absent-source
        // ambiguity. The conservative fallback keeps that case guarded.
        if source.iter().any(|at| sheet.cell(at).is_none_or(|cell| cell.formula.is_some())) {
            return Ok(None);
        }
        let Some(kdm) = state.kdm() else { return Ok(None) };
        let Some(axes) = kdm.model().ids().sheets.get(si) else { return Ok(None) };
        if axes.rows.get(target.end.row as usize).is_none()
            || axes.cols.get(target.end.col as usize).is_none()
            || kdm.replica().map_entries(&axes.object, "constraints").is_err()
        {
            return Ok(None);
        }
        let mut under = Under::default();
        let sid = ids::sheet_tool_id(sheet);
        under.objects.push(json!(sid));
        let mut fields = vec!["props.protection".to_owned()];
        if params.get("sheet").is_some_and(Value::is_string) || text.contains('!') {
            fields.push("name".into());
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) {
            window(&mut under, "workbook", "sheets", index, index.saturating_add(1));
        }
        window(&mut under, &sid, "rows", target.start.row as usize, target.end.row as usize + 1);
        window(&mut under, &sid, "cols", target.start.col as usize, target.end.col as usize + 1);
        fields.extend(kdm_model::constraint_fields(destination));
        let mut styles = BTreeSet::new();
        for at in source.iter() {
            let Some(cell) = sheet.cell(at) else { return Ok(None) };
            let id = format!("cell:{}!{at}", sheet.sheet_id);
            under.objects.push(json!(id));
            under.premises.push(premise(id, ["input", "text", "style", "array", "picture"]));
            let Some(key) = kdm.model().ids().styles.get(cell.style.0 as usize) else { return Ok(None) };
            styles.insert(format!("styles.{key}.style"));
        }
        for at in destination.iter() {
            let id = format!("cell:{}!{at}", sheet.sheet_id);
            under.objects.push(json!(id));
            under.premises.push(premise(id, ["input", "text", "style", "array", "picture"]));
            // Shared check_protection reads the OLD effective lock flag before
            // the destination's style is replaced by the copied raw style.
            if sheet.is_protected() {
                if sheet.cell(at).is_none() {
                    let Some(row) = axes.rows.get(at.row as usize) else { return Ok(None) };
                    under.premises.push(premise(format!("row:{row}"), ["info.style"]).positions(["index"]));
                    if sheet.rows.get(&at.row).and_then(|row| row.style).is_none() {
                        let Some(col) = axes.cols.get(at.col as usize) else { return Ok(None) };
                        under.premises.push(premise(format!("col:{col}"), ["info.style"]).positions(["index"]));
                    }
                }
                let Some(key) = kdm.model().ids().styles.get(sheet.style_id(at).0 as usize) else { return Ok(None) };
                styles.insert(format!("styles.{key}.style.protection.locked"));
            }
        }
        under.premises.push(premise(sid, fields));
        under.premises.push(premise("workbook", ["props.defaultStyle", "props.dateSystem"]));
        under.premises.push(premise("workbook", styles));
        self.entry_row_premises(&mut under, si, destination);
        self.hold_shared_edit_reads(&mut under, si, &[destination], false)?;
        merged(&mut under);
        Ok(Some(under))
    }
}
