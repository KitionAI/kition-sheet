//! Formula entry in the cell editor and the formula bar, as WPS and Excel offer it: pointing at
//! cells to insert their references (point mode), F4's cycle through absolute and relative
//! references, and the function list and argument hint shown while a formula is typed. Positions
//! are in characters (the cell editor's); the formula bar converts from and to UTF-16.

use std::sync::OnceLock;

use kition_sheet_engine::core::{CellRef, RangeRef, col_to_letters};
use serde::Serialize;

/// Whether a reference can be inserted at `caret` in `chars` by pointing: in a formula, outside a
/// string, right after `=`, an operator, an opening parenthesis, a brace or an argument separator.
pub fn can_point(chars: &[char], caret: usize) -> bool {
    if chars.first() != Some(&'=') || in_string(chars, caret) {
        return false;
    }
    let before = chars.get(..caret.min(chars.len())).and_then(|head| head.iter().rev().find(|c| !c.is_whitespace()));
    matches!(before, Some('=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | ';' | '{'))
}

/// Whether `caret` is inside a string literal of a formula.
fn in_string(chars: &[char], caret: usize) -> bool {
    chars.iter().take(caret).filter(|c| **c == '"').count() % 2 == 1
}

/// The text of the range `a`:`b` (in either order), its sheet named when `sheet` is given.
pub fn range_text(sheet: Option<&str>, a: CellRef, b: CellRef, whole: Option<bool>) -> String {
    let r = RangeRef::new(a, b);
    let body = match whole {
        // Whole columns (`B:C`) or whole rows (`3:4`), from a heading.
        Some(true) => format!("{}:{}", col_to_letters(r.start.col), col_to_letters(r.end.col)),
        Some(false) => format!("{}:{}", r.start.row + 1, r.end.row + 1),
        None if r.is_single() => r.start.a1(),
        None => format!("{}:{}", r.start.a1(), r.end.a1()),
    };
    match sheet {
        Some(name) => format!("{}!{body}", kition_sheet_engine::formula::quote_sheet(name)),
        None => body,
    }
}

/// One cell of a reference as typed: `$A$1`.
struct CellText {
    col_abs: bool,
    col: String,
    row_abs: bool,
    row: String,
}

fn cell_text(s: &str) -> Option<CellText> {
    let (col_abs, s) = match s.strip_prefix('$') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let letters = s.chars().take_while(char::is_ascii_alphabetic).count();
    let (col, s) = s.split_at(letters);
    let (row_abs, row) = match s.strip_prefix('$') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let valid = (1..=3).contains(&letters)
        && !row.is_empty()
        && row.chars().all(|c| c.is_ascii_digit())
        && kition_sheet_engine::core::letters_to_col(col).is_some_and(|c| c < kition_sheet_engine::core::MAX_COLS);
    valid.then(|| CellText { col_abs, col: col.to_ascii_uppercase(), row_abs, row: row.to_string() })
}

/// F4: the reference at or just before `caret` cycles `A1` → `$A$1` → `A$1` → `$A1` → `A1` (both
/// ends of a range together, as the first one was). The new text and the reference's new span.
pub fn cycle_reference(chars: &[char], from: usize, to: usize) -> Option<(String, usize, usize)> {
    if chars.first() != Some(&'=') || in_string(chars, from) {
        return None;
    }
    let part = |c: &char| c.is_ascii_alphanumeric() || *c == '$' || *c == ':';
    let caret = to.min(chars.len());
    // The reference the caret is in or right after (a selection: the one it starts in).
    let probe = if from < to { from } else { caret };
    let mut start = probe;
    while start > 0 && chars.get(start - 1).is_some_and(part) {
        start -= 1;
    }
    let mut end = probe;
    while chars.get(end).is_some_and(part) {
        end += 1;
    }
    if start == end || chars.get(end) == Some(&'(') || chars.get(start.wrapping_sub(1)).is_some_and(|c| *c == '.' || *c == '_' || *c == '[') {
        return None;
    }
    let text: String = chars.get(start..end)?.iter().collect();
    let cells: Vec<CellText> = text.split(':').map(cell_text).collect::<Option<_>>()?;
    if cells.is_empty() || cells.len() > 2 {
        return None;
    }
    let first = cells.first()?;
    let (col_abs, row_abs) = match (first.col_abs, first.row_abs) {
        (false, false) => (true, true),
        (true, true) => (false, true),
        (false, true) => (true, false),
        (true, false) => (false, false),
    };
    let dollar = |on: bool| if on { "$" } else { "" };
    let new: Vec<String> = cells.iter().map(|c| format!("{}{}{}{}", dollar(col_abs), c.col, dollar(row_abs), c.row)).collect();
    let new = new.join(":");
    let mut out: String = chars.get(..start)?.iter().collect();
    out.push_str(&new);
    out.extend(chars.get(end..)?.iter());
    let new_end = start + new.chars().count();
    Some((out, start, new_end))
}

/// A function the list offers: its name, its syntax and what it does.
#[derive(Clone, Serialize)]
pub struct FunctionInfo {
    pub name: String,
    pub signature: String,
    pub description: String,
}

/// Every worksheet function, by name.
fn functions() -> &'static [FunctionInfo] {
    static LIST: OnceLock<Vec<FunctionInfo>> = OnceLock::new();
    LIST.get_or_init(|| {
        kition_sheet_engine::cmd::formulas::function_list()
            .into_iter()
            .map(|f| FunctionInfo {
                name: f["name"].as_str().unwrap_or_default().to_string(),
                signature: f["signature"].as_str().unwrap_or_default().to_string(),
                description: f["description"].as_str().unwrap_or_default().to_string(),
            })
            .filter(|f| !f.name.is_empty())
            .collect()
    })
}

/// What to show while a formula is typed: the functions whose names start with the word being
/// typed (`prefix`: where that word is), and the function whose arguments the caret is in, with
/// the argument it is at.
#[derive(Default, Serialize)]
pub struct Hint {
    #[serde(skip)]
    pub prefix: Option<(usize, usize)>,
    pub functions: Vec<FunctionInfo>,
    pub call: Option<Call>,
}

#[derive(Serialize)]
pub struct Call {
    pub name: String,
    pub signature: String,
    pub description: String,
    pub argument: usize,
    /// The argument's place in `signature` (UTF-16), to show it in bold.
    pub bold: Option<(usize, usize)>,
}

/// Most names the list shows.
const LIST_LIMIT: usize = 60;

pub fn hint(chars: &[char], caret: usize) -> Hint {
    let mut out = Hint::default();
    if chars.first() != Some(&'=') {
        return out;
    }
    let caret = caret.min(chars.len());
    if !in_string(chars, caret) {
        let word = |c: &char| c.is_ascii_alphanumeric() || *c == '.' || *c == '_';
        let mut start = caret;
        while start > 1 && chars.get(start - 1).is_some_and(word) {
            start -= 1;
        }
        let before = chars.get(start.wrapping_sub(1)).copied();
        let opens = matches!(before, Some('=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ';' | '{' | ' '));
        let typed: String = chars.get(start..caret).unwrap_or_default().iter().collect();
        if start < caret && opens && chars.get(start).is_some_and(char::is_ascii_alphabetic) && chars.get(caret) != Some(&'(') {
            let upper = typed.to_ascii_uppercase();
            out.functions = functions().iter().filter(|f| f.name.starts_with(&upper)).take(LIST_LIMIT).cloned().collect();
            if !out.functions.is_empty() {
                out.prefix = Some((start, caret));
            }
        }
    }
    out.call = call_at(chars, caret);
    out
}

/// The innermost function call the caret is in, and which of its arguments.
fn call_at(chars: &[char], caret: usize) -> Option<Call> {
    // (name, argument) per open parenthesis; `None` names a brace (an array constant).
    let mut stack: Vec<(Option<String>, usize)> = Vec::new();
    let mut quoted = false;
    for (i, c) in chars.iter().enumerate().take(caret).skip(1) {
        match c {
            '"' => quoted = !quoted,
            _ if quoted => {}
            '(' => {
                let mut s = i;
                while s > 1 && chars.get(s - 1).is_some_and(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_') {
                    s -= 1;
                }
                let name: String = chars.get(s..i).unwrap_or_default().iter().collect();
                stack.push((Some(name.to_ascii_uppercase()), 0));
            }
            '{' => stack.push((None, 0)),
            ')' | '}' => {
                stack.pop();
            }
            ',' => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            _ => {}
        }
    }
    let (name, argument) = stack.iter().rev().find_map(|(name, arg)| name.as_ref().filter(|n| !n.is_empty()).map(|n| (n.clone(), *arg)))?;
    let f = functions().iter().find(|f| f.name == name)?;
    Some(Call { bold: bold(&f.signature, argument), name, signature: f.signature.clone(), description: f.description.clone(), argument })
}

/// Where argument `argument` is in a signature such as `SUM(number1, [number2], ...)`: past the
/// listed ones, the one repeated before `...`.
fn bold(signature: &str, argument: usize) -> Option<(usize, usize)> {
    let open = signature.find('(')?;
    let close = signature.rfind(')')?;
    let inner = signature.get(open + 1..close)?;
    let mut parts: Vec<(usize, &str)> = Vec::new();
    let mut at = open + 1;
    for p in inner.split(',') {
        let trimmed = p.trim_start();
        parts.push((at + (p.len() - trimmed.len()), trimmed.trim_end()));
        at += p.len() + 1;
    }
    let pick = match parts.get(argument) {
        Some((_, p)) if *p != "..." => parts.get(argument),
        _ => parts.iter().position(|(_, p)| *p == "...").and_then(|i| i.checked_sub(1)).and_then(|i| parts.get(i)),
    }?;
    let utf16 = |bytes: usize| signature.get(..bytes).unwrap_or_default().encode_utf16().count();
    Some((utf16(pick.0), utf16(pick.0 + pick.1.len())))
}

/// `name(` in place of the word being typed (`prefix`), the caret after the parenthesis.
pub fn accept(chars: &[char], prefix: (usize, usize), name: &str) -> (String, usize) {
    let (start, end) = (prefix.0.min(chars.len()), prefix.1.min(chars.len()));
    let mut out: String = chars.get(..start).unwrap_or_default().iter().collect();
    out.push_str(name);
    out.push('(');
    let caret = start + name.chars().count() + 1;
    // An opening parenthesis already there is not doubled.
    let rest = chars.get(end..).unwrap_or_default();
    out.extend(if rest.first() == Some(&'(') { rest.get(1..).unwrap_or_default() } else { rest }.iter());
    (out, caret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn points_after_operators_only() {
        assert!(can_point(&c("="), 1));
        assert!(can_point(&c("=SUM("), 5));
        assert!(can_point(&c("=A1+"), 4));
        assert!(can_point(&c("=SUM(A1, "), 9));
        assert!(!can_point(&c("=A1"), 3));
        assert!(!can_point(&c("=\"a+"), 4));
        assert!(!can_point(&c("abc+"), 4));
        assert_eq!(range_text(None, CellRef::parse("C3").unwrap(), CellRef::parse("A1").unwrap(), None), "A1:C3");
        assert_eq!(range_text(Some("销售 表"), CellRef::parse("B2").unwrap(), CellRef::parse("B2").unwrap(), None), "'销售 表'!B2");
        assert_eq!(range_text(None, CellRef::new(0, 1), CellRef::new(9, 2), Some(true)), "B:C");
    }

    #[test]
    fn f4_cycles_the_reference_at_the_caret() {
        let step = |s: &str, at: usize| cycle_reference(&c(s), at, at).map(|(t, _, e)| (t, e));
        assert_eq!(step("=A1+B2", 3), Some(("=$A$1+B2".into(), 5)));
        assert_eq!(step("=$A$1+B2", 5), Some(("=A$1+B2".into(), 4)));
        assert_eq!(step("=A$1", 4), Some(("=$A1".into(), 4)));
        assert_eq!(step("=$A1", 4), Some(("=A1".into(), 3)));
        assert_eq!(step("=SUM(A1:B3)", 10), Some(("=SUM($A$1:$B$3)".into(), 14)));
        assert_eq!(step("=Sheet2!c5*2", 10), Some(("=Sheet2!$C$5*2".into(), 12)));
        assert_eq!(step("=LOG10(5)", 6), None);
        assert_eq!(step("=\"A1\"", 3), None);
        assert_eq!(step("A1", 2), None);
    }

    #[test]
    fn hints_list_functions_and_the_argument() {
        let h = hint(&c("=SU"), 3);
        assert!(h.functions.iter().any(|f| f.name == "SUM"));
        assert!(h.functions.iter().all(|f| f.name.starts_with("SU")));
        assert_eq!(h.prefix, Some((1, 3)));
        assert!(hint(&c("=\"su"), 4).functions.is_empty());
        assert!(hint(&c("su"), 2).functions.is_empty());
        let h = hint(&c("=IF(A1>0,SUM(B1,"), 16);
        let call = h.call.unwrap();
        assert_eq!((call.name.as_str(), call.argument), ("SUM", 1));
        let (a, b) = call.bold.unwrap();
        assert_eq!(&call.signature[a..b], "[number2]");
        let call = hint(&c("=IF(A1>0,"), 9).call.unwrap();
        assert_eq!((call.name.as_str(), call.argument), ("IF", 1));
        let call = hint(&c("=SUM({1,2},"), 11).call.unwrap();
        assert_eq!(call.argument, 1);
        let call = hint(&c("=SUM(1,2,3,4"), 12).call.unwrap();
        let (a, b) = call.bold.unwrap();
        assert_eq!(&call.signature[a..b], "[number2]");
        assert_eq!(accept(&c("=1+su*2"), (3, 5), "SUM"), ("=1+SUM(*2".into(), 7));
    }
}
