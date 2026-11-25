# Sheet kit

The Sheet kit opens, edits and checks Excel workbooks (`.xlsx`) with Excel's commands and formula
engine. An agent can read and write cells and formulas, format, sort and filter ranges, work with
tables, charts and pivot tables, check the workbook for formula errors and broken names, and export
it to PDF, CSV or HTML.

## Tools

The kit's prefix is `sheet`. Its tools are:

| Tool | Does |
|---|---|
| `sheet_inspect` | The workbook as JSON (its sheets, the used range and the values), with a preview |
| `sheet_query` | Reads objects of the workbook's object model by id, or by following fields from the root |
| `sheet_commands` | Finds commands by keywords, each with its parameter schema |
| `sheet_run` | Runs a list of command steps |
| `sheet_batch` | Runs one batch of object-model operations |
| `sheet_check` | What is wrong with the workbook, as diagnostics |
| `sheet_create` | Makes a new `.xlsx` workbook |
| `sheet_export` | Writes the workbook to a new PDF, CSV or HTML file |
| `sheet_draft_ready`, `sheet_draft_reopen` | Asks the person to review your draft, or takes it back to edit |

Every tool takes `path`, the workbook's path in the workspace. With `draft: true` your edits go to
your draft of the workbook and leave the file as it is; only the person merges or discards a draft.

The kit reads and writes only the workbook Kition opened for you. Commands that name a file by
path (`file.open`, `file.save`, `data.getData`, `image.insert`, …) are not offered: enter data
with `range.setValues`, or CSV text with `data.fromTextCsv {text}`, and write files with
`sheet_export`. A stored script (`script.run`) runs only steps you could run yourself; one
that names a file is refused before any step runs. Spelling (`spelling.check`) is not available.

## Addressing, ids and coordinates

- **Sheets have ids.** `sheet_inspect` lists every sheet in `details.workbook.sheets`, whatever
  sheet the person has open: `{"id": 2, "toolId": "sheet:2", "name": "Notes", "index": 1, "hidden":
  false, "used": "A1:D9", "drawings": [{"id": 5, "toolId": "drawing:5", "kind": "chart", "at":
  "E2"}]}`. The id stays when the sheet is renamed or moved and when the file is saved and opened
  again; the name and the position (`index`, from 0) do not. Name a sheet by its tool id
  (`"sheet:2"`) or its name, never by its position.
- **Drawing objects have ids.** Shapes, text boxes, groups, charts and pictures are
  `drawing:<id>`, unique in the workbook and kept through save and reopen. Groups are member
  sets with a read-only identity object, without a stored parent transform. Commands' `id`, `ids`
  and `chart` take the tool id (`"drawing:5"`), and a tool id names its sheet.
- **Cells are addressed in A1 notation** on a sheet: `B2`, `A1:D10`, a cell of another sheet
  `Data!B2` (`'My Sheet'!B2` when the name has spaces). Rows count from **1** and columns from
  **A**, as in Excel; no command takes a 0-based cell index. An address is a position: inserting
  or deleting rows or columns moves the cells after them, and their addresses with them. A
  receipt names the rows or columns created or deleted, on which sheet, at which index.
  It currently also lists changed cell address/label metadata; read the range again after a shift.
  Object-model cells use positional tool addresses (`cell:1!B7`, with the sheet id), and labels
  such as `Data!B7`. These are addresses, not identities that follow cell content across a shift.
- **Defined names** have `name:<id>` tool ids, preserved on rename and save/reopen.
  Names, row/column identities, deleted axis markers and cell provenance are stored in the
  workbook's Kition identity part. Row/column ids follow the axis; a cell tool address remains positional.
- **Tables and pivots** have `table:<sheetId>:<id>` and `pivot:<sheetId>:<id>` tool ids.
  A sheet holds its features as objects: `tables` (each `Table` with its `columns`, one
  `TableColumn` each), `pivots`, `merges`, `condFormats`, `validations` and `sparklines`, and its
  filter in `autofilter`; their ranges name rows and columns by their objects. They are written
  by the kit's commands only (read-only to batch writes); `groups` is a read-only index of the
  shapes' groups. Commands accept table/pivot tool ids through rename and save/reopen.
- **Comments** are `Comment` objects in the sheet's `comments` map, keyed by the cell's row and
  column objects: `author`, `text`, `replies` (each a `Reply` with `author` and `text`),
  `threaded`, `resolved` and `visible`. Workbook `styleObjects` names immutable interned styles by
  content hash (`style:<hash>`), named cell styles by definition name (`style:cell:<hash>`), and
  named table styles by their native UID (`style:table:<uid>`). Table-style rename preserves
  that UID; redefining a cell style preserves its id. Commands that apply or modify existing
  styles take those named tool ids. Cell styles have no rename command. Named-style keys follow
  the engine's ASCII case comparison, so distinct non-ASCII names remain independent.
