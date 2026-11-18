//! Sheet charts and the shared chart description (`KitDrawing 1.1`, `kition_office_core::chart`).
//!
//! A sheet's chart shows its cells: it is described with the values its series' ranges hold now,
//! and a change of its type, titles, legend, labels, gridlines or series colours is written back to
//! it. Its categories and values are the cells', so a change to them is refused (DR16): the cells
//! are edited, or the chart given other ranges (Select Data).

use kition_office_core::Xfrm;
use kition_office_core::chart::{AxisSpec, ChartKind as Kind, ChartSpec, LegendPos as Legend, SeriesSpec};
use kition_office_core::model::{self as dm, DrawingObject};
use kition_office_core::ops::DrawingError;
use kition_sheet_core::CellRef;
use kition_sheet_model::{Anchor, Chart, ChartKind, LegendPos, Sheet, Workbook};
use serde_json::json;

use super::drawing::{color_back, color_of};

/// The shared type of a Sheet chart type; the types the description does not edit are named by
/// their DrawingML element (`c:` for the classic ones, the `cx:` layout for the 2016 ones).
pub fn kind_of(k: ChartKind) -> Kind {
    match k {
        ChartKind::ColumnClustered => Kind::Column,
        ChartKind::ColumnStacked => Kind::StackedColumn,
        ChartKind::ColumnStacked100 => Kind::PercentColumn,
        ChartKind::BarClustered => Kind::Bar,
        ChartKind::BarStacked => Kind::StackedBar,
        ChartKind::BarStacked100 => Kind::PercentBar,
        ChartKind::Line => Kind::Line,
        ChartKind::LineMarkers => Kind::LineMarkers,
        ChartKind::LineStacked => Kind::StackedLine,
        ChartKind::Pie => Kind::Pie,
        ChartKind::Doughnut => Kind::Doughnut,
        ChartKind::Area => Kind::Area,
        ChartKind::AreaStacked => Kind::StackedArea,
        ChartKind::Scatter | ChartKind::ScatterLines => Kind::Scatter,
        ChartKind::Radar => Kind::Radar,
        ChartKind::Bubble => Kind::Other("bubbleChart".into()),
        ChartKind::Stock => Kind::Other("stockChart".into()),
        ChartKind::Histogram => Kind::Other("clusteredColumn".into()),
        ChartKind::Waterfall => Kind::Other("waterfall".into()),
        ChartKind::Funnel => Kind::Other("funnel".into()),
        ChartKind::Treemap => Kind::Other("treemap".into()),
        ChartKind::Sunburst => Kind::Other("sunburst".into()),
        ChartKind::BoxWhisker => Kind::Other("boxWhisker".into()),
        // Described by its first series' type, with each series' own.
        ChartKind::Combo => Kind::Column,
    }
}

/// The Sheet type of a shared type the description edits.
pub(crate) fn type_of(k: &Kind) -> Option<ChartKind> {
    Some(match k {
        Kind::Column => ChartKind::ColumnClustered,
        Kind::StackedColumn => ChartKind::ColumnStacked,
        Kind::PercentColumn => ChartKind::ColumnStacked100,
        Kind::Bar => ChartKind::BarClustered,
        Kind::StackedBar => ChartKind::BarStacked,
        Kind::PercentBar => ChartKind::BarStacked100,
        Kind::Line => ChartKind::Line,
        Kind::LineMarkers => ChartKind::LineMarkers,
        Kind::StackedLine => ChartKind::LineStacked,
        Kind::Area => ChartKind::Area,
        Kind::StackedArea => ChartKind::AreaStacked,
        Kind::Pie => ChartKind::Pie,
        Kind::Doughnut => ChartKind::Doughnut,
        Kind::Scatter => ChartKind::Scatter,
        Kind::Radar => ChartKind::Radar,
        Kind::Other(_) => return None,
    })
}

