//! Conditional formatting as Excel documents it. A cellIs rule honours
//! its operator (Excel's names, `lessThan`, …); a rule refuses keys it does not read; `style`
//! takes hex colours; the rule with the higher priority wins the fill whether it is a colour
//! scale or a formatted rule; icon sets cut at percent of the min–max span by default, take
//! `thresholds`, and keep them through .xlsx.

use kition_sheet_core::CellRef;
use kition_sheet_engine::Session;
use kition_sheet_engine::cf::{CfCache, CfLook};
use kition_sheet_engine::model::{Color, PatternType};
use serde_json::{Value as Json, json};

fn workbook(values: &[f64]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows: Vec<Json> = values.iter().map(|v| json!([v])).collect();
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s
}

fn look(s: &Session, cell: &str) -> Option<CfLook> {
    let wb = &s.doc().unwrap().wb;
    CfCache::new().look(wb, wb.active_sheet, CellRef::parse(cell).unwrap())
}

fn styled(s: &Session, cell: &str) -> bool {
    look(s, cell).is_some_and(|l| l.style.is_some())
}

fn icon(s: &Session, cell: &str) -> usize {
    look(s, cell).and_then(|l| l.icon).map(|(_, i)| i).unwrap_or(usize::MAX)
}

#[test]
fn cell_is_honours_the_operator() {
    let mut s = workbook(&[-5.0, 0.0, 5.0]);
    s.execute("conditionalFormat.apply", json!({"range": "A1:A3", "rule": {"type": "cellIs", "operator": "lessThan", "value": 0}})).unwrap();
    assert_eq!([styled(&s, "A1"), styled(&s, "A2"), styled(&s, "A3")], [true, false, false]);

    for (operator, expected) in [
        ("greaterThan", [false, false, true]),
        ("greaterThanOrEqual", [false, true, true]),
        ("lessThanOrEqual", [true, true, false]),
        ("equal", [false, true, false]),
        ("notEqual", [true, false, true]),
    ] {
        let mut s = workbook(&[-5.0, 0.0, 5.0]);
        s.execute("conditionalFormat.apply", json!({"range": "A1:A3", "rule": {"type": "cellIs", "operator": operator, "value": 0}})).unwrap();
        assert_eq!([styled(&s, "A1"), styled(&s, "A2"), styled(&s, "A3")], expected, "{operator}");
    }
    let mut s = workbook(&[-5.0, 0.0, 5.0]);
    s.execute("conditionalFormat.apply", json!({"range": "A1:A3", "rule": {"type": "cellIs", "operator": "between", "value": -1, "value2": 9}}))
        .unwrap();
    assert_eq!([styled(&s, "A1"), styled(&s, "A2"), styled(&s, "A3")], [false, true, true]);
}

#[test]
fn a_rule_refuses_what_it_does_not_read() {
    let mut s = workbook(&[1.0]);
    let refused = |s: &mut Session, rule: Json| s.execute("conditionalFormat.apply", json!({"range": "A1", "rule": rule})).unwrap_err().to_string();
    let message = refused(&mut s, json!({"type": "cellIs", "operator": "smallerThan", "value": 0}));
    assert!(message.contains("lessThan"), "{message}");
    let message = refused(&mut s, json!({"type": "cellIs", "operator": "between", "values": [1, 2]}));
    assert!(message.contains("values") && message.contains("value2"), "{message}");
    let message = refused(&mut s, json!({"type": "expression", "formula": "=A1>0", "format": {"bold": true}}));
    assert!(message.contains("format") && message.contains("style"), "{message}");
    refused(&mut s, json!({"type": "colorScale", "colors": ["#ff0000", "#00ff00"], "preset": "redText"}));
    refused(&mut s, json!({"type": "iconSet", "set": "3Hearts"}));
    assert!(s.doc().unwrap().wb.active().unwrap().cond_formats.is_empty());
}

#[test]
fn style_takes_hex_colours() {
    let mut s = workbook(&[1.0]);
    s.execute(
        "conditionalFormat.apply",
        json!({"range": "A1", "rule": {"type": "expression", "formula": "=A1>0", "style": {"fill": "#FFEB9C", "font": {"color": "#9C5700", "bold": true}, "borders": {"bottom": {"style": "Thin", "color": "#C00000"}}}}}),
    )
    .unwrap();
    let style = look(&s, "A1").unwrap().style.unwrap();
    assert_eq!((style.fill.pattern, style.fill.fg), (PatternType::Solid, Color::Rgb(0xFFEB9C)));
    assert_eq!(style.font.color, Color::Rgb(0x9C5700));
    assert!(style.font.bold);
    assert_eq!(style.borders.bottom.color, Color::Rgb(0xC00000));

    // A fill given as its colour alone is solid; the engine's own form still works.
    let mut s = workbook(&[1.0]);
    s.execute(
        "conditionalFormat.apply",
        json!({"range": "A1", "rule": {"type": "expression", "formula": "=A1>0", "style": {"fill": {"fg": "#00FF00"}}}}),
    )
    .unwrap();
    let style = look(&s, "A1").unwrap().style.unwrap();
    assert_eq!((style.fill.pattern, style.fill.fg), (PatternType::Solid, Color::Rgb(0x00FF00)));
    let mut s = workbook(&[1.0]);
    s.execute(
        "conditionalFormat.apply",
        json!({"range": "A1", "rule": {"type": "expression", "formula": "=A1>0", "style": {"fill": {"fg": {"Rgb": 255}, "pattern": "Solid"}}}}),
    )
    .unwrap();
    assert_eq!(look(&s, "A1").unwrap().style.unwrap().fill.fg, Color::Rgb(255));

    // Format Cells takes them too.
    s.execute("format.cells", json!({"range": "A1", "style": {"fill": "#123456", "font": {"color": "#654321"}}})).unwrap();
    let wb = &s.doc().unwrap().wb;
    let cell_style = wb.styles.get(wb.active().unwrap().style_id(CellRef::parse("A1").unwrap()));
    assert_eq!(
        (cell_style.fill.pattern, cell_style.fill.fg, cell_style.font.color),
        (PatternType::Solid, Color::Rgb(0x123456), Color::Rgb(0x654321))
    );
    assert!(s.execute("format.cells", json!({"range": "A1", "style": {"fill": "#12345"}})).is_err());
}

