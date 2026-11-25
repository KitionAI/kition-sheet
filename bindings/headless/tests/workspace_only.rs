//! `kition-sheet` reads and writes only inside its workspace, and only through its host (KHP A29,
//! A42, A58, section 10.6): whatever an agent runs, nothing appears in the process's current
//! folder, its home folder or its temporary folder (its private runtime folder, where the kit keeps
//! nothing), nothing outside the workspace is read into an answer, changed or deleted, no program
//! is started, and the workspace holds only what the test put there.
//!
//! A workspace audit (sheet section, from reading the code) found that the engine read
//! and wrote any path the commands that name files are given (`file.open`, `data.getData`,
//! `data.fromTextCsv`, `image.insert`, `page.background`, `file.save`, `file.saveAs`,
//! `file.exportCsv`, `file.exportHtml`, `file.exportPdf`, `file.print`), and that
//! `script.run`, which the agent is offered, ran a workbook's stored steps with no check:
//! the headless kit keeps those commands from the agent and checks `model.batch` command steps,
//! but a script's steps reached them all, and a workbook could carry such a script. Spelling read
//! the machine's word list (`/usr/share/dict`).
//!
//! The test runs itself again as a child process whose current folder, `HOME` and `TMPDIR` are
//! fresh folders and whose `PATH` starts with fake print and opener programs that only record that
//! they ran. A folder beside them (`outside/`) holds this test's sentinel files, which the agent
//! names by path (directly, inside `model.batch` as a script, as the agent and as a person it claims
//! to be, and inside a script of the workbook's own), and which a workbook in the workspace names
//! in its stored scripts. `TMPDIR` holds a planted file too: a path into the private runtime folder
//! is refused like any other path outside the workspace (A58).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_sheet_kit::SheetDocument;
use serde_json::{Map, Value, json};

const MANIFEST: &str = include_str!("../../../kit.json");
/// Set in the child: the folder holding `workspace/`, `outside/` and the process's folders.
const CHILD: &str = "KITION_SHEET_TEST_SANDBOX";
/// Text only files outside the workspace (and a hidden one in it) hold: it must reach no answer.
const SECRET: &str = "SENTINEL-3f9b";
/// Text only the file planted in the private runtime folder holds.
const RUNTIME: &str = "RUNTIME-3f9b";
/// The programs the kit might try to start, faked first on `PATH`.
const FAKES: &[&str] = &["lp", "lpr", "lpstat", "open", "xdg-open", "say"];
/// What the workspace holds: what the test put there, nothing the kit wrote.
const WORKSPACE: &[&str] = &[".hidden.csv", "basic.xlsx", "hostile.xlsx", "link.csv"];

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str) -> Option<Vec<u8>> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = BASE64.iter().position(|known| *known == byte)?;
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// Every file and folder under `dir`, relative to it, with a file's bytes (a link's target).
fn tree(dir: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().into_owned();
            let link = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink());
            if link {
                out.push((name, std::fs::read_link(&path).ok().map(|target| target.to_string_lossy().into_owned().into_bytes())));
            } else if path.is_dir() {
                stack.push(path.clone());
                out.push((name, None));
            } else {
                out.push((name, std::fs::read(&path).ok()));
            }
        }
    }
    out.sort();
    out
}

fn names(dir: &Path) -> Vec<String> {
    tree(dir).into_iter().map(|(name, _)| name).collect()
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn at(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// The paths a command that reads a file is given: outside the workspace every way it can be
/// named, into the private runtime folder, through a link out of the workspace, and a hidden file.
fn reads(root: &Path, command: &str) -> Vec<String> {
    let outside = root.join("outside");
    let sentinel = if command == "file.open" { "planted.xlsx" } else { "secret.csv" };
    vec![
        at(&outside, sentinel),
        format!("../outside/{sentinel}"),
        format!("file://{}", at(&outside, sentinel)),
        at(&root.join("tmp"), "planted.csv"),
        "link.csv".to_owned(),
        ".hidden.csv".to_owned(),
    ]
}

/// The paths a command that writes a file is given: outside the workspace, climbing out of it,
/// a bare name (the process's current folder), the home and the temporary folder.
fn writes(root: &Path, command: &str) -> Vec<String> {
    vec![
        at(&root.join("outside"), &format!("made-{command}")),
        format!("../outside/climb-{command}"),
        format!("made-{command}.xlsx"),
        at(&root.join("home"), &format!("made-{command}")),
        at(&root.join("tmp"), &format!("made-{command}")),
    ]
}

/// A workbook of the kit's own engine with `text` in A1.
fn workbook_with(text: &str) -> Vec<u8> {
    let mut doc = SheetDocument::create("planted.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": text })).unwrap();
    doc.save(None).unwrap()
}

