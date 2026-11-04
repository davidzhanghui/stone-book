//! 有序列表编号菜单（思路来自 quiet.md 的 numbering.rs）：点击编号弹出
//! 「继续之前的编号 / 开始新列表 / 修改编号值」。
//!
//! 三个选项都落到同一个字段 `Block::num`：从这一项起按该值继续计数（见 `convert::ordered_numbers`）。

use super::{EditKind, Editor};
use crate::line_input::{InputEvent, LineInput};
use crate::model::BlockKind;
use crate::theme::Theme;
use gpui::{prelude::FluentBuilder, *};

pub struct OrderedMenu {
    pub block: usize,
    /// 「修改编号值」的输入框
    pub input: Option<(Entity<LineInput>, Subscription)>,
}

/// 菜单里能做的事。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberOptions {
    /// 这一项当前显示的编号
    pub current: usize,
    /// 能接上的前一个列表的最后编号（这一项是列表开头、前面还有同级有序列表时）
    pub previous: Option<usize>,
    /// 这一项是不是所在列表的第一项
    pub first: bool,
}

impl Editor {
    /// 第 b 块（有序列表项）的编号选项。
    pub fn ordered_number_options(&self, b: usize) -> Option<NumberOptions> {
        let blocks = &self.content.blocks;
        let blk = blocks.get(b)?;
        if blk.kind != BlockKind::Ordered {
            return None;
        }
        let nums = crate::convert::ordered_numbers(blocks);
        let ind = blk.indent;
        // 往前跳过更深层级的列表项；遇到同级有序项说明不是开头
        let mut k = b;
        let mut first = true;
        while k > 1 {
            k -= 1;
            let p = &blocks[k];
            if p.kind.is_list() && p.indent > ind {
                continue;
            }
            first = !(p.kind == BlockKind::Ordered && p.indent == ind);
            break;
        }
        let previous = if first { (1..b).rev().find(|&k| blocks[k].kind == BlockKind::Ordered && blocks[k].indent == ind).map(|k| nums[k]) } else { None };
        Some(NumberOptions { current: nums[b], previous, first })
    }

    /// 设置（或清除）第 b 块的指定编号。
    pub fn set_ordered_number(&mut self, b: usize, num: Option<u32>, cx: &mut Context<Self>) {
        if self.content.blocks.get(b).is_none_or(|x| x.kind != BlockKind::Ordered) {
            return;
        }
        self.checkpoint(EditKind::Other);
        self.block_mut(b).num = num;
        self.pop.ordered_menu = None;
        self.changed(cx);
    }

    pub fn open_ordered_menu(&mut self, b: usize, cx: &mut Context<Self>) {
        if self.is_read_only() {
            return;
        }
        self.pop.ordered_menu = if self.pop.ordered_menu.as_ref().is_some_and(|m| m.block == b) { None } else { Some(OrderedMenu { block: b, input: None }) };
        cx.notify();
    }

