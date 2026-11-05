//! 演示模式（F5）：按二级及以上标题分页。
//!
//! - 幻灯片渲染覆盖正文里的大部分块：行内样式、表格、分栏、高亮块、时间轴、嵌入对象（画板 / 电子表格 / 多维表格）、附件卡片；
//! - 「演讲者备注」块不出现在幻灯片上，按 `S` 切到演讲者视图查看（当前页 + 下一页 + 备注 + 计时）；
//! - `L` 激光笔、`B` 黑屏、`T` 显示 / 隐藏计时。

use super::*;
use crate::canvas::paint::{paint_scene, View};
use crate::model::{Block, RichText};
use chrono::Local;
use std::sync::Arc;
use std::time::Instant;

/// 演示模式的一页：标题 + 内容块 + 演讲者备注
pub struct Slide {
    title: String,
    blocks: Vec<Arc<Block>>,
    cover: bool,
    notes: Vec<RichText>,
}

impl Slide {
    fn new(title: String, cover: bool) -> Self {
        Slide { title, blocks: vec![], cover, notes: vec![] }
    }
}

/// 演示时的界面状态（不写进文档）。
#[derive(Default)]
pub struct PresentUi {
    /// 演讲者视图
    presenter: bool,
    /// 激光笔开着时的指针位置
    laser: Option<Option<Point<Pixels>>>,
    black: bool,
    hide_timer: bool,
    started: Option<Instant>,
    _tick: Option<Task<()>>,
}

pub(super) fn build_slides(content: &DocContent) -> Vec<Slide> {
    let mut slides = vec![Slide::new(content.title(), true)];
    let mut cur: Option<Slide> = None;
    for b in content.blocks.iter().skip(1) {
        match b.kind {
            BlockKind::Heading { level } if level <= 2 => {
                if let Some(s) = cur.take() {
                    slides.push(s);
                }
                cur = Some(Slide::new(b.text.text.clone(), false));
            }
            BlockKind::Notes => {
                if b.text.is_empty() {
                    continue;
                }
                match cur.as_mut() {
                    Some(s) => s.notes.push(b.text.clone()),
                    None => slides[0].notes.push(b.text.clone()),
                }
            }
            _ => {
                let s = cur.get_or_insert_with(|| Slide::new(String::new(), false));
                // 没有标题分隔的长文：每 10 块一页
                if s.blocks.len() >= 10 && s.title.is_empty() {
                    let full = cur.take().unwrap();
                    slides.push(full);
                    let mut next = Slide::new(String::new(), false);
                    next.blocks.push(b.clone());
                    cur = Some(next);
                } else if !(matches!(b.kind, BlockKind::Text) && b.text.is_empty()) {
                    s.blocks.push(b.clone());
                }
            }
        }
    }
    if let Some(s) = cur {
        slides.push(s);
    }
    // 封面页用文档开头（第一个标题前）的文字做副标题
    if slides.len() > 1 && slides[1].title.is_empty() && slides[1].blocks.len() <= 2 {
        let intro = slides.remove(1);
        slides[0].blocks = intro.blocks;
        slides[0].notes.extend(intro.notes);
    }
    slides
}

/// 富文本 → 带样式的文字（粗斜体、颜色、背景、链接、行内代码；行内公式显示为 `$源码$`）。
pub(super) fn rich(rt: &RichText, t: &Theme) -> StyledText {
    let mut out = String::new();
    let mut hl = Vec::new();
    for (r, m) in rt.runs() {
        let start = out.len();
        match &m.math {
            Some(tex) => {
                for _ in 0..(r.end - r.start) / 3 {
                    out.push_str(&format!("${tex}$"));
                }
            }
            None => out.push_str(&rt.text[r]),
        }
        let end = out.len();
        if m.is_plain() || start == end {
            continue;
        }
        let mut h = HighlightStyle::default();
        if m.bold {
            h.font_weight = Some(FontWeight::BOLD);
        }
        if m.italic {
            h.font_style = Some(FontStyle::Italic);
        }
        if m.link.is_some() {
            h.color = Some(t.link);
            h.underline = Some(UnderlineStyle { color: Some(t.link), thickness: px(1.), wavy: false });
        } else if m.color > 0 {
            h.color = Some(t.fg_palette[(m.color & 7) as usize]);
        }
        if m.code {
            h.background_color = Some(t.inline_code_bg);
            if h.color.is_none() {
                h.color = Some(t.inline_code_fg);
            }
        }
        if m.bg > 0 {
            h.background_color = Some(t.text_bg(m.bg));
        }
        if m.underline && h.underline.is_none() {
            h.underline = Some(UnderlineStyle { color: None, thickness: px(1.), wavy: false });
        }
        if m.strike {
            h.strikethrough = Some(StrikethroughStyle { color: None, thickness: px(1.) });
        }
        hl.push((start..end, h));
    }
    StyledText::new(out).with_highlights(hl)
}