#[test]
fn the_higher_priority_rule_wins_the_fill() {
    // The newer rule has the higher priority: a formatted rule added after a colour scale wins.
    let mut s = workbook(&[1.0, 2.0, 3.0]);
    s.execute("conditionalFormat.apply", json!({"range": "A1:A3", "rule": {"type": "colorScale"}})).unwrap();
    s.execute("conditionalFormat.apply", json!({"range": "A1:A3", "rule": {"type": "expression", "formula": "=A1=2", "style": {"fill": "#0000FF"}}}))
        .unwrap();
    let both = look(&s, "A2").unwrap();
    assert_eq!(both.scale_fill, None);
    assert_eq!(both.style.unwrap().fill.fg, Color::Rgb(0x0000FF));
    assert!(look(&s, "A1").unwrap().scale_fill.is_some());

    // A colour scale added last wins over the older rule's fill.
    let mut s = workbook(&[1.0, 2.0, 3.0]);
    s.execute("conditionalFormat.apply", json!({"range": "A1:A3", "rule": {"type": "expression", "formula": "=A1=2", "style": {"fill": "#0000FF"}}}))
        .unwrap();
    s.execute("conditionalFormat.apply", json!({"range": "A1:A3", "rule": {"type": "colorScale"}})).unwrap();
    assert!(look(&s, "A2").unwrap().scale_fill.is_some());
}

#[test]
fn formatted_rules_combine_their_formats() {
    let mut s = workbook(&[5.0]);
    s.execute("conditionalFormat.apply", json!({"range": "A1", "rule": {"type": "expression", "formula": "=A1>0", "style": {"fill": "#00FF00"}}}))
        .unwrap();
    s.execute(
        "conditionalFormat.apply",
        json!({"range": "A1", "rule": {"type": "expression", "formula": "=A1>1", "style": {"font": {"bold": true}}}}),
    )
    .unwrap();
    let style = look(&s, "A1").unwrap().style.unwrap();
    assert!(style.font.bold);
    assert_eq!(style.fill.fg, Color::Rgb(0x00FF00));
}

#[test]
fn icon_sets_cut_at_percent_of_the_span_by_default() {
    // Percent of min–max: 33 % and 67 % of 1..100 are 33.67 and 67.33. (Percentiles would put
    // 3 in the middle and 4 at the top.)
    let mut s = workbook(&[1.0, 2.0, 3.0, 4.0, 100.0]);
    s.execute("conditionalFormat.apply", json!({"range": "A1:A5", "rule": {"type": "iconSet", "set": "3Arrows"}})).unwrap();
    assert_eq!([icon(&s, "A1"), icon(&s, "A3"), icon(&s, "A4"), icon(&s, "A5")], [0, 0, 0, 2]);
}

#[test]
fn icon_sets_take_thresholds_and_keep_them_in_xlsx() {
    let mut s = workbook(&[1.0, 2.0, 3.0, 4.0, 100.0]);
    s.execute(
        "conditionalFormat.apply",
        json!({"range": "A1:A5", "rule": {"type": "iconSet", "set": "3Flags", "thresholds": [{"type": "number", "value": 2}, {"type": "number", "value": 4}]}}),
    )
    .unwrap();
    let icons = |s: &Session| [icon(s, "A1"), icon(s, "A2"), icon(s, "A3"), icon(s, "A4"), icon(s, "A5")];
    assert_eq!(icons(&s), [0, 1, 1, 2, 2]);
    let bytes = kition_sheet_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let (wb, _) = kition_sheet_xlsx::read_xlsx(&bytes).unwrap();
    let mut reopened = Session::new();
    reopened.add_document(kition_sheet_engine::DocState::new(wb, None, "Book".into()));
    assert_eq!(icons(&reopened), [0, 1, 1, 2, 2]);

    // Bare numbers are percent of the span, as in Excel's dialog.
    let mut s = workbook(&[0.0, 10.0, 50.0, 90.0, 100.0]);
    s.execute("conditionalFormat.apply", json!({"range": "A1:A5", "rule": {"type": "iconSet", "set": "3Symbols", "thresholds": [10, 90]}})).unwrap();
    assert_eq!(icons(&s), [0, 1, 1, 2, 2]);
    // One threshold per boundary between icons.
    assert!(
        s.execute("conditionalFormat.apply", json!({"range": "A1:A5", "rule": {"type": "iconSet", "set": "3Symbols", "thresholds": [10]}})).is_err()
    );
}
