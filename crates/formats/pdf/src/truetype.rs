//! TrueType fonts embedded in PDFs: for text the standard 14 fonts cannot show (Chinese,
//! Japanese, Korean, Cyrillic beyond Latin-1…). The host gives the fonts at run time
//! ([`add_font`]); a PDF that uses one embeds a subset of it (only the glyphs it shows,
//! renumbered from 1) as a `CIDFontType2` with `Identity-H` encoding and a `ToUnicode` map, so
//! the text stays searchable and copyable.
//!
//! Only TrueType outlines (`glyf`) are read; files are checked as they are parsed (bounds,
//! counts) and a malformed font is refused, never trusted.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(at)?, *d.get(at + 1)?]))
}
fn i16_at(d: &[u8], at: usize) -> Option<i16> {
    u16_at(d, at).map(|v| v as i16)
}
fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*d.get(at)?, *d.get(at + 1)?, *d.get(at + 2)?, *d.get(at + 3)?]))
}

/// A parsed TrueType font: what layout and embedding need.
pub struct TrueType {
    data: Vec<u8>,
    tables: BTreeMap<[u8; 4], (usize, usize)>,
    /// Its PostScript-safe name (letters and digits).
    pub name: String,
    pub units_per_em: u16,
    pub ascent: i16,
    pub descent: i16,
    bbox: [i16; 4],
    num_glyphs: u16,
    long_loca: bool,
    h_metrics: u16,
    cmap: HashMap<u32, u16>,
}

/// Most glyphs a font may declare (the format's own limit).
const MAX_GLYPHS: usize = 65_535;

impl TrueType {
    /// Parses a font file; `None` when it is not a TrueType-outline font or is malformed.
    pub fn parse(data: Vec<u8>) -> Option<TrueType> {
        let version = u32_at(&data, 0)?;
        if version != 0x0001_0000 && version != u32::from_be_bytes(*b"true") {
            return None;
        }
        let count = usize::from(u16_at(&data, 4)?);
        let mut tables = BTreeMap::new();
        for i in 0..count {
            let at = 12 + i * 16;
            let tag: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
            let offset = u32_at(&data, at + 8)? as usize;
            let length = u32_at(&data, at + 12)? as usize;
            if offset.checked_add(length)? > data.len() {
                return None;
            }
            tables.insert(tag, (offset, length));
        }
        for needed in [b"head", b"hhea", b"maxp", b"hmtx", b"loca", b"glyf", b"cmap"] {
            if !tables.contains_key(needed) {
                return None;
            }
        }
        let head = tables.get(b"head")?.0;
        let units_per_em = u16_at(&data, head + 18)?.max(16);
        let bbox = [i16_at(&data, head + 36)?, i16_at(&data, head + 38)?, i16_at(&data, head + 40)?, i16_at(&data, head + 42)?];
        let long_loca = i16_at(&data, head + 50)? == 1;
        let hhea = tables.get(b"hhea")?.0;
        let ascent = i16_at(&data, hhea + 4)?;
        let descent = i16_at(&data, hhea + 6)?;
        let h_metrics = u16_at(&data, hhea + 34)?.max(1);
        let num_glyphs = u16_at(&data, tables.get(b"maxp")?.0 + 4)?;
        let mut font = TrueType {
            data,
            tables,
            name: String::new(),
            units_per_em,
            ascent,
            descent,
            bbox,
            num_glyphs,
            long_loca,
            h_metrics,
            cmap: HashMap::new(),
        };
        font.cmap = font.read_cmap()?;
        font.name = font.read_name().unwrap_or_else(|| "Embedded".into());
        Some(font)
    }

    fn table(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        let (offset, length) = *self.tables.get(tag)?;
        self.data.get(offset..offset + length)
    }

