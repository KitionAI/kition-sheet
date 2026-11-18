//! Workbooks.

use std::collections::BTreeMap;
use std::sync::Arc;

use kition_sheet_core::DateSystem;
use kition_sheet_formula::Expr;
use serde::{Deserialize, Serialize};

use crate::cell::Formula;
use crate::sheet::Sheet;
use crate::style::{StyleTable, Theme};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalcMode {
    #[default]
    Automatic,
    AutomaticExceptTables,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalcSettings {
    pub mode: CalcMode,
    pub iterative: bool,
    pub max_iterations: u32,
    pub max_change: f64,
    /// Use precision as displayed.
    pub precision_as_displayed: bool,
    /// Recalculate every formula when the file is next opened (`fullCalcOnLoad`), as the file
    /// asked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub full_calc_on_load: bool,
}

impl Default for CalcSettings {
    fn default() -> Self {
        CalcSettings {
            mode: CalcMode::Automatic,
            iterative: false,
            max_iterations: 100,
            max_change: 0.001,
            precision_as_displayed: false,
            full_calc_on_load: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefinedName {
    pub name: String,
    /// `None` = workbook scope; otherwise the sheet index.
    pub scope: Option<usize>,
    /// Formula text without `=` (`Sheet1!$A$1:$A$10`, `0.07`, `LAMBDA(x,x*2)`).
    pub formula: String,
    pub comment: String,
    pub hidden: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DocProps {
    pub title: String,
    pub subject: String,
    pub author: String,
    pub company: String,
    pub keywords: String,
    pub description: String,
    /// `cp:category`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// `cp:lastModifiedBy`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_modified_by: String,
    /// Creation and last modification times (W3CDTF, e.g. `2026-01-02T03:04:05Z`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub modified: String,
    /// The custom document properties part (`docProps/custom.xml`), verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_xml: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workbook {
    pub sheets: Vec<Arc<Sheet>>,
    pub names: Vec<DefinedName>,
    #[serde(default)]
    pub identities: crate::identities::WorkbookIdentities,
    pub styles: StyleTable,
    pub theme: Theme,
    pub date_system: DateSystem,
    pub calc: CalcSettings,
    pub active_sheet: usize,
    pub props: DocProps,
    /// Workbook structure protection.
    pub protected_structure: bool,
    /// Custom fill lists (Options › Custom Lists) beyond the built-in ones.
    pub custom_lists: Vec<Vec<String>>,
    /// Named cell styles (Cell Styles gallery): name → style.
    pub cell_styles: Vec<(String, crate::style::Style)>,
    /// The colours a file's style part lists of its own (`styles.xml` `<colors>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<Palette>,
    /// What the file says of its named cell styles beyond their formats, by name (see
    /// `CellStyleInfo`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cell_style_info: BTreeMap<String, CellStyleInfo>,
    /// The workbook window as the file gave it (`bookViews/workbookView`, without the active
    /// sheet, which is `active_sheet`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<WorkbookView>,
    /// The application versions that wrote the file (`fileVersion`), kept as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_version: Option<FileVersion>,
    /// The table and pivot table styles the file defines (`tableStyles`).
    #[serde(default, skip_serializing_if = "TableStyles::is_empty")]
    pub table_styles: TableStyles,
    /// Extensions of the workbook and style parts Kition does not read, kept as written.
    #[serde(default, skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

/// The table styles of a workbook (SpreadsheetML `tableStyles`): the styles the file defines and
/// the defaults it names for new tables and pivot tables.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableStyles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_table: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_pivot: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<TableStyleDef>,
}

impl TableStyles {
    pub fn is_empty(&self) -> bool {
        *self == TableStyles::default()
    }
}

/// A table or pivot table style a file defines (`tableStyle`), as written: its name, whether it is
/// offered for tables and for pivot tables (absent: both), its revision id, and its elements.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableStyleDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<bool>,
    /// `xr9:uid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<TableStyleElement>,
}

/// A part of a table and how a table style formats it (`tableStyleElement`): the part
/// (`wholeTable`, `headerRow`, `firstRowStripe`, …), the stripe size, and the format.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableStyleElement {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    pub style: crate::style::Style,
}

/// Extension lists (`extLst`) of the workbook and style parts that Kition does not read, kept as
/// written: each item is one `ext` element, declaring the namespaces it uses (Excel's calculation
/// features, slicer styles, …). Extensions naming other parts are not kept.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extensions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workbook: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
}

impl Extensions {
    pub fn is_empty(&self) -> bool {
        self.workbook.is_empty() && self.styles.is_empty()
    }
}

/// A named cell style's place among Excel's built-in styles (SpreadsheetML `cellStyle`): which
/// one it is (`builtinId`; applications show it under their own language's name, whatever name
/// the file gives it), the outline level of the row and column level styles (`iLevel`), whether
/// its format was changed from the built-in one (`customBuiltin`), and whether the gallery hides
/// it (`hidden`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellStyleInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom_builtin: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// The formats the style leaves out (`applyX="0"` on its `cellStyleXfs` entry: Excel's
    /// "Style includes" boxes cleared); applying the style leaves a cell's own.
    #[serde(default, skip_serializing_if = "StyleParts::is_empty")]
    pub excludes: StyleParts,
}

/// A set of the parts of a cell format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleParts {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub number_format: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub font: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fill: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub border: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub alignment: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub protection: bool,
}

