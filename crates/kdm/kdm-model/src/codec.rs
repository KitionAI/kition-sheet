//! What the replica stores for a cell's input, a formula and a position (G2).
//!
//! **Cell inputs** are one string: `=` and a formula, or a literal. A number is Rust's shortest
//! round-trip form (`3.5`, `1e300`), `TRUE`/`FALSE` a boolean, `#DIV/0!`… an error, the empty
//! string no value; any other string is text, and text that would read as one of those (or starts
//! with `=` or `'`) is stored behind a `'`.
//!
//! **Formulas** are stored in *id form*: each reference of the parsed formula becomes a token
//! `⟦@<sheet>|<kind>|<lines>⟧` naming the referenced sheet by its object id and every row and
//! column by its line object (`$` keeps an anchor absolute; `#n` is a line by index, below the
//! materialised part of the sheet), the rest is the formula's own text. So inserting rows changes
//! no formula in the replica, and a formula written concurrently with an insert still names the
//! rows it meant. Reading renders each token back to A1 with the current positions: a reference to
//! a deleted sheet, row or column reads `#REF!`; a range whose first or last line was deleted
//! shrinks to the lines left. Kinds: `c` a cell (`r,c`), `r` a range (`r,c;r,c`), `R` whole rows
//! (`r;r`), `C` whole columns (`c;c`). The sheet is empty (the formula's own sheet), an object id,
//! a quoted name (`'Name'`, a sheet that does not exist) or a 3-D span (`a~b`, whose lines are by
//! index). A formula that does not parse, that is not in the engine's canonical spelling, or whose
//! text holds `⟦` or `⟧` is stored as written (behind a `⟧` in the last case).
//!
//! **Positions** in features are tokens of their own (`r,c` a cell, `r;r,c;c` a range with `*`
//! for a whole axis, `r` a line), with the same line tokens.

use std::sync::Arc;

use kition_sheet_core::{CellError, CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value};
use kition_sheet_formula::ast::{Anchor, RefKind, Reference, SheetSel};
use kition_sheet_formula::{Expr, parse, print, printer};
use kition_sheet_model::{Cell, Formula, StyleId};

use crate::KdmResult;
use crate::lines::{Axis, LineAt, Use, line, span};

pub(crate) const OPEN: char = '\u{27E6}';
pub(crate) const CLOSE: char = '\u{27E7}';

/// What encoding needs from the writer: sheets by name, and the token of a line (materialising
/// it as `purpose` allows).
pub(crate) trait Encode {
    fn sheet_index(&self, name: &str) -> Option<usize>;
    fn sheet_object(&self, sheet: usize) -> Option<String>;
    fn line(&mut self, sheet: usize, axis: Axis, index: u32, purpose: Use) -> KdmResult<String>;
    /// The `styles` key of a style of the view's table (`None`: the default style).
    fn style_key(&self, id: StyleId) -> Option<String>;
}

/// What decoding needs from the reader.
pub(crate) trait Decode {
    fn sheet_of_object(&self, object: &str) -> Option<usize>;
    fn sheet_name(&self, sheet: usize) -> Option<&str>;
    fn line_at(&self, sheet: usize, axis: Axis, token: &str) -> LineAt;
    /// The read table's style of a `styles` key (the default style for a key it lacks).
    fn style_id(&self, key: &str) -> StyleId;
}

// ---------------------------------------------------------------- literals

fn needs_quote(t: &str) -> bool {
    t.is_empty()
        || t.starts_with(['=', '\''])
        || t.eq_ignore_ascii_case("TRUE")
        || t.eq_ignore_ascii_case("FALSE")
        || CellError::parse(t).is_some()
        || t.parse::<f64>().is_ok()
}

/// A literal cell value as stored.
pub(crate) fn literal(v: &Value) -> String {
    match v {
        Value::Empty => String::new(),
        Value::Number(n) => format!("{n:?}"),
        Value::Bool(b) => (if *b { "TRUE" } else { "FALSE" }).to_owned(),
        Value::Error(e) => e.as_str().to_owned(),
        Value::Text(t) if needs_quote(t) => format!("'{t}"),
        Value::Text(t) => t.to_string(),
        // Arrays are formula results, never literals; keep their first value.
        Value::Array(a) => a.data.first().map(literal).unwrap_or_default(),
    }
}

