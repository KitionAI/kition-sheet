//! Kition Sheet charts (layer L3, toolkit-free).
//!
//! - [`resolve`] pulls series names, categories and values out of a workbook,
//! - [`render`] lays a chart out into a list of drawing primitives ([`Prim`]) that any UI
//!   toolkit (or the built-in [`rasterize`]) can paint,
//! - [`render_sparkline`] draws in-cell sparklines,
//! - [`render_shape`] draws a drawing shape's outline and fill (preset, custom geometry,
//!   gradients, transparency, rotation and mirroring; its text is left to the caller),
//! - [`render_diagram`] draws a smart graphic's kept drawing (outlines, text, picture boxes),
//! - [`rasterize`] turns primitives into RGBA pixels for PNG export and tests.
//!
//! The look (palette, greys, spacing) is Kition Sheet's own design.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod axis;
mod diagram;
mod raster;
mod render;
mod resolve;
mod shape;
mod sparkline;

#[cfg(test)]
mod tests;

pub use kition_sheet_model::{Chart, ChartKind, LegendPos, SparklineKind};
use serde::{Deserialize, Serialize};

pub use axis::{Scale, format_number, nice_scale};
pub use diagram::{DiagramDrawing, render_diagram};
pub use raster::rasterize;
pub use render::{Swatch, legend_entries, render};
pub use resolve::{default_palette, resolve, series_color};
pub use shape::{render_shape, shape_rgba};
pub use sparkline::render_sparkline;

/// Straight (non-premultiplied) RGBA colour.
pub type Rgba = [u8; 4];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// A drawing primitive in chart coordinates (origin top-left, y down, units = points/pixels).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Prim {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Option<Rgba>,
        stroke: Option<(Rgba, f32)>,
        radius: f32,
    },
    Line {
        pts: Vec<[f32; 2]>,
        color: Rgba,
        width: f32,
        dash: bool,
    },
    Polygon {
        pts: Vec<[f32; 2]>,
        fill: Rgba,
        stroke: Option<(Rgba, f32)>,
    },
    /// Pie/doughnut sector: centre, radii, angles in radians (0 = 12 o'clock, clockwise).
    Wedge {
        cx: f32,
        cy: f32,
        r_outer: f32,
        r_inner: f32,
        a0: f32,
        a1: f32,
        fill: Rgba,
        stroke: Option<(Rgba, f32)>,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: Option<Rgba>,
        stroke: Option<(Rgba, f32)>,
    },
    /// Contours filled together with the even-odd rule (holes where they overlap), in a colour or
    /// a gradient.
    Path {
        contours: Vec<Vec<[f32; 2]>>,
        paint: Paint,
    },
    /// Text anchored at `(x, y)` per `align`/`valign`, rotated by `rotation` radians about the anchor.
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: Rgba,
        bold: bool,
        align: HAlign,
        valign: VAlign,
        rotation: f32,
    },
}

impl Prim {
    /// The primitive moved by (`dx`, `dy`) (lines, polygons and paths; others as they are).
    pub fn translated(&self, dx: f32, dy: f32) -> Prim {
        let mv = |pts: &[[f32; 2]]| pts.iter().map(|q| [q[0] + dx, q[1] + dy]).collect::<Vec<_>>();
        match self {
            Prim::Line { pts, color, width, dash } => Prim::Line { pts: mv(pts), color: *color, width: *width, dash: *dash },
            Prim::Polygon { pts, fill, stroke } => Prim::Polygon { pts: mv(pts), fill: *fill, stroke: *stroke },
            Prim::Rect { x, y, w, h, fill, stroke, radius } => {
                Prim::Rect { x: x + dx, y: y + dy, w: *w, h: *h, fill: *fill, stroke: *stroke, radius: *radius }
            }
            Prim::Circle { cx, cy, r, fill, stroke } => Prim::Circle { cx: cx + dx, cy: cy + dy, r: *r, fill: *fill, stroke: *stroke },
            Prim::Path { contours, paint } => Prim::Path {
                contours: contours.iter().map(|c| mv(c)).collect(),
                paint: match paint {
                    Paint::Linear { from, to, stops } => {
                        Paint::Linear { from: [from[0] + dx, from[1] + dy], to: [to[0] + dx, to[1] + dy], stops: stops.clone() }
                    }
                    Paint::Radial { center, radius, stops } => {
                        Paint::Radial { center: [center[0] + dx, center[1] + dy], radius: *radius, stops: stops.clone() }
                    }
                    solid => solid.clone(),
                },
            },
            Prim::Wedge { cx, cy, r_outer, r_inner, a0, a1, fill, stroke } => {
                Prim::Wedge { cx: cx + dx, cy: cy + dy, r_outer: *r_outer, r_inner: *r_inner, a0: *a0, a1: *a1, fill: *fill, stroke: *stroke }
            }
            Prim::Text { x, y, text, size, color, bold, align, valign, rotation } => Prim::Text {
                x: x + dx,
                y: y + dy,
                text: text.clone(),
                size: *size,
                color: *color,
                bold: *bold,
                align: *align,
                valign: *valign,
                rotation: *rotation,
            },
        }
    }

