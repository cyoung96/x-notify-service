//! HTML 实体读取与解码:命名实体(amp/lt/gt/quot/apos/nbsp)
//! 与数字实体(`&#65;` / `&#x41;`),上限 10 字符防止吞掉普通文本。

/// 读取实体,返回 (解码字符, 下一个下标);非实体时解码为 None
pub(super) fn read_entity(chars: &[char], from: usize) -> (Option<char>, usize) {
    let mut k = from + 1;
    let mut ent = String::new();
    while k < chars.len() && chars[k] != ';' && k - from <= 10 {
        ent.push(chars[k]);
        k += 1;
    }
    if k < chars.len()
        && chars[k] == ';'
        && let Some(ch) = decode_entity(&ent)
    {
        return (Some(ch), k + 1);
    }
    (None, from)
}

fn decode_entity(ent: &str) -> Option<char> {
    match ent {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let num = ent.strip_prefix('#')?;
            let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num.parse::<u32>().ok()
            };
            char::from_u32(code?)
        }
    }
}
