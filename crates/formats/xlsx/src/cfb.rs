//! Just enough of the Compound File Binary (CFB/OLE2) container to tell what a non-zip
//! spreadsheet file is: Excel wraps password-protected workbooks in a CFB holding an
//! `EncryptedPackage` stream, while Excel 97-2003 `.xls` (and other legacy Office files) are
//! CFB files without one. Every read is bounds-checked and every loop bounded.

/// The CFB signature at the start of the file.
pub(crate) const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

#[cfg(test)]
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
/// Sector ids at or above this are markers (free, end of chain, FAT/DIFAT sector), not sectors.
const MAX_REG_SECT: u32 = 0xFFFF_FFFA;
/// Directory sectors followed at most (4096 entries with 512-byte sectors).
const MAX_DIR_SECTORS: usize = 1024;
/// DIFAT sectors followed at most.
const MAX_DIFAT_SECTORS: usize = 1024;

/// Stream names that mark an encrypted (password-protected) OOXML package.
const ENCRYPTED_STREAMS: [&str; 2] = ["EncryptedPackage", "EncryptionInfo"];

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([*s.first()?, *s.get(1)?, *s.get(2)?, *s.get(3)?]))
}

/// Whether the compound file holds an encryption stream. Walks the directory through the FAT;
/// when the structure can't be followed (truncated or malformed), falls back to scanning the
/// bytes for the UTF-16LE stream names.
pub(crate) fn is_encrypted(bytes: &[u8]) -> bool {
    match directory_names(bytes) {
        Some(names) => names.iter().any(|n| ENCRYPTED_STREAMS.iter().any(|e| n.eq_ignore_ascii_case(e))),
        None => ENCRYPTED_STREAMS.iter().any(|e| contains_utf16(bytes, e)),
    }
}

fn contains_utf16(hay: &[u8], needle: &str) -> bool {
    let n: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    !n.is_empty() && hay.windows(n.len()).any(|w| w == n.as_slice())
}

/// Names of every directory entry, or `None` when the directory can't be read.
fn directory_names(b: &[u8]) -> Option<Vec<String>> {
    Some(Compound::open(b)?.entries.into_iter().map(|e| e.name).collect())
}

/// Most bytes one stream may have (a hostile size can't make us allocate more).
const MAX_STREAM: usize = 512 * 1024 * 1024;

/// One directory entry.
pub(crate) struct Entry {
    pub name: String,
    /// 1 storage, 2 stream, 5 root (0 when a writer left it unset).
    pub kind: u8,
    start: u32,
    size: u64,
}

/// A compound file opened for reading: its sectors, allocation table and directory.
pub(crate) struct Compound<'a> {
    b: &'a [u8],
    size: usize,
    fat: Vec<u32>,
    pub entries: Vec<Entry>,
    mini_cutoff: u64,
    mini_fat: Vec<u32>,
    /// The root entry's stream: the mini stream small streams live in.
    mini_stream: Vec<u8>,
}

