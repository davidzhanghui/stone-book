//! 日历视图（R9）：月 / 周 / 日三种；开始 + 结束日期的记录画成跨天的条；周、日视图按时间段放置；
//! 颜色跟随单选字段或填色规则；右侧「未排期」列表可以把记录拖进日历；拖动记录到另一天改日期。

use super::editor::{BaseEditor, Drag, Popup};
use super::formula::{self, FVal};
use super::model::*;
use super::widgets::*;
use crate::theme::Theme;
use crate::ui::icon;
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde_json::Value;
use std::collections::HashMap;

const DAY: i64 = 86_400_000;
const HOUR_H: f32 = 40.;

fn day_ms(d: NaiveDate) -> i64 {
    Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).single().map(|x| x.timestamp_millis()).unwrap_or(0)
}

impl BaseEditor {
    /// 卡片颜色：视图指定的单选字段 → 填色规则（整行）→ 第一个单选字段。
    pub(super) fn record_color(&self, rid: &str) -> Option<u8> {
        let b = self.b();
        let t = b.table(&self.tid)?;
        let v = self.view();
        if let Some(c) = self.colors_cache.as_ref().and_then(|(_, c)| c.row.get(rid).copied()) {
            return Some(c);
        }
        let f = match v.color_field.as_ref().and_then(|f| t.fields.get(f)) {
            Some(f) => f,
            None => t.field_list().into_iter().find(|f| matches!(f.kind, FieldType::Select | FieldType::Stage))?,
        };
        let id = str_list(b.record(&self.tid, rid)?.cells.get(&f.id)).into_iter().next()?;
        f.opt(&id).map(|o| o.color)
    }

    /// 记录的 (开始, 结束)（毫秒；结束缺失或早于开始时与开始相同）。
    fn cal_span(&self, rid: &str, v: &View) -> Option<(i64, i64)> {
        let b = self.b();
        let s = match b.fval(&self.tid, rid, v.date_field.as_deref()?) {
            FVal::Date(d) => d,
            _ => return None,
        };
        let e = v.end_field.as_deref().and_then(|f| match b.fval(&self.tid, rid, f) {
            FVal::Date(d) => Some(d),
            _ => None,
        });
        Some((s, e.unwrap_or(s).max(s)))
    }

