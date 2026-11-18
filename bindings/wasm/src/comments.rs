//! Comment threads a host keeps itself (Kit UI protocol 2.6 section 7k, capability
//! `comment-anchors`; Kition One): the editor asks the engine for the selection's anchor to send
//! with `comment.create` (`editor.commentAnchor`), and where the host's threads' anchors are now,
//! to mark them and to answer `comments/locate` (`editor.commentPlaces`).
//!
//! An anchor names its range by the replica's sheet, row and column objects (`kdm-model`'s
//! `anchors`), so it follows rows and columns inserted or deleted before it and its sheet's
//! renames and moves. Neither command changes the workbook.

use serde_json::{Value as Json, json};

use crate::SheetDocument;

/// At most this many anchors are looked for in one call (the host shows at most a few hundred).
const PLACES_LIMIT: usize = 1000;
/// The protocol's limit on an anchor's ids, and on a label's characters.
const IDS_LIMIT: usize = 256;
const LABEL_LIMIT: usize = 500;

/// A label the host shows as plain text (UI-V6): no control or bidirectional formatting
/// characters, at most 500 characters.
fn plain(text: &str) -> String {
    let bidi = |c: char| matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}');
    text.chars().filter(|c| !c.is_control() && !bidi(*c)).take(LABEL_LIMIT).collect()
}

impl SheetDocument {
    /// Runs `editor.commentAnchor` and `editor.commentPlaces`; `None` for any other command.
    pub(crate) fn comment_command(&mut self, command: &str, params: &Json) -> Option<Result<Json, String>> {
        match command {
            "editor.commentAnchor" => Some(self.comment_anchor()),
            "editor.commentPlaces" => Some(Ok(self.comment_places(params))),
            _ => None,
        }
    }

    /// The anchor of the selected range that holds the active cell, on the active sheet: its ids
    /// and a label (`Sheet1!B3:D7`). Makes the workbook's replica if it has none yet.
    fn comment_anchor(&mut self) -> Result<Json, String> {
        let (sheet, range, name) = {
            let st = self.doc_state()?;
            let selection = &st.selection;
            let range = selection.ranges.iter().rev().find(|range| range.contains(selection.active)).or(selection.ranges.last()).copied();
            let range = range.ok_or("nothing is selected")?;
            let name = st.wb.sheets.get(st.wb.active_sheet).map(|sheet| sheet.name.clone()).ok_or("the workbook has no sheets")?;
            (st.wb.active_sheet, range, name)
        };
        // The replica names the range's lines; it is made lazily, so make it now.
        self.session.version().map_err(crate::engine_error)?;
        let ids = self
            .session
            .kdm()
            .and_then(|kdm| kdm.comment_anchor(sheet, range))
            .ok_or("the workbook's cells have no objects to anchor a comment to")?;
        let label = plain(&format!("{}!{}", kition_sheet_engine::formula::printer::quote_sheet(&name), range.a1()));
        Ok(json!({ "anchor": { "ids": ids, "label": label } }))
    }

    /// Where each anchor of `params.anchors` (`{ ids }`, as `editor.commentAnchor` gave them) is
    /// now: its sheet, its range, and, on the active sheet, its rectangle in document pixels
    /// (`[x, y, width, height]`, the canvas the editor draws). An anchor not found (its sheet or
    /// its cells deleted, malformed, from another workbook) answers `found: false`.
    fn comment_places(&self, params: &Json) -> Json {
        let anchors = params.get("anchors").and_then(Json::as_array).map(Vec::as_slice).unwrap_or_default();
        let places: Vec<Json> = anchors.iter().take(PLACES_LIMIT).map(|anchor| self.comment_place(anchor)).collect();
        json!({ "places": places })
    }

    fn comment_place(&self, anchor: &Json) -> Json {
        let ids: Option<Vec<String>> = anchor
            .get("ids")
            .and_then(Json::as_array)
            .filter(|ids| ids.len() <= IDS_LIMIT)
            .and_then(|ids| ids.iter().map(|id| id.as_str().map(str::to_owned)).collect());
        let found = ids.and_then(|ids| self.session.kdm()?.find_comment_anchor(&ids));
        let Some((sheet, range)) = found else { return json!({ "found": false }) };
        let Ok(st) = self.doc_state() else { return json!({ "found": false }) };
        let Some(shown) = st.wb.sheets.get(sheet) else { return json!({ "found": false }) };
        let active = sheet == st.wb.active_sheet;
        let mut place = json!({ "found": true, "sheet": shown.name, "range": range.a1(), "active": active });
        if active {
            let (x0, y0, x1, y1) = Self::cell_rect(shown, range);
            place["rect"] = json!([x0, y0, x1 - x0, y1 - y0]);
        }
        place
    }
}
