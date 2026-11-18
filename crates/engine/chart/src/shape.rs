//! Drawing shapes as primitives: the outline of their kind, preset or custom geometry, filled in a
//! colour or a gradient with its transparency, stroked with their pen, turned and mirrored about
//! their centre. Their text is the caller's (it draws glyphs in its own faces).

use kition_office_core::preset::FillMode;
use kition_sheet_model::{Color, CustomPath, Gradient, PathFill, Shape, ShapeKind, Theme};

use crate::{Paint, Prim, Rgba};

/// EMU per pixel (96 dpi).
const EMU_PER_PX: f32 = 9525.0;

/// A model colour with an opacity in 1,000ths of a percent; `None` for automatic.
pub fn shape_rgba(c: Color, alpha: Option<u32>, theme: &Theme) -> Option<Rgba> {
    let [r, g, b] = c.resolve(theme)?;
    let a = alpha.map_or(255, |a| ((a.min(100_000) as f32) / 100_000.0 * 255.0).round() as u8);
    Some([r, g, b, a])
}

/// One sub-path in the shape's box: its points, whether it closes, is filled and is stroked, and
/// how a custom path changes the fill (`lighten`, `darken`).
struct Sub {
    pts: Vec<[f32; 2]>,
    closed: bool,
    fill: Option<PathFill>,
    stroke: bool,
}

fn closed(pts: Vec<[f32; 2]>) -> Sub {
    Sub { pts, closed: true, fill: Some(PathFill::Norm), stroke: true }
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<[f32; 2]> {
    (0..72)
        .map(|i| {
            let a = i as f32 / 72.0 * std::f32::consts::TAU;
            [cx + rx * a.cos(), cy + ry * a.sin()]
        })
        .collect()
}

/// The outline of a preset by name from the preset catalog Kition's kits share
/// (`kition-office-core`), flattened into sub-paths; `None` for a name the catalog does not draw.
fn preset(sh: &Shape, name: &str, w: f32, h: f32) -> Option<Vec<Sub>> {
    let g = kition_office_core::preset::build(name, f64::from(w), f64::from(h), &adjust_values(sh, name))?;
    let mut out = Vec::new();
    for sub in &g.paths {
        let fill = match sub.fill {
            FillMode::Norm => Some(PathFill::Norm),
            FillMode::None => None,
            FillMode::Lighten => Some(PathFill::Lighten),
            FillMode::LightenLess => Some(PathFill::LightenLess),
            FillMode::Darken => Some(PathFill::Darken),
            FillMode::DarkenLess => Some(PathFill::DarkenLess),
        };
        let mut els = Vec::new();
        kurbo::flatten(&sub.path, 0.2, |el| els.push(el));
        let mut pts: Vec<[f32; 2]> = Vec::new();
        let finish = |pts: &mut Vec<[f32; 2]>, closed: bool, out: &mut Vec<Sub>| {
            if pts.len() > 1 {
                out.push(Sub { pts: std::mem::take(pts), closed, fill: if closed { fill } else { None }, stroke: sub.stroke });
            }
            pts.clear();
        };
        for el in els.into_iter().take(1_000_000) {
            match el {
                kurbo::PathEl::MoveTo(p) => {
                    finish(&mut pts, false, &mut out);
                    pts.push([p.x as f32, p.y as f32]);
                }
                kurbo::PathEl::LineTo(p) => pts.push([p.x as f32, p.y as f32]),
                kurbo::PathEl::ClosePath => finish(&mut pts, true, &mut out),
                _ => {}
            }
        }
        // An open sub-path (a line, an arc) still strokes; a filled one closes implicitly.
        let implicit = fill.is_some();
        finish(&mut pts, implicit, &mut out);
    }
    Some(out)
}

/// A preset's adjust values (`val N` formulas, by guide name) in the catalog's order; a guide not
/// given takes the preset's default.
fn adjust_values(sh: &Shape, name: &str) -> Vec<f64> {
    let Some(p) = sh.preset.as_ref().filter(|p| p.name == name) else { return Vec::new() };
    let mut out: Vec<Option<f64>> = Vec::new();
    for (guide, formula) in p.adj.iter().take(16) {
        let (Some(i), Some(v)) = (
            kition_office_core::model::guide_index(guide),
            formula.strip_prefix("val ").and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()),
        ) else {
            continue;
        };
        if i < 16 {
            if out.len() <= i {
                out.resize(i + 1, None);
            }
            if let Some(slot) = out.get_mut(i) {
                *slot = Some(v);
            }
        }
    }
    let defaults = kition_office_core::preset::info(name).map(|i| i.defaults).unwrap_or(&[]);
    out.iter().enumerate().map(|(i, v)| v.or_else(|| defaults.get(i).copied()).unwrap_or(0.0)).collect()
}

