//! 文字的样式分段（画板：选中几个字单独加粗、斜体、变色）。
//!
//! 分段为空表示整段文字使用元素自身的样式；不为空时各段长度（字节）之和等于文字长度，
//! 每段完整给出加粗与修饰位，颜色为 None 时使用元素的文字颜色。
//! 编辑过程中由 [`splice`] 跟随文字增删，样式与元素一致的分段由 [`normalize`] 合并清空。

use super::Col;
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Fmt {
    #[serde(default)]
    pub bold: bool,
    /// 斜体 / 下划线 / 删除线（`DECO_*` 位）
    #[serde(default)]
    pub deco: u8,
    /// None：使用元素的文字颜色
    #[serde(default)]
    pub color: Option<Col>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub len: usize,
    #[serde(flatten)]
    pub fmt: Fmt,
}

/// 分段是否与文字长度对得上（对不上时按无分段处理）。
pub fn valid(spans: &[Span], len: usize) -> bool {
    !spans.is_empty() && spans.iter().map(|s| s.len).sum::<usize>() == len
}

/// 解析成覆盖全文的 (长度, 样式)；没有分段或分段失效时整段用 `base`。
pub fn resolve(spans: &[Span], len: usize, base: Fmt) -> Vec<(usize, Fmt)> {
    if valid(spans, len) {
        spans.iter().map(|s| (s.len, s.fmt)).collect()
    } else {
        vec![(len, base)]
    }
}

/// 位置 `pos` 处新输入文字应继承的样式：取前一个字的样式（在开头时取第一个字）。
pub fn fmt_at(spans: &[Span], len: usize, pos: usize, base: Fmt) -> Fmt {
    if !valid(spans, len) {
        return base;
    }
    let probe = pos.saturating_sub(1);
    let mut at = 0;
    for s in spans {
        if probe < at + s.len {
            return s.fmt;
        }
        at += s.len;
    }
    spans.last().map(|s| s.fmt).unwrap_or(base)
}

/// 把 `range` 替换成 `new_len` 字节的新文字，新文字使用 `fmt`。
pub fn splice(spans: &mut Vec<Span>, old_len: usize, range: Range<usize>, new_len: usize, fmt: Fmt) {
    if !valid(spans, old_len) {
        spans.clear();
        return;
    }
    let mut out: Vec<Span> = Vec::with_capacity(spans.len() + 2);
    let mut at = 0;
    let mut inserted = false;
    for s in spans.iter() {
        let (s0, s1) = (at, at + s.len);
        at = s1;
        // 区间前的部分
        if s0 < range.start {
            out.push(Span { len: s1.min(range.start) - s0, fmt: s.fmt });
        }
        if !inserted && s1 >= range.start {
            out.push(Span { len: new_len, fmt });
            inserted = true;
        }
        // 区间后的部分
        if s1 > range.end {
            out.push(Span { len: s1 - s0.max(range.end), fmt: s.fmt });
        }
    }
    if !inserted {
        out.push(Span { len: new_len, fmt });
    }
    *spans = merge(out);
}

/// 对 `range` 内的文字套用 `f`（没有分段时先按 `base` 展开）。
pub fn apply(spans: &mut Vec<Span>, len: usize, base: Fmt, range: Range<usize>, f: impl Fn(&mut Fmt)) {
    if !valid(spans, len) {
        *spans = vec![Span { len, fmt: base }];
    }
    let mut out = Vec::with_capacity(spans.len() + 2);
    let mut at = 0;
    for s in spans.iter() {
        let (s0, s1) = (at, at + s.len);
        at = s1;
        for (a, b, inside) in [(s0, s1.min(range.start), false), (s0.max(range.start), s1.min(range.end), true), (s0.max(range.end), s1, false)] {
            if b > a {
                let mut fmt = s.fmt;
                if inside {
                    f(&mut fmt);
                }
                out.push(Span { len: b - a, fmt });
            }
        }
    }
    *spans = merge(out);
}