/// A stored literal back as a value.
pub(crate) fn parse_literal(s: &str) -> Value {
    if s.is_empty() {
        return Value::Empty;
    }
    if let Some(t) = s.strip_prefix('\'') {
        return Value::Text(Arc::from(t));
    }
    match s {
        "TRUE" => return Value::Bool(true),
        "FALSE" => return Value::Bool(false),
        _ => {}
    }
    if let Some(e) = CellError::ALL.iter().chain([CellError::Circ].iter()).find(|e| e.as_str() == s) {
        return Value::Error(*e);
    }
    match s.parse::<f64>() {
        Ok(n) => Value::Number(n),
        Err(_) => Value::Text(Arc::from(s)),
    }
}

/// A cell's input as stored: its formula in id form, or its literal value. `host` is its sheet.
pub(crate) fn encode_input<E: Encode + ?Sized>(e: &mut E, cell: &Cell, host: usize) -> KdmResult<String> {
    match &cell.formula {
        Some(f) => Ok(format!("={}", encode_formula(e, &f.text, f.expr.as_deref(), Some(host))?)),
        None => Ok(literal(&cell.value)),
    }
}

/// A stored input back as a cell's value and formula (a formula's value is left empty: results
/// are not stored).
pub(crate) fn decode_input<D: Decode + ?Sized>(d: &D, input: &str, host: usize) -> (Value, Option<Formula>) {
    match input.strip_prefix('=') {
        Some(body) => {
            let text = decode_formula(d, body, Some(host));
            let expr = parse(&text).ok().map(Arc::new);
            (Value::Empty, Some(Formula { text, expr, array: None }))
        }
        None => (parse_literal(input), None),
    }
}

// ---------------------------------------------------------------- formulas

