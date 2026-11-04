//! 多维表格界面的小部件：选项标签、头像、菜单项、面板、工具栏按钮等。

use crate::theme::Theme;
use crate::ui::{icon, Tooltip};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// 选项标签配色：(浅色背景, 文字)。深色模式下用文字色的半透明作背景。
const TAGS: [(u32, u32); 12] = [
    (0xe1eaff, 0x1d4fc3), // 蓝
    (0xd5f6f2, 0x0e7a6e), // 青
    (0xece2fe, 0x6425d0), // 紫
    (0xd9f5d6, 0x237b19), // 绿
    (0xfff5b8, 0x8f6400), // 黄
    (0xfeead2, 0xb35a00), // 橙
    (0xfde2e2, 0xc02a2a), // 红
    (0xfdddef, 0xb0237a), // 粉
    (0xeef6c6, 0x5b7a00), // 黄绿
    (0xf6e1d3, 0x8a4b23), // 棕
    (0xdff3ff, 0x0b6aa2), // 天蓝
    (0xeff0f1, 0x4e5969), // 灰
];

pub fn tag_color(c: u8, dark: bool) -> (Hsla, Hsla) {
    let (bg, fg) = TAGS[c as usize % TAGS.len()];
    let (bg, fg): (Hsla, Hsla) = (rgb(bg).into(), rgb(fg).into());
    if dark {
        let mut f = fg;
        f.l = 0.78;
        (fg.opacity(0.32), f)
    } else {
        (bg, fg)
    }
}

/// 纯色（色块、看板列头小圆点、甘特条）。
pub fn solid_color(c: u8) -> Hsla {
    let (_, fg) = TAGS[c as usize % TAGS.len()];
    let mut h: Hsla = rgb(fg).into();
    h.l = (h.l + 0.12).min(0.6);
    h
}

pub fn tag(name: impl Into<SharedString>, color: u8, t: &Theme) -> Div {
    let (bg, fg) = tag_color(color, t.dark);
    div()
        .flex_none()
        .px(px(8.))
        .h(px(20.))
        .flex()
        .items_center()
        .rounded(px(10.))
        .bg(bg)
        .text_color(fg)
        .text_size(px(12.))
        .whitespace_nowrap()
        .overflow_hidden()
        .max_w(px(200.))
        .child(name.into())
}

/// 人员头像：取名字最后一个字（中文）或首字母，按名字哈希配色。
pub fn avatar(name: &str, size: f32) -> Div {
    let ch: String = if !name.is_ascii() {
        name.chars().last().map(String::from).unwrap_or_default()
    } else {
        name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
    };
    let h = name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let colors = [0x3370ff, 0x14b8a6, 0x7f3bf5, 0xf5a339, 0xe5484d, 0x2ea121, 0xf14c9d, 0x0ea5e9];
    div()
        .flex_none()
        .size(px(size))
        .rounded_full()
        .bg(rgb(colors[h as usize % colors.len()]))
        .text_color(white())
        .text_size(px(size * 0.5))
        .flex()
        .items_center()
        .justify_center()
        .child(ch)
}

pub fn person_chip(name: &str, t: &Theme) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.))
        .pr(px(8.))
        .h(px(22.))
        .rounded(px(11.))
        .bg(t.hover)
        .text_size(px(12.))
        .child(avatar(name, 20.))
        .child(name.to_string())
}

