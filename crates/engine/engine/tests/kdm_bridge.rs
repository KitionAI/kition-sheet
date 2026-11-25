//! The command bridge to each workbook's KDM replica (Kition phase 3f, Task 2): every mutating
//! command replicates as one transaction that reproduces the workbook in a peer (cell edits, fill,
//! sort, paste, row and column inserts and deletes, formulas, styles, sheet add/rename/delete);
//! undo of a step a peer's later edits followed keeps those edits; selection-only and view changes
//! write nothing; the history limit never loses an edit; the selection follows remote row inserts
//! and deletes; formatting characters of one cell keeps every format and undo takes back the last;
//! and two sessions taking turns at random commands hold the same workbook, equal to its own
//! replica after every command.

use kition_sheet_engine::model::Workbook;
use kition_sheet_engine::{MemoryBlobs, Origin, Session, Version};
use serde_json::{Value, json};

/// A session with a small budget: labels in A, costs in B, a total in B5, a doubled cost in C2.
fn new_session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    for (cell, input) in [
        ("A1", "Item"),
        ("B1", "Cost"),
        ("A2", "Rent"),
        ("B2", "1200"),
        ("A3", "Food"),
        ("B3", "450"),
        ("A4", "Fuel"),
        ("B4", "80"),
        ("A5", "Total"),
        ("B5", "=SUM(B2:B4)"),
        ("C2", "=B2*2"),
    ] {
        run(&mut s, "cell.set", json!({"cell": cell, "input": input}));
    }
    run(&mut s, "selection.set", json!({"range": "A1"}));
    s
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, params).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A second session holding a replica of `a`'s active workbook.
fn peer(a: &mut Session) -> Session {
    let mut b = Session::new();
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    b.open_snapshot(&a.snapshot().unwrap(), blobs).unwrap();
    b
}

/// Sends `from`'s changes (and the blobs they need) that `to` lacks.
fn send(from: &mut Session, to: &mut Session) {
    let since = to.version().unwrap();
    let blobs = from.blobs_since(&since).unwrap();
    to.add_blobs(&blobs).unwrap();
    let bytes = from.export_changes(&since).unwrap();
    to.import_changes(&bytes).unwrap();
}

fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..8 {
        send(a, b);
        send(b, a);
        if a.version().unwrap() == b.version().unwrap() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

fn wb(s: &Session) -> &Workbook {
    &s.active().unwrap().wb
}

fn canonical(wb: &Workbook) -> Value {
    serde_json::to_value(kdm_model::normalize(wb)).unwrap()
}

/// `a` and `b` hold the same replicated state and the same workbook (view state aside).
fn assert_same(a: &mut Session, b: &mut Session, what: &str) {
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap(), "{what}: replicas differ");
    let (x, y) = (canonical(wb(a)), canonical(wb(b)));
    assert!(x == y, "{what}: workbooks differ:\n{:#?}", differences(&x, &y));
}

/// The workbook as read back fresh from the session's replica, without flushing anything.
fn replica_wb(s: &Session) -> Workbook {
    let kdm = s.kdm().expect("an edit made the replica");
    kdm_model::read_document(kdm.replica(), kdm.blobs()).unwrap().0
}

/// The engine's workbook is exactly what its replica holds.
fn assert_holds(s: &Session, what: &str) {
    let (mine, held) = (canonical(wb(s)), canonical(&replica_wb(s)));
    assert!(mine == held, "{what}: the workbook is not what its replica holds:\n{:#?}", differences(&mine, &held));
}

/// The replica's own undo steps (one per undoable transaction).
fn replica_steps(s: &Session) -> usize {
    s.kdm().map_or(0, |k| k.steps().0)
}

fn last_steps(s: &Session) -> u32 {
    s.active().unwrap().undo.last().unwrap().steps
}

fn history(s: &Session) -> usize {
    s.active().unwrap().undo.len()
}

/// `cell.get` of `a1` on the active sheet.
fn get(s: &mut Session, a1: &str) -> Value {
    run(s, "cell.get", json!({"cell": a1}))
}

fn formula(s: &mut Session, a1: &str) -> Value {
    get(s, a1)["formula"].clone()
}

fn value(s: &mut Session, a1: &str) -> Value {
    get(s, a1)["value"].clone()
}

