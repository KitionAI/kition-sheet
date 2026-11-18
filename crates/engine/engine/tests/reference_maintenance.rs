//! References kept right through structural edits, and recalculation (F1): inserting and deleting
//! rows and columns, deleting and renaming sheets, copying, filling, sorting; volatile and
//! cross-sheet dependencies; circular references, iterative calculation and manual mode. The
//! expected behaviour is WPS's (and Excel's, where they agree).

use std::sync::Arc;

use kition_sheet_core::{CellError, CellRef, Value};
use kition_sheet_engine::Session;
use serde_json::{Value as Json, json};

fn workbook(sheets: &[&str]) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for name in sheets.iter().skip(1) {
        if s.doc().unwrap().wb.sheet_index(name).is_none() {
            run(&mut s, "home.insertSheet", json!({"name": name}));
        }
    }
    run(&mut s, "sheet.activate", json!({"sheet": sheets[0]}));
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn set(s: &mut Session, sheet: &str, cell: &str, input: &str) {
    run(s, "cell.set", json!({"sheet": sheet, "cell": cell, "input": input}));
}

fn value(s: &Session, sheet: &str, cell: &str) -> Value {
    let wb = &s.doc().unwrap().wb;
    wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap().value(CellRef::parse(cell).unwrap())
}

fn formula(s: &Session, sheet: &str, cell: &str) -> String {
    let wb = &s.doc().unwrap().wb;
    wb.sheet(wb.sheet_index(sheet).unwrap())
        .unwrap()
        .cell(CellRef::parse(cell).unwrap())
        .and_then(|c| c.formula.as_ref())
        .map(|f| f.text.clone())
        .unwrap_or_default()
}

fn num(n: f64) -> Value {
    Value::Number(n)
}

fn sheet_mut<'a>(s: &'a mut Session, sheet: &str) -> &'a mut kition_sheet_model::Sheet {
    let d = s.doc_mut().unwrap();
    let i = d.wb.sheet_index(sheet).unwrap();
    Arc::make_mut(&mut d.wb).sheet_mut(i).unwrap()
}

fn sheet<'a>(s: &'a Session, sheet: &str) -> &'a kition_sheet_model::Sheet {
    let wb = &s.doc().unwrap().wb;
    wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap()
}

fn name_formula(s: &Session, name: &str) -> String {
    s.doc().unwrap().wb.names.iter().find(|n| n.name == name).map(|n| n.formula.clone()).unwrap_or_default()
}

/// Sheet1 with a chart, a conditional format, a validation and a name, all reading Sheet2.
fn features_on_sheet2() -> Session {
    let mut s = workbook(&["Sheet1", "Sheet2"]);
    for (i, v) in [1.0, 2.0, 3.0].iter().enumerate() {
        set(&mut s, "Sheet2", &format!("A{}", i + 1), &v.to_string());
        set(&mut s, "Sheet1", &format!("A{}", i + 1), &v.to_string());
    }
    run(&mut s, "insert.chart", json!({"range": "Sheet1!A1:A3", "type": "column"}));
    sheet_mut(&mut s, "Sheet1").charts[0].series[0].values = "Sheet2!$A$1:$A$3".into();
    run(
        &mut s,
        "home.conditionalFormat",
        json!({"range": "Sheet1!C1:C3", "rule": {"type": "expression", "formula": "=Sheet2!$A1>1", "style": {"fill": "#FF0000"}}}),
    );
    run(&mut s, "data.validation", json!({"range": "Sheet1!D1", "type": "list", "formula1": "=Sheet2!$A$1:$A$3"}));
    run(&mut s, "formulas.defineName", json!({"name": "Rate", "refersTo": "=Sheet2!$A$2"}));
    set(&mut s, "Sheet1", "E1", "=Rate*10");
    s
}

