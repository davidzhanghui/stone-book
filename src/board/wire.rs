//! 低保真线框控件（画板图形库「线框」分组 + 「线框」模板）：按钮、输入框、下拉、复选、Tab、导航栏、卡片、
//! 手机 / 浏览器外框……全部用普通图形拼成一组，插入后可以逐个编辑，不引入第三方素材。

use super::model::*;
use crate::canvas::{rect, Col, Pt, ShapeKind};

/// 线框控件：(编号, 名称)（图形库里的列表见 library.rs 的 WIRE，这里供测试遍历）
#[cfg(test)]
pub const WIDGETS: &[(u8, &str)] = &[
    (0, "按钮"),
    (1, "输入框"),
    (2, "下拉框"),
    (3, "复选框"),
    (4, "单选框"),
    (5, "开关"),
    (6, "搜索框"),
    (7, "Tab 栏"),
    (8, "导航栏"),
    (9, "列表"),
    (10, "卡片"),
    (11, "图片占位"),
    (12, "头像"),
    (13, "文本段落"),
    (14, "手机外框"),
    (15, "浏览器外框"),
];

const LINE: u32 = 0x8f959e;
const SOFT: u32 = 0xf2f3f5;
const INK: u32 = 0x1f2329;
const MUTED: u32 = 0x8f959e;
const ACCENT: u32 = 0x3370ff;

struct Kit<'a> {
    doc: &'a mut BoardDoc,
    out: Vec<El>,
    dark: bool,
}

impl Kit<'_> {
    fn shape(&mut self, k: ShapeKind, x: f32, y: f32, w: f32, h: f32, fill: Option<u32>, stroke: Option<u32>, text: &str) -> &mut El {
        let mut e = self.doc.new_el(ElKind::Shape(k), rect(x, y, w, h), self.dark);
        e.fill = fill.map(Col::rgb);
        match stroke {
            Some(s) => e.stroke = Col::rgb(s),
            None => e.no_stroke = true,
        }
        e.stroke_w = 1.2;
        e.text = text.to_string();
        e.font_size = 14.;
        e.text_color = Some(Col::rgb(INK));
        self.out.push(e);
        self.out.last_mut().unwrap()
    }

    fn text(&mut self, x: f32, y: f32, w: f32, h: f32, s: &str, size: f32, color: u32) -> &mut El {
        let mut e = self.doc.new_el(ElKind::Text, rect(x, y, w, h), self.dark);
        e.text = s.to_string();
        e.font_size = size;
        e.text_color = Some(Col::rgb(color));
        e.fill = None;
        e.v_align = 2;
        self.out.push(e);
        self.out.last_mut().unwrap()
    }

    fn bar(&mut self, x: f32, y: f32, w: f32, h: f32, col: u32) {
        self.shape(ShapeKind::Rect, x, y, w, h, Some(col), None, "");
    }
}

