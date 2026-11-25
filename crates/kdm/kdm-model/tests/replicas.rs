//! Two replicas through `KitSession<SheetModel>`, one following the other's changes: an undone
//! row delete brings the row back on both, and what a workbook keeps of its fonts, theme, palette,
//! window, styles, links, extensions and outline reaches the peer.

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, delete_rows, edit, first, input, peer, put, sync};
use kition_sheet_model::Workbook;

/// A small sheet `Data` (A1:B6: labels and numbers), and `Other`.
fn base() -> Workbook {
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.name = "Data".into();
    }
    wb.sheets.push(std::sync::Arc::new(kition_sheet_model::Sheet::new("Other")));
    for r in 1..=6 {
        put(&mut wb, 0, &format!("A{r}"), &format!("row {r}"));
        put(&mut wb, 0, &format!("B{r}"), &format!("{}", r * 10));
    }
    wb
}

fn pair() -> (SharedBlobs, Session, Session) {
    let blobs = SharedBlobs::default();
    let a = first(&base(), &blobs);
    let b = peer(&a, 2, &blobs);
    (blobs, a, b)
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().workbook, &b.model().workbook);
    assert_workbook_valid(&a.model().workbook);
    assert_replica_clean(a.replica());
}

fn data(s: &Session, a1: &str) -> String {
    input(&s.model().workbook, 0, a1)
}

