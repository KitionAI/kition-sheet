//! Find and Replace as the editor's Find and Replace window uses them (plan "Sheet usable by
//! hand" stage 4A; walkthrough Q1, Q2; WPS W-O3): Find Next goes on by rows from the active cell
//! and round to the first; Find Previous (`backward`) goes back and round to the last; a Replace
//! counts and writes only the cells it changes, and one that changes nothing leaves no undo step.

use kition_sheet_engine::Session;
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn run(s: &mut Session, command: &str, params: Json) -> Json {
    s.execute(command, params.clone()).unwrap_or_else(|e| panic!("{command} {params}: {e}"))
}

fn active(s: &Session) -> String {
    s.doc().unwrap().selection.active.a1()
}

fn undo_depth(s: &Session) -> usize {
    s.doc().unwrap().undo.len()
}

fn sample(s: &mut Session) {
    run(s, "range.setValues", json!({"range": "B2:C3", "values": [["Zq1", "x"], ["y", "Zq2"]]}));
    run(s, "cell.set", json!({"cell": "A5", "input": "zq3"}));
    run(s, "select.set", json!({"range": "A1"}));
}

#[test]
fn find_previous_goes_back_and_round_to_the_last() {
    let mut s = session();
    sample(&mut s);
    // Find Next by rows: B2, C3, A5, then round to B2.
    for want in ["B2", "C3", "A5", "B2"] {
        let r = run(&mut s, "edit.find", json!({"what": "zq"}));
        assert_eq!(r["cell"], want);
        assert_eq!(active(&s), want);
    }
    // Find Previous from B2: round to the last (A5), then C3, then B2.
    for want in ["A5", "C3", "B2"] {
        let r = run(&mut s, "edit.find", json!({"what": "zq", "backward": true}));
        assert_eq!(r["cell"], want);
        assert_eq!(r["count"], 3);
    }
    // By columns, backward from B2: A5 comes before it (column A).
    let r = run(&mut s, "edit.find", json!({"what": "zq", "backward": true, "byColumns": true}));
    assert_eq!(r["cell"], "A5");
    // `backward: false` is Find Next.
    let r = run(&mut s, "edit.find", json!({"what": "zq", "backward": false}));
    assert_eq!(r["cell"], "B2");
}

#[test]
fn a_replace_that_changes_nothing_leaves_no_undo_step() {
    let mut s = session();
    sample(&mut s);
    let depth = undo_depth(&s);
    let revision = s.doc().unwrap().revision;
    // Nothing matches.
    assert_eq!(run(&mut s, "edit.replace", json!({"what": "nothere", "with": "x"}))["replaced"], 0);
    // The active cell (A1) does not match: Replace (one) changes nothing.
    assert_eq!(run(&mut s, "edit.replace", json!({"what": "Zq", "with": "Rq", "all": false}))["replaced"], 0);
    // A wildcard matches, but the text holds no `*` to replace: nothing changes, nothing counts.
    assert_eq!(run(&mut s, "edit.replace", json!({"what": "Zq*", "with": "Rq"}))["replaced"], 0);
    assert_eq!(undo_depth(&s), depth, "no undo step for a replace that changed nothing");
    assert_eq!(s.doc().unwrap().revision, revision);
    // Replace All changes the three cells in one undo step, and counts them.
    assert_eq!(run(&mut s, "edit.replace", json!({"what": "zq", "with": "Rq"}))["replaced"], 3);
    assert_eq!(undo_depth(&s), depth + 1);
    assert_eq!(run(&mut s, "cell.get", json!({"cell": "B2"}))["value"], "Rq1");
    assert_eq!(run(&mut s, "cell.get", json!({"cell": "A5"}))["value"], "Rq3");
    assert_eq!(run(&mut s, "cell.get", json!({"cell": "C2"}))["value"], "x");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(run(&mut s, "cell.get", json!({"cell": "B2"}))["value"], "Zq1");
    assert_eq!(run(&mut s, "cell.get", json!({"cell": "A5"}))["value"], "zq3");
}