/// The outline of a shape's kind.
fn kind_outline(sh: &Shape, w: f32, h: f32) -> Vec<Sub> {
    match sh.kind {
        ShapeKind::RoundedRectangle => preset(sh, "roundRect", w, h).unwrap_or_default(),
        ShapeKind::Ellipse => preset(sh, "ellipse", w, h).unwrap_or_default(),
        ShapeKind::Triangle => preset(sh, "triangle", w, h).unwrap_or_default(),
        ShapeKind::Line | ShapeKind::Arrow => vec![Sub { pts: vec![[0.0, 0.0], [w, h]], closed: false, fill: None, stroke: true }],
        ShapeKind::Ink => {
            let pts: Vec<[f32; 2]> = sh
                .text
                .split(';')
                .take(100_000)
                .filter_map(|p| {
                    let (x, y) = p.split_once(',')?;
                    Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
                })
                .collect();
            vec![Sub { pts, closed: false, fill: None, stroke: true }]
        }
        _ => vec![closed(vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]])],
    }
}

/// Our path syntax (`M L C Q Z` with numbers) into commands.
fn parse_path(d: &str) -> Vec<(char, Vec<f32>)> {
    let mut out: Vec<(char, Vec<f32>)> = Vec::new();
    for token in d.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).take(2_000_000) {
        match token {
            "M" | "L" | "C" | "Q" | "Z" => out.push((token.chars().next().unwrap_or('Z'), Vec::new())),
            _ => {
                if let (Some(last), Ok(v)) = (out.last_mut(), token.parse::<f32>())
                    && v.is_finite()
                {
                    last.1.push(v);
                }
            }
        }
    }
    out
}

