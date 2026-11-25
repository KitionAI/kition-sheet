//! A session a host runs (`Session::set_hosted`; KHP A29, A42, A58, section 10.6; engine contract
//! E22) reaches no file by path itself: a file a command names is read only through the host
//! (`kition_kit::host`, which resolves it inside the workspace and refuses the rest), no command
//! writes a file by path (the host saves and exports workbooks), and nothing of the machine is
//! read (the system word list). A script, which is the workbook's data, asks the host's gate
//! (`kition_kit::dispatch::may_run`) for every step before any runs, so a workbook cannot carry
//! steps its caller may not run.
//!
//! The workspace audit (sheet section) found every by-path command (`file.open`,
//! `data.getData`, `data.fromTextCsv`, `image.insert`, `page.background`, `file.save*`,
//! `file.exportCsv`, `file.exportHtml`, `file.exportPdf`, `file.print`) reading and writing the
//! file system directly, `script.run` running a workbook's stored steps with no check, and
//! spelling reading `/usr/share/dict`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use kdm_model::kition_kit::dispatch::{self, Gate, Refusal, RefusalCode, Role};
use kdm_model::kition_kit::host;
use kition_sheet_engine::Session;
use serde_json::{Value, json};

/// Text only the file outside the workspace holds.
const SECRET: &str = "SENTINEL-5c1e";

/// The lent root is the process's: the tests that set it take turns.
static TURN: Mutex<()> = Mutex::new(());

/// A scratch folder holding `workspace/in.csv` and `outside/secret.csv`, its path canonical.
fn folders(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kition-sheet-hosted-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["workspace", "outside"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("workspace/in.csv"), "region,amount\nNorth,120\n").unwrap();
    std::fs::write(root.join("outside/secret.csv"), format!("region,amount\n{SECRET},1\n")).unwrap();
    root
}

/// A hosted session over a new workbook, its host lending `root/workspace`.
fn hosted(root: &Path) -> Session {
    host::set_root(Some(root.join("workspace")));
    host::set_folder("");
    let mut s = Session::new();
    s.new_workbook();
    s.set_hosted(true);
    s
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
    out.sort();
    out
}

