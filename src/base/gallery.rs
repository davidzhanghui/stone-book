//! 画册视图：每条记录一张卡片，附件字段里的第一张图片做封面；按行虚拟化，几千条记录也流畅。

use super::editor::BaseEditor;
use super::model::*;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;

impl BaseEditor {
    /// 画册卡片的固定高度（虚拟列表要求每行等高）。
    fn gallery_card_h(&self, v: &View) -> f32 {
        let n = (self.visible().len().saturating_sub(1) as f32
            - if v.cover.is_some() && self.visible().iter().any(|f| Some(&f.id) == v.cover.as_ref()) { 1. } else { 0. })
        .min(4.);
        let cover = if v.cover.is_some() {
            if v.compact {
                120.
            } else {
                168.
            }
        } else {
            0.
        };
        if v.compact {
            cover + 16. + 22. + 4. + 26. * n.min(3.).max(0.)
        } else {
            cover + 24. + 22. + 8. * (n + 1.) + 40. * n.max(0.)
        }
    }

    pub(super) fn render_gallery(&mut self, t: &Theme, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let v = self.view();
        let rows = self.rows();
        let width = self.frame.borrow().get("gallery").map(|b| f32::from(b.size.width)).unwrap_or(1000.);
        let card_w = if v.compact { 200. } else { 248. };
        let gap = 16.;
        let per = (((width - 32. + gap) / (card_w + gap)).floor() as usize).max(1);
        let card_h = self.gallery_card_h(&v);
        // 最后多一个「+」卡片
        let n = rows.len() + 1;
        let lines = n.div_ceil(per);
        let frame = self.frame.clone();
        let list = uniform_list(
            "gallery-list",
            lines,
            cx.processor(move |this: &mut BaseEditor, range: std::ops::Range<usize>, _, cx| {
                let t = crate::theme::theme(cx).clone();
                let v = this.view();
                let rows = this.rows();
                range
                    .map(|line| {
                        let mut row = div().flex().flex_row().gap(px(gap)).px(px(16.)).pt(px(gap)).h(px(card_h + gap));
                        for i in line * per..((line + 1) * per).min(rows.len() + 1) {
                            if i == rows.len() {
                                row = row.child(
                                    div()
                                        .id("g-add")
                                        .w(px(card_w))
                                        .h(px(card_h))
                                        .flex_none()
                                        .rounded(px(8.))
                                        .border_1()
                                        .border_dashed()
                                        .border_color(t.border_strong)
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(6.))
                                        .text_size(px(13.))
                                        .text_color(t.text_muted)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(t.hover))
                                        .child(icon("plus", 20., t.text_muted))
                                        .child("新建记录")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(rid) = this.add_record(vec![], None, cx) {
                                                this.detail = Some(rid);
                                            }
                                        })),
                                );
                                continue;
                            }
                            let rid = rows[i].clone();
                            let card = this.render_card(&rid, &v, None, card_w, &t, cx);
                            row = row.child(
                                div()
                                    .id(SharedString::from(format!("gc-{rid}")))
                                    .w(px(card_w))
                                    .h(px(card_h))
                                    .flex_none()
                                    .overflow_hidden()
                                    .rounded(px(8.))
                                    .cursor_pointer()
                                    .hover(|s| s.shadow_md())
                                    .child(card.h_full())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.detail = Some(rid.clone());
                                        cx.notify();
                                    })),
                            );
                        }
                        row
                    })
                    .collect()
            }),
        )
        .size_full()
        .track_scroll(self.gallery_scroll.clone());
        div()
            .size_full()
            .relative()
            .child(list)
            .child(
                canvas(
                    move |b, _, _| {
                        frame.borrow_mut().insert("gallery".into(), b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(rows.is_empty() && (!self.search.is_empty() || !v.filter.conds.is_empty()), |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(40.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .text_color(t.text_muted)
                        .text_size(px(13.))
                        .child("没有符合条件的记录"),
                )
            })
            .when(v.cover.is_none(), |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(16.))
                        .right(px(16.))
                        .px(px(12.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .bg(t.accent_soft)
                        .text_size(px(12.))
                        .text_color(t.accent)
                        .child("在「卡片配置」里选择一个附件字段作为封面"),
                )
            })
    }
}
