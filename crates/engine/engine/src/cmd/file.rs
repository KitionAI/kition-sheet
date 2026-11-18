//! File menu: new, open, save, close, export, properties.

use serde_json::{Value as Json, json};

use super::*;
use crate::DocState;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "file.new", "New Workbook", ["File"], Some("Cmd+N"), "{sample?: \"budget\"|\"sales\"|\"grades\"}", always, new_workbook),
        cmd!(noundo "file.open", "Open…", ["File"], Some("Cmd+O"), "{path} | {name, base64} (xlsx, xlsm, xlsb and ods data import, csv, tsv, txt, json)", always, open),
        cmd!(noundo "file.save", "Save", ["File"], Some("Cmd+S"), "{path?} (xlsx by default; .csv/.tsv/.json/.html by extension)", has_doc, save),
        cmd!(noundo "file.saveAs", "Save As…", ["File"], Some("Cmd+Shift+S"), "{path}", has_doc, save_as),
        cmd!(query "file.saveBytes", "Encode Workbook", [], None, "{format?: xlsx|csv|tsv|json|html} → {base64}", has_doc, save_bytes),
        cmd!(noundo "file.close", "Close", ["File"], Some("Cmd+W"), "{force?: bool}", has_doc, close),
        cmd!(noundo "file.exportCsv", "Export as CSV", ["File", "Export"], None, "{path, sheet?, bom?: bool (UTF-8 with a byte-order mark, as Excel's and WPS's CSV UTF-8)}", has_doc, |s, p| export(s, p, "csv")),
        cmd!(noundo "file.exportHtml", "Save as Web Page", ["File", "Export"], None, "{path}", has_doc, |s, p| export(s, p, "html")),
        // The editor's Export buttons and the agent's `sheet_export` (kit.json `agent.export`): the
        // file comes back as base64 and the host writes it as a new file, never the engine.
        cmd!(
            query "file.export",
            "Export",
            [],
            None,
            "{format: pdf|csv|html|xlsx|xltx|png, sheet? (csv, html, png: index or name; the active sheet by default), sheets?: active|all (pdf), range?: \"A1:F40\" (pdf, png: active sheet), fitToPage?: bool (pdf), bom?: bool (csv: UTF-8 with a byte-order mark, as Excel's and WPS's CSV UTF-8)} → {format, extension, mime, bytes, data (base64), pages? (pdf)}; xlsx is a copy of the workbook, xltx the workbook as a template, png a picture of the sheet's cells (the kit's renderer)",
            has_doc,
            export_file
        ),
        cmd!("file.properties", "Properties", ["File"], None, "{title?, subject?, author?, company?, keywords?, description?}", has_doc, properties),
        cmd!(noundo "window.activate", "Switch Window", ["Window"], None, "{index}", has_doc, activate_window),
    ]
}

fn new_workbook(s: &mut Session, p: &Json) -> Result<Json> {
    let i = match str_param(p, "sample") {
        Some(name) => {
            let wb = crate::sample::build(name).ok_or_else(|| bad("file.new", format!("unknown sample `{name}`")))?;
            let title = crate::sample::title(name);
            s.add_document(DocState::new(wb, None, title))
        }
        None => s.new_workbook(),
    };
    Ok(json!({"index": i, "title": s.doc()?.display_title()}))
}

