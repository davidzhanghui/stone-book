//! 石渠 Stonebrook —— 纯本地的块级文档编辑器（Rust + GPUI）。

#[cfg(feature = "dev")]
mod autotest;
mod backup;
mod base;
mod board;
mod canvas;
mod clipboard;
mod convert;
mod diagram;
mod editor;
mod highlight;
mod katex_assets;
mod line_input;
mod linkcard;
mod math;
mod media;
mod model;
mod objects;
mod seed;
mod settings;
mod sheet;
mod store;
mod templates;
mod text_area;
mod theme;
mod ui;
mod webview;
mod words;
mod workspace;

use editor::*;
use gpui::*;
use workspace::*;

fn bind_keys(cx: &mut App) {
    let e = Some("Editor");
    cx.bind_keys([
        // 光标
        KeyBinding::new("left", MoveLeft, e),
        KeyBinding::new("right", MoveRight, e),
        KeyBinding::new("up", MoveUp, e),
        KeyBinding::new("down", MoveDown, e),
        KeyBinding::new("ctrl-b", MoveLeft, e),
        KeyBinding::new("ctrl-f", MoveRight, e),
        KeyBinding::new("ctrl-p", MoveUp, e),
        KeyBinding::new("ctrl-n", MoveDown, e),
        KeyBinding::new("shift-left", SelectLeft, e),
        KeyBinding::new("shift-right", SelectRight, e),
        KeyBinding::new("shift-up", SelectUp, e),
        KeyBinding::new("shift-down", SelectDown, e),
        KeyBinding::new("alt-left", WordLeft, e),
        KeyBinding::new("alt-right", WordRight, e),
        KeyBinding::new("alt-shift-left", SelectWordLeft, e),
        KeyBinding::new("alt-shift-right", SelectWordRight, e),
        KeyBinding::new("cmd-left", LineStart, e),
        KeyBinding::new("cmd-right", LineEnd, e),
        KeyBinding::new("ctrl-a", LineStart, e),
        KeyBinding::new("ctrl-e", LineEnd, e),
        KeyBinding::new("home", LineStart, e),
        KeyBinding::new("end", LineEnd, e),
        KeyBinding::new("cmd-shift-left", SelectLineStart, e),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, e),
        KeyBinding::new("shift-home", SelectLineStart, e),
        KeyBinding::new("shift-end", SelectLineEnd, e),
        KeyBinding::new("cmd-up", DocStart, e),
        KeyBinding::new("cmd-down", DocEnd, e),
        KeyBinding::new("cmd-shift-up", SelectDocStart, e),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, e),
        // 编辑
        KeyBinding::new("backspace", Backspace, e),
        KeyBinding::new("shift-backspace", Backspace, e),
        KeyBinding::new("ctrl-h", Backspace, e),
        KeyBinding::new("delete", Delete, e),
        KeyBinding::new("ctrl-d", Delete, e),
        KeyBinding::new("alt-backspace", DeleteWordBack, e),
        KeyBinding::new("cmd-backspace", DeleteLineBack, e),
        KeyBinding::new("enter", Enter, e),
        KeyBinding::new("shift-enter", SoftEnter, e),
        KeyBinding::new("tab", Tab, e),
        KeyBinding::new("shift-tab", ShiftTab, e),
        KeyBinding::new("cmd-a", SelectAll, e),
        KeyBinding::new("cmd-c", Copy, e),
        KeyBinding::new("cmd-x", Cut, e),
        KeyBinding::new("cmd-v", Paste, e),
        KeyBinding::new("cmd-shift-v", PastePlain, e),
        KeyBinding::new("cmd-z", Undo, e),
        KeyBinding::new("cmd-shift-z", Redo, e),
        KeyBinding::new("cmd-y", Redo, e),
        KeyBinding::new("escape", Escape, e),
        // 格式
        KeyBinding::new("cmd-b", Bold, e),
        KeyBinding::new("cmd-i", Italic, e),
        KeyBinding::new("cmd-u", Underline, e),
        KeyBinding::new("cmd-shift-x", Strike, e),
        KeyBinding::new("cmd-e", InlineCode, e),
        KeyBinding::new("cmd-k", InsertLink, e),
        KeyBinding::new("cmd-alt-0", SetText, e),
        KeyBinding::new("cmd-alt-1", SetH1, e),
        KeyBinding::new("cmd-alt-2", SetH2, e),
        KeyBinding::new("cmd-alt-3", SetH3, e),
        KeyBinding::new("cmd-alt-4", SetH4, e),
        KeyBinding::new("cmd-shift-7", SetOrdered, e),
        KeyBinding::new("cmd-shift-8", SetBullet, e),
        KeyBinding::new("cmd-shift-9", SetTodo, e),
        KeyBinding::new("cmd-shift-.", SetQuote, e),
        KeyBinding::new("cmd-alt-c", SetCode, e),
        KeyBinding::new("cmd-enter", ToggleTodo, e),
        KeyBinding::new("cmd-d", DuplicateBlock, e),
        KeyBinding::new("cmd-ctrl-up", MoveBlockUp, e),
        KeyBinding::new("cmd-ctrl-down", MoveBlockDown, e),
        KeyBinding::new("alt-shift-up", MoveBlockUp, e),
        KeyBinding::new("alt-shift-down", MoveBlockDown, e),
        KeyBinding::new("cmd-f", Find, e),
        KeyBinding::new("cmd-g", FindNext, e),
        KeyBinding::new("cmd-shift-g", FindPrev, e),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, e),
        // 全局
        KeyBinding::new("cmd-n", NewDoc, None),
        KeyBinding::new("cmd-shift-n", NewSubDoc, None),
        KeyBinding::new("cmd-p", QuickOpen, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-shift-p", CommandPalette, None),
        KeyBinding::new("f5", Present, None),
        KeyBinding::new("cmd-\\", ToggleSidebar, None),
        KeyBinding::new("cmd-shift-o", ToggleOutline, None),
        KeyBinding::new("cmd-shift-f", FocusMode, None),
        KeyBinding::new("cmd-shift-d", CycleTheme, None),
        KeyBinding::new("cmd-alt-p", TogglePerf, None),
        KeyBinding::new("cmd-shift-e", ExportMarkdown, None),
        KeyBinding::new("cmd-o", ImportMarkdown, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    line_input::bind_keys(cx);
    text_area::bind_keys(cx);
    board::view::bind_keys(cx);
    sheet::bind_keys(cx);
    base::editor::bind_keys(cx);
}

fn menus() -> Vec<Menu> {
    #[allow(unused_mut)]
    let mut menus = vec![
        Menu {
            name: crate::store::APP_NAME.into(),
            items: vec![
                MenuItem::action(format!("关于{}", crate::store::APP_NAME), About),
                MenuItem::separator(),
                MenuItem::action("设置…", OpenSettings),
                MenuItem::action("命令面板…", CommandPalette),
                MenuItem::separator(),
                MenuItem::os_submenu("服务", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(format!("退出{}", crate::store::APP_NAME), Quit),
            ],
        },
        Menu {
            name: "文件".into(),
            items: vec![
                MenuItem::action("新建文档", NewDoc),
                MenuItem::action("新建子文档", NewSubDoc),
                MenuItem::action("快速打开…", QuickOpen),
                MenuItem::separator(),
                MenuItem::action("导入 Markdown…", ImportMarkdown),
                MenuItem::action("导出为 Markdown…", ExportMarkdown),
                MenuItem::action("导出为 HTML…", ExportHtml),
                MenuItem::action("导出为 PDF…", ExportPdf),
                MenuItem::separator(),
                MenuItem::action("导出完整备份…", ExportBackup),
                MenuItem::action("从备份恢复…", RestoreBackup),
                MenuItem::action("演示模式", Present),
                MenuItem::separator(),
                MenuItem::action("移到回收站", TrashDoc),
            ],
        },
        Menu {
            name: "编辑".into(),
            items: vec![
                MenuItem::action("撤销", Undo),
                MenuItem::action("重做", Redo),
                MenuItem::separator(),
                MenuItem::action("剪切", Cut),
                MenuItem::action("复制", Copy),
                MenuItem::action("粘贴", Paste),
                MenuItem::action("粘贴为纯文本", PastePlain),
                MenuItem::action("全选", SelectAll),
                MenuItem::separator(),
                MenuItem::action("查找…", Find),
                MenuItem::action("查找下一个", FindNext),
                MenuItem::action("查找上一个", FindPrev),
            ],
        },
        Menu {
            name: "格式".into(),
            items: vec![
                MenuItem::action("粗体", Bold),
                MenuItem::action("斜体", Italic),
                MenuItem::action("下划线", Underline),
                MenuItem::action("删除线", Strike),
                MenuItem::action("行内代码", InlineCode),
                MenuItem::action("链接…", InsertLink),
                MenuItem::separator(),
                MenuItem::action("正文", SetText),
                MenuItem::action("一级标题", SetH1),
                MenuItem::action("二级标题", SetH2),
                MenuItem::action("三级标题", SetH3),
                MenuItem::action("四级标题", SetH4),
                MenuItem::action("有序列表", SetOrdered),
                MenuItem::action("无序列表", SetBullet),
                MenuItem::action("待办事项", SetTodo),
                MenuItem::action("引用", SetQuote),
                MenuItem::action("代码块", SetCode),
                MenuItem::separator(),
                MenuItem::action("插入图片…", InsertImage),
                MenuItem::action("创建块副本", DuplicateBlock),
                MenuItem::action("上移块", MoveBlockUp),
                MenuItem::action("下移块", MoveBlockDown),
            ],
        },
        Menu {
            name: "视图".into(),
            items: vec![
                MenuItem::action("侧边栏", ToggleSidebar),
                MenuItem::action("大纲", ToggleOutline),
                MenuItem::action("专注模式", FocusMode),
                MenuItem::action("切换深浅色", CycleTheme),
                MenuItem::separator(),
                MenuItem::action("性能面板", TogglePerf),
                MenuItem::action("打开欢迎文档", OpenWelcome),
            ],
        },
    ];
    #[cfg(feature = "dev")]
    if let Some(view) = menus.last_mut() {
        view.items.push(MenuItem::separator());
        view.items.push(MenuItem::action("生成百万字压测文档", StressTest));
        view.items.push(MenuItem::action("生成 10 万行多维表格压测", BaseStressTest));
    }
    menus
}

/// 把 panic 信息落到数据目录的 panic.log。
///
/// 双击打开的 .app 没有终端接住 stderr，崩溃后只能看到系统「意外退出」对话框，
/// 具体是哪一行、什么原因全丢了。这里额外写一份文件，并保留默认钩子（终端里仍能看到）。
fn install_panic_log() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = store::data_dir().join("panic.log");
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        let loc = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "(非字符串 panic)".to_string()
        };
        let bt = std::backtrace::Backtrace::force_capture();
        let body = format!("[{at}] thread={name}\n位置: {loc}\n消息: {msg}\n\n{bt}\n{}\n", "-".repeat(70));
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(body.as_bytes());
        }
        default_hook(info);
    }));
}

