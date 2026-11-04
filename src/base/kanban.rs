//! 看板视图：按分组字段把记录分成列；拖动卡片换列 = 修改分组字段的值，列内拖动 = 调整手动顺序。
//! 支持单选 / 多选 / 流程 / 人员 / 复选框 / 评分 / 关联字段分组，列可折叠、重命名、新增。

use super::cells::Mode;
use super::editor::{BaseEditor, Drag, Popup};
use super::model::*;
use super::widgets::*;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde_json::Value;
use std::path::PathBuf;

const COL_W: f32 = 280.;

pub(super) struct KCol {
    pub key: String,
    pub label: String,
    pub color: Option<u8>,
    pub cards: Vec<String>,
}

impl BaseEditor {
    /// 看板列（含空列）；分组字段缺失时返回 None。
    pub(super) fn kanban_columns(&mut self) -> Option<(Field, Vec<KCol>)> {
        let v = self.view();
        let f = v.group_field.as_ref().and_then(|g| self.field(g))?;
        let rows = self.rows();
        let b = self.b();
        let mut cols: Vec<KCol> = vec![KCol { key: String::new(), label: "未分组".into(), color: None, cards: vec![] }];
        match f.kind {
            FieldType::Select | FieldType::Stage | FieldType::MultiSelect => {
                cols.extend(f.prop.options.iter().map(|o| KCol { key: o.id.clone(), label: o.name.clone(), color: Some(o.color), cards: vec![] }))
            }
            FieldType::Checkbox => {
                cols.clear();
                cols.push(KCol { key: "0".into(), label: "未勾选".into(), color: Some(11), cards: vec![] });
                cols.push(KCol { key: "1".into(), label: "已勾选".into(), color: Some(3), cards: vec![] });
            }
            FieldType::Rating => {
                let max = if f.prop.max > 0. { f.prop.max as i32 } else { 5 };
                cols.extend((1..=max).map(|i| KCol { key: i.to_string(), label: format!("{} 分", i), color: Some(4), cards: vec![] }));
            }
            _ => {}
        }
        for r in rows.iter() {
            for (k, label, color) in b.group_keys(&self.tid, r, &f.id, true) {
                let k = if f.kind == FieldType::Rating { k.trim_end_matches(".0").to_string() } else { k };
                match cols.iter_mut().find(|c| c.key == k) {
                    Some(c) => c.cards.push(r.clone()),
                    None => cols.push(KCol { key: k, label, color, cards: vec![r.clone()] }),
                }
            }
        }
        // 「未分组」没有卡片时不显示（复选框除外）
        cols.retain(|c| !c.key.is_empty() || !c.cards.is_empty());
        Some((f, cols))
    }

    /// 把卡片移到某列的某个位置（before：插到哪张卡片之前）。
    pub(super) fn move_card(&mut self, rid: &str, from: Option<&str>, to: &str, before: Option<&str>, cx: &mut Context<Self>) {
        let v = self.view();
        let Some(f) = v.group_field.as_ref().and_then(|g| self.field(g)) else { return };
        let tid = self.tid.clone();
        let cur = self.b().record(&tid, rid).and_then(|r| r.cells.get(&f.id)).cloned();
        let val = match f.kind {
            FieldType::Select | FieldType::Stage => {
                if to.is_empty() {
                    Value::Null
                } else {
                    Value::from(to)
                }
            }
            FieldType::Checkbox => Value::Bool(to == "1"),
            FieldType::Rating => to.parse::<f64>().map(Value::from).unwrap_or(Value::Null),
            _ => {
                let mut list = str_list(cur.as_ref());
                if to.is_empty() {
                    list.clear();
                } else {
                    if let Some(fr) = from {
                        list.retain(|x| x != fr);
                    }
                    if !list.iter().any(|x| x == to) {
                        list.push(to.to_string());
                    }
                }
                Value::from(list)
            }
        };
        self.bm().set_cell(&tid, rid, &f.id, val);
        // 列内位置：取前后两张卡片的 rank 中点
        if v.sorts.is_empty() {
            if let Some((_, cols)) = self.kanban_columns() {
                if let Some(col) = cols.iter().find(|c| c.key == to) {
                    let list: Vec<&String> = col.cards.iter().filter(|r| r.as_str() != rid).collect();
                    let i = before.and_then(|b| list.iter().position(|r| r.as_str() == b)).unwrap_or(list.len());
                    let rank = |r: Option<&&String>| r.and_then(|r| self.b().record(&tid, r).map(|x| x.rank));
                    let new_rank = rank_between(if i > 0 { rank(list.get(i - 1)) } else { None }, rank(list.get(i)));
                    let rec = self.b().record(&tid, rid).cloned();
                    if let Some(mut r) = rec {
                        r.rank = new_rank;
                        self.bm().put_record(&tid, r);
                    }
                }
            }
        }
        self.commit(cx);
    }

