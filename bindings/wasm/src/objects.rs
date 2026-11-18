//! Drawing objects under the select tool: a press on a shape selects it, a drag moves it, its
//! handles resize and turn it, its adjust handles (the yellow dots) reshape it (through the
//! gestures Kition's kits share, `kition_office_core::edit`), and typing on a selected shape edits its
//! text. A gesture changes nothing while it runs: its preview is an overlay, and the release runs
//! one `drawing.transform` or, for an adjust handle, one `drawing.format` (one undo step); Escape
//! drops it. The shape's text being typed is drawn with the shape, as the cell being typed is.

use kition_kit::tools::{PointerAction, ToolMods, ToolOverlay, ToolPointer};
use kition_office_core::edit::{Gesture, HandleHit, Mods, Preview, adjust_handles, box_handles, hit_adjust, hit_handle};
use kition_office_core::ink::{EraserGesture, InkGesture, InkTool};
use kition_office_core::model::{DrawingObject, ObjectKind};
use kition_office_core::ops::{DrawingOp, FrameChange};
use kition_office_core::{Point, Xfrm};
use serde_json::json;

use crate::{ACCENT, HEADER_H, HEADER_W, SheetDocument, engine_error};

/// Handle hit radius in screen pixels.
const REACH_PX: f64 = 4.0;
const HANDLE: [u8; 4] = [255, 255, 255, 255];
/// Adjust handles are yellow, as in Office.
const ADJUST: [u8; 4] = [255, 214, 0, 255];

/// The text of a shape being typed, before it is set.
pub(crate) struct ShapeEdit {
    pub id: u32,
    pub text: String,
}

/// A gesture under way and its latest preview.
/// A pen stroke or an eraser drag under way (`KitDrawing 1.2`, DR20): previewed while the pointer
/// moves, one `drawing.ink` or `drawing.erase` on release, nothing on Escape.
pub(crate) enum InkDrag {
    Pen { gesture: InkGesture, zoom: f64 },
    Eraser { gesture: EraserGesture },
}

pub(crate) struct ObjectDrag {
    gesture: Gesture,
    preview: Preview,
    /// A connector's end being dragged (instead of `gesture`), and its frame now.
    end: Option<(kition_office_core::connect::EndDrag, Xfrm)>,
    /// A shape from Insert › Shapes being drawn (instead of `gesture`), and its frame now.
    create: Option<(kition_office_core::connect::CreateDrag, Xfrm)>,
}

fn sheet_point(x: f64, y: f64) -> Point {
    Point::new(x - HEADER_W, y - HEADER_H)
}

fn canvas(p: Point) -> [f64; 2] {
    [p.x + HEADER_W, p.y + HEADER_H]
}

impl SheetDocument {
    /// The active sheet's drawing objects (shapes, then the charts over them), bottom to top, as
    /// the shared model describes them.
    pub(crate) fn drawing_objects(&self) -> Vec<DrawingObject> {
        let Ok(doc) = self.session.doc() else { return Vec::new() };
        kition_sheet_engine::cmd::drawing::objects(&doc.wb)
    }

    fn selected_object(&self) -> Option<DrawingObject> {
        let id = self.session.selected_shape?.to_string();
        self.drawing_objects().into_iter().find(|o| o.id == id)
    }

    /// Sets the shape being typed, entering what was typed into the shape before.
    pub(crate) fn finish_shape_text(&mut self) -> Result<(), String> {
        if let Some(edit) = self.shape_edit.take() {
            self.edits += 1;
            let unchanged =
                self.drawing_objects().iter().any(|o| o.id == edit.id.to_string() && o.text.as_ref().is_some_and(|t| t.text() == edit.text));
            if !unchanged {
                self.session.execute("drawing.text", json!({ "id": edit.id.to_string(), "text": edit.text })).map_err(engine_error)?;
            }
        }
        Ok(())
    }

