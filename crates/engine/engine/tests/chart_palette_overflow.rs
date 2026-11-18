//! C8-sheet-read: generated chart palettes accept many positional series without overflow.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kition_sheet_engine::Session;
use kition_sheet_engine::core::{CellRef, RangeRef};
use kition_sheet_engine::model::Color;
use serde_json::{Value, json};

#[test]
fn generated_palette_tints_saturate_before_series_index_arithmetic_overflows() {
    let mut failures = Vec::new();
    for palette in ["monochrome", "accent1"] {
        let mut session = Session::new();
        session.new_workbook();
        let mut headers = vec![json!("Category")];
        headers.extend((1..=200).map(|index| json!(format!("Series {index}"))));
        let mut values = vec![json!("One")];
        values.extend((1..=200).map(Value::from));
        session.execute("range.setValues", json!({"range":"A1","values":[headers,values]})).unwrap();
        // Two areas keep the 200 columns as series rather than switching to rows.
        let left = RangeRef::new(CellRef::new(0, 0), CellRef::new(1, 100)).a1();
        let right = RangeRef::new(CellRef::new(0, 101), CellRef::new(1, 200)).a1();
        session.execute("insert.chart", json!({"range":format!("{left},{right}"),"type":"column"})).unwrap();
        assert_eq!(session.doc().unwrap().wb.active().unwrap().charts[0].series.len(), 200);
        if let Err(error) = session.execute("chart.changeColors", json!({"chart":1,"palette":palette})) {
            failures.push(format!("{palette}: {error}"));
            continue;
        }
        let document = session.doc().unwrap();
        let series = &document.wb.active().unwrap().charts[0].series;
        let first_tint = if palette == "monochrome" { -400 } else { -500 };
        assert_eq!(series[0].color, Some(Color::Theme(4, first_tint)));
        assert!(series.iter().skip(7).all(|series| series.color == Some(Color::Theme(4, 800))), "{palette} must retain its upper tint clamp");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
