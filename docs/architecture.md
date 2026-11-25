# Architecture

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Excel (Microsoft 365)

How Kition Sheet is built today. Rules for contributors are in [`AGENTS.md`](../AGENTS.md).

## Workspace and layering

An engine-first Cargo workspace: the engine crates, the in-page editor's engine and the headless
kit. There is no desktop app, CLI or MCP server in it.
Layering is enforced by `cargo xtask layers`. Line counts are Rust lines including tests.

| Layer | Crate | Lines | Job |
|---|---|---|---|
| L0 | `core` | ~1.5k | Cell and range addresses (A1, R1C1, sheet-qualified; 1,048,576 × 16,384), `Value`, `CellError`, Excel coercion, 1900/1904 date serials, typed-input parsing |
| L0 | `numfmt` | ~2.3k | Excel number-format codes (sections, conditions, colours, dates, elapsed time, fractions, scientific, fill), General, `TEXT()`; en-US only |
| L1 | `formula` | ~2.1k | Tokenizer, parser, AST, printer (A1 and R1C1), reference adjustment for copy, fill and structural edits |
| L1 | `functions` | ~13.6k | The function library: `FnSpec` registry (name, arity, category, scalar args, signature, description) and pure implementations; automatic array lifting |
| L2 | `model` | ~3.8k | Workbook, sheets, copy-on-write cell store (64-row bands behind `Arc`), interned styles, names, tables, CF, validation, comments, charts, shapes, images, sparklines, print settings, protection |
| L2 | `calc` | ~3.3k | Evaluator (references, names, tables, LET/LAMBDA, special forms like OFFSET/INDIRECT/SUBTOTAL) and dependency-graph recalculation with dynamic arrays, spills and cycle detection; single-threaded |
| L2 | `xlsx` | ~22.2k | XLSX read/write (transitional and strict read), CSV/TSV, XLSB and ODS data import, pivot table caches, DrawingML charts and drawings; byte slices only (WASM-safe); lenient reader that reports warnings |
| L2 | `pdf` | ~1.6k | Small PDF 1.7 writer (14 standard fonts, vectors, images) for print and export |
| L3 | `chart` | ~4.3k | Toolkit-free chart layout to drawing primitives, a rasterizer, sparklines |
| L4 | `kdm-model` | ~12.2k | Kition: the workbook as a `kit.sheet` KDM replica (kition-sdk session): schema, full and incremental writes (row/column inserts as list edits of stable line objects, formulas with id-based references), reads, merge repair |
| L5 | `engine` | ~34k | `Session`: open workbooks, selection, history (COW snapshots), clipboard, fill, sort, filter, pivot tables, what-if, print, file I/O (`io.rs`), and the command registry (`cmd/*`) with the Excel ribbon catalog (`catalog.rs`) |
| L6 | `kition-sheet-kit` | ~29.6k | Kition's in-page editor engine (Apache-2.0): the workbook as a `KitDocument` with its own cell painting, the grid's selection and typing as `KitTools` |
| app | `kition-sheet` | — | The headless kit executable for Kition's agent, on kition-sdk's `kition-headless` |
| tool | `xtask` | — | `ci`, `parity`, `assets`, `layers`, `wasm`, `version`, `stats` |

## Data flow

1. **Every action is a command.** The kit's editor (`web/`) and the headless kit executable
   (`bindings/headless`) both call `Session::execute(id, params)` through `KitTools`. Commands are
   `CommandSpec { id, label, menu, shortcut, params, enabled, run, journal, undoable }`; ids
   follow Excel's ribbon (`format.bold`, `chart.insert`, `data.sortAscending`) plus primitives
   (`cell.set`, `select.set`, `range.setValues`). `Session::execute` catches escaped panics and
   restores the workbook (never-crash guard).
2. **Edits** change the model through copy-on-write bands; undo keeps the previous snapshot.
3. **Recalc** marks dependents of changed cells dirty through the dependency graph (with range
   nodes for range dependents), evaluates in topological order, spills dynamic arrays and reports
   `#SPILL!`/cycles. Volatile functions are always dirty.
