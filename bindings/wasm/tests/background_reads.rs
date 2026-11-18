//! C8-sheet-read-2: a fixed wrapper destination does not use a delegate's default.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;

#[test]
fn background_declares_only_its_active_part() {
    // extra.rs background always supplies A1 (both the image location and the
    // clear branch ignore selection). insert.rs insert_picture's fallback to
    // selection.active cannot choose where this wrapper's write lands.
    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let command = doc.commands().into_iter().find(|command| command.id == "pageLayout.background").unwrap();
    let reads: std::collections::BTreeSet<_> = command.reads.iter().map(|state| state.as_str()).collect();
    assert_eq!(reads, std::collections::BTreeSet::from(["part"]));
}
