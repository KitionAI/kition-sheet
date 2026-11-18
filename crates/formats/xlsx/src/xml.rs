//! A small XML tree built on `quick-xml`, with streaming of repeated elements (worksheet rows,
//! shared strings) so large parts never need a full tree, plus escaping helpers for writing.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::IoError;

/// Maximum element nesting accepted in any part.
pub const MAX_DEPTH: usize = 256;

/// An element: local name (namespace prefix dropped), attributes by local name, children and
/// its direct text content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct El {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<El>,
    pub text: String,
}

impl El {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
    pub fn attr_u32(&self, k: &str) -> Option<u32> {
        self.attr(k).and_then(|v| v.trim().parse::<u32>().ok())
    }
    pub fn attr_i64(&self, k: &str) -> Option<i64> {
        self.attr(k).and_then(|v| v.trim().parse::<i64>().ok())
    }
    pub fn attr_f64(&self, k: &str) -> Option<f64> {
        self.attr(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
    }
    /// xsd:boolean (`1`/`true`/`0`/`false`).
    pub fn attr_bool(&self, k: &str) -> Option<bool> {
        match self.attr(k)?.trim() {
            "1" | "true" | "on" | "t" => Some(true),
            "0" | "false" | "off" | "f" => Some(false),
            _ => None,
        }
    }
    pub fn flag(&self, k: &str, default: bool) -> bool {
        self.attr_bool(k).unwrap_or(default)
    }
    pub fn child(&self, name: &str) -> Option<&El> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn kids<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// Follows a path of child names.
    pub fn path(&self, names: &[&str]) -> Option<&El> {
        let mut e = self;
        for n in names {
            e = e.child(n)?;
        }
        Some(e)
    }
    /// `val` attribute of a child (the common `<c:x val=".."/>` pattern).
    pub fn child_val(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|c| c.attr("val"))
    }
}

/// Text of a SpreadsheetML string item (`<si>`, `<is>`, comment `<text>`): `<t>` and rich text
/// runs `<r><t>`, phonetic runs ignored. Decodes `_xHHHH_` escapes.
pub fn rich_text(e: &El) -> String {
    let mut s = String::new();
    for c in &e.children {
        match c.name.as_str() {
            "t" => s.push_str(&c.text),
            "r" => {
                for t in c.kids("t") {
                    s.push_str(&t.text);
                }
            }
            _ => {}
        }
    }
    decode_xstring(&s)
}

/// Converts input bytes to UTF-8 XML text (handles BOMs and UTF-16).
fn to_utf8(xml: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if let Some(rest) = xml.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return std::borrow::Cow::Borrowed(rest);
    }
    let utf16 = |le: bool, b: &[u8]| -> Vec<u8> {
        let units = b.as_chunks::<2>().0.iter().map(|c| if le { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) });
        let s: String = char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')).collect();
        // The declaration may claim UTF-16; quick-xml without the encoding feature ignores it.
        s.into_bytes()
    };
    if let Some(rest) = xml.strip_prefix(&[0xFF, 0xFE]) {
        return std::borrow::Cow::Owned(utf16(true, rest));
    }
    if let Some(rest) = xml.strip_prefix(&[0xFE, 0xFF]) {
        return std::borrow::Cow::Owned(utf16(false, rest));
    }
    // UTF-16 without BOM: `<\0?\0`.
    if xml.len() >= 4 && xml[1] == 0 && xml[3] == 0 && xml[0] != 0 {
        return std::borrow::Cow::Owned(utf16(true, xml));
    }
    if xml.len() >= 4 && xml[0] == 0 && xml[2] == 0 && xml[1] != 0 {
        return std::borrow::Cow::Owned(utf16(false, xml));
    }
    std::borrow::Cow::Borrowed(xml)
}

fn local(name: &[u8]) -> String {
    let n = match name.iter().rposition(|&b| b == b':') {
        Some(i) => name.get(i + 1..).unwrap_or(name),
        None => name,
    };
    String::from_utf8_lossy(n).into_owned()
}

fn start_el(e: &BytesStart<'_>) -> El {
    let mut el = El { name: local(e.name().as_ref()), ..Default::default() };
    for a in e.attributes().with_checks(false).flatten() {
        let key = a.key.as_ref();
        // Namespace declarations carry no data for us.
        if key == b"xmlns" || key.starts_with(b"xmlns:") {
            continue;
        }
        let v = match a.unescape_value() {
            Ok(v) => v.into_owned(),
            Err(_) => String::from_utf8_lossy(&a.value).into_owned(),
        };
        el.attrs.push((local(key), v));
    }
    el
}

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    })
}

