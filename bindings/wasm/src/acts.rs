//! What each command acts on and the editor state it reads besides its parameters
//! (`KitCommand::targets`, `KitCommand::reads`), for kition-sdk's explicit call context: a host
//! sets the active sheet and the selection a command reads from a run step's `at.part` and `on`,
//! and puts them back after the call (task card C4/C5/C6-sheet).
//!
//! `reads` is what the engine's code reads, not what descriptions say: `tests/acts.rs` follows
//! each command's `enabled` predicate and `run` code through the engine to the accessors of the
//! selection, of the active sheet and of the modes, and fails when [`reads`] says otherwise.
use kition_kit::EditorState::{self, Mode, Part, Selection};

const SELECTION_AND_PART: &[EditorState] = &[Selection, Part];
const PART: &[EditorState] = &[Part];
const SELECTION: &[EditorState] = &[Selection];
const MODE: &[EditorState] = &[Mode];
const MODE_AND_PART: &[EditorState] = &[Mode, Part];
const ALL: &[EditorState] = &[Selection, Part, Mode];

/// The editor state engine command `id` reads besides its parameters: the selection (the selected
/// cells, what a `range` defaults to, and the selected drawing objects), the active sheet (the
/// call's part; what a command without `sheet` or a `Sheet!` range acts on) and the modes (the
/// editing mode, the view mode and the Draw tab's pen). Derived from the code (`tests/acts.rs`).
pub fn reads(id: &str) -> &'static [EditorState] {
    match id {
        "arrange.align"
        | "arrange.bringForward"
        | "arrange.group"
        | "arrange.rotate"
        | "arrange.selectionPane"
        | "arrange.sendBackward"
        | "chart.addElement"
        | "chart.changeColors"
        | "chart.changeType"
        | "chart.delete"
        | "chart.formatSelection"
        | "chart.move"
        | "chart.quickLayout"
        | "chart.selectData"
        | "chart.set"
        | "chart.shapeFill"
        | "chart.shapeOutline"
        | "chart.switchRowColumn"
        | "data.circleInvalid"
        | "data.dataTable"
        | "data.goalSeek"
        | "data.reapply"
        | "data.scenarioAdd"
        | "data.scenarioDelete"
        | "data.scenarioManager"
        | "data.scenarioShow"
        | "data.scenarioSummary"
        | "ink.toShape"
        | "drawing.arrange"
        | "drawing.connect"
        | "drawing.chart"
        | "drawing.format"
        | "drawing.group"
        | "drawing.reconnect"
        | "drawing.text"
        | "drawing.transform"
        | "drawing.ungroup"
        | "file.exportCsv"
        | "file.exportHtml"
        | "file.printPreview"
        | "file.save"
        | "file.saveAs"
        | "file.saveBytes"
        | "file.exportPdf"
        | "file.print"
        | "name.manage"
        | "formula.showFormulas"
        | "formula.watchWindow"
        | "column.setDefaultWidth"
        | "sheet.delete"
        | "conditionalFormat.manageRules"
        | "sparkline.insert"
        | "object.delete"
        | "object.move"
        | "object.setAnchorMode"
        | "page.background"
        | "page.headerFooter"
        | "page.margins"
        | "page.orientation"
        | "page.pageSetup"
        | "page.printGridlines"
        | "page.printHeadings"
        | "page.printTitles"
        | "page.scaleToFit"
        | "page.size"
        | "sheet.protect"
        | "sheet.unprotect"
        | "spelling.change"
        | "workbook.statistics"
        | "shape.setText"
        | "sheet.activate"
        | "sheet.next"
        | "sheet.previous"
        | "sheet.rename"
        | "sheet.tabColor"
        | "view.freezeFirstColumn"
        | "view.freezeTopRow"
        | "view.gridlineColor"
        | "view.gridlines"
        | "view.headings"
        | "view.rightToLeft"
        | "view.showZeros"
        | "view.unfreezePanes"
        | "view.zoom" => PART,
        "cell.formatText"
        | "cell.get"
        | "cell.set"
        | "cell.toggleCheckbox"
        | "data.advancedFilter"
        | "data.clearFilter"
        | "data.consolidate"
        | "data.filter"
        | "data.filterBy"
        | "data.filterValues"
        | "data.smartFill"
        | "data.fromTableRange"
        | "data.fromTextCsv"
        | "data.getData"
        | "data.group"
        | "data.hideDetail"
        | "data.removeDuplicates"
        | "data.showDetail"
        | "data.sort"
        | "data.sortAscending"
        | "data.sortDescending"
        | "data.subtotal"
        | "data.textToColumns"
        | "data.ungroup"
        | "data.validate"
        | "data.validation"
        | "drawing.delete"
        | "drawing.describe"
        | "drawing.erase"
        | "drawing.insert"
        | "edit.autoFill"
        | "edit.clearAll"
        | "edit.clearComments"
        | "edit.clearContents"
        | "edit.clearFormats"
        | "edit.clearHyperlinks"
        | "edit.copy"
        | "edit.cut"
        | "edit.fillDown"
        | "edit.fillLeft"
        | "edit.fillRight"
        | "edit.fillSeries"
        | "edit.fillUp"
        | "edit.find"
        | "edit.smartFill"
        | "edit.formatPainter"
        | "edit.goTo"
        | "edit.goToSpecial"
        | "edit.moveCells"
        | "edit.paste"
        | "edit.pasteSpecial"
        | "edit.replace"
        | "edit.selectAll"
        | "file.export"
        | "formula.autoSum"
        | "name.addFromSelection"
        | "name.define"
        | "formula.evaluate"
        | "formula.evaluateFormula"
        | "formula.insertFunction"
        | "formula.traceDependents"
        | "formula.tracePrecedents"
        | "format.accounting"
        | "format.alignBottom"
        | "format.alignCenter"
        | "format.alignLeft"
        | "format.alignMiddle"
        | "format.alignRight"
        | "format.alignTop"
        | "column.autofitWidth"
        | "row.autofitHeight"
        | "format.bold"
        | "format.borders"
        | "style.apply"
        | "conditionalFormat.clearRules"
        | "column.setWidth"
        | "format.comma"
        | "conditionalFormat.apply"
        | "format.decreaseDecimal"
        | "format.decreaseFontSize"
        | "format.decreaseIndent"
        | "cell.delete"
        | "column.delete"
        | "row.delete"
        | "format.doubleUnderline"
        | "format.drawBorder"
        | "format.fillColor"
        | "format.color"
        | "format.fontName"
        | "format.fontSize"
        | "table.formatAs"
        | "format.cells"
        | "column.hide"
        | "row.hide"
        | "format.increaseDecimal"
        | "format.increaseFontSize"
        | "format.increaseIndent"
        | "cell.insert"
        | "column.insert"
        | "row.insert"
        | "sheet.insert"
        | "format.italic"
        | "format.justify"
        | "cell.lock"
        | "cell.mergeAcross"
        | "cell.merge"
        | "cell.mergeCenter"
        | "style.add"
        | "format.numberFormat"
        | "format.orientation"
        | "format.percent"
        | "row.setHeight"
        | "format.shrinkToFit"
        | "format.strikethrough"
        | "format.subscript"
        | "format.superscript"
        | "format.underline"
        | "column.show"
        | "row.show"
        | "cell.unmerge"
        | "format.wrapText"
        | "chart.insert"
        | "checkbox.insert"
        | "icon.insert"
        | "link.insert"
        | "image.insert"
        | "pivot.insert"
        | "chart.recommended"
        | "pivot.recommended"
        | "shape.insert"
        | "symbol.insert"
        | "table.insert"
        | "textBox.insert"
        | "artText.insert"
        | "page.breaks"
        | "page.printArea"
        | "pivot.addField"
        | "pivot.changeSource"
        | "pivot.collapse"
        | "pivot.delete"
        | "pivot.fieldList"
        | "pivot.fields"
        | "pivot.filter"
        | "pivot.grandTotals"
        | "pivot.group"
        | "pivot.layout"
        | "pivot.moveField"
        | "pivot.refresh"
        | "pivot.removeField"
        | "pivot.reportLayout"
        | "pivot.sort"
        | "pivot.style"
        | "pivot.subtotals"
        | "pivot.valueSettings"
        | "range.fill"
        | "range.setValues"
        | "comment.delete"
        | "comment.add"
        | "note.add"
        | "comment.reply"
        | "comment.resolve"
        | "note.show"
        | "spelling.check"
        | "select.currentRegion"
        | "select.move"
        | "select.set"
        | "select.stats"
        | "sheet.hide"
        | "sheet.move"
        | "sheet.read"
        | "sheet.unhide"
        | "table.bandedColumns"
        | "table.bandedRows"
        | "table.convertToRange"
        | "table.filterButton"
        | "table.firstColumn"
        | "table.headerRow"
        | "table.lastColumn"
        | "table.removeDuplicates"
        | "table.rename"
        | "table.resize"
        | "table.style"
        | "table.styles"
        | "table.summarizePivot"
        | "table.totalFunction"
        | "table.totalRow"
        | "view.customViews"
        | "view.newWindow"
        | "view.freezeColumns"
        | "view.freezePanes"
        | "view.freezeRows"
        | "view.split"
        | "view.zoomToSelection" => SELECTION_AND_PART,
        "script.run" | "file.inspect" => ALL,
        "ink.eraser" | "ink.highlighter" | "ink.pen" | "ink.pencil" | "ink.select" | "ink.shape" | "ink.state" => MODE,
        "ink.stroke" | "drawing.ink" => MODE_AND_PART,
        "edit.redo" | "edit.undo" | "select.column" | "select.next" | "select.row" => SELECTION,
        _ => &[],
    }
}

