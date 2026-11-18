//! Fonts: `check` says when the renderer draws a font in another face
//! (`sheet.font-substituted`) and when no face it has draws a character
//! (`sheet.missing-glyph`, drawn as an empty box); characters the built-in symbol faces have
//! (★ ✔ ☐) are drawn from them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::diagnostics::Diagnostic;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

fn codes(found: &[Diagnostic], code: &str) -> Vec<String> {
    found.iter().filter(|d| d.code == code).map(|d| d.message.clone()).collect()
}

/// Dark pixels in cell `col` of row 1 (64 × 20 at 1:1, after the headings).
fn ink(doc: &SheetDocument, col: u32) -> usize {
    let px = doc.render(46.0 + 64.0 * f64::from(col), 21.0, 1.0, 64, 20).unwrap();
    px.chunks(4).filter(|p| p[0] < 120 && p[1] < 120 && p[2] < 120).count()
}

// One test: the faces added at run time are the process's, so the steps run in order.
#[test]
fn substituted_fonts_and_missing_glyphs_are_reported() {
    let mut doc = SheetDocument::create("Fonts.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "A1", "values": [["Plain", "Fancy", "Done ✓", "★", "☐"]] })).unwrap();
    doc.execute("home.fontName", json!({ "range": "B1", "name": "Segoe UI" })).unwrap();

    // A font with no face and no metric-compatible stand-in: drawn in the built-in sans face.
    let found = doc.check();
    let substituted = codes(&found, "sheet.font-substituted");
    assert_eq!(substituted.len(), 1, "{found:?}");
    assert!(substituted[0].contains("Segoe UI"), "{substituted:?}");
    // Calibri, the default, is drawn in the built-in face while the host adds no fonts at all
    // (the kit's own fallback).
    assert!(!substituted[0].contains("Calibri"));
    let diagnostic = found.iter().find(|d| d.code == "sheet.font-substituted").unwrap();
    assert_eq!(diagnostic.target.cell.as_deref(), Some("B1"));

    // ✓ is in no face the renderer has: an empty box.
    let missing = codes(&found, "sheet.missing-glyph");
    assert_eq!(missing.len(), 1, "{found:?}");
    assert!(missing[0].contains('✓') && missing[0].contains("C1"), "{missing:?}");
    // ★ and ☐ come from the built-in symbol faces: drawn, not boxes, and not reported.
    assert!(!missing[0].contains('★') && !missing[0].contains('☐'));
    assert!(ink(&doc, 3) > 10, "the star is drawn");

    // Once the host adds fonts, an Office font whose stand-in it did not add is reported too.
    assert_eq!(kition_sheet_kit::add_font(epaint_default_fonts::HACK_REGULAR), 1);
    let found = doc.check();
    let substituted = codes(&found, "sheet.font-substituted");
    assert!(substituted.iter().any(|m| m.contains("Calibri") && m.contains("Carlito")), "{substituted:?}");
    // A font the host adds draws the cells set in its family (Hack for `Hack`), not reported.
    doc.execute("home.fontName", json!({ "range": "A1", "name": "Hack" })).unwrap();
    let before = ink(&doc, 0);
    let substituted = codes(&doc.check(), "sheet.font-substituted");
    assert!(!substituted.iter().any(|m| m.starts_with("Hack ")), "{substituted:?}");
    let mut sans = SheetDocument::create("Sans.xlsx", 0, 0).unwrap();
    sans.execute("range.setValues", json!({ "range": "A1", "values": [["Plain"]] })).unwrap();
    sans.execute("home.fontName", json!({ "range": "A1", "name": "Some Sans" })).unwrap();
    assert_ne!(before, ink(&sans, 0), "Hack draws the text, not the built-in sans");
}
