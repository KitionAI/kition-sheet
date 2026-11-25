//! The engine bridge's `SheetSession`s, whose views follow a change of only cells by reading just
//! those (rule P1), equal a fresh read after every record, undo, redo and import: ~300 seeded
//! random edits (cell values, formulas within and across sheets, typing in text cells by
//! character, styles, row and column inserts and deletes, sheet renames and deletes, merges,
//! defined names, row heights) on one session, with its changes sent on one line to two others
//! now and then. At the end every replica has the same state hash and the same (id-blind) view,
//! and the replicas hold nothing dangling.

mod common;

use common::{assert_replica_clean, assert_same, lines, put, rich};
use kdm_model::{SheetSession, StructuralEdit};
use kition_kit::kdm::{Origin, Version};
use kition_kit::session::MemoryBlobs;
use kition_sheet_core::{CellRef, RangeRef, Value};
use kition_sheet_model::{Cell, DefinedName, LineInfo, StyleId, Workbook};

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

fn a1(row: u32, col: u32) -> String {
    CellRef::new(row, col).a1()
}

/// A reference to a random cell, maybe on another sheet.
fn reference(wb: &Workbook, rng: &mut Rng) -> String {
    let cell = a1(rng.below(30) as u32, rng.below(8) as u32);
    if rng.chance(30) {
        let other = &wb.sheets[rng.below(wb.sheets.len())].name;
        format!("{}!{cell}", kition_sheet_formula::quote_sheet(other))
    } else if rng.chance(20) {
        format!("SUM({cell}:{})", a1(rng.below(30) as u32, rng.below(8) as u32))
    } else {
        cell
    }
}

/// Values, formulas, clearing and typing: edits of only cells.
const CELL_EDITS: [usize; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 20, 21, 22];

/// Every other edit but a sheet insert (a new sheet's lines are made as it is typed in).
const OTHER_EDITS: [usize; 10] = [9, 10, 11, 12, 14, 15, 16, 17, 18, 19];