    /// The primitive at `s` times its size (from its units to output pixels).
    pub fn scaled(&self, s: f32) -> Prim {
        let points = |pts: &[[f32; 2]]| pts.iter().map(|[x, y]| [x * s, y * s]).collect::<Vec<_>>();
        let stroke = |stroke: &Option<(Rgba, f32)>| stroke.map(|(color, width)| (color, width * s));
        match self {
            Prim::Rect { x, y, w, h, fill, stroke: line, radius } => {
                Prim::Rect { x: x * s, y: y * s, w: w * s, h: h * s, fill: *fill, stroke: stroke(line), radius: radius * s }
            }
            Prim::Line { pts, color, width, dash } => Prim::Line { pts: points(pts), color: *color, width: width * s, dash: *dash },
            Prim::Polygon { pts, fill, stroke: line } => Prim::Polygon { pts: points(pts), fill: *fill, stroke: stroke(line) },
            Prim::Wedge { cx, cy, r_outer, r_inner, a0, a1, fill, stroke: line } => Prim::Wedge {
                cx: cx * s,
                cy: cy * s,
                r_outer: r_outer * s,
                r_inner: r_inner * s,
                a0: *a0,
                a1: *a1,
                fill: *fill,
                stroke: stroke(line),
            },
            Prim::Circle { cx, cy, r, fill, stroke: line } => Prim::Circle { cx: cx * s, cy: cy * s, r: r * s, fill: *fill, stroke: stroke(line) },
            Prim::Path { contours, paint } => Prim::Path {
                contours: contours.iter().map(|c| points(c)).collect(),
                paint: match paint {
                    Paint::Solid(c) => Paint::Solid(*c),
                    Paint::Linear { from, to, stops } => {
                        Paint::Linear { from: [from[0] * s, from[1] * s], to: [to[0] * s, to[1] * s], stops: stops.clone() }
                    }
                    Paint::Radial { center, radius, stops } => {
                        Paint::Radial { center: [center[0] * s, center[1] * s], radius: radius * s, stops: stops.clone() }
                    }
                },
            },
            Prim::Text { x, y, text, size, color, bold, align, valign, rotation } => Prim::Text {
                x: x * s,
                y: y * s,
                text: text.clone(),
                size: size * s,
                color: *color,
                bold: *bold,
                align: *align,
                valign: *valign,
                rotation: *rotation,
            },
        }
    }
}

/// How a [`Prim::Path`] is filled: a colour, or colour stops (position 0–1, colour) along a line
/// from `from` to `to`, or out from `center` to `radius`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Paint {
    Solid(Rgba),
    Linear { from: [f32; 2], to: [f32; 2], stops: Vec<(f32, Rgba)> },
    Radial { center: [f32; 2], radius: f32, stops: Vec<(f32, Rgba)> },
}

