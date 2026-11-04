//! 颜色面板（同类 11 × 6：首行标准色 + 5 行由浅到深）、最近使用、更多颜色（色相 / 饱和度 / 明度取色器 + HEX / RGB），
//! 以及边框面板（全部 / 外 / 内 / 内部横线 / 内部竖线 / 四边 / 两条对角线 / 无，线条颜色、线型）。

use super::*;
use crate::theme::Theme;
use crate::ui::{icon, Tooltip};
use gpui::prelude::FluentBuilder;
use std::cell::Cell as StdCell;
use std::rc::Rc;

/// 11 列基础色：灰 蓝 天蓝 青 绿 黄绿 黄 橙 红 玫红 紫
const BASE: [u32; 11] = [0x8f959e, 0x3370ff, 0x14c0ff, 0x2ec4b6, 0x34c724, 0xa2d20c, 0xffc60a, 0xff8800, 0xf54a45, 0xf01d94, 0x7f3bf5];

fn mix(a: u32, b: u32, k: f32) -> u32 {
    let ch = |s: u32| (((a >> s) & 255) as f32 * (1. - k) + ((b >> s) & 255) as f32 * k).round() as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// 第 row 行第 col 列的颜色（0 行为标准色：白 + 10 种彩色）
pub(crate) fn palette_color(row: usize, col: usize) -> u32 {
    if col == 0 {
        return [0xffffff, 0xf2f3f5, 0xdee0e3, 0x8f959e, 0x373c43, 0x1f2329][row];
    }
    let b = BASE[col];
    match row {
        0 => b,
        1 => mix(b, 0xffffff, 0.88),
        2 => mix(b, 0xffffff, 0.62),
        3 => mix(b, 0xffffff, 0.2),
        4 => mix(b, 0x000000, 0.3),
        _ => mix(b, 0x000000, 0.58),
    }
}

/// 更多颜色取色器状态
pub(crate) struct Picker {
    /// "color" / "fill" / "border"
    pub target: &'static str,
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub hex: Entity<LineInput>,
    pub sq: Rc<StdCell<Bounds<Pixels>>>,
    pub bar: Rc<StdCell<Bounds<Pixels>>>,
    _sub: Subscription,
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1. - ((h * 6.) % 2. - 1.).abs());
    let m = v - c;
    let (r, g, b) = match (h * 6.) as i32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let f = |v: f32| ((v + m) * 255.).round().clamp(0., 255.) as u32;
    (f(r) << 16) | (f(g) << 8) | f(b)
}

fn rgb_to_hsv(c: u32) -> (f32, f32, f32) {
    let (r, g, b) = (((c >> 16) & 255) as f32 / 255., ((c >> 8) & 255) as f32 / 255., (c & 255) as f32 / 255.);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let d = mx - mn;
    let h = if d == 0. {
        0.
    } else if mx == r {
        ((g - b) / d).rem_euclid(6.) / 6.
    } else if mx == g {
        ((b - r) / d + 2.) / 6.
    } else {
        ((r - g) / d + 4.) / 6.
    };
    (h, if mx == 0. { 0. } else { d / mx }, mx)
}

impl SheetEditor {
    /// 记录最近使用的颜色（最多 10 个）。
    pub(crate) fn remember_color(hex: &str) {
        if hex.len() != 7 {
            return;
        }
        RECENT_COLORS.with(|r| {
            let mut r = r.borrow_mut();
            r.retain(|c| c != hex);
            r.insert(0, hex.to_string());
            r.truncate(10);
        });
    }

