//! Table styles as WPS for Mac has them (module M7 「套用表格样式」, cases TS-C…): Format as Table
//! as a table or only its look (WPS's Only Format As Table), the range it takes, header names,
//! merged cells and array formulas, the gallery by theme colour, our own styles composed part by
//! part with their priorities (header over first column over banded rows over banded columns),
//! hidden rows keeping the bands alternate, a cell's own format over the style, AutoExpand when
//! typing or pasting beside a table, rows and columns inserted and deleted, clearing, merging,
//! the format painter, styles of the workbook's own (New Table Style, set as default, delete) and
//! saving them to .xlsx.

use kition_sheet_engine::Session;
use kition_sheet_engine::core::{CellRef, RangeRef};
use kition_sheet_engine::io::{open_bytes, save_bytes};
use kition_sheet_engine::model::{BorderStyle, Color, PatternType, Style, Table, Workbook};
use kition_sheet_engine::tables::{self, Layout};
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn cell(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

fn own(s: &Session, a1: &str) -> Style {
    let d = s.doc().unwrap();
    let sheet = d.wb.active().unwrap();
    d.wb.styles.get(sheet.style_id(cell(a1))).clone()
}

/// What the grid shows for a cell: its own format, with its table's look under it.
fn shown(s: &Session, a1: &str) -> Style {
    let d = s.doc().unwrap();
    let si = d.wb.active_sheet;
    let sheet = d.wb.active().unwrap();
    match sheet.table_at(cell(a1)) {
        Some(t) => tables::table_cell_style(&d.wb, si, t, cell(a1)),
        None => own(s, a1),
    }
}

fn fill(st: &Style) -> Option<Color> {
    (st.fill.pattern != PatternType::None).then_some(st.fill.fg)
}

fn table(s: &Session) -> Option<Table> {
    s.doc().unwrap().wb.active().unwrap().tables.first().cloned()
}

fn tables_on(s: &Session) -> usize {
    s.doc().unwrap().wb.active().unwrap().tables.len()
}

fn value(s: &mut Session, a1: &str) -> Json {
    run(s, "cell.get", json!({"cell": a1}))["value"].clone()
}

fn disabled(s: &Session, id: &str) -> bool {
    s.commands().iter().any(|c| c.id == id && !c.enabled)
}

/// A1:D10: a header row (名称, 地区, 一季度, 二季度) and nine rows of data.
fn data(s: &mut Session) {
    let mut rows = vec![json!(["名称", "地区", "一季度", "二季度"])];
    for r in 2..=10 {
        rows.push(json!([format!("n{r}"), if r % 2 == 0 { "东" } else { "西" }, r * 10, r * 7]));
    }
    run(s, "range.setValues", json!({"range": "A1", "values": rows}));
}

#[test]
fn format_as_table_from_one_cell_takes_the_data_around_it() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "selection.set", json!({"range": "B3"}));
    let gallery = run(&mut s, "table.styles", json!({}));
    assert_eq!(gallery["range"], "A1:D10", "TS-C6: the range offered is the data around the cell");
    let made = run(&mut s, "home.formatAsTable", json!({"style": "TableStyleMedium2"}));
    assert_eq!(made["range"], "A1:D10", "TS-C6");
    assert_eq!(table(&s).unwrap().range.a1(), "A1:D10");
    assert_eq!(fill(&own(&s, "F1")), None, "TS-C6: cells outside keep their format");
    // TS-C7: a range selected is taken as it is, never widened.
    let mut s = session();
    data(&mut s);
    run(&mut s, "selection.set", json!({"range": "A1:B5"}));
    run(&mut s, "home.formatAsTable", json!({"style": "TableStyleLight9"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:B5", "TS-C7");
    // Whole columns take the part of them holding something.
    let mut s = session();
    data(&mut s);
    run(&mut s, "selection.set", json!({"range": "A:C"}));
    run(&mut s, "home.formatAsTable", json!({"style": "TableStyleLight9"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:C10", "TS-C7: whole columns");
}

#[test]
fn only_format_writes_the_look_into_the_cells() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "selection.set", json!({"range": "A1:D10"}));
    run(&mut s, "home.formatAsTable", json!({"style": "TableStyleMedium2", "mode": "format", "titleRows": 1}));
    assert_eq!(tables_on(&s), 0, "TS-X1: Only Format As Table makes no table");
    let header = own(&s, "A1");
    assert_eq!(fill(&header), Some(Color::Theme(4, 0)), "the header takes Accent 1");
    assert!(header.font.bold && header.font.color == Color::Theme(0, 0));
    assert_eq!(fill(&own(&s, "A2")), Some(Color::Theme(4, 800)), "the first data row is banded");
    assert_eq!(fill(&own(&s, "A3")), None, "the second is not");
    assert_eq!(value(&mut s, "C3"), json!(30.0), "TS-C10: the data stay");
    // Two title rows.
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2", "mode": "format", "titleRows": 2}));
    assert_eq!(fill(&own(&s, "A2")), Some(Color::Theme(4, 0)), "the second title row looks like the header");
    assert_eq!(fill(&own(&s, "A3")), Some(Color::Theme(4, 800)));
}

#[test]
fn headers_get_names_keep_their_format_and_lose_formulas() {
    let mut s = session();
    run(&mut s, "range.setValues", json!({"range": "A1", "values": [[null, "地区", "=1+1"], [1, 2, 3], [4, 5, 6]]}));
    run(&mut s, "home.bold", json!({"range": "B1", "on": true}));
    run(&mut s, "home.formatAsTable", json!({"range": "A1:C3", "style": "TableStyleMedium2", "header": true, "columnPrefix": "列"}));
    assert_eq!(value(&mut s, "A1"), json!("列1"), "TS-C73: an empty header is named");
    assert_eq!(value(&mut s, "B1"), json!("地区"), "TS-C75: text stays the header");
    assert!(own(&s, "B1").font.bold, "TS-C74: the header keeps its format");
    let c1 = run(&mut s, "cell.get", json!({"cell": "C1"}));
    assert_eq!(
        (c1["value"].clone(), c1.get("formula").cloned().unwrap_or(Json::Null)),
        (json!("2"), Json::Null),
        "TS-C77: a formula header keeps its value"
    );
}

#[test]
fn merged_cells_come_apart_and_array_formulas_or_tables_refuse() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.mergeCells", json!({"range": "A2:B3"}));
    run(&mut s, "selection.set", json!({"range": "C5"}));
    run(&mut s, "home.formatAsTable", json!({"style": "TableStyleMedium2"}));
    let sheet = s.doc().unwrap().wb.active().unwrap().clone();
    assert!(sheet.merges.is_empty(), "TS-C13/C14/C96: merged cells in the range come apart");
    assert_eq!(value(&mut s, "A2"), json!("n2"), "TS-C96: the merged text stays in the first cell");
    // TS-C16: a range overlapping a table is refused, the table stays.
    run(&mut s, "selection.set", json!({"range": "E1"}));
    let err = s.execute("home.formatAsTable", json!({"range": "C3:F4", "style": "TableStyleLight1"})).unwrap_err().to_string();
    assert!(err.contains("overlap"), "{err}");
    assert_eq!((tables_on(&s), table(&s).unwrap().style), (1, "TableStyleMedium2".to_string()));
    // TS-C46: an array formula can't be in a table.
    let mut s = session();
    run(&mut s, "selection.set", json!({"range": "A1:A3"}));
    run(&mut s, "cell.set", json!({"cell": "A1", "input": "=ROW(A1:A3)", "array": true}));
    let err = s.execute("home.formatAsTable", json!({"range": "A1:B4", "style": "TableStyleMedium2"})).unwrap_err().to_string();
    assert!(err.contains("array"), "{err}");
    // TS-C9: several areas grey Format as Table.
    let mut s = session();
    data(&mut s);
    run(&mut s, "selection.set", json!({"range": "A1:A2,C1:C2"}));
    assert!(disabled(&s, "home.formatAsTable"), "TS-C9");
}

#[test]
fn parts_of_a_style_layer_header_first_column_rows_then_columns() {
    let def = tables::look("TableStyleMedium2").unwrap();
    let mut l = Layout { rows: 5, cols: 4, header_rows: 1, totals: false, banded_rows: true, banded_cols: true, first_col: true, last_col: false };
    let header = tables::compose(def, &l, 0, 0, 0, 0);
    assert_eq!(header.fill.as_ref().map(|f| f.fg), Some(Color::Theme(4, 0)), "TS-C25: the header beats the first column");
    let first = tables::compose(def, &l, 1, 0, 0, 0);
    assert!(first.bold, "the first column is bold");
    let rows = tables::compose(def, &l, 1, 1, 0, 1);
    assert_eq!(rows.fill.as_ref().map(|f| f.fg), Some(Color::Theme(4, 800)), "banded rows over banded columns");
    let cols = tables::compose(def, &l, 2, 2, 1, 2);
    assert_eq!(cols.fill.as_ref().map(|f| f.fg), Some(Color::Theme(4, 800)), "a column band where the row has none");
    let neither = tables::compose(def, &l, 2, 1, 1, 1);
    assert_eq!(neither.fill, None, "neither band");
    l.banded_rows = false;
    l.banded_cols = false;
    l.first_col = false;
    l.header_rows = 0;
    let plain = tables::compose(def, &l, 2, 1, 1, 1);
    assert_eq!(plain.fill, None, "TS-C23: every option off leaves rows and columns unbanded");
    // A colour slot recolours the same design (TS-C5): Medium4 is Medium2's design in Accent 3.
    let accent3 = tables::compose(tables::look("TableStyleMedium4").unwrap(), &Layout { header_rows: 1, ..l }, 0, 0, 0, 0);
    assert_eq!(accent3.fill.map(|f| f.fg), Some(Color::Theme(6, 0)));
}

#[test]
fn the_options_change_what_the_table_shows() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    assert_eq!(fill(&shown(&s, "B2")), Some(Color::Theme(4, 800)), "banded rows on by default");
    run(&mut s, "table.bandedRows", json!({"on": false}));
    assert_eq!(fill(&shown(&s, "B2")), None, "TS-C24");
    run(&mut s, "table.bandedColumns", json!({"on": true}));
    assert_eq!(fill(&shown(&s, "A2")), Some(Color::Theme(4, 800)), "TS-C24: banded columns");
    assert_eq!(fill(&shown(&s, "B2")), None);
    run(&mut s, "table.firstColumn", json!({"on": true}));
    assert!(shown(&s, "A5").font.bold, "TS-C38: first column");
    run(&mut s, "table.lastColumn", json!({"on": true}));
    assert!(shown(&s, "D5").font.bold, "TS-C38: last column");
    run(&mut s, "table.totalRow", json!({"on": true}));
    let t = table(&s).unwrap();
    assert_eq!(t.range.a1(), "A1:D11", "TS-C38: the total row is added under the data");
    assert_eq!(shown(&s, "B11").borders.top.style, BorderStyle::Double, "the total row's double rule");
    // The header row off and on again.
    run(&mut s, "table.headerRow", json!({"on": false}));
    let t = table(&s).unwrap();
    assert_eq!((t.header_row, t.range.a1()), (false, "A2:D11".to_string()));
    assert_eq!(value(&mut s, "A1"), Json::Null, "the header's text leaves the sheet");
    run(&mut s, "selection.set", json!({"range": "B5"}));
    run(&mut s, "table.headerRow", json!({"on": true}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:D11");
    assert_eq!(value(&mut s, "B1"), json!("地区"), "the names come back");
    // A style chosen in the gallery, then none.
    run(&mut s, "table.style", json!({"style": "TableStyleLight1"}));
    assert_eq!(table(&s).unwrap().style, "TableStyleLight1", "TS-C21");
    run(&mut s, "table.style", json!({"style": "None"}));
    assert_eq!(fill(&shown(&s, "A1")), None, "TS-C121: no style shows no look");
    assert_eq!(tables_on(&s), 1, "the table stays");
}

#[test]
fn hidden_rows_keep_the_bands_alternate_and_a_cells_own_format_wins() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    run(&mut s, "home.hideRows", json!({"rows": "3"}));
    assert_eq!(fill(&shown(&s, "A4")), None, "TS-C100: row 4 follows row 2 on screen, so it is not banded");
    assert_eq!(fill(&shown(&s, "A5")), Some(Color::Theme(4, 800)));
    run(&mut s, "home.unhideRows", json!({"rows": "3"}));
    assert_eq!(fill(&shown(&s, "A4")), Some(Color::Theme(4, 800)), "TS-C100: shown again, the bands are as before");
    run(&mut s, "home.fillColor", json!({"range": "B2", "color": "#FF0000"}));
    assert_eq!(fill(&shown(&s, "B2")), Some(Color::Rgb(0xFF0000)), "TS-C53/C56: the cell's own fill wins");
    run(&mut s, "edit.clearFormats", json!({"range": "B2"}));
    assert_eq!(fill(&shown(&s, "B2")), Some(Color::Theme(4, 800)), "TS-C53/C122: cleared, the table's look shows again");
    assert_eq!(tables_on(&s), 1);
}

#[test]
fn typing_or_pasting_beside_a_table_makes_it_larger() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    run(&mut s, "cell.set", json!({"cell": "B11", "input": "新"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:D11", "TS-C57: typing under the table adds a row");
    run(&mut s, "cell.set", json!({"cell": "E5", "input": "x"}));
    let t = table(&s).unwrap();
    assert_eq!(t.range.a1(), "A1:E11", "TS-C57: typing right of it adds a column");
    assert_eq!(value(&mut s, "E1"), json!("Column5"), "the new column is named");
    run(&mut s, "cell.set", json!({"cell": "E1", "input": "备注"}));
    assert_eq!(table(&s).unwrap().columns.last().unwrap().name, "备注");
    // TS-C62: above or left, no.
    run(&mut s, "cell.set", json!({"cell": "F20", "input": "far"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:E11", "TS-C62");
    // TS-C63: a cell that already held something, no.
    run(&mut s, "cell.set", json!({"cell": "A13", "input": "a"}));
    run(&mut s, "cell.set", json!({"cell": "A12", "input": "b"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:E12", "the empty A12 grows it");
    run(&mut s, "cell.set", json!({"cell": "A13", "input": "changed"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:E12", "TS-C63: A13 held something already");
    // TS-C61: filling does not grow it.
    run(&mut s, "edit.fillDown", json!({"range": "B12:B14"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:E12", "TS-C61");
    // TS-C58: a block pasted under the table grows it by its rows.
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    run(&mut s, "edit.paste", json!({"at": "A11", "text": "p\t1\nq\t2\n"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:D12", "TS-C58");
    // TS-C64: a block pasted inside the table and running past it does not.
    run(&mut s, "edit.paste", json!({"at": "B12", "text": "x\ny\nz\n"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:D12", "TS-C64");
    // A total row stops growth under the table.
    run(&mut s, "table.totalRow", json!({"on": true}));
    run(&mut s, "cell.set", json!({"cell": "A16", "input": "under"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:D13");
}

#[test]
fn rows_and_columns_inserted_or_deleted_resize_the_table() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    run(&mut s, "home.insertRows", json!({"rows": "5"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:D11", "TS-C69: a row inserted inside");
    run(&mut s, "home.insertColumns", json!({"cols": "D"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:E11", "TS-C59: a column inserted at the last column");
    run(&mut s, "home.insertColumns", json!({"cols": "G"}));
    assert_eq!(table(&s).unwrap().range.a1(), "A1:E11", "TS-C67: a column inserted past it does not");
    run(&mut s, "home.deleteRows", json!({"rows": "2:3"}));
    run(&mut s, "home.deleteColumns", json!({"cols": "B"}));
    let t = table(&s).unwrap();
    assert_eq!((t.range.a1(), t.columns.len()), ("A1:D9".to_string(), 4), "TS-C70/C89: deleting shrinks it");
    assert_eq!(fill(&shown(&s, "A2")), Some(Color::Theme(4, 800)), "TS-C70: the bands start again under the header");
}

#[test]
fn clearing_merging_and_painting() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    run(&mut s, "selection.set", json!({"range": "B2:C3"}));
    assert!(disabled(&s, "home.mergeCenter") && disabled(&s, "home.mergeCells"), "TS-C97: merging is greyed in a table");
    assert!(s.execute("home.mergeCells", json!({"range": "B2:C3"})).is_err(), "TS-C97");
    // TS-C54 and the format painter carrying a table's look out of it.
    run(&mut s, "selection.set", json!({"range": "A1"}));
    run(&mut s, "edit.formatPainter", json!({}));
    run(&mut s, "edit.formatPainter", json!({"apply": "F1"}));
    assert_eq!(fill(&own(&s, "F1")), Some(Color::Theme(4, 0)), "painting a header cell paints its look");
    // TS-C124: clearing contents keeps the look.
    run(&mut s, "edit.clearContents", json!({"range": "B3:C4"}));
    assert_eq!(fill(&shown(&s, "B4")), Some(Color::Theme(4, 800)), "TS-C124");
    // TS-C123: the whole table cleared of formats shows no style; TS-C127: cleared of all, it goes.
    run(&mut s, "edit.clearFormats", json!({"range": "A1:D10"}));
    assert_eq!((tables_on(&s), fill(&shown(&s, "A1"))), (1, None), "TS-C123");
    run(&mut s, "edit.clearAll", json!({"range": "A1:D10"}));
    assert_eq!(tables_on(&s), 0, "TS-C127");
}

#[test]
fn convert_to_range_keeps_the_look_and_ends_the_table() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    let before = shown(&s, "A2");
    run(&mut s, "selection.set", json!({"range": "B2"}));
    run(&mut s, "table.convertToRange", json!({}));
    assert_eq!(tables_on(&s), 0, "TS-C129");
    assert_eq!(fill(&own(&s, "A2")), fill(&before), "TS-C130: the look is now the cells' own");
    run(&mut s, "home.formatAsTable", json!({"style": "TableStyleLight1"}));
    assert_eq!(tables_on(&s), 1, "TS-C131: it can be formatted as a table again");
}

#[test]
fn the_gallery_offers_theme_colours_and_groups() {
    let mut s = session();
    data(&mut s);
    let g = run(&mut s, "table.styles", json!({"slot": 3}));
    assert_eq!(g["colors"].as_array().unwrap().len(), 7, "TS-C4: Text 1 and six accents");
    let groups: Vec<(String, usize)> =
        g["groups"].as_array().unwrap().iter().map(|x| (x["id"].as_str().unwrap().to_string(), x["styles"].as_array().unwrap().len())).collect();
    assert_eq!(groups, [("light".to_string(), 3), ("medium".to_string(), 4), ("dark".to_string(), 5)], "TS-C4");
    assert_eq!(g["groups"][1]["styles"][0]["name"], "TableStyleMedium4", "TS-C5: the medium group in Accent 3");
    let other = run(&mut s, "table.styles", json!({"slot": 1}));
    assert_eq!(other["groups"][1]["styles"][0]["name"], "TableStyleMedium2", "TS-C5: another colour, other styles");
    assert_ne!(other["groups"][1]["styles"][0]["preview"], g["groups"][1]["styles"][0]["preview"], "TS-C5: and other previews");
    assert_eq!(g["default"], "TableStyleMedium2", "TS-C29");
}

fn custom(s: &mut Session) {
    run(
        s,
        "table.newStyle",
        json!({"name": "我的样式", "elements": {
            "headerRow": {"fill": "#123456", "color": "#FFFFFF", "bold": true},
            "firstRowStripe": {"fill": "#EEDDCC"},
            "wholeTable": {"border": "#654321"}
        }, "default": true}),
    );
}

#[test]
fn styles_of_the_workbooks_own_are_kept_and_saved() {
    let mut s = session();
    data(&mut s);
    custom(&mut s);
    let g = run(&mut s, "table.styles", json!({}));
    assert_eq!(g["custom"][0]["name"], "我的样式", "TS-C22/C115: the new style is listed");
    assert_eq!(g["default"], "我的样式", "TS-C30/C35: and is the default");
    run(&mut s, "selection.set", json!({"range": "B2"}));
    run(&mut s, "home.formatAsTable", json!({}));
    assert_eq!(table(&s).unwrap().style, "我的样式", "TS-C35: Format as Table takes the default");
    assert_eq!(fill(&shown(&s, "A1")), Some(Color::Rgb(0x123456)), "TS-C33: its colours");
    assert_eq!(fill(&shown(&s, "A2")), Some(Color::Rgb(0xEEDDCC)));
    assert_eq!(shown(&s, "A5").borders.left.color, Color::Rgb(0x654321));
    // Saved and opened again (TS-C35 reopened, the .xlsx's own table styles).
    let bytes = save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
    let wb: Workbook = open_bytes("Book.xlsx", &bytes).unwrap().0;
    assert_eq!(wb.table_styles.default_table.as_deref(), Some("我的样式"));
    let t = wb.active().unwrap().tables[0].clone();
    assert_eq!(t.style, "我的样式");
    let a1 = tables::table_cell_style(&wb, 0, &t, cell("A1"));
    assert_eq!(fill(&a1), Some(Color::Rgb(0x123456)), "the reopened file draws the style");
    // Modified, then deleted: its tables take the default.
    run(&mut s, "table.newStyle", json!({"name": "我的样式2", "replace": "我的样式", "elements": {"headerRow": {"fill": "#00FF00"}}}));
    assert_eq!((table(&s).unwrap().style, fill(&shown(&s, "A1"))), ("我的样式2".to_string(), Some(Color::Rgb(0x00FF00))), "TS-C37");
    run(&mut s, "table.deleteStyle", json!({"name": "我的样式2"}));
    assert_eq!(table(&s).unwrap().style, "TableStyleMedium2", "TS-C28: deleted, its table takes the default");
    assert!(run(&mut s, "table.styles", json!({}))["custom"].as_array().unwrap().is_empty());
    assert!(s.execute("table.newStyle", json!({"name": "TableStyleLight1"})).is_err(), "a built-in name is refused");
}

#[test]
fn every_builtin_style_is_drawn_and_named_tables_round_trip() {
    let wb = Workbook::default();
    for fam in ["Light", "Medium", "Dark"] {
        for n in 1..=28 {
            let name = format!("TableStyle{fam}{n}");
            if let Some(def) = tables::look(&name) {
                assert!(def.elements.iter().any(|e| e.kind == "headerRow"), "{name}");
                let _ = tables::preview(&wb, def);
            }
        }
    }
    assert_eq!(tables::look("TableStyleLight21").map(|d| d.name.as_str()), Some("TableStyleLight21"));
    assert!(tables::look("TableStyleDark12").is_none());
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleDark3"}));
    run(&mut s, "table.style", json!({"style": "None"}));
    let bytes = save_bytes(&s.doc().unwrap().wb, "Book.xlsx").unwrap();
    let wb = open_bytes("Book.xlsx", &bytes).unwrap().0;
    assert_eq!(wb.active().unwrap().tables[0].style, "", "no style stays no style");
    let range = RangeRef::parse("A1:D10").unwrap();
    assert_eq!(wb.active().unwrap().tables[0].range, range);
}

#[test]
fn resize_table_takes_valid_ranges_on_its_sheet() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    run(&mut s, "home.formatAsTable", json!({"range": "H1:I3", "style": "TableStyleLight1", "header": true}));
    run(&mut s, "selection.set", json!({"range": "B2"}));
    let range = |s: &Session| table(s).unwrap().range.a1();
    run(&mut s, "table.resize", json!({"range": "a1:c5"}));
    assert_eq!(range(&s), "A1:C5", "TS-C39/C48: a1:c5 is taken as A1:C5");
    run(&mut s, "table.resize", json!({"range": "$A$1:$D$1048999"}));
    assert_eq!(range(&s), "A:D", "TS-C49: past the last row it is cut to the sheet (A1:D1048576)");
    run(&mut s, "table.resize", json!({"range": "a:b"}));
    assert_eq!(range(&s), "A:B", "TS-C48: whole columns");
    run(&mut s, "table.resize", json!({"range": "A1:D10"}));
    for bad in ["B1:A0", "1:2", "", "Sheet9!A1:B4", "A1:I5"] {
        assert!(s.execute("table.resize", json!({"range": bad})).is_err(), "TS-C40/C44/C50/C51: {bad}");
        assert_eq!(range(&s), "A1:D10", "the table keeps its range: {bad}");
    }
}

#[test]
fn one_row_groups_alone() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "selection.set", json!({"range": "B3"}));
    run(&mut s, "data.group", json!({"rows": "1"}));
    run(&mut s, "data.hideDetail", json!({"rows": "1"}));
    let sheet = s.doc().unwrap().wb.active().unwrap().clone();
    assert!(sheet.is_row_hidden(0) && !sheet.is_row_hidden(2), "TS-C105: \"1\" is row 1, not the selection's row");
}

#[test]
fn a_formula_pasted_into_a_header_becomes_its_value() {
    let mut s = session();
    data(&mut s);
    run(&mut s, "home.formatAsTable", json!({"range": "A1:D10", "style": "TableStyleMedium2"}));
    run(&mut s, "cell.set", json!({"cell": "H1", "input": "=10*3"}));
    run(&mut s, "edit.copy", json!({"range": "H1"}));
    run(&mut s, "edit.paste", json!({"at": "B1"}));
    let b1 = run(&mut s, "cell.get", json!({"cell": "B1"}));
    assert_eq!((b1["value"].clone(), b1["formula"].clone()), (json!("30"), Json::Null), "TS-C85: the header shows the formula's value, as text");
    assert_eq!(table(&s).unwrap().columns[1].name, "30");
    assert_eq!(run(&mut s, "cell.get", json!({"cell": "H1"}))["formula"], "=10*3", "the source keeps its formula");
}
