//! `drawing.ink` and `drawing.erase`: the Kit Harness Protocol's `KitDrawing 1.2` ink over the
//! active sheet's ink strokes (an `Ink` shape per stroke, `draw.rs`), through the ink contract
//! Kition's kits share (`kition_office_core::ink`). Points are sheet points; each command is one undo
//! step.

use kition_office_core::ink::{self, InkStroke, InkTool};
use kition_office_core::model as dm;
use kition_sheet_model::{Color, Shape, ShapeKind, Sheet};
use serde_json::{Value as Json, json};

use super::*;

/// How far around the eraser's path it takes ink, points.
pub const ERASER_RADIUS: f64 = 4.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "drawing.ink",
            "Draw Ink Strokes",
            [],
            None,
            "{strokes: [{tool?: pen|pencil|highlighter, color?, width?, points: [[x, y, pressure?], …]}]} (sheet points; members left out take the current pen)",
            has_doc,
            add
        ),
        cmd!("drawing.erase", "Erase Ink", [], None, "{path: [[x, y], …] | x, y, radius?} (deletes the strokes the path touches)", has_doc, erase),
    ]
}

fn fail(cmd: &str, e: kition_office_core::ops::DrawingError) -> EngineError {
    let property = e.property.as_deref().map(|p| format!(" ({p})")).unwrap_or_default();
    bad(cmd, format!("{}{property}: {}", e.code, e.message))
}

fn tool_name(t: InkTool) -> &'static str {
    match t {
        InkTool::Pen => "pen",
        InkTool::Pencil => "pencil",
        InkTool::Highlighter => "highlighter",
    }
}

/// An ink stroke shape of `sheet`, as a shared stroke in sheet points.
pub fn stroke_of(sheet: &Sheet, sh: &Shape) -> InkStroke {
    let (x, y) = (sheet.col_left(sh.anchor.cell.col) + f64::from(sh.anchor.dx), sheet.row_top(sh.anchor.cell.row) + f64::from(sh.anchor.dy));
    let color = match sh.line {
        Color::Rgb(v) => Some(dm::Color::rgb(dm::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))),
        _ => None,
    };
    InkStroke {
        tool: InkTool::parse(super::draw::ink_tool(sh)).unwrap_or_default(),
        color,
        width: f64::from(super::draw::ink_width(sh)),
        points: super::draw::ink_points(sh).into_iter().map(|[px, py]| [x + f64::from(px), y + f64::from(py), 0.5]).collect(),
    }
}

fn current_pen(s: &Session) -> InkStroke {
    let state = super::draw::draw_state(s);
    let tool = state["tool"].as_str().and_then(InkTool::parse).unwrap_or_default();
    let color = state["color"].as_str().and_then(|c| dm::Color::from_value(&json!(c)));
    InkStroke { tool, color, width: state["width"].as_f64().unwrap_or(2.0), points: Vec::new() }
}

fn ink_ids(s: &Session) -> Vec<u32> {
    s.doc()
        .ok()
        .and_then(|d| d.wb.active().map(|sh| sh.shapes.iter().filter(|x| x.kind == ShapeKind::Ink).map(|x| x.id).collect()))
        .unwrap_or_default()
}

fn add(s: &mut Session, p: &Json) -> Result<Json> {
    let strokes = ink::parse_strokes(p, &current_pen(s)).map_err(|e| fail("drawing.ink", e))?;
    let mut calls = Vec::with_capacity(strokes.len());
    for st in &strokes {
        let color = match st.color.as_ref().map(|c| &c.base) {
            Some(dm::ColorBase::Rgb { rgb }) => format!("#{:02X}{:02X}{:02X}", rgb.0, rgb.1, rgb.2),
            Some(dm::ColorBase::Theme { .. }) => {
                return Err(fail("drawing.ink", kition_office_core::ops::DrawingError::unsupported("color", "ink takes an RGB colour")));
            }
            None => super::draw::draw_state(s)["color"].as_str().unwrap_or("#1F5FC9").to_string(),
        };
        // A dot is a stroke too: two points at one place.
        let mut points: Vec<Json> = st.points.iter().map(|q| json!([q[0], q[1]])).collect();
        if points.len() == 1 {
            points.push(points[0].clone());
        }
        calls.push(json!({ "points": points, "color": color, "width": st.width, "tool": tool_name(st.tool) }));
    }
    let before = ink_ids(s);
    for call in calls {
        s.execute("draw.stroke", call)?;
    }
    let ids: Vec<String> = ink_ids(s).into_iter().filter(|id| !before.contains(id)).map(|id| id.to_string()).collect();
    Ok(json!({ "ids": ids }))
}

