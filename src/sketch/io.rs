//! `.excalidraw` / `.excalidraw.md`（Obsidian）读写、图片 dataURL ↔ 本地文件、官方剪贴板格式。

use super::model::*;
use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::path::Path;

pub fn mime_of(path: &str) -> &'static str {
    let p = path.to_lowercase();
    if p.ends_with(".png") {
        "image/png"
    } else if p.ends_with(".jpg") || p.ends_with(".jpeg") {
        "image/jpeg"
    } else if p.ends_with(".gif") {
        "image/gif"
    } else if p.ends_with(".webp") {
        "image/webp"
    } else if p.ends_with(".svg") {
        "image/svg+xml"
    } else if p.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/png"
    }
}

fn ext_of_mime(m: &str) -> &'static str {
    match m {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        _ => "png",
    }
}

/// 解析任意来源：`.excalidraw` JSON、官方剪贴板 JSON、Obsidian `.excalidraw.md`。
pub fn parse(text: &str) -> Result<SketchDoc> {
    let t = text.trim_start_matches('\u{feff}').trim();
    if t.starts_with('{') {
        let v: Value = serde_json::from_str(t)?;
        return from_value(v);
    }
    // Obsidian：## Drawing 下的 ```compressed-json 或 ```json 代码块
    if let Some(body) = code_block(t, "compressed-json") {
        let packed: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        let json = lz_decompress_base64(&packed).ok_or_else(|| anyhow!("compressed-json 解压失败"))?;
        return from_value(serde_json::from_str(&json)?);
    }
    if let Some(body) = code_block(t, "json") {
        return from_value(serde_json::from_str(body.trim())?);
    }
    Err(anyhow!("不是 Excalidraw 文件"))
}

fn from_value(mut v: Value) -> Result<SketchDoc> {
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("excalidraw").to_string();
    if ty != "excalidraw" && ty != "excalidraw/clipboard" {
        return Err(anyhow!("不支持的类型：{ty}"));
    }
    if ty == "excalidraw/clipboard" {
        v["type"] = json!("excalidraw");
    }
    if v.get("elements").is_none() {
        return Err(anyhow!("缺少 elements"));
    }
    let mut d: SketchDoc = serde_json::from_value(v)?;
    // 文件里的 null 字段统一成默认值
    for e in d.elements.iter_mut() {
        if e.kind == "text" && e.text.is_none() {
            e.text = Some(String::new());
        }
    }
    Ok(d)
}

fn code_block<'a>(t: &'a str, lang: &str) -> Option<&'a str> {
    let open = format!("```{lang}");
    let start = t.find(&open)? + open.len();
    let rest = &t[start..];
    let rest = rest.strip_prefix('\r').unwrap_or(rest);
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let end = rest.find("```")?;
    Some(&rest[..end])
}

/// 把 files 里的 dataURL 落盘到资源目录，对象行只存路径。
pub fn externalize_files(d: &mut SketchDoc, assets: &Path) {
    let _ = std::fs::create_dir_all(assets);
    for f in d.files.values_mut() {
        if f.data_url.is_empty() {
            continue;
        }
        let Some((head, b64)) = f.data_url.split_once(',') else { continue };
        let mime = head.trim_start_matches("data:").split(';').next().unwrap_or("image/png").to_string();
        let bytes = if head.contains("base64") {
            base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()
        } else {
            Some(urlencoding_decode(b64).into_bytes())
        };
        if let Some(bytes) = bytes {
            if let Some(p) = crate::convert::save_image_bytes(&bytes, ext_of_mime(&mime), assets) {
                f.local_path = Some(p);
                if f.mime_type.is_empty() {
                    f.mime_type = mime;
                }
                f.data_url.clear();
            }
        }
    }
}

fn urlencoding_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 本地图片 → dataURL（导出 / 复制到剪贴板时）。
fn inline_files(d: &mut SketchDoc) {
    for f in d.files.values_mut() {
        if let Some(p) = f.local_path.take() {
            if let Ok(bytes) = std::fs::read(&p) {
                let mime = if f.mime_type.is_empty() { mime_of(&p).to_string() } else { f.mime_type.clone() };
                f.data_url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
                f.mime_type = mime;
            }
        }
    }
}

