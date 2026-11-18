//! DrawingML charts (`xl/charts/chartN.xml`): reading the common 2-D/3-D chart groups into the
//! model, and writing model charts (with cached series values) from scratch.

use std::fmt::Write as _;

use kition_sheet_core::Value;
use kition_sheet_formula::{Expr, SheetSel};
use kition_sheet_model::{Chart, ChartAxis, ChartKind, Color, LegendPos, Series, SeriesErrors, SeriesTrend, Workbook};

use crate::drawing::{dml_color, srgb};
use crate::xml::{El, esc, num};

// ---------------------------------------------------------------- reading

fn group_kind(g: &El) -> Option<ChartKind> {
    let grouping = g.child_val("grouping").unwrap_or("standard");
    let stacked = grouping == "stacked";
    let pct = grouping == "percentStacked";
    Some(match g.name.as_str() {
        "barChart" | "bar3DChart" => {
            let bar = g.child_val("barDir") == Some("bar");
            match (bar, stacked, pct) {
                (false, true, _) => ChartKind::ColumnStacked,
                (false, _, true) => ChartKind::ColumnStacked100,
                (false, _, _) => ChartKind::ColumnClustered,
                (true, true, _) => ChartKind::BarStacked,
                (true, _, true) => ChartKind::BarStacked100,
                (true, _, _) => ChartKind::BarClustered,
            }
        }
        "lineChart" | "line3DChart" => {
            if stacked || pct {
                ChartKind::LineStacked
            } else {
                let group_markers = g.child_val("marker").is_none_or(|v| v == "1" || v == "true");
                // A line chart's series have no markers; one with a marker of its own stays a line
                // chart while any other has none.
                let any_none = g.kids("ser").any(|s| s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")) == Some("none"));
                if !group_markers || any_none { ChartKind::Line } else { ChartKind::LineMarkers }
            }
        }
        "pieChart" | "pie3DChart" | "ofPieChart" => ChartKind::Pie,
        "doughnutChart" => ChartKind::Doughnut,
        "areaChart" | "area3DChart" => {
            if stacked || pct {
                ChartKind::AreaStacked
            } else {
                ChartKind::Area
            }
        }
        "scatterChart" => {
            let no_lines = g.kids("ser").all(|s| s.path(&["spPr", "ln"]).is_some_and(|l| l.child("noFill").is_some()));
            if g.child_val("scatterStyle") == Some("marker") || (no_lines && g.kids("ser").next().is_some()) {
                ChartKind::Scatter
            } else {
                ChartKind::ScatterLines
            }
        }
        "bubbleChart" => ChartKind::Bubble,
        "radarChart" => ChartKind::Radar,
        "stockChart" => ChartKind::Stock,
        _ => return None,
    })
}

fn data_ref(e: Option<&El>) -> Option<String> {
    let e = e?;
    for k in ["numRef", "strRef", "multiLvlStrRef"] {
        if let Some(f) = e.child(k).and_then(|r| r.child("f")) {
            let t = f.text.trim();
            return Some(t.strip_prefix('=').unwrap_or(t).to_string());
        }
    }
    // Literal values become an array constant.
    for k in ["numLit", "strLit"] {
        if let Some(l) = e.child(k) {
            let pts: Vec<String> = l
                .kids("pt")
                .filter_map(|p| p.child("v"))
                .map(|v| if k == "numLit" { v.text.trim().to_string() } else { format!("\"{}\"", v.text.replace('"', "\"\"")) })
                .collect();
            return Some(format!("{{{}}}", pts.join(",")));
        }
    }
    None
}

fn read_series(s: &El) -> Series {
    let name = s.child("tx").and_then(|tx| {
        if let Some(f) = tx.path(&["strRef", "f"]) {
            let t = f.text.trim();
            Some(t.strip_prefix('=').unwrap_or(t).to_string())
        } else {
            tx.child("v").map(|v| v.text.clone())
        }
    });
    let sp = s.child("spPr");
    let color = sp
        .and_then(|p| p.child("solidFill"))
        .and_then(dml_color)
        .or_else(|| sp.and_then(|p| p.path(&["ln", "solidFill"])).and_then(dml_color))
        .or_else(|| s.path(&["marker", "spPr", "solidFill"]).and_then(dml_color));
    Series {
        name,
        categories: data_ref(s.child("cat")).or_else(|| data_ref(s.child("xVal"))),
        values: data_ref(s.child("val")).or_else(|| data_ref(s.child("yVal"))).unwrap_or_default(),
        bubble_sizes: data_ref(s.child("bubbleSize")),
        color,
        secondary: false,
        kind: None,
        smooth: s.child("smooth").and_then(|e| e.attr("val")).and_then(|v| v.parse::<u8>().ok()).is_some_and(|v| v != 0),
        trendline: s.child("trendline").map(|t| {
            let num = |k: &str| t.child(k).and_then(|e| e.attr("val")).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
            SeriesTrend {
                kind: t.child("trendlineType").and_then(|e| e.attr("val")).unwrap_or("linear").to_string(),
                order: num("order").map(|v| v as u32),
                period: num("period").map(|v| v as u32),
                forward: num("forward").filter(|v| *v > 0.0),
                backward: num("backward").filter(|v| *v > 0.0),
            }
        }),
        error_bars: s
            .kids("errBars")
            .find(|e| e.child("errDir").and_then(|d| d.attr("val")).is_none_or(|d| d == "y"))
            .filter(|e| e.child("errBarType").and_then(|d| d.attr("val")).is_none_or(|t| t == "both"))
            .map(|e| SeriesErrors {
                kind: e.child("errValType").and_then(|v| v.attr("val")).unwrap_or("fixedVal").to_string(),
                value: e.child("val").and_then(|v| v.attr("val")).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()),
            }),
        line_width: sp
            .and_then(|p| p.child("ln"))
            .and_then(|l| l.attr("w"))
            .and_then(|w| w.trim().parse::<f64>().ok())
            .filter(|w| w.is_finite() && *w >= 0.0)
            .map(|w| (w / 12_700.0).min(1584.0) as f32),
        marker: s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")).filter(|v| MARKERS.contains(v)).map(str::to_string),
    }
}

