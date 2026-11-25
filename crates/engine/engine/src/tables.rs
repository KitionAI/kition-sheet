//! Table styles: how a cell inside a table looks. The built-in styles are Kition Sheet's own
//! designs, drawn in code from the workbook's theme colours and keyed by the familiar
//! `TableStyleLight1…21 / Medium1…28 / Dark1…11` names so files round-trip; a style the workbook
//! defines itself (`tableStyles`, Home › Format as Table › New Table Style) is drawn from its
//! elements. The gallery offers them as WPS does: a theme colour (Text 1 or Accent 1–6) and, in it,
//! the light, medium and dark designs.

use std::collections::HashMap;
use std::sync::OnceLock;

use kition_sheet_core::{CellRef, RangeRef};
use kition_sheet_model::*;
use serde_json::{Value as Json, json};

/// The style a new table gets when the workbook names no default.
pub const DEFAULT_STYLE: &str = "TableStyleMedium2";
/// The gallery's theme colours: Text 1 and Accent 1–6.
pub const COLOR_SLOTS: u32 = 7;
/// The parts of a table a style formats, from the one under the others to the one on top: a
/// cell's look is each part that covers it, in this order (a header beats the first column, the
/// first column beats banded rows, banded rows beat banded columns).
pub const ELEMENT_ORDER: [&str; 13] = [
    "wholeTable",
    "firstColumnStripe",
    "secondColumnStripe",
    "firstRowStripe",
    "secondRowStripe",
    "lastColumn",
    "firstColumn",
    "headerRow",
    "totalRow",
    "firstHeaderCell",
    "lastHeaderCell",
    "firstTotalCell",
    "lastTotalCell",
];

/// (family, number) from a style name, e.g. `TableStyleMedium2` → ("Medium", 2).
pub fn parse_style_name(name: &str) -> (&'static str, u32) {
    let rest = name.strip_prefix("TableStyle").unwrap_or(name);
    for fam in ["Light", "Medium", "Dark"] {
        if let Some(n) = rest.strip_prefix(fam) {
            let k = n.parse::<u32>().unwrap_or(2).max(1); // the gallery starts at 1; 0 would underflow `n - 1`
            return (fam, k);
        }
    }
    ("Medium", 2)
}

/// The theme index a gallery colour slot draws with: 0 is Text 1, 1–6 are Accent 1–6.
pub fn slot_theme(slot: u32) -> u8 {
    if slot == 0 { 1 } else { 3 + slot.min(6) as u8 }
}

/// A shade of colour slot `slot`: `tint` in thousandths, lighter when positive, darker when
/// negative. Text 1 has no hue of its own: its colour is a dark grey and its tints are greys.
fn shade(slot: u32, tint: i16) -> Color {
    if slot == 0 { Color::Theme(1, if tint >= 0 { tint.max(350) } else { 150 }) } else { Color::Theme(slot_theme(slot), tint) }
}

const WHITE: Color = Color::Theme(0, 0);
const DARK: Color = Color::Theme(1, 0);

fn thin(c: Color) -> BorderLine {
    BorderLine { style: BorderStyle::Thin, color: c }
}

fn outline(s: &mut Style, line: BorderLine) {
    s.borders.top = line;
    s.borders.bottom = line;
    s.borders.left = line;
    s.borders.right = line;
}

fn grid(s: &mut Style, line: BorderLine) {
    outline(s, line);
    s.borders.horizontal = line;
    s.borders.vertical = line;
}

fn fill(s: &mut Style, c: Color) {
    s.fill = Fill::solid(c);
}

fn emphasis(s: &mut Style, c: Color) {
    s.font.bold = true;
    s.font.color = c;
}

/// Collects a style's elements.
struct Design(Vec<TableStyleElement>);

impl Design {
    fn add(&mut self, kind: &str, f: impl FnOnce(&mut Style)) -> &mut Self {
        let mut s = Style::default();
        f(&mut s);
        self.0.push(TableStyleElement { kind: kind.to_string(), size: None, style: s });
        self
    }
}

