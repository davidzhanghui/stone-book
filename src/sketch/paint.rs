//! 用 GPUI 画草图：图元 → 路径 / 文字 / 位图。
//!
//! 文字与图片在屏幕上不旋转（GPUI 0.2.2 的 `paint_glyph` / `paint_image` 不接受变换），
//! 只把它们放在旋转后的中心；SVG / PNG 导出是正确旋转的。

use super::geom::{self, R};
use super::model::*;
use super::render::{cached, color, font_family, Prim, Xf};
use crate::canvas::paint::View;
use crate::canvas::{pt, Col, PathCmd};
use gpui::{fill, point, px, size, App, Bounds, ContentMask, Corners, Hsla, PathBuilder, PathStyle, Pixels, SharedString, StrokeOptions, TextRun, Window};
use lyon_tessellation::{LineCap, LineJoin};

fn build(cmds: &[PathCmd], view: &View, stroke: Option<(f32, bool)>, dash: Option<(f32, f32)>) -> Option<gpui::Path<Pixels>> {
    if cmds.is_empty() {
        return None;
    }
    let mut b = match stroke {
        Some((w, round)) => {
            let w = (w * view.scale).max(0.6);
            if round {
                let o = StrokeOptions::default().with_line_width(w).with_line_cap(LineCap::Round).with_line_join(LineJoin::Round);
                PathBuilder::stroke(px(w)).with_style(PathStyle::Stroke(o))
            } else {
                let o = StrokeOptions::default().with_line_width(w).with_line_join(LineJoin::Round).with_line_cap(LineCap::Round);
                PathBuilder::stroke(px(w)).with_style(PathStyle::Stroke(o))
            }
        }
        None => PathBuilder::fill(),
    };
    if let Some((on, off)) = dash {
        b = b.dash_array(&[px((on * view.scale).max(0.5)), px((off * view.scale).max(0.5))]);
    }
    for c in cmds {
        match *c {
            PathCmd::Move(p) => b.move_to(view.p(p)),
            PathCmd::Line(p) => b.line_to(view.p(p)),
            PathCmd::Cubic(a, c2, p) => b.cubic_bezier_to(view.p(p), view.p(a), view.p(c2)),
            PathCmd::Close => b.close(),
        }
    }
    b.build().ok()
}

fn text_run(len: usize, family: &'static str, color: Hsla) -> TextRun {
    TextRun { len, font: gpui::font(family), color, background_color: None, underline: None, strikethrough: None }
}

/// 画一个元素的图元。
pub fn paint_prims(prims: &[Prim], xf: &Xf, view: &View, window: &mut Window, cx: &mut App) {
    for p in prims {
        match p {
            Prim::Fill { cmds, color } => {
                if let Some(path) = build(&xf.cmds(cmds), view, None, None) {
                    window.paint_path(path, color.hsla());
                }
            }
            Prim::Stroke { cmds, color, width, dash, round } => {
                if let Some(path) = build(&xf.cmds(cmds), view, Some((*width, *round)), *dash) {
                    window.paint_path(path, color.hsla());
                }
            }
            Prim::Text { lines, x, y, w, size: fs, line_h, color, family, align, bg } => {
                let s = fs * view.scale;
                if s < 2. {
                    continue;
                }
                let (fam, _) = font_family(*family);
                // 文字框中心经过旋转后的位置；文字本身水平绘制
                let h = *line_h * lines.len() as f32;
                let c_local = pt(x + w / 2., y + h / 2.);
                let c = view.p(xf.apply(c_local));
                let top = c.y - px(h * view.scale / 2.);
                let left = c.x - px(w * view.scale / 2.);
                if let Some(bg) = bg {
                    window.paint_quad(fill(Bounds::new(point(left - px(4.), top), size(px(w * view.scale) + px(8.), px(h * view.scale))), bg.hsla()));
                }
                for (i, line) in lines.iter().enumerate() {
                    if line.is_empty() {
                        continue;
                    }
                    let ss: SharedString = line.clone().into();
                    let shaped = window.text_system().shape_line(ss.clone(), px(s), &[text_run(ss.len(), fam, color.hsla())], None);
                    let lw = shaped.width;
                    let xx = match align {
                        1 => left + (px(w * view.scale) - lw) / 2.,
                        2 => left + px(w * view.scale) - lw,
                        _ => left,
                    };
                    let yy = top + px(line_h * view.scale * i as f32);
                    shaped.paint(point(xx, yy), px(line_h * view.scale), window, cx).ok();
                }
            }
            Prim::Image { w, h, path } => {
                let c = view.p(xf.apply(pt(w / 2., h / 2.)));
                let b = Bounds::new(point(c.x - px(w * view.scale / 2.), c.y - px(h * view.scale / 2.)), size(px(w * view.scale), px(h * view.scale)));
                match path.as_deref().and_then(|p| crate::canvas::paint::load_image(p, window)) {
                    Some(img) => {
                        window.paint_image(b, Corners::default(), img, 0, false).ok();
                    }
                    None => {
                        window.paint_quad(fill(b, gpui::hsla(0., 0., 0.5, 0.15)));
                    }
                }
            }
        }
    }
}

