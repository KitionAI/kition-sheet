//! The convergence suite's perf mode on Kition Sheet (Kition `docs/kits.md` section 7):
//! `cargo test --release -p kition-sheet-kit --test perf -- --ignored --nocapture`.

mod common;

use kition_convergence::CountingAlloc;
use kition_convergence::perf::{self, PerfOptions};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

#[test]
#[ignore = "a measurement, not a check; run in release"]
fn budgets() {
    let report = perf::measure(&common::SheetKit, &PerfOptions { allocator: Some(&ALLOC), ..PerfOptions::default() }).unwrap();
    println!("{}", report.render());
}
