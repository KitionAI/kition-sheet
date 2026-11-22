//! Compares the kit's rendering of a workbook (`corpus_render`'s output) with another
//! application's rendering of the same file (kept outside the repository, never committed):
//! a PNG of each sheet at one pixel per point with a 15-point margin, and a PDF of the workbook.
//!
//! `cargo run --release -p kition-sheet-kit --example corpus_compare -- ours/ ref-1.png[,ref-2.png] ref.pdf`
//! prints one JSON line of agreement scores (each from 0 to 1):
//!
//! - `cols`, `rows`: column widths and row heights within 1.5 pt (both found from the drawn grid);
//! - `text`: cells whose ink (text) has the same number of lines, starts within 4 pt and has
//!   about the same amount of ink;
//! - `fill`: cells whose background colour matches;
//! - `pages`: the smaller page count over the larger;
//! - `alloc`: cell texts that start on the same PDF page (`pdftotext` must be installed);
//! - `score`: their mean; `first`: the first row that differs (its height or most of its cells).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use image::{RgbImage, imageops::FilterType};
use serde_json::{Value, json};

/// The reference images' margin, in points (one pixel each).
const REF_MARGIN: u32 = 15;

fn lum(p: &image::Rgb<u8>) -> i32 {
    (i32::from(p[0]) * 299 + i32::from(p[1]) * 587 + i32::from(p[2]) * 114) / 1000
}

/// Grid lines: positions (in pixels) of thin lines that cross at least `share` of the image.
fn lines(img: &RgbImage, vertical: bool, share: f64) -> Vec<f64> {
    let (w, h) = img.dimensions();
    let (len, span) = if vertical { (w, h) } else { (h, w) };
    let at = |i: u32, j: u32| if vertical { img.get_pixel(i, j) } else { img.get_pixel(j, i) };
    let mut hits = Vec::new();
    for i in 2..len.saturating_sub(2) {
        let mut count = 0u32;
        for j in 0..span {
            let c = lum(at(i, j));
            let (a, b) = (lum(at(i - 2, j)), lum(at(i + 2, j)));
            if (c - a).abs() > 20 && (c - b).abs() > 20 {
                count += 1;
            }
        }
        hits.push((i, f64::from(count) / f64::from(span.max(1))));
    }
    let mut out = Vec::new();
    let mut run: Vec<u32> = Vec::new();
    for (i, s) in hits {
        if s >= share {
            if run.last().is_some_and(|&l| i > l + 1) {
                out.push(run.iter().map(|&v| f64::from(v)).sum::<f64>() / run.len() as f64);
                run.clear();
            }
            run.push(i);
        }
    }
    if !run.is_empty() {
        out.push(run.iter().map(|&v| f64::from(v)).sum::<f64>() / run.len() as f64);
    }
    out
}

struct Look {
    bg: [u8; 3],
    ink: u32,
    lines: u32,
    left: f64,
    top: f64,
    right: f64,
    /// Inked bands (text lines): first and last inked pixel row, from the cell's top.
    bands: Vec<(f64, f64)>,
}

/// What a cell's interior shows: its commonest colour, and the ink unlike it.
fn look(img: &RgbImage, x0: f64, x1: f64, y0: f64, y1: f64) -> Look {
    let (w, h) = img.dimensions();
    let (ax, bx) = ((x0 + 2.0).max(0.0) as u32, ((x1 - 2.0).max(0.0) as u32).min(w));
    let (ay, by) = ((y0 + 2.0).max(0.0) as u32, ((y1 - 1.0).max(0.0) as u32).min(h));
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for y in ay..by {
        for x in ax..bx {
            let p = img.get_pixel(x, y);
            *counts.entry([p[0] >> 3, p[1] >> 3, p[2] >> 3]).or_default() += 1;
        }
    }
    let bg = counts.into_iter().max_by_key(|(_, n)| *n).map_or([31, 31, 31], |(c, _)| c);
    let bg = bg.map(|v| (v << 3) | 4);
    let mut ink = 0;
    let (mut left, mut top, mut right) = (f64::MAX, f64::MAX, 0.0f64);
    let mut row_ink = Vec::new();
    for y in ay..by {
        let mut n = 0;
        for x in ax..bx {
            let p = img.get_pixel(x, y);
            let d: i32 = (0..3).map(|i| (i32::from(p[i]) - i32::from(bg[i])).abs()).sum();
            if d > 150 {
                n += 1;
                left = left.min(f64::from(x) - x0);
                right = right.max(f64::from(x) - x0);
                top = top.min(f64::from(y) - y0);
            }
        }
        ink += n;
        row_ink.push(n > 0);
    }
    // Text lines: bands of inked rows separated by two or more empty ones.
    let mut lines = 0;
    let mut gap = 2;
    let mut bands: Vec<(f64, f64)> = Vec::new();
    for (i, inked) in row_ink.into_iter().enumerate() {
        let at = f64::from(ay) + i as f64 - y0;
        if inked {
            if gap >= 2 {
                lines += 1;
                bands.push((at, at));
            } else if let Some(last) = bands.last_mut() {
                last.1 = at;
            }
            gap = 0;
        } else {
            gap += 1;
        }
    }
    Look { bg, ink, lines, left, top, right, bands }
}

