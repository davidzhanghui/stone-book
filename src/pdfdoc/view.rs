//! PDF 阅读器全屏界面：顶栏（翻页 / 缩放 / 标注工具 / 搜索 / 导出）+ 原生 PDF 区域 + 右侧栏（批注 / 目录）。
//!
//! 原生视图叠在 GPUI 之上，所以弹层一律放在顶栏和侧栏里，不盖到 PDF 区域上。

use super::{host, Annot, AnnotKind, PdfDoc, COLORS};
use crate::line_input::{InputEvent, LineInput};
use crate::theme::{theme, Theme};
use crate::ui::{icon, Tooltip};
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub enum PdfEvent {
    Close,
    /// 把一条批注摘录到正文（引用块 + 回跳链接）
    Excerpt {
        annot: Annot,
    },
    /// 用一条批注做闪卡（正面摘录、背面批注内容）
    MakeCard {
        annot: Annot,
    },
}

impl EventEmitter<PdfEvent> for PdfReader {}

/// 便签输入：新建（None）或修改某条批注的内容（Some(id)）
struct NoteEdit {
    input: Entity<LineInput>,
    target: Option<String>,
    _sub: Subscription,
}

pub struct PdfReader {
    pub obj: String,
    focus: FocusHandle,
    doc: PdfDoc,
    /// 当前标注颜色
    color: u8,
    find: Entity<LineInput>,
    find_info: (usize, usize),
    /// 上次搜索的词（回车时词没变就跳下一个）
    last_q: String,
    filter: Entity<LineInput>,
    /// 侧栏颜色筛选
    filter_color: Option<u8>,
    note: Option<NoteEdit>,
    /// 侧栏：0 批注 1 目录
    tab: u8,
    outline: Option<Vec<(usize, String, usize)>>,
    page: (usize, usize),
    has_sel: bool,
    /// 首次显示后要定位到的批注
    pending: Rc<Cell<Option<(usize, (f32, f32, f32, f32))>>>,
    failed: Rc<Cell<bool>>,
    status: Option<(String, Instant)>,
    _subs: Vec<Subscription>,
    _poll: Task<()>,
}