#[test]
fn one_command_one_transaction_reproduces_in_a_peer() {
    let cases: Vec<(&str, fn(&mut Session))> = vec![
        ("cell edit", |s| {
            run(s, "cell.set", json!({"cell": "B3", "input": "500"}));
        }),
        ("formula entry", |s| {
            run(s, "cell.set", json!({"cell": "D2", "input": "=B2+B3"}));
        }),
        ("fill down", |s| {
            run(s, "cell.set", json!({"cell": "D2", "input": "=B2*3"}));
            run(s, "edit.fillDown", json!({"range": "D2:D4"}));
        }),
        ("sort", |s| {
            run(s, "data.sortAscending", json!({"range": "A2:B4", "column": "B"}));
        }),
        ("copy and paste", |s| {
            run(s, "edit.copy", json!({"range": "A2:C3"}));
            run(s, "edit.paste", json!({"at": "E7"}));
        }),
        ("insert rows", |s| {
            run(s, "home.insertRows", json!({"rows": "3:4"}));
        }),
        ("delete rows", |s| {
            run(s, "home.deleteRows", json!({"rows": "3:3"}));
        }),
        ("insert columns", |s| {
            run(s, "home.insertColumns", json!({"cols": "B:B"}));
        }),
        ("delete columns", |s| {
            run(s, "home.deleteColumns", json!({"cols": "A:A"}));
        }),
        ("style change", |s| {
            run(s, "home.bold", json!({"range": "A1:B1", "on": true}));
            run(s, "home.fillColor", json!({"range": "B2:B4", "color": "#ffeeaa"}));
        }),
        ("sheet add", |s| {
            run(s, "home.insertSheet", json!({"name": "Notes"}));
            run(s, "cell.set", json!({"cell": "A1", "input": "on the new sheet"}));
        }),
        ("sheet rename", |s| {
            run(s, "cell.set", json!({"cell": "D1", "input": "=Sheet1!B5"}));
            run(s, "sheet.rename", json!({"sheet": 0, "name": "Budget"}));
        }),
        ("sheet delete", |s| {
            run(s, "home.insertSheet", json!({"name": "Scratch"}));
            run(s, "home.deleteSheet", json!({"sheet": "Scratch"}));
        }),
    ];
    for (name, case) in cases {
        let mut a = new_session();
        let mut b = peer(&mut a);
        let steps = replica_steps(&a);
        let entries = history(&a);
        case(&mut a);
        let commands = history(&a) - entries;
        assert!(commands >= 1, "{name}: an undo step");
        assert_eq!(last_steps(&a), 1, "{name}: exactly one transaction for the last step");
        assert_eq!(replica_steps(&a) - steps, commands, "{name}: one transaction per step, nothing else");
        assert_eq!(a.kdm_resets(), 0, "{name}: the replica took every write");
        assert_holds(&a, name);
        send(&mut a, &mut b);
        assert_same(&mut a, &mut b, name);
    }
}

#[test]
fn a_row_insert_keeps_formulas_on_their_rows_in_the_peer() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    run(&mut a, "home.insertRows", json!({"rows": "2:2"}));
    send(&mut a, &mut b);
    assert_eq!(formula(&mut b, "B6"), json!("=SUM(B3:B5)"), "the total still sums the same rows");
    assert_eq!(formula(&mut b, "C3"), json!("=B3*2"));
    assert_eq!(value(&mut b, "B6"), json!(1730.0), "the peer recalculated");
    assert_eq!(value(&mut b, "C3"), json!(2400.0));
}

#[test]
fn a_command_that_runs_commands_is_one_step() {
    let mut s = new_session();
    let entries = history(&s);
    run(
        &mut s,
        "automate.newScript",
        json!({"name": "Fill", "commands": [
            {"command": "cell.set", "params": {"cell": "E1", "input": "one"}},
            {"command": "cell.set", "params": {"cell": "E2", "input": "two"}},
            {"command": "home.insertRows", "params": {"rows": "1:1"}},
        ]}),
    );
    let before = canonical(wb(&s));
    let after_new = history(&s);
    run(&mut s, "automate.runScript", json!({"name": "Fill"}));
    assert_eq!(history(&s), after_new + 1, "one history step for the whole script");
    assert!(history(&s) > entries);
    assert_eq!(last_steps(&s), 3, "each command it ran wrote its own transaction, all in that step");
    assert_holds(&s, "after the script");
    run(&mut s, "edit.undo", json!({}));
    assert!(canonical(wb(&s)) == before, "one undo takes the whole script back");
    assert_holds(&s, "after the undo");
}