/// The sub-paths of a custom path, flattened, in the shape's box `w` × `h`.
fn custom(p: &CustomPath, w: f32, h: f32) -> Vec<Sub> {
    let sx = if p.w > 0.0 { w / p.w as f32 } else { 1.0 / EMU_PER_PX };
    let sy = if p.h > 0.0 { h / p.h as f32 } else { 1.0 / EMU_PER_PX };
    let at = |x: f32, y: f32| [x * sx, y * sy];
    let fill = (p.fill != PathFill::None).then_some(p.fill);
    let mut subs = Vec::new();
    let mut cur: Vec<[f32; 2]> = Vec::new();
    let flush = |cur: &mut Vec<[f32; 2]>, closed: bool, subs: &mut Vec<Sub>| {
        if cur.len() > 1 {
            subs.push(Sub { pts: std::mem::take(cur), closed, fill, stroke: p.stroke });
        } else {
            cur.clear();
        }
    };
    for (c, v) in parse_path(&p.d) {
        let last = cur.last().copied().unwrap_or([0.0, 0.0]);
        match (c, v.as_slice()) {
            ('M', [x, y, ..]) => {
                flush(&mut cur, false, &mut subs);
                cur.push(at(*x, *y));
            }
            ('L', [x, y, ..]) => cur.push(at(*x, *y)),
            ('C', [x1, y1, x2, y2, x, y, ..]) => {
                let (p1, p2, p3) = (at(*x1, *y1), at(*x2, *y2), at(*x, *y));
                for i in 1..=16 {
                    let t = i as f32 / 16.0;
                    let u = 1.0 - t;
                    let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    cur.push([
                        k[0] * last[0] + k[1] * p1[0] + k[2] * p2[0] + k[3] * p3[0],
                        k[0] * last[1] + k[1] * p1[1] + k[2] * p2[1] + k[3] * p3[1],
                    ]);
                }
            }
            ('Q', [x1, y1, x, y, ..]) => {
                let (p1, p2) = (at(*x1, *y1), at(*x, *y));
                for i in 1..=12 {
                    let t = i as f32 / 12.0;
                    let u = 1.0 - t;
                    cur.push([u * u * last[0] + 2.0 * u * t * p1[0] + t * t * p2[0], u * u * last[1] + 2.0 * u * t * p1[1] + t * t * p2[1]]);
                }
            }
            ('Z', _) => {
                let start = cur.first().copied();
                flush(&mut cur, true, &mut subs);
                // A path going on after a close starts where that sub-path did.
                if let Some(s) = start {
                    cur.push(s);
                }
            }
            _ => {}
        }
        if cur.len() > 1_000_000 {
            break;
        }
    }
    flush(&mut cur, false, &mut subs);
    subs
}

/// The fill colour a `lighten`/`darken` path gives `c`.
fn shade(c: Rgba, how: PathFill) -> Rgba {
    let (k, to) = match how {
        PathFill::Lighten => (0.4, 255.0),
        PathFill::LightenLess => (0.2, 255.0),
        PathFill::Darken => (0.4, 0.0),
        PathFill::DarkenLess => (0.2, 0.0),
        _ => return c,
    };
    let m = |v: u8| (f32::from(v) + (to - f32::from(v)) * k).round() as u8;
    [m(c[0]), m(c[1]), m(c[2]), c[3]]
}

