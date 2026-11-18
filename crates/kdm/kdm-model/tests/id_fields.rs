//! Every position, local id or formula stored as data must be written in a form that survives a
//! structural edit on another replica (an object id, a line token, id-form formula text) and
//! translated back (`kdm_model`'s `codec`, `features`, `write` and `read`). Kition Sheet's
//! positions are integers (`CellRef`, `RangeRef`, line indexes), its style ids `StyleId`s and its
//! formulas strings, so this lists every model field typed with an integer, a position, a style id
//! or a string: a new one fails here until it is classified (handled by the bridge, or neither a
//! position, an id nor a formula).

const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../../../engine/model/src/lib.rs")),
    ("workbook.rs", include_str!("../../../engine/model/src/workbook.rs")),
    ("sheet.rs", include_str!("../../../engine/model/src/sheet.rs")),
    ("cell.rs", include_str!("../../../engine/model/src/cell.rs")),
    ("rich.rs", include_str!("../../../engine/model/src/rich.rs")),
    ("style.rs", include_str!("../../../engine/model/src/style.rs")),
    ("features.rs", include_str!("../../../engine/model/src/features.rs")),
    ("store.rs", include_str!("../../../engine/model/src/store.rs")),
    ("structure.rs", include_str!("../../../engine/model/src/structure.rs")),
];