/// A random edit, of one of `kinds` when given.
fn edit_of(wb: &mut Workbook, rng: &mut Rng, kinds: Option<&[usize]>) -> Vec<StructuralEdit> {
    let s = rng.below(wb.sheets.len());
    let (row, col) = (rng.below(30) as u32, rng.below(8) as u32);
    let kind = rng.below(23);
    match kinds.map_or(kind, |k| k[kind % k.len()]) {
        // Values.
        0..=4 => {
            let v = if rng.chance(50) { format!("{}", rng.below(1000)) } else { format!("t{}", rng.below(100)) };
            put(wb, s, &a1(row, col), &v);
        }
        // Formulas.
        5..=7 => {
            let f = format!("={}+{}", reference(wb, rng), reference(wb, rng));
            put(wb, s, &a1(row, col), &f);
        }
        // Clearing.
        8 => {
            if let Some(sh) = wb.sheet_mut(s) {
                sh.remove_cell(CellRef::new(row, col));
            }
        }
        // Styles.
        9 => {
            let bold = rng.chance(50);
            let base = wb.sheets[s].cell(CellRef::new(row, col)).map(|c| c.style).unwrap_or_default();
            let id = wb.styles.derive(base, |st| {
                if bold {
                    st.font.bold = !st.font.bold;
                } else {
                    st.font.size = 8.0 + (row % 5) as f32;
                }
            });
            if let Some(sh) = wb.sheet_mut(s) {
                sh.set_style(CellRef::new(row, col), id);
            }
        }
        // Rows and columns.
        10 | 11 => return vec![lines(wb, s, true, row, 1 + rng.below(3) as u32, rng.chance(60))],
        12 => return vec![lines(wb, s, false, col, 1 + rng.below(2) as u32, rng.chance(60))],
        // Sheets.
        13 if wb.sheets.len() < 4 => {
            let name = (1..).map(|n| format!("S{n}")).find(|n| wb.sheet_index(n).is_none()).unwrap_or_default();
            common::insert_sheet(wb, rng.below(wb.sheets.len() + 1), &name);
        }
        14 => {
            let name = format!("R{}", rng.below(1000));
            if wb.sheet_index(&name).is_none() {
                common::rename_sheet(wb, s, &name);
            }
        }
        15 if wb.sheets.len() > 1 => common::delete_sheet(wb, s),
        // Merges.
        16 => {
            let sh = match wb.sheet_mut(s) {
                Some(sh) => sh,
                None => return vec![],
            };
            if !sh.merges.is_empty() && rng.chance(40) {
                let k = rng.below(sh.merges.len());
                sh.merges.remove(k);
            } else {
                let m = RangeRef::new(CellRef::new(row, col), CellRef::new(row + 1, col + 1));
                if !sh.merges.iter().any(|x| x.intersects(&m)) {
                    sh.merges.push(m);
                }
            }
        }
        // Defined names.
        17 => {
            if !wb.names.is_empty() && rng.chance(40) {
                let k = rng.below(wb.names.len());
                wb.names.remove(k);
            } else {
                let name = format!("N{}", rng.below(20));
                let scope = if rng.chance(30) { Some(s) } else { None };
                if !wb.names.iter().any(|n| n.name.eq_ignore_ascii_case(&name) && n.scope == scope) {
                    let target =
                        format!("{}!${}${}", kition_sheet_formula::quote_sheet(&wb.sheets[s].name), kition_sheet_core::col_to_letters(col), row + 1);
                    wb.names.push(DefinedName { name, scope, formula: target, comment: String::new(), hidden: false });
                }
            }
        }
        // Typing in a text cell: one character inserted or deleted (K5: these merge by
        // character with the other sessions' typing in the same cell).
        20..=22 => {
            let c = CellRef::new(row, col);
            let (text, style) = match wb.sheets[s].cell(c) {
                Some(cell) => (
                    match (&cell.formula, &cell.value) {
                        (None, Value::Text(t)) => t.to_string(),
                        _ => String::new(),
                    },
                    cell.style,
                ),
                None => (String::new(), StyleId::DEFAULT),
            };
            let mut chars: Vec<char> = text.chars().collect();
            if !chars.is_empty() && rng.chance(30) {
                chars.remove(rng.below(chars.len()));
            } else {
                let letter = char::from(b'a' + rng.below(26) as u8);
                chars.insert(rng.below(chars.len() + 1), letter);
            }
            let new: String = chars.into_iter().collect();
            if let Some(sh) = wb.sheet_mut(s) {
                if new.is_empty() && style == StyleId::DEFAULT {
                    sh.remove_cell(c);
                } else {
                    sh.set_cell(c, Cell { value: Value::Text(new.as_str().into()), formula: None, style, rich: None });
                }
            }
        }
        // Row heights.
        _ => {
            if let Some(sh) = wb.sheet_mut(s) {
                if rng.chance(70) {
                    sh.rows
                        .insert(row, LineInfo { size: Some(12.0 + row as f32), custom: true, style: Some(StyleId::DEFAULT), ..Default::default() });
                } else {
                    sh.rows.remove(&row);
                }
            }
        }
    }
    vec![]
}

/// The `SheetSession` a peer opens from `of`'s snapshot.
fn sheet_peer(of: &SheetSession) -> SheetSession {
    let blobs = MemoryBlobs(of.blobs_since(&Version::empty()).into_iter().collect());
    SheetSession::open_snapshot(&of.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}")), blobs).unwrap_or_else(|e| panic!("open: {e}"))
}

fn sheet_send(from: &SheetSession, to: &mut SheetSession) {
    let since = to.version();
    to.add_blobs(&MemoryBlobs(from.blobs_since(&since).into_iter().collect()));
    to.import_changes(&from.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("import: {e}"));
}