fn quote(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

fn unquote(s: &str) -> Option<String> {
    let inner = s.strip_prefix('\'')?.strip_suffix('\'')?;
    Some(inner.replace("''", "'"))
}

/// The id form of formula `text` (without `=`), whose parse is `expr` when the caller has it, on
/// sheet `host` (`None`: no sheet, as a workbook-scope name: its unqualified references stay by
/// index). See the module docs.
pub(crate) fn encode_formula<E: Encode + ?Sized>(e: &mut E, text: &str, expr: Option<&Expr>, host: Option<usize>) -> KdmResult<String> {
    if text.contains(OPEN) || text.contains(CLOSE) {
        return Ok(format!("{CLOSE}{text}"));
    }
    let parsed;
    let expr = match expr {
        Some(x) => x,
        None => match parse(text) {
            Ok(x) => {
                parsed = x;
                &parsed
            }
            Err(_) => return Ok(text.to_owned()),
        },
    };
    if print(expr) != text {
        return Ok(text.to_owned());
    }
    let mut refs: Vec<Reference> = Vec::new();
    let mapped = expr.clone().map(&mut |x| match x {
        Expr::Ref(r) => {
            let k = refs.len();
            refs.push(r);
            Expr::Name(format!("{OPEN}{k}{CLOSE}"))
        }
        other => other,
    });
    if refs.is_empty() {
        return Ok(text.to_owned());
    }
    let printed = print(&mapped);
    let mut out = String::with_capacity(printed.len() + refs.len() * 48);
    let mut rest = printed.as_str();
    while let Some(at) = rest.find(OPEN) {
        out.push_str(rest.get(..at).unwrap_or(""));
        let tail = rest.get(at + OPEN.len_utf8()..).unwrap_or("");
        let Some(end) = tail.find(CLOSE) else { return Ok(text.to_owned()) };
        let Some(r) = tail.get(..end).and_then(|k| k.parse::<usize>().ok()).and_then(|k| refs.get(k)) else { return Ok(text.to_owned()) };
        let token = ref_token(e, r, host)?;
        out.push_str(&token);
        rest = tail.get(end + CLOSE.len_utf8()..).unwrap_or("");
    }
    out.push_str(rest);
    Ok(out)
}

fn ref_token<E: Encode + ?Sized>(e: &mut E, r: &Reference, host: Option<usize>) -> KdmResult<String> {
    let sheet_of = |name: &str| -> (String, Option<usize>) {
        match e.sheet_index(name).and_then(|i| Some((e.sheet_object(i)?, i))) {
            Some((object, i)) => (object, Some(i)),
            None => (quote(name), None),
        }
    };
    let (sheet, lines) = match &r.sheet {
        SheetSel::Current => (String::new(), host),
        SheetSel::Named(n) => sheet_of(n),
        SheetSel::Span(a, b) => (format!("{}~{}", sheet_of(a).0, sheet_of(b).0), None),
    };
    let mut part = |axis: Axis, index: u32, abs: bool| -> KdmResult<String> {
        let line = match lines {
            Some(s) => e.line(s, axis, index, Use::Ref)?,
            None => format!("#{index}"),
        };
        Ok(if abs { format!("${line}") } else { line })
    };
    let kind = match &r.kind {
        RefKind::Cell(a) => format!("c|{},{}", part(Axis::Rows, a.row, a.row_abs)?, part(Axis::Cols, a.col, a.col_abs)?),
        RefKind::Range(a, b) => format!(
            "r|{},{};{},{}",
            part(Axis::Rows, a.row, a.row_abs)?,
            part(Axis::Cols, a.col, a.col_abs)?,
            part(Axis::Rows, b.row, b.row_abs)?,
            part(Axis::Cols, b.col, b.col_abs)?
        ),
        RefKind::Rows(r0, a0, r1, a1) => format!("R|{};{}", part(Axis::Rows, *r0, *a0)?, part(Axis::Rows, *r1, *a1)?),
        RefKind::Cols(c0, a0, c1, a1) => format!("C|{};{}", part(Axis::Cols, *c0, *a0)?, part(Axis::Cols, *c1, *a1)?),
    };
    Ok(format!("{OPEN}@{sheet}|{kind}{CLOSE}"))
}

/// Formula text (without `=`) back from its id form, with the current positions (`host`: the
/// formula's sheet). See the module docs.
pub(crate) fn decode_formula<D: Decode + ?Sized>(d: &D, stored: &str, host: Option<usize>) -> String {
    if let Some(raw) = stored.strip_prefix(CLOSE) {
        return raw.to_owned();
    }
    if !stored.contains(OPEN) {
        return stored.to_owned();
    }
    let mut out = String::with_capacity(stored.len());
    let mut rest = stored;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(rest.get(..at).unwrap_or(""));
        let tail = rest.get(at + OPEN.len_utf8()..).unwrap_or("");
        let Some(end) = tail.find(CLOSE) else {
            out.push_str("#REF!");
            rest = "";
            break;
        };
        let rendered = tail.get(..end).and_then(|token| render(d, token, host)).unwrap_or_else(|| "#REF!".to_owned());
        rest = tail.get(end + CLOSE.len_utf8()..).unwrap_or("");
        // `A1#` of a reference that is gone reads `#REF!`, as the engine prints it.
        if rendered == "#REF!"
            && let Some(after) = rest.strip_prefix('#')
        {
            rest = after;
        }
        out.push_str(&rendered);
    }
    out.push_str(rest);
    out
}

/// `s` split at its first `sep` outside `'…'` quotes.
fn split_outside_quotes(s: &str, sep: char) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (i, ch) in s.char_indices() {
        if ch == '\'' {
            quoted = !quoted;
        } else if ch == sep && !quoted {
            return Some((s.get(..i)?, s.get(i + ch.len_utf8()..)?));
        }
    }
    None
}

/// One end of a reference: whether it is absolute, and where its line is.
fn end<D: Decode + ?Sized>(d: &D, sheet: Option<usize>, axis: Axis, token: &str) -> (bool, LineAt) {
    let (abs, token) = match token.strip_prefix('$') {
        Some(t) => (true, t),
        None => (false, token),
    };
    let at = match sheet {
        Some(s) => d.line_at(s, axis, token),
        None => match token.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => LineAt::Index(n),
            None => LineAt::Missing,
        },
    };
    (abs, at)
}

/// A single line of a cell reference: a deleted one is `#REF!`.
fn exact(at: LineAt, max: u32) -> Option<u32> {
    match at {
        LineAt::Live(i) | LineAt::Index(i) if i < max => Some(i),
        _ => None,
    }
}