/// The colours a series' points (`c:dPt`) are filled with, in point order; points without one
/// are automatic. Empty when none has one.
fn slice_colors(s: &El) -> Vec<Color> {
    let mut colors = Vec::new();
    for pt in s.kids("dPt") {
        let Some(idx) = pt.child_val("idx").and_then(|v| v.parse::<usize>().ok()).filter(|i| *i < 10_000) else { continue };
        let Some(color) = pt.path(&["spPr", "solidFill"]).and_then(dml_color) else { continue };
        if colors.len() <= idx {
            colors.resize(idx + 1, Color::Auto);
        }
        if let Some(slot) = colors.get_mut(idx) {
            *slot = color;
        }
    }
    colors
}

fn rich_title(t: &El) -> Option<String> {
    let tx = t.child("tx")?;
    if let Some(rich) = tx.child("rich") {
        let paras: Vec<String> =
            rich.kids("p").map(|p| p.kids("r").filter_map(|r| r.child("t")).map(|t| t.text.as_str()).collect::<String>()).collect();
        let s = paras.join("\n");
        return if s.is_empty() { None } else { Some(s) };
    }
    if let Some(sr) = tx.child("strRef") {
        if let Some(v) = sr.path(&["strCache", "pt", "v"]) {
            return Some(v.text.clone());
        }
        return sr.child("f").map(|f| f.text.trim().to_string());
    }
    None
}

fn axis_ids(g: &El) -> Vec<String> {
    g.kids("axId").filter_map(|a| a.attr("val")).map(str::to_string).collect()
}

