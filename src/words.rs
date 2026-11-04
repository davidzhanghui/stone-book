//! 分词（L4）：含中日文的文本用系统 CFStringTokenizer 按词切分（⌥←/→、双击选词），
//! 其他文本沿用 Unicode 词边界。

use unicode_segmentation::UnicodeSegmentation;

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF))
}

/// 返回覆盖全文的分段 (字节起点, 片段)，与 `split_word_bound_indices` 相同语义。
pub fn segments(text: &str) -> Vec<(usize, &str)> {
    if !has_cjk(text) {
        return text.split_word_bound_indices().collect();
    }
    let cuts = tokenize(text);
    let mut out = Vec::new();
    let mut prev = 0;
    for (s, e) in cuts {
        if s > prev {
            // 词之间的空白 / 标点：再按 Unicode 规则细分
            for (i, w) in text[prev..s].split_word_bound_indices() {
                out.push((prev + i, w));
            }
        }
        if e > s {
            out.push((s, &text[s..e]));
        }
        prev = e;
    }
    if prev < text.len() {
        for (i, w) in text[prev..].split_word_bound_indices() {
            out.push((prev + i, w));
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn tokenize(text: &str) -> Vec<(usize, usize)> {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{kCFAllocatorDefault, CFRange, CFRelease};
    use core_foundation_sys::locale::CFLocaleCopyCurrent;
    use core_foundation_sys::string_tokenizer::*;

    // UTF-16 偏移 → 字节偏移
    let mut u16_to_byte = Vec::with_capacity(text.len() + 1);
    for (i, ch) in text.char_indices() {
        for _ in 0..ch.len_utf16() {
            u16_to_byte.push(i);
        }
    }
    u16_to_byte.push(text.len());
    let n16 = u16_to_byte.len() as isize - 1;
    let cf = CFString::new(text);
    let mut out = vec![];
    unsafe {
        let locale = CFLocaleCopyCurrent();
        let tk = CFStringTokenizerCreate(
            kCFAllocatorDefault,
            cf.as_concrete_TypeRef(),
            CFRange { location: 0, length: n16 },
            kCFStringTokenizerUnitWordBoundary,
            locale,
        );
        if tk.is_null() {
            CFRelease(locale as _);
            return text.split_word_bound_indices().map(|(i, w)| (i, i + w.len())).collect();
        }
        while CFStringTokenizerAdvanceToNextToken(tk) != kCFStringTokenizerTokenNone {
            let r = CFStringTokenizerGetCurrentTokenRange(tk);
            let (s, e) = (r.location.clamp(0, n16) as usize, (r.location + r.length).clamp(0, n16) as usize);
            out.push((u16_to_byte[s], u16_to_byte[e]));
        }
        CFRelease(tk as _);
        CFRelease(locale as _);
    }
    out
}

#[cfg(not(target_os = "macos"))]
fn tokenize(text: &str) -> Vec<(usize, usize)> {
    text.split_word_bound_indices().map(|(i, w)| (i, i + w.len())).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn chinese_words() {
        let segs: Vec<&str> = super::segments("我们今天发布新版本").into_iter().map(|s| s.1).collect();
        // 至少应当把若干汉字合并成词，而不是每个字一段
        assert!(segs.len() < 9, "{segs:?}");
        assert_eq!(segs.concat(), "我们今天发布新版本");
        let mixed: Vec<&str> = super::segments("用 Rust 写编辑器").into_iter().map(|s| s.1).collect();
        assert_eq!(mixed.concat(), "用 Rust 写编辑器");
        assert!(mixed.contains(&"Rust"));
    }
}
