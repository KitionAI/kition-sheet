//! `file.render`: the agent's preview by explicit view (`agent.render` in kit.json, Kit Process
//! Protocol section 7, task card C4/C5/C6-sheet). It draws a named sheet's range with its row and
//! column headings, so a model reads addresses off the picture:
//!
//! - `target.sheet`: a sheet's name or tool id (`sheet:<id>`); without it the first sheet in the
//!   workbook's order, never the active one.
//! - `target.range`: a range in A1 notation (its sheet, when written, must be `target.sheet`);
//!   without it the sheet's used range (A1 on an empty sheet), cut to [`MAX_CELLS`] cells (whole
//!   rows from the top, at most [`MAX_COLUMNS`] columns) and said so (`capped`, `usedRange`). A
//!   range asked for beyond the cap is refused: the picture is what was asked or nothing.
//! - `scale`: pixels per document pixel (a point at 100 %), more than 0 and at most [`MAX_SCALE`],
//!   default 1; brought down so the longest side is at most `maxSize` (16 to 8192) and the
//!   renderer's largest side.
//!
//! It is a function of the workbook and the call: the active sheet, the selection, the zoom, the
//! viewport, the cell or shape being typed and the frozen panes' scrolled view are neither read nor
//! changed, and nothing is drawn from the clock, so the same call answers the same bytes.

use std::borrow::Cow;

use image::ImageEncoder as _;
use kition_kit::MAX_RENDER_SIDE;
use kition_sheet_engine::core::{CellRef, RangeRef, col_to_letters};
use kition_sheet_engine::model::Sheet;
use serde_json::{Value as Json, json};

use crate::draw::{Canvas, Line, Rect};
use crate::ids::{self, ToolId};
use crate::{HEADING, HEADING_LINE, HEADING_TEXT, Paint, SheetDocument, WHITE};

/// The most cells one picture draws.
pub const MAX_CELLS: u64 = 5000;
/// The most columns a used range is drawn with before it is cut.
pub const MAX_COLUMNS: u32 = 50;
/// The largest `scale`.
pub const MAX_SCALE: f64 = 4.0;
/// `maxSize`'s bounds.
const MAX_SIZE: std::ops::RangeInclusive<u64> = 16..=8192;
/// The headings' band: the row numbers' width and the column letters' height, in document pixels.
const BAND_W: f64 = 46.0;
const BAND_H: f64 = 21.0;

const C: &str = "file.render";

fn bad(message: impl std::fmt::Display) -> String {
    format!("{C}: {message}")
}

