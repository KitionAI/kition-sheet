//! Shared test helpers: a workbook exercising every replicated kind of content, a blob store
//! shared between replicas, the engine's structural edits replayed on a workbook (with the log
//! entry the engine would record), and a multi-replica harness over `KitSession<SheetModel>`.
//! Each `tests/*.rs` binary uses only some.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use kdm_model::{Ids, SheetModel, StructuralEdit, normalize, write_changes, write_document};
use kition_kit::kdm::{ActorId, DocId, ObjectId, Origin, Replica};
use kition_kit::session::{BlobRef, BlobSink, BlobSource, KitError, KitModel, KitSession, sha256_hex};
use kition_sheet_core::{CellRef, RangeRef, Value};
use kition_sheet_formula::adjust::{Axis, Edit, adjust};
use kition_sheet_model::{
    Anchor, AutoFilter, Cell, CellPicture, CfOperator, CfRule, CfValueKind, Chart, ChartKind, Comment, CondFormat, DefinedName, Formula, Hyperlink,
    Image, LineInfo, PivotTable, RichText, RunColor, RunFont, Series, Shape, ShapeKind, Sheet, SheetProtection, Sparkline, Style, StyleId, Table,
    TableColumn, TextRun, Validation, ValidationKind, Visibility, Workbook,
};

pub type Session = KitSession<SheetModel>;

/// One blob store every replica of a test shares (blobs live outside the replica).
#[derive(Clone, Default)]
pub struct SharedBlobs(pub Arc<Mutex<HashMap<String, Arc<[u8]>>>>);

impl BlobSource for SharedBlobs {
    fn get(&self, hash: &str) -> Option<Arc<[u8]>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(hash).cloned()
    }
}

impl BlobSink for SharedBlobs {
    fn put(&mut self, bytes: &[u8], mime: &str) -> BlobRef {
        let hash = sha256_hex(bytes);
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).entry(hash.clone()).or_insert_with(|| Arc::from(bytes));
        BlobRef { hash, mime: mime.to_owned(), size: bytes.len() as u64 }
    }
}

pub fn actor(value: u64) -> ActorId {
    ActorId::from_u64(value).unwrap_or_else(|_| panic!("actor {value}"))
}

pub fn person(name: &str) -> Origin {
    Origin::person(format!("user:{name}"))
}

pub fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap_or_else(|| panic!("cell {a1}"))
}

pub fn range(a1: &str) -> RangeRef {
    RangeRef::parse(a1).unwrap_or_else(|| panic!("range {a1}"))
}

/// A cell from typed input: `=…` a formula, a number, `TRUE`/`FALSE`, else text.
pub fn cell(input: &str) -> Cell {
    if input.starts_with('=') {
        return Cell::formula(Formula::new(input));
    }
    let value = match input {
        "TRUE" => Value::Bool(true),
        "FALSE" => Value::Bool(false),
        _ => input.parse::<f64>().map_or_else(|_| Value::Text(Arc::from(input)), Value::Number),
    };
    Cell::value(value)
}

/// Sets `a1` of sheet `sheet` from typed input, keeping its style.
pub fn put(wb: &mut Workbook, sheet: usize, a1: &str, input: &str) {
    let sh = wb.sheet_mut(sheet).unwrap_or_else(|| panic!("sheet {sheet}"));
    let c = at(a1);
    let style = sh.cells.get(c).map(|x| x.style).unwrap_or_default();
    let mut new = cell(input);
    new.style = style;
    sh.set_cell(c, new);
}

/// What the formula bar shows for `a1` of sheet `sheet`.
pub fn input(wb: &Workbook, sheet: usize, a1: &str) -> String {
    wb.sheet(sheet).map(|s| s.input_text(at(a1))).unwrap_or_default()
}

pub fn sheet_index(wb: &Workbook, name: &str) -> usize {
    wb.sheet_index(name).unwrap_or_else(|| panic!("sheet {name}"))
}

