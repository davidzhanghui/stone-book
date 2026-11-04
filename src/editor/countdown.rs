//! 倒计时块：彩色翻页数字「天 : 时 : 分 : 秒」居中显示，悬停右上角出「设置 / 颜色」；
//! 设置弹窗二选一：输入倒计时时长，或选择具体日期（日历 + 时分列表）；可勾选结束时显示气泡提醒。

use super::*;
use crate::model::{Block, BlockKind, COUNTDOWN_COLORS};
use crate::theme::Theme;
use crate::ui::{icon, Tooltip};
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};

const UNITS: [&str; 4] = ["天", "时", "分", "秒"];
const TILE: f32 = 44.;

#[derive(Clone, Copy, PartialEq)]
enum Picker {
    /// 日历，正在看的年月
    Date { year: i32, month: u32 },
    /// 时 / 分两列
    Time,
}

/// 倒计时设置弹窗的编辑状态（点确定才写回文档）。
pub struct CountdownEdit {
    pub block: usize,
    /// 选中「选择具体日期」（否则是「输入倒计时时长」）
    by_date: bool,
    /// 天 / 时 / 分 / 秒
    parts: [Entity<LineInput>; 4],
    date: NaiveDate,
    hour: u32,
    minute: u32,
    notify: bool,
    picker: Option<Picker>,
    hour_scroll: ScrollHandle,
    min_scroll: ScrollHandle,
    /// 上一帧测得的日期框左下角与弹窗原点（窗口坐标），下拉浮层据此贴在日期框下面
    field_at: Rc<std::cell::Cell<(f32, f32)>>,
    dlg_at: Rc<std::cell::Cell<(f32, f32)>>,
    _subs: Vec<Subscription>,
}

fn color_of(c: u8) -> Hsla {
    rgb(COUNTDOWN_COLORS[(c as usize).min(COUNTDOWN_COLORS.len() - 1)].1).into()
}

impl Editor {
    /// 打开倒计时设置弹窗。
    pub(super) fn open_countdown_edit(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(BlockKind::Countdown { target, notify, dur, .. }) = self.content.blocks.get(ix).map(|b| b.kind.clone()) else { return };
        let now = Local::now();
        // 具体日期：回显原目标时间；已过期或按时长设置的，默认当前时间
        let when = match dur {
            None if target > now.timestamp_millis() => Local.timestamp_millis_opt(target).single().unwrap_or(now),
            _ => now,
        };
        let secs = dur.unwrap_or(0).max(0);
        let vals = [secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60];
        let parts = vals.map(|v| {
            cx.new(|cx| {
                let mut i = LineInput::new("0", cx);
                i.set_text(&v.to_string(), cx);
                i.collapse_to_end(cx);
                i
            })
        });
        let on_input = |this: &mut Self, _: &Entity<LineInput>, ev: &InputEvent, window: &mut Window, cx: &mut Context<Self>| match ev {
            InputEvent::Submit { .. } => this.countdown_confirm(window, cx),
            InputEvent::Cancel => this.countdown_cancel(window, cx),
            _ => {
                // 在时长框里输入即切到「输入倒计时时长」
                if let Some(ce) = this.pop.countdown_edit.as_mut() {
                    ce.by_date = false;
                }
                cx.notify();
            }
        };
        let subs = parts.iter().map(|p| cx.subscribe_in(p, window, on_input)).collect();
        // 键盘焦点放在时长框上，弹窗开着时按键不会落进正文；回车确定、Esc 取消
        window.focus(&parts[0].focus_handle(cx));
        self.pop.countdown_color = None;
        self.pop.countdown_edit = Some(CountdownEdit {
            block: ix,
            by_date: dur.is_none(),
            parts,
            date: when.date_naive(),
            hour: when.hour(),
            minute: when.minute(),
            notify,
            picker: None,
            hour_scroll: ScrollHandle::new(),
            min_scroll: ScrollHandle::new(),
            field_at: Rc::default(),
            dlg_at: Rc::default(),
            _subs: subs,
        });
        cx.notify();
    }

