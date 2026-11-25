//! Blind engine reading, committed before opening the binding declarations.
//! Inline same-field decision notes identify later replacement-hold amendments.
//! `root` is the workbook; `sheet` is the resolved sheet. Braces name stored
//! fields; `@` means the address/order window, not all nested object contents.
//! Session facts are recorded even when they are outside the SDK editor enum.

pub const READINGS: &str = r##"
ROW file.new | params={} | resolves=sample selects a compiled workbook, no old document target | holds=none | reads=session.documents | writes=adds and activates a workbook | note=file.rs:34; engine lifecycle, not a write to the existing workbook
ROW file.open | params={"name":"Book.xlsx","base64":""} | resolves=bytes by name format; path matches already-open documents before reading | holds=none | reads=session.documents,session.dirty,session.history | writes=adds document or activates existing; can close untouched blank | note=file.rs:46; blank replacement checks cells of every sheet
ROW file.save | params={"path":"out.xlsx"} | resolves=path else active document path | holds=whole workbook | reads=session.path | writes=external file and saved baseline for xlsx/json | note=file.rs:94; no path queues save dialog
ROW file.saveAs | params={"path":"out.xlsx"} | resolves=required path else dialog | holds=whole workbook | reads=none | writes=external file and saved baseline for xlsx/json | note=file.rs:116; delegates save
ROW file.close | params={"force":true} | resolves=active document | holds=none | reads=session.dirty,session.documents | writes=closes document; dirty without force opens dialog | note=file.rs:134
ROW file.exportCsv | params={"path":"out.csv","sheet":"Sheet1"} | resolves=sheet by name or zero-based index else active; copied workbook active index only | holds=sheet cells/styles/features/geometry/theme and calculated values | reads=part | writes=external CSV only | note=file.rs:147; numeric sheet is an index, never a sheet id
ROW file.exportHtml | params={"path":"out.html"} | resolves=sheet by name/index else active despite schema summary | holds=sheet cells/styles/features/geometry/theme and calculated values | reads=part | writes=external HTML only | note=file.rs:147
ROW file.properties | params={"title":"New"} | resolves=workbook property keys; no object lookup | holds=root{props.properties.title} | reads=none | writes=only supplied string properties; returns all properties | note=file.rs:206; return additionally reads every property
ROW file.properties | params={} | resolves=workbook | holds=root{props.properties} | reads=none | writes=no properties; returns all properties | note=file.rs:206; read-only parameter branch
ROW window.activate | params={"index":0} | resolves=index is zero-based session document position | holds=none | reads=session.documents | writes=active document | note=file.rs:225; not sheet position
ROW view.freezePanes | params={} | resolves=active sheet; missing cell checks current freeze before reading selection.active | holds=sheet{props.freeze,~freezeOrigin};row/column address windows at active cell and top; replaced values: pane counts and effective frozen origin, including explicit top | reads=part,selection | writes=unfreeze if already frozen else freeze counts and view top-left | note=view.rs:172; active cell read only on freeze path; same-field decision
ROW view.freezePanes | params={"cell":"Sheet2!C5","top":"B2"} | resolves=cell and top are A1 coordinates with sheet prefixes discarded; active sheet always | holds=sheet{props.freeze,~freezeOrigin};row/column address windows at C5 and B2; replaced values: pane counts and effective frozen origin, including explicit top | reads=part | writes=freeze counts and view top-left | note=view.rs:172; explicit cell bypasses toggle; same-field decision
ROW view.freezeTopRow | params={"top":"A11"} | resolves=active sheet; top coordinates ignore sheet prefix | holds=sheet{props.freeze,~freezeOrigin};row/column address windows at top; replaced values: pane counts and effective frozen origin, including explicit top | reads=part | writes=freeze=(1,0) and view top-left | note=view.rs:23,209; no active cell read; same-field decision
ROW view.freezeFirstColumn | params={} | resolves=active sheet; top defaults A1 | holds=sheet{props.freeze,~freezeOrigin}; replaced values: pane counts and effective frozen origin, including explicit top | reads=part | writes=freeze=(0,1) and view top-left | note=view.rs:28,209; same-field decision
ROW view.freezeRows | params={} | resolves=active sheet and selection.active; top defaults A1 | holds=sheet{props.freeze,~freezeOrigin};row address window at active cell; replaced values: pane counts and effective frozen origin, including explicit top | reads=part,selection | writes=freeze rows only; zero rows errors | note=view.rs:185; same-field decision
ROW view.freezeRows | params={"cell":"Sheet2!C5"} | resolves=active sheet; sheet prefix in cell ignored | holds=sheet{props.freeze,~freezeOrigin};row address window at row 5; replaced values: pane counts and effective frozen origin, including explicit top | reads=part | writes=freeze=(4,0), view top-left A1 | note=view.rs:185; same-field decision
ROW view.freezeColumns | params={} | resolves=active sheet and selection.active | holds=sheet{props.freeze,~freezeOrigin};column address window at active cell; replaced values: pane counts and effective frozen origin, including explicit top | reads=part,selection | writes=freeze columns only; zero columns errors | note=view.rs:185; same-field decision
ROW view.freezeColumns | params={"cell":"C5"} | resolves=active sheet; cell coordinates only | holds=sheet{props.freeze,~freezeOrigin};column address window at C; replaced values: pane counts and effective frozen origin, including explicit top | reads=part | writes=freeze=(0,2), view top-left A1 | note=view.rs:185; same-field decision
ROW view.unfreezePanes | params={} | resolves=active sheet | holds=sheet{props.freeze,~freezeOrigin}; replaced values: pane counts and effective frozen origin, including explicit top | reads=part | writes=freeze=None; view top-left kept | note=view.rs:198; same-field decision
ROW view.gridlines | params={} | resolves=active sheet even if sheet/range params present | holds=sheet{props.showGridlines} | reads=part | writes=toggle show_gridlines | note=view.rs:222
ROW view.gridlines | params={"on":true} | resolves=active sheet | holds=sheet{props.showGridlines} | reads=part | writes=sets show_gridlines | note=view.rs:222; on omits toggle dependency but writes same property
ROW view.gridlineColor | params={"color":"#123456"} | resolves=active sheet; color literal or auto | holds=sheet{props.gridColor} | reads=part | writes=grid color only | note=view.rs:240
ROW view.focusCell | params={} | resolves=session preferences | holds=none | reads=session.prefs.focus_cell,session.prefs.focus_color | writes=focus preference; color-only preserves on | note=view.rs:262; not saved
ROW view.headings | params={} | resolves=active sheet | holds=sheet{props.showHeadings} | reads=part | writes=toggle or explicit on | note=view.rs:222
ROW view.formulaBar | params={} | resolves=session preferences | holds=none | reads=session.prefs.formula_bar | writes=formula bar preference | note=view.rs:278; not saved
ROW view.showZeros | params={} | resolves=active sheet | holds=sheet{props.showZeros} | reads=part | writes=toggle or explicit on | note=view.rs:222
ROW view.rightToLeft | params={} | resolves=active sheet | holds=sheet{props.rightToLeft} | reads=part | writes=toggle or explicit on | note=view.rs:222
ROW view.zoom | params={"percent":150} | resolves=active sheet | holds=none; replaced values: destination ~viewZoom | reads=part,session.dirty | writes=personal zoom and saved baseline if clean | note=view.rs:283; zoom excluded from KDM; same-field decision
ROW view.zoomToSelection | params={} | resolves=active sheet and selection.current rectangle | holds=selected rows/columns{info.size,info.hidden};sheet{props.defaultRowHeight,props.defaultColWidth};selected row/column address windows; replaced values: destination ~viewZoom | reads=part,selection,session.dirty | writes=personal zoom from span geometry | note=view.rs:298; viewWidth/viewHeight default 1200/700; no cell contents; same-field decision
ROW page.orientation | params={"orientation":"landscape"} | resolves=active sheet; sheet/range ignored | holds=sheet{props.print.orientation} | reads=part | writes=orientation only | note=view.rs:310
ROW page.size | params={"paper":"A4"} | resolves=active sheet | holds=sheet{props.print.paper} | reads=part | writes=paper only; missing paper defaults A4 | note=view.rs:319
ROW page.margins | params={"left":0.5} | resolves=active sheet | holds=sheet{props.print.margins} | reads=part | writes=supplied margins; preset replaces six then supplied keys override | note=view.rs:328; margins share one array register
ROW page.printArea | params={"range":"Sheet2!C3:D4"} | resolves=range parsed after stripping sheet prefix; active sheet always | holds=sheet{props.print.print_area};row/column address windows C3:D4 | reads=part | writes=print area on active sheet | note=view.rs:352; target_sheet never called
ROW page.printArea | params={} | resolves=active sheet and first selection area | holds=sheet{props.print.print_area};selected address windows | reads=part,selection | writes=print area | note=view.rs:352
ROW page.printArea | params={"clear":true} | resolves=active sheet; target_range still parses/defaults before clear branch | holds=sheet{props.print.print_area} | reads=part,selection | writes=clears print area | note=view.rs:352; explicit range avoids selection read even when clearing
ROW page.printTitles | params={"rows":"1:2","cols":"A:B"} | resolves=active sheet; rows/cols direct RangeRef parsing, no sheet lookup | holds=sheet{props.print.title_rows,props.print.title_cols};address windows rows 1:2 and columns A:B | reads=part | writes=both title spans; omitted/invalid span becomes None | note=view.rs:362
ROW page.scaleToFit | params={"width":2} | resolves=active sheet; omitted partner is zero, not old fit dimension | holds=sheet{props.print.fit_to} | reads=part | writes=fit_to=(2,0) | note=view.rs:374; differs from pageSetup preservation branch
ROW page.scaleToFit | params={"scale":90} | resolves=active sheet | holds=sheet{props.print.scale,props.print.fit_to} | reads=part | writes=scale and clears fit_to | note=view.rs:374
ROW page.printGridlines | params={} | resolves=active sheet | holds=sheet{props.print.gridlines} | reads=part | writes=toggle or explicit on | note=view.rs:222
ROW page.printHeadings | params={} | resolves=active sheet | holds=sheet{props.print.headings} | reads=part | writes=toggle or explicit on | note=view.rs:222
ROW page.breaks | params={"cell":"Sheet2!C5"} | resolves=active sheet; ignores cell sheet prefix | holds=sheet{props.print.row_breaks,props.print.col_breaks};row/column address windows C5 | reads=part | writes=insert/remove break at row/column; de-duplicates and sorts | note=view.rs:389; absent insert/remove inserts
ROW page.breaks | params={"reset":true} | resolves=active sheet; still reads selection.active before reset | holds=sheet{props.print.row_breaks,props.print.col_breaks} | reads=part,selection | writes=clears both lists | note=view.rs:389
ROW page.headerFooter | params={"header":"Title"} | resolves=active sheet | holds=sheet{props.print.header} | reads=part | writes=supplied header/footer only | note=view.rs:417
ROW theme.set | params={"name":"Meadow"} | resolves=builtin palette name or twelve literal colors; fonts literals | holds=root{theme} | reads=none | writes=theme name/colors and supplied major/minor fonts | note=view.rs:444; no cell/style read or recalculation/fitting
ROW page.pageSetup | params={"sheet":"Sheet2","orientation":"landscape"} | resolves=sheet by name or zero-based index, range/cell prefix fallback, then active | holds=sheet{props.print.orientation} | reads=part | writes=supplied print properties; answer reads all print settings | note=print.rs:1486; name resolution holds sheet name, index holds tab order
ROW page.pageSetup | params={"fitWidth":2} | resolves=active sheet | holds=sheet{props.print.fit_to} | reads=part | writes=fit width; omitted height taken from old fit_to or 1 | note=print.rs:1592; unlike scaleToFit
ROW page.pageSetup | params={"printArea":"C3:D4","titleRows":"1:2","rowBreaks":[5]} | resolves=active sheet; strips prefix from printArea/title spans; rowBreaks are one-based | holds=sheet{props.print.print_area,props.print.title_rows,props.print.row_breaks};corresponding address windows | reads=part | writes=area/titles/break lists; invalid break entries filtered | note=print.rs:1518-1575
ROW sheet.protect | params={"sheet":"Sheet2","password":"p"} | resolves=sheet by name/index or range/cell prefix else active | holds=sheet{props.protection} | reads=part | writes=replaces protection with explicit defaults; no old password check | note=review.rs:26
ROW sheet.unprotect | params={"sheet":1,"password":"p"} | resolves=sheet zero-based index; name/prefix also supported | holds=sheet{props.protection} | reads=part | writes=clears protection after old hash password check | note=review.rs:49
ROW workbook.protect | params={} | resolves=workbook | holds=root{props.protectedStructure} | reads=none | writes=toggle protected_structure | note=review.rs:63; explicit on avoids toggle read
ROW spelling.addToDictionary | params={"word":"Kition"} | resolves=session dictionary case-insensitive membership | holds=none | reads=session.prefs.user_dictionary | writes=preference only | note=spelling.rs:159
ROW spelling.change | params={"cell":"Sheet2!C3","word":"teh","to":"the"} | resolves=cell prefix discarded; reads and writes active sheet C3, ignoring sheet parameter | holds=active C3{input,text,style,array};destination constraints/protection/inherited style;fitted row inputs;translated formula sources if replacement parses formula;C3 address windows | reads=part | writes=cell.set with replaced input, can parse formula or number | note=spelling.rs:167; small local parameter-resolution defect candidate
ROW spelling.change | params={"word":"teh","to":"the","all":true} | resolves=edit.replace with matchCase=true/all=true and no sheet/range passed; active sheet used range | holds=active used range cell inputs/styles and replacement destinations/constraints/fitted rows;formula sources if parsed | reads=part | writes=all matching active-sheet entries via edit.replace | note=spelling.rs:170; cell parameter ignored on all path
ROW file.exportPdf | params={} | resolves=active/all sheets and range on active sheet; build_pdf reads all render inputs | holds=whole rendered sheets,print settings,styles,theme,formula values,objects,geometry | reads=part,session.path | writes=external PDF or bytes only | note=print.rs:1431
ROW file.print | params={} | resolves=same as exportPdf; no workbook mutation | holds=whole rendered sheets,print settings,styles,theme,formula values,objects,geometry | reads=part,session.path | writes=PDF bytes and optional external file | note=print.rs:1442
"##;

