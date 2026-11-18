//! Format fidelity scorecard (roadmap phase 7).
//!
//! `KITION_SHEET_CORPUS=<dir>`: every Excel package in that directory is read, written and read
//! again, and the test reports
//!
//! - per feature class (values, formulas, cell styles, sheet layout, charts, …), which files keep
//!   it through import → export → import (the two documents' leaves of that class are compared as
//!   multisets of `path=value`, so a moved paragraph is not a loss but a dropped property is), and
//! - what the written package lost against the original package's XML: elements and attributes
//!   counted per part kind (part names with digits folded), relationship types, and binary parts.
//!
//! `KITION_FIDELITY_OUT=<dir>` writes the full report (`sheet-fidelity.txt`, per file and
//! aggregated) and the aggregate scorecard (`sheet-fidelity.json`, no file names) there.
//! `KITION_FIDELITY_BASELINE=<file>` fails when the corpus scores below such a scorecard: fewer
//! files keeping a class, fewer leaves kept, or less XML retained (the non-regression gate).
//! Skipped when the corpus variable is unset. The KDM leg of the product's path (import into a
//! replica, read back) is covered by `kdm-model`'s `tests/roundtrip.rs`. Cells are compared by
//! address with their style resolved, so a renumbered style table is not a loss.
//!
//! The package layer is also compared canonically (`fidelity/canonical.rs`, Sheet only): both
//! packages are rewritten into one canonical form first (shared strings and the style table by
//! content, defaults, empty containers, …), so that what it still finds lost is lost rather than
//! written differently. The scorecard's `xmlCanonical` holds that number and, per rule, how many
//! raw losses the rule accounts for (measured by turning that rule off alone).
//!
//! The scorer below the kit-specific part is the same in Kition Word, Slide and Sheet, but for
//! the canonical comparison's hook (`canonical_features`).

use std::collections::BTreeMap;
use std::io::Read;

use serde_json::{Value, json};

use kition_sheet_model::Workbook;

#[path = "fidelity/canonical.rs"]
mod canonical;

// ---- Kit-specific -------------------------------------------------------------------------------

const KIT: &str = "sheet";
const CORPUS_VAR: &str = "KITION_SHEET_CORPUS";
const EXTENSIONS: &[&str] = &["xlsx", "xlsm", "xltx", "xltm"];
/// A class the self-test\'s document must have.
const PRIMARY_CLASS: &str = "values";

type Model = Workbook;

fn import(bytes: &[u8]) -> Result<Model, String> {
    kition_sheet_xlsx::read_xlsx(bytes).map(|(wb, _)| wb).map_err(|error| error.to_string())
}

fn export(wb: &Model) -> Result<Vec<u8>, String> {
    kition_sheet_xlsx::write_xlsx(wb).map_err(|error| error.to_string())
}

/// The model as JSON, with each sheet's cells by address and every style id replaced by the
/// style it names (the table itself is left out: the writer may number styles differently).
fn model_value(wb: &Model) -> Value {
    let style = |id: kition_sheet_model::StyleId| serde_json::to_value(wb.styles.get(id)).unwrap_or(Value::Null);
    let mut value = serde_json::to_value(wb).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut value {
        map.remove("styles");
    }
    if let Some(sheets) = value.get_mut("sheets").and_then(Value::as_array_mut) {
        for (item, sheet) in sheets.iter_mut().zip(&wb.sheets) {
            let Value::Object(map) = item else { continue };
            let mut cells = serde_json::Map::new();
            for (at, cell) in sheet.cells.iter() {
                let mut v = serde_json::to_value(cell).unwrap_or(Value::Null);
                if let Value::Object(fields) = &mut v {
                    fields.insert("style".into(), style(cell.style));
                }
                cells.insert(at.a1(), v);
            }
            map.insert("cells".into(), Value::Object(cells));
            if let Some(images) = map.get_mut("images").and_then(Value::as_array_mut) {
                for (item, image) in images.iter_mut().zip(&sheet.images) {
                    item["data"] = json!(digest(&image.data));
                }
            }
            for (key, lines) in [("rows", &sheet.rows), ("cols", &sheet.cols)] {
                let mut out = serde_json::Map::new();
                for (index, line) in lines {
                    let mut v = serde_json::to_value(line).unwrap_or(Value::Null);
                    if let (Value::Object(fields), Some(id)) = (&mut v, line.style) {
                        fields.insert("style".into(), style(id));
                    }
                    out.insert(format!("L{index}"), v);
                }
                map.insert(key.into(), Value::Object(out));
            }
        }
    }
    value
}