/// A real 1x1 PNG: the `.xlsx` writer checks a cell picture's header and size.
pub const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60, 0x00, 0x02, 0x00,
    0x00, 0x05, 0x00, 0x01, 0x7a, 0x5e, 0xab, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn chart(id: u32) -> Chart {
    Chart {
        id,
        kind: ChartKind::Line,
        anchor: Anchor { cell: at("F2"), dx: 2.5, dy: 1.0, width: 200.0, height: 120.0, mode: Default::default(), one_cell: false },
        title: Some("Totals".into()),
        series: vec![Series {
            name: Some("Data!$B$1".into()),
            categories: Some("Data!$A$2:$A$5".into()),
            values: "Data!$B$2:$B$5".into(),
            ..Default::default()
        }],
        legend: Default::default(),
        data_labels: true,
        gridlines: true,
        style: 2,
        x_title: None,
        y_title: Some("Amount".into()),
        source: Some("Data!$A$1:$B$5".into()),
        by_rows: false,
        x_axis: Default::default(),
        y_axis: Default::default(),
        label_position: None,
        totals: vec![],
        point_colors: vec![],
    }
}

/// A workbook with every kind of replicated content: two sheets (one hidden, with a tab colour),
/// literals of every kind, rich text, formulas within and across sheets (ranges, whole rows and columns,
/// absolute anchors, a spill reference, a legacy array formula, a LAMBDA name), styles on cells,
/// rows and columns, row and column info, merges, a conditional format, a validation, a table with
/// a calculated column, an autofilter, a chart, a shape, an image, a sparkline, a pivot table,
/// comments, links, a cell picture, frozen panes, print settings, protection, defined names of
/// both scopes, custom lists, cell styles, and content far below the materialised rows.
/// Rich text runs setting every run property, in three runs (the last plain).
pub fn rich_runs() -> RichText {
    let every = RunFont {
        name: Some("Cambria".into()),
        size: Some(10.5),
        bold: Some(false),
        italic: Some(true),
        underline: Some(kition_sheet_model::Underline::Double),
        strike: Some(true),
        color: Some(RunColor { theme: Some(4), tint: Some(-0.249977111117893), ..RunColor::default() }),
        vert: Some(kition_sheet_model::VertAlign::Superscript),
        family: Some(1),
        charset: Some(134),
        scheme: Some("major".into()),
        outline: Some(true),
        shadow: Some(false),
        condense: Some(true),
        extend: Some(false),
    };
    let red = RunFont { color: Some(RunColor { rgb: Some("FFFF0000".into()), ..RunColor::default() }), size: Some(11.0), ..RunFont::default() };
    RichText::new([TextRun::new("Total ", every), TextRun::new("due", red), TextRun::new(" now", RunFont::default())])
        .unwrap_or_else(|| panic!("rich runs"))
}

