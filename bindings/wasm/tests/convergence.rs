//! The Kition platform convergence suite (L6, `kition-convergence`) against `SheetDocument`,
//! driven as a host drives it: the contract 1 `execute` with the engine's own commands, and the
//! contract 2 members (`open_with_replica`, `version`, `export_changes`, `import_changes`,
//! `snapshot`, `undo`, `redo`, `blobs`, `add_blobs`).
//!
//! The probe is the first sheet: cell `B1` (a number) for field edits, cell `A1` (text) for
//! typing. Cell text is a KDM `text` field that the bridge writes by character, so the text
//! scenarios run on it; a line break typed into the cell is the "paragraph" the split and merge
//! scenarios use.

mod common;

use common::SheetKit;
use kition_convergence::{Net, Options, Script, wasm};
use kition_sheet_engine::core::CellRef;
use kition_sheet_engine::core::Value;

#[test]
fn converges() {
    let report = kition_convergence::run(&SheetKit, &Options::from_env());
    println!("{}", report.render());
    report.save_if_requested().unwrap();
    assert!(report.level6(), "{}", report.render());
}

/// Architecture section 16: a row inserted above a referenced cell, concurrent with a new formula
/// referencing that cell, keeps the formula pointing at the intended cell.
#[test]
fn row_insertion_concurrent_with_a_formula_keeps_its_reference() {
    let mut net = Net::new(&SheetKit, 2, 31).unwrap();
    net.settle().unwrap();
    common::run(net.doc(0).unwrap(), "home.insertRows", serde_json::json!({ "rows": "1:1" })).unwrap();
    common::run(net.doc(1).unwrap(), "cell.set", serde_json::json!({ "cell": "C1", "input": "=B1*3" })).unwrap();
    net.settle().unwrap();
    for index in 0..2 {
        let wb = common::raw_view(net.doc(index).unwrap()).unwrap();
        let sheet = wb.sheet(0).unwrap();
        let at = |a1: &str| CellRef::parse(a1).unwrap();
        assert_eq!(sheet.input_text(at("B2")), "120", "replica {index}: B1 moved to B2");
        assert_eq!(sheet.input_text(at("C2")), "=B2*3", "replica {index}: the formula moved with its row and follows B1");
        assert_eq!(sheet.cell(at("C2")).map(|cell| cell.value.clone()), Some(Value::Number(360.0)), "replica {index}");
    }
}

/// A formula edited on one replica while the other deletes the row it references: both replicas
/// read the same formula (a `#REF!` or the moved reference), never a mix.
#[test]
fn row_deletion_concurrent_with_a_formula_converges() {
    let mut net = Net::new(&SheetKit, 2, 32).unwrap();
    net.settle().unwrap();
    common::run(net.doc(0).unwrap(), "home.deleteRows", serde_json::json!({ "rows": "1:1" })).unwrap();
    common::run(net.doc(1).unwrap(), "cell.set", serde_json::json!({ "cell": "D3", "input": "=B2+B1" })).unwrap();
    net.settle().unwrap();
}

#[test]
fn wasm_parity() {
    let Some(engine) = wasm::WasmEngine::from_env("kition_sheet_kit", "SheetDocument", "Book.xlsx") else {
        println!("skipped: set KITION_WASM_ENGINE to a built Sheet engine folder");
        return;
    };
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sheet-convergence");
    wasm::check_parity(&SheetKit, &engine, &scratch, 1).unwrap();
}

#[test]
fn scripted_steps_are_supported() {
    let mut net = Net::new(&SheetKit, 1, 1).unwrap();
    for step in [Script::SetField { slot: 0, value: 1 }, Script::SetField { slot: 1, value: 1 }, Script::Create, Script::Reorder { which: 0 }] {
        assert!(net.script(0, &step).unwrap(), "{step:?}");
    }
}
