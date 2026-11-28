use super::*;
use kition_kit::testing::check_contract;

/// A workbook with a header, two numbers, and their sum.
fn fixture() -> Vec<u8> {
    let mut doc = SheetDocument::create("Budget.xlsx", 0, 0).unwrap();
    for (cell, input) in [("A1", "Item"), ("B1", "Cost"), ("A2", "Rent"), ("B2", "1200"), ("A3", "Food"), ("B3", "450"), ("B4", "=SUM(B2:B3)")] {
        doc.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
    }
    doc.save(Some("Budget.xlsx")).unwrap()
}

#[test]
fn keeps_the_kit_contract() {
    check_contract::<SheetDocument>(&fixture(), "Budget.xlsx", |doc| doc.execute("cell.set", json!({ "cell": "C2", "input": "99" })).map(|_| ()))
        .unwrap();
}

fn event(action: PointerAction, x: f64, y: f64, shift: bool) -> ToolPointer {
    ToolPointer { action, x, y, mods: ToolMods { shift, ..ToolMods::default() }, pressure: 1.0, zoom: 1.0 }
}

/// A new sheet's column width and row height (WPS for Mac's: 69 x 19 pixels).
const CW: f64 = kition_sheet_engine::model::DEFAULT_COL_WIDTH as f64;
const RH: f64 = kition_sheet_engine::model::DEFAULT_ROW_HEIGHT as f64;

/// The canvas centre of a cell at the default sizes.
fn centre(col: u32, row: u32) -> (f64, f64) {
    (HEADER_W + f64::from(col) * CW + CW / 2.0, HEADER_H + f64::from(row) * RH + RH / 2.0)
}

fn click(doc: &mut SheetDocument, (x, y): (f64, f64), shift: bool) -> ToolResponse {
    doc.pointer(&event(PointerAction::Down, x, y, shift)).unwrap();
    doc.pointer(&event(PointerAction::Up, x, y, shift)).unwrap()
}

fn get(doc: &SheetDocument, key: &str) -> Json {
    doc.inspect().unwrap()[key].clone()
}

/// Escape cancels first (KitPointer 4.4): `inspect` says whether it has something to cancel, so
/// the editor leaves an Escape with nothing to cancel to the host (in Focus, it leaves Focus).
#[test]
fn inspect_says_when_escape_has_something_to_cancel() {
    let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
    click(&mut doc, centre(1, 1), false);
    assert_eq!(get(&doc, "escape"), json!(false), "a selected cell has nothing to cancel");
    KitTools::text(&mut doc, "7", 1.0).unwrap();
    assert_eq!(get(&doc, "escape"), json!(true), "typing in a cell");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(get(&doc, "escape"), json!(false), "Escape dropped the typing");
    doc.execute("ink.shape", json!({ "preset": "rect" })).unwrap();
    assert_eq!(get(&doc, "escape"), json!(true), "a shape chosen to draw");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(get(&doc, "escape"), json!(false), "Escape put the shape down");
}

#[test]
fn formula_reference_highlights_follow_the_draft_and_clear_on_escape() {
    let mut doc = SheetDocument::create("References.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(3, 1), false);
    let response = KitTools::text(&mut doc, "=B5+B10+B5", 1.0).unwrap();
    let refs = get(&doc, "formulaReferences");
    assert_eq!(refs.as_array().unwrap().len(), 3);
    assert_eq!(refs[0]["text"], "B5");
    assert_eq!(refs[1]["text"], "B10");
    assert_ne!(refs[0]["color"], refs[1]["color"]);
    assert_eq!(refs[0]["color"], refs[2]["color"]);
    let color: [u8; 4] = serde_json::from_value(refs[0]["color"].clone()).unwrap();
    assert!(response.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { color: c, .. } if *c == color)));
    let edit = doc.edit_box(0.0, 0.0, 1.0).unwrap();
    assert!(edit.segments.iter().any(|s| s.text == "B5" && s.color == color));
    // At display density 2 the glyph interiors include opaque pixels, so their exact color
    // can be checked without a tolerance for antialiased edge pixels.
    let pixels = doc.render(HEADER_W + 3.0 * CW, HEADER_H + RH, 2.0, 320, 48).unwrap();
    assert!(pixels.as_chunks::<4>().0.contains(&color), "reference text is colored in the cell editor");
    assert!(!doc.dirty());
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(get(&doc, "formulaReferences"), json!([]));
    assert!(value(&mut doc, "D2")["input"].as_str().unwrap_or_default().is_empty());
}

#[test]
fn grid_viewport_extends_a_blank_sheet_without_changing_the_workbook() {
    let mut doc = SheetDocument::create("Wide.xlsx", 0, 0).unwrap();
    doc.execute("editor.viewport", json!({ "right": 3840, "bottom": 2400 })).unwrap();
    assert!(doc.width() >= 3840 && doc.height() >= 2400);
    let x = HEADER_W + 40.0 * CW;
    assert_eq!(doc.render(x, HEADER_H + 10.0, 1.0, 1, 1).unwrap(), GRIDLINE);
    assert_eq!(doc.render(x + 10.0, 4.0, 1.0, 1, 1).unwrap(), HEADING);
    click(&mut doc, (x + CW / 2.0, HEADER_H + RH / 2.0), false);
    assert_eq!(get(&doc, "active"), "AO1");
    doc.execute("editor.viewport", json!({ "right": 8000, "bottom": 6000 })).unwrap();
    assert!(doc.width() >= 8000 && doc.height() >= 6000);
    assert!(!doc.dirty());
    assert!(!get(&doc, "canUndo").as_bool().unwrap());
}

#[test]
fn formula_reference_bar_preview_is_transient_and_does_not_outline_other_sheets() {
    let mut doc = SheetDocument::create("References.xlsx", 0, 0).unwrap();
    doc.execute("editor.formula", json!({ "text": "=B5+'Other Sheet'!B10" })).unwrap();
    let refs = get(&doc, "formulaReferences");
    assert_eq!(refs.as_array().unwrap().len(), 2);
    let local: [u8; 4] = serde_json::from_value(refs[0]["color"].clone()).unwrap();
    let foreign: [u8; 4] = serde_json::from_value(refs[1]["color"].clone()).unwrap();
    let response = doc.state(0.0, 0.0, ToolMods::default(), 1.0);
    assert!(response.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { color, .. } if *color == local)));
    assert!(!response.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { color, .. } if *color == foreign)));
    assert!(!doc.dirty());
    // Typing in the formula bar is typing in the cell (M3, as WPS: the cell shows it, BAR-C13).
    assert_eq!(get(&doc, "editing"), true);
    doc.execute("editor.formula", json!({})).unwrap();
    assert_eq!(get(&doc, "formulaReferences"), json!([]));
    assert!(!doc.dirty());
    assert!(!get(&doc, "canUndo").as_bool().unwrap());
}

fn value(doc: &mut SheetDocument, cell: &str) -> Json {
    doc.execute("cell.get", json!({ "cell": cell })).unwrap()
}

#[test]
fn draws_the_grid_with_headings_and_values() {
    let doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
    assert_eq!(doc.width(), (HEADER_W + 26.0 * CW) as u32);
    assert_eq!(doc.height(), (HEADER_H + 100.0 * RH) as u32);
    let heading = doc.render(5.0, 30.0, 1.0, 1, 1).unwrap();
    assert_eq!(heading, HEADING);
    // B2 holds 1200, right-aligned: dark pixels at its right, none at its left.
    let (x, y) = centre(1, 1);
    let cell = doc.render(x - CW / 2.0, y - RH / 2.0, 1.0, CW as u32, RH as u32).unwrap();
    let dark = |range: std::ops::Range<usize>| {
        cell.as_chunks::<4>().0.iter().enumerate().filter(|(i, px)| range.contains(&(i % CW as usize)) && px[0] < 120).count()
    };
    assert!(dark(40..62) > 10, "the number is drawn at the right");
    assert_eq!(dark(2..20), 0);
}

#[test]
fn clicks_select_and_typing_enters_the_active_cell() {
    let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
    let response = click(&mut doc, centre(1, 1), false);
    assert_eq!(get(&doc, "active"), "B2");
    assert!(response.wants_text && response.busy);
    assert!(matches!(response.requests.first(), Some(ToolRequest::Status { text }) if text == "B2: 1200"));
    // Typing replaces the content; Enter enters it and moves down; the sum follows.
    let blank = doc.render(HEADER_W + CW, HEADER_H + RH, 1.0, CW as u32, RH as u32).unwrap();
    let revision = doc.revision();
    let typing = KitTools::text(&mut doc, "99", 1.0).unwrap();
    assert!(matches!(typing.requests.first(), Some(ToolRequest::Status { text }) if text == "B2: 99"));
    // The cell being typed is drawn in the cell (a new revision, so the page repaints it).
    assert!(doc.revision() > revision);
    assert_ne!(doc.render(HEADER_W + CW, HEADER_H + RH, 1.0, CW as u32, RH as u32).unwrap(), blank, "the typed text is drawn");
    assert!(!doc.dirty(), "nothing is entered until Enter");
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(get(&doc, "active"), "B3");
    assert!(doc.dirty());
    assert_eq!(value(&mut doc, "B4")["value"], 549.0);
    // Escape drops what is being typed.
    KitTools::text(&mut doc, "7", 1.0).unwrap();
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(value(&mut doc, "B3")["value"], 450.0);
    // Delete clears; undo brings it back.
    doc.key("Delete", ToolMods::default(), 1.0).unwrap();
    assert_eq!(value(&mut doc, "B4")["value"], 99.0);
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(value(&mut doc, "B4")["value"], 549.0);
}

#[test]
fn drags_shift_clicks_and_headings_select_ranges() {
    let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
    let (x, y) = centre(0, 0);
    let (tx, ty) = centre(1, 2);
    doc.pointer(&event(PointerAction::Down, x, y, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, tx, ty, false)).unwrap();
    let dragged = doc.pointer(&event(PointerAction::Up, tx, ty, false)).unwrap();
    assert_eq!(get(&doc, "selection"), "A1:B3");
    assert!(dragged.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Highlight { .. })));
    click(&mut doc, centre(0, 0), false);
    click(&mut doc, centre(2, 3), true);
    assert_eq!(get(&doc, "selection"), "A1:C4");
    // The heading of column B.
    click(&mut doc, (HEADER_W + CW + 10.0, 10.0), false);
    assert!(get(&doc, "selection").as_str().is_some_and(|range| range.starts_with("B1:B") || range == "B:B"), "{}", get(&doc, "selection"));
    // Arrows move; Shift extends.
    click(&mut doc, centre(0, 0), false);
    doc.key("ArrowDown", ToolMods::default(), 1.0).unwrap();
    doc.key("ArrowRight", ToolMods { shift: true, ..ToolMods::default() }, 1.0).unwrap();
    assert_eq!(get(&doc, "selection"), "A2:B2");
}

#[test]
fn the_formula_bar_enters_formulas_and_double_click_edits_in_place() {
    let mut doc = SheetDocument::create("New.xlsx", 0, 0).unwrap();
    assert!(!doc.dirty());
    doc.execute("command.runLine", json!({ "text": "21" })).unwrap();
    doc.execute("command.runLine", json!({ "text": "=A1*2" })).unwrap();
    assert_eq!(value(&mut doc, "A2")["value"], 42.0);
    assert_eq!(get(&doc, "active"), "A3");
    doc.pointer(&event(PointerAction::DoubleClick, centre(0, 1).0, centre(0, 1).1, false)).unwrap();
    assert_eq!(get(&doc, "editing"), true);
    let editing = doc.state(0.0, 0.0, ToolMods::default(), 1.0);
    assert!(matches!(editing.requests.first(), Some(ToolRequest::Status { text }) if text == "A2: =A1*2"));
    doc.key("Backspace", ToolMods::default(), 1.0).unwrap();
    KitTools::text(&mut doc, "3", 1.0).unwrap();
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(value(&mut doc, "A2")["value"], 63.0);
    assert_eq!(&doc.save(None).unwrap()[..2], b"PK");
}

#[test]
fn draws_chinese_with_a_font_added_at_run_time() {
    use kition_kit::KitDocument;
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "北极光北极光" })).unwrap();
    let ink = |doc: &SheetDocument| {
        let pixels = doc.render(0.0, 0.0, 2.0, 400, 120).unwrap();
        pixels.chunks(4).filter(|px| px[0] < 100 && px[1] < 100 && px[2] < 100).count()
    };
    let boxes = ink(&doc);
    assert_eq!(add_font(b"not a font"), 0);
    let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kition-fonts/fonts/droid-sans-fallback/DroidSansFallbackFull.ttf");
    let Ok(bytes) = std::fs::read(font) else { return };
    assert_eq!(add_font(&bytes), 1);
    // Real glyphs carry clearly more ink than the empty boxes the built-in face drew.
    assert!(ink(&doc) * 10 > boxes * 13, "{} vs {boxes}", ink(&doc));
}

/// How many pixels in the canvas area (x, y, w, h) at 1:1 are close to `color`.
fn pixels_like(doc: &SheetDocument, area: (f64, f64, u32, u32), color: [u8; 3]) -> usize {
    use kition_kit::KitDocument;
    let pixels = doc.render(area.0, area.1, 1.0, area.2, area.3).unwrap();
    pixels.chunks(4).filter(|px| (0..3).all(|i| (i32::from(px[i]) - i32::from(color[i])).abs() < 24)).count()
}

fn numbers() -> SheetDocument {
    use kition_kit::KitDocument;
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    for (row, value) in [10, 40, 70, 100].iter().enumerate() {
        doc.execute("cell.set", json!({ "cell": format!("A{}", row + 1), "input": value.to_string() })).unwrap();
    }
    doc
}

#[test]
fn draws_conditional_formats() {
    use kition_kit::KitDocument;
    let mut doc = numbers();
    let plain = pixels_like(&doc, (0.0, 0.0, 200, 120), [0x63, 0x8E, 0xC6]);
    doc.execute("conditionalFormat.apply", json!({ "range": "A1:A4", "rule": { "type": "dataBar" } })).unwrap();
    let barred = doc.render(0.0, 0.0, 1.0, 200, 120).unwrap();
    let blue = barred.chunks(4).filter(|px| px[2] > px[0].saturating_add(40)).count();
    assert!(blue > 200 && plain == 0, "data bars drew {blue} bluish pixels");

    let mut scaled = numbers();
    scaled.execute("conditionalFormat.apply", json!({ "range": "A1:A4", "rule": { "type": "colorScale" } })).unwrap();
    let pixels = scaled.render(0.0, 0.0, 1.0, 200, 120).unwrap();
    let tinted = pixels.chunks(4).filter(|px| px[0].abs_diff(px[1]) > 40 || px[1].abs_diff(px[2]) > 40).count();
    assert!(tinted > 500, "a color scale tinted {tinted} pixels");
}

#[test]
fn wraps_text_onto_more_lines() {
    use kition_kit::KitDocument;
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "one two three four five six seven" })).unwrap();
    doc.execute("row.setHeight", json!({ "rows": "2:2", "height": 80 })).unwrap();
    let ink = |doc: &SheetDocument, y: f64| {
        let pixels = doc.render(60.0, y, 1.0, 120, 20).unwrap();
        pixels.chunks(4).filter(|px| px[0] < 120).count()
    };
    let bottom_before = ink(&doc, 60.0);
    doc.execute("select.set", json!({ "range": "B2" })).unwrap();
    doc.execute("format.wrapText", json!({})).unwrap();
    doc.execute("format.alignTop", json!({})).unwrap();
    // Wrapped, the words fill lines further down the tall cell.
    assert!(ink(&doc, 40.0) + ink(&doc, 60.0) > bottom_before + 40, "{} {} {bottom_before}", ink(&doc, 40.0), ink(&doc, 60.0));
}

#[test]
fn draws_charts_over_the_cells() {
    use kition_kit::KitDocument;
    let mut doc = numbers();
    let blank = doc.render(200.0, 0.0, 1.0, 400, 300).unwrap();
    doc.execute("chart.insert", json!({ "range": "A1:A4", "type": "column", "at": "D2" })).unwrap();
    let drawn = doc.render(200.0, 0.0, 1.0, 400, 300).unwrap();
    let changed = blank.chunks(4).zip(drawn.chunks(4)).filter(|(a, b)| a != b).count();
    assert!(changed > 5_000, "the chart changed {changed} pixels");
    let _ = pixels_like(&doc, (0.0, 0.0, 1, 1), [0, 0, 0]);
}

#[test]
fn pins_the_headings_and_the_frozen_panes() {
    let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
    assert_eq!(doc.pinned(), (HEADER_W as u32, HEADER_H as u32));
    doc.execute("view.freezePanes", json!({ "cell": "C3" })).unwrap();
    assert_eq!(doc.pinned(), ((HEADER_W + 2.0 * CW) as u32, (HEADER_H + 2.0 * RH) as u32));
    // The split lines are drawn inside the frozen panes.
    let (left, top) = doc.pinned();
    assert_eq!(doc.render(f64::from(left) - 1.0, 100.0, 1.0, 1, 1).unwrap(), FREEZE_LINE);
    assert_eq!(doc.render(200.0, f64::from(top) - 1.0, 1.0, 1, 1).unwrap(), FREEZE_LINE);
    doc.execute("view.unfreezePanes", json!({})).unwrap();
    assert_eq!(doc.pinned(), (HEADER_W as u32, HEADER_H as u32));
}

#[test]
fn the_catalog_flags_as_mutating_exactly_what_changes_the_workbook() {
    let doc = SheetDocument::create("Flags.xlsx", 0, 0).unwrap();
    let mutates = |id: &str| doc.commands().into_iter().find(|command| command.id == id).unwrap_or_else(|| panic!("no {id}")).mutates;
    for id in [
        "cell.set",
        "cell.formatText",
        "format.bold",
        "edit.undo",
        "edit.redo",
        "file.new",
        "file.open",
        "file.save",
        "view.freezePanes",
        "view.gridlines",
        "command.runLine",
    ] {
        assert!(mutates(id), "{id} changes the workbook");
    }
    for id in ["select.set", "edit.selectAll", "edit.copy", "sheet.next", "view.zoom", "file.saveBytes", "file.exportPdf", "cell.get"] {
        assert!(!mutates(id), "{id} does not change the workbook");
    }
}

#[test]
fn commands_flagged_as_not_mutating_never_commit_to_the_replica() {
    let bytes = fixture();
    let commands = SheetDocument::open("Budget.xlsx", &bytes).unwrap().commands();
    // Commands that write files are left out: the test must not write any.
    for command in commands.iter().filter(|command| !command.mutates && !command.params.as_deref().unwrap_or_default().contains("path")) {
        let mut doc = SheetDocument::open("Budget.xlsx", &bytes).unwrap();
        doc.execute("edit.selectAll", json!({})).unwrap();
        let before = doc.version().unwrap();
        let _ = doc.execute(&command.id, json!({}));
        assert_eq!(doc.version().unwrap(), before, "{} committed to the replica", command.id);
    }
}

