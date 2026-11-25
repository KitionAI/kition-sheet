//! Blind engine reading for C8-sheet-read. Written before opening binding declarations/tests.
//! Inline same-field decision notes identify later replacement-hold amendments.
//!
//! `part` is the workbook's active sheet; `active` is selection.active; `selection` is the
//! selected range(s). They are semantic reads, not claims about the SDK enum spelling.
//! Common operation facts named below are expanded here to keep each command's row readable:
//! `commit` = destination sheet protection and old locked-cell style for changed content;
//! changed header cells/table column names and dependent structured-reference formulas;
//! connectors/endpoints/axis geometry on changed sheets; cell/style/number-format/merge/column
//! geometry and default font/height for automatically fitted changed rows. A structural edit
//! checks every table header before recalculation. Recalculation's outputs are consequences;
//! explicit values used to choose an edit, including trial results, are premises.
//! `region` includes the empty boundary cells tested by selection::current_region, including
//! diagonals; `data-range` additionally checks table membership and used-range trimming.
//! `pivot-refresh` = chosen pivot definition; source table/name definition or A1 sheet/window;
//! source headers with display formats, all source values and first data row's formats; date
//! system for grouped labels; old and computed output cells/spills; destination protection,
//! intersecting tables/pivots; destination column width/hidden state; commit's structural reads.
//! No rows were pinned from a declaration after review.

