//! DrawingML spreadsheet drawings (`xl/drawings/drawingN.xml`): anchors, pictures, shapes and
//! chart frames.

use std::fmt::Write as _;

use kition_sheet_core::{CellRef, MAX_COLS, MAX_ROWS};
use kition_sheet_model::{
    Anchor, AnchorMode, Color, ConnectionEnd, CustomGeom, DiagramPart, Gradient, GradientStop, Image, Outline, PresetGeom, Shape, ShapeConnection,
    ShapeEffects, ShapeGlow, ShapeKind, ShapeShadow, Sheet, Theme,
};

use crate::IoError;
use crate::read::Ctx;
use crate::tables::EMU_PER_PX;
use crate::xml::{El, esc, esc_attr};

// ---------------------------------------------------------------- reading

fn emu_px(v: Option<i64>) -> f32 {
    v.map(|e| (e as f64 / EMU_PER_PX).clamp(-1e7, 1e7) as f32).unwrap_or(0.0)
}

fn marker(e: &El) -> (CellRef, f32, f32) {
    let num = |n: &str| e.child(n).and_then(|c| c.text.trim().parse::<i64>().ok());
    let col = num("col").unwrap_or(0).clamp(0, MAX_COLS as i64 - 1) as u32;
    let row = num("row").unwrap_or(0).clamp(0, MAX_ROWS as i64 - 1) as u32;
    (CellRef::new(row, col), emu_px(num("colOff")).max(0.0), emu_px(num("rowOff")).max(0.0))
}

fn ext_px(e: Option<&El>) -> (f32, f32) {
    match e {
        Some(x) => (emu_px(x.attr_i64("cx")).max(0.0), emu_px(x.attr_i64("cy")).max(0.0)),
        None => (0.0, 0.0),
    }
}

fn read_anchor(a: &El, sheet: &Sheet) -> Option<Anchor> {
    match a.name.as_str() {
        "twoCellAnchor" => {
            let (cell, dx, dy) = marker(a.child("from")?);
            let (to, tdx, tdy) = a.child("to").map(marker).unwrap_or((cell, dx, dy));
            let x1 = sheet.col_left(cell.col) + dx as f64;
            let y1 = sheet.row_top(cell.row) + dy as f64;
            let x2 = sheet.col_left(to.col) + tdx as f64;
            let y2 = sheet.row_top(to.row) + tdy as f64;
            // Excel stores "Move and size with cells" as a plain twoCellAnchor, "Move but don't
            // size" as editAs="oneCell", and "Don't move or size" as editAs="absolute".
            let mode = match a.attr("editAs") {
                Some("absolute") => AnchorMode::Absolute,
                Some("oneCell") => AnchorMode::MoveOnly,
                _ => AnchorMode::MoveAndSize,
            };
            Some(Anchor { cell, dx, dy, width: (x2 - x1).max(0.0) as f32, height: (y2 - y1).max(0.0) as f32, mode, one_cell: false })
        }
        "oneCellAnchor" => {
            let (cell, dx, dy) = marker(a.child("from")?);
            let (width, height) = ext_px(a.child("ext"));
            Some(Anchor { cell, dx, dy, width, height, mode: AnchorMode::MoveOnly, one_cell: true })
        }
        "absoluteAnchor" => {
            let pos = a.child("pos");
            let x = emu_px(pos.and_then(|p| p.attr_i64("x"))).max(0.0) as f64;
            let y = emu_px(pos.and_then(|p| p.attr_i64("y"))).max(0.0) as f64;
            let col = sheet.col_at(x);
            let row = sheet.row_at(y);
            let (width, height) = ext_px(a.child("ext"));
            Some(Anchor {
                cell: CellRef::new(row, col),
                dx: (x - sheet.col_left(col)).max(0.0) as f32,
                dy: (y - sheet.row_top(row)).max(0.0) as f32,
                width,
                height,
                mode: AnchorMode::Absolute,
                one_cell: false,
            })
        }
        _ => None,
    }
}

/// Colour of a DrawingML colour choice inside `e` (e.g. a `solidFill`).
pub fn dml_color(e: &El) -> Option<Color> {
    if let Some(c) = e.child("srgbClr").and_then(|c| c.attr("val")) {
        return Color::from_hex(c);
    }
    if let Some(c) = e.child("sysClr").and_then(|c| c.attr("lastClr")) {
        return Color::from_hex(c);
    }
    if let Some(c) = e.child("schemeClr").and_then(|c| c.attr("val")) {
        let i = match c {
            "bg1" | "lt1" => 0,
            "tx1" | "dk1" => 1,
            "bg2" | "lt2" => 2,
            "tx2" | "dk2" => 3,
            "accent1" => 4,
            "accent2" => 5,
            "accent3" => 6,
            "accent4" => 7,
            "accent5" => 8,
            "accent6" => 9,
            "hlink" => 10,
            "folHlink" => 11,
            _ => return None,
        };
        return Some(Color::Theme(i, 0));
    }
    None
}

fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// A 2-D affine map `x' = a x + c y + e`, `y' = b x + d y + f` (EMU of a group's child space to
/// sheet pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Affine([f64; 6]);

impl Affine {
    const ID: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    fn translate(x: f64, y: f64) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }
    fn scale(x: f64, y: f64) -> Affine {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }
    /// Clockwise by `rad` (y grows downwards).
    fn rotate(rad: f64) -> Affine {
        let (s, c) = rad.sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }
    /// `self` after `inner`.
    fn of(self, inner: Affine) -> Affine {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = inner.0;
        Affine([a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * e2 + c * f2 + e, b * e2 + d * f2 + f])
    }
    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
}

/// An object's `a:xfrm` (EMU; rotation in 60,000ths of a degree).
#[derive(Clone, Copy, Debug, Default)]
struct Xfrm {
    off: (f64, f64),
    ext: (f64, f64),
    ch_off: (f64, f64),
    ch_ext: (f64, f64),
    rot: i32,
    flip_h: bool,
    flip_v: bool,
}

fn read_xfrm(e: Option<&El>) -> Xfrm {
    let Some(e) = e else { return Xfrm::default() };
    let pair = |n: &str, x: &str, y: &str| {
        e.child(n).map(|c| (c.attr_f64(x).unwrap_or(0.0).clamp(-1e13, 1e13), c.attr_f64(y).unwrap_or(0.0).clamp(-1e13, 1e13))).unwrap_or((0.0, 0.0))
    };
    Xfrm {
        off: pair("off", "x", "y"),
        ext: pair("ext", "cx", "cy"),
        ch_off: pair("chOff", "x", "y"),
        ch_ext: pair("chExt", "cx", "cy"),
        rot: e.attr_i64("rot").unwrap_or(0).rem_euclid(21_600_000) as i32,
        flip_h: e.flag("flipH", false),
        flip_v: e.flag("flipV", false),
    }
}

/// The `a:xfrm` of a drawing object.
fn obj_xfrm(obj: &El) -> Option<&El> {
    match obj.name.as_str() {
        "graphicFrame" => obj.child("xfrm"),
        "grpSp" => obj.path(&["grpSpPr", "xfrm"]),
        _ => obj.path(&["spPr", "xfrm"]),
    }
}

/// Rotation (radians) and mirroring about the centre of the box `off` + `ext`.
fn turn(x: &Xfrm, off: (f64, f64), ext: (f64, f64)) -> Affine {
    let (cx, cy) = (off.0 + ext.0 / 2.0, off.1 + ext.1 / 2.0);
    let flip = Affine::scale(if x.flip_h { -1.0 } else { 1.0 }, if x.flip_v { -1.0 } else { 1.0 });
    let rot = Affine::rotate(f64::from(x.rot) / 60_000.0 * std::f64::consts::PI / 180.0);
    Affine::translate(cx, cy).of(rot).of(flip).of(Affine::translate(-cx, -cy))
}

/// The map from a group's child space into the space of `outer`, the group's own box being
/// `off` + `ext` there (the anchor's box, in pixels, for a group on the sheet).
fn group_map(outer: Affine, x: &Xfrm, off: (f64, f64), ext: (f64, f64), emu_ext: (f64, f64)) -> Affine {
    let ch = if x.ch_ext.0 > 0.0 && x.ch_ext.1 > 0.0 { x.ch_ext } else { emu_ext };
    let k = |e: f64, c: f64| if c > 0.0 { e / c } else { 1.0 / EMU_PER_PX };
    let fit = Affine::translate(off.0, off.1).of(Affine::scale(k(ext.0, ch.0), k(ext.1, ch.1))).of(Affine::translate(-x.ch_off.0, -x.ch_off.1));
    outer.of(turn(x, off, ext)).of(fit)
}

/// Where an object of a group lands: its box on the sheet (pixels), rotation and mirroring.
fn place_in(m: &Affine, x: &Xfrm) -> ((f64, f64, f64, f64), i32, bool, bool) {
    let (cx, cy) = m.apply(x.off.0 + x.ext.0 / 2.0, x.off.1 + x.ext.1 / 2.0);
    let [a, b, c, d, _, _] = m.0;
    let (su, sv) = ((a * a + b * b).sqrt(), (c * c + d * d).sqrt());
    let (w, h) = ((x.ext.0 * su).max(0.0), (x.ext.1 * sv).max(0.0));
    let phi = (b.atan2(a).to_degrees() * 60_000.0).round() as i64;
    let mirrored = a * d - b * c < 0.0;
    let rot = if mirrored { phi - i64::from(x.rot) } else { phi + i64::from(x.rot) };
    let rot = (rot.rem_euclid(21_600_000)) as i32;
    ((cx - w / 2.0, cy - h / 2.0, w, h), rot, x.flip_h, x.flip_v ^ mirrored)
}

