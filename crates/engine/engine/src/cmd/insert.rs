//! Insert tab: tables, charts, sparklines, pictures, shapes, hyperlinks, comments and notes,
//! conditional formatting (Home › Styles) and symbols.

use kition_sheet_core::{CellRef, RangeRef};
use kition_sheet_model::*;
use serde_json::{Value as Json, json};

use super::*;
use crate::selection::current_region;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "insert.table",
            "Table",
            ["Insert", "Tables"],
            Some("Cmd+T"),
            "{range?, header?: true, style?: \"TableStyleMedium2\", name?, filter?: true, columnPrefix?: \"Column\", clearPictures?: bool (pictures in the header row go; without it they refuse the table)}",
            can_format_table,
            insert_table
        ),
        cmd!(
            "home.formatAsTable",
            "Format as Table",
            ["Home", "Styles"],
            None,
            "{range?, style?: \"TableStyleMedium2\" (default: the workbook's default table style), mode?: table|format (format: only the look, written into the cells, as WPS's Only Format As Table), titleRows?: 1 (format), header?, filter?: true, columnPrefix?: \"Column\" (names for empty headers), clearPictures?: bool (pictures in the header row go; without it they refuse the table)}",
            can_format_table,
            insert_table
        ),
        cmd!(query "table.styles", "Table Styles Gallery", [], None, "{slot?: 0..6 (the gallery's theme colour: Text 1, Accent 1–6)} → {slot, colors, groups: [{id: light|medium|dark, styles: [{name, preview}]}], custom: [{name, preview, elements}], default, current, table, range}", has_doc, table_styles),
        cmd!(
            "table.newStyle",
            "New Table Style…",
            ["Home", "Styles", "Format as Table"],
            None,
            "{name, elements: {wholeTable?|headerRow?|totalRow?|firstColumn?|lastColumn?|firstRowStripe?|secondRowStripe?|firstColumnStripe?|secondColumnStripe?: {fill?: \"#RRGGBB\", color?: \"#RRGGBB\", bold?, italic?, border?: \"#RRGGBB\" (around it), lines?: \"#RRGGBB\" (inside it), size?: 1 (stripes)}}, replace?: \"name\" (modifies that style), default?: bool}",
            has_doc,
            table_new_style
        ),
        cmd!(
            "table.deleteStyle",
            "Delete Table Style",
            [],
            None,
            "{name} (a style the workbook defines; its tables take the default style)",
            has_doc,
            table_delete_style
        ),
        cmd!("table.setDefaultStyle", "Set As Default Table Style", [], None, "{name} (\"\": Kition Sheet's default)", has_doc, table_default_style),
        cmd!("table.totalRow", "Total Row", ["Table", "Table Style Options"], None, "{table?, on?: bool}", has_doc, |s, p| table_flag(
            s, p, "totals"
        )),
        cmd!("table.bandedRows", "Banded Rows", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(
            s,
            p,
            "bandedRows"
        )),
        cmd!("table.bandedColumns", "Banded Columns", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(
            s,
            p,
            "bandedCols"
        )),
        cmd!("table.headerRow", "Header Row", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, table_header),
        cmd!("table.filterButton", "Filter Button", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(
            s, p, "filter"
        )),
        cmd!("table.firstColumn", "First Column", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(
            s, p, "firstCol"
        )),
        cmd!("table.lastColumn", "Last Column", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(s, p, "lastCol")),
        cmd!(
            "table.style",
            "Table Styles",
            ["Table", "Table Styles"],
            None,
            "{table?, style: \"TableStyleMedium2\" | a style the workbook defines | \"None\" (no style)}",
            has_doc,
            table_style
        ),
        cmd!("table.rename", "Table Name", ["Table", "Properties"], None, "{table?, name}", has_doc, table_rename),
        cmd!("table.convertToRange", "Convert to Range", ["Table", "Tools"], None, "{table?}", has_doc, table_convert),
        cmd!(
            "table.totalFunction",
            "Totals Function",
            [],
            None,
            "{table?, column: \"Sales\", function: sum|average|count|countNums|max|min|stdDev|var|none}",
            has_doc,
            table_total_fn
        ),
        cmd!(
            "insert.chart",
            "Insert Chart",
            ["Insert", "Charts"],
            None,
            "{range?, type?: column|bar|line|pie|doughnut|area|scatter|bubble|radar|combo|histogram|waterfall|funnel|treemap|sunburst|boxWhisker|stock, subtype?: clustered|stacked|stacked100|markers, title?, at?: \"H2\", width?, height?, seriesTypes?: [column|line|area…], secondary?: [series index], totals?: [point index]}",
            has_doc,
            insert_chart
        ),
        cmd!("insert.recommendedCharts", "Recommended Charts", ["Insert", "Charts"], None, "{range?}", has_doc, |s, p| insert_chart(
            s,
            &with(p, "type", json!("auto"))
        )),
        cmd!(
            "chart.set",
            "Chart Properties",
            ["Chart Design"],
            None,
            "{chart?: id, type?, title?, legend?: none|bottom|top|left|right, dataLabels?, gridlines?, style?, xTitle?, yTitle?, at?, width?, height?, dx?, dy?, totals?: [point index]}",
            has_doc,
            chart_set
        ),
        cmd!("chart.switchRowColumn", "Switch Row/Column", ["Chart Design", "Data"], None, "{chart?}", has_doc, chart_switch),
        cmd!("chart.delete", "Delete Chart", [], None, "{chart?}", has_doc, chart_delete),
        cmd!(
            "insert.sparkline",
            "Sparklines",
            ["Insert", "Sparklines"],
            None,
            "{range: \"B2:M2\", location: \"N2\", type?: line|column|winLoss, markers?}",
            has_doc,
            insert_sparkline
        ),
        cmd!(
            "insert.picture",
            "Picture",
            ["Insert", "Illustrations"],
            None,
            "{path? | base64?: \"...\", mime?, at?: \"B2\", width?, height?, alt?, mode?: moveOnly|moveAndSize|absolute, placement?: cell|overCells}",
            has_doc,
            insert_picture
        ),
        cmd!(
            "insert.shape",
            "Shapes",
            ["Insert", "Illustrations"],
            None,
            "{kind: rectangle|roundedRectangle|ellipse|triangle|line|arrow|textBox, at?, width?, height?, text?, fill?, line?}",
            has_doc,
            insert_shape
        ),
        cmd!(
            "insert.icons",
            "Icons",
            ["Insert", "Illustrations"],
            None,
            "{name: chart|table|sum|filter|lock|comment|picture|book|folder|search|calc|check|… , at?, size?: 48, color?: hex}",
            has_doc,
            insert_icon
        ),
        cmd!("insert.textBox", "Text Box", ["Insert", "Text"], None, "{at?, width?, height?, text?}", has_doc, |s, p| insert_shape(
            s,
            &with(p, "kind", json!("textBox"))
        )),
        // Art text: a text box without fill or outline, its text large and coloured, optionally
        // bent by a preset text warp (KitDrawing 1.3 DR26), as `drawing.insert` makes it.
        cmd!(
            "insert.artText",
            "Art text",
            ["Insert", "Text"],
            None,
            "{text?, warp?: preset (textArchUp, textWave1, textInflate…)}",
            has_doc,
            |s, p| {
                let text = p.get("text").and_then(Json::as_str).unwrap_or("Your text here");
                let mut params = json!({
                    "kind": "textBox",
                    "paragraphs": [{ "align": "center", "runs": [{ "text": text, "size": 36, "bold": true, "color": { "rgb": "#156082" } }] }],
                    "fill": { "none": true },
                    "stroke": { "none": true },
                });
                if let Some(w) = p.get("warp") {
                    params["warp"] = w.clone();
                }
                super::drawing::insert(s, &params)
            }
        ),
        cmd!("shape.setText", "Edit Text Box", [], None, "{id: textBox id, text: string}", has_doc, shape_set_text),
        cmd!("object.delete", "Delete Object", [], None, "{kind: chart|image|shape, id}", has_doc, delete_object),
        cmd!("object.move", "Move Object", [], None, "{kind: chart|image|shape, id, at?: \"C3\", dx?, dy?, width?, height?}", has_doc, move_object),
        cmd!(
            "object.setAnchorMode",
            "Object Properties",
            [],
            None,
            "{kind: chart|image|shape, id, mode: moveAndSize|moveOnly|absolute}",
            has_doc,
            set_anchor_mode
        ),
        cmd!(
            "insert.link",
            "Link",
            ["Insert", "Links"],
            Some("Cmd+K"),
            "{cell?, target: \"https://…\" | \"Sheet2!A1\" | \"mailto:…\", text?, tooltip?, remove?: bool}",
            crate::cmd::extra::not_on_checkbox,
            insert_link
        ),
        cmd!("review.newNote", "New Note", ["Review", "Notes"], None, "{cell?, text, author?}", has_doc, |s, p| comment(s, p, false)),
        cmd!("review.newComment", "New Comment", ["Review", "Comments"], None, "{cell?, text, author?}", has_doc, |s, p| comment(s, p, true)),
        cmd!("review.replyComment", "Reply", [], None, "{cell?, text, author?}", has_doc, reply_comment),
        cmd!("review.resolveComment", "Resolve Thread", [], None, "{cell?, resolved?: true}", has_doc, resolve_comment),
        cmd!("review.deleteComment", "Delete Comment", ["Review", "Comments"], None, "{cell?}", has_doc, delete_comment),
        cmd!("review.showNote", "Show/Hide Note", ["Review", "Notes"], None, "{cell?}", has_doc, show_note),
        cmd!(
            "insert.symbol",
            "Symbol",
            ["Insert", "Symbols"],
            None,
            "{char: \"€\"} (inserted at the end of the active cell's text)",
            has_doc,
            insert_symbol
        ),
        cmd!(
            "home.conditionalFormat",
            "Conditional Formatting",
            ["Home", "Styles"],
            None,
            "{range?, rule: {type: cellIs|expression|containsText|notContainsText|beginsWith|endsWith|blanks|noBlanks|errors|noErrors|duplicate|unique|top10|aboveAverage|timePeriod|colorScale|dataBar|iconSet, operator?: greaterThan|lessThan|greaterThanOrEqual|lessThanOrEqual|equal|notEqual|between|notBetween, value?, value2?, formula?, text?, rank?, bottom?, percent?, below?, period?, colors?: [low, mid?, high], color?, min?, max?, gradient?, set?: 3Arrows|3TrafficLights1|3Flags|3Symbols|3Stars|4Rating|5Quarters…, thresholds?: [boundaries between icons, ascending: numbers (percent of the min–max span) or {type: number|percent|percentile|formula, value}], reverse?, showValue?, style?: {fill: \"#RRGGBB\", font: {color: \"#RRGGBB\", bold, italic, strike}, borders}|preset: lightRedFill|redText|yellowFill|greenFill|redBorder|lightRedFillDarkRedText…}} (a rule refuses keys it does not read; newer rules take precedence)",
            has_doc,
            add_cf
        ),
        cmd!("home.clearRules", "Clear Rules", ["Home", "Styles", "Conditional Formatting"], None, "{range?|sheet: true}", has_doc, clear_cf),
        cmd!(
            "home.manageRules",
            "Manage Rules…",
            ["Home", "Styles", "Conditional Formatting"],
            None,
            "{delete?: index, moveUp?: index, moveDown?: index}",
            has_doc,
            manage_cf
        ),
    ]
}

