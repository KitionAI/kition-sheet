//! Text widths in Office's fonts, for sizing columns and rows as Excel does. The advance widths of
//! the printable ASCII characters (U+0020 to U+007E, in units of a 2048-unit em) are those of the
//! open, metric-compatible stand-ins the grid draws these fonts in (decision F1: Carlito, Caladea,
//! Arimo, Tinos and Cousine, SIL Open Font License 1.1), which match the Office fonts' widths.

use kition_sheet_model::Font;

const CALIBRI: [u16; 95] = [
    463, 667, 821, 1020, 1038, 1464, 1397, 452, 621, 621, 1020, 1020, 511, 627, 517, 791, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038,
    548, 548, 1020, 1020, 1020, 949, 1831, 1185, 1114, 1092, 1260, 1000, 941, 1292, 1276, 516, 653, 1064, 861, 1751, 1322, 1356, 1058, 1378, 1112,
    941, 998, 1314, 1162, 1822, 1063, 998, 959, 628, 791, 628, 1020, 1020, 596, 981, 1076, 866, 1076, 1019, 625, 964, 1076, 470, 490, 931, 470, 1636,
    1076, 1080, 1076, 1076, 714, 801, 686, 1076, 925, 1464, 887, 927, 809, 644, 943, 644, 1020,
];
const CALIBRI_BOLD: [u16; 95] = [
    463, 667, 898, 1020, 1038, 1493, 1443, 478, 638, 638, 1020, 1020, 528, 627, 547, 880, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038, 1038,
    565, 565, 1020, 1020, 1020, 949, 1840, 1241, 1148, 1084, 1291, 999, 940, 1305, 1292, 546, 678, 1120, 866, 1790, 1349, 1385, 1090, 1405, 1153,
    968, 1014, 1337, 1211, 1856, 1128, 1064, 979, 665, 880, 665, 1020, 1020, 615, 1011, 1099, 857, 1099, 1031, 648, 971, 1099, 503, 523, 983, 503,
    1666, 1099, 1101, 1099, 1099, 728, 817, 710, 1099, 969, 1526, 941, 970, 814, 704, 973, 704, 1020,
];
const CAMBRIA: [u16; 95] = [
    451, 586, 805, 1268, 1036, 1823, 1407, 485, 782, 782, 874, 1135, 420, 674, 422, 1004, 1059, 741, 967, 944, 1008, 973, 963, 963, 993, 979, 541,
    541, 1135, 1135, 1135, 864, 1812, 1227, 1186, 1063, 1280, 1106, 1042, 1104, 1425, 702, 623, 1290, 1026, 1819, 1419, 1227, 1092, 1227, 1255, 928,
    1141, 1407, 1225, 1821, 1169, 1141, 1038, 717, 1004, 717, 1135, 760, 309, 963, 1065, 862, 1065, 903, 641, 940, 1135, 567, 522, 1075, 555, 1665,
    1143, 983, 1065, 1026, 811, 803, 666, 1118, 963, 1475, 989, 987, 877, 793, 647, 793, 1458,
];
const CAMBRIA_BOLD: [u16; 95] = [
    451, 686, 864, 1266, 1112, 1999, 1516, 514, 836, 836, 928, 1212, 475, 637, 471, 1034, 1092, 938, 1038, 1040, 1133, 1065, 1102, 1012, 1139, 1094,
    573, 573, 1212, 1212, 1212, 926, 1886, 1337, 1298, 1128, 1360, 1145, 1094, 1200, 1473, 733, 696, 1393, 1108, 1731, 1405, 1325, 1214, 1325, 1311,
    969, 1202, 1423, 1294, 1964, 1262, 1202, 1135, 754, 1034, 754, 1212, 760, 377, 1075, 1163, 922, 1167, 1051, 686, 1040, 1229, 631, 608, 1206, 637,
    1825, 1237, 1085, 1174, 1145, 930, 909, 748, 1247, 1063, 1595, 1075, 1067, 950, 805, 655, 805, 1212,
];
const ARIAL: [u16; 95] = [
    569, 569, 727, 1139, 1139, 1821, 1366, 391, 682, 682, 797, 1196, 569, 682, 569, 569, 1139, 1139, 1139, 1139, 1139, 1139, 1139, 1139, 1139, 1139,
    569, 569, 1196, 1196, 1196, 1139, 2079, 1366, 1366, 1479, 1479, 1366, 1251, 1593, 1479, 569, 1024, 1366, 1139, 1706, 1479, 1593, 1366, 1593,
    1479, 1366, 1251, 1479, 1366, 1933, 1366, 1366, 1251, 569, 569, 569, 961, 1139, 682, 1139, 1139, 1024, 1139, 1139, 569, 1139, 1139, 455, 455,
    1024, 455, 1706, 1139, 1139, 1139, 1139, 682, 1024, 569, 1139, 1024, 1479, 1024, 1024, 1024, 684, 532, 684, 1196,
];
const ARIAL_BOLD: [u16; 95] = [
    569, 682, 971, 1139, 1139, 1821, 1479, 487, 682, 682, 797, 1196, 569, 682, 569, 569, 1139, 1139, 1139, 1139, 1139, 1139, 1139, 1139, 1139, 1139,
    682, 682, 1196, 1196, 1196, 1251, 1997, 1479, 1479, 1479, 1479, 1366, 1251, 1593, 1479, 569, 1139, 1479, 1251, 1706, 1479, 1593, 1366, 1593,
    1479, 1366, 1251, 1479, 1366, 1933, 1366, 1366, 1251, 682, 569, 682, 1196, 1139, 682, 1139, 1251, 1139, 1251, 1139, 682, 1251, 1251, 569, 569,
    1139, 569, 1821, 1251, 1251, 1251, 1251, 797, 1139, 682, 1251, 1139, 1593, 1139, 1139, 1024, 797, 573, 797, 1196,
];
const TIMES: [u16; 95] = [
    512, 682, 836, 1024, 1024, 1706, 1593, 369, 682, 682, 1024, 1155, 512, 682, 512, 569, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024,
    569, 569, 1155, 1155, 1155, 909, 1886, 1479, 1366, 1366, 1479, 1251, 1139, 1479, 1479, 682, 797, 1479, 1251, 1821, 1479, 1479, 1139, 1479, 1366,
    1139, 1251, 1479, 1479, 1933, 1479, 1479, 1251, 682, 569, 682, 961, 1024, 682, 909, 1024, 909, 1024, 909, 682, 1024, 1024, 569, 569, 1024, 569,
    1593, 1024, 1024, 1024, 1024, 682, 797, 569, 1024, 1024, 1479, 1024, 1024, 909, 983, 410, 983, 1108,
];
const TIMES_BOLD: [u16; 95] = [
    512, 682, 1137, 1024, 1024, 2048, 1706, 569, 682, 682, 1024, 1167, 512, 682, 512, 569, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024,
    1024, 682, 682, 1167, 1167, 1167, 1024, 1905, 1479, 1366, 1479, 1479, 1366, 1251, 1593, 1593, 797, 1024, 1593, 1366, 1933, 1479, 1593, 1251,
    1593, 1479, 1139, 1366, 1479, 1479, 2048, 1479, 1479, 1366, 682, 569, 682, 1190, 1024, 682, 1024, 1139, 909, 1139, 909, 682, 1024, 1139, 569,
    682, 1139, 569, 1706, 1139, 1024, 1139, 1139, 909, 797, 682, 1139, 1024, 1479, 1024, 1024, 909, 807, 451, 807, 1065,
];
const COURIER: [u16; 95] = [
    1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229,
    1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229,
    1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229,
    1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229, 1229,
];

