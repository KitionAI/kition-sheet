//! A small rasteriser for the grid: axis-aligned rectangles and lines, and single-line text with
//! ab_glyph, into straight RGBA8. The upstream editor drew its grid with egui (since removed);
//! the in-page editor needs pixels, so that cell painting is redone here.

use std::sync::RwLock;

use ab_glyph::{Font, FontArc, FontVec, PxScale, ScaleFont, VariableFont};

pub type Rgba = [u8; 4];

/// How far a synthetic italic leans: the horizontal shift per pixel of height (about 12°).
pub const OBLIQUE: f64 = 0.21;

/// The faces cells are drawn in: egui's defaults, as Kition Sheet's web build uses.
pub struct Faces {
    pub sans: FontArc,
    pub mono: FontArc,
}

impl Faces {
    pub fn load() -> Option<Self> {
        Some(Self {
            sans: FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).ok()?,
            mono: FontArc::try_from_slice(epaint_default_fonts::HACK_REGULAR).ok()?,
        })
    }

    /// The face a cell set in the font `name` is drawn in: the open, metric-compatible stand-in
    /// for an Office font when the editor has added it (decision F1), so text takes the width it
    /// takes in Excel; else egui's defaults. `bold` and `italic` in the answer are what is left to
    /// synthesise (the stand-in has no such face).
    pub fn pick(&self, name: &str, bold: bool, italic: bool) -> Picked {
        self.pick_from(&FAMILIES.read().unwrap_or_else(|poisoned| poisoned.into_inner()), name, bold, italic)
    }

    /// `pick` among the stand-in faces `families`.
    pub(crate) fn pick_from(&self, families: &[Styled], name: &str, bold: bool, italic: bool) -> Picked {
        let lower = name.trim().to_lowercase();
        // Chinese faces draw their Latin letters in the Arial stand-in, as WPS for Mac draws them
        // in a sans face (and the engine measures them as Arial); their Chinese characters come
        // from the fallback faces.
        let cjk = kition_sheet_engine::metrics::CJK_FACES.contains(&lower.as_str());
        // A Chinese face with an open counterpart the editor added (楷体: a Kai face) draws in it,
        // its Chinese characters and its Latin letters alike.
        if let Some(styled) = CJK_STAND_INS.iter().find(|(chinese, _)| *chinese == lower).and_then(|(_, open)| {
            families.iter().filter(|styled| styled.family == *open && !styled.italic).min_by_key(|styled| u8::from(styled.bold))
        }) {
            return Picked { face: styled.face.clone(), bold: bold && !styled.bold, italic };
        }
        let family = STAND_INS.iter().find(|(office, _)| *office == lower).map_or(if cjk { "arimo" } else { lower.as_str() }, |(_, open)| open);
        let best = families
            .iter()
            .filter(|styled| styled.family == family && (!styled.bold || bold) && (!styled.italic || italic))
            .max_by_key(|styled| u8::from(styled.bold) + u8::from(styled.italic));
        if let Some(styled) = best {
            return Picked { face: styled.face.clone(), bold: bold && !styled.bold, italic: italic && !styled.italic };
        }
        let mono = lower.contains("courier") || lower.contains("mono") || lower.contains("consolas");
        Picked { face: if mono { self.mono.clone() } else { self.sans.clone() }, bold, italic }
    }
}

/// What the renderer draws a font in when it has no face of that family (see
/// [`Faces::substitute`]).
pub struct Substitute {
    /// The open, metric-compatible stand-in to add for an Office font (`carlito` for Calibri).
    pub stand_in: Option<&'static str>,
    /// Drawn in the built-in monospace face (else the built-in sans).
    pub mono: bool,
}

impl Faces {
    /// `None` when a face of the font `name`'s family (or its stand-in's) has been added; else
    /// what it is drawn in instead.
    pub fn substitute(&self, name: &str) -> Option<Substitute> {
        let lower = name.trim().to_ascii_lowercase();
        let stand_in = STAND_INS.iter().find(|(office, _)| *office == lower).map(|(_, open)| *open);
        let family = stand_in.unwrap_or(lower.as_str());
        let families = FAMILIES.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        if families.iter().any(|styled| styled.family == family) {
            return None;
        }
        Some(Substitute { stand_in, mono: lower.contains("courier") || lower.contains("mono") || lower.contains("consolas") })
    }
}

