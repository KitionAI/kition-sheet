//! `kition-sheet`: the Sheet kit for Kition's agent. The same document the in-page editor runs
//! (`kition-sheet-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange.

const SANDBOXED: &[&str] = &[];
/// `inspect()` keys about the editing session rather than the file.
const VOLATILE: &[&str] = kition_sheet_kit::EDITOR_STATE;

fn main() -> std::process::ExitCode {
    kition_headless::run_manifest_model::<kition_sheet_kit::SheetDocument>(include_str!("../../../kit.json"), SANDBOXED, VOLATILE)
}
