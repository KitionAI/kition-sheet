//! A chart's axes (bounds, major unit, number format, hidden), data label position, and its
//! series' line widths and markers are read, written and read back.

use kition_sheet_model::{Anchor, Chart, ChartAxis, ChartKind, Color, LegendPos, Series, Sheet, Workbook};
use std::sync::Arc;

use crate::{read_xlsx, write_xlsx};

fn chart(kind: ChartKind, series: Vec<Series>) -> Chart {
    Chart {
        id: 1,
        kind,
        anchor: Anchor { cell: kition_sheet_core::CellRef::new(0, 3), width: 480.0, height: 300.0, ..Anchor::default() },
        title: None,
        series,
        legend: LegendPos::Bottom,
        data_labels: true,
        gridlines: true,
        style: 2,
        x_title: None,
        y_title: None,
        source: None,
        by_rows: false,
        x_axis: ChartAxis { number_format: Some("mmm".into()), hidden: true, ..Default::default() },
        y_axis: ChartAxis { min: Some(-10.0), max: Some(250.0), major: Some(50.0), number_format: Some("0.0%".into()), hidden: false },
        label_position: Some("t".into()),
        totals: vec![],
        point_colors: vec![],
    }
}

fn round_trip(c: Chart) -> (Chart, String) {
    let mut wb = Workbook::default();
    let mut s = Sheet::new("Data");
    s.charts.push(c);
    wb.sheets = vec![Arc::new(s)];
    let bytes = write_xlsx(&wb).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("xl/charts/chart1.xml").unwrap(), &mut xml).unwrap();
    let (back, _) = read_xlsx(&bytes).unwrap();
    (back.sheets[0].charts[0].clone(), xml)
}

fn series(marker: Option<&str>, width: Option<f32>) -> Series {
    Series {
        name: Some("Data!$B$1".into()),
        categories: Some("Data!$A$2:$A$5".into()),
        values: "Data!$B$2:$B$5".into(),
        color: Some(Color::Rgb(0x4472C4)),
        line_width: width,
        marker: marker.map(str::to_string),
        ..Default::default()
    }
}

#[test]
fn axes_label_positions_widths_and_markers_round_trip() {
    let (back, _) = round_trip(chart(ChartKind::Line, vec![series(Some("diamond"), Some(3.5)), series(Some("none"), None)]));
    assert_eq!(back.y_axis, ChartAxis { min: Some(-10.0), max: Some(250.0), major: Some(50.0), number_format: Some("0.0%".into()), hidden: false });
    // A category axis keeps its format and visibility (it has no bounds).
    assert_eq!(back.x_axis, ChartAxis { number_format: Some("mmm".into()), hidden: true, ..Default::default() });
    assert_eq!(back.label_position.as_deref(), Some("t"));
    assert_eq!((back.series[0].marker.as_deref(), back.series[0].line_width), (Some("diamond"), Some(3.5)));
    // A line chart's series have no marker by default; one without a width is written with the
    // one it is drawn with (2.25 points). One series with a marker leaves it a line chart.
    assert_eq!((back.series[1].marker.as_deref(), back.series[1].line_width), (None, Some(2.25)));
    assert_eq!(back.kind, ChartKind::Line);
}

#[test]
fn a_line_with_markers_keeps_its_type_beside_a_series_without() {
    let (back, _) = round_trip(chart(ChartKind::LineMarkers, vec![series(None, None), series(Some("none"), None)]));
    // The series without markers of its own makes it a line chart whose other series has them.
    assert_eq!((back.kind, back.series[0].marker.as_deref(), back.series[1].marker.as_deref()), (ChartKind::Line, Some("auto"), None));
    let (back, _) = round_trip(chart(ChartKind::LineMarkers, vec![series(None, None), series(Some("star"), None)]));
    assert_eq!((back.kind, back.series[0].marker.as_deref(), back.series[1].marker.as_deref()), (ChartKind::LineMarkers, None, Some("star")));
}

#[test]
fn a_label_position_the_type_does_not_take_is_not_written() {
    // `t` is a line's position: Excel refuses a column chart naming it.
    let (back, xml) = round_trip(chart(ChartKind::ColumnClustered, vec![series(None, Some(1.0))]));
    assert!(!xml.contains("dLblPos"), "{xml}");
    assert_eq!(back.label_position, None);
    // A column's own position is.
    let mut c = chart(ChartKind::ColumnClustered, vec![series(None, None)]);
    c.label_position = Some("inEnd".into());
    assert_eq!(round_trip(c).0.label_position.as_deref(), Some("inEnd"));
    // Bars take an outline width; they have no marker.
    let (back, xml) = round_trip(chart(ChartKind::ColumnClustered, vec![series(Some("square"), Some(2.0))]));
    assert_eq!((back.series[0].line_width, back.series[0].marker.as_deref()), (Some(2.0), None));
    assert!(!xml.contains("<c:marker>"), "{xml}");
}