    fn read_cmap(&self) -> Option<HashMap<u32, u16>> {
        let cmap = self.table(b"cmap")?;
        let count = usize::from(u16_at(cmap, 2)?);
        // Prefer the full Unicode map (3,10 or 0,4/0,6), then the BMP one.
        let mut best: Option<(u8, usize)> = None;
        for i in 0..count {
            let at = 4 + i * 8;
            let (platform, encoding) = (u16_at(cmap, at)?, u16_at(cmap, at + 2)?);
            let offset = u32_at(cmap, at + 4)? as usize;
            let rank = match (platform, encoding) {
                (3, 10) | (0, 4) | (0, 6) => 3,
                (3, 1) | (0, 3) => 2,
                (0, _) => 1,
                _ => continue,
            };
            if best.is_none_or(|(r, _)| rank > r) {
                best = Some((rank, offset));
            }
        }
        let (_, offset) = best?;
        let sub = cmap.get(offset..)?;
        let mut out = HashMap::new();
        match u16_at(sub, 0)? {
            4 => {
                let segments = usize::from(u16_at(sub, 6)? / 2);
                let ends = 14;
                let starts = ends + segments * 2 + 2;
                let deltas = starts + segments * 2;
                let ranges = deltas + segments * 2;
                for s in 0..segments {
                    let (end, start) = (u16_at(sub, ends + s * 2)?, u16_at(sub, starts + s * 2)?);
                    let delta = u16_at(sub, deltas + s * 2)?;
                    let range = usize::from(u16_at(sub, ranges + s * 2)?);
                    if start > end || start == 0xFFFF {
                        continue;
                    }
                    for c in start..=end {
                        let glyph = if range == 0 {
                            c.wrapping_add(delta)
                        } else {
                            let at = ranges + s * 2 + range + usize::from(c - start) * 2;
                            match u16_at(sub, at) {
                                Some(0) | None => 0,
                                Some(g) => g.wrapping_add(delta),
                            }
                        };
                        if glyph != 0 && glyph < self.num_glyphs {
                            out.insert(u32::from(c), glyph);
                        }
                    }
                }
            }
            12 => {
                let groups = u32_at(sub, 12)? as usize;
                for g in 0..groups.min(100_000) {
                    let at = 16 + g * 12;
                    let (start, end, first) = (u32_at(sub, at)?, u32_at(sub, at + 4)?, u32_at(sub, at + 8)?);
                    if start > end || end > 0x10FFFF || end - start > 0x10000 {
                        continue;
                    }
                    for c in start..=end {
                        let glyph = first.saturating_add(c - start);
                        if glyph != 0 && glyph < u32::from(self.num_glyphs) {
                            out.insert(c, glyph as u16);
                        }
                    }
                }
            }
            _ => return None,
        }
        Some(out)
    }

    fn read_name(&self) -> Option<String> {
        let name = self.table(b"name")?;
        let count = usize::from(u16_at(name, 2)?);
        let strings = usize::from(u16_at(name, 4)?);
        for i in 0..count {
            let at = 6 + i * 12;
            let (platform, id) = (u16_at(name, at)?, u16_at(name, at + 6)?);
            if id != 6 {
                continue;
            }
            let (length, offset) = (usize::from(u16_at(name, at + 8)?), usize::from(u16_at(name, at + 10)?));
            let raw = name.get(strings + offset..strings + offset + length)?;
            let text: String = if platform == 3 || platform == 0 {
                raw.chunks(2).filter_map(|c| (c.len() == 2).then(|| char::from_u32(u32::from(u16::from_be_bytes([c[0], c[1]])))).flatten()).collect()
            } else {
                raw.iter().map(|&b| char::from(b)).collect()
            };
            let clean: String = text.chars().filter(char::is_ascii_alphanumeric).take(48).collect();
            if !clean.is_empty() {
                return Some(clean);
            }
        }
        None
    }

    /// The glyph that draws `c`, if the font has one.
    pub fn glyph(&self, c: char) -> Option<u16> {
        self.cmap.get(&u32::from(c)).copied()
    }

    /// A glyph's advance width in font units.
    pub fn advance(&self, glyph: u16) -> u16 {
        let Some(hmtx) = self.table(b"hmtx") else { return 0 };
        let index = usize::from(glyph.min(self.h_metrics - 1));
        u16_at(hmtx, index * 4).unwrap_or(0)
    }

    /// The advance of `c` in thousandths of an em (PDF glyph space), if the font has it.
    pub fn width_1000(&self, c: char) -> Option<u32> {
        let glyph = self.glyph(c)?;
        Some(u32::from(self.advance(glyph)) * 1000 / u32::from(self.units_per_em))
    }

    fn glyph_data(&self, glyph: u16) -> Option<&[u8]> {
        let loca = self.table(b"loca")?;
        let glyf = self.table(b"glyf")?;
        let g = usize::from(glyph);
        let (start, end) = if self.long_loca {
            (u32_at(loca, g * 4)? as usize, u32_at(loca, g * 4 + 4)? as usize)
        } else {
            (usize::from(u16_at(loca, g * 2)?) * 2, usize::from(u16_at(loca, g * 2 + 2)?) * 2)
        };
        if start >= end {
            return Some(&[]);
        }
        glyf.get(start..end)
    }