/// 导出成官方 `.excalidraw` 文本（图片内联，未引用的图片去掉）。
pub fn export_json(d: &SketchDoc) -> String {
    let mut d = d.clone();
    let used: std::collections::HashSet<String> = d.live().filter_map(|e| e.file_id().map(|s| s.to_string())).collect();
    d.files.retain(|k, _| used.contains(k));
    inline_files(&mut d);
    d.source = Some("https://stonebrook.app".into());
    serde_json::to_string_pretty(&d).unwrap_or_default()
}

/// 官方剪贴板格式（excalidraw.com 里 ⌘V 可直接粘贴）。
pub fn to_clipboard(d: &SketchDoc, ids: &[String]) -> String {
    let mut sub = d.clone();
    sub.elements.retain(|e| ids.contains(&e.id) && !e.is_deleted);
    let used: std::collections::HashSet<String> = sub.elements.iter().filter_map(|e| e.file_id().map(|s| s.to_string())).collect();
    sub.files.retain(|k, _| used.contains(k));
    inline_files(&mut sub);
    let v = json!({ "type": "excalidraw/clipboard", "elements": sub.elements, "files": sub.files });
    serde_json::to_string(&v).unwrap_or_default()
}

/// 从剪贴板文本解析元素（官方剪贴板 / 整个文件 / Obsidian）。
pub fn from_clipboard(text: &str) -> Option<SketchDoc> {
    let t = text.trim();
    if !(t.starts_with('{') && t.contains("\"elements\"")) && !t.contains("```compressed-json") {
        return None;
    }
    parse(t).ok()
}

// ───────────── LZ-String（decompressFromBase64）─────────────
// 移植自 pieroxy/lz-string（MIT / WTFPL），Obsidian Excalidraw 插件用它压缩 JSON。

const KEY_B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";

pub fn lz_decompress_base64(input: &str) -> Option<String> {
    if input.is_empty() {
        return Some(String::new());
    }
    let mut lut = [0u32; 256];
    for (i, c) in KEY_B64.iter().enumerate() {
        lut[*c as usize] = i as u32;
    }
    let bytes = input.as_bytes();
    let get = |i: usize| -> u32 { bytes.get(i).map(|b| lut[*b as usize]).unwrap_or(0) };
    lz_decompress(bytes.len(), 32, get)
}

