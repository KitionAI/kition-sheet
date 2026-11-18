//! `kition-sheet` served headless keeps the editor's state (the active sheet, the selection, the
//! cell being typed, the selected drawing object) out of what an agent reads of the workbook, and
//! sets it for one call and puts it back through the Kit Process Protocol's `context.get`,
//! `context.set` and `context.restore` (section 7a), so nothing an agent does depends on or leaves
//! behind the person's active sheet or selection (task card C4/C5/C6-sheet).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_sheet_kit::SheetDocument;
use serde_json::{Map, Value, json};

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The headless kit as the `kition-sheet` executable makes it.
fn kit() -> Headless<SheetDocument> {
    Headless::<SheetDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_sheet_kit::EDITOR_STATE).unwrap(),
    )
}

/// A folder holding `Book.xlsx` with the sheets `Data`, `Notes` and `Summary`, a few values on
/// each, `Data` active; the kit opens it.
fn open(name: &str) -> (Headless<SheetDocument>, String, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("kition-sheet-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut book = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    book.execute("sheet.rename", json!({ "name": "Data" })).unwrap();
    book.execute("range.setValues", json!({ "range": "A1", "values": [["Region", "Sales"], ["North", 120], ["South", 80]] })).unwrap();
    for name in ["Notes", "Summary"] {
        book.execute("home.insertSheet", json!({ "name": name, "before": 9 })).unwrap();
        book.execute("range.setValues", json!({ "range": "A1", "values": [[name]] })).unwrap();
    }
    book.execute("sheet.activate", json!({ "sheet": "Data" })).unwrap();
    std::fs::write(root.join("Book.xlsx"), book.save(None).unwrap()).unwrap();
    let mut kit = kit();
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("Book.xlsx").unwrap(), read_only: false, create: None }).unwrap().id;
    (kit, id, root)
}

fn run(kit: &mut Headless<SheetDocument>, id: &str, command: &str, value: Value, origin: Origin) -> Value {
    kit.execute(id, command, params(value), origin).unwrap_or_else(|error| panic!("{command}: {error:?}")).result.unwrap_or(Value::Null)
}

#[test]
fn what_an_agent_reads_of_the_workbook_leaves_out_the_editors_state() {
    let (mut kit, id, root) = open("reads");
    let before = kit.describe(&id).unwrap().state;
    // The person goes to another sheet, selects a range there, and starts the format painter.
    run(&mut kit, &id, "sheet.activate", json!({ "sheet": "Summary" }), Origin::User);
    run(&mut kit, &id, "selection.set", json!({ "range": "B2:C4" }), Origin::User);
    let moved = kit.describe(&id).unwrap().state;
    let shown = Value::Object(moved.clone());
    assert_eq!(moved, before, "the active sheet and the selection changed what the agent reads of the workbook");
    for key in kition_sheet_kit::EDITOR_STATE {
        assert!(shown.pointer(key).is_none(), "{key} is the editor's, not the workbook's: {shown}");
    }
    let _ = std::fs::remove_dir_all(root);
}

/// The sheets as the agent reads them: `(id, name)` in tab order.
fn sheets(kit: &mut Headless<SheetDocument>, id: &str) -> Vec<(u64, String)> {
    let state = kit.describe(id).unwrap().state;
    state["workbook"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sheet| (sheet["id"].as_u64().unwrap(), sheet["name"].as_str().unwrap().to_owned()))
        .collect()
}

fn sheet_id(kit: &mut Headless<SheetDocument>, id: &str, name: &str) -> u64 {
    sheets(kit, id).into_iter().find(|(_, n)| n == name).unwrap().0
}

fn read(kit: &mut Headless<SheetDocument>, id: &str, sheet: &str, range: &str) -> Value {
    run(kit, id, "sheet.read", json!({ "sheet": sheet, "range": range }), Origin::Agent)["values"].clone()
}

#[test]
fn every_sheet_is_listed_with_its_id_name_and_position_and_read_by_its_tool_id() {
    let (mut kit, id, root) = open("listed");
    let state = Value::Object(kit.describe(&id).unwrap().state);
    let listed = state["workbook"]["sheets"].as_array().unwrap();
    assert_eq!(listed.iter().map(|sheet| sheet["name"].clone()).collect::<Vec<_>>(), [json!("Data"), json!("Notes"), json!("Summary")]);
    for (index, sheet) in listed.iter().enumerate() {
        assert_eq!(sheet["index"], json!(index));
        assert_eq!(sheet["toolId"], json!(format!("sheet:{}", sheet["id"])), "{sheet}");
    }
    let saved = kit.context_get(&id).unwrap();
    let notes = format!("sheet:{}", sheet_id(&mut kit, &id, "Notes"));
    assert_eq!(read(&mut kit, &id, &notes, "A1"), json!([["Notes"]]), "sheet.read takes a sheet's tool id");
    assert_eq!(kit.context_get(&id).unwrap(), saved, "reading another sheet activates nothing");
    let _ = std::fs::remove_dir_all(root);
}

