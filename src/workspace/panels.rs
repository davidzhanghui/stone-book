//! 工作区的几个全局面板：设置（⌘,）、命令面板（⇧⌘P）、演示模式（F5）、PDF 导出、写作统计。

use super::*;
use crate::canvas::paint::{paint_scene, View};
use crate::line_input::{InputEvent, LineInput};
use crate::model::Block;
use chrono::{Datelike, Duration as CDuration, Local, NaiveDate};
use std::sync::Arc;

type CmdFn = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);

/// 命令面板条目：(名称, 快捷键, 搜索关键词, 执行)
const COMMANDS: &[(&str, &str, &str, CmdFn)] = &[
    ("新建文档", "⌘N", "new xinjian wendang", |_, w, cx| w.dispatch_action(Box::new(NewDoc), cx)),
    ("新建子文档", "⇧⌘N", "sub zi wendang", |_, w, cx| w.dispatch_action(Box::new(NewSubDoc), cx)),
    ("从模板新建…", "", "template moban", |ws, _, cx| ws.open_templates(true, cx)),
    ("另存为模板", "", "save template moban lingcun", |ws, _, cx| ws.save_as_template(cx)),
    ("快速打开 / 全文搜索", "⌘P", "open search sousuo dakai", |_, w, cx| w.dispatch_action(Box::new(QuickOpen), cx)),
    ("演示模式", "F5", "present slides yanshi huandeng ppt", |ws, w, cx| ws.present(&Present, w, cx)),
    ("切换阅读 / 编辑模式", "⌥⌘R", "read only reading yuedu bianji zhidu", |_, w, cx| w.dispatch_action(Box::new(ToggleReadMode), cx)),
    ("以思维导图查看", "", "mindmap siwei daotu", |ws, w, cx| ws.open_doc_mindmap(w, cx)),
    ("导出为 PDF", "", "pdf export daochu", |ws, w, cx| ws.export_pdf(&ExportPdf, w, cx)),
    ("导出为 Markdown", "⇧⌘E", "markdown md export daochu", |_, w, cx| w.dispatch_action(Box::new(ExportMarkdown), cx)),
    ("导出完整备份（文档 + 图片 + 附件）", "", "backup beifen export daochu quanbu", |_, w, cx| w.dispatch_action(Box::new(ExportBackup), cx)),
    ("从备份恢复…", "", "restore backup huifu beifen", |_, w, cx| w.dispatch_action(Box::new(RestoreBackup), cx)),
    ("导出为 HTML", "", "html export daochu", |_, w, cx| w.dispatch_action(Box::new(ExportHtml), cx)),
    ("导出为飞书 DocxXML", "", "feishu lark docxml xml export daochu", |_, w, cx| w.dispatch_action(Box::new(ExportDocxXml), cx)),
    ("导入 Markdown…", "⌘O", "import daoru markdown feishu docxml xml", |_, w, cx| w.dispatch_action(Box::new(ImportMarkdown), cx)),
    ("切换深色 / 浅色主题", "⇧⌘D", "theme dark light zhuti shense", |_, w, cx| w.dispatch_action(Box::new(CycleTheme), cx)),
    ("专注模式", "⇧⌘F", "focus zhuanzhu", |_, w, cx| w.dispatch_action(Box::new(FocusMode), cx)),
    ("显示 / 隐藏侧边栏", "⌘\\", "sidebar cebianlan", |_, w, cx| w.dispatch_action(Box::new(ToggleSidebar), cx)),
    ("显示 / 隐藏大纲", "⇧⌘O", "outline dagang", |_, w, cx| w.dispatch_action(Box::new(ToggleOutline), cx)),
    ("设置", "⌘,", "settings shezhi preferences", |ws, w, cx| ws.open_settings(&OpenSettings, w, cx)),
    ("快捷键", "⌘/", "shortcuts keys keymap kuaijiejian anjian", |_, w, cx| w.dispatch_action(Box::new(ShowShortcuts), cx)),
    ("性能面板", "⌥⌘P", "perf xingneng", |_, w, cx| w.dispatch_action(Box::new(TogglePerf), cx)),
    ("打开欢迎文档", "", "welcome huanying", |_, w, cx| w.dispatch_action(Box::new(OpenWelcome), cx)),
    ("把当前文档移到回收站", "", "trash delete shanchu huishouzhan", |_, w, cx| w.dispatch_action(Box::new(TrashDoc), cx)),
    ("关于石渠", "", "about guanyu", |_, w, cx| w.dispatch_action(Box::new(About), cx)),
];