/// `insert.chart`'s `type` and `subtype` for a shared type.
pub fn insert_type(k: &Kind) -> Option<(&'static str, &'static str)> {
    Some(match k {
        Kind::Column => ("column", "clustered"),
        Kind::StackedColumn => ("column", "stacked"),
        Kind::PercentColumn => ("column", "stacked100"),
        Kind::Bar => ("bar", "clustered"),
        Kind::StackedBar => ("bar", "stacked"),
        Kind::PercentBar => ("bar", "stacked100"),
        Kind::Line => ("line", ""),
        Kind::LineMarkers => ("line", "markers"),
        Kind::StackedLine => ("line", "stacked"),
        Kind::Area => ("area", ""),
        Kind::StackedArea => ("area", "stacked"),
        Kind::Pie => ("pie", ""),
        Kind::Doughnut => ("doughnut", ""),
        Kind::Scatter => ("scatter", ""),
        Kind::Radar => ("radar", ""),
        Kind::Other(_) => return None,
    })
}

fn legend_of(l: LegendPos) -> Option<Legend> {
    match l {
        LegendPos::None => None,
        LegendPos::Bottom => Some(Legend::Bottom),
        LegendPos::Top => Some(Legend::Top),
        LegendPos::Left => Some(Legend::Left),
        LegendPos::Right => Some(Legend::Right),
    }
}

fn legend_back(l: Option<Legend>) -> LegendPos {
    match l {
        None => LegendPos::None,
        Some(Legend::Top) => LegendPos::Top,
        Some(Legend::Left) => LegendPos::Left,
        Some(Legend::Right | Legend::TopRight) => LegendPos::Right,
        Some(Legend::Bottom) => LegendPos::Bottom,
    }
}

fn number_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { v.to_string() }
}

/// Chart `c` of sheet `si`, described with what its ranges hold now.
pub fn spec_of(wb: &Workbook, si: usize, c: &Chart) -> ChartSpec {
    let data = kition_sheet_chart::resolve(wb, si, c);
    let main = match c.kind {
        ChartKind::Combo => c.series.first().and_then(|s| s.kind).unwrap_or(ChartKind::ColumnClustered),
        k => k,
    };
    let kind = kind_of(main);
    let mut categories = data.categories.clone();
    if kind == Kind::Scatter
        && let Some(x) = data.series.first().and_then(|s| s.x.as_ref())
    {
        categories = x.iter().map(|v| v.map(number_text).unwrap_or_default()).collect();
    }
    let n = categories.len().max(data.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
    categories.extend((categories.len()..n).map(|i| (i + 1).to_string()));
    ChartSpec {
        kind: kind.clone(),
        title: c.title.clone(),
        series: data
            .series
            .iter()
            .zip(&c.series)
            .map(|(d, s)| {
                let mut values = d.values.clone();
                values.resize(n, None);
                SeriesSpec {
                    name: d.name.clone(),
                    values,
                    color: s.color.map(|c| color_of(c, None)),
                    kind: s.kind.filter(|k| c.kind == ChartKind::Combo && kind_of(*k) != kind).map(kind_of),
                    smooth: s.smooth,
                    trendline: s.trendline.as_ref().map(|t| kition_office_core::chart::Trendline {
                        kind: t.kind.clone(),
                        order: t.order,
                        period: t.period,
                        forward: t.forward,
                        backward: t.backward,
                    }),
                    error_bars: s.error_bars.as_ref().map(|e| kition_office_core::chart::ErrorBars { kind: e.kind.clone(), value: e.value }),
                    secondary: s.secondary,
                    // In hundredths of a point (no f32 noise).
                    line_width: s.line_width.map(|w| (f64::from(w) * 100.0).round() / 100.0),
                    marker: s.marker.clone(),
                }
            })
            .collect(),
        categories,
        legend: legend_of(c.legend),
        data_labels: c.data_labels,
        gridlines: c.gridlines,
        x_title: c.x_title.clone(),
        y_title: c.y_title.clone(),
        x_axis: axis_spec(&c.x_axis),
        y_axis: axis_spec(&c.y_axis),
        data_label_position: c.label_position.clone(),
    }
}

/// An axis's settings, described.
fn axis_spec(a: &kition_sheet_model::ChartAxis) -> AxisSpec {
    AxisSpec { min: a.min, max: a.max, major: a.major, number_format: a.number_format.clone(), hidden: a.hidden }
}

/// An axis's settings from its description.
fn axis_back(a: &AxisSpec) -> kition_sheet_model::ChartAxis {
    kition_sheet_model::ChartAxis { min: a.min, max: a.max, major: a.major, number_format: a.number_format.clone(), hidden: a.hidden }
}

/// Where a type's data labels may sit, as Office offers them (and as Excel takes them in a file).
fn label_positions(kind: &Kind) -> &'static [&'static str] {
    match kind {
        Kind::Pie => &["bestFit", "ctr", "inEnd", "outEnd"],
        Kind::Column | Kind::Bar => &["ctr", "inBase", "inEnd", "outEnd"],
        Kind::StackedColumn | Kind::PercentColumn | Kind::StackedBar | Kind::PercentBar => &["ctr", "inBase", "inEnd"],
        Kind::Line | Kind::LineMarkers | Kind::StackedLine | Kind::Scatter => &["ctr", "l", "r", "t", "b"],
        _ => &[],
    }
}

