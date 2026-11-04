//! 草图界面：顶部工具栏、左上菜单、右上完成、左侧属性面板、左下缩放栏、右键菜单、提示。

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

const STROKES: [&str; 5] = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];
const BGS: [&str; 5] = [TRANSPARENT, "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];
const CANVAS_BGS: [&str; 5] = ["#ffffff", "#f8f9fa", "#f5faff", "#fffce8", "#fdf8f6"];

fn hex_hsla(s: &str) -> Hsla {
    crate::canvas::Col::parse(s).map(|c| c.hsla()).unwrap_or(gpui::transparent_black())
}

impl SketchEditor {
    pub(super) fn render_toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let tools: [(Tool, &'static str, &'static str); 12] = [
            (Tool::Hand, "hand", "抓手 H"),
            (Tool::Select, "pointer", "选择 V / 1"),
            (Tool::Rect, "square", "矩形 R / 2"),
            (Tool::Diamond, "diamond", "菱形 D / 3"),
            (Tool::Ellipse, "circle", "椭圆 O / 4"),
            (Tool::Arrow, "arrow-right", "箭头 A / 5"),
            (Tool::Line, "minus", "直线 L / 6"),
            (Tool::Draw, "pencil", "画笔 P / 7"),
            (Tool::Text, "type", "文本 T / 8"),
            (Tool::Image, "image", "图片 I / 9"),
            (Tool::Eraser, "eraser2", "橡皮 E / 0"),
            (Tool::Frame, "section", "框架 F"),
        ];
        let lock = self.tool_lock;
        div().absolute().top(px(12.)).left_0().right_0().flex().justify_center().child(
            panel(t)
                .h(px(44.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(2.))
                .child(
                    div()
                        .id("sk-lock")
                        .size(px(34.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .cursor_pointer()
                        .when(lock, |d| d.bg(t.accent_soft))
                        .hover(|s| s.bg(t.hover))
                        .tooltip(|_, cx| cx.new(|_| Tooltip("保持当前工具 Q".into())).into())
                        .child(icon(if lock { "lock" } else { "unlock" }, 16., if lock { t.accent } else { t.text_muted }))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tool_lock = !this.tool_lock;
                            cx.notify();
                        })),
                )
                .child(div().w(px(1.)).h(px(20.)).mx(px(4.)).bg(t.border))
                .children(tools.into_iter().enumerate().map(|(i, (tool, ic, tip))| {
                    let on = self.tool == tool;
                    div()
                        .id(("sk-tool", i))
                        .size(px(34.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .cursor_pointer()
                        .when(on, |d| d.bg(t.accent_soft))
                        .hover(|s| s.bg(t.hover))
                        .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                        .child(icon(ic, 17., if on { t.accent } else { t.text }))
                        .on_click(cx.listener(move |this, _, w, cx| this.set_tool(tool, w, cx)))
                })),
        )
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
                    .child(btn("sz-undo", "rotate-ccw", "撤销 ⌘Z", !self.undo.is_empty()).on_click(cx.listener(|this, _, _, cx| this.do_undo(cx))))
                    .child(btn("sz-redo", "redo", "重做 ⇧⌘Z", !self.redo.is_empty()).on_click(cx.listener(|this, _, _, cx| this.do_redo(cx)))),
            )
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
                .child(item("cm-dup", "再制", "⌘D").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.duplicate(cx);
                })))
                .child(item("cm-png", "复制为 PNG", "⇧⌘C").on_click(cx.listener(move |this, _, _, cx| {
                    close(this);
                    this.copy_png(cx);
                })))
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

    pub(super) fn render_props(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let creating = matches!(self.tool, Tool::Rect | Tool::Diamond | Tool::Ellipse | Tool::Arrow | Tool::Line | Tool::Draw | Tool::Text);
        let sel = self.selected();
        if sel.is_empty() && !creating {
            return None;
        }
        if self.editing.as_ref().is_some_and(|e| e.frame_name) {
            return None;
        }
        // 展示哪些属性：按选中元素（或当前工具）的类型
        let kinds: Vec<String> = if sel.is_empty() {
            vec![self.tool.kind().unwrap_or("rectangle").to_string()]
        } else {
            let mut k: Vec<String> = sel.iter().map(|e| e.kind.clone()).collect();
            // 容器里的文字也算文字属性
            if sel.iter().any(|e| e.bound_text_id().is_some()) {
                k.push("text".into());
            }
            k
        };
        let has = |k: &str| kinds.iter().any(|x| x == k);
        let shape = has("rectangle") || has("diamond") || has("ellipse");
        let linear = has("arrow") || has("line");
        let first = sel.first().copied();
        let cur = &self.cur;
        let stroke = first.map(|e| e.stroke_color.clone()).unwrap_or(cur.stroke.clone());
        let bg = first.map(|e| e.background_color.clone()).unwrap_or(cur.bg.clone());
        let fill = first.map(|e| e.fill_style.clone()).unwrap_or(cur.fill.clone());
        let sw = first.map(|e| e.stroke_width).unwrap_or(cur.stroke_width);
        let ss = first.map(|e| e.stroke_style.clone()).unwrap_or(cur.stroke_style.clone());
        let rough = first.map(|e| e.roughness).unwrap_or(cur.roughness);
        let op = first.map(|e| e.opacity).unwrap_or(cur.opacity);
        let round = first.map(|e| e.roundness().is_some()).unwrap_or(cur.round);
        let text_el = sel.iter().find(|e| e.is_text()).copied().or_else(|| first.and_then(|e| e.bound_text_id()).and_then(|t| self.doc.get(&t)));
        let fs = text_el.map(|e| e.font_size_or()).unwrap_or(cur.font_size);
        let ff = text_el.map(|e| e.font_family()).unwrap_or(cur.font_family);
        let ta = text_el.map(|e| e.text_align().to_string()).unwrap_or(cur.text_align.clone());
        let sa = first.and_then(|e| e.arrowhead(true).map(|s| s.to_string())).or(if first.is_none() { cur.start_arrow.clone() } else { None });
        let ea = first.and_then(|e| e.arrowhead(false).map(|s| s.to_string())).or(if first.is_none() { cur.end_arrow.clone() } else { None });

        let title = |s: &'static str| div().mt(px(8.)).mb(px(4.)).text_size(px(11.5)).text_color(t.text_muted).child(s);
        let chip = |id: SharedString, label: String, on: bool| {
            div()
                .id(id)
                .h(px(26.))
                .min_w(px(28.))
                .px(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(12.))
                .border_1()
                .border_color(if on { t.accent } else { t.border })
                .when(on, |d| d.bg(t.accent_soft).text_color(t.accent))
                .hover(|s| s.bg(t.hover))
                .child(label)
        };
        let swatch = |id: SharedString, c: &str, on: bool| {
            let transparent = c == TRANSPARENT;
            div()
                .id(id)
                .size(px(24.))
                .rounded(px(5.))
                .cursor_pointer()
                .border_1()
                .border_color(if on { t.accent } else { t.border })
                .when(on, |d| d.border_2())
                .when(!transparent, |d| d.bg(hex_hsla(c)))
                .when(transparent, |d| d.flex().items_center().justify_center().child(icon("ban", 12., t.text_muted)))
        };
        let row = || div().flex().flex_wrap().gap(px(5.));

        let mut p = panel(t).absolute().top(px(68.)).left(px(16.)).w(px(212.)).p(px(10.)).flex().flex_col();
        let text_only = kinds.iter().all(|k| k == "text");
        let image_only = kinds.iter().all(|k| k == "image" || k == "frame");

        if !image_only {
            p = p.child(title(if text_only { "文字颜色" } else { "描边" })).child(row().children(STROKES.iter().enumerate().map(|(i, c)| {
                let c = c.to_string();
                let on = stroke.eq_ignore_ascii_case(&c);
                let c2 = c.clone();
                swatch(format!("ps-{i}").into(), &c, on).on_click(cx.listener(move |this, _, _, cx| {
                    let c = c2.clone();
                    let c3 = c.clone();
                    this.set_prop(move |s| s.stroke = c.clone(), move |e| e.stroke_color = c3.clone(), cx)
                }))
            })));
        }
        if shape || (linear && has("line")) || (sel.is_empty() && creating && !matches!(self.tool, Tool::Text | Tool::Arrow | Tool::Line | Tool::Draw)) {
            p = p.child(title("背景")).child(row().children(BGS.iter().enumerate().map(|(i, c)| {
                let c = c.to_string();
                let on = bg.eq_ignore_ascii_case(&c);
                let c2 = c.clone();
                swatch(format!("pb-{i}").into(), &c, on).on_click(cx.listener(move |this, _, _, cx| {
                    let c = c2.clone();
                    let c3 = c.clone();
                    this.set_prop(
                        move |s| s.bg = c.clone(),
                        move |e| {
                            if !e.is_text() {
                                e.background_color = c3.clone()
                            }
                        },
                        cx,
                    )
                }))
            })));
            if bg != TRANSPARENT {
                let fills = [("hachure", "斜线"), ("cross-hatch", "交叉"), ("solid", "实心"), ("zigzag", "折线")];
                p = p.child(title("填充")).child(row().children(fills.into_iter().map(|(k, l)| {
                    chip(format!("pf-{k}").into(), l.into(), fill == k)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_prop(move |s| s.fill = k.into(), move |e| e.fill_style = k.into(), cx)))
                })));
            }
        }
        if !text_only && !image_only {
            let widths = [(1., "细"), (2., "粗"), (4., "特粗")];
            p = p.child(title("描边宽度")).child(row().children(widths.into_iter().map(|(w, l)| {
                chip(format!("pw-{w}").into(), l.into(), (sw - w).abs() < 0.01).on_click(cx.listener(move |this, _, _, cx| {
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
            let styles = [("solid", "实线"), ("dashed", "虚线"), ("dotted", "点线")];
            p = p.child(title("边框样式")).child(row().children(styles.into_iter().map(|(k, l)| {
                chip(format!("pss-{k}").into(), l.into(), ss == k).on_click(cx.listener(move |this, _, _, cx| {
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
            if !has("freedraw") || kinds.len() > 1 {
                let roughs = [(0., "朴素"), (1., "手绘"), (2., "潦草")];
                p = p.child(title("线条风格")).child(row().children(roughs.into_iter().map(|(r, l)| {
                    chip(format!("pr-{r}").into(), l.into(), (rough - r).abs() < 0.01).on_click(cx.listener(move |this, _, _, cx| {
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
        }
        if has("rectangle") || has("diamond") || linear {
            let opts = [(false, if linear && !shape { "直线" } else { "直角" }), (true, if linear && !shape { "曲线" } else { "圆角" })];
            p = p.child(title(if linear && !shape { "线型" } else { "边角" })).child(row().children(opts.into_iter().map(|(r, l)| {
                chip(format!("prd-{r}").into(), l.into(), round == r).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.round = r,
                        move |e| match e.kind.as_str() {
                            "rectangle" => e.set("roundness", if r { json!({ "type": 3 }) } else { Value::Null }),
                            "diamond" | "arrow" | "line" => e.set("roundness", if r { json!({ "type": 2 }) } else { Value::Null }),
                            _ => {}
                        },
                        cx,
                    )
                }))
            })));
        }
        if has("arrow") || (sel.is_empty() && self.tool == Tool::Arrow) {
            let heads: [(Option<&'static str>, &'static str); 7] = [
                (None, "无"),
                (Some("arrow"), "箭头"),
                (Some("triangle"), "三角"),
                (Some("triangle_outline"), "空三角"),
                (Some("bar"), "竖线"),
                (Some("dot"), "圆点"),
                (Some("diamond"), "菱形"),
            ];
            for start in [true, false] {
                let curh = if start { sa.clone() } else { ea.clone() };
                p = p.child(title(if start { "起点" } else { "终点" })).child(row().children(heads.iter().map(|(k, l)| {
                    let k = *k;
                    chip(format!("pa-{start}-{}", k.unwrap_or("none")).into(), (*l).into(), curh.as_deref() == k).on_click(cx.listener(
                        move |this, _, _, cx| {
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
                        },
                    ))
                })));
            }
        }
        if has("text") || (sel.is_empty() && self.tool == Tool::Text) {
            let sizes = [(16., "小"), (20., "中"), (28., "大"), (36., "特大")];
            p = p.child(title("字号")).child(row().children(sizes.into_iter().map(|(f, l)| {
                chip(format!("pfs-{f}").into(), l.into(), (fs - f).abs() < 0.01).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.font_size = f,
                        move |e| {
                            if e.is_text() {
                                e.font_size = Some(f)
                            }
                        },
                        cx,
                    )
                }))
            })));
            let fams = [(FONT_HAND, "手写"), (FONT_NORMAL, "常规"), (FONT_CODE, "代码")];
            let ff_group = |f: i64| match f {
                2 | 6 | 7 | 9 => FONT_NORMAL,
                3 | 8 => FONT_CODE,
                _ => FONT_HAND,
            };
            p = p.child(title("字体")).child(row().children(fams.into_iter().map(|(f, l)| {
                chip(format!("pff-{f}").into(), l.into(), ff_group(ff) == f).on_click(cx.listener(move |this, _, _, cx| {
                    this.set_prop(
                        move |s| s.font_family = f,
                        move |e| {
                            if e.is_text() {
                                e.set("fontFamily", json!(f))
                            }
                        },
                        cx,
                    )
                }))
            })));
            let aligns = [("left", "左"), ("center", "中"), ("right", "右")];
            p = p.child(title("对齐")).child(row().children(aligns.into_iter().map(|(a, l)| {
                chip(format!("pta-{a}").into(), l.into(), ta == a).on_click(cx.listener(move |this, _, _, cx| {
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
        }
        if !has("frame") {
            let ops = [20., 40., 60., 80., 100.];
            p = p.child(title("透明度")).child(row().children(ops.into_iter().map(|o| {
                chip(format!("po-{o}").into(), format!("{o}"), (op - o).abs() < 0.5)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_prop(move |s| s.opacity = o, move |e| e.opacity = o, cx)))
            })));
        }
        if !sel.is_empty() {
            let layers = [(-2, "置底"), (-1, "下移"), (1, "上移"), (2, "置顶")];
            p = p.child(title("图层")).child(
                row().children(
                    layers
                        .into_iter()
                        .map(|(d, l)| chip(format!("pl-{d}").into(), l.into(), false).on_click(cx.listener(move |this, _, _, cx| this.z_order(d, cx)))),
                ),
            );
            if sel.len() >= 2 {
                let al = [(0u8, "左"), (1, "水平中"), (2, "右"), (3, "上"), (4, "垂直中"), (5, "下")];
                p = p.child(title("对齐")).child(
                    row().children(
                        al.into_iter()
                            .map(|(k, l)| chip(format!("pal-{k}").into(), l.into(), false).on_click(cx.listener(move |this, _, _, cx| this.align(k, cx)))),
                    ),
                );
                if sel.len() >= 3 {
                    p = p.child(
                        row()
                            .mt(px(5.))
                            .child(chip("pd-h".into(), "水平分布".into(), false).on_click(cx.listener(|this, _, _, cx| this.distribute(true, cx))))
                            .child(chip("pd-v".into(), "垂直分布".into(), false).on_click(cx.listener(|this, _, _, cx| this.distribute(false, cx)))),
                    );
                }
            }
            let grouped = sel.iter().any(|e| !e.group_ids.is_empty());
            p = p.child(title("操作")).child(
                row()
                    .child(chip("pop-dup".into(), "再制".into(), false).on_click(cx.listener(|this, _, _, cx| this.duplicate(cx))))
                    .child(chip("pop-del".into(), "删除".into(), false).on_click(cx.listener(|this, _, _, cx| this.delete_sel(cx))))
                    .when(sel.len() >= 2, |d| d.child(chip("pop-grp".into(), "成组".into(), false).on_click(cx.listener(|this, _, _, cx| this.group(cx)))))
                    .when(grouped, |d| d.child(chip("pop-ungrp".into(), "解组".into(), false).on_click(cx.listener(|this, _, _, cx| this.ungroup(cx)))))
                    .child(chip("pop-lock".into(), "锁定".into(), false).on_click(cx.listener(|this, _, _, cx| this.toggle_lock(cx)))),
            );
        }
        Some(p.id("sk-props").max_h(px(640.)).overflow_y_scroll().into_any_element())
    }
}