pub fn read_chart(root: &El) -> Option<Chart> {
    let chart = root.child("chart")?;
    let plot = chart.child("plotArea")?;
    let groups: Vec<(&El, ChartKind)> = plot.children.iter().filter_map(|g| group_kind(g).map(|k| (g, k))).collect();
    let (first, first_kind) = *groups.first()?;
    let first_axes = axis_ids(first);
    let mut series = vec![];
    for (g, k) in &groups {
        for s in g.kids("ser") {
            let mut ser = read_series(s);
            // A marker is the series' own where it is not its type's.
            let default = match k {
                ChartKind::Line | ChartKind::LineStacked | ChartKind::ScatterLines => "none",
                _ => "auto",
            };
            if ser.marker.as_deref() == Some(default) {
                ser.marker = None;
            }
            // In a combination every series has its group's type.
            if groups.len() > 1 {
                ser.kind = Some(*k);
                ser.secondary = axis_ids(g) != first_axes;
            }
            series.push(ser);
        }
    }
    let kind = if groups.len() > 1 { ChartKind::Combo } else { first_kind };
    let show_val = |e: &El| e.child("dLbls").and_then(|d| d.child_val("showVal")).is_some_and(|v| v == "1" || v == "true");
    let data_labels = groups.iter().any(|(g, _)| show_val(g) || g.kids("ser").any(show_val));
    let val_axes: Vec<&El> = plot.kids("valAx").collect();
    let cat_ax = plot.child("catAx").or_else(|| plot.child("dateAx"));
    let scatter = matches!(first_kind, ChartKind::Scatter | ChartKind::ScatterLines | ChartKind::Bubble);
    let (x_ax, y_ax) = if scatter {
        let x = val_axes.iter().find(|a| matches!(a.child_val("axPos"), Some("b") | Some("t"))).copied();
        let y = val_axes.iter().find(|a| matches!(a.child_val("axPos"), Some("l") | Some("r"))).copied();
        (x, y)
    } else {
        (cat_ax, val_axes.first().copied())
    };
    // A pie's or doughnut's slice colours (its first series' `c:dPt`).
    let point_colors = match first_kind {
        ChartKind::Pie | ChartKind::Doughnut => first.kids("ser").next().map(slice_colors).unwrap_or_default(),
        _ => vec![],
    };
    let legend = match chart.child("legend") {
        None => LegendPos::None,
        Some(l) => match l.child_val("legendPos").unwrap_or("r") {
            "b" => LegendPos::Bottom,
            "t" => LegendPos::Top,
            "l" => LegendPos::Left,
            _ => LegendPos::Right,
        },
    };
    Some(Chart {
        id: 0,
        kind,
        anchor: Default::default(),
        title: chart.child("title").and_then(rich_title),
        series,
        legend,
        data_labels,
        gridlines: val_axes.iter().any(|a| a.child("majorGridlines").is_some()),
        style: root.child_val("style").and_then(|v| v.parse().ok()).unwrap_or(2),
        x_title: x_ax.and_then(|a| a.child("title")).and_then(rich_title),
        y_title: y_ax.and_then(|a| a.child("title")).and_then(rich_title),
        source: root.path(&["extLst", "ext", "source"]).and_then(|source| source.attr("range")).map(str::to_owned),
        by_rows: root.path(&["extLst", "ext", "source"]).and_then(|source| source.attr("byRows")) == Some("1"),
        x_axis: x_ax.map(axis_of).unwrap_or_default(),
        y_axis: y_ax.map(axis_of).unwrap_or_default(),
        label_position: groups
            .iter()
            .flat_map(|(g, _)| std::iter::once(*g).chain(g.kids("ser")))
            .find_map(|e| e.path(&["dLbls", "dLblPos"]).and_then(|p| p.attr("val")))
            .filter(|v| LABEL_POSITIONS.contains(v))
            .map(str::to_string),
        totals: vec![],
        point_colors,
    })
}

/// Marker symbols (DrawingML `ST_MarkerStyle`).
const MARKERS: [&str; 11] = ["none", "auto", "circle", "dash", "diamond", "dot", "plus", "square", "star", "triangle", "x"];
/// Data label positions (DrawingML `ST_DLblPos`).
const LABEL_POSITIONS: [&str; 9] = ["bestFit", "b", "ctr", "inBase", "inEnd", "l", "outEnd", "r", "t"];

/// An axis's scale and labels as the file sets them: bounds, major unit, its own number format,
/// and whether it is deleted.
fn axis_of(a: &El) -> ChartAxis {
    let num = |e: Option<&El>| e.and_then(|e| e.attr("val")).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    ChartAxis {
        min: num(a.path(&["scaling", "min"])),
        max: num(a.path(&["scaling", "max"])),
        major: num(a.child("majorUnit")).filter(|v| *v > 0.0),
        number_format: a
            .child("numFmt")
            .filter(|f| f.attr("sourceLinked").is_some_and(|v| v == "0" || v == "false"))
            .and_then(|f| f.attr("formatCode"))
            .filter(|c| !c.is_empty() && *c != "General")
            .map(str::to_string),
        hidden: a.child_val("delete").is_some_and(|v| v == "1" || v == "true"),
    }
}

// ---------------------------------------------------------------- writing