/// The commands the binding adds, what they read (by hand: the scan reads the engine's code), and
/// why.
pub const BINDING_COMMANDS: &[(&str, &[EditorState], &str)] = &[
    ("file.render", &[], "draws the sheet and range its `target` names, never the active ones"),
    ("command.runLine", ALL, "enters its text into the active cell of the active sheet and clears its editing mode"),
    ("editor.viewport", &[], "records the editor's visible extent"),
    ("editor.wording", &[], "sets the editor's language for made-up names"),
    ("editor.scheme", &[], "sets the color scheme the editor's grid is drawn in"),
    ("editor.formula", SELECTION_AND_PART, "types in the formula bar of the active cell"),
    ("editor.cycleReference", SELECTION, "cycles the reference at the caret of the cell being typed"),
    ("editor.acceptFunction", SELECTION, "puts the chosen function into the cell being typed"),
    ("editor.contextMenu", SELECTION_AND_PART, "right-clicks the active sheet's grid, selecting what it hits"),
    ("editor.insertText", SELECTION_AND_PART, "inserts at the caret of the active cell being typed"),
    ("editor.freezePreview", PART, "previews a freeze of the active sheet"),
    ("editor.validation", SELECTION_AND_PART, "answers the active cell's data validation message"),
    ("editor.commentAnchor", SELECTION_AND_PART, "answers the anchor of the selection on the active sheet"),
    ("editor.commentPlaces", PART, "finds comment anchors on the active sheet"),
];

