//! Every command declares the editor state it reads (`KitCommand::reads`): what the engine's code
//! reads, found by reading it. Each command's `enabled` predicate and `run` function (or closure)
//! are followed through the engine's functions and the commands it runs (`execute("…")`) to the
//! accessors of the selection (the selected cells and drawing objects), of the active sheet (the
//! call's part) and of the editing mode, the view mode and the Draw tab's pen (`mode`); a command
//! that reaches one and does not declare it, or declares what it never reads, fails here (task card
//! C4/C5/C6-sheet). The commands the kit's binding adds are declared in `acts.rs` by hand and listed
//! there. To see how the scan
//! reached a reading, set `ACTS_DEBUG=<command id>` or `ACTS_FN=<engine function>`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use kition_kit::{EditorState, KitDocument};
use kition_sheet_kit::SheetDocument;

/// Code that reads the selection: the selected cells (the document's selection, an edit's copy of
/// it, the ranges a command defaults to) and the selected drawing objects.
const SELECTION: &[&str] = &[".selection", ".sel", "selected_shape", "target_ranges(", "target_range("];
/// Code that reads the active sheet, a call's part: the workbook's active sheet and the helpers
/// that default to it.
const PART: &[&str] = &["wb.active()", ".active_sheet", "sheet_index()", "target_sheet(", "active_sheet(", "DocState::new((*d.wb).clone()"];
/// Code that reads the editor's modes: the editing mode, the view mode and the Draw tab's pen.
const MODE: &[&str] = &["s.mode", "self.mode", ".view_mode", ".draw_tool", ".draw_color", ".draw_width", ".draw_shape"];
/// `enabled` predicates that gate on the workbook alone (open, has history, a clipboard), not on
/// what is active or selected.
const DOCUMENT_GATES: &[&str] = &["always", "has_doc", "can_undo", "can_redo", "has_clipboard"];
/// The engine's plumbing every edit passes through, which carries the selection and the active
/// sheet along (to restore them on undo, to switch sheets) without a command reading them.
const PLUMBING: &[&str] =
    &["commit", "edit", "execute", "execute_command", "execute_headless", "run", "wb_switch_sheet", "sync_all_kdm", "push_undo"];

fn engine_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/engine/engine/src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path.file_name().is_some_and(|n| n == "tests.rs" || n.to_string_lossy().ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
}

