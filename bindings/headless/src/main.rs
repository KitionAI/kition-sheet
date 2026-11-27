//! `kition-sheet`: the Sheet kit for Kition's agent. The same document the in-page editor runs
//! (`kition-sheet-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model
//! (`model.describe`, `model.batch`) over the document's change exchange, and the identity a file
//! carries (`file.identity`, `file.fork`): an `.xlsx` is written from the workbook and
//! embeds no replica (its `xl/kition/identities.json` part holds object ids, not a document id or
//! history), so it carries none.

use kition_headless::{FileIdentity, Headless, KitInfo};

const SANDBOXED: &[&str] = &[];
/// `inspect()` keys about the editing session rather than the file.
const VOLATILE: &[&str] = kition_sheet_kit::EDITOR_STATE;

fn main() -> std::process::ExitCode {
    let kit = match KitInfo::from_manifest(include_str!("../../../kit.json"), SANDBOXED, VOLATILE) {
        Ok(kit) => kit,
        Err(error) => {
            eprintln!("kit: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let backend = Headless::<kition_sheet_kit::SheetDocument>::with_replica_port(kit.clone()).with_file_identity(FileIdentity::NotEmbedded);
    kition_headless::serve_backend(&kit, backend)
}