fn open(s: &mut Session, p: &Json) -> Result<Json> {
    let (name, bytes) = if let Some(b) = str_param(p, "base64") {
        (str_param(p, "name").unwrap_or("Book.xlsx").to_string(), crate::io::base64_decode(b).ok_or_else(|| bad("file.open", "invalid base64"))?)
    } else if let Some(path) = str_param(p, "path") {
        // Already open? Switch to it.
        if let Some(i) = s.documents().iter().position(|d| d.path.as_deref() == Some(path)) {
            s.set_active(i);
            return Ok(json!({"index": i, "alreadyOpen": true}));
        }
        (path.to_string(), s.read_path(path)?)
    } else {
        s.ui_requests.push(crate::UiRequest::Dialog("open".into(), json!({})));
        return ok();
    };
    let (wb, warnings) = crate::io::open_bytes(&name, &bytes)?;
    let imported =
        matches!(crate::io::FileKind::from_path(&name), Some(crate::io::FileKind::Ods | crate::io::FileKind::Xlsb | crate::io::FileKind::Xls))
            || matches!(
                kition_sheet_xlsx::sniff(&bytes),
                kition_sheet_xlsx::Format::Ods | kition_sheet_xlsx::Format::Xlsb | kition_sheet_xlsx::Format::Xls
            );
    let title = if imported {
        format!("{} (imported).xlsx", std::path::Path::new(&name).file_stem().and_then(|n| n.to_str()).unwrap_or("Book"))
    } else {
        std::path::Path::new(&name).file_name().and_then(|n| n.to_str()).unwrap_or("Book").to_string()
    };
    // An import has no save target: ordinary Save must ask for a new destination.
    let path = str_param(p, "path").filter(|_| !imported).map(str::to_string);
    // Replace an untouched blank Book1 like Excel does.
    if s.documents().len() == 1
        && s.active().is_some_and(|d| d.path.is_none() && !d.is_dirty() && d.undo.is_empty() && d.wb.sheets.iter().all(|sh| sh.cells.is_empty()))
    {
        s.close_document(0);
    }
    let i = s.add_document(DocState::new(wb, path, title));
    if !warnings.is_empty() {
        let mut message = warnings.iter().take(5).cloned().collect::<Vec<_>>().join("\n\n");
        if warnings.len() > 5 {
            message.push_str(&format!("\n\n{} additional import warnings.", warnings.len() - 5));
        }
        s.ui_requests.push(crate::UiRequest::Message(message));
    }
    let d = s.doc()?;
    Ok(
        json!({"index": i, "title": d.display_title(), "sheets": d.wb.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), "warnings": warnings}),
    )
}

