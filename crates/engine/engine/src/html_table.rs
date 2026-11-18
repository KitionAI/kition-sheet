//! The first table of HTML from the system clipboard, as other spreadsheets and web pages put it
//! there: each cell's text (line breaks kept), its place in the grid after the merged cells
//! (`rowspan`, `colspan`) above and before it, and the formatting its styles give it (inline
//! `style`, the classes of a `<style>` block, the table's and row's inherited properties). The
//! tab-separated text beside it loses all of that: a line break in a cell starts a new row there.

use kition_sheet_model::{BorderLine, BorderStyle, Color, HAlign, VAlign};
use std::collections::HashMap;

/// The most cells a pasted table places, and the most rows or columns one cell spans.
const MAX_CELLS: usize = 1_000_000;
const MAX_SPAN: u32 = 1_000;
const MAX_AREA: u32 = 100_000;

#[derive(Debug, Default)]
pub(crate) struct HtmlTable {
    pub cells: Vec<HtmlCell>,
    pub rows: u32,
    pub cols: u32,
    /// Column widths the table declares (`<col>`, or a cell's own width), in pixels.
    pub widths: Vec<Option<f32>>,
}

#[derive(Debug, Default)]
pub(crate) struct HtmlCell {
    pub row: u32,
    pub col: u32,
    pub rows: u32,
    pub cols: u32,
    pub text: String,
    pub look: Look,
}

/// What a cell's styles set; `None` leaves the workbook's default.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Look {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub color: Option<Color>,
    pub fill: Option<Color>,
    pub size: Option<f32>,
    pub font: Option<String>,
    pub h: Option<HAlign>,
    pub v: Option<VAlign>,
    pub wrap: Option<bool>,
    /// Left, right, top, bottom.
    pub borders: [Option<BorderLine>; 4],
}

/// The first table in `html`, or `None` when it has none.
pub(crate) fn parse(html: &str) -> Option<HtmlTable> {
    let tokens = tokenize(html);
    let classes = class_rules(&tokens);
    let mut reader = Reader { classes, ..Reader::default() };
    let mut seen = false;
    for token in &tokens {
        match token {
            Token::Open { name, attrs, .. } if name == "table" => {
                reader.depth += 1;
                if reader.depth == 1 {
                    if seen {
                        break;
                    }
                    seen = true;
                    reader.table = reader.inherited(&[], attrs, "table");
                } else {
                    reader.inner(token);
                }
            }
            Token::Close(name) if name == "table" && reader.depth > 0 => {
                reader.depth -= 1;
                if reader.depth == 0 {
                    reader.end_cell();
                    break;
                }
                reader.inner(token);
            }
            _ if reader.depth == 1 => reader.structure(token),
            _ if reader.depth > 1 => reader.inner(token),
            _ => {}
        }
    }
    reader.end_cell();
    seen.then(|| reader.finish())
}

#[derive(Debug, PartialEq)]
enum Token {
    Open {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Close(String),
    Text(String),
    /// The content of a `<style>` element.
    Style(String),
}

/// HTML as tags and text; comments, declarations, scripts, and unknown syntax are dropped.
fn tokenize(html: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.push(Token::Text(rest.to_string()));
            break;
        };
        if lt > 0 {
            out.push(Token::Text(rest.get(..lt).unwrap_or_default().to_string()));
        }
        rest = rest.get(lt..).unwrap_or_default();
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").and_then(|end| after.get(end + 3..)).unwrap_or_default();
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.find('>').and_then(|end| rest.get(end + 1..)).unwrap_or_default();
            continue;
        }
        let closing = rest.starts_with("</");
        let body = rest.get(if closing { 2 } else { 1 }..).unwrap_or_default();
        let name_len = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == ':' || c == '-')).unwrap_or(body.len());
        if name_len == 0 {
            // A '<' that starts no tag is text.
            out.push(Token::Text("<".into()));
            rest = rest.get(1..).unwrap_or_default();
            continue;
        }
        let name = body.get(..name_len).unwrap_or_default().to_ascii_lowercase();
        let (attrs, after) = attributes(body.get(name_len..).unwrap_or_default());
        rest = after;
        if closing {
            out.push(Token::Close(name));
            continue;
        }
        if name == "style" || name == "script" {
            let end = find_ignore_case(rest, &format!("</{name}"));
            let content = rest.get(..end.unwrap_or(rest.len())).unwrap_or_default();
            if name == "style" {
                out.push(Token::Style(content.to_string()));
            }
            rest = end.and_then(|end| rest.get(end..)).and_then(|r| r.find('>').and_then(|gt| r.get(gt + 1..))).unwrap_or_default();
            continue;
        }
        out.push(Token::Open { name, attrs });
    }
    out
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    let lower = haystack.to_ascii_lowercase();
    lower.find(needle)
}

