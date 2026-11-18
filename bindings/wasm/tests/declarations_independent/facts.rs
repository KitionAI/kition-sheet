//! C8-sheet-pin: path-indexed facts, with the retained source row in every result.
use kition_kit::KitDocument;
use serde_json::json;

#[path = "facts/data.rs"]
mod data;
#[path = "facts/edit.rs"]
mod edit;
#[path = "facts/gate.rs"]
mod gate;
#[path = "facts/model.rs"]
mod model;
#[path = "facts/objects.rs"]
mod objects;
#[path = "facts/view.rs"]
mod view;

#[test]
fn every_registered_write_path_compares_all_held_facts() {
    let mut census = gate::Census::default();
    let mut count = 0;
    let mut compared = std::collections::BTreeSet::new();
    let mut keys = std::collections::BTreeSet::new();
    let rows = super::paths::rows().into_iter().chain(super::paths::parse("binding_entry", super::binding_entry::READINGS));
    for row in rows {
        let (mut doc, params) = super::paths::fixture(&row);
        if !doc.commands().into_iter().any(|command| command.id == row.command && command.mutates) {
            continue;
        }
        let mut cx = model::Context::new(&mut doc);
        cx.route(row.command, &params);
        let wanted = match row.family {
            "data_insert" => data::expected(&row, &params, &cx),
            "edit_format" => edit::expected(&row, &params, &cx),
            "objects" => objects::expected(&row, &params, &cx),
            "view_file" => view::expected(&row, &params, &cx),
            "binding_entry" => binding(&params, &cx),
            _ => panic!("unprojected family {}", row.family),
        };
        let under = doc.under(row.command, &params).unwrap().expect("registered write has an explicit declaration");
        let actual = model::Facts::from_under(&under);
        actual.validate(&cx);
        let less = wanted.difference(&actual, &cx);
        let more = actual.difference(&wanted, &cx);
        let key = format!("{}|{}|{}", row.family, row.command, row.params);
        assert!(keys.insert(key.clone()), "duplicate retained path {key}");
        compared.insert(row.command.to_owned());
        let record = json!({"key":key,"family":row.family,"command":row.command,"line":row.line,"source":source_row(&row),"holds":row.holds,"reads":row.reads,"note":row.note,"params":params,"expected":wanted.0,"actual":actual.0,"less":less,"more":more});
        eprintln!("PATH_FACT {record}");
        census.record(row.family, row.line, row.command, &less, &more, record);
        count += 1;
    }
    let registry = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0)
        .unwrap()
        .commands()
        .into_iter()
        .filter(|command| command.mutates)
        .map(|command| command.id)
        .collect();
    assert_eq!(compared, registry, "every registered write needs a compared blind path");
    assert_eq!(count, 382);
    census.finish(count, compared.len());
}

fn source_row(row: &super::paths::Reading) -> serde_json::Value {
    let (file, source, readings) = match row.family {
        "view_file" => ("view_file.rs", include_str!("view_file.rs"), super::view_file::READINGS),
        "data_insert" => ("data_insert.rs", include_str!("data_insert.rs"), super::data_insert::READINGS),
        "edit_format" => ("edit_format.rs", include_str!("edit_format.rs"), super::edit_format::READINGS),
        "objects" => ("objects.rs", include_str!("objects.rs"), super::objects::READINGS),
        "binding_entry" => ("binding_entry.rs", include_str!("binding_entry.rs"), super::binding_entry::READINGS),
        other => panic!("unrecorded source family {other}"),
    };
    let raw = readings.lines().nth(row.line - 1).unwrap();
    let line = source.lines().position(|line| line == raw).expect("retained row must exist verbatim") + 1;
    json!({"file":format!("bindings/wasm/tests/declarations_independent/{file}"),"line":line,"row":raw})
}

fn binding(params: &serde_json::Value, cx: &model::Context) -> model::Facts {
    let mut out = model::Facts::default();
    cx.entry(&mut out, cx.active, kition_sheet_engine::core::RangeRef::cell(cx.active_cell), true);
    if let Some(input) = params.get("text").and_then(serde_json::Value::as_str).filter(|input| input.starts_with('=')) {
        cx.expression(&mut out, cx.active, &kition_sheet_engine::formula::parse(input).unwrap(), 0);
    }
    // commandLine -> step(1,0) -> selection.move: the first visible row after
    // the source merge, plus the destination merge, determines the new cell.
    let sheet = &cx.wb.sheets[cx.active];
    let source = sheet.merge_at(cx.active_cell).map_or(cx.active_cell, |range| range.end);
    let mut row = source.row + 1;
    while sheet.is_row_hidden(row) {
        row += 1;
    }
    let destination = kition_sheet_engine::core::CellRef::new(row, cx.active_cell.col);
    cx.address(&mut out, cx.active, kition_sheet_engine::core::RangeRef::cell(destination));
    cx.constraints(&mut out, cx.active, kition_sheet_engine::core::RangeRef::cell(destination));
    for index in source.row + 1..=row {
        cx.line(&mut out, cx.active, true, index, &["info.hidden"]);
    }
    out
}

#[test]
fn view_paths_compare_all_held_facts() {
    let mut count = 0;
    for row in super::paths::rows().into_iter().filter(|row| row.family == "view_file") {
        let (mut doc, params) = super::paths::fixture(&row);
        if !doc.commands().into_iter().any(|command| command.id == row.command && command.mutates) {
            continue;
        }
        let cx = model::Context::new(&mut doc);
        let wanted = view::expected(&row, &params, &cx);
        let actual = model::Facts::from_under(&doc.under(row.command, &params).unwrap().unwrap_or_default());
        let less = wanted.difference(&actual, &cx);
        let more = actual.difference(&wanted, &cx);
        eprintln!("PATH_FACT {}", json!({"key":format!("{}|{}|{}",row.family,row.command,row.params),"less":less,"more":more}));
        assert!(less.is_empty(), "{} {} missing facts: {less:?}", row.command, params);
        assert!(more.is_empty(), "{} {} excess facts: {more:?}", row.command, params);
        count += 1;
    }
    assert_eq!(count, 45);
}

#[test]
fn registered_write_paths_are_available_to_the_fact_comparison() {
    let mut commands = std::collections::BTreeSet::new();
    let mut count = 0;
    for row in super::paths::rows() {
        let (doc, params) = super::paths::fixture(&row);
        let command = doc.commands().into_iter().find(|command| command.id == row.command).unwrap();
        if !command.mutates {
            continue;
        }
        assert!(!row.holds.is_empty() && !row.reads.is_empty());
        count += 1;
        commands.insert(row.command);
        if std::env::var_os("C8_FACT_DUMP").is_some() {
            eprintln!(
                "PATH_UNDER {}",
                json!({
                    "family":row.family,"line":row.line,"command":row.command,
                    "params":params,"holds":row.holds,"reads":row.reads,
                    "under":doc.under(row.command,&params).unwrap(),
                })
            );
        }
    }
    assert_eq!(commands.len(), 261);
    assert_eq!(count, 380);
}
