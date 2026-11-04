//! 几何：旋转、包围盒、命中测试、箭头绑定、文字换行与容器排版。
//!
//! 绑定点算法（射线与矩形 / 菱形 / 椭圆轮廓求交）参考 gpui-excalidraw `core/binding.rs`
//! （MIT，Copyright (c) LittleGuest），并按官方语义改为「点相对 (x, y)」。

use super::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct V {
    pub x: f64,
    pub y: f64,
}

pub fn v(x: f64, y: f64) -> V {
    V { x, y }
}

impl V {
    pub fn add(self, o: V) -> V {
        v(self.x + o.x, self.y + o.y)
    }
    pub fn sub(self, o: V) -> V {
        v(self.x - o.x, self.y - o.y)
    }
    pub fn scale(self, s: f64) -> V {
        v(self.x * s, self.y * s)
    }
    pub fn len(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    pub fn dist(self, o: V) -> f64 {
        self.sub(o).len()
    }
    /// 绕 c 顺时针旋转 a 弧度（屏幕坐标系，y 向下）
    pub fn rot(self, a: f64, c: V) -> V {
        if a == 0. {
            return self;
        }
        let (s, co) = a.sin_cos();
        let d = self.sub(c);
        v(c.x + d.x * co - d.y * s, c.y + d.x * s + d.y * co)
    }
}

/// 轴对齐矩形
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct R {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl R {
    pub fn from_pts(pts: impl IntoIterator<Item = V>) -> Option<R> {
        let mut it = pts.into_iter();
        let f = it.next()?;
        let mut r = R { x0: f.x, y0: f.y, x1: f.x, y1: f.y };
        for p in it {
            r.x0 = r.x0.min(p.x);
            r.y0 = r.y0.min(p.y);
            r.x1 = r.x1.max(p.x);
            r.y1 = r.y1.max(p.y);
        }
        Some(r)
    }
    pub fn xywh(x: f64, y: f64, w: f64, h: f64) -> R {
        R { x0: x.min(x + w), y0: y.min(y + h), x1: x.max(x + w), y1: y.max(y + h) }
    }
    pub fn w(&self) -> f64 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f64 {
        self.y1 - self.y0
    }
    pub fn center(&self) -> V {
        v((self.x0 + self.x1) / 2., (self.y0 + self.y1) / 2.)
    }
    pub fn union(&self, o: &R) -> R {
        R { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
    pub fn inflate(&self, d: f64) -> R {
        R { x0: self.x0 - d, y0: self.y0 - d, x1: self.x1 + d, y1: self.y1 + d }
    }
    pub fn contains(&self, p: V) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }
    pub fn contains_r(&self, o: &R) -> bool {
        o.x0 >= self.x0 && o.x1 <= self.x1 && o.y0 >= self.y0 && o.y1 <= self.y1
    }
}

pub fn center(e: &Element) -> V {
    v(e.x + e.width / 2., e.y + e.height / 2.)
}

/// 线 / 自由画的点（世界坐标，未旋转）
pub fn abs_points(e: &Element) -> Vec<V> {
    e.pts().iter().map(|p| v(e.x + p[0], e.y + p[1])).collect()
}

/// 线的旋转中心：点的包围盒中心（官方以 x/y + 点包围盒为准）
pub fn linear_center(e: &Element) -> V {
    R::from_pts(abs_points(e)).map(|r| r.center()).unwrap_or(v(e.x, e.y))
}

pub fn rot_center(e: &Element) -> V {
    if e.points.is_some() && (e.is_linear() || e.kind == "freedraw") {
        linear_center(e)
    } else {
        center(e)
    }
}

/// 线的点（世界坐标，已旋转）
pub fn world_points(e: &Element) -> Vec<V> {
    let c = rot_center(e);
    abs_points(e).into_iter().map(|p| p.rot(e.angle, c)).collect()
}

/// 未旋转时的四角
pub fn corners(e: &Element) -> [V; 4] {
    let c = center(e);
    let (x0, y0, x1, y1) = (e.x, e.y, e.x + e.width, e.y + e.height);
    [v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)].map(|p| p.rot(e.angle, c))
}

/// 轴对齐包围盒（考虑旋转）
pub fn bounds(e: &Element) -> R {
    if e.points.is_some() && (e.is_linear() || e.kind == "freedraw") {
        let r = R::from_pts(world_points(e)).unwrap_or(R::xywh(e.x, e.y, 0., 0.));
        return r.inflate(e.stroke_width / 2.);
    }
    if e.kind == "ellipse" && e.angle != 0. {
        // 旋转椭圆的精确包围盒
        let (a, b) = (e.width / 2., e.height / 2.);
        let (s, c) = e.angle.sin_cos();
        let hw = ((a * c).powi(2) + (b * s).powi(2)).sqrt();
        let hh = ((a * s).powi(2) + (b * c).powi(2)).sqrt();
        let cc = center(e);
        return R { x0: cc.x - hw, y0: cc.y - hh, x1: cc.x + hw, y1: cc.y + hh };
    }
    R::from_pts(corners(e)).unwrap()
}

pub fn bounds_of<'a>(els: impl IntoIterator<Item = &'a Element>) -> Option<R> {
    els.into_iter().map(bounds).reduce(|a, b| a.union(&b))
}

fn dist_seg(p: V, a: V, b: V) -> f64 {
    let ab = b.sub(a);
    let l2 = ab.x * ab.x + ab.y * ab.y;
    if l2 < 1e-9 {
        return p.dist(a);
    }
    let t = ((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / l2;
    let t = t.clamp(0., 1.);
    p.dist(a.add(ab.scale(t)))
}

pub fn dist_polyline(p: V, pts: &[V]) -> f64 {
    if pts.len() == 1 {
        return p.dist(pts[0]);
    }
    pts.windows(2).map(|w| dist_seg(p, w[0], w[1])).fold(f64::MAX, f64::min)
}

/// 曲线（官方 roundness 线）用 Catmull-Rom 打平，命中测试与绑定都用它。
pub fn curve_points(pts: &[V], round: bool) -> Vec<V> {
    if !round || pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 0..pts.len() - 1 {
        let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
        let (p1, p2) = (pts[i], pts[i + 1]);
        let p3 = if i + 2 < pts.len() { pts[i + 2] } else { p2 };
        for k in 1..=10 {
            let t = k as f64 / 10.;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f64, b: f64, c: f64, d: f64| 0.5 * ((2. * b) + (-a + c) * t + (2. * a - 5. * b + 4. * c - d) * t2 + (-a + 3. * b - 3. * c + d) * t3);
            out.push(v(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)));
        }
    }
    out
}

/// 命中测试。tol 为世界坐标下的容差。
pub fn hit(e: &Element, p: V, tol: f64) -> bool {
    if e.is_deleted {
        return false;
    }
    if !bounds(e).inflate(tol + 4.).contains(p) {
        return false;
    }
    let w = e.stroke_width / 2. + tol;
    match e.kind.as_str() {
        "line" | "arrow" => {
            let pts = curve_points(&world_points(e), e.roundness().is_some() && !e.elbowed());
            if dist_polyline(p, &pts) <= w {
                return true;
            }
            // 闭合且有填充的线：内部也算
            e.has_fill() && pts.len() > 2 && pts[0].dist(*pts.last().unwrap()) < 1. && inside_poly(p, &pts)
        }
        "freedraw" => dist_polyline(p, &world_points(e)) <= w + e.stroke_width,
        _ => {
            // 转到元素自身坐标系
            let c = center(e);
            let q = p.rot(-e.angle, c);
            let (x0, y0, x1, y1) = (e.x, e.y, e.x + e.width, e.y + e.height);
            let filled = e.has_fill() || matches!(e.kind.as_str(), "text" | "image" | "iframe" | "embeddable");
            match e.kind.as_str() {
                "ellipse" => {
                    let (a, b) = ((e.width / 2.).max(1.), (e.height / 2.).max(1.));
                    let d = ((q.x - c.x) / a).powi(2) + ((q.y - c.y) / b).powi(2);
                    let inner_a = (a - w).max(0.5);
                    let inner_b = (b - w).max(0.5);
                    let din = ((q.x - c.x) / inner_a).powi(2) + ((q.y - c.y) / inner_b).powi(2);
                    let outer = ((q.x - c.x) / (a + w)).powi(2) + ((q.y - c.y) / (b + w)).powi(2);
                    outer <= 1. && (filled || din >= 1. || d >= 1.)
                }
                "diamond" => {
                    let poly = [v(c.x, y0), v(x1, c.y), v(c.x, y1), v(x0, c.y), v(c.x, y0)];
                    dist_polyline(q, &poly) <= w || (filled && inside_poly(q, &poly))
                }
                "frame" | "magicframe" => {
                    // 框架：点边框或名称标签
                    let edge = dist_polyline(q, &[v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1), v(x0, y0)]) <= w + 2.;
                    edge || R::xywh(e.x, e.y - 24., e.width.min(220.), 22.).contains(q)
                }
                _ => {
                    let r = R::xywh(x0, y0, e.width, e.height);
                    if filled {
                        r.inflate(w).contains(q)
                    } else {
                        let poly = [v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1), v(x0, y0)];
                        dist_polyline(q, &poly) <= w
                    }
                }
            }
        }
    }
}