/// The elements of built-in style `fam` `n` (our own designs). Light: rules, frame, grid. Medium:
/// banded, tiles, deep header, soft grid. Dark: dark body; Dark8–11 pair two colours.
fn design(fam: &str, n: u32) -> Vec<TableStyleElement> {
    let mut d = Design(Vec::new());
    let (pattern, k) = ((n - 1) / 7, (n - 1) % 7);
    match (fam, pattern) {
        ("Light", 0) => {
            d.add("wholeTable", |s| {
                s.borders.top = thin(shade(k, 0));
                s.borders.bottom = thin(shade(k, 0));
            })
            .add("headerRow", |s| {
                emphasis(s, shade(k, -250));
                s.borders.bottom = thin(shade(k, 0));
            })
            .add("totalRow", |s| {
                s.font.bold = true;
                s.borders.top = BorderLine { style: BorderStyle::Double, color: shade(k, 0) };
            })
            .add("firstColumn", |s| s.font.bold = true)
            .add("lastColumn", |s| s.font.bold = true)
            .add("firstRowStripe", |s| fill(s, shade(k, 850)))
            .add("firstColumnStripe", |s| fill(s, shade(k, 850)));
        }
        ("Light", 1) => {
            d.add("wholeTable", |s| outline(s, thin(shade(k, 0))))
                .add("headerRow", |s| {
                    fill(s, shade(k, 0));
                    emphasis(s, WHITE);
                })
                .add("totalRow", |s| {
                    s.font.bold = true;
                    s.borders.top = BorderLine { style: BorderStyle::Double, color: shade(k, 0) };
                })
                .add("firstColumn", |s| s.font.bold = true)
                .add("lastColumn", |s| s.font.bold = true)
                .add("firstRowStripe", |s| {
                    s.borders.top = thin(shade(k, 0));
                    s.borders.bottom = thin(shade(k, 0));
                })
                .add("firstColumnStripe", |s| {
                    s.borders.left = thin(shade(k, 0));
                    s.borders.right = thin(shade(k, 0));
                });
        }
        ("Light", _) => {
            d.add("wholeTable", |s| grid(s, thin(shade(k, 400))))
                .add("headerRow", |s| {
                    s.font.bold = true;
                    s.borders.bottom = BorderLine { style: BorderStyle::Medium, color: shade(k, 0) };
                })
                .add("totalRow", |s| {
                    s.font.bold = true;
                    s.borders.top = BorderLine { style: BorderStyle::Double, color: shade(k, 0) };
                })
                .add("firstColumn", |s| s.font.bold = true)
                .add("lastColumn", |s| s.font.bold = true)
                .add("firstRowStripe", |s| fill(s, shade(k, 800)))
                .add("firstColumnStripe", |s| fill(s, shade(k, 800)));
        }
        ("Medium", 0) => {
            d.add("wholeTable", |s| {
                outline(s, thin(shade(k, 400)));
                s.borders.horizontal = thin(shade(k, 400));
            })
            .add("headerRow", |s| {
                fill(s, shade(k, 0));
                emphasis(s, WHITE);
            })
            .add("totalRow", |s| {
                s.font.bold = true;
                s.borders.top = BorderLine { style: BorderStyle::Double, color: shade(k, 0) };
            })
            .add("firstColumn", |s| s.font.bold = true)
            .add("lastColumn", |s| s.font.bold = true)
            .add("firstRowStripe", |s| fill(s, shade(k, 800)))
            .add("firstColumnStripe", |s| fill(s, shade(k, 800)));
        }
        ("Medium", 1) => {
            d.add("wholeTable", |s| {
                fill(s, shade(k, 800));
                grid(s, thin(WHITE));
            })
            .add("headerRow", |s| {
                fill(s, shade(k, 0));
                emphasis(s, WHITE);
            })
            .add("totalRow", |s| {
                fill(s, shade(k, 0));
                emphasis(s, WHITE);
            })
            .add("firstColumn", |s| {
                fill(s, shade(k, 0));
                emphasis(s, WHITE);
            })
            .add("lastColumn", |s| {
                fill(s, shade(k, 0));
                emphasis(s, WHITE);
            })
            .add("firstRowStripe", |s| fill(s, shade(k, 600)))
            .add("firstColumnStripe", |s| fill(s, shade(k, 600)));
        }
        ("Medium", 2) => {
            d.add("wholeTable", |s| {
                s.borders.top = thin(shade(k, -250));
                s.borders.bottom = thin(shade(k, -250));
            })
            .add("headerRow", |s| {
                fill(s, shade(k, -250));
                emphasis(s, WHITE);
            })
            .add("totalRow", |s| {
                fill(s, shade(k, 800));
                s.font.bold = true;
                s.borders.top = BorderLine { style: BorderStyle::Double, color: shade(k, -250) };
            })
            .add("firstColumn", |s| {
                fill(s, shade(k, 800));
                s.font.bold = true;
            })
            .add("lastColumn", |s| {
                fill(s, shade(k, 800));
                s.font.bold = true;
            })
            .add("firstRowStripe", |s| fill(s, shade(k, 850)))
            .add("firstColumnStripe", |s| fill(s, shade(k, 850)));
        }
        ("Medium", _) => {
            d.add("wholeTable", |s| {
                fill(s, shade(k, 850));
                grid(s, thin(shade(k, 400)));
            })
            .add("headerRow", |s| {
                fill(s, shade(k, 600));
                emphasis(s, DARK);
            })
            .add("totalRow", |s| {
                fill(s, shade(k, 600));
                s.font.bold = true;
                s.borders.top = BorderLine { style: BorderStyle::Double, color: shade(k, 0) };
            })
            .add("firstColumn", |s| s.font.bold = true)
            .add("lastColumn", |s| s.font.bold = true)
            .add("firstRowStripe", |s| fill(s, shade(k, 700)))
            .add("firstColumnStripe", |s| fill(s, shade(k, 700)));
        }
        ("Dark", 0) => {
            d.add("wholeTable", |s| {
                fill(s, shade(k, 0));
                s.font.color = WHITE;
            })
            .add("headerRow", |s| {
                fill(s, DARK);
                emphasis(s, WHITE);
                s.borders.bottom = BorderLine { style: BorderStyle::Medium, color: WHITE };
            })
            .add("totalRow", |s| {
                fill(s, DARK);
                emphasis(s, WHITE);
                s.borders.top = BorderLine { style: BorderStyle::Double, color: WHITE };
            })
            .add("firstColumn", |s| {
                fill(s, shade(k, -250));
                emphasis(s, WHITE);
            })
            .add("lastColumn", |s| {
                fill(s, shade(k, -250));
                emphasis(s, WHITE);
            })
            .add("firstRowStripe", |s| fill(s, shade(k, -250)))
            .add("firstColumnStripe", |s| fill(s, shade(k, -250)));
        }
        _ => {
            // Dark8–11: a light body in one colour under a header in another.
            let (a, b) = match n {
                9 => (1, 2),
                10 => (3, 4),
                11 => (5, 6),
                _ => (0, 0),
            };
            d.add("wholeTable", |s| fill(s, shade(a, 850)))
                .add("headerRow", |s| {
                    fill(s, shade(b, 0));
                    emphasis(s, WHITE);
                })
                .add("totalRow", |s| {
                    fill(s, shade(a, 850));
                    s.font.bold = true;
                    s.borders.top = BorderLine { style: BorderStyle::Double, color: shade(b, 0) };
                })
                .add("firstColumn", |s| {
                    fill(s, shade(a, 600));
                    s.font.bold = true;
                })
                .add("lastColumn", |s| {
                    fill(s, shade(a, 600));
                    s.font.bold = true;
                })
                .add("firstRowStripe", |s| fill(s, shade(a, 600)))
                .add("firstColumnStripe", |s| fill(s, shade(a, 600)));
        }
    }
    d.0
}

