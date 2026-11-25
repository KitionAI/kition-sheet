# Formula language

> **Last updated:** 2026-11-17

## Entering formulas (the kit's editor, as in WPS)

- **Point mode.** While a formula is typed in a cell and the caret is where a reference can go
  (after `=`, an operator, `(`, `,` or `:`), clicking a cell inserts its reference, dragging
  inserts a range, Shift+click widens it, and a column or row heading inserts the whole column or
  row; the range has a dashed frame. In Enter mode (typing over a cell, not F2) the arrow keys move
  the pointed cell from the cell being typed and Shift+arrows widen it. Another click right after
  replaces the reference; typing anything else ends pointing.
- **Another sheet.** Clicking a sheet tab (or Ctrl+Page Up/Down) while a formula waits for a
  reference keeps it open: that sheet's cells are pointed at with its (quoted) name, the formula
  bar shows the formula, and Enter puts it in its own cell and shows its sheet again; Escape drops
  it and returns. Text that is no formula is entered when the tab is clicked, as before.
- **F4** cycles the reference at (or just before) the caret: `A1` → `$A$1` → `A$1` → `$A1` → `A1`,
  both ends of a range together, in the cell and in the formula bar (`editor.cycleReference`).
- **Function list and argument hint.** Typing a function name lists the functions starting with
  it, with the chosen one's syntax and description; Up and Down choose, Tab or a click inserts
  `NAME(` (`editor.acceptFunction`). Inside a call, a hint shows its syntax with the current
  argument in bold. Both work in the cell and in the formula bar. The descriptions of the most used
  functions are in Chinese in a zh-CN editor (`fn:NAME` in `web/src/strings.ts`).
- **The formula bar is the cell.** What is typed in the bar is the cell being typed
  (`editor.formula {text, start, end}`): the cell shows it as it is typed, and while a formula
  waits for a reference a click or drag on the grid (or another sheet, the Name Box then showing
  the formula's cell) inserts the reference at the bar's caret and the bar takes the keyboard back.
  Enter enters and moves down, Shift+Enter up, Tab right, Shift+Tab left, ✓ enters and stays
  (`enter`), Escape and ✕ drop it; leaving the bar for the grid keeps the typing. Alt+Enter
  (Option+Return) breaks the line in the bar and in the cell (the cell editor shows the lines; the
  cell wraps). The bar expands to several lines (its button, Ctrl+Shift+U).
- **Insert Function (fx, Formulas › Insert Function).** Search for a function or choose a
  category (常用函数 first), see its syntax and description, and OK puts `NAME(` at the caret of
  the formula being typed, or starts `=NAME(` in the active cell; cancelling keeps what was typed.
  F2 (`edit.beginEdit`) edits the active cell in place.
- Escape cancels; a misspelt function shows `#NAME?`; the formula bar shows the formula while the
  cell shows its value; text from an input method (Chinese) goes through the editor's text sink.
- A circular reference shows 0 (WPS); Formulas › Error Checking lists its cells. With iterative
  calculation on (`formula.calculationOptions` `iterative`, `maxIterations`, `maxChange`, saved in
  the file's `calcPr`) it settles to a number.
- Not yet: a dialog on a new circular reference, WPS's 函数参数 (Function Arguments) dialog after
  Insert Function (the argument hint shows instead), and Chinese descriptions beyond the most used
  functions.

## Formula language (Spanish)

The formula crate has a Spanish formula syntax for entry and display (`FormulaLocale::Es`,
`parse_input` and `print_input` in `crates/engine/formula/src/input.rs`, tested in
`crates/engine/formula/tests/spanish_input.rs`). The kit's editor does not select it yet: formulas
are typed and shown in English. Other languages have no table and use the English function names
and separators.

| Spanish | Canonical English |
| --- | --- |
| `SUMA` | `SUM` |
| `PROMEDIO` | `AVERAGE` |
| `SI` | `IF` |
| `CONTAR` | `COUNT` |
| `SUMAR.SI` | `SUMIF` |
| `CONTAR.SI` | `COUNTIF` |
| `BUSCARV` | `VLOOKUP` |
| `HOY` | `TODAY` |
| `VERDADERO`, `FALSO` | `TRUE`, `FALSE` (literals or function calls) |

Names are case-insensitive. Spanish formulas use decimal commas (`1,5`), semicolon arguments
and reference unions, and backslashes between array columns (`{1\2;3\4}`). For example,
`=SI(VERDADERO;SUMA(1,5;2,25);0)` evaluates to 3.75. English retains comma arguments,
decimal points and comma array columns; it does not silently translate Spanish aliases.

Workbook/sheet names, table names, LET bindings and LAMBDA parameters take precedence over
aliases. Text, sheet names and table column names are unchanged. When a localized spelling
would collide with a name, displayed formulas retain the canonical spelling to preserve meaning.

Stored formulas, XLSX import and export, engine commands and clipboard exchange use strict
English syntax (`kition_sheet_formula::parse`). The Spanish syntax applies only where a caller
parses or prints at an entry boundary with `parse_input` and `print_input`; it does not change the
workbook format or teach the calculation engine ambiguous aliases.