#[derive(Clone, Copy, PartialEq)]
enum G {
    Bar { bar: bool, grouping: &'static str },
    Line { stacked: bool, markers: bool },
    Pie,
    Doughnut,
    Area { stacked: bool },
    Scatter { lines: bool },
    Bubble,
    Radar,
    Stock,
}

fn group_for(k: ChartKind) -> G {
    match k {
        ChartKind::ColumnStacked => G::Bar { bar: false, grouping: "stacked" },
        ChartKind::ColumnStacked100 => G::Bar { bar: false, grouping: "percentStacked" },
        ChartKind::BarClustered => G::Bar { bar: true, grouping: "clustered" },
        ChartKind::BarStacked => G::Bar { bar: true, grouping: "stacked" },
        ChartKind::BarStacked100 => G::Bar { bar: true, grouping: "percentStacked" },
        ChartKind::Line => G::Line { stacked: false, markers: false },
        ChartKind::LineMarkers => G::Line { stacked: false, markers: true },
        ChartKind::LineStacked => G::Line { stacked: true, markers: false },
        ChartKind::Pie => G::Pie,
        ChartKind::Doughnut => G::Doughnut,
        ChartKind::Area => G::Area { stacked: false },
        ChartKind::AreaStacked => G::Area { stacked: true },
        ChartKind::Scatter => G::Scatter { lines: false },
        ChartKind::ScatterLines => G::Scatter { lines: true },
        ChartKind::Bubble => G::Bubble,
        ChartKind::Radar => G::Radar,
        ChartKind::Stock => G::Stock,
        // Combo charts start from clustered columns; chartex kinds have their own part.
        _ => G::Bar { bar: false, grouping: "clustered" },
    }
}

const MAX_CACHE: u64 = 100_000;

/// Values of a same-workbook reference (`Sheet1!$B$2:$B$9`), row-major.
pub(crate) fn ref_values(wb: &Workbook, sheet_idx: usize, f: &str) -> Option<Vec<Value>> {
    let e = kition_sheet_formula::parse(f).ok()?;
    let Expr::Ref(r) = e else { return None };
    let si = match &r.sheet {
        SheetSel::Current => sheet_idx,
        SheetSel::Named(n) => wb.sheet_index(n)?,
        SheetSel::Span(..) => return None,
    };
    let sheet = wb.sheet(si)?;
    let mut range = r.range();
    // Whole columns/rows: limit to the used range.
    if let Some(u) = sheet.used_range() {
        range = range.intersection(&u)?;
    }
    if range.count() > MAX_CACHE {
        return None;
    }
    Some(range.iter().map(|c| sheet.value(c)).collect())
}

fn is_ref(f: &str) -> bool {
    matches!(kition_sheet_formula::parse(f), Ok(Expr::Ref(_)))
}

fn num_data(tag: &str, f: Option<&str>, wb: &Workbook, si: usize) -> String {
    let f = f.unwrap_or("").trim();
    if f.is_empty() {
        return format!("<c:{tag}><c:numLit><c:ptCount val=\"0\"/></c:numLit></c:{tag}>");
    }
    if f.starts_with('{') {
        let vals: Vec<&str> = f.trim_matches(|c| c == '{' || c == '}').split([',', ';']).collect();
        let mut s = format!("<c:{tag}><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            if let Ok(n) = v.trim().parse::<f64>() {
                let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", num(n));
            }
        }
        let _ = write!(s, "</c:numLit></c:{tag}>");
        return s;
    }
    let mut s = format!("<c:{tag}><c:numRef><c:f>{}</c:f>", esc(&crate::fmla::text_to_file(f)));
    if let Some(vals) = ref_values(wb, si, f) {
        let _ = write!(s, "<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            if let Value::Number(n) = v {
                let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", num(*n));
            }
        }
        s.push_str("</c:numCache>");
    }
    let _ = write!(s, "</c:numRef></c:{tag}>");
    s
}

fn str_data(tag: &str, f: &str, wb: &Workbook, si: usize) -> String {
    let f = f.trim();
    if f.starts_with('{') {
        let vals: Vec<String> = f.trim_matches(|c| c == '{' || c == '}').split([',', ';']).map(|v| v.trim().trim_matches('"').to_string()).collect();
        let mut s = format!("<c:{tag}><c:strLit><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc(v));
        }
        let _ = write!(s, "</c:strLit></c:{tag}>");
        return s;
    }
    let mut s = format!("<c:{tag}><c:strRef><c:f>{}</c:f>", esc(&crate::fmla::text_to_file(f)));
    if let Some(vals) = ref_values(wb, si, f) {
        let _ = write!(s, "<c:strCache><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            if !v.is_empty() {
                let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc(&v.display()));
            }
        }
        s.push_str("</c:strCache>");
    }
    let _ = write!(s, "</c:strRef></c:{tag}>");
    s
}