fn at(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// Every sheet of the active workbook as CSV.
fn shown(s: &mut Session) -> String {
    let sheets = s.doc().unwrap().wb.sheets.len();
    (0..sheets)
        .map(|sheet| {
            let out = s.execute("file.export", json!({ "format": "csv", "sheet": sheet })).unwrap();
            String::from_utf8_lossy(&kition_sheet_engine::io::base64_decode(out["data"].as_str().unwrap()).unwrap()).into_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What `cell` holds, as typed.
fn text(s: &mut Session, cell: &str) -> Value {
    s.execute("cell.get", json!({ "cell": cell })).unwrap()["input"].clone()
}

#[test]
fn a_hosted_session_reads_a_file_only_through_its_host() {
    let _turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    let root = folders("reads");
    let outside = root.join("outside");
    let mut s = hosted(&root);
    // A file of the workspace, by its path there: lent by the host.
    s.execute("data.fromTextCsv", json!({ "path": "in.csv" })).unwrap();
    assert!(shown(&mut s).contains("North"), "the workspace's file was read through the host");
    // Outside the workspace, however it is named: refused, nothing read.
    let secret = at(&outside, "secret.csv");
    for path in [secret.clone(), "../outside/secret.csv".to_owned(), format!("file://{secret}"), "/../outside/secret.csv".to_owned()] {
        for command in ["data.fromTextCsv", "data.getData", "file.open", "image.insert", "page.background"] {
            let answer = s.execute(command, json!({ "path": path }));
            assert!(answer.is_err(), "{command} {path} was read: {answer:?}");
        }
    }
    assert_eq!(s.documents().len(), 1, "no workbook was opened from outside the workspace");
    assert!(!shown(&mut s).contains(SECRET), "a file outside the workspace reached the workbook");
    host::set_root(None);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_hosted_session_writes_no_file_by_path() {
    let _turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    let root = folders("writes");
    let outside = root.join("outside");
    let mut s = hosted(&root);
    s.execute("cell.set", json!({ "cell": "A1", "input": "kept in memory" })).unwrap();
    // A relative path names a file of the process's current folder.
    let relative = format!("kition-sheet-hosted-{}.xlsx", std::process::id());
    for (command, path) in [
        ("file.save", at(&outside, "save.xlsx")),
        ("file.saveAs", at(&outside, "save-as.xlsx")),
        ("file.saveAs", "../outside/climb.xlsx".to_owned()),
        ("file.saveAs", relative.clone()),
        ("file.exportCsv", at(&outside, "export")),
        ("file.exportHtml", at(&outside, "export")),
        ("file.exportPdf", at(&outside, "export.pdf")),
        ("file.print", at(&outside, "print.pdf")),
    ] {
        let answer = s.execute(command, json!({ "path": path }));
        assert!(answer.is_err(), "{command} {path} wrote a file: {answer:?}");
    }
    assert_eq!(names(&outside), ["secret.csv"], "nothing was written outside the workspace");
    assert_eq!(names(&root.join("workspace")), ["in.csv"], "nothing was written by path into the workspace either");
    assert!(!Path::new(&relative).exists(), "nothing was written into the current folder");
    // What the host writes comes back as bytes, as before.
    let pdf = s.execute("file.exportPdf", json!({})).unwrap();
    assert!(pdf["base64"].as_str().is_some_and(|data| !data.is_empty()), "{pdf}");
    let xlsx = s.execute("file.export", json!({ "format": "xlsx" })).unwrap();
    assert!(xlsx["data"].as_str().is_some_and(|data| !data.is_empty()));
    assert_eq!(text(&mut s, "A1"), json!("kept in memory"));
    host::set_root(None);
    let _ = std::fs::remove_dir_all(root);
}

/// A gate as kition-headless sets one for the agent: it refuses the commands that name files and
/// records every command it was asked about.
struct NoFiles {
    asked: Mutex<Vec<String>>,
}

impl Gate for NoFiles {
    fn role(&self) -> Role {
        Role::Agent
    }

    fn check(&self, command: &str, _params: &Value) -> Result<(), Refusal> {
        self.asked.lock().unwrap_or_else(PoisonError::into_inner).push(command.to_owned());
        if command.starts_with("file.") || command.starts_with("data.from") {
            Err(Refusal::new(RefusalCode::NotAllowed, command, "commands that name files are not available to the agent"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn a_script_asks_the_hosts_gate_for_every_step_before_any_runs() {
    let root = std::env::temp_dir().join(format!("kition-sheet-hosted-script-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let leak = at(&root, "leak.xlsx");
    let mut s = Session::new();
    s.new_workbook();
    let leaking = json!({ "name": "Leak", "commands": [
        { "command": "cell.set", "params": { "cell": "A1", "input": "first" } },
        { "command": "file.saveAs", "params": { "path": leak } },
        { "command": "cell.set", "params": { "cell": "A2", "input": "last" } },
    ] });
    s.execute("script.new", leaking).unwrap();
    let allowed = json!({ "name": "Fine", "commands": [{ "command": "cell.set", "params": { "cell": "A3", "input": "ran" } }] });
    s.execute("script.new", allowed).unwrap();
    let gate = Arc::new(NoFiles { asked: Mutex::new(Vec::new()) });
    let (refused, fine) = dispatch::with_gate(gate.clone(), || {
        let refused = s.execute("script.run", json!({ "name": "Leak" }));
        let refusal = dispatch::take_refusal();
        let fine = s.execute("script.run", json!({ "name": "Fine" }));
        ((refused, refusal), fine)
    });
    let (refused, refusal) = refused;
    assert!(refused.is_err(), "the script ran a step the gate refused: {refused:?}");
    assert_eq!(refusal.map(|refusal| refusal.command), Some("file.saveAs".to_owned()), "the host answers the gate's refusal");
    assert!(gate.asked.lock().unwrap_or_else(PoisonError::into_inner).iter().any(|command| command == "file.saveAs"));
    assert!(!Path::new(&leak).exists(), "the refused step wrote its file");
    assert_eq!(text(&mut s, "A1"), json!(""), "no step of a refused script ran");
    assert_eq!(text(&mut s, "A2"), json!(""));
    assert!(fine.is_ok(), "a script whose steps the gate allows runs: {fine:?}");
    assert_eq!(text(&mut s, "A3"), json!("ran"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_hosted_session_reads_no_system_word_list() {
    let mut s = Session::new();
    s.new_workbook();
    s.set_hosted(true);
    s.execute("cell.set", json!({ "cell": "A1", "input": "Helllo wrold" })).unwrap();
    let spelling = s.execute("spelling.check", json!({}));
    assert!(spelling.as_ref().is_err_and(|error| error.to_string().contains("isn't available here")), "{spelling:?}");
    assert_eq!(s.execute("spelling.thesaurus", json!({ "word": "running" })).unwrap(), json!([]));
}