/// Whether the host has added any font at run time (see [`add_fallback`]).
pub fn fonts_added() -> bool {
    !FAMILIES.read().unwrap_or_else(|poisoned| poisoned.into_inner()).is_empty()
        || !FALLBACKS.read().unwrap_or_else(|poisoned| poisoned.into_inner()).is_empty()
}

/// Whether a face the renderer has draws `c` (the text's face, a face added at run time, or a
/// built-in one); otherwise it shows as an empty box.
pub fn draws(primary: &FontArc, c: char) -> bool {
    c.is_control() || face_for(primary, c).1.glyph_id(c).0 != 0
}

/// The faces tried last for a character no other face has: the built-in sans and monospace
/// faces and the symbol faces that come with them (★ ✔ ☐ ☑ ▲ ▼ ● →…).
fn built_in() -> &'static [FontArc] {
    static FACES: std::sync::OnceLock<Vec<FontArc>> = std::sync::OnceLock::new();
    FACES.get_or_init(|| {
        [
            epaint_default_fonts::UBUNTU_LIGHT,
            epaint_default_fonts::HACK_REGULAR,
            epaint_default_fonts::NOTO_EMOJI_REGULAR,
            epaint_default_fonts::EMOJI_ICON,
        ]
        .into_iter()
        .filter_map(|bytes| FontArc::try_from_slice(bytes).ok())
        .collect()
    })
}

/// A face picked for a font (see `Faces::pick`), and the styles it still has to synthesise.
pub struct Picked {
    pub face: FontArc,
    pub bold: bool,
    pub italic: bool,
}

/// Office fonts and their open, metric-compatible stand-ins (the same advance widths).
const STAND_INS: &[(&str, &str)] = &[
    ("calibri", "carlito"),
    ("calibri light", "carlito"),
    ("cambria", "caladea"),
    ("arial", "arimo"),
    ("helvetica", "arimo"),
    ("liberation sans", "arimo"),
    ("times new roman", "tinos"),
    ("times", "tinos"),
    ("liberation serif", "tinos"),
    ("courier new", "cousine"),
    ("courier", "cousine"),
    ("liberation mono", "cousine"),
];

/// Chinese fonts and the open faces of their style that kition-fonts has: Ma Shan Zheng, a Kai
/// (brush script) face, for 楷体. kition-fonts has no open Song (宋体) or FangSong (仿宋) face; those
/// draw their Chinese characters in the fallback faces.
const CJK_STAND_INS: &[(&str, &str)] = &[
    ("楷体", "ma shan zheng"),
    ("kaiti", "ma shan zheng"),
    ("楷体_gb2312", "ma shan zheng"),
    ("stkaiti", "ma shan zheng"),
    ("华文楷体", "ma shan zheng"),
    ("kaiti sc", "ma shan zheng"),
];

/// A face added at run time for its family (see `STAND_INS`), with its style.
pub(crate) struct Styled {
    family: String,
    bold: bool,
    italic: bool,
    face: FontArc,
}

/// The stand-in faces added at run time, by family.
static FAMILIES: RwLock<Vec<Styled>> = RwLock::new(Vec::new());

/// A font file's family (lower case) and whether it is bold and italic, from its tables.
fn family_of(bytes: &[u8]) -> Option<(String, bool, bool)> {
    let face = ttf_parser::Face::parse(bytes, 0).ok()?;
    let name = |id| face.names().into_iter().find(|n| n.name_id == id && n.is_unicode()).and_then(|n| n.to_string());
    let family = name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| name(ttf_parser::name_id::FAMILY))?;
    Some((family.to_ascii_lowercase(), face.is_bold() || face.weight().to_number() >= 600, face.is_italic()))
}

/// A font file's faces when it is a stand-in for an Office font (see `STAND_INS`): the face, and
/// a variable one's bold instance too.
pub(crate) fn stand_ins(bytes: &[u8]) -> Option<Vec<Styled>> {
    family_of(bytes).filter(|(family, ..)| STAND_INS.iter().any(|(_, open)| open == family)).map(|named| styled(bytes, named))
}