pub const READINGS: &str = r##"
ROW data.sortAscending | params={} | resolves=column letter absolute or numeric offset from data-range; omitted column is active.col; range prefix ignored; active sheet only | holds=data-range including empty region boundaries; table header flag or first two rows values/bold styles for guessed header; whole sort body cells/formulas/pictures; body merges; row info with hidden retained; floating anchors and old/new axis geometry; filter criteria and criterion source cells/display styles; commit | reads=part,selection,active | writes=reordered rows/formulas/pictures/row heights and floating anchors; filters reapplied | note=data.rs:185,230; default expanding range
ROW data.sortAscending | params={"range":"Sheet2!A1:C5","column":"B"} | resolves=range body A1:C5 on ACTIVE sheet despite Sheet2 prefix | holds=explicit range row/column windows; guessed header values/bold; sort body cells/formulas/pictures/merges/rows/floating anchors/geometry; filter inputs; commit | reads=part | writes=sorts active sheet | note=data.rs:144,185; explicit range path
ROW data.sortDescending | params={} | resolves=same data-range and column resolution as sortAscending | holds=data-range; guessed header values/bold/table header flag; body cells/formulas/pictures/merges; rows; floating anchors/geometry; filter criteria/source/display; commit | reads=part,selection,active | writes=descending sort with blanks last | note=data.rs:185,230
ROW data.sort | params={"range":"A1:C5","header":false,"keys":[{"column":"B","order":"asc"}]} | resolves=keys[].column letters absolute or numeric offsets; key row is one-based in columns orientation; active sheet regardless range prefix | holds=range windows; body cells/formulas/pictures/merges; rows if sorting rows; floating anchors/geometry; all re-applied filter criteria and sources; commit | reads=part | writes=sort body | note=data.rs:195,230; explicit header avoids header guess
ROW data.sort | params={"keys":[{"column":"B","order":"asc","by":"cellColor"}]} | resolves=default data-range; omitted header guessed; fontColor analogous | holds=data-range; guessed header; body cells/pictures/merges/rows; key resolved fill/font colors including theme and inherited styles; anchors/geometry; filter inputs; commit | reads=part,selection,active | writes=color sort | note=data.rs:195,244; styles/theme only color keys
ROW data.sort | params={} | resolves=data-range and guessed header | holds=data-range; header guess values/bold/table header | reads=part,selection,active | writes=sort dialog only | note=data.rs:216; empty keys branch
ROW data.filter | params={} | resolves=active-cell table first; active sheet AutoFilter second; otherwise data-range | holds=table extents/filterButton at active cell; else AutoFilter existence/range and every row hidden flag when removing; else data-range | reads=part,active,selection | writes=toggles chosen table button or removes sheet filter and unhides all stored rows or creates filter | note=data.rs:439
ROW data.filter | params={"range":"A1:C5"} | resolves=explicit range ONLY used when no table at active cell and no sheet filter | holds=active-cell table membership/button; AutoFilter existence; range windows when creating | reads=part,active | writes=toggles/creates filter | note=data.rs:439; range does not override active-cell table
ROW data.filterBy | params={"table":"Sales","column":"B","values":["yes"]} | resolves=case-insensitive table name on ACTIVE sheet only; column letter absolute or numeric offset from filter start | holds=table filter extent/header/filterButton/criteria; every criterion column value and display style; workbook date/format settings for display; filter row hidden destinations; commit | reads=part | writes=criteria and row visibility | note=data.rs:528,547,614; clear/custom/top/aboveAverage share apply path
ROW data.filterBy | params={"cell":"Sheet2!B1","values":["yes"]} | resolves=cell prefix ignored; filter_scope_at active-sheet B1 header first then containment; column defaults cell.col | holds=active-sheet filter/table memberships/extents/criteria; criterion columns values/display; output row visibility; commit | reads=part | writes=filter criterion and row visibility | note=data.rs:528,547; header-address path
ROW data.filterBy | params={"column":0,"aboveAverage":true} | resolves=sheet filter else active-cell table else toggle_filter creates from data-range | holds=AutoFilter existence; table membership at active; region boundaries/used range if creating; complete filter criterion column values/display and average/top statistics; output row visibility; commit | reads=part,active,selection | writes=creates filter if absent then changes criterion/visibility | note=data.rs:547; implicit creation
ROW data.clearFilter | params={"table":"Sales"} | resolves=active-sheet table by case-insensitive name; explicit cell uses header/containment | holds=chosen table extent/header/filterButton/criteria and output row hidden flags; commit | reads=part | writes=clears table criteria and shows its filtered rows | note=data.rs:723
ROW data.clearFilter | params={} | resolves=sheet AutoFilter else active-cell table; clearing sheet also reapplies all table filters with criteria | holds=sheet filter; active table membership if no filter; every reapplied table criterion/value/display source; row hidden destinations; commit | reads=part,active | writes=clears sheet criteria and reapplies scopes | note=data.rs:723,694
ROW data.reapply | params={} | resolves=every filter on active sheet | holds=sheet/table filter extents/header/button/criteria; all criterion values/display formats and date system; row visibility destinations; commit | reads=part | writes=reapplies filter visibility | note=data.rs:759
ROW data.removeDuplicates | params={} | resolves=data-range clipped to used range; columns letters absolute or numeric offsets; header guessed when absent | holds=data-range and used range; header/table/bold guess; comparison cells including columns outside range if requested; full body cells/pictures; fitted rows/commit | reads=part,selection,active | writes=compacts kept cells/pictures in range; removes duplicate tails; formulas copied without relative adjustment | note=data.rs:900
ROW data.textToColumns | params={"range":"Sheet2!A1:A5","destination":"Sheet3!B2","delimiters":[","]} | resolves=source sheet via sheet param/index/name or range prefix; destination PREFIX IGNORED and B2 is on source sheet; only source first column read | holds=source first-column values/windows and computed precedents; data-dependent output rectangle/windows; destination inherited styles/checkbox/numfmt; date system; parsed formula names/dependencies; commit; replaced values: each output cell input,text,array,picture | reads=none | writes=split strings interpreted as entries across source-sheet output rectangle | note=data.rs:964; invalid input_to_cell errors swallowed into empty cell; same-field decision
ROW data.textToColumns | params={} | resolves=selection first range/active sheet; destination defaults range.start; tabs by default; fixedWidths uses character cuts | holds=selection source values/windows; computed output rectangle; destination styles/date system/formula names/dependencies; commit; replaced values: each output cell input,text,array,picture | reads=part,selection | writes=split entries | note=data.rs:964; same-field decision
ROW data.validation | params={"range":"Sheet2!A1:A3","type":"list","formula1":"Sheet3!A1:A5"} | resolves=target_sheet index/name/range prefix; target_ranges all A1 areas | holds=destination row/column windows and overlapping validation definitions/ranges; no list values are evaluated by this handler | reads=none | writes=subtracts destination from old validation ranges then appends literal new definition | note=data.rs:1109; formula1 is stored text, not read now
ROW data.validation | params={"clear":true} | resolves=active sheet and selection areas | holds=selection address windows and overlapping validation definitions | reads=part,selection | writes=subtracts selected cells from rules preserving fragments | note=data.rs:1112
ROW data.validation | params={} | resolves=target sheet and selection | holds=none beyond addressing | reads=part,selection | writes=dialog only | note=data.rs:1121
ROW data.circleInvalid | params={} | resolves=active sheet validations clipped to used range; 100000 cells per rule | holds=validation definitions; used range; checked cell input; date system; validation formula/list names/source windows and precedents | reads=part | writes=none; returns invalid cell list | note=data.rs:1252; registered noundo but read-only
ROW data.group | params={"rows":"2:5"} | resolves=rows or cols textual span on active sheet; no sheet parameter | holds=addressed axis outline levels and address windows | reads=part | writes=outline level plus one capped seven | note=data.rs:1280,1296
ROW data.group | params={} | resolves=selection current; full columns select columns otherwise rows | holds=selected axis outline levels/windows | reads=part,selection | writes=increments outline | note=data.rs:1280
ROW data.ungroup | params={"cols":"B:C"} | resolves=active-sheet column span; default selection same as group | holds=axis outline and hidden fields/windows | reads=part | writes=decrements outline; unhides when zero | note=data.rs:1296
ROW data.hideDetail | params={"rows":"3"} | resolves=active sheet span; maximum selected outline level expands both ways over >=level | holds=selected and expanded neighbour outline levels including stopping boundary; expanded hidden flags/address windows | reads=part | writes=hides full expanded group | note=data.rs:1314
ROW data.showDetail | params={} | resolves=selection span; maximum selected outline expands to full surrounding group | holds=selected/expanded/stopping-neighbour outline fields/windows and hidden destinations | reads=part,selection | writes=shows expanded group | note=data.rs:1314
ROW data.subtotal | params={"range":"A1:C5","groupBy":"A","columns":["C"]} | resolves=active-sheet data-range; groupBy/columns letters absolute or offsets; header always assumed despite header param | holds=data-range; group labels including sentinel row immediately after range; all rows inserted through sheet tail and structural formulas/features/names; insertion formatting neighbours; destination cell styles/constraints; aggregate source windows/values; commit | reads=part | writes=inserted total rows, SUBTOTAL formulas, bold, outline and grand total via sequential nested execute | note=data.rs:1354; ordinary error after earlier nested writes can leave partial subtotal
ROW data.smartFill | params={} | resolves=delegates edit.smartFill unchanged | holds=edit.smartFill source examples/neighbour patterns and output facts | reads=part,selection,active | writes=smart-fill output | note=data.rs:126; delegate row audited in edit family
ROW table.insert | params={"range":"Sheet2!A1:C5","header":true,"name":"Sales"} | resolves=target sheet by index/name/range prefix; table range A1; enabled still reads ACTIVE sheet protection and selection area count | holds=active-sheet protection/selection enabled; target protection; range cells and overlapping arrays/merges/tables; header values/pictures; global table/name uniqueness; chosen default table style only if omitted; address windows; commit | reads=part,selection | writes=table; clears merges; headers become unique text; may clear header pictures | note=insert.rs:265,307; explicit range/header
ROW table.insert | params={"range":"A1:C5","header":false} | resolves=active sheet; generated headers insert cells only in chosen columns down entire sheet | holds=range plus column tails cells/pictures; merges/arrays/tables; global table/name uniqueness; chosen default style; prefix preference; commit | reads=part,selection | writes=shifts column cells/pictures down one without shared structural reference rewrite; inserts header/table | note=insert.rs:399; inserted-header path
ROW table.insert | params={} | resolves=selection block else active-cell current region; full rows/columns clip target used range | holds=selection/region empty boundaries/used range; active protection; first row values for header guess; tables/arrays/merges/header pictures; name uniqueness; default table style; commit | reads=part,selection,active,prefs.column_prefix | writes=table and normalized headers | note=insert.rs:246,307
ROW table.formatAs | params={"range":"A1:C5","header":true} | resolves=same insert_table path | holds=table.insert facts including active protection/selection enabled | reads=part,selection,prefs.column_prefix | writes=table | note=insert.rs:307; table mode
ROW table.formatAs | params={"range":"Sheet2!A1:C5","mode":"format","style":"MyTableStyle"} | resolves=target sheet as target_sheet; enabled checks active sheet/selection count | holds=range cells inherited styles; chosen custom style definition; row hidden state for banding; active protection and selection enabled; output row fitting/commit | reads=part,selection | writes=derived plain cell styles only | note=insert.rs:462; range_looks tables.rs:528
ROW table.newStyle | params={"name":"NewStyle","elements":{"wholeTable":{"bold":true}}} | resolves=name case-insensitive custom style uniqueness | holds=existing custom style names and default_table only if default false/true branch reads it | reads=none | writes=new table-style definition and optionally default | note=insert.rs:573
ROW table.newStyle | params={"name":"Renamed","replace":"OldStyle","default":false,"elements":{}} | resolves=replace case-insensitive custom style name; name uniqueness; default false final check case-sensitive | holds=old chosen style definition/uid; custom name uniqueness; default_table; every table whose style names OldStyle and membership of those users | reads=none | writes=replaces style; rewrites referring table styles and default | note=insert.rs:573,622
ROW table.deleteStyle | params={"name":"OldStyle"} | resolves=case-insensitive custom style name | holds=chosen style definition; default table-style setting/fallback definition; every table referencing that style and their membership | reads=none | writes=removes style and reassigns table users to fallback | note=insert.rs:654
ROW table.setDefaultStyle | params={"name":"MyTableStyle"} | resolves=builtin exact name or case-insensitive custom definition; empty clears | holds=chosen custom style name/existence; old default is destination; replaced values: default_table | reads=none | writes=default_table | note=insert.rs:680; same-field decision
ROW table.totalRow | params={"table":"Sales","on":true} | resolves=table name workbook-wide; omitted table active-cell containment | holds=chosen table definition; output total row existing cells/styles/windows; current totals flag and last column totals default; SUBTOTAL source/table rows and hidden flags; commit | reads=none | writes=extends table and writes total labels/formulas or removes old total row | note=insert.rs:696,713,765
ROW table.totalRow | params={} | resolves=table at active cell on active sheet | holds=chosen table extent/totals/column definitions; total destination styles/cells; subtotal sources; commit | reads=part,active | writes=toggles total row | note=insert.rs:713
ROW table.bandedRows | params={"table":"Sales","on":true} | resolves=table name workbook-wide; default active-cell table | holds=chosen table definition/extent; banded_rows only read when on omitted; replaced values: banded_rows even with explicit on | reads=none | writes=banded_rows flag | note=insert.rs:713; same-field decision
ROW table.bandedColumns | params={"table":"Sales"} | resolves=table name workbook-wide; default active-cell table | holds=chosen table definition/banded_cols flag | reads=none | writes=toggles banded_cols | note=insert.rs:713
ROW table.filterButton | params={} | resolves=active-cell table | holds=table membership/extent/filter_button | reads=part,active | writes=toggles filter_button | note=insert.rs:713
ROW table.firstColumn | params={"table":"Sales","on":true} | resolves=table name workbook-wide | holds=chosen table definition | reads=none | writes=first_col flag | note=insert.rs:713
ROW table.lastColumn | params={"table":"Sales"} | resolves=table name workbook-wide | holds=chosen table last_col flag | reads=none | writes=toggles last_col | note=insert.rs:713
ROW table.headerRow | params={"table":"Sales","on":false} | resolves=table name workbook-wide else active-cell table | holds=table extent/header flag/column names; removed header cells' inherited styles and row/column windows; selection.active decides relocation; structural commit; replaced values: removed header input,text,array,picture | reads=active | writes=clears header content preserving styles; shrinks table; moves active selection if on header | note=insert.rs:819; same-field decision
ROW table.headerRow | params={"table":"Sales","on":true} | resolves=table name workbook-wide; row above data is new header | holds=table definition; destination header cells/styles/windows/protection; structural commit | reads=active | writes=restores column labels to row above; expands table | note=insert.rs:819
ROW table.style | params={"table":"Sales","style":"MyTableStyle"} | resolves=table name workbook-wide; style text stored without resolving definition | holds=chosen table definition; style assignment does not inspect custom definition | reads=none | writes=table.style only | note=insert.rs:867
ROW table.rename | params={"table":"Sales","name":"Revenue"} | resolves=table name workbook-wide; new name uniqueness against tables only | holds=chosen table; global other-table names; cell formulas referencing old table name including newly created references | reads=none | writes=table name and matching cell formulas; no names/chart/pivot formula rewrite | note=insert.rs:895
ROW table.convertToRange | params={"table":"Sales"} | resolves=table name workbook-wide | holds=table definition; full table cell styles; selected custom table style definition; hidden rows/columns for visible band index; table window; commit | reads=none | writes=materializes table look into cell styles and removes table | note=insert.rs:943; tables.rs:510
ROW table.totalFunction | params={"table":"Sales","column":"Amount","function":"average"} | resolves=table name workbook-wide; column name via case-insensitive table column_index | holds=table column definitions/totals status; entire totals row destination cells/inherited styles; subtotal sources; structural commit | reads=none | writes=one column total fn then rewrites EVERY total-row cell | note=insert.rs:789
ROW chart.insert | params={"range":"Sheet2!A1:C5","type":"column"} | resolves=target_sheet uses sheet index/name/prefix; multiple range areas share rows; default anchor right of first area | holds=source windows/cell values and first-column numfmts for date-label guess; source sheet name; computed single-series title source/dependencies when title omitted; object id allocation; commit | reads=none | writes=new chart series/anchor/title | note=insert.rs:1118
ROW chart.insert | params={} | resolves=selection block else active-cell region; selected sheet active | holds=region empty boundaries; selected source values/styles/date label guess; source sheet name; default title source/dependencies; object identity allocation | reads=part,selection,active | writes=chart | note=insert.rs:1091,1118
ROW chart.recommended | params={} | resolves=chart.insert with type auto; selected block/region | holds=same chart source and geometry/default-title facts; type chosen by dimensions | reads=part,selection,active | writes=new chart | note=insert.rs:110,1118
ROW chart.set | params={"chart":7,"title":"New"} | resolves=numeric chart id on ACTIVE sheet only; omitted chart last chart on active sheet | holds=chosen chart fields/kind for totals validation; membership for last-chart fallback; commit connector geometry if present; replaced values: supplied kind,totals,title,legend,data_labels,gridlines,style,x_title,y_title and anchor fields | reads=part | writes=provided properties only | note=insert.rs:1258,1270; same-field decision
ROW chart.set | params={"chart":7,"at":"Sheet2!C3","width":300} | resolves=chart id active-sheet-only; anchor prefix ignored | holds=chosen chart anchor and kind; anchor row/column windows; connector dependencies; replaced values: supplied kind,totals,title,legend,data_labels,gridlines,style,x_title,y_title and anchor fields | reads=part | writes=anchor fields | note=insert.rs:1270; same-field decision
ROW chart.set | params={"chart":7,"type":"waterfall","totals":[0],"title":"New","legend":"top","dataLabels":true,"gridlines":false,"style":3,"xTitle":"X","yTitle":"Y"} | resolves=integer chart id on active sheet | holds=chosen chart kind,totals,title,legend,data_labels,gridlines,style,x_title,y_title; membership; protection | reads=part | writes=all nine supplied scalar fields | note=insert.rs::chart_set; same-field decision
ROW chart.switchRowColumn | params={"chart":7} | resolves=chart id on active sheet; source or series_block bounding references; source prefix sheet name else chosen sheet; missing named sheet falls back chosen sheet | holds=chart source/by_rows/series; source sheet names/windows; used range clipping even finite ranges; source cell values/styles/date-label guess; commit | reads=part | writes=rebuilds series and toggles by_rows | note=insert.rs:1330
ROW chart.delete | params={} | resolves=last chart on active sheet; explicit numeric chart id active-sheet-only | holds=chosen chart identity and last-member selection; replaced values: entire removed object content | reads=part | writes=removes chart | note=insert.rs:1397; same-field decision
ROW sparkline.insert | params={"range":"Sheet2!B2:M2","location":"Sheet3!N2"} | resolves=location prefix ignored; destination ACTIVE sheet; source is stored text | holds=destination sparkline at N2 and address windows; source values are not read at creation | reads=part | writes=replaces sparkline at destination | note=insert.rs:1405
ROW image.insert | params={"base64":"AA==","at":"B2"} | resolves=floating overCells default; at prefix ignored; active sheet; bytes supply dimensions | holds=active sheet identity and anchor row/column windows; identity allocation | reads=part | writes=new floating image | note=insert.rs:1432; valid bytes required for real cell case
ROW image.insert | params={"placement":"cell","at":"B2","base64":"AA=="} | resolves=bare A1 only; merged destination redirects to merge.start; active sheet | holds=destination merge definition; destination cell/style/picture and inherited style; sheet protection and locked style; row/column windows and fitted row; commit | reads=part | writes=cell picture and stand-in cell value | note=insert.rs:1451; invalid bytes fail before resolving
ROW image.insert | params={"placement":"cell","base64":"AA=="} | resolves=active cell redirected to merge start | holds=same destination/merge/protection/style/windows/fitted-row facts as explicit cell picture | reads=part,active | writes=cell picture | note=insert.rs:1451
ROW shape.insert | params={"at":"B2","kind":"rectangle"} | resolves=active sheet; at prefix ignored; omitted at active cell | holds=sheet identity; anchor address windows; object identity allocation | reads=part | writes=new shape | note=insert.rs:1539
ROW icon.insert | params={"name":"check"} | resolves=active sheet and active cell; explicit at prefix ignored | holds=sheet identity; anchor address windows; object identity allocation | reads=part,active | writes=new icon shape | note=insert.rs:1948
ROW textBox.insert | params={"text":"hello"} | resolves=insert_shape forced textBox; active sheet/active cell | holds=sheet identity; anchor address windows; object identity allocation | reads=part,active | writes=new textbox | note=insert.rs:157
ROW artText.insert | params={"text":"hello","warp":"textWave1"} | resolves=drawing.insert forced textBox; frame absent always uses ACTIVE cell and sheet geometry | holds=active-sheet protection; active-cell row/column geometry to/from frame; default shape geometry; object identity allocation | reads=part,active | writes=new rich textbox and selected_shape | note=insert.rs:172; drawing.rs:206
ROW shape.setText | params={"id":7,"text":"hello"} | resolves=numeric id on ACTIVE sheet shapes only; kind must textBox | holds=shape kind/text; active sheet protection | reads=part | writes=shape.text only; rich text body left intact | note=insert.rs:1598
ROW object.delete | params={"kind":"image","id":7} | resolves=active-sheet list by kind/id; unknown kind chooses shapes | holds=chosen object identity/list membership; replaced values: entire removed object content | reads=part | writes=removes object | note=insert.rs:1628; same-field decision
ROW object.move | params={"kind":"shape","id":7,"at":"Sheet2!C3","width":50} | resolves=id in active-sheet kind list; at prefix ignored | holds=object anchor destination and old anchor fields retained; address windows; connector endpoint/geometry dependencies | reads=part | writes=anchor fields and rerouted connectors | note=insert.rs:1643
ROW object.setAnchorMode | params={"kind":"shape","id":7,"mode":"absolute"} | resolves=id on active-sheet kind list | holds=chosen object anchor mode destination/identity; connector consequences | reads=part | writes=anchor.mode | note=insert.rs:1688
ROW link.insert | params={"cell":"Sheet2!B2","target":"https://example.com","text":"label"} | resolves=target_sheet name/index/prefix; cell body B2; enabled checks ACTIVE cell checkbox independently | holds=active checkbox merge/value/style enabled; target checkbox/value/style; target hyperlink; target input/style/constraints/windows and fitted row; commit | reads=part,active | writes=hyperlink and derived link style; optional text replaces cell | note=insert.rs:1723; extra.rs:962 enabled path
ROW link.insert | params={"cell":"B2","remove":true} | resolves=target_sheet else active; enabled still active checkbox | holds=active checkbox enabled; addressed hyperlink/window | reads=part,active | writes=removes hyperlink only | note=insert.rs:1726
ROW note.add | params={"cell":"Sheet2!B2","text":"note","author":"A"} | resolves=target sheet index/name/prefix; cell body | holds=addressed comment destination and row/column windows | reads=none | writes=replaces comment with non-threaded note | note=insert.rs:1762
ROW comment.add | params={"text":"comment"} | resolves=active sheet/active cell; author preference fallback | holds=addressed comment/window | reads=part,active,prefs.user_name | writes=replaces comment with threaded root and empty replies | note=insert.rs:1762
ROW comment.reply | params={"cell":"B2","text":"reply"} | resolves=target_sheet else active; cell else active; author preference fallback | holds=addressed comment/replies and row/column windows | reads=part,prefs.user_name | writes=appends reply | note=insert.rs:1775
ROW comment.resolve | params={"cell":"Sheet2!B2","resolved":true} | resolves=target sheet index/name/prefix; cell else active | holds=addressed comment resolved destination/window | reads=none | writes=resolved flag; no-op if missing | note=insert.rs:1788
ROW comment.delete | params={} | resolves=active sheet; selection RANGES not active cell despite registry doc cell? | holds=comments intersecting every selected range and absence map/window | reads=part,selection | writes=deletes comments in all selected areas | note=insert.rs:1801
ROW note.show | params={"cell":"B2"} | resolves=target_sheet else active; cell else active | holds=addressed comment visible flag/windows | reads=part | writes=toggles visible | note=insert.rs:1811
ROW symbol.insert | params={"char":"x"} | resolves=ACTIVE cell and sheet; current raw input text concatenated then cell.set | holds=active cell input/formula/rich/style; cell.set destination/constraints/formula sources; fitted row | reads=part,active | writes=parsed concatenation as cell entry | note=insert.rs:1825
ROW conditionalFormat.apply | params={"range":"Sheet2!A1:A3","rule":{"type":"expression","formula":"B1>0"}} | resolves=target_sheet index/name/range prefix; all A1 areas | holds=destination windows; EVERY existing conditional-format priority for increment and exhausted check; rule membership/count; formula is stored unevaluated | reads=none | writes=increments all old priorities and inserts first rule | note=insert.rs:1741,1882
ROW conditionalFormat.apply | params={} | resolves=active sheet/selection | holds=none beyond addressing | reads=part,selection | writes=dialog only | note=insert.rs:1745
ROW conditionalFormat.clearRules | params={"range":"A1:A3"} | resolves=target_sheet else active; all target areas; sheet:true removes all | holds=overlapping conditional-format range lists; all rules for whole-sheet path; address windows; replaced values: all fields and numeric ranges of removed rules/areas | reads=part | writes=removes entire intersecting rule areas rather than subtracting rectangle | note=insert.rs:1894; same-field decision
ROW conditionalFormat.manageRules | params={"delete":0} | resolves=target_sheet else active; indices refer ordered cond_formats list | holds=whole ordered conditional-format list and priorities; deletion index window | reads=part | writes=deletes/reorders rules then renumbers priorities | note=insert.rs:1913; combined delete+move can use stale pre-delete n and panic
ROW formula.autoSum | params={"range":"Sheet2!A1:B5"} | resolves=range prefix ignored; ALWAYS active sheet; only first selected area | holds=bottom-row blank test; source aggregate windows/formula precedents; output bottom row or row below cells/styles/constraints; fitted rows; commit | reads=part | writes=one formula per column; sequential cell.set writes | note=formulas.rs:84; multi-cell path
ROW formula.autoSum | params={} | resolves=current selection; single-cell scans above else left | holds=scanned numeric cells and look-ahead empty/stop cells, including above gaps and left candidate; formula-source windows; destination constraints/style; fitted row | reads=part,selection | writes=guessed aggregate formula or edit-cell request | note=formulas.rs:111
ROW formula.autoSum | params={"range":"B5","enter":false} | resolves=bare range B5 on active sheet; scan above/left as normal | holds=all scan numeric/empty boundary facts | reads=part | writes=UI edit request only | note=formulas.rs:145
ROW formula.insertFunction | params={"name":"SUM"} | resolves=does not write cell; UI request only; enabled checks active checkbox | holds=active-cell checkbox value/style/merge for enabled | reads=part,active | writes=edit-cell UI request; omitted name dialog | note=formulas.rs:154; extra.rs:962
ROW name.define | params={"name":"Result","refersTo":"=Sheet2!$A$1","scope":"Sheet2"} | resolves=scope sheet NAME only; name case-insensitive same scope; refersTo stored parsed expression, not evaluated | holds=scope name/identity; chosen name definition/existence in same scope; no referenced cell content | reads=none | writes=replaces or creates DefinedName | note=formulas.rs:243
ROW name.define | params={"name":"Result"} | resolves=Workbook scope default; refersTo active sheet NAME and first current selection | holds=active sheet name; selection address windows; chosen workbook name definition/existence | reads=part,selection | writes=absolute selected range definition | note=formulas.rs:260
ROW name.define | params={} | resolves=no name | holds=none | reads=none | writes=dialog only | note=formulas.rs:244
ROW name.delete | params={"name":"Result"} | resolves=case-insensitive name EVERY scope when scope omitted; Workbook only root scope else exact sheet name resolution | holds=matching scoped definitions and membership/existence; sheet identity if named scope | reads=none | writes=removes matching names | note=formulas.rs:328
ROW name.addFromSelection | params={"range":"Sheet2!A1:C5","top":true} | resolves=range prefix ignored; ACTIVE sheet labels/name; edge top/left/bottom/right source cells; Workbook scope only | holds=edge label values and computed precedents; active sheet NAME; range address windows; existing workbook name definitions/uniqueness for replace/skip | reads=part | writes=generated name definitions; no cell changes | note=formulas.rs:364
ROW name.addFromSelection | params={} | resolves=selection current on active sheet; top edge default | holds=selection edge labels and their formula precedents; active sheet name; window; existing names | reads=part,selection | writes=generated workbook names | note=formulas.rs:364
ROW formula.removeArrows | params={} | resolves=no object | holds=none | reads=none | writes=none | note=formulas.rs:54; registered noundo no-op
ROW formula.showFormulas | params={} | resolves=active sheet only; on omitted toggles | holds=sheet.show_formulas | reads=part | writes=show_formulas flag | note=formulas.rs:445
ROW formula.showFormulas | params={"on":true} | resolves=active sheet only | holds=sheet.show_formulas destination | reads=part | writes=explicit show_formulas | note=formulas.rs:445
ROW formula.calculateNow | params={} | resolves=every workbook formula | holds=all formula inputs/names/tables/settings/date system and computed-source structures; no input edit decision | reads=none | writes=recalculates all cached formula values/spills/circular state | note=formulas.rs:498
ROW formula.calculateSheet | params={} | resolves=calc_now recalculates ALL sheets despite command label | holds=same workbook-wide calculation facts as calculateNow | reads=none | writes=recalculates all sheets | note=formulas.rs:498; no active-sheet selection
ROW formula.calculationOptions | params={"mode":"automatic","iterative":true,"maxIterations":200,"maxChange":0.01} | resolves=no object params | holds=selected calc settings destinations; mode for returned answer; automatic mode recalculates all as consequence | reads=none | writes=provided calc settings; automatic triggers structural recalculation/header sync | note=formulas.rs:507
ROW pivot.insert | params={"source":"Sheet2!A1:C5","destination":"Sheet1!H3","rows":["Region"],"values":["Amount"]} | resolves=source table name first else A1 sheet name else defined name up to depth four; destination prefix name else active; aliases range/location accepted | holds=source values/headers/display/first-data numfmt/date system; source name/table definitions/windows; destination cells/spills/tables/pivots/protection/column widths-hidden; global pivot names and id allocation; pivot-refresh | reads=part | writes=new pivot/report and active-sheet/selection | note=pivot.rs:523; pivot.rs engine:1093
ROW pivot.insert | params={} | resolves=active-cell table else selection trimmed used-range/current region; destination new sheet BEFORE active at A3 | holds=table/region boundaries/used range; all source facts; protected_structure; tab order/sheet names; names scoped indices; pivot name uniqueness; pivot-refresh | reads=part,selection,active | writes=inserts sheet; increments scoped name indices; pivot/report; active selection | note=pivot.rs:465,523
ROW pivot.recommended | params={"source":"Sales"} | resolves=table source else A1/defined name; no apply gives suggestions only | holds=resolved source headers/values/numfmt/date facts and labels used in suggestions | reads=part | writes=none | note=pivot.rs:1253
ROW pivot.recommended | params={"source":"Sales","apply":0,"destination":"Sheet2!H3"} | resolves=suggestion index based on live source distinct/numeric/date classifications; destination passed to insert_pivot | holds=all suggestion source facts; pivot.insert creation/destination facts | reads=part | writes=chosen pivot/report | note=pivot.rs:1253
ROW table.summarizePivot | params={"table":"Sales","destination":"Sheet2!H3"} | resolves=table name workbook-wide else active-cell table; forced source table name | holds=chosen table/source facts and pivot.insert destination/creation facts | reads=part | writes=new pivot/report | note=pivot.rs:632
ROW pivot.addField | params={"pivot":"Report","field":"Amount","area":"values"} | resolves=pivot name case-insensitive active sheet first then other sheets; numeric id similarly; omitted pivot contains active cell else last active-sheet pivot | holds=pivot-refresh; source numeric classification selects default Sum vs Count; existing captions/areas determine uniqueness/removal | reads=part | writes=field configuration then regenerates report | note=pivot.rs:189,734; func omitted
ROW pivot.addField | params={"field":"Region","area":"rows"} | resolves=active-cell report else last pivot on active sheet; source field by case-insensitive header | holds=pivot-refresh and field area settings retained | reads=part,active | writes=field area then report | note=pivot.rs:734
ROW pivot.removeField | params={"pivot":7,"field":"Amount"} | resolves=pivot id active-first across workbook; field matches value caption before source name; omitted area tries all | holds=pivot-refresh and current area/caption layout | reads=part | writes=removes matched field(s), regenerates report | note=pivot.rs:750
ROW pivot.moveField | params={"pivot":7,"field":"Region","from":"rows","to":"columns"} | resolves=pivot id active-first; field/caption in specified area; position zero-based | holds=pivot-refresh; current source field numeric default; preserved axis settings/hidden selection | reads=part | writes=moves field preserving axis settings then report | note=pivot.rs:776
ROW pivot.moveField | params={"pivot":7,"field":"Region","from":"rows","to":"rows","position":0} | resolves=field in same area by case-insensitive source or value caption | holds=pivot-refresh and ordered field list | reads=part | writes=reorders same-area field then report | note=pivot.rs:783
ROW pivot.valueSettings | params={"pivot":7,"index":0,"func":"average"} | resolves=index zero-based takes precedence over field; omitted index/field only if one value; field caption before source | holds=pivot-refresh; value field old func/name/format/showAs and peer names; source headers validate caption | reads=part | writes=patches value definition, automatic caption then report | note=pivot.rs:840
ROW pivot.filter | params={"pivot":7,"field":"Region","selected":["East"]} | resolves=source field header; active-first pivot identity resolution | holds=pivot-refresh and axis/filter memberships | reads=part | writes=selected labels/filter definition then report | note=pivot.rs:912
ROW pivot.sort | params={"pivot":7,"field":"Region","order":"desc"} | resolves=field in rows/columns by case-insensitive source name | holds=pivot-refresh and axis field configuration | reads=part | writes=sort flag then report | note=pivot.rs:968
ROW pivot.group | params={"pivot":7,"field":"Date","by":"months"} | resolves=axis field source name; numeric source classification checked for grouping | holds=pivot-refresh; source numeric/date data and date system; old collapsed items | reads=part | writes=grouping and clears collapsed items; report | note=pivot.rs:994
ROW pivot.collapse | params={"pivot":7,"field":"Region"} | resolves=axis field source name; missing items expands to distinct formatted source labels | holds=pivot-refresh; complete grouped/formatted distinct labels when omitted items; current collapsed list | reads=part | writes=collapse list then report | note=pivot.rs:1013
ROW pivot.collapse | params={"pivot":7,"field":"Region","items":["East"],"collapse":false} | resolves=explicit labels matched case-insensitive in collapsed items | holds=pivot-refresh and collapsed list | reads=part | writes=removes specified collapsed items then report | note=pivot.rs:1013
ROW pivot.layout | params={"pivot":7,"layout":"tabular"} | resolves=pivot active-first name/id; default active-cell report/last | holds=pivot-refresh and old unprovided layout flags | reads=part | writes=layout plus tabular sets subtotals_top false; report | note=pivot.rs:1047
ROW pivot.grandTotals | params={"pivot":7,"mode":"rows"} | resolves=mode to grandTotalsRows/Cols forwarded layout | holds=pivot-refresh and other layout flags | reads=part | writes=grand total flags and report | note=pivot.rs:1102
ROW pivot.subtotals | params={"pivot":7,"mode":"bottom"} | resolves=mode forwarded layout subtotals | holds=pivot-refresh and unprovided layout flags | reads=part | writes=subtotals flags/report | note=pivot.rs:135
ROW pivot.reportLayout | params={"pivot":7,"layout":"outline"} | resolves=same layout handler | holds=pivot-refresh and unprovided layout flags | reads=part | writes=layout/report | note=pivot.rs:141
ROW pivot.style | params={"pivot":7,"style":"PivotStyleMedium9"} | resolves=style is builtin palette text consumed by pivot look, no custom style lookup | holds=pivot-refresh and old layout flags | reads=part | writes=style/report | note=pivot.rs:150
ROW pivot.refresh | params={"pivot":7} | resolves=active-first name/id or active-cell report/last | holds=pivot-refresh | reads=part | writes=regenerates report/widths/last_range | note=pivot.rs:1121
ROW data.refreshAll | params={} | resolves=all pivots on all sheets in live list order | holds=every pivot-refresh and pivot membership; earlier output may feed later source | reads=none | writes=each successful refreshed report; errors collected without aborting other pivots | note=pivot.rs:1125
ROW pivot.changeSource | params={"pivot":7,"source":"Sheet2!A1:C5"} | resolves=source bare A1 qualified ACTIVE sheet before chosen pivot; old source read by update too | holds=old and new source definitions/windows/values/headers/formats; pivot-refresh; field membership on new headers | reads=part | writes=source and prunes missing fields; report | note=pivot.rs:1145
ROW pivot.delete | params={"pivot":7} | resolves=active-first name/id; omitted active-cell report/last | holds=chosen pivot/last_range and every output cell/window; protection through commit | reads=part | writes=clears report via take_cells and removes pivot | note=pivot.rs:1171; does not read source
ROW data.goalSeek | params={"set":"Sheet2!B5","to":10,"changing":"Sheet1!B2"} | resolves=each cell sheet NAME independently else active; bare A1 only not name/table | holds=set formula and all trial precedents/names/table extents/settings/date system/visibility; changing old constant/formula/style; max_iterations/max_change; output fitted row/protection/windows; commit | reads=none | writes=found constant to changing even nonconverged; calculated result returned | note=whatif.rs:307; Trial::run reevaluates dependency graph
ROW data.scenarioAdd | params={"name":"Base","changing":"Sheet2!B2:B3"} | resolves=comma A1 ranges one sheet; whole axes trimmed used range; <=32 cells; stores selected sheet NAME | holds=changing values when values omitted; sheet name/windows/used range if whole axes; hidden _sc_scenarios workbook name complete contents and uniqueness | reads=none | writes=scenario stored in hidden workbook name | note=whatif.rs:423
ROW data.scenarioAdd | params={"name":"Base","changing":"B2:B3","values":[1,2],"replace":true} | resolves=bare addresses active sheet; explicit values stored JSON without parsing | holds=sheet name and changing address windows; complete scenario metadata/name definition; old changing values are unrelated | reads=part | writes=replaces/adds scenario in hidden name | note=whatif.rs:423
ROW data.scenarioShow | params={"name":"Base"} | resolves=scenario name active sheet NAME preferred else first workbook match; destination sheet by stored scenario.sheet; stored changing A1 coordinates parsed with active fallback but returned sheet ignored | holds=chosen scenario data/order; destination sheet name/identity/windows; old destination styles/checkbox/numfmt; date system; scenario values interpreted as entries and formula dependencies; fitted rows; commit; replaced values: each destination input,text,array,picture | reads=part | writes=scenario values into stored scenario sheet | note=whatif.rs:482; same-field decision
ROW data.scenarioDelete | params={"name":"Base"} | resolves=active sheet NAME preferred else first same name workbook-wide | holds=complete _sc_scenarios metadata/order and active sheet name for choice | reads=part | writes=removes scenario and rewrites hidden name | note=whatif.rs:507
ROW data.scenarioSummary | params={"resultCells":"B10:B12,D4"} | resolves=scenarios on active sheet name; resultCells same sheet; scenario changing cells union; report appended new unique sheet name | holds=all active-sheet scenarios; current changing/results values/styles/numfmt; all trial formula precedents/names/settings/date/visibility; sheet-name uniqueness; default style; structural commit | reads=part | writes=new summary sheet and active selection | note=whatif.rs:518; protected_structure not checked
ROW data.dataTable | params={"range":"Sheet2!D2:E10","columnInput":"Sheet1!B2"} | resolves=range and input cell sheet names independently else active; whole range clips used; inputs outside table | holds=left edge trial values; top edge formulas and all trial precedents/names/settings/date/visibility; input old cell/style; output body styles/protection/windows; commit; replaced values: output body input,text,array,picture | reads=none | writes=static trial results to body, preserving style | note=whatif.rs:629; column-input path; same-field decision
ROW data.dataTable | params={"range":"D2:M3","rowInput":"B2"} | resolves=bare cells active sheet | holds=top edge trial values; left edge formulas/all trial precedents; input style; body cells/styles/windows; calc/date/name/table facts; commit; replaced values: output body input,text,array,picture | reads=part | writes=static row-input trial results | note=whatif.rs:665; same-field decision
ROW data.dataTable | params={"range":"D2:M10","rowInput":"B1","columnInput":"B2"} | resolves=bare addresses active sheet; row and column input independent sheet prefixes allowed | holds=both edge values; corner formula and all trial precedents; input styles; output body cells/styles/windows; calc/date/name/table facts; commit; replaced values: output body input,text,array,picture | reads=part | writes=static two-input trial results | note=whatif.rs:680; same-field decision
ROW data.advancedFilter | params={"range":"Sheet1!A1:C5","criteria":"Sheet2!H1:I3"} | resolves=list/criteria by sheet NAME and A1; default list ACTIVE-cell current_region | holds=list/criteria source values/formulas/display styles; computed criteria translated for every row with their actual csi context and precedents; optional unique entire display row; list row windows/hidden destinations | reads=none | writes=hides/shows list rows; structural recalculation | note=whatif.rs:788
ROW data.advancedFilter | params={"criteria":"H1:I3","copyTo":"Sheet2!K1","unique":true} | resolves=list active-cell region; copyTo sheet name/A1; extract area full original list height despite fewer matches | holds=current-region boundary absence; list/criteria values/styles/formula precedents; full destination extract old cells/styles/windows/protection and output fitted rows; commit | reads=part,active | writes=clears entire extract region and copies kept values/styles with formulas made static | note=whatif.rs:865
ROW data.consolidate | params={"sources":["Sheet2!A1:C5","Sheet3!A1:C5"],"destination":"Sheet1!H1"} | resolves=each source/destination sheet NAME A1; whole axes trimmed used range; source may semicolon text | holds=all source values/windows; used range for whole axes; output cells/inherited styles/protection/windows/fitted rows; commit | reads=none | writes=aggregated constants preserving destination style | note=whatif.rs:988; positional path
ROW data.consolidate | params={"sources":["A1:C5"],"topRow":true,"leftColumn":true} | resolves=source active sheet; destination ACTIVE cell; labels matched case-insensitive first seen order | holds=source data and label display styles/numfmts/date system; all label-dependent output size/address cells/styles/protection; fitted rows; commit | reads=part,active | writes=labelled aggregate constants | note=whatif.rs:988; labels/default-destination path
"##;

