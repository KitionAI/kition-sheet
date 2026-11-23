//! Comparison algebra. Expected footprints never receive a declaration or Under.
use kition_kit::kdm::{ActorId, Replica, tool_ids::ToolIds};
use kition_kit::session::{KitModel, MemoryBlobs};
use kition_kit::{KitDocument, Under};
use kition_sheet_engine::core::{CellRef, RangeRef};
use kition_sheet_engine::model::Workbook;
use kition_sheet_kit::SheetDocument;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) struct Fact {
    pub object: String,
    pub kind: String,
    pub field: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Facts(pub BTreeSet<Fact>);

fn path_covers(parent: &str, child: &str) -> bool {
    parent == "*"
        || parent == child
        || parent == "~layout" && matches!(child, "~fit" | "info" | "info.style" | "info.size" | "info.custom")
        || parent == "~style" && child == "info.style"
        // insert::set_anchor_mode assigns only this scalar; the private fact reads
        // the same mode (pin_narrow pairs unrelated geometry with mode conflicts).
        || matches!((parent, child), ("~anchorMode", "frame.anchor.mode") | ("frame.anchor.mode", "~anchorMode"))
        || child.strip_prefix(parent).is_some_and(|tail| tail.starts_with('.') || tail.starts_with('['))
}

impl Facts {
    pub fn fact(&mut self, object: &str, kind: &str, field: &str) {
        self.0.insert(Fact { object: object.into(), kind: kind.into(), field: field.into() });
    }

    pub fn add(&mut self, object: &str, fields: impl IntoIterator<Item = impl AsRef<str>>) {
        self.fact(object, "identity", "exists");
        for field in fields {
            self.fact(object, "field", field.as_ref());
        }
    }

    pub fn position(&mut self, object: &str, fields: &[&str]) {
        self.fact(object, "identity", "exists");
        for field in fields {
            self.fact(object, "position", field);
        }
    }

    pub fn window(&mut self, object: &str, field: &str, from: usize, to: usize) {
        assert!(from <= to);
        if from == to {
            self.fact(object, "gap", &format!("{field}:{from}"));
        } else {
            // Intervals remain bounded symbolic intervals, including whole axes.
            self.fact(object, "window", &format!("{field}:{from}:{to}"));
        }
    }

    pub fn from_under(under: &Under) -> Self {
        let mut out = Self::default();
        for premise in &under.premises {
            let object = premise.id.as_str().expect("object tool id");
            out.add(object, premise.fields.clone().unwrap_or_else(|| vec!["*".into()]));
            out.position(
                object,
                &premise.positions.as_ref().map(|fields| fields.iter().map(String::as_str).collect::<Vec<_>>()).unwrap_or_else(|| vec!["*"]),
            );
            for (field, identity) in &premise.identities {
                out.fact(object, "identities", &format!("{field}={identity}"));
            }
        }
        for object in &under.objects {
            let object = object.as_str().expect("object tool id");
            if !under.premises.iter().any(|premise| premise.id == object) {
                out.add(object, ["*"]);
                out.position(object, &["*"]);
            }
        }
        for window in &under.within {
            assert!(window.time.is_none(), "sheet addresses are ordinal coordinates");
            out.window(window.parent.as_str().expect("parent tool id"), &window.field, window.from, window.to);
        }
        for field in under.covers.as_ref().into_iter().flat_map(|covers| &covers.fields) {
            out.fact("call", "covers", field);
        }
        for order in &under.orders {
            out.fact("call", "order", &serde_json::to_string(order).unwrap());
        }
        for input in &under.inputs {
            out.fact("call", "external", input);
        }
        assert!(under.content.is_empty(), "raw under must expose its axis fields before under_step normalization");
        out
    }

    pub fn validate(&self, cx: &Context) {
        let schema: Value = serde_json::from_str(include_str!("../../../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
        let names = ToolIds::of_replica(&cx.replica);
        for (ty, definition) in schema["types"].as_object().unwrap() {
            for object in cx.replica.objects_of_type(ty) {
                let id = names.show(&object);
                for fact in self.0.iter().filter(|fact| fact.object == id && fact.kind == "field" && fact.field.starts_with('~')) {
                    let root = fact.field.trim_start_matches('~').split(['.', '[']).next().unwrap();
                    assert!(definition["derived"].get(root).is_some(), "undeclared derived fact cannot hold anything: {fact:?}");
                }
            }
        }
    }

    pub fn contains(&self, fact: &Fact, cx: &Context) -> bool {
        if fact.kind == "position"
            && fact.field == "z"
            && let Some((sheet, field)) = cx.ownership.get(&fact.object)
            && field == "charts"
            && self.0.iter().any(|held| held.object == *sheet && held.kind == "field" && held.field == "~chartStack")
        {
            return true;
        }
        if (fact.kind == "identity" || fact.kind == "position" && fact.field == "index")
            && let Some((si, rows, index)) = cx.axis_index(&fact.object)
            && self.contains(
                &Fact { object: cx.sid(si), kind: "window".into(), field: format!("{}:{index}:{}", if rows { "rows" } else { "cols" }, index + 1) },
                cx,
            )
        {
            return true;
        }
        if self.0.iter().any(|held| held.object == fact.object && held.kind == fact.kind && path_covers(&held.field, &fact.field)) {
            return true;
        }
        if matches!(fact.kind.as_str(), "window" | "gap") {
            let parts: Vec<_> = fact.field.split(':').collect();
            let field = parts[0];
            if self.0.iter().any(|held| held.object == fact.object && held.kind == "field" && path_covers(&held.field, field)) {
                return true;
            }
            let from: usize = parts[1].parse().unwrap();
            let to = if fact.kind == "gap" { from } else { parts[2].parse().unwrap() };
            let mut spans: Vec<_> = self
                .0
                .iter()
                .filter_map(|held| {
                    if held.object != fact.object || held.kind != "window" {
                        return None;
                    }
                    let parts: Vec<_> = held.field.split(':').collect();
                    (parts[0] == field).then(|| (parts[1].parse::<usize>().unwrap(), parts[2].parse::<usize>().unwrap()))
                })
                .collect();
            spans.sort_unstable();
            let mut covered = from;
            for (start, end) in spans {
                if start > covered {
                    break;
                }
                covered = covered.max(end);
                if covered >= to && (to > from || end > from) {
                    return true;
                }
            }
            if to > from {
                let si = (0..cx.wb.sheets.len()).find(|si| cx.sid(*si) == fact.object);
                if let Some(si) = si {
                    let axes = if field == "rows" { &cx.ids.sheets[si].rows } else { &cx.ids.sheets[si].cols };
                    if to <= axes.len()
                        && (from..to).all(|index| {
                            let object = cx.axis(si, field == "rows", index as u32);
                            self.0.iter().any(|held| held.object == object && held.kind == "position" && path_covers(&held.field, "index"))
                        })
                    {
                        return true;
                    }
                }
            }
        }
        // An owning-field guard includes its children's content and identity,
        // including a virtual empty cell. It does not include a child's position.
        if fact.kind != "position" {
            if fact.object == "workbook" && fact.field.starts_with("styles.") {
                for held in self.0.iter().filter(|held| held.kind == "field" && held.field == "~layout") {
                    if let Some((si, rows, index)) = cx.axis_index(&held.object)
                        && cx.wb.sheets[si].cells.iter().any(|(at, _)| {
                            let style = cx.wb.sheets[si].style_id(at);
                            (if rows { at.row } else { at.col }) as usize == index
                                && cx.ids.styles.get(style.0 as usize).is_some_and(|key| path_covers(&format!("styles.{key}.style"), &fact.field))
                        })
                    {
                        return true;
                    }
                }
            }
            if fact.kind == "field"
                && fact.field == "props.defaultRowHeight"
                && self.0.iter().any(|held| {
                    held.kind == "field"
                        && matches!(held.field.as_str(), "~layout" | "*")
                        && cx.ownership.get(&held.object).is_some_and(|(owner, field)| *owner == fact.object && field == "rows")
                })
            {
                return true;
            }
            if let Some((parent, field)) = cx.ownership.get(&fact.object)
                && self.0.iter().any(|held| held.object == *parent && held.kind == "field" && path_covers(&held.field, field))
            {
                return true;
            }
            if let Some((sid, _)) = fact.object.strip_prefix("cell:").and_then(|tail| tail.split_once('!')) {
                let owner = format!("sheet:{sid}");
                if self.0.iter().any(|held| held.object == owner && held.kind == "field" && path_covers(&held.field, "cells")) {
                    return true;
                }
            }
        }
        false
    }

    /// A sheet's tables, pivot tables and comments are `Table`, `Pivot` and `Comment` objects
    /// with tool ids of their own: what a fact holds of a sheet's `tables` or `pivots` (the list, or an item by name and
    /// a path inside it) it holds of those objects too.
    fn feature_objects(&self, cx: &Context) -> Facts {
        const TABLE: [&str; 5] = ["key", "name", "range", "props", "columns"];
        const PIVOT: [&str; 6] = ["key", "name", "anchor", "source", "lastRange", "props"];
        let mut more: Vec<(String, Vec<String>)> = Vec::new();
        for fact in self.0.iter().filter(|fact| fact.kind == "field") {
            let Some((si, sheet)) = cx.wb.sheets.iter().enumerate().find(|(_, s)| format!("sheet:{}", s.sheet_id) == fact.object) else { continue };
            // A cell's comment is a `Comment` with a tool id of its own (`comment:<sheetId>!B7`).
            if let Some((row, col)) = fact.field.strip_prefix("comments.row:").and_then(|key| key.split_once(":col:"))
                && let Some(axes) = cx.ids.sheets.get(si)
                && let (Some(r), Some(c)) = (axes.rows.iter().position(|o| o.as_str() == row), axes.cols.iter().position(|o| o.as_str() == col))
            {
                let at = CellRef::new(r as u32, c as u32);
                if sheet.comments.contains_key(&at) {
                    more.push((
                        format!("comment:{}!{at}", sheet.sheet_id),
                        ["author", "text", "replies", "threaded", "resolved", "visible"].map(str::to_owned).to_vec(),
                    ));
                }
            }
            for (list, prefix, all) in [("tables", "table", &TABLE[..]), ("pivots", "pivot", &PIVOT[..])] {
                let named: Vec<(String, u32)> = if list == "tables" {
                    sheet.tables.iter().map(|t| (t.name.clone(), t.id)).collect()
                } else {
                    sheet.pivots.iter().map(|p| (p.name.clone(), p.id)).collect()
                };
                let tool = |id: u32| format!("{prefix}:{}:{id}", sheet.sheet_id);
                let Some(rest) = fact.field.strip_prefix(list) else { continue };
                if rest.is_empty() || rest.starts_with('.') {
                    for (_, id) in &named {
                        more.push((tool(*id), all.iter().map(|f| (*f).to_owned()).collect()));
                    }
                } else if let Some(selector) = rest.strip_prefix("[name=") {
                    let mut values = serde_json::Deserializer::from_str(selector).into_iter::<Value>();
                    let Some(Ok(Value::String(name))) = values.next() else { continue };
                    let tail = &selector[values.byte_offset()..];
                    let inside = tail.strip_prefix(']').unwrap_or("").strip_prefix('.').unwrap_or("");
                    for (_, id) in named.iter().filter(|(n, _)| *n == name) {
                        let fields = if inside.is_empty() { all.iter().map(|f| (*f).to_owned()).collect() } else { vec![inside.to_owned()] };
                        more.push((tool(*id), fields));
                    }
                }
            }
        }
        let mut out = Facts::default();
        for (object, fields) in more {
            out.add(&object, fields);
        }
        out
    }

    /// These facts without the ones the kit's declaration derives from its own premises on a
    /// sheet's tables, pivot tables and comments ([`Facts::feature_objects`]): the comparison is
    /// of what the sheet's premises hold, as before those were objects of their own.
    pub fn less_feature_objects(mut self, cx: &Context) -> Self {
        let derived = self.feature_objects(cx);
        self.0.retain(|fact| !derived.0.contains(fact) || fact.object.starts_with("sheet:"));
        self
    }

    pub fn difference(&self, other: &Self, cx: &Context) -> Vec<Fact> {
        self.0.iter().filter(|fact| !other.contains(fact, cx)).cloned().collect()
    }
}

pub(super) struct Context {
    pub wb: Workbook,
    pub ids: kdm_model::Ids,
    pub replica: Replica,
    pub ownership: BTreeMap<String, (String, String)>,
    pub active: usize,
    pub selected: RangeRef,
    pub active_cell: CellRef,
    pub editor: kition_sheet_engine::EditorState,
}

impl Context {
    pub fn new(doc: &mut SheetDocument) -> Self {
        let replica = Replica::load(kdm_model::SheetModel::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap();
        let mut blobs = MemoryBlobs::default();
        let bytes = doc.blobs().unwrap();
        let mut offset = 0;
        while offset < bytes.len() {
            let len = u64::from_le_bytes(bytes[offset + 64..offset + 72].try_into().unwrap()) as usize;
            blobs.0.insert(String::from_utf8(bytes[offset..offset + 64].to_vec()).unwrap(), bytes[offset + 72..offset + 72 + len].into());
            offset += 72 + len;
        }
        let (wb, ids) = kdm_model::read_document(&replica, &blobs).unwrap();
        let names = ToolIds::of_replica(&replica);
        let mut ownership = BTreeMap::new();
        for (si, axes) in ids.sheets.iter().enumerate() {
            let sid = format!("sheet:{}", wb.sheets[si].sheet_id);
            for (field, objects) in
                [("rows", &axes.rows), ("cols", &axes.cols), ("images", &axes.images), ("shapes", &axes.shapes), ("charts", &axes.charts)]
            {
                for id in objects {
                    ownership.insert(names.show(id), (sid.clone(), field.into()));
                }
            }
        }
        let state: kition_sheet_engine::EditorState = serde_json::from_value(doc.editor_state()["engine"].clone()).unwrap();
        let active = wb.sheets.iter().position(|sheet| sheet.sheet_id == state.sheet).unwrap();
        let selected = state.selection.current();
        Self { wb, ids, replica, ownership, active, selected, active_cell: state.selection.active, editor: state }
    }

    pub fn route(&mut self, command: &str, params: &Value) {
        // KitDocument::execute switches an agent's named target before the
        // engine call, restoring the editor afterwards (lib.rs:2376).
        if command.starts_with("view.")
            || command.starts_with("selection.")
            || matches!(
                command,
                "chart.move"
                    | "chart.selectData"
                    | "sheet.activate"
                    | "sheet.next"
                    | "sheet.previous"
                    | "edit.selectAll"
                    | "edit.goTo"
                    | "edit.goToSpecial"
                    | "edit.find"
                    | "window.activate"
            )
        {
            return;
        }
        let target = self.sheet(params);
        if target != self.active {
            let selection = self
                .editor
                .sheet_selections
                .iter()
                .find(|(id, _)| *id == self.wb.sheets[target].sheet_id)
                .map(|(_, selection)| selection.clone())
                .unwrap_or_default();
            self.active = target;
            self.selected = selection.current();
            self.active_cell = selection.active;
        }
    }

    pub fn sid(&self, si: usize) -> String {
        format!("sheet:{}", self.wb.sheets[si].sheet_id)
    }
    pub fn cell(&self, si: usize, at: CellRef) -> String {
        format!("cell:{}!{at}", self.wb.sheets[si].sheet_id)
    }
    pub fn axis(&self, si: usize, rows: bool, at: u32) -> String {
        let axes = &self.ids.sheets[si];
        format!("{}:{}", if rows { "row" } else { "col" }, if rows { &axes.rows } else { &axes.cols }[at as usize])
    }
    pub fn axis_index(&self, object: &str) -> Option<(usize, bool, usize)> {
        for (si, axes) in self.ids.sheets.iter().enumerate() {
            for (rows, list) in [(true, &axes.rows), (false, &axes.cols)] {
                let prefix = if rows { "row:" } else { "col:" };
                if let Some(index) = object.strip_prefix(prefix).and_then(|id| list.iter().position(|candidate| candidate.as_str() == id)) {
                    return Some((si, rows, index));
                }
            }
        }
        None
    }

    pub fn geometry(&self, out: &mut Facts, si: usize, row: u32, col: u32) {
        out.add(&self.sid(si), ["props.defaultRowHeight", "props.defaultColWidth"]);
        self.address(out, si, RangeRef::new(CellRef::default(), CellRef::new(row, col)));
        for (rows, end) in [(true, row), (false, col)] {
            for i in 0..=end {
                out.add(&self.axis(si, rows, i), ["info.size", "info.hidden"]);
            }
        }
    }
    pub fn sheet(&self, params: &Value) -> usize {
        if let Some(index) = params.get("sheet").and_then(Value::as_u64) {
            return index as usize;
        }
        let named = params.get("sheet").and_then(Value::as_str).or_else(|| {
            ["range", "cell", "rows", "cols"]
                .into_iter()
                .find_map(|key| params.get(key).and_then(Value::as_str)?.rsplit_once('!').map(|(name, _)| name))
        });
        named.and_then(|name| self.wb.sheet_index(name.trim_matches('\''))).unwrap_or(self.active)
    }
    pub fn range(&self, params: &Value) -> RangeRef {
        ["range", "cell", "at"].into_iter().find_map(|key| params.get(key).and_then(Value::as_str).and_then(area)).unwrap_or(self.selected)
    }
    pub fn address(&self, out: &mut Facts, si: usize, range: RangeRef) {
        let sid = self.sid(si);
        out.add(&sid, Vec::<String>::new());
        for (rows, field, first, last) in [(true, "rows", range.start.row, range.end.row), (false, "cols", range.start.col, range.end.col)] {
            out.window(&sid, field, first as usize, last as usize + 1);
            for at in
                first..=last.min(if rows { self.ids.sheets[si].rows.len() as u32 } else { self.ids.sheets[si].cols.len() as u32 }.saturating_sub(1))
            {
                out.position(&self.axis(si, rows, at), &["index"]);
            }
        }
    }
    pub fn lookup(&self, out: &mut Facts, params: &Value, si: usize) {
        if let Some(index) = params.get("sheet").and_then(Value::as_u64) {
            out.window("workbook", "sheets", index as usize, index as usize + 1);
        } else if params.get("sheet").is_some_and(Value::is_string)
            || ["range", "cell", "rows", "cols"].iter().any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|text| text.contains('!')))
        {
            out.add(&self.sid(si), ["name"]);
        }
    }
    pub fn whole(&self, out: &mut Facts) {
        let names = ToolIds::of_replica(&self.replica);
        for ty in self.replica.schema().types.keys() {
            for id in self.replica.objects_of_type(ty) {
                let id = names.show(&id);
                out.add(&id, ["*"]);
                out.position(&id, &["*"]);
            }
        }
    }

    pub fn constraints(&self, out: &mut Facts, si: usize, range: RangeRef) {
        out.add(&self.sid(si), kdm_model::constraint_fields(range));
    }

    pub fn inherited_style(&self, out: &mut Facts, si: usize, at: CellRef) {
        out.add(&self.cell(si, at), ["style"]);
        self.line(out, si, true, at.row, &["info.style"]);
        self.line(out, si, false, at.col, &["info.style"]);
        out.add("workbook", ["props.defaultStyle"]);
        let style = self.wb.sheets[si].style_id(at);
        if let Some(key) = self.ids.styles.get(style.0 as usize).filter(|key| style.0 != 0 && !key.is_empty()) {
            out.add("workbook", [format!("styles.{key}.style")]);
        }
    }

    pub fn fit(&self, out: &mut Facts, si: usize, range: RangeRef) {
        out.add(&self.sid(si), ["props.defaultRowHeight"]);
        for row in range.start.row..=range.end.row {
            self.line(out, si, true, row, &["info.size", "info.custom", "~fit"]);
        }
    }

    pub fn line(&self, out: &mut Facts, si: usize, rows: bool, at: u32, fields: &[&str]) {
        let axes = &self.ids.sheets[si];
        if (if rows { &axes.rows } else { &axes.cols }).get(at as usize).is_some() {
            out.add(&self.axis(si, rows, at), fields.iter().copied());
        } else {
            // Implicit axes have no object yet; a sheet-wide or owning-list
            // guard still includes the default line at this coordinate.
            out.add(&self.sid(si), fields.iter().map(|field| format!("{}.{at}.{field}", if rows { "rows" } else { "cols" })));
        }
    }

    pub fn shared(&self, out: &mut Facts, si: usize, range: Option<RangeRef>, structural: bool) {
        out.add(&self.sid(si), ["~editConnectors"]);
        if structural {
            out.add("workbook", ["~headerSync"]);
        }
        if let Some(range) = range {
            for table in self.wb.sheets[si].tables.iter().filter(|table| table.header_row) {
                let header = RangeRef::new(table.range.start, CellRef::new(table.range.start.row, table.range.end.col));
                if !range.intersects(&header) {
                    continue;
                }
                for at in header.iter().filter(|at| range.contains(*at)) {
                    out.add(&self.axis(si, false, at.col), ["~headerReferences"]);
                }
            }
        }
    }

    pub fn entry(&self, out: &mut Facts, si: usize, range: RangeRef, parse: bool) {
        self.address(out, si, range);
        self.constraints(out, si, range);
        out.add(&self.sid(si), ["props.protection"]);
        for at in range.iter().take(100_000) {
            out.add(&self.cell(si, at), ["input", "text", "array", "picture"]);
            self.inherited_style(out, si, at);
        }
        if parse {
            out.add("workbook", ["props.dateSystem"]);
        }
        self.fit(out, si, range);
        self.shared(out, si, Some(range), false);
    }

    pub fn source(&self, out: &mut Facts, si: usize, range: RangeRef, display: bool) {
        self.address(out, si, range);
        for at in range.iter().take(100_000) {
            out.add(&self.cell(si, at), ["input", "text", "array"]);
            if display {
                self.inherited_style(out, si, at);
            }
            if let Some(expr) = self.wb.sheets[si].cell(at).and_then(|cell| cell.formula.as_ref()).and_then(|formula| formula.expr.as_deref()) {
                self.expression(out, si, expr, 0);
            }
        }
        if display {
            out.add("workbook", ["props.dateSystem"]);
        }
    }

    pub fn expression(&self, out: &mut Facts, si: usize, expr: &kition_sheet_engine::formula::Expr, depth: usize) {
        use kition_sheet_engine::formula::{Expr, ast::SheetSel};
        assert!(depth < 32, "oracle formula recursion bound");
        expr.walk(&mut |node| match node {
            Expr::Ref(reference) => {
                let owners = match &reference.sheet {
                    SheetSel::Current => vec![si],
                    SheetSel::Named(name) => self.wb.sheet_index(name).into_iter().collect(),
                    SheetSel::Span(first, last) => {
                        let (a, b) = (self.wb.sheet_index(first).unwrap(), self.wb.sheet_index(last).unwrap());
                        (a.min(b)..=a.max(b)).collect()
                    }
                };
                for owner in owners {
                    let mut range = reference.range();
                    if !matches!(reference.sheet, SheetSel::Current) {
                        out.add(&self.sid(owner), ["name"]);
                    }
                    if range.count() > 100_000
                        && let Some(used) = self.wb.sheets[owner].used_range()
                    {
                        range.end.row = range.end.row.min(used.end.row);
                        range.end.col = range.end.col.min(used.end.col);
                    }
                    self.address(out, owner, range);
                    for at in range.iter().take(100_000) {
                        out.add(&self.cell(owner, at), ["input", "text", "array"]);
                        if let Some(expr) =
                            self.wb.sheets[owner].cell(at).and_then(|cell| cell.formula.as_ref()).and_then(|formula| formula.expr.as_deref())
                        {
                            self.expression(out, owner, expr, depth + 1);
                        }
                    }
                }
            }
            Expr::Name(name) => {
                for (i, definition) in self.wb.names.iter().enumerate().filter(|(_, definition)| definition.name.eq_ignore_ascii_case(name)) {
                    out.add(&format!("name:{}", self.ids.names[i]), ["name", "scope", "formula", "props"]);
                    let expr = kition_sheet_engine::formula::parse(&definition.formula).unwrap();
                    self.expression(out, definition.scope.unwrap_or(si), &expr, depth + 1);
                }
            }
            Expr::Struct(reference) => {
                if let Some((owner, index)) = self.wb.table(&reference.table) {
                    out.add(&self.sid(owner), [format!("tables[name={}]", serde_json::json!(reference.table))]);
                    self.source(out, owner, self.wb.sheets[owner].tables[index].range, false);
                }
            }
            _ => {}
        });
    }
}

pub(super) fn area(text: &str) -> Option<RangeRef> {
    let body = text.rsplit('!').next()?;
    RangeRef::parse(body).or_else(|| RangeRef::parse(&format!("{body}:{body}")))
}

#[test]
fn field_comparison_respects_path_boundaries() {
    assert!(path_covers("parts.series", "parts.series.0.color"));
    assert!(!path_covers("parts.series", "parts.seriesOther"));
    assert!(!path_covers("props.print.fit_to", "props.print.scale"));
}

#[test]
fn comparison_distinguishes_content_positions_windows_and_empty_fields() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let cx = Context::new(&mut doc);
    let mut wanted = Facts::default();
    wanted.add("sheet:1", ["props.print.orientation"]);
    wanted.window("sheet:1", "rows", 1, 4);
    let mut actual = Facts::default();
    actual.add("sheet:1", ["props.print.orientation", "props.print.paper"]);
    actual.window("sheet:1", "rows", 1, 2);
    actual.window("sheet:1", "rows", 2, 4);
    assert!(wanted.difference(&actual, &cx).is_empty());
    assert_eq!(actual.difference(&wanted, &cx), vec![Fact { object: "sheet:1".into(), kind: "field".into(), field: "props.print.paper".into() }]);
    wanted.position("sheet:1", &["sheets"]);
    assert_eq!(wanted.difference(&actual, &cx).len(), 1);
    let bounded = Facts::from_under(&Under {
        premises: vec![kition_kit::Premise::on("sheet:1").fields(Vec::<String>::new()).positions(Vec::<String>::new())],
        ..Default::default()
    });
    let whole = Facts::from_under(&Under { premises: vec![kition_kit::Premise::on("sheet:1")], ..Default::default() });
    assert!(!bounded.contains(&Fact { object: "sheet:1".into(), kind: "field".into(), field: "props.print.orientation".into() }, &cx));
    assert!(whole.contains(&Fact { object: "sheet:1".into(), kind: "field".into(), field: "props.print.orientation".into() }, &cx));
    assert_eq!(whole.difference(&bounded, &cx).len(), 2);
}