pub fn inside_poly(p: V, poly: &[V]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y + 1e-12) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ───────────── 绑定 ─────────────

pub const BIND_DIST: f64 = 14.;

/// 从 from 指向目标中心的射线与目标轮廓的交点（考虑旋转）。
pub fn focus_point(t: &Element, from: V) -> V {
    let w = t.width.abs().max(0.0001);
    let h = t.height.abs().max(0.0001);
    let c = center(t);
    let lf = from.rot(-t.angle, c);
    let (dx, dy) = (lf.x - c.x, lf.y - c.y);
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return v(c.x + w / 2., c.y).rot(t.angle, c);
    }
    let (hw, hh) = (w / 2., h / 2.);
    let local = match t.kind.as_str() {
        "diamond" => {
            let s = 1. / (dx.abs() / hw + dy.abs() / hh).max(1e-9);
            v(c.x + dx * s, c.y + dy * s)
        }
        "ellipse" => {
            let s = 1. / ((dx / hw).powi(2) + (dy / hh).powi(2)).sqrt().max(1e-9);
            v(c.x + dx * s, c.y + dy * s)
        }
        _ => {
            let tx = if dx.abs() > 1e-9 { hw / dx.abs() } else { f64::INFINITY };
            let ty = if dy.abs() > 1e-9 { hh / dy.abs() } else { f64::INFINITY };
            let s = tx.min(ty);
            v(c.x + dx * s, c.y + dy * s)
        }
    };
    local.rot(t.angle, c)
}

