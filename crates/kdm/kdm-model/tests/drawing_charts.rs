//! Charts through the replica, as `Chart` objects (schema version 4): concurrent changes to
//! different charts of a sheet, and to different members of one chart, all survive (`KitDrawing`
//! DR14), a row insert that moves a chart's ranges among them; a replica that kept its charts in
//! the sheet's `features` (schema version 3) reads and migrates.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, insert_rows, peer, put, sync};
use kdm_model::SheetSession;
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema};
use kition_kit::session::MemoryBlobs;
use kition_sheet_model::{Anchor, Chart, ChartKind, LegendPos, Series, Workbook};
use serde_json::json;

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

/// The schema replicas were written under before charts were objects: version 3, without `Chart`.
fn schema_v3() -> std::sync::Arc<Schema> {
    let mut raw: serde_json::Value = serde_json::from_str(include_str!("../model/schema.json")).unwrap_or_else(|e| panic!("schema: {e}"));
    raw["schemaVersion"] = json!(3);
    raw["types"].as_object_mut().unwrap_or_else(|| panic!("types")).remove("Chart");
    std::sync::Arc::new(Schema::parse(&raw.to_string()).unwrap_or_else(|e| panic!("v3 schema: {e}")))
}

/// A replica as a schema 3 build wrote it: one sheet whose `features` hold two charts.
fn replica_v3() -> Vec<u8> {
    let actor = ActorId::from_u64(7).unwrap_or_else(|_| panic!("actor"));
    let (replica, ()) = Replica::create_with(schema_v3(), DocId::new(), actor, &Origin::system("create"), |tx| {
        let root = ObjectId::from_string("workbook");
        tx.create_with_id("Workbook", &root, serde_json::Map::new())?;
        let sheet = tx.create("Sheet", [("name".to_owned(), json!("Data"))].into_iter().collect())?;
        tx.list_insert(&root, "sheets", 0, &sheet)?;
        let mut lines = Vec::new();
        for (field, k) in [("rows", 0), ("rows", 1), ("cols", 0), ("cols", 1)] {
            let line = tx.create(if field == "rows" { "Row" } else { "Col" }, serde_json::Map::new())?;
            tx.list_insert(&sheet, field, k, &line)?;
            lines.push(line);
        }
        let cell = format!("{},{}", lines[1].as_str(), lines[3].as_str());
        let anchor = json!({ "cell": cell, "dx": 0.0, "dy": 0.0, "width": 300.0, "height": 200.0, "mode": "MoveAndSize" });
        let features = json!({ "charts": [
            { "id": 5, "kind": "Pie", "anchor": anchor, "title": "Kept", "series": [{ "name": null, "categories": null, "values": "1,2", "bubble_sizes": null, "color": null, "secondary": false, "kind": null }], "legend": "Right", "data_labels": false, "gridlines": false, "style": 1, "x_title": null, "y_title": null, "source": null, "by_rows": false },
            { "id": 6, "kind": "Line", "anchor": anchor, "title": "Other", "series": [], "legend": "Bottom", "data_labels": false, "gridlines": true, "style": 1, "x_title": null, "y_title": null, "source": null, "by_rows": false },
        ] });
        tx.set(&sheet, "features", features)?;
        tx.raise_schema_version(3)?;
        Ok(())
    })
    .unwrap_or_else(|e| panic!("v3 replica: {e}"));
    replica.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"))
}

#[test]
fn a_version_3_replica_reads_its_charts_and_moves_them_into_objects_on_its_first_write() {
    let bytes = replica_v3();
    let mut s = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open: {e}"));
    assert!(!s.migrated());
    let check = |wb: &Workbook| {
        let charts = &wb.sheets[0].charts;
        assert_eq!(charts.iter().map(|c| c.id).collect::<Vec<_>>(), vec![5, 6]);
        assert_eq!((charts[0].kind, charts[0].title.as_deref(), charts[0].anchor.cell), (ChartKind::Pie, Some("Kept"), at("B2")));
    };
    check(s.workbook());
    let mut after = s.workbook().clone();
    if let Some(sheet) = after.sheet_mut(0) {
        sheet.charts[1].title = Some("New".into());
    }
    assert_eq!(s.record(&Origin::person("user:a"), &after, &[]).unwrap_or_else(|e| panic!("record: {e}")), 1, "one undo step: the edit");
    assert!(s.migrated());
    assert_eq!(s.replica().min_schema_version(), 4);
    assert_eq!(s.replica().objects_of_type("Chart").len(), 2);
    let sheet = s.model().ids().sheets[0].object.clone();
    assert!(s.replica().get(&sheet, "features").unwrap_or_default().get("charts").is_none(), "no charts left in `features`");
    let (read, _) = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("read: {e}"));
    assert_eq!(read.sheets[0].charts[1].title.as_deref(), Some("New"));
    assert_eq!(read.sheets[0].charts[0].title.as_deref(), Some("Kept"));
    // A schema 3 kit refuses the migrated replica rather than showing it without its charts.
    let actor = ActorId::from_u64(11).unwrap_or_else(|_| panic!("actor"));
    let migrated = s.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    assert!(matches!(Replica::load(schema_v3(), &migrated, actor), Err(KdmError::NewerSchema { found: 4, reads: 3 })));
    // Undo takes back the edit, not the migration.
    assert!(s.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    check(s.workbook());
    assert_eq!(s.replica().objects_of_type("Chart").len(), 2, "the charts stay objects after the undo");
}

#[test]
fn two_people_migrating_one_replica_at_once_converge_on_the_same_charts() {
    let bytes = replica_v3();
    let mut a = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open a: {e}"));
    let mut b = SheetSession::open_snapshot(&bytes, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open b: {e}"));
    for (s, title) in [(&mut a, "from a"), (&mut b, "from b")] {
        let mut after = s.workbook().clone();
        if let Some(sheet) = after.sheet_mut(0) {
            sheet.charts[0].title = Some(title.into());
        }
        s.record(&Origin::person("user:x"), &after, &[]).unwrap_or_else(|e| panic!("record: {e}"));
    }
    let to_b = a.export_changes(&b.version()).unwrap_or_else(|e| panic!("export: {e}"));
    let to_a = b.export_changes(&a.version()).unwrap_or_else(|e| panic!("export: {e}"));
    b.import_changes(&to_b).unwrap_or_else(|e| panic!("import b: {e}"));
    a.import_changes(&to_a).unwrap_or_else(|e| panic!("import a: {e}"));
    assert_eq!(a.replica().objects_of_type("Chart").len(), 2, "the same two objects, not four");
    assert_eq!(a.workbook().sheets[0].charts, b.workbook().sheets[0].charts);
}
