//! Charts made from cells as Excel makes them. A scatter chart takes
//! its first column as X; a combo chart is columns and lines (and takes each series' type); a
//! numeric first column under a blank corner, or a column of dates, is the categories; a
//! multi-area range charts all its areas; a doughnut follows the theme in every slice; a
//! forecast trendline stays in the plot; a waterfall draws only the totals it is given.

use kition_sheet_chart::{ChartData, Prim, legend_entries, render, resolve, series_color};
use kition_sheet_engine::Session;
use kition_sheet_engine::model::{Chart, ChartKind};
use serde_json::{Value as Json, json};

fn workbook(values: Json) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range": "A1", "values": values})).unwrap();
    s
}

fn chart(s: &Session) -> Chart {
    s.doc().unwrap().wb.active().unwrap().charts.last().unwrap().clone()
}

fn data(s: &Session) -> ChartData {
    let wb = &s.doc().unwrap().wb;
    resolve(wb, wb.active_sheet, &chart(s))
}

struct Approx;

impl kition_sheet_chart::Measure for Approx {
    fn text_width(&self, text: &str, size: f32, _bold: bool) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }
}

#[test]
fn scatter_takes_the_first_column_as_x() {
    let mut s = workbook(json!([["Height", "Weight"], [150, 50], [160, 58], [175, 70]]));
    s.execute("chart.insert", json!({"range": "A1:B4", "type": "scatter"})).unwrap();
    let d = data(&s);
    assert_eq!(d.series.len(), 1);
    assert_eq!(d.series[0].name, "Weight");
    assert_eq!(d.series[0].x, Some(vec![Some(150.0), Some(160.0), Some(175.0)]));
    assert_eq!(d.series[0].values, vec![Some(50.0), Some(58.0), Some(70.0)]);

    // Without headers too.
    let mut s = workbook(json!([[1, 10], [2, 20], [4, 30]]));
    s.execute("chart.insert", json!({"range": "A1:B3", "type": "scatter"})).unwrap();
    assert_eq!(data(&s).series.len(), 1);
    assert_eq!(data(&s).series[0].x, Some(vec![Some(1.0), Some(2.0), Some(4.0)]));
}

#[test]
fn combo_is_columns_and_lines() {
    let table = json!([["Month", "Revenue", "Profit", "Margin"], ["Jan", 100, 30, 0.3], ["Feb", 120, 40, 0.33], ["Mar", 90, 20, 0.22]]);
    let mut s = workbook(table.clone());
    s.execute("chart.insert", json!({"range": "A1:D4", "type": "combo"})).unwrap();
    let c = chart(&s);
    assert_eq!(c.kind, ChartKind::Combo);
    let kinds: Vec<Option<ChartKind>> = c.series.iter().map(|x| x.kind).collect();
    assert_eq!(kinds, [Some(ChartKind::ColumnClustered), Some(ChartKind::Line), Some(ChartKind::Line)]);

    // Each series' type, and the secondary axis, as given.
    let mut s = workbook(table);
    s.execute("chart.insert", json!({"range": "A1:D4", "type": "combo", "seriesTypes": ["column", "column", "line"], "secondary": [2]})).unwrap();
    let c = chart(&s);
    let kinds: Vec<(Option<ChartKind>, bool)> = c.series.iter().map(|x| (x.kind, x.secondary)).collect();
    assert_eq!(kinds, [(Some(ChartKind::ColumnClustered), false), (Some(ChartKind::ColumnClustered), false), (Some(ChartKind::Line), true)]);
    // It stays so through .xlsx.
    let bytes = kition_sheet_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let (wb, _) = kition_sheet_xlsx::read_xlsx(&bytes).unwrap();
    let back = &wb.sheets[0].charts[0];
    let resolved = resolve(&wb, 0, back);
    let kinds: Vec<(ChartKind, bool)> = resolved.series.iter().map(|x| (x.kind, x.secondary)).collect();
    assert_eq!(kinds, [(ChartKind::ColumnClustered, false), (ChartKind::ColumnClustered, false), (ChartKind::Line, true)]);
    assert!(s.execute("chart.insert", json!({"range": "A1:D4", "type": "combo", "seriesTypes": ["column", "pie", "line"]})).is_err());
}

