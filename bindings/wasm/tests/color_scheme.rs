//! The editor's grid in the host's color scheme (`editor.scheme`; task dark-surface-board-sheet,
//! row H16 of Kition's host matrix). In the dark scheme the empty cells, gridlines and headings are
//! dark and automatic text is light; the colours the workbook sets (a fill, a font colour) are drawn
//! as they are; nothing moves; the switch repaints (the revision moves) without changing or
//! dirtying the workbook; pictures, the agent's preview and the saved file stay light and byte for
//! byte what they were. Before, the grid had one light palette and was white in a dark host.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

/// Default cells are 69 × 19 at 1:1 (`inspect().defaultSize`), after the headings (46 × 21).
const COL_W: f64 = 69.0;
const ROW_H: f64 = 19.0;
const LEFT: f64 = 46.0;
const TOP: f64 = 21.0;
const DARK_BACKGROUND: [u8; 4] = [30, 30, 30, 255];

/// Values, a formula, a cell the file fills yellow, a cell whose font the file colours red, and a
/// selection.
fn book() -> SheetDocument {
    let mut doc = SheetDocument::create("Budget.xlsx", 0, 0).unwrap();
    doc.execute(
        "range.setValues",
        json!({ "range": "A1", "values": [["Item", "Cost", "Paid", "Late"], ["Rent", 1200, "Yes", "No"], ["Food", 450]] }),
    )
    .unwrap();
    doc.execute("cell.set", json!({ "cell": "B4", "input": "=SUM(B2:B3)" })).unwrap();
    doc.execute("home.fillColor", json!({ "range": "C2", "color": "#FFFF00" })).unwrap();
    doc.execute("home.fontColor", json!({ "range": "D2", "color": "#FF0000" })).unwrap();
    doc.execute("selection.set", json!({ "range": "A1:B2" })).unwrap();
    doc
}

/// The pixels of cell (`col`, `row`), 0-based, at 1:1.
fn cell(doc: &SheetDocument, col: u32, row: u32) -> Vec<u8> {
    doc.render(LEFT + COL_W * f64::from(col), TOP + ROW_H * f64::from(row), 1.0, COL_W as u32, ROW_H as u32).unwrap()
}

/// The pixel at (`x`, `y`) of a cell's pixels.
fn at(px: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * COL_W as usize + x) * 4;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