/// Keys whose values are ids or caches a re-import may renumber without losing anything.
fn ignored(key: &str) -> bool {
    matches!(key, "spill" | "spill_ranges" | "id")
}

/// Whether two numbers of `class` are the same after a round trip: cell values and formulas must
/// be exact; sizes (pixels and points through `f32`) may move by a hundredth.
fn close(class: &str, a: f64, b: f64) -> bool {
    if matches!(class, "values" | "formulas") {
        return a == b;
    }
    (a - b).abs() <= 0.01_f64.max(a.abs() * 1e-4)
}

/// The feature class of a leaf, by its path (`.key` segments, `[]` for array items, `#` for
/// numeric map keys).
fn classify(path: &str) -> &'static str {
    const RULES: &[(&str, &str)] = &[
        (".rich", "rich text"),
        (".formula", "formulas"),
        (".value", "values"),
        (".style.font", "fonts"),
        (".style.fill", "fills"),
        (".style.border", "borders"),
        (".style.numfmt", "number formats"),
        (".style.num_fmt", "number formats"),
        (".style.align", "alignment"),
        (".style", "cell styles"),
        ("cell_styles", "cell styles"),
        ("charts", "charts"),
        ("images", "pictures"),
        ("cell_pictures", "pictures"),
        ("shapes", "shapes"),
        ("sparklines", "sparklines"),
        ("pivots", "pivot tables"),
        ("cond_formats", "conditional formats"),
        ("validations", "data validation"),
        (".tables", "tables"),
        ("autofilter", "tables"),
        ("comments", "comments"),
        ("hyperlinks", "hyperlinks"),
        (".names", "defined names"),
        ("merges", "merges"),
        (".rows", "rows and columns"),
        (".cols", "rows and columns"),
        ("default_", "rows and columns"),
        ("print", "print settings"),
        ("protect", "protection"),
        (".theme", "theme"),
        (".sheets[]", "sheet settings"),
    ];
    let lower = path.to_ascii_lowercase();
    for (needle, class) in RULES {
        if lower.contains(&needle.to_ascii_lowercase()) {
            return class;
        }
    }
    "workbook settings"
}

/// A small workbook exercising most classes, for the self-test.
fn synthetic() -> Model {
    use kition_sheet_core::{CellRef, Value as CellValue};
    use kition_sheet_model::Cell;
    let mut wb = Workbook::new();
    if let Some(sheet) = wb.sheets.first_mut().map(std::sync::Arc::make_mut) {
        sheet.cells.set(CellRef { row: 0, col: 0 }, Cell::value(CellValue::Number(1.5)));
        sheet.cells.set(CellRef { row: 0, col: 1 }, Cell::value(CellValue::Text("two".into())));
        let bold = kition_sheet_model::RunFont { bold: Some(true), ..Default::default() };
        let runs = [kition_sheet_model::TextRun::new("rich ", bold), kition_sheet_model::TextRun::new("text", Default::default())];
        if let Some(rich) = kition_sheet_model::RichText::new(runs) {
            sheet.cells.set(CellRef { row: 0, col: 2 }, Cell::rich_text(rich, Default::default()));
        }
    }
    wb
}

/// The canonical features of a package (`canonical::RULES`, all on but `off`), or `None` when
/// the kit has no canonical form.
fn canonical_features(package: &[u8], off: Option<&str>) -> Option<Features> {
    let mut parts = canonical::read(package).ok()?;
    canonical::canonicalize(&mut parts, off);
    let raw_names = off == Some("names");
    let mut out = Features::new();
    for (name, tree) in &parts {
        let kind = fold_digits(name);
        *out.entry("(parts)".into()).or_default().entry(kind.clone()).or_default() += 1;
        if let Some(tree) = tree {
            canonical::count(tree, raw_names, out.entry(kind).or_default());
        }
    }
    Some(out)
}

/// The canonical rules' names.
fn canonical_rules() -> Vec<&'static str> {
    canonical::RULES.iter().map(|(name, _)| *name).collect()
}

