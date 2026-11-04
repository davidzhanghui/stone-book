//! 画板搜索替换（⌘F，画板右上角的搜索）：
//! 图形、文本、便签、连线说明、分区标题、表格单元格、思维导图节点里的文字都能搜到；
//! 命中处在画布上高亮（当前项橙色），↵ / ⇧↵ 切换并把画面移过去；可替换当前项或全部替换。

use super::model::*;
use super::ui::panel;
use super::view::*;
use crate::canvas::{rect, Rect};
use crate::line_input::{InputEvent, LineInput};
use crate::theme::Theme;
use crate::ui::{icon, Tooltip};
use gpui::prelude::FluentBuilder;
use gpui::*;

pub struct Find {
    pub q: Entity<LineInput>,
    pub r: Entity<LineInput>,
    /// 当前查找的文字（输入框变化时同步，绘制高亮时不必再读输入框）
    pub query: String,
    pub replace: bool,
    pub cur: usize,
    _subs: Vec<Subscription>,
}

impl Find {
    pub fn focused(&self, window: &Window, cx: &App) -> bool {
        self.q.focus_handle(cx).is_focused(window) || self.r.focus_handle(cx).is_focused(window)
    }
}

/// 一处命中：元素、哪段文字、字节区间。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub id: u64,
    pub part: Part,
    pub start: usize,
    pub end: usize,
}

/// 不区分大小写查找所有不重叠的出现位置（按字符比较，中文与英文混排也不会错位）。
pub fn find_all(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    let nd: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if nd.is_empty() {
        return vec![];
    }
    let hc: Vec<(usize, char)> = hay.char_indices().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < hc.len() {
        let mut j = 0;
        let mut k = i;
        while j < nd.len() && k < hc.len() {
            let lc: Vec<char> = hc[k].1.to_lowercase().collect();
            if nd[j..].starts_with(&lc) {
                j += lc.len();
                k += 1;
            } else {
                break;
            }
        }
        if j == nd.len() {
            let end = hc.get(k).map(|x| x.0).unwrap_or(hay.len());
            out.push((hc[i].0, end));
            i = k;
        } else {
            i += 1;
        }
    }
    out
}

