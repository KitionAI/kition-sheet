//! TEMPORARY (plain model round 5): runs `tests/perf_scale.rs` in a release build, one size per
//! process, from the debug test run CI makes, and prints its lines to the job's log. Deleted once
//! the numbers are taken.

use std::io::Write;
use std::path::Path;
use std::process::Command;

#[test]
fn tmp_release_perf() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    for size in ["cells_10k", "cells_100k", "cells_1m"] {
        let out = Command::new(&cargo)
            .args(["test", "--release", "-p", "kdm-model", "--test", "perf_scale", "--", "--ignored", "--nocapture", "--exact", size])
            .env("CARGO_TARGET_DIR", root.join("target/perf-release"))
            .current_dir(&root)
            .output()
            .unwrap_or_else(|e| panic!("cargo: {e}"));
        let text = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = text.lines().filter(|l| l.contains("PERF") || l.contains("panicked") || l.contains("error")).collect();
        let _ = std::io::stderr().write_all(format!("TMP-PERF {size} ({}):\n{}\n", out.status, lines.join("\n")).as_bytes());
    }
}