// ---- Shared scorer (identical in kition-word, kition-slide, kition-sheet) ------------------------

/// Leaves of a model by class: `path=value` → count.
type Leaves = BTreeMap<&'static str, BTreeMap<String, u64>>;
/// Features of a package by part kind: feature → count.
type Features = BTreeMap<String, BTreeMap<String, u64>>;

fn digest(bytes: &[u8]) -> String {
    // FNV-1a: enough to tell two media items apart in a report.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:016x}/{}", hash, bytes.len())
}

fn fold_digits(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_digits = false;
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

fn leaves(value: &Value) -> Leaves {
    fn walk(value: &Value, path: &mut String, out: &mut Leaves) {
        match value {
            Value::Null => {}
            Value::Object(map) => {
                for (key, item) in map {
                    if ignored(key) {
                        continue;
                    }
                    let len = path.len();
                    path.push('.');
                    path.push_str(if key.chars().all(|c| c.is_ascii_digit() || c == '-') { "#" } else { key });
                    walk(item, path, out);
                    path.truncate(len);
                }
            }
            Value::Array(items) if items.iter().any(|v| v.is_object() || v.is_array()) => {
                let len = path.len();
                path.push_str("[]");
                for item in items {
                    walk(item, path, out);
                }
                path.truncate(len);
            }
            scalar => {
                *out.entry(classify(path)).or_default().entry(format!("{path}={scalar}")).or_default() += 1;
            }
        }
    }
    let mut out = Leaves::new();
    walk(value, &mut String::new(), &mut out);
    out
}

fn features(package: &[u8]) -> Result<Features, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).map_err(|e| e.to_string())?;
    let mut out = Features::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let kind = fold_digits(&name);
        *out.entry("(parts)".into()).or_default().entry(kind.clone()).or_default() += 1;
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".xml") || lower.ends_with(".rels") || lower.ends_with(".vml")) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let counts = out.entry(kind).or_default();
        let mut reader = quick_xml::Reader::from_reader(bytes.as_slice());
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)) => {
                    let element = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                        if key == "xmlns" || key.starts_with("xmlns:") {
                            continue;
                        }
                        if element == "Relationship" && key == "Type" {
                            let value = String::from_utf8_lossy(&attr.value).into_owned();
                            let tail = value.rsplit('/').next().unwrap_or_default().to_owned();
                            *counts.entry(format!("rel:{tail}")).or_default() += 1;
                        }
                        *counts.entry(format!("{element}@{key}")).or_default() += 1;
                    }
                    *counts.entry(element).or_default() += 1;
                }
                Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                Ok(_) => {}
            }
            buf.clear();
        }
    }
    Ok(out)
}

#[derive(Default)]
struct ClassScore {
    files: u64,
    files_kept: u64,
    leaves: u64,
    kept: u64,
    added: u64,
}

#[derive(Default)]
struct Scorecard {
    files: u64,
    unreadable: u64,
    unwritable: u64,
    unrereadable: u64,
    classes: BTreeMap<&'static str, ClassScore>,
    /// (class, path with digits folded) → (files, leaves lost, an example: lost leaf → a leaf the
    /// second import has instead on that path, if any)
    lost_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    /// (class, path with digits folded) → (files, leaves added, an example)
    added_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    xml_original: u64,
    xml_kept: u64,
    /// (part kind, feature) → (files, count lost)
    lost_features: BTreeMap<(String, String), (u64, u64)>,
    part_kinds: BTreeMap<String, (u64, u64)>,
    /// The same, compared canonically ([`canonical_features`]).
    canonical: PackageScore,
    /// Canonical rule → raw losses it accounts for (the losses found with the rule off alone,
    /// less those found with every rule on).
    rules: BTreeMap<&'static str, i64>,
    lines: Vec<String>,
}

/// Package features of a corpus, original against export.
#[derive(Default)]
struct PackageScore {
    original: u64,
    kept: u64,
    lost_features: BTreeMap<(String, String), (u64, u64)>,
    part_kinds: BTreeMap<String, (u64, u64)>,
}