#[test]
fn undo_after_a_remote_edit_keeps_it() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    run(&mut a, "cell.set", json!({"cell": "B2", "input": "1300"}));
    send(&mut a, &mut b);
    run(&mut b, "home.insertRows", json!({"rows": "1:1"}));
    run(&mut b, "cell.set", json!({"cell": "A1", "input": "Budget"}));
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");
    assert_eq!(value(&mut a, "B3"), json!(1300.0), "a's edit moved down with its row");
    run(&mut a, "edit.undo", json!({}));
    assert_eq!(value(&mut a, "B3"), json!(1200.0), "the local edit is undone");
    assert_eq!(value(&mut a, "A1"), json!("Budget"), "the remote insert and edit survive the local undo");
    assert_eq!(value(&mut a, "B6"), json!(1730.0), "the undo's result is recalculated");
    assert_holds(&a, "after the undo");
    run(&mut a, "edit.redo", json!({}));
    assert_eq!(value(&mut a, "B3"), json!(1300.0));
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after the redo");
}

#[test]
fn undo_back_to_the_saved_workbook_is_clean() {
    let mut s = new_session();
    let st = s.active_mut().unwrap();
    st.saved = st.wb.clone();
    run(&mut s, "cell.set", json!({"cell": "A2", "input": "Dirty"}));
    assert!(s.active().unwrap().is_dirty());
    run(&mut s, "edit.undo", json!({}));
    assert!(!s.active().unwrap().is_dirty(), "the replica's undo result is the saved workbook");
}

#[test]
fn selection_and_view_changes_write_nothing() {
    let mut a = new_session();
    run(&mut a, "home.insertSheet", json!({"name": "Second"}));
    let mut b = peer(&mut a);
    let version = a.version().unwrap();
    let b_selection = b.active().unwrap().selection.clone();
    run(&mut a, "selection.set", json!({"range": "B2:C4"}));
    run(&mut a, "selection.move", json!({"dr": 1, "dc": 0}));
    run(&mut a, "sheet.activate", json!({"sheet": 0}));
    run(&mut a, "view.zoom", json!({"percent": 150}));
    run(&mut a, "edit.copy", json!({"range": "A1:B2"}));
    assert_eq!(a.version().unwrap(), version, "no transaction");
    assert_eq!(wb(&a).active_sheet, 0);
    send(&mut a, &mut b);
    assert_eq!(b.active().unwrap().selection, b_selection, "the peer's own selection is untouched");
    assert_same(&mut a, &mut b, "nothing to send");
    // An import keeps this person's view of the sheet.
    run(&mut b, "cell.set", json!({"cell": "F1", "input": "remote"}));
    send(&mut b, &mut a);
    assert_eq!(wb(&a).active_sheet, 0, "still on the sheet a chose");
    assert_eq!(wb(&a).sheets[0].zoom, 150, "a's zoom survives the import");
}

#[test]
fn every_edit_replicates_at_the_history_limit() {
    let mut s = new_session();
    for i in 0..105 {
        run(&mut s, "cell.set", json!({"cell": "A10", "input": format!("{i}")}));
        let held = replica_wb(&s);
        let text = held.sheets[0].input_text(kition_sheet_engine::core::CellRef::new(9, 0));
        assert_eq!(text, format!("{i}"), "edit {i} replicated");
        assert_eq!(last_steps(&s), 1, "edit {i}: one replica step");
    }
    assert_eq!(history(&s), 100);
    assert_eq!(s.kdm().unwrap().steps().0, 100, "the replica keeps only the steps the history can take");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(value(&mut s, "A10"), json!(103.0), "undo reverts the last edit only");
    assert_holds(&s, "after the undo");
    for _ in 0..120 {
        let _ = s.execute("edit.undo", json!({}));
    }
    assert_eq!(value(&mut s, "A10"), json!(4.0), "undo stops where the history does");
    assert_holds(&s, "at the bottom of the history");
}

