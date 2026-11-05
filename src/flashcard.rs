//! 闪卡（`BlockKind::Flashcard`）：问答卡与挖空卡，用 FSRS（rs-fsrs）安排复习。
//!
//! - 正面在块的 text 槽，背面在 `back`；正面里写 `{{c1::答案}}` / `{{c1::答案::提示}}` 就是挖空卡，
//!   每个序号（c1、c2…）各算一张卡，分别排程；没有挖空就是一张问答卡（序号 0）。
//! - 复习状态（[`Sched`]）存在块里，跟着文档走（备份、导出都在）。
//! - 复习队列由工作区扫描所有含闪卡的文档生成，见 workspace/review.rs。

use chrono::{DateTime, TimeZone, Utc};
use rs_fsrs::{Card, Rating, State, FSRS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// 一张卡的复习状态（rs-fsrs `Card` 的可序列化版本，时间为毫秒时间戳）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sched {
    pub due: i64,
    #[serde(default)]
    pub stability: f64,
    #[serde(default)]
    pub difficulty: f64,
    #[serde(default)]
    pub elapsed_days: i64,
    #[serde(default)]
    pub scheduled_days: i64,
    #[serde(default)]
    pub reps: i32,
    #[serde(default)]
    pub lapses: i32,
    /// 0 新卡 1 学习中 2 复习 3 重新学习
    #[serde(default)]
    pub state: u8,
    #[serde(default)]
    pub last: i64,
}

fn ms(t: DateTime<Utc>) -> i64 {
    t.timestamp_millis()
}

fn at(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(Utc::now)
}

impl Sched {
    fn to_card(&self) -> Card {
        Card {
            due: at(self.due),
            stability: self.stability,
            difficulty: self.difficulty,
            elapsed_days: self.elapsed_days,
            scheduled_days: self.scheduled_days,
            reps: self.reps,
            lapses: self.lapses,
            state: match self.state {
                1 => State::Learning,
                2 => State::Review,
                3 => State::Relearning,
                _ => State::New,
            },
            last_review: at(self.last),
        }
    }

    fn from_card(c: &Card) -> Self {
        Sched {
            due: ms(c.due),
            stability: c.stability,
            difficulty: c.difficulty,
            elapsed_days: c.elapsed_days,
            scheduled_days: c.scheduled_days,
            reps: c.reps,
            lapses: c.lapses,
            state: c.state as u8,
            last: ms(c.last_review),
        }
    }
}

/// 评分（1 重来 2 困难 3 良好 4 简单）。
pub fn rating(n: u8) -> Rating {
    match n {
        1 => Rating::Again,
        2 => Rating::Hard,
        4 => Rating::Easy,
        _ => Rating::Good,
    }
}

/// 按评分算出新的复习状态。`cur` 为 None 表示新卡。
pub fn review(cur: Option<&Sched>, r: u8, now_ms: i64) -> Sched {
    let now = at(now_ms);
    let card = cur.map(Sched::to_card).unwrap_or_else(|| Card { due: now, last_review: now, ..Default::default() });
    let info = FSRS::default().next(card, now, rating(r));
    Sched::from_card(&info.card)
}

/// 四个评分各自的下次复习间隔（给按钮上显示「10 分钟 / 3 天」）。
pub fn preview(cur: Option<&Sched>, now_ms: i64) -> [i64; 4] {
    let mut out = [0; 4];
    for (i, r) in (1..=4u8).enumerate() {
        out[i] = review(cur, r, now_ms).due - now_ms;
    }
    out
}

/// 间隔的人话写法：「< 1 分钟」「10 分钟」「3 小时」「4 天」「2.5 个月」「1.2 年」。
pub fn fmt_interval(ms: i64) -> String {
    let m = ms as f64 / 60_000.;
    if m < 1. {
        "< 1 分钟".into()
    } else if m < 60. {
        format!("{} 分钟", m.round() as i64)
    } else if m < 60. * 24. {
        format!("{} 小时", (m / 60.).round() as i64)
    } else if m < 60. * 24. * 30. {
        format!("{} 天", (m / 1440.).round() as i64)
    } else if m < 60. * 24. * 365. {
        format!("{:.1} 个月", m / 43200.)
    } else {
        format!("{:.1} 年", m / 525600.)
    }
}

/// 一处挖空：(在文本里的字节区间, 序号, 答案, 提示)
#[derive(Debug, PartialEq)]
pub struct Cloze {
    pub range: std::ops::Range<usize>,
    pub n: u8,
    pub answer: String,
    pub hint: String,
}

