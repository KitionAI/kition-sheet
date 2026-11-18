//! Draw tab: pen and eraser tools, ink strokes, Ink to Shape.

use kition_sheet_core::CellRef;
use kition_sheet_model::*;
use serde_json::{Value as Json, json};

use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "draw.pen", "Pen", ["Draw", "Tools"], None, "{on?: bool (default true), color?: hex, width?: 2}", has_doc, |s, p| pen(s, p, "pen")),
        cmd!(noundo "draw.pencil", "Pencil", ["Draw", "Tools"], None, "{on?: bool (default true), color?: hex, width?: 1}", has_doc, |s, p| pen(s, p, "pencil")),
        cmd!(noundo "draw.highlighter", "Highlighter", ["Draw", "Tools"], None, "{on?: bool (default true), color?: hex, width?: 12}", has_doc, |s, p| pen(
            s,
            p,
            "highlighter"
        )),
        cmd!(noundo "draw.eraser", "Eraser", ["Draw", "Tools"], None, "{on?: bool (default true)}", has_doc, eraser),
        cmd!(noundo "draw.select", "Select", ["Draw", "Tools"], None, "{}", has_doc, |s, _| {
            s.draw_tool = String::new();
            s.draw_shape = None;
            Ok(draw_state(s))
        }),
        // Insert › Shapes: the next drag on the sheet draws this shape (a line between two shapes
        // glues to them); `null` puts it down.
        cmd!(noundo "draw.shape", "Draw Shape", [], None, "{preset: DrawingML preset | null}", has_doc, |s, p| {
            s.draw_shape = match p.get("preset").and_then(Json::as_str) {
                Some(name) if kition_office_core::preset::info(name).is_some() || kition_office_core::connect::is_line_preset(name) => {
                    s.draw_tool = String::new();
                    Some(name.to_string())
                }
                Some(name) => return Err(bad("draw.shape", format!("preset `{name}` is not drawn"))),
                None => None,
            };
            Ok(json!({ "shape": s.draw_shape }))
        }),
        // The pen in use, as the shared Draw controls show it (the same in every kit).
        cmd!(query "draw.state", "Draw State", [], None, "{}", has_doc, |s, _| Ok(draw_state(s))),
        cmd!(noundo "draw.lasso", "Lasso Select", ["Draw", "Tools"], None, "{} (selects ink by dragging; same as Select Objects)", has_doc, |s, _| {
            s.draw_tool = String::new();
            ok()
        }),
        cmd!("draw.stroke", "Ink Stroke", [], None, "{points: [[x,y],…] in sheet points, color?: hex, width?: 2}", has_doc, stroke),
        cmd!(
            "draw.inkToShape",
            "Ink to Shape",
            ["Draw", "Convert"],
            None,
            "{id?} converts an ink stroke (default: the last) into a rectangle, ellipse, triangle or line",
            has_doc,
            ink_to_shape
        ),
    ]
}

/// A pen's colour and width when picked up without them.
fn pen_defaults(tool: &str) -> (&'static str, f32) {
    match tool {
        "highlighter" => ("#FFFF00", 12.0),
        "pencil" => ("#595959", 1.0),
        _ => ("#1F5FC9", 2.0),
    }
}

/// Picks up pen `tool` (`on: false` puts it down), with its colour and width.
fn pen(s: &mut Session, p: &Json, tool: &str) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(true);
    if !on {
        if s.draw_tool == tool {
            s.draw_tool = String::new();
        }
        return Ok(draw_state(s));
    }
    // Another pen starts with its own look; the same pen keeps what was set.
    if s.draw_tool != tool {
        let (color, width) = pen_defaults(tool);
        s.draw_color = color.into();
        s.draw_width = width;
    }
    s.draw_tool = tool.into();
    if let Some(c) = str_param(p, "color") {
        s.draw_color = c.to_string();
    }
    if let Some(w) = f64_param(p, "width") {
        s.draw_width = w.clamp(0.25, 72.0) as f32;
    }
    Ok(draw_state(s))
}

fn eraser(s: &mut Session, p: &Json) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(true);
    s.draw_tool = if on {
        "eraser".into()
    } else if s.draw_tool == "eraser" {
        String::new()
    } else {
        s.draw_tool.clone()
    };
    Ok(draw_state(s))
}

/// The pen in use: `{tool, color, width}` (`tool`: select, pen, pencil, highlighter or eraser).
pub fn draw_state(s: &Session) -> Json {
    let tool = if s.draw_tool.is_empty() { "select" } else { s.draw_tool.as_str() };
    let (color, width) = pen_defaults(tool);
    let color = if s.draw_color.is_empty() { color.to_string() } else { s.draw_color.clone() };
    let width = if s.draw_width > 0.0 { s.draw_width } else { width };
    json!({ "tool": tool, "color": color, "width": width })
}

fn parse_points(t: &str) -> Vec<[f32; 2]> {
    t.split(';').filter_map(|pt| pt.split_once(',').and_then(|(x, y)| Some([x.parse().ok()?, y.parse().ok()?]))).collect()
}

