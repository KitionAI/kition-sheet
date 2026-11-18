//! Address coverage follows logical axes, including their unmaterialized empty tail.
use kition_kit::{ContentAsk, ContentPremise, Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, Premise, Under, Within};
use serde_json::{Value, json};

use crate::{SheetDocument, ids};

fn axis(window: &Within) -> bool {
    matches!(window.field.as_str(), "rows" | "cols") && window.parent.as_str().is_some_and(|id| id.starts_with("sheet:"))
}

fn line(id: &Value) -> bool {
    id.as_str().is_some_and(|id| id.starts_with("row:") || id.starts_with("col:"))
}

fn overlay(current: &mut Value, previous: &Value) {
    if let (Some(current), Some(previous)) = (current.as_object_mut(), previous.as_object()) {
        for (key, value) in previous {
            if value.is_null() {
                current.remove(key);
            } else if let Some(current) = current.get_mut(key) {
                overlay(current, value);
            } else {
                current.insert(key.clone(), value.clone());
            }
        }
    } else {
        *current = previous.clone();
    }
}

fn members(order: &Value, window: &Within) -> Result<Vec<Value>, String> {
    let anchors = order.get(&window.field).and_then(Value::as_object).ok_or("structural axis anchors are unavailable")?;
    Ok(anchors
        .iter()
        .filter_map(|(id, anchor)| {
            let index = anchor.get("index").and_then(Value::as_u64)?;
            let covered = if window.from == window.to { index <= window.from as u64 } else { index < window.to as u64 };
            covered.then(|| json!(id))
        })
        .collect())
}

fn at(value: &Value, path: &str) -> Value {
    path.split('.').try_fold(value, |value, key| value.get(key)).cloned().unwrap_or(Value::Null)
}

impl SheetDocument {
    pub(crate) fn axis_order(&self, si: usize) -> Result<Value, String> {
        let state = self.doc_state()?;
        let kdm = state.kdm().ok_or("logical axes need an observed replica")?;
        let axes = kdm.model().ids().sheets.get(si).ok_or("logical sheet is unavailable")?;
        let mut order = serde_json::Map::new();
        for field in ["rows", "cols"] {
            let mut anchors = serde_json::Map::new();
            let mut live = 0usize;
            for id in kdm.replica().list(&axes.object, field).map_err(|error| error.to_string())? {
                let gone = kdm.replica().get(&id, "gone").map_err(|error| error.to_string())? == true;
                let padding = id.as_str().starts_with('p') && id.as_str().len() == 33;
                if gone || !padding {
                    anchors.insert(format!("{}:{id}", if field == "rows" { "row" } else { "col" }), json!({"index":live,"gone":gone}));
                }
                live = live.saturating_add(usize::from(!gone));
            }
            order.insert(field.into(), Value::Object(anchors));
        }
        Ok(Value::Object(order))
    }

    fn axis_members(&self, window: &Within, historical: Option<&Value>) -> Result<Vec<Value>, String> {
        let si = self
            .doc_state()?
            .wb
            .sheets
            .iter()
            .position(|sheet| json!(ids::sheet_tool_id(sheet)) == window.parent)
            .ok_or("address sheet is unavailable")?;
        let mut order = self.axis_order(si)?;
        if let Some(previous) = historical {
            overlay(&mut order, previous);
        }
        members(&order, window)
    }

    pub(crate) fn logical_axis_premises(&self, mut under: Under) -> Result<Under, String> {
        if !under.within.iter().any(axis) {
            return Ok(under);
        }
        let state = self.doc_state()?;
        let kdm = state.kdm().ok_or("logical axes need an observed replica")?;
        let mut owners = std::collections::BTreeMap::new();
        for (si, axes) in kdm.model().ids().sheets.iter().enumerate() {
            let Some(sheet) = state.wb.sheet(si) else { continue };
            for (prefix, ids) in [("row", &axes.rows), ("col", &axes.cols)] {
                for id in ids {
                    owners.insert(format!("{prefix}:{id}"), ids::sheet_tool_id(sheet));
                }
            }
        }
        under.objects.retain(|id| !line(id));
        let mut held = Vec::new();
        for premise in std::mem::take(&mut under.premises) {
            if let Some((id, sheet)) = premise.id.as_str().and_then(|id| owners.get(id).map(|owner| (id, owner))) {
                let fields = premise.fields.unwrap_or_else(|| vec!["*".into()]);
                if !fields.is_empty() {
                    under.content.push(ContentPremise {
                        id: json!("axis:all"),
                        field: "~contents".into(),
                        region: json!({"sheet":sheet,"axis":id,"fields":fields}),
                    });
                }
            } else {
                held.push(premise);
            }
        }
        under.premises = held;
        let mut physical = Vec::new();
        for window in std::mem::take(&mut under.within) {
            if !axis(&window) {
                physical.push(window);
                continue;
            }
            for id in self.axis_members(&window, None)? {
                under.objects.push(id.clone());
                under.premises.push(Premise::on(id).fields(Vec::<String>::new()).positions(["index", "gone"]));
            }
            let covers = under.covers.get_or_insert_with(Covers::default);
            if !covers.fields.iter().any(|field| field == "~axisOrder") {
                covers.fields.push("~axisOrder".into());
            }
        }
        under.within = physical;
        crate::place::merged(&mut under);
        Ok(under)
    }