/// `file: field` for every such field of the model, and how the bridge handles it.
const HANDLED: &[&str] = &[
    // Positions: line tokens (`codec`), keys `<row>:<col>` of the cells, comments and hyperlinks
    // maps, `Row`/`Col` objects for line info.
    "cell.rs: array: Option<RangeRef>",
    "features.rs: anchor: CellRef",
    "features.rs: cell: CellRef",
    "features.rs: col_breaks: Vec<u32>",
    "features.rs: row_breaks: Vec<u32>",
    "features.rs: last_range: Option<RangeRef>",
    "features.rs: print_area: Option<RangeRef>",
    "features.rs: range: RangeRef",
    "features.rs: ranges: Vec<RangeRef>",
    "features.rs: title_cols: Option<(u32, u32)>",
    "features.rs: title_rows: Option<(u32, u32)>",
    "sheet.rs: cell_pictures: BTreeMap<CellRef, Arc<CellPicture>>",
    "sheet.rs: comments: BTreeMap<CellRef, Comment>",
    "sheet.rs: hyperlinks: BTreeMap<CellRef, Hyperlink>",
    "sheet.rs: cols: BTreeMap<u32, LineInfo>",
    "sheet.rs: rows: BTreeMap<u32, LineInfo>",
    "sheet.rs: merges: Vec<RangeRef>",
    // Frozen panes are a count of rows and columns from the top left, kept by count.
    "sheet.rs: freeze: Option<(u32, u32)>",
    // Style ids: the style's key in the `styles` map (its content hash).
    "cell.rs: style: StyleId",
    "features.rs: style: Option<crate::style::StyleId>",
    // Formulas: id form (`codec::encode_formula`, `encode_text`): a cell's formula, a defined
    // name's, a conditional format's (`CellIs` a/b, `Expression`, `CfValueKind::Formula`), a
    // table column's calculated formula and custom totals label, a chart series' name, categories,
    // values and bubble sizes and the chart's source, a sparkline's and a pivot table's source.
    // (A validation's `f1`/`f2` too; their names hold digits, so this scan does not see them.)
    "cell.rs: text: String",
    "workbook.rs: formula: String",
    "features.rs: a: String",
    "features.rs: b: Option<String>",
    "features.rs: formula: String",
    "features.rs: formula: Option<String>",
    "features.rs: totals_label: Option<String>",
    "features.rs: values: String",
    "features.rs: name: Option<String>",
    "features.rs: categories: Option<String>",
    "features.rs: bubble_sizes: Option<String>",
    "features.rs: source: Option<String>",
    "features.rs: source: String",
    // A defined name's scope: the `Name` object's `scope` ref to a `Sheet`.
    "workbook.rs: scope: Option<usize>",
    // A sheet's own id (`sheetId`, its tool id `sheet:<id>`): stored as the `Sheet` object's
    // read-only `sheetId`, a number, not a position; a repeated one is numbered afresh on read.
    "sheet.rs: sheet_id: u32",
    // Local object ids (charts, images, shapes, tables; pivot tables): stored as data, repeated
    // ones (made concurrently) given fresh ids on read (`read::dedupe_local_ids`).
    "features.rs: id: u32",
    // A sheet protection's password hash: beyond 2^53, so stored as a string.
    "features.rs: password_hash: Option<u64>",
    // Per person (G4), never written: the active sheet, a sheet's zoom, active cell and scroll.
    "workbook.rs: active_sheet: usize",
    "sheet.rs: zoom: u16",
    "sheet.rs: view_active: CellRef",
    "sheet.rs: view_top_left: CellRef",
    // The workbook window's first tab shown (`firstSheet`): a scroll hint of the tab bar, kept
    // as the file gave it and clamped to the sheets and the active one when written.
    "workbook.rs: first_sheet: Option<u32>",
    // Derived (calculation), never written.
    "sheet.rs: spill: BTreeMap<CellRef, Value>",
    "sheet.rs: spill_ranges: BTreeMap<CellRef, RangeRef>",
    "sheet.rs: circular: BTreeSet<CellRef>",
    // The structural log's sheet index: not stored (read by the bridge after each command).
    "structure.rs: sheet: usize",
    // Not positions, ids or formulas: counts, sizes, settings, offsets within a range, bytes,
    // names and labels (table and pivot field names are referenced by name, which needs no
    // translation), text, caches of the engine.
    // A sheet's gridline colour (0xRRGGBB, a setting of the sheet's view).
    "sheet.rs: grid_color: Option<u32>",
    "cell.rs: Cell { value: v, formula: None, style: StyleId::DEFAULT, rich: None }",
    "cell.rs: alt: String",
    // Rich text runs (marks on the cell's text, see `marks`): characters, a face name, a colour as
    // the file names it (hex, theme slot, palette index), font family, character set and scheme.
    "rich.rs: charset: Option<u32>",
    "rich.rs: family: Option<u32>",
    "rich.rs: indexed: Option<u32>",
    "rich.rs: name: Option<String>",
    "rich.rs: rgb: Option<String>",
    "rich.rs: scheme: Option<String>",
    "rich.rs: text: String",
    "rich.rs: theme: Option<u32>",
    "cell.rs: data: Vec<u8>",
    "cell.rs: mime: String",
    "features.rs: a: (String, String)",
    "features.rs: alt: String",
    "features.rs: anchor: CellRef::default()",
    "features.rs: author: String",
    "features.rs: b: Option<(String, String)>",
    "features.rs: collapsed_items: Vec<String>",
    "features.rs: count: u32",
    // A waterfall's total points: indexes into its series, not sheet positions.
    "features.rs: totals: Vec<u32>",
    "features.rs: criteria: Vec<(u32, FilterCriterion)>",
    "features.rs: data: Vec<u8>",
    "features.rs: error_message: String",
    "features.rs: error_title: String",
    "features.rs: fit_to: Option<(u16, u16)>",
    "features.rs: footer: String",
    "features.rs: header: String",
    "features.rs: input_message: String",
    "features.rs: input_title: String",
    "features.rs: mime: String",
    "features.rs: name: String",
    "features.rs: number_format: Option<String>",
    "features.rs: outline: u8",
    "features.rs: paper: String",
    "features.rs: period: String",
    "features.rs: priority: u32",
    "features.rs: rank: u32",
    "features.rs: replies: Vec<(String, String)>",
    "features.rs: scale: u16",
    "features.rs: selected: Option<Vec<String>>",
    "features.rs: set: String",
    "features.rs: source_col: String",
    "features.rs: std_dev: u8",
    "features.rs: style: String",
    "features.rs: style: u32",
    "features.rs: target: String",
    "features.rs: text: String",
    "features.rs: title: Option<String>",
    "features.rs: tooltip: Option<String>",
    "features.rs: values: Vec<String>",
    "features.rs: x_title: Option<String>",
    "features.rs: y_title: Option<String>",
    "sheet.rs: lines: Vec<(u32, f64, f64)>",
    "sheet.rs: max: u32",
    "sheet.rs: name: String",
    "sheet.rs: view_active: CellRef::default()",
    "sheet.rs: view_top_left: CellRef::default()",
    "store.rs: bands: BTreeMap<u32, Arc<Band>>",
    "store.rs: len: usize",
    "store.rs: rows: BTreeMap<u32, BTreeMap<u32, Cell>>",
    "style.rs: colors: [u32; 12]",
    "style.rs: indent: u8",
    "style.rs: index: HashMap<Style, StyleId>",
    "style.rs: charset: Option<u32>",
    "style.rs: major_font: String",
    "style.rs: minor_font: String",
    "style.rs: name: String",
    "style.rs: raw: Option<String>",
    "style.rs: rotation: i16",
    "style.rs: scheme: Option<String>",
    "workbook.rs: author: String",
    "workbook.rs: category: String",
    // Kept as the file wrote them (the canonical fidelity round): a link's text and namespace
    // attributes of other applications, a picture's and the workbook's extension markup, the
    // outline levels a sheet reserves, the file's table styles (names, revision id, element
    // kinds and stripe sizes; their formats are styles, not ids).
    "features.rs: blip_ext: Option<String>",
    "features.rs: display: Option<String>",
    "features.rs: ns: String",
    "features.rs: value: String",
    "sheet.rs: outline_levels: Option<(u8, u8)>",
    "workbook.rs: default_pivot: Option<String>",
    "workbook.rs: default_table: Option<String>",
    "workbook.rs: kind: String",
    "workbook.rs: size: Option<u32>",
    "workbook.rs: styles: Vec<String>",
    "workbook.rs: uid: Option<String>",
    "workbook.rs: workbook: Vec<String>",
    "workbook.rs: cell_styles: Vec<(String, crate::style::Style)>",
    "workbook.rs: comment: String",
    "workbook.rs: company: String",
    "workbook.rs: created: String",
    "workbook.rs: custom_lists: Vec<Vec<String>>",
    "workbook.rs: custom_xml: Option<String>",
    "workbook.rs: description: String",
    "workbook.rs: indexed: Vec<u32>",
    "workbook.rs: keywords: String",
    "workbook.rs: last_modified_by: String",
    "workbook.rs: max_iterations: u32",
    "workbook.rs: modified: String",
    "workbook.rs: name: String",
    "workbook.rs: subject: String",
    "workbook.rs: title: String",
    // The workbook window (place, size, tab bar share, visibility) and the versions that wrote
    // the file, as the file gave them.
    // Which named cell styles are Excel's built-in ones (by name), and their outline level.
    "workbook.rs: cell_style_info: BTreeMap<String, CellStyleInfo>",
    "workbook.rs: builtin: Option<u32>",
    "workbook.rs: level: Option<u32>",
    "workbook.rs: x: Option<i32>",
    "workbook.rs: y: Option<i32>",
    "workbook.rs: width: Option<u32>",
    "workbook.rs: height: Option<u32>",
    "workbook.rs: tab_ratio: Option<u32>",
    "workbook.rs: visibility: Option<String>",
    "workbook.rs: app_name: String",
    "workbook.rs: last_edited: String",
    "workbook.rs: lowest_edited: String",
    "workbook.rs: rup_build: String",
    "workbook.rs: code_name: String",
    // Form controls kept from the file (module M9): their VML shape, control element and
    // properties part verbatim, and their place as the file gave it. The bridge does not carry
    // them (a replica has none); the session that opened the file writes them back on save.
    "features.rs: vml: String",
    "features.rs: shape_type: String",
    "features.rs: control: Option<String>",
    "features.rs: props: Option<String>",
    "features.rs: shape_id: u32",
    "features.rs: from: (CellRef, f32, f32)",
    "features.rs: to: (CellRef, f32, f32)",
    // Smart graphics kept from the file: their parts verbatim, by role (or, for the drawing's
    // pictures, the relationship id the drawing names them by), with their relationship and
    // content types and file extensions. Like form controls, the session that opened the file
    // writes them back on save.
    "features.rs: role: String",
    "features.rs: rel_type: String",
    "features.rs: content_type: String",
    "features.rs: ext: String",
    // A drawing shape's groups: keys its members share, object ids (from reading a file or from
    // `drawing.group`) stored as values like the shape's own id, never a position. A connector's
    // ends name shapes by those same ids, and their connection sites by index.
    "features.rs: groups: Vec<u32>",
    "features.rs: site: u32",
    // A chart series' trendline and error bars: their types by name, a polynomial's order and
    // a moving average's period, values rather than positions.
    "features.rs: kind: String",
    "features.rs: order: Option<u32>",
    "features.rs: period: Option<u32>",
    // A shape's geometry, turn, opacity, gradient and pen as the file gave them (DrawingML
    // units), its custom paths, and its text body and style references kept as written.
    "features.rs: adj: Vec<(String, String)>",
    "features.rs: d: String",
    "features.rs: rot: i32",
    "features.rs: fill_alpha: Option<u32>",
    "features.rs: alpha: Option<u32>",
    "features.rs: angle: i32",
    "features.rs: pos: u32",
    "features.rs: path: Option<String>",
    "features.rs: width: Option<u32>",
    "features.rs: dash: Option<String>",
    "features.rs: cap: Option<String>",
    "features.rs: join: Option<String>",
    "features.rs: miter_limit: Option<u32>",
    "features.rs: head: Option<String>",
    "features.rs: tail: Option<String>",
    "features.rs: text_body: Option<String>",
    "features.rs: style_refs: Option<String>",
    "features.rs: marker: Option<String>",
    "features.rs: label_position: Option<String>",
];

