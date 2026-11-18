//! Wrapped cell text, laid out as WPS for Mac lays it out (module M9): the line breaks the grid,
//! the PDF export and row auto-fit all share, so a row is as tall as the lines it shows.
//!
//! - Latin words break at spaces; a word wider than the line breaks between its letters.
//! - Chinese, Japanese and Korean text breaks between any two characters, except that a line
//!   never starts with closing punctuation (`，。、；：？！）」』】》` …) and never ends with
//!   opening punctuation (`（「『【《` …), the line-breaking rules Chinese typesetting uses.
//! - Spaces at the end of a line take no room.

use kition_sheet_model::Font;

/// Lines of a cell wrap inside its width less this padding (both sides, px at 96 dpi; about
/// 4.9 pt, fitted to WPS's line breaks in the golden workbooks).
pub const CELL_PADDING_PX: f32 = 6.5;

/// Characters that may not start a line.
fn no_line_start(c: char) -> bool {
    matches!(
        c,
        '，' | '。'
            | '、'
            | '；'
            | '：'
            | '？'
            | '！'
            | '）'
            | '」'
            | '』'
            | '】'
            | '》'
            | '〉'
            | '”'
            | '’'
            | '…'
            | '·'
            | '%'
            | '％'
            | ','
            | '.'
            | ';'
            | ':'
            | '!'
            | '?'
            | ')'
            | ']'
            | '}'
            | '〕'
            | '］'
            | '｝'
            | 'ー'
            | '～'
            | '℃'
    )
}

/// Characters that may not end a line.
fn no_line_end(c: char) -> bool {
    matches!(c, '（' | '「' | '『' | '【' | '《' | '〈' | '“' | '‘' | '(' | '[' | '{' | '〔' | '［' | '｛' | '￥' | '$')
}

/// Whether text may break between any two of these characters (ideographs, kana, Hangul,
/// full-width forms and CJK punctuation).
pub fn is_cjk(c: char) -> bool {
    matches!(u32::from(c), 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

/// Whether a line may break between `a` and `b` (after `a`).
fn may_break(a: char, b: char) -> bool {
    if no_line_start(b) || no_line_end(a) {
        return false;
    }
    a == ' ' || is_cjk(a) || is_cjk(b)
}

/// Most lines one cell's text wraps to (bounds the work on hostile input).
const MAX_LINES: usize = 4096;

/// `text` broken into lines no wider than `width` (px at 96 dpi) in `font`; each paragraph
/// (`\n`) starts a line. Never empty: an empty text is one empty line.
pub fn wrap_lines(text: &str, font: &Font, width: f32) -> Vec<String> {
    wrap_with(text, width, &|c| crate::metrics::char_width(c, font))
}

/// [`wrap_lines`] with a measure of one character's advance.
pub fn wrap_with(text: &str, width: f32, advance: &dyn Fn(char) -> f32) -> Vec<String> {
    let width = if width.is_finite() { width.max(1.0) } else { 1.0 };
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let chars: Vec<char> = paragraph.trim_end_matches('\r').chars().collect();
        let mut start = 0;
        while start < chars.len() {
            if out.len() >= MAX_LINES {
                return out;
            }
            // Take characters while they fit (trailing spaces always fit).
            let mut used = 0.0f32;
            let mut end = start;
            let mut last_break: Option<usize> = None;
            while let Some(&c) = chars.get(end) {
                let w = advance(c);
                if c != ' ' && used + w > width && end > start {
                    break;
                }
                used += w;
                end += 1;
                if let (Some(&a), Some(&b)) = (chars.get(end - 1), chars.get(end))
                    && may_break(a, b)
                {
                    last_break = Some(end);
                }
            }
            if end < chars.len() {
                // Break at the last opportunity on the line, else between characters (keeping
                // closing punctuation with the character before it when that still fits).
                if let Some(at) = last_break.filter(|at| *at > start) {
                    end = at;
                } else if end > start + 1 && chars.get(end).is_some_and(|c| no_line_start(*c)) {
                    end -= 1;
                }
            }
            let line: String = chars.get(start..end).unwrap_or_default().iter().collect();
            out.push(line.trim_end_matches(' ').to_owned());
            start = end;
            // Spaces a break leaves at the start of the next line are dropped.
            while chars.get(start) == Some(&' ') {
                start += 1;
            }
        }
        if chars.is_empty() || out.is_empty() {
            out.push(String::new());
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn em(_: char) -> f32 {
        10.0
    }

    #[test]
    fn chinese_breaks_between_characters_but_not_before_closing_punctuation() {
        assert_eq!(wrap_with("一二三四五六", 30.0, &em), ["一二三", "四五六"]);
        // "，" may not start a line: it stays with the character before it, which moves down.
        assert_eq!(wrap_with("一二三，四五", 30.0, &em), ["一二", "三，四", "五"]);
        // "（" may not end a line.
        assert_eq!(wrap_with("一二（三）", 30.0, &em), ["一二", "（三）"]);
    }

    #[test]
    fn latin_words_break_at_spaces() {
        assert_eq!(wrap_with("ab cd ef", 50.0, &em), ["ab cd", "ef"]);
        assert_eq!(wrap_with("abcdefgh", 30.0, &em), ["abc", "def", "gh"]);
        assert_eq!(wrap_with("第一\n第二", 100.0, &em), ["第一", "第二"]);
        assert_eq!(wrap_with("", 100.0, &em), [""]);
        // Hostile widths and lengths stay bounded.
        assert_eq!(wrap_with("abc", f32::NAN, &em).len(), 3);
        assert!(wrap_with(&"字".repeat(100_000), 1.0, &em).len() <= MAX_LINES);
    }
}
