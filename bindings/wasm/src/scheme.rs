//! The host's color scheme in the editor's grid (Kit UI protocol `KitTheme.colorScheme`, Kition's
//! host matrix row H16). A workbook is not a page: as in the spreadsheet apps the kit matches
//! (`docs/parity/ui-parity.md`, "Dark mode, system appearance"), the grid itself follows the
//! scheme. In the dark scheme the empty cells, gridlines, headings, outline gutter, freeze lines and
//! the cell being typed are drawn dark, and the *automatic* colours (a font or border colour the
//! file leaves automatic) are drawn light; every colour the file sets (a fill, a font colour, a
//! border colour, the sheet's gridline colour, a number format's colour) is drawn as it is, with
//! automatic text on a filled cell dark or light by that fill.
//!
//! The scheme is the editor's view only (`editor.scheme`): it is never saved, never marks the
//! workbook changed, and every picture made of the workbook (File › Export as Picture, the agent's
//! `file.render`, PDF and print) is drawn with [`Palette::LIGHT`]. Nothing moves between schemes:
//! only colours differ.

use crate::draw::Rgba;
use crate::{EDITING_TEXT, FREEZE_LINE, GRIDLINE, HEADING, HEADING_LINE, HEADING_TEXT, WHITE};

/// The host's color scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scheme {
    #[default]
    Light,
    Dark,
}

impl Scheme {
    /// `"light"` or `"dark"`; anything else is not a scheme.
    pub fn parse(name: &str) -> Option<Scheme> {
        match name {
            "light" => Some(Scheme::Light),
            "dark" => Some(Scheme::Dark),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scheme::Light => "light",
            Scheme::Dark => "dark",
        }
    }

    pub fn palette(self) -> &'static Palette {
        match self {
            Scheme::Light => &Palette::LIGHT,
            Scheme::Dark => &Palette::DARK,
        }
    }
}

/// The grid's own colours in a scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// The cells without a fill, the inside of a merged area, the outline gutter, the cell being typed.
    pub background: Rgba,
    /// Gridlines, unless the sheet sets their colour.
    pub gridline: Rgba,
    /// The row and column headings' band, their lines and their labels.
    pub heading: Rgba,
    pub heading_line: Rgba,
    pub heading_text: Rgba,
    /// Automatic text on a cell without a fill.
    pub text: Rgba,
    /// An automatic border colour.
    pub border: Rgba,
    /// The frozen panes' lines.
    pub freeze_line: Rgba,
    /// Automatic text in the cell being typed, and its caret.
    pub editing_text: Rgba,
}

/// Black: automatic text on a light fill.
const BLACK: Rgba = [0, 0, 0, 255];

impl Palette {
    /// The light scheme: the colours the grid has always had (and every picture of the workbook).
    pub const LIGHT: Palette = Palette {
        background: WHITE,
        gridline: GRIDLINE,
        heading: HEADING,
        heading_line: HEADING_LINE,
        heading_text: HEADING_TEXT,
        text: BLACK,
        border: BLACK,
        freeze_line: FREEZE_LINE,
        editing_text: EDITING_TEXT,
    };

    /// The dark scheme: Kition's dark paper (#1e1e1e, as its other dark work areas), headings a step
    /// lighter, gridlines as visible against the cells as the light ones are, text at WCAG AA.
    pub const DARK: Palette = Palette {
        background: [30, 30, 30, 255],
        gridline: [62, 62, 66, 255],
        heading: [42, 42, 46, 255],
        heading_line: [72, 72, 78, 255],
        heading_text: [184, 184, 192, 255],
        text: [230, 230, 230, 255],
        border: [200, 200, 204, 255],
        freeze_line: [140, 140, 140, 255],
        editing_text: [240, 240, 240, 255],
    };

    /// Automatic text on a cell with `fill` (none: the grid's background). On the light grid always
    /// black, as before; on the dark grid light, except on a light fill the file set, where black
    /// reads better.
    pub fn auto_text(&self, fill: Option<Rgba>) -> Rgba {
        match fill {
            Some(fill) if *self != Palette::LIGHT => {
                if contrast(BLACK, fill) >= contrast(self.text, fill) {
                    BLACK
                } else {
                    self.text
                }
            }
            _ => self.text,
        }
    }
}

/// WCAG relative luminance.
pub fn luminance(color: Rgba) -> f64 {
    let channel = |v: u8| {
        let c = f64::from(v) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(color[0]) + 0.7152 * channel(color[1]) + 0.0722 * channel(color[2])
}

/// WCAG contrast ratio.
pub fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dark_grid_is_dark_and_its_text_meets_wcag_aa() {
        let dark = Palette::DARK;
        assert!(luminance(dark.background) <= 0.1 && luminance(dark.heading) <= 0.1, "the grid and its headings are dark");
        assert!(contrast(dark.text, dark.background) >= 4.5, "automatic text: {:.2}", contrast(dark.text, dark.background));
        assert!(contrast(dark.editing_text, dark.background) >= 4.5, "the cell being typed");
        assert!(contrast(dark.heading_text, dark.heading) >= 4.5, "heading labels: {:.2}", contrast(dark.heading_text, dark.heading));
        assert!(contrast(dark.border, dark.background) >= 3.0, "automatic borders (non-text, 3:1)");
        // Gridlines and heading lines stand out from the cells at least as much as the light ones.
        let light = Palette::LIGHT;
        assert!(contrast(dark.gridline, dark.background) >= contrast(light.gridline, light.background));
        assert!(contrast(dark.heading_line, dark.heading) >= contrast(light.heading_line, light.heading));
    }

    #[test]
    fn automatic_text_reads_on_the_files_fills() {
        // The light grid draws automatic text black whatever the fill, as it always has.
        assert_eq!(Palette::LIGHT.auto_text(None), BLACK);
        assert_eq!(Palette::LIGHT.auto_text(Some([20, 20, 20, 255])), BLACK);
        // The dark grid: light on its own background, the more readable of black and light on a fill.
        assert_eq!(Palette::DARK.auto_text(None), Palette::DARK.text);
        for fill in [[255, 255, 0, 255], [255, 255, 255, 255], [198, 239, 206, 255], [0, 32, 96, 255], [192, 0, 0, 255], [112, 48, 160, 255]] {
            let text = Palette::DARK.auto_text(Some(fill));
            assert!(contrast(text, fill) >= 4.5, "automatic text on {fill:?}: {:.2}", contrast(text, fill));
        }
    }

    #[test]
    fn schemes_by_name() {
        assert_eq!(Scheme::parse("dark"), Some(Scheme::Dark));
        assert_eq!(Scheme::parse("light"), Some(Scheme::Light));
        assert_eq!(Scheme::parse("sepia"), None);
        assert_eq!(Scheme::default().palette(), &Palette::LIGHT);
        assert_eq!(Scheme::Dark.name(), "dark");
    }
}