impl PdfReader {
    /// `goto`：打开后定位到 (页, 批注 id)，来自摘录链接。
    pub fn new(obj: String, goto: Option<(usize, String)>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut doc = crate::objects::pdf(&obj).map(|d| (*d).clone()).unwrap_or_default();
        let mut pending = None;
        if let Some((page, aid)) = goto {
            doc.last_page = page;
            pending = doc.annot(&aid).and_then(|a| a.rects.first().map(|r| (a.page, *r)));
        }
        let find = cx.new(|cx| LineInput::new("搜索 PDF", cx));
        let filter = cx.new(|cx| LineInput::new("筛选批注", cx));
        let subs = vec![
            cx.subscribe_in(&find, window, |this, inp, ev: &InputEvent, window, cx| match ev {
                InputEvent::Submit { shift } => {
                    let q = inp.read(cx).text();
                    if this.find_info.1 == 0 || this.find_q_changed(&q) {
                        let n = host::find(&q);
                        this.find_info = (0, n);
                        this.last_q = q;
                    } else {
                        this.find_info = host::find_step(if *shift { -1 } else { 1 });
                    }
                    cx.notify();
                }
                InputEvent::Changed => {
                    if inp.read(cx).text().is_empty() {
                        host::find("");
                        this.find_info = (0, 0);
                        cx.notify();
                    }
                }
                InputEvent::Cancel => window.focus(&this.focus),
                _ => {}
            }),
            cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        let poll = cx.spawn(async move |this: WeakEntity<PdfReader>, cx: &mut AsyncApp| loop {
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let alive = this.update(cx, |this, cx| {
                let page = host::page_info().unwrap_or(this.page);
                let sel = host::selection().is_some();
                let stale = this.status.as_ref().is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(3));
                if page != this.page || sel != this.has_sel || stale {
                    this.page = page;
                    this.has_sel = sel;
                    if stale {
                        this.status = None;
                    }
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        });
        let focus = cx.focus_handle();
        PdfReader {
            obj,
            focus,
            doc,
            color: 0,
            find,
            find_info: (0, 0),
            filter,
            filter_color: None,
            note: None,
            tab: 0,
            outline: None,
            page: (0, 0),
            has_sel: false,
            pending: Rc::new(Cell::new(pending)),
            failed: Rc::new(Cell::new(false)),
            status: None,
            _subs: subs,
            _poll: poll,
            last_q: String::new(),
        }
    }

    fn find_q_changed(&self, q: &str) -> bool {
        self.last_q != q
    }

    fn say(&mut self, s: impl Into<String>, cx: &mut Context<Self>) {
        self.status = Some((s.into(), Instant::now()));
        cx.notify();
    }

    fn save(&mut self) {
        crate::objects::update(&self.obj, crate::objects::ObjData::Pdf(Arc::new(self.doc.clone())));
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = host::hide() {
            self.doc.last_page = p;
        }
        self.save();
        cx.emit(PdfEvent::Close);
    }

    /// 用当前选区新建批注（跨页时每页一条）。
    fn mark(&mut self, kind: AnnotKind, cx: &mut Context<Self>) {
        let Some((pages, text)) = host::selection() else {
            self.say("先在 PDF 上拖选文字", cx);
            return;
        };
        let now = crate::store::now_ms();
        for (i, (page, rects)) in pages.into_iter().enumerate() {
            let a = Annot {
                id: super::new_annot_id(),
                page,
                kind,
                color: self.color,
                rects,
                text: if i == 0 { text.clone() } else { String::new() },
                note: String::new(),
                created: now,
            };
            host::add_annot(&a);
            self.doc.annots.push(a);
        }
        host::clear_selection();
        self.has_sel = false;
        self.save();
        cx.notify();
    }

    fn open_note(&mut self, target: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let init = target.as_ref().and_then(|id| self.doc.annot(id)).map(|a| a.note.clone()).unwrap_or_default();
        let input = cx.new(|cx| {
            let mut i = LineInput::new(if target.is_some() { "写批注，回车保存" } else { "便签内容，回车添加" }, cx);
            i.set_text(&init, cx);
            i
        });
        let sub = cx.subscribe_in(&input, window, |this, inp, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => {
                let text = inp.read(cx).text();
                this.commit_note(text, cx);
                window.focus(&this.focus);
            }
            InputEvent::Cancel => {
                this.note = None;
                window.focus(&this.focus);
                cx.notify();
            }
            _ => {}
        });
        window.focus(&input.focus_handle(cx));
        self.note = Some(NoteEdit { input, target, _sub: sub });
        cx.notify();
    }

    fn commit_note(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(n) = self.note.take() else { return };
        match n.target {
            Some(id) => {
                if let Some(a) = self.doc.annots.iter_mut().find(|a| a.id == id) {
                    a.note = text;
                    host::refresh_annot(a);
                }
            }
            None if !text.trim().is_empty() => {
                let (page, rect, quote) = match host::selection() {
                    Some((pages, t)) => {
                        let (p, r) = &pages[0];
                        let last = r.last().copied().unwrap_or((0., 0., 0., 0.));
                        (*p, (last.0 + last.2, last.1 + last.3 - 4., 20., 20.), t)
                    }
                    None => match host::note_anchor() {
                        Some((p, r)) => (p, r, String::new()),
                        None => return,
                    },
                };
                let a = Annot {
                    id: super::new_annot_id(),
                    page,
                    kind: AnnotKind::Note,
                    color: self.color,
                    rects: vec![rect],
                    text: quote,
                    note: text,
                    created: crate::store::now_ms(),
                };
                host::add_annot(&a);
                host::clear_selection();
                self.doc.annots.push(a);
            }
            None => {}
        }
        self.save();
        cx.notify();
    }

    fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        host::remove_annot(id);
        self.doc.annots.retain(|a| a.id != id);
        self.save();
        cx.notify();
    }

    fn recolor(&mut self, id: &str, c: u8, cx: &mut Context<Self>) {
        if let Some(a) = self.doc.annots.iter_mut().find(|a| a.id == id) {
            a.color = c;
            host::refresh_annot(a);
        }
        self.save();
        cx.notify();
    }

    fn export_md(&mut self, cx: &mut Context<Self>) {
        if self.doc.annots.is_empty() {
            self.say("还没有批注", cx);
            return;
        }
        crate::clipboard::write(&self.doc.to_markdown(), None, None);
        self.say("批注已复制为 Markdown", cx);
    }

    fn export_pdf(&mut self, cx: &mut Context<Self>) {
        let dir = dirs_home().join("Desktop");
        let name = self.doc.name.trim_end_matches(".pdf").to_string() + "（批注）.pdf";
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this: WeakEntity<PdfReader>, cx: &mut AsyncApp| {
            if let Ok(Ok(Some(path))) = rx.await {
                this.update(cx, |this, cx| {
                    let ok = host::write_to(&path);
                    this.say(if ok { "已导出带批注的 PDF" } else { "导出失败" }, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn visible_annots(&self, cx: &App) -> Vec<Annot> {
        let q = self.filter.read(cx).text().to_lowercase();
        self.doc
            .sorted()
            .into_iter()
            .filter(|a| self.filter_color.is_none_or(|c| a.color == c))
            .filter(|a| q.is_empty() || a.text.to_lowercase().contains(&q) || a.note.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    fn tool_btn(id: &'static str, ic: &'static str, tip: &'static str, t: &Theme) -> Stateful<Div> {
        div()
            .id(id)
            .h(px(28.))
            .min_w(px(28.))
            .px(px(6.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
            .child(icon(ic, 15., t.text_secondary))
    }

    fn text_btn(id: &'static str, label: &'static str, t: &Theme) -> Stateful<Div> {
        div()
            .id(id)
            .h(px(28.))
            .px(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(5.))
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(t.text)
            .hover(|s| s.bg(t.hover))
            .child(crate::i18n::tr(label))
    }

    fn input_box(input: &Entity<LineInput>, w: f32, t: &Theme) -> Div {
        div()
            .w(px(w))
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border)
            .bg(t.bg)
            .text_size(px(13.))
            .child(input.clone())
    }

    fn render_toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let (cur, total) = self.page;
        let mark = |id: &'static str, label: &'static str, kind: AnnotKind| {
            Self::text_btn(id, label, t).on_click(cx.listener(move |this, _, _, cx| this.mark(kind, cx)))
        };
        div()
            .h(px(48.))
            .flex_none()
            // 左侧让出透明标题栏的红绿灯
            .pl(px(80.))
            .pr(px(12.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .border_b_1()
            .border_color(t.border)
            .bg(t.panel_bg)
            .child(
                div()
                    .id("pdf-done")
                    .h(px(28.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .bg(t.accent)
                    .text_color(gpui::white())
                    .text_size(px(13.))
                    .cursor_pointer()
                    .child(crate::i18n::tr("完成"))
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
            .child(
                div()
                    .max_w(px(220.))
                    .px(px(6.))
                    .text_size(px(14.))
                    .text_color(t.text)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.doc.name.clone()),
            )
            .child(Self::tool_btn("pdf-prev", "chevron-up", "上一页", t).on_click(|_, _, _| host::step_page(-1)))
            .child(div().text_size(px(12.)).text_color(t.text_muted).child(if total > 0 { format!("{} / {}", cur + 1, total) } else { String::new() }))
            .child(Self::tool_btn("pdf-next", "chevron-down", "下一页", t).on_click(|_, _, _| host::step_page(1)))
            .child(Self::tool_btn("pdf-zo", "zoom-out", "缩小", t).on_click(|_, _, _| host::zoom(-1)))
            .child(Self::tool_btn("pdf-fit", "fit-page", "适应宽度", t).on_click(|_, _, _| host::zoom(0)))
            .child(Self::tool_btn("pdf-zi", "zoom-in", "放大", t).on_click(|_, _, _| host::zoom(1)))
            .child(Self::tool_btn("pdf-thumbs", "sidebar", "缩略图", t).on_click(|_, _, _| host::toggle_thumbs()))
            .child(div().w(px(1.)).h(px(20.)).mx(px(4.)).bg(t.border))
            .children(COLORS.iter().enumerate().map(|(i, (name, c))| {
                let on = self.color as usize == i;
                div()
                    .id(("pdf-color", i))
                    .size(px(20.))
                    .flex_none()
                    .rounded_full()
                    .border_2()
                    .border_color(if on { t.text } else { gpui::transparent_black() })
                    .bg(rgb(*c))
                    .cursor_pointer()
                    .tooltip(move |_, cx| cx.new(|_| Tooltip((*name).into())).into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.color = i as u8;
                        cx.notify();
                    }))
            }))
            .child(mark("pdf-hl", "高亮", AnnotKind::Highlight))
            .child(mark("pdf-ul", "下划线", AnnotKind::Underline))
            .child(mark("pdf-st", "删除线", AnnotKind::Strike))
            .child(Self::text_btn("pdf-note", "便签", t).on_click(cx.listener(|this, _, window, cx| this.open_note(None, window, cx))))
            .when_some(self.note.as_ref(), |d, n| d.child(Self::input_box(&n.input, 200., t)))
            .child(div().flex_1())
            .when_some(self.status.as_ref(), |d, (s, _)| d.child(div().text_size(px(12.)).text_color(t.accent).child(s.clone())))
            .child(Self::input_box(&self.find, 160., t))
            .when(self.find_info.1 > 0, |d| {
                d.child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("{}/{}", self.find_info.0 + 1, self.find_info.1)))
            })
            .child(Self::tool_btn("pdf-fprev", "arrow-up", "上一个", t).on_click(cx.listener(|this, _, _, cx| {
                this.find_info = host::find_step(-1);
                cx.notify();
            })))
            .child(Self::tool_btn("pdf-fnext", "arrow-down", "下一个", t).on_click(cx.listener(|this, _, _, cx| {
                this.find_info = host::find_step(1);
                cx.notify();
            })))
            .child(Self::tool_btn("pdf-md", "copy", "复制全部批注（Markdown）", t).on_click(cx.listener(|this, _, _, cx| this.export_md(cx))))
            .child(Self::tool_btn("pdf-export", "download", "导出带批注的 PDF…", t).on_click(cx.listener(|this, _, _, cx| this.export_pdf(cx))))
    }

    fn render_sidebar(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = |id: &'static str, label: &'static str, k: u8, on: bool| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(6.))
                .text_size(px(13.))
                .cursor_pointer()
                .border_b_2()
                .border_color(if on { t.accent } else { gpui::transparent_black() })
                .text_color(if on { t.text } else { t.text_muted })
                .child(crate::i18n::tr(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = k;
                    cx.notify();
                }))
        };
        let head = div()
            .flex()
            .flex_row()
            .gap(px(4.))
            .px(px(10.))
            .border_b_1()
            .border_color(t.border)
            .child(tab("pdf-tab-a", "批注", 0, self.tab == 0))
            .child(tab("pdf-tab-o", "目录", 1, self.tab == 1))
            .child(div().flex_1())
            .child(div().py(px(8.)).text_size(px(12.)).text_color(t.text_muted).child(format!("{}", self.doc.annots.len())));
        let body: AnyElement = if self.tab == 1 {
            let items = self.outline.get_or_insert_with(host::outline).clone();
            div()
                .id("pdf-outline")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .py(px(6.))
                .when(items.is_empty(), |d| d.child(div().p(px(16.)).text_size(px(13.)).text_color(t.text_muted).child(crate::i18n::tr("这份 PDF 没有目录"))))
                .children(items.into_iter().enumerate().map(|(i, (depth, label, page))| {
                    div()
                        .id(("pdf-ol", i))
                        .flex()
                        .flex_row()
                        .gap(px(6.))
                        .pl(px(12. + depth as f32 * 14.))
                        .pr(px(10.))
                        .py(px(5.))
                        .text_size(px(13.))
                        .text_color(t.text)
                        .cursor_pointer()
                        .hover(|s| s.bg(t.hover))
                        .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
                        .child(div().text_color(t.text_muted).text_size(px(11.)).child(format!("{}", page + 1)))
                        .on_click(move |_, _, _| host::go_to(page, None))
                }))
                .into_any_element()
        } else {
            let list = self.visible_annots(cx);
            let chip = |id: usize, c: Option<u8>, on: bool| {
                div()
                    .id(("pdf-fc", id))
                    .size(px(18.))
                    .flex_none()
                    .rounded_full()
                    .border_2()
                    .border_color(if on { t.text } else { t.border })
                    .when_some(c, |d, c| d.bg(rgb(COLORS[c as usize].1)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.filter_color = c;
                        cx.notify();
                    }))
            };
            div()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .p(px(10.))
                        .child(Self::input_box(&self.filter, 120., t))
                        .child(chip(9, None, self.filter_color.is_none()))
                        .children((0..COLORS.len() as u8).map(|c| chip(c as usize, Some(c), self.filter_color == Some(c)))),
                )
                .child(
                    div()
                        .id("pdf-annots")
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_y_scroll()
                        .px(px(10.))
                        .pb(px(10.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .when(list.is_empty(), |d| {
                            d.child(
                                div()
                                    .p(px(8.))
                                    .text_size(px(13.))
                                    .text_color(t.text_muted)
                                    .child(crate::i18n::tr("在 PDF 上拖选文字，再点顶栏的「高亮」「下划线」等即可添加批注。")),
                            )
                        })
                        .children(list.into_iter().enumerate().map(|(i, a)| self.render_annot(i, a, t, cx))),
                )
                .into_any_element()
        };
        div().w(px(300.)).flex_none().flex().flex_col().border_l_1().border_color(t.border).bg(t.panel_bg).child(head).child(body)
    }

    fn render_annot(&self, i: usize, a: Annot, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let col = rgb(COLORS[a.color as usize % COLORS.len()].1);
        let (id1, id2, id3) = (a.id.clone(), a.id.clone(), a.id.clone());
        let goto = (a.page, a.rects.first().copied());
        let excerpt = a.clone();
        let card = a.clone();
        let small = |id: (&'static str, usize), ic: &'static str, tip: &'static str| {
            div()
                .id(id)
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                .child(icon(ic, 13., t.text_muted))
        };
        div()
            .id(("pdf-a", i))
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(10.))
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.bg)
            .cursor_pointer()
            .hover(|s| s.border_color(t.accent))
            .on_click(move |_, _, _| host::go_to(goto.0, goto.1))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(div().size(px(10.)).rounded_full().bg(col))
                    .child(div().flex_1().text_size(px(11.)).text_color(t.text_muted).child(format!(
                        "第 {} 页 · {}",
                        a.page + 1,
                        crate::i18n::tr(a.kind.label())
                    )))
                    .children((0..COLORS.len() as u8).filter(|c| *c != a.color).map(|c| {
                        let id = id1.clone();
                        div()
                            .id(SharedString::from(format!("pdf-rc-{i}-{c}")))
                            .size(px(10.))
                            .rounded_full()
                            .bg(rgb(COLORS[c as usize].1))
                            .opacity(0.6)
                            .cursor_pointer()
                            .hover(|s| s.opacity(1.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.recolor(&id, c, cx);
                            }))
                    }))
                    .child(small(("pdf-ex", i), "arrow-down-to-line", "摘录到正文").on_click(cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        cx.emit(PdfEvent::Excerpt { annot: excerpt.clone() });
                    })))
                    .child(small(("pdf-card", i), "book", "制成闪卡").on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        cx.emit(PdfEvent::MakeCard { annot: card.clone() });
                        this.say("已在正文 PDF 块下方生成闪卡", cx);
                    })))
                    .child(small(("pdf-ed", i), "pencil", "写批注").on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_note(Some(id2.clone()), window, cx);
                    })))
                    .child(small(("pdf-del", i), "trash", "删除").on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.delete(&id3, cx);
                    }))),
            )
            .when(!a.text.is_empty(), |d| {
                d.child(
                    div()
                        .pl(px(8.))
                        .border_l_2()
                        .border_color(col)
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .text_color(t.text)
                        .line_clamp(4)
                        .child(a.text.clone()),
                )
            })
            .when(!a.note.is_empty(), |d| d.child(div().text_size(px(13.)).line_height(px(20.)).text_color(t.text_secondary).child(a.note.clone())))
    }
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(|| "/".into())
}