pub fn rich() -> Workbook {
    let mut wb = Workbook::new();
    wb.sheets.push(Arc::new(Sheet::new("Summary")));
    let bold = wb.styles.derive(StyleId::DEFAULT, |s| s.font.bold = true);
    let fill = wb.styles.derive(bold, |s| s.fill = kition_sheet_model::Fill::solid(kition_sheet_model::Color::rgb(200, 220, 255)));
    let pct = wb.styles.derive(StyleId::DEFAULT, |s| s.num_fmt = kition_sheet_model::NumFmt::new("0.0%"));
    {
        let d = wb.sheet_mut(0).unwrap_or_else(|| panic!("Data"));
        d.name = "Data".into();
    }
    for (a1, v) in [
        ("A1", "Item"),
        ("B1", "Amount"),
        ("C1", "Share"),
        ("A2", "Apples"),
        ("B2", "12"),
        ("A3", "Pears"),
        ("B3", "7.25"),
        ("A4", "Plums"),
        ("B4", "-3"),
        ("A5", "Figs"),
        ("B5", "1e300"),
        ("D1", "TRUE"),
        ("D2", "FALSE"),
        ("C2", "=B2/SUM($B$2:$B$5)"),
        ("C3", "=B3/SUM($B$2:$B$5)"),
        ("C4", "=SUM(2:3)+COUNT(B:B)"),
        ("C5", "=SEQUENCE(2)"),
        ("C7", "=SUM(C5#)"),
        ("C8", "=Summary!B2*2"),
        ("C9", "=Missing!A1"),
        ("C10", "=Double(4)"),
        ("C11", "=SUM(A1:A1048576)"),
        ("A400001", "far below"),
    ] {
        put(&mut wb, 0, a1, v);
    }
    {
        let d = wb.sheet_mut(0).unwrap_or_else(|| panic!("Data"));
        d.set_cell(at("E1"), Cell::value(Value::Text(Arc::from("12"))));
        d.set_cell(at("E2"), Cell::value(Value::Text(Arc::from("=not a formula"))));
        d.set_cell(at("E3"), Cell::value(Value::Error(kition_sheet_core::CellError::Div0)));
        d.set_cell(at("E4"), Cell::value(Value::Text(Arc::from(""))));
        d.set_cell(at("E5"), Cell { value: Value::Empty, formula: None, style: fill, rich: None });
        d.set_cell(at("E6"), Cell::rich_text(rich_runs(), bold));
        let mut array = Formula::new("=B2:B3*2");
        array.array = Some(range("F10:F11"));
        d.set_cell(at("F10"), Cell::formula(array));
        for a1 in ["A1", "B1", "C1"] {
            if let Some(c) = d.cells.get_mut(at(a1)) {
                c.style = bold;
            }
        }
        if let Some(c) = d.cells.get_mut(at("C2")) {
            c.style = pct;
        }
        d.rows.insert(0, LineInfo { size: Some(28.0), custom: true, style: Some(bold), ..Default::default() });
        d.rows.insert(6, LineInfo { hidden: true, ..Default::default() });
        d.rows.insert(500_000, LineInfo { size: Some(9.5), ..Default::default() });
        d.cols.insert(0, LineInfo { size: Some(90.0), ..Default::default() });
        d.cols.insert(3, LineInfo { outline: 1, collapsed: false, style: Some(StyleId::DEFAULT), ..Default::default() });
        d.merges = vec![range("A12:C13")];
        d.freeze = Some((1, 1));
        d.tab_color = Some(kition_sheet_model::Color::rgb(10, 120, 40));
        d.cond_formats = vec![
            CondFormat {
                ranges: vec![range("B2:B5")],
                rule: CfRule::CellIs { op: CfOperator::Greater, a: "$B$3".into(), b: None, style: Box::new(Style::default()) },
                priority: 1,
                stop_if_true: false,
            },
            CondFormat {
                ranges: vec![range("C:C")],
                rule: CfRule::Expression { formula: "C2>0.5".into(), style: Box::new(Style::default()) },
                priority: 2,
                stop_if_true: true,
            },
            CondFormat {
                ranges: vec![range("B2:B5")],
                rule: CfRule::DataBar {
                    min: CfValueKind::Formula("$B$2".into()),
                    max: CfValueKind::Max,
                    color: kition_sheet_model::Color::rgb(1, 2, 3),
                    gradient: true,
                    show_value: true,
                },
                priority: 3,
                stop_if_true: false,
            },
        ];
        d.validations = vec![Validation { ranges: vec![range("D2:D5")], kind: ValidationKind::List, f1: "=$A$2:$A$5".into(), ..Default::default() }];
        d.tables = vec![Table {
            id: 3,
            name: "Fruit".into(),
            range: range("H1:I4"),
            header_row: true,
            totals_row: false,
            columns: vec![
                TableColumn { name: "Name".into(), totals: Default::default(), totals_label: None, formula: None },
                TableColumn { name: "Double".into(), totals: Default::default(), totals_label: None, formula: Some("[@Name]&\"!\"".into()) },
            ],
            style: "TableStyleMedium2".into(),
            banded_rows: true,
            banded_cols: false,
            first_col: false,
            last_col: false,
            filter_button: true,
            criteria: vec![],
        }];
        d.autofilter = Some(AutoFilter { range: range("A1:C5"), criteria: vec![], extra: vec![] });
        d.charts = vec![chart(4)];
        d.shapes = vec![Shape {
            id: 5,
            kind: ShapeKind::TextBox,
            anchor: Anchor { cell: at("K2"), dx: 0.0, dy: 0.0, width: 80.0, height: 40.0, mode: Default::default(), one_cell: false },
            fill: kition_sheet_model::Color::Auto,
            line: kition_sheet_model::Color::Auto,
            text: "Note".into(),
            ..Default::default()
        }];
        d.images = vec![Image {
            id: 6,
            anchor: Anchor { cell: at("K8"), dx: 1.0, dy: 1.0, width: 30.0, height: 20.0, mode: Default::default(), one_cell: false },
            data: PNG.to_vec(),
            mime: "image/png".into(),
            alt: "logo".into(),
            blip_ext: None,
        }];
        d.sparklines = vec![Sparkline {
            cell: at("G2"),
            source: "Data!B2:B5".into(),
            kind: Default::default(),
            color: kition_sheet_model::Color::Auto,
            markers: true,
        }];
        d.pivots = vec![PivotTable {
            id: 1,
            name: "Pivot1".into(),
            source: "Data!$A$1:$B$5".into(),
            anchor: at("M2"),
            last_range: Some(range("M2:N6")),
            ..Default::default()
        }];
        d.comments.insert(
            at("A2"),
            Comment {
                author: "Ann".into(),
                text: "fresh".into(),
                replies: vec![("Bo".into(), "yes".into())],
                threaded: true,
                resolved: false,
                visible: false,
            },
        );
        d.hyperlinks.insert(at("A3"), Hyperlink::new("https://example.com".into(), Some("site".into())));
        d.hyperlinks.insert(at("A400001"), Hyperlink::new("Summary!A1".into(), None));
        d.set_picture(at("J2"), Arc::new(CellPicture { data: PNG.to_vec(), mime: "image/png".into(), alt: "in cell".into() }));
        d.print.print_area = Some(range("A1:D20"));
        d.print.title_rows = Some((0, 0));
        d.print.row_breaks = vec![9];
        d.print.header = "&CData".into();
        d.protection =
            Some(SheetProtection { password_hash: Some(kition_sheet_model::password_hash("pw")), select_locked: true, ..Default::default() });
        d.zoom = 130;
        d.view_active = at("B3");
    }
    put(&mut wb, 1, "A1", "Total");
    put(&mut wb, 1, "B1", "=SUM(Data!B2:B5)");
    put(&mut wb, 1, "B2", "=COUNT(Data!D:D)+Data!$B$2");
    {
        let s = wb.sheet_mut(1).unwrap_or_else(|| panic!("Summary"));
        s.visibility = Visibility::Hidden;
        s.show_gridlines = false;
    }
    wb.names = vec![
        DefinedName { name: "Prices".into(), scope: None, formula: "Data!$B$2:$B$5".into(), comment: "the prices".into(), hidden: false },
        DefinedName { name: "Local".into(), scope: Some(1), formula: "Summary!$B$1".into(), comment: String::new(), hidden: true },
        DefinedName { name: "Double".into(), scope: None, formula: "LAMBDA(x,x*2)".into(), comment: String::new(), hidden: false },
    ];
    wb.custom_lists = vec![vec!["Low".into(), "High".into()]];
    wb.cell_styles = vec![("Highlight".into(), wb.styles.get(fill).clone())];
    wb.calc.iterative = true;
    wb.props.title = "Rich".into();
    wb.apply_name_call_case();
    wb
}