// Pinned from the declaration after review of handler defaults. These editor-only
// projections are not blind readings; READINGS above keeps every original branch row.
// Each set is the union of actual code paths, including omitted target parameters.
pub const REVIEWED_EDITOR: &[(&str, &[&str], &str)] = &[
    (
        "table.bandedRows",
        &["part", "selection"],
        "insert.rs:655 table_flag -> find_table:640 defaults to the active-sheet table containing selection.active",
    ),
    (
        "table.bandedColumns",
        &["part", "selection"],
        "insert.rs:655 table_flag -> find_table:640 defaults to the active-sheet table containing selection.active",
    ),
    (
        "table.headerRow",
        &["part", "selection"],
        "insert.rs:758 table_header -> find_table:640; omitted table uses active cell, and hiding a header also reads cx.sel.active",
    ),
    (
        "table.firstColumn",
        &["part", "selection"],
        "insert.rs:655 table_flag -> find_table:640 defaults to the active-sheet table containing selection.active",
    ),
    (
        "table.lastColumn",
        &["part", "selection"],
        "insert.rs:655 table_flag -> find_table:640 defaults to the active-sheet table containing selection.active",
    ),
    ("table.style", &["part", "selection"], "insert.rs:806 table_style -> find_table:640 defaults to active sheet and selection.active"),
    ("table.rename", &["part", "selection"], "insert.rs:834 table_rename -> find_table:640 defaults to active sheet and selection.active"),
    ("table.convertToRange", &["part", "selection"], "insert.rs:880 table_convert -> find_table:640 defaults to active sheet and selection.active"),
    ("table.totalFunction", &["part", "selection"], "insert.rs:728 table_total_fn -> find_table:640 defaults to active sheet and selection.active"),
    ("shape.insert", &["part", "selection"], "insert.rs:1400 insert_shape defaults at to d.selection.active and inserts on d.wb.active_sheet"),
    ("note.add", &["part", "selection"], "insert.rs:1586 comment -> target_sheet defaults to active sheet; omitted cell uses selection.active"),
    (
        "comment.reply",
        &["part", "selection"],
        "insert.rs:1599 reply_comment -> target_sheet defaults to active sheet; omitted cell uses selection.active",
    ),
    (
        "comment.resolve",
        &["part", "selection"],
        "insert.rs:1611 resolve_comment -> target_sheet defaults to active sheet; omitted cell uses selection.active",
    ),
    ("note.show", &["part", "selection"], "insert.rs:1632 show_note -> target_sheet defaults to active sheet; omitted cell uses selection.active"),
    (
        "conditionalFormat.clearRules",
        &["part", "selection"],
        "insert.rs:1884 clear_cf always calls target_ranges, whose omitted range reads selection.ranges, and target_sheet defaults active",
    ),
    (
        "data.ungroup",
        &["part", "selection"],
        "data.rs:1257 group -> outline_lines:1244 uses selection.current when rows/cols are omitted; cx.sheet_index selects active sheet",
    ),
    (
        "data.hideDetail",
        &["part", "selection"],
        "data.rs:1274 detail -> outline_lines:1244 uses selection.current when rows/cols are omitted; cx.sheet_index selects active sheet",
    ),
    (
        "data.subtotal",
        &["part", "selection"],
        "data.rs:1303 subtotal -> data_range:121 defaults to selection block or active-cell table/current region on active sheet",
    ),
    (
        "data.fromTableRange",
        &["part", "selection"],
        "extra.rs:852 from_table_range -> target_range defaults selection; active-sheet source then from_csv -> sheet.insert remembers selection",
    ),
    (
        "pivot.insert",
        &["part", "selection"],
        "pivot.rs:523 insert_pivot -> source_param:484/default_source:465 uses active-sheet selection; destination defaults new sheet before active; no editor mode read",
    ),
    (
        "pivot.recommended",
        &["part", "selection"],
        "pivot.rs:1253 recommended -> source_param:484/default_source:465; optional apply calls insert_pivot:523; no editor mode read",
    ),
    (
        "table.summarizePivot",
        &["part", "selection"],
        "pivot.rs:632 summarize_table defaults table to active-sheet table at selection.active then calls insert_pivot:523; no editor mode read",
    ),
    (
        "pivot.removeField",
        &["part", "selection"],
        "pivot.rs:750 remove_field -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.moveField",
        &["part", "selection"],
        "pivot.rs:776 move_field -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.valueSettings",
        &["part", "selection"],
        "pivot.rs:840 value_settings -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.filter",
        &["part", "selection"],
        "pivot.rs:912 filter -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.sort",
        &["part", "selection"],
        "pivot.rs:968 sort -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.group",
        &["part", "selection"],
        "pivot.rs:994 group -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.collapse",
        &["part", "selection"],
        "pivot.rs:1013 collapse -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.layout",
        &["part", "selection"],
        "pivot.rs:1047 layout -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.grandTotals",
        &["part", "selection"],
        "pivot.rs:1102 grand_totals delegates layout:1047 -> find_pivot:189; omitted pivot uses active-sheet selection.active",
    ),
    (
        "pivot.subtotals",
        &["part", "selection"],
        "pivot.rs:123 registry closure delegates layout:1047 -> find_pivot:189; omitted pivot uses active-sheet selection.active",
    ),
    (
        "pivot.reportLayout",
        &["part", "selection"],
        "pivot.rs:127 registry delegates layout:1047 -> find_pivot:189; omitted pivot uses active-sheet selection.active",
    ),
    (
        "pivot.style",
        &["part", "selection"],
        "pivot.rs:136 registry delegates layout:1047 -> find_pivot:189; omitted pivot uses active-sheet selection.active",
    ),
    (
        "pivot.refresh",
        &["part", "selection"],
        "pivot.rs:1121 refresh -> update:439 -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "pivot.changeSource",
        &["part", "selection"],
        "pivot.rs:1145 change_source -> find_pivot:189 and update:439; omitted pivot uses active-sheet selection.active; bare source uses active sheet",
    ),
    (
        "pivot.delete",
        &["part", "selection"],
        "pivot.rs:1171 delete -> find_pivot:189; omitted pivot uses active-sheet selection.active before last-pivot fallback",
    ),
    (
        "data.goalSeek",
        &["part"],
        "whatif.rs:307 goal_seek -> cell_arg:100 resolves unqualified set/changing cells against wb.active_sheet; there is no selection fallback",
    ),
];