4. **Rendering**: the kit's editor reads display values (number formats applied in
   `engine/display.rs`) for the visible window only; charts render through `crates/engine/chart`
   primitives.
5. **Files**: `engine/io.rs` sniffs content (XLSX, XLSB, ODS) before trusting the extension and
   dispatches to `crates/formats/xlsx`; imports that lose data return warnings shown to the user.

## Agent control

Kition's agent drives the engine through `bindings/headless`, a headless executable built on
kition-sdk's `kition-headless`, over the same `KitTools` contract as the in-page editor
(`bindings/wasm`). There is no separate control channel, MCP server or CLI in this
repository.

The Rust bindings pin the public SDK revision; the web editor consumes its
published packages. Local web checkout links are opt-in with `KITION_LINK=1`.
Typed references, editor-read declarations and per-call premises reach
the shared guard through `bindings/wasm/src/place.rs`. Address windows hold
positional cell and line lookups. Derived facts represent bounded source
dependencies and shared edit consequences; complex paths retain conservative
holds that can reject unrelated edits.

Painter pickup holds the expanded source range. Anchor-mode assignments use a
private scalar fact because the stored frame is positional. Arithmetic formula
sources hold raw values; `CELL` metadata queries additionally hold source formats.
Fitting guards height contributions, with a conservative whole-row guard retained
for multi-cell Bold. Formatting holds its effective style definitions explicitly,
and sort retains complete row metadata. Checkbox delegation preserves named versus
numeric sheet addressing.
Alignment holds source frames and the axis prefixes needed for result-anchor conversion.
Axis content comparisons use the complete historical value, so newly materialized
geometry is compared with its implicit defaults.
Table duplicate removal includes external comparison columns in its source guards.
Image alignment holds the stored image anchor rather than a shape frame.

Logical-axis content regions use a private workbook-lifetime fact holder.
Sheet insertion, deletion and undo do not create or remove that holder. Its
derived contents stay out of public receipts, and derived enumeration preserves
the original sheet and axis order when facts for an object are merged.
The same holder retains conditional-clear coverage independently of stored sheet
edits, so historical recipients remain visible when public receipts omit private facts.

The canonical model exposes typed owning cell maps and owning drawing indexes.
Public loads translate their references to tool IDs. Read-only indexes expose
tables, pivots, groups and style definitions while embedded feature registers
remain authoritative. Environment sampling follows owning maps, excludes image
containers and uses recorded bindings when replay allocates fresh identities.

Cell entry holds source expressions, names, table definitions, destination
values, covering constraints and fitted-row inputs. Formatting holds assigned
styles and fitting dependencies. Tables, filters, outline operations and sparse
metadata clears declare their bounded source and destination footprints.
Absolute assignments also hold the observed field being replaced, following the
same-field decision. Conditional shape fill requires its branch's explicit
parameter rather than a static target shared by both branches.

Outside rereads pair the canonical imported file model with the retained
replica, then apply changed fields as an import transaction. Persisted row and
object identities, editor state and undo histories are retained. New objects
are created in reference dependency order. Saved pane origins and zoom are
restored from the file separately from replica content. Deleted active editor
targets are refused. Dependent premise planning reopens a replica snapshot with
blobs, editor state, clipboard content and the captured painter source in an
isolated session.

Receipts describe the settled saved model; unavailable before/after values may
be omitted. Closing the last workbook can return a closed result without a
document view. Gate regression cases exercise these contracts through both native
bindings and the headless executable. Every registered write and retained
handler-reading path has a bidirectional comparison of declared content,
identity, position and address facts. The original blind families and labelled
binding entry supplement retain their provenance. Missing facts fail the gate;
excess facts have decreasing global, per-path and per-class ceilings. These
comparisons are separate from invocation, metadata and sampled footprint checks.
Conservative fallbacks still prevent a complete precision claim.

- No threading in calc (see [hardware-parity.md](parity/hardware-parity.md)).
- No string catalog or text shaping (see [localization-parity.md](parity/localization-parity.md)).
- XLSX reader builds the whole workbook in memory; no streaming.
- Unmodelled XLSX parts are not carried through a save.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
