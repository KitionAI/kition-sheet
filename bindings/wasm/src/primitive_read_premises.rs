//! Local materialized-checkbox and direct-stroke premises.
use crate::{
    SheetDocument, ids,
    place::{merged, premise, window},
};
use kition_kit::Under;
use kition_sheet_engine::{
    core::{CellRef, RangeRef, Value as CellValue},
    model::StyleId,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const LIMIT: usize = 100_000;

impl SheetDocument {
    pub(crate) fn primitive_read_premises(&self, command: &str, params: &Value, si: usize) -> Result<Option<Under>, String> {
        match command {
            "edit.formatPainter" if params.get("apply").is_none() => self.painter_pickup_premises(params, si),
            "formulas.insertFunction" => self.function_request_premises(si),
            "insert.shape" | "insert.icons" | "insert.textBox" => self.anchored_creation_premises(params, si),
            "insert.symbol" => {
                // insert.rs::insert_symbol appends to the active cell's raw
                // input, then enters the result through cell.set. The composed
                // text may create new formula dependencies or a fitted height.
                let state = self.doc_state()?;
                let at = state.selection.active;
                let current = state.wb.active().and_then(|sheet| sheet.cell(at)).map(|cell| cell.input_text()).unwrap_or_default();
                let symbol = params.get("char").and_then(Value::as_str).unwrap_or_default();
                let entry = json!({"cell":at.a1(),"input":format!("{current}{symbol}")});
                let under = self.call_premises("cell.set", &entry)?;
                self.shared_edit_premises("cell.set", &entry, under)
            }
            "cell.toggleCheckbox" => self.checkbox_toggle_premises(params, si),
            // These commands only choose a personal drawing tool or its defaults.
            // Their editor-state reads are declared separately from workbook facts.
            "draw.pen" | "draw.pencil" | "draw.highlighter" | "draw.eraser" | "draw.select" | "draw.shape" | "draw.lasso" => {
                Ok(Some(Under::default()))
            }
            "draw.stroke" => {
                // This handler filters malformed points, then keeps its first 20,000
                // finite samples. Only the minimum corner determines its cell anchor.
                // Passing all samples through drawing.ink would apply its different cap.
                let points: Vec<_> = params
                    .get("points")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|point| Some([point.get(0)?.as_f64()?, point.get(1)?.as_f64()?]))
                    .filter(|point| point[0].is_finite() && point[1].is_finite())
                    .take(20_000)
                    .collect();
                if points.len() < 2 {
                    return Ok(None);
                }
                let x = points.iter().map(|point| point[0]).fold(f64::INFINITY, f64::min).max(0.0);
                let y = points.iter().map(|point| point[1]).fold(f64::INFINITY, f64::min).max(0.0);
                self.object_read_premises("drawing.ink", &json!({"strokes":[{"points":[[x,y]]}]}), si)
            }
            _ => Ok(None),
        }
    }

    fn painter_pickup_premises(&self, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if params.get("cancel").and_then(Value::as_bool) == Some(true)
            || self.session.format_painter.is_some() && params.get("sticky").and_then(Value::as_bool).is_none()
        {
            return Ok(Some(Under::default()));
        }
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let mut source = state.selection.current();
        loop {
            let expanded = sheet.merges.iter().filter(|merge| merge.intersects(&source)).fold(source, |range, merge| range.union(merge));
            if expanded == source {
                break;
            }
            source = expanded;
        }
        // Retain the audited source-entry guards, including inherited formats and
        // merge coverage, without holding unrelated objects cloned in the snapshot.
        let entry = json!({"range":source.a1(),"input":""});
        let under = self.call_premises("range.fill", &entry)?;
        let Some(mut under) = self.shared_edit_premises("range.fill", &entry, under)? else { return Ok(None) };
        for held in &mut under.premises {
            if held.id == "workbook"
                && let Some(fields) = &mut held.fields
            {
                fields.retain(|field| field != "props.dateSystem");
            }
        }
        under.premises.push(premise("workbook", ["theme", "cellStyles"]));
        merged(&mut under);
        Ok(Some(under))
    }

    fn anchored_creation_premises(&self, params: &Value, si: usize) -> Result<Option<Under>, String> {
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
        let at = params
            .get("at")
            .and_then(Value::as_str)
            .and_then(|text| text.rsplit('!').next())
            .and_then(CellRef::parse)
            .unwrap_or(state.selection.active);
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        under.premises.push(premise(sid.clone(), Vec::<String>::new()));
        under.premises.push(premise("workbook", ["~nextObjectId"]));
        for (field, kind, index, lines) in [("rows", "row", at.row, &axes.rows), ("cols", "col", at.col, &axes.cols)] {
            let Some(raw) = lines.get(index as usize) else { return Ok(None) };
            window(&mut under, &sid, field, index as usize, index as usize + 1);
            under.premises.push(premise(format!("{kind}:{raw}"), Vec::<String>::new()).positions(["index"]));
        }
        // These legacy constructors store an anchored object without evaluating
        // sheet geometry. The shared edit pass adds connector rerouting inputs.
        merged(&mut under);
        Ok(Some(under))
    }

    fn function_request_premises(&self, si: usize) -> Result<Option<Under>, String> {
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) else { return Ok(None) };
        let source = state.selection.active;
        let anchor = sheet.merge_at(source).map_or(source, |merge| merge.start);
        let Some(cell) = sheet.cell(anchor) else { return Ok(None) };
        let style = state.wb.styles.get(sheet.style_id(anchor));
        if style.checkbox && (cell.formula.is_some() || sheet.spill.contains_key(&anchor)) {
            return Ok(None);
        }
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        under.premises.push(premise(sid.clone(), kdm_model::constraint_fields(RangeRef::cell(source))));
        for at in BTreeSet::from([source, anchor]) {
            for (field, kind, index, lines) in [("rows", "row", at.row, &axes.rows), ("cols", "col", at.col, &axes.cols)] {
                let Some(raw) = lines.get(index as usize) else { return Ok(None) };
                window(&mut under, &sid, field, index as usize, index as usize + 1);
                under.premises.push(premise(format!("{kind}:{raw}"), Vec::<String>::new()).positions(["index"]));
            }
        }
        let id = format!("cell:{}!{anchor}", sheet.sheet_id);
        under.premises.push(premise(id.clone(), ["style"]));
        if cell.style == StyleId::DEFAULT {
            under.premises.push(premise("workbook", ["props.defaultStyle"]));
        }
        if style.checkbox {
            under.premises.push(premise(id, ["input", "text"]));
        }
        merged(&mut under);
        Ok(Some(under))
    }

    fn checkbox_toggle_premises(&self, params: &Value, si: usize) -> Result<Option<Under>, String> {
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let Some(kdm) = state.kdm() else { return Ok(None) };
        let Some(axes) = kdm.model().ids().sheets.get(si) else { return Ok(None) };
        if kdm.replica().map_entries(&axes.object, "constraints").is_err() {
            return Ok(None);
        }
        let explicit = params.get("cell").and_then(Value::as_str).and_then(|cell| cell.rsplit('!').next()).and_then(CellRef::parse);
        let sources = if let Some(at) = explicit {
            BTreeSet::from([at])
        } else {
            if state
                .selection
                .ranges
                .iter()
                .try_fold(0u64, |count, range| count.checked_add(range.count().min(LIMIT as u64)))
                .is_none_or(|count| count > LIMIT as u64)
            {
                return Ok(None);
            }
            state.selection.ranges.iter().flat_map(|range| range.iter().take(LIMIT)).collect()
        };
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        let mut fields = BTreeSet::new();
        if params.get("sheet").is_some_and(Value::is_string) || params.get("cell").and_then(Value::as_str).is_some_and(|cell| cell.contains('!')) {
            fields.insert("name".to_owned());
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) {
            window(&mut under, "workbook", "sheets", index, index.saturating_add(1));
        }
        let mut entered_edit = false;
        let mut changed = Vec::new();
        for source in sources {
            // An explicit cell is redirected to the merged anchor. The selection
            // path checks the anchor but keeps each selected coordinate for writing.
            let target = if explicit.is_some() { sheet.merge_at(source).map_or(source, |merge| merge.start) } else { source };
            let anchor = sheet.merge_at(target).map_or(target, |merge| merge.start);
            // The pinned SDK cannot always distinguish a newly created source
            // from an unchanged absent cell. Keep empty eligibility conservative.
            if sheet.cell(anchor).is_none() || sheet.cell(target).is_none() {
                return Ok(None);
            }
            for at in BTreeSet::from([source, target, anchor]) {
                for (field, kind, index, lines) in [("rows", "row", at.row, &axes.rows), ("cols", "col", at.col, &axes.cols)] {
                    let Some(raw) = lines.get(index as usize) else { return Ok(None) };
                    window(&mut under, &sid, field, index as usize, index as usize + 1);
                    under.premises.push(premise(format!("{kind}:{raw}"), Vec::<String>::new()).positions(["index"]));
                }
                fields.extend(kdm_model::constraint_fields(RangeRef::cell(at)));
            }
            let anchor_id = format!("cell:{}!{anchor}", sheet.sheet_id);
            under.objects.push(json!(anchor_id));
            under.premises.push(premise(anchor_id.clone(), ["style"]));
            let style = sheet.style_id(anchor);
            if style == StyleId::DEFAULT {
                under.premises.push(premise("workbook", ["props.defaultStyle"]));
            }
            if state.wb.styles.get(style).checkbox {
                // Formula/spill eligibility needs its evaluated value, beyond the
                // literal input register. Keep that path conservative for now.
                if sheet.cell(anchor).is_some_and(|cell| cell.formula.is_some()) || sheet.spill.contains_key(&anchor) {
                    return Ok(None);
                }
                under.premises.push(premise(anchor_id, ["input", "text"]));
                if matches!(sheet.value(anchor), CellValue::Bool(_)) {
                    entered_edit = true;
                    let target_id = format!("cell:{}!{target}", sheet.sheet_id);
                    under.objects.push(json!(target_id));
                    under.premises.push(premise(target_id, ["input", "text"]));
                    if sheet.cell(target).is_some_and(|cell| cell.formula.is_none() && matches!(cell.value, CellValue::Bool(_))) {
                        let range = RangeRef::cell(target);
                        changed.push(range);
                        self.entry_row_premises(&mut under, si, range);
                        under.premises.push(premise("workbook", ["props.defaultStyle"]));
                        fields.insert("props.protection".into());
                        if sheet.is_protected() {
                            under.premises.push(premise(format!("cell:{}!{target}", sheet.sheet_id), ["style"]));
                        }
                    }
                }
            }
            if fields.len() > LIMIT || under.premises.len() > LIMIT {
                return Ok(None);
            }
        }
        if entered_edit {
            self.hold_shared_edit_reads(&mut under, si, &changed, false)?;
        }
        under.premises.push(premise(sid, fields));
        merged(&mut under);
        Ok(Some(under))
    }
}
