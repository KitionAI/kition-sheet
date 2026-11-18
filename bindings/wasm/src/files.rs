//! The parameters of this kit's commands that name files or folders on disk (KHP A28), declared
//! as typed files in the commands' schemas (`kition_kit::files`): a host lends them, and a
//! headless kit keeps those commands from agents (the host opens, saves and exports documents).

use kition_kit::files::FileParam;

/// Every file or folder parameter of the kit's commands.
pub const FILES: &[FileParam] = &[
    FileParam::read("file.open", "path", &[]),
    FileParam::write("file.save", "path", &[]),
    FileParam::create("file.saveAs", "path", &[]),
    FileParam::create("file.exportCsv", "path", &[]),
    FileParam::create("file.exportHtml", "path", &[]),
    FileParam::read("insert.picture", "path", &[]),
    FileParam::read("data.fromTextCsv", "path", &[]),
    FileParam::read("data.getData", "path", &[]),
    FileParam::read("pageLayout.background", "path", &[]),
    FileParam::create("file.exportPdf", "path", &[]),
    FileParam::create("file.print", "path", &[]),
];

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;

    use super::*;

    /// Parameters whose names read like files but are not: ids, property paths, directions,
    /// folders of the kit's own panels, in-memory data.
    const NOT_FILES: &[(&str, &str)] = &[("drawing.erase", "path")];

    #[test]
    fn every_file_parameter_is_declared() {
        let doc = crate::SheetDocument::create("Untitled.xlsx", 64, 64).unwrap_or_else(|error| panic!("{error}"));
        let problems = kition_kit::files::check(&doc.commands(), FILES, NOT_FILES);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
