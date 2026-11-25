//! C8-sheet-read: blind engine readings, committed before opening binding declarations.
//! Inline same-field decision notes identify later replacement-hold amendments.
//!
//! Shared fact notation (required facts, not whole-document premises):
//! - ADDRESS(S,R): sheet lookup (name or numeric tab position), row/column address windows R.
//! - ENTRY(S,R): existing destination input/rich text/array bounds/pictures/style, inherited
//!   row/column/default styles and their definitions, sheet protection, covering validations
//!   and table definitions/header columns; string parsing reads date_system and named-function
//!   case definitions; formulas read resolved names/tables, precedents and their address windows.
//! - FIT(S,rows): automatic row-height calculation's custom/size flags, default row height,
//!   default font size, same-row cell text/formulas/results/styles, merges, wrapped-cell column
//!   width/hidden/default width and display number formats/date_system. Unwrapped default-font
//!   rows take a fast path. This is cmd/mod.rs:369-436, not every cell in the workbook.
//! - STYLE(S,R): current cell/inherited row/column style definitions and rich runs (changed font
//!   fields clear run overrides), protection.format_cells, filtered-out row flags/AutoFilter
//!   criteria/range, ADDRESS(S,R), FIT(S,rows(R)). Full lines read/update their line styles and
//!   only materialized cells; whole lines do NOT skip filtered rows.
//! - HEADERS(S,R): tables overlapping changed header cells, all their column names, displayed
//!   header text/styles, header formula precedents; structured-reference formulas mentioning
//!   renamed columns across all sheets. sync_headers runs for every edit; when structural=true
//!   it checks every table header, not only the addressed sheet.
//! - CONNECTORS(S): on a modified sheet with connectors, connector/end-point frames/connections,
//!   shape geometry and sheet row/column sizes used by reroute_connectors.
//! - STRUCTURE(S,band): existing cells/features/axis properties shifted in band; merges, CF/DV,
//!   tables/filter, pictures/comments/links, sparklines/print area/freeze; floating-object anchor
//!   modes/corners and row/column geometry; changed cell/feature formulas and names throughout
//!   workbook, tab/name resolution, protection permissions/locked styles, HEADERS(all), CONNECTORS.
//! - PASTE(S,R): destination old cells/styles/pictures, protection, ADDRESS, FIT, HEADERS,
//!   source workbook snapshot/session clipboard. Source snapshot facts are session facts; a
//!   copied snapshot is not a live read of the current source (a cut source is live).
//!
//! All writes through edit/commit also run protection checks over changed content and recalc
//! affected formulas (structural edits recalc all). Recalc consequences alone are not an input
//! premise on every dependent formula. Every changed sheet with connectors reroutes them.
//! `part` means active sheet, `selection` includes active cell/anchor/ranges; extra session facts
//! are named explicitly. Each row below is from engine code, none pinned from declarations.