    /// A pointer event for drawing objects; `true` when it was theirs.
    pub(crate) fn object_pointer(&mut self, event: &ToolPointer) -> Result<bool, String> {
        let p = sheet_point(event.x, event.y);
        if let Some(handled) = self.ink_pointer(event, p)? {
            return Ok(handled);
        }
        let tol = REACH_PX / event.zoom.max(0.05);
        let mods = Mods { shift: event.mods.shift, alt: event.mods.alt };
        match event.action {
            PointerAction::Down => {
                // A selected shape's adjust handles, then its other handles, then the topmost shape
                // under the pointer.
                // A shape chosen in Insert › Shapes: this drag draws it.
                if let Some(preset) = self.session.draw_shape.clone() {
                    self.finish_shape_text()?;
                    let drag = kition_office_core::connect::CreateDrag::start(&preset, p, &self.drawing_objects(), tol * 1.5);
                    let frame = drag.frame();
                    self.object_drag = Some(ObjectDrag {
                        gesture: Gesture::moving(Vec::new(), p),
                        preview: Preview { frames: Vec::new() },
                        end: None,
                        create: Some((drag, frame)),
                    });
                    return Ok(true);
                }
                // A selected connector's end first: it drags onto another shape's site, or off.
                if let Some(o) = self.selected_object()
                    && let Some(end) = kition_office_core::connect::hit_end(&o, p, tol)
                {
                    self.finish_shape_text()?;
                    let drag = kition_office_core::connect::EndDrag::start(&o, end, &self.drawing_objects());
                    let frame = drag.frame();
                    self.object_drag = Some(ObjectDrag {
                        gesture: Gesture::moving(Vec::new(), p),
                        preview: Preview { frames: Vec::new() },
                        end: Some((drag, frame)),
                        create: None,
                    });
                    return Ok(true);
                }
                if let Some(o) = self.selected_object().filter(|o| matches!(o.kind, ObjectKind::Shape | ObjectKind::TextBox))
                    && let Some(index) = hit_adjust(&o.frame, &o.geometry, p, tol)
                {
                    self.finish_shape_text()?;
                    let gesture = Gesture::adjusting(o.id.clone(), o.frame, o.geometry.clone(), index);
                    self.object_drag = Some(ObjectDrag { gesture, preview: Preview { frames: Vec::new() }, end: None, create: None });
                    return Ok(true);
                }
                if let Some(o) = self.selected_object().filter(|o| o.kind != ObjectKind::Ink)
                    && let Some(h) = hit_handle(&o.frame, p, tol, !matches!(o.kind, ObjectKind::Chart | ObjectKind::Ink | ObjectKind::Group))
                {
                    self.finish_shape_text()?;
                    let gesture = match h {
                        HandleHit::Resize(i) => Gesture::resizing(o.id.clone(), i, o.frame, p),
                        HandleHit::Rotate => Gesture::rotating(vec![(o.id.clone(), o.frame)], &o.frame, p),
                    };
                    self.object_drag = Some(ObjectDrag { gesture, preview: Preview { frames: Vec::new() }, end: None, create: None });
                    return Ok(true);
                }
                let objects = self.drawing_objects();
                let Some(hit) = kition_office_core::edit::hit_topmost(&objects, p, tol) else {
                    self.also_selected.clear();
                    self.finish_shape_text()?;
                    if self.session.selected_shape.take().is_some() {
                        self.edits += 1;
                    }
                    return Ok(false);
                };
                let id: u32 = hit.id.parse().map_err(|_| "a shape id is a number".to_string())?;
                // A click in the shape being typed keeps typing.
                if self.shape_edit.as_ref().is_some_and(|e| e.id == id) {
                    return Ok(true);
                }
                // Shift adds the object to the selection (for Group); it does not drag.
                if event.mods.shift && self.session.selected_shape.is_some_and(|s| s != id) {
                    if !self.also_selected.contains(&id) {
                        self.also_selected.push(id);
                        self.edits += 1;
                    }
                    return Ok(true);
                }
                self.also_selected.clear();
                self.finish_shape_text()?;
                self.commit()?;
                self.session.selected_shape = Some(id);
                self.edits += 1;
                self.object_drag = Some(ObjectDrag {
                    gesture: Gesture::moving(vec![(hit.id.clone(), hit.frame)], p),
                    preview: Preview { frames: Vec::new() },
                    end: None,
                    create: None,
                });
                Ok(true)
            }
            PointerAction::Drag => {
                let Some(drag) = self.object_drag.as_mut() else { return Ok(false) };
                if let Some((end, frame)) = drag.end.as_mut() {
                    *frame = end.drag(p, tol * 1.5).0;
                } else if let Some((create, frame)) = drag.create.as_mut() {
                    *frame = create.drag(p, tol * 1.5, mods.shift);
                } else {
                    drag.preview = drag.gesture.drag(p, mods, tol, None);
                }
                self.edits += 1;
                Ok(true)
            }
            PointerAction::Up => {
                let Some(drag) = self.object_drag.take() else { return Ok(false) };
                self.edits += 1;
                if let Some((end, _)) = &drag.end {
                    self.session.execute("drawing.reconnect", end.release()).map_err(engine_error)?;
                    return Ok(true);
                }
                if let Some((create, _)) = &drag.create {
                    self.session.draw_shape = None;
                    let (command, params, ends) = create.release();
                    let made = self.session.execute(command, params).map_err(engine_error)?;
                    let id = made.get("id").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
                    for (end, at) in ends {
                        let which = if end == 0 { "start" } else { "end" };
                        self.session
                            .execute("drawing.reconnect", json!({ "id": id, "end": which, "to": at.id, "site": at.site }))
                            .map_err(engine_error)?;
                    }
                    // The new object is selected.
                    self.session.selected_shape = id.parse().ok();
                    return Ok(true);
                }
                match drag.gesture.release(&drag.preview) {
                    Some(DrawingOp::Transform(FrameChange::Set(frames))) => {
                        if let Some((id, frame)) = frames.first() {
                            self.session.execute("drawing.transform", json!({ "ids": [id], "frame": frame })).map_err(engine_error)?;
                        }
                    }
                    Some(DrawingOp::Format { ids, format }) => {
                        self.session.execute("drawing.format", json!({ "ids": ids, "geometry": format.geometry })).map_err(engine_error)?;
                    }
                    _ => {}
                }
                Ok(true)
            }
            PointerAction::DoubleClick => {
                let objects = self.drawing_objects();
                let Some(hit) = kition_office_core::edit::hit_topmost(&objects, p, tol) else { return Ok(false) };
                // A chart has no text of a shape's: its titles are set in Chart Design.
                if matches!(hit.kind, ObjectKind::Chart | ObjectKind::Ink) {
                    return Ok(true);
                }
                let id: u32 = hit.id.parse().map_err(|_| "a shape id is a number".to_string())?;
                self.session.selected_shape = Some(id);
                if self.shape_edit.is_none() {
                    self.shape_edit = Some(ShapeEdit { id, text: hit.text.as_ref().map(|t| t.text()).unwrap_or_default() });
                }
                self.edits += 1;
                Ok(true)
            }
            PointerAction::Move => Ok(false),
        }
    }