- **Commands' `sheet`** takes a sheet's name or tool id (or a position from 0, which changes when
  sheets move: avoid it). A range with a sheet (`Data!B2`) names its sheet too.
- `sheet_inspect` answers a sheet's values as `content.rows`: each row starts with its row number,
  counted from 1, then the cell **values**. A formula cell shows its computed value, not its
  formula: `[5, "Total", 300.0]` may be `=SUM(B2:B4)`. Without `target` it shows the start of the
  sheet the file opens on; `target: {"sheet": "sheet:2", "range": "A1:F40"}` shows that sheet and
  range without making the sheet active.
- Diagnostics name their `sheet` and `cell` (or range) in `target`.

## Where a step acts: `target`, `at` and `on`

Nothing a call does depends on, or leaves behind, the sheet or cells the person has active or
selected: name them.

- A `sheet_run` step is `{command, params?, on?, at?}`. The run's `target` (`{"sheet": …}`) is the
  sheet every step acts in; a step's `at.part` (`{"sheet": …}`) overrides it for that step. A
  sheet is `{"sheet": 2}` (its id), `{"sheet": "sheet:2"}` or `{"sheet": "Notes"}`; never a
  position.
- `on` names what the step acts on: **cells** as A1 ranges (`["B2:D10"]`, or `["Notes!B2:D10"]`,
  whose sheet must be the step's) **or drawing objects** by id (`["drawing:5"]`), never both in one
  step. It goes into the command's own parameter for it (`range`, `cell`, `ids`, `id` or `chart`:
  `sheet_commands` with `detail` marks it as the command's `targets`; then do not give that
  parameter too), or, for a command that acts on the selection (`reads: selection`), it is
  selected for that step only. Cells in `on` without a sheet need `target` or `at.part` when the
  workbook has more than one sheet. A command whose parameter takes one range takes one `on` item.
