//! The Kition Sheet document model: workbooks, sheets, a copy-on-write cell store, interned
//! styles, defined names, tables, conditional formats, validation, comments, charts and print
//! settings. Pure data; calculation lives in `kition-sheet-calc`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod b64;
pub mod cell;
mod cellmap;
pub mod features;
pub mod identities;
pub mod rich;
pub mod sheet;
pub mod store;
pub mod structure;
pub mod style;
pub mod units;
pub mod workbook;

pub use cell::{Cell, CellPicture, Formula};
pub use features::*;
pub use identities::{CellIdentity, SheetIdentities, WorkbookIdentities};
pub use rich::{RichText, RunColor, RunFont, TextRun};
pub use sheet::{DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT, LineIndex, Sheet, Visibility};
pub use store::CellStore;
pub use structure::StructuralEdit;
pub use style::{
    Alignment, BorderLine, BorderStyle, Borders, Color, Fill, Font, HAlign, NumFmt, PatternType, Style, StyleId, StyleTable, Theme, Underline,
    VAlign, VertAlign,
};
pub use workbook::{
    CalcMode, CalcSettings, CellStyleInfo, DefinedName, DocProps, Extensions, FileVersion, Palette, StyleParts, TableStyleDef, TableStyleElement,
    TableStyles, Workbook, WorkbookView,
};

/// Font used for new workbooks. Only a name: renderers substitute an available open font
/// (e.g. the metric-compatible Carlito) when it isn't installed.
pub const DEFAULT_FONT: &str = "Calibri";
pub const DEFAULT_FONT_SIZE: f32 = 11.0;
