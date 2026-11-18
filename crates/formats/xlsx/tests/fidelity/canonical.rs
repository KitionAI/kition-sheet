//! The canonical package comparison of the fidelity scorecard (`tests/fidelity.rs`, Sheet only).
//!
//! The raw package layer counts elements and attributes as each package writes them. A writer
//! that encodes the same workbook differently shows up there as lost features although nothing a
//! reader sees is gone: a shared string table without duplicates, adjacent runs with the same
//! font merged, a style table renumbered and deduplicated, attributes left at their defaults.
//! This module rewrites the original package and the export into one canonical form before they
//! are counted, so that what the canonical comparison still finds lost is lost. Every rewrite is
//! applied to both packages alike, and every rule has a name: the scorecard measures each rule's
//! share of the raw losses by turning that rule off alone.
//!
//! The rules, in the order they run:
//!
//! - `names`: elements and attributes are named by namespace, not by the prefix a package
//!   chose (`ap:Properties` and `Properties` in the extended-properties namespace are one name).
//! - `policy`: what Kition never writes by decision, not by loss: the name and version of the
//!   application that wrote the file (`docProps/app.xml` `Application`, `AppVersion`) and the
//!   author's folder (`x15ac:absPath`, a host path, with the `mc:AlternateContent` around it).
//! - `markup`: processing hints: `mc:Ignorable`, and `xml:space` on text without leading or
//!   trailing whitespace (where it changes nothing).
//! - `defaults`: attributes holding their schema default (`summaryBelow="1"`, `state="visible"`,
//!   `textRotation="0"`, `<b val="1"/>`, …; booleans compared as `0`/`1`). The table is
//!   [`DEFAULTS`]; only defaults ECMA-376 states are listed.
//! - `strings-inline`: inline strings (`c t="inlineStr"` with `is`) are shared strings with the
//!   same content, so they are moved into the shared string table.
//! - `strings-runs`: a shared string's runs by content: a run without properties is plain text,
//!   adjacent runs with equal properties are one run, a string of plain runs is plain text.
//! - `strings-dedupe`: the shared string table as the set of strings the cells use: duplicates
//!   and items no cell names are dropped.
//! - `styles-dedupe`: the style table by content: each cell format (`xf`) is resolved into its
//!   number format code (built-in ids by their codes), font, fill, border, alignment and
//!   protection, and equal formats are one; the font, fill, border and number format lists are
//!   then only what the formats hold. Differential formats (`dxf`) are deduplicated too.
//! - `styles-apply`: an `applyX` flag of a cell format whose component equals the parent cell
//!   style's changes nothing; nor does one whose component equals the default cell format's (the
//!   first), which a writer may make the Normal style itself. A cell style includes every
//!   component unless it says `applyX="0"` (its "Style includes" box cleared, as Excel writes
//!   it), so its `applyX="1"` is the default; the component it leaves out is unused (the flag
//!   itself is compared).
//! - `styles-equivalent`: a border side without a style is no side (its colour unused), nor is a
//!   diagonal while neither `diagonalUp` nor `diagonalDown` is set (it is not drawn), a fill
//!   without a pattern has no colours, a solid fill's background colour is unused (outside
//!   differential formats), and an automatic border colour is system colour 64, as Excel writes
//!   it.
//! - `cells`: a cell's type without a value, formula or inline string means nothing; a cell
//!   format equal to the default format is the default; a cell left with only its reference is no
//!   cell, unless it lies in a styled row or column (where it overrides the line's format). A
//!   row's format applies only with `customFormat="1"`: without it, or when it is the default
//!   format, the row has none.
//! - `cols`: column widths in whole pixels (7 per character, as the model keeps them); a column
//!   at the sheet's default width with nothing else set is no column; adjacent columns with equal
//!   settings are one range.
//! - `empty`: containers whose absence means the same as their empty form (`<sheetPr/>`,
//!   `<selection/>`, `<headerFooter/>`, `<extLst/>`, `<numFmts count="0"/>`, …; [`EMPTY`]).
//!
//! Values are not compared here (the model leaves compare them); like the raw layer, this counts
//! elements and attributes by name.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;

use quick_xml::NsReader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;

/// The rules by name, with what each rewrites, in the order they run.
pub const RULES: &[(&str, &str)] = &[
    ("names", "names by namespace, not by prefix"),
    ("policy", "writer identity and host paths (never written by decision)"),
    ("markup", "mc:Ignorable, xml:space where it changes nothing"),
    ("defaults", "attributes at their schema default"),
    ("strings-inline", "inline strings as shared strings"),
    ("strings-runs", "plain runs as plain text, equal adjacent runs merged"),
    ("strings-dedupe", "duplicate and unused shared strings"),
    ("styles-dedupe", "style table resolved by content and deduplicated"),
    ("styles-apply", "apply flags whose component equals the parent style's"),
    ("styles-equivalent", "unused border colours and fill colours, auto colour as 64"),
    ("cells", "types without values, default formats, empty unstyled cells"),
    ("cols", "column widths in pixels, default-width columns, merged ranges"),
    ("empty", "empty containers"),
];

/// Namespaces by URI and the name the canonical form gives them (`""`: unprefixed).
const NAMESPACES: &[(&str, &str)] = &[
    ("http://schemas.openxmlformats.org/spreadsheetml/2006/main", ""),
    ("http://purl.oclc.org/ooxml/spreadsheetml/main", ""),
    ("http://schemas.openxmlformats.org/package/2006/relationships", ""),
    ("http://schemas.openxmlformats.org/package/2006/content-types", ""),
    ("http://schemas.openxmlformats.org/officeDocument/2006/extended-properties", ""),
    ("http://schemas.openxmlformats.org/officeDocument/2006/custom-properties", ""),
    ("http://schemas.openxmlformats.org/officeDocument/2006/relationships", "r"),
    ("http://purl.oclc.org/ooxml/officeDocument/relationships", "r"),
    ("http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes", "vt"),
    ("http://schemas.openxmlformats.org/package/2006/metadata/core-properties", "cp"),
    ("http://purl.org/dc/elements/1.1/", "dc"),
    ("http://purl.org/dc/terms/", "dcterms"),
    ("http://purl.org/dc/dcmitype/", "dcmitype"),
    ("http://www.w3.org/2001/XMLSchema-instance", "xsi"),
    ("http://schemas.openxmlformats.org/markup-compatibility/2006", "mc"),
    ("http://schemas.openxmlformats.org/drawingml/2006/main", "a"),
    ("http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing", "xdr"),
    ("http://schemas.openxmlformats.org/drawingml/2006/chart", "c"),
    ("http://schemas.microsoft.com/office/spreadsheetml/2009/9/main", "x14"),
    ("http://schemas.microsoft.com/office/spreadsheetml/2010/11/main", "x15"),
    ("http://schemas.microsoft.com/office/spreadsheetml/2010/11/ac", "x15ac"),
    ("http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac", "x14ac"),
    ("http://schemas.microsoft.com/office/excel/2006/main", "xm"),
    ("http://www.w3.org/XML/1998/namespace", "xml"),
];

