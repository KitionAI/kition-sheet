//! C8-sheet-read: one census over every registered write.
//!
//! This is a lower-bound census: exact reviewed reference/editor metadata plus
//! sampled required facts and unrelated-cell exclusions. It does not claim every
//! prose fact/engine branch has an executable oracle. The original four READINGS
//! constants remain the blind evidence; every typed projection here is pinned
//! after reviewing declarations, schemas and those retained readings.

use kition_kit::{KitCommand, KitDocument, Under};
use kition_sheet_kit::SheetDocument;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type Set = BTreeSet<String>;
const CLASSES: [&str; 6] = [
    "parameter resolved differently from its name",
    "value naming another object",
    "default taken from elsewhere",
    "two code paths computing one fact differently",
    "a read that is not held (or unread fact held)",
    "editor state read but not declared",
];

#[derive(Default)]
struct Census {
    less: [Set; 6],
    more: [Set; 6],
}

impl Census {
    fn record(&mut self, id: &str, wanted: &Set, actual: &Set) {
        for (missing, facts) in [(true, wanted.difference(actual)), (false, actual.difference(wanted))] {
            for fact in facts {
                let class = if fact.starts_with("editor:") {
                    5
                } else if fact.starts_with("place:") {
                    1
                } else {
                    0
                };
                let slots = if missing { &mut self.less } else { &mut self.more };
                slots[class].insert(id.into());
                eprintln!("CENSUS_FACT {id} {} {fact}", if missing { "LESS" } else { "MORE" });
            }
        }
    }
    fn union(slots: &[Set; 6]) -> Set {
        slots.iter().flat_map(|set| set.iter().cloned()).collect()
    }
    fn summary(&self, name: &str, total: usize) -> (Set, Set) {
        let less = Self::union(&self.less);
        let more = Self::union(&self.more);
        let differing = less.union(&more).count();
        eprintln!(
            "CENSUS {name}: {differing}/{total} ({:.4}%) differ; LESS({})={less:?}; MORE({})={more:?}",
            100.0 * differing as f64 / total as f64,
            less.len(),
            more.len()
        );
        for (index, class) in CLASSES.iter().enumerate() {
            eprintln!(
                "CENSUS_CLASS {name} {class}: LESS({})={:?}; MORE({})={:?}",
                self.less[index].len(),
                self.less[index],
                self.more[index].len(),
                self.more[index]
            );
        }
        (less, more)
    }
}

