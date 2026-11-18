//! Timings of the bridge on large workbooks (rule P1), ignored by default: run with
//! `cargo test --release -p kdm-model --test perf -- --ignored --nocapture --test-threads=1`.
//! Each prints the import (a new replica), the snapshot's size and time, opening it, one cell
//! edit (text, number, a new cell: the median of 21), a 1,000-cell paste, and on a peer one cell edit
//! imported (typing in a cell, a new cell, a new row past the data) and undone (the medians of 21).
//! Native only (the clock).

// The clock is read off wasm only: these tests never run there.
#![allow(clippy::disallowed_methods)]

mod common;

use std::time::{Duration, Instant};

use common::put;
use kdm_model::SheetSession;
use kition_kit::kdm::Origin;
use kition_kit::session::MemoryBlobs;
use kition_sheet_model::{LineInfo, Workbook};

/// The 10,000-row sheet of `write_changes.rs`: numbers in A, text in B, a formula in C (and E1, so
/// column D is empty and new cells go there without new columns).
fn rows_10k() -> Workbook {
    let mut wb = Workbook::new();
    put(&mut wb, 0, "E1", "edge");
    for r in 1..=10_000u32 {
        put(&mut wb, 0, &format!("A{r}"), &format!("{r}"));
        put(&mut wb, 0, &format!("B{r}"), &format!("item {r}"));
        put(&mut wb, 0, &format!("C{r}"), &format!("=A{r}*2+A{}", r + 1));
    }
    let sheet = wb.sheet_mut(0).unwrap_or_else(|| panic!("sheet"));
    for r in (0..10_000).step_by(100) {
        sheet.rows.insert(r, LineInfo { size: Some(24.0), custom: true, ..Default::default() });
    }
    wb
}