#[test]
fn draws_rich_text_runs_in_their_own_colours() {
    use kition_kit::KitDocument;
    use kition_sheet_engine::model::{Cell, RichText, RunColor, RunFont, StyleId, TextRun, Underline};
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "Plain then red" })).unwrap();
    let area = (HEADER_W + CW, HEADER_H + RH, 192, RH as u32);
    let red = [255, 0, 0];
    assert_eq!(pixels_like(&doc, area, red), 0, "plain text is black");
    let colour = RunFont {
        color: Some(RunColor { rgb: Some("FFFF0000".into()), ..RunColor::default() }),
        underline: Some(Underline::Single),
        ..RunFont::default()
    };
    let runs = RichText::new([TextRun::new("Plain then ", RunFont::default()), TextRun::new("red", colour)]).unwrap();
    let sheet = std::sync::Arc::make_mut(&mut doc.session.doc_mut().unwrap().wb).sheet_mut(0).unwrap();
    sheet.set_cell(CellRef::new(1, 1), Cell::rich_text(runs, StyleId::DEFAULT));
    let one_line = pixels_like(&doc, area, red);
    assert!(one_line > 10, "the red run is drawn red, underlined: {one_line} pixels");
    // Its underline: a red row of pixels under the run.
    let pixels = doc.render(area.0, area.1, 1.0, area.2, area.3).unwrap();
    let rows = pixels.as_chunks::<4>().0.chunks(area.2 as usize).map(|row| row.iter().filter(|px| px[0] > 200 && px[1] < 60).count()).max();
    assert!(rows.unwrap_or(0) >= 12, "an underline as wide as the run: {rows:?}");
    // Wrapped onto lines, the run keeps its colour.
    doc.execute("select.set", json!({ "range": "B2" })).unwrap();
    doc.execute("format.wrapText", json!({})).unwrap();
    doc.execute("row.setHeight", json!({ "rows": "2:2", "height": 80 })).unwrap();
    let wrapped = pixels_like(&doc, (HEADER_W + CW, HEADER_H + RH, CW as u32, 80), red);
    assert!(wrapped > 10, "wrapped, the red run is still red: {wrapped} pixels");
}

/// The ink in each half (top, bottom) of the canvas area, and the column its weight centres on.
fn ink_halves(doc: &SheetDocument, area: (f64, f64, u32, u32)) -> [(f64, f64); 2] {
    use kition_kit::KitDocument;
    let pixels = doc.render(area.0, area.1, 1.0, area.2, area.3).unwrap();
    let mut halves = [(0.0f64, 0.0f64); 2];
    for (i, px) in pixels.chunks(4).enumerate() {
        let ink = f64::from(255 - px[0].max(px[1]).max(px[2]));
        if ink > 40.0 {
            let (x, y) = (i % area.2 as usize, i / area.2 as usize);
            let half = &mut halves[usize::from(y >= area.3 as usize / 2)];
            half.0 += ink;
            half.1 += ink * x as f64;
        }
    }
    halves.map(|(weight, sum)| (weight, if weight > 0.0 { sum / weight } else { 0.0 }))
}

#[test]
fn italic_text_is_slanted() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "IIIIII" })).unwrap();
    // The text's upper and lower halves (the glyphs stand on the cell's baseline).
    let area = (HEADER_W + CW, HEADER_H + 2.0 * RH - 14.0, CW as u32, 12);
    let [top, bottom] = ink_halves(&doc, area);
    assert!(top.0 > 1000.0 && bottom.0 > 1000.0, "{top:?} {bottom:?}");
    let upright = top.1 - bottom.1;
    assert!(upright.abs() < 0.3, "upright: {top:?} {bottom:?}");
    doc.execute("select.set", json!({ "range": "B2" })).unwrap();
    doc.execute("format.italic", json!({})).unwrap();
    let [top, bottom] = ink_halves(&doc, area);
    assert!(top.1 - bottom.1 > upright + 0.7, "italic leans right: {top:?} {bottom:?}");
    // An italic run of rich text leans too.
    doc.execute("format.italic", json!({})).unwrap();
    doc.execute("cell.formatText", json!({ "cell": "B2", "toggle": "italic" })).unwrap();
    let [top, bottom] = ink_halves(&doc, area);
    assert!(top.1 - bottom.1 > upright + 0.7, "an italic run leans right: {top:?} {bottom:?}");
}

#[test]
fn selected_characters_of_the_cell_being_typed_take_font_commands() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "Hello world" })).unwrap();
    // F2 edits in place; Home, then Shift+Right five times selects "Hello".
    click(&mut doc, centre(1, 1), false);
    doc.key("F2", ToolMods::default(), 1.0).unwrap();
    doc.key("Home", ToolMods::default(), 1.0).unwrap();
    for _ in 0..5 {
        doc.key("ArrowRight", ToolMods { shift: true, ..ToolMods::default() }, 1.0).unwrap();
    }
    assert_eq!(get(&doc, "editing"), true);
    let revision = doc.revision();
    // Bold (as Cmd+B or the ribbon runs it) formats the selection, not the cell; the ribbon shows
    // the selection bold.
    assert_eq!(doc.execute("format.bold", json!({})).unwrap()["editing"], true);
    assert!(doc.revision() > revision, "drawn again");
    assert_eq!(get(&doc, "style")["font"]["bold"], true);
    doc.execute("format.color", json!({ "color": "#FF0000" })).unwrap();
    // While typing, the red run is drawn red in the cell.
    assert!(pixels_like(&doc, (HEADER_W + CW, HEADER_H + RH, CW as u32, RH as u32), [255, 0, 0]) > 10, "the editor shows the run's colour");
    assert_eq!(value(&mut doc, "B2")["runs"], Json::Null, "nothing entered yet");
    // Typing after the caret takes the run before it; Enter enters text and runs.
    doc.key("End", ToolMods::default(), 1.0).unwrap();
    KitTools::text(&mut doc, "!", 1.0).unwrap();
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    let cell = value(&mut doc, "B2");
    assert_eq!(cell["value"], "Hello world!");
    let runs = cell["runs"].as_array().cloned().unwrap_or_default();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!((runs[0]["text"].clone(), runs[0]["font"]["bold"].clone()), (json!("Hello"), json!(true)));
    assert_eq!(runs[0]["font"]["color"]["rgb"], "FFFF0000");
    assert_eq!(runs[1]["text"], " world!");
    // The cell's style is untouched; with no characters selected, Bold is the cell's.
    assert_eq!(get(&doc, "style")["font"]["bold"], false);
    // The runs are saved: the file has a bold run.
    let saved = SheetDocument::open("Saved.xlsx", &doc.save(None).unwrap()).unwrap();
    let mut saved = saved;
    assert_eq!(value(&mut saved, "B2")["runs"][0]["font"]["bold"], true);
    // Undo takes the entry back.
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(value(&mut doc, "B2")["runs"], Json::Null);
}

#[test]
fn clicks_and_drags_in_the_cell_being_typed_place_the_caret_and_select() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "alpha beta" })).unwrap();
    doc.pointer(&event(PointerAction::DoubleClick, centre(1, 1).0, centre(1, 1).1, false)).unwrap();
    assert_eq!(get(&doc, "editing"), true);
    let (x0, y) = (HEADER_W + CW + 2.5, centre(1, 1).1);
    // A press at the text's start places the caret there; a drag to the right selects.
    doc.pointer(&event(PointerAction::Down, x0, y, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, x0 + 200.0, y, false)).unwrap();
    doc.pointer(&event(PointerAction::Up, x0 + 200.0, y, false)).unwrap();
    assert_eq!(get(&doc, "editing"), true, "the press stayed in the cell being typed");
    assert_eq!(doc.editing.as_ref().map(Edit::selection), Some((0, 10)));
    // A double-click selects a word; Delete removes it.
    doc.pointer(&event(PointerAction::DoubleClick, x0 + 3.0, y, false)).unwrap();
    assert_eq!(doc.editing.as_ref().map(Edit::selection), Some((0, 5)));
    doc.key("Delete", ToolMods::default(), 1.0).unwrap();
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(value(&mut doc, "B2")["value"], " beta");
    // A press elsewhere enters the cell being typed and selects there.
    doc.pointer(&event(PointerAction::DoubleClick, centre(1, 1).0, centre(1, 1).1, false)).unwrap();
    KitTools::text(&mut doc, "X", 1.0).unwrap();
    click(&mut doc, centre(3, 5), false);
    assert_eq!(get(&doc, "editing"), false);
    assert_eq!(value(&mut doc, "B2")["value"], " betaX");
    assert_eq!(get(&doc, "active"), "D6");
}

#[test]
fn draws_shapes_with_their_transparency_geometry_and_text() {
    use kition_kit::KitDocument;
    use kition_sheet_engine::model::{Anchor, Color, CustomGeom, CustomPath, PathFill, Shape, ShapeKind};
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let blank = doc.render(0.0, 0.0, 1.0, 600, 300).unwrap();
    let d = doc.session.doc_mut().unwrap();
    let sheet = std::sync::Arc::make_mut(&mut d.wb).sheet_mut(0).unwrap();
    // A half see-through red box at C3, and a grouped green triangle drawn by its own path.
    sheet.shapes.push(Shape {
        id: 1,
        anchor: Anchor { cell: kition_sheet_engine::core::CellRef::new(2, 2), width: 128.0, height: 60.0, ..Default::default() },
        fill: Color::Rgb(0xFF0000),
        fill_alpha: Some(50_000),
        ..Default::default()
    });
    sheet.shapes.push(Shape {
        id: 2,
        anchor: Anchor { cell: kition_sheet_engine::core::CellRef::new(2, 6), width: 100.0, height: 100.0, ..Default::default() },
        fill: Color::Rgb(0x00FF00),
        groups: vec![1],
        geom: Some(CustomGeom {
            paths: vec![CustomPath { w: 2.0, h: 2.0, d: "M 0 2 L 1 0 L 2 2 Z".into(), fill: PathFill::Norm, stroke: true }],
            source: None,
        }),
        ..Default::default()
    });
    sheet.shapes.push(Shape {
        id: 3,
        kind: ShapeKind::TextBox,
        anchor: Anchor { cell: kition_sheet_engine::core::CellRef::new(8, 2), width: 200.0, height: 40.0, ..Default::default() },
        text: "Hello there".into(),
        ..Default::default()
    });
    let pink = pixels_like(&doc, (0.0, 0.0, 600, 300), [255, 128, 128]);
    assert!(pink > 128 * 60 / 2, "the box blends with the grid under it: {pink}");
    let green = pixels_like(&doc, (0.0, 0.0, 600, 300), [0, 255, 0]);
    // A triangle covers half its box.
    assert!(green > 4000 && green < 6000, "{green}");
    let drawn = doc.render(0.0, 0.0, 1.0, 600, 300).unwrap();
    let changed = blank.chunks(4).zip(drawn.chunks(4)).filter(|(a, b)| a != b).count();
    let text = doc.render(HEADER_W + 128.0, HEADER_H + 160.0, 1.0, 200, 40).unwrap();
    assert!(text.chunks(4).filter(|px| px[0] < 100).count() > 30, "the text box's text is drawn");
    assert!(changed > pink + green);
}

#[test]
fn art_text_fills_its_box_and_bends() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let frame = json!({ "x": 100, "y": 100, "w": 300, "h": 120 });
    let id =
        doc.execute("drawing.insert", json!({ "kind": "textBox", "frame": frame, "text": "Art text" })).unwrap()["id"].as_str().unwrap().to_string();
    let d = doc.execute("drawing.describe", json!({ "ids": [id] })).unwrap();
    let f = &d["objects"][0]["frame"];
    let (x, y, w, h) = (f["x"].as_f64().unwrap(), f["y"].as_f64().unwrap(), f["w"].as_f64().unwrap(), f["h"].as_f64().unwrap());
    // Dark pixels in the box's bottom quarter, where flat text at the top does not reach.
    let low = |doc: &SheetDocument| {
        // Frames are in the grid's pixels.
        let (bx, by, bw, bh) = (HEADER_W + x, HEADER_H + y, w, h);
        let img = doc.render(bx, by + bh * 0.75, 1.0, bw as u32, (bh * 0.2) as u32).unwrap();
        img.chunks(4).filter(|p| p[0] < 100 && p[1] < 100).count()
    };
    let flat = low(&doc);
    doc.execute("drawing.text", json!({ "id": id, "warp": "textInflate" })).unwrap();
    let bent = low(&doc);
    assert!(flat < 20 && bent > 200, "flat {flat}, bent {bent}");
    let d = doc.execute("drawing.describe", json!({ "ids": [id] })).unwrap();
    assert_eq!(d["objects"][0]["text"]["warp"]["preset"], "textInflate");
    // Its adjust value is drawn: a shallow inflate leaves the bottom edge's corners empty.
    doc.execute("drawing.text", json!({ "id": id, "warp": { "preset": "textInflate", "adjust": { "adj": 2000 } } })).unwrap();
    assert_ne!(low(&doc), bent);
}

#[test]
fn draws_office_fonts_in_their_metric_compatible_stand_ins() {
    use ab_glyph::Font;
    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kition-fonts/fonts");
    let (Ok(carlito), Ok(arimo)) = (std::fs::read(fonts.join("carlito/Carlito-Regular.ttf")), std::fs::read(fonts.join("arimo/Arimo-Variable.ttf")))
    else {
        return;
    };
    // Kept out of the faces every document draws with (other tests compare renderings).
    let mut families = draw::stand_ins(&carlito).unwrap();
    assert_eq!(families.len(), 1);
    // A variable face joins as its regular and bold instances.
    families.extend(draw::stand_ins(&arimo).unwrap());
    assert_eq!(families.len(), 3);
    assert!(draw::stand_ins(b"not a font").is_none());
    let faces = draw::Faces::load().unwrap();
    let pick = |name: &str, bold: bool| faces.pick_from(&families, name, bold, false);
    // In units of a 2048-unit em.
    let advance = |face: &ab_glyph::FontArc, c: char| face.h_advance_unscaled(face.glyph_id(c)) * 2048.0 / face.units_per_em().unwrap_or(2048.0);
    // Calibri's digits are 1038 units wide in 2048, Arial's 1139: Excel's column widths follow them.
    let calibri = pick("Calibri", false);
    assert!((advance(&calibri.face, '0') - 1038.0).abs() < 1.0, "{}", advance(&calibri.face, '0'));
    let arial = pick("arial", false);
    assert!((advance(&arial.face, '0') - 1139.0).abs() < 1.0);
    // Carlito's bold was not added, so it is synthesised; Arimo's bold instance needs none.
    assert!(pick("Calibri", true).bold);
    let bold = pick("Arial", true);
    assert!(!bold.bold && !bold.italic);
    assert_ne!(advance(&bold.face, 'w'), advance(&arial.face, 'w'));
    // Other fonts keep the built-in faces.
    assert!((advance(&pick("Verdana", false).face, '0') - advance(&faces.sans, '0')).abs() < 0.01);
}

/// Clicking another sheet's tab while a cell is being typed enters the typing on the sheet it was
/// typed on, and the other sheet opens with nothing typed (reference case UI-C86, UI-C87). Before, the
/// typing went with the person to the next sheet and landed in its active cell on Enter.
#[test]
fn switching_sheets_enters_the_typing_on_its_own_sheet() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("sheet.insert", json!({ "name": "Two" })).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": 0 })).unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "测试", 1.0).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": "Two" })).unwrap();
    assert_eq!(get(&doc, "sheet"), json!("Two"));
    assert_eq!(get(&doc, "editing"), json!(false));
    assert!(value(&mut doc, "A1")["input"].as_str().unwrap_or_default().is_empty(), "nothing typed on Two");
    assert_eq!(value(&mut doc, "Sheet1!A1")["input"], json!("测试"));
    // A formula left open is entered as the cell takes it (its parenthesis closed) or dropped:
    // never a half-typed formula, and never on the other sheet.
    click(&mut doc, centre(0, 1), false);
    KitTools::text(&mut doc, "=SUM(B1:B3", 1.0).unwrap();
    doc.execute("sheet.previous", json!({})).unwrap();
    assert_eq!(get(&doc, "sheet"), json!("Sheet1"));
    assert_eq!(get(&doc, "editing"), json!(false));
    let entered = value(&mut doc, "Two!A2")["input"].as_str().unwrap_or_default().to_owned();
    assert!(entered.is_empty() || entered == "=SUM(B1:B3)", "{entered}");
    assert!(value(&mut doc, "Sheet1!A2")["input"].as_str().unwrap_or_default().is_empty());
}

/// `inspect` gives the sheet tabs what they show: every sheet in order, hidden ones marked, tab
/// colours and locks; and the status bar's sum, average and count for a selection of more than
/// one cell (reference case UI-C16, UI-C17, UI-C70, UI-C102).
#[test]
fn inspect_gives_the_tabs_and_the_selection_statistics() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "A1", "values": [[1], [2], [3]] })).unwrap();
    for name in ["Two", "Three"] {
        doc.execute("sheet.insert", json!({ "name": name })).unwrap();
    }
    doc.execute("sheet.tabColor", json!({ "sheet": "Sheet1", "color": "#FF0000" })).unwrap();
    doc.execute("sheet.hide", json!({ "sheet": "Three" })).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": "Two" })).unwrap();
    doc.execute("sheet.protect", json!({})).unwrap();
    let tabs = get(&doc, "tabs");
    assert_eq!(tabs[1]["protected"], json!(true), "{tabs}");
    assert_eq!(tabs.as_array().map(Vec::len), Some(3), "{tabs}");
    assert_eq!(tabs[0]["color"], json!("#FF0000"));
    assert_eq!(tabs[2]["hidden"], json!(true));
    assert_eq!(tabs[0]["hidden"], json!(false));
    doc.execute("sheet.activate", json!({ "sheet": "Sheet1" })).unwrap();
    doc.execute("select.set", json!({ "range": "A1:A3" })).unwrap();
    let stats = get(&doc, "stats");
    assert_eq!((stats["sum"].as_f64(), stats["count"].as_u64(), stats["average"].as_f64()), (Some(6.0), Some(3), Some(2.0)), "{stats}");
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    assert_eq!(get(&doc, "stats"), Json::Null, "one cell has no statistics");
}

/// Switching sheets is view state: it neither marks the workbook unsaved nor makes the replica a
/// viewer's session would read as an edit (the first switch in a read-only document was refused
/// as "view only", reference case UI-C69).
#[test]
fn switching_sheets_makes_no_replica_and_no_change() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("sheet.insert", json!({ "name": "Two" })).unwrap();
    let bytes = doc.save(Some("Book.xlsx")).unwrap();
    let mut doc = SheetDocument::open("Book.xlsx", &bytes).unwrap();
    assert!(!doc.has_replica());
    for sheet in ["Sheet1", "Two", "Sheet1"] {
        doc.execute("sheet.activate", json!({ "sheet": sheet })).unwrap();
        doc.execute("sheet.next", json!({})).unwrap();
    }
    assert!(!doc.has_replica(), "a sheet switch made a replica");
    assert!(!doc.dirty(), "a sheet switch made the workbook unsaved");
}

