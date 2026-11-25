//! Drawing shapes through the replica, as `Shape` objects: groups, preset and
//! custom geometry, rotation and mirroring, transparency, gradients, pens, kept text bodies and
//! style references, and one-cell anchors reach a peer; later changes to different shapes, and to
//! one shape's text body, frame and each of its other members, all reach it too (`KitDrawing 1.0`
//! DR14).

mod common;

use common::{Session, SharedBlobs, assert_replica_clean, assert_same, assert_workbook_valid, at, edit, first, peer, sync};
use kition_sheet_model::{
    Anchor, AnchorMode, Color, CustomGeom, CustomPath, Gradient, GradientStop, Outline, PathFill, PresetGeom, Shape, ShapeKind, Workbook,
};

fn shapes() -> Vec<Shape> {
    let anchor = |a1: &str| Anchor { cell: at(a1), dx: 2.0, dy: 3.0, width: 80.0, height: 40.0, mode: AnchorMode::MoveOnly, one_cell: true };
    vec![
        Shape {
            id: 10,
            kind: ShapeKind::TextBox,
            anchor: anchor("C3"),
            fill: Color::Theme(4, 400),
            line: Color::Rgb(0xB4C6E7),
            text: "Title".into(),
            groups: vec![1],
            fill_alpha: Some(35_000),
            outline: Outline { width: Some(9525), dash: Some("solid".into()), cap: Some("flat".into()), alpha: Some(80_000), ..Default::default() },
            text_body: Some(
                "<xdr:txBody><a:bodyPr anchor=\"ctr\"/><a:lstStyle/><a:p><a:r><a:rPr sz=\"1400\" b=\"1\"/><a:t>Title</a:t></a:r></a:p></xdr:txBody>"
                    .into(),
            ),
            ..Default::default()
        },
        Shape {
            id: 11,
            kind: ShapeKind::Rectangle,
            anchor: anchor("E3"),
            line: Color::Rgb(0),
            groups: vec![1, 2],
            rot: 5_400_000,
            flip_v: true,
            geom: Some(CustomGeom {
                paths: vec![CustomPath { w: 200.0, h: 100.0, d: "M 0 100 L 100 0 L 200 100 Z".into(), fill: PathFill::None, stroke: true }],
                source: Some("<a:custGeom><a:avLst/><a:gdLst><a:gd name=\"mid\" fmla=\"*/ w 1 2\"/></a:gdLst></a:custGeom>".into()),
            }),
            ..Default::default()
        },
        Shape {
            id: 12,
            kind: ShapeKind::Rectangle,
            anchor: anchor("E8"),
            fill: Color::Rgb(0x4472C4),
            preset: Some(PresetGeom { name: "star5".into(), adj: vec![("adj".into(), "val 19098".into())] }),
            gradient: Some(Gradient {
                stops: vec![
                    GradientStop { pos: 0, color: Color::Rgb(0x4472C4), alpha: None },
                    GradientStop { pos: 100_000, color: Color::Theme(1, 0), alpha: Some(0) },
                ],
                angle: 5_400_000,
                scaled: Some(true),
                ..Default::default()
            }),
            style_refs: Some("<xdr:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"/></a:lnRef></xdr:style>".into()),
            ..Default::default()
        },
    ]
}

fn assert_converged(a: &Session, b: &Session) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_same("views", &a.model().workbook, &b.model().workbook);
    assert_workbook_valid(&a.model().workbook);
    assert_replica_clean(a.replica());
}

#[test]
fn a_shapes_groups_geometry_transparency_and_text_body_reach_the_peer_and_later_changes_follow() {
    let blobs = SharedBlobs::default();
    let mut a = first(&Workbook::new(), &blobs);
    let mut b = peer(&a, 2, &blobs);
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes = shapes();
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(b.model().workbook.sheet(0).map(|s| s.shapes.clone()), Some(shapes()));

    // Two edits of the new fields, one after the other.
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].fill_alpha = Some(10_000);
            s.shapes[1].groups = vec![1];
        }
        vec![]
    });
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[2].gradient = None;
            s.shapes[0].outline.width = Some(28_575);
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let after = b.model().workbook.sheet(0).map(|s| s.shapes.clone()).unwrap_or_default();
    assert_eq!(after.len(), 3);
    // Changes to different shapes both stay, and so do changes to two members of one shape
    // (each its own entry of the shape's `look`).
    assert_eq!(after[1].groups, vec![1], "the change of shape 11 stays");
    assert!(after[2].gradient.is_none(), "the change of shape 12 stays");
    assert_eq!((after[0].fill_alpha, after[0].outline.width), (Some(10_000), Some(28_575)), "{:?}", after[0]);

    // An undo reaches the peer too.
    a.undo().unwrap_or_else(|e| panic!("undo: {e}"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
}

#[test]
fn an_art_text_warp_a_connector_end_and_a_move_all_stay() {
    let blobs = SharedBlobs::default();
    let mut wb = Workbook::new();
    if let Some(s) = wb.sheet_mut(0) {
        s.shapes = shapes();
    }
    let mut a = first(&wb, &blobs);
    let mut b = peer(&a, 2, &blobs);
    let warped = "<xdr:txBody><a:bodyPr anchor=\"ctr\"><a:prstTxWarp prst=\"textArchUp\"><a:avLst/></a:prstTxWarp></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr sz=\"1400\" b=\"1\"/><a:t>Title</a:t></a:r></a:p></xdr:txBody>";
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].text_body = Some(warped.into());
            s.shapes[1].connection =
                Some(kition_sheet_model::ShapeConnection { start: Some(kition_sheet_model::ConnectionEnd { id: 12, site: 1 }), end: None });
        }
        vec![]
    });
    edit(&mut a, &blobs, |wb| {
        if let Some(s) = wb.sheet_mut(0) {
            s.shapes[0].anchor.dx = 55.0;
            s.shapes[0].fill = Color::Rgb(0x112233);
            s.shapes[1].line = Color::Rgb(0x00AA00);
        }
        vec![]
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let after = b.model().workbook.sheet(0).map(|s| s.shapes.clone()).unwrap_or_default();
    // The warp (in the text body), then a move and fill of that shape; the glued end, then a pen
    // on the connector: each member is its own entry, so all stay.
    assert_eq!((after[0].text_body.as_deref(), after[0].anchor.dx, after[0].fill), (Some(warped), 55.0, Color::Rgb(0x112233)));
    assert_eq!((after[1].connection.as_ref().and_then(|c| c.start).map(|e| e.id), after[1].line), (Some(12), Color::Rgb(0x00AA00)));
}