impl StyleParts {
    pub fn is_empty(&self) -> bool {
        *self == StyleParts::default()
    }
}

/// The workbook window (SpreadsheetML `workbookView`): its place and size on screen (twips), the
/// share of the window the sheet tabs take, the first tab shown, and its switches. Every field is
/// optional: absent means the application's default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbookView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Tab bar width, per mille of the window width (`tabRatio`, Excel's default 600).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_ratio: Option<u32>,
    /// The first sheet tab shown in the tab bar (`firstSheet`, a sheet index).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_sheet: Option<u32>,
    /// `visible`, `hidden` or `veryHidden`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimized: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_horizontal_scroll: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_vertical_scroll: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_sheet_tabs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_filter_date_grouping: Option<bool>,
}

/// The versions of the applications that wrote a file (SpreadsheetML `fileVersion`), as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileVersion {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub app_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_edited: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lowest_edited: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rup_build: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code_name: String,
}

/// A workbook's own colours: the legacy indexed palette it replaces and the recently used colours
/// (SpreadsheetML `<colors>`, `<indexedColors>` and `<mruColors>`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    /// The indexed colours, `0xRRGGBB` each, replacing the built-in 64 (empty: the built-in ones).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexed: Vec<u32>,
    /// The recently used colours, as written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<crate::rich::RunColor>,
}

impl Default for Workbook {
    fn default() -> Self {
        Workbook::new()
    }
}