#[test]
fn the_selection_follows_remote_row_and_column_edits() {
    let mut a = new_session();
    let mut b = peer(&mut a);
    run(&mut a, "selection.set", json!({"range": "B4"}));
    run(&mut b, "home.insertRows", json!({"rows": "2:3"}));
    send(&mut b, &mut a);
    assert_eq!(a.active().unwrap().selection.active.a1(), "B6", "two rows inserted above move it down");
    run(&mut b, "home.deleteRows", json!({"rows": "6:6"}));
    send(&mut b, &mut a);
    assert_eq!(a.active().unwrap().selection.active.a1(), "B6", "its row deleted: the row now in its place");
    run(&mut b, "home.insertColumns", json!({"cols": "A:A"}));
    send(&mut b, &mut a);
    assert_eq!(a.active().unwrap().selection.active.a1(), "C6", "a column inserted left of it moves it right");
    run(&mut a, "cell.set", json!({"input": "here"}));
    assert_holds(&a, "an edit after the imports");
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");
}

#[test]
fn a_remote_deletion_of_the_active_sheet_moves_the_selection_on() {
    let mut a = new_session();
    run(&mut a, "home.insertSheet", json!({"name": "Gone"}));
    let mut b = peer(&mut a);
    run(&mut a, "sheet.activate", json!({"sheet": "Gone"}));
    run(&mut a, "selection.set", json!({"range": "D9"}));
    run(&mut b, "home.deleteSheet", json!({"sheet": "Gone"}));
    send(&mut b, &mut a);
    let st = a.active().unwrap();
    assert_eq!(st.wb.sheets.len(), 1);
    assert_eq!(st.wb.active_sheet, 0, "on a sheet that exists");
    run(&mut a, "cell.set", json!({"cell": "A7", "input": "still editing"}));
    assert_holds(&a, "an edit after the import");
}

#[test]
fn agent_edits_record_their_origin_and_undo_on_this_replica() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("cell.set", json!({"cell": "A2", "input": "Agent"}), &agent).unwrap();
    assert_eq!(s.origin(), kition_sheet_engine::kdm_bridge::local_person(), "the origin is restored after the call");
    assert_eq!(value(&mut s, "A2"), json!("Agent"));
    // KitModel M12a: the person's Undo takes back the person's own last step, never the agent's.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(value(&mut s, "A2"), json!("Agent"), "the person's undo keeps the agent's edit");
    assert_eq!(formula(&mut s, "C2"), Value::Null, "and takes back the person's last cell");
    run(&mut s, "edit.redo", json!({}));
    // E14a: the agent's scope by its own origin, or by `scope`.
    s.execute_as("edit.undo", json!({}), &agent).unwrap();
    assert_eq!(value(&mut s, "A2"), json!("Rent"), "the agent's undo takes back the agent's edit");
    s.execute_as("edit.redo", json!({}), &agent).unwrap();
    assert_eq!(value(&mut s, "A2"), json!("Agent"));
    run(&mut s, "edit.undo", json!({"scope": "agent"}));
    assert_eq!(value(&mut s, "A2"), json!("Rent"));
}

#[test]
fn a_file_newer_than_its_stored_replica_wins_on_open() {
    let mut a = new_session();
    let snapshot = a.snapshot().unwrap();
    let blobs = a.blobs_since(&Version::empty()).unwrap();
    let doc_id = a.doc_id().unwrap();
    run(&mut a, "cell.set", json!({"cell": "A2", "input": "Newer"}));
    let file = kition_sheet_engine::io::save_bytes(wb(&a), "Book.xlsx").unwrap();

    let mut b = Session::new();
    b.open_with_replica("Book.xlsx", &file, &snapshot, blobs.clone()).unwrap();
    assert_eq!(value(&mut b, "A2"), json!("Newer"), "the file's content");
    let opened = kition_sheet_engine::io::open_bytes("Book.xlsx", &file).unwrap().0;
    assert!(canonical(wb(&b)) == canonical(&opened), "the whole file");
    assert_holds(&b, "recorded onto the replica");
    assert_eq!(b.doc_id().unwrap(), doc_id, "the replica's own identity");
    assert!(!b.active().unwrap().is_dirty(), "opening is clean");
    assert!(b.active().unwrap().undo.is_empty(), "opening is not an undo step");

    let mut c = Session::new();
    c.open_with_replica("Book.xlsx", &[], &snapshot, blobs).unwrap();
    send(&mut b, &mut c);
    assert_same(&mut b, &mut c, "a peer of the older replica gets the file's content");
}