    /// The glyphs a composite glyph is made of (its components).
    fn components(data: &[u8]) -> Vec<(usize, u16)> {
        let mut out = Vec::new();
        if i16_at(data, 0).is_none_or(|contours| contours >= 0) {
            return out;
        }
        let mut at = 10;
        for _ in 0..64 {
            let (Some(flags), Some(glyph)) = (u16_at(data, at), u16_at(data, at + 2)) else { break };
            out.push((at + 2, glyph));
            at += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
            at += if flags & 0x0008 != 0 {
                2
            } else if flags & 0x0040 != 0 {
                4
            } else if flags & 0x0080 != 0 {
                8
            } else {
                0
            };
            if flags & 0x0020 == 0 {
                break;
            }
        }
        out
    }

    /// A font file holding only `glyphs` (original ids, in their new order: the new id of
    /// `glyphs[i]` is `i`; `glyphs[0]` should be 0, `.notdef`). Components of composite glyphs
    /// must be in the list too ([`TrueType::closure`]).
    /// `chars` (character, new glyph id) become the subset's own `cmap`.
    pub fn subset(&self, glyphs: &[u16], chars: &[(char, u16)]) -> Option<Vec<u8>> {
        let renumber: HashMap<u16, u16> = glyphs.iter().enumerate().map(|(i, g)| (*g, i as u16)).collect();
        let mut glyf = Vec::new();
        let mut loca = Vec::with_capacity((glyphs.len() + 1) * 4);
        let mut hmtx = Vec::with_capacity(glyphs.len() * 4);
        let hmtx_src = self.table(b"hmtx")?;
        for &g in glyphs {
            loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
            let mut data = self.glyph_data(g).unwrap_or(&[]).to_vec();
            for (at, component) in Self::components(&data) {
                let new = renumber.get(&component).copied().unwrap_or(0);
                if let Some(slot) = data.get_mut(at..at + 2) {
                    slot.copy_from_slice(&new.to_be_bytes());
                }
            }
            glyf.extend_from_slice(&data);
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
            let lsb_at =
                if g < self.h_metrics { usize::from(g) * 4 + 2 } else { usize::from(self.h_metrics) * 4 + usize::from(g - self.h_metrics) * 2 };
            hmtx.extend_from_slice(&self.advance(g).to_be_bytes());
            hmtx.extend_from_slice(&i16_at(hmtx_src, lsb_at).unwrap_or(0).to_be_bytes());
        }
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        let count = glyphs.len() as u16;
        let mut head = self.table(b"head")?.to_vec();
        head.get_mut(8..12)?.copy_from_slice(&[0; 4]);
        head.get_mut(50..52)?.copy_from_slice(&1i16.to_be_bytes());
        let mut hhea = self.table(b"hhea")?.to_vec();
        hhea.get_mut(34..36)?.copy_from_slice(&count.to_be_bytes());
        let mut maxp = self.table(b"maxp")?.to_vec();
        maxp.get_mut(4..6)?.copy_from_slice(&count.to_be_bytes());
        // post format 3: no glyph names.
        let mut post = vec![0, 3, 0, 0];
        post.extend_from_slice(self.table(b"post").and_then(|p| p.get(4..32)).unwrap_or(&[0; 28]));
        let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
            (*b"head", head),
            (*b"hhea", hhea),
            (*b"maxp", maxp),
            (*b"hmtx", hmtx),
            (*b"loca", loca),
            (*b"glyf", glyf),
            (*b"post", post),
            (*b"cmap", cmap_format4(chars)),
        ];
        for tag in [b"cvt ", b"fpgm", b"prep", b"OS/2", b"name"] {
            if let Some(t) = self.table(tag) {
                tables.push((*tag, t.to_vec()));
            }
        }
        tables.sort_by_key(|(tag, _)| *tag);
        Some(assemble(tables))
    }

    /// `glyphs` plus every glyph their composites use, `.notdef` first, deduplicated.
    pub fn closure(&self, glyphs: impl IntoIterator<Item = u16>) -> Vec<u16> {
        let mut out = vec![0u16];
        let mut seen: std::collections::HashSet<u16> = std::iter::once(0).collect();
        let mut todo: Vec<u16> = glyphs.into_iter().collect();
        while let Some(g) = todo.pop() {
            if g >= self.num_glyphs || !seen.insert(g) || out.len() >= MAX_GLYPHS {
                continue;
            }
            out.push(g);
            if let Some(data) = self.glyph_data(g) {
                todo.extend(Self::components(data).into_iter().map(|(_, c)| c));
            }
        }
        out
    }

    /// The font's bounding box in thousandths of an em.
    pub fn bbox_1000(&self) -> [i32; 4] {
        self.bbox.map(|v| i32::from(v) * 1000 / i32::from(self.units_per_em))
    }

    /// Ascent and descent (negative) in thousandths of an em.
    pub fn metrics_1000(&self) -> (i32, i32) {
        let k = |v: i16| i32::from(v) * 1000 / i32::from(self.units_per_em);
        (k(self.ascent), k(self.descent))
    }
}

