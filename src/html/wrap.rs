//! 估宽折行:CJK 记 1 单位、其余 0.55,行容量随该行字号缩放;
//! 行首禁则(`kinsoku`):折行点若使标点成为下一行行首,把前一字符一并带下去;
//! 超过 `MAX_LINES` 截断,末尾追加 …。

use super::parse::{Line, LogicalLines, Run, RunStyle};

/// 最多显示行数,超出截断加 …
const MAX_LINES: usize = 5;
/// 14px 基准下每行估宽容量:正文区约 338px / 14px ≈ 24 单位
const BASE_LINE_UNITS: f64 = 24.0;

/// 物理行(折行后,带本行生效字号)
pub(super) struct LineOut {
    pub runs: Vec<Run>,
    pub size: Option<u16>,
}

pub(super) struct WrappedLines(pub Vec<LineOut>);

pub(super) fn wrap_lines(lines: LogicalLines) -> WrappedLines {
    let LogicalLines(logical) = lines;
    let mut out: Vec<LineOut> = Vec::new();
    let mut truncated = false;
    'outer: for Line(runs) in logical {
        let line_size = runs.iter().find_map(|r| r.style.size);
        let font = f64::from(line_size.unwrap_or(super::BASE_FONT_SIZE));
        let max_units = BASE_LINE_UNITS * f64::from(super::BASE_FONT_SIZE) / font;
        let mut cur: Vec<Run> = Vec::new();
        let mut units = 0f64;
        for run in runs {
            let mut seg = String::new();
            for ch in run.text.chars() {
                if units + char_units(ch) > max_units {
                    // 行首禁则:循环把 seg 末尾字符带下去,直到行首字符合法
                    let (head, rest) = split_with_kinsoku(seg, ch);
                    seg = rest;
                    if !seg.is_empty() {
                        cur.push(Run {
                            text: std::mem::take(&mut seg),
                            style: run.style,
                        });
                    }
                    out.push(LineOut {
                        runs: std::mem::take(&mut cur),
                        size: line_size,
                    });
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
                cur.push(Run {
                    text: seg,
                    style: run.style,
                });
            }
        }
        out.push(LineOut {
            runs: cur,
            size: line_size,
        });
        if out.len() >= MAX_LINES {
            truncated = true;
            break;
        }
    }
    if truncated && let Some(last) = out.last_mut() {
        let st = last.runs.last().map_or(
            RunStyle {
                bold: false,
                color: None,
                size: None,
            },
            |r| r.style,
        );
        last.runs.push(Run {
            text: "…".into(),
            style: st,
        });
    }
    WrappedLines(out)
}

const fn char_units(c: char) -> f64 {
    if c.is_ascii() { 0.55 } else { 1.0 }
}

/// 行首禁则(`kinsoku`):这些标点不能出现在折行后下一行的开头
const fn is_no_line_start(c: char) -> bool {
    matches!(
        c,
        '，' | '。'
            | '、'
            | '；'
            | '：'
            | '！'
            | '？'
            | '…'
            | '·'
            | ','
            | '.'
            | ';'
            | ':'
            | '!'
            | '?'
            | ')'
            | ']'
            | '}'
            | '%'
            | '）'
            | '》'
            | '〉'
            | '」'
            | '』'
            | '】'
            | '〕'
            | '"'
            | '”'
            | '\''
            | '’'
    )
}

/// 折行点落在禁则标点 `next` 上时,返回 (带下去的行首字符, 留在本行的剩余);
/// 整段都是禁则字符时放弃处理(原样保留)
fn split_with_kinsoku(seg: String, next: char) -> (String, String) {
    if !is_no_line_start(next) {
        return (String::new(), seg);
    }
    let mut rest = seg;
    let mut head = String::new();
    let mut first = next;
    while is_no_line_start(first) {
        match rest.pop() {
            Some(prev) => {
                head.insert(0, prev);
                first = prev;
            }
            None => return (String::new(), head), // 整行禁则:不强行拆
        }
    }
    (head, rest)
}

#[cfg(test)]
mod tests {
    use super::super::parse;

    /// 行首禁则:折行后任何一行的行首不能是标点
    #[test]
    fn kinsoku_no_punctuation_at_line_start() {
        // 24 个汉字恰好填满一行,逗号将被禁则处理:前一字符带到第二行行首
        let text = format!("{},后续内容继续排列显示", "一".repeat(24));
        let wrapped = super::wrap_lines(parse::parse_logical_lines(&text));
        assert!(wrapped.0.len() >= 2, "应当折行");
        for line in &wrapped.0 {
            if let Some(c) = line.runs.first().and_then(|r| r.text.chars().next()) {
                assert!(!super::is_no_line_start(c), "行首出现禁则标点: {c}");
            }
        }
    }
}

#[cfg(test)]
mod more_tests {
    use super::super::{parse, to_plain_text, to_styled_lines};

    /// 基础子集:加粗/颜色/字号/br/实体/未知标签剥除
    #[test]
    fn subset_parse_and_markup() {
        let p = parse(
            "<b>紧急</b> 普通 <font color=\"#d93025\">红</font><br>第二行 <i>斜体剥除</i> &amp; 实体",
        );
        let lines = to_styled_lines(&p);
        assert_eq!(lines.len(), 2, "br 应产生两行");
        assert!(lines[0].0.contains("**紧急**"), "加粗标记: {}", lines[0].0);
        assert!(
            lines[0].0.contains("<font color=\"#d93025\">红</font>"),
            "颜色标记: {}",
            lines[0].0
        );
        assert!(!lines[0].0.contains("<i>"), "未知标签应剥除");
        assert!(!lines[1].0.contains("斜体剥除</i>"));
        assert!(lines[1].0.contains("斜体剥除"), "剥除后内文保留");
        assert!(lines[1].0.contains("& 实体"), "实体应解码: {}", lines[1].0);
        assert!(lines[0].1.is_none(), "无显式字号时为 None");
    }

    /// 字号按行生效;超范围字号忽略
    #[test]
    fn font_size_per_line_and_clamp() {
        let p = parse("<font size=\"17\">大字行</font><br><font size=\"99\">非法字号回落</font>");
        let lines = to_styled_lines(&p);
        assert_eq!(lines[0].1, Some(17));
        assert_eq!(lines[1].1, None, "超范围字号应忽略");
    }

    /// 超过 `MAX_LINES` 截断加省略号
    #[test]
    fn truncate_with_ellipsis() {
        let text = std::array::from_fn::<_, 8, _>(|_| "很长的一行内容呀".repeat(3)).join("<br>");
        let p = parse(&text);
        let lines = to_styled_lines(&p);
        assert_eq!(lines.len(), super::MAX_LINES);
        assert!(lines.last().unwrap().0.ends_with('…'));
    }

    /// 纯文本提取:标签全剥、行按 \n 连接
    #[test]
    fn plain_text_extraction() {
        let t = to_plain_text("<b>A</b><font color=\"red\">B</font><br>C &lt;D&gt;");
        assert_eq!(t, "AB\nC <D>");
    }
}
