//! The bridge's costs at 10,000, 100,000 and 1,000,000 cells (rule P1; plan 2026-11-24, plain
//! model, section 10), ignored by default. Each size runs in a process of its own, so the peak
//! memory is that size's alone:
//! `cargo test --release -p kdm-model --test perf_scale -- --ignored --nocapture --exact cells_10k`
//! (and `cells_100k`, `cells_1m`). Each prints one line: the import (a new replica), the
//! snapshot's and the content's bytes, opening each, one cell edited (the median of 5, split into
//! the diff written, the positions projected, the bare commit and the view adopted), the size of
//! that change, its import by another replica (the median of 5), the `.xlsx` the kit writes, and
//! the process's peak resident memory (Linux; `n/a` elsewhere); at a million cells the edit must
//! stay under 50 ms. One test runs in every (debug) CI build: an edit costs about the same in a
//! sheet a hundred times longer. Native only (the clock).

// The clock is read off wasm only: these tests never run there.
#![allow(clippy::disallowed_methods)]

mod common;

use std::time::{Duration, Instant};

use common::put;
use kdm_model::SheetSession;
use kition_kit::kdm::Origin;
use kition_kit::session::MemoryBlobs;
use kition_sheet_core::{CellRef, Value};
use kition_sheet_model::{Cell, Workbook};

const COLS: u32 = 50;

/// `rows` rows of 50 numbers on one sheet.
fn numbers(rows: u32) -> Workbook {
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).unwrap_or_else(|| panic!("sheet"));
    for r in 0..rows {
        for c in 0..COLS {
            sheet.set_cell(CellRef::new(r, c), Cell::value(Value::Number(f64::from(r * COLS + c))));
        }
    }
    wb
}

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1000.0)
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn mb(bytes: usize) -> String {
    format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// The process's peak resident memory (`VmHWM`), on Linux.
fn peak_memory() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| status.lines().find(|l| l.starts_with("VmHWM:")).map(|l| l.trim_start_matches("VmHWM:").trim().to_owned()))
        .unwrap_or_else(|| "n/a".into())
}

/// One record's phases, as `SheetSession::record_marked` marks them.
#[derive(Default)]
struct Phases {
    write: Vec<Duration>,
    positions: Vec<Duration>,
    commit: Vec<Duration>,
    view: Vec<Duration>,
    total: Vec<Duration>,
}