/// A fresh replica of `wb` (its creating transaction, not an undo step) and its objects.
pub fn new_replica(wb: &Workbook, blobs: &SharedBlobs) -> (Replica, Ids) {
    let mut ids = Ids::default();
    let mut sink = blobs.clone();
    let (replica, ()) = Replica::create_with(SheetModel::schema(), DocId::new(), actor(1), &Origin::system("create"), |tx| {
        write_document(tx, wb, &mut ids, &mut sink).map(|_| ())
    })
    .unwrap_or_else(|error| panic!("create: {error}"));
    (replica, ids)
}

pub fn first(wb: &Workbook, blobs: &SharedBlobs) -> Session {
    let (replica, _) = new_replica(wb, blobs);
    KitSession::from_replica(replica, Box::new(blobs.clone())).unwrap_or_else(|error| panic!("attach: {error}"))
}

pub fn peer(of: &Session, value: u64, blobs: &SharedBlobs) -> Session {
    let snapshot = of.snapshot().unwrap_or_else(|error| panic!("snapshot: {error}"));
    KitSession::open_snapshot(&snapshot, Box::new(blobs.clone()), actor(value)).unwrap_or_else(|error| panic!("open_snapshot: {error}"))
}

/// Edits `session`'s view with `f` (which answers the structural edits it made, as the engine's
/// log would) and records what changed, the way the engine bridge will.
pub fn edit(session: &mut Session, blobs: &SharedBlobs, f: impl FnOnce(&mut Workbook) -> Vec<StructuralEdit>) {
    let before = session.model().workbook.clone();
    let mut after = before.clone();
    let edits = f(&mut after);
    let mut ids = session.model().ids().clone();
    let mut sink = blobs.clone();
    session
        .record(&person("test"), |tx| write_changes(tx, &before, &after, &edits, &mut ids, &mut sink).map(|_| ()).map_err(KitError::from))
        .unwrap_or_else(|error| panic!("record: {error}"));
    session.model_mut().adopt(after, ids);
}

