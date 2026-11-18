//! Blind engine readings for C8-sheet-read; written before opening binding declarations.
//! Inline same-field decision notes identify later replacement-hold amendments.
//! `part` is the active sheet, `active` the selected drawing, and named session members
//! are recorded even where the SDK's four editor-state categories cannot represent them.
//! Every edit also runs cmd/mod.rs::edit/commit: table-header synchronization, connector
//! rerouting on changed sheets, cell-content protection, and recalc/row fitting for touched
//! cells. Those consequences inherit the source facts recorded for the initiating edit.
//! Original READINGS rows were committed blind; later corrections and branches are labelled inline.

pub const READINGS: &str = r##"
# cmd/extra.rs: chart lookup is active-sheet-local, integer chart id, else LAST chart (not selected_shape).
ROW chart.addElement | params={"chart":1,"element":"title"} | resolves=chart integer id in active sheet; omitted chart selects last | holds=chosen chart membership, title; active sheet identity; replaced values: title | reads=part | writes=title retaining nonempty old title else Chart Title | note=extra.rs:268 title on path; same-field decision
ROW chart.addElement | params={"chart":1,"element":"axisTitles"} | resolves=chart integer id in active sheet; omitted chart selects last | holds=chosen chart membership,x_title,y_title; replaced values: x_title,y_title | reads=part | writes=axis titles retaining each existing title else Axis Title | note=extra.rs:290 axisTitles on path; same-field decision
ROW chart.addElement | params={"chart":1,"element":"legend","on":false} | resolves=chart integer id active sheet; omitted chart last | holds=chosen chart membership; replaced values: legend | reads=part | writes=legend only | note=extra.rs:273 explicit scalar branch; same-field decision
ROW chart.addElement | params={"chart":1,"element":"smooth","on":true} | resolves=chart integer id active sheet; omitted chart last | holds=chosen chart membership,series membership; replaced values: every series smooth | reads=part | writes=each existing series smooth; other fields preserved | note=extra.rs:294 smooth branch; dataLabels and gridlines similarly explicit flags; same-field decision
ROW chart.addElement | params={"chart":1,"element":"title","on":false} | resolves=integer chart id on active sheet; omitted chart selects last | holds=chosen chart membership; replaced title | reads=part | writes=title from explicit on=false | note=extra.rs::add_element; same-field decision
ROW chart.addElement | params={"chart":1,"element":"axisTitles","on":false} | resolves=integer chart id on active sheet; omitted chart selects last | holds=chosen chart membership; replaced x_title,y_title | reads=part | writes=x_title,y_title from explicit on=false | note=extra.rs::add_element; same-field decision
ROW chart.addElement | params={"chart":1,"element":"dataLabels","on":false} | resolves=integer chart id on active sheet; omitted chart selects last | holds=chosen chart membership; replaced data_labels | reads=part | writes=data_labels from explicit on=false | note=extra.rs::add_element; same-field decision
ROW chart.addElement | params={"chart":1,"element":"gridlines","on":false} | resolves=integer chart id on active sheet; omitted chart selects last | holds=chosen chart membership; replaced gridlines | reads=part | writes=gridlines from explicit on=false | note=extra.rs::add_element; same-field decision
ROW chart.quickLayout | params={"chart":1,"layout":1} | resolves=chart integer id active sheet; omitted chart last | holds=chosen chart membership; replaced values: legend,data_labels,gridlines,x_title,y_title | reads=part | writes=legend,data_labels,gridlines,x_title=None,y_title=None | note=extra.rs:304 layouts 1-4 replace values; same-field decision
ROW chart.quickLayout | params={"chart":1,"layout":5} | resolves=chart integer id active sheet; omitted chart last | holds=chosen chart membership,x_title,y_title; replaced values: legend,data_labels,gridlines,x_title,y_title | reads=part | writes=legend,data_labels,gridlines,missing axis titles | note=extra.rs:304 layouts 5-6 preserve existing axis titles; same-field decision
ROW chart.changeColors | params={"chart":1,"colors":["#123456"]} | resolves=chart integer id active sheet; omitted chart last | holds=chosen chart kind,series membership; for non-pie no cell sources; replaced values: per-series color and point_colors when pie/doughnut | reads=part | writes=per-series colors by index | note=extra.rs:329 non-pie branch; same-field decision
ROW chart.changeColors | params={"chart":1,"palette":"accent1"} | resolves=chart integer id active sheet; omitted chart last | holds=pie/doughnut chart kind,first-series values formula and evaluated shape,all source/nested formula cells,names,tables,address windows determining point count; replaced values: per-series color and point_colors when pie/doughnut | reads=part | writes=point_colors length=min(first resolved series length,10000), clears each series color | note=extra.rs:364 pie/doughnut branch; theme colors stay symbolic; same-field decision
ROW chart.selectData | params={"chart":1,"range":"Sheet2!A1:C5"} | resolves=chart integer id on ACTIVE sheet; source range sheet NAME from prefix else active; A1 rectangle; byRows explicit else width>height+1 | holds=chosen chart membership; source sheet name,range addresses,cached source values; first-column effective number formats for date-label detection; inherited row/column styles and definitions; replaced values: series,source,by_rows | reads=part | writes=series,source,by_rows | note=extra.rs:382 -> insert.rs:937 series_from_range; source sheet differs from destination; same-field decision
ROW chart.changeType | params={"chart":1,"type":"line"} | resolves=delegates chart.set; active-sheet chart id else last | holds=chosen chart membership and existing kind for totals validation if totals supplied; replaced values: supplied kind,totals,title,legend,data_labels,gridlines,style,x_title,y_title and anchor fields | reads=part | writes=chart.set supplied kind and optional chart fields | note=extra.rs:39 -> insert.rs:1164; no source evaluation; same-field decision
ROW chart.move | params={"chart":1,"sheet":"Sheet2","at":"B2"} | resolves=chart id on active SOURCE sheet; sheet is DESTINATION name; at parses A1 and discards any prefix | holds=source chart membership/frame and source stack position,destination sheet name and existence,destination stack insertion edge; at address windows on DESTINATION sheet | reads=part | writes=remove source chart,push destination chart,anchor cell if at | note=extra.rs:403; sheet does not choose source chart
ROW chart.move | params={"chart":1} | resolves=chart id on active sheet else last; no destination sheet means same sheet | holds=source chart membership,source chart stack order | reads=part | writes=remove chart then append it on same sheet | note=extra.rs:403 missing sheet still changes z order
ROW chart.formatSelection | params={"chart":1,"series":0,"color":"#123456"} | resolves=chart integer id active sheet else last; series is zero-based INDEX not object id; omitted series zero | holds=chosen chart series membership/order; replaced values: chosen series color | reads=part | writes=selected series color; out-of-range series no-op | note=extra.rs:421; same-field decision
ROW chart.shapeFill | params={"chart":1,"color":"#123456"} | resolves=kind other than shape delegates format_series; chart integer id active sheet else last; series index default 0 | holds=chosen chart series membership/order; replaced values: chosen series color | reads=part | writes=series color | note=extra.rs:432 chart branch; same-field decision
ROW chart.shapeFill | params={"kind":"shape","id":1,"color":"#123456"} | resolves=id integer shape id ACTIVE sheet; missing id LAST shape including ink/icons | holds=chosen shape membership; replaced values: chosen shape look.fill | reads=part | writes=shape fill color only | note=extra.rs:432 -> with_shape:445; selected_shape ignored; same-field decision
ROW chart.shapeOutline | params={"id":1,"color":"#123456"} | resolves=id integer shape id active sheet; missing id LAST shape; kind ignored | holds=chosen shape membership; replaced values: chosen shape look.line | reads=part | writes=shape line color only | note=extra.rs:440; selected_shape ignored; same-field decision
ROW arrange.bringForward | params={"kind":"shape","id":1} | resolves=integer id in active sheet list chosen by kind chart/image/else shape | holds=chosen object's membership and list z order; neighbouring position for one-step move | reads=part | writes=reorder same-kind list one step forward or to end | note=extra.rs:461; toFront OR toBack controls extreme regardless direction
ROW arrange.sendBackward | params={"kind":"shape","id":1,"toBack":true} | resolves=integer id active sheet list by kind chart/image/else shape | holds=chosen membership and list order; first edge if extreme | reads=part | writes=reorder same-kind list backward or to start | note=extra.rs:461; missing id object is silent no-op
ROW arrange.align | params={"kind":"shape","ids":[1,2],"align":"right"} | resolves=ids numeric array cast to u32 in active sheet list by kind; strings ignored; align unknown means middle | holds=chosen anchors incl size; row/column geometry from origin through old and resulting anchors; connectors and endpoint geometry rerouted by edit | reads=part | writes=anchors aligned to selected bounding box | note=extra.rs:525 requires at least two matched ids
ROW arrange.group | params={"kind":"shape","ids":[1,2]} | resolves=ids numeric array on active sheet kind list; forces align=left | holds=chosen anchors,row/column geometry through old/new anchors; connected geometry | reads=part | writes=left-align anchors ONLY; no group membership | note=extra.rs:115 -> align:525 despite group command name
ROW arrange.rotate | params={"id":1,"flip":"horizontal"} | resolves=shape integer id active sheet else LAST shape; kind and flip ignored | holds=chosen shape anchor width,height | reads=part | writes=swap anchor width and height | note=extra.rs:572 no actual flip or rotation property
ROW view.normal | params={} | resolves=no object parameters | holds=none | reads=none | writes=session view_mode=normal | note=extra.rs:576
ROW view.pageLayout | params={} | resolves=no object parameters | holds=none | reads=none | writes=session view_mode=pageLayout | note=extra.rs:576
ROW view.pageBreakPreview | params={} | resolves=no object parameters | holds=none | reads=none | writes=session view_mode=pageBreakPreview | note=extra.rs:576
ROW view.customViews | params={"save":"work"} | resolves=save is key within JSON in first exact-case hidden defined name _sc_custom_views; name scope ignored | holds=stored _sc_custom_views formula,active sheet tab INDEX,active sheet zoom and freeze | reads=part,selection | writes=hidden-name JSON entry sheet index,selection A1,zoom,freeze; replacing duplicate exact names | note=extra.rs:601 save path uses stash_get/stash_set
ROW view.customViews | params={"show":"work"} | resolves=show is JSON key in _sc_custom_views; stored sheet NUMERIC INDEX; stored selection A1 | holds=stored view data,tab index window for stored sheet,stored selection addresses; replaced values: destination zoom and freeze; pane origin when restored counts replace it | reads=none | writes=activate stored sheet,set selection,set stored zoom; stored pane counts restored | note=extra.rs:614 show path executes sheet.activate/selection.set/view.zoom; same-field decision
ROW view.customViews | params={"delete":"work"} | resolves=delete JSON key in exact-case _sc_custom_views hidden name | holds=_sc_custom_views formula | reads=none | writes=remove requested key,replace hidden name | note=extra.rs:626 delete path; params empty only queries stash
ROW view.split | params={} | resolves=active sheet; active cell used only when currently unfrozen | holds=active sheet freeze,top-left viewport/address windows when freezing | reads=part,selection | writes=freeze panes at active cell or clear freeze; top-left reset through delegated view commands | note=extra.rs:640 -> view.freezePanes/view.unfreezePanes
ROW view.newWindow | params={} | resolves=active document window | holds=whole active workbook copy,document path and display title | reads=part,selection,windows | writes=new independent DocState with copied workbook and selection,then activates it | note=extra.rs:645 despite linked-copy wording
ROW view.arrangeAll | params={} | resolves=none | holds=none | reads=none | writes=none | note=extra.rs:132 handler is ok()
ROW view.hideWindow | params={} | resolves=current session active window index | holds=none | reads=windows | writes=append active index to hidden_windows,activate first unhidden window or 0 | note=extra.rs:654
ROW view.unhideWindow | params={} | resolves=index u32 else last hidden session window index | holds=none | reads=windows | writes=remove hidden index and set active window | note=extra.rs:665
ROW view.ruler | params={"on":true} | resolves=none | holds=none | reads=none | writes=none | note=extra.rs:135 handler is ok()
ROW table.resize | params={"table":"Sales","range":"A1:D20"} | resolves=table case-insensitive NAME in ACTIVE sheet else table covering active cell; range must qualify SAME active sheet,whole columns permitted/clamped | holds=chosen table range/columns; active sheet name; overlapping table extents incl absence; array formula extents incl absence; new header values; overlapping merges; row/column address windows; header synchronization facts | reads=part | writes=table range/columns reconstructed from new header values matching old column metadata by name; removes overlapping merges; recalc | note=extra.rs:733; missing table additionally reads selection active
ROW table.resize | params={"range":"A:D"} | resolves=table covering active cell in ACTIVE sheet; whole columns row 0..MAX_ROWS-1 | holds=selected table columns/range,active sheet name,overlaps and arrays,new header row cells,merges,new address windows | reads=part,selection | writes=resize table preserving header row and intersecting original | note=extra.rs:733 selection fallback
ROW table.removeDuplicates | params={"table":"Sales","columns":[0]} | resolves=case-insensitive table NAME active sheet else selected-cell table; columns zero-based relative positions delegated | holds=table extent/header/totals flags,data rows full cell contents/styles,comparison column values,merges,array ranges,protection,fit row facts | reads=part | writes=delegates data.removeDuplicates; shrink table end when removed>0,no totals,and removed<data height | note=extra.rs:782 table path; unnamed table adds selection
ROW table.removeDuplicates | params={"range":"A1:C5"} | resolves=no matching selected table delegates data.removeDuplicates unchanged; range sheet handling follows data handler | holds=duplicate-comparison block,current-region boundary if singleton,row contents/style,merges,objects moved by deletion,protection,fit rows | reads=part,selection | writes=data.removeDuplicates effects | note=extra.rs:790 fallback path
ROW data.fromTextCsv | params={"text":"Name,Value\nA,1","sheet":"Imported"} | resolves=sheet is NEW sheet name not target; text literal or external path; delimiter explicit else first-line tab/comma | holds=workbook sheet-name uniqueness and tab insertion edge; table names/id uniqueness; input bytes; data parser defaults | reads=part | writes=insertSheet then A1 values then table then autofit; table/autofit errors swallowed | note=extra.rs:813 -> from_csv:840; failed named insertion retries default name
ROW data.getData | params={"text":"[{\"Name\":\"A\",\"Value\":1}]"} | resolves=JSON object keys union becomes header row; source path or text; optional sheet names new sheet | holds=sheet-name uniqueness,tab insertion edge,table uniqueness,id allocator,input bytes,CSV/date parsing defaults if CSV | reads=part | writes=new sheet,static values,new table,autofit | note=extra.rs:813 JSON branch; missing input requests open dialog
ROW data.fromTableRange | params={"range":"Sheet2!A1:C5"} | resolves=range strips sheet prefix but source is ACTIVE sheet; absent range selection; singleton expands current region | holds=active sheet name,source values,source number formats/effective styles/date system,source region empty boundary,source addresses; import sheet/table uniqueness | reads=part | writes=formatted TSV snapshot imported to new sheet named source (2) | note=extra.rs:855; missing range additionally selection; prefix does not change source sheet
ROW automate.recordActions | params={} | resolves=no object name; recording marker is session journal index | holds=none | reads=recording,journal | writes=recording=start current journal length | note=extra.rs:882 start path
ROW automate.recordActions | params={"stop":true,"name":"Work"} | resolves=name is JSON script key in _sc_scripts hidden defined name; fallback Script recording-start-index | holds=_sc_scripts stored JSON/name slot | reads=recording,journal | writes=stop recording and save non-automate journal calls as script | note=extra.rs:883 stop OR already-recording path; absent recording returns false without name write
ROW automate.newScript | params={"name":"Work","commands":[{"command":"cell.set","params":{"cell":"A1","input":"1"}}]} | resolves=name JSON key in exact-case _sc_scripts hidden defined name; commands remain stored data | holds=_sc_scripts formula and membership; no invoked-command sources at creation | reads=none | writes=replace named script retaining other scripts; stash replaces duplicate exact-name definitions | note=extra.rs:899
ROW automate.runScript | params={"name":"Work"} | resolves=name JSON key inside exact-case _sc_scripts hidden name; each nested command resolves its own params at execution time | holds=_sc_scripts formula; union of nested handlers workbook and editor prerequisites,including editor state modified by earlier steps; max100000 steps | reads=part,selection,active,mode,journal | writes=execute each non-automate nested command after all host gates pass | note=extra.rs:925 arbitrary stored commands; cannot narrow independently without expanding script
ROW automate.deleteScript | params={"name":"Work"} | resolves=name JSON key inside exact-case _sc_scripts hidden name | holds=_sc_scripts formula | reads=none | writes=remove script key and replace hidden definition | note=extra.rs:955
ROW insert.checkbox | params={"sheet":"Sheet2","range":"A1:B2"} | resolves=sheet by INDEX integer or NAME string,prefix else active; range A1 list else selection; merged non-anchor cells skipped | holds=destination input/formula,array membership incl overlapping arrays,merge coverage,effective inherited style,protection,fit rows; destination address windows | reads=none | writes=checkbox style centered,boolean value preserves existing literal bool otherwise false,remove formula/rich/picture | note=extra.rs:997 explicit target path; absent sheet adds part,absent range adds selection
ROW insert.checkbox | params={} | resolves=active sheet and selected ranges; merged anchors only | holds=destination values/formulas,arrays,merges,styles,protection,fit rows,address windows | reads=part,selection | writes=checkbox styles and bool values; max100000 cells | note=extra.rs:997 implicit target
ROW cell.toggleCheckbox | params={"sheet":"Sheet2","cell":"B2"} | resolves=sheet index/name or cell prefix else active; A1 cell redirects merge to top-left | holds=destination merge coverage,effective style checkbox bit,existing bool and formula status,protection,fit row facts,address windows | reads=none | writes=invert bool only if materialized literal boolean checkbox | note=extra.rs:1049 explicit path
ROW cell.toggleCheckbox | params={} | resolves=active sheet; every selection cell checked using merged-anchor style/value but selected coordinate itself retained | holds=selected cells and merge-anchor styles/values,merges,formula status,protection,fit rows,address windows | reads=part,selection | writes=literal boolean at selected coordinate only; formula checkbox or merged non-anchor counted but not written | note=extra.rs:1053 selection path
ROW pageLayout.themeColors | params={"colors":["#000000","#FFFFFF","#222222","#EEEEEE","#112233","#223344","#334455","#445566","#556677","#667788","#778899","#8899AA"]} | resolves=missing name defaults workbook theme name; supplied name chooses original theme preset | holds=current theme name when missing; current theme font defaults retained by pageLayout.theme; replaced values: theme name,colors and supplied font fields | reads=none | writes=delegates pageLayout.theme replacing colors/name,retaining fonts unless supplied | note=extra.rs:1081 -> view.rs:444; same-field decision
ROW pageLayout.themeFonts | params={"major":"Kition Sans","minor":"Kition Sans"} | resolves=major/minor literal font family names,not object ids | holds=current theme name and all twelve colors; omitted font fields preserve current fonts; replaced values: theme name,colors,major_font,minor_font | reads=none | writes=delegates pageLayout.theme keeping old colors/name | note=extra.rs:1089; same-field decision
ROW pageLayout.themeEffects | params={"name":"Simple"} | resolves=none | holds=none | reads=none | writes=none | note=extra.rs:198 handler is ok()
ROW pageLayout.background | params={"clear":true} | resolves=active sheet; images selected by exact alt text Sheet background | holds=matching-image membership incl absence and image alt; active sheet identity; replaced values: entire removed background images | reads=part | writes=remove every image whose alt equals Sheet background | note=extra.rs:1095 clear branch; same-field decision
ROW pageLayout.background | params={"data":"AA=="} | resolves=delegates insert.picture with forced at=A1,alt=Sheet background; picture source from data/path/url | holds=image id allocator,destination A1 address window,source bytes; image insertion footprint | reads=part | writes=append image on active sheet with background alt | note=extra.rs:1103 insert branch
ROW home.drawBorder | params={"sheet":"Sheet2","range":"A1:B2","edge":"top"} | resolves=delegates home.borders using preset from edge; sheet index/name/prefix; A1 ranges else selection | holds=destination effective styles,edge-neighbour border styles,merge coverage,protection,address windows,fit rows affected by formatting | reads=none | writes=border edges; unspecified edge defaults all; outline maps outside | note=extra.rs:1110 -> format.rs::borders; absent target adds part/selection
ROW home.newCellStyle | params={"name":"Review","fromCell":"Sheet2!B2"} | resolves=name case-insensitive cell-style definition; fromCell A1 strips sheet prefix and ALWAYS uses active sheet | holds=source effective cell/row/column style and definition,source address windows; existing case-insensitive cell_style_info builtin metadata and chosen definition | reads=part | writes=replace case-insensitive named style; preserve builtin metadata marked custom_builtin | note=extra.rs:1128 source prefix ignored
ROW home.newCellStyle | params={"name":"Review"} | resolves=name case-insensitive style; source active cell on active sheet | holds=active-cell effective inherited style,source address windows,existing style builtin metadata | reads=part,selection | writes=named style definition | note=extra.rs:1133 fallback to selection.active
ROW formulas.useInFormula | params={"name":"Revenue"} | resolves=name literal text; never resolves defined name here | holds=none | reads=none | writes=UI EditCell request with =Revenue only | note=extra.rs:1154 no workbook write
ROW formulas.watchWindow | params={"add":"Sheet2!B2"} | resolves=add literal watch-address string; hidden name exact _sc_watch; referenced sheet NAME else active,invalid name falls back active,invalid cell falls back A1 | holds=_sc_watch list; for returned values watched cells,formulas and sheet-name lookup; watch list update itself needs only prior list | reads=part | writes=hidden name directly via Arc::make_mut despite query-marked registry | note=extra.rs:1161 add/remove mutate wb with no undo
ROW formulas.watchWindow | params={"remove":"Sheet2!B2"} | resolves=remove exact watch-address string key; no A1 normalization | holds=_sc_watch stored list; remaining watched-cell data for response | reads=part | writes=replace watch list excluding exact string | note=extra.rs:1177 query-marked mutation