fn series_xml(s: &Series, idx: usize, g: G, wb: &Workbook, si: usize, points: &[Color]) -> String {
    let mut x = format!("<c:ser><c:idx val=\"{idx}\"/><c:order val=\"{idx}\"/>");
    if let Some(n) = &s.name {
        if is_ref(n) {
            x.push_str(&str_data("tx", n, wb, si));
        } else {
            let lit = n.trim().strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(n);
            let _ = write!(x, "<c:tx><c:v>{}</c:v></c:tx>", esc(lit));
        }
    }
    let color = s.color.as_ref().and_then(|c: &Color| srgb(c, &wb.theme));
    let line_like = matches!(g, G::Line { .. } | G::Radar | G::Scatter { lines: true });
    let width = s.line_width.filter(|w| w.is_finite() && *w >= 0.0).map(|w| (f64::from(w.min(1584.0)) * 12_700.0).round() as i64);
    match (&color, line_like) {
        (Some(c), true) => {
            let _ = write!(x, "<c:spPr><a:ln w=\"{}\" cap=\"rnd\"><a:solidFill>{c}</a:solidFill><a:round/></a:ln></c:spPr>", width.unwrap_or(28575));
        }
        (None, true) if width.is_some() => {
            let _ = write!(x, "<c:spPr><a:ln w=\"{}\" cap=\"rnd\"><a:round/></a:ln></c:spPr>", width.unwrap_or(28575));
        }
        (Some(c), false) if g == (G::Scatter { lines: false }) => {
            let _ = write!(
                x,
                "<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr><c:marker><c:symbol val=\"circle\"/><c:size val=\"5\"/><c:spPr><a:solidFill>{c}</a:solidFill></c:spPr></c:marker>"
            );
        }
        (Some(c), false) => {
            let outline = width.map(|w| format!("<a:ln w=\"{w}\"/>")).unwrap_or_default();
            let _ = write!(x, "<c:spPr><a:solidFill>{c}</a:solidFill>{outline}</c:spPr>");
        }
        (None, false) if width.is_some() && !matches!(g, G::Scatter { lines: false } | G::Stock) => {
            let _ = write!(x, "<c:spPr><a:ln w=\"{}\"/></c:spPr>", width.unwrap_or(0));
        }
        (None, _) if g == (G::Scatter { lines: false }) || g == G::Stock => x.push_str("<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr>"),
        _ => {}
    }
    let marked = matches!(g, G::Line { .. } | G::Scatter { .. } | G::Radar);
    match g {
        _ if marked && s.marker.as_deref().is_some_and(|m| MARKERS.contains(&m)) => {
            let m = s.marker.as_deref().unwrap_or("auto");
            let _ = write!(x, "<c:marker><c:symbol val=\"{m}\"/>");
            if m != "none" {
                x.push_str("<c:size val=\"5\"/>");
                if let Some(c) = &color {
                    let _ = write!(x, "<c:spPr><a:solidFill>{c}</a:solidFill></c:spPr>");
                }
            }
            x.push_str("</c:marker>");
        }
        G::Bar { .. } => x.push_str("<c:invertIfNegative val=\"0\"/>"),
        G::Line { markers: false, .. } | G::Scatter { lines: true } | G::Stock => x.push_str("<c:marker><c:symbol val=\"none\"/></c:marker>"),
        // Stated, so a series without markers beside it does not make it a line chart's.
        G::Line { markers: true, .. } => x.push_str("<c:marker><c:symbol val=\"auto\"/></c:marker>"),
        G::Bubble => x.push_str("<c:invertIfNegative val=\"0\"/>"),
        _ => {}
    }
    // Trendline and error bars, where the chart type takes them.
    if matches!(g, G::Bar { .. } | G::Line { .. } | G::Area { .. } | G::Scatter { .. }) {
        let word = |v: &str| v.chars().all(|c| c.is_ascii_alphanumeric());
        if let Some(t) = s.trendline.as_ref().filter(|t| word(&t.kind)) {
            let _ = write!(x, "<c:trendline><c:trendlineType val=\"{}\"/>", t.kind);
            if t.kind == "poly" {
                let _ = write!(x, "<c:order val=\"{}\"/>", t.order.unwrap_or(2).clamp(2, 6));
            }
            if t.kind == "movingAvg" {
                let _ = write!(x, "<c:period val=\"{}\"/>", t.period.unwrap_or(2).clamp(2, 255));
            } else {
                if let Some(f) = t.forward.filter(|v| v.is_finite() && *v > 0.0) {
                    let _ = write!(x, "<c:forward val=\"{f}\"/>");
                }
                if let Some(b) = t.backward.filter(|v| v.is_finite() && *v > 0.0) {
                    let _ = write!(x, "<c:backward val=\"{b}\"/>");
                }
            }
            x.push_str("<c:dispRSqr val=\"0\"/><c:dispEq val=\"0\"/></c:trendline>");
        }
        if let Some(e) = s.error_bars.as_ref().filter(|e| word(&e.kind)) {
            x.push_str("<c:errBars>");
            if matches!(g, G::Scatter { .. }) {
                x.push_str("<c:errDir val=\"y\"/>");
            }
            let _ = write!(x, "<c:errBarType val=\"both\"/><c:errValType val=\"{}\"/><c:noEndCap val=\"0\"/>", e.kind);
            if e.kind != "stdErr" {
                let v = e.value.filter(|v| v.is_finite()).unwrap_or(if e.kind == "percentage" { 5.0 } else { 1.0 });
                let _ = write!(x, "<c:val val=\"{v}\"/>");
            }
            x.push_str("</c:errBars>");
        }
    }
    // A pie's or doughnut's slice colours.
    for (j, c) in points.iter().enumerate().take(10_000) {
        if let Some(c) = srgb(c, &wb.theme) {
            let _ = write!(x, "<c:dPt><c:idx val=\"{j}\"/><c:bubble3D val=\"0\"/><c:spPr><a:solidFill>{c}</a:solidFill></c:spPr></c:dPt>");
        }
    }
    match g {
        G::Scatter { .. } | G::Bubble => {
            if let Some(c) = &s.categories {
                x.push_str(&num_data("xVal", Some(c), wb, si));
            }
            x.push_str(&num_data("yVal", Some(&s.values), wb, si));
            if g == G::Bubble {
                x.push_str(&num_data("bubbleSize", s.bubble_sizes.as_deref().or(Some(&s.values)), wb, si));
                x.push_str("<c:bubble3D val=\"0\"/>");
            } else {
                let _ = write!(x, "<c:smooth val=\"{}\"/>", s.smooth as u8);
            }
        }
        _ => {
            if let Some(c) = &s.categories {
                x.push_str(&str_data("cat", c, wb, si));
            }
            x.push_str(&num_data("val", Some(&s.values), wb, si));
            if matches!(g, G::Line { .. }) {
                let _ = write!(x, "<c:smooth val=\"{}\"/>", s.smooth as u8);
            }
        }
    }
    x.push_str("</c:ser>");
    x
}