/// Attribute defaults of SpreadsheetML elements (ECMA-376 Part 1, §18), by element: an attribute
/// holding its default is the attribute left out.
pub const DEFAULTS: &[(&str, &[(&str, &str)])] = &[
    (
        "sheetPr",
        &[
            ("syncHorizontal", "0"),
            ("syncVertical", "0"),
            ("transitionEvaluation", "0"),
            ("transitionEntry", "0"),
            ("published", "1"),
            ("filterMode", "0"),
            ("enableFormatConditionsCalculation", "1"),
        ],
    ),
    ("outlinePr", &[("applyStyles", "0"), ("summaryBelow", "1"), ("summaryRight", "1"), ("showOutlineSymbols", "1")]),
    ("pageSetUpPr", &[("autoPageBreaks", "1"), ("fitToPage", "0")]),
    (
        "sheetView",
        &[
            ("windowProtection", "0"),
            ("showFormulas", "0"),
            ("showGridLines", "1"),
            ("showRowColHeaders", "1"),
            ("showZeros", "1"),
            ("rightToLeft", "0"),
            ("tabSelected", "0"),
            ("showRuler", "1"),
            ("showOutlineSymbols", "1"),
            ("defaultGridColor", "1"),
            ("showWhiteSpace", "1"),
            ("view", "normal"),
            ("colorId", "64"),
            ("zoomScale", "100"),
            ("zoomScaleNormal", "0"),
            ("zoomScaleSheetLayoutView", "0"),
            ("zoomScalePageLayoutView", "0"),
        ],
    ),
    ("pane", &[("xSplit", "0"), ("ySplit", "0"), ("activePane", "topLeft"), ("state", "split")]),
    ("selection", &[("pane", "topLeft"), ("activeCellId", "0"), ("sqref", "A1")]),
    (
        "sheetFormatPr",
        &[
            ("baseColWidth", "8"),
            ("customHeight", "0"),
            ("zeroHeight", "0"),
            ("thickTop", "0"),
            ("thickBottom", "0"),
            ("outlineLevelRow", "0"),
            ("outlineLevelCol", "0"),
        ],
    ),
    ("col", &[("style", "0"), ("hidden", "0"), ("bestFit", "0"), ("customWidth", "0"), ("phonetic", "0"), ("outlineLevel", "0"), ("collapsed", "0")]),
    (
        "row",
        &[
            ("hidden", "0"),
            ("customHeight", "0"),
            ("customFormat", "0"),
            ("outlineLevel", "0"),
            ("collapsed", "0"),
            ("thickTop", "0"),
            ("thickBot", "0"),
            ("ph", "0"),
        ],
    ),
    ("c", &[("s", "0"), ("t", "n"), ("cm", "0"), ("vm", "0"), ("ph", "0")]),
    ("f", &[("t", "normal"), ("aca", "0"), ("dt2D", "0"), ("dtr", "0"), ("del1", "0"), ("del2", "0"), ("ca", "0"), ("bx", "0")]),
    (
        "dataValidation",
        &[
            ("type", "none"),
            ("errorStyle", "stop"),
            ("imeMode", "noControl"),
            ("operator", "between"),
            ("allowBlank", "0"),
            ("showDropDown", "0"),
            ("showInputMessage", "0"),
            ("showErrorMessage", "0"),
        ],
    ),
    ("dataValidations", &[("disablePrompts", "0")]),
    ("printOptions", &[("horizontalCentered", "0"), ("verticalCentered", "0"), ("headings", "0"), ("gridLines", "0"), ("gridLinesSet", "1")]),
    (
        "pageSetup",
        &[
            ("paperSize", "1"),
            ("scale", "100"),
            ("firstPageNumber", "1"),
            ("fitToWidth", "1"),
            ("fitToHeight", "1"),
            ("pageOrder", "downThenOver"),
            ("orientation", "default"),
            ("usePrinterDefaults", "1"),
            ("blackAndWhite", "0"),
            ("draft", "0"),
            ("cellComments", "none"),
            ("useFirstPageNumber", "0"),
            ("errors", "displayed"),
            ("horizontalDpi", "600"),
            ("verticalDpi", "600"),
            ("copies", "1"),
        ],
    ),
    ("headerFooter", &[("differentOddEven", "0"), ("differentFirst", "0"), ("scaleWithDoc", "1"), ("alignWithMargins", "1")]),
    (
        "sheetProtection",
        &[
            ("sheet", "0"),
            ("objects", "0"),
            ("scenarios", "0"),
            ("formatCells", "1"),
            ("formatColumns", "1"),
            ("formatRows", "1"),
            ("insertColumns", "1"),
            ("insertRows", "1"),
            ("insertHyperlinks", "1"),
            ("deleteColumns", "1"),
            ("deleteRows", "1"),
            ("selectLockedCells", "0"),
            ("sort", "1"),
            ("autoFilter", "1"),
            ("pivotTables", "1"),
            ("selectUnlockedCells", "0"),
        ],
    ),
    ("workbookProtection", &[("lockStructure", "0"), ("lockWindows", "0"), ("lockRevision", "0")]),
    (
        "workbookPr",
        &[
            ("date1904", "0"),
            ("showObjects", "all"),
            ("showBorderUnselectedTables", "1"),
            ("filterPrivacy", "0"),
            ("promptedSolutions", "0"),
            ("showInkAnnotation", "1"),
            ("backupFile", "0"),
            ("saveExternalLinkValues", "1"),
            ("updateLinks", "userSet"),
            ("hidePivotFieldList", "0"),
            ("showPivotChartFilter", "0"),
            ("allowRefreshQuery", "0"),
            ("publishItems", "0"),
            ("checkCompatibility", "0"),
            ("autoCompressPictures", "1"),
            ("refreshAllConnections", "0"),
        ],
    ),
    (
        "workbookView",
        &[
            ("visibility", "visible"),
            ("minimized", "0"),
            ("showHorizontalScroll", "1"),
            ("showVerticalScroll", "1"),
            ("showSheetTabs", "1"),
            ("tabRatio", "600"),
            ("firstSheet", "0"),
            ("activeTab", "0"),
            ("autoFilterDateGrouping", "1"),
        ],
    ),
    ("sheet", &[("state", "visible")]),
    (
        "calcPr",
        &[
            ("calcMode", "auto"),
            ("fullCalcOnLoad", "0"),
            ("refMode", "A1"),
            ("iterate", "0"),
            ("iterateCount", "100"),
            ("iterateDelta", "0.001"),
            ("fullPrecision", "1"),
            ("calcCompleted", "1"),
            ("calcOnSave", "1"),
            ("concurrentCalc", "1"),
            ("forceFullCalc", "0"),
        ],
    ),
    ("definedName", &[("hidden", "0"), ("function", "0"), ("vbProcedure", "0"), ("xlm", "0"), ("publishToServer", "0"), ("workbookParameter", "0")]),
    ("cellStyle", &[("hidden", "0"), ("customBuiltin", "0")]),
    ("xf", &[("quotePrefix", "0"), ("pivotButton", "0"), ("xfId", "0")]),
    (
        "alignment",
        &[
            ("horizontal", "general"),
            ("vertical", "bottom"),
            ("textRotation", "0"),
            ("wrapText", "0"),
            ("indent", "0"),
            ("relativeIndent", "0"),
            ("justifyLastLine", "0"),
            ("shrinkToFit", "0"),
            ("readingOrder", "0"),
        ],
    ),
    ("protection", &[("locked", "1"), ("hidden", "0")]),
    ("b", &[("val", "1")]),
    ("i", &[("val", "1")]),
    ("strike", &[("val", "1")]),
    ("condense", &[("val", "1")]),
    ("extend", &[("val", "1")]),
    ("outline", &[("val", "1")]),
    ("shadow", &[("val", "1")]),
    ("u", &[("val", "single")]),
    ("border", &[("diagonalUp", "0"), ("diagonalDown", "0"), ("outline", "1")]),
    ("gradientFill", &[("type", "linear"), ("degree", "0"), ("left", "0"), ("right", "0"), ("top", "0"), ("bottom", "0")]),
    (
        "table",
        &[("headerRowCount", "1"), ("totalsRowCount", "0"), ("totalsRowShown", "1"), ("insertRow", "0"), ("insertRowShift", "0"), ("published", "0")],
    ),
    ("cfRule", &[("stopIfTrue", "0"), ("aboveAverage", "1"), ("percent", "0"), ("bottom", "0"), ("equalAverage", "0")]),
];

