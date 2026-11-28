# Gaps

> **Last updated:** 2026-11-27 · **Source:** `features.yaml` (updated 2026-11-27) and the engine's command list

Every known shortfall of Kition Sheet, from the feature catalogue: features that are not yet `has`,
then the differences recorded on features that work. Update the feature in `features.yaml` when you
land work, then this list. The comparison with the target application is in
[parity/target-app-parity.md](parity/target-app-parity.md); the order of work is in [roadmap.md](roadmap.md).

## Features not done

- `sheet.agent.chart-premises` (P0, partial): Source-dependent chart writes guard the cells they read.
- `sheet.agent.env-cells` (P0, partial): Cell environment sampling declares ownership and stable labels. Owning cell maps and image-container exclusions are supported; environment sampling does not establish complete command conformance.
- `sheet.agent.env-replay` (P0, partial): Environment replay binds newly created identities. Replay uses recorded identity bindings for fresh allocations; other command-family limitations remain independently tracked.
- `sheet.agent.format-premises` (P0, partial): Formatting guards destinations and fitted rows.
- `sheet.agent.formula-metadata` (P0, partial): Formula sources hold the visibility geometry and sheet metadata they read.
- `sheet.agent.independent-reading` (P0, partial): Independent handler readings gate premise declarations. Every registered write and retained handler-reading path has a bidirectional fact comparison with decreasing excess-hold limits; conservative excess holds still require review.
- `sheet.agent.named-destination` (P0, partial): Static single-cell names address cell entry.
- `sheet.agent.precise-families` (P0, partial): Bounded command families guard their read and write footprints.
- `sheet.agent.premises` (P0, partial): Writes hold observed addresses and the fields they replace.
- `sheet.agent.axis-premises` (P1, partial): Fixed axis properties hold their fields and positional windows.
- `sheet.cells.selection` (P1, partial): Select cells, rows, columns and regions; go to. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.charts.design` (P1, partial): Chart type, data, layout, elements, colours and formatting. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.data.import` (P1, partial): Import from text and CSV, tables and ranges; queries and refresh. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.data.sort-filter` (P1, partial): Sort, filter, advanced filter and remove duplicates. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.data.tools` (P1, partial): Text to columns, data validation, outline, subtotals and consolidate. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.data.what-if` (P1, partial): Goal Seek, scenarios and data tables. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.drawing.objects` (P1, partial): Drawing objects: describe, insert, transform, format, connect, group, arrange; ink. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.edit.clipboard` (P1, partial): Undo, redo, cut, copy, paste and paste special. Copy and paste by key, and pasting another spreadsheet's HTML table, are shown in the editor; undo, redo and paste special are not, and no case reopens a pasted table after saving.
- `sheet.edit.fill-clear` (P1, partial): Fill down, right, up, left, series, AutoFill and smart fill; clear contents, formats, comments and links. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.edit.find-replace` (P1, partial): Find and replace. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.file.export-print` (P1, partial): Export to CSV, web page and PDF; print and print preview. Export as PDF, CSV and web page run in the editor through the host (regressions/issue-export-through-host); Print and Print Preview have no editor case, and no case reopens an exported file.
- `sheet.file.workbooks` (P1, partial): New, open, save, save as, close and workbook properties. Save and open round trips are tested at the XLSX format level and for single values through the engine; no case reopens a whole workbook through the commands.
- `sheet.format.conditional` (P1, partial): Conditional formatting rules. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.formulas.auditing` (P1, partial): Trace precedents and dependents, show formulas, error checking, evaluation, watch window and calculation. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.formulas.functions` (P1, partial): AutoSum, function library and recently used functions. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.insert.objects` (P1, partial): Insert tables, charts, sparklines, pictures, shapes, icons, text boxes, art text, links, symbols and checkboxes. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.page.setup` (P1, partial): Orientation, size, margins, print area and titles, scaling, breaks, header and footer. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.pivot.tables` (P1, partial): Pivot table fields, layout, totals, grouping, filtering, styles and refresh. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.review.comments` (P1, partial): Notes and threaded comments. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.review.protect-proof` (P1, partial): Cell locking, sheet and workbook protection, statistics, accessibility and spelling. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.tables.design` (P1, partial): Table styles, style options, totals, resizing and conversion. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.automate.scripts` (P2, partial): Record, create, run and delete scripts; add-ins. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.draw.ink-tools` (P2, partial): Draw tab pens, highlighter, eraser, lasso and ink to shape. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.
- `sheet.insert.smart-graphic` (P2, partial): Smart graphics in opened workbooks: kept on save, drawn in the editor (with empty nodes' prompts) and printed. Diagrams can't be inserted or edited (insert.smartGraphic), only kept, moved with inserted rows and columns, drawn and printed; the editor doesn't draw their pictures; text is drawn in the theme's body font, one line per paragraph as the shared reader lays it out.
- `sheet.page.themes` (P2, partial): Themes, theme colours, fonts and effects; sheet background. Engine tests run these commands, but no case shows them working in the editor and surviving save and reopen.

## Differences on working features

- `sheet.cells.entry`: Save and reopen as .xlsx is proven by command for text, numbers, booleans and formulas (arithmetic, SUM, IF, CONCAT); the feature's other commands are covered by engine tests only.
- `sheet.cells.rows-columns`: Save and reopen as .xlsx is proven by command for column width, row height, inserting rows and columns, and hiding columns; the feature's other commands are covered by engine tests only.
- `sheet.format.alignment`: Save and reopen as .xlsx is proven by command for merge and centre, horizontal and vertical alignment, wrap, indent and rotation; the feature's other commands are covered by engine tests only.
- `sheet.format.font`: Save and reopen as .xlsx is proven by command for bold, italic, double underline, strikethrough, font, size, font colour, fill and borders; the feature's other commands are covered by engine tests only.
- `sheet.format.number`: Save and reopen as .xlsx is proven by command for accounting, percent, comma and custom number and date formats; the feature's other commands are covered by engine tests only.
- `sheet.formulas.references`: WPS warns about a new circular reference with a dialog; Kition Sheet shows 0 and lists the cells in Error Checking and the workbook check, without a dialog. INDIRECT text naming a renamed sheet is not rewritten (as in WPS and Excel).
- `sheet.formulas.results`: Results that depend on the locale (currency signs, d/m order, weekday and month names) follow Chinese and en-US conventions; the oracle's WPS ran in an English (UK) region, so those rows carry documented zh-CN results. The long tail of statistical and engineering functions is checked by engine tests, not against WPS.
- `sheet.security.workspace`: A hosted workbook has no spelling (it reads no system word list and the kit bundles none), and no command reads a file of the workspace by path; links a workbook holds to other workbooks are dropped on open.
- `sheet.sheets.manage`: Save and reopen as .xlsx is proven by command for inserting, renaming, colouring, moving and hiding sheets; the feature's other commands are covered by engine tests only.
- `sheet.editor.data-validation`: 圈释无效数据 draws no circles in the editor (the command lists the cells); a list from another sheet's range follows the engine's evaluation without WPS's refusal of whole-column sources.
- `sheet.editor.keyboard-and-zoom`: Checked in the development host only: whether Kition Desktop gives the editor the keyboard as a file opens is the host's part. Page Down counts a screenful in rows of the sheet's default height. The zoom is the view's: it is not saved and an agent's inspect() reports the sheet's stored zoom. Cmd+= still zooms in (the sheet tab tests zoom with it), and with only cells selected Cmd+Shift+= and Cmd+- zoom too until stage 7 asks what to insert or delete (WPS W-I8). Find, Replace, Go To and Custom Sort open the commands' parameter forms until stages 4A and 4B. A real mouse notch's delta and a trackpad pinch are checked with synthetic wheel events only.
- `sheet.editor.selection-keys`: On a Mac, kit-editor gives the engine's tool Ctrl+Enter as Cmd+Enter; the kit hands it on as Enter (only typed in the browser tests on Linux, where Ctrl is the main modifier and fills, as in Excel for Windows). Only a column of the default width widens, once: a width the file or a person set, or one widened before, stays. Where Cmd+Home lands with frozen panes and how a merged cell takes part in the round of Enter are Excel's (WPS untested). Cmd+click takes a cell out on the press; a drag from it selects nothing more.
- `sheet.editor.filter-buttons`: 颜色筛选 (by fill or font colour) is kept from files and offered to agents but not in the list; date filters by year, month and day (WPS's 日期筛选 tree) are not offered; columns inserted inside a table do not shift its criteria.
- `sheet.editor.format-font`: Cloud font services (licensed fonts, downloads, uploads, weight submenus), collaborators, cell locks and mobile are not planned; 楷体 draws in Ma Shan Zheng, and the Song and FangSong fonts in Droid Sans Fallback (no open Song or FangSong face in kition-fonts); read-only shows no Home tab, so the font of a cell is not shown to a viewer.
- `sheet.editor.formula-bar`: WPS cloud's separate views (独立视图) are not planned.
- `sheet.editor.insert-content`: WPS cloud's cell components are not planned: ratings (评星), progress bars (进度), task lists (任务清单), date reminders (日期提醒), @mentions, attachments and cloud files. The Symbol dialog has no help entry and does not put the chosen font on the symbol it inserts; rich text in notes (备注) is not offered; cells can't be dragged to move them.
- `sheet.editor.move-cells`: A move onto another sheet by dragging (Alt+drag onto a tab) and a drag that scrolls the grid at its edge are not offered; conditional formats and validation that lie partly inside the block stay where they are (Excel splits them).
- `sheet.editor.rows-columns`: WPS cloud's add-rows-at-the-bottom bar and locked (permission) cells are not planned.
- `sheet.editor.sheet-menu`: The case table's cloud rows (merge sheets, per-visitor sheet locks, download, offline and autosave notices, collaborators, version history) are deferred (docs/gaps.md); a sheet name may hold a comma and the workbook has no fixed sheet limit, as in WPS and Excel.
- `sheet.editor.sheet-tabs`: The cloud-only rows of the case table (collaboration, permissions, autosave status, version history, share links, merge/lock/download sheet) are deferred (docs/gaps.md).
- `sheet.editor.table-styles`: WPS's online “more tablestyles” and New Pivot Table Style are not offered; cloud-only cases (date reminders, attachments, history, collaborators, mobile) are not planned.
- `sheet.editor.view-settings`: WPS cloud's own views (独立视图), collaborators and mobile are not planned; the highlight is kept for the next time (WPS for Mac turns it off when a file is opened again).
- `sheet.editor.workbook-files`: WPS cloud's file features are not planned: upload, sharing, collaboration, version history, permissions, H5 and WeChat preview, the cloud template library. Saving as .xls, .ods or .xlsb is not offered (they open read-only); form controls are kept but not drawn in PDFs and do not move with inserted rows; CSV from the editor carries a UTF-8 byte-order mark (agents choose); Latin text inside Chinese fonts is measured with Arial's widths, so a few wrapped lines break one word differently from WPS.
- `sheet.formulas.names`: Save and reopen as .xlsx is proven by command for defining a name and using it in a formula; the feature's other commands are covered by engine tests only.
- `sheet.review.host-comments`: An empty cell below or right of everything the sheet holds has no row or column object, so rows or columns inserted between the data's end and it do not move its anchor.
- `sheet.view.window`: Save and reopen as .xlsx is proven by command for freezing panes and hiding gridlines; the feature's other commands are covered by engine tests only.

## Catalogue

The catalogue has 87 features: 51 `has`, 36 not yet.
