//! Reading the workbook part and everything hanging off it.

use std::sync::Arc;

use kition_sheet_core::{DateSystem, RangeRef};
use kition_sheet_model::{CalcMode, CellPicture, DefinedName, FileVersion, Sheet, Style, StyleId, StyleTable, Visibility, Workbook, WorkbookView};

use crate::package::{Package, Rel};
use crate::styles::{StylesIn, read_styles};
use crate::xml::{self, El};
use crate::{IoError, ReadReport};

/// State shared while reading one package.
pub struct Ctx<'a> {
    pub pkg: Package<'a>,
    pub report: ReadReport,
    /// Shared strings: each item's text and its rich text runs, if any.
    pub sst: Vec<(Arc<str>, Option<Arc<kition_sheet_model::RichText>>)>,
    /// Cell format index → model style.
    pub xf_map: Vec<StyleId>,
    pub dxfs: Vec<Style>,
    pub palette: Vec<u32>,
    pub date_system: DateSystem,
    pub styles: StyleTable,
    /// Threaded-comment person id → display name.
    pub persons: std::collections::HashMap<String, String>,
    /// The object ids given so far (charts, pictures, shapes and their groups, diagrams, tables):
    /// unique in the workbook.
    pub used_ids: std::collections::HashSet<u32>,
    /// The highest of them.
    pub max_id: u32,
    /// Custom number formats from the styles part, by id.
    pub num_fmts: std::collections::HashMap<u32, String>,
    /// Value metadata blocks, indexed by the worksheet's one-based `vm` attribute.
    pub cell_pictures: Vec<Option<Arc<CellPicture>>>,
    /// Pixels per character of column width (the workbook's character unit).
    pub char_px: f64,
    /// The height of a row of the default font, in px: what rows without a height of their own
    /// are, unless the sheet sets its default row height (`customHeight`).
    pub auto_row_px: f32,
}

impl Ctx<'_> {
    pub fn warn(&mut self, m: impl Into<String>) {
        self.report.warn(m);
    }
    /// A fresh id for a chart, picture, shape or table.
    pub fn next_object_id(&mut self) -> u32 {
        self.object_id(None)
    }
    /// An id for a drawing object: the file's own (`cNvPr id`, `own`) when it is one (from 1) no
    /// object of the workbook has yet, so the object keeps it; else a fresh one. Files number each
    /// drawing part's objects on their own, so ids repeat across sheets: the later holder gets a
    /// new id, which a save writes.
    pub fn object_id(&mut self, own: Option<u32>) -> u32 {
        if let Some(own) = own.filter(|id| *id >= 1)
            && self.used_ids.insert(own)
        {
            self.max_id = self.max_id.max(own);
            return own;
        }
        loop {
            self.max_id = self.max_id.saturating_add(1);
            if self.used_ids.insert(self.max_id) || self.max_id == u32::MAX {
                return self.max_id;
            }
        }
    }
    pub fn style(&self, xf: Option<u32>) -> StyleId {
        xf.and_then(|i| self.xf_map.get(i as usize).copied()).unwrap_or_default()
    }
    /// Reads and parses an optional part: problems other than size limits become warnings.
    pub fn optional_xml(&mut self, part: &str) -> Result<Option<El>, IoError> {
        match self.pkg.read_xml(part) {
            Ok(Some(e)) => Ok(Some(e)),
            Ok(None) => {
                self.warn(format!("missing part {part}"));
                Ok(None)
            }
            Err(IoError::TooLarge(m)) => Err(IoError::TooLarge(m)),
            Err(e) => {
                self.warn(format!("skipped {part}: {e}"));
                Ok(None)
            }
        }
    }
}

pub fn find_rel<'r>(rels: &'r [Rel], kind: &str) -> Option<&'r Rel> {
    rels.iter().find(|r| r.kind == kind)
}