fn with(p: &Json, k: &str, v: Json) -> Json {
    let mut m = p.as_object().cloned().unwrap_or_default();
    m.insert(k.into(), v);
    Json::Object(m)
}

fn block(s: &Session, p: &Json) -> Result<RangeRef> {
    if p.get("range").is_some() {
        return target_range(s, p);
    }
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let sel = d.selection.current();
    if !sel.is_single() {
        return Ok(sel);
    }
    Ok(current_region(sh, d.selection.active))
}

// ---------------------------------------------------------------- tables

/// Whether Format as Table (and Insert › Table) can run on the selection: one area, not several,
/// on a sheet that is not protected (WPS greys the button otherwise).
fn can_format_table(s: &Session) -> std::result::Result<(), String> {
    has_doc(s)?;
    let d = s.active().ok_or("no workbook open")?;
    if d.selection.ranges.len() > 1 {
        return Err("select one range (not several) to format as a table".into());
    }
    if d.wb.active().is_some_and(|sh| sh.protection.is_some()) {
        return Err("the sheet is protected".into());
    }
    Ok(())
}

/// The range Format as Table takes: the one given, else the selection, else (one cell) the data
/// around it; whole rows or columns take the part of them that holds something.
fn table_block(s: &Session, p: &Json, sheet: usize) -> Result<RangeRef> {
    let r = block(s, p)?;
    if !(r.is_full_rows() || r.is_full_cols()) {
        return Ok(r);
    }
    s.doc()?
        .wb
        .sheet(sheet)
        .and_then(|sh| sh.used_range())
        .and_then(|u| r.intersection(&u))
        .ok_or_else(|| EngineError::Other("There is nothing in the selected rows or columns to format as a table.".into()))
}

fn insert_table(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let r = table_block(s, p, sheet)?;
    let d = s.doc()?;
    let style = match str_param(p, "style") {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => crate::tables::default_style(&d.wb),
    };
    if str_param(p, "mode") == Some("format") {
        let rows = u32_param(p, "titleRows").unwrap_or(1).min(r.height());
        return format_only(s, sheet, r, &style, rows);
    }
    if d.wb.sheet(sheet).is_some_and(|sh| sh.tables.iter().any(|t| t.range.intersects(&r))) {
        return Err(EngineError::Other("A table can't overlap another table.".into()));
    }
    if r.count() > 50_000_000 {
        return Err(bad("insert.table", "range too large"));
    }
    // A table can't hold part of an array formula (Excel, WPS).
    let arrays = d
        .wb
        .sheet(sheet)
        .is_some_and(|sh| sh.cells.iter().any(|(_, x)| x.formula.as_ref().and_then(|f| f.array).is_some_and(|a| !a.is_single() && a.intersects(&r))));
    if arrays {
        return Err(EngineError::Other("Multi-cell array formulas aren't allowed in tables.".into()));
    }
    let header = bool_param(p, "header").unwrap_or_else(|| {
        let sh = d.wb.sheet(sheet);
        sh.is_some_and(|sh| (r.start.col..=r.end.col).all(|c| sh.value(CellRef::new(r.start.row, c)).is_text()) && r.height() > 1)
    });
    // A header row can't hold pictures (WPS asks, then clears them; the header is named 列1…).
    let header_pictures: Vec<CellRef> = if header {
        d.wb.sheet(sheet)
            .map(|sh| sh.cell_pictures.keys().filter(|c| c.row == r.start.row && c.col >= r.start.col && c.col <= r.end.col).copied().collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if !header_pictures.is_empty() && bool_param(p, "clearPictures") != Some(true) {
        return Err(EngineError::Other("A table's header row can't hold pictures.".into()));
    }
    let name = str_param(p, "name")
        .map(str::to_string)
        .unwrap_or_else(|| (1..).map(|n| format!("Table{n}")).find(|n| d.wb.table(n).is_none()).unwrap_or_else(|| "Table".into()));
    if d.wb.table(&name).is_some() || d.wb.names.iter().any(|n| n.name.eq_ignore_ascii_case(&name)) {
        return Err(EngineError::Other("The name entered already exists. Enter a unique name.".into()));
    }
    let prefix = str_param(p, "columnPrefix").filter(|x| !x.trim().is_empty()).map_or_else(|| s.prefs.column_prefix.clone(), str::to_string);
    let filter = bool_param(p, "filter").unwrap_or(true);
    let id = next_object_id(&d.wb)?;
    edit(s, |cx| {
        // Merged cells in a table come apart (their text stays in the first cell).
        cx.sheet_mut(sheet)?.merges.retain(|m| !m.intersects(&r));
        for c in &header_pictures {
            let sh = cx.sheet_mut(sheet)?;
            sh.cell_pictures.remove(c);
            // The picture's stand-in value (#VALUE!) goes too: the header is named 列1….
            sh.set_value(*c, kition_sheet_core::Value::Empty);
            cx.touch(sheet, *c);
        }
        // Without a header row, insert one with generic names.
        let r = if header {
            r
        } else {
            let sh = cx.sheet_mut(sheet)?;
            sh.cells.shift_rows_in_cols(r.start.col, r.end.col, r.start.row, 1);
            sh.cell_pictures = std::mem::take(&mut sh.cell_pictures)
                .into_iter()
                .filter_map(|(c, v)| {
                    let dest = if c.col >= r.start.col && c.col <= r.end.col && c.row >= r.start.row { c.offset(1, 0) } else { Some(c) };
                    dest.map(|c| (c, v))
                })
                .collect();
            for (i, c) in (r.start.col..=r.end.col).enumerate() {
                sh.set_value(CellRef::new(r.start.row, c), kition_sheet_core::Value::text(format!("{prefix}{}", i + 1)));
            }
            cx.structural = true;
            RangeRef::new(r.start, CellRef::new(r.end.row + 1, r.end.col))
        };
        let sh = cx.sheet_mut(sheet)?;
        let mut names: Vec<String> = Vec::new();
        // Lower-case names taken, and the next suffix for each header, so that thousands of equal
        // headers don't take cubic time.
        let mut taken = std::collections::HashSet::new();
        let mut next_suffix = std::collections::HashMap::new();
        for c in r.start.col..=r.end.col {
            // A header shows its text: a formula there becomes the value it shows (Excel, WPS).
            let mut n = sh.value(CellRef::new(r.start.row, c)).display();
            if n.is_empty() {
                n = format!("{prefix}{}", c - r.start.col + 1);
            }
            let base = n.clone();
            let key = base.to_ascii_lowercase();
            let mut k = next_suffix.get(&key).copied().unwrap_or(2);
            while taken.contains(&n.to_ascii_lowercase()) {
                n = format!("{base}{k}");
                k += 1;
            }
            next_suffix.insert(key, k);
            taken.insert(n.to_ascii_lowercase());
            // Headers are text.
            sh.set_value(CellRef::new(r.start.row, c), kition_sheet_core::Value::text(n.as_str()));
            names.push(n);
        }
        sh.tables.push(Table {
            id,
            name: name.clone(),
            range: r,
            header_row: true,
            totals_row: false,
            columns: names.into_iter().map(|n| TableColumn { name: n, totals: TotalsFn::None, totals_label: None, formula: None }).collect(),
            style: style.clone(),
            banded_rows: true,
            banded_cols: false,
            first_col: false,
            last_col: false,
            filter_button: filter,
            criteria: vec![],
        });
        cx.structural = true;
        Ok(json!({"table": name, "range": r.a1(), "style": style}))
    })
}

/// Only Format As Table (WPS): the style's look written into the cells as their own format, with
/// `title_rows` header rows; no table is made.
fn format_only(s: &mut Session, sheet: usize, r: RangeRef, style: &str, title_rows: u32) -> Result<Json> {
    if r.count() > 2_000_000 {
        return Err(bad("home.formatAsTable", "range too large to format cell by cell"));
    }
    if crate::tables::style_def(&s.doc()?.wb, style).is_none() {
        return Err(bad("home.formatAsTable", format!("no table style named `{style}`")));
    }
    edit(s, |cx| {
        let looks = crate::tables::range_looks(&cx.wb, sheet, r, style, title_rows);
        for (c, st) in looks {
            let id = cx.wb.styles.intern(st);
            cx.sheet_mut(sheet)?.set_style(c, id);
            cx.touch(sheet, c);
        }
        Ok(json!({"range": r.a1(), "style": style, "mode": "format"}))
    })
}

/// A table style element as `table.newStyle` takes it (and `table.styles` answers it).
fn element_style(kind: &str, v: &Json) -> Result<TableStyleElement> {
    let colour = |key: &str| -> Result<Option<Color>> {
        match str_param(v, key) {
            None | Some("") => Ok(None),
            Some(x) => Color::from_hex(x).map(Some).ok_or_else(|| bad("table.newStyle", format!("`{kind}.{key}`: not a colour: {x} (\"#RRGGBB\")"))),
        }
    };
    let mut st = Style::default();
    if let Some(c) = colour("fill")? {
        st.fill = Fill::solid(c);
    }
    if let Some(c) = colour("color")? {
        st.font.color = c;
    }
    st.font.bold = bool_param(v, "bold").unwrap_or(false);
    st.font.italic = bool_param(v, "italic").unwrap_or(false);
    if let Some(c) = colour("border")? {
        let line = BorderLine { style: BorderStyle::Thin, color: c };
        st.borders.top = line;
        st.borders.bottom = line;
        st.borders.left = line;
        st.borders.right = line;
    }
    if let Some(c) = colour("lines")? {
        let line = BorderLine { style: BorderStyle::Thin, color: c };
        st.borders.horizontal = line;
        st.borders.vertical = line;
    }
    let size = u32_param(v, "size").filter(|n| *n > 1 && kind.ends_with("Stripe")).map(|n| n.min(9));
    Ok(TableStyleElement { kind: kind.to_string(), size, style: st })
}

/// What `table.newStyle` would take to make style `def` again (for New Table Style's Modify).
fn element_specs(wb: &Workbook, def: &TableStyleDef) -> Json {
    let hex = |c: Color| c.hex(&wb.theme);
    let mut out = serde_json::Map::new();
    for e in &def.elements {
        let st = &e.style;
        let mut v = serde_json::Map::new();
        if st.fill.pattern != PatternType::None {
            v.insert("fill".into(), json!(hex(st.fill.fg)));
        }
        if st.font.color != Color::Auto {
            v.insert("color".into(), json!(hex(st.font.color)));
        }
        if st.font.bold {
            v.insert("bold".into(), json!(true));
        }
        if st.font.italic {
            v.insert("italic".into(), json!(true));
        }
        if !st.borders.top.is_none() {
            v.insert("border".into(), json!(hex(st.borders.top.color).unwrap_or_else(|| "#000000".into())));
        }
        if !st.borders.horizontal.is_none() {
            v.insert("lines".into(), json!(hex(st.borders.horizontal.color).unwrap_or_else(|| "#000000".into())));
        }
        if let Some(n) = e.size {
            v.insert("size".into(), json!(n));
        }
        out.insert(e.kind.clone(), Json::Object(v));
    }
    Json::Object(out)
}

/// The gallery: its theme colours, the built-in styles for one of them by group, the workbook's
/// own styles, the default, the active table's style, and the range Format as Table would take.
fn table_styles(s: &mut Session, p: &Json) -> Result<Json> {
    use crate::tables::*;
    let d = s.doc()?;
    let wb = &d.wb;
    let sheet = wb.active_sheet;
    let table = wb.sheet(sheet).and_then(|sh| sh.table_at(d.selection.active));
    let current = table.map(|t| t.style.clone());
    let slot = u32_param(p, "slot").or_else(|| current.as_deref().and_then(slot_of)).unwrap_or(1).min(COLOR_SLOTS - 1);
    let colors: Vec<Json> = (0..COLOR_SLOTS).map(|k| json!(Color::Theme(slot_theme(k), 0).hex(&wb.theme))).collect();
    let item = |name: &str| style_def(wb, name).map(|def| json!({ "name": name, "preview": preview(wb, def) }));
    let groups: Vec<Json> = gallery(slot)
        .into_iter()
        .map(|(id, names)| json!({ "id": id, "styles": names.iter().filter_map(|n| item(n)).collect::<Vec<_>>() }))
        .collect();
    let custom: Vec<Json> =
        custom_styles(wb).map(|def| json!({ "name": def.name, "preview": preview(wb, def), "elements": element_specs(wb, def) })).collect();
    let range = match table {
        Some(t) => Some(t.range.a1()),
        None => table_block(s, &json!({}), sheet).ok().map(|r| r.a1()),
    };
    Ok(json!({
        "slot": slot,
        "colors": colors,
        "groups": groups,
        "custom": custom,
        "default": default_style(wb),
        "current": current,
        "table": table.map(|t| t.name.clone()),
        "range": range,
    }))
}

fn table_new_style(s: &mut Session, p: &Json) -> Result<Json> {
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad("table.newStyle", "missing `name`"))?.to_string();
    if name.chars().count() > 255 {
        return Err(bad("table.newStyle", "the name is longer than 255 characters"));
    }
    if crate::tables::is_builtin(&name) || name.eq_ignore_ascii_case("None") {
        return Err(EngineError::Other(format!("“{name}” is a built-in style's name. Enter another name.")));
    }
    let replace = str_param(p, "replace").map(str::to_string);
    let wb = &s.doc()?.wb;
    let taken = crate::tables::custom_styles(wb)
        .any(|d| d.name.eq_ignore_ascii_case(&name) && replace.as_deref().is_none_or(|r| !r.eq_ignore_ascii_case(&name)));
    if taken {
        return Err(EngineError::Other(format!("A table style named “{name}” already exists. Enter another name.")));
    }
    if let Some(r) = &replace
        && !crate::tables::custom_styles(wb).any(|d| d.name.eq_ignore_ascii_case(r))
    {
        return Err(bad("table.newStyle", format!("no table style named `{r}` to modify")));
    }
    let mut elements = Vec::new();
    if let Some(Json::Object(map)) = p.get("elements") {
        for kind in crate::tables::ELEMENT_ORDER {
            if let Some(v) = map.get(kind) {
                elements.push(element_style(kind, v)?);
            }
        }
        if let Some(unknown) = map.keys().find(|k| !crate::tables::ELEMENT_ORDER.contains(&k.as_str())) {
            return Err(bad("table.newStyle", format!("`elements`: no table part named `{unknown}`")));
        }
    }
    let default = bool_param(p, "default");
    let uid = replace
        .as_ref()
        .and_then(|old| wb.table_styles.styles.iter().find(|style| style.name.eq_ignore_ascii_case(old)))
        .map(kdm_model::table_style_uid)
        .unwrap_or_else(|| format!("{{{}}}", kdm_model::kition_kit::kdm::ObjectId::new()));
    edit(s, |cx| {
        let ts = &mut cx.wb.table_styles;
        let def = TableStyleDef { name: name.clone(), table: None, pivot: Some(false), uid: Some(uid), elements };
        match &replace {
            Some(old) => {
                if let Some(slot) = ts.styles.iter_mut().find(|d| d.name.eq_ignore_ascii_case(old)) {
                    *slot = def;
                }
                if ts.default_table.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(old)) {
                    ts.default_table = Some(name.clone());
                }
                for i in 0..cx.wb.sheets.len() {
                    if cx.wb.sheets.get(i).is_some_and(|sh| sh.tables.iter().any(|t| t.style.eq_ignore_ascii_case(old)))
                        && let Some(sh) = cx.wb.sheet_mut(i)
                    {
                        for t in sh.tables.iter_mut().filter(|t| t.style.eq_ignore_ascii_case(old)) {
                            t.style = name.clone();
                        }
                    }
                }
            }
            None => ts.styles.push(def),
        }
        match default {
            Some(true) => cx.wb.table_styles.default_table = Some(name.clone()),
            Some(false) if cx.wb.table_styles.default_table.as_deref() == Some(name.as_str()) => cx.wb.table_styles.default_table = None,
            _ => {}
        }
        cx.structural = true;
        Ok(json!({ "name": name }))
    })
}