impl PackageScore {
    /// Adds a file's comparison; returns its (original, kept) features.
    fn add(&mut self, original: &Features, exported: &Features) -> (u64, u64) {
        let (mut total, mut kept) = (0u64, 0u64);
        for (kind, counts) in original {
            let theirs = exported.get(kind);
            let kind_total: u64 = counts.values().sum();
            let mut kind_kept = 0;
            for (feature, n) in counts {
                let m = theirs.and_then(|t| t.get(feature)).copied().unwrap_or(0);
                kind_kept += m.min(*n);
                if m < *n {
                    let entry = self.lost_features.entry((kind.clone(), feature.clone())).or_default();
                    entry.0 += 1;
                    entry.1 += n - m;
                }
            }
            let entry = self.part_kinds.entry(kind.clone()).or_default();
            entry.0 += kind_total;
            entry.1 += kind_kept;
            total += kind_total;
            kept += kind_kept;
        }
        self.original += total;
        self.kept += kept;
        (total, kept)
    }
}

/// Features `exported` lost of `original`.
fn lost_of(original: &Features, exported: &Features) -> u64 {
    let mut score = PackageScore::default();
    let (total, kept) = score.add(original, exported);
    total - kept
}

/// The number a leaf holds, if it holds one.
fn number_of(leaf: &str) -> Option<f64> {
    leaf.split_once('=').and_then(|(_, v)| if v.starts_with('"') { None } else { v.parse().ok() })
}

/// Folded path → (count, an example).
type Paths = BTreeMap<String, (u64, String)>;

/// How many of `left`'s leaves `right` keeps (equal, or a number [`close`] to a spare one on the
/// same path), how many it adds, what it lost and what it added.
fn compare(class: &str, left: &BTreeMap<String, u64>, right: &BTreeMap<String, u64>) -> (u64, u64, Paths, Paths) {
    let mut spare: BTreeMap<String, Vec<(f64, &str)>> = BTreeMap::new();
    let mut added = 0;
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        added += extra;
        if let Some(v) = number_of(leaf) {
            for _ in 0..extra {
                spare.entry(path_of(leaf)).or_default().push((v, leaf));
            }
        }
    }
    let mut kept = 0;
    let mut lost = Paths::new();
    for (leaf, n) in left {
        let m = right.get(leaf).copied().unwrap_or(0);
        kept += m.min(*n);
        let path = path_of(leaf);
        for _ in m..*n {
            if let (Some(v), Some(list)) = (number_of(leaf), spare.get_mut(&path))
                && let Some(i) = list.iter().position(|(w, _)| close(class, v, *w))
            {
                list.swap_remove(i);
                kept += 1;
                added -= 1;
                continue;
            }
            let entry = lost.entry(fold_digits(&path)).or_default();
            entry.0 += 1;
            if entry.1.is_empty() {
                let instead = right.keys().find(|r| path_of(r) == path && !left.contains_key(*r)).map(String::as_str).unwrap_or("nothing");
                entry.1 = format!("{} → {}", clip(leaf), clip(instead));
            }
        }
    }
    let mut gained = Paths::new();
    let mut note = |leaf: &str, n: u64| {
        let entry = gained.entry(fold_digits(&path_of(leaf))).or_default();
        entry.0 += n;
        if entry.1.is_empty() {
            entry.1 = clip(leaf);
        }
    };
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        if extra > 0 && number_of(leaf).is_none() {
            note(leaf, extra);
        }
    }
    for (_, leaf) in spare.values().flatten() {
        note(leaf, 1);
    }
    (kept, added, lost, gained)
}

/// `leaf`, cut to a readable length.
fn clip(leaf: &str) -> String {
    let mut out: String = leaf.chars().take(160).collect();
    if out.len() < leaf.len() {
        out.push('…');
    }
    out
}

fn path_of(leaf: &str) -> String {
    leaf.split_once('=').map(|(p, _)| p.to_owned()).unwrap_or_default()
}

