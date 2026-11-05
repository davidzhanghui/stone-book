//! 草图（Excalidraw 兼容）：与画板并列的嵌入对象，官方 `.excalidraw` 结构即存储格式。
//!
//! - `model`：官方元素结构（未知字段原样保留）
//! - `geom`：旋转、命中、绑定、文字排版
//! - `render` / `paint` / `svg`：手绘图元生成、屏幕绘制、导出
//! - `io`：`.excalidraw` / `.excalidraw.md` / 剪贴板
//! - `view` / `interact` / `ops` / `ui`：全屏编辑器

#[cfg(test)]
mod editor_tests;
pub mod fonts;
pub mod geom;
pub mod icons;
mod interact;
pub mod io;
pub mod mermaid;
pub mod model;
mod ops;
pub mod paint;
pub mod render;
pub mod svg;
#[cfg(feature = "dev")]
mod test_drive;
mod tools;
mod ui;
pub mod view;

pub use model::SketchDoc;

/// 文档导出 Markdown 时的占位描述
pub fn to_markdown(d: &SketchDoc) -> String {
    let texts = d.texts();
    if texts.is_empty() {
        "[草图]\n".into()
    } else {
        format!("[草图：{}]\n", texts.join(" / "))
    }
}

/// 文档导出 HTML：内嵌 SVG
pub fn to_html(d: &SketchDoc) -> String {
    if d.is_empty() {
        return "<div class=\"sketch\"></div>".into();
    }
    format!("<div class=\"sketch\">{}</div>", svg::to_svg(d, &[], svg::SvgOpts::default()))
}

/// 文档内预览的显示尺寸（像素）：按内容比例，宽不超过 max_w、高不超过 440。
pub fn preview_size(d: &SketchDoc, max_w: f32) -> Option<(f32, f32)> {
    let r = geom::bounds_of(d.live())?.inflate(12.);
    let (w, h) = (r.w() as f32, r.h() as f32);
    let s = (max_w / w).min(1.).min(440. / h).max(0.05);
    Some((w * s, (h * s).max(40.)))
}