/// Prints the line for `rows` rows of 50 numbers; answers the median edit of one cell.
fn measure(name: &str, rows: u32) -> Duration {
    let origin = Origin::person("user:perf");
    let wb = numbers(rows);
    let t = Instant::now();
    let created = SheetSession::create(&wb).unwrap_or_else(|e| panic!("create: {e}"));
    let import = t.elapsed();
    drop(wb);
    let snapshot = created.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    let content = created.replica().content_snapshot().unwrap_or_else(|e| panic!("content: {e}"));
    drop(created);
    let t = Instant::now();
    let from_content = SheetSession::open_snapshot(&content, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open content: {e}"));
    let open_content = t.elapsed();
    drop(from_content);
    let t = Instant::now();
    let mut s = SheetSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open: {e}"));
    let open = t.elapsed();
    let mut peer = SheetSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open peer: {e}"));

    // A warm-up edit, then five, each of an existing cell in the middle of the sheet.
    let mut phases = Phases::default();
    let (mut sizes, mut imports) = (Vec::new(), Vec::new());
    for k in 0..6u32 {
        let since = peer.version();
        let mut next = s.workbook().clone();
        let cell = format!("B{}", rows / 2 + k);
        put(&mut next, 0, &cell, &format!("{}", 0.5 + f64::from(k)));
        let mut marks: Vec<(&'static str, Instant)> = Vec::with_capacity(4);
        let t = Instant::now();
        s.record_marked(&origin, &next, &[], &mut |phase| marks.push((phase, Instant::now()))).unwrap_or_else(|e| panic!("record: {e}"));
        let total = t.elapsed();
        let bytes = s.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"));
        let ti = Instant::now();
        peer.import_changes(&bytes).unwrap_or_else(|e| panic!("import: {e}"));
        let imported = ti.elapsed();
        assert_eq!(common::input(peer.workbook(), 0, &cell), format!("{}", 0.5 + f64::from(k)), "the peer holds the edit");
        if k == 0 {
            continue;
        }
        let at = |phase: &str| marks.iter().find(|(p, _)| *p == phase).map(|(_, i)| *i);
        let (Some(w), Some(p), Some(c), Some(v)) = (at("write"), at("positions"), at("commit"), at("view")) else {
            panic!("a record of an existing cell marks every phase: {:?}", marks.iter().map(|(p, _)| *p).collect::<Vec<_>>())
        };
        phases.write.push(w - t);
        phases.positions.push(p - w);
        phases.commit.push(c - p);
        phases.view.push(v - c);
        phases.total.push(total);
        sizes.push(bytes.len());
        imports.push(imported);
    }
    sizes.sort_unstable();
    let edit = median(phases.total.clone());
    let xlsx = kdm_model::encode("Book.xlsx", s.workbook()).unwrap_or_else(|e| panic!("xlsx: {e}"));
    let line = format!(
        "PERF {name}: import {} | snapshot {} | content {} | open snapshot {} | open content {} | edit one cell {} (write_changes {}, positions {}, commit {}, view {}) | change {} bytes | peer import {} | xlsx {} KB | peak memory {}\n",
        ms(import),
        mb(snapshot.len()),
        mb(content.len()),
        ms(open),
        ms(open_content),
        ms(edit),
        ms(median(phases.write)),
        ms(median(phases.positions)),
        ms(median(phases.commit)),
        ms(median(phases.view)),
        sizes[sizes.len() / 2],
        ms(median(imports)),
        xlsx.len() / 1024,
        peak_memory(),
    );
    use std::io::Write;
    let _ = std::io::stderr().write_all(line.as_bytes());
    edit
}

#[test]
#[ignore = "timings: run in release with --ignored, one size per process"]
fn cells_10k() {
    measure("10,000 cells (200 rows x 50 columns)", 200);
}

#[test]
#[ignore = "timings: run in release with --ignored, one size per process"]
fn cells_100k() {
    measure("100,000 cells (2,000 rows x 50 columns)", 2_000);
}

#[test]
#[ignore = "timings: run in release with --ignored, one size per process"]
fn cells_1m() {
    let edit = measure("1,000,000 cells (20,000 rows x 50 columns)", 20_000);
    // Rule P1: one cell typed into a sheet of a million cells reads no other cell and no line.
    assert!(edit < Duration::from_millis(50), "one cell's edit took {} at a million cells", ms(edit));
}

/// The median time of one existing cell's record (of 11) in a sheet of `rows` rows of one number.
fn edit_time(rows: u32) -> Duration {
    let mut wb = Workbook::new();
    let sheet = wb.sheet_mut(0).unwrap_or_else(|| panic!("sheet"));
    for r in 0..rows {
        sheet.set_cell(CellRef::new(r, 0), Cell::value(Value::Number(f64::from(r))));
    }
    let mut s = SheetSession::create(&wb).unwrap_or_else(|e| panic!("create: {e}"));
    let origin = Origin::person("user:perf");
    let mut times = Vec::new();
    for k in 0..12u32 {
        let mut next = s.workbook().clone();
        put(&mut next, 0, &format!("A{}", rows / 2 + k), &format!("{}", 0.5 + f64::from(k)));
        let t = Instant::now();
        s.record(&origin, &next, &[]).unwrap_or_else(|e| panic!("record: {e}"));
        // The first record after opening checks every line's position once.
        if k > 0 {
            times.push(t.elapsed());
        }
    }
    median(times)
}

/// Rule P1, in the debug build CI runs: editing one cell costs about the same in a sheet of
/// 50,000 rows as in one of 500 (the write neither diffs the whole view nor reads every row's
/// position; before the plain model's re-pin each edit read all of them, a hundred times more
/// here).
#[test]
fn one_cells_edit_costs_the_same_in_a_hundred_times_longer_sheet() {
    let short = edit_time(500);
    let long = edit_time(50_000);
    eprintln!("one cell's edit: {} at 500 rows, {} at 50,000 rows", ms(short), ms(long));
    assert!(long < short * 4 + Duration::from_millis(3), "one cell's edit took {} at 50,000 rows, {} at 500", ms(long), ms(short));
}
