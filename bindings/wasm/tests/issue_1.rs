//! Conditional formatting in the rendered grid: icon sets draw their own icons (an arrow points
//! up or down, a symbol set is not a dot), not one coloured dot for every set.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

/// The cells are a new sheet's size by default (69 × 19 document pixels, WPS's), after the
/// 46-pixel row headings and the 21-pixel column headings.
const SCALE: f64 = 2.0;
const ROW: f64 = kition_sheet_engine::model::DEFAULT_ROW_HEIGHT as f64;

/// Coloured (saturated) pixels of the icon at the left of A`row`, top half and bottom half.
fn icon_halves(doc: &SheetDocument, row: u32) -> (usize, usize, Vec<u8>) {
    let (x, y) = (46.0, 21.0 + ROW * f64::from(row - 1));
    let (w, h) = ((16.0 * SCALE) as u32, (ROW * SCALE) as u32);
    let px = doc.render(x, y, SCALE, w, h).unwrap();
    let (mut top, mut bottom) = (0, 0);
    for (i, p) in px.chunks(4).enumerate() {
        let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
        if hi - lo > 60 {
            if (i as u32 / w) < h / 2 { top += 1 } else { bottom += 1 }
        }
    }
    (top, bottom, px)
}

#[test]
fn icon_sets_draw_their_icons() {
    let mut doc = SheetDocument::create("Icons.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "A1", "values": [[0], [50], [100]] })).unwrap();
    doc.execute("conditionalFormat.apply", json!({ "range": "A1:A3", "rule": { "type": "iconSet", "set": "3Arrows" } })).unwrap();
    let (down_top, down_bottom, down) = icon_halves(&doc, 1);
    let (up_top, up_bottom, up) = icon_halves(&doc, 3);
    // An arrow's head is wider than its shaft: most of an up arrow is in its top half.
    assert!(up_top * 10 > up_bottom * 11, "up arrow: {up_top} above, {up_bottom} below");
    assert!(down_bottom * 10 > down_top * 11, "down arrow: {down_top} above, {down_bottom} below");
    assert_ne!(up, down);

    let mut symbols = SheetDocument::create("Symbols.xlsx", 0, 0).unwrap();
    symbols.execute("range.setValues", json!({ "range": "A1", "values": [[0], [50], [100]] })).unwrap();
    symbols.execute("conditionalFormat.apply", json!({ "range": "A1:A3", "rule": { "type": "iconSet", "set": "3Symbols" } })).unwrap();
    let mut lights = SheetDocument::create("Lights.xlsx", 0, 0).unwrap();
    lights.execute("range.setValues", json!({ "range": "A1", "values": [[0], [50], [100]] })).unwrap();
    lights.execute("conditionalFormat.apply", json!({ "range": "A1:A3", "rule": { "type": "iconSet", "set": "3TrafficLights1" } })).unwrap();
    // A check mark in a green circle is not a plain green dot.
    assert_ne!(icon_halves(&symbols, 3).2, icon_halves(&lights, 3).2);
}