/// `source` with every comment, string and character literal blanked (to spaces), so brackets and
/// names are only the code's; same length, byte for byte.
fn code_only(source: &str) -> Vec<u8> {
    let b = source.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for byte in out.iter_mut().take(to).skip(from) {
            *byte = b' ';
        }
    };
    while i < b.len() {
        let ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if b[i..].starts_with(b"//") {
            let end = b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |n| i + n);
            blank(&mut out, i, end);
            i = end;
        } else if b[i..].starts_with(b"/*") {
            let end = source[i + 2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2);
            blank(&mut out, i, end);
            i = end;
        } else if !ident_before && (b[i] == b'r' || b[i..].starts_with(b"br")) && {
            let start = if b[i] == b'b' { i + 2 } else { i + 1 };
            let hashes = b[start..].iter().take_while(|&&c| c == b'#').count();
            b.get(start + hashes) == Some(&b'"')
        } {
            let start = if b[i] == b'b' { i + 2 } else { i + 1 };
            let hashes = b[start..].iter().take_while(|&&c| c == b'#').count();
            let close = format!("\"{}", "#".repeat(hashes));
            let body = start + hashes + 1;
            let end = source[body..].find(&close).map_or(b.len(), |n| body + n + close.len());
            blank(&mut out, i, end);
            i = end;
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            blank(&mut out, i, (j + 1).min(b.len()));
            i = j + 1;
        } else if b[i] == b'\'' {
            // A character literal ('a', '\n', '{', 'é'), not a lifetime ('a).
            let rest = &source[i + 1..];
            let len = if let Some(escaped) = rest.strip_prefix('\\') {
                escaped.find('\'').map(|n| n + 2)
            } else {
                rest.chars().next().map(char::len_utf8).filter(|&n| rest[n..].starts_with('\''))
            };
            match len {
                Some(len) => {
                    blank(&mut out, i, i + len + 2);
                    i += len + 2;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    out
}

/// The position after the bracket matching the one at `open`.
fn matching(code: &[u8], open: usize) -> usize {
    let (left, right) = match code[open] {
        b'(' => (b'(', b')'),
        b'{' => (b'{', b'}'),
        _ => (b'[', b']'),
    };
    let mut depth = 0usize;
    for (at, &c) in code.iter().enumerate().skip(open) {
        if c == left {
            depth += 1;
        } else if c == right {
            depth -= 1;
            if depth == 0 {
                return at + 1;
            }
        }
    }
    code.len()
}

/// The source cut before its `#[cfg(test)]` module, the code of what is left, and its file.
struct File {
    source: String,
    code: Vec<u8>,
    name: String,
}

fn files() -> Vec<File> {
    let mut paths = Vec::new();
    rust_files(&engine_src(), &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let mut source = std::fs::read_to_string(&path).unwrap();
            if let Some(at) = source.find("#[cfg(test)]\nmod ") {
                source.truncate(at);
            }
            let code = code_only(&source);
            File { source, code, name: path.to_string_lossy().into_owned() }
        })
        .collect()
}

fn ident_at(code: &[u8], at: usize) -> &str {
    let end = code[at..].iter().position(|c| !(c.is_ascii_alphanumeric() || *c == b'_')).map_or(code.len(), |n| at + n);
    std::str::from_utf8(&code[at..end]).unwrap_or("")
}

/// What a piece of code reads directly, the free functions it calls, and the commands it runs.
#[derive(Default, Clone)]
struct Reach {
    reads: BTreeSet<&'static str>,
    calls: BTreeSet<String>,
    runs: BTreeSet<String>,
}

fn local_closures(code: &[u8]) -> Vec<(String, usize, usize)> {
    let text = std::str::from_utf8(code).unwrap_or("");
    let mut found = Vec::new();
    for (at, _) in text.match_indices("let ") {
        if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
            continue;
        }
        let mut start = at + 4;
        while code.get(start).is_some_and(u8::is_ascii_whitespace) {
            start += 1;
        }
        if text[start..].starts_with("mut ") {
            start += 4;
        }
        let name = ident_at(code, start);
        if name.is_empty() {
            continue;
        }
        let rest = text[start + name.len()..].trim_start();
        let Some(value) = rest.strip_prefix('=').map(str::trim_start) else { continue };
        let value = value.strip_prefix("move ").map_or(value, str::trim_start);
        if !value.starts_with('|') {
            continue;
        }
        let mut scopes = Vec::new();
        for (index, byte) in code.iter().enumerate().take(at) {
            match byte {
                b'{' => scopes.push(index),
                b'}' => {
                    scopes.pop();
                }
                _ => {}
            }
        }
        let end = scopes.last().map_or(code.len(), |open| matching(code, *open));
        found.push((name.to_owned(), start, end));
    }
    found
}

fn non_delete_call(name: &str, code: &[u8], source: &str, open: usize) -> bool {
    if name != "apply" {
        return false;
    }
    let end = matching(code, open);
    let args = split_args(code, open + 1, end.saturating_sub(1));
    let Some((from, to)) = args.get(1) else { return false };
    let argument = source[*from..*to].trim();
    argument.starts_with("\"drawing.") && argument.ends_with('"') && argument != "\"drawing.delete\""
}