/// Elements whose empty form (no children, no text, no attribute but a zero `count`) means the
/// same as no element.
pub const EMPTY: &[&str] = &[
    "sheetPr",
    "pageSetUpPr",
    "outlinePr",
    "selection",
    "headerFooter",
    "extLst",
    "a:extLst",
    "definedNames",
    "workbookProtection",
    "numFmts",
    "dxfs",
    "mergeCells",
    "hyperlinks",
    "dataValidations",
    "cols",
    "rowBreaks",
    "colBreaks",
    "printOptions",
    "pageSetup",
    "alignment",
    "protection",
    "rPr",
];

/// Built-in number formats every locale shares (ECMA-376 Part 1, §18.8.30).
const BUILTIN_FORMATS: &[(u32, &str)] = &[
    (0, "General"),
    (1, "0"),
    (2, "0.00"),
    (3, "#,##0"),
    (4, "#,##0.00"),
    (9, "0%"),
    (10, "0.00%"),
    (11, "0.00E+00"),
    (12, "# ?/?"),
    (13, "# ??/??"),
    (14, "mm-dd-yy"),
    (15, "d-mmm-yy"),
    (16, "d-mmm"),
    (17, "mmm-yy"),
    (18, "h:mm AM/PM"),
    (19, "h:mm:ss AM/PM"),
    (20, "h:mm"),
    (21, "h:mm:ss"),
    (22, "m/d/yy h:mm"),
    (37, "#,##0 ;(#,##0)"),
    (38, "#,##0 ;[Red](#,##0)"),
    (39, "#,##0.00;(#,##0.00)"),
    (40, "#,##0.00;[Red](#,##0.00)"),
    (45, "mm:ss"),
    (46, "[h]:mm:ss"),
    (47, "mmss.0"),
    (48, "##0.0E+0"),
    (49, "@"),
];

/// An element: its canonical name, the name as written, attributes, children and text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub raw: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    pub text: String,
}

/// An attribute: its canonical name, the name as written and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Attr {
    pub name: String,
    pub raw: String,
    pub value: String,
}

impl Node {
    /// A new element in the main namespace (written unprefixed).
    pub fn new(name: &str) -> Node {
        Node { name: name.to_owned(), raw: name.to_owned(), ..Default::default() }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.name == name).map(|a| a.value.as_str())
    }

    pub fn set_attr(&mut self, name: &str, value: &str) {
        match self.attrs.iter_mut().find(|a| a.name == name) {
            Some(a) => a.value = value.to_owned(),
            None => self.attrs.push(Attr { name: name.to_owned(), raw: name.to_owned(), value: value.to_owned() }),
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|a| a.name != name);
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    fn child_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.children.iter_mut().find(|c| c.name == name)
    }

    /// Whether the element is empty: no children, no text, no attribute but a zero `count`.
    fn is_empty(&self) -> bool {
        self.children.is_empty() && self.text.trim().is_empty() && self.attrs.iter().all(|a| a.name == "count" && a.value.trim() == "0")
    }

    /// The element's content as one string, attributes sorted; children in order, or sorted
    /// when `ordered` is false.
    pub fn key(&self, ordered: bool) -> String {
        let mut attrs: Vec<String> = self.attrs.iter().map(|a| format!("{}={:?}", a.name, a.value)).collect();
        attrs.sort();
        let mut children: Vec<String> = self.children.iter().map(|c| c.key(ordered)).collect();
        if !ordered {
            children.sort();
        }
        format!("<{} {}>{:?}{}</>", self.name, attrs.join(" "), self.text, children.concat())
    }

    fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Node)) {
        f(self);
        for child in &mut self.children {
            child.walk_mut(f);
        }
    }
}

/// A part: its name, and its tree when it is XML (`.xml`, `.rels`, `.vml`).
pub type Part = (String, Option<Node>);

/// The parts of a package, XML parts parsed.
pub fn read(package: &[u8]) -> Result<Vec<Part>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".xml") || lower.ends_with(".rels") || lower.ends_with(".vml")) {
            out.push((name, None));
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        out.push((name, Some(parse(&bytes))));
    }
    Ok(out)
}

fn prefix_of(uri: &[u8]) -> Option<&'static str> {
    NAMESPACES.iter().find(|(u, _)| u.as_bytes() == uri).map(|(_, p)| *p)
}

fn canonical_name(uri: Option<&[u8]>, local: &[u8], raw: &str) -> String {
    match uri.and_then(prefix_of) {
        Some("") => String::from_utf8_lossy(local).into_owned(),
        Some(prefix) => format!("{prefix}:{}", String::from_utf8_lossy(local)),
        None => raw.to_owned(),
    }
}

fn element(reader: &NsReader<&[u8]>, uri: Option<Vec<u8>>, e: &BytesStart<'_>) -> Node {
    let raw = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let name = canonical_name(uri.as_deref(), e.local_name().as_ref(), &raw);
    let mut node = Node { name, raw, ..Default::default() };
    for attr in e.attributes().with_checks(false).flatten() {
        let key = attr.key.as_ref();
        if key == b"xmlns" || key.starts_with(b"xmlns:") {
            continue;
        }
        let raw = String::from_utf8_lossy(key).into_owned();
        let (resolved, local) = reader.resolve_attribute(attr.key);
        let uri = match resolved {
            ResolveResult::Bound(ns) => Some(ns.as_ref().to_vec()),
            _ => None,
        };
        let name = canonical_name(uri.as_deref(), local.as_ref(), &raw);
        let value = attr.unescape_value().map(|v| v.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&attr.value).into_owned());
        node.attrs.push(Attr { name, raw, value });
    }
    node
}