/// 退出前：把还没编码的电子表格入队，并等写队列全部写完。
pub fn flush_all() {
    objects::flush_dirty(true);
    if let Some(s) = store::global() {
        s.flush();
    }
}

fn main() {
    install_panic_log();
    // 上次选择了「从备份恢复」：在打开数据库之前换入
    if let Some(msg) = backup::apply_pending() {
        eprintln!("{msg}");
    }
    let store = match store::Store::open() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("无法打开数据库: {e}");
            std::process::exit(1);
        }
    };
    settings::load(&store);
    board::iconlib::warm();
    store::install(store.clone());
    // 后台回收：7 天前起不再被任何文档引用的嵌入对象
    {
        let s = store.clone();
        std::thread::spawn(move || {
            let n = objects::gc(&s);
            if n > 0 {
                eprintln!("回收了 {n} 个未引用对象");
            }
        });
    }
    Application::new().with_assets(ui::Assets).run(move |cx: &mut App| {
        cx.set_global(theme::Theme::light());
        bind_keys(cx);
        cx.set_menus(menus());
        cx.on_action(|_: &Quit, cx: &mut App| {
            flush_all();
            cx.quit();
        });
        // 电子表格停手后落盘（只在有脏表时做事）
        cx.spawn(async move |cx: &mut AsyncApp| loop {
            cx.background_executor().timer(objects::SHEET_DEBOUNCE / 2).await;
            if cx.update(|_| objects::flush_dirty(false)).is_err() {
                break;
            }
        })
        .detach();
        cx.on_action(|_: &About, _: &mut App| {});

        let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(crate::store::APP_NAME.into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(16.), px(18.))),
                    }),
                    window_min_size: Some(size(px(720.), px(480.))),
                    app_id: Some("app.stonebrook".into()),
                    // 脚本测试模式下用浮动窗口，避免被其他窗口遮挡后 GPUI 暂停绘制
                    kind: if std::env::var_os("STONEBROOK_SCRIPT").is_some() { WindowKind::Floating } else { WindowKind::Normal },
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Workspace::new(store, window, cx)),
            )
            .expect("open window");

        window
            .update(cx, |ws, window, cx| {
                let editor = ws.editor().clone();
                window.focus(&editor.focus_handle(cx));
                // 关窗口前同步保存
                window.on_window_should_close(cx, move |_, cx| {
                    editor.update(cx, |e, _| e.save_now_queued());
                    flush_all();
                    true
                });
            })
            .ok();
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
        #[cfg(feature = "dev")]
        if let Some(script) = std::env::var("STONEBROOK_SCRIPT").ok().and_then(|p| std::fs::read_to_string(p).ok()) {
            autotest::run(script, window, cx);
        }
    });
}