/// 端点应落的位置：轮廓交点再向外留 gap；官方新版带 fixedPoint 时按相对位置取点。
pub fn bound_point(t: &Element, b: &Binding, toward: V) -> V {
    if let Some([fx, fy]) = b.fixed_point {
        let c = center(t);
        let p = v(t.x + t.width * fx, t.y + t.height * fy).rot(t.angle, c);
        return p;
    }
    let p = focus_point(t, toward);
    let c = center(t);
    let d = p.sub(c);
    let l = d.len();
    if l < 1e-6 {
        return p;
    }
    p.add(d.scale(b.gap.max(0.) / l))
}

/// 找到 p 附近可绑定的元素（从上往下）。
pub fn find_bindable<'a>(doc: &'a SketchDoc, p: V, exclude: &[&str], tol: f64) -> Option<&'a Element> {
    doc.live().rev().filter(|e| e.is_bindable() && !e.locked && !exclude.contains(&e.id.as_str()) && !e.is_text()).find(|e| {
        let r = bounds(e).inflate(tol);
        r.contains(p) && (hit(e, p, tol) || inside_shape(e, p))
    })
}

/// 点是否在形状轮廓内（无论有无填充）
pub fn inside_shape(e: &Element, p: V) -> bool {
    let c = center(e);
    let q = p.rot(-e.angle, c);
    match e.kind.as_str() {
        "ellipse" => ((q.x - c.x) / (e.width / 2.).max(1.)).powi(2) + ((q.y - c.y) / (e.height / 2.).max(1.)).powi(2) <= 1.,
        "diamond" => inside_poly(q, &[v(c.x, e.y), v(e.x + e.width, c.y), v(c.x, e.y + e.height), v(e.x, c.y)]),
        _ => R::xywh(e.x, e.y, e.width, e.height).contains(q),
    }
}