fn save(s: &mut Session, p: &Json) -> Result<Json> {
    let path = match str_param(p, "path").map(str::to_string).or_else(|| s.active().and_then(|d| d.path.clone())) {
        Some(p) => p,
        None => {
            s.ui_requests.push(crate::UiRequest::Dialog("saveAs".into(), json!({})));
            return ok();
        }
    };
    let bytes = crate::io::save_bytes(&s.doc()?.wb, &path)?;
    s.write_path(&path, &bytes)?;
    let kind = crate::io::FileKind::from_path(&path).unwrap_or(crate::io::FileKind::Xlsx);
    // Only a full-fidelity format clears the dirty flag and becomes the document's file.
    if matches!(kind, crate::io::FileKind::Xlsx | crate::io::FileKind::Json) {
        let d = s.doc_mut()?;
        d.saved = d.wb.clone();
        d.path = Some(path.clone());
    }
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn save_as(s: &mut Session, p: &Json) -> Result<Json> {
    if str_param(p, "path").is_none() {
        s.ui_requests.push(crate::UiRequest::Dialog("saveAs".into(), json!({})));
        return ok();
    }
    save(s, p)
}

fn save_bytes(s: &mut Session, p: &Json) -> Result<Json> {
    let fmt = str_param(p, "format").unwrap_or("xlsx");
    let d = s.doc()?;
    let bytes = crate::io::save_bytes(&d.wb, &format!("x.{fmt}"))?;
    let name = format!("{}.{fmt}", std::path::Path::new(&d.display_title()).file_stem().and_then(|x| x.to_str()).unwrap_or("Book"));
    Ok(json!({"name": name, "base64": crate::io::base64_encode(&bytes), "bytes": bytes.len()}))
}

fn close(s: &mut Session, p: &Json) -> Result<Json> {
    let force = bool_param(p, "force").unwrap_or(false);
    let d = s.doc()?;
    if d.is_dirty() && !force {
        s.ui_requests.push(crate::UiRequest::Dialog("saveChanges".into(), json!({"title": d.display_title()})));
        return Ok(json!({"closed": false, "dirty": true}));
    }
    let i = s.active_index();
    s.close_document(i);
    Ok(json!({"closed": true}))
}

fn export(s: &mut Session, p: &Json, ext: &str) -> Result<Json> {
    let path = str_param(p, "path").ok_or_else(|| bad("file.export", "missing `path`"))?.to_string();
    let d = s.doc()?;
    let mut wb = (*d.wb).clone();
    if p.get("sheet").is_some() {
        wb.active_sheet = target_sheet(s, p)?;
    }
    let target = if path.ends_with(&format!(".{ext}")) { path.clone() } else { format!("{path}.{ext}") };
    let bytes = with_bom(crate::io::save_bytes(&wb, &target)?, ext == "csv" && bool_param(p, "bom") == Some(true));
    s.write_path(&target, &bytes)?;
    Ok(json!({"path": target, "bytes": bytes.len()}))
}

/// `file.export`: the workbook as PDF (as Print lays it out), or one sheet as CSV or a web page,
/// answered as base64 (`data`) without writing a file or changing the workbook.
fn export_file(s: &mut Session, p: &Json) -> Result<Json> {
    let format =
        str_param(p, "format").ok_or_else(|| bad("file.export", "missing `format` (pdf, csv, html, xlsx, xltx or png)"))?.to_ascii_lowercase();
    let (bytes, mime, pages) = match format.as_str() {
        "pdf" => {
            let (bytes, pages) = super::print::build_pdf(s, p, "file.export")?;
            (bytes, "application/pdf", Some(pages))
        }
        "csv" | "html" => {
            let d = s.doc()?;
            let mut wb = (*d.wb).clone();
            wb.active_sheet = target_sheet(s, p)?;
            let mime = if format == "csv" { "text/csv" } else { "text/html" };
            let bytes = with_bom(crate::io::save_bytes(&wb, &format!("x.{format}"))?, format == "csv" && bool_param(p, "bom") == Some(true));
            (bytes, mime, None)
        }
        // The whole workbook as a copy (an .xls or .csv opened here becomes an Excel workbook),
        // or as a template (WPS's 保存为模板 / Save As › Excel Template).
        "xlsx" | "xltx" => {
            let d = s.doc()?;
            let mime = if format == "xlsx" {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            } else {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.template"
            };
            (crate::io::save_bytes(&d.wb, &format!("x.{format}"))?, mime, None)
        }
        other => return Err(bad("file.export", format!("`format` must be pdf, csv or html (or xlsx, xltx, png), not `{other}`"))),
    };
    let mut out = json!({"format": format, "extension": format, "mime": mime, "bytes": bytes.len(), "data": crate::io::base64_encode(&bytes)});
    if let Some(pages) = pages {
        out["pages"] = json!(pages);
    }
    Ok(out)
}

/// CSV bytes with a UTF-8 byte-order mark in front when `bom` (what Excel for Windows needs to read
/// UTF-8 rather than the system's code page).
fn with_bom(bytes: Vec<u8>, bom: bool) -> Vec<u8> {
    if !bom || bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return bytes;
    }
    let mut out = Vec::with_capacity(bytes.len() + 3);
    out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    out.extend_from_slice(&bytes);
    out
}

fn properties(s: &mut Session, p: &Json) -> Result<Json> {
    edit(s, |cx| {
        let pr = &mut cx.wb.props;
        for (k, slot) in [
            ("title", &mut pr.title),
            ("subject", &mut pr.subject),
            ("author", &mut pr.author),
            ("company", &mut pr.company),
            ("keywords", &mut pr.keywords),
            ("description", &mut pr.description),
        ] {
            if let Some(v) = str_param(p, k) {
                *slot = v.to_string();
            }
        }
        Ok(serde_json::to_value(&cx.wb.props).unwrap_or_default())
    })
}

fn activate_window(s: &mut Session, p: &Json) -> Result<Json> {
    let i = u32_param(p, "index").ok_or_else(|| bad("window.activate", "missing `index`"))? as usize;
    if i >= s.documents().len() {
        return Err(bad("window.activate", "no such window"));
    }
    s.set_active(i);
    ok()
}