/// 解析 `{{c1::答案}}` / `{{c1::答案::提示}}`。
pub fn clozes(text: &str) -> Vec<Cloze> {
    let mut out = vec![];
    let mut i = 0;
    while let Some(off) = text[i..].find("{{c") {
        let s = i + off;
        let rest = &text[s + 3..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let after = s + 3 + digits.len();
        let (Ok(n), true) = (digits.parse::<u8>(), text[after..].starts_with("::")) else {
            i = s + 3;
            continue;
        };
        let body_start = after + 2;
        let Some(end) = text[body_start..].find("}}") else { break };
        let body = &text[body_start..body_start + end];
        let (answer, hint) = body.split_once("::").unwrap_or((body, ""));
        out.push(Cloze { range: s..body_start + end + 2, n, answer: answer.to_string(), hint: hint.to_string() });
        i = body_start + end + 2;
    }
    out
}

/// 一个闪卡块里有哪些卡：挖空序号；没有挖空时为 `[0]`（问答卡）。
pub fn card_numbers(front: &str) -> Vec<u8> {
    let set: BTreeSet<u8> = clozes(front).into_iter().map(|c| c.n).collect();
    if set.is_empty() {
        vec![0]
    } else {
        set.into_iter().collect()
    }
}

/// 复习时显示的正面：第 n 处挖空遮住（显示提示或 `[…]`），其余挖空直接显示答案；`reveal` 时全部显示。
/// 返回 (文字, 被遮住 / 揭晓部分的字节区间)，界面据此高亮。
pub fn render_front(front: &str, n: u8, reveal: bool) -> (String, Vec<std::ops::Range<usize>>) {
    let mut out = String::new();
    let mut marks = vec![];
    let mut last = 0;
    for c in clozes(front) {
        out.push_str(&front[last..c.range.start]);
        let start = out.len();
        if c.n == n && !reveal {
            out.push_str(&if c.hint.is_empty() { "[…]".to_string() } else { format!("[{}]", c.hint) });
        } else {
            out.push_str(&c.answer);
        }
        if c.n == n {
            marks.push(start..out.len());
        }
        last = c.range.end;
    }
    out.push_str(&front[last..]);
    (out, marks)
}

/// 正文里展示挖空卡时的纯文本（去掉 `{{c1::…}}` 记号，只留答案）。
pub fn strip_clozes(front: &str) -> String {
    render_front(front, u8::MAX, true).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clozes() {
        let t = "首都是{{c1::北京}}，人口约{{c2::2100 万::数字}}。";
        let c = clozes(t);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].n, c[0].answer.as_str(), c[0].hint.as_str()), (1, "北京", ""));
        assert_eq!((c[1].n, c[1].answer.as_str(), c[1].hint.as_str()), (2, "2100 万", "数字"));
        assert_eq!(card_numbers(t), vec![1, 2]);
        assert_eq!(card_numbers("普通问题"), vec![0]);
        assert!(clozes("{{c::x}} {{cx::y}} {{c1:z}}").is_empty());
    }

    #[test]
    fn front_masks_only_target() {
        let t = "A{{c1::甲}}B{{c2::乙::提示}}C";
        assert_eq!(render_front(t, 1, false).0, "A[…]B乙C");
        assert_eq!(render_front(t, 2, false).0, "A甲B[提示]C");
        let (s, m) = render_front(t, 2, true);
        assert_eq!(s, "A甲B乙C");
        assert_eq!(&s[m[0].clone()], "乙");
        assert_eq!(strip_clozes(t), "A甲B乙C");
    }

    #[test]
    fn scheduling_moves_due_forward() {
        let now = 1_800_000_000_000;
        let s = review(None, 3, now);
        assert!(s.due > now);
        assert_eq!(s.reps, 1);
        let again = review(Some(&s), 1, s.due);
        assert!(again.lapses >= s.lapses);
        let p = preview(Some(&s), s.due);
        assert!(p[0] <= p[1] && p[1] <= p[2] && p[2] <= p[3], "{p:?}");
        let back: Sched = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn interval_text() {
        assert_eq!(fmt_interval(30_000), "< 1 分钟");
        assert_eq!(fmt_interval(10 * 60_000), "10 分钟");
        assert_eq!(fmt_interval(3 * 86_400_000), "3 天");
    }
}