const TYPES: &[&str] = &["u8", "u16", "u32", "u64", "usize", "i16", "i32", "i64", "CellRef", "RangeRef", "StyleId", "String"];

fn names_a_kind(line: &str) -> bool {
    line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').any(|t| TYPES.contains(&t))
}

#[test]
fn every_position_id_and_formula_field_is_handled() {
    let mut found = Vec::new();
    for (file, source) in SOURCES {
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with("//")
                || line.starts_with('#')
                || line.contains("fn ")
                || line.contains("let ")
                || line.contains("=>")
                || line.contains(" = ")
                || line.contains("::new()")
                || line.contains("Some(")
            {
                continue;
            }
            let line = line.strip_prefix("pub ").unwrap_or(line).trim_end_matches(',');
            let Some((name, _)) = line.split_once(": ") else { continue };
            let field_like = name.chars().all(|c| c.is_ascii_lowercase() || c == '_') || name.contains(" { ");
            if field_like && names_a_kind(line) {
                found.push(format!("{file}: {line}"));
            }
        }
    }
    found.sort();
    found.dedup();
    let unhandled: Vec<_> = found.iter().filter(|f| !HANDLED.contains(&f.as_str())).collect();
    assert!(unhandled.is_empty(), "model fields holding positions, ids, integers or strings that kdm-model does not classify: {unhandled:#?}");
    let stale: Vec<_> = HANDLED.iter().filter(|h| !found.iter().any(|f| f == *h)).collect();
    assert!(stale.is_empty(), "listed as handled but no longer in the model (update this list): {stale:#?}");
}