    /// 拖动中：根据上一帧记录的列 / 卡片位置算出落点 (列键, 插到哪张卡之前)。
    fn kanban_target(&self, pos: Point<Pixels>) -> Option<(String, Option<String>)> {
        let Some(Drag::Card { rid, .. }) = &self.drag else { return None };
        let frame = self.frame.borrow();
        let (key, _) = frame.iter().filter_map(|(k, b)| k.strip_prefix("kcol:").map(|k| (k, b))).find(|(_, b)| pos.x >= b.left() && pos.x <= b.right())?;
        let mut cards: Vec<(&str, &Bounds<Pixels>)> =
            frame.iter().filter_map(|(k, b)| k.strip_prefix(&format!("kcard:{key}:")).map(|r| (r, b))).filter(|(r, _)| *r != rid.as_str()).collect();
        cards.sort_by(|a, b| a.1.top().partial_cmp(&b.1.top()).unwrap_or(std::cmp::Ordering::Equal));
        let before = cards.iter().find(|(_, b)| pos.y < b.top() + b.size.height / 2.).map(|(r, _)| r.to_string());
        Some((key.to_string(), before))
    }

    pub(super) fn kanban_drag_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(Drag::Card { start, moved, .. }) = &mut self.drag {
            if !*moved && (pos - *start).magnitude() > 5. {
                *moved = true;
            }
        }
        // 靠近左右边缘时自动横向滚动
        if let Some(b) = self.frame.borrow().get("kanban").copied() {
            let off = self.kanban_scroll.offset();
            if pos.x > b.right() - px(60.) {
                self.kanban_scroll.set_offset(point(off.x - px(14.), off.y));
            } else if pos.x < b.left() + px(60.) && off.x < px(0.) {
                self.kanban_scroll.set_offset(point((off.x + px(14.)).min(px(0.)), off.y));
            }
        }
        cx.notify();
    }

    pub(super) fn kanban_drop(&mut self, d: Drag, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Drag::Card { rid, moved, .. } = &d else { return };
        if !*moved {
            self.detail = Some(rid.clone());
            cx.notify();
            return;
        }
        self.drag = Some(d);
        let target = self.kanban_target(pos);
        let Some(Drag::Card { rid, from, .. }) = self.drag.take() else { return };
        if let Some((to, before)) = target {
            self.move_card(&rid, Some(&from), &to, before.as_deref(), cx);
        }
    }

    /// 卡片（看板 / 画册共用）。
    pub(super) fn render_card(&mut self, rid: &str, v: &View, skip: Option<&str>, w: f32, t: &Theme, cx: &mut Context<Self>) -> Div {
        let tid = self.tid.clone();
        let title = self.b().title(&tid, rid);
        let untitled = self.b().primary_text(&tid, rid).trim().is_empty();
        let cover = v.cover.as_ref().and_then(|c| {
            let b = self.b();
            attachments(b.record(&tid, rid).and_then(|r| r.cells.get(c))).into_iter().find(|a| a.is_image())
        });
        let gallery_limit = if v.kind == ViewKind::Gallery { 4 } else { usize::MAX };
        // 画册卡片只显示前 4 个字段（在「卡片配置」里调整顺序 / 显隐）
        let fields: Vec<Field> =
            self.visible().into_iter().skip(1).filter(|f| Some(f.id.as_str()) != skip && Some(&f.id) != v.cover.as_ref()).take(gallery_limit).collect();
        let compact = v.compact;
        let cover_h = if v.kind == ViewKind::Gallery {
            if compact {
                120.
            } else {
                168.
            }
        } else if compact {
            96.
        } else {
            140.
        };
        let gallery = v.kind == ViewKind::Gallery;
        let mut body = div().flex().flex_col().gap(px(if compact { 4. } else { 8. })).p(px(if compact { 8. } else { 12. }));
        body = body.child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if untitled { t.text_muted } else { t.text })
                .when(gallery, |d| d.overflow_hidden().whitespace_nowrap().text_ellipsis())
                .child(title),
        );
        for f in fields {
            let empty = {
                let b = self.b();
                b.fval(&tid, rid, &f.id).is_null() && !f.kind.computed()
            };
            if empty && (compact || !gallery) {
                continue;
            }
            let content = self.cell_content(rid, &f, if gallery { Mode::Grid } else { Mode::Card }, t, cx);
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(12.5))
                    .when(!compact, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(icon(f.kind.icon(), 11., t.text_muted))
                                .child(f.name.clone()),
                        )
                    })
                    .child(if empty {
                        div().h(px(20.)).text_color(t.placeholder).child("—").into_any_element()
                    } else {
                        div().max_h(px(66.)).when(gallery, |d| d.h(px(22.)).flex().items_center()).overflow_hidden().child(content).into_any_element()
                    }),
            );
        }
        let has_cover_field = v.cover.is_some();
        div()
            .w(px(w))
            .flex()
            .flex_col()
            .rounded(px(8.))
            .overflow_hidden()
            .bg(t.panel_bg)
            .border_1()
            .border_color(t.border)
            .shadow_sm()
            .when(gallery && has_cover_field, |d| {
                d.child(match &cover {
                    Some(a) => div()
                        .h(px(cover_h))
                        .flex_none()
                        .w_full()
                        .overflow_hidden()
                        .bg(t.sidebar_bg)
                        .child(img(PathBuf::from(a.preview())).size_full().object_fit(if v.cover_fill { ObjectFit::Cover } else { ObjectFit::Contain }))
                        .into_any_element(),
                    None => div()
                        .h(px(cover_h))
                        .flex_none()
                        .w_full()
                        .overflow_hidden()
                        .bg(t.sidebar_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon("image", 28., t.border_strong))
                        .into_any_element(),
                })
            })
            .when(!gallery, |d| {
                d.when_some(cover, |d, a| {
                    d.child(
                        div()
                            .h(px(cover_h))
                            .flex_none()
                            .w_full()
                            .overflow_hidden()
                            .bg(t.sidebar_bg)
                            .child(img(PathBuf::from(a.preview())).size_full().object_fit(if v.cover_fill { ObjectFit::Cover } else { ObjectFit::Contain })),
                    )
                })
            })
            .child(body)
    }

    pub(super) fn render_kanban(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let v = self.view();
        let Some((f, cols)) = self.kanban_columns() else {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .text_color(t.text_muted)
                .child("看板需要一个分组字段（单选、多选、人员、复选框、流程、评分或关联）")
                .child(
                    primary_btn("k-set", "设置分组字段", t)
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| this.open_popup(Popup::KanbanSet, ev.position, cx))),
                )
                .into_any_element();
        };
        let _ = window;
        let dragging = match &self.drag {
            Some(Drag::Card { rid, moved: true, .. }) => Some(rid.clone()),
            _ => None,
        };
        let target = if dragging.is_some() { self.kanban_target(self.mouse) } else { None };
        // 落点算完再清空，本帧绘制时各列 / 卡片重新登记位置
        self.frame.borrow_mut().clear();
        let can_add_group = f.kind.has_options();
        let frame = self.frame.clone();
        let mut row = div().flex().flex_row().gap(px(12.)).p(px(16.)).h_full().items_start();
        for col in cols {
            let collapsed = v.collapsed.contains(&format!("k:{}", col.key));
            let key = col.key.clone();
            let frame_c = frame.clone();
            let key_c = key.clone();
            let reg = canvas(
                move |b, _, _| {
                    frame_c.borrow_mut().insert(format!("kcol:{key_c}"), b);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full();
            let hl = target.as_ref().is_some_and(|(k, _)| k == &col.key);
            if collapsed {
                let k2 = key.clone();
                row = row.child(
                    div()
                        .id(SharedString::from(format!("kc-{key}")))
                        .relative()
                        .w(px(44.))
                        .flex_none()
                        .h_full()
                        .max_h(px(420.))
                        .rounded(px(8.))
                        .bg(t.sidebar_bg)
                        .when(hl, |d| d.border_2().border_color(t.accent))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(8.))
                        .pt(px(12.))
                        .cursor_pointer()
                        .child(icon("chevron-right", 14., t.text_secondary))
                        .child(div().text_size(px(12.)).text_color(t.text_muted).child(col.cards.len().to_string()))
                        .children(col.label.chars().take(12).map(|c| div().text_size(px(13.)).child(c.to_string())))
                        .child(reg)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let k = format!("k:{k2}");
                            this.update_view(cx, |v| v.collapsed.retain(|x| x != &k));
                        })),
                );
                continue;
            }
            let rename = self.inputs.get(&format!("opt:{key}")).map(|x| x.0.clone()).filter(|_| self.renaming.as_deref() == Some(&format!("opt:{key}")));
            let (k_menu, k_add, k_add2) = (key.clone(), key.clone(), key.clone());
            let header = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .px(px(4.))
                .h(px(36.))
                .flex_none()
                .child(match (rename, col.color) {
                    (Some(i), _) => div().w(px(160.)).child(input_box(t).h(px(26.)).child(i)).into_any_element(),
                    (None, Some(c)) => tag(col.label.clone(), c, t).into_any_element(),
                    (None, None) => {
                        if f.kind == FieldType::Person && !col.key.is_empty() {
                            person_chip(&col.label, t).into_any_element()
                        } else {
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if col.key.is_empty() { t.text_muted } else { t.text })
                                .child(col.label.clone())
                                .into_any_element()
                        }
                    }
                })
                .child(div().text_size(px(12.)).text_color(t.text_muted).child(col.cards.len().to_string()))
                .child(div().flex_1())
                .child(
                    div()
                        .id(SharedString::from(format!("km-{key}")))
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.hover))
                        .child(icon("more", 14., t.text_secondary))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, _, cx| this.open_popup(Popup::KanbanCol(k_menu.clone()), ev.position, cx)),
                        ),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("ka-{key}")))
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.hover))
                        .child(icon("plus", 14., t.text_secondary))
                        .tooltip(tip("在此列新建记录"))
                        .on_click(cx.listener(move |this, _, _, cx| this.kanban_add(&k_add, true, cx))),
                );
            let mut list = div()
                .id(SharedString::from(format!("kl-{key}")))
                .flex_1()
                .min_h(px(40.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(8.))
                .px(px(4.))
                .pb(px(8.));
            let mut placed = false;
            let placeholder = || div().h(px(64.)).flex_none().rounded(px(8.)).border_2().border_dashed().border_color(t.accent.opacity(0.6)).bg(t.accent_soft);
            for rid in &col.cards {
                if hl && !placed && target.as_ref().and_then(|x| x.1.as_ref()) == Some(rid) {
                    list = list.child(placeholder());
                    placed = true;
                }
                let is_drag = dragging.as_deref() == Some(rid.as_str());
                let card = self.render_card(rid, &v, Some(&f.id), COL_W - 22., t, cx);
                let (r1, k1) = (rid.clone(), key.clone());
                let frame_r = frame.clone();
                let reg_key = format!("kcard:{key}:{rid}");
                list = list.child(
                    div()
                        .id(SharedString::from(format!("card-{key}-{rid}")))
                        .relative()
                        .flex_none()
                        .cursor_pointer()
                        .when(is_drag, |d| d.opacity(0.35))
                        .hover(|s| s.opacity(0.92))
                        .child(card)
                        .child(
                            canvas(
                                move |b, _, _| {
                                    frame_r.borrow_mut().insert(reg_key.clone(), b);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, w, cx| {
                                w.focus(&this.focus);
                                let origin = this.frame.borrow().get(&format!("kcard:{k1}:{r1}")).map(|b| b.origin).unwrap_or(ev.position);
                                this.drag = Some(Drag::Card {
                                    rid: r1.clone(),
                                    from: k1.clone(),
                                    start: ev.position,
                                    grab: ev.position - origin,
                                    moved: false,
                                    w: COL_W - 22.,
                                });
                                cx.notify();
                            }),
                        ),
                );
            }
            if hl && !placed {
                list = list.child(placeholder());
            }
            list = list.child(
                div()
                    .id(SharedString::from(format!("kadd-{key}")))
                    .flex_none()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .text_color(t.text_muted)
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover).text_color(t.text_secondary))
                    .child(icon("plus", 14., t.text_muted))
                    .child("新建记录")
                    .on_click(cx.listener(move |this, _, _, cx| this.kanban_add(&k_add2, false, cx))),
            );
            row = row.child(
                div()
                    .relative()
                    .w(px(COL_W))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .rounded(px(10.))
                    .bg(t.sidebar_bg)
                    .p(px(6.))
                    .when(hl, |d| d.bg(t.accent_soft))
                    .child(header)
                    .child(list)
                    .child(reg),
            );
        }
        if can_add_group {
            row = row.child(
                div()
                    .id("k-newgroup")
                    .flex_none()
                    .w(px(200.))
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(12.))
                    .rounded(px(8.))
                    .text_size(px(13.))
                    .text_color(t.text_secondary)
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(icon("plus", 14., t.text_secondary))
                    .child("新建分组")
                    .on_click(cx.listener(|this, _, w, cx| this.kanban_new_group(w, cx))),
            );
        }
        let frame_k = frame.clone();
        div()
            .id("kanban")
            .size_full()
            .relative()
            .overflow_x_scroll()
            .track_scroll(&self.kanban_scroll)
            .child(row)
            .child(
                canvas(
                    move |b, _, _| {
                        frame_k.borrow_mut().insert("kanban".into(), b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }

    fn kanban_add(&mut self, key: &str, top: bool, cx: &mut Context<Self>) {
        let v = self.view();
        let Some(f) = v.group_field.as_ref().and_then(|g| self.field(g)) else { return };
        let preset = match f.kind {
            _ if key.is_empty() => vec![],
            FieldType::Select | FieldType::Stage => vec![(f.id.clone(), Value::from(key))],
            FieldType::Checkbox => vec![(f.id.clone(), Value::Bool(key == "1"))],
            FieldType::Rating => vec![(f.id.clone(), key.parse::<f64>().map(Value::from).unwrap_or(Value::Null))],
            _ => vec![(f.id.clone(), Value::from(vec![key.to_string()]))],
        };
        let first =
            if top { self.kanban_columns().and_then(|(_, cols)| cols.into_iter().find(|c| c.key == key).and_then(|c| c.cards.first().cloned())) } else { None };
        let Some(rid) = self.add_record(preset, None, cx) else { return };
        if let Some(first) = first {
            let tid = self.tid.clone();
            let r0 = self.b().record(&tid, &first).map(|r| r.rank);
            let rec = self.b().record(&tid, &rid).cloned();
            if let Some(mut r) = rec {
                r.rank = rank_between(None, r0);
                self.bm().put_record(&tid, r);
                self.commit(cx);
            }
        }
        self.detail = Some(rid);
        cx.notify();
    }

    fn kanban_new_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let v = self.view();
        let Some(mut f) = v.group_field.as_ref().and_then(|g| self.field(g)) else { return };
        let n = f.prop.options.len() + 1;
        let mut name = format!("分组 {n}");
        let mut k = n;
        while f.opt_by_name(&name).is_some() {
            k += 1;
            name = format!("分组 {k}");
        }
        let id = f.ensure_opt(&name);
        let tid = self.tid.clone();
        self.bm().put_field(&tid, f);
        self.commit(cx);
        let key = format!("opt:{id}");
        let i = self.input(&key, "分组名称", &name, true, window, cx);
        i.update(cx, |i, cx| i.set_text(&name, cx));
        self.renaming = Some(key);
        cx.notify();
    }

    pub(super) fn render_kanban_col_menu(&mut self, key: String, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let v = self.view();
        let f = v.group_field.as_ref().and_then(|g| self.field(g));
        let is_opt = f.as_ref().is_some_and(|f| f.kind.has_options()) && !key.is_empty();
        let (k1, k2, k3, k4) = (key.clone(), key.clone(), key.clone(), key.clone());
        let label = f.as_ref().and_then(|f| f.opt(&key)).map(|o| o.name.clone()).unwrap_or_default();
        let color = f.as_ref().and_then(|f| f.opt(&key)).map(|o| o.color).unwrap_or(0);
        panel(t)
            .w(px(220.))
            .when(is_opt, |d| {
                d.child(menu_item("kc-rename", Some("pencil"), "重命名分组", t).on_click(cx.listener(move |this, _, w, cx| {
                    this.close_popups();
                    let key = format!("opt:{k1}");
                    let i = this.input(&key, "分组名称", &label, true, w, cx);
                    i.update(cx, |i, cx| i.set_text(&label, cx));
                    this.renaming = Some(key);
                    cx.notify();
                })))
                .child(section_label("颜色", t))
                .child(div().px(px(12.)).pb(px(6.)).flex().flex_row().flex_wrap().gap(px(6.)).children((0..OPT_COLORS as u8).map(|c| {
                    let k = k4.clone();
                    let (bg, fg) = tag_color(c, t.dark);
                    div()
                        .id(("kc-color", c as usize))
                        .size(px(22.))
                        .rounded(px(5.))
                        .bg(bg)
                        .border_1()
                        .border_color(if c == color { fg } else { bg })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let Some(fid) = this.view().group_field else { return };
                            if let Some(mut f) = this.field(&fid) {
                                if let Some(o) = f.prop.options.iter_mut().find(|o| o.id == k) {
                                    o.color = c;
                                }
                                let tid = this.tid.clone();
                                this.bm().put_field(&tid, f);
                                this.commit(cx);
                            }
                        }))
                })))
            })
            .child(menu_item("kc-fold", Some("fold-vertical"), "折叠分组", t).on_click(cx.listener(move |this, _, _, cx| {
                let k = format!("k:{k2}");
                this.close_popups();
                this.update_view(cx, |v| v.collapsed.push(k));
            })))
            .when(is_opt, |d| {
                d.child(menu_sep(t)).child(menu_item("kc-del", Some("trash"), "删除分组（卡片移到未分组）", t).on_click(cx.listener(move |this, _, _, cx| {
                    this.close_popups();
                    let Some(fid) = this.view().group_field else { return };
                    let Some(mut f) = this.field(&fid) else { return };
                    let tid = this.tid.clone();
                    let rids: Vec<String> = this
                        .b()
                        .table(&tid)
                        .map(|t| t.records.values().filter(|r| str_list(r.cells.get(&fid)).contains(&k3)).map(|r| r.id.clone()).collect())
                        .unwrap_or_default();
                    for r in rids {
                        let cur = str_list(this.b().record(&tid, &r).and_then(|x| x.cells.get(&fid)));
                        let left: Vec<String> = cur.into_iter().filter(|x| x != &k3).collect();
                        let v = if f.kind == FieldType::MultiSelect { Value::from(left) } else { Value::Null };
                        this.bm().set_cell(&tid, &r, &fid, v);
                    }
                    f.prop.options.retain(|o| o.id != k3);
                    this.bm().put_field(&tid, f);
                    this.commit(cx);
                })))
            })
            .into_any_element()
    }

    /// 拖动中跟随鼠标的卡片。
    pub(super) fn render_drag_ghost(&mut self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        match &self.drag {
            Some(Drag::Card { rid, grab, moved: true, w, .. }) => {
                let (rid, grab, w) = (rid.clone(), *grab, *w);
                let v = self.view();
                let skip = v.group_field.clone();
                let card = self.render_card(&rid, &v, skip.as_deref(), w, t, cx);
                let at = self.mouse - grab;
                Some(
                    deferred(anchored().position(at).child(div().w(px(w)).opacity(0.92).shadow_lg().rounded(px(8.)).child(card)))
                        .with_priority(10)
                        .into_any_element(),
                )
            }
            Some(Drag::CalItem { rid, moved: true, .. }) => {
                let title = self.b().title(&self.tid, rid);
                Some(
                    deferred(
                        anchored()
                            .position(self.mouse + point(px(8.), px(8.)))
                            .child(div().px(px(8.)).py(px(4.)).rounded(px(4.)).bg(t.accent).text_color(white()).text_size(px(12.)).shadow_lg().child(title)),
                    )
                    .with_priority(10)
                    .into_any_element(),
                )
            }
            _ => None,
        }
    }
}
