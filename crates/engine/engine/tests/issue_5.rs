//! GROUPBY and PIVOTBY take their function as Excel writes it, a bare
//! function name (`SUM`, an eta-reduced LAMBDA) or a LAMBDA, besides a name in quotes; a bare
//! name answered #NAME?.

use kition_sheet_core::{CellRef, Value};
use kition_sheet_engine::Session;
use serde_json::json;

fn orders() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute(
        "range.setValues",
        json!({"range": "A1", "values": [
            ["Region", "Product", "Amount"],
            ["East", "Lamp", 10],
            ["West", "Desk", 40],
            ["East", "Desk", 5],
            ["West", "Lamp", 7],
            ["East", "Lamp", 3]
        ]}),
    )
    .unwrap();
    s
}

fn v(s: &Session, a: &str) -> Value {
    s.doc().unwrap().wb.active().unwrap().value(CellRef::parse(a).unwrap())
}

fn n(x: f64) -> Value {
    Value::Number(x)
}

fn t(x: &str) -> Value {
    Value::from(x.to_string())
}

#[test]
fn groupby_takes_a_bare_function_name() {
    let mut s = orders();
    s.execute("cell.set", json!({"cell": "E1", "input": "=GROUPBY(A2:A6,C2:C6,SUM)"})).unwrap();
    assert_eq!([v(&s, "E1"), v(&s, "F1")], [t("East"), n(18.0)]);
    assert_eq!([v(&s, "E2"), v(&s, "F2")], [t("West"), n(47.0)]);
    assert_eq!([v(&s, "E3"), v(&s, "F3")], [t("Total"), n(65.0)]);
}

#[test]
fn groupby_takes_other_bare_names_and_lambdas() {
    let mut s = orders();
    s.execute("cell.set", json!({"cell": "E1", "input": "=GROUPBY(A2:A6,C2:C6,AVERAGE,0,0)"})).unwrap();
    assert_eq!([v(&s, "E1"), v(&s, "F1"), v(&s, "F2")], [t("East"), n(6.0), n(23.5)]);
    s.execute("cell.set", json!({"cell": "H1", "input": "=GROUPBY(A2:A6,C2:C6,LAMBDA(x,MAX(x)-MIN(x)),0,0)"})).unwrap();
    assert_eq!([v(&s, "H1"), v(&s, "I1"), v(&s, "I2")], [t("East"), n(7.0), n(33.0)]);
    s.execute("cell.set", json!({"cell": "K1", "input": "=GROUPBY(A2:A6,C2:C6,COUNTA,0,0)"})).unwrap();
    assert_eq!([v(&s, "L1"), v(&s, "L2")], [n(3.0), n(2.0)]);
}

#[test]
fn pivotby_takes_a_bare_function_name() {
    let mut s = orders();
    s.execute("cell.set", json!({"cell": "E1", "input": "=PIVOTBY(A2:A6,B2:B6,C2:C6,SUM)"})).unwrap();
    // Header row: blank, the column keys, Total.
    assert_eq!([v(&s, "F1"), v(&s, "G1"), v(&s, "H1")], [t("Desk"), t("Lamp"), t("Total")]);
    assert_eq!([v(&s, "E2"), v(&s, "F2"), v(&s, "G2"), v(&s, "H2")], [t("East"), n(5.0), n(13.0), n(18.0)]);
    assert_eq!([v(&s, "E3"), v(&s, "F3"), v(&s, "G3"), v(&s, "H3")], [t("West"), n(40.0), n(7.0), n(47.0)]);
}

#[test]
fn a_bare_name_that_is_no_function_stays_name_error() {
    let mut s = orders();
    s.execute("cell.set", json!({"cell": "E1", "input": "=GROUPBY(A2:A6,C2:C6,NOSUCHFUNCTION)"})).unwrap();
    assert!(v(&s, "E1").is_error());
}

#[test]
fn a_bare_function_name_survives_xlsx() {
    let mut s = orders();
    s.execute("cell.set", json!({"cell": "E1", "input": "=GROUPBY(A2:A6,C2:C6,SUM)"})).unwrap();
    let bytes = kition_sheet_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let mut sheet = String::new();
    {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut sheet).unwrap();
    }
    assert!(sheet.contains("_xlfn.GROUPBY(A2:A6,C2:C6,_xleta.SUM)"), "{sheet}");
    let (wb, _) = kition_sheet_xlsx::read_xlsx(&bytes).unwrap();
    let cell = wb.sheets[0].cell(CellRef::parse("E1").unwrap()).unwrap();
    assert_eq!(cell.formula.as_ref().unwrap().text, "GROUPBY(A2:A6,C2:C6,SUM)");
}

#[test]
fn other_lambda_functions_take_a_bare_function_name() {
    let mut s = orders();
    s.execute("cell.set", json!({"cell": "E1", "input": "=BYROW(C2:C4,SUM)"})).unwrap();
    s.execute("cell.set", json!({"cell": "F1", "input": "=REDUCE(0,C2:C6,SUM)"})).unwrap();
    assert_eq!([v(&s, "E1"), v(&s, "E2"), v(&s, "F1")], [n(10.0), n(40.0), n(65.0)]);
}