impl Workbook {
    /// A new workbook with one sheet, "Sheet1".
    pub fn new() -> Workbook {
        let mut first = Sheet::new("Sheet1");
        first.sheet_id = 1;
        Workbook {
            sheets: vec![Arc::new(first)],
            names: vec![],
            identities: Default::default(),
            styles: StyleTable::default(),
            theme: Theme::default(),
            date_system: DateSystem::D1900,
            calc: CalcSettings::default(),
            active_sheet: 0,
            props: DocProps::default(),
            protected_structure: false,
            custom_lists: vec![],
            cell_styles: vec![],
            palette: None,
            cell_style_info: BTreeMap::new(),
            view: None,
            file_version: None,
            table_styles: TableStyles::default(),
            extensions: Extensions::default(),
        }
    }
    pub fn sheet(&self, i: usize) -> Option<&Sheet> {
        self.sheets.get(i).map(|s| s.as_ref())
    }
    pub fn sheet_mut(&mut self, i: usize) -> Option<&mut Sheet> {
        self.sheets.get_mut(i).map(Arc::make_mut)
    }
    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets.iter().position(|s| s.name.eq_ignore_ascii_case(name))
    }
    /// The index of the sheet whose id ([`Sheet::sheet_id`]) is `id`.
    pub fn sheet_by_id(&self, id: u32) -> Option<usize> {
        if id == 0 {
            return None;
        }
        self.sheets.iter().position(|s| s.sheet_id == id)
    }
    /// Each sheet's id as [`Workbook::number_sheets`] would leave it, without changing the
    /// workbook: what a writer saves.
    pub fn numbered_sheet_ids(&self) -> Vec<u32> {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.sheets.iter().map(|s| s.sheet_id).max().unwrap_or(0);
        self.sheets
            .iter()
            .map(|sheet| {
                if sheet.sheet_id != 0 && seen.insert(sheet.sheet_id) {
                    return sheet.sheet_id;
                }
                next = next.saturating_add(1);
                seen.insert(next);
                next
            })
            .collect()
    }
    /// Gives every sheet without an id (0), or with the id of a sheet before it, a fresh one: the
    /// next number after the highest the workbook has. Sheets that have their own keep it (and
    /// their `Arc` is left as it is).
    pub fn number_sheets(&mut self) {
        let ids = self.numbered_sheet_ids();
        for (sheet, id) in self.sheets.iter_mut().zip(ids) {
            if sheet.sheet_id != id {
                Arc::make_mut(sheet).sheet_id = id;
            }
        }
    }
    pub fn active(&self) -> Option<&Sheet> {
        self.sheet(self.active_sheet)
    }
    /// A fresh sheet name `SheetN` not in use.
    /// The name of a new sheet: `Sheet` and the number after the highest one taken (after Sheet1 to
    /// Sheet4 with Sheet2 deleted, Sheet5), so a deleted sheet's name does not come back, as in
    /// Excel and WPS.
    pub fn next_sheet_name(&self) -> String {
        let highest = self
            .sheets
            .iter()
            .filter_map(|s| {
                let (head, n) = s.name.split_at_checked(5)?;
                if !head.eq_ignore_ascii_case("Sheet") || n.is_empty() || n.len() > 6 || n.starts_with('0') || !n.bytes().all(|b| b.is_ascii_digit())
                {
                    return None;
                }
                n.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0);
        (highest.saturating_add(1)..).map(|n| format!("Sheet{n}")).find(|n| self.sheet_index(n).is_none()).unwrap_or_else(|| "Sheet".into())
    }
    /// Validates a sheet name like Excel: 1–31 chars, none of `: \ / ? * [ ]`, not starting or
    /// ending with `'`, unique (case-insensitive) unless it is `except`'s own name.
    pub fn check_sheet_name(&self, name: &str, except: Option<usize>) -> Result<(), String> {
        let n = name.chars().count();
        if n == 0 {
            return Err("A sheet name can't be blank.".into());
        }
        if n > 31 {
            return Err("A sheet name can't be longer than 31 characters.".into());
        }
        if name.chars().any(|c| matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']')) {
            return Err("A sheet name can't contain any of these characters: : \\ / ? * [ ]".into());
        }
        if name.starts_with('\'') || name.ends_with('\'') {
            return Err("A sheet name can't begin or end with an apostrophe.".into());
        }
        if name.eq_ignore_ascii_case("History") {
            return Err("\"History\" is a reserved name.".into());
        }
        if let Some(i) = self.sheet_index(name)
            && Some(i) != except
        {
            return Err("That name is already taken. Try a different one.".into());
        }
        Ok(())
    }
    /// Finds a defined name visible from `sheet`: sheet scope first, then workbook scope.
    pub fn name(&self, name: &str, sheet: usize) -> Option<&DefinedName> {
        self.names
            .iter()
            .find(|n| n.scope == Some(sheet) && n.name.eq_ignore_ascii_case(name))
            .or_else(|| self.names.iter().find(|n| n.scope.is_none() && n.name.eq_ignore_ascii_case(name)))
    }
    /// Spells calls of LAMBDA names the way the names are defined (`Double(4)`, where the
    /// parser gives `DOUBLE(4)`: it upper-cases function names, as built-in ones are shown).
    pub fn name_call_case(&self, e: Expr) -> Expr {
        spell_calls(e, &self.lambda_names())
    }
    /// [`Workbook::name_call_case`] for every cell formula and defined name (after reading a
    /// file).
    pub fn apply_name_call_case(&mut self) {
        let lambdas = self.lambda_names();
        if lambdas.is_empty() {
            return;
        }
        for sheet in self.sheets.iter_mut() {
            let fixed: Vec<_> = sheet
                .cells
                .iter()
                .filter_map(|(c, cell)| {
                    let f = cell.formula.as_ref()?;
                    let e = f.expr()?;
                    let ne = spell_calls(e.clone(), &lambdas);
                    (ne != e).then(|| (c, Formula { array: f.array, ..Formula::from_expr(ne) }))
                })
                .collect();
            if fixed.is_empty() {
                continue;
            }
            let sheet = Arc::make_mut(sheet);
            for (c, f) in fixed {
                if let Some(cell) = sheet.cells.get_mut(c) {
                    cell.formula = Some(Arc::new(f));
                }
            }
        }
        for n in self.names.iter_mut() {
            if let Ok(e) = kition_sheet_formula::parse(&n.formula) {
                let ne = spell_calls(e.clone(), &lambdas);
                if ne != e {
                    n.formula = kition_sheet_formula::print(&ne);
                }
            }
        }
    }
    /// Names of the defined names holding a LAMBDA (callable like functions).
    fn lambda_names(&self) -> Vec<String> {
        let is_lambda = |f: &str| f.trim_start_matches('=').trim_start().get(..7).is_some_and(|p| p.eq_ignore_ascii_case("LAMBDA("));
        self.names.iter().filter(|n| is_lambda(&n.formula)).map(|n| n.name.clone()).collect()
    }
    /// Table by name across sheets: (sheet index, table index).
    pub fn table(&self, name: &str) -> Option<(usize, usize)> {
        for (si, s) in self.sheets.iter().enumerate() {
            if let Some(ti) = s.tables.iter().position(|t| t.name.eq_ignore_ascii_case(name)) {
                return Some((si, ti));
            }
        }
        None
    }
    /// The next free object ID, or `None` when an imported ID exhausts the namespace.
    pub fn next_object_id(&self) -> Option<u32> {
        let mut m = 0;
        for s in &self.sheets {
            for c in &s.charts {
                m = m.max(c.id);
            }
            for c in &s.images {
                m = m.max(c.id);
            }
            for c in &s.shapes {
                // A group's key is an object id too.
                m = c.groups.iter().copied().fold(m.max(c.id), u32::max);
            }
            for c in &s.tables {
                m = m.max(c.id);
            }
            for c in &s.diagrams {
                m = m.max(c.id);
            }
        }
        m.checked_add(1)
    }
}

/// Calls of the names in `lambdas` spelled as the names are.
fn spell_calls(e: Expr, lambdas: &[String]) -> Expr {
    if lambdas.is_empty() {
        return e;
    }
    e.map(&mut |x| match x {
        Expr::Call(n, args) => match lambdas.iter().find(|l| l.eq_ignore_ascii_case(&n)) {
            Some(l) => Expr::Call(l.clone(), args),
            None => Expr::Call(n, args),
        },
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_names() {
        let wb = Workbook::new();
        assert_eq!(wb.next_sheet_name(), "Sheet2");
        assert!(wb.check_sheet_name("sheet1", None).is_err());
        assert!(wb.check_sheet_name("sheet1", Some(0)).is_ok());
        assert!(wb.check_sheet_name("a/b", None).is_err());
        assert!(wb.check_sheet_name(&"x".repeat(32), None).is_err());
        assert!(wb.check_sheet_name("Budget 2026", None).is_ok());
    }
}