    fn countdown_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ce) = self.pop.countdown_edit.as_ref() else { return };
        let now = Local::now().timestamp_millis();
        let (target, dur) = if ce.by_date {
            let dt = ce.date.and_hms_opt(ce.hour, ce.minute, 0).and_then(|d| d.and_local_timezone(Local).single());
            match dt.map(|d| d.timestamp_millis()) {
                Some(ms) if ms > now => (ms, None),
                _ => {
                    cx.emit(EditorEvent::Toast("请选择晚于当前的时间".into()));
                    return;
                }
            }
        } else {
            let v: Vec<i64> = ce.parts.iter().map(|p| p.read(cx).text().trim().parse::<i64>().unwrap_or(0).clamp(0, 99_999)).collect();
            let secs = v[0] * 86400 + v[1] * 3600 + v[2] * 60 + v[3];
            (now + secs * 1000, Some(secs))
        };
        let Some(ce) = self.pop.countdown_edit.take() else { return };
        let (ix, notify) = (ce.block, ce.notify);
        if let Some(blk) = self.content.blocks.get(ix).filter(|b| matches!(b.kind, BlockKind::Countdown { .. })) {
            self.countdown_fired.remove(&blk.id);
            self.update_kind(
                ix,
                |k| {
                    if let BlockKind::Countdown { target: t, notify: n, dur: d, .. } = k {
                        (*t, *n, *d) = (target, notify, dur);
                    }
                },
                cx,
            );
        }
        window.focus(&self.focus);
        cx.notify();
    }

    pub(super) fn countdown_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pop.countdown_edit.take().is_some() {
            window.focus(&self.focus);
            cx.notify();
        }
    }

    fn countdown_set_color(&mut self, ix: usize, c: u8, cx: &mut Context<Self>) {
        self.update_kind(
            ix,
            |k| {
                if let BlockKind::Countdown { color, .. } = k {
                    *color = c;
                }
            },
            cx,
        );
    }

    pub(super) fn render_countdown(&mut self, ix: usize, blk: &Block, selected: bool, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let BlockKind::Countdown { target, color, notify, .. } = &blk.kind else { return div().into_any_element() };
        let now = Local::now().timestamp_millis();
        // 向上取整：最后一秒显示 00:00:00:01，到点才归零
        let left = ((*target - now).max(0) + 999) / 1000;
        let done = *target <= now;
        // 刚到点（打开文档前早已结束的不算）：弹一次气泡提醒
        if done && *notify && now - *target < 5_000 && self.countdown_fired.insert(blk.id) {
            cx.defer_in(window, |_, _, cx| cx.emit(EditorEvent::Toast("⏰ 倒计时已结束".into())));
        }
        let col = color_of(*color);
        let id = blk.id as usize;
        let vals = [left / 86400, left / 3600 % 24, left / 60 % 60, left % 60];
        let tile = |v: i64, unit: &'static str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .child(
                    div()
                        .relative()
                        .min_w(px(TILE))
                        .h(px(TILE))
                        .px(px(6.))
                        .rounded(px(4.))
                        .overflow_hidden()
                        .bg(col)
                        .flex()
                        .items_center()
                        .justify_center()
                        // 上半截略亮，做出翻页钟的质感
                        .child(div().absolute().top_0().left_0().w_full().h(px(TILE / 2.)).bg(gpui::white().opacity(0.12)))
                        .child(div().text_size(px(24.)).font_weight(FontWeight::BOLD).text_color(gpui::white()).child(format!("{v:02}"))),
                )
                .child(div().text_size(px(12.)).text_color(col).child(unit))
        };
        let colon =
            || div().h(px(TILE)).w(px(12.)).flex().items_center().justify_center().text_size(px(20.)).font_weight(FontWeight::BOLD).text_color(col).child(":");
        let mut digits = div().flex().flex_row().items_start();
        for (i, v) in vals.into_iter().enumerate() {
            if i > 0 {
                digits = digits.child(colon());
            }
            digits = digits.child(tile(v, UNITS[i]));
        }

        let palette_open = self.pop.countdown_color == Some(ix);
        let active = palette_open || self.pop.countdown_edit.as_ref().is_some_and(|ce| ce.block == ix);
        let tool = |name: &'static str, tip: &'static str, key: &'static str| {
            div()
                .id((key, id))
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .child(icon(name, 15., t.text_secondary))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
        };
        let tools = div()
            .absolute()
            .top(px(8.))
            .right(px(8.))
            .flex()
            .flex_row()
            .gap(px(2.))
            .when(!active && !selected, |d| d.invisible().group_hover("cd", |s| s.visible()))
            .child(tool("pencil", "设置", "cd-set").on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_countdown_edit(ix, window, cx);
                }),
            ))
            .child(
                div()
                    .relative()
                    .child(tool("palette", "颜色", "cd-color").on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.pop.countdown_color = if this.pop.countdown_color == Some(ix) { None } else { Some(ix) };
                            cx.notify();
                        }),
                    ))
                    .when(palette_open, |d| d.child(div().absolute().top(px(30.)).right_0().child(self.render_countdown_palette(ix, *color, t, cx)))),
            );

        div()
            .id(("cd", id))
            .group("cd")
            .relative()
            .w_full()
            .pt(px(if done && *notify { 12. } else { 16. }))
            .pb(px(14.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(if selected { t.accent } else { t.border })
            .when(done && *notify, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .px(px(10.))
                        .py(px(2.))
                        .rounded(px(12.))
                        .bg(col.opacity(0.12))
                        .text_size(px(12.))
                        .text_color(col)
                        .child(icon("clock", 12., col))
                        .child("倒计时已结束"),
                )
            })
            .child(digits)
            .child(tools)
            .child(self.void_marker(ix))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    this.mouse_select(Pos::new(ix, 0, 0), 1, ev.modifiers.shift, cx);
                    // 双击打开设置
                    if ev.click_count >= 2 {
                        this.open_countdown_edit(ix, window, cx);
                    }
                }),
            )
            .into_any_element()
    }

    /// 颜色浮层：7 个色块，点选即生效。
    fn render_countdown_palette(&self, ix: usize, cur: u8, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        deferred(
            anchored().anchor(Corner::TopRight).snap_to_window_with_margin(px(8.)).child(
                div()
                    .id("cd-palette")
                    .occlude()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .bg(t.panel_bg)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(8.))
                    .shadow_lg()
                    .child(div().text_size(px(12.)).text_color(t.text).child("颜色"))
                    .child(div().flex().flex_row().gap(px(4.)).children(COUNTDOWN_COLORS.iter().enumerate().map(|(i, &(name, c))| {
                        let on = cur as usize == i;
                        div()
                            .id(("cd-sw", i))
                            .size(px(24.))
                            .p(px(2.))
                            .rounded(px(5.))
                            .border_2()
                            .border_color(if on { t.accent } else { gpui::transparent_black() })
                            .cursor_pointer()
                            .hover(|s| s.border_color(if on { t.accent } else { t.border_strong }))
                            .child(div().size_full().rounded(px(3.)).bg(rgb(c)))
                            .tooltip(move |_, cx| cx.new(|_| Tooltip(name.into())).into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.countdown_set_color(ix, i as u8, cx);
                                }),
                            )
                    })))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    // 推迟到本次点击分发完再关：点在「颜色」按钮上时由按钮自己切换
                    .on_mouse_down_out(cx.listener(move |_, _, window, cx| {
                        cx.defer_in(window, move |this, _, cx| {
                            if this.pop.countdown_color == Some(ix) {
                                this.pop.countdown_color = None;
                                cx.notify();
                            }
                        })
                    })),
            ),
        )
        .with_priority(4)
    }

    /// 倒计时设置弹窗（居中模态）。
    pub(super) fn render_countdown_dialog(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ce = self.pop.countdown_edit.as_ref()?;
        let vp = window.viewport_size();
        let picker = match ce.picker {
            Some(Picker::Date { year, month }) => Some(self.render_calendar_pop(ce, year, month, t, cx).into_any_element()),
            Some(Picker::Time) => Some(self.render_time_pop(ce, t, cx).into_any_element()),
            None => None,
        };
        let set = |f: fn(&mut CountdownEdit)| {
            cx.listener(move |this: &mut Self, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if let Some(ce) = this.pop.countdown_edit.as_mut() {
                    f(ce);
                }
                cx.notify();
            })
        };
        let radio = |on: bool, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(14.))
                .text_color(t.text)
                .child(
                    div()
                        .size(px(16.))
                        .rounded_full()
                        .border_1()
                        .border_color(if on { t.accent } else { t.border_strong })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(on, |d| d.border_color(t.accent).child(div().size(px(8.)).rounded_full().bg(t.accent))),
                )
                .child(label)
        };
        let section = |key: &'static str, on: bool| {
            div()
                .id(key)
                .p(px(14.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .rounded(px(6.))
                .border_1()
                .border_color(if on { t.accent } else { t.border })
                .cursor_pointer()
        };

        // 输入倒计时时长
        let by_date = ce.by_date;
        let mut dur_row = div().flex().flex_row().items_center().gap(px(8.));
        for (i, p) in ce.parts.iter().enumerate() {
            let fh = p.focus_handle(cx);
            let focused = fh.is_focused(window);
            dur_row = dur_row
                .child(
                    div()
                        .id(("cd-part", i))
                        .w(px(52.))
                        .h(px(32.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(if focused && !by_date { t.accent } else { t.border })
                        .when(by_date, |d| d.bg(t.hover).text_color(t.text_muted))
                        .when(!by_date, |d| d.text_color(t.text))
                        .text_size(px(14.))
                        .cursor_text()
                        .child(div().flex_1().min_w(px(0.)).line_height(px(20.)).child(p.clone()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(ce) = this.pop.countdown_edit.as_mut() {
                                    ce.by_date = false;
                                    ce.picker = None;
                                }
                                window.focus(&fh);
                                cx.notify();
                            }),
                        ),
                )
                .child(div().text_size(px(14.)).text_color(t.text).mr(px(4.)).child(UNITS[i]));
        }
        let dur_sec = section("cd-sec-dur", !by_date).child(radio(!by_date, "输入倒计时时长")).child(dur_row).on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                if let Some(ce) = this.pop.countdown_edit.as_mut() {
                    ce.by_date = false;
                    ce.picker = None;
                    window.focus(&ce.parts[0].focus_handle(cx));
                }
                cx.notify();
            }),
        );

        // 选择具体日期
        let seg = |key: &'static str, text: String, open: bool| {
            div().id(key).px(px(2.)).rounded(px(3.)).when(open, |d| d.bg(t.accent_soft)).hover(|s| s.bg(t.hover)).child(text)
        };
        let date_open = matches!(ce.picker, Some(Picker::Date { .. }));
        let time_open = ce.picker == Some(Picker::Time);
        let open_date = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            if let Some(ce) = this.pop.countdown_edit.as_mut() {
                ce.by_date = true;
                ce.picker = Some(Picker::Date { year: ce.date.year(), month: ce.date.month() });
            }
            cx.notify();
        });
        let field = div()
            .w(px(240.))
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(6.))
            .border_1()
            .border_color(if by_date && ce.picker.is_some() { t.accent } else { t.border })
            .when(!by_date, |d| d.bg(t.hover).text_color(t.text_muted))
            .when(by_date, |d| d.text_color(t.text))
            .text_size(px(14.))
            .child(seg("cd-date", ce.date.format("%Y-%m-%d").to_string(), date_open).on_mouse_down(MouseButton::Left, open_date))
            .child(seg("cd-time", format!("{:02}:{:02}", ce.hour, ce.minute), time_open).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(ce) = this.pop.countdown_edit.as_mut() {
                        ce.by_date = true;
                        ce.picker = Some(Picker::Time);
                        // 选中项滚到列顶（每项 28px）
                        ce.hour_scroll.set_offset(point(px(0.), px(-(ce.hour as f32) * 28.)));
                        ce.min_scroll.set_offset(point(px(0.), px(-(ce.minute as f32) * 28.)));
                    }
                    cx.notify();
                }),
            ))
            .child(div().flex_1())
            .child(div().id("cd-cal").cursor_pointer().child(icon("calendar", 15., t.text_muted)).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(ce) = this.pop.countdown_edit.as_mut() {
                        ce.by_date = true;
                        ce.picker = Some(Picker::Date { year: ce.date.year(), month: ce.date.month() });
                    }
                    cx.notify();
                }),
            ));
        let date_sec = section("cd-sec-date", by_date)
            .child(radio(by_date, "选择具体日期"))
            .child(div().relative().w(px(240.)).child(field).child({
                let at = ce.field_at.clone();
                canvas(move |b, _, _| at.set((b.origin.x.into(), b.bottom().into())), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(ce) = this.pop.countdown_edit.as_mut() {
                        ce.by_date = true;
                        ce.picker = None;
                    }
                    window.focus(&this.focus);
                    cx.notify();
                }),
            );

        let checkbox = div()
            .id("cd-notify")
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .text_color(t.text)
            .cursor_pointer()
            .child(
                div()
                    .size(px(16.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(if ce.notify { t.accent } else { t.border_strong })
                    .when(ce.notify, |d| d.bg(t.accent))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(ce.notify, |d| d.child(icon("check", 12., gpui::white()))),
            )
            .child("倒计时结束时显示气泡提醒")
            .on_mouse_down(MouseButton::Left, set(|ce| ce.notify = !ce.notify));
        let button = |key: &'static str, label: &'static str, primary: bool| {
            div()
                .id(key)
                .h(px(32.))
                .px(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_size(px(14.))
                .cursor_pointer()
                .when(primary, |d| d.bg(t.accent).text_color(gpui::white()).hover(|s| s.opacity(0.9)))
                .when(!primary, |d| d.border_1().border_color(t.border).text_color(t.text).hover(|s| s.bg(t.hover)))
                .child(label)
        };

        let dialog = div()
            .id("cd-dialog")
            .occlude()
            .w(px(520.))
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .bg(t.panel_bg)
            .rounded(px(10.))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).text_color(t.text).child("倒计时设置"))
                    .child(
                        div()
                            .id("cd-close")
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .child(icon("x", 16., t.text_secondary))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.countdown_cancel(window, cx);
                                }),
                            ),
                    ),
            )
            .child(dur_sec)
            .child(date_sec)
            .child(
                div().pt(px(4.)).flex().items_center().justify_between().child(checkbox).child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .child(button("cd-cancel", "取消", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.countdown_cancel(window, cx);
                            }),
                        ))
                        .child(button("cd-ok", "确定", true).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.countdown_confirm(window, cx);
                            }),
                        )),
                ),
            )
            // 下拉浮层作为最后一个子元素，才能盖住下面的勾选框和按钮
            .children(picker.map(|p| {
                let ((fx, fy), (dx, dy)) = (ce.field_at.get(), ce.dlg_at.get());
                div().absolute().left(px(fx - dx)).top(px(fy - dy + 4.)).child(p)
            }))
            .child({
                let at = ce.dlg_at.clone();
                canvas(move |b, _, _| at.set((b.origin.x.into(), b.origin.y.into())), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
            })
            .on_mouse_down(MouseButton::Left, set(|ce| ce.picker = None));

        Some(
            deferred(
                anchored().position(point(px(0.), px(0.))).child(
                    div()
                        .id("cd-mask")
                        .w(vp.width)
                        .h(vp.height)
                        .occlude()
                        .bg(gpui::black().opacity(0.4))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(dialog)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                ),
            )
            .with_priority(8)
            .into_any_element(),
        )
    }

    /// 日历浮层：年月切换、6×7 日期格、「今天」。
    fn render_calendar_pop(&self, ce: &CountdownEdit, year: i32, month: u32, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let today = Local::now().date_naive();
        let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(today);
        let lead = first.weekday().num_days_from_sunday() as i64;
        let start = first - chrono::Duration::days(lead);
        let nav = |key: &'static str, name: &'static str, dm: i32| {
            div()
                .id(key)
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .child(icon(name, 14., t.text_secondary))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(ce) = this.pop.countdown_edit.as_mut() {
                            if let Some(Picker::Date { year, month }) = ce.picker.as_mut() {
                                let m = *year * 12 + *month as i32 - 1 + dm;
                                (*year, *month) = (m.div_euclid(12), m.rem_euclid(12) as u32 + 1);
                            }
                        }
                        cx.notify();
                    }),
                )
        };
        let head = div()
            .flex()
            .items_center()
            .justify_between()
            .pb(px(6.))
            .child(div().flex().gap(px(2.)).child(nav("cd-py", "chevron-left", -12)).child(nav("cd-pm", "chevron-left", -1)))
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(t.text).child(format!("{year}年 {month}月")))
            .child(div().flex().gap(px(2.)).child(nav("cd-nm", "chevron-right", 1)).child(nav("cd-ny", "chevron-right", 12)));
        let cell = |w: f32| div().w(px(w)).h(px(30.)).flex().items_center().justify_center();
        let week = div().flex().children(["日", "一", "二", "三", "四", "五", "六"].map(|d| cell(34.).text_size(px(12.)).text_color(t.text_muted).child(d)));
        let mut grid = div().flex().flex_col();
        for r in 0..6 {
            let mut row = div().flex();
            for c in 0..7 {
                let d = start + chrono::Duration::days(r * 7 + c);
                let (cur, sel, is_today, past) = (d.month() == month, d == ce.date, d == today, d < today);
                row = row.child(
                    cell(34.).child(
                        div()
                            .id(("cd-day", (r * 7 + c) as usize))
                            .size(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .text_size(px(13.))
                            .text_color(if sel {
                                gpui::white()
                            } else if past || !cur {
                                t.placeholder
                            } else if is_today {
                                t.accent
                            } else {
                                t.text
                            })
                            .when(sel, |s| s.bg(t.accent))
                            .when(is_today && !sel, |s| s.border_1().border_color(t.accent))
                            .when(!past, |s| s.cursor_pointer().when(!sel, |s| s.hover(|s| s.bg(t.hover))))
                            .child(d.day().to_string())
                            .when(!past, |s| {
                                s.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        if let Some(ce) = this.pop.countdown_edit.as_mut() {
                                            ce.date = d;
                                            ce.picker = None;
                                        }
                                        cx.notify();
                                    }),
                                )
                            }),
                    ),
                );
            }
            grid = grid.child(row);
        }
        let today_btn = div()
            .id("cd-today")
            .mt(px(6.))
            .pt(px(6.))
            .border_t_1()
            .border_color(t.border)
            .flex()
            .justify_center()
            .text_size(px(13.))
            .text_color(t.accent)
            .cursor_pointer()
            .child("今天")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(ce) = this.pop.countdown_edit.as_mut() {
                        ce.date = today;
                        ce.picker = None;
                    }
                    cx.notify();
                }),
            );
        self.picker_shell("cd-cal-pop", t, cx, div().p(px(10.)).child(head).child(week).child(grid).child(today_btn))
    }

    /// 时间浮层：时、分两列，可滚动。
    fn render_time_pop(&self, ce: &CountdownEdit, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let column = |key: &'static str, n: u32, cur: u32, handle: &ScrollHandle, hour: bool| {
            div().id(key).w(px(60.)).h(px(196.)).p(px(4.)).overflow_y_scroll().track_scroll(handle).children((0..n).map(|v| {
                let on = v == cur;
                div()
                    .id((key, v as usize))
                    .h(px(28.))
                    .flex_none()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .text_size(px(13.))
                    .text_color(if on { t.accent } else { t.text })
                    .when(on, |d| d.bg(t.accent_soft))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(format!("{v:02}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(ce) = this.pop.countdown_edit.as_mut() {
                                if hour {
                                    ce.hour = v;
                                } else {
                                    ce.minute = v;
                                }
                            }
                            cx.notify();
                        }),
                    )
            }))
        };
        let body = div().flex().flex_row().child(column("cd-hours", 24, ce.hour, &ce.hour_scroll, true)).child(div().w(px(1.)).bg(t.border)).child(column(
            "cd-mins",
            60,
            ce.minute,
            &ce.min_scroll,
            false,
        ));
        self.picker_shell("cd-time-pop", t, cx, body)
    }

    fn picker_shell(&self, key: &'static str, t: &Theme, cx: &mut Context<Self>, body: Div) -> impl IntoElement {
        div()
            .id(key)
            .occlude()
            .bg(t.panel_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(8.))
            .shadow_lg()
            .child(body)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(move |_, _, window, cx| {
                let key_open = key;
                cx.defer_in(window, move |this, _, cx| {
                    // 点在日期框的另一段上时，那一段已换成自己的浮层，不要关掉
                    if let Some(ce) = this.pop.countdown_edit.as_mut() {
                        let same = match ce.picker {
                            Some(Picker::Date { .. }) => key_open == "cd-cal-pop",
                            Some(Picker::Time) => key_open == "cd-time-pop",
                            None => false,
                        };
                        if same {
                            ce.picker = None;
                            cx.notify();
                        }
                    }
                })
            }))
    }
}
