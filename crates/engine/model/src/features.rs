//! Sheet features beyond cells: tables, conditional formatting, data validation, comments,
//! hyperlinks, charts, images, autofilter, print settings.

use kition_sheet_core::{CellRef, RangeRef};
use serde::{Deserialize, Serialize};

use crate::style::{Color, Style};

// ---------------------------------------------------------------- tables

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TotalsFn {
    #[default]
    None,
    Average,
    Count,
    CountNums,
    Max,
    Min,
    Sum,
    StdDev,
    Var,
    Custom,
}

impl TotalsFn {
    /// SUBTOTAL function number.
    pub fn subtotal_code(&self) -> Option<u32> {
        Some(match self {
            TotalsFn::Average => 101,
            TotalsFn::Count => 103,
            TotalsFn::CountNums => 102,
            TotalsFn::Max => 104,
            TotalsFn::Min => 105,
            TotalsFn::Sum => 109,
            TotalsFn::StdDev => 107,
            TotalsFn::Var => 110,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    pub name: String,
    pub totals: TotalsFn,
    /// Totals row label (first column) or custom formula.
    pub totals_label: Option<String>,
    /// Calculated column formula.
    pub formula: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub id: u32,
    pub name: String,
    /// Whole table including header and totals rows.
    pub range: RangeRef,
    pub header_row: bool,
    pub totals_row: bool,
    pub columns: Vec<TableColumn>,
    pub style: String,
    pub banded_rows: bool,
    pub banded_cols: bool,
    pub first_col: bool,
    pub last_col: bool,
    pub filter_button: bool,
    /// The table's own filter (its header's buttons): column offset within `range` → criterion.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub criteria: Vec<(u32, FilterCriterion)>,
}

impl Table {
    pub fn data_range(&self) -> Option<RangeRef> {
        let r0 = self.range.start.row + self.header_row as u32;
        let r1 = self.range.end.row.checked_sub(self.totals_row as u32)?;
        if r1 < r0 {
            return None;
        }
        Some(RangeRef::new(CellRef::new(r0, self.range.start.col), CellRef::new(r1, self.range.end.col)))
    }
    pub fn header_range(&self) -> Option<RangeRef> {
        self.header_row.then(|| RangeRef::new(self.range.start, CellRef::new(self.range.start.row, self.range.end.col)))
    }
    pub fn totals_range(&self) -> Option<RangeRef> {
        self.totals_row.then(|| RangeRef::new(CellRef::new(self.range.end.row, self.range.start.col), self.range.end))
    }
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }
}

// ---------------------------------------------------------------- conditional formatting

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CfOperator {
    Between,
    NotBetween,
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CfValueKind {
    Min,
    Max,
    Number(f64),
    Percent(f64),
    Percentile(f64),
    Formula(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CfRule {
    /// Cell value compared with one or two formulas.
    CellIs {
        op: CfOperator,
        a: String,
        b: Option<String>,
        style: Box<Style>,
    },
    /// Custom formula (relative to the top-left cell of the range).
    Expression {
        formula: String,
        style: Box<Style>,
    },
    ContainsText {
        text: String,
        style: Box<Style>,
    },
    NotContainsText {
        text: String,
        style: Box<Style>,
    },
    BeginsWith {
        text: String,
        style: Box<Style>,
    },
    EndsWith {
        text: String,
        style: Box<Style>,
    },
    Blanks {
        style: Box<Style>,
    },
    NoBlanks {
        style: Box<Style>,
    },
    Errors {
        style: Box<Style>,
    },
    NoErrors {
        style: Box<Style>,
    },
    Duplicate {
        style: Box<Style>,
    },
    Unique {
        style: Box<Style>,
    },
    /// Top/bottom N or N percent.
    Top10 {
        bottom: bool,
        percent: bool,
        rank: u32,
        style: Box<Style>,
    },
    AboveAverage {
        below: bool,
        equal: bool,
        std_dev: u8,
        style: Box<Style>,
    },
    /// `yesterday`, `today`, `tomorrow`, `last7Days`, `lastWeek`, `thisWeek`, `nextWeek`, `lastMonth`, `thisMonth`, `nextMonth`.
    TimePeriod {
        period: String,
        style: Box<Style>,
    },
    ColorScale {
        stops: Vec<(CfValueKind, Color)>,
    },
    DataBar {
        min: CfValueKind,
        max: CfValueKind,
        color: Color,
        gradient: bool,
        show_value: bool,
    },
    /// Icon set name (`3Arrows`, `3TrafficLights1`, `4Rating`, `5Quarters`…).
    IconSet {
        set: String,
        thresholds: Vec<CfValueKind>,
        reverse: bool,
        show_value: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CondFormat {
    pub ranges: Vec<RangeRef>,
    pub rule: CfRule,
    pub priority: u32,
    pub stop_if_true: bool,
}

// ---------------------------------------------------------------- data validation

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationKind {
    #[default]
    Any,
    Whole,
    Decimal,
    List,
    Date,
    Time,
    TextLength,
    Custom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorStyle {
    #[default]
    Stop,
    Warning,
    Information,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Validation {
    pub ranges: Vec<RangeRef>,
    pub kind: ValidationKind,
    pub op: CfOperator,
    /// Formula 1 (list source: `"a,b,c"` literal list or `=$A$1:$A$5`).
    pub f1: String,
    pub f2: Option<String>,
    pub allow_blank: bool,
    pub in_cell_dropdown: bool,
    pub input_title: String,
    pub input_message: String,
    pub show_input: bool,
    pub error_title: String,
    pub error_message: String,
    pub error_style: ErrorStyle,
    pub show_error: bool,
}

impl Default for Validation {
    fn default() -> Self {
        Validation {
            ranges: vec![],
            kind: ValidationKind::Any,
            op: CfOperator::Between,
            f1: String::new(),
            f2: None,
            allow_blank: true,
            in_cell_dropdown: true,
            input_title: String::new(),
            input_message: String::new(),
            show_input: true,
            error_title: String::new(),
            error_message: String::new(),
            error_style: ErrorStyle::Stop,
            show_error: true,
        }
    }
}

// ---------------------------------------------------------------- form controls

/// A form control (Excel's and WPS's form controls: check boxes, option buttons, buttons, list
/// and combo boxes, spin buttons, scroll bars, labels, group boxes), kept as the file had it so
/// that saving gives it back, and drawn where it sits. Kition Sheet does not run or edit them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FormControl {
    /// Its legacy drawing (VML) shape and the shape type that shape uses, verbatim.
    pub vml: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shape_type: String,
    /// The control element and its properties part (`formControlPr`), verbatim, when the file
    /// has them (Excel 2010 and later write both).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub props: Option<String>,
    /// The shape's id (the number in `_x0000_s1025`).
    pub shape_id: u32,
    /// `Checkbox`, `Button`, `Radio`, `Drop`, `List`, `Spin`, `Scroll`, `Label`, `GBox`, `Edit`…
    pub kind: String,
    /// Where it sits: the cells its corners are in and the corners' offsets in them (px).
    pub from: (CellRef, f32, f32),
    pub to: (CellRef, f32, f32),
    pub checked: bool,
    /// Its caption (a check box's or a button's text).
    pub text: String,
}

/// A smart graphic, kept as the file had it so that saving gives it back, and drawn from the
/// ready-made drawing Office keeps with it (`kition_office_core::smartart`). Kition Sheet moves and
/// sizes it but does not edit it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Diagram {
    pub id: u32,
    pub anchor: Anchor,
    /// The frame's name (`cNvPr@name`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The diagram's parts: its data model, layout, quick style and colours, and the drawing.
    pub parts: Vec<DiagramPart>,
    /// The drawing's own parts (its pictures), by the relationship ids the drawing uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawing_parts: Vec<DiagramPart>,
}

/// One part of a [`Diagram`], verbatim.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiagramPart {
    /// Which part: `data`, `layout`, `quickStyle`, `colors` or `drawing`; for a drawing's own
    /// part, the relationship id the drawing names it by.
    pub role: String,
    /// The relationship type (a URI) and the part's content type.
    pub rel_type: String,
    pub content_type: String,
    /// The part's file name extension (`xml`, `png`…).
    pub ext: String,
    #[serde(with = "crate::b64")]
    pub data: Vec<u8>,
}

impl Diagram {
    /// The part playing `role`.
    pub fn part(&self, role: &str) -> Option<&DiagramPart> {
        self.parts.iter().find(|p| p.role == role)
    }
}

// ---------------------------------------------------------------- comments, links

/// A note (classic comment) or a threaded comment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub text: String,
    /// Replies of a threaded comment (author, text).
    pub replies: Vec<(String, String)>,
    pub threaded: bool,
    pub resolved: bool,
    pub visible: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hyperlink {
    /// URL, `mailto:`, or an in-workbook location like `Sheet2!A1`.
    pub target: String,
    pub tooltip: Option<String>,
    /// The link's text as the file gives it (`display`); `None`: the cell's text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// `target` is a file or address outside the workbook although it does not look like one (a
    /// relative path such as `docs/report.xlsx`, read from the file's relationships).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
}

impl Hyperlink {
    /// A link to `target` with a tooltip.
    pub fn new(target: String, tooltip: Option<String>) -> Hyperlink {
        Hyperlink { target, tooltip, ..Default::default() }
    }

    /// Whether `target` names something outside the workbook (written as a relationship).
    pub fn is_external(&self) -> bool {
        Self::looks_external(&self.target) || self.external
    }

    /// Whether a target looks like an address outside the workbook: a URL, `mailto:`, a UNC or
    /// `file:` path.
    pub fn looks_external(target: &str) -> bool {
        let t = target.trim();
        let lower = t.to_ascii_lowercase();
        t.contains("://") || lower.starts_with("mailto:") || t.starts_with("\\\\") || lower.starts_with("file:")
    }
}

// ---------------------------------------------------------------- drawing objects

/// How a floating object is pinned to the grid when its cells move or resize. Mirrors Excel's
/// Format ▸ Size and Properties ▸ Properties, stored in the file as the anchor's `editAs`:
/// `twoCell` (absent `editAs`), `oneCell` and `absolute`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnchorMode {
    /// Move and size with cells (Excel's default; `<xdr:twoCellAnchor>`).
    #[default]
    MoveAndSize,
    /// Move but don't size with cells (`<xdr:twoCellAnchor editAs="oneCell">` / `<xdr:oneCellAnchor>`).
    MoveOnly,
    /// Don't move or size with cells (`editAs="absolute"` / `<xdr:absoluteAnchor>`).
    Absolute,
}

/// Position of a floating object: top-left cell plus offset, size in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub cell: CellRef,
    pub dx: f32,
    pub dy: f32,
    pub width: f32,
    pub height: f32,
    /// How the object follows its anchor cell when rows/columns move (sort, insert, delete).
    #[serde(default)]
    pub mode: AnchorMode,
    /// Written as a one-cell anchor (`xdr:oneCellAnchor`: the first cell and the size) while the
    /// mode is "Move but don't size"; a two-cell anchor otherwise. Additive.
    #[serde(default, skip_serializing_if = "is_false")]
    pub one_cell: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChartKind {
    #[default]
    ColumnClustered,
    ColumnStacked,
    ColumnStacked100,
    BarClustered,
    BarStacked,
    BarStacked100,
    Line,
    LineMarkers,
    LineStacked,
    Pie,
    Doughnut,
    Area,
    AreaStacked,
    Scatter,
    ScatterLines,
    Bubble,
    Radar,
    Histogram,
    Waterfall,
    Funnel,
    Treemap,
    Sunburst,
    BoxWhisker,
    Stock,
    Combo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegendPos {
    None,
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Series {
    /// Formula text for the series name (`Sheet1!$B$1`) or a literal.
    pub name: Option<String>,
    pub categories: Option<String>,
    pub values: String,
    /// X values for scatter / bubble sizes.
    pub bubble_sizes: Option<String>,
    pub color: Option<Color>,
    /// Combo charts: this series drawn as a line on the secondary axis.
    pub secondary: bool,
    pub kind: Option<ChartKind>,
    /// Draw line segments as a smoothed (spline) curve.
    #[serde(default)]
    pub smooth: bool,
    /// Its trendline (`c:trendline`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trendline: Option<SeriesTrend>,
    /// Its error bars on its values (`c:errBars`), both ways.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_bars: Option<SeriesErrors>,
    /// Its line's (or its bars' outline's) width in points (`c:spPr/a:ln@w`); `None`: the
    /// chart's style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<f32>,
    /// Its marker symbol (`c:marker/c:symbol`: `none`, `circle`, `square`…); `None`: the type's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
}

/// An axis's scale and labels as a file or a person set them (`c:scaling` `c:min` and `c:max`,
/// `c:majorUnit`, `c:numFmt`, `c:delete`); members left `None` are automatic.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartAxis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The distance between major tick marks and gridlines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major: Option<f64>,
    /// The labels' number format; `None`: the data's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_format: Option<String>,
    /// The axis (its line and labels) is not shown.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

impl ChartAxis {
    pub fn is_default(&self) -> bool {
        *self == ChartAxis::default()
    }
}

/// A series' trendline: its type (`linear`, `exp`, `log`, `poly`, `power`, `movingAvg`), a
/// polynomial's order, a moving average's period, and how far it runs past the data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeriesTrend {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backward: Option<f64>,
}

/// A series' error bars: the amount's type (`fixedVal`, `percentage`, `stdDev`, `stdErr`) and
/// value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeriesErrors {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    pub id: u32,
    pub kind: ChartKind,
    pub anchor: Anchor,
    pub title: Option<String>,
    pub series: Vec<Series>,
    pub legend: LegendPos,
    pub data_labels: bool,
    pub gridlines: bool,
    pub style: u32,
    pub x_title: Option<String>,
    pub y_title: Option<String>,
    /// Source range when created from a selection (for Switch Row/Column).
    pub source: Option<String>,
    pub by_rows: bool,
    /// The category (or a scatter chart's X) axis.
    #[serde(default, skip_serializing_if = "ChartAxis::is_default")]
    pub x_axis: ChartAxis,
    /// The (primary) value axis.
    #[serde(default, skip_serializing_if = "ChartAxis::is_default")]
    pub y_axis: ChartAxis,
    /// Where data labels sit (`c:dLblPos`: `outEnd`, `ctr`, `t`…); `None`: the type's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_position: Option<String>,
    /// A waterfall's points set as totals (`cx:subtotals`), from 0: their bars stand on the
    /// axis and show the point's own value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub totals: Vec<u32>,
    /// A pie's or doughnut's slice colours (`c:dPt`), in point order; empty: the theme's accents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub point_colors: Vec<Color>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub id: u32,
    pub anchor: Anchor,
    /// Encoded bytes (PNG/JPEG), base64 in JSON.
    #[serde(with = "crate::b64")]
    pub data: Vec<u8>,
    pub mime: String,
    pub alt: String,
    /// The picture's extensions as written (`a:blip/a:extLst` items, such as Office's hint not to
    /// compress it), written back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blip_ext: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    #[default]
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Triangle,
    Line,
    Arrow,
    TextBox,
    /// An icon from Kition Sheet's own icon set; `Shape::text` holds the icon name.
    Icon,
    /// A freehand ink stroke; `Shape::text` holds the points as `x,y;x,y;…` relative to the
    /// anchor (sheet points), `Shape::line` the ink colour and `fill` unused.
    Ink,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub id: u32,
    pub kind: ShapeKind,
    pub anchor: Anchor,
    pub fill: Color,
    pub line: Color,
    pub text: String,
    // Additive fields (a reader that does not know them reads the shape without them).
    /// The groups (`xdr:grpSp`) the shape is a member of, outermost first: each a key the
    /// members of one group share on their sheet. A member keeps its own place (`anchor`) on the
    /// sheet; the group is written around its members.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<u32>,
    /// The preset outline (`a:prstGeom@prst`) when `kind` does not name it (a star, a chevron…),
    /// with its adjust values (`a:avLst`), written back as it was; drawn as `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<PresetGeom>,
    /// Custom geometry (`a:custGeom`), drawn instead of `kind`'s outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geom: Option<CustomGeom>,
    /// Rotation clockwise in 60,000ths of a degree (`a:xfrm@rot`) and mirroring (`flipH`,
    /// `flipV`) about the shape's centre.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub rot: i32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_h: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_v: bool,
    /// Opacity of the solid fill in 1,000ths of a percent (`a:alpha`: 100000 is opaque), as the
    /// file gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_alpha: Option<u32>,
    /// A gradient fill (`a:gradFill`), drawn instead of `fill` (which holds its first colour for
    /// readers that do not know gradients).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    /// The outline's pen (`a:ln`): width, dash, cap, join, opacity; its colour is `line`.
    #[serde(default, skip_serializing_if = "Outline::is_empty")]
    pub outline: Outline,
    /// Shadow and glow (`a:effectLst`: `a:outerShdw`, `a:glow`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<ShapeEffects>,
    /// A connector's glued ends (`xdr:cxnSp`: `a:stCxn`, `a:endCxn`): the shapes' ids and their
    /// connection sites; the connector follows them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ShapeConnection>,
    /// The text body as the file wrote it (`xdr:txBody`: runs' fonts, sizes and colours,
    /// paragraph alignment and spacing, insets), written back while `text` is still its text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_body: Option<String>,
    /// The shape's style references (`xdr:style`: theme line, fill, effect and font), as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_refs: Option<String>,
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// A preset outline by name, with its adjust values (`a:gd` name and formula).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetGeom {
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub adj: Vec<(String, String)>,
}

/// How a custom sub-path is filled (`a:path@fill`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathFill {
    #[default]
    Norm,
    None,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

/// One sub-path of a custom geometry, in its own coordinate space `w` × `h` stretched over the
/// shape: `M x y`, `L x y`, `C x1 y1 x2 y2 x y`, `Q x1 y1 x y`, `Z`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomPath {
    pub w: f64,
    pub h: f64,
    pub d: String,
    pub fill: PathFill,
    pub stroke: bool,
}

impl Default for CustomPath {
    fn default() -> Self {
        CustomPath { w: 0.0, h: 0.0, d: String::new(), fill: PathFill::Norm, stroke: true }
    }
}

/// Custom geometry: its paths as evaluated, and the file's `a:custGeom` element (guides, adjust
/// handles, connection sites, text box), written back as it was while `paths` are still what it
/// evaluates to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomGeom {
    pub paths: Vec<CustomPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// One colour stop of a gradient: position and opacity in 1,000ths of a percent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientStop {
    pub pos: u32,
    pub color: Color,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<u32>,
}

/// A gradient fill (`a:gradFill`): its stops, and either a linear direction (`a:lin@ang`, 60,000ths
/// of a degree clockwise from left to right) or a path shape (`circle`, `rect`, `shape`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gradient {
    pub stops: Vec<GradientStop>,
    pub angle: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rot_with_shape: Option<bool>,
}

/// An outline's pen beyond its colour (`a:ln`).
/// A connector's glued ends.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeConnection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<ConnectionEnd>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<ConnectionEnd>,
}

/// One glued end: the shape's id and its connection site (an index into its geometry's sites).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionEnd {
    pub id: u32,
    pub site: u32,
}