#[test]
fn grouped_rows_keep_a_loadable_replica() {
    let mut doc = SheetDocument::open("Book.xlsx", include_bytes!("../../../conformance/basic.xlsx")).unwrap();
    doc.execute("range.setValues", json!({"range":"X1:Y3","values":[["Item","Amount"],["First",12],["Second",24]]})).unwrap();
    doc.execute("data.group", json!({"rows":"5:6"})).unwrap();
    kdm_model::SheetSession::create(&doc.doc_state().unwrap().wb).unwrap();
    doc.snapshot().unwrap();
}

fn key(doc: &mut SheetDocument, key: &str, shift: bool) {
    doc.key(key, ToolMods { shift, ..ToolMods::default() }, 1.0).unwrap();
}

fn formula_of(doc: &mut SheetDocument, cell: &str) -> String {
    value(doc, cell)["input"].as_str().unwrap_or_default().to_string()
}

/// Point mode (F1, as WPS): typing a formula where a reference can go, a click, a drag, Shift and
/// the arrow keys point at cells and insert their references; any other key ends pointing.
#[test]
fn pointing_at_cells_while_typing_a_formula_inserts_their_references() {
    let mut doc = SheetDocument::create("Point.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(3, 0), false);
    KitTools::text(&mut doc, "=SUM(", 1.0).unwrap();
    // A press and a drag point at a range; the active cell stays the one being typed.
    doc.pointer(&event(PointerAction::Down, centre(0, 1).0, centre(0, 1).1, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, centre(1, 3).0, centre(1, 3).1, false)).unwrap();
    let response = doc.pointer(&event(PointerAction::Up, centre(1, 3).0, centre(1, 3).1, false)).unwrap();
    assert_eq!(get(&doc, "editText"), json!("=SUM(A2:B4"));
    assert_eq!(get(&doc, "active"), json!("D1"));
    assert!(response.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { dashed: true, .. })), "the pointed range has a dashed frame");
    // Another click right after replaces the reference; typing ends pointing.
    click(&mut doc, centre(2, 5), false);
    assert_eq!(get(&doc, "editText"), json!("=SUM(C6"));
    KitTools::text(&mut doc, ",", 1.0).unwrap();
    click(&mut doc, centre(0, 0), false);
    click(&mut doc, centre(0, 2), true);
    assert_eq!(get(&doc, "editText"), json!("=SUM(C6,A1:A3"));
    KitTools::text(&mut doc, ")", 1.0).unwrap();
    // Not where a reference can go: a click enters the formula and selects the cell.
    click(&mut doc, centre(5, 5), false);
    assert_eq!(formula_of(&mut doc, "D1"), "=SUM(C6,A1:A3)");
    assert_eq!(get(&doc, "active"), json!("F6"));
    // Arrow keys in Enter mode point from the cell being typed; Shift widens.
    KitTools::text(&mut doc, "=", 1.0).unwrap();
    key(&mut doc, "ArrowUp", false);
    key(&mut doc, "ArrowUp", false);
    key(&mut doc, "ArrowLeft", true);
    assert_eq!(get(&doc, "editText"), json!("=E4:F4"));
    KitTools::text(&mut doc, "*2", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(formula_of(&mut doc, "F6"), "=E4:F4*2");
    // A column heading points at the whole column.
    click(&mut doc, centre(7, 0), false);
    KitTools::text(&mut doc, "=SUM(", 1.0).unwrap();
    click(&mut doc, (HEADER_W + CW + 10.0, HEADER_H / 2.0), false);
    assert_eq!(get(&doc, "editText"), json!("=SUM(B:B"));
    key(&mut doc, "Escape", false);
}