#[test]
fn blind_data_insert_rows_cover_owned_registry() {
    use std::collections::BTreeSet;

    let rows: Vec<_> = READINGS.lines().filter(|line| line.starts_with("ROW ")).collect();
    let mut commands = BTreeSet::new();
    for row in &rows {
        let fields: Vec<_> = row.split(" | ").collect();
        assert_eq!(fields.len(), 7, "{row}");
        assert!(fields[0].starts_with("ROW "), "{row}");
        for (field, key) in fields.iter().skip(1).zip(["params=", "resolves=", "holds=", "reads=", "writes=", "note="]) {
            assert!(field.starts_with(key) && field.len() > key.len(), "{row}");
        }
        commands.insert(fields[0].trim_start_matches("ROW "));
    }
    assert_eq!(commands.len(), 97, "owned registry write count");
    assert!(rows.len() > commands.len(), "branch readings must remain explicit");
}

// Comparison added after blind commit c1c1ce8. The sentinel is in no source, destination,
// current region, formula precedent or structural edit on the fixture. Holding it is strictly
// more than these handlers read. This projection does not claim to compare every field named
// in the prose rows; additional exact source/parameter checks below cover discovered deficits.
#[test]
fn unrelated_data_cell_holds_only_decrease() {
    use std::collections::BTreeSet;

    use kition_kit::KitDocument;
    use kition_sheet_kit::SheetDocument;
    use serde_json::{Value, json};

    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute(
        "range.setValues",
        json!({"range":"A1","values":[["Region","Amount","Date"],["East",3,45000],["West",2,45001],["East",1,45002],["West",4,45003]]}),
    )
    .unwrap();
    doc.execute("table.insert", json!({"range":"A1:C5","name":"Sales","header":true})).unwrap();
    let chart = doc.execute("chart.insert", json!({"range":"A1:C5"})).unwrap()["chart"].clone();
    let shape = doc.execute("textBox.insert", json!({"at":"H1","text":"old"})).unwrap()["shape"].clone();
    let pivot = doc
        .execute("pivot.insert", json!({"source":"Sales","destination":"H8","name":"Report","rows":["Region"],"values":["Amount"]}))
        .unwrap()["id"]
        .clone();
    doc.execute("sheet.insert", json!({"name":"Sheet2","before":9})).unwrap();
    doc.execute("sheet.insert", json!({"name":"Sheet3","before":9})).unwrap();
    doc.execute("cell.set", json!({"sheet":"Sheet3","cell":"Z20","input":"unrelated"})).unwrap();
    doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
    doc.execute("select.set", json!({"range":"B3"})).unwrap();
    doc.version().expect("materialize observed identities");

    let catalogue = doc.commands();
    let mut more = BTreeSet::new();
    let mut compared = BTreeSet::new();
    for line in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let fields: Vec<_> = line.split(" | ").collect();
        let id = fields[0].trim_start_matches("ROW ");
        let command = catalogue.iter().find(|command| command.id == id).unwrap();
        if !command.mutates {
            continue;
        }
        let mut params: Value = serde_json::from_str(fields[1].trim_start_matches("params=")).unwrap();
        for (key, replacement) in [("chart", &chart), ("id", &shape), ("pivot", &pivot)] {
            if params.get(key).is_some_and(Value::is_number) {
                params[key] = replacement.clone();
            }
        }
        let under = doc.under(id, &params).unwrap_or_else(|error| panic!("{id} {params}: {error}"));
        compared.insert(id);
        if under.is_some_and(|under| {
            under
                .premises
                .iter()
                .any(|premise| premise.id == "cell:3!Z20" && premise.fields.as_ref().is_none_or(|fields| fields.iter().any(|field| field == "input")))
        }) {
            more.insert(id);
        }
    }
    // Initial comparison: 58/93 commands held this unrelated cell. Entries can
    // only be removed after the corresponding hold is narrowed.
    let baseline: BTreeSet<&str> = [
        "data.advancedFilter",
        "data.consolidate",
        "data.dataTable",
        "data.smartFill",
        "data.goalSeek",
        "data.refreshAll",
        "data.removeDuplicates",
        "data.scenarioAdd",
        "data.scenarioDelete",
        "data.scenarioShow",
        "data.scenarioSummary",
        "data.sort",
        "data.sortAscending",
        "data.sortDescending",
        "data.subtotal",
        "data.textToColumns",
        "formula.autoSum",
        "name.addFromSelection",
        "name.define",
        "conditionalFormat.apply",
        "table.formatAs",
        "chart.insert",
        "link.insert",
        "image.insert",
        "pivot.insert",
        "chart.recommended",
        "pivot.recommended",
        "sparkline.insert",
        "table.insert",
        "artText.insert",
        "pivot.addField",
        "pivot.changeSource",
        "pivot.collapse",
        "pivot.delete",
        "pivot.filter",
        "pivot.grandTotals",
        "pivot.group",
        "pivot.layout",
        "pivot.moveField",
        "pivot.refresh",
        "pivot.removeField",
        "pivot.reportLayout",
        "pivot.sort",
        "pivot.style",
        "pivot.subtotals",
        "pivot.valueSettings",
        "note.add",
        "comment.resolve",
        "table.summarizePivot",
    ]
    .into_iter()
    .collect();
    eprintln!("data/insert/formulas/pivot/whatif projection: {}/{} held-more commands: {more:?}", more.len(), compared.len());
    assert!(more.is_subset(&baseline), "new unrelated-cell hold(s): {:?}", more.difference(&baseline).collect::<Vec<_>>());
    assert_eq!(more, baseline, "coverage improved: decrease this baseline, never add a command");
}