fn accepted_target(command: &KitCommand, candidate: Option<&'static str>) -> Option<&'static str> {
    candidate.filter(|key| {
        command.schema.as_ref().is_some_and(|schema| schema.get(*key).is_some() && kition_kit::schema::check_targets(schema, key).is_ok())
    })
}

fn fields(command: &KitCommand) -> Set {
    let mut facts = Set::new();
    if let Some(target) = command.targets {
        facts.insert(format!("target:{target}"));
    }
    facts.extend(command.places.iter().map(|key| format!("place:{key}")));
    facts.extend(command.objects.iter().map(|key| format!("object:{key}")));
    facts.extend(command.object_types.iter().flat_map(|(key, types)| types.iter().map(move |kind| format!("type:{key}:{kind}"))));
    facts.extend(command.name_params.iter().map(|(key, field)| format!("name:{key}:{field}")));
    facts.extend(command.reads.iter().map(|state| format!("editor:{}", state.as_str())));
    facts
}

fn family_ids(rows: &str) -> BTreeSet<&str> {
    rows.lines().filter_map(|row| row.strip_prefix("ROW ")).filter_map(|row| row.split(" | ").next()).collect()
}

fn expected_edit_roles(command: &KitCommand) -> (Option<&'static str>, Vec<&'static str>) {
    let has = |key: &&str| command.schema.as_ref().is_some_and(|schema| schema.get(*key).is_some());
    let candidate = ["range", "cell"].into_iter().find(has);
    let mut places: Vec<_> = ["sheet", "cell", "range", "rows", "cols"].into_iter().filter(has).collect();
    match command.id.as_str() {
        "cell.set" | "style.apply" => places.push("name"),
        "edit.paste" | "edit.pasteSpecial" => places.push("at"),
        "sheet.insert" => places.push("before"),
        "sheet.move" => places.push("to"),
        "edit.moveCells" => places.extend(["from", "to"]),
        "edit.autoFill" => places.extend(["source", "target"]),
        "edit.formatPainter" => places.push("apply"),
        _ => {}
    }
    (candidate, places)
}

fn data_roles(id: &str) -> (Option<&'static str>, &'static [&'static str], &'static [&'static str]) {
    match id {
        "data.sortAscending" | "data.sortDescending" => (Some("range"), &["range", "cell", "column"], &[]),
        // Pinned after SDK review: place names are literal top-level keys in 34dff06;
        // the `keys` value contains the independently read column/row selectors.
        "data.sort" => (Some("range"), &["range", "keys"], &[]),
        "data.filter" => (Some("range"), &["range"], &[]),
        // Pinned after Agent routing review (lib.rs execute -> command_target_sheet): even
        // engine-ignored bodies can supply the active part through a sheet/range/cell prefix.
        // filterBy's no-scope branch calls toggle_filter with empty params.
        "data.filterBy" => (Some("cell"), &["sheet", "range", "cell", "table", "column"], &[]),
        "data.clearFilter" => (Some("cell"), &["cell", "table"], &[]),
        "data.removeDuplicates" => (Some("range"), &["range", "columns"], &[]),
        "data.textToColumns" => (Some("range"), &["sheet", "range", "cell", "destination"], &[]),
        "data.validation" => (Some("range"), &["sheet", "range", "cell"], &[]),
        "data.group" | "data.ungroup" | "data.hideDetail" | "data.showDetail" => (None, &["rows", "cols"], &[]),
        "data.subtotal" => (Some("range"), &["range", "groupBy", "columns"], &[]),
        "data.smartFill" => (Some("range"), &["sheet", "range", "cell"], &[]),
        "data.reapply" | "data.refreshAll" => (None, &[], &[]),
        "table.insert" => (Some("range"), &["sheet", "range", "cell"], &[]),
        "table.formatAs" => (Some("range"), &["sheet", "range", "cell", "style"], &[]),
        // New names are literal creation labels. replace resolves an existing custom style.
        "table.newStyle" => (None, &["replace"], &[]),
        "table.deleteStyle" | "table.setDefaultStyle" => (None, &["name"], &[]),
        "table.totalRow"
        | "table.bandedRows"
        | "table.bandedColumns"
        | "table.filterButton"
        | "table.firstColumn"
        | "table.lastColumn"
        | "table.headerRow"
        | "table.style"
        | "table.rename"
        | "table.convertToRange" => (None, &["table"], &[]),
        "table.totalFunction" => (None, &["table", "column"], &[]),
        "chart.insert" | "chart.recommended" => (None, &["sheet", "range", "cell", "at"], &[]),
        "chart.set" => (Some("chart"), &["at"], &["chart"]),
        "chart.switchRowColumn" | "chart.delete" => (Some("chart"), &[], &["chart"]),
        // range is stored source text. It remains a natural on target for the selected data;
        // location separately names the sparkline being replaced.
        "sparkline.insert" => (Some("range"), &["range", "location"], &[]),
        "image.insert" | "shape.insert" | "icon.insert" | "textBox.insert" => (None, &["at"], &[]),
        "artText.insert" | "symbol.insert" => (None, &[], &[]),
        "shape.setText" | "object.delete" | "object.setAnchorMode" => (Some("id"), &[], &["id"]),
        "object.move" => (Some("id"), &["at"], &["id"]),
        // Internal link text is stored, not dereferenced as current source-cell contents.
        "link.insert" => (Some("cell"), &["sheet", "cell", "target"], &[]),
        "note.add" | "comment.add" | "comment.reply" | "comment.resolve" | "note.show" => (Some("cell"), &["sheet", "range", "cell"], &[]),
        // Pinned after helper review: target_ranges resolves cell when supplied; the blind
        // empty-params branch above recorded only the fallback to selection ranges.
        "comment.delete" => (Some("cell"), &["cell"], &[]),
        "conditionalFormat.apply" => (Some("range"), &["sheet", "range", "cell"], &[]),
        // Here sheet is the whole-sheet boolean, not a Sheet name or tab index.
        "conditionalFormat.clearRules" => (Some("range"), &["range", "cell"], &[]),
        // cell/range prefixes can pick the sheet; their bodies do not select the indexed rule.
        "conditionalFormat.manageRules" => (None, &["sheet", "range", "cell", "delete", "moveUp", "moveDown"], &[]),
        "formula.autoSum" | "name.addFromSelection" => (Some("range"), &["range"], &[]),
        "formula.insertFunction" | "formula.showFormulas" | "formula.calculationOptions" => (None, &[], &[]),
        // refersTo is parsed and stored; it needs address/name facts, not source values.
        "name.define" => (None, &["name", "scope", "refersTo"], &[]),
        "name.delete" => (None, &["name", "scope"], &[]),
        "pivot.insert" | "pivot.recommended" => (None, &["source", "range", "destination", "location"], &[]),
        // summarize_table replaces source from the chosen table; range can still supply the
        // Agent part. rows/columns are source-field lists, not worksheet axes.
        "table.summarizePivot" => (None, &["table", "range", "destination", "location"], &[]),
        "pivot.addField"
        | "pivot.removeField"
        | "pivot.moveField"
        | "pivot.valueSettings"
        | "pivot.filter"
        | "pivot.sort"
        | "pivot.group"
        | "pivot.collapse"
        | "pivot.layout"
        | "pivot.grandTotals"
        | "pivot.subtotals"
        | "pivot.reportLayout"
        | "pivot.style"
        | "pivot.refresh"
        | "pivot.delete" => (None, &["pivot"], &[]),
        "pivot.changeSource" => (None, &["pivot", "source"], &[]),
        "data.goalSeek" => (Some("changing"), &["set", "changing"], &[]),
        "data.scenarioAdd" => (None, &["changing", "name"], &[]),
        "data.scenarioShow" | "data.scenarioDelete" => (None, &["name"], &[]),
        "data.scenarioSummary" => (None, &["resultCells"], &[]),
        "data.dataTable" => (Some("range"), &["range", "rowInput", "columnInput"], &[]),
        // on selects the source list, including in copy mode.
        "data.advancedFilter" => (Some("range"), &["range", "criteria", "copyTo"], &[]),
        "data.consolidate" => (None, &["sources", "destination"], &[]),
        _ => panic!("unreviewed data/insert reference mapping: {id}"),
    }
}

fn object_roles(id: &str) -> (Option<&'static str>, &'static [(&'static str, &'static [&'static str])], &'static [&'static str]) {
    match id {
        "arrange.align" | "arrange.group" => (Some("ids"), &[("ids", &["Shape", "Chart", "Image"])], &[]),
        "arrange.bringForward" | "arrange.sendBackward" => (Some("id"), &[("id", &["Shape", "Chart", "Image"])], &[]),
        "arrange.rotate" | "chart.shapeOutline" | "ink.toShape" => (Some("id"), &[("id", &["Shape"])], &[]),
        "chart.addElement" | "chart.changeColors" | "chart.changeType" | "chart.formatSelection" | "chart.quickLayout" => {
            (Some("chart"), &[("chart", &["Chart"])], &[])
        }
        "chart.move" => (Some("chart"), &[("chart", &["Chart"])], &["sheet", "at"]),
        "chart.selectData" => (Some("chart"), &[("chart", &["Chart"])], &["range"]),
        // extra.rs:432 chooses id for shapes; lib.rs::refuse_unknown rejects
        // chart/on on that branch before execution. Chart on remains supported.
        "chart.shapeFill" => (Some("chart"), &[("id", &["Shape"]), ("chart", &["Chart"])], &[]),
        "drawing.arrange" => (Some("ids"), &[("ids", &["Shape", "Chart"])], &[]),
        "drawing.chart" => (Some("ids"), &[("ids", &["Chart"]), ("id", &["Chart"])], &[]),
        "drawing.connect" => (None, &[("from", &["Shape"]), ("to", &["Shape"])], &[]),
        "drawing.delete" | "drawing.format" | "drawing.transform" => {
            (Some("ids"), &[("ids", &["Shape", "Chart", "Group"]), ("id", &["Shape", "Chart", "Group"])], &[])
        }
        "drawing.group" => (Some("ids"), &[("ids", &["Shape", "Group"])], &[]),
        "drawing.ungroup" => (Some("ids"), &[("ids", &["Group"])], &[]),
        "drawing.reconnect" => (Some("id"), &[("id", &["Shape"]), ("to", &["Shape"])], &["point"]),
        "drawing.text" => (Some("id"), &[("id", &["Shape"])], &[]),
        "drawing.erase" => (None, &[], &["path", "x", "y"]),
        // The pinned SDK reads top-level place keys literally; the
        // precise point dependencies remain in Under.
        "drawing.ink" => (None, &[], &["strokes"]),
        "ink.stroke" => (None, &[], &["points"]),
        "drawing.insert" => (None, &[], &["range", "frame"]),
        "cell.toggleCheckbox" => (Some("cell"), &[], &["sheet", "cell"]),
        "checkbox.insert" => (Some("range"), &[], &["sheet", "range", "cell"]),
        "format.drawBorder" => (Some("range"), &[], &["sheet", "range", "cell"]),
        "style.add" => (None, &[], &["name", "fromCell"]),
        "data.fromTableRange" => (Some("range"), &[], &["range"]),
        "table.resize" => (Some("range"), &[], &["table", "range"]),
        "table.removeDuplicates" => (Some("range"), &[], &["table", "range", "columns"]),
        "script.delete"
        | "script.new"
        | "script.record"
        | "script.run"
        | "data.fromTextCsv"
        | "data.getData"
        | "formula.useInFormula"
        | "formula.watchWindow"
        | "page.background"
        | "theme.colors"
        | "theme.effects"
        | "theme.fonts"
        | "view.customViews"
        | "view.newWindow"
        | "view.split" => (None, &[], &[]),
        _ => panic!("unreviewed object write {id}"),
    }
}

fn view_roles(id: &str) -> (Option<&'static str>, &'static [&'static str]) {
    match id {
        "view.freezePanes" | "view.freezeRows" | "view.freezeColumns" => (Some("cell"), &["cell", "top"]),
        "view.freezeTopRow" | "view.freezeFirstColumn" => (None, &["top"]),
        "page.printArea" => (Some("range"), &["range"]),
        "page.breaks" => (Some("cell"), &["cell"]),
        // Pinned after review: word is a source-membership predicate on the
        // all-match path, which needs the SDK's place coverage callback.
        "spelling.change" => (Some("cell"), &["cell", "word"]),
        "page.printTitles" => (None, &["cols", "rows"]),
        "page.pageSetup" => {
            // print.rs:1515-1573 interprets these values as coordinates on the
            // resolved sheet, discarding any prefixes inside the setting values.
            (Some("range"), &["sheet", "range", "cell", "printArea", "titleRows", "titleCols", "rowBreaks", "colBreaks"])
        }
        "sheet.protect" | "sheet.unprotect" => (None, &["sheet"]),
        _ => (None, &[]),
    }
}

// Declaration snapshot at 51e29b7, pinned after review; not a blind oracle.
const BASELINE_EDITOR: &str = r#"
arrange.align|part
arrange.bringForward|part
arrange.group|part
arrange.rotate|part
arrange.sendBackward|part
script.delete|
script.new|
script.record|
script.run|
cell.formatText|selection,part
cell.set|selection,part
cell.toggleCheckbox|selection,part
chart.addElement|part
chart.changeColors|part
chart.changeType|part
chart.delete|part
chart.formatSelection|part
chart.move|part
chart.quickLayout|part
chart.selectData|part
chart.set|part
chart.shapeFill|part
chart.shapeOutline|part
chart.switchRowColumn|part
commandLine|selection,part
data.advancedFilter|selection,part
data.clearFilter|selection,part
data.consolidate|selection,part
data.dataTable|part
data.filter|selection,part
data.filterBy|selection,part
data.smartFill|selection,part
data.fromTableRange|selection,part
data.fromTextCsv|
data.getData|
data.goalSeek|part
data.group|selection,part
data.hideDetail|selection,part
data.reapply|part
data.refreshAll|
data.removeDuplicates|selection,part
data.scenarioAdd|part
data.scenarioDelete|part
data.scenarioShow|part
data.scenarioSummary|part
data.showDetail|selection,part
data.sort|selection,part
data.sortAscending|selection,part
data.sortDescending|selection,part
data.subtotal|selection,part
data.textToColumns|selection,part
data.ungroup|selection,part
data.validation|selection,part
ink.toShape|part
ink.stroke|mode,part
drawing.arrange|part
drawing.chart|selection,part
drawing.connect|part
drawing.delete|selection,part
drawing.erase|selection,part
drawing.format|selection,part
drawing.group|part
drawing.ink|mode,part
drawing.insert|selection,part
drawing.reconnect|selection,part
drawing.text|selection,part
drawing.transform|selection,part
drawing.ungroup|part
edit.autoFill|selection,part
edit.clearAll|selection,part
edit.clearComments|selection,part
edit.clearContents|selection,part
edit.clearFormats|selection,part
edit.clearHyperlinks|selection,part
edit.fillDown|selection,part
edit.fillLeft|selection,part
edit.fillRight|selection,part
edit.fillSeries|selection,part
edit.fillUp|selection,part
edit.smartFill|selection,part
edit.formatPainter|selection,part
edit.moveCells|selection,part
edit.paste|selection,part
edit.pasteSpecial|selection,part
edit.redo|selection
edit.replace|selection,part
edit.undo|selection
file.close|
file.new|part
file.open|selection,part
file.properties|
file.save|
file.saveAs|
formula.autoSum|selection,part
formula.calculationOptions|
name.addFromSelection|selection,part
name.define|selection,part
name.delete|
formula.insertFunction|
formula.showFormulas|part
formula.useInFormula|
formula.watchWindow|part
format.accounting|selection,part
format.alignBottom|selection,part
format.alignCenter|selection,part
format.alignLeft|selection,part
format.alignMiddle|selection,part
format.alignRight|selection,part
format.alignTop|selection,part
column.autofitWidth|selection,part
row.autofitHeight|selection,part
format.bold|selection,part
format.borders|selection,part
style.apply|selection,part
conditionalFormat.clearRules|selection,part
column.setWidth|selection,part
format.comma|selection,part
conditionalFormat.apply|selection,part
format.decreaseDecimal|selection,part
format.decreaseFontSize|selection,part
format.decreaseIndent|selection,part
column.setDefaultWidth|part
cell.delete|selection,part
column.delete|selection,part
row.delete|selection,part
sheet.delete|part
format.doubleUnderline|selection,part
format.drawBorder|
format.fillColor|selection,part
format.color|selection,part
format.fontName|selection,part
format.fontSize|selection,part
table.formatAs|selection,part
format.cells|selection,part
column.hide|selection,part
row.hide|selection,part
format.increaseDecimal|selection,part
format.increaseFontSize|selection,part
format.increaseIndent|selection,part
cell.insert|selection,part
column.insert|selection,part
row.insert|selection,part
sheet.insert|selection,part
format.italic|selection,part
format.justify|selection,part
cell.lock|selection,part
conditionalFormat.manageRules|part
cell.mergeAcross|selection,part
cell.merge|selection,part
cell.mergeCenter|selection,part
style.add|selection,part
format.numberFormat|selection,part
format.orientation|selection,part
format.percent|selection,part
row.setHeight|selection,part
format.shrinkToFit|selection,part
format.strikethrough|selection,part
format.subscript|selection,part
format.superscript|selection,part
format.underline|selection,part
column.show|selection,part
row.show|selection,part
cell.unmerge|selection,part
format.wrapText|selection,part
chart.insert|selection,part
checkbox.insert|selection,part
icon.insert|selection,part
link.insert|selection,part
image.insert|selection,part
pivot.insert|selection,part,mode
chart.recommended|selection,part
pivot.recommended|selection,part,mode
shape.insert|selection,part
sparkline.insert|part
symbol.insert|selection,part
table.insert|selection,part
textBox.insert|selection,part
artText.insert|selection,part
object.delete|part
object.move|part
object.setAnchorMode|part
page.background|part
page.breaks|selection,part
page.headerFooter|part
page.margins|part
page.orientation|part
page.pageSetup|part
page.printArea|selection,part
page.printGridlines|part
page.printHeadings|part
page.printTitles|part
page.scaleToFit|part
page.size|part
theme.set|
theme.colors|
theme.effects|
theme.fonts|
pivot.addField|selection,part
pivot.changeSource|selection,part
pivot.collapse|selection,part
pivot.delete|selection,part
pivot.filter|selection,part
pivot.grandTotals|selection,part
pivot.group|selection,part
pivot.layout|selection,part
pivot.moveField|selection,part
pivot.refresh|selection,part
pivot.removeField|selection,part
pivot.reportLayout|selection,part
pivot.sort|selection,part
pivot.style|selection,part
pivot.subtotals|selection,part
pivot.valueSettings|selection,part
range.fill|selection,part
range.setValues|selection,part
spelling.change|part
comment.delete|selection,part
comment.add|selection,part
note.add|selection,part
sheet.protect|part
workbook.protect|
comment.reply|selection,part
comment.resolve|selection,part
note.show|selection,part
sheet.unprotect|part
shape.setText|part
sheet.hide|selection,part
sheet.move|selection,part
sheet.rename|part
sheet.tabColor|part
sheet.unhide|selection,part
table.bandedColumns|selection,part
table.bandedRows|selection,part
table.convertToRange|selection,part
table.deleteStyle|
table.filterButton|selection,part
table.firstColumn|selection,part
table.headerRow|selection,part
table.lastColumn|selection,part
table.newStyle|
table.removeDuplicates|selection,part
table.rename|selection,part
table.resize|selection,part
table.setDefaultStyle|
table.style|selection,part
table.summarizePivot|selection,part,mode
table.totalFunction|selection,part
table.totalRow|selection,part
view.customViews|selection,part
view.freezeColumns|selection,part
view.freezeFirstColumn|part
view.freezePanes|selection,part
view.freezeRows|selection,part
view.freezeTopRow|part
view.gridlineColor|part
view.gridlines|part
view.headings|part
view.newWindow|selection
view.rightToLeft|part
view.showZeros|part
view.split|part
view.unfreezePanes|part
"#;

fn editor_expectations() -> BTreeMap<String, Set> {
    let mut expected = BTreeMap::<String, Set>::new();
    for row in super::readings() {
        let id = row.trim_start_matches("ROW ").split(" | ").next().unwrap();
        let states = row.split(" | ").find_map(|field| field.strip_prefix("reads=")).unwrap();
        let states = states.split(',').filter_map(|state| match state.trim() {
            "part" => Some("part"),
            "selection" | "active" => Some("selection"),
            "mode" | "draw_color" | "draw_width" => Some("mode"),
            _ => None,
        });
        expected.entry(id.into()).or_default().extend(states.map(str::to_owned));
    }
    for (id, states) in &mut expected {
        if let Some((reviewed, _)) = super::edit_format::reviewed_editor(id) {
            *states = reviewed.iter().map(|state| (*state).into()).collect();
        }
    }
    for &(id, states, _) in super::data_insert::REVIEWED_EDITOR.iter().chain(super::objects::REVIEWED_EDITOR.iter()) {
        expected.insert(id.into(), states.iter().map(|state| (*state).into()).collect());
    }
    expected
}

pub(super) fn expected_metadata(command: &KitCommand) -> Set {
    let id = command.id.as_str();
    let data = family_ids(super::data_insert::READINGS);
    let objects = family_ids(super::objects::READINGS);
    let edit = family_ids(super::edit_format::READINGS);
    let view = family_ids(super::view_file::READINGS);
    let mut typed: Vec<(&str, &[&str])> = Vec::new();
    let (target, places, refs): (_, Vec<_>, Vec<_>) = if data.contains(id) {
        let (target, places, refs) = data_roles(id);
        for key in refs {
            let kinds: &[&str] = if *key == "chart" {
                &["Chart"]
            } else if id == "shape.setText" {
                &["Shape"]
            } else {
                &["Shape", "Chart", "Image"]
            };
            typed.push((*key, kinds));
        }
        (target, places.to_vec(), refs.to_vec())
    } else if objects.contains(id) {
        let (target, references, places) = object_roles(id);
        typed.extend_from_slice(references);
        (target, places.to_vec(), references.iter().map(|(key, _)| *key).collect())
    } else if edit.contains(id) {
        let (target, places) = expected_edit_roles(command);
        (target, places, Vec::new())
    } else if view.contains(id) {
        let (target, places) = view_roles(id);
        (target, places.to_vec(), Vec::new())
    } else {
        assert_eq!(id, "command.runLine", "every write must have a reviewed metadata projection");
        (None, Vec::new(), Vec::new())
    };
    let has = |key: &str| command.schema.as_ref().is_some_and(|schema| schema.get(key).is_some());
    let target = accepted_target(command, target);
    let mut facts = Set::new();
    if let Some(target) = target {
        facts.insert(format!("target:{target}"));
    }
    for place in places.iter().filter(|place| has(place)) {
        facts.insert(format!("place:{place}"));
    }
    for key in refs.into_iter().filter(|key| has(key) && Some(*key) != target) {
        facts.insert(format!("object:{key}"));
    }
    for (key, kinds) in typed.into_iter().filter(|(key, _)| has(key)) {
        facts.extend(kinds.iter().map(|kind| format!("type:{key}:{kind}")));
    }
    if places.contains(&"sheet") && has("sheet") {
        facts.insert("type:sheet:Sheet".into());
        facts.insert("name:sheet:name".into());
    }
    if id == "cell.set" {
        facts.insert("type:name:Name".into());
        facts.insert("name:name:name".into());
    }
    facts
}

// Literal replay of reference declarations at 51e29b7 on the offered schemas.
// Baseline declaration data is pinned after review; expected metadata above
// derives from the independently committed readings and labelled reviews.
fn baseline_metadata(command: &KitCommand) -> Set {
    let id = command.id.as_str();
    let has = |key: &str| command.schema.as_ref().is_some_and(|schema| schema.get(key).is_some());
    let candidate = if matches!(id, "drawing.insert" | "chart.insert" | "chart.recommended" | "pivot.insert" | "pivot.recommended") {
        None
    } else {
        ["ids", "id", "chart", "range", "cell"].into_iter().find(|key| has(key))
    };
    let target = accepted_target(command, candidate);
    let mut facts = Set::new();
    if let Some(target) = target {
        facts.insert(format!("target:{target}"));
    }
    for key in baseline_drawing_parameters(id).iter().copied().filter(|key| has(key) && Some(*key) != target) {
        facts.insert(format!("object:{key}"));
    }
    let mut places: Vec<_> = ["sheet", "scope", "table", "pivot", "cell", "range", "rows", "cols"].into_iter().filter(|key| has(key)).collect();
    match id {
        "edit.paste" | "edit.pasteSpecial" if has("at") => places.push("at"),
        "sheet.insert" => places.push("before"),
        "sheet.move" => places.push("to"),
        "edit.moveCells" => places.extend(["from", "to"]),
        "drawing.erase" => places.extend(["path", "x", "y"]),
        "name.define"
        | "name.delete"
        | "formula.useInFormula"
        | "style.apply"
        | "style.add"
        | "table.deleteStyle"
        | "table.setDefaultStyle"
        | "cell.set" => places.push("name"),
        _ => {}
    }
    for place in places {
        facts.insert(format!("place:{place}"));
    }
    if !baseline_drawing_parameters(id).is_empty() {
        for (key, kinds) in baseline_types(id) {
            facts.extend(kinds.iter().map(|kind| format!("type:{key}:{kind}")));
        }
    }
    if has("sheet") {
        facts.insert("type:sheet:Sheet".into());
        facts.insert("name:sheet:name".into());
        if id == "cell.set" {
            facts.insert("name:name:name".into());
        }
    }
    if id == "cell.set" {
        facts.insert("type:name:Name".into());
    }
    facts
}

const DRAWING: &[&str] = &["Shape", "Chart", "Image", "Group"];
const SHAPE: &[&str] = &["Shape"];
const CHART: &[&str] = &["Chart"];
fn baseline_drawing_parameters(command: &str) -> &'static [&'static str] {
    match command {
        "drawing.describe" | "drawing.group" | "drawing.ungroup" | "drawing.arrange" | "arrange.align" | "arrange.group" => &["ids"],
        "drawing.transform" | "drawing.format" | "drawing.delete" | "drawing.chart" => &["ids", "id"],
        "drawing.text"
        | "shape.setText"
        | "object.move"
        | "object.delete"
        | "object.setAnchorMode"
        | "arrange.bringForward"
        | "arrange.sendBackward"
        | "arrange.rotate"
        | "chart.shapeOutline" => &["id"],
        "drawing.reconnect" => &["id", "to"],
        "drawing.connect" => &["from", "to"],
        "chart.shapeFill" => &["id", "chart"],
        "chart.set"
        | "chart.addElement"
        | "chart.changeColors"
        | "chart.changeType"
        | "chart.delete"
        | "chart.formatSelection"
        | "chart.move"
        | "chart.quickLayout"
        | "chart.selectData"
        | "chart.switchRowColumn" => &["chart"],
        _ => &[],
    }
}
fn baseline_types(command: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match command {
        "drawing.connect" => &[("from", SHAPE), ("to", SHAPE)],
        "drawing.reconnect" => &[("id", SHAPE), ("to", SHAPE)],
        "drawing.text" | "shape.setText" | "arrange.rotate" | "chart.shapeOutline" => &[("id", SHAPE)],
        "drawing.chart" => &[("ids", CHART), ("id", CHART)],
        "chart.shapeFill" => &[("id", SHAPE), ("chart", CHART)],
        "drawing.group" | "drawing.ungroup" | "arrange.group" => &[("ids", SHAPE)],
        id if id.starts_with("chart.") => &[("chart", CHART)],
        _ => &[("ids", DRAWING), ("id", DRAWING)],
    }
}

#[test]
fn all_registered_writes_have_one_gating_metadata_census() {
    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let writes: Vec<_> = doc.commands().into_iter().filter(|command| command.mutates).collect();
    assert_eq!(writes.len(), 262);
    let editors = editor_expectations();
    let baseline_editors: BTreeMap<_, _> = BASELINE_EDITOR.lines().filter_map(|line| line.split_once('|')).collect();
    assert_eq!(baseline_editors.len(), 262);
    let (mut before, mut after) = (Census::default(), Census::default());
    for command in &writes {
        let mut wanted = expected_metadata(command);
        let editor = editors.get(&command.id).expect("every write has editor rows");
        wanted.extend(editor.iter().map(|state| format!("editor:{state}")));
        let mut old = baseline_metadata(command);
        let old_editor = baseline_editors.get(command.id.as_str()).expect("baseline registry coverage");
        old.extend(old_editor.split(',').filter(|state| !state.is_empty()).map(|state| format!("editor:{state}")));
        before.record(&command.id, &wanted, &old);
        after.record(&command.id, &wanted, &fields(command));
    }
    before.summary("baseline51e29b7 reviewed metadata lower bound", writes.len());
    let (less, more) = after.summary("current reviewed metadata lower bound", writes.len());
    assert!(less.is_empty(), "new missing metadata; zero floor: {less:?}");
    assert!(more.is_empty(), "new excessive metadata; zero floor: {more:?}");
}

fn data_probe() -> (Set, Set, Set) {
    use std::collections::BTreeSet;

    use kition_kit::KitDocument;
    use kition_sheet_kit::SheetDocument;
    use serde_json::{Value, json};

    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute(
        "range.setValues",
        json!({"range":"A1","values":[["Region","Amount","Date"],["East",3,45000],["West",2,45001],["East",1,45002],["West",4,45003]]}),
    )
    .unwrap();
    doc.execute("table.insert", json!({"range":"A1:C5","name":"Sales","header":true})).unwrap();
    let chart = doc.execute("chart.insert", json!({"range":"A1:C5"})).unwrap()["chart"].clone();
    let shape = doc.execute("textBox.insert", json!({"at":"H1","text":"old"})).unwrap()["shape"].clone();
    let pivot = doc
        .execute("pivot.insert", json!({"source":"Sales","destination":"H8","name":"Report","rows":["Region"],"values":["Amount"]}))
        .unwrap()["id"]
        .clone();
    doc.execute("sheet.insert", json!({"name":"Sheet2","before":9})).unwrap();
    doc.execute("sheet.insert", json!({"name":"Sheet3","before":9})).unwrap();
    doc.execute("cell.set", json!({"sheet":"Sheet3","cell":"Z20","input":"unrelated"})).unwrap();
    doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
    doc.execute("select.set", json!({"range":"B3"})).unwrap();
    doc.version().expect("materialize observed identities");

    let catalogue = doc.commands();
    let mut more = BTreeSet::new();
    let mut compared = BTreeSet::new();
    for line in super::data_insert::READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let fields: Vec<_> = line.split(" | ").collect();
        let id = fields[0].trim_start_matches("ROW ");
        let command = catalogue.iter().find(|command| command.id == id).unwrap();
        if !command.mutates {
            continue;
        }
        let mut params: Value = serde_json::from_str(fields[1].trim_start_matches("params=")).unwrap();
        for (key, replacement) in [("chart", &chart), ("id", &shape), ("pivot", &pivot)] {
            if params.get(key).is_some_and(Value::is_number) {
                params[key] = replacement.clone();
            }
        }
        let under = doc.under(id, &params).unwrap_or_else(|error| panic!("{id} {params}: {error}"));
        compared.insert(id);
        if under.is_some_and(|under| {
            under
                .premises
                .iter()
                .any(|premise| premise.id == "cell:3!Z20" && premise.fields.as_ref().is_none_or(|fields| fields.iter().any(|field| field == "input")))
        }) {
            more.insert(id);
        }
    }

    (compared.into_iter().map(str::to_owned).collect(), Set::new(), more.into_iter().map(str::to_owned).collect())
}

fn edit_probe() -> (Set, Set, Set) {
    use kition_kit::KitDocument;
    use kition_kit::kdm::{ActorId, Replica, tool_ids::ToolIds};
    use kition_kit::session::KitModel;
    use kition_sheet_engine::core::RangeRef;
    use kition_sheet_kit::SheetDocument;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    doc.execute("sheet.rename", json!({"name":"Data"})).expect("first sheet name");
    for sheet in ["Data", "Second"] {
        if sheet == "Second" {
            doc.execute("sheet.insert", json!({"name":sheet})).expect("second sheet");
        }
        doc.execute("range.setValues", json!({"range":"A1","values":[[1,2,3,4,5],[6,7,8,9,10],[11,12,13,14,15],[16,17,18,19,20],[21,22,23,24,25],[26,27,28,29,30],[31,32,33,34,35],[36,37,38,39,40]]})).expect("cells");
    }
    doc.execute("cell.set", json!({"cell":"Z20","input":"99"})).expect("unrelated constant");
    doc.execute("name.define", json!({"name":"Destination","refersTo":"=Second!$B$2"})).expect("cross-sheet name");
    doc.execute("sheet.activate", json!({"sheet":"Data"})).expect("active sheet");
    doc.execute("select.set", json!({"range":"C5"})).expect("distinct active cell");
    doc.execute("style.add", json!({"name":"Custom"})).expect("named cell style");
    doc.execute("edit.copy", json!({"range":"B2:C3"})).expect("clipboard snapshot");
    doc.execute("edit.formatPainter", json!({"sticky":true})).expect("painter snapshot");
    doc.version().expect("observed identities");
    let model = Replica::load(kdm_model::SheetModel::schema(), &doc.snapshot().expect("snapshot"), ActorId::random()).expect("fixture model");
    let names = ToolIds::of_replica(&model);
    let named_sheet = names.resolve("sheet:2", |id| model.exists(id)).expect("named destination sheet");
    let named_rows = model.list(&named_sheet, "rows").expect("named destination rows");
    let named_row = format!("row:{}", named_rows.get(1).expect("named destination row 2"));
    let catalogue = doc.commands();
    let mut compared = BTreeSet::new();
    let mut less = BTreeSet::new();
    let mut more = BTreeSet::new();
    let mut samples: Vec<_> = super::edit_format::READINGS
        .lines()
        .filter_map(|line| line.strip_prefix("ROW "))
        .map(|line| {
            let fields: Vec<_> = line.split(" | ").collect();
            (fields[0], serde_json::from_str::<Value>(fields[1].trim_start_matches("params=")).expect("parameters"))
        })
        .collect();
    // Pinned from the declaration after review: the blind cellStyle note records Title's
    // theme-font branch, but its original concrete sample used Custom. Preserve that sample.
    samples.push(("style.apply", json!({"sheet":"Data","range":"B2","name":"Title"})));
    for (id, params) in samples {
        if !catalogue.iter().any(|command| command.id == id && command.mutates) {
            continue;
        }
        compared.insert(id);
        let under = doc.under(id, &params).unwrap_or_else(|error| panic!("{id} {params}: {error}")).unwrap_or_default();
        let second = params.get("sheet").and_then(Value::as_u64) == Some(1);
        let owner = if second { 2 } else { 1 };
        let mut required: Vec<(String, &'static str)> = Vec::new();
        let target = params
            .get("range")
            .or_else(|| params.get("cell"))
            .and_then(Value::as_str)
            .and_then(|text| RangeRef::parse(text.rsplit_once('!').map_or(text, |(_, body)| body)))
            .map(|range| range.start.a1())
            .unwrap_or_else(|| "C5".into());
        let mut cell = |at: &str, field| required.push((format!("cell:{owner}!{at}"), field));
        match id {
            "cell.set" => {
                if params.get("name").is_some() {
                    // Pinned after derived-field review: named entry deliberately holds the
                    // resolved persistent row, whose layout digest includes complete cells
                    // and resolved styles, rather than freezing the cell's A1 tool address.
                    if !field_held(&under, &named_row, "~layout") {
                        required.push(("cell:2!B2".into(), "style"));
                    }
                } else {
                    let target = if params.get("array").and_then(Value::as_bool) == Some(true) { "C5" } else { &target };
                    cell(target, "style");
                    cell(target, "input");
                }
            }
            "range.setValues" | "range.fill" => {
                cell(&target, "style");
                cell(&target, "input");
                if id == "range.setValues" && params.get("range").is_some() {
                    cell("C3", "style");
                }
            }
            "cell.formatText" => {
                cell(&target, "style");
                cell(&target, "text");
            }
            "edit.paste" | "edit.pasteSpecial" => {
                let at = params.get("at").and_then(Value::as_str).unwrap_or("C5");
                cell(at, "style");
            }
            "edit.clearAll" | "edit.clearContents" => cell(&target, "input"),
            "edit.clearFormats" => cell(&target, "style"),
            "edit.clearComments" | "edit.clearHyperlinks" => {
                // Sparse map keys contain opaque axis ids. Address windows remain a
                // mechanical projection; exact sparse-member behaviour is tested separately.
                assert!(under.within.iter().any(|window| window.parent == "sheet:1" && window.field == "rows"), "{id}: address window");
            }
            "edit.fillDown" | "edit.fillRight" | "edit.fillUp" | "edit.fillLeft" => {
                let source = match (id, params["range"].as_str()) {
                    ("edit.fillDown", Some("B2:C2")) => "B1",
                    ("edit.fillRight", _) => "A2",
                    ("edit.fillUp", _) => "B3",
                    ("edit.fillLeft", _) => "C2",
                    _ => "B2",
                };
                cell(source, "input");
            }
            "edit.autoFill" => cell(if params.get("source").is_some() { "B2" } else { "C5" }, "style"),
            "edit.fillSeries" => cell("B2", "input"),
            "edit.smartFill" => cell("B4", "input"),
            "edit.replace" => cell("C5", "input"),
            "edit.formatPainter" if params.get("cancel").is_none() => cell(if params.get("apply").is_some() { "B2" } else { "C5" }, "style"),
            "edit.formatPainter" | "edit.undo" | "edit.redo" => {}
            "edit.moveCells" => cell(if params.get("from").is_some() { "B2" } else { "C5" }, "input"),
            "format.bold"
            | "format.italic"
            | "format.underline"
            | "format.doubleUnderline"
            | "format.strikethrough"
            | "format.superscript"
            | "format.subscript"
            | "format.fontName"
            | "format.fontSize"
            | "format.increaseFontSize"
            | "format.decreaseFontSize"
            | "format.color"
            | "format.fillColor"
            | "format.alignLeft"
            | "format.alignCenter"
            | "format.alignRight"
            | "format.justify"
            | "format.alignTop"
            | "format.alignMiddle"
            | "format.alignBottom"
            | "format.wrapText"
            | "format.shrinkToFit"
            | "format.increaseIndent"
            | "format.decreaseIndent"
            | "format.orientation"
            | "format.numberFormat"
            | "format.accounting"
            | "format.percent"
            | "format.comma"
            | "format.increaseDecimal"
            | "format.decreaseDecimal"
            | "cell.lock" => cell(&target, "style"),
            "format.borders" => {
                cell(&target, "style");
                if params["preset"] == "none" {
                    cell("A2", "style");
                    cell("D3", "style");
                }
            }
            "format.cells" if params.get("style").is_some() || params.get("edges").is_some() => cell(&target, "style"),
            "format.cells" => {}
            "cell.mergeCenter" | "cell.merge" => cell("C3", "input"),
            "cell.mergeAcross" | "cell.unmerge" => {
                assert!(under.within.iter().any(|window| window.parent == "sheet:1" && window.field == "rows"), "{id}: merge address window");
            }
            "style.apply" => {
                cell(&target, "style");
                if params["name"] == "Title" {
                    required.push(("workbook".into(), "theme.major_font"));
                }
            }
            "row.setHeight"
            | "column.setWidth"
            | "row.autofitHeight"
            | "column.autofitWidth"
            | "row.hide"
            | "column.hide"
            | "row.show"
            | "column.show" => {
                let prefix = if matches!(id, "column.setWidth" | "column.autofitWidth" | "column.hide" | "column.show") { "col:" } else { "row:" };
                assert!(
                    under.premises.iter().any(|premise| premise.id.as_str().is_some_and(|id| id.starts_with(prefix))),
                    "{id}: resolved axis reads"
                );
            }
            "column.setDefaultWidth" => required.push(("sheet:1".into(), "props.defaultColWidth")),
            "row.insert" | "column.insert" | "row.delete" | "column.delete" | "cell.insert" | "cell.delete" => {
                required.push(("sheet:1".into(), "props.protection"));
            }
            "sheet.insert" => {
                required.extend([("workbook".into(), "props.protectedStructure"), ("sheet:2".into(), "name")]);
                let at = params.get("before").and_then(Value::as_u64).unwrap_or(1) as usize;
                if !under.within.iter().any(|window| window.parent == "workbook" && window.field == "sheets" && window.from == at && window.to == at)
                {
                    less.insert(id);
                    eprintln!("FOOTPRINT LESS {id} params={params}: default insertion is after the active tab, workbook.sheets gap {at}");
                }
            }
            "sheet.delete" => {
                required.extend([
                    ("workbook".into(), "props.protectedStructure"),
                    ("sheet:2".into(), "props.visibility"),
                    ("cell:1!B2".into(), "input"),
                ]);
            }
            "sheet.rename" => required.extend([("sheet:1".into(), "name"), ("sheet:2".into(), "name")]),
            "sheet.move" => {
                required.push(("workbook".into(), "props.protectedStructure"));
                if params.get("copy").and_then(Value::as_bool) == Some(true) {
                    required.extend([("sheet:2".into(), "name"), ("cell:1!B2".into(), "input"), ("cell:1!B2".into(), "style")]);
                }
            }
            "sheet.tabColor" => required.push(("sheet:1".into(), "props.tabColor")),
            "sheet.hide" => required.extend([("sheet:1".into(), "props.visibility"), ("sheet:2".into(), "props.visibility")]),
            "sheet.unhide" if params.get("sheet").is_some() => required.push(("sheet:1".into(), "props.visibility")),
            "sheet.unhide" => {}
            _ => panic!("registered write lacks a typed read projection: {id}"),
        }
        if id == "row.setHeight" {
            required.push((format!("sheet:{owner}"), "~editConnectors"));
        }
        let missing: Vec<_> = required.iter().filter(|(object, field)| !field_held(&under, object, field)).collect();
        if !missing.is_empty() {
            less.insert(id);
            eprintln!("FOOTPRINT LESS {id} params={params}: {missing:?}");
        }
        if !matches!(id, "edit.undo" | "edit.redo") && field_held(&under, "cell:2!Z20", "input") {
            more.insert(id);
        }
    }

    (compared.into_iter().map(str::to_owned).collect(), less.into_iter().map(str::to_owned).collect(), more.into_iter().map(str::to_owned).collect())
}

fn object_probe() -> (Set, Set, Set) {
    use kition_kit::{KitDocument, Under};
    use kition_sheet_kit::SheetDocument;
    use serde_json::{Value, json};
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({"range":"A1","values":[["Name","Value","Other"],["One",1,2],["Two",3,4]]})).unwrap();
    doc.execute("drawing.ink", json!({"strokes":[{"points":[[10,100],[40,100]],"color":"#123456","width":2,"tool":"pen"}]})).unwrap();
    for x in [10, 180] {
        doc.execute("drawing.insert", json!({"kind":"shape","frame":{"x":x,"y":10,"w":80,"h":40}})).unwrap();
    }
    doc.execute("chart.insert", json!({"range":"A1:C3","type":"column"})).unwrap();
    doc.execute("table.insert", json!({"range":"A1:C3","name":"Sales","header":true})).unwrap();
    let connector = doc.execute("drawing.connect", json!({"from":2,"to":3})).unwrap()["id"].as_str().unwrap().parse::<u32>().unwrap();
    let mut grouped = Vec::new();
    for x in [400, 600] {
        grouped.push(
            doc.execute("drawing.insert", json!({"kind":"shape","frame":{"x":x,"y":10,"w":80,"h":40}})).unwrap()["id"]
                .as_str()
                .unwrap()
                .parse::<u32>()
                .unwrap(),
        );
    }
    let group = doc.execute("drawing.group", json!({"ids":grouped})).unwrap()["id"].as_str().unwrap().parse::<u32>().unwrap();
    let reconnect_target = grouped.first().copied().unwrap();
    let reconnect_target_tool = format!("drawing:{reconnect_target}");
    doc.execute("sheet.insert", json!({"name":"Sheet2"})).unwrap();
    doc.execute("cell.set", json!({"cell":"D12","input":"sentinel"})).unwrap();
    doc.execute("range.setValues", json!({"range":"A1","values":[["Name","Value","Other"],["One",1,2],["Two",3,4]]})).unwrap();
    doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
    doc.execute("select.set", json!({"range":"B2"})).unwrap();
    doc.version().unwrap();

    fn field_held(under: &Under, object: &str, field: &str) -> bool {
        under.premises.iter().any(|premise| {
            premise.id == object
                && premise.fields.as_ref().is_none_or(|fields| {
                    fields.iter().any(|held| held == field || field.strip_prefix(held).is_some_and(|suffix| suffix.starts_with('.')))
                })
        })
    }
    let commands = doc.commands();
    let mut less = BTreeSet::new();
    let mut more = BTreeSet::new();
    let mut total = BTreeSet::new();
    for row in super::objects::READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let fields: Vec<_> = row.split(" | ").collect();
        let id = fields[0].trim_start_matches("ROW ");
        if !commands.iter().any(|command| command.id == id && command.mutates) {
            continue;
        }
        total.insert(id);
        let mut params: Value = serde_json::from_str(fields.iter().find_map(|field| field.strip_prefix("params=")).unwrap()).unwrap();
        // Readers used 1 for a representative chart/shape. Fixture ids are ink1,
        // shape2,shape3,chart4; remapping preserves their independently read roles.
        if params.get("chart").is_some() {
            params["chart"] = json!(4);
        }
        let chart_path = id == "drawing.chart";
        if params.get("id").is_some() {
            params["id"] = json!(if chart_path {
                4
            } else if id == "ink.toShape" {
                1
            } else if id == "drawing.reconnect" {
                connector
            } else {
                2
            });
        }
        if params.get("ids").is_some() {
            params["ids"] = json!(if id == "drawing.ungroup" { vec![group] } else { vec![2, 3] });
        }
        if params.get("from").is_some() {
            params["from"] = json!(2);
        }
        if params.get("to").is_some() {
            params["to"] = json!(if id == "drawing.reconnect" { reconnect_target } else { 3 });
        }
        let under = doc.under(id, &params).unwrap_or_else(|error| panic!("{id}: {error}")).unwrap_or_default();
        let mut required: Vec<(&str, &str)> = Vec::new();
        match id {
            "chart.addElement" if params.get("element").and_then(Value::as_str) == Some("title") => {
                required.push(("drawing:4", "parts.title"));
            }
            "chart.quickLayout" if params.get("layout").and_then(Value::as_u64).is_some_and(|n| n >= 5) => {
                required.push(("drawing:4", "parts.x_title"));
            }
            "chart.changeColors" | "chart.formatSelection" => required.push(("drawing:4", "parts.series")),
            "chart.selectData" => required.extend([("cell:2!B2", "input"), ("cell:2!A2", "style")]),
            "chart.move" => {
                if !under.premises.iter().any(|premise| {
                    premise.id == "drawing:4" && premise.positions.as_ref().is_some_and(|positions| positions.iter().any(|field| field == "frame"))
                }) {
                    less.insert(id);
                    eprintln!("LESS {id} params={params}: retained frame not held");
                }
            }
            "chart.shapeFill" if params.get("kind").and_then(Value::as_str) == Some("shape") => {
                // same-field decision: the replaced fill is held.
                let tool = if params.get("id").is_some() { "drawing:2" } else { "drawing:3" };
                required.push((tool, "look.fill"));
            }
            "chart.shapeFill" => required.push(("drawing:4", "parts.series")),
            "chart.shapeOutline" => {
                // same-field decision: the replaced line is held.
                required.push(("drawing:2", "look.line"));
            }
            "ink.toShape" => {
                required.extend([("drawing:1", "text"), ("drawing:1", "look.kind"), ("drawing:1", "look.line"), ("drawing:1", "look.fill")])
            }
            "drawing.ink" => required.push(("sheet:1", "props.protection")),
            "drawing.text" => required.extend([("drawing:2", "text"), ("drawing:2", "look.text_body"), ("drawing:2", "look.kind")]),
            "drawing.format" => required.push(("drawing:2", "look.kind")),
            "drawing.connect" => {
                required.extend([("drawing:2", "look.preset"), ("drawing:3", "look.geom"), ("workbook", "~nextObjectId")]);
            }
            "drawing.group" => required.push(("workbook", "~nextObjectId")),
            "drawing.reconnect" if params.get("to").is_some() => {
                required.extend([(reconnect_target_tool.as_str(), "look.preset"), (reconnect_target_tool.as_str(), "look.geom")]);
            }
            "drawing.chart" => required.extend([("drawing:4", "parts.series"), ("cell:1!B2", "input")]),
            "style.add" => required.push(("cell:1!B2", "style")),
            "data.fromTableRange" => required.push(("cell:2!B2", "input")),
            // same-field decision: these fields are assigned destinations.
            "theme.colors" => required.extend([("workbook", "theme.name"), ("workbook", "theme.colors")]),
            "theme.fonts" => required.extend([
                ("workbook", "theme.name"),
                ("workbook", "theme.colors"),
                ("workbook", "theme.major_font"),
                ("workbook", "theme.minor_font"),
            ]),
            _ => {}
        }
        let missing: Vec<_> = required.iter().filter(|(object, field)| !field_held(&under, object, field)).collect();
        if !missing.is_empty() {
            less.insert(id);
            eprintln!("LESS {id} params={params}: {missing:?}");
        }
        if matches!(
            id,
            "arrange.align" | "arrange.group" | "drawing.connect" | "drawing.transform" | "drawing.format" | "drawing.text" | "drawing.chart"
        ) {
            for kind in ["row:", "col:"] {
                let held = under.premises.iter().any(|premise| {
                    premise.id.as_str().is_some_and(|id| id.starts_with(kind))
                        && premise.fields.as_ref().is_none_or(|fields| fields.iter().any(|field| field == "info" || field == "info.size"))
                });
                if !held {
                    less.insert(id);
                    eprintln!("LESS {id} params={params}: coordinate conversion does not hold {kind}info.size");
                }
            }
        }
        if !matches!(id, "script.run" | "view.newWindow") && field_held(&under, "cell:2!D12", "input") {
            more.insert(id);
        }
    }

    (total.into_iter().map(str::to_owned).collect(), less.into_iter().map(str::to_owned).collect(), more.into_iter().map(str::to_owned).collect())
}

fn field_held(under: &kition_kit::Under, object: &str, field: &str) -> bool {
    // Pinned after SDK ownership review and the independent source-deletion writer test:
    // a whole owning Sheet premise also covers its child Cell data.
    let owner = object.strip_prefix("cell:").and_then(|address| address.split_once('!')).map(|(sheet, _)| format!("sheet:{sheet}"));
    under.premises.iter().any(|premise| {
        owner.as_ref().is_some_and(|owner| premise.id == *owner && premise.fields.is_none())
            || premise.id == object
                && premise.fields.as_ref().is_none_or(|fields| {
                    fields.iter().any(|held| held == field || field.strip_prefix(held).is_some_and(|suffix| suffix.starts_with('.')))
                })
    })
}

fn view_probe() -> (Set, Set, Set) {
    use serde_json::json;
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let values: Vec<_> = (1..=12).map(|row| json!([row, row * 2, row * 3, row * 4])).collect();
    doc.execute("range.setValues", json!({"range":"A1","values":values})).unwrap();
    doc.execute("sheet.insert", json!({"name":"Sheet2"})).unwrap();
    doc.execute("range.setValues", json!({"range":"A1","values":values})).unwrap();
    doc.execute("cell.set", json!({"cell":"Z20","input":"unrelated constant"})).unwrap();
    doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
    doc.execute("select.set", json!({"range":"C5"})).unwrap();
    doc.version().unwrap();
    let catalogue = doc.commands();
    let (mut compared, mut less, mut more) = (Set::new(), Set::new(), Set::new());
    for row in super::view_file::READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let fields: Vec<_> = row.split(" | ").collect();
        let id = fields[0].trim_start_matches("ROW ");
        if !catalogue.iter().any(|command| command.id == id && command.mutates) {
            continue;
        }
        compared.insert(id.into());
        let mut params: Value = serde_json::from_str(fields[1].trim_start_matches("params=")).unwrap();
        // Public protection schemas reject the engine-only sheet parameter.
        if matches!(id, "sheet.protect" | "sheet.unprotect") {
            params.as_object_mut().unwrap().remove("sheet");
        }
        let under: Under = doc.under(id, &params).unwrap().unwrap_or_default();
        if !matches!(id, "file.save" | "file.saveAs") && field_held(&under, "cell:2!Z20", "input") {
            more.insert(id.into());
        }
        // Pinned after handler review: the empty properties call only returns
        // current properties; no stale write depends on holding that response.
        if id == "file.properties" && params.as_object().is_some_and(|params| params.is_empty()) {
            continue;
        }
        if matches!(id, "sheet.protect" | "sheet.unprotect") && !field_held(&under, "sheet:1", "~editConnectors") {
            less.insert(id.into());
        }
        let holds = fields[3].trim_start_matches("holds=");
        let second = !id.starts_with("view.")
            && (params.get("sheet").is_some_and(|sheet| sheet == &json!(1) || sheet == &json!("Sheet2"))
                || ["range", "cell"].iter().any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|value| value.starts_with("Sheet2!"))));
        let owner = if holds.starts_with("root{") {
            Some(("root{", "workbook"))
        } else if holds.starts_with("sheet{") {
            Some(("sheet{", if second { "sheet:2" } else { "sheet:1" }))
        } else {
            None
        };
        if let Some((prefix, owner)) = owner {
            for field in holds.trim_start_matches(prefix).split('}').next().unwrap().split(',') {
                let field = field.replace("props.properties", "props.docProps");
                if !field_held(&under, owner, &field) {
                    less.insert(id.into());
                }
            }
        }
    }
    // A saved origin is read only when the sheet is already frozen and the
    // command actually freezes again. The unfrozen fixture above has no saved
    // origin, and freezePanes without a cell merely unfreezes an existing pane.
    doc.execute("view.freezePanes", json!({"cell":"B2","top":"A11"})).unwrap();
    for command in ["view.freezePanes", "view.freezeRows", "view.freezeColumns", "view.freezeTopRow", "view.freezeFirstColumn"] {
        let params = if matches!(command, "view.freezePanes" | "view.freezeRows" | "view.freezeColumns") { json!({"cell":"C5"}) } else { json!({}) };
        let under = doc.under(command, &params).unwrap().unwrap();
        if !field_held(&under, "sheet:1", "~freezeOrigin") {
            less.insert(command.into());
        }
    }
    (compared, less, more)
}

// Each legacy premise witness was established in a source-coded replay or an
// observed pre-fix failure; it is a lower bound, never a claim about unprobed
// paths. These legacy data are pinned after declaration review at 51e29b7.
fn baseline_premise_witnesses(census: &mut Census) {
    // The source and destination parameters routed the active sheet before
    // chart lookup (independent_chart's before-fix disk-writer probes).
    census.less[0].extend(["chart.move", "chart.selectData"].into_iter().map(str::to_owned));
    let data_broad = [
        "data.advancedFilter",
        "data.consolidate",
        "data.dataTable",
        "data.smartFill",
        "data.goalSeek",
        "data.refreshAll",
        "data.removeDuplicates",
        "data.scenarioAdd",
        "data.scenarioDelete",
        "data.scenarioShow",
        "data.scenarioSummary",
        "data.sort",
        "data.sortAscending",
        "data.sortDescending",
        "data.subtotal",
        "data.textToColumns",
        "formula.autoSum",
        "name.addFromSelection",
        "name.define",
        "formula.insertFunction",
        "formula.showFormulas",
        "conditionalFormat.apply",
        "table.formatAs",
        "chart.insert",
        "icon.insert",
        "link.insert",
        "image.insert",
        "pivot.insert",
        "chart.recommended",
        "pivot.recommended",
        "shape.insert",
        "sparkline.insert",
        "symbol.insert",
        "table.insert",
        "textBox.insert",
        "artText.insert",
        "pivot.addField",
        "pivot.changeSource",
        "pivot.collapse",
        "pivot.delete",
        "pivot.filter",
        "pivot.grandTotals",
        "pivot.group",
        "pivot.layout",
        "pivot.moveField",
        "pivot.refresh",
        "pivot.removeField",
        "pivot.reportLayout",
        "pivot.sort",
        "pivot.style",
        "pivot.subtotals",
        "pivot.valueSettings",
        "note.add",
        "comment.resolve",
        "table.deleteStyle",
        "table.newStyle",
        "table.setDefaultStyle",
        "table.summarizePivot",
    ];
    let edit_broad = [
        "edit.paste",
        "edit.pasteSpecial",
        "edit.fillDown",
        "edit.fillRight",
        "edit.fillUp",
        "edit.fillLeft",
        "edit.autoFill",
        "edit.fillSeries",
        "edit.smartFill",
        "edit.replace",
        "edit.formatPainter",
        "edit.moveCells",
        "cell.insert",
        "cell.delete",
    ];
    let object_broad = [
        "script.delete",
        "script.new",
        "script.record",
        "cell.toggleCheckbox",
        "data.fromTableRange",
        "data.fromTextCsv",
        "data.getData",
        "ink.toShape",
        "ink.stroke",
        "drawing.ink",
        "drawing.insert",
        "formula.useInFormula",
        "formula.watchWindow",
        "checkbox.insert",
        "page.background",
        "theme.colors",
        "theme.effects",
        "theme.fonts",
        "table.resize",
        "view.customViews",
        "view.split",
    ];
    let view_broad = [
        "file.close",
        "file.new",
        "file.open",
        "page.breaks",
        "page.pageSetup",
        "page.printArea",
        "page.printTitles",
        "theme.set",
        "spelling.change",
        "view.freezeColumns",
        "view.freezeFirstColumn",
        "view.freezePanes",
        "view.freezeRows",
        "view.freezeTopRow",
        "view.unfreezePanes",
    ];
    census.more[4].extend(data_broad.into_iter().chain(edit_broad).chain(object_broad).chain(view_broad).map(str::to_owned));
    census.more[4].extend(super::objects::REVIEWED_CHART_FIELDS.iter().map(|(id, _, _, _)| (*id).to_owned()));
    // Maintainer-approved necessity review, independently red before narrowing:
    // address shifts preserve these CF operations' rule decisions, and table
    // header/totals writes do not use the overwritten destination constants.
    census.more[4].extend(
        ["conditionalFormat.manageRules", "conditionalFormat.clearRules", "table.headerRow", "table.totalRow", "table.totalFunction", "table.insert"]
            .into_iter()
            .map(str::to_owned),
    );
    census.less[2].insert("style.apply".into());
    census.more[3].insert("page.scaleToFit".into());
    let freeze = ["view.freezeColumns", "view.freezeFirstColumn", "view.freezePanes", "view.freezeRows", "view.freezeTopRow"];
    census.less[2].extend(freeze.into_iter().chain(["row.insert", "column.insert"]).map(str::to_owned));
    census.less[4].extend(
        freeze
            .into_iter()
            .chain([
                "sheet.protect",
                "sheet.unprotect",
                "row.setHeight",
                "style.apply",
                "sheet.rename",
                "sheet.hide",
                "name.define",
                "cell.set",
                "range.setValues",
                "range.fill",
                "edit.clearContents",
                "row.insert",
                "column.insert",
                "sheet.insert",
                "sheet.delete",
                "sheet.move",
                "arrange.align",
                "arrange.group",
                "drawing.connect",
                "drawing.group",
                "drawing.transform",
                "drawing.format",
                "drawing.text",
                "drawing.chart",
                "drawing.reconnect",
                "chart.selectData",
            ])
            .map(str::to_owned),
    );
}

#[test]
fn all_registered_writes_have_a_gating_lower_bound_census() {
    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let writes: Vec<_> = doc.commands().into_iter().filter(|command| command.mutates).collect();
    let editors = editor_expectations();
    let baseline_editors: BTreeMap<_, _> = BASELINE_EDITOR.lines().filter_map(|line| line.split_once('|')).collect();
    let (mut before, mut after) = (Census::default(), Census::default());
    for command in &writes {
        let mut wanted = expected_metadata(command);
        wanted.extend(editors.get(&command.id).unwrap().iter().map(|state| format!("editor:{state}")));
        let mut old = baseline_metadata(command);
        old.extend(
            baseline_editors.get(command.id.as_str()).unwrap().split(',').filter(|state| !state.is_empty()).map(|state| format!("editor:{state}")),
        );
        before.record(&command.id, &wanted, &old);
        after.record(&command.id, &wanted, &fields(command));
    }
    baseline_premise_witnesses(&mut before);
    let mut probed = Set::new();
    for (family, (covered, less, more)) in [
        ("data/insert/formulas/pivot/trial", data_probe()),
        ("edit/format/sheet", edit_probe()),
        ("objects/extra", object_probe()),
        ("view/file/protection", view_probe()),
    ] {
        eprintln!("CENSUS_PROBE {family}: {} writes; LESS={less:?}; MORE={more:?}", covered.len());
        assert!(probed.is_disjoint(&covered), "a write is counted by two families");
        probed.extend(covered);
        after.less[4].extend(less);
        after.more[4].extend(more);
    }
    let (less, more) = chart_field_probe();
    after.less[4].extend(less);
    after.more[4].extend(more);
    // lib.rs::refuse_unknown rejects the wrong conditional parameter;
    // pin_shape_target exercises the outside-writer refusal and explicit id.
    if writes.iter().any(|command| command.id == "chart.shapeFill" && (command.targets != Some("chart") || !command.objects.contains(&"id"))) {
        for class in [0, 3] {
            after.less[class].insert("chart.shapeFill".into());
            after.more[class].insert("chart.shapeFill".into());
        }
    }
    probed.insert("command.runLine".into()); // binding-only row is pinned; cell entry is covered in edit_probe.
    let all: Set = writes.iter().map(|command| command.id.clone()).collect();
    assert_eq!(probed, all, "every registered write must participate in this census");
    before.summary("baseline51e29b7 metadata and sampled-premise lower bound", all.len());
    let (less, more) = after.summary("current metadata and sampled-premise lower bound", all.len());
    let branch_residual = Set::new();
    assert!(less.is_subset(&branch_residual), "new missing declaration facts: {less:?}");
    for class in [0, 3] {
        assert!(after.less[class].is_subset(&branch_residual) && after.more[class].is_subset(&branch_residual), "new target/branch faults");
    }
    for class in [1, 2, 4, 5] {
        assert!(after.less[class].is_empty(), "new missing facts in {}", CLASSES[class]);
    }
    let residual: Set = [
        "data.advancedFilter",
        "data.consolidate",
        "data.dataTable",
        "data.smartFill",
        "data.goalSeek",
        "data.refreshAll",
        "data.removeDuplicates",
        "data.scenarioAdd",
        "data.scenarioDelete",
        "data.scenarioShow",
        "data.scenarioSummary",
        "data.sort",
        "data.sortAscending",
        "data.sortDescending",
        "data.subtotal",
        "data.textToColumns",
        "formula.autoSum",
        "name.addFromSelection",
        "name.define",
        "conditionalFormat.apply",
        "table.formatAs",
        "chart.insert",
        "link.insert",
        "image.insert",
        "pivot.insert",
        "chart.recommended",
        "pivot.recommended",
        "sparkline.insert",
        "table.insert",
        "artText.insert",
        "pivot.addField",
        "pivot.changeSource",
        "pivot.collapse",
        "pivot.delete",
        "pivot.filter",
        "pivot.grandTotals",
        "pivot.group",
        "pivot.layout",
        "pivot.moveField",
        "pivot.refresh",
        "pivot.removeField",
        "pivot.reportLayout",
        "pivot.sort",
        "pivot.style",
        "pivot.subtotals",
        "pivot.valueSettings",
        "note.add",
        "comment.resolve",
        "table.summarizePivot",
        "edit.paste",
        "edit.pasteSpecial",
        "edit.autoFill",
        "edit.fillSeries",
        "edit.smartFill",
        "edit.replace",
        "edit.formatPainter",
        "edit.moveCells",
        "cell.insert",
        "cell.delete",
        "data.fromTableRange",
        "data.fromTextCsv",
        "data.getData",
        "drawing.insert",
        "page.background",
        "table.resize",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert!(after.more[4].is_subset(&residual), "new excessive content facts: {:?}", after.more[4].difference(&residual).collect::<Vec<_>>());
    let allowed: Set = residual.union(&branch_residual).cloned().collect();
    assert!(more.is_subset(&allowed), "new excessive declaration facts: {:?}", more.difference(&allowed).collect::<Vec<_>>());
    // Do not require equality: a repair may remove an allowance before the next
    // census, but a new command can never hide behind another one's improvement.
}

fn chart_field_probe() -> (Set, Set) {
    use kition_kit::KitDocument;
    use serde_json::json;
    let mut doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("range.setValues", json!({"range":"A1","values":[["Name","Value","Other"],["One",1,2],["Two",3,4],["Three",5,6]]})).unwrap();
    doc.execute("chart.insert", json!({"range":"A1:C4","type":"column","title":"Original"})).unwrap();
    doc.version().unwrap();
    let mut less = BTreeSet::new();
    let mut more = BTreeSet::new();
    for (id, params, fields, _) in super::objects::REVIEWED_CHART_FIELDS {
        let params = serde_json::from_str(params).unwrap();
        let under = doc.under(id, &params).unwrap().unwrap_or_default();
        let premises: Vec<_> = under.premises.iter().filter(|premise| premise.id == "drawing:1").collect();
        if fields.iter().any(|required| {
            !premises.iter().any(|premise| {
                premise.fields.as_ref().is_none_or(|held| {
                    held.iter().any(|held| held == required || required.strip_prefix(held).is_some_and(|tail| tail.starts_with('.')))
                })
            })
        }) {
            less.insert(*id);
        }
        if premises.iter().any(|premise| {
            premise.fields.as_ref().is_none_or(|held| {
                held.iter()
                    .any(|held| !fields.iter().any(|allowed| held == allowed || held.strip_prefix(allowed).is_some_and(|tail| tail.starts_with('.'))))
            }) || premise
                .positions
                .as_ref()
                .is_none_or(|positions| positions.iter().any(|position| position != "sheet" && (*id != "chart.move" || position != "frame")))
        }) {
            more.insert(*id);
        }
    }

    (less.into_iter().map(str::to_owned).collect(), more.into_iter().map(str::to_owned).collect())
}