/// Every built-in style by name, built once.
fn builtins() -> &'static HashMap<String, TableStyleDef> {
    static ALL: OnceLock<HashMap<String, TableStyleDef>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = HashMap::new();
        for (fam, count) in [("Light", 21), ("Medium", 28), ("Dark", 11)] {
            for n in 1..=count {
                let name = format!("TableStyle{fam}{n}");
                all.insert(name.clone(), TableStyleDef { name, table: None, pivot: None, uid: None, elements: design(fam, n) });
            }
        }
        all
    })
}

/// A built-in style by name (`TableStyleMedium2`); `None` for other names.
pub fn look(name: &str) -> Option<&'static TableStyleDef> {
    builtins().get(name)
}

/// Whether `name` is one of the built-in styles.
pub fn is_builtin(name: &str) -> bool {
    builtins().contains_key(name)
}

/// The styles the workbook defines for tables (New Table Style, or a file's own).
pub fn custom_styles(wb: &Workbook) -> impl Iterator<Item = &TableStyleDef> {
    wb.table_styles.styles.iter().filter(|d| d.table != Some(false) && !is_builtin(&d.name))
}

/// The definition a table style name draws with: the workbook's own style of that name, else the
/// built-in one; no style (`""` or `None`) has none. A name neither knows draws as the default.
pub fn style_def<'a>(wb: &'a Workbook, name: &str) -> Option<&'a TableStyleDef> {
    if name.is_empty() || name.eq_ignore_ascii_case("None") {
        return None;
    }
    custom_styles(wb).find(|d| d.name.eq_ignore_ascii_case(name)).or_else(|| look(name)).or_else(|| look(DEFAULT_STYLE))
}

/// The workbook's default table style: the one it names, else Kition Sheet's.
pub fn default_style(wb: &Workbook) -> String {
    wb.table_styles
        .default_table
        .clone()
        .filter(|n| is_builtin(n) || custom_styles(wb).any(|d| d.name.eq_ignore_ascii_case(n)))
        .unwrap_or_else(|| DEFAULT_STYLE.to_string())
}