#[test]
fn a_file_its_replica_holds_opens_as_that_replica() {
    let mut a = new_session();
    run(&mut a, "home.insertSheet", json!({"name": "Second"}));
    let file = kition_sheet_engine::io::save_bytes(wb(&a), "Book.xlsx").unwrap();
    let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs_since(&Version::empty()).unwrap());
    let version = a.version().unwrap();
    let mut b = Session::new();
    b.open_with_replica("Book.xlsx", &file, &snapshot, blobs).unwrap();
    assert_eq!(b.version().unwrap(), version, "nothing recorded");
    assert_eq!(wb(&b).active_sheet, wb(&a).active_sheet, "the file's active sheet");
    assert!(!b.active().unwrap().is_dirty());
}

#[test]
fn passthroughs_answer_before_any_edit() {
    let mut s = Session::new();
    s.new_workbook();
    assert!(s.doc_id().is_none(), "no replica before anything asks for one");
    let version = s.version().unwrap();
    assert!(s.doc_id().is_some());
    assert_eq!(s.kdm_resets(), 0);
    assert!(s.export_changes(&Version::empty()).unwrap().len() > 1);
    assert_eq!(s.version().unwrap(), version, "asking writes nothing");
    let blobs: MemoryBlobs = s.blobs().unwrap();
    assert!(blobs.0.is_empty(), "a new workbook has no pictures");
}

/// A tiny deterministic generator (no extra dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn a1(rng: &mut Rng) -> String {
    let col = (b'A' + rng.below(5) as u8) as char;
    format!("{col}{}", 1 + rng.below(9))
}

/// One random command: cell edits and formulas, fill, sort, copy and paste, row and column
/// inserts and deletes, styles, sheet add/rename/delete/activate, undo and redo.
fn random_command(s: &mut Session, rng: &mut Rng) -> String {
    let sheets = wb(s).sheets.len() as u64;
    let (id, params): (&str, Value) = match rng.below(20) {
        0..=3 => {
            let input = match rng.below(4) {
                0 => format!("{}", rng.below(1000)),
                1 => format!("text {}", rng.below(50)),
                2 => format!("=SUM({}:{})", a1(rng), a1(rng)),
                _ => format!("={}*2", a1(rng)),
            };
            ("cell.set", json!({"cell": a1(rng), "input": input}))
        }
        4 => {
            let at = 1 + rng.below(8);
            ("home.insertRows", json!({"rows": format!("{at}:{}", at + rng.below(2))}))
        }
        5 => {
            let at = 1 + rng.below(8);
            ("home.deleteRows", json!({"rows": format!("{at}:{at}")}))
        }
        6 => {
            let col = (b'A' + rng.below(4) as u8) as char;
            ("home.insertColumns", json!({"cols": format!("{col}:{col}")}))
        }
        7 => {
            let col = (b'A' + rng.below(4) as u8) as char;
            ("home.deleteColumns", json!({"cols": format!("{col}:{col}")}))
        }
        8 => ("home.bold", json!({"range": format!("{}:{}", a1(rng), a1(rng)), "on": rng.below(2) == 0})),
        9 => ("data.sortAscending", json!({"range": "A2:B6", "column": "B"})),
        10 => {
            let col = (b'A' + rng.below(4) as u8) as char;
            ("edit.fillDown", json!({"range": format!("{col}2:{col}5")}))
        }
        11 => {
            let _ = s.execute("edit.copy", json!({"range": format!("{}:{}", a1(rng), a1(rng))}));
            ("edit.paste", json!({"at": a1(rng)}))
        }
        12 => ("home.insertSheet", json!({})),
        13 => ("sheet.rename", json!({"name": format!("S{}", rng.below(100))})),
        14 if sheets > 1 => ("home.deleteSheet", json!({"sheet": rng.below(sheets)})),
        14 => ("home.insertSheet", json!({})),
        15 => ("sheet.activate", json!({"sheet": rng.below(sheets)})),
        16 => ("edit.redo", json!({})),
        _ => ("edit.undo", json!({})),
    };
    let r = s.execute(id, params.clone());
    format!("{id} {params} -> {}", if r.is_ok() { "ok" } else { "refused" })
}

