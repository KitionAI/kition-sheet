//! Source evidence for every reviewed editor union. Locations are computed from
//! exact source anchors so the audit cannot keep silently citing stale lines.
use std::collections::{BTreeMap, BTreeSet};

struct Evidence {
    commands: &'static str,
    states: &'static str,
    file: &'static str,
    anchors: &'static [&'static str],
    reason: &'static str,
}

const EDITOR: &[Evidence] = &[
    Evidence {
        commands: "command.runLine",
        states: "part,selection,mode",
        file: "binding",
        anchors: &["if command == \"commandLine\""],
        reason: "binding entry chooses current part/cell and clears editing mode",
    },
    Evidence {
        commands: "file.save,file.saveAs",
        states: "part",
        file: "io",
        anchors: &["pub fn save_bytes", "wb.active_sheet"],
        reason: "CSV/TSV/HTML formats encode the active sheet; XLSX does not add that default",
    },
    Evidence {
        commands: "format.drawBorder",
        states: "part,selection",
        file: "extra",
        anchors: &["fn draw_border", "s.execute(\"format.borders\""],
        reason: "delegates target_sheet/target_ranges, including omitted range",
    },
    Evidence {
        commands: "data.fromTextCsv,data.getData",
        states: "part,selection",
        file: "extra",
        anchors: &["fn from_csv", "s.execute(\"sheet.insert\""],
        reason: "sheet insertion remembers current selection and defaults insertion edge from active tab",
    },
    Evidence {
        commands: "page.background",
        states: "part",
        file: "extra",
        anchors: &["fn background", "insert_picture_at(s, &params, CellRef::new(0, 0))"],
        reason: "fixed A1 bypasses picture's selection default; clear chooses active sheet",
    },
    Evidence {
        commands: "spelling.change",
        states: "part",
        file: "spelling",
        anchors: &["fn change", "\"matchCase\": true, \"all\": true", "\"cell\": c.a1()"],
        reason: "both delegated defaults are unreachable; restores blind part-only meaning",
    },
    Evidence {
        commands: "edit.clearAll,edit.clearFormats,edit.clearContents,edit.clearComments,edit.clearHyperlinks",
        states: "part,selection",
        file: "edit",
        anchors: &["fn clear(", "let sheet = target_sheet(s, p)?;", "let ranges = target_ranges(s, p)?;"],
        reason: "omitted sheet and ranges select active part and ranges",
    },
    Evidence {
        commands: "edit.fillDown,edit.fillRight,edit.fillUp,edit.fillLeft",
        states: "part,selection",
        file: "edit",
        anchors: &["fn fill_dir", "target_range(s, p)?"],
        reason: "directional fills include selection default",
    },
    Evidence {
        commands: "edit.fillSeries",
        states: "part,selection",
        file: "edit",
        anchors: &["fn fill_series", "target_range(s, p)?"],
        reason: "series defaults range on active part",
    },
    Evidence {
        commands: "edit.smartFill",
        states: "part,selection",
        file: "edit",
        anchors: &["fn smart_fill", "target_range(s, p)?"],
        reason: "smart fill defaults range on active part",
    },
    Evidence {
        commands: "format.fontName,format.fontSize,format.color,format.fillColor,format.alignTop,format.alignMiddle,format.alignBottom,format.increaseIndent,format.decreaseIndent,format.orientation,format.numberFormat,format.accounting,format.percent,format.comma,style.apply",
        states: "part,selection",
        file: "format",
        anchors: &["pub(crate) fn apply_style", "let sheet = target_sheet(s, p)?;", "let ranges = target_ranges(s, p)?;"],
        reason: "all delegate apply_style; its defaults include part and selection",
    },
    Evidence {
        commands: "format.borders",
        states: "part,selection",
        file: "format",
        anchors: &["fn borders", "target_ranges(s, p)?"],
        reason: "borders has the same omitted targets",
    },
    Evidence {
        commands: "cell.unmerge",
        states: "part,selection",
        file: "format",
        anchors: &["fn merge(", "target_ranges(s, p)?"],
        reason: "unmerge selects target ranges even though it skips content/fitting",
    },
    Evidence {
        commands: "column.setWidth,row.autofitHeight,column.autofitWidth,row.hide,column.hide,row.show,column.show",
        states: "part,selection",
        file: "format",
        anchors: &["pub fn line_targets", "s.doc()?.selection.ranges.iter()", "fn line_sheet", "Ok(s.doc()?.wb.active_sheet)"],
        reason: "line helpers default axes and sheet from editor",
    },
    Evidence {
        commands: "column.insert,row.delete,column.delete",
        states: "part,selection",
        file: "sheet",
        anchors: &["fn line_blocks", "s.doc()?.selection.ranges"],
        reason: "line block defaults and target_sheet select editor axes/part",
    },
    Evidence {
        commands: "cell.insert,cell.delete",
        states: "part,selection",
        file: "sheet",
        anchors: &["fn insert_cells", "fn delete_cells", "target_range(s, p)?"],
        reason: "cell-band defaults select current range and active part",
    },
    Evidence {
        commands: "sheet.rename,sheet.tabColor",
        states: "part",
        file: "sheet",
        anchors: &["fn rename_sheet", "fn tab_color", "target_sheet(s, p)?"],
        reason: "default sheet is active; neither reads a selected range",
    },
    Evidence {
        commands: "sheet.delete",
        states: "part",
        file: "sheet",
        anchors: &["fn delete_sheet", "let was_active = d.wb.active_sheet == i;", "d.sheet_selections"],
        reason: "part chooses default/deletion follow-up; restored selection is remembered, not the current selection",
    },
    Evidence {
        commands: "edit.undo,edit.redo",
        states: "selection",
        file: "edit",
        anchors: &["fn step_history", "fn step_once", "selection: d.selection.clone()"],
        reason: "history chooses the workbook snapshot; current selection is carried to the opposite history entry",
    },
    Evidence {
        commands: "table.bandedRows,table.bandedColumns,table.headerRow,table.firstColumn,table.lastColumn,table.style,table.rename,table.convertToRange,table.totalFunction",
        states: "part,selection",
        file: "insert",
        anchors: &["fn find_table", "let sheet = d.wb.active_sheet;", "t.range.contains(d.selection.active)"],
        reason: "omitted table chooses containment on active part; header-off also relocates selected header cell",
    },
    Evidence {
        commands: "shape.insert",
        states: "part,selection",
        file: "insert",
        anchors: &["fn insert_shape", "cell_param(p, \"at\").unwrap_or(d.selection.active)"],
        reason: "implicit anchor selects active cell; insertion chooses active part",
    },
    Evidence {
        commands: "note.add,comment.reply,comment.resolve,note.show",
        states: "part,selection",
        file: "insert",
        anchors: &[
            "fn comment",
            "fn reply_comment",
            "fn resolve_comment",
            "fn show_note",
            "cell_param(p, \"cell\").unwrap_or(s.doc()?.selection.active)",
        ],
        reason: "comment helpers share optional cell and target_sheet defaults",
    },
    Evidence {
        commands: "conditionalFormat.clearRules",
        states: "part,selection",
        file: "insert",
        anchors: &["fn clear_cf", "let ranges = target_ranges(s, p)?;"],
        reason: "range default is read before whole-sheet branch",
    },
    Evidence {
        commands: "data.ungroup,data.hideDetail",
        states: "part,selection",
        file: "data",
        anchors: &["pub fn outline_lines", "let r = s.doc()?.selection.current();", "let si = cx.sheet_index();"],
        reason: "omitted axes choose selection; group/detail choose active part",
    },
    Evidence {
        commands: "data.subtotal",
        states: "part,selection",
        file: "data",
        anchors: &["fn subtotal", "let r = data_range(s, p)?;", "fn data_range"],
        reason: "data range includes selection/current-region fallback on active part",
    },
    Evidence {
        commands: "data.fromTableRange",
        states: "part,selection",
        file: "extra",
        anchors: &["fn from_table_range", "target_range(s, p)?", "fn from_csv"],
        reason: "source target defaults and delegated new-sheet selection memory",
    },
    Evidence {
        commands: "pivot.insert,pivot.recommended,table.summarizePivot",
        states: "part,selection",
        file: "pivot",
        anchors: &["fn default_source", "fn source_param", "fn summarize_table", "fn insert_pivot", "fn recommended"],
        reason: "source/table defaults use active part/cell; no mode input",
    },
    Evidence {
        commands: "pivot.removeField,pivot.moveField,pivot.valueSettings,pivot.filter,pivot.sort,pivot.group,pivot.collapse,pivot.layout,pivot.grandTotals,pivot.subtotals,pivot.reportLayout,pivot.style,pivot.refresh,pivot.changeSource,pivot.delete",
        states: "part,selection",
        file: "pivot",
        anchors: &["fn find_pivot", "let active = d.wb.active_sheet;", "d.selection.active"],
        reason: "active-first lookup then active-cell report/default last pivot; changeSource additionally qualifies bare source on active part",
    },
    Evidence {
        commands: "data.goalSeek",
        states: "part",
        file: "whatif",
        anchors: &["fn cell_arg", "fn goal_seek", "wb.active_sheet"],
        reason: "required cell arguments qualify bare addresses on active part; no selection default",
    },
    Evidence {
        commands: "view.newWindow",
        states: "part,selection",
        file: "extra",
        anchors: &["fn new_window", "DocState::new((*d.wb).clone()", "copy.selection = d.selection.clone()"],
        reason: "newly activated copy keeps source active part, as demonstrated by projection_audit",
    },
];