pub fn link_chip(name: impl Into<SharedString>, t: &Theme) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .h(px(20.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(t.accent_soft)
        .text_color(t.link)
        .text_size(px(12.))
        .whitespace_nowrap()
        .overflow_hidden()
        .max_w(px(180.))
        .child(name.into())
}

/// 弹出面板外框。
pub fn panel(t: &Theme) -> Div {
    div()
        .occlude()
        .flex()
        .flex_col()
        .bg(t.panel_bg)
        .border_1()
        .border_color(t.border)
        .rounded(px(8.))
        .shadow_lg()
        .py(px(4.))
        .text_size(px(13.))
        .text_color(t.text)
        .font_family("PingFang SC")
}

/// 菜单项：图标 + 文字（+ 右侧附注）。
pub fn menu_item(id: impl Into<ElementId>, ic: Option<&'static str>, label: impl Into<SharedString>, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .mx(px(4.))
        .px(px(8.))
        .h(px(32.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .rounded(px(6.))
        .text_size(px(13.))
        .cursor_pointer()
        .hover(|s| s.bg(t.hover))
        .when_some(ic, |d, i| d.child(icon(i, 15., t.text_secondary)))
        .child(div().flex_1().whitespace_nowrap().child(label.into()))
}

pub fn menu_sep(t: &Theme) -> Div {
    div().my(px(4.)).mx(px(8.)).h(px(1.)).bg(t.border)
}

pub fn section_label(s: impl Into<SharedString>, t: &Theme) -> Div {
    div().px(px(12.)).pt(px(8.)).pb(px(4.)).text_size(px(12.)).text_color(t.text_muted).child(s.into())
}

/// 工具栏按钮：图标 + 文字；`on` 时高亮（设置了筛选 / 排序后按钮变色并显示条数）。
pub fn tool_btn(id: impl Into<ElementId>, ic: &'static str, label: impl Into<SharedString>, on: bool, t: &Theme) -> Stateful<Div> {
    let fg = if on { t.accent } else { t.text_secondary };
    div()
        .id(id)
        .flex_none()
        .px(px(8.))
        .h(px(28.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(fg)
        .when(on, |d| d.bg(t.accent_soft))
        .hover(|s| s.bg(t.hover))
        .child(icon(ic, 15., fg))
        .child(label.into())
}

pub fn primary_btn(id: impl Into<ElementId>, label: impl Into<SharedString>, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(14.))
        .h(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(t.accent)
        .text_color(white())
        .text_size(px(13.))
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
        .child(label.into())
}

pub fn ghost_btn(id: impl Into<ElementId>, label: impl Into<SharedString>, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(14.))
        .h(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .border_1()
        .border_color(t.border_strong)
        .text_size(px(13.))
        .cursor_pointer()
        .hover(|s| s.bg(t.hover))
        .child(label.into())
}

/// 下拉选择框（点击后由调用方弹出选项）。
pub fn select_box(id: impl Into<ElementId>, label: impl Into<SharedString>, ic: Option<&'static str>, t: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(30.))
        .px(px(8.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .border_1()
        .border_color(t.border_strong)
        .cursor_pointer()
        .hover(|s| s.border_color(t.accent))
        .when_some(ic, |d, i| d.child(icon(i, 14., t.text_secondary)))
        .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label.into()))
        .child(icon("chevron-down", 14., t.text_muted))
}

/// 输入框外框（里面放 LineInput）。
pub fn input_box(t: &Theme) -> Div {
    div().h(px(30.)).px(px(8.)).flex().items_center().rounded(px(6.)).border_1().border_color(t.border_strong).text_size(px(13.)).line_height(px(28.))
}

pub fn check_box(on: bool, t: &Theme) -> Div {
    div()
        .flex_none()
        .size(px(16.))
        .rounded(px(4.))
        .border_1()
        .border_color(if on { t.accent } else { t.border_strong })
        .when(on, |d| d.bg(t.accent))
        .flex()
        .items_center()
        .justify_center()
        .when(on, |d| d.child(icon("check", 12., white())))
}

pub fn switch(on: bool, t: &Theme) -> Div {
    div()
        .flex_none()
        .w(px(28.))
        .h(px(16.))
        .rounded(px(8.))
        .bg(if on { t.accent } else { t.border_strong })
        .relative()
        .child(div().absolute().top(px(2.)).left(px(if on { 14. } else { 2. })).size(px(12.)).rounded_full().bg(white()))
}

pub fn tip(s: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |_, cx| cx.new(|_| Tooltip(s.into())).into()
}

/// 评分图标名。
pub fn rating_icon(sym: &str) -> &'static str {
    match sym {
        "heart" => "f-heart",
        "thumb" => "f-thumb",
        "fire" => "f-fire",
        _ => "f-star",
    }
}

/// 已评分的实心图标。
pub fn rating_icon_on(sym: &str) -> &'static str {
    match sym {
        "heart" => "f-heart-on",
        "thumb" => "f-thumb-on",
        "fire" => "f-fire-on",
        _ => "f-star-on",
    }
}

pub const RATING_SYMS: [(&str, &str); 4] = [("star", "星星"), ("heart", "爱心"), ("thumb", "点赞"), ("fire", "火焰")];

pub fn rating_color(sym: &str) -> Hsla {
    match sym {
        "heart" => rgb(0xf54a45).into(),
        "thumb" => rgb(0x3370ff).into(),
        "fire" => rgb(0xff7d00).into(),
        _ => rgb(0xffc60a).into(),
    }
}