/// The paint of a gradient over the box `w` × `h` (before the shape turns).
fn gradient_paint(g: &Gradient, theme: &Theme, w: f32, h: f32) -> Option<Paint> {
    let mut stops: Vec<(f32, Rgba)> =
        g.stops.iter().filter_map(|s| Some((s.pos.min(100_000) as f32 / 100_000.0, shape_rgba(s.color, s.alpha, theme)?))).collect();
    if stops.is_empty() {
        return None;
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (cx, cy) = (w / 2.0, h / 2.0);
    if g.path.is_some() {
        return Some(Paint::Radial { center: [cx, cy], radius: (w * w + h * h).sqrt() / 2.0, stops });
    }
    let a = (g.angle as f32 / 60_000.0).to_radians();
    let (dx, dy) = (a.cos(), a.sin());
    let half = (w * dx.abs() + h * dy.abs()) / 2.0;
    Some(Paint::Linear { from: [cx - dx * half, cy - dy * half], to: [cx + dx * half, cy + dy * half], stops })
}

/// An arrowhead at `tip`, coming from `from`, `size` long.
fn arrowhead(tip: [f32; 2], from: [f32; 2], size: f32, kind: &str, color: Rgba) -> Option<Prim> {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if !(len.is_finite() && len > 1e-3) {
        return None;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy, ux);
    let back = [tip[0] - ux * size, tip[1] - uy * size];
    let half = size * 0.5;
    let pts = match kind {
        "none" => return None,
        "diamond" => vec![
            tip,
            [back[0] + ux * size / 2.0 + nx * half, back[1] + uy * size / 2.0 + ny * half],
            back,
            [back[0] + ux * size / 2.0 - nx * half, back[1] + uy * size / 2.0 - ny * half],
        ],
        "oval" => ellipse(tip[0] - ux * half, tip[1] - uy * half, half, half),
        _ => vec![tip, [back[0] + nx * half, back[1] + ny * half], [back[0] - nx * half, back[1] - ny * half]],
    };
    Some(Prim::Path { contours: vec![pts], paint: Paint::Solid(color) })
}

/// The primitives of a shape `w` × `h` (sheet pixels) with its top-left at the origin: its fill,
/// then its outline and arrowheads. Text is not included.
pub fn render_shape(sh: &Shape, theme: &Theme, w: f32, h: f32) -> Vec<Prim> {
    let (w, h) = (if w.is_finite() { w.max(0.0) } else { 0.0 }, if h.is_finite() { h.max(0.0) } else { 0.0 });
    let subs = match (&sh.geom, &sh.preset) {
        (Some(g), _) if !g.paths.is_empty() => g.paths.iter().take(1000).flat_map(|p| custom(p, w, h)).collect(),
        (_, Some(p)) => preset(sh, &p.name, w, h).unwrap_or_else(|| kind_outline(sh, w, h)),
        _ => kind_outline(sh, w, h),
    };
    // Mirroring and rotation about the centre.
    let (cx, cy) = (w / 2.0, h / 2.0);
    let rad = (sh.rot as f32 / 60_000.0).to_radians();
    let (sin, cos) = rad.sin_cos();
    let turn = |p: [f32; 2]| {
        let x = if sh.flip_h { w - p[0] } else { p[0] } - cx;
        let y = if sh.flip_v { h - p[1] } else { p[1] } - cy;
        [cx + x * cos - y * sin, cy + x * sin + y * cos]
    };
    let ink = sh.kind == ShapeKind::Ink;
    let fill_paint = if ink {
        None
    } else {
        match &sh.gradient {
            Some(g) => gradient_paint(g, theme, w, h),
            None => shape_rgba(sh.fill, sh.fill_alpha, theme).map(Paint::Solid),
        }
    };
    let turn_paint = |p: Paint| match p {
        Paint::Linear { from, to, stops } => Paint::Linear { from: turn(from), to: turn(to), stops },
        Paint::Radial { center, radius, stops } => Paint::Radial { center: turn(center), radius, stops },
        solid => solid,
    };
    let mut out = Vec::new();
    // Shadow and glow, under the shape: the shadow as copies of its filled outline moved its
    // distance (spread over its blur), the glow as wide soft pens along its outline.
    if let Some(fx) = sh.effects.as_ref().filter(|_| !ink) {
        let closed: Vec<Vec<[f32; 2]>> = subs.iter().filter(|s| s.pts.len() > 2).map(|s| s.pts.iter().map(|p| turn(*p)).collect()).collect();
        if let Some(sd) = &fx.shadow
            && let Some(rgba) = shape_rgba(sd.color, sd.alpha, theme)
        {
            let a = sd.angle.to_radians();
            let (dx, dy) = (sd.distance * a.cos(), sd.distance * a.sin());
            let blur = sd.blur.clamp(0.0, 100.0);
            let spots: Vec<(f32, f32)> = if blur < 0.5 {
                vec![(0.0, 0.0)]
            } else {
                (0..8).map(|k| (k as f32).to_radians() * 45.0).map(|t| (blur / 2.0 * t.cos(), blur / 2.0 * t.sin())).collect()
            };
            let alpha = (f32::from(rgba[3]) / spots.len() as f32 * if spots.len() > 1 { 2.0 } else { 1.0 }).clamp(1.0, 255.0) as u8;
            for (ox, oy) in spots {
                let moved: Vec<Vec<[f32; 2]>> = closed.iter().map(|c| c.iter().map(|p| [p[0] + dx + ox, p[1] + dy + oy]).collect()).collect();
                out.push(Prim::Path { contours: moved, paint: Paint::Solid([rgba[0], rgba[1], rgba[2], alpha]) });
            }
        }
        if let Some(g) = &fx.glow
            && let Some(rgba) = shape_rgba(g.color, g.alpha, theme)
        {
            let size = g.size.clamp(0.0, 150.0);
            for k in (1..=4).rev() {
                let width = 2.0 * size * k as f32 / 4.0;
                let alpha = (f32::from(rgba[3]) / 4.0).clamp(1.0, 255.0) as u8;
                for c in &closed {
                    let mut pts = c.clone();
                    if let Some(first) = pts.first().copied() {
                        pts.push(first);
                    }
                    out.push(Prim::Line { pts, color: [rgba[0], rgba[1], rgba[2], alpha], width, dash: false });
                }
            }
        }
    }
    // Fills: the plain sub-paths together (so holes stay holes), lightened or darkened ones apart.
    if let Some(paint) = fill_paint {
        let plain: Vec<Vec<[f32; 2]>> =
            subs.iter().filter(|s| s.fill == Some(PathFill::Norm) && s.pts.len() > 2).map(|s| s.pts.iter().map(|p| turn(*p)).collect()).collect();
        if !plain.is_empty() {
            out.push(Prim::Path { contours: plain, paint: turn_paint(paint.clone()) });
        }
        let base = paint.mean();
        for s in subs.iter().filter(|s| s.pts.len() > 2) {
            if let Some(how) = s.fill.filter(|f| *f != PathFill::Norm) {
                out.push(Prim::Path { contours: vec![s.pts.iter().map(|p| turn(*p)).collect()], paint: Paint::Solid(shade(base, how)) });
            }
        }
    }
    let o = &sh.outline;
    let line_arrow = sh.kind == ShapeKind::Arrow && sh.preset.is_none() && sh.geom.is_none();
    let pen = if ink {
        // Ink keeps its width in the red channel of `fill` (shapes had no width before `outline`).
        match sh.fill {
            Color::Rgb(v) => (((v >> 16) & 0xFF) as f32).max(1.0),
            _ => 2.0,
        }
    } else {
        o.width.map_or(1.0, |e| (e as f32 / EMU_PER_PX).max(0.5))
    };
    let stroke = shape_rgba(sh.line, o.alpha, theme).or_else(|| line_arrow.then_some([0, 0, 0, 255]));
    if let Some(color) = stroke {
        let dash = o.dash.as_deref().is_some_and(|d| d != "solid");
        for s in subs.iter().filter(|s| s.stroke && s.pts.len() > 1) {
            let mut pts: Vec<[f32; 2]> = s.pts.iter().map(|p| turn(*p)).collect();
            if s.closed
                && let Some(first) = pts.first().copied()
            {
                pts.push(first);
            }
            out.push(Prim::Line { pts, color, width: pen, dash });
        }
        // Arrowheads at the ends of the first open sub-path.
        if let Some(s) = subs.iter().find(|s| !s.closed && s.pts.len() > 1) {
            let pts: Vec<[f32; 2]> = s.pts.iter().map(|p| turn(*p)).collect();
            let size = (pen * 4.0).max(6.0);
            let tail = o.tail.as_deref().or((line_arrow && o.head.is_none()).then_some("triangle"));
            if let (Some(kind), [.., a, b]) = (tail, pts.as_slice()) {
                out.extend(arrowhead(*b, *a, size, kind, color));
            }
            if let (Some(kind), [a, b, ..]) = (o.head.as_deref(), pts.as_slice()) {
                out.extend(arrowhead(*a, *b, size, kind, color));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_sheet_model::{CustomGeom, GradientStop, Outline, PresetGeom};

    fn theme() -> Theme {
        Theme::default()
    }

    fn pixel(img: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn a_see_through_fill_and_pen_keep_their_opacity() {
        let sh = Shape {
            fill: Color::Rgb(0xFF0000),
            fill_alpha: Some(50_000),
            line: Color::Rgb(0x0000FF),
            outline: Outline { alpha: Some(25_000), width: Some(4 * 9525), ..Default::default() },
            ..Default::default()
        };
        let prims = render_shape(&sh, &theme(), 40.0, 20.0);
        let Prim::Path { paint: Paint::Solid(fill), .. } = &prims[0] else { panic!("{prims:?}") };
        assert_eq!(*fill, [255, 0, 0, 128]);
        let Prim::Line { color, width, .. } = &prims[1] else { panic!("{prims:?}") };
        assert_eq!((*color, *width), ([0, 0, 255, 64], 4.0));
        // Over a transparent background the middle keeps half its opacity.
        let img = crate::rasterize(&prims, 40, 20, [255, 255, 255, 0]);
        assert_eq!(pixel(&img, 40, 20, 10), [255, 0, 0, 128]);
    }

    #[test]
    fn a_gradient_runs_along_its_angle() {
        let g = Gradient {
            stops: vec![
                GradientStop { pos: 0, color: Color::Rgb(0x000000), alpha: None },
                GradientStop { pos: 100_000, color: Color::Rgb(0xFFFFFF), alpha: Some(0) },
            ],
            angle: 0,
            ..Default::default()
        };
        let sh = Shape { gradient: Some(g.clone()), ..Default::default() };
        let img = crate::rasterize(&render_shape(&sh, &theme(), 100.0, 10.0), 100, 10, [0, 0, 0, 0]);
        let (left, right) = (pixel(&img, 100, 1, 5), pixel(&img, 100, 98, 5));
        assert!(left[0] < 10 && left[3] > 245, "{left:?}");
        assert!(right[3] < 10, "the last stop is see-through: {right:?}");
        // At 90 degrees it runs top to bottom.
        let sh = Shape { gradient: Some(Gradient { angle: 5_400_000, ..g }), ..Default::default() };
        let img = crate::rasterize(&render_shape(&sh, &theme(), 10.0, 100.0), 10, 100, [0, 0, 0, 0]);
        assert!(pixel(&img, 10, 5, 1)[3] > 245 && pixel(&img, 10, 5, 98)[3] < 10);
    }

    #[test]
    fn custom_geometry_is_drawn_in_the_shapes_box_and_holes_stay_holes() {
        // A square with a square hole, in a 100 × 100 path space, on a 50 × 50 shape.
        let geom = CustomGeom {
            paths: vec![CustomPath {
                w: 100.0,
                h: 100.0,
                d: "M 0 0 L 100 0 L 100 100 L 0 100 Z M 25 25 L 75 25 L 75 75 L 25 75 Z".into(),
                ..Default::default()
            }],
            source: None,
        };
        let sh = Shape { fill: Color::Rgb(0x00FF00), geom: Some(geom), ..Default::default() };
        let prims = render_shape(&sh, &theme(), 50.0, 50.0);
        let Prim::Path { contours, .. } = &prims[0] else { panic!("{prims:?}") };
        assert_eq!(contours.len(), 2);
        assert_eq!(contours[0][2], [50.0, 50.0]);
        let img = crate::rasterize(&prims, 50, 50, [255, 255, 255, 0]);
        assert_eq!(pixel(&img, 50, 5, 5), [0, 255, 0, 255]);
        assert_eq!(pixel(&img, 50, 25, 25)[3], 0, "the hole");
        // An open, unfilled path is only stroked.
        let geom =
            CustomGeom { paths: vec![CustomPath { w: 10.0, h: 10.0, d: "M 0 10 L 0 0".into(), fill: PathFill::None, stroke: true }], source: None };
        let sh = Shape { line: Color::Rgb(0), fill: Color::Rgb(0xFF0000), geom: Some(geom), ..Default::default() };
        let prims = render_shape(&sh, &theme(), 2.0, 30.0);
        assert!(matches!(prims.as_slice(), [Prim::Line { pts, .. }] if pts == &vec![[0.0, 30.0], [0.0, 0.0]]), "{prims:?}");
    }

    #[test]
    fn rotation_and_mirroring_turn_the_outline_about_the_centre() {
        let sh = Shape { kind: ShapeKind::Line, line: Color::Rgb(0), flip_v: true, ..Default::default() };
        let prims = render_shape(&sh, &theme(), 10.0, 10.0);
        let Prim::Line { pts, .. } = &prims[0] else { panic!() };
        assert_eq!(pts, &vec![[0.0, 10.0], [10.0, 0.0]], "a flipped line runs up");
        let sh = Shape { kind: ShapeKind::Line, line: Color::Rgb(0), rot: 5_400_000, ..Default::default() };
        let prims = render_shape(&sh, &theme(), 10.0, 0.0);
        let Prim::Line { pts, .. } = &prims[0] else { panic!() };
        assert!((pts[0][0] - 5.0).abs() < 1e-4 && (pts[0][1] + 5.0).abs() < 1e-4 && (pts[1][1] - 5.0).abs() < 1e-4, "{pts:?}");
    }

    #[test]
    fn presets_by_name_and_arrowheads() {
        let sh = Shape { fill: Color::Rgb(0), preset: Some(PresetGeom { name: "diamond".into(), adj: vec![] }), ..Default::default() };
        let Prim::Path { contours, .. } = &render_shape(&sh, &theme(), 20.0, 10.0)[0] else { panic!() };
        assert_eq!(contours[0], vec![[10.0, 0.0], [20.0, 5.0], [10.0, 10.0], [0.0, 5.0]]);
        // An arrow without a pen colour is drawn black, with a head at its end.
        let sh = Shape { kind: ShapeKind::Arrow, ..Default::default() };
        let prims = render_shape(&sh, &theme(), 40.0, 0.0);
        assert!(matches!(prims.as_slice(), [Prim::Line { color: [0, 0, 0, 255], .. }, Prim::Path { .. }]), "{prims:?}");
        // A pen's own head and tail.
        let sh = Shape {
            kind: ShapeKind::Line,
            line: Color::Rgb(0),
            outline: Outline { head: Some("oval".into()), ..Default::default() },
            ..Default::default()
        };
        assert_eq!(render_shape(&sh, &theme(), 40.0, 0.0).len(), 2);
    }

    #[test]
    fn presets_come_from_the_shared_catalog() {
        // A preset this kit never drew itself now draws, as the other kits draw it.
        let heart = Shape { fill: Color::Rgb(0xFF0000), preset: Some(PresetGeom { name: "heart".into(), adj: vec![] }), ..Default::default() };
        let prims = render_shape(&heart, &Theme::default(), 100.0, 80.0);
        assert!(prims.iter().any(|p| matches!(p, Prim::Path { .. })), "{prims:?}");
        // The outline spans what the shared geometry spans.
        let subs = preset(&heart, "heart", 100.0, 80.0).unwrap_or_default();
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in subs.iter().flat_map(|s| &s.pts) {
            (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
        }
        let shared = kition_office_core::preset::build("heart", 100.0, 80.0, &[]).map(|g| kurbo::Shape::bounding_box(&g.outline()));
        let bb = shared.unwrap_or_default();
        for (a, b) in [(x0, bb.x0), (y0, bb.y0), (x1, bb.x1), (y1, bb.y1)] {
            assert!((f64::from(a) - b).abs() < 0.5, "{a} vs {b}");
        }
    }

    #[test]
    fn adjust_values_follow_the_guides_with_defaults() {
        let sh = Shape {
            preset: Some(PresetGeom { name: "rightArrow".into(), adj: vec![("adj2".into(), "val 40000".into()), ("adj9".into(), "bad".into())] }),
            ..Default::default()
        };
        let defaults = kition_office_core::preset::info("rightArrow").map(|i| i.defaults.to_vec()).unwrap_or_default();
        assert_eq!(adjust_values(&sh, "rightArrow"), vec![defaults[0], 40000.0]);
        assert!(adjust_values(&sh, "roundRect").is_empty(), "another preset's values are not used");
    }
}