fn source(file: &str) -> &'static str {
    match file {
        "binding" => include_str!("../../src/lib.rs"),
        "io" => include_str!("../../../../crates/engine/engine/src/io.rs"),
        "extra" => include_str!("../../../../crates/engine/engine/src/cmd/extra.rs"),
        "spelling" => include_str!("../../../../crates/engine/engine/src/cmd/spelling.rs"),
        "edit" => include_str!("../../../../crates/engine/engine/src/cmd/edit.rs"),
        "format" => include_str!("../../../../crates/engine/engine/src/cmd/format.rs"),
        "sheet" => include_str!("../../../../crates/engine/engine/src/cmd/sheet.rs"),
        "insert" => include_str!("../../../../crates/engine/engine/src/cmd/insert.rs"),
        "data" => include_str!("../../../../crates/engine/engine/src/cmd/data.rs"),
        "pivot" => include_str!("../../../../crates/engine/engine/src/cmd/pivot.rs"),
        "whatif" => include_str!("../../../../crates/engine/engine/src/cmd/whatif.rs"),
        _ => panic!("unknown evidence source {file}"),
    }
}

#[test]
fn every_reviewed_editor_union_has_handler_evidence() {
    let mut inventory = BTreeMap::<&str, BTreeSet<&str>>::new();
    for row in super::REVIEWED.lines().filter_map(|row| row.strip_prefix("ROW ")) {
        let fields: Vec<_> = row.split(" | ").collect();
        inventory.insert(fields[0], fields[4].strip_prefix("reads=").unwrap().split(',').collect());
    }
    for row in super::edit_format::READINGS.lines().filter_map(|row| row.strip_prefix("ROW ")) {
        let command = row.split(" | ").next().unwrap();
        if let Some((states, _)) = super::edit_format::reviewed_editor(command) {
            inventory.insert(command, states.iter().copied().collect());
        }
    }
    for (id, states, _) in super::data_insert::REVIEWED_EDITOR.iter().chain(super::objects::REVIEWED_EDITOR) {
        inventory.insert(id, states.iter().copied().collect());
    }
    assert_eq!(inventory.len(), 92);
    let mut audited = BTreeSet::new();
    for evidence in EDITOR {
        let text = source(evidence.file);
        let sites: Vec<_> = evidence
            .anchors
            .iter()
            .map(|anchor| {
                let offset = text.find(anchor).unwrap_or_else(|| panic!("{}: missing source anchor {anchor}", evidence.file));
                format!("{}:{}", evidence.file, text[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1)
            })
            .collect();
        for command in evidence.commands.split(',') {
            assert!(audited.insert(command), "duplicate audit {command}");
            let states: BTreeSet<_> = evidence.states.split(',').collect();
            assert_eq!(inventory.get(command), Some(&states), "{command}: source audit and reviewed union disagree");
            eprintln!("EDITOR_AUDIT {command}: {states:?}; evidence={sites:?}; {}", evidence.reason);
        }
    }
    assert_eq!(audited, inventory.keys().copied().collect());
}

#[test]
fn every_reviewed_chart_projection_has_branch_evidence() {
    for (command, params, fields, reason) in super::objects::REVIEWED_CHART_FIELDS {
        let params: serde_json::Value = serde_json::from_str(params).unwrap();
        let (file, anchors): (&str, &[&str]) = match *command {
            "chart.addElement" => match params["element"].as_str().unwrap() {
                "title" => ("extra", &["fn add_element", "c.title.clone().filter"]),
                "legend" => ("extra", &["fn add_element", "c.legend = if !on"]),
                "dataLabels" => ("extra", &["fn add_element", "\"dataLabels\" => c.data_labels = on"]),
                "gridlines" => ("extra", &["fn add_element", "\"gridlines\" => c.gridlines = on"]),
                "axisTitles" => ("extra", &["fn add_element", "c.x_title.clone().unwrap_or_else", "c.y_title.clone().unwrap_or_else"]),
                "smooth" => ("extra", &["fn add_element", "for s in &mut c.series", "s.smooth = on"]),
                _ => panic!("unaudited chart element"),
            },
            "chart.quickLayout" => ("extra", &["fn quick_layout", "c.x_title = None;", "c.y_title = None;"]),
            "chart.changeType" => {
                ("insert", &["fn chart_set", "c.kind = chart_kind", "fn totals_param", "None | Some(Json::Null) => return Ok(None)"])
            }
            "chart.formatSelection" | "chart.shapeFill" => ("extra", &["fn format_series", "c.series.get_mut(idx)"]),
            "chart.changeColors" => ("extra", &["fn change_colors", "for (i, se) in c.series.iter_mut().enumerate()"]),
            "chart.selectData" => ("extra", &["fn select_data", "c.series = series;", "c.source = Some(source);", "c.by_rows = by_rows;"]),
            "chart.switchRowColumn" => ("insert", &["fn chart_switch", "c.source.clone().map", "series_block(&c.series)"]),
            "chart.set" => ("insert", &["fn chart_set", "c.title = t.as_str().map(str::to_string);"]),
            "chart.move" => ("extra", &["fn move_chart", "cx.sheet_mut(si)?.charts.remove(ci)", "cx.sheet_mut(dest)?.charts.push(chart)"]),
            _ => panic!("unaudited chart projection {command}"),
        };
        let text = source(file);
        let sites: Vec<_> = anchors
            .iter()
            .map(|anchor| {
                let offset = text.find(anchor).unwrap_or_else(|| panic!("{file}: missing {anchor}"));
                format!("{file}:{}", text[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1)
            })
            .collect();
        eprintln!("CHART_AUDIT {command} {params}: inputs={fields:?}; evidence={sites:?}; {reason}");
    }
    assert_eq!(super::objects::REVIEWED_CHART_FIELDS.len(), 15);
    let text = source("extra");
    for anchor in ["fn shape_fill", "|sh| sh.fill = color", "fn shape_outline", "|sh| sh.line = color", "fn with_shape", "None => shapes.last_mut()"]
    {
        let offset = text.find(anchor).unwrap_or_else(|| panic!("missing shape branch {anchor}"));
        eprintln!(
            "READ_AUDIT same-field decision shape-color destination: extra:{} {anchor}",
            text[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1
        );
    }
}

#[test]
fn theme_projection_holds_inputs_and_replaced_fields() {
    use kition_kit::KitDocument;
    use serde_json::json;
    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let mut excessive = BTreeSet::new();
    for (command, params, expected) in [
        // same-field decision: both theme delegates assign name/colors.
        ("theme.colors", json!({}), vec!["theme.colors", "theme.name"]),
        ("theme.colors", json!({"name":"Classic"}), vec!["theme.colors", "theme.name"]),
        ("theme.fonts", json!({"major":"Kition Mono"}), vec!["theme.colors", "theme.name", "theme.major_font"]),
    ] {
        let under = doc.under(command, &params).unwrap().unwrap();
        let fields: BTreeSet<_> = under
            .premises
            .iter()
            .filter(|premise| premise.id == "workbook")
            .flat_map(|premise| premise.fields.as_ref().expect("theme reads must be bounded"))
            .map(String::as_str)
            .collect();
        let wanted: BTreeSet<_> = expected.into_iter().collect();
        assert!(
            wanted.iter().all(|field| fields.iter().any(|held| field == held || field.strip_prefix(held).is_some_and(|tail| tail.starts_with('.')))),
            "{command}: missing actual inputs"
        );
        if fields.iter().any(|held| !wanted.contains(held)) {
            excessive.insert(command);
            eprintln!("THEME_FACT_MORE {command} {params}: wanted={wanted:?} actual={fields:?}");
        }
    }
    assert!(excessive.is_empty(), "unexpected theme holds: {excessive:?}");
    for (file, anchors) in [
        ("extra", vec!["fn theme_colors", "params.get(\"name\").is_none()", "fn theme_fonts", "s.doc()?.wb.theme.colors.iter()"]),
        ("view", vec!["fn theme(", "cx.wb.theme.colors = colors;", "cx.wb.theme.major_font = m.clone();"]),
    ] {
        let text = if file == "view" { include_str!("../../../../crates/engine/engine/src/cmd/view.rs") } else { source(file) };
        for anchor in anchors {
            let offset = text.find(anchor).unwrap_or_else(|| panic!("{file}: missing {anchor}"));
            eprintln!("READ_AUDIT theme delegates {file}:{} {anchor}", text[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1);
        }
    }
}
