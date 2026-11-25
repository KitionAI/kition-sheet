//! What [`KitDocument::check`](kition_kit::KitDocument::check) reports on a workbook (Kit
//! Standard 2.0 A21): formulas that end in an error or go round in a circle, defined names that
//! point nowhere, merged ranges that overlap or hide values, and formulas that read a single cell
//! beyond the data of its sheet. Targets name the sheet and the cell or range in A1 notation.
//!
//! Codes: `sheet.formula-error`, `sheet.circular-reference`, `sheet.broken-name`,
//! `sheet.overlapping-merges`, `sheet.merge-hides-data`,
//! `sheet.reference-outside-data`, `sheet.font-substituted`, `sheet.missing-glyph`, and
//! `sheet.truncated` when a code is found more often than
//! [`PER_CODE`] times.

use std::collections::BTreeMap;

use kition_kit::diagnostics::{Diagnostic, Severity, Target};
use kition_sheet_engine::core::{CellError, Value};
use kition_sheet_engine::formula::{Expr, RefKind, SheetSel};
use kition_sheet_engine::model::{Sheet, Workbook};
use serde_json::json;

use crate::draw::Faces;

/// At most this many diagnostics of one code; the rest are counted in one `sheet.truncated`.
pub const PER_CODE: usize = 100;

/// Everything worth fixing in the workbook, errors first; `faces`: what the renderer draws text
/// with.
pub fn check(wb: &Workbook, faces: &Faces) -> Vec<Diagnostic> {
    check_with(wb, faces, crate::draw::fonts_added())
}

/// [`check`], `added`: whether the host has added fonts (see [`fonts`]).
fn check_with(wb: &Workbook, faces: &Faces, added: bool) -> Vec<Diagnostic> {
    let mut found = Found::default();
    for sheet in &wb.sheets {
        formulas(wb, sheet, &mut found);
        merges(sheet, &mut found);
    }
    names(wb, &mut found);
    fonts(wb, faces, added, &mut found);
    found.finish()
}

