//! Smart graphics on synthetic packages: the frame's parts, the drawing its data part names
//! and the drawing's pictures are kept, written back under new names and read again.

use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const NS: &str = r#"xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(crate) const DATA: &str = r#"<?xml version="1.0"?><dgm:dataModel xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dgm:ptLst><dgm:pt modelId="{1}" type="doc"/><dgm:pt modelId="{2}"><dgm:t><a:bodyPr/><a:p><a:r><a:t>Plan</a:t></a:r></a:p></dgm:t></dgm:pt></dgm:ptLst><dgm:extLst><a:ext uri="http://schemas.microsoft.com/office/drawing/2008/diagram"><dsp:dataModelExt xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" relId="rId9" minVer="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:ext></dgm:extLst></dgm:dataModel>"#;
pub(crate) const DRAWING: &str = r#"<?xml version="1.0"?><dsp:drawing xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><dsp:spTree><dsp:sp modelId="{3}"><dsp:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></dsp:spPr><dsp:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="2000"/><a:t>Plan</a:t></a:r></a:p></dsp:txBody></dsp:sp><dsp:sp modelId="{4}"><dsp:spPr><a:xfrm><a:off x="1270000" y="0"/><a:ext cx="635000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></a:blipFill></dsp:spPr></dsp:sp></dsp:spTree></dsp:drawing>"#;

/// A one-sheet package with a smart graphic frame over B2:F10 whose drawing holds a picture.
pub(crate) fn with_diagram() -> Vec<u8> {
    let frame = r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>5</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>9</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Steps"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId2" r:lo="rId3" r:qs="rId4" r:cs="rId5"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>"#;
    let drawing = format!(r#"<?xml version="1.0"?><xdr:wsDr {NS}>{frame}</xdr:wsDr>"#);
    let drels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="{REL}/diagramData" Target="../diagrams/data1.xml"/><Relationship Id="rId3" Type="{REL}/diagramLayout" Target="../diagrams/layout1.xml"/><Relationship Id="rId4" Type="{REL}/diagramQuickStyle" Target="../diagrams/quickStyle1.xml"/><Relationship Id="rId5" Type="{REL}/diagramColors" Target="../diagrams/colors1.xml"/><Relationship Id="rId9" Type="http://schemas.microsoft.com/office/2007/relationships/diagramDrawing" Target="../diagrams/drawing1.xml"/></Relationships>"#
    );
    let pic_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/image" Target="../media/image1.png"/></Relationships>"#
    );
    let srels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
    minimal(
        r#"<sheetFormatPr defaultRowHeight="15"/><sheetData/><drawing r:id="rId1"/>"#,
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", &srels),
            ("xl/drawings/drawing1.xml", &drawing),
            ("xl/drawings/_rels/drawing1.xml.rels", &drels),
            ("xl/diagrams/data1.xml", DATA),
            ("xl/diagrams/layout1.xml", "<dgm:layoutDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>"),
            ("xl/diagrams/quickStyle1.xml", "<dgm:styleDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>"),
            ("xl/diagrams/colors1.xml", "<dgm:colorsDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>"),
            ("xl/diagrams/drawing1.xml", DRAWING),
            ("xl/diagrams/_rels/drawing1.xml.rels", &pic_rels),
            ("xl/media/image1.png", "picture bytes"),
        ],
        "",
        "",
    )
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn a_smartart_diagram_is_kept_with_its_drawing_and_pictures() {
    let (wb, report) = read_xlsx(&with_diagram()).unwrap();
    assert!(report.warnings.iter().all(|w| !w.contains("smart graphic")), "{:?}", report.warnings);
    let d = &wb.sheets[0].diagrams[0];
    assert_eq!(d.name, "Steps");
    assert_eq!((d.anchor.cell.col, d.anchor.cell.row), (1, 1));
    let roles: Vec<&str> = d.parts.iter().map(|p| p.role.as_str()).collect();
    assert_eq!(roles, ["data", "layout", "quickStyle", "colors", "drawing"]);
    assert_eq!(d.part("drawing").unwrap().data, DRAWING.as_bytes());
    assert_eq!(d.drawing_parts.len(), 1);
    assert_eq!((d.drawing_parts[0].role.as_str(), d.drawing_parts[0].data.as_slice()), ("rId1", b"picture bytes".as_slice()));

    let saved = write_xlsx(&wb).unwrap();
    let again = read_xlsx(&saved).unwrap().0;
    assert_eq!(serde_json::to_value(&again.sheets[0].diagrams).unwrap().as_array().unwrap().len(), 1);
    let back = &again.sheets[0].diagrams[0];
    assert_eq!((back.name.as_str(), back.anchor), (d.name.as_str(), d.anchor));
    for (p, q) in d.parts.iter().zip(&back.parts) {
        assert_eq!((&p.role, &p.rel_type, &p.content_type), (&q.role, &q.rel_type, &q.content_type));
        if p.role != "data" {
            assert_eq!(p.data, q.data, "{}", p.role);
        }
    }
    assert_eq!(back.drawing_parts, d.drawing_parts);
    // The data part names the drawing by its new relationship id.
    let frame = part(&saved, "xl/drawings/drawing1.xml");
    assert!(frame.contains("<dgm:relIds") && frame.contains("r:dm=\"rId"), "{frame}");
    let data = part(&saved, "xl/diagrams/data1.xml");
    let rels = part(&saved, "xl/drawings/_rels/drawing1.xml.rels");
    let rid = data.split("relId=\"").nth(1).unwrap().split('"').next().unwrap();
    assert!(rels.contains(&format!("Id=\"{rid}\" Type=\"http://schemas.microsoft.com/office/2007/relationships/diagramDrawing\"")), "{rels}");
    assert!(part(&saved, "[Content_Types].xml").contains("diagramDrawing+xml"));
}