/// 简单表格：第一行可作为标题行。
fn grid(rows: Vec<Vec<AnyElement>>, header: bool, k: f32, t: &Theme) -> AnyElement {
    let n = rows.len();
    div()
        .flex()
        .flex_col()
        .w_full()
        .border_1()
        .border_color(t.border)
        .rounded(px(6. * k))
        .overflow_hidden()
        .text_size(px(20. * k))
        .line_height(px(30. * k))
        .children(rows.into_iter().enumerate().map(|(r, cells)| {
            div()
                .flex()
                .flex_row()
                .w_full()
                .when(r + 1 < n, |d| d.border_b_1().border_color(t.border))
                .when(header && r == 0, |d| d.bg(t.code_bg).font_weight(FontWeight::SEMIBOLD))
                .children(cells.into_iter().enumerate().map(|(c, cell)| {
                    div().flex_1().min_w(px(0.)).px(px(12. * k)).py(px(6. * k)).when(c > 0, |d| d.border_l_1().border_color(t.border)).child(cell)
                }))
        }))
        .into_any_element()
}

fn text_rows(rows: Vec<Vec<String>>) -> Vec<Vec<AnyElement>> {
    rows.into_iter().map(|r| r.into_iter().map(|s| s.into_any_element()).collect()).collect()
}

/// Markdown 表格（多维表格导出）→ 行列文字。
fn md_table(md: &str) -> Vec<Vec<String>> {
    md.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('|') && !l.trim_matches(|c: char| c == '|' || c == '-' || c == ':' || c.is_whitespace()).is_empty())
        .map(|l| l.trim_matches('|').split(" | ").map(|c| c.trim().replace("\\|", "|").replace("<br>", " ")).collect())
        .collect()
}

/// 画板场景按 (最大宽, 最大高) 等比缩放后绘制。
fn scene_el(scene: crate::canvas::Scene, max_w: f32, max_h: f32) -> Option<AnyElement> {
    let (w, h) = (scene.width.max(1.), scene.height.max(1.));
    let s = (max_w / w).min(max_h / h).min(1.6);
    let scene = Arc::new(scene);
    Some(
        div()
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(w * s))
                    .h(px(h * s))
                    .child(canvas(|b, _, _| b, move |b, _, window, cx| paint_scene(&scene, View { origin: b.origin, scale: s }, b, window, cx)).size_full()),
            )
            .into_any_element(),
    )
}

fn card(ic: &'static str, label: String, k: f32, t: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12. * k))
        .px(px(18. * k))
        .py(px(12. * k))
        .rounded(px(10. * k))
        .border_1()
        .border_color(t.border)
        .text_size(px(22. * k))
        .text_color(t.text)
        .child(crate::ui::icon(ic, 22. * k, t.text_secondary))
        .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
        .into_any_element()
}

