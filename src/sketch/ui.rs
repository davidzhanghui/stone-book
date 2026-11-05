//! 草图界面：顶部工具栏（含「更多工具」）、左上菜单、右上完成、左侧属性面板（对齐 Excalidraw：图标按钮、
//! 颜色选择器、透明度滑块）、左下缩放栏、右键菜单、提示、浮动输入、Mermaid 对话框。

use super::model::*;
use super::view::*;
use crate::theme::Theme;
use crate::ui::{icon, Tooltip};
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde_json::{json, Value};

fn panel(t: &Theme) -> Div {
    div().occlude().rounded(px(10.)).bg(t.panel_bg).border_1().border_color(t.border).shadow_lg()
}

pub(super) const STROKES: [&str; 5] = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];
pub(super) const BGS: [&str; 5] = [TRANSPARENT, "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];
const CANVAS_BGS: [&str; 5] = ["#ffffff", "#f8f9fa", "#f5faff", "#fffce8", "#fdf8f6"];

/// 官方调色板（`COLOR_PALETTE`）：颜色选择器 3×5 网格，字母为快捷键；单色的五档相同。
pub(super) const PALETTE: [(char, [&str; 5]); 15] = [
    ('q', [TRANSPARENT; 5]),
    ('w', ["#ffffff"; 5]),
    ('e', ["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"]),
    ('r', ["#1e1e1e"; 5]),
    ('t', ["#f8f1ee", "#eaddd7", "#d2bab0", "#a18072", "#846358"]),
    ('a', ["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"]),
    ('s', ["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"]),
    ('d', ["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"]),
    ('f', ["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"]),
    ('g', ["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"]),
    ('z', ["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"]),
    ('x', ["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"]),
    ('c', ["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"]),
    ('v', ["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"]),
    ('b', ["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"]),
];

/// 颜色在调色板里的位置：(第几种颜色, 第几档)；单色返回档位 None
pub(super) fn palette_pos(c: &str) -> Option<(usize, Option<usize>)> {
    PALETTE.iter().enumerate().find_map(|(i, (_, shades))| {
        let single = shades.iter().all(|s| *s == shades[0]);
        shades.iter().position(|s| s.eq_ignore_ascii_case(c)).map(|k| (i, if single { None } else { Some(k) }))
    })
}

pub(super) fn hex_hsla(s: &str) -> Hsla {
    crate::canvas::Col::parse(s).map(|c| c.hsla()).unwrap_or(gpui::transparent_black())
}

/// 透明色的棋盘格底
fn checker(n: f32, t: &Theme) -> Div {
    let cell = n / 4.;
    let (a, b) = if t.dark { (hsla(0., 0., 0.35, 1.), hsla(0., 0., 0.25, 1.)) } else { (hsla(0., 0., 1., 1.), hsla(0., 0., 0.88, 1.)) };
    div()
        .size(px(n))
        .flex()
        .flex_col()
        .children((0..4).map(move |r| div().flex().children((0..4).map(move |c| div().size(px(cell)).bg(if (r + c) % 2 == 0 { a } else { b })))))
}

/// 色块（透明显示棋盘格）
fn color_chip(c: &str, n: f32, t: &Theme) -> Div {
    let d = div().size(px(n)).rounded(px(5.)).overflow_hidden().border_1().border_color(t.border);
    if c == TRANSPARENT || c.is_empty() {
        d.child(checker(n, t))
    } else {
        d.bg(hex_hsla(c))
    }
}

/// 工具栏按钮：(工具, 图标, 提示, 快捷键角标)
const MAIN_TOOLS: [(Tool, &str, &str, &str); 11] = [
    (Tool::Hand, "hand", "抓手 H", ""),
    (Tool::Select, "pointer", "选择 V（再点一次：套索）", "V"),
    (Tool::Rect, "square", "矩形 R", "R"),
    (Tool::Diamond, "diamond", "菱形 D", "D"),
    (Tool::Ellipse, "circle", "椭圆 O", "O"),
    (Tool::Arrow, "arrow-right", "箭头 A", "A"),
    (Tool::Line, "minus", "直线 L", "L"),
    (Tool::Draw, "pencil", "画笔 P", "P"),
    (Tool::Text, "sk-text", "文本 T", "T"),
    (Tool::Sticky, "sk-sticky", "便签 N", "N"),
    (Tool::Eraser, "eraser2", "橡皮 E", "E"),
];

/// 「更多工具」菜单项
const MORE_TOOLS: [(Tool, &str, &str, &str); 6] = [
    (Tool::Image, "image", "插入图像", "9"),
    (Tool::Frame, "sk-hash", "画框工具", "F"),
    (Tool::ShapeDraw, "sk-draw-shape", "画完识别形状", "⇧X"),
    (Tool::Laser, "sk-laser", "激光笔", "K"),
    (Tool::Bucket, "sk-bucket", "油漆桶", "B"),
    (Tool::Lasso, "sk-lasso", "套索选择", ""),
];

impl SketchEditor {
    pub(super) fn render_toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let lock = self.tool_lock;
        let more_on = MORE_TOOLS.iter().any(|(tool, ..)| *tool == self.tool);
        let btn = |id: ElementId, on: bool, ic: &'static str, badge: &'static str| {
            div()
                .id(id)
                .size(px(36.))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .cursor_pointer()
                .when(on, |d| d.bg(t.accent_soft))
                .hover(|s| s.bg(t.hover))
                .child(icon(ic, 17., if on { t.accent } else { t.text }))
                .when(!badge.is_empty(), |d| d.child(div().absolute().right(px(3.)).bottom(px(1.)).text_size(px(9.)).text_color(t.text_muted).child(badge)))
        };
        div().absolute().top(px(12.)).left_0().right_0().flex().justify_center().child(
            panel(t)
                .h(px(46.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(2.))
                .child(
                    btn("sk-lock".into(), lock, if lock { "lock" } else { "unlock" }, "")
                        .tooltip(|_, cx| cx.new(|_| Tooltip("保持当前工具 Q".into())).into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tool_lock = !this.tool_lock;
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.)).h(px(22.)).mx(px(4.)).bg(t.border))
                .children(MAIN_TOOLS.into_iter().enumerate().map(|(i, (tool, ic, tip, badge))| {
                    let on = self.tool == tool || (tool == Tool::Select && self.tool == Tool::Lasso);
                    let ic = if tool == Tool::Select && self.tool == Tool::Lasso { "sk-lasso" } else { ic };
                    btn(("sk-tool", i).into(), on, ic, badge).tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into()).on_click(cx.listener(
                        move |this, _, w, cx| {
                            // 已是选择工具时再点：切到套索
                            let tool = if tool == Tool::Select && this.tool == Tool::Select { Tool::Lasso } else { tool };
                            this.set_tool(tool, w, cx)
                        },
                    ))
                }))
                .child(div().w(px(1.)).h(px(22.)).mx(px(4.)).bg(t.border))
                .child(
                    btn("sk-more".into(), more_on || self.popover == Some(Popover::Tools), "sk-more", "")
                        .tooltip(|_, cx| cx.new(|_| Tooltip("更多工具".into())).into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.popover = if this.popover == Some(Popover::Tools) { None } else { Some(Popover::Tools) };
                            this.menu = None;
                            cx.notify();
                        })),
                ),
        )
    }

    fn render_tools_menu(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let item = |id: ElementId, ic: &'static str, label: &'static str, hint: &'static str, on: bool| {
            div()
                .id(id)
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(on, |d| d.bg(t.accent_soft).text_color(t.accent))
                .hover(|s| s.bg(t.hover))
                .child(icon(ic, 16., if on { t.accent } else { t.text }))
                .child(div().flex_1().child(label))
                .child(div().text_size(px(11.5)).text_color(t.text_muted).child(hint))
        };
        let menu = panel(t)
            .w(px(250.))
            .p(px(6.))
            .flex()
            .flex_col()
            .children(MORE_TOOLS.into_iter().enumerate().map(|(i, (tool, ic, label, hint))| {
                item(("sk-more-item", i).into(), ic, label, hint, self.tool == tool).on_click(cx.listener(move |this, _, w, cx| {
                    this.popover = None;
                    this.set_tool(tool, w, cx)
                }))
            }))
            .child(div().h(px(1.)).my(px(4.)).bg(t.border))
            .child(div().px(px(10.)).py(px(3.)).text_size(px(12.)).text_color(t.text_muted).child("生成"))
            .child(item("sk-more-mermaid".into(), "sk-mermaid", "Mermaid 至草图", "", false).on_click(cx.listener(|this, _, w, cx| this.open_mermaid(w, cx))));
        // 右边缘对齐工具栏右端（工具栏居中、约 540px 宽）
        div().absolute().top(px(64.)).left_0().right_0().flex().justify_center().child(div().w(px(540.)).flex().justify_end().child(menu)).into_any_element()
    }

    pub(super) fn render_topbar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.doc.live().count();
        div()
            .absolute()
            .top(px(12.))
            .left(px(88.))
            .right(px(16.))
            .flex()
            .justify_between()
            .items_center()
            // 左：菜单
            .child(
                panel(t)
                    .h(px(44.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .id("sk-menu")
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .cursor_pointer()
                            .when(self.popover == Some(Popover::Menu), |d| d.bg(t.accent_soft))
                            .hover(|s| s.bg(t.hover))
                            .child(icon("list", 17., t.text))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.popover = if this.popover == Some(Popover::Menu) { None } else { Some(Popover::Menu) };
                                this.menu = None;
                                cx.notify();
                            })),
                    )
                    .child(icon("pen-tool", 15., t.accent))
                    .child(div().text_size(px(13.)).child("草图"))
                    .child(div().pr(px(6.)).text_size(px(12.)).text_color(t.text_muted).child(format!("{n} 个元素"))),
            )
            // 右：导出 + 完成
            .child(
                panel(t)
                    .h(px(44.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("sk-export")
                            .h(px(32.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .cursor_pointer()
                            .text_size(px(13.))
                            .when(self.popover == Some(Popover::Export), |d| d.bg(t.accent_soft))
                            .hover(|s| s.bg(t.hover))
                            .child("导出")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.popover = if this.popover == Some(Popover::Export) { None } else { Some(Popover::Export) };
                                this.menu = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("sk-done")
                            .h(px(30.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .cursor_pointer()
                            .bg(t.accent)
                            .text_color(gpui::white())
                            .text_size(px(13.))
                            .hover(|s| s.opacity(0.9))
                            .child("完成")
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    ),
            )
    }

    pub(super) fn render_popover(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.popover?;
        let item = |id: &'static str, label: &'static str, hint: &'static str| {
            div()
                .id(id)
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .hover(|s| s.bg(t.hover))
                .child(label)
                .child(div().text_size(px(11.5)).text_color(t.text_muted).child(hint))
        };
        let el = match p {
            Popover::Tools => return Some(self.render_tools_menu(t, cx)),
            Popover::Color(target) => return Some(self.render_color_picker(target, t, cx)),
            Popover::Head(start) => return Some(self.render_head_picker(start, t, cx)),
            Popover::Fonts => return Some(self.render_font_picker(t, cx)),
            Popover::Menu => panel(t)
                .absolute()
                .top(px(62.))
                .left(px(88.))
                .w(px(250.))
                .p(px(6.))
                .flex()
                .flex_col()
                .child(item("pm-open", "打开 .excalidraw…", "⌘O").on_click(cx.listener(|this, _, _, cx| this.open_file(cx))))
                .child(item("pm-save", "另存为 .excalidraw…", "⌘S").on_click(cx.listener(|this, _, _, cx| this.save_as(cx))))
                .child(item("pm-view", if self.view_mode { "退出查看模式" } else { "查看模式" }, "⌥R").on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_view_mode(cx);
                })))
                .child(item("pm-grid", if self.grid { "隐藏网格" } else { "显示网格" }, "⌘'").on_click(cx.listener(|this, _, _, cx| {
                    this.grid = !this.grid;
                    this.popover = None;
                    this.commit(cx);
                })))
                .child(item("pm-unlock", "解锁全部", "").on_click(cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    this.unlock_all(cx);
                })))
                .child(item("pm-clear", "清空画布", "").on_click(cx.listener(|this, _, _, cx| this.clear_canvas(cx))))
                .child(div().h(px(1.)).my(px(4.)).bg(t.border))
                .child(div().px(px(10.)).py(px(4.)).text_size(px(12.)).text_color(t.text_muted).child("画布背景"))
                .child(div().px(px(10.)).pb(px(6.)).flex().gap(px(6.)).children(CANVAS_BGS.iter().enumerate().map(|(i, c)| {
                    let on = self.doc.bg_color().eq_ignore_ascii_case(c);
                    let c = *c;
                    div()
                        .id(("pm-bg", i))
                        .size(px(22.))
                        .rounded(px(5.))
                        .bg(hex_hsla(c))
                        .border_1()
                        .border_color(if on { t.accent } else { t.border })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| this.set_bg(c, cx)))
                })))
                .into_any_element(),
            Popover::Export => panel(t)
                .absolute()
                .top(px(62.))
                .right(px(16.))
                .w(px(230.))
                .p(px(6.))
                .flex()
                .flex_col()
                .child(div().px(px(10.)).py(px(4.)).text_size(px(12.)).text_color(t.text_muted).child(if self.sel.is_empty() {
                    "导出全部"
                } else {
                    "导出选中元素"
                }))
                .child(item("pe-png1", "PNG（1x）", "").on_click(cx.listener(|this, _, _, cx| this.export_image(Some(1.), cx))))
                .child(item("pe-png2", "PNG（2x）", "").on_click(cx.listener(|this, _, _, cx| this.export_image(Some(2.), cx))))
                .child(item("pe-png3", "PNG（3x）", "").on_click(cx.listener(|this, _, _, cx| this.export_image(Some(3.), cx))))
                .child(item("pe-svg", "SVG", "").on_click(cx.listener(|this, _, _, cx| this.export_image(None, cx))))
                .child(item("pe-copy", "复制为 PNG", "⇧⌘C").on_click(cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    this.copy_png(cx);
                })))
                .child(div().h(px(1.)).my(px(4.)).bg(t.border))
                .child(item("pe-json", ".excalidraw 文件", "⌘S").on_click(cx.listener(|this, _, _, cx| this.save_as(cx))))
                .into_any_element(),
        };
        Some(el)
    }

    pub(super) fn render_zoombar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let btn = |id: &'static str, ic: &'static str, tip: &'static str, enabled: bool| {
            div()
                .id(id)
                .size(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(t.hover)))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                .child(icon(ic, 15., if enabled { t.text } else { t.text_muted.opacity(0.5) }))
        };
        div()
            .absolute()
            .bottom(px(16.))
            .left(px(16.))
            .flex()
            .gap(px(8.))
            .child(
                panel(t)
                    .h(px(36.))
                    .px(px(3.))
                    .flex()
                    .items_center()
                    .child(btn("sz-out", "zoom-out", "缩小 ⌘-", true).on_click(cx.listener(|this, _, _, cx| this.zoom_step(0.8, cx))))
                    .child(
                        div()
                            .id("sz-pct")
                            .w(px(52.))
                            .text_center()
                            .text_size(px(12.5))
                            .cursor_pointer()
                            .tooltip(|_, cx| cx.new(|_| Tooltip("重置为 100% ⌘0".into())).into())
                            .child(format!("{}%", (self.zoom * 100.).round()))
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_100(cx))),
                    )
                    .child(btn("sz-in", "zoom-in", "放大 ⌘=", true).on_click(cx.listener(|this, _, _, cx| this.zoom_step(1.25, cx))))
                    .child(btn("sz-fit", "fit-page", "缩放至合适 ⇧1", true).on_click(cx.listener(|this, _, _, cx| this.zoom_fit(cx)))),
            )
            .child(
                panel(t)
                    .h(px(36.))
                    .px(px(3.))
                    .flex()
                    .items_center()
                    .when(self.view_mode, |d| d.invisible())
                    .child(btn("sz-undo", "rotate-ccw", "撤销 ⌘Z", !self.undo.is_empty()).on_click(cx.listener(|this, _, _, cx| this.do_undo(cx))))
                    .child(btn("sz-redo", "redo", "重做 ⇧⌘Z", !self.redo.is_empty()).on_click(cx.listener(|this, _, _, cx| this.do_redo(cx)))),
            )
    }

    /// 带链接的元素右上角外侧的小链接图标（官网同款），单击打开链接 / 跳到目标元素
    pub(super) fn render_link_badges(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if matches!(self.drag, Some(Drag::Create { .. }) | Some(Drag::Draw { .. }) | Some(Drag::Linear { .. })) {
            return vec![];
        }
        let size = self.canvas_bounds.size;
        let editing = self.editing.as_ref().map(|e| e.id.clone());
        let mut out = vec![];
        for e in self.doc.live() {
            let Some(link) = e.link().filter(|l| !l.trim().is_empty()) else { continue };
            // 嵌入网页自己就显示网址；正在编辑文字的元素不挡输入框
            if matches!(e.kind.as_str(), "embeddable" | "iframe") || editing.as_deref() == Some(e.id.as_str()) || e.container_id().is_some() {
                continue;
            }
            let corner = if e.points.is_some() {
                let r = super::geom::bounds(e);
                super::geom::v(r.x1, r.y0)
            } else {
                super::geom::corners(e)[1]
            };
            let p = self.to_screen(corner) - self.canvas_bounds.origin;
            let (x, y) = (p.x + px(4.), p.y - px(24.));
            if x < px(-24.) || y < px(-24.) || x > size.width || y > size.height {
                continue;
            }
            let link = link.to_string();
            let internal = super::tools::element_link_id(&link).is_some();
            let tip: SharedString = if internal { "跳到链接的元素".into() } else { link.clone().into() };
            out.push(
                div()
                    .id(SharedString::from(format!("sk-link-{}", e.id)))
                    .occlude()
                    .absolute()
                    .left(x)
                    .top(y)
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .bg(t.panel_bg)
                    .border_1()
                    .border_color(t.border)
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(icon("link", 12., t.accent))
                    .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.clone())).into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.menu = None;
                            this.follow_link(&link, cx);
                        }),
                    )
                    .into_any_element(),
            );
        }
        out
    }

    /// 查看模式时顶部中间的提示条（代替工具栏），带退出按钮
    pub(super) fn render_view_badge(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .top(px(12.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                panel(t)
                    .h(px(44.))
                    .pl(px(14.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon("eye", 15., t.accent))
                    .child(div().text_size(px(13.)).child("查看模式"))
                    .child(
                        div()
                            .id("sk-view-exit")
                            .h(px(32.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .cursor_pointer()
                            .text_size(px(13.))
                            .text_color(t.accent)
                            .hover(|s| s.bg(t.hover))
                            .tooltip(|_, cx| cx.new(|_| Tooltip("退出查看模式 ⌥R".into())).into())
                            .child("退出")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_view_mode(cx))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_toast(&self, t: &Theme) -> Option<AnyElement> {
        let (msg, _) = self.toast.as_ref()?;
        Some(
            div()
                .absolute()
                .bottom(px(70.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(panel(t).px(px(14.)).py(px(8.)).text_size(px(13.)).child(msg.clone()))
                .into_any_element(),
        )
    }

    pub(super) fn render_menu(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let at = self.menu?;
        let o = self.canvas_bounds.origin;
        let has = !self.sel.is_empty();
        let item = |id: &'static str, label: &'static str, hint: &'static str| {
            div()
                .id(id)
                .h(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(18.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .hover(|s| s.bg(t.hover))
                .child(label)
                .child(div().text_size(px(11.5)).text_color(t.text_muted).child(hint))
        };
        let close = |this: &mut SketchEditor| this.menu = None;
        let mut m = panel(t).absolute().left(at.x - o.x).top(at.y - o.y).w(px(220.)).p(px(5.)).flex().flex_col();
        m = m.child(item("cm-paste", "粘贴", "⌘V").on_click(cx.listener(move |this, _, _, cx| {
            close(this);
            this.paste(cx);
        })));
        if has {
            m = m
                .child(item("cm-copy", "复制", "⌘C").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.copy(cx);
                })))
                .child(item("cm-cut", "剪切", "⌘X").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.cut(cx);
                })))
                .child(item("cm-dup", "创建副本", "⌘D").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.duplicate(cx);
                })))
                .child(item("cm-png", "复制为 PNG", "⇧⌘C").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.copy_png(cx);
                })))
                .when(self.link_target_of_selection().is_some(), |m| {
                    m.child(item("cm-elink", "复制对象链接", "").on_click(cx.listener(move |this, _, _, cx| {
                        close(this);
                        this.copy_element_link(cx);
                    })))
                })
                .child(div().h(px(1.)).my(px(3.)).bg(t.border))
                .child(item("cm-group", "成组", "⌘G").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.group(cx);
                })))
                .child(item("cm-ungroup", "取消成组", "⇧⌘G").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.ungroup(cx);
                })))
                .child(item("cm-lock", "锁定 / 解锁", "⇧⌘L").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.toggle_lock(cx);
                })))
                .child(div().h(px(1.)).my(px(3.)).bg(t.border))
                .child(item("cm-front", "置于顶层", "⇧⌘]").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.z_order(2, cx);
                })))
                .child(item("cm-back", "置于底层", "⇧⌘[").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.z_order(-2, cx);
                })))
                .child(div().h(px(1.)).my(px(3.)).bg(t.border))
                .child(item("cm-del", "删除", "⌫").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.delete_sel(cx);
                })));
        } else {
            m = m
                .child(item("cm-all", "全选", "⌘A").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.select_all(cx);
                })))
                .child(item("cm-grid", "网格开关", "⌘'").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.grid = !this.grid;
                    this.commit(cx);
                })))
                .child(item("cm-unlock", "解锁全部", "").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.unlock_all(cx);
                })));
        }
        Some(m.into_any_element())
    }

    // ───────────── 属性面板 ─────────────

    /// 面板里展示哪些元素种类：选区（容器文字也算文字），或当前工具将要画的种类
    fn panel_kinds(&self) -> Option<Vec<String>> {
        let sel = self.selected();
        if sel.is_empty() {
            let k = match self.tool {
                Tool::Bucket => "bucket",
                t => t.kind()?,
            };
            if matches!(k, "image" | "frame" | "embeddable") {
                return None;
            }
            return Some(vec![k.to_string()]);
        }
        let mut k: Vec<String> = sel.iter().map(|e| e.kind.clone()).collect();
        if sel.iter().any(|e| e.bound_text_id().is_some() && !e.is_sticky()) {
            k.push("text".into());
        }
        Some(k)
    }

    pub(super) fn render_props(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.editing.as_ref().is_some_and(|e| e.frame_name) {
            return None;
        }
        let kinds = self.panel_kinds()?;
        let sel = self.selected();
        let has = |k: &str| kinds.iter().any(|x| x == k);
        let all = |f: &dyn Fn(&str) -> bool| kinds.iter().all(|x| f(x));
        let shape = has("rectangle") || has("diamond") || has("ellipse");
        let sticky_only = all(&|k| k == "stickynote");
        let text_only = all(&|k| k == "text");
        let image_only = all(&|k| k == "image");
        let frame_only = all(&|k| matches!(k, "frame" | "magicframe" | "embeddable" | "iframe"));
        let bucket = has("bucket");
        let first = sel.first().copied();
        let cur = &self.cur;
        let stroke = first.map(|e| e.stroke_color.clone()).unwrap_or(if sticky_only { cur.sticky_ink.clone() } else { cur.stroke.clone() });
        let bg = first.map(|e| e.background_color.clone()).unwrap_or(if sticky_only { cur.sticky_bg.clone() } else { cur.bg.clone() });
        let fill = first.map(|e| e.fill_style.clone()).unwrap_or(cur.fill.clone());
        let sw = first.map(|e| e.stroke_width).unwrap_or(cur.stroke_width);
        let ss = first.map(|e| e.stroke_style.clone()).unwrap_or(cur.stroke_style.clone());
        let rough = first.map(|e| e.roughness).unwrap_or(cur.roughness);
        let round = first.map(|e| e.roundness().is_some()).unwrap_or(cur.round);
        let text_el = sel.iter().find(|e| e.is_text()).copied().or_else(|| first.and_then(|e| e.bound_text_id()).and_then(|t| self.doc.get(&t)));
        let fs = text_el.map(|e| e.get("baseFontSize").and_then(|v| v.as_f64()).unwrap_or(e.font_size_or())).unwrap_or(cur.font_size);
        let ff = text_el.map(|e| e.font_family()).unwrap_or(cur.font_family);
        let ta = text_el.map(|e| e.text_align().to_string()).unwrap_or(if sticky_only { "center".into() } else { cur.text_align.clone() });
        let va = text_el.map(|e| e.vertical_align().to_string()).unwrap_or("middle".into());
        let arrow_el = sel.iter().find(|e| e.kind == "arrow").copied();
        let arrow_type = match arrow_el {
            Some(a) if a.elbowed() => "elbow",
            Some(a) if a.roundness().is_none() => "sharp",
            Some(_) => "round",
            None => cur.arrow_type,
        };
        let sa = arrow_el.map(|e| e.arrowhead(true).map(|s| s.to_string())).unwrap_or(cur.start_arrow.clone());
        let ea = arrow_el.map(|e| e.arrowhead(false).map(|s| s.to_string())).unwrap_or(cur.end_arrow.clone());

        let title = |s: &'static str| div().mt(px(8.)).mb(px(5.)).text_size(px(12.5)).text_color(t.text).child(s);
        let btn_bg = if t.dark { hsla(0., 0., 1., 0.06) } else { hsla(240. / 360., 0.1, 0.96, 1.) };
        let ibtn = move |id: SharedString, ic: &'static str, on: bool, tip: &'static str| {
            div()
                .id(id)
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .cursor_pointer()
                .bg(if on { t.accent_soft } else { btn_bg })
                .hover(|s| s.bg(t.hover))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                .child(icon(ic, 17., if on { t.accent } else { t.text }))
        };
        let tbtn = move |id: SharedString, label: &'static str, on: bool, tip: &'static str| {
            div()
                .id(id)
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(14.))
                .bg(if on { t.accent_soft } else { btn_bg })
                .when(on, |d| d.text_color(t.accent))
                .hover(|s| s.bg(t.hover))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                .child(label)
        };
        let row = || div().flex().flex_wrap().items_center().gap(px(6.));

        let mut p = panel(t).absolute().top(px(70.)).left(px(16.)).w(px(232.)).px(px(12.)).pb(px(12.)).pt(px(2.)).flex().flex_col();

        // 描边 / 文字颜色
        if !image_only && !frame_only && !bucket {
            p = p.child(title(if sticky_only || text_only { "文字颜色" } else { "描边" })).child(self.swatch_row(PickTarget::Stroke, &STROKES, &stroke, t, cx));
        }
        // 背景
        let bg_section = shape || sticky_only || has("line") || has("freedraw") || bucket || has("stickynote");
        if bg_section {
            let picks: &[&str; 5] = if sticky_only { &STICKY_BGS } else { &BGS };
            p = p.child(title("背景")).child(self.swatch_row(PickTarget::Bg, picks, &bg, t, cx));
        }
        // 填充
        if bg_section && !sticky_only && !bucket && bg != TRANSPARENT && !has("freedraw") {
            let fills = [("hachure", "sk-fill-hachure", "斜线"), ("cross-hatch", "sk-fill-cross", "交叉线"), ("solid", "sk-fill-solid", "实心")];
            let mut r = row();
            for (k, ic, tip) in fills {
                r = r.child(
                    ibtn(format!("pf-{k}").into(), ic, fill == k, tip)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_prop(move |s| s.fill = k.into(), move |e| e.fill_style = k.into(), cx))),
                );
            }
            if fill == "zigzag" {
                r = r.child(ibtn("pf-zigzag".into(), "sk-fill-zigzag", true, "折线"));
            }
            p = p.child(title("填充")).child(r);
        }
        let strokes_ok = !text_only && !image_only && !sticky_only && !frame_only && !bucket;
        if strokes_ok {
            let widths = [(1., "sk-sw-thin", "细"), (2., "sk-sw-bold", "粗"), (4., "sk-sw-extra", "特粗")];
            p = p.child(title("描边宽度")).child(row().children(widths.into_iter().map(|(w, ic, tip)| {
                ibtn(format!("pw-{w}").into(), ic, (sw - w).abs() < 0.01, tip).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.stroke_width = w,
                        move |e| {
                            if !e.is_text() {
                                e.stroke_width = w
                            }
                        },
                        cx,
                    )
                }))
            })));
            if !all(&|k| k == "freedraw") {
                let styles = [("solid", "sk-ss-solid", "实线"), ("dashed", "sk-ss-dashed", "虚线"), ("dotted", "sk-ss-dotted", "点线")];
                p = p.child(title("边框样式")).child(row().children(styles.into_iter().map(|(k, ic, tip)| {
                    ibtn(format!("pss-{k}").into(), ic, ss == k, tip).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_prop(
                            move |s| s.stroke_style = k.into(),
                            move |e| {
                                if !e.is_text() {
                                    e.stroke_style = k.into()
                                }
                            },
                            cx,
                        )
                    }))
                })));
            }
        }
        if (strokes_ok || sticky_only) && !all(&|k| k == "freedraw") {
            let roughs = [(0., "sk-rough-0", "朴素"), (1., "sk-rough-1", "手绘"), (2., "sk-rough-2", "潦草")];
            p = p.child(title("线条风格")).child(row().children(roughs.into_iter().map(|(r, ic, tip)| {
                ibtn(format!("pr-{r}").into(), ic, (rough - r).abs() < 0.01, tip).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.roughness = r,
                        move |e| {
                            if !e.is_text() {
                                e.roughness = r
                            }
                        },
                        cx,
                    )
                }))
            })));
        }
        // 边角：矩形 / 菱形 / 直线 / 便签 / 图片 / 嵌入网页
        if (has("rectangle") || has("diamond") || has("line") || has("stickynote") || has("image") || has("embeddable"))
            && !has("ellipse")
            && !has("arrow")
            && !bucket
        {
            let opts = [(false, "sk-edge-sharp", "直角"), (true, "sk-edge-round", "圆角")];
            p = p.child(title("边角")).child(row().children(opts.into_iter().map(|(r, ic, tip)| {
                ibtn(format!("prd-{r}").into(), ic, round == r, tip).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.round = r,
                        move |e| match e.kind.as_str() {
                            "rectangle" | "stickynote" | "image" | "embeddable" => e.set("roundness", if r { json!({ "type": 3 }) } else { Value::Null }),
                            "diamond" | "line" => e.set("roundness", if r { json!({ "type": 2 }) } else { Value::Null }),
                            _ => {}
                        },
                        cx,
                    )
                }))
            })));
        }
        // 箭头类型 + 端点
        if has("arrow") {
            let types = [("sharp", "sk-arrow-sharp", "直线箭头"), ("round", "sk-arrow-curved", "曲线箭头"), ("elbow", "sk-arrow-elbow", "折线箭头")];
            p = p.child(title("箭头类型")).child(row().children(types.into_iter().map(|(k, ic, tip)| {
                ibtn(format!("pat-{k}").into(), ic, arrow_type == k, tip).on_click(cx.listener(move |this, _, _, cx| this.set_arrow_type(k, cx)))
            })));
            let head_icon = |h: &Option<String>| -> &'static str {
                match h.as_deref() {
                    None => "sk-head-none",
                    Some("arrow") => "sk-head-arrow",
                    Some("bar") => "sk-head-bar",
                    Some("dot") | Some("circle") => "sk-head-circle",
                    Some("circle_outline") => "sk-head-circle_outline",
                    Some("triangle") => "sk-head-triangle",
                    Some("triangle_outline") => "sk-head-triangle_outline",
                    Some("diamond") => "sk-head-diamond",
                    Some("diamond_outline") => "sk-head-diamond_outline",
                    Some(_) => "sk-head-arrow",
                }
            };
            let (si, ei) = (head_icon(&sa), head_icon(&ea));
            p = p.child(title("端点")).child(
                row()
                    .child(
                        div()
                            .id("ph-start")
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .bg(if self.popover == Some(Popover::Head(true)) { t.accent_soft } else { btn_bg })
                            .hover(|s| s.bg(t.hover))
                            .when(sa.is_none(), |d| d.opacity(0.55))
                            .tooltip(|_, cx| cx.new(|_| Tooltip("起点".into())).into())
                            .child(icon(si, 17., t.text).with_transformation(Transformation::rotate(radians(std::f32::consts::PI))))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.popover = if this.popover == Some(Popover::Head(true)) { None } else { Some(Popover::Head(true)) };
                                cx.notify();
                            })),
                    )
                    .child(ibtn("ph-end".into(), ei, self.popover == Some(Popover::Head(false)), "终点").on_click(cx.listener(|this, _, _, cx| {
                        this.popover = if this.popover == Some(Popover::Head(false)) { None } else { Some(Popover::Head(false)) };
                        cx.notify();
                    }))),
            );
        }
        // 字体 / 字号 / 对齐
        if has("text") || has("stickynote") {
            let group = |f: i64| match f {
                2 | 6 | 7 | 9 => FONT_NORMAL,
                3 | 8 => FONT_CODE,
                _ => FONT_HAND,
            };
            let fams = [(FONT_HAND, "sk-font-hand", "手写体"), (FONT_NORMAL, "sk-font-normal", "常规"), (FONT_CODE, "sk-font-code", "代码")];
            let mut r = row();
            for (f, ic, tip) in fams {
                r = r.child(
                    ibtn(format!("pff-{f}").into(), ic, group(ff) == f && !matches!(ff, 1 | 2 | 3 | 7 | 9), tip)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_font_family(f, cx))),
                );
            }
            r = r.child(div().w(px(1.)).h(px(20.)).bg(t.border)).child(
                ibtn("pff-more".into(), "sk-font-more", matches!(ff, 1 | 2 | 3 | 7 | 9) || self.popover == Some(Popover::Fonts), "更多字体").on_click(
                    cx.listener(|this, _, _, cx| {
                        this.popover = if this.popover == Some(Popover::Fonts) { None } else { Some(Popover::Fonts) };
                        cx.notify();
                    }),
                ),
            );
            p = p.child(title("字体")).child(r);
            let sizes = [(16., "S", "小"), (20., "M", "中"), (28., "L", "大"), (36., "XL", "特大")];
            p = p.child(title("字体大小")).child(row().children(sizes.into_iter().map(|(f, l, tip)| {
                tbtn(format!("pfs-{f}").into(), l, (fs - f).abs() < 0.01, tip).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.font_size = f,
                        move |e| {
                            if e.is_text() {
                                e.font_size = Some(f);
                                if e.get("baseFontSize").is_some() {
                                    e.set("baseFontSize", json!(f));
                                }
                            }
                        },
                        cx,
                    )
                }))
            })));
            let aligns = [("left", "text-left", "左对齐"), ("center", "text-center", "居中"), ("right", "text-right", "右对齐")];
            p = p.child(title("文本对齐")).child(row().children(aligns.into_iter().map(|(a, ic, tip)| {
                ibtn(format!("pta-{a}").into(), ic, ta == a, tip).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.text_align = a.into(),
                        move |e| {
                            if e.is_text() {
                                e.set("textAlign", json!(a))
                            }
                        },
                        cx,
                    )
                }))
            })));
            // 容器 / 便签里的文字：竖直对齐
            let in_container = has("stickynote") || sel.iter().any(|e| e.is_container() && e.bound_text_id().is_some());
            if in_container {
                let vas = [("top", "sk-valign-top", "顶端对齐"), ("middle", "sk-valign-mid", "垂直居中"), ("bottom", "sk-valign-bottom", "底端对齐")];
                p = p.child(row().mt(px(6.)).children(vas.into_iter().map(|(a, ic, tip)| {
                    ibtn(format!("pva-{a}").into(), ic, va == a, tip).on_click(cx.listener(move |this, _, _, cx| {
                        this.set_prop(
                            |_| {},
                            move |e| {
                                if e.is_text() {
                                    e.set("verticalAlign", json!(a))
                                }
                            },
                            cx,
                        )
                    }))
                })));
            }
        }
        // 透明度
        if !frame_only && !bucket {
            p = p.child(title("透明度")).child(self.render_opacity(t, cx));
        }
        if !sel.is_empty() {
            let layers = [
                (-2, "sk-send-back", "置于底层 ⇧⌘["),
                (-1, "sk-send-backward", "下移一层 ⌘["),
                (1, "sk-bring-forward", "上移一层 ⌘]"),
                (2, "sk-bring-front", "置于顶层 ⇧⌘]"),
            ];
            p = p.child(title("图层")).child(
                row().children(
                    layers
                        .into_iter()
                        .map(|(d, ic, tip)| ibtn(format!("pl-{d}").into(), ic, false, tip).on_click(cx.listener(move |this, _, _, cx| this.z_order(d, cx)))),
                ),
            );
            if sel.len() >= 2 {
                let al = [
                    (0u8, "align-h-left", "左对齐"),
                    (1, "align-h-mid", "水平居中"),
                    (2, "align-h-right", "右对齐"),
                    (3, "align-v-top", "顶端对齐"),
                    (4, "align-v-mid", "垂直居中"),
                    (5, "align-v-bottom", "底端对齐"),
                ];
                p = p.child(title("对齐")).child(
                    row()
                        .children(
                            al.into_iter().map(|(k, ic, tip)| {
                                ibtn(format!("pal-{k}").into(), ic, false, tip).on_click(cx.listener(move |this, _, _, cx| this.align(k, cx)))
                            }),
                        )
                        .when(sel.len() >= 3, |r| {
                            r.child(ibtn("pd-h".into(), "columns", false, "水平等距分布").on_click(cx.listener(|this, _, _, cx| this.distribute(true, cx))))
                                .child(ibtn("pd-v".into(), "rows", false, "垂直等距分布").on_click(cx.listener(|this, _, _, cx| this.distribute(false, cx))))
                        }),
                );
            }
            let grouped = sel.iter().any(|e| !e.group_ids.is_empty());
            let single = self.single();
            let linear = single.is_some_and(|e| e.is_linear() && !e.elbowed());
            let image = single.is_some_and(|e| e.kind == "image");
            let editing_line = single.is_some_and(|e| self.line_edit.as_deref() == Some(e.id.as_str()));
            let cropping = single.is_some_and(|e| self.crop_edit.as_deref() == Some(e.id.as_str()));
            p = p.child(title("操作")).child(
                row()
                    .child(ibtn("pop-dup".into(), "copy", false, "创建副本 ⌘D").on_click(cx.listener(|this, _, _, cx| this.duplicate(cx))))
                    .child(ibtn("pop-del".into(), "trash", false, "删除 ⌫").on_click(cx.listener(|this, _, _, cx| this.delete_sel(cx))))
                    .child(ibtn("pop-link".into(), "link", false, "链接").on_click(cx.listener(|this, _, w, cx| {
                        let ids: Vec<String> = this.sel.iter().cloned().collect();
                        let cur = this.single().and_then(|e| e.link().map(|s| s.to_string())).unwrap_or_default();
                        this.open_prompt(PromptKind::Link(ids), &cur, w, cx);
                    })))
                    .when(linear, |r| {
                        r.child(
                            ibtn("pop-line".into(), "sk-edit-line", editing_line, "编辑线条节点 ⌘↵")
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_line_edit(cx))),
                        )
                    })
                    .when(image, |r| r.child(ibtn("pop-crop".into(), "sk-crop", cropping, "裁剪").on_click(cx.listener(|this, _, _, cx| this.toggle_crop(cx)))))
                    .when(sel.len() >= 2, |r| r.child(ibtn("pop-grp".into(), "group", false, "成组 ⌘G").on_click(cx.listener(|this, _, _, cx| this.group(cx)))))
                    .when(grouped, |r| {
                        r.child(ibtn("pop-ungrp".into(), "layers", false, "取消成组 ⇧⌘G").on_click(cx.listener(|this, _, _, cx| this.ungroup(cx))))
                    }),
            );
        }
        Some(p.id("sk-props").max_h(px(700.)).overflow_y_scroll().into_any_element())
    }

    /// 一行：5 个常用色 + 分隔 + 当前色（点开颜色选择器）
    fn swatch_row(&self, target: PickTarget, picks: &[&str; 5], cur: &str, t: &Theme, cx: &mut Context<Self>) -> Div {
        let tag = if target == PickTarget::Stroke { "s" } else { "b" };
        let open = self.popover == Some(Popover::Color(target));
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .children(picks.iter().enumerate().map(|(i, c)| {
                let on = cur.eq_ignore_ascii_case(c);
                let c = c.to_string();
                div()
                    .id(SharedString::from(format!("psw-{tag}-{i}")))
                    .p(px(1.))
                    .rounded(px(7.))
                    .border_1()
                    .border_color(if on { t.accent } else { transparent_black() })
                    .cursor_pointer()
                    .child(color_chip(&c, 22., t))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_color(target, &c, cx)))
            }))
            .child(div().w(px(1.)).h(px(20.)).mx(px(2.)).bg(t.border))
            .child(
                div()
                    .id(SharedString::from(format!("psw-{tag}-cur")))
                    .p(px(1.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if open { t.accent } else { transparent_black() })
                    .cursor_pointer()
                    .tooltip(|_, cx| cx.new(|_| Tooltip("更多颜色".into())).into())
                    .child(color_chip(cur, 28., t))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.popover = if this.popover == Some(Popover::Color(target)) { None } else { Some(Popover::Color(target)) };
                        this.shade = None;
                        this.prompt = None;
                        cx.notify();
                    })),
            )
    }

    /// 透明度滑块（0–100）
    fn render_opacity(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let op = self.selected().first().map(|e| e.opacity).unwrap_or(self.cur.opacity) as f32;
        let ent = cx.entity();
        let ent2 = cx.entity();
        let (accent, track, thumb) = (t.accent, t.accent_soft, if t.dark { hsla(0., 0., 0.85, 1.) } else { hsla(0., 0., 0.22, 1.) });
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("sk-opacity")
                    .h(px(24.))
                    .w_full()
                    .cursor_pointer()
                    .child(
                        canvas(
                            move |b, _, cx| {
                                ent.update(cx, |this, _| this.slider_bounds = b);
                            },
                            move |b, _, window, _| {
                                let w = f32::from(b.size.width);
                                let cy = b.origin.y + b.size.height / 2.;
                                let x = b.origin.x + px(w * op / 100.);
                                window.paint_quad(fill(Bounds::new(point(b.origin.x, cy - px(2.5)), size(b.size.width, px(5.))), track).corner_radii(px(2.5)));
                                window.paint_quad(
                                    fill(Bounds::new(point(b.origin.x, cy - px(2.5)), size(x - b.origin.x, px(5.))), accent.opacity(0.55))
                                        .corner_radii(px(2.5)),
                                );
                                window.paint_quad(fill(Bounds::new(point(x - px(9.), cy - px(9.)), size(px(18.), px(18.))), thumb).corner_radii(px(9.)));
                                // 拖动中：全局跟踪鼠标（离开滑块也继续）
                                let e1 = ent2.clone();
                                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        e1.update(cx, |this, cx| {
                                            if matches!(this.drag, Some(Drag::Slider)) {
                                                this.opacity_at(ev.position.x, cx);
                                            }
                                        });
                                    }
                                });
                                let e2 = ent2.clone();
                                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        e2.update(cx, |this, cx| {
                                            if matches!(this.drag, Some(Drag::Slider)) {
                                                this.drag = None;
                                                this.commit(cx);
                                            }
                                        });
                                    }
                                });
                            },
                        )
                        .size_full(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                            if !this.sel.is_empty() {
                                this.snapshot();
                            }
                            this.drag = Some(Drag::Slider);
                            this.opacity_at(ev.position.x, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(12.))
                    .text_color(t.text_secondary)
                    .child("0")
                    .when(op > 0.5 && op < 99.5, |d| d.child(div().text_color(t.accent).child(format!("{}", op.round()))))
                    .child("100"),
            )
    }

    /// 按滑块上的横坐标设透明度（拖动中不入撤销栈，松手时写回）
    pub(super) fn opacity_at(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let b = self.slider_bounds;
        let w = f32::from(b.size.width).max(1.);
        let o = ((f32::from(x - b.origin.x) / w).clamp(0., 1.) * 100.).round() as f64;
        self.cur.opacity = o;
        let ids = self.sel_with_bound();
        for e in self.doc.elements.iter_mut().filter(|e| ids.contains(&e.id) && !e.locked) {
            e.opacity = o;
            e.bump();
        }
        cx.notify();
    }

    fn set_font_family(&mut self, f: i64, cx: &mut Context<Self>) {
        self.popover = None;
        self.set_prop(
            move |s| s.font_family = f,
            move |e| {
                if e.is_text() {
                    e.set("fontFamily", json!(f))
                }
            },
            cx,
        )
    }

    fn set_arrow_type(&mut self, k: &'static str, cx: &mut Context<Self>) {
        self.cur.arrow_type = k;
        let ids: Vec<String> = self.selected().iter().filter(|e| e.kind == "arrow" && !e.locked).map(|e| e.id.clone()).collect();
        if ids.is_empty() {
            cx.notify();
            return;
        }
        self.snapshot();
        for id in &ids {
            let Some(i) = self.doc.index(id) else { continue };
            let was_elbow = self.doc.elements[i].elbowed();
            {
                let e = &mut self.doc.elements[i];
                match k {
                    "elbow" => {
                        e.set("elbowed", json!(true));
                        e.set("roundness", Value::Null);
                    }
                    "sharp" => {
                        e.set("elbowed", json!(false));
                        e.set("roundness", Value::Null);
                    }
                    _ => {
                        e.set("elbowed", json!(false));
                        e.set("roundness", json!({ "type": 2 }));
                    }
                }
                e.bump();
            }
            if k == "elbow" {
                super::geom::reroute_elbow(&mut self.doc, i);
            } else if was_elbow {
                // 折线改回普通箭头：只留两端
                let pts = super::geom::world_points(&self.doc.elements[i]);
                let e = &mut self.doc.elements[i];
                let (a, b) = (pts[0], pts[pts.len() - 1]);
                e.x = a.x;
                e.y = a.y;
                e.points = Some(vec![[0., 0.], [b.x - a.x, b.y - a.y]]);
                e.normalize_points();
                e.set("fixedSegments", Value::Null);
            }
            super::geom::layout_bound_text(&mut self.doc, id);
        }
        super::geom::update_bound_arrows(&mut self.doc, &[]);
        self.commit(cx);
    }

    pub(super) fn toggle_line_edit(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.single().filter(|e| e.is_linear() && !e.elbowed()).cloned() else { return };
        self.line_edit = if self.line_edit.as_deref() == Some(e.id.as_str()) { None } else { Some(e.id.clone()) };
        self.line_pt = None;
        cx.notify();
    }

    // ───────────── 颜色选择器 ─────────────

    /// 吸管光标：吸管图标 + 右下方的实时预览色圈（官网样式）
    pub(super) fn render_eyedropper(&self, t: &Theme) -> Option<AnyElement> {
        self.eyedropper?;
        if self.mermaid.is_some() || self.menu.is_some() {
            return None;
        }
        let p = self.to_screen(self.mouse_world) - self.canvas_bounds.origin;
        if p.x < px(0.) || p.y < px(0.) || p.x > self.canvas_bounds.size.width || p.y > self.canvas_bounds.size.height {
            return None;
        }
        let c = self.color_under(self.mouse_world);
        Some(
            div()
                .absolute()
                // 图标笔尖（视图框 24 里的 (4,20)）对准光标
                .left(p.x - px(3.))
                .top(p.y - px(15.))
                .child(icon("sk-eyedropper", 18., t.text))
                .child(
                    div()
                        .absolute()
                        .left(px(8.))
                        .top(px(24.))
                        .rounded_full()
                        .shadow_md()
                        .border_2()
                        .border_color(hsla(0., 0., 1., 1.))
                        .child(color_chip(&c, 30., t).rounded_full()),
                )
                .into_any_element(),
        )
    }

    fn render_color_picker(&self, target: PickTarget, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let sel = self.selected();
        let sticky = if sel.is_empty() { self.tool == Tool::Sticky } else { sel.iter().all(|e| e.is_sticky()) };
        let cur = match (sel.first(), target) {
            (Some(e), PickTarget::Stroke) => e.stroke_color.clone(),
            (Some(e), PickTarget::Bg) => e.background_color.clone(),
            (None, PickTarget::Stroke) => {
                if sticky {
                    self.cur.sticky_ink.clone()
                } else {
                    self.cur.stroke.clone()
                }
            }
            (None, PickTarget::Bg) => {
                if sticky {
                    self.cur.sticky_bg.clone()
                } else {
                    self.cur.bg.clone()
                }
            }
        };
        let pos = palette_pos(&cur);
        let shade = self.shade.or(pos.and_then(|(_, s)| s)).unwrap_or(if target == PickTarget::Stroke { 4 } else { 1 });
        let cell = |id: SharedString, c: String, key: String, on: bool| {
            let dark_text = crate::canvas::Col::parse(&c).map(|k| k.hsla().l > 0.6).unwrap_or(true);
            div()
                .id(id)
                .size(px(34.))
                .relative()
                .rounded(px(7.))
                .border_1()
                .border_color(if on { t.accent } else { t.border })
                .when(on, |d| d.border_2())
                .cursor_pointer()
                .overflow_hidden()
                .child(color_chip(&c, 34., t).absolute().top_0().left_0().border_0())
                .child(
                    div()
                        .absolute()
                        .right(px(4.))
                        .bottom(px(1.))
                        .text_size(px(10.5))
                        .font_family("Menlo")
                        .text_color(if dark_text { hsla(0., 0., 0.15, 1.) } else { hsla(0., 0., 1., 1.) })
                        .child(key),
                )
        };
        let grid = div().flex().flex_col().gap(px(5.)).children((0..3).map(|r| {
            div().flex().gap(px(5.)).children((0..5).map(|k| {
                let i = r * 5 + k;
                let (key, shades) = PALETTE[i];
                let single = shades.iter().all(|s| *s == shades[0]);
                let c = if single { shades[0] } else { shades[shade] }.to_string();
                let on = pos.is_some_and(|(pi, _)| pi == i);
                let c2 = c.clone();
                cell(format!("cp-{i}").into(), c, key.to_string(), on).on_click(cx.listener(move |this, _, _, cx| this.set_color(target, &c2, cx)))
            }))
        }));
        let shades_row = match pos {
            Some((pi, Some(si))) => div().flex().gap(px(5.)).children(PALETTE[pi].1.iter().enumerate().map(|(k, c)| {
                let c2 = c.to_string();
                cell(format!("cs-{k}").into(), c.to_string(), format!("⇧{}", k + 1), k == si).on_click(cx.listener(move |this, _, _, cx| {
                    this.shade = Some(k);
                    this.set_color(target, &c2, cx)
                }))
            })),
            _ => div().text_size(px(12.)).text_color(t.text_muted).child("当前颜色没有色调"),
        };
        let hex_editing = matches!(self.prompt.as_ref().map(|p| &p.kind), Some(PromptKind::Hex(k)) if *k == target);
        let hex = cur.trim_start_matches('#').to_string();
        let hex_row = div()
            .id("cp-hex")
            .h(px(34.))
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .rounded(px(8.))
            .border_1()
            .border_color(if hex_editing { t.accent } else { t.border })
            .child(div().text_color(t.text_muted).child("#"))
            .child(match self.prompt.as_ref().filter(|_| hex_editing) {
                Some(p) => div().flex_1().text_size(px(13.)).child(p.input.clone()).into_any_element(),
                None => div()
                    .id("cp-hex-text")
                    .flex_1()
                    .text_size(px(13.))
                    .cursor_text()
                    .child(if cur == TRANSPARENT { "transparent".to_string() } else { hex.clone() })
                    .on_click(cx.listener(move |this, _, w, cx| {
                        let init = if hex == TRANSPARENT { String::new() } else { hex.clone() };
                        this.open_prompt(PromptKind::Hex(target), &init, w, cx);
                        // 保持颜色选择器打开
                        this.popover = Some(Popover::Color(target));
                    }))
                    .into_any_element(),
            })
            .child(div().w(px(1.)).h(px(18.)).bg(t.border))
            .child(
                div()
                    .id("cp-eye")
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(self.eyedropper == Some(target), |d| d.bg(t.accent_soft))
                    .hover(|s| s.bg(t.hover))
                    .tooltip(|_, cx| cx.new(|_| Tooltip("吸管：从画布取色（Esc 退出）".into())).into())
                    .child(icon("sk-eyedropper", 15., if self.eyedropper == Some(target) { t.accent } else { t.text }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.eyedropper = if this.eyedropper == Some(target) { None } else { Some(target) };
                        this.prompt = None;
                        cx.notify();
                    })),
            );
        let top = if target == PickTarget::Stroke { 70. } else { 128. };
        panel(t)
            .absolute()
            .top(px(top))
            .left(px(256.))
            .w(px(214.))
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().text_size(px(13.)).child("颜色"))
            .child(grid)
            .child(div().mt(px(8.)).text_size(px(13.)).child("色调明暗"))
            .child(shades_row)
            .child(div().mt(px(8.)).text_size(px(13.)).child("十六进制值"))
            .child(hex_row)
            .into_any_element()
    }

    /// 颜色选择器打开时的键盘：字母选色、⇧1–5 选色调、Esc 关闭
    pub(super) fn picker_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(Popover::Color(target)) = self.popover else { return false };
        if self.prompt.is_some() {
            return false;
        }
        let k = ev.keystroke.key.as_str();
        if k == "escape" && self.eyedropper.is_some() {
            // 先退出吸管，选择器留着
            self.eyedropper = None;
            cx.notify();
            return true;
        }
        if k == "escape" {
            self.popover = None;
            cx.notify();
            return true;
        }
        let cur = match (self.selected().first(), target) {
            (Some(e), PickTarget::Stroke) => e.stroke_color.clone(),
            (Some(e), PickTarget::Bg) => e.background_color.clone(),
            (None, PickTarget::Stroke) => self.cur.stroke.clone(),
            (None, PickTarget::Bg) => self.cur.bg.clone(),
        };
        let pos = palette_pos(&cur);
        let digit = match k {
            "1" | "!" => Some(0),
            "2" | "@" => Some(1),
            "3" | "#" => Some(2),
            "4" | "$" => Some(3),
            "5" | "%" => Some(4),
            _ => None,
        };
        if let (Some(d), Some((pi, Some(_)))) = (digit.filter(|_| ev.keystroke.modifiers.shift), pos) {
            self.shade = Some(d);
            let c = PALETTE[pi].1[d];
            self.set_color(target, c, cx);
            return true;
        }
        if ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control {
            return false;
        }
        if let Some((_, shades)) = PALETTE.iter().find(|(key, _)| k.len() == 1 && k.starts_with(*key)) {
            let shade = self.shade.or(pos.and_then(|(_, s)| s)).unwrap_or(if target == PickTarget::Stroke { 4 } else { 1 });
            let single = shades.iter().all(|s| *s == shades[0]);
            let c = if single { shades[0] } else { shades[shade] };
            self.set_color(target, c, cx);
            return true;
        }
        false
    }

    fn render_head_picker(&self, start: bool, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let heads: [(Option<&'static str>, &'static str, &'static str); 9] = [
            (None, "sk-head-none", "无"),
            (Some("arrow"), "sk-head-arrow", "箭头"),
            (Some("bar"), "sk-head-bar", "竖线"),
            (Some("circle"), "sk-head-circle", "圆点"),
            (Some("circle_outline"), "sk-head-circle_outline", "空心圆"),
            (Some("triangle"), "sk-head-triangle", "三角"),
            (Some("triangle_outline"), "sk-head-triangle_outline", "空心三角"),
            (Some("diamond"), "sk-head-diamond", "菱形"),
            (Some("diamond_outline"), "sk-head-diamond_outline", "空心菱形"),
        ];
        let arrow = self.selected().into_iter().find(|e| e.kind == "arrow").cloned();
        let cur = match &arrow {
            Some(a) => a.arrowhead(start).map(|s| if s == "dot" { "circle".to_string() } else { s.to_string() }),
            None => {
                if start {
                    self.cur.start_arrow.clone()
                } else {
                    self.cur.end_arrow.clone()
                }
            }
        };
        let btn_bg = if t.dark { hsla(0., 0., 1., 0.06) } else { hsla(240. / 360., 0.1, 0.96, 1.) };
        panel(t)
            .absolute()
            .top(px(300.))
            .left(px(256.))
            .w(px(200.))
            .p(px(10.))
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(heads.into_iter().map(|(k, ic, tip)| {
                let on = cur.as_deref() == k;
                let mut ic_el = icon(ic, 17., if on { t.accent } else { t.text });
                if start {
                    ic_el = ic_el.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)));
                }
                div()
                    .id(SharedString::from(format!("hp-{}", k.unwrap_or("none"))))
                    .size(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .bg(if on { t.accent_soft } else { btn_bg })
                    .hover(|s| s.bg(t.hover))
                    .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                    .child(ic_el)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.popover = None;
                        this.set_prop(
                            move |s| {
                                if start {
                                    s.start_arrow = k.map(|x| x.to_string())
                                } else {
                                    s.end_arrow = k.map(|x| x.to_string())
                                }
                            },
                            move |e| {
                                if e.kind == "arrow" {
                                    e.set(if start { "startArrowhead" } else { "endArrowhead" }, k.map(|x| json!(x)).unwrap_or(Value::Null))
                                }
                            },
                            cx,
                        )
                    }))
            }))
            .into_any_element()
    }

    fn render_font_picker(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let fonts: [(i64, &'static str, &'static str); 8] = [
            (5, "Excalifont", "手写（默认）"),
            (1, "Virgil", "手写（旧版）"),
            (6, "Nunito", "常规（默认）"),
            (2, "Helvetica", "常规（旧版）"),
            (7, "Lilita One", "标题"),
            (9, "Liberation Sans", "常规"),
            (8, "Comic Shanns", "代码（默认）"),
            (3, "Cascadia", "代码（旧版）"),
        ];
        let cur = self.selected().iter().find(|e| e.is_text()).map(|e| e.font_family()).unwrap_or(self.cur.font_family);
        panel(t)
            .absolute()
            .top(px(300.))
            .left(px(256.))
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .children(fonts.into_iter().map(|(f, name, hint)| {
                let (screen, _) = super::render::font_family(f);
                div()
                    .id(("fp", f as usize))
                    .h(px(34.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(cur == f, |d| d.bg(t.accent_soft).text_color(t.accent))
                    .hover(|s| s.bg(t.hover))
                    .child(div().font_family(screen).text_size(px(15.)).child(name))
                    .child(div().text_size(px(11.5)).text_color(t.text_muted).child(hint))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_font_family(f, cx)))
            }))
            .into_any_element()
    }

    // ───────────── 浮动输入 / Mermaid ─────────────

    pub(super) fn render_prompt(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.prompt.as_ref()?;
        let title = match &p.kind {
            PromptKind::Link(_) => "链接",
            PromptKind::Hex(_) => return None,
        };
        let input = p.input.clone();
        Some(
            div()
                .absolute()
                .top(px(70.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    panel(t)
                        .w(px(440.))
                        .p(px(12.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(div().text_size(px(13.)).child(title))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_1()
                                        .h(px(32.))
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(t.accent)
                                        .text_size(px(13.))
                                        .child(input),
                                )
                                .child(
                                    div()
                                        .id("prompt-ok")
                                        .h(px(32.))
                                        .px(px(14.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.))
                                        .bg(t.accent)
                                        .text_color(white())
                                        .text_size(px(13.))
                                        .cursor_pointer()
                                        .child("确定")
                                        .on_click(cx.listener(|this, _, w, cx| {
                                            if let Some(p) = this.prompt.take() {
                                                let text = p.input.read(cx).text();
                                                this.apply_prompt(&p.kind, text.trim(), cx);
                                            }
                                            w.focus(&this.focus);
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(div().text_size(px(11.5)).text_color(t.text_muted).child("↵ 确定 · Esc 取消")),
                )
                .into_any_element(),
        )
    }

    fn render_mermaid_preview(&self, dlg: &MermaidDlg, t: &Theme) -> AnyElement {
        let dark = t.dark;
        let bg = super::paint::bg_hsla(&self.doc, dark);
        let frame = div().h(px(380.)).rounded(px(8.)).border_1().border_color(t.border).bg(bg).overflow_hidden().flex().items_center().justify_center();
        match &dlg.preview {
            Some(MermaidPreview::Doc(d)) => {
                let d = d.clone();
                frame
                    .p(px(12.))
                    .child(canvas(|_, _, _| (), move |b, _, window, cx| super::paint::paint_fit(&d, b, dark, window, cx)).size_full())
                    .into_any_element()
            }
            Some(MermaidPreview::Image(p)) => {
                frame.p(px(12.)).child(img(p.clone()).max_w_full().max_h_full().object_fit(ObjectFit::Contain)).into_any_element()
            }
            Some(MermaidPreview::Error(e)) if !e.is_empty() => {
                frame.p(px(16.)).child(div().text_size(px(12.5)).text_color(hsla(0., 0.7, 0.5, 1.)).child(e.clone())).into_any_element()
            }
            _ => frame.child(div().text_size(px(12.5)).text_color(t.text_muted).child("输入 Mermaid 代码后在这里预览")).into_any_element(),
        }
    }

    pub(super) fn render_mermaid(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dlg = self.mermaid.as_ref()?;
        let btn = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .h(px(32.))
                .px(px(16.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(primary, |d| d.bg(t.accent).text_color(white()))
                .when(!primary, |d| d.border_1().border_color(t.border).hover(|s| s.bg(t.hover)))
                .child(label)
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(t.overlay)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    panel(t)
                        .w(px(980.))
                        .max_w(relative(0.94))
                        .p(px(18.))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(div().text_size(px(16.)).child("Mermaid 至草图"))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(t.text_muted)
                                .child("流程图（flowchart / graph）会转换成可编辑的形状和箭头；时序图、类图等其他图表作为图片插入。"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.))
                                        .child(div().text_size(px(12.5)).text_color(t.text_muted).child("Mermaid 语法"))
                                        .child(
                                            div()
                                                .id("mm-src")
                                                .h(px(380.))
                                                .p(px(10.))
                                                .rounded(px(8.))
                                                .border_1()
                                                .border_color(t.border)
                                                .bg(t.code_bg)
                                                .font_family("Menlo")
                                                .overflow_y_scroll()
                                                .child(dlg.area.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.))
                                        .child(div().text_size(px(12.5)).text_color(t.text_muted).child("预览"))
                                        .child(self.render_mermaid_preview(dlg, t)),
                                ),
                        )
                        .when_some(dlg.error.clone(), |d, e| d.child(div().text_size(px(12.5)).text_color(hsla(0., 0.7, 0.5, 1.)).child(e)))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(btn("mm-cancel", "取消", false).on_click(cx.listener(|this, _, w, cx| {
                                    this.mermaid = None;
                                    w.focus(&this.focus);
                                    cx.notify();
                                })))
                                .child(btn("mm-ok", "插入", true).on_click(cx.listener(|this, _, w, cx| this.insert_mermaid(w, cx)))),
                        ),
                )
                .into_any_element(),
        )
    }
}