/// A tag's attributes up to its `>`, and what follows the tag.
fn attributes(mut s: &str) -> (Vec<(String, String)>, &str) {
    let mut attrs = Vec::new();
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if s.is_empty() {
            return (attrs, s);
        }
        if let Some(after) = s.strip_prefix('>') {
            return (attrs, after);
        }
        let first = s.chars().next().map_or(1, char::len_utf8);
        let name_len = s.find(|c: char| c.is_whitespace() || c == '=' || c == '>' || c == '/').unwrap_or(s.len()).max(first);
        let name = s.get(..name_len).unwrap_or_default().to_ascii_lowercase();
        s = s.get(name_len..).unwrap_or_default().trim_start();
        let mut value = String::new();
        if let Some(after) = s.strip_prefix('=') {
            let after = after.trim_start();
            match after.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let inner = after.get(1..).unwrap_or_default();
                    let end = inner.find(q).unwrap_or(inner.len());
                    value = decode_entities(inner.get(..end).unwrap_or_default());
                    s = inner.get(end + 1..).unwrap_or_default();
                }
                _ => {
                    let end = after.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(after.len());
                    value = decode_entities(after.get(..end).unwrap_or_default());
                    s = after.get(end..).unwrap_or_default();
                }
            }
        }
        attrs.push((name, value));
    }
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(rest.get(..amp).unwrap_or_default());
        rest = rest.get(amp..).unwrap_or_default();
        let end = rest.get(1..).map(|r| r.find(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).unwrap_or(r.len()) + 1);
        let named = end.and_then(|e| rest.get(1..e)).unwrap_or_default();
        let decoded = match named {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => named
                .strip_prefix('#')
                .and_then(|n| match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => n.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match (decoded, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                let e = if rest.get(e..e + 1) == Some(";") { e + 1 } else { e };
                rest = rest.get(e..).unwrap_or_default();
            }
            _ => {
                out.push('&');
                rest = rest.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

type Decls = Vec<(String, String)>;

/// `prop: value; …` as lowercase properties and trimmed values.
fn declarations(css: &str) -> Decls {
    css.split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            let v = v.trim().trim_end_matches("!important").trim();
            Some((k.trim().to_ascii_lowercase(), v.to_string()))
        })
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect()
}

/// The `<style>` rules for `.class`, `td.class`, `td`, and `th` (the selectors spreadsheets
/// write), by the class (`""` for the bare `td`/`th` rules).
fn class_rules(tokens: &[Token]) -> HashMap<String, Decls> {
    let mut rules: HashMap<String, Decls> = HashMap::new();
    for token in tokens {
        let Token::Style(css) = token else { continue };
        let css = css.replace("<!--", "").replace("-->", "");
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let selectors = rest.get(..open).unwrap_or_default();
            let after = rest.get(open + 1..).unwrap_or_default();
            let close = after.find('}').unwrap_or(after.len());
            let decls = declarations(after.get(..close).unwrap_or_default());
            for selector in selectors.split(',') {
                let selector = selector.trim().rsplit(char::is_whitespace).next().unwrap_or_default().to_ascii_lowercase();
                let key = match selector.as_str() {
                    "td" | "th" => Some(String::new()),
                    s => s.strip_prefix("td.").or_else(|| s.strip_prefix("th.")).or_else(|| s.strip_prefix('.')).map(str::to_string),
                };
                if let Some(key) = key {
                    rules.entry(key).or_default().extend(decls.iter().cloned());
                }
            }
            rest = after.get(close + 1..).unwrap_or_default();
        }
    }
    rules
}

