//! 逐行输出 Slint `StyledText` 的 markdown 标记。
//! 加粗用 `**…**`,颜色用 `<font color="#rrggbb">`;字号不进标记
//! (`StyledText` 不支持行内字号),由调用方按行设置元素字号。

use std::fmt::Write as _;

use super::attr::FontSize;
use super::wrap::WrappedLines;

pub(super) fn styled_lines(lines: &WrappedLines) -> Vec<(String, Option<FontSize>)> {
    lines
        .0
        .iter()
        .map(|line| {
            let mut out = String::new();
            for run in &line.runs {
                let text = escape_markup(&run.text);
                if text.is_empty() {
                    continue;
                }
                if let Some(rgb) = run.style.color {
                    let _ = write!(out, "<font color=\"{}\">", rgb.hex());
                }
                if run.style.bold {
                    out.push_str("**");
                    out.push_str(&text);
                    out.push_str("**");
                } else {
                    out.push_str(&text);
                }
                if run.style.color.is_some() {
                    out.push_str("</font>");
                }
            }
            (out, line.size)
        })
        .collect()
}

/// 反斜杠转义 markdown/HTML 特殊字符,防止正文内容被当作标记
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