impl Scorecard {
    fn file(&mut self, name: &str, bytes: &[u8]) {
        self.files += 1;
        let Ok(first) = import(bytes) else {
            self.unreadable += 1;
            self.lines.push(format!("{name}: not importable"));
            return;
        };
        let Ok(written) = export(&first) else {
            self.unwritable += 1;
            self.lines.push(format!("{name}: export failed"));
            return;
        };
        let Ok(second) = import(&written) else {
            self.unrereadable += 1;
            self.lines.push(format!("{name}: export not re-importable"));
            return;
        };
        let (a, b) = (leaves(&model_value(&first)), leaves(&model_value(&second)));
        let mut lost_classes = Vec::new();
        let empty = BTreeMap::new();
        for (class, left) in &a {
            let right = b.get(class).unwrap_or(&empty);
            let score = self.classes.entry(class).or_default();
            let total: u64 = left.values().sum();
            let (kept, added, lost_paths, added_paths) = compare(class, left, right);
            score.files += 1;
            score.leaves += total;
            score.kept += kept;
            score.added += added;
            if kept == total && added == 0 {
                score.files_kept += 1;
            } else {
                lost_classes.push(format!("{class} {kept}/{total}+{added}"));
            }
            for (paths, into) in [(lost_paths, &mut self.lost_leaves), (added_paths, &mut self.added_leaves)] {
                for (path, (n, example)) in paths {
                    let entry = into.entry((class, path)).or_default();
                    entry.0 += 1;
                    entry.1 += n;
                    if entry.2.is_empty() {
                        entry.2 = example;
                    }
                }
            }
        }
        for (class, right) in &b {
            if !a.contains_key(class) {
                let score = self.classes.entry(class).or_default();
                score.files += 1;
                score.added += right.values().sum::<u64>();
                lost_classes.push(format!("{class} 0/0+{}", right.values().sum::<u64>()));
            }
        }
        let (Ok(original), Ok(exported)) = (features(bytes), features(&written)) else {
            self.lines.push(format!("{name}: classes changed: {lost_classes:?}; package not comparable"));
            return;
        };
        let mut raw = PackageScore::default();
        let (total, kept) = raw.add(&original, &exported);
        for (key, (files, count)) in raw.lost_features {
            let entry = self.lost_features.entry(key).or_default();
            entry.0 += files;
            entry.1 += count;
        }
        for (kind, (t, k)) in raw.part_kinds {
            let entry = self.part_kinds.entry(kind).or_default();
            entry.0 += t;
            entry.1 += k;
        }
        self.xml_original += total;
        self.xml_kept += kept;
        let mut line = format!("{name}: xml {:.1}%", percent(kept, total));
        if let (Some(a), Some(b)) = (canonical_features(bytes, None), canonical_features(&written, None)) {
            let mut file = PackageScore::default();
            let (ct, ck) = file.add(&a, &b);
            self.canonical.add(&a, &b);
            line.push_str(&format!(", canonical {:.1}%", percent(ck, ct)));
            let mut lost: Vec<_> = file.lost_features.iter().map(|((kind, feature), (_, n))| (*n, format!("{kind} {feature}"))).collect();
            lost.sort_by(|x, y| y.cmp(x));
            if !lost.is_empty() {
                let top: Vec<String> = lost.iter().take(12).map(|(n, f)| format!("{f} {n}")).collect();
                line.push_str(&format!(" (canonical losses: {})", top.join(", ")));
            }
            let lost_all = ct - ck;
            for rule in canonical_rules() {
                if let (Some(a), Some(b)) = (canonical_features(bytes, Some(rule)), canonical_features(&written, Some(rule))) {
                    *self.rules.entry(rule).or_default() += lost_of(&a, &b) as i64 - lost_all as i64;
                }
            }
        }
        self.lines.push(format!("{line}; classes changed: {lost_classes:?}"));
    }

    fn json(&self) -> Value {
        let classes: serde_json::Map<String, Value> = self
            .classes
            .iter()
            .map(|(class, s)| {
                ((*class).to_owned(), json!({ "files": s.files, "filesKept": s.files_kept, "leaves": s.leaves, "kept": s.kept, "added": s.added }))
            })
            .collect();
        json!({
            "kit": KIT,
            "files": self.files,
            "unreadable": self.unreadable,
            "unwritable": self.unwritable,
            "unrereadable": self.unrereadable,
            "classes": classes,
            "xml": { "original": self.xml_original, "kept": self.xml_kept },
            "xmlCanonical": {
                "original": self.canonical.original,
                "kept": self.canonical.kept,
                "parts": self.canonical.part_kinds.iter().map(|(kind, (t, k))| (kind.clone(), json!({ "original": t, "kept": k }))).collect::<serde_json::Map<_, _>>(),
                "rules": self.rules.iter().map(|(rule, n)| ((*rule).to_owned(), json!(n))).collect::<serde_json::Map<_, _>>(),
            },
        })
    }