/// A Unicode BMP `cmap` (platform 3, encoding 1, format 4) mapping each character to its glyph.
fn cmap_format4(chars: &[(char, u16)]) -> Vec<u8> {
    let mut pairs: Vec<(u16, u16)> =
        chars.iter().filter_map(|(c, g)| u16::try_from(u32::from(*c)).ok().filter(|u| *u != 0xFFFF).map(|u| (u, *g))).collect();
    pairs.sort_unstable();
    pairs.dedup_by_key(|(u, _)| *u);
    pairs.truncate(8000);
    let segments = pairs.len() + 1;
    let mut sub = Vec::new();
    let length = 16 + segments * 8;
    for v in [4u16, length as u16, 0, (segments * 2) as u16] {
        sub.extend_from_slice(&v.to_be_bytes());
    }
    let mut power = 1usize;
    let mut log = 0u16;
    while power * 2 <= segments {
        power *= 2;
        log += 1;
    }
    for v in [(power * 2) as u16, log, ((segments - power) * 2) as u16] {
        sub.extend_from_slice(&v.to_be_bytes());
    }
    for (u, _) in &pairs {
        sub.extend_from_slice(&u.to_be_bytes());
    }
    sub.extend_from_slice(&0xFFFFu16.to_be_bytes());
    sub.extend_from_slice(&[0, 0]);
    for (u, _) in &pairs {
        sub.extend_from_slice(&u.to_be_bytes());
    }
    sub.extend_from_slice(&0xFFFFu16.to_be_bytes());
    for (u, g) in &pairs {
        sub.extend_from_slice(&g.wrapping_sub(*u).to_be_bytes());
    }
    sub.extend_from_slice(&1u16.to_be_bytes());
    for _ in 0..segments {
        sub.extend_from_slice(&[0, 0]);
    }
    let mut out = Vec::new();
    for v in [0u16, 1, 3, 1] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&12u32.to_be_bytes());
    out.extend_from_slice(&sub);
    out
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// A font file from its tables (sorted by tag), with checksums.
fn assemble(tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let count = tables.len() as u16;
    let mut power = 1u16;
    let mut log = 0u16;
    while power * 2 <= count {
        power *= 2;
        log += 1;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&(power * 16).to_be_bytes());
    out.extend_from_slice(&log.to_be_bytes());
    out.extend_from_slice(&(count * 16 - power * 16).to_be_bytes());
    let mut offset = 12 + usize::from(count) * 16;
    let mut head_at = None;
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        if tag == b"head" {
            head_at = Some(offset);
        }
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    if let Some(at) = head_at {
        let adjust = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        if let Some(slot) = out.get_mut(at + 8..at + 12) {
            slot.copy_from_slice(&adjust.to_be_bytes());
        }
    }
    out
}

/// The fonts the host added, tried in order for characters the standard fonts lack.
static FONTS: RwLock<Vec<Arc<TrueType>>> = RwLock::new(Vec::new());

/// Adds a TrueType font for PDF text the standard fonts cannot show. Answers whether it was
/// usable (TrueType outlines, well formed). Adding the same font twice is harmless.
pub fn add_font(bytes: &[u8]) -> bool {
    let Some(font) = TrueType::parse(bytes.to_vec()) else { return false };
    let mut fonts = FONTS.write().unwrap_or_else(|poisoned| poisoned.into_inner());
    if fonts.iter().any(|f| f.name == font.name && f.num_glyphs == font.num_glyphs) {
        return true;
    }
    fonts.push(Arc::new(font));
    true
}

/// The added font that draws `c` (its index among the added fonts, and the font).
pub(crate) fn font_for(c: char) -> Option<(usize, Arc<TrueType>)> {
    let fonts = FONTS.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    fonts.iter().enumerate().find(|(_, f)| f.glyph(c).is_some()).map(|(i, f)| (i, f.clone()))
}

/// The added font at `index`.
pub(crate) fn font_at(index: usize) -> Option<Arc<TrueType>> {
    FONTS.read().unwrap_or_else(|poisoned| poisoned.into_inner()).get(index).cloned()
}