#[test]
fn a_numeric_first_column_under_a_blank_corner_is_the_categories() {
    let mut s = workbook(json!([[null, "Sales", "Costs"], [2021, 10, 4], [2022, 12, 5], [2023, 15, 6]]));
    s.execute("chart.insert", json!({"range": "A1:C4", "type": "column"})).unwrap();
    let d = data(&s);
    assert_eq!(d.series.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Sales", "Costs"]);
    assert_eq!(d.categories, ["2021", "2022", "2023"]);
}

#[test]
fn a_date_first_column_is_the_categories() {
    let mut s = workbook(json!([["Day", "Visits"], [46000, 10], [46001, 12], [46002, 15]]));
    s.execute("format.numberFormat", json!({"range": "A2:A4", "format": "d mmm"})).unwrap();
    s.execute("chart.insert", json!({"range": "A1:B4", "type": "line"})).unwrap();
    let d = data(&s);
    assert_eq!(d.series.len(), 1);
    assert_eq!(d.series[0].name, "Visits");
    assert_eq!(d.categories.len(), 3);
    assert_ne!(d.categories[0], "46000");
}

#[test]
fn a_multi_area_range_charts_every_area() {
    let mut s = workbook(json!([["Region", "Skip", "Q1", "Q2"], ["North", "x", 10, 11], ["South", "y", 20, 21], ["West", "z", 30, 31]]));
    s.execute("chart.insert", json!({"range": "A1:A4,C1:D4", "type": "column"})).unwrap();
    let d = data(&s);
    assert_eq!(d.series.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Q1", "Q2"]);
    assert_eq!(d.categories, ["North", "South", "West"]);
    assert_eq!(d.series[1].values, vec![Some(11.0), Some(21.0), Some(31.0)]);
    // Areas must line up.
    assert!(s.execute("chart.insert", json!({"range": "A1:A4,C2:D3", "type": "column"})).is_err());
}

#[test]
fn a_doughnut_follows_the_theme_in_every_slice() {
    let mut s = workbook(json!([["Brand", "Share"], ["A", 40], ["B", 30], ["C", 20], ["D", 10]]));
    s.execute("chart.insert", json!({"range": "A1:B5", "type": "doughnut"})).unwrap();
    let colors = ["#112233", "#445566", "#778899", "#AABBCC", "#102030", "#405060", "#708090", "#A0B0C0", "#123456", "#654321", "#0563C1", "#954F72"];
    s.execute("theme.colors", json!({"colors": colors})).unwrap();
    let wb = &s.doc().unwrap().wb;
    let c = chart(&s);
    let entries = legend_entries(&c, &resolve(wb, 0, &c));
    let slices: Vec<[u8; 4]> = entries.iter().map(|e| e.1).collect();
    let theme: Vec<[u8; 4]> = (0..4).map(|j| series_color(&wb.theme, j)).collect();
    assert_eq!(slices, theme);
    // Accent 2 (the theme's sixth colour).
    assert_eq!(slices[1], [0x40, 0x50, 0x60, 0xFF]);
}

#[test]
fn chart_change_colors_recolours_every_slice() {
    let mut s = workbook(json!([["Brand", "Share"], ["A", 40], ["B", 30], ["C", 20]]));
    s.execute("chart.insert", json!({"range": "A1:B4", "type": "pie"})).unwrap();
    s.execute("chart.changeColors", json!({"colors": ["#FF0000", "#00FF00", "#0000FF"]})).unwrap();
    let wb = &s.doc().unwrap().wb;
    let c = chart(&s);
    let slices: Vec<[u8; 4]> = legend_entries(&c, &resolve(wb, 0, &c)).iter().map(|e| e.1).collect();
    assert_eq!(slices, [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]);
    // Through .xlsx as well.
    let bytes = kition_sheet_xlsx::write_xlsx(wb).unwrap();
    let (back, _) = kition_sheet_xlsx::read_xlsx(&bytes).unwrap();
    let c = &back.sheets[0].charts[0];
    let slices: Vec<[u8; 4]> = legend_entries(c, &resolve(&back, 0, c)).iter().map(|e| e.1).collect();
    assert_eq!(slices, [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]);
}

/// Every point of every line in `prims` lies inside the chart's `w` × `h` box.
fn inside(prims: &[Prim], w: f32, h: f32) -> bool {
    prims.iter().all(|p| match p {
        Prim::Line { pts, .. } => pts.iter().all(|q| q[0] >= -0.5 && q[0] <= w + 0.5 && q[1] >= -0.5 && q[1] <= h + 0.5),
        _ => true,
    })
}

#[test]
fn a_forecast_trendline_stays_in_the_plot() {
    let mut s = workbook(json!([["Month", "Sales"], ["Jan", 10], ["Feb", 14], ["Mar", 19], ["Apr", 22]]));
    s.execute("chart.insert", json!({"range": "A1:B5", "type": "line"})).unwrap();
    let id = chart(&s).id.to_string();
    s.execute("drawing.chart", json!({"id": id, "seriesFormat": [{"index": 0, "trendline": {"kind": "linear", "forward": 6}}]})).unwrap();
    let d = data(&s);
    let c = chart(&s);
    let prims = render(&c, &d, 400.0, 300.0, &Approx);
    assert!(inside(&prims, 400.0, 300.0));
    // The plot's right edge: the trendline ends at or before the last gridline's end.
    let right = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { pts, dash: false, width, .. } if *width < 1.0 => pts.iter().map(|q| q[0]).reduce(f32::max),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    let trend_right = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { pts, width, .. } if (*width - 1.5).abs() < 1e-3 => pts.iter().map(|q| q[0]).reduce(f32::max),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(trend_right > 0.0 && trend_right <= right + 0.5, "trendline reaches {trend_right}, the plot ends at {right}");

    // A scatter chart's X axis widens to show the forecast.
    let mut s = workbook(json!([["X", "Y"], [1, 2], [2, 4], [3, 6]]));
    s.execute("chart.insert", json!({"range": "A1:B4", "type": "scatter"})).unwrap();
    let id = chart(&s).id.to_string();
    s.execute("drawing.chart", json!({"id": id, "seriesFormat": [{"index": 0, "trendline": {"kind": "linear", "forward": 5}}]})).unwrap();
    let prims = render(&chart(&s), &data(&s), 400.0, 300.0, &Approx);
    assert!(inside(&prims, 400.0, 300.0));
}

fn texts(prims: &[Prim]) -> Vec<String> {
    prims.iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.clone()) } else { None }).collect()
}