fn pdf_pages(pdf: &Path) -> Vec<String> {
    let Ok(out) = Command::new("pdftotext").args(["-raw", "-enc", "UTF-8"]).arg(pdf).arg("-").output() else { return vec![] };
    String::from_utf8_lossy(&out.stdout).split('\u{c}').map(|page| page.chars().filter(|c| !c.is_whitespace()).collect()).collect()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(ours), Some(refs), Some(ref_pdf)) = (args.first(), args.get(1), args.get(2)) else {
        return Err("usage: corpus_compare <ours-dir> <ref-1.png[,ref-2.png]> <ref.pdf>".into());
    };
    let ours = Path::new(ours);
    let (mut cols_ok, mut cols_n, mut rows_ok, mut rows_n, mut text_ok, mut text_n, mut fill_ok, mut fill_n) = (0, 0, 0, 0, 0, 0, 0, 0);
    let mut first = Value::Null;
    let mut dump = std::env::var_os("CORPUS_COMPARE_DUMP").map(|_| {
        String::from("sheet\trow\tcol\toursH\trefH\toursW\trefW\toursLines\trefLines\toursLeft\trefLeft\toursRight\trefRight\toursBands\trefBands\n")
    });
    let col_index = |pairs: &[((f64, f64), (f64, f64))], x: f64| pairs.iter().position(|((a, _), _)| *a == x).map_or(0, |i| i + 1);
    let mut keys: Vec<String> = Vec::new();
    let mut sheets = Vec::new();
    for (index, reference) in refs.split(',').enumerate() {
        let geometry: Value =
            serde_json::from_str(&std::fs::read_to_string(ours.join(format!("sheet{}.json", index + 1))).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let full = image::open(ours.join(format!("sheet{}.png", index + 1))).map_err(|e| e.to_string())?.to_rgb8();
        let theirs = image::open(reference).map_err(|e| format!("{reference}: {e}"))?.to_rgb8();
        let grid = |img: &RgbImage, k: f64| {
            let scaled = |v: Vec<f64>| v.into_iter().map(|x| x * k).collect::<Vec<_>>();
            (scaled(lines(img, true, 0.6)), scaled(lines(img, false, 0.6)))
        };
        // The grid found at full size, then ours at one pixel per point, as the reference is.
        let (my_cols, my_rows) = grid(&full, 0.75);
        let (their_cols, their_rows) = grid(&theirs, 1.0);
        let (w, h) = full.dimensions();
        let mine = image::imageops::resize(&full, (f64::from(w) * 0.75).round() as u32, (f64::from(h) * 0.75).round() as u32, FilterType::Triangle);
        if std::env::var_os("CORPUS_COMPARE_DEBUG").is_some() {
            eprintln!(
                "ours cols {my_cols:?}\nref cols {their_cols:?}\nours rows {:?}\nref rows {:?}",
                &my_rows[..my_rows.len().min(30)],
                &their_rows[..their_rows.len().min(30)]
            );
        }
        let pairs = |a: &[f64], b: &[f64]| a.windows(2).zip(b.windows(2)).map(|(x, y)| ((x[0], x[1]), (y[0], y[1]))).collect::<Vec<_>>();
        let col_pairs = pairs(&my_cols, &their_cols);
        let row_pairs = pairs(&my_rows, &their_rows);
        for ((a, b), (c, d)) in &col_pairs {
            cols_n += 1;
            if ((b - a) - (d - c)).abs() <= 1.5 {
                cols_ok += 1;
            }
        }
        for (ri, ((a, b), (c, d))) in row_pairs.iter().enumerate() {
            rows_n += 1;
            let height_ok = ((b - a) - (d - c)).abs() <= 1.5;
            if height_ok {
                rows_ok += 1;
            }
            let (mut row_text_bad, mut row_cells) = (0, 0);
            for ((ma, mb), (ta, tb)) in &col_pairs {
                let mine_look = look(&mine, *ma, *mb, *a, *b);
                let their_look = look(&theirs, *ta, *tb, *c, *d);
                if let Some(out) = dump.as_mut() {
                    use std::fmt::Write as _;
                    let bands = |l: &Look| l.bands.iter().map(|(a, b)| format!("{a:.0}-{b:.0}")).collect::<Vec<_>>().join(" ");
                    let _ = writeln!(
                        out,
                        "{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{}\t{}",
                        index + 1,
                        ri + 1,
                        col_index(&col_pairs, *ma),
                        b - a,
                        d - c,
                        mb - ma,
                        tb - ta,
                        mine_look.lines,
                        their_look.lines,
                        if mine_look.ink > 0 { mine_look.left } else { -1.0 },
                        if their_look.ink > 0 { their_look.left } else { -1.0 },
                        if mine_look.ink > 0 { mine_look.right } else { -1.0 },
                        if their_look.ink > 0 { their_look.right } else { -1.0 },
                        bands(&mine_look),
                        bands(&their_look)
                    );
                }
                fill_n += 1;
                if (0..3).all(|i| (i32::from(mine_look.bg[i]) - i32::from(their_look.bg[i])).abs() <= 16) {
                    fill_ok += 1;
                }
                if mine_look.ink == 0 && their_look.ink == 0 {
                    continue;
                }
                text_n += 1;
                row_cells += 1;
                let ratio = f64::from(mine_look.ink.max(1)) / f64::from(their_look.ink.max(1));
                let same = mine_look.lines == their_look.lines
                    && (mine_look.left - their_look.left).abs() <= 4.0
                    && (mine_look.top - their_look.top).abs() <= 4.0
                    && (0.6..=1.67).contains(&ratio);
                if same {
                    text_ok += 1;
                } else {
                    row_text_bad += 1;
                }
            }
            if first.is_null() && (!height_ok || (row_cells > 0 && row_text_bad * 2 > row_cells)) {
                first = json!({ "sheet": index + 1, "row": ri + 1, "oursHeight": ((b - a) * 10.0).round() / 10.0, "refHeight": ((d - c) * 10.0).round() / 10.0, "cellsDiffering": row_text_bad, "cells": row_cells });
            }
        }
        sheets.push(json!({
            "sheet": index + 1,
            "oursCols": my_cols.len().saturating_sub(1), "refCols": their_cols.len().saturating_sub(1),
            "oursRows": my_rows.len().saturating_sub(1), "refRows": their_rows.len().saturating_sub(1),
            "oursSize": [mine.width(), mine.height()], "refSize": [theirs.width(), theirs.height()],
            "refMargin": REF_MARGIN,
        }));
        if let Some(cells) = geometry["cells"].as_array() {
            let texts: Vec<String> = cells
                .iter()
                .filter_map(|c| c["text"].as_str())
                .map(|t| t.chars().filter(|c| !c.is_whitespace()).take(8).collect::<String>())
                .filter(|k: &String| k.chars().count() >= 4)
                .collect();
            let mut seen: HashMap<String, usize> = HashMap::new();
            for t in &texts {
                *seen.entry(t.clone()).or_default() += 1;
            }
            keys.extend(texts.into_iter().filter(|t| seen.get(t) == Some(&1)));
        }
    }
    let my_pages = pdf_pages(&ours.join("book.pdf"));
    let their_pages = pdf_pages(Path::new(ref_pdf));
    let count = |pages: &[String]| pages.iter().filter(|p| !p.is_empty()).count().max(pages.len().saturating_sub(1));
    let (mp, tp) = (count(&my_pages), count(&their_pages));
    let page_of = |pages: &[String], key: &str| pages.iter().position(|p| p.contains(key));
    let (mut alloc_ok, mut alloc_n) = (0, 0);
    for key in &keys {
        if let Some(theirs) = page_of(&their_pages, key) {
            alloc_n += 1;
            if page_of(&my_pages, key) == Some(theirs) {
                alloc_ok += 1;
            }
        }
    }
    if let (Some(out), Some(path)) = (dump, std::env::var_os("CORPUS_COMPARE_DUMP")) {
        std::fs::write(path, out).map_err(|e| e.to_string())?;
    }
    let ratio = |ok: i32, n: i32| if n == 0 { 1.0 } else { f64::from(ok) / f64::from(n) };
    let scores = [
        ratio(cols_ok, cols_n),
        ratio(rows_ok, rows_n),
        ratio(text_ok, text_n),
        ratio(fill_ok, fill_n),
        if mp.max(tp) == 0 { 1.0 } else { mp.min(tp) as f64 / mp.max(tp) as f64 },
        ratio(alloc_ok, alloc_n),
    ];
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    println!(
        "{}",
        json!({
            "cols": round(scores[0]), "rows": round(scores[1]), "text": round(scores[2]), "fill": round(scores[3]),
            "pages": round(scores[4]), "alloc": round(scores[5]), "score": round(scores.iter().sum::<f64>() / scores.len() as f64),
            "oursPages": mp, "refPages": tp, "keys": alloc_n, "first": first, "sheets": sheets,
        })
    );
    Ok(())
}
