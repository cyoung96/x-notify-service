//! 标签属性解析:`<font>/<span>` 的 color 与 size(font-size)取值,
//! 支持引号与裸值、#RGB/#RRGGBB 与命名颜色。

/// 字号允许范围(超出视为无效样式,回落默认)
const FONT_SIZE_MIN: u16 = 11;
const FONT_SIZE_MAX: u16 = 18;

pub(super) fn parse_color_attr(attrs: &str) -> Option<(u8, u8, u8)> {
    let lower = attrs.to_ascii_lowercase();
    let pos = lower.find("color")?;
    parse_color_value(&extract_attr_value(&attrs[pos..])?)
}

/// `<font size="16">` / `<font size="16px">` / `style="font-size:16px"`
pub(super) fn parse_size_attr(attrs: &str) -> Option<u16> {
    let lower = attrs.to_ascii_lowercase();
    let pos = lower.find("font-size").or_else(|| lower.find("size"))?;
    let value = extract_attr_value(&attrs[pos..])?;
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    let size: u16 = digits.parse().ok()?;
    (FONT_SIZE_MIN..=FONT_SIZE_MAX)
        .contains(&size)
        .then_some(size)
}

/// 从 "xxx=值 ..." 形式的属性串中提取值(支持引号与裸值)
fn extract_attr_value(s: &str) -> Option<String> {
    let mut start: Option<usize> = None;
    let mut end = s.len();
    let mut quote: Option<char> = None;
    for (idx, ch) in s.char_indices().skip(5) {
        match (quote, ch) {
            (None, c) if c.is_whitespace() || c == '=' || c == ':' => {}
            (None, c) if c == '"' || c == '\'' => {
                quote = Some(c);
                start = Some(idx + c.len_utf8());
            }
            (None, _) => {
                start = Some(idx);
                end = s[idx..]
                    .find(char::is_whitespace)
                    .map_or(s.len(), |p| idx + p);
                break;
            }
            (Some(q), c) if c == q => {
                end = idx;
                break;
            }
            _ => {}
        }
    }
    start.map(|s0| s[s0..end].to_string())
}

fn parse_color_value(v: &str) -> Option<(u8, u8, u8)> {
    let v = v.trim().trim_end_matches(';');
    if let Some(hex) = v.strip_prefix('#') {
        let b = hex.as_bytes();
        let all_hex = b.iter().all(u8::is_ascii_hexdigit);
        return match b.len() {
            3 if all_hex => {
                assert_eq!(b.len(), 3, "hex 颜色长度守卫"); // 多次索引前置断言,辅助静态验证
                let d = |hi: u8, lo: u8| {
                    u8::from_str_radix(&format!("{}{}", hi as char, lo as char), 16).ok()
                };
                Some((d(b[0], b[0])?, d(b[1], b[1])?, d(b[2], b[2])?))
            }
            6 if all_hex => {
                let h = |r: std::ops::Range<usize>| -> Option<u8> {
                    u8::from_str_radix(std::str::from_utf8(&b[r]).ok()?, 16).ok()
                };
                Some((h(0..2)?, h(2..4)?, h(4..6)?))
            }
            _ => None,
        };
    }
    let named = match v.to_ascii_lowercase().as_str() {
        "red" => (0xd9, 0x30, 0x25),
        "green" => (0x00, 0x87, 0x3a),
        "blue" => (0x1a, 0x6d, 0xf2),
        "orange" => (0xe8, 0x71, 0x0a),
        "yellow" => (0xb2, 0x8b, 0x00),
        "purple" => (0x8b, 0x17, 0xb0),
        "gray" | "grey" => (0x86, 0x90, 0x9c),
        "black" => (0x1f, 0x23, 0x29),
        "white" => (0xff, 0xff, 0xff),
        _ => return None,
    };
    Some(named)
}
