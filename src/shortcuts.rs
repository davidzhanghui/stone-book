//! 快捷键面板（⌘/）与自定义快捷键（思路来自 quiet.md 的 shortcuts.rs）。
//!
//! 启动时把 `main.rs` 里绑定好的全部按键记为「基准」；用户改过的按键存在数据目录的
//! `shortcuts.json`（动作名 → 按键，空字符串表示停用）。每次修改后清空按键映射，
//! 按「基准 + 自定义」重新绑定：改过的动作只保留一条新绑定，沿用原绑定的上下文。

use gpui::*;
use std::collections::BTreeMap;

/// 面板里列出、允许修改的动作：(动作名, 分组, 名称)。动作名就是 GPUI 的 `namespace::Action`。
pub const ITEMS: &[(&str, &str, &str)] = &[
    ("stonebrook::NewDoc", "应用与窗口", "新建文档"),
    ("stonebrook::NewSubDoc", "应用与窗口", "新建子文档"),
    ("stonebrook::QuickOpen", "应用与窗口", "快速打开 / 全文搜索"),
    ("stonebrook::CommandPalette", "应用与窗口", "命令面板"),
    ("stonebrook::ShowShortcuts", "应用与窗口", "查看快捷键"),
    ("stonebrook::OpenSettings", "应用与窗口", "设置"),
    ("stonebrook::ToggleSidebar", "应用与窗口", "显示 / 隐藏侧边栏"),
    ("stonebrook::ToggleOutline", "应用与窗口", "显示 / 隐藏大纲"),
    ("stonebrook::FocusMode", "应用与窗口", "专注模式"),
    ("stonebrook::CycleTheme", "应用与窗口", "切换深浅色"),
    ("stonebrook::TogglePerf", "应用与窗口", "性能面板"),
    ("stonebrook::Present", "文档", "演示模式"),
    ("stonebrook::ToggleReadMode", "文档", "阅读 / 编辑模式"),
    ("stonebrook::ImportMarkdown", "文档", "导入 Markdown"),
    ("stonebrook::ExportMarkdown", "文档", "导出 Markdown"),
    ("editor::Find", "文档", "文档内查找"),
    ("editor::FindNext", "文档", "查找下一个"),
    ("editor::FindPrev", "文档", "查找上一个"),
    ("editor::Undo", "编辑", "撤销"),
    ("editor::Redo", "编辑", "重做"),
    ("editor::Copy", "编辑", "复制"),
    ("editor::Cut", "编辑", "剪切"),
    ("editor::Paste", "编辑", "粘贴"),
    ("editor::PastePlain", "编辑", "粘贴为纯文本"),
    ("editor::SelectAll", "编辑", "全选"),
    ("editor::InsertLink", "编辑", "插入链接"),
    ("editor::DuplicateBlock", "编辑", "复制当前块"),
    ("editor::MoveBlockUp", "编辑", "上移当前块"),
    ("editor::MoveBlockDown", "编辑", "下移当前块"),
    ("editor::ToggleTodo", "编辑", "勾选 / 取消待办"),
    ("editor::InsertTimestamp", "编辑", "插入音频时间戳"),
    ("editor::ToggleFold", "编辑", "折叠 / 展开子项"),
    ("editor::FocusBlock", "编辑", "聚焦当前项 / 退出聚焦"),
    ("editor::Bold", "文字样式", "粗体"),
    ("editor::Italic", "文字样式", "斜体"),
    ("editor::Underline", "文字样式", "下划线"),
    ("editor::Strike", "文字样式", "删除线"),
    ("editor::InlineCode", "文字样式", "行内代码"),
    ("editor::SetText", "段落与块", "正文"),
    ("editor::SetH1", "段落与块", "一级标题"),
    ("editor::SetH2", "段落与块", "二级标题"),
    ("editor::SetH3", "段落与块", "三级标题"),
    ("editor::SetH4", "段落与块", "四级标题"),
    ("editor::SetOrdered", "段落与块", "有序列表"),
    ("editor::SetBullet", "段落与块", "无序列表"),
    ("editor::SetTodo", "段落与块", "待办"),
    ("editor::SetQuote", "段落与块", "引用"),
    ("editor::SetCode", "段落与块", "代码块"),
];

pub struct Shortcuts {
    base: Vec<KeyBinding>,
    custom: BTreeMap<String, String>,
}
impl Global for Shortcuts {}

fn path() -> std::path::PathBuf {
    crate::store::data_dir().join("shortcuts.json")
}

fn load_custom() -> BTreeMap<String, String> {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_custom(custom: &BTreeMap<String, String>) -> std::io::Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // 先写临时文件再改名：写一半崩溃也不会留下坏文件
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(custom).unwrap_or_default())?;
    std::fs::rename(tmp, p)
}

