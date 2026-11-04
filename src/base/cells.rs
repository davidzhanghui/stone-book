//! 单元格：按字段类型显示（表格 / 卡片 / 详情三种密度），以及选项、日期、关联、人员、附件的编辑弹层。

use super::editor::{BaseEditor, Popup};
use super::formula::{self, FVal};
use super::model::*;
use super::widgets::*;
use crate::theme::Theme;
use crate::ui::icon;
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Grid,
    Card,
    Detail,
}

fn fmt_size(n: u64) -> String {
    match n {
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1 << 20) as f64),
        n if n >= 1 << 10 => format!("{} KB", n >> 10),
        n => format!("{n} B"),
    }
}

impl BaseEditor {
    /// 单元格内容（只读显示；复选框、评分可直接点）。
    pub(super) fn cell_content(&self, rid: &str, f: &Field, mode: Mode, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let b = self.b();
        let tid = self.tid.clone();
        let raw = b.record(&tid, rid).and_then(|r| r.cells.get(&f.id)).cloned();
        let wrap = mode != Mode::Grid;
        let row = || div().flex().flex_row().items_center().gap(px(4.)).when(wrap, |d| d.flex_wrap()).overflow_hidden();
        let text_el = |s: String, color: Hsla| -> AnyElement {
            if mode == Mode::Grid {
                div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(color).child(s).into_any_element()
            } else {
                div().text_color(color).child(s).into_any_element()
            }
        };
        use FieldType::*;
        match f.kind {
            Select | Stage | MultiSelect => {
                let opts: Vec<SelOpt> = str_list(raw.as_ref()).iter().filter_map(|id| f.opt(id).cloned()).collect();
                if f.kind == Stage && mode != Mode::Grid && !f.prop.options.is_empty() {
                    // 流程：整条进度，当前节点之前的都点亮
                    let cur = opts.first().and_then(|o| f.prop.options.iter().position(|x| x.id == o.id));
                    return div()
                        .flex()
                        .flex_row()
                        .gap(px(2.))
                        .children(f.prop.options.iter().enumerate().map(|(i, o)| {
                            let on = cur.is_some_and(|c| i <= c);
                            let (bg, fg) = tag_color(o.color, t.dark);
                            div()
                                .px(px(8.))
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .text_size(px(11.5))
                                .when(i == 0, |d| d.rounded_l(px(10.)))
                                .when(i + 1 == f.prop.options.len(), |d| d.rounded_r(px(10.)))
                                .bg(if on { bg } else { t.hover })
                                .text_color(if on { fg } else { t.text_muted })
                                .child(o.name.clone())
                        }))
                        .into_any_element();
                }
                row().children(opts.into_iter().map(|o| tag(o.name, o.color, t))).into_any_element()
            }
            Person => row().children(str_list(raw.as_ref()).iter().map(|n| person_chip(n, t))).into_any_element(),
            Link | DuplexLink => {
                let ids = str_list(raw.as_ref());
                row().children(ids.iter().filter(|r| b.record(&f.prop.table, r).is_some()).map(|r| link_chip(b.title(&f.prop.table, r), t))).into_any_element()
            }
            Checkbox => {
                let on = raw.and_then(|v| v.as_bool()).unwrap_or(false);
                let (rid, fid) = (rid.to_string(), f.id.clone());
                div()
                    .id(SharedString::from(format!("cb-{rid}-{fid}")))
                    .cursor_pointer()
                    .child(check_box(on, t))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.set_cell(&rid, &fid, Value::Bool(!on), false, cx);
                        }),
                    )
                    .into_any_element()
            }
            Rating => {
                let n = raw.and_then(|v| v.as_f64()).unwrap_or(0.) as i32;
                let max = if f.prop.max > 0. { f.prop.max as i32 } else { 5 };
                let ic = rating_icon(&f.prop.symbol);
                let on_ic = rating_icon_on(&f.prop.symbol);
                let col = rating_color(&f.prop.symbol);
                let (rid, fid) = (rid.to_string(), f.id.clone());
                div()
                    .flex()
                    .flex_row()
                    .gap(px(2.))
                    .children((1..=max).map(|i| {
                        let (rid, fid) = (rid.clone(), fid.clone());
                        div()
                            .id(SharedString::from(format!("rt-{rid}-{fid}-{i}")))
                            .cursor_pointer()
                            .child(icon(if i <= n { on_ic } else { ic }, 14., if i <= n { col } else { t.border_strong }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    // 再点一次当前分数则清零
                                    this.set_cell(&rid, &fid, if i == n { Value::Null } else { Value::from(i) }, false, cx);
                                }),
                            )
                    }))
                    .into_any_element()
            }
            Progress => {
                let Some(n) = raw.and_then(|v| v.as_f64()) else { return div().into_any_element() };
                let frac = (n / if f.prop.max > 0. { f.prop.max } else { 1. }).clamp(0., 1.) as f32;
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .w_full()
                    .child(
                        div()
                            .flex_1()
                            .max_w(px(140.))
                            .h(px(6.))
                            .rounded(px(3.))
                            .bg(t.hover)
                            .child(div().h_full().w(relative(frac)).rounded(px(3.)).bg(if frac >= 1. { rgb(0x34c724).into() } else { t.accent })),
                    )
                    .child(div().text_size(px(12.)).text_color(t.text_secondary).child(b.text(&tid, rid, &f.id)))
                    .into_any_element()
            }
            Attachment => {
                let files = attachments(raw.as_ref());
                let sz = match mode {
                    Mode::Grid => 24.,
                    Mode::Card => 36.,
                    Mode::Detail => 72.,
                };
                row()
                    .gap(px(6.))
                    .children(files.into_iter().enumerate().map(|(i, a)| {
                        let (rid, fid) = (rid.to_string(), f.id.clone());
                        let el = if a.is_image() {
                            div()
                                .size(px(sz))
                                .flex_none()
                                .rounded(px(4.))
                                .overflow_hidden()
                                .border_1()
                                .border_color(t.border)
                                .child(img(PathBuf::from(a.preview())).size_full().object_fit(ObjectFit::Cover))
                                .into_any_element()
                        } else {
                            let ext = a.name.rsplit('.').next().unwrap_or("").to_uppercase();
                            div()
                                .h(px(sz))
                                .min_w(px(sz))
                                .px(px(6.))
                                .flex_none()
                                .rounded(px(4.))
                                .bg(t.hover)
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(11.))
                                .child(icon("file", 13., t.text_secondary))
                                .when(mode != Mode::Grid, |d| {
                                    d.child(div().max_w(px(120.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(a.name.clone()))
                                })
                                .when(mode == Mode::Grid, |d| d.child(ext))
                                .into_any_element()
                        };
                        div().id(SharedString::from(format!("att-{}", a.path))).cursor_pointer().child(el).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, w, cx| {
                                if ev.click_count >= 2 {
                                    cx.stop_propagation();
                                    this.open_file_view(&rid, &fid, i, w, cx);
                                }
                            }),
                        )
                    }))
                    .into_any_element()
            }
            Button => {
                let label = if f.prop.label.trim().is_empty() { f.name.clone() } else { f.prop.label.clone() };
                let (bg, fg) = tag_color(f.prop.color, t.dark);
                let (rid, fid) = (rid.to_string(), f.id.clone());
                div()
                    .id(SharedString::from(format!("btn-{rid}-{fid}")))
                    .px(px(10.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(bg)
                    .text_color(fg)
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.press_button(&rid, &fid, cx);
                        }),
                    )
                    .into_any_element()
            }
            Url => {
                let s = raw.and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
                if s.is_empty() {
                    return div().into_any_element();
                }
                let url = if s.contains("://") { s.clone() } else { format!("https://{s}") };
                div()
                    .id(SharedString::from(format!("url-{rid}-{}", f.id)))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(t.link)
                    .underline()
                    .cursor_pointer()
                    .child(s)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, ev: &MouseDownEvent, _, cx| {
                            if ev.modifiers.platform || ev.click_count >= 2 {
                                cx.stop_propagation();
                                cx.open_url(&url);
                            }
                        }),
                    )
                    .into_any_element()
            }
            _ => {
                let v = b.fval(&tid, rid, &f.id);
                let s = super::calc::format_fval(f, &v);
                let color = if matches!(v, FVal::Err(_)) {
                    t.fg_palette[1]
                } else if matches!(f.kind, CreatedTime | ModifiedTime | AutoNumber) {
                    t.text_secondary
                } else {
                    t.text
                };
                match &v {
                    // 查找引用 / 公式得到多个值：拆成小标签
                    FVal::List(l) if mode != Mode::Grid || l.len() > 1 => row()
                        .children(l.iter().filter(|x| !x.is_null()).map(|x| {
                            div().flex_none().px(px(6.)).h(px(20.)).flex().items_center().rounded(px(4.)).bg(t.hover).text_size(px(12.)).child(x.text())
                        }))
                        .into_any_element(),
                    _ => text_el(s, color),
                }
            }
        }
    }

    /// 是否右对齐（数字类）。
    pub(super) fn right_align(&self, f: &Field) -> bool {
        matches!(f.kind, FieldType::Number | FieldType::Currency | FieldType::AutoNumber)
            || (f.kind == FieldType::Formula && f.prop.fmt > 0)
            || (matches!(f.kind, FieldType::Formula | FieldType::Lookup) && matches!(f.prop.agg.as_str(), "sum" | "avg" | "count" | "max" | "min"))
    }

    /// 打开某单元格的编辑器：文本类在表格里原地编辑，其余弹出对应面板。
    pub(super) fn open_cell_editor(&mut self, rid: &str, f: &Field, at: Point<Pixels>, inline: bool, window: &mut Window, cx: &mut Context<Self>) {
        use FieldType::*;
        let (rid, fid) = (rid.to_string(), f.id.clone());
        match f.kind {
            Select | MultiSelect | Stage => self.open_popup(Popup::Options { rid, fid }, at, cx),
            Person => self.open_popup(Popup::People { rid, fid }, at, cx),
            Link | DuplexLink => {
                if self.b().table(&f.prop.table).is_none() {
                    self.show_toast("关联的数据表已被删除，请重新设置字段", cx);
                    return;
                }
                self.open_popup(Popup::Links { rid, fid }, at, cx)
            }
            Attachment => self.open_popup(Popup::Files { rid, fid }, at, cx),
            Date => {
                let cur = self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)).and_then(|v| v.as_i64()).unwrap_or_else(formula::today);
                let d = Local.timestamp_millis_opt(cur).single().unwrap_or_else(Local::now);
                self.open_popup(Popup::Date { rid, fid, ym: (d.year(), d.month()) }, at, cx)
            }
            Checkbox => {
                let on = self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)).and_then(|v| v.as_bool()).unwrap_or(false);
                self.set_cell(&rid, &fid, Value::Bool(!on), false, cx);
            }
            k if k.computed() => self.show_toast(&format!("「{}」由系统计算，不能直接编辑", f.kind.label()), cx),
            Rating => {}
            _ if inline => self.start_cell_edit(None, window, cx),
            _ => {}
        }
    }

    // ───────────── 编辑弹层 ─────────────

    pub(super) fn render_cell_popup(&mut self, p: &Popup, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        match p {
            Popup::Options { rid, fid } => Some(self.render_options(rid.clone(), fid.clone(), t, window, cx)),
            Popup::Date { rid, fid, ym } => Some(self.render_date(rid.clone(), fid.clone(), *ym, t, cx)),
            Popup::Links { rid, fid } => Some(self.render_links(rid.clone(), fid.clone(), t, window, cx)),
            Popup::People { rid, fid } => Some(self.render_people(rid.clone(), fid.clone(), t, window, cx)),
            Popup::Files { rid, fid } => Some(self.render_files(rid.clone(), fid.clone(), t, cx)),
            _ => None,
        }
    }

    fn render_options(&mut self, rid: String, fid: String, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(f) = self.field(&fid) else { return div().into_any_element() };
        let multi = f.kind == FieldType::MultiSelect;
        let input = self.input("q:opt", "搜索或创建选项", "", true, window, cx);
        let q = self.input_text("q:opt", cx).trim().to_string();
        let cur = str_list(self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)));
        // 选项引用：选项来自另一张表的字段；有联动条件时只列满足条件的
        let allowed: Option<Vec<String>> = (!f.prop.opt_table.is_empty() && !f.prop.opt_field.is_empty()).then(|| {
            let b = self.b();
            let rs = b.lookup_matches(&self.tid, &rid, &f.prop.opt_table, &f.prop.conds);
            let mut names: Vec<String> = vec![];
            for r in rs {
                let v = b.fval(&f.prop.opt_table, &r, &f.prop.opt_field);
                let parts: Vec<String> = match v {
                    super::formula::FVal::List(l) => l.iter().map(|x| x.text()).collect(),
                    v => vec![v.text()],
                };
                for n in parts.into_iter().filter(|n| !n.trim().is_empty()) {
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
            }
            names
        });
        let f = match &allowed {
            Some(names) if names.iter().any(|n| f.opt_by_name(n).is_none()) => {
                // 引用到的新值补成选项（随下一次选择一起保存）
                let mut f2 = f.clone();
                for n in names {
                    f2.ensure_opt(n);
                }
                f2
            }
            _ => f,
        };
        let opts: Vec<SelOpt> = f
            .prop
            .options
            .iter()
            .filter(|o| allowed.as_ref().is_none_or(|a| a.contains(&o.name)))
            .filter(|o| q.is_empty() || o.name.to_lowercase().contains(&q.to_lowercase()))
            .cloned()
            .collect();
        let exact = f.prop.options.iter().any(|o| o.name == q);
        let f_save = f.clone();
        let choose = move |this: &mut BaseEditor, id: String, cx: &mut Context<BaseEditor>| {
            if this.field(&fid).is_some_and(|x| x.prop.options.len() != f_save.prop.options.len()) {
                let tid = this.tid.clone();
                this.bm().put_field(&tid, f_save.clone());
            }
            let mut cur = str_list(this.b().record(&this.tid, &rid).and_then(|r| r.cells.get(&fid)));
            let v = if multi {
                if cur.contains(&id) {
                    cur.retain(|x| x != &id);
                } else {
                    cur.push(id);
                }
                Value::from(cur)
            } else if cur.first() == Some(&id) {
                Value::Null
            } else {
                Value::from(id)
            };
            this.set_cell(&rid, &fid, v, false, cx);
            if !multi {
                this.close_popups();
            } else if let Some((i, _)) = this.inputs.get("q:opt") {
                i.update(cx, |i, cx| i.set_text("", cx));
            }
        };
        let choose2 = choose.clone();
        let f2 = f.clone();
        panel(t)
            .w(px(280.))
            .child(div().px(px(8.)).pt(px(4.)).pb(px(6.)).child(input_box(t).child(input)))
            .when(!cur.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(12.))
                        .pb(px(6.))
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(4.))
                        .children(cur.iter().filter_map(|id| f.opt(id)).map(|o| tag(o.name.clone(), o.color, t))),
                )
            })
            .child(menu_sep(t))
            .child(div().id("opt-list").max_h(px(280.)).overflow_y_scroll().children(opts.into_iter().map(|o| {
                let on = cur.contains(&o.id);
                let id = o.id.clone();
                let ch = choose.clone();
                div()
                    .id(SharedString::from(format!("opt-{}", o.id)))
                    .mx(px(4.))
                    .px(px(8.))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(tag(o.name, o.color, t))
                    .child(div().flex_1())
                    .when(on, |d| d.child(icon("check", 14., t.accent)))
                    .on_click(cx.listener(move |this, _, _, cx| ch(this, id.clone(), cx)))
            })))
            .when(!q.is_empty() && !exact, |d| {
                d.child(menu_item("opt-new", Some("plus"), format!("创建选项「{q}」"), t).on_click(cx.listener(move |this, _, _, cx| {
                    let mut f = f2.clone();
                    let id = f.ensure_opt(&q);
                    let tid = this.tid.clone();
                    this.bm().put_field(&tid, f);
                    choose2(this, id, cx);
                })))
            })
            .into_any_element()
    }

    fn render_date(&mut self, rid: String, fid: String, ym: (i32, u32), t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let cur = self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)).and_then(|v| v.as_i64());
        let cur_day = cur.map(formula::day_start);
        let today = formula::today();
        let first = NaiveDate::from_ymd_opt(ym.0, ym.1, 1).unwrap_or_default();
        let offset = first.weekday().num_days_from_monday() as i64;
        let start = first - chrono::Duration::days(offset);
        let nav = |id: &'static str, ic: &'static str, delta: i32, cx: &mut Context<Self>| {
            let (rid, fid) = (rid.clone(), fid.clone());
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .child(icon(ic, 14., t.text_secondary))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let m = ym.0 * 12 + ym.1 as i32 - 1 + delta;
                    if let Some((p, _)) = this.popup.as_mut() {
                        *p = Popup::Date { rid: rid.clone(), fid: fid.clone(), ym: (m.div_euclid(12), m.rem_euclid(12) as u32 + 1) };
                    }
                    cx.notify();
                }))
        };
        let mut grid = div().flex().flex_col().gap(px(2.)).px(px(10.));
        grid = grid.child(
            div().flex().flex_row().children(
                ["一", "二", "三", "四", "五", "六", "日"]
                    .iter()
                    .map(|w| div().w(px(34.)).h(px(24.)).flex().items_center().justify_center().text_size(px(12.)).text_color(t.text_muted).child(*w)),
            ),
        );
        for wk in 0..6 {
            let mut row = div().flex().flex_row();
            for d in 0..7 {
                let day = start + chrono::Duration::days(wk * 7 + d);
                let ms = Local.from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap()).single().map(|x| x.timestamp_millis()).unwrap_or(0);
                let in_month = day.month() == ym.1;
                let sel = cur_day == Some(ms);
                let is_today = ms == today;
                let (rid, fid) = (rid.clone(), fid.clone());
                // 保留原来的时分
                let keep = cur.map(|c| c - formula::day_start(c)).unwrap_or(0);
                row = row.child(
                    div()
                        .id(SharedString::from(format!("day-{ms}")))
                        .w(px(34.))
                        .h(px(30.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_size(px(13.))
                        .text_color(if sel {
                            white()
                        } else if !in_month {
                            t.text_muted
                        } else if is_today {
                            t.accent
                        } else {
                            t.text
                        })
                        .when(sel, |s| s.bg(t.accent))
                        .when(is_today && !sel, |s| s.border_1().border_color(t.accent))
                        .when(!sel, |s| s.hover(|s| s.bg(t.hover)))
                        .child(day.day().to_string())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_cell(&rid, &fid, Value::from(ms + keep), false, cx);
                            this.close_popups();
                        })),
                );
            }
            grid = grid.child(row);
        }
        let (rid2, fid2) = (rid.clone(), fid.clone());
        let (rid3, fid3) = (rid.clone(), fid.clone());
        panel(t)
            .w(px(266.))
            .pb(px(8.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(10.))
                    .h(px(38.))
                    .child(nav("d-prev", "chevron-left", -1, cx))
                    .child(div().flex_1().flex().justify_center().font_weight(FontWeight::MEDIUM).child(format!("{} 年 {} 月", ym.0, ym.1)))
                    .child(nav("d-next", "chevron-right", 1, cx)),
            )
            .child(grid)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .px(px(10.))
                    .pt(px(8.))
                    .child(ghost_btn("d-today", "今天", t).h(px(26.)).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_cell(&rid2, &fid2, Value::from(formula::today()), false, cx);
                        this.close_popups();
                    })))
                    .child(div().flex_1())
                    .child(ghost_btn("d-clear", "清除", t).h(px(26.)).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_cell(&rid3, &fid3, Value::Null, false, cx);
                        this.close_popups();
                    }))),
            )
            .into_any_element()
    }

    fn render_links(&mut self, rid: String, fid: String, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(f) = self.field(&fid) else { return div().into_any_element() };
        let target = f.prop.table.clone();
        let input = self.input("q:link", "搜索记录", "", true, window, cx);
        let q = self.input_text("q:link", cx).trim().to_lowercase();
        let cur = str_list(self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)));
        let (tname, items): (String, Vec<(String, String)>) = {
            let b = self.b();
            let Some(tt) = b.table(&target) else { return div().into_any_element() };
            let items =
                tt.order.iter().map(|r| (r.clone(), b.title(&target, r))).filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)).take(200).collect();
            (tt.meta.name.clone(), items)
        };
        let qraw = self.input_text("q:link", cx).trim().to_string();
        let toggle = {
            let (rid, fid) = (rid.clone(), fid.clone());
            move |this: &mut BaseEditor, id: String, cx: &mut Context<BaseEditor>| {
                let mut cur = str_list(this.b().record(&this.tid, &rid).and_then(|r| r.cells.get(&fid)));
                if cur.contains(&id) {
                    cur.retain(|x| x != &id);
                } else {
                    cur.push(id);
                }
                this.set_cell(&rid, &fid, Value::from(cur), false, cx);
            }
        };
        let toggle2 = toggle.clone();
        panel(t)
            .w(px(320.))
            .child(
                div()
                    .px(px(12.))
                    .pt(px(6.))
                    .pb(px(2.))
                    .text_size(px(12.))
                    .text_color(t.text_muted)
                    .child(format!("关联「{tname}」中的记录{}", if f.prop.single { "（仅一条）" } else { "" })),
            )
            .child(div().px(px(8.)).py(px(6.)).child(input_box(t).gap(px(6.)).child(icon("search", 14., t.text_muted)).child(input)))
            .child(div().id("link-list").max_h(px(300.)).overflow_y_scroll().children(items.into_iter().map(|(id, name)| {
                let on = cur.contains(&id);
                let tg = toggle.clone();
                let id2 = id.clone();
                div()
                    .id(SharedString::from(format!("lk-{id}")))
                    .mx(px(4.))
                    .px(px(8.))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(check_box(on, t))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name))
                    .on_click(cx.listener(move |this, _, _, cx| tg(this, id2.clone(), cx)))
            })))
            .child(menu_sep(t))
            .child(
                menu_item("lk-new", Some("plus"), if qraw.is_empty() { "新建一条记录并关联".to_string() } else { format!("新建记录「{qraw}」并关联") }, t)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let pid = this.b().table(&target).and_then(|t| t.primary().map(|p| p.id.clone()));
                        let r = this.bm().new_record(&target, None);
                        if let (Some(r), Some(pid)) = (r, pid) {
                            if !qraw.is_empty() {
                                this.bm().set_cell(&target, &r.id, &pid, Value::from(qraw.clone()));
                            }
                            toggle2(this, r.id, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    /// 本多维表格里出现过的所有人员名字。
    fn known_people(&self) -> Vec<String> {
        let b = self.b();
        let mut out: Vec<String> = vec![];
        let me = crate::settings::author();
        if !me.is_empty() {
            out.push(me);
        }
        for t in b.table_list() {
            let pf: Vec<&String> = t.fields.values().filter(|f| f.kind == FieldType::Person).map(|f| &f.id).collect();
            for r in t.records.values() {
                for f in &pf {
                    for n in str_list(r.cells.get(*f)) {
                        if !out.contains(&n) {
                            out.push(n);
                        }
                    }
                }
            }
        }
        out
    }

    fn render_people(&mut self, rid: String, fid: String, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let input = self.input("q:ppl", "搜索或输入名字添加", "", true, window, cx);
        let q = self.input_text("q:ppl", cx).trim().to_string();
        let cur = str_list(self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)));
        let people: Vec<String> = self.known_people().into_iter().filter(|n| q.is_empty() || n.contains(&q)).collect();
        let exact = people.contains(&q);
        let toggle = {
            let (rid, fid) = (rid.clone(), fid.clone());
            move |this: &mut BaseEditor, name: String, cx: &mut Context<BaseEditor>| {
                let mut cur = str_list(this.b().record(&this.tid, &rid).and_then(|r| r.cells.get(&fid)));
                if cur.contains(&name) {
                    cur.retain(|x| x != &name);
                } else {
                    cur.push(name);
                }
                this.set_cell(&rid, &fid, Value::from(cur), false, cx);
                if let Some((i, _)) = this.inputs.get("q:ppl") {
                    i.update(cx, |i, cx| i.set_text("", cx));
                }
            }
        };
        let toggle2 = toggle.clone();
        panel(t)
            .w(px(260.))
            .child(div().px(px(8.)).pt(px(4.)).pb(px(6.)).child(input_box(t).child(input)))
            .child(div().id("ppl-list").max_h(px(260.)).overflow_y_scroll().children(people.into_iter().map(|n| {
                let on = cur.contains(&n);
                let tg = toggle.clone();
                let n2 = n.clone();
                div()
                    .id(SharedString::from(format!("pp-{n}")))
                    .mx(px(4.))
                    .px(px(8.))
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(avatar(&n, 22.))
                    .child(div().flex_1().child(n.clone()))
                    .when(on, |d| d.child(icon("check", 14., t.accent)))
                    .on_click(cx.listener(move |this, _, _, cx| tg(this, n2.clone(), cx)))
            })))
            .when(!q.is_empty() && !exact, |d| {
                d.child(menu_item("pp-new", Some("plus"), format!("添加「{q}」"), t).on_click(cx.listener(move |this, _, _, cx| toggle2(this, q.clone(), cx))))
            })
            .into_any_element()
    }

    fn render_files(&mut self, rid: String, fid: String, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let files = attachments(self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)));
        let (rid2, fid2) = (rid.clone(), fid.clone());
        panel(t)
            .w(px(340.))
            .child(section_label(format!("附件 · {} 个", files.len()), t))
            .child(div().id("file-list").max_h(px(320.)).overflow_y_scroll().children(files.iter().enumerate().map(|(i, a)| {
                let (rid, fid) = (rid.clone(), fid.clone());
                let (rid_o, fid_o) = (rid.clone(), fid.clone());
                div()
                    .id(("file", i))
                    .group("file")
                    .mx(px(4.))
                    .px(px(8.))
                    .py(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(6.))
                    .hover(|s| s.bg(t.hover))
                    .child(if a.is_image() {
                        div()
                            .size(px(40.))
                            .rounded(px(4.))
                            .overflow_hidden()
                            .child(img(PathBuf::from(a.preview())).size_full().object_fit(ObjectFit::Cover))
                            .into_any_element()
                    } else {
                        div()
                            .size(px(40.))
                            .rounded(px(4.))
                            .bg(t.hover)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("file", 18., t.text_secondary))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(a.name.clone()))
                            .child(div().text_size(px(11.)).text_color(t.text_muted).child(fmt_size(a.size))),
                    )
                    .child(
                        div()
                            .id(("file-open", i))
                            .px(px(6.))
                            .text_size(px(12.))
                            .text_color(t.accent)
                            .cursor_pointer()
                            .child("打开")
                            .on_click(cx.listener(move |this, _, w, cx| this.open_file_view(&rid_o, &fid_o, i, w, cx))),
                    )
                    .child(
                        div()
                            .id(("file-del", i))
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .child(icon("trash", 13., t.text_muted))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let mut list = attachments(this.b().record(&this.tid, &rid).and_then(|r| r.cells.get(&fid)));
                                if i < list.len() {
                                    list.remove(i);
                                }
                                this.set_cell(&rid, &fid, serde_json::to_value(list).unwrap_or(Value::Null), false, cx);
                            })),
                    )
            })))
            .child(menu_sep(t))
            .child(
                menu_item("file-add", Some("upload-file"), "上传文件", t)
                    .on_click(cx.listener(move |this, _, _, cx| this.pick_files(rid2.clone(), fid2.clone(), cx))),
            )
            .into_any_element()
    }
}
