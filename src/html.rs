//! 极简 HTML 子集解析:业务所需的「加粗 + 颜色 + 字号 + 换行 + 实体」,
//! 其余标签一律剥除保留内文。输出逐行 StyledText 标记(行距/字号按行生效)。

pub const MAX_LINES: usize = 5;
/// 14px 基准下每行估宽容量:CJK 记 1 单位、其余记 0.55,正文区约 338px / 14px ≈ 24 单位
const BASE_LINE_UNITS: f64 = 24.0;
pub const BASE_FONT_SIZE: u16 = 14;
pub const FONT_SIZE_MIN: u16 = 11;
pub const FONT_SIZE_MAX: u16 = 18;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunStyle {
    pub bold: bool,
    pub color: Option<(u8, u8, u8)>,
    pub size: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
}

/// 逻辑行(解析产物,未折行)
#[derive(Debug, Clone)]
pub struct Line(pub Vec<Run>);

/// 物理行(折行后,带本行生效字号)
pub struct LineOut {
    pub runs: Vec<Run>,
    pub size: Option<u16>,
}

pub struct Parsed {
    pub lines: Vec<LineOut>,
}


pub fn parse(html: &str) -> Parsed {
    let mut lines: Vec<Line> = vec![Line(Vec::new())];
    let mut bold_depth: u32 = 0;
    let mut color_stack: Vec<Option<(u8, u8, u8)>> = vec![None];
    let mut size_stack: Vec<Option<u16>> = vec![None];
    let mut text = String::new();

    let chars: Vec<char> = html.chars().collect();
    let mut i = 0usize;

    macro_rules! flush {
        () => {
            if !text.is_empty() {
                let st = RunStyle {
                    bold: bold_depth > 0,
                    color: *color_stack.last().unwrap_or(&None),
                    size: *size_stack.last().unwrap_or(&None),
                };
                if let Some(line) = lines.last_mut() {
                    if let Some(last) = line.0.last_mut() {
                        if last.style == st {
                            last.text.push_str(&text);
                            text.clear();
                        }
                    }
                    if !text.is_empty() {
                        line.0.push(Run { text: std::mem::take(&mut text), style: st });
                    }
                }
                text.clear();
            }
        };
    }

    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            let mut j = i + 1;
            let mut closing = false;
            if j < chars.len() && chars[j] == '/' {
                closing = true;
                j += 1;
            }
            let mut name = String::new();
            while j < chars.len() && chars[j].is_ascii_alphabetic() {
                name.push(chars[j].to_ascii_lowercase());
                j += 1;
            }
            let mut attrs = String::new();
            while j < chars.len() && chars[j] != '>' {
                attrs.push(chars[j]);
                j += 1;
            }
            if j < chars.len() {
                j += 1;
            }
            if name.is_empty() {
                text.push('<');
                i += 1;
                continue;
            }
            match name.as_str() {
                "b" | "strong" => {
                    flush!();
                    if closing {
                        bold_depth = bold_depth.saturating_sub(1);
                    } else {
                        bold_depth += 1;
                    }
                }
                "font" | "span" => {
                    flush!();
                    if closing {
                        color_stack.pop();
                        if color_stack.is_empty() {
                            color_stack.push(None);
                        }
                        size_stack.pop();
                        if size_stack.is_empty() {
                            size_stack.push(None);
                        }
                    } else {
                        let inherited_color = *color_stack.last().unwrap_or(&None);
                        let inherited_size = *size_stack.last().unwrap_or(&None);
                        let color = parse_color_attr(&attrs).or(inherited_color);
                        let size = parse_size_attr(&attrs).or(inherited_size);
                        color_stack.push(color);
                        size_stack.push(size);
                    }
                }
                "br" => {
                    flush!();
                    lines.push(Line(Vec::new()));
                }
                "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                    if !closing => {
                        let line_empty = lines.last().is_some_and(|l| l.0.is_empty());
                        if !line_empty || !text.is_empty() {
                            flush!();
                            lines.push(Line(Vec::new()));
                        }
                    }
                _ => {} // 其余标签(i/u/a/s/table…)一律剥除,只保留内文
            }
            i = j;
            continue;
        }
        if c == '&' {
            let mut k = i + 1;
            let mut ent = String::new();
            while k < chars.len() && chars[k] != ';' && k - i <= 10 {
                ent.push(chars[k]);
                k += 1;
            }
            if k < chars.len() && chars[k] == ';'
                && let Some(ch) = decode_entity(&ent) {
                    text.push(ch);
                    i = k + 1;
                    continue;
                }
            text.push('&');
            i += 1;
            continue;
        }
        if c == '\n' || c == '\r' {
            text.push(' ');
            i += 1;
            continue;
        }
        text.push(c);
        i += 1;
    }
    flush!();
    while lines.len() > 1 && lines.last().is_some_and(|l| l.0.is_empty()) {
        lines.pop();
    }

    Parsed { lines: wrap_lines(lines) }
}