fn cf_formula(s: &Session, sh: &str) -> String {
    match &sheet(s, sh).cond_formats[0].rule {
        kition_sheet_model::CfRule::Expression { formula, .. } => formula.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn deleting_a_sheet_turns_chart_cf_validation_and_name_references_into_ref() {
    let mut s = features_on_sheet2();
    assert_eq!(value(&s, "Sheet1", "E1"), num(20.0));
    run(&mut s, "home.deleteSheet", json!({"sheet": "Sheet2"}));
    let sh = sheet(&s, "Sheet1");
    assert!(sh.charts[0].series[0].values.contains("#REF!"), "chart series: {}", sh.charts[0].series[0].values);
    assert!(cf_formula(&s, "Sheet1").contains("#REF!"), "conditional format: {}", cf_formula(&s, "Sheet1"));
    assert!(sheet(&s, "Sheet1").validations[0].f1.contains("#REF!"), "validation: {}", sheet(&s, "Sheet1").validations[0].f1);
    assert!(name_formula(&s, "Rate").contains("#REF!"), "name: {}", name_formula(&s, "Rate"));
    assert_eq!(value(&s, "Sheet1", "E1"), Value::Error(CellError::Ref));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(sheet(&s, "Sheet1").charts[0].series[0].values, "Sheet2!$A$1:$A$3");
    assert_eq!(value(&s, "Sheet1", "E1"), num(20.0));
}

#[test]
fn renaming_a_sheet_renames_it_in_every_feature_formula() {
    let mut s = features_on_sheet2();
    run(&mut s, "sheet.rename", json!({"sheet": "Sheet2", "name": "数据 表"}));
    assert_eq!(sheet(&s, "Sheet1").charts[0].series[0].values, "'数据 表'!$A$1:$A$3");
    assert_eq!(cf_formula(&s, "Sheet1"), "'数据 表'!$A1>1");
    assert_eq!(sheet(&s, "Sheet1").validations[0].f1, "'数据 表'!$A$1:$A$3");
    assert_eq!(name_formula(&s, "Rate"), "'数据 表'!$A$2");
    assert_eq!(value(&s, "Sheet1", "E1"), num(20.0));
}

#[test]
fn inserting_rows_moves_references_from_every_sheet_names_cf_and_validation() {
    let mut s = workbook(&["Sheet1", "Sheet2"]);
    for r in 1..=5 {
        set(&mut s, "Sheet1", &format!("A{r}"), &r.to_string());
    }
    set(&mut s, "Sheet2", "A1", "=SUM(Sheet1!A1:A5)");
    set(&mut s, "Sheet2", "A2", "=Sheet1!$A$3");
    set(&mut s, "Sheet2", "A3", "=Sheet1!A$3+Sheet1!$A3");
    set(&mut s, "Sheet2", "A4", "=SUM(Sheet1!A:A)");
    set(&mut s, "Sheet2", "A5", "=SUM(Sheet1!1:2)");
    set(&mut s, "Sheet1", "B1", "=A1*2");
    run(&mut s, "formulas.defineName", json!({"name": "Mid", "refersTo": "=Sheet1!$A$2:$A$4"}));
    set(&mut s, "Sheet2", "A6", "=SUM(Mid)");
    run(
        &mut s,
        "home.conditionalFormat",
        json!({"range": "Sheet1!C1:C5", "rule": {"type": "expression", "formula": "=$A1>2", "style": {"fill": "#FF0000"}}}),
    );
    run(&mut s, "data.validation", json!({"range": "Sheet1!D1", "type": "list", "formula1": "=$A$1:$A$5"}));
    run(
        &mut s,
        "home.conditionalFormat",
        json!({"range": "Sheet2!B1:B2", "rule": {"type": "expression", "formula": "=Sheet1!$A$5>0", "style": {"fill": "#00FF00"}}}),
    );

    run(&mut s, "home.insertRows", json!({"sheet": "Sheet1", "rows": "2:3"}));
    assert_eq!(formula(&s, "Sheet2", "A1"), "SUM(Sheet1!A1:A7)");
    assert_eq!(formula(&s, "Sheet2", "A2"), "Sheet1!$A$5");
    assert_eq!(formula(&s, "Sheet2", "A3"), "Sheet1!A$5+Sheet1!$A5");
    assert_eq!(formula(&s, "Sheet2", "A4"), "SUM(Sheet1!A:A)");
    assert_eq!(formula(&s, "Sheet2", "A5"), "SUM(Sheet1!1:4)");
    assert_eq!(formula(&s, "Sheet1", "B1"), "A1*2");
    assert_eq!(name_formula(&s, "Mid"), "Sheet1!$A$4:$A$6");
    assert_eq!(value(&s, "Sheet2", "A1"), num(15.0));
    assert_eq!(value(&s, "Sheet2", "A2"), num(3.0));
    assert_eq!(value(&s, "Sheet2", "A6"), num(9.0));
    let sh1 = sheet(&s, "Sheet1");
    assert_eq!(sh1.cond_formats[0].ranges[0].a1(), "C1:C7");
    assert_eq!(cf_formula(&s, "Sheet1"), "$A1>2");
    assert_eq!(sheet(&s, "Sheet1").validations[0].f1, "$A$1:$A$7");
    assert_eq!(cf_formula(&s, "Sheet2"), "Sheet1!$A$7>0");

    // Deleting rows: ranges shrink, a reference to a deleted cell is #REF!.
    run(&mut s, "home.deleteRows", json!({"sheet": "Sheet1", "rows": "2:3"}));
    run(&mut s, "home.deleteRows", json!({"sheet": "Sheet1", "rows": "3"}));
    assert_eq!(formula(&s, "Sheet2", "A1"), "SUM(Sheet1!A1:A4)");
    assert_eq!(value(&s, "Sheet2", "A1"), num(12.0));
    assert_eq!(value(&s, "Sheet2", "A2"), Value::Error(CellError::Ref));
    assert_eq!(name_formula(&s, "Mid"), "Sheet1!$A$2:$A$3");
    assert_eq!(value(&s, "Sheet2", "A6"), num(6.0));
    assert_eq!(sheet(&s, "Sheet1").validations[0].f1, "$A$1:$A$4");
    assert_eq!(cf_formula(&s, "Sheet2"), "Sheet1!$A$4>0");
    // The whole referenced block deleted.
    set(&mut s, "Sheet2", "C1", "=SUM(Sheet1!A2:A3)");
    run(&mut s, "home.deleteRows", json!({"sheet": "Sheet1", "rows": "2:3"}));
    assert_eq!(value(&s, "Sheet2", "C1"), Value::Error(CellError::Ref));
}

#[test]
fn inserting_and_deleting_columns_moves_references() {
    let mut s = workbook(&["Sheet1"]);
    set(&mut s, "Sheet1", "A1", "1");
    set(&mut s, "Sheet1", "B1", "2");
    set(&mut s, "Sheet1", "C1", "3");
    set(&mut s, "Sheet1", "A3", "=SUM(A1:C1)");
    set(&mut s, "Sheet1", "B3", "=$C$1*B$1");
    run(&mut s, "home.insertColumns", json!({"cols": "B"}));
    assert_eq!(formula(&s, "Sheet1", "A3"), "SUM(A1:D1)");
    assert_eq!(formula(&s, "Sheet1", "C3"), "$D$1*C$1");
    assert_eq!(value(&s, "Sheet1", "C3"), num(6.0));
    run(&mut s, "home.deleteColumns", json!({"cols": "D"}));
    assert_eq!(formula(&s, "Sheet1", "A3"), "SUM(A1:C1)");
    assert_eq!(value(&s, "Sheet1", "A3"), num(3.0));
    assert_eq!(value(&s, "Sheet1", "C3"), Value::Error(CellError::Ref));
}

#[test]
fn copying_and_filling_formulas_moves_relative_references_only() {
    let mut s = workbook(&["Sheet1"]);
    set(&mut s, "Sheet1", "A1", "=B1+$C$1+B$1+$C1");
    run(&mut s, "edit.copy", json!({"range": "A1"}));
    run(&mut s, "edit.paste", json!({"at": "D3"}));
    assert_eq!(formula(&s, "Sheet1", "D3"), "E3+$C$1+E$1+$C3");
    run(&mut s, "range.fill", json!({"range": "A1:A3", "input": "=B1*2"}));
    assert_eq!(formula(&s, "Sheet1", "A3"), "B3*2");
    set(&mut s, "Sheet1", "F1", "=G1");
    run(&mut s, "edit.fillDown", json!({"range": "F1:F4"}));
    assert_eq!(formula(&s, "Sheet1", "F4"), "G4");
    // A reference copied above row 1 is #REF!.
    set(&mut s, "Sheet1", "H5", "=H4");
    run(&mut s, "edit.copy", json!({"range": "H5"}));
    run(&mut s, "edit.paste", json!({"at": "H1"}));
    assert_eq!(value(&s, "Sheet1", "H1"), Value::Error(CellError::Ref));
}

#[test]
fn cutting_cells_moves_their_dependents_references() {
    let mut s = workbook(&["Sheet1", "Sheet2"]);
    set(&mut s, "Sheet1", "A1", "5");
    set(&mut s, "Sheet2", "A1", "=Sheet1!A1*2");
    set(&mut s, "Sheet1", "B1", "=A1+1");
    run(&mut s, "edit.cut", json!({"range": "Sheet1!A1"}));
    run(&mut s, "edit.paste", json!({"at": "Sheet1!C5"}));
    assert_eq!(formula(&s, "Sheet2", "A1"), "Sheet1!C5*2");
    assert_eq!(formula(&s, "Sheet1", "B1"), "C5+1");
    assert_eq!(value(&s, "Sheet2", "A1"), num(10.0));
}

#[test]
fn sorting_rows_keeps_each_rows_formula_on_its_row() {
    let mut s = workbook(&["Sheet1"]);
    for (r, v) in [(1, "3"), (2, "1"), (3, "2")] {
        set(&mut s, "Sheet1", &format!("A{r}"), v);
        set(&mut s, "Sheet1", &format!("B{r}"), &format!("=A{r}*10"));
    }
    set(&mut s, "Sheet1", "D1", "=A1");
    run(&mut s, "data.sort", json!({"range": "A1:B3", "keys": [{"column": "A", "order": "asc"}]}));
    assert_eq!(value(&s, "Sheet1", "A1"), num(1.0));
    assert_eq!(value(&s, "Sheet1", "B1"), num(10.0));
    assert_eq!(formula(&s, "Sheet1", "B1"), "A1*10");
    assert_eq!(value(&s, "Sheet1", "B3"), num(30.0));
    // A reference from outside the sorted range keeps its cell.
    assert_eq!(formula(&s, "Sheet1", "D1"), "A1");
    assert_eq!(value(&s, "Sheet1", "D1"), num(1.0));
}

#[test]
fn volatile_and_cross_sheet_dependencies_recalculate() {
    let mut s = workbook(&["Sheet1", "Sheet2", "Sheet3"]);
    set(&mut s, "Sheet1", "A1", "=INDIRECT(\"B1\")");
    set(&mut s, "Sheet1", "A2", "=SUM(OFFSET(B1,0,0,2,1))");
    set(&mut s, "Sheet1", "A3", "=INDIRECT(\"Sheet3!A1\")*3");
    set(&mut s, "Sheet1", "B1", "5");
    set(&mut s, "Sheet1", "B2", "6");
    assert_eq!(value(&s, "Sheet1", "A1"), num(5.0));
    assert_eq!(value(&s, "Sheet1", "A2"), num(11.0));
    set(&mut s, "Sheet3", "A1", "1");
    set(&mut s, "Sheet2", "A1", "=Sheet3!A1+1");
    set(&mut s, "Sheet1", "C1", "=Sheet2!A1*2");
    set(&mut s, "Sheet3", "A1", "10");
    assert_eq!(value(&s, "Sheet1", "C1"), num(22.0));
    assert_eq!(value(&s, "Sheet1", "A3"), num(30.0));
    run(&mut s, "formulas.defineName", json!({"name": "Base", "refersTo": "=Sheet3!$A$1"}));
    set(&mut s, "Sheet1", "C2", "=Base+1");
    set(&mut s, "Sheet3", "A1", "20");
    assert_eq!(value(&s, "Sheet1", "C2"), num(21.0));
    run(&mut s, "formulas.defineName", json!({"name": "Base", "refersTo": "=Sheet1!$B$2"}));
    assert_eq!(value(&s, "Sheet1", "C2"), num(7.0));
    // A formula naming a name defined later picks it up.
    set(&mut s, "Sheet1", "C3", "=Later*2");
    assert_eq!(value(&s, "Sheet1", "C3"), Value::Error(CellError::Name));
    run(&mut s, "formulas.defineName", json!({"name": "Later", "refersTo": "=4"}));
    assert_eq!(value(&s, "Sheet1", "C3"), num(8.0));
}

#[test]
fn a_circular_reference_shows_zero_and_is_reported() {
    let mut s = workbook(&["Sheet1"]);
    set(&mut s, "Sheet1", "A1", "=B1+1");
    set(&mut s, "Sheet1", "B1", "=A1");
    set(&mut s, "Sheet1", "C1", "=B1+5");
    assert_eq!(value(&s, "Sheet1", "A1"), num(0.0));
    assert_eq!(value(&s, "Sheet1", "B1"), num(0.0));
    assert!(
        sheet(&s, "Sheet1").circular.contains(&CellRef::parse("A1").unwrap())
            || sheet(&s, "Sheet1").circular.contains(&CellRef::parse("B1").unwrap())
    );
    let found = run(&mut s, "formulas.errorChecking", json!({}));
    assert!(!found["circular"].as_array().unwrap().is_empty(), "{found}");
    // Breaking the circle recalculates the cells.
    set(&mut s, "Sheet1", "B1", "7");
    assert_eq!(value(&s, "Sheet1", "A1"), num(8.0));
    assert_eq!(value(&s, "Sheet1", "C1"), num(12.0));
    assert!(sheet(&s, "Sheet1").circular.is_empty());
}

#[test]
fn iterative_calculation_resolves_circular_references() {
    let mut s = workbook(&["Sheet1"]);
    run(&mut s, "formulas.calculationOptions", json!({"iterative": true, "maxIterations": 100, "maxChange": 0.000001}));
    set(&mut s, "Sheet1", "B1", "10");
    // x = (x + 10) / 2 converges to 10.
    set(&mut s, "Sheet1", "A1", "=(A1+B1)/2");
    let a1 = value(&s, "Sheet1", "A1").as_f64().unwrap();
    assert!((a1 - 10.0).abs() < 1e-5, "{a1}");
    // A count that never settles stops at the maximum number of iterations.
    run(&mut s, "formulas.calculationOptions", json!({"maxIterations": 50}));
    set(&mut s, "Sheet1", "C1", "=C1+1");
    assert_eq!(value(&s, "Sheet1", "C1"), num(50.0));
    // Two cells in a loop.
    set(&mut s, "Sheet1", "D1", "=E1/2+1");
    set(&mut s, "Sheet1", "E1", "=D1");
    let d1 = value(&s, "Sheet1", "D1").as_f64().unwrap();
    assert!((d1 - 2.0).abs() < 1e-3, "{d1}");
    assert!(sheet(&s, "Sheet1").circular.is_empty());
    // The setting is saved with the workbook.
    let bytes = kition_sheet_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let (back, _) = kition_sheet_xlsx::read_xlsx(&bytes).unwrap();
    assert!(back.calc.iterative);
    assert_eq!(back.calc.max_iterations, 50);
}

#[test]
fn manual_calculation_waits_for_calculate_now() {
    let mut s = workbook(&["Sheet1"]);
    set(&mut s, "Sheet1", "B1", "1");
    set(&mut s, "Sheet1", "A1", "=B1*2");
    run(&mut s, "formulas.calculationOptions", json!({"mode": "manual"}));
    set(&mut s, "Sheet1", "B1", "5");
    assert_eq!(value(&s, "Sheet1", "A1"), num(2.0));
    // A formula entered in manual mode is calculated itself.
    set(&mut s, "Sheet1", "A2", "=B1+1");
    assert_eq!(value(&s, "Sheet1", "A2"), num(6.0));
    run(&mut s, "formulas.calculateNow", json!({}));
    assert_eq!(value(&s, "Sheet1", "A1"), num(10.0));
    run(&mut s, "formulas.calculationOptions", json!({"mode": "automatic"}));
    set(&mut s, "Sheet1", "B1", "7");
    assert_eq!(value(&s, "Sheet1", "A1"), num(14.0));
}

#[test]
fn deep_nesting_huge_ranges_and_whole_columns_never_panic() {
    let mut s = workbook(&["Sheet1"]);
    let deep = format!("={}1{}", "(".repeat(500), ")".repeat(500));
    let _ = s.execute("cell.set", json!({"cell": "A1", "input": deep}));
    let nested = format!("={}1{}", "ABS(".repeat(300), ")".repeat(300));
    let _ = s.execute("cell.set", json!({"cell": "A2", "input": nested}));
    set(&mut s, "Sheet1", "B1", "3");
    set(&mut s, "Sheet1", "B1048576", "4");
    set(&mut s, "Sheet1", "A3", "=SUM(B:B)+COUNTA(B:B)+SUMPRODUCT((B1:B1048576>0)*1)");
    assert_eq!(value(&s, "Sheet1", "A3"), num(11.0));
    set(&mut s, "Sheet1", "A4", "=SUM(A:XFD)");
    set(&mut s, "Sheet1", "A5", "=REPT(\"x\",100000)");
    assert_eq!(value(&s, "Sheet1", "A5"), Value::Error(CellError::Value));
    set(&mut s, "Sheet1", "A6", "=SEQUENCE(100000000)");
    assert!(value(&s, "Sheet1", "A6").is_error());
    // A long chain of dependents.
    set(&mut s, "Sheet1", "C1", "1");
    for r in 2..=3000 {
        set(&mut s, "Sheet1", &format!("C{r}"), &format!("=C{}+1", r - 1));
    }
    set(&mut s, "Sheet1", "C1", "2");
    assert_eq!(value(&s, "Sheet1", "C3000"), num(3001.0));
}

#[test]
fn a_tables_calculated_column_follows_sheet_rename_copy_and_delete() {
    let mut s = workbook(&["Sheet1", "Sheet2"]);
    set(&mut s, "Sheet2", "A1", "4");
    set(&mut s, "Sheet1", "A1", "Qty");
    set(&mut s, "Sheet1", "B1", "Total");
    set(&mut s, "Sheet1", "A2", "1");
    set(&mut s, "Sheet1", "A3", "2");
    run(&mut s, "insert.table", json!({"range": "Sheet1!A1:B3", "header": true, "name": "Orders"}));
    sheet_mut(&mut s, "Sheet1").tables[0].columns[1].formula = Some("Orders[@Qty]*Sheet2!$A$1+SUM(Sheet1!$A$2:$A$3)".into());
    run(&mut s, "sheet.rename", json!({"sheet": "Sheet2", "name": "价格"}));
    assert_eq!(sheet(&s, "Sheet1").tables[0].columns[1].formula.as_deref(), Some("Orders[@Qty]*价格!$A$1+SUM(Sheet1!$A$2:$A$3)"));
    run(&mut s, "home.insertRows", json!({"sheet": "Sheet1", "rows": "1"}));
    assert_eq!(sheet(&s, "Sheet1").tables[0].columns[1].formula.as_deref(), Some("Orders[@Qty]*价格!$A$1+SUM(Sheet1!$A$3:$A$4)"));
    let copied = run(&mut s, "sheet.move", json!({"sheet": "Sheet1", "to": 2, "copy": true}));
    let copy_name = copied["name"].as_str().unwrap().to_string();
    let copy = sheet(&s, &copy_name);
    let table = &copy.tables[0];
    assert_ne!(table.name, "Orders");
    assert_eq!(
        table.columns[1].formula.clone().unwrap(),
        format!("{}[@Qty]*价格!$A$1+SUM({}!$A$3:$A$4)", table.name, kition_sheet_formula::quote_sheet(&copy_name))
    );
    run(&mut s, "home.deleteSheet", json!({"sheet": "价格"}));
    assert!(sheet(&s, "Sheet1").tables[0].columns[1].formula.as_deref().unwrap().contains("#REF!"));
}