#[test]
fn undoing_a_row_delete_brings_the_row_back() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| vec![delete_rows(wb, 0, 1, 2)]);
    sync(&mut a, &mut b);
    assert_eq!(data(&b, "A2"), "row 4");
    a.undo().unwrap_or_else(|error| panic!("undo: {error}"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!((data(&b, "A2"), data(&b, "B3")), ("row 2".into(), "30".into()));
}

#[test]
fn a_fonts_character_set_theme_reference_and_the_workbooks_palette_reach_the_peer() {
    let (blobs, mut a, mut b) = pair();
    edit(&mut a, &blobs, |wb| {
        let style = wb.styles.derive(kition_sheet_model::StyleId::DEFAULT, |s| {
            s.font.name = "SimSun".into();
            s.font.charset = Some(134);
            s.font.scheme = Some("minor".into());
        });
        if let Some(sheet) = wb.sheet_mut(0) {
            sheet.set_style(common::at("A1"), style);
        }
        wb.palette = Some(kition_sheet_model::Palette {
            indexed: vec![0x000000, 0x123456],
            recent: vec![kition_sheet_model::RunColor { theme: Some(4), tint: Some(-0.25), ..Default::default() }],
        });
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let wb = &b.model().workbook;
    let font = &wb.styles.get(wb.sheet(0).and_then(|s| s.cell(common::at("A1"))).map(|c| c.style).unwrap_or_default()).font;
    assert_eq!((font.charset, font.scheme.as_deref()), (Some(134), Some("minor")));
    assert_eq!(wb.palette.as_ref().map(|p| p.indexed.clone()), Some(vec![0x000000, 0x123456]));
    assert_eq!(wb.palette, a.model().workbook.palette);
}

#[test]
fn the_workbook_window_file_version_and_built_in_styles_reach_the_peer_and_a_0_4_reader_still_reads_the_props() {
    let (blobs, mut a, mut b) = pair();
    let view = kition_sheet_model::WorkbookView {
        width: Some(29040),
        height: Some(15720),
        tab_ratio: Some(745),
        first_sheet: Some(1),
        ..Default::default()
    };
    let version = kition_sheet_model::FileVersion {
        app_name: "xl".into(),
        last_edited: "7".into(),
        lowest_edited: "6".into(),
        rup_build: "27425".into(),
        ..Default::default()
    };
    edit(&mut a, &blobs, |wb| {
        wb.view = Some(view.clone());
        wb.file_version = Some(version.clone());
        wb.cell_styles.push(("Gut".into(), kition_sheet_model::Style::default()));
        wb.cell_style_info.insert("Gut".into(), kition_sheet_model::CellStyleInfo { builtin: Some(26), ..Default::default() });
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let wb = &b.model().workbook;
    assert_eq!((wb.view.as_ref(), wb.file_version.as_ref()), (Some(&view), Some(&version)));
    assert_eq!(wb.cell_style_info.get("Gut").and_then(|i| i.builtin), Some(26));

    // The props register is JSON (no schema change): a 0.4.x reader takes the keys it knows and
    // leaves the new ones alone.
    let props = b.replica().get(&kition_kit::kdm::ObjectId::from_string("workbook"), "props").unwrap_or_else(|e| panic!("props: {e}"));
    assert!(["view", "fileVersion", "cellStyleInfo"].iter().all(|k| props.get(k).is_some()), "{props}");
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct Props04 {
        date_system: kition_sheet_core::DateSystem,
        calc: kition_sheet_model::CalcSettings,
        doc_props: kition_sheet_model::DocProps,
        protected_structure: bool,
    }
    let old: Props04 = serde_json::from_value(props).unwrap_or_else(|e| panic!("a 0.4.x reader reads the props: {e}"));
    assert_eq!(old.doc_props, wb.props);
}

/// A workbook whose Normal style is its file's own (not Kition's default).
fn with_normal(name: &str) -> Workbook {
    let mut wb = base();
    let mut normal = kition_sheet_model::Style::default();
    normal.font.name = name.into();
    normal.font.size = 10.0;
    normal.align.wrap = true;
    wb.styles.set_default(normal);
    wb
}

#[test]
fn a_files_own_normal_style_reaches_the_peer_and_follows_a_change() {
    use kition_sheet_model::StyleId;
    let blobs = SharedBlobs::default();
    let mut a = first(&with_normal("Arial"), &blobs);
    let mut b = peer(&a, 2, &blobs);
    for s in [&a, &b] {
        let wb = &s.model().workbook;
        assert_eq!(wb.styles.get(StyleId::DEFAULT).font.name, "Arial");
        assert_eq!(wb.sheet(0).and_then(|sh| sh.cell(common::at("A1"))).map(|c| c.style), Some(StyleId::DEFAULT));
    }
    // A bold cell derived from it, and Kition's own default as a style like any other.
    edit(&mut a, &blobs, |wb| {
        let bold = wb.styles.derive(StyleId::DEFAULT, |s| s.font.bold = true);
        let kition = wb.styles.intern(kition_sheet_model::Style::default());
        if let Some(sheet) = wb.sheet_mut(0) {
            sheet.set_style(common::at("A2"), bold);
            sheet.set_style(common::at("A3"), kition);
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let wb = &b.model().workbook;
    let style = |a1: &str| wb.styles.get(wb.sheet(0).and_then(|sh| sh.cell(common::at(a1))).map(|c| c.style).unwrap_or_default()).clone();
    assert!(style("A2").font.bold && style("A2").font.name == "Arial");
    assert_eq!(style("A3"), kition_sheet_model::Style::default());
    assert_eq!(style("A1").font.name, "Arial");
    // The file changed elsewhere with another Normal style: the peer follows.
    edit(&mut a, &blobs, |wb| {
        let mut normal = wb.styles.get(StyleId::DEFAULT).clone();
        normal.font.name = "Georgia".into();
        wb.styles.set_default(normal);
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(b.model().workbook.styles.get(StyleId::DEFAULT).font.name, "Georgia");
    // A workbook on Kition's default names none: its replica reads as before.
    let plain = first(&base(), &blobs);
    assert_eq!(plain.model().workbook.styles.get(StyleId::DEFAULT), &kition_sheet_model::Style::default());
}

#[test]
fn what_a_file_keeps_of_its_links_styles_extensions_and_outline_reaches_the_peer() {
    use kition_sheet_model::{
        Anchor, AutoFilter, Extensions, ForeignAttr, Hyperlink, Image, StyleParts, TableStyleDef, TableStyleElement, TableStyles,
    };
    let (blobs, mut a, mut b) = pair();
    let tables = TableStyles {
        default_table: Some("TableStyleMedium9".into()),
        default_pivot: None,
        styles: vec![TableStyleDef {
            name: "Team Blue".into(),
            pivot: Some(false),
            table: None,
            uid: Some("{00000000-0000-0000-0000-000000000009}".into()),
            elements: vec![TableStyleElement { kind: "firstRowStripe".into(), size: Some(2), style: kition_sheet_model::Style::default() }],
        }],
    };
    let extensions = Extensions { workbook: vec!["<ext uri=\"{0}\"/>".into()], styles: vec!["<ext uri=\"{1}\"/>".into()] };
    let link = Hyperlink { target: "docs/report.xlsx".into(), tooltip: None, display: Some("the report".into()), external: true };
    let extra = vec![ForeignAttr { name: "v:followRange".into(), ns: "urn:example:vendor".into(), value: "0".into() }];
    let excludes = StyleParts { font: true, alignment: true, ..Default::default() };
    edit(&mut a, &blobs, |wb| {
        wb.table_styles = tables.clone();
        wb.extensions = extensions.clone();
        wb.calc.full_calc_on_load = true;
        wb.cell_styles.push(("One decimal".into(), kition_sheet_model::Style::default()));
        wb.cell_style_info.insert("One decimal".into(), kition_sheet_model::CellStyleInfo { excludes, ..Default::default() });
        if let Some(sheet) = wb.sheet_mut(0) {
            sheet.outline_levels = Some((0, 3));
            sheet.hyperlinks.insert(common::at("A1"), link.clone());
            sheet.autofilter = Some(AutoFilter { range: common::range("A1:B6"), criteria: vec![], extra: extra.clone() });
            sheet.images.push(Image {
                id: 7,
                anchor: Anchor { cell: common::at("D2"), dx: 0.0, dy: 0.0, width: 30.0, height: 20.0, mode: Default::default(), one_cell: false },
                data: common::PNG.to_vec(),
                mime: "image/png".into(),
                alt: String::new(),
                blip_ext: Some("<a:ext uri=\"{2}\"/>".into()),
            });
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let wb = &b.model().workbook;
    assert_eq!((&wb.table_styles, &wb.extensions, wb.calc.full_calc_on_load), (&tables, &extensions, true));
    assert_eq!(wb.cell_style_info.get("One decimal").map(|i| i.excludes), Some(excludes));
    let sheet = wb.sheet(0).unwrap_or_else(|| panic!("a sheet"));
    assert_eq!(sheet.outline_levels, Some((0, 3)));
    assert_eq!(sheet.hyperlinks.get(&common::at("A1")), Some(&link));
    assert_eq!(sheet.autofilter.as_ref().map(|f| &f.extra), Some(&extra));
    assert_eq!(sheet.images.first().and_then(|i| i.blip_ext.as_deref()), Some("<a:ext uri=\"{2}\"/>"));

    // Additive keys of JSON registers (no schema change): a 0.4.x reader reads the props.
    let props = b.replica().get(&kition_kit::kdm::ObjectId::from_string("workbook"), "props").unwrap_or_else(|e| panic!("props: {e}"));
    assert!(["tableStyles", "extensions"].iter().all(|k| props.get(k).is_some()), "{props}");
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct Props04 {
        date_system: kition_sheet_core::DateSystem,
        calc: kition_sheet_model::CalcSettings,
        doc_props: kition_sheet_model::DocProps,
        protected_structure: bool,
    }
    let old: Props04 = serde_json::from_value(props).unwrap_or_else(|e| panic!("a 0.4.x reader reads the props: {e}"));
    assert_eq!(old.doc_props, wb.props);
}
