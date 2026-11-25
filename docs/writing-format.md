# Sheet writing format

How a model writes Kition Sheet documents: the steps it sends, how it names what it changes,
what a new document starts with, the named values commands accept, and worked examples.
The package ships the short version for models as [agent/guide.md](../agent/guide.md); this file is
the complete reference it is drawn from. Everything below comes from the engine itself
(its 337 commands and their parameter schemas, and what `inspect` reports) and from the guide.

## What a model writes

A model never writes the file bytes. It calls the kit's agent tools (prefix `sheet`), and the
engine applies each change, keeps it in undo history and saves the file:

- `sheet_inspect`: read the document; `sheet_commands`: find commands by keyword;
- `sheet_run`: run steps `{command, params}` in order;
- `sheet_check`: list problems; `sheet_query` / `sheet_batch`: read or set objects of the object model.

## Syntax

A step is one engine command with its parameters, as JSON:

```json
{"command": "<command id>", "params": {"<name>": <value>}}
```

`sheet_run` takes a list of steps and runs them in order, stopping at the first failure. Parameter
names, types and required fields come from each command's schema; `sheet_commands` returns them.
Commands by namespace (the part of the id before the first dot):

| Namespace | Commands | Change the document | Open to the agent | Examples |
| --- | ---: | ---: | ---: | --- |
| `accessibility` | 1 | 0 | 1 | `accessibility.check` |
| `addIn` | 1 | 0 | 1 | `addIn.manage` |
| `app` | 1 | 0 | 1 | `app.commands` |
| `arrange` | 6 | 5 | 6 | `arrange.bringForward`, `arrange.sendBackward`, `arrange.selectionPane` |
| `artText` | 1 | 1 | 1 | `artText.insert` |
| `cell` | 11 | 10 | 11 | `cell.set`, `cell.formatText`, `cell.get` |
| `chart` | 14 | 14 | 14 | `chart.set`, `chart.switchRowColumn`, `chart.delete` |
| `checkbox` | 1 | 1 | 1 | `checkbox.insert` |
| `column` | 7 | 7 | 7 | `column.setWidth`, `column.autofitWidth`, `column.setDefaultWidth` |
| `command` | 1 | 1 | 1 | `command.runLine` |
| `comment` | 4 | 4 | 4 | `comment.add`, `comment.reply`, `comment.resolve` |
| `conditionalFormat` | 3 | 3 | 3 | `conditionalFormat.apply`, `conditionalFormat.clearRules`, `conditionalFormat.manageRules` |
| `data` | 32 | 28 | 32 | `data.sortAscending`, `data.sortDescending`, `data.sort` |
| `drawing` | 14 | 13 | 14 | `drawing.ink`, `drawing.erase`, `drawing.describe` |
| `edit` | 26 | 18 | 26 | `edit.selectAll`, `edit.goTo`, `edit.goToSpecial` |
| `file` | 13 | 6 | 13 | `file.new`, `file.open`, `file.save` |
| `format` | 34 | 34 | 34 | `format.bold`, `format.italic`, `format.underline` |
| `formula` | 23 | 6 | 23 | `formula.autoSum`, `formula.insertFunction`, `formula.functions` |
| `history` | 1 | 0 | 1 | `history.list` |
| `icon` | 1 | 1 | 1 | `icon.insert` |
| `image` | 1 | 1 | 1 | `image.insert` |
| `ink` | 10 | 2 | 10 | `ink.pen`, `ink.pencil`, `ink.highlighter` |
| `link` | 1 | 1 | 1 | `link.insert` |
| `name` | 4 | 3 | 4 | `name.define`, `name.delete`, `name.manage` |
| `note` | 2 | 2 | 2 | `note.add`, `note.show` |
| `object` | 3 | 3 | 3 | `object.delete`, `object.move`, `object.setAnchorMode` |
| `page` | 12 | 12 | 12 | `page.orientation`, `page.size`, `page.margins` |
| `pivot` | 21 | 18 | 21 | `pivot.fields`, `pivot.fieldList`, `pivot.list` |
| `range` | 2 | 2 | 2 | `range.setValues`, `range.fill` |
| `row` | 6 | 6 | 6 | `row.setHeight`, `row.autofitHeight`, `row.hide` |
| `script` | 5 | 4 | 5 | `script.record`, `script.new`, `script.list` |
| `select` | 7 | 0 | 7 | `select.set`, `select.move`, `select.next` |
| `shape` | 2 | 2 | 2 | `shape.setText`, `shape.insert` |
| `sheet` | 13 | 9 | 13 | `sheet.rename`, `sheet.move`, `sheet.tabColor` |
| `sparkline` | 1 | 1 | 1 | `sparkline.insert` |
| `spelling` | 4 | 1 | 4 | `spelling.check`, `spelling.addToDictionary`, `spelling.change` |
| `style` | 2 | 2 | 2 | `style.apply`, `style.add` |
| `symbol` | 1 | 1 | 1 | `symbol.insert` |
| `table` | 15 | 15 | 15 | `table.totalRow`, `table.bandedRows`, `table.bandedColumns` |
| `textBox` | 1 | 1 | 1 | `textBox.insert` |
| `theme` | 4 | 4 | 4 | `theme.set`, `theme.colors`, `theme.fonts` |
| `view` | 22 | 9 | 22 | `view.freezePanes`, `view.freezeTopRow`, `view.freezeFirstColumn` |
| `window` | 1 | 0 | 1 | `window.activate` |
| `workbook` | 2 | 1 | 2 | `workbook.protect`, `workbook.statistics` |