fn erase(s: &mut Session, p: &Json) -> Result<Json> {
    let gesture = ink::parse_eraser(p, ERASER_RADIUS).map_err(|e| fail("drawing.erase", e))?;
    let d = s.doc()?;
    let si = d.wb.active_sheet;
    let sheet = d.wb.active().ok_or(EngineError::NoDocument)?;
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    let inks: Vec<&Shape> = sheet.shapes.iter().filter(|x| x.kind == ShapeKind::Ink).collect();
    let strokes: Vec<InkStroke> = inks.iter().map(|x| stroke_of(sheet, x)).collect();
    let hit: Vec<u32> = gesture.hits(&strokes).into_iter().filter_map(|i| inks.get(i).map(|x| x.id)).collect();
    if hit.is_empty() {
        return Ok(json!({ "erased": 0, "ids": [] }));
    }
    edit(s, |cx| {
        cx.sheet_mut(si)?.shapes.retain(|x| !hit.contains(&x.id));
        Ok(())
    })?;
    if s.selected_shape.is_some_and(|id| hit.contains(&id)) {
        s.selected_shape = None;
    }
    Ok(json!({ "erased": hit.len(), "ids": hit.iter().map(u32::to_string).collect::<Vec<_>>() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_office_core::model::DrawingObject;

    fn inks(s: &mut Session) -> Vec<DrawingObject> {
        let ids: Vec<String> = s.doc().unwrap().wb.active().unwrap().shapes.iter().map(|x| x.id.to_string()).collect();
        if ids.is_empty() {
            return Vec::new();
        }
        let d = s.execute("drawing.describe", json!({ "ids": ids })).unwrap();
        serde_json::from_value::<Vec<DrawingObject>>(d["objects"].clone()).unwrap().into_iter().filter(|o| o.kind == dm::ObjectKind::Ink).collect()
    }

    #[test]
    fn strokes_draw_describe_undo_erase_and_reopen_as_xlsx() {
        let mut s = Session::new();
        s.new_workbook();
        let r = s
            .execute(
                "drawing.ink",
                json!({ "strokes": [
                    { "points": [[100, 100], [200, 100]], "color": "#C00000", "width": 1.5 },
                    { "tool": "highlighter", "points": [[100, 160], [200, 160]] },
                ] }),
            )
            .unwrap();
        let ids: Vec<String> = serde_json::from_value(r["ids"].clone()).unwrap();
        assert_eq!(ids.len(), 2);
        let objects = inks(&mut s);
        let pen = objects.iter().find(|o| o.ink.as_ref().unwrap()[0].tool == InkTool::Pen).unwrap();
        let st = &pen.ink.as_ref().unwrap()[0];
        assert_eq!(st.width, 1.5, "the width exactly");
        assert!(
            (st.points[0][0] - 100.0).abs() < 0.2 && (st.points[1][0] - 200.0).abs() < 0.2 && (st.points[0][1] - 100.0).abs() < 0.2,
            "{:?}",
            st.points
        );
        assert_eq!(st.color, Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0))));
        let hl = objects.iter().find(|o| o.ink.as_ref().unwrap()[0].tool == InkTool::Highlighter).unwrap();
        assert_eq!(hl.ink.as_ref().unwrap()[0].width, 2.0, "the current pen's width for a stroke without its own");
        // Ink moves, but keeps its size and look.
        s.execute("drawing.transform", json!({ "ids": [pen.id], "dx": 10 })).unwrap();
        assert!(s.execute("drawing.transform", json!({ "ids": [pen.id], "rotateBy": 90 })).is_err());
        assert!(s.execute("drawing.format", json!({ "ids": [pen.id], "fill": { "color": "#00FF00" } })).is_err());
        // The eraser takes the stroke its path crosses, and only it.
        let r = s.execute("drawing.erase", json!({ "path": [[160, 80], [160, 120]] })).unwrap();
        assert_eq!(r["erased"], 1);
        let left = inks(&mut s);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].ink.as_ref().unwrap()[0].tool, InkTool::Highlighter);
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(inks(&mut s).len(), 2, "undo brings it back");

        let b64 = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap()["base64"].as_str().unwrap().to_string();
        let mut back = Session::new();
        back.execute("file.open", json!({"name": "i.xlsx", "base64": b64})).unwrap();
        let reopened = inks(&mut back);
        assert_eq!(reopened.len(), 2, "the strokes reopen as ink, not as shapes holding their points as text");
        let pen = reopened.iter().find(|o| o.ink.as_ref().unwrap()[0].tool == InkTool::Pen).unwrap();
        let st = &pen.ink.as_ref().unwrap()[0];
        assert_eq!((st.width, st.color.clone()), (1.5, Some(dm::Color::rgb(dm::Rgb(0xC0, 0, 0)))));
        assert!((st.points[0][0] - 110.0).abs() < 0.5 && (st.points[1][0] - 210.0).abs() < 0.5, "{:?}", st.points);
        assert!(reopened.iter().any(|o| o.ink.as_ref().unwrap()[0].tool == InkTool::Highlighter), "a highlighter stays one");
    }

    #[test]
    fn pens_are_picked_up_with_their_own_look_and_bad_strokes_change_nothing() {
        let mut s = Session::new();
        s.new_workbook();
        let st = s.execute("draw.highlighter", json!({})).unwrap();
        assert_eq!((st["tool"].as_str(), st["width"].as_f64(), st["color"].as_str()), (Some("highlighter"), Some(12.0), Some("#FFFF00")));
        // Picking up the same pen again keeps it in hand.
        assert_eq!(s.execute("draw.highlighter", json!({ "width": 8 })).unwrap()["tool"], "highlighter");
        assert_eq!(s.execute("draw.state", json!({})).unwrap()["width"], 8.0);
        assert_eq!(s.execute("draw.select", json!({})).unwrap()["tool"], "select");
        assert!(s.execute("drawing.ink", json!({ "strokes": [{ "points": [] }] })).is_err());
        assert!(inks(&mut s).is_empty());
        assert_eq!(s.execute("drawing.erase", json!({ "x": 5, "y": 5 })).unwrap()["erased"], 0);
    }
}
