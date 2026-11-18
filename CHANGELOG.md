# Changelog

Changes to Kition Sheet, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## 0.1.0

First public release of the source, under the Apache License 2.0. It contains:

- The Rust engine: workbooks of many sheets with values, formulas and Excel's function library;
  references that follow inserts, deletes, copies, sorts and sheet renames; number formats,
  styles, merges, conditional formatting, validation, tables, filters, charts, pivot tables and
  sparklines; `.xlsx` and `.xlsm` read and written, other spreadsheet files read; export to PDF,
  CSV, HTML and PNG.
- The kit engine (`kition-sheet-kit`), native and WebAssembly: the workbook is a KDM replica,
  replicas merge concurrent edits, and undo is per person and per agent. A check catalogue reports
  formula errors, circular references, broken names, merges that hide data and text that does not
  fit.
- The headless kit (`kition-sheet`) for agents over the Kit Protocol: it reads a workbook's sheets
  and cells, names sheets and drawings by stable ids, runs the editor's own commands, previews a range, and works in drafts.
  A write names the revision it read and is refused when someone else changed what it rests on.
  It reads nothing outside its workspace.
- The kit's editor (`web/`): the grid with its ribbon, formula bar and sheet tabs, in kit-editor's
  frame.
- NOTICE and docs/attribution.md list the upstream work, the fonts and every asset, and Loro
  (MIT), bundled by the editor and linked by the engine.
