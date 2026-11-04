//! 草图 → SVG（导出、复制为图片、文档导出 HTML）。PNG 走 `canvas::svg::svg_to_png`。

use super::geom::{self, R};
use super::model::*;
use super::render::{cached, color, font_family, Prim, Xf};
use crate::canvas::{Col, PathCmd};
use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn d_attr(cmds: &[PathCmd]) -> String {
    let mut d = String::new();
    for c in cmds {
        match *c {
            PathCmd::Move(p) => write!(d, "M{:.2} {:.2} ", p.x, p.y).ok(),
            PathCmd::Line(p) => write!(d, "L{:.2} {:.2} ", p.x, p.y).ok(),
            PathCmd::Cubic(a, b, p) => write!(d, "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ", a.x, a.y, b.x, b.y, p.x, p.y).ok(),
            PathCmd::Close => write!(d, "Z ").ok(),
        };
    }
    d
}

fn paint_attr(name: &str, c: Col) -> String {
    if c.alpha() < 0.999 {
        format!("{name}=\"{}\" {name}-opacity=\"{:.3}\"", c.hex(), c.alpha())
    } else {
        format!("{name}=\"{}\"", c.hex())
    }
}

/// 导出选项
#[derive(Clone, Copy)]
pub struct SvgOpts {
    pub dark: bool,
    pub background: bool,
    pub padding: f64,
}

impl Default for SvgOpts {
    fn default() -> Self {
        SvgOpts { dark: false, background: true, padding: 10. }
    }
}

/// 只导出 ids 里的元素（空 = 全部）
pub fn to_svg(doc: &SketchDoc, only: &[String], o: SvgOpts) -> String {
    let els: Vec<&Element> = doc.live().filter(|e| only.is_empty() || only.contains(&e.id)).collect();
    let r = geom::bounds_of(els.iter().copied()).unwrap_or(R::xywh(0., 0., 100., 60.)).inflate(o.padding);
    let (w, h) = (r.w().ceil(), r.h().ceil());
    let mut out = String::new();
    writeln!(out, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"{:.2} {:.2} {w} {h}\">", r.x0, r.y0).ok();
    out.push_str("<!-- svg-source:excalidraw (stonebrook) -->\n");
    if o.background {
        let bg = color(&doc.bg_color(), 100., o.dark).unwrap_or(Col::rgb(0xffffff));
        writeln!(out, "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{w}\" height=\"{h}\" {}/>", r.x0, r.y0, paint_attr("fill", bg)).ok();
    }
    // 框架裁剪
    let mut clip_n = 0;
    for e in &els {
        let prims = cached(doc, e, o.dark);
        let xf = Xf::of(e);
        let mut clip_open = false;
        if let Some(f) = e.frame_id().and_then(|f| doc.get(f)) {
            clip_n += 1;
            writeln!(
                out,
                "<clipPath id=\"f{clip_n}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath><g clip-path=\"url(#f{clip_n})\">",
                f.x, f.y, f.width, f.height
            )
            .ok();
            clip_open = true;
        }
        let deg = (xf.angle as f64).to_degrees();
        writeln!(out, "<g transform=\"rotate({deg:.3} {:.2} {:.2}) translate({:.2} {:.2})\">", xf.cx, xf.cy, xf.ox, xf.oy).ok();
        for p in prims.iter() {
            prim_svg(&mut out, doc, p);
        }
        out.push_str("</g>\n");
        if clip_open {
            out.push_str("</g>\n");
        }
    }
    out.push_str("</svg>\n");
    out
}

fn prim_svg(out: &mut String, doc: &SketchDoc, p: &Prim) {
    match p {
        Prim::Fill { cmds, color } => {
            writeln!(out, "<path d=\"{}\" {} stroke=\"none\"/>", d_attr(cmds), paint_attr("fill", *color)).ok();
        }
        Prim::Stroke { cmds, color, width, dash, .. } => {
            let dash = dash.map(|(a, b)| format!(" stroke-dasharray=\"{a} {b}\"")).unwrap_or_default();
            writeln!(
                out,
                "<path d=\"{}\" fill=\"none\" {} stroke-width=\"{width}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{dash}/>",
                d_attr(cmds),
                paint_attr("stroke", *color)
            )
            .ok();
        }
        Prim::Text { lines, x, y, w, size, line_h, color, family, align, bg } => {
            let (_, fam) = font_family(*family);
            let (anchor, tx) = match align {
                1 => ("middle", x + w / 2.),
                2 => ("end", x + w),
                _ => ("start", *x),
            };
            if let Some(bg) = bg {
                writeln!(out, "<rect x=\"{}\" y=\"{y}\" width=\"{}\" height=\"{}\" {}/>", x - 4., w + 8., line_h * lines.len() as f32, paint_attr("fill", *bg))
                    .ok();
            }
            for (i, l) in lines.iter().enumerate() {
                // 基线：行顶 + 行高的中部偏下（与屏幕绘制对齐）
                let by = y + line_h * i as f32 + line_h / 2. + size * 0.35;
                writeln!(
                    out,
                    "<text x=\"{tx:.2}\" y=\"{by:.2}\" font-family=\"{}\" font-size=\"{size}\" text-anchor=\"{anchor}\" {} style=\"white-space: pre\">{}</text>",
                    esc(fam),
                    paint_attr("fill", *color),
                    esc(l)
                )
                .ok();
            }
        }
        Prim::Image { w, h, path } => {
            let data = path.as_deref().and_then(|p| std::fs::read(p).ok().map(|b| (p.to_string(), b)));
            match data {
                Some((p, bytes)) => {
                    use base64::Engine;
                    let mime = super::io::mime_of(&p);
                    writeln!(
                        out,
                        "<image x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" href=\"data:{mime};base64,{}\"/>",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    )
                    .ok();
                }
                None => {
                    let _ = doc;
                    writeln!(out, "<rect width=\"{w}\" height=\"{h}\" fill=\"#868e96\" fill-opacity=\"0.15\"/>").ok();
                }
            }
        }
    }
}

/// 导出 PNG（scale：1x / 2x / 3x）
pub fn to_png(doc: &SketchDoc, only: &[String], o: SvgOpts, scale: f32) -> Option<Vec<u8>> {
    crate::canvas::svg::svg_to_png(&to_svg(doc, only, o), scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_svg_and_png() {
        let mut d = SketchDoc::new();
        let mut r = Element::new("rectangle", 10., 10.);
        r.width = 120.;
        r.height = 60.;
        r.background_color = "#a5d8ff".into();
        r.fill_style = "hachure".into();
        r.angle = 0.3;
        let mut t = Element::new("text", 20., 100.);
        t.text = Some("你好 sketch".into());
        geom::fit_text(&mut t);
        let mut a = Element::new("arrow", 0., 0.);
        a.points = Some(vec![[0., 0.], [100., 40.], [200., 0.]]);
        a.normalize_points();
        d.elements = vec![r, t, a];
        let svg = to_svg(&d, &[], SvgOpts::default());
        assert!(svg.contains("你好 sketch"));
        assert!(svg.contains("rotate("));
        let png = to_png(&d, &[], SvgOpts::default(), 1.).expect("png");
        assert!(png.len() > 200);
    }
}
