//! 文档里的闪卡块：上半正面、下半背面，都能直接编辑；顶部显示卡型与复习进度。

use super::render::SlotSpec;
use super::*;
use crate::flashcard::{card_numbers, clozes, fmt_interval};
use crate::model::BlockKind;
use crate::theme::Theme;
use crate::ui::icon;

impl Editor {
    pub(super) fn render_flashcard(&mut self, ix: usize, blk: &Block, focused: bool, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let BlockKind::Flashcard { back, sched, .. } = &blk.kind else { return div().into_any_element() };
        let nums = card_numbers(&blk.text.text);
        let cloze = nums != [0];
        let now = crate::store::now_ms();
        let due = nums.iter().filter(|n| sched.get(&n.to_string()).is_none_or(|s| s.due <= now)).count();
        let next = nums.iter().filter_map(|n| sched.get(&n.to_string())).map(|s| s.due).filter(|d| *d > now).min();
        let status = if sched.is_empty() {
            "新卡".to_string()
        } else if due > 0 {
            format!("{due} 张待复习")
        } else {
            next.map(|d| format!("{}后复习", fmt_interval(d - now))).unwrap_or_default()
        };
        let kind = if cloze { format!("挖空 · {} 处", clozes(&blk.text.text).len()) } else { "问答".to_string() };
        let front_spec = SlotSpec {
            placeholder: Some(if cloze { "正面" } else { "正面：问题。写 {{c1::答案}} 可做挖空卡" }.into()),
            weight: FontWeight::MEDIUM,
            ..SlotSpec::body(t)
        };
        let back_spec =
            SlotSpec {
                color: t.text_secondary, placeholder: Some(if cloze { "背面（可选）：补充说明" } else { "背面：答案" }.into()), ..SlotSpec::body(t)
            };
        let label = |s: &'static str| div().text_size(px(11.)).text_color(t.text_muted).child(crate::i18n::tr(s));
        div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(px(10.))
            .border_1()
            .border_color(if focused && self.head.block == ix { t.accent } else { t.border })
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .px(px(14.))
                    .py(px(6.))
                    .bg(t.sidebar_bg)
                    .border_b_1()
                    .border_color(t.border)
                    .text_size(px(12.))
                    .text_color(t.text_secondary)
                    .child(icon("book", 13., t.accent))
                    .child(crate::i18n::tr("闪卡"))
                    .child(div().text_color(t.text_muted).child(kind))
                    .child(div().flex_1())
                    .child(div().text_color(if due > 0 || sched.is_empty() { t.accent } else { t.text_muted }).child(status)),
            )
            .child(
                div().px(px(14.)).pt(px(10.)).pb(px(8.)).child(label("正面")).child(
                    div().on_mouse_down(MouseButton::Left, self.slot_click(ix, 0, cx)).child(self.render_slot(ix, 0, &blk.text, front_spec, focused, t)),
                ),
            )
            .child(div().mx(px(14.)).h(px(1.)).bg(t.border))
            .child(
                div()
                    .px(px(14.))
                    .pt(px(8.))
                    .pb(px(10.))
                    .child(label("背面"))
                    .child(div().on_mouse_down(MouseButton::Left, self.slot_click(ix, 1, cx)).child(self.render_slot(ix, 1, back, back_spec, focused, t))),
            )
            .into_any_element()
    }

    /// 新建闪卡块。
    pub fn flashcard_block(front: RichText, back: RichText) -> Block {
        Block::new(BlockKind::Flashcard { back, card_id: crate::store::new_uuid()[..12].to_string(), sched: Default::default() }, front)
    }

    /// 块菜单「转为闪卡」：正文类块的文字做正面。
    pub fn convert_to_flashcard(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(b) = self.content.blocks.get(ix).filter(|b| ix > 0 && b.kind.is_textual()) else { return };
        let (text, indent) = (b.text.clone(), b.indent);
        self.checkpoint(EditKind::Other);
        let blk = self.block_mut(ix);
        *blk = Self::flashcard_block(text, RichText::default());
        blk.indent = indent;
        self.head = Pos::new(ix, 1, 0);
        self.anchor = self.head;
        self.changed(cx);
    }

    /// 写回复习状态（复习面板评分后，当前打开的文档走这里，保证编辑器里的内容同步）。
    pub fn set_card_sched(&mut self, card_id: &str, n: u8, s: crate::flashcard::Sched, cx: &mut Context<Self>) -> bool {
        let Some(ix) = self.content.blocks.iter().position(|b| matches!(&b.kind, BlockKind::Flashcard { card_id: c, .. } if c == card_id)) else {
            return false;
        };
        if let BlockKind::Flashcard { sched, .. } = &mut self.block_mut(ix).kind {
            sched.insert(n.to_string(), s);
        }
        // 复习不进撤销栈（不是编辑）
        self.version += 1;
        self.dirty = true;
        self.save_now(cx);
        cx.notify();
        true
    }

    /// PDF 摘录「制成闪卡」：插在这份 PDF 的块后面。
    pub fn insert_card_after_embed(&mut self, obj: &str, front: &str, back: &str, cx: &mut Context<Self>) {
        let at =
            self.content.blocks.iter().position(|b| matches!(&b.kind, BlockKind::Embed { id, .. } if id == obj)).map_or(self.content.blocks.len(), |i| i + 1);
        self.checkpoint(EditKind::Other);
        self.insert_blocks(at, vec![Self::flashcard_block(RichText::plain(front), RichText::plain(back))]);
        self.changed(cx);
    }
}
