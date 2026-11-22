//! The editor repaints the person's grid after an agent's step only when the step moved the
//! revision or `inspect()` (kition-sdk `agentSkipsUnchangedPaint`, which `web/src/App.tsx` turns
//! on): a step that changes what the grid draws must move one of them, or the person keeps stale
//! pixels; a step that only reads must move neither, or every read repaints the whole grid and
//! drops the paint in progress (regression case `regressions/issue-agent-reads-repaint`).
//!
//! Each step runs as the page runs an agent's step (`kitEngine.worker.ts` `inAgentState`): the
//! person's editor state is taken, the agent's own put in its place, the command runs as the
//! agent, and the person's state is put back before the page reads the revision, `inspect()` and
//! the pixels.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_sheet_engine::Origin;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

/// The part of the grid the checks look at: the headings and the first dozen columns and two
/// dozen rows, at 1:1.
const VIEW: (u32, u32) = (900, 500);

/// `basic.xlsx` as a host opens it, with a volatile formula in view (F3: `=RAND()`), which only a
/// recalculation changes.
fn workbook() -> SheetDocument {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.xlsx"));
    let mut doc = SheetDocument::open("basic.xlsx", &std::fs::read(path).unwrap()).unwrap();
    // A host owns the files: a step that names none writes nothing next to the tests.
    doc.hosted(&kition_kit::host::Hosting { files_only: true });
    doc.execute("cell.set", json!({ "cell": "F3", "input": "=RAND()" })).unwrap();
    doc
}

/// What the page compares after a step, and what the person sees.
#[derive(PartialEq)]
struct Seen {
    revision: u64,
    inspected: Option<Value>,
    pixels: Vec<u8>,
}

fn seen(doc: &SheetDocument) -> Seen {
    Seen { revision: doc.revision(), inspected: doc.inspect().ok(), pixels: doc.render(0.0, 0.0, 1.0, VIEW.0, VIEW.1).unwrap_or_default() }
}

/// The agent's run on the live document: its own editor state from step to step, the person's
/// put back after each.
#[derive(Default)]
struct Agent {
    state: Option<Value>,
}

impl Agent {
    fn step(&mut self, doc: &mut SheetDocument, command: &str, params: Value) -> Result<Value, String> {
        let person = doc.editor_state();
        if let Some(state) = &self.state {
            doc.restore_editor_state(state)?;
        }
        let result = doc.execute_as(command, params, &Origin::agent("agent:test"));
        self.state = Some(doc.editor_state());
        // A step that closed the workbook leaves no state to put back (the page reopens it).
        let _ = doc.restore_editor_state(&person);
        result
    }
}