/// An anchor for a box on the sheet (pixels), following its cells as `like` does.
fn anchor_at(sheet: &Sheet, r: (f64, f64, f64, f64), like: &Anchor) -> Anchor {
    let (x, y) = (r.0.clamp(0.0, 1e9), r.1.clamp(0.0, 1e9));
    // A box a rounding error above a line starts on that line.
    let col = sheet.col_at(x + 1e-3);
    let row = sheet.row_at(y + 1e-3);
    Anchor {
        cell: CellRef::new(row, col),
        dx: (x - sheet.col_left(col)).max(0.0) as f32,
        dy: (y - sheet.row_top(row)).max(0.0) as f32,
        width: r.2.clamp(0.0, 1e7) as f32,
        height: r.3.clamp(0.0, 1e7) as f32,
        mode: like.mode,
        one_cell: like.one_cell,
    }
}

fn anchor_box(sheet: &Sheet, a: &Anchor) -> (f64, f64, f64, f64) {
    (
        sheet.col_left(a.cell.col) + f64::from(a.dx),
        sheet.row_top(a.cell.row) + f64::from(a.dy),
        f64::from(a.width.max(0.0)),
        f64::from(a.height.max(0.0)),
    )
}

/// Where a drawing object goes: the anchor it sits in, or a group's map.
#[derive(Clone, Copy)]
enum Place {
    Anchor(Anchor),
    Group(Affine, Anchor),
}

/// What reading one drawing part keeps track of.
struct DrawingRead<'a> {
    rels: &'a [crate::package::Rel],
    /// Groups met so far on the sheet (their keys are object ids).
    groups: u32,
    /// Pictures' extension lists as written, by the picture's object id.
    blip_exts: std::collections::HashMap<String, String>,
    /// Shapes' drawing ids in the part (`cNvPr@id`) to their object ids, for connectors' ends.
    file_ids: std::collections::HashMap<u32, u32>,
}

pub fn read_drawing(cx: &mut Ctx<'_>, part: &str, sheet: &mut Sheet) -> Result<(), IoError> {
    let Some(x) = cx.optional_xml(part)? else { return Ok(()) };
    let rels = cx.pkg.rels(part)?;
    let groups = sheet.shapes.iter().flat_map(|s| s.groups.iter().copied()).max().unwrap_or(0);
    let mut blip_exts = std::collections::HashMap::new();
    if let Some(raw) = cx.pkg.read(part)? {
        for list in crate::xml::extension_lists(&raw) {
            if list.path.last().is_some_and(|n| n == "blip")
                && list.path.iter().any(|n| n == "pic")
                && let Some(id) = list.object
                && !list.exts.is_empty()
            {
                blip_exts.insert(id, list.exts.concat());
            }
        }
    }
    let mut dr = DrawingRead { rels: &rels, groups, blip_exts, file_ids: std::collections::HashMap::new() };
    let first = sheet.shapes.len();
    for a in &x.children {
        let Some(anchor) = read_anchor(a, sheet) else { continue };
        for obj in alternate_content(&a.children) {
            read_object(cx, &mut dr, sheet, obj, Place::Anchor(anchor), &[], 0)?;
        }
    }
    // Connectors' ends name drawing ids in the part: their shapes' object ids now (an end whose
    // shape was not read is left free).
    for sh in sheet.shapes.iter_mut().skip(first) {
        if let Some(c) = sh.connection.as_mut() {
            let map = |e: Option<ConnectionEnd>| e.and_then(|e| dr.file_ids.get(&e.id).map(|id| ConnectionEnd { id: *id, site: e.site }));
            c.start = map(c.start);
            c.end = map(c.end);
            if c.start.is_none() && c.end.is_none() {
                sh.connection = None;
            }
        }
    }
    Ok(())
}

/// The relationship type URI of a smart graphic part role.
fn diagram_rel_type(kind: &str) -> String {
    if kind == "diagramDrawing" {
        "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing".into()
    } else {
        format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}")
    }
}

/// The content type of a smart graphic part role.
pub(crate) fn diagram_content_type(role: &str) -> &'static str {
    match role {
        "data" => "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml",
        "layout" => "application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml",
        "quickStyle" => "application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml",
        "colors" => "application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml",
        _ => "application/vnd.ms-office.drawingml.diagramDrawing+xml",
    }
}

/// A smart graphic frame's parts (`dgm:relIds`: data, layout, quick style, colours) and the drawing
/// its data part names (`dsp:dataModelExt relId`), with the drawing's own parts (its pictures).
/// `None` without a data part or a drawing: nothing to draw.
fn read_diagram(cx: &mut Ctx<'_>, dr: &DrawingRead<'_>, gd: &El) -> Result<Option<DiagramRead>, IoError> {
    let Some(ids) = gd.child("relIds") else { return Ok(None) };
    let mut parts = Vec::new();
    let mut data = None;
    for (attr, role) in [("dm", "data"), ("lo", "layout"), ("qs", "quickStyle"), ("cs", "colors")] {
        let Some(rel) = ids.attr(attr).and_then(|id| dr.rels.iter().find(|r| r.id == id && !r.external)) else { continue };
        let Some(bytes) = cx.pkg.read(&rel.target)? else { continue };
        if role == "data" {
            data = Some(bytes.clone());
        }
        parts.push(DiagramPart {
            role: role.into(),
            rel_type: diagram_rel_type(&rel.kind),
            content_type: diagram_content_type(role).into(),
            ext: "xml".into(),
            data: bytes,
        });
    }
    // The drawing: the relationship the data part names.
    let Some(data) = data else { return Ok(None) };
    let drawing_id = kition_office_core::xml::parse(&String::from_utf8_lossy(&data))
        .ok()
        .and_then(|d| kition_office_core::xml::Xml::qfind(&d.root, "dsp:dataModelExt").and_then(|e| e.attr("relId").map(str::to_string)));
    let Some(rel) = drawing_id.and_then(|id| dr.rels.iter().find(|r| r.id == id && !r.external && r.kind == "diagramDrawing")) else {
        return Ok(None);
    };
    let target = rel.target.clone();
    let Some(bytes) = cx.pkg.read(&target)? else { return Ok(None) };
    parts.push(DiagramPart {
        role: "drawing".into(),
        rel_type: diagram_rel_type(&rel.kind),
        content_type: diagram_content_type("drawing").into(),
        ext: "xml".into(),
        data: bytes,
    });
    // The drawing's pictures, by the ids it names them by.
    let mut drawing_parts = Vec::new();
    for r in cx.pkg.rels(&target)?.into_iter().filter(|r| !r.external).take(1000) {
        if let Some(bytes) = cx.pkg.read(&r.target)? {
            let ext = r.target.rsplit('.').next().unwrap_or("bin").to_ascii_lowercase();
            drawing_parts.push(DiagramPart {
                role: r.id.clone(),
                rel_type: format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{}", r.kind),
                content_type: mime_for(&r.target).to_string(),
                ext,
                data: bytes,
            });
        }
    }
    Ok(Some((parts, drawing_parts)))
}

type DiagramRead = (Vec<DiagramPart>, Vec<DiagramPart>);

/// Writes a smart graphic's parts under `xl/diagrams/`, related from the sheet's drawing;
/// returns the frame's `dgm:relIds` attributes. The data part is pointed at the drawing's new id.
pub fn write_diagram(out: &mut crate::write::Out, drels: &mut crate::write::Rels, d: &kition_sheet_model::Diagram) -> Vec<(&'static str, String)> {
    out.diagrams += 1;
    let n = out.diagrams;
    let drawing = d.part("drawing").map(|p| {
        let name = format!("drawing{n}.xml");
        out.part(&format!("xl/diagrams/{name}"), Some(&p.content_type), p.data.clone());
        let mut rels = crate::write::Rels::default();
        for q in &d.drawing_parts {
            out.images += 1;
            let target = format!("media/image{}.{}", out.images, q.ext);
            out.default_type(&q.ext, &q.content_type);
            out.part(&format!("xl/{target}"), None, q.data.clone());
            rels.add_as(&q.role, &q.rel_type, &format!("../{target}"));
        }
        if !rels.is_empty() {
            out.part(&format!("xl/diagrams/_rels/{name}.rels"), None, rels.xml().into_bytes());
        }
        drels.add_uri(&p.rel_type, &format!("../diagrams/{name}"))
    });
    let mut ids = Vec::new();
    for (attr, role, file) in [("dm", "data", "data"), ("lo", "layout", "layout"), ("qs", "quickStyle", "quickStyle"), ("cs", "colors", "colors")] {
        let Some(p) = d.part(role) else { continue };
        let data = match (role, &drawing) {
            ("data", Some(rid)) => retarget_drawing(&p.data, rid),
            _ => p.data.clone(),
        };
        out.part(&format!("xl/diagrams/{file}{n}.xml"), Some(&p.content_type), data);
        ids.push((attr, drels.add_uri(&p.rel_type, &format!("../diagrams/{file}{n}.xml"))));
    }
    ids
}

/// A diagram data part with its `dsp:dataModelExt relId` set to `rid`.
fn retarget_drawing(data: &[u8], rid: &str) -> Vec<u8> {
    let s = String::from_utf8_lossy(data);
    let Some(ext) = s.find("dataModelExt") else { return data.to_vec() };
    let end = s[ext..].find('>').map_or(s.len(), |e| ext + e);
    let Some(at) = s[ext..end].find("relId=").map(|a| ext + a + "relId=".len()) else { return data.to_vec() };
    let Some(quote) = s[at..].chars().next().filter(|c| *c == '"' || *c == '\'') else { return data.to_vec() };
    let Some(close) = s[at + 1..end].find(quote).map(|c| at + 1 + c) else { return data.to_vec() };
    format!("{}{rid}{}", &s[..at + 1], &s[close..]).into_bytes()
}

