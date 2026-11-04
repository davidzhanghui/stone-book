//! CSS 配色主题（思路来自 quiet.md 的 themes.rs / docs/THEMES.md）。
//!
//! 一个主题就是一份 CSS 文件，用变量覆盖 `Theme` 里的颜色：
//!
//! ```css
//! :root { --theme-name: "暖纸"; --accent: #b5651d; }   /* 明暗都生效 */
//! .light { --bg: #fbf6ec; --text: #3b3024; }             /* 只在浅色模式 */
//! .dark  { --bg: #22201c; }                              /* 只在深色模式 */
//! ```
//!
//! - 颜色：`#rgb` `#rgba` `#rrggbb` `#rrggbbaa` `rgb()` `rgba()` `hsl()` `hsla()` `transparent`，以及 `var(--x, 备用值)`；
//! - 不认识的变量、写错的值不会让主题失效：跳过并在设置里列为「未生效」；
//! - 内置主题编进二进制；用户主题放在 `<数据目录>/themes/*.css`，保存后自动重新加载。

use crate::theme::Theme;
use gpui::{hsla, Hsla, Rgba};
use std::collections::HashMap;
use std::path::PathBuf;

/// 内置主题：(ID, 名称, CSS)。ID 为空的是默认主题（不覆盖任何颜色）。
pub const BUILTIN: &[(&str, &str, &str)] =
    &[("", "默认", ""), ("sepia", "暖纸", include_str!("../assets/themes/sepia.css")), ("nord", "北欧", include_str!("../assets/themes/nord.css"))];

pub fn themes_dir() -> PathBuf {
    crate::store::data_dir().join("themes")
}

/// 可选的主题：(选择值, 显示名)。内置为 `builtin:<id>`（默认为空串），用户主题为 `user:<文件名>`。
pub fn list() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> =
        BUILTIN.iter().map(|(id, name, _)| (if id.is_empty() { String::new() } else { format!("builtin:{id}") }, name.to_string())).collect();
    let mut user: Vec<(String, String)> = std::fs::read_dir(themes_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "css"))
        .map(|e| {
            let file = e.file_name().to_string_lossy().to_string();
            let src = std::fs::read_to_string(e.path()).unwrap_or_default();
            let name = theme_name(&src).unwrap_or_else(|| file.trim_end_matches(".css").to_string());
            (format!("user:{file}"), name)
        })
        .collect();
    user.sort_by(|a, b| a.1.cmp(&b.1));
    out.extend(user);
    out
}

/// 选择值对应的 CSS 源码（读不到时为 None）。
pub fn source(sel: &str) -> Option<String> {
    if let Some(id) = sel.strip_prefix("builtin:") {
        return BUILTIN.iter().find(|b| b.0 == id).map(|b| b.2.to_string());
    }
    if let Some(file) = sel.strip_prefix("user:") {
        return std::fs::read_to_string(themes_dir().join(file)).ok();
    }
    Some(String::new())
}

pub fn user_path(sel: &str) -> Option<PathBuf> {
    sel.strip_prefix("user:").map(|f| themes_dir().join(f))
}

/// `--theme-name: "名字"`
pub fn theme_name(src: &str) -> Option<String> {
    let clean = strip_comments(src);
    let (_, rest) = clean.split_once("--theme-name")?.1.split_once(':')?;
    let v = rest.split([';', '}']).next()?.trim().trim_matches(['"', '\'']).trim();
    (!v.is_empty()).then(|| v.to_string())
}

// ───────────────────────────── 解析 ─────────────────────────────

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = rest[i + 2..].split_once("*/").map(|x| x.1).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// 规则：(选择器们, 声明们)。`@media` 等 at 规则整段跳过。
fn rules(src: &str) -> Vec<(Vec<String>, Vec<(String, String)>)> {
    let src = strip_comments(src);
    let mut out = vec![];
    let mut rest = src.as_str();
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        // 跳过 at 规则（包括嵌套的大括号）
        if selector.starts_with('@') {
            let mut depth = 0;
            let mut end = rest.len();
            for (i, ch) in rest[open..].char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            rest = &rest[end..];
            continue;
        }
        let Some(close) = rest[open..].find('}') else { break };
        let body = &rest[open + 1..open + close];
        let decls = body
            .split(';')
            .filter_map(|d| d.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, _)| !k.is_empty())
            .collect();
        out.push((selector.split(',').map(|s| s.trim().to_string()).collect(), decls));
        rest = &rest[open + close + 1..];
    }
    out
}