fn reach(code: &[u8], source: &str) -> Reach {
    let text = std::str::from_utf8(code).unwrap_or("");
    let mut found = Reach::default();
    let has = |token: &str| {
        text.match_indices(token).any(|(at, _)| {
            let end = at + token.len();
            // `s.mode` is not `cells.mode`.
            let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
            if token.as_bytes().first().is_some_and(|c| ident(*c)) && at > 0 && ident(code[at - 1]) {
                return false;
            }
            // `selection.slide` is not `selection.slides`.
            let next = code.get(end).copied().unwrap_or(b' ');
            let whole = token.ends_with('(') || token.ends_with(')') || !(next.is_ascii_alphanumeric() || next == b'_');
            // Setting it (`sel.shapes = vec![id]`, `sel.shapes.clear()`) is no read.
            let rest = text[end..].trim_start();
            let written = (rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>"))
                || [".clear()", ".push(", ".retain(", ".extend(", ".truncate(", ".dedup("].iter().any(|write| rest.starts_with(write));
            whole && !written
        })
    };
    if SELECTION.iter().any(|token| has(token)) {
        found.reads.insert("selection");
    }
    if PART.iter().any(|token| has(token)) {
        found.reads.insert("part");
    }
    if MODE.iter().any(|token| has(token)) {
        found.reads.insert("mode");
    }
    let closures = local_closures(code);
    for at in 0..code.len() {
        let starts =
            (code[at].is_ascii_alphabetic() || code[at] == b'_') && (at == 0 || !(code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_'));
        if !starts {
            continue;
        }
        let name = ident_at(code, at);
        let after = at + name.len();
        let method = at > 0 && code[at - 1] == b'.';
        if code.get(after) == Some(&b'(') && !method && in_engine(code, at) {
            let qualified = at >= 2 && &code[at - 2..at] == b"::";
            let local = !qualified && closures.iter().any(|(bound, from, to)| bound == name && *from < at && at < *to);
            if !local {
                let path = std::str::from_utf8(&code[call_start(code, at)..after]).unwrap_or(name);
                let call = if non_delete_call(name, code, source, after) { format!("{path}#non-delete") } else { path.to_owned() };
                found.calls.insert(call);
            }
        }
        // `s.execute("shape.rotate", …)`: the command runs another, and reads what it reads.
        if (name == "execute" || name == "execute_unguarded") && code.get(after) == Some(&b'(') {
            let rest = &source[after + 1..];
            let rest = rest.trim_start();
            if let Some(stripped) = rest.strip_prefix('"')
                && let Some(end) = stripped.find('"')
            {
                let id = &stripped[..end];
                let suffix = fixed_delegate(id, code, source, after);
                found.runs.insert(format!("{id}{suffix}"));
            } else {
                // A script can dispatch any stored command before changing context.
                found.reads.extend(["selection", "part", "mode"]);
            }
        }
    }
    found
}

// Only specialize literal top-level json! members. Dynamic payloads keep the
// full union, including null/invalid cells and array-entry selection.
fn fixed_delegate(id: &str, code: &[u8], source: &str, open: usize) -> &'static str {
    let end = matching(code, open);
    let args = split_args(code, open + 1, end - 1);
    let Some(&(from, to)) = args.get(1) else { return "" };
    let body = source[from..to].trim();
    if !body.starts_with("json!(") {
        return "";
    }
    let Some(brace) = code[from..to].iter().position(|byte| *byte == b'{').map(|at| from + at) else { return "" };
    let close = matching(code, brace);
    let mut members = BTreeMap::new();
    for (from, to) in split_args(code, brace + 1, close - 1) {
        let Some(colon) = code[from..to].iter().position(|byte| *byte == b':').map(|at| from + at) else { continue };
        let Ok(key) = serde_json::from_str::<String>(source[from..colon].trim()) else { continue };
        members.insert(key, source[colon + 1..to].trim());
    }
    if id == "edit.replace" && members.get("all") == Some(&"true") {
        return "#all";
    }
    if id == "cell.set" && !members.contains_key("array") {
        let valid = members.get("cell").is_some_and(|value| {
            value.ends_with(".a1()")
                || serde_json::from_str::<String>(value).ok().is_some_and(|text| kition_sheet_engine::core::CellRef::parse(&text).is_some())
        });
        if valid {
            return "#cell";
        }
    }
    ""
}

fn private_session_factory(params: &[u8], body: &[u8]) -> bool {
    let params = std::str::from_utf8(params).unwrap_or("").split_whitespace().collect::<String>();
    let plain_inputs =
        params.split(',').filter(|arg| !arg.is_empty()).all(|arg| arg.split_once(':').is_some_and(|(_, ty)| matches!(ty, "&str" | "&[u8]")));
    let body = std::str::from_utf8(body).unwrap_or("").split_whitespace().collect::<String>();
    plain_inputs && body.contains("Session::new()")
}

fn without_delete_branch(code: &[u8], source: &str) -> Option<Reach> {
    let compact = std::str::from_utf8(code).unwrap_or("").split_whitespace().collect::<String>();
    if !compact.contains("DrawingOp::parse(cmd,p)") || !compact.contains("letdelete=matches!(op,DrawingOp::Delete(_));") {
        return None;
    }
    let text = std::str::from_utf8(code).unwrap_or("");
    let mut specialized = code.to_vec();
    for (at, _) in text.match_indices("if delete") {
        let after = at + "if delete".len();
        if code.get(after).is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_') {
            continue;
        }
        let Some(open) = code[after..].iter().position(|byte| !byte.is_ascii_whitespace()).map(|index| after + index) else { continue };
        if code[open] == b'{' {
            specialized[at..matching(code, open)].fill(b' ');
        }
    }
    Some(reach(&specialized, source))
}

/// Whether the call at `at` is to the engine's own code: unqualified, or through `crate`, `super`,
/// `self` or one of its modules, not `std::fs::rename` or another crate's function.
fn in_engine(code: &[u8], at: usize) -> bool {
    let start = call_start(code, at);
    if start == at {
        return true;
    }
    let first = ident_at(code, start);
    [
        "crate",
        "super",
        "self",
        "cmd",
        "edit",
        "format",
        "sheet",
        "insert",
        "data",
        "formulas",
        "review",
        "view",
        "inspect",
        "extra",
        "draw",
        "ink",
        "drawing",
        "spelling",
        "pivot",
        "whatif",
        "print",
        "file",
        "chart_spec",
        "tables",
        "fill",
        "cf",
        "display",
        "selection",
        "kdm_bridge",
        "io",
        "metrics",
        "sample",
        "shape_text",
        "text_format",
        "text_layout",
        "html_table",
        "context",
        "catalog",
    ]
    .contains(&first)
}

fn call_start(code: &[u8], at: usize) -> usize {
    let mut start = at;
    while start >= 2 && &code[start - 2..start] == b"::" {
        let mut from = start - 2;
        while from > 0 && (code[from - 1].is_ascii_alphanumeric() || code[from - 1] == b'_') {
            from -= 1;
        }
        start = from;
    }
    start
}

/// A `cmd!` argument: a closure or call, or a function given by name (`has_selection`, `order`).
fn given(code: &[u8], source: &str) -> Reach {
    let mut found = reach(code, source);
    let name = std::str::from_utf8(code).unwrap_or("").trim();
    if !name.is_empty()
        && name.split("::").all(|segment| !segment.is_empty() && segment.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
        && let Some(last) = name.rsplit("::").next()
        && let Some(at) = name.rfind(last)
        && in_engine(name.as_bytes(), at)
    {
        found.calls.insert(name.to_owned());
    }
    found
}

/// Every `fn` body, by name (a name defined in several files keeps each, by file).
fn functions(files: &[File]) -> BTreeMap<String, Vec<(String, Reach)>> {
    let mut out: BTreeMap<String, Vec<(String, Reach)>> = BTreeMap::new();
    for file in files {
        let code = &file.code;
        let text = std::str::from_utf8(code).unwrap_or("");
        for (at, _) in text.match_indices("fn ") {
            if at > 0 && (code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_') {
                continue;
            }
            let name = ident_at(code, at + 3).to_owned();
            if name.is_empty() {
                continue;
            }
            let Some(open) = code[at..].iter().position(|&c| c == b'(').map(|n| at + n) else { continue };
            let params_end = matching(code, open);
            let Some(brace) = code[params_end..].iter().position(|&c| c == b'{' || c == b';').map(|n| params_end + n) else { continue };
            if code[brace] == b';' {
                continue;
            }
            let end = matching(code, brace);
            let body = if PLUMBING.contains(&name.as_str()) || private_session_factory(&code[open + 1..params_end - 1], &code[brace..end]) {
                Reach::default()
            } else {
                reach(&code[brace..end], &file.source[brace..end])
            };
            if name == "apply"
                && let Some(specialized) = without_delete_branch(&code[brace..end], &file.source[brace..end])
            {
                out.entry(format!("{name}#non-delete")).or_default().push((file.name.clone(), specialized));
            }
            out.entry(name).or_default().push((file.name.clone(), body));
        }
    }
    out
}

/// One `cmd!(…)` entry: its id, its `enabled` and `run` code, and its file.
struct Spec {
    id: String,
    enabled: Reach,
    run: Reach,
    file: String,
}

fn split_args(code: &[u8], from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut args = Vec::new();
    let (mut start, mut at) = (from, from);
    while at < to {
        match code[at] {
            b'(' | b'[' | b'{' => at = matching(code, at),
            b',' => {
                args.push((start, at));
                at += 1;
                start = at;
            }
            _ => at += 1,
        }
    }
    if code[start..to].iter().any(|c| !c.is_ascii_whitespace()) {
        args.push((start, to));
    }
    args
}

fn specs(files: &[File]) -> Vec<Spec> {
    let mut out = Vec::new();
    for file in files {
        let code = &file.code;
        let text = std::str::from_utf8(code).unwrap_or("");
        for (at, _) in text.match_indices("cmd!(") {
            let open = at + 4;
            let end = matching(code, open);
            let args = split_args(code, open + 1, end - 1);
            if args.len() < 7 {
                continue;
            }
            let (id_from, id_to) = args[0];
            let id_source = &file.source[id_from..id_to];
            let Some(id) = id_source.split('"').nth(1) else { continue };
            let (enabled_from, enabled_to) = args[5];
            let enabled_name = std::str::from_utf8(&code[enabled_from..enabled_to]).unwrap_or("").trim().to_owned();
            let enabled = if DOCUMENT_GATES.contains(&enabled_name.as_str()) {
                Reach::default()
            } else {
                given(&code[enabled_from..enabled_to], &file.source[enabled_from..enabled_to])
            };
            // A closure's `|s, p|` holds a comma: the run is everything after `enabled`.
            let (run_from, run_to) = (args[6].0, args[args.len() - 1].1);
            let run = given(&code[run_from..run_to], &file.source[run_from..run_to]);
            out.push(Spec { id: id.to_owned(), enabled, run, file: file.name.clone() });
        }
    }
    out
}

/// What every command reads, through the functions and commands it reaches.
fn derived() -> BTreeMap<String, BTreeSet<&'static str>> {
    let mut derived = derived_from(files());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    let source = std::fs::read_to_string(path).unwrap();
    let code = code_only(&source);
    // The PNG binding wrapper is outside the engine scan. Verify its actual call
    // chain, then derive the state from those bodies rather than prescribing it.
    let body = |name: &str| {
        let declaration = format!("fn {name}(");
        let at = source.find(&declaration).unwrap_or_else(|| panic!("binding method {name} disappeared"));
        let params = at + declaration.len() - 1;
        let params_end = matching(&code, params);
        let open = params_end + code[params_end..].iter().position(|byte| *byte == b'{').unwrap();
        let end = matching(&code, open);
        (&code[open..end], &source[open..end])
    };
    let (run, run_source) = body("run_command");
    let branch = run_source.find("if command == \"file.export\"").unwrap();
    let open = branch + run[branch..].iter().position(|byte| *byte == b'{').unwrap();
    let end = matching(run, open);
    assert!(run_source[branch..end].contains("self.export_picture(&params)"), "file.export's binding dispatch changed");
    let mut reads = BTreeSet::new();
    for (method, next) in [("export_picture", "extent"), ("extent", "active"), ("active", "")] {
        let (code, source) = body(method);
        if !next.is_empty() {
            assert!(std::str::from_utf8(code).unwrap().contains(&format!("self.{next}()")), "binding call {method} -> {next} changed");
        }
        reads.extend(reach(code, source).reads);
    }
    derived.entry("file.export".into()).or_default().extend(reads);
    derived
}

fn definitions<'a>(name: &'a str, file: &str, functions: &'a BTreeMap<String, Vec<(String, Reach)>>) -> Option<(&'a str, Vec<&'a (String, Reach)>)> {
    let (path, leaf) = name.rsplit_once("::").unwrap_or(("", name));
    let key = if functions.contains_key(leaf) { leaf } else { leaf.strip_suffix("#non-delete").unwrap_or(leaf) };
    let defs = functions.get(key)?;
    let module = path.rsplit("::").next().filter(|module| !module.is_empty() && !["self", "super", "crate"].contains(module));
    let qualified: Vec<_> = defs
        .iter()
        .filter(|(file, _)| {
            module.is_some_and(|module| {
                let path = Path::new(file);
                path.file_stem().is_some_and(|stem| stem == module)
                    || path.file_name().is_some_and(|name| name == "mod.rs")
                        && path.parent().and_then(Path::file_name).is_some_and(|name| name == module)
            })
        })
        .collect();
    if !qualified.is_empty() {
        return Some((key, qualified));
    }
    let own: Vec<_> = defs.iter().filter(|(source, _)| source == file).collect();
    Some((key, if own.is_empty() { defs.iter().collect() } else { own }))
}