#[test]
fn blind_view_rows_have_required_sections() {
    let mut count = 0;
    for line in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        for field in ["params=", "resolves=", "holds=", "reads=", "writes=", "note="] {
            assert!(line.contains(field), "missing {field}: {line}");
        }
        count += 1;
    }
    assert_eq!(count, 55);
}

/// Projection of the blind rows' explicit stored-property sets. Address facts
/// remain covered by the outside-writer tests; response-only reads are excluded.
#[test]
fn stored_view_property_mismatches_only_decrease() {
    use kition_kit::KitDocument;
    use std::collections::BTreeSet;
    let mut doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    let values: Vec<_> = (1..=12).map(|row| serde_json::json!([row, row * 2, row * 3, row * 4])).collect();
    doc.execute("range.setValues", serde_json::json!({"range":"A1","values":values})).expect("cells");
    doc.execute("sheet.insert", serde_json::json!({"name":"Sheet2","before":9})).expect("second sheet");
    doc.execute("range.setValues", serde_json::json!({"range":"A1","values":values})).expect("second sheet cells");
    doc.execute("sheet.activate", serde_json::json!({"sheet":"Sheet1"})).expect("active sheet");
    doc.execute("select.set", serde_json::json!({"range":"C5"})).expect("selection");
    doc.version().expect("materialize observed identities");
    let (mut less, mut more, mut compared) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for row in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let parts: Vec<_> = row.split(" | ").collect();
        let command = parts[0].trim_start_matches("ROW ");
        let params: serde_json::Value = serde_json::from_str(parts[1].trim_start_matches("params=")).expect("params");
        if command == "file.properties" && params.as_object().is_some_and(|p| p.is_empty()) {
            continue;
        }
        let holds = parts[3].trim_start_matches("holds=");
        let (prefix, id) = if holds.starts_with("root{") {
            ("root{", "workbook")
        } else if holds.starts_with("sheet{") {
            // Pinned after reviewing binding dispatch: a qualified range/cell selects its
            // sheet before these active-sheet engine handlers run. Raw rows remain above.
            let second = !command.starts_with("view.")
                && (params.get("sheet") == Some(&serde_json::json!(1))
                    || params.get("sheet") == Some(&serde_json::json!("Sheet2"))
                    || ["range", "cell"]
                        .iter()
                        .any(|key| params.get(key).and_then(serde_json::Value::as_str).is_some_and(|r| r.starts_with("Sheet2!"))));
            ("sheet{", if second { "sheet:2" } else { "sheet:1" })
        } else {
            continue;
        };
        compared.insert(command.to_owned());
        let mut expected: BTreeSet<String> = holds
            .trim_start_matches(prefix)
            .split('}')
            .next()
            .expect("fields")
            .split(',')
            .map(|f| f.replace("props.properties", "props.docProps"))
            .collect();
        if id.starts_with("sheet:")
            && !command.starts_with("view.")
            && (params.get("sheet").is_some_and(serde_json::Value::is_string)
                || ["range", "cell"].iter().any(|key| params.get(key).and_then(serde_json::Value::as_str).is_some_and(|r| r.contains('!'))))
        {
            expected.insert("name".into());
        }
        // Pinned after reading cmd::edit's shared commit path: replacing sheet
        // protection also reroutes that sheet's connectors.
        if matches!(command, "sheet.protect" | "sheet.unprotect") {
            expected.insert("~editConnectors".into());
        }
        let under = doc.under(command, &params).expect("under").expect("stored write");
        let mut actual = BTreeSet::new();
        for premise in &under.premises {
            if premise.id == id {
                match &premise.fields {
                    Some(fields) => actual.extend(fields.iter().cloned()),
                    None => {
                        actual.insert("*".into());
                    }
                }
            } else if premise.fields.as_ref().is_none_or(|fields| !fields.is_empty()) {
                more.insert(command.to_owned());
            }
        }
        let missing: Vec<_> = if actual.contains("*") { Vec::new() } else { expected.difference(&actual).collect() };
        let extra: Vec<_> = actual.difference(&expected).collect();
        if !missing.is_empty() {
            less.insert(command.to_owned());
        }
        if !extra.is_empty() {
            more.insert(command.to_owned());
        }
        if !missing.is_empty() || !extra.is_empty() {
            eprintln!("VIEW {command} {params}: LESS={missing:?} MORE={extra:?}");
        }
    }
    eprintln!("view property projection: {}/{} differ, {} less, {} more", less.union(&more).count(), compared.len(), less.len(), more.len());
    assert!(less.is_empty(), "missing properties: {less:?}");
    assert!(more.is_empty(), "new excessive property holds: {more:?}");
}

