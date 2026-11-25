//! C8-land-13-sheet: a second engine really rewrites the XLSX between observe and write.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions, Guard};
use kition_kits::{KitDefinition, KitManager};
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};
use std::path::PathBuf;

fn book() -> Vec<u8> {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("sheet.rename", json!({"name":"Data"})).unwrap();
    let values: Vec<_> = (1..=12).map(|n| json!([format!("row{n}"), n])).collect();
    doc.execute("range.setValues", json!({"range":"A1", "values":values})).unwrap();
    for x in [10, 120] {
        doc.execute("drawing.insert", json!({"kind":"textBox","text":"before","frame":{"x":x,"y":10,"w":50,"h":30}})).unwrap();
    }
    doc.execute("drawing.ink", json!({"strokes":[{"points":[[10,100],[90,100]],"width":2}]})).unwrap();
    doc.execute("sheet.insert", json!({"name":"Notes","before":9})).unwrap();
    doc.execute("sheet.activate", json!({"sheet":"Data"})).unwrap();
    doc.save(None).unwrap()
}

struct Fixture {
    kits: KitManager,
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Book.xlsx"), book()).unwrap();
        let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(dir.path().to_path_buf()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-sheet"))))]);
        Self { kits, dir }
    }

    fn bytes(&self) -> Vec<u8> {
        std::fs::read(self.dir.path().join("Book.xlsx")).unwrap()
    }

    fn colleague(&self, steps: &[(&str, Value)]) {
        let mut other = SheetDocument::open("Book.xlsx", &self.bytes()).unwrap();
        for (command, params) in steps {
            other.execute(command, params.clone()).unwrap_or_else(|e| panic!("colleague {command}: {e}"));
        }
        std::fs::write(self.dir.path().join("Book.xlsx"), other.save(None).unwrap()).unwrap();
        let file = std::fs::File::options().write(true).open(self.dir.path().join("Book.xlsx")).unwrap();
        #[allow(clippy::disallowed_methods)] // a native test: the colleague's write is an hour old
        let an_hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        file.set_times(std::fs::FileTimes::new().set_modified(an_hour_ago)).unwrap();
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        self.call_guard(action, input.take(), Guard::Premise).await
    }

    async fn call_guard(&self, action: &str, mut input: Value, guard: Guard) -> Result<Value, Value> {
        input["path"] = json!("Book.xlsx");
        match tools::call_as(&self.kits, &format!("sheet_{action}"), &input, CallOptions { guard, ..CallOptions::default() }, "agent-me")
            .await
            .unwrap()
        {
            Ok(output) => Ok(output.output),
            Err(error) => Err(error.structured()),
        }
    }

    async fn basis(&self) -> Value {
        let answer = self.call("inspect", json!({})).await.unwrap();
        answer["revision"].clone()
    }

    async fn run(&self, basis: Value, command: &str, params: Value) -> Result<Value, Value> {
        self.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await
    }

    fn cell(&self, address: &str) -> Value {
        SheetDocument::open("Book.xlsx", &self.bytes()).unwrap().execute("cell.get", json!({"sheet":"Data","cell":address})).unwrap()["value"].clone()
    }

