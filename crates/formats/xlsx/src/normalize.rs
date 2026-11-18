//! Canonical entry values use exactly the units the XLSX reader reconstructs.
use std::sync::Arc;

use kition_sheet_model::{LineInfo, StyleId, Workbook, units};

use crate::tables::{col_width_to_px, pt_to_px, px_to_col_width, px_to_pt};

pub fn normalize_file_units(workbook: &mut Workbook) -> bool {
    let char_px = units::workbook_char_px(workbook);
    let automatic_row = (units::default_row_height_pt(workbook.styles.get(StyleId::DEFAULT).font.size) * units::PX_PER_PT) as f32;
    let column = |value| col_width_to_px(px_to_col_width(value, char_px), char_px);
    let row = |value| pt_to_px(units::file_row_height_pt(px_to_pt(value)));
    let mut changed = false;
    for original in &mut workbook.sheets {
        let mut sheet = (**original).clone();
        sheet.default_col_width = column(sheet.default_col_width);
        if sheet.default_col_width <= 0.0 {
            sheet.default_col_width = crate::FILE_DEFAULT_COL_PX;
        }
        sheet.default_row_height = row(sheet.default_row_height);
        if sheet.default_row_height <= 0.0 {
            sheet.default_row_height = automatic_row;
        }
        for info in sheet.cols.values_mut() {
            info.size = info.size.map(column);
            info.custom = false;
            info.outline = info.outline.min(7);
            if info.style == Some(StyleId::DEFAULT) {
                info.style = None;
            }
        }
        sheet.cols.retain(|_, info| {
            *info != LineInfo::default() && !(info.size == Some(sheet.default_col_width) && LineInfo { size: None, ..*info } == LineInfo::default())
        });
        for info in sheet.rows.values_mut() {
            info.size = info.size.map(row).filter(|height| info.custom || (*height - sheet.default_row_height).abs() > 0.5);
            info.custom &= info.size.is_some();
            info.outline = info.outline.min(7);
        }
        sheet.rows.retain(|_, info| *info != LineInfo::default());
        let (rows, cols) = sheet.outline_levels.unwrap_or((0, 0));
        let levels = (
            rows.max(sheet.rows.values().map(|info| info.outline).max().unwrap_or(0)),
            cols.max(sheet.cols.values().map(|info| info.outline).max().unwrap_or(0)),
        );
        sheet.outline_levels = (levels != (0, 0)).then_some(levels);
        for sparkline in &mut sheet.sparklines {
            if !sparkline.source.contains('!') {
                sparkline.source = format!("{}!{}", kition_sheet_formula::quote_sheet(&sheet.name), sparkline.source);
            }
        }
        let anchors: Vec<_> = sheet.shapes.iter().map(|shape| crate::drawing::file_anchor(&sheet, &shape.anchor)).collect();
        for (shape, anchor) in sheet.shapes.iter_mut().zip(anchors) {
            shape.anchor = anchor;
        }
        let anchors: Vec<_> = sheet.charts.iter().map(|chart| crate::drawing::file_anchor(&sheet, &chart.anchor)).collect();
        for (chart, anchor) in sheet.charts.iter_mut().zip(anchors) {
            chart.anchor = anchor;
        }
        let anchors: Vec<_> = sheet.images.iter().map(|image| crate::drawing::file_anchor(&sheet, &image.anchor)).collect();
        for (image, anchor) in sheet.images.iter_mut().zip(anchors) {
            image.anchor = anchor;
        }
        let anchors: Vec<_> = sheet.diagrams.iter().map(|diagram| crate::drawing::file_anchor(&sheet, &diagram.anchor)).collect();
        for (diagram, anchor) in sheet.diagrams.iter_mut().zip(anchors) {
            diagram.anchor = anchor;
        }
        if sheet != **original {
            *Arc::make_mut(original) = sheet;
            changed = true;
        }
    }
    changed
}
