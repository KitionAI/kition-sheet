//! The agent's `sheet_export` (KHP A41, kit.json `agent.export`) on the headless kit: Kition runs
//! the manifest's export command as the agent on the document it opened, the command answers the
//! file as base64 in the field the manifest names, and Kition writes the new file. The kit writes
//! no file of its own.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_sheet_kit::SheetDocument;
use serde_json::{Map, Value, json};

const MANIFEST: &str = include_str!("../../../kit.json");

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str) -> Vec<u8> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = BASE64.iter().position(|known| *known == byte).unwrap();
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    out
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[test]
fn the_agent_exports_every_declared_format_and_the_kit_writes_no_file() {
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    let export = &manifest["agent"]["export"];
    let (command, data) = (export["command"].as_str().unwrap(), export["data"].as_str().unwrap());
    let root = std::env::temp_dir().join(format!("kition-sheet-agent-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::copy(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.xlsx")), root.join("basic.xlsx")).unwrap();
    let mut kit = Headless::<SheetDocument>::new(KitInfo::from_manifest(MANIFEST, &[], &[]).unwrap());
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("basic.xlsx").unwrap(), read_only: false, create: None }).unwrap().id;
    let heads = [
        ("pdf", b"%PDF-".as_slice()),
        ("csv", b"".as_slice()),
        ("html", b"<".as_slice()),
        ("png", b"\x89PNG".as_slice()),
        ("xltx", b"PK".as_slice()),
    ];
    let declared: Vec<&str> = manifest["exports"].as_array().unwrap().iter().map(|entry| entry["extension"].as_str().unwrap()).collect();
    assert_eq!(declared, heads.iter().map(|(extension, _)| *extension).collect::<Vec<_>>());
    for (extension, head) in heads {
        let answer = kit.execute(&id, command, params(json!({ "format": extension })), Origin::Agent).unwrap().result.unwrap();
        let bytes = decode(answer[data].as_str().unwrap());
        assert!(!bytes.is_empty() && bytes.starts_with(head), "{extension}: {:?}", &bytes[..bytes.len().min(8)]);
    }
    let target = root.join("out.pdf");
    assert!(kit.execute(&id, command, params(json!({ "format": "pdf", "path": target.to_string_lossy() })), Origin::Agent).is_err());
    assert!(!target.exists());
    let _ = std::fs::remove_dir_all(root);
}