### The workbook

- A workbook has **sheets** (`details.sheets` in `sheet_inspect`); one is active. Commands act on the
  active sheet unless they take `sheet`.
- `sheet_inspect` answers the active sheet's used range as `content.rows`: each row starts with its
  1-based row number, then the cell **values**. A formula cell shows its computed value, not its
  formula: `[5, "Total", 300.0]` may be `=SUM(B2:B4)`.
- To see formulas, run `sheet.read` with `"formulas": true`. Without it, `sheet.read` answers values.
- Cells are addressed in **A1 notation**: `B2`, ranges `A1:D10`, other sheets `Data!B2`. Rows count
  from 1 and columns from A, as in Excel. There are no 0-based cell indexes in the commands.

### Writing cells

- `range.setValues { "range": "C1", "values": [[...], ...] }` writes rows starting at the range's
  top-left cell. Numbers, strings, booleans and `null`; a string that starts with `=` is a formula.
  This is the fastest way to write a block.
- `cell.set { "cell": "B2", "input": "..." }` enters one cell as if typed. `input` is always a string:
  `"42"` and `"=A1*2"` are a number and a formula, typed the way a person would.
- Write a **formula** when the value depends on other cells (totals, ratios, lookups). Write a
  number only when the person gave a number. A total typed as `300` instead of `=SUM(B2:B4)` stops
  updating.

## Text writing format

Besides single commands, the engine takes whole text in one step:

- `data.fromTextCsv` `{path | text, delimiter?: ",", sheet?: new sheet name} → imports into a new sheet as a table`: Imports CSV or delimited text (from a path or inline text) into a new sheet as a table.

## Defaults

What a new, empty document starts with:

- File name `Untitled sheet.xlsx`, size 1710 × 2021 (`kit.json` `creates`).

What `sheet_inspect` reports for a new document made at that size (generated from the engine; lists shortened to 20 entries):