    /// A key for drawing objects; `true` when it was theirs.
    /// A pointer event while the Draw tab holds a pen or the eraser: it draws or erases ink
    /// instead of selecting. `None` when no pen or eraser is in use.
    fn ink_pointer(&mut self, event: &ToolPointer, p: Point) -> Result<Option<bool>, String> {
        let tool = self.session.draw_tool.clone();
        let pen = InkTool::parse(&tool);
        if pen.is_none() && tool != "eraser" && self.ink_drag.is_none() {
            return Ok(None);
        }
        let pressure = f64::from(event.pressure);
        match event.action {
            PointerAction::Down => {
                self.finish_shape_text()?;
                self.ink_drag = Some(match pen {
                    Some(tool) => {
                        let state = kition_sheet_engine::cmd::draw::draw_state(&self.session);
                        let color = state["color"].as_str().and_then(|c| kition_office_core::model::Color::from_value(&json!(c)));
                        let width = state["width"].as_f64().unwrap_or(2.0);
                        InkDrag::Pen { gesture: InkGesture::start(tool, color, width, p, pressure), zoom: event.zoom }
                    }
                    None => InkDrag::Eraser { gesture: EraserGesture::start(p, kition_sheet_engine::cmd::ink::ERASER_RADIUS) },
                });
                self.edits += 1;
                Ok(Some(true))
            }
            PointerAction::Drag => {
                match self.ink_drag.as_mut() {
                    Some(InkDrag::Pen { gesture, .. }) => gesture.drag(p, pressure),
                    Some(InkDrag::Eraser { gesture }) => gesture.drag(p),
                    None => return Ok(Some(false)),
                }
                self.edits += 1;
                Ok(Some(true))
            }
            PointerAction::Up => {
                self.edits += 1;
                match self.ink_drag.take() {
                    Some(InkDrag::Pen { gesture, .. }) => {
                        self.session.execute("drawing.ink", json!({ "strokes": [gesture.release()] })).map_err(engine_error)?;
                    }
                    Some(InkDrag::Eraser { gesture }) => {
                        let path: Vec<[f64; 2]> = gesture.path().iter().map(|q| [q.x, q.y]).collect();
                        self.session.execute("drawing.erase", json!({ "path": path, "radius": gesture.radius })).map_err(engine_error)?;
                    }
                    None => return Ok(Some(false)),
                }
                Ok(Some(true))
            }
            _ => Ok(Some(self.ink_drag.is_some())),
        }
    }

