//! Checkbox cells (Excel's cell controls, module M8): written as cell formats whose complement
//! names a Checkbox feature property bag, read back from Excel's layout (bags in any order), and
//! not made up from other files (a complement without the bags, a form control's checkbox).

use std::io::Read;

use kition_sheet_core::{CellRef, Value};
use kition_sheet_model::*;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn part(bytes: &[u8], name: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut file = zip.by_name(name).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).unwrap();
    Some(text)
}

fn checkbox(wb: &Workbook, cell: &str) -> bool {
    let sheet = wb.sheet(0).unwrap();
    wb.styles.get(sheet.style_id(at(cell))).checkbox
}

#[test]
fn checkboxes_round_trip_as_excels_cell_controls() {
    let mut wb = Workbook::new();
    let boxed = wb.styles.derive(StyleId::DEFAULT, |s| {
        s.checkbox = true;
        s.align.h = HAlign::Center;
    });
    let red = wb.styles.derive(boxed, |s| s.font.color = Color::Rgb(0xFF0000));
    let sheet = wb.sheet_mut(0).unwrap();
    sheet.set_cell(at("A1"), Cell { value: Value::Bool(true), formula: None, style: boxed, rich: None });
    sheet.set_cell(at("A2"), Cell { value: Value::Bool(false), formula: None, style: boxed, rich: None });
    sheet.set_cell(at("A3"), Cell { value: Value::Bool(true), formula: None, style: red, rich: None });
    sheet.set_cell(at("B1"), Cell::value(Value::Bool(true)));
    let bytes = write_xlsx(&wb).unwrap();

    let bags = part(&bytes, "xl/featurePropertyBag/featurePropertyBag.xml").expect("the feature property bags are written");
    assert!(bags.contains(r#"<bag type="Checkbox"/>"#), "{bags}");
    let styles = part(&bytes, "xl/styles.xml").unwrap();
    assert!(styles.contains("<xfpb:xfComplement i=\"0\"/>"), "{styles}");
    let rels = part(&bytes, "xl/_rels/workbook.xml.rels").unwrap();
    assert!(rels.contains("relationships/FeaturePropertyBag") && rels.contains("featurePropertyBag/featurePropertyBag.xml"), "{rels}");
    let types = part(&bytes, "[Content_Types].xml").unwrap();
    assert!(types.contains("application/vnd.ms-excel.featurepropertybag+xml"), "{types}");
    let sheet = part(&bytes, "xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains(r#"<c r="A1" s="#) && sheet.contains(r#"t="b"><v>1</v>"#), "{sheet}");

    let (back, _) = read_xlsx(&bytes).unwrap();
    for cell in ["A1", "A2", "A3"] {
        assert!(checkbox(&back, cell), "{cell} is a checkbox again");
    }
    assert!(!checkbox(&back, "B1"), "a plain TRUE is not");
    let s = back.sheet(0).unwrap();
    assert_eq!((s.value(at("A1")), s.value(at("A2"))), (Value::Bool(true), Value::Bool(false)));
    assert_eq!(back.styles.get(s.style_id(at("A3"))).font.color, Color::Rgb(0xFF0000), "the rest of the format stays");

    // A workbook without checkboxes writes no bags.
    let plain = write_xlsx(&Workbook::new()).unwrap();
    assert!(part(&plain, "xl/featurePropertyBag/featurePropertyBag.xml").is_none());
}

const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xfpb="http://schemas.microsoft.com/office/spreadsheetml/2022/featurepropertybag"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="0" fontId="0" fillId="0" borderId="0"><extLst><ext uri="{C7286773-470A-42A8-94C5-96B5CB345126}"><xfpb:xfComplement i="1"/></ext></extLst></xf><xf numFmtId="0" fontId="0" fillId="0" borderId="0"><extLst><ext uri="{C7286773-470A-42A8-94C5-96B5CB345126}"><xfpb:xfComplement i="0"/></ext></extLst></xf></cellXfs></styleSheet>"#;
const STYLES_REL: &str =
    r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;
const FPB_REL: &str = r#"<Relationship Id="rId3" Type="http://schemas.microsoft.com/office/2022/11/relationships/FeaturePropertyBag" Target="featurePropertyBag/featurePropertyBag.xml"/>"#;
const CELLS: &str = r#"<sheetData><row r="1"><c r="A1" s="1" t="b"><v>1</v></c><c r="B1" s="2" t="b"><v>0</v></c></row></sheetData>"#;

#[test]
fn reads_excels_bags_in_any_order_and_only_checkboxes() {
    // The list maps complement 0 to a bag that leads nowhere near a checkbox, complement 1 to one
    // leading to the Checkbox bag (written last here).
    let bags = r#"<?xml version="1.0"?><FeaturePropertyBags xmlns="http://schemas.microsoft.com/office/spreadsheetml/2022/featurepropertybag"><bag type="XFComplements" extRef="XFComplementsMapperExtRef"><a k="MappedFeaturePropertyBags"><bagId>4</bagId><bagId>1</bagId></a></bag><bag type="XFComplement"><bagId k="XFControls">2</bagId></bag><bag type="XFControls"><bagId k="CellControl">3</bagId></bag><bag type="Checkbox"/><bag type="SomethingElse"/></FeaturePropertyBags>"#;
    let file =
        minimal(CELLS, &[("xl/styles.xml", STYLES), ("xl/featurePropertyBag/featurePropertyBag.xml", bags)], &format!("{STYLES_REL}{FPB_REL}"), "");
    let (wb, _) = read_xlsx(&file).unwrap();
    assert!(checkbox(&wb, "A1"), "complement 1 leads to the Checkbox bag");
    assert!(!checkbox(&wb, "B1"), "complement 0 does not");

    // A complement without the bags part is not a checkbox.
    let file = minimal(CELLS, &[("xl/styles.xml", STYLES)], STYLES_REL, "");
    let (wb, _) = read_xlsx(&file).unwrap();
    assert!(!checkbox(&wb, "A1") && !checkbox(&wb, "B1"));
    assert_eq!(wb.sheet(0).unwrap().value(at("A1")), Value::Bool(true));
}

/// INSC-C329: a form control's checkbox over A1 (a drawing, not the cell) is left behind; A1 keeps
/// its value and shows no checkbox.
#[test]
fn a_form_controls_checkbox_is_not_a_cell_checkbox() {
    let body = r#"<sheetData><row r="1"><c r="A1" t="b"><v>1</v></c></row></sheetData><legacyDrawing r:id="rId9"/><controls><control shapeId="1025" r:id="rId8" name="Check Box 1"/></controls>"#;
    let ctrl = r#"<?xml version="1.0"?><formControlPr xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" objectType="CheckBox" checked="Checked" fmlaLink="$A$1"/>"#;
    let file = minimal(body, &[("xl/ctrlProps/ctrlProp1.xml", ctrl)], "", "");
    let (wb, _) = read_xlsx(&file).unwrap();
    assert!(!checkbox(&wb, "A1"));
    assert_eq!(wb.sheet(0).unwrap().value(at("A1")), Value::Bool(true));
}

/// Form controls (a check box and a button over the cells) are kept: read with what drawing them
/// needs, and written back (their VML shapes, control elements and properties parts) beside the
/// notes, whose shapes take other ids.
#[test]
fn form_controls_are_kept_on_save() {
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t201" coordsize="21600,21600" o:spt="201" path="m,l,21600r21600,l21600,xe"><v:stroke joinstyle="miter"/></v:shapetype><v:shape id="_x0000_s1025" type="#_x0000_t201" style="position:absolute;margin-left:48pt;margin-top:15pt;width:96pt;height:18pt;z-index:1"><v:textbox><div style="text-align:left"><font face="Calibri" size="220">Agree &amp; sign<br></font></div></v:textbox><x:ClientData ObjectType="Checkbox"><x:Anchor>1, 0, 1, 0, 2, 32, 2, 4</x:Anchor><x:Checked>1</x:Checked><x:FmlaLink>$D$2</x:FmlaLink></x:ClientData></v:shape><v:shape id="_x0000_s1026" type="#_x0000_t201" style="position:absolute"><v:textbox><div>Run</div></v:textbox><x:ClientData ObjectType="Button"><x:Anchor>4, 0, 4, 0, 5, 10, 6, 0</x:Anchor></x:ClientData></v:shape></xml>"##;
    let body = r#"<sheetData><row r="1"><c r="A1" t="b"><v>1</v></c></row></sheetData><legacyDrawing r:id="rId9"/><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="x14"><controls><mc:AlternateContent><mc:Choice Requires="x14"><control shapeId="1025" r:id="rId8" name="Check Box 1"><controlPr defaultSize="0" autoFill="0" autoLine="0" autoPict="0"><anchor moveWithCells="1"><from><xdr:col xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">1</xdr:col></from></anchor></controlPr></control></mc:Choice></mc:AlternateContent></controls></mc:Choice></mc:AlternateContent>"#;
    let ctrl = r#"<?xml version="1.0"?><formControlPr xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" objectType="CheckBox" checked="Checked" fmlaLink="$D$2"/>"#;
    let sheet_rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/ctrlProp" Target="../ctrlProps/ctrlProp1.xml"/><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#;
    let file = minimal(
        body,
        &[("xl/ctrlProps/ctrlProp1.xml", ctrl), ("xl/drawings/vmlDrawing1.vml", vml), ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels)],
        "",
        "",
    );
    let (mut wb, _) = read_xlsx(&file).unwrap();
    let controls = &wb.sheet(0).unwrap().form_controls;
    assert_eq!(controls.len(), 2);
    let check = &controls[0];
    assert_eq!((check.kind.as_str(), check.shape_id, check.checked, check.text.as_str()), ("Checkbox", 1025, true, "Agree & sign"));
    assert_eq!((check.from.0, check.to.0, check.to.1), (at("B2"), at("C3"), 32.0));
    assert!(check.control.as_deref().is_some_and(|c| c.contains("Check Box 1")) && check.props.as_deref().is_some_and(|p| p.contains("CheckBox")));
    assert_eq!((controls[1].kind.as_str(), controls[1].text.as_str(), controls[1].control.is_none()), ("Button", "Run", true));

    // A note beside them: its shape takes another block of ids.
    wb.sheet_mut(0)
        .unwrap()
        .comments
        .insert(at("A1"), Comment { author: "K".into(), text: "note".into(), replies: vec![], threaded: false, resolved: false, visible: false });
    let saved = write_xlsx(&wb).unwrap();
    let sheet = part(&saved, "xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains("<legacyDrawing r:id=") && sheet.contains("<controls>") && sheet.contains("name=\"Check Box 1\""), "{sheet}");
    let rels = part(&saved, "xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    assert!(rels.contains("relationships/ctrlProp") && rels.contains("../ctrlProps/ctrlProp1.xml"), "{rels}");
    let vml_out = part(&saved, "xl/drawings/vmlDrawing1.vml").unwrap();
    assert!(vml_out.contains("_x0000_s1025") && vml_out.contains("_x0000_s2049") && vml_out.contains("data=\"1,2\""), "{vml_out}");
    assert!(part(&saved, "xl/ctrlProps/ctrlProp1.xml").is_some_and(|p| p.contains("CheckBox")));
    let types = part(&saved, "[Content_Types].xml").unwrap();
    assert!(types.contains("application/vnd.ms-excel.controlproperties+xml"));
    let (back, _) = read_xlsx(&saved).unwrap();
    let again = &back.sheet(0).unwrap().form_controls;
    assert_eq!(again.len(), 2);
    assert_eq!((again[0].kind.as_str(), again[0].checked, again[0].text.as_str()), ("Checkbox", true, "Agree & sign"));
    assert_eq!(back.sheet(0).unwrap().comments.len(), 1, "the note is read from its own part");
}