```json
{
  "active": "A1",
  "canRedo": false,
  "canUndo": false,
  "chart": null,
  "content": "",
  "editing": false,
  "selection": "A1",
  "shape": null,
  "shapeEditing": false,
  "shapes": [],
  "sheet": "Sheet1",
  "sheets": [
    "Sheet1"
  ],
  "style": {
    "align": {
      "h": "General",
      "indent": 0,
      "rotation": 0,
      "shrink": false,
      "v": "Bottom",
      "wrap": false
    },
    "borders": {
      "bottom": {
        "color": "Auto",
        "style": "None"
      },
      "diag_down": {
        "color": "Auto",
        "style": "None"
      },
      "diag_up": {
        "color": "Auto",
        "style": "None"
      },
      "left": {
        "color": "Auto",
        "style": "None"
      },
      "right": {
        "color": "Auto",
        "style": "None"
      },
      "top": {
        "color": "Auto",
        "style": "None"
      }
    },
    "fill": {
      "bg": "Auto",
      "fg": "Auto",
      "pattern": "None"
    },
    "font": {
      "bold": false,
      "color": "Auto",
      "italic": false,
      "name": "Calibri",
      "size": 11,
      "strike": false,
      "underline": "None",
      "vert": "Baseline"
    },
    "num_fmt": "General",
    "protection": {
      "hidden": false,
      "locked": true
    }
  },
  "table": null,
  "title": "Book1",
  "used": null
}
```

## Themes, layouts and other named values

Parameters that take one of a fixed list of names (themes, layouts, presets, styles, kinds, modes,
units and similar), from the command schemas. A parameter not listed here takes free values; its
schema says which.

| Parameter | Values | Used by |
| --- | --- | --- |
| `align` | `left`, `center`, `right`, `top`, `middle`, `bottom` | `arrange.align` |
| `dateUnit` | `day`, `weekday`, `month`, `year` | `edit.fillSeries` |
| `direction` | `columns`, `rows` | `edit.fillSeries` |
| `errorStyle` | `stop`, `warning`, `information` | `data.validation` |
| `kind` | `chart`, `image`, `shape` | `arrange.align`, `arrange.bringForward`, `arrange.group`, `arrange.sendBackward` and 3 more |
| `kind` | `shape` | `arrange.rotate`, `chart.shapeFill`, `chart.shapeOutline` |
| `kind` | `blanks`, `constants`, `formulas`, `comments`, `lastCell`, `currentRegion`, `visible`, `conditionalFormats`, `dataValidation`, `errors`, `numbers`, `text` | `edit.goToSpecial` |
| `kind` | `rectangle`, `roundedRectangle`, `ellipse`, `triangle`, `line`, `arrow`, `textBox` | `shape.insert` |
| `kind` | `shape`, `textBox`, `chart` | `drawing.insert` |
| `layout` | `compact`, `outline`, `tabular` | `pivot.layout`, `pivot.reportLayout` |
| `mode` | `series`, `copy`, `formats`, `values` | `edit.autoFill` |
| `mode` | `moveOnly`, `moveAndSize`, `absolute` | `image.insert` |
| `mode` | `moveAndSize`, `moveOnly`, `absolute` | `object.setAnchorMode` |
| `mode` | `automatic`, `automaticExceptTables`, `manual` | `formula.calculationOptions` |
| `mode` | `off`, `on`, `rows`, `columns` | `pivot.grandTotals` |
| `mode` | `none`, `top`, `bottom` | `pivot.subtotals` |
| `preset` | `bottom`, `top`, `left`, `right`, `none`, `all`, `outside`, `thickOutside`, `thickBottom`, `doubleBottom`, `topBottom`, `topThickBottom`, `topDoubleBottom`, `insideHorizontal`, `insideVertical`, `inside`, `diagonalDown`, `diagonalUp` | `format.borders` |
| `preset` | `normal`, `wide`, `narrow` | `page.margins` |
| `preset` | `straightConnector1`, `bentConnector3`, `curvedConnector3` | `drawing.connect` |
| `style` | `single`, `double`, `singleAccounting`, `doubleAccounting` | `format.underline` |
| `subtype` | `clustered`, `stacked`, `stacked100`, `markers` | `chart.insert` |
| `type` | `linear`, `growth`, `date`, `autofill` | `edit.fillSeries` |
| `type` | `column`, `bar`, `line`, `pie`, `doughnut`, `area`, `scatter`, `bubble`, `radar`, `combo`, `histogram`, `waterfall`, `funnel`, `treemap`, `sunburst`, `boxWhisker`, `stock` | `chart.insert` |
| `type` | `line`, `column`, `winLoss` | `sparkline.insert` |
| `type` | `cellIs`, `expression`, `containsText`, `beginsWith`, `endsWith`, `blanks`, `noBlanks`, `errors`, `noErrors`, `duplicate`, `unique`, `top10`, `aboveAverage`, `timePeriod`, `colorScale`, `dataBar`, `iconSet` | `conditionalFormat.apply` |
| `type` | `any`, `whole`, `decimal`, `list`, `date`, `time`, `textLength`, `custom` | `data.validation` |
| `vertAlign` | `superscript`, `subscript`, `baseline` | `cell.formatText` |