    pub(crate) fn logical_axis_covered(&self, command: &str, params: &Value, objects: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
        let raw = self.shared_edit_premises(command, params, self.call_premises(command, params)?)?;
        let windows: Vec<_> = raw.as_ref().into_iter().flat_map(|under| &under.within).filter(|window| axis(window)).collect();
        if windows.is_empty() {
            return self.premise_covered(command, params, objects);
        }
        let logical = self.logical_axis_premises(raw.clone().unwrap_or_default())?;
        let mut answer = self.premise_covered(command, params, objects)?.unwrap_or_else(|| CoveredAt { under: logical.objects, covered: Vec::new() });
        answer.under.retain(|id| !line(id));
        let mut unknown = false;
        for window in windows {
            let previous = objects.iter().find(|ask| ask.id == window.parent);
            unknown |= previous.is_some_and(|ask| ask.deleted || ask.unknown.iter().any(|field| field == "~axisOrder" || field == "$was"));
            answer.under.extend(self.axis_members(window, previous.and_then(|ask| ask.before.get("~axisOrder")))?);
        }
        answer.under.sort_by_key(Value::to_string);
        answer.under.dedup();
        for ask in objects {
            if let Some(covered) = answer.covered.iter_mut().find(|covered| covered.id == ask.id) {
                if unknown {
                    covered.covered = None;
                }
            } else {
                answer.covered.push(Covered {
                    id: ask.id.clone(),
                    covered: (!unknown).then(|| answer.under.contains(&ask.id)),
                    ..Default::default()
                });
            }
        }
        Ok(Some(answer))
    }

    pub(crate) fn axis_content_at(&self, ask: &ContentAsk) -> Result<Option<bool>, String> {
        if ask.premise.field != "~contents" {
            return Ok(None);
        }
        let axis = ask.premise.region.get("axis").and_then(Value::as_str).ok_or("axis content needs an identity")?;
        let fields = ask.premise.region.get("fields").and_then(Value::as_array).ok_or("axis content needs fields")?;
        let values = self.premise_derived(std::slice::from_ref(&ask.premise.id), &[])?;
        let now = values
            .iter()
            .find(|value| value.id == ask.premise.id)
            .and_then(|value| value.values.get("contents"))
            .ok_or("axis contents are unavailable")?;
        // ContentAsk carries the complete historical value, including absent axes.
        let before = &ask.before;
        let empty = self.empty_axis_contents(&ask.premise.region["sheet"], axis.starts_with("row:"))?;
        let before = before.get(axis).unwrap_or(&empty);
        let now = now.get(axis).unwrap_or(&empty);
        for field in fields.iter().filter_map(Value::as_str) {
            if field == "*" {
                if before != now {
                    return Ok(Some(true));
                }
            } else if at(before, field) != at(now, field) {
                return Ok(Some(true));
            }
        }
        Ok(Some(false))
    }

    fn empty_axis_contents(&self, sid: &Value, rows: bool) -> Result<Value, String> {
        let state = self.doc_state()?;
        let sheet = state.wb.sheets.iter().find(|sheet| json!(ids::sheet_tool_id(sheet)) == *sid).ok_or("axis sheet is unavailable")?;
        let style = state.wb.styles.get(Default::default());
        let layout = if rows { json!([[], [], null, sheet.default_row_height, style]) } else { json!([[], null, style]) };
        let hash = |value: &Value| serde_json::to_vec(value).map(|bytes| kition_kit::session::sha256_hex(&bytes)).map_err(|error| error.to_string());
        Ok(json!({"info":null,"~style":null,"~hidden":false,"~outline":0,"~fit":hash(&json!([]))?,"~layout":hash(&layout)?}))
    }

    pub(crate) fn axis_values(&self, wanted: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        let order = names.is_empty() || names.iter().any(|name| name == "axisOrder");
        let contents = names.is_empty() || names.iter().any(|name| name == "contents");
        if !order && !contents {
            return Ok(());
        }
        let state = self.doc_state()?;
        let Some(kdm) = state.kdm() else { return Ok(()) };
        let mut layout = Vec::new();
        if contents {
            self.row_layout_values(&[], &[], &mut layout)?;
            self.edit_read_values(&[], &[], &mut layout)?;
            self.object_read_values(&[], &[], &mut layout)?;
            self.view_read_values(&[], &[], &mut layout)?;
            self.table_formula_values(&[], &[], &mut layout)?;
        }
        let mut by_axis = std::collections::BTreeMap::<String, serde_json::Map<String, Value>>::new();
        for value in found.iter().chain(&layout) {
            if let Some(id) = value.id.as_str().filter(|id| id.starts_with("row:") || id.starts_with("col:")) {
                by_axis.entry(id.into()).or_default().extend(value.values.iter().map(|(key, value)| (format!("~{key}"), value.clone())));
            }
        }
        let mut all = serde_json::Map::new();
        let facts = json!("axis:all");
        let contents = contents && (wanted.is_empty() || wanted.contains(&facts));
        for (si, sheet) in self.doc_state()?.wb.sheets.iter().enumerate() {
            let id = json!(ids::sheet_tool_id(sheet));
            if kdm.model().ids().sheets.get(si).is_none() {
                continue;
            }
            if order && (wanted.is_empty() || wanted.contains(&id)) {
                let mut values = serde_json::Map::new();
                values.insert("axisOrder".into(), self.axis_order(si)?);
                found.push(DerivedValues { id, values });
            }
            if contents && let Some(axes) = kdm.model().ids().sheets.get(si) {
                for (prefix, ids) in [("row", &axes.rows), ("col", &axes.cols)] {
                    for raw in ids {
                        let axis = format!("{prefix}:{raw}");
                        let mut fields =
                            serde_json::Map::from_iter([("info".into(), kdm.replica().get(raw, "info").map_err(|error| error.to_string())?)]);
                        if let Some(values) = by_axis.get(&axis) {
                            fields.extend(values.clone());
                        }
                        all.insert(axis, Value::Object(fields));
                    }
                }
            }
        }
        if contents {
            found.push(DerivedValues { id: facts, values: serde_json::Map::from_iter([("contents".into(), Value::Object(all))]) });
        }
        Ok(())
    }
}