fn lz_decompress(length: usize, reset: u32, get: impl Fn(usize) -> u32) -> Option<String> {
    let mut dict: Vec<Vec<u16>> = vec![vec![0], vec![1], vec![2]];
    let mut enlarge_in: u32 = 4;
    let mut dict_size: usize = 4;
    let mut num_bits: u32 = 3;
    let mut result: Vec<u16> = vec![];
    let mut val = get(0);
    let mut position = reset;
    let mut index = 1usize;
    let read = |n: u32, val: &mut u32, position: &mut u32, index: &mut usize| -> u32 {
        let mut bits = 0u32;
        let maxpower = 1u32 << n;
        let mut power = 1u32;
        while power != maxpower {
            let resb = *val & *position;
            *position >>= 1;
            if *position == 0 {
                *position = reset;
                *val = get(*index);
                *index += 1;
            }
            bits |= if resb > 0 { 1 } else { 0 } * power;
            power <<= 1;
        }
        bits
    };
    let c: Vec<u16> = match read(2, &mut val, &mut position, &mut index) {
        0 => vec![read(8, &mut val, &mut position, &mut index) as u16],
        1 => vec![read(16, &mut val, &mut position, &mut index) as u16],
        _ => return Some(String::new()),
    };
    dict.push(c.clone());
    let mut w = c.clone();
    result.extend_from_slice(&c);
    loop {
        if index > length + 1 {
            return None;
        }
        let mut cc = read(num_bits, &mut val, &mut position, &mut index) as usize;
        match cc {
            0 | 1 => {
                let b = read(if cc == 0 { 8 } else { 16 }, &mut val, &mut position, &mut index) as u16;
                dict.push(vec![b]);
                dict_size += 1;
                cc = dict_size - 1;
                enlarge_in -= 1;
            }
            2 => return Some(String::from_utf16_lossy(&result)),
            _ => {}
        }
        if enlarge_in == 0 {
            enlarge_in = 1 << num_bits;
            num_bits += 1;
        }
        let entry: Vec<u16> = if cc < dict.len() {
            dict[cc].clone()
        } else if cc == dict_size {
            let mut e = w.clone();
            e.push(w[0]);
            e
        } else {
            return None;
        };
        result.extend_from_slice(&entry);
        let mut ne = w.clone();
        ne.push(entry[0]);
        dict.push(ne);
        dict_size += 1;
        enlarge_in -= 1;
        w = entry;
        if enlarge_in == 0 {
            enlarge_in = 1 << num_bits;
            num_bits += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lz_vector_from_reference_impl() {
        // node -e 'require("lz-string").compressToBase64(JSON.stringify({...}))'（lz-string 1.5.0）
        let packed = "N4IgLgngDgpiBcIYA8DGBDANgSwCYCd0B3EAGhADcZ8BnbAewDsEAmcmTGAWxkbBoQBtUHgTgAjGXDQ4iMCjBTkCceQityRPGAAWCAMwAGcjpjYA5jsXwWAVnLzk1kIAN5QL6aAAh3Yp9fBexGLAAVBTF3Lx8AXwBdcnQoKABlMHR5BGAo8gAzbE4BeEyooA";
        let s = lz_decompress_base64(packed).unwrap();
        assert!(s.contains("你好 hi"), "{s}");
        let d = parse(&s).unwrap();
        assert_eq!(d.elements[0].text.as_deref(), Some("你好 hi"));
    }

    #[test]
    fn obsidian_markdown() {
        let md = "---\nexcalidraw-plugin: parsed\n---\n# Excalidraw Data\n## Text Elements\n你好 hi ^t1\n\n%%\n## Drawing\n```compressed-json\nN4IgLgngDgpiBcIYA8DGBDANgSwCYCd0B3EAGhADcZ8BnbAewDsEAmcmTGAWxkbBoQBtUHgTgAjGXDQ4iMCjBTkCceQityRPGAAWCAMwAGcjpjY\nA5jsXwWAVnLzk1kIAN5QL6aAAh3Yp9fBexGLAAVBTF3Lx8AXwBdcnQoKABlMHR5BGAo8gAzbE4BeEyooA\n```\n%%";
        let d = parse(md).unwrap();
        assert_eq!(d.elements.len(), 1);
    }

    #[test]
    fn official_fixture_roundtrip() {
        let src = include_str!("../../tests/fixtures/sketch/official.excalidraw");
        let orig: Value = serde_json::from_str(src).unwrap();
        let d = parse(src).unwrap();
        let out: Value = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        // 结构等价：所有元素的所有字段原样保留
        assert_eq!(orig["elements"], out["elements"]);
        assert_eq!(orig["appState"], out["appState"]);
        assert_eq!(orig["files"], out["files"]);
        // 不支持的类型原样保留
        assert!(d.elements.iter().any(|e| e.kind == "embeddable"));
    }

    #[test]
    fn files_externalize_and_inline() {
        let src = include_str!("../../tests/fixtures/sketch/official.excalidraw");
        let mut d = parse(src).unwrap();
        let dir = std::env::temp_dir().join(format!("sb-sketch-{}", new_id()));
        externalize_files(&mut d, &dir);
        let f = d.files.values().next().unwrap();
        assert!(f.data_url.is_empty());
        assert!(std::path::Path::new(f.local_path.as_ref().unwrap()).exists());
        let stored = serde_json::to_string(&d).unwrap();
        assert!(!stored.contains("base64,"));
        let out: Value = serde_json::from_str(&export_json(&d)).unwrap();
        let file = out["files"].as_object().unwrap().values().next().unwrap();
        assert!(file["dataURL"].as_str().unwrap().starts_with("data:image/png;base64,"));
        assert!(file.get("stonebrookPath").is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn clipboard_roundtrip() {
        let mut d = SketchDoc::new();
        let r = Element::new("rectangle", 0., 0.);
        let id = r.id.clone();
        d.elements.push(r);
        let s = to_clipboard(&d, std::slice::from_ref(&id));
        assert!(s.contains("excalidraw/clipboard"));
        let back = from_clipboard(&s).unwrap();
        assert_eq!(back.elements[0].id, id);
        assert!(from_clipboard("hello").is_none());
    }
}