/// Fonts the renderer draws in another face (`sheet.font-substituted`: text takes other widths
/// than in Excel), and characters no face it has draws (`sheet.missing-glyph`: empty boxes).
/// An Office font drawn in the built-in face while the host has added no fonts at all is the
/// kit's own fallback (decision F1), not reported; once the host adds fonts, a missing stand-in
/// is.
fn fonts(wb: &Workbook, faces: &Faces, added: bool, found: &mut Found) {
    // Per font name: how many cells use it and the first one.
    let mut used: BTreeMap<String, (usize, String, String)> = BTreeMap::new();
    let mut picked: BTreeMap<String, ab_glyph::FontArc> = BTreeMap::new();
    for sheet in &wb.sheets {
        for (cell, content) in sheet.cells.iter() {
            if content.value.is_empty() {
                continue;
            }
            let font = &wb.styles.get(content.style).font;
            let runs = content.runs();
            let mut names: Vec<&str> = vec![font.name.as_str()];
            if let Some(runs) = runs {
                names.extend(runs.runs.iter().filter_map(|run| run.font.name.as_deref()));
            }
            for name in &names {
                let entry = used.entry((*name).to_string()).or_insert_with(|| (0, sheet.name.clone(), cell.a1()));
                entry.0 += 1;
            }
            // Characters of text no face draws.
            let Value::Text(text) = &content.value else { continue };
            let mut missing: Vec<char> = Vec::new();
            let mut look = |name: &str, part: &str| {
                let face = picked.entry(name.to_string()).or_insert_with(|| faces.pick(name, false, false).face);
                for c in part.chars().filter(|c| !c.is_whitespace()) {
                    if !crate::draw::draws(face, c) && !missing.contains(&c) && missing.len() < 20 {
                        missing.push(c);
                    }
                }
            };
            match runs {
                Some(runs) => {
                    for run in &runs.runs {
                        look(run.font.name.as_deref().unwrap_or(&font.name), &run.text);
                    }
                }
                None => look(&font.name, text),
            }
            if !missing.is_empty() {
                let chars: String = missing.iter().map(|c| format!("{c} (U+{:04X})", u32::from(*c))).collect::<Vec<_>>().join(", ");
                found.push(
                    Diagnostic::new(
                        "sheet.missing-glyph",
                        Severity::Warning,
                        format!(
                            "{}!{} has characters no font the renderer has draws; they show as empty boxes: {chars}. Add a font that has them.",
                            sheet.name,
                            cell.a1()
                        ),
                    )
                    .on(at(sheet, cell.a1())),
                );
            }
        }
    }
    for (name, (count, sheet, cell)) in used {
        let Some(substitute) = faces.substitute(&name) else { continue };
        if substitute.stand_in.is_some() && !added {
            continue;
        }
        let face = if substitute.mono { "monospace" } else { "sans" };
        let add = match substitute.stand_in {
            Some(open) => format!("add {}, its metric-compatible stand-in", capitalised(open)),
            None => format!("add a {name} font file"),
        };
        let cells = if count == 1 { "1 cell".to_string() } else { format!("{count} cells") };
        found.push(
            Diagnostic::new(
                "sheet.font-substituted",
                Severity::Info,
                format!("{name} ({cells}, first {sheet}!{cell}) is drawn in the built-in {face} face, so its text takes other widths than in Excel: {add}."),
            )
            .on(Target { sheet: Some(sheet), cell: Some(cell), ..Target::default() }),
        );
    }
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

#[derive(Default)]
struct Found {
    list: Vec<Diagnostic>,
    counts: BTreeMap<String, usize>,
}

impl Found {
    fn push(&mut self, diagnostic: Diagnostic) {
        let count = self.counts.entry(diagnostic.code.clone()).or_default();
        *count += 1;
        if *count <= PER_CODE {
            self.list.push(diagnostic);
        }
    }

    fn finish(mut self) -> Vec<Diagnostic> {
        for (code, count) in &self.counts {
            if *count > PER_CODE {
                self.list.push(Diagnostic::new(
                    "sheet.truncated",
                    Severity::Info,
                    format!("{} more `{code}` diagnostics are not listed.", count - PER_CODE),
                ));
            }
        }
        self.list.sort_by_key(|diagnostic| diagnostic.severity);
        self.list
    }
}

fn at(sheet: &Sheet, cell: String) -> Target {
    Target { sheet: Some(sheet.name.clone()), cell: Some(cell), ..Target::default() }
}

/// Formula results that are errors, and single-cell references past the data of their sheet.
fn formulas(wb: &Workbook, sheet: &Sheet, found: &mut Found) {
    for (cell, content) in sheet.cells.iter() {
        let Some(formula) = &content.formula else { continue };
        // A circular reference shows 0, so calc remembers its cells.
        if sheet.circular.contains(&cell) && content.value != Value::Error(CellError::Circ) {
            let message = format!("{}!{} is part of a circular reference: ={}", sheet.name, cell.a1(), formula.text);
            found.push(Diagnostic::new("sheet.circular-reference", Severity::Error, message).on(at(sheet, cell.a1())));
        }
        if let Value::Error(error) = &content.value {
            let (code, severity) = match error {
                CellError::Circ => ("sheet.circular-reference", Severity::Error),
                CellError::NA => ("sheet.formula-error", Severity::Warning),
                _ => ("sheet.formula-error", Severity::Error),
            };
            let message = if *error == CellError::Circ {
                format!("{}!{} is part of a circular reference: ={}", sheet.name, cell.a1(), formula.text)
            } else {
                format!("{}!{} evaluates to {}: ={}", sheet.name, cell.a1(), error.as_str(), formula.text)
            };
            found.push(Diagnostic::new(code, severity, message).on(at(sheet, cell.a1())));
        }
        let Some(expr) = formula.expr_arc() else { continue };
        for reference in expr.references() {
            let RefKind::Cell(anchor) = &reference.kind else { continue };
            let target = match &reference.sheet {
                SheetSel::Current => Some(sheet),
                SheetSel::Named(name) => wb.sheet_index(name).and_then(|index| wb.sheets.get(index)).map(AsRef::as_ref),
                SheetSel::Span(..) => None,
            };
            let Some(target) = target else { continue };
            let referred = anchor.cell();
            let beyond = target.cells.used_range().is_none_or(|used| !used.contains(referred));
            if beyond && target.cells.get(referred).is_none_or(|cell| cell.value.is_empty() && cell.formula.is_none()) {
                found.push(
                    Diagnostic::new(
                        "sheet.reference-outside-data",
                        Severity::Warning,
                        format!("{}!{} reads {}!{}, an empty cell beyond the data of its sheet.", sheet.name, cell.a1(), target.name, referred.a1()),
                    )
                    .on(at(sheet, cell.a1())),
                );
            }
        }
    }
}

/// Merged ranges that overlap or hide values outside their top-left cell. (A merge of one cell never
/// reaches a workbook: the XLSX reader and writer leave it out.)
fn merges(sheet: &Sheet, found: &mut Found) {
    for (index, merge) in sheet.merges.iter().enumerate() {
        if let Some(other) = sheet.merges[index + 1..].iter().find(|other| other.intersects(merge)) {
            found.push(
                Diagnostic::new(
                    "sheet.overlapping-merges",
                    Severity::Error,
                    format!("{}!{} and {} are merged ranges that overlap.", sheet.name, merge.a1(), other.a1()),
                )
                .on(at(sheet, merge.a1()))
                .fix("cell.unmerge", Some(json!({ "range": other.a1() }))),
            );
        }
        let hidden = sheet.cells.iter_range(*merge).filter(|(cell, content)| *cell != merge.start && !content.value.is_empty()).count();
        if hidden > 0 {
            found.push(
                Diagnostic::new(
                    "sheet.merge-hides-data",
                    Severity::Warning,
                    format!("{}!{} is merged over {hidden} more cells with values; only its top-left cell shows.", sheet.name, merge.a1()),
                )
                .on(at(sheet, merge.a1()))
                .fix("cell.unmerge", Some(json!({ "range": merge.a1() }))),
            );
        }
    }
}

/// Defined names whose formula is `#REF!`, does not parse, names a sheet the workbook lacks, or
/// whose scope is a sheet that does not exist.
fn names(wb: &Workbook, found: &mut Found) {
    for name in &wb.names {
        let parsed = kition_sheet_engine::formula::parse(&name.formula);
        let missing_sheet = parsed.as_ref().ok().and_then(|expr| {
            expr.references()
                .into_iter()
                .find_map(|reference| reference.sheet_name().filter(|sheet| wb.sheet_index(sheet).is_none()).map(str::to_owned))
        });
        let refers_to_error = parsed.as_ref().is_ok_and(|expr| {
            let mut error = false;
            expr.walk(&mut |node| error |= matches!(node, Expr::Error(CellError::Ref)));
            error
        });
        let why = if name.scope.is_some_and(|scope| scope >= wb.sheets.len()) {
            Some("belongs to a sheet that no longer exists".to_owned())
        } else if parsed.is_err() {
            Some(format!("has a formula that does not parse: ={}", name.formula))
        } else if refers_to_error || name.formula.contains("#REF!") {
            Some(format!("refers to #REF!: ={}", name.formula))
        } else {
            missing_sheet.map(|sheet| format!("refers to a sheet named \"{sheet}\" that the workbook does not have"))
        };
        if let Some(why) = why {
            let scope = name.scope.and_then(|scope| wb.sheets.get(scope)).map(|sheet| sheet.name.clone());
            let mut diagnostic = Diagnostic::new("sheet.broken-name", Severity::Error, format!("The defined name {} {why}.", name.name)).fix(
                "name.delete",
                Some(match &scope {
                    Some(sheet) => json!({ "name": name.name, "scope": sheet }),
                    None => json!({ "name": name.name, "scope": "Workbook" }),
                }),
            );
            diagnostic.target = Target { path: Some(format!("names.{}", name.name)), sheet: scope, ..Target::default() };
            found.push(diagnostic);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kition_sheet_engine::core::{CellRef, RangeRef};
    use kition_sheet_engine::model::{Cell, DefinedName, Formula};

    use super::*;

    fn cell(a1: &str) -> CellRef {
        CellRef::parse(a1).unwrap_or_default()
    }

    fn book(edit: impl FnOnce(&mut Sheet, &mut Workbook)) -> Workbook {
        let mut wb = Workbook::default();
        let mut sheet = Sheet::new("Data");
        edit(&mut sheet, &mut wb);
        wb.sheets = vec![Arc::new(sheet)];
        wb
    }

    /// The check as a host that added no fonts sees it (other tests add fonts to the process).
    fn check(wb: &Workbook) -> Vec<Diagnostic> {
        super::check_with(wb, &Faces::load().unwrap(), false)
    }

    fn codes(found: &[Diagnostic]) -> Vec<&str> {
        found.iter().map(|diagnostic| diagnostic.code.as_str()).collect()
    }

    #[test]
    fn a_clean_workbook_has_nothing_to_report() {
        let wb = book(|sheet, _| {
            sheet.cells.set(cell("A1"), Cell::value(Value::Number(2.0)));
            let mut sum = Cell::formula(Formula::new("=A1*2"));
            sum.value = Value::Number(4.0);
            sheet.cells.set(cell("A2"), sum);
        });
        assert_eq!(check(&wb), []);
    }

    #[test]
    fn reports_formula_errors_and_circles_errors_first() {
        let wb = book(|sheet, _| {
            let mut div = Cell::formula(Formula::new("=1/0"));
            div.value = Value::Error(CellError::Div0);
            sheet.cells.set(cell("B2"), div);
            let mut circle = Cell::formula(Formula::new("=C3+1"));
            circle.value = Value::Error(CellError::Circ);
            sheet.cells.set(cell("C3"), circle);
            let mut lookup = Cell::formula(Formula::new("=NA()"));
            lookup.value = Value::Error(CellError::NA);
            sheet.cells.set(cell("D4"), lookup);
        });
        let found = check(&wb);
        assert_eq!(found.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).count(), 2, "{found:?}");
        assert_eq!(found.last().map(|diagnostic| diagnostic.severity), Some(Severity::Warning));
        let b2 = found.iter().find(|diagnostic| diagnostic.target.cell.as_deref() == Some("B2")).map(|diagnostic| diagnostic.code.as_str());
        assert_eq!(b2, Some("sheet.formula-error"));
        assert!(codes(&found).contains(&"sheet.circular-reference"));
        assert_eq!(found[0].target.sheet.as_deref(), Some("Data"));
        assert!(kition_kit::diagnostics::check_diagnostics(&found).is_ok());
    }

    #[test]
    fn reports_references_past_the_data_but_not_ranges() {
        let wb = book(|sheet, _| {
            sheet.cells.set(cell("A1"), Cell::value(Value::Number(1.0)));
            let mut far = Cell::formula(Formula::new("=A1+Z900"));
            far.value = Value::Number(1.0);
            sheet.cells.set(cell("B1"), far);
            let mut sum = Cell::formula(Formula::new("=SUM(A1:A1000)"));
            sum.value = Value::Number(1.0);
            sheet.cells.set(cell("B2"), sum);
        });
        let found = check(&wb);
        assert_eq!(codes(&found), ["sheet.reference-outside-data"], "{found:?}");
        assert_eq!(found[0].target.cell.as_deref(), Some("B1"));
    }

    #[test]
    fn reports_overlapping_and_hiding_merges() {
        let wb = book(|sheet, _| {
            let range = |a: &str, b: &str| RangeRef::new(cell(a), cell(b));
            sheet.merges = vec![range("A1", "B2"), range("B2", "C3")];
            sheet.cells.set(cell("B1"), Cell::value(Value::Number(3.0)));
        });
        let found = check(&wb);
        let mut seen = codes(&found);
        seen.sort_unstable();
        assert_eq!(seen, ["sheet.merge-hides-data", "sheet.overlapping-merges"]);
        assert_eq!(found[0].fix.as_ref().map(|fix| fix.command.as_str()), Some("cell.unmerge"));
    }

    #[test]
    fn reports_names_that_point_nowhere() {
        let wb = book(|_, wb| {
            let name = |name: &str, formula: &str, scope: Option<usize>| DefinedName {
                name: name.into(),
                scope,
                formula: formula.into(),
                comment: String::new(),
                hidden: false,
            };
            wb.names = vec![
                name("Good", "Data!$A$1", None),
                name("Gone", "#REF!", None),
                name("Elsewhere", "Missing!$A$1", None),
                name("Orphan", "1", Some(4)),
            ];
        });
        let found = check(&wb);
        let broken: Vec<_> = found
            .iter()
            .filter(|diagnostic| diagnostic.code == "sheet.broken-name")
            .map(|diagnostic| diagnostic.target.path.clone().unwrap_or_default())
            .collect();
        assert_eq!(broken, ["names.Gone", "names.Elsewhere", "names.Orphan"]);
    }

    #[test]
    fn caps_each_code_and_says_how_many_more() {
        let wb = book(|sheet, _| {
            for row in 0..(PER_CODE as u32 + 5) {
                let mut error = Cell::formula(Formula::new("=1/0"));
                error.value = Value::Error(CellError::Div0);
                sheet.cells.set(CellRef::new(row, 0), error);
            }
        });
        let found = check(&wb);
        assert_eq!(found.len(), PER_CODE + 1);
        assert_eq!(found.last().map(|diagnostic| diagnostic.message.as_str()), Some("5 more `sheet.formula-error` diagnostics are not listed."));
    }
}