/// 命令面板里显示的按键：命令对应的动作被用户改过键时显示新按键。
fn command_hint(name: &str, default: &str, cx: &App) -> String {
    let action = match name {
        "新建文档" => "stonebrook::NewDoc",
        "新建子文档" => "stonebrook::NewSubDoc",
        "快速打开 / 全文搜索" => "stonebrook::QuickOpen",
        "演示模式" => "stonebrook::Present",
        "切换阅读 / 编辑模式" => "stonebrook::ToggleReadMode",
        "导出为 Markdown" => "stonebrook::ExportMarkdown",
        "导入 Markdown…" => "stonebrook::ImportMarkdown",
        "切换深色 / 浅色主题" => "stonebrook::CycleTheme",
        "专注模式" => "stonebrook::FocusMode",
        "显示 / 隐藏侧边栏" => "stonebrook::ToggleSidebar",
        "显示 / 隐藏大纲" => "stonebrook::ToggleOutline",
        "设置" => "stonebrook::OpenSettings",
        "快捷键" => "stonebrook::ShowShortcuts",
        "性能面板" => "stonebrook::TogglePerf",
        _ => return default.to_string(),
    };
    crate::shortcuts::hint(action, cx)
}

fn cmd_matches(q: &str) -> Vec<usize> {
    let q = q.trim().to_lowercase();
    COMMANDS
        .iter()
        .enumerate()
        .filter(|(_, (name, _, kw, _))| {
            q.is_empty()
                || name.to_lowercase().contains(&q)
                || crate::i18n::tr(name).to_lowercase().contains(&q)
                || kw.contains(&q)
                || kw.replace(' ', "").contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

/// 演示模式的一页：标题 + 内容块
pub struct Slide {
    title: String,
    blocks: Vec<Arc<Block>>,
    cover: bool,
}

fn build_slides(content: &DocContent) -> Vec<Slide> {
    let mut slides = vec![Slide { title: content.title(), blocks: vec![], cover: true }];
    let mut cur: Option<Slide> = None;
    for b in content.blocks.iter().skip(1) {
        match b.kind {
            BlockKind::Heading { level } if level <= 2 => {
                if let Some(s) = cur.take() {
                    slides.push(s);
                }
                cur = Some(Slide { title: b.text.text.clone(), blocks: vec![], cover: false });
            }
            _ => {
                let s = cur.get_or_insert_with(|| Slide { title: String::new(), blocks: vec![], cover: false });
                // 没有标题分隔的长文：每 10 块一页
                if s.blocks.len() >= 10 && s.title.is_empty() {
                    let full = cur.take().unwrap();
                    slides.push(full);
                    cur = Some(Slide { title: String::new(), blocks: vec![b.clone()], cover: false });
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
    }
    slides
}

impl Workspace {
    // ───────────── 写作统计 ─────────────

    /// 最近 n 天的每日输入字数（今天在最后）。
    fn daily_stats(&self, days: i64) -> Vec<(NaiveDate, usize)> {
        let today = Local::now().date_naive();
        (0..days)
            .rev()
            .map(|d| {
                let day = today - CDuration::days(d);
                let n = self.store.get_kv(&format!("stats:{}", day.format("%Y-%m-%d"))).and_then(|v| v.parse().ok()).unwrap_or(0);
                (day, n)
            })
            .collect()
    }

    fn render_heatmap(&self, t: &Theme) -> impl IntoElement {
        let weeks = 20i64;
        let today = Local::now().date_naive();
        let pad = today.weekday().num_days_from_monday() as i64;
        let stats = self.daily_stats(weeks * 7 + pad - 6);
        let max = stats.iter().map(|s| s.1).max().unwrap_or(0).max(1);
        let total: usize = stats.iter().map(|s| s.1).sum();
        let active = stats.iter().filter(|s| s.1 > 0).count();
        // 连续写作天数
        let mut streak = 0;
        for s in stats.iter().rev() {
            if s.1 > 0 {
                streak += 1;
            } else if s.0 != today {
                break;
            }
        }
        let level = |n: usize| -> Hsla {
            if n == 0 {
                return t.hover;
            }
            let r = n as f32 / max as f32;
            let a = if r > 0.75 {
                1.0
            } else if r > 0.5 {
                0.75
            } else if r > 0.25 {
                0.5
            } else {
                0.28
            };
            t.accent.opacity(a)
        };
        let mut cols = div().flex().flex_row().gap(px(3.));
        for w in 0..weeks {
            let mut col = div().flex().flex_col().gap(px(3.));
            for d in 0..7 {
                let i = (w * 7 + d) as usize;
                let cell = stats.get(i).copied();
                col = col.child(div().size(px(11.)).rounded(px(2.)).bg(match cell {
                    Some((day, n)) if day <= today => level(n),
                    _ => gpui::transparent_black(),
                }));
            }
            cols = cols.child(col);
        }
        div().flex().flex_col().gap_2().child(cols).child(
            div()
                .flex()
                .flex_row()
                .gap_4()
                .text_size(px(12.))
                .text_color(t.text_secondary)
                .child(format!("近 {weeks} 周共写 {total} 字"))
                .child(format!("{active} 天有输入"))
                .child(format!("连续 {streak} 天")),
        )
    }

    // ───────────── 设置 ─────────────

    pub fn open_settings(&mut self, _: &OpenSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.save_now(cx));
        self.settings_open = !self.settings_open;
        self.more_menu = false;
        cx.notify();
    }

    fn set_language(&mut self, english: bool, _: &mut Window, cx: &mut Context<Self>) {
        crate::i18n::set_english(english);
        self.store.set_kv("lang", if english { "en" } else { "zh" });
        crate::refresh_menus(cx);
        cx.refresh_windows();
        cx.notify();
    }

    fn set_css_theme(&mut self, sel: String, window: &mut Window, cx: &mut Context<Self>) {
        self.store.set_kv("css_theme", &sel);
        self.css_theme = sel;
        self.apply_theme(window, cx);
        cx.notify();
    }

    /// 在默认文本编辑器里打开主题文件。
    fn open_theme_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if std::process::Command::new("open").arg("-t").arg(path).spawn().is_err() {
            cx.reveal_path(path);
        }
    }

    /// 写一份新主题（内容为 `src`，空时用写满全部变量的模板），选中并打开。
    fn create_css_theme(&mut self, name: &str, src: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let dir = crate::theme_css::themes_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.show_toast(format!("无法创建主题文件夹：{e}"), cx);
            return;
        }
        let file = (1..)
            .map(|i| if i == 1 { format!("{name}.css") } else { format!("{name} {i}.css") })
            .find(|f| !dir.join(f).exists())
            .unwrap_or_else(|| format!("{name}.css"));
        let shown = file.trim_end_matches(".css").to_string();
        let src = match src {
            // 复制内置主题：改个名字，免得两份同名
            Some(s) => match crate::theme_css::theme_name(&s) {
                Some(old) => s.replacen(&format!("\"{old}\""), &format!("\"{shown}\""), 1),
                None => s,
            },
            None => crate::theme_css::template(&shown),
        };
        if let Err(e) = std::fs::write(dir.join(&file), src) {
            self.show_toast(format!("无法写入主题文件：{e}"), cx);
            return;
        }
        self.set_css_theme(format!("user:{file}"), window, cx);
        self.open_theme_file(&dir.join(&file), cx);
        self.show_toast(format!("已创建主题「{shown}」，保存文件后自动生效"), cx);
    }

    /// 编辑当前主题：用户主题直接打开；内置主题先复制一份再打开（内置的不能改）。
    fn edit_css_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = crate::theme_css::user_path(&self.css_theme) {
            self.open_theme_file(&path, cx);
            return;
        }
        let builtin = crate::theme_css::source(&self.css_theme).filter(|s| !s.is_empty());
        let name = crate::theme_css::list().into_iter().find(|(v, _)| *v == self.css_theme).map(|(_, n)| n).unwrap_or_default();
        match builtin {
            Some(src) => self.create_css_theme(&format!("{name} 副本"), Some(src), window, cx),
            None => self.create_css_theme("我的主题", None, window, cx),
        }
    }

    fn set_theme_mode(&mut self, mode: ThemeMode, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_mode = mode;
        self.store.set_kv(
            "theme",
            match mode {
                ThemeMode::Light => "light",
                ThemeMode::Dark => "dark",
                ThemeMode::System => "system",
            },
        );
        self.apply_theme(window, cx);
    }

    pub(super) fn render_settings(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let seg = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .px_3()
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(on, |d| d.bg(t.panel_bg).shadow_sm().text_color(t.accent))
                .when(!on, |d| d.text_color(t.text_secondary).hover(|s| s.text_color(t.text)))
                .child(crate::i18n::tr(label))
        };
        let group = || div().flex().flex_row().p(px(3.)).gap(px(2.)).rounded(px(8.)).bg(t.hover);
        let row = |label: &'static str, desc: &'static str| {
            div().flex().flex_row().items_center().justify_between().gap(px(16.)).py_3().border_b_1().border_color(t.border).child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(px(2.))
                    .child(div().text_size(px(14.)).text_color(t.text).child(crate::i18n::tr(label)))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr(desc))),
            )
        };
        let mode = self.theme_mode;
        let fs = crate::settings::font_size() as u32;
        let wide = crate::settings::wide();
        let net = crate::settings::network();
        let docs = self.docs.iter().filter(|d| !d.deleted).count();
        let objs = self.store.object_ids().len();
        let assets = dir_size(&crate::store::data_dir().join("assets"));
        let data_dir = crate::store::data_dir();
        div()
            .absolute()
            .inset_0()
            .bg(t.overlay)
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.settings_open = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("settings")
                    .w(px(560.))
                    .max_h(px(680.))
                    .overflow_y_scroll()
                    .p(px(28.))
                    .bg(t.panel_bg)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(16.))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD).text_color(t.text).child(crate::i18n::tr("设置")))
                            .child(
                                div()
                                    .id("set-close")
                                    .size(px(28.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(t.hover))
                                    .child(icon("x", 15., t.text_muted))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.settings_open = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(div().mt_4().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("外观")))
                    .child(
                        row("主题", "深色模式下所有图表、公式都会自动适配").child(
                            group()
                                .child(
                                    seg("th-l", "浅色", mode == ThemeMode::Light)
                                        .on_click(cx.listener(|this, _, w, cx| this.set_theme_mode(ThemeMode::Light, w, cx))),
                                )
                                .child(
                                    seg("th-d", "深色", mode == ThemeMode::Dark)
                                        .on_click(cx.listener(|this, _, w, cx| this.set_theme_mode(ThemeMode::Dark, w, cx))),
                                )
                                .child(
                                    seg("th-s", "跟随系统", mode == ThemeMode::System)
                                        .on_click(cx.listener(|this, _, w, cx| this.set_theme_mode(ThemeMode::System, w, cx))),
                                ),
                        ),
                    )
                    .child({
                        // 配色方案：内置 + 用户主题（<数据目录>/themes/*.css）
                        let themes = crate::theme_css::list();
                        let current = self.css_theme.clone();
                        let problems = self.theme_problems.clone();
                        let btn = |id: &'static str, label: &'static str| {
                            div()
                                .id(id)
                                .px_3()
                                .py(px(4.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(t.border)
                                .cursor_pointer()
                                .text_size(px(12.))
                                .text_color(t.text_secondary)
                                .hover(|s| s.bg(t.hover).text_color(t.text))
                                .child(crate::i18n::tr(label))
                        };
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .py_3()
                            .border_b_1()
                            .border_color(t.border)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(div().text_size(px(14.)).text_color(t.text).child(crate::i18n::tr("配色方案")))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(t.text_muted)
                                            .child(crate::i18n::tr("用 CSS 变量改界面与文档的颜色，保存文件后自动生效")),
                                    ),
                            )
                            .child(div().flex().flex_row().flex_wrap().gap(px(6.)).children(themes.into_iter().enumerate().map(|(k, (sel, name))| {
                                let on = sel == current;
                                div()
                                    .id(("css-theme", k))
                                    .px_3()
                                    .py(px(5.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(if on { t.accent } else { t.border })
                                    .cursor_pointer()
                                    .text_size(px(13.))
                                    .text_color(if on { t.accent } else { t.text_secondary })
                                    .hover(|s| s.text_color(t.text))
                                    .child(crate::i18n::tr_owned(&name))
                                    .on_click(cx.listener(move |this, _, w, cx| this.set_css_theme(sel.clone(), w, cx)))
                            })))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap(px(6.))
                                    .child(btn("css-new", "新建主题").on_click(cx.listener(|this, _, w, cx| this.create_css_theme("我的主题", None, w, cx))))
                                    .child(btn("css-edit", "编辑 CSS").on_click(cx.listener(|this, _, w, cx| this.edit_css_theme(w, cx))))
                                    .child(btn("css-dir", "打开主题文件夹").on_click(cx.listener(|_, _, _, cx| {
                                        let dir = crate::theme_css::themes_dir();
                                        let _ = std::fs::create_dir_all(&dir);
                                        cx.reveal_path(&dir);
                                    }))),
                            )
                            .when(!problems.is_empty(), |d| {
                                d.child(
                                    div()
                                        .p_2()
                                        .rounded(px(6.))
                                        .bg(t.hover)
                                        .text_size(px(12.))
                                        .text_color(t.text_secondary)
                                        .child(div().text_color(t.text).child(format!("未生效（{}）", problems.len())))
                                        .children(problems.into_iter().take(12).map(|p| div().child(p))),
                                )
                            })
                    })
                    .child(
                        row("界面语言", "菜单、面板与提示的语言；文档内容不变").child(
                            group()
                                .child(seg("lang-zh", "中文", !crate::i18n::english()).on_click(cx.listener(|this, _, w, cx| this.set_language(false, w, cx))))
                                .child(
                                    seg("lang-en", "English", crate::i18n::english()).on_click(cx.listener(|this, _, w, cx| this.set_language(true, w, cx))),
                                ),
                        ),
                    )
                    .child(row("正文字号", "影响所有文档的正文与列表").child(group().children(crate::settings::FONT_SIZES.iter().enumerate().map(
                        |(i, &v)| {
                            seg(["fs14", "fs15", "fs16", "fs17"][i], ["14", "15", "16", "17"][i], fs == v).on_click(cx.listener(move |this, _, _, cx| {
                                crate::settings::set_font_size(&this.store, v);
                                this.editor.update(cx, |e, cx| e.relayout(cx));
                                cx.notify();
                            }))
                        },
                    ))))
                    .child(
                        row("页面宽度", "宽版适合表格、图表较多的文档").child(
                            group()
                                .child(seg("w-n", "标准", !wide).on_click(cx.listener(|this, _, _, cx| {
                                    crate::settings::set_wide(&this.store, false);
                                    this.editor.update(cx, |e, cx| e.relayout(cx));
                                    cx.notify();
                                })))
                                .child(seg("w-w", "宽版", wide).on_click(cx.listener(|this, _, _, cx| {
                                    crate::settings::set_wide(&this.store, true);
                                    this.editor.update(cx, |e, cx| e.relayout(cx));
                                    cx.notify();
                                }))),
                        ),
                    )
                    .child(div().mt_5().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("评论")))
                    .child(
                        row("评论作者名", "添加评论时显示的名字").child(
                            div()
                                .w(px(200.))
                                .px_2()
                                .py_1()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(t.border)
                                .text_size(px(13.))
                                .line_height(px(22.))
                                .child(self.author_input.clone()),
                        ),
                    )
                    .child(div().mt_5().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("隐私")))
                    .child(
                        row("允许联网", "仅用于网页卡片抓取标题和封面；关闭后石渠完全离线").child(
                            group()
                                .child(seg("net-on", "开启", net).on_click(cx.listener(|this, _, _, cx| {
                                    crate::settings::set_network(&this.store, true);
                                    cx.notify();
                                })))
                                .child(seg("net-off", "关闭", !net).on_click(cx.listener(|this, _, _, cx| {
                                    crate::settings::set_network(&this.store, false);
                                    cx.notify();
                                }))),
                        ),
                    )
                    .child(div().mt_5().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("写作统计")))
                    .child(div().pt_3().child(self.render_heatmap(t)))
                    .child(div().mt_5().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("数据")))
                    .child(
                        row("本地数据", "文档、图片、嵌入对象全部保存在这台电脑上").child(
                            div()
                                .text_size(px(12.5))
                                .text_color(t.text_secondary)
                                .child(format!("{docs} 篇文档 · {objs} 个嵌入对象 · 资源 {}", crate::convert::human_size(assets))),
                        ),
                    )
                    .child(
                        div()
                            .pt_3()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(11.5))
                                    .text_color(t.text_muted)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(data_dir.display().to_string()),
                            )
                            .child(
                                div()
                                    .id("set-reveal")
                                    .px_3()
                                    .py(px(5.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(t.border)
                                    .cursor_pointer()
                                    .text_size(px(12.5))
                                    .hover(|s| s.bg(t.hover))
                                    .child(crate::i18n::tr("在访达中显示"))
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.reveal_path(&data_dir.join(crate::store::DB_FILE));
                                    })),
                            ),
                    ),
            )
    }

    // ───────────── 命令面板 ─────────────

    pub fn command_palette(&mut self, _: &CommandPalette, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.palette = None;
            window.focus(&self.editor.focus_handle(cx));
            cx.notify();
            return;
        }
        let input = cx.new(|cx| LineInput::new("输入命令，例如「导出」「主题」「演示」", cx));
        let sub = cx.subscribe_in(&input, window, |this, input, ev: &InputEvent, window, cx| {
            let q = input.read(cx).text();
            let n = cmd_matches(&q).len();
            match ev {
                InputEvent::Changed => {
                    if let Some(p) = &mut this.palette {
                        p.2 = 0;
                    }
                }
                InputEvent::Up => {
                    if let Some(p) = &mut this.palette {
                        p.2 = p.2.saturating_sub(1);
                    }
                }
                InputEvent::Down => {
                    if let Some(p) = &mut this.palette {
                        p.2 = (p.2 + 1).min(n.saturating_sub(1));
                    }
                }
                InputEvent::Submit { .. } => {
                    let sel = this.palette.as_ref().map(|p| p.2).unwrap_or(0);
                    if let Some(&i) = cmd_matches(&q).get(sel) {
                        this.run_command(i, window, cx);
                    }
                    return;
                }
                InputEvent::Cancel => {
                    this.palette = None;
                    window.focus(&this.editor.focus_handle(cx));
                }
            }
            cx.notify();
        });
        window.focus(&input.focus_handle(cx));
        self.palette = Some((input, sub, 0));
        self.more_menu = false;
        cx.notify();
    }

    fn run_command(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        window.focus(&self.editor.focus_handle(cx));
        (COMMANDS[i].3)(self, window, cx);
        cx.notify();
    }

    pub(super) fn render_palette(&self, t: &Theme, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (input, _, sel) = self.palette.as_ref()?;
        let q = input.read(cx).text();
        let items = cmd_matches(&q);
        let sel = (*sel).min(items.len().saturating_sub(1));
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_center()
                .pt(px(110.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.palette = None;
                        window.focus(&this.editor.focus_handle(cx));
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(560.))
                        .h_auto()
                        .flex()
                        .flex_col()
                        .bg(t.panel_bg)
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(12.))
                        .shadow_lg()
                        .overflow_hidden()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_4()
                                .h(px(50.))
                                .border_b_1()
                                .border_color(t.border)
                                .child(icon("command", 16., t.text_muted))
                                .child(div().flex_1().text_size(px(15.)).line_height(px(24.)).child(input.clone())),
                        )
                        .child(
                            div()
                                .id("palette-list")
                                .max_h(px(400.))
                                .overflow_y_scroll()
                                .p(px(6.))
                                .when(items.is_empty(), |d| {
                                    d.child(div().p_4().text_size(px(13.)).text_color(t.text_muted).child(crate::i18n::tr("没有匹配的命令")))
                                })
                                .children(items.iter().enumerate().map(|(k, &i)| {
                                    let (name, key, _, _) = COMMANDS[i];
                                    div()
                                        .id(("cmd", i))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .px_3()
                                        .py(px(8.))
                                        .rounded(px(7.))
                                        .cursor_pointer()
                                        .text_size(px(14.))
                                        .when(k == sel, |d| d.bg(t.active))
                                        .hover(|s| s.bg(t.hover))
                                        .child(div().flex_1().text_color(t.text).child(crate::i18n::tr(name)))
                                        .child(div().text_size(px(12.)).text_color(t.text_muted).child(command_hint(name, key, cx)))
                                        .on_click(cx.listener(move |this, _, window, cx| this.run_command(i, window, cx)))
                                })),
                        ),
                ),
        )
    }

    // ───────────── PDF 导出 ─────────────

    pub fn export_pdf(&mut self, _: &ExportPdf, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.save_now(cx));
        let (title, html) = {
            let e = self.editor.read(cx);
            let title = e.content.title();
            (title.clone(), pdf_page(&title, &e.content.blocks))
        };
        let name = format!("{}.pdf", if title.trim().is_empty() { "未命名文档".to_string() } else { title.replace(['/', ':'], "-") });
        let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this: WeakEntity<Workspace>, cx: &mut AsyncApp| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            this.update(cx, |this, cx| this.show_toast("正在生成 PDF…".into(), cx)).ok();
            let ok = crate::webview::html_to_pdf(cx, &html, &crate::store::data_dir(), &path).await;
            this.update(cx, |this, cx| {
                if ok {
                    this.show_toast(format!("已导出 PDF：{}", path.display()), cx);
                    cx.reveal_path(&path);
                } else {
                    this.show_toast("PDF 导出失败".into(), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// 测试脚本：直接导出 PDF 到指定路径。
    #[cfg(feature = "dev")]
    pub fn test_pdf(&mut self, path: String, cx: &mut Context<Self>) {
        let html = {
            let e = self.editor.read(cx);
            pdf_page(&e.content.title(), &e.content.blocks)
        };
        cx.spawn(async move |_: WeakEntity<Workspace>, cx: &mut AsyncApp| {
            let ok = crate::webview::html_to_pdf(cx, &html, &crate::store::data_dir(), std::path::Path::new(&path)).await;
            eprintln!("pdf export: {ok}");
        })
        .detach();
    }

    // ───────────── 演示模式 ─────────────

    pub fn present(&mut self, _: &Present, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.save_now(cx));
        let slides = build_slides(&self.editor.read(cx).content);
        self.slides = Some((slides, 0));
        window.focus(&self.present_focus);
        if !window.is_fullscreen() {
            window.toggle_fullscreen();
            self.present_fullscreen = true;
        }
        cx.notify();
    }

    fn exit_present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.slides = None;
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
        cx.notify();
    }

    fn slide_block(b: &Block, t: &Theme, window: &mut Window) -> Option<AnyElement> {
        let text = b.text.plain_with_math();
        let indent = px(b.indent as f32 * 28.);
        let line = |prefix: String, s: String, size: f32, col: Hsla| {
            div()
                .flex()
                .flex_row()
                .gap_3()
                .pl(indent)
                .text_size(px(size))
                .line_height(px(size * 1.55))
                .text_color(col)
                .when(!prefix.is_empty(), |d| d.child(div().text_color(t.accent).child(prefix)))
                .child(div().flex_1().child(s))
                .into_any_element()
        };
        Some(match &b.kind {
            BlockKind::Text => line(String::new(), text, 26., t.text),
            BlockKind::Heading { .. } => div().pt_2().text_size(px(30.)).font_weight(FontWeight::SEMIBOLD).text_color(t.text).child(text).into_any_element(),
            BlockKind::Bullet => line("•".into(), text, 26., t.text),
            BlockKind::Ordered => line("›".into(), text, 26., t.text),
            BlockKind::Todo { checked } => line(if *checked { "☑".into() } else { "☐".into() }, text, 26., if *checked { t.text_muted } else { t.text }),
            BlockKind::Quote | BlockKind::Callout { .. } => div()
                .pl_5()
                .border_l_4()
                .border_color(t.accent)
                .text_size(px(24.))
                .line_height(px(38.))
                .text_color(t.text_secondary)
                .child(text)
                .into_any_element(),
            BlockKind::Code { .. } => div()
                .p_4()
                .rounded(px(10.))
                .bg(t.code_bg)
                .font_family("Menlo")
                .text_size(px(18.))
                .line_height(px(28.))
                .text_color(t.text)
                .whitespace_normal()
                .child(text)
                .into_any_element(),
            BlockKind::Image { path, .. } => {
                div().flex().justify_center().child(img(std::path::PathBuf::from(path)).max_h(px(460.)).max_w_full().rounded(px(8.))).into_any_element()
            }
            BlockKind::Math => {
                let p = crate::math::cached_path(&b.text.text, true)?;
                div().flex().justify_center().child(img(p).max_h(px(160.))).into_any_element()
            }
            BlockKind::Diagram { offsets, .. } => {
                let mut m = crate::editor::GpuiMeasure { window };
                let scene = Arc::new(crate::diagram::render(&b.text.text, t.dark, None, &mut m, offsets).ok()?);
                let (w, h): (f32, f32) = (scene.width.max(1.), scene.height.max(1.));
                let s = (1100. / w).min(440. / h).min(1.6);
                div()
                    .flex()
                    .justify_center()
                    .child(div().w(px(w * s)).h(px(h * s)).child(
                        canvas(|b, _, _| b, move |b, _, window, cx| paint_scene(&scene, View { origin: b.origin, scale: s }, b, window, cx)).size_full(),
                    ))
                    .into_any_element()
            }
            BlockKind::Divider => div().h(px(2.)).my_2().bg(t.border).into_any_element(),
            _ => return None,
        })
    }

    pub(super) fn render_present(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (slides, i) = self.slides.as_ref()?;
        let t = if theme(cx).dark { theme(cx).clone() } else { Theme::dark() };
        let slide = slides.get(*i)?;
        let n = slides.len();
        let body: Vec<AnyElement> = slide.blocks.iter().filter_map(|b| Self::slide_block(b, &t, window)).collect();
        Some(
            div()
                .id("present")
                .absolute()
                .inset_0()
                .occlude()
                .track_focus(&self.present_focus)
                .bg(t.bg)
                .text_color(t.text)
                .font_family("PingFang SC")
                .flex()
                .flex_col()
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| match ev.keystroke.key.as_str() {
                    "right" | "down" | "space" | "pagedown" | "enter" => this.slide_step(1, cx),
                    "left" | "up" | "pageup" | "backspace" => this.slide_step(-1, cx),
                    "home" => this.slide_step(-1000, cx),
                    "end" => this.slide_step(1000, cx),
                    "escape" | "f5" => this.exit_present(window, cx),
                    _ => {}
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                        let w = window.viewport_size().width;
                        this.slide_step(if ev.position.x < w / 4. { -1 } else { 1 }, cx);
                    }),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .px(px(120.))
                        .py(px(60.))
                        .gap_4()
                        .overflow_hidden()
                        .when(slide.cover, |d| {
                            d.items_center()
                                .child(div().text_size(px(64.)).font_weight(FontWeight::BOLD).text_center().child(if slide.title.is_empty() {
                                    "未命名文档".to_string()
                                } else {
                                    slide.title.clone()
                                }))
                                .child(div().w(px(80.)).h(px(4.)).rounded(px(2.)).bg(t.accent))
                                .children(
                                    slide
                                        .blocks
                                        .iter()
                                        .map(|b| div().text_size(px(26.)).text_color(t.text_secondary).text_center().child(b.text.plain_with_math())),
                                )
                                .child(div().mt_4().text_size(px(16.)).text_color(t.text_muted).child(Local::now().format("%Y 年 %m 月 %d 日").to_string()))
                        })
                        .when(!slide.cover, |d| {
                            d.when(!slide.title.is_empty(), |d| {
                                d.child(div().text_size(px(46.)).font_weight(FontWeight::BOLD).child(slide.title.clone()))
                                    .child(div().w(px(56.)).h(px(4.)).mb_4().rounded(px(2.)).bg(t.accent))
                            })
                            .children(body)
                        }),
                )
                .child(
                    div()
                        .h(px(48.))
                        .px(px(40.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .text_size(px(13.))
                        .text_color(t.text_muted)
                        .child(crate::i18n::tr("← → 翻页 · Esc 退出"))
                        .child(format!("{} / {}", i + 1, n)),
                )
                .child(div().h(px(3.)).w(relative((*i + 1) as f32 / n as f32)).bg(t.accent)),
        )
    }
}

/// PDF 用的 HTML：与网页导出同样式，额外加页边距。
fn pdf_page(title: &str, blocks: &[Arc<Block>]) -> String {
    convert::full_html_page(title, &convert::blocks_to_html(blocks)).replace("</head>", "<style>body{margin:0 auto;padding:40px 48px}</style></head>")
}

fn dir_size(p: &std::path::Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => dir_size(&e.path()),
                    Ok(m) => m.len(),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}