/// 生成一个控件（左上角在 at），返回一组元素（已设同一 group）。
pub fn build(doc: &mut BoardDoc, kind: u8, at: Pt, dark: bool) -> Vec<El> {
    let (x, y) = (at.x, at.y);
    let mut k = Kit { doc, out: vec![], dark };
    match kind {
        0 => {
            k.shape(ShapeKind::RoundRect, x, y, 120., 40., Some(ACCENT), None, "按钮").text_color = Some(Col::rgb(0xffffff));
        }
        1 => {
            k.shape(ShapeKind::RoundRect, x, y, 240., 40., Some(0xffffff), Some(LINE), "");
            k.text(x + 12., y, 200., 40., "请输入", 14., MUTED);
        }
        2 => {
            k.shape(ShapeKind::RoundRect, x, y, 200., 40., Some(0xffffff), Some(LINE), "");
            k.text(x + 12., y, 150., 40., "请选择", 14., MUTED);
            k.text(x + 172., y, 20., 40., "▾", 14., MUTED);
        }
        3 => {
            k.shape(ShapeKind::RoundRect, x, y + 10., 20., 20., Some(ACCENT), None, "✓").text_color = Some(Col::rgb(0xffffff));
            k.text(x + 30., y, 120., 40., "选项", 14., INK);
        }
        4 => {
            k.shape(ShapeKind::Circle, x, y + 10., 20., 20., Some(0xffffff), Some(ACCENT), "");
            k.shape(ShapeKind::Circle, x + 6., y + 16., 8., 8., Some(ACCENT), None, "");
            k.text(x + 30., y, 120., 40., "选项", 14., INK);
        }
        5 => {
            k.shape(ShapeKind::Stadium, x, y + 8., 44., 24., Some(ACCENT), None, "");
            k.shape(ShapeKind::Circle, x + 23., y + 11., 18., 18., Some(0xffffff), None, "");
            k.text(x + 54., y, 100., 40., "开关", 14., INK);
        }
        6 => {
            k.shape(ShapeKind::Stadium, x, y, 260., 36., Some(SOFT), None, "");
            k.text(x + 14., y, 20., 36., "⌕", 16., MUTED);
            k.text(x + 36., y, 200., 36., "搜索", 14., MUTED);
        }
        7 => {
            for (i, s) in ["推荐", "关注", "热门", "最新"].iter().enumerate() {
                let xi = x + i as f32 * 80.;
                k.text(xi, y, 80., 36., s, 14., if i == 0 { INK } else { MUTED }).text_align = 2;
            }
            k.bar(x + 24., y + 36., 32., 3., ACCENT);
            k.bar(x, y + 40., 320., 1., 0xdee0e3);
        }
        8 => {
            k.shape(ShapeKind::Rect, x, y, 360., 52., Some(0xffffff), Some(0xdee0e3), "");
            k.text(x + 12., y, 30., 52., "‹", 22., INK);
            k.text(x + 80., y, 200., 52., "页面标题", 16., INK).text_align = 2;
            k.text(x + 318., y, 30., 52., "⋯", 18., INK);
        }
        9 => {
            for i in 0..3 {
                let yi = y + i as f32 * 64.;
                k.shape(ShapeKind::Circle, x + 12., yi + 12., 40., 40., Some(SOFT), None, "");
                k.text(x + 64., yi + 10., 220., 22., "列表标题", 15., INK);
                k.text(x + 64., yi + 32., 220., 20., "辅助说明文字", 12., MUTED);
                k.bar(x + 64., yi + 63., 280., 1., 0xdee0e3);
            }
        }
        10 => {
            k.shape(ShapeKind::RoundRect, x, y, 240., 220., Some(0xffffff), Some(0xdee0e3), "");
            k.shape(ShapeKind::Rect, x + 1., y + 1., 238., 120., Some(SOFT), None, "图片");
            k.text(x + 14., y + 130., 210., 26., "卡片标题", 16., INK).bold = true;
            k.text(x + 14., y + 158., 210., 40., "一段描述文字，说明卡片的内容。", 13., MUTED);
        }
        11 => {
            k.shape(ShapeKind::Rect, x, y, 200., 140., Some(SOFT), Some(LINE), "");
            k.text(x, y, 200., 140., "🖼 图片", 14., MUTED).text_align = 2;
        }
        12 => {
            k.shape(ShapeKind::Circle, x, y, 48., 48., Some(SOFT), Some(LINE), "👤");
        }
        13 => {
            k.text(x, y, 300., 28., "段落标题", 18., INK).bold = true;
            for (i, w) in [300., 280., 300., 180.].iter().enumerate() {
                k.bar(x, y + 40. + i as f32 * 18., *w, 8., 0xdee0e3);
            }
        }
        14 => {
            k.shape(ShapeKind::RoundRect, x, y, 375., 760., Some(0xffffff), Some(INK), "").stroke_w = 3.;
            k.shape(ShapeKind::Stadium, x + 140., y + 12., 96., 22., Some(INK), None, "");
            k.text(x + 24., y + 10., 60., 26., "9:41", 13., INK);
            k.shape(ShapeKind::Stadium, x + 120., y + 742., 136., 5., Some(INK), None, "");
        }
        15 => {
            k.shape(ShapeKind::RoundRect, x, y, 960., 600., Some(0xffffff), Some(LINE), "");
            k.bar(x + 1., y + 1., 958., 40., SOFT);
            for (i, c) in [0xff5f57u32, 0xfebc2e, 0x28c840].iter().enumerate() {
                k.shape(ShapeKind::Circle, x + 14. + i as f32 * 18., y + 14., 12., 12., Some(*c), None, "");
            }
            k.shape(ShapeKind::Stadium, x + 200., y + 8., 560., 24., Some(0xffffff), None, "");
            k.text(x + 216., y + 8., 400., 24., "https://example.com", 12., MUTED);
        }
        _ => {}
    }
    let mut out = k.out;
    if out.len() > 1 {
        let g = doc.alloc_id();
        for e in out.iter_mut() {
            e.group = Some(g);
        }
    }
    out
}

