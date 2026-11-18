//! Spreadsheet lengths as WPS Office for Mac lays them out (module M9, decision D1): Kition
//! Sheet matches WPS rather than Excel for Windows, whose units come from Windows' 96 dpi pixels.
//! The rules were measured black-box from WPS's own PDF exports of synthetic workbooks.
//!
//! - **Column widths.** A width of `w` characters is `w` times one *character unit*, a whole
//!   number of 1/180-inch steps (0.4 pt) set by the workbook's default font: its digit width
//!   at that resolution. Calibri, Arial or Helvetica at 11 pt is 16 steps (6.4 pt, 8.53 px), so
//!   a column 10 characters wide is 64 pt. There is no padding term (Excel adds 5 pixels).
//! - **Row heights are whole points.** A row WPS sizes itself is one line of the default font:
//!   16 pt at 11 pt, whatever the face ([`default_row_height_pt`]); a height a file gives that
//!   is not a whole number of pixels (`ht="37.15"`) is cut to whole points ([`file_row_height_pt`]).
//! - **Wrapped text.** Lines are 1.525 em apart ([`line_pitch_pt`]); a row of `n` wrapped lines
//!   is `n` pitches tall, rounded up to a whole point ([`wrapped_height_pt`]).

/// One step of WPS's character unit, in points (1/180 inch).
pub const CHAR_STEP_PT: f64 = 0.4;

/// Pixels (96 dpi) per point.
pub const PX_PER_PT: f64 = 96.0 / 72.0;

/// How a face's digits measure: their width in em, and whether WPS rounds the width in steps to
/// the nearest step (else up).
fn digit_em(font: &str) -> (f64, bool) {
    let name = font.trim().to_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| name.contains(k));
    if has(&["仿宋", "fangsong"]) {
        (0.545, false)
    } else if has(&["楷", "kaiti", "stkaiti"]) {
        (0.5, false)
    } else if has(&["宋", "simsun", "songti", "stsong", "nsimsun"]) {
        (7.0 / 15.0, true)
    } else if has(&["雅黑", "yahei", "pingfang", "苹方", "等线", "dengxian", "cambria"]) {
        (0.6, false)
    } else if has(&["times", "georgia", "garamond", "palatino", "book antiqua", "serif"]) && !name.contains("sans") {
        (0.5, false)
    } else {
        (0.556, false)
    }
}

/// WPS's character unit for a default font, in 1/180-inch steps (16 for Calibri 11).
pub fn char_steps(font: &str, size: f32) -> f64 {
    let size = if size.is_finite() { f64::from(size).clamp(1.0, 409.0) } else { 11.0 };
    let (em, nearest) = digit_em(font);
    let steps = size * 2.5 * em;
    let steps = if nearest { (steps + 0.5).floor() } else { (steps - 1e-6).ceil() };
    steps.max(1.0)
}

/// The width of one character of column width, in pixels at 96 dpi.
pub fn char_width_px(font: &str, size: f32) -> f64 {
    char_steps(font, size) * CHAR_STEP_PT * PX_PER_PT
}

/// The height WPS gives a row of one line in a font of `size` points (any face), in whole
/// points.
pub fn default_row_height_pt(size: f32) -> f64 {
    // Measured: font size → row height (pt).
    const TABLE: [(f64, f64); 10] =
        [(9.0, 13.0), (10.0, 15.0), (11.0, 16.0), (12.0, 17.0), (13.0, 19.0), (14.0, 20.0), (16.0, 23.0), (18.0, 26.0), (24.0, 34.0), (36.0, 51.0)];
    let size = if size.is_finite() { f64::from(size).clamp(1.0, 409.0) } else { 11.0 };
    let first = TABLE[0];
    let last = TABLE[TABLE.len() - 1];
    let exact = if size <= first.0 {
        first.1 * size / first.0
    } else if size >= last.0 {
        last.1 * size / last.0
    } else {
        TABLE.windows(2).find(|pair| size <= pair[1].0).map_or(last.1, |pair| {
            let ((a, ha), (b, hb)) = (pair[0], pair[1]);
            ha + (hb - ha) * (size - a) / (b - a)
        })
    };
    exact.round().max(1.0)
}

/// A row height a file gives (`ht`, points) as WPS for Mac shows it: whole points, cut down,
/// unless it is a whole number of pixels (as Excel and Kition Sheet write them), which is kept.
pub fn file_row_height_pt(pt: f64) -> f64 {
    if !pt.is_finite() || pt <= 0.0 {
        return 0.0;
    }
    let px = pt * PX_PER_PT;
    if (px - px.round()).abs() < 0.01 || (pt - pt.round()).abs() < 1e-6 { pt } else { pt.floor().max(1.0) }
}

