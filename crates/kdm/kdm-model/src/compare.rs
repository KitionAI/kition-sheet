//! [`normalize`]: an id-blind form of a [`Workbook`], for comparing views. Style ids are local to
//! each view (the order a table interned its styles), formula results are derived (never
//! stored), and the per-person state is each person's own (G4), so two views of the same content
//! (a round trip, or two replicas that converged) need not share them.

use std::collections::HashMap;
use std::sync::Arc;

use kition_sheet_core::{CellRef, Value};
use kition_sheet_model::{Cell, CellStore, Formula, StyleId, StyleTable, Workbook};

/// An id-blind copy of `wb`: the style table rebuilt with the styles in use only, numbered in
/// order of first use (each sheet's cells row by row, then its row and column info), every style
/// id rewritten to match; formula cells without their cached value and formulas without their
/// parse (`text` says it all); spills cleared; the active sheet, and each sheet's zoom, active
/// cell and scroll position, at their defaults; sheets without an id numbered as a read numbers
/// them (`Workbook::number_sheets`).
pub fn normalize(wb: &Workbook) -> Workbook {
    let mut out = wb.clone();
    out.identities = Default::default();
    out.active_sheet = 0;
    // A view whose sheets are not numbered yet is the numbered one: the ids a read gives them.
    out.number_sheets();

    // Styles by first use.
    let mut order: Vec<StyleId> = vec![StyleId::DEFAULT];
    let mut index: HashMap<StyleId, ()> = HashMap::from([(StyleId::DEFAULT, ())]);
    let mut note = |id: StyleId, order: &mut Vec<StyleId>| {
        if index.insert(id, ()).is_none() {
            order.push(id);
        }
    };
    for sheet in &wb.sheets {
        for (_, cell) in sheet.cells.iter() {
            note(cell.style, &mut order);
        }
        for info in sheet.rows.values().chain(sheet.cols.values()) {
            if let Some(s) = info.style {
                note(s, &mut order);
            }
        }
    }
    let mut table = StyleTable::default();
    table.set_default(wb.styles.get(StyleId::DEFAULT).clone());
    let map: HashMap<StyleId, StyleId> = order.iter().map(|id| (*id, table.intern(wb.styles.get(*id).clone()))).collect();
    out.styles = table;
    let restyle = |id: StyleId| map.get(&id).copied().unwrap_or_default();

    for sheet in out.sheets.iter_mut() {
        let sheet = Arc::make_mut(sheet);
        sheet.identities = Default::default();
        sheet.zoom = 100;
        sheet.view_active = CellRef::default();
        sheet.view_top_left = CellRef::default();
        sheet.spill.clear();
        sheet.spill_ranges.clear();
        let mut cells = CellStore::new();
        for (c, cell) in sheet.cells.iter() {
            let formula = cell.formula.as_ref().map(|f| Arc::new(Formula { text: f.text.clone(), expr: None, array: f.array }));
            let value = if formula.is_some() { Value::Empty } else { cell.value.clone() };
            let rich = cell.runs().map(|r| Arc::new(r.clone()));
            cells.set(c, Cell { value, formula, style: restyle(cell.style), rich });
        }
        sheet.cells = cells;
        for info in sheet.rows.values_mut().chain(sheet.cols.values_mut()) {
            info.style = info.style.map(restyle);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_ids_and_results_do_not_matter() {
        let mut a = Workbook::new();
        let bold = a.styles.derive(StyleId::DEFAULT, |s| s.font.bold = true);
        let italic = a.styles.derive(StyleId::DEFAULT, |s| s.font.italic = true);
        let mut b = Workbook::new();
        let italic_b = b.styles.derive(StyleId::DEFAULT, |s| s.font.italic = true);
        let bold_b = b.styles.derive(StyleId::DEFAULT, |s| s.font.bold = true);
        let put = |wb: &mut Workbook, bold: StyleId, italic: StyleId, result: f64| {
            let sheet = wb.sheet_mut(0).unwrap();
            sheet.cells.set(CellRef::new(0, 0), Cell { value: Value::Number(1.0), formula: None, style: bold, rich: None });
            let mut f = Cell::formula(Formula::new("=A1*2"));
            f.value = Value::Number(result);
            f.style = italic;
            sheet.cells.set(CellRef::new(1, 0), f);
            sheet.zoom = if result > 1.0 { 150 } else { 100 };
        };
        put(&mut a, bold, italic, 2.0);
        put(&mut b, bold_b, italic_b, 0.0);
        assert_ne!(a, b);
        assert_eq!(normalize(&a), normalize(&b));
    }
}