fn table_delete_style(s: &mut Session, p: &Json) -> Result<Json> {
    let name = str_param(p, "name").ok_or_else(|| bad("table.deleteStyle", "missing `name`"))?.to_string();
    if !crate::tables::custom_styles(&s.doc()?.wb).any(|d| d.name.eq_ignore_ascii_case(&name)) {
        return Err(bad("table.deleteStyle", format!("no table style named `{name}` (only styles the workbook defines can be deleted)")));
    }
    edit(s, |cx| {
        let ts = &mut cx.wb.table_styles;
        ts.styles.retain(|d| !d.name.eq_ignore_ascii_case(&name));
        if ts.default_table.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(&name)) {
            ts.default_table = None;
        }
        let fallback = crate::tables::default_style(&cx.wb);
        for i in 0..cx.wb.sheets.len() {
            if cx.wb.sheets.get(i).is_some_and(|sh| sh.tables.iter().any(|t| t.style.eq_ignore_ascii_case(&name)))
                && let Some(sh) = cx.wb.sheet_mut(i)
            {
                for t in sh.tables.iter_mut().filter(|t| t.style.eq_ignore_ascii_case(&name)) {
                    t.style = fallback.clone();
                }
            }
        }
        cx.structural = true;
        Ok(Json::Null)
    })
}

fn table_default_style(s: &mut Session, p: &Json) -> Result<Json> {
    let name = str_param(p, "name").map(str::trim).ok_or_else(|| bad("table.setDefaultStyle", "missing `name`"))?.to_string();
    let wb = &s.doc()?.wb;
    let known = name.is_empty() || crate::tables::is_builtin(&name) || crate::tables::custom_styles(wb).any(|d| d.name.eq_ignore_ascii_case(&name));
    if !known {
        return Err(bad("table.setDefaultStyle", format!("no table style named `{name}`")));
    }
    edit(s, |cx| {
        cx.wb.table_styles.default_table = (!name.is_empty()).then(|| name.clone());
        Ok(json!({ "default": crate::tables::default_style(&cx.wb) }))
    })
}

fn find_table(s: &Session, p: &Json) -> Result<(usize, usize)> {
    let d = s.doc()?;
    if let Some(n) = str_param(p, "table") {
        return d.wb.table(n).ok_or_else(|| bad("table", format!("no table named `{n}`")));
    }
    let sheet = d.wb.active_sheet;
    let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    let ti = sh
        .tables
        .iter()
        .position(|t| t.range.contains(d.selection.active))
        .ok_or_else(|| EngineError::Disabled("table".into(), "select a cell in a table".into()))?;
    Ok((sheet, ti))
}

fn table_flag(s: &mut Session, p: &Json, which: &str) -> Result<Json> {
    let (si, ti) = find_table(s, p)?;
    let on = bool_param(p, "on");
    edit(s, |cx| {
        let sh = cx.sheet_mut(si)?;
        let Some(t) = sh.tables.get_mut(ti) else { return Ok(Json::Null) };
        match which {
            "totals" => {
                let v = on.unwrap_or(!t.totals_row);
                if v != t.totals_row {
                    t.totals_row = v;
                    if v {
                        t.range.end.row += 1;
                        if let Some(first) = t.columns.first_mut() {
                            first.totals_label = Some("Total".into());
                        }
                        if t.columns.len() > 1
                            && let Some(last) = t.columns.last_mut()
                            && last.totals == TotalsFn::None
                        {
                            last.totals = TotalsFn::Sum;
                        }
                    } else {
                        let row = t.range.end.row;
                        t.range.end.row = t.range.end.row.saturating_sub(1);
                        let (c0, c1) = (t.range.start.col, t.range.end.col);
                        for c in c0..=c1 {
                            sh.remove_cell(CellRef::new(row, c));
                        }
                    }
                    write_totals(sh, ti);
                    cx.structural = true;
                }
            }
            "bandedRows" => t.banded_rows = on.unwrap_or(!t.banded_rows),
            "bandedCols" => t.banded_cols = on.unwrap_or(!t.banded_cols),
            "firstCol" => t.first_col = on.unwrap_or(!t.first_col),
            "lastCol" => t.last_col = on.unwrap_or(!t.last_col),
            "filter" => t.filter_button = on.unwrap_or(!t.filter_button),
            _ => {}
        }
        Ok(Json::Null)
    })
}

/// Writes the totals row cells (labels and SUBTOTAL formulas).
fn write_totals(sh: &mut Sheet, ti: usize) {
    let Some(t) = sh.tables.get(ti).cloned() else { return };
    if !t.totals_row {
        return;
    }
    let row = t.range.end.row;
    for (i, col) in t.columns.iter().enumerate() {
        let c = CellRef::new(row, t.range.start.col + i as u32);
        if let Some(code) = col.totals.subtotal_code() {
            let f = format!("=SUBTOTAL({code},{}[{}])", t.name, col.name);
            sh.set_cell(
                c,
                Cell {
                    formula: Some(std::sync::Arc::new(Formula::new(&f))),
                    value: kition_sheet_core::Value::Empty,
                    style: sh.style_id(c),
                    rich: None,
                },
            );
        } else if let Some(l) = &col.totals_label {
            sh.set_value(c, kition_sheet_core::Value::text(l.as_str()));
        } else {
            sh.remove_cell(c);
        }
    }
}

fn table_total_fn(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ti) = find_table(s, p)?;
    let col = str_param(p, "column").ok_or_else(|| bad("table.totalFunction", "missing `column`"))?.to_string();
    let f = match str_param(p, "function").unwrap_or("sum") {
        "average" => TotalsFn::Average,
        "count" => TotalsFn::Count,
        "countNums" => TotalsFn::CountNums,
        "max" => TotalsFn::Max,
        "min" => TotalsFn::Min,
        "stdDev" => TotalsFn::StdDev,
        "var" => TotalsFn::Var,
        "none" => TotalsFn::None,
        _ => TotalsFn::Sum,
    };
    edit(s, |cx| {
        let sh = cx.sheet_mut(si)?;
        if let Some(t) = sh.tables.get_mut(ti)
            && let Some(i) = t.column_index(&col)
            && let Some(c) = t.columns.get_mut(i)
        {
            c.totals = f;
        }
        write_totals(sh, ti);
        cx.structural = true;
        Ok(Json::Null)
    })
}