/// 画整张草图（编辑器与文档内预览共用）。hide：正在编辑文字的元素不画。
pub fn paint_doc(doc: &SketchDoc, view: View, clip: Bounds<Pixels>, dark: bool, hide: Option<&str>, erase: &[String], window: &mut Window, cx: &mut App) {
    // 框架里的元素按框架裁剪
    let frames: std::collections::HashMap<&str, R> = doc.live().filter(|e| e.is_frame()).map(|e| (e.id.as_str(), R::xywh(e.x, e.y, e.width, e.height))).collect();
    for e in doc.live() {
        if hide == Some(e.id.as_str()) {
            continue;
        }
        let b = geom::bounds(e).inflate(e.stroke_width * 4. + 30.);
        let sb = view.r(crate::canvas::rect(b.x0 as f32, b.y0 as f32, b.w() as f32, b.h() as f32));
        if !sb.intersects(&clip) {
            continue;
        }
        let prims = cached(doc, e, dark);
        let xf = Xf::of(e);
        let faded = erase.iter().any(|x| *x == e.id);
        let mut draw = |window: &mut Window, cx: &mut App| {
            if faded {
                let ghost: Vec<Prim> = prims.iter().cloned().map(|p| fade(p, 0.25)).collect();
                paint_prims(&ghost, &xf, &view, window, cx);
            } else {
                paint_prims(&prims, &xf, &view, window, cx);
            }
        };
        match e.frame_id().and_then(|f| frames.get(f)) {
            Some(fr) => {
                let fb = view.r(crate::canvas::rect(fr.x0 as f32, fr.y0 as f32, fr.w() as f32, fr.h() as f32));
                window.with_content_mask(Some(ContentMask { bounds: fb }), |window| draw(window, cx));
            }
            None => draw(window, cx),
        }
    }
}

fn fade(p: Prim, k: f32) -> Prim {
    let f = |c: Col| c.with_alpha(c.alpha() * k);
    match p {
        Prim::Fill { cmds, color } => Prim::Fill { cmds, color: f(color) },
        Prim::Stroke { cmds, color, width, dash, round } => Prim::Stroke { cmds, color: f(color), width, dash, round },
        Prim::Text { lines, x, y, w, size, line_h, color, family, align, bg } => Prim::Text { lines, x, y, w, size, line_h, color: f(color), family, align, bg: bg.map(f) },
        other => other,
    }
}

/// 画布底色
pub fn bg_hsla(doc: &SketchDoc, dark: bool) -> Hsla {
    color(&doc.bg_color(), 100., dark).unwrap_or(if dark { Col::rgb(0x121212) } else { Col::rgb(0xffffff) }).hsla()
}

/// 把整张草图缩放画进 bounds（文档内预览）。
pub fn paint_fit(doc: &SketchDoc, bounds: Bounds<Pixels>, dark: bool, window: &mut Window, cx: &mut App) {
    let Some(r) = geom::bounds_of(doc.live()) else { return };
    let r = r.inflate(12.);
    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let s = (bw / r.w() as f32).min(bh / r.h() as f32).min(1.);
    let ox = bounds.origin.x + px((bw - r.w() as f32 * s) / 2. - r.x0 as f32 * s);
    let oy = bounds.origin.y + px((bh - r.h() as f32 * s) / 2. - r.y0 as f32 * s);
    paint_doc(doc, View { origin: point(ox, oy), scale: s }, bounds, dark, None, &[], window, cx);
}