/// Properties a cell takes from its table and row: the inherited ones, and a row's background.
fn inheritable(prop: &str, from: &str) -> bool {
    prop.starts_with("font")
        || matches!(prop, "color" | "text-align" | "vertical-align" | "white-space" | "text-decoration" | "text-decoration-line")
        || (from == "tr" && matches!(prop, "background" | "background-color"))
}

#[derive(Default)]
struct Reader {
    classes: HashMap<String, Decls>,
    depth: u32,
    table: Decls,
    row_decls: Decls,
    /// Rows started, and the next free column in the current one.
    row: Option<u32>,
    col: u32,
    /// Slots taken by cells spanning down from earlier rows.
    taken: std::collections::HashSet<(u32, u32)>,
    cell: Option<Open>,
    cells: Vec<HtmlCell>,
    widths: Vec<Option<f32>>,
    /// Widths from `<col>`, which come before the cells' own.
    col_widths: Vec<Option<f32>>,
    next_col: u32,
    full: bool,
}

/// The cell being read.
struct Open {
    cell: HtmlCell,
    decls: Decls,
    text: Text,
}

impl Reader {
    /// `attrs`' declarations after those of the element's classes, over the inheritable ones of
    /// `parent`.
    fn inherited(&self, parent: &[(String, String)], attrs: &[(String, String)], element: &str) -> Decls {
        let mut decls: Decls = parent.to_vec();
        let own = self.own(attrs, element);
        decls.extend(own.into_iter().filter(|(k, _)| element == "td" || inheritable(k, element)));
        decls
    }

    fn own(&self, attrs: &[(String, String)], element: &str) -> Decls {
        let mut decls = Decls::new();
        if element == "td" {
            decls.extend(self.classes.get("").cloned().unwrap_or_default());
        }
        for class in attr(attrs, "class").unwrap_or_default().split_whitespace() {
            decls.extend(self.classes.get(&class.to_ascii_lowercase()).cloned().unwrap_or_default());
        }
        // Presentational attributes, which inline styles override.
        if let Some(align) = attr(attrs, "align") {
            decls.push(("text-align".into(), align.to_string()));
        }
        if let Some(valign) = attr(attrs, "valign") {
            decls.push(("vertical-align".into(), valign.to_string()));
        }
        if let Some(bg) = attr(attrs, "bgcolor") {
            decls.push(("background-color".into(), bg.to_string()));
        }
        decls.extend(declarations(attr(attrs, "style").unwrap_or_default()));
        decls
    }