    /// The stroke being drawn, or the eraser's path.
    pub(crate) fn ink_overlays(&self, out: &mut Vec<ToolOverlay>) {
        let Some(drag) = &self.ink_drag else { return };
        let path = |pts: &mut dyn Iterator<Item = Point>| {
            let mut d = String::new();
            for (i, q) in pts.enumerate() {
                let [x, y] = canvas(q);
                d.push_str(&format!("{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" }));
            }
            d
        };
        match drag {
            InkDrag::Pen { gesture, zoom } => {
                let s = gesture.preview();
                let (r, g, b) = match s.color.as_ref().map(|c| &c.base) {
                    Some(kition_office_core::model::ColorBase::Rgb { rgb }) => (rgb.0, rgb.1, rgb.2),
                    _ => (0, 0, 0),
                };
                let alpha = (s.tool.alpha() * 255.0).round() as u8;
                let mut pts = s.path().into_iter();
                out.push(ToolOverlay::Path { d: path(&mut pts), color: [r, g, b, alpha], width: (s.width * zoom).max(1.0) as f32, dashed: false });
            }
            InkDrag::Eraser { gesture } => {
                let mut pts = gesture.path().iter().copied();
                out.push(ToolOverlay::Path { d: path(&mut pts), color: [120, 120, 120, 160], width: 2.0, dashed: true });
            }
        }
    }

    /// Whether Escape cancels something: typing in a cell (`key`), or in `object_key` a gesture,
    /// a shape chosen to draw, typing in a shape, or a selected object.
    pub(crate) fn escape_cancels(&self) -> bool {
        self.editing.is_some()
            || self.shape_edit.is_some()
            || self.ink_drag.is_some()
            || self.object_drag.is_some()
            || self.session.draw_shape.is_some()
            || self.selected_object().is_some()
            || self.session.format_painter.is_some()
    }

    pub(crate) fn object_key(&mut self, key: &str, mods: ToolMods) -> Result<bool, String> {
        if key == "Escape" && self.ink_drag.take().is_some() {
            self.edits += 1;
            return Ok(true);
        }
        if key == "Escape" && self.object_drag.take().is_some() {
            self.session.draw_shape = None;
            self.edits += 1;
            return Ok(true);
        }
        // Escape puts down a shape chosen to draw.
        if key == "Escape" && self.session.draw_shape.take().is_some() {
            self.edits += 1;
            return Ok(true);
        }
        if let Some(edit) = self.shape_edit.as_mut() {
            match key {
                "Backspace" => {
                    edit.text.pop();
                }
                "Enter" if !mods.shift => edit.text.push('\n'),
                "Enter" => edit.text.push('\n'),
                "Escape" => self.finish_shape_text()?,
                _ => return Ok(true),
            }
            self.edits += 1;
            return Ok(true);
        }
        let Some(id) = self.session.selected_shape.filter(|_| self.selected_object().is_some()) else { return Ok(false) };
        let step = if mods.alt { 1.0 } else { 6.0 };
        let nudge = match key {
            "ArrowLeft" => Some((-step, 0.0)),
            "ArrowRight" => Some((step, 0.0)),
            "ArrowUp" => Some((0.0, -step)),
            "ArrowDown" => Some((0.0, step)),
            _ => None,
        };
        if let Some((dx, dy)) = nudge {
            self.session.execute("drawing.transform", json!({ "ids": [id.to_string()], "dx": dx, "dy": dy })).map_err(engine_error)?;
            return Ok(true);
        }
        match key {
            "Delete" | "Backspace" => {
                self.session.execute("drawing.delete", json!({ "ids": [id.to_string()] })).map_err(engine_error)?;
            }
            "Escape" => self.session.selected_shape = None,
            "Enter" | "F2" if self.selected_object().is_some_and(|o| !matches!(o.kind, ObjectKind::Chart | ObjectKind::Ink)) => {
                let text = self.selected_object().and_then(|o| o.text).map(|t| t.text()).unwrap_or_default();
                self.shape_edit = Some(ShapeEdit { id, text });
            }
            _ => return Ok(false),
        }
        self.edits += 1;
        Ok(true)
    }

    /// Text inserted (Insert › Symbol) into drawing objects: at the end of the shape being typed,
    /// or after the selected shape's text, the shape then being typed. `true` when it was theirs.
    pub(crate) fn object_insert(&mut self, text: &str) -> bool {
        if self.shape_edit.is_none()
            && let Some(o) = self.selected_object().filter(|o| !matches!(o.kind, ObjectKind::Chart | ObjectKind::Ink))
            && let Some(id) = self.session.selected_shape
        {
            self.shape_edit = Some(ShapeEdit { id, text: o.text.map(|t| t.text()).unwrap_or_default() });
        }
        let Some(edit) = self.shape_edit.as_mut() else { return false };
        if edit.text.chars().count() + text.chars().count() <= 32_767 {
            edit.text.push_str(text);
        }
        self.edits += 1;
        true
    }

    /// A font command (bold, italic, underline, font, size, colour) with shapes selected or one
    /// being typed: it formats all of their text, as Excel and WPS do (`None`: not theirs). The
    /// shape being typed takes its text first.
    pub(crate) fn shape_font(&mut self, command: &str, params: &serde_json::Value) -> Option<Result<serde_json::Value, String>> {
        if !matches!(command, "home.bold" | "home.italic" | "home.underline" | "home.fontName" | "home.fontSize" | "home.fontColor") {
            return None;
        }
        let mut ids: Vec<u32> = self.shape_edit.as_ref().map(|e| e.id).into_iter().collect();
        if ids.is_empty() {
            ids.extend(self.session.selected_shape.filter(|_| self.selected_object().is_some()));
            ids.extend(self.also_selected.iter().copied());
        }
        let objects: Vec<DrawingObject> = self
            .drawing_objects()
            .into_iter()
            .filter(|o| ids.iter().any(|id| o.id == id.to_string()) && matches!(o.kind, ObjectKind::Shape | ObjectKind::TextBox))
            .collect();
        if objects.is_empty() {
            return None;
        }
        Some(self.format_shapes(command, params, objects))
    }

    fn format_shapes(&mut self, command: &str, params: &serde_json::Value, objects: Vec<DrawingObject>) -> Result<serde_json::Value, String> {
        self.finish_shape_text()?;
        // A toggle turns on unless every run already has it (Excel).
        let on = params.get("on").and_then(serde_json::Value::as_bool);
        let all = |test: &dyn Fn(&kition_office_core::model::Run) -> bool| {
            objects.iter().flat_map(|o| o.text.iter().flat_map(|t| t.paragraphs.iter().flat_map(|p| p.runs.iter()))).all(test)
        };
        let set: Box<dyn Fn(&mut kition_office_core::model::Run)> = match command {
            "home.bold" => {
                let v = on.unwrap_or_else(|| !all(&|r| r.bold == Some(true)));
                Box::new(move |r| r.bold = v.then_some(true))
            }
            "home.italic" => {
                let v = on.unwrap_or_else(|| !all(&|r| r.italic == Some(true)));
                Box::new(move |r| r.italic = v.then_some(true))
            }
            "home.underline" => {
                let v = on.unwrap_or_else(|| {
                    params.get("style").and_then(serde_json::Value::as_str).map_or_else(|| !all(&|r| r.underline == Some(true)), |s| s != "none")
                });
                Box::new(move |r| r.underline = v.then_some(true))
            }
            "home.fontName" => {
                let name = params
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .ok_or("home.fontName: missing `name`")?
                    .to_owned();
                Box::new(move |r| r.font = Some(name.clone()))
            }
            "home.fontSize" => {
                let size = params
                    .get("size")
                    .and_then(serde_json::Value::as_f64)
                    .filter(|n| (1.0..=409.0).contains(n))
                    .ok_or("home.fontSize: `size` is 1 to 409")?;
                Box::new(move |r| r.size = Some(size))
            }
            _ => {
                let color = params.get("color").and_then(kition_office_core::model::Color::from_value).ok_or("home.fontColor: missing `color`")?;
                Box::new(move |r| r.color = Some(color.clone()))
            }
        };
        for o in objects {
            let Some(mut body) = o.text else { continue };
            for run in body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
                set(run);
            }
            let paragraphs = serde_json::to_value(&body.paragraphs).map_err(|e| e.to_string())?;
            self.session.execute("drawing.text", json!({ "id": o.id, "paragraphs": paragraphs })).map_err(engine_error)?;
        }
        self.edits += 1;
        Ok(json!({ "shapes": true }))
    }

    /// Typed text for drawing objects: into the shape being typed, or (a shape selected) its
    /// text replaced, as typing on a selected shape does. `true` when it was theirs.
    pub(crate) fn object_text(&mut self, text: &str) -> bool {
        if self.shape_edit.is_none()
            && let Some(id) =
                self.session.selected_shape.filter(|_| self.selected_object().is_some_and(|o| !matches!(o.kind, ObjectKind::Chart | ObjectKind::Ink)))
        {
            self.shape_edit = Some(ShapeEdit { id, text: String::new() });
        }
        let Some(edit) = self.shape_edit.as_mut() else { return false };
        if edit.text.chars().count() + text.chars().count() <= 32_767 {
            edit.text.push_str(text);
        }
        self.edits += 1;
        true
    }

    /// The selected shape's frame and handles, a gesture's preview, and the caret of the shape
    /// being typed.
    pub(crate) fn object_overlays(&self, out: &mut Vec<ToolOverlay>) {
        // A shape being drawn: its outline (a line, or its box) as it would be.
        if let Some((create, f)) = self.object_drag.as_ref().and_then(|d| d.create.as_ref()) {
            let d = if kition_office_core::connect::is_line_preset(&create.preset) {
                let (p0, p1) = kition_office_core::connect::line_ends(f);
                let ([x0, y0], [x1, y1]) = (canvas(p0), canvas(p1));
                format!("M{x0:.1},{y0:.1} L{x1:.1},{y1:.1}")
            } else {
                let o = DrawingObject { frame: *f, geometry: kition_office_core::model::Geometry::preset(&create.preset), ..Default::default() };
                let path = kition_office_core::edit::outline(&o).outline();
                (kition_office_core::kurbo::Affine::translate((HEADER_W, HEADER_H)) * f.affine() * path).to_svg()
            };
            out.push(ToolOverlay::Path { d, color: ACCENT, width: 1.5, dashed: false });
            return;
        }
        let Some(o) = self.selected_object() else { return };
        let quad = |f: &Xfrm| {
            let a = f.affine();
            let pts = [(0.0, 0.0), (f.w, 0.0), (f.w, f.h), (0.0, f.h)].map(|(x, y)| canvas(a * Point::new(x, y)));
            format!(
                "M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z",
                pts[0][0], pts[0][1], pts[1][0], pts[1][1], pts[2][0], pts[2][1], pts[3][0], pts[3][1]
            )
        };
        // A connector's end under way: the line as it would be.
        if let Some((_, f)) = self.object_drag.as_ref().and_then(|d| d.end.as_ref()) {
            let (p0, p1) = kition_office_core::connect::line_ends(f);
            let ([x0, y0], [x1, y1]) = (canvas(p0), canvas(p1));
            out.push(ToolOverlay::Path { d: format!("M{x0:.1},{y0:.1} L{x1:.1},{y1:.1}"), color: ACCENT, width: 1.5, dashed: false });
            for [x, y] in [[x0, y0], [x1, y1]] {
                out.push(ToolOverlay::Anchor { x, y, color: ADJUST, filled: true, size: 8.0 });
            }
            return;
        }
        // An adjust handle under way: the shape's new outline.
        if let Some((_, path)) = self.object_drag.as_ref().and_then(|d| d.gesture.preview_outline()) {
            let shifted = kition_office_core::kurbo::Affine::translate((HEADER_W, HEADER_H)) * path;
            out.push(ToolOverlay::Path { d: quad(&o.frame), color: ACCENT, width: 1.0, dashed: true });
            out.push(ToolOverlay::Path { d: shifted.to_svg(), color: ACCENT, width: 1.5, dashed: false });
            return;
        }
        let preview = self.object_drag.as_ref().and_then(|d| d.preview.frames.first().map(|(_, f)| *f));
        out.push(ToolOverlay::Path { d: quad(&o.frame), color: ACCENT, width: 1.0, dashed: preview.is_some() });
        if let Some(f) = preview {
            out.push(ToolOverlay::Path { d: quad(&f), color: ACCENT, width: 1.5, dashed: false });
            return;
        }
        // Objects added with Shift: their frames.
        let objects = self.drawing_objects();
        for id in &self.also_selected {
            if let Some(other) = objects.iter().find(|x| x.id == id.to_string()) {
                out.push(ToolOverlay::Path { d: quad(&other.frame), color: ACCENT, width: 1.0, dashed: false });
            }
        }
        // Ink only moves: its frame, without handles.
        if self.shape_edit.is_none() && o.kind != ObjectKind::Ink {
            let (pts, rot) = box_handles(&o.frame);
            for q in pts {
                let [x, y] = canvas(q);
                out.push(ToolOverlay::Anchor { x, y, color: HANDLE, filled: true, size: 8.0 });
            }
            if !matches!(o.kind, ObjectKind::Chart | ObjectKind::Ink | ObjectKind::Group) {
                let [x, y] = canvas(rot);
                out.push(ToolOverlay::Anchor { x, y, color: ACCENT, filled: true, size: 8.0 });
            }
            // A connector's ends, which drag onto other shapes' sites.
            if kition_office_core::connect::is_connector(&o) {
                let (p0, p1) = kition_office_core::connect::line_ends(&o.frame);
                for q in [p0, p1] {
                    let [x, y] = canvas(q);
                    out.push(ToolOverlay::Anchor { x, y, color: ADJUST, filled: false, size: 9.0 });
                }
            }
            if matches!(o.kind, ObjectKind::Shape | ObjectKind::TextBox) {
                for (_, q) in adjust_handles(&o.frame, &o.geometry) {
                    let [x, y] = canvas(q);
                    out.push(ToolOverlay::Anchor { x, y, color: ADJUST, filled: true, size: 8.0 });
                }
            }
        } else if let Some([x, y0, y1]) = self.shape_caret() {
            out.push(ToolOverlay::Line { ax: x, ay: y0, bx: x, by: y1, color: ACCENT, dashed: false });
        }
    }

    /// Where the caret of the shape being typed is (after its text): canvas x and the line's
    /// top and bottom.
    fn shape_caret(&self) -> Option<[f64; 3]> {
        let edit = self.shape_edit.as_ref()?;
        let (_, sheet) = self.sheet()?;
        let mut shape = sheet.shapes.iter().find(|s| s.id == edit.id)?.clone();
        shape.text.clone_from(&edit.text);
        shape.text_body = None;
        let (w, h) = (shape.anchor.width.max(0.0), shape.anchor.height.max(0.0));
        let at = (
            Self::col_x(sheet, shape.anchor.cell.col) + f64::from(shape.anchor.dx),
            Self::row_y(sheet, shape.anchor.cell.row) + f64::from(shape.anchor.dy),
        );
        let text = kition_sheet_engine::shape_text::shape_text(&shape);
        let theme = &self.sheet()?.0.theme;
        let measure = |run: &kition_sheet_engine::shape_text::ShapeTextRun, size: f32, s: &str| {
            crate::Line { font: &self.shape_face(theme, run).face, size, text: s }.width() as f32
        };
        let pieces = kition_sheet_engine::shape_text::layout(&text, w, h, &measure);
        match pieces.last() {
            Some(piece) => {
                let width = crate::Line { font: &self.shape_face(theme, piece.run).face, size: piece.size, text: &piece.text }.width();
                let x = at.0 + f64::from(piece.x) + width;
                let size = f64::from(piece.size);
                // A new line after the text starts below it.
                let (x, base) =
                    if edit.text.ends_with('\n') { (at.0 + 8.0, f64::from(piece.baseline) + size * 1.2) } else { (x, f64::from(piece.baseline)) };
                Some([x, at.1 + base - size, at.1 + base + size * 0.25])
            }
            None => Some([at.0 + f64::from(w) / 2.0, at.1 + f64::from(h) / 2.0 - 8.0, at.1 + f64::from(h) / 2.0 + 8.0]),
        }
    }
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use kition_kit::tools::KitTools;
    use kition_kit::tools::{PointerAction, ToolMods, ToolPointer};
    use kition_office_core::model::DrawingObject;
    use serde_json::json;

    use crate::{HEADER_H, HEADER_W, SheetDocument};

    fn ev(action: PointerAction, x: f64, y: f64) -> ToolPointer {
        ToolPointer { action, x: x + HEADER_W, y: y + HEADER_H, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 }
    }

    fn shape(doc: &mut SheetDocument, id: &str) -> DrawingObject {
        let d = doc.execute("drawing.describe", json!({ "ids": [id] })).unwrap();
        serde_json::from_value(d["objects"][0].clone()).unwrap()
    }

    fn with_shape() -> (SheetDocument, String) {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let r =
            doc.execute("drawing.insert", json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 100, "y": 60, "w": 120, "h": 60 } })).unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        doc.session.selected_shape = None;
        (doc, id)
    }

    fn undo_steps(doc: &SheetDocument) -> usize {
        doc.session.active().map_or(0, |st| st.undo.len())
    }

    #[test]
    fn a_shape_chosen_in_insert_shapes_is_drawn_by_a_drag_and_a_line_between_shapes_glues() {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let count = |doc: &SheetDocument| doc.session.doc().unwrap().wb.active().unwrap().shapes.len();
        doc.execute("draw.shape", json!({ "preset": "ellipse" })).unwrap();
        doc.pointer(&ev(PointerAction::Down, 100.0, 60.0)).unwrap();
        let r = doc.pointer(&ev(PointerAction::Drag, 220.0, 120.0)).unwrap();
        assert!(r.overlays.iter().any(|o| matches!(o, kition_kit::tools::ToolOverlay::Path { width, .. } if *width == 1.5)), "the outline previews");
        assert_eq!(count(&doc), 0, "nothing is made before the release");
        doc.pointer(&ev(PointerAction::Up, 220.0, 120.0)).unwrap();
        let ellipse = doc.session.selected_shape.unwrap().to_string();
        let e = shape(&mut doc, &ellipse);
        assert_eq!(e.geometry, kition_office_core::model::Geometry::preset("ellipse"));
        assert!((e.frame.x - 100.0).abs() < 1.0 && (e.frame.w - 120.0).abs() < 1.0 && (e.frame.h - 60.0).abs() < 1.0, "{:?}", e.frame);
        assert_eq!(doc.session.draw_shape, None);
        let rect = doc
            .execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": 100, "y": 300, "w": 120, "h": 50 } }))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let from = kition_office_core::connect::sites(&shape(&mut doc, &ellipse)).into_iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
        let to = kition_office_core::connect::sites(&shape(&mut doc, &rect)).into_iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
        let before = count(&doc);
        doc.execute("draw.shape", json!({ "preset": "line" })).unwrap();
        doc.pointer(&ev(PointerAction::Down, from.x + 2.0, from.y)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, to.x, to.y - 3.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, to.x, to.y - 3.0)).unwrap();
        let made = doc.session.selected_shape.unwrap().to_string();
        let line = shape(&mut doc, &made);
        let conn = line.connection.clone().expect("glued");
        assert_eq!((conn.start.unwrap().id, conn.end.unwrap().id), (ellipse, rect));
        assert_eq!(count(&doc), before + 1);
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(count(&doc), before);
        doc.execute("draw.shape", json!({ "preset": "rect" })).unwrap();
        doc.key("Escape", kition_kit::tools::ToolMods::default(), 1.0).unwrap();
        assert_eq!(doc.session.draw_shape, None);
        assert!(doc.execute("draw.shape", json!({ "preset": "notAShape" })).is_err());
    }

    #[test]
    fn a_connectors_end_drags_onto_another_shape_in_one_step() {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let ins = |doc: &mut SheetDocument, x: f64, y: f64| -> String {
            let r =
                doc.execute("drawing.insert", json!({ "geometry": { "preset": "rect" }, "frame": { "x": x, "y": y, "w": 80, "h": 40 } })).unwrap();
            r["id"].as_str().unwrap().to_string()
        };
        let (a, b, c) = (ins(&mut doc, 40.0, 40.0), ins(&mut doc, 300.0, 200.0), ins(&mut doc, 40.0, 300.0));
        let k = doc.execute("drawing.connect", json!({ "from": a, "to": b })).unwrap()["id"].as_str().unwrap().to_string();
        doc.session.selected_shape = Some(k.parse().unwrap());
        let o = shape(&mut doc, &k);
        let (p0, p1) = kition_office_core::connect::line_ends(&o.frame);
        let r = doc.pointer(&ev(PointerAction::Move, 500.0, 500.0)).unwrap();
        assert!(
            r.overlays.iter().filter(|o| matches!(o, kition_kit::tools::ToolOverlay::Anchor { color, .. } if *color == super::ADJUST)).count() >= 2,
            "its ends show"
        );
        let target = kition_office_core::connect::sites(&shape(&mut doc, &c)).into_iter().min_by(|x, y| x.y.total_cmp(&y.y)).unwrap();
        let before = doc.session.active().map_or(0, |st| st.undo.len());
        doc.pointer(&ev(PointerAction::Down, p1.x, p1.y)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, target.x + 2.0, target.y + 3.0)).unwrap();
        assert_eq!(shape(&mut doc, &k).frame, o.frame, "the drag previews only");
        doc.pointer(&ev(PointerAction::Up, target.x + 2.0, target.y + 3.0)).unwrap();
        let now = shape(&mut doc, &k);
        let conn = now.connection.clone().unwrap();
        assert_eq!((conn.start.unwrap().id, conn.end.unwrap().id), (a.clone(), c.clone()));
        let (q0, q1) = kition_office_core::connect::line_ends(&now.frame);
        assert!((q0 - p0).hypot() < 1.0 && (q1 - target).hypot() < 1.0, "{q0:?} {q1:?} {target:?}");
        assert_eq!(doc.session.active().map_or(0, |st| st.undo.len()), before + 1);
        // Dropped in the open: the end comes free there.
        doc.pointer(&ev(PointerAction::Down, q1.x, q1.y)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, 500.0, 420.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 500.0, 420.0)).unwrap();
        let free = shape(&mut doc, &k);
        assert_eq!(free.connection.unwrap().end, None);
        assert!((kition_office_core::connect::line_ends(&free.frame).1 - kition_office_core::Point::new(500.0, 420.0)).hypot() < 1.0);
    }

    #[test]
    fn a_pen_draws_a_stroke_and_the_eraser_takes_it_back_with_the_pointer() {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let inks = |doc: &mut SheetDocument| -> Vec<DrawingObject> {
            let ids: Vec<String> = doc.session.doc().unwrap().wb.active().unwrap().shapes.iter().map(|x| x.id.to_string()).collect();
            if ids.is_empty() {
                return Vec::new();
            }
            let d = doc.execute("drawing.describe", json!({ "ids": ids })).unwrap();
            serde_json::from_value::<Vec<DrawingObject>>(d["objects"].clone())
                .unwrap()
                .into_iter()
                .filter(|o| o.kind == kition_office_core::model::ObjectKind::Ink)
                .collect()
        };
        doc.execute("draw.pen", json!({ "color": "#C00000", "width": 3 })).unwrap();
        let steps = undo_steps(&doc);
        doc.pointer(&ev(PointerAction::Down, 100.0, 80.0)).unwrap();
        for i in 1..=10 {
            let r = doc.pointer(&ev(PointerAction::Drag, 100.0 + f64::from(i) * 10.0, 80.0)).unwrap();
            assert!(!r.overlays.is_empty(), "the stroke shows while it is drawn");
        }
        assert!(inks(&mut doc).is_empty(), "nothing is added before the pointer lifts");
        doc.pointer(&ev(PointerAction::Up, 200.0, 80.0)).unwrap();
        let drawn = inks(&mut doc);
        assert_eq!(drawn.len(), 1);
        assert_eq!(undo_steps(&doc), steps + 1, "one undo step");
        let st = &drawn[0].ink.as_ref().unwrap()[0];
        assert_eq!(st.width, 3.0);
        assert!((st.points[0][0] - 100.0).abs() < 1.0 && (st.points.last().unwrap()[0] - 200.0).abs() < 1.0, "{:?}", st.points);
        // Escape drops a stroke being drawn.
        doc.pointer(&ev(PointerAction::Down, 100.0, 150.0)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, 180.0, 150.0)).unwrap();
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        doc.pointer(&ev(PointerAction::Up, 180.0, 150.0)).unwrap();
        assert_eq!(inks(&mut doc).len(), 1);
        // The eraser's drag across the stroke takes it.
        doc.execute("draw.eraser", json!({})).unwrap();
        doc.pointer(&ev(PointerAction::Down, 150.0, 60.0)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, 150.0, 100.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 150.0, 100.0)).unwrap();
        assert!(inks(&mut doc).is_empty());
        // Back to selecting: the pointer selects cells again.
        doc.execute("draw.select", json!({})).unwrap();
        doc.pointer(&ev(PointerAction::Down, 100.0, 200.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 140.0, 200.0)).unwrap();
        assert!(inks(&mut doc).is_empty());
    }

    #[test]
    fn a_shapes_adjust_handle_reshapes_it_in_one_step() {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let r = doc
            .execute("drawing.insert", json!({ "geometry": { "preset": "roundRect" }, "frame": { "x": 100, "y": 60, "w": 200, "h": 100 } }))
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        doc.session.selected_shape = None;
        // Select it: its yellow handle shows on the top edge.
        doc.pointer(&ev(PointerAction::Down, 200.0, 110.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 200.0, 110.0)).unwrap();
        let o = shape(&mut doc, &id);
        let (_, at) = kition_office_core::edit::adjust_handles(&o.frame, &o.geometry)[0];
        assert!((at.y - o.frame.y).abs() < 0.1, "{at:?} {:?}", o.frame);
        let r = doc.pointer(&ev(PointerAction::Move, 600.0, 400.0)).unwrap();
        assert!(r.overlays.iter().any(|o| matches!(o, kition_kit::tools::ToolOverlay::Anchor { color, .. } if *color == super::ADJUST)));
        // Dragging it in rounds the corners more, in one undo step; the preview changes nothing.
        let steps = undo_steps(&doc);
        doc.pointer(&ev(PointerAction::Down, at.x, at.y)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, at.x + 25.0, at.y)).unwrap();
        assert_eq!(shape(&mut doc, &id).geometry, o.geometry);
        doc.pointer(&ev(PointerAction::Up, at.x + 25.0, at.y)).unwrap();
        let after = shape(&mut doc, &id);
        let v = after.geometry.adjust_values()[0];
        assert!((v - (16_667.0 + 25.0 / o.frame.h.min(o.frame.w) * 100_000.0)).abs() <= 1.0, "{:?}", after.geometry);
        assert_eq!(after.frame, o.frame);
        assert_eq!(undo_steps(&doc), steps + 1);
        // It is saved and reads back.
        let bytes = doc.save(Some("Book.xlsx")).unwrap();
        let mut back = SheetDocument::open("Book.xlsx", &bytes).unwrap();
        let ids: Vec<String> = back.session.doc().unwrap().wb.active().unwrap().shapes.iter().map(|x| x.id.to_string()).collect();
        let reopened = shape(&mut back, &ids[0]);
        assert!((reopened.geometry.adjust_values()[0] - v).abs() <= 1.0, "{:?}", reopened.geometry);
    }

    #[test]
    fn a_drag_moves_a_shape_in_one_step_and_leaves_the_cells_alone() {
        let (mut doc, id) = with_shape();
        let selection = doc.session.active().unwrap().selection.clone();
        let steps = undo_steps(&doc);
        doc.pointer(&ev(PointerAction::Down, 160.0, 90.0)).unwrap();
        assert_eq!(doc.session.selected_shape.map(|i| i.to_string()), Some(id.clone()));
        doc.pointer(&ev(PointerAction::Drag, 180.0, 95.0)).unwrap();
        // The preview is an overlay: the shape has not moved yet.
        assert_eq!(shape(&mut doc, &id).frame.x, 100.0);
        let r = doc.pointer(&ev(PointerAction::Drag, 200.0, 100.0)).unwrap();
        assert!(r.overlays.len() > 2);
        doc.pointer(&ev(PointerAction::Up, 200.0, 100.0)).unwrap();
        let f = shape(&mut doc, &id).frame;
        assert!((f.x - 140.0).abs() < 0.01 && (f.y - 70.0).abs() < 0.01, "{f:?}");
        assert_eq!(undo_steps(&doc), steps + 1, "one gesture, one undo step");
        assert_eq!(doc.session.active().unwrap().selection, selection, "the cell selection stays");
        doc.execute("edit.undo", json!({})).unwrap();
        assert_eq!(shape(&mut doc, &id).frame.x, 100.0);
    }

    #[test]
    fn handles_resize_and_turn_and_escape_cancels() {
        let (mut doc, id) = with_shape();
        doc.pointer(&ev(PointerAction::Down, 160.0, 90.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 160.0, 90.0)).unwrap();
        // The bottom-right handle.
        doc.pointer(&ev(PointerAction::Down, 220.0, 120.0)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, 250.0, 140.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 250.0, 140.0)).unwrap();
        let f = shape(&mut doc, &id).frame;
        assert!((f.w - 150.0).abs() < 0.01 && (f.h - 80.0).abs() < 0.01, "{f:?}");
        // The rotation handle, 20 pt above the top centre; a drag to the right of the centre turns
        // it a quarter.
        let (cx, cy) = (f.x + f.w / 2.0, f.y + f.h / 2.0);
        doc.pointer(&ev(PointerAction::Down, cx, f.y - 20.0)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, cx + 100.0, cy)).unwrap();
        doc.pointer(&ev(PointerAction::Up, cx + 100.0, cy)).unwrap();
        assert!((shape(&mut doc, &id).frame.rot - 90.0).abs() < 0.5);
        // A move cancelled with Escape changes nothing.
        let before = shape(&mut doc, &id).frame;
        doc.pointer(&ev(PointerAction::Down, cx, cy)).unwrap();
        doc.pointer(&ev(PointerAction::Drag, cx + 50.0, cy)).unwrap();
        doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        doc.pointer(&ev(PointerAction::Up, cx + 50.0, cy)).unwrap();
        assert_eq!(shape(&mut doc, &id).frame, before);
    }

    #[test]
    fn typing_on_a_selected_shape_sets_its_text_and_keys_nudge_and_delete() {
        let (mut doc, id) = with_shape();
        doc.pointer(&ev(PointerAction::Down, 160.0, 90.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 160.0, 90.0)).unwrap();
        let steps = undo_steps(&doc);
        KitTools::text(&mut doc, "Q", 1.0).unwrap();
        // An input method's composed text arrives whole.
        KitTools::text(&mut doc, "3 增长", 1.0).unwrap();
        doc.key("Backspace", ToolMods::default(), 1.0).unwrap();
        let r = doc.key("Escape", ToolMods::default(), 1.0).unwrap();
        assert_eq!(shape(&mut doc, &id).text.unwrap().text(), "Q3 增");
        assert_eq!(undo_steps(&doc), steps + 1, "the typing is one step");
        assert!(!r.overlays.is_empty());
        // The cell under the shape got nothing.
        assert_eq!(doc.execute("cell.get", json!({ "cell": "C4" })).unwrap()["input"], json!(""));
        doc.key("ArrowRight", ToolMods::default(), 1.0).unwrap();
        assert!((shape(&mut doc, &id).frame.x - 106.0).abs() < 0.01);
        doc.key("Delete", ToolMods::default(), 1.0).unwrap();
        assert!(doc.execute("drawing.describe", json!({})).unwrap()["objects"].as_array().unwrap().is_empty());
        // Clicking a cell afterwards selects the cell as before.
        doc.pointer(&ev(PointerAction::Down, 10.0, 10.0)).unwrap();
        assert!(doc.session.selected_shape.is_none());
    }

    #[test]
    fn a_shape_edited_with_the_pointer_saves_and_reopens() {
        let (mut doc, id) = with_shape();
        doc.pointer(&ev(PointerAction::DoubleClick, 160.0, 90.0)).unwrap();
        KitTools::text(&mut doc, "Hi", 1.0).unwrap();
        doc.pointer(&ev(PointerAction::Down, 10.0, 10.0)).unwrap();
        doc.pointer(&ev(PointerAction::Up, 10.0, 10.0)).unwrap();
        let bytes = doc.save(Some("Book.xlsx")).unwrap();
        let mut back = SheetDocument::open("Book.xlsx", &bytes).unwrap();
        let o = shape(&mut back, &id);
        assert_eq!(o.text.unwrap().text(), "Hi");
        assert_eq!(o.geometry, kition_office_core::model::Geometry::preset("ellipse"));
    }
}

