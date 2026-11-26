//! C8-sheet-pin source-only supplement, committed before the new fact comparator.
//! The previous command.runLine record was reviewed after declarations; it is not
//! retroactively blind. These additional paths come from execute and step only.

pub const READINGS: &str = r##"
ROW command.runLine | params={"text":"42"} | resolves=active sheet and selection.active; explicit cell passed to cell.set; then select.move dr=1 dc=0 | holds=ENTRY(active,active-cell);FIT(active,active-row);HEADERS(active,active-cell);CONNECTORS(active);navigation source/destination merges and traversed hidden row/column flags and address windows | reads=part,selection,mode | writes=active cell entry, clears editing, moves selection down | note=bindings/wasm/src/lib.rs execute command.runLine branch and step; cmd/edit.rs cell_set and move_selection
ROW command.runLine | params={} | resolves=missing/non-string text defaults empty; active sheet and selection.active; then select.move dr=1 dc=0 | holds=ENTRY(active,active-cell);FIT(active,active-row);HEADERS(active,active-cell);CONNECTORS(active);navigation source/destination merges and traversed hidden row/column flags and address windows | reads=part,selection,mode | writes=clears active cell entry, clears editing, moves selection down | note=bindings/wasm/src/lib.rs execute command.runLine branch and step; cmd/edit.rs cell_set and move_selection
"##;