/// A font file's faces under its family name: the face, and a variable one's bold instance too.
fn styled(bytes: &[u8], (family, bold, italic): (String, bool, bool)) -> Vec<Styled> {
    let mut added = Vec::new();
    if let Ok(mut heavy) = FontVec::try_from_vec(bytes.to_vec())
        && !bold
        && heavy.set_variation(b"wght", 700.0)
    {
        added.push(Styled { family: family.clone(), bold: true, italic, face: FontArc::new(heavy) });
    }
    if let Ok(face) = FontArc::try_from_vec(bytes.to_vec()) {
        added.push(Styled { family, bold, italic, face });
    }
    added
}

/// Faces added at run time for characters the built-in ones lack (Chinese in a web build, which
/// has no system fonts), tried in the order added.
static FALLBACKS: RwLock<Vec<FontArc>> = RwLock::new(Vec::new());

/// Adds a font file (TTF or OTF): a stand-in for an Office font joins its family; any other face
/// draws the cells set in its family (JetBrains Mono for `JetBrains Mono`) and is a fallback for
/// characters other faces lack. Answers how many faces it added.
pub fn add_fallback(bytes: Vec<u8>) -> usize {
    if let Some(added) = stand_ins(&bytes) {
        let count = added.len();
        FAMILIES.write().unwrap_or_else(|poisoned| poisoned.into_inner()).extend(added);
        return count;
    }
    if let Some(named) = family_of(&bytes) {
        FAMILIES.write().unwrap_or_else(|poisoned| poisoned.into_inner()).extend(styled(&bytes, named));
    }
    match FontArc::try_from_vec(bytes) {
        Ok(face) => {
            FALLBACKS.write().unwrap_or_else(|poisoned| poisoned.into_inner()).push(face);
            1
        }
        Err(_) => 0,
    }
}

/// The face that draws `c`: `primary`, or the first fallback that has it. The number tells faces
/// apart (0 is `primary`), so kerning stays within one face.
fn face_for(primary: &FontArc, c: char) -> (usize, FontArc) {
    if c.is_control() || primary.glyph_id(c).0 != 0 {
        return (0, primary.clone());
    }
    let fallbacks = FALLBACKS.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    fallbacks
        .iter()
        .chain(built_in())
        .enumerate()
        .find(|(_, face)| face.glyph_id(c).0 != 0)
        .map_or_else(|| (0, primary.clone()), |(index, face)| (index + 1, face.clone()))
}