fn derived_from(files: Vec<File>) -> BTreeMap<String, BTreeSet<&'static str>> {
    let functions = functions(&files);
    let specs = specs(&files);
    // A function's reads, to a fixed point over the calls (a name defined in several files is the
    // caller's own file's when it has one, else every one of them).
    let mut reads: BTreeMap<(String, String), BTreeSet<&'static str>> = BTreeMap::new();
    let mut runs: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for (name, defs) in &functions {
        for (file, body) in defs {
            reads.insert((name.clone(), file.clone()), body.reads.clone());
            runs.insert((name.clone(), file.clone()), body.runs.clone());
        }
    }
    let resolve = |name: &str, file: &str, reads: &BTreeMap<(String, String), BTreeSet<&'static str>>| -> BTreeSet<&'static str> {
        let Some((key, chosen)) = definitions(name, file, &functions) else { return BTreeSet::new() };
        chosen.iter().flat_map(|(f, _)| reads.get(&(key.to_owned(), f.clone())).cloned().unwrap_or_default()).collect()
    };
    let resolve_runs = |name: &str, file: &str, runs: &BTreeMap<(String, String), BTreeSet<String>>| -> BTreeSet<String> {
        let Some((key, chosen)) = definitions(name, file, &functions) else { return BTreeSet::new() };
        chosen.iter().flat_map(|(f, _)| runs.get(&(key.to_owned(), f.clone())).cloned().unwrap_or_default()).collect()
    };
    loop {
        let mut changed = false;
        for (name, defs) in &functions {
            for (file, body) in defs {
                let mut now = reads.get(&(name.clone(), file.clone())).cloned().unwrap_or_default();
                let mut now_runs = runs.get(&(name.clone(), file.clone())).cloned().unwrap_or_default();
                for call in &body.calls {
                    if call != name {
                        now.extend(resolve(call, file, &reads));
                        now_runs.extend(resolve_runs(call, file, &runs));
                    }
                }
                if reads.get(&(name.clone(), file.clone())) != Some(&now) {
                    reads.insert((name.clone(), file.clone()), now);
                    changed = true;
                }
                if runs.get(&(name.clone(), file.clone())) != Some(&now_runs) {
                    runs.insert((name.clone(), file.clone()), now_runs);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    if let Ok(wanted) = std::env::var("ACTS_FN") {
        for (file, body) in functions.get(&wanted).into_iter().flatten() {
            eprintln!("fn {wanted} in {file}: direct {:?}", body.reads);
            for call in &body.calls {
                eprintln!("  calls {call}: {:?}", resolve(call, file, &reads));
            }
        }
    }
    let mut commands: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    let mut command_runs = BTreeMap::new();
    for spec in &specs {
        let mut now: BTreeSet<&'static str> = spec.enabled.reads.iter().chain(&spec.run.reads).copied().collect();
        let mut nested = spec.run.runs.clone();
        for call in spec.enabled.calls.iter().filter(|call| !DOCUMENT_GATES.contains(&call.as_str())).chain(&spec.run.calls) {
            now.extend(resolve(call, &spec.file, &reads));
            nested.extend(resolve_runs(call, &spec.file, &runs));
        }
        if std::env::var("ACTS_DEBUG").is_ok_and(|id| id == spec.id) {
            for call in spec.enabled.calls.iter().chain(&spec.run.calls) {
                eprintln!("{} calls {call}: {:?}", spec.id, resolve(call, &spec.file, &reads));
            }
        }
        commands.insert(spec.id.clone(), now);
        command_runs.insert(spec.id.clone(), nested);
    }
    // Commands that run other commands read what those read.
    loop {
        let mut changed = false;
        for spec in &specs {
            let mut now = commands[&spec.id].clone();
            for other in &command_runs[&spec.id] {
                let (id, fixed) = other.split_once('#').unwrap_or((other, ""));
                let mut nested = commands.get(id).cloned().unwrap_or_default();
                if matches!((id, fixed), ("edit.replace", "all") | ("cell.set", "cell")) {
                    nested.remove("selection");
                }
                now.extend(nested);
            }
            if commands[&spec.id] != now {
                commands.insert(spec.id.clone(), now);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    commands
}

#[test]
fn fixed_delegate_parameters_exclude_only_unreachable_selection_defaults() {
    for (source, suffix) in [
        (r#"s.execute("cell.set", json!({"cell":c.a1(),"input":text}))"#, "cell.set#cell"),
        (r#"s.execute("cell.set", json!({"cell":"C3","array":true}))"#, "cell.set"),
        (r#"s.execute("cell.set", json!({"cell":null}))"#, "cell.set"),
        (r#"s.execute("cell.set", json!({"cell":p.get("cell")}))"#, "cell.set"),
        (r#"s.execute("edit.replace", json!({"all":true,"what":word}))"#, "edit.replace#all"),
        (r#"s.execute("edit.replace", json!({"all":p.get("all")}))"#, "edit.replace"),
        (r#"s.execute("edit.replace", json!({"nested":{"all":true}}))"#, "edit.replace"),
    ] {
        assert_eq!(reach(&code_only(source), source).runs, BTreeSet::from([suffix.to_owned()]));
    }
    let source = "crate::DocState::new((*d.wb).clone(), d.path.clone(), title)";
    assert!(reach(&code_only(source), source).reads.contains("part"));
}

fn name(state: EditorState) -> &'static str {
    state.as_str()
}

#[test]
fn local_closures_keep_their_reads_without_resolving_unrelated_global_functions() {
    let source = "{ let add = move |d| d.wb.active(); add(s); ink::add(s); }";
    let reached = reach(&code_only(source), source);
    assert_eq!(reached.reads, BTreeSet::from(["part"]));
    assert!(reached.calls.contains("ink::add"), "the qualified call remains global");
    assert!(!reached.calls.contains("add"), "the unqualified local closure does not resolve globally");
    let scoped = "{ { let add = || s.selection; add(); } add(); }";
    let reached = reach(&code_only(scoped), scoped);
    assert_eq!(reached.reads, BTreeSet::from(["selection"]));
    assert!(reached.calls.contains("add"), "the outer call is beyond the closure's lexical scope");
}

#[test]
fn private_session_construction_excludes_only_plain_input_factories() {
    let factory = "{ let mut s = Session::new(); s.execute(\"select.move\", p); }";
    assert!(private_session_factory(b"name: &str, bytes: &[u8]", &code_only(factory)));
    assert!(!private_session_factory(b"s: &mut Session", &code_only(factory)));
    assert!(!private_session_factory(b"wb: &Workbook", &code_only(factory)));
    assert!(!private_session_factory(b"&mut self", &code_only(factory)));
    assert!(!private_session_factory(b"name: &str", b"{ s.selection.active; }"));
    let source = r#"
        fn factory(name: &str) { let mut s = Session::new(); s.execute("select.move", p); }
        fn existing(s: &mut Session) { let mut local = Session::new(); s.selection.active; }
        cmd!("factory", "Factory", [], None, "{}", always, factory);
        cmd!("existing", "Existing", [], None, "{}", always, existing);
    "#;
    let reads = derived_from(vec![File { source: source.into(), code: code_only(source), name: "factory.rs".into() }]);
    assert!(reads["factory"].is_empty());
    assert_eq!(reads["existing"], BTreeSet::from(["selection"]));
}

#[test]
fn a_qualified_factory_call_does_not_union_another_modules_same_named_function() {
    let factory = "fn build(name: &str) { let mut s = Session::new(); s.selection.active; }";
    let existing = "fn build(s: &Session) { s.doc().wb.active(); }";
    let commands = r#"
        cmd!("factory", "Factory", [], None, "{}", always, |s,p| sample::build(p));
        cmd!("existing", "Existing", [], None, "{}", always, |s,p| crate::build(s));
    "#;
    let reads = derived_from(vec![
        File { source: factory.into(), code: code_only(factory), name: "sample.rs".into() },
        File { source: existing.into(), code: code_only(existing), name: "lib.rs".into() },
        File { source: commands.into(), code: code_only(commands), name: "file.rs".into() },
    ]);
    assert!(reads["factory"].is_empty());
    assert_eq!(reads["existing"], BTreeSet::from(["part"]));
}

#[test]
fn a_qualified_call_to_a_same_named_function_is_not_self_recursion() {
    let handler = r#"
        fn save_bytes(s: &Session) { crate::io::save_bytes(&s.doc().wb, "x.csv"); }
        cmd!("save", "Save", [], None, "{}", has_doc, save_bytes);
    "#;
    let serializer = "fn save_bytes(wb: &Workbook, path: &str) { let sheet = wb.active_sheet; wb.sheet(sheet); }";
    let reads = derived_from(vec![
        File { source: handler.into(), code: code_only(handler), name: "file.rs".into() },
        File { source: serializer.into(), code: code_only(serializer), name: "io.rs".into() },
    ]);
    assert_eq!(reads["save"], BTreeSet::from(["part"]));
}

#[test]
fn a_static_non_delete_drawing_call_keeps_unconditional_reads() {
    let source = r#"
        fn apply(s: &mut Session, cmd: &str, p: &Json) {
            let op = DrawingOp::parse(cmd, p);
            s.doc().wb.active();
            let delete = matches!(op, DrawingOp::Delete(_));
            if delete { s.selected_shape.is_some(); }
            s.mode;
        }
        cmd!("drawing.text", "Text", [], None, "{}", has_doc, |s,p| apply(s, "drawing.text", p));
        cmd!("drawing.delete", "Delete", [], None, "{}", has_doc, |s,p| apply(s, "drawing.delete", p));
    "#;
    let reads = derived_from(vec![File { source: source.into(), code: code_only(source), name: "drawing.rs".into() }]);
    assert_eq!(reads["drawing.text"], BTreeSet::from(["part", "mode"]));
    assert_eq!(reads["drawing.delete"], BTreeSet::from(["part", "selection", "mode"]));
    let without_discriminator = source.replace("let delete = matches!(op, DrawingOp::Delete(_));", "let delete = unknown();");
    let conservative =
        derived_from(vec![File { source: without_discriminator.clone(), code: code_only(&without_discriminator), name: "drawing.rs".into() }]);
    assert!(conservative["drawing.text"].contains("selection"), "without a proven discriminator the full function remains reachable");
}

#[test]
fn every_command_declares_the_editor_state_its_code_reads() {
    let derived = derived();
    if std::env::var("ACTS_PRINT").is_ok() {
        for (id, reads) in &derived {
            println!("{id} {reads:?}");
        }
    }
    let doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let commands = doc.commands();
    let mut wrong = Vec::new();
    for command in &commands {
        let declared: BTreeSet<&'static str> = command.reads.iter().copied().map(name).collect();
        let expected: BTreeSet<&'static str> = match derived.get(&command.id) {
            Some(read) => read.clone(),
            None => {
                let Some((_, state, _)) = kition_sheet_kit::acts::BINDING_COMMANDS.iter().find(|(id, _, _)| *id == command.id) else {
                    wrong.push(format!("{}: neither an engine command the scan found nor one acts::BINDING_COMMANDS declares", command.id));
                    continue;
                };
                state.iter().copied().map(name).collect()
            }
        };
        if declared != expected {
            wrong.push(format!("{}: declares {declared:?}, its code reads {expected:?}", command.id));
        }
    }
    assert!(wrong.is_empty(), "{} commands declare other editor state than they read:\n{}", wrong.len(), wrong.join("\n"));
    let listed: BTreeSet<&str> = commands.iter().map(|command| command.id.as_str()).collect();
    for (id, _, _) in kition_sheet_kit::acts::BINDING_COMMANDS {
        assert!(listed.contains(id), "acts::BINDING_COMMANDS names {id}, which the kit does not list");
    }
}