/// Objects in markup-compatibility blocks: the first choice (chartex frames), else the fallback.
fn alternate_content(children: &[El]) -> Vec<&El> {
    children
        .iter()
        .flat_map(|o| match o.name.as_str() {
            "AlternateContent" => o
                .child("Choice")
                .filter(|c| c.child("graphicFrame").is_some())
                .or_else(|| o.child("Fallback"))
                .map(|c| c.children.iter().collect())
                .unwrap_or_default(),
            _ => vec![o],
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn read_object(
    cx: &mut Ctx<'_>,
    dr: &mut DrawingRead<'_>,
    sheet: &mut Sheet,
    obj: &El,
    place: Place,
    groups: &[u32],
    depth: usize,
) -> Result<(), IoError> {
    if !matches!(obj.name.as_str(), "pic" | "graphicFrame" | "sp" | "cxnSp" | "grpSp") {
        return Ok(());
    }
    let xf = read_xfrm(obj_xfrm(obj));
    // The object's box on the sheet, rotation and mirroring.
    let (anchor, rot, flip_h, flip_v) = match place {
        Place::Anchor(a) => (a, xf.rot, xf.flip_h, xf.flip_v),
        Place::Group(m, like) => {
            let (r, rot, fh, fv) = place_in(&m, &xf);
            (anchor_at(sheet, r, &like), rot, fh, fv)
        }
    };
    match obj.name.as_str() {
        "pic" => {
            let Some(rid) = obj.path(&["blipFill", "blip"]).and_then(|b| b.attr("embed")) else { return Ok(()) };
            let Some(rel) = dr.rels.iter().find(|r| r.id == rid && !r.external) else {
                cx.warn("a linked (not embedded) picture was skipped");
                return Ok(());
            };
            let target = rel.target.clone();
            let Some(data) = cx.pkg.read(&target)? else {
                cx.warn(format!("missing picture {target}"));
                return Ok(());
            };
            let alt = obj.path(&["nvPicPr", "cNvPr"]).and_then(|c| c.attr("descr")).unwrap_or("").to_string();
            let id = cx.object_id(obj.path(&["nvPicPr", "cNvPr"]).and_then(|c| c.attr_u32("id")));
            let blip_ext = obj.path(&["nvPicPr", "cNvPr"]).and_then(|c| c.attr("id")).and_then(|id| dr.blip_exts.get(id)).cloned();
            sheet.images.push(Image { id, anchor, data, mime: mime_for(&target).to_string(), alt, blip_ext });
        }
        "graphicFrame" => {
            let Some(gd) = obj.path(&["graphic", "graphicData"]) else { return Ok(()) };
            // The frame's own id (`cNvPr id`), kept when it is free.
            let frame_id = obj.path(&["nvGraphicFramePr", "cNvPr"]).and_then(|c| c.attr_u32("id"));
            let uri = gd.attr("uri").unwrap_or("");
            if let Some(rid) = gd.child("chart").and_then(|c| c.attr("id")) {
                let Some(rel) = dr.rels.iter().find(|r| r.id == rid) else { return Ok(()) };
                let target = rel.target.clone();
                if let Some(cx_xml) = cx.optional_xml(&target)? {
                    let chartex = uri.contains("chartex");
                    match if chartex { crate::chartex::read_chartex(&cx_xml) } else { crate::chart::read_chart(&cx_xml) } {
                        Some(mut ch) => {
                            ch.id = cx.object_id(frame_id);
                            ch.anchor = anchor;
                            sheet.charts.push(ch);
                        }
                        None => cx.warn(format!("chart {target} has no supported plot and was skipped")),
                    }
                }
            } else if uri.contains("chartex") || uri.contains("chartEx") {
                cx.warn("a histogram, waterfall, treemap… chart without its chart part was skipped");
            } else if uri.ends_with("/diagram") && matches!(place, Place::Anchor(_)) {
                match read_diagram(cx, dr, gd)? {
                    Some((parts, drawing_parts)) => {
                        let name = obj.path(&["nvGraphicFramePr", "cNvPr"]).and_then(|c| c.attr("name")).unwrap_or("").to_string();
                        let id = cx.object_id(frame_id);
                        sheet.diagrams.push(kition_sheet_model::Diagram { id, anchor, name, parts, drawing_parts });
                    }
                    None => cx.warn("a smart graphic without its drawing was skipped"),
                }
            } else {
                cx.warn("an embedded object (OLE…) was skipped");
            }
        }
        "sp" | "cxnSp" => {
            if let Some(mut s) = read_shape(obj, anchor) {
                let nv = obj.child("nvSpPr").or_else(|| obj.child("nvCxnSpPr"));
                let file_id = nv.and_then(|n| n.child("cNvPr")).and_then(|c| c.attr_u32("id"));
                s.id = cx.object_id(file_id);
                if let Some(file_id) = file_id {
                    dr.file_ids.insert(file_id, s.id);
                }
                // A connector's ends, by drawing id until the whole part is read.
                let ends = nv.and_then(|n| n.child("cNvCxnSpPr"));
                let end = |name: &str| {
                    ends.and_then(|c| c.child(name)).and_then(|e| Some(ConnectionEnd { id: e.attr_u32("id")?, site: e.attr_u32("idx").unwrap_or(0) }))
                };
                let (start, stop) = (end("stCxn"), end("endCxn"));
                if start.is_some() || stop.is_some() {
                    s.connection = Some(ShapeConnection { start, end: stop });
                }
                s.groups = groups.to_vec();
                s.rot = rot;
                s.flip_h = flip_h;
                s.flip_v = flip_v;
                sheet.shapes.push(s);
            }
        }
        "grpSp" => {
            if depth >= 32 {
                cx.warn("groups nested too deeply were skipped");
                return Ok(());
            }
            let m = match place {
                Place::Anchor(a) => {
                    let r = anchor_box(sheet, &a);
                    group_map(Affine::ID, &xf, (r.0, r.1), (r.2, r.3), xf.ext)
                }
                Place::Group(m, _) => group_map(m, &xf, xf.off, xf.ext, xf.ext),
            };
            let like = match place {
                Place::Anchor(a) | Place::Group(_, a) => a,
            };
            // A group's key is an object id, as a shape's is: unique on the workbook, so the
            // group can be named (`KitDrawing` groups).
            dr.groups = dr.groups.saturating_add(1);
            let mut path = groups.to_vec();
            path.push(cx.object_id(obj.path(&["nvGrpSpPr", "cNvPr"]).and_then(|c| c.attr_u32("id"))));
            for child in alternate_content(&obj.children) {
                read_object(cx, dr, sheet, child, Place::Group(m, like), &path, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// A DrawingML colour inside `e` (a `solidFill`, a gradient stop, a style reference) with its
/// opacity: `srgbClr`, `sysClr` (its last colour), `schemeClr` with the luminance changes
/// Office's lighter and darker variants write (`lumMod`, `lumOff`; `tint`, `shade` as near as a
/// tint goes).
pub fn dml_color_alpha(e: &El) -> Option<(Color, Option<u32>)> {
    let c = e.children.iter().find(|c| matches!(c.name.as_str(), "srgbClr" | "sysClr" | "schemeClr"))?;
    let alpha = c.child("alpha").and_then(|a| a.attr_u32("val")).map(|v| v.min(100_000));
    let color = match c.name.as_str() {
        "srgbClr" => Color::from_hex(c.attr("val")?)?,
        "sysClr" => Color::from_hex(c.attr("lastClr")?)?,
        _ => {
            let Some(Color::Theme(i, _)) = dml_color(e) else { return None };
            let val = |n: &str| c.child(n).and_then(|x| x.attr_i64("val")).map(|v| v.clamp(0, 100_000));
            let tint = match (val("lumMod"), val("lumOff"), val("tint"), val("shade")) {
                (_, Some(off), _, _) if off > 0 => off / 100,
                (Some(m), _, _, _) if m < 100_000 => -(100_000 - m) / 100,
                (_, _, Some(t), _) if t < 100_000 => (100_000 - t) / 100,
                (_, _, _, Some(s)) if s < 100_000 => -(100_000 - s) / 100,
                _ => 0,
            };
            Color::Theme(i, tint.clamp(-1000, 1000) as i16)
        }
    };
    Some((color, alpha))
}

fn read_gradient(g: &El) -> Gradient {
    let stops = g
        .child("gsLst")
        .map(|l| {
            l.kids("gs")
                .take(64)
                .filter_map(|gs| {
                    let (color, alpha) = dml_color_alpha(gs)?;
                    Some(GradientStop { pos: gs.attr_u32("pos").unwrap_or(0).min(100_000), color, alpha })
                })
                .collect()
        })
        .unwrap_or_default();
    let lin = g.child("lin");
    Gradient {
        stops,
        angle: lin.and_then(|l| l.attr_i64("ang")).unwrap_or(0).rem_euclid(21_600_000) as i32,
        scaled: lin.and_then(|l| l.attr_bool("scaled")),
        path: g.child("path").and_then(|p| p.attr("path")).map(str::to_string),
        rot_with_shape: g.attr_bool("rotWithShape"),
    }
}

const FILLS: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

/// The plain text of a text body: its paragraphs' runs, one line per paragraph.
fn body_text(tb: &El) -> String {
    tb.kids("p").map(|p| p.kids("r").filter_map(|r| r.child("t")).map(|t| t.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// The preset outline the writer gives each kind.
fn kind_preset(kind: ShapeKind) -> &'static str {
    match kind {
        ShapeKind::RoundedRectangle => "roundRect",
        ShapeKind::Ellipse => "ellipse",
        ShapeKind::Triangle => "triangle",
        ShapeKind::Line | ShapeKind::Arrow => "line",
        _ => "rect",
    }
}

fn read_shape(obj: &El, anchor: Anchor) -> Option<Shape> {
    let sp = obj.child("spPr")?;
    let prst_el = sp.child("prstGeom");
    let prst = prst_el.and_then(|g| g.attr("prst")).unwrap_or("rect");
    let tx_box = obj.path(&["nvSpPr", "cNvSpPr"]).is_some_and(|c| c.flag("txBox", false));
    let ln = sp.child("ln");
    let end = |n: &str| ln.and_then(|l| l.child(n)).and_then(|t| t.attr("type")).map(str::to_string);
    let (head, tail) = (end("headEnd"), end("tailEnd"));
    let arrow = [&head, &tail].iter().any(|t| t.as_deref().is_some_and(|v| v != "none"));
    let kind = match prst {
        _ if tx_box => ShapeKind::TextBox,
        "roundRect" => ShapeKind::RoundedRectangle,
        "ellipse" => ShapeKind::Ellipse,
        "triangle" => ShapeKind::Triangle,
        "line" | "straightConnector1" if arrow => ShapeKind::Arrow,
        "line" | "straightConnector1" => ShapeKind::Line,
        "rightArrow" => ShapeKind::Arrow,
        _ => ShapeKind::Rectangle,
    };
    let style = obj.child("style");
    let style_color = |n: &str| style.and_then(|s| s.child(n)).filter(|r| r.attr_u32("idx").unwrap_or(0) > 0).and_then(dml_color_alpha);
    // Fill: the shape's own, else its style's (Office's inserted shapes write only a reference).
    let (mut fill, mut fill_alpha, mut gradient) = (Color::Auto, None, None);
    if let Some(f) = sp.child("solidFill") {
        if let Some((c, a)) = dml_color_alpha(f) {
            (fill, fill_alpha) = (c, a);
        }
    } else if let Some(g) = sp.child("gradFill") {
        let gr = read_gradient(g);
        fill = gr.stops.first().map(|s| s.color).unwrap_or_default();
        gradient = Some(gr);
    } else if !FILLS.iter().any(|n| sp.child(n).is_some())
        && let Some((c, a)) = style_color("fillRef")
    {
        (fill, fill_alpha) = (c, a);
    }
    let mut outline = Outline::default();
    let mut line = Color::Auto;
    if let Some(l) = ln {
        if let Some((c, a)) = l.child("solidFill").and_then(dml_color_alpha) {
            (line, outline.alpha) = (c, a);
        } else if !FILLS.iter().any(|n| l.child(n).is_some())
            && let Some((c, a)) = style_color("lnRef")
        {
            (line, outline.alpha) = (c, a);
        }
        outline.width = l.attr_u32("w");
        outline.cap = l.attr("cap").map(str::to_string);
        outline.dash = l.child("prstDash").and_then(|d| d.attr("val")).map(str::to_string);
        if l.child("round").is_some() {
            outline.join = Some("round".into());
        } else if l.child("bevel").is_some() {
            outline.join = Some("bevel".into());
        } else if let Some(m) = l.child("miter") {
            outline.join = Some("miter".into());
            outline.miter_limit = m.attr_u32("lim");
        }
        // The tail the writer gives an arrow is not kept twice.
        let line_arrow = kind == ShapeKind::Arrow && matches!(prst, "line" | "straightConnector1") && sp.child("custGeom").is_none();
        if !(line_arrow && head.is_none() && tail.as_deref() == Some("triangle")) {
            outline.head = head;
            outline.tail = tail;
        }
    } else if let Some((c, a)) = style_color("lnRef") {
        (line, outline.alpha) = (c, a);
    }
    // The outline the shape's kind is drawn with, else the file's preset or custom geometry.
    let adj: Vec<(String, String)> = prst_el
        .and_then(|g| g.child("avLst"))
        .map(|l| l.kids("gd").take(64).filter_map(|gd| Some((gd.attr("name")?.to_string(), gd.attr("fmla")?.to_string()))).collect())
        .unwrap_or_default();
    let preset = (prst_el.is_some() && (prst != kind_preset(kind) || !adj.is_empty())).then(|| PresetGeom { name: prst.to_string(), adj });
    let (w_emu, h_emu) = (f64::from(anchor.width) * EMU_PER_PX, f64::from(anchor.height) * EMU_PER_PX);
    let geom = sp.child("custGeom").map(|cg| CustomGeom {
        paths: crate::custgeom::read_cust_geom(cg, w_emu, h_emu),
        source: crate::custgeom::has_more_than_paths(cg).then(|| crate::xml::dml_xml(cg, "a:custGeom")),
    });
    // Shadow and glow (EMU lengths, 60,000ths of a degree).
    let emu_pt = |v: Option<i64>| v.map_or(0.0, |e| (e.clamp(0, 50_000_000) as f32) / 12_700.0);
    let effects = sp.child("effectLst").map(|fx| ShapeEffects {
        shadow: fx.child("outerShdw").and_then(|o| {
            let (color, alpha) = dml_color_alpha(o)?;
            Some(ShapeShadow {
                color,
                alpha,
                blur: emu_pt(o.attr_i64("blurRad")),
                distance: emu_pt(o.attr_i64("dist")),
                angle: o.attr_i64("dir").map_or(0.0, |d| (d as f32 / 60_000.0).rem_euclid(360.0)),
            })
        }),
        glow: fx.child("glow").and_then(|g| {
            let (color, alpha) = dml_color_alpha(g)?;
            Some(ShapeGlow { color, alpha, size: emu_pt(g.attr_i64("rad")) })
        }),
    });
    let effects = effects.filter(|e| e.shadow.is_some() || e.glow.is_some());
    let tb = obj.child("txBody");
    let text = tb.map(body_text).unwrap_or_default();
    // The body as written, unless it is what the writer makes of the plain text.
    let text_body = tb.map(|tb| crate::xml::dml_xml(tb, "xdr:txBody")).filter(|src| *src != plain_body(&text, None));
    Some(Shape {
        id: 0,
        kind,
        anchor,
        fill,
        line,
        text,
        preset,
        geom,
        fill_alpha,
        gradient,
        outline,
        effects,
        text_body,
        style_refs: style.map(|s| crate::xml::dml_xml(s, "xdr:style")),
        ..Default::default()
    })
    .map(|sh| as_ink(obj, sh))
}

/// An ink stroke's points (shape-local, in the anchor's units), as its text holds them.
fn ink_points(text: &str) -> Vec<[f64; 2]> {
    text.split(';')
        .take(100_000)
        .filter_map(|p| {
            let (x, y) = p.split_once(',')?;
            let (x, y) = (x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?);
            (x.is_finite() && y.is_finite()).then_some([x, y])
        })
        .collect()
}

/// The prefix of the name an ink stroke is written under, which reading takes it back by.
const INK_NAME: &str = "Ink ";

/// A shape read as an ink stroke when it is one Kition wrote: named [`INK_NAME`]…, without text,
/// and drawn by one open custom path. Its points go back into its text, in the anchor's units.
fn as_ink(obj: &El, mut sh: Shape) -> Shape {
    let named = obj.path(&["nvSpPr", "cNvPr"]).and_then(|c| c.attr("name")).is_some_and(|n| n.starts_with(INK_NAME));
    let path = match &sh.geom {
        Some(g) if named && sh.text.is_empty() && g.paths.len() == 1 => g.paths.first().cloned(),
        _ => None,
    };
    let Some(p) = path.filter(|p| !p.d.contains('Z') && !p.d.contains('z')) else { return sh };
    let (sx, sy) = (if p.w > 0.0 { f64::from(sh.anchor.width) / p.w } else { 1.0 }, if p.h > 0.0 { f64::from(sh.anchor.height) / p.h } else { 1.0 });
    let nums: Vec<f64> = p.d.split(|c: char| c.is_whitespace() || c == ',').filter_map(|t| t.parse::<f64>().ok()).collect();
    let text = nums.as_chunks::<2>().0.iter().map(|[x, y]| format!("{:.1},{:.1}", x * sx, y * sy)).collect::<Vec<_>>().join(";");
    sh.kind = ShapeKind::Ink;
    sh.text = text;
    sh.geom = None;
    sh.preset = None;
    sh.text_body = None;
    // Older readers keep a rounded width in the fill's red channel.
    let w = sh.outline.width.map_or(2.0, |e| f64::from(e) / 12_700.0);
    sh.fill = Color::Rgb((w.round().clamp(1.0, 255.0) as u32) << 16);
    sh.anchor.mode = AnchorMode::Absolute;
    sh
}

/// An ink stroke: an open custom path through its points, its pen's colour, width and opacity on
/// the outline, no fill and no text (Excel shows it as drawn; Kition reads it back as ink).
fn write_ink(s: &mut String, sh: &Shape, theme: &Theme, (id, n): (u32, u32), bx: (f64, f64, f64, f64)) {
    let (w_emu, h_emu) = (emu(bx.2.max(0.0)), emu(bx.3.max(0.0)));
    let _ = write!(
        s,
        "<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{INK_NAME}{}\"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{w_emu}\" cy=\"{h_emu}\"/></a:xfrm>",
        n,
        emu(bx.0),
        emu(bx.1),
    );
    // Hundredths of the anchor's unit: the path's own coordinate space.
    let (pw, ph) = ((f64::from(sh.anchor.width) * 100.0).round().max(1.0), (f64::from(sh.anchor.height) * 100.0).round().max(1.0));
    let _ = write!(
        s,
        "<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"l\" t=\"t\" r=\"r\" b=\"b\"/><a:pathLst><a:path w=\"{pw}\" h=\"{ph}\" fill=\"none\">"
    );
    for (i, [x, y]) in ink_points(&sh.text).into_iter().enumerate() {
        let tag = if i == 0 { "a:moveTo" } else { "a:lnTo" };
        let _ = write!(s, "<{tag}><a:pt x=\"{}\" y=\"{}\"/></{tag}>", (x * 100.0).round() as i64, (y * 100.0).round() as i64);
    }
    s.push_str("</a:path></a:pathLst></a:custGeom><a:noFill/>");
    let width = sh.outline.width.unwrap_or_else(|| match sh.fill {
        Color::Rgb(v) => ((v >> 16) & 0xFF).max(1) * 12_700,
        _ => 25_400,
    });
    let _ = write!(s, "<a:ln w=\"{width}\" cap=\"rnd\">");
    if let Some(c) = dml_color_xml(&sh.line, sh.outline.alpha, theme) {
        let _ = write!(s, "<a:solidFill>{c}</a:solidFill>");
    }
    s.push_str("<a:round/></a:ln></xdr:spPr></xdr:sp>");
}

// ---------------------------------------------------------------- writing

/// The opening `<xdr:twoCellAnchor>` for a model anchor, carrying Excel's `editAs` when the mode
/// is not the default: omitted for "Move and size with cells", `editAs="oneCell"` for "Move but
/// don't size", `editAs="absolute"` for "Don't move or size". (Excel writes pictures this way;
/// an anchor read from a `oneCellAnchor` is written as one while it does not size with cells;
/// `absoluteAnchor` is read on import but not written.)
/// Returns the opening tag and the matching closing tag.
fn anchor_open(sheet: &Sheet, a: &Anchor) -> (String, &'static str) {
    let px = |v: f64| ((v.max(0.0)) * EMU_PER_PX).round() as i64;
    let x1 = sheet.col_left(a.cell.col) + a.dx as f64;
    let y1 = sheet.row_top(a.cell.row) + a.dy as f64;
    let x2 = x1 + a.width.max(0.0) as f64;
    let y2 = y1 + a.height.max(0.0) as f64;
    let tc = sheet.col_at(x2);
    let tr = sheet.row_at(y2);
    let tdx = x2 - sheet.col_left(tc);
    let tdy = y2 - sheet.row_top(tr);
    if a.one_cell && a.mode == AnchorMode::MoveOnly {
        let open = format!(
            "<xdr:oneCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from><xdr:ext cx=\"{}\" cy=\"{}\"/>",
            a.cell.col,
            px(a.dx as f64),
            a.cell.row,
            px(a.dy as f64),
            px(a.width as f64),
            px(a.height as f64)
        );
        return (open, "</xdr:oneCellAnchor>");
    }
    let edit = match a.mode {
        AnchorMode::MoveAndSize => "",
        AnchorMode::MoveOnly => " editAs=\"oneCell\"",
        AnchorMode::Absolute => " editAs=\"absolute\"",
    };
    let open = format!(
        "<xdr:twoCellAnchor{edit}><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from><xdr:to><xdr:col>{tc}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{tr}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:to>",
        a.cell.col,
        px(a.dx as f64),
        a.cell.row,
        px(a.dy as f64),
        px(tdx),
        px(tdy)
    );
    (open, "</xdr:twoCellAnchor>")
}

pub(crate) fn file_anchor(sheet: &Sheet, anchor: &Anchor) -> Anchor {
    let quantize = |value: f64| emu_px(Some((value.max(0.0) * EMU_PER_PX).round() as i64)).max(0.0);
    let mut result = *anchor;
    result.dx = quantize(f64::from(anchor.dx));
    result.dy = quantize(f64::from(anchor.dy));
    if anchor.one_cell && anchor.mode == AnchorMode::MoveOnly {
        result.width = quantize(f64::from(anchor.width));
        result.height = quantize(f64::from(anchor.height));
    } else {
        let x = sheet.col_left(anchor.cell.col) + f64::from(anchor.dx) + f64::from(anchor.width.max(0.0));
        let y = sheet.row_top(anchor.cell.row) + f64::from(anchor.dy) + f64::from(anchor.height.max(0.0));
        let col = sheet.col_at(x);
        let row = sheet.row_at(y);
        result.width = (sheet.col_left(col) + f64::from(quantize(x - sheet.col_left(col))) - sheet.col_left(anchor.cell.col) - f64::from(result.dx))
            .max(0.0) as f32;
        result.height = (sheet.row_top(row) + f64::from(quantize(y - sheet.row_top(row))) - sheet.row_top(anchor.cell.row) - f64::from(result.dy))
            .max(0.0) as f32;
    }
    result
}

fn xfrm(a: &Anchor, tag: &str) -> String {
    let e = |v: f32| ((v.max(0.0) as f64) * EMU_PER_PX).round() as i64;
    format!("<{tag}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></{tag}>", e(a.width), e(a.height))
}

pub fn srgb(c: &Color, theme: &Theme) -> Option<String> {
    let [r, g, b] = c.resolve(theme)?;
    Some(format!("<a:srgbClr val=\"{r:02X}{g:02X}{b:02X}\"/>"))
}

/// One drawing object to write.
pub enum Obj<'a> {
    Image(&'a Image, String),
    Chart(&'a kition_sheet_model::Chart, String),
    Shape(&'a Shape),
    /// A smart graphic frame with its `dgm:relIds` attributes.
    Diagram(&'a kition_sheet_model::Diagram, Vec<(&'static str, String)>),
}

const EMU_PER_PX_F: f64 = EMU_PER_PX;

fn emu(v: f64) -> i64 {
    (v * EMU_PER_PX_F).round().clamp(-1e13, 1e13) as i64
}

/// The box (pixels) around shapes on the sheet.
fn bounds(sheet: &Sheet, shapes: &[&Shape]) -> (f64, f64, f64, f64) {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for sh in shapes {
        let (x, y, w, h) = anchor_box(sheet, &sh.anchor);
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + w), y1.max(y + h));
    }
    if x0.is_finite() { (x0, y0, x1 - x0, y1 - y0) } else { (0.0, 0.0, 0.0, 0.0) }
}

/// A group (`xdr:grpSp`) of the members at nesting `depth`, its child space the sheet's (EMU), so
/// each member keeps its place; members of a group inside it are written as that group.
fn write_group(s: &mut String, sheet: &Sheet, theme: &Theme, members: &[&Shape], depth: usize, next: &mut u32, ids: &mut PartIds) {
    *next += 1;
    // The group's key is its id (unique in the workbook, as a shape's is).
    let id = ids.take(members.first().and_then(|m| m.groups.get(depth)).copied().unwrap_or(0));
    let (x, y, w, h) = bounds(sheet, members);
    let _ = write!(
        s,
        "<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id=\"{}\" name=\"Group {}\"/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/><a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm></xdr:grpSpPr>",
        id,
        *next - 1,
        emu(x),
        emu(y),
        emu(w),
        emu(h),
        emu(x),
        emu(y),
        emu(w),
        emu(h)
    );
    let mut inner = std::collections::HashSet::new();
    for sh in members {
        match sh.groups.get(depth + 1) {
            Some(&key) if depth < 32 => {
                if inner.insert(key) {
                    let sub: Vec<&Shape> = members.iter().copied().filter(|m| m.groups.get(depth + 1) == Some(&key)).collect();
                    write_group(s, sheet, theme, &sub, depth + 1, next, ids);
                }
            }
            _ => {
                *next += 1;
                write_shape(s, sh, theme, (ids.take(sh.id), *next - 1), anchor_box(sheet, &sh.anchor));
            }
        }
    }
    s.push_str("</xdr:grpSp>");
}

/// The scheme colour names a theme index is written with.
const SCHEME_NAMES: [&str; 12] = ["bg1", "tx1", "bg2", "tx2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];

/// A DrawingML colour element for a model colour: theme colours stay theme colours (their tint as
/// the luminance change Office writes), with the opacity when one is given. `None` for automatic.
fn dml_color_xml(c: &Color, alpha: Option<u32>, theme: &Theme) -> Option<String> {
    let alpha = alpha.map(|a| format!("<a:alpha val=\"{}\"/>", a.min(100_000))).unwrap_or_default();
    match *c {
        Color::Auto => None,
        Color::Theme(i, tint) if usize::from(i) < SCHEME_NAMES.len() => {
            let t = i32::from(tint.clamp(-1000, 1000)) * 100;
            let lum = match t {
                t if t > 0 => format!("<a:lumMod val=\"{}\"/><a:lumOff val=\"{t}\"/>", 100_000 - t),
                t if t < 0 => format!("<a:lumMod val=\"{}\"/>", 100_000 + t),
                _ => String::new(),
            };
            Some(format!("<a:schemeClr val=\"{}\">{lum}{alpha}</a:schemeClr>", SCHEME_NAMES[usize::from(i)]))
        }
        _ => {
            let [r, g, b] = c.resolve(theme)?;
            Some(if alpha.is_empty() {
                format!("<a:srgbClr val=\"{r:02X}{g:02X}{b:02X}\"/>")
            } else {
                format!("<a:srgbClr val=\"{r:02X}{g:02X}{b:02X}\">{alpha}</a:srgbClr>")
            })
        }
    }
}

/// The text body the writer makes of plain text, one paragraph per line; `like` is the file's
/// body whose box and first paragraph's and run's formatting the new text takes.
/// The text body `src` (`xdr:txBody`) holding `text` instead of its own text, keeping every
/// paragraph's and run's look: the characters the two texts share at their start and end keep
/// their runs, and the characters typed between them take the look of the character before them
/// (or after them, at the start), as an edit in Excel does. `None` when `src` does not read.
pub fn retext_body(src: &str, text: &str) -> Option<String> {
    let tb = crate::xml::parse(src.as_bytes()).ok()?;
    // Each paragraph's pPr and end-of-paragraph look, and each run's look, by index.
    let paras: Vec<&El> = tb.kids("p").collect();
    let mut looks: Vec<String> = Vec::new();
    // The old text's characters, each with its paragraph and run look; a paragraph's end is '\n'
    // with no run.
    let mut chars: Vec<(char, usize, Option<usize>)> = Vec::new();
    for (pi, p) in paras.iter().enumerate() {
        if pi > 0 {
            chars.push(('\n', pi - 1, None));
        }
        for r in p.kids("r") {
            let look = r.child("rPr").map(|e| crate::xml::dml_xml(e, "a:rPr")).unwrap_or_default();
            looks.push(look);
            let at = looks.len() - 1;
            chars.extend(r.child("t").map(|t| t.text.as_str()).unwrap_or_default().chars().map(|c| (c, pi, Some(at))));
        }
    }
    let old: Vec<char> = chars.iter().map(|c| c.0).collect();
    let new: Vec<char> = text.chars().collect();
    let start = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let end = old.iter().rev().zip(new.iter().rev()).take(old.len().min(new.len()) - start).take_while(|(a, b)| a == b).count();
    // The look typed characters take: the character before the edit's, else the one after.
    let typed = chars[..start].iter().rev().chain(chars[old.len() - end..].iter()).find(|c| c.2.is_some()).map(|c| (c.1, c.2));
    let mut out: Vec<(char, usize, Option<usize>)> = chars[..start].to_vec();
    for &c in &new[start..new.len() - end] {
        let (pi, run) = typed.unwrap_or((paras.len().saturating_sub(1), None));
        out.push((c, pi, if c == '\n' { None } else { run }));
    }
    out.extend_from_slice(&chars[old.len() - end..]);
    // Written back: the body's own properties, then a paragraph per line, its runs grouped by look.
    let mut s = String::from("<xdr:txBody>");
    for e in tb.children.iter().filter(|e| e.name.ends_with("bodyPr") || e.name.ends_with("lstStyle")) {
        s.push_str(&crate::xml::dml_xml(e, if e.name.ends_with("bodyPr") { "a:bodyPr" } else { "a:lstStyle" }));
    }
    let lines: Vec<&[(char, usize, Option<usize>)]> = out.split(|c| c.0 == '\n').collect();
    // The paragraph each line's look comes from: its first character's, else the paragraph its
    // line break ended in (the one after it), else the last.
    let breaks: Vec<usize> = out.iter().filter(|c| c.0 == '\n').map(|c| c.1).collect();
    for (li, line) in lines.iter().enumerate() {
        let pi = line
            .first()
            .map(|c| c.1)
            .or_else(|| li.checked_sub(1).and_then(|k| breaks.get(k)).map(|p| (p + 1).min(paras.len().saturating_sub(1))))
            .unwrap_or(paras.len().saturating_sub(1));
        let p = paras.get(pi);
        s.push_str("<a:p>");
        if let Some(ppr) = p.and_then(|p| p.child("pPr")) {
            s.push_str(&crate::xml::dml_xml(ppr, "a:pPr"));
        }
        let mut k = 0;
        while k < line.len() {
            let look = line[k].2;
            let n = line[k..].iter().take_while(|c| c.2 == look).count();
            let run: String = line[k..k + n].iter().map(|c| c.0).collect();
            let r_pr = look.and_then(|i| looks.get(i)).cloned().unwrap_or_default();
            let _ = write!(s, "<a:r>{r_pr}<a:t>{}</a:t></a:r>", esc(&run));
            k += n;
        }
        if let Some(end) = p.and_then(|p| p.child("endParaRPr")) {
            s.push_str(&crate::xml::dml_xml(end, "a:endParaRPr"));
        }
        s.push_str("</a:p>");
    }
    s.push_str("</xdr:txBody>");
    Some(s)
}

fn plain_body(text: &str, like: Option<&El>) -> String {
    let body_pr = like
        .and_then(|b| b.child("bodyPr"))
        .map(|e| crate::xml::dml_xml(e, "a:bodyPr"))
        .unwrap_or_else(|| "<a:bodyPr wrap=\"square\" rtlCol=\"0\" anchor=\"t\"/>".into());
    let first_p = like.and_then(|b| b.child("p"));
    let p_pr = first_p.and_then(|p| p.child("pPr")).map(|e| crate::xml::dml_xml(e, "a:pPr")).unwrap_or_default();
    let r_pr = like
        .and_then(|b| b.kids("p").flat_map(|p| p.kids("r")).find_map(|r| r.child("rPr")))
        .map(|e| crate::xml::dml_xml(e, "a:rPr"))
        .unwrap_or_else(|| "<a:rPr lang=\"en-US\"/>".into());
    let end_pr = if like.is_some() {
        r_pr.replacen("<a:rPr", "<a:endParaRPr", 1).replace("</a:rPr>", "</a:endParaRPr>")
    } else {
        "<a:endParaRPr lang=\"en-US\"/>".into()
    };
    let mut s = format!("<xdr:txBody>{body_pr}<a:lstStyle/>");
    for line in text.split('\n') {
        if line.is_empty() {
            let _ = write!(s, "<a:p>{p_pr}{end_pr}</a:p>");
        } else {
            let _ = write!(s, "<a:p>{p_pr}<a:r>{r_pr}<a:t>{}</a:t></a:r></a:p>", esc(line));
        }
    }
    s.push_str("</xdr:txBody>");
    s
}

thread_local! {
    /// While a drawing part is written: each shape's object id to its drawing id there, for
    /// connectors' ends ([`write_drawing`] fills them in last).
    static DRAWING_IDS: std::cell::RefCell<std::collections::HashMap<u32, u32>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A connector's ends as written before every shape's drawing id is known: placeholders that
/// [`fill_connection_ids`] replaces.
fn connection_xml(c: &ShapeConnection) -> String {
    let end =
        |tag: &str, e: &Option<ConnectionEnd>| e.map(|e| format!("<a:{tag} id=\"{{{{cxn:{}}}}}\" idx=\"{}\"/>", e.id, e.site)).unwrap_or_default();
    format!("<xdr:cNvCxnSpPr>{}{}</xdr:cNvCxnSpPr>", end("stCxn", &c.start), end("endCxn", &c.end))
}

/// `x` with each connector end's placeholder replaced by its shape's drawing id, or the end left
/// out when that shape was not written.
fn fill_connection_ids(x: String, ids: &std::collections::HashMap<u32, u32>) -> String {
    if !x.contains("{{cxn:") {
        return x;
    }
    let mut out = String::with_capacity(x.len());
    let mut rest = x.as_str();
    while let Some(at) = rest.find("{{cxn:") {
        let Some(close) = rest[at..].find("}}") else { break };
        let id: Option<u32> = rest[at + 6..at + close].parse().ok();
        match id.and_then(|id| ids.get(&id)) {
            Some(written) => {
                out.push_str(&rest[..at]);
                out.push_str(&written.to_string());
                rest = &rest[at + close + 2..];
            }
            None => {
                // Drop the whole `<a:stCxn …/>` element.
                let start = rest[..at].rfind('<').unwrap_or(at);
                out.push_str(&rest[..start]);
                let end = rest[at..].find("/>").map_or(rest.len(), |e| at + e + 2);
                rest = &rest[end..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A shape whose box on the sheet is `bx` (pixels): a `xdr:sp`, or a `xdr:cxnSp` for a connector.
/// `id` is its drawing id (`cNvPr id`), `n` the number in its name (`Shape n`).
fn write_shape(s: &mut String, sh: &Shape, theme: &Theme, (id, n): (u32, u32), bx: (f64, f64, f64, f64)) {
    DRAWING_IDS.with(|m| m.borrow_mut().insert(sh.id, id));
    let start = s.len();
    write_shape_as_sp(s, sh, theme, (id, n), bx);
    // A connector is a `xdr:cxnSp`: its non-visual properties name its ends, and it has no text.
    if let Some(c) = &sh.connection {
        let mut part = s.split_off(start);
        part = part
            .replacen("<xdr:sp macro=\"\" textlink=\"\">", "<xdr:cxnSp macro=\"\">", 1)
            .replacen("<xdr:nvSpPr>", "<xdr:nvCxnSpPr>", 1)
            .replacen("</xdr:nvSpPr>", "</xdr:nvCxnSpPr>", 1);
        if let Some(a) = part.find("<xdr:cNvSpPr") {
            let end = part[a..].find("</xdr:cNvSpPr>").map(|e| a + e + "</xdr:cNvSpPr>".len()).or_else(|| part[a..].find("/>").map(|e| a + e + 2));
            if let Some(end) = end {
                part.replace_range(a..end, &connection_xml(c));
            }
        }
        if let (Some(a), Some(b)) = (part.find("<xdr:txBody>"), part.find("</xdr:txBody>")) {
            part.replace_range(a..b + "</xdr:txBody>".len(), "");
        }
        if let Some(a) = part.rfind("</xdr:sp>") {
            part.replace_range(a..a + "</xdr:sp>".len(), "</xdr:cxnSp>");
        }
        s.push_str(&part);
    }
}

/// One `xdr:sp` whose box on the sheet is `bx` (pixels).
fn write_shape_as_sp(s: &mut String, sh: &Shape, theme: &Theme, (id, n): (u32, u32), bx: (f64, f64, f64, f64)) {
    if sh.kind == ShapeKind::Ink {
        write_ink(s, sh, theme, (id, n), bx);
        return;
    }
    let tx = sh.kind == ShapeKind::TextBox;
    let _ = write!(
        s,
        "<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"Shape {}\"/><xdr:cNvSpPr{}/></xdr:nvSpPr><xdr:spPr><a:xfrm",
        n,
        if tx { " txBox=\"1\"" } else { "" },
    );
    if sh.rot.rem_euclid(21_600_000) != 0 {
        let _ = write!(s, " rot=\"{}\"", sh.rot.rem_euclid(21_600_000));
    }
    if sh.flip_h {
        s.push_str(" flipH=\"1\"");
    }
    if sh.flip_v {
        s.push_str(" flipV=\"1\"");
    }
    let (w_emu, h_emu) = (emu(bx.2.max(0.0)), emu(bx.3.max(0.0)));
    let _ = write!(s, "><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{w_emu}\" cy=\"{h_emu}\"/></a:xfrm>", emu(bx.0), emu(bx.1));
    match &sh.geom {
        Some(g) => match g.source.as_deref().and_then(|src| crate::custgeom::source_if_current(src, &g.paths, w_emu as f64, h_emu as f64)) {
            Some(src) => s.push_str(src),
            None => crate::custgeom::write_cust_geom(s, &g.paths),
        },
        None => {
            let (name, adj) = match &sh.preset {
                Some(p) if !p.name.is_empty() => (p.name.as_str(), p.adj.as_slice()),
                Some(p) => (kind_preset(sh.kind), p.adj.as_slice()),
                None => (kind_preset(sh.kind), &[][..]),
            };
            let _ = write!(s, "<a:prstGeom prst=\"{}\"><a:avLst>", esc_attr(name));
            for (n, f) in adj {
                let _ = write!(s, "<a:gd name=\"{}\" fmla=\"{}\"/>", esc_attr(n), esc_attr(f));
            }
            s.push_str("</a:avLst></a:prstGeom>");
        }
    }
    match &sh.gradient {
        Some(g) if !g.stops.is_empty() => {
            s.push_str("<a:gradFill");
            if let Some(r) = g.rot_with_shape {
                let _ = write!(s, " rotWithShape=\"{}\"", u8::from(r));
            }
            s.push_str("><a:gsLst>");
            for stop in &g.stops {
                let c = dml_color_xml(&stop.color, stop.alpha, theme).unwrap_or_else(|| "<a:srgbClr val=\"FFFFFF\"/>".into());
                let _ = write!(s, "<a:gs pos=\"{}\">{c}</a:gs>", stop.pos.min(100_000));
            }
            s.push_str("</a:gsLst>");
            match &g.path {
                Some(p) => {
                    let _ = write!(s, "<a:path path=\"{}\"/>", esc_attr(p));
                }
                None => {
                    let _ = write!(s, "<a:lin ang=\"{}\"", g.angle.rem_euclid(21_600_000));
                    if let Some(k) = g.scaled {
                        let _ = write!(s, " scaled=\"{}\"", u8::from(k));
                    }
                    s.push_str("/>");
                }
            }
            s.push_str("</a:gradFill>");
        }
        _ => match dml_color_xml(&sh.fill, sh.fill_alpha, theme) {
            Some(c) => {
                let _ = write!(s, "<a:solidFill>{c}</a:solidFill>");
            }
            None => s.push_str("<a:noFill/>"),
        },
    }
    let o = &sh.outline;
    let line_c = dml_color_xml(&sh.line, o.alpha, theme);
    // A line arrow is drawn with a pen and an arrowhead even when it names neither.
    let arrow = sh.kind == ShapeKind::Arrow && sh.preset.is_none() && sh.geom.is_none();
    // A style that gives a pen (`a:lnRef` past 0) would draw one where the shape has none.
    let styled_pen = sh
        .style_refs
        .as_deref()
        .and_then(|src| crate::xml::parse(src.as_bytes()).ok())
        .and_then(|st| st.child("lnRef").and_then(|r| r.attr_u32("idx")))
        .is_some_and(|idx| idx > 0);
    if line_c.is_some() || arrow || !o.is_empty() || styled_pen {
        s.push_str("<a:ln");
        if let Some(w) = o.width {
            let _ = write!(s, " w=\"{w}\"");
        }
        if let Some(c) = &o.cap {
            let _ = write!(s, " cap=\"{}\"", esc_attr(c));
        }
        s.push('>');
        match line_c {
            Some(c) => {
                let _ = write!(s, "<a:solidFill>{c}</a:solidFill>");
            }
            None if arrow => s.push_str("<a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill>"),
            None => s.push_str("<a:noFill/>"),
        }
        if let Some(d) = &o.dash {
            let _ = write!(s, "<a:prstDash val=\"{}\"/>", esc_attr(d));
        }
        match o.join.as_deref() {
            Some("round") => s.push_str("<a:round/>"),
            Some("bevel") => s.push_str("<a:bevel/>"),
            Some("miter") => match o.miter_limit {
                Some(l) => {
                    let _ = write!(s, "<a:miter lim=\"{l}\"/>");
                }
                None => s.push_str("<a:miter/>"),
            },
            _ => {}
        }
        if let Some(h) = &o.head {
            let _ = write!(s, "<a:headEnd type=\"{}\"/>", esc_attr(h));
        }
        match &o.tail {
            Some(t) => {
                let _ = write!(s, "<a:tailEnd type=\"{}\"/>", esc_attr(t));
            }
            None if arrow && o.head.is_none() => s.push_str("<a:tailEnd type=\"triangle\"/>"),
            None => {}
        }
        s.push_str("</a:ln>");
    }
    if let Some(fx) = sh.effects.as_ref().filter(|e| e.shadow.is_some() || e.glow.is_some()) {
        let emu = |pt: f32| ((pt.clamp(0.0, 3000.0)) * 12_700.0).round() as i64;
        s.push_str("<a:effectLst>");
        if let Some(g) = &fx.glow {
            let c = dml_color_xml(&g.color, g.alpha, theme).unwrap_or_else(|| "<a:srgbClr val=\"000000\"/>".into());
            let _ = write!(s, "<a:glow rad=\"{}\">{c}</a:glow>", emu(g.size));
        }
        if let Some(sh) = &fx.shadow {
            let c = dml_color_xml(&sh.color, sh.alpha, theme).unwrap_or_else(|| "<a:srgbClr val=\"000000\"/>".into());
            let _ = write!(
                s,
                "<a:outerShdw blurRad=\"{}\" dist=\"{}\" dir=\"{}\" algn=\"ctr\" rotWithShape=\"0\">{c}</a:outerShdw>",
                emu(sh.blur),
                emu(sh.distance),
                (sh.angle.rem_euclid(360.0) * 60_000.0).round() as i64
            );
        }
        s.push_str("</a:effectLst>");
    }
    s.push_str("</xdr:spPr>");
    if let Some(st) = &sh.style_refs {
        s.push_str(st);
    }
    let kept = sh.text_body.as_deref().and_then(|src| crate::xml::parse(src.as_bytes()).ok().map(|tb| (src, tb)));
    match kept {
        Some((src, tb)) if body_text(&tb) == sh.text => s.push_str(src),
        Some((_, tb)) => s.push_str(&plain_body(&sh.text, Some(&tb))),
        None if !sh.text.is_empty() => s.push_str(&plain_body(&sh.text, None)),
        None => {}
    }
    s.push_str("</xdr:sp>");
}

/// The drawing ids (`cNvPr id`) of one drawing part: each object's own id when it is one (from 1)
/// not yet written in the part, else a number after every id of the part.
struct PartIds {
    used: std::collections::HashSet<u32>,
    max: u32,
}

impl PartIds {
    fn new(objs: &[Obj<'_>]) -> PartIds {
        let max = objs
            .iter()
            .map(|o| match o {
                Obj::Image(img, _) => img.id,
                Obj::Chart(ch, _) => ch.id,
                Obj::Shape(sh) => sh.groups.iter().copied().fold(sh.id, u32::max),
                Obj::Diagram(d, _) => d.id,
            })
            .max()
            .unwrap_or(0);
        PartIds { used: std::collections::HashSet::new(), max }
    }

    fn take(&mut self, own: u32) -> u32 {
        if own >= 1 && self.used.insert(own) {
            return own;
        }
        loop {
            self.max = self.max.saturating_add(1);
            if self.used.insert(self.max) || self.max == u32::MAX {
                return self.max;
            }
        }
    }
}

/// The drawing part for a sheet's objects (relationship ids already assigned).
pub fn write_drawing(sheet: &Sheet, theme: &Theme, objs: &[Obj<'_>]) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">",
    );
    // Names count the objects (`Shape 1`, `Picture 2`); ids are the objects' own (unique in the
    // workbook, so in the part), which a reader keeps.
    let mut next = 1u32;
    let mut ids = PartIds::new(objs);
    let mut written_groups = std::collections::HashSet::new();
    DRAWING_IDS.with(|m| m.borrow_mut().clear());
    for o in objs {
        if let Obj::Shape(sh) = o
            && let Some(&key) = sh.groups.first()
        {
            // A group, written once around all its members, where its first member is.
            if written_groups.insert(key) {
                let members: Vec<&Shape> = objs
                    .iter()
                    .filter_map(|o| match o {
                        Obj::Shape(m) if m.groups.first() == Some(&key) => Some(*m),
                        _ => None,
                    })
                    .collect();
                let bx = bounds(sheet, &members);
                let (open, close) = anchor_open(sheet, &anchor_at(sheet, bx, &sh.anchor));
                s.push_str(&open);
                write_group(&mut s, sheet, theme, &members, 0, &mut next, &mut ids);
                s.push_str("<xdr:clientData/>");
                s.push_str(close);
            }
            continue;
        }
        next += 1;
        let n = next - 1;
        let id = ids.take(match o {
            Obj::Image(img, _) => img.id,
            Obj::Chart(ch, _) => ch.id,
            Obj::Shape(sh) => sh.id,
            Obj::Diagram(d, _) => d.id,
        });
        let close_tag;
        match o {
            Obj::Image(img, rid) => {
                let (open, close) = anchor_open(sheet, &img.anchor);
                close_tag = close;
                s.push_str(&open);
                let _ = write!(
                    s,
                    "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"Picture {}\" descr=\"{}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{rid}\"{}><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>",
                    n,
                    esc_attr(&img.alt),
                    match &img.blip_ext {
                        Some(ext) => format!("><a:extLst>{ext}</a:extLst></a:blip"),
                        None => "/".to_owned(),
                    },
                    xfrm(&img.anchor, "a:xfrm")
                );
            }
            Obj::Chart(ch, rid) if crate::chartex::is_chartex(ch.kind) => {
                // Chartex frames come with a fallback for readers that don't know them.
                let (open, close) = anchor_open(sheet, &ch.anchor);
                close_tag = close;
                s.push_str(&open);
                let (prefix, ns) = crate::chartex::requires(ch.kind);
                let _ = write!(
                    s,
                    "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:{prefix}=\"{ns}\" Requires=\"{prefix}\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"Chart {}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>{}<a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chart xmlns:cx=\"http://schemas.microsoft.com/office/drawing/2014/chartex\" r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice><mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"0\" name=\"\"/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr><xdr:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>This chart type can't be shown by this application.</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>",
                    n,
                    xfrm(&ch.anchor, "xdr:xfrm"),
                    xfrm(&ch.anchor, "a:xfrm")
                );
            }
            Obj::Chart(ch, rid) => {
                let (open, close) = anchor_open(sheet, &ch.anchor);
                close_tag = close;
                s.push_str(&open);
                let _ = write!(
                    s,
                    "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"Chart {}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>{}<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame>",
                    n,
                    xfrm(&ch.anchor, "xdr:xfrm")
                );
            }
            Obj::Shape(sh) => {
                let (open, close) = anchor_open(sheet, &sh.anchor);
                close_tag = close;
                s.push_str(&open);
                write_shape(&mut s, sh, theme, (id, n), anchor_box(sheet, &sh.anchor));
            }
            Obj::Diagram(d, ids) => {
                let (open, close) = anchor_open(sheet, &d.anchor);
                close_tag = close;
                s.push_str(&open);
                let name = if d.name.is_empty() { format!("Diagram {n}") } else { d.name.clone() };
                let _ = write!(
                    s,
                    "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>{}<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"><dgm:relIds xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"",
                    esc_attr(&name),
                    xfrm(&d.anchor, "xdr:xfrm")
                );
                for (attr, rid) in ids {
                    let _ = write!(s, " r:{attr}=\"{rid}\"");
                }
                s.push_str("/></a:graphicData></a:graphic></xdr:graphicFrame>");
            }
        }
        s.push_str("<xdr:clientData/>");
        s.push_str(close_tag);
    }
    s.push_str("</xdr:wsDr>");
    let ids = DRAWING_IDS.with(|m| std::mem::take(&mut *m.borrow_mut()));
    fill_connection_ids(s, &ids)
}

/// File extension for an image MIME type.
pub fn image_ext(mime: &str, data: &[u8]) -> &'static str {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpeg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        "image/x-emf" => "emf",
        "image/x-wmf" => "wmf",
        "image/webp" => "webp",
        _ => {
            if data.starts_with(b"\x89PNG") {
                "png"
            } else if data.starts_with(&[0xFF, 0xD8]) {
                "jpeg"
            } else if data.starts_with(b"GIF8") {
                "gif"
            } else {
                "png"
            }
        }
    }
}

pub fn image_content_type(ext: &str) -> &'static str {
    match ext {
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

// ---------------------------------------------------------------- shape text, for drawing

/// Where a shape's text sits across or down its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextPlace {
    #[default]
    Start,
    Middle,
    End,
}

/// One run of a shape's text with its look.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeTextRun {
    pub text: String,
    /// Points.
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Color,
    /// Opacity in 1,000ths of a percent.
    pub alpha: Option<u32>,
    pub font: Option<String>,
}

/// One paragraph of a shape's text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeTextPara {
    pub align: TextPlace,
    pub runs: Vec<ShapeTextRun>,
    /// The size (points) of an empty paragraph's line.
    pub size: f32,
    /// Line spacing: a multiple of the font's line, or exact points.
    pub line_multiple: f32,
    pub line_points: Option<f32>,
    /// Space before the paragraph and left indent, in points.
    pub space_before: f32,
    pub indent: f32,
}

/// A shape's text as it is drawn: paragraphs, their place down the box, and the box's insets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeText {
    pub paras: Vec<ShapeTextPara>,
    pub anchor: TextPlace,
    /// Left, top, right, bottom insets in points.
    pub insets: [f32; 4],
    pub wrap: bool,
    /// Art text's preset text warp (`a:bodyPr/a:prstTxWarp`, KitDrawing 1.3 DR26): its name and
    /// adjust values (`a:gd` `val`); the text bends with it.
    pub warp: Option<(String, Vec<(String, f64)>)>,
}

/// The style references Excel gives a shape it inserts (`xdr:style`): its outline and fill from
/// the theme's first accent, its text in the theme's body font, white (lt1).
pub const DEFAULT_SHAPE_STYLE: &str = "<xdr:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"><a:shade val=\"50000\"/></a:schemeClr></a:lnRef><a:fillRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:fillRef><a:effectRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></xdr:style>";

/// How a shape's text is drawn: from its kept text body (fonts, sizes, colours, alignment,
/// spacing) while that is still its text, else its plain text in the default look (11 points,
/// a text box's at its top left, another shape's in its middle).
pub fn shape_text(sh: &Shape) -> ShapeText {
    // An icon's text names it and an ink stroke's holds its points: neither is shown.
    if matches!(sh.kind, ShapeKind::Icon | ShapeKind::Ink) {
        return ShapeText::default();
    }
    let body = sh.text_body.as_deref().and_then(|src| crate::xml::parse(src.as_bytes()).ok()).filter(|tb| body_text(tb) == sh.text);
    // The text colour the shape's style gives (`xdr:style/a:fontRef`: white on Excel's default
    // shape), for runs that set none.
    let styled = sh
        .style_refs
        .as_deref()
        .and_then(|src| crate::xml::parse(src.as_bytes()).ok())
        .and_then(|st| st.child("fontRef").and_then(dml_color_alpha))
        .unwrap_or((Color::Rgb(0), None));
    let emu_pt = |v: Option<i64>, default: f32| v.map_or(default, |e| (e.clamp(0, 50_000_000) as f32) / 12_700.0);
    let Some(tb) = body else {
        let middle = sh.kind != ShapeKind::TextBox;
        let align = if middle { TextPlace::Middle } else { TextPlace::Start };
        let paras = sh
            .text
            .split('\n')
            .map(|line| ShapeTextPara {
                align,
                runs: if line.is_empty() {
                    vec![]
                } else {
                    vec![ShapeTextRun {
                        text: line.to_string(),
                        size: 11.0,
                        bold: false,
                        italic: false,
                        underline: false,
                        strike: false,
                        color: styled.0,
                        alpha: styled.1,
                        font: None,
                    }]
                },
                size: 11.0,
                line_multiple: 1.0,
                ..Default::default()
            })
            .collect();
        return ShapeText { paras, anchor: align, insets: [7.2, 3.6, 7.2, 3.6], wrap: true, warp: None };
    };
    let bp = tb.child("bodyPr");
    let attr = |n: &str| bp.and_then(|b| b.attr_i64(n));
    let anchor = match bp.and_then(|b| b.attr("anchor")) {
        Some("ctr") => TextPlace::Middle,
        Some("b") => TextPlace::End,
        _ => TextPlace::Start,
    };
    let insets = [emu_pt(attr("lIns"), 7.2), emu_pt(attr("tIns"), 3.6), emu_pt(attr("rIns"), 7.2), emu_pt(attr("bIns"), 3.6)];
    let wrap = bp.and_then(|b| b.attr("wrap")) != Some("none");
    let warp = bp.and_then(|b| b.child("prstTxWarp")).and_then(|w| {
        let name = w.attr("prst").filter(|p| *p != "textNoShape" && *p != "textPlain")?;
        let adj = w
            .child("avLst")
            .map(|av| {
                av.kids("gd")
                    .filter_map(|gd| Some((gd.attr("name")?.to_string(), gd.attr("fmla")?.strip_prefix("val ")?.trim().parse::<f64>().ok()?)))
                    .filter(|(_, v)| v.is_finite())
                    .take(8)
                    .collect()
            })
            .unwrap_or_default();
        Some((name.to_string(), adj))
    });
    let mut paras = Vec::new();
    for p in tb.kids("p").take(10_000) {
        let ppr = p.child("pPr");
        let align = match ppr.and_then(|x| x.attr("algn")) {
            Some("ctr") => TextPlace::Middle,
            Some("r") => TextPlace::End,
            _ => TextPlace::Start,
        };
        let spacing = |n: &str| ppr.and_then(|x| x.child(n));
        let line_multiple =
            spacing("lnSpc").and_then(|l| l.child("spcPct")).and_then(|v| v.attr_f64("val")).map_or(1.0, |v| (v / 100_000.0).clamp(0.1, 10.0) as f32);
        let line_points =
            spacing("lnSpc").and_then(|l| l.child("spcPts")).and_then(|v| v.attr_f64("val")).map(|v| (v / 100.0).clamp(0.0, 2000.0) as f32);
        let space_before =
            spacing("spcBef").and_then(|l| l.child("spcPts")).and_then(|v| v.attr_f64("val")).map_or(0.0, |v| (v / 100.0).clamp(0.0, 2000.0) as f32);
        let indent = emu_pt(ppr.and_then(|x| x.attr_i64("marL")), 0.0);
        let look = |rpr: Option<&El>, text: String| {
            let flag = |n: &str| rpr.and_then(|r| r.attr_bool(n)).unwrap_or(false);
            let (color, alpha) = rpr.and_then(|r| r.child("solidFill")).and_then(dml_color_alpha).unwrap_or(styled);
            ShapeTextRun {
                text,
                size: rpr.and_then(|r| r.attr_f64("sz")).map_or(11.0, |v| (v / 100.0).clamp(1.0, 4000.0) as f32),
                bold: flag("b"),
                italic: flag("i"),
                underline: rpr.and_then(|r| r.attr("u")).is_some_and(|u| u != "none"),
                strike: rpr.and_then(|r| r.attr("strike")).is_some_and(|u| u != "noStrike"),
                color,
                alpha,
                font: rpr.and_then(|r| r.child("latin").or_else(|| r.child("ea"))).and_then(|f| f.attr("typeface")).map(str::to_string),
            }
        };
        let runs: Vec<ShapeTextRun> = p.kids("r").filter_map(|r| Some(look(r.child("rPr"), r.child("t")?.text.clone()))).collect();
        let size = runs.first().map(|r| r.size).unwrap_or_else(|| look(p.child("endParaRPr"), String::new()).size);
        paras.push(ShapeTextPara { align, runs, size, line_multiple, line_points, space_before, indent });
    }
    ShapeText { paras, anchor, insets, wrap, warp }
}