pub fn read_xlsx(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    let pkg = Package::open(bytes)?;
    let mut cx = Ctx {
        pkg,
        report: ReadReport::default(),
        sst: vec![],
        xf_map: vec![],
        dxfs: vec![],
        palette: crate::tables::INDEXED.to_vec(),
        date_system: DateSystem::D1900,
        styles: StyleTable::default(),
        persons: Default::default(),
        used_ids: Default::default(),
        max_id: 0,
        num_fmts: Default::default(),
        cell_pictures: vec![],
        char_px: kition_sheet_model::units::char_width_px(kition_sheet_model::DEFAULT_FONT, kition_sheet_model::DEFAULT_FONT_SIZE),
        auto_row_px: crate::FILE_DEFAULT_ROW_PX,
    };
    let root_rels = cx.pkg.rels("")?;
    let wb_part = find_rel(&root_rels, "officeDocument").map(|r| r.target.clone()).unwrap_or_else(|| "xl/workbook.xml".into());
    let wb_xml = match cx.pkg.read_xml(&wb_part)? {
        Some(x) => x,
        None => {
            if cx.pkg.has("xl/workbook.xml") && wb_part != "xl/workbook.xml" {
                cx.pkg.read_xml("xl/workbook.xml")?.ok_or_else(|| IoError::Format("no workbook part".into()))?
            } else if cx.pkg.has("content.xml") {
                return Err(IoError::Format("this is an OpenDocument file, not XLSX".into()));
            } else {
                return Err(IoError::Format("no workbook part".into()));
            }
        }
    };
    if wb_xml.name != "workbook" {
        return Err(IoError::Format(format!("unexpected root element <{}> in workbook part", wb_xml.name)));
    }
    let wb_rels = cx.pkg.rels(&wb_part)?;
    cx.cell_pictures = match crate::richdata::read(&mut cx, &wb_rels) {
        Ok(pictures) => pictures,
        Err(error) => {
            cx.warn(format!("cell-picture data could not be read ({error}); scalar fallbacks were retained"));
            vec![]
        }
    };
    let mut wb = Workbook::new();
    wb.sheets.clear();
    // Extensions of the workbook part Kition does not read, kept as written.
    let raw = match cx.pkg.read(&wb_part)? {
        Some(raw) => Some(raw),
        None => cx.pkg.read("xl/workbook.xml")?,
    };
    wb.extensions.workbook = root_extensions(raw.as_deref(), "workbook");

    // Styles and theme.
    if let Some(r) = find_rel(&wb_rels, "styles").cloned() {
        if let Some(x) = cx.optional_xml(&r.target)? {
            let StylesIn { mut xfs, dxfs, palette, colors, named, named_info, num_fmts, table_styles, xf_complements } = read_styles(&x);
            // Excel's checkboxes: cell formats whose complement leads to a Checkbox bag.
            if xf_complements.iter().any(Option::is_some)
                && let Some(r) = find_rel(&wb_rels, "FeaturePropertyBag").cloned()
                && let Some(bags) = cx.optional_xml(&r.target)?
            {
                let boxes = crate::styles::checkbox_complements(&bags);
                for (s, i) in xfs.iter_mut().zip(&xf_complements) {
                    s.checkbox = i.is_some_and(|i| boxes.contains(&i));
                }
            }
            wb.extensions.styles = root_extensions(cx.pkg.read(&r.target)?.as_deref(), "styleSheet");
            wb.table_styles = table_styles;
            wb.palette = colors;
            cx.num_fmts = num_fmts;
            // The first cell format is the file's Normal style: the default for unstyled cells.
            cx.xf_map = xfs
                .into_iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == 0 {
                        cx.styles.set_default(s);
                        StyleId::DEFAULT
                    } else {
                        cx.styles.intern(s)
                    }
                })
                .collect();
            cx.dxfs = dxfs;
            cx.palette = palette;
            wb.cell_styles = named;
            wb.cell_style_info = named_info;
        }
    } else {
        cx.warn("no styles part");
    }
    if let Some(r) = find_rel(&wb_rels, "theme").cloned()
        && let Some(x) = cx.optional_xml(&r.target)?
    {
        wb.theme = crate::theme::read_theme(&x);
        // Kept only when it holds more than the model writes back.
        wb.theme.raw = crate::theme::keepable(cx.pkg.read(&r.target)?.as_deref()).filter(|raw| *raw != crate::theme::write_theme(&wb.theme));
    }
    // The character unit and the automatic row height come from the Normal style's font.
    {
        let font = &cx.styles.get(StyleId::DEFAULT).font;
        let face = kition_sheet_model::units::face_name(font, &wb.theme).to_owned();
        cx.char_px = kition_sheet_model::units::char_width_px(&face, font.size);
        cx.auto_row_px = (kition_sheet_model::units::default_row_height_pt(font.size) * kition_sheet_model::units::PX_PER_PT) as f32;
    }
    // Shared strings, streamed item by item.
    if let Some(r) = find_rel(&wb_rels, "sharedStrings").cloned() {
        match cx.pkg.read(&r.target)? {
            Some(b) => {
                let mut sst = Vec::new();
                let res = xml::parse_streaming(&b, &["si"], &mut |si| {
                    sst.push(crate::rich::read_item(&si));
                    Ok(())
                });
                if let Err(e) = res {
                    cx.warn(format!("shared strings are damaged ({e}); kept {} strings", sst.len()));
                }
                cx.sst = sst;
            }
            None => cx.warn(format!("missing part {}", r.target)),
        }
    }

    if let Some(r) = find_rel(&wb_rels, "person").cloned()
        && let Ok(Some(x)) = cx.pkg.read_xml(&r.target)
    {
        for p in x.kids("person") {
            if let (Some(id), Some(n)) = (p.attr("id"), p.attr("displayName")) {
                cx.persons.insert(id.to_string(), n.to_string());
            }
        }
    }

    // Workbook properties.
    if let Some(p) = wb_xml.child("workbookPr")
        && p.flag("date1904", false)
    {
        cx.date_system = DateSystem::D1904;
    }
    wb.date_system = cx.date_system;
    if let Some(c) = wb_xml.child("calcPr") {
        wb.calc.mode = match c.attr("calcMode") {
            Some("manual") => CalcMode::Manual,
            Some("autoNoTable") => CalcMode::AutomaticExceptTables,
            _ => CalcMode::Automatic,
        };
        wb.calc.iterative = c.flag("iterate", false);
        if let Some(n) = c.attr_u32("iterateCount") {
            wb.calc.max_iterations = n.clamp(1, 32767);
        }
        if let Some(d) = c.attr_f64("iterateDelta") {
            wb.calc.max_change = d.abs();
        }
        wb.calc.precision_as_displayed = !c.flag("fullPrecision", true);
        wb.calc.full_calc_on_load = c.flag("fullCalcOnLoad", false);
    }
    if let Some(p) = wb_xml.child("workbookProtection") {
        wb.protected_structure = p.flag("lockStructure", false);
    }
    let active_tab = wb_xml.path(&["bookViews", "workbookView"]).and_then(|v| v.attr_u32("activeTab")).unwrap_or(0) as usize;

    // Sheets. `file_to_model[i]` maps the i-th <sheet> to a model sheet index.
    let mut file_to_model: Vec<Option<usize>> = vec![];
    // Each model sheet's worksheet part (for the identity part's fingerprints).
    let mut sheet_parts: Vec<Option<String>> = vec![];
    let mut pivot_caches = crate::pivot::CachesIn::new(&wb_xml, &wb_rels);
    let sheet_els: Vec<El> = wb_xml.child("sheets").map(|s| s.kids("sheet").cloned().collect()).unwrap_or_default();
    for (i, se) in sheet_els.iter().enumerate() {
        let name = se.attr("name").unwrap_or("").to_string();
        let rel = se.attr("id").and_then(|id| wb_rels.iter().find(|r| r.id == id)).cloned();
        let Some(rel) = rel else {
            cx.warn(format!("sheet \"{name}\" has no part; skipped"));
            file_to_model.push(None);
            continue;
        };
        if rel.kind != "worksheet" {
            cx.warn(format!("sheet \"{name}\" is a {} and was skipped", rel.kind));
            file_to_model.push(None);
            continue;
        }
        let name = unique_name(&wb, &name, i);
        let mut sheet = match crate::sheet_read::read_sheet(&mut cx, &rel.target, &name) {
            Ok(s) => s,
            Err(IoError::TooLarge(m)) => return Err(IoError::TooLarge(m)),
            Err(e) => {
                cx.warn(format!("sheet \"{name}\" could not be read: {e}"));
                Sheet::new(name.clone())
            }
        };
        match crate::pivot::read_pivots(&mut cx, &mut pivot_caches, &rel.target, &name) {
            Ok(p) => sheet.pivots = p,
            Err(IoError::TooLarge(m)) => return Err(IoError::TooLarge(m)),
            Err(e) => cx.warn(format!("Pivot tables on sheet \"{name}\" could not be read: {e}")),
        }
        // The sheet's own id; one that repeats another's is numbered afresh below.
        sheet.sheet_id = se.attr_u32("sheetId").unwrap_or(0);
        sheet.visibility = match se.attr("state") {
            Some("hidden") => Visibility::Hidden,
            Some("veryHidden") => Visibility::VeryHidden,
            _ => Visibility::Visible,
        };
        file_to_model.push(Some(wb.sheets.len()));
        sheet_parts.push(Some(rel.target.clone()));
        wb.sheets.push(Arc::new(sheet));
    }
    if wb.sheets.is_empty() {
        cx.warn("the workbook has no worksheets; added an empty sheet");
        wb.sheets.push(Arc::new(Sheet::new("Sheet1")));
    }
    wb.number_sheets();
    wb.active_sheet = file_to_model.get(active_tab).copied().flatten().unwrap_or(0);
    wb.view = wb_xml.path(&["bookViews", "workbookView"]).and_then(|v| read_workbook_view(v, &file_to_model));
    wb.file_version = wb_xml.child("fileVersion").and_then(read_file_version);

    // Defined names. Chartex parts refer to their data through hidden `_xlchart.*` names.
    let mut chart_names: Vec<(String, String)> = vec![];
    if let Some(dn) = wb_xml.child("definedNames") {
        for d in dn.kids("definedName") {
            let Some(name) = d.attr("name") else { continue };
            let scope = match d.attr_u32("localSheetId") {
                Some(i) => match file_to_model.get(i as usize).copied().flatten() {
                    Some(m) => Some(m),
                    None => continue,
                },
                None => None,
            };
            let text = d.text.trim();
            let lname = name.to_ascii_lowercase();
            if lname == "_xlnm._filterdatabase" {
                continue;
            }
            if lname.starts_with("_xlchart.") {
                chart_names.push((lname, crate::fmla::from_file(text)));
                continue;
            }
            if lname == "_xlnm.print_area" || lname == "_xlnm.print_titles" {
                if let Some(si) = scope
                    && let Some(sheet) = wb.sheet_mut(si)
                {
                    if lname == "_xlnm.print_area" {
                        sheet.print.print_area = parse_print_area(text);
                    } else {
                        let (rows, cols) = parse_print_titles(text);
                        sheet.print.title_rows = rows;
                        sheet.print.title_cols = cols;
                    }
                }
                continue;
            }
            wb.names.push(DefinedName {
                name: name.to_string(),
                scope,
                formula: crate::fmla::from_file(text),
                comment: d.attr("comment").unwrap_or("").to_string(),
                hidden: d.flag("hidden", false),
            });
        }
    }

    if !chart_names.is_empty() {
        let resolve = |f: &mut String| {
            if let Some((_, r)) = chart_names.iter().find(|(n, _)| f.eq_ignore_ascii_case(n)) {
                *f = r.clone();
            }
        };
        for sheet in wb.sheets.iter_mut().filter(|s| !s.charts.is_empty()) {
            for ch in Arc::make_mut(sheet).charts.iter_mut() {
                for s in ch.series.iter_mut() {
                    resolve(&mut s.values);
                    s.categories.iter_mut().for_each(&resolve);
                    s.name.iter_mut().for_each(&resolve);
                }
            }
        }
    }

    // Document properties.
    if let Some(r) = find_rel(&root_rels, "core-properties").cloned()
        && let Ok(Some(x)) = cx.pkg.read_xml(&r.target)
    {
        let get = |n: &str| x.child(n).map(|e| e.text.trim().to_string()).unwrap_or_default();
        wb.props.title = get("title");
        wb.props.subject = get("subject");
        wb.props.author = get("creator");
        wb.props.keywords = get("keywords");
        wb.props.description = get("description");
        wb.props.category = get("category");
        wb.props.last_modified_by = get("lastModifiedBy");
        wb.props.created = get("created");
        wb.props.modified = get("modified");
    }
    // Custom document properties, verbatim (they name no other part).
    if let Some(r) = find_rel(&root_rels, "custom-properties").cloned()
        && let Ok(Some(b)) = cx.pkg.read_limited(&r.target, 1 << 20)
        && let Ok(text) = String::from_utf8(b)
        && text.contains("Properties")
    {
        wb.props.custom_xml = Some(text);
    }
    if let Some(r) = find_rel(&root_rels, "extended-properties").cloned()
        && let Ok(Some(x)) = cx.pkg.read_xml(&r.target)
    {
        wb.props.company = x.child("Company").map(|e| e.text.trim().to_string()).unwrap_or_default();
    }

    if wb_rels.iter().any(|r| r.kind == "vbaProject") {
        cx.warn("macros (VBA project) are not supported and were dropped");
    }
    if wb_rels.iter().any(|r| r.kind == "externalLink") {
        cx.warn("links to external workbooks are not supported");
    }
    // Calls of LAMBDA names are spelled like the names (the parser upper-cases them).
    wb.apply_name_call_case();
    wb.styles = cx.styles;
    crate::identities::read(&mut wb, &mut cx.pkg, &sheet_parts, &wb_part)?;
    Ok((wb, cx.report))
}

