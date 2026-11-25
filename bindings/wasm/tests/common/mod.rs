//! `SheetKit`: the `kition-convergence` adapter for `SheetDocument`, which its performance mode
//! drives (`tests/perf.rs`).

#![allow(dead_code)]

use std::collections::BTreeMap;

use kdm_model::SheetModel;
use kition_convergence::{Blobs, KitAdapter, Rng, Script};
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::{KitModel, MemoryBlobs};
use kition_sheet_engine::core::CellRef;
use kition_sheet_engine::model::Workbook;
use kition_sheet_kit::SheetDocument;
use serde_json::{Value, json};

pub struct SheetKit;

const NAME: &str = "Book.xlsx";
const SCHEMA: &str = include_str!("../../../../crates/kdm/kdm-model/model/schema.json");

/// Runs a command; a command the workbook refuses is an error.
pub fn run(doc: &mut SheetDocument, command: &str, params: Value) -> Result<Value, String> {
    doc.execute(command, params).map_err(|error| format!("{command}: {error}"))
}

/// The engine's view of the workbook (its own JSON form), formula results included.
pub fn raw_view(doc: &mut SheetDocument) -> Result<Workbook, String> {
    let bytes = doc.save(Some("view.json"))?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

/// The engine's view of the workbook, without the per-person view state, normalised for
/// comparison (`kdm_model::normalize`).
pub fn view(doc: &mut SheetDocument) -> Result<Workbook, String> {
    raw_view(doc).map(strip)
}

fn strip(mut wb: Workbook) -> Workbook {
    for sheet in &mut wb.sheets {
        let sheet = std::sync::Arc::make_mut(sheet);
        sheet.zoom = 100;
        sheet.view_active = CellRef::new(0, 0);
        sheet.view_top_left = CellRef::new(0, 0);
    }
    kdm_model::normalize(&wb)
}

fn replica(doc: &mut SheetDocument) -> Result<(Replica, MemoryBlobs), String> {
    let snapshot = doc.snapshot()?;
    let blobs = decode_blobs(&doc.blobs()?)?;
    let replica = Replica::load(SheetModel::schema(), &snapshot, ActorId::random()).map_err(|error| error.to_string())?;
    Ok((replica, blobs))
}

/// The workbook as a fresh read of the replica.
fn read(doc: &mut SheetDocument) -> Result<Workbook, String> {
    let (replica, blobs) = replica(doc)?;
    let (wb, _) = kdm_model::read_document(&replica, &blobs).map_err(|error| error.to_string())?;
    Ok(strip(wb))
}

fn probe_text(doc: &mut SheetDocument) -> Result<String, String> {
    let wb = read(doc)?;
    Ok(wb.sheet(0).map(|sheet| sheet.input_text(CellRef::new(0, 0))).unwrap_or_default())
}

fn sheets(doc: &SheetDocument) -> usize {
    doc.inspect().ok().and_then(|value| value["sheets"].as_array().map(Vec::len)).unwrap_or(1)
}

/// `(64-char hex sha256, u64 LE length, bytes) * N`, as `blobs` and `add_blobs` move them.
fn encode_blobs(blobs: &Blobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, bytes) in blobs {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn decode_blobs(bytes: &[u8]) -> Result<MemoryBlobs, String> {
    let mut out = MemoryBlobs::default();
    let mut at = 0usize;
    while at < bytes.len() {
        let hash = bytes.get(at..at + 64).ok_or("truncated blob hash")?;
        let len: [u8; 8] = bytes.get(at + 64..at + 72).ok_or("truncated blob length")?.try_into().map_err(|_| "blob length")?;
        let end = at + 72 + usize::try_from(u64::from_le_bytes(len)).map_err(|_| "blob length")?;
        let data = bytes.get(at + 72..end).ok_or("truncated blob")?;
        out.0.insert(String::from_utf8_lossy(hash).into_owned(), data.into());
        at = end;
    }
    Ok(out)
}

/// A 1 x 1 PNG of a colour from `seed` (stored zlib block, so no compressor is needed).
fn png(seed: u32) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let body = [kind.as_slice(), data].concat();
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    }
    let raw = [0u8, (seed & 0xFF) as u8, ((seed >> 8) & 0xFF) as u8, ((seed >> 16) & 0xFF) as u8];
    let (mut a, mut b) = (1u32, 0u32);
    for byte in raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    let mut zlib = vec![0x78, 0x01, 0x01, raw.len() as u8, 0, !(raw.len() as u8), 0xFF];
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Replaces the probe cell's text with `text` (the bridge diffs it into character edits).
fn set_text(doc: &mut SheetDocument, text: &str) -> Result<(), String> {
    run(doc, "cell.set", json!({ "cell": "A1", "input": text })).map(|_| ())
}

fn column(index: usize) -> char {
    (b'A' + index as u8) as char
}

impl KitAdapter for SheetKit {
    type Doc = SheetDocument;

    fn name(&self) -> &str {
        "kit.sheet"
    }

    fn schema(&self) -> &str {
        SCHEMA
    }

    fn create(&self) -> Result<SheetDocument, String> {
        let mut doc = SheetDocument::create(NAME, 0, 0)?;
        for (cell, input) in [("A1", "Hello world"), ("A2", "Budget"), ("B1", "120"), ("B2", "=B1*2"), ("A5", "below")] {
            run(&mut doc, "cell.set", json!({ "cell": cell, "input": input }))?;
        }
        run(&mut doc, "home.insertSheet", json!({ "name": "Notes" }))?;
        run(&mut doc, "sheet.activate", json!({ "sheet": 0 }))?;
        doc.version()?;
        Ok(doc)
    }

    fn join(&self, snapshot: &[u8], blobs: &Blobs) -> Result<SheetDocument, String> {
        let mut doc = SheetDocument::open_with_replica(NAME, &[], snapshot, &encode_blobs(blobs))?;
        if !doc.has_replica() {
            return Err("the snapshot did not open as a replica".into());
        }
        doc.version()?;
        Ok(doc)
    }

    fn version(&self, doc: &mut SheetDocument) -> Result<Vec<u8>, String> {
        doc.version()
    }

    fn export_changes(&self, doc: &mut SheetDocument, since: &[u8]) -> Result<Vec<u8>, String> {
        doc.export_changes(since)
    }

    fn import_changes(&self, doc: &mut SheetDocument, bytes: &[u8]) -> Result<(), String> {
        doc.import_changes(bytes).map(|_| ())
    }

    fn snapshot(&self, doc: &mut SheetDocument) -> Result<Vec<u8>, String> {
        doc.snapshot()
    }

    fn state(&self, doc: &mut SheetDocument) -> Result<Value, String> {
        replica(doc)?.0.state_json().map_err(|error| error.to_string())
    }

    fn undo(&self, doc: &mut SheetDocument) -> Result<bool, String> {
        Ok(doc.undo()?.0)
    }

    fn redo(&self, doc: &mut SheetDocument) -> Result<bool, String> {
        Ok(doc.redo()?.0)
    }

    fn random_edit(&self, doc: &mut SheetDocument, rng: &mut Rng) -> Result<Option<String>, String> {
        let count = sheets(doc);
        // Edits land on a random sheet some of the time.
        if rng.chance(20) {
            let _ = run(doc, "sheet.activate", json!({ "sheet": rng.below(count) }));
        }
        let cell = format!("{}{}", column(rng.below(6)), 1 + rng.below(12));
        let range = format!("{}{}:{}{}", column(rng.below(3)), 1 + rng.below(6), column(3 + rng.below(3)), 7 + rng.below(6));
        let line = 1 + rng.below(10);
        let value = rng.below(1000);
        let (command, params) = match rng.below(20) {
            0..=3 => ("cell.set", json!({ "cell": cell, "input": value.to_string() })),
            4..=5 => ("cell.set", json!({ "cell": cell, "input": format!("item {value}") })),
            6 => {
                let other = format!("{}{}", column(rng.below(6)), 1 + rng.below(12));
                ("cell.set", json!({ "cell": cell, "input": format!("={other}+{value}") }))
            }
            7 => ("cell.set", json!({ "cell": cell, "input": "=SUM(A1:F12)" })),
            8 => ("cell.set", json!({ "cell": cell, "input": "=Notes!A1*2" })),
            9 => ("home.bold", json!({ "range": range })),
            10 => ("home.fillColor", json!({ "range": range, "color": format!("#{:06X}", value * 16_111 % 0xFF_FFFF) })),
            11 => ("home.insertRows", json!({ "rows": format!("{line}:{line}") })),
            12 => ("home.deleteRows", json!({ "rows": format!("{line}:{line}") })),
            13 => ("home.insertColumns", json!({ "cols": format!("{c}:{c}", c = column(rng.below(5))) })),
            14 => ("home.deleteColumns", json!({ "cols": format!("{c}:{c}", c = column(rng.below(5))) })),
            15 => ("home.mergeCells", json!({ "range": format!("{c}{line}:{d}{line}", c = column(0), d = column(1 + rng.below(3))) })),
            16 => ("edit.fillDown", json!({ "range": range })),
            17 => ("home.insertSheet", json!({ "name": format!("S{value}") })),
            18 => ("sheet.rename", json!({ "sheet": rng.below(count), "name": format!("R{value}") })),
            _ if count > 1 && rng.chance(50) => ("home.deleteSheet", json!({ "sheet": rng.below(count) })),
            _ => ("sheet.move", json!({ "sheet": rng.below(count), "to": rng.below(count) })),
        };
        Ok(run(doc, command, params).ok().map(|_| command.to_owned()))
    }

    fn script(&self, doc: &mut SheetDocument, step: &Script) -> Result<bool, String> {
        run(doc, "sheet.activate", json!({ "sheet": 0 }))?;
        match step {
            // B1's value (its `input`) and its fill (its `style`): two registers of one `Cell`.
            Script::SetField { slot: 0, value } => run(doc, "cell.set", json!({ "cell": "B1", "input": (1000 + value).to_string() }))?,
            Script::SetField { value, .. } => {
                run(doc, "home.fillColor", json!({ "range": "B1", "color": format!("#{:06X}", (value + 1) * 40_503 % 0xFF_FFFF) }))?
            }
            Script::Create => run(doc, "home.insertSheet", json!({}))?,
            Script::Delete => run(doc, "home.deleteRows", json!({ "rows": "1:1" }))?,
            Script::Duplicate => run(doc, "sheet.move", json!({ "sheet": 0, "to": sheets(doc), "copy": true }))?,
            Script::Reorder { which } => {
                let count = sheets(doc);
                let from = usize::from(*which) % count;
                run(doc, "sheet.move", json!({ "sheet": from, "to": if from == 0 { count - 1 } else { 0 } }))?
            }
            Script::InsertText { at, text } => {
                let old: Vec<char> = probe_text(doc)?.chars().collect();
                let at = (*at).min(old.len());
                let new: String = old[..at].iter().chain(text.chars().collect::<Vec<_>>().iter()).chain(old[at..].iter()).collect();
                set_text(doc, &new)?;
                return Ok(true);
            }
            Script::DeleteText { at, len } => {
                let old: Vec<char> = probe_text(doc)?.chars().collect();
                let new: String = old.iter().enumerate().filter(|(index, _)| *index < *at || *index >= at + len).map(|(_, c)| *c).collect();
                set_text(doc, &new)?;
                return Ok(true);
            }
            // A line break in the cell's text.
            Script::SplitParagraph { at } => return self.script(doc, &Script::InsertText { at: *at, text: "\n".into() }),
            Script::MergeParagraph { at } => {
                if probe_text(doc)?.chars().nth(*at) != Some('\n') {
                    return Err(format!("no line break at {at}"));
                }
                return self.script(doc, &Script::DeleteText { at: *at, len: 1 });
            }
            Script::InsertBlob { seed } => {
                run(doc, "insert.picture", json!({ "base64": base64(&png(*seed)), "mime": "image/png", "at": "D4", "placement": "overCells" }))?
            }
            _ => return Ok(false),
        };
        Ok(true)
    }

    fn text(&self, doc: &mut SheetDocument) -> Result<Option<String>, String> {
        probe_text(doc).map(Some)
    }

    fn check_view(&self, doc: &mut SheetDocument) -> Result<(), String> {
        let (view, fresh) = (view(doc)?, read(doc)?);
        if view.sheets.len() != fresh.sheets.len() {
            return Err(format!("the view has {} sheets, the replica {}", view.sheets.len(), fresh.sheets.len()));
        }
        for (a, b) in view.sheets.iter().zip(&fresh.sheets) {
            if a.name != b.name {
                return Err(format!("sheet {} in the view is {} in the replica", a.name, b.name));
            }
            let cells_a: BTreeMap<CellRef, String> = a.cells.iter().map(|(at, _)| (at, a.input_text(at))).collect();
            let cells_b: BTreeMap<CellRef, String> = b.cells.iter().map(|(at, _)| (at, b.input_text(at))).collect();
            if cells_a != cells_b {
                let at = cells_a
                    .iter()
                    .find(|(at, text)| cells_b.get(at) != Some(text))
                    .map(|(at, _)| *at)
                    .or_else(|| cells_b.keys().find(|at| !cells_a.contains_key(at)).copied());
                let show = |sheet: &kition_sheet_engine::model::Sheet| at.and_then(|at| sheet.cell(at).cloned());
                return Err(format!(
                    "sheet {}: the view and the replica differ at {}: {:?} in the view, {:?} in the replica",
                    a.name,
                    at.map(|at| at.a1()).unwrap_or_default(),
                    show(a),
                    show(b)
                ));
            }
        }
        if view != fresh {
            return Err("the view and a fresh read of the replica differ (beyond cell inputs)".into());
        }
        Ok(())
    }

    fn blobs(&self, doc: &mut SheetDocument) -> Result<Blobs, String> {
        Ok(decode_blobs(&doc.blobs()?)?.0.into_iter().map(|(hash, bytes)| (hash, bytes.to_vec())).collect())
    }

    fn add_blobs(&self, doc: &mut SheetDocument, blobs: &Blobs) -> Result<(), String> {
        doc.add_blobs(&encode_blobs(blobs))
    }

    fn export_file(&self, doc: &mut SheetDocument) -> Result<Option<Vec<u8>>, String> {
        doc.save(None).map(Some)
    }

    /// Typing one character into a cell and entering it, then repainting the visible grid.
    fn keystroke(&self, doc: &mut SheetDocument, rng: &mut Rng) -> Result<(), String> {
        let cell = format!("{}{}", column(rng.below(6)), 1 + rng.below(30));
        run(doc, "cell.set", json!({ "cell": cell, "input": format!("k{}", rng.below(10)) }))?;
        doc.render_area(0, 0, 1200, 800, 1200, 800).map(|_| ())
    }
}