fn dlbls(show: bool, position: Option<&str>) -> String {
    let pos = position.map(|p| format!("<c:dLblPos val=\"{p}\"/>")).unwrap_or_default();
    format!(
        "<c:dLbls>{pos}<c:showLegendKey val=\"0\"/><c:showVal val=\"{}\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>",
        show as u8
    )
}

/// The data label positions a chart group takes (Excel refuses a file naming another).
fn positions_for(g: G) -> &'static [&'static str] {
    match g {
        G::Bar { grouping: "clustered", .. } => &["outEnd", "inEnd", "ctr", "inBase"],
        G::Bar { .. } => &["inEnd", "ctr", "inBase"],
        G::Line { .. } | G::Scatter { .. } => &["t", "b", "l", "r", "ctr"],
        G::Pie => &["bestFit", "outEnd", "inEnd", "ctr"],
        G::Bubble => &["t", "b", "l", "r", "ctr"],
        G::Doughnut | G::Area { .. } | G::Radar | G::Stock => &[],
    }
}

fn group_xml(g: G, series: &[(usize, &Series)], ch: &Chart, axes: (u32, u32), wb: &Workbook, si: usize) -> String {
    let points = |i: usize| if matches!(g, G::Pie | G::Doughnut) && i == 0 { ch.point_colors.as_slice() } else { &[] };
    let sers: String = series.iter().map(|(i, s)| series_xml(s, *i, g, wb, si, points(*i))).collect();
    let dl = dlbls(ch.data_labels, ch.label_position.as_deref().filter(|p| ch.data_labels && positions_for(g).contains(p)));
    let ax = format!("<c:axId val=\"{}\"/><c:axId val=\"{}\"/>", axes.0, axes.1);
    match g {
        G::Bar { bar, grouping } => {
            let overlap = if grouping == "clustered" { String::new() } else { "<c:overlap val=\"100\"/>".into() };
            format!(
                "<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}{dl}<c:gapWidth val=\"150\"/>{overlap}{ax}</c:barChart>",
                if bar { "bar" } else { "col" }
            )
        }
        G::Line { stacked, .. } => format!(
            "<c:lineChart><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{sers}{dl}<c:marker val=\"1\"/>{ax}</c:lineChart>",
            if stacked { "stacked" } else { "standard" }
        ),
        G::Pie => format!("<c:pieChart><c:varyColors val=\"1\"/>{sers}{dl}<c:firstSliceAng val=\"0\"/></c:pieChart>"),
        G::Doughnut => {
            format!("<c:doughnutChart><c:varyColors val=\"1\"/>{sers}{dl}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"50\"/></c:doughnutChart>")
        }
        G::Area { stacked } => format!(
            "<c:areaChart><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{sers}{dl}{ax}</c:areaChart>",
            if stacked { "stacked" } else { "standard" }
        ),
        G::Scatter { .. } => format!("<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>{sers}{dl}{ax}</c:scatterChart>"),
        G::Bubble => {
            format!("<c:bubbleChart><c:varyColors val=\"0\"/>{sers}{dl}<c:bubbleScale val=\"100\"/><c:showNegBubbles val=\"0\"/>{ax}</c:bubbleChart>")
        }
        G::Radar => format!("<c:radarChart><c:radarStyle val=\"marker\"/><c:varyColors val=\"0\"/>{sers}{dl}{ax}</c:radarChart>"),
        G::Stock => format!("<c:stockChart>{sers}{dl}<c:hiLowLines/>{ax}</c:stockChart>"),
    }
}