pub(crate) fn unique_name(wb: &Workbook, name: &str, i: usize) -> String {
    let mut base: String = name.chars().filter(|c| !matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']')).take(31).collect();
    if base.trim().is_empty() {
        base = format!("Sheet{}", i + 1);
    }
    if wb.sheet_index(&base).is_none() {
        return base;
    }
    (2..).map(|n| format!("{} ({n})", base.chars().take(25).collect::<String>())).find(|n| wb.sheet_index(n).is_none()).unwrap_or(base)
}

/// Strips a sheet qualifier (`'My Sheet'!$A$1` → `$A$1`).
fn strip_sheet(s: &str) -> &str {
    match s.rfind('!') {
        Some(i) => s.get(i + 1..).unwrap_or(""),
        None => s,
    }
}

pub fn parse_print_area(text: &str) -> Option<RangeRef> {
    let first = text.split(',').next()?;
    let r = strip_sheet(first.trim()).replace('$', "");
    RangeRef::parse(&r)
}

pub fn parse_print_titles(text: &str) -> (Option<(u32, u32)>, Option<(u32, u32)>) {
    let mut rows = None;
    let mut cols = None;
    for part in text.split(',') {
        let p = strip_sheet(part.trim()).replace('$', "");
        let Some((a, b)) = p.split_once(':') else { continue };
        if let (Ok(r0), Ok(r1)) = (a.parse::<u32>(), b.parse::<u32>()) {
            if r0 >= 1 && r1 >= r0 && r1 <= kition_sheet_core::MAX_ROWS {
                rows = Some((r0 - 1, r1 - 1));
            }
        } else if let (Some(c0), Some(c1)) = (kition_sheet_core::letters_to_col(a), kition_sheet_core::letters_to_col(b))
            && c1 >= c0
        {
            cols = Some((c0, c1));
        }
    }
    (rows, cols)
}

