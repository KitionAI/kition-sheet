//! Drawing shapes on synthetic packages: groups (nested, with their child spaces), custom
//! geometry, transparency, gradients, pens, kept text bodies and style references, one-cell
//! anchors, written back and read again.

use std::io::Read;

use kition_sheet_core::CellRef;
use kition_sheet_model::{AnchorMode, Color, PathFill, Shape, ShapeKind, Sheet, Workbook};

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const NS: &str = r#"xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

/// A one-sheet package whose drawing holds `anchors`; the sheet's rows and columns are 20 × 64
/// pixels (15 points, the default width).
pub(super) fn with_drawing(anchors: &str) -> Vec<u8> {
    let drawing = format!(r#"<?xml version="1.0"?><xdr:wsDr {NS}>{anchors}</xdr:wsDr>"#);
    let srels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#;
    minimal(
        r#"<sheetFormatPr defaultRowHeight="15"/><sheetData/><drawing r:id="rId1"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", srels), ("xl/drawings/drawing1.xml", &drawing)],
        "",
        "",
    )
}

pub(super) fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn sheet(wb: &Workbook) -> &Sheet {
    wb.sheet(0).unwrap()
}

/// Whether two shapes are the same, their places within a hundredth of a pixel.
fn assert_same(x: &Shape, y: &Shape) {
    let close = |p: f32, q: f32| (p - q).abs() < 0.01;
    assert!(
        x.anchor.cell == y.anchor.cell
            && close(x.anchor.dx, y.anchor.dx)
            && close(x.anchor.dy, y.anchor.dy)
            && close(x.anchor.width, y.anchor.width)
            && close(x.anchor.height, y.anchor.height)
            && x.anchor.mode == y.anchor.mode
            && x.anchor.one_cell == y.anchor.one_cell,
        "{:?} became {:?}",
        x.anchor,
        y.anchor
    );
    assert_eq!(Shape { id: 0, anchor: y.anchor, ..x.clone() }, Shape { id: 0, ..y.clone() });
}

/// Reads, writes and reads again: the shapes must come back the same.
fn round_trip(bytes: &[u8]) -> (Workbook, Workbook, Vec<u8>) {
    let (first, _) = read_xlsx(bytes).unwrap();
    let saved = write_xlsx(&first).unwrap();
    let (second, _) = read_xlsx(&saved).unwrap();
    assert_eq!(sheet(&first).shapes.len(), sheet(&second).shapes.len());
    for (x, y) in sheet(&first).shapes.iter().zip(&sheet(&second).shapes) {
        assert_same(x, y);
    }
    (first, second, saved)
}

fn sp(id: u32, x: i64, y: i64, cx: i64, cy: i64, fill: &str) -> String {
    format!(
        r#"<xdr:sp><xdr:nvSpPr><xdr:cNvPr id="{id}" name="s{id}"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{fill}"/></a:solidFill></xdr:spPr></xdr:sp>"#
    )
}

const EMU: i64 = 9525;

#[test]
fn grouped_shapes_are_read_in_place_and_written_back_as_groups() {
    // A group at C3 of 200 × 100 px whose child space is twice as large, holding a shape and a
    // nested group, itself shifting its child space by (1000, 1000) px.
    let nested = format!(
        r#"<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id="5" name="inner"/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr><a:xfrm><a:off x="{}" y="0"/><a:ext cx="{}" cy="{}"/><a:chOff x="{}" y="{}"/><a:chExt cx="{}" cy="{}"/></a:xfrm></xdr:grpSpPr>{}{}</xdr:grpSp>"#,
        200 * EMU,
        200 * EMU,
        200 * EMU,
        1000 * EMU,
        1000 * EMU,
        200 * EMU,
        200 * EMU,
        sp(6, 1000 * EMU, 1000 * EMU, 100 * EMU, 100 * EMU, "00FF00"),
        sp(7, 1100 * EMU, 1100 * EMU, 100 * EMU, 100 * EMU, "0000FF"),
    );
    let group = format!(
        r#"<xdr:oneCellAnchor><xdr:from><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="{}" cy="{}"/><xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id="2" name="outer"/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/><a:chOff x="0" y="0"/><a:chExt cx="{}" cy="{}"/></a:xfrm></xdr:grpSpPr>{}{}</xdr:grpSp><xdr:clientData/></xdr:oneCellAnchor>"#,
        200 * EMU,
        100 * EMU,
        200 * EMU,
        100 * EMU,
        400 * EMU,
        200 * EMU,
        sp(3, 0, 0, 200 * EMU, 100 * EMU, "FF0000"),
        nested,
    );
    let (wb, second, saved) = round_trip(&with_drawing(&group));
    let s = sheet(&wb);
    assert_eq!(s.shapes.len(), 3);
    let (x0, y0) = (s.col_left(2), s.row_top(2));
    let px = |sh: &Shape| {
        (
            s.col_left(sh.anchor.cell.col) + f64::from(sh.anchor.dx) - x0,
            s.row_top(sh.anchor.cell.row) + f64::from(sh.anchor.dy) - y0,
            sh.anchor.width,
            sh.anchor.height,
        )
    };
    // Halved: the child space is 400 × 200 for a 200 × 100 group.
    let r = px(&s.shapes[0]);
    assert!(r.0.abs() < 1e-3 && r.1.abs() < 1e-3 && (r.2, r.3) == (100.0, 50.0), "{r:?}");
    // Group keys are object ids: one outer group, and a nested one inside it, none a shape's id.
    let outer = s.shapes[0].groups.clone();
    assert_eq!(outer.len(), 1);
    assert!(s.shapes.iter().flat_map(|sh| sh.groups.iter()).all(|k| s.shapes.iter().all(|sh| sh.id != *k)));
    // The nested group sits at 200 in the outer child space (100 px on the sheet) and maps its
    // own (1000, 1000)–(1200, 1200) onto 200 × 200 of the outer space (100 × 100 px).
    let a = px(&s.shapes[1]);
    assert!((a.0 - 100.0).abs() < 1e-3 && a.1.abs() < 1e-3 && (a.2 - 50.0).abs() < 1e-3, "{a:?}");
    let b = px(&s.shapes[2]);
    assert!((b.0 - 150.0).abs() < 1e-3 && (b.1 - 50.0).abs() < 1e-3, "{b:?}");
    assert_eq!(s.shapes[1].groups.len(), 2);
    assert_eq!(s.shapes[1].groups[0], outer[0]);
    assert_eq!(s.shapes[2].groups, s.shapes[1].groups);
    assert_ne!(s.shapes[1].groups[1], outer[0]);
    assert!(s.shapes.iter().all(|sh| sh.anchor.mode == AnchorMode::MoveOnly));
    // Written as one anchor holding the group and its nested group.
    let xml = part(&saved, "xl/drawings/drawing1.xml");
    assert_eq!(xml.matches("<xdr:grpSp>").count(), 2, "{xml}");
    assert_eq!(xml.matches("<xdr:oneCellAnchor>").count(), 1, "{xml}");
    let again = sheet(&second);
    assert_eq!(again.shapes[2].groups.len(), 2);
    assert_eq!(again.shapes[1].groups, again.shapes[2].groups);
    assert_eq!(again.shapes[0].groups, vec![again.shapes[1].groups[0]]);
}

#[test]
fn a_turned_and_mirrored_group_turns_its_members() {
    // A group turned a quarter and flipped: its member keeps its size and takes the turn.
    let group = format!(
        r#"<xdr:oneCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="{}" cy="{}"/><xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id="2" name="g"/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr><a:xfrm rot="5400000" flipH="1"><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/><a:chOff x="0" y="0"/><a:chExt cx="{}" cy="{}"/></a:xfrm></xdr:grpSpPr>{}</xdr:grpSp><xdr:clientData/></xdr:oneCellAnchor>"#,
        100 * EMU,
        100 * EMU,
        100 * EMU,
        100 * EMU,
        100 * EMU,
        100 * EMU,
        sp(3, 0, 0, 100 * EMU, 50 * EMU, "FF0000"),
    );
    let (wb, _, _) = round_trip(&with_drawing(&group));
    let sh = &sheet(&wb).shapes[0];
    assert_eq!((sh.anchor.width, sh.anchor.height), (100.0, 50.0));
    // Mirrored by the group: the member is mirrored too (up-down after the turn).
    assert!(sh.flip_v && !sh.flip_h);
    assert!(sh.rot == 5_400_000 || sh.rot == 16_200_000, "{}", sh.rot);
    assert!(sh.anchor.one_cell);
}

#[test]
fn custom_geometry_is_kept_with_its_guides_and_redrawn_when_edited() {
    let shape = r#"<xdr:oneCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="952500" cy="476250"/>
      <xdr:sp><xdr:nvSpPr><xdr:cNvPr id="2" name="path"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="952500" cy="476250"/></a:xfrm>
        <a:custGeom><a:avLst/><a:gdLst><a:gd name="mid" fmla="*/ w 1 2"/></a:gdLst><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="0" b="0"/>
          <a:pathLst><a:path w="200" h="100"><a:moveTo><a:pt x="0" y="100"/></a:moveTo><a:lnTo><a:pt x="mid" y="0"/></a:lnTo><a:lnTo><a:pt x="200" y="100"/></a:lnTo><a:close/></a:path>
          <a:path w="200" h="100" fill="none" stroke="0"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:cubicBezTo><a:pt x="50" y="0"/><a:pt x="150" y="100"/><a:pt x="200" y="100"/></a:cubicBezTo></a:path></a:pathLst></a:custGeom>
        <a:noFill/><a:ln w="9525"><a:solidFill><a:srgbClr val="B4C6E7"/></a:solidFill><a:miter lim="800000"/></a:ln></xdr:spPr></xdr:sp><xdr:clientData/></xdr:oneCellAnchor>"#;
    let (wb, _, saved) = round_trip(&with_drawing(shape));
    let sh = &sheet(&wb).shapes[0];
    let g = sh.geom.as_ref().unwrap();
    assert_eq!(g.paths.len(), 2);
    assert_eq!(g.paths[0].d, "M 0 100 L 100 0 L 200 100 Z");
    assert_eq!((g.paths[1].fill, g.paths[1].stroke), (PathFill::None, false));
    assert!(g.source.as_deref().is_some_and(|s| s.contains("fmla=\"*/ w 1 2\"")));
    assert_eq!((sh.outline.join.as_deref(), sh.outline.miter_limit, sh.outline.width), (Some("miter"), Some(800_000), Some(9525)));
    let xml = part(&saved, "xl/drawings/drawing1.xml");
    assert!(xml.contains(r#"<a:gd name="mid" fmla="*/ w 1 2"/>"#), "the guides are written back: {xml}");
    assert!(!xml.contains("prstGeom"), "{xml}");
    assert!(xml.contains(r#"<xdr:oneCellAnchor>"#));

    // Edited paths are written as plain paths (the guides no longer describe them).
    let mut wb = wb;
    wb.sheet_mut(0).unwrap().shapes[0].geom.as_mut().unwrap().paths[0].d = "M 0 0 L 200 0 L 200 100 Z".into();
    let saved = write_xlsx(&wb).unwrap();
    let xml = part(&saved, "xl/drawings/drawing1.xml");
    assert!(!xml.contains("fmla") && xml.contains(r#"<a:pt x="200" y="0"/>"#), "{xml}");
    let (back, _) = read_xlsx(&saved).unwrap();
    assert_eq!(sheet(&back).shapes[0].geom.as_ref().unwrap().paths[0].d, "M 0 0 L 200 0 L 200 100 Z");
}

#[test]
fn transparency_gradients_and_pens_are_kept() {
    let shape = r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
      <xdr:sp><xdr:nvSpPr><xdr:cNvPr id="2" name="a"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>
        <a:solidFill><a:schemeClr val="accent2"><a:lumMod val="60000"/><a:lumOff val="40000"/><a:alpha val="35000"/></a:schemeClr></a:solidFill>
        <a:ln w="28575" cap="rnd"><a:solidFill><a:srgbClr val="C00000"><a:alpha val="50000"/></a:srgbClr></a:solidFill><a:prstDash val="sysDash"/><a:round/><a:headEnd type="oval"/><a:tailEnd type="stealth"/></a:ln></xdr:spPr></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>
      <xdr:twoCellAnchor><xdr:from><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>6</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
      <xdr:sp><xdr:nvSpPr><xdr:cNvPr id="3" name="b"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
        <a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="4472C4"/></a:gs><a:gs pos="100000"><a:srgbClr val="4472C4"><a:alpha val="0"/></a:srgbClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="1"/></a:gradFill>
        </xdr:spPr></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>"#;
    let (wb, _, saved) = round_trip(&with_drawing(shape));
    let s = sheet(&wb);
    let a = &s.shapes[0];
    assert_eq!((a.kind, a.fill, a.fill_alpha), (ShapeKind::Ellipse, Color::Theme(5, 400), Some(35_000)));
    assert_eq!((a.line, a.outline.alpha, a.outline.width), (Color::Rgb(0xC00000), Some(50_000), Some(28_575)));
    assert_eq!((a.outline.cap.as_deref(), a.outline.dash.as_deref(), a.outline.join.as_deref()), (Some("rnd"), Some("sysDash"), Some("round")));
    assert_eq!((a.outline.head.as_deref(), a.outline.tail.as_deref()), (Some("oval"), Some("stealth")));
    let g = s.shapes[1].gradient.as_ref().unwrap();
    assert_eq!((g.angle, g.scaled, g.rot_with_shape, g.stops.len()), (5_400_000, Some(true), Some(true), 2));
    assert_eq!((g.stops[1].pos, g.stops[1].alpha), (100_000, Some(0)));
    assert_eq!(s.shapes[1].fill, Color::Rgb(0x4472C4), "readers without gradients see the first colour");
    // A theme colour stays a theme colour, its lighter variant as Office writes it.
    let xml = part(&saved, "xl/drawings/drawing1.xml");
    assert!(
        xml.contains(r#"<a:schemeClr val="accent2"><a:lumMod val="60000"/><a:lumOff val="40000"/><a:alpha val="35000"/></a:schemeClr>"#),
        "{xml}"
    );
}

#[test]
fn a_text_body_is_kept_while_its_text_is_and_lends_its_look_to_new_text() {
    let shape = r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
      <xdr:sp><xdr:nvSpPr><xdr:cNvPr id="2" name="t"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></xdr:spPr>
        <xdr:txBody><a:bodyPr vert="horz" lIns="0" tIns="0" rIns="0" bIns="0" anchor="ctr"/><a:lstStyle/>
          <a:p><a:pPr algn="ctr"><a:lnSpc><a:spcPct val="109000"/></a:lnSpc></a:pPr><a:r><a:rPr lang="zh-CN" sz="1400" b="1" spc="-10"><a:solidFill><a:srgbClr val="1F3864"><a:alpha val="100000"/></a:srgbClr></a:solidFill><a:latin typeface="Arial"/><a:ea typeface="PingFang SC"/><a:hlinkClick r:id="rId9"/></a:rPr><a:t>Title</a:t></a:r><a:r><a:rPr lang="en-US" sz="1000"/><a:t> and more</a:t></a:r></a:p>
          <a:p><a:endParaRPr sz="800"/></a:p></xdr:txBody></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>"#;
    let (wb, _, saved) = round_trip(&with_drawing(shape));
    let sh = &sheet(&wb).shapes[0];
    assert_eq!(sh.text, "Title and more\n");
    let body = sh.text_body.as_deref().unwrap();
    assert!(body.starts_with("<xdr:txBody><a:bodyPr vert=\"horz\""), "{body}");
    assert!(!body.contains("hlinkClick"), "relationships are not kept: {body}");
    let xml = part(&saved, "xl/drawings/drawing1.xml");
    assert!(xml.contains(r#"<a:latin typeface="Arial"/><a:ea typeface="PingFang SC"/>"#) && xml.contains(r#"<a:spcPct val="109000"/>"#), "{xml}");
    // The look the text is drawn with.
    let text = crate::shape_text(sh);
    assert_eq!(text.anchor, crate::TextPlace::Middle);
    assert_eq!(text.insets, [0.0; 4]);
    let title = &text.paras[0].runs[0];
    assert_eq!((title.size, title.bold, title.color, title.font.as_deref()), (14.0, true, Color::Rgb(0x1F3864), Some("Arial")));
    assert_eq!((text.paras[0].align, text.paras[0].line_multiple), (crate::TextPlace::Middle, 1.09));
    assert_eq!(text.paras[1].size, 8.0);

    // New text takes the box and the first run's look.
    let mut wb = wb;
    wb.sheet_mut(0).unwrap().shapes[0].text = "New".into();
    let saved = write_xlsx(&wb).unwrap();
    let xml = part(&saved, "xl/drawings/drawing1.xml");
    assert!(
        xml.contains(r#"anchor="ctr""#) && xml.contains(r#"sz="1400" b="1""#) && xml.contains("<a:t>New</a:t>") && !xml.contains("more"),
        "{xml}"
    );
    let (back, _) = read_xlsx(&saved).unwrap();
    assert_eq!(sheet(&back).shapes[0].text, "New");
}

#[test]
fn style_references_give_the_colours_and_are_written_back() {
    // Office's inserted shapes name their fill and pen by theme style reference only.
    let shape = r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>3</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
      <xdr:sp><xdr:nvSpPr><xdr:cNvPr id="2" name="s"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="star5"><a:avLst><a:gd name="adj" fmla="val 19098"/></a:avLst></a:prstGeom></xdr:spPr>
        <xdr:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></xdr:style>
      </xdr:sp><xdr:clientData/></xdr:twoCellAnchor>"#;
    let (wb, _, saved) = round_trip(&with_drawing(shape));
    let sh = &sheet(&wb).shapes[0];
    assert_eq!((sh.fill, sh.line), (Color::Theme(4, 0), Color::Theme(4, -500)));
    let p = sh.preset.as_ref().unwrap();
    assert_eq!((p.name.as_str(), p.adj.as_slice()), ("star5", &[("adj".to_string(), "val 19098".to_string())][..]));
    let xml = part(&saved, "xl/drawings/drawing1.xml");
    assert!(xml.contains(r#"<a:prstGeom prst="star5"><a:avLst><a:gd name="adj" fmla="val 19098"/></a:avLst></a:prstGeom>"#), "{xml}");
    assert!(xml.contains(r#"<xdr:style><a:lnRef idx="2">"#), "{xml}");
}

#[test]
fn shapes_made_in_kition_round_trip_unchanged() {
    let mut wb = Workbook::new();
    let s = wb.sheet_mut(0).unwrap();
    let at = |a: &str| CellRef::parse(a).unwrap();
    for (i, kind) in [ShapeKind::Rectangle, ShapeKind::Arrow, ShapeKind::TextBox, ShapeKind::Line].into_iter().enumerate() {
        s.shapes.push(Shape {
            id: i as u32 + 1,
            kind,
            anchor: kition_sheet_model::Anchor { cell: at("B2"), dx: 3.0, dy: 2.0, width: 90.0, height: 30.0, ..Default::default() },
            fill: if kind == ShapeKind::Rectangle { Color::Theme(4, 0) } else { Color::Auto },
            line: if kind == ShapeKind::Arrow { Color::Rgb(0) } else { Color::Theme(4, -250) },
            text: if kind == ShapeKind::TextBox { "a\n\nb".into() } else { String::new() },
            ..Default::default()
        });
    }
    let saved = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&saved).unwrap();
    assert_eq!(sheet(&back).shapes.len(), 4);
    for (a, b) in wb.sheet(0).unwrap().shapes.iter().zip(&sheet(&back).shapes) {
        assert_same(a, b);
    }
}

#[test]
fn shape_text_takes_its_colour_from_the_style_unless_a_run_sets_one() {
    // Excel's default shape: its style's font reference makes its text white (lt1).
    let shape = r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
      <xdr:sp><xdr:nvSpPr><xdr:cNvPr id="2" name="s"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr>
        <xdr:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></xdr:style>
        <xdr:txBody><a:bodyPr anchor="ctr"/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"/><a:t>White</a:t></a:r><a:r><a:rPr lang="en-US" sz="1100"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:rPr><a:t> red</a:t></a:r></a:p></xdr:txBody></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>"#;
    let (wb, _, _) = round_trip(&with_drawing(shape));
    let mut sh = sheet(&wb).shapes[0].clone();
    let runs = &crate::shape_text(&sh).paras[0].runs;
    assert_eq!((runs[0].color, runs[1].color), (Color::Theme(0, 0), Color::Rgb(0xFF0000)));
    // Plain text typed into it is white too.
    sh.text = "Typed".into();
    sh.text_body = None;
    assert_eq!(crate::shape_text(&sh).paras[0].runs[0].color, Color::Theme(0, 0));
}