/// 一个内容块在幻灯片上的样子；k 为字号缩放（演讲者视图里缩小显示）。
fn slide_block(b: &Block, t: &Theme, k: f32, window: &mut Window) -> Option<AnyElement> {
    let indent = px(b.indent as f32 * 28. * k);
    let line = |prefix: String, body: StyledText, size: f32, col: Hsla| {
        div()
            .flex()
            .flex_row()
            .gap(px(12. * k))
            .pl(indent)
            .text_size(px(size * k))
            .line_height(px(size * 1.55 * k))
            .text_color(col)
            .when(!prefix.is_empty(), |d| d.child(div().flex_none().text_color(t.accent).child(prefix)))
            .child(div().flex_1().min_w(px(0.)).child(body))
            .into_any_element()
    };
    let align = |d: Div| match b.align {
        1 => d.text_center(),
        2 => d.text_right(),
        _ => d,
    };
    Some(match &b.kind {
        BlockKind::Text => align(div()).child(line(String::new(), rich(&b.text, t), 26., t.text)).into_any_element(),
        BlockKind::Heading { .. } => {
            align(div().pt(px(8. * k)).text_size(px(30. * k)).font_weight(FontWeight::SEMIBOLD).text_color(t.text)).child(rich(&b.text, t)).into_any_element()
        }
        BlockKind::Bullet => line("•".into(), rich(&b.text, t), 26., t.text),
        BlockKind::Ordered => line("›".into(), rich(&b.text, t), 26., t.text),
        BlockKind::Todo { checked } => {
            line(if *checked { "☑".into() } else { "☐".into() }, rich(&b.text, t), 26., if *checked { t.text_muted } else { t.text })
        }
        BlockKind::Quote => div()
            .pl(px(20. * k))
            .border_l_4()
            .border_color(t.accent)
            .text_size(px(24. * k))
            .line_height(px(38. * k))
            .text_color(t.text_secondary)
            .child(rich(&b.text, t))
            .into_any_element(),
        BlockKind::Callout { emoji, color } => {
            let ci = (*color & 7) as usize;
            div()
                .flex()
                .flex_row()
                .gap(px(14. * k))
                .px(px(22. * k))
                .py(px(16. * k))
                .rounded(px(12. * k))
                .bg(t.callout_bg[ci])
                .border_1()
                .border_color(t.callout_border[ci])
                .text_size(px(24. * k))
                .line_height(px(38. * k))
                .text_color(t.text)
                .child(div().flex_none().child(emoji.clone()))
                .child(div().flex_1().min_w(px(0.)).child(rich(&b.text, t)))
                .into_any_element()
        }
        BlockKind::Code { .. } => div()
            .p(px(16. * k))
            .rounded(px(10. * k))
            .bg(t.code_bg)
            .font_family("Menlo")
            .text_size(px(18. * k))
            .line_height(px(28. * k))
            .text_color(t.text)
            .whitespace_normal()
            .child(b.text.text.clone())
            .into_any_element(),
        BlockKind::Image { path, .. } => {
            div().flex().justify_center().child(img(std::path::PathBuf::from(path)).max_h(px(460. * k)).max_w_full().rounded(px(8.))).into_any_element()
        }
        BlockKind::Math => {
            let p = crate::math::cached_path(&b.text.text, true)?;
            div().flex().justify_center().child(img(p).max_h(px(160. * k))).into_any_element()
        }
        BlockKind::Diagram { offsets, .. } => {
            let mut m = crate::editor::GpuiMeasure { window };
            let scene = crate::diagram::render(&b.text.text, t.dark, None, &mut m, offsets).ok()?;
            scene_el(scene, 1100. * k, 440. * k)?
        }
        BlockKind::Divider => div().h(px(2.)).my(px(8. * k)).bg(t.border).into_any_element(),
        BlockKind::Table { rows, cols, cells, header_row, .. } => {
            let cols = (*cols).max(1);
            let rows: Vec<Vec<AnyElement>> = (0..*rows).map(|r| (0..cols).map(|c| rich(&cells[r * cols + c], t).into_any_element()).collect()).collect();
            grid(rows, *header_row, k, t)
        }
        BlockKind::Columns { cols } => div()
            .flex()
            .flex_row()
            .gap(px(32. * k))
            .text_size(px(24. * k))
            .line_height(px(38. * k))
            .children(cols.iter().map(|c| div().flex_1().min_w(px(0.)).child(rich(c, t))))
            .into_any_element(),
        BlockKind::Timeline { items, .. } => div()
            .flex()
            .flex_col()
            .gap(px(10. * k))
            .children(items.iter().map(|it| {
                div()
                    .flex()
                    .flex_row()
                    .gap(px(16. * k))
                    .text_size(px(22. * k))
                    .line_height(px(34. * k))
                    .child(div().w(px(140. * k)).flex_none().text_color(t.accent).child(rich(&it.time, t)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(rich(&it.title, t)))
                            .when(!it.detail.is_empty(), |d| d.child(div().text_color(t.text_secondary).text_size(px(19. * k)).child(rich(&it.detail, t)))),
                    )
            }))
            .into_any_element(),
        BlockKind::Embed { kind, id } => match crate::objects::get(id)? {
            crate::objects::ObjData::Board(bd) => {
                let mut scene = bd.to_scene(t.dark);
                scene.fit(24.);
                if scene.items.is_empty() {
                    return None;
                }
                scene_el(scene, 1100. * k, 460. * k)?
            }
            crate::objects::ObjData::Sheet(s) => {
                let vals = crate::objects::sheet_values(&s, 14, 10);
                if vals.is_empty() {
                    return None;
                }
                grid(text_rows(vals), true, k, t)
            }
            crate::objects::ObjData::Base(_) => {
                let mut rows = md_table(&crate::objects::to_markdown(kind, id));
                if rows.is_empty() {
                    return None;
                }
                rows.truncate(14);
                grid(text_rows(rows), true, k, t)
            }
            crate::objects::ObjData::Pdf(p) => card("file", p.name.clone(), k, t),
        },
        BlockKind::File { name, .. } => card(if crate::editor::is_image_name(name) { "image" } else { "paperclip" }, name.clone(), k, t),
        BlockKind::Video { name, .. } => card("video", name.clone(), k, t),
        BlockKind::LinkCard { url } => card("globe", url.clone(), k, t),
        BlockKind::Button { .. } if !b.text.is_empty() => div()
            .flex()
            .child(
                div()
                    .px(px(18. * k))
                    .py(px(8. * k))
                    .rounded(px(8. * k))
                    .bg(t.accent)
                    .text_color(gpui::white())
                    .text_size(px(20. * k))
                    .child(b.text.text.clone()),
            )
            .into_any_element(),
        _ => return None,
    })
}