/// The view equals a fresh read of the session's own replica.
fn assert_view_is_read(what: &str, s: &SheetSession) {
    let fresh = kdm_model::read_document(s.replica(), s.blobs()).unwrap_or_else(|e| panic!("{what}: read: {e}")).0;
    assert_same(what, s.workbook(), &fresh);
}

/// Mostly typing, values, formulas and clearing (changes of only cells), with the occasional
/// other edit (a full read), and undo and redo, all on the first session; now and then its
/// changes go to one of the two others, which only follow (one line of changes). `appending`: the
/// sheets' lines end where the data does, so edits make new lines below and right of it;
/// otherwise every sheet's lines reach past where the edits go (row 30, column H, and a few line
/// deletes).
fn run_sessions(seed: u64, ops: usize, appending: bool) {
    let mut base = rich();
    if !appending {
        for k in 0..base.sheets.len() {
            put(&mut base, k, "T100", "edge");
        }
    }
    let mut all = vec![SheetSession::create(&base).unwrap_or_else(|e| panic!("create: {e}"))];
    all.push(sheet_peer(&all[0]));
    all.push(sheet_peer(&all[0]));
    let origin = Origin::person("user:test");
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    for op in 0..ops {
        match rng.below(10) {
            0 => {
                all[0].undo().unwrap_or_else(|e| panic!("seed {seed}: undo: {e}"));
                assert_view_is_read(&format!("seed {seed} op {op}: undo"), &all[0]);
            }
            1 => {
                all[0].redo().unwrap_or_else(|e| panic!("seed {seed}: redo: {e}"));
                assert_view_is_read(&format!("seed {seed} op {op}: redo"), &all[0]);
            }
            2 | 3 => {
                let j = 1 + rng.below(all.len() - 1);
                let (a, b) = all.split_at_mut(j);
                sheet_send(&a[0], &mut b[0]);
                assert_view_is_read(&format!("seed {seed} op {op}: import"), &b[0]);
            }
            _ => {
                let mut step = Rng(rng.next() | 1);
                let mut wb = all[0].workbook().clone();
                let kinds = if rng.chance(85) { &CELL_EDITS[..] } else { &OTHER_EDITS[..] };
                let edits = edit_of(&mut wb, &mut step, Some(kinds));
                all[0].record(&origin, &wb, &edits).unwrap_or_else(|e| panic!("seed {seed}: record: {e}"));
                assert_view_is_read(&format!("seed {seed} op {op}: record"), &all[0]);
            }
        }
    }
    for j in 1..all.len() {
        let (a, b) = all.split_at_mut(j);
        sheet_send(&a[0], &mut b[0]);
    }
    let hash = all[0].state_hash().unwrap_or_else(|e| panic!("hash: {e}"));
    for (k, s) in all.iter().enumerate() {
        assert_eq!(s.state_hash().unwrap_or_else(|e| panic!("hash: {e}")), hash, "seed {seed}: session {k} has another state");
        assert_view_is_read(&format!("seed {seed}: session {k} at the end"), s);
        assert_same(&format!("seed {seed}: session {k}"), all[0].workbook(), s.workbook());
        assert_replica_clean(s.replica());
    }
}

#[test]
fn sheet_session_views_equal_a_fresh_read() {
    // 49, 52, 67, 212: an undo restoring a cell entry a peer re-keyed, a cleared cell in a deleted
    // row, and undoing the step that made lines or a name (read again in full). 351: an entry a
    // merge kept whose object an undo deleted, in a sheet the view had no cell index for.
    for seed in (21..=40).chain([49, 52, 67, 212, 351]) {
        run_sessions(seed, 300, false);
    }
}

#[test]
fn sheet_session_views_equal_a_fresh_read_typing_past_the_data() {
    // 34, 40: an image whose anchor row a delete cut through. 224: a sheet deleted while a peer
    // typed in it (the repair after the merge).
    for seed in (31..=50).chain([224]) {
        run_sessions(seed, 300, true);
    }
}