impl Focusable for PdfReader {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PdfReader {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx).clone();
        let path = std::path::PathBuf::from(&self.doc.path);
        let missing = !path.exists() || self.failed.get();
        let (obj, annots, start, dark) = (self.obj.clone(), Arc::new(self.doc.annots.clone()), self.doc.last_page, t.dark);
        let (pending, failed) = (self.pending.clone(), self.failed.clone());
        let area: AnyElement = if missing {
            host::hide();
            div().flex_1().flex().items_center().justify_center().text_color(t.text_muted).child(crate::i18n::tr("PDF 文件不存在或无法打开")).into_any_element()
        } else {
            canvas(
                |b, _, _| b,
                move |b, _, window, _| {
                    if !host::show(window, &obj, &path, b, &annots, start, dark) {
                        failed.set(true);
                        return;
                    }
                    if let Some((page, r)) = pending.take() {
                        host::go_to(page, Some(r));
                    }
                },
            )
            .flex_1()
            .h_full()
            .into_any_element()
        };
        div()
            .id("pdf-reader")
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                match k.key.as_str() {
                    "escape" if this.note.is_some() => {
                        this.note = None;
                        cx.notify();
                    }
                    "escape" => this.close(cx),
                    "f" if k.modifiers.platform => window.focus(&this.find.focus_handle(cx)),
                    "right" | "pagedown" => host::step_page(1),
                    "left" | "pageup" => host::step_page(-1),
                    _ => {}
                }
            }))
            .child(self.render_toolbar(&t, cx))
            .child(div().flex_1().min_h(px(0.)).flex().flex_row().child(area).child(self.render_sidebar(&t, cx)))
    }
}

impl Drop for PdfReader {
    fn drop(&mut self) {
        host::hide();
    }
}