/// A table's shape, as its style options give it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub rows: u32,
    pub cols: u32,
    /// Rows formatted as the header (a table has 0 or 1; a range formatted without a table may have more).
    pub header_rows: u32,
    pub totals: bool,
    pub banded_rows: bool,
    pub banded_cols: bool,
    pub first_col: bool,
    pub last_col: bool,
}

impl Layout {
    pub fn of(t: &Table) -> Layout {
        Layout {
            rows: t.range.height(),
            cols: t.range.width(),
            header_rows: u32::from(t.header_row),
            totals: t.totals_row,
            banded_rows: t.banded_rows,
            banded_cols: t.banded_cols,
            first_col: t.first_col,
            last_col: t.last_col,
        }
    }
}

/// What a table style gives one cell, before the cell's own format.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub fill: Option<Fill>,
    pub color: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub top: Option<BorderLine>,
    pub bottom: Option<BorderLine>,
    pub left: Option<BorderLine>,
    pub right: Option<BorderLine>,
}

impl Look {
    /// Lays element format `s` over the look; `edges` says whether the cell is on the top, bottom,
    /// left and right edge of the part (an edge takes the part's outer border, inside it the
    /// part's horizontal or vertical one).
    fn apply(&mut self, s: &Style, [top, bottom, left, right]: [bool; 4]) {
        if s.fill.pattern != PatternType::None {
            self.fill = Some(s.fill);
        }
        if s.font.color != Color::Auto {
            self.color = Some(s.font.color);
        }
        self.bold |= s.font.bold;
        self.italic |= s.font.italic;
        let b = &s.borders;
        for (slot, line) in [
            (&mut self.top, if top { b.top } else { b.horizontal }),
            (&mut self.bottom, if bottom { b.bottom } else { b.horizontal }),
            (&mut self.left, if left { b.left } else { b.vertical }),
            (&mut self.right, if right { b.right } else { b.vertical }),
        ] {
            if !line.is_none() {
                *slot = Some(line);
            }
        }
    }
}

/// The look style `def` gives the cell at row `r`, column `c` of a table shaped `l` (offsets from
/// its top-left); `band_row` and `band_col` are the cell's place among the shown data rows and
/// columns (hidden ones don't count, so the bands stay alternate).
pub fn compose(def: &TableStyleDef, l: &Layout, r: u32, c: u32, band_row: u32, band_col: u32) -> Look {
    let mut look = Look::default();
    let last_r = l.rows.saturating_sub(1);
    let last_c = l.cols.saturating_sub(1);
    let d0 = l.header_rows;
    let d1 = if l.totals { last_r.saturating_sub(1) } else { last_r };
    let data = r >= d0 && r <= d1 && !(l.totals && r == last_r);
    let size = |kind: &str| def.elements.iter().find(|e| e.kind == kind).and_then(|e| e.size).unwrap_or(1).clamp(1, 9);
    // Where a stripe pattern puts index `i`: in the first stripe or the second, the place in it,
    // and its length.
    let stripe = |i: u32, first: &str, second: &str| {
        let (s1, s2) = (size(first), size(second));
        let p = i % (s1 + s2);
        if p < s1 { (true, p, s1) } else { (false, p - s1, s2) }
    };
    for kind in ELEMENT_ORDER {
        let Some(e) = def.elements.iter().find(|e| e.kind == kind) else { continue };
        let edges = match kind {
            "wholeTable" => Some([r == 0, r == last_r, c == 0, c == last_c]),
            "firstRowStripe" | "secondRowStripe" if l.banded_rows && data => {
                let (first, at, len) = stripe(band_row, "firstRowStripe", "secondRowStripe");
                (first == (kind == "firstRowStripe")).then_some([at == 0 || r == d0, at + 1 == len || r == d1, c == 0, c == last_c])
            }
            "firstColumnStripe" | "secondColumnStripe" if l.banded_cols && data => {
                let (first, at, len) = stripe(band_col, "firstColumnStripe", "secondColumnStripe");
                (first == (kind == "firstColumnStripe")).then_some([r == d0, r == d1, at == 0 || c == 0, at + 1 == len || c == last_c])
            }
            "lastColumn" if l.last_col && c == last_c => Some([r == 0, r == last_r, true, true]),
            "firstColumn" if l.first_col && c == 0 => Some([r == 0, r == last_r, true, true]),
            "headerRow" if r < l.header_rows => Some([r == 0, r + 1 == l.header_rows, c == 0, c == last_c]),
            "totalRow" if l.totals && r == last_r && r >= l.header_rows => Some([true, true, c == 0, c == last_c]),
            "firstHeaderCell" if l.first_col && r < l.header_rows && c == 0 => Some([r == 0, r + 1 == l.header_rows, true, true]),
            "lastHeaderCell" if l.last_col && r < l.header_rows && c == last_c => Some([r == 0, r + 1 == l.header_rows, true, true]),
            "firstTotalCell" if l.first_col && l.totals && r == last_r && c == 0 => Some([true; 4]),
            "lastTotalCell" if l.last_col && l.totals && r == last_r && c == last_c => Some([true; 4]),
            _ => None,
        };
        if let Some(edges) = edges {
            look.apply(&e.style, edges);
        }
    }
    look
}