/// An area being drawn: `w` × `h` pixels, clipped to `clip` (x0, y0, x1, y1).
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// A rectangle in canvas pixels.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn intersect(self, other: Rect) -> Rect {
        Rect { x0: self.x0.max(other.x0), y0: self.y0.max(other.y0), x1: self.x1.min(other.x1), y1: self.y1.min(other.y1) }
    }

    pub fn is_empty(self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

impl Canvas {
    pub fn new(w: u32, h: u32, background: Rgba) -> Self {
        Self { w: w as usize, h: h as usize, px: background.repeat(w as usize * h as usize) }
    }

    fn bounds(&self) -> Rect {
        Rect { x0: 0.0, y0: 0.0, x1: self.w as f64, y1: self.h as f64 }
    }

    /// Blends `color` at `coverage` (0..1) over the pixel, keeping it opaque.
    fn blend(&mut self, x: usize, y: usize, color: Rgba, coverage: f32) {
        let a = coverage.clamp(0.0, 1.0) * f32::from(color[3]) / 255.0;
        if a <= 0.0 || x >= self.w || y >= self.h {
            return;
        }
        let i = (y * self.w + x) * 4;
        if let Some(px) = self.px.get_mut(i..i + 3) {
            for (channel, source) in px.iter_mut().zip(color) {
                *channel = (f32::from(source) * a + f32::from(*channel) * (1.0 - a)).round() as u8;
            }
        }
    }

    /// Draws straight-alpha RGBA `image` (`w` × `h`) with its top-left at (`x`, `y`), clipped to
    /// `clip`.
    pub fn image(&mut self, image: &[u8], w: usize, h: usize, x: f64, y: f64, clip: Rect) {
        let clip = clip.intersect(self.bounds());
        let (ox, oy) = (x.round() as i64, y.round() as i64);
        for row in 0..h {
            let py = oy + row as i64;
            if (py as f64) < clip.y0 || (py as f64) >= clip.y1 {
                continue;
            }
            for col in 0..w {
                let px = ox + col as i64;
                if (px as f64) < clip.x0 || (px as f64) >= clip.x1 {
                    continue;
                }
                let i = (row * w + col) * 4;
                if let Some(&[r, g, b, a]) = image.get(i..i + 4).and_then(|p| <&[u8; 4]>::try_from(p).ok()) {
                    self.blend(px as usize, py as usize, [r, g, b, 255], f32::from(a) / 255.0);
                }
            }
        }
    }

    /// Fills `rect` (pixel edges rounded to whole pixels).
    pub fn fill(&mut self, rect: Rect, color: Rgba) {
        let r = rect.intersect(self.bounds());
        if r.is_empty() {
            return;
        }
        let (x0, x1) = (r.x0.round() as usize, r.x1.round() as usize);
        for y in r.y0.round() as usize..r.y1.round() as usize {
            for x in x0..x1 {
                self.blend(x, y, color, 1.0);
            }
        }
    }

    /// A horizontal line `width` pixels thick whose top edge is at `y`.
    pub fn hline(&mut self, x0: f64, x1: f64, y: f64, width: f64, color: Rgba) {
        self.fill(Rect { x0, y0: y, x1, y1: y + width }, color);
    }

    /// A vertical line `width` pixels thick whose left edge is at `x`.
    pub fn vline(&mut self, x: f64, y0: f64, y1: f64, width: f64, color: Rgba) {
        self.fill(Rect { x0: x, y0, x1: x + width, y1 }, color);
    }
}

/// The scale that sets `face`'s em at `size` pixels (a point size is an em; ab_glyph's own
/// `PxScale` is the face's ascent-to-descent height, which made text a fifth too small).
pub fn em_scale(face: &FontArc, size: f32) -> PxScale {
    let upm = face.units_per_em().filter(|u| *u > 0.0).unwrap_or(1000.0);
    let height = face.height_unscaled();
    PxScale::from(if height > 0.0 { size * height / upm } else { size })
}

/// One line of text set in a face at a pixel size (its em).
pub struct Line<'a> {
    pub font: &'a FontArc,
    pub size: f32,
    pub text: &'a str,
}