/// Header Row: off, the header's cells leave the table (their text goes, the names stay); on, the
/// row above the data takes the column names again.
fn table_header(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ti) = find_table(s, p)?;
    let t = s.doc()?.wb.sheet(si).and_then(|sh| sh.tables.get(ti)).cloned().ok_or(EngineError::NoDocument)?;
    let on = bool_param(p, "on").unwrap_or(!t.header_row);
    if on == t.header_row {
        return ok();
    }
    if on && t.range.start.row == 0 {
        return Err(EngineError::Other("There is no row above the table for its header.".into()));
    }
    if !on && t.range.height() <= 1 {
        return Err(EngineError::Other("A table needs a row besides its header.".into()));
    }
    edit(s, |cx| {
        let sh = cx.sheet_mut(si)?;
        let (c0, c1) = (t.range.start.col, t.range.end.col);
        if on {
            let row = t.range.start.row - 1;
            for (i, col) in t.columns.iter().enumerate() {
                let c = CellRef::new(row, c0 + i as u32);
                sh.set_value(c, kition_sheet_core::Value::text(col.name.as_str()));
            }
        } else {
            let row = t.range.start.row;
            for c in c0..=c1 {
                let at = CellRef::new(row, c);
                let style = sh.style_id(at);
                sh.cells.set(at, Cell { style, ..Default::default() });
            }
        }
        if let Some(x) = sh.tables.get_mut(ti) {
            x.header_row = on;
            if on {
                x.range.start.row -= 1;
            } else {
                x.range.start.row += 1;
            }
        }
        // A selection on the header goes to the first row of data, so the table stays selected.
        let (row, active) = (t.range.start.row, cx.sel.active);
        if !on && active.row == row && (c0..=c1).contains(&active.col) {
            *cx.sel = crate::Selection::range(RangeRef::cell(CellRef::new(row + 1, active.col)));
        }
        cx.structural = true;
        Ok(Json::Null)
    })
}

fn table_style(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ti) = find_table(s, p)?;
    let style = str_param(p, "style").map(str::trim).unwrap_or("TableStyleMedium2");
    // No style ("None", Clear in the gallery): the table keeps its options but shows no look.
    let style = if style.eq_ignore_ascii_case("None") { String::new() } else { style.to_string() };
    edit(s, |cx| {
        if let Some(t) = cx.sheet_mut(si)?.tables.get_mut(ti) {
            t.style = style.clone();
        }
        Ok(Json::Null)
    })
}

/// Names rewritten by a table rename, excluding string literals and function names.
pub fn table_references(expr: &kition_sheet_formula::Expr) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    expr.walk(&mut |node| match node {
        kition_sheet_formula::Expr::Struct(reference) => {
            names.insert(reference.table.to_ascii_lowercase());
        }
        kition_sheet_formula::Expr::Name(name) => {
            names.insert(name.to_ascii_lowercase());
        }
        _ => {}
    });
    names
}

fn table_rename(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ti) = find_table(s, p)?;
    let name = str_param(p, "name").ok_or_else(|| bad("table.rename", "missing `name`"))?.to_string();
    if name.is_empty()
        || name.contains(' ')
        || name.chars().next().is_some_and(|c| c.is_ascii_digit())
        || kition_sheet_core::CellRef::parse(&name).is_some()
    {
        return Err(EngineError::Other("The name that you entered is not valid.".into()));
    }
    if s.doc()?.wb.table(&name).is_some_and(|x| x != (si, ti)) {
        return Err(EngineError::Other("The name entered already exists. Enter a unique name.".into()));
    }
    edit(s, |cx| {
        let old = cx.wb.sheet(si).and_then(|sh| sh.tables.get(ti)).map(|t| t.name.clone()).unwrap_or_default();
        if let Some(t) = cx.sheet_mut(si)?.tables.get_mut(ti) {
            t.name = name.clone();
        }
        // Structured references follow the rename.
        for i in 0..cx.wb.sheets.len() {
            let keys: Vec<(CellRef, std::sync::Arc<Formula>)> =
                cx.wb.sheets.get(i).map(|s| s.cells.iter().filter_map(|(c, x)| x.formula.clone().map(|f| (c, f))).collect()).unwrap_or_default();
            let Some(sh) = cx.wb.sheet_mut(i) else { continue };
            for (c, f) in keys {
                let Some(e) = f.expr() else { continue };
                if !table_references(&e).contains(&old.to_ascii_lowercase()) {
                    continue;
                }
                let ne = e.map(&mut |x| match x {
                    kition_sheet_formula::Expr::Struct(mut st) if st.table.eq_ignore_ascii_case(&old) => {
                        st.table = name.clone();
                        kition_sheet_formula::Expr::Struct(st)
                    }
                    kition_sheet_formula::Expr::Name(n) if n.eq_ignore_ascii_case(&old) => kition_sheet_formula::Expr::Name(name.clone()),
                    o => o,
                });
                if let Some(cell) = sh.cells.get_mut(c) {
                    cell.formula = Some(std::sync::Arc::new(Formula::from_expr(ne)));
                }
            }
        }
        cx.structural = true;
        Ok(Json::Null)
    })
}

fn table_convert(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ti) = find_table(s, p)?;
    edit(s, |cx| {
        // Apply the table look as plain formatting, then drop the table.
        let wb = &mut cx.wb;
        let Some(t) = wb.sheet(si).and_then(|sh| sh.tables.get(ti)).cloned() else { return Ok(Json::Null) };
        let looks: Vec<(CellRef, Style)> = t.range.iter().map(|c| (c, crate::tables::table_cell_style(wb, si, &t, c))).collect();
        for (c, st) in looks {
            let id = wb.styles.intern(st);
            if let Some(sh) = wb.sheet_mut(si) {
                sh.set_style(c, id);
            }
        }
        if let Some(sh) = wb.sheet_mut(si) {
            sh.tables.remove(ti);
        }
        cx.structural = true;
        Ok(Json::Null)
    })
}

// ---------------------------------------------------------------- charts

pub fn chart_kind(t: &str, sub: &str) -> ChartKind {
    match (t, sub) {
        ("column", "stacked") => ChartKind::ColumnStacked,
        ("column", "stacked100") => ChartKind::ColumnStacked100,
        ("bar", "stacked") => ChartKind::BarStacked,
        ("bar", "stacked100") => ChartKind::BarStacked100,
        ("bar", _) => ChartKind::BarClustered,
        ("line", "markers") => ChartKind::LineMarkers,
        ("line", "stacked") => ChartKind::LineStacked,
        ("line", _) => ChartKind::Line,
        ("pie", _) => ChartKind::Pie,
        ("doughnut", _) => ChartKind::Doughnut,
        ("area", "stacked") => ChartKind::AreaStacked,
        ("area", _) => ChartKind::Area,
        ("scatter", "lines") => ChartKind::ScatterLines,
        ("scatter", _) => ChartKind::Scatter,
        ("bubble", _) => ChartKind::Bubble,
        ("radar", _) => ChartKind::Radar,
        ("histogram", _) => ChartKind::Histogram,
        ("waterfall", _) => ChartKind::Waterfall,
        ("funnel", _) => ChartKind::Funnel,
        ("treemap", _) => ChartKind::Treemap,
        ("sunburst", _) => ChartKind::Sunburst,
        ("boxWhisker", _) => ChartKind::BoxWhisker,
        ("stock", _) => ChartKind::Stock,
        ("combo", _) => ChartKind::Combo,
        _ => ChartKind::ColumnClustered,
    }
}

/// Builds series from a block (or blocks side by side, sharing their rows: `A1:A5,C1:D5`), as
/// Excel does: the first row holds the series names when its cells after the first are text, and
/// the first column the categories when it is text, when the corner cell is blank under a row of
/// names, when it holds dates, or (`scatter`) as the X values.
pub fn series_from_range(wb: &Workbook, sh: &Sheet, areas: &[RangeRef], by_rows: bool, scatter: bool) -> Vec<Series> {
    let Some(first) = areas.first() else { return vec![] };
    let q = kition_sheet_formula::quote_sheet(&sh.name);
    let abs = |r: RangeRef| {
        format!(
            "{q}!${}${}:${}${}",
            kition_sheet_core::col_to_letters(r.start.col),
            r.start.row + 1,
            kition_sheet_core::col_to_letters(r.end.col),
            r.end.row + 1
        )
    };
    let abs1 = |c: CellRef| format!("{q}!${}${}", kition_sheet_core::col_to_letters(c.col), c.row + 1);
    // The block's columns, left to right across its areas (several areas: series in columns).
    let cols: Vec<u32> = areas.iter().flat_map(|a| a.start.col..=a.end.col).take(MAX_CHART_CELLS as usize).collect();
    let (r0, r1) = (first.start.row, first.end.row);
    let by_rows = by_rows && areas.len() == 1;
    let (Some(&c0), Some(&c_last)) = (cols.first(), cols.last()) else { return vec![] };
    let is_text = |c: CellRef| sh.value(c).is_text() || sh.value(c).is_empty();
    let header_row =
        r1 > r0 && cols.iter().any(|c| sh.value(CellRef::new(r0, *c)).is_text()) && cols.iter().skip(1).all(|c| is_text(CellRef::new(r0, *c)));
    let data_r0 = r0 + header_row as u32;
    let first_col_dates = (data_r0..=r1).filter(|row| !sh.value(CellRef::new(*row, c0)).is_empty()).all(|row| {
        let c = CellRef::new(row, c0);
        matches!(sh.value(c), kition_sheet_core::Value::Number(_))
            && kition_sheet_numfmt::NumberFormat::parse(wb.styles.get(sh.style_id(c)).num_fmt.as_str()).is_date()
    }) && (data_r0..=r1).any(|row| !sh.value(CellRef::new(row, c0)).is_empty());
    let corner_blank = header_row && sh.value(CellRef::new(r0, c0)).is_empty();
    let label_col = cols.len() > 1 && (scatter || corner_blank || first_col_dates || (data_r0..=r1).all(|row| is_text(CellRef::new(row, c0))));
    let data_cols: Vec<u32> = cols.iter().skip(label_col as usize).copied().collect();
    let series = |name: Option<String>, categories: Option<String>, values: String| Series {
        name,
        categories,
        values,
        bubble_sizes: None,
        color: None,
        secondary: false,
        kind: None,
        smooth: false,
        trendline: None,
        error_bars: None,
        line_width: None,
        marker: None,
    };
    if !by_rows {
        let categories = label_col.then(|| abs(RangeRef::new(CellRef::new(data_r0, c0), CellRef::new(r1, c0))));
        data_cols
            .iter()
            .map(|c| {
                series(
                    header_row.then(|| abs1(CellRef::new(r0, *c))),
                    categories.clone(),
                    abs(RangeRef::new(CellRef::new(data_r0, *c), CellRef::new(r1, *c))),
                )
            })
            .collect()
    } else {
        let Some(&dc0) = data_cols.first() else { return vec![] };
        (data_r0..=r1)
            .map(|row| {
                series(
                    label_col.then(|| abs1(CellRef::new(row, c0))),
                    header_row.then(|| abs(RangeRef::new(CellRef::new(r0, dc0), CellRef::new(r0, c_last)))),
                    abs(RangeRef::new(CellRef::new(row, dc0), CellRef::new(row, c_last))),
                )
            })
            .collect()
    }
}

/// Most cells a chart's source block may span (bounds the series it builds).
const MAX_CHART_CELLS: u64 = 1_000_000;

