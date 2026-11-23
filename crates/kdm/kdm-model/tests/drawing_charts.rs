//! Charts through the replica, as `Chart` objects: concurrent changes to different charts of a
//! sheet, and to different members of one chart, all survive (`KitDrawing` DR14), a row insert
//! that moves a chart's ranges among them.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, insert_rows, peer, put, sync};
use kition_sheet_model::{Anchor, Chart, ChartKind, LegendPos, Series, Workbook};

fn chart(id: u32, a1: &str) -> Chart {
    Chart {
        id,
        kind: ChartKind::ColumnClustered,
        anchor: Anchor { cell: at(a1), width: 300.0, height: 200.0, ..Default::default() },
        title: Some("Sales".into()),
        series: vec![Series {
            name: Some("Sheet1!$B$1".into()),
            categories: Some("Sheet1!$A$2:$A$3".into()),
            values: "Sheet1!$B$2:$B$3".into(),
            ..Default::default()
        }],
        legend: LegendPos::Bottom,
        data_labels: false,
        gridlines: true,
        style: 1,
        x_title: None,
        y_title: None,
        source: None,
        by_rows: false,
        x_axis: Default::default(),
        y_axis: Default::default(),
        label_position: None,
        totals: vec![],
        point_colors: vec![],
    }
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().workbook, &b.model().workbook);
    assert_workbook_valid(&a.model().workbook);
    assert_replica_clean(a.replica());
}

#[test]
fn concurrent_changes_to_two_charts_and_to_one_charts_members_all_stay() {
    let blobs = SharedBlobs::default();
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.charts = vec![chart(5, "E2"), chart(6, "E20")];
    }
    let mut a = first(&wb, &blobs);
    let mut b = peer(&a, 2, &blobs);
    assert_eq!(a.replica().objects_of_type("Chart").len(), 2, "each chart is an object");
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.charts[0].title = Some("From A".into());
            s.charts[1].anchor.dx = 30.0;
        }
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.charts[0].legend = LegendPos::Top;
            s.charts[0].data_labels = true;
            s.charts[1].kind = ChartKind::Line;
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let charts = a.model().workbook.sheet(0).map(|s| s.charts.clone()).unwrap_or_default();
    assert_eq!(charts[0].title.as_deref(), Some("From A"));
    assert_eq!((charts[0].legend, charts[0].data_labels), (LegendPos::Top, true));
    assert_eq!((charts[1].anchor.dx, charts[1].kind), (30.0, ChartKind::Line));
    // Deleting a chart deletes its object.
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.charts.remove(1);
        }
        vec![]
    });
    assert_eq!(a.replica().objects_of_type("Chart").len(), 1);
    assert_replica_clean(a.replica());
}

#[test]
fn a_trendline_a_secondary_series_and_a_retitle_all_stay() {
    let blobs = SharedBlobs::default();
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        let mut c = chart(5, "E2");
        c.series.push(Series { name: Some("Sheet1!$C$1".into()), values: "Sheet1!$C$2:$C$3".into(), ..Default::default() });
        s.charts = vec![c];
    }
    let mut a = first(&wb, &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.charts[0].series[0].trendline = Some(kition_sheet_model::SeriesTrend { kind: "linear".into(), ..Default::default() });
            s.charts[0].kind = ChartKind::Combo;
            s.charts[0].series[1].kind = Some(ChartKind::Line);
            s.charts[0].series[1].secondary = true;
        }
        vec![]
    });
    edit(&mut b, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.charts[0].title = Some("From B".into());
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let c = b.model().workbook.sheet(0).map(|s| s.charts[0].clone()).unwrap();
    assert_eq!(c.title.as_deref(), Some("From B"));
    assert_eq!(c.series[0].trendline.as_ref().map(|t| t.kind.as_str()), Some("linear"));
    assert_eq!((c.kind, c.series[1].kind, c.series[1].secondary), (ChartKind::Combo, Some(ChartKind::Line), true));
}

#[test]
fn a_row_insert_moving_a_charts_ranges_and_a_retitle_both_stay() {
    let blobs = SharedBlobs::default();
    let mut wb = Workbook::new();
    // The chart's data (its rows exist as objects, so its ranges are stored as positions).
    for (a1, v) in [("A2", "Q1"), ("B2", "10"), ("A3", "Q2"), ("B3", "20")] {
        put(&mut wb, 0, a1, v);
    }
    if let Some(s) = wb.sheet_mut(0) {
        s.charts = vec![chart(5, "E2")];
    }
    let mut a = first(&wb, &blobs);
    let mut b = peer(&a, 2, &blobs);
    // B retitles the chart while A inserts two rows above its data.
    edit(&mut a, &blobs, |wb| {
        let edit = insert_rows(wb, 0, 0, 2);
        // As the engine's row insert does: the chart's ranges move with their rows.
        if let Some(s) = wb.sheet_mut(0) {
            let series = &mut s.charts[0].series[0];
            series.values = "Sheet1!$B$4:$B$5".into();
            series.categories = Some("Sheet1!$A$4:$A$5".into());
            series.name = Some("Sheet1!$B$3".into());
        }
        vec![edit]
    });
    edit(&mut b, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.charts[0].title = Some("Moved".into());
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let c = &a.model().workbook.sheets[0].charts[0];
    assert_eq!(c.title.as_deref(), Some("Moved"));
    assert!(c.series[0].values.contains("$B$4:$B$5"), "the values moved down with their rows: {}", c.series[0].values);
}