// Structured mappings below were pinned from the declaration after review of
// schema filtering and binding dispatch. READINGS retains the original rows with
// labelled replacement-hold amendments, including engine-only sheet parameters.
const WRITES: &[&str] = &[
    "file.new",
    "file.open",
    "file.save",
    "file.saveAs",
    "file.close",
    "file.properties",
    "view.freezePanes",
    "view.freezeTopRow",
    "view.freezeFirstColumn",
    "view.freezeRows",
    "view.freezeColumns",
    "view.unfreezePanes",
    "view.gridlines",
    "view.gridlineColor",
    "view.headings",
    "view.showZeros",
    "view.rightToLeft",
    "page.orientation",
    "page.size",
    "page.margins",
    "page.printArea",
    "page.printTitles",
    "page.scaleToFit",
    "page.printGridlines",
    "page.printHeadings",
    "page.breaks",
    "page.headerFooter",
    "theme.set",
    "page.pageSetup",
    "sheet.protect",
    "sheet.unprotect",
    "workbook.protect",
    "spelling.change",
];

#[test]
fn every_view_file_parameter_declaration_matches_reviewed_handler_resolution() {
    use kition_kit::KitDocument;
    use std::collections::BTreeSet;

    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    let commands = doc.commands();
    let registered: BTreeSet<_> = commands.iter().filter(|c| c.mutates && WRITES.contains(&c.id.as_str())).map(|c| c.id.as_str()).collect();
    assert_eq!(registered, WRITES.iter().copied().collect());
    assert_eq!(registered.len(), 33);
    let mut problems = Vec::new();
    for id in registered {
        let command = commands.iter().find(|c| c.id == id).expect("registered");
        let (target, places): (Option<&str>, &[&str]) = match id {
            "view.freezePanes" | "view.freezeRows" | "view.freezeColumns" => (Some("cell"), &["cell", "top"]),
            "view.freezeTopRow" | "view.freezeFirstColumn" => (None, &["top"]),
            "page.printArea" => (Some("range"), &["range"]),
            "page.breaks" => (Some("cell"), &["cell"]),
            // Pinned after review: word is a source-membership predicate on the
            // all-match path, which needs the SDK's place coverage callback.
            "spelling.change" => (Some("cell"), &["cell", "word"]),
            "page.printTitles" => (None, &["cols", "rows"]),
            "page.pageSetup" => {
                // print.rs:1515-1573 interprets these values as coordinates on the
                // resolved sheet, discarding any prefixes inside the setting values.
                (Some("range"), &["sheet", "range", "cell", "printArea", "titleRows", "titleCols", "rowBreaks", "colBreaks"])
            }
            "sheet.protect" | "sheet.unprotect" => (None, &["sheet"]),
            _ => (None, &[]),
        };
        let expected: BTreeSet<_> = places.iter().copied().collect();
        let actual: BTreeSet<_> = command.places.iter().copied().collect();
        if actual != expected {
            problems.push(format!(
                "{id} places LESS={:?} MORE={:?}",
                expected.difference(&actual).collect::<Vec<_>>(),
                actual.difference(&expected).collect::<Vec<_>>()
            ));
        }
        if command.targets != target {
            problems.push(format!("{id} target actual={:?} expected={target:?}", command.targets));
        }
        if !command.objects.is_empty() {
            problems.push(format!("{id} unexpected object-id parameters {:?}", command.objects));
        }
        let sheet_parameter = matches!(id, "page.pageSetup" | "sheet.protect" | "sheet.unprotect");
        let expected_types: &[(&str, &[&str])] = if sheet_parameter { &[("sheet", &["Sheet"])] } else { &[] };
        let actual_types: BTreeSet<_> = command.object_types.iter().map(|(key, types)| (*key, types.to_vec())).collect();
        let wanted_types: BTreeSet<_> = expected_types.iter().map(|(key, types)| (*key, types.to_vec())).collect();
        if actual_types != wanted_types {
            problems.push(format!("{id} object_types actual={actual_types:?} expected={wanted_types:?}"));
        }
        let expected_names: &[(&str, &str)] = if sheet_parameter { &[("sheet", "name")] } else { &[] };
        if command.name_params != expected_names {
            problems.push(format!("{id} name_params actual={:?} expected={expected_names:?}", command.name_params));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_view_file_editor_declaration_matches_the_reviewed_handler_union() {
    use kition_kit::KitDocument;
    use std::collections::{BTreeMap, BTreeSet};

    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    let mut wanted: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for row in READINGS.lines().filter(|line| line.starts_with("ROW ")) {
        let fields: Vec<_> = row.split(" | ").collect();
        let id = fields[0].trim_start_matches("ROW ");
        if WRITES.contains(&id) {
            wanted
                .entry(id)
                .or_default()
                .extend(fields[4].trim_start_matches("reads=").split(',').filter(|state| matches!(*state, "part" | "selection" | "mode")));
        }
    }
    // spelling::change fixes all=true for replace or supplies cell.set's cell.
    wanted.insert("spelling.change", BTreeSet::from(["part"]));
    // Pinned after review: io::save_bytes dispatches CSV/TSV/HTML through the
    // active sheet; the original blind XLSX save rows remain above.
    for command in ["file.save", "file.saveAs"] {
        wanted.insert(command, BTreeSet::from(["part"]));
    }
    let mut mismatches = Vec::new();
    for command in doc.commands().into_iter().filter(|command| command.mutates && WRITES.contains(&command.id.as_str())) {
        let declared: BTreeSet<_> = command.reads.iter().map(|state| state.as_str()).collect();
        let expected = wanted.get(command.id.as_str()).expect("every registered write read");
        if &declared != expected {
            mismatches.push(format!(
                "{} LESS={:?} MORE={:?}",
                command.id,
                expected.difference(&declared).collect::<Vec<_>>(),
                declared.difference(expected).collect::<Vec<_>>()
            ));
        }
    }
    assert_eq!(wanted.len(), 33);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

mod footprint {
    use std::collections::{BTreeMap, BTreeSet};

    use kition_kit::kdm::{ActorId, Replica, tool_ids::ToolIds};
    use kition_kit::session::KitModel;
    use kition_kit::{KitDocument, Under};
    use kition_sheet_engine::core::{CellRef, RangeRef};
    use kition_sheet_kit::SheetDocument;
    use serde_json::{Value, json};

    #[derive(Clone, Debug, Default)]
    struct Fields {
        content: Option<BTreeSet<String>>,
        positions: Option<BTreeSet<String>>,
    }

    #[derive(Default)]
    struct Facts {
        objects: BTreeMap<String, Fields>,
        windows: BTreeSet<(String, String, usize)>,
        covers: BTreeSet<String>,
        whole_document: bool,
    }

    impl Facts {
        fn add(&mut self, id: &str, content: impl IntoIterator<Item = impl Into<String>>, positions: &[&str]) {
            let fields = self.objects.entry(id.into()).or_insert_with(|| Fields { content: Some(BTreeSet::new()), positions: Some(BTreeSet::new()) });
            fields.content.as_mut().expect("bounded content").extend(content.into_iter().map(Into::into));
            fields.positions.as_mut().expect("bounded positions").extend(positions.iter().map(|s| (*s).into()));
        }

        fn address(&mut self, replica: &Replica, sid: &str, field: &str, first: usize, last: usize) {
            let names = ToolIds::of_replica(replica);
            let sheet = names.resolve(sid, |id| replica.exists(id)).expect("fixture sheet");
            let lines = replica.list(&sheet, field).expect("fixture lines");
            assert!(last < lines.len(), "fixture must materialize every expected line");
            for (index, raw) in lines.iter().enumerate().take(last + 1).skip(first) {
                self.windows.insert((sid.into(), field.into(), index));
                self.add(&names.show(raw), Vec::<String>::new(), &["index"]);
            }
        }

        fn whole_document(&mut self, replica: &Replica) {
            self.whole_document = true;
            let names = ToolIds::of_replica(replica);
            for ty in replica.schema().types.keys() {
                for id in replica.objects_of_type(ty) {
                    self.objects.insert(names.show(&id), Fields::default());
                }
            }
        }

        fn actual(under: &Under) -> Self {
            let mut facts = Self::default();
            for premise in &under.premises {
                let id = premise.id.as_str().expect("tool id").to_owned();
                let next = Fields {
                    content: premise.fields.as_ref().map(|items| items.iter().cloned().collect()),
                    positions: premise.positions.as_ref().map(|items| items.iter().cloned().collect()),
                };
                if let Some(previous) = facts.objects.get_mut(&id) {
                    for (previous, next) in [(&mut previous.content, next.content), (&mut previous.positions, next.positions)] {
                        match (previous.as_mut(), next) {
                            (Some(before), Some(after)) => before.extend(after),
                            (_, None) => *previous = None,
                            _ => {}
                        }
                    }
                } else {
                    facts.objects.insert(id, next);
                }
            }
            for object in &under.objects {
                facts.objects.entry(object.as_str().expect("tool id").into()).or_default();
            }
            for window in &under.within {
                assert!(window.time.is_none(), "sheet coordinates are not time spans");
                for index in window.from..window.to {
                    facts.windows.insert((window.parent.as_str().expect("parent tool id").into(), window.field.clone(), index));
                }
            }
            facts.covers = under.covers.as_ref().map(|covers| covers.fields.iter().cloned().collect()).unwrap_or_default();
            facts
        }

        fn differences(&self, actual: &Self) -> (Vec<String>, Vec<String>) {
            let (mut less, mut more) = (Vec::new(), Vec::new());
            for (id, wanted) in &self.objects {
                let Some(held) = actual.objects.get(id) else {
                    less.push(format!("{id}: missing object"));
                    continue;
                };
                for (kind, wanted, held) in [("field", &wanted.content, &held.content), ("position", &wanted.positions, &held.positions)] {
                    match (wanted, held) {
                        (Some(wanted), Some(held)) => {
                            for field in wanted.iter().filter(|field| !held.iter().any(|ancestor| covers(ancestor, field))) {
                                less.push(format!("{id}: missing {kind} {field}"));
                            }
                            for field in held.iter().filter(|field| !wanted.iter().any(|ancestor| covers(ancestor, field))) {
                                more.push(format!("{id}: extra {kind} {field}"));
                            }
                        }
                        (Some(_), None) => more.push(format!("{id}: every {kind}")),
                        (None, Some(_)) => less.push(format!("{id}: not every {kind}")),
                        (None, None) => {}
                    }
                }
            }
            for id in actual.objects.keys().filter(|id| !self.objects.contains_key(*id)) {
                more.push(format!("{id}: unrelated object"));
            }
            for (sid, field, index) in self.windows.difference(&actual.windows) {
                let broad_list = actual
                    .objects
                    .get(sid)
                    .is_some_and(|held| held.content.as_ref().is_none_or(|fields| fields.iter().any(|ancestor| covers(ancestor, field))));
                if !broad_list {
                    less.push(format!("{sid}.{field}[{index}]: missing coordinate window"));
                }
            }
            for window in actual.windows.difference(&self.windows) {
                more.push(format!("{}.{:?}[{}]: unread coordinate window", window.0, window.1, window.2));
            }
            if !actual.whole_document {
                for field in self.covers.difference(&actual.covers) {
                    less.push(format!("coverage: missing predicate {field}"));
                }
            }
            for field in actual.covers.difference(&self.covers) {
                more.push(format!("coverage: unread predicate {field}"));
            }
            (less, more)
        }
    }

    fn covers(ancestor: &str, field: &str) -> bool {
        ancestor == field || field.strip_prefix(ancestor).is_some_and(|tail| tail.starts_with('.') || tail.starts_with('['))
    }

    fn replica(doc: &mut SheetDocument) -> Replica {
        Replica::load(kdm_model::SheetModel::schema(), &doc.snapshot().expect("snapshot"), ActorId::random()).expect("replica")
    }

    fn fixture() -> SheetDocument {
        let mut doc = SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
        let values: Vec<_> = (1..=12).map(|row| json!([row, row * 2, row * 3, row * 4])).collect();
        for index in 0..3 {
            if index != 0 {
                doc.execute("sheet.insert", json!({"name":format!("Sheet{}",index+1),"before":9})).expect("sheet");
            }
            doc.execute("range.setValues", json!({"range":"A1","values":values})).expect("cells");
            doc.execute("cell.set", json!({"cell":"C3","input":"teh"})).expect("spelling source");
        }
        doc.execute("cell.set", json!({"cell":"Z20","input":"unrelated"})).expect("unread sentinel");
        doc.execute("sheet.activate", json!({"sheet":"Sheet1"})).expect("active sheet");
        doc.execute("select.set", json!({"range":"C5"})).expect("selection");
        doc.version().expect("materialized identities");
        doc
    }

    fn sheet_id(command: &str, params: &Value) -> &'static str {
        // Pinned after review: agent dispatch switches non-view commands to the
        // named sheet. View commands keep the editor and ignore the cell prefix.
        if !command.starts_with("view.")
            && (params.get("sheet") == Some(&json!(1))
                || params.get("sheet") == Some(&json!("Sheet2"))
                || ["range", "cell", "rows", "cols"]
                    .iter()
                    .any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|r| r.starts_with("Sheet2!"))))
        {
            "sheet:2"
        } else {
            "sheet:1"
        }
    }

    fn expected(command: &str, params: &Value, holds: &str, model: &Replica) -> Facts {
        let mut facts = Facts::default();
        let sid = sheet_id(command, params);
        if holds == "whole workbook" {
            facts.whole_document(model);
            return facts;
        }
        let property_prefix = if holds.starts_with("root{") {
            Some(("root{", "workbook"))
        } else if holds.starts_with("sheet{") {
            Some(("sheet{", sid))
        } else {
            None
        };
        if let Some((prefix, id)) = property_prefix {
            let fields = holds.trim_start_matches(prefix).split('}').next().expect("fields");
            // Pinned after review: empty file.properties only answers metadata;
            // response-only reads do not become premises for a write.
            let fields: Vec<_> = if command == "file.properties" && params.as_object().is_some_and(|p| p.is_empty()) {
                Vec::new()
            } else {
                fields.split(',').map(|field| field.replace("props.properties", "props.docProps")).collect()
            };
            facts.add(id, fields, &[]);
        }
        if sid.starts_with("sheet:")
            && !command.starts_with("view.")
            && (params.get("sheet").is_some_and(Value::is_string)
                || ["range", "cell", "rows", "cols"].iter().any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|r| r.contains('!'))))
        {
            facts.add(sid, ["name"], &[]);
        }
        if let Some(index) = params.get("sheet").and_then(Value::as_u64) {
            facts.windows.insert(("workbook".into(), "sheets".into(), index as usize));
        }
        match command {
            // Pinned after reading the shared edit tail, separately from the
            // unchanged blind direct-handler rows.
            "sheet.protect" | "sheet.unprotect" => facts.add(sid, ["~editConnectors"], &[]),
            "view.freezePanes" | "view.freezeRows" | "view.freezeColumns" | "view.freezeTopRow" | "view.freezeFirstColumn" => {
                let (row, col) = match params.get("top").and_then(Value::as_str) {
                    Some("B2") => (1, 1),
                    Some("A11") => (10, 0),
                    _ => (0, 0),
                };
                facts.address(model, sid, "rows", row, row);
                facts.address(model, sid, "cols", col, col);
                if matches!(command, "view.freezePanes" | "view.freezeRows") {
                    facts.address(model, sid, "rows", 4, 4);
                }
                if matches!(command, "view.freezePanes" | "view.freezeColumns") {
                    facts.address(model, sid, "cols", 2, 2);
                }
            }
            "page.printArea" if params.get("clear") != Some(&json!(true)) => {
                if params.get("range").is_some() {
                    facts.address(model, sid, "rows", 2, 3);
                    facts.address(model, sid, "cols", 2, 3);
                } else {
                    facts.address(model, sid, "rows", 4, 4);
                    facts.address(model, sid, "cols", 2, 2);
                }
            }
            "page.printTitles" => {
                facts.address(model, sid, "rows", 0, 1);
                facts.address(model, sid, "cols", 0, 1);
            }
            "page.breaks" if params.get("reset") != Some(&json!(true)) => {
                facts.address(model, sid, "rows", 4, 4);
                facts.address(model, sid, "cols", 2, 2);
            }
            "page.pageSetup" if params.get("printArea").is_some() => {
                facts.address(model, sid, "rows", 2, 3);
                facts.address(model, sid, "cols", 2, 3);
                facts.address(model, sid, "rows", 0, 1);
                facts.address(model, sid, "rows", 4, 4);
            }
            "spelling.change" => spelling(&mut facts, model, sid, params.get("all") == Some(&json!(true))),
            _ => {}
        }
        facts
    }

    fn spelling(facts: &mut Facts, model: &Replica, sid: &str, all: bool) {
        let sheet = if sid == "sheet:2" { 2 } else { 1 };
        facts.add(&format!("cell:{sheet}!C3"), ["input", "text", "style", "array", "picture"], &[]);
        // Pinned after reviewing edit.rs:1680-1725 and Sheet::input_text:
        // replace scans every stored input, including picture presence, rather
        // than a used-range rectangle. It bypasses entry's merge/validation
        // checks. The fixture's sole matching entry is C3; table-header discovery
        // remains read through the shared edit tail. Header membership and the
        // column names used for de-duplication are held per matching cell.
        let mut sheet_fields = if all {
            facts.covers.extend(["input".into(), "~spellingInputs".into()]);
            Vec::new()
        } else {
            kdm_model::constraint_fields(RangeRef::cell(CellRef::new(2, 2)))
        };
        sheet_fields.push("~editConnectors".into());
        sheet_fields.extend(["props.protection", "props.defaultRowHeight"].map(str::to_owned));
        facts.add(sid, sheet_fields, &[]);
        facts.add("workbook", ["props.dateSystem", "props.defaultStyle"], &[]);
        facts.address(model, sid, "rows", 2, 2);
        facts.address(model, sid, "cols", 2, 2);
        let names = ToolIds::of_replica(model);
        let sheet = names.resolve(sid, |id| model.exists(id)).expect("sheet");
        let rows = model.list(&sheet, "rows").expect("rows");
        let cols = model.list(&sheet, "cols").expect("cols");
        facts.add(&names.show(&rows[2]), ["info.style", "info.size", "info.custom", "~fit"], &["index"]);
        if all {
            facts.add(&names.show(&rows[2]), ["~spellingHeader"], &[]);
        }
        facts.add(&names.show(&cols[2]), ["info.style"], &["index"]);
    }

    // Pinned from 51e29b7:place.rs after review. This records the baseline
    // declaration algorithm, not an additional blind reading or a production
    // reader for old formats. Property commands were narrow; all other commands
    // in this family fell through to document_premises.
    fn baseline(command: &str, params: &Value, model: &Replica) -> Facts {
        let mut facts = Facts::default();
        let fields: &[&str] = match command {
            "view.gridlines" => &["props.showGridlines"],
            "view.gridlineColor" => &["props.gridColor"],
            "view.headings" => &["props.showHeadings"],
            "view.showZeros" => &["props.showZeros"],
            "view.rightToLeft" => &["props.rightToLeft"],
            "page.orientation" => &["props.print.orientation"],
            "page.size" => &["props.print.paper"],
            "page.margins" => &["props.print.margins"],
            "page.scaleToFit" => &["props.print.scale", "props.print.fit_to"],
            "page.printGridlines" => &["props.print.gridlines"],
            "page.printHeadings" => &["props.print.headings"],
            "sheet.protect" | "sheet.unprotect" => &["props.protection"],
            "page.headerFooter" => {
                facts.add(
                    sheet_id(command, params),
                    ["header", "footer"].into_iter().filter(|key| params.get(*key).is_some()).map(|key| format!("props.print.{key}")),
                    &[],
                );
                return facts;
            }
            "file.properties" => {
                facts.add(
                    "workbook",
                    ["title", "subject", "author", "company", "keywords", "description"]
                        .into_iter()
                        .filter(|key| params.get(*key).is_some())
                        .map(|key| format!("props.docProps.{key}")),
                    &[],
                );
                return facts;
            }
            "workbook.protect" => {
                facts.add("workbook", ["props.protectedStructure"], &[]);
                return facts;
            }
            _ => {
                facts.whole_document(model);
                return facts;
            }
        };
        facts.add(sheet_id(command, params), fields.iter().copied(), &[]);
        facts
    }

    #[test]
    fn every_registered_view_file_write_has_a_gating_premise_comparison() {
        let mut doc = fixture();
        let commands = doc.commands();
        let model = replica(&mut doc);
        let (mut less, mut more, mut compared) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        let (mut baseline_less, mut baseline_more) = (BTreeSet::new(), BTreeSet::new());
        for row in super::READINGS.lines().filter(|line| line.starts_with("ROW ")) {
            let fields: Vec<_> = row.split(" | ").collect();
            let command = fields[0].trim_start_matches("ROW ");
            let declared = commands.iter().find(|c| c.id == command).expect("registered engine handler");
            if !declared.mutates {
                continue;
            }
            compared.insert(command);
            let params: Value = serde_json::from_str(fields[1].trim_start_matches("params=")).expect("params");
            let wanted = expected(command, &params, fields[3].trim_start_matches("holds="), &model);
            let (missing, extra) = wanted.differences(&baseline(command, &params, &model));
            if !missing.is_empty() {
                baseline_less.insert(command);
            }
            if !extra.is_empty() {
                baseline_more.insert(command);
            }
            let under = doc.under(command, &params).unwrap_or_else(|error| panic!("{command} {params}: {error}")).unwrap_or_default();
            let (missing, extra) = wanted.differences(&Facts::actual(&under));
            if !missing.is_empty() {
                less.insert(command);
            }
            if !extra.is_empty() {
                more.insert(command);
            }
            if !missing.is_empty() || !extra.is_empty() {
                eprintln!(
                    "VIEW_FOOTPRINT {command} {params}: LESS={missing:?} MORE({})={:?}",
                    extra.len(),
                    extra.iter().take(12).collect::<Vec<_>>()
                );
            }
        }
        assert_eq!(compared, super::WRITES.iter().copied().collect(), "all 33 writes must be compared");
        let baseline_broad = BTreeSet::from([
            "file.close",
            "file.new",
            "file.open",
            "page.breaks",
            "page.pageSetup",
            "page.printArea",
            "page.printTitles",
            "page.scaleToFit",
            "theme.set",
            "spelling.change",
            "view.freezeColumns",
            "view.freezeFirstColumn",
            "view.freezePanes",
            "view.freezeRows",
            "view.freezeTopRow",
            "view.unfreezePanes",
        ]);
        assert_eq!(
            baseline_less,
            BTreeSet::from(["sheet.protect", "sheet.unprotect"]),
            "the shared edit tail also reads connector inputs for sheet protection"
        );
        assert_eq!(baseline_more, baseline_broad, "baseline 51e29b7 premise replay");
        // Baseline reference metadata omitted the five top coordinates and
        // pageSetup's five coordinate-valued settings. Baseline editor reads
        // omitted spelling's delegated selection and the active sheet used by
        // CSV/TSV/HTML saves, and overdeclared file.new/open.
        // The unrepresented saved freeze origin is a second LESS cause for the
        // same five freeze commands; its proven outside-writer case is separate.
        baseline_less.extend([
            "view.freezeColumns",
            "view.freezeFirstColumn",
            "view.freezePanes",
            "view.freezeRows",
            "view.freezeTopRow",
            "page.pageSetup",
            "spelling.change",
            "file.save",
            "file.saveAs",
        ]);
        // Classes overlap by command. In the missing-read class the five top
        // entries are metadata omissions, already covered by broad stored holds;
        // their wrong-write cause is the separately unrepresented origin default.
        let freeze = ["view.freezeColumns", "view.freezeFirstColumn", "view.freezePanes", "view.freezeRows", "view.freezeTopRow"];
        let broad: Vec<_> = baseline_more.iter().copied().filter(|command| *command != "page.scaleToFit").collect();
        let unheld = [
            "view.freezeColumns",
            "view.freezeFirstColumn",
            "view.freezePanes",
            "view.freezeRows",
            "view.freezeTopRow",
            "sheet.protect",
            "sheet.unprotect",
        ];
        let classes: [(&str, &[&str], &[&str]); 6] = [
            ("parameter resolved differently from its name", &[], &[]),
            ("value naming another object", &["page.pageSetup", "spelling.change"], &[]),
            ("default taken from elsewhere", &freeze, &[]),
            ("two code paths computing one fact differently", &[], &["page.scaleToFit"]),
            ("a read that is not held (or an unread fact held)", &unheld, &broad),
            ("editor state read but not declared", &["spelling.change", "file.save", "file.saveAs"], &["file.new", "file.open"]),
        ];
        let (mut classified_less, mut classified_more) = (BTreeSet::new(), BTreeSet::new());
        for (class, missing, extra) in classes {
            classified_less.extend(missing.iter().copied());
            classified_more.extend(extra.iter().copied());
            eprintln!("VIEW_BASELINE_CLASS {class}: LESS({})={missing:?}; MORE({})={extra:?}", missing.len(), extra.len());
        }
        assert_eq!(classified_less, baseline_less);
        assert_eq!(classified_more, baseline_more);
        eprintln!(
            "VIEW_BASELINE 51e29b7: {}/33 differ; LESS(11)={baseline_less:?}; MORE(16)={baseline_more:?}; includes shared connector reads and the unrepresented freeze origin",
            baseline_less.union(&baseline_more).count()
        );
        // Exact residual sets, never a count-only allowance that can hide a new
        // wrong command as another one improves. Remove entries after fixes.
        let known_less = BTreeSet::new();
        // The lifecycle row for close is specifically its force:true branch.
        let known_more = BTreeSet::new();
        eprintln!("VIEW_FOOTPRINT {}/33 differ; LESS={less:?}; MORE={more:?}", less.union(&more).count());
        assert!(less.is_subset(&known_less), "new missing facts: {less:?}");
        assert!(more.is_subset(&known_more), "new excessive facts: {:?}", more.difference(&known_more).collect::<Vec<_>>());
        assert_eq!(less, known_less, "decrease the missing-fact floor");
        assert_eq!(more, known_more, "decrease the excessive-fact floor");
    }

    #[test]
    fn reviewed_freeze_defaults_hold_saved_origin_and_scroll_geometry() {
        for command in ["view.freezePanes", "view.freezeRows", "view.freezeColumns", "view.freezeTopRow", "view.freezeFirstColumn"] {
            let mut doc = fixture();
            doc.execute("view.freezePanes", json!({"cell":"C5","top":"B2"})).expect("saved origin");
            doc.version().expect("materialized origin");
            let params = if matches!(command, "view.freezeTopRow" | "view.freezeFirstColumn") { json!({}) } else { json!({"cell":"D8"}) };
            let under = doc.under(command, &params).expect("under").expect("write premises");
            let origin = doc.derived(&[json!("sheet:1")], &["freezeOrigin".into()]).expect("derived origin");
            assert!(origin.iter().any(|value| value.id == "sheet:1" && value.values.get("freezeOrigin") == Some(&json!([1, 1]))), "{origin:?}");
            assert!(
                under.premises.iter().any(|p| p.id == "sheet:1" && p.fields.as_ref().is_none_or(|f| f.iter().any(|f| f == "~freezeOrigin"))),
                "{command}: {under:?}"
            );
        }
        let mut doc = fixture();
        let sizes = doc.inspect().expect("geometry")["defaultSize"].clone();
        let width = sizes[0].as_f64().expect("default width");
        let height = sizes[1].as_f64().expect("default height");
        doc.execute("editor.viewport", json!({"right":12,"bottom":12,"x":width*2.0,"y":height*2.0})).expect("scroll");
        let model = replica(&mut doc);
        let mut wanted = Facts::default();
        // same-field decision: explicit/default origins are replacement destinations.
        wanted.add("sheet:1", ["props.freeze", "~freezeOrigin", "props.defaultRowHeight", "props.defaultColWidth"], &[]);
        wanted.address(&model, "sheet:1", "rows", 2, 2);
        wanted.address(&model, "sheet:1", "cols", 2, 2);
        wanted.address(&model, "sheet:1", "rows", 7, 7);
        let names = ToolIds::of_replica(&model);
        let sheet = names.resolve("sheet:1", |id| model.exists(id)).expect("sheet");
        for field in ["rows", "cols"] {
            for line in model.list(&sheet, field).expect("axes").iter().take(3) {
                wanted.add(&names.show(line), ["info.size", "info.hidden"], &[]);
            }
        }
        let under = doc.under("view.freezeRows", &json!({"cell":"C8"})).expect("under").expect("write premises");
        let (less, more) = wanted.differences(&Facts::actual(&under));
        assert!(less.is_empty() && more.is_empty(), "scroll default LESS={less:?}; MORE={more:?}");
    }

    #[test]
    fn lifecycle_calls_that_save_nothing_do_not_hold_old_workbook_contents() {
        let doc = fixture();
        for (command, params) in
            [("file.new", json!({})), ("file.open", json!({"name":"Other.xlsx","base64":""})), ("file.close", json!({"force":true}))]
        {
            let under = doc.under(command, &params).expect("under").unwrap_or_default();
            assert!(under.objects.is_empty() && under.premises.is_empty() && under.within.is_empty(), "{command}: {under:?}");
        }
    }
}