/// The cells a chart is made of: the range given (several areas side by side, sharing their
/// rows, as `A1:A5,C1:D5`), else the selection, else the current region.
fn chart_areas(s: &Session, p: &Json) -> Result<Vec<RangeRef>> {
    if p.get("range").is_none() {
        return Ok(vec![block(s, p)?]);
    }
    let areas = target_ranges(s, p)?;
    let Some(first) = areas.first() else { return Err(bad("insert.chart", "no range")) };
    if areas.iter().any(|a| (a.start.row, a.end.row) != (first.start.row, first.end.row)) {
        return Err(bad("insert.chart", "the areas of a chart's range share their rows, side by side (A1:A5,C1:D5)"));
    }
    Ok(areas)
}

/// Series types a combination chart takes (`seriesTypes`), as `drawing.chart` names them.
fn combo_series_kind(name: &str) -> Result<ChartKind> {
    let shared = kition_office_core::chart::ChartKind::parse(name)
        .filter(|k| !k.is_pie() && *k != kition_office_core::chart::ChartKind::Radar)
        .and_then(|k| super::chart_spec::type_of(&k))
        .ok_or_else(|| {
            bad("insert.chart", format!("a combination's series are column, stackedColumn, bar, line, lineMarkers, area or scatter, not `{name}`"))
        })?;
    Ok(shared)
}

fn insert_chart(s: &mut Session, p: &Json) -> Result<Json> {
    let areas = chart_areas(s, p)?;
    let Some(&r) = areas.first() else { return Err(bad("insert.chart", "no range")) };
    let cells: u64 = areas.iter().map(RangeRef::count).sum();
    if cells > MAX_CHART_CELLS {
        return Err(bad("insert.chart", "range too large for a chart"));
    }
    let sheet = target_sheet(s, p)?;
    let d = s.doc()?;
    let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    let mut t = str_param(p, "type").unwrap_or("column").to_string();
    // Excel plots series by the longer dimension (more rows → series in columns).
    let data_h = r.height();
    let data_w: u32 = areas.iter().map(RangeRef::width).sum();
    let by_rows = data_w > data_h + 1 && areas.len() == 1;
    if t == "auto" {
        t = if data_h > 12 && data_w <= 3 { "line".into() } else { "column".into() };
    }
    let kind = chart_kind(&t, str_param(p, "subtype").unwrap_or(""));
    let scatter = matches!(kind, ChartKind::Scatter | ChartKind::ScatterLines | ChartKind::Bubble);
    let mut series = series_from_range(&d.wb, sh, &areas, by_rows && !scatter, scatter);
    if series.is_empty() {
        return Err(EngineError::Other("Select data for the chart first.".into()));
    }
    // A combination: each series' type as given, else Excel's Clustered Column - Line (the first
    // series columns, the others lines); `secondary` puts series on the second value axis.
    let types: Option<Vec<String>> = match p.get("seriesTypes") {
        None | Some(Json::Null) => None,
        Some(Json::Array(list)) => Some(
            list.iter()
                .map(|v| v.as_str().map(str::to_string).ok_or_else(|| bad("insert.chart", "`seriesTypes` lists type names")))
                .collect::<Result<_>>()?,
        ),
        Some(other) => return Err(bad("insert.chart", format!("`seriesTypes` lists a type per series, got {other}"))),
    };
    let secondary: Vec<u64> = match p.get("secondary") {
        None | Some(Json::Null) => vec![],
        Some(Json::Array(list)) => {
            list.iter().map(|v| v.as_u64().ok_or_else(|| bad("insert.chart", "`secondary` lists series indexes from 0"))).collect::<Result<_>>()?
        }
        Some(other) => return Err(bad("insert.chart", format!("`secondary` lists series indexes from 0, got {other}"))),
    };
    if (types.is_some() || !secondary.is_empty()) && kind != ChartKind::Combo {
        return Err(bad("insert.chart", "`seriesTypes` and `secondary` are for type: combo"));
    }
    if kind == ChartKind::Combo {
        for (i, se) in series.iter_mut().enumerate() {
            se.kind = Some(match types.as_ref().and_then(|t| t.get(i)) {
                Some(name) => combo_series_kind(name)?,
                None if i == 0 => ChartKind::ColumnClustered,
                None => ChartKind::Line,
            });
            se.secondary = secondary.contains(&(i as u64));
        }
    }
    let totals = totals_param("insert.chart", p, kind)?;
    let id = next_object_id(&d.wb)?;
    let at = cell_param(p, "at").unwrap_or_else(|| CellRef::new(r.start.row, r.end.col + 2));
    let title = str_param(p, "title").map(str::to_string).or_else(|| {
        if series.len() == 1 {
            series[0].name.as_ref().map(|n| kition_sheet_calc::evaluate(&d.wb, sheet, at, n).display())
        } else {
            Some("Chart Title".into())
        }
    });
    let chart = Chart {
        id,
        kind,
        anchor: Anchor {
            cell: at,
            dx: 0.0,
            dy: 0.0,
            width: f64_param(p, "width").unwrap_or(480.0) as f32,
            height: f64_param(p, "height").unwrap_or(288.0) as f32,
            mode: anchor_mode_param(p).unwrap_or_default(),
            one_cell: false,
        },
        title,
        series,
        legend: if matches!(kind, ChartKind::Pie | ChartKind::Doughnut) { LegendPos::Right } else { LegendPos::Bottom },
        data_labels: false,
        gridlines: !matches!(kind, ChartKind::Pie | ChartKind::Doughnut | ChartKind::Treemap | ChartKind::Sunburst | ChartKind::Funnel),
        style: 1,
        x_title: None,
        y_title: None,
        source: Some(areas.iter().map(|a| format!("{}!{}", kition_sheet_formula::quote_sheet(&sh.name), a.a1())).collect::<Vec<_>>().join(",")),
        by_rows,
        x_axis: Default::default(),
        y_axis: Default::default(),
        label_position: None,
        totals: totals.unwrap_or_default(),
        point_colors: vec![],
    };
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.charts.push(chart);
        Ok(json!({"chart": id}))
    })
}

/// A waterfall's `totals`: the points set as totals, by index from 0.
fn totals_param(command: &str, p: &Json, kind: ChartKind) -> Result<Option<Vec<u32>>> {
    let list = match p.get("totals") {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Array(list)) => list,
        Some(other) => return Err(bad(command, format!("`totals` lists the points set as totals, by index from 0, got {other}"))),
    };
    if kind != ChartKind::Waterfall {
        return Err(bad(command, "`totals` is for a waterfall chart"));
    }
    let mut out: Vec<u32> = list
        .iter()
        .map(|v| v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| bad(command, format!("`totals`: not a point index: {v}"))))
        .collect::<Result<_>>()?;
    out.sort_unstable();
    out.dedup();
    Ok(Some(out))
}

fn find_chart(s: &Session, p: &Json) -> Result<(usize, usize)> {
    let d = s.doc()?;
    let sheet = d.wb.active_sheet;
    let sh = d.wb.sheet(sheet).ok_or(EngineError::NoDocument)?;
    let idx = match u32_param(p, "chart") {
        Some(id) => sh.charts.iter().position(|c| c.id == id),
        None => sh.charts.len().checked_sub(1),
    };
    idx.map(|i| (sheet, i)).ok_or_else(|| bad("chart", "no such chart"))
}

fn chart_set(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ci) = find_chart(s, p)?;
    edit(s, |cx| {
        let sh = cx.sheet_mut(si)?;
        if sh.is_protected() {
            return Err(EngineError::Other(PROTECTED.into()));
        }
        let Some(c) = sh.charts.get_mut(ci) else { return Ok(Json::Null) };
        if let Some(t) = str_param(p, "type") {
            c.kind = chart_kind(t, str_param(p, "subtype").unwrap_or(""));
        }
        if let Some(totals) = totals_param("chart.set", p, c.kind)? {
            c.totals = totals;
        }
        if let Some(t) = p.get("title") {
            c.title = t.as_str().map(str::to_string);
        }
        if let Some(l) = str_param(p, "legend") {
            c.legend = match l {
                "none" => LegendPos::None,
                "top" => LegendPos::Top,
                "left" => LegendPos::Left,
                "right" => LegendPos::Right,
                _ => LegendPos::Bottom,
            };
        }
        if let Some(v) = bool_param(p, "dataLabels") {
            c.data_labels = v;
        }
        if let Some(v) = bool_param(p, "gridlines") {
            c.gridlines = v;
        }
        if let Some(v) = u32_param(p, "style") {
            c.style = v.clamp(1, 16);
        }
        if let Some(t) = p.get("xTitle") {
            c.x_title = t.as_str().map(str::to_string);
        }
        if let Some(t) = p.get("yTitle") {
            c.y_title = t.as_str().map(str::to_string);
        }
        if let Some(at) = cell_param(p, "at") {
            c.anchor.cell = at;
        }
        if let Some(w) = f64_param(p, "width") {
            c.anchor.width = w.clamp(20.0, 10000.0) as f32;
        }
        if let Some(h) = f64_param(p, "height") {
            c.anchor.height = h.clamp(20.0, 10000.0) as f32;
        }
        if let Some(v) = f64_param(p, "dx") {
            c.anchor.dx = v as f32;
        }
        if let Some(v) = f64_param(p, "dy") {
            c.anchor.dy = v as f32;
        }
        Ok(Json::Null)
    })
}

fn chart_switch(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ci) = find_chart(s, p)?;
    edit(s, |cx| {
        let Some(sh) = cx.wb.sheet(si) else { return Ok(Json::Null) };
        if sh.is_protected() {
            return Err(EngineError::Other(PROTECTED.into()));
        }
        let Some(c) = sh.charts.get(ci) else { return Ok(Json::Null) };
        // Charts read from a file have no source range: take the block their series span.
        let Some((src, by_rows)) = c.source.clone().map(|s| (s, c.by_rows)).or_else(|| series_block(&c.series)) else { return Ok(Json::Null) };
        let (sheet_name, body) = split_sheet(&src);
        let src_sheet = sheet_name.and_then(|n| cx.wb.sheet_index(&n)).unwrap_or(si);
        if body.contains(',') {
            return Err(bad("chart.switchRowColumn", "a chart of several areas keeps its series in columns"));
        }
        let Some(mut r) = RangeRef::parse(body) else { return Ok(Json::Null) };
        let Some(ssh) = cx.wb.sheet(src_sheet) else { return Ok(Json::Null) };
        // Whole-column/row references (common in files) span a million cells: clip the block's
        // far edges to the cells in use, then refuse anything still too large to chart.
        if let Some(used) = ssh.used_range() {
            r.end.row = r.end.row.min(used.end.row).max(r.start.row);
            r.end.col = r.end.col.min(used.end.col).max(r.start.col);
        }
        if r.count() > MAX_CHART_CELLS {
            return Err(bad("chart.switchRowColumn", "range too large for a chart"));
        }
        let by_rows = !by_rows;
        let series = series_from_range(&cx.wb, ssh, &[r], by_rows, false);
        if let Some(c) = cx.sheet_mut(si)?.charts.get_mut(ci) {
            c.source.get_or_insert(src);
            c.by_rows = by_rows;
            c.series = series;
        }
        Ok(Json::Null)
    })
}

