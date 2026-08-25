//! 正文 HTML 子集(加粗/颜色/字号/换行/实体)的解析与排版输出。
//!
//! 流水线:parse(标签与实体 → 带样式逻辑行)→ wrap(估宽折行 + 行首禁则)
//! → markup(逐行输出 StyledText 标记)。中间类型对本模块外不可见,
//! 调用方只依赖三个入口:[parse]、[to_styled_lines]、[to_plain_text]。

mod markup;
mod parse;
mod wrap;

/// 正文默认字号(缺省与折行估宽的基准)
pub const BASE_FONT_SIZE: u16 = 14;

/// 解析结果:折行后的物理行(对外不透明)
pub struct Parsed {
    lines: wrap::WrappedLines,
}

/// 解析 HTML 子集并完成折行
pub fn parse(html: &str) -> Parsed {
    Parsed { lines: wrap::wrap_lines(parse::parse_logical_lines(html)) }
}

/// 逐行转为 StyledText markdown 标记:加粗用 `**…**`,颜色用 `<font color>`;
/// 字号不进标记(StyledText 不支持行内字号),由调用方按行设置元素字号。
/// 返回 (该行标记, 该行字号)。
pub fn to_styled_lines(parsed: &Parsed) -> Vec<(String, Option<u16>)> {
    markup::styled_lines(&parsed.lines)
}

/// 提取纯文本(供系统通知兜底)
pub fn to_plain_text(html: &str) -> String {
    parse(html)
        .lines
        .0
        .iter()
        .map(|l| l.runs.iter().map(|r| r.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
