//! Every command says what it does and every read command an agent is offered says what it answers
//! (kition_kit::testing::check_commands, KHP A1, W1-46 P2-1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_sheet_kit::SheetDocument;

fn basic() -> SheetDocument {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.xlsx"));
    SheetDocument::open("basic.xlsx", &std::fs::read(path).unwrap()).unwrap()
}

/// Runs a read command on `basic.xlsx` with its sample parameters (`command_samples.json`, `{}` when
/// it has none), after the sample's setup steps.
fn sample(id: &str) -> Result<serde_json::Value, String> {
    let samples: serde_json::Value = serde_json::from_str(include_str!("command_samples.json")).unwrap();
    let tmp = std::env::temp_dir().join(format!("kition-sheet-samples-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let fill = |value: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&value.to_string().replace("$tmp", &tmp.to_string_lossy())).unwrap()
    };
    let mut doc = basic();
    let entry = &samples[id];
    for step in entry["setup"].as_array().into_iter().flatten() {
        doc.execute(step[0].as_str().unwrap(), fill(&step[1]))?;
    }
    doc.execute(id, entry.get("params").map_or(serde_json::json!({}), fill))
}

/// The word list `spelling.check` checks against in these tests (`KITION_SHEET_WORDS`), so its
/// sample answers on a machine with no system word list too (a hosted CI runner).
const WORDS: &str = "a\nand\namount\ncity\ndate\nname\nnorth\nregion\nsales\nsouth\nthe\ntotal\nvalue\n";

/// What each read command answers matches the output schema it declares (KHP A1).
#[test]
fn every_read_command_answers_what_its_output_schema_says() {
    // The engine reads its word list once per process, so the test runs itself again with
    // `KITION_SHEET_WORDS` naming the tests' list.
    if std::env::var_os("KITION_SHEET_WORDS").is_none() {
        let words = std::env::temp_dir().join(format!("kition-sheet-words-{}.txt", std::process::id()));
        std::fs::write(&words, WORDS).unwrap();
        let name = "every_read_command_answers_what_its_output_schema_says";
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name, "--exact", "--nocapture", "--test-threads=1"])
            .env("KITION_SHEET_WORDS", &words)
            .output()
            .unwrap();
        let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && log.contains("1 passed"), "{log}");
        return;
    }
    let mut failures = Vec::new();
    for command in basic().commands().into_iter().filter(|c| c.schema.is_some() && !c.mutates) {
        let Some(schema) = command.output_schema else { continue };
        if let Err(error) = kition_kit::schema::validate_output(&schema) {
            failures.push(format!("{}: {error}", command.id));
            continue;
        }
        match sample(&command.id) {
            Ok(value) => {
                let wrapped = serde_json::json!({ "answer": schema });
                let params = serde_json::Map::from_iter([("answer".to_owned(), value)]);
                if let Err(error) = kition_kit::schema::check_params(&wrapped, &params) {
                    failures.push(format!("{}: {error}", command.id));
                }
            }
            Err(error) => failures.push(format!("{}: the sample call failed: {error}", command.id)),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_command_is_described_and_every_read_command_declares_its_output() {
    if let Err(problems) = kition_kit::testing::check_commands(&basic().commands(), kition_sheet_kit::NO_OUTPUT) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

/// `KITION_DUMP_COMMANDS=<file>`: writes the command list and what each read command answers on
/// `basic.xlsx` with no parameters, for writing descriptions and output schemas.
#[test]
fn dump_commands() {
    let Ok(out) = std::env::var("KITION_DUMP_COMMANDS") else { return };
    let doc = basic();
    let mut list = Vec::new();
    for command in doc.commands() {
        let mut entry = serde_json::to_value(&command).unwrap();
        if command.schema.is_some() && !command.mutates {
            entry["sample"] = match sample(&command.id) {
                Ok(value) => value,
                Err(error) => serde_json::json!({ "error": error }),
            };
        }
        list.push(entry);
    }
    std::fs::write(out, serde_json::to_string_pretty(&list).unwrap()).unwrap();
}