/// 控件的外框尺寸（插入时居中用）。
pub fn size(kind: u8) -> (f32, f32) {
    match kind {
        0 => (120., 40.),
        1 => (240., 40.),
        2 => (200., 40.),
        3..=5 => (150., 40.),
        6 => (260., 36.),
        7 => (320., 41.),
        8 => (360., 52.),
        9 => (344., 192.),
        10 => (240., 220.),
        11 => (200., 140.),
        12 => (48., 48.),
        13 => (300., 120.),
        14 => (375., 760.),
        _ => (960., 600.),
    }
}

fn place(doc: &mut BoardDoc, kind: u8, x: f32, y: f32, dark: bool) {
    let els = build(doc, kind, crate::canvas::pt(x, y), dark);
    doc.els.extend(els);
}

fn label(doc: &mut BoardDoc, x: f32, y: f32, w: f32, s: &str, size: f32, dark: bool) {
    let mut k = Kit { doc, out: vec![], dark };
    k.text(x, y, w, size * 1.8, s, size, INK);
    let out = k.out;
    doc.els.extend(out);
}

/// 模板：登录页（手机）
pub fn login_page(dark: bool) -> BoardDoc {
    let mut d = BoardDoc::new(false);
    place(&mut d, 14, 0., 0., dark);
    label(&mut d, 40., 120., 300., "欢迎回来", 26., dark);
    label(&mut d, 40., 170., 300., "登录以继续使用", 14., dark);
    place(&mut d, 1, 40., 240., dark);
    place(&mut d, 1, 40., 300., dark);
    place(&mut d, 3, 40., 356., dark);
    place(&mut d, 0, 40., 420., dark);
    if let Some(b) = d.els.last_mut() {
        b.w = 295.;
        b.text = "登录".into();
    }
    label(&mut d, 40., 480., 300., "还没有账号？立即注册", 13., dark);
    // 输入框与按钮同宽
    for e in d.els.iter_mut().filter(|e| e.w == 240. && e.h == 40.) {
        e.w = 295.;
    }
    d
}

/// 模板：列表页（手机）
pub fn list_page(dark: bool) -> BoardDoc {
    let mut d = BoardDoc::new(false);
    place(&mut d, 14, 0., 0., dark);
    place(&mut d, 8, 8., 48., dark);
    place(&mut d, 6, 58., 112., dark);
    place(&mut d, 7, 28., 164., dark);
    place(&mut d, 9, 16., 220., dark);
    place(&mut d, 9, 16., 412., dark);
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_widget_builds_a_group() {
        for &(k, name) in WIDGETS {
            let mut d = BoardDoc::new(false);
            let els = build(&mut d, k, crate::canvas::pt(0., 0.), false);
            assert!(!els.is_empty(), "{name} 为空");
            if els.len() > 1 {
                let g = els[0].group;
                assert!(g.is_some() && els.iter().all(|e| e.group == g), "{name} 没有组成一组");
            }
            let mut ids: Vec<u64> = els.iter().map(|e| e.id).collect();
            ids.dedup();
            assert_eq!(ids.len(), els.len());
        }
        for t in [login_page, list_page] {
            assert!(t(false).els.len() > 10);
        }
    }
}