/// 某个模式下生效的变量（`:root` 先，`.light` / `.dark` 后，后写的覆盖先写的）。
fn variables(src: &str, dark: bool, unknown_selectors: &mut Vec<String>) -> Vec<(String, String)> {
    let mode = if dark { ".dark" } else { ".light" };
    let mut root = vec![];
    let mut moded = vec![];
    for (sels, decls) in rules(src) {
        for s in &sels {
            match s.as_str() {
                ":root" | "html" => root.extend(decls.iter().cloned()),
                ".light" | ".dark" | ":root.light" | ":root.dark" => {
                    if s.ends_with(mode) {
                        moded.extend(decls.iter().cloned());
                    }
                }
                other => {
                    if !unknown_selectors.iter().any(|u| u == other) {
                        unknown_selectors.push(other.to_string());
                    }
                }
            }
        }
    }
    root.extend(moded);
    root
}

fn hex(s: &str) -> Option<Rgba> {
    let h = s.strip_prefix('#')?;
    let n = |i: usize, w: usize| u8::from_str_radix(&h[i..i + w], 16).ok();
    let (r, g, b, a) = match h.len() {
        3 | 4 => {
            let d = |i| n(i, 1).map(|v| v * 17);
            (d(0)?, d(1)?, d(2)?, if h.len() == 4 { d(3)? } else { 255 })
        }
        6 | 8 => (n(0, 2)?, n(2, 2)?, n(4, 2)?, if h.len() == 8 { n(6, 2)? } else { 255 }),
        _ => return None,
    };
    Some(Rgba { r: r as f32 / 255., g: g as f32 / 255., b: b as f32 / 255., a: a as f32 / 255. })
}

fn func_args(s: &str, names: &[&str]) -> Option<Vec<String>> {
    let (name, rest) = s.split_once('(')?;
    if !names.contains(&name.trim()) {
        return None;
    }
    let inner = rest.strip_suffix(')')?;
    Some(inner.split([',', ' ', '/']).map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect())
}

fn num(s: &str, scale: f32) -> Option<f32> {
    match s.strip_suffix('%') {
        Some(p) => p.parse::<f32>().ok().map(|v| v / 100.),
        None => s.parse::<f32>().ok().map(|v| v / scale),
    }
}

pub fn parse_color(s: &str) -> Option<Hsla> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "transparent" => return Some(hsla(0., 0., 0., 0.)),
        "white" => return Some(hsla(0., 0., 1., 1.)),
        "black" => return Some(hsla(0., 0., 0., 1.)),
        _ => {}
    }
    if s.starts_with('#') {
        return hex(&s).map(Into::into);
    }
    if let Some(a) = func_args(&s, &["rgb", "rgba"]) {
        if a.len() < 3 {
            return None;
        }
        let alpha = a.get(3).map(|x| num(x, 1.)).unwrap_or(Some(1.))?;
        return Some(Rgba { r: num(&a[0], 255.)?, g: num(&a[1], 255.)?, b: num(&a[2], 255.)?, a: alpha }.into());
    }
    if let Some(a) = func_args(&s, &["hsl", "hsla"]) {
        if a.len() < 3 {
            return None;
        }
        let h = a[0].trim_end_matches("deg").parse::<f32>().ok()?.rem_euclid(360.) / 360.;
        let alpha = a.get(3).map(|x| num(x, 1.)).unwrap_or(Some(1.))?;
        return Some(hsla(h, num(&a[1], 100.)?, num(&a[2], 100.)?, alpha));
    }
    None
}

/// 解开 `var(--x, 备用值)`（最多套 8 层）。
fn resolve(v: &str, vars: &HashMap<String, String>, depth: usize) -> Option<String> {
    let v = v.trim();
    if depth > 8 {
        return None;
    }
    let Some(inner) = v.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) else { return Some(v.to_string()) };
    let (name, fallback) = match inner.split_once(',') {
        Some((n, f)) => (n.trim(), Some(f.trim())),
        None => (inner.trim(), None),
    };
    match vars.get(name) {
        Some(x) => resolve(x, vars, depth + 1),
        None => fallback.and_then(|f| resolve(f, vars, depth + 1)),
    }
}

// ───────────────────────────── 变量 → 颜色 ─────────────────────────────

