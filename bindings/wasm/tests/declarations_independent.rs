//! C8-sheet-read: expectations transcribed from the engine before comparison.
//! Family modules are committed blind. Comparison code is added only after
//! those commits, so its review cannot silently rewrite the expectations.

#[path = "declarations_independent/all_commands.rs"]
mod all_commands;
#[path = "declarations_independent/audit.rs"]
mod audit;
#[path = "declarations_independent/binding_entry.rs"]
mod binding_entry;
#[path = "declarations_independent/data_insert.rs"]
mod data_insert;
#[path = "declarations_independent/edit_format.rs"]
mod edit_format;
#[path = "declarations_independent/facts.rs"]
mod facts;
#[path = "declarations_independent/objects.rs"]
mod objects;
#[path = "declarations_independent/paths.rs"]
mod paths;
#[path = "declarations_independent/view_file.rs"]
mod view_file;

fn readings() -> impl Iterator<Item = &'static str> {
    [view_file::READINGS, data_insert::READINGS, edit_format::READINGS, objects::READINGS, binding_entry::READINGS, REVIEWED]
        .into_iter()
        .flat_map(str::lines)
        .filter(|line| line.starts_with("ROW "))
}

// Pinned from the declaration after review: this binding-only handler was opened
// with the declarations, so it cannot honestly be labelled a blind engine row.
const REVIEWED: &str = r##"
ROW commandLine | params={"text":"42"} | resolves=active sheet and active cell; delegates cell.set then moves down | holds=destination entry,style,array,covering constraints,inherited style,fitted row and formula sources | reads=part,selection,mode | writes=cell input and selection | note=lib.rs:403; pinned from the declaration after review
ROW file.save | params={"path":"out.csv"} | resolves=active sheet for CSV/TSV/HTML save formats | holds=whole workbook snapshot | reads=part | writes=encoded file through host | note=io.rs:200-213; editor projection pinned from the declaration after qualified source review; original blind XLSX row retained
ROW file.saveAs | params={"path":"out.csv"} | resolves=delegates file.save; active sheet for CSV/TSV/HTML | holds=whole workbook snapshot | reads=part | writes=encoded file through host | note=file.rs:116 and io.rs:200-213; editor projection pinned from the declaration after qualified source review; original blind XLSX row retained
ROW format.drawBorder | params={"range":"A1","edge":"bottom"} | resolves=delegates format.borders with kind from edge | holds=format.borders fields and windows | reads=part,selection | writes=border styles | note=extra.rs:1074; editor projection pinned from the declaration after review of the delegated path; original blind row retained
ROW data.fromTextCsv | params={"text":"A,B\n1,2"} | resolves=creates a new sheet at the active tab; insertSheet remembers old selection before resetting | holds=sheet-name uniqueness,tab insertion edge,table names and parser defaults | reads=part,selection | writes=imported sheet and table | note=sheet.rs:613; editor projection pinned from the declaration after handler review; original blind row retained
ROW data.getData | params={"text":"[{\"A\":1}]"} | resolves=creates a new sheet at the active tab; insertSheet remembers old selection before resetting | holds=sheet-name uniqueness,tab insertion edge,table names and parser defaults | reads=part,selection | writes=imported sheet and table | note=sheet.rs:613; editor projection pinned from the declaration after handler review; original blind row retained
ROW page.background | params={"clear":false} | resolves=fixed A1 origin on the active sheet; delegate selection default is unreachable | holds=picture insertion facts | reads=part | writes=background picture | note=extra.rs background and insert.rs insert_picture_at; C8-sheet-read-2 restores the blind meaning; background_selection.rs proves selection independence
ROW spelling.change | params={"word":"teh","to":"the","all":true} | resolves=all branch fixes all=true in edit.replace; single-cell branch supplies cell.set's cell, so no selection default determines either result | holds=replacement destinations and entry inputs | reads=part | writes=replaced entries | note=spelling.rs:167-179 and edit.rs:1680-1725; C8-sheet-read-2 restores blind meaning, projection_audit.rs tests both branches with different selections
"##;