/// A cell's own format with a table style's look under it: the cell's own fill, text colour and
/// borders win; bold and italic add up.
pub fn dress(own: &Style, look: &Look) -> Style {
    let mut s = own.clone();
    if own.fill.pattern == PatternType::None
        && let Some(f) = &look.fill
    {
        s.fill = *f;
    }
    if own.font.color == Color::Auto
        && let Some(c) = look.color
    {
        s.font.color = c;
    }
    s.font.bold |= look.bold;
    s.font.italic |= look.italic;
    for (edge, line) in
        [(&mut s.borders.top, look.top), (&mut s.borders.bottom, look.bottom), (&mut s.borders.left, look.left), (&mut s.borders.right, look.right)]
    {
        if edge.is_none()
            && let Some(l) = line
        {
            *edge = l;
        }
    }
    s
}

/// How many of the rows (columns) from `from` up to `to` (not included) are hidden.
fn hidden_between(lines: &std::collections::BTreeMap<u32, LineInfo>, from: u32, to: u32) -> u32 {
    if to <= from {
        return 0;
    }
    lines.range(from..to).filter(|(_, info)| info.hidden).count() as u32
}

/// The style a table gives a cell (merged over the cell's own style: explicit fills, colours and
/// borders win). Banded rows and columns count only the shown ones, so a filter or hidden rows
/// keep the bands alternate.
pub fn table_cell_style(wb: &Workbook, sheet: usize, t: &Table, c: CellRef) -> Style {
    let own = wb.sheet(sheet).map(|sh| wb.styles.get(sh.style_id(c)).clone()).unwrap_or_default();
    let Some(def) = style_def(wb, &t.style) else { return own };
    let Some(sh) = wb.sheet(sheet) else { return own };
    if !t.range.contains(c) {
        return own;
    }
    let l = Layout::of(t);
    let (r, col) = (c.row - t.range.start.row, c.col - t.range.start.col);
    let d0 = t.range.start.row + u32::from(t.header_row);
    let band_row = c.row.saturating_sub(d0).saturating_sub(hidden_between(&sh.rows, d0, c.row));
    let band_col = col.saturating_sub(hidden_between(&sh.cols, t.range.start.col, c.col));
    dress(&own, &compose(def, &l, r, col, band_row, band_col))
}

/// What a range formatted with style `name` and no table would look like, cell by cell: the look
/// is written into the cells (Format as Table › Only Format As Table). `header_rows` rows on top
/// take the header's format; rows are banded.
pub fn range_looks(wb: &Workbook, sheet: usize, r: RangeRef, name: &str, header_rows: u32) -> Vec<(CellRef, Style)> {
    let Some(sh) = wb.sheet(sheet) else { return vec![] };
    let Some(def) = style_def(wb, name) else { return vec![] };
    let l = Layout {
        rows: r.height(),
        cols: r.width(),
        header_rows: header_rows.min(r.height()),
        totals: false,
        banded_rows: true,
        banded_cols: false,
        first_col: false,
        last_col: false,
    };
    let d0 = r.start.row + l.header_rows;
    r.iter()
        .map(|c| {
            let own = wb.styles.get(sh.style_id(c)).clone();
            let band_row = c.row.saturating_sub(d0).saturating_sub(hidden_between(&sh.rows, d0, c.row));
            (c, dress(&own, &compose(def, &l, c.row - r.start.row, c.col - r.start.col, band_row, c.col - r.start.col)))
        })
        .collect()
}