    /// A tag or text of the table itself (not of a table inside one of its cells).
    fn structure(&mut self, token: &Token) {
        match token {
            Token::Open { name, attrs } => match name.as_str() {
                "tr" => {
                    self.end_cell();
                    self.row = Some(self.row.map_or(0, |r| r.saturating_add(1)));
                    self.col = 0;
                    let table = self.table.clone();
                    self.row_decls = self.inherited(&table, attrs, "tr");
                }
                "td" | "th" => {
                    self.end_cell();
                    if self.full {
                        return;
                    }
                    let row = match self.row {
                        Some(row) => row,
                        None => {
                            // A cell before any row starts one.
                            self.row = Some(0);
                            0
                        }
                    };
                    while self.taken.contains(&(row, self.col)) {
                        self.col = self.col.saturating_add(1);
                    }
                    let span = |key| attr(attrs, key).and_then(|v| v.trim().parse::<u32>().ok()).filter(|n| *n > 0).unwrap_or(1).min(MAX_SPAN);
                    let cols = span("colspan");
                    // A cell claims at most MAX_AREA slots, and the table at most MAX_CELLS.
                    let rows = span("rowspan").min((MAX_AREA / cols).max(1));
                    if self.taken.len() > MAX_CELLS {
                        self.full = true;
                        return;
                    }
                    let mut decls = self.row_decls.clone();
                    if name == "th" {
                        decls.push(("font-weight".into(), "bold".into()));
                        decls.push(("text-align".into(), "center".into()));
                    }
                    decls.extend(self.own(attrs, "td"));
                    if cols == 1 {
                        let width = attr(attrs, "width")
                            .and_then(length)
                            .or_else(|| decls.iter().rev().find(|(k, _)| k == "width").and_then(|(_, v)| length(v)));
                        if let Some(width) = width {
                            set_width(&mut self.widths, self.col, width);
                        }
                    }
                    for r in row..row.saturating_add(rows) {
                        for c in self.col..self.col.saturating_add(cols) {
                            if r != row || c != self.col {
                                self.taken.insert((r, c));
                            }
                        }
                    }
                    let pre = decls.iter().rev().find(|(k, _)| k == "white-space").is_some_and(|(_, v)| v.starts_with("pre") || v == "break-spaces");
                    self.cell = Some(Open { cell: HtmlCell { row, col: self.col, rows, cols, ..HtmlCell::default() }, decls, text: Text::new(pre) });
                    self.col = self.col.saturating_add(cols);
                    if self.cells.len() + 1 >= MAX_CELLS {
                        self.full = true;
                    }
                }
                "col" => {
                    let span = attr(attrs, "span").and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(1).clamp(1, MAX_SPAN);
                    let width = attr(attrs, "width").and_then(length).or_else(|| {
                        declarations(attr(attrs, "style").unwrap_or_default())
                            .into_iter()
                            .rev()
                            .find(|(k, _)| k == "width")
                            .and_then(|(_, v)| length(&v))
                    });
                    for _ in 0..span {
                        if let Some(width) = width {
                            set_width(&mut self.col_widths, self.next_col, width);
                        }
                        self.next_col = self.next_col.saturating_add(1);
                    }
                }
                _ => self.inner(token),
            },
            Token::Close(name) if matches!(name.as_str(), "td" | "th" | "tr") => self.end_cell(),
            _ => self.inner(token),
        }
    }

    /// Content of the current cell.
    fn inner(&mut self, token: &Token) {
        let Some(open) = self.cell.as_mut() else { return };
        open.text.token(token);
    }

    fn end_cell(&mut self) {
        let Some(Open { mut cell, decls, text }) = self.cell.take() else { return };
        let (text, inline) = text.finish();
        cell.look = look(&decls, &inline);
        if text.contains('\n') {
            cell.look.wrap = Some(true);
        }
        cell.text = text;
        self.cells.push(cell);
    }

    fn finish(mut self) -> HtmlTable {
        let rows = self.cells.iter().map(|c| c.row.saturating_add(c.rows)).max().unwrap_or(0);
        let cols = self.cells.iter().map(|c| c.col.saturating_add(c.cols)).max().unwrap_or(0);
        let mut widths = std::mem::take(&mut self.col_widths);
        for (i, w) in self.widths.iter().enumerate() {
            if widths.get(i).copied().flatten().is_none() {
                set_width_at(&mut widths, i, *w);
            }
        }
        widths.truncate(cols as usize);
        HtmlTable { cells: self.cells, rows, cols, widths }
    }
}

fn set_width(widths: &mut Vec<Option<f32>>, col: u32, width: f32) {
    if widths.get(col as usize).copied().flatten().is_none() {
        set_width_at(widths, col as usize, Some(width));
    }
}

fn set_width_at(widths: &mut Vec<Option<f32>>, i: usize, width: Option<f32>) {
    if i >= MAX_SPAN as usize * 16 {
        return;
    }
    if widths.len() <= i {
        widths.resize(i + 1, None);
    }
    if let Some(slot) = widths.get_mut(i) {
        *slot = width;
    }
}