fn char_units(c: char) -> f64 {
    if c.is_ascii() {
        0.55
    } else {
        1.0
    }
}

/// 行首禁则(kinsoku):这些标点不能出现在折行后下一行的开头
fn is_no_line_start(c: char) -> bool {
    matches!(c,
        '，' | '。' | '、' | '；' | '：' | '！' | '？' | '…' | '·'
        | ',' | '.' | ';' | ':' | '!' | '?'
        | ')' | ']' | '}' | '%'
        | '）' | '》' | '〉' | '」' | '』' | '】' | '〕'
        | '"' | '”' | '\'' | '’'
    )
}

/// 按估宽把逻辑行折成物理行:行容量随该行字号缩放(字号大 → 每行字数少);
/// 行生效字号取行内首个显式字号;超过 MAX_LINES 截断加 …;
/// 行首禁则:折行点若使标点成为下一行行首,则把前一字符一并带到下一行。
fn wrap_lines(lines: Vec<Line>) -> Vec<LineOut> {
    let mut out: Vec<LineOut> = Vec::new();
    let mut truncated = false;
    'outer: for Line(runs) in lines {
        let line_size = runs.iter().find_map(|r| r.style.size);
        let font = line_size.unwrap_or(BASE_FONT_SIZE) as f64;
        let max_units = BASE_LINE_UNITS * (BASE_FONT_SIZE as f64) / font;
        let mut cur: Vec<Run> = Vec::new();
        let mut units = 0.0;
        for run in runs {
            let mut seg = String::new();
            for ch in run.text.chars() {
                if units + char_units(ch) > max_units {
                    // 行首禁则:循环把 seg 末尾字符带下去,直到行首字符合法
                    let mut head = String::new();
                    let mut first = ch;
                    while is_no_line_start(first) {
                        match seg.pop() {
                            Some(prev) => {
                                head.insert(0, prev);
                                first = prev;
                            }
                            None => break, // 整行都是禁则字符,放弃处理
                        }
                    }
                    if !seg.is_empty() {
                        cur.push(Run { text: std::mem::take(&mut seg), style: run.style });
                    }
                    out.push(LineOut { runs: std::mem::take(&mut cur), size: line_size });
                    units = head.chars().map(char_units).sum();
                    seg = head;
                    if out.len() >= MAX_LINES {
                        truncated = true;
                        break 'outer;
                    }
                }
                seg.push(ch);
                units += char_units(ch);
            }
            if !seg.is_empty() {
                cur.push(Run { text: seg, style: run.style });
            }
        }
        out.push(LineOut { runs: cur, size: line_size });
        if out.len() >= MAX_LINES {
            truncated = true;
            break;
        }
    }
    if truncated
        && let Some(last) = out.last_mut() {
            let st = last.runs.last().map(|r| r.style).unwrap_or(RunStyle {
                bold: false,
                color: None,
                size: None,
            });
            last.runs.push(Run { text: "…".into(), style: st });
        }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 行首禁则:折行后任何一行的行首不能是标点
    #[test]
    fn kinsoku_no_punctuation_at_line_start() {
        // 24 个汉字恰好填满一行,逗号将被禁则处理:前一字符带到第二行行首
        let text = format!("{},后续内容继续排列显示", "一".repeat(24));
        let parsed = parse(&text);
        assert!(parsed.lines.len() >= 2, "应当折行");
        for line in &parsed.lines {
            if let Some(c) = line.runs.first().and_then(|r| r.text.chars().next()) {
                assert!(!is_no_line_start(c), "行首出现禁则标点: {c}");
            }
        }
    }
}

