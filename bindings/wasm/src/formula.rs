//! Reference decoration for drafts, shared by the cell editor and formula bar. The formula
//! lexer keeps strings and function names out of the reference set, even in unfinished edits.

use std::collections::HashMap;

use kition_sheet_engine::core::RangeRef;
use kition_sheet_engine::formula::{Expr, SheetSel, lexer, parse};
use serde::Serialize;

use crate::{RichText, RunColor, RunFont, TextRun};

const COLORS: [[u8; 4]; 6] =
    [[43, 92, 217, 255], [211, 47, 77, 255], [26, 134, 65, 255], [132, 63, 191, 255], [176, 95, 13, 255], [0, 128, 139, 255]];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Reference {
    pub text: String,
    /// UTF-16 offsets for the browser input. Byte offsets stay private to Rust.
    pub start: usize,
    pub end: usize,
    pub color: [u8; 4],
    #[serde(skip)]
    pub range: RangeRef,
    #[serde(skip)]
    pub local: bool,
    #[serde(skip)]
    bytes: (usize, usize),
}

/// `home`: the sheet the formula belongs to (its plain references are there); `shown`: the sheet
/// on screen, whose references are outlined (`local`).
pub(crate) fn references(text: &str, home: &str, shown: &str) -> Vec<Reference> {
    let Some(body) = text.strip_prefix('=') else { return Vec::new() };
    // Preserve complete tokens before an unfinished string, sheet prefix, or structured ref.
    let tokens = lexer::tokenize(body).or_else(|e| lexer::tokenize(body.get(..e.pos).unwrap_or_default())).unwrap_or_default();
    let tokens: Vec<_> = tokens.iter().filter(|t| !matches!(t.tok, lexer::Tok::Space | lexer::Tok::Eof)).collect();
    let mut colors = HashMap::new();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(first) = tokens.get(i) {
        let mut last = i;
        if matches!(first.tok, lexer::Tok::Sheet(..)) {
            last += 1;
        }
        let Some(cell) = tokens.get(last) else { break };
        if !matches!(cell.tok, lexer::Tok::Word(_) | lexer::Tok::Number(_)) {
            i = last + 1;
            continue;
        }
        if tokens.get(last + 1).is_some_and(|t| matches!(t.tok, lexer::Tok::Colon))
            && tokens.get(last + 2).is_some_and(|t| matches!(t.tok, lexer::Tok::Word(_) | lexer::Tok::Number(_)))
        {
            last += 2;
        }
        let end = tokens.get(last).map_or(cell.end, |t| t.end);
        let source = body.get(first.start..end).unwrap_or_default();
        if let Ok(Expr::Ref(reference)) = parse(source) {
            let (name, local) = match &reference.sheet {
                SheetSel::Current => (home.to_lowercase(), home.eq_ignore_ascii_case(shown)),
                SheetSel::Named(name) => (name.to_lowercase(), name.eq_ignore_ascii_case(shown)),
                SheetSel::Span(a, b) => (format!("{}:{}", a.to_lowercase(), b.to_lowercase()), false),
            };
            let range = reference.range();
            let next = colors.len() % COLORS.len();
            let index = *colors.entry((name, range.start, range.end)).or_insert(next);
            let (start, end) = (first.start + 1, end + 1);
            out.push(Reference {
                text: source.to_owned(),
                start: text.get(..start).unwrap_or_default().encode_utf16().count(),
                end: text.get(..end).unwrap_or_default().encode_utf16().count(),
                color: COLORS.get(index).copied().unwrap_or(COLORS[0]),
                range,
                local,
                bytes: (start, end),
            });
        }
        i = last + 1;
    }
    out
}

/// Formula syntax colors are temporary, never saved as rich text in the workbook.
pub(crate) fn rich_text(text: &str, references: &[Reference]) -> RichText {
    let mut runs = Vec::new();
    let mut at = 0;
    for reference in references {
        let (start, end) = reference.bytes;
        if let Some(plain) = text.get(at..start).filter(|s| !s.is_empty()) {
            runs.push(TextRun::new(plain, RunFont::default()));
        }
        if let Some(value) = text.get(start..end) {
            let [r, g, b, _] = reference.color;
            let color = RunColor { rgb: Some(format!("FF{r:02X}{g:02X}{b:02X}")), ..Default::default() };
            runs.push(TextRun::new(value, RunFont { color: Some(color), ..Default::default() }));
        }
        at = end;
    }
    if let Some(plain) = text.get(at..).filter(|s| !s.is_empty()) {
        runs.push(TextRun::new(plain, RunFont::default()));
    }
    RichText { runs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_reference_lexer_handles_ranges_strings_anchors_and_unfinished_input() {
        let refs = references("=SUM($B$5:B10)+B5+\"C9\"+LOG10(10)+'Other Sheet'!A1+Sheet1!B5+\"", "Sheet1", "Sheet1");
        assert_eq!(refs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["$B$5:B10", "B5", "'Other Sheet'!A1", "Sheet1!B5"]);
        assert_ne!(refs[0].color, refs[1].color);
        assert!(!refs[2].local);
        assert_eq!(refs[1].color, refs[3].color);
        let refs = references("=SUM(B:B,5:10)+B5+", "Sheet1", "Sheet1");
        assert_eq!(refs.len(), 3);
        assert!(refs[0].range.is_full_cols());
        assert!(refs[1].range.is_full_rows());
        assert!(references("B5", "Sheet1", "Sheet1").is_empty());
        assert!(references("=\"B5\"+Table1[B10]+XFE1", "Sheet1", "Sheet1").is_empty());
    }

    #[test]
    fn formula_reference_offsets_are_browser_utf16_units() {
        let refs = references("=\"\u{1f680}\"+B5", "Sheet1", "Sheet1");
        assert_eq!((refs[0].start, refs[0].end), (6, 8));
    }
}