fn luminance(px: &[u8]) -> f64 {
    let channel = |v: u8| {
        let c = f64::from(v) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(px[0]) + 0.7152 * channel(px[1]) + 0.0722 * channel(px[2])
}

fn count(px: &[u8], keep: impl Fn(&[u8]) -> bool) -> usize {
    px.chunks(4).filter(|p| keep(p)).count()
}

fn scheme(doc: &mut SheetDocument, name: &str) -> serde_json::Value {
    doc.execute("editor.scheme", json!({ "scheme": name })).unwrap()
}

#[test]
fn the_dark_grid_is_dark_and_the_files_colours_are_kept() {
    let mut doc = book();
    let light_text = cell(&doc, 0, 1);
    assert!(count(&light_text, |p| luminance(p) < 0.1) > 10, "the light grid draws Rent dark");

    assert_eq!(scheme(&mut doc, "dark"), json!({ "changed": true, "scheme": "dark" }));
    // An empty cell, inside its gridlines, and the headings' corner.
    assert_eq!(at(&cell(&doc, 5, 6), 30, 10), DARK_BACKGROUND);
    let corner = doc.render(2.0, 2.0, 1.0, 1, 1).unwrap();
    assert!(luminance(&corner) <= 0.1, "the headings are dark: {corner:?}");
    // Automatic text is light on the dark grid.
    let rent = cell(&doc, 0, 1);
    assert!(count(&rent, |p| luminance(p) > 0.5) > 10, "the dark grid draws Rent light");
    assert_eq!(count(&rent, |p| luminance(p) < 0.005), 0, "no black text on the dark grid");
    // The file's yellow fill is kept, with automatic text dark on it.
    let paid = cell(&doc, 2, 1);
    assert_eq!(at(&paid, COL_W as usize - 6, 3), [255, 255, 0, 255], "the fill, right of the text");
    assert!(count(&paid, |p| luminance(p) < 0.1) > 10, "Yes is dark on the yellow fill");
    // The file's red font is kept.
    let late = cell(&doc, 3, 1);
    assert!(count(&late, |p| p[0] > 200 && p[1] < 80 && p[2] < 80) > 5, "No stays red");
    // The formula's value is drawn too.
    assert!(count(&cell(&doc, 1, 3), |p| luminance(p) > 0.5) > 10, "the total 1650 is drawn light");
}

#[test]
fn nothing_moves_between_schemes() {
    let mut doc = book();
    let (w, h) = (6 * COL_W as u32, 6 * ROW_H as u32);
    let light = doc.render(LEFT, TOP, 1.0, w, h).unwrap();
    let inspected = doc.inspect().unwrap();
    scheme(&mut doc, "dark");
    let dark = doc.render(LEFT, TOP, 1.0, w, h).unwrap();
    assert_eq!(doc.inspect().unwrap(), inspected, "the editor's state (selection, extent, ...) is the same");
    assert_eq!((doc.width(), doc.height()), {
        let mut again = book();
        scheme(&mut again, "light");
        (again.width(), again.height())
    });
    // Every pixel that is not the background in one scheme is not the background in the other:
    // gridlines, letters and fills in the same places.
    let drawn = |px: &[u8], background: [u8; 4]| -> Vec<bool> {
        px.chunks(4).map(|p| p.iter().zip(background).take(3).any(|(a, b)| a.abs_diff(b) > 20)).collect()
    };
    let (a, b) = (drawn(&light, [255, 255, 255, 255]), drawn(&dark, DARK_BACKGROUND));
    let differ = a.iter().zip(&b).filter(|(a, b)| a != b).count();
    let marked = a.iter().filter(|a| **a).count();
    assert!(marked > 500, "the grid draws something: {marked}");
    assert!(differ * 50 < marked, "the same pixels are drawn in both schemes: {differ} of {marked} differ");
}

#[test]
fn the_switch_repaints_and_changes_nothing_in_the_workbook() {
    let mut doc = book();
    let before = doc.render(0.0, 0.0, 1.0, 400, 200).unwrap();
    let (revision, dirty) = (doc.revision(), doc.dirty());
    let saved = doc.save(None).unwrap();
    let preview = doc.execute("file.render", json!({ "target": { "range": "A1:D4" } })).unwrap();
    let picture = doc.execute("file.export", json!({ "format": "png" })).unwrap();

    scheme(&mut doc, "dark");
    assert!(doc.revision() > revision, "the revision moves, so the page repaints the grid");
    assert_eq!(doc.dirty(), dirty, "a scheme is not a change of the workbook");
    assert_ne!(doc.render(0.0, 0.0, 1.0, 400, 200).unwrap(), before);
    // The same scheme again changes nothing.
    let now = doc.revision();
    assert_eq!(scheme(&mut doc, "dark"), json!({ "changed": false, "scheme": "dark" }));
    assert_eq!(doc.revision(), now);
    // Pictures, the agent's preview and the file are what they were.
    assert_eq!(doc.save(None).unwrap(), saved, "nothing about the scheme is saved");
    assert_eq!(doc.execute("file.render", json!({ "target": { "range": "A1:D4" } })).unwrap(), preview);
    assert_eq!(doc.execute("file.export", json!({ "format": "png" })).unwrap(), picture);

    // Back to light: the grid as it was, byte for byte.
    assert_eq!(scheme(&mut doc, "light"), json!({ "changed": true, "scheme": "light" }));
    assert_eq!(doc.render(0.0, 0.0, 1.0, 400, 200).unwrap(), before);
    assert_eq!(doc.dirty(), dirty);

    assert!(doc.execute("editor.scheme", json!({ "scheme": "sepia" })).unwrap_err().contains("invalid-params"));
    assert!(doc.execute("editor.scheme", json!({})).is_err());
}
