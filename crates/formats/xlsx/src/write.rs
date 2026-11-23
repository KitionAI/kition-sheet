//! Writing a workbook as an XLSX package.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;

use kition_sheet_core::RangeRef;
use kition_sheet_formula::quote_sheet;
use kition_sheet_model::{CalcMode, RichText, Style, Visibility, Workbook};

use crate::IoError;
use crate::package::ZipOut;
use crate::xml::{esc, esc_attr};

pub const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

pub const CT_WORKSHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
pub const CT_DRAWING: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
pub const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
pub const CT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
pub const CT_COMMENTS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";

/// A part's relationships.
#[derive(Default)]
pub struct Rels {
    items: Vec<(String, String, String, bool)>,
}

impl Rels {
    /// Adds a relationship of type `kind` (last segment, e.g. `worksheet`); returns its id.
    pub fn add(&mut self, kind: &str, target: &str) -> String {
        let id = format!("rId{}", self.items.len() + 1);
        let ty = match kind {
            "core-properties" => "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties".to_string(),
            k => format!("{NS_REL}/{k}"),
        };
        self.items.push((id.clone(), ty, target.to_string(), false));
        id
    }
    pub fn add_external(&mut self, kind: &str, target: &str) -> String {
        let id = format!("rId{}", self.items.len() + 1);
        self.items.push((id.clone(), format!("{NS_REL}/{kind}"), target.to_string(), true));
        id
    }
    /// Adds a Microsoft extension relationship with its complete namespace URI.
    pub fn add_uri(&mut self, ty: &str, target: &str) -> String {
        let id = format!("rId{}", self.items.len() + 1);
        self.items.push((id.clone(), ty.to_string(), target.to_string(), false));
        id
    }
    /// Adds a relationship with a given id (a copied part's own, which its markup names).
    pub fn add_as(&mut self, id: &str, ty: &str, target: &str) {
        self.items.push((id.to_string(), ty.to_string(), target.to_string(), false));
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn xml(&self) -> String {
        let mut s = format!("{XML_DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (id, ty, target, ext) in &self.items {
            let _ = write!(
                s,
                "<Relationship Id=\"{id}\" Type=\"{ty}\" Target=\"{}\"{}/>",
                esc_attr(target),
                if *ext { " TargetMode=\"External\"" } else { "" }
            );
        }
        s.push_str("</Relationships>");
        s
    }
}

/// Shared string table being built: one item per distinct text, and per distinct rich text
/// (a rich text and the same text plain are different items).
#[derive(Default)]
pub struct Sst {
    list: Vec<(String, Option<Arc<RichText>>)>,
    map: HashMap<String, u32>,
    /// Rich items by their text: the indexes of the items with that text.
    rich: HashMap<String, Vec<u32>>,
    pub count: u64,
}

impl Sst {
    pub fn index(&mut self, s: &str) -> u32 {
        self.count += 1;
        if let Some(&i) = self.map.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push((s.to_string(), None));
        self.map.insert(s.to_string(), i);
        i
    }

