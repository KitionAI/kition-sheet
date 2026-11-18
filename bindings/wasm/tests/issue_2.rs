//! Charts through the kit: `chart.changeColors {colors}` is taken (its schema
//! had only `palette`, so the catalogued `colors` was refused), and the chart commands' schemas
//! name the series types, the secondary axis and the waterfall totals they take.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[test]
fn change_colors_takes_a_list_of_colours() {
    let mut doc = SheetDocument::create("Colors.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({ "range": "A1", "values": [["Brand", "Share"], ["A", 40], ["B", 30]] })).unwrap();
    doc.execute("insert.chart", json!({ "range": "A1:B3", "type": "column" })).unwrap();
    doc.execute("chart.changeColors", json!({ "colors": ["#ff0000", "#00ff00"] })).unwrap();
    let described = doc.execute("drawing.describe", json!({})).unwrap();
    assert_eq!(described["objects"][0]["chart"]["series"][0]["color"], json!({ "rgb": "#FF0000" }), "{described}");
}

#[test]
fn chart_schemas_name_what_the_commands_take() {
    let doc = SheetDocument::create("Schemas.xlsx", 0, 0).unwrap();
    let commands = doc.commands();
    let schema = |id: &str| commands.iter().find(|c| c.id == id).and_then(|c| c.schema.clone()).unwrap_or_else(|| panic!("{id}"));
    let insert = schema("insert.chart");
    for key in ["seriesTypes", "secondary", "totals"] {
        assert!(insert.get(key).is_some(), "insert.chart: {key} in {insert}");
    }
    assert!(schema("chart.set").get("totals").is_some());
    assert!(schema("chart.changeColors").get("colors").is_some());
    let series_format = schema("drawing.chart")["seriesFormat"].to_string();
    assert!(series_format.contains("secondary") && series_format.contains("kind"), "{series_format}");
}