#[test]
fn a_waterfall_draws_only_the_totals_it_is_given() {
    let table = json!([["Step", "Amount"], ["Last year", 100], ["Volume", 20], ["Price", -5], ["Costs", -15], ["This year", "=SUM(B2:B5)"]]);
    let mut s = workbook(table.clone());
    s.execute("chart.insert", json!({"range": "A1:B6", "type": "waterfall", "totals": [0, 4]})).unwrap();
    let c = chart(&s);
    assert_eq!(c.totals, vec![0, 4]);
    let d = data(&s);
    assert_eq!(d.categories.len(), 5);
    let legend: Vec<String> = legend_entries(&c, &d).into_iter().map(|e| e.0).collect();
    assert_eq!(legend, ["Increase", "Decrease", "Total"]);
    let with_totals = render(&c, &d, 400.0, 300.0, &Approx);

    // No Total of its own: five points, five categories, no total in the legend.
    let mut s = workbook(table);
    s.execute("chart.insert", json!({"range": "A1:B6", "type": "waterfall"})).unwrap();
    let plain = chart(&s);
    assert!(plain.totals.is_empty());
    let prims = render(&plain, &data(&s), 400.0, 300.0, &Approx);
    assert!(!texts(&prims).iter().any(|t| t == "Total"), "{:?}", texts(&prims));
    let legend: Vec<String> = legend_entries(&plain, &data(&s)).into_iter().map(|e| e.0).collect();
    assert_eq!(legend, ["Increase", "Decrease"]);
    assert_ne!(with_totals, prims);

    // Totals can be changed and survive .xlsx.
    s.execute("chart.set", json!({"totals": [4]})).unwrap();
    assert_eq!(chart(&s).totals, vec![4]);
    let bytes = kition_sheet_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let (wb, _) = kition_sheet_xlsx::read_xlsx(&bytes).unwrap();
    assert_eq!(wb.sheets[0].charts[0].totals, vec![4]);
}
