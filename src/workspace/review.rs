//! 闪卡复习面板（⇧⌘R / 命令面板「复习闪卡」）：扫描所有文档里到期的卡，逐张复习，按 FSRS 排下次时间。
//!
//! 评分后把新的复习状态写回卡片所在的文档：当前打开的文档走编辑器（保证界面同步），其他文档直接读改存。

use super::*;
use crate::flashcard::{card_numbers, fmt_interval, preview, render_front, Sched};
use crate::model::RichText;

/// 每次最多引入的新卡数
const NEW_PER_SESSION: usize = 20;

#[derive(Clone)]
pub struct Item {
    doc_id: String,
    doc_title: String,
    card_id: String,
    n: u8,
    front: String,
    back: RichText,
    sched: Option<Sched>,
}

pub struct Review {
    queue: Vec<Item>,
    revealed: bool,
    done: usize,
    focus: FocusHandle,
}

/// 收集到期的卡：已到期的按到期时间排在前，新卡随后（最多 NEW_PER_SESSION 张）。
fn collect(docs: Vec<(String, String, String)>, now: i64) -> Vec<Item> {
    let mut due = vec![];
    let mut fresh = vec![];
    for (doc_id, title, json) in docs {
        let Ok(content) = DocContent::try_from_json(&json) else { continue };
        for b in &content.blocks {
            let BlockKind::Flashcard { back, card_id, sched } = &b.kind else { continue };
            if card_id.is_empty() || b.text.text.trim().is_empty() {
                continue;
            }
            for n in card_numbers(&b.text.text) {
                let s = sched.get(&n.to_string()).cloned();
                let it = Item {
                    doc_id: doc_id.clone(),
                    doc_title: title.clone(),
                    card_id: card_id.clone(),
                    n,
                    front: b.text.text.clone(),
                    back: back.clone(),
                    sched: s.clone(),
                };
                match s {
                    Some(s) if s.due <= now => due.push((s.due, it)),
                    Some(_) => {}
                    None => fresh.push(it),
                }
            }
        }
    }
    due.sort_by_key(|(d, _)| *d);
    let mut out: Vec<Item> = due.into_iter().map(|(_, it)| it).collect();
    out.extend(fresh.into_iter().take(NEW_PER_SESSION));
    out
}