/// The workbook window apart from its active tab; `None` when it names nothing else. The first
/// tab shown is mapped to the model's sheets (the first sheet read at or after it).
fn read_workbook_view(v: &El, file_to_model: &[Option<usize>]) -> Option<WorkbookView> {
    let first_sheet = v.attr_u32("firstSheet").and_then(|i| file_to_model.iter().skip(i as usize).find_map(|m| *m)).map(|i| i as u32);
    let view = WorkbookView {
        x: v.attr_i64("xWindow").and_then(|n| i32::try_from(n).ok()),
        y: v.attr_i64("yWindow").and_then(|n| i32::try_from(n).ok()),
        width: v.attr_u32("windowWidth"),
        height: v.attr_u32("windowHeight"),
        tab_ratio: v.attr_u32("tabRatio").map(|n| n.min(1000)),
        first_sheet,
        visibility: v.attr("visibility").filter(|s| matches!(*s, "visible" | "hidden" | "veryHidden")).map(str::to_owned),
        minimized: v.attr_bool("minimized"),
        show_horizontal_scroll: v.attr_bool("showHorizontalScroll"),
        show_vertical_scroll: v.attr_bool("showVerticalScroll"),
        show_sheet_tabs: v.attr_bool("showSheetTabs"),
        auto_filter_date_grouping: v.attr_bool("autoFilterDateGrouping"),
    };
    (view != WorkbookView::default()).then_some(view)
}

/// `fileVersion` as written; `None` when it holds nothing.
fn read_file_version(v: &El) -> Option<FileVersion> {
    let text = |k: &str| v.attr(k).unwrap_or_default().to_owned();
    let version = FileVersion {
        app_name: text("appName"),
        last_edited: text("lastEdited"),
        lowest_edited: text("lowestEdited"),
        rup_build: text("rupBuild"),
        code_name: text("codeName"),
    };
    (version != FileVersion::default()).then_some(version)
}

/// The `ext` items of a part's root extension list (`extLst` directly below `root`).
fn root_extensions(raw: Option<&[u8]>, root: &str) -> Vec<String> {
    let Some(raw) = raw else { return vec![] };
    crate::xml::extension_lists(raw)
        .into_iter()
        .filter(|l| l.path.len() == 1 && l.path.first().is_some_and(|n| n == root))
        .flat_map(|l| l.exts)
        .collect()
}