/// Two sessions take turns at random commands, four at a time, exchanging changes after each turn
/// (one line of changes). After every command the workbook is exactly what its own replica holds;
/// after every exchange both hold the same replicated state and the same workbook.
#[test]
fn random_commands_keep_workbook_and_replica_equal_and_follow_on_a_peer() {
    for seed in [3u64, 11, 2026] {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut a = new_session();
        let mut b = peer(&mut a);
        let mut log: Vec<String> = Vec::new();
        for step in 0..60 {
            let turn = if (step / 4) % 2 == 0 { ("a", &mut a) } else { ("b", &mut b) };
            for (name, s) in [turn] {
                let what = random_command(s, &mut rng);
                log.push(format!("{name}: {what}"));
                let context = || format!("seed {seed}, step {step}, {name}; last commands:\n{}", log[log.len().saturating_sub(6)..].join("\n"));
                assert_eq!(s.kdm_resets(), 0, "{}", context());
                let (mine, held) = (canonical(wb(s)), canonical(&replica_wb(s)));
                assert!(mine == held, "the workbook is not what its replica holds: {}\n{:#?}", context(), differences(&mine, &held));
                let st = s.active().unwrap();
                assert!(st.wb.active_sheet < st.wb.sheets.len(), "active sheet out of range: {}", context());
            }
            if step % 4 == 3 {
                sync(&mut a, &mut b);
                assert_same(&mut a, &mut b, &format!("seed {seed}, step {step}"));
            }
        }
        sync(&mut a, &mut b);
        assert_same(&mut a, &mut b, &format!("seed {seed}, final"));
        let mut c = peer(&mut a);
        assert_same(&mut a, &mut c, &format!("seed {seed}: a fresh peer matches"));
    }
}

/// The first few paths at which two JSON values differ (for a readable failure).
fn differences(a: &Value, b: &Value) -> Vec<String> {
    fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        if out.len() >= 5 || a == b {
            return;
        }
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for key in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    walk(x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null), format!("{path}.{key}"), out);
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    walk(p, q, format!("{path}[{i}]"), out);
                }
            }
            _ => out.push(format!("{path}: {} vs {}", short(a), short(b))),
        }
    }
    fn short(v: &Value) -> String {
        let s = v.to_string();
        if s.len() > 300 { format!("{}…", s.chars().take(300).collect::<String>()) } else { s }
    }
    let mut out = Vec::new();
    walk(a, b, String::new(), &mut out);
    out
}

/// A session on a workbook whose A1 is rich text: "Total " bold, "due" red, " now" plain.
fn rich_session() -> Session {
    use kition_sheet_engine::model::{Cell, RichText, RunColor, RunFont, StyleId, TextRun};
    let red = RunFont { color: Some(RunColor { rgb: Some("FFFF0000".into()), ..RunColor::default() }), ..RunFont::default() };
    let runs = [
        TextRun::new("Total ", RunFont { bold: Some(true), ..RunFont::default() }),
        TextRun::new("due", red),
        TextRun::new(" now", RunFont::default()),
    ];
    let mut book = Workbook::new();
    book.sheet_mut(0).unwrap().set_cell(kition_sheet_engine::cell(0, 0), Cell::rich_text(RichText::new(runs).unwrap(), StyleId::DEFAULT));
    let mut s = Session::new();
    s.add_document(kition_sheet_engine::DocState::new(book, None, "Rich".into()));
    run(&mut s, "cell.set", json!({"cell": "B1", "input": "1"}));
    s
}

fn runs(s: &Session, a1: &str) -> Vec<(String, Option<bool>)> {
    let cell = wb(s).active().unwrap().cell(kition_sheet_engine::core::CellRef::parse(a1).unwrap()).unwrap();
    cell.runs().map(|r| r.runs.iter().map(|r| (r.text.clone(), r.font.bold)).collect()).unwrap_or_default()
}