#[test]
fn editor_read_projection_has_only_decreasing_mismatches() {
    use kition_kit::KitDocument;
    use std::collections::{BTreeMap, BTreeSet};
    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    let mut expected: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut pinned: BTreeSet<_> = REVIEWED.lines().filter_map(|line| line.strip_prefix("ROW ")).filter_map(|line| line.split(" | ").next()).collect();
    for line in readings() {
        let id = line.trim_start_matches("ROW ").split(" | ").next().expect("command");
        let reads = line.split(" | ").find_map(|field| field.strip_prefix("reads=")).expect("reads");
        let states = expected.entry(id).or_default();
        for read in reads.split(',').map(str::trim) {
            match read {
                "part" | "selection" | "mode" => {
                    states.insert(read);
                }
                "active" => {
                    states.insert("selection");
                }
                "draw_color" | "draw_width" => {
                    states.insert("mode");
                }
                _ => {}
            }
        }
    }
    for (command, states) in &mut expected {
        if let Some((reviewed, reason)) = edit_format::reviewed_editor(command) {
            assert!(!reason.is_empty());
            *states = reviewed.iter().copied().collect();
            pinned.insert(*command);
        }
    }
    for &(command, reviewed, reason) in data_insert::REVIEWED_EDITOR {
        assert!(!reason.is_empty());
        expected.insert(command, reviewed.iter().copied().collect());
        pinned.insert(command);
    }
    for &(command, reviewed, reason) in objects::REVIEWED_EDITOR {
        assert!(!reason.is_empty());
        expected.insert(command, reviewed.iter().copied().collect());
        pinned.insert(command);
    }
    eprintln!("EDITOR reviewed/pinned {} commands: {pinned:?}; original blind rows retained", pinned.len());
    let (mut less, mut more) = (BTreeSet::new(), BTreeSet::new());
    let mut registered = BTreeMap::new();
    let mut total = 0;
    for command in doc.commands().into_iter().filter(|command| command.mutates) {
        total += 1;
        let wanted = expected.get(command.id.as_str()).expect("every write has rows");
        registered.insert(command.id.clone(), wanted.clone());
        let actual: BTreeSet<_> = command.reads.iter().map(|read| read.as_str()).collect();
        let missing: Vec<_> = wanted.difference(&actual).collect();
        let extra: Vec<_> = actual.difference(wanted).collect();
        if !missing.is_empty() {
            less.insert(command.id.clone());
        }
        if !extra.is_empty() {
            more.insert(command.id.clone());
        }
        if !missing.is_empty() || !extra.is_empty() {
            eprintln!("EDITOR {} LESS={missing:?} MORE={extra:?}", command.id);
        }
    }
    if std::env::var_os("C8_EDITOR_DUMP").is_some() {
        eprintln!("EDITOR_EXPECTED {}", serde_json::to_string(&registered).expect("editor state rows"));
    }
    eprintln!("EDITOR {} / {total} differ; {} less; {} more", less.union(&more).count(), less.len(), more.len());
    // Calibrated from the first independent comparison; these limits only decrease.
    assert!(less.len() <= EDITOR_LEFT.0 && more.len() <= EDITOR_LEFT.1, "editor declaration floor exceeded");
}

// Initial projection: 10 missing and 87 excessive editor declarations out of
// 262 writes. Excess entries are review candidates: some blind rows describe
// explicit-parameter paths without separately spelling their default path.
const EDITOR_LEFT: (usize, usize) = (0, 0);

#[test]
fn independent_rows_parse() {
    for line in readings() {
        let params = line.split(" | ").find_map(|field| field.strip_prefix("params=")).expect("params");
        serde_json::from_str::<serde_json::Value>(params).expect(line);
    }
}

#[test]
fn every_registered_write_has_an_independent_row() {
    use kition_kit::KitDocument;
    use std::collections::BTreeSet;
    let doc = kition_sheet_kit::SheetDocument::create("Book.xlsx", 0, 0).expect("fixture");
    let written: BTreeSet<_> = readings().map(|line| line.trim_start_matches("ROW ").split(" | ").next().expect("command")).collect();
    let writes: BTreeSet<_> = doc.commands().into_iter().filter(|command| command.mutates).map(|command| command.id).collect();
    let missing: Vec<_> = writes.iter().filter(|id| !written.contains(id.as_str())).collect();
    eprintln!("{} binding writes; {} commands recorded in blind families; missing: {missing:?}", writes.len(), written.len());
    for (family, rows) in [
        ("view/file/protection", view_file::READINGS),
        ("data/insert/formulas/pivot/trial", data_insert::READINGS),
        ("edit/format/sheet", edit_format::READINGS),
        ("objects/extra", objects::READINGS),
    ] {
        let ids: BTreeSet<_> = rows.lines().filter_map(|line| line.strip_prefix("ROW ")).filter_map(|line| line.split(" | ").next()).collect();
        eprintln!(
            "blind family {family}: {} registered writes / {} recorded handlers",
            writes.iter().filter(|id| ids.contains(id.as_str())).count(),
            ids.len()
        );
    }
    assert!(missing.is_empty(), "unread writes: {missing:?}");
}
