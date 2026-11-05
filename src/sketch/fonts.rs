//! 草图手写字体「Stonebrook Sketch」：Excalifont（拉丁）+ 小赖（GB2312 常用汉字）子集合并，均为 SIL OFL 1.1。
//!
//! 字体随程序内嵌（zlib 压缩），启动时注册给 GPUI 与 resvg；文字宽度用同一份字形的真实步进测量，
//! 保证换行、编辑框、屏幕绘制、SVG / PNG 导出一致。

use std::io::Read;
use std::sync::OnceLock;

pub const HAND_FAMILY: &str = "Stonebrook Sketch";

static PACKED: &[u8] = include_bytes!("../../assets/fonts/StonebrookSketch-Regular.ttf.z");

/// 解压后的 TTF
pub fn hand_ttf() -> &'static [u8] {
    static TTF: OnceLock<Vec<u8>> = OnceLock::new();
    TTF.get_or_init(|| {
        let mut out = Vec::with_capacity(3_400_000);
        flate2::read::ZlibDecoder::new(PACKED).read_to_end(&mut out).ok();
        out
    })
}

fn face() -> Option<&'static ttf_parser::Face<'static>> {
    static FACE: OnceLock<Option<ttf_parser::Face<'static>>> = OnceLock::new();
    FACE.get_or_init(|| ttf_parser::Face::parse(hand_ttf(), 0).ok()).as_ref()
}

/// 注册给 GPUI（屏幕绘制、编辑框）。
pub fn register(cx: &gpui::App) {
    let bytes = hand_ttf();
    if !bytes.is_empty() {
        cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(bytes)]).ok();
    }
}

/// 手写字体里某个字的步进（em 的倍数）；字体里没有的字返回 None。
pub fn hand_advance(c: char) -> Option<f64> {
    let f = face()?;
    let g = f.glyph_index(c)?;
    let adv = f.glyph_hor_advance(g)? as f64;
    Some(adv / f.units_per_em() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_has_latin_and_cjk() {
        assert!(hand_ttf().len() > 1_000_000);
        assert!(hand_advance('A').is_some_and(|a| a > 0.2 && a < 1.2));
        assert!(hand_advance('中').is_some_and(|a| a > 0.8 && a < 1.2));
        assert!(hand_advance('，').is_some());
    }
}
