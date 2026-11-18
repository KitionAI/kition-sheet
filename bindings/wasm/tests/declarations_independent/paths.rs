//! Invocation coverage for the original blind paths. Successful invocation is a
//! prerequisite to a read comparison, not proof that every prose fact is held.
//! In particular, neither a union over commands nor a successful `under` call is
//! counted here as a complete path comparison.

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Reading {
    pub family: &'static str,
    pub line: usize,
    pub command: &'static str,
    pub params: Value,
    pub holds: &'static str,
    pub reads: &'static str,
    pub note: &'static str,
}

pub(super) fn rows() -> Vec<Reading> {
    [
        ("view_file", super::view_file::READINGS),
        ("data_insert", super::data_insert::READINGS),
        ("edit_format", super::edit_format::READINGS),
        ("objects", super::objects::READINGS),
    ]
    .into_iter()
    .flat_map(|(family, source)| parse(family, source))
    .collect()
}

pub(super) fn parse(family: &'static str, source: &'static str) -> Vec<Reading> {
    source
        .lines()
        .enumerate()
        .filter_map(move |(line, row)| {
            let body = row.strip_prefix("ROW ")?;
            let fields: Vec<_> = body.split(" | ").collect();
            Some(Reading {
                family,
                line: line + 1,
                command: fields[0],
                params: serde_json::from_str(fields[1].strip_prefix("params=").unwrap()).unwrap(),
                holds: fields[3].strip_prefix("holds=").unwrap(),
                reads: fields[4].strip_prefix("reads=").unwrap(),
                note: fields[6].strip_prefix("note=").unwrap(),
            })
        })
        .collect()
}

fn run(doc: &mut SheetDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("fixture {command}: {error}"))
}

fn picture() -> Value {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    json!(kition_sheet_engine::io::base64_encode(bytes.get_ref()))
}