/// A gallery thumbnail of a style: a 5×5 table (a header and four banded rows), each cell as
/// `[fill, text, top, bottom, left, right]` colours (`#RRGGBB` or null).
pub fn preview(wb: &Workbook, def: &TableStyleDef) -> Json {
    let l = Layout { rows: 5, cols: 5, header_rows: 1, totals: false, banded_rows: true, banded_cols: false, first_col: false, last_col: false };
    let hex = |c: Option<Color>| c.and_then(|c| c.hex(&wb.theme));
    let line = |b: Option<BorderLine>| hex(b.map(|b| b.color).map(|c| if c == Color::Auto { DARK } else { c }));
    let rows: Vec<Json> = (0..5)
        .map(|r| {
            let cells: Vec<Json> = (0..5)
                .map(|c| {
                    let lk = compose(def, &l, r, c, r.saturating_sub(1), c);
                    json!([hex(lk.fill.as_ref().map(|f| f.fg)), hex(lk.color), line(lk.top), line(lk.bottom), line(lk.left), line(lk.right)])
                })
                .collect();
            Json::Array(cells)
        })
        .collect();
    Json::Array(rows)
}

/// The built-in styles the gallery shows for colour slot `slot`, by group: light (3 designs),
/// medium (4) and dark (the slot's dark design, then the four two-colour ones).
pub fn gallery(slot: u32) -> Vec<(&'static str, Vec<String>)> {
    let k = slot.min(COLOR_SLOTS - 1);
    let pick = |fam: &str, designs: u32| (0..designs).map(|p| format!("TableStyle{fam}{}", p * 7 + k + 1)).collect::<Vec<_>>();
    let mut dark = pick("Dark", 1);
    dark.extend((8..=11).map(|n| format!("TableStyleDark{n}")));
    vec![("light", pick("Light", 3)), ("medium", pick("Medium", 4)), ("dark", dark)]
}

/// The colour slot a built-in style belongs to (its theme colour in the gallery).
pub fn slot_of(name: &str) -> Option<u32> {
    if !is_builtin(name) {
        return None;
    }
    let (fam, n) = parse_style_name(name);
    if fam == "Dark" && n >= 8 {
        return None;
    }
    Some((n - 1) % 7)
}

/// Whether a cell holds something (a value or a formula).
fn filled(sh: &Sheet, c: CellRef) -> bool {
    sh.cell(c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some())
}

/// A free name for a table's new column: the header text when there is one, else the generic
/// name (`列5`, `Column5`).
fn new_column_name(t: &Table, text: &str, prefix: &str) -> String {
    let base = if text.trim().is_empty() { format!("{prefix}{}", t.columns.len() + 1) } else { text.to_string() };
    let mut name = base.clone();
    let mut k = 2;
    while t.columns.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
        name = format!("{base}{k}");
        k += 1;
    }
    name
}

/// Typing or pasting right under a table, or right of it, makes the table larger (AutoExpand, as
/// in Excel and WPS): new entries starting in the row under its last row (in its columns) add
/// those rows, entries starting in the column after its last column (in its rows) add those
/// columns. Entries into cells that held something, above or left of a table, under its total row
/// or into another table's way do not.
pub(crate) fn auto_expand(cx: &mut crate::cmd::Ctx, before: &Workbook) {
    // Per sheet: the area the edit wrote, and whether it filled a cell that was empty.
    let mut boxes: std::collections::BTreeMap<usize, (RangeRef, bool)> = std::collections::BTreeMap::new();
    for &(si, c) in cx.changed.iter().take(1_000_000) {
        let was_empty = before.sheet(si).is_none_or(|sh| !filled(sh, c));
        let now = cx.wb.sheet(si).is_some_and(|sh| filled(sh, c));
        let (b, new) = boxes.entry(si).or_insert((RangeRef::new(c, c), false));
        *b = RangeRef::new(CellRef::new(b.start.row.min(c.row), b.start.col.min(c.col)), CellRef::new(b.end.row.max(c.row), b.end.col.max(c.col)));
        *new |= was_empty && now;
    }
    for (si, (b, new)) in boxes {
        if !new {
            continue;
        }
        let Some(sh) = cx.wb.sheet(si) else { continue };
        let mut grow: Vec<(usize, RangeRef)> = Vec::new();
        for (ti, t) in sh.tables.iter().enumerate() {
            let r = t.range;
            let mut n = r;
            if !t.totals_row && r.end.row.checked_add(1) == Some(b.start.row) && b.start.col >= r.start.col && b.start.col <= r.end.col {
                n.end.row = b.end.row;
            } else if r.end.col.checked_add(1) == Some(b.start.col) && b.start.row >= r.start.row && b.start.row <= r.end.row {
                n.end.col = b.end.col;
            }
            let clear = !sh.tables.iter().enumerate().any(|(j, o)| j != ti && o.range.intersects(&n))
                && !sh.merges.iter().any(|m| m.intersects(&n) && !n.contains_range(m));
            if n != r && clear && u64::from(n.width()) <= 16_384 {
                grow.push((ti, n));
            }
        }
        if grow.is_empty() {
            continue;
        }
        let prefix = cx.column_prefix.clone();
        let Some(sh) = cx.wb.sheet_mut(si) else { continue };
        for (ti, n) in grow {
            let Some(t) = sh.tables.get(ti).cloned() else { continue };
            let mut columns = t.columns.clone();
            for col in t.range.end.col + 1..=n.end.col {
                let head = CellRef::new(n.start.row, col);
                let text = if t.header_row { sh.value(head).display() } else { String::new() };
                let name = new_column_name(&Table { columns: columns.clone(), ..t.clone() }, &text, &prefix);
                if t.header_row && text != name {
                    sh.set_value(head, kition_sheet_core::Value::text(name.as_str()));
                }
                columns.push(TableColumn { name, totals: TotalsFn::None, totals_label: None, formula: None });
            }
            if let Some(t) = sh.tables.get_mut(ti) {
                t.range = n;
                t.columns = columns;
            }
        }
        cx.structural = true;
    }
}
/// Text and text-value status used when a changed table header becomes a column name.
pub fn header_text(wb: &Workbook, si: usize, at: CellRef) -> Option<(String, bool)> {
    let sheet = wb.sheet(si)?;
    let shown = sheet
        .cell(at)
        .and_then(|cell| cell.formula.as_ref())
        .and_then(|formula| formula.expr_arc())
        .map(|expr| kition_sheet_calc::recalc::evaluate_expr(wb, si, at, &expr));
    let text = match &shown {
        Some(value) => value.display(),
        None => crate::display::cell_text(wb, sheet, at),
    };
    Some((text, shown.is_none() && sheet.value(at).is_text()))
}

