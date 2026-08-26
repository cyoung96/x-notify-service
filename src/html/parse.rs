//! 标签与实体解析:HTML 子集 → 带样式的逻辑行(未折行)。
//! 支持的子集:`<b>/<strong>`、`<font>/<span>` 的 color 与 size、`<br>`、
//! 块级标签换行、HTML 实体;其余标签一律剥除保留内文。

/// 字号允许范围(超出视为无效样式,回落默认)
const FONT_SIZE_MIN: u16 = 11;
const FONT_SIZE_MAX: u16 = 18;

/// 一段同样式文本
pub(super) struct Run {
    pub text: String,
    pub style: RunStyle,
}

/// 样式:加粗 + 颜色 + 字号(均为解析到的显式值)
#[derive(Clone, Copy, PartialEq)]
pub(super) struct RunStyle {
    pub bold: bool,
    pub color: Option<(u8, u8, u8)>,
    pub size: Option<u16>,
}

/// 逻辑行:一组 Run
pub(super) struct Line(pub Vec<Run>);

/// 逻辑行集合(parse 的产物)
pub(super) struct LogicalLines(pub Vec<Line>);

/// 解析入口:HTML 子集 → 逻辑行
pub(super) fn parse_logical_lines(html: &str) -> LogicalLines {
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
                let st = current_style(bold_depth, &color_stack, &size_stack);
                append_run(lines.last_mut(), &mut text, st);
            }
        };
    }

    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            let (name, attrs, closing, next) = read_tag(&chars, i);
            if name.is_empty() {
                text.push('<');
                i += 1;
                continue;
            }
            apply_tag(
                &name,
                &attrs,
                closing,
                &mut lines,
                &mut text,
                &mut bold_depth,
                &mut color_stack,
                &mut size_stack,
            );
            i = next;
            continue;
        }
        if c == '&' {
            let (decoded, next) = read_entity(&chars, i);
            if let Some(ch) = decoded {
                text.push(ch);
                i = next;
            } else {
                text.push('&');
                i += 1;
            }
            continue;
        }
        if c == '\n' || c == '\r' {
            text.push(' '); // HTML 语义:源码换行等价空白
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
    LogicalLines(lines)
}

/// 当前生效样式(栈顶)
fn current_style(
    bold_depth: u32,
    color_stack: &[Option<(u8, u8, u8)>],
    size_stack: &[Option<u16>],
) -> RunStyle {
    RunStyle {
        bold: bold_depth > 0,
        color: *color_stack.last().unwrap_or(&None),
        size: *size_stack.last().unwrap_or(&None),
    }
}

/// 把累积文本按样式并入当前行(与行尾同样式的 Run 合并)
fn append_run(line: Option<&mut Line>, text: &mut String, st: RunStyle) {
    let Some(line) = line else { return };
    if let Some(last) = line.0.last_mut()
        && last.style == st
    {
        last.text.push_str(text);
        text.clear();
        return;
    }
    line.0.push(Run {
        text: std::mem::take(text),
        style: st,
    });
}

/// 读取一个标签,返回 (小写标签名, 原始属性串, 是否闭合, 下一个下标)
fn read_tag(chars: &[char], from: usize) -> (String, String, bool, usize) {
    let mut j = from + 1;
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
        j += 1; // 跳过 '>'
    }
    (name, attrs, closing, j)
}

/// 应用标签语义
#[allow(clippy::too_many_arguments)]
fn apply_tag(
    name: &str,
    attrs: &str,
    closing: bool,
    lines: &mut Vec<Line>,
    text: &mut String,
    bold_depth: &mut u32,
    color_stack: &mut Vec<Option<(u8, u8, u8)>>,
    size_stack: &mut Vec<Option<u16>>,
) {
    let st = current_style(*bold_depth, color_stack, size_stack);
    match name {
        "b" | "strong" => {
            append_run(lines.last_mut(), text, st);
            *bold_depth = if closing {
                bold_depth.saturating_sub(1)
            } else {
                *bold_depth + 1
            };
        }
        "font" | "span" => {
            append_run(lines.last_mut(), text, st);
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
                // 未显式指定时继承外层样式(栈顶)
                let inherited_color = color_stack.last().copied().flatten();
                let inherited_size = size_stack.last().copied().flatten();
                let color = parse_color_attr(attrs).or(inherited_color);
                let size = parse_size_attr(attrs).or(inherited_size);
                color_stack.push(color);
                size_stack.push(size);
            }
        }
        "br" => {
            append_run(lines.last_mut(), text, st);
            lines.push(Line(Vec::new()));
        }
        "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" if !closing => {
            // 块级标签换行(连续块标签不产生空行)
            let line_empty = lines.last().is_some_and(|l| l.0.is_empty());
            if !line_empty || !text.is_empty() {
                append_run(lines.last_mut(), text, st);
                lines.push(Line(Vec::new()));
            }
        }
        _ => {} // 其余标签(i/u/a/s/table…)一律剥除,只保留内文
    }
}

/// 读取实体,返回 (解码字符, 下一个下标);非实体时解码为 None
fn read_entity(chars: &[char], from: usize) -> (Option<char>, usize) {
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
