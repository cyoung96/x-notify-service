//! 标签与实体解析:HTML 子集 → 带样式的逻辑行(未折行)。
//! 支持的子集:`<b>/<strong>`、`<font>/<span>` 的 color 与 size、`<br>`、
//! 块级标签换行、HTML 实体;其余标签一律剥除保留内文。

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
            let (decoded, next) = super::entity::read_entity(&chars, i);
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
                let color = super::attr::parse_color_attr(attrs).or(inherited_color);
                let size = super::attr::parse_size_attr(attrs).or(inherited_size);
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