    /// The item of `s` with the runs `rich` (plain when `None` or when they do not spell `s`).
    pub fn index_rich(&mut self, s: &str, rich: Option<&Arc<RichText>>) -> u32 {
        let Some(rich) = rich.filter(|r| r.spells(s)) else { return self.index(s) };
        self.count += 1;
        let same = self.rich.get(s).and_then(|items| {
            items
                .iter()
                .copied()
                .find(|&i| self.list.get(i as usize).and_then(|(_, r)| r.as_ref()).is_some_and(|r| Arc::ptr_eq(r, rich) || **r == **rich))
        });
        if let Some(i) = same {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push((s.to_string(), Some(rich.clone())));
        self.rich.entry(s.to_string()).or_default().push(i);
        i
    }
}

/// Package being assembled.
pub struct Out {
    parts: Vec<(String, Vec<u8>)>,
    overrides: Vec<(String, String)>,
    defaults: BTreeMap<String, String>,
    pub dxfs: Vec<Style>,
    pub sst: Sst,
    pub images: u32,
    pub charts: u32,
    /// References of chartex parts, defined as hidden `_xlchart.v1.N` names.
    pub chart_names: Vec<String>,
    pub drawings: u32,
    /// Smart graphics written so far.
    pub diagrams: u32,
    pub tables: u32,
    pub comments: u32,
    /// Form controls' properties parts written so far.
    pub ctrl_props: u32,
    pub table_id: u32,
    pub dynamic_arrays: bool,
    pub needs_calc: bool,
    pub cell_pictures: crate::richdata::PicturesOut,
}

impl Out {
    /// The bytes of a part written so far.
    pub(crate) fn part_bytes(&self, name: &str) -> Option<&[u8]> {
        self.parts.iter().find(|(n, _)| n == name).map(|(_, data)| data.as_slice())
    }
    pub fn part(&mut self, name: &str, content_type: Option<&str>, data: Vec<u8>) {
        if let Some(ct) = content_type {
            self.overrides.push((format!("/{name}"), ct.to_string()));
        }
        self.parts.push((name.to_string(), data));
    }
    pub fn default_type(&mut self, ext: &str, ct: &str) {
        self.defaults.entry(ext.to_string()).or_insert_with(|| ct.to_string());
    }
}

/// `$A$1:$D$20` (or `$1:$3` / `$A:$C`).
pub fn abs_range(r: &RangeRef) -> String {
    let c = |col: u32| kition_sheet_core::col_to_letters(col);
    if r.is_full_rows() && !r.is_full_cols() {
        return format!("${}:${}", r.start.row + 1, r.end.row + 1);
    }
    if r.is_full_cols() && !r.is_full_rows() {
        return format!("${}:${}", c(r.start.col), c(r.end.col));
    }
    if r.is_single() {
        return format!("${}${}", c(r.start.col), r.start.row + 1);
    }
    format!("${}${}:${}${}", c(r.start.col), r.start.row + 1, c(r.end.col), r.end.row + 1)
}

pub fn write_xlsx(wb: &Workbook) -> Result<Vec<u8>, IoError> {
    write_package(wb, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml")
}

/// The workbook as an Excel template (`.xltx`): the same package with the template's main part.
pub fn write_xltx(wb: &Workbook) -> Result<Vec<u8>, IoError> {
    write_package(wb, "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml")
}

fn write_package(wb: &Workbook, main: &str) -> Result<Vec<u8>, IoError> {
    if wb.sheets.is_empty() {
        return Err(IoError::Format("a workbook needs at least one sheet".into()));
    }
    if wb.sheets.iter().any(|sheet| sheet.cell_pictures.keys().any(|c| sheet.cell(*c).is_some_and(|cell| cell.formula.is_some()))) {
        return Err(IoError::Format("a static cell picture cannot also contain a formula".into()));
    }
    let mut out = Out {
        parts: vec![],
        overrides: vec![],
        defaults: BTreeMap::new(),
        dxfs: vec![],
        sst: Sst::default(),
        images: 0,
        charts: 0,
        chart_names: vec![],
        drawings: 0,
        diagrams: 0,
        tables: 0,
        comments: 0,
        ctrl_props: 0,
        table_id: 0,
        dynamic_arrays: false,
        needs_calc: false,
        cell_pictures: Default::default(),
    };
    out.default_type("rels", "application/vnd.openxmlformats-package.relationships+xml");
    out.default_type("xml", "application/xml");

    // Which sheet is active (must be visible), and at least one visible sheet.
    let any_visible = wb.sheets.iter().any(|s| s.visibility == Visibility::Visible);
    let active = if wb.sheets.get(wb.active_sheet).is_some_and(|s| s.visibility == Visibility::Visible) {
        wb.active_sheet
    } else {
        wb.sheets.iter().position(|s| s.visibility == Visibility::Visible).unwrap_or(0)
    };

    let mut wb_rels = Rels::default();
    let mut sheet_parts = Vec::with_capacity(wb.sheets.len());
    let mut pivots = crate::pivot::PivotWriter::default();
    let mut sheets_xml = String::new();
    // Each sheet's own id (`Sheet::sheet_id`), so it survives the file.
    let sheet_ids = wb.numbered_sheet_ids();
    for (i, sheet) in wb.sheets.iter().enumerate() {
        let (xml, mut rels) = crate::sheet_write::write_sheet(wb, i, i == active, &mut out)?;
        pivots.add_sheet(wb, i, &mut out, &mut rels);
        let name = format!("xl/worksheets/sheet{}.xml", i + 1);
        out.part(&name, Some(CT_WORKSHEET), xml.into_bytes());
        sheet_parts.push(name);
        if !rels.is_empty() {
            out.part(&format!("xl/worksheets/_rels/sheet{}.xml.rels", i + 1), None, rels.xml().into_bytes());
        }
        let rid = wb_rels.add("worksheet", &format!("worksheets/sheet{}.xml", i + 1));
        let state = match sheet.visibility {
            Visibility::Visible => "",
            _ if !any_visible && i == active => "",
            Visibility::Hidden => " state=\"hidden\"",
            Visibility::VeryHidden => " state=\"veryHidden\"",
        };
        let name: String = sheet.name.chars().take(31).collect();
        let _ =
            write!(sheets_xml, "<sheet name=\"{}\" sheetId=\"{}\"{state} r:id=\"{rid}\"/>", esc_attr(&name), sheet_ids.get(i).copied().unwrap_or(1));
    }

    // Defined names, then print areas and titles.
    let mut names = String::new();
    let mut seen = std::collections::HashSet::new();
    for n in &wb.names {
        if n.name.is_empty() || !seen.insert((n.name.to_ascii_lowercase(), n.scope)) {
            continue;
        }
        if let Some(sc) = n.scope
            && sc >= wb.sheets.len()
        {
            continue;
        }
        let _ = write!(names, "<definedName name=\"{}\"", esc_attr(&n.name));
        if let Some(sc) = n.scope {
            let _ = write!(names, " localSheetId=\"{sc}\"");
        }
        if n.hidden {
            names.push_str(" hidden=\"1\"");
        }
        if !n.comment.is_empty() {
            let _ = write!(names, " comment=\"{}\"", esc_attr(&n.comment));
        }
        let _ = write!(names, ">{}</definedName>", esc(&crate::fmla::text_to_file(&n.formula)));
    }
    for (i, f) in out.chart_names.iter().enumerate() {
        let _ = write!(
            names,
            "<definedName name=\"{}{i}\" hidden=\"1\">{}</definedName>",
            crate::chartex::NAME_PREFIX,
            esc(&crate::fmla::text_to_file(f))
        );
    }
    for (i, sheet) in wb.sheets.iter().enumerate() {
        let q = quote_sheet(&sheet.name);
        // The sheet's filter range, as Excel names it (hidden) beside the `autoFilter`.
        if let Some(af) = &sheet.autofilter {
            let _ = write!(
                names,
                "<definedName name=\"_xlnm._FilterDatabase\" localSheetId=\"{i}\" hidden=\"1\">{}!{}</definedName>",
                esc(&q),
                abs_range(&af.range)
            );
        }
        if let Some(pa) = &sheet.print.print_area {
            let _ = write!(names, "<definedName name=\"_xlnm.Print_Area\" localSheetId=\"{i}\">{}!{}</definedName>", esc(&q), abs_range(pa));
        }
        let mut titles = vec![];
        if let Some((c0, c1)) = sheet.print.title_cols {
            titles.push(format!("{q}!{}", abs_range(&RangeRef::cols(c0.min(c1), c0.max(c1)))));
        }
        if let Some((r0, r1)) = sheet.print.title_rows {
            titles.push(format!("{q}!{}", abs_range(&RangeRef::rows(r0.min(r1), r0.max(r1)))));
        }
        if !titles.is_empty() {
            let _ = write!(names, "<definedName name=\"_xlnm.Print_Titles\" localSheetId=\"{i}\">{}</definedName>", esc(&titles.join(",")));
        }
    }

    let mut w = format!("{XML_DECL}<workbook xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\">");
    if let Some(v) = &wb.file_version {
        w.push_str(&file_version_xml(v));
    }
    w.push_str(if wb.date_system == kition_sheet_core::DateSystem::D1904 { "<workbookPr date1904=\"1\"/>" } else { "<workbookPr/>" });
    if wb.protected_structure {
        w.push_str("<workbookProtection lockStructure=\"1\"/>");
    }
    let _ = write!(w, "<bookViews>{}</bookViews><sheets>{sheets_xml}</sheets>", workbook_view_xml(wb.view.as_ref(), active, wb.sheets.len()));
    if !names.is_empty() {
        let _ = write!(w, "<definedNames>{names}</definedNames>");
    }
    w.push_str("<calcPr calcId=\"191029\"");
    match wb.calc.mode {
        CalcMode::Automatic => {}
        CalcMode::Manual => w.push_str(" calcMode=\"manual\""),
        CalcMode::AutomaticExceptTables => w.push_str(" calcMode=\"autoNoTable\""),
    }
    if wb.calc.iterative {
        let _ = write!(w, " iterate=\"1\" iterateCount=\"{}\" iterateDelta=\"{}\"", wb.calc.max_iterations, crate::xml::num(wb.calc.max_change));
    }
    if wb.calc.precision_as_displayed {
        w.push_str(" fullPrecision=\"0\"");
    }
    if out.needs_calc || wb.calc.full_calc_on_load {
        w.push_str(" fullCalcOnLoad=\"1\"");
    }
    w.push_str("/>");
    w.push_str(&pivots.workbook_xml(&mut wb_rels));
    if !wb.extensions.workbook.is_empty() {
        let _ = write!(w, "<extLst>{}</extLst>", wb.extensions.workbook.concat());
    }
    w.push_str("</workbook>");

    // Workbook-level parts.
    let (styles, fmt_ids) = crate::styles::write_styles(wb, &out.dxfs, &pivots.number_formats(wb));
    pivots.finish(wb, &mut out, &|code: &str| crate::tables::builtin_id(code).or_else(|| fmt_ids.get(code).copied()));
    out.part("xl/styles.xml", Some("application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"), styles.into_bytes());
    wb_rels.add("styles", "styles.xml");
    // Checkboxes (Excel's cell controls): the feature property bags their cell formats point at.
    if wb.styles.iter().any(|(_, st)| st.checkbox) {
        let bags =
            format!("{XML_DECL}<FeaturePropertyBags xmlns=\"{}\">{}</FeaturePropertyBags>", crate::styles::NS_FPB, crate::styles::FPB_CHECKBOX);
        out.part("xl/featurePropertyBag/featurePropertyBag.xml", Some(crate::styles::CT_FPB), bags.into_bytes());
        wb_rels.add_uri(crate::styles::REL_FPB, "featurePropertyBag/featurePropertyBag.xml");
    }
    out.part(
        "xl/theme/theme1.xml",
        Some("application/vnd.openxmlformats-officedocument.theme+xml"),
        crate::theme::theme_part(&wb.theme).into_bytes(),
    );
    wb_rels.add("theme", "theme/theme1.xml");
    let mut sst = format!("{XML_DECL}<sst xmlns=\"{NS_MAIN}\" count=\"{}\" uniqueCount=\"{}\">", out.sst.count, out.sst.list.len());
    for (t, rich) in &out.sst.list {
        sst.push_str("<si>");
        crate::rich::write_item(&mut sst, t, rich.as_deref());
        sst.push_str("</si>");
    }
    sst.push_str("</sst>");
    out.part("xl/sharedStrings.xml", Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"), sst.into_bytes());
    wb_rels.add("sharedStrings", "sharedStrings.xml");
    if !out.cell_pictures.is_empty() {
        crate::richdata::write(&mut out, &mut wb_rels)?;
    } else if out.dynamic_arrays {
        out.part(
            "xl/metadata.xml",
            Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"),
            METADATA.as_bytes().to_vec(),
        );
        wb_rels.add("sheetMetadata", "metadata.xml");
    }
    crate::identities::write(wb, &mut out, &mut wb_rels, &sheet_parts, w.as_bytes())?;
    out.part("xl/workbook.xml", Some(main), w.into_bytes());
    out.part("xl/_rels/workbook.xml.rels", None, wb_rels.xml().into_bytes());

    // Document properties.
    let p = &wb.props;
    let mut core = format!(
        "{XML_DECL}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">"
    );
    for (tag, v) in [
        ("dc:title", &p.title),
        ("dc:subject", &p.subject),
        ("dc:creator", &p.author),
        ("cp:keywords", &p.keywords),
        ("dc:description", &p.description),
        ("cp:lastModifiedBy", &p.last_modified_by),
        ("cp:category", &p.category),
    ] {
        if !v.is_empty() {
            let _ = write!(core, "<{tag}>{}</{tag}>", esc(v));
        }
    }
    for (tag, v) in [("dcterms:created", &p.created), ("dcterms:modified", &p.modified)] {
        if !v.is_empty() {
            let _ = write!(core, "<{tag} xsi:type=\"dcterms:W3CDTF\">{}</{tag}>", esc(v));
        }
    }
    core.push_str("</cp:coreProperties>");
    out.part("docProps/core.xml", Some("application/vnd.openxmlformats-package.core-properties+xml"), core.into_bytes());
    let mut app = String::from(XML_DECL);
    app.push_str("<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"><Application>Kition Sheet</Application>");
    // What the workbook holds, as Excel summarises it: its worksheets by name.
    let n = wb.sheets.len();
    let _ = write!(
        app,
        "<HeadingPairs><vt:vector size=\"2\" baseType=\"variant\"><vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>{n}</vt:i4></vt:variant></vt:vector></HeadingPairs><TitlesOfParts><vt:vector size=\"{n}\" baseType=\"lpstr\">"
    );
    for sheet in &wb.sheets {
        let _ = write!(app, "<vt:lpstr>{}</vt:lpstr>", esc(&sheet.name));
    }
    app.push_str("</vt:vector></TitlesOfParts>");
    if !p.company.is_empty() {
        let _ = write!(app, "<Company>{}</Company>", esc(&p.company));
    }
    app.push_str("</Properties>");
    out.part("docProps/app.xml", Some("application/vnd.openxmlformats-officedocument.extended-properties+xml"), app.into_bytes());
    let mut root = Rels::default();
    root.add("officeDocument", "xl/workbook.xml");
    root.add("core-properties", "docProps/core.xml");
    root.add("extended-properties", "docProps/app.xml");
    if let Some(custom) = p.custom_xml.as_deref().filter(|c| !c.is_empty()) {
        out.part("docProps/custom.xml", Some("application/vnd.openxmlformats-officedocument.custom-properties+xml"), custom.as_bytes().to_vec());
        root.add("custom-properties", "docProps/custom.xml");
    }
    out.part("_rels/.rels", None, root.xml().into_bytes());

    // Content types first, then the parts in creation order.
    let mut ct = format!("{XML_DECL}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    for (ext, t) in &out.defaults {
        let _ = write!(ct, "<Default Extension=\"{ext}\" ContentType=\"{t}\"/>");
    }
    for (part, t) in &out.overrides {
        let _ = write!(ct, "<Override PartName=\"{}\" ContentType=\"{t}\"/>", esc_attr(part));
    }
    ct.push_str("</Types>");
    let mut zip = ZipOut::new();
    zip.add("[Content_Types].xml", ct.as_bytes())?;
    // Conventional order: rels, docProps, workbook, the rest.
    let rank = |n: &str| match n {
        "_rels/.rels" => 0,
        n if n.starts_with("docProps/") => 1,
        "xl/workbook.xml" => 2,
        "xl/_rels/workbook.xml.rels" => 3,
        _ => 4,
    };
    let mut parts = std::mem::take(&mut out.parts);
    parts.sort_by_key(|(n, _)| rank(n));
    for (name, data) in &parts {
        zip.add(name, data)?;
    }
    zip.finish()
}

/// Cell metadata declaring dynamic-array formulas (MS-XLSX `XLDAPR`).
const METADATA: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<metadata xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\"><metadataTypes count=\"1\"><metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/></metadataTypes><futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata><cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata></metadata>";

/// `fileVersion` as the file gave it.
fn file_version_xml(v: &kition_sheet_model::FileVersion) -> String {
    let mut s = String::from("<fileVersion");
    for (name, value) in [
        ("appName", &v.app_name),
        ("lastEdited", &v.last_edited),
        ("lowestEdited", &v.lowest_edited),
        ("rupBuild", &v.rup_build),
        ("codeName", &v.code_name),
    ] {
        if !value.is_empty() {
            let _ = write!(s, " {name}=\"{}\"", esc_attr(value));
        }
    }
    s.push_str("/>");
    s
}

/// The workbook window: the file's own place, size and switches, and the active tab. The first
/// tab shown is kept while it names a sheet at or before the active one.
fn workbook_view_xml(view: Option<&kition_sheet_model::WorkbookView>, active: usize, sheets: usize) -> String {
    let mut s = String::from("<workbookView");
    if let Some(v) = view {
        let flag = |s: &mut String, name: &str, value: Option<bool>| {
            if let Some(b) = value {
                let _ = write!(s, " {name}=\"{}\"", b as u8);
            }
        };
        if let Some(vis) = v.visibility.as_deref().filter(|x| matches!(*x, "visible" | "hidden" | "veryHidden")) {
            let _ = write!(s, " visibility=\"{vis}\"");
        }
        flag(&mut s, "minimized", v.minimized);
        flag(&mut s, "showHorizontalScroll", v.show_horizontal_scroll);
        flag(&mut s, "showVerticalScroll", v.show_vertical_scroll);
        flag(&mut s, "showSheetTabs", v.show_sheet_tabs);
        for (name, value) in [
            ("xWindow", v.x.map(i64::from)),
            ("yWindow", v.y.map(i64::from)),
            ("windowWidth", v.width.map(i64::from)),
            ("windowHeight", v.height.map(i64::from)),
        ] {
            if let Some(n) = value {
                let _ = write!(s, " {name}=\"{n}\"");
            }
        }
        if let Some(r) = v.tab_ratio {
            let _ = write!(s, " tabRatio=\"{}\"", r.min(1000));
        }
        if let Some(first) = v.first_sheet.map(|f| f as usize).filter(|&f| f < sheets) {
            let _ = write!(s, " firstSheet=\"{}\"", first.min(active));
        }
    }
    let _ = write!(s, " activeTab=\"{active}\"");
    if let Some(b) = view.and_then(|v| v.auto_filter_date_grouping) {
        let _ = write!(s, " autoFilterDateGrouping=\"{}\"", b as u8);
    }
    s.push_str("/>");
    s
}