impl Paint {
    /// The colour at a point.
    pub fn at(&self, x: f32, y: f32) -> Rgba {
        let (t, stops) = match self {
            Paint::Solid(c) => return *c,
            Paint::Linear { from, to, stops } => {
                let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 { ((x - from[0]) * dx + (y - from[1]) * dy) / len2 } else { 0.0 };
                (t, stops)
            }
            Paint::Radial { center, radius, stops } => {
                let d = ((x - center[0]).powi(2) + (y - center[1]).powi(2)).sqrt();
                (if *radius > 0.0 { d / radius } else { 0.0 }, stops)
            }
        };
        stop_color(stops, t)
    }

    /// One colour standing for the paint (its middle), for surfaces that draw only flat colours.
    pub fn mean(&self) -> Rgba {
        match self {
            Paint::Solid(c) => *c,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => stop_color(stops, 0.5),
        }
    }

    /// Whether every pixel is opaque.
    pub fn opaque(&self) -> bool {
        match self {
            Paint::Solid(c) => c[3] == 255,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => stops.iter().all(|s| s.1[3] == 255),
        }
    }
}

/// The colour `t` (0–1, clamped) of the way along colour stops sorted by position.
fn stop_color(stops: &[(f32, Rgba)], t: f32) -> Rgba {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let Some(first) = stops.first() else { return [0, 0, 0, 0] };
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.0 {
            let k = if b.0 > a.0 { (t - a.0) / (b.0 - a.0) } else { 1.0 };
            let mix = |i: usize| (f32::from(a.1[i]) + (f32::from(b.1[i]) - f32::from(a.1[i])) * k).round().clamp(0.0, 255.0) as u8;
            return [mix(0), mix(1), mix(2), mix(3)];
        }
    }
    stops.last().map_or(first.1, |s| s.1)
}

/// Measures text width (the UI passes real font metrics; tests use a fixed-width approximation).
pub trait Measure {
    fn text_width(&self, text: &str, size: f32, bold: bool) -> f32;
}

/// Fixed-width approximation: `0.55 × size` per character (bold 10% wider).
#[derive(Clone, Copy, Debug, Default)]
pub struct ApproxMeasure;

impl Measure for ApproxMeasure {
    fn text_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let n = text.chars().count() as f32;
        n * 0.55 * size * if bold { 1.1 } else { 1.0 }
    }
}

/// Resolved data: series names, categories and values pulled from the workbook.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartData {
    pub categories: Vec<String>,
    pub series: Vec<SeriesData>,
    /// A pie's, doughnut's, treemap's or sunburst's point colours (the chart's own, else the
    /// workbook theme's accents); empty: the default theme's.
    #[serde(default)]
    pub points: Vec<Rgba>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeriesData {
    pub name: String,
    pub values: Vec<Option<f64>>,
    /// Numeric X values (scatter, bubble).
    pub x: Option<Vec<Option<f64>>>,
    /// Bubble sizes.
    pub sizes: Option<Vec<Option<f64>>>,
    pub color: Rgba,
    /// Per-series kind (combo charts).
    pub kind: ChartKind,
    /// Plotted against the secondary (right) value axis.
    pub secondary: bool,
    /// Draw line segments as a smoothed (spline) curve.
    pub smooth: bool,
    /// Number format code of the source values ("General" by default).
    pub number_format: String,
    /// Its trendline and error bars (KitDrawing 1.1 DR16b), fitted and measured by
    /// `kition_office_core::chart::trend`.
    pub trendline: Option<kition_office_core::chart::Trendline>,
    pub error_bars: Option<kition_office_core::chart::ErrorBars>,
    /// Its line's width in points; `None`: the chart's (2.25).
    pub line_width: Option<f32>,
    /// Its marker symbol (`none`, `circle`, `square`…); `None`: the type's.
    pub marker: Option<String>,
}

impl Default for SeriesData {
    fn default() -> Self {
        SeriesData {
            name: String::new(),
            values: vec![],
            x: None,
            sizes: None,
            color: [0x15, 0x60, 0x82, 0xFF],
            kind: ChartKind::ColumnClustered,
            secondary: false,
            smooth: false,
            number_format: "General".into(),
            trendline: None,
            error_bars: None,
            line_width: None,
            marker: None,
        }
    }
}