#[test]
fn independent_external_sort_keys_are_held() {
    use kition_kit::KitDocument;
    use kition_sheet_kit::SheetDocument;
    use serde_json::json;

    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B20","input":"3"})).unwrap();
    let under =
        doc.under("data.sort", &json!({"range":"A2:C4","header":false,"orientation":"columns","keys":[{"row":20,"order":"asc"}]})).unwrap().unwrap();
    assert!(under.premises.iter().any(|premise| premise.id == "cell:1!B20"), "external sort source absent: {under:?}");
    assert!(
        under.within.iter().any(|window| window.field == "rows" && window.from == 19 && window.to == 20),
        "external key address window absent: {under:?}"
    );
}

#[test]
fn insert_function_declares_its_checkbox_enabled_reads() {
    use kition_kit::{EditorState, KitDocument};
    use kition_sheet_kit::SheetDocument;

    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let command = doc.commands().into_iter().find(|command| command.id == "formula.insertFunction").unwrap();
    assert_eq!(command.reads, [EditorState::Selection, EditorState::Part], "enabled reads the active-cell checkbox");
}

// Pinned from the declaration after review of the frozen readings and all handler parameter
// branches. This is a reviewed projection, not additional blind evidence. A role is assigned
// because the handler resolves its value, never because its spelling resembles an address.
// Public schemas sometimes omit engine-helper parameters; only offered paths participate.
fn reviewed_reference_roles(id: &str) -> (Option<&'static str>, &'static [&'static str], &'static [&'static str]) {
    match id {
        "data.sortAscending" | "data.sortDescending" => (Some("range"), &["range", "cell", "column"], &[]),
        // Pinned after SDK review: place names are literal top-level keys in 34dff06;
        // the `keys` value contains the independently read column/row selectors.
        "data.sort" => (Some("range"), &["range", "keys"], &[]),
        "data.filter" => (Some("range"), &["range"], &[]),
        // Pinned after Agent routing review (lib.rs execute -> command_target_sheet): even
        // engine-ignored bodies can supply the active part through a sheet/range/cell prefix.
        // filterBy's no-scope branch calls toggle_filter with empty params.
        "data.filterBy" => (Some("cell"), &["sheet", "range", "cell", "table", "column"], &[]),
        "data.clearFilter" => (Some("cell"), &["cell", "table"], &[]),
        "data.removeDuplicates" => (Some("range"), &["range", "columns"], &[]),
        "data.textToColumns" => (Some("range"), &["sheet", "range", "cell", "destination"], &[]),
        "data.validation" => (Some("range"), &["sheet", "range", "cell"], &[]),
        "data.group" | "data.ungroup" | "data.hideDetail" | "data.showDetail" => (None, &["rows", "cols"], &[]),
        "data.subtotal" => (Some("range"), &["range", "groupBy", "columns"], &[]),
        "data.smartFill" => (Some("range"), &["sheet", "range", "cell"], &[]),
        "data.reapply" | "data.refreshAll" => (None, &[], &[]),
        "table.insert" => (Some("range"), &["sheet", "range", "cell"], &[]),
        "table.formatAs" => (Some("range"), &["sheet", "range", "cell", "style"], &[]),
        // New names are literal creation labels. replace resolves an existing custom style.
        "table.newStyle" => (None, &["replace"], &[]),
        "table.deleteStyle" | "table.setDefaultStyle" => (None, &["name"], &[]),
        "table.totalRow"
        | "table.bandedRows"
        | "table.bandedColumns"
        | "table.filterButton"
        | "table.firstColumn"
        | "table.lastColumn"
        | "table.headerRow"
        | "table.style"
        | "table.rename"
        | "table.convertToRange" => (None, &["table"], &[]),
        "table.totalFunction" => (None, &["table", "column"], &[]),
        "chart.insert" | "chart.recommended" => (None, &["sheet", "range", "cell", "at"], &[]),
        "chart.set" => (Some("chart"), &["at"], &["chart"]),
        "chart.switchRowColumn" | "chart.delete" => (Some("chart"), &[], &["chart"]),
        // range is stored source text. It remains a natural on target for the selected data;
        // location separately names the sparkline being replaced.
        "sparkline.insert" => (Some("range"), &["range", "location"], &[]),
        "image.insert" | "shape.insert" | "icon.insert" | "textBox.insert" => (None, &["at"], &[]),
        "artText.insert" | "symbol.insert" => (None, &[], &[]),
        "shape.setText" | "object.delete" | "object.setAnchorMode" => (Some("id"), &[], &["id"]),
        "object.move" => (Some("id"), &["at"], &["id"]),
        // Internal link text is stored, not dereferenced as current source-cell contents.
        "link.insert" => (Some("cell"), &["sheet", "cell", "target"], &[]),
        "note.add" | "comment.add" | "comment.reply" | "comment.resolve" | "note.show" => (Some("cell"), &["sheet", "range", "cell"], &[]),
        // Pinned after helper review: target_ranges resolves cell when supplied; the blind
        // empty-params branch above recorded only the fallback to selection ranges.
        "comment.delete" => (Some("cell"), &["cell"], &[]),
        "conditionalFormat.apply" => (Some("range"), &["sheet", "range", "cell"], &[]),
        // Here sheet is the whole-sheet boolean, not a Sheet name or tab index.
        "conditionalFormat.clearRules" => (Some("range"), &["range", "cell"], &[]),
        // cell/range prefixes can pick the sheet; their bodies do not select the indexed rule.
        "conditionalFormat.manageRules" => (None, &["sheet", "range", "cell", "delete", "moveUp", "moveDown"], &[]),
        "formula.autoSum" | "name.addFromSelection" => (Some("range"), &["range"], &[]),
        "formula.insertFunction" | "formula.showFormulas" | "formula.calculationOptions" => (None, &[], &[]),
        // refersTo is parsed and stored; it needs address/name facts, not source values.
        "name.define" => (None, &["name", "scope", "refersTo"], &[]),
        "name.delete" => (None, &["name", "scope"], &[]),
        "pivot.insert" | "pivot.recommended" => (None, &["source", "range", "destination", "location"], &[]),
        // summarize_table replaces source from the chosen table; range can still supply the
        // Agent part. rows/columns are source-field lists, not worksheet axes.
        "table.summarizePivot" => (None, &["table", "range", "destination", "location"], &[]),
        "pivot.addField"
        | "pivot.removeField"
        | "pivot.moveField"
        | "pivot.valueSettings"
        | "pivot.filter"
        | "pivot.sort"
        | "pivot.group"
        | "pivot.collapse"
        | "pivot.layout"
        | "pivot.grandTotals"
        | "pivot.subtotals"
        | "pivot.reportLayout"
        | "pivot.style"
        | "pivot.refresh"
        | "pivot.delete" => (None, &["pivot"], &[]),
        "pivot.changeSource" => (None, &["pivot", "source"], &[]),
        "data.goalSeek" => (Some("changing"), &["set", "changing"], &[]),
        "data.scenarioAdd" => (None, &["changing", "name"], &[]),
        "data.scenarioShow" | "data.scenarioDelete" => (None, &["name"], &[]),
        "data.scenarioSummary" => (None, &["resultCells"], &[]),
        "data.dataTable" => (Some("range"), &["range", "rowInput", "columnInput"], &[]),
        // on selects the source list, including in copy mode.
        "data.advancedFilter" => (Some("range"), &["range", "criteria", "copyTo"], &[]),
        "data.consolidate" => (None, &["sources", "destination"], &[]),
        _ => panic!("unreviewed data/insert reference mapping: {id}"),
    }
}

#[test]
fn all_data_reference_roles_match_reviewed_handler_resolution_with_decreasing_floors() {
    use kition_kit::KitDocument;
    use std::collections::BTreeSet;

    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    let recorded: BTreeSet<_> = READINGS.lines().filter_map(|row| row.strip_prefix("ROW ")).filter_map(|row| row.split(" | ").next()).collect();
    let commands = doc.commands();
    let mut compared = BTreeSet::new();
    let mut less = BTreeSet::new();
    let mut more = BTreeSet::new();
    for command in commands.iter().filter(|command| command.mutates && recorded.contains(command.id.as_str())) {
        let id = command.id.as_str();
        compared.insert(id);
        let schema = command.schema.as_ref().expect("parameter schema");
        let offered = |path: &&str| schema.get(path.split('.').next().unwrap_or_default().trim_end_matches("[]")).is_some();
        let (target, places, objects) = reviewed_reference_roles(id);
        let target = target.filter(|key| schema.get(*key).is_some() && kition_kit::schema::check_targets(schema, key).is_ok());
        let wanted_places: BTreeSet<_> = places.iter().copied().filter(offered).collect();
        let actual_places: BTreeSet<_> = command.places.iter().copied().collect();
        let wanted_objects: BTreeSet<_> = objects.iter().copied().filter(offered).filter(|key| Some(*key) != target).collect();
        let actual_objects: BTreeSet<_> = command.objects.iter().copied().collect();
        let missing_places: Vec<_> = wanted_places.difference(&actual_places).copied().collect();
        let extra_places: Vec<_> = actual_places.difference(&wanted_places).copied().collect();
        let missing_objects: Vec<_> = wanted_objects.difference(&actual_objects).copied().collect();
        let extra_objects: Vec<_> = actual_objects.difference(&wanted_objects).copied().collect();
        if !missing_places.is_empty() || !missing_objects.is_empty() || target.is_some() && command.targets != target {
            less.insert(id);
        }
        if !extra_places.is_empty() || !extra_objects.is_empty() || command.targets.is_some() && command.targets != target {
            more.insert(id);
        }
        if !missing_places.is_empty()
            || !extra_places.is_empty()
            || !missing_objects.is_empty()
            || !extra_objects.is_empty()
            || command.targets != target
        {
            eprintln!(
                "DATA_REFERENCE {id}: places LESS={missing_places:?} MORE={extra_places:?}; objects LESS={missing_objects:?} MORE={extra_objects:?}; target expected={target:?} actual={:?}",
                command.targets
            );
        }
    }
    assert_eq!(compared.len(), 93, "every registered data/insert/formula/pivot/trial write has an explicit role mapping");
    // Reviewed comparison: 37/93 commands differed, 35 LESS and 8 MORE (6 both).
    // The raw comparison's extra MORE commands omitted Agent sheet-prefix routing and
    // target_ranges' cell-body branch for comment deletion. Blind raw rows remain intact.
    // This census covers schema-filtered parameter/target metadata, not every read fact.
    // Missing reference parameters: 33 commands (coordinates, field/axis selectors, and
    // style/scenario/rule names or indexes), plus wrong targets on newNote/newComment.
    // The 33-command set is {table.formatAs,table.newStyle,table.totalFunction,
    // chart.insert,chart.set,sparkline.insert,image.insert,shape.insert,icon.insert,
    // textBox.insert,object.move,link.insert,conditionalFormat.manageRules,data.sortAscending,
    // data.sortDescending,data.sort,data.filterBy,data.removeDuplicates,data.textToColumns,
    // data.subtotal,name.define,pivot.insert,pivot.recommended,
    // table.summarizePivot,pivot.changeSource,data.goalSeek,data.scenarioAdd,data.scenarioShow,
    // data.scenarioDelete,data.scenarioSummary,data.dataTable,data.advancedFilter,data.consolidate}.
    // Wrong/missing targets: LESS 4 {newNote,newComment,filterBy,goalSeek}; MORE 5
    // {newNote,newComment,filterBy,manageRules,summarizePivot}. The two note targets are
    // the only LESS commands with no missing reference parameter. Literal-as-place MORE:
    // 4 {conditionalFormat.clearRules,pivot.insert,table.summarizePivot,pivot.grandTotals}; overlap
    // with wrong-target MORE is summarizePivot. Reviewed MORE8 is therefore the union:
    // {note.add,comment.add,data.filterBy,conditionalFormat.manageRules,table.summarizePivot,
    // conditionalFormat.clearRules,pivot.insert,pivot.grandTotals}. Source examples: destination/at,
    // refersTo/criteria, column(s)/keys[].row, style/replace, scenario name, rule indexes.
    // Roles are now corrected; these missing/excess floors must remain zero.
    let less_baseline: BTreeSet<&str> = BTreeSet::new();
    let more_baseline: BTreeSet<&str> = BTreeSet::new();
    eprintln!("data reference projection: {} writes; LESS {less:?}; MORE {more:?}", compared.len());
    assert!(less.is_subset(&less_baseline), "new missing data reference roles: {less:?}");
    assert!(more.is_subset(&more_baseline), "new excessive data reference roles: {more:?}");
}

/// Reviewed projections of the blind table-definition/source/style and outline-boundary
/// facts. These use populated objects and source rows, including both stopping neighbours;
/// unrelated-cell exclusions for the same commands remain in the all93 projection above.
#[test]
fn reviewed_narrowed_table_and_outline_inputs_are_held() {
    use kition_kit::kdm::{ActorId, Replica, tool_ids::ToolIds};
    use kition_kit::session::KitModel;
    use kition_kit::{KitDocument, Under};
    use kition_sheet_kit::SheetDocument;
    use serde_json::json;
    use std::collections::BTreeSet;

    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    doc.execute("range.setValues", json!({"range":"A1","values":[["Region","Amount","Other"],["East",2,3],["West",4,5],["East",6,7],["West",8,9]]}))
        .expect("table source");
    doc.execute("table.insert", json!({"range":"A1:C5","name":"Sales","header":true})).expect("table");
    doc.execute("table.totalRow", json!({"table":"Sales","on":true})).expect("materialized totals");
    doc.execute("cell.set", json!({"cell":"D8","input":"padding"})).expect("materialized boundary axes");
    doc.execute("data.group", json!({"rows":"2:4"})).expect("row outline");
    doc.execute("data.group", json!({"cols":"B:C"})).expect("column outline");
    doc.execute("select.set", json!({"range":"B3"})).expect("table and outline selection");
    doc.version().expect("identities");
    let model = Replica::load(kdm_model::SheetModel::schema(), &doc.snapshot().expect("snapshot"), ActorId::random()).expect("model");
    let names = ToolIds::of_replica(&model);
    let sheet = names.resolve("sheet:1", |id| model.exists(id)).expect("source sheet");
    let rows = model.list(&sheet, "rows").expect("source rows");
    let cols = model.list(&sheet, "cols").expect("source columns");
    let table = model
        .objects_of_type("Table")
        .into_iter()
        .find(|id| model.get(id, "name").ok().and_then(|value| value.as_str().map(str::to_owned)).as_deref() == Some("Sales"))
        .expect("table identity");
    let table_id = names.show(&table);

    fn held(under: &Under, id: &str, field: &str) -> bool {
        under.premises.iter().any(|premise| {
            premise.id == id
                && premise.fields.as_ref().is_none_or(|fields| {
                    fields.iter().any(|stored| stored == field || field.strip_prefix(stored).is_some_and(|suffix| suffix.starts_with('.')))
                })
        })
    }

    let mut missing = Vec::new();
    let mut compared = BTreeSet::new();
    for (command, params) in [
        ("table.bandedRows", json!({"table":"Sales"})),
        ("table.bandedColumns", json!({"table":"Sales"})),
        ("table.filterButton", json!({"table":"Sales"})),
        ("table.firstColumn", json!({"table":"Sales"})),
        ("table.lastColumn", json!({"table":"Sales"})),
        ("table.style", json!({"table":"Sales","style":"None"})),
        ("table.headerRow", json!({"table":"Sales","on":false})),
        ("table.totalRow", json!({"table":"Sales","on":false})),
        ("table.totalFunction", json!({"table":"Sales","column":"Amount","function":"average"})),
        ("table.convertToRange", json!({"table":"Sales"})),
        ("table.rename", json!({"table":"Sales","name":"Revenue"})),
    ] {
        compared.insert(command);
        let under = doc.under(command, &params).expect(command).expect("table premise");
        let mut required = vec![("sheet:1".to_owned(), "tables[name=\"Sales\"]")];
        if command == "table.headerRow" {
            // Pinned after reviewing header-off: it preserves the latest column
            // names and does not use them to remove the existing header row.
            required =
                vec![("sheet:1".to_owned(), "tables[name=\"Sales\"].range"), ("sheet:1".to_owned(), "tables[name=\"Sales\"].props.header_row")];
        }
        match command {
            // same-field decision: these commands replace destination input.
            "table.headerRow" => required.extend([("cell:1!A1".into(), "style"), ("cell:1!A1".into(), "input")]),
            "table.totalRow" => required.extend([("cell:1!B6".into(), "style"), ("cell:1!B6".into(), "input")]),
            "table.totalFunction" => required.extend([("cell:1!B6".into(), "style"), ("cell:1!B6".into(), "input"), ("cell:1!B2".into(), "input")]),
            "table.convertToRange" => required.extend([("cell:1!B2".into(), "style"), ("workbook".into(), "theme")]),
            "table.rename" => required.extend([("workbook".into(), "~tableNames"), (table_id.clone(), "~formulaInputs")]),
            _ => {}
        }
        for (id, field) in required {
            if !held(&under, &id, field) {
                missing.push(format!("{command} {params}: {id}.{field}"));
            }
        }
    }
    for (command, params, columns, first, last, detail) in [
        ("data.group", json!({"rows":"3"}), false, 2, 2, false),
        ("data.ungroup", json!({"cols":"B:C"}), true, 1, 2, false),
        ("data.hideDetail", json!({"rows":"3"}), false, 0, 4, true),
        ("data.showDetail", json!({}), false, 0, 4, true),
    ] {
        compared.insert(command);
        let under = doc.under(command, &params).expect(command).expect("outline premise");
        let lines = if columns { &cols } else { &rows };
        for (index, raw) in lines.iter().enumerate().take(last + 1).skip(first) {
            let id = format!("{}:{raw}", if columns { "col" } else { "row" });
            // Pinned after normalized-field review: these derived values are scalar outline
            // and hidden flags, including false/default values for the stopping neighbours.
            if !held(&under, &id, "~outline") && !held(&under, &id, "info.outline") {
                missing.push(format!("{command}: {id} outline at index {index}"));
            }
            if (command == "data.ungroup" || detail && index > first && index < last)
                && !held(&under, &id, "~hidden")
                && !held(&under, &id, "info.hidden")
            {
                missing.push(format!("{command}: {id} hidden at index {index}"));
            }
        }
    }
    assert_eq!(compared.len(), 15);
    eprintln!("reviewed narrowed data inputs: {} commands; missing={missing:?}", compared.len());
    assert!(missing.is_empty(), "new missing bounded data facts: {missing:?}");
}