fn render<D: Decode + ?Sized>(d: &D, token: &str, host: Option<usize>) -> Option<String> {
    let body = token.strip_prefix('@')?;
    let (sheet_part, rest) = split_outside_quotes(body, '|')?;
    let (kind, parts) = rest.split_once('|')?;
    let name_of = |part: &str| -> Option<(String, Option<usize>)> {
        if part.starts_with('\'') {
            return Some((unquote(part)?, None));
        }
        let i = d.sheet_of_object(part)?;
        Some((d.sheet_name(i)?.to_owned(), Some(i)))
    };
    let (sheet, lines) = if sheet_part.is_empty() {
        (SheetSel::Current, host)
    } else if let Some((a, b)) = split_outside_quotes(sheet_part, '~') {
        (SheetSel::Span(name_of(a)?.0, name_of(b)?.0), None)
    } else {
        let (name, i) = name_of(sheet_part)?;
        (SheetSel::Named(name), i)
    };
    let kind = match kind {
        "c" => {
            let (r, c) = parts.split_once(',')?;
            let ((ra, rat), (ca, cat)) = (end(d, lines, Axis::Rows, r), end(d, lines, Axis::Cols, c));
            RefKind::Cell(Anchor { row: exact(rat, MAX_ROWS)?, col: exact(cat, MAX_COLS)?, row_abs: ra, col_abs: ca })
        }
        "r" => {
            let (a, b) = parts.split_once(';')?;
            let (r0, c0) = a.split_once(',')?;
            let (r1, c1) = b.split_once(',')?;
            let ((ra0, rat0), (ca0, cat0)) = (end(d, lines, Axis::Rows, r0), end(d, lines, Axis::Cols, c0));
            let ((ra1, rat1), (ca1, cat1)) = (end(d, lines, Axis::Rows, r1), end(d, lines, Axis::Cols, c1));
            let (row0, row1) = span(rat0, rat1, MAX_ROWS)?;
            let (col0, col1) = span(cat0, cat1, MAX_COLS)?;
            RefKind::Range(Anchor { row: row0, col: col0, row_abs: ra0, col_abs: ca0 }, Anchor { row: row1, col: col1, row_abs: ra1, col_abs: ca1 })
        }
        "R" => {
            let (a, b) = parts.split_once(';')?;
            let ((a0, at0), (a1, at1)) = (end(d, lines, Axis::Rows, a), end(d, lines, Axis::Rows, b));
            let (r0, r1) = span(at0, at1, MAX_ROWS)?;
            RefKind::Rows(r0, a0, r1, a1)
        }
        "C" => {
            let (a, b) = parts.split_once(';')?;
            let ((a0, at0), (a1, at1)) = (end(d, lines, Axis::Cols, a), end(d, lines, Axis::Cols, b));
            let (c0, c1) = span(at0, at1, MAX_COLS)?;
            RefKind::Cols(c0, a0, c1, a1)
        }
        _ => return None,
    };
    Some(printer::reference_a1(&Reference { sheet, kind }))
}

/// A feature's formula text, with or without a leading `=`, in id form.
pub(crate) fn encode_text<E: Encode + ?Sized>(e: &mut E, text: &str, host: Option<usize>) -> KdmResult<String> {
    match text.strip_prefix('=') {
        Some(body) => Ok(format!("={}", encode_formula(e, body, None, host)?)),
        None => encode_formula(e, text, None, host),
    }
}

/// [`encode_text`] back.
pub(crate) fn decode_text<D: Decode + ?Sized>(d: &D, stored: &str, host: Option<usize>) -> String {
    match stored.strip_prefix('=') {
        Some(body) => format!("={}", decode_formula(d, body, host)),
        None => decode_formula(d, stored, host),
    }
}

// ---------------------------------------------------------------- positions in features

/// A cell's token (`r,c`).
pub(crate) fn encode_cell<E: Encode + ?Sized>(e: &mut E, sheet: usize, c: CellRef, purpose: Use) -> KdmResult<String> {
    Ok(format!("{},{}", e.line(sheet, Axis::Rows, c.row, purpose)?, e.line(sheet, Axis::Cols, c.col, purpose)?))
}

fn encode_span<E: Encode + ?Sized>(e: &mut E, sheet: usize, axis: Axis, lo: u32, hi: u32, purpose: Use) -> KdmResult<String> {
    if lo == 0 && hi >= axis.max() - 1 {
        return Ok("*".to_owned());
    }
    Ok(format!("{};{}", e.line(sheet, axis, lo, purpose)?, e.line(sheet, axis, hi, purpose)?))
}