/// Parses a whole part into a tree.
pub fn parse(xml: &[u8]) -> Result<El, IoError> {
    parse_streaming(xml, &[], &mut |_| Ok(()))
}

/// Parses a part; complete elements named in `split` (below the root) are handed to `cb` and not
/// kept in the returned tree.
pub fn parse_streaming(xml: &[u8], split: &[&str], cb: &mut dyn FnMut(El) -> Result<(), IoError>) -> Result<El, IoError> {
    let data = to_utf8(xml);
    let mut reader = Reader::from_reader(data.as_ref());
    {
        let cfg = reader.config_mut();
        cfg.check_end_names = false;
        cfg.allow_unmatched_ends = true;
        cfg.allow_dangling_amp = true;
        cfg.trim_text_start = false;
        cfg.trim_text_end = false;
    }
    let mut stack: Vec<El> = Vec::new();
    let mut root: Option<El> = None;
    let mut buf = Vec::new();
    // Attach a finished element to its parent (or emit it / make it the root).
    fn finish(stack: &mut [El], root: &mut Option<El>, el: El, split: &[&str], cb: &mut dyn FnMut(El) -> Result<(), IoError>) -> Result<(), IoError> {
        match stack.last_mut() {
            Some(parent) => {
                if split.contains(&el.name.as_str()) {
                    cb(el)?;
                } else {
                    parent.children.push(el);
                }
            }
            None => {
                if root.is_none() {
                    *root = Some(el);
                }
            }
        }
        Ok(())
    }
    loop {
        let ev = reader.read_event_into(&mut buf).map_err(|e| IoError::Xml(format!("{e} at byte {}", reader.buffer_position())))?;
        match ev {
            Event::Start(e) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(IoError::Xml("XML nested too deeply".into()));
                }
                stack.push(start_el(&e));
            }
            Event::Empty(e) => {
                let el = start_el(&e);
                finish(&mut stack, &mut root, el, split, cb)?;
            }
            Event::End(_) => {
                if let Some(el) = stack.pop() {
                    finish(&mut stack, &mut root, el, split, cb)?;
                }
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    let s = t.xml_content().map_err(|e| IoError::Xml(e.to_string()))?;
                    top.text.push_str(&s);
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    match r.resolve_char_ref() {
                        Ok(Some(c)) => top.text.push(c),
                        _ => {
                            let name = String::from_utf8_lossy(&r).into_owned();
                            match named_entity(&name) {
                                Some(c) => top.text.push(c),
                                None => {
                                    top.text.push('&');
                                    top.text.push_str(&name);
                                    top.text.push(';');
                                }
                            }
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    // Unclosed elements (truncated part): close them so we keep what we have.
    while let Some(el) = stack.pop() {
        finish(&mut stack, &mut root, el, split, cb)?;
    }
    root.ok_or_else(|| IoError::Xml("no root element".into()))
}

// ---------------------------------------------------------------- extension lists

/// An extension list (`extLst`) of a part, as written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtList {
    /// Local names of the elements the list is in, outermost first.
    pub path: Vec<String>,
    /// The last drawing object id (`cNvPr@id`) met before the list.
    pub object: Option<String>,
    /// Its `ext` elements as written, each declaring the namespaces it uses from its ancestors,
    /// so it can be copied into another document. Extensions naming relationships (`r:id`,
    /// `r:embed`, …) are left out: the parts they name are not kept.
    pub exts: Vec<String>,
}

const NS_RELATIONSHIPS: [&str; 2] =
    ["http://schemas.openxmlformats.org/officeDocument/2006/relationships", "http://purl.oclc.org/ooxml/officeDocument/relationships"];

/// Upper bound of the extension markup kept from one part.
const MAX_EXT_BYTES: usize = 1 << 20;

/// Every extension list of a part ([`ExtList`]).
pub fn extension_lists(xml: &[u8]) -> Vec<ExtList> {
    let data = to_utf8(xml);
    let bytes: &[u8] = data.as_ref();
    let mut reader = Reader::from_reader(bytes);
    {
        let cfg = reader.config_mut();
        cfg.check_end_names = false;
        cfg.allow_unmatched_ends = true;
        cfg.allow_dangling_amp = true;
    }
    // Open elements: local name and the namespaces each declares.
    let mut stack: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut out: Vec<ExtList> = Vec::new();
    let mut list: Option<(usize, ExtList)> = None;
    let mut ext_start: Option<(usize, usize)> = None;
    let mut object = None;
    let mut kept = 0usize;
    let mut buf = Vec::new();
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let Ok(event) = reader.read_event_into(&mut buf) else { break };
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                let name = local(e.name().as_ref());
                if name == "cNvPr"
                    && let Some(id) = start_el(e).attr("id")
                {
                    object = Some(id.to_owned());
                }
                let in_list = list.as_ref().is_some_and(|(depth, _)| *depth == stack.len());
                if name == "ext" && in_list && ext_start.is_none() {
                    if empty {
                        take_ext(bytes.get(before..after), &stack, &mut list, &mut kept);
                    } else {
                        ext_start = Some((before, stack.len()));
                    }
                }
                if name == "extLst" && !empty && list.is_none() {
                    let path = stack.iter().map(|(n, _)| n.clone()).collect();
                    list = Some((stack.len() + 1, ExtList { path, object: object.clone(), exts: vec![] }));
                }
                if !empty {
                    if stack.len() >= MAX_DEPTH {
                        break;
                    }
                    let decls = e
                        .attributes()
                        .with_checks(false)
                        .flatten()
                        .filter_map(|a| {
                            let key = a.key.as_ref();
                            let prefix = key.strip_prefix(b"xmlns:")?;
                            Some((String::from_utf8_lossy(prefix).into_owned(), String::from_utf8_lossy(&a.value).into_owned()))
                        })
                        .collect();
                    stack.push((name, decls));
                }
            }
            Event::End(_) => {
                stack.pop();
                if let Some((start, depth)) = ext_start
                    && stack.len() == depth
                {
                    ext_start = None;
                    take_ext(bytes.get(start..after), &stack, &mut list, &mut kept);
                }
                if list.as_ref().is_some_and(|(depth, _)| stack.len() + 1 == *depth)
                    && let Some((_, l)) = list.take()
                {
                    out.push(l);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

/// Adds an `ext` element's markup to the open list, made self-contained.
fn take_ext(raw: Option<&[u8]>, stack: &[(String, Vec<(String, String)>)], list: &mut Option<(usize, ExtList)>, kept: &mut usize) {
    let (Some(raw), Some((_, l))) = (raw, list.as_mut()) else { return };
    let raw = String::from_utf8_lossy(raw);
    if *kept + raw.len() > MAX_EXT_BYTES {
        return;
    }
    // Namespaces in scope from the ancestors, inner declarations first.
    let mut scope: Vec<(&str, &str)> = Vec::new();
    for (_, decls) in stack.iter().rev() {
        for (p, uri) in decls {
            if !scope.iter().any(|(q, _)| q == p) {
                scope.push((p, uri));
            }
        }
    }
    let used = used_prefixes(&raw);
    // Leave out extensions that name relationships.
    let declared_inside = |p: &str| raw.contains(&format!("xmlns:{p}="));
    for p in &used {
        let bound_outside = scope.iter().find(|(q, _)| q == p).map(|(_, u)| *u);
        let bound_inside = NS_RELATIONSHIPS.iter().any(|u| raw.contains(&format!("xmlns:{p}=\"{u}\"")));
        if bound_inside || (!declared_inside(p) && bound_outside.is_some_and(|u| NS_RELATIONSHIPS.contains(&u))) {
            return;
        }
    }
    let mut decls = String::new();
    for p in &used {
        if declared_inside(p) {
            continue;
        }
        if let Some((_, uri)) = scope.iter().find(|(q, _)| q == p) {
            decls.push_str(&format!(" xmlns:{p}=\"{}\"", esc_attr(uri)));
        }
    }
    let at = raw.char_indices().skip(1).find(|(_, c)| c.is_whitespace() || *c == '>' || *c == '/').map_or(raw.len(), |(i, _)| i);
    let mut out = String::with_capacity(raw.len() + decls.len());
    out.push_str(raw.get(..at).unwrap_or_default());
    out.push_str(&decls);
    out.push_str(raw.get(at..).unwrap_or_default());
    *kept += out.len();
    l.exts.push(out);
}

/// The attributes in another vocabulary (prefixed, not namespace declarations, not naming
/// relationships) of the first `<element>` of a part, with their namespaces as declared on the
/// element or the root.
pub fn foreign_attrs(xml: &[u8], element: &str) -> Vec<kition_sheet_model::ForeignAttr> {
    let data = to_utf8(xml);
    let bytes: &[u8] = data.as_ref();
    let decls = |e: &BytesStart<'_>| -> Vec<(String, String)> {
        e.attributes()
            .with_checks(false)
            .flatten()
            .filter_map(|a| {
                let prefix = a.key.as_ref().strip_prefix(b"xmlns:")?;
                Some((String::from_utf8_lossy(prefix).into_owned(), String::from_utf8_lossy(&a.value).into_owned()))
            })
            .collect()
    };
    // The root's declarations.
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let scope = loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => break decls(&e),
            Ok(Event::Eof) | Err(_) => return vec![],
            Ok(_) => {}
        }
        buf.clear();
    };
    let needle = format!("<{element}");
    let Some(at) = bytes.windows(needle.len()).enumerate().find_map(|(i, w)| {
        (w == needle.as_bytes() && bytes.get(i + needle.len()).is_some_and(|c| c.is_ascii_whitespace() || *c == b'>' || *c == b'/')).then_some(i)
    }) else {
        return vec![];
    };
    let Some(tag) = bytes.get(at..).and_then(|rest| rest.iter().position(|c| *c == b'>').and_then(|end| rest.get(..=end))) else { return vec![] };
    let mut reader = Reader::from_reader(tag);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let Ok(Event::Start(e) | Event::Empty(e)) = reader.read_event_into(&mut buf) else { return vec![] };
    let mut local_scope = decls(&e);
    local_scope.extend(scope);
    e.attributes()
        .with_checks(false)
        .flatten()
        .filter_map(|a| {
            let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
            let (prefix, _) = key.split_once(':')?;
            if prefix == "xmlns" || prefix == "xml" {
                return None;
            }
            let ns = local_scope.iter().find(|(p, _)| p == prefix).map(|(_, u)| u.clone())?;
            if NS_RELATIONSHIPS.contains(&ns.as_str()) {
                return None;
            }
            let value = a.unescape_value().map(|v| v.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned());
            Some(kition_sheet_model::ForeignAttr { name: key, ns, value })
        })
        .collect()
}

/// Namespace prefixes markup appears to use (`<p:name`, `</p:name`, ` p:attr=`).
fn used_prefixes(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let starts = matches!(b.get(i), Some(b'<' | b' ' | b'\t' | b'\n' | b'\r' | b'/'));
        i += 1;
        if !starts {
            continue;
        }
        let begin = i;
        while b.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')) {
            i += 1;
        }
        if i > begin && b.get(i) == Some(&b':') && b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
            let p = String::from_utf8_lossy(b.get(begin..i).unwrap_or_default()).into_owned();
            if p != "xmlns" && p != "xml" && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- writing helpers

/// DrawingML markup kept as written: `e` with its element named `root` (`xdr:txBody`) and every
/// element below it in the DrawingML main namespace (`a:`). Extension lists, markup-compatibility
/// blocks and elements naming relationships (hyperlinks) are left out, as their targets are not
/// kept; text is kept in elements without children (`a:t`).
pub fn dml_xml(e: &El, root: &str) -> String {
    fn walk(e: &El, name: &str, out: &mut String, depth: usize) {
        out.push('<');
        out.push_str(name);
        for (k, v) in &e.attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            out.push_str(&esc_attr(v));
            out.push('"');
        }
        let kids: Vec<&El> = e.children.iter().filter(|c| !skipped(c)).collect();
        if kids.is_empty() && (e.text.is_empty() || !e.children.is_empty()) {
            out.push_str("/>");
            return;
        }
        out.push('>');
        if kids.is_empty() {
            out.push_str(&esc(&e.text));
        } else if depth < MAX_DEPTH {
            for c in kids {
                walk(c, &format!("a:{}", c.name), out, depth + 1);
            }
        }
        out.push_str("</");
        out.push_str(name);
        out.push('>');
    }
    fn skipped(e: &El) -> bool {
        matches!(e.name.as_str(), "extLst" | "AlternateContent" | "hlinkClick" | "hlinkMouseOver" | "hlinkHover")
    }
    let mut out = String::new();
    walk(e, root, &mut out, 0);
    out
}