/// Synchronizes edited headers with column names and their structured references.
pub(crate) fn sync_headers(cx: &mut crate::cmd::Ctx) {
    if cx.wb.sheets.iter().all(|s| s.tables.is_empty()) {
        return;
    }
    // (sheet, table name, table range, old column name, new column name)
    let mut renames: Vec<(usize, String, RangeRef, String, String)> = Vec::new();
    let heads: Vec<(usize, RangeRef)> = (0..cx.wb.sheets.len())
        .flat_map(|si| cx.wb.sheet(si).into_iter().flat_map(move |sh| sh.tables.iter().filter(|t| t.header_row).map(move |t| (si, t.range))))
        .map(|(si, r)| (si, RangeRef::new(r.start, CellRef::new(r.start.row, r.end.col))))
        .collect();
    // A structural edit can move anything into a header row: then every header is checked.
    let changed: std::collections::BTreeSet<(usize, CellRef)> = if cx.structural {
        heads.iter().flat_map(|(s, r)| r.iter().map(move |c| (*s, c))).collect()
    } else {
        cx.changed.iter().filter(|(si, c)| heads.iter().any(|(s, r)| s == si && r.contains(*c))).copied().collect()
    };
    // Per table: lower-case column name → column, and the next number to try for a repeated name
    // (both kept up to date, so a wide header row stays linear).
    type Names = (std::collections::HashMap<String, usize>, std::collections::HashMap<String, usize>);
    let mut names: std::collections::HashMap<(usize, usize), Names> = std::collections::HashMap::new();
    for (si, c) in changed {
        let Some(sh) = cx.wb.sheet(si) else { continue };
        let Some(ti) = sh.tables.iter().position(|t| t.header_row && c.row == t.range.start.row && t.range.contains(c)) else { continue };
        let Some(t) = sh.tables.get(ti) else { continue };
        let col = (c.col - t.range.start.col) as usize;
        let Some(old) = t.columns.get(col).map(|x| x.name.clone()) else { continue };
        // A formula in a header (pasted or typed there) becomes the value it shows (Excel, WPS):
        // its value is worked out now, before the workbook recalculates.
        let Some((text, is_text)) = header_text(&cx.wb, si, c) else { continue };
        if text == old && is_text {
            continue;
        }
        let (taken, next) = names
            .entry((si, ti))
            .or_insert_with(|| (t.columns.iter().enumerate().map(|(i, x)| (x.name.to_lowercase(), i)).collect(), Default::default()));
        if taken.get(&old.to_lowercase()) == Some(&col) {
            taken.remove(&old.to_lowercase());
        }
        let base = if text.trim().is_empty() { format!("{}{}", cx.column_prefix, col + 1) } else { text.clone() };
        let k = next.entry(base.to_lowercase()).or_insert(2);
        let mut name = base.clone();
        while taken.contains_key(&name.to_lowercase()) {
            name = format!("{base}{k}");
            *k += 1;
        }
        taken.insert(name.to_lowercase(), col);
        let (table, range) = (t.name.clone(), t.range);
        let Some(sh) = cx.wb.sheet_mut(si) else { continue };
        if let Some(x) = sh.tables.get_mut(ti).and_then(|t| t.columns.get_mut(col)) {
            x.name = name.clone();
        }
        // Headers are text: a number typed there, or a generated name, is written back as text.
        if name != text || !is_text {
            let style = sh.style_id(c);
            sh.cells.set(c, Cell { value: kition_sheet_core::Value::text(name.as_str()), style, ..Default::default() });
        }
        if !old.eq_ignore_ascii_case(&name) {
            renames.push((si, table, range, old, name));
        }
    }
    if renames.is_empty() {
        return;
    }
    // Structured references follow the new names.
    for i in 0..cx.wb.sheets.len() {
        let formulas: Vec<(CellRef, std::sync::Arc<Formula>)> =
            cx.wb.sheets.get(i).map(|s| s.cells.iter().filter_map(|(c, x)| x.formula.clone().map(|f| (c, f))).collect()).unwrap_or_default();
        for (c, f) in formulas {
            let Some(e) = f.expr() else { continue };
            let mut hit = false;
            let ne = e.map(&mut |x| match x {
                kition_sheet_formula::Expr::Struct(mut st) => {
                    for (si, table, range, old, name) in &renames {
                        let this = if st.table.is_empty() { i == *si && range.contains(c) } else { st.table.eq_ignore_ascii_case(table) };
                        if this {
                            for end in [&mut st.col_start, &mut st.col_end].into_iter().flatten() {
                                if end.eq_ignore_ascii_case(old) {
                                    *end = name.clone();
                                    hit = true;
                                }
                            }
                        }
                    }
                    kition_sheet_formula::Expr::Struct(st)
                }
                o => o,
            });
            if hit && let Some(cell) = cx.wb.sheet_mut(i).and_then(|s| s.cells.get_mut(c)) {
                cell.formula = Some(std::sync::Arc::new(Formula::from_expr(ne)));
            }
        }
    }
    cx.structural = true;
}

