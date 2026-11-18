//! Laying out a drawing shape's text in its box: paragraphs broken into lines at spaces and
//! between East Asian characters, each line placed by its paragraph's alignment, the block by the
//! box's anchor and insets. Surfaces measure with their own faces and draw the pieces.

pub use kition_sheet_xlsx::{ShapeText, ShapeTextPara, ShapeTextRun, TextPlace, shape_text};

/// Sheet pixels per point.
const PX_PER_PT: f32 = 96.0 / 72.0;

/// A piece of one run on one line: where its text starts (left, baseline) in the shape's box, in
/// sheet pixels, and its size in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece<'a> {
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
    pub text: String,
    pub run: &'a ShapeTextRun,
}

/// Whether a line may break before `c` (East Asian scripts break between characters).
fn breaks_around(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// Words: runs of characters up to and including a space, or one East Asian character.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if breaks_around(c) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(c.to_string());
            continue;
        }
        cur.push(c);
        if c == ' ' {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The pieces of `text` in a box `w` × `h` (sheet pixels); `measure` gives the width (pixels) of
/// some text of a run at a size (pixels).
pub fn layout<'a>(text: &'a ShapeText, w: f32, h: f32, measure: &dyn Fn(&ShapeTextRun, f32, &str) -> f32) -> Vec<Piece<'a>> {
    let [il, it, ir, ib] = text.insets.map(|v| v * PX_PER_PT);
    // Lines: (height, baseline offset, width, align, pieces with x from the line start).
    let mut lines: Vec<(f32, f32, f32, TextPlace, Vec<Piece<'a>>)> = Vec::new();
    let mut space_before = Vec::new();
    for para in text.paras.iter().take(10_000) {
        let indent = para.indent * PX_PER_PT;
        let avail = (w - il - ir - indent).max(1.0);
        let height = |size_px: f32| para.line_points.map_or(size_px * 1.2 * para.line_multiple, |p| p * PX_PER_PT);
        let first_line = lines.len();
        let mut line: Vec<Piece<'a>> = Vec::new();
        let (mut x, mut tallest) = (0.0f32, 0.0f32);
        let finish = |line: &mut Vec<Piece<'a>>, x: f32, tallest: f32, lines: &mut Vec<(f32, f32, f32, TextPlace, Vec<Piece<'a>>)>| {
            let size = if tallest > 0.0 { tallest } else { para.size * PX_PER_PT };
            let lh = height(size).max(1.0);
            // The baseline sits where a line of `size` puts it, the extra spacing above.
            lines.push((lh, lh - size * 0.25, x, para.align, std::mem::take(line)));
        };
        for run in para.runs.iter().take(10_000) {
            let size = run.size * PX_PER_PT;
            for word in words(&run.text) {
                let ww = measure(run, size, &word);
                if text.wrap && x > 0.0 && x + ww > avail && !word.trim().is_empty() {
                    finish(&mut line, x, tallest, &mut lines);
                    (x, tallest) = (0.0, 0.0);
                }
                tallest = tallest.max(size);
                match line.last_mut() {
                    // Words of one run on one line draw as one piece.
                    Some(p) if std::ptr::eq(p.run, run) => p.text.push_str(&word),
                    _ => line.push(Piece { x: indent + x, baseline: 0.0, size, text: word, run }),
                }
                x += ww;
            }
        }
        finish(&mut line, x, tallest, &mut lines);
        space_before.push((first_line, para.space_before * PX_PER_PT));
    }
    // Space before each paragraph (not above the first).
    let mut extra = vec![0.0f32; lines.len()];
    for (i, (line, space)) in space_before.into_iter().enumerate() {
        if i > 0
            && let Some(e) = extra.get_mut(line)
        {
            *e = space;
        }
    }
    let total: f32 = lines.iter().map(|l| l.0).sum::<f32>() + extra.iter().sum::<f32>();
    let mut y = match text.anchor {
        TextPlace::Start => it,
        TextPlace::Middle => it + ((h - it - ib) - total) / 2.0,
        TextPlace::End => h - ib - total,
    };
    let mut out = Vec::new();
    for ((lh, base, width, align, pieces), gap) in lines.into_iter().zip(extra) {
        y += gap;
        let free = (w - il - ir - width).max(0.0);
        let dx = il
            + match align {
                TextPlace::Start => 0.0,
                TextPlace::Middle => free / 2.0,
                TextPlace::End => free,
            };
        for mut p in pieces {
            p.x += dx;
            p.baseline = y + base;
            out.push(p);
        }
        y += lh;
    }
    out
}

/// The face shape text is drawn and printed with: egui's default sans (Ubuntu Light), as Kition
/// Sheet's page draws it (`None` only if the bundled face would not load).
pub fn face() -> Option<&'static ab_glyph::FontArc> {
    static FACE: std::sync::OnceLock<Option<ab_glyph::FontArc>> = std::sync::OnceLock::new();
    FACE.get_or_init(|| ab_glyph::FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).ok()).as_ref()
}

/// The width (pixels) of `text` at `size` pixels in `font`, kerned.
pub fn text_width(font: &ab_glyph::FontArc, size: f32, text: &str) -> f32 {
    use ab_glyph::{Font, ScaleFont};
    let scaled = font.as_scaled(ab_glyph::PxScale::from(size));
    let mut last = None;
    let mut w = 0.0;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = last {
            w += scaled.kern(prev, id);
        }
        w += scaled.h_advance(id);
        last = Some(id);
    }
    w
}