/// The distance between the baselines of wrapped lines in a font of `size` points, in points.
pub fn line_pitch_pt(size: f32) -> f64 {
    let size = if size.is_finite() { f64::from(size).clamp(1.0, 409.0) } else { 11.0 };
    size * 1.525
}

/// The height of a row holding `lines` wrapped lines in a font of `size` points, in points.
pub fn wrapped_height_pt(size: f32, lines: usize) -> f64 {
    let lines = lines.clamp(1, 10_000) as f64;
    (lines * line_pitch_pt(size) - 1e-6).ceil()
}

/// Where the first baseline of top-aligned text sits below the cell's top, in points.
pub fn first_baseline_pt(size: f32) -> f64 {
    let size = if size.is_finite() { f64::from(size).clamp(1.0, 409.0) } else { 11.0 };
    size * 1.1
}

/// The face of a font as a file names it: a theme font (`scheme` minor or major) is the
/// theme's Latin face.
pub fn face_name<'a>(font: &'a crate::Font, theme: &'a crate::Theme) -> &'a str {
    match font.scheme.as_deref() {
        Some("minor") if !theme.minor_font.is_empty() => &theme.minor_font,
        Some("major") if !theme.major_font.is_empty() => &theme.major_font,
        _ => &font.name,
    }
}

/// The workbook's default font (its Normal style's): face name and size.
pub fn normal_font(wb: &crate::Workbook) -> (String, f32) {
    let font = &wb.styles.get(crate::StyleId::DEFAULT).font;
    (face_name(font, &wb.theme).to_owned(), font.size)
}

/// The width of one character of column width in a workbook, in pixels at 96 dpi.
pub fn workbook_char_px(wb: &crate::Workbook) -> f64 {
    let (name, size) = normal_font(wb);
    char_width_px(&name, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The character units WPS for Mac uses (measured from its PDF exports).
    #[test]
    fn character_units_match_wps() {
        for (font, size, steps) in [
            ("Calibri", 11.0, 16.0),
            ("Calibri", 12.0, 17.0),
            ("Calibri", 10.0, 14.0),
            ("Arial", 11.0, 16.0),
            ("Helvetica", 11.0, 16.0),
            ("黑体", 11.0, 16.0),
            ("微软雅黑", 11.0, 17.0),
            ("Cambria", 11.0, 17.0),
            ("Times New Roman", 11.0, 14.0),
            ("仿宋", 11.0, 15.0),
            ("楷体", 11.0, 14.0),
            ("宋体", 9.0, 11.0),
            ("宋体", 10.0, 12.0),
            ("宋体", 11.0, 13.0),
            ("宋体", 12.0, 14.0),
            ("宋体", 13.0, 15.0),
            ("宋体", 14.0, 16.0),
            ("宋体", 16.0, 19.0),
            ("宋体", 18.0, 21.0),
            ("宋体", 24.0, 28.0),
            ("宋体", 36.0, 42.0),
        ] {
            assert_eq!(char_steps(font, size), steps, "{font} {size}");
        }
        assert!((char_width_px("Calibri", 11.0) - 8.5333).abs() < 0.001);
    }

    #[test]
    fn row_heights_and_wrapped_lines_match_wps() {
        assert_eq!(default_row_height_pt(11.0), 16.0);
        assert_eq!(default_row_height_pt(9.0), 13.0);
        assert_eq!(default_row_height_pt(15.0), 22.0);
        // Heights a generator wrote (37.15 pt) are cut to whole points; whole pixels are kept.
        assert_eq!(file_row_height_pt(37.15), 37.0);
        assert_eq!(file_row_height_pt(14.6), 14.0);
        assert_eq!(file_row_height_pt(14.25), 14.25);
        assert_eq!(file_row_height_pt(16.0), 16.0);
        // Four, six and eight lines of 10 pt text: 61, 92 and 122 pt.
        assert_eq!(wrapped_height_pt(10.0, 4), 61.0);
        assert_eq!(wrapped_height_pt(10.0, 6), 92.0);
        assert_eq!(wrapped_height_pt(10.0, 8), 122.0);
        // Hostile sizes stay finite.
        assert!(default_row_height_pt(f32::NAN).is_finite() && char_steps("x", f32::INFINITY).is_finite());
    }
}