/// Parses XML into a document node (no name) holding the root, leniently: mismatched end tags
/// close the innermost element, and a parse error ends the document where it is.
pub fn parse(bytes: &[u8]) -> Node {
    let mut reader = NsReader::from_reader(bytes);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut stack = vec![Node::default()];
    loop {
        let (uri, event) = match reader.read_resolved_event_into(&mut buf) {
            Ok((ResolveResult::Bound(ns), event)) => (Some(ns.as_ref().to_vec()), event),
            Ok((_, event)) => (None, event),
            Err(_) => break,
        };
        match event {
            Event::Start(e) => {
                let node = element(&reader, uri, &e);
                stack.push(node);
            }
            Event::Empty(e) => {
                let node = element(&reader, uri, &e);
                if let Some(top) = stack.last_mut() {
                    top.children.push(node);
                }
            }
            Event::End(_) => {
                if stack.len() > 1
                    && let Some(node) = stack.pop()
                    && let Some(top) = stack.last_mut()
                {
                    top.children.push(node);
                }
            }
            Event::Text(t) => {
                if let (Some(top), Ok(text)) = (stack.last_mut(), t.decode()) {
                    top.text.push_str(&text);
                }
            }
            Event::CData(t) => {
                if let (Some(top), Ok(text)) = (stack.last_mut(), t.decode()) {
                    top.text.push_str(&text);
                }
            }
            Event::GeneralRef(r) => {
                let c = match r.resolve_char_ref() {
                    Ok(Some(c)) => Some(c),
                    _ => match r.decode().as_deref() {
                        Ok("amp") => Some('&'),
                        Ok("lt") => Some('<'),
                        Ok("gt") => Some('>'),
                        Ok("quot") => Some('"'),
                        Ok("apos") => Some('\''),
                        _ => None,
                    },
                };
                if let (Some(top), Some(c)) = (stack.last_mut(), c) {
                    top.text.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    while stack.len() > 1 {
        if let Some(node) = stack.pop()
            && let Some(top) = stack.last_mut()
        {
            top.children.push(node);
        }
    }
    stack.pop().unwrap_or_default()
}

/// Rewrites parsed parts into the canonical form, every rule on except `off` (for measuring that
/// rule's share). `names` is applied when the parts are counted ([`count`]).
pub fn canonicalize(parts: &mut Vec<Part>, off: Option<&str>) {
    let on = |rule: &str| off != Some(rule);
    if on("policy") {
        for (name, tree) in parts.iter_mut() {
            if let Some(tree) = tree {
                policy(name, tree);
            }
        }
    }
    if on("markup") {
        for tree in parts.iter_mut().filter_map(|(_, t)| t.as_mut()) {
            markup(tree);
        }
    }
    if on("defaults") {
        for tree in parts.iter_mut().filter_map(|(_, t)| t.as_mut()) {
            defaults(tree);
        }
    }
    strings(parts, on("strings-inline"), on("strings-runs"), on("strings-dedupe"));
    let formats = styles(parts, on("styles-dedupe"), on("styles-apply"), on("styles-equivalent"));
    for (name, tree) in parts.iter_mut() {
        if !is_worksheet(name) {
            continue;
        }
        let Some(tree) = tree else { continue };
        if on("cells") {
            cells(tree, &formats);
        }
        if on("cols") {
            cols(tree);
        }
    }
    if on("empty") {
        for tree in parts.iter_mut().filter_map(|(_, t)| t.as_mut()) {
            empty(tree);
        }
    }
}

/// Counts a tree's elements and attributes (`element`, `element@attribute`, and `rel:<type>` for
/// relationships), by canonical name, or by the name as written when `raw_names`.
pub fn count(tree: &Node, raw_names: bool, counts: &mut BTreeMap<String, u64>) {
    for child in &tree.children {
        count_element(child, raw_names, counts);
    }
}

fn count_element(node: &Node, raw_names: bool, counts: &mut BTreeMap<String, u64>) {
    let element = if raw_names { &node.raw } else { &node.name };
    for attr in &node.attrs {
        let key = if raw_names { &attr.raw } else { &attr.name };
        if node.name == "Relationship" && attr.name == "Type" {
            let tail = attr.value.rsplit('/').next().unwrap_or_default();
            *counts.entry(format!("rel:{tail}")).or_default() += 1;
        }
        *counts.entry(format!("{element}@{key}")).or_default() += 1;
    }
    *counts.entry(element.clone()).or_default() += 1;
    for child in &node.children {
        count_element(child, raw_names, counts);
    }
}

fn is_worksheet(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("xl/worksheets/") && lower.ends_with(".xml") && !lower.contains("/_rels/")
}

fn root_mut(tree: &mut Node) -> Option<&mut Node> {
    tree.children.first_mut()
}

// ---- policy, markup, defaults, empty ----------------------------------------------------------

fn policy(part: &str, tree: &mut Node) {
    if part.eq_ignore_ascii_case("docProps/app.xml")
        && let Some(root) = root_mut(tree)
    {
        root.children.retain(|c| c.name != "Application" && c.name != "AppVersion");
    }
    fn strip(node: &mut Node) {
        node.children.retain(|c| c.name != "x15ac:absPath");
        for child in &mut node.children {
            strip(child);
        }
        // An alternate content block left without choices.
        for child in &mut node.children {
            if child.name == "mc:AlternateContent" {
                child.children.retain(|c| !(matches!(c.name.as_str(), "mc:Choice" | "mc:Fallback") && c.children.is_empty()));
            }
        }
        node.children.retain(|c| !(c.name == "mc:AlternateContent" && c.children.is_empty()));
    }
    strip(tree);
}

fn markup(tree: &mut Node) {
    tree.walk_mut(&mut |node| {
        node.remove_attr("mc:Ignorable");
        if node.attr("xml:space").is_some() && !needs_space(&node.text) {
            node.remove_attr("xml:space");
        }
    });
}

/// Whether text keeps something only `xml:space="preserve"` keeps: leading or trailing
/// whitespace.
fn needs_space(text: &str) -> bool {
    text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace)
}

fn boolean(value: &str) -> &str {
    match value.trim() {
        "true" | "on" => "1",
        "false" | "off" => "0",
        v => v,
    }
}

fn defaults(tree: &mut Node) {
    let table: HashMap<&str, &[(&str, &str)]> = DEFAULTS.iter().copied().collect();
    tree.walk_mut(&mut |node| {
        if let Some(list) = table.get(node.name.as_str()) {
            node.attrs.retain(|a| !list.iter().any(|(name, default)| a.name == *name && boolean(&a.value) == *default));
        }
    });
}

fn empty(node: &mut Node) {
    for child in &mut node.children {
        empty(child);
    }
    node.children.retain(|c| !(EMPTY.contains(&c.name.as_str()) && c.is_empty()));
}

// ---- strings ---------------------------------------------------------------------------------

fn is_shared_strings(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with("sharedstrings.xml")
}

fn strings(parts: &mut Vec<Part>, inline: bool, runs: bool, dedupe: bool) {
    let mut items: Vec<Node> = parts
        .iter()
        .find(|(n, _)| is_shared_strings(n))
        .and_then(|(_, t)| t.as_ref())
        .and_then(|t| t.children.first())
        .map(|sst| sst.children.clone())
        .unwrap_or_default();
    let mut used = BTreeSet::new();
    let mut moved = false;
    for (name, tree) in parts.iter_mut() {
        if !is_worksheet(name) {
            continue;
        }
        let Some(tree) = tree else { continue };
        tree.walk_mut(&mut |node| {
            if node.name != "c" {
                return;
            }
            if inline
                && node.attr("t") == Some("inlineStr")
                && let Some(at) = node.children.iter().position(|c| c.name == "is")
            {
                let mut item = node.children.remove(at);
                item.name = "si".into();
                item.raw = "si".into();
                let mut v = Node::new("v");
                v.text = items.len().to_string();
                items.push(item);
                node.children.retain(|c| c.name != "v");
                node.children.push(v);
                node.set_attr("t", "s");
                moved = true;
            }
            if node.attr("t") == Some("s")
                && let Some(i) = node.child("v").and_then(|v| v.text.trim().parse::<usize>().ok())
            {
                used.insert(i);
            }
        });
    }
    if !(moved || runs || dedupe) {
        return;
    }
    if runs {
        for item in &mut items {
            canonical_runs(item);
        }
    }
    if dedupe {
        let mut seen = BTreeSet::new();
        items = items.into_iter().enumerate().filter(|(i, item)| used.contains(i) && seen.insert(item.key(true))).map(|(_, item)| item).collect();
    }
    match parts.iter_mut().find(|(n, _)| is_shared_strings(n)) {
        Some((_, Some(tree))) => {
            if let Some(sst) = root_mut(tree) {
                sst.children = items;
            }
        }
        _ if !items.is_empty() => {
            let mut sst = Node::new("sst");
            sst.children = items;
            let mut doc = Node::default();
            doc.children.push(sst);
            parts.push(("xl/sharedStrings.xml".into(), Some(doc)));
        }
        _ => {}
    }
}

/// A string item's runs by content: plain runs as plain text, equal adjacent runs merged, a
/// string of plain runs as one `t`.
fn canonical_runs(item: &mut Node) {
    let mut segments: Vec<(Option<Node>, String)> = Vec::new();
    let mut others = Vec::new();
    for child in std::mem::take(&mut item.children) {
        match child.name.as_str() {
            "t" => segments.push((None, child.text)),
            "r" => {
                let props = child.child("rPr").filter(|p| !p.children.is_empty() || !p.attrs.is_empty()).cloned();
                let text: String = child.children.iter().filter(|c| c.name == "t").map(|c| c.text.as_str()).collect();
                segments.push((props, text));
            }
            _ => others.push(child),
        }
    }
    let mut merged: Vec<(Option<Node>, String)> = Vec::new();
    for (props, text) in segments {
        if text.is_empty() {
            continue;
        }
        match merged.last_mut() {
            Some((last, so_far)) if last.as_ref().map(|p| p.key(false)) == props.as_ref().map(|p| p.key(false)) => so_far.push_str(&text),
            _ => merged.push((props, text)),
        }
    }
    let text_node = |text: String| {
        let mut t = Node::new("t");
        if needs_space(&text) {
            t.set_attr("xml:space", "preserve");
        }
        t.text = text;
        t
    };
    if merged.iter().all(|(props, _)| props.is_none()) {
        let text: String = merged.into_iter().map(|(_, t)| t).collect();
        item.children.push(text_node(text));
    } else {
        for (props, text) in merged {
            let mut run = Node::new("r");
            if let Some(props) = props {
                run.children.push(props);
            }
            run.children.push(text_node(text));
            item.children.push(run);
        }
    }
    item.children.extend(others);
}

// ---- styles ----------------------------------------------------------------------------------

/// What the cell rules need from the style table: for each cell format, whether it formats
/// cells as the default format does.
#[derive(Default)]
pub struct Formats {
    pub like_default: Vec<bool>,
}

impl Formats {
    fn is_default(&self, index: Option<&str>) -> bool {
        match index.map(|s| s.trim().parse::<usize>()) {
            None => true,
            Some(Ok(i)) => self.like_default.get(i).copied().unwrap_or(false),
            Some(Err(_)) => false,
        }
    }
}

const COMPONENTS: &[(&str, &str)] = &[
    ("applyNumberFormat", "numFmt"),
    ("applyFont", "font"),
    ("applyFill", "fill"),
    ("applyBorder", "border"),
    ("applyAlignment", "alignment"),
    ("applyProtection", "protection"),
];

fn list(root: &Node, name: &str, item: &str) -> Vec<Node> {
    root.child(name).map(|l| l.children.iter().filter(|c| c.name == item).cloned().collect()).unwrap_or_default()
}

/// A fill or border by what it shows (rule `styles-equivalent`); `differential` for a `dxf`'s,
/// where a solid fill's colour is its background colour.
fn equivalent(node: &mut Node, differential: bool) {
    match node.name.as_str() {
        "fill" => {
            if let Some(pattern) = node.child_mut("patternFill") {
                // Outside differential formats a pattern fill without a type has none.
                match pattern.attr("patternType") {
                    None | Some("none") if !differential => {
                        pattern.children.clear();
                        pattern.remove_attr("patternType");
                    }
                    Some("none") => pattern.children.clear(),
                    Some("solid") if !differential => pattern.children.retain(|c| c.name != "bgColor"),
                    _ => {}
                }
            }
        }
        "border" => {
            let diagonal = ["diagonalUp", "diagonalDown"].iter().any(|d| node.attr(d).map(boolean) == Some("1"));
            node.children.retain(|side| match side.name.as_str() {
                "diagonal" if !diagonal => false,
                "left" | "right" | "top" | "bottom" | "diagonal" | "start" | "end" | "vertical" | "horizontal" => {
                    side.attr("style").is_some_and(|s| s != "none")
                }
                _ => true,
            });
            for side in &mut node.children {
                for color in side.children.iter_mut().filter(|c| c.name == "color") {
                    if color.attr("auto").map(boolean) == Some("1") {
                        color.attrs.clear();
                        color.set_attr("indexed", "64");
                    }
                }
            }
        }
        _ => {}
    }
}

/// Resolves the cell format `xf` into one element holding its components.
fn resolve(xf: &Node, formats: &HashMap<u32, String>, fonts: &[Node], fills: &[Node], borders: &[Node]) -> Node {
    let id = |name: &str| xf.attr(name).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
    let mut out = Node { name: xf.name.clone(), raw: xf.raw.clone(), ..Default::default() };
    out.attrs = xf.attrs.iter().filter(|a| !matches!(a.name.as_str(), "numFmtId" | "fontId" | "fillId" | "borderId")).cloned().collect();
    let format = id("numFmtId") as u32;
    let code = formats
        .get(&format)
        .cloned()
        .or_else(|| BUILTIN_FORMATS.iter().find(|(i, _)| *i == format).map(|(_, c)| (*c).to_owned()))
        .unwrap_or_else(|| format!("#{format}"));
    if code != "General" {
        let mut n = Node::new("numFmt");
        n.set_attr("formatCode", &code);
        out.children.push(n);
    }
    for (list, index) in [(fonts, id("fontId")), (fills, id("fillId")), (borders, id("borderId"))] {
        if let Some(item) = list.get(index) {
            out.children.push(item.clone());
        }
    }
    out.children.extend(xf.children.iter().cloned());
    out
}

fn component_key(xf: &Node, component: &str) -> String {
    xf.child(component).map(|c| c.key(false)).unwrap_or_default()
}

fn styles(parts: &mut [Part], dedupe: bool, apply: bool, equivalents: bool) -> Formats {
    let Some((_, Some(tree))) = parts.iter_mut().find(|(n, _)| n.to_ascii_lowercase().ends_with("xl/styles.xml")) else {
        return Formats::default();
    };
    let Some(root) = root_mut(tree) else { return Formats::default() };
    let formats: HashMap<u32, String> = list(root, "numFmts", "numFmt")
        .iter()
        .filter_map(|n| Some((n.attr("numFmtId")?.trim().parse().ok()?, n.attr("formatCode")?.to_owned())))
        .collect();
    let fonts = list(root, "fonts", "font");
    let mut fills = list(root, "fills", "fill");
    let mut borders = list(root, "borders", "border");
    let (mut fills_seen, mut borders_seen) = (fills.clone(), borders.clone());
    for item in fills_seen.iter_mut().chain(borders_seen.iter_mut()) {
        equivalent(item, false);
    }
    if equivalents {
        fills.clone_from(&fills_seen);
        borders.clone_from(&borders_seen);
    }
    let style_xfs = list(root, "cellStyleXfs", "xf");
    let cell_xfs = list(root, "cellXfs", "xf");
    let mut resolved_styles: Vec<Node> = style_xfs.iter().map(|xf| resolve(xf, &formats, &fonts, &fills, &borders)).collect();
    let mut resolved_cells: Vec<Node> = cell_xfs.iter().map(|xf| resolve(xf, &formats, &fonts, &fills, &borders)).collect();
    // Which formats format cells as the default one does (applying the equivalences, whether or
    // not the rule rewrites the part).
    let seen_key = |xf: &Node| {
        let mut r = resolve(xf, &formats, &fonts, &fills_seen, &borders_seen);
        r.attrs.retain(|a| !a.name.starts_with("apply") && a.name != "xfId");
        r.key(false)
    };
    let default_key = cell_xfs.first().map(seen_key);
    let like_default = cell_xfs.iter().map(|xf| Some(seen_key(xf)) == default_key).collect();

    if apply {
        // What a cell style leaves out is unused.
        for xf in &mut resolved_styles {
            for (flag, component) in COMPONENTS {
                if xf.attr(flag).map(boolean) == Some("0") {
                    xf.children.retain(|c| c.name != *component);
                }
            }
        }
        let normal = resolved_styles.first().cloned();
        let first = resolved_cells.first().cloned();
        for xf in &mut resolved_styles {
            if let Some(normal) = &normal {
                drop_applied(xf, normal, true);
            }
        }
        for (xf, raw) in resolved_cells.iter_mut().zip(&cell_xfs) {
            let parent = raw.attr("xfId").and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
            if let Some(parent) = resolved_styles.get(parent) {
                let parent = parent.clone();
                drop_applied(xf, &parent, false);
            }
            if let Some(first) = &first {
                drop_applied(xf, first, false);
            }
        }
    }
    if dedupe {
        let dedup = |items: Vec<Node>| {
            let mut seen = BTreeSet::new();
            items.into_iter().filter(|n| seen.insert(n.key(false))).collect::<Vec<_>>()
        };
        let mut dxfs = list(root, "dxfs", "dxf");
        if equivalents {
            for dxf in &mut dxfs {
                for c in &mut dxf.children {
                    equivalent(c, true);
                }
            }
        }
        let dxfs = dedup(dxfs);
        let styles_out = dedup(resolved_styles);
        let cells_out = dedup(resolved_cells);
        root.children.retain(|c| !matches!(c.name.as_str(), "numFmts" | "fonts" | "fills" | "borders"));
        for (name, items) in [("cellStyleXfs", styles_out), ("cellXfs", cells_out), ("dxfs", dxfs)] {
            if let Some(l) = root.child_mut(name) {
                l.children = items;
            }
        }
    } else {
        // The rules that do not resolve the table rewrite it in place.
        if apply {
            let strip = |name: &str, resolved: &[Node], root: &mut Node| {
                if let Some(l) = root.child_mut(name) {
                    for (xf, r) in l.children.iter_mut().filter(|c| c.name == "xf").zip(resolved) {
                        xf.attrs.retain(|a| !a.name.starts_with("apply") || r.attr(&a.name).is_some());
                    }
                }
            };
            strip("cellStyleXfs", &resolved_styles, root);
            strip("cellXfs", &resolved_cells, root);
        }
        if equivalents {
            for (name, items) in [("fills", &fills), ("borders", &borders)] {
                if let Some(l) = root.child_mut(name) {
                    l.children = items.clone();
                }
            }
            if let Some(dxfs) = root.child_mut("dxfs") {
                for dxf in &mut dxfs.children {
                    for c in &mut dxf.children {
                        equivalent(c, true);
                    }
                }
            }
        }
    }
    Formats { like_default }
}

/// Drops the `applyX` flags of `xf` whose component equals `parent`'s; for a cell style
/// (`style`), the flags saying `1`, its default (a `0` says the style leaves the component out).
fn drop_applied(xf: &mut Node, parent: &Node, style: bool) {
    for (flag, component) in COMPONENTS {
        let Some(value) = xf.attr(flag) else { continue };
        if style {
            if boolean(value) == "1" {
                xf.remove_attr(flag);
            }
            continue;
        }
        if component_key(xf, component) == component_key(parent, component) {
            xf.remove_attr(flag);
        }
    }
}

// ---- cells and columns -----------------------------------------------------------------------

fn cells(tree: &mut Node, formats: &Formats) {
    let Some(root) = root_mut(tree) else { return };
    // Styled columns: (first, last) 1-based.
    let mut styled_cols = Vec::new();
    if let Some(cols) = root.child("cols") {
        for col in cols.children.iter().filter(|c| c.name == "col") {
            if !formats.is_default(col.attr("style")) {
                let at = |n: &str| col.attr(n).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
                styled_cols.push((at("min"), at("max")));
            }
        }
    }
    let Some(data) = root.child_mut("sheetData") else { return };
    for row in data.children.iter_mut().filter(|r| r.name == "row") {
        let row_styled = row.attr("customFormat").map(boolean) == Some("1") && !formats.is_default(row.attr("s"));
        if !row_styled {
            row.remove_attr("s");
            row.remove_attr("customFormat");
        }
        for cell in row.children.iter_mut().filter(|c| c.name == "c") {
            if !cell.children.iter().any(|c| matches!(c.name.as_str(), "v" | "f" | "is")) {
                cell.remove_attr("t");
            }
            if cell.attr("s").is_some() && formats.is_default(cell.attr("s")) {
                cell.remove_attr("s");
            }
        }
        row.children.retain(|cell| {
            if cell.name != "c" || !cell.children.is_empty() || cell.attrs.iter().any(|a| a.name != "r") || row_styled {
                return true;
            }
            let col = cell.attr("r").map(column_of).unwrap_or(0);
            styled_cols.iter().any(|(a, b)| (*a..=*b).contains(&col))
        });
    }
}

/// The 1-based column of an A1 reference.
fn column_of(reference: &str) -> u32 {
    reference
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .fold(0u32, |n, c| n.saturating_mul(26).saturating_add(u32::from(c.to_ascii_uppercase()) - 64))
}

/// A width in characters as whole pixels at 7 pixels per character.
fn pixels(width: &str) -> Option<i64> {
    width.trim().parse::<f64>().ok().filter(|w| w.is_finite()).map(|w| (w * 7.0).round() as i64)
}

fn cols(tree: &mut Node) {
    let Some(root) = root_mut(tree) else { return };
    let default = root.child("sheetFormatPr").and_then(|f| f.attr("defaultColWidth")).and_then(pixels);
    let Some(cols) = root.child_mut("cols") else { return };
    let mut out: Vec<Node> = Vec::new();
    for mut col in std::mem::take(&mut cols.children) {
        if col.name != "col" {
            out.push(col);
            continue;
        }
        let width = col.attr("width").and_then(pixels);
        if let Some(w) = width {
            col.set_attr("width", &w.to_string());
        }
        let only_width = col.attrs.iter().all(|a| matches!(a.name.as_str(), "min" | "max" | "width" | "customWidth"));
        if only_width && default.is_some() && width == default {
            continue;
        }
        let settings = |c: &Node| {
            let mut s: Vec<String> =
                c.attrs.iter().filter(|a| a.name != "min" && a.name != "max").map(|a| format!("{}={}", a.name, a.value)).collect();
            s.sort();
            s
        };
        let bound = |c: &Node, n: &str| c.attr(n).and_then(|v| v.trim().parse::<u64>().ok());
        if let Some(last) = out.last_mut()
            && last.name == "col"
            && settings(last) == settings(&col)
            && bound(last, "max").map(|m| m + 1) == bound(&col, "min")
            && let Some(max) = col.attr("max").map(str::to_owned)
        {
            last.set_attr("max", &max);
            continue;
        }
        out.push(col);
    }
    cols.children = out;
}

// ---- tests -----------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

    fn counts(parts: &[Part]) -> BTreeMap<String, BTreeMap<String, u64>> {
        let mut out = BTreeMap::new();
        for (name, tree) in parts {
            if let Some(tree) = tree {
                count(tree, false, out.entry(name.clone()).or_default());
            }
        }
        out
    }

    fn package(parts: &[(&str, String)]) -> Vec<Part> {
        parts.iter().map(|(n, x)| ((*n).to_owned(), Some(parse(x.as_bytes())))).collect()
    }

    fn sheet(rows: &str) -> String {
        format!("<worksheet xmlns=\"{MAIN}\"><sheetData>{rows}</sheetData></worksheet>")
    }

    fn sst(items: &str) -> String {
        format!("<sst xmlns=\"{MAIN}\">{items}</sst>")
    }

    /// Canonical counts of two packages.
    fn both(a: &[(&str, String)], b: &[(&str, String)]) -> (BTreeMap<String, BTreeMap<String, u64>>, BTreeMap<String, BTreeMap<String, u64>>) {
        let (mut a, mut b) = (package(a), package(b));
        canonicalize(&mut a, None);
        canonicalize(&mut b, None);
        (counts(&a), counts(&b))
    }

    #[test]
    fn names_are_compared_by_namespace() {
        let a = parse(b"<ap:Properties xmlns:ap=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><ap:Company>x</ap:Company></ap:Properties>");
        let b = parse(
            b"<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Company>x</Company></Properties>",
        );
        let (mut ca, mut cb) = (BTreeMap::new(), BTreeMap::new());
        count(&a, false, &mut ca);
        count(&b, false, &mut cb);
        assert_eq!(ca, cb);
        let mut raw = BTreeMap::new();
        count(&a, true, &mut raw);
        assert!(raw.contains_key("ap:Properties"));
    }

    #[test]
    fn shared_strings_compare_by_content() {
        // Original: a duplicate item, an unused item, a run without properties, two adjacent runs
        // with the same font, xml:space on text without edge spaces, and an inline string.
        let rpr = "<rPr><b/><sz val=\"11\"/></rPr>";
        let original = [
            (
                "xl/sharedStrings.xml",
                sst(&format!(
                    "<si><t xml:space=\"preserve\">a</t></si><si><t>a</t></si><si><t>unused</t></si><si><r><t>plain</t></r></si><si><r>{rpr}<t>bo</t></r><r>{rpr}<t>ld</t></r><r><t> x</t></r></si>"
                )),
            ),
            (
                "xl/worksheets/sheet1.xml",
                sheet(
                    "<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c><c r=\"C1\" t=\"s\"><v>3</v></c><c r=\"D1\" t=\"s\"><v>4</v></c><c r=\"E1\" t=\"inlineStr\"><is><t>inline</t></is></c></row>",
                ),
            ),
        ];
        let exported = [
            (
                "xl/sharedStrings.xml",
                sst(&format!(
                    "<si><t>a</t></si><si><t>plain</t></si><si><r>{rpr}<t>bold</t></r><r><t xml:space=\"preserve\"> x</t></r></si><si><t>inline</t></si>"
                )),
            ),
            (
                "xl/worksheets/sheet1.xml",
                sheet(
                    "<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>0</v></c><c r=\"C1\" t=\"s\"><v>1</v></c><c r=\"D1\" t=\"s\"><v>2</v></c><c r=\"E1\" t=\"s\"><v>3</v></c></row>",
                ),
            ),
        ];
        let (a, b) = both(&original, &exported);
        assert_eq!(a, b);
    }

    #[test]
    fn a_lost_run_property_is_still_lost() {
        let original = [
            ("xl/sharedStrings.xml", sst("<si><r><rPr><b/><color rgb=\"FFFF0000\"/></rPr><t>x</t></r></si>")),
            ("xl/worksheets/sheet1.xml", sheet("<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row>")),
        ];
        let exported = [
            ("xl/sharedStrings.xml", sst("<si><r><rPr><b/></rPr><t>x</t></r></si>")),
            ("xl/worksheets/sheet1.xml", sheet("<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row>")),
        ];
        let (a, b) = both(&original, &exported);
        assert_eq!(a["xl/sharedStrings.xml"].get("color"), Some(&1));
        assert_eq!(b["xl/sharedStrings.xml"].get("color"), None);
    }

    fn style_sheet(body: &str) -> String {
        format!(
            "<styleSheet xmlns=\"{MAIN}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"x14ac\">{body}</styleSheet>"
        )
    }

    #[test]
    fn style_tables_compare_by_content() {
        // Original: a duplicate font, a duplicate cell format, a custom id for a built-in code,
        // border sides with style none and a colour, a solid fill's background colour, defaults
        // and apply flags on components equal to the parent's.
        let original = style_sheet(concat!(
            "<numFmts count=\"1\"><numFmt numFmtId=\"176\" formatCode=\"@\"/></numFmts>",
            "<fonts count=\"3\"><font><sz val=\"11\"/><name val=\"Arial\"/></font><font><b val=\"1\"/><sz val=\"11\"/><name val=\"Arial\"/></font><font><sz val=\"11\"/><name val=\"Arial\"/></font></fonts>",
            "<fills count=\"3\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFFFFF00\"/><bgColor indexed=\"64\"/></patternFill></fill></fills>",
            "<borders count=\"2\"><border><left style=\"none\"><color rgb=\"00000000\"/></left><right/><top/><bottom/><diagonal/></border><border><left style=\"thin\"><color auto=\"1\"/></left></border></borders>",
            "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
            "<cellXfs count=\"4\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\" pivotButton=\"0\"/><xf numFmtId=\"176\" fontId=\"1\" fillId=\"2\" borderId=\"1\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"><alignment textRotation=\"0\" wrapText=\"1\"/></xf><xf numFmtId=\"0\" fontId=\"2\" fillId=\"0\" borderId=\"0\" applyFont=\"1\"/><xf numFmtId=\"176\" fontId=\"1\" fillId=\"2\" borderId=\"1\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"><alignment wrapText=\"1\"/></xf></cellXfs>",
            "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\" hidden=\"0\"/></cellStyles>"
        ));
        let exported = style_sheet(concat!(
            "<fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Arial\"/></font><font><b/><sz val=\"11\"/><name val=\"Arial\"/></font></fonts>",
            "<fills count=\"3\"><fill><patternFill/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFFFFF00\"/></patternFill></fill></fills>",
            "<borders count=\"2\"><border><left/><right/><top/><bottom/><diagonal/></border><border><left style=\"thin\"><color indexed=\"64\"/></left><right/><top/><bottom/><diagonal/></border></borders>",
            "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
            "<cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"49\" fontId=\"1\" fillId=\"2\" borderId=\"1\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\" applyAlignment=\"1\"><alignment wrapText=\"1\"/></xf></cellXfs>",
            "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>"
        ));
        let (a, b) = both(&[("xl/styles.xml", original)], &[("xl/styles.xml", exported)]);
        // The export adds `applyAlignment` (an addition, not a loss); everything else is equal.
        let mut b = b;
        if let Some(c) = b.get_mut("xl/styles.xml") {
            c.remove("xf@applyAlignment");
        }
        assert_eq!(a, b);
    }

    #[test]
    fn a_lost_font_property_is_still_lost() {
        let original = style_sheet(
            "<fonts count=\"1\"><font><sz val=\"11\"/><charset val=\"134\"/></font></fonts><cellXfs count=\"1\"><xf fontId=\"0\"/></cellXfs>",
        );
        let exported = style_sheet("<fonts count=\"1\"><font><sz val=\"11\"/></font></fonts><cellXfs count=\"1\"><xf fontId=\"0\"/></cellXfs>");
        let (a, b) = both(&[("xl/styles.xml", original)], &[("xl/styles.xml", exported)]);
        assert_eq!(a["xl/styles.xml"].get("charset"), Some(&1));
        assert_eq!(b["xl/styles.xml"].get("charset"), None);
    }

    #[test]
    fn valueless_default_cells_are_no_cells_outside_styled_lines() {
        let styles = style_sheet("<fonts><font/><font><b/></font></fonts><cellXfs><xf/><xf fontId=\"1\"/><xf fontId=\"0\"/></cellXfs>");
        let original = [
            ("xl/styles.xml", styles.clone()),
            (
                "xl/worksheets/sheet1.xml",
                format!(
                    "<worksheet xmlns=\"{MAIN}\"><cols><col min=\"3\" max=\"3\" width=\"9\" style=\"1\"/></cols><sheetData><row r=\"1\"><c r=\"A1\" s=\"0\" t=\"s\"/><c r=\"B1\" s=\"2\"><v>1</v></c><c r=\"C1\" s=\"0\"/><c r=\"D1\" s=\"1\" t=\"s\"/></row><row r=\"2\" s=\"1\" customFormat=\"1\"><c r=\"A2\" s=\"0\"/></row></sheetData></worksheet>"
                ),
            ),
        ];
        let exported = [
            ("xl/styles.xml", styles),
            (
                "xl/worksheets/sheet1.xml",
                format!(
                    "<worksheet xmlns=\"{MAIN}\"><cols><col min=\"3\" max=\"3\" width=\"9\" style=\"1\"/></cols><sheetData><row r=\"1\"><c r=\"B1\"><v>1</v></c><c r=\"C1\"/><c r=\"D1\" s=\"1\"/></row><row r=\"2\" s=\"1\" customFormat=\"1\"><c r=\"A2\"/></row></sheetData></worksheet>"
                ),
            ),
        ];
        let (a, b) = both(&original, &exported);
        assert_eq!(a, b);
        // A cell dropped from a styled row is a loss.
        let lost = [exported[0].clone(), ("xl/worksheets/sheet1.xml", exported[1].1.replace("<c r=\"A2\"/>", ""))];
        let (a, c) = both(&original, &lost);
        assert!(c["xl/worksheets/sheet1.xml"]["c"] < a["xl/worksheets/sheet1.xml"]["c"]);
    }

    #[test]
    fn defaults_empty_containers_and_policy() {
        let original = [
            (
                "xl/worksheets/sheet1.xml",
                format!(
                    "<worksheet xmlns=\"{MAIN}\"><sheetPr><outlinePr summaryBelow=\"1\" summaryRight=\"true\"/><pageSetUpPr/></sheetPr><sheetViews><sheetView showGridLines=\"1\" workbookViewId=\"0\"><selection/></sheetView></sheetViews><sheetFormatPr baseColWidth=\"8\" defaultRowHeight=\"15\"/><cols><col min=\"1\" max=\"1\" width=\"9\" customWidth=\"1\"/><col min=\"2\" max=\"2\" width=\"20.01\" customWidth=\"1\"/><col min=\"3\" max=\"3\" width=\"20\" customWidth=\"1\"/></cols><sheetData/><headerFooter/></worksheet>"
                ),
            ),
            ("docProps/app.xml", "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Some app</Application><AppVersion>1.0</AppVersion><Company/></Properties>".to_owned()),
        ];
        let exported = [
            (
                "xl/worksheets/sheet1.xml",
                format!(
                    "<worksheet xmlns=\"{MAIN}\"><sheetViews><sheetView workbookViewId=\"0\"/></sheetViews><sheetFormatPr defaultRowHeight=\"15\"/><cols><col min=\"1\" max=\"1\" width=\"9\" customWidth=\"1\"/><col min=\"2\" max=\"3\" width=\"20\" customWidth=\"1\"/></cols><sheetData/></worksheet>"
                ),
            ),
            ("docProps/app.xml", "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Kition</Application><Company/></Properties>".to_owned()),
        ];
        let (a, b) = both(&original, &exported);
        assert_eq!(a, b);
        // The author's folder is never written.
        let workbook = format!(
            "<workbook xmlns=\"{MAIN}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:AlternateContent><mc:Choice Requires=\"x15\"><x15ac:absPath xmlns:x15ac=\"http://schemas.microsoft.com/office/spreadsheetml/2010/11/ac\" url=\"C:\\\\folder\\\\\"/></mc:Choice></mc:AlternateContent><sheets/></workbook>"
        );
        let (a, b) = both(&[("xl/workbook.xml", workbook)], &[("xl/workbook.xml", format!("<workbook xmlns=\"{MAIN}\"><sheets/></workbook>"))]);
        assert_eq!(a, b);
    }

    #[test]
    fn every_rule_can_be_turned_off_alone() {
        let original = [
            ("xl/sharedStrings.xml", sst("<si><t>a</t></si><si><t>a</t></si>")),
            ("xl/worksheets/sheet1.xml", sheet("<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c></row>")),
        ];
        for (rule, _) in RULES {
            let mut parts = package(&original);
            canonicalize(&mut parts, Some(rule));
            let c = counts(&parts);
            let expected = if *rule == "strings-dedupe" { 2 } else { 1 };
            assert_eq!(c["xl/sharedStrings.xml"]["si"], expected, "{rule}");
        }
    }
}