impl Workspace {
    pub fn review_cards(&mut self, _: &ReviewCards, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.save_now(cx));
        let queue = collect(self.store.flashcard_docs(), crate::store::now_ms());
        if queue.is_empty() {
            self.show_toast("没有到期的闪卡。在文档里输入「/闪卡」添加".into(), cx);
            return;
        }
        let focus = cx.focus_handle();
        window.focus(&focus);
        self.review = Some(Review { queue, revealed: false, done: 0, focus });
        cx.notify();
    }

    fn close_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done = self.review.take().map_or(0, |r| r.done);
        window.focus(&self.editor.focus_handle(cx));
        if done > 0 {
            self.show_toast(format!("本次复习了 {done} 张卡"), cx);
        }
        cx.notify();
    }

    fn rate(&mut self, r: u8, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rv) = self.review.as_mut().filter(|rv| rv.revealed) else { return };
        let now = crate::store::now_ms();
        let mut it = rv.queue.remove(0);
        let s = crate::flashcard::review(it.sched.as_ref(), r, now);
        rv.revealed = false;
        rv.done += 1;
        // 短期内又到期（「重来」、学习阶段）的卡：放回队尾，这一轮里再见一次
        if s.due <= now + 20 * 60_000 {
            it.sched = Some(s.clone());
            rv.queue.push(it.clone());
        }
        self.save_sched(&it.doc_id, &it.card_id, it.n, s, cx);
        if self.review.as_ref().is_some_and(|rv| rv.queue.is_empty()) {
            self.close_review(window, cx);
        }
        cx.notify();
    }

    fn save_sched(&mut self, doc_id: &str, card_id: &str, n: u8, s: Sched, cx: &mut Context<Self>) {
        if doc_id == self.current {
            let (cid, s2) = (card_id.to_string(), s.clone());
            if self.editor.update(cx, |e, cx| e.set_card_sched(&cid, n, s2, cx)) {
                return;
            }
        }
        let Some((mut content, None)) = self.store.load(doc_id) else { return };
        for b in content.blocks.iter_mut() {
            if matches!(&b.kind, BlockKind::Flashcard { card_id: c, .. } if c == card_id) {
                if let BlockKind::Flashcard { sched, .. } = &mut std::sync::Arc::make_mut(b).kind {
                    sched.insert(n.to_string(), s);
                }
                break;
            }
        }
        self.store.save(doc_id, &content);
    }

    pub(super) fn render_review(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let rv = self.review.as_ref()?;
        let it = rv.queue.first()?;
        let t = theme(cx).clone();
        let now = crate::store::now_ms();
        let (front, marks) = render_front(&it.front, it.n, rv.revealed);
        let hl: Vec<(std::ops::Range<usize>, HighlightStyle)> = marks
            .into_iter()
            .map(|r| {
                (
                    r,
                    HighlightStyle {
                        color: Some(t.accent),
                        font_weight: Some(FontWeight::SEMIBOLD),
                        background_color: Some(t.accent_soft),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let ivals = preview(it.sched.as_ref(), now);
        let btn = |k: u8, label: &'static str, col: Hsla| {
            div()
                .id(("rv-rate", k as usize))
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.))
                .py(px(10.))
                .rounded(px(8.))
                .border_1()
                .border_color(t.border)
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .child(div().text_size(px(15.)).text_color(col).font_weight(FontWeight::MEDIUM).child(format!("{} {}", k, crate::i18n::tr(label))))
                .child(div().text_size(px(12.)).text_color(t.text_muted).child(fmt_interval(ivals[k as usize - 1])))
                .on_click(cx.listener(move |this, _, window, cx| this.rate(k, window, cx)))
        };
        let kind = if it.n == 0 { "问答".to_string() } else { format!("挖空 c{}", it.n) };
        Some(
            div()
                .id("review")
                .absolute()
                .inset_0()
                .occlude()
                .track_focus(&rv.focus)
                .bg(t.bg.opacity(0.97))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    let revealed = this.review.as_ref().is_some_and(|r| r.revealed);
                    match ev.keystroke.key.as_str() {
                        "escape" => this.close_review(window, cx),
                        "space" | "enter" if !revealed => {
                            if let Some(r) = this.review.as_mut() {
                                r.revealed = true;
                            }
                            cx.notify();
                        }
                        "space" | "enter" => this.rate(3, window, cx),
                        k @ ("1" | "2" | "3" | "4") if revealed => this.rate(k.parse().unwrap_or(3), window, cx),
                        _ => {}
                    }
                }))
                .child(
                    div()
                        .w(px(640.))
                        .max_w(relative(0.9))
                        .flex()
                        .flex_col()
                        .gap(px(18.))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(10.))
                                .text_size(px(12.))
                                .text_color(t.text_muted)
                                .child(format!("{} · {}", if it.doc_title.trim().is_empty() { "未命名文档" } else { &it.doc_title }, kind))
                                .child(div().flex_1())
                                .child(format!("剩余 {} · 已复习 {}", rv.queue.len(), rv.done)),
                        )
                        .child(
                            div()
                                .p(px(28.))
                                .rounded(px(14.))
                                .border_1()
                                .border_color(t.border)
                                .bg(t.panel_bg)
                                .shadow_lg()
                                .flex()
                                .flex_col()
                                .gap(px(18.))
                                .child(div().text_size(px(22.)).line_height(px(34.)).text_color(t.text).child(StyledText::new(front).with_highlights(hl)))
                                .when(rv.revealed && !it.back.is_empty(), |d| {
                                    d.child(div().h(px(1.)).bg(t.border)).child(
                                        div().text_size(px(19.)).line_height(px(30.)).text_color(t.text_secondary).child(super::present::rich(&it.back, &t)),
                                    )
                                }),
                        )
                        .child(if rv.revealed {
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(10.))
                                .child(btn(1, "重来", t.fg_palette[1]))
                                .child(btn(2, "困难", t.fg_palette[2]))
                                .child(btn(3, "良好", t.fg_palette[4]))
                                .child(btn(4, "简单", t.accent))
                                .into_any_element()
                        } else {
                            div()
                                .id("rv-show")
                                .py(px(12.))
                                .flex()
                                .justify_center()
                                .rounded(px(8.))
                                .bg(t.accent)
                                .text_color(gpui::white())
                                .text_size(px(15.))
                                .cursor_pointer()
                                .child(crate::i18n::tr("显示答案（空格）"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(r) = this.review.as_mut() {
                                        r.revealed = true;
                                    }
                                    cx.notify();
                                }))
                                .into_any_element()
                        })
                        .child(
                            div()
                                .flex()
                                .justify_center()
                                .text_size(px(12.))
                                .text_color(t.text_muted)
                                .child(crate::i18n::tr("空格显示答案 · 1–4 评分 · Esc 结束")),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::collect;
    use crate::model::{Block, BlockKind, DocContent, RichText};
    use std::sync::Arc;

    #[test]
    fn queue_has_due_then_new_and_skips_future() {
        let now = 1_800_000_000_000;
        let card = |front: &str, sched: Vec<(u8, i64)>| {
            Arc::new(Block::new(
                BlockKind::Flashcard {
                    back: RichText::plain("答"),
                    card_id: format!("id-{front}"),
                    sched: sched.into_iter().map(|(n, due)| (n.to_string(), crate::flashcard::Sched { due, ..Default::default() })).collect(),
                },
                RichText::plain(front),
            ))
        };
        let mut doc = DocContent::new_empty("卡片");
        doc.blocks.push(card("新问题", vec![]));
        doc.blocks.push(card("到期", vec![(0, now - 1000)]));
        doc.blocks.push(card("未到期", vec![(0, now + 86_400_000)]));
        doc.blocks.push(card("A{{c1::甲}}B{{c2::乙}}", vec![(1, now - 5000)]));
        let q = collect(vec![("d".into(), "卡片".into(), doc.to_json())], now);
        let names: Vec<(String, u8)> = q.iter().map(|i| (i.front.clone(), i.n)).collect();
        assert_eq!(names, vec![("A{{c1::甲}}B{{c2::乙}}".into(), 1), ("到期".into(), 0), ("新问题".into(), 0), ("A{{c1::甲}}B{{c2::乙}}".into(), 2)]);
    }
}