pub fn send(from: &Session, to: &mut Session) {
    let bytes = from.export_changes(&to.version()).unwrap_or_else(|error| panic!("export: {error}"));
    to.import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
}

/// Exchanges until every session holds the same version (repairs are changes too).
pub fn sync_all(sessions: &mut [Session]) {
    for _ in 0..8 {
        for i in 0..sessions.len() {
            for j in 0..sessions.len() {
                if i != j {
                    let bytes = sessions[i].export_changes(&sessions[j].version()).unwrap_or_else(|error| panic!("export: {error}"));
                    sessions[j].import_changes(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
                }
            }
        }
        if sessions.windows(2).all(|w| w[0].version() == w[1].version()) {
            return;
        }
    }
    panic!("the replicas did not converge");
}

pub fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version() == b.version() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

/// Panics with where `a` and `b` first differ unless they normalize to the same workbook.
pub fn assert_same(what: &str, a: &Workbook, b: &Workbook) {
    let (x, y) = (normalize(a), normalize(b));
    if x == y {
        return;
    }
    assert_eq!(x.sheets.len(), y.sheets.len(), "{what}: sheet count");
    for (sa, sb) in x.sheets.iter().zip(&y.sheets) {
        assert_eq!(sa.name, sb.name, "{what}: sheet names");
        let cells_a: BTreeMap<CellRef, &Cell> = sa.cells.iter().collect();
        let cells_b: BTreeMap<CellRef, &Cell> = sb.cells.iter().collect();
        for (c, cell) in &cells_a {
            assert_eq!(Some(cell), cells_b.get(c), "{what}: sheet {}, cell {}", sa.name, c.a1());
        }
        for c in cells_b.keys() {
            assert!(cells_a.contains_key(c), "{what}: sheet {}, cell {} only on the right", sa.name, c.a1());
        }
        assert_eq!(sa.rows, sb.rows, "{what}: sheet {} row info", sa.name);
        assert_eq!(sa.cols, sb.cols, "{what}: sheet {} column info", sa.name);
        assert_eq!(sa.merges, sb.merges, "{what}: sheet {} merges", sa.name);
        assert_eq!(sa.cond_formats, sb.cond_formats, "{what}: sheet {} conditional formats", sa.name);
        assert_eq!(sa.validations, sb.validations, "{what}: sheet {} validations", sa.name);
        assert_eq!(sa.tables, sb.tables, "{what}: sheet {} tables", sa.name);
        assert_eq!(sa.charts, sb.charts, "{what}: sheet {} charts", sa.name);
        assert_eq!(sa.images, sb.images, "{what}: sheet {} images", sa.name);
        assert_eq!(sa.print, sb.print, "{what}: sheet {} print settings", sa.name);
        assert_eq!(sa.comments, sb.comments, "{what}: sheet {} comments", sa.name);
        assert_eq!(sa.hyperlinks, sb.hyperlinks, "{what}: sheet {} links", sa.name);
        assert_eq!(sa.cell_pictures, sb.cell_pictures, "{what}: sheet {} cell pictures", sa.name);
        assert_eq!(sa, sb, "{what}: sheet {}", sa.name);
    }
    assert_eq!(x.names, y.names, "{what}: names");
    assert_eq!(x.styles, y.styles, "{what}: styles");
    assert_eq!(x, y, "{what}: workbook");
}

/// The replica holds no list entry naming a deleted object, and no cell, comment or link keyed by
/// a deleted row or column.
pub fn assert_replica_clean(replica: &Replica) {
    let root = ObjectId::from_string("workbook");
    for field in ["sheets", "names"] {
        for item in replica.list(&root, field).unwrap_or_default() {
            assert!(replica.exists(&item), "workbook.{field} lists deleted {item}");
        }
    }
    for sheet in replica.list(&root, "sheets").unwrap_or_default() {
        let mut live: [HashSet<String>; 2] = [HashSet::new(), HashSet::new()];
        for (k, field) in ["rows", "cols"].into_iter().enumerate() {
            let items = replica.list(&sheet, field).unwrap_or_default();
            let distinct: HashSet<&ObjectId> = items.iter().collect();
            assert_eq!(distinct.len(), items.len(), "sheet {sheet}.{field} lists a line twice");
            for item in items {
                assert!(replica.exists(&item), "sheet {sheet}.{field} lists deleted {item}");
                if replica.get(&item, "gone").unwrap_or(serde_json::Value::Null) != serde_json::Value::Bool(true) {
                    live[k].insert(item.as_str().to_owned());
                }
            }
        }
        let token = |t: &str, set: &HashSet<String>| t.strip_prefix('#').map_or_else(|| set.contains(t), |n| n.parse::<u32>().is_ok());
        for field in ["cells", "comments", "hyperlinks"] {
            for (key, _) in replica.map_entries(&sheet, field).unwrap_or_default() {
                let ok = key.match_indices(':').any(|(i, _)| token(&key[..i], &live[0]) && token(&key[i + 1..], &live[1]));
                assert!(ok, "sheet {sheet}.{field} holds {key}, keyed by a deleted line");
            }
        }
    }
}

/// The engine's invariants hold in `wb`: at least one sheet, one visible, unique sheet names,
/// names scoped to existing sheets.
pub fn assert_workbook_valid(wb: &Workbook) {
    assert!(!wb.sheets.is_empty(), "a workbook has a sheet");
    assert!(wb.sheets.iter().any(|s| s.visibility == Visibility::Visible), "a sheet is visible");
    let names: HashSet<String> = wb.sheets.iter().map(|s| s.name.to_ascii_lowercase()).collect();
    assert_eq!(names.len(), wb.sheets.len(), "sheet names are unique");
    for n in &wb.names {
        assert!(n.scope.is_none_or(|s| s < wb.sheets.len()), "name {} is scoped to a sheet that exists", n.name);
    }
    assert!(wb.active_sheet < wb.sheets.len());
}

// ---------------------------------------------------------------- the engine's structural edits

fn map_line(v: u32, at: u32, count: u32, insert: bool, max: u32) -> Option<u32> {
    if insert {
        if v >= at { v.checked_add(count).filter(|n| *n < max) } else { Some(v) }
    } else if v < at {
        Some(v)
    } else if v >= at.saturating_add(count) {
        Some(v - count)
    } else {
        None
    }
}

/// A span `[lo, hi]` after an insert or delete (the engine's `map_span`).
fn map_span(lo: u32, hi: u32, at: u32, count: u32, insert: bool, max: u32) -> Option<(u32, u32)> {
    if insert {
        let lo2 = if lo >= at { lo as u64 + count as u64 } else { lo as u64 };
        let hi2 = (if hi >= at { hi as u64 + count as u64 } else { hi as u64 }).min(max as u64 - 1);
        (lo2 <= hi2).then_some((lo2 as u32, hi2 as u32))
    } else {
        let end = at.saturating_add(count);
        if lo >= at && hi < end {
            return None;
        }
        let lo2 = if lo < at {
            lo
        } else if lo >= end {
            lo - count
        } else {
            at
        };
        let hi2 = if hi < at {
            hi
        } else if hi >= end {
            hi - count
        } else {
            at.saturating_sub(1)
        };
        Some((lo2, hi2.max(lo2)))
    }
}

fn map_range(r: RangeRef, rows: bool, at: u32, count: u32, insert: bool) -> Option<RangeRef> {
    if rows {
        let (a, b) = map_span(r.start.row, r.end.row, at, count, insert, kition_sheet_core::MAX_ROWS)?;
        Some(RangeRef::new(CellRef::new(a, r.start.col), CellRef::new(b, r.end.col)))
    } else {
        let (a, b) = map_span(r.start.col, r.end.col, at, count, insert, kition_sheet_core::MAX_COLS)?;
        Some(RangeRef::new(CellRef::new(r.start.row, a), CellRef::new(r.end.row, b)))
    }
}

/// Rewrites every formula and defined name for `e` on sheet `target` (the engine's
/// `rewrite_all_formulas`).
pub fn rewrite(wb: &mut Workbook, target: &str, e: &Edit) {
    for si in 0..wb.sheets.len() {
        let host = wb.sheets[si].name.clone();
        let formulas: Vec<(CellRef, Arc<Formula>)> =
            wb.sheets[si].cells.iter().filter_map(|(c, cell)| cell.formula.clone().map(|f| (c, f))).collect();
        let Some(sheet) = wb.sheet_mut(si) else { continue };
        for (c, f) in formulas {
            let Some(expr) = f.expr() else { continue };
            let new = adjust(expr, &host, target, e);
            if kition_sheet_formula::print(&new) != f.text
                && let Some(cell) = sheet.cells.get_mut(c)
            {
                let mut nf = Formula::from_expr(new);
                nf.array = f.array;
                cell.formula = Some(Arc::new(nf));
            }
        }
    }
    for n in wb.names.iter_mut() {
        if let Ok(expr) = kition_sheet_formula::parse(&n.formula) {
            n.formula = kition_sheet_formula::print(&adjust(expr, "\u{0}", target, e));
        }
    }
}

/// Inserts (`insert`) or deletes `count` rows (`rows`) or columns at `at` of sheet `sheet` the
/// way the engine does (cells, line info, merges, notes, links, the print area, anchors, formulas), and answers the log
/// entry the engine records.
pub fn lines(wb: &mut Workbook, sheet: usize, rows: bool, at: u32, count: u32, insert: bool) -> StructuralEdit {
    let axis = if rows { Axis::Rows } else { Axis::Cols };
    let e = if insert { Edit::Insert { axis, at, count } } else { Edit::Delete { axis, at, count } };
    let name = wb.sheets[sheet].name.clone();
    let max = if rows { kition_sheet_core::MAX_ROWS } else { kition_sheet_core::MAX_COLS };
    {
        let sh = wb.sheet_mut(sheet).unwrap_or_else(|| panic!("sheet {sheet}"));
        if !insert {
            let last = (at + count - 1).min(max - 1);
            let block = if rows { RangeRef::rows(at, last) } else { RangeRef::cols(at, last) };
            sh.take_cells(block);
        }
        match (rows, insert) {
            (true, true) => sh.cells.shift_rows(at, count as i64),
            (true, false) => sh.cells.shift_rows(at + count, -(count as i64)),
            (false, true) => sh.cells.shift_cols(at, count as i64),
            (false, false) => sh.cells.shift_cols(at + count, -(count as i64)),
        }
        let info = if rows { &mut sh.rows } else { &mut sh.cols };
        *info = std::mem::take(info).into_iter().filter_map(|(k, v)| map_line(k, at, count, insert, max).map(|n| (n, v))).collect();
        sh.merges = sh.merges.iter().filter_map(|m| map_range(*m, rows, at, count, insert)).filter(|m| !m.is_single()).collect();
        let cell = |c: CellRef| -> Option<CellRef> {
            if rows {
                map_line(c.row, at, count, insert, max).map(|r| CellRef::new(r, c.col))
            } else {
                map_line(c.col, at, count, insert, max).map(|x| CellRef::new(c.row, x))
            }
        };
        sh.comments = std::mem::take(&mut sh.comments).into_iter().filter_map(|(c, v)| cell(c).map(|c| (c, v))).collect();
        sh.hyperlinks = std::mem::take(&mut sh.hyperlinks).into_iter().filter_map(|(c, v)| cell(c).map(|c| (c, v))).collect();
        sh.cell_pictures = std::mem::take(&mut sh.cell_pictures).into_iter().filter_map(|(c, v)| cell(c).map(|c| (c, v))).collect();
        sh.print.print_area = sh.print.print_area.and_then(|r| map_range(r, rows, at, count, insert));
        // Floating objects and sparklines move with their anchor cell (the engine also refits an
        // object a delete cuts through; none here is).
        let mut anchors: Vec<&mut CellRef> = sh.charts.iter_mut().map(|o| &mut o.anchor.cell).collect();
        anchors.extend(sh.images.iter_mut().map(|o| &mut o.anchor.cell));
        anchors.extend(sh.shapes.iter_mut().map(|o| &mut o.anchor.cell));
        anchors.extend(sh.sparklines.iter_mut().map(|o| &mut o.cell));
        for c in anchors {
            if let Some(moved) = cell(*c) {
                *c = moved;
            }
        }
    }
    rewrite(wb, &name, &e);
    StructuralEdit::new(sheet, e)
}

pub fn insert_rows(wb: &mut Workbook, sheet: usize, at: u32, count: u32) -> StructuralEdit {
    lines(wb, sheet, true, at, count, true)
}

pub fn delete_rows(wb: &mut Workbook, sheet: usize, at: u32, count: u32) -> StructuralEdit {
    lines(wb, sheet, true, at, count, false)
}

/// Renames sheet `sheet`, rewriting the formulas that name it (the engine's `sheet.rename`).
pub fn rename_sheet(wb: &mut Workbook, sheet: usize, name: &str) {
    let old = wb.sheets[sheet].name.clone();
    for si in 0..wb.sheets.len() {
        let formulas: Vec<(CellRef, Arc<Formula>)> =
            wb.sheets[si].cells.iter().filter_map(|(c, cell)| cell.formula.clone().map(|f| (c, f))).collect();
        let Some(sh) = wb.sheet_mut(si) else { continue };
        for (c, f) in formulas {
            let Some(expr) = f.expr() else { continue };
            let new = kition_sheet_formula::adjust::rename_sheet(expr, &old, name);
            if kition_sheet_formula::print(&new) != f.text
                && let Some(cell) = sh.cells.get_mut(c)
            {
                let mut nf = Formula::from_expr(new);
                nf.array = f.array;
                cell.formula = Some(Arc::new(nf));
            }
        }
    }
    for n in wb.names.iter_mut() {
        if let Ok(expr) = kition_sheet_formula::parse(&n.formula) {
            n.formula = kition_sheet_formula::print(&kition_sheet_formula::adjust::rename_sheet(expr, &old, name));
        }
    }
    // Chart series too, as the engine's `sheet.rename` does.
    let fix = |t: &str| {
        kition_sheet_formula::parse(t)
            .map(|e| kition_sheet_formula::print(&kition_sheet_formula::adjust::rename_sheet(e, &old, name)))
            .unwrap_or_else(|_| t.to_owned())
    };
    for si in 0..wb.sheets.len() {
        let Some(sh) = wb.sheet_mut(si) else { continue };
        for chart in sh.charts.iter_mut() {
            for series in chart.series.iter_mut() {
                series.values = fix(&series.values);
                series.categories = series.categories.as_deref().map(fix);
                series.name = series.name.as_deref().map(fix);
            }
        }
    }
    if let Some(sh) = wb.sheet_mut(sheet) {
        sh.name = name.to_owned();
    }
}

/// Deletes sheet `sheet` (the engine's `home.deleteSheet`): formulas naming it read `#REF!`,
/// names scoped to it go, later scopes shift.
pub fn delete_sheet(wb: &mut Workbook, sheet: usize) {
    let order: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
    let name = wb.sheets[sheet].name.clone();
    wb.sheets.remove(sheet);
    for si in 0..wb.sheets.len() {
        let formulas: Vec<(CellRef, Arc<Formula>)> =
            wb.sheets[si].cells.iter().filter_map(|(c, cell)| cell.formula.clone().map(|f| (c, f))).collect();
        let Some(sh) = wb.sheet_mut(si) else { continue };
        for (c, f) in formulas {
            let Some(expr) = f.expr() else { continue };
            let new = kition_sheet_formula::adjust::delete_sheet(expr, &name, &order);
            if kition_sheet_formula::print(&new) != f.text
                && let Some(cell) = sh.cells.get_mut(c)
            {
                let mut nf = Formula::from_expr(new);
                nf.array = f.array;
                cell.formula = Some(Arc::new(nf));
            }
        }
    }
    wb.names.retain(|n| n.scope != Some(sheet));
    for n in wb.names.iter_mut() {
        if let Some(s) = n.scope.as_mut()
            && *s > sheet
        {
            *s -= 1;
        }
        if let Ok(expr) = kition_sheet_formula::parse(&n.formula) {
            n.formula = kition_sheet_formula::print(&kition_sheet_formula::adjust::delete_sheet(expr, &name, &order));
        }
    }
    if !wb.sheets.iter().any(|s| s.visibility == Visibility::Visible)
        && let Some(s) = wb.sheet_mut(0)
    {
        s.visibility = Visibility::Visible;
    }
    wb.active_sheet = wb.active_sheet.min(wb.sheets.len().saturating_sub(1));
}

/// Inserts a new sheet at `at` (the engine's `home.insertSheet`): later scopes shift.
pub fn insert_sheet(wb: &mut Workbook, at: usize, name: &str) {
    let at = at.min(wb.sheets.len());
    wb.sheets.insert(at, Arc::new(Sheet::new(name)));
    for n in wb.names.iter_mut() {
        if let Some(s) = n.scope.as_mut()
            && *s >= at
        {
            *s += 1;
        }
    }
}
