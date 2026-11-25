# Changelog

Changes to Kition Sheet, newest first. A version is the kit package version in `kit.json`;
each package is published with a `package-v<version>` tag.

## Unreleased

- The engine runs on kition-sdk's plain document model (no CRDT): Kition Desktop is for one person, so workbooks are no longer merged from concurrent edits; replicas take each other's changes in order. Loro is no longer bundled or linked. An unchanged workbook saves to the same bytes, and the file opened again keeps its sheets', rows', columns' and cells' object ids.
- The engine merges drafts (`mergeChanges`, `compareChanges`) and restores earlier versions (`restoreState`, the Kit UI protocol's `replica/restore`) itself, each as one undo step, and answers receipts (`changesSince`), agent replies in tool ids (`modelReply`) and the object model (`modelDescribe`, `modelBatch`) in the editor too (engine contract K3a).
- Editing one cell costs about the same however long the workbook is: the bridge no longer copies every row's and column's id, nor reads every line's position again, for each edit.

## 0.1.4

- The same kit as 0.1.3, built on kition-sdk 6672663 (the replica's undo passes over an undo group whose changes came to nothing): the 0.1.3 package was built on kition-sdk 6b4d471.

## 0.1.3

- A host asks a workbook file for its document id (`document.identity`) and copies a workbook as a new document (`document.fork`): an `.xlsx` is written from the workbook and embeds no replica (its `xl/kition/identities.json` part holds object ids, not a document id or history), so it carries no id and its fork is the same bytes (Kit Process Protocol section 5.6, kition-sdk 6b4d471).
- The headless program speaks Kit Process Protocol version 1 (`headless.protocol` is `kition-kit/1`), as hosts on kition-sdk from bb1b07b require.
- kition-sdk 6672663: the replica's undo passes over an undo group whose changes came to nothing (a value written and written back), and the same Undo takes back the step before it, as before kition-sdk 4b81569.

## 0.1.2

- In a dark host, filter buttons, check boxes, note and comment marks, form controls and chart chrome (the chart area, gridlines, axis lines and automatic text) follow the dark palette instead of staying light; series colours and the colours the file sets are kept. Export as Picture, PDF, print and the saved file still use the light palette.
- `kit.json` declares the engine's editor state as `engine.editorState`, so a host that does not run the engine's Rust code reads `inspect()` without it.

## 0.1.1

- The grid follows the host's dark scheme: in a dark host the cells, gridlines and headings are drawn dark, with automatic text and borders light (text at WCAG AA); the light scheme keeps exactly the colours it had.

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