impl<'a> Compound<'a> {
    pub(crate) fn open(b: &'a [u8]) -> Option<Compound<'a>> {
        if !b.starts_with(&SIGNATURE) {
            return None;
        }
        let shift = u16_at(b, 30)?;
        if shift != 9 && shift != 12 {
            return None;
        }
        let size = 1usize << shift;
        let per = size / 4;
        let sector = |id: u32| -> Option<&[u8]> {
            let start = (id as usize).checked_add(1)?.checked_mul(size)?;
            b.get(start..start.checked_add(size)?)
        };
        // The DIFAT: the FAT's sector ids, 109 in the header then chained DIFAT sectors.
        let mut difat: Vec<u32> = (0..109).filter_map(|i| u32_at(b, 76 + i * 4)).filter(|&s| s < MAX_REG_SECT).collect();
        let mut next = u32_at(b, 68)?;
        for _ in 0..MAX_DIFAT_SECTORS {
            if next >= MAX_REG_SECT {
                break;
            }
            let s = sector(next)?;
            difat.extend((0..per - 1).filter_map(|i| u32_at(s, i * 4)).filter(|&x| x < MAX_REG_SECT));
            next = u32_at(s, (per - 1) * 4)?;
        }
        let mut fat = Vec::new();
        for id in difat.iter().take(b.len() / size + 1) {
            let s = sector(*id)?;
            fat.extend((0..per).filter_map(|i| u32_at(s, i * 4)));
        }
        let mut cf = Compound {
            b,
            size,
            fat,
            entries: Vec::new(),
            mini_cutoff: u64::from(u32_at(b, 56).unwrap_or(4096)),
            mini_fat: Vec::new(),
            mini_stream: Vec::new(),
        };
        let dir = cf.chain(u32_at(b, 48)?, MAX_DIR_SECTORS);
        for id in dir {
            let Some(s) = sector(id) else { break };
            for e in s.as_chunks::<128>().0 {
                let len = usize::from(u16_at(e, 64)?);
                let kind = *e.get(66)?;
                if !(2..=64).contains(&len) {
                    continue;
                }
                let units: Vec<u16> = (0..(len - 2) / 2).filter_map(|i| u16_at(e, i * 2)).collect();
                let size = if shift == 9 { u64::from(u32_at(e, 120)?) } else { u64::from(u32_at(e, 120)?) | (u64::from(u32_at(e, 124)?) << 32) };
                cf.entries.push(Entry { name: String::from_utf16_lossy(&units), kind, start: u32_at(e, 116)?, size });
            }
        }
        // The mini FAT and the mini stream (the root entry's data).
        let mini_fat_start = u32_at(b, 60)?;
        let mut mini_fat = Vec::new();
        for id in cf.chain(mini_fat_start, b.len() / size + 1) {
            if let Some(s) = sector(id) {
                mini_fat.extend((0..per).filter_map(|i| u32_at(s, i * 4)));
            }
        }
        cf.mini_fat = mini_fat;
        if let Some(root) = cf.entries.iter().find(|e| e.kind == 5) {
            let (start, len) = (root.start, root.size);
            cf.mini_stream = cf.read_chain(start, len).unwrap_or_default();
        }
        Some(cf)
    }

    /// The sector ids of a chain, at most `limit` of them (a cyclic chain stops there).
    fn chain(&self, first: u32, limit: usize) -> Vec<u32> {
        let mut out = Vec::new();
        let mut id = first;
        while id < MAX_REG_SECT && out.len() < limit {
            out.push(id);
            id = match self.fat.get(id as usize) {
                Some(&n) => n,
                None => break,
            };
        }
        out
    }

    fn read_chain(&self, first: u32, len: u64) -> Option<Vec<u8>> {
        let len = usize::try_from(len).ok()?.min(MAX_STREAM).min(self.b.len());
        let mut out = Vec::with_capacity(len);
        for id in self.chain(first, len / self.size + 2) {
            let start = (id as usize).checked_add(1)?.checked_mul(self.size)?;
            let s = self.b.get(start..start.checked_add(self.size)?.min(self.b.len()))?;
            out.extend_from_slice(s);
            if out.len() >= len {
                break;
            }
        }
        out.truncate(len);
        Some(out)
    }

    /// A stream's bytes by name (case-insensitive), from the regular or the mini stream.
    pub(crate) fn read(&self, name: &str) -> Option<Vec<u8>> {
        let e = self.entries.iter().find(|e| e.kind != 1 && e.kind != 5 && e.name.eq_ignore_ascii_case(name))?;
        if e.size >= self.mini_cutoff {
            return self.read_chain(e.start, e.size);
        }
        let len = usize::try_from(e.size).ok()?.min(self.mini_stream.len());
        let mut out = Vec::with_capacity(len);
        let mut id = e.start;
        for _ in 0..(len / 64 + 2) {
            if id >= MAX_REG_SECT || out.len() >= len {
                break;
            }
            let start = (id as usize).checked_mul(64)?;
            out.extend_from_slice(self.mini_stream.get(start..start.checked_add(64)?.min(self.mini_stream.len()))?);
            id = *self.mini_fat.get(id as usize)?;
        }
        out.truncate(len);
        Some(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal valid compound file (512-byte sectors: FAT at sector 0, directory at sector 1)
    /// whose directory holds a root entry plus the given stream names.
    pub(crate) fn build(streams: &[&str]) -> Vec<u8> {
        let mut b = vec![0u8; 512 * 3];
        b[..8].copy_from_slice(&SIGNATURE);
        b[24..26].copy_from_slice(&0x3Eu16.to_le_bytes()); // minor version
        b[26..28].copy_from_slice(&3u16.to_le_bytes()); // major version
        b[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes()); // byte order
        b[30..32].copy_from_slice(&9u16.to_le_bytes()); // sector shift
        b[32..34].copy_from_slice(&6u16.to_le_bytes()); // mini sector shift
        b[44..48].copy_from_slice(&1u32.to_le_bytes()); // FAT sectors
        b[48..52].copy_from_slice(&1u32.to_le_bytes()); // first directory sector
        b[60..64].copy_from_slice(&END_OF_CHAIN.to_le_bytes()); // no mini FAT
        b[68..72].copy_from_slice(&END_OF_CHAIN.to_le_bytes()); // no DIFAT sectors
        for i in 0..109 {
            b[76 + i * 4..80 + i * 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        }
        b[76..80].copy_from_slice(&0u32.to_le_bytes()); // FAT lives in sector 0
        // FAT (sector 0, at byte 512): sector 0 is a FAT sector, sector 1 ends its chain.
        for i in 0..128 {
            b[512 + i * 4..516 + i * 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        }
        b[512..516].copy_from_slice(&0xFFFF_FFFDu32.to_le_bytes());
        b[516..520].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
        // Directory (sector 1, at byte 1024).
        for (k, name) in std::iter::once("Root Entry").chain(streams.iter().copied()).take(4).enumerate() {
            let e = 1024 + k * 128;
            let units: Vec<u16> = name.encode_utf16().take(31).collect();
            for (i, u) in units.iter().enumerate() {
                b[e + i * 2..e + i * 2 + 2].copy_from_slice(&u.to_le_bytes());
            }
            b[e + 64..e + 66].copy_from_slice(&((units.len() as u16 + 1) * 2).to_le_bytes());
            b[e + 66] = if k == 0 { 5 } else { 2 };
        }
        b
    }

    #[test]
    fn reads_directory_names() {
        let b = build(&["EncryptionInfo", "EncryptedPackage"]);
        assert_eq!(directory_names(&b), Some(vec!["Root Entry".into(), "EncryptionInfo".into(), "EncryptedPackage".into()]));
        assert!(is_encrypted(&b));
        assert!(!is_encrypted(&build(&["Workbook", "\u{5}SummaryInformation"])));
    }

    #[test]
    fn hostile_headers_never_panic() {
        let mut b = build(&["Workbook"]);
        for at in [30usize, 48, 68, 76, 512, 516, 1024 + 64] {
            for v in [0u32, 1, 7, 0xFFFF_FFFA, 0xFFFF_FFFF, 0x7FFF_FFFF] {
                let mut c = b.clone();
                c[at..at + 4].copy_from_slice(&v.to_le_bytes());
                let _ = is_encrypted(&c);
                for cut in [0usize, 8, 100, 600, 1100] {
                    let _ = is_encrypted(&c[..cut.min(c.len())]);
                }
            }
        }
        // A directory chain that loops back on itself stops at the bound.
        b[516..520].copy_from_slice(&1u32.to_le_bytes());
        assert!(!is_encrypted(&b));
    }
}