pub const READINGS: &str = r###"
# cmd/edit.rs -- entries
ROW cell.set | params={"sheet":"Data","cell":"B2","input":"7"} | resolves=sheet name Data; cell A1 B2 | holds=ADDRESS(Data,B2);ENTRY(Data,B2);FIT(Data,2);HEADERS(Data,B2);CONNECTORS(Data) | reads=none | writes=B2 content/rich/picture; table header and automatic height consequences | note=edit.rs:295 literal entry; expand true may grow adjacent tables
ROW cell.set | params={"sheet":1,"cell":"B2","input":"=A1+1","expand":false} | resolves=sheet numeric tab index 1, NOT sheet id; B2 A1 | holds=ADDRESS(index1,B2);ENTRY(index1,B2);formula precedent A1 with axis windows;FIT(index1,2);HEADERS(index1,B2);CONNECTORS(index1) | reads=none | writes=B2 formula; recalc and height | note=edit.rs:295 formula branch
ROW cell.set | params={"input":"text"} | resolves=active sheet; active selection cell, NOT first selected range start | holds=ADDRESS(active,active-cell);ENTRY(active,active-cell);FIT(active,active-row);HEADERS(active,active-cell);CONNECTORS(active) | reads=part,selection | writes=active cell, preserved rich runs via retext | note=edit.rs:295 default cell branch
ROW cell.set | params={"sheet":"Data","name":"Destination","input":"7"} | resolves=scope lookup in Data, defined name may point to a different sheet; static one-cell only | holds=name definition/scope;resolved sheet-name lookup;ADDRESS(resolved,name-cell);ENTRY(resolved,name-cell);FIT(resolved,name-row);HEADERS(resolved,name-cell);CONNECTORS(resolved) | reads=none | writes=resolved name cell, not Data necessarily | note=edit.rs:298 named_cell branch
ROW cell.set | params={"sheet":"Data","cell":"B2","input":"=A1:A2","array":true} | resolves=sheet Data; protection checks B2 but array writes selection.current range start (not B2) | holds=selection array destination input/styles/bounds;ADDRESS(Data,selection.current);ENTRY(Data,selection.start);formula A1:A2;FIT(Data,selection.start.row);HEADERS(Data,selection.start);CONNECTORS(Data);B2 locked/protection check | reads=selection | writes=selection.current first formula and clears other contents, at most 1000000; rich/picture | note=edit.rs:319 array branch checks wrong cell when B2 differs from selection start
ROW cell.set | params={"cell":"B2","input":"text","runs":[{"text":"text","font":{}}]} | resolves=active sheet; B2 | holds=ADDRESS(active,B2);ENTRY(active,B2);FIT(active,2);HEADERS(active,B2);CONNECTORS(active) | reads=part | writes=matching runs replace rich text; unmatched runs discarded | note=edit.rs:354 explicit runs bypass keep_runs
ROW cell.formatText | params={"sheet":"Data","cell":"B2","bold":true} | resolves=Data name; B2 or active selection cell if invalid/missing cell | holds=ADDRESS(Data,B2);B2 text/formula/rich;cell style font and definition;sheet protection and locked style;FIT(Data,2);HEADERS(Data,B2);CONNECTORS(Data) | reads=none | writes=B2 rich runs, touched height | note=edit.rs:249 explicit font patch
ROW cell.formatText | params={"toggle":"bold"} | resolves=active sheet/selection active cell; whole text default | holds=ADDRESS(active,active-cell);cell text/rich/base font;selected run resolved font toggles;protection;FIT(active,active-row);HEADERS(active,active-cell);CONNECTORS(active) | reads=part,selection | writes=selected text runs | note=edit.rs:249 and text_format.rs:54 toggle scans selected resolved run properties
ROW range.setValues | params={"sheet":"Data","range":"B2","values":[[1,true],["=A1",null]]} | resolves=Data; range FIRST area's start only; actual ragged matrix dimensions define writes | holds=ADDRESS(Data,B2:C3);ENTRY(Data,B2:C3);A1 precedent;FIT(Data,2:3);HEADERS(Data,B2:C3);CONNECTORS(Data) | reads=none | writes=matrix cells; number/bool branches preserve inherited style without parser | note=edit.rs:391 each scalar non-array row counts one column
ROW range.setValues | params={"values":[["text"]]} | resolves=active sheet; current selection first area's start | holds=ADDRESS(active,selection.start);ENTRY(active,selection.start);FIT(active,row);HEADERS(active,selection.start);CONNECTORS(active) | reads=part,selection | writes=matrix from selection start, NOT active cell | note=edit.rs:391 default range
ROW range.fill | params={"sheet":"Data","range":"B2:C3","input":"=A1"} | resolves=Data; explicit range first cell B2 is formula/style origin | holds=ADDRESS(Data,B2:C3);ENTRY(Data,B2 origin);existing each destination cell style;translated formula sources A1:B2;FIT(Data,2:3);HEADERS(Data,B2:C3);CONNECTORS(Data) | reads=none | writes=filled cells; missing cells inherit ORIGIN style not their own row/column style | note=edit.rs:450 base parsed once, per-existing-cell style retained
ROW range.fill | params={"input":"=A1"} | resolves=active sheet; all selection areas; origin is active cell even when not first area start | holds=ADDRESS(active,selection areas);ENTRY(active,active origin);each destination existing cell style;translated precedents from active origin;FIT(active,selected rows);HEADERS(active,selection);CONNECTORS(active) | reads=part,selection | writes=every selected cell | note=edit.rs:450 implicit Ctrl+Enter path
# cmd/edit.rs -- selection/session paths (retained even if binding excludes them as writes)
ROW select.set | params={"sheet":"Data","range":"A1:B2","active":"B2"} | resolves=Data name; all comma-separated areas; active B2 | holds=sheet membership/name and merges intersecting areas (transitive expansion) | reads=none | writes=active sheet and selection; remembered per-sheet selection | note=edit.rs:490 explicit range/active
ROW select.set | params={} | resolves=active sheet; prior selection ranges | holds=active sheet merges | reads=part,selection | writes=selection active/anchor reset to last range start | note=edit.rs:490 missing range
ROW select.move | params={"dr":1,"dc":0} | resolves=active sheet; active cell or far selection corner when extend | holds=travelled row/column hidden flags;source/destination merges;address windows | reads=part,selection | writes=selection, expanding merges | note=edit.rs:544 nonjump path
ROW select.move | params={"dr":1,"dc":0,"jump":true,"extend":true} | resolves=active sheet; far selection corner | holds=value occupancy along jump ray and used range;merges at landing/expanded selection;axis windows | reads=part,selection | writes=selection | note=edit.rs:544 jump branch; selection.rs:122
ROW select.next | params={} | resolves=selection ranges/current active | holds=none | reads=selection | writes=active cell cycles selected cells | note=edit.rs:611 cycle; no workbook read
ROW edit.selectAll | params={} | resolves=active sheet/active cell/current selection | holds=current-region occupancy including spills and empty boundary cells | reads=part,selection | writes=current-region or whole-sheet selection | note=edit.rs:619 selection.rs:182
ROW select.currentRegion | params={} | resolves=active sheet/active cell | holds=current-region occupancy/spills and boundary | reads=part,selection | writes=selection | note=edit.rs:632
ROW select.row | params={} | resolves=current selection first area's row span | holds=none | reads=selection | writes=whole-row selection | note=edit.rs:642
ROW select.column | params={} | resolves=current selection first area's column span | holds=none | reads=selection | writes=whole-column selection | note=edit.rs:650
ROW edit.goTo | params={"reference":"Destination"} | resolves=defined name in active-sheet scope, else table name, else A1; table addresses data_range without totals/header | holds=selected name formula/scope or table definition;sheet-name lookup;destination merges | reads=part | writes=active sheet and selection | note=edit.rs:658 name/table branch
ROW edit.goTo | params={"reference":"Data!B2"} | resolves=A1 sheet prefix Data | holds=sheet-name lookup and destination merges | reads=none | writes=active sheet and selection | note=edit.rs:658 A1 branch
ROW edit.goToSpecial | params={"kind":"visible"} | resolves=active sheet; single selection expands to used range else selection bounds | holds=used-range if expanded;row/column hidden flags in scope;axis windows | reads=part,selection | writes=matching cell ranges | note=edit.rs:688 kind-specific predicate
ROW edit.goToSpecial | params={"kind":"formulas"} | resolves=active sheet; used-range or selection bounds | holds=scope cell input/formula/type/spill and membership for matching constants/formulas/errors/numbers/text/blanks;comments/CF/DV only for their respective kinds | reads=part,selection | writes=matching cells coalesced/truncated 2000 ranges | note=edit.rs:688 lastCell uses used end; currentRegion uses occupancy frontier
ROW edit.undo | params={} | resolves=active document/history scope from origin or explicit scope | holds=current workbook and replica/history entries for selected scope;undo stack | reads=part,selection,history,origin | writes=workbook/selection/history/revision/calculation | note=edit.rs:760 undo by snapshot or replica replay preserving other scope
ROW edit.redo | params={} | resolves=active document/history scope from origin | holds=current workbook and redo replica/history entries | reads=part,selection,history,origin | writes=workbook/selection/history/revision/calculation | note=edit.rs:760 redo branch
ROW edit.copy | params={"range":"B2:C3"} | resolves=target_sheet from sheet or range prefix else active; single source range | holds=source cell inputs/computed display/number formats/date system and source features carried by clipboard snapshot | reads=part | writes=session clipboard snapshot/text | note=edit.rs:829 finite source; html false
ROW edit.copy | params={"range":"B:B","html":true} | resolves=source full line clipped to used range union its start | holds=source used-range;cell display/styles/theme/merges/comments/links/row+column dimensions/defaults;HTML payload | reads=part | writes=session clipboard snapshot/text/html | note=edit.rs:829 whole-line and HTML paths
ROW edit.cut | params={"range":"B2:C3"} | resolves=same copy_impl source resolution | holds=source displayed cells/styles and snapshot feature facts | reads=part | writes=session cut clipboard, NO cell deletion yet | note=edit.rs:829 cut true
ROW edit.clearClipboard | params={} | resolves=session clipboard and painter | holds=none | reads=none | writes=clipboard and painter cleared | note=edit.rs:932 no workbook read
ROW edit.find | params={"what":"x","lookIn":"values"} | resolves=active sheet or all tabs when within workbook | holds=all searched cell membership/text/computed values/formats/protection-hidden-formula status;next-hit tab order | reads=part,selection | writes=last_find and next-hit active sheet/selection;all true returns list only | note=edit.rs:1616
ROW edit.beginEdit | params={} | resolves=none in handler; queues edit UI request | holds=none | reads=none | writes=UI request only | note=edit.rs:2140 handler does not read cell, later UI does
# cmd/edit.rs -- paste paths
ROW edit.paste | params={"sheet":"Data","at":"B2","text":"1\t2"} | resolves=Data; at strips any sheet prefix, no prefix-based sheet selection from at; text differs from clipboard or no clipboard | holds=ADDRESS(Data,B2:C2);ENTRY(Data,B2:C2);FIT(Data,2);HEADERS(Data,B2:C2);adjacent table extents/merges/new headers for auto_expand;CONNECTORS(Data) | reads=clipboard | writes=TSV matrix and selection; grow table; parse failures become text | note=edit.rs:961 external text branch; paste_text 996
ROW edit.paste | params={"at":"B2","html":"<table><tr><td>x</td></tr></table>"} | resolves=active sheet; at B2; HTML first parsed table | holds=ADDRESS(active,pasted span);destination merges/pictures/protection;default style definition;FIT(active,pasted rows);HEADERS(active,pasted area);adjacent table growth facts;CONNECTORS(active) | reads=part,clipboard | writes=HTML cells/styles/merges/declared column widths and selection | note=edit.rs:1031 table path; missing at uses selection.current.start
ROW edit.paste | params={} | resolves=active sheet; selection start; source from session clipboard snapshot; matching text chooses internal source | holds=PASTE(active,tiled destination);cut source current cells/pictures and all moved references if same-doc cut;adjacent table growth facts | reads=part,selection,clipboard | writes=internal paste via what all, selection; cut clears clipboard | note=edit.rs:961 fallback and data-kition-sheet HTML payload paths
ROW edit.pasteSpecial | params={"sheet":"Data","at":"B2","what":"all"} | resolves=Data; at B2; source clipboard snapshot; same-doc cut instead reads live source cells/pictures | holds=PASTE(Data,destination);source merges and destination intersecting merges;source comments;cut current source and global formula/name rewrites | reads=selection,clipboard | writes=content/formula/style/picture/comments, merges, destination selection | note=edit.rs:1100 normal all; explicit at disables tiling
ROW edit.pasteSpecial | params={"what":"values","operation":"multiply","transpose":true,"skipBlanks":true} | resolves=active sheet/selection.start; tiled when destination multiple of transposed source dimensions | holds=PASTE(active,tiled span);source cached values/spills/pictures;destination old value/formula/picture and translated formula precedents;blank source occupancy | reads=part,selection,clipboard | writes=combined numeric/formula values and pictures preserving destination style; selection | note=edit.rs:1100 arithmetic/transposed/tiled/skip blanks branch
ROW edit.pasteSpecial | params={"what":"formats"} | resolves=active sheet/selection.start and tile dimensions | holds=PASTE(active,span);source style/merges;destination style checkbox and merges | reads=part,selection,clipboard | writes=styles preserving checkbox, merges; NOTE set_cell removes destination pictures | note=edit.rs:1234 formats branch
ROW edit.pasteSpecial | params={"what":"comments"} | resolves=active sheet/selection.start and source snapshot | holds=ADDRESS(active,span);destination comments/pictures;source comments;FIT(active,dest rows);HEADERS(active,dest span);CONNECTORS(active) | reads=part,selection,clipboard | writes=source-present comments copied;existing comments retained when source missing;touch cells | note=edit.rs:1303 comments branch still touch and new_picture insert
ROW edit.pasteSpecial | params={"what":"validation"} | resolves=active sheet/selection.start | holds=PASTE(active,span) | reads=part,selection,clipboard | writes=old cells set back via set_cell, may remove pictures; no validation rule copied | note=edit.rs:1252 Validation arm is empty; no subsequent validation copy
ROW edit.pasteSpecial | params={"what":"columnWidths"} | resolves=active sheet/selection.start tiled source columns | holds=ADDRESS(active,span);source column LineInfo;destination pictures;FIT(active,dest rows);HEADERS(active,dest span);CONNECTORS(active); replaced values: entire destination column LineInfo (size,hidden,outline,style,custom) | reads=part,selection,clipboard | writes=entire source column info including style/hidden/outline, not only width | note=edit.rs:1309 ColumnWidths branch; same-field decision
ROW edit.pasteSpecial | params={"what":"formulasAndNumberFormats","link":true} | resolves=active sheet/selection.start; source sheet NAME used for cross-sheet link | holds=PASTE(active,span);source sheet name for link text;new reference source windows/values;destination old style | reads=part,selection,clipboard | writes=formulas linking source, preserving old style even number-format mode because link bypasses what switch | note=edit.rs:1193 link branch; AllExceptBorders additionally reads old border; number-format modes read source format
# cmd/edit.rs -- clear/fill/replace/painter/move
ROW edit.clearAll | params={"sheet":"Data","range":"B2:C3"} | resolves=Data; all explicit areas | holds=ADDRESS(Data,B2:C3);selected cells/pictures;comments/hyperlinks entries;wholly-contained merges/tables;protection;FIT(Data,intersect-used rows);HEADERS(all if table removed);CONNECTORS(Data) | reads=none | writes=cells/pictures/notes/links removed;contained merges/tables removed | note=edit.rs:1420 sparse metadata handled before used-range clipping
ROW edit.clearFormats | params={"range":"B2:C3"} | resolves=active sheet; all explicit areas | holds=ADDRESS(active,B2:C3);selected cells/styles/checkbox definitions;contained merges/table styles;FIT(active,intersect-used rows);HEADERS(active,selected cells);CONNECTORS(active) | reads=part | writes=cell styles reset preserving checkbox;contained merges removed/table style cleared;rich text retained | note=edit.rs:1420
ROW edit.clearContents | params={"range":"B2:C3"} | resolves=active sheet; explicit areas or selection | holds=ADDRESS(active,B2:C3);selected input/styles/checkbox definitions;protection;FIT(active,intersect-used rows);HEADERS(active,selected cells);CONNECTORS(active) | reads=part | writes=values/formulas/pictures/checkbox removed, rich text retained | note=edit.rs:1420
ROW edit.clearComments | params={"range":"B2:C3"} | resolves=active sheet; explicit areas or selection | holds=ADDRESS(active,B2:C3);selected sparse comments entries;CONNECTORS(active) | reads=part | writes=selected comments only, even outside stored cells | note=edit.rs:1444 no touch/FIT on comments branch
ROW edit.clearHyperlinks | params={"range":"B2:C3"} | resolves=active sheet; explicit areas or selection | holds=ADDRESS(active,B2:C3);selected sparse hyperlinks entries;CONNECTORS(active) | reads=part | writes=selected links only, even outside stored cells | note=edit.rs:1447 no touch/FIT
ROW edit.fillDown | params={"range":"B2:C3"} | resolves=active sheet; source B2:C2, destination B3:C3 | holds=ADDRESS(active,B2:C3);source full cells/pictures;destination cells/protection;translated formula precedents;FIT(active,3);HEADERS(active,B3:C3);CONNECTORS(active) | reads=part | writes=copy fills down source first row | note=edit.rs:1503 fill_dir -> fill.rs:35
ROW edit.fillDown | params={"range":"B2:C2"} | resolves=active sheet; one-row selection extends source to row ABOVE B1:C1 | holds=ADDRESS(active,B1:C2);source B1:C1 cells/pictures;destination/protection;translated formula precedents;FIT(active,2);HEADERS(active,B2:C2);CONNECTORS(active) | reads=part | writes=selected row from neighbour, not itself | note=edit.rs:1518 single-row fallback
ROW edit.fillRight | params={"range":"B2:B3"} | resolves=active sheet; one-column range extends source to LEFT A2:A3 | holds=ADDRESS(active,A2:B3);source A2:A3 cells/pictures;destination/protection;translated precedents;FIT(active,2:3);HEADERS(active,B2:B3);CONNECTORS(active) | reads=part | writes=selected column from neighbour | note=edit.rs:1521
ROW edit.fillUp | params={"range":"B2:C3"} | resolves=active sheet; source bottom row B3:C3, writes B2:C2 | holds=ADDRESS(active,B2:C3);source full cells/pictures;dest/protection;translated formula precedents;FIT(active,2);HEADERS(active,B2:C2);CONNECTORS(active) | reads=part | writes=fill upward | note=edit.rs:1503 one-row fillUp no-op; whole lines clipped used range
ROW edit.fillLeft | params={"range":"B2:C3"} | resolves=active sheet; source right column C2:C3, writes B2:B3 | holds=ADDRESS(active,B2:C3);source cells/pictures;dest/protection;translated formula precedents;FIT(active,2:3);HEADERS(active,B2:B3);CONNECTORS(active) | reads=part | writes=fill left | note=edit.rs:1503 one-column fillLeft no-op; whole lines clipped used range
ROW edit.autoFill | params={"source":"B2:B3","target":"B2:B8"} | resolves=active sheet; source/target parse bare A1 (sheet-qualified source invalid falls back target_range); lane orientation from dimensions | holds=ADDRESS(active,B2:B8);source cells/pictures/first number format;custom_lists/date_system for series detection;destination/protection;translated precedents;FIT(active,4:8);HEADERS(active,B4:B8);CONNECTORS(active) | reads=part | writes=series/copies outside source and selection union | note=edit.rs:1545 fill.rs:35 Pattern detection; dates months value path incorrectly fixed D1900 in fill.rs:268
ROW edit.autoFill | params={"target":"B2:B8","mode":"formats"} | resolves=active sheet; source default target_range selection; target B2:B8 | holds=ADDRESS(active,source and target);source cell styles;destination full cells;protection;FIT(active,filled rows);HEADERS(active,filled cells);CONNECTORS(active) | reads=part,selection | writes=styles via cells.set preserves destination pictures;values mode preserves dest CELL style (not inherited) | note=edit.rs:1545 formats/values/copy variants
ROW edit.fillSeries | params={"range":"B2:B8","type":"date","dateUnit":"month"} | resolves=active sheet; explicit range; direction default width greater than height | holds=ADDRESS(active,B2:B8);first cell each lane numeric computed value and inherited style;date_system;destination/protection;FIT(active,3:8);HEADERS(active,B3:B8);CONNECTORS(active) | reads=part | writes=static values beyond seed until stop;seed style copied | note=edit.rs:1572 fill.rs:312; type autofill falls through linear
ROW edit.smartFill | params={"range":"C4:C10"} | resolves=active sheet; ONLY start C4 used; neighbours B column define contiguous vertical region possibly outside requested rows | holds=ADDRESS(active,region B:C);source B contiguous values and empty boundary;all C examples/emptiness in region;C top style;protection;FIT(active,written rows);HEADERS(active,written cells);CONNECTORS(active) | reads=part | writes=blank C cells within learned region, not just C4:C10 | note=edit.rs:1587 fill.rs:370
ROW edit.replace | params={"what":"old","with":"new"} | resolves=active sheet, ignores sheet/range params; all stored cells unless all false then only active coordinate | holds=searched cell membership/input including pictures;matched cell parsing/styles/date-system/rich;formula sources;protection;FIT(active,matched rows);HEADERS(active,matches);CONNECTORS(active) | reads=part,selection | writes=matching cells, counts; selection read unconditionally but only affects all false | note=edit.rs:1688
ROW edit.replace | params={"what":"old","with":"new","all":false,"within":"workbook"} | resolves=active coordinate applied on EVERY sheet, not only active sheet; scans workbook cells | holds=tab membership;each sheet cell at active coordinate input/style/rich/pictures;parser sources;protection;FIT(each,matches);HEADERS(each,matches);CONNECTORS(each) | reads=selection | writes=one matching coordinate per sheet | note=edit.rs:1688 all=false/workbook cross-product
ROW edit.formatPainter | params={"sticky":true} | resolves=active sheet/selection.current, expands transitively through touching merges | holds=source styles/inherited styles/table look definitions/theme/hidden row+col band positions/merges stored in snapshot | reads=part,selection,format_painter | writes=session painter snapshot/source/sticky | note=edit.rs:1762 pickup; no apply
ROW edit.formatPainter | params={"cancel":true} | resolves=none | holds=none | reads=none | writes=clears painter session state | note=edit.rs:1763 cancel; second click with no sticky also clears
ROW edit.formatPainter | params={"apply":"B2:C3"} | resolves=ACTIVE sheet regardless sheet param; source frozen painter snapshot; single target cell or exactly merged target expands to source dimensions | holds=ADDRESS(active,destination);dest styles/checkbox/rich/pictures/protection/merges/filter hidden rows;source snapshot cell styles/table style definition/theme/visibility;FIT(active,dest rows);HEADERS(all);CONNECTORS(active) | reads=part,format_painter | writes=dest styles/merges;full-line line styles;selection;may clear painter | note=edit.rs:1809 paint_formats; structural=true
ROW edit.moveCells | params={"from":"B2:C3","to":"E5"} | resolves=active sheet, explicit sheet must equal active; from/to prefixes stripped and ignored | holds=ADDRESS(active,source and destination);live source/dest cells/pictures/features;all formulas/names adjusted by move;protection;FIT(active,source/dest rows);HEADERS(all);CONNECTORS(active) | reads=part | writes=block including metadata then paste, global references, selection/clipboard | note=edit.rs:1953 move_block has multiple edit commits
ROW edit.moveCells | params={"to":"E5","copy":true} | resolves=active sheet; selection.current as source, multiple areas refused | holds=ADDRESS(active,source/dest);source snapshot cells/styles/pictures/comments/merges;destination old data/protection;translated precedents;FIT(active,dest rows);HEADERS(active,dest);CONNECTORS(active) | reads=part,selection | writes=copied content/styles/comments/merges and selection | note=edit.rs:1953 copy path
ROW edit.moveCells | params={"from":"2:3","to":"6:6","insert":"down"} | resolves=active sheet; whole rows source and row destination; whole lines coerce axis | holds=STRUCTURE(active,insertion/deletion bands);source line size/style/visibility;used-range cap | reads=part | writes=insert lines, move block, delete old lines;global refs/features;selection | note=edit.rs:2051 insert_block delegates nested commands; errors can follow earlier commits
# cmd/format.rs
ROW format.bold | params={} | resolves=active sheet and active cell decide toggle; target selection areas | holds=STYLE(active,selection);active-cell resolved bold | reads=part,selection | writes=font.bold inverse active style | note=format.rs:304 toggle
ROW format.bold | params={"sheet":"Data","range":"B2","on":true} | resolves=Data B2; explicit on bypasses active-style read | holds=STYLE(Data,B2) | reads=none | writes=font.bold true | note=format.rs:304 explicit toggle
ROW format.italic | params={} | resolves=active sheet/cell toggle and selection destinations | holds=STYLE(active,selection);active-cell italic | reads=part,selection | writes=font.italic | note=format.rs:304
ROW format.italic | params={"sheet":"Data","range":"B2","on":false} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=font.italic false | note=format.rs:304 explicit toggle
ROW format.underline | params={"sheet":"Data","range":"B2","on":true} | resolves=Data B2 target; ALWAYS reads active cell underline before unwrap_or even explicit on | holds=STYLE(Data,B2);active-sheet active-cell style underline | reads=part,selection | writes=chosen underline | note=format.rs:313 eager active_style read
ROW format.underline | params={} | resolves=active sheet/cell underline toggles default single; target selection | holds=STYLE(active,selection);active-cell underline | reads=part,selection | writes=underline single/none | note=format.rs:313
ROW format.doubleUnderline | params={} | resolves=active sheet/cell underline compared double; target selection | holds=STYLE(active,selection);active-cell underline | reads=part,selection | writes=double underline or none | note=format.rs:313 style injected double
ROW format.strikethrough | params={} | resolves=active sheet/cell toggle and selection destinations | holds=STYLE(active,selection);active-cell strike | reads=part,selection | writes=font.strike | note=format.rs:304
ROW format.strikethrough | params={"sheet":"Data","range":"B2","on":true} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=font.strike | note=format.rs:304 explicit toggle
ROW format.superscript | params={} | resolves=active sheet/cell toggle and selection | holds=STYLE(active,selection);active-cell vert | reads=part,selection | writes=font.vert superscript/baseline | note=format.rs:304
ROW format.subscript | params={} | resolves=active sheet/cell toggle and selection | holds=STYLE(active,selection);active-cell vert | reads=part,selection | writes=font.vert subscript/baseline | note=format.rs:304
ROW format.fontName | params={"sheet":"Data","range":"B2","name":"Arial"} | resolves=Data B2; name is font text, NOT object name | holds=STYLE(Data,B2) | reads=none | writes=font name/script names plus rich override removal | note=format.rs:329
ROW format.fontSize | params={"sheet":"Data","range":"B2","size":14} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=font size rounded half point | note=format.rs:334
ROW format.increaseFontSize | params={"sheet":"Data","range":"B2"} | resolves=target Data B2 but next size and enabled check from ACTIVE sheet's active cell | holds=STYLE(Data,B2);active-cell resolved font size | reads=part,selection | writes=next active size to B2, not B2 next size | note=format.rs:361 can_grow and 373 step_font
ROW format.decreaseFontSize | params={"sheet":"Data","range":"B2"} | resolves=Data B2 but active cell size determines result | holds=STYLE(Data,B2);active-cell font size | reads=part,selection | writes=previous active size | note=format.rs:367 can_shrink/373
ROW format.color | params={"sheet":"Data","range":"B2","color":"#CC0000"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=font colour;theme colours stored symbolic, no theme lookup | note=format.rs:405
ROW format.fillColor | params={"sheet":"Data","range":"B2","color":"#FF0000"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=solid fill or clears | note=format.rs:410
ROW format.borders | params={"sheet":"Data","range":"B2:C3","preset":"outside"} | resolves=Data explicit ranges; no filtered-row skip | holds=ADDRESS(Data,B2:C3);target style definitions/inherited styles;sheet protection.format_cells;CONNECTORS(Data) | reads=none | writes=selected boundary style fields; NO automatic FIT/touch | note=format.rs:437
ROW format.borders | params={"sheet":"Data","range":"B2:C3","preset":"none"} | resolves=Data selected cells PLUS facing edges on adjacent cells A2:A3,D2:D3,B1:C1,B4:C4 | holds=ADDRESS(Data,selected+adjacent edges);selected and adjacent inherited styles/borders;protection;CONNECTORS(Data) | reads=none | writes=selected borders reset;neighbour facing edges cleared | note=format.rs:546 neighbour branch
ROW format.cells | params={} | resolves=none | holds=none | reads=none | writes=dialog request only | note=format.rs:616 no style/edges/merge
ROW format.cells | params={"sheet":"Data","range":"B2","style":{"font":{"bold":true}}} | resolves=Data B2 target; validation merges patch into ACTIVE cell style first | holds=STYLE(Data,B2);active-cell resolved style for patch validation | reads=part,selection | writes=partial style; no content edit | note=format.rs:616
ROW format.cells | params={"sheet":"Data","range":"B2:C3","edges":[{"preset":"none"}],"merge":true} | resolves=Data B2:C3; nested border/merge callbacks; cell param discarded by extras forwarding | holds=border selected/neighbour styles;merge table+merge intersections;selected non-leading content/checkbox;FIT(Data,cleared rows);HEADERS(Data,cleared cells);CONNECTORS(Data) | reads=part,selection | writes=edges then merge; enabled can_merge checks active selection tables | note=format.rs:643 format_cells_extras; may fail after border edit
ROW format.alignLeft | params={"sheet":"Data","range":"B2"} | resolves=Data target, active sheet/cell horizontal align toggles | holds=STYLE(Data,B2);active-cell align.h | reads=part,selection | writes=left or General | note=format.rs:687 halign
ROW format.alignCenter | params={"sheet":"Data","range":"B2"} | resolves=Data target, active horizontal alignment | holds=STYLE(Data,B2);active-cell align.h | reads=part,selection | writes=center or General | note=format.rs:687
ROW format.alignRight | params={"sheet":"Data","range":"B2"} | resolves=Data target, active horizontal alignment | holds=STYLE(Data,B2);active-cell align.h | reads=part,selection | writes=right or General | note=format.rs:687
ROW format.justify | params={"sheet":"Data","range":"B2"} | resolves=Data target, active horizontal alignment | holds=STYLE(Data,B2);active-cell align.h | reads=part,selection | writes=justify or General | note=format.rs:687
ROW format.alignTop | params={"sheet":"Data","range":"B2"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=align.v top | note=format.rs:694 valign
ROW format.alignMiddle | params={"sheet":"Data","range":"B2"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=align.v center | note=format.rs:694
ROW format.alignBottom | params={"sheet":"Data","range":"B2"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=align.v bottom | note=format.rs:694
ROW format.wrapText | params={} | resolves=active cell wrap toggle; selection destinations | holds=STYLE(active,selection);active-cell wrap | reads=part,selection | writes=align.wrap and fitted rows | note=format.rs:304
ROW format.shrinkToFit | params={} | resolves=active cell shrink toggle; selection destinations | holds=STYLE(active,selection);active-cell shrink | reads=part,selection | writes=align.shrink | note=format.rs:304
ROW format.increaseIndent | params={"sheet":"Data","range":"B2"} | resolves=Data B2; own current indent+horizontal alignment | holds=STYLE(Data,B2) | reads=none | writes=indent+1, alignment becomes left for unsupported alignments | note=format.rs:698
ROW format.decreaseIndent | params={"sheet":"Data","range":"B2"} | resolves=Data B2; own indent/alignment | holds=STYLE(Data,B2) | reads=none | writes=indent-1 clamped, possibly left | note=format.rs:698
ROW format.orientation | params={"sheet":"Data","range":"B2","angle":"vertical"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=rotation 255 or numeric clamp | note=format.rs:707
ROW cell.mergeCenter | params={"sheet":"Data","range":"B2:C3"} | resolves=Data B2:C3; can_merge also checks ACTIVE selection tables | holds=ADDRESS(Data,B2:C3);active selection table intersection;Data overlapping tables/merges;nonleading cell values/formulas/styles/checkbox;FIT(Data,cleared rows);HEADERS(Data,cleared cells);STYLE(Data,B2:C3);CONNECTORS(Data) | reads=part,selection | writes=merge+center, or unmerge if exact merges already all present | note=format.rs:742
ROW cell.mergeAcross | params={"sheet":"Data","range":"B2:C3"} | resolves=Data B2:C3; can_merge active selection check | holds=ADDRESS(Data,B2:C3);active selection tables;Data overlapping tables/merges;CONNECTORS(Data) | reads=part,selection | writes=one merge per row; DOES NOT clear nonleading cell contents | note=format.rs:742 across branch
ROW cell.merge | params={"sheet":"Data","range":"B2:C3"} | resolves=Data B2:C3; active-selection enabled table check | holds=ADDRESS(Data,B2:C3);active selection tables;Data overlapping tables/merges;nonleading input/style/checkbox;protection;FIT(Data,cleared rows);HEADERS(Data,cleared);CONNECTORS(Data) | reads=part,selection | writes=merge and clears nonleading content for <=1000000 cells | note=format.rs:742
ROW cell.unmerge | params={"sheet":"Data","range":"B2:C3"} | resolves=Data B2:C3 | holds=ADDRESS(Data,B2:C3);intersecting merge entries;CONNECTORS(Data) | reads=none | writes=intersecting merges removed;no content/FIT | note=format.rs:742 unmerge branch
ROW format.numberFormat | params={"sheet":"Data","range":"B2","code":"0.000"} | resolves=Data B2; code or feature-name mapping | holds=STYLE(Data,B2) | reads=none | writes=num_fmt | note=format.rs:791
ROW format.accounting | params={"sheet":"Data","range":"B2","symbol":"EUR"} | resolves=Data B2; symbol ignored, fixed dollar accounting code | holds=STYLE(Data,B2) | reads=none | writes=num_fmt | note=format.rs:799 set_fmt
ROW format.percent | params={"sheet":"Data","range":"B2"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=num_fmt 0% | note=format.rs:799
ROW format.comma | params={"sheet":"Data","range":"B2"} | resolves=Data B2 | holds=STYLE(Data,B2) | reads=none | writes=fixed comma num_fmt | note=format.rs:799
ROW format.increaseDecimal | params={"sheet":"Data","range":"B2"} | resolves=Data target; active sheet/cell format AND computed value derive new code | holds=STYLE(Data,B2);active-cell num_fmt/value/spill and formula precedents if General | reads=part,selection | writes=active-derived number format on target | note=format.rs:881 decimals
ROW format.decreaseDecimal | params={"sheet":"Data","range":"B2"} | resolves=Data target; active cell format/value | holds=STYLE(Data,B2);active-cell num_fmt/value/spill/precedents | reads=part,selection | writes=active-derived format | note=format.rs:881
ROW style.apply | params={"sheet":"Data","range":"B2","name":"Custom"} | resolves=Data B2; name case-insensitive cell_styles lookup before builtin | holds=STYLE(Data,B2);chosen named cell-style definition or absence;theme major_font for builtin Title only | reads=none | writes=chosen style retaining old num_fmt if chosen General | note=format.rs:1079 only_fmt depends exact input spelling, not resolved style identity
ROW row.setHeight | params={"rows":"Data!2:3","height":20} | resolves=Data prefix; rows 1-based positions; explicit sheet wins prefix | holds=ADDRESS(Data,rows2:3);row size/hidden/custom;CONNECTORS(Data) | reads=none | writes=size,hidden=(height zero),custom=true; no FIT | note=format.rs:1132
ROW row.setHeight | params={"height":20} | resolves=active sheet; all selection row spans | holds=ADDRESS(active,selected rows);row size/hidden/custom;CONNECTORS(active) | reads=part,selection | writes=row properties | note=format.rs:1132 line_targets fallback
ROW column.setWidth | params={"sheet":1,"cols":"B:C","width":64} | resolves=sheet numeric tab index 1; B:C positional columns | holds=ADDRESS(index1,colsB:C);column size/hidden;CONNECTORS(index1) | reads=none | writes=size,hidden=(width zero), not custom | note=format.rs:1152 chars uses fixed 7px digit metric, not default font
ROW row.autofitHeight | params={"rows":"Data!2:3"} | resolves=Data prefix STRIPPED; ACTUAL sheet is active; ignores explicit sheet too | holds=ADDRESS(active,rows2:3);FIT(active,2:3);CONNECTORS(active) | reads=part | writes=active rows custom/hidden false and auto-fitted height | note=format.rs:1249 active-sheet mismatch
ROW column.autofitWidth | params={"cols":"Data!B:C"} | resolves=Data prefix STRIPPED; ACTUAL active sheet; used range clips source rows | holds=ADDRESS(active,colsB:C);used-range;column cells displayed value/styles/font/indent/date-system/merges;column size/hidden;CONNECTORS(active) | reads=part | writes=active column widths=max text width and hidden false if text exists | note=format.rs:1210 active-sheet mismatch
ROW column.setDefaultWidth | params={"sheet":"Data","width":80} | resolves=ALWAYS active sheet, sheet ignored | holds=active default_col_width;CONNECTORS(active) | reads=part | writes=active default width clamped | note=format.rs:1270
ROW row.hide | params={"rows":"Data!2:3"} | resolves=prefix stripped, ALWAYS active sheet | holds=ADDRESS(active,rows2:3);row hidden;HEADERS(all);CONNECTORS(active) | reads=part | writes=active rows hidden=true;structural full recalc | note=format.rs:1279 hide
ROW column.hide | params={"cols":"Data!B:C"} | resolves=prefix stripped, ALWAYS active sheet | holds=ADDRESS(active,colsB:C);column hidden;HEADERS(all);CONNECTORS(active) | reads=part | writes=active columns hidden=true | note=format.rs:1279
ROW row.show | params={"rows":"Data!2"} | resolves=ALWAYS active sheet; single row reveals adjacent rows 1:3 too | holds=ADDRESS(active,rows1:3);existing row hidden/zero-size state;HEADERS(all);CONNECTORS(active) | reads=part | writes=hidden=false and zero size cleared on materialized lines only | note=format.rs:1286 neighbour expansion
ROW column.show | params={"cols":"Data!B"} | resolves=ALWAYS active sheet; single B also A:C | holds=ADDRESS(active,A:C);existing column hidden/zero-size state;HEADERS(all);CONNECTORS(active) | reads=part | writes=hidden=false and zero size cleared | note=format.rs:1286
ROW cell.lock | params={} | resolves=active cell locked toggle; selection destinations | holds=STYLE(active,selection);active-cell protection.locked | reads=part,selection | writes=style locked flag | note=format.rs:304
# cmd/sheet.rs
ROW row.insert | params={"sheet":"Data","rows":"3:4"} | resolves=Data; rows positional range 3:4; rows prefix stripped rather than used for sheet | holds=STRUCTURE(Data,rows>=3);used-range room check;row BEFORE insertion cells/style/size, neighbour outline minimum;CF ending previous row;ADDRESS(Data,insertion) | reads=none | writes=insert rows, inherited formats sans checkbox, refs/features shifted | note=sheet.rs:409 format_inserted/unbox_inserted; count replaces span size
ROW row.insert | params={"below":true} | resolves=active sheet; selection row spans coalesced; inserts AFTER each run | holds=STRUCTURE(active,insertion bands);source row AFTER insertion styles/size;neighbour outlines;used-range;CF preceding boundary | reads=part,selection | writes=insert lines and selection;clipboard invalidated | note=sheet.rs:409 below reverses format source
ROW column.insert | params={"sheet":"Data","cols":"B:C","right":true} | resolves=Data; positional columns; insert after C;cols prefix ignored by target_sheet | holds=STRUCTURE(Data,cols>=D);source column AFTER new block styles/size;neighbour outlines;used range;CF boundary | reads=none | writes=insert columns;table column definitions and formulas updated | note=sheet.rs:409
ROW row.delete | params={"sheet":"Data","rows":"3:5"} | resolves=Data positional row blocks; filter-hidden rows INSIDE active filter criteria skipped | holds=STRUCTURE(Data,rows>=3);filter range/criteria and candidate row hidden;deleted line/cell locked styles;ADDRESS(Data,rows3:5) | reads=none | writes=visible requested rows removed and following shifted;clipboard invalidated | note=sheet.rs:493 deleted_runs
ROW column.delete | params={"sheet":"Data","cols":"B:C"} | resolves=Data positional columns; no filter skip for columns | holds=STRUCTURE(Data,cols>=B);deleted column/cell protection/locked styles;ADDRESS(Data,B:C) | reads=none | writes=columns removed, features/global references shifted | note=sheet.rs:493
ROW cell.insert | params={"sheet":"Data","range":"B2:C3","shift":"down"} | resolves=Data B2:C3; first range only | holds=STRUCTURE(Data,colsB:C rows>=2);ADDRESS(Data,B2:C3) | reads=none | writes=band shifted down, feature/global references adjusted;no format inherited | note=sheet.rs:532 shift non-row/column branch
ROW cell.insert | params={"sheet":"Data","range":"B2:C3","shift":"row"} | resolves=range rows2:3, but delegate DROPS sheet; ACTUAL active sheet | holds=STRUCTURE(active,rows>=2);previous row format and neighbour outlines;used-range room/protection | reads=part | writes=active rows inserted | note=sheet.rs:536 delegation wrong-sheet risk
ROW cell.delete | params={"sheet":"Data","range":"B2:C3","shift":"left"} | resolves=Data B2:C3; first range only | holds=STRUCTURE(Data,rows2:3 cols>=B);ADDRESS(Data,B2:C3) | reads=none | writes=band cells removed/shifted, features/references updated | note=sheet.rs:561
ROW cell.delete | params={"sheet":"Data","range":"B2:C3","shift":"column"} | resolves=columns B:C but delegate drops sheet; ACTUAL active sheet | holds=STRUCTURE(active,cols>=B);deleted column/cell protection | reads=part | writes=active columns removed | note=sheet.rs:566 wrong-sheet delegation
ROW sheet.insert | params={"name":"Added","before":1} | resolves=before numeric insertion position clamped; name explicit uniqueness lookup | holds=tab order/membership;all sheet names;protected_structure;defined-name scopes;active remembered selection | reads=part,selection | writes=sheet inserted, name scopes shifted, active sheet/selection and memories | note=sheet.rs:594
ROW sheet.insert | params={} | resolves=default unique SheetN name; insert AFTER active sheet | holds=tab order/membership and names;protected_structure;name scopes | reads=part,selection | writes=new active sheet at A1 | note=sheet.rs:594 default position/name
ROW sheet.delete | params={"sheet":"Data"} | resolves=Data by name; last-visible check | holds=Data whole sheet;all sheet visibility/tab order;protected_structure;affected formula/name/feature refs to Data and 3D order;defined scopes | reads=part,selection | writes=sheet removed, refs #REF or shifted 3D, names scoped removed;active/memory follows | note=sheet.rs:671
ROW sheet.rename | params={"sheet":"Data","name":"Renamed"} | resolves=Data by name; new name trimmed; validates all sheet names;no protected_structure check in handler | holds=Data name;all names for uniqueness;affected cell/feature formula/name/internal hyperlink references to old name;tab lookup | reads=none | writes=sheet name and refs;structural recalc;CONNECTORS changed sheets | note=sheet.rs:747
ROW sheet.move | params={"sheet":"Data","to":0} | resolves=Data source; to numeric final position clamped after removal | holds=tab order/membership;protected_structure;defined-name scopes;HEADERS(all);CONNECTORS(changed sheets) | reads=part,selection | writes=tab moved;name scope indexes;active/memory follows identity | note=sheet.rs:1008 noncopy
ROW sheet.move | params={"sheet":"Data","to":0,"copy":true} | resolves=Data copy;to insertion position clamped before addition | holds=source Data whole sheet;tab/names/table names/object ids for unique names/ids;source scoped names;protected_structure;HEADERS(all);CONNECTORS(copy) | reads=part,selection | writes=new sheet deep clone/rekeyed objects/tables/groups;rewritten self refs;new active A1 | note=sheet.rs:1008 sheet_copy;connector endpoint ids not remapped in sheet_copy
ROW sheet.tabColor | params={"sheet":"Data","color":"#FF0000"} | resolves=Data name; colour literal or symbolic | holds=Data tab_color;sheet name resolution;CONNECTORS(Data) | reads=none | writes=tab colour only plus shared tail | note=sheet.rs:1088
ROW sheet.hide | params={"sheet":"Data"} | resolves=Data name; counts visible sheets even if Data already hidden | holds=all sheet visibility/tab order;Data visibility;CONNECTORS(Data) | reads=part,selection | writes=visibility Hidden;active nearest visible and remembered selection | note=sheet.rs:1099
ROW sheet.unhide | params={"sheet":"Data","activate":false} | resolves=Data name; explicit sheet | holds=Data visibility;name resolution;CONNECTORS(Data) | reads=part,selection | writes=Visible;optional active switch and remembered selection | note=sheet.rs:1118 active read by before/follow even activate false
ROW sheet.unhide | params={} | resolves=all hidden sheet names/visibility | holds=tab membership and names/visibility | reads=none | writes=dialog request only | note=sheet.rs:1119
ROW sheet.activate | params={"sheet":"Data"} | resolves=Data name or 0-based numeric tab index; no visible-sheet validation | holds=sheet name/order membership;remembered selection | reads=part,selection,sheet_selections | writes=active sheet/selection/session memory | note=sheet.rs:1158
ROW sheet.next | params={} | resolves=next visible sheet to right, clamps at edge | holds=tab order/membership and searched visibility | reads=part,selection,sheet_selections | writes=active sheet/selection/memories | note=sheet.rs:1165
ROW sheet.previous | params={} | resolves=previous visible sheet to left, clamps edge | holds=tab order/membership and searched visibility | reads=part,selection,sheet_selections | writes=active sheet/selection/memories | note=sheet.rs:1165
"###;

// Pinned from the declaration after reviewing default paths omitted by explicit blind samples.
// This is separate from READINGS, which remains the original independent record.
pub fn reviewed_editor(command: &str) -> Option<(&'static [&'static str], &'static str)> {
    match command {
        "edit.clearAll" | "edit.clearFormats" | "edit.clearContents" | "edit.clearComments" | "edit.clearHyperlinks" => {
            Some((&["part", "selection"], "edit.rs clear -> target_sheet/target_ranges defaults"))
        }
        "edit.fillDown" | "edit.fillRight" | "edit.fillUp" | "edit.fillLeft" | "edit.fillSeries" | "edit.smartFill" => {
            Some((&["part", "selection"], "edit.rs fill_dir/fill_series/smart_fill -> target_range default"))
        }
        "format.fontName"
        | "format.fontSize"
        | "format.color"
        | "format.fillColor"
        | "format.alignTop"
        | "format.alignMiddle"
        | "format.alignBottom"
        | "format.increaseIndent"
        | "format.decreaseIndent"
        | "format.orientation"
        | "format.numberFormat"
        | "format.accounting"
        | "format.percent"
        | "format.comma"
        | "style.apply" => Some((&["part", "selection"], "format.rs apply_style -> target_sheet/target_ranges defaults")),
        "format.borders" | "cell.unmerge" => Some((&["part", "selection"], "format.rs borders/merge -> target_sheet/target_ranges defaults")),
        "column.setWidth" | "row.autofitHeight" | "column.autofitWidth" | "row.hide" | "column.hide" | "row.show" | "column.show" => {
            Some((&["part", "selection"], "format.rs line_sheet/line_targets default axes from selection"))
        }
        "column.insert" | "row.delete" | "column.delete" => {
            Some((&["part", "selection"], "sheet.rs insert_lines/delete_lines -> line_blocks default; implicit selection span"))
        }
        "cell.insert" | "cell.delete" => Some((&["part", "selection"], "sheet.rs insert_cells/delete_cells -> target_range default")),
        "sheet.rename" | "sheet.tabColor" => Some((&["part"], "sheet.rs rename_sheet/tab_color -> target_sheet default")),
        "sheet.delete" => Some((&["part"], "sheet.rs delete_sheet restores remembered selection, not current selection")),
        "edit.undo" | "edit.redo" => {
            Some((&["selection"], "edit.rs step_history saves/restores selection; snapshot active tab does not choose target"))
        }
        _ => None,
    }
}

#[test]
fn blind_rows_are_well_formed_and_cover_engine_specs() {
    use std::collections::BTreeSet;
    let mut found = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for line in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let fields: Vec<_> = line.split(" | ").collect();
        assert_eq!(fields.len(), 7, "{line}");
        let command = fields[0].strip_prefix("ROW ").unwrap_or_default();
        let params = fields[1].strip_prefix("params=").unwrap_or_default();
        let _: serde_json::Value = serde_json::from_str(params).unwrap_or_else(|e| panic!("{command}: {e}: {params}"));
        assert!(paths.insert((command, params)), "duplicate path: {command} {params}");
        for (field, prefix) in fields[2..].iter().zip(["resolves=", "holds=", "reads=", "writes=", "note="]) {
            assert!(field.starts_with(prefix), "{line}");
            assert!(field.len() > prefix.len(), "{line}");
        }
        found.insert(command);
    }
    let mut registry = BTreeSet::new();
    for source in [
        include_str!("../../../../crates/engine/engine/src/cmd/edit.rs"),
        include_str!("../../../../crates/engine/engine/src/cmd/format.rs"),
        include_str!("../../../../crates/engine/engine/src/cmd/sheet.rs"),
    ] {
        for after in source.split("cmd!(").skip(1) {
            let Some((_, literal)) = after.split_once('"') else { continue };
            let Some((id, _)) = literal.split_once('"') else { continue };
            registry.insert(id);
        }
    }
    assert_eq!(registry, found, "blind rows must enumerate every owned engine command");
    eprintln!("blind edit/format/sheet: {} command handlers, {} code-path rows", found.len(), paths.len());
}

/// Projections of the blind ENTRY/FIT/STRUCTURE facts onto the normalized model. These are
/// deliberately required-fact checks, not snapshots copied from `under`. The baseline can
/// decrease as missing read classes are repaired; increasing it requires owner review.
#[test]
fn required_edit_facts_have_an_only_decreasing_missing_floor() {
    use kition_kit::KitDocument;
    use kition_sheet_kit::SheetDocument;
    use serde_json::{Value, json};
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).expect("create fixture");
    doc.execute("range.setValues", json!({"range":"A1","values":[[1,2,3,4],[5,6,7,8],[9,10,11,12],[13,14,15,16],[17,18,19,20]]}))
        .expect("fixture cells");
    doc.execute("sheet.insert", json!({"name":"Sheet2"})).expect("second sheet");
    doc.execute("range.setValues", json!({"range":"A1","values":[[1,2,3,4],[5,6,7,8],[9,10,11,12],[13,14,15,16],[17,18,19,20]]}))
        .expect("second fixture cells");
    doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).expect("first active");
    doc.execute("select.set", json!({"range":"C5"})).expect("first selection");
    doc.version().expect("materialize observed identities");
    let field_held = |under: &kition_kit::Under, id: &str, field: &str| {
        under.premises.iter().any(|premise| {
            premise.id == id
                && premise.fields.as_ref().is_none_or(|fields| {
                    fields.iter().any(|held| held == field || field.strip_prefix(held.as_str()).is_some_and(|rest| rest.starts_with('.')))
                })
        })
    };
    // `derived` enumerates rows in sheet/row order. Querying the model supplies opaque row
    // identities without obtaining them from the declaration whose fields are being checked.
    let derived = doc.derived(&[], &["fit".into()]).expect("row identities");
    let row_ids: Vec<_> = derived.iter().filter_map(|value| value.id.as_str()).filter(|id| id.starts_with("row:")).collect();
    let row_at = |under: &kition_kit::Under, row: usize, field: &str| row_ids.get(row).is_some_and(|id| field_held(under, id, field));
    let mut missing = Vec::new();
    for (command, params, row) in [
        ("cell.set", json!({"cell":"B3","input":"x"}), 2),
        ("range.setValues", json!({"range":"B3","values":[["x"]]}), 2),
        ("range.fill", json!({"range":"B3","input":"x"}), 2),
        ("edit.clearContents", json!({"range":"B3"}), 2),
    ] {
        let under = doc.under(command, &params).expect(command).expect("premises");
        if !row_at(&under, row, "~fit") && !row_at(&under, row, "~layout") {
            missing.push(format!("{command}: automatic fitting reads the rest of row {}", row + 1));
        }
    }
    for (command, params, axis) in [("row.insert", json!({"rows":"3:3"}), "row"), ("column.insert", json!({"cols":"C:C"}), "col")] {
        let under = doc.under(command, &params).expect(command).expect("premises");
        if !under.premises.iter().any(|premise| {
            premise.id.as_str().is_some_and(|id| id.starts_with(axis))
                && premise.fields.as_ref().is_none_or(|fields| fields.iter().any(|field| field == "~insertFormat"))
        }) {
            missing.push(format!("{command}: inherits neighbouring cell and line formats"));
        }
    }
    for command in ["sheet.insert", "sheet.delete", "sheet.move"] {
        let params: Value = match command {
            "sheet.insert" => json!({"name":"Added"}),
            "sheet.delete" => json!({"sheet":"Sheet2"}),
            _ => json!({"sheet":"Sheet2","to":0}),
        };
        let under = doc.under(command, &params).expect(command).expect("premises");
        if !field_held(&under, "workbook", "props.protectedStructure") {
            missing.push(format!("{command}: workbook structure protection"));
        }
    }
    // The SDK sets the command's part before asking `under`; reproduce that context rather
    // than comparing with a first-sheet selection the handler will never see.
    doc.set_context(&kition_kit::CallContext { part: Some(json!({"sheet":"sheet:2"})), ..Default::default() }).expect("second-sheet context");
    let under = doc.under("format.increaseFontSize", &json!({"range":"Sheet2!B2"})).expect("font premises").expect("under");
    if !field_held(&under, "cell:2!A1", "style") {
        missing.push("format.increaseFontSize: target sheet's remembered active cell A1, not first sheet's C5".into());
    }
    eprintln!("edit required facts missing {}: {}", missing.len(), missing.join("; "));
    assert!(missing.is_empty(), "missing fact floor only decreases: {missing:?}");
}

// Reviewed projections added after blind commit ead3822. The prose READINGS above remains
// byte-for-byte the blind record. These typed checks project selected recorded facts onto KDM
// fields and reference roles; they are not a claim that every prose fact has an executable
// check. In particular HEADERS, CONNECTORS, transitive formula sources and snapshot payloads
// need the independent disk-writer behaviour tests as well.
fn field_held(under: &kition_kit::Under, object: &str, field: &str) -> bool {
    // Pinned after SDK ownership review and the independent source-deletion writer test:
    // a whole owning Sheet premise also covers its child Cell data.
    let owner = object.strip_prefix("cell:").and_then(|address| address.split_once('!')).map(|(sheet, _)| format!("sheet:{sheet}"));
    under.premises.iter().any(|premise| {
        owner.as_ref().is_some_and(|owner| premise.id == *owner && premise.fields.is_none())
            || premise.id == object
                && premise.fields.as_ref().is_none_or(|fields| {
                    fields.iter().any(|held| held == field || field.strip_prefix(held).is_some_and(|suffix| suffix.starts_with('.')))
                })
    })
}

#[test]
fn independently_read_edit_reference_roles_have_only_decreasing_missing_floor() {
    use kition_kit::KitDocument;
    use std::collections::BTreeSet;
    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    let catalogue = doc.commands();
    let mut compared = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut engine_only = BTreeSet::new();
    for row in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let fields: Vec<_> = row.split(" | ").collect();
        let id = fields[0].trim_start_matches("ROW ");
        let command = catalogue.iter().find(|command| command.id == id).expect("registered handler");
        if !command.mutates {
            continue;
        }
        compared.insert(id);
        let params: serde_json::Value = serde_json::from_str(fields[1].trim_start_matches("params=")).expect("blind parameters");
        for key in params.as_object().expect("object parameters").keys() {
            // Pinned after schema review: engine helpers accept additional keys which the
            // public binding rejects. Those samples cannot establish a public role deficit.
            if command.schema.as_ref().is_none_or(|schema| schema.get(key).is_none()) {
                engine_only.insert(format!("{id}.{key}"));
                continue;
            }
            let names_place = match key.as_str() {
                "sheet" => id != "column.setDefaultWidth",
                "cell" => true,
                "range" => id != "edit.replace",
                "rows" | "cols" => true,
                "at" => matches!(id, "edit.paste" | "edit.pasteSpecial"),
                "source" | "target" => id == "edit.autoFill",
                "apply" => id == "edit.formatPainter",
                "from" => id == "edit.moveCells",
                "to" => matches!(id, "edit.moveCells" | "sheet.move"),
                "before" => id == "sheet.insert",
                "name" => matches!(id, "cell.set" | "style.apply"),
                _ => false,
            };
            if names_place
                && !command.places.contains(&key.as_str())
                && !command.objects.contains(&key.as_str())
                && command.targets != Some(key.as_str())
            {
                missing.insert(id);
                eprintln!("REFERENCE LESS {id}.{key}: {}", fields[2]);
            }
            if key == "sheet" && names_place {
                assert!(command.object_types.iter().any(|(key, types)| *key == "sheet" && types.contains(&"Sheet")), "{id}: sheet type");
            }
            if key == "name" && id == "cell.set" {
                assert!(command.object_types.iter().any(|(key, types)| *key == "name" && types.contains(&"Name")), "cell.set: defined-name type");
            }
        }
    }
    assert_eq!(compared.len(), 84, "every registered edit/format/sheet write needs a role projection");
    // These are address-valued parameters, not object-id arrays. This comparison deliberately
    // does not infer excess roles from an explicit-parameter sample that omits a default path.
    let baseline: BTreeSet<&str> = BTreeSet::new();
    eprintln!("edit reference projection: {} writes; LESS {missing:?}; engine-only sample keys excluded={engine_only:?}", compared.len());
    assert!(missing.is_subset(&baseline), "new missing edit reference roles: {missing:?}");
}

/// Each registered write is matched explicitly. Required-field checks catch missing recorded
/// inputs; the second-sheet numeric sentinel catches a broad hold outside the recorded read
/// footprint. Undo/redo legitimately read whole snapshots, so have no sentinel exclusion.
#[test]
fn all_edit_read_footprints_have_only_decreasing_missing_and_excessive_floors() {
    use kition_kit::KitDocument;
    use kition_kit::kdm::{ActorId, Replica, tool_ids::ToolIds};
    use kition_kit::session::KitModel;
    use kition_sheet_engine::core::RangeRef;
    use kition_sheet_kit::SheetDocument;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    doc.execute("sheet.rename", json!({"name":"Data"})).expect("first sheet name");
    for sheet in ["Data", "Second"] {
        if sheet == "Second" {
            doc.execute("sheet.insert", json!({"name":sheet})).expect("second sheet");
        }
        doc.execute("range.setValues", json!({"range":"A1","values":[[1,2,3,4,5],[6,7,8,9,10],[11,12,13,14,15],[16,17,18,19,20],[21,22,23,24,25],[26,27,28,29,30],[31,32,33,34,35],[36,37,38,39,40]]})).expect("cells");
    }
    doc.execute("cell.set", json!({"cell":"Z20","input":"99"})).expect("unrelated constant");
    doc.execute("name.define", json!({"name":"Destination","refersTo":"=Second!$B$2"})).expect("cross-sheet name");
    doc.execute("sheet.activate", json!({"sheet":"Data"})).expect("active sheet");
    doc.execute("select.set", json!({"range":"C5"})).expect("distinct active cell");
    doc.execute("style.add", json!({"name":"Custom"})).expect("named cell style");
    doc.execute("edit.copy", json!({"range":"B2:C3"})).expect("clipboard snapshot");
    doc.execute("edit.formatPainter", json!({"sticky":true})).expect("painter snapshot");
    doc.version().expect("observed identities");
    let model = Replica::load(kdm_model::SheetModel::schema(), &doc.snapshot().expect("snapshot"), ActorId::random()).expect("fixture model");
    let names = ToolIds::of_replica(&model);
    let named_sheet = names.resolve("sheet:2", |id| model.exists(id)).expect("named destination sheet");
    let named_rows = model.list(&named_sheet, "rows").expect("named destination rows");
    let named_row = format!("row:{}", named_rows.get(1).expect("named destination row 2"));
    let catalogue = doc.commands();
    let mut compared = BTreeSet::new();
    let mut less = BTreeSet::new();
    let mut more = BTreeSet::new();
    let mut samples: Vec<_> = READINGS
        .lines()
        .filter_map(|line| line.strip_prefix("ROW "))
        .map(|line| {
            let fields: Vec<_> = line.split(" | ").collect();
            (fields[0], serde_json::from_str::<Value>(fields[1].trim_start_matches("params=")).expect("parameters"))
        })
        .collect();
    // Pinned from the declaration after review: the blind cellStyle note records Title's
    // theme-font branch, but its original concrete sample used Custom. Preserve that sample.
    samples.push(("style.apply", json!({"sheet":"Data","range":"B2","name":"Title"})));
    for (id, params) in samples {
        if !catalogue.iter().any(|command| command.id == id && command.mutates) {
            continue;
        }
        compared.insert(id);
        let under = doc.under(id, &params).unwrap_or_else(|error| panic!("{id} {params}: {error}")).unwrap_or_default();
        let second = params.get("sheet").and_then(Value::as_u64) == Some(1);
        let owner = if second { 2 } else { 1 };
        let mut required: Vec<(String, &'static str)> = Vec::new();
        let target = params
            .get("range")
            .or_else(|| params.get("cell"))
            .and_then(Value::as_str)
            .and_then(|text| RangeRef::parse(text.rsplit_once('!').map_or(text, |(_, body)| body)))
            .map(|range| range.start.a1())
            .unwrap_or_else(|| "C5".into());
        let mut cell = |at: &str, field| required.push((format!("cell:{owner}!{at}"), field));
        match id {
            "cell.set" => {
                if params.get("name").is_some() {
                    // Pinned after derived-field review: named entry deliberately holds the
                    // resolved persistent row, whose layout digest includes complete cells
                    // and resolved styles, rather than freezing the cell's A1 tool address.
                    if !field_held(&under, &named_row, "~layout") {
                        required.push(("cell:2!B2".into(), "style"));
                    }
                } else {
                    let target = if params.get("array").and_then(Value::as_bool) == Some(true) { "C5" } else { &target };
                    cell(target, "style");
                    cell(target, "input");
                }
            }
            "range.setValues" | "range.fill" => {
                cell(&target, "style");
                cell(&target, "input");
                if id == "range.setValues" && params.get("range").is_some() {
                    cell("C3", "style");
                }
            }
            "cell.formatText" => {
                cell(&target, "style");
                cell(&target, "text");
            }
            "edit.paste" | "edit.pasteSpecial" => {
                let at = params.get("at").and_then(Value::as_str).unwrap_or("C5");
                cell(at, "style");
            }
            "edit.clearAll" | "edit.clearContents" => cell(&target, "input"),
            "edit.clearFormats" => cell(&target, "style"),
            "edit.clearComments" | "edit.clearHyperlinks" => {
                // Sparse map keys contain opaque axis ids. Address windows remain a
                // mechanical projection; exact sparse-member behaviour is tested separately.
                assert!(under.within.iter().any(|window| window.parent == "sheet:1" && window.field == "rows"), "{id}: address window");
            }
            "edit.fillDown" | "edit.fillRight" | "edit.fillUp" | "edit.fillLeft" => {
                let source = match (id, params["range"].as_str()) {
                    ("edit.fillDown", Some("B2:C2")) => "B1",
                    ("edit.fillRight", _) => "A2",
                    ("edit.fillUp", _) => "B3",
                    ("edit.fillLeft", _) => "C2",
                    _ => "B2",
                };
                cell(source, "input");
            }
            "edit.autoFill" => cell(if params.get("source").is_some() { "B2" } else { "C5" }, "style"),
            "edit.fillSeries" => cell("B2", "input"),
            "edit.smartFill" => cell("B4", "input"),
            "edit.replace" => cell("C5", "input"),
            "edit.formatPainter" if params.get("cancel").is_none() => cell(if params.get("apply").is_some() { "B2" } else { "C5" }, "style"),
            "edit.formatPainter" | "edit.undo" | "edit.redo" => {}
            "edit.moveCells" => cell(if params.get("from").is_some() { "B2" } else { "C5" }, "input"),
            "format.bold"
            | "format.italic"
            | "format.underline"
            | "format.doubleUnderline"
            | "format.strikethrough"
            | "format.superscript"
            | "format.subscript"
            | "format.fontName"
            | "format.fontSize"
            | "format.increaseFontSize"
            | "format.decreaseFontSize"
            | "format.color"
            | "format.fillColor"
            | "format.alignLeft"
            | "format.alignCenter"
            | "format.alignRight"
            | "format.justify"
            | "format.alignTop"
            | "format.alignMiddle"
            | "format.alignBottom"
            | "format.wrapText"
            | "format.shrinkToFit"
            | "format.increaseIndent"
            | "format.decreaseIndent"
            | "format.orientation"
            | "format.numberFormat"
            | "format.accounting"
            | "format.percent"
            | "format.comma"
            | "format.increaseDecimal"
            | "format.decreaseDecimal"
            | "cell.lock" => cell(&target, "style"),
            "format.borders" => {
                cell(&target, "style");
                if params["preset"] == "none" {
                    cell("A2", "style");
                    cell("D3", "style");
                }
            }
            "format.cells" if params.get("style").is_some() || params.get("edges").is_some() => cell(&target, "style"),
            "format.cells" => {}
            "cell.mergeCenter" | "cell.merge" => cell("C3", "input"),
            "cell.mergeAcross" | "cell.unmerge" => {
                assert!(under.within.iter().any(|window| window.parent == "sheet:1" && window.field == "rows"), "{id}: merge address window");
            }
            "style.apply" => {
                cell(&target, "style");
                if params["name"] == "Title" {
                    required.push(("workbook".into(), "theme.major_font"));
                }
            }
            "row.setHeight"
            | "column.setWidth"
            | "row.autofitHeight"
            | "column.autofitWidth"
            | "row.hide"
            | "column.hide"
            | "row.show"
            | "column.show" => {
                let prefix = if matches!(id, "column.setWidth" | "column.autofitWidth" | "column.hide" | "column.show") { "col:" } else { "row:" };
                assert!(
                    under.premises.iter().any(|premise| premise.id.as_str().is_some_and(|id| id.starts_with(prefix))),
                    "{id}: resolved axis reads"
                );
            }
            "column.setDefaultWidth" => required.push(("sheet:1".into(), "props.defaultColWidth")),
            "row.insert" | "column.insert" | "row.delete" | "column.delete" | "cell.insert" | "cell.delete" => {
                required.push(("sheet:1".into(), "props.protection"));
            }
            "sheet.insert" => {
                required.extend([("workbook".into(), "props.protectedStructure"), ("sheet:2".into(), "name")]);
                let at = params.get("before").and_then(Value::as_u64).unwrap_or(1) as usize;
                if !under.within.iter().any(|window| window.parent == "workbook" && window.field == "sheets" && window.from == at && window.to == at)
                {
                    less.insert(id);
                    eprintln!("FOOTPRINT LESS {id} params={params}: default insertion is after the active tab, workbook.sheets gap {at}");
                }
            }
            "sheet.delete" => {
                required.extend([
                    ("workbook".into(), "props.protectedStructure"),
                    ("sheet:2".into(), "props.visibility"),
                    ("cell:1!B2".into(), "input"),
                ]);
            }
            "sheet.rename" => required.extend([("sheet:1".into(), "name"), ("sheet:2".into(), "name")]),
            "sheet.move" => {
                required.push(("workbook".into(), "props.protectedStructure"));
                if params.get("copy").and_then(Value::as_bool) == Some(true) {
                    required.extend([("sheet:2".into(), "name"), ("cell:1!B2".into(), "input"), ("cell:1!B2".into(), "style")]);
                }
            }
            "sheet.tabColor" => required.push(("sheet:1".into(), "props.tabColor")),
            "sheet.hide" => required.extend([("sheet:1".into(), "props.visibility"), ("sheet:2".into(), "props.visibility")]),
            "sheet.unhide" if params.get("sheet").is_some() => required.push(("sheet:1".into(), "props.visibility")),
            "sheet.unhide" => {}
            _ => panic!("registered write lacks a typed read projection: {id}"),
        }
        let missing: Vec<_> = required.iter().filter(|(object, field)| !field_held(&under, object, field)).collect();
        if !missing.is_empty() {
            less.insert(id);
            eprintln!("FOOTPRINT LESS {id} params={params}: {missing:?}");
        }
        if !matches!(id, "edit.undo" | "edit.redo") && field_held(&under, "cell:2!Z20", "input") {
            more.insert(id);
        }
    }
    assert_eq!(compared.len(), 84, "all registered family writes must have typed probes");
    let missing_baseline: BTreeSet<&str> = BTreeSet::new();
    let excessive_baseline = BTreeSet::from([
        "edit.paste",
        "edit.pasteSpecial",
        "edit.autoFill",
        "edit.fillSeries",
        "edit.smartFill",
        "edit.replace",
        "edit.formatPainter",
        "edit.moveCells",
        "cell.insert",
        "cell.delete",
    ]);
    eprintln!("edit footprint projection: {} writes; LESS {less:?}; MORE {more:?}", compared.len());
    assert!(less.is_subset(&missing_baseline), "new missing edit facts: {less:?}");
    assert!(more.is_subset(&excessive_baseline), "new unrelated-cell edit holds: {more:?}");
    assert_eq!(more, excessive_baseline, "coverage improved: decrease this baseline, never add a command");
}