/// A CSS length (or a bare number of pixels) in pixels.
fn length(v: &str) -> Option<f32> {
    let v = v.trim().to_ascii_lowercase();
    let (number, unit) = v.split_at(v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(v.len()));
    let n: f32 = number.parse().ok()?;
    let px = match unit.trim() {
        "" | "px" => n,
        "pt" => n * 96.0 / 72.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        "em" | "rem" => n * 16.0,
        _ => return None,
    };
    (px.is_finite() && px >= 0.0).then_some(px)
}

/// The text of a cell, and the inline formatting every visible part of it shares.
struct Text {
    out: String,
    pre: bool,
    /// The inline formatting open around the text now (`<b>`, `<span style>`, …).
    stack: Vec<(String, Decls)>,
    /// The formatting of each visible piece of text.
    pieces: Vec<Decls>,
}

impl Text {
    fn new(pre: bool) -> Text {
        Text { out: String::new(), pre, stack: Vec::new(), pieces: Vec::new() }
    }

    fn token(&mut self, token: &Token) {
        match token {
            Token::Text(raw) => {
                let text = decode_entities(raw);
                for c in text.chars() {
                    match c {
                        '\r' => {}
                        '\n' if self.pre => self.out.push('\n'),
                        c if c.is_whitespace() && c != '\u{a0}' && !self.pre => {
                            if !self.out.is_empty() && !self.out.ends_with([' ', '\n']) {
                                self.out.push(' ');
                            }
                        }
                        '\u{a0}' => self.out.push(' '),
                        c => self.out.push(c),
                    }
                }
                if text.chars().any(|c| !c.is_whitespace()) {
                    self.pieces.push(self.stack.iter().flat_map(|(_, d)| d.iter().cloned()).collect());
                }
            }
            Token::Open { name, attrs } => match name.as_str() {
                "br" => self.line(true),
                "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.line(false),
                "img" | "hr" | "input" | "meta" | "link" | "wbr" => {}
                _ => {
                    let mut decls = match name.as_str() {
                        "b" | "strong" => vec![("font-weight".to_string(), "bold".to_string())],
                        "i" | "em" => vec![("font-style".into(), "italic".into())],
                        "u" | "ins" => vec![("text-decoration".into(), "underline".into())],
                        "s" | "strike" | "del" => vec![("text-decoration".into(), "line-through".into())],
                        _ => Vec::new(),
                    };
                    if name == "font" {
                        if let Some(color) = attr(attrs, "color") {
                            decls.push(("color".into(), color.to_string()));
                        }
                        if let Some(face) = attr(attrs, "face") {
                            decls.push(("font-family".into(), face.to_string()));
                        }
                    }
                    decls.extend(declarations(attr(attrs, "style").unwrap_or_default()));
                    self.stack.push((name.clone(), decls));
                }
            },
            Token::Close(name) => {
                if matches!(name.as_str(), "p" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                    self.line(false);
                } else if let Some(at) = self.stack.iter().rposition(|(n, _)| n == name) {
                    self.stack.truncate(at);
                }
            }
            Token::Style(_) => {}
        }
    }

    /// A line break: `<br>` always; a block only between pieces of text.
    fn line(&mut self, hard: bool) {
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        if hard || (!self.out.is_empty() && !self.out.ends_with('\n')) {
            self.out.push('\n');
        }
    }

    fn finish(self) -> (String, Decls) {
        let lines: Vec<&str> = self.out.split('\n').map(|l| if self.pre { l } else { l.trim_matches(' ') }).collect();
        let text = lines.join("\n");
        let text = text.trim_matches('\n').to_string();
        // Formatting every piece has, so the cell takes it.
        let mut shared = Decls::new();
        if let Some((first, rest)) = self.pieces.split_first() {
            let first_look = look(first, &[]);
            let mut common = first_look.clone();
            for piece in rest {
                let other = look(piece, &[]);
                common = intersect(common, &other);
            }
            shared = look_decls(&common);
        }
        (text, shared)
    }
}