    pub(super) fn render_calendar(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let v = self.view();
        let Some(df) = v.date_field.as_ref().and_then(|f| self.field(f)) else {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .text_color(t.text_muted)
                .child("日历视图需要一个日期字段")
                .child(
                    primary_btn("c-set", "设置日期字段", t)
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| this.open_popup(Popup::DateSet, ev.position, cx))),
                )
                .into_any_element();
        };
        let _ = self.colors();
        let rows = self.rows();
        let mut spans: Vec<(String, i64, i64)> = vec![];
        let mut undated = vec![];
        for r in rows.iter() {
            match self.cal_span(r, &v) {
                Some((s, e)) => spans.push((r.clone(), s, e)),
                None => undated.push(r.clone()),
            }
        }
        let drag_rid = match &self.drag {
            Some(Drag::CalItem { rid, moved: true, .. }) => Some(rid.clone()),
            _ => None,
        };
        let hover = drag_rid.as_ref().and_then(|_| {
            self.frame
                .borrow()
                .iter()
                .filter(|(k, b)| (k.starts_with("cal:") || k.starts_with("calh:")) && b.contains(&self.mouse))
                .map(|(k, _)| k.clone())
                .max_by_key(|k| k.starts_with("calh:"))
        });
        self.frame.borrow_mut().retain(|k, _| !k.starts_with("cal:") && !k.starts_with("calh:"));
        let mode = v.cal_mode.min(2);
        let anchor = formula::day_start(self.cal_day);
        let (y, m) = self.cal_month;
        let title = match mode {
            0 => format!("{y} 年 {m} 月"),
            1 => {
                let d = Local.timestamp_millis_opt(anchor).single().map(|x| x.date_naive()).unwrap_or_default();
                let mon = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
                let sun = mon + chrono::Duration::days(6);
                format!("{} 年 {} 月 {} 日 – {} 月 {} 日", mon.year(), mon.month(), mon.day(), sun.month(), sun.day())
            }
            _ => formula::fmt_date(anchor, 4) + " " + formula::fmt_date(anchor, 5).split(' ').nth(1).unwrap_or(""),
        };
        let nav = |id: &'static str, label: &'static str, delta: i32, cx: &mut Context<Self>| {
            ghost_btn(id, label, t).h(px(28.)).on_click(cx.listener(move |this, _, _, cx| {
                let mode = this.view().cal_mode.min(2);
                if delta == 0 {
                    let n = Local::now();
                    this.cal_month = (n.year(), n.month());
                    this.cal_day = formula::today();
                } else if mode == 0 {
                    let k = this.cal_month.0 * 12 + this.cal_month.1 as i32 - 1 + delta;
                    this.cal_month = (k.div_euclid(12), k.rem_euclid(12) as u32 + 1);
                } else {
                    this.cal_day += delta as i64 * if mode == 1 { 7 } else { 1 } * DAY;
                }
                cx.notify();
            }))
        };
        let seg = |id: &'static str, label: &'static str, k: u8, cx: &mut Context<Self>| {
            let on = mode == k;
            div()
                .id(id)
                .px(px(12.))
                .h(px(28.))
                .flex()
                .items_center()
                .text_size(px(12.5))
                .cursor_pointer()
                .when(on, |d| d.bg(t.accent_soft).text_color(t.accent))
                .when(!on, |d| d.hover(|s| s.bg(t.hover)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    // 切换月 / 周 / 日是界面状态，直接写视图不进撤销
                    let tid = this.tid.clone();
                    let mut v = this.view();
                    v.cal_mode = k;
                    this.bm().put_view(&tid, v);
                    this.bm().commit_silent();
                    cx.notify();
                }))
        };
        let body: AnyElement = if mode == 0 {
            self.cal_month_grid(&v, &df, &spans, drag_rid.as_deref(), hover.as_deref(), t, cx)
        } else {
            self.cal_time_grid(&v, &df, &spans, anchor, if mode == 1 { 7 } else { 1 }, drag_rid.as_deref(), hover.as_deref(), t, cx)
        };
        let editable = !df.kind.computed();
        let side = (!undated.is_empty() && editable).then(|| {
            let mut list = div().id("cal-undated").flex_1().overflow_y_scroll().flex().flex_col().gap(px(4.)).p(px(8.));
            for r in undated.iter().take(300) {
                let title = self.b().title(&self.tid, r);
                let r1 = r.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("cu-{r}")))
                        .px(px(8.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(5.))
                        .bg(t.panel_bg)
                        .border_1()
                        .border_color(t.border)
                        .text_size(px(12.5))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .cursor_grab()
                        .when(drag_rid.as_deref() == Some(r.as_str()), |d| d.opacity(0.4))
                        .child(title)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                this.drag = Some(Drag::CalItem { rid: r1.clone(), start: ev.position, moved: false });
                                cx.notify();
                            }),
                        ),
                );
            }
            div()
                .w(px(220.))
                .flex_none()
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(t.border)
                .bg(t.sidebar_bg)
                .child(div().px(px(12.)).py(px(10.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(format!("未排期 · {} 条", undated.len())))
                .child(div().px(px(12.)).pb(px(6.)).text_size(px(12.)).text_color(t.text_muted).child("拖到日历上设置日期"))
                .child(list)
        });
        let _ = window;
        div()
            .size_full()
            .flex()
            .flex_row()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .p(px(16.))
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(title))
                            .child(div().w(px(12.)))
                            .child(nav(
                                "c-prev",
                                if mode == 0 {
                                    "上个月"
                                } else if mode == 1 {
                                    "上一周"
                                } else {
                                    "前一天"
                                },
                                -1,
                                cx,
                            ))
                            .child(nav("c-today", "今天", 0, cx))
                            .child(nav(
                                "c-next",
                                if mode == 0 {
                                    "下个月"
                                } else if mode == 1 {
                                    "下一周"
                                } else {
                                    "后一天"
                                },
                                1,
                                cx,
                            ))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(t.border_strong)
                                    .overflow_hidden()
                                    .child(seg("cm-m", "月", 0, cx))
                                    .child(seg("cm-w", "周", 1, cx))
                                    .child(seg("cm-d", "日", 2, cx)),
                            )
                            .child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("按「{}」排列", df.name))),
                    )
                    .child(body),
            )
            .children(side)
            .into_any_element()
    }

    /// 一个日程条（月视图里跨天的部分去掉相邻一侧的圆角）。
    #[allow(clippy::too_many_arguments)]
    fn cal_item(&self, key: String, rid: &str, label: String, first: bool, last: bool, dragging: bool, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let color = self.record_color(rid);
        let (bg, fg) = color.map(|c| tag_color(c, t.dark)).unwrap_or((t.accent_soft, t.accent));
        let r1 = rid.to_string();
        div()
            .id(SharedString::from(key))
            .h(px(22.))
            .flex_none()
            .px(px(6.))
            .flex()
            .items_center()
            .when(first, |d| d.rounded_l(px(4.)))
            .when(last, |d| d.rounded_r(px(4.)))
            .when(!first, |d| d.ml(px(-5.)))
            .when(!last, |d| d.mr(px(-5.)))
            .bg(bg)
            .text_color(fg)
            .text_size(px(12.))
            .overflow_hidden()
            .whitespace_nowrap()
            .cursor_pointer()
            .when(dragging, |d| d.opacity(0.4))
            .child(if first { label } else { String::new() })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.drag = Some(Drag::CalItem { rid: r1.clone(), start: ev.position, moved: false });
                }),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn cal_month_grid(
        &mut self,
        _v: &View,
        df: &Field,
        spans: &[(String, i64, i64)],
        drag: Option<&str>,
        hover: Option<&str>,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (y, m) = self.cal_month;
        let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or_default();
        let start = first - chrono::Duration::days(first.weekday().num_days_from_monday() as i64);
        let grid_lo = day_ms(start);
        let grid_hi = grid_lo + 42 * DAY;
        // 每天的日程（跨天记录在每一天都有一段），同一记录在各天保持同一行位置
        let mut by_day: HashMap<i64, Vec<(String, bool, bool)>> = HashMap::new();
        let mut sorted: Vec<&(String, i64, i64)> = spans.iter().filter(|s| formula::day_start(s.2) >= grid_lo && formula::day_start(s.1) < grid_hi).collect();
        sorted.sort_by_key(|s| (s.1, -(s.2 - s.1)));
        for (rid, s, e) in sorted {
            let (s0, e0) = (formula::day_start(*s), formula::day_start(*e));
            let mut d = s0.max(grid_lo);
            while d <= e0 && d < grid_hi {
                by_day.entry(d).or_default().push((
                    rid.clone(),
                    d == s0 || Local.timestamp_millis_opt(d).single().is_some_and(|x| x.weekday().num_days_from_monday() == 0),
                    d == e0,
                ));
                d += DAY;
            }
        }
        let today = formula::today();
        let editable = !df.kind.computed();
        let mut weeks = div().flex_1().flex().flex_col().border_l_1().border_t_1().border_color(t.border);
        for w in 0..6 {
            let mut week = div().flex_1().flex().flex_row();
            for d in 0..7 {
                let day = start + chrono::Duration::days(w * 7 + d);
                let ms = day_ms(day);
                let items = by_day.get(&ms).cloned().unwrap_or_default();
                let in_month = day.month() == m;
                let is_today = ms == today;
                let frame = self.frame.clone();
                let df_id = df.id.clone();
                let key = format!("cal:{ms}");
                let mut cell = div()
                    .id(SharedString::from(format!("cal-{ms}")))
                    .group("calday")
                    .relative()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .py(px(4.))
                    .border_r_1()
                    .border_b_1()
                    .border_color(t.border)
                    .when(!in_month, |d| d.bg(t.sidebar_bg))
                    .when(hover == Some(key.as_str()), |d| d.bg(t.accent_soft))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .px(px(4.))
                            .when(day.day() == 1, |d| {
                                d.child(
                                    div()
                                        .mr(px(2.))
                                        .text_size(px(12.5))
                                        .text_color(if in_month { t.text } else { t.text_muted })
                                        .child(format!("{}月", day.month())),
                                )
                            })
                            .child(
                                div()
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .text_size(px(12.5))
                                    .when(is_today, |d| d.bg(t.accent).text_color(white()))
                                    .when(!is_today, |d| d.text_color(if in_month { t.text } else { t.text_muted }))
                                    .child(day.day().to_string()),
                            )
                            .child(div().flex_1())
                            .when(editable, |d| {
                                d.child(
                                    div()
                                        .id(SharedString::from(format!("cal-add-{ms}")))
                                        .size(px(20.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .invisible()
                                        .group_hover("calday", |s| s.visible())
                                        .hover(|s| s.bg(t.hover))
                                        .cursor_pointer()
                                        .child(icon("plus", 12., t.text_secondary))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(rid) = this.add_record(vec![(df_id.clone(), Value::from(ms))], None, cx) {
                                                this.detail = Some(rid);
                                            }
                                        })),
                                )
                            }),
                    )
                    .child(
                        canvas(
                            move |b, _, _| {
                                frame.borrow_mut().insert(key.clone(), b);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    );
                let max = 4;
                for (rid, first, last) in items.iter().take(max) {
                    let title = self.b().title(&self.tid, rid);
                    cell = cell.child(div().px(px(4.)).child(self.cal_item(
                        format!("ci-{ms}-{rid}"),
                        rid,
                        title,
                        *first,
                        *last,
                        drag == Some(rid.as_str()),
                        t,
                        cx,
                    )));
                }
                if items.len() > max {
                    cell = cell.child(div().px(px(8.)).text_size(px(11.5)).text_color(t.text_muted).child(format!("还有 {} 条", items.len() - max)));
                }
                week = week.child(cell);
            }
            weeks = weeks.child(week);
        }
        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div().flex().flex_row().children(
                    ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                        .iter()
                        .map(|w| div().flex_1().pb(px(6.)).text_size(px(12.)).text_color(t.text_muted).child(*w)),
                ),
            )
            .child(weeks)
            .into_any_element()
    }

    /// 周 / 日视图：上方全天日程，下方 0–24 点时间轴（带时间的日程按时段放置）。
    #[allow(clippy::too_many_arguments)]
    fn cal_time_grid(
        &mut self,
        _v: &View,
        df: &Field,
        spans: &[(String, i64, i64)],
        anchor: i64,
        days: i64,
        drag: Option<&str>,
        hover: Option<&str>,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let d0 = Local.timestamp_millis_opt(anchor).single().map(|x| x.date_naive()).unwrap_or_default();
        let start = if days == 7 { d0 - chrono::Duration::days(d0.weekday().num_days_from_monday() as i64) } else { d0 };
        let today = formula::today();
        let wd = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
        let mut head = div().flex().flex_row().pl(px(52.));
        let mut allday = div().flex().flex_row().pl(px(52.)).border_b_1().border_color(t.border).min_h(px(30.));
        let mut cols = div().flex().flex_row().relative();
        // 时间刻度
        cols = cols.child(div().w(px(52.)).flex_none().flex().flex_col().children(
            (0..24).map(|h| div().h(px(HOUR_H)).pr(px(6.)).flex().justify_end().text_size(px(11.)).text_color(t.text_muted).child(format!("{h:02}:00"))),
        ));
        let editable = !df.kind.computed();
        for i in 0..days {
            let day = start + chrono::Duration::days(i);
            let ms = day_ms(day);
            let is_today = ms == today;
            head = head.child(
                div()
                    .flex_1()
                    .py(px(6.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(if is_today { t.accent } else { t.text_secondary })
                    .child(wd[day.weekday().num_days_from_monday() as usize])
                    .child(
                        div()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .when(is_today, |d| d.bg(t.accent).text_color(white()))
                            .child(day.day().to_string()),
                    ),
            );
            // 全天 / 跨天
            let mut ad = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(2.)).p(px(2.)).border_l_1().border_color(t.border).relative();
            let key = format!("cal:{ms}");
            let frame = self.frame.clone();
            let k2 = key.clone();
            ad = ad.when(hover == Some(key.as_str()), |d| d.bg(t.accent_soft)).child(
                canvas(
                    move |b, _, _| {
                        frame.borrow_mut().insert(k2.clone(), b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
            let mut col = div()
                .flex_1()
                .min_w(px(0.))
                .relative()
                .h(px(24. * HOUR_H))
                .border_l_1()
                .border_color(t.border)
                .when(is_today, |d| d.bg(t.accent_soft.opacity(0.4)));
            for h in 0..24 {
                let hk = format!("calh:{}", ms + h * 3_600_000);
                let frame = self.frame.clone();
                let hk2 = hk.clone();
                let df_id = df.id.clone();
                let at = ms + h * 3_600_000;
                col = col.child(
                    div()
                        .id(SharedString::from(hk.clone()))
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(h as f32 * HOUR_H))
                        .h(px(HOUR_H))
                        .border_b_1()
                        .border_color(t.border)
                        .when(hover == Some(hk.as_str()), |d| d.bg(t.accent_soft))
                        .child(
                            canvas(
                                move |b, _, _| {
                                    frame.borrow_mut().insert(hk2.clone(), b);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .when(editable, |d| {
                            d.on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                                if ev.click_count() >= 2 {
                                    if let Some(rid) = this.add_record(vec![(df_id.clone(), Value::from(at))], None, cx) {
                                        this.detail = Some(rid);
                                    }
                                }
                            }))
                        }),
                );
            }
            for (rid, s, e) in spans {
                let (s0, e0) = (formula::day_start(*s), formula::day_start(*e));
                if ms < s0 || ms > e0 {
                    continue;
                }
                let timed = s0 == e0 && *s != s0;
                let title = self.b().title(&self.tid, rid);
                if timed {
                    let st = Local.timestamp_millis_opt(*s).single().map(|x| x.hour() as f32 + x.minute() as f32 / 60.).unwrap_or(0.);
                    let en = if e > s {
                        Local.timestamp_millis_opt(*e).single().map(|x| x.hour() as f32 + x.minute() as f32 / 60.).unwrap_or(st + 1.)
                    } else {
                        st + 1.
                    };
                    let label = format!("{} {}", formula::fmt_date(*s, 1).split(' ').nth(1).unwrap_or(""), title);
                    col = col.child(div().absolute().left(px(2.)).right(px(2.)).top(px(st * HOUR_H + 1.)).h(px(((en - st).max(0.5)) * HOUR_H - 2.)).child(
                        self.cal_item(format!("ct-{ms}-{rid}"), rid, label, true, true, drag == Some(rid.as_str()), t, cx).h_full().items_start().pt(px(3.)),
                    ));
                } else {
                    ad = ad.child(self.cal_item(
                        format!("ca-{ms}-{rid}"),
                        rid,
                        title,
                        ms == s0 || i == 0,
                        ms == e0 || i == days - 1,
                        drag == Some(rid.as_str()),
                        t,
                        cx,
                    ));
                }
            }
            allday = allday.child(ad);
            cols = cols.child(col);
        }
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .border_1()
            .border_color(t.border)
            .rounded(px(8.))
            .overflow_hidden()
            .child(head.border_b_1().border_color(t.border))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .child(div().absolute().child(div().w(px(52.)).text_size(px(11.)).text_color(t.text_muted).pt(px(8.)).pl(px(6.)).child("全天")))
                    .child(allday.flex_1()),
            )
            .child(div().id("cal-hours").flex_1().min_h(px(0.)).overflow_y_scroll().child(cols))
            .into_any_element()
    }

    pub(super) fn calendar_drop(&mut self, d: Drag, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Drag::CalItem { rid, moved, .. } = d else { return };
        if !moved {
            self.detail = Some(rid);
            return;
        }
        let v = self.view();
        let Some(fid) = v.date_field.clone() else { return };
        if self.field(&fid).is_none_or(|f| f.kind.computed()) {
            return;
        }
        // 落在时间格上：设到那个小时；落在日期格上：保留原来的时分
        let target = {
            let frame = self.frame.borrow();
            frame
                .iter()
                .filter(|(k, b)| (k.starts_with("cal:") || k.starts_with("calh:")) && b.contains(&pos))
                .map(|(k, _)| k.clone())
                .max_by_key(|k| k.starts_with("calh:"))
        };
        let Some(target) = target else { return };
        let (timed, at) = match target.split_once(':') {
            Some(("calh", x)) => (true, x.parse::<i64>().unwrap_or(0)),
            Some((_, x)) => (false, x.parse::<i64>().unwrap_or(0)),
            None => return,
        };
        let cur = self.b().record(&self.tid, &rid).and_then(|r| r.cells.get(&fid)).and_then(|v| v.as_i64());
        let keep = if timed { 0 } else { cur.map(|c| c - formula::day_start(c)).unwrap_or(0) };
        let new_start = at + keep;
        let tid = self.tid.clone();
        // 有结束日期时整体平移，保持时长
        if let (Some(end), Some(c)) = (v.end_field.as_ref().filter(|e| self.field(e).is_some_and(|f| !f.kind.computed())), cur) {
            let e = self.b().record(&tid, &rid).and_then(|r| r.cells.get(end)).and_then(|v| v.as_i64());
            if let Some(e) = e {
                self.bm().set_cell(&tid, &rid, end, Value::from(e + (new_start - c)));
            }
        }
        self.bm().set_cell(&tid, &rid, &fid, Value::from(new_start));
        self.commit(cx);
    }
}
