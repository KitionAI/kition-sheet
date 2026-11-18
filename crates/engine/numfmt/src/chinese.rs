//! `[DBNum1]` / `[DBNum2]`: numbers written in Chinese numerals, as Chinese spreadsheets show them.
//! DBNum1 uses the everyday numerals (一千二百三十四), DBNum2 the financial ones (壹仟贰佰叁拾肆).
//! Whole numbers read with their units; digits after a decimal point and four-digit years read
//! digit by digit (二〇二六).

const DIGITS1: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const DIGITS2: [&str; 10] = ["零", "壹", "贰", "叁", "肆", "伍", "陆", "柒", "捌", "玖"];
const UNITS1: [&str; 4] = ["", "十", "百", "千"];
const UNITS2: [&str; 4] = ["", "拾", "佰", "仟"];
const GROUPS: [&str; 5] = ["", "万", "亿", "万亿", "亿亿"];

/// Rewrites every run of ASCII digits in `text`.
pub(crate) fn numerals(text: &str, level: u8, date: bool) -> String {
    if level != 1 && level != 2 {
        return text.to_string();
    }
    let digits = if level == 1 { &DIGITS1 } else { &DIGITS2 };
    let mut out = String::with_capacity(text.len() * 3);
    let mut run = String::new();
    let mut after_point = false;
    let flush = |run: &mut String, out: &mut String, after_point: bool| {
        if run.is_empty() {
            return;
        }
        if after_point || (date && run.len() == 4) || run.len() > 20 {
            for b in run.bytes() {
                out.push_str(digits.get(usize::from(b - b'0')).copied().unwrap_or(""));
            }
        } else {
            out.push_str(&counting(run.trim_start_matches('0'), level));
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() {
            run.push(c);
        } else {
            flush(&mut run, &mut out, after_point);
            after_point = c == '.' && !date;
            out.push(if c == '.' && !date { '.' } else { c });
        }
    }
    flush(&mut run, &mut out, after_point);
    out
}

/// A whole number (decimal digits, no leading zeros) in counting form.
fn counting(n: &str, level: u8) -> String {
    let (digits, units) = if level == 1 { (&DIGITS1, &UNITS1) } else { (&DIGITS2, &UNITS2) };
    if n.is_empty() {
        return if level == 1 { "〇".into() } else { "零".into() };
    }
    let zero = "零";
    let bytes: Vec<u8> = n.bytes().map(|b| b - b'0').collect();
    let groups = bytes.len().div_ceil(4);
    let mut out = String::new();
    let mut pending_zero = false;
    for g in 0..groups {
        let hi = bytes.len() - (groups - 1 - g) * 4;
        let lo = hi.saturating_sub(4);
        let part = bytes.get(lo..hi).unwrap_or(&[]);
        if part.iter().all(|d| *d == 0) {
            pending_zero = !out.is_empty();
            continue;
        }
        let width = part.len();
        for (i, d) in part.iter().enumerate() {
            let pos = width - 1 - i;
            if *d == 0 {
                if !out.is_empty() {
                    pending_zero = true;
                }
                continue;
            }
            if pending_zero {
                out.push_str(zero);
                pending_zero = false;
            }
            // 10–19 read 十… in everyday numerals, 壹拾… in financial ones.
            let skip_one = level == 1 && *d == 1 && pos == 1 && out.is_empty();
            if !skip_one {
                out.push_str(digits.get(usize::from(*d)).copied().unwrap_or(""));
            }
            out.push_str(units.get(pos).copied().unwrap_or(""));
        }
        out.push_str(GROUPS.get(groups - 1 - g).copied().unwrap_or(""));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_forms() {
        assert_eq!(numerals("1234", 1, false), "一千二百三十四");
        assert_eq!(numerals("1234", 2, false), "壹仟贰佰叁拾肆");
        assert_eq!(numerals("1004", 1, false), "一千零四");
        assert_eq!(numerals("10", 1, false), "十");
        assert_eq!(numerals("10", 2, false), "壹拾");
        assert_eq!(numerals("100000", 1, false), "十万");
        assert_eq!(numerals("100010", 1, false), "十万零一十");
        assert_eq!(numerals("0", 1, false), "〇");
        assert_eq!(numerals("1234.56", 1, false), "一千二百三十四.五六");
        assert_eq!(numerals("-12", 1, false), "-十二");
        assert_eq!(numerals("2026年11月10日", 1, true), "二〇二六年十一月十日");
    }
}
