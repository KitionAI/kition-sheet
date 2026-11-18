//! One id space (task card C4/C5/C6-sheet, KitObjectModel OM9): what the agent's tools call each
//! sheet and drawing object, and the one place their ids become the kit's own numbers.
//!
//! - A sheet is `sheet:<id>`, its id the number the workbook gives it (`sheetId` in the file):
//!   it survives renaming, moving, saving and reopening. Commands' `sheet` parameters take that tool
//!   id beside a sheet's name (and, as before, its position from 0); a call's part
//!   (`{"sheet": …}`) takes the id, the tool id or the name, never a position.
//! - A drawing object (shape, chart, picture) is `drawing:<id>`, its id unique in the workbook and
//!   kept through save and reopen. Commands' `id` and `ids` take the tool id beside the number.
//! - Cells are addressed by sheet and A1 range, a position: inserting or deleting rows or columns
//!   moves the cells after them, and their addresses with them.

use kition_sheet_engine::model::{Sheet, Visibility, Workbook};
use serde_json::{Value as Json, json};

/// The most drawing objects listed for one sheet.
const DRAWINGS_LISTED: usize = 200;

/// An id as the kit's tools write it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolId {
    Sheet(u32),
    Drawing(u32),
}

impl ToolId {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let number = |text: &str| text.trim().parse::<u32>().ok();
        if let Some(rest) = text.strip_prefix("sheet:") {
            return number(rest).map(Self::Sheet);
        }
        text.strip_prefix("drawing:").and_then(number).map(Self::Drawing)
    }
}

/// A sheet's id, as its tool id ends.
pub(crate) fn sheet_id(sheet: &Sheet) -> u32 {
    sheet.sheet_id
}

/// The tool id of a sheet.
pub(crate) fn sheet_tool_id(sheet: &Sheet) -> String {
    format!("sheet:{}", sheet_id(sheet))
}

/// The index of the sheet with id `id`.
pub(crate) fn sheet_index(wb: &Workbook, id: u32) -> Option<usize> {
    wb.sheets.iter().position(|sheet| sheet_id(sheet) == id)
}

/// The sheet holding drawing object `id` (a shape, a group, a chart or a picture), and what it is.
pub(crate) fn drawing_sheet(wb: &Workbook, id: u32) -> Option<(usize, &'static str)> {
    wb.sheets.iter().enumerate().find_map(|(index, sheet)| drawing_kind(sheet, id).map(|kind| (index, kind)))
}

fn drawing_kind(sheet: &Sheet, id: u32) -> Option<&'static str> {
    if sheet.charts.iter().any(|chart| chart.id == id) {
        Some("chart")
    } else if sheet.images.iter().any(|image| image.id == id) {
        Some("picture")
    } else if sheet.shapes.iter().any(|shape| shape.id == id) {
        Some("shape")
    } else if sheet.shapes.iter().any(|shape| shape.groups.first() == Some(&id)) {
        Some("group")
    } else {
        None
    }
}

/// Every sheet of the workbook, in tab order, with its id, tool id, name and position (from 0),
/// whether it is hidden, its used range, and its drawing objects by id: what an agent names sheets
/// and objects by. None of it depends on the active sheet or the selection.
pub(crate) fn workbook_json(wb: &Workbook) -> Json {
    let sheets: Vec<Json> = wb
        .sheets
        .iter()
        .enumerate()
        .map(|(index, sheet)| {
            let mut drawings: Vec<Json> = Vec::new();
            let mut groups: Vec<u32> = Vec::new();
            for shape in &sheet.shapes {
                if let Some(&group) = shape.groups.first()
                    && !groups.contains(&group)
                {
                    groups.push(group);
                    drawings.push(drawing_json(group, "group", shape.anchor.cell.a1()));
                }
                drawings.push(drawing_json(shape.id, "shape", shape.anchor.cell.a1()));
            }
            drawings.extend(sheet.charts.iter().map(|chart| drawing_json(chart.id, "chart", chart.anchor.cell.a1())));
            drawings.extend(sheet.images.iter().map(|image| drawing_json(image.id, "picture", image.anchor.cell.a1())));
            let count = drawings.len();
            drawings.truncate(DRAWINGS_LISTED);
            let mut out = json!({
                "id": sheet_id(sheet),
                "toolId": sheet_tool_id(sheet),
                "name": sheet.name,
                "index": index,
                "hidden": sheet.visibility != Visibility::Visible,
                "used": sheet.used_range().map(|range| range.a1()),
                "drawings": drawings,
            });
            if count > DRAWINGS_LISTED {
                out["moreDrawings"] = json!(count - DRAWINGS_LISTED);
            }
            out
        })
        .collect();
    json!({ "sheets": sheets })
}

fn drawing_json(id: u32, kind: &str, at: String) -> Json {
    json!({ "id": id, "toolId": format!("drawing:{id}"), "kind": kind, "at": at })
}