    /// 颜色面板当前选中的颜色（#rrggbb）。
    fn panel_color(&self, target: &str) -> String {
        match target {
            "fill" => self.model.borrow().resolve_color(&self.cur_style().fill.color),
            "border" => self.border_color.clone(),
            "opt-color" => match &self.dlg {
                Some(super::dialogs::Dlg::List(d)) => {
                    d.color_pick.and_then(|i| d.opts.get(i)).map(|o| format!("#{:06x}", super::ext::opt_colors(o.1, &o.2).0)).unwrap_or_default()
                }
                _ => String::new(),
            },
            "spark-color" | "spark-mark" => match self.side {
                Some(super::dialogs::Side::Spark(c)) => self
                    .sx()
                    .cell(c.r, c.c)
                    .and_then(|x| {
                        x.spark.as_ref().map(|s| {
                            if target == "spark-color" {
                                s.color.clone()
                            } else {
                                s.mark_color(self.spark_open.saturating_sub(8) as usize).unwrap_or_default().to_string()
                            }
                        })
                    })
                    .unwrap_or_default(),
                _ => String::new(),
            },
            _ => self.model.borrow().resolve_color(&self.cur_style().font.color),
        }
    }

    /// 下拉选项 / 迷你图的颜色（颜色面板命令 `opt-color:` / `spark-color:`，空 = 恢复默认）。
    pub(crate) fn apply_obj_color(&mut self, target: &str, hex: &str, cx: &mut Context<Self>) {
        Self::remember_color(hex);
        match target {
            "opt-color" => {
                if let Some(super::dialogs::Dlg::List(d)) = &mut self.dlg {
                    if let Some(o) = d.color_pick.and_then(|i| d.opts.get_mut(i)) {
                        o.2 = hex.to_lowercase();
                    }
                    d.color_pick = None;
                }
            }
            _ => {
                if let Some(super::dialogs::Side::Spark(c)) = self.side {
                    let hex = hex.to_lowercase();
                    let k = self.spark_open.saturating_sub(8) as usize;
                    let mark = target == "spark-mark";
                    self.edit_spark(
                        c,
                        |s| {
                            if mark {
                                if s.mark_colors.len() <= k {
                                    s.mark_colors.resize(k + 1, String::new());
                                }
                                s.mark_colors[k] = hex;
                            } else {
                                s.color = hex;
                            }
                        },
                        cx,
                    );
                }
                self.spark_open = 0;
            }
        }
        self.picker = None;
        cx.notify();
    }

    pub(crate) fn open_picker(&mut self, target: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let cur = self.panel_color(target);
        let c = u32::from_str_radix(cur.trim_start_matches('#'), 16).unwrap_or(0xf54a45);
        let (h, s, v) = rgb_to_hsv(c);
        let hex = cx.new(|cx| LineInput::new("#RRGGBB", cx));
        hex.update(cx, |i, cx| i.set_text(&format!("#{c:06X}"), cx));
        let sub = cx.subscribe_in(&hex, window, |this, inp, ev: &InputEvent, _, cx| match ev {
            InputEvent::Changed => {
                let t = inp.read(cx).text();
                if let Ok(c) = u32::from_str_radix(t.trim().trim_start_matches('#'), 16).map_err(|_| ()).and_then(|c| {
                    if t.trim().trim_start_matches('#').len() == 6 {
                        Ok(c)
                    } else {
                        Err(())
                    }
                }) {
                    if let Some(p) = &mut this.picker {
                        (p.h, p.s, p.v) = rgb_to_hsv(c);
                    }
                    cx.notify();
                }
            }
            InputEvent::Submit { .. } => this.picker_apply(cx),
            InputEvent::Cancel => {
                this.picker = None;
                cx.notify();
            }
            _ => {}
        });
        self.picker = Some(Picker { target, h, s, v, hex, sq: Default::default(), bar: Default::default(), _sub: sub });
        cx.notify();
    }

    fn picker_set(&mut self, f: impl FnOnce(&mut Picker), cx: &mut Context<Self>) {
        if let Some(p) = &mut self.picker {
            f(p);
            let hex = format!("#{:06X}", hsv_to_rgb(p.h, p.s, p.v));
            p.hex.update(cx, |i, cx| i.set_text(&hex, cx));
        }
        cx.notify();
    }

    fn picker_apply(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.picker.take() else { return };
        let hex = format!("#{:06x}", hsv_to_rgb(p.h, p.s, p.v));
        Self::remember_color(&hex);
        self.pop = None;
        match p.target {
            "border" => self.border_color = hex,
            "fill" => self.style_path("fill.color", &hex, cx),
            t @ ("opt-color" | "spark-color" | "spark-mark") => self.apply_obj_color(t, &hex, cx),
            _ => self.style_path("font.color", &hex, cx),
        }
        cx.notify();
    }

