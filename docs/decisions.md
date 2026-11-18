# Decisions

## Formula localization stays at the entry boundary

Localized formula syntax lives in the formula crate (`FormulaLocale` in
`crates/engine/formula/src/input.rs`): `parse_input` reads a formula typed in a language into the
canonical English expression, and `print_input` prints one back for editing. Spanish is the only
table: its function names, decimal comma, `;` between arguments and `\` between array
columns. A language without a table uses the English function names and separators. Existing
names and LET/LAMBDA bindings take precedence over aliases, and a displayed formula keeps the
canonical spelling where a localized one would collide with a name. Stored formulas, XLSX and
engine commands keep English syntax (`kition_sheet_formula::parse`). The editor in `web/` does
not select a formula language yet.

## Preserve array evaluation when exporting formulas

Kition Sheet evaluates ordinary formulas with array semantics even when the final result occupies one cell. XLSX export marks actual spills and formulas whose results may differ under legacy implicit intersection as dynamic, using the spill range or a single-cell array reference. Legacy CSE array ranges keep their existing representation.

The exporter (`crates/formats/xlsx/src/array_formula.rs`) inspects the parsed formula without evaluating it. It distinguishes range-taking functions such as `SUM` from scalar arguments and operators: `SUM(A1:A3)` and `A1*2` stay ordinary, while `SUM(A1:A3*B1:B3)` and `SQRT(A1:A3)` retain array evaluation. Legacy array-aware contexts such as `SUMPRODUCT` do not need an extra marker. Explicit `@` scalarizes its result without hiding array-sensitive operations inside it.

Names are resolved using workbook/sheet scope, and structured references distinguish one table-row cell from a column. Literal `INDIRECT` and `INDEX` arguments permit tighter classification; unresolved reference dimensions are conservatively treated as potentially multiple cells. Name/AST traversal is bounded, with array preservation as the fallback. This is an export classifier, not a claim of exact Excel metadata parity for every function or user-defined function. It stays within the format crate's existing dependencies and does not change calculation or import behavior.

This is a writer policy; interpreting implicit intersection in imported legacy formulas remains separate work. See Microsoft's [Formula vs. Formula2 documentation](https://learn.microsoft.com/en-us/office/vba/excel/concepts/cells-and-ranges/range-formula-vs-formula2).

## Static cell pictures

Store immutable picture payloads in `Sheet::cell_pictures`, a sparse map keyed by `CellRef`,
shared through `Arc`. Ordinary cells keep their original size. Content replacement removes the
map entry; copy, fill, sort and structural edits explicitly transport it, and undo shares bytes.
Serialize the map as address/payload pairs so JSON does not require struct-valued object keys.
Keep the scalar `#VALUE!` fallback used by XLSX consumers that cannot interpret rich images.
Cell pictures do not extend formula evaluation to rich values or implement
`IMAGE()`; references and text-only exports use the fallback.

Write picture value metadata and existing dynamic-array cell metadata into the same workbook
metadata part, with independent index tables. Resolve input parts through their relationships.
Support embedded PNG/JPEG files without fetching external resources. See
[cell pictures](features/cell-pictures.md) for the supported user workflow and current limits.

Picture metadata is optional during XLSX import: part-size limits, record limits and relationship
read errors produce warnings and retain scalar cell fallbacks
(`crates/formats/xlsx/src/richdata.rs`, which reads its parts through `Package::read_limited`).