fn title_xml(t: &str) -> String {
    let mut s = String::from("<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/>");
    for line in t.split('\n') {
        let _ = write!(s, "<a:p><a:pPr><a:defRPr/></a:pPr><a:r><a:t>{}</a:t></a:r></a:p>", esc(line));
    }
    s.push_str("</c:rich></c:tx><c:overlay val=\"0\"/></c:title>");
    s
}

struct Ax<'a> {
    cat: bool,
    id: u32,
    cross: u32,
    pos: &'a str,
    delete: bool,
    grid: bool,
    title: Option<&'a str>,
    crosses_max: bool,
    /// Its scale and labels as set (bounds and major unit only on a value axis).
    spec: Option<&'a ChartAxis>,
}

fn axis_xml(a: &Ax<'_>) -> String {
    let spec = a.spec.cloned().unwrap_or_default();
    let finite = |v: Option<f64>| v.filter(|v| v.is_finite());
    let mut bounds = String::new();
    if !a.cat {
        if let Some(v) = finite(spec.max) {
            let _ = write!(bounds, "<c:max val=\"{v}\"/>");
        }
        if let Some(v) = finite(spec.min) {
            let _ = write!(bounds, "<c:min val=\"{v}\"/>");
        }
    }
    let mut s = format!(
        "<c:{0}><c:axId val=\"{1}\"/><c:scaling><c:orientation val=\"minMax\"/>{bounds}</c:scaling><c:delete val=\"{2}\"/><c:axPos val=\"{3}\"/>",
        if a.cat { "catAx" } else { "valAx" },
        a.id,
        (a.delete || spec.hidden) as u8,
        a.pos
    );
    if a.grid {
        s.push_str("<c:majorGridlines/>");
    }
    if let Some(t) = a.title {
        s.push_str(&title_xml(t));
    }
    match spec.number_format.as_deref() {
        Some(code) => {
            let _ = write!(s, "<c:numFmt formatCode=\"{}\" sourceLinked=\"0\"/>", esc(code));
        }
        None => s.push_str("<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>"),
    }
    let _ = write!(
        s,
        "<c:majorTickMark val=\"out\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"{}\"/><c:crosses val=\"{}\"/>",
        a.cross,
        if a.crosses_max { "max" } else { "autoZero" }
    );
    if a.cat {
        s.push_str("<c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>");
    } else {
        s.push_str("<c:crossBetween val=\"between\"/>");
        if let Some(v) = finite(spec.major).filter(|v| *v > 0.0) {
            let _ = write!(s, "<c:majorUnit val=\"{v}\"/>");
        }
        s.push_str("</c:valAx>");
    }
    s
}