impl SheetDocument {
    pub(crate) fn render_view(&self, params: &Json) -> Result<Json, String> {
        let st = self.doc_state()?;
        let wb = &st.wb;
        let target = match params.get("target") {
            None | Some(Json::Null) => serde_json::Map::new(),
            Some(Json::Object(target)) => target.clone(),
            Some(other) => return Err(bad(format!("`target` is {other}: an object `{{sheet?, range?}}`"))),
        };
        if let Some(other) = target.keys().find(|key| !matches!(key.as_str(), "sheet" | "range")) {
            return Err(bad(format!(
                "`target.{other}` is not a view it draws: `target` takes `sheet` (a name or `sheet:<id>`) and `range` (A1 notation)"
            )));
        }
        let listed = "the sheets' names and ids are in sheet_inspect's `workbook.sheets`";
        let index = match target.get("sheet") {
            None | Some(Json::Null) => 0,
            Some(Json::String(text)) => match ToolId::parse(text) {
                Some(ToolId::Sheet(id)) => ids::sheet_index(wb, id).ok_or_else(|| bad(format!("no sheet has the tool id `{text}`: {listed}")))?,
                Some(ToolId::Drawing(_)) => return Err(bad(format!("`{text}` is a drawing object, not a sheet: {listed}"))),
                None => wb.sheet_index(text).ok_or_else(|| bad(format!("the workbook has no sheet named `{text}`: {listed}")))?,
            },
            Some(other) => return Err(bad(format!("`target.sheet` is {other}: a sheet's name or its tool id `sheet:<id>` ({listed})"))),
        };
        let sheet = wb.sheet(index).ok_or_else(|| bad("the workbook has no sheets"))?;
        let used = sheet.used_range();
        let (range, capped) = match target.get("range") {
            None | Some(Json::Null) => {
                let whole = used.unwrap_or(RangeRef::cell(CellRef::new(0, 0)));
                let cols = whole.width().min(MAX_COLUMNS);
                let rows = u32::try_from((MAX_CELLS / u64::from(cols.max(1))).max(1)).unwrap_or(u32::MAX).min(whole.height());
                let cut = RangeRef::new(whole.start, CellRef::new(whole.start.row + rows - 1, whole.start.col + cols - 1));
                (cut, cut != whole)
            }
            Some(Json::String(text)) => {
                let (named, body) = match text.rsplit_once('!') {
                    Some((named, body)) => (Some(named.trim_matches('\'').replace("''", "'")), body),
                    None => (None, text.as_str()),
                };
                if let Some(named) = named
                    && wb.sheet_index(&named) != Some(index)
                {
                    return Err(bad(format!(
                        "`target.range` names sheet `{named}`, and the picture is of sheet `{}`: give that sheet in `target.sheet`",
                        sheet.name
                    )));
                }
                let range = RangeRef::parse(body).ok_or_else(|| {
                    bad(format!(
                        "`target.range` `{text}` is not a range in A1 notation within the sheet (columns A to XFD, rows 1 to 1048576), like `A1:D20`"
                    ))
                })?;
                // Whole rows or columns are drawn as far as the sheet is used.
                let range = if range.is_full_rows() || range.is_full_cols() {
                    used.and_then(|used| range.intersection(&used)).unwrap_or(RangeRef::cell(range.start))
                } else {
                    range
                };
                if range.count() > MAX_CELLS {
                    return Err(bad(format!(
                        "`target.range` `{text}` has {} cells, and a picture draws at most {MAX_CELLS} cells: ask for a smaller range (sheet.read reads any size)",
                        range.count()
                    )));
                }
                (range, false)
            }
            Some(other) => return Err(bad(format!("`target.range` is {other}: a range in A1 notation, like `A1:D20`"))),
        };
        let mut scale = match params.get("scale") {
            None | Some(Json::Null) => 1.0,
            Some(value) => value
                .as_f64()
                .filter(|scale| scale.is_finite() && *scale > 0.0 && *scale <= MAX_SCALE)
                .ok_or_else(|| bad(format!("`scale` is {value}: pixels per document pixel, more than 0 and at most {MAX_SCALE}")))?,
        };
        // Seen from its first row and column: a frozen pane's scrolled view is the person's.
        let shown: Cow<'_, Sheet> = if crate::view::origin(sheet) == (0, 0) {
            Cow::Borrowed(sheet)
        } else {
            let mut copy = sheet.clone();
            copy.view_top_left = CellRef::default();
            Cow::Owned(copy)
        };
        let sheet = shown.as_ref();
        let (x0, y0) = (Self::col_x(sheet, range.start.col), Self::row_y(sheet, range.start.row));
        let (x1, y1) = (Self::col_x(sheet, range.end.col + 1), Self::row_y(sheet, range.end.row + 1));
        let (doc_w, doc_h) = (BAND_W + (x1 - x0).max(1.0), BAND_H + (y1 - y0).max(1.0));
        let longest = doc_w.max(doc_h);
        if let Some(value) = params.get("maxSize").filter(|value| !value.is_null()) {
            let max = value
                .as_u64()
                .filter(|max| MAX_SIZE.contains(max))
                .ok_or_else(|| bad(format!("`maxSize` is {value}: the longest side in pixels, {} to {}", MAX_SIZE.start(), MAX_SIZE.end())))?;
            scale = scale.min(max as f64 / longest);
        }
        scale = scale.min(f64::from(MAX_RENDER_SIDE) / longest);
        let width = ((doc_w * scale).floor() as u32).max(1);
        let height = ((doc_h * scale).floor() as u32).max(1);
        let (band_w, band_h) = ((BAND_W * scale).round(), (BAND_H * scale).round());
        let cells_w = width.saturating_sub(band_w as u32).max(1);
        let cells_h = height.saturating_sub(band_h as u32).max(1);
        let extent = (range.end.col + 1, range.end.row + 1);
        let paint = Paint { plain: false, editor: false, headings: false };
        let cells = self.paint_sheet(wb, index, sheet, extent, (x0, y0, scale, cells_w, cells_h), paint);
        let mut canvas = Canvas::new(width, height, WHITE);
        let all = Rect { x0: 0.0, y0: 0.0, x1: f64::from(width), y1: f64::from(height) };
        canvas.image(&cells, cells_w as usize, cells_h as usize, band_w, band_h, all);
        self.paint_bands(&mut canvas, sheet, range, (x0, y0, scale), (band_w, band_h), (f64::from(width), f64::from(height)));
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(&canvas.px, width, height, image::ExtendedColorType::Rgba8).map_err(bad)?;
        let mut answer = json!({
            "png": kition_sheet_engine::io::base64_encode(&png),
            "mimeType": "image/png",
            "width": width,
            "height": height,
            "scale": scale,
            "sheet": sheet.name,
            "sheetId": ids::sheet_tool_id(sheet),
            "range": range.a1(),
            "cells": range.count(),
            "maxCells": MAX_CELLS,
            "capped": capped,
        });
        if capped {
            answer["usedRange"] = json!(used.map(|used| used.a1()));
        }
        Ok(answer)
    }

    /// The column letters over the cells and the row numbers left of them, for `range` drawn from
    /// document point (`x0`, `y0`) at `scale`, the cells starting at (`band_w`, `band_h`).
    fn paint_bands(
        &self,
        canvas: &mut Canvas,
        sheet: &Sheet,
        range: RangeRef,
        (x0, y0, scale): (f64, f64, f64),
        (band_w, band_h): (f64, f64),
        (right, bottom): (f64, f64),
    ) {
        let line = scale.clamp(1.0, 2.0).round();
        canvas.fill(Rect { x0: 0.0, y0: 0.0, x1: right, y1: band_h }, HEADING);
        canvas.fill(Rect { x0: 0.0, y0: 0.0, x1: band_w, y1: bottom }, HEADING);
        let size = (11.0 * 96.0 / 72.0 * scale.min(2.0)) as f32;
        let to_x = |x: f64| band_w + (x - x0) * scale;
        let to_y = |y: f64| band_h + (y - y0) * scale;
        let label = |canvas: &mut Canvas, text: &str, cell: Rect| {
            let text = Line { font: &self.faces.sans, size, text };
            let (ascent, descent) = text.metrics();
            let baseline = ((cell.y0 + cell.y1) / 2.0 + (ascent - descent) / 2.0).round();
            text.draw(canvas, ((cell.x0 + cell.x1) / 2.0 - text.width() / 2.0).round(), baseline, cell, HEADING_TEXT, false);
        };
        for col in range.start.col..=range.end.col {
            let (a, b) = (to_x(Self::col_x(sheet, col)), to_x(Self::col_x(sheet, col + 1)));
            if b - a < 1.0 {
                continue;
            }
            canvas.vline(a.round(), 0.0, band_h, line, HEADING_LINE);
            label(canvas, &col_to_letters(col), Rect { x0: a, y0: 0.0, x1: b, y1: band_h });
        }
        for row in range.start.row..=range.end.row {
            let (a, b) = (to_y(Self::row_y(sheet, row)), to_y(Self::row_y(sheet, row + 1)));
            if b - a < 1.0 {
                continue;
            }
            canvas.hline(0.0, band_w, a.round(), line, HEADING_LINE);
            label(canvas, &(u64::from(row) + 1).to_string(), Rect { x0: 0.0, y0: a, x1: band_w, y1: b });
        }
        canvas.hline(0.0, right, band_h.round() - line, line, HEADING_LINE);
        canvas.vline(band_w.round() - line, 0.0, bottom, line, HEADING_LINE);
    }
}