/// 在 `main.rs` 绑定完全部按键之后调用。
pub fn init(cx: &mut App) {
    let base = cx.key_bindings().borrow().bindings().cloned().collect::<Vec<_>>();
    cx.set_global(Shortcuts { base, custom: load_custom() });
    rebuild(cx);
}

fn rebuild(cx: &mut App) {
    let s = cx.global::<Shortcuts>();
    let mut done: std::collections::HashSet<&str> = Default::default();
    let mut bindings = Vec::with_capacity(s.base.len());
    for b in &s.base {
        let name = b.action().name();
        match s.custom.get(name) {
            None => bindings.push(b.clone()),
            // 改过的动作：只用第一条原绑定的上下文生成一条新绑定，其余原绑定（如 ⌘Y 之于重做）一并去掉
            Some(key) => {
                if key.is_empty() || !done.insert(name) {
                    continue;
                }
                if let Ok(nb) = KeyBinding::load(key, b.action().boxed_clone(), b.predicate(), false, b.action_input(), &DummyKeyboardMapper) {
                    bindings.push(nb);
                }
            }
        }
    }
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
}

/// 动作当前的全部按键（GPUI 写法，如 `cmd-shift-p`）。
pub fn keys_for(action: &str, cx: &App) -> Vec<String> {
    let s = cx.global::<Shortcuts>();
    if let Some(k) = s.custom.get(action) {
        return if k.is_empty() { vec![] } else { vec![k.clone()] };
    }
    s.base.iter().filter(|b| b.action().name() == action).map(|b| b.keystrokes().iter().map(|k| k.unparse()).collect::<Vec<_>>().join(" ")).collect()
}

/// 界面上显示的按键提示：动作当前的第一个按键（改过就是新的），没有按键时为空。
pub fn hint(action: &str, cx: &App) -> String {
    if cx.try_global::<Shortcuts>().is_none() {
        return String::new();
    }
    keys_for(action, cx).first().map(|k| display(k)).unwrap_or_default()
}

pub fn is_custom(action: &str, cx: &App) -> bool {
    cx.global::<Shortcuts>().custom.contains_key(action)
}

/// 另一个列出的动作已经用了这个按键（且两者的生效范围可能重叠）时，返回它的名称。
pub fn conflict(action: &str, key: &str, cx: &App) -> Option<&'static str> {
    let key = normalize(key).unwrap_or_else(|| key.to_string());
    let key = key.as_str();
    ITEMS.iter().filter(|(a, _, _)| *a != action).find(|(a, _, _)| keys_for(a, cx).iter().any(|k| k == key)).map(|(_, _, label)| *label)
}

/// 统一写法（修饰键顺序固定），比较和存储都用它：`cmd-alt-z` → `alt-cmd-z`。
pub fn normalize(key: &str) -> Option<String> {
    key.split(' ').map(|s| Keystroke::parse(s).ok().map(|k| k.unparse())).collect::<Option<Vec<_>>>().map(|v| v.join(" "))
}

/// 设置（Some）或恢复默认（None）。
pub fn set(action: &str, key: Option<String>, cx: &mut App) -> Result<(), String> {
    let key = match key {
        Some(k) if !k.is_empty() => Some(normalize(&k).ok_or_else(|| format!("无法识别的按键：{k}"))?),
        k => k,
    };
    let mut custom = cx.global::<Shortcuts>().custom.clone();
    match key {
        Some(k) => custom.insert(action.to_string(), k),
        None => custom.remove(action),
    };
    save_custom(&custom).map_err(|e| format!("保存快捷键失败：{e}"))?;
    cx.global_mut::<Shortcuts>().custom = custom;
    rebuild(cx);
    Ok(())
}

/// 全部恢复默认。
pub fn reset_all(cx: &mut App) -> Result<(), String> {
    save_custom(&BTreeMap::new()).map_err(|e| format!("保存快捷键失败：{e}"))?;
    cx.global_mut::<Shortcuts>().custom.clear();
    rebuild(cx);
    Ok(())
}

/// 录制时接受的按键：要带 ⌘ / ⌃ / ⌥ 之一，或是功能键（F1–F12）。只按修饰键、或单个字母不算。
pub fn recordable(k: &Keystroke) -> bool {
    let m = k.modifiers;
    let function_key = k.key.len() >= 2 && k.key.starts_with('f') && k.key[1..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n));
    !k.key.is_empty() && !matches!(k.key.as_str(), "shift" | "control" | "alt" | "platform" | "function") && (m.platform || m.control || m.alt || function_key)
}