/// A workbook whose stored scripts each run one step naming a file outside the workspace: what
/// an agent runs with `script.run` and nothing else.
fn hostile_xlsx(root: &Path) -> Vec<u8> {
    let mut doc = SheetDocument::create("hostile.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({ "cell": "A1", "input": "Hostile" })).unwrap();
    let mut steps = Vec::new();
    for command in ["data.fromTextCsv", "data.getData", "file.open"] {
        steps.extend(reads(root, command).into_iter().map(|path| (command, path)));
    }
    for command in ["file.saveAs", "file.exportCsv", "file.exportPdf"] {
        steps.extend(writes(root, command).into_iter().map(|path| (command, path)));
    }
    for (index, (command, path)) in steps.iter().enumerate() {
        let script = json!({ "name": format!("Planted {index}"), "commands": [{ "command": command, "params": { "path": path } }] });
        doc.execute("script.new", script).unwrap();
    }
    let bytes = doc.save(None).unwrap();
    // The scripts survive the file: the agent finds them in the workbook it opens.
    let mut reopened = SheetDocument::open("hostile.xlsx", &bytes).unwrap();
    let listed = reopened.execute("script.list", json!({})).unwrap();
    assert_eq!(listed.as_array().map(Vec::len), Some(steps.len()), "the planted workbook keeps its scripts: {listed}");
    bytes
}

/// The workspace's files and the sentinels outside it, made by the parent before the child runs.
fn plant(root: &Path) {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    std::fs::copy(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.xlsx")), workspace.join("basic.xlsx")).unwrap();
    std::fs::write(workspace.join(".hidden.csv"), format!("name,value\n{SECRET},1\n")).unwrap();
    std::fs::write(outside.join("secret.csv"), format!("name,value\n{SECRET},1\n")).unwrap();
    std::fs::write(outside.join("sub/keep.txt"), SECRET).unwrap();
    std::fs::write(outside.join("planted.xlsx"), workbook_with(SECRET)).unwrap();
    std::fs::write(root.join("tmp/planted.csv"), format!("name,value\n{RUNTIME},1\n")).unwrap();
    // A symbolic link in the workspace that leads out of it.
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.join("secret.csv"), workspace.join("link.csv")).unwrap();
    #[cfg(not(unix))]
    std::fs::write(workspace.join("link.csv"), "name,value\nlink,1\n").unwrap();
    std::fs::write(workspace.join("hostile.xlsx"), hostile_xlsx(root)).unwrap();

    // The programs the kit might start: each records that it ran, and does nothing else.
    for fake in FAKES {
        let script = root.join("bin").join(fake);
        std::fs::write(&script, format!("#!/bin/sh\necho \"$0 $*\" >> \"{}\"\n", at(&root.join("marks"), fake))).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
}

/// The agent's session: the backend the executable serves, over the workspace, and every answer
/// it gave.
struct Agent {
    kit: Headless<SheetDocument>,
    folder: HostFolder,
    answers: Vec<(String, Value)>,
    scripts: usize,
}

impl Agent {
    fn open(&mut self, name: &str) -> String {
        self.kit.open(OpenRequest { file: &self.folder.file(name).unwrap(), read_only: false, create: None }).unwrap().id
    }

    fn keep(&mut self, what: String, answer: Result<Value, String>) -> Result<Value, String> {
        self.answers.push((what, answer.clone().unwrap_or_else(Value::String)));
        answer
    }

    /// `command` as the agent calls it directly.
    fn run(&mut self, id: &str, command: &str, value: Value) -> Result<Value, String> {
        let answer =
            self.kit.execute(id, command, params(value), Origin::Agent).map(|done| done.result.unwrap_or(Value::Null)).map_err(|e| format!("{e:?}"));
        self.keep(command.to_owned(), answer)
    }

    /// `ops` as one `model.batch`, with the batch's `origin` (none: the object model's default, a
    /// script; or one the caller claims).
    fn batch(&mut self, id: &str, what: String, ops: Value, origin: &Value) -> Result<Value, String> {
        let mut request = json!({ "documentId": id, "ops": ops });
        if !origin.is_null() {
            request["origin"] = origin.clone();
        }
        let answer = self.kit.model("model.batch", request).map_err(|e| format!("{e:?}"));
        self.keep(format!("model.batch {what} as {origin}"), answer)
    }

    /// `command` as the one step of a script of the kit's own: stored and run by the agent, and
    /// stored and run in one `model.batch` by a person the caller claims to be.
    fn script(&mut self, id: &str, command: &str, value: &Value) {
        let name = format!("Agent script {}", self.scripts);
        self.scripts += 1;
        let steps = json!([{ "command": command, "params": value }]);
        let _ = self.run(id, "script.new", json!({ "name": name, "commands": steps }));
        let _ = self.run(id, "script.run", json!({ "name": name }));
        let name = format!("{name} (batch)");
        let ops = json!([
            { "op": "command", "id": "script.new", "params": { "name": name, "commands": steps } },
            { "op": "command", "id": "script.run", "params": { "name": name } },
        ]);
        let _ = self.batch(id, format!("script.run {command}"), ops, &json!({ "kind": "person", "principal": "person:local" }));
    }

    /// `command` every way an agent reaches it: directly, as one `command` operation of
    /// `model.batch` (as a script, as the agent, as a person it claims to be), and inside a script.
    fn every_way(&mut self, id: &str, command: &str, value: &Value) {
        let _ = self.run(id, command, value.clone());
        for origin in [Value::Null, json!({ "kind": "agent", "principal": "agent:test" }), json!({ "kind": "person", "principal": "person:local" })] {
            let ops = json!([{ "op": "command", "id": command, "params": value }]);
            let _ = self.batch(id, command.to_owned(), ops, &origin);
        }
        self.script(id, command, value);
    }

    /// Every sheet of document `id` as CSV, as the agent exports it, kept as answers.
    fn sheets(&mut self, id: &str) {
        for sheet in 0..12 {
            let _ = self.run(id, "file.export", json!({ "format": "csv", "sheet": sheet }));
        }
    }

    fn model(&mut self, method: &str, request: Value) {
        let answer = self.kit.model(method, request).unwrap_or(Value::Null);
        self.answers.push((method.into(), answer));
    }
}

/// What the agent does in the child. Errors are fine; reaching files outside the workspace is not.
/// Returns what went wrong that the child can see (answers); the parent checks the folders and
/// the fake programs.
fn act(root: &Path) -> Vec<String> {
    let mut wrong = Vec::new();
    let kit = KitInfo::from_manifest(MANIFEST, &[], kition_sheet_kit::EDITOR_STATE).unwrap();
    let mut agent =
        Agent { kit: Headless::with_replica_port(kit), folder: HostFolder::new(&root.join("workspace")).unwrap(), answers: Vec::new(), scripts: 0 };
    agent.kit.set_host(agent.folder.host());
    let id = agent.open("basic.xlsx");
    if agent.run(&id, "cell.set", json!({ "cell": "H1", "input": "Typed by the agent" })).is_err() {
        wrong.push("the agent cannot type in a workbook of its workspace".to_owned());
    }
    // Scripts still run what the agent may run.
    let allowed = json!({ "name": "Allowed", "commands": [{ "command": "cell.set", "params": { "cell": "H2", "input": "By a script" } }] });
    let _ = agent.run(&id, "script.new", allowed);
    if agent.run(&id, "script.run", json!({ "name": "Allowed" })).is_err() {
        wrong.push("a script of commands the agent may run was refused".to_owned());
    }

    // Every command that names a file, with that parameter outside the workspace every way it can
    // be named, every way the agent reaches it.
    let commands = SheetDocument::create("sweep.xlsx", 0, 0).unwrap().commands();
    let mut named = 0;
    for command in &commands {
        let Some(schema) = command.schema.as_ref().and_then(Value::as_object) else { continue };
        for (name, node) in schema.iter().filter(|(_, node)| node["type"] == "file") {
            named += 1;
            let paths = if node["access"] == "read" { reads(root, &command.id) } else { writes(root, &command.id) };
            for path in paths {
                agent.every_way(&id, &command.id, &json!({ name.as_str(): path }));
            }
        }
    }
    if named < 11 {
        wrong.push(format!("only {named} file parameters were found in the commands' schemas"));
    }
    // Saving with no path: the workbook's own name, never a file of this process's folders.
    for command in ["file.save", "file.saveAs"] {
        agent.every_way(&id, command, &json!({}));
    }
    agent.sheets(&id);

    // A workbook of the workspace whose stored scripts name files outside it: the agent runs each.
    let hostile = agent.open("hostile.xlsx");
    let listed = agent.run(&hostile, "script.list", json!({})).unwrap_or_default();
    let planted: Vec<String> = listed.as_array().into_iter().flatten().filter_map(|script| script["name"].as_str().map(str::to_owned)).collect();
    if planted.is_empty() {
        wrong.push(format!("hostile.xlsx: its scripts are not there: {listed}"));
    }
    for name in &planted {
        let ran = agent.run(&hostile, "script.run", json!({ "name": name }));
        if ran.is_ok() {
            wrong.push(format!("hostile.xlsx: the agent ran `{name}`, a step naming a file outside the workspace"));
        }
        let ops = json!([{ "op": "command", "id": "script.run", "params": { "name": name } }]);
        let _ = agent.batch(&hostile, format!("script.run {name}"), ops, &json!({ "kind": "person", "principal": "person:local" }));
    }
    agent.sheets(&hostile);

    // Spelling reads no word list of the machine.
    let _ = agent.run(&hostile, "cell.set", json!({ "cell": "B2", "input": "Helllo wrold" }));
    if agent.run(&hostile, "spelling.check", json!({})).is_ok() {
        wrong.push("spelling.check checked against a word list of the machine".to_owned());
    }

    // Every command the kit offers the agent, with no parameters, on both workbooks.
    for doc in [&id, &hostile] {
        let offered: Vec<String> = agent.kit.commands(Some(doc.as_str())).unwrap().into_iter().map(|command| command.id).collect();
        for command in offered {
            if command == "file.close" {
                continue;
            }
            let _ = agent.run(doc, &command, json!({}));
        }
        agent.sheets(doc);
        agent.model("model.export", json!({ "documentId": doc }));
    }

    // Nothing outside the workspace reached an answer: its files' text, the private runtime
    // folder's planted file. (Paths the agent gave may come back in errors.)
    let leaks: [&[u8]; 2] = [SECRET.as_bytes(), RUNTIME.as_bytes()];
    for (what, answer) in &agent.answers {
        let mut texts = Vec::new();
        strings(answer, &mut texts);
        for text in texts {
            for leak in leaks {
                if text.windows(leak.len()).any(|window| window == leak) {
                    wrong.push(format!("{what}: an answer holds `{}` from outside the workspace", String::from_utf8_lossy(leak)));
                }
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    wrong
}

/// Every string in `value`, and the bytes of each that reads as base64.
fn strings(value: &Value, out: &mut Vec<Vec<u8>>) {
    match value {
        Value::String(text) => {
            out.push(text.as_bytes().to_vec());
            if text.len() >= 16
                && let Some(bytes) = decode(text)
            {
                out.push(bytes);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(fields) => fields.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[test]
fn agent_commands_reach_no_file_outside_the_workspace() {
    if let Some(root) = std::env::var_os(CHILD) {
        let wrong = act(&PathBuf::from(root));
        assert!(wrong.is_empty(), "{wrong:#?}");
        return;
    }
    let root = std::env::temp_dir().join(format!("kition-sheet-workspace-only-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["workspace", "cwd", "home", "tmp", "outside/sub", "bin", "marks"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let root = std::fs::canonicalize(root).unwrap();
    plant(&root);
    let outside = tree(&root.join("outside"));
    let home = tree(&root.join("home"));
    let tmp = tree(&root.join("tmp"));
    let workspace = tree(&root.join("workspace"));

    let path = format!("{}:/usr/bin:/bin", root.join("bin").display());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "agent_commands_reach_no_file_outside_the_workspace", "--nocapture", "--test-threads=1"])
        .current_dir(root.join("cwd"))
        .env(CHILD, &root)
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("tmp"))
        .env("XDG_CONFIG_HOME", root.join("home/.config"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("XDG_CACHE_HOME", root.join("home/.cache"))
        .env("PATH", path)
        .status()
        .unwrap();
    // A program the child started may still be recording that it ran.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut wrong = Vec::new();
    if !status.success() {
        wrong.push(format!("the agent's session found something outside the workspace (see its output above): {status}"));
    }
    let found = names(&root.join("cwd"));
    if !found.is_empty() {
        wrong.push(format!("the kit wrote into its current folder: {found:?}"));
    }
    if tree(&root.join("home")) != home {
        wrong.push(format!("the kit wrote into its home folder: {:?}", names(&root.join("home"))));
    }
    // The kit keeps no runtime state, so even its private runtime folder (A58) stays as planted.
    if tree(&root.join("tmp")) != tmp {
        wrong.push(format!("the kit wrote into its temporary folder: {:?}", names(&root.join("tmp"))));
    }
    let started = names(&root.join("marks"));
    if !started.is_empty() {
        wrong.push(format!("the kit started programs: {started:?}"));
    }
    let now = tree(&root.join("outside"));
    if now != outside {
        let before: Vec<&String> = outside.iter().map(|(name, _)| name).collect();
        let after: Vec<&String> = now.iter().map(|(name, _)| name).collect();
        wrong.push(format!("the folder outside the workspace changed: before {before:?}, after {after:?}"));
    }
    let found = names(&root.join("workspace"));
    if found != WORKSPACE {
        wrong.push(format!("the workspace holds {found:?}, not only what the test put there ({WORKSPACE:?})"));
    } else if tree(&root.join("workspace")) != workspace {
        wrong.push("a file in the workspace changed".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let _ = std::fs::remove_dir_all(&root);
}