/// Chart part XML for a model chart on sheet `si`.
pub fn write_chart(wb: &Workbook, si: usize, ch: &Chart) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:roundedCorners val=\"0\"/>",
    );
    if (1..=48).contains(&ch.style) {
        let _ = write!(s, "<c:style val=\"{}\"/>", ch.style);
    }
    s.push_str("<c:chart>");
    match &ch.title {
        Some(t) if !t.is_empty() => {
            s.push_str(&title_xml(t));
            s.push_str("<c:autoTitleDeleted val=\"0\"/>");
        }
        _ => s.push_str("<c:autoTitleDeleted val=\"1\"/>"),
    }
    s.push_str("<c:plotArea><c:layout/>");
    let all: Vec<(usize, &Series)> = ch.series.iter().enumerate().collect();
    let primary_g;
    let mut secondary: Option<G> = None;
    if ch.kind == ChartKind::Combo {
        // Series without a kind of their own: the first is a column, the others lines (as drawn).
        let kind = |i: usize, x: &Series| x.kind.filter(|k| *k != ChartKind::Combo).or((i > 0).then_some(ChartKind::Line));
        let is_line = |k: Option<ChartKind>| matches!(k.map(group_for), Some(G::Line { .. } | G::Area { .. } | G::Radar | G::Scatter { .. }));
        let bars: Vec<(usize, &Series)> = all.iter().filter(|(i, x)| !x.secondary && !is_line(kind(*i, x))).copied().collect();
        let lines: Vec<(usize, &Series)> = all.iter().filter(|(i, x)| !x.secondary && is_line(kind(*i, x))).copied().collect();
        let sec: Vec<(usize, &Series)> = all.iter().filter(|(_, x)| x.secondary).copied().collect();
        primary_g = G::Bar { bar: false, grouping: "clustered" };
        if !bars.is_empty() || (lines.is_empty() && sec.is_empty()) {
            s.push_str(&group_xml(primary_g, &bars, ch, (1, 2), wb, si));
        }
        if !lines.is_empty() {
            s.push_str(&group_xml(G::Line { stacked: false, markers: false }, &lines, ch, (1, 2), wb, si));
        }
        if !sec.is_empty() {
            let g = match sec.first().and_then(|(_, x)| x.kind).map(group_for) {
                Some(b @ G::Bar { .. }) => b,
                _ => G::Line { stacked: false, markers: false },
            };
            s.push_str(&group_xml(g, &sec, ch, (3, 4), wb, si));
            secondary = Some(g);
        }
    } else {
        primary_g = match group_for(ch.kind) {
            // A stock chart has 3 or 4 series (high-low-close, open-high-low-close).
            G::Stock if !(3..=4).contains(&ch.series.len()) => G::Line { stacked: false, markers: false },
            g => g,
        };
        s.push_str(&group_xml(primary_g, &all, ch, (1, 2), wb, si));
    }
    match primary_g {
        G::Pie | G::Doughnut => {}
        G::Scatter { .. } | G::Bubble => {
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 1,
                cross: 2,
                pos: "b",
                delete: false,
                grid: false,
                title: ch.x_title.as_deref(),
                crosses_max: false,
                spec: Some(&ch.x_axis),
            }));
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 2,
                cross: 1,
                pos: "l",
                delete: false,
                grid: ch.gridlines,
                title: ch.y_title.as_deref(),
                crosses_max: false,
                spec: Some(&ch.y_axis),
            }));
        }
        G::Bar { bar: true, .. } => {
            s.push_str(&axis_xml(&Ax {
                cat: true,
                id: 1,
                cross: 2,
                pos: "l",
                delete: false,
                grid: false,
                title: ch.x_title.as_deref(),
                crosses_max: false,
                spec: Some(&ch.x_axis),
            }));
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 2,
                cross: 1,
                pos: "b",
                delete: false,
                grid: ch.gridlines,
                title: ch.y_title.as_deref(),
                crosses_max: false,
                spec: Some(&ch.y_axis),
            }));
        }
        _ => {
            s.push_str(&axis_xml(&Ax {
                cat: true,
                id: 1,
                cross: 2,
                pos: "b",
                delete: false,
                grid: false,
                title: ch.x_title.as_deref(),
                crosses_max: false,
                spec: Some(&ch.x_axis),
            }));
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 2,
                cross: 1,
                pos: "l",
                delete: false,
                grid: ch.gridlines,
                title: ch.y_title.as_deref(),
                crosses_max: false,
                spec: Some(&ch.y_axis),
            }));
        }
    }
    if secondary.is_some() {
        s.push_str(&axis_xml(&Ax { cat: true, id: 3, cross: 4, pos: "b", delete: true, grid: false, title: None, crosses_max: false, spec: None }));
        s.push_str(&axis_xml(&Ax { cat: false, id: 4, cross: 3, pos: "r", delete: false, grid: false, title: None, crosses_max: true, spec: None }));
    }
    s.push_str("</c:plotArea>");
    let pos = match ch.legend {
        LegendPos::None => None,
        LegendPos::Bottom => Some("b"),
        LegendPos::Top => Some("t"),
        LegendPos::Left => Some("l"),
        LegendPos::Right => Some("r"),
    };
    if let Some(p) = pos {
        let _ = write!(s, "<c:legend><c:legendPos val=\"{p}\"/><c:overlay val=\"0\"/></c:legend>");
    }
    s.push_str("<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>");
    if let Some(source) = &ch.source {
        let _ = write!(
            s,
            "<c:extLst><c:ext uri=\"https://kition.ai/sheet/chart-source\"><k:source xmlns:k=\"https://kition.ai/sheet\" range=\"{}\" byRows=\"{}\"/></c:ext></c:extLst>",
            esc(source),
            u8::from(ch.by_rows)
        );
    }
    s.push_str("</c:chartSpace>");
    s
}
