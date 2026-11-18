//! Form controls (check boxes, buttons, list boxes… drawn by the legacy VML drawing, described by
//! the `controls` element and its `ctrlProp` parts): kept verbatim so a saved file gives them
//! back, with what drawing them needs (kind, place, checked, caption).
//!
//! VML is not well-formed XML (Excel writes `<br>` without an end), so it is handled as text:
//! each `<v:shape>…</v:shape>` and `<v:shapetype>…</v:shapetype>` block is found and kept as
//! written.

use kition_sheet_core::CellRef;
use kition_sheet_model::FormControl;

/// Most controls read from one sheet.
const MAX_CONTROLS: usize = 2000;

/// Every element named `name` (any prefix) in `xml`, as written: from its start tag to its end
/// (or the start tag itself when it is empty).
pub(crate) fn raw_elements(xml: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while out.len() < MAX_CONTROLS {
        let Some(found) = xml.get(at..).and_then(|rest| rest.find('<')).map(|i| at + i) else { break };
        at = found + 1;
        let tag_end = xml.get(found + 1..).and_then(|rest| rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/')).map(|i| found + 1 + i);
        let Some(tag_end) = tag_end else { break };
        let tag = xml.get(found + 1..tag_end).unwrap_or("");
        if tag.rsplit(':').next() != Some(name) {
            continue;
        }
        let Some(close) = xml.get(found..).and_then(|rest| rest.find('>')).map(|i| found + i) else { break };
        if xml.get(close - 1..close) == Some("/") {
            out.push(xml.get(found..=close).unwrap_or("").to_owned());
            at = close + 1;
            continue;
        }
        let end_tag = format!("</{tag}>");
        let Some(end) = xml.get(close..).and_then(|rest| rest.find(&end_tag)).map(|i| close + i + end_tag.len()) else { break };
        out.push(xml.get(found..end).unwrap_or("").to_owned());
        at = end;
    }
    out
}

/// An attribute's value in a start tag (`name="…"`, any prefix on the name).
pub(crate) fn attr(element: &str, name: &str) -> Option<String> {
    let head = &element[..element.find('>').unwrap_or(element.len())];
    let mut rest = head;
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq].rsplit(|c: char| c.is_whitespace()).next().unwrap_or("");
        let after = &rest[eq + 2..];
        let value_end = after.find('"')?;
        if key.rsplit(':').next() == Some(name) {
            return Some(crate::xml::unescape(&after[..value_end]));
        }
        rest = &after[value_end + 1..];
    }
    None
}

/// The text inside an element (`<x:Checked>1</x:Checked>` → `1`), tags stripped.
fn inner_text(element: &str, name: &str) -> Option<String> {
    let block = raw_elements(element, name).into_iter().next()?;
    let start = block.find('>')? + 1;
    let end = block.rfind("</")?;
    let inner = block.get(start..end)?;
    let mut text = String::new();
    let mut in_tag = false;
    for c in inner.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    Some(crate::xml::unescape(text.trim()))
}

/// The form controls of a sheet's legacy drawing, matched with the sheet's `control` elements
/// (shape id → element) and their properties parts (relationship id → part).
pub(crate) fn read(vml: &str, controls: &[String], props: &dyn Fn(&str) -> Option<String>) -> Vec<FormControl> {
    let types = raw_elements(vml, "shapetype");
    let mut out = Vec::new();
    for shape in raw_elements(vml, "shape") {
        let Some(kind) = raw_elements(&shape, "ClientData").first().and_then(|d| attr(d, "ObjectType")) else { continue };
        if kind == "Note" {
            continue;
        }
        let shape_id = attr(&shape, "id").and_then(|id| id.trim_start_matches("_x0000_s").parse::<u32>().ok()).unwrap_or(0);
        let shape_type = attr(&shape, "type")
            .map(|t| t.trim_start_matches('#').to_owned())
            .and_then(|t| types.iter().find(|st| attr(st, "id").as_deref() == Some(t.as_str())).cloned())
            .unwrap_or_default();
        let anchor: Vec<f32> =
            inner_text(&shape, "Anchor").map(|a| a.split(',').filter_map(|n| n.trim().parse::<f32>().ok()).collect()).unwrap_or_default();
        let cell = |c: usize, r: usize| {
            CellRef::new(anchor.get(r).copied().unwrap_or(0.0).max(0.0) as u32, anchor.get(c).copied().unwrap_or(0.0).max(0.0) as u32)
        };
        let offset = |i: usize| anchor.get(i).copied().unwrap_or(0.0).clamp(0.0, 10_000.0);
        let control = controls.iter().find(|c| attr(c, "shapeId").and_then(|id| id.parse::<u32>().ok()) == Some(shape_id)).cloned();
        let props = control.as_deref().and_then(|c| attr(c, "id")).and_then(|rid| props(&rid));
        let checked = inner_text(&shape, "Checked").is_some_and(|v| v.trim() == "1")
            || props.as_deref().and_then(|p| attr(p, "checked")).is_some_and(|v| v.eq_ignore_ascii_case("checked"));
        let text = raw_elements(&shape, "textbox").first().and_then(|t| inner_text(t, "textbox")).unwrap_or_default();
        out.push(FormControl {
            vml: shape,
            shape_type,
            control,
            props,
            shape_id,
            kind,
            from: (cell(0, 2), offset(1), offset(3)),
            to: (cell(4, 6), offset(5), offset(7)),
            checked,
            text,
        });
        if out.len() >= MAX_CONTROLS {
            break;
        }
    }
    out
}

/// The VML shape blocks of the controls, the shape types they use (once each), and the shape id
/// blocks (1024 ids each) they take.
pub(crate) fn vml_parts(controls: &[FormControl]) -> (String, String, Vec<u32>) {
    let mut shapes = String::new();
    let mut types: Vec<&str> = Vec::new();
    let mut blocks = Vec::new();
    for c in controls {
        shapes.push_str(&c.vml);
        if !c.shape_type.is_empty() && !types.contains(&c.shape_type.as_str()) {
            types.push(&c.shape_type);
        }
        let block = c.shape_id / 1024;
        if !blocks.contains(&block) {
            blocks.push(block);
        }
    }
    (types.concat(), shapes, blocks)
}