/// 100,000 cells: 10,000 rows of ten columns, half text, half numbers, a formula every tenth row
/// (and L1, so column K is empty and new cells go there without new columns).
fn cells_100k() -> Workbook {
    let mut wb = Workbook::new();
    put(&mut wb, 0, "L1", "edge");
    let cols = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"];
    for r in 1..=10_000u32 {
        for (k, c) in cols.iter().enumerate() {
            let input = match k {
                9 if r % 10 == 0 => format!("=SUM(A{r}:I{r})"),
                k if k % 2 == 0 => format!("{}", r * 10 + k as u32),
                _ => format!("text {c}{r}"),
            };
            put(&mut wb, 0, &format!("{c}{r}"), &input);
        }
    }
    wb
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn measure(name: &str, wb: &Workbook, text_cell: &str, number_cell: &str, empty_col: &str) {
    let origin = Origin::person("user:perf");
    let t = Instant::now();
    let created = SheetSession::create(wb).unwrap_or_else(|e| panic!("create: {e}"));
    let import = t.elapsed();
    let t = Instant::now();
    let snapshot = created.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    let snap_time = t.elapsed();
    drop(created);
    let t = Instant::now();
    let mut s = SheetSession::open_snapshot(&snapshot, MemoryBlobs::default()).unwrap_or_else(|e| panic!("open: {e}"));
    let open = t.elapsed();

    // The first record after opening may migrate (0.3) or warm up: timed on its own.
    let mut view = s.workbook().clone();
    put(&mut view, 0, text_cell, "first edit");
    let t = Instant::now();
    s.record(&origin, &view, &[]).unwrap_or_else(|e| panic!("record: {e}"));
    let first = t.elapsed();

    let mut text = Vec::new();
    let mut number = Vec::new();
    for k in 0..21 {
        let mut next = s.workbook().clone();
        put(&mut next, 0, text_cell, &format!("edited text {k}"));
        let t = Instant::now();
        s.record(&origin, &next, &[]).unwrap_or_else(|e| panic!("record: {e}"));
        text.push(t.elapsed());
        let mut next = s.workbook().clone();
        put(&mut next, 0, number_cell, &format!("{}", 1000 + k));
        let t = Instant::now();
        s.record(&origin, &next, &[]).unwrap_or_else(|e| panic!("record: {e}"));
        number.push(t.elapsed());
    }

    // A 1,000-cell paste: 100 rows by 10 columns, over existing cells and past them.
    let mut pasted = s.workbook().clone();
    for r in 2001..=2100u32 {
        for c in ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"] {
            put(&mut pasted, 0, &format!("{c}{r}"), &format!("pasted {c}{r}"));
        }
    }
    let t = Instant::now();
    s.record(&origin, &pasted, &[]).unwrap_or_else(|e| panic!("record: {e}"));
    let paste = t.elapsed();
    let after = s.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));

    // A new cell typed (its `Cell` object is created ahead, in a `system` transaction).
    let mut typed_new = Vec::new();
    for k in 0..21 {
        let mut next = s.workbook().clone();
        put(&mut next, 0, &format!("{empty_col}{}", 100 + k), &format!("fresh {k}"));
        let t = Instant::now();
        s.record(&origin, &next, &[]).unwrap_or_else(|e| panic!("record: {e}"));
        typed_new.push(t.elapsed());
    }

    // A collaborator's replica: one cell edit of the other's, imported (its typing in an existing
    // cell, then a value in a new cell), and an undo of one's own.
    let mut peer = SheetSession::open_snapshot(&after, s.blobs().clone()).unwrap_or_else(|e| panic!("open: {e}"));
    let send_edit = |s: &mut SheetSession, peer: &mut SheetSession, cell: &str, input: &str| {
        let since = peer.version();
        let mut next = s.workbook().clone();
        put(&mut next, 0, cell, input);
        s.record(&origin, &next, &[]).unwrap_or_else(|e| panic!("record: {e}"));
        let bytes = s.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"));
        let t = Instant::now();
        peer.import_changes(&bytes).unwrap_or_else(|e| panic!("import: {e}"));
        t.elapsed()
    };
    let first_import = send_edit(&mut s, &mut peer, text_cell, "typed first");
    let (mut typed, mut added, mut appended, mut undone) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    // Each import holds one edit: typing in an existing cell, a new cell, a new row past the data.
    for k in 0..21 {
        typed.push(send_edit(&mut s, &mut peer, text_cell, &format!("typed {k}")));
    }
    for k in 0..21 {
        added.push(send_edit(&mut s, &mut peer, &format!("{empty_col}{}", 3 + k), &format!("new {k}")));
    }
    for k in 0..21 {
        appended.push(send_edit(&mut s, &mut peer, &format!("A{}", 10_100 + k), &format!("{k}")));
    }
    // Undoing them: the new rows (a full read: the view's last lines go), then the new cells.
    let mut unrowed = Vec::new();
    for k in 0..42 {
        let t = Instant::now();
        s.undo().unwrap_or_else(|e| panic!("undo: {e}"));
        if k < 21 { unrowed.push(t.elapsed()) } else { undone.push(t.elapsed()) }
    }

    eprintln!(
        "{name}: import {} | snapshot {} KB in {} | open {} | first edit {} | text edit {} | number edit {} | new cell edit {} | 1,000-cell paste {} | snapshot after the edits {} KB | a peer's first import of a cell edit {} | typing imported {} | a new cell imported {} | a new row imported {} | undo of a new cell {} | undo of a new row {}",
        ms(import),
        snapshot.len() / 1024,
        ms(snap_time),
        ms(open),
        ms(first),
        ms(median(text)),
        ms(median(number)),
        ms(median(typed_new)),
        ms(paste),
        after.len() / 1024,
        ms(first_import),
        ms(median(typed)),
        ms(median(added)),
        ms(median(appended)),
        ms(median(undone)),
        ms(median(unrowed))
    );
}

#[test]
#[ignore = "timings: run in release with --ignored"]
fn perf_10000_rows() {
    measure("10,000 rows (30,000 cells)", &rows_10k(), "B5000", "A5000", "D");
}

#[test]
#[ignore = "timings: run in release with --ignored"]
fn perf_100000_cells() {
    measure("100,000 cells", &cells_100k(), "B5000", "A5000", "K");
}
