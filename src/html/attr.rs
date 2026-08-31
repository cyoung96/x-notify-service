//! 标签属性解析:`<font>/<span>` 的 color 与 size(font-size)取值,
//! 支持引号与裸值、#RGB/#RRGGBB 与命名颜色。

/// 字号(11-18):构造即校验,超范围视为无效样式回落默认
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FontSize(pub u16);

impl FontSize {
    const MIN: u16 = 11;
    const MAX: u16 = 18;
    /// 域内返回 Some,域外 None(调用方回落默认)
    pub(super) fn new(value: u16) -> Option<Self> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
    }
}

/// 颜色(RGB 各通道 0-255)
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Rgb(pub u8, pub u8, pub u8);

pub(super) fn parse_color_attr(attrs: &str) -> Option<Rgb> {
    let lower = attrs.to_ascii_lowercase();
    let pos = lower.find("color")?;
    parse_color_value(&extract_attr_value(&attrs[pos..])?)
}

/// `<font size="16">` / `<font size="16px">` / `style="font-size:16px"`
pub(super) fn parse_size_attr(attrs: &str) -> Option<FontSize> {
    let lower = attrs.to_ascii_lowercase();
    let pos = lower.find("font-size").or_else(|| lower.find("size"))?;
    let value = extract_attr_value(&attrs[pos..])?;
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    FontSize::new(digits.parse().ok()?)
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

fn parse_color_value(value: &str) -> Option<Rgb> {
    let value = value.trim().trim_end_matches(';');
    if let Some(hex) = value.strip_prefix('#') {
        let bytes = hex.as_bytes();
        let all_hex = bytes.iter().all(u8::is_ascii_hexdigit);
        // 两位 hex → u8;#RGB 的每位自我重复为 #RRGGBB 语义
        let hex_byte =
            |hi: u8, lo: u8| u8::from_str_radix(&format!("{}{}", hi as char, lo as char), 16).ok();
        return match bytes {
            [r, g, b] if all_hex => {
                Some(Rgb(hex_byte(*r, *r)?, hex_byte(*g, *g)?, hex_byte(*b, *b)?))
            }
            [r0, r1, g0, g1, b0, b1] if all_hex => Some(Rgb(
                hex_byte(*r0, *r1)?,
                hex_byte(*g0, *g1)?,
                hex_byte(*b0, *b1)?,
            )),
            _ => None,
        };
    }
    let named = match value.to_ascii_lowercase().as_str() {
        "red" => Rgb(0xd9, 0x30, 0x25),
        "green" => Rgb(0x00, 0x87, 0x3a),
        "blue" => Rgb(0x1a, 0x6d, 0xf2),
        "orange" => Rgb(0xe8, 0x71, 0x0a),
        "yellow" => Rgb(0xb2, 0x8b, 0x00),
        "purple" => Rgb(0x8b, 0x17, 0xb0),
        "gray" | "grey" => Rgb(0x86, 0x90, 0x9c),
        "black" => Rgb(0x1f, 0x23, 0x29),
        "white" => Rgb(0xff, 0xff, 0xff),
        _ => return None,
    };
    Some(named)
}
