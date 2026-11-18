//! Conditional-format icon sets, drawn in code (original geometry: arrows, lights, flags,
//! symbols, stars, triangles, rating bars, quarters, boxes) in the families Excel's sets are
//! named after. Index 0 is the lowest icon of the set.

use kition_sheet_chart::{Prim, Rgba};

const RED: Rgba = [0xD2, 0x3C, 0x2E, 0xFF];
const YELLOW: Rgba = [0xF0, 0xB0, 0x1E, 0xFF];
const GREEN: Rgba = [0x2E, 0x9A, 0x4C, 0xFF];
const GRAY: Rgba = [0x86, 0x86, 0x8C, 0xFF];
const LIGHT: Rgba = [0xD9, 0xD9, 0xDE, 0xFF];
const BLACK: Rgba = [0x3A, 0x3A, 0x3E, 0xFF];
const PINK: Rgba = [0xF2, 0x9C, 0x96, 0xFF];
const BLUE: Rgba = [0x2F, 0x6E, 0xB8, 0xFF];
const GOLD: Rgba = [0xF2, 0xB2, 0x22, 0xFF];
const WHITE: Rgba = [0xFF, 0xFF, 0xFF, 0xFF];

/// Points of a polygon given in a unit box, placed in the `size` box at (`x`, `y`).
fn place(points: &[[f32; 2]], x: f32, y: f32, size: f32) -> Vec<[f32; 2]> {
    points.iter().map(|p| [x + p[0] * size, y + p[1] * size]).collect()
}

/// An arrow pointing up, turned clockwise by `turn` eighths of a full turn.
fn arrow(turn: u8, color: Rgba, x: f32, y: f32, size: f32) -> Prim {
    const UP: [[f32; 2]; 7] = [[0.5, 0.06], [0.94, 0.5], [0.66, 0.5], [0.66, 0.94], [0.34, 0.94], [0.34, 0.5], [0.06, 0.5]];
    let angle = f32::from(turn) * std::f32::consts::FRAC_PI_4;
    let (sin, cos) = angle.sin_cos();
    let turned: Vec<[f32; 2]> = UP
        .iter()
        .map(|p| {
            let (dx, dy) = (p[0] - 0.5, p[1] - 0.5);
            [0.5 + dx * cos - dy * sin, 0.5 + dx * sin + dy * cos]
        })
        .collect();
    Prim::Polygon { pts: place(&turned, x, y, size), fill: color, stroke: None }
}

fn circle(color: Rgba, x: f32, y: f32, size: f32) -> Prim {
    Prim::Circle { cx: x + size / 2.0, cy: y + size / 2.0, r: size * 0.45, fill: Some(color), stroke: None }
}

/// A five-pointed star in the unit box.
fn star_points() -> Vec<[f32; 2]> {
    (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { 0.5 } else { 0.21 };
            let a = std::f32::consts::PI * i as f32 / 5.0;
            [0.5 + r * a.sin(), 0.53 - r * a.cos()]
        })
        .collect()
}