/// Resolves the predefined and numeric character references of text taken as written.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(end) = after.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let entity = &after[1..end];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Escapes text content.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            c if is_xml_char(c) => o.push(c),
            _ => {}
        }
    }
    o
}

/// Escapes an attribute value (double-quoted).
pub fn esc_attr(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            '\t' => o.push_str("&#9;"),
            c if is_xml_char(c) => o.push(c),
            _ => {}
        }
    }
    o
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

/// ST_Xstring encoding for cell text: characters XML can't carry become `_xHHHH_`, and literal
/// `_xHHHH_` sequences are protected with `_x005F_`. The result still needs [`esc`].
pub fn encode_xstring(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' && looks_like_escape(&chars, i) {
            o.push_str("_x005F_");
        } else if !is_xml_char(c) || (c.is_control() && !matches!(c, '\t' | '\n' | '\r')) {
            o.push_str(&format!("_x{:04X}_", c as u32 & 0xFFFF));
        } else {
            o.push(c);
        }
    }
    o
}

fn looks_like_escape(chars: &[char], i: usize) -> bool {
    chars.get(i + 1) == Some(&'x') && chars.get(i + 6) == Some(&'_') && (i + 2..i + 6).all(|k| chars.get(k).is_some_and(|c| c.is_ascii_hexdigit()))
}

