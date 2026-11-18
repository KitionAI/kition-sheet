//! `file.render`, the agent's preview (`agent.render` in kit.json): a picture of a named sheet's
//! range with its row and column headings, a function of the workbook and the call alone. It never
//! reads or moves the active sheet, the selection, the zoom, the viewport or the cell being typed,
//! and the same call answers the same bytes (task card C4/C5/C6-sheet).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

/// `Data` (a small table) and `Notes` (other words), `Data` first in the workbook.
fn book() -> SheetDocument {
    let mut book = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    book.execute("sheet.rename", json!({ "name": "Data" })).unwrap();
    book.execute("range.setValues", json!({ "range": "A1", "values": [["Region", "Sales"], ["North", 120], ["South", 80], ["East", 95]] })).unwrap();
    book.execute("home.bold", json!({ "range": "A1:B1", "on": true })).unwrap();
    book.execute("home.insertSheet", json!({ "name": "Notes", "before": 9 })).unwrap();
    book.execute("range.setValues", json!({ "range": "A1", "values": [["Ship", "Friday"], ["Owner", "Lee"]] })).unwrap();
    book
}

fn render(book: &mut SheetDocument, params: Value) -> Value {
    book.execute("file.render", params.clone()).unwrap_or_else(|error| panic!("file.render {params}: {error}"))
}

fn png(answer: &Value) -> Vec<u8> {
    kition_sheet_engine_base64(answer["png"].as_str().unwrap())
}