/// The block a chart's series come from, and whether each series is a row of it: the bounding
/// range of the series names, categories and values, when they're all references to one sheet.
pub fn series_block(series: &[Series]) -> Option<(String, bool)> {
    let mut sheet: Option<String> = None;
    let mut bounds: Option<RangeRef> = None;
    let mut rows = 0;
    for s in series {
        for (i, f) in [Some(&s.values), s.categories.as_ref(), s.name.as_ref()].into_iter().flatten().enumerate() {
            let Ok(kition_sheet_formula::Expr::Ref(r)) = kition_sheet_formula::parse(f) else { return None };
            let kition_sheet_formula::SheetSel::Named(n) = &r.sheet else { return None };
            if !sheet.get_or_insert_with(|| n.clone()).eq_ignore_ascii_case(n) {
                return None;
            }
            let range = r.range();
            if i == 0 && range.height() == 1 && range.width() > 1 {
                rows += 1;
            }
            bounds = Some(bounds.map_or(range, |b| b.union(&range)));
        }
    }
    Some((format!("{}!{}", kition_sheet_formula::quote_sheet(&sheet?), bounds?.a1()), rows > 0 && rows == series.len()))
}

fn chart_delete(s: &mut Session, p: &Json) -> Result<Json> {
    let (si, ci) = find_chart(s, p)?;
    edit(s, |cx| {
        cx.sheet_mut(si)?.charts.remove(ci);
        Ok(Json::Null)
    })
}

fn insert_sparkline(s: &mut Session, p: &Json) -> Result<Json> {
    let src = str_param(p, "range").ok_or_else(|| bad("insert.sparkline", "missing `range`"))?.to_string();
    let loc = cell_param(p, "location").ok_or_else(|| bad("insert.sparkline", "missing `location`"))?;
    let kind = match str_param(p, "type") {
        Some("column") => SparklineKind::Column,
        Some("winLoss") => SparklineKind::WinLoss,
        _ => SparklineKind::Line,
    };
    let markers = bool_param(p, "markers").unwrap_or(false);
    let sheet = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        sh.sparklines.retain(|x| x.cell != loc);
        sh.sparklines.push(Sparkline { cell: loc, source: src.clone(), kind, color: Color::Theme(4, 0), markers });
        Ok(Json::Null)
    })
}

struct PictureInput {
    data: Vec<u8>,
    cell_mime: Option<&'static str>,
}

fn picture_input(s: &mut Session, p: &Json) -> Result<Option<PictureInput>> {
    let placement = str_param(p, "placement").unwrap_or("overCells");
    if !matches!(placement, "cell" | "overCells") {
        return Err(bad("insert.picture", "placement must be cell or overCells"));
    }
    let data = if let Some(b) = str_param(p, "base64") {
        crate::io::base64_decode(b).ok_or_else(|| bad("insert.picture", "invalid base64"))?
    } else if let Some(path) = str_param(p, "path") {
        s.read_path(path)?
    } else {
        s.ui_requests.push(crate::UiRequest::Dialog("insertPicture".into(), json!({"placement": placement})));
        return Ok(None);
    };
    let cell_mime = if placement == "cell" {
        Some(validate_cell_picture(&data)?)
    } else {
        if data.len() > 64 * 1024 * 1024 {
            return Err(bad("insert.picture", "image larger than 64 MB"));
        }
        None
    };
    Ok(Some(PictureInput { data, cell_mime }))
}

fn insert_picture(s: &mut Session, p: &Json) -> Result<Json> {
    let Some(input) = picture_input(s, p)? else { return ok() };
    let d = s.doc()?;
    let at = if input.cell_mime.is_some() {
        match str_param(p, "at") {
            Some(a) => CellRef::parse(a).ok_or_else(|| bad("insert.picture", "invalid cell address"))?,
            None => d.selection.active,
        }
    } else {
        cell_param(p, "at").unwrap_or(d.selection.active)
    };
    insert_picture_input(s, p, input, at)
}

/// A wrapper with a fixed origin never takes the picture command's selection default.
pub(super) fn insert_picture_at(s: &mut Session, p: &Json, at: CellRef) -> Result<Json> {
    let Some(input) = picture_input(s, p)? else { return ok() };
    insert_picture_input(s, p, input, at)
}

fn insert_picture_input(s: &mut Session, p: &Json, input: PictureInput, at: CellRef) -> Result<Json> {
    let PictureInput { data, cell_mime } = input;
    if let Some(mime) = cell_mime {
        let d = s.doc()?;
        let sheet = d.wb.active_sheet;
        let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
        let at = sh.merge_at(at).map(|r| r.start).unwrap_or(at);
        let style = sh.style_id(at);
        if sh.is_protected() && d.wb.styles.get(style).protection.locked {
            return Err(EngineError::Other("The cell you're trying to change is on a protected sheet.".into()));
        }
        let picture = CellPicture { data, mime: mime.into(), alt: str_param(p, "alt").unwrap_or("").into() };
        return edit(s, |cx| {
            cx.sheet_mut(sheet)?.set_picture(at, std::sync::Arc::new(picture));
            cx.touch(sheet, at);
            Ok(json!({"cell": at.a1(), "placement": "cell"}))
        });
    }
    let mime = str_param(p, "mime")
        .map(str::to_string)
        .unwrap_or_else(|| if data.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png".into() } else { "image/jpeg".into() });
    let (w, h) = crate::io::image_size(&data).unwrap_or((320, 240));
    let d = s.doc()?;
    let id = next_object_id(&d.wb)?;
    let scale = (640.0 / w.max(1) as f32).min(1.0);
    let img = Image {
        id,
        anchor: Anchor {
            cell: at,
            dx: 0.0,
            dy: 0.0,
            width: f64_param(p, "width").map(|v| v as f32).unwrap_or(w as f32 * scale),
            height: f64_param(p, "height").map(|v| v as f32).unwrap_or(h as f32 * scale),
            // Excel inserts pictures as "Move but don't size with cells" (`editAs="oneCell"`).
            mode: anchor_mode_param(p).unwrap_or(kition_sheet_model::AnchorMode::MoveOnly),
            one_cell: false,
        },
        data,
        mime,
        alt: str_param(p, "alt").unwrap_or("").into(),
        blip_ext: None,
    };
    let sheet = d.wb.active_sheet;
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.images.push(img);
        Ok(json!({"image": id}))
    })
}

/// Validate before mutating the workbook; header dimensions alone do not prove the pixels decode.
fn validate_cell_picture(data: &[u8]) -> Result<&'static str> {
    if data.len() > 16 * 1024 * 1024 {
        return Err(bad("insert.picture", "cell pictures must be at most 16 MiB"));
    }
    let format = image::guess_format(data).map_err(|_| bad("insert.picture", "choose a valid PNG or JPEG picture"))?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        _ => return Err(bad("insert.picture", "cell pictures support PNG and JPEG")),
    };
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(data), format)
        .into_dimensions()
        .map_err(|_| bad("insert.picture", "invalid image dimensions"))?;
    if width == 0 || height == 0 || width > 8192 || height > 8192 || u64::from(width) * u64::from(height) > 16_000_000 {
        return Err(bad("insert.picture", "cell pictures are limited to 8192 pixels per side and 16 million pixels"));
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(data), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| bad("insert.picture", "picture cannot be decoded within the 64 MiB limit"))?;
    Ok(mime)
}

fn insert_shape(s: &mut Session, p: &Json) -> Result<Json> {
    let kind = match str_param(p, "kind").unwrap_or("rectangle") {
        "roundedRectangle" => ShapeKind::RoundedRectangle,
        "ellipse" | "oval" => ShapeKind::Ellipse,
        "triangle" => ShapeKind::Triangle,
        "line" => ShapeKind::Line,
        "arrow" => ShapeKind::Arrow,
        "textBox" => ShapeKind::TextBox,
        _ => ShapeKind::Rectangle,
    };
    let d = s.doc()?;
    let id = next_object_id(&d.wb)?;
    let at = cell_param(p, "at").unwrap_or(d.selection.active);
    let fill =
        super::format::color_param(p.get("fill")).unwrap_or(if kind == ShapeKind::TextBox { Color::rgb(255, 255, 255) } else { Color::Theme(4, 0) });
    let line = super::format::color_param(p.get("line")).unwrap_or(if kind == ShapeKind::TextBox {
        Color::rgb(0x80, 0x80, 0x80)
    } else {
        Color::Theme(4, -250)
    });
    let shape = Shape {
        id,
        kind,
        anchor: Anchor {
            cell: at,
            dx: 0.0,
            dy: 0.0,
            width: f64_param(p, "width").unwrap_or(144.0) as f32,
            height: f64_param(p, "height").unwrap_or(if kind == ShapeKind::Line { 0.0 } else { 96.0 }) as f32,
            mode: anchor_mode_param(p).unwrap_or_default(),
            one_cell: false,
        },
        fill,
        line,
        text: str_param(p, "text").unwrap_or("").into(),
        // A shape's text is white, as in a shape Excel inserts.
        style_refs: (!matches!(kind, ShapeKind::TextBox | ShapeKind::Line)).then(|| kition_sheet_xlsx::DEFAULT_SHAPE_STYLE.to_string()),
        ..Default::default()
    };
    let sheet = d.wb.active_sheet;
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.shapes.push(shape);
        Ok(json!({"shape": id}))
    })
}

/// A text box holds at most as many characters as a cell.
const SHAPE_TEXT_MAX_CHARS: usize = 32_767;

fn shape_set_text(s: &mut Session, p: &Json) -> Result<Json> {
    let id =
        p.get("id").and_then(Json::as_u64).and_then(|id| u32::try_from(id).ok()).ok_or_else(|| bad("shape.setText", "`id` must be a text box id"))?;
    let text = str_param(p, "text").ok_or_else(|| bad("shape.setText", "`text` must be a string"))?;
    if text.chars().nth(SHAPE_TEXT_MAX_CHARS).is_some() {
        return Err(bad("shape.setText", "`text` is longer than 32,767 characters"));
    }
    let d = s.doc()?;
    let sheet = d.wb.active_sheet;
    let sh = d.wb.active().ok_or_else(|| bad("shape.setText", "no such text box"))?;
    let shape = sh.shapes.iter().find(|shape| shape.id == id).ok_or_else(|| bad("shape.setText", "no such text box"))?;
    if shape.kind != ShapeKind::TextBox {
        return Err(bad("shape.setText", "the shape is not a text box"));
    }
    // Sheet protection locks drawing objects (there is no "edit objects" allowance yet).
    if sh.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    if shape.text == text {
        return ok();
    }
    edit(s, |cx| {
        let shape = cx.sheet_mut(sheet)?.shapes.iter_mut().find(|shape| shape.id == id).ok_or_else(|| bad("shape.setText", "no such text box"))?;
        shape.text = text.into();
        Ok(Json::Null)
    })
}

fn delete_object(s: &mut Session, p: &Json) -> Result<Json> {
    let id = u32_param(p, "id").ok_or_else(|| bad("object.delete", "missing `id`"))?;
    let kind = str_param(p, "kind").unwrap_or("").to_string();
    let sheet = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        match kind.as_str() {
            "chart" => sh.charts.retain(|c| c.id != id),
            "image" => sh.images.retain(|c| c.id != id),
            _ => sh.shapes.retain(|c| c.id != id),
        }
        Ok(Json::Null)
    })
}