# cmd/draw.rs and cmd/ink.rs: ink geometry is sheet points converted through active-sheet axes.
ROW draw.pen | params={"on":true} | resolves=no workbook object | holds=none | reads=mode,draw_color,draw_width | writes=session draw_tool,default color/width when changing tool,clear on off only if same tool | note=draw.rs:66; explicit color/width override defaults
ROW draw.pencil | params={"on":true} | resolves=no workbook object | holds=none | reads=mode,draw_color,draw_width | writes=session pencil tool and tool-specific defaults when changing | note=draw.rs:66
ROW draw.highlighter | params={"on":true} | resolves=no workbook object | holds=none | reads=mode,draw_color,draw_width | writes=session highlighter tool and defaults when changing | note=draw.rs:66
ROW draw.eraser | params={"on":false} | resolves=no workbook object | holds=none | reads=mode,draw_color,draw_width | writes=session draw_tool only; returns draw_state | note=draw.rs:90 off retains another tool
ROW draw.select | params={} | resolves=none | holds=none | reads=draw_color,draw_width | writes=clear draw_tool and draw_shape; returns draw_state | note=draw.rs:16
ROW draw.shape | params={"preset":"rect"} | resolves=preset via shared preset table/line presets | holds=none | reads=none | writes=session draw_shape and clear draw_tool when preset supplied; null clears draw_shape only | note=draw.rs:23
ROW draw.lasso | params={} | resolves=none | holds=none | reads=none | writes=clear session draw_tool | note=draw.rs:38
ROW draw.stroke | params={"points":[[10,10],[80,40]],"color":"#123456","width":2,"tool":"pen"} | resolves=active sheet only,points min bounding corner converted to anchor using row_at/col_at | holds=sheet row/column sizes/hidden geometry from origin through minimum point; object id allocator | reads=part | writes=append absolute-anchor Ink shape with relative point text,width/alpha/color | note=draw.rs:115 explicit pen; geometry used even though ink is absolute
ROW draw.stroke | params={"points":[[10,10],[80,40]]} | resolves=active sheet; missing tool/color/width from session pen defaults | holds=axis geometry through anchor; object id allocator | reads=part,mode,draw_color,draw_width | writes=append Ink shape | note=draw.rs:140 omitted pen settings
ROW draw.inkToShape | params={"id":1} | resolves=id INTEGER active-sheet shape id with kind=Ink; omitted id LAST ink in shape order | holds=chosen ink kind,text points,frame width/height,line color; shape order/membership when default; replaced values: kind,fill,text | reads=part | writes=kind inferred from closedness/corners,fill=line for open line else Auto,clear text | note=draw.rs:203 does not read selected_shape; same-field decision
ROW drawing.ink | params={"strokes":[{"points":[[10,10],[80,40]]}]} | resolves=active sheet; shared stroke parser; omitted pen members current session; each stroke delegates draw.stroke | holds=axis geometry through each anchor; object id allocator; existing ink-id set for returned ids | reads=part,mode,draw_color,draw_width | writes=append one Ink per stroke; one-point dot duplicates its point | note=ink.rs:73; pressure dropped when delegated to draw.stroke
ROW drawing.ink | params={"strokes":[{"points":[[10,10],[80,40]],"color":"#123456","width":2,"tool":"pen"}]} | resolves=active sheet; all pen members explicit | holds=axis geometry through stroke anchors; object id allocator; existing ink ids for result | reads=part | writes=append ink shape | note=ink.rs:73 explicit settings still current_pen constructed but does not determine output
ROW drawing.erase | params={"x":10,"y":10,"radius":4} | resolves=active sheet; hit-tests EVERY ink shape in sheet coordinates with shared eraser algorithm | holds=active sheet protection; all ink geometry potentially entering/leaving gesture incl previously unhit strokes; axis geometry affecting ink positions | reads=part,active | writes=remove hit shapes,clear selected_shape if hit | note=ink.rs:99; hit set empty returns no mutation