/// Clicking another sheet's tab while a formula waits for a reference keeps it open: that sheet's
/// cells are pointed at with its name, and Enter puts the formula in its own cell and returns.
#[test]
fn pointing_at_another_sheets_cells() {
    let mut doc = SheetDocument::create("Sheets.xlsx", 0, 0).unwrap();
    doc.execute("sheet.insert", json!({ "name": "数据 表" })).unwrap();
    doc.execute("cell.set", json!({ "sheet": "数据 表", "cell": "B2", "input": "21" })).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": "Sheet1" })).unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "=", 1.0).unwrap();
    doc.execute("sheet.activate", json!({ "sheet": "数据 表" })).unwrap();
    assert_eq!(get(&doc, "editing"), json!(true));
    assert_eq!(get(&doc, "editCell"), json!({ "sheet": "Sheet1", "cell": "A1" }));
    click(&mut doc, centre(1, 1), false);
    assert_eq!(get(&doc, "editText"), json!("='数据 表'!B2"));
    KitTools::text(&mut doc, "*2", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(get(&doc, "sheet"), json!("Sheet1"));
    assert_eq!(formula_of(&mut doc, "A1"), "='数据 表'!B2*2");
    assert_eq!(value(&mut doc, "A1")["value"], json!(42.0));
    // Escape on the other sheet drops the formula and returns too.
    click(&mut doc, centre(0, 1), false);
    KitTools::text(&mut doc, "=1+", 1.0).unwrap();
    doc.execute("sheet.next", json!({})).unwrap();
    assert_eq!(get(&doc, "sheet"), json!("数据 表"));
    key(&mut doc, "Escape", false);
    assert_eq!(get(&doc, "sheet"), json!("Sheet1"));
    assert_eq!(formula_of(&mut doc, "A2"), "");
    // Text that is no formula is entered when the tab is clicked, as before.
    KitTools::text(&mut doc, "plain", 1.0).unwrap();
    doc.execute("sheet.next", json!({})).unwrap();
    assert_eq!(get(&doc, "editing"), json!(false));
}

/// F4 cycles the reference at the caret; the function list and argument hint follow the caret;
/// Tab or a click takes a function.
#[test]
fn f4_function_list_and_argument_hint_in_the_cell_editor() {
    let mut doc = SheetDocument::create("Hints.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "=B2", 1.0).unwrap();
    key(&mut doc, "F4", false);
    assert_eq!(get(&doc, "editText"), json!("=$B$2"));
    key(&mut doc, "F4", false);
    key(&mut doc, "F4", false);
    key(&mut doc, "F4", false);
    assert_eq!(get(&doc, "editText"), json!("=B2"));
    KitTools::text(&mut doc, "+vlo", 1.0).unwrap();
    let hint = get(&doc, "formulaHint");
    assert_eq!(hint["functions"][0]["name"], json!("VLOOKUP"));
    assert_eq!(hint["replace"], json!([4, 7]));
    assert!(hint["at"].is_array());
    key(&mut doc, "Tab", false);
    assert_eq!(get(&doc, "editText"), json!("=B2+VLOOKUP("));
    KitTools::text(&mut doc, "1,", 1.0).unwrap();
    let call = &get(&doc, "formulaHint")["call"];
    assert_eq!((call["name"].clone(), call["argument"].clone()), (json!("VLOOKUP"), json!(1)));
    KitTools::text(&mut doc, "su", 1.0).unwrap();
    key(&mut doc, "ArrowDown", false);
    assert_eq!(get(&doc, "formulaHint")["pick"], json!(1));
    doc.execute("editor.acceptFunction", json!({ "name": "SUM" })).unwrap();
    assert_eq!(get(&doc, "editText"), json!("=B2+VLOOKUP(1,SUM("));
    // The formula bar's F4 and hint.
    let cycled = doc.execute("editor.cycleReference", json!({ "text": "=中+A1", "start": 5, "end": 5 })).unwrap();
    assert_eq!(cycled, json!({ "text": "=中+$A$1", "start": 7, "end": 7 }));
    let preview = doc.execute("editor.formula", json!({ "text": "=IF(", "start": 4, "end": 4 })).unwrap();
    assert_eq!(preview["hint"]["call"]["name"], json!("IF"));
    doc.execute("editor.formula", json!({})).unwrap();
}

/// A function taken from the list, then its arguments typed and Enter: the cell gets the formula.
#[test]
fn a_function_taken_from_the_list_is_entered_with_its_arguments() {
    let mut doc = SheetDocument::create("Accept.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(0, 9), false);
    KitTools::text(&mut doc, "=su", 1.0).unwrap();
    doc.execute("editor.acceptFunction", json!({ "name": "SUM" })).unwrap();
    KitTools::text(&mut doc, "1,2)", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(formula_of(&mut doc, "A10"), "=SUM(1,2)");
    assert_eq!(get(&doc, "active"), json!("A11"));
}

/// FX-C6 (the editor routes keys by the last tool answer it has, which may be older than the
/// latest call): every tool answer from the click on the cell to Enter, the function list's click
/// (`editor.acceptFunction`, which answers no tool response) and the `ai/inspect` calls between the
/// keys included, says the engine takes typing (`wantsText`), so no answer the editor holds can
/// send a typed key elsewhere; and the inspect calls leave the cell being typed alone.
#[test]
fn every_tool_answer_from_the_function_list_to_enter_takes_typing() {
    let mut doc = SheetDocument::create("Accept.xlsx", 0, 0).unwrap();
    let mut answers = vec![click(&mut doc, centre(0, 9), false)];
    for ch in ["=", "s", "u"] {
        answers.push(KitTools::text(&mut doc, ch, 1.0).unwrap());
        doc.inspect().unwrap();
    }
    answers.push(KitTools::state(&mut doc, centre(0, 9).0, centre(0, 9).1, ToolMods::default(), 1.0));
    doc.execute("editor.acceptFunction", json!({ "name": "SUM" })).unwrap();
    doc.inspect().unwrap();
    answers.push(KitTools::state(&mut doc, centre(0, 9).0, centre(0, 9).1, ToolMods::default(), 1.0));
    for ch in ["1", ",", "2", ")"] {
        doc.inspect().unwrap();
        answers.push(KitTools::text(&mut doc, ch, 1.0).unwrap());
        answers.push(KitTools::state(&mut doc, centre(0, 9).0, centre(0, 9).1, ToolMods::default(), 1.0));
    }
    assert_eq!(get(&doc, "editText"), json!("=SUM(1,2)"));
    answers.push(doc.key("Enter", ToolMods::default(), 1.0).unwrap());
    let refused: Vec<usize> = answers.iter().enumerate().filter(|(_, a)| !a.wants_text).map(|(i, _)| i).collect();
    assert!(refused.is_empty(), "answers {refused:?} of {} say the engine does not take typing", answers.len());
    assert_eq!(formula_of(&mut doc, "A10"), "=SUM(1,2)");
}

/// The formula bar (M3, as WPS): what is typed there is the cell being typed, so the cell shows it
/// (BAR-C13), pointing at cells inserts their references at the bar's caret, ✓ enters it and stays
/// (BAR-C12), Enter moves down, ✕ and Escape drop it.
#[test]
fn the_formula_bar_types_in_the_cell_and_points_at_cells() {
    let mut doc = SheetDocument::create("Bar.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "abc" })).unwrap();
    click(&mut doc, centre(1, 1), false);
    let typed = doc.execute("editor.formula", json!({ "text": "abcdef" })).unwrap();
    assert_eq!((typed["editing"].clone(), typed["text"].clone(), typed["start"].clone()), (json!(true), json!("abcdef"), json!(6)));
    assert_eq!(get(&doc, "editText"), json!("abcdef"));
    assert_eq!(get(&doc, "editCaret"), json!(6));
    assert!(doc.edit_box(0.0, 0.0, 1.0).is_some(), "the cell shows what the bar types");
    assert_eq!(formula_of(&mut doc, "B2"), "abc", "not entered while typed");
    // ✓: entered, the selection stays.
    doc.execute("editor.formula", json!({ "enter": "stay" })).unwrap();
    assert_eq!(formula_of(&mut doc, "B2"), "abcdef");
    assert_eq!(get(&doc, "active"), json!("B2"));
    // ✕: dropped.
    doc.execute("editor.formula", json!({ "text": "abcdefXYZ" })).unwrap();
    doc.execute("editor.formula", json!({})).unwrap();
    assert_eq!(formula_of(&mut doc, "B2"), "abcdef");
    assert_eq!(get(&doc, "editing"), json!(false));
    // A formula typed in the bar points at the cell clicked; the caret follows the reference.
    doc.execute("cell.set", json!({ "cell": "C1", "input": "5" })).unwrap();
    click(&mut doc, centre(0, 4), false);
    doc.execute("editor.formula", json!({ "text": "=SUM(" })).unwrap();
    click(&mut doc, centre(2, 0), false);
    assert_eq!(get(&doc, "editText"), json!("=SUM(C1"));
    assert_eq!(get(&doc, "pointing"), json!(true));
    assert_eq!(get(&doc, "editCaret"), json!(7));
    assert_eq!(get(&doc, "active"), json!("A5"), "pointing leaves the cell being typed active");
    // Another click replaces the reference just pointed at.
    click(&mut doc, centre(1, 1), false);
    assert_eq!(get(&doc, "editText"), json!("=SUM(B2"));
    let state = doc.execute("editor.formula", json!({ "text": "=SUM(B2,C1)", "start": 11, "end": 11 })).unwrap();
    assert_eq!(state["hint"], Json::Null);
    assert_eq!(get(&doc, "pointing"), json!(false), "typing ends pointing");
    doc.execute("editor.formula", json!({ "enter": "down" })).unwrap();
    assert_eq!(formula_of(&mut doc, "A5"), "=SUM(B2,C1)");
    assert_eq!(get(&doc, "active"), json!("A6"));
    // Shift+Enter, Tab, Shift+Tab.
    doc.execute("editor.formula", json!({ "text": "1", "enter": "right" })).unwrap();
    assert_eq!((formula_of(&mut doc, "A6"), get(&doc, "active")), ("1".to_owned(), json!("B6")));
    doc.execute("editor.formula", json!({ "text": "2", "enter": "up" })).unwrap();
    assert_eq!((formula_of(&mut doc, "B6"), get(&doc, "active")), ("2".to_owned(), json!("B5")));
    doc.execute("editor.formula", json!({ "text": "3", "enter": "left" })).unwrap();
    assert_eq!((formula_of(&mut doc, "B5"), get(&doc, "active")), ("3".to_owned(), json!("A5")));
    // Peeking changes nothing.
    let peek = doc.execute("editor.formula", json!({ "peek": true })).unwrap();
    assert_eq!(peek["editing"], json!(false));
}

/// Retyping in the bar keeps the fonts of the characters it did not change.
#[test]
fn the_formula_bar_keeps_the_fonts_of_untouched_characters() {
    let mut doc = SheetDocument::create("Runs.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "North 北区" })).unwrap();
    doc.execute("cell.formatText", json!({ "cell": "A1", "start": 6, "end": 8, "toggle": "italic" })).unwrap();
    doc.execute("editor.formula", json!({ "text": "North! 北区" })).unwrap();
    doc.execute("editor.formula", json!({ "enter": "stay" })).unwrap();
    let runs = value(&mut doc, "A1")["runs"].clone();
    let italic: String =
        runs.as_array().unwrap().iter().filter(|r| r["font"]["italic"] == json!(true)).map(|r| r["text"].as_str().unwrap()).collect();
    assert_eq!(italic, "北区", "{runs}");
}

/// Insert Function with a function chosen goes in at the caret of the formula being typed, or
/// starts one; F2 (`edit.beginEdit`) edits the active cell in place (BAR-C14).
#[test]
fn insert_function_and_f2_type_in_the_active_cell() {
    let mut doc = SheetDocument::create("Functions.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(0, 0), false);
    doc.execute("formula.insertFunction", json!({ "name": "sum" })).unwrap();
    assert_eq!(get(&doc, "editText"), json!("=SUM("));
    assert!(get(&doc, "formulaHint")["call"]["name"] == json!("SUM"));
    doc.execute("editor.formula", json!({ "text": "=SUM(1,2)+" })).unwrap();
    doc.execute("formula.insertFunction", json!({ "name": "MAX" })).unwrap();
    assert_eq!(get(&doc, "editText"), json!("=SUM(1,2)+MAX("));
    KitTools::text(&mut doc, "4)", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(value(&mut doc, "A1")["value"], json!(7.0));
    doc.execute("cell.set", json!({ "cell": "B1", "input": "kept" })).unwrap();
    click(&mut doc, centre(1, 0), false);
    doc.execute("edit.beginEdit", json!({})).unwrap();
    assert_eq!(get(&doc, "editText"), json!("kept"));
    key(&mut doc, "Escape", false);
    assert_eq!(get(&doc, "editing"), json!(false));
}

/// Alt+Enter (Option+Return) breaks the line in the cell being typed: the editor shows two lines,
/// and the cell gets the line break and wraps.
#[test]
fn alt_enter_breaks_the_line_in_the_cell() {
    let mut doc = SheetDocument::create("Lines.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "first", 1.0).unwrap();
    doc.key("Enter", ToolMods { alt: true, ..ToolMods::default() }, 1.0).unwrap();
    KitTools::text(&mut doc, "second", 1.0).unwrap();
    assert_eq!(get(&doc, "editText"), json!("first\nsecond"));
    let lines = doc.edit_box(0.0, 0.0, 1.0).unwrap();
    assert_eq!(lines.lines.len(), 2);
    assert!(lines.lines[1].baseline > lines.lines[0].baseline);
    assert!(lines.rect.y1 - lines.rect.y0 > 20.0, "the box is taller than one row");
    // A click on the second line puts the caret there.
    let at = doc.edit_index(lines.start + 1.0, lines.lines[1].baseline - 3.0, false).unwrap();
    assert_eq!(at, 6);
    key(&mut doc, "Enter", false);
    assert_eq!(formula_of(&mut doc, "A1"), "first\nsecond");
    assert_eq!(get(&doc, "active"), json!("A2"));
    let wrap = value(&mut doc, "A1")["style"]["align"]["wrap"].clone();
    assert_eq!(wrap, json!(true));
}

/// View › Show: the formula bar (the person's) and the sheet's gridlines and headings.
#[test]
fn inspect_gives_the_show_settings() {
    let mut doc = SheetDocument::create("Show.xlsx", 0, 0).unwrap();
    assert_eq!((get(&doc, "formulaBar"), get(&doc, "gridlines"), get(&doc, "headings")), (json!(true), json!(true), json!(true)));
    doc.execute("view.formulaBar", json!({})).unwrap();
    assert_eq!(get(&doc, "formulaBar"), json!(false));
    assert!(!doc.dirty() && !doc.has_replica(), "the formula bar is not the workbook's");
    doc.execute("view.gridlines", json!({ "on": false })).unwrap();
    assert_eq!((get(&doc, "formulaBar"), get(&doc, "gridlines")), (json!(false), json!(false)));
    // View › Zoom: the editor zooms the view each time it is set, even to the same percentage.
    assert_eq!((get(&doc, "zoom"), get(&doc, "zoomAsked")), (json!(100), json!(0)));
    doc.execute("view.zoom", json!({ "percent": 150 })).unwrap();
    doc.execute("view.zoom", json!({ "percent": 150 })).unwrap();
    assert_eq!((get(&doc, "zoom"), get(&doc, "zoomAsked")), (json!(150), json!(2)));
}

/// A right-click on the grid (`editor.contextMenu`, module M4): a cell outside the selection is
/// selected, one inside keeps it; a heading selects its row or column; the corner selects the
/// whole sheet (not the region around the active cell); what was being typed goes in first; the
/// insert menus read the selection's size from `lines`.
#[test]
fn a_right_click_selects_what_it_presses_and_names_its_menu() {
    let mut doc = SheetDocument::create("Menu.xlsx", 0, 0).unwrap();
    for (cell, input) in [("A1", "1"), ("A2", "2"), ("B1", "3")] {
        doc.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
    }
    let menu = |doc: &mut SheetDocument, (x, y): (f64, f64)| doc.execute("editor.contextMenu", json!({ "x": x, "y": y })).unwrap()["target"].clone();
    doc.execute("select.set", json!({ "range": "B2:C4" })).unwrap();
    assert_eq!(menu(&mut doc, centre(2, 3)), json!("cell"));
    assert_eq!(get(&doc, "selection"), json!("B2:C4"));
    assert_eq!(get(&doc, "lines"), json!({ "rows": 3, "cols": 2, "wholeRows": false, "wholeCols": false }));
    assert_eq!(menu(&mut doc, centre(5, 7)), json!("cell"));
    assert_eq!(get(&doc, "selection"), json!("F8"));
    // Row 5's heading, then column C's.
    assert_eq!(menu(&mut doc, (HEADER_W / 2.0, HEADER_H + 4.0 * RH + RH / 2.0)), json!("rows"));
    assert_eq!(get(&doc, "selection"), json!("5:5"));
    assert_eq!(menu(&mut doc, (HEADER_W + 2.0 * CW + CW / 2.0, HEADER_H / 2.0)), json!("cols"));
    assert_eq!(get(&doc, "selection"), json!("C:C"));
    // The corner: the whole sheet, by right-click and by click.
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    assert_eq!(menu(&mut doc, (2.0, 2.0)), json!("all"));
    assert_eq!(get(&doc, "lines")["wholeRows"], json!(true));
    assert_eq!(get(&doc, "lines")["wholeCols"], json!(true));
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    click(&mut doc, (2.0, 2.0), false);
    assert_eq!(get(&doc, "lines")["wholeCols"], json!(true));
    // Shift+click on a heading extends to whole columns (rows); a drag along the headings too.
    click(&mut doc, (HEADER_W + 2.0 * CW + CW / 2.0, HEADER_H / 2.0), false);
    click(&mut doc, (HEADER_W + 4.0 * CW + CW / 2.0, HEADER_H / 2.0), true);
    assert_eq!(get(&doc, "selection"), json!("C:E"));
    doc.pointer(&event(PointerAction::Down, HEADER_W / 2.0, HEADER_H + 2.0 * RH + RH / 2.0, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, HEADER_W / 2.0, HEADER_H + 5.0 * RH + RH / 2.0, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, centre(3, 6).0, centre(3, 6).1, false)).unwrap();
    doc.pointer(&event(PointerAction::Up, centre(3, 6).0, centre(3, 6).1, false)).unwrap();
    assert_eq!(get(&doc, "selection"), json!("3:7"));
    // Typing, then a right-click: the text goes in first.
    click(&mut doc, centre(3, 0), false);
    KitTools::text(&mut doc, "x", 1.0).unwrap();
    menu(&mut doc, centre(3, 5));
    assert_eq!(get(&doc, "editing"), json!(false));
    assert_eq!(doc.execute("cell.get", json!({ "cell": "D1" })).unwrap()["value"], json!("x"));
    // Inserting rows while a cell is typed enters it first, into the cell it was typed in.
    click(&mut doc, centre(3, 2), false);
    KitTools::text(&mut doc, "y", 1.0).unwrap();
    doc.execute("row.insert", json!({})).unwrap();
    assert_eq!(doc.execute("cell.get", json!({ "cell": "D4" })).unwrap()["value"], json!("y"));
}

/// View › 高亮行列 (module M5, HL-C7, HL-C9, HL-C31): the rows and columns of the selection's area,
/// across the grid but not over the area itself, tinted in the person's colour as overlays (so they
/// follow the panes of frozen panes and stay out of the document's pixels); off, none.
#[test]
fn the_highlight_tints_the_selections_rows_and_columns_as_overlays() {
    let mut doc = SheetDocument::create("Focus.xlsx", 0, 0).unwrap();
    doc.execute("select.set", json!({ "range": "C6" })).unwrap();
    let tints = |doc: &mut SheetDocument| {
        doc.state(0.0, 0.0, ToolMods::default(), 1.0)
            .overlays
            .into_iter()
            .filter_map(|o| match o {
                ToolOverlay::Highlight { quad, color } if color[3] >= 90 && color[3] != SELECTION[3] => Some((quad, color)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(tints(&mut doc).is_empty(), "off by default");
    let pixels_before = doc.render(0.0, 0.0, 1.0, 300, 200).unwrap();
    doc.execute("view.focusCell", json!({ "on": true })).unwrap();
    let on = tints(&mut doc);
    assert_eq!(on.len(), 4, "left and right of the cell in its row, above and below it in its column");
    let (x0, y0) = (HEADER_W + 2.0 * CW, HEADER_H + 5.0 * RH);
    for (quad, color) in &on {
        assert_eq!(color[..3], [0xFB, 0xE9, 0xC6]);
        let inside = |x: f64, y: f64| quad[0][0] <= x && x <= quad[2][0] && quad[0][1] <= y && y <= quad[2][1];
        assert!(!inside(x0 + 30.0, y0 + 10.0), "the active cell itself is not tinted");
        assert!(quad[0][0] >= HEADER_W && quad[0][1] >= HEADER_H, "nothing over the headings");
    }
    assert_eq!(doc.render(0.0, 0.0, 1.0, 300, 200).unwrap(), pixels_before, "never drawn into the document's pixels");
    // A range: its rows and columns.
    doc.execute("select.set", json!({ "range": "B2:D4" })).unwrap();
    let range = tints(&mut doc);
    assert!(range.iter().any(|(q, _)| (q[0][1] - (HEADER_H + RH)).abs() < 0.5 && (q[2][1] - (HEADER_H + 4.0 * RH)).abs() < 0.5));
    doc.execute("view.focusCell", json!({ "color": "#3D7FE0" })).unwrap();
    assert!(tints(&mut doc).iter().all(|(_, c)| c[..3] == [0x3D, 0x7F, 0xE0] && c[3] < 200), "a deep colour is fainter");
    doc.execute("view.focusCell", json!({ "on": false })).unwrap();
    assert!(tints(&mut doc).is_empty());
    assert!(!doc.dirty());
}

/// Pointing at a choice of the Freeze Panes menu (module M5, FRZ-C7, FRZ-C15, FRZ-C23): the rows and
/// columns it would freeze show tinted with their line, as overlays; nothing freezes, and a freeze
/// chosen ends the preview.
#[test]
fn the_freeze_menu_previews_its_choice() {
    let mut doc = SheetDocument::create("Freeze.xlsx", 0, 0).unwrap();
    let preview = |doc: &mut SheetDocument| {
        doc.state(0.0, 0.0, ToolMods::default(), 1.0)
            .overlays
            .into_iter()
            .filter(|o| matches!(o, ToolOverlay::Highlight { color, .. } if *color == FREEZE_PREVIEW_TINT))
            .count()
    };
    assert_eq!(doc.execute("editor.freezePreview", json!({ "rows": 5, "cols": 2 })).unwrap(), json!({ "preview": [5, 2] }));
    assert_eq!(preview(&mut doc), 2);
    assert_eq!(doc.inspect().unwrap()["freezePreview"], json!([5, 2]));
    assert_eq!(doc.inspect().unwrap()["freeze"], json!([0, 0]), "a preview freezes nothing");
    assert!(!doc.dirty());
    doc.execute("editor.freezePreview", json!({ "rows": 1 })).unwrap();
    assert_eq!(preview(&mut doc), 1);
    doc.execute("view.freezeTopRow", json!({})).unwrap();
    assert_eq!(preview(&mut doc), 0, "the choice ends the preview");
    assert_eq!(doc.inspect().unwrap()["freeze"], json!([1, 0]));
    doc.execute("editor.freezePreview", json!({ "rows": 3 })).unwrap();
    doc.execute("editor.freezePreview", json!({})).unwrap();
    assert_eq!(preview(&mut doc), 0);
    assert_eq!(doc.inspect().unwrap()["freezePreview"], Json::Null);
    // Hostile counts stay within the sheet.
    doc.execute("editor.freezePreview", json!({ "rows": u64::MAX, "cols": 1e9 })).unwrap();
}

/// The gridline colour (module M5, GRID-X1) draws the gridlines; hidden gridlines draw none.
#[test]
fn gridlines_take_the_sheets_colour() {
    let mut doc = SheetDocument::create("Grid.xlsx", 0, 0).unwrap();
    let line = |doc: &SheetDocument| doc.render(HEADER_W + CW, HEADER_H + 30.0, 1.0, 1, 1).unwrap();
    assert_eq!(line(&doc), GRIDLINE);
    doc.execute("view.gridlineColor", json!({ "color": "#3366FF" })).unwrap();
    assert_eq!(line(&doc), [0x33, 0x66, 0xFF, 255]);
    assert_eq!(doc.inspect().unwrap()["gridColor"], json!("#3366FF"));
    doc.execute("view.gridlines", json!({ "on": false })).unwrap();
    assert_eq!(line(&doc), WHITE);
}

/// The freeze lines stayed one pixel wide at 1:1, so a view zoomed out (an exact reduction of 1:1)
/// showed them barely darker than a gridline (module M5, FRZ-C69). They are two pixels wide now.
#[test]
fn the_freeze_lines_show_when_zoomed_out() {
    let mut doc = SheetDocument::create("Zoomed.xlsx", 0, 0).unwrap();
    doc.execute("view.freezePanes", json!({ "cell": "C6" })).unwrap();
    let (left, top) = doc.pinned();
    // At 50% each pixel is a 2 × 2 block of 1:1: the line fills a whole pixel.
    let row = doc.render(300.0, f64::from(top) - 2.0, 0.5, 1, 1).unwrap();
    let col = doc.render(f64::from(left) - 2.0, 300.0, 0.5, 1, 1).unwrap();
    assert_eq!(row, FREEZE_LINE);
    assert_eq!(col, FREEZE_LINE);
}

/// The format painter in the grid (module M6, FP-C6 to FP-C25): a press and release on a cell, a
/// drag over a range or a heading paints what was picked up; the pointer shows the brush while it
/// is up; Esc puts it down; once it is down a click only selects.
#[test]
fn the_format_painter_paints_what_is_pressed_and_esc_puts_it_down() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("format.bold", json!({ "range": "A1", "on": true })).unwrap();
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    doc.execute("edit.formatPainter", json!({})).unwrap();
    assert_eq!(get(&doc, "formatPainter"), json!("once"));
    assert!(get(&doc, "escape").as_bool().unwrap_or(false), "Esc has the painter to put down");
    let hover = doc.pointer(&event(PointerAction::Move, centre(3, 3).0, centre(3, 3).1, false)).unwrap();
    assert!(hover.cursor.contains("url("), "FP-C9: the brush pointer: {}", hover.cursor);
    click(&mut doc, centre(1, 0), false);
    let bold = |doc: &SheetDocument, cell: &str| {
        doc.sheet().is_some_and(|(wb, sheet)| CellRef::parse(cell).is_some_and(|c| wb.styles.get(sheet.style_id(c)).font.bold))
    };
    assert!(bold(&doc, "B1"), "FP-C6: B1 is painted");
    assert_eq!(get(&doc, "formatPainter"), Json::Null, "FP-C6: once, then down");
    assert_eq!(doc.state(0.0, 0.0, ToolMods::default(), 1.0).cursor, "cell");
    click(&mut doc, centre(2, 0), false);
    assert!(!bold(&doc, "C1"), "FP-C12: C1 only selected");

    // Sticky: a drag over a range, then a row heading; Esc ends it.
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    doc.execute("edit.formatPainter", json!({ "sticky": true })).unwrap();
    doc.pointer(&event(PointerAction::Down, centre(1, 2).0, centre(1, 2).1, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, centre(3, 4).0, centre(3, 4).1, false)).unwrap();
    doc.pointer(&event(PointerAction::Up, centre(3, 4).0, centre(3, 4).1, false)).unwrap();
    assert!(["B3", "D5", "C4"].iter().all(|c| bold(&doc, c)) && !bold(&doc, "E5"), "FP-C25: the range dragged over is painted");
    click(&mut doc, (HEADER_W / 2.0, HEADER_H + 7.0 * RH + RH / 2.0), false);
    assert!(bold(&doc, "Q8"), "FP-C23: a row heading paints the row");
    assert_eq!(get(&doc, "formatPainter"), json!("sticky"));
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert_eq!(get(&doc, "formatPainter"), Json::Null, "Esc puts it down");
    click(&mut doc, centre(5, 9), false);
    assert!(!bold(&doc, "F10"));
}

/// While a cell is typed the ribbon shows the font, size and colour of the selected characters
/// (FONT-C33): the cell's where they have none of their own, nothing when the fonts differ.
#[test]
fn the_ribbon_shows_the_selected_characters_font() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "字体样式测试" })).unwrap();
    doc.execute("cell.formatText", json!({ "cell": "A1", "start": 2, "end": 4, "name": "楷体", "size": 18 })).unwrap();
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    doc.edit_in_place(CellRef::new(0, 0));
    let font = |doc: &SheetDocument| get(doc, "style")["font"].clone();
    if let Some(edit) = doc.editing.as_mut() {
        edit.move_to(3, false);
    }
    assert_eq!((font(&doc)["name"].clone(), font(&doc)["size"].clone()), (json!("楷体"), json!(18.0)), "the caret in 样式");
    if let Some(edit) = doc.editing.as_mut() {
        edit.move_to(1, false);
    }
    assert_eq!(font(&doc)["name"], json!("Calibri"), "the caret in plain text: the cell's font");
    if let Some(edit) = doc.editing.as_mut() {
        edit.move_to(0, false);
        edit.move_to(4, true);
    }
    assert_eq!(font(&doc)["name"], json!(""), "two fonts selected: none shown");
}

/// Frozen panes made in a scrolled view start at its top-left (carried from M5): the rows above
/// are out of view, the frozen rows come first under the headings, and the pinned band is theirs.
#[test]
fn panes_frozen_in_a_scrolled_view_start_at_its_top_left() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A11", "input": "top" })).unwrap();
    doc.execute("select.set", json!({ "range": "C15" })).unwrap();
    // The editor says the view is scrolled to row 11 (10 rows of the default height).
    doc.execute("editor.viewport", json!({ "right": 1200, "bottom": 900, "x": 0, "y": 10.0 * RH })).unwrap();
    doc.execute("view.freezeRows", json!({})).unwrap();
    assert_eq!(get(&doc, "freeze"), json!([4, 0]));
    assert_eq!(get(&doc, "freezeShift"), json!([0.0, 10.0 * RH]));
    assert_eq!(doc.pinned().1, (HEADER_H + 4.0 * RH).round() as u32, "rows 11–14 are pinned");
    // The first row under the headings is row 11: a click there selects A11.
    click(&mut doc, centre(0, 0), false);
    assert_eq!(get(&doc, "active"), json!("A11"));
    let rect = get(&doc, "activeRect");
    assert_eq!(rect[1], json!(HEADER_H), "A11 is drawn at the top");
    doc.execute("view.unfreezePanes", json!({})).unwrap();
    assert_eq!(get(&doc, "freezeShift"), json!([0.0, 0.0]));
}

/// Cmd+click (Ctrl+click) adds cells to the selection, as WPS and Excel do (FONT-C29, FONT-C106):
/// a font command then formats every area.
#[test]
fn a_command_click_adds_a_cell_to_the_selection() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(0, 0), false);
    let command =
        |action, (x, y): (f64, f64)| ToolPointer { action, x, y, mods: ToolMods { command: true, ..ToolMods::default() }, pressure: 1.0, zoom: 1.0 };
    doc.pointer(&command(PointerAction::Down, centre(2, 2))).unwrap();
    doc.pointer(&command(PointerAction::Up, centre(2, 2))).unwrap();
    doc.pointer(&command(PointerAction::Down, centre(4, 1))).unwrap();
    doc.pointer(&command(PointerAction::Drag, centre(5, 3))).unwrap();
    doc.pointer(&command(PointerAction::Up, centre(5, 3))).unwrap();
    assert_eq!(get(&doc, "selection"), json!("A1,C3,E2:F4"));
    assert_eq!(get(&doc, "active"), json!("E2"));
    doc.execute("format.italic", json!({})).unwrap();
    let italic = |doc: &SheetDocument, cell: &str| {
        doc.sheet().is_some_and(|(wb, sheet)| CellRef::parse(cell).is_some_and(|c| wb.styles.get(sheet.style_id(c)).font.italic))
    };
    assert!(["A1", "C3", "F4"].iter().all(|c| italic(&doc, c)) && !italic(&doc, "B2"));
}

/// A menu item run without the values it needs asks for its dialog: the editor reads the request
/// from inspect (each once, by its count) and opens Format Cells, Sort, Define Name… (FC-X1).
#[test]
fn a_command_without_its_values_asks_the_editor_for_its_dialog() {
    let mut doc = SheetDocument::create("Dialogs.xlsx", 0, 0).unwrap();
    assert_eq!(get(&doc, "dialog"), Json::Null);
    doc.execute("format.cells", json!({})).unwrap();
    let asked = get(&doc, "dialog");
    assert_eq!((asked["name"].clone(), asked["command"].clone(), asked["seq"].clone()), (json!("formatCells"), json!("format.cells"), json!(1)));
    doc.execute("name.define", json!({})).unwrap();
    assert_eq!((get(&doc, "dialog")["name"].clone(), get(&doc, "dialog")["seq"].clone()), (json!("defineName"), json!(2)));
    // An agent's call never opens a dialog.
    doc.execute_as("format.cells", json!({}), &kition_sheet_engine::Origin::agent("agent:test")).unwrap();
    assert_eq!(get(&doc, "dialog")["seq"], json!(2));
    // With its values it runs, and asks for nothing.
    doc.execute("format.cells", json!({ "style": { "font": { "bold": true } } })).unwrap();
    assert_eq!(get(&doc, "dialog")["seq"], json!(2));
    // The active table's options, for the Table Design tab.
    doc.execute("range.setValues", json!({ "range": "A1", "values": [["a", "b"], [1, 2]] })).unwrap();
    doc.execute("table.formatAs", json!({ "range": "A1:B2", "style": "TableStyleLight9" })).unwrap();
    let t = get(&doc, "tableInfo");
    assert_eq!((t["style"].clone(), t["header"].clone(), t["bandedRows"].clone()), (json!("TableStyleLight9"), json!(true), json!(true)));
}

/// Checkbox cells in the grid (module M8): a press on the box ticks or clears it, a press beside it
/// only selects; Space toggles; typing, a double-click and F2 leave it, with WPS's notice; the
/// formula bar and Insert Link stay shut there.
#[test]
fn checkbox_cells_toggle_on_their_box_and_refuse_typing() {
    let mut doc = SheetDocument::create("Boxes.xlsx", 0, 0).unwrap();
    doc.execute("checkbox.insert", json!({ "range": "A1:A2" })).unwrap();
    let (cx, cy) = centre(0, 0);
    click(&mut doc, (cx, cy), false);
    assert_eq!(value(&mut doc, "A1")["value"], json!(true), "a press on the box ticks it");
    assert_eq!(get(&doc, "checkbox"), json!(true));
    click(&mut doc, (HEADER_W + 5.0, cy), false);
    assert_eq!(value(&mut doc, "A1")["value"], json!(true), "beside the box: only selected");
    assert_eq!(get(&doc, "active"), json!("A1"));
    KitTools::text(&mut doc, " ", 1.0).unwrap();
    assert_eq!(value(&mut doc, "A1")["value"], json!(false), "Space clears it");
    KitTools::text(&mut doc, "已完成", 1.0).unwrap();
    assert_eq!(get(&doc, "editing"), json!(false), "typing is refused");
    assert_eq!(value(&mut doc, "A1")["value"], json!(false));
    let notice = get(&doc, "notice");
    assert_eq!(notice["text"], json!(kition_sheet_engine::cmd::extra::CHECKBOX_LOCKED), "{notice}");
    doc.pointer(&event(PointerAction::DoubleClick, HEADER_W + 5.0, cy, false)).unwrap();
    key(&mut doc, "F2", false);
    assert_eq!(get(&doc, "editing"), json!(false), "a double-click and F2 don't open it");
    assert!(get(&doc, "notice")["seq"].as_u64() > notice["seq"].as_u64(), "F2 says why again");
    doc.execute("editor.formula", json!({ "text": "x" })).unwrap();
    assert_eq!(get(&doc, "editing"), json!(false), "nor the formula bar");
    assert!(doc.execute("link.insert", json!({ "target": "https://example.com" })).is_err());
    // A plain cell: typed as before.
    click(&mut doc, centre(1, 0), false);
    assert_eq!(get(&doc, "checkbox"), json!(false));
    KitTools::text(&mut doc, "a", 1.0).unwrap();
    assert_eq!(get(&doc, "editing"), json!(true));
}

/// Insert › Symbol in the editor (WPS): at the caret of the cell being typed; with nothing typed,
/// after the cell's text with the cell left open; an agent's goes straight in. A date (Ctrl+;)
/// takes the cell's place; a checkbox refuses both.
#[test]
fn symbols_go_in_at_the_caret_or_after_the_text() {
    let mut doc = SheetDocument::create("Symbols.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "温度" })).unwrap();
    doc.execute("symbol.insert", json!({ "char": "℃" })).unwrap();
    assert_eq!((get(&doc, "editing"), get(&doc, "editText")), (json!(true), json!("温度℃")), "the cell opens with the symbol after its text");
    doc.execute("symbol.insert", json!({ "char": "±" })).unwrap();
    key(&mut doc, "ArrowLeft", false);
    doc.execute("symbol.insert", json!({ "char": "≈" })).unwrap();
    assert_eq!(get(&doc, "editText"), json!("温度℃≈±"), "at the caret");
    key(&mut doc, "Enter", false);
    assert_eq!(value(&mut doc, "A1")["value"], json!("温度℃≈±"));
    // An agent's goes straight into the active cell.
    doc.execute("select.set", json!({ "range": "B1" })).unwrap();
    doc.execute_as("symbol.insert", json!({ "char": "≠" }), &kition_sheet_engine::Origin::agent("agent:test")).unwrap();
    assert_eq!((get(&doc, "editing"), value(&mut doc, "B1")["value"].clone()), (json!(false), json!("≠")));
    // A date takes the cell's place.
    doc.execute("editor.insertText", json!({ "text": "2026/11/10" })).unwrap();
    assert_eq!(get(&doc, "editText"), json!("2026/11/10"));
    key(&mut doc, "Escape", false);
    doc.execute("checkbox.insert", json!({ "range": "C1" })).unwrap();
    doc.execute("select.set", json!({ "range": "C1" })).unwrap();
    let answer = doc.execute("symbol.insert", json!({ "char": "±" })).unwrap();
    assert!(answer["refused"].is_string() && get(&doc, "editing") == json!(false), "{answer}");
    assert!(doc.execute("editor.insertText", json!({ "text": "" })).is_err());
}

/// A text box's text (FONT-C111, FONT-C123, FONT-C54, INSC-C93): the Home font commands with it
/// selected format all of its text, leaving the cells alone; a symbol goes in at its end.
#[test]
fn font_commands_and_symbols_reach_a_selected_text_box() {
    let mut doc = SheetDocument::create("Box.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "cell" })).unwrap();
    let id =
        doc.execute("drawing.insert", json!({ "kind": "textBox", "text": "Box", "frame": { "x": 300, "y": 60, "w": 120, "h": 40 } })).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
    click(&mut doc, (HEADER_W + 360.0, HEADER_H + 80.0), false);
    assert_eq!(get(&doc, "shape"), json!(id));
    doc.execute("format.bold", json!({})).unwrap();
    doc.execute("format.fontName", json!({ "name": "楷体" })).unwrap();
    let runs = |doc: &mut SheetDocument| {
        doc.execute("drawing.describe", json!({ "ids": [id] })).unwrap()["objects"][0]["text"]["paragraphs"][0]["runs"].clone()
    };
    let r = runs(&mut doc);
    assert_eq!((r[0]["bold"].clone(), r[0]["font"].clone()), (json!(true), json!("楷体")), "{r}");
    assert_ne!(value(&mut doc, "A1")["style"]["font"]["bold"], json!(true), "the cells are left alone");
    doc.execute("format.bold", json!({})).unwrap();
    assert_eq!(runs(&mut doc)[0].get("bold"), None, "a second Bold takes it off");
    doc.execute("symbol.insert", json!({ "char": "＋" })).unwrap();
    assert_eq!(get(&doc, "shapeEditing"), json!(true));
    key(&mut doc, "Escape", false);
    let text = doc.execute("drawing.describe", json!({ "ids": [id] })).unwrap()["objects"][0]["text"].clone();
    assert!(text.to_string().contains("Box＋"), "{text}");
}

/// IMP-C39 (WPS, Excel): a value typed into a cell its data validation refuses stays in the cell
/// being typed, with the rule's message; a value it allows goes in.
#[test]
fn typed_values_are_checked_against_data_validation() {
    let mut doc = SheetDocument::create("Rules.xlsx", 0, 0).unwrap();
    doc.execute("data.validation", json!({ "range": "A1", "type": "list", "formula1": "\"是,否\"", "errorMessage": "只能选择是或否" })).unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "也许", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(value(&mut doc, "A1")["value"], json!(null), "the refused value stays out");
    assert_eq!(get(&doc, "editing"), json!(true), "the cell stays open to be corrected");
    assert_eq!(get(&doc, "notice")["text"], json!("只能选择是或否"));
    key(&mut doc, "Escape", false);
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "是", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(value(&mut doc, "A1")["value"], json!("是"));
}

/// Cell text is drawn at its point size (an em of 11 pt is 14.67 px): a Chinese character's ink is
/// most of an em tall. ab_glyph's own scale is the face's ascent-to-descent height, which had
/// drawn text a fifth too small (module M9, against WPS's rendering).
#[test]
fn text_is_drawn_at_its_point_size() {
    let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kition-fonts/fonts/droid-sans-fallback/DroidSansFallbackFull.ttf");
    let Ok(bytes) = std::fs::read(font) else { return };
    add_font(&bytes);
    let mut doc = SheetDocument::create("Size.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "国" })).unwrap();
    doc.execute("format.fontSize", json!({ "range": "A1", "size": 72 })).unwrap();
    let (w, h) = (300u32, 200u32);
    let px = doc.render(HEADER_W, HEADER_H, 1.0, w, h).unwrap();
    let rows: Vec<u32> = (0..h).filter(|y| (0..w).any(|x| px.get(((y * w + x) * 4) as usize).is_some_and(|v| *v < 100))).collect();
    let ink = rows.last().copied().unwrap_or(0) - rows.first().copied().unwrap_or(0);
    // 72 pt is 96 px; the character's ink is most of that (it was about 70 px).
    assert!(ink > 80, "ink {ink} px tall");
}

/// File › Export as Picture: the sheet's cells as a PNG, on a white margin, without headings.
#[test]
fn the_sheet_exports_as_a_picture() {
    let mut doc = SheetDocument::create("Picture.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "C3", "input": "x" })).unwrap();
    doc.execute("format.fillColor", json!({ "range": "C3", "color": "#FF0000" })).unwrap();
    let answer = doc.execute("file.export", json!({ "format": "png" })).unwrap();
    assert_eq!((answer["extension"].as_str(), answer["mime"].as_str()), (Some("png"), Some("image/png")));
    let png = kition_sheet_engine::io::base64_decode(answer["data"].as_str().unwrap()).unwrap();
    let picture = image::load_from_memory(&png).unwrap().to_rgba8();
    // The used range (C3) at 1:1 (a default column and row) with 20 px of margin all round.
    assert_eq!(picture.dimensions(), (CW as u32 + 40, RH as u32 + 40));
    assert_eq!(picture.get_pixel(20 + 30, 20 + 10).0, [255, 0, 0, 255], "C3's fill");
    assert_eq!(picture.get_pixel(5, 5).0, [255, 255, 255, 255], "no headings in the margin");
    assert!(doc.execute("file.export", json!({ "format": "png", "range": "nope" })).is_err());
}

/// A pointer event with modifier keys held.
fn event_with(action: PointerAction, (x, y): (f64, f64), mods: ToolMods) -> ToolPointer {
    ToolPointer { action, x, y, mods, pressure: 1.0, zoom: 1.0 }
}

/// Drags from `from` to `to` (canvas points) with `mods` held, in a few steps.
fn drag_with(doc: &mut SheetDocument, from: (f64, f64), to: (f64, f64), mods: ToolMods) {
    doc.pointer(&event_with(PointerAction::Down, from, mods)).unwrap();
    for i in 1..=4 {
        let t = f64::from(i) / 4.0;
        doc.pointer(&event_with(PointerAction::Drag, (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t), mods)).unwrap();
    }
    doc.pointer(&event_with(PointerAction::Up, to, mods)).unwrap();
}

/// CLN-X1 (WPS, Excel): the selection's border is grabbed with the move cursor and dragged; the
/// cells land where it is dropped, and a formula that read them follows.
#[test]
fn dragging_the_selections_border_moves_its_cells() {
    let mut doc = SheetDocument::create("Move.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "5" })).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "=B2*2" })).unwrap();
    click(&mut doc, centre(1, 1), false);
    // The left border of B2.
    let border = (HEADER_W + CW + 1.0, centre(1, 1).1);
    let hover = doc.pointer(&event(PointerAction::Move, border.0, border.1, false)).unwrap();
    assert_eq!(hover.cursor, "move", "the move cursor over the border");
    assert_eq!(doc.pointer(&event(PointerAction::Move, centre(1, 1).0, centre(1, 1).1, false)).unwrap().cursor, "cell", "inside: the cell cursor");
    // Dragged two columns right and three rows down: D5.
    let to = (border.0 + 2.0 * CW, border.1 + 3.0 * RH);
    doc.pointer(&event(PointerAction::Down, border.0, border.1, false)).unwrap();
    let during = doc.pointer(&event(PointerAction::Drag, to.0, to.1, false)).unwrap();
    assert!(during.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { dashed: true, .. })), "a dashed frame where it lands");
    assert!(matches!(during.requests.first(), Some(ToolRequest::Status { text }) if text == "D5"), "{:?}", during.requests);
    doc.pointer(&event(PointerAction::Up, to.0, to.1, false)).unwrap();
    assert_eq!(value(&mut doc, "D5")["value"], json!(5.0));
    assert_eq!(value(&mut doc, "B2")["value"], json!(null));
    assert_eq!(formula_of(&mut doc, "A1"), "=D5*2");
    assert_eq!(get(&doc, "selection"), json!("D5"));
    // A press on the border released in place is a click.
    let border = (HEADER_W + 3.0 * CW + 1.0, centre(3, 4).1);
    click(&mut doc, border, false);
    assert_eq!(value(&mut doc, "D5")["value"], json!(5.0));
}

/// CLN-X2: Cmd (Ctrl) copies; a drop onto cells that hold something asks first (WPS's
/// 「是否替换目标单元格内容？」), and the answer runs `edit.moveCells`.
#[test]
fn a_drag_onto_data_asks_and_cmd_copies() {
    let mut doc = SheetDocument::create("Move.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "a" })).unwrap();
    doc.execute("cell.set", json!({ "cell": "A3", "input": "old" })).unwrap();
    click(&mut doc, centre(0, 0), false);
    let border = (centre(0, 0).0, HEADER_H + 1.0);
    let copy = ToolMods { command: true, ..ToolMods::default() };
    assert_eq!(doc.pointer(&event_with(PointerAction::Move, border, copy)).unwrap().cursor, "copy");
    drag_with(&mut doc, border, (border.0, border.1 + 4.0 * RH), copy);
    assert_eq!((value(&mut doc, "A1")["value"].clone(), value(&mut doc, "A5")["value"].clone()), (json!("a"), json!("a")), "a copy");
    // Onto A3, which holds something: asked, nothing moved yet.
    click(&mut doc, centre(0, 0), false);
    drag_with(&mut doc, border, (border.0, border.1 + 2.0 * RH), ToolMods::default());
    let asked = get(&doc, "dialog");
    assert_eq!(asked["name"], json!("moveCells"));
    assert_eq!(asked["params"]["to"], json!("A3"));
    assert_eq!(value(&mut doc, "A3")["value"], json!("old"));
    doc.execute("edit.moveCells", asked["params"].clone()).unwrap();
    assert_eq!((value(&mut doc, "A1")["value"].clone(), value(&mut doc, "A3")["value"].clone()), (json!(null), json!("a")));
}

/// CLN-X3: Shift makes room: the block goes in between the rows (a thick line shows where).
#[test]
fn shift_dragging_the_border_inserts_the_cells() {
    let mut doc = SheetDocument::create("Move.xlsx", 0, 0).unwrap();
    for (i, v) in ["a", "b", "c", "d"].iter().enumerate() {
        doc.execute("cell.set", json!({ "cell": format!("A{}", i + 1), "input": v })).unwrap();
    }
    click(&mut doc, centre(0, 0), false);
    let shift = ToolMods { shift: true, ..ToolMods::default() };
    let border = (centre(0, 0).0, HEADER_H + 1.0);
    // Dropped just above the line between rows 3 and 4.
    let to = (centre(0, 0).0, HEADER_H + 3.0 * RH - 2.0);
    doc.pointer(&event_with(PointerAction::Down, border, shift)).unwrap();
    let during = doc.pointer(&event_with(PointerAction::Drag, to, shift)).unwrap();
    assert!(during.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { dashed: false, width, .. } if *width >= 3.0)), "a line where it goes in");
    doc.pointer(&event_with(PointerAction::Up, to, shift)).unwrap();
    let column: Vec<Json> = ["A1", "A2", "A3", "A4"].iter().map(|c| value(&mut doc, c)["value"].clone()).collect();
    assert_eq!(column, [json!("b"), json!("c"), json!("a"), json!("d")]);
}

/// CLN-X7: a filter button's press opens the editor's filter list for its column (the sheet's
/// filter and a table's alike), and does not select.
#[test]
fn a_filter_buttons_press_asks_for_its_list() {
    let mut doc = SheetDocument::create("Filter.xlsx", 0, 0).unwrap();
    for (cell, input) in [("A1", "名称"), ("B1", "数量"), ("A2", "x"), ("B2", "1")] {
        doc.execute("cell.set", json!({ "cell": cell, "input": input })).unwrap();
    }
    doc.execute("table.insert", json!({ "range": "A1:B2" })).unwrap();
    doc.execute("select.set", json!({ "range": "B2" })).unwrap();
    // B1's button sits at the right of the cell.
    let button = (HEADER_W + 2.0 * CW - 8.0, HEADER_H + RH - 7.0);
    click(&mut doc, button, false);
    let asked = get(&doc, "dialog");
    assert_eq!(asked["name"], json!("filterMenu"));
    assert_eq!(asked["params"]["cell"], json!("B1"));
    assert_eq!(get(&doc, "selection"), json!("B2"), "the press does not select");
    // Beside the button: an ordinary click.
    click(&mut doc, (HEADER_W + CW + 10.0, HEADER_H + RH / 2.0), false);
    assert_eq!(get(&doc, "selection"), json!("B1"));
}

/// CLN-X11 (WPS): a value its cell's rule refuses is held and the editor asks with the rule's
/// style: a Stop rule retries or cancels; a Warning's Yes enters it and moves on.
#[test]
fn a_refused_value_waits_for_the_answer_to_the_rules_message() {
    let mut doc = SheetDocument::create("Rules.xlsx", 0, 0).unwrap();
    doc.execute("data.validation", json!({ "range": "A1", "type": "whole", "operator": "between", "formula1": "1", "formula2": "10", "errorStyle": "warning", "errorTitle": "超出范围", "errorMessage": "请输入 1 到 10" })).unwrap();
    doc.execute("data.validation", json!({ "range": "A2", "type": "list", "formula1": "\"是,否\"", "inputTitle": "选择", "inputMessage": "是或否" }))
        .unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "20", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    let asked = get(&doc, "dialog");
    assert_eq!(asked["name"], json!("validationError"));
    assert_eq!((asked["params"]["style"].clone(), asked["params"]["title"].clone()), (json!("warning"), json!("超出范围")));
    assert_eq!((get(&doc, "editing"), get(&doc, "active")), (json!(true), json!("A1")), "held in its cell");
    doc.execute("editor.validation", json!({ "answer": "accept" })).unwrap();
    assert_eq!(value(&mut doc, "A1")["value"], json!(20.0), "Yes: it goes in");
    assert_eq!(get(&doc, "active"), json!("A2"), "and Enter's move happens");
    // A2 shows its list's arrow (entries) and its input message.
    let validation = get(&doc, "validation");
    assert_eq!(validation["list"], json!(["是", "否"]));
    assert_eq!(validation["prompt"]["title"], json!("选择"));
    assert_eq!(validation["rule"]["type"], json!("list"));
    // Stop (the default): Retry keeps it, Cancel drops it.
    KitTools::text(&mut doc, "也许", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(get(&doc, "dialog")["params"]["style"], json!("stop"));
    doc.execute("editor.validation", json!({ "answer": "accept" })).unwrap();
    assert_eq!(get(&doc, "editing"), json!(true), "a Stop rule never lets it in");
    doc.execute("editor.validation", json!({ "answer": "cancel" })).unwrap();
    assert_eq!((get(&doc, "editing"), value(&mut doc, "A2")["value"].clone()), (json!(false), json!(null)));
}

/// CLN-X17 (WPS, Excel): a formula typed that makes a new circular reference asks the editor to
/// warn once; the cell shows 0; with iterative calculation on there is no warning.
#[test]
fn a_new_circular_reference_is_warned_about_once() {
    let mut doc = SheetDocument::create("Loop.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(0, 2), false);
    KitTools::text(&mut doc, "=A3*2", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    let asked = get(&doc, "dialog");
    assert_eq!((asked["name"].clone(), asked["params"]["cell"].clone()), (json!("circularReference"), json!("A3")));
    assert_eq!(value(&mut doc, "A3")["value"], json!(0.0));
    let seq = asked["seq"].clone();
    // Another entry asks nothing.
    KitTools::text(&mut doc, "5", 1.0).unwrap();
    key(&mut doc, "Enter", false);
    assert_eq!(get(&doc, "dialog")["seq"], seq);
}

/// The text and caret of the cell being typed sit centred up and down in a tall cell (at its top
/// when the cell is top-aligned), not on its bottom edge.
#[test]
fn the_cell_being_typed_is_centred_in_a_tall_row() {
    use kition_kit::KitDocument;
    let mut doc = SheetDocument::create("Tall.xlsx", 0, 0).unwrap();
    doc.execute("row.setHeight", json!({ "rows": "1:1", "height": 40 })).unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "x", 1.0).unwrap();
    let b = doc.edit_box(0.0, 0.0, 1.0).unwrap();
    let line = &b.lines[0];
    let (above, below) = (line.baseline - b.ascent - b.rect.y0, b.rect.y1 - (line.baseline + b.descent));
    assert!((above - below).abs() < 1.0, "centred: {above} above, {below} below");
    assert!(above > 5.0, "the row is taller than the text");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    doc.execute("format.alignTop", json!({})).unwrap();
    click(&mut doc, centre(0, 0), false);
    KitTools::text(&mut doc, "x", 1.0).unwrap();
    let b = doc.edit_box(0.0, 0.0, 1.0).unwrap();
    assert!(b.lines[0].baseline - b.ascent - b.rect.y0 < 2.0, "top-aligned stays at the top");
}

/// Over the cell being typed the pointer is the text cursor (the cell cross elsewhere), and a
/// drag that stays within one character does not repaint the view.
#[test]
fn the_cell_being_typed_takes_the_text_cursor_and_drags_repaint_only_when_the_caret_moves() {
    use kition_kit::KitDocument;
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "alpha beta" })).unwrap();
    doc.pointer(&event(PointerAction::DoubleClick, centre(1, 1).0, centre(1, 1).1, false)).unwrap();
    let (x0, y) = (HEADER_W + CW + 2.5, centre(1, 1).1);
    let over = doc.pointer(&event(PointerAction::Move, x0 + 10.0, y, false)).unwrap();
    assert_eq!(over.cursor, "text");
    let away = doc.pointer(&event(PointerAction::Move, centre(4, 6).0, centre(4, 6).1, false)).unwrap();
    assert_eq!(away.cursor, "cell");
    doc.pointer(&event(PointerAction::Down, x0, y, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, x0 + 30.0, y, false)).unwrap();
    let before = doc.revision();
    let held = doc.pointer(&event(PointerAction::Drag, x0 + 30.2, y, false)).unwrap();
    assert_eq!(doc.revision(), before, "the caret stayed put");
    assert_eq!(held.cursor, "text", "selecting keeps the text cursor");
    doc.pointer(&event(PointerAction::Drag, x0 + 200.0, y, false)).unwrap();
    assert!(doc.revision() > before, "the caret moved");
    doc.pointer(&event(PointerAction::Up, x0 + 200.0, y, false)).unwrap();
    assert_eq!(doc.editing.as_ref().map(Edit::selection), Some((0, 10)));
}

/// A sheet with every affordance and object the editor draws over its cells: filter buttons on a
/// header, checkbox cells (empty, ticked, one on a fill), a note and a threaded comment, a data
/// bar, a sparkline, a column chart, outline buttons, and form controls kept from a file.
fn affordances() -> SheetDocument {
    let mut doc = SheetDocument::create("Affordances.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "A1:D5", "values": [["Item", "Q1", "Q2", "Q3"], ["Rent", 12, 14, 9], ["Food", 4, 7, 6], ["Fuel", 8, 3, 5], ["Tax", 2, 6, 11]] }))
        .unwrap();
    doc.execute("data.filter", json!({ "range": "A1:D5" })).unwrap();
    doc.execute("checkbox.insert", json!({ "range": "F2:F4" })).unwrap();
    doc.execute("cell.set", json!({ "cell": "F3", "input": "TRUE" })).unwrap();
    doc.execute("format.fillColor", json!({ "range": "F4", "color": "#C6EFCE" })).unwrap();
    doc.execute("note.add", json!({ "cell": "B2", "text": "A note" })).unwrap();
    doc.execute("comment.add", json!({ "cell": "C2", "text": "A comment", "author": "Example" })).unwrap();
    doc.execute("conditionalFormat.apply", json!({ "range": "D2:D5", "rule": { "type": "dataBar", "color": "#5B9BD5" } })).unwrap();
    doc.execute("sparkline.insert", json!({ "range": "B2:D2", "location": "E2", "type": "line", "markers": true })).unwrap();
    doc.execute("data.group", json!({ "rows": "7:8" })).unwrap();
    doc.execute("chart.insert", json!({ "range": "A1:B5", "type": "column" })).unwrap();
    let sheet = std::sync::Arc::make_mut(&mut doc.session.doc_mut().unwrap().wb).sheet_mut(0).unwrap();
    // The chart right of everything else, so nothing is under it.
    let anchor = &mut sheet.charts[0].anchor;
    (anchor.cell, anchor.dx, anchor.dy) = (CellRef::new(0, 10), 0.0, 0.0);
    for (kind, row, checked, text) in [("Checkbox", 11, true, "Paid"), ("Button", 13, false, "Run"), ("Drop", 15, false, "")] {
        sheet.form_controls.push(kition_sheet_engine::model::FormControl {
            kind: kind.into(),
            from: (CellRef::new(row, 6), 2.0, 2.0),
            to: (CellRef::new(row + 1, 8), 2.0, 2.0),
            checked,
            text: text.into(),
            ..Default::default()
        });
    }
    doc.execute("select.set", json!({ "range": "K30" })).unwrap();
    doc
}

/// Writes `px` as a PNG to `KITION_SHEET_PNG/<name>.png` when that folder is given, for a person to look at.
fn shoot(name: &str, px: &[u8], w: u32, h: u32) {
    if let Some(dir) = std::env::var_os("KITION_SHEET_PNG") {
        image::RgbaImage::from_raw(w, h, px.to_vec()).unwrap().save(std::path::Path::new(&dir).join(format!("{name}.png"))).unwrap();
    }
}

/// The editor's affordances follow the host's scheme in dark (task note-and-dark-leftovers): no
/// filter button, checkbox, form control or chart area is a light patch on the dark grid; the
/// marks on them read; the colours the file sets (a fill, the data bar's and the chart's series
/// colours) are kept; light is what it was, and pictures stay light.
#[test]
fn the_affordances_and_objects_follow_the_dark_scheme() {
    let mut doc = affordances();
    let (w, h) = (1300u32, 420u32);
    let light = doc.render(0.0, 0.0, 1.0, w, h).unwrap();
    let picture = doc.execute("file.export", json!({ "format": "png" })).unwrap();
    doc.execute("editor.scheme", json!({ "scheme": "dark" })).unwrap();
    let dark = doc.render(0.0, 0.0, 1.0, w, h).unwrap();
    shoot("sheet-affordances-light", &light, w, h);
    shoot("sheet-affordances-dark", &dark, w, h);

    let region = |px: &[u8], (x0, y0, x1, y1): (u32, u32, u32, u32)| -> Vec<[u8; 4]> {
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (y * w + x) as usize * 4)).map(|i| [px[i], px[i + 1], px[i + 2], px[i + 3]]).collect()
    };
    let sheet = &doc.sheet().unwrap().1.clone();
    let cell = |col: u32, row: u32| {
        let (x0, y0, x1, y1) = SheetDocument::cell_rect(sheet, RangeRef::new(CellRef::new(row, col), CellRef::new(row, col)));
        (x0 as u32 + 1, y0 as u32 + 1, x1 as u32 - 1, y1 as u32 - 1)
    };
    let light_patch = |p: &[u8; 4]| p[0] >= 240 && p[1] >= 240 && p[2] >= 240;
    // Each affordance's cell, and the chart and form controls where they sit.
    let chart = sheet.charts[0].anchor;
    let chart_x = SheetDocument::col_x(sheet, chart.cell.col) + f64::from(chart.dx);
    let chart_y = SheetDocument::row_y(sheet, chart.cell.row) + f64::from(chart.dy);
    let chart_box = (
        chart_x as u32 + 2,
        chart_y as u32 + 2,
        ((chart_x + f64::from(chart.width)) as u32).min(w) - 2,
        ((chart_y + f64::from(chart.height)) as u32).min(h) - 2,
    );
    let places = [
        ("filter buttons", (cell(0, 0).0, cell(0, 0).1, cell(3, 0).2, cell(3, 0).3)),
        ("empty checkbox", cell(5, 1)),
        ("ticked checkbox", cell(5, 2)),
        ("checkbox on a fill", cell(5, 3)),
        ("form controls", (cell(6, 11).0, cell(6, 11).1, cell(8, 16).2, cell(8, 16).3)),
        ("chart", chart_box),
    ];
    for (name, at) in places {
        let patches = region(&dark, at).iter().filter(|p| light_patch(p)).count();
        assert_eq!(patches, 0, "{name}: {patches} light pixels on the dark grid");
    }
    // Over the whole editor, no light patch is left.
    let anywhere = dark.chunks(4).filter(|p| light_patch(&[p[0], p[1], p[2], p[3]])).count();
    assert_eq!(anywhere, 0, "{anywhere} light pixels in the dark editor");
    // The marks read: a filter arrow, the tick, the note's and the comment's corners, a caption.
    let lit = |px: &[u8], at, min: f64| region(px, at).iter().filter(|p| scheme::luminance(**p) >= min).count();
    assert!(lit(&dark, cell(0, 0), 0.5) > 4, "the filter arrow is light on its dark button");
    assert!(lit(&dark, cell(5, 2), 0.3) > 10, "the ticked box is light, its tick dark");
    assert!(lit(&dark, (cell(6, 11).0, cell(6, 11).1, cell(8, 12).2, cell(8, 12).3), 0.5) > 10, "the check box's caption is light");
    let corner =
        |px: &[u8], at: (u32, u32, u32, u32)| region(px, (at.2 - 5, at.1, at.2, at.1 + 4)).into_iter().map(scheme::luminance).fold(0.0f64, f64::max);
    assert!(corner(&dark, cell(1, 1)) > 0.1, "the note's corner shows");
    assert!(corner(&dark, cell(2, 1)) > 0.1, "the comment's corner shows");
    // The chart's area is dark, its text light, its columns the file's colour in both schemes.
    let area = region(&dark, chart_box);
    let dark_share = area.iter().filter(|p| scheme::luminance(**p) <= 0.05).count() as f64 / area.len() as f64;
    assert!(dark_share > 0.5, "the chart's area follows: {dark_share:.2} of it is dark");
    let series = |px: &[u8]| {
        let mut counts = std::collections::HashMap::<[u8; 3], usize>::new();
        for p in region(px, chart_box) {
            if p[0].abs_diff(p[1]) > 40 || p[1].abs_diff(p[2]) > 40 {
                *counts.entry([p[0], p[1], p[2]]).or_default() += 1;
            }
        }
        counts.into_iter().max_by_key(|(_, n)| *n)
    };
    let (in_light, in_dark) = (series(&light).unwrap(), series(&dark).unwrap());
    assert_eq!(in_light.0, in_dark.0, "the columns keep the file's colour");
    assert!(in_light.1.abs_diff(in_dark.1) * 20 <= in_light.1, "the columns are as large: {in_light:?} {in_dark:?}");
    // The file's fill on F4 and the data bar's colour are kept.
    assert!(region(&dark, cell(5, 3)).iter().any(|p| p[..3] == [0xC6, 0xEF, 0xCE]), "the fill is kept");
    assert!(region(&dark, cell(3, 1)).iter().any(|p| p[..3] == [0x5B, 0x9B, 0xD5]), "the data bar keeps its colour");

    // Pictures stay light, and light is what it was.
    assert_eq!(doc.execute("file.export", json!({ "format": "png" })).unwrap(), picture);
    doc.execute("editor.scheme", json!({ "scheme": "light" })).unwrap();
    assert_eq!(doc.render(0.0, 0.0, 1.0, w, h).unwrap(), light);
}

/// The engine members a host reaches while a person types into a cell (each `ai/inspect` reply
/// through `modelReply`, receipts, the object model, a draft's comparison) read the workbook as
/// it is: none writes or drops the formula being typed, which Enter then enters whole.
#[test]
fn the_engine_members_leave_a_cell_being_typed_alone() {
    let mut doc = SheetDocument::open("Budget.xlsx", &fixture()).unwrap();
    let since = doc.version().unwrap();
    let mut draft = SheetDocument::open_with_replica("Budget.xlsx", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
    draft.execute("cell.set", json!({ "cell": "D1", "input": "draft" })).unwrap();
    let changes = draft.export_changes(&since).unwrap();
    click(&mut doc, centre(2, 1), false);
    KitTools::text(&mut doc, "=SUM(1,", 1.0).unwrap();
    assert_eq!(get(&doc, "editing"), true);
    let inspected = doc.inspect().unwrap();
    doc.model_reply(&json!({ "details": inspected }).to_string()).unwrap();
    doc.changes_since(&since);
    doc.model_describe().unwrap();
    doc.model_batch(&json!({ "ops": [] })).unwrap();
    doc.compare_changes(&changes).unwrap();
    doc.version().unwrap();
    assert_eq!(get(&doc, "editing"), true, "still typing");
    KitTools::text(&mut doc, "2)", 1.0).unwrap();
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    let cell = doc.execute("cell.get", json!({ "cell": "C2" })).unwrap();
    assert_eq!((cell["input"].clone(), cell["value"].clone()), (json!("=SUM(1,2)"), json!(3.0)), "{cell}");
}

fn mods(shift: bool, command: bool) -> ToolMods {
    ToolMods { shift, command, ..ToolMods::default() }
}

fn press(doc: &mut SheetDocument, name: &str, mods: ToolMods) {
    doc.key(name, mods, 1.0).unwrap();
}

/// Home, End and their Cmd and Shift forms as WPS has them (W-B9–W-B11; walkthrough C6): Home to
/// the row's column A, Cmd+Home to A1 (below and right of frozen panes), Cmd+End to the used
/// range's last cell, Shift extending; End alone does nothing. Ctrl is Cmd's twin.
#[test]
fn home_and_end_move_as_wps_does() {
    let mut doc = SheetDocument::create("Keys.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "J61", "input": "last" })).unwrap();
    doc.execute("cell.set", json!({ "cell": "C3", "input": "x" })).unwrap();
    doc.execute("select.set", json!({ "range": "G7" })).unwrap();
    press(&mut doc, "Home", mods(false, false));
    assert_eq!(get(&doc, "selection"), "A7");
    press(&mut doc, "End", mods(false, false));
    assert_eq!(get(&doc, "selection"), "A7", "End alone does nothing");
    press(&mut doc, "End", mods(false, true));
    assert_eq!(get(&doc, "selection"), "J61");
    press(&mut doc, "Home", mods(false, true));
    assert_eq!(get(&doc, "selection"), "A1");
    doc.execute("select.set", json!({ "range": "G7" })).unwrap();
    press(&mut doc, "Home", mods(true, false));
    assert_eq!((get(&doc, "selection"), get(&doc, "active")), (json!("A7:G7"), json!("G7")));
    doc.execute("select.set", json!({ "range": "B3" })).unwrap();
    press(&mut doc, "End", mods(true, true));
    assert_eq!(get(&doc, "selection"), "B3:J61");
    press(&mut doc, "Home", ToolMods { shift: true, control: true, ..ToolMods::default() });
    assert_eq!(get(&doc, "selection"), "A1:B3");
    // Frozen panes: Cmd+Home goes to the scrolling pane's first cell.
    doc.execute("view.freezePanes", json!({ "cell": "B2" })).unwrap();
    press(&mut doc, "Home", ToolMods { control: true, ..ToolMods::default() });
    assert_eq!(get(&doc, "selection"), "B2");
    // While a cell is typed in place, Home and End move the caret, not the cell.
    doc.execute("select.set", json!({ "range": "D5" })).unwrap();
    press(&mut doc, "F2", mods(false, false));
    KitTools::text(&mut doc, "ab", 1.0).unwrap();
    press(&mut doc, "Home", mods(false, false));
    KitTools::text(&mut doc, "x", 1.0).unwrap();
    press(&mut doc, "Enter", mods(false, false));
    assert_eq!(value(&mut doc, "D5")["value"], "xab");
}

/// Enter and Tab in a selection of more than one cell go round its cells, the selection staying
/// (WPS W-B4; walkthrough C3): Enter down each column, Tab along each row, Shift back; typing goes
/// into the active cell and moves on the same way; areas one after another; a merge once; a value
/// the cell's validation refuses holds the active cell where it is.
#[test]
fn enter_and_tab_go_round_the_selection() {
    let mut doc = SheetDocument::create("Cycle.xlsx", 0, 0).unwrap();
    let walk = |doc: &mut SheetDocument, name: &str, shift: bool, n: usize| {
        (0..n)
            .map(|_| {
                press(doc, name, mods(shift, false));
                get(doc, "active").as_str().unwrap_or_default().to_owned()
            })
            .collect::<Vec<_>>()
    };
    doc.execute("select.set", json!({ "range": "B2:C4" })).unwrap();
    assert_eq!(walk(&mut doc, "Enter", false, 6), ["B3", "B4", "C2", "C3", "C4", "B2"]);
    assert_eq!(walk(&mut doc, "Tab", false, 3), ["C2", "B3", "C3"]);
    assert_eq!(walk(&mut doc, "Tab", true, 2), ["B3", "C2"]);
    assert_eq!(walk(&mut doc, "Enter", true, 1), ["B4"]);
    assert_eq!(get(&doc, "selection"), "B2:C4", "the selection stays");
    // Typing in the selection: each Enter enters the value and goes on.
    doc.execute("select.set", json!({ "range": "B2:C4", "active": "B2" })).unwrap();
    for text in ["1", "2", "3", "4"] {
        KitTools::text(&mut doc, text, 1.0).unwrap();
        press(&mut doc, "Enter", mods(false, false));
    }
    assert_eq!(["B2", "B3", "B4", "C2"].map(|c| value(&mut doc, c)["value"].clone()), [json!(1.0), json!(2.0), json!(3.0), json!(4.0)]);
    assert_eq!((get(&doc, "active"), get(&doc, "selection")), (json!("C3"), json!("B2:C4")));
    // Several areas, one after another; a merged area is one stop.
    doc.execute("cell.merge", json!({ "range": "F2:G2" })).unwrap();
    doc.execute("select.set", json!({ "range": "A10:A11,F2:G3", "active": "A10" })).unwrap();
    assert_eq!(walk(&mut doc, "Tab", false, 5), ["A11", "F2", "F3", "G3", "A10"]);
    // A refused value holds: the cell stays open and the active cell where it is.
    doc.execute("data.validation", json!({ "range": "E5", "type": "list", "formula1": "\"是,否\"" })).unwrap();
    doc.execute("select.set", json!({ "range": "E5:E6", "active": "E5" })).unwrap();
    KitTools::text(&mut doc, "也许", 1.0).unwrap();
    press(&mut doc, "Enter", mods(false, false));
    assert_eq!((get(&doc, "active"), get(&doc, "editing")), (json!("E5"), json!(true)));
    doc.execute("editor.validation", json!({ "answer": "cancel" })).unwrap();
}

/// Tab along a row and Enter goes to the next row at the column the Tabs began in, typed or not
/// (WPS W-B3; walkthrough C2); an arrow or a click ends the run; Shift+Enter goes up as before.
#[test]
fn enter_after_tabs_goes_back_to_the_first_column() {
    let mut doc = SheetDocument::create("Rows.xlsx", 0, 0).unwrap();
    doc.execute("select.set", json!({ "range": "B3" })).unwrap();
    for text in ["x", "y"] {
        KitTools::text(&mut doc, text, 1.0).unwrap();
        press(&mut doc, "Tab", mods(false, false));
    }
    KitTools::text(&mut doc, "z", 1.0).unwrap();
    press(&mut doc, "Enter", mods(false, false));
    assert_eq!(get(&doc, "active"), "B4");
    assert_eq!(["B3", "C3", "D3"].map(|c| value(&mut doc, c)["value"].clone()), [json!("x"), json!("y"), json!("z")]);
    // Without typing.
    press(&mut doc, "Tab", mods(false, false));
    press(&mut doc, "Tab", mods(false, false));
    press(&mut doc, "Enter", mods(false, false));
    assert_eq!(get(&doc, "active"), "B5");
    // Shift+Tab goes on with the run.
    press(&mut doc, "Tab", mods(false, false));
    press(&mut doc, "Tab", mods(false, false));
    press(&mut doc, "Tab", mods(true, false));
    press(&mut doc, "Enter", mods(false, false));
    assert_eq!(get(&doc, "active"), "B6");
    // An arrow ends the run.
    press(&mut doc, "Tab", mods(false, false));
    press(&mut doc, "ArrowRight", mods(false, false));
    press(&mut doc, "Enter", mods(false, false));
    assert_eq!(get(&doc, "active"), "D7");
    // A click ends it too.
    press(&mut doc, "Tab", mods(false, false));
    click(&mut doc, centre(5, 6), false);
    press(&mut doc, "Enter", mods(false, false));
    assert_eq!(get(&doc, "active"), "F8");
    // Shift+Enter after Tabs goes up, in the column it is in.
    press(&mut doc, "Tab", mods(false, false));
    press(&mut doc, "Enter", mods(true, false));
    assert_eq!(get(&doc, "active"), "G7");
    // The formula bar's Enter and Tab end the same way.
    doc.execute("select.set", json!({ "range": "B20" })).unwrap();
    doc.execute("editor.formula", json!({ "text": "a" })).unwrap();
    doc.execute("editor.formula", json!({ "enter": "right" })).unwrap();
    doc.execute("editor.formula", json!({ "text": "b" })).unwrap();
    doc.execute("editor.formula", json!({ "enter": "down" })).unwrap();
    assert_eq!(get(&doc, "active"), "B21");
}

/// Cmd+Enter (Ctrl+Enter elsewhere) enters what is typed into every selected cell (WPS W-C10;
/// walkthrough D14), a formula's relative references moving; the active cell and the selection
/// stay; one undo takes it all back.
#[test]
fn cmd_enter_fills_the_selection() {
    let mut doc = SheetDocument::create("Fill.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "A2", "values": [[1], [2], [3], [4]] })).unwrap();
    doc.execute("select.set", json!({ "range": "B2:B5", "active": "B2" })).unwrap();
    KitTools::text(&mut doc, "7", 1.0).unwrap();
    press(&mut doc, "Enter", mods(false, true));
    assert_eq!(["B2", "B3", "B4", "B5"].map(|c| value(&mut doc, c)["value"].clone()), [json!(7.0), json!(7.0), json!(7.0), json!(7.0)]);
    assert_eq!((get(&doc, "active"), get(&doc, "selection"), get(&doc, "editing")), (json!("B2"), json!("B2:B5"), json!(false)));
    doc.execute("select.set", json!({ "range": "C2:C5", "active": "C2" })).unwrap();
    KitTools::text(&mut doc, "=A2*2", 1.0).unwrap();
    press(&mut doc, "Enter", ToolMods { control: true, ..ToolMods::default() });
    assert_eq!(value(&mut doc, "C5")["value"], json!(8.0));
    assert_eq!(value(&mut doc, "C5")["input"], json!("=A5*2"));
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(value(&mut doc, "C2")["value"], json!(null), "one undo takes the whole fill back");
    assert_eq!(value(&mut doc, "C5")["value"], json!(null));
}

/// A date typed into a column of the default width widens it, in the same undo step as the value
/// (WPS W-E6; walkthrough D10); the grid then shows the date, not ####.
#[test]
fn a_typed_date_widens_its_column() {
    let mut doc = SheetDocument::create("Dates.xlsx", 0, 0).unwrap();
    let width = |doc: &SheetDocument| doc.sheet().map(|(_, sheet)| sheet.col_width(0)).unwrap_or_default();
    let before = width(&doc);
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    KitTools::text(&mut doc, "2026-11-27", 1.0).unwrap();
    press(&mut doc, "Enter", mods(false, false));
    assert!(width(&doc) > before, "{} > {before}", width(&doc));
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!((width(&doc), value(&mut doc, "A1")["value"].clone()), (before, json!(null)));
}

/// Cmd+click on a selected cell takes it out (WPS W-A6; walkthrough B6): a one-cell area goes, a
/// cell inside a larger area leaves the pieces around it; Cmd+click on a heading adds its column
/// or row (walkthrough B8); a one-cell area among several is tinted.
#[test]
fn cmd_click_takes_a_selected_cell_out_and_adds_headings() {
    let mut doc = SheetDocument::create("Pick.xlsx", 0, 0).unwrap();
    let cmd = |action, (x, y): (f64, f64)| ToolPointer { action, x, y, mods: mods(false, true), pressure: 1.0, zoom: 1.0 };
    let cmd_click = |doc: &mut SheetDocument, at: (f64, f64)| {
        doc.pointer(&cmd(PointerAction::Down, at)).unwrap();
        doc.pointer(&cmd(PointerAction::Up, at)).unwrap()
    };
    doc.execute("select.set", json!({ "range": "B3:D6" })).unwrap();
    cmd_click(&mut doc, centre(2, 3));
    let selection = get(&doc, "selection").as_str().unwrap_or_default().to_owned();
    let doc_state = doc.session.active().unwrap().selection.clone();
    assert!(!doc_state.contains(CellRef::parse("C4").unwrap()), "{selection}");
    for cell in ["B3", "D3", "B4", "D4", "B6", "D6"] {
        assert!(doc_state.contains(CellRef::parse(cell).unwrap()), "{cell} in {selection}");
    }
    assert_eq!(doc_state.ranges.len(), 4, "{selection}");
    // A one-cell area among several: Cmd+click adds it, tinted; again takes it out.
    cmd_click(&mut doc, centre(7, 2));
    assert!(get(&doc, "selection").as_str().unwrap_or_default().ends_with("H3"));
    cmd_click(&mut doc, centre(9, 4));
    assert_eq!(get(&doc, "active"), "J5");
    let (x0, y0, x1, y1) = (HEADER_W + 7.0 * CW, HEADER_H + 2.0 * RH, HEADER_W + 8.0 * CW, HEADER_H + 3.0 * RH);
    let tinted = doc.respond().overlays.iter().any(|overlay| {
        matches!(overlay, ToolOverlay::Highlight { quad, .. } if (quad[0][0] - x0).abs() < 0.5 && (quad[0][1] - y0).abs() < 0.5 && (quad[2][0] - x1).abs() < 0.5 && (quad[2][1] - y1).abs() < 0.5)
    });
    assert!(tinted, "H3 is tinted among the areas");
    cmd_click(&mut doc, centre(7, 2));
    assert!(!doc.session.active().unwrap().selection.contains(CellRef::parse("H3").unwrap()));
    // The only selected cell stays selected.
    doc.execute("select.set", json!({ "range": "E5" })).unwrap();
    cmd_click(&mut doc, centre(4, 4));
    assert_eq!(get(&doc, "selection"), "E5");
    // Headings: Cmd+click adds the column, a drag from it more.
    doc.execute("select.set", json!({ "range": "C:G" })).unwrap();
    let heading = |col: u32| (HEADER_W + f64::from(col) * CW + 10.0, 10.0);
    cmd_click(&mut doc, heading(9));
    assert_eq!(get(&doc, "selection"), "C:G,J:J");
    doc.pointer(&cmd(PointerAction::Down, heading(11))).unwrap();
    doc.pointer(&cmd(PointerAction::Drag, heading(12))).unwrap();
    doc.pointer(&cmd(PointerAction::Up, heading(12))).unwrap();
    assert_eq!(get(&doc, "selection"), "C:G,J:J,L:M");
}

/// The fill handle's corner of a range at the default sizes (document pixels).
fn corner_of(last_col: u32, last_row: u32) -> (f64, f64) {
    (HEADER_W + f64::from(last_col + 1) * CW, HEADER_H + f64::from(last_row + 1) * RH)
}

fn undo_steps(doc: &SheetDocument) -> usize {
    doc.session.doc().map(|d| d.undo.len()).unwrap_or(0)
}

/// F1 (WPS W-A1, R-H1): a selection's bottom-right corner has the fill handle, a 6-pixel square
/// on a white edge; over it (9 × 9 screen pixels) the pointer is the thin cross; it is not there
/// for several areas, whole columns, or while typing.
#[test]
fn fill_handle_is_drawn_and_shows_the_cross() {
    let mut doc = SheetDocument::create("Fill.xlsx", 0, 0).unwrap();
    let shown = click(&mut doc, centre(1, 1), false);
    let (x, y) = corner_of(1, 1);
    let handle = |response: &ToolResponse| {
        response.overlays.iter().any(|o| matches!(o, ToolOverlay::Anchor { x: ax, y: ay, filled: true, size, color } if (ax - x).abs() < 0.5 && (ay - y).abs() < 0.5 && *size == 6.0 && *color == ACCENT))
    };
    assert!(handle(&shown), "the handle at B2's corner: {:?}", shown.overlays);
    assert_eq!(doc.pointer(&event(PointerAction::Move, x + 4.0, y + 4.0, false)).unwrap().cursor, "crosshair", "4 px off: the handle");
    assert_ne!(doc.pointer(&event(PointerAction::Move, x + 7.0, y + 7.0, false)).unwrap().cursor, "crosshair", "7 px off: the cells");
    assert_eq!(doc.pointer(&event(PointerAction::Move, centre(1, 1).0, centre(1, 1).1, false)).unwrap().cursor, "cell");
    // Zoomed in, the reach is still 4.5 screen pixels.
    let zoomed = ToolPointer { action: PointerAction::Move, x: x + 3.0, y: y + 3.0, mods: ToolMods::default(), pressure: 1.0, zoom: 2.0 };
    assert_ne!(doc.pointer(&zoomed).unwrap().cursor, "crosshair", "6 screen px off at 200%");
    doc.execute("select.set", json!({ "range": "B2,D4" })).unwrap();
    assert!(!doc.respond().overlays.iter().any(|o| matches!(o, ToolOverlay::Anchor { size, .. } if *size == 6.0)), "none for several areas");
    doc.execute("select.set", json!({ "range": "C:C" })).unwrap();
    assert!(!doc.respond().overlays.iter().any(|o| matches!(o, ToolOverlay::Anchor { size, .. } if *size == 6.0)), "none for a whole column");
    click(&mut doc, centre(1, 1), false);
    KitTools::text(&mut doc, "7", 1.0).unwrap();
    assert!(!handle(&doc.respond()), "none while typing");
}

/// F5 (WPS W-E1, W-E2): a drag from the corner fills, it no longer moves the cells: a single number
/// counts on (5…10) along the direction the pointer went further, in one undo step, the selection
/// the filled block; with Cmd held at the release it is copied; a dashed frame shows the cells it
/// would fill meanwhile, and the workbook does not change until the release.
#[test]
fn fill_handle_corner_fills_instead_of_moving() {
    let mut doc = SheetDocument::create("Fill.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "5" })).unwrap();
    click(&mut doc, centre(0, 0), false);
    let before = undo_steps(&doc);
    let from = corner_of(0, 0);
    doc.pointer(&event(PointerAction::Down, from.0 - 1.0, from.1 - 1.0, false)).unwrap();
    // Five rows down and a little right: down wins.
    let to = (centre(0, 5).0 + CW * 0.4, centre(0, 5).1);
    let during = doc.pointer(&event(PointerAction::Drag, to.0, to.1, false)).unwrap();
    assert_eq!(during.cursor, "crosshair");
    assert!(
        during.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { dashed: true, color, .. } if *color == [64, 64, 64, 255])),
        "a dashed frame"
    );
    assert_eq!(value(&mut doc, "A2")["value"], json!(null), "nothing filled yet");
    doc.pointer(&event(PointerAction::Up, to.0, to.1, false)).unwrap();
    let column: Vec<Json> = (1..=6).map(|r| value(&mut doc, &format!("A{r}"))["value"].clone()).collect();
    assert_eq!(column, [5.0, 6.0, 7.0, 8.0, 9.0, 10.0].map(|n| json!(n)));
    assert_eq!(value(&mut doc, "B2")["value"], json!(null), "only down");
    assert_eq!(get(&doc, "selection"), "A1:A6");
    assert_eq!(undo_steps(&doc), before + 1, "one undo step");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(value(&mut doc, "A6")["value"], json!(null));
    assert_eq!(value(&mut doc, "A1")["value"], json!(5.0), "the cells did not move");
    // Cmd at the release: a copy.
    click(&mut doc, centre(0, 0), false);
    let cmd = ToolMods { command: true, ..ToolMods::default() };
    doc.pointer(&event(PointerAction::Down, from.0, from.1, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, centre(0, 3).0, centre(0, 3).1, false)).unwrap();
    doc.pointer(&event_with(PointerAction::Up, centre(0, 3), cmd)).unwrap();
    let column: Vec<Json> = (1..=4).map(|r| value(&mut doc, &format!("A{r}"))["value"].clone()).collect();
    assert_eq!(column, [json!(5.0), json!(5.0), json!(5.0), json!(5.0)]);
    // Two cells go on by their step; further right than down fills right.
    doc.execute("range.setValues", json!({ "range": "C1", "values": [[1, 2]] })).unwrap();
    doc.execute("select.set", json!({ "range": "C1:D1" })).unwrap();
    let from = corner_of(3, 0);
    doc.pointer(&event(PointerAction::Down, from.0, from.1, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, centre(6, 1).0, centre(6, 1).1, false)).unwrap();
    doc.pointer(&event(PointerAction::Up, centre(6, 1).0, centre(6, 1).1, false)).unwrap();
    let row: Vec<Json> = ["E1", "F1", "G1"].iter().map(|c| value(&mut doc, c)["value"].clone()).collect();
    assert_eq!(row, [json!(3.0), json!(4.0), json!(5.0)]);
    assert_eq!(value(&mut doc, "C2")["value"], json!(null));
    // A press and release on the handle does nothing.
    let steps = undo_steps(&doc);
    let corner = corner_of(6, 0);
    click(&mut doc, corner, false);
    assert_eq!(undo_steps(&doc), steps);
    assert_eq!(get(&doc, "selection"), "C1:G1");
}

/// A formula filled down moves its relative references; Escape drops the drag (no change, no undo
/// step); dragged back into the selection, the release clears the cells passed over.
#[test]
fn fill_handle_formulas_escape_and_clearing() {
    let mut doc = SheetDocument::create("Fill.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "A1", "values": [[1], [2], [3]] })).unwrap();
    doc.execute("cell.set", json!({ "cell": "B1", "input": "=A1*2" })).unwrap();
    doc.execute("select.set", json!({ "range": "B1" })).unwrap();
    let from = corner_of(1, 0);
    drag_with(&mut doc, from, centre(1, 2), ToolMods::default());
    assert_eq!(formula_of(&mut doc, "B3"), "=A3*2");
    assert_eq!(value(&mut doc, "B3")["value"], json!(6.0));
    // Escape mid-drag.
    let steps = undo_steps(&doc);
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    let from = corner_of(0, 0);
    doc.pointer(&event(PointerAction::Down, from.0, from.1, false)).unwrap();
    doc.pointer(&event(PointerAction::Drag, centre(0, 6).0, centre(0, 6).1, false)).unwrap();
    assert_eq!(get(&doc, "escape"), json!(true), "Escape has the drag to cancel");
    let dropped = doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert!(!dropped.overlays.iter().any(|o| matches!(o, ToolOverlay::Path { dashed: true, .. })), "the frame goes");
    doc.pointer(&event(PointerAction::Up, centre(0, 6).0, centre(0, 6).1, false)).unwrap();
    assert_eq!((value(&mut doc, "A4")["value"].clone(), undo_steps(&doc)), (json!(null), steps), "nothing filled");
    // Back into A1:B3 from its corner to A1: rows 2 and 3 are cleared, A1:B1 stays selected.
    doc.execute("select.set", json!({ "range": "A1:B3" })).unwrap();
    let from = corner_of(1, 2);
    doc.pointer(&event(PointerAction::Down, from.0, from.1, false)).unwrap();
    let back = doc.pointer(&event(PointerAction::Drag, centre(1, 0).0, centre(1, 0).1, false)).unwrap();
    assert!(
        back.overlays.iter().any(|o| matches!(o, ToolOverlay::Highlight { color, .. } if *color == [96, 96, 96, 72])),
        "the cells to clear are shaded"
    );
    doc.pointer(&event(PointerAction::Up, centre(1, 0).0, centre(1, 0).1, false)).unwrap();
    assert_eq!((value(&mut doc, "A2")["value"].clone(), value(&mut doc, "B3")["value"].clone()), (json!(null), json!(null)));
    assert_eq!(value(&mut doc, "A1")["value"], json!(1.0));
    assert_eq!(get(&doc, "selection"), "A1:B1");
}

/// F2 (WPS W-E7): a double-click on the handle fills down as far as the data in the column on the
/// left goes (else the one on the right), counting a single number on; nothing when the cell
/// beside the row under it is empty.
#[test]
fn fill_handle_double_click_fills_beside_the_data() {
    let mut doc = SheetDocument::create("Fill.xlsx", 0, 0).unwrap();
    let rows: Vec<Json> = (0..60).map(|i| json!([format!("r{i}")])).collect();
    doc.execute("range.setValues", json!({ "range": "C2", "values": rows })).unwrap();
    doc.execute("cell.set", json!({ "cell": "D2", "input": "1" })).unwrap();
    doc.execute("select.set", json!({ "range": "D2" })).unwrap();
    let corner = corner_of(3, 1);
    // The editor sends the presses, then the double-click.
    click(&mut doc, corner, false);
    click(&mut doc, corner, false);
    doc.pointer(&event(PointerAction::DoubleClick, corner.0, corner.1, false)).unwrap();
    assert_eq!(value(&mut doc, "D61")["value"], json!(60.0));
    assert_eq!(value(&mut doc, "D62")["value"], json!(null));
    assert_eq!(get(&doc, "editing"), json!(false), "not typing");
    assert_eq!(get(&doc, "selection"), "D2:D61");
    // The right-hand column when the left one is empty.
    doc.execute("range.setValues", json!({ "range": "G2", "values": [["a"], ["b"], ["c"]] })).unwrap();
    doc.execute("cell.set", json!({ "cell": "F1", "input": "Mon" })).unwrap();
    doc.execute("select.set", json!({ "range": "F1" })).unwrap();
    let corner = corner_of(5, 0);
    doc.pointer(&event(PointerAction::DoubleClick, corner.0, corner.1, false)).unwrap();
    assert_eq!(value(&mut doc, "F4")["value"], json!("Thu"));
    assert_eq!(value(&mut doc, "F5")["value"], json!(null));
    // Nothing beside it: nothing happens.
    doc.execute("cell.set", json!({ "cell": "J1", "input": "x" })).unwrap();
    doc.execute("select.set", json!({ "range": "J1" })).unwrap();
    let steps = undo_steps(&doc);
    let corner = corner_of(9, 0);
    doc.pointer(&event(PointerAction::DoubleClick, corner.0, corner.1, false)).unwrap();
    assert_eq!(undo_steps(&doc), steps);
}

/// With frozen rows, the handle of a selection ending on the last frozen row answers only in the
/// frozen pane: a press just under the freeze line is the cells'.
#[test]
fn fill_handle_answers_only_in_its_pane() {
    let mut doc = SheetDocument::create("Fill.xlsx", 0, 0).unwrap();
    doc.execute("view.freezePanes", json!({ "cell": "A3" })).unwrap();
    doc.execute("select.set", json!({ "range": "B2" })).unwrap();
    let (x, y) = corner_of(1, 1);
    assert_eq!(doc.pointer(&event(PointerAction::Move, x, y - 2.0, false)).unwrap().cursor, "crosshair", "above the line");
    assert_ne!(doc.pointer(&event(PointerAction::Move, x, y + 2.0, false)).unwrap().cursor, "crosshair", "under the line");
}

/// The frames of `overlays`: each closed 2 px rectangle path, dashed or not, as (x0, y0, x1, y1).
fn frames(overlays: &[ToolOverlay], dashed_only: bool) -> Vec<(f64, f64, f64, f64)> {
    overlays
        .iter()
        .filter_map(|o| match o {
            ToolOverlay::Path { d, color, width, dashed } if *color == ACCENT && *width == 2.0 && *dashed == dashed_only && d.ends_with('Z') => {
                let n: Vec<f64> = d.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).filter_map(|t| t.parse().ok()).collect();
                Some((n[0], n[1], n[4], n[5]))
            }
            _ => None,
        })
        .collect()
}

/// The rectangle of cells `(c0, r0)`–`(c1, r1)` at the default sizes.
fn cells_rect((c0, r0): (u32, u32), (c1, r1): (u32, u32)) -> (f64, f64, f64, f64) {
    (HEADER_W + f64::from(c0) * CW, HEADER_H + f64::from(r0) * RH, HEADER_W + f64::from(c1 + 1) * CW, HEADER_H + f64::from(r1 + 1) * RH)
}

fn near(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5 && (a.2 - b.2).abs() < 0.5 && (a.3 - b.3).abs() < 0.5
}

/// B3 (WPS W-A1, W-A3): every selected area has a 2 px frame; inside an area of several cells the
/// active cell stays white (no tint over it) and has no frame of its own; one selected cell keeps
/// its frame; several areas each have theirs.
#[test]
fn selection_outline_frames_every_area_and_leaves_the_active_cell_white() {
    let mut doc = SheetDocument::create("Outline.xlsx", 0, 0).unwrap();
    doc.execute("select.set", json!({ "range": "B2:D6", "active": "B2" })).unwrap();
    let shown = doc.respond().overlays;
    let solid = frames(&shown, false);
    assert_eq!(solid.len(), 1, "one frame: {solid:?}");
    assert!(near(solid[0], cells_rect((1, 1), (3, 5))), "round B2:D6: {solid:?}");
    let active = cells_rect((1, 1), (1, 1));
    let tints: Vec<_> = shown
        .iter()
        .filter_map(|o| match o {
            ToolOverlay::Highlight { quad, color } if *color == SELECTION => Some((quad[0][0], quad[0][1], quad[2][0], quad[2][1])),
            _ => None,
        })
        .collect();
    assert!(!tints.is_empty());
    let (cx, cy) = centre(1, 1);
    assert!(!tints.iter().any(|t| cx > t.0 && cx < t.2 && cy > t.1 && cy < t.3), "B2 is not tinted: {tints:?} {active:?}");
    let (dx, dy) = centre(3, 5);
    assert!(tints.iter().any(|t| dx > t.0 && dx < t.2 && dy > t.1 && dy < t.3), "D6 is");
    // One cell: its own frame.
    doc.execute("select.set", json!({ "range": "C3" })).unwrap();
    let solid = frames(&doc.respond().overlays, false);
    assert_eq!(solid.len(), 1);
    assert!(near(solid[0], cells_rect((2, 2), (2, 2))));
    // Several areas: a frame each.
    doc.execute("select.set", json!({ "range": "B2:C3,E5:F7" })).unwrap();
    let solid = frames(&doc.respond().overlays, false);
    assert!(solid.iter().any(|f| near(*f, cells_rect((1, 1), (2, 2)))), "{solid:?}");
    assert!(solid.iter().any(|f| near(*f, cells_rect((4, 4), (5, 6)))), "{solid:?}");
    // Typing: the frame goes round the cell being typed.
    doc.execute("select.set", json!({ "range": "B2:D6", "active": "B2" })).unwrap();
    KitTools::text(&mut doc, "x", 1.0).unwrap();
    assert_eq!(frames(&doc.respond().overlays, false).len(), 2, "the area's frame and the typed cell's");
}

/// J3 (WPS W-F1, W-F3; plan section 2, "Escape keeps the system clipboard"): the person's copy has
/// marching ants (a dashed frame) in place of its frame; pasting leaves them, so the cells paste
/// again; Escape ends the copy (Escape has something to cancel meanwhile) and a paste of the same
/// text then pastes nothing; an agent's copy shows none.
#[test]
fn clipboard_marks_ants_round_the_copy_until_escape() {
    let mut doc = SheetDocument::create("Ants.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "B2", "values": [["a", "b"], ["c", "d"]] })).unwrap();
    doc.execute("select.set", json!({ "range": "B2:C3" })).unwrap();
    assert_eq!(get(&doc, "escape"), json!(false));
    doc.execute("edit.copy", json!({})).unwrap();
    let ants = frames(&doc.respond().overlays, true);
    assert_eq!(ants.len(), 1, "ants: {ants:?}");
    assert!(near(ants[0], cells_rect((1, 1), (2, 2))));
    assert!(frames(&doc.respond().overlays, false).is_empty(), "the ants take the frame's place");
    assert_eq!(get(&doc, "copied")["range"], json!("B2:C3"));
    assert_eq!(get(&doc, "escape"), json!(true), "Escape ends the copy");
    let text = doc.session.clipboard.as_ref().unwrap().text.clone();
    // Pasted twice: two copies, the ants still there.
    doc.execute("select.set", json!({ "range": "F2" })).unwrap();
    doc.execute("edit.paste", json!({ "text": text })).unwrap();
    doc.execute("select.set", json!({ "range": "F6" })).unwrap();
    doc.execute("edit.paste", json!({ "text": text })).unwrap();
    assert_eq!(value(&mut doc, "G3")["value"], json!("d"));
    assert_eq!(value(&mut doc, "G7")["value"], json!("d"));
    assert_eq!(frames(&doc.respond().overlays, true).len(), 1, "still marked");
    // Escape: the ants go; the same text pasted again pastes nothing.
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    assert!(frames(&doc.respond().overlays, true).is_empty());
    assert_eq!(get(&doc, "copied"), Json::Null);
    assert_eq!(get(&doc, "escape"), json!(false));
    doc.execute("select.set", json!({ "range": "J2" })).unwrap();
    doc.execute("edit.paste", json!({ "text": text })).unwrap();
    assert_eq!(value(&mut doc, "J2")["value"], json!(null), "the ended copy pastes nothing");
    // Other text from the system clipboard pastes.
    doc.execute("edit.paste", json!({ "text": "z" })).unwrap();
    assert_eq!(value(&mut doc, "J2")["value"], json!("z"));
}

/// J4 (WPS W-F6, W-F8): Enter on the target pastes once and ends the copy (the next Enter moves);
/// a cut pastes once; typing ends the copy, and so does a change to the copied cells.
#[test]
fn clipboard_marks_enter_pastes_once_and_typing_ends_the_copy() {
    let mut doc = SheetDocument::create("Ants.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "7" })).unwrap();
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    doc.execute("edit.copy", json!({})).unwrap();
    click(&mut doc, centre(2, 0), false);
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(value(&mut doc, "C1")["value"], json!(7.0), "Enter pasted");
    assert_eq!(get(&doc, "copied"), Json::Null, "and ended the copy");
    assert_eq!(get(&doc, "active"), json!("C1"));
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(get(&doc, "active"), json!("C2"), "the next Enter moves");
    assert_eq!(value(&mut doc, "C2")["value"], json!(null));
    // A cut pastes once.
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    doc.execute("edit.cut", json!({})).unwrap();
    let text = doc.session.clipboard.as_ref().unwrap().text.clone();
    doc.execute("select.set", json!({ "range": "E1" })).unwrap();
    doc.execute("edit.paste", json!({ "text": text })).unwrap();
    assert_eq!((value(&mut doc, "A1")["value"].clone(), value(&mut doc, "E1")["value"].clone()), (json!(null), json!(7.0)));
    doc.execute("select.set", json!({ "range": "E5" })).unwrap();
    doc.execute("edit.paste", json!({ "text": text })).unwrap();
    assert_eq!(value(&mut doc, "E5")["value"], json!(null), "a cut pastes once");
    // Typing in a cell ends the copy.
    doc.execute("select.set", json!({ "range": "E1" })).unwrap();
    doc.execute("edit.copy", json!({})).unwrap();
    assert!(get(&doc, "copied").is_object());
    click(&mut doc, centre(6, 6), false);
    KitTools::text(&mut doc, "x", 1.0).unwrap();
    assert_eq!(get(&doc, "copied"), Json::Null, "typing ended it");
    doc.key("Escape", ToolMods::default(), 1.0).unwrap();
    // A change to the copied cells ends it.
    doc.execute("select.set", json!({ "range": "E1" })).unwrap();
    doc.execute("edit.copy", json!({})).unwrap();
    doc.execute("cell.set", json!({ "cell": "E1", "input": "8" })).unwrap();
    assert_eq!(get(&doc, "copied"), Json::Null, "the copied cell changed");
    click(&mut doc, centre(7, 7), false);
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(value(&mut doc, "H8")["value"], json!(null), "Enter only moves");
    assert_eq!(get(&doc, "active"), json!("H9"));
}

/// P2 (WPS W-P2, W-P3): Cmd dragging the border copies; Option (Alt) dragging moves.
#[test]
fn border_drag_copies_with_cmd_and_moves_with_alt() {
    let mut doc = SheetDocument::create("Move.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "B2", "input": "m" })).unwrap();
    click(&mut doc, centre(1, 1), false);
    let border = (HEADER_W + CW + 1.0, centre(1, 1).1);
    let alt = ToolMods { alt: true, ..ToolMods::default() };
    assert_eq!(doc.pointer(&event_with(PointerAction::Move, border, alt)).unwrap().cursor, "move", "Option: the move cursor");
    drag_with(&mut doc, border, (border.0, border.1 + 3.0 * RH), alt);
    assert_eq!((value(&mut doc, "B2")["value"].clone(), value(&mut doc, "B5")["value"].clone()), (json!(null), json!("m")), "Option moved");
    let border = (HEADER_W + CW + 1.0, centre(1, 4).1);
    let cmd = ToolMods { command: true, ..ToolMods::default() };
    assert_eq!(doc.pointer(&event_with(PointerAction::Move, border, cmd)).unwrap().cursor, "copy");
    drag_with(&mut doc, border, (border.0 + 2.0 * CW, border.1), cmd);
    assert_eq!((value(&mut doc, "B5")["value"].clone(), value(&mut doc, "D5")["value"].clone()), (json!("m"), json!("m")), "Cmd copied");
}

/// W-P6: a double-click on the active cell's bottom border goes to the last row of its data
/// block (as Cmd+↓), on its right border to the block's last column; the press's click before it
/// does not move the jump's start.
#[test]
fn border_double_click_goes_to_the_end_of_the_data() {
    let mut doc = SheetDocument::create("Jump.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "B2", "values": [[1, 1, 1], [2, 0, 0], [3, 0, 0], [4, 0, 0]] })).unwrap();
    click(&mut doc, centre(1, 1), false);
    // Just under B2's bottom edge: the press's click alone would select B3.
    let bottom = (centre(1, 1).0, HEADER_H + 2.0 * RH + 1.5);
    click(&mut doc, bottom, false);
    click(&mut doc, bottom, false);
    doc.pointer(&event(PointerAction::DoubleClick, bottom.0, bottom.1, false)).unwrap();
    assert_eq!(get(&doc, "active"), json!("B5"), "to the block's last row");
    assert_eq!(get(&doc, "editing"), json!(false), "not typing");
    let right = (HEADER_W + 2.0 * CW - 1.5, centre(1, 4).1);
    click(&mut doc, right, false);
    click(&mut doc, right, false);
    doc.pointer(&event(PointerAction::DoubleClick, right.0, right.1, false)).unwrap();
    assert_eq!(get(&doc, "active"), json!("D5"), "to the block's last column");
}

/// H1 (WPS W-I1, W-I2): dragging a column's edge shows its new width beside the guide line, in
/// characters and pixels; a row's in points and pixels.
#[test]
fn resize_shows_the_new_size_beside_the_line() {
    let mut doc = SheetDocument::create("Resize.xlsx", 0, 0).unwrap();
    let edge = (HEADER_W + CW, HEADER_H / 2.0);
    doc.pointer(&event(PointerAction::Down, edge.0, edge.1, false)).unwrap();
    let during = doc.pointer(&event(PointerAction::Drag, edge.0 + 30.0, edge.1, false)).unwrap();
    let width = CW + 30.0;
    let text = during.overlays.iter().find_map(|o| match o {
        ToolOverlay::Measure { text, .. } => Some(text.clone()),
        _ => None,
    });
    let chars = kition_sheet_engine::cmd::format::points_to_chars(width);
    assert_eq!(text, Some(format!("{chars:.2} ({width} px)")));
    let done = doc.pointer(&event(PointerAction::Up, edge.0 + 30.0, edge.1, false)).unwrap();
    assert!(!done.overlays.iter().any(|o| matches!(o, ToolOverlay::Measure { .. })), "gone at the release");
    let edge = (HEADER_W / 2.0, HEADER_H + RH);
    doc.pointer(&event(PointerAction::Down, edge.0, edge.1, false)).unwrap();
    let during = doc.pointer(&event(PointerAction::Drag, edge.0, edge.1 + 20.0, false)).unwrap();
    let height = RH + 20.0;
    assert!(
        during.overlays.iter().any(|o| matches!(o, ToolOverlay::Measure { text, .. } if *text == format!("{:.2} pt ({height} px)", height * 0.75)))
    );
}

/// C8 (the race the hand tests met): Shift+Space decided in the editor's queue, after the keys
/// typed before it: `editor.insertText {typing}` types the space only into a cell being typed and
/// otherwise does nothing, so the editor selects the row instead.
#[test]
fn insert_text_typing_only_types_into_what_is_typed() {
    let mut doc = SheetDocument::create("Space.xlsx", 0, 0).unwrap();
    click(&mut doc, centre(1, 1), false);
    assert_eq!(doc.execute("editor.insertText", json!({ "text": " ", "typing": true })).unwrap(), json!({ "editing": false }));
    assert_eq!(get(&doc, "editing"), json!(false), "nothing typed, nothing started");
    KitTools::text(&mut doc, "a", 1.0).unwrap();
    doc.execute("editor.insertText", json!({ "text": " ", "typing": true })).unwrap();
    KitTools::text(&mut doc, "b", 1.0).unwrap();
    doc.key("Enter", ToolMods::default(), 1.0).unwrap();
    assert_eq!(value(&mut doc, "B2")["value"], json!("a b"));
}

/// The person's copy emptied by an agent (`edit.clearClipboard`): the system clipboard's text still
/// pastes, as any other (web/tests/clipboard-keys.mjs pastes after an agent cleared the clipboard).
#[test]
fn clipboard_marks_an_agents_clear_leaves_the_system_text_pasting() {
    let mut doc = SheetDocument::create("Ants.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "q" })).unwrap();
    doc.execute("select.set", json!({ "range": "A1" })).unwrap();
    doc.execute("edit.copy", json!({})).unwrap();
    let text = doc.session.clipboard.as_ref().unwrap().text.clone();
    // An agent empties the clipboard: the person's copy is not ended by them.
    let agent = kition_sheet_engine::Origin::agent("agent:test");
    doc.execute_as("edit.clearClipboard", json!({}), &agent).unwrap();
    assert!(doc.session.clipboard.is_none());
    doc.execute("select.set", json!({ "range": "D4" })).unwrap();
    doc.execute("edit.paste", json!({ "text": text })).unwrap();
    assert_eq!(value(&mut doc, "D4")["value"], json!("q"), "the system clipboard's text pastes");
    // An agent's copy is not marked: the person's Enter moves.
    doc.execute_as("edit.copy", json!({ "range": "D4" }), &agent).unwrap();
    assert_eq!(get(&doc, "copied"), Json::Null);
}