// Each invocation gets a fresh workbook. No earlier row can accidentally create
// the object, clipboard, filter, history entry, etc. needed by a later row.
pub(super) fn fixture(row: &Reading) -> (SheetDocument, Value) {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    // Path saves are engine-only: the agent host owns files. Exercise their
    // actual handler in an unhosted native fixture, with disposable output.
    if !matches!(row.command, "file.save" | "file.saveAs") {
        doc.hosted(&kition_kit::host::Hosting { files_only: true });
    }
    let home = if row.family == "edit_format" { "Data" } else { "Sheet1" };
    if home == "Data" {
        run(&mut doc, "sheet.rename", json!({"name":home}));
    }
    for name in [home, if home == "Data" { "Second" } else { "Sheet2" }, "Sheet3"] {
        if name != home {
            run(&mut doc, "home.insertSheet", json!({"name":name,"before":99}));
        }
        run(
            &mut doc,
            "range.setValues",
            json!({"range":"A1","values":[["Region","Amount","Date"],["East",3,45000],["West",2,45001],["East",1,45002],["West",4,45003]]}),
        );
        run(&mut doc, "cell.set", json!({"cell":"Z20","input":"unrelated sentinel"}));
    }
    run(&mut doc, "sheet.activate", json!({"sheet":home}));
    run(&mut doc, "selection.set", json!({"range":"B3"}));
    let mut params = row.params.clone();
    if row.command == "file.open" {
        params["base64"] = json!(kition_sheet_engine::io::base64_encode(&doc.save(Some("Book.xlsx")).unwrap()));
    }
    if matches!(row.command, "file.save" | "file.saveAs") {
        params["path"] = json!(std::env::temp_dir().join(format!("c8-sheet-path-{}-{}.xlsx", std::process::id(), row.command)));
    }
    if row.command == "insert.picture" || row.command == "pageLayout.background" && params.get("data").is_some() {
        params.as_object_mut().unwrap().remove("data");
        params["base64"] = picture();
    }
    if row.family == "edit_format" {
        run(&mut doc, "formulas.defineName", json!({"name":"Destination","refersTo":"=Second!$B$2"}));
        run(&mut doc, "home.newCellStyle", json!({"name":"Custom"}));
        if row.command == "cell.formatText" {
            run(&mut doc, "cell.set", json!({"cell":"B2","input":"text"}));
            run(&mut doc, "cell.set", json!({"cell":"B3","input":"text"}));
        }
        if row.command == "edit.pasteSpecial" || row.command == "edit.paste" && params.get("text").is_none() && params.get("html").is_none() {
            run(&mut doc, "edit.copy", json!({"range":"A2:B3"}));
        }
        if row.command == "edit.formatPainter" && params.get("apply").is_some() {
            run(&mut doc, "edit.formatPainter", json!({"sticky":true}));
        }
    }
    if row.family == "data_insert" || row.family == "objects" {
        if !matches!(row.command, "insert.table" | "home.formatAsTable") {
            run(&mut doc, "insert.table", json!({"range":"A1:C5","name":"Sales","header":true}));
        }
        for name in ["MyTableStyle", "OldStyle"] {
            run(&mut doc, "table.newStyle", json!({"name":name,"elements":{}}));
        }
    }
    if row.command == "table.removeDuplicates" && row.params.get("range").is_some() {
        // The fallback is selected by absence of a table at the active cell,
        // even though its explicit data range can overlap an existing table.
        run(&mut doc, "selection.set", json!({"range":"H10:J14"}));
        run(&mut doc, "range.setValues", json!({"range":"A2","values":[["East",3,45000],["East",3,45000]]}));
    }
    if row.command == "table.headerRow" && params.get("on") == Some(&json!(true)) {
        run(&mut doc, "table.headerRow", json!({"table":"Sales","on":false}));
    }
    match row.command {
        "view.unfreezePanes" => {
            run(&mut doc, "view.freezePanes", json!({"cell":"C5"}));
        }
        "view.gridlines" if params.get("on").is_some() => {
            run(&mut doc, "view.gridlines", json!({"on":false}));
        }
        "pageLayout.size" => {
            run(&mut doc, "pageLayout.size", json!({"paper":"Letter"}));
        }
        "pageLayout.printArea" if params.get("clear").is_some() => {
            run(&mut doc, "pageLayout.printArea", json!({"range":"A1:C5"}));
        }
        "pageLayout.breaks" if params.get("reset").is_some() => {
            run(&mut doc, "pageLayout.breaks", json!({"cell":"C5"}));
        }
        "review.unprotectSheet" => {
            run(&mut doc, "review.protectSheet", json!({"sheet":params["sheet"],"password":"p"}));
        }
        "review.changeSpelling" => {
            for sheet in ["Sheet1", "Sheet2"] {
                run(&mut doc, "cell.set", json!({"sheet":sheet,"cell":"C3","input":"teh"}));
            }
        }
        "cell.toggleCheckbox" => {
            run(
                &mut doc,
                "insert.checkbox",
                json!({"sheet":params.get("sheet").cloned().unwrap_or(json!(home)),"range":params.get("cell").cloned().unwrap_or(json!("B3"))}),
            );
        }
        "pageLayout.background" if params.get("clear").is_some() => {
            run(&mut doc, "insert.picture", json!({"base64":picture(),"at":"A1","alt":"Sheet background"}));
        }
        "data.ungroup" | "data.hideDetail" | "data.showDetail" => {
            run(&mut doc, "data.group", json!({"rows":"2:4"}));
            run(&mut doc, "data.group", json!({"cols":"B:C"}));
            if row.command == "data.showDetail" {
                run(&mut doc, "data.hideDetail", json!({"rows":"3"}));
            }
        }
        "edit.replace" => {
            run(&mut doc, "cell.set", json!({"cell":"B3","input":"old"}));
        }
        "edit.clearFormats" => {
            run(&mut doc, "home.bold", json!({"range":"B2:C3","on":true}));
        }
        "edit.clearComments" => {
            run(&mut doc, "review.newComment", json!({"cell":"B2","text":"remove me"}));
        }
        "edit.clearHyperlinks" => {
            run(&mut doc, "insert.link", json!({"cell":"B2","target":"https://example.com"}));
        }
        "home.italic" if params.get("on") == Some(&json!(false)) => {
            run(&mut doc, "home.italic", json!({"range":"B2","on":true}));
        }
        "home.borders" if params.get("preset") == Some(&json!("none")) => {
            run(&mut doc, "home.borders", json!({"range":"B2:C3","preset":"outside"}));
        }
        "home.alignBottom" => {
            run(&mut doc, "home.alignTop", json!({"range":"B2"}));
        }
        "home.decreaseIndent" => {
            run(&mut doc, "home.increaseIndent", json!({"range":"B2"}));
        }
        "home.unmergeCells" => {
            run(&mut doc, "home.mergeCells", json!({"range":"B2:C3"}));
        }
        "home.unhideRows" => {
            run(&mut doc, "home.hideRows", json!({"rows":"2"}));
        }
        "home.unhideColumns" => {
            run(&mut doc, "home.hideColumns", json!({"cols":"B"}));
        }
        "sheet.unhide" if params.get("sheet").is_some() => {
            run(&mut doc, "sheet.hide", json!({"sheet":"Data"}));
        }
        "insert.link" if params.get("remove").is_some() => {
            run(&mut doc, "insert.link", json!({"cell":"B2","target":"https://example.com"}));
        }
        "formulas.createFromSelection" if params.as_object().unwrap().is_empty() => {
            run(&mut doc, "selection.set", json!({"range":"A1:C5"}));
        }
        _ => {}
    }
    if row.command.starts_with("pivot.") || row.command == "data.refreshAll" {
        let pivot = run(
            &mut doc,
            "insert.pivotTable",
            json!({"source":"Sales","destination":"H8","name":"Report","rows":if row.command == "pivot.moveField" { json!(["Date","Region"]) } else { json!(["Region","Date"]) },"values":["Amount"]}),
        )["id"]
            .clone();
        if params.get("pivot").is_some_and(Value::is_number) {
            params["pivot"] = pivot.clone();
        }
        if row.command == "pivot.collapse" && params.get("collapse") == Some(&json!(false)) {
            run(&mut doc, "pivot.collapse", json!({"pivot":pivot,"field":"Region","items":["East"],"collapse":true}));
        }
        if matches!(row.command, "pivot.refresh" | "data.refreshAll") {
            run(&mut doc, "cell.set", json!({"cell":"B2","input":"11"}));
        }
    }
    if row.command == "formulas.deleteName" {
        run(&mut doc, "formulas.defineName", json!({"name":"Result","refersTo":"=42"}));
    }
    if row.command == "data.goalSeek" {
        run(&mut doc, "cell.set", json!({"cell":"Sheet2!B5","input":"=Sheet1!B2*2"}));
    }
    if matches!(row.command, "data.smartFill" | "edit.smartFill") {
        run(&mut doc, "edit.clearAll", json!({"range":"A1:D8"}));
        run(
            &mut doc,
            "range.setValues",
            json!({"range":"B1","values":[["Ada Lovelace","Ada"],["Grace Hopper",null],["Alan Turing",null],["Katherine Johnson",null]]}),
        );
        run(&mut doc, "selection.set", json!({"range":"C4"}));
    }
    if row.command == "edit.redo" {
        run(&mut doc, "edit.undo", json!({}));
    }
    if row.command == "sheet.unhide" && params.as_object().unwrap().is_empty() {
        run(&mut doc, "sheet.hide", json!({"sheet":"Sheet3"}));
    }
    if matches!(row.command, "data.scenarioShow" | "data.scenarioDelete" | "data.scenarioSummary") {
        run(&mut doc, "data.scenarioAdd", json!({"name":"Base","changing":"B2:B3","values":[3,2]}));
        if row.command == "data.scenarioShow" {
            run(&mut doc, "cell.set", json!({"cell":"B2","input":"11"}));
        }
    }
    if row.command == "view.customViews" && params.get("save").is_none() {
        run(&mut doc, "view.customViews", json!({"save":"work"}));
        run(&mut doc, "selection.set", json!({"range":"D7"}));
    }
    if row.command == "automate.runScript" || row.command == "automate.deleteScript" {
        run(&mut doc, "automate.newScript", json!({"name":"Work","commands":[{"command":"cell.set","params":{"cell":"H1","input":"42"}}]}));
    }
    if row.command == "automate.recordActions" && params.get("stop").is_some() {
        run(&mut doc, "automate.recordActions", json!({}));
        run(&mut doc, "cell.set", json!({"cell":"H1","input":"42"}));
    }
    if row.command == "home.manageRules" || row.command == "home.clearRules" {
        for range in ["A1:A3", "B1:B3", "C1:C3"] {
            run(&mut doc, "home.conditionalFormat", json!({"range":range,"rule":{"type":"blanks"}}));
        }
    }
    if row.command.starts_with("review.") && row.command.contains("Comment") || row.command == "review.showNote" {
        run(&mut doc, "review.newComment", json!({"cell":"B2","text":"original"}));
        run(&mut doc, "review.newComment", json!({"cell":"B3","text":"original"}));
        run(&mut doc, "review.newComment", json!({"cell":"Sheet2!B2","text":"original"}));
    }
    let object_path = row.family == "objects"
        && (row.command.starts_with("chart.")
            || row.command.starts_with("arrange.")
            || row.command.starts_with("drawing.")
            || row.command == "draw.inkToShape")
        || row.family == "data_insert" && (row.command.starts_with("chart.") || row.command.starts_with("object.") || row.command == "shape.setText");
    if object_path {
        let ink = run(&mut doc, "draw.stroke", json!({"points":[[10,10],[80,40]],"color":"#123456","width":2}))["shape"].clone();
        let first = run(&mut doc, "insert.textBox", json!({"at":"H1","text":"old"}))["shape"].clone();
        let second = run(&mut doc, "insert.textBox", json!({"at":"J1","text":"second"}))["shape"].clone();
        if row.command == "drawing.arrange" {
            run(&mut doc, "insert.textBox", json!({"at":"L1","text":"third"}));
        }
        let image = if params.get("kind") == Some(&json!("image")) {
            Some(run(&mut doc, "insert.picture", json!({"base64":picture(),"at":"L1"}))["image"].clone())
        } else {
            None
        };
        let chart_source = if row.command == "drawing.chart" && params.get("series").is_some() {
            run(&mut doc, "range.setValues", json!({"range":"J10","values":[["Name","Sales"],["One",1],["Two",2]]}));
            "J10:K12"
        } else {
            "A1:C5"
        };
        let chart = run(
            &mut doc,
            "insert.chart",
            json!({"range":chart_source,"type":if params.get("palette").is_some() { "pie" } else { "column" }}),
        )["chart"]
            .clone();
        let connector = if row.command == "drawing.reconnect" {
            Some(run(&mut doc, "drawing.connect", json!({"from":first,"to":second}))["id"].clone())
        } else {
            None
        };
        let group =
            if row.command == "drawing.ungroup" { Some(run(&mut doc, "drawing.group", json!({"ids":[first,second]}))["id"].clone()) } else { None };
        if params.get("chart").is_some_and(Value::is_number) {
            params["chart"] = chart.clone();
        }
        if params.get("id").is_some() {
            params["id"] = if row.command == "drawing.chart" {
                chart
            } else if row.command == "draw.inkToShape" {
                ink
            } else if let Some(image) = image {
                image
            } else {
                connector.unwrap_or(first.clone())
            };
        }
        if params.get("ids").is_some() {
            params["ids"] = group.map_or_else(|| json!([first, second]), |group| json!([group]));
        }
        if row.command == "drawing.connect" {
            params["from"] = first;
            params["to"] = second;
        } else if row.command == "drawing.reconnect" && params.get("to").is_some() {
            params["to"] = second;
        }
    }
    doc.version().unwrap();
    (doc, params)
}