/// Chart `c` of sheet `si`, described: its frame in sheet points from its cell anchor.
pub fn to_drawing(wb: &Workbook, si: usize, sheet: &Sheet, c: &Chart) -> DrawingObject {
    let a = &c.anchor;
    let frame = Xfrm::new(
        sheet.col_left(a.cell.col) + f64::from(a.dx),
        sheet.row_top(a.cell.row) + f64::from(a.dy),
        f64::from(a.width.max(0.0)),
        f64::from(a.height.max(0.0)),
    );
    let mut placement = dm::Placement { kind: dm::PlacementKind::Cell, ..Default::default() };
    placement.extra.insert("mode".into(), json!(super::drawing::mode_name(a.mode)));
    placement.extra.insert("cell".into(), json!(a.cell.to_string()));
    DrawingObject { id: c.id.to_string(), kind: dm::ObjectKind::Chart, frame, placement, chart: Some(spec_of(wb, si, c)), ..Default::default() }
}

/// Writes what differs between `before` and `after` into chart `c`.
pub fn write_back(sheet: &Sheet, c: &mut Chart, before: &DrawingObject, after: &DrawingObject) -> Result<(), DrawingError> {
    if (after.frame.rot, after.frame.flip_h, after.frame.flip_v) != (before.frame.rot, before.frame.flip_h, before.frame.flip_v) {
        return Err(DrawingError::unsupported("frame.rot", "a chart does not turn"));
    }
    for (property, changed) in [
        ("fill", after.fill != before.fill),
        ("stroke", after.stroke != before.stroke),
        ("geometry", after.geometry != before.geometry),
        ("text", after.text != before.text),
    ] {
        if changed {
            return Err(DrawingError::unsupported(property, "a chart's look is changed with drawing.chart"));
        }
    }
    if after.frame != before.frame {
        let f = &after.frame;
        kition_office_core::ops::check_frame(f)?;
        let (x, y) = (f.x.max(0.0), f.y.max(0.0));
        let cell = CellRef::new(sheet.row_at(y), sheet.col_at(x));
        c.anchor = Anchor {
            cell,
            dx: (x - sheet.col_left(cell.col)) as f32,
            dy: (y - sheet.row_top(cell.row)) as f32,
            width: (f.w as f32).clamp(20.0, 10_000.0),
            height: (f.h as f32).clamp(20.0, 10_000.0),
            ..c.anchor
        };
    }
    if after.placement != before.placement
        && let Some(mode) = after.placement.extra.get("mode").and_then(serde_json::Value::as_str)
    {
        c.anchor.mode = super::drawing::anchor_mode(mode).ok_or_else(|| DrawingError::invalid("mode is moveAndSize, moveOnly or absolute"))?;
    }
    let (Some(was), Some(now)) = (&before.chart, &after.chart) else { return Ok(()) };
    if now == was {
        return Ok(());
    }
    if now.categories != was.categories {
        return Err(DrawingError::unsupported("chart.categories", "a chart shows the sheet's cells: edit the cells, or give it other ranges"));
    }
    if now.series.len() != was.series.len() || now.series.iter().zip(&was.series).any(|(a, b)| a.name != b.name || a.values != b.values) {
        return Err(DrawingError::unsupported("chart.series", "a chart shows the sheet's cells: edit the cells, or give it other ranges"));
    }
    if now.kind != was.kind {
        if matches!(was.kind, Kind::Other(_)) {
            return Err(DrawingError::unsupported("chart.kind", "this chart keeps its type"));
        }
        let k = type_of(&now.kind).ok_or_else(|| DrawingError::unsupported("chart.kind", "this type is not one a chart changes to"))?;
        if now.kind.is_pie() && c.series.len() > 1 {
            return Err(DrawingError::invalid("a pie or doughnut chart shows one series: give it one range"));
        }
        c.kind = k;
        // As Insert › Chart sets them up for the type.
        if now.kind.is_pie() {
            c.gridlines = false;
        }
    }
    // Axes: a category axis has no bounds (a scatter chart's X axis has).
    if now.x_axis != was.x_axis {
        let x = &now.x_axis;
        if now.kind != Kind::Scatter && (x.min.is_some() || x.max.is_some() || x.major.is_some()) {
            return Err(DrawingError::unsupported("chart.xAxis", "a category axis has no bounds or unit: set its number format or hide it"));
        }
        c.x_axis = axis_back(x);
    }
    if now.y_axis != was.y_axis {
        c.y_axis = axis_back(&now.y_axis);
    }
    if now.data_label_position != was.data_label_position || now.kind != was.kind {
        match now.data_label_position.as_deref() {
            Some(p) if !label_positions(&now.kind).contains(&p) => {
                if now.data_label_position != was.data_label_position {
                    return Err(DrawingError::invalid(format!("this chart type's data labels do not sit at `{p}`")));
                }
                // A new type that does not take the position goes back to its own.
                c.label_position = None;
            }
            p => c.label_position = p.map(str::to_string),
        }
    }
    // A series' own type: a combination (each series keeps its type), or one type again.
    if now.kind != was.kind || now.series.iter().zip(&was.series).any(|(a, b)| a.kind != b.kind) {
        let own = |s: &kition_office_core::chart::SeriesSpec| s.kind.clone().unwrap_or_else(|| now.kind.clone());
        let combo = now.series.iter().any(|s| own(s) != now.kind);
        if combo {
            let mut kinds = Vec::with_capacity(now.series.len());
            for s in &now.series {
                let k = own(s);
                if k.is_pie() || k == Kind::Radar {
                    return Err(DrawingError::unsupported("chart.series.kind", "a combination mixes column, bar, line, area and scatter series"));
                }
                kinds.push(type_of(&k).ok_or_else(|| DrawingError::unsupported("chart.series.kind", "this type is not one a series changes to"))?);
            }
            c.kind = ChartKind::Combo;
            for (s, k) in c.series.iter_mut().zip(kinds) {
                s.kind = Some(k);
            }
        } else if c.kind == ChartKind::Combo {
            c.kind = type_of(&now.kind).ok_or_else(|| DrawingError::unsupported("chart.kind", "this type is not one a chart changes to"))?;
            for s in &mut c.series {
                s.kind = None;
            }
        }
    }
    for (s, (a, b)) in c.series.iter_mut().zip(now.series.iter().zip(&was.series)) {
        s.smooth = a.smooth;
        if a.line_width != b.line_width {
            s.line_width = a.line_width.map(|w| w as f32);
        }
        if a.marker != b.marker {
            s.marker.clone_from(&a.marker);
        }
        if a.secondary != b.secondary {
            s.secondary = a.secondary;
        }
        if a.trendline != b.trendline {
            s.trendline = a.trendline.as_ref().map(|t| kition_sheet_model::SeriesTrend {
                kind: t.kind.clone(),
                order: t.order,
                period: t.period,
                forward: t.forward,
                backward: t.backward,
            });
        }
        if a.error_bars != b.error_bars {
            s.error_bars = a.error_bars.as_ref().map(|e| kition_sheet_model::SeriesErrors { kind: e.kind.clone(), value: e.value });
        }
        if a.color != b.color {
            s.color = a.color.as_ref().map(|col| color_back(col).map(|(c, _)| c)).transpose()?;
        }
    }
    c.title.clone_from(&now.title);
    c.legend = legend_back(now.legend);
    c.data_labels = now.data_labels;
    if now.gridlines != was.gridlines {
        c.gridlines = now.gridlines;
    }
    c.x_title.clone_from(&now.x_title);
    c.y_title.clone_from(&now.y_title);
    Ok(())
}