- A command given its sheet or range (`sheet`, `Data!B2`, a drawing's tool id) acts there and
  leaves the active sheet and the selection as they were, also when it fails. Only the commands
  whose work is moving the editor (`sheet.activate`, `select.set`, `view.*`) move it, and the
  editor is put back after the call: a later call starts again from the person's sheet.
- A sheet you insert (`sheet.insert`) is where the next steps of the same run land when they
  name no sheet, so "insert a sheet, then fill it" works in one run; name it in later calls.
- The file saves the person's active sheet and cell, never where the run worked.

## Previews of a named range

`sheet_inspect` with `"preview": true` draws `target`: `{"sheet": "sheet:2", "range": "A1:H30"}`
(the sheet's name or tool id; the range in A1). Without a sheet it draws the first sheet, never the
active one; without a range, the sheet's used range, cut to 5000 cells (`preview.capped` says so,
with `usedRange`). The picture has the row numbers and column letters, so read addresses off it.
`scale` (at most 4) and `maxSize` (the longest side in pixels) set its size. A range of more than
5000 cells is refused: ask for a smaller one, and read values with `sheet.read`. The same call
answers the same picture.

## What a call changed, and what others changed

- **Receipts.** A successful `sheet_run` or `sheet_batch` answers `changes`: `created`, `changed`
  (with `edits` and available field `before`/`after` values), `moved` and `deleted`, naming sheets
  (`sheet:2`) and drawing objects (`drawing:5`) by the ids `sheet_inspect` lists, and
  `diagnostics: {new, resolved}`. Cell edits name their positional cell object; read the range
  back with `sheet.read` to see the values. Inserted rows have `row:` ids and an `at` location
  with their parent sheet and index. Positional cell address/label edits accompany the shift.
- **Pass the latest `revision` as `basis`.** Every answer of inspect, query, run and batch has a
  short `revision`. Pass the last one you were given as `basis` on your next call: the answer then
  says what others (people, other agents, changes made in the file) changed since, in `others`
  (with `by`).
- **A `conflict`** (retryable) means others changed what the write acts on since your `basis` (it
  was `deleted`, `relocated` or `changed`, or the file changed in a way the server cannot describe
  object by object, `unknown`). Nothing was run. Read `others` and `conflicts`, inspect again if you
  need to, decide again, and send the call with the `revision` the refusal gives as `basis`. Do not
  undo others' changes. The same-field decision includes the value an absolute assignment
  replaces. Another unassigned field is unrelated to that assignment; the coverage limits below
  describe remaining conservative premises. Your own earlier edits are accounted for when
  checking premises. If rereading the file deletes the observed recipient, a stale write
  refuses `deleted` even when the reread was triggered by your own call.

## Premise coverage and limits

The engine uses the public kition-sdk revision pinned in its Cargo manifest.
Commands declare typed references, editor reads, source and destination values,
and address windows. Bounded entry, formatting, table, filter, outline and drawing
paths have narrower holds; complex operations retain conservative workbook holds.
Those holds can refuse changes outside the actual inputs. The fact comparison
projects every retained handler-reading path into content, identity, position
and address facts and compares both directions against its declaration.
Every registered write is represented. The binding entry supplement is labelled
separately from the original blind families. Missing facts are rejected;
conservative excess holds have decreasing per-path and per-class limits.

Receipts include proven public derived changes. Private comparison facts remain
available to conflict checks; their names and values are excluded from public
creation, change and deletion receipts.
Replica commands that replace the workbook continue with the replacement.
Conditional-rule clears compare historical coverage even when an outside axis shift
moves a previously selected rule out of the requested range.

Picking up the format painter holds its source range rather than unrelated drawing
objects. Setting an object's anchor mode holds the observed mode while permitting
geometry changes. Arithmetic formula entry holds source values without holding their display
formatting; metadata queries such as `CELL` retain the formats they read.

Automatic row fitting holds height contributions, wrapping inputs and assigned styles;
plain neighboring values and colors can remain unrelated. Checkbox creation on a named
sheet follows that sheet through unrelated tab insertion. Alignment holds source frames
and the row and column prefixes used to convert its result anchors, while permitting
unrelated fill changes and geometry edits beyond those prefixes.
Multi-cell Bold retains a conservative whole-row guard. Duplicate removal also
holds comparison columns outside its table, and image alignment holds image anchors.

Absolute assignments hold the observed value they replace. When a colleague
changes that field after observation, assigning it again refuses `changed`.
A field the command preserves can remain unrelated. An address whose occupant
moves refuses `relocated`; deletion of the observed recipient refuses `deleted`.
Reobserve and decide again after either refusal.

Formula entry holds referenced cells, names, table definitions and the address
windows needed to interpret them. Source creation in a previously empty cell is
also a change. Positional insertion windows detect removals and moves before a
gap while allowing changes after an addressed window.

`chart.shapeFill` has conditional parameters: use `id` with
`kind:"shape"`, and `chart` for the chart-series branch. Its ambiguous
`on` form with `kind:"shape"` is refused. Chart-series `on` remains supported.
Explicit tool IDs remain the clearest way to name the
recipient.

Outside file rereads reconcile the canonical file model into the retained
replica. Persisted row and object identities, editor state and undo histories
survive that import. A deleted active sheet or selected drawing refuses rather
than selecting an unrelated replacement. Undo after reobservation reverses the
agent's own edit while retaining unrelated outside edits.

Planning dependent steps uses an isolated replica with editor state, blobs,
clipboard content and the captured Format Painter source.
Earlier creation and selection steps determine later steps' premises. The
planned copy is discarded before the real write. Unsupported or unprovable
inputs can still refuse conservatively.

Owning cell maps declare their member type, translate references to public tool
IDs and participate in environment snapshots. Sampling excludes image
containers. Environment replay supplies the recorded identity bindings when a
creation allocates fresh IDs.

Raw object-model operations declare premise contracts. Raw `Cell.input`
assignments use `cell.set`'s source premises, while execution remains a raw
assignment followed by model settling. Use commands for semantic operations;
read-only identity indexes remain read-only.

A successful close can return `{closed:true}` without a document view. A
receipt edit may omit `before` or `after` when that value is unavailable;
an absent value does not mean JSON `null`. Query the affected range or object
when a complete value is needed.

## Workflow

1. Inspect the workbook and retain its revision.
2. Find the command and check its parameter schema.
3. Run the intended steps with explicit sheets or object IDs and the revision as
   `basis`.
4. Read back the affected range or objects, including formulas when relevant.
5. Run `sheet_check` and resolve relevant diagnostics.

## Common tasks

- [`range.setValues`](/kits/sheet/commands/range.setValues/) `{ "range": "C1", "values": [[...], ...] }`
  writes rows starting at the range's top-left cell. Values are numbers, strings, booleans and
  `null`; a string that starts with `=` is a formula. It is the fastest way to write a block.
- [`cell.set`](/kits/sheet/commands/cell.set/) `{ "cell": "B2", "input": "..." }` enters one cell as
  if typed. `input` is always a string: `"42"` and `"=A1*2"` are a number and a formula, typed the
  way a person would.
- Write a **formula** when the value depends on other cells (totals, ratios, lookups). Write a
  number only when the person gave a number. A total typed as `300` instead of `=SUM(B2:B4)` stops
  updating.
- [`sheet.read`](/kits/sheet/commands/sheet.read/) answers values; with `"formulas": true` it
  answers formulas.

## Example 1: add a target column and a share formula

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

## Example 2: fix a division by zero the check reports

After [`cell.set`](/kits/sheet/commands/cell.set/) `{ "cell": "C3", "input": "0" }`, `sheet_check`
answers:

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

## Example 3: format a range on a named sheet without activating it

`sheet_inspect` lists `{"id": 2, "toolId": "sheet:2", "name": "Notes", …}`, and the person has the
`Summary` sheet open. Bold Notes' header row and give it a fill:

```json
sheet_run {
  "path": "budget.xlsx", "basis": "qx7", "target": { "sheet": "sheet:2" },
  "steps": [
    { "command": "format.bold", "on": ["A1:D1"], "params": { "on": true } },
    { "command": "format.fillColor", "on": ["A1:D1"], "params": { "color": "#DDEBF7" } }
  ]
}
```

`on` goes into each command's `range`. The person stays on `Summary`, and the file opens there.
Check it with `sheet_inspect {"target": {"sheet": "sheet:2", "range": "A1:D5"}, "preview": true}`.

## Example 4: edit two sheets in one run

```json
sheet_run {
  "path": "budget.xlsx",
  "steps": [
    { "command": "range.setValues", "at": { "part": { "sheet": "Data" } },
      "params": { "range": "A5", "values": [["Total", "=SUM(B2:B4)"]] } },
    { "command": "cell.set", "at": { "part": { "sheet": "Notes" } }, "on": ["A1"],
      "params": { "input": "Checked" } }
  ]
}
```

Each step lands on its own sheet; nothing else moves. The same with ranges that name their sheet:
`{"command": "range.setValues", "params": {"range": "Data!A5", "values": [...]}}`.

## Pitfalls

- **Values or formulas:** `sheet_inspect` and a plain [`sheet.read`](/kits/sheet/commands/sheet.read/)
  show computed values. Read with `"formulas": true` before you change a formula.
- **Name the sheet:** a step with no `target`, `at.part`, `sheet` or `Sheet!` range acts on the
  sheet the file opens on (or one the run inserted). Name the sheet when the workbook has more than
  one; `sheet.activate` is never needed to edit another sheet.
- **Positions move:** a sheet's `index` changes when sheets move, and A1 addresses below inserted
  or deleted rows change; ids (`sheet:2`, `drawing:5`) do not.
- **Merges:** a merged range shows only its top-left value.

## Checks

`sheet_check` lists what is wrong with the workbook. Each diagnostic has a stable `code`, a
`severity` (`error`, `warning` or `info`), a `message`, a `target` and, for some, a `fix`: a
command step to run as it is. Act on the code, not on the message. A `fix` is a suggestion: run it
only when it is what the person asked for. Every code the kit reports is on the
[checks page](/kits/sheet/checks/).

- `sheet.formula-error` (error): a formula ends in `#DIV/0!`, `#VALUE!` or `#REF!`. `#N/A` is
  reported as a warning, since lookups use it on purpose. Fix the formula or guard it.
- `sheet.circular-reference` (error): a formula refers to itself.
- `sheet.broken-name` (error): a defined name points nowhere (`#REF!`, a deleted sheet). Fix the
  name's formula or delete it.
- `sheet.merge-hides-data` (warning): a merge hides other values. Its `fix` is
  [`cell.unmerge`](/kits/sheet/commands/cell.unmerge/) on that range.
- `sheet.reference-outside-data` (warning): a formula reads one empty cell beyond the sheet's data,
  often a typo (`Z900` instead of `B9`). Ranges such as `SUM(A1:A1000)` are not reported.
- `sheet.missing-glyph` (warning): a cell's text has characters no font the renderer has draws
  (✓ and ✗ in Carlito); they show as empty boxes. Use other characters or add a font that has them.
- `sheet.font-substituted` (info): cells are set in a font the renderer has no face for, so their
  text takes other widths than in Excel; the host adds the font (for Calibri, Carlito).
