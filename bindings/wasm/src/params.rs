//! The parameters commands read that their notation does not spell out, added to the schemas the
//! catalogue offers agents. Every command refuses a key its schema does not name (engine rule E7a),
//! so the schema must name every key the command reads: the target helpers' `cell` beside `range`
//! and `sheet` (`target_ranges`, `target_sheet`), page margins also taken at the top level, and
//! the other keys a handler reads beside its listed ones.

use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum Kind {
    Integer,
    Number,
    String,
    Boolean,
    Array,
    Json,
    IndexOrName,
}

impl Kind {
    fn node(self) -> Value {
        let description = "read by the command beside its listed parameters";
        let kind = match self {
            Kind::IndexOrName => return json!({ "oneOf": [{ "type": "integer" }, { "type": "string" }], "description": description }),
            Kind::Integer => "integer",
            Kind::Number => "number",
            Kind::String => "string",
            Kind::Boolean => "boolean",
            Kind::Array => "array",
            Kind::Json => "json",
        };
        json!({ "type": kind, "description": description })
    }
}

/// Keys a command's handler reads (directly or through a helper it passes its parameters to) that
/// its notation leaves out, found by reading each handler.
const EXTRA: &[(&str, &[(&str, Kind)])] = &[
    // KitDrawing's operations (kition-office-core `DrawingOp::parse`) read these too.
    ("drawing.insert", &[("paragraphs", Kind::Array), ("body", Kind::Json), ("warp", Kind::Json)]),
    ("drawing.transform", &[("id", Kind::Json), ("frame", Kind::Json), ("dx", Kind::Number)]),
    ("drawing.format", &[("id", Kind::Json), ("effects", Kind::Json)]),
    ("drawing.text", &[("text", Kind::Json), ("paragraphs", Kind::Array), ("body", Kind::Json), ("warp", Kind::Json)]),
    ("drawing.delete", &[("id", Kind::Json)]),
    ("drawing.erase", &[("x", Kind::Number)]),
    (
        "drawing.chart",
        &[
            ("ids", Kind::Array),
            ("categories", Kind::Json),
            ("seriesFormat", Kind::Json),
            ("xAxis", Kind::Json),
            ("yAxis", Kind::Json),
            ("dataLabelPosition", Kind::String),
        ],
    ),
    ("arrange.bringForward", &[("toBack", Kind::Boolean)]),
    ("arrange.sendBackward", &[("toFront", Kind::Boolean)]),
    ("data.filterBy", &[("cell", Kind::String), ("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("data.group", &[("cols", Kind::String)]),
    ("data.hideDetail", &[("cols", Kind::String), ("rows", Kind::Json)]),
    ("data.showDetail", &[("cols", Kind::String), ("rows", Kind::Json)]),
    ("data.sortAscending", &[("cell", Kind::String)]),
    ("data.sortDescending", &[("cell", Kind::String)]),
    ("data.ungroup", &[("cols", Kind::String), ("rows", Kind::Json)]),
    ("ink.stroke", &[("tool", Kind::String)]),
    ("ink.toShape", &[("color", Kind::String), ("tool", Kind::String), ("width", Kind::Number)]),
    ("edit.clearAll", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.clearComments", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.clearContents", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.clearFormats", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.clearHyperlinks", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.fillDown", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.fillLeft", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.fillRight", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.fillUp", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("edit.paste", &[("sheet", Kind::IndexOrName)]),
    ("edit.pasteSpecial", &[("at", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("file.exportCsv", &[("cell", Kind::String), ("range", Kind::String)]),
    ("file.exportHtml", &[("cell", Kind::String), ("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.accounting", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.alignBottom", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.alignCenter", &[("cell", Kind::String), ("on", Kind::Boolean), ("sheet", Kind::IndexOrName)]),
    ("format.alignLeft", &[("cell", Kind::String), ("on", Kind::Boolean), ("sheet", Kind::IndexOrName)]),
    ("format.alignMiddle", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.alignRight", &[("cell", Kind::String), ("on", Kind::Boolean), ("sheet", Kind::IndexOrName)]),
    ("format.alignTop", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.bold", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.borders", &[("sheet", Kind::IndexOrName)]),
    ("style.apply", &[("sheet", Kind::IndexOrName)]),
    ("format.comma", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.decreaseDecimal", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.decreaseFontSize", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.decreaseIndent", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("column.setDefaultWidth", &[("sheet", Kind::IndexOrName)]),
    ("cell.delete", &[("sheet", Kind::IndexOrName)]),
    ("column.delete", &[("cell", Kind::String), ("count", Kind::Integer), ("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("row.delete", &[("cell", Kind::String), ("count", Kind::Integer), ("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.color", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.fontSize", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.fontName", &[("sheet", Kind::IndexOrName)]),
    ("format.fillColor", &[("sheet", Kind::IndexOrName)]),
    ("format.cells", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.increaseDecimal", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.increaseFontSize", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.increaseIndent", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("column.insert", &[("cell", Kind::String), ("count", Kind::Integer), ("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("cell.insert", &[("sheet", Kind::IndexOrName)]),
    ("row.insert", &[("sheet", Kind::IndexOrName)]),
    ("format.italic", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.justify", &[("cell", Kind::String), ("on", Kind::Boolean), ("sheet", Kind::IndexOrName)]),
    ("cell.lock", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("conditionalFormat.manageRules", &[("cell", Kind::String), ("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("cell.mergeAcross", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("cell.merge", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("cell.mergeCenter", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.numberFormat", &[("cell", Kind::String), ("sheet", Kind::IndexOrName), ("code", Kind::String)]),
    ("format.orientation", &[("sheet", Kind::IndexOrName)]),
    ("format.percent", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.shrinkToFit", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.strikethrough", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.subscript", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.superscript", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("cell.unmerge", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("format.underline", &[("sheet", Kind::IndexOrName)]),
    ("format.wrapText", &[("cell", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("image.insert", &[("anchorMode", Kind::String)]),
    ("checkbox.insert", &[("sheet", Kind::IndexOrName)]),
    ("artText.insert", &[("width", Kind::Number)]),
    ("cell.toggleCheckbox", &[("sheet", Kind::IndexOrName)]),
    ("format.drawBorder", &[("sheet", Kind::IndexOrName)]),
    ("formula.autoSum", &[("enter", Kind::Boolean)]),
    ("page.background", &[("base64", Kind::String)]),
    ("range.setValues", &[("sheet", Kind::IndexOrName)]),
    ("range.fill", &[("sheet", Kind::IndexOrName)]),
    ("table.removeDuplicates", &[("range", Kind::String)]),
    ("sheet.protect", &[("sheet", Kind::IndexOrName)]),
    ("sheet.unprotect", &[("sheet", Kind::IndexOrName)]),
    (
        "page.pageSetup",
        &[
            ("cell", Kind::String),
            ("range", Kind::String),
            ("left", Kind::Number),
            ("right", Kind::Number),
            ("top", Kind::Number),
            ("bottom", Kind::Number),
        ],
    ),
    ("comment.add", &[("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("note.add", &[("range", Kind::String), ("sheet", Kind::IndexOrName)]),
    ("sheet.move", &[("cell", Kind::String), ("range", Kind::String)]),
    (
        "table.summarizePivot",
        &[
            ("columns", Kind::Array),
            ("filters", Kind::Array),
            ("location", Kind::String),
            ("name", Kind::String),
            ("range", Kind::String),
            ("replace", Kind::Boolean),
            ("rows", Kind::Json),
            ("source", Kind::Json),
            ("values", Kind::Array),
        ],
    ),
];

/// Adds to `schema` (a command's `ParameterSchemaSpec`) the keys `id` reads that its notation
/// leaves out.
pub fn complete(id: &str, schema: &mut Value) {
    let Some(properties) = schema.as_object_mut() else { return };
    for (key, kind) in EXTRA.iter().filter(|(command, _)| *command == id).flat_map(|(_, keys)| keys.iter()) {
        properties.entry(*key).or_insert_with(|| kind.node());
    }
}
