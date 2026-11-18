//! Writes the conformance fixtures (KHP `spec/conformance.md`): `basic.xlsx`, which `sheet_check`
//! finds clean, one `check-<code>.xlsx` per code of `agent/checks.json`, on which the check reports
//! that code, and `golden/` (the previews of `basic.xlsx`).
//! `cargo run -p kition-sheet-kit --example checks -- conformance`. The workbooks are built on the
//! model and written by the engine's own XLSX writer, so they are the same on every run.

use std::path::Path;
use std::sync::Arc;

use kition_sheet_engine::core::{CellError, CellRef, RangeRef, Value};
use kition_sheet_engine::model::{Cell, DefinedName, Formula, Sheet, Workbook};

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

fn number(sheet: &mut Sheet, a1: &str, value: f64) {
    sheet.cells.set(cell(a1), Cell::value(Value::Number(value)));
}

fn text(sheet: &mut Sheet, a1: &str, value: &str) {
    sheet.cells.set(cell(a1), Cell::value(Value::Text(value.into())));
}

/// A formula with the value Excel would have saved for it.
fn formula(sheet: &mut Sheet, a1: &str, source: &str, value: Value) {
    let mut formula = Cell::formula(Formula::new(source));
    formula.value = value;
    sheet.cells.set(cell(a1), formula);
}

fn sales(sheet: &mut Sheet) {
    text(sheet, "A1", "Region");
    text(sheet, "B1", "Sales");
    for (row, (region, value)) in [("North", 120.0), ("South", 80.0), ("East", 100.0)].into_iter().enumerate() {
        text(sheet, &format!("A{}", row + 2), region);
        number(sheet, &format!("B{}", row + 2), value);
    }
    text(sheet, "A5", "Total");
    formula(sheet, "B5", "=SUM(B2:B4)", Value::Number(300.0));
}

fn fixtures() -> Vec<(&'static str, Workbook)> {
    let range = |a: &str, b: &str| RangeRef::new(cell(a), cell(b));
    let name =
        |name: &str, formula: &str| DefinedName { name: name.into(), scope: None, formula: formula.into(), comment: String::new(), hidden: false };
    vec![
        ("basic.xlsx", book(|sheet, _| sales(sheet))),
        (
            "check-formula-error.xlsx",
            book(|sheet, _| {
                sales(sheet);
                formula(sheet, "C2", "=B2/0", Value::Error(CellError::Div0));
            }),
        ),
        (
            "check-circular-reference.xlsx",
            book(|sheet, _| {
                sales(sheet);
                formula(sheet, "C2", "=C2+B2", Value::Error(CellError::Circ));
            }),
        ),
        (
            "check-reference-outside-data.xlsx",
            book(|sheet, _| {
                sales(sheet);
                formula(sheet, "C2", "=B2+Z900", Value::Number(120.0));
            }),
        ),
        (
            "check-merge-hides-data.xlsx",
            book(|sheet, _| {
                sales(sheet);
                sheet.merges = vec![range("A2", "B2")];
            }),
        ),
        (
            "check-overlapping-merges.xlsx",
            book(|sheet, _| {
                sales(sheet);
                sheet.merges = vec![range("D1", "E2"), range("E2", "F3")];
            }),
        ),
        (
            "check-broken-name.xlsx",
            book(|sheet, wb| {
                sales(sheet);
                wb.names = vec![name("Sales", "Data!$B$2:$B$4"), name("Targets", "#REF!")];
            }),
        ),
        (
            "check-font-substituted.xlsx",
            book(|sheet, wb| {
                sales(sheet);
                text(sheet, "D1", "Note");
                let mut style = wb.styles.get(kition_sheet_engine::model::StyleId::DEFAULT).clone();
                style.font.name = "Segoe UI".into();
                let id = wb.styles.intern(style);
                if let Some(content) = sheet.cells.get_mut(cell("D1")) {
                    content.style = id;
                }
            }),
        ),
        (
            "check-missing-glyph.xlsx",
            book(|sheet, _| {
                sales(sheet);
                text(sheet, "C2", "Done \u{2713}");
            }),
        ),
        (
            "check-truncated.xlsx",
            book(|sheet, _| {
                sales(sheet);
                for row in 0..105u32 {
                    let mut error = Cell::formula(Formula::new("=1/0"));
                    error.value = Value::Error(CellError::Div0);
                    sheet.cells.set(CellRef::new(row + 10, 4), error);
                }
            }),
        ),
    ]
}

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "conformance".into());
    let only: Vec<String> = std::env::args().skip(2).collect();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    for (name, wb) in fixtures() {
        if !only.is_empty() && !only.iter().any(|wanted| wanted == name) {
            continue;
        }
        let bytes = kition_sheet_engine::io::save_bytes(&wb, name).map_err(|error| error.to_string())?;
        std::fs::write(Path::new(&dir).join(name), &bytes).map_err(|error| format!("{name}: {error}"))?;
        println!("wrote {name}");
        if name == "basic.xlsx" {
            golden(Path::new(&dir), &bytes)?;
        }
    }
    Ok(())
}

/// `golden/basic.png` and `golden/basic-scale-2.png`: the engine's own render of `basic.xlsx`, at 1
/// and 2 pixels per point, which `<prefix>_inspect { preview }` must match.
fn golden(dir: &Path, bytes: &[u8]) -> Result<(), String> {
    use kition_kit::KitDocument;
    let doc = kition_sheet_kit::SheetDocument::open("basic.xlsx", bytes)?;
    let golden = dir.join("golden");
    std::fs::create_dir_all(&golden).map_err(|error| error.to_string())?;
    for (file, scale) in [("basic.png", 1.0), ("basic-scale-2.png", 2.0)] {
        let (width, height) = ((f64::from(doc.width().min(1200)) * scale) as u32, (f64::from(doc.height().min(900)) * scale) as u32);
        let rgba = doc.render(0.0, 0.0, scale, width, height)?;
        let picture = image::RgbaImage::from_raw(width, height, rgba).ok_or("a render of the wrong size")?;
        picture.save(golden.join(file)).map_err(|error| error.to_string())?;
        println!("wrote golden/{file}");
    }
    Ok(())
}