/// What both looks set alike.
fn intersect(a: Look, b: &Look) -> Look {
    fn same<T: PartialEq + Clone>(a: Option<T>, b: &Option<T>) -> Option<T> {
        if a.as_ref() == b.as_ref() { a } else { None }
    }
    Look {
        bold: same(a.bold, &b.bold),
        italic: same(a.italic, &b.italic),
        underline: same(a.underline, &b.underline),
        strike: same(a.strike, &b.strike),
        color: same(a.color, &b.color),
        fill: None,
        size: same(a.size, &b.size),
        font: same(a.font, &b.font),
        h: None,
        v: None,
        wrap: None,
        borders: [None; 4],
    }
}

/// The inline look as declarations the cell's own come before.
fn look_decls(look: &Look) -> Decls {
    let mut d = Decls::new();
    if let Some(bold) = look.bold {
        d.push(("font-weight".into(), if bold { "bold" } else { "normal" }.into()));
    }
    if let Some(italic) = look.italic {
        d.push(("font-style".into(), if italic { "italic" } else { "normal" }.into()));
    }
    let lines: Vec<&str> =
        [(look.underline, "underline"), (look.strike, "line-through")].iter().filter(|(on, _)| *on == Some(true)).map(|(_, l)| *l).collect();
    if !lines.is_empty() {
        d.push(("text-decoration".into(), lines.join(" ")));
    }
    if let Some(Color::Rgb(rgb)) = look.color {
        d.push(("color".into(), format!("#{rgb:06x}")));
    }
    if let Some(size) = look.size {
        d.push(("font-size".into(), format!("{size}pt")));
    }
    if let Some(font) = &look.font {
        d.push(("font-family".into(), font.clone()));
    }
    d
}

/// A cell's look from its declarations (later ones win), then the inline formatting its whole
/// text shares.
fn look(decls: &[(String, String)], inline: &[(String, String)]) -> Look {
    let mut look = Look::default();
    for (k, v) in decls.iter().chain(inline) {
        let lower = v.to_ascii_lowercase();
        match k.as_str() {
            "font-weight" => look.bold = Some(lower == "bold" || lower == "bolder" || lower.parse::<u32>().is_ok_and(|w| w >= 600)),
            "font-style" => look.italic = Some(lower == "italic" || lower == "oblique"),
            "text-decoration" | "text-decoration-line" => {
                look.underline = Some(lower.contains("underline"));
                look.strike = Some(lower.contains("line-through"));
            }
            "color" => look.color = css_color(&lower),
            "background" | "background-color" => look.fill = css_color(&lower).or_else(|| lower.split_whitespace().find_map(css_color)),
            "font-size" => {
                if let Some(px) = length(&lower) {
                    look.size = Some((px * 72.0 / 96.0 * 2.0).round() / 2.0).filter(|s| (1.0..=409.0).contains(s));
                }
            }
            "font-family" => {
                look.font = v.split(',').map(|f| f.trim().trim_matches(['"', '\'']).to_string()).find(|f| {
                    !f.is_empty()
                        && !matches!(
                            f.to_ascii_lowercase().as_str(),
                            "sans-serif" | "serif" | "monospace" | "system-ui" | "-apple-system" | "blinkmacsystemfont" | "inherit" | "initial"
                        )
                })
            }
            "text-align" => {
                look.h = match lower.as_str() {
                    "left" | "start" => Some(HAlign::Left),
                    "center" | "middle" => Some(HAlign::Center),
                    "right" | "end" => Some(HAlign::Right),
                    "justify" => Some(HAlign::Justify),
                    _ => None,
                }
            }
            "vertical-align" => {
                look.v = match lower.as_str() {
                    "top" | "text-top" => Some(VAlign::Top),
                    "middle" | "center" => Some(VAlign::Center),
                    "bottom" | "text-bottom" | "baseline" => Some(VAlign::Bottom),
                    _ => None,
                }
            }
            "white-space" => look.wrap = Some(lower == "normal" || lower == "pre-wrap" || lower == "pre-line" || lower == "break-spaces"),
            "border" => look.borders = [border(&lower); 4],
            "border-left" => look.borders[0] = border(&lower),
            "border-right" => look.borders[1] = border(&lower),
            "border-top" => look.borders[2] = border(&lower),
            "border-bottom" => look.borders[3] = border(&lower),
            _ => {}
        }
    }
    look
}