/// A range's token (`r;r,c;c`, `*` for a whole axis).
pub(crate) fn encode_range<E: Encode + ?Sized>(e: &mut E, sheet: usize, r: RangeRef, purpose: Use) -> KdmResult<String> {
    let rows = encode_span(e, sheet, Axis::Rows, r.start.row, r.end.row, purpose)?;
    let cols = encode_span(e, sheet, Axis::Cols, r.start.col, r.end.col, purpose)?;
    Ok(format!("{rows},{cols}"))
}

/// A cell back: a deleted line moves to the next live one.
pub(crate) fn decode_cell<D: Decode + ?Sized>(d: &D, sheet: usize, token: &str) -> Option<CellRef> {
    let (r, c) = token.split_once(',')?;
    Some(CellRef::new(line(d.line_at(sheet, Axis::Rows, r), MAX_ROWS)?, line(d.line_at(sheet, Axis::Cols, c), MAX_COLS)?))
}

fn decode_span<D: Decode + ?Sized>(d: &D, sheet: usize, axis: Axis, token: &str) -> Option<(u32, u32)> {
    if token == "*" {
        return Some((0, axis.max() - 1));
    }
    let (a, b) = token.split_once(';')?;
    let (lo, hi) = span(d.line_at(sheet, axis, a), d.line_at(sheet, axis, b), axis.max())?;
    Some((lo.min(hi), lo.max(hi)))
}

/// A range back, shrunk to its live lines; `None` when nothing is left of it.
pub(crate) fn decode_range<D: Decode + ?Sized>(d: &D, sheet: usize, token: &str) -> Option<RangeRef> {
    let (rows, cols) = token.split_once(',')?;
    let (r0, r1) = decode_span(d, sheet, Axis::Rows, rows)?;
    let (c0, c1) = decode_span(d, sheet, Axis::Cols, cols)?;
    Some(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)))
}