/// 让所有绑定到 ids 中元素（或 ids 为空时所有）的线端点跟随目标。
pub fn update_bound_arrows(doc: &mut SketchDoc, moved: &[String]) {
    let jobs: Vec<usize> = doc
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_deleted && e.is_linear())
        .filter(|(_, e)| [true, false].iter().any(|s| e.binding(*s).is_some_and(|b| moved.is_empty() || moved.contains(&b.element_id))))
        .map(|(i, _)| i)
        .collect();
    for i in jobs {
        let e = doc.elements[i].clone();
        let mut pts = abs_points(&e);
        if pts.len() < 2 {
            continue;
        }
        // 旋转过的线：先转成世界坐标点、再去掉旋转
        let c = rot_center(&e);
        if e.angle != 0. {
            pts = pts.into_iter().map(|p| p.rot(e.angle, c)).collect();
        }
        let n = pts.len();
        let sb = e.binding(true);
        let eb = e.binding(false);
        let st = sb.as_ref().and_then(|b| doc.get(&b.element_id).cloned());
        let et = eb.as_ref().and_then(|b| doc.get(&b.element_id).cloned());
        // 两端都绑定时，先用目标中心作朝向，避免互相依赖
        if let (Some(b), Some(t)) = (&sb, &st) {
            let toward = if n == 2 { et.as_ref().map(center).unwrap_or(pts[1]) } else { pts[1] };
            pts[0] = bound_point(t, b, toward);
        }
        if let (Some(b), Some(t)) = (&eb, &et) {
            let toward = if n == 2 { st.as_ref().map(center).unwrap_or(pts[0]) } else { pts[n - 2] };
            pts[n - 1] = bound_point(t, b, toward);
        }
        if n == 2 {
            if let (Some(b), Some(t)) = (&sb, &st) {
                pts[0] = bound_point(t, b, pts[1]);
            }
        }
        let el = &mut doc.elements[i];
        el.angle = 0.;
        el.x = pts[0].x;
        el.y = pts[0].y;
        el.points = Some(pts.iter().map(|p| [p.x - pts[0].x, p.y - pts[0].y]).collect());
        el.normalize_points();
        el.bump();
    }
    // 箭头标签跟着箭头
    let label_owners: Vec<String> = doc.live().filter(|e| e.is_linear() && e.bound_text_id().is_some()).map(|e| e.id.clone()).collect();
    for id in label_owners {
        layout_bound_text(doc, &id);
    }
}

// ───────────── 文字 ─────────────

/// 估算一行文字宽度（与导出 / 换行共用，保证屏幕、SVG、PNG 一致）。
pub fn text_width(s: &str, size: f64) -> f64 {
    s.chars()
        .map(|c| {
            let w = if c.is_ascii() {
                match c {
                    'i' | 'l' | 'j' | '.' | ',' | '\'' | '!' | '|' | ':' | ';' => 0.32,
                    'm' | 'w' | 'M' | 'W' => 0.85,
                    ' ' => 0.33,
                    c if c.is_ascii_uppercase() => 0.66,
                    c if c.is_ascii_digit() => 0.56,
                    _ => 0.54,
                }
            } else if c as u32 >= 0x2e80 {
                1.0
            } else {
                0.62
            };
            w * size
        })
        .sum()
}

/// 按最大宽度换行（中文逐字可断，英文按单词断）。
pub fn wrap_text(text: &str, size: f64, max_w: f64) -> String {
    if max_w <= size {
        return text.to_string();
    }
    let mut out: Vec<String> = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        let mut word = String::new();
        let flush_word = |line: &mut String, word: &mut String, out: &mut Vec<String>| {
            if word.is_empty() {
                return;
            }
            if text_width(&format!("{line}{word}"), size) <= max_w || line.is_empty() {
                line.push_str(word);
            } else {
                out.push(std::mem::take(line).trim_end().to_string());
                *line = word.trim_start().to_string();
            }
            word.clear();
            // 单个超长单词：硬断
            while text_width(line, size) > max_w && line.chars().count() > 1 {
                let mut cut = String::new();
                for ch in line.chars() {
                    if text_width(&format!("{cut}{ch}"), size) > max_w && !cut.is_empty() {
                        break;
                    }
                    cut.push(ch);
                }
                let rest = line[cut.len()..].to_string();
                out.push(cut);
                *line = rest;
            }
        };
        for ch in para.chars() {
            if ch.is_ascii_alphanumeric() || (ch.is_ascii_punctuation() && !word.is_empty()) {
                word.push(ch);
            } else {
                flush_word(&mut line, &mut word, &mut out);
                word.push(ch);
                flush_word(&mut line, &mut word, &mut out);
            }
        }
        flush_word(&mut line, &mut word, &mut out);
        out.push(line);
    }
    out.join("\n")
}