/// Formatting different characters of one cell through `cell.formatText` (and the same
/// characters with different properties): every format lands on both replicas, as marks.
#[test]
fn formatting_characters_of_one_cell_keeps_every_format() {
    let mut a = rich_session();
    let mut b = peer(&mut a);
    // "Total due now": "Total" italicised, "due" bolded too, then "now" bolded.
    run(&mut a, "cell.formatText", json!({"cell": "A1", "start": 0, "end": 5, "italic": true}));
    run(&mut a, "cell.formatText", json!({"cell": "A1", "start": 6, "end": 9, "bold": true}));
    run(&mut a, "cell.formatText", json!({"cell": "A1", "start": 10, "end": 13, "toggle": "bold"}));
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "exchanged");
    for s in [&mut a, &mut b] {
        assert_eq!(value(s, "A1"), json!("Total due now"));
        let expected = [("Total", Some(true)), (" ", Some(true)), ("due", Some(true)), (" ", None), ("now", Some(true))];
        assert_eq!(runs(s, "A1"), expected.map(|(t, b)| (t.to_owned(), b)));
        let cell = wb(s).active().unwrap().cell(kition_sheet_engine::core::CellRef::parse("A1").unwrap()).unwrap().clone();
        let italic: String = cell.runs().unwrap().runs.iter().filter(|r| r.font.italic == Some(true)).map(|r| r.text.as_str()).collect();
        assert_eq!(italic, "Total");
        let red: String = cell.runs().unwrap().runs.iter().filter(|r| r.font.color.is_some()).map(|r| r.text.as_str()).collect();
        assert_eq!(red, "due", "the colour a run had stays");
    }
    // The undo takes back only the last formatting.
    run(&mut a, "edit.undo", json!({}));
    sync(&mut a, &mut b);
    assert_same(&mut a, &mut b, "after the undo");
    assert_eq!(runs(&a, "A1").last().cloned(), Some((" now".into(), None)));
    assert_holds(&a, "after the undo");
    assert_holds(&b, "after the undo");
}

#[test]
fn the_persons_undo_keeps_the_agents_step_and_the_agents_undo_takes_it_back() {
    // KitModel 1.1 M12a and engine rule E14a: the person's and the agents' undo histories are
    // separate; edit.undo and edit.redo work in their `scope`, else in that of who runs them.
    let mut s = new_session();
    let agent = Origin::agent("test");
    run(&mut s, "cell.set", json!({"cell": "D1", "input": "mine"}));
    s.execute_as("cell.set", json!({"cell": "D2", "input": "theirs"}), &agent).unwrap();
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(value(&mut s, "D1"), Value::Null, "the person's undo takes back the person's cell");
    assert_eq!(value(&mut s, "D2"), json!("theirs"), "and keeps the agent's later cell");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(value(&mut s, "D1"), json!("mine"));
    s.execute_as("edit.undo", json!({}), &agent).unwrap();
    assert_eq!(value(&mut s, "D2"), Value::Null, "the agent's undo takes back the agent's cell");
    assert_eq!(value(&mut s, "D1"), json!("mine"), "and keeps the person's");
    run(&mut s, "edit.redo", json!({"scope": "agent"}));
    assert_eq!(value(&mut s, "D2"), json!("theirs"), "the person can redo the agent's step by naming its scope");
    assert!(s.execute("edit.undo", json!({"scope": "everyone"})).is_err(), "an unknown scope is refused");
    assert_holds(&s, "after scoped undo and redo");
}

#[test]
fn an_agent_undo_under_the_persons_later_step_keeps_the_persons_edit() {
    let mut s = new_session();
    let agent = Origin::agent("test");
    s.execute_as("cell.set", json!({"cell": "D2", "input": "theirs"}), &agent).unwrap();
    run(&mut s, "cell.set", json!({"cell": "D1", "input": "mine"}));
    s.execute_as("edit.undo", json!({}), &agent).unwrap();
    assert_eq!(value(&mut s, "D2"), Value::Null, "only the agent's cell goes");
    assert_eq!(value(&mut s, "D1"), json!("mine"), "the person's later cell stays");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(value(&mut s, "D1"), Value::Null, "then the person's undo takes back the person's cell");
    s.execute_as("edit.redo", json!({}), &agent).unwrap();
    assert_eq!(value(&mut s, "D2"), json!("theirs"), "and the agent's redo brings the agent's cell back");
    assert_eq!(value(&mut s, "D1"), Value::Null);
    assert_holds(&s, "after scoped undo and redo");
}
