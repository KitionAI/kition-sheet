//! The Sheet kit's call context, receipts and ids end to end (task card C4/C5/C6-sheet): the
//! headless `kition-sheet` executable under the kits' MCP server's tools (`kition-kits`, the library
//! `kition-kits-mcp` serves, at the pinned kition-sdk revision), on an `.xlsx` in a workspace.
//!
//! A run names where each step acts (`target`, `at.part`: a sheet by id) and what on (`on`: A1
//! ranges or drawing ids); the person's active sheet and cell are what the saved file keeps, and
//! receipts (`changes`) name sheets and drawing objects by the ids `sheet_inspect` lists.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_sheet_engine::core::CellRef;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

const ME: &str = "agent-me";

fn kit() -> &'static KitDefinition {
    KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()
}

/// `Data` (a small table) and `Notes` (a rectangle at A1), saved with the person on `Data` at C3.
fn book() -> Vec<u8> {
    let mut book = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    book.execute("sheet.rename", json!({ "name": "Data" })).unwrap();
    book.execute("range.setValues", json!({ "range": "A1", "values": [["Region", "Sales"], ["North", 120], ["South", 80]] })).unwrap();
    book.execute("sheet.insert", json!({ "name": "Notes", "before": 9 })).unwrap();
    book.execute("drawing.insert", json!({ "kind": "shape", "frame": { "x": 10, "y": 10, "w": 50, "h": 30 } })).unwrap();
    let (mut wb, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &book.save(None).unwrap()).unwrap();
    wb.active_sheet = 0;
    for sheet in &mut wb.sheets {
        std::sync::Arc::make_mut(sheet).view_active = CellRef::parse("C3").unwrap();
    }
    kition_sheet_engine::io::save_bytes(&wb, "Book.xlsx").unwrap()
}