impl BoardEditor {
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_edit(cx);
        self.close_overlays();
        if let Some(f) = &self.find {
            let q = f.q.clone();
            q.update(cx, |q, cx| {
                let t = q.text();
                q.set_text(&t, cx)
            });
            window.focus(&q.focus_handle(cx));
            return;
        }
        let q = cx.new(|cx| LineInput::new("在画板中搜索", cx));
        let r = cx.new(|cx| LineInput::new("替换为", cx));
        let s1 = cx.subscribe_in(&q, window, |this, q, ev: &InputEvent, window, cx| match ev {
            InputEvent::Changed => {
                let text = q.read(cx).text();
                if let Some(f) = this.find.as_mut() {
                    f.query = text;
                    f.cur = 0;
                }
                this.reveal_hit();
                cx.notify();
            }
            InputEvent::Submit { shift } => this.find_step(if *shift { -1 } else { 1 }, cx),
            InputEvent::Down => this.find_step(1, cx),
            InputEvent::Up => this.find_step(-1, cx),
            InputEvent::Cancel => this.close_find(window, cx),
        });
        let s2 = cx.subscribe_in(&r, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => this.replace_one(cx),
            InputEvent::Cancel => this.close_find(window, cx),
            _ => {}
        });
        window.focus(&q.focus_handle(cx));
        self.find = Some(Find { q, r, query: String::new(), replace: false, cur: 0, _subs: vec![s1, s2] });
        cx.notify();
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find = None;
        window.focus(&self.focus);
        cx.notify();
    }

    /// 当前所有命中（按元素在画板里的先后、文字里的先后）。
    pub(super) fn find_hits(&self) -> Vec<Hit> {
        let Some(f) = self.find.as_ref().filter(|f| !f.query.is_empty()) else { return vec![] };
        let lk = self.doc.lookup();
        let mut out = vec![];
        for e in self.doc.els.iter().filter(|e| lk.visible(e.id)) {
            for part in e.parts() {
                for (start, end) in find_all(e.part_text(part), &f.query) {
                    out.push(Hit { id: e.id, part, start, end });
                }
            }
        }
        out
    }

    /// 命中处在画布上的区域（世界坐标）。
    fn hit_rect(&self, h: &Hit) -> Option<Rect> {
        let e = self.doc.get(h.id)?;
        Some(match (h.part, e.grid()) {
            (Part::Cell(i), Some(g)) => g.cell(i),
            _ if e.is_section() => self.section_title_rect(e),
            _ if e.is_lane() => rect(e.x, e.y, e.w, e.title_h().max(24.)),
            _ if e.is_connector() => {
                let m = self.doc.connector_mid(e);
                let w = crate::canvas::geom::estimate_width(&e.text, e.font_size, false) + 12.;
                rect(m.x - w / 2., m.y - e.font_size * 0.8, w, e.font_size * 1.6)
            }
            _ => self.doc.bounds_of(e),
        })
    }

    /// 叠加层用：(区域, 是否当前项)。同一区域只画一次。
    pub(super) fn find_rects(&self) -> Vec<(Rect, bool)> {
        let Some(f) = self.find.as_ref() else { return vec![] };
        let hits = self.find_hits();
        let cur = hits.get(f.cur.min(hits.len().saturating_sub(1))).copied();
        let mut out: Vec<(Rect, bool)> = vec![];
        for h in &hits {
            let Some(r) = self.hit_rect(h) else { continue };
            let is_cur = Some(*h) == cur;
            match out.iter_mut().find(|x| x.0 == r) {
                Some(x) => x.1 |= is_cur,
                None => out.push((r, is_cur)),
            }
        }
        out
    }

    /// 把当前命中移到视口里（已在视口内则不动）。
    fn reveal_hit(&mut self) {
        let hits = self.find_hits();
        let Some(f) = self.find.as_ref() else { return };
        let Some(h) = hits.get(f.cur.min(hits.len().saturating_sub(1))) else { return };
        let Some(r) = self.hit_rect(h) else { return };
        let cb = self.canvas_bounds;
        let (vw, vh) = (f32::from(cb.size.width), f32::from(cb.size.height));
        let s = self.view().r(r);
        let (x0, y0) = (f32::from(s.origin.x - cb.origin.x), f32::from(s.origin.y - cb.origin.y));
        let (x1, y1) = (x0 + f32::from(s.size.width), y0 + f32::from(s.size.height));
        let margin = 80.;
        if x0 < margin || y0 < margin || x1 > vw - margin || y1 > vh - margin {
            let c = r.center();
            self.offset = crate::canvas::pt(vw / 2. - c.x * self.zoom, vh / 2. - c.y * self.zoom);
        }
    }

    pub(super) fn find_step(&mut self, d: i32, cx: &mut Context<Self>) {
        let n = self.find_hits().len();
        if let Some(f) = self.find.as_mut() {
            if n > 0 {
                f.cur = ((f.cur.min(n - 1) as i32 + d).rem_euclid(n as i32)) as usize;
            }
        }
        self.reveal_hit();
        cx.notify();
    }

    fn replacement(&self, cx: &App) -> String {
        self.find.as_ref().map(|f| f.r.read(cx).text()).unwrap_or_default()
    }

    /// 替换当前项，然后停在下一处。
    pub(super) fn replace_one(&mut self, cx: &mut Context<Self>) {
        let hits = self.find_hits();
        let Some(cur) = self.find.as_ref().map(|f| f.cur.min(hits.len().saturating_sub(1))) else { return };
        let Some(h) = hits.get(cur).copied() else { return };
        if self.doc.get(h.id).is_none_or(|e| e.locked) {
            self.find_step(1, cx);
            return;
        }
        let with = self.replacement(cx);
        self.snapshot();
        if let Some(i) = self.doc.index(h.id) {
            let mut t = self.doc.els[i].part_text(h.part).to_string();
            t.replace_range(h.start..h.end, &with);
            self.doc.els[i].set_part_text(h.part, t);
        }
        self.commit(cx);
        self.reveal_hit();
    }

    pub(super) fn replace_all(&mut self, cx: &mut Context<Self>) {
        let hits = self.find_hits();
        if hits.is_empty() {
            return;
        }
        let with = self.replacement(cx);
        self.snapshot();
        // 从后往前替换，前面的字节位置不受影响
        for h in hits.iter().rev() {
            if let Some(i) = self.doc.index(h.id).filter(|i| !self.doc.els[*i].locked) {
                let mut t = self.doc.els[i].part_text(h.part).to_string();
                t.replace_range(h.start..h.end, &with);
                self.doc.els[i].set_part_text(h.part, t);
            }
        }
        if let Some(f) = self.find.as_mut() {
            f.cur = 0;
        }
        self.commit(cx);
    }

    /// 右上角的搜索框。
    pub(super) fn render_find(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let f = self.find.as_ref()?;
        let n = self.find_hits().len();
        let count = if f.query.is_empty() {
            String::new()
        } else if n == 0 {
            "无结果".into()
        } else {
            format!("{}/{}", f.cur.min(n - 1) + 1, n)
        };
        let btn = |id: &'static str, ic: &'static str, tip: &'static str| {
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                .child(icon(ic, 15., t.text_secondary))
        };
        let text_btn = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .h(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(12.5))
                .when(primary, |d| d.bg(t.accent).text_color(gpui::white()))
                .when(!primary, |d| d.border_1().border_color(t.border).hover(|s| s.bg(t.hover)))
                .child(label)
        };
        let field =
            |input: Entity<LineInput>| div().flex_1().h(px(30.)).px(px(8.)).flex().items_center().rounded(px(6.)).bg(t.hover).text_size(px(13.)).child(input);
        Some(
            div()
                .absolute()
                .top(px(62.))
                .right(px(16.))
                .w(px(380.))
                .child(
                    panel(t)
                        .id("board-find")
                        .p(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .child(icon("search", 15., t.text_muted))
                                .child(field(f.q.clone()))
                                .child(div().min_w(px(44.)).text_size(px(12.)).text_color(t.text_muted).flex().justify_center().child(count))
                                .child(btn("fd-prev", "chevron-up", "上一个 ⇧↵").on_click(cx.listener(|this, _, _, cx| this.find_step(-1, cx))))
                                .child(btn("fd-next", "chevron-down", "下一个 ↵").on_click(cx.listener(|this, _, _, cx| this.find_step(1, cx))))
                                .child(
                                    div()
                                        .id("fd-rep")
                                        .h(px(26.))
                                        .px(px(6.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .text_size(px(12.5))
                                        .when(f.replace, |d| d.bg(t.accent_soft).text_color(t.accent))
                                        .hover(|s| s.bg(t.hover))
                                        .child("替换")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            let mut focus = None;
                                            if let Some(f) = this.find.as_mut() {
                                                f.replace = !f.replace;
                                                focus = f.replace.then(|| f.r.clone());
                                            }
                                            if let Some(r) = focus {
                                                window.focus(&r.focus_handle(cx));
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(btn("fd-close", "x", "关闭 Esc").on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx)))),
                        )
                        .when(f.replace, |d| {
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .pl(px(19.))
                                    .child(field(f.r.clone()))
                                    .child(text_btn("fd-one", "替换", false).on_click(cx.listener(|this, _, _, cx| this.replace_one(cx))))
                                    .child(text_btn("fd-all", "全部替换", true).on_click(cx.listener(|this, _, _, cx| this.replace_all(cx)))),
                            )
                        }),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn find_is_case_insensitive_and_char_safe() {
        assert_eq!(super::find_all("流程 Flow flow", "FLOW"), vec![(7, 11), (12, 16)]);
        assert_eq!(super::find_all("开始开始", "开始"), vec![(0, 6), (6, 12)]);
        assert!(super::find_all("abc", "").is_empty());
    }
}