#[cfg(test)]
mod tests {
    use kition_sheet_core::Value;
    use serde_json::json;

    use crate::Session;

    #[test]
    fn editing_a_header_renames_the_column() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["a", "b", "c"], [1, 2, 3]]})).unwrap();
        s.execute("table.insert", json!({"range": "A1:C2", "header": true})).unwrap();
        s.execute("cell.set", json!({"cell": "E1", "input": "=SUM(Table1[a])"})).unwrap();
        s.execute("cell.set", json!({"cell": "A1", "input": "zz"})).unwrap();
        s.execute("edit.clearContents", json!({"range": "B1"})).unwrap();
        s.execute("cell.set", json!({"cell": "C1", "input": "ZZ"})).unwrap();
        let get = |s: &mut Session, a: &str| s.execute("cell.get", json!({"cell": a})).unwrap();
        assert_eq!(get(&mut s, "E1")["formula"], "=SUM(Table1[zz])");
        assert_eq!(get(&mut s, "E1")["value"], json!(1.0));
        let d = s.doc().unwrap();
        let sh = d.wb.sheet(0).unwrap();
        let names: Vec<&str> = sh.tables[0].columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["zz", "Column2", "ZZ2"]);
        assert_eq!(sh.value(kition_sheet_core::CellRef::new(0, 1)), Value::from("Column2"));
        assert_eq!(sh.value(kition_sheet_core::CellRef::new(0, 2)), Value::from("ZZ2"));
        s.execute("range.setValues", json!({"range": "A1", "values": [["x", "x", 5]]})).unwrap();
        let sh = s.doc().unwrap().wb.sheet(0).unwrap();
        let names: Vec<&str> = sh.tables[0].columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["x", "x2", "5"]);
        assert_eq!(sh.value(kition_sheet_core::CellRef::new(0, 2)), Value::from("5"));
    }

    #[test]
    fn go_to_a_table_selects_its_data() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["a", "b"], [1, 2], [3, 4]]})).unwrap();
        s.execute("table.insert", json!({"range": "A1:B3", "header": true})).unwrap();
        assert_eq!(s.execute("edit.goTo", json!({"reference": "Table1"})).unwrap()["selection"], "A2:B3");
    }
}

#[cfg(test)]
mod style_tests {
    use super::*;

    #[test]
    fn zero_numbered_styles_do_not_underflow() {
        assert_eq!(parse_style_name("TableStyleLight0"), ("Light", 1));
        assert_eq!(parse_style_name("TableStyleMedium0"), ("Medium", 1));
        for name in ["TableStyleLight0", "TableStyleMedium0", "TableStyleDark0"] {
            let _ = look(name);
        }
    }
}
