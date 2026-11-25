//! Table controls read their definition and bounded cell/style footprints.
use crate::{
    SheetDocument, ids,
    place::{area, merged, premise, window},
};
use kition_kit::{DerivedValues, Under};
use kition_sheet_engine::core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use serde_json::{Value, json};

impl SheetDocument {
    pub(crate) fn table_dependency_values(&self, names: &[String], values: &mut serde_json::Map<String, Value>) -> Result<(), String> {
        let state = self.doc_state()?;
        let wb = &state.wb;
        if names.is_empty() || names.iter().any(|name| name == "tableNames") {
            let tables: Vec<_> =
                wb.sheets.iter().flat_map(|sheet| sheet.tables.iter().map(|table| (sheet.sheet_id, table.id, &table.name))).collect();
            values.insert(
                "tableNames".into(),
                json!(kition_kit::session::sha256_hex(&serde_json::to_vec(&tables).map_err(|error| error.to_string())?)),
            );
        }
        Ok(())
    }

    pub(crate) fn table_formula_values(&self, requested: &[Value], names: &[String], found: &mut Vec<DerivedValues>) -> Result<(), String> {
        if requested.len() > 100_000 {
            return Ok(());
        }
        let requested: std::collections::BTreeSet<_> = requested.iter().filter_map(Value::as_str).collect();
        let complete = names.is_empty() || names.iter().any(|name| name == "formulaInputs");
        let columns = names.is_empty() || names.iter().any(|name| name == "headerReferences");
        if complete || columns {
            let state = self.doc_state()?;
            let wb = &state.wb;
            let mut references = std::collections::BTreeMap::<String, Vec<_>>::new();
            let mut column_references = std::collections::BTreeMap::<(u32, u32, usize), Vec<_>>::new();
            let mut count = 0usize;
            for sheet in &wb.sheets {
                for (at, cell) in sheet.cells.iter() {
                    let Some(formula) = &cell.formula else { continue };
                    count += 1;
                    if count > 100_000 {
                        return Ok(());
                    }
                    let Some(expr) = formula.expr() else { continue };
                    let digest = kition_kit::session::sha256_hex(formula.text.as_bytes());
                    for name in kition_sheet_engine::cmd::insert::table_references(&expr) {
                        references.entry(name).or_default().push((sheet.sheet_id, at, digest.clone()));
                    }
                    if columns {
                        let mut used = std::collections::BTreeSet::new();
                        expr.walk(&mut |node| {
                            let kition_sheet_engine::formula::Expr::Struct(reference) = node else { return };
                            let table = if reference.table.is_empty() {
                                sheet.table_at(at).map(|table| (sheet.as_ref(), table))
                            } else {
                                wb.table(&reference.table).and_then(|(si, ti)| Some((wb.sheet(si)?, wb.sheet(si)?.tables.get(ti)?)))
                            };
                            let Some((owner, table)) = table else { return };
                            for name in [&reference.col_start, &reference.col_end].into_iter().flatten() {
                                if let Some(column) = table.columns.iter().position(|column| column.name.eq_ignore_ascii_case(name)) {
                                    used.insert((owner.sheet_id, table.id, column));
                                }
                            }
                        });
                        for column in used {
                            column_references.entry(column).or_default().push((sheet.sheet_id, at, digest.clone()));
                        }
                    }
                }
            }
            let mut emitted_headers = 0usize;
            let mut axis_references = std::collections::BTreeMap::<String, Vec<Value>>::new();
            for (si, sheet) in wb.sheets.iter().enumerate() {
                for table in &sheet.tables {
                    let id = json!(format!("table:{}:{}", sheet.sheet_id, table.id));
                    let mut values = serde_json::Map::new();
                    if complete && (requested.is_empty() || id.as_str().is_some_and(|id| requested.contains(id))) {
                        let sources = references.get(&table.name.to_ascii_lowercase());
                        let digest = kition_kit::session::sha256_hex(&serde_json::to_vec(&sources).map_err(|error| error.to_string())?);
                        values.insert("formulaInputs".into(), json!(digest));
                    }
                    if columns && table.header_row {
                        for column in 0..table.columns.len() {
                            let Some(at) = table.range.start.offset(0, column as i64).filter(|at| table.range.contains(*at)) else { continue };
                            let Some(raw) =
                                state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)).and_then(|axes| axes.cols.get(at.col as usize))
                            else {
                                return Ok(());
                            };
                            let axis = format!("col:{raw}");
                            if !requested.is_empty() && !requested.contains(axis.as_str()) {
                                continue;
                            }
                            if emitted_headers >= 100_000 {
                                return Ok(());
                            }
                            emitted_headers += 1;
                            let inputs = column_references.get(&(sheet.sheet_id, table.id, column));
                            axis_references.entry(axis).or_default().push(json!([table.id, column, inputs]));
                        }
                    }
                    if values.is_empty() {
                        continue;
                    }
                    if let Some(existing) = found.iter_mut().find(|existing| existing.id == id) {
                        existing.values.extend(values);
                    } else {
                        found.push(DerivedValues { id, values });
                    }
                }
            }
            // Column identities survive row movement. Positional Cell tool IDs
            // cannot carry derived guards without inventing receipt changes when
            // a cell's address changes in the pinned SDK.
            let slots: std::collections::BTreeMap<_, _> =
                found.iter().enumerate().filter_map(|(index, value)| value.id.as_str().map(|id| (id.to_owned(), index))).collect();
            for (axis, sources) in axis_references {
                let digest = kition_kit::session::sha256_hex(&serde_json::to_vec(&sources).map_err(|error| error.to_string())?);
                if let Some(existing) = slots.get(&axis).and_then(|index| found.get_mut(*index)) {
                    existing.values.insert("headerReferences".into(), json!(digest));
                } else {
                    found.push(DerivedValues { id: json!(axis), values: [("headerReferences".into(), json!(digest))].into_iter().collect() });
                }
            }
        }
        Ok(())
    }

    fn table_insert_premises(&self, params: &Value, si: usize) -> Result<Option<Under>, String> {
        if params.get("header").and_then(Value::as_bool) != Some(true) || params.get("mode").and_then(Value::as_str) == Some("format") {
            return Ok(None);
        }
        let Some(range) = params.get("range").and_then(Value::as_str).and_then(area).filter(|range| range.count() <= 100_000) else {
            return Ok(None);
        };
        let state = self.doc_state()?;
        let Some(sheet) = state.wb.sheet(si) else { return Ok(None) };
        let sid = ids::sheet_tool_id(sheet);
        let header = RangeRef::new(range.start, CellRef::new(range.start.row, range.end.col));
        let mut under =
            self.call_premises("range.fill", &json!({"sheet":sid,"range":header.a1(),"input":format!("=SUM({})", header.a1())}))?.unwrap_or_default();
        let mut fields = kdm_model::constraint_fields(range);
        fields.extend(["~arrays", "props.protection"].map(str::to_owned));
        if params.get("range").and_then(Value::as_str).is_some_and(|range| range.contains('!')) || params.get("sheet").is_some_and(Value::is_string) {
            fields.push("name".into());
        }
        under.premises.push(premise(sid, fields));
        window(&mut under, &ids::sheet_tool_id(sheet), "rows", range.start.row as usize, range.end.row as usize + 1);
        window(&mut under, &ids::sheet_tool_id(sheet), "cols", range.start.col as usize, range.end.col as usize + 1);
        if !self.table_lookup_premises(&mut under, params, si, range)? {
            under.premises.push(premise("workbook", ["~tableNames", "names"]));
        }
        if params.get("style").and_then(Value::as_str).is_none_or(|style| style.trim().is_empty()) {
            under.premises.push(premise("workbook", ["props.tableStyles.default_table"]));
        }
        merged(&mut under);
        Ok(Some(under))
    }

    pub(crate) fn table_control_premises(&self, command: &str, params: &Value, context: usize) -> Result<Option<Under>, String> {
        if command == "table.insert" {
            return self.table_insert_premises(params, context);
        }
        if !matches!(
            command,
            "table.headerRow" | "table.totalRow" | "table.totalFunction" | "table.convertToRange" | "table.rename" | "table.removeDuplicates"
        ) {
            return Ok(None);
        }
        let state = self.doc_state()?;
        let wb = &state.wb;
        let target = params.get("table").and_then(Value::as_str).and_then(|name| wb.table(name)).or_else(|| {
            params
                .get("table")
                .is_none()
                .then(|| {
                    wb.sheet(context)?.tables.iter().position(|table| table.range.contains(state.selection.active)).map(|index| (context, index))
                })
                .flatten()
        });
        let Some((si, ti)) = target else { return Ok(None) };
        let Some(sheet) = wb.sheet(si) else { return Ok(None) };
        let Some(table) = sheet.tables.get(ti) else { return Ok(None) };
        let sid = ids::sheet_tool_id(sheet);
        let mut under = Under::default();
        under.objects.push(json!(sid));
        let table_field = format!("tables[name={}]", json!(table.name));
        let header_off = command == "table.headerRow" && !params.get("on").and_then(Value::as_bool).unwrap_or(!table.header_row);
        let mut table_fields = vec!["props.protection".into()];
        if header_off {
            // Removing a header preserves the current column names. A colleague's
            // header-cell edit can rename a column without changing this operation.
            let value = serde_json::to_value(table).map_err(|error| error.to_string())?;
            table_fields.extend(value.as_object().into_iter().flat_map(|table| table.keys()).filter(|key| key.as_str() != "columns").map(|key| {
                match key.as_str() {
                    // The `Table` object's own fields; the rest of the table is its `props`.
                    "id" => format!("{table_field}.key"),
                    "name" | "range" => format!("{table_field}.{key}"),
                    _ => format!("{table_field}.props.{key}"),
                }
            }));
        } else {
            table_fields.push(table_field);
        }
        under.premises.push(premise(sid.clone(), table_fields));
        if command == "table.rename" {
            if wb.sheets.iter().flat_map(|sheet| sheet.cells.iter()).filter(|(_, cell)| cell.formula.is_some()).take(100_001).count() > 100_000 {
                return Ok(None);
            }
            let tid = format!("table:{}:{}", sheet.sheet_id, table.id);
            let present = state.kdm().is_some_and(|kdm| {
                kdm.replica().objects_of_type("Table").iter().any(|object| {
                    kdm.replica().get(object, "key").ok().and_then(|value| value.as_str().map(str::to_owned))
                        == Some(format!("{}:{}", sheet.sheet_id, table.id))
                })
            });
            if !present {
                return Ok(None);
            }
            under.premises.push(premise("workbook", ["~tableNames"]));
            under.premises.push(premise(tid, ["~formulaInputs"]).positions(Vec::<String>::new()));
            return Ok(Some(under));
        }
        if command == "table.removeDuplicates" {
            if si != context {
                return Ok(None);
            }
            let Some(data) = table.data_range() else { return Ok(Some(under)) };
            let range = RangeRef::new(table.header_range().map_or(data.start, |header| header.start), data.end);
            if range.count() > 10_000 {
                return Ok(None);
            }
            let mut sources = vec![format!("{}[#All]", table.name)];
            if let Some(columns) = params.get("columns").and_then(Value::as_array) {
                if columns.len() > 100_000 {
                    return Ok(None);
                }
                let mut external = std::collections::BTreeSet::new();
                for column in columns {
                    let col = match column {
                        Value::String(text) => kition_sheet_engine::core::letters_to_col(text.trim_start_matches('$')),
                        Value::Number(number) => {
                            let Some(col) = number
                                .as_u64()
                                .and_then(|offset| u32::try_from(offset).ok())
                                .and_then(|offset| range.start.col.checked_add(offset))
                                .filter(|col| *col < MAX_COLS)
                            else {
                                return Ok(None);
                            };
                            Some(col)
                        }
                        _ => None,
                    };
                    if let Some(col) = col.filter(|col| *col < range.start.col || *col > range.end.col) {
                        external.insert(col);
                    }
                }
                if (external.len() as u64).checked_mul(u64::from(data.height())).is_none_or(|count| count > 100_000) {
                    return Ok(None);
                }
                sources.extend(external.into_iter().map(|col| {
                    let col = kition_sheet_engine::core::col_to_letters(col);
                    format!("${col}${}:${col}${}", data.start.row.saturating_add(1), data.end.row.saturating_add(1))
                }));
            }
            if let Some(cells) =
                self.call_premises("range.fill", &json!({"sheet":sid,"range":range.a1(),"input":format!("=SUM({})", sources.join(","))}))?
            {
                under.objects.extend(cells.objects);
                under.premises.extend(cells.premises);
                under.within.extend(cells.within);
            }
            self.entry_row_premises(&mut under, si, range);
            merged(&mut under);
            return Ok(Some(under));
        }
        let mut source = false;
        let row = match command {
            "table.headerRow" => {
                let on = params.get("on").and_then(Value::as_bool).unwrap_or(!table.header_row);
                if on == table.header_row {
                    return Ok(Some(under));
                }
                if on { table.range.start.row.checked_sub(1) } else { Some(table.range.start.row) }
            }
            "table.totalRow" => {
                let on = params.get("on").and_then(Value::as_bool).unwrap_or(!table.totals_row);
                if on == table.totals_row {
                    return Ok(Some(under));
                }
                source = on;
                if on { table.range.end.row.checked_add(1).filter(|row| *row < MAX_ROWS) } else { Some(table.range.end.row) }
            }
            "table.totalFunction" => {
                if !table.totals_row {
                    return Ok(Some(under));
                }
                source = true;
                Some(table.range.end.row)
            }
            _ => Some(table.range.start.row),
        };
        let Some(row) = row else { return Ok(None) };
        let range = if command == "table.convertToRange" {
            table.range
        } else {
            RangeRef::new(CellRef::new(row, table.range.start.col), CellRef::new(row, table.range.end.col))
        };
        if range.count() > 100_000 || source && table.range.count() > 100_000 {
            return Ok(None);
        }
        let input = if source { format!("=SUBTOTAL(109,{}[#Data])", table.name) } else { String::new() };
        if let Some(mut cells) = self.call_premises("range.fill", &json!({"sheet":sid,"range":range.a1(),"input":input}))? {
            if command != "table.convertToRange" {
                // These handlers retain effective styles and do not fit the row.
                // same-field decision: hold the content they replace.
                for dependency in &mut cells.premises {
                    let Some(id) = dependency.id.as_str() else { continue };
                    if id == sid {
                        if let Some(fields) = &mut dependency.fields {
                            fields.retain(|field| field == "name" || field == "props.protection" || field.starts_with("tables["));
                        }
                    } else if id.starts_with("row:") {
                        if let Some(fields) = &mut dependency.fields {
                            fields.retain(|field| matches!(field.as_str(), "info.style" | "info.hidden"));
                        }
                    } else if let Some(at) = id.strip_prefix(&format!("cell:{}!", sheet.sheet_id)).and_then(CellRef::parse)
                        && range.contains(at)
                        && let Some(fields) = &mut dependency.fields
                    {
                        fields.retain(|field| matches!(field.as_str(), "style" | "input" | "text" | "array" | "picture"));
                    }
                }
            }
            under.objects.extend(cells.objects);
            under.premises.extend(cells.premises);
            under.within.extend(cells.within);
        }
        if command == "table.convertToRange" {
            under.premises.push(premise("workbook", ["props.tableStyles", "theme"]));
            if let Some(axes) = state.kdm().and_then(|kdm| kdm.model().ids().sheets.get(si)) {
                for (kind, first, last, lines) in
                    [("row", range.start.row, range.end.row, &axes.rows), ("col", range.start.col, range.end.col, &axes.cols)]
                {
                    for raw in lines.iter().skip(first as usize).take((last - first) as usize + 1) {
                        under.premises.push(premise(format!("{kind}:{raw}"), ["info.hidden"]).positions(["index"]));
                    }
                }
            }
        }
        merged(&mut under);
        Ok(Some(under))
    }
}