/// 主题里每个颜色对应的变量名（也是模板与文档的顺序）。
fn slots(t: &mut Theme) -> Vec<(String, &mut Hsla)> {
    let mut v: Vec<(String, &mut Hsla)> = vec![
        ("--bg".into(), &mut t.bg),
        ("--sidebar-bg".into(), &mut t.sidebar_bg),
        ("--panel-bg".into(), &mut t.panel_bg),
        ("--hover".into(), &mut t.hover),
        ("--active".into(), &mut t.active),
        ("--border".into(), &mut t.border),
        ("--border-strong".into(), &mut t.border_strong),
        ("--text".into(), &mut t.text),
        ("--text-secondary".into(), &mut t.text_secondary),
        ("--text-muted".into(), &mut t.text_muted),
        ("--placeholder".into(), &mut t.placeholder),
        ("--accent".into(), &mut t.accent),
        ("--accent-soft".into(), &mut t.accent_soft),
        ("--selection".into(), &mut t.selection),
        ("--cursor".into(), &mut t.cursor),
        ("--code-bg".into(), &mut t.code_bg),
        ("--inline-code-bg".into(), &mut t.inline_code_bg),
        ("--inline-code-fg".into(), &mut t.inline_code_fg),
        ("--quote-bar".into(), &mut t.quote_bar),
        ("--link".into(), &mut t.link),
        ("--table-header".into(), &mut t.table_header),
        ("--find-match".into(), &mut t.find_match),
        ("--find-current".into(), &mut t.find_current),
        ("--overlay".into(), &mut t.overlay),
        ("--drop-line".into(), &mut t.drop_line),
        ("--syntax-keyword".into(), &mut t.syntax.keyword),
        ("--syntax-string".into(), &mut t.syntax.string),
        ("--syntax-comment".into(), &mut t.syntax.comment),
        ("--syntax-number".into(), &mut t.syntax.number),
        ("--syntax-function".into(), &mut t.syntax.function),
        ("--syntax-type".into(), &mut t.syntax.typ),
    ];
    for (i, c) in t.fg_palette.iter_mut().enumerate().skip(1) {
        v.push((format!("--color-{i}"), c));
    }
    for (i, c) in t.bg_palette.iter_mut().enumerate().skip(1) {
        v.push((format!("--highlight-{i}"), c));
    }
    for (i, c) in t.bg_strong.iter_mut().enumerate() {
        v.push((format!("--highlight-strong-{i}"), c));
    }
    for (i, c) in t.callout_bg.iter_mut().enumerate() {
        v.push((format!("--callout-bg-{i}"), c));
    }
    for (i, c) in t.callout_border.iter_mut().enumerate() {
        v.push((format!("--callout-border-{i}"), c));
    }
    v
}

/// 把主题 CSS 叠加到 `base` 上，返回未生效的条目（写错的值、不认识的变量 / 选择器）。
pub fn apply(base: &mut Theme, src: &str) -> Vec<String> {
    let mut problems = vec![];
    let mut bad_selectors = vec![];
    let decls = variables(src, base.dark, &mut bad_selectors);
    let vars: HashMap<String, String> = decls.iter().filter(|(k, _)| k.starts_with("--")).cloned().collect();
    // 整个文件里被 var() 引用过的变量（不分明暗：只在 .light 里用到的自定义变量在深色下也不算拼错）
    let clean = strip_comments(src);
    let used_by_var: Vec<&str> = clean.split("var(").skip(1).filter_map(|r| r.split([',', ')']).next()).map(str::trim).collect();
    let mut slots = slots(base);
    for (k, v) in &vars {
        if k == "--theme-name" || k == "--theme-author" || k == "--theme-description" {
            continue;
        }
        match slots.iter_mut().find(|(name, _)| name == k) {
            Some((_, slot)) => match resolve(v, &vars, 0).as_deref().and_then(parse_color) {
                Some(c) => **slot = c,
                None => problems.push(format!("{k}: {v}（无法识别的颜色）")),
            },
            // 自定义变量：被 var() 引用就是有用的，否则大概是拼错了
            None if !used_by_var.contains(&k.as_str()) => problems.push(format!("{k}（不认识的变量）")),
            None => {}
        }
    }
    for (k, _) in decls.iter().filter(|(k, _)| !k.starts_with("--")) {
        problems.push(format!("{k}（只支持 -- 开头的颜色变量）"));
    }
    for s in bad_selectors {
        problems.push(format!("{s}（只支持 :root、.light、.dark）"));
    }
    problems.sort();
    problems.dedup();
    problems
}

