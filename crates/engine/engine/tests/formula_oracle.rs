//! Formula results against a reference spreadsheet (F1).
//!
//! `data/formula_oracle.tsv` holds a fixture (cells on Sheet1 and Sheet2, defined names) and
//! one case per line: a formula and the value the reference application computed for it. The
//! expected values were observed black-box: the formulas were written to a workbook without
//! cached values, opened and recalculated in WPS Office for Mac (zh-CN), saved, and the cached
//! values read back. Functions that WPS lacks carry the result documented by Microsoft
//! (`# doc` after the value). Each case is entered at `Sheet1!A<n>` (n = case number), as a
//! person would type it, and compared with the reference at the 15 significant digits a
//! spreadsheet keeps.
//!
//! Line formats:
//! - `@Sheet!A1<TAB>n|s|b|f<TAB>value`: a fixture cell (number, text, boolean, formula without `=`).
//! - `@Name<TAB>name<TAB>refers-to`: a workbook-scoped defined name.
//! - `=formula<TAB>expected[<TAB>~tolerance][ # note]`: a case; expected is `n:<number>`,
//!   `s:<text>` (`\t`, `\n`, `\\` escaped), `b:TRUE|FALSE` or `e:<error>`. A `~1e-12` relative
//!   tolerance is given only where the reference itself is approximate (iterative solvers, the
//!   rounded figures of Microsoft's documentation).

use kition_sheet_core::{CellRef, Value};
use kition_sheet_engine::Session;
use serde_json::json;

const DATA: &str = include_str!("data/formula_oracle.tsv");

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A number as a spreadsheet keeps it: rounded to 15 significant digits.
fn sig15(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    format!("{x:.14e}").parse().unwrap_or(x)
}

fn matches(actual: &Value, expected: &str, tolerance: f64) -> bool {
    let (kind, want) = expected.split_once(':').unwrap_or((expected, ""));
    match (kind, actual) {
        ("n", Value::Number(a)) => {
            let w: f64 = want.parse().unwrap();
            let a = sig15(*a);
            a == w || (a - w).abs() <= tolerance.max(1e-14) * w.abs().max(a.abs())
        }
        ("s", Value::Text(t)) => **t == *unescape(want),
        ("b", Value::Bool(b)) => (if *b { "TRUE" } else { "FALSE" }) == want,
        ("e", Value::Error(e)) => e.as_str() == want,
        _ => false,
    }
}

fn show(v: &Value) -> String {
    match v {
        Value::Number(n) => format!("n:{}", sig15(*n)),
        Value::Text(t) => format!("s:{t}"),
        Value::Bool(b) => format!("b:{}", if *b { "TRUE" } else { "FALSE" }),
        Value::Error(e) => format!("e:{}", e.as_str()),
        Value::Empty => "empty".into(),
        Value::Array(_) => "array".into(),
    }
}

#[test]
fn formulas_match_the_reference_application() {
    let mut s = Session::new();
    s.new_workbook();
    if s.doc().unwrap().wb.sheet_index("Sheet2").is_none() {
        s.execute("sheet.insert", json!({"name": "Sheet2"})).unwrap();
    }
    s.execute("sheet.activate", json!({"sheet": "Sheet1"})).unwrap();
    let mut cases = Vec::new();
    // An audit run can point at another file of the same format.
    let other = std::env::var("FORMULA_ORACLE_DATA").ok().and_then(|p| std::fs::read_to_string(p).ok());
    for line in other.as_deref().unwrap_or(DATA).lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            let mut parts = rest.splitn(3, '\t');
            let (target, kind, value) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap_or(""));
            if kind == "name" {
                s.execute("name.define", json!({"name": target, "refersTo": value})).unwrap();
                continue;
            }
            let (sheet, cell) = target.split_once('!').unwrap();
            let v = match kind {
                "n" => json!(value.parse::<f64>().unwrap()),
                "b" => json!(value.eq_ignore_ascii_case("TRUE")),
                "s" => json!(format!("'{value}")),
                "f" => json!(format!("={value}")),
                other => panic!("fixture kind {other}"),
            };
            s.execute("range.setValues", json!({"sheet": sheet, "range": cell, "values": [[v]]})).unwrap();
        } else if line.starts_with('=') {
            let mut parts = line.split('\t');
            let (formula, expected) = (parts.next().unwrap().to_string(), parts.next().unwrap_or("").to_string());
            let tolerance =
                parts.next().and_then(|n| n.strip_prefix('~')).and_then(|n| n.split_whitespace().next()).map_or(0.0, |t| t.parse().unwrap());
            cases.push((formula, expected, tolerance));
        }
    }
    for (i, (formula, _, _)) in cases.iter().enumerate() {
        s.execute("cell.set", json!({"sheet": "Sheet1", "cell": format!("A{}", i + 1), "input": formula})).unwrap();
    }
    let wb = &s.doc().unwrap().wb;
    let sheet = wb.sheet(wb.sheet_index("Sheet1").unwrap()).unwrap();
    let mut wrong = Vec::new();
    for (i, (formula, expected, tolerance)) in cases.iter().enumerate() {
        let got = sheet.value(CellRef::new(i as u32, 0));
        if !matches(&got, expected, *tolerance) {
            wrong.push(format!("A{}  {formula}  expected {expected}, got {}", i + 1, show(&got)));
        }
    }
    assert!(wrong.is_empty(), "{} of {} formulas differ from the reference:\n{}", wrong.len(), cases.len(), wrong.join("\n"));
}