/// The saved file's active sheet and active cell, as the person opens it.
fn person(bytes: &[u8]) -> (String, String) {
    let opened = SheetDocument::open("Book.xlsx", bytes).unwrap().inspect().unwrap();
    (opened["sheet"].as_str().unwrap().to_owned(), opened["active"].as_str().unwrap().to_owned())
}

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("Book.xlsx"), book()).unwrap();
        Self { kits: Self::server(workspace.path()), workspace }
    }

    /// A new server on `workspace` (a process of `kition-kits-mcp` is one of these).
    fn server(workspace: &Path) -> KitManager {
        KitManager::new(Some(workspace.to_path_buf()), vec![(kit(), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-sheet"))))])
    }

    fn file(&self) -> Vec<u8> {
        std::fs::read(self.workspace.path().join("Book.xlsx")).unwrap()
    }

    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!("Book.xlsx");
        match tools::call_as(&self.kits, &format!("sheet_{action}"), &input, CallOptions::default(), ME).await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("sheet_{action} is no tool"),
        }
    }

    async fn ok(&self, action: &str, input: Value) -> Value {
        self.call(action, input).await.unwrap_or_else(|error| panic!("sheet_{action}: {error}"))
    }

    async fn refused(&self, action: &str, input: Value) -> Value {
        match self.call(action, input).await {
            Err(error) => error,
            Ok(answer) => panic!("sheet_{action} was expected to be refused: {answer}"),
        }
    }

    /// The sheets as `sheet_inspect` lists them.
    async fn sheets(&self) -> Vec<Value> {
        self.ok("inspect", json!({})).await["details"]["workbook"]["sheets"].as_array().unwrap().clone()
    }

    async fn sheet(&self, name: &str) -> Value {
        self.sheets().await.into_iter().find(|sheet| sheet["name"] == name).unwrap_or_else(|| panic!("no sheet {name}"))
    }

    async fn read(&self, sheet: &str, range: &str) -> Value {
        self.ok("inspect", json!({ "target": { "sheet": sheet, "range": range } })).await["content"]["rows"].clone()
    }

    async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array().map(|items| items.iter().filter_map(|item| item["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

// ---- Receipts name what a call changed by the ids inspect lists ----

#[tokio::test(flavor = "multi_thread")]
async fn a_receipt_names_the_sheets_and_drawing_objects_a_run_changed_by_the_ids_inspect_listed() {
    let f = Fixture::new();
    let notes = f.sheet("Notes").await;
    let (sheet, drawing) = (notes["toolId"].as_str().unwrap().to_owned(), notes["drawings"][0]["toolId"].as_str().unwrap().to_owned());
    // A rename names the sheet, with before and after.
    let ran = f.ok("run", json!({ "steps": [{ "command": "sheet.rename", "params": { "sheet": sheet, "name": "Memo" } }] })).await;
    assert_eq!(
        ran["changes"]["changed"],
        json!([{ "id": sheet, "type": "Sheet", "fields": ["name"], "edits": [{ "field": "name", "before": "Notes", "after": "Memo" }] }]),
        "{ran}"
    );
    // A drawing object's fill, by `on`: the receipt names exactly that object.
    let ran = f.ok("run", json!({ "steps": [{ "command": "drawing.format", "on": [drawing], "params": { "fill": "#C00000" } }] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), std::slice::from_ref(&drawing), "{ran}");
    // A drawing object made on a named sheet: `created` names it by the id inspect then lists.
    let ran = f
        .ok(
            "run",
            json!({ "target": { "sheet": sheet }, "steps": [{ "command": "drawing.insert", "params": { "kind": "textBox", "text": "New" } }] }),
        )
        .await;
    let listed = f.sheet("Memo").await["drawings"].as_array().unwrap().iter().map(|d| d["toolId"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    let made = listed.iter().find(|id| **id != drawing).unwrap().clone();
    assert!(ids(&ran["changes"]["created"]).contains(&made), "{made}: {ran}");
    f.shutdown().await;
}

/// Rows inserted shift every address below them: the receipt says so compactly, as the rows
/// created (where, on which sheet), with positional cell metadata distinguished from content.
#[tokio::test(flavor = "multi_thread")]
async fn a_receipt_for_inserted_rows_says_what_moved_without_listing_the_cells() {
    let f = Fixture::new();
    let data = f.sheet("Data").await["toolId"].as_str().unwrap().to_owned();
    let ran = f.ok("run", json!({ "steps": [{ "command": "row.insert", "params": { "rows": "Data!2:3" } }] })).await;
    let created = ran["changes"]["created"].as_array().unwrap();
    assert_eq!(created.len(), 2, "two independently named rows: {ran}");
    for (row, object) in (1..=2).zip(created) {
        assert_eq!(object["type"], "Row", "{ran}");
        assert!(object["id"].as_str().unwrap().starts_with("row:"), "{ran}");
        assert_eq!(object["at"], json!({"parent":data,"field":"rows","index":row}), "{ran}");
        assert_eq!(object["values"]["index"], row, "{ran}");
    }
    let named =
        ["changed", "moved", "deleted"].iter().flat_map(|list| ran["changes"][*list].as_array().cloned().unwrap_or_default()).collect::<Vec<_>>();
    let cells: Vec<_> = named.iter().filter(|object| object["type"] == "Cell").collect();
    assert_eq!(cells.len(), 4, "four positional addresses move: {ran}");
    for object in cells {
        assert_eq!(object["fields"], json!(["address", "label"]), "no stored cell content changes: {ran}");
        assert_eq!(object["edits"].as_array().unwrap().len(), 2, "{ran}");
    }
    assert_eq!(f.read("Data", "A4:B4").await, json!([[4, "North", 120.0]]), "North moved down two rows");
    f.shutdown().await;
}

// ---- Steps on two sheets; the person's sheet and cell stay ----

#[tokio::test(flavor = "multi_thread")]
async fn a_run_edits_two_sheets_by_id_and_the_saved_file_keeps_the_persons_sheet_and_cell() {
    let f = Fixture::new();
    assert_eq!(person(&f.file()), ("Data".to_owned(), "C3".to_owned()));
    let (data, notes) = (f.sheet("Data").await["id"].clone(), f.sheet("Notes").await["id"].clone());
    let ran = f
        .ok(
            "run",
            json!({ "target": { "sheet": notes }, "steps": [
                { "command": "range.setValues", "params": { "range": "B2", "values": [["Q1", "Q2"]] } },
                { "command": "format.bold", "on": ["B2:C2"], "params": { "on": true } },
                { "command": "range.setValues", "at": { "part": { "sheet": data } }, "params": { "range": "C1", "values": [["Target"], [100], [100]] } },
                { "command": "select.stats", "at": { "part": { "sheet": data } }, "on": ["B2:B3"] }
            ] }),
        )
        .await;
    assert_eq!(ran["completed"][3]["result"]["sum"], json!(200.0), "select.stats read the cells `on` named on Data: {ran}");
    assert_eq!(f.read("Notes", "B2:C2").await, json!([[2, "Q1", "Q2"]]));
    assert_eq!(f.read("Data", "C1:C3").await, json!([[1, "Target"], [2, 100.0], [3, 100.0]]));
    assert_eq!(f.read("Notes", "C1:C3").await, json!([[1, null], [2, "Q2"], [3, null]]), "Data's values stayed on Data");
    assert_eq!(person(&f.file()), ("Data".to_owned(), "C3".to_owned()), "the saved file opens where the person was, not where the agent worked");
    // A sheet the agent makes takes the steps after it that name none.
    f.ok(
        "run",
        json!({ "steps": [
        { "command": "sheet.insert", "params": { "name": "Q4" } },
        { "command": "range.setValues", "params": { "range": "A1", "values": [["fresh"]] } }
    ] }),
    )
    .await;
    assert_eq!(f.read("Q4", "A1").await, json!([[1, "fresh"]]));
    assert_eq!(person(&f.file()), ("Data".to_owned(), "C3".to_owned()), "the new sheet is not left active");
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_later_step_changes_nothing_and_leaves_the_person_where_they_were() {
    let f = Fixture::new();
    let notes = f.sheet("Notes").await["id"].clone();
    let before = f.file();
    let refused = f
        .refused(
            "run",
            json!({ "target": { "sheet": notes }, "steps": [
                { "command": "range.setValues", "params": { "range": "A1", "values": [["kept?"]] } },
                { "command": "format.bold", "params": { "range": "Nowhere!A1", "on": true } }
            ] }),
        )
        .await;
    assert_eq!(refused["error"]["index"], json!(1), "{refused}");
    assert_eq!(f.file(), before, "the run was rolled back: nothing changed in the file");
    assert_eq!(f.read("Notes", "A1").await, json!([[1, null]]));
    assert_eq!(person(&f.file()), ("Data".to_owned(), "C3".to_owned()));
    f.shutdown().await;
}

// ---- What the workbook lacks or cannot mean is refused, naming where valid ids come from ----

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_sheet_a_bad_range_or_a_mixed_on_is_invalid_params() {
    let f = Fixture::new();
    let drawing = f.sheet("Notes").await["drawings"][0]["toolId"].clone();
    let before = f.file();
    for (input, says) in [
        (
            json!({ "target": { "sheet": 99 }, "steps": [{ "command": "range.setValues", "params": { "range": "A1", "values": [[1]] } }] }),
            "no sheet has id 99",
        ),
        (
            json!({ "target": { "sheet": "Nowhere" }, "steps": [{ "command": "range.setValues", "params": { "range": "A1", "values": [[1]] } }] }),
            "no sheet named `Nowhere`",
        ),
        (json!({ "target": { "sheet": "Data" }, "steps": [{ "command": "select.stats", "on": ["B2:ZZZZ9"] }] }), "neither a range in A1 notation"),
        (
            json!({ "target": { "sheet": "Notes" }, "steps": [{ "command": "select.stats", "on": ["B2", drawing] }] }),
            "mixes cells and drawing objects",
        ),
        (json!({ "steps": [{ "command": "select.stats", "on": ["B2"] }] }), "an address means something only on its sheet"),
        (json!({ "steps": [{ "command": "range.setValues", "at": { "time": 2 }, "params": { "range": "A1", "values": [[1]] } }] }), "no timeline"),
    ] {
        let refused = f.refused("run", input.clone()).await;
        let code = refused["error"]["code"].as_str().unwrap().to_owned();
        assert!(code == "invalid-params" || code == "unsupported", "{input}: {refused}");
        assert!(refused.to_string().contains(says), "{input}: {refused}");
    }
    assert_eq!(f.file(), before, "nothing changed in the file");
    f.shutdown().await;
}

// ---- Ids survive saving and a new server ----

#[tokio::test(flavor = "multi_thread")]
async fn ids_from_an_earlier_inspect_name_the_same_sheets_and_objects_after_a_new_server_opens_the_saved_file() {
    let f = Fixture::new();
    let before = f.sheets().await;
    // The agent renames and moves a sheet and adds a chart; the file is saved.
    f.ok(
        "run",
        json!({ "steps": [
        { "command": "sheet.rename", "params": { "sheet": before[1]["toolId"], "name": "Memo" } },
        { "command": "sheet.move", "params": { "sheet": "Memo", "to": 0 } },
        { "command": "drawing.insert", "params": { "kind": "chart", "range": "Data!A1:B3" } }
    ] }),
    )
    .await;
    let Fixture { kits, workspace } = f;
    kits.shutdown().await;
    let f = Fixture { kits: Fixture::server(workspace.path()), workspace };
    let after = f.sheets().await;
    let memo = after.iter().find(|sheet| sheet["name"] == "Memo").unwrap();
    assert_eq!((memo["id"].clone(), memo["index"].clone()), (before[1]["id"].clone(), json!(0)), "renamed and moved, the same id: {after:?}");
    assert_eq!(memo["drawings"][0]["toolId"], before[1]["drawings"][0]["toolId"], "the rectangle keeps its id");
    let data = after.iter().find(|sheet| sheet["name"] == "Data").unwrap();
    assert_eq!(data["id"], before[0]["id"]);
    let chart = data["drawings"][0]["toolId"].as_str().unwrap().to_owned();
    // The commands take the earlier ids; the receipt names them.
    let ran = f
        .ok("run", json!({ "steps": [{ "command": "drawing.format", "on": [before[1]["drawings"][0]["toolId"]], "params": { "fill": "#00C000" } }] }))
        .await;
    assert_eq!(ids(&ran["changes"]["changed"]), [before[1]["drawings"][0]["toolId"].as_str().unwrap().to_owned()], "{ran}");
    assert!(chart.starts_with("drawing:"), "{chart}");
    f.shutdown().await;
}