# cmd/drawing.rs: ids are numeric strings/numbers resolved in ACTIVE sheet, groups via group keys.
ROW drawing.insert | params={"kind":"shape","frame":{"x":10,"y":10,"w":80,"h":40},"geometry":{"preset":"rect"}} | resolves=active sheet; frame in sheet points maps through axes; no target object | holds=sheet protection,axis geometry from origin through frame anchor,object id allocator; connector reroute footprint on changed sheet | reads=part | writes=append shape default theme-relative fill/line and optional supplied look/text; selects new id | note=drawing.rs:205 explicit frame; next_object_id scans all sheets
ROW drawing.insert | params={"kind":"textBox","text":"Note"} | resolves=active sheet and active cell for frame origin; default144x48 (shape144x96) | holds=protection,active-cell address windows,axis geometry through active anchor,object id allocator | reads=part,selection | writes=append text box/shape and select it | note=drawing.rs:228 implicit frame
ROW drawing.insert | params={"kind":"chart","range":"A1:C5","chart":{"kind":"column"}} | resolves=delegates insert.chart using range; source sheet prefix forwarded; later apply always active sheet; range absent selection/current region | holds=source values,first-column effective number formats,source name/windows,default-title source,object id allocator; requested frame axis geometry and protection | reads=part | writes=insert chart then supplied shared chart fields and optional frame transform; select id | note=drawing.rs:270; missing range adds selection/current-region boundary
ROW drawing.delete | params={"ids":[1]} | resolves=numeric-string/number ids on active sheet; id may identify shape,chart,or group key | holds=chosen membership/kind/group membership,active sheet protection; matching group descendants; connector geometry during post-edit reroute; replaced values: entire selected shapes/charts/group descendants | reads=part,active | writes=remove named shapes/charts or every group descendant; selected_shape cleared only if named id | note=drawing.rs:318 delete path; removed content is held under the same-field decision; same-field decision
ROW drawing.transform | params={"ids":[1],"dx":10,"dy":20} | resolves=numeric ids active sheet; group keys select descendant shapes; chart id selects chart | holds=chosen frame/placement and group membership,protection,axis geometry through old/new anchors; connector connections/endpoints/geometry; chart sources evaluated for response frames on chart path | reads=part | writes=relative transform or group proportional member transforms,reroute connectors | note=drawing.rs:318 relative frame path; group rotate/flip refused
ROW drawing.transform | params={"id":1,"frame":{"x":10,"y":10,"w":80,"h":40}} | resolves=single numeric shape/chart id active sheet; group allowed for move/scale | holds=old frame for change detection and untouched placement/group restrictions; sheet protection; axes mapping new point; grouped members frames; connector dependencies | reads=part | writes=replace supplied absolute frame; preserve anchor mode; chart sizes clamp20..10000 | note=drawing.rs:318 absolute path; old frame can affect no-op and group scale result
ROW drawing.format | params={"ids":[1],"fill":{"color":"#123456"}} | resolves=active-sheet numeric shape ids; groups and charts reject nontrivial format | holds=chosen shape kind and previous fill/gradient/alpha for equality,protection; connector rerouting dependencies of changed sheet | reads=part | writes=fill and clears gradient where solid; no source cells on ordinary shape path | note=drawing.rs:318 -> shared ops::Format -> write_back:1037
ROW drawing.format | params={"ids":[1],"stroke":{"color":"#123456"},"effects":{"shadow":null}} | resolves=numeric shape ids active sheet | holds=chosen shape kind,line,outline width/dash/alpha and effects preserving omitted members; sheet protection | reads=part | writes=merge stroke/effects or replace geometry when supplied; unchanged fields preserved | note=drawing.rs:1071 shared merge_stroke/default line/effects merge
ROW drawing.text | params={"id":1,"text":"New text"} | resolves=numeric id active sheet shape,textbox; chart/ink text changes refused; group nontrivial text refused | holds=shape kind,old text and text_body incl run styling and warp; protection | reads=part | writes=plain text plus retexted XML preserving styles; explicit paragraphs serialize requested styles | note=drawing.rs:1094 and shared ops::Text
ROW drawing.text | params={"id":1,"warp":"textArchUp"} | resolves=numeric shape id active sheet; warp preset from shared supported list | holds=old shape kind,text,text_body/warp/paragraphs; protection | reads=part | writes=serialize text body preserving paragraphs,replace/remove warp | note=drawing.rs:1100 warp-only path
ROW drawing.reconnect | params={"id":1,"end":"end","to":2,"site":0} | resolves=id connector on active sheet; to another active-sheet shape id; site INDEX of target geometry | holds=connector kind,preset/frame/connections; target existence,geometry,frame; opposite endpoint; axes through recalculated connector anchors; protection | reads=part | writes=replace one connection end; reroute connector to target site | note=drawing.rs:954 validates target existence; connect::reroute resolves target site
ROW drawing.reconnect | params={"id":1,"end":"end","point":[20,30]} | resolves=id active-sheet connector; no to frees endpoint | holds=connector kind,preset,frame,other connection end,opposite endpoint geometry if glued,axes/protection | reads=part | writes=free endpoint and optionally move endpoint; reroute remaining glued end | note=drawing.rs:318 shared ops::Reconnect
ROW drawing.connect | params={"from":1,"to":2} | resolves=from/to numeric strings or numbers converted u32,active-sheet SHAPES only; sites explicit indices else nearest geometry sites | holds=both endpoints kind,preset/custom geometry,frame; sheet axes; protection; object id allocator | reads=part | writes=append connector with new frame and stored endpoint ids/sites; selects connector | note=drawing.rs:424 -> Connect::sites
ROW drawing.group | params={"ids":[1,2]} | resolves=active-sheet shapes or outermost group keys; charts refused; nested member ids refused | holds=chosen memberships,kind,group hierarchy/top-level status; protection; object id allocator | reads=part | writes=prepend fresh outer group key on each matched descendant shape; selects group | note=drawing.rs:466; preserves member order and frames
ROW drawing.ungroup | params={"ids":[3]} | resolves=ids numeric group keys active sheet; every named id must be group | holds=group membership/hierarchy and member order; member geometry for group_object response; protection | reads=part | writes=remove group keys from every descendant,clear selected_shape,return direct members | note=drawing.rs:510; nested groups may be ungrouped directly
ROW drawing.arrange | params={"ids":[1],"order":"forward"} | resolves=ids strings/numbers on active sheet,shape and chart stacks separately; group keys absent from shape stack not found | holds=chosen memberships and active-sheet shape/chart z orders,protection | reads=part | writes=reorder shape/chart lists preserving order within selected set; charts remain above shapes | note=drawing.rs:542 -> shared arrange::arrange
ROW drawing.chart | params={"id":1,"title":"New title"} | resolves=exactly one numeric chart id active sheet; shape ids refused | holds=chosen chart type/series/settings; all evaluated name/value/category series formulas and their nested sources/names/tables/address windows; category number formats/date system; protection | reads=part | writes=supplied chart members preserving others; guards against changed categories/names/values; returns frames | note=drawing.rs:352 -> chart_spec.rs:135,247; series payload values treated as comparison,not source storage
ROW drawing.chart | params={"id":1,"series":[{"name":"Sales","values":[1,2],"color":"#123456"}]} | resolves=one numeric chart active sheet; series are positional payload entries | holds=resolved chart series name/value/category data,series membership/order and current types/settings,source dependencies and format/date interpretation | reads=part | writes=only allowed series visual fields after exact name/value length/content checks; pie one-series and combo constraints | note=chart_spec.rs:284,318 values that differ are refused
"##;

