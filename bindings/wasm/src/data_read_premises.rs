//! Sources read by data handlers outside their write rectangles.
use crate::SheetDocument;
use kition_sheet_engine::core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use serde_json::Value;

fn column(value: &Value, base: u32, limit: u32) -> Option<u32> {
    let index = match value {
        Value::String(text) => kition_sheet_engine::core::letters_to_col(text.trim_start_matches('$')),
        Value::Number(number) => number.as_u64().and_then(|offset| u32::try_from(offset).ok()).and_then(|offset| base.checked_add(offset)),
        _ => None,
    }?;
    (index < limit).then_some(index)
}

impl SheetDocument {
    pub(crate) fn independent_sort_key_sources(&self, params: &Value, sheet_index: usize, range: RangeRef) -> Result<Vec<RangeRef>, String> {
        let state = self.doc_state()?;
        let wb = &state.wb;
        let sheet = wb.sheet(sheet_index).ok_or("the sort names no sheet")?;
        let header = params.get("header").and_then(Value::as_bool).unwrap_or_else(|| {
            if let Some(table) = sheet.table_at(range.start) {
                return table.header_row;
            }
            if range.height() < 2 {
                return false;
            }
            (range.start.col..=range.end.col).any(|col| {
                let top = CellRef::new(range.start.row, col);
                let next = CellRef::new(range.start.row + 1, col);
                let a = sheet.value(top);
                let b = sheet.value(next);
                a.is_text() && !b.is_text() && !b.is_empty()
                    || wb.styles.get(sheet.style_id(top)).font.bold != wb.styles.get(sheet.style_id(next)).font.bold
            })
        });
        let by_columns = params.get("orientation").and_then(Value::as_str) == Some("columns");
        let body = if by_columns && header && range.width() > 1 {
            RangeRef::new(CellRef::new(range.start.row, range.start.col + 1), range.end)
        } else if !by_columns && header && range.height() > 1 {
            RangeRef::new(CellRef::new(range.start.row + 1, range.start.col), range.end)
        } else {
            range
        };
        if params.get("keys").and_then(Value::as_array).is_some_and(|keys| keys.len() > 64) {
            return Err("too many sort levels (64 max)".into());
        }
        let mut sources = Vec::new();
        for key in params.get("keys").and_then(Value::as_array).into_iter().flatten() {
            if by_columns && key.get("row").and_then(Value::as_u64).is_some_and(|row| !(1..=u64::from(MAX_ROWS)).contains(&row)) {
                return Err("a sort key row must be between 1 and 1048576".into());
            }
            if !(by_columns && key.get("row").and_then(Value::as_u64).is_some())
                && let Some(offset) = key.get("column").and_then(Value::as_u64)
            {
                let (base, limit) = if by_columns { (range.start.row, MAX_ROWS) } else { (range.start.col, MAX_COLS) };
                if u32::try_from(offset).ok().and_then(|offset| base.checked_add(offset)).is_none_or(|index| index >= limit) {
                    return Err("a sort key offset exceeds the sheet".into());
                }
            }
            let index = if by_columns {
                key.get("row")
                    .and_then(Value::as_u64)
                    .and_then(|row| u32::try_from(row).ok())
                    .and_then(|row| row.checked_sub(1))
                    .filter(|row| *row < MAX_ROWS)
                    .or_else(|| key.get("column").and_then(|value| column(value, range.start.row, MAX_ROWS)))
            } else {
                key.get("column").and_then(|value| column(value, range.start.col, MAX_COLS))
            };
            if let Some(index) = index {
                let source = if by_columns {
                    RangeRef::new(CellRef::new(index, body.start.col), CellRef::new(index, body.end.col))
                } else {
                    RangeRef::new(CellRef::new(body.start.row, index), CellRef::new(body.end.row, index))
                };
                if !body.contains_range(&source) && !sources.contains(&source) {
                    sources.push(source);
                }
            }
        }
        Ok(sources)
    }
}