#[test]
fn blind_paths_have_distinct_invocations_and_report_unreached_samples() {
    let rows = rows();
    // Five replacement branches added under same-field decision.
    assert_eq!(rows.len(), 430);
    let mut keys = BTreeSet::new();
    let mut writes = BTreeSet::new();
    let mut invoked = BTreeSet::new();
    let mut counts = BTreeMap::<&str, [usize; 3]>::new();
    let mut failures = BTreeSet::new();
    let mut target_differences = BTreeSet::new();
    for row in rows {
        let key = format!("{}|{}|{}", row.family, row.command, row.params);
        assert!(keys.insert(key.clone()), "duplicate blind path {key}");
        let (mut doc, params) = fixture(&row);
        let command = doc.commands().into_iter().find(|command| command.id == row.command).unwrap();
        let count = counts.entry(row.family).or_default();
        count[0] += 1;
        if !command.mutates {
            count[1] += 1;
            continue;
        }
        writes.insert(row.command);
        let wanted_targets: BTreeSet<_> =
            super::all_commands::expected_metadata(&command).into_iter().filter_map(|fact| fact.strip_prefix("target:").map(str::to_owned)).collect();
        let actual_targets: BTreeSet<_> = command.targets.into_iter().map(str::to_owned).collect();
        if wanted_targets != actual_targets {
            target_differences.insert(key.clone());
            eprintln!(
                "PATH_TARGET_FACT {}:{} {} LESS {:?} MORE {:?}; branch evidence extra.rs::shape_fill -> with_shape",
                row.family,
                row.line,
                row.command,
                wanted_targets.difference(&actual_targets),
                actual_targets.difference(&wanted_targets)
            );
        }
        let before = doc.save(Some("Book.json")).unwrap();
        let editor_before = doc.editor_state();
        let under = doc.under(row.command, &params);
        let result = doc.execute(row.command, params.clone());
        if row.command == "cell.toggleCheckbox" {
            assert_eq!(result.as_ref().unwrap()["toggled"], 1, "the fixture must contain a literal checkbox");
        }
        if row.command == "table.removeDuplicates" && row.params.get("range").is_some() {
            assert_eq!(result.as_ref().unwrap()["removed"], 1, "the fallback must process its explicit block");
        }
        if row.command == "formulas.autoSum" && row.params.get("enter") == Some(&json!(false)) {
            assert_eq!(result.as_ref().unwrap()["formula"], "=SUM(B2:B4)");
            assert_eq!(doc.save(Some("Book.json")).unwrap(), before, "preview must not enter the formula");
        }
        if result.is_ok() && doc.save(Some("Book.json")).ok().as_ref() == Some(&before) && doc.editor_state() == editor_before {
            eprintln!("PATH_NO_CHANGE {}:{} {} params={} result={result:?}; {}", row.family, row.line, row.command, params, row.note);
        }
        if matches!(row.command, "file.save" | "file.saveAs") {
            let path = params["path"].as_str().unwrap();
            if result.is_ok() {
                assert!(std::fs::metadata(path).unwrap().len() > 0);
                std::fs::remove_file(path).unwrap();
            }
        }
        if let Err(error) = result {
            failures.insert(key);
            eprintln!(
                "PATH_UNREACHED {}:{} {} original={} fixture={} error={error}; source={}",
                row.family, row.line, row.command, row.params, params, row.note
            );
        } else if let Err(error) = under {
            failures.insert(key);
            eprintln!("PATH_UNDER_ERROR {}:{} {}: {error}", row.family, row.line, row.command);
        } else {
            count[2] += 1;
            invoked.insert(row.command);
        }
    }
    assert_eq!(writes.len(), 261, "binding-only commandLine is separate from the blind engine rows");
    eprintln!("PATH_INVOCATIONS family => [raw, nonwrite, successful write invocation]: {counts:?}");
    eprintln!("PATH_COMMANDS successfully invoked {}/{}, absent={:?}", invoked.len(), writes.len(), writes.difference(&invoked).collect::<Vec<_>>());
    eprintln!("PATH_UNREACHED_KEYS {failures:#?}");
    let allowed: BTreeSet<_> = UNREACHED.iter().map(|key| (*key).to_owned()).collect();
    assert!(failures.is_subset(&allowed), "new unreached paths: {:?}", failures.difference(&allowed).collect::<Vec<_>>());
    assert!(failures.is_empty(), "every blind write needs an executable fixture: {failures:#?}");
    let allowed_targets = BTreeSet::new();
    assert!(target_differences.is_subset(&allowed_targets), "new path target mismatches: {target_differences:?}");
    eprintln!(
        "PATH_TARGET_FACTS: {}/380 paths differ, {}/261 commands; LESS/MORE target path class; residual={target_differences:?}",
        target_differences.len(),
        usize::from(!target_differences.is_empty())
    );
    // This gate measures invocation coverage only. A successful invocation still
    // needs branch assertions and a complete independent fact oracle.
}

// The invocation floor is now zero and cannot grow.
const UNREACHED: &[&str] = &[];