    async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn insert_after_the_address_is_unrelated() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("row.insert", json!({"rows":"Data!11:11"}))]);
    f.run(basis, "cell.set", json!({"sheet":"Data","cell":"B7","input":"42"})).await.unwrap();
    assert_eq!(f.cell("B7"), 42.0);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_unrelated_cell_does_not_hold_the_entire_cells_map() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"Z20","input":"99"}))]);
    f.run(basis, "cell.set", json!({"sheet":"Data","cell":"B7","input":"42"})).await.unwrap();
    assert_eq!(f.cell("B7"), 42.0);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn formatting_holds_destination_and_fitted_rows_but_allows_other_rows() {
    for change in ["other-row", "other-sheet", "destination", "new-row-cell", "before"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        let other = match change {
            "other-row" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
            "other-sheet" => ("cell.set", json!({"cell":"Notes!A1","input":"99"})),
            "destination" => ("format.italic", json!({"range":"Data!B7","on":true})),
            "new-row-cell" => ("cell.set", json!({"cell":"Data!Z7","input":"new"})),
            _ => ("row.insert", json!({"rows":"Data!2"})),
        };
        f.colleague(&[other]);
        let before = f.bytes();
        let result = f.run(basis, "format.bold", json!({"range":"Data!A7:B7","on":true})).await;
        if matches!(change, "other-row" | "other-sheet") {
            result.unwrap_or_else(|error| panic!("{change}: {error}"));
        } else {
            let error = result.expect_err(change);
            assert!(error.to_string().contains("conflict"), "{change}: {error}");
            assert_eq!(f.bytes(), before);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_formats_and_rich_text_hold_their_inputs_without_the_active_cell() {
    let mut problems = Vec::new();
    for (command, params) in [
        ("format.fontName", json!({"range":"Data!A7:B7","name":"Arial"})),
        ("format.numberFormat", json!({"range":"Data!A7:B7","format":"Number"})),
        ("format.fillColor", json!({"range":"Data!A7:B7","color":"#224466"})),
        ("format.increaseIndent", json!({"range":"Data!A7:B7"})),
        ("cell.formatText", json!({"cell":"Data!A7","start":0,"end":2,"bold":true})),
    ] {
        for change in ["active", "destination"] {
            let f = Fixture::new();
            let basis = f.basis().await;
            let range = if change == "active" { "Data!A1" } else { "Data!A7" };
            f.colleague(&[("format.italic", json!({"range":range,"on":true}))]);
            let before = f.bytes();
            let answer = f.run(basis, command, params.clone()).await;
            if change == "destination" {
                if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    problems.push(format!("{command}/{change}: required refusal, got {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                problems.push(format!("{command}/{change}: unrelated change refused: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn borders_and_merges_hold_only_their_style_or_content_footprints() {
    let mut problems = Vec::new();
    for mode in ["format.borders", "format.drawBorder", "cell.merge", "cell.mergeCenter", "cell.mergeAcross", "cell.unmerge", "edges", "merge"] {
        let command = if matches!(mode, "edges" | "merge") { "format.cells" } else { mode };
        for change in ["unrelated", "value", "style", "neighbor", "tall-neighbor", "overlap", "unrelated-merge"] {
            let f = Fixture::new();
            let border = matches!(mode, "format.borders" | "format.drawBorder" | "edges");
            if command == "cell.unmerge" {
                f.colleague(&[("cell.mergeAcross", json!({"range":"Data!A7:B8"}))]);
            }
            let basis = f.basis().await;
            let other = match change {
                "unrelated" => ("cell.set", json!({"cell":"Data!A1","input":"unrelated"})),
                "value" => ("cell.set", json!({"cell":"Data!B7","input":"99"})),
                "style" => ("format.italic", json!({"range":"Data!B7","on":true})),
                "neighbor" => ("format.italic", json!({"range":"Data!C7","on":true})),
                "tall-neighbor" => ("format.fontSize", json!({"range":"Data!C7","size":40})),
                "unrelated-merge" => ("cell.mergeAcross", json!({"range":"Data!C1:D2"})),
                _ => ("cell.mergeAcross", json!({"range":"Data!A6:C7"})),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let params = match mode {
                "format.borders" => json!({"range":"Data!A7:B8","preset":"none"}),
                "edges" => json!({"range":"Data!A7:B8","edges":[{"preset":"none"}]}),
                "merge" => json!({"range":"Data!A7:B8","merge":true}),
                _ => json!({"range":"Data!A7:B8"}),
            };
            let answer = f.run(basis, command, params).await;
            // Borders read styles, including adjacent facing edges only for removal.
            // Across and Unmerge change merge metadata, without reading cell entries.
            let necessary = border && change == "style"
                || matches!(mode, "format.borders" | "edges") && matches!(change, "neighbor" | "tall-neighbor")
                || !border && change == "overlap"
                || matches!(mode, "cell.merge" | "cell.mergeCenter" | "merge") && matches!(change, "value" | "style" | "tall-neighbor");
            if necessary {
                if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    problems.push(format!("{mode}/{change}: required refusal, got {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                problems.push(format!("{mode}/{change}: unrelated change refused: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn outline_controls_hold_selected_levels_and_discovered_boundaries() {
    let mut problems = Vec::new();
    for rows in [true, false] {
        let key = if rows { "rows" } else { "cols" };
        let selected = if rows { "6:7" } else { "F:G" };
        for command in ["data.group", "data.ungroup", "data.hideDetail", "data.showDetail"] {
            for change in ["unrelated", "level", "visibility", "boundary", "rename"] {
                let f = Fixture::new();
                if !rows {
                    f.colleague(&[("cell.set", json!({"cell":"Data!K1","input":"1"}))]);
                }
                f.colleague(&[("data.group", json!({(key):if rows { "4:9" } else { "D:I" }}))]);
                if command == "data.showDetail" {
                    f.colleague(&[(if rows { "row.hide" } else { "column.hide" }, json!({(key):if rows { "6" } else { "F" }}))]);
                }
                let basis = f.basis().await;
                let other = match change {
                    "unrelated" => ("cell.set", json!({"cell":if rows { "Data!B7" } else { "Data!F7" },"input":"99"})),
                    "level" => ("data.group", json!({(key):selected})),
                    "visibility" if command == "data.showDetail" => {
                        (if rows { "row.show" } else { "column.show" }, json!({(key):if rows { "6" } else { "F" }}))
                    }
                    "visibility" => (if rows { "row.hide" } else { "column.hide" }, json!({(key):if rows { "6" } else { "F" }})),
                    "boundary" => ("data.group", json!({(key):if rows { "3" } else { "C" }})),
                    _ => ("sheet.rename", json!({"name":"Renamed"})),
                };
                f.colleague(&[other]);
                let before = f.bytes();
                let answer =
                    f.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":command,"params":{(key):selected}}]})).await;
                let detail = matches!(command, "data.hideDetail" | "data.showDetail");
                let necessary = change == "level" || change == "visibility" && command != "data.group" || change == "boundary" && detail;
                if necessary {
                    if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                        problems.push(format!("{command}/{key}/{change}: required refusal, got {answer:?}"));
                    } else {
                        assert_eq!(f.bytes(), before);
                    }
                } else if let Err(error) = answer {
                    problems.push(format!("{command}/{key}/{change}: unrelated change refused: {error}"));
                }
                f.shutdown().await;
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn named_cell_styles_hold_the_chosen_definition_and_written_styles() {
    let mut problems = Vec::new();
    for form in ["custom", "builtin", "definition"] {
        for change in ["unrelated", "other-style", "chosen-style", "source-or-destination"] {
            let f = Fixture::new();
            f.colleague(&[
                ("format.bold", json!({"range":"Data!A1","on":true})),
                ("style.add", json!({"name":"Entry","fromCell":"A1"})),
                ("style.add", json!({"name":"OtherStyle","fromCell":"B1"})),
            ]);
            let basis = f.basis().await;
            let chosen = if form == "builtin" { "Good" } else { "Entry" };
            match change {
                "unrelated" => f.colleague(&[("cell.set", json!({"cell":"Data!Z20","input":"99"}))]),
                "other-style" | "chosen-style" => f.colleague(&[
                    ("format.italic", json!({"range":"Data!B1","on":true})),
                    ("style.add", json!({"name":if change == "other-style" { "OtherStyle" } else { chosen },"fromCell":"B1"})),
                ]),
                _ => f.colleague(&[("format.italic", json!({"range":if form == "definition" { "Data!A1" } else { "Data!B7" },"on":true}))]),
            }
            let before = f.bytes();
            let (command, params) = if form == "definition" {
                ("style.add", json!({"name":chosen,"fromCell":"A1"}))
            } else {
                ("style.apply", json!({"name":chosen,"range":"Data!B7"}))
            };
            let answer = f.run(basis, command, params).await;
            if matches!(change, "chosen-style" | "source-or-destination") {
                if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    problems.push(format!("{form}/{change}: required refusal, got {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                problems.push(format!("{form}/{change}: unrelated change refused: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn table_rename_holds_only_its_own_formula_membership() {
    let mut problems = Vec::new();
    for change in ["other-reference", "chosen-reference", "new-other-reference", "new-chosen-reference", "other-body"] {
        let f = Fixture::new();
        f.colleague(&[
            ("range.setValues", json!({"range":"Data!D1","values":[["Code","Value"],["a",10],["b",20],["c",30],["d",40]]})),
            ("table.insert", json!({"range":"Data!A1:B5","name":"Sales","header":true})),
            ("table.insert", json!({"range":"Data!D1:E5","name":"Other","header":true})),
            ("cell.set", json!({"cell":"Data!G1","input":"=SUM(Sales[#All])"})),
            ("cell.set", json!({"cell":"Data!G2","input":"=SUM(Other[#All])"})),
        ]);
        let basis = f.basis().await;
        let (cell, input) = match change {
            "other-reference" => ("G2", "=SUM(Other[#All])+1"),
            "chosen-reference" => ("G1", "=SUM(Sales[#All])+1"),
            "new-other-reference" => ("H2", "=SUM(Other[#All])"),
            "new-chosen-reference" => ("H1", "=SUM(Sales[#All])"),
            _ => ("E2", "77"),
        };
        f.colleague(&[("cell.set", json!({"cell":format!("Data!{cell}"),"input":input}))]);
        let before = f.bytes();
        let answer = f.run(basis, "table.rename", json!({"table":"Sales","name":"Revenue"})).await;
        if matches!(change, "chosen-reference" | "new-chosen-reference") {
            if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                problems.push(format!("{change}: required refusal, got {answer:?}"));
            } else {
                assert_eq!(f.bytes(), before);
            }
        } else if let Err(error) = answer {
            problems.push(format!("{change}: unrelated change refused: {error}"));
        }
        f.shutdown().await;
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn conditional_format_writes_hold_rules_and_windows_without_cell_values() {
    let mut problems = Vec::new();
    for (command, params) in [
        ("conditionalFormat.apply", json!({"range":"Data!A7:B8","rule":{"type":"cellIs","value":9}})),
        ("conditionalFormat.clearRules", json!({"range":"Data!A7:B8"})),
        ("conditionalFormat.clearRules", json!({"sheet":true})),
        ("conditionalFormat.manageRules", json!({"moveDown":0})),
    ] {
        for change in ["rules", "other-rules", "value", "before", "after"] {
            let f = Fixture::new();
            f.colleague(&[
                ("conditionalFormat.apply", json!({"range":"Data!A7:B8","rule":{"type":"cellIs","value":3}})),
                ("conditionalFormat.apply", json!({"range":"Data!D7:E8","rule":{"type":"cellIs","value":4}})),
            ]);
            let basis = f.basis().await;
            let colleague = match change {
                "rules" => ("conditionalFormat.apply", json!({"range":"Data!Z20","rule":{"type":"cellIs","value":8}})),
                "other-rules" => ("conditionalFormat.apply", json!({"range":"Notes!A1","rule":{"type":"cellIs","value":8}})),
                "value" => ("cell.set", json!({"cell":"Data!A7","input":"99"})),
                "before" => ("row.insert", json!({"rows":"Data!2"})),
                _ => ("row.insert", json!({"rows":"Data!12"})),
            };
            f.colleague(&[colleague]);
            let before = f.bytes();
            let result = f.run(basis, command, params.clone()).await;
            let before_is_read = match (command, params.get("sheet").and_then(Value::as_bool)) {
                // approved (necessity review): moving rules retains their current ranges.
                ("conditionalFormat.manageRules", _) => false,
                // same-field decision: deleting rules holds their replaced ranges.
                ("conditionalFormat.clearRules", Some(true)) => true,
                // same-field decision: an intersecting rule is removed with its ranges.
                ("conditionalFormat.clearRules", _) => true,
                _ => true,
            };
            let necessary = change == "rules" || change == "before" && before_is_read;
            if necessary {
                if !result.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    problems.push(format!("{command} {params} {change}: required refusal, got {result:?}"));
                } else {
                    if command == "conditionalFormat.clearRules" && change == "before" {
                        assert!(result.as_ref().unwrap_err()["conflicts"].to_string().contains("changed"));
                    }
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = result {
                problems.push(format!("{command} {params} {change}: unrelated change refused: {error}"));
            } else if change == "before" {
                let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
                let sheet = saved.sheet(0).unwrap();
                assert_eq!(sheet.value(kition_sheet_engine::core::CellRef::parse("B8").unwrap()).display(), "7", "outside insertion survives");
                let ranges: Vec<_> = sheet.cond_formats.iter().flat_map(|rule| rule.ranges.iter().map(|range| range.a1())).collect();
                let expected = if command == "conditionalFormat.manageRules" {
                    vec!["A8:B9", "D8:E9"]
                } else if params.get("sheet").and_then(Value::as_bool) == Some(true) {
                    vec![]
                } else {
                    vec!["D8:E9"]
                };
                assert_eq!(ranges, expected, "saved rules combine the current addresses and the requested operation");
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn conditional_format_numeric_sheet_holds_its_tab_position() {
    for change in ["tab-before", "cell"] {
        let f = Fixture::new();
        f.colleague(&[
            ("conditionalFormat.apply", json!({"range":"Data!A7:B8","rule":{"type":"cellIs","value":3}})),
            ("conditionalFormat.apply", json!({"range":"Data!D7:E8","rule":{"type":"cellIs","value":4}})),
        ]);
        let basis = f.basis().await;
        if change == "tab-before" {
            f.colleague(&[("sheet.move", json!({"sheet":"Notes","to":0}))]);
        } else {
            f.colleague(&[("cell.set", json!({"cell":"Notes!A1","input":"99"}))]);
        }
        let before = f.bytes();
        let answer = f.run(basis, "conditionalFormat.manageRules", json!({"sheet":0,"moveDown":0})).await;
        if change == "tab-before" {
            assert!(answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")), "{answer:?}");
            assert_eq!(f.bytes(), before);
        } else {
            answer.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn clear_families_hold_the_metadata_or_cells_they_actually_clear() {
    let mut problems = Vec::new();
    for command in ["edit.clearAll", "edit.clearFormats", "edit.clearComments", "edit.clearHyperlinks"] {
        for change in ["unrelated", "target-note", "target-link", "target-style", "target-value", "before", "other-note"] {
            let f = Fixture::new();
            f.colleague(&[
                ("note.add", json!({"cell":"Data!B7","text":"observed"})),
                ("link.insert", json!({"cell":"Data!B7","target":"https://example.org","tooltip":"observed"})),
            ]);
            let basis = f.basis().await;
            let colleague = match change {
                "unrelated" => ("cell.set", json!({"cell":"Data!B12","input":"99"})),
                "target-note" => ("note.add", json!({"cell":"Data!B7","text":"changed"})),
                "target-link" => ("link.insert", json!({"cell":"Data!B7","target":"https://example.org","tooltip":"changed"})),
                "target-style" => ("format.bold", json!({"range":"Data!B7","on":true})),
                "target-value" => ("cell.set", json!({"cell":"Data!B7","input":"99"})),
                "before" => ("row.insert", json!({"rows":"Data!2"})),
                _ => ("note.add", json!({"cell":"Data!A12","text":"unrelated"})),
            };
            f.colleague(&[colleague]);
            let before = f.bytes();
            let result = f.run(basis, command, json!({"range":"Data!A7:B8"})).await;
            let necessary = change == "before"
                || matches!(command, "edit.clearAll" | "edit.clearFormats") && matches!(change, "target-style" | "target-value")
                || matches!(command, "edit.clearAll" | "edit.clearComments") && change == "target-note"
                || matches!(command, "edit.clearAll" | "edit.clearHyperlinks") && change == "target-link";
            if necessary {
                if !result.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    problems.push(format!("{command}/{change}: required refusal, got {result:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = result {
                problems.push(format!("{command}/{change}: unrelated change refused: {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn filters_hold_scopes_sources_and_visibility_but_allow_unrelated_cells() {
    let mut problems = Vec::new();
    for command in ["data.filter", "data.filterBy", "data.clearFilter", "data.reapply"] {
        for change in ["unrelated", "source", "new-source", "criteria", "visibility"] {
            let f = Fixture::new();
            f.colleague(&[("data.filter", json!({"range":"Data!A1:B10"})), ("data.filterBy", json!({"column":"B","custom":{"op":">","value":"4"}}))]);
            if change == "new-source" {
                f.colleague(&[("cell.set", json!({"cell":"Data!B7","input":""}))]);
            }
            let basis = f.basis().await;
            let other = match change {
                "unrelated" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
                "source" | "new-source" => ("cell.set", json!({"cell":"Data!B7","input":"99"})),
                "criteria" => ("data.filterBy", json!({"column":"B","custom":{"op":">","value":"6"}})),
                _ => ("row.hide", json!({"rows":"Data!12"})),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let params = if command == "data.filterBy" { json!({"cell":"Data!B1","column":"B","values":["7","8"]}) } else { json!({}) };
            let answer = f.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":command,"params":params}]})).await;
            // Filter off and Clear do not read source entries. Only filterBy reports all
            // hidden rows; toggle-off writes them all. Reapply/Clear leave row 12 alone.
            let necessary = change == "criteria"
                || matches!(change, "source" | "new-source") && matches!(command, "data.filterBy" | "data.reapply")
                || change == "visibility" && matches!(command, "data.filter" | "data.filterBy");
            if necessary {
                if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                    problems.push(format!("{command}/{change}: required refusal, got {answer:?}"));
                } else {
                    assert_eq!(f.bytes(), before);
                }
            } else if let Err(error) = answer {
                problems.push(format!("{command}/{change}: unrelated refusal {error}"));
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn automatic_sizing_holds_only_the_fitted_lines() {
    let mut problems = Vec::new();
    for rows in [true, false] {
        for change in ["unrelated", "source", "new-source", "before"] {
            let f = Fixture::new();
            let basis = f.basis().await;
            let other = match change {
                "unrelated" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
                "source" => ("cell.set", json!({"cell":"Data!B7","input":"longer\nsource"})),
                "new-source" => ("cell.set", json!({"cell":if rows {"Data!Z7"} else {"Data!B20"},"input":"new\nsource"})),
                _ if rows => ("row.insert", json!({"rows":"Data!2"})),
                _ => ("column.insert", json!({"cols":"Data!A"})),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let (command, params) =
                if rows { ("row.autofitHeight", json!({"rows":"Data!7"})) } else { ("column.autofitWidth", json!({"cols":"Data!B"})) };
            let answer = f.run(basis, command, params).await;
            if change == "unrelated" {
                if let Err(error) = answer {
                    problems.push(format!("{command}: unrelated refusal {}", error.to_string().chars().take(512).collect::<String>()));
                }
            } else if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                problems.push(format!("{command}/{change}: required refusal, got {answer:?}"));
            } else {
                assert_eq!(f.bytes(), before);
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn structural_table_controls_hold_their_table_and_written_cells() {
    let mut problems = Vec::new();
    for (command, params, target) in [
        ("table.headerRow", json!({"table":"Sales","on":false}), "A1"),
        ("table.totalRow", json!({"table":"Sales","on":true}), "B5"),
        ("table.totalFunction", json!({"table":"Sales","column":"row1","function":"count"}), "B5"),
        ("table.convertToRange", json!({"table":"Sales"}), "B3"),
    ] {
        for change in ["unrelated", "destination", "definition"] {
            let f = Fixture::new();
            f.colleague(&[("table.insert", json!({"range":"Data!A1:B4","name":"Sales","header":true}))]);
            if command == "table.totalFunction" {
                f.colleague(&[("table.totalRow", json!({"table":"Sales","on":true}))]);
            }
            let basis = f.basis().await;
            let other = match change {
                "unrelated" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
                "destination" => ("cell.set", json!({"sheet":"Data","cell":target,"input":"99","expand":false})),
                _ => ("table.bandedRows", json!({"table":"Sales","on":false})),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let answer = f.run(basis, command, params.clone()).await;
            // same-field decision: header removal and totals replace cell input.
            let destination_is_read = true;
            if change == "unrelated" || change == "destination" && !destination_is_read {
                if let Err(error) = answer {
                    problems.push(format!("{command}: unrelated refusal {}", error.to_string().chars().take(512).collect::<String>()));
                } else if change == "destination" {
                    let (outside, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &before).unwrap();
                    let (saved, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &f.bytes()).unwrap();
                    let sheet = saved.sheet(0).unwrap();
                    assert_eq!(sheet.rows, outside.sheet(0).unwrap().rows, "outside row metadata survives");
                    let table = sheet.tables.iter().find(|table| table.name == "Sales").unwrap();
                    if command == "table.headerRow" {
                        assert!(!table.header_row);
                        assert_eq!(table.range.a1(), "A2:B4");
                        assert_eq!(table.columns[0].name, "99", "header-off preserves the outside header rename");
                        assert_eq!(f.cell("A1"), Value::Null);
                    } else {
                        assert!(table.totals_row);
                        assert_eq!(table.range.a1(), "A1:B5");
                        assert_eq!(f.cell("B5"), json!(9.0), "the requested totals formula replaces the outside destination constant");
                        if command == "table.totalFunction" {
                            assert_eq!(f.cell("A5"), json!(3.0));
                        }
                    }
                }
            } else if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                problems.push(format!("{command}/{change}: required refusal, got {answer:?}"));
            } else {
                if change == "destination" {
                    assert!(answer.as_ref().unwrap_err()["conflicts"].to_string().contains("changed"));
                }
                assert_eq!(f.bytes(), before);
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn table_creation_rename_and_duplicates_hold_their_actual_inputs() {
    let mut problems = Vec::new();
    for family in ["insert", "rename", "duplicates"] {
        for change in ["unrelated", "unrelated-formula", "other-body", "inside", "source", "membership"] {
            let f = Fixture::new();
            if family != "insert" {
                f.colleague(&[
                    ("table.insert", json!({"range":"Data!A1:B4","header":true,"name":"Sales"})),
                    ("cell.set", json!({"cell":"Data!D1","input":"=SUM(Sales[1])"})),
                ]);
            }
            let basis = f.basis().await;
            let other = match change {
                "unrelated" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
                "unrelated-formula" => ("cell.set", json!({"cell":"Data!D20","input":"=1+1"})),
                "other-body" => ("cell.set", json!({"cell":"Data!B3","input":"99"})),
                "inside" => ("row.insert", json!({"rows":"Data!3"})),
                "source" if family == "rename" => ("cell.set", json!({"cell":"Data!D1","input":"=SUM(Sales[1])+1"})),
                "source" => ("cell.set", json!({"cell":"Data!B2","input":"99"})),
                _ if family == "rename" => ("cell.set", json!({"cell":"Data!D20","input":"=SUM(Sales[1])"})),
                _ if family == "insert" => ("table.insert", json!({"range":"Data!A7:B10","header":true,"name":"NewSales"})),
                _ => ("table.bandedRows", json!({"table":"Sales","on":false})),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let (command, params) = match family {
                "insert" => ("table.insert", json!({"range":"Data!A2:B4","header":true,"name":"NewSales"})),
                "rename" => ("table.rename", json!({"table":"Sales","name":"Revenue"})),
                _ => ("table.removeDuplicates", json!({"table":"Sales"})),
            };
            let answer = f.run(basis, command, params).await;
            if matches!(change, "unrelated" | "unrelated-formula")
                || change == "other-body" && family != "duplicates"
                || change == "inside" && family == "rename"
            {
                if let Err(error) = answer {
                    problems.push(format!("{command}: unrelated refusal {}", error.to_string().chars().take(512).collect::<String>()));
                }
            } else if !answer.as_ref().is_err_and(|error| error.to_string().contains("conflict")) {
                problems.push(format!("{command}/{change}: required refusal, got {answer:?}"));
            } else {
                assert_eq!(f.bytes(), before);
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn formula_fill_holds_every_translated_source_and_its_windows() {
    for change in ["unrelated", "after", "relative", "absolute", "before", "inside"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        let other = match change {
            "unrelated" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
            "after" => ("row.insert", json!({"rows":"Data!11:11"})),
            "relative" => ("cell.set", json!({"cell":"Data!B5","input":"99"})),
            "absolute" => ("cell.set", json!({"cell":"Data!B1","input":"99"})),
            "before" => ("row.insert", json!({"rows":"Data!2:2"})),
            _ => ("row.insert", json!({"rows":"Data!4:4"})),
        };
        f.colleague(&[other]);
        let before = f.bytes();
        let result = f.run(basis, "range.fill", json!({"range":"Data!C2:C4","input":"=SUM(B2:B3)+$B$1"})).await;
        if matches!(change, "unrelated" | "after") {
            result.unwrap_or_else(|error| panic!("{change}: {error}"));
            assert_eq!(f.cell("C4"), json!(10.0));
        } else {
            let error = result.expect_err(change);
            assert!(error.to_string().contains(if matches!(change, "before" | "inside") { "relocated" } else { "changed" }), "{change}: {error}");
            assert_eq!(f.bytes(), before);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn narrowed_families_allow_unrelated_changes_and_refuse_their_dependencies() {
    let mut problems = Vec::new();
    for family in ["paste", "sort", "name", "validation", "table"] {
        for related in [false, true] {
            let f = Fixture::new();
            f.colleague(&[
                ("name.define", json!({"name":"Rate","refersTo":"=Data!B7"})),
                ("table.insert", json!({"range":"Data!A1:B4","header":false,"name":"Sales"})),
            ]);
            let basis = f.basis().await;
            let other = if !related {
                ("cell.set", json!({"cell":"Data!Z20","input":"99"}))
            } else {
                match family {
                    "paste" => ("cell.set", json!({"cell":"Data!B2","input":"99"})),
                    "sort" => ("cell.set", json!({"cell":"Data!B7","input":"99"})),
                    "name" => ("name.define", json!({"name":"Rate","refersTo":"=Data!B8"})),
                    "validation" => ("data.validation", json!({"range":"Data!B7","type":"whole","formula1":"2","formula2":"9"})),
                    _ => ("cell.set", json!({"cell":"Data!B1","input":"Changed header"})),
                }
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let (command, params) = match family {
                "paste" => ("edit.paste", json!({"at":"B7","text":"=B2\t43\n44\t45"})),
                "sort" => ("data.sort", json!({"range":"Data!A5:B8","header":false,"keys":[{"column":"B","order":"desc"}]})),
                "name" => ("name.define", json!({"name":"Rate","refersTo":"=Data!B9"})),
                "validation" => ("data.validation", json!({"range":"Data!B7:B8","type":"whole","formula1":"1","formula2":"10"})),
                _ => ("table.resize", json!({"table":"Sales","range":"Data!A1:B5"})),
            };
            let answer = f.run(basis, command, params).await;
            if related {
                match answer {
                    Err(error) if error.to_string().contains("conflict") => assert_eq!(f.bytes(), before),
                    answer => problems.push(format!(
                        "{family}: changed dependency was not refused: {}",
                        match answer {
                            Ok(_) => "applied".to_owned(),
                            Err(error) => error.to_string().chars().take(512).collect::<String>(),
                        }
                    )),
                }
            } else {
                if let Err(error) = answer {
                    problems.push(format!("{family}: unrelated cell was refused: {}", error.to_string().chars().take(512).collect::<String>()));
                }
            }
            f.shutdown().await;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn named_cell_writes_follow_identity_and_hold_definition_and_content() {
    for change in ["unrelated", "other-name", "before", "rename-sheet", "destination", "definition", "delete", "rename-name", "picture"] {
        let f = Fixture::new();
        f.colleague(&[("name.define", json!({"name":"Destination","refersTo":"=Data!$B$7"}))]);
        let picture = json!({"at":"B7","base64":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==","placement":"cell","alt":"before"});
        if change == "picture" {
            f.colleague(&[("image.insert", picture.clone())]);
        }
        let basis = f.basis().await;
        let other = match change {
            "unrelated" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
            "other-name" => ("name.define", json!({"name":"Other","refersTo":"0.2"})),
            "before" => ("row.insert", json!({"rows":"Data!2"})),
            "rename-sheet" => ("sheet.rename", json!({"sheet":"Data","name":"Renamed"})),
            "destination" => ("cell.set", json!({"cell":"Data!B7","input":"99"})),
            "delete" | "rename-name" => ("name.delete", json!({"name":"Destination"})),
            "picture" => {
                let mut updated = picture;
                updated["alt"] = json!("theirs");
                ("image.insert", updated)
            }
            _ => ("name.define", json!({"name":"Destination","refersTo":"=Data!$B$8"})),
        };
        f.colleague(&[other]);
        if change == "rename-name" {
            f.colleague(&[("name.define", json!({"name":"MovedName","refersTo":"=Data!$B$7"}))]);
        }
        let before = f.bytes();
        let answer = f.run(basis, "cell.set", json!({"name":"Destination","input":"42"})).await;
        if matches!(change, "destination" | "definition" | "delete" | "rename-name" | "picture") {
            let error = answer.expect_err(change);
            assert!(error.to_string().contains("conflict"), "{change}: {error}");
            assert_eq!(f.bytes(), before);
        } else {
            answer.unwrap_or_else(|error| panic!("{change}: {error}"));
            let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            let sheet = if change == "rename-sheet" { "Renamed" } else { "Data" };
            let at = if change == "before" { "B8" } else { "B7" };
            assert_eq!(saved.execute("cell.get", json!({"sheet":sheet,"cell":at})).unwrap()["value"], 42.0);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn comment_commands_hold_the_target_comment_and_its_address() {
    for (command, params) in [
        ("note.add", json!({"text":"A note"})),
        ("comment.add", json!({"text":"A thread"})),
        ("comment.reply", json!({"text":"A reply"})),
        ("comment.resolve", json!({"resolved":true})),
        ("note.show", json!({})),
        ("comment.delete", json!({})),
    ] {
        for change in ["value", "other-note", "after", "before", "target-note"] {
            let f = Fixture::new();
            f.colleague(&[("comment.add", json!({"sheet":"Data","cell":"B7","text":"Observed"}))]);
            let basis = f.basis().await;
            let other = match change {
                "value" => ("cell.set", json!({"sheet":"Data","cell":"B7","input":"99"})),
                "other-note" => ("note.add", json!({"sheet":"Data","cell":"Z20","text":"Unrelated"})),
                "after" => ("row.insert", json!({"rows":"Data!11:11"})),
                "before" => ("row.insert", json!({"rows":"Data!2:2"})),
                _ => ("note.add", json!({"sheet":"Data","cell":"B7","text":"Changed"})),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let mut write = params.clone();
            write["cell"] = json!("Data!B7");
            let result = f.run(basis, command, write).await;
            if matches!(change, "before" | "target-note") {
                let error = result.expect_err(command);
                assert!(error.to_string().contains(if change == "before" { "relocated" } else { "changed" }), "{command}/{change}: {error}");
                assert_eq!(f.bytes(), before);
            } else {
                result.unwrap_or_else(|error| panic!("{command}/{change}: {error}"));
                if change == "value" {
                    assert_eq!(f.cell("B7"), 99.0);
                }
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fixed_axis_properties_hold_their_fields_and_address_windows() {
    for (command, write, rows, unhide) in [
        ("row.setHeight", json!({"rows":"Data!7","height":28}), true, false),
        ("column.setWidth", json!({"cols":"Data!B","width":90}), false, false),
        ("row.hide", json!({"rows":"Data!7"}), true, false),
        ("column.hide", json!({"cols":"Data!B"}), false, false),
        ("row.show", json!({"rows":"Data!7"}), true, true),
        ("column.show", json!({"cols":"Data!B"}), false, true),
    ] {
        for change in ["value", "other-axis", "after", "before", "property", "neighbour"] {
            let f = Fixture::new();
            if !rows {
                f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"C12","input":"13"}))]);
            }
            let size = if rows { "row.setHeight" } else { "column.setWidth" };
            let target = if rows { json!({"sheet":"Data","rows":"7","height":25}) } else { json!({"sheet":"Data","cols":"B","width":85}) };
            if unhide {
                f.colleague(&[(if rows { "row.hide" } else { "column.hide" }, write.clone())]);
            }
            let basis = f.basis().await;
            let other = match change {
                "value" => ("cell.set", json!({"sheet":"Data","cell":"B7","input":"99"})),
                "other-axis" => (size, if rows { json!({"rows":"Data!20","height":25}) } else { json!({"cols":"Data!Z","width":85}) }),
                "after" => {
                    (if rows { "row.insert" } else { "column.insert" }, if rows { json!({"rows":"Data!11"}) } else { json!({"cols":"Data!E"}) })
                }
                "before" => {
                    (if rows { "row.insert" } else { "column.insert" }, if rows { json!({"rows":"Data!2"}) } else { json!({"cols":"Data!A"}) })
                }
                "neighbour" => (size, if rows { json!({"rows":"Data!6","height":25}) } else { json!({"cols":"Data!A","width":85}) }),
                _ if matches!(command, "row.hide" | "column.hide") => (command, write.clone()),
                _ => (size, target),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let result = f.run(basis, command, write.clone()).await;
            if matches!(change, "before" | "property") || unhide && change == "neighbour" {
                let error = result.expect_err(command);
                assert!(error.to_string().contains(if change == "before" { "relocated" } else { "changed" }), "{command}/{change}: {error}");
                assert_eq!(f.bytes(), before);
            } else {
                result.unwrap_or_else(|error| panic!("{command}/{change}: {error}"));
                if change == "value" {
                    assert_eq!(f.cell("B7"), 99.0);
                }
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn property_commands_allow_cells_but_refuse_changes_to_the_held_property() {
    for (command, write, other) in [
        ("sheet.protect", json!({"formatCells":true}), json!({"formatRows":true})),
        ("workbook.protect", json!({"on":true}), json!({"on":true})),
        ("view.gridlines", json!({"on":false}), json!({"on":false})),
        ("view.headings", json!({"on":false}), json!({"on":false})),
        ("view.showZeros", json!({"on":false}), json!({"on":false})),
        ("view.rightToLeft", json!({"on":true}), json!({"on":true})),
        ("page.orientation", json!({"orientation":"landscape"}), json!({"orientation":"landscape"})),
        ("page.size", json!({"paper":"A3"}), json!({"paper":"A3"})),
        ("page.headerFooter", json!({"header":"New header"}), json!({"header":"Other header"})),
    ] {
        for necessary in [false, true] {
            let f = Fixture::new();
            let basis = f.basis().await;
            if necessary {
                f.colleague(&[(command, other.clone())]);
            } else {
                f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"Z20","input":"99"}))]);
            }
            let before = f.bytes();
            let result = f.run(basis, command, write.clone()).await;
            if necessary {
                let error = result.expect_err(command);
                assert!(error.to_string().contains("changed") || error.to_string().contains("unknown"), "{command}: {error}");
                assert_eq!(f.bytes(), before, "{command}");
            } else {
                result.unwrap_or_else(|error| panic!("{command}: {error}"));
                assert_eq!(f.cell("Z20"), 99.0);
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cell_constraints_hold_only_regions_that_cover_the_destination() {
    for (command, field, near, far, rest) in [
        ("cell.merge", "range", "B7:C8", "Z20:AA21", json!({})),
        ("table.insert", "range", "B5:C9", "Z20:AA22", json!({"name":"ReceiptTable","header":true})),
        ("data.validation", "range", "B7:C8", "Z20:AA21", json!({"type":"whole","formula1":"1","formula2":"100"})),
    ] {
        for necessary in [false, true] {
            let f = Fixture::new();
            let basis = f.basis().await;
            let mut params = rest.clone();
            params[field] = json!(if necessary { near } else { far });
            f.colleague(&[(command, params)]);
            let before = f.bytes();
            let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"B7","input":"42"})).await;
            if necessary {
                let error = result.expect_err(command);
                assert!(error.to_string().contains("changed") || error.to_string().contains("unknown"), "{command}: {error}");
                assert_eq!(f.bytes(), before);
            } else {
                result.unwrap_or_else(|error| panic!("{command}: {error}"));
                assert_eq!(f.cell("B7"), 42.0);
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn workbook_property_writes_hold_only_the_properties_they_read() {
    for (command, write, sibling, changed) in [
        ("file.properties", json!({"title":"Receipt"}), json!({"author":"Colleague"}), json!({"title":"Colleague"})),
        ("formula.calculationOptions", json!({"mode":"manual"}), json!({"maxIterations":200}), json!({"mode":"manual"})),
        ("formula.calculationOptions", json!({"iterative":true}), json!({"maxChange":0.01}), json!({"iterative":true})),
    ] {
        for change in ["cell", "sibling", "property"] {
            let f = Fixture::new();
            let basis = f.basis().await;
            match change {
                "cell" => f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"Z20","input":"99"}))]),
                "sibling" => f.colleague(&[(command, sibling.clone())]),
                _ => f.colleague(&[(command, changed.clone())]),
            }
            let before = f.bytes();
            let result = f.run(basis, command, write.clone()).await;
            assert_eq!(result.is_ok(), change != "property", "{command}/{change}: {result:?}");
            if change == "property" {
                assert_eq!(f.bytes(), before);
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn table_appearance_writes_hold_the_chosen_table() {
    for command in ["table.bandedRows", "table.bandedColumns", "table.firstColumn", "table.lastColumn", "table.filterButton", "table.style"] {
        for change in ["cell", "other-table", "chosen-table", "rename-away"] {
            let f = Fixture::new();
            f.colleague(&[
                ("table.insert", json!({"range":"Data!A1:B12","header":true,"name":"Observed"})),
                ("range.setValues", json!({"range":"Data!D1:E3","values":[["Item","Value"],["Other",1],["Last",2]]})),
                ("table.insert", json!({"range":"Data!D1:E3","header":true,"name":"Unrelated"})),
            ]);
            let basis = f.basis().await;
            let other = match change {
                "cell" => ("cell.set", json!({"cell":"Data!Z20","input":"99"})),
                "other-table" => ("table.style", json!({"table":"Unrelated","style":"None"})),
                "chosen-table" => ("table.style", json!({"table":"Observed","style":"None"})),
                _ => ("table.rename", json!({"table":"Observed","name":"Renamed"})),
            };
            f.colleague(&[other]);
            let before = f.bytes();
            let params =
                if command == "table.style" { json!({"table":"Observed","style":"TableStyleLight1"}) } else { json!({"table":"Observed","on":true}) };
            let result = f.run(basis, command, params).await;
            assert_eq!(result.is_ok(), matches!(change, "cell" | "other-table"), "{command}/{change}: {result:?}");
            if matches!(change, "chosen-table" | "rename-away") {
                assert_eq!(f.bytes(), before);
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn chart_handlers_hold_the_source_cells_they_read() {
    let mut accepted = Vec::new();
    for (command, params) in [
        ("chart.selectData", json!({"chart":"drawing:4","range":"A1:B12"})),
        ("chart.switchRowColumn", json!({"chart":"drawing:4"})),
        ("chart.changeColors", json!({"chart":"drawing:4","palette":"monochrome"})),
        ("drawing.chart", json!({"id":"drawing:4","title":"New title"})),
    ] {
        let f = Fixture::new();
        f.colleague(&[("chart.insert", json!({"range":"Data!A1:B12","type":"pie"}))]);
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"A1","input":"Changed header"}))]);
        let before = f.bytes();
        match f.run(basis, command, params).await {
            Ok(_) => accepted.push(command),
            Err(error) => {
                assert!(error.to_string().contains("changed"), "{command}: {error}");
                assert_eq!(f.bytes(), before);
            }
        }
        f.shutdown().await;
    }
    assert!(accepted.is_empty(), "source-dependent chart writes accepted stale cells: {accepted:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn chart_source_holds_allow_unrelated_cells_and_follow_formula_dependencies() {
    chart_formula_sources(true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn object_target_creation_of_empty_formula_sources() {
    chart_formula_sources(false).await;
}

async fn chart_formula_sources(existing: bool) {
    for (command, params) in [
        ("chart.selectData", json!({"chart":"drawing:4","range":"A1:B12"})),
        ("chart.switchRowColumn", json!({"chart":"drawing:4"})),
        ("chart.changeColors", json!({"chart":"drawing:4","palette":"monochrome"})),
        ("drawing.chart", json!({"id":"drawing:4","title":"New title"})),
        ("drawing.transform", json!({"ids":["drawing:4"],"dx":10})),
    ] {
        for (cell, expected) in [("D1", false), ("Z20", true)] {
            let f = Fixture::new();
            if existing {
                f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"D1","input":"1"}))]);
            }
            f.colleague(&[
                ("cell.set", json!({"sheet":"Data","cell":"B2","input":"=D1*2"})),
                ("chart.insert", json!({"range":"Data!A1:B12","type":"pie"})),
            ]);
            let doc = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
            let under = doc.under(command, &params).unwrap().unwrap();
            assert!(under.objects.contains(&json!("cell:1!D1")), "{command}: {under:?}");
            let basis = f.basis().await;
            f.colleague(&[("cell.set", json!({"sheet":"Data","cell":cell,"input":"99"}))]);
            let result = f.run(basis, command, params.clone()).await;
            assert_eq!(result.is_ok(), expected, "{command} after {cell}: {result:?}");
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn formula_metadata_changes_refuse_stale_meaning_but_allow_unrelated_cells() {
    let mut wrong = Vec::new();
    for (formula, other, nested) in [
        ("=SUBTOTAL(109,B2:B9)", ("row.hide", json!({"rows":4})), false),
        ("=AGGREGATE(9,1,B2:B9)", ("row.hide", json!({"rows":4})), false),
        ("=CELL(\"width\",B7)", ("column.setWidth", json!({"cols":"Data!B","width":140})), false),
        ("=CELL(\"sheetname\",B7)", ("sheet.rename", json!({"sheet":"Data","name":"Renamed"})), false),
        ("=SHEET(\"Data\")", ("sheet.move", json!({"sheet":"Data","to":1})), false),
        ("=SHEETS()", ("sheet.insert", json!({"name":"Added"})), false),
        ("=D4", ("column.setWidth", json!({"cols":"Data!D","width":140})), true),
    ] {
        for necessary in [false, true] {
            let f = Fixture::new();
            if nested {
                f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"D4","input":"=CELL(\"width\")"}))]);
            }
            let basis = f.basis().await;
            f.colleague(&[if necessary { other.clone() } else { ("cell.set", json!({"sheet":"Data","cell":"Z20","input":"99"})) }]);
            let before = f.bytes();
            let result = f.run(basis, "cell.set", json!({"sheet":"sheet:1","cell":"E1","input":formula})).await;
            if necessary {
                match result {
                    Ok(_) => wrong.push(format!("{formula}: stale write accepted")),
                    Err(error) => {
                        assert!(error.to_string().contains("changed") || error.to_string().contains("relocated"), "{formula}: {error}");
                        assert_eq!(f.bytes(), before);
                    }
                }
            } else if let Err(error) = result {
                wrong.push(format!("{formula}: unrelated edit refused: {error}"));
            } else {
                assert_eq!(f.cell("Z20"), 99.0);
            }
            f.shutdown().await;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_formula_holds_previously_empty_source_cells() {
    for (address, necessary) in [("C5", true), ("C20", false)] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"C12","input":"1"}))]);
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"sheet":"Data","cell":address,"input":"99"}))]);
        let before = f.bytes();
        let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":"=SUM(C2:C9)"})).await;
        if necessary {
            let error = result.expect_err("a new value in an observed blank source changes the premise");
            assert!(error.to_string().contains("changed") || error.to_string().contains("unknown"), "{error}");
            assert_eq!(f.bytes(), before);
        } else {
            result.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_formula_holds_a_source_cleared_after_observe() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("edit.clearContents", json!({"range":"B5"}))]);
    let before = f.bytes();
    let error = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":"=SUM(B2:B9)"})).await.expect_err("a cleared source changed");
    assert!(error.to_string().contains("deleted") || error.to_string().contains("changed") || error.to_string().contains("unknown"), "{error}");
    assert_eq!(f.bytes(), before);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn formula_dependency_audit_includes_large_dynamic_and_structured_sources() {
    let mut failures = Vec::new();
    for formula in ["=SUM(B:B)", "=INDIRECT(\"B7\")", "=SUM(OFFSET(B2,0,0,8,1))", "=SUM(Sales[Amount])"] {
        let f = Fixture::new();
        f.colleague(&[
            ("range.setValues", json!({"range":"A1:B1","values":[["Item","Amount"]]})),
            ("table.insert", json!({"range":"A1:B12","name":"Sales","header":true})),
        ]);
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":"B7","input":"99"}))]);
        let before = f.bytes();
        let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":formula})).await;
        if result.is_ok() || f.bytes() != before {
            failures.push(formula);
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "stale formula sources were accepted: {failures:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn conservative_handlers_hold_child_cells_they_read() {
    let mut failures = Vec::new();
    for (command, params) in [
        ("format.bold", json!({"range":"B7","on":true})),
        ("data.sortAscending", json!({"range":"A2:B9","header":false})),
        ("edit.fillDown", json!({"range":"B2:B9"})),
        ("spelling.change", json!({"cell":"B7","word":"7","to":"seven"})),
    ] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":"B7","input":"99"}))]);
        let before = f.bytes();
        let result = f.run(basis, command, params).await;
        if result.is_ok() || f.bytes() != before {
            failures.push(command);
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "document holds missed child edits: {failures:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn structured_formula_allows_changes_outside_its_source_column() {
    for (command, params) in [
        ("cell.set", json!({"cell":"B20","input":"99"})),
        ("cell.set", json!({"cell":"A7","input":"Another label"})),
        ("drawing.transform", json!({"id":1,"dx":200})),
    ] {
        let f = Fixture::new();
        f.colleague(&[
            ("range.setValues", json!({"range":"A1:B1","values":[["Item","Amount"]]})),
            ("table.insert", json!({"range":"A1:B12","name":"Sales","header":true})),
        ]);
        let basis = f.basis().await;
        f.colleague(&[(command, params)]);
        f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":"=SUM(Sales[Amount])"})).await.unwrap();
        assert_eq!(f.cell("D1"), 77.0);
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn formula_premises_follow_signed_and_text_formatted_entry() {
    let mut failures = Vec::new();
    for (input, text_format, refuse) in [("+SUM(B2:B9)", false, true), ("-SUM(B2:B9)", false, true), ("=SUM(B2:B9)", true, false)] {
        let f = Fixture::new();
        if text_format {
            f.colleague(&[("format.numberFormat", json!({"range":"D1","format":"@"}))]);
        }
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"cell":"B7","input":"99"}))]);
        let before = f.bytes();
        let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":input})).await;
        if result.is_err() != refuse || (refuse && f.bytes() != before) || (!refuse && result.is_ok() && f.cell("D1") != input) {
            failures.push((input, text_format));
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "formula premises disagree with entry interpretation: {failures:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn structured_formula_refuses_a_changed_table_definition() {
    for (command, params) in [("table.rename", json!({"table":"Sales","name":"Revenue"})), ("table.resize", json!({"table":"Sales","range":"A1:B9"}))]
    {
        let f = Fixture::new();
        f.colleague(&[
            ("range.setValues", json!({"range":"A1:B1","values":[["Item","Amount"]]})),
            ("table.insert", json!({"range":"A1:B12","name":"Sales","header":true})),
        ]);
        let basis = f.basis().await;
        f.colleague(&[(command, params)]);
        let before = f.bytes();
        let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":"=SUM(Sales[Amount])"})).await;
        assert!(result.is_err(), "the source table changed: {command}");
        assert_eq!(f.bytes(), before);
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn formula_sheet_context_preserves_meaning_across_tab_edits() {
    let mut failures = Vec::new();
    for kind in ["current-rename", "named-rename-away", "span-move-out"] {
        let f = Fixture::new();
        if kind != "current-rename" {
            f.colleague(&[("sheet.insert", json!({"name":"Source","before":1})), ("cell.set", json!({"sheet":"Source","cell":"B7","input":"70"}))]);
        }
        let basis = f.basis().await;
        let (command, params, input, refuse) = match kind {
            "current-rename" => ("sheet.rename", json!({"sheet":"Data","name":"Renamed"}), "=SUM(B2:B9)", false),
            "named-rename-away" => ("sheet.rename", json!({"sheet":"Source","name":"Archived"}), "=SUM(Source!B7)", true),
            _ => ("sheet.move", json!({"sheet":"Source","to":2}), "=SUM(Data:Notes!B7)", true),
        };
        f.colleague(&[(command, params)]);
        let before = f.bytes();
        let result = f.run(basis, "cell.set", json!({"sheet":"sheet:1","cell":"D1","input":input})).await;
        if result.is_err() != refuse || (refuse && f.bytes() != before) {
            failures.push(kind);
        }
        f.shutdown().await;
    }
    assert!(failures.is_empty(), "formula sheet meaning was guarded incorrectly: {failures:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_away_and_name_swap_are_refused_for_writes_by_name() {
    for swap in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        let mut steps = vec![("sheet.rename", json!({"sheet":0,"name":"OldData"}))];
        if swap {
            steps.push(("sheet.rename", json!({"sheet":1,"name":"Data"})));
        }
        f.colleague(&steps);
        let before = f.bytes();
        let error =
            f.run(basis, "cell.set", json!({"sheet":"Data","cell":"B7","input":"42"})).await.expect_err("a name cannot silently name another sheet");
        assert!(error.to_string().contains("changed") || error.to_string().contains("unknown"), "{error}");
        assert_eq!(f.bytes(), before);
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn point_eraser_refuses_a_stroke_moved_out_and_allows_an_unrelated_move() {
    for (id, necessary) in [(3, true), (2, false)] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("drawing.transform", json!({"ids":[id],"dx":200}))]);
        let before = f.bytes();
        let result =
            f.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":"drawing.erase","params":{"x":50,"y":100}}]})).await;
        if necessary {
            let error = result.expect_err("point used to cover a different stroke set");
            assert!(error.to_string().contains("relocated") || error.to_string().contains("changed"), "{error}");
            assert_eq!(f.bytes(), before);
        } else {
            result.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn epoch_premise_and_off_have_different_stale_write_contracts() {
    for (guard, address, refused) in [(Guard::Premise, "B12", false), (Guard::Epoch, "B12", true), (Guard::Off, "B7", false)] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"sheet":"Data","cell":address,"input":"99"}))]);
        let result = f
            .call_guard("run", json!({"basis":basis,"steps":[{"command":"cell.set","params":{"sheet":"Data","cell":"B7","input":"42"}}]}), guard)
            .await;
        assert_eq!(result.is_err(), refused, "{guard:?}: {result:?}");
        if !refused {
            assert_eq!(f.cell("B7"), 42.0);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn deletion_before_an_empty_insertion_window() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("row.delete", json!({"rows":"Data!2:2"}))]);
    // The gap retains its original predecessors through the outside reread.
    let result = f.run(basis, "row.insert", json!({"rows":"Data!7:7"})).await;
    assert!(result.is_err(), "SDK accepted deletion before the observed gap");
    let error = result.unwrap_err();
    assert!(error.to_string().contains("relocated"), "{error}");
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn move_out_before_an_empty_insertion_window() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("edit.moveCells", json!({"from":"Data!2:2","to":"Data!12:12","insert":"down"}))]);
    let result = f.run(basis, "row.insert", json!({"rows":"Data!7:7"})).await;
    assert!(result.is_err(), "SDK accepted moving a row out before the observed gap");
    let error = result.unwrap_err();
    assert!(error.to_string().contains("relocated"), "{error}");
    f.shutdown().await;
}

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn point_facts(bytes: &[u8]) -> Value {
    let doc = SheetDocument::open("Book.xlsx", bytes).unwrap();
    let objects = doc.under("drawing.erase", &json!({"x":50,"y":100})).unwrap().unwrap().objects;
    let values = doc.derived(&[json!("sheet:1")], &["ink".into()]).unwrap();
    let strokes: Vec<_> = values[0].values["ink"].as_array().unwrap().iter().filter(|v| objects.contains(&v["id"])).cloned().collect();
    json!({"objects":objects,"strokes":strokes})
}

#[tokio::test(flavor = "multi_thread")]
async fn fifty_unrelated_changes_table() {
    for guard in [Guard::Premise, Guard::Epoch] {
        let f = Fixture::new();
        let mut refused = 0;
        for round in 0..50 {
            let basis = f.basis().await;
            f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"B12","input":format!("colleague{round}")}))]);
            let result = f
                .call_guard(
                    "run",
                    json!({"basis":basis,"steps":[{"command":"cell.set","params":{"sheet":"Data","cell":"B7","input":format!("agent{round}")}}]}),
                    guard,
                )
                .await;
            refused += usize::from(result.is_err());
        }
        println!("unrelated | {guard:?} | rounds 50 | applied {} | refused {refused}", 50 - refused);
        assert_eq!(refused, if guard == Guard::Epoch { 50 } else { 0 });
        f.shutdown().await;
    }
}

// Necessity rules, fixed before the first run:
// id: drawing:1 must exist and retain its text; position/fill/other objects are unrelated.
// index: sheet index 0 / B7 must name the same cell and retain its entry and parsing style.
// point: the erased stroke set at sheet:1 (50,100) and its held stroke values must hold.
// structural: Data!7:7 is an insertion gap; earlier membership changes and a changed sheet
// name are necessary refusals, while cell text, drawings and changes after the gap are unrelated.
const CHANGES: &[(&str, &str, &str, [bool; 4])] = &[
    ("other-cell", "cell.set", r#"{"sheet":"Data","cell":"B12","input":"99"}"#, [false, false, false, false]),
    ("target-cell", "cell.set", r#"{"sheet":"Data","cell":"B7","input":"99"}"#, [false, true, false, false]),
    ("insert-before", "row.insert", r#"{"rows":"Data!2:2"}"#, [false, true, true, true]),
    ("insert-after", "row.insert", r#"{"rows":"Data!11:11"}"#, [false, false, false, false]),
    ("target-text", "drawing.text", r#"{"id":1,"text":"theirs"}"#, [true, false, false, false]),
    ("move-ink", "drawing.transform", r#"{"ids":[3],"dx":200}"#, [false, false, true, false]),
    ("rename-target", "sheet.rename", r#"{"sheet":0,"name":"OldData"}"#, [false, false, false, true]),
    ("rename-other", "sheet.rename", r#"{"sheet":1,"name":"Journal"}"#, [false, false, false, false]),
];

#[tokio::test(flavor = "multi_thread")]
async fn fifty_rounds_mixed_table_with_fixed_necessity_rules() {
    let forms = [
        ("id", "drawing.text", json!({"id":"drawing:1","text":"mine"})),
        ("index", "cell.set", json!({"sheet":0,"cell":"B7","input":"42"})),
        ("point", "drawing.erase", json!({"x":50,"y":100})),
        ("structural", "row.insert", json!({"rows":"Data!7:7"})),
    ];
    // rounds, applied, necessary refusal, false refusal, wrong application, kit failure.
    let mut table = vec![vec![[0usize; 6]; 4]; CHANGES.len()];
    let mut seed = 0x9e3779b97f4a7c15u64;
    for round in 0..50 {
        let kind = if round < CHANGES.len() { round } else { (next(&mut seed) % CHANGES.len() as u64) as usize };
        let (_, command, params, necessary) = CHANGES[kind];
        for (form, (name, write, params_write)) in forms.iter().enumerate() {
            let f = Fixture::new();
            let basis = f.basis().await;
            let point_before = point_facts(&f.bytes());
            f.colleague(&[(command, serde_json::from_str(params).unwrap())]);
            // Evaluate the predeclared point rule from facts. Absolute ink does not move
            // when rows are inserted, so an earlier truth-table shortcut was incorrect.
            let needs_refusal = if *name == "point" { point_before != point_facts(&f.bytes()) } else { necessary[form] };
            let answer = f.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":write,"params":params_write}]})).await;
            let row = &mut table[kind][form];
            row[0] += 1;
            match answer {
                Ok(_) => row[if needs_refusal { 4 } else { 1 }] += 1,
                Err(error) if error.to_string().contains("conflict") => row[if needs_refusal { 2 } else { 3 }] += 1,
                Err(error) => {
                    row[5] += 1;
                    println!("kit failure {} {name}: {error}", CHANGES[kind].0);
                }
            }
            f.shutdown().await;
        }
    }
    println!("mixed seed 0x9e3779b97f4a7c15; change | form | rounds | applied | necessary | false | wrong | kit-failed");
    for (kind, rows) in table.iter().enumerate() {
        for (form, row) in rows.iter().enumerate() {
            println!("{} | {} | {} | {} | {} | {} | {} | {}", CHANGES[kind].0, forms[form].0, row[0], row[1], row[2], row[3], row[4], row[5]);
        }
    }
    std::fs::write("../../target/c8-land13-mixed.json", serde_json::to_vec_pretty(&table).unwrap()).unwrap();
    assert_eq!(table.iter().flatten().map(|r| r[3] + r[4] + r[5]).sum::<usize>(), 0, "false refusals, wrong writes and kit failures must be zero");
}

// New forms only; these rules were written before their first execution.
// formatting: Data!A7:B7 must retain its entries/styles and position; the named sheet
// must retain its name. Other rows, drawings and the other sheet are unrelated.
// paste: B7:C8 on persistent sheet:1 must retain destination entries/styles and position.
// The target sheet's name is unrelated because the call uses its persistent id.
// sort: Data!A5:B8 must retain its entries/styles, geometry and named-sheet resolution.
// named: Destination follows its existing cell identity across insertion and sheet rename;
// only a changed target entry among CHANGES requires refusal.
const FAMILY_NECESSARY: [[bool; 4]; 8] = [
    [false, false, false, false], // other-cell
    [true, true, true, true],     // target-cell
    [true, true, true, false],    // insert-before
    [false, false, false, false], // insert-after
    [false, false, false, false], // target-text
    [false, false, false, false], // move-ink
    [true, false, true, false],   // rename-target
    [false, false, false, false], // rename-other
];

#[tokio::test(flavor = "multi_thread")]
async fn fifty_rounds_added_families_with_fixed_necessity_rules() {
    let forms = [
        ("formatting", "format.bold", json!({"range":"Data!A7:B7","on":true})),
        ("paste", "edit.paste", json!({"at":"B7","text":"42\t43\n44\t45"})),
        ("sort", "data.sort", json!({"range":"Data!A5:B8","header":false,"keys":[{"column":"B","order":"desc"}]})),
        ("named", "cell.set", json!({"name":"Destination","input":"42"})),
    ];
    let mut table = vec![vec![[0usize; 6]; 4]; CHANGES.len()];
    let mut seed = 0x9e3779b97f4a7c15u64;
    for round in 0..50 {
        let kind = if round < CHANGES.len() { round } else { (next(&mut seed) % CHANGES.len() as u64) as usize };
        let (_, command, params, _) = CHANGES[kind];
        for (form, (name, write, params_write)) in forms.iter().enumerate() {
            let f = Fixture::new();
            f.colleague(&[("name.define", json!({"name":"Destination","refersTo":"=Data!$B$7"}))]);
            let basis = f.basis().await;
            f.colleague(&[(command, serde_json::from_str(params).unwrap())]);
            let before = f.bytes();
            let answer = f.call("run", json!({"basis":basis,"target":{"sheet":"sheet:1"},"steps":[{"command":write,"params":params_write}]})).await;
            let row = &mut table[kind][form];
            row[0] += 1;
            match answer {
                Ok(_) => {
                    row[if FAMILY_NECESSARY[kind][form] { 4 } else { 1 }] += 1;
                    if *name == "named" {
                        let mut saved = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
                        let sheet = if kind == 6 { "OldData" } else { "Data" };
                        let at = if kind == 2 { "B8" } else { "B7" };
                        assert_eq!(saved.execute("cell.get", json!({"sheet":sheet,"cell":at})).unwrap()["value"], 42.0);
                    }
                }
                Err(error) if error.to_string().contains("conflict") => {
                    row[if FAMILY_NECESSARY[kind][form] { 2 } else { 3 }] += 1;
                    assert_eq!(f.bytes(), before);
                }
                Err(error) => {
                    row[5] += 1;
                    println!("kit failure {} {name}: {error}", CHANGES[kind].0);
                }
            }
            f.shutdown().await;
        }
    }
    println!("added families seed 0x9e3779b97f4a7c15; change | form | rounds | applied | necessary | false | wrong | kit-failed");
    for (kind, rows) in table.iter().enumerate() {
        for (form, row) in rows.iter().enumerate() {
            println!("{} | {} | {} | {} | {} | {} | {} | {}", CHANGES[kind].0, forms[form].0, row[0], row[1], row[2], row[3], row[4], row[5]);
        }
    }
    std::fs::write("../../target/c8-sheet-3-mixed-families.json", serde_json::to_vec_pretty(&table).unwrap()).unwrap();
    assert_eq!(table.iter().flatten().map(|r| r[3] + r[4] + r[5]).sum::<usize>(), 0, "false refusals, wrong writes and kit failures must be zero");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_cell_is_refused_but_an_unrelated_cell_is_allowed() {
    for (address, conflict) in [("B7", true), ("B12", false)] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[("cell.set", json!({"sheet":"Data","cell":address,"input":"99"}))]);
        let before = f.bytes();
        let ran = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"B7","input":"42"})).await;
        if conflict {
            let error = ran.expect_err("overwriting an observed value must conflict");
            assert!(error.to_string().contains("changed"), "{error}");
            assert_eq!(f.bytes(), before);
        } else {
            let ran = ran.unwrap_or_else(|e| panic!("unrelated {address}: {e}"));
            assert_eq!(f.cell("B7"), 42.0);
            assert_eq!(f.cell(address), 99.0);
            assert_eq!(ran["others"]["by"], json!(["outside"]), "{ran}");
            for list in ["created", "moved", "deleted"] {
                assert_eq!(ran["others"][list], json!([]), "{ran}");
            }
            let changed = ran["others"]["changed"].as_array().unwrap();
            assert_eq!(changed.len(), 1, "private layout guards are not public changed objects: {ran}");
            assert_eq!(changed[0]["id"], "cell:1!B12", "{ran}");
            assert_eq!(changed[0]["fields"], json!(["input"]), "{ran}");
            assert_eq!(changed[0]["edits"].as_array().unwrap().len(), 1, "{ran}");
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn creating_the_previously_empty_destination_refuses_a_stale_cell_write() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"Z20","input":"99"}))]);
    let before = f.bytes();
    let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"Z20","input":"42"})).await;
    assert!(result.is_err(), "the destination was empty at basis: {result:?}");
    assert_eq!(f.bytes(), before);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn formula_text_holds_referenced_values_and_coordinate_windows() {
    for (command, params, refused) in [
        ("row.insert", json!({"rows":"Data!2:2"}), true),
        ("row.insert", json!({"rows":"Data!6:6"}), true),
        ("row.insert", json!({"rows":"Data!11:11"}), false),
        ("cell.set", json!({"sheet":"Data","cell":"B5","input":"99"}), true),
        ("cell.set", json!({"sheet":"Data","cell":"Z20","input":"99"}), false),
    ] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[(command, params.clone())]);
        let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":"=SUM(B2:B9)"})).await;
        assert_eq!(result.is_err(), refused, "{command} {params}: {result:?}");
        if refused && command == "row.insert" {
            assert!(result.unwrap_err().to_string().contains("relocated"));
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_formula_using_a_deleted_or_redefined_name_is_refused() {
    for (command, params) in [("name.delete", json!({"name":"Revenue"})), ("name.define", json!({"name":"Revenue","refersTo":"Data!B2:B5"}))] {
        let f = Fixture::new();
        f.colleague(&[("name.define", json!({"name":"Revenue","refersTo":"Data!B2:B9"}))]);
        let basis = f.basis().await;
        f.colleague(&[(command, params)]);
        let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":"=SUM(Revenue)"})).await;
        assert!(result.is_err(), "{command}: {result:?}");
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn formula_dependencies_follow_names_and_other_formulas_but_ignore_other_names() {
    for (formula, other, refused) in [("=SUM(Revenue)", "B5", true), ("=D2", "B5", true), ("=SUM(Revenue)", "name", false)] {
        let f = Fixture::new();
        f.colleague(&[
            ("name.define", json!({"name":"Revenue","refersTo":"Data!B2:B9"})),
            ("name.define", json!({"name":"Expenses","refersTo":"Data!B10:B12"})),
            ("cell.set", json!({"sheet":"Data","cell":"D2","input":"=SUM(B2:B9)"})),
        ]);
        let basis = f.basis().await;
        if other == "name" {
            f.colleague(&[("name.define", json!({"name":"Expenses","refersTo":"Data!B10:B11"}))]);
        } else {
            f.colleague(&[("cell.set", json!({"sheet":"Data","cell":other,"input":"99"}))]);
        }
        let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"D1","input":formula})).await;
        assert_eq!(result.is_err(), refused, "{formula}, {other}: {result:?}");
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn recalculated_values_are_counted_in_the_receipt() {
    let f = Fixture::new();
    f.colleague(&[
        ("cell.set", json!({"sheet":"Data","cell":"D1","input":"=B7*2"})),
        ("cell.set", json!({"sheet":"Data","cell":"D2","input":"=D1+1"})),
        ("cell.set", json!({"sheet":"Data","cell":"D3","input":"=B7*0"})),
    ]);
    let basis = f.basis().await;
    let result = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"B7","input":"42"})).await.unwrap();
    let changes = result["changes"]["changed"].as_array().unwrap();
    let carried: Vec<_> = changes.iter().filter_map(|change| change.get("carries")).collect();
    assert_eq!(carried, vec![&json!({"count":2,"what":"computed cell values"})], "{result}");
    assert_eq!(f.cell("D1"), 84.0);
    assert_eq!(f.cell("D2"), 85.0);
    assert_eq!(f.cell("D3"), 0.0);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_move_holds_child_position_but_not_child_text() {
    for (write, params, refused) in
        [("drawing.text", json!({"id":"drawing:1","text":"mine"}), false), ("drawing.transform", json!({"ids":["drawing:1"],"dx":10}), true)]
    {
        let f = Fixture::new();
        f.colleague(&[("drawing.group", json!({"ids":[1,2]}))]);
        let grouped = SheetDocument::open("Book.xlsx", &f.bytes()).unwrap();
        let group = grouped.inspect().unwrap()["workbook"]["sheets"][0]["drawings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|drawing| drawing["kind"] == "group")
            .unwrap()["toolId"]
            .clone();
        let basis = f.basis().await;
        f.colleague(&[("drawing.transform", json!({"ids":[group],"dx":100}))]);
        let before = f.bytes();
        let answer = f.run(basis, write, params).await;
        assert_eq!(answer.is_err(), refused, "{write}: {answer:?}");
        if refused {
            let error = answer.unwrap_err();
            assert!(error.to_string().contains("relocated") || error.to_string().contains("changed"), "{error}");
            assert_eq!(f.bytes(), before);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_insert_before_b7_changes_what_the_address_names() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.colleague(&[("row.insert", json!({"rows":"Data!2:2"}))]);
    let before = f.bytes();
    let error = f.run(basis, "cell.set", json!({"sheet":"Data","cell":"B7","input":"42"})).await.expect_err("B7 now names the former B6");
    assert!(error.to_string().contains("changed") || error.to_string().contains("relocated"), "{error}");
    assert_eq!(f.bytes(), before);
    assert_eq!(f.cell("B8"), 7.0);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn blank_axes_do_not_hide_insertions_before_a_structural_gap() {
    for (command, before, stale) in [
        ("row.insert", json!({"rows":"Data!2:2"}), json!({"rows":"Data!7:7"})),
        ("column.insert", json!({"cols":"Data!B:B"}), json!({"cols":"Data!G:G"})),
        ("sheet.insert", json!({"name":"Before","before":0}), json!({"name":"Stale","before":1})),
    ] {
        let f = Fixture::new();
        f.colleague(&[("cell.set", json!({"sheet":"Data","cell":"J12","input":"boundary"}))]);
        let basis = f.basis().await;
        f.colleague(&[(command, before)]);
        let bytes = f.bytes();
        let result = f.run(basis, command, stale).await;
        assert!(result.as_ref().is_err_and(|error| error.to_string().contains("relocated")), "{command}: {result:?}");
        assert_eq!(f.bytes(), bytes);
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn text_by_id_holds_text_and_survives_a_move_but_not_a_text_change_or_deletion() {
    for (command, params, conflict) in [
        ("drawing.transform", json!({"ids":[1],"dx":100}), false),
        ("drawing.text", json!({"id":1,"text":"theirs"}), true),
        ("drawing.delete", json!({"ids":[1]}), true),
    ] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.colleague(&[(command, params)]);
        let before = f.bytes();
        let ran = f.run(basis, "drawing.text", json!({"id":"drawing:1","text":"mine"})).await;
        if conflict {
            let error = ran.expect_err("the held text or object disappeared");
            assert!(error.to_string().contains(if command == "drawing.delete" { "deleted" } else { "changed" }), "{error}");
            assert_eq!(f.bytes(), before);
        } else {
            ran.unwrap_or_else(|e| panic!("text by id survives movement: {e}"));
        }
        f.shutdown().await;
    }
}