/// The glyph outlines of `text` at `size` pixels in `font`, its baseline at `y` from `x`;
/// `italic` slants them (a synthetic oblique, as the page draws it).
pub fn outline(font: &ab_glyph::FontArc, size: f32, text: &str, x: f64, y: f64, italic: bool) -> kition_office_core::kurbo::BezPath {
    use ab_glyph::{Font, ScaleFont};
    use kition_office_core::kurbo::{BezPath, Point};
    let scaled = font.as_scaled(ab_glyph::PxScale::from(size));
    let (kx, ky) = (f64::from(scaled.h_scale_factor()), f64::from(scaled.v_scale_factor()));
    let mut path = BezPath::new();
    let (mut pen, mut last) = (x, None);
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = last {
            pen += f64::from(scaled.kern(prev, id));
        }
        let at = |p: ab_glyph::Point| {
            let (gx, gy) = (f64::from(p.x) * kx, f64::from(p.y) * ky);
            Point::new(pen + gx + if italic { gy * 0.21 } else { 0.0 }, y - gy)
        };
        if let Some(o) = font.outline(id) {
            let mut end: Option<Point> = None;
            for curve in &o.curves {
                let (start, stop) = match curve {
                    ab_glyph::OutlineCurve::Line(a, b) | ab_glyph::OutlineCurve::Quad(a, _, b) | ab_glyph::OutlineCurve::Cubic(a, _, _, b) => {
                        (at(*a), at(*b))
                    }
                };
                if end.is_none_or(|e| (e - start).hypot() > 1e-6) {
                    if end.is_some() {
                        path.close_path();
                    }
                    path.move_to(start);
                }
                match curve {
                    ab_glyph::OutlineCurve::Line(..) => path.line_to(stop),
                    ab_glyph::OutlineCurve::Quad(_, q, _) => path.quad_to(at(*q), stop),
                    ab_glyph::OutlineCurve::Cubic(_, q, r, _) => path.curve_to(at(*q), at(*r), stop),
                }
                end = Some(stop);
            }
            if end.is_some() {
                path.close_path();
            }
        }
        pen += f64::from(scaled.h_advance(id));
        last = Some(id);
    }
    path
}

/// Art text (KitDrawing 1.3 DR26): runs' outlines (shape pixels, each with its colour) filling the
/// text area `margins` (x, y, w, h), bent by `preset` at its `adjust` values with the warp
/// geometry Kition's kits share, and mapped by `place`: each run's contours, flattened.
pub fn warped_contours<C: Copy>(
    runs: &[(C, kition_office_core::kurbo::BezPath)],
    margins: (f64, f64, f64, f64),
    preset: &str,
    adjust: &[(String, f64)],
    place: kition_office_core::kurbo::Affine,
) -> Vec<(C, Vec<Vec<[f32; 2]>>)> {
    use kition_office_core::kurbo::{Affine, PathEl, Shape as _};
    let ink = runs.iter().filter(|(_, p)| !p.elements().is_empty()).map(|(_, p)| p.bounding_box()).reduce(|a, b| a.union(b));
    let Some(ink) = ink.filter(|b| b.width() > 0.0 && b.height() > 0.0) else { return Vec::new() };
    let (x, y, w, h) = margins;
    let fit = Affine::scale_non_uniform(w / ink.width(), h / ink.height()) * Affine::translate(-ink.origin().to_vec2());
    let back = place * Affine::translate((x, y));
    runs.iter()
        .filter(|(_, p)| !p.elements().is_empty())
        .map(|(color, p)| {
            let bent = back * kition_office_core::warp::warp_path_adj(preset, adjust, w, h, &(fit * p));
            let mut contours: Vec<Vec<[f32; 2]>> = Vec::new();
            for el in bent.elements() {
                match *el {
                    PathEl::MoveTo(q) => contours.push(vec![[q.x as f32, q.y as f32]]),
                    PathEl::LineTo(q) => {
                        if let Some(c) = contours.last_mut() {
                            c.push([q.x as f32, q.y as f32]);
                        }
                    }
                    _ => {}
                }
            }
            (*color, contours)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_sheet_model::{Shape, ShapeKind};

    fn fixed(_: &ShapeTextRun, size: f32, text: &str) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }

    #[test]
    fn plain_text_wraps_at_spaces_and_between_east_asian_characters() {
        let sh = Shape {
            kind: ShapeKind::TextBox,
            text: "aaaa bbbb cccc\n\u{4E00}\u{4E01}\u{4E02}\u{4E03}\u{4E04}\u{4E05}\u{4E06}\u{4E07}".into(),
            ..Default::default()
        };
        let text = shape_text(&sh);
        // 11 pt is 14.67 px, a character 7.33 px: six characters fit in 50 px of room.
        let pieces = layout(&text, 50.0 + 2.0 * 9.6, 400.0, &fixed);
        let lines: Vec<&str> = pieces.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(lines, ["aaaa ", "bbbb ", "cccc", "\u{4E00}\u{4E01}\u{4E02}\u{4E03}\u{4E04}\u{4E05}", "\u{4E06}\u{4E07}"]);
        // A text box's text starts at its top left inset, one line under the other.
        assert!((pieces[0].x - 9.6).abs() < 1e-3);
        assert!(pieces[1].baseline > pieces[0].baseline);
    }

    #[test]
    fn a_shape_centres_its_plain_text() {
        let sh = Shape { kind: ShapeKind::Rectangle, text: "ab".into(), ..Default::default() };
        let text = shape_text(&sh);
        let pieces = layout(&text, 200.0, 100.0, &fixed);
        let width = 2.0 * 11.0 * PX_PER_PT * 0.5;
        assert!((pieces[0].x - (200.0 - width) / 2.0).abs() < 1e-3, "{pieces:?}");
        assert!(pieces[0].baseline > 40.0 && pieces[0].baseline < 60.0, "{pieces:?}");
    }
}