    fn report(&self) -> String {
        let mut out = String::new();
        let ok = self.files - self.unreadable - self.unwritable - self.unrereadable;
        out.push_str(&format!(
            "{KIT} fidelity: {} files, {ok} round-tripped ({} unreadable, {} export failed, {} export unreadable)\n\n",
            self.files, self.unreadable, self.unwritable, self.unrereadable
        ));
        out.push_str("class                        files kept   leaves kept      added\n");
        for (class, s) in &self.classes {
            out.push_str(&format!(
                "{class:<28} {:>4}/{:<4}  {:>6.2}% of {:<9} {}\n",
                s.files_kept,
                s.files,
                percent(s.kept, s.leaves),
                s.leaves,
                s.added
            ));
        }
        out.push_str(&format!(
            "\nXML retained: {:.2}% of {} elements and attributes\n",
            percent(self.xml_kept, self.xml_original),
            self.xml_original
        ));
        let mut kinds: Vec<_> = self.part_kinds.iter().collect();
        kinds.sort_by_key(|(_, (total, kept))| std::cmp::Reverse(total - kept));
        out.push_str("\npart kinds by features lost:\n");
        for (kind, (total, kept)) in kinds.iter().take(25) {
            out.push_str(&format!("  {kind:<50} {:>9} lost of {:<9} ({:.1}% kept)\n", total - kept, total, percent(*kept, *total)));
        }
        let mut lost: Vec<_> = self.lost_features.iter().collect();
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost XML features by files (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*count, *files)));
        out.push_str("\ntop lost XML features by count (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        if self.canonical.original > 0 {
            out.push_str(&format!(
                "\nXML retained, compared canonically: {:.2}% of {} elements and attributes\n",
                percent(self.canonical.kept, self.canonical.original),
                self.canonical.original
            ));
            out.push_str("\nraw losses each canonical rule accounts for (that rule off alone):\n");
            for (rule, what) in canonical::RULES {
                out.push_str(&format!("  {rule:<20} {:>9}  {what}\n", self.rules.get(rule).copied().unwrap_or(0)));
            }
            let mut kinds: Vec<_> = self.canonical.part_kinds.iter().collect();
            kinds.sort_by_key(|(_, (total, kept))| std::cmp::Reverse(total - kept));
            out.push_str("\npart kinds by features lost, compared canonically:\n");
            for (kind, (total, kept)) in kinds.iter().take(25) {
                out.push_str(&format!("  {kind:<50} {:>9} lost of {:<9} ({:.1}% kept)\n", total - kept, total, percent(*kept, *total)));
            }
            let mut lost: Vec<_> = self.canonical.lost_features.iter().collect();
            lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*files, *count)));
            out.push_str("\ntop lost XML features compared canonically, by files (files, count):\n");
            for ((kind, feature), (files, count)) in lost.iter().take(150) {
                out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
            }
        }
        let mut lost: Vec<_> = self.lost_leaves.iter().collect();
        lost.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in lost.iter().take(300) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        let mut added: Vec<_> = self.added_leaves.iter().collect();
        added.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop added model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in added.iter().take(100) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        out.push_str("\nper file:\n");
        for line in &self.lines {
            out.push_str(&format!("  {line}\n"));
        }
        out
    }
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 { 100.0 } else { part as f64 * 100.0 / whole as f64 }
}