fn css_color(c: Hsla) -> String {
    let r: Rgba = c.into();
    let b = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    if r.a >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", b(r.r), b(r.g), b(r.b))
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", b(r.r), b(r.g), b(r.b), b(r.a))
    }
}

/// 新主题的模板：写满全部变量和当前默认值（浅色 / 深色各一份），可以直接当参考。
pub fn template(name: &str) -> String {
    let block = |mut t: Theme| slots(&mut t).into_iter().map(|(k, c)| format!("  {k}: {};\n", css_color(*c))).collect::<String>();
    format!(
        "/* 石渠配色主题。保存后自动生效；写错的值会在 设置 › 外观 里列为「未生效」。\n   :root 里的变量明暗都生效，.light / .dark 里的只在对应模式生效。\n   颜色可写 #rgb #rrggbb #rrggbbaa rgb() rgba() hsl() hsla()，也可以用 var(--自定义变量)。 */\n\n:root {{\n  --theme-name: \"{name}\";\n}}\n\n.light {{\n{}}}\n\n.dark {{\n{}}}\n",
        block(Theme::light()),
        block(Theme::dark())
    )
}

#[cfg(test)]
mod tests {
    use super::{apply, parse_color, template, theme_name, BUILTIN};
    use crate::theme::Theme;
    use gpui::Rgba;

    fn rgb(c: gpui::Hsla) -> (u8, u8, u8, u8) {
        let r: Rgba = c.into();
        let b = |v: f32| (v * 255.).round() as u8;
        (b(r.r), b(r.g), b(r.b), b(r.a))
    }

    #[test]
    fn colors_in_every_notation() {
        assert_eq!(rgb(parse_color("#f00").unwrap()), (255, 0, 0, 255));
        assert_eq!(rgb(parse_color("#00ff0080").unwrap()), (0, 255, 0, 128));
        assert_eq!(rgb(parse_color("rgb(0, 0, 255)").unwrap()), (0, 0, 255, 255));
        assert_eq!(rgb(parse_color("rgba(0 0 255 / 50%)").unwrap()), (0, 0, 255, 128));
        assert_eq!(rgb(parse_color("hsl(120, 100%, 50%)").unwrap()), (0, 255, 0, 255));
        assert_eq!(parse_color("transparent").unwrap().a, 0.);
        assert!(parse_color("#12").is_none() && parse_color("blue-ish").is_none());
    }

    #[test]
    fn root_then_mode_and_problems_are_reported() {
        let css = "/* 注释 */ :root { --theme-name: \"测试\"; --brand: #123456; --accent: var(--brand); --text: nope; --typo: #fff; }
            .dark { --accent: #ff0000; } h1 { color: red; } @media (x) { :root { --bg: #000; } }";
        let mut light = Theme::light();
        let problems = apply(&mut light, css);
        assert_eq!(rgb(light.accent), (0x12, 0x34, 0x56, 255), "var() 引用自定义变量");
        assert_eq!(rgb(light.text), rgb(Theme::light().text), "写错的值保持原样");
        assert_eq!(rgb(light.bg), rgb(Theme::light().bg), "@media 整段忽略");
        assert!(problems.iter().any(|p| p.starts_with("--text")), "{problems:?}");
        assert!(problems.iter().any(|p| p.starts_with("--typo")), "{problems:?}");
        assert!(problems.iter().any(|p| p.starts_with("h1")), "{problems:?}");
        assert!(!problems.iter().any(|p| p.starts_with("--brand")), "被引用的自定义变量不算问题");
        let mut dark = Theme::dark();
        apply(&mut dark, css);
        assert_eq!(rgb(dark.accent), (255, 0, 0, 255), ".dark 覆盖 :root");
        assert_eq!(theme_name(css).as_deref(), Some("测试"));
    }

    #[test]
    fn template_and_builtins_apply_cleanly() {
        let tpl = template("我的主题");
        let mut t = Theme::light();
        assert!(apply(&mut t, &tpl).is_empty(), "模板本身不应有未生效的条目");
        assert_eq!(rgb(t.accent), rgb(Theme::light().accent), "模板写的就是当前值");
        for (id, _, css) in BUILTIN.iter().filter(|b| !b.0.is_empty()) {
            for mut base in [Theme::light(), Theme::dark()] {
                let p = apply(&mut base, css);
                assert!(p.is_empty(), "内置主题 {id} 有未生效的条目：{p:?}");
            }
        }
    }
}