fn fmt_elapsed(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

impl Workspace {
    pub fn present(&mut self, _: &Present, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.save_now(cx));
        let slides = build_slides(&self.editor.read(cx).content);
        self.slides = Some((slides, 0));
        // 计时器每秒刷新一次（退出演示后任务自然结束）
        let tick = cx.spawn(async move |this: WeakEntity<Workspace>, cx: &mut AsyncApp| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
            let alive = this.update(cx, |this, cx| {
                if this.slides.is_some() && !this.present_ui.hide_timer {
                    cx.notify();
                }
                this.slides.is_some()
            });
            if !alive.unwrap_or(false) {
                break;
            }
        });
        self.present_ui = PresentUi { started: Some(Instant::now()), _tick: Some(tick), ..Default::default() };
        window.focus(&self.present_focus);
        if !window.is_fullscreen() {
            window.toggle_fullscreen();
            self.present_fullscreen = true;
        }
        cx.notify();
    }

    fn exit_present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.slides = None;
        self.present_ui = PresentUi::default();
        if self.present_fullscreen && window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        self.present_fullscreen = false;
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }

    pub fn slide_step(&mut self, d: i32, cx: &mut Context<Self>) {
        if let Some((s, i)) = &mut self.slides {
            *i = (*i as i32 + d).clamp(0, s.len() as i32 - 1) as usize;
        }
        self.present_ui.black = false;
        cx.notify();
    }

    /// 一页幻灯片的内容区（k 为缩放）。
    fn slide_body(slide: &Slide, k: f32, t: &Theme, window: &mut Window) -> Div {
        let body: Vec<AnyElement> = slide.blocks.iter().filter_map(|b| slide_block(b, t, k, window)).collect();
        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .px(px(120. * k))
            .py(px(60. * k))
            .gap(px(16. * k))
            .overflow_hidden()
            .when(slide.cover, |d| {
                d.items_center()
                    .child(div().text_size(px(64. * k)).font_weight(FontWeight::BOLD).text_center().child(if slide.title.is_empty() {
                        "未命名文档".to_string()
                    } else {
                        slide.title.clone()
                    }))
                    .child(div().w(px(80. * k)).h(px(4. * k)).rounded(px(2.)).bg(t.accent))
                    .children(slide.blocks.iter().map(|b| div().text_size(px(26. * k)).text_color(t.text_secondary).text_center().child(rich(&b.text, t))))
                    .child(div().mt(px(16. * k)).text_size(px(16. * k)).text_color(t.text_muted).child(Local::now().format("%Y 年 %m 月 %d 日").to_string()))
            })
            .when(!slide.cover, |d| {
                d.when(!slide.title.is_empty(), |d| {
                    d.child(div().text_size(px(46. * k)).font_weight(FontWeight::BOLD).child(slide.title.clone()))
                        .child(div().w(px(56. * k)).h(px(4. * k)).mb(px(16. * k)).rounded(px(2.)).bg(t.accent))
                })
                .children(body)
            })
    }

    /// 演讲者视图：左边当前页，右边计时、下一页、备注。
    fn presenter_view(&self, slides: &[Slide], i: usize, t: &Theme, window: &mut Window) -> AnyElement {
        let vp = window.viewport_size();
        let cur_w = f32::from(vp.width) * 0.6;
        let k = (cur_w / f32::from(vp.width)).min(0.62);
        let elapsed = self.present_ui.started.map_or(0, |s| s.elapsed().as_secs());
        let next = slides.get(i + 1);
        let notes = &slides[i].notes;
        let label = |s: &'static str| div().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr(s));
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_row()
            .gap(px(24.))
            .p(px(24.))
            .child(
                div().w(px(cur_w)).flex_none().flex().flex_col().gap(px(8.)).child(label("当前页")).child(
                    div()
                        .w_full()
                        .h(px(cur_w * 9. / 16.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(t.border)
                        .overflow_hidden()
                        .child(Self::slide_body(&slides[i], k, t, window)),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_end()
                            .gap(px(16.))
                            .child(div().text_size(px(44.)).font_weight(FontWeight::SEMIBOLD).font_family("Menlo").child(fmt_elapsed(elapsed)))
                            .child(div().pb(px(8.)).text_size(px(16.)).text_color(t.text_muted).child(Local::now().format("%H:%M").to_string())),
                    )
                    .child(label("下一页"))
                    .child(
                        div()
                            .px(px(14.))
                            .py(px(10.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(t.border)
                            .text_size(px(18.))
                            .text_color(if next.is_some() { t.text } else { t.text_muted })
                            .child(match next {
                                Some(s) if !s.title.is_empty() => s.title.clone(),
                                Some(s) => s.blocks.first().map(|b| b.plain_text()).unwrap_or_default().chars().take(40).collect(),
                                None => crate::i18n::tr("（已是最后一页）").to_string(),
                            }),
                    )
                    .child(label("演讲者备注"))
                    .child(
                        div()
                            .id("present-notes")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .text_size(px(22.))
                            .line_height(px(34.))
                            .text_color(t.text)
                            .when(notes.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_color(t.text_muted)
                                        .text_size(px(15.))
                                        .child(crate::i18n::tr("这一页没有备注。在标题下插入「演讲者备注」块即可添加。")),
                                )
                            })
                            .children(notes.iter().map(|n| div().child(rich(n, t)))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_present(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (slides, i) = self.slides.as_ref()?;
        let t = if theme(cx).dark { theme(cx).clone() } else { Theme::dark() };
        let slide = slides.get(*i)?;
        let n = slides.len();
        let ui = &self.present_ui;
        let main: AnyElement = if ui.black {
            div().flex_1().bg(gpui::black()).into_any_element()
        } else if ui.presenter {
            self.presenter_view(slides, *i, &t, window)
        } else {
            div().flex_1().min_h(px(0.)).child(Self::slide_body(slide, 1., &t, window)).into_any_element()
        };
        let laser = ui.laser.flatten();
        let hint = if ui.presenter {
            "← → 翻页 · S 返回幻灯片 · Esc 退出"
        } else {
            "← → 翻页 · S 演讲者视图 · L 激光笔 · B 黑屏 · T 计时 · Esc 退出"
        };
        let elapsed = ui.started.map_or(0, |s| s.elapsed().as_secs());
        Some(
            div()
                .id("present")
                .absolute()
                .inset_0()
                .occlude()
                .track_focus(&self.present_focus)
                .bg(if ui.black { gpui::black() } else { t.bg })
                .text_color(t.text)
                .font_family("PingFang SC")
                .flex()
                .flex_col()
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| match ev.keystroke.key.as_str() {
                    "right" | "down" | "space" | "pagedown" | "enter" => this.slide_step(1, cx),
                    "left" | "up" | "pageup" | "backspace" => this.slide_step(-1, cx),
                    "home" => this.slide_step(-1000, cx),
                    "end" => this.slide_step(1000, cx),
                    "s" => {
                        this.present_ui.presenter = !this.present_ui.presenter;
                        cx.notify();
                    }
                    "l" => {
                        this.present_ui.laser = if this.present_ui.laser.is_some() { None } else { Some(None) };
                        cx.notify();
                    }
                    "b" | "." => {
                        this.present_ui.black = !this.present_ui.black;
                        cx.notify();
                    }
                    "t" => {
                        this.present_ui.hide_timer = !this.present_ui.hide_timer;
                        cx.notify();
                    }
                    "escape" | "f5" => this.exit_present(window, cx),
                    _ => {}
                }))
                .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                    if this.present_ui.laser.is_some() {
                        this.present_ui.laser = Some(Some(ev.position));
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                        // 激光笔开着时点击不翻页，免得指点时误翻
                        if this.present_ui.laser.is_some() {
                            return;
                        }
                        let w = window.viewport_size().width;
                        this.slide_step(if ev.position.x < w / 4. { -1 } else { 1 }, cx);
                    }),
                )
                .child(main)
                .when(!ui.black, |d| {
                    d.child(
                        div()
                            .h(px(48.))
                            .px(px(40.))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .text_size(px(13.))
                            .text_color(t.text_muted)
                            .child(crate::i18n::tr(hint))
                            .child(
                                div().flex().flex_row().gap(px(20.)).when(!ui.hide_timer && !ui.presenter, |d| d.child(fmt_elapsed(elapsed))).child(format!(
                                    "{} / {}",
                                    i + 1,
                                    n
                                )),
                            ),
                    )
                    .child(div().h(px(3.)).w(relative((*i + 1) as f32 / n as f32)).bg(t.accent))
                })
                .when_some(laser, |d, p| {
                    d.child(div().absolute().left(p.x - px(9.)).top(p.y - px(9.)).size(px(18.)).rounded_full().bg(rgb(0xff2d2d)).shadow(vec![BoxShadow {
                        color: hsla(0., 1., 0.55, 0.7),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(16.),
                        spread_radius: px(4.),
                    }]))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    // 不用 `super::*`：gpui 导出的 `test` 宏会顶替标准库的 #[test]
    use super::{build_slides, fmt_elapsed, md_table};
    use crate::model::{Block, BlockKind, DocContent, RichText};
    use std::sync::Arc;

    fn doc(blocks: Vec<Block>) -> DocContent {
        let mut all = vec![Arc::new(Block::new(BlockKind::Title, RichText::plain("标题")))];
        all.extend(blocks.into_iter().map(Arc::new));
        DocContent { blocks: all }
    }

    #[test]
    fn notes_stay_off_slides() {
        let d = doc(vec![
            Block::new(BlockKind::Notes, RichText::plain("封面备注")),
            Block::new(BlockKind::Heading { level: 2 }, RichText::plain("第一节")),
            Block::text("正文"),
            Block::new(BlockKind::Notes, RichText::plain("第一节备注")),
            Block::new(BlockKind::Heading { level: 2 }, RichText::plain("第二节")),
        ]);
        let s = build_slides(&d);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].notes[0].text, "封面备注");
        assert_eq!(s[1].blocks.len(), 1);
        assert_eq!(s[1].notes[0].text, "第一节备注");
        assert!(s[2].notes.is_empty());
    }

    #[test]
    fn md_table_rows() {
        let rows = md_table("| 名称 | 状态 |\n| --- | --- |\n| a\\|b | 完成 |\n");
        assert_eq!(rows, vec![vec!["名称".to_string(), "状态".into()], vec!["a|b".into(), "完成".into()]]);
    }

    #[test]
    fn elapsed_format() {
        assert_eq!(fmt_elapsed(65), "01:05");
        assert_eq!(fmt_elapsed(3725), "1:02:05");
    }
}