/// [`reads`] for any command the kit lists: the engine's, or the binding's own.
pub fn reads_any(id: &str) -> &'static [EditorState] {
    BINDING_COMMANDS.iter().find(|(command, _, _)| *command == id).map_or_else(|| reads(id), |(_, state, _)| *state)
}

/// Commands whose parameter that looks like a target is not what they act on, and why.
pub const NOT_TARGETS: &[(&str, &str)] = &[
    ("drawing.insert", "`range` is a new chart's data; the object is made at `frame`, else at the active cell"),
    ("chart.insert", "`range` is the chart's data; the chart is a new object"),
    ("chart.recommended", "`range` is the charts' data"),
    ("pivot.insert", "`range` is the pivot table's source; the table is made at `at`"),
    ("pivot.recommended", "`range` is the pivot tables' source"),
    ("conditionalFormat.manageRules", "its offered range does not choose the managed rule"),
    ("table.summarizePivot", "the table is the pivot table's source; destination names where its new report goes"),
];

/// The parameter that names what command `id` acts on: drawing objects (`ids`, `id`, `chart`, by
/// the ids sheet_inspect lists), else cells (`range`, else `cell`, in A1 notation). `None` for a
/// command without one, or when kition-sdk's `schema::check_targets` does not take its schema for
/// an id (an untyped or object parameter): `on` then reaches the command through the selection it
/// reads.
pub fn targets(id: &str, schema: Option<&serde_json::Value>) -> Option<&'static str> {
    let schema = schema?;
    if NOT_TARGETS.iter().any(|(command, _)| *command == id) {
        return None;
    }
    let name = match id {
        "data.filterBy" | "note.add" | "comment.add" => "cell",
        "chart.shapeFill" => "chart",
        "data.goalSeek" => "changing",
        _ => ["ids", "id", "chart", "range", "cell"].into_iter().find(|name| schema.get(*name).is_some())?,
    };
    schema.get(name)?;
    kition_kit::schema::check_targets(schema, name).is_ok().then_some(name)
}