pub fn text_size(text: &str, size: f64, line_h: f64) -> (f64, f64) {
    let lines: Vec<&str> = text.split('\n').collect();
    let w = lines.iter().map(|l| text_width(l, size)).fold(0., f64::max);
    (w.max(size * 0.3), lines.len().max(1) as f64 * size * line_h)
}

/// 独立文字：按内容更新宽高（autoResize）。
pub fn fit_text(e: &mut Element) {
    let text = e.text.clone().unwrap_or_default();
    let fs = e.font_size_or();
    let lh = e.line_height();
    let auto = e.get("autoResize").and_then(|v| v.as_bool()).unwrap_or(true);
    if auto {
        let (w, h) = text_size(&text, fs, lh);
        e.width = w;
        e.height = h;
    } else {
        let wrapped = wrap_text(&e.original_text(), fs, e.width);
        let (_, h) = text_size(&wrapped, fs, lh);
        e.text = Some(wrapped);
        e.height = h;
    }
}

/// 容器内可放文字的最大宽度（官方 getBoundTextMaxWidth）
pub fn container_text_max_w(c: &Element) -> f64 {
    let w = c.width.abs();
    match c.kind.as_str() {
        "ellipse" => (w / 2. * std::f64::consts::SQRT_2 - BOUND_TEXT_PADDING * 2.).max(10.),
        "diamond" => (w / 2. - BOUND_TEXT_PADDING * 2.).max(10.),
        "arrow" | "line" => 220.,
        _ => (w - BOUND_TEXT_PADDING * 2.).max(10.),
    }
}

fn container_text_max_h(c: &Element) -> f64 {
    let h = c.height.abs();
    match c.kind.as_str() {
        "ellipse" => h / 2. * std::f64::consts::SQRT_2 - BOUND_TEXT_PADDING * 2.,
        "diamond" => h / 2. - BOUND_TEXT_PADDING * 2.,
        _ => h - BOUND_TEXT_PADDING * 2.,
    }
}

/// 按容器（或箭头）重新排版它的绑定文字：换行、必要时撑高容器、居中摆放。
pub fn layout_bound_text(doc: &mut SketchDoc, container: &str) {
    let Some(c) = doc.get(container).cloned() else { return };
    let Some(tid) = c.bound_text_id() else { return };
    let Some(ti) = doc.index(&tid) else { return };
    let t = doc.elements[ti].clone();
    if t.is_deleted {
        return;
    }
    let fs = t.font_size_or();
    let lh = t.line_height();
    let max_w = container_text_max_w(&c);
    let wrapped = wrap_text(&t.original_text(), fs, max_w);
    let (tw, th) = text_size(&wrapped, fs, lh);
    let (x, y) = if c.is_linear() {
        let pts = world_points(&c);
        let mid = polyline_mid(&curve_points(&pts, c.roundness().is_some() && !c.elbowed()));
        (mid.x - tw / 2., mid.y - th / 2.)
    } else {
        // 容器放不下：撑高（官方行为）
        let need = th + BOUND_TEXT_PADDING * 2.;
        let max_h = container_text_max_h(&c);
        if th > max_h {
            if let Some(ci) = doc.index(container) {
                let ce = &mut doc.elements[ci];
                let ratio = match ce.kind.as_str() {
                    "ellipse" => std::f64::consts::SQRT_2,
                    "diamond" => 2.,
                    _ => 1.,
                };
                let new_h = (need * ratio).max(ce.height);
                ce.height = new_h;
                ce.bump();
            }
        }
        let c = doc.get(container).cloned().unwrap();
        let cc = center(&c);
        let x = match t.text_align() {
            "left" => cc.x - max_w / 2.,
            "right" => cc.x + max_w / 2. - tw,
            _ => cc.x - tw / 2.,
        };
        let y = match t.vertical_align() {
            "top" => c.y + BOUND_TEXT_PADDING + (c.height - container_text_max_h(&c) - BOUND_TEXT_PADDING * 2.) / 2.,
            "bottom" => c.y + c.height - BOUND_TEXT_PADDING - th - (c.height - container_text_max_h(&c) - BOUND_TEXT_PADDING * 2.) / 2.,
            _ => cc.y - th / 2.,
        };
        (x, y)
    };
    let angle = if c.is_linear() { 0. } else { c.angle };
    let te = &mut doc.elements[ti];
    if te.text.as_deref() != Some(&wrapped) || te.x != x || te.y != y || te.width != tw || te.height != th || te.angle != angle {
        te.text = Some(wrapped);
        te.x = x;
        te.y = y;
        te.width = tw;
        te.height = th;
        te.angle = angle;
        te.bump();
    }
}