fn move_object(s: &mut Session, p: &Json) -> Result<Json> {
    let id = u32_param(p, "id").ok_or_else(|| bad("object.move", "missing `id`"))?;
    let kind = str_param(p, "kind").unwrap_or("").to_string();
    let sheet = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        let anchor = match kind.as_str() {
            "chart" => sh.charts.iter_mut().find(|c| c.id == id).map(|c| &mut c.anchor),
            "image" => sh.images.iter_mut().find(|c| c.id == id).map(|c| &mut c.anchor),
            "shape" => sh.shapes.iter_mut().find(|c| c.id == id).map(|c| &mut c.anchor),
            _ => None,
        };
        let Some(a) = anchor else { return Err(bad("object.move", "no such object")) };
        if let Some(at) = cell_param(p, "at") {
            a.cell = at;
        }
        if let Some(v) = f64_param(p, "dx") {
            a.dx = v as f32;
        }
        if let Some(v) = f64_param(p, "dy") {
            a.dy = v as f32;
        }
        if let Some(v) = f64_param(p, "width") {
            a.width = v.clamp(1.0, 10000.0) as f32;
        }
        if let Some(v) = f64_param(p, "height") {
            a.height = v.clamp(0.0, 10000.0) as f32;
        }
        Ok(Json::Null)
    })
}

/// Sets an object's anchor mode (Excel's "Move and size with cells" / "Move but don't size with
/// cells" / "Don't move or size with cells"), so it follows — or ignores — its rows and columns
/// when the sheet is sorted or edited structurally.
fn set_anchor_mode(s: &mut Session, p: &Json) -> Result<Json> {
    let id = u32_param(p, "id").ok_or_else(|| bad("object.setAnchorMode", "missing `id`"))?;
    let kind = str_param(p, "kind").unwrap_or("").to_string();
    if !matches!(kind.as_str(), "chart" | "image" | "shape") {
        return Err(bad("object.setAnchorMode", "kind must be chart, image or shape"));
    }
    let mode = anchor_mode_param(p).ok_or_else(|| bad("object.setAnchorMode", "mode must be moveAndSize, moveOnly or absolute"))?;
    let sheet = s.doc()?.wb.active_sheet;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        let anchor = match kind.as_str() {
            "chart" => sh.charts.iter_mut().find(|c| c.id == id).map(|c| &mut c.anchor),
            "image" => sh.images.iter_mut().find(|c| c.id == id).map(|c| &mut c.anchor),
            _ => sh.shapes.iter_mut().find(|c| c.id == id).map(|c| &mut c.anchor),
        };
        let Some(a) = anchor else { return Err(bad("object.setAnchorMode", "no such object")) };
        a.mode = mode;
        Ok(json!({"mode": anchor_mode_name(mode)}))
    })
}

// ---------------------------------------------------------------- links & comments

fn insert_link(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = cell_param(p, "cell").unwrap_or(s.doc()?.selection.active);
    if bool_param(p, "remove").unwrap_or(false) {
        return edit(s, |cx| {
            cx.sheet_mut(sheet)?.hyperlinks.remove(&at);
            Ok(Json::Null)
        });
    }
    // A checkbox cell holds TRUE or FALSE: no link (WPS greys Insert Link there).
    if crate::cmd::extra::is_checkbox(&s.doc()?.wb, sheet, at) {
        return Err(EngineError::Other("A checkbox cell can't hold a link.".into()));
    }
    let Some(target) = str_param(p, "target").map(str::to_string) else {
        s.ui_requests.push(crate::UiRequest::Dialog("insertLink".into(), json!({})));
        return ok();
    };
    let text = str_param(p, "text").map(str::to_string);
    let tooltip = str_param(p, "tooltip").map(str::to_string);
    edit(s, |cx| {
        let link_style = cx.wb.styles.derive(cx.wb.sheet(sheet).map(|sh| sh.style_id(at)).unwrap_or_default(), |st| {
            st.font.color = Color::Theme(10, 0);
            st.font.underline = Underline::Single;
        });
        let sh = cx.sheet_mut(sheet)?;
        if let Some(t) = text.clone().or_else(|| sh.value(at).is_empty().then(|| target.clone())) {
            sh.set_cell(at, Cell { value: kition_sheet_core::Value::text(t.as_str()), formula: None, style: link_style, rich: None });
        } else {
            sh.set_style(at, link_style);
        }
        sh.hyperlinks.insert(at, Hyperlink::new(target.clone(), tooltip.clone()));
        cx.touch(sheet, at);
        Ok(Json::Null)
    })
}

fn comment(s: &mut Session, p: &Json, threaded: bool) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = cell_param(p, "cell").unwrap_or(s.doc()?.selection.active);
    let text = str_param(p, "text").unwrap_or("").to_string();
    let author = str_param(p, "author").map(str::to_string).unwrap_or_else(|| s.prefs.user_name.clone());
    edit(s, |cx| {
        cx.sheet_mut(sheet)?
            .comments
            .insert(at, Comment { author: author.clone(), text: text.clone(), replies: vec![], threaded, resolved: false, visible: false });
        Ok(json!({"cell": at.a1()}))
    })
}

fn reply_comment(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = cell_param(p, "cell").unwrap_or(s.doc()?.selection.active);
    let text = str_param(p, "text").unwrap_or("").to_string();
    let author = str_param(p, "author").map(str::to_string).unwrap_or_else(|| s.prefs.user_name.clone());
    edit(s, |cx| {
        let c = cx.sheet_mut(sheet)?.comments.get_mut(&at).ok_or_else(|| EngineError::Other("There's no comment in this cell.".into()))?;
        c.replies.push((author.clone(), text.clone()));
        Ok(Json::Null)
    })
}

fn resolve_comment(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = cell_param(p, "cell").unwrap_or(s.doc()?.selection.active);
    let v = bool_param(p, "resolved").unwrap_or(true);
    edit(s, |cx| {
        if let Some(c) = cx.sheet_mut(sheet)?.comments.get_mut(&at) {
            c.resolved = v;
        }
        Ok(Json::Null)
    })
}

fn delete_comment(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let ranges = target_ranges(s, p)?;
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.comments.retain(|c, _| !ranges.iter().any(|r| r.contains(*c)));
        Ok(Json::Null)
    })
}

fn show_note(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let at = cell_param(p, "cell").unwrap_or(s.doc()?.selection.active);
    edit(s, |cx| {
        if let Some(c) = cx.sheet_mut(sheet)?.comments.get_mut(&at) {
            c.visible = !c.visible;
        }
        Ok(Json::Null)
    })
}

fn insert_symbol(s: &mut Session, p: &Json) -> Result<Json> {
    let ch = str_param(p, "char").unwrap_or("").to_string();
    let d = s.doc()?;
    let at = d.selection.active;
    let cur = d.wb.active().and_then(|sh| sh.cell(at)).map(|c| c.input_text()).unwrap_or_default();
    s.execute("cell.set", json!({"cell": at.a1(), "input": format!("{cur}{ch}")}))
}

// ---------------------------------------------------------------- conditional formatting

fn preset_style(name: &str) -> Style {
    let mut s = Style::default();
    match name {
        "redText" => s.font.color = Color::rgb(0x9C, 0x00, 0x06),
        "yellowFill" | "yellowFillDarkYellowText" => {
            s.fill = Fill::solid(Color::rgb(0xFF, 0xEB, 0x9C));
            s.font.color = Color::rgb(0x9C, 0x57, 0x00);
        }
        "greenFill" | "greenFillDarkGreenText" => {
            s.fill = Fill::solid(Color::rgb(0xC6, 0xEF, 0xCE));
            s.font.color = Color::rgb(0x00, 0x61, 0x00);
        }
        "lightRedFill" => s.fill = Fill::solid(Color::rgb(0xFF, 0xC7, 0xCE)),
        "redBorder" => {
            let l = BorderLine { style: BorderStyle::Thin, color: Color::rgb(0xC0, 0x00, 0x00) };
            s.borders = Borders { left: l, right: l, top: l, bottom: l, ..Default::default() };
        }
        _ => {
            s.fill = Fill::solid(Color::rgb(0xFF, 0xC7, 0xCE));
            s.font.color = Color::rgb(0x9C, 0x00, 0x06);
        }
    }
    s
}

/// The keys each rule type reads, besides `type` (and `style`/`preset` where it takes a format).
fn cf_rule_keys(kind: &str) -> Option<(&'static [&'static str], bool)> {
    Some(match kind {
        "cellIs" => (&["operator", "value", "value2"], true),
        "expression" => (&["formula"], true),
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => (&["text"], true),
        "blanks" | "noBlanks" | "errors" | "noErrors" | "duplicate" | "unique" => (&[], true),
        "top10" => (&["rank", "bottom", "percent"], true),
        "aboveAverage" => (&["below"], true),
        "timePeriod" => (&["period"], true),
        "colorScale" => (&["colors"], false),
        "dataBar" => (&["min", "max", "color", "gradient", "showValue"], false),
        "iconSet" => (&["set", "thresholds", "reverse", "showValue"], false),
        _ => return None,
    })
}

/// Icon sets as files name them, with their icon count.
const ICON_SETS: &[&str] = &[
    "3Arrows",
    "3ArrowsGray",
    "3Flags",
    "3TrafficLights1",
    "3TrafficLights2",
    "3Signs",
    "3Symbols",
    "3Symbols2",
    "3Stars",
    "3Triangles",
    "4Arrows",
    "4ArrowsGray",
    "4RedToBlack",
    "4Rating",
    "4TrafficLights",
    "5Arrows",
    "5ArrowsGray",
    "5Rating",
    "5Quarters",
    "5Boxes",
];