// Pinned after reviewing the complete delegated paths. The original blind rows
// above remain unchanged; these complete state sets replace their projections.
pub const REVIEWED_EDITOR: &[(&str, &[&str], &str)] = &[
    ("home.drawBorder", &["part", "selection"], "extra::draw_border delegates home.borders, including its selected-range default"),
    ("data.fromTextCsv", &["part", "selection"], "from_csv delegates insertSheet, which remembers the old selection"),
    ("data.getData", &["part", "selection"], "get_data delegates insertSheet, which remembers the old selection"),
    ("pageLayout.background", &["part"], "extra.rs background fixes at=A1; insert.rs insert_picture_at takes the fixed origin without selection"),
    ("view.newWindow", &["part", "selection"], "extra.rs::new_window copies wb.active_sheet as well as selection into the newly activated DocState"),
];

// Pinned from the declaration after review of the scalar handlers and destinations.
// Source-cell and allocation facts are compared separately; this matrix bounds
// the chosen chart object's inputs and replacement destinations.
// same-field decision: the value being replaced is held as observed.
pub const REVIEWED_CHART_FIELDS: &[(&str, &str, &[&str], &str)] = &[
    ("chart.addElement", r#"{"chart":1,"element":"title"}"#, &["parts.title"], "title preserves an existing nonempty title when on; off replaces it"),
    ("chart.addElement", r#"{"chart":1,"element":"legend","on":false}"#, &["parts.legend"], "same-field decision: replaces the observed legend"),
    ("chart.addElement", r#"{"chart":1,"element":"dataLabels"}"#, &["parts.data_labels"], "same-field decision: replaces data_labels"),
    ("chart.addElement", r#"{"chart":1,"element":"gridlines"}"#, &["parts.gridlines"], "same-field decision: replaces gridlines"),
    (
        "chart.addElement",
        r#"{"chart":1,"element":"axisTitles"}"#,
        &["parts.x_title", "parts.y_title"],
        "on preserves existing axis titles or defaults each separately",
    ),
    (
        "chart.addElement",
        r#"{"chart":1,"element":"smooth"}"#,
        &["parts.series"],
        "walks positional series; only smooth changes; series projection is still coarse",
    ),
    (
        "chart.quickLayout",
        r#"{"chart":1,"layout":2}"#,
        &["parts.legend", "parts.data_labels", "parts.gridlines", "parts.x_title", "parts.y_title"],
        "same-field decision: holds all five layout destinations",
    ),
    ("chart.changeType", r#"{"chart":1,"type":"line"}"#, &["parts.kind"], "same-field decision: explicit type replaces kind"),
    (
        "chart.formatSelection",
        r##"{"chart":1,"series":0,"color":"#123456"}"##,
        &["parts.series"],
        "chosen positional series membership and color; per-series projection is still coarse",
    ),
    ("chart.shapeFill", r##"{"chart":1,"series":0,"color":"#123456"}"##, &["parts.series"], "non-shape branch delegates format_series"),
    (
        "chart.changeColors",
        r##"{"chart":1,"colors":["#123456"]}"##,
        &["parts.kind", "parts.series"],
        "column path uses kind and series order; pie also replaces point_colors and resolves its first series point count",
    ),
    (
        "chart.selectData",
        r#"{"chart":1,"range":"A1:C4"}"#,
        &["parts.series", "parts.source", "parts.by_rows"],
        "same-field decision: replaces series/source/by_rows",
    ),
    (
        "chart.switchRowColumn",
        r#"{"chart":1}"#,
        &["parts.series", "parts.source", "parts.by_rows"],
        "source/by_rows else series_block; source sheet name, values, numfmt, date system and used-range clipping",
    ),
    ("chart.set", r#"{"chart":1,"title":"Updated"}"#, &["parts.title"], "same-field decision: assigns the observed title"),
    (
        "chart.move",
        r#"{"chart":1}"#,
        &[],
        "reviewed after the independent writer probe: retains the latest title and series while reading the retained frame and sheet positions",
    ),
];

#[cfg(test)]
mod tests {
    use super::{READINGS, REVIEWED_CHART_FIELDS, REVIEWED_EDITOR};
    use std::collections::BTreeSet;

    // Pinned after declaration review; parameter roles come from the retained
    // handler readings. Schema filtering excludes engine-only sample keys.
    #[test]
    fn every_object_write_has_exact_reviewed_parameter_roles() {
        use kition_kit::KitDocument;
        let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let ids: BTreeSet<_> = READINGS.lines().filter_map(|line| line.strip_prefix("ROW ")).filter_map(|line| line.split(" | ").next()).collect();
        let (mut less, mut more, mut compared) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        let (mut baseline_less, mut baseline_more) = (BTreeSet::new(), BTreeSet::new());
        for command in doc.commands().into_iter().filter(|command| command.mutates && ids.contains(command.id.as_str())) {
            let id = command.id.as_str();
            let (candidate, references, places): (Option<&str>, &[(&str, &[&str])], &[&str]) = match id {
                "arrange.align" | "arrange.group" => (Some("ids"), &[("ids", &["Shape", "Chart", "Image"])], &[]),
                "arrange.bringForward" | "arrange.sendBackward" => (Some("id"), &[("id", &["Shape", "Chart", "Image"])], &[]),
                "arrange.rotate" | "chart.shapeOutline" | "draw.inkToShape" => (Some("id"), &[("id", &["Shape"])], &[]),
                "chart.addElement" | "chart.changeColors" | "chart.changeType" | "chart.formatSelection" | "chart.quickLayout" => {
                    (Some("chart"), &[("chart", &["Chart"])], &[])
                }
                "chart.move" => (Some("chart"), &[("chart", &["Chart"])], &["sheet", "at"]),
                "chart.selectData" => (Some("chart"), &[("chart", &["Chart"])], &["range"]),
                // extra.rs:432 chooses id for shapes; lib.rs::refuse_unknown
                // rejects chart/on there. Chart on retains its typed target.
                "chart.shapeFill" => (Some("chart"), &[("id", &["Shape"]), ("chart", &["Chart"])], &[]),
                "drawing.arrange" => (Some("ids"), &[("ids", &["Shape", "Chart"])], &[]),
                "drawing.chart" => (Some("ids"), &[("ids", &["Chart"]), ("id", &["Chart"])], &[]),
                "drawing.connect" => (None, &[("from", &["Shape"]), ("to", &["Shape"])], &[]),
                "drawing.delete" | "drawing.format" | "drawing.transform" => {
                    (Some("ids"), &[("ids", &["Shape", "Chart", "Group"]), ("id", &["Shape", "Chart", "Group"])], &[])
                }
                "drawing.group" => (Some("ids"), &[("ids", &["Shape", "Group"])], &[]),
                "drawing.ungroup" => (Some("ids"), &[("ids", &["Group"])], &[]),
                "drawing.reconnect" => (Some("id"), &[("id", &["Shape"]), ("to", &["Shape"])], &["point"]),
                "drawing.text" => (Some("id"), &[("id", &["Shape"])], &[]),
                "drawing.erase" => (None, &[], &["path", "x", "y"]),
                // The pinned SDK reads top-level place keys literally; the
                // precise point dependencies remain in Under.
                "drawing.ink" => (None, &[], &["strokes"]),
                "draw.stroke" => (None, &[], &["points"]),
                "drawing.insert" => (None, &[], &["range", "frame"]),
                "cell.toggleCheckbox" => (Some("cell"), &[], &["sheet", "cell"]),
                "insert.checkbox" => (Some("range"), &[], &["sheet", "range", "cell"]),
                "home.drawBorder" => (Some("range"), &[], &["sheet", "range", "cell"]),
                "home.newCellStyle" => (None, &[], &["name", "fromCell"]),
                "data.fromTableRange" => (Some("range"), &[], &["range"]),
                "table.resize" => (Some("range"), &[], &["table", "range"]),
                "table.removeDuplicates" => (Some("range"), &[], &["table", "range", "columns"]),
                "automate.deleteScript"
                | "automate.newScript"
                | "automate.recordActions"
                | "automate.runScript"
                | "data.fromTextCsv"
                | "data.getData"
                | "formulas.useInFormula"
                | "formulas.watchWindow"
                | "pageLayout.background"
                | "pageLayout.themeColors"
                | "pageLayout.themeEffects"
                | "pageLayout.themeFonts"
                | "view.customViews"
                | "view.newWindow"
                | "view.split" => (None, &[], &[]),
                _ => panic!("unreviewed object write {id}"),
            };
            compared.insert(id.to_string());
            let schema = command.schema.as_ref().unwrap();
            let accepts = |key: &str| schema.get(key.split('.').next().unwrap().trim_end_matches("[]")).is_some();
            let target = candidate.filter(|key| accepts(key) && kition_kit::schema::check_targets(schema, key).is_ok());
            let objects: BTreeSet<_> = references.iter().map(|(key, _)| *key).filter(|key| accepts(key) && Some(*key) != target).collect();
            let places: BTreeSet<_> = places.iter().copied().filter(|key| accepts(key)).collect();
            let mut types: BTreeSet<_> =
                references.iter().filter(|(key, _)| accepts(key)).flat_map(|(key, types)| types.iter().map(|kind| (*key, *kind))).collect();
            let mut names = BTreeSet::new();
            if places.contains("sheet") {
                types.insert(("sheet", "Sheet"));
                names.insert(("sheet", "name"));
            }
            // Replay the declaration algorithm at 51e29b7 against the same
            // accepted public schemas; this is baseline code, not an oracle.
            let baseline_target = if id == "drawing.insert" {
                None
            } else {
                ["ids", "id", "chart", "range", "cell"]
                    .into_iter()
                    .find(|key| accepts(key))
                    .filter(|key| kition_kit::schema::check_targets(schema, key).is_ok())
            };
            let baseline_objects: BTreeSet<_> = references
                .iter()
                .map(|(key, _)| *key)
                .filter(|key| id != "draw.inkToShape" && accepts(key) && Some(*key) != baseline_target)
                .collect();
            let mut baseline_places: BTreeSet<_> =
                ["sheet", "scope", "table", "pivot", "cell", "range", "rows", "cols"].into_iter().filter(|key| accepts(key)).collect();
            match id {
                "drawing.erase" => baseline_places.extend(["path", "x", "y"]),
                "home.newCellStyle" | "formulas.useInFormula" => {
                    baseline_places.insert("name");
                }
                _ => {}
            }
            let mut baseline_types: BTreeSet<_> = match id {
                "draw.inkToShape" => BTreeSet::new(),
                "drawing.group" | "drawing.ungroup" | "arrange.group" => [("ids", "Shape")].into_iter().collect(),
                "arrange.align"
                | "arrange.bringForward"
                | "arrange.sendBackward"
                | "drawing.arrange"
                | "drawing.delete"
                | "drawing.format"
                | "drawing.transform" => {
                    ["ids", "id"].into_iter().flat_map(|key| ["Shape", "Chart", "Image", "Group"].map(|kind| (key, kind))).collect()
                }
                _ => references.iter().flat_map(|(key, types)| types.iter().map(|kind| (*key, *kind))).collect(),
            };
            let mut baseline_names = BTreeSet::new();
            if accepts("sheet") {
                baseline_types.insert(("sheet", "Sheet"));
                baseline_names.insert(("sheet", "name"));
            }
            if !objects.is_subset(&baseline_objects)
                || !places.is_subset(&baseline_places)
                || !types.is_subset(&baseline_types)
                || !names.is_subset(&baseline_names)
                || target.is_some() && target != baseline_target
            {
                baseline_less.insert(id.to_string());
            }
            if !baseline_objects.is_subset(&objects)
                || !baseline_places.is_subset(&places)
                || !baseline_types.is_subset(&types)
                || !baseline_names.is_subset(&names)
                || baseline_target.is_some() && target != baseline_target
            {
                baseline_more.insert(id.to_string());
            }
            let actual_objects: BTreeSet<_> = command.objects.iter().copied().collect();
            let actual_places: BTreeSet<_> = command.places.iter().copied().collect();
            let actual_types: BTreeSet<_> = command.object_types.iter().flat_map(|(key, types)| types.iter().map(|kind| (*key, *kind))).collect();
            let actual_names: BTreeSet<_> = command.name_params.iter().copied().collect();
            let missing = !objects.is_subset(&actual_objects)
                || !places.is_subset(&actual_places)
                || !types.is_subset(&actual_types)
                || !names.is_subset(&actual_names)
                || target.is_some() && target != command.targets;
            let extra = !actual_objects.is_subset(&objects)
                || !actual_places.is_subset(&places)
                || !actual_types.is_subset(&types)
                || !actual_names.is_subset(&names)
                || command.targets.is_some() && target != command.targets;
            if missing {
                less.insert(id.to_string());
            }
            if extra {
                more.insert(id.to_string());
            }
            if missing || extra {
                eprintln!(
                    "OBJECT_REFERENCE {id}: target {target:?}/{:?}; objects {objects:?}/{actual_objects:?}; places {places:?}/{actual_places:?}; types {types:?}/{actual_types:?}; names {names:?}/{actual_names:?}",
                    command.targets
                );
            }
        }
        assert_eq!(compared.len(), 51, "every registered object-family write must be compared");
        eprintln!("object reference comparison: {}/51 differ; LESS {less:?}; MORE {more:?}", less.union(&more).count());
        eprintln!(
            "object reference baseline51e29b7: {}/51 differ; LESS {baseline_less:?}; MORE {baseline_more:?}",
            baseline_less.union(&baseline_more).count()
        );
        assert!(less.is_empty(), "new missing object reference roles: {less:?}");
        assert!(more.is_empty(), "new excessive object reference roles: {more:?}");
    }

    #[test]
    fn scalar_chart_object_field_excesses_only_decrease() {
        use kition_kit::KitDocument;
        use serde_json::json;
        let mut doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("range.setValues", json!({"range":"A1","values":[["Name","Value","Other"],["One",1,2],["Two",3,4],["Three",5,6]]})).unwrap();
        doc.execute("insert.chart", json!({"range":"A1:C4","type":"column","title":"Original"})).unwrap();
        doc.version().unwrap();
        let mut less = BTreeSet::new();
        let mut more = BTreeSet::new();
        for (id, params, fields, _) in REVIEWED_CHART_FIELDS {
            let params = serde_json::from_str(params).unwrap();
            let under = doc.under(id, &params).unwrap().unwrap_or_default();
            let premises: Vec<_> = under.premises.iter().filter(|premise| premise.id == "drawing:1").collect();
            if fields.iter().any(|required| {
                !premises.iter().any(|premise| {
                    premise.fields.as_ref().is_none_or(|held| {
                        held.iter().any(|held| held == required || required.strip_prefix(held).is_some_and(|tail| tail.starts_with('.')))
                    })
                })
            }) {
                less.insert(*id);
            }
            if premises.iter().any(|premise| {
                premise.fields.as_ref().is_none_or(|held| {
                    held.iter().any(|held| {
                        !fields.iter().any(|allowed| held == allowed || held.strip_prefix(allowed).is_some_and(|tail| tail.starts_with('.')))
                    })
                }) || premise
                    .positions
                    .as_ref()
                    .is_none_or(|positions| positions.iter().any(|position| position != "sheet" && (*id != "chart.move" || position != "frame")))
            }) {
                more.insert(*id);
            }
        }
        eprintln!("reviewed chart object fields: LESS {less:?}; MORE {more:?}");
        assert!(less.is_empty(), "new missing chart fields: {less:?}");
        assert!(more.is_empty(), "new excessive chart fields: {more:?}");
    }

    #[test]
    fn reviewed_parameter_roles_distinguish_creation_names_and_typed_alignment_ids() {
        use kition_kit::KitDocument;
        let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let commands = doc.commands();
        let mut wrong = Vec::new();
        // Pinned after reviewing the raw rows' creation/literal/type distinctions.
        for id in ["data.fromTextCsv", "data.getData"] {
            let command = commands.iter().find(|command| command.id == id).unwrap();
            if command.places.contains(&"sheet")
                || command.object_types.iter().any(|(parameter, _)| *parameter == "sheet")
                || command.name_params.iter().any(|(parameter, _)| *parameter == "sheet")
            {
                wrong.push(format!("{id}: sheet creates a new name, not an existing Sheet reference"));
            }
        }
        let group = commands.iter().find(|command| command.id == "arrange.group").unwrap();
        let kinds: BTreeSet<_> =
            group.object_types.iter().filter(|(parameter, _)| *parameter == "ids").flat_map(|(_, kinds)| kinds.iter().copied()).collect();
        if kinds != BTreeSet::from(["Shape", "Chart", "Image"]) {
            wrong.push(format!("arrange.group: kind chooses Shape, Chart or Image anchors, declared {kinds:?}"));
        }
        let literal = commands.iter().find(|command| command.id == "formulas.useInFormula").unwrap();
        if literal.places.contains(&"name") || literal.objects.contains(&"name") {
            wrong.push("formulas.useInFormula: name is unparsed formula-bar text".into());
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn reviewed_editor_read_paths_are_exact() {
        use kition_kit::KitDocument;
        let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let commands = doc.commands();
        let mut wrong = Vec::new();
        for (id, states) in [
            ("drawing.transform", &["part"][..]),
            ("drawing.format", &["part"][..]),
            ("drawing.text", &["part"][..]),
            ("drawing.reconnect", &["part"][..]),
            ("drawing.chart", &["part"][..]),
            ("insert.pivotTable", &["part", "selection"][..]),
            ("insert.recommendedPivotTables", &["part", "selection"][..]),
            ("table.summarizePivot", &["part", "selection"][..]),
            ("file.new", &[][..]),
            ("file.open", &[][..]),
            ("file.save", &["part"][..]),
            ("file.saveAs", &["part"][..]),
            ("file.saveBytes", &["part"][..]),
            ("file.exportPdf", &["part"][..]),
            ("file.print", &["part"][..]),
            ("file.export", &["part", "selection"][..]),
        ] {
            let command = commands.iter().find(|command| command.id == id).unwrap();
            let actual: BTreeSet<_> = command.reads.iter().map(|state| state.as_str()).collect();
            let wanted: BTreeSet<_> = states.iter().copied().collect();
            if actual != wanted {
                wrong.push(format!("{id}: declares {actual:?}, independently reviewed paths read {wanted:?}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    fn command_ids(source: &str) -> BTreeSet<&str> {
        source
            .split("cmd!(")
            .skip(1)
            .filter_map(|part| {
                let part = part.trim_start();
                if part.starts_with("query ") {
                    return None;
                }
                part.split('"').nth(1)
            })
            .collect()
    }

    #[test]
    fn blind_rows_cover_every_owned_write_and_query_marked_watch_mutation() {
        let mut expected = BTreeSet::new();
        for source in [
            include_str!("../../../../crates/engine/engine/src/cmd/extra.rs"),
            include_str!("../../../../crates/engine/engine/src/cmd/draw.rs"),
            include_str!("../../../../crates/engine/engine/src/cmd/ink.rs"),
            include_str!("../../../../crates/engine/engine/src/cmd/drawing.rs"),
        ] {
            expected.extend(command_ids(source));
        }
        expected.insert("formulas.watchWindow");
        let mut observed = BTreeSet::new();
        let mut unique_paths = BTreeSet::new();
        let mut rows = 0;
        for line in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
            let (command, rest) = line[4..].split_once(" | ").unwrap();
            observed.insert(command);
            assert!(unique_paths.insert(line), "duplicate row: {line}");
            for field in ["params=", "resolves=", "holds=", "reads=", "writes=", "note="] {
                assert!(rest.split(" | ").any(|part| part.starts_with(field)), "{command}: missing {field}");
            }
            rows += 1;
        }
        assert_eq!(observed, expected, "the blind reading must cover every owned registered write");
        eprintln!("blind object reading: {} commands, {rows} code-path rows", observed.len());
    }

    #[test]
    fn declared_object_editor_state_matches_blind_union_with_decreasing_floor() {
        use kition_kit::KitDocument;
        use kition_sheet_kit::SheetDocument;
        use std::collections::BTreeMap;
        let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        let mut expected: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for row in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
            let mut fields = row.split(" | ");
            let id = fields.next().unwrap().trim_start_matches("ROW ");
            let states = fields.find_map(|field| field.strip_prefix("reads=")).unwrap();
            let states = states.split(',').filter_map(|state| match state {
                "part" => Some("part"),
                "selection" | "active" => Some("selection"),
                "mode" | "draw_color" | "draw_width" => Some("mode"),
                _ => None,
            });
            expected.entry(id).or_default().extend(states);
        }
        for (id, states, _) in REVIEWED_EDITOR.iter().chain(crate::data_insert::REVIEWED_EDITOR.iter()) {
            if expected.contains_key(id) {
                expected.insert(id, states.iter().copied().collect());
            }
        }
        let mut less = BTreeSet::new();
        let mut more = BTreeSet::new();
        let mut total = 0;
        for command in doc.commands().into_iter().filter(|command| command.mutates) {
            let Some(wanted) = expected.get(command.id.as_str()) else { continue };
            let declared: BTreeSet<_> = command.reads.iter().map(|state| state.as_str()).collect();
            total += 1;
            if !wanted.is_subset(&declared) {
                eprintln!("{} missing {:?}", command.id, wanted.difference(&declared).collect::<Vec<_>>());
                less.insert(command.id.clone());
            }
            if !declared.is_subset(wanted) {
                eprintln!("{} extra {:?}", command.id, declared.difference(wanted).collect::<Vec<_>>());
                more.insert(command.id.clone());
            }
        }
        eprintln!("object editor comparison: {total} writes; LESS {less:?}; MORE {more:?}");
        // Initial mismatch floors may only decrease as each discrepancy is repaired.
        assert!(less.is_empty(), "new missing editor reads: {less:?}");
        assert!(more.is_empty(), "new excessive editor reads: {more:?}");
    }

    // These are mechanical projections of the committed blind rows onto stored KDM paths.
    // The unrelated-cell exclusion tests a bounded footprint; it does not claim every
    // possible excess field is detected. Script execution and copying a window legitimately
    // read the whole document, so they have no unrelated-cell exclusion.
    #[test]
    fn object_read_footprint_projection_has_only_decreasing_mismatches() {
        use kition_kit::{KitDocument, Under};
        use kition_sheet_kit::SheetDocument;
        use serde_json::{Value, json};
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
        doc.execute("range.setValues", json!({"range":"A1","values":[["Name","Value","Other"],["One",1,2],["Two",3,4]]})).unwrap();
        doc.execute("drawing.ink", json!({"strokes":[{"points":[[10,100],[40,100]],"color":"#123456","width":2,"tool":"pen"}]})).unwrap();
        for x in [10, 180] {
            doc.execute("drawing.insert", json!({"kind":"shape","frame":{"x":x,"y":10,"w":80,"h":40}})).unwrap();
        }
        doc.execute("insert.chart", json!({"range":"A1:C3","type":"column"})).unwrap();
        doc.execute("insert.table", json!({"range":"A1:C3","name":"Sales","header":true})).unwrap();
        let connector = doc.execute("drawing.connect", json!({"from":2,"to":3})).unwrap()["id"].as_str().unwrap().parse::<u32>().unwrap();
        let mut grouped = Vec::new();
        for x in [400, 600] {
            grouped.push(
                doc.execute("drawing.insert", json!({"kind":"shape","frame":{"x":x,"y":10,"w":80,"h":40}})).unwrap()["id"]
                    .as_str()
                    .unwrap()
                    .parse::<u32>()
                    .unwrap(),
            );
        }
        let group = doc.execute("drawing.group", json!({"ids":grouped})).unwrap()["id"].as_str().unwrap().parse::<u32>().unwrap();
        let reconnect_target = grouped.first().copied().unwrap();
        let reconnect_target_tool = format!("drawing:{reconnect_target}");
        doc.execute("home.insertSheet", json!({"name":"Sheet2"})).unwrap();
        doc.execute("cell.set", json!({"cell":"D12","input":"sentinel"})).unwrap();
        doc.execute("range.setValues", json!({"range":"A1","values":[["Name","Value","Other"],["One",1,2],["Two",3,4]]})).unwrap();
        doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).unwrap();
        doc.execute("selection.set", json!({"range":"B2"})).unwrap();
        doc.version().unwrap();

        fn field_held(under: &Under, object: &str, field: &str) -> bool {
            under.premises.iter().any(|premise| {
                premise.id == object
                    && premise.fields.as_ref().is_none_or(|fields| {
                        fields.iter().any(|held| held == field || field.strip_prefix(held).is_some_and(|suffix| suffix.starts_with('.')))
                    })
            })
        }
        let commands = doc.commands();
        let mut less = BTreeSet::new();
        let mut more = BTreeSet::new();
        let mut unrelated_more = BTreeSet::new();
        let mut total = BTreeSet::new();
        for row in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
            let fields: Vec<_> = row.split(" | ").collect();
            let id = fields[0].trim_start_matches("ROW ");
            if !commands.iter().any(|command| command.id == id && command.mutates) {
                continue;
            }
            total.insert(id);
            let mut params: Value = serde_json::from_str(fields.iter().find_map(|field| field.strip_prefix("params=")).unwrap()).unwrap();
            // Readers used 1 for a representative chart/shape. Fixture ids are ink1,
            // shape2,shape3,chart4; remapping preserves their independently read roles.
            if params.get("chart").is_some() {
                params["chart"] = json!(4);
            }
            let chart_path = id == "drawing.chart";
            if params.get("id").is_some() {
                params["id"] = json!(if chart_path {
                    4
                } else if id == "draw.inkToShape" {
                    1
                } else if id == "drawing.reconnect" {
                    connector
                } else {
                    2
                });
            }
            if params.get("ids").is_some() {
                params["ids"] = json!(if id == "drawing.ungroup" { vec![group] } else { vec![2, 3] });
            }
            if params.get("from").is_some() {
                params["from"] = json!(2);
            }
            if params.get("to").is_some() {
                params["to"] = json!(if id == "drawing.reconnect" { reconnect_target } else { 3 });
            }
            let under = doc.under(id, &params).unwrap_or_else(|error| panic!("{id}: {error}")).unwrap_or_default();
            let mut required: Vec<(&str, &str)> = Vec::new();
            match id {
                "chart.addElement" if params.get("element").and_then(Value::as_str) == Some("title") => {
                    required.push(("drawing:4", "parts.title"));
                }
                "chart.quickLayout" if params.get("layout").and_then(Value::as_u64).is_some_and(|n| n >= 5) => {
                    required.push(("drawing:4", "parts.x_title"));
                }
                "chart.changeColors" | "chart.formatSelection" => required.push(("drawing:4", "parts.series")),
                "chart.selectData" => required.extend([("cell:2!B2", "input"), ("cell:2!A2", "style")]),
                "chart.move" => {
                    if !under.premises.iter().any(|premise| {
                        premise.id == "drawing:4"
                            && premise.positions.as_ref().is_some_and(|positions| positions.iter().any(|field| field == "frame"))
                    }) {
                        less.insert(id);
                        eprintln!("LESS {id} params={params}: retained frame not held");
                    }
                }
                "chart.shapeFill" if params.get("kind").and_then(Value::as_str) == Some("shape") => {
                    // same-field decision: hold the replaced fill only.
                    let tool = if params.get("id").is_some() { "drawing:2" } else { "drawing:3" };
                    required.push((tool, "look.fill"));
                }
                "chart.shapeFill" => required.push(("drawing:4", "parts.series")),
                "chart.shapeOutline" => {
                    // same-field decision: hold the replaced line only.
                    required.push(("drawing:2", "look.line"));
                }
                "draw.inkToShape" => {
                    required.extend([("drawing:1", "text"), ("drawing:1", "look.kind"), ("drawing:1", "look.line"), ("drawing:1", "look.fill")])
                }
                "drawing.ink" => required.push(("sheet:1", "props.protection")),
                "drawing.text" => required.extend([("drawing:2", "text"), ("drawing:2", "look.text_body"), ("drawing:2", "look.kind")]),
                "drawing.format" => required.push(("drawing:2", "look.kind")),
                "drawing.connect" => {
                    required.extend([("drawing:2", "look.preset"), ("drawing:3", "look.geom"), ("workbook", "~nextObjectId")]);
                }
                "drawing.group" => required.push(("workbook", "~nextObjectId")),
                "drawing.reconnect" if params.get("to").is_some() => {
                    required.extend([(reconnect_target_tool.as_str(), "look.preset"), (reconnect_target_tool.as_str(), "look.geom")]);
                }
                "drawing.chart" => required.extend([("drawing:4", "parts.series"), ("cell:1!B2", "input")]),
                "home.newCellStyle" => required.push(("cell:1!B2", "style")),
                "data.fromTableRange" => required.push(("cell:2!B2", "input")),
                "pageLayout.themeColors" => required.extend([("workbook", "theme.name"), ("workbook", "theme.colors")]),
                "pageLayout.themeFonts" => required.extend([
                    ("workbook", "theme.name"),
                    ("workbook", "theme.colors"),
                    ("workbook", "theme.major_font"),
                    ("workbook", "theme.minor_font"),
                ]),
                _ => {}
            }
            let missing: Vec<_> = required.iter().filter(|(object, field)| !field_held(&under, object, field)).collect();
            if !missing.is_empty() {
                less.insert(id);
                eprintln!("LESS {id} params={params}: {missing:?}");
            }
            if matches!(
                id,
                "arrange.align" | "arrange.group" | "drawing.connect" | "drawing.transform" | "drawing.format" | "drawing.text" | "drawing.chart"
            ) {
                for kind in ["row:", "col:"] {
                    let held = under.premises.iter().any(|premise| {
                        premise.id.as_str().is_some_and(|id| id.starts_with(kind))
                            && premise.fields.as_ref().is_none_or(|fields| fields.iter().any(|field| field == "info" || field == "info.size"))
                    });
                    if !held {
                        less.insert(id);
                        eprintln!("LESS {id} params={params}: coordinate conversion does not hold {kind}info.size");
                    }
                }
            }
            if !matches!(id, "automate.runScript" | "view.newWindow") && field_held(&under, "cell:2!D12", "input") {
                more.insert(id);
                unrelated_more.insert(id);
            }
        }
        eprintln!("object footprint projection: {} commands; LESS {less:?}; MORE {more:?}", total.len());
        let excessive =
            BTreeSet::from(["data.fromTableRange", "data.fromTextCsv", "data.getData", "drawing.insert", "pageLayout.background", "table.resize"]);
        assert!(less.is_empty(), "new missing held facts: {less:?}");
        assert!(unrelated_more.is_subset(&excessive), "new unrelated-cell holds: {unrelated_more:?}");
        let allowed = excessive.union(&BTreeSet::from(["pageLayout.themeColors", "pageLayout.themeFonts"])).copied().collect();
        assert!(more.is_subset(&allowed), "new excessive object facts: {more:?}");
    }
}