/// 界面上的写法：`cmd-shift-p` → `⇧⌘P`。
pub fn display(key: &str) -> String {
    key.split(' ')
        .map(|stroke| {
            let Ok(k) = Keystroke::parse(stroke) else { return stroke.to_string() };
            let m = k.modifiers;
            let mut s = String::new();
            for (on, sym) in [(m.control, "⌃"), (m.alt, "⌥"), (m.shift, "⇧"), (m.platform, "⌘")] {
                if on {
                    s.push_str(sym);
                }
            }
            s.push_str(&match k.key.as_str() {
                "enter" => "↵".to_string(),
                "backspace" => "⌫".into(),
                "delete" => "⌦".into(),
                "escape" => "Esc".into(),
                "space" => "Space".into(),
                "tab" => "Tab".into(),
                "up" => "↑".into(),
                "down" => "↓".into(),
                "left" => "←".into(),
                "right" => "→".into(),
                k if k.chars().count() == 1 => k.to_uppercase(),
                k => k.to_uppercase(),
            });
            s
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{display, recordable, ITEMS};
    use gpui::Keystroke;

    #[test]
    fn display_uses_mac_symbols_in_mac_order() {
        assert_eq!(display("cmd-shift-p"), "⇧⌘P");
        assert_eq!(display("cmd-alt-1"), "⌥⌘1");
        assert_eq!(display("ctrl-cmd-up"), "⌃⌘↑");
        assert_eq!(display("f5"), "F5");
        assert_eq!(display("cmd-\\"), "⌘\\");
    }

    #[test]
    fn only_chords_and_function_keys_can_be_recorded() {
        let p = |s| Keystroke::parse(s).unwrap();
        assert!(recordable(&p("cmd-j")));
        assert!(recordable(&p("ctrl-alt-k")));
        assert!(recordable(&p("f6")));
        assert!(!recordable(&p("j")));
        assert!(!recordable(&p("shift-j")));
        assert!(!recordable(&p("enter")));
    }

    #[gpui::test]
    fn custom_keys_replace_and_reset(cx: &mut gpui::TestAppContext) {
        use crate::editor::{Bold, Redo};
        let _env = crate::store::TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("stonebrook-keys-{}", std::process::id()));
        std::env::set_var(crate::store::DATA_DIR_ENV, &dir);
        cx.update(|cx| {
            let e = Some("Editor");
            cx.bind_keys([gpui::KeyBinding::new("cmd-b", Bold, e), gpui::KeyBinding::new("cmd-shift-z", Redo, e), gpui::KeyBinding::new("cmd-y", Redo, e)]);
            super::init(cx);
            let bound = |cx: &gpui::App, key: &str| {
                let key = super::normalize(key).unwrap();
                cx.key_bindings().borrow().bindings().any(|b| b.keystrokes().iter().map(|k| k.unparse()).collect::<Vec<_>>().join(" ") == key)
            };
            assert_eq!(super::keys_for("editor::Redo", cx), vec![super::normalize("cmd-shift-z").unwrap(), "cmd-y".to_string()]);

            super::set("editor::Bold", Some("cmd-j".into()), cx).unwrap();
            assert!(bound(cx, "cmd-j") && !bound(cx, "cmd-b"), "改键后旧按键应失效、新按键生效");
            assert_eq!(super::conflict("editor::Redo", "cmd-j", cx), Some("粗体"));
            // 一个动作有两条原绑定时，改键后只剩一条
            super::set("editor::Redo", Some("cmd-alt-z".into()), cx).unwrap();
            assert!(!bound(cx, "cmd-y") && !bound(cx, "cmd-shift-z") && bound(cx, "cmd-alt-z"));

            // 已写进 shortcuts.json，重启后由 init 读回
            assert_eq!(super::load_custom().get("editor::Bold").map(String::as_str), Some("cmd-j"));

            super::set("editor::Bold", None, cx).unwrap();
            assert!(bound(cx, "cmd-b") && !bound(cx, "cmd-j"));
            super::reset_all(cx).unwrap();
            assert!(bound(cx, "cmd-y") && bound(cx, "cmd-shift-z"));
        });
        std::env::remove_var(crate::store::DATA_DIR_ENV);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_listed_action_has_a_binding() {
        // ITEMS 里的动作名必须真实存在：拼错了面板里就会显示「未设置」
        let src = include_str!("main.rs");
        for (name, _, _) in ITEMS {
            let short = name.rsplit("::").next().unwrap();
            assert!(src.contains(&format!(", {short},")) || src.contains(&format!(", {short}, ")), "{name} 在 main.rs 里没有绑定");
        }
    }
}