/// 合并相邻同样式的分段；全部与 `base` 相同时清空（整段用元素样式）。
pub fn normalize(spans: &mut Vec<Span>, len: usize, base: Fmt) {
    if !valid(spans, len) || spans.iter().all(|s| s.fmt == base) {
        spans.clear();
        return;
    }
    *spans = merge(std::mem::take(spans));
}

/// `range` 内的文字是否全部满足 `pred`（用于切换加粗等：全部加粗时取消，否则加粗）。
pub fn all(spans: &[Span], len: usize, base: Fmt, range: Range<usize>, pred: impl Fn(&Fmt) -> bool) -> bool {
    let mut at = 0;
    for (l, fmt) in resolve(spans, len, base) {
        let (s0, s1) = (at, at + l);
        at = s1;
        if s1 > range.start && s0 < range.end && !pred(&fmt) {
            return false;
        }
    }
    true
}

fn merge(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for s in spans {
        if s.len == 0 {
            continue;
        }
        match out.last_mut() {
            Some(l) if l.fmt == s.fmt => l.len += s.len,
            _ => out.push(s),
        }
    }
    out
}

/// 按段落（`\n`）切开：返回每段的 [(长度, 样式)]，不含换行符本身。
pub fn paragraphs(text: &str, segs: &[(usize, Fmt)]) -> Vec<(String, Vec<(usize, Fmt)>)> {
    let mut out = vec![];
    let mut pos = 0;
    for para in text.split('\n') {
        let (p0, p1) = (pos, pos + para.len());
        pos = p1 + 1;
        let mut runs = vec![];
        let mut at = 0;
        for (l, fmt) in segs {
            let (s0, s1) = (at, at + l);
            at = s1;
            let (a, b) = (s0.max(p0), s1.min(p1));
            if b > a {
                runs.push((b - a, *fmt));
            }
        }
        if runs.is_empty() {
            // 空段落：用前一个字的样式占位（行高一致）
            let fmt = segs.iter().scan(0, |acc, (l, f)| {
                *acc += l;
                Some((*acc, *f))
            });
            let f = fmt.filter(|(end, _)| *end <= p0.max(1)).last().map(|(_, f)| f).or(segs.first().map(|s| s.1)).unwrap_or_default();
            runs.push((0, f));
        }
        out.push((para.to_string(), runs));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b() -> Fmt {
        Fmt { bold: true, ..Default::default() }
    }

    #[test]
    fn apply_and_normalize() {
        let mut s = vec![];
        apply(&mut s, 6, Fmt::default(), 2..4, |f| f.bold = true);
        assert_eq!(s.iter().map(|s| s.len).collect::<Vec<_>>(), vec![2, 2, 2]);
        assert!(all(&s, 6, Fmt::default(), 2..4, |f| f.bold));
        assert!(!all(&s, 6, Fmt::default(), 1..4, |f| f.bold));
        apply(&mut s, 6, Fmt::default(), 0..6, |f| f.bold = false);
        normalize(&mut s, 6, Fmt::default());
        assert!(s.is_empty());
    }

    #[test]
    fn splice_follows_edits() {
        let mut s = vec![Span { len: 3, fmt: Fmt::default() }, Span { len: 3, fmt: b() }];
        // 在加粗段里输入：继承加粗
        let f = fmt_at(&s, 6, 5, Fmt::default());
        splice(&mut s, 6, 5..5, 3, f);
        assert_eq!(s, vec![Span { len: 3, fmt: Fmt::default() }, Span { len: 6, fmt: b() }]);
        // 删除跨两段的文字
        splice(&mut s, 9, 1..5, 0, Fmt::default());
        assert_eq!(s, vec![Span { len: 1, fmt: Fmt::default() }, Span { len: 4, fmt: b() }]);
        // 替换全部
        splice(&mut s, 5, 0..5, 2, b());
        assert_eq!(s, vec![Span { len: 2, fmt: b() }]);
    }

    #[test]
    fn paragraphs_split() {
        let segs = [(4, Fmt::default()), (3, b())];
        let p = paragraphs("ab\ncde", &segs[..]);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].1, vec![(2, Fmt::default())]);
        assert_eq!(p[1].1, vec![(1, Fmt::default()), (2, b())]);
    }
}