fn kition_sheet_engine_base64(text: &str) -> Vec<u8> {
    // The PNG is base64 (standard alphabet, padded).
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in text.bytes().filter(|b| *b != b'=') {
        let value = ALPHABET.iter().position(|c| *c == byte).unwrap() as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

#[test]
fn it_answers_a_png_of_the_named_range_with_what_it_shows() {
    let mut book = book();
    let answer = render(&mut book, json!({ "target": { "sheet": "Data", "range": "A1:B4" } }));
    let bytes = png(&answer);
    assert!(bytes.starts_with(b"\x89PNG"), "{answer}");
    assert_eq!(answer["mimeType"], "image/png");
    assert_eq!((answer["sheet"].clone(), answer["range"].clone()), (json!("Data"), json!("A1:B4")));
    assert_eq!(answer["sheetId"], json!(format!("sheet:{}", book.inspect().unwrap()["workbook"]["sheets"][0]["id"])));
    assert_eq!(answer["scale"], json!(1.0));
    assert!(answer["width"].as_u64().unwrap() > 100 && answer["height"].as_u64().unwrap() > 60, "{answer}");
    assert_eq!(answer["capped"], json!(false));
}

#[test]
fn it_never_reads_or_moves_the_editor_and_answers_the_same_bytes() {
    let mut book = book();
    // Notes is active (it was inserted last); the call names Data.
    let call = json!({ "target": { "sheet": "Data", "range": "A1:B4" }, "scale": 1.5 });
    let first = render(&mut book, call.clone());
    let (revision, dirty, state) = (book.revision(), book.dirty(), book.editor_state());
    assert_eq!(render(&mut book, call.clone()), first, "the same call answers the same bytes");
    assert_eq!((book.revision(), book.dirty(), book.editor_state()), (revision, dirty, state), "rendering changed the editor or the workbook");
    // Another active sheet, another selection, a zoom, a viewport and a cell being typed.
    book.execute("sheet.activate", json!({ "sheet": "Data" })).unwrap();
    book.execute("selection.set", json!({ "range": "A2:B3" })).unwrap();
    book.execute("view.zoom", json!({ "percent": 200 })).unwrap();
    book.execute("editor.viewport", json!({ "right": 4000, "bottom": 3000, "x": 300, "y": 200 })).unwrap();
    assert_eq!(render(&mut book, call), first, "the active sheet, the selection, the zoom or the viewport changed the picture");
}

#[test]
fn another_range_or_another_sheet_is_another_picture() {
    let mut book = book();
    let data = render(&mut book, json!({ "target": { "sheet": "Data", "range": "A1:B4" } }));
    let part = render(&mut book, json!({ "target": { "sheet": "Data", "range": "A2:B3" } }));
    let notes = render(&mut book, json!({ "target": { "sheet": "Notes", "range": "A1:B4" } }));
    assert_ne!(data["png"], part["png"]);
    assert_ne!(data["png"], notes["png"]);
    assert!(part["height"].as_u64() < data["height"].as_u64(), "two rows are shorter than four");
    // By tool id; without a sheet, the first sheet by workbook order, never the active one.
    let id = book.inspect().unwrap()["workbook"]["sheets"][1]["toolId"].clone();
    assert_eq!(render(&mut book, json!({ "target": { "sheet": id, "range": "A1:B4" } }))["png"], notes["png"]);
    let first = render(&mut book, json!({ "target": { "range": "A1:B4" } }));
    assert_eq!((first["sheet"].clone(), first["png"].clone()), (json!("Data"), data["png"].clone()), "Notes is active, Data is first");
    // Without a range, the sheet's used range.
    assert_eq!(render(&mut book, json!({ "target": { "sheet": "Data" } }))["range"], "A1:B4");
}

#[test]
fn scale_and_max_size_set_the_pictures_size() {
    let mut book = book();
    let one = render(&mut book, json!({ "target": { "sheet": "Data" } }));
    let two = render(&mut book, json!({ "target": { "sheet": "Data" }, "scale": 2 }));
    let width = |answer: &Value| answer["width"].as_u64().unwrap();
    assert!(width(&two).abs_diff(2 * width(&one)) <= 2, "{} vs {}", width(&two), width(&one));
    let small = render(&mut book, json!({ "target": { "sheet": "Data" }, "scale": 2, "maxSize": 64 }));
    assert!(width(&small).max(small["height"].as_u64().unwrap()) <= 64, "{small}");
    assert!(small["scale"].as_f64().unwrap() < 1.0, "{small}");
}

#[test]
fn a_used_range_beyond_the_cap_is_drawn_up_to_it_and_says_so() {
    let mut book = book();
    let rows: Vec<Value> = (0..400)
        .map(|r| json!([r, r * 2, r * 3, r * 4, r * 5, r * 6, r * 7, r * 8, r * 9, r * 10, r * 11, r * 12, r * 13, r * 14, r * 15]))
        .collect();
    book.execute("range.setValues", json!({ "range": "Notes!A1", "values": rows })).unwrap();
    let answer = render(&mut book, json!({ "target": { "sheet": "Notes" } }));
    assert_eq!(answer["capped"], json!(true), "{answer}");
    let max = answer["maxCells"].as_u64().unwrap();
    assert!(answer["cells"].as_u64().unwrap() <= max, "{answer}");
    assert_eq!(answer["usedRange"], "A1:O400");
    // A range asked for beyond the cap is refused, never drawn as another.
    let error = book.execute("file.render", json!({ "target": { "sheet": "Notes", "range": "A1:O400" } })).unwrap_err();
    assert!(error.contains("cells") && error.contains(&max.to_string()), "{error}");
}

#[test]
fn what_it_cannot_draw_is_refused_saying_what_it_takes() {
    let mut book = book();
    for (target, says) in [
        (json!({ "sheet": "Nowhere" }), "no sheet named `Nowhere`"),
        (json!({ "sheet": "sheet:99" }), "no sheet has the tool id `sheet:99`"),
        (json!({ "sheet": "Data", "range": "B2:" }), "not a range"),
        (json!({ "sheet": "Data", "range": "A1:XFE3" }), "not a range"),
        (json!({ "sheet": "Data", "range": "Notes!A1" }), "names sheet `Notes`"),
        (json!({ "slide": 1 }), "`target.slide`"),
    ] {
        let error = book.execute("file.render", json!({ "target": target })).unwrap_err();
        assert!(error.contains(says), "{target}: {error}");
    }
    for (params, says) in [(json!({ "scale": 0 }), "`scale`"), (json!({ "scale": 9 }), "`scale`"), (json!({ "maxSize": 8 }), "`maxSize`")] {
        let error = book.execute("file.render", params.clone()).unwrap_err();
        assert!(error.contains(says), "{params}: {error}");
    }
}
