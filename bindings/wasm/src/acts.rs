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
        | "draw.inkToShape"
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
        | "formulas.nameManager"
        | "formulas.showFormulas"
        | "formulas.watchWindow"
        | "home.defaultWidth"
        | "home.deleteSheet"
        | "home.manageRules"
        | "insert.sparkline"
        | "object.delete"
        | "object.move"
        | "object.setAnchorMode"
        | "pageLayout.background"
        | "pageLayout.headerFooter"
        | "pageLayout.margins"
        | "pageLayout.orientation"
        | "pageLayout.pageSetup"
        | "pageLayout.printGridlines"
        | "pageLayout.printHeadings"
        | "pageLayout.printTitles"
        | "pageLayout.scaleToFit"
        | "pageLayout.size"
        | "review.protectSheet"
        | "review.unprotectSheet"
        | "review.changeSpelling"
        | "review.workbookStatistics"
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
        | "formulas.autoSum"
        | "formulas.createFromSelection"
        | "formulas.defineName"
        | "formulas.evaluate"
        | "formulas.evaluateFormula"
        | "formulas.insertFunction"
        | "formulas.traceDependents"
        | "formulas.tracePrecedents"
        | "home.accounting"
        | "home.alignBottom"
        | "home.alignCenter"
        | "home.alignLeft"
        | "home.alignMiddle"
        | "home.alignRight"
        | "home.alignTop"
        | "home.autofitColumnWidth"
        | "home.autofitRowHeight"
        | "home.bold"
        | "home.borders"
        | "home.cellStyle"
        | "home.clearRules"
        | "home.columnWidth"
        | "home.comma"
        | "home.conditionalFormat"
        | "home.decreaseDecimal"
        | "home.decreaseFontSize"
        | "home.decreaseIndent"
        | "home.deleteCells"
        | "home.deleteColumns"
        | "home.deleteRows"
        | "home.doubleUnderline"
        | "home.drawBorder"
        | "home.fillColor"
        | "home.fontColor"
        | "home.fontName"
        | "home.fontSize"
        | "home.formatAsTable"
        | "home.formatCells"
        | "home.hideColumns"
        | "home.hideRows"
        | "home.increaseDecimal"
        | "home.increaseFontSize"
        | "home.increaseIndent"
        | "home.insertCells"
        | "home.insertColumns"
        | "home.insertRows"
        | "home.insertSheet"
        | "home.italic"
        | "home.justify"
        | "home.lockCell"
        | "home.mergeAcross"
        | "home.mergeCells"
        | "home.mergeCenter"
        | "home.newCellStyle"
        | "home.numberFormat"
        | "home.orientation"
        | "home.percent"
        | "home.rowHeight"
        | "home.shrinkToFit"
        | "home.strikethrough"
        | "home.subscript"
        | "home.superscript"
        | "home.underline"
        | "home.unhideColumns"
        | "home.unhideRows"
        | "home.unmergeCells"
        | "home.wrapText"
        | "insert.chart"
        | "insert.checkbox"
        | "insert.icons"
        | "insert.link"
        | "insert.picture"
        | "insert.pivotTable"
        | "insert.recommendedCharts"
        | "insert.recommendedPivotTables"
        | "insert.shape"
        | "insert.symbol"
        | "insert.table"
        | "insert.textBox"
        | "insert.artText"
        | "pageLayout.breaks"
        | "pageLayout.printArea"
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
        | "review.deleteComment"
        | "review.newComment"
        | "review.newNote"
        | "review.replyComment"
        | "review.resolveComment"
        | "review.showNote"
        | "review.spelling"
        | "selection.currentRegion"
        | "selection.move"
        | "selection.set"
        | "selection.stats"
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
        "automate.runScript" | "document.inspect" => ALL,
        "draw.eraser" | "draw.highlighter" | "draw.pen" | "draw.pencil" | "draw.select" | "draw.shape" | "draw.state" => MODE,
        "draw.stroke" | "drawing.ink" => MODE_AND_PART,
        "edit.redo" | "edit.undo" | "selection.column" | "selection.next" | "selection.row" => SELECTION,
        _ => &[],
    }
}

/// The commands the binding adds, what they read (by hand: the scan reads the engine's code), and
/// why.
pub const BINDING_COMMANDS: &[(&str, &[EditorState], &str)] = &[
    ("file.render", &[], "draws the sheet and range its `target` names, never the active ones"),
    ("commandLine", ALL, "enters its text into the active cell of the active sheet and clears its editing mode"),
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
    ("insert.chart", "`range` is the chart's data; the chart is a new object"),
    ("insert.recommendedCharts", "`range` is the charts' data"),
    ("insert.pivotTable", "`range` is the pivot table's source; the table is made at `at`"),
    ("insert.recommendedPivotTables", "`range` is the pivot tables' source"),
    ("home.manageRules", "its offered range does not choose the managed rule"),
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
        "data.filterBy" | "review.newNote" | "review.newComment" => "cell",
        "chart.shapeFill" => "chart",
        "data.goalSeek" => "changing",
        _ => ["ids", "id", "chart", "range", "cell"].into_iter().find(|name| schema.get(*name).is_some())?,
    };
    schema.get(name)?;
    kition_kit::schema::check_targets(schema, name).is_ok().then_some(name)
}