/// The part of a polygon left of `x = cut` (one Sutherland–Hodgman pass).
fn left_of(points: &[[f32; 2]], cut: f32) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    for (i, a) in points.iter().enumerate() {
        let b = points.get((i + 1) % points.len()).copied().unwrap_or(*a);
        let (a_in, b_in) = (a[0] <= cut, b[0] <= cut);
        if a_in {
            out.push(*a);
        }
        if a_in != b_in {
            let t = (cut - a[0]) / (b[0] - a[0]);
            out.push([cut, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

fn symbol_strokes(kind: u8, color: Rgba, x: f32, y: f32, size: f32, width: f32) -> Vec<Prim> {
    let line = |pts: &[[f32; 2]]| Prim::Line { pts: place(pts, x, y, size), color, width, dash: false };
    match kind {
        // A cross.
        0 => vec![line(&[[0.3, 0.3], [0.7, 0.7]]), line(&[[0.7, 0.3], [0.3, 0.7]])],
        // An exclamation mark.
        1 => vec![
            line(&[[0.5, 0.22], [0.5, 0.58]]),
            Prim::Circle { cx: x + 0.5 * size, cy: y + 0.75 * size, r: width * 0.6, fill: Some(color), stroke: None },
        ],
        // A check mark.
        _ => vec![line(&[[0.27, 0.52], [0.44, 0.69], [0.74, 0.33]])],
    }
}

/// Colour of the `index`-th of `count` icons going from red through yellow to green.
fn traffic(index: usize, count: usize) -> Rgba {
    match (count, index) {
        (4, 0) => BLACK,
        (4, 1) | (3, 0) => RED,
        (4, 2) | (3, 1) => YELLOW,
        _ => GREEN,
    }
}

/// The icon `index` (0 = lowest) of icon set `set`, drawn in the `size`-pixel square at (`x`, `y`).
pub fn icon(set: &str, index: usize, x: f32, y: f32, size: f32) -> Vec<Prim> {
    let count = set.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(3) as usize;
    let index = index.min(count.saturating_sub(1));
    let family = set.trim_start_matches(|c: char| c.is_ascii_digit());
    let stroke = (size * 0.13).max(1.2);
    match family {
        "Arrows" | "ArrowsGray" => {
            // Down, then turning up through the diagonals and the side.
            let turns: &[u8] = match count {
                4 => &[4, 3, 1, 0],
                5 => &[4, 3, 2, 1, 0],
                _ => &[4, 2, 0],
            };
            let color = if family == "ArrowsGray" {
                GRAY
            } else {
                match (count, index) {
                    (_, 0) => RED,
                    (3, 1) | (4, 1) | (4, 2) | (5, 1) | (5, 2) | (5, 3) => YELLOW,
                    _ => GREEN,
                }
            };
            vec![arrow(turns.get(index).copied().unwrap_or(0), color, x, y, size)]
        }
        "TrafficLights2" => vec![
            Prim::Rect { x: x + size * 0.05, y: y + size * 0.05, w: size * 0.9, h: size * 0.9, fill: Some(BLACK), stroke: None, radius: size * 0.22 },
            Prim::Circle { cx: x + size / 2.0, cy: y + size / 2.0, r: size * 0.3, fill: Some(traffic(index, count)), stroke: None },
        ],
        "TrafficLights1" | "TrafficLights" => vec![circle(traffic(index, count), x, y, size)],
        "RedToBlack" => vec![circle([BLACK, GRAY, PINK, RED][index.min(3)], x, y, size)],
        "Signs" => vec![match index {
            0 => Prim::Polygon { pts: place(&[[0.5, 0.05], [0.95, 0.5], [0.5, 0.95], [0.05, 0.5]], x, y, size), fill: RED, stroke: None },
            1 => Prim::Polygon { pts: place(&[[0.5, 0.08], [0.95, 0.9], [0.05, 0.9]], x, y, size), fill: YELLOW, stroke: None },
            _ => circle(GREEN, x, y, size),
        }],
        "Symbols" => {
            let color = [RED, YELLOW, GREEN][index.min(2)];
            let mut out = vec![circle(color, x, y, size)];
            out.extend(symbol_strokes(index.min(2) as u8, WHITE, x, y, size, stroke));
            out
        }
        "Symbols2" => {
            let color = [RED, YELLOW, GREEN][index.min(2)];
            symbol_strokes(index.min(2) as u8, color, x, y, size, stroke * 1.4)
        }
        "Flags" => {
            let color = [RED, YELLOW, GREEN][index.min(2)];
            vec![
                Prim::Line { pts: place(&[[0.22, 0.08], [0.22, 0.95]], x, y, size), color: BLACK, width: (size * 0.09).max(1.0), dash: false },
                Prim::Polygon { pts: place(&[[0.26, 0.08], [0.9, 0.3], [0.26, 0.55]], x, y, size), fill: color, stroke: None },
            ]
        }
        "Stars" => {
            let star = star_points();
            let mut out = vec![Prim::Polygon { pts: place(&star, x, y, size), fill: WHITE, stroke: Some((GOLD, (size * 0.07).max(1.0))) }];
            match index {
                0 => {}
                1 => out.push(Prim::Polygon { pts: place(&left_of(&star, 0.5), x, y, size), fill: GOLD, stroke: None }),
                _ => out.push(Prim::Polygon { pts: place(&star, x, y, size), fill: GOLD, stroke: None }),
            }
            out
        }
        "Triangles" => vec![match index {
            0 => Prim::Polygon { pts: place(&[[0.1, 0.25], [0.9, 0.25], [0.5, 0.85]], x, y, size), fill: RED, stroke: None },
            1 => Prim::Rect { x: x + size * 0.15, y: y + size * 0.42, w: size * 0.7, h: size * 0.16, fill: Some(YELLOW), stroke: None, radius: 0.0 },
            _ => Prim::Polygon { pts: place(&[[0.5, 0.15], [0.9, 0.75], [0.1, 0.75]], x, y, size), fill: GREEN, stroke: None },
        }],
        "Rating" => {
            // Four bars rising left to right; the lowest of four shows one, of five none.
            let lit = if count == 4 { index + 1 } else { index };
            (0..4)
                .map(|bar| {
                    let h = 0.25 + 0.2 * bar as f32;
                    Prim::Rect {
                        x: x + size * (0.08 + 0.22 * bar as f32),
                        y: y + size * (0.92 - h),
                        w: size * 0.17,
                        h: size * h,
                        fill: Some(if bar < lit { BLUE } else { LIGHT }),
                        stroke: None,
                        radius: 0.0,
                    }
                })
                .collect()
        }
        "Quarters" => {
            let (cx, cy, r) = (x + size / 2.0, y + size / 2.0, size * 0.44);
            let mut out = vec![Prim::Circle { cx, cy, r, fill: Some(WHITE), stroke: Some((BLACK, (size * 0.08).max(1.0))) }];
            if index > 0 {
                let a1 = std::f32::consts::TAU * index as f32 / 4.0;
                out.push(Prim::Wedge { cx, cy, r_outer: r, r_inner: 0.0, a0: 0.0, a1, fill: BLACK, stroke: None });
            }
            out
        }
        "Boxes" => (0..4)
            .map(|cell| {
                let (col, row) = ((cell % 2) as f32, (cell / 2) as f32);
                // Filled from the bottom left: the lowest of five is empty, the highest full.
                let order = [2, 3, 0, 1][cell];
                Prim::Rect {
                    x: x + size * (0.08 + 0.44 * col),
                    y: y + size * (0.08 + 0.44 * row),
                    w: size * 0.4,
                    h: size * 0.4,
                    fill: Some(if order < index { BLUE } else { LIGHT }),
                    stroke: None,
                    radius: 0.0,
                }
            })
            .collect(),
        _ => vec![circle(traffic(index.min(2), 3), x, y, size)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_set_draws_each_of_its_icons_differently() {
        for set in [
            "3Arrows",
            "3ArrowsGray",
            "3Flags",
            "3TrafficLights1",
            "3TrafficLights2",
            "3Signs",
            "3Symbols",
            "3Symbols2",
            "3Stars",
            "3Triangles",
            "4Arrows",
            "4ArrowsGray",
            "4RedToBlack",
            "4Rating",
            "4TrafficLights",
            "5Arrows",
            "5ArrowsGray",
            "5Rating",
            "5Quarters",
            "5Boxes",
        ] {
            let count = set[..1].parse::<usize>().unwrap();
            let images: Vec<Vec<u8>> = (0..count).map(|i| kition_sheet_chart::rasterize(&icon(set, i, 0.0, 0.0, 16.0), 16, 16, WHITE)).collect();
            for a in 0..count {
                assert!(images[a].chunks(4).any(|p| p != WHITE), "{set} {a} draws nothing");
                for b in a + 1..count {
                    assert_ne!(images[a], images[b], "{set}: icons {a} and {b} look the same");
                }
            }
        }
    }
}
