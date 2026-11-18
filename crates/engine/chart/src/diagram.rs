//! Smart graphics as primitives: the drawing Office keeps beside the diagram's data, read by
//! the shared reader ([`kition_office_core::smartart`]) and turned into the sheet's primitives in
//! pixels. Pictures are listed apart for the caller to place (it decodes them).

use kition_office_core::kurbo::{BezPath, PathEl, Point};
use kition_office_core::model::Rgb;
use kition_office_core::smartart::{Item, Seg, TextAlign};
use kition_office_core::xml::parse;
use kition_sheet_model::{Diagram, Theme};

use crate::{HAlign, Paint, Prim, Rgba, VAlign};

/// Pixels per point (96 dpi).
const PX: f32 = 96.0 / 72.0;

/// A diagram drawn into its `w` × `h` pixel box.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiagramDrawing {
    pub prims: Vec<Prim>,
    /// Pictures: their box (`[x, y, w, h]`, pixels) and the index of their part in
    /// [`Diagram::drawing_parts`].
    pub images: Vec<([f32; 4], usize)>,
}

/// The sheet theme's colours in DrawingML slot order (dk1, lt1, dk2, lt2, accents, links).
fn theme_slots(theme: &Theme) -> [Rgb; 12] {
    let mut c = theme.colors;
    c.swap(0, 1);
    c.swap(2, 3);
    c.map(|v| Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn rgba(c: Rgb) -> Rgba {
    [c.0, c.1, c.2, 255]
}

/// Flattened contours of a path: points and whether the contour closes.
fn contours(path: &BezPath) -> Vec<(Vec<[f32; 2]>, bool)> {
    let mut out = Vec::new();
    let mut pts: Vec<[f32; 2]> = Vec::new();
    let mut els = Vec::new();
    kition_office_core::kurbo::flatten(path, 0.2, |el| els.push(el));
    for el in els.into_iter().take(1_000_000) {
        match el {
            PathEl::MoveTo(p) => {
                if pts.len() > 1 {
                    out.push((std::mem::take(&mut pts), false));
                }
                pts = vec![[p.x as f32, p.y as f32]];
            }
            PathEl::LineTo(p) => pts.push([p.x as f32, p.y as f32]),
            PathEl::ClosePath => {
                if pts.len() > 1 {
                    out.push((std::mem::take(&mut pts), true));
                }
                pts.clear();
            }
            _ => {}
        }
    }
    if pts.len() > 1 {
        out.push((pts, false));
    }
    out
}

/// A filled and stroked outline, already in pixels.
fn outline(out: &mut Vec<Prim>, path: &BezPath, fill: Option<Rgb>, stroke: Option<Rgb>, width: f32) {
    let parts = contours(path);
    if let Some(fill) = fill {
        let closed: Vec<Vec<[f32; 2]>> = parts.iter().filter(|(p, _)| p.len() > 2).map(|(p, _)| p.clone()).collect();
        if !closed.is_empty() {
            out.push(Prim::Path { contours: closed, paint: Paint::Solid(rgba(fill)) });
        }
    }
    if let Some(stroke) = stroke.filter(|_| width > 0.0) {
        for (mut pts, closed) in parts {
            if closed && let Some(first) = pts.first().copied() {
                pts.push(first);
            }
            out.push(Prim::Line { pts, color: rgba(stroke), width, dash: false });
        }
    }
}

/// Draws a diagram into a `w` × `h` pixel box. `prompts`: whether empty placeholder nodes show
/// their prompt ("[Text]"), as an editor does; printed copies leave them out.
pub fn render_diagram(d: &Diagram, theme: &Theme, w: f32, h: f32, prompts: bool) -> DiagramDrawing {
    let mut drawing = DiagramDrawing::default();
    let Some(part) = d.part("drawing") else { return drawing };
    let Ok(tree) = parse(&String::from_utf8_lossy(&part.data)) else { return drawing };
    let data = d.part("data").and_then(|p| parse(&String::from_utf8_lossy(&p.data)).ok());
    let slots = theme_slots(theme);
    let mut image = |rid: &str| d.drawing_parts.iter().position(|p| p.role == rid).map(|i| i.to_string());
    let items = kition_office_core::smartart::items(&tree.root, data.as_ref().map(|d| &d.root), &slots, w / PX, h / PX, &mut image);
    let px = |v: f32| v * PX;
    let out = &mut drawing.prims;
    for item in items {
        match item {
            Item::Preset { rect: [x, y, rw, rh], prst, fill, stroke, stroke_width } => {
                let Some(g) = kition_office_core::preset::build(&prst, f64::from(px(rw)), f64::from(px(rh)), &[]) else { continue };
                let shift = kition_office_core::kurbo::Affine::translate((f64::from(px(x)), f64::from(px(y))));
                for sub in &g.paths {
                    let fill = fill.filter(|_| !matches!(sub.fill, kition_office_core::preset::FillMode::None));
                    let stroke = stroke.filter(|_| sub.stroke);
                    outline(out, &(shift * sub.path.clone()), fill, stroke, px(stroke_width));
                }
            }
            Item::Path { segs, fill, stroke, stroke_width } => {
                let mut path = BezPath::new();
                let p = |x: f32, y: f32| Point::new(f64::from(px(x)), f64::from(px(y)));
                for seg in segs {
                    match seg {
                        Seg::Move(x, y) => path.move_to(p(x, y)),
                        Seg::Line(x, y) => path.line_to(p(x, y)),
                        Seg::Cubic(x1, y1, x2, y2, x, y) => path.curve_to(p(x1, y1), p(x2, y2), p(x, y)),
                        Seg::Close => path.close_path(),
                    }
                }
                outline(out, &path, fill, stroke, px(stroke_width));
            }
            Item::Text { rect: [x, y, rw, rh], text, size, color, bold, align, prompt, .. } => {
                if prompt && !prompts {
                    continue;
                }
                let (ax, halign) = match align {
                    TextAlign::Left => (x, HAlign::Left),
                    TextAlign::Center => (x + rw / 2.0, HAlign::Center),
                    TextAlign::Right => (x + rw, HAlign::Right),
                };
                out.push(Prim::Text {
                    x: px(ax),
                    y: px(y + rh / 2.0),
                    text,
                    size: px(size),
                    color: rgba(color),
                    bold,
                    align: halign,
                    valign: VAlign::Middle,
                    rotation: 0.0,
                });
            }
            Item::Image { rect, media } => {
                if let Ok(i) = media.parse() {
                    drawing.images.push((rect.map(px), i));
                }
            }
        }
    }
    drawing
}

#[cfg(test)]
mod tests {
    use kition_sheet_model::{Anchor, DiagramPart};

    use super::*;

    const DRAWING: &str = r#"<dsp:drawing xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><dsp:spTree><dsp:sp modelId="{S}"><dsp:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></dsp:spPr><dsp:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="2000" b="1"/><a:t>Plan</a:t></a:r></a:p><a:p><a:endParaRPr sz="1000"/></a:p></dsp:txBody></dsp:sp><dsp:sp modelId="{P}"><dsp:spPr><a:xfrm><a:off x="1270000" y="0"/><a:ext cx="635000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:blipFill><a:blip r:embed="rId1"/></a:blipFill></dsp:spPr></dsp:sp></dsp:spTree></dsp:drawing>"#;
    const DATA: &str = r#"<dgm:dataModel xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:ptLst><dgm:pt modelId="{B}"><dgm:prSet phldrT="[Text]" phldr="1"/></dgm:pt></dgm:ptLst><dgm:cxnLst><dgm:cxn modelId="{1}" type="presOf" srcId="{B}" destId="{S}" srcOrd="0" destOrd="1"/></dgm:cxnLst></dgm:dataModel>"#;

    fn part(role: &str, data: &str) -> DiagramPart {
        DiagramPart { role: role.into(), rel_type: String::new(), content_type: String::new(), ext: "xml".into(), data: data.as_bytes().to_vec() }
    }

    fn diagram() -> Diagram {
        Diagram {
            id: 1,
            anchor: Anchor::default(),
            name: String::new(),
            parts: vec![part("data", DATA), part("drawing", DRAWING)],
            drawing_parts: vec![DiagramPart { role: "rId1".into(), ..part("", "picture") }],
        }
    }

    fn texts(d: &DiagramDrawing) -> Vec<(&str, f32, bool)> {
        d.prims.iter().filter_map(|p| if let Prim::Text { text, size, bold, .. } = p { Some((text.as_str(), *size, *bold)) } else { None }).collect()
    }

    #[test]
    fn a_diagram_draws_its_kept_drawing_in_the_sheet_theme() {
        let theme = Theme::default();
        let d = render_diagram(&diagram(), &theme, 400.0, 400.0, true);
        // The rounded box in the theme's first accent, at its size in pixels.
        let accent = theme.colors[4];
        let fill = [(accent >> 16) as u8, (accent >> 8) as u8, accent as u8, 255];
        let Some(Prim::Path { contours, paint }) = d.prims.first() else { panic!("{:?}", d.prims) };
        assert_eq!(*paint, Paint::Solid(fill));
        let right = contours[0].iter().map(|p| p[0]).fold(0.0, f32::max);
        assert!((right - 100.0 * PX).abs() < 0.5, "{right}");
        // Its text at 20 points in pixels, and the empty placeholder's prompt in the editor.
        assert_eq!(texts(&d), [("Plan", 20.0 * PX, true), ("[Text]", 10.0 * PX, false)]);
        // The picture's box, by its part.
        assert_eq!(d.images.len(), 1);
        let ([x, _, w, _], i) = d.images[0];
        assert_eq!(i, 0);
        assert!((x - 100.0 * PX).abs() < 0.5 && (w - 50.0 * PX).abs() < 0.5, "{:?}", d.images);
        // Printed, the prompt is left out.
        assert_eq!(texts(&render_diagram(&diagram(), &theme, 400.0, 400.0, false)), [("Plan", 20.0 * PX, true)]);
    }
}
