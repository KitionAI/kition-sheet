# Chart parity

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4

Excel's chart types and chart elements against Kition Sheet. Part of
[target-app-parity.md](target-app-parity.md). **Breadth ~50%, ready ~30%, 40–60 h** (estimated).

Sources: `crates/engine/model/src/features.rs` (`ChartKind`, `Chart`, `Series`), `crates/engine/chart/src`
(layout and rendering), `crates/formats/xlsx/src/chart.rs` (DrawingML chart parts) and
`crates/formats/xlsx/src/chartex.rs` (chartex parts).

## Chart types

| Excel type | Excel subtypes | Kition Sheet | Notes |
|---|---|---|---|
| Column | clustered, stacked, 100% stacked, 3-D clustered/stacked/100%, 3-D column | 3 of 7 | no 3-D |
| Bar | clustered, stacked, 100%, 3-D ×3 | 3 of 6 | no 3-D |
| Line | line, stacked, 100% stacked, with markers ×3, 3-D line | 3 of 7 (line, with markers, stacked) | no 100% stacked; smoothed lines per series |
| Pie | pie, 3-D pie, pie of pie, bar of pie, doughnut | 2 of 5 (pie, doughnut) | no exploded, pie of pie, bar of pie |
| Area | area, stacked, 100%, 3-D ×3 | 2 of 6 | no 100%, no 3-D |
| X Y (scatter) | markers, smooth lines (±markers), straight lines (±markers) | 2 of 5 | smoothed lines per series |
| Bubble | bubble, 3-D bubble | 1 of 2 | |
| Stock | high-low-close, open-high-low-close, volume-HLC, volume-OHLC | 1 kind | |
| Surface | 3-D surface, wireframe, contour, wireframe contour | 0 of 4 | |
| Radar | radar, with markers, filled | 1 of 3 | |
| Treemap, Sunburst | 1 each | 2 of 2 | written as chartex parts |
| Histogram, Pareto | 2 | 1 of 2 | no Pareto |
| Box & Whisker | 1 | 1 of 1 | written as a chartex part |
| Waterfall, Funnel | 2 | 2 of 2 | written as chartex parts; waterfall totals kept |
| Filled map | 1 | 0 | needs geographic boundary data (licensing decision) |
| Combo | custom combinations, secondary axis | yes (line on secondary axis) | |
| **Total** | ~60 subtypes | **25 (~42%)** | usage-weighted breadth higher (~65%): column, bar, line, pie and scatter dominate |

## Chart elements and formatting

| Element | Excel | Kition Sheet |
|---|---|---|
| Move and resize the chart object | yes | yes (the drawing frame) |
| Chart title, axis titles | text, linked to cells, full formatting | text only |
| Legend | position, overlay, formatting, per-entry delete | position only |
| Data labels | value, category, series, percentage, from cells, callouts, position | on/off and position |
| Gridlines | major/minor per axis, formatting | one on/off flag |
| Axis options | min/max/units, log scale, reverse order, crossing point, display units, date axis, number format | min, max, major unit, number format, hidden; no log scale, reverse order, crossing point, display units or date axis |
| Secondary axis | any series | line series in combo charts |
| Trendlines | linear, exponential, logarithmic, polynomial, power, moving average, equation and R² | the six types, with forecast forward and backward; no equation or R² |
| Error bars | fixed, %, standard deviation/error, custom | fixed, %, standard deviation and error, both ways; no custom |
| Up/down bars, high-low lines, drop lines, series lines | yes | **missing** |
| Data table under the chart | yes | **missing** |
| Per-series / per-point fill, line, marker, effects | Format pane for every element | series colour, line width and marker, pie slice colours; Format Chart pane covers chart-level options |
| Chart styles and colour palettes | ~16 styles × colour sets | style number stored, own palettes |
| Chart sheets | yes | **missing** (skipped with a warning on open) |
| Chart templates (`.crtx`) | yes | **missing** |
| Recommended Charts | yes | partial (Insert ▸ Recommended) |
| Switch Row/Column, Select Data | yes | yes, also on charts read from files |
| Sparklines (line, column, win/loss, markers, axis) | yes | line, column, win/loss |
| Pivot charts | yes | **missing** |
| Animation on data change | yes (Windows) | no |

## XLSX round-trip

Charts are read from and written to DrawingML chart parts, but only the fields the model holds
survive: anything else (log scales, display units, trendline equations, most per-point and
per-element formats) is lost when Kition Sheet saves a workbook it opened. That is the main reason charts are
**~30% ready**: a user who opens an Excel workbook with charts and saves it gets simpler charts
back.

## Work (40–60 h)

1. Preserve unmodelled chart XML on round-trip (keep the original part, patch only edited fields): 6–10 h.
2. The remaining axis options, label options, gridlines per axis, legend formatting: 8–12 h.
3. Trendline equations and R², custom error bars: 2–4 h.
4. Per-element Format pane parity, exploded pie, pie/bar of pie, 100% variants: 8–12 h.
5. 3-D column/bar/pie/area, surface and contour, Pareto: 8–12 h.
6. Chart sheets, pivot charts, templates: 6–10 h. Filled map: maintainer decision on boundary data.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