/// The regressions of `now` against `baseline`, both [`Scorecard::json`] values.
fn regressions(now: &Value, baseline: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let number = |v: &Value, path: &[&str]| path.iter().fold(v, |v, key| &v[*key]).as_u64().unwrap_or(0);
    for key in ["unreadable", "unwritable", "unrereadable"] {
        if number(now, &[key]) > number(baseline, &[key]) {
            out.push(format!("{key}: {} > {}", number(now, &[key]), number(baseline, &[key])));
        }
    }
    let ratio = |kept: u64, total: u64| if total == 0 { 1.0 } else { kept as f64 / total as f64 };
    if let Some(classes) = baseline["classes"].as_object() {
        for class in classes.keys() {
            let (b, n) = (&baseline["classes"][class], &now["classes"][class]);
            if number(n, &["filesKept"]) < number(b, &["filesKept"]) {
                out.push(format!("{class}: {} files keep it, baseline {}", number(n, &["filesKept"]), number(b, &["filesKept"])));
            }
            // More leaves lost than the class grew by: a model that learns a property may also
            // count it on objects that were lost already, which is not a regression.
            let lost = |v: &Value| number(v, &["leaves"]).saturating_sub(number(v, &["kept"]));
            let grown = number(n, &["leaves"]).saturating_sub(number(b, &["leaves"]));
            if lost(n) > lost(b) + grown {
                out.push(format!("{class}: {} leaves lost, baseline {} (+{grown} leaves)", lost(n), lost(b)));
            }
        }
    }
    let (rb, rn) = (
        ratio(number(baseline, &["xml", "kept"]), number(baseline, &["xml", "original"])),
        ratio(number(now, &["xml", "kept"]), number(now, &["xml", "original"])),
    );
    if rn + 1e-9 < rb {
        out.push(format!("XML retained {:.4}%, baseline {:.4}%", rn * 100.0, rb * 100.0));
    }
    if baseline.get("xmlCanonical").is_some() {
        let (rb, rn) = (
            ratio(number(baseline, &["xmlCanonical", "kept"]), number(baseline, &["xmlCanonical", "original"])),
            ratio(number(now, &["xmlCanonical", "kept"]), number(now, &["xmlCanonical", "original"])),
        );
        if rn + 1e-9 < rb {
            out.push(format!("XML retained canonically {:.4}%, baseline {:.4}%", rn * 100.0, rb * 100.0));
        }
    }
    out
}

#[test]
fn corpus_fidelity() {
    let Ok(dir) = std::env::var(CORPUS_VAR) else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{dir}: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e))))
        .collect();
    files.sort();
    let mut card = Scorecard::default();
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
        card.file(&name, &bytes);
    }
    let report = card.report();
    let summary: String = report.split("\npart kinds").next().unwrap_or_default().to_owned();
    eprintln!("{summary}");
    let now = card.json();
    if let Ok(out) = std::env::var("KITION_FIDELITY_OUT") {
        let _ = std::fs::create_dir_all(&out);
        std::fs::write(format!("{out}/{KIT}-fidelity.txt"), &report).unwrap_or_else(|error| panic!("{out}: {error}"));
        let pretty = serde_json::to_string_pretty(&now).unwrap_or_default();
        std::fs::write(format!("{out}/{KIT}-fidelity.json"), pretty).unwrap_or_else(|error| panic!("{out}: {error}"));
    }
    if let Ok(path) = std::env::var("KITION_FIDELITY_BASELINE") {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
        let baseline: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
        let regressed = regressions(&now, &baseline);
        assert!(regressed.is_empty(), "fidelity regressed against {path}:\n{}", regressed.join("\n"));
    }
}

#[test]
fn scorer_passes_a_lossless_round_trip_and_flags_a_regression() {
    let doc = synthetic();
    let bytes = export(&doc).unwrap_or_else(|error| panic!("export: {error}"));
    let mut card = Scorecard::default();
    card.file("synthetic", &bytes);
    assert_eq!(card.unreadable + card.unwritable + card.unrereadable, 0, "{}", card.report());
    assert!(card.classes.values().all(|s| s.files_kept == s.files), "{}", card.report());
    assert!(card.classes.contains_key(PRIMARY_CLASS), "{}", card.report());
    assert!(card.xml_original > 0 && card.xml_kept == card.xml_original, "{}", card.report());
    assert!(card.canonical.original > 0 && card.canonical.kept == card.canonical.original, "{}", card.report());
    // A baseline of this run accepts it; one that kept more flags it.
    let now = card.json();
    assert!(regressions(&now, &now).is_empty());
    let mut better = now.clone();
    better["xml"]["kept"] = json!(card.xml_kept + 1);
    better["xml"]["original"] = json!(card.xml_original);
    assert_eq!(regressions(&now, &better).len(), 1);
    let mut better = now.clone();
    better["xmlCanonical"]["kept"] = json!(card.canonical.kept + 1);
    assert_eq!(regressions(&now, &better).len(), 1);
}