/// A shape's shadow and glow.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeEffects {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<ShapeShadow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glow: Option<ShapeGlow>,
}

/// An outer shadow: the silhouette in `color` (opacity `alpha`, 1,000ths of a percent), blurred
/// `blur` points, cast `distance` points in direction `angle` (degrees clockwise from right).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeShadow {
    pub color: Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<u32>,
    pub blur: f32,
    pub distance: f32,
    pub angle: f32,
}

/// A glow `size` points wide around the outline, in `color` (opacity `alpha`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeGlow {
    pub color: Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<u32>,
    pub size: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Outline {
    /// Width in EMU (12700 per point).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Preset dash (`solid`, `dash`, `sysDot`…).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash: Option<String>,
    /// Line cap (`flat`, `rnd`, `sq`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap: Option<String>,
    /// Line join (`round`, `bevel`, `miter`) and the miter limit (1,000ths of a percent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub miter_limit: Option<u32>,
    /// Opacity of the outline's colour, as [`Shape::fill_alpha`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<u32>,
    /// Arrowheads at the start and end (`a:headEnd@type`, `a:tailEnd@type`) other than `kind`'s.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<String>,
}

impl Outline {
    pub fn is_empty(&self) -> bool {
        *self == Outline::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SparklineKind {
    #[default]
    Line,
    Column,
    WinLoss,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sparkline {
    pub cell: CellRef,
    pub source: String,
    pub kind: SparklineKind,
    pub color: Color,
    pub markers: bool,
}

// ---------------------------------------------------------------- filter & sort

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FilterCriterion {
    /// Show rows whose display text is one of these (case-insensitive); `blanks` includes empty.
    Values {
        values: Vec<String>,
        blanks: bool,
    },
    /// Custom: one or two comparisons (`op` like `>=`, `=*abc*`), joined by and/or.
    Custom {
        a: (String, String),
        b: Option<(String, String)>,
        and: bool,
    },
    Top10 {
        bottom: bool,
        percent: bool,
        count: u32,
    },
    AboveAverage(bool),
    FillColor(Color),
    FontColor(Color),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoFilter {
    pub range: RangeRef,
    /// Column offset within `range` → criterion.
    pub criteria: Vec<(u32, FilterCriterion)>,
    /// Attributes another application gave the filter (its own namespace), kept as written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<ForeignAttr>,
}

/// An attribute of another application's vocabulary, kept as written: its prefixed name, its
/// namespace and its value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignAttr {
    pub name: String,
    pub ns: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintSettings {
    pub orientation: Orientation,
    /// Paper size name (`Letter`, `A4`, `Legal`…).
    pub paper: String,
    /// Margins in inches: left, right, top, bottom, header, footer.
    pub margins: [f32; 6],
    pub print_area: Option<RangeRef>,
    pub title_rows: Option<(u32, u32)>,
    pub title_cols: Option<(u32, u32)>,
    /// Percent scale, or fit to pages (wide, tall).
    pub scale: u16,
    pub fit_to: Option<(u16, u16)>,
    pub gridlines: bool,
    pub headings: bool,
    pub center_h: bool,
    pub center_v: bool,
    pub header: String,
    pub footer: String,
    pub row_breaks: Vec<u32>,
    pub col_breaks: Vec<u32>,
}

impl Default for PrintSettings {
    fn default() -> Self {
        PrintSettings {
            orientation: Orientation::Portrait,
            // WPS (and Excel outside North America) print on A4 unless a sheet says otherwise.
            paper: "A4".into(),
            margins: [0.7, 0.7, 0.75, 0.75, 0.3, 0.3],
            print_area: None,
            title_rows: None,
            title_cols: None,
            scale: 100,
            fit_to: None,
            gridlines: false,
            headings: false,
            center_h: false,
            center_v: false,
            header: String::new(),
            footer: String::new(),
            row_breaks: vec![],
            col_breaks: vec![],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetProtection {
    /// Hash of the password (our own salted FNV; never the password itself).
    pub password_hash: Option<u64>,
    pub select_locked: bool,
    pub select_unlocked: bool,
    pub format_cells: bool,
    pub format_columns: bool,
    pub format_rows: bool,
    pub insert_columns: bool,
    pub insert_rows: bool,
    pub delete_columns: bool,
    pub delete_rows: bool,
    pub sort: bool,
    pub autofilter: bool,
}

/// Simple non-cryptographic password hash for sheet protection (like Excel's, protection is a
/// convenience, not security).
pub fn password_hash(pw: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325 ^ 0x5348_4545_5443;
    for b in pw.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Outline grouping info for a row or column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LineInfo {
    /// Size in points; `None` = default (rows auto-fit to content).
    pub size: Option<f32>,
    pub hidden: bool,
    pub outline: u8,
    pub collapsed: bool,
    pub style: Option<crate::style::StyleId>,
    /// The size was set by the user (rows): automatic heights follow the content.
    #[serde(default)]
    pub custom: bool,
}

// ---------------------------------------------------------------- pivot tables

/// How a pivot table lays out its row fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotLayout {
    /// All row fields in one column, inner items indented ("Row Labels" header).
    #[default]
    Compact,
    /// One column per row field; each item on its own row.
    Outline,
    /// One column per row field; inner items start on their parent's row, totals below.
    Tabular,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotSort {
    #[default]
    Asc,
    Desc,
    /// Source order (first appearance).
    None,
}

/// Grouping of a date field (values are date serials).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotDateGroup {
    #[default]
    None,
    Years,
    Quarters,
    Months,
    Days,
}

/// A field placed in the Rows or Columns area.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotField {
    /// Source column header name.
    pub source_col: String,
    pub sort: PivotSort,
    pub date_group: PivotDateGroup,
    /// Item labels whose detail is hidden (collapsed).
    pub collapsed_items: Vec<String>,
}

/// Summarize-values-by function.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotFunc {
    #[default]
    Sum,
    Count,
    Average,
    Max,
    Min,
    Product,
    CountNumbers,
    StdDev,
    StdDevP,
    Var,
    VarP,
}

/// Show-values-as transform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotShowAs {
    #[default]
    Normal,
    PercentOfGrandTotal,
    PercentOfColumnTotal,
    PercentOfRowTotal,
    /// Running total in the first row field.
    RunningTotal,
    /// Rank largest to smallest among sibling row items.
    Rank,
}

/// A field in the Values area.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotValue {
    pub source_col: String,
    pub func: PivotFunc,
    /// Caption, e.g. "Sum of Sales".
    pub name: String,
    pub show_as: PivotShowAs,
    pub number_format: Option<String>,
}

/// A report filter (or, for a field also in Rows/Columns, its hidden-items filter).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotFilter {
    pub source_col: String,
    /// Item labels shown; `None` = all.
    pub selected: Option<Vec<String>>,
}

/// A pivot table: its definition plus where its output was last written. The output itself is
/// static cell values and styles on the sheet, rewritten on every refresh.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotTable {
    pub id: u32,
    pub name: String,
    /// Source: range formula text (`Sheet1!$A$1:$E$200`), a table name or a defined name.
    pub source: String,
    /// Top-left of the output body on the pivot's sheet.
    pub anchor: CellRef,
    pub rows: Vec<PivotField>,
    pub columns: Vec<PivotField>,
    pub values: Vec<PivotValue>,
    pub filters: Vec<PivotFilter>,
    pub layout: PivotLayout,
    /// Grand total for each row (the "Grand Total" column), like Excel's "On for Rows".
    pub grand_totals_rows: bool,
    /// Grand total for each column (the "Grand Total" row), like Excel's "On for Columns".
    pub grand_totals_cols: bool,
    pub subtotals: bool,
    /// Subtotals at the top of each group (Compact/Outline); otherwise below it.
    pub subtotals_top: bool,
    pub style: String,
    pub show_headers: bool,
    /// Area written by the last refresh.
    pub last_range: Option<RangeRef>,
}

impl Default for PivotTable {
    fn default() -> Self {
        PivotTable {
            id: 0,
            name: "Pivot table 1".into(),
            source: String::new(),
            anchor: CellRef::default(),
            rows: vec![],
            columns: vec![],
            values: vec![],
            filters: vec![],
            layout: PivotLayout::Compact,
            grand_totals_rows: true,
            grand_totals_cols: true,
            subtotals: true,
            subtotals_top: true,
            style: "PivotStyleLight16".into(),
            show_headers: true,
            last_range: None,
        }
    }
}