#[cfg(test)]
mod paint_tests {
    use kition_kit::KitDocument;
    use serde_json::json;

    use crate::SheetDocument;

    /// A shape drawn in two tiles is the shape drawn whole: no tile cuts it off.
    #[test]
    fn a_shape_split_across_tiles_paints_the_same() {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("drawing.insert", json!({ "geometry": { "preset": "ellipse" }, "frame": { "x": 100, "y": 60, "w": 180, "h": 120 }, "fill": { "color": "#FF0000" }, "stroke": { "none": true } })).unwrap();
        let (w, h) = (400u32, 300u32);
        let whole = doc.render(0.0, 0.0, 1.0, w, h).unwrap();
        let top = doc.render(0.0, 0.0, 1.0, w, 150).unwrap();
        let bottom = doc.render(0.0, 150.0, 1.0, w, 150).unwrap();
        let px = |buf: &[u8], x: u32, y: u32, width: u32| {
            let i = ((y * width + x) * 4) as usize;
            [buf[i], buf[i + 1], buf[i + 2]]
        };
        let mut differ = 0;
        for y in 0..h {
            for x in 0..w {
                let tiled = if y < 150 { px(&top, x, y, w) } else { px(&bottom, x, y - 150, w) };
                if tiled != px(&whole, x, y, w) {
                    differ += 1;
                }
            }
        }
        assert_eq!(differ, 0, "{differ} pixels differ between the tiles and the whole");
        // The ellipse's lowest part (sheet y 170, canvas + header) is red.
        let [r, g, _] = px(&whole, 46 + 190, 21 + 170, w);
        assert!(r > 200 && g < 60, "the bottom of the shape is painted: {r},{g}");
    }
}