/// A command's sample parameters (`command_samples.json`, `{}` when it has none) and the steps the
/// person takes first.
fn sample(id: &str) -> (Vec<(String, Value)>, Value) {
    let samples: Value = serde_json::from_str(include_str!("command_samples.json")).unwrap();
    let tmp = std::env::temp_dir().join(format!("kition-sheet-agent-paint-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let fill = |value: &Value| -> Value { serde_json::from_str(&value.to_string().replace("$tmp", &tmp.to_string_lossy())).unwrap() };
    let entry = &samples[id];
    let setup = entry["setup"].as_array().into_iter().flatten().map(|step| (step[0].as_str().unwrap().to_owned(), fill(&step[1]))).collect();
    (setup, entry.get("params").map_or(json!({}), fill))
}

/// Every command an agent can run: when its step changes what the grid draws, it moves the
/// revision or `inspect()`, so the page repaints it (a skipped paint would leave the old pixels).
#[test]
fn every_agent_step_that_changes_the_grid_moves_the_revision_or_inspect() {
    let ids: Vec<String> = workbook().commands().into_iter().map(|command| command.id).collect();
    assert!(ids.len() > 300, "the catalogue lists the commands ({})", ids.len());
    let mut stale = Vec::new();
    for id in &ids {
        let mut doc = workbook();
        let (setup, params) = sample(id);
        for (command, params) in setup {
            let _ = doc.execute(&command, params);
        }
        let before = seen(&doc);
        let _ = Agent::default().step(&mut doc, id, params);
        let after = seen(&doc);
        // A closed workbook is the page's to reopen: it repaints whatever the revision says.
        if after.inspected.is_none() {
            continue;
        }
        if after.pixels != before.pixels && after.revision == before.revision && after.inspected == before.inspected {
            stale.push(id.clone());
        }
    }
    assert!(stale.is_empty(), "agent steps that change the grid but neither the revision nor inspect(): {stale:#?}");
}

/// An agent's Calculate Now (F9, Shift+F9) draws the new values of `RAND()` and `NOW()`: it is no
/// undo step, so the drawing's own count moves the revision.
#[test]
fn an_agents_recalculation_moves_the_revision_and_draws_the_new_values() {
    for command in ["formulas.calculateNow", "formulas.calculateSheet"] {
        let mut doc = workbook();
        let before = seen(&doc);
        let value = doc.execute("cell.get", json!({ "cell": "F3" })).unwrap()["value"].clone();
        Agent::default().step(&mut doc, command, json!({})).unwrap();
        let after = seen(&doc);
        assert_ne!(doc.execute("cell.get", json!({ "cell": "F3" })).unwrap()["value"], value, "{command} recalculates RAND()");
        assert_ne!(after.pixels, before.pixels, "{command}: the grid shows the new value");
        assert_ne!(after.revision, before.revision, "{command}: the page repaints it");
    }
}

/// Reading steps (the cells, the sheet, the workbook, a selection of the agent's own, a picture of
/// a range, a formula evaluated, on the active sheet or another) move neither the revision nor
/// `inspect()`: the page does not repaint the person's grid for them.
#[test]
fn an_agents_reading_steps_move_neither_the_revision_nor_inspect() {
    let mut doc = workbook();
    let first = doc.inspect().unwrap()["sheet"].as_str().unwrap().to_owned();
    doc.execute("home.insertSheet", json!({ "name": "Notes" })).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": first })).unwrap();
    doc.execute("selection.set", json!({ "range": "B2" })).unwrap();
    let before = seen(&doc);
    let mut agent = Agent::default();
    let steps = [
        ("cell.get", json!({ "cell": "A1" })),
        ("cell.get", json!({ "cell": "B3" })),
        ("cell.get", json!({ "sheet": "Notes", "cell": "A1" })),
        ("sheet.read", json!({})),
        ("sheet.read", json!({ "range": "A1:B5", "formulas": true })),
        ("document.inspect", json!({})),
        ("selection.set", json!({ "range": "H30" })),
        ("selection.stats", json!({})),
        ("history.list", json!({})),
        ("formulas.evaluate", json!({ "formula": "=SUM(B2:B5)" })),
        ("sheet.activate", json!({ "sheet": "Notes" })),
        ("cell.get", json!({ "cell": "A1" })),
        ("file.render", json!({ "target": { "range": "A1:D10" } })),
    ];
    for (command, params) in steps {
        agent.step(&mut doc, command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"));
        let now = seen(&doc);
        assert_eq!(now.revision, before.revision, "{command} {params} moved the revision");
        assert!(now.inspected == before.inspected, "{command} {params} changed inspect()");
        assert!(now.pixels == before.pixels, "{command} {params} changed the person's grid");
    }
}

/// The person's own scrolling (`editor.viewport`) is no change of the document: the revision stays,
/// so the host hears of no change while the person scrolls. An agent's that changes the extent the
/// grid is drawn to moves it.
#[test]
fn only_anothers_viewport_moves_the_revision() {
    let mut doc = workbook();
    let revision = doc.revision();
    doc.execute("editor.viewport", json!({ "right": 4000, "bottom": 3000 })).unwrap();
    assert_eq!(doc.revision(), revision, "the person's scrolling");
    Agent::default().step(&mut doc, "editor.viewport", json!({ "right": 900, "bottom": 500 })).unwrap();
    assert_ne!(doc.revision(), revision, "an agent's step that changes the extent drawn");
    let revision = doc.revision();
    Agent::default().step(&mut doc, "editor.viewport", json!({ "right": 900, "bottom": 500 })).unwrap();
    assert_eq!(doc.revision(), revision, "an agent's step that leaves the extent as it was");
}

/// An agent's edits the person sees (a value, a fill, a conditional format, frozen panes, the
/// zoom) move the revision as before.
#[test]
fn an_agents_visible_edits_move_the_revision() {
    let steps = [
        ("cell.set", json!({ "cell": "D4", "input": "42" })),
        ("home.fillColor", json!({ "range": "D2:D6", "color": "#FFC000" })),
        ("home.conditionalFormat", json!({ "range": "B2:B5", "rule": { "type": "dataBar" } })),
        ("view.freezePanes", json!({ "cell": "B3" })),
        ("view.zoom", json!({ "percent": 150 })),
        ("editor.scheme", json!({ "scheme": "dark" })),
    ];
    for (command, params) in steps {
        let mut doc = workbook();
        let before = seen(&doc);
        Agent::default().step(&mut doc, command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"));
        let after = seen(&doc);
        assert!(after.revision != before.revision || after.inspected != before.inspected, "{command} {params}: the page repaints it");
    }
}