## Examples

### Example 1: add a target column and a share formula

The workbook's sheet `Data` has `Region` / `Sales` in A1:B1, three regions in rows 2–4, and
`Total` with `=SUM(B2:B4)` in row 5.

```json
sheet_run {
  "path": "sales.xlsx",
  "steps": [
    { "command": "range.setValues", "params": { "range": "C1", "values": [["Target"], [100], [100], [100], ["=SUM(C2:C4)"]] } },
    { "command": "range.setValues", "params": { "range": "D1", "values": [["Share"], ["=B2/C2"], ["=B3/C3"], ["=B4/C4"], ["=B5/C5"]] } }
  ]
}
```

Each step answers `{ "cells": 5 }`. Then read the values back:

```json
sheet_run { "path": "sales.xlsx", "steps": [{ "command": "sheet.read", "params": { "range": "D2:D5" } }] }
```

It answers `"values": [[1.2], [0.8], [1.0], [1.0]]`, and `sheet_check` finds nothing wrong.

### Example 2: fix a division by zero the check reports

After `cell.set { "cell": "C3", "input": "0" }`, `sheet_check` answers:

```json
{ "diagnostics": [{ "code": "sheet.formula-error", "severity": "error",
  "message": "Data!D3 evaluates to #DIV/0!: =B3/C3", "target": { "sheet": "Data", "cell": "D3" } }] }
```

Guard the formula instead of deleting it:

```json
sheet_run {
  "path": "sales.xlsx",
  "steps": [{ "command": "cell.set", "params": { "cell": "D3", "input": "=IF(C3=0,\"\",B3/C3)" } }]
}
```

`sheet_check` then answers `the kit found nothing wrong`.

## Workflow

1. `sheet_inspect` to see the sheets, the used range and the values, plus a preview.
2. Find the command: `sheet_commands { "query": "set values" }`. Use specific words (`merge cells`,
   `number format`, `sort range`); a broad query answers only the first 12 of many. Follow each
   command's parameter schema; don't invent command ids.
3. `sheet_run` small batches of steps. A batch saves when it succeeds; formulas recalculate
   immediately.
4. `sheet_run` `sheet.read` on the range you changed (`"formulas": true` to see formulas), and
   `sheet_check`, then fix what it reports. Each diagnostic names its `sheet` and `cell` (or range),
   and many carry a `fix`.

## Pitfalls

- **Values vs formulas:** `sheet_inspect` and plain `sheet.read` show computed values. Read with
  `"formulas": true` before you change a formula.
- **Errors:** a formula that ends in `#DIV/0!`, `#VALUE!` or `#REF!` is `sheet.formula-error`
  (error); `#N/A` is reported as a warning, since lookups use it on purpose. A formula that refers
  to itself is `sheet.circular-reference`.
- **Reading past the data:** `sheet.reference-outside-data` (warning) means a formula reads one
  empty cell beyond the sheet's data, often a typo (`Z900` instead of `B9`). Ranges such as
  `SUM(A1:A1000)` are not reported.
- **Fonts:** `sheet.font-substituted` (info) names a font the renderer draws in its built-in face
  (add the font, or for an Office font its stand-in: Carlito for Calibri);
  `sheet.missing-glyph` (warning) names characters no face draws, shown as empty boxes.
- **Merges:** a merged range shows only its top-left value. `sheet.merge-hides-data` names a merge
  that hides other values; its `fix` is `cell.unmerge` on that range.
- **Defined names:** `sheet.broken-name` is a name that points nowhere (`#REF!`, a deleted sheet);
  fix the name's formula or delete it.