/// Chinese faces: their Latin letters are drawn (and measured) in the Arial stand-in, as WPS
/// for Mac draws them in a sans face (PingFang); their Chinese characters a whole em.
pub const CJK_FACES: [&str; 22] = [
    "宋体",
    "新宋体",
    "simsun",
    "nsimsun",
    "微软雅黑",
    "microsoft yahei",
    "黑体",
    "simhei",
    "等线",
    "等线 light",
    "dengxian",
    "仿宋",
    "fangsong",
    "楷体",
    "kaiti",
    "华文宋体",
    "stsong",
    "华文黑体",
    "stheiti",
    "苹方-简",
    "pingfang sc",
    "songti sc",
];

/// The advance table for a font's name and weight, when it is one of the fonts above.
fn table(name: &str, bold: bool) -> Option<&'static [u16; 95]> {
    let lower = name.trim().to_lowercase();
    let table = match lower.as_str() {
        "calibri" | "calibri light" | "carlito" => (&CALIBRI, &CALIBRI_BOLD),
        "cambria" | "caladea" => (&CAMBRIA, &CAMBRIA_BOLD),
        "arial" | "helvetica" | "liberation sans" | "arimo" => (&ARIAL, &ARIAL_BOLD),
        "times new roman" | "times" | "liberation serif" | "tinos" => (&TIMES, &TIMES_BOLD),
        "courier new" | "courier" | "liberation mono" | "cousine" => (&COURIER, &COURIER),
        _ if CJK_FACES.contains(&lower.as_str()) => (&ARIAL, &ARIAL_BOLD),
        _ => return None,
    };
    Some(if bold { table.1 } else { table.0 })
}

/// The advance of one character in `font`, in pixels at 96 dpi (see [`text_width`]).
pub fn char_width(c: char, font: &Font) -> f32 {
    let Some(table) = table(&font.name, font.bold) else {
        return crate::cmd::format::approx_text_width(c.encode_utf8(&mut [0; 4]), font.size, font.bold);
    };
    let em = match u32::from(c) {
        code @ 0x20..=0x7e => f32::from(table[(code - 0x20) as usize]) / 2048.0,
        _ if c.is_control() => 0.0,
        _ => 1.0,
    };
    em * font.size * 96.0 / 72.0
}

/// The width of `text` in `font` at its size, in pixels at 96 dpi: exact for the fonts above
/// (characters beyond ASCII, such as Chinese, a whole em), estimated for others.
pub fn text_width(text: &str, font: &Font) -> f32 {
    let Some(table) = table(&font.name, font.bold) else {
        return crate::cmd::format::approx_text_width(text, font.size, font.bold);
    };
    let ems: f32 = text
        .chars()
        .map(|c| match u32::from(c) {
            code @ 0x20..=0x7e => f32::from(table[(code - 0x20) as usize]) / 2048.0,
            _ if c.is_control() => 0.0,
            _ => 1.0,
        })
        .sum();
    ems * font.size * 96.0 / 72.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(name: &str, size: f32, bold: bool) -> Font {
        Font { name: name.into(), size, bold, ..Font::default() }
    }

    #[test]
    fn calibri_digits_are_seven_pixels_at_eleven_points() {
        // Excel's default column (8.43 characters, 64 pixels) is built on this width.
        let width = text_width("0", &font("Calibri", 11.0, false));
        assert!((width - 7.43).abs() < 0.01, "{width}");
        assert!(text_width("Total", &font("Calibri", 11.0, true)) > text_width("Total", &font("Calibri", 11.0, false)));
        assert_eq!(text_width("iiii", &font("Courier New", 10.0, false)), text_width("MMMM", &font("Courier New", 10.0, false)));
        // Fonts without a table keep the estimate.
        assert_eq!(text_width("abc", &font("Verdana", 11.0, false)), crate::cmd::format::approx_text_width("abc", 11.0, false));
    }
}
