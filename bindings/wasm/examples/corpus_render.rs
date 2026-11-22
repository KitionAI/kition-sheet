//! Renders a workbook the way the corpus comparison (`corpus_compare`) needs it (`plan/` compares it with another
//! application's rendering of the same file, which is never committed): for each sheet, its used
//! range drawn by the kit's own renderer at 1:1 (96 dpi pixels) without headings or gridlines,
//! with a 20-pixel white margin, plus the geometry (column edges, row edges, the text of each
//! cell) as JSON; and the whole workbook as the kit's PDF export prints it.
//!
//! `cargo run --release -p kition-sheet-kit --example corpus_render -- book.xlsx out/`
//! writes `out/sheet<N>.png`, `out/sheet<N>.json` and `out/book.pdf`. Fonts come from the
//! kition-fonts checkout (`KITION_FONTS_DIR`, default `../kition-fonts`), as the web build's do.

use std::path::{Path, PathBuf};
use std::time::Instant;

use kition_kit::KitDocument;
use kition_sheet_kit::{HEADER_H, HEADER_W, SheetDocument};
use serde_json::json;

/// White space around the drawn range, in 96 dpi pixels (20 px = the other application's 15 pt).
const MARGIN: u32 = 20;
const TILE: u32 = 4096;

fn fonts_dir() -> PathBuf {
    std::env::var_os("KITION_FONTS_DIR").map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kition-fonts"), PathBuf::from)
}

fn add_fonts() -> usize {
    let dir = fonts_dir().join("fonts");
    let files = [
        "droid-sans-fallback/DroidSansFallbackFull.ttf",
        "carlito/Carlito-Regular.ttf",
        "carlito/Carlito-Bold.ttf",
        "carlito/Carlito-Italic.ttf",
        "carlito/Carlito-BoldItalic.ttf",
        "caladea/Caladea-Regular.ttf",
        "caladea/Caladea-Bold.ttf",
        "caladea/Caladea-Italic.ttf",
        "caladea/Caladea-BoldItalic.ttf",
        "arimo/Arimo-Variable.ttf",
        "arimo/Arimo-Italic-Variable.ttf",
        "tinos/Tinos-Regular.ttf",
        "tinos/Tinos-Bold.ttf",
        "tinos/Tinos-Italic.ttf",
        "cousine/Cousine-Regular.ttf",
    ];
    files.iter().filter_map(|f| std::fs::read(dir.join(f)).ok()).map(|bytes| kition_sheet_kit::add_font(&bytes)).sum()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(input), Some(out)) = (args.first(), args.get(1)) else {
        return Err("usage: corpus_render <book.xlsx> <out-dir> [--no-pdf]".into());
    };
    let pdf = !args.iter().any(|a| a == "--no-pdf");
    let out = Path::new(out);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let faces = add_fonts();
    let bytes = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
    let name = Path::new(input).file_name().map_or_else(|| "book.xlsx".to_owned(), |n| n.to_string_lossy().into_owned());
    #[allow(clippy::disallowed_methods)] // a native tool
    let t0 = Instant::now();
    let mut doc = SheetDocument::open(&name, &bytes)?;
    let opened = t0.elapsed();
    let (wb, _) = kition_sheet_engine::io::open_bytes(&name, &bytes).map_err(|e| e.to_string())?;
    let book = doc.execute("document.inspect", json!({}))?;
    let sheets: Vec<String> =
        book["sheets"].as_array().map(|all| all.iter().filter_map(|s| s["name"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    let mut report = Vec::new();
    for (index, sheet_name) in sheets.iter().enumerate() {
        doc.execute("sheet.activate", json!({ "sheet": sheet_name }))?;
        doc.execute("view.headings", json!({ "on": false }))?;
        doc.execute("view.gridlines", json!({ "on": false }))?;
        let Some(sheet) = wb.sheets.get(index) else { continue };
        let Some(used) = sheet.used_range() else { continue };
        let edges = |count: u32, size: &dyn Fn(u32) -> f32| {
            let mut at = 0.0f64;
            let mut out = vec![0.0];
            for i in 0..=count {
                at += f64::from(size(i));
                out.push(at);
            }
            out
        };
        let cols = edges(used.end.col, &|c| sheet.col_width(c));
        let rows = edges(used.end.row, &|r| sheet.row_height(r));
        let mut cells = Vec::new();
        for (c, _) in sheet.cells.iter() {
            let text = kition_sheet_engine::display::cell_text(&wb, sheet, c);
            if !text.trim().is_empty() {
                cells.push(json!({ "r": c.row, "c": c.col, "text": text }));
            }
        }
        let (Some(&right), Some(&bottom)) = (cols.last(), rows.last()) else { continue };
        let (w, h) = (right.ceil() as u32 + 2 * MARGIN + 1, bottom.ceil() as u32 + 2 * MARGIN + 1);
        #[allow(clippy::disallowed_methods)] // a native tool
        let t1 = Instant::now();
        let mut image = vec![255u8; (w as usize) * (h as usize) * 4];
        let (x0, y0) = (HEADER_W - f64::from(MARGIN), HEADER_H - f64::from(MARGIN));
        let mut ty = 0;
        while ty < h {
            let th = TILE.min(h - ty);
            let mut tx = 0;
            while tx < w {
                let tw = TILE.min(w - tx);
                let tile = doc.render(x0 + f64::from(tx), y0 + f64::from(ty), 1.0, tw, th)?;
                for row in 0..th {
                    let from = (row as usize) * (tw as usize) * 4;
                    let to = (((ty + row) as usize) * (w as usize) + tx as usize) * 4;
                    if let (Some(src), Some(dst)) = (tile.get(from..from + tw as usize * 4), image.get_mut(to..to + tw as usize * 4)) {
                        dst.copy_from_slice(src);
                    }
                }
                tx += tw;
            }
            ty += th;
        }
        // Blank out the margin (the renderer draws the sheet's own white there, or headings' bands).
        for y in 0..h {
            for x in 0..w {
                if x < MARGIN || y < MARGIN || x > w - MARGIN || y > h - MARGIN {
                    let at = ((y as usize) * (w as usize) + x as usize) * 4;
                    if let Some(px) = image.get_mut(at..at + 4) {
                        px.copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
            }
        }
        let rendered = t1.elapsed();
        let png = out.join(format!("sheet{}.png", index + 1));
        image::save_buffer(&png, &image, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
        std::fs::write(
            out.join(format!("sheet{}.json", index + 1)),
            serde_json::to_string(&json!({ "name": sheet_name, "margin": MARGIN, "cols": cols, "rows": rows, "cells": cells }))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        report.push(json!({ "sheet": sheet_name, "png": png.display().to_string(), "width": w, "height": h, "renderMs": rendered.as_millis() }));
    }
    let mut pages = 0;
    if pdf {
        doc.execute("sheet.activate", json!({ "sheet": sheets.first().cloned().unwrap_or_default() }))?;
        #[allow(clippy::disallowed_methods)] // a native tool
        let t2 = Instant::now();
        let answer = doc.execute("file.exportPdf", json!({ "sheets": "all" }))?;
        pages = answer["pages"].as_u64().unwrap_or(0);
        let data = answer["base64"].as_str().and_then(kition_sheet_engine::io::base64_decode).ok_or("no PDF")?;
        std::fs::write(out.join("book.pdf"), data).map_err(|e| e.to_string())?;
        report.push(json!({ "pdfPages": pages, "pdfMs": t2.elapsed().as_millis() }));
    }
    println!("{}", json!({ "fonts": faces, "openMs": opened.as_millis(), "sheets": report, "pages": pages }));
    Ok(())
}