impl Line<'_> {
    pub fn width(&self) -> f64 {
        let mut width = 0.0;
        let mut last = None;
        for c in self.text.chars() {
            let (which, face) = face_for(self.font, c);
            let font = face.as_scaled(em_scale(&face, self.size));
            let id = font.glyph_id(c);
            if let Some((before, prev)) = last
                && before == which
            {
                width += font.kern(prev, id);
            }
            width += font.h_advance(id);
            last = Some((which, id));
        }
        f64::from(width)
    }

    /// The glyph outlines of the text with its baseline at `y`, starting at `x` (pixels), as one
    /// path; `italic` slants them by [`OBLIQUE`].
    pub fn outline(&self, x: f64, y: f64, italic: bool) -> kition_office_core::kurbo::BezPath {
        use kition_office_core::kurbo::{BezPath, Point};
        let mut path = BezPath::new();
        let mut pen = x;
        let mut last = None;
        for c in self.text.chars() {
            let (which, face) = face_for(self.font, c);
            let scaled = face.as_scaled(em_scale(&face, self.size));
            let id = scaled.glyph_id(c);
            if let Some((before, prev)) = last
                && before == which
            {
                pen += f64::from(scaled.kern(prev, id));
            }
            let (kx, ky) = (f64::from(scaled.h_scale_factor()), f64::from(scaled.v_scale_factor()));
            let at = |p: ab_glyph::Point| {
                let (gx, gy) = (f64::from(p.x) * kx, f64::from(p.y) * ky);
                Point::new(pen + gx + if italic { gy * OBLIQUE } else { 0.0 }, y - gy)
            };
            if let Some(o) = face.outline(id) {
                let mut end: Option<Point> = None;
                for curve in &o.curves {
                    let (start, stop) = match curve {
                        ab_glyph::OutlineCurve::Line(a, b) | ab_glyph::OutlineCurve::Quad(a, _, b) | ab_glyph::OutlineCurve::Cubic(a, _, _, b) => {
                            (at(*a), at(*b))
                        }
                    };
                    if end.is_none_or(|e| (e - start).hypot() > 1e-6) {
                        if end.is_some() {
                            path.close_path();
                        }
                        path.move_to(start);
                    }
                    match curve {
                        ab_glyph::OutlineCurve::Line(..) => path.line_to(stop),
                        ab_glyph::OutlineCurve::Quad(_, q, _) => path.quad_to(at(*q), stop),
                        ab_glyph::OutlineCurve::Cubic(_, q, r, _) => path.curve_to(at(*q), at(*r), stop),
                    }
                    end = Some(stop);
                }
                if end.is_some() {
                    path.close_path();
                }
            }
            pen += f64::from(scaled.h_advance(id));
            last = Some((which, id));
        }
        path
    }

    /// Ascent and descent in pixels.
    pub fn metrics(&self) -> (f64, f64) {
        let font = self.font.as_scaled(em_scale(self.font, self.size));
        (f64::from(font.ascent()), f64::from(-font.descent()))
    }

    /// Draws the text with its baseline at `y`, starting at `x`, clipped to `clip`; `bold`
    /// overstrikes it, as egui fakes bold when a face has none.
    pub fn draw(&self, canvas: &mut Canvas, x: f64, y: f64, clip: Rect, color: Rgba, bold: bool) {
        self.draw_styled(canvas, x, y, clip, color, bold, false);
    }

    /// [`Line::draw`], and `italic` slants it (a synthetic oblique: the faces have no italic, so
    /// each pixel moves right by [`OBLIQUE`] of its height above the baseline).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_styled(&self, canvas: &mut Canvas, x: f64, y: f64, clip: Rect, color: Rgba, bold: bool, italic: bool) {
        self.draw_weighted(canvas, x, y, clip, color, bold, bold, italic);
    }

    /// [`Line::draw_styled`], emboldening the characters of the text's own face when `bold` and
    /// those a fallback face draws (Chinese in a Latin face, say) when `heavy`: a bold face for
    /// the Latin letters has no bold Chinese.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_weighted(&self, canvas: &mut Canvas, x: f64, y: f64, clip: Rect, color: Rgba, bold: bool, heavy: bool, italic: bool) {
        let clip = clip.intersect(canvas.bounds());
        if clip.is_empty() {
            return;
        }
        let strikes: &[f64] = if bold || heavy { &[0.0, (f64::from(self.size) / 24.0).max(0.5)] } else { &[0.0] };
        for (pass, dx) in strikes.iter().enumerate() {
            let mut pen = x + dx;
            let mut last = None;
            for c in self.text.chars() {
                let (which, face) = face_for(self.font, c);
                let scaled = face.as_scaled(em_scale(&face, self.size));
                let id = scaled.glyph_id(c);
                if let Some((before, prev)) = last
                    && before == which
                {
                    pen += f64::from(scaled.kern(prev, id));
                }
                let glyph = id.with_scale_and_position(em_scale(&face, self.size), ab_glyph::point(pen as f32, y as f32));
                pen += f64::from(scaled.h_advance(id));
                last = Some((which, id));
                if pen < clip.x0 || (pass > 0 && !(if which == 0 { bold } else { heavy })) {
                    continue;
                }
                let Some(outlined) = face.outline_glyph(glyph) else { continue };
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let (px, py) = (f64::from(bounds.min.x) + f64::from(gx), f64::from(bounds.min.y) + f64::from(gy));
                    if !italic {
                        if px >= clip.x0 && px < clip.x1 && py >= clip.y0 && py < clip.y1 {
                            canvas.blend(px as usize, py as usize, color, coverage);
                        }
                        return;
                    }
                    // Slanted: the pixel's coverage split between the two pixels it now spans.
                    let shifted = px + (y - (py + 0.5)) * OBLIQUE;
                    let left = shifted.floor();
                    let right_share = (shifted - left) as f32;
                    for (column, share) in [(left, 1.0 - right_share), (left + 1.0, right_share)] {
                        if share > 0.0 && column >= clip.x0 && column < clip.x1 && py >= clip.y0 && py < clip.y1 {
                            canvas.blend(column as usize, py as usize, color, coverage * share);
                        }
                    }
                });
                if pen > clip.x1 {
                    break;
                }
            }
        }
    }
}