pub fn polyline_mid(pts: &[V]) -> V {
    if pts.is_empty() {
        return V::default();
    }
    let total: f64 = pts.windows(2).map(|w| w[0].dist(w[1])).sum();
    let mut acc = 0.;
    for w in pts.windows(2) {
        let l = w[0].dist(w[1]);
        if acc + l >= total / 2. && l > 0. {
            let t = (total / 2. - acc) / l;
            return w[0].add(w[1].sub(w[0]).scale(t));
        }
        acc += l;
    }
    pts[0]
}

/// 自适应圆角半径（官方 getCornerRadius）
pub fn corner_radius(e: &Element) -> f64 {
    let x = e.width.abs().min(e.height.abs());
    match e.roundness() {
        None => 0.,
        Some(2) => x * 0.25,
        Some(_) => {
            let fixed = e.roundness_value().unwrap_or(32.);
            if x <= fixed / 0.25 {
                x * 0.25
            } else {
                fixed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Element {
        let mut e = Element::new("rectangle", x, y);
        e.width = w;
        e.height = h;
        e
    }

    #[test]
    fn focus_point_on_rect_edge() {
        let r = rect(0., 0., 100., 50.);
        let p = focus_point(&r, v(300., 25.));
        assert!((p.x - 100.).abs() < 1e-6 && (p.y - 25.).abs() < 1e-6);
    }

    #[test]
    fn arrow_follows_moved_target() {
        let mut d = SketchDoc::new();
        let a = rect(0., 0., 100., 100.);
        let b = rect(300., 0., 100., 100.);
        let mut arr = Element::new("arrow", 104., 50.);
        arr.points = Some(vec![[0., 0.], [192., 0.]]);
        arr.set_binding(true, Some(&a.id));
        arr.set_binding(false, Some(&b.id));
        let bid = b.id.clone();
        d.elements = vec![a, b, arr];
        d.elements[1].y = 200.;
        update_bound_arrows(&mut d, &[bid]);
        let pts = world_points(&d.elements[2]);
        let end = *pts.last().unwrap();
        // 终点贴着 b 的轮廓（留 4px 间距）
        let b = &d.elements[1];
        assert!(bounds(b).inflate(5.).contains(end), "{end:?}");
        assert!(!bounds(b).inflate(-1.).contains(end));
    }

    #[test]
    fn hit_rotated_rect_outline() {
        let mut r = rect(0., 0., 100., 20.);
        r.angle = std::f64::consts::FRAC_PI_2;
        // 旋转 90° 后竖着：中心 (50,10)，上边缘在 y = -40
        assert!(hit(&r, v(50., -40.), 2.));
        assert!(!hit(&r, v(0., 10.), 2.));
    }

    #[test]
    fn wrap_cjk_and_words() {
        let s = wrap_text("hello world foo", 20., 70.);
        assert!(s.lines().count() >= 2);
        let s = wrap_text("一二三四五六七八", 20., 60.);
        assert!(s.lines().all(|l| text_width(l, 20.) <= 60.));
    }
}