/// Inverse of [`encode_xstring`].
pub fn decode_xstring(s: &str) -> String {
    if !s.contains("_x") {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut o = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '_' && looks_like_escape(&chars, i) {
            let hex: String = chars[i + 2..i + 6].iter().collect();
            if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                o.push(ch);
                i += 7;
                continue;
            }
        }
        o.push(chars[i]);
        i += 1;
    }
    o
}

/// `<t>` element with `xml:space="preserve"` when needed.
pub fn t_el(s: &str) -> String {
    let enc = esc(&encode_xstring(s));
    if s.starts_with([' ', '\t', '\n', '\r']) || s.ends_with([' ', '\t', '\n', '\r']) || s.contains('\n') {
        format!("<t xml:space=\"preserve\">{enc}</t>")
    } else {
        format!("<t>{enc}</t>")
    }
}

/// Formats a number for an XML attribute/value (shortest round-trip form).
pub fn num(n: f64) -> String {
    if !n.is_finite() {
        return "0".into();
    }
    if n == n.trunc() && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    format!("{n:?}").replace('e', "E")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let x = br#"<?xml version="1.0"?><a:root xmlns:a="x" k="v &amp; w"><b>t &lt; u&#65;</b><c/><![CDATA[<raw>]]></a:root>"#;
        let e = parse(x).unwrap();
        assert_eq!(e.name, "root");
        assert_eq!(e.attr("k"), Some("v & w"));
        assert_eq!(e.child("b").unwrap().text, "t < uA");
        assert!(e.child("c").is_some());
        assert_eq!(e.text, "<raw>");
    }

    #[test]
    fn streaming_split() {
        let x = b"<r><row n='1'/><row n='2'><c/></row><other/></r>";
        let mut n = 0;
        let root = parse_streaming(x, &["row"], &mut |e| {
            assert_eq!(e.name, "row");
            n += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn deep_nesting_is_an_error() {
        let mut s = String::new();
        for _ in 0..10_000 {
            s.push_str("<a>");
        }
        assert!(parse(s.as_bytes()).is_err());
    }

    #[test]
    fn garbage() {
        assert!(parse(b"").is_err());
        assert!(parse(b"\x00\x01\x02 not xml").is_err() || parse(b"\x00\x01\x02 not xml").is_ok());
        let _ = parse(b"<a><b></a>");
        let _ = parse(b"<a attr='&bogus;'>&unknown;</a>");
    }

    #[test]
    fn xstring() {
        assert_eq!(decode_xstring("a_x000D_b"), "a\rb");
        assert_eq!(encode_xstring("a\u{1}b"), "a_x0001_b");
        assert_eq!(decode_xstring(&encode_xstring("_x0041_")), "_x0041_");
        assert_eq!(decode_xstring("_x00"), "_x00");
    }

    #[test]
    fn numbers() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(0.1), "0.1");
        assert_eq!(num(1e-7), "1E-7");
        assert_eq!(num(-3.0), "-3");
        assert_eq!(num(1.5e20), "1.5E20");
        assert_eq!("1.5E20".parse::<f64>().unwrap(), 1.5e20);
    }
}