/// A line back: a deleted line is `None`.
pub(crate) fn decode_line<D: Decode + ?Sized>(d: &D, sheet: usize, axis: Axis, token: &str) -> Option<u32> {
    exact(d.line_at(sheet, axis, token), axis.max())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::lines::Lines;
    use kition_kit::kdm::ObjectId;

    /// Two sheets, `Data` (object `s1`) and `Other Sheet` (`s2`), each with lines `r0`, `r1`… and
    /// `c0`, `c1`…; `gone` lines are deleted.
    struct Book {
        lines: HashMap<(usize, Axis), Lines>,
        created: usize,
    }

    impl Book {
        fn new(gone: &[&str]) -> Book {
            let mut lines = HashMap::new();
            for s in 0..2 {
                for (axis, prefix) in [(Axis::Rows, "r"), (Axis::Cols, "c")] {
                    let order = (0..10)
                        .map(|i| format!("{prefix}{s}x{i}"))
                        .map(|id| (ObjectId::from_string(id.as_str()), gone.contains(&id.as_str())))
                        .collect();
                    lines.insert((s, axis), Lines::new(order));
                }
            }
            Book { lines, created: 0 }
        }
    }

    impl Encode for Book {
        fn sheet_index(&self, name: &str) -> Option<usize> {
            ["Data", "Other Sheet"].iter().position(|n| n.eq_ignore_ascii_case(name))
        }
        fn sheet_object(&self, sheet: usize) -> Option<String> {
            Some(format!("s{}", sheet + 1))
        }
        fn line(&mut self, sheet: usize, axis: Axis, index: u32, _purpose: Use) -> KdmResult<String> {
            let lines = &self.lines[&(sheet, axis)];
            Ok(match lines.live.get(index as usize) {
                Some(o) => o.as_str().to_owned(),
                None => {
                    self.created += 1;
                    format!("#{index}")
                }
            })
        }
        fn style_key(&self, _id: StyleId) -> Option<String> {
            None
        }
    }

    impl Decode for Book {
        fn sheet_of_object(&self, object: &str) -> Option<usize> {
            match object {
                "s1" => Some(0),
                "s2" => Some(1),
                _ => None,
            }
        }
        fn sheet_name(&self, sheet: usize) -> Option<&str> {
            ["Data", "Other Sheet"].get(sheet).copied()
        }
        fn line_at(&self, sheet: usize, axis: Axis, token: &str) -> LineAt {
            self.lines.get(&(sheet, axis)).map_or(LineAt::Missing, |l| l.at(token))
        }
        fn style_id(&self, _key: &str) -> StyleId {
            StyleId::DEFAULT
        }
    }

    fn round_trip(text: &str) -> String {
        let mut book = Book::new(&[]);
        let stored = encode_formula(&mut book, text, None, Some(0)).unwrap();
        decode_formula(&book, &stored, Some(0))
    }

    #[test]
    fn formulas_round_trip() {
        for text in [
            "SUM(A1:B3)+$C$2*'Other Sheet'!D4",
            "A1#",
            "SUM(2:3)+COUNT($B:$C)",
            "Missing!A1+1",
            "SUM(Data:'Other Sheet'!A1)",
            "\"⟦not a token⟧\"&A1",
            "IF(A1>0,\"{@x}\",B2)",
            "1+2",
            "Table1[[#Headers],[Col1]]",
            "SUM(A1:A1048576)",
        ] {
            assert_eq!(round_trip(text), text, "{text}");
        }
        // Not canonical, or not a formula: stored as written.
        let mut book = Book::new(&[]);
        assert_eq!(encode_formula(&mut book, "sum( a1 )", None, Some(0)).unwrap(), "sum( a1 )");
        assert_eq!(encode_formula(&mut book, "SUM(", None, Some(0)).unwrap(), "SUM(");
    }

    #[test]
    fn formulas_name_lines_by_object() {
        let mut book = Book::new(&[]);
        let stored = encode_formula(&mut book, "A2+'Other Sheet'!$B$3", None, Some(0)).unwrap();
        assert_eq!(stored, "\u{27E6}@|c|r0x1,c0x0\u{27E7}+\u{27E6}@s2|c|$r1x2,$c1x1\u{27E7}");
        // Rows deleted above move the references; a reference to a deleted row is #REF!.
        let book = Book::new(&["r0x0", "r1x2"]);
        assert_eq!(decode_formula(&book, &stored, Some(0)), "A1+#REF!");
    }

    #[test]
    fn ranges_shrink_when_their_ends_are_deleted() {
        let mut book = Book::new(&[]);
        let stored = encode_formula(&mut book, "SUM(A2:A5)", None, Some(0)).unwrap();
        let book = Book::new(&["r0x1", "r0x4"]);
        assert_eq!(decode_formula(&book, &stored, Some(0)), "SUM(A2:A3)");
        let book = Book::new(&["r0x1", "r0x2", "r0x3", "r0x4"]);
        assert_eq!(decode_formula(&book, &stored, Some(0)), "SUM(#REF!)");
    }

    #[test]
    fn literals_round_trip() {
        for v in [
            Value::Empty,
            Value::Number(3.5),
            Value::Number(-0.0),
            Value::Number(1e300),
            Value::Bool(true),
            Value::Error(CellError::Div0),
            Value::Text(Arc::from("plain")),
            Value::Text(Arc::from("")),
            Value::Text(Arc::from("12")),
            Value::Text(Arc::from("true")),
            Value::Text(Arc::from("=not a formula")),
            Value::Text(Arc::from("'quoted")),
            Value::Text(Arc::from("#N/A")),
            Value::Text(Arc::from("inf")),
        ] {
            let back = parse_literal(&literal(&v));
            match (&v, &back) {
                (Value::Number(a), Value::Number(b)) => assert_eq!(a.to_bits(), b.to_bits()),
                _ => assert_eq!(back, v),
            }
        }
    }

    #[test]
    fn feature_positions_round_trip() {
        let mut book = Book::new(&[]);
        let r = RangeRef::parse("B2:D4").unwrap();
        let token = encode_range(&mut book, 0, r, Use::Content).unwrap();
        assert_eq!(decode_range(&book, 0, &token), Some(r));
        let whole = RangeRef::cols(1, 2);
        let token = encode_range(&mut book, 0, whole, Use::Content).unwrap();
        assert!(token.starts_with("*,"));
        assert_eq!(decode_range(&book, 0, &token), Some(whole));
        let cell = encode_cell(&mut book, 1, CellRef::new(3, 4), Use::Ref).unwrap();
        let book = Book::new(&["r1x3"]);
        assert_eq!(decode_cell(&book, 1, &cell), Some(CellRef::new(3, 4)), "a deleted anchor row moves to the next row");
    }
}