fn add_cf(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "home.conditionalFormat";
    let ranges = target_ranges(s, p)?;
    let sheet = target_sheet(s, p)?;
    let Some(rule) = p.get("rule") else {
        s.ui_requests.push(crate::UiRequest::Dialog("newFormattingRule".into(), json!({})));
        return ok();
    };
    let Some(given) = rule.as_object() else {
        return Err(bad(CMD, "`rule` is an object such as {type: \"cellIs\", operator: \"lessThan\", value: 0}"));
    };
    let g = |k: &str| rule.get(k).and_then(Json::as_str).map(str::to_string);
    let kind = g("type").unwrap_or_else(|| "cellIs".into());
    let Some((keys, formatted)) = cf_rule_keys(&kind) else {
        return Err(bad(
            CMD,
            format!(
                "unknown rule type `{kind}`: use cellIs, expression, containsText, notContainsText, beginsWith, endsWith, blanks, noBlanks, errors, noErrors, duplicate, unique, top10, aboveAverage, timePeriod, colorScale, dataBar or iconSet"
            ),
        ));
    };
    // A key the rule does not read is refused, not ignored (engine rule E7a).
    for key in given.keys() {
        let known = key == "type" || keys.contains(&key.as_str()) || (formatted && (key == "style" || key == "preset"));
        if !known {
            let hint = match key.as_str() {
                "values" | "value1" | "formula1" | "formula2" if kind == "cellIs" => " (cellIs takes `value` and, for between, `value2`)",
                "format" | "fill" | "font" if formatted => {
                    " (the format is `style`, for example {fill: \"#FFC7CE\", font: {color: \"#9C0006\"}}, or a `preset`)"
                }
                "style" | "preset" => " (this rule type draws its own colours)",
                _ => "",
            };
            let mut takes: Vec<&str> = keys.to_vec();
            if formatted {
                takes.extend(["style", "preset"]);
            }
            return Err(bad(CMD, format!("a {kind} rule does not read `{key}`{hint}; it takes type, {}", takes.join(", "))));
        }
    }
    let style = Box::new(match rule.get("style") {
        Some(st) if st.is_object() => {
            let st = super::format::style_patch(CMD, st)?;
            let base = serde_json::to_value(Style::default()).map_err(|e| EngineError::Other(e.to_string()))?;
            serde_json::from_value(super::format::merge_json(base, &st)).map_err(|e| bad(CMD, e.to_string()))?
        }
        Some(_) => return Err(bad(CMD, "`style` is an object such as {fill: \"#FFC7CE\", font: {color: \"#9C0006\", bold: true}}")),
        None => preset_style(g("preset").as_deref().unwrap_or("lightRedFillDarkRedText")),
    });
    let value_kind = |v: &Json, default_type: &str| -> Result<CfValueKind> {
        let (t, val) = match v {
            Json::Object(o) => (o.get("type").and_then(Json::as_str).unwrap_or("number"), o.get("value").unwrap_or(&Json::Null)),
            other => (default_type, other),
        };
        let number = || val.as_f64().ok_or_else(|| bad(CMD, format!("a {t} threshold needs a number `value`, got {val}")));
        Ok(match t {
            "min" | "lowest" => CfValueKind::Min,
            "max" | "highest" => CfValueKind::Max,
            "percent" => CfValueKind::Percent(number()?),
            "percentile" => CfValueKind::Percentile(number()?),
            "formula" => CfValueKind::Formula(val.as_str().unwrap_or("").trim_start_matches('=').into()),
            "number" | "num" => CfValueKind::Number(number()?),
            other => return Err(bad(CMD, format!("unknown threshold type `{other}`: use number, percent, percentile, formula, min or max"))),
        })
    };
    let rule = match kind.as_str() {
        "cellIs" => {
            let op = super::data::operator_param(CMD, g("operator").as_deref(), CfOperator::Greater)?;
            let a = rule.get("value").map(super::edit::json_to_input).ok_or_else(|| bad(CMD, "a cellIs rule needs `value`"))?;
            let b = rule.get("value2").map(super::edit::json_to_input);
            if matches!(op, CfOperator::Between | CfOperator::NotBetween) && b.is_none() {
                return Err(bad(CMD, "between and notBetween need `value2` as well as `value`"));
            }
            let quote = |v: String| {
                if kition_sheet_core::parse::parse_number_text(&v).is_some() || v.starts_with('=') {
                    v.trim_start_matches('=').to_string()
                } else {
                    format!("\"{}\"", v.replace('"', "\"\""))
                }
            };
            CfRule::CellIs { op, a: quote(a), b: b.map(quote), style }
        }
        "expression" => CfRule::Expression { formula: g("formula").unwrap_or_default().trim_start_matches('=').to_string(), style },
        "containsText" => CfRule::ContainsText { text: g("text").unwrap_or_default(), style },
        "notContainsText" => CfRule::NotContainsText { text: g("text").unwrap_or_default(), style },
        "beginsWith" => CfRule::BeginsWith { text: g("text").unwrap_or_default(), style },
        "endsWith" => CfRule::EndsWith { text: g("text").unwrap_or_default(), style },
        "blanks" => CfRule::Blanks { style },
        "noBlanks" => CfRule::NoBlanks { style },
        "errors" => CfRule::Errors { style },
        "noErrors" => CfRule::NoErrors { style },
        "duplicate" => CfRule::Duplicate { style },
        "unique" => CfRule::Unique { style },
        "top10" => CfRule::Top10 {
            bottom: rule.get("bottom").and_then(Json::as_bool).unwrap_or(false),
            percent: rule.get("percent").and_then(Json::as_bool).unwrap_or(false),
            rank: rule.get("rank").and_then(Json::as_u64).unwrap_or(10) as u32,
            style,
        },
        "aboveAverage" => CfRule::AboveAverage { below: rule.get("below").and_then(Json::as_bool).unwrap_or(false), equal: false, std_dev: 0, style },
        "timePeriod" => CfRule::TimePeriod { period: g("period").unwrap_or_else(|| "today".into()), style },
        "colorScale" => {
            let cols: Vec<Color> = match rule.get("colors") {
                Some(Json::Array(a)) => a
                    .iter()
                    .map(|c| super::format::color_param(Some(c)).ok_or_else(|| bad(CMD, format!("not a colour: {c}"))))
                    .collect::<Result<_>>()?,
                Some(other) => return Err(bad(CMD, format!("`colors` is a list of two or three colours, got {other}"))),
                None => vec![Color::rgb(0xF8, 0x69, 0x6B), Color::rgb(0xFF, 0xEB, 0x84), Color::rgb(0x63, 0xBE, 0x7B)],
            };
            let stops = match cols.as_slice() {
                [low, high] => vec![(CfValueKind::Min, *low), (CfValueKind::Max, *high)],
                [low, mid, high] => vec![(CfValueKind::Min, *low), (CfValueKind::Percentile(50.0), *mid), (CfValueKind::Max, *high)],
                _ => return Err(bad(CMD, "`colors` is a list of two or three colours (lowest to highest)")),
            };
            CfRule::ColorScale { stops }
        }
        "dataBar" => CfRule::DataBar {
            min: rule.get("min").map(|v| value_kind(v, "number")).transpose()?.unwrap_or(CfValueKind::Min),
            max: rule.get("max").map(|v| value_kind(v, "number")).transpose()?.unwrap_or(CfValueKind::Max),
            color: super::format::color_param(rule.get("color")).unwrap_or(Color::rgb(0x63, 0x8E, 0xC6)),
            gradient: rule.get("gradient").and_then(Json::as_bool).unwrap_or(true),
            show_value: rule.get("showValue").and_then(Json::as_bool).unwrap_or(true),
        },
        "iconSet" => {
            let set = g("set").unwrap_or_else(|| "3TrafficLights1".into());
            let Some(set) = ICON_SETS.iter().find(|s| s.eq_ignore_ascii_case(&set)) else {
                return Err(bad(CMD, format!("unknown icon set `{set}`: use {}", ICON_SETS.join(", "))));
            };
            let count = crate::cf::icon_count(set);
            // Files store one threshold per icon, the first (the lowest icon's) being the floor;
            // a rule takes the boundaries between icons, ascending, by default percent of the
            // span from the lowest value to the highest, as Excel's are (0, 33, 67 for three).
            let cuts: Vec<CfValueKind> = match rule.get("thresholds") {
                Some(Json::Array(list)) if list.len() + 1 == count => list.iter().map(|v| value_kind(v, "percent")).collect::<Result<_>>()?,
                Some(other) => {
                    return Err(bad(
                        CMD,
                        format!("{set} has {count} icons, so `thresholds` lists the {} boundaries between them, ascending: got {other}", count - 1),
                    ));
                }
                None => (1..count).map(|i| CfValueKind::Percent((100.0 * i as f64 / count as f64).round())).collect(),
            };
            let mut thresholds = vec![CfValueKind::Percent(0.0)];
            thresholds.extend(cuts);
            CfRule::IconSet {
                set: (*set).to_string(),
                thresholds,
                reverse: rule.get("reverse").and_then(Json::as_bool).unwrap_or(false),
                show_value: rule.get("showValue").and_then(Json::as_bool).unwrap_or(true),
            }
        }
        other => return Err(bad(CMD, format!("unknown rule type `{other}`"))),
    };
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        // New rules take precedence over older ones.
        for cf in sh.cond_formats.iter_mut() {
            cf.priority = cf.priority.checked_add(1).ok_or_else(|| bad(CMD, "conditional formatting priority space is exhausted"))?;
        }
        sh.cond_formats.insert(0, CondFormat { ranges: ranges.clone(), rule, priority: 1, stop_if_true: false });
        Ok(json!({"rules": sh.cond_formats.len()}))
    })
}

fn clear_cf(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    let whole = bool_param(p, "sheet").unwrap_or(false);
    let ranges = target_ranges(s, p)?;
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        if whole {
            sh.cond_formats.clear();
        } else {
            for cf in sh.cond_formats.iter_mut() {
                cf.ranges.retain(|r| !ranges.iter().any(|x| x.intersects(r)));
            }
            sh.cond_formats.retain(|c| !c.ranges.is_empty());
        }
        Ok(Json::Null)
    })
}

fn manage_cf(s: &mut Session, p: &Json) -> Result<Json> {
    let sheet = target_sheet(s, p)?;
    if p.get("delete").is_none() && p.get("moveUp").is_none() && p.get("moveDown").is_none() {
        let list: Vec<Json> = s
            .doc()?
            .wb
            .sheet(sheet)
            .map(|sh| {
                sh.cond_formats.iter().map(|c| json!({"ranges": c.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>(), "rule": c.rule})).collect()
            })
            .unwrap_or_default();
        return Ok(json!({"rules": list}));
    }
    edit(s, |cx| {
        let sh = cx.sheet_mut(sheet)?;
        if let Some(i) = u32_param(p, "delete").map(|v| v as usize)
            && i < sh.cond_formats.len()
        {
            sh.cond_formats.remove(i);
        }
        let n = sh.cond_formats.len();
        if let Some(i) = u32_param(p, "moveUp").map(|v| v as usize)
            && i > 0
            && i < n
        {
            sh.cond_formats.swap(i, i - 1);
        }
        if let Some(i) = u32_param(p, "moveDown").map(|v| v as usize)
            && i + 1 < n
        {
            sh.cond_formats.swap(i, i + 1);
        }
        for (k, cf) in sh.cond_formats.iter_mut().enumerate() {
            cf.priority = u32::try_from(k)
                .ok()
                .and_then(|priority| priority.checked_add(1))
                .ok_or_else(|| bad("home.manageRules", "conditional formatting priority space is exhausted"))?;
        }
        Ok(Json::Null)
    })
}

fn insert_icon(s: &mut Session, p: &Json) -> Result<Json> {
    let name = str_param(p, "name").unwrap_or("check").to_string();
    let size = f64_param(p, "size").unwrap_or(48.0).clamp(8.0, 1000.0) as f32;
    let d = s.doc()?;
    let id = next_object_id(&d.wb)?;
    let at = cell_param(p, "at").unwrap_or(d.selection.active);
    let color = super::format::color_param(p.get("color")).unwrap_or(Color::Theme(1, 0));
    let shape = Shape {
        id,
        kind: ShapeKind::Icon,
        anchor: Anchor { cell: at, dx: 4.0, dy: 4.0, width: size, height: size, mode: anchor_mode_param(p).unwrap_or_default(), one_cell: false },
        fill: color,
        line: Color::Auto,
        text: name,
        ..Default::default()
    };
    let sheet = d.wb.active_sheet;
    edit(s, |cx| {
        cx.sheet_mut(sheet)?.shapes.push(shape);
        Ok(json!({"shape": id}))
    })
}

#[cfg(test)]
mod independent_read_tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn combined_conditional_format_delete_and_move_survives() {
        for movement in [json!({"moveUp":1}), json!({"moveDown":0})] {
            let mut session = Session::new();
            session.new_workbook();
            for range in ["A1", "B1"] {
                session.execute("home.conditionalFormat", json!({"range":range,"rule":{"type":"blanks"}})).unwrap();
            }
            let mut params = movement;
            params["delete"] = json!(0);
            let answer = session.execute("home.manageRules", params);
            assert!(answer.is_ok(), "combined deletion and movement must not panic: {answer:?}");
            let rules = session.execute("home.manageRules", json!({})).unwrap();
            assert_eq!(rules["rules"].as_array().map(Vec::len), Some(1));
        }
    }
}