fn stroke(s: &mut Session, p: &Json) -> Result<Json> {
    let pts: Vec<[f64; 2]> = p
        .get("points")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|q| Some([q.get(0)?.as_f64()?, q.get(1)?.as_f64()?]))
                .filter(|q| q[0].is_finite() && q[1].is_finite())
                .take(20_000)
                .collect()
        })
        .unwrap_or_default();
    if pts.len() < 2 {
        return Err(bad("draw.stroke", "a stroke needs at least two points"));
    }
    let minx = pts.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min).max(0.0);
    let miny = pts.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min).max(0.0);
    let maxx = pts.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
    let maxy = pts.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max);
    let color =
        str_param(p, "color").map(str::to_string).unwrap_or_else(|| if s.draw_color.is_empty() { "#1F5FC9".into() } else { s.draw_color.clone() });
    let width = f64_param(p, "width").map(|w| w as f32).unwrap_or(if s.draw_width <= 0.0 { 2.0 } else { s.draw_width }).clamp(0.25, 72.0);
    // A pencil's and a highlighter's ink is translucent; the alpha names the pen on reading.
    let tool = str_param(p, "tool").unwrap_or(if s.draw_tool.is_empty() || s.draw_tool == "eraser" { "pen" } else { s.draw_tool.as_str() });
    let alpha = match tool {
        "highlighter" => Some(50_000),
        "pencil" => Some(85_000),
        _ => None,
    };
    let d = s.doc()?;
    let si = d.wb.active_sheet;
    let sh = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    if sh.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    let cell = CellRef::new(sh.row_at(miny), sh.col_at(minx));
    let dx = (minx - sh.col_left(cell.col)) as f32;
    let dy = (miny - sh.row_top(cell.row)) as f32;
    let text = pts.iter().map(|q| format!("{:.1},{:.1}", q[0] - minx, q[1] - miny)).collect::<Vec<_>>().join(";");
    let id = next_object_id(&d.wb)?;
    let shape = Shape {
        id,
        kind: ShapeKind::Ink,
        anchor: Anchor {
            cell,
            dx,
            dy,
            width: (maxx - minx).max(1.0) as f32,
            height: (maxy - miny).max(1.0) as f32,
            // Ink is a freehand mark: pinned where it was drawn, it does not follow cells.
            mode: kition_sheet_model::AnchorMode::Absolute,
            one_cell: false,
        },
        fill: Color::from_hex(&format!("#{:02X}0000", width.round().clamp(0.0, 255.0) as u8)).unwrap_or_default(),
        line: Color::from_hex(&color).unwrap_or(Color::rgb(0x1F, 0x5F, 0xC9)),
        // The width exactly (the fill's red channel keeps a rounded one for older readers).
        outline: Outline { width: Some((width * 12_700.0).round() as u32), alpha, cap: Some("rnd".into()), ..Default::default() },
        text,
        ..Default::default()
    };
    edit(s, |cx| {
        cx.sheet_mut(si)?.shapes.push(shape);
        Ok(json!({"shape": id}))
    })
}

/// An ink stroke's width: its outline's (strokes drawn since `KitDrawing 1.2`), else the red
/// channel of its `fill` where earlier ones kept it rounded.
pub fn ink_width(sh: &Shape) -> f32 {
    if let Some(w) = sh.outline.width {
        return w as f32 / 12_700.0;
    }
    match sh.fill {
        Color::Rgb(v) => ((v >> 16) & 0xFF).max(1) as f32,
        _ => 2.0,
    }
}

/// The pen that drew an ink stroke, from its outline's alpha.
pub fn ink_tool(sh: &Shape) -> &'static str {
    match sh.outline.alpha {
        Some(a) if a <= 60_000 => "highlighter",
        Some(a) if a < 100_000 => "pencil",
        _ => "pen",
    }
}

pub fn ink_points(sh: &Shape) -> Vec<[f32; 2]> {
    parse_points(&sh.text)
}

fn ink_to_shape(s: &mut Session, p: &Json) -> Result<Json> {
    let si = s.doc()?.wb.active_sheet;
    let id = u32_param(p, "id");
    edit(s, |cx| {
        let sh = cx.sheet_mut(si)?;
        if sh.is_protected() {
            return Err(EngineError::Other(PROTECTED.into()));
        }
        let idx = match id {
            Some(id) => sh.shapes.iter().position(|x| x.id == id && x.kind == ShapeKind::Ink),
            None => sh.shapes.iter().rposition(|x| x.kind == ShapeKind::Ink),
        }
        .ok_or_else(|| bad("draw.inkToShape", "no ink stroke to convert"))?;
        let ink = sh.shapes.get_mut(idx).ok_or_else(|| bad("draw.inkToShape", "gone"))?;
        let pts = parse_points(&ink.text);
        let (w, h) = (ink.anchor.width.max(1.0), ink.anchor.height.max(1.0));
        let closed = match (pts.first(), pts.last()) {
            (Some(a), Some(b)) => ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < 0.25 * w.max(h),
            _ => false,
        };
        let kind = if !closed {
            ShapeKind::Line
        } else {
            // Corners hugging the bounding box mean a rectangle; one apex at the top a triangle.
            let near = |cx: f32, cy: f32| pts.iter().any(|q| ((q[0] - cx).powi(2) + (q[1] - cy).powi(2)).sqrt() < 0.15 * w.min(h).max(8.0));
            let corners = [near(0.0, 0.0), near(w, 0.0), near(0.0, h), near(w, h)].iter().filter(|b| **b).count();
            let apex = near(w / 2.0, 0.0);
            if corners >= 3 {
                ShapeKind::Rectangle
            } else if apex && near(0.0, h) && near(w, h) {
                ShapeKind::Triangle
            } else {
                ShapeKind::Ellipse
            }
        };
        let color = ink.line;
        ink.kind = kind;
        ink.fill = if kind == ShapeKind::Line { color } else { Color::Auto };
        ink.text = String::new();
        if kind == ShapeKind::Line && pts.first().zip(pts.last()).is_some_and(|(a, b)| a[1] > b[1]) {
            // Lines are drawn top-left to bottom-right; keep the stroke direction simple.
        }
        Ok(json!({"kind": format!("{kind:?}")}))
    })
}