/// A run as `kition-kits` makes it over the Kit Process Protocol (section 7a): `context.get` before
/// the first step, `context.set` with each step's `on` and `at.part`, `context.restore` after the
/// last. Steps land on the sheets they name; the person's sheet and selection are as they were.
#[test]
fn a_run_sets_each_steps_sheet_and_selection_and_puts_the_editor_back() {
    let (mut kit, id, root) = open("run");
    // The person is on Data with A2:B3 selected.
    run(&mut kit, &id, "selection.set", json!({ "range": "A2:B3" }), Origin::User);
    let saved = kit.context_get(&id).unwrap();
    let (notes, summary) = (sheet_id(&mut kit, &id, "Notes"), sheet_id(&mut kit, &id, "Summary"));
    // Step 1: bold on Notes' B2:C3 through `on` (`home.bold` reads the selection).
    kit.context_set(&id, &params(json!({ "part": { "sheet": notes }, "on": ["B2:C3"] }))).unwrap();
    run(&mut kit, &id, "range.setValues", json!({ "range": "B2", "values": [["x", "y"], ["z", "w"]] }), Origin::Agent);
    run(&mut kit, &id, "home.bold", json!({ "on": true }), Origin::Agent);
    // Step 2: values on Summary (`at.part` by tool id).
    kit.context_set(&id, &params(json!({ "part": { "sheet": format!("sheet:{summary}") } }))).unwrap();
    run(&mut kit, &id, "range.setValues", json!({ "range": "A5", "values": [["Total", 200]] }), Origin::Agent);
    kit.context_restore(&id, &saved).unwrap();
    assert_eq!(kit.context_get(&id).unwrap(), saved, "the editor is where the person left it");
    assert_eq!(read(&mut kit, &id, "Notes", "B2:C3"), json!([["x", "y"], ["z", "w"]]));
    assert_eq!(read(&mut kit, &id, "Summary", "A5:B5"), json!([["Total", 200.0]]));
    assert_eq!(read(&mut kit, &id, "Data", "A5:B5"), json!([[null, null]]), "nothing landed on the person's sheet");
    let bold = run(&mut kit, &id, "cell.get", json!({ "sheet": "Notes", "cell": "C3" }), Origin::Agent);
    assert!(bold.to_string().contains("\"bold\":true"), "Notes!C3 is bold: {bold}");
    let unbold = run(&mut kit, &id, "cell.get", json!({ "sheet": "Data", "cell": "B3" }), Origin::Agent);
    assert!(!unbold.to_string().contains("\"bold\":true"), "the person's selection was not formatted: {unbold}");
    assert_eq!(kit.context_get(&id).unwrap(), saved, "reading by sheet moved nothing");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_context_the_workbook_lacks_is_invalid_params_and_says_where_ids_come_from() {
    let (mut kit, id, root) = open("refused");
    let saved = kit.context_get(&id).unwrap();
    let notes = sheet_id(&mut kit, &id, "Notes");
    for (given, says) in [
        (json!({ "part": { "sheet": 999 } }), "no sheet has id 999"),
        (json!({ "part": { "sheet": "Nowhere" } }), "no sheet named `Nowhere`"),
        (json!({ "part": { "slide": 1 } }), "names no part of a workbook"),
        (json!({ "part": { "sheet": notes }, "on": ["B2:ZZZZ9"] }), "neither a range in A1 notation"),
        (json!({ "part": { "sheet": notes }, "on": ["B2", "drawing:4"] }), "mixes cells and drawing objects"),
        (json!({ "part": { "sheet": notes }, "on": ["Summary!B2"] }), "the call's part is sheet `Notes`"),
        (json!({ "on": ["B2"] }), "an address means something only on its sheet"),
        (json!({ "on": ["drawing:77"] }), "no drawing object `drawing:77`"),
        (json!({ "part": { "sheet": notes }, "on": [true] }), "an item is a range"),
    ] {
        let error = kit.context_set(&id, &params(given.clone())).unwrap_err();
        assert_eq!(error.code, kition_kit_server::ErrorReason::InvalidParams.code(), "{given}: {}", error.message);
        assert!(error.message.contains(says) || error.data.as_ref().is_some_and(|data| data.to_string().contains(says)), "{given}: {error:?}");
        assert_eq!(kit.context_get(&id).unwrap(), saved, "{given} changed the editor");
    }
    let error = kit.context_set(&id, &params(json!({ "time": 1.5 }))).unwrap_err();
    assert_eq!(error.code, kition_kit_server::ErrorReason::CommandDisabled.code(), "a member the kit cannot set: {error:?}");
    assert_eq!(kit.context_get(&id).unwrap(), saved);
    let _ = std::fs::remove_dir_all(root);
}

/// An agent's command that names its sheet or range acts there and leaves the person's sheet and
/// selection as they were, also when it fails; a navigation command keeps its effect, and a sheet
/// the agent inserts is where its next steps land when they name none.
#[test]
fn an_agents_command_given_its_sheet_moves_nothing() {
    let (mut kit, id, root) = open("named");
    run(&mut kit, &id, "selection.set", json!({ "range": "B2" }), Origin::User);
    let saved = kit.context_get(&id).unwrap();
    run(&mut kit, &id, "home.bold", json!({ "range": "Summary!A1:B2", "on": true }), Origin::Agent);
    run(&mut kit, &id, "range.setValues", json!({ "range": "Notes!C1", "values": [[1]] }), Origin::Agent);
    let notes = format!("sheet:{}", sheet_id(&mut kit, &id, "Notes"));
    run(&mut kit, &id, "home.italic", json!({ "sheet": notes, "range": "C1", "on": true }), Origin::Agent);
    // A chart of Summary's cells is made on Summary, and the person's selection stays: the
    // insert selects what it made only for the agent's call.
    run(&mut kit, &id, "range.setValues", json!({ "range": "Summary!A2", "values": [["x", 1], ["y", 2]] }), Origin::Agent);
    run(&mut kit, &id, "drawing.insert", json!({ "kind": "chart", "range": "Summary!A1:B3" }), Origin::Agent);
    let state = Value::Object(kit.describe(&id).unwrap().state);
    let on = |name: &str| state["workbook"]["sheets"].as_array().unwrap().iter().find(|sheet| sheet["name"] == name).unwrap()["drawings"].clone();
    assert_eq!(on("Summary").as_array().map(Vec::len), Some(1), "the chart is on the sheet its range names: {state}");
    assert_eq!(on("Data"), json!([]), "{state}");
    assert_eq!(kit.context_get(&id).unwrap(), saved, "an agent's command given its sheet moved the editor");
    let failed = kit.execute(&id, "home.bold", params(json!({ "range": "Nowhere!A1", "on": true })), Origin::Agent);
    assert!(failed.is_err(), "a sheet the workbook lacks is refused");
    assert_eq!(kit.context_get(&id).unwrap(), saved, "a failed command moved the editor");
    let italic = run(&mut kit, &id, "cell.get", json!({ "sheet": "Notes", "cell": "C1" }), Origin::Agent);
    assert!(italic.to_string().contains("\"italic\":true"), "{italic}");
    // Navigation keeps its effect; a new sheet takes the steps that name no sheet.
    run(&mut kit, &id, "sheet.activate", json!({ "sheet": "Summary" }), Origin::Agent);
    assert_ne!(kit.context_get(&id).unwrap(), saved, "sheet.activate is the agent's move");
    run(&mut kit, &id, "home.insertSheet", json!({ "name": "Q4" }), Origin::Agent);
    run(&mut kit, &id, "range.setValues", json!({ "range": "A1", "values": [["fresh"]] }), Origin::Agent);
    assert_eq!(read(&mut kit, &id, "Q4", "A1"), json!([["fresh"]]), "create a sheet, then fill it");
    kit.context_restore(&id, &saved).unwrap();
    assert_eq!(kit.context_get(&id).unwrap(), saved);
    let _ = std::fs::remove_dir_all(root);
}

/// `kition_kit::testing::check_context` with a context that really moves the editor: another
/// sheet and a selection there.
#[test]
fn the_editor_state_hooks_pass_the_sdks_contract_check() {
    let mut book = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    book.execute("home.insertSheet", json!({ "name": "Other", "before": 9 })).unwrap();
    book.execute("sheet.activate", json!({ "sheet": 0 })).unwrap();
    let other = book.inspect().unwrap()["workbook"]["sheets"][1]["id"].clone();
    let context = kition_kit::CallContext { on: vec![json!("C3:D4")], part: Some(json!({ "sheet": other })), time: None };
    kition_kit::testing::check_context(&mut book, &context, kition_sheet_kit::EDITOR_STATE).unwrap();
    let before = book.editor_state();
    book.set_context(&context).unwrap();
    assert_ne!(book.editor_state(), before, "the context moved the editor");
}

/// What the kit publishes for the agent: the editor state each command reads (`x-khp-reads`), the
/// parameter `on` fills where kition-sdk takes it (`x-khp-role`), and the view the render command
/// draws (`target.sheet`, `target.range`).
#[test]
fn the_agents_commands_say_what_they_read_and_the_view_the_render_takes() {
    let (mut kit, id, root) = open("published");
    let commands = kit.agent_commands(Some(&id)).unwrap();
    let params = |command: &str| commands.iter().find(|listed| listed.id == command).map(|listed| listed.params.clone()).unwrap();
    assert_eq!(params("home.bold")["x-khp-reads"], json!(["selection", "part"]));
    assert_eq!(params("home.bold")["properties"]["range"]["x-khp-role"], "place");
    assert_eq!(params("cell.set")["properties"]["cell"]["x-khp-role"], "place");
    assert_eq!(params("drawing.delete")["properties"]["ids"]["x-khp-role"], "targets");
    assert_eq!(params("chart.delete")["properties"]["chart"]["x-khp-role"], "targets");
    assert!(params("file.render").get("x-khp-reads").is_none());
    let view = params("file.render")["properties"]["target"]["properties"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(view, ["range", "sheet"]);
    let _ = std::fs::remove_dir_all(root);
}