    /// 颜色面板：`cmd` 为 color / fill / border。
    pub(crate) fn color_panel(&self, cmd: &'static str, t: &Theme, cx: &mut Context<Self>) -> Div {
        let cur = self.panel_color(cmd).to_lowercase();
        let recent = RECENT_COLORS.with(|r| r.borrow().clone());
        let swatch = |id: SharedString, hex: String, cx: &mut Context<Self>| {
            let on = hex == cur;
            let c = hex_to_hsla(&hex).unwrap_or(t.bg);
            let tip = hex.to_uppercase();
            div()
                .id(id)
                .size(px(20.))
                .rounded(px(4.))
                .bg(c)
                .border_1()
                .border_color(if on { t.accent } else { hsla(0., 0., 0.5, 0.18) })
                .when(on, |d| d.flex().items_center().justify_center().child(icon("sh-check", 12., if c.l > 0.7 { rgb(0x1f2329).into() } else { white() })))
                .cursor_pointer()
                .hover(|s| s.border_color(t.accent))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.clone().into())).into())
                .on_click(Self::cmd(format!("{}:{hex}", if cmd == "border" { "border-color" } else { cmd }), cx))
        };
        let reset = match cmd {
            "color" => "默认颜色",
            "fill" => "无填充",
            "opt-color" | "spark-color" => "恢复默认",
            _ => "默认（黑色）",
        };
        let mut p = Self::panel(t).p(px(10.)).gap(px(6.)).w(px(276.));
        p = p.child(
            div()
                .id("pal-reset")
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .border_1()
                .border_color(t.border)
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .text_size(px(12.5))
                .child(reset)
                .on_click(Self::cmd(format!("{}:", if cmd == "border" { "border-color" } else { cmd }), cx)),
        );
        for row in 0..6 {
            let cells: Vec<_> = (0..11).map(|col| swatch(format!("pal-{cmd}-{row}-{col}").into(), format!("#{:06x}", palette_color(row, col)), cx)).collect();
            p = p.child(div().flex().flex_row().gap(px(4.)).when(row == 1, |d| d.mt(px(6.))).children(cells));
        }
        if !recent.is_empty() {
            let cells: Vec<_> = recent.into_iter().enumerate().map(|(i, h)| swatch(format!("pal-{cmd}-r{i}").into(), h, cx)).collect();
            p = p
                .child(div().mt(px(4.)).text_size(px(12.)).text_color(t.text_muted).child("最近使用"))
                .child(div().flex().flex_row().gap(px(4.)).children(cells));
        }
        p = p.child(Self::sep(t).mx(px(0.))).child(
            div()
                .id("pal-more")
                .h(px(28.))
                .px(px(4.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .child(div().size(px(14.)).rounded_full().bg(linear_gradient(135., linear_color_stop(rgb(0xf54a45), 0.), linear_color_stop(rgb(0x3370ff), 1.))))
                .child(div().flex_1().child("更多颜色"))
                .child(icon("chevron-right", 13., t.text_muted))
                .on_click(cx.listener(move |this, _, w, cx| this.open_picker(cmd, w, cx))),
        );
        if let Some(pk) = self.picker.as_ref().filter(|p| p.target == cmd) {
            p = p.child(self.picker_el(pk, t, cx));
        }
        p
    }

    fn picker_el(&self, pk: &Picker, t: &Theme, cx: &mut Context<Self>) -> Div {
        let (w, h) = (256., 140.);
        let hue = hsv_to_rgb(pk.h, 1., 1.);
        let cur = hsv_to_rgb(pk.h, pk.s, pk.v);
        let (sq, bar) = (pk.sq.clone(), pk.bar.clone());
        let (sq2, bar2) = (pk.sq.clone(), pk.bar.clone());
        let sv_at =
            move |p: Point<Pixels>, b: Bounds<Pixels>| (((p.x - b.left()) / b.size.width).clamp(0., 1.), (1. - (p.y - b.top()) / b.size.height).clamp(0., 1.));
        let h_at = move |p: Point<Pixels>, b: Bounds<Pixels>| ((p.x - b.left()) / b.size.width).clamp(0., 0.999);
        let stops = [0xff0000, 0xffff00, 0x00ff00, 0x00ffff, 0x0000ff, 0xff00ff, 0xff0000];
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .mt(px(4.))
            .child(
                div()
                    .id("pk-sq")
                    .relative()
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(4.))
                    .overflow_hidden()
                    .bg(rgb(hue))
                    .cursor_pointer()
                    .child(div().absolute().inset_0().bg(linear_gradient(90., linear_color_stop(white(), 0.), linear_color_stop(white().opacity(0.), 1.))))
                    .child(div().absolute().inset_0().bg(linear_gradient(180., linear_color_stop(black().opacity(0.), 0.), linear_color_stop(black(), 1.))))
                    .child(
                        div()
                            .absolute()
                            .left(px(pk.s * w - 6.))
                            .top(px((1. - pk.v) * h - 6.))
                            .size(px(12.))
                            .rounded_full()
                            .border_2()
                            .border_color(white())
                            .shadow_sm(),
                    )
                    .child(canvas(move |b, _, _| sq.set(b), |_, _, _, _| {}).absolute().inset_0())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            let (s, v) = sv_at(ev.position, sq2.get());
                            this.picker_set(|p| (p.s, p.v) = (s, v), cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                        if ev.pressed_button == Some(MouseButton::Left) {
                            if let Some(b) = this.picker.as_ref().map(|p| p.sq.get()) {
                                let (s, v) = sv_at(ev.position, b);
                                this.picker_set(|p| (p.s, p.v) = (s, v), cx);
                            }
                        }
                    })),
            )
            .child(
                div()
                    .id("pk-bar")
                    .relative()
                    .w(px(w))
                    .h(px(10.))
                    .rounded(px(5.))
                    .overflow_hidden()
                    .flex()
                    .flex_row()
                    .cursor_pointer()
                    .children((0..6).map(|i| {
                        div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(rgb(stops[i]), 0.), linear_color_stop(rgb(stops[i + 1]), 1.)))
                    }))
                    .child(div().absolute().left(px(pk.h * w - 3.)).top_0().w(px(6.)).h_full().rounded(px(3.)).border_2().border_color(white()))
                    .child(canvas(move |b, _, _| bar.set(b), |_, _, _, _| {}).absolute().inset_0())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            let h = h_at(ev.position, bar2.get());
                            this.picker_set(|p| p.h = h, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                        if ev.pressed_button == Some(MouseButton::Left) {
                            if let Some(b) = this.picker.as_ref().map(|p| p.bar.get()) {
                                let h = h_at(ev.position, b);
                                this.picker_set(|p| p.h = h, cx);
                            }
                        }
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .child(div().size(px(22.)).rounded(px(4.)).bg(rgb(cur)).border_1().border_color(t.border))
                    .child(div().text_color(t.text_muted).child("HEX"))
                    .child(
                        div()
                            .w(px(76.))
                            .h(px(24.))
                            .px(px(6.))
                            .rounded(px(5.))
                            .border_1()
                            .border_color(t.border_strong)
                            .line_height(px(22.))
                            .child(pk.hex.clone()),
                    )
                    .child(div().text_color(t.text_muted).child(format!("R {} G {} B {}", (cur >> 16) & 255, (cur >> 8) & 255, cur & 255)))
                    .child(div().flex_1())
                    .child(
                        crate::base::widgets::primary_btn("pk-ok", "确定", t)
                            .h(px(24.))
                            .px(px(10.))
                            .on_click(cx.listener(|this, _, _, cx| this.picker_apply(cx))),
                    ),
            )
    }

    /// 边框面板
    pub(crate) fn border_panel(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let b = |id: &'static str, ic: &'static str, tip: &'static str, cmd: &'static str, cx: &mut Context<Self>| {
            Self::ib(id, ic, tip, false, t).size(px(32.)).on_click(Self::cmd(cmd, cx))
        };
        const STYLES: [(&str, &str); 6] =
            [("thin", "细实线"), ("medium", "中实线"), ("thick", "粗实线"), ("dashed", "虚线"), ("dotted", "点线"), ("double", "双线")];
        let style_label = STYLES.iter().find(|s| s.0 == self.border_style).map(|s| s.1).unwrap_or("细实线");
        let line = |style: &str| {
            let (w, dash) = match style {
                "medium" => (2., false),
                "thick" => (3., false),
                "dashed" => (1., true),
                "dotted" => (1., true),
                "double" => (3., false),
                _ => (1., false),
            };
            div()
                .w(px(70.))
                .h(px(w))
                .when(!dash, |d| d.bg(t.text))
                .when(dash, |d| d.border_t_1().border_dashed().border_color(t.text))
                .when(style == "double", |d| d.bg(t.bg).border_t_1().border_b_1().border_color(t.text))
        };
        let bc = hex_to_hsla(&self.border_color).unwrap_or(t.text);
        Self::panel(t)
            .p(px(8.))
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(4.))
                    .child(b("b-all", "sh-b-all", "所有边框", "border:all", cx))
                    .child(b("b-outer", "sh-b-outer", "外边框", "border:outer", cx))
                    .child(b("b-inner", "sh-b-inner", "内部边框", "border:inner", cx))
                    .child(b("b-cv", "sh-b-cv", "内部竖线", "border:center-v", cx))
                    .child(b("b-ch", "sh-b-ch", "内部横线", "border:center-h", cx))
                    .child(b("b-dd", "sh-b-diag-down", "左上到右下斜线", "border:diag-down", cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(4.))
                    .child(b("b-left", "sh-b-left", "左边框 ⌥⇧4", "border:left", cx))
                    .child(b("b-right", "sh-b-right", "右边框 ⌥⇧2", "border:right", cx))
                    .child(b("b-top", "sh-b-top", "上边框 ⌥⇧1", "border:top", cx))
                    .child(b("b-bottom", "sh-b-bottom", "下边框 ⌥⇧3", "border:bottom", cx))
                    .child(b("b-none", "sh-b-none", "无边框 ⌥⇧5", "border:none", cx))
                    .child(b("b-du", "sh-b-diag-up", "左下到右上斜线", "border:diag-up", cx)),
            )
            .child(Self::sep(t).mx(px(0.)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .px(px(4.))
                    .child(
                        div()
                            .id("b-color")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(2.))
                            .px(px(4.))
                            .h(px(26.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .child(div().flex().flex_col().items_center().child(icon("sh-pen", 14., t.text)).child(div().w(px(16.)).h(px(3.)).bg(bc)))
                            .child(icon("sh-chevron", 10., t.text_muted))
                            .tooltip(|_, cx| cx.new(|_| Tooltip("线条颜色".into())).into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.border_sub = if this.border_sub == 1 { 0 } else { 1 };
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("b-style")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .px(px(6.))
                            .h(px(26.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .child(line(&self.border_style))
                            .child(icon("sh-chevron", 10., t.text_muted))
                            .tooltip(move |_, cx| cx.new(|_| Tooltip(format!("线型：{style_label}").into())).into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.border_sub = if this.border_sub == 2 { 0 } else { 2 };
                                cx.notify();
                            })),
                    ),
            )
            .when(self.border_sub == 1, |d| d.child(self.color_panel("border", t, cx).border_0().shadow_none().p(px(2.))))
            .when(self.border_sub == 2, |d| {
                d.children(STYLES.iter().map(|(k, name)| {
                    let k = *k;
                    div()
                        .id(SharedString::from(format!("bs-{k}")))
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(10.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.hover))
                        .when(self.border_style == k, |d| d.text_color(t.accent))
                        .child(line(k))
                        .child(*name)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.border_style = k.to_string();
                            this.border_sub = 0;
                            cx.notify();
                        }))
                }))
            })
    }
}