/// 逐行转为 StyledText markdown 标记:加粗用 **…**,颜色用 <font color>;
/// 字号不进标记(StyledText 不支持行内字号),由调用方按行设置元素字号
pub fn to_styled_lines(parsed: &Parsed) -> Vec<(String, Option<u16>)> {
    parsed
        .lines
        .iter()
        .map(|line| {
            let mut out = String::new();
            for run in &line.runs {
                let text = escape_markup(&run.text);
                if text.is_empty() {
                    continue;
                }
                let colored = run.style.color.is_some();
                if colored {
                    let (r, g, b) = run.style.color.unwrap_or((0x5f, 0x66, 0x72));
                    out.push_str(&format!("<font color=\"#{r:02x}{g:02x}{b:02x}\">"));
                }
                if run.style.bold {
                    out.push_str("**");
                    out.push_str(&text);
                    out.push_str("**");
                } else {
                    out.push_str(&text);
                }
                if colored {
                    out.push_str("</font>");
                }
            }
            (out, line.size)
        })
        .collect()
}

fn escape_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '*' | '<' | '>' | '`' | '_' | '~' | '[' | ']' | '#' | '+' | '-' | '.' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

fn parse_color_attr(attrs: &str) -> Option<(u8, u8, u8)> {
    let lower = attrs.to_ascii_lowercase();
    let pos = lower.find("color")?;
    parse_color_value(&extract_attr_value(&attrs[pos..])?)
}

/// `<font size="16">` / `<font size="16px">` / `style="font-size:16px"`
fn parse_size_attr(attrs: &str) -> Option<u16> {
    let lower = attrs.to_ascii_lowercase();
    let pos = lower.find("font-size").or_else(|| lower.find("size"))?;
    let value = extract_attr_value(&attrs[pos..])?;
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    let size: u16 = digits.parse().ok()?;
    if !(FONT_SIZE_MIN..=FONT_SIZE_MAX).contains(&size) {
        return None;
    }
    Some(size)
}

fn extract_attr_value(s: &str) -> Option<String> {
    let mut start: Option<usize> = None;
    let mut end = s.len();
    let mut quote: Option<char> = None;
    for (idx, ch) in s.char_indices().skip(5) {
        match (quote, ch) {
            (None, c) if c.is_whitespace() || c == '=' || c == ':' => continue,
            (None, c) if c == '"' || c == '\'' => {
                quote = Some(c);
                start = Some(idx + c.len_utf8());
            }
            (None, _) => {
                start = Some(idx);
                end = s[idx..].find(char::is_whitespace).map(|p| idx + p).unwrap_or(s.len());
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
        let all_hex = b.iter().all(|c| c.is_ascii_hexdigit());
        return match b.len() {
            3 if all_hex => {
                let d = |hi: u8, lo: u8| {
                    u8::from_str_radix(&format!("{}{}", hi as char, lo as char), 16).ok()
                };
                Some((d(b[0], b[0])?, d(b[1], b[1])?, d(b[2], b[2])?))
            }
            6 if all_hex => {
                let h = |r: std::ops::Range<usize>| {
                    std::str::from_utf8(&b[r]).ok().and_then(|s| u8::from_str_radix(s, 16).ok())
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
            code.and_then(char::from_u32)
        }
    }
}

/// 提取纯文本(供系统通知兜底)
pub fn to_plain_text(html: &str) -> String {
    parse(html)
        .lines
        .iter()
        .map(|l| l.runs.iter().map(|run| run.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