    /// 切到「修改编号值」：输入框预填当前编号，回车确定，Esc 取消。
    fn edit_ordered_number(&mut self, b: usize, current: usize, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| LineInput::new("输入编号", cx));
        input.update(cx, |i, cx| i.set_text(&current.to_string(), cx));
        let sub = cx.subscribe_in(&input, window, move |this, input, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => {
                if let Some(n) = parse_number(&input.read(cx).text()) {
                    this.set_ordered_number(b, Some(n), cx);
                    window.focus(&this.focus);
                }
            }
            InputEvent::Cancel => {
                this.pop.ordered_menu = None;
                window.focus(&this.focus);
                cx.notify();
            }
            _ => cx.notify(),
        });
        window.focus(&input.focus_handle(cx));
        self.pop.ordered_menu = Some(OrderedMenu { block: b, input: Some((input, sub)) });
        cx.notify();
    }

    /// 编号标记旁的菜单（打开时才渲染）。
    pub(super) fn render_ordered_menu(&self, ix: usize, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.pop.ordered_menu.as_ref().filter(|m| m.block == ix)?;
        let opts = self.ordered_number_options(ix)?;
        let panel = div()
            .id("ordered-number-menu")
            .occlude()
            .p_1()
            .bg(t.panel_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(8.))
            .shadow_lg()
            .text_size(px(13.))
            .text_color(t.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.pop.ordered_menu = None;
                cx.notify();
            }));
        let panel = if let Some((input, _)) = &menu.input {
            let valid = parse_number(&input.read(cx).text()).is_some();
            panel
                .w(px(220.))
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(div().text_color(t.text_secondary).child(crate::i18n::tr("新编号为")))
                .child(
                    div()
                        .h(px(30.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .border_1()
                        .border_color(if valid { t.accent } else { t.border_strong })
                        .rounded(px(6.))
                        .child(input.clone()),
                )
                .child(
                    div()
                        .id("ordered-number-ok")
                        .h(px(30.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .bg(if valid { t.accent } else { t.hover })
                        .text_color(if valid { gpui::white() } else { t.text_muted })
                        .child(crate::i18n::tr("确定"))
                        .when(valid, |d| {
                            let input = input.clone();
                            d.cursor_pointer().on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(n) = parse_number(&input.read(cx).text()) {
                                    this.set_ordered_number(ix, Some(n), cx);
                                    window.focus(&this.focus);
                                }
                            }))
                        }),
                )
        } else {
            let row = |id: &'static str, label: &'static str, enabled: bool| {
                div()
                    .id(id)
                    .px_2()
                    .py(px(6.))
                    .rounded(px(4.))
                    .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(t.hover)))
                    .when(!enabled, |d| d.text_color(t.text_muted))
                    .child(label)
            };
            let current = opts.current;
            panel
                .w(px(180.))
                .child(
                    row("ol-continue", "继续之前的编号", opts.previous.is_some()).when_some(opts.previous, |d, prev| {
                        d.on_click(cx.listener(move |this, _, _, cx| this.set_ordered_number(ix, Some(prev as u32 + 1), cx)))
                    }),
                )
                .child(
                    row("ol-restart", "开始新列表", current != 1)
                        .when(current != 1, |d| d.on_click(cx.listener(move |this, _, _, cx| this.set_ordered_number(ix, Some(1), cx)))),
                )
                .child(row("ol-edit", "修改编号值", true).on_click(cx.listener(move |this, _, window, cx| this.edit_ordered_number(ix, current, window, cx))))
                .when(self.content.blocks[ix].num.is_some(), |d| {
                    d.child(div().my_1().h(px(1.)).bg(t.border))
                        .child(row("ol-auto", "恢复自动编号", true).on_click(cx.listener(move |this, _, _, cx| this.set_ordered_number(ix, None, cx))))
                })
        };
        Some(deferred(anchored().offset(point(px(0.), px(30.))).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(2).into_any_element())
    }
}

/// 编号必须是 1..=99999 的整数。
pub fn parse_number(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok().filter(|n| (1..=99_999).contains(n))
}

#[cfg(test)]
mod tests {
    use super::parse_number;
    use crate::convert::ordered_numbers;
    use crate::model::{Block, BlockKind, RichText};

    fn ol(indent: u8, num: Option<u32>) -> Block {
        let mut b = Block::new(BlockKind::Ordered, RichText::plain("x"));
        b.indent = indent;
        b.num = num;
        b
    }

    #[test]
    fn explicit_numbers_restart_the_count() {
        let blocks = vec![
            Block::new(BlockKind::Title, RichText::default()),
            ol(0, None),
            ol(0, None),
            Block::text("插一段"),
            ol(0, Some(3)),
            ol(0, None),
            ol(0, Some(1)),
            ol(1, Some(5)),
            ol(1, None),
            ol(0, None),
        ];
        let blocks: Vec<std::sync::Arc<Block>> = blocks.into_iter().map(std::sync::Arc::new).collect();
        assert_eq!(ordered_numbers(&blocks), vec![0, 1, 2, 0, 3, 4, 1, 5, 6, 2]);
    }

    #[test]
    fn number_input_must_be_positive() {
        assert_eq!(parse_number(" 12 "), Some(12));
        assert_eq!(parse_number("0"), None);
        assert_eq!(parse_number("-3"), None);
        assert_eq!(parse_number("abc"), None);
    }
}