/// A `border` shorthand (`1px solid #000`, `.5pt solid windowtext`, `none`).
fn border(v: &str) -> Option<BorderLine> {
    let mut width = None;
    let mut style = None;
    let mut color = Color::Auto;
    for part in v.split_whitespace() {
        match part {
            "none" | "hidden" => return None,
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset" => style = Some(part),
            "thin" => width = Some(1.0),
            "medium" => width = Some(2.0),
            "thick" => width = Some(3.0),
            _ => {
                if let Some(px) = length(part) {
                    width = Some(px);
                } else if let Some(c) = css_color(part) {
                    color = c;
                }
            }
        }
    }
    let width = width.unwrap_or(1.0);
    if width <= 0.0 {
        return None;
    }
    let style = match (style?, width) {
        ("dashed", w) if w > 1.5 => BorderStyle::MediumDashed,
        ("dashed", _) => BorderStyle::Dashed,
        ("dotted", _) => BorderStyle::Dotted,
        ("double", _) => BorderStyle::Double,
        (_, w) if w > 2.5 => BorderStyle::Thick,
        (_, w) if w > 1.2 => BorderStyle::Medium,
        _ => BorderStyle::Thin,
    };
    Some(BorderLine { style, color })
}

/// A CSS colour; `None` for transparent, the system's text colour, and what it cannot read.
fn css_color(v: &str) -> Option<Color> {
    let v = v.trim().trim_end_matches(';');
    if let Some(hex) = v.strip_prefix('#') {
        let hex: String = match hex.len() {
            3 | 4 => hex.chars().take(3).flat_map(|c| [c, c]).collect(),
            6 | 8 => hex.chars().take(6).collect(),
            _ => return None,
        };
        return u32::from_str_radix(&hex, 16).ok().map(Color::Rgb);
    }
    if let Some(args) = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb(")) {
        let parts: Vec<f32> = args
            .trim_end_matches(')')
            .split([',', ' ', '/'])
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.trim_end_matches('%').parse().ok())
            .collect();
        if parts.get(3).is_some_and(|a| *a == 0.0) {
            return None;
        }
        let channel = |i: usize| parts.get(i).map(|c| c.clamp(0.0, 255.0) as u8);
        return Some(Color::rgb(channel(0)?, channel(1)?, channel(2)?));
    }
    let rgb = match v {
        "black" => 0x000000,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "gray" | "grey" => 0x808080,
        "silver" => 0xc0c0c0,
        "maroon" => 0x800000,
        "navy" => 0x000080,
        "purple" => 0x800080,
        "teal" => 0x008080,
        "olive" => 0x808000,
        "lime" => 0x00ff00,
        "aqua" | "cyan" => 0x00ffff,
        "fuchsia" | "magenta" => 0xff00ff,
        "orange" => 0xffa500,
        _ => return None,
    };
    Some(Color::Rgb(rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(t: &HtmlTable, row: u32, col: u32) -> &HtmlCell {
        t.cells.iter().find(|c| c.row == row && c.col == col).unwrap_or_else(|| panic!("no cell at {row},{col}"))
    }

    #[test]
    fn places_cells_after_merges_and_keeps_line_breaks() {
        let t = parse(
            "<meta charset='utf-8'><table><colgroup><col width=\"120\"><col style=\"width:75pt\"></colgroup>\
             <tr><td colspan=\"2\">Size</td><td>x</td></tr>\
             <tr><td rowspan=\"2\">Ops</td><td>1. close<br>2. open</td><td>a</td></tr>\
             <tr><td><div>load</div><div>files</div></td><td>b</td></tr></table>",
        )
        .unwrap();
        assert_eq!((t.rows, t.cols), (3, 3));
        assert_eq!((cell(&t, 0, 0).text.as_str(), cell(&t, 0, 0).cols), ("Size", 2));
        assert_eq!(cell(&t, 0, 2).text, "x");
        assert_eq!(cell(&t, 1, 0).rows, 2);
        assert_eq!(cell(&t, 1, 1).text, "1. close\n2. open");
        assert_eq!(cell(&t, 1, 1).look.wrap, Some(true));
        assert_eq!(cell(&t, 2, 1).text, "load\nfiles", "the rowspan above moves this cell to column B");
        assert_eq!(cell(&t, 2, 2).text, "b");
        assert_eq!(t.widths, vec![Some(120.0), Some(100.0)]);
    }

    #[test]
    fn reads_styles_from_classes_inline_and_inherited() {
        let t = parse(
            "<style><!--td {white-space:nowrap} .xl65 {font-weight:700; background:#E8E6F4; mso-pattern:black none; border:.5pt solid windowtext}\
             .xl66 {text-align:center; border-bottom:1.5pt solid #000000; vertical-align:middle}--></style>\
             <table style=\"font-size:14px;color:#333333\"><tr style=\"background-color:rgb(255, 255, 0)\">\
             <td class=xl65>Head</td><td class=\"xl66\" style=\"font-size:16pt\"><b>Bold</b> <b>too</b></td><td><i>half</i> plain</td></tr></table>",
        )
        .unwrap();
        let head = &cell(&t, 0, 0).look;
        assert_eq!(head.bold, Some(true));
        assert_eq!(head.fill, Some(Color::Rgb(0xe8e6f4)), "the class's background over the row's");
        assert_eq!(head.borders, [Some(BorderLine { style: BorderStyle::Thin, color: Color::Auto }); 4]);
        assert_eq!(head.color, Some(Color::Rgb(0x333333)));
        assert_eq!(head.size, Some(10.5));
        assert_eq!(head.wrap, Some(false));
        let second = &cell(&t, 0, 1).look;
        assert_eq!((second.h, second.v, second.size, second.bold), (Some(HAlign::Center), Some(VAlign::Center), Some(16.0), Some(true)));
        assert_eq!(second.fill, Some(Color::Rgb(0xffff00)));
        assert_eq!(second.borders[3], Some(BorderLine { style: BorderStyle::Medium, color: Color::Rgb(0) }));
        assert_eq!(second.borders[0], None);
        assert_eq!(cell(&t, 0, 2).look.italic, None, "only part of the text is italic");
    }

    #[test]
    fn decodes_entities_and_collapses_source_whitespace() {
        let t =
            parse("<table><tr><td>\n  A &amp; B&nbsp;&lt;3&#x4E2D;&#25991;\n  </td><td style='white-space:pre'>a  b\nc</td></tr></table>").unwrap();
        assert_eq!(cell(&t, 0, 0).text, "A & B <3\u{4e2d}\u{6587}");
        assert_eq!(cell(&t, 0, 1).text, "a  b\nc");
    }

    #[test]
    fn reads_only_the_first_table_and_none_without_one() {
        assert!(parse("<p>no table here</p>").is_none());
        let t = parse("<table><tr><td>one<table><tr><td>inner</td></tr></table></td><td>two</td></tr></table><table><tr><td>later</td></tr></table>")
            .unwrap();
        assert_eq!(t.cols, 2);
        assert_eq!(cell(&t, 0, 1).text, "two");
        assert!(t.cells.iter().all(|c| c.text != "later"));
    }

    #[test]
    fn survives_broken_html() {
        for html in [
            "<table",
            "<table><tr><td colspan=99999999>x",
            "<table><td rowspan=-1>a</td>&#xffffffff;&",
            "<table><tr><td style='border:1px'><<</td></tr>",
            "<table><col span=0 width=abc>",
        ] {
            let _ = parse(html);
        }
        let t = parse("<table><tr><td colspan=99999999>x").unwrap();
        assert_eq!(t.cols, MAX_SPAN);
    }
}
