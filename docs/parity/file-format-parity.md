# File-format parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4

Every format Excel opens or saves, with Kition Sheet's read and write support, fidelity and tests.
Part of [target-app-parity.md](target-app-parity.md). **File formats: ~50% ready, 80–120 h**
(estimated).

Excel's list comes from Microsoft's public "File formats that are supported in Excel"
documentation (the Office bundle is off limits under this repo's clean-room rule). Kition Sheet's
support comes from `crates/engine/engine/src/io.rs` (`FileKind`), `crates/formats/xlsx/src` and the tests in
`crates/formats/xlsx/src/tests/` (round-trip, fixtures, malformed input, CSV, ODS, XLSB, pivot tables).

## Workbook formats

| Format | Excel | Kition Sheet read | Kition Sheet write | Fidelity / notes | Tests |
|---|---|---|---|---|---|
| `.xlsx` Excel Workbook (transitional) | R/W, native | yes | yes | Cells, rich text in cells (runs of shared and inline strings, every run property), shared/array/dynamic formulas, styles (fonts with their character set and theme font reference; the custom indexed palette and recent colours), themes, merges, CF, validation, tables, comments and notes, hyperlinks, names, freeze panes, outline, print settings, pictures, shapes, charts (simplified, see [chart-parity.md](chart-parity.md)), sparklines, pivot tables (caches and tables), protection, calc settings, legacy form controls (VML check boxes, option buttons, lists and their `ctrlProp` parts, kept as they were and written back). Column widths in characters follow the Normal font's digit as WPS for Mac measures it (steps of 0.4 pt); rows without `customHeight` are sized again from their text on open, and a height that is no whole number of pixels is cut to whole points (WPS). **Not preserved:** unmodelled parts and extensions (ActiveX controls, slicer caches, timelines, external links, OLE objects, custom XML, data model, query tables, chart details). Never checked in Excel across a corpus: repair prompts unknown. 100 MB files fail | `roundtrip.rs`, `fixtures.rs`, `malformed.rs`, `pivot_tests.rs`, `rich_tests.rs`, `font_tests.rs`, `checkbox_tests.rs`, engine `tests/workbook_files.rs` |
| `.xlsx` Strict Open XML | R/W | yes | no (writes transitional) | Excel also saves transitional by default | fixtures |
| `.xlsm` Macro-Enabled Workbook | R/W | yes | yes, **without macros** | VBA project dropped with a warning; saving as `.xlsm` loses the macros | — |
| `.xltx` / `.xltm` templates | R/W | yes (opened read-only, as a preview) | `.xltx` (File › Save as Template, `file.export {format:"xltx"}`) | Written with the template content type; "new from template" behaviour missing; `.xltm` not written | `workbook_files.rs`, `gate-file.mjs` |
| `.xlsb` Binary Workbook | R/W | **values only** | no | Names, visibility, numbers, text, booleans, errors, merges, cached formula results; no formulas, formatting, charts, names | `xlsb_tests.rs` |
| `.xls` Excel 97-2003 (BIFF8) | R/W | **yes** (opened read-only; File › Export as Excel Workbook makes an `.xlsx`) | no | Clean-room from [MS-XLS] and [MS-CFB]: compound file with mini stream, shared strings across CONTINUE, values (RK, MULRK, numbers, text, booleans, errors), formulas decoded from their tokens (shared and array formulas, 3-D and external references, defined names) with cached results, fonts, fills, borders, number formats, column widths and row heights, merges, panes, hyperlinks, notes, hidden rows, columns and sheets, print area and titles. Charts, pictures, conditional formats and validation are not read. A web page named `.xls` opens as its table | `xls/tests.rs`, `workbook_files.rs`, `gate-file.mjs` |
| `.xlt` Excel 97-2003 template | R/W | yes (as `.xls`) | no | | `xls/tests.rs` |
| `.xlam` / `.xla` add-ins | R/W | no | no | VBA; out of scope | — |
| `.xml` XML Spreadsheet 2003 | R/W | no | no | Simple to support | — |
| `.ods` OpenDocument Spreadsheet | R/W | **values only** | no | Sheets, values, dates, merges, repeated cells, cached formula results; no formulas, formatting, charts; encrypted ODS refused | `ods_tests.rs` |
| Password-encrypted workbooks (Agile/Standard encryption) | R/W | **no**; recognised and refused with a clear message | no | Common in business; needs AES decryption on top of the CFB reader (`crates/formats/xlsx/src/cfb.rs`) | — |
| `.xlw` Excel 4.0 workbook, `.wk*`, `.dbf` | open only (some) | no | no | Rare | — |

## Text and data formats

The XLSX writer includes an optional `xl/kition/identities.json` part for tool identities and
cell provenance. It contains no replica history. Sheets, axes (including deleted-axis markers),
cells, drawings, defined names, tables and pivots retain their observed identities after a Kition
save/reopen. Other applications may discard this part. Cells remain addressed by A1 position.
Drawing endpoints normalize to integer EMUs, column widths to the file's character-width unit,
and row heights to stored precision on entry. Chart source settings and threaded-comment state
are retained in extensions. `bindings/wasm/tests/saved_receipts.rs` checks the object-model saved
invariant across every conformance workbook and representative write families; this does not
establish preservation of unmodelled XLSX content.

| Format | Excel | Kition Sheet read | Kition Sheet write | Notes |
|---|---|---|---|---|
| `.csv` UTF-8 / Windows / Mac / MS-DOS | R/W | yes (UTF-8 and UTF-16 BOMs, UTF-8, GBK/GB18030 for Chinese text that is not UTF-8, Windows-1252 fallback; delimiter detection) | yes (UTF-8, CRLF; the **displayed** values, as Excel and WPS write them; a UTF-8 byte-order mark on request, which the editor's Export as CSV asks for so Excel for Windows reads Chinese text rightly) | No encoding choice on save (UTF-8 only). Re-import of a single-column export splits text |
| `.txt` tab-delimited, Unicode text | R/W | yes (`.txt`, `.tsv`, `.tab`) | yes | Text Import Wizard options partly via Text to Columns |
| `.prn` formatted text (space-delimited) | R/W | no | no | |
| `.dif`, `.slk` (SYLK) | R/W | no | no | Small, legacy |
| `.htm` / `.html` web page | R/W (Windows) | yes, via Get Data ▸ From HTML | yes (one sheet as a table) | Formatting not exported |
| `.mht` / `.mhtml` single-file web page | R/W (Windows) | no | no | |
| `.json` | via Power Query | Kition Sheet's own workbook JSON (debug) and JSON import | Kition Sheet JSON | Not Power Query's JSON shaping; notes and hyperlinks break export |
| `.xml` with XML maps | Developer ▸ Import/Export | no | no | |

## Export and print

| Format | Excel | Kition Sheet | Notes |
|---|---|---|---|
| PDF | Save As / Export | yes (own PDF 1.7 writer) | Latin text in the 14 standard PDF fonts; other text (Chinese, Japanese, Korean…) in a subset of a font the host added (TrueType, Type0/Identity-H with ToUnicode, so it can be searched and copied). A4 unless the sheet says otherwise (WPS); File › Export as PDF writes every sheet. Scripts without a host font still drop out. Form controls are not drawn |
| XPS | Windows | no | Low priority |
| Print | yes | no print command in the editor (`file.print` is left to the host) | PDF export above |
| Images of ranges/charts (Copy as Picture) | yes | File › Export as Picture (`file.export {format:"png"}`): the used range of the active sheet as a PNG at 1:1 to 4:1 (capped at 400 megapixels) | No Copy as Picture to the clipboard |

## Import sources

Excel's Get Data reaches files (Excel, CSV, XML, JSON, PDF, Parquet, folders), databases (SQL
Server, Access, ODBC, …), web pages, SharePoint and online services, through Power Query.
Kition Sheet imports CSV/TSV, JSON and HTML tables only, with no query editor, no connections and no
refresh. Counted under features (data import and shaping, ~10% ready), not here.

## Work (80–120 h)

| Work | Hours |
|---|---|
| Real-world XLSX corpus run, and confirm Excel opens our output with no repair prompt (a maintainer runs Excel) | 15–25 |
| Preserve unmodelled XLSX parts and extensions on round trip (pass-through with relationship fix-up) | 10–15 |
| Streaming XLSX reader for very large files, part-size limits that degrade instead of failing | 10–15 |
| Legacy `.xls` (BIFF8): charts, pictures, conditional formats and validation on import; then write | 10–15 |
| Encrypted workbooks: open with password prompt, save with password | 8–12 |
| XLSB and ODS: formulas and formatting on import; then write | 10–15 |
| CSV/text: encoding choice on save, `.prn`, `.dif`, `.slk`, XML Spreadsheet 2003 | 5–8 |
| PDF: fonts for scripts beyond Chinese, Japanese and Korean; form controls drawn | (counted in [localization](localization-parity.md) script work) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
