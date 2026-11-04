//! 模板库补充的模板：时间线、图示、矩阵、画布、架构图、UML 等。
//! 全部由普通图形 / 路径 / 便签 / 连线拼成，插入后可以逐个编辑。

use super::super::model::*;
use super::B;
use crate::canvas::{pt, rect, Arrow, Col, PathCmd, Pt, Rect, ShapeKind as K};
use std::f32::consts::TAU;

const GREEN: (u32, u32) = (0x34c724, 0xd9f5d6);
const PURPLE: (u32, u32) = (0x7f3bf5, 0xece2fe);
const YELLOW: (u32, u32) = (0xffc60a, 0xfff5cc);
const RED: (u32, u32) = (0xf54a45, 0xfde2e2);
const BLUE: (u32, u32) = (0x3370ff, 0xe1eaff);
const ORANGE: (u32, u32) = (0xff8800, 0xfeead2);
const GREY: (u32, u32) = (0x8f959e, 0xeff0f1);
const PAL: [(u32, u32); 6] = [BLUE, GREEN, YELLOW, PURPLE, RED, ORANGE];

fn deg(d: f32) -> f32 {
    d.to_radians()
}

fn polar(c: Pt, r: f32, a: f32) -> Pt {
    pt(c.x + r * a.cos(), c.y + r * a.sin())
}

/// 圆弧上的点（角度为弧度，屏幕坐标 y 向下，角度增大为顺时针）。
fn arc_pts(c: Pt, r: f32, a0: f32, a1: f32) -> Vec<Pt> {
    let n = ((a1 - a0).abs() / TAU * 96.).ceil().max(2.) as usize;
    (0..=n).map(|i| polar(c, r, a0 + (a1 - a0) * i as f32 / n as f32)).collect()
}

/// 圆环的一段（r0 = 0 时为扇形）；head > 0 时末端带箭头，head 为箭头尖多出的角度。
fn band(c: Pt, r0: f32, r1: f32, a0: f32, a1: f32, head: f32) -> Vec<Pt> {
    let s = if a1 >= a0 { 1. } else { -1. };
    let mut v = arc_pts(c, r1, a0, a1);
    if head > 0. {
        let w = (r1 - r0) * 0.45;
        v.push(polar(c, r1 + w, a1));
        v.push(polar(c, (r0 + r1) / 2., a1 + s * head));
        v.push(polar(c, r0 - w, a1));
    }
    if r0 <= 0. {
        v.push(c);
    } else {
        v.extend(arc_pts(c, r0, a1, a0));
    }
    v
}

fn polyline(pts: &[Pt], close: bool) -> Vec<PathCmd> {
    let mut v: Vec<PathCmd> = pts.iter().enumerate().map(|(i, p)| if i == 0 { PathCmd::Move(*p) } else { PathCmd::Line(*p) }).collect();
    if close {
        v.push(PathCmd::Close);
    }
    v
}

impl B {
    fn last(&mut self) -> &mut El {
        self.d.els.last_mut().unwrap()
    }
    fn tint(&self, c: (u32, u32)) -> Col {
        if self.dark {
            Col::rgb(c.0).with_alpha(0.22)
        } else {
            Col::rgb(c.1)
        }
    }
    fn surface(&self) -> Col {
        Col::rgb(if self.dark { 0x25272b } else { 0xffffff })
    }
    fn line_col(&self) -> Col {
        Col::rgb(if self.dark { 0x5f6368 } else { 0xc9cdd4 })
    }
    fn block(&mut self, k: K, r: Rect, text: &str, c: (u32, u32)) -> u64 {
        self.colored(k, r, text, c.0, c.1)
    }
    /// 浅色填充、无边框
    fn flat(&mut self, k: K, r: Rect, text: &str, c: (u32, u32)) -> u64 {
        let id = self.block(k, r, text, c);
        self.last().no_stroke = true;
        id
    }
    /// 白底灰边框
    fn plain(&mut self, k: K, r: Rect, text: &str) -> u64 {
        let id = self.shape(k, r, text);
        let (s, f) = (self.line_col(), self.surface());
        let e = self.last();
        e.stroke = s;
        e.fill = Some(f);
        id
    }
    /// 实心色块、白字
    fn solid(&mut self, k: K, r: Rect, text: &str, c: (u32, u32)) -> u64 {
        let id = self.block(k, r, text, c);
        let e = self.last();
        e.fill = Some(Col::rgb(c.0));
        e.text_color = Some(Col::rgb(0xffffff));
        id
    }
    /// 在矩形内居中的文字
    fn label(&mut self, r: Rect, text: &str, size: f32) -> u64 {
        let mut el = self.d.new_el(ElKind::Text, r, self.dark);
        el.text = text.into();
        el.font_size = size;
        el.text_align = 2;
        el.v_align = 2;
        let id = el.id;
        self.d.els.push(el);
        id
    }
    /// 左上对齐的文字
    fn note(&mut self, at: Pt, w: f32, text: &str, size: f32) -> u64 {
        let n = text.lines().count().max(1) as f32;
        let id = self.label(rect(at.x, at.y, w, size * 1.4 * n + 12.), text, size);
        let e = self.last();
        e.text_align = 1;
        e.v_align = 1;
        id
    }
    /// 标题 + 说明（标题加粗）
    fn titled(&mut self, at: Pt, w: f32, title: &str, body: &str) {
        self.note(at, w, title, 15.);
        self.last().bold = true;
        self.note(pt(at.x, at.y + 28.), w, body, 13.);
    }
    fn line(&mut self, a: Pt, b: Pt, end: Arrow) -> u64 {
        let mut el = self.d.new_el(
            ElKind::Connector { a: End::Free { x: a.x, y: a.y }, b: End::Free { x: b.x, y: b.y }, route: Route::Straight, start: Arrow::None, end },
            Rect::default(),
            self.dark,
        );
        el.font_size = 14.;
        let id = el.id;
        self.d.els.push(el);
        id
    }
    fn dashed(&mut self, a: Pt, b: Pt) -> u64 {
        let id = self.line(a, b, Arrow::None);
        self.last().dash = true;
        id
    }
    fn path(&mut self, cmds: Vec<PathCmd>, stroke: Option<Col>, fill: Option<Col>) -> u64 {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for c in &cmds {
            let ps: &[Pt] = match c {
                PathCmd::Move(p) | PathCmd::Line(p) => std::slice::from_ref(p),
                PathCmd::Cubic(a, b, p) => &[*a, *b, *p],
                PathCmd::Close => &[],
            };
            for p in ps {
                (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
            }
        }
        let r = rect(x0, y0, (x1 - x0).max(1.), (y1 - y0).max(1.));
        let mut el = self.d.new_el(ElKind::Path { segs: PathSeg::normalize(&cmds, r) }, r, self.dark);
        el.fill = fill;
        match stroke {
            Some(c) => el.stroke = c,
            None => el.no_stroke = true,
        }
        el.stroke_w = 1.5;
        let id = el.id;
        self.d.els.push(el);
        id
    }
    /// 浅色填充的多边形
    fn poly(&mut self, pts: &[Pt], c: (u32, u32), stroke: bool) -> u64 {
        let f = self.tint(c);
        self.path(polyline(pts, true), stroke.then(|| Col::rgb(c.0)), Some(f))
    }
    fn conn_route(&mut self, a: u64, b: u64, route: Route, label: &str) -> u64 {
        let id = self.conn(a, b, label);
        if let ElKind::Connector { route: r, .. } = &mut self.last().kind {
            *r = route;
        }
        id
    }
    fn icon(&mut self, name: &str, r: Rect, c: u32) -> u64 {
        let mut el = self.d.new_el(ElKind::Icon { name: name.into() }, r, self.dark);
        el.stroke = Col::rgb(c);
        let id = el.id;
        self.d.els.push(el);
        id
    }
    /// 带彩色标题栏的卡片
    fn card(&mut self, r: Rect, c: (u32, u32), title: &str, body: &str) {
        self.plain(K::Rect, r, "");
        self.flat(K::Rect, rect(r.x + 1., r.y + 1., r.w - 2., 30.), title, c);
        self.last().bold = true;
        self.note(pt(r.x + 6., r.y + 36.), r.w - 12., body, 13.);
    }
    /// 四象限面板：每格带标题和两张便签
    fn quads(&mut self, cells: [(&str, (u32, u32)); 4], notes: [[&str; 2]; 4]) {
        let (w, h) = (320., 240.);
        for (i, ((title, c), ns)) in cells.iter().zip(notes).enumerate() {
            let (x, y) = ((i % 2) as f32 * (w + 12.), (i / 2) as f32 * (h + 12.));
            self.block(K::Rect, rect(x, y, w, h), title, *c);
            let e = self.last();
            e.text_align = 1;
            e.v_align = 1;
            e.bold = true;
            for (k, n) in ns.iter().enumerate() {
                self.sticky(rect(x + 20. + k as f32 * 140., y + 56., 124., 104.), [0, 0, 5, 7][i], n);
            }
        }
    }
    /// 左侧 + 底部的坐标轴
    fn axes(&mut self, w: f32, h: f32, xl: &str, yl: &str) {
        self.line(pt(-24., h + 24.), pt(-24., -20.), Arrow::Filled);
        self.line(pt(-24., h + 24.), pt(w + 20., h + 24.), Arrow::Filled);
        self.label(rect(-110., -40., 80., 30.), yl, 14.);
        self.label(rect(w + 24., h + 10., 80., 30.), xl, 14.);
    }
}

// ───────────── 时间线 ─────────────

pub(super) fn timeline_new(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = [
        ("2024 Q1", "立项", "完成需求调研与可行性评估"),
        ("2024 Q2", "研发", "核心功能开发与内测"),
        ("2024 Q3", "发布", "正式上线并开始推广"),
        ("2024 Q4", "增长", "拓展渠道，用户破百万"),
    ];
    b.line(pt(-40., 0.), pt(4. * 210. + 20., 0.), Arrow::None);
    b.last().dotted = true;
    for (i, (when, t, d)) in items.iter().enumerate() {
        let x = i as f32 * 210. + 80.;
        let c = PAL[i];
        b.block(K::Circle, rect(x - 8., -8., 16., 16.), "", c);
        let y = if i % 2 == 0 { -170. } else { 40. };
        b.card(rect(x - 95., y, 190., 130.), c, when, &format!("{t}\n{d}"));
    }
    b.d
}

pub(super) fn timeline(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = [("1 月", "需求调研"), ("3 月", "方案设计"), ("6 月", "开发测试"), ("9 月", "正式上线")];
    b.line(pt(-40., 0.), pt(4. * 200. + 40., 0.), Arrow::Filled);
    for (i, (when, t)) in items.iter().enumerate() {
        let x = i as f32 * 200. + 80.;
        let c = PAL[[0, 2, 4, 4][i]];
        let top = if i % 2 == 0 { -150. } else { -110. };
        b.block(K::Rect, rect(x - 70., top, 140., 56.), t, c);
        b.line(pt(x, top + 56.), pt(x, 0.), Arrow::None);
        b.last().stroke = Col::rgb(c.0);
        b.block(K::Circle, rect(x - 5., -5., 10., 10.), "", c);
        b.last().fill = Some(Col::rgb(c.0));
        b.flat(K::Rect, rect(x - 80., 20., 160., 28.), when, c);
    }
    b.d
}

pub(super) fn h_timeline(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = [("阶段一", "市场调研"), ("阶段二", "产品设计"), ("阶段三", "研发测试"), ("阶段四", "上线运营")];
    for i in (0..items.len()).rev() {
        let x = i as f32 * 180.;
        b.flat(K::Stadium, rect(x, 0., 220., 52.), "", [PURPLE, BLUE, GREEN, YELLOW][i]);
    }
    for (i, (t, d)) in items.iter().enumerate() {
        let x = i as f32 * 180.;
        let c = [PURPLE, BLUE, GREEN, YELLOW][i];
        b.plain(K::Circle, rect(x + 6., 6., 40., 40.), &format!("{}", i + 1));
        b.last().stroke = Col::rgb(c.0);
        let (y0, y1) = if i % 2 == 0 { (6., -70.) } else { (46., 122.) };
        b.line(pt(x + 26., y0), pt(x + 26., y1), Arrow::None);
        b.last().stroke = Col::rgb(c.0);
        b.titled(pt(x + 34., if i % 2 == 0 { -84. } else { 70. }), 140., t, d);
    }
    b.d
}

pub(super) fn v_timeline(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = [
        ("2021", "公司成立", "完成天使轮融资"),
        ("2022", "产品发布", "首款产品上线"),
        ("2023", "规模扩张", "团队突破 100 人"),
        ("2024", "全球化", "进入海外市场"),
    ];
    b.dashed(pt(0., -20.), pt(0., 4. * 130. + 10.));
    for (i, (when, t, d)) in items.iter().enumerate() {
        let y = i as f32 * 130. + 40.;
        let c = PAL[i];
        b.block(K::Circle, rect(-8., y - 8., 16., 16.), "", c);
        let x = if i % 2 == 0 { 30. } else { -250. };
        b.card(rect(x, y - 50., 220., 100.), c, when, &format!("{t}\n{d}"));
    }
    b.d
}

pub(super) fn milestone(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = [("Q1", "· 明确目标\n· 组建团队"), ("Q2", "· 完成原型\n· 用户访谈"), ("Q3", "· 开发上线\n· 灰度验证"), ("Q4", "· 全量推广\n· 复盘总结")];
    for (i, (q, d)) in items.iter().enumerate() {
        let x = i as f32 * 200.;
        let c = [PURPLE, YELLOW, RED, BLUE][i];
        b.flat(K::Chevron, rect(x, 0., 180., 40.), q, c);
        b.last().bold = true;
        b.block(K::Rect, rect(x, 60., 180., 160.), d, c);
        let e = b.last();
        e.dash = true;
        e.text_align = 1;
        e.v_align = 1;
    }
    b.d
}

pub(super) fn milestone_new(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = [("项目启动", "3 月 1 日"), ("设计评审", "4 月 15 日"), ("开发完成", "6 月 30 日"), ("正式发布", "8 月 8 日")];
    for (i, (t, d)) in items.iter().enumerate() {
        let x = i as f32 * 210.;
        let c = [GREEN, YELLOW, RED, BLUE][i];
        b.flat(K::Stadium, rect(x, 0., 196., 12.), "", c);
        b.block(K::Circle, rect(x - 6., 0., 12., 12.), "", c);
        b.line(pt(x, 12.), pt(x, 140.), Arrow::None);
        b.last().stroke = Col::rgb(c.0);
        b.titled(pt(x + 10., 40.), 170., t, d);
    }
    b.d
}

pub(super) fn roadmap(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = ["调研：明确用户与场景", "规划：确定版本范围", "开发：迭代交付功能", "验证：灰度与数据分析", "推广：全量发布与运营"];
    let mut prev = None;
    for (i, t) in items.iter().enumerate() {
        let y = i as f32 * 84.;
        let x = if i % 2 == 0 { 0. } else { 40. };
        let c = [PURPLE, BLUE, GREEN, YELLOW, ORANGE][i];
        let id = b.block(K::Stadium, rect(x, y, 300., 56.), t, c);
        let cx = if i % 2 == 0 { x + 300. - 52. } else { x + 8. };
        b.plain(K::Circle, rect(cx, y + 8., 40., 40.), &format!("{}", i + 1));
        b.last().stroke = Col::rgb(c.0);
        if let Some(p) = prev {
            let s = if i % 2 == 0 { 0. } else { 1. };
            b.conn_sides(p, (s, 0.5), id, (s, 0.5), "");
            if let ElKind::Connector { route, end, .. } = &mut b.last().kind {
                *route = Route::Curve;
                *end = Arrow::None;
            }
        }
        prev = Some(id);
    }
    b.d
}

// ───────────── 图示 ─────────────

fn ring(b: &mut B, names: &[&str], colors: &[(u32, u32)], center: &str) {
    let c = pt(0., 0.);
    let n = names.len() as f32;
    let step = 360. / n;
    for (i, t) in names.iter().enumerate() {
        let a0 = -90. + i as f32 * step + 6.;
        let a1 = a0 + step - 22.;
        b.poly(&band(c, 150., 200., deg(a0), deg(a1), deg(10.)), colors[i % colors.len()], false);
        let p = polar(c, 270., deg(a0 + step / 2. - 8.));
        b.label(rect(p.x - 70., p.y - 20., 140., 40.), t, 15.);
        b.last().bold = true;
    }
    b.label(rect(-100., -30., 200., 60.), center, 22.);
    b.last().bold = true;
}

pub(super) fn flywheel(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    ring(&mut b, &["更多用户", "更多内容", "更好体验"], &[GREEN, BLUE, YELLOW], "增长飞轮");
    b.d
}

pub(super) fn cycle(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    ring(&mut b, &["计划 Plan", "执行 Do", "检查 Check", "处理 Act"], &[GREEN, BLUE, YELLOW, PURPLE], "PDCA 循环");
    b.d
}

pub(super) fn radar(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let c = pt(0., 0.);
    let axes = ["沟通", "执行", "创新", "协作", "专业", "学习"];
    let vertex = |r: f32, i: usize| polar(c, r, deg(-90. + i as f32 * 60.));
    for (k, r) in [240., 180., 120., 60.].iter().enumerate() {
        let pts: Vec<Pt> = (0..6).map(|i| vertex(*r, i)).collect();
        let f = if k % 2 == 0 { b.tint(GREY) } else { b.surface() };
        let s = b.line_col();
        b.path(polyline(&pts, true), Some(s), Some(f));
    }
    for i in 0..6 {
        b.line(c, vertex(240., i), Arrow::None);
        b.last().stroke = b.line_col();
        let p = vertex(275., i);
        b.label(rect(p.x - 40., p.y - 16., 80., 32.), axes[i], 14.);
    }
    let vals = [0.8, 0.65, 0.5, 0.75, 0.4, 0.62];
    let pts: Vec<Pt> = vals.iter().enumerate().map(|(i, v)| vertex(240. * v, i)).collect();
    b.path(polyline(&pts, true), Some(Col::rgb(PURPLE.0)), Some(Col::rgb(PURPLE.0).with_alpha(0.2)));
    for p in pts {
        b.block(K::Circle, rect(p.x - 5., p.y - 5., 10., 10.), "", PURPLE);
    }
    b.d
}

/// 金字塔各层：顶点在 (0, 0)，底宽 = 1.2 × 高。
fn pyramid_layers(b: &mut B, h: f32, names: &[&str], colors: &[(u32, u32)]) -> Vec<(f32, f32)> {
    let n = names.len() as f32;
    let lh = h / n;
    let hw = |y: f32| 0.6 * y;
    let mut mids = vec![];
    for (i, t) in names.iter().enumerate() {
        let (y0, y1) = (i as f32 * lh + if i == 0 { 0. } else { 3. }, (i + 1) as f32 * lh);
        b.poly(&[pt(-hw(y0), y0), pt(hw(y0), y0), pt(hw(y1), y1), pt(-hw(y1), y1)], colors[i % colors.len()], true);
        let ty = if i == 0 { y1 - 36. } else { (y0 + y1) / 2. - 16. };
        b.label(rect(-90., ty, 180., 32.), t, 15.);
        mids.push((hw((y0 + y1) / 2.), (y0 + y1) / 2.));
    }
    mids
}

pub(super) fn pyramid(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    pyramid_layers(&mut b, 420., &["自我实现", "尊重需求", "社交需求", "安全需求", "生理需求"], &[YELLOW, GREEN, PURPLE, BLUE, ORANGE]);
    b.d
}

pub(super) fn pyramid_list(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let mids = pyramid_layers(&mut b, 300., &["", "", "", ""], &[YELLOW, GREEN, PURPLE, BLUE]);
    let items = ["愿景：成为行业第一", "战略：聚焦核心用户", "战术：打磨关键体验", "执行：每周迭代交付"];
    for (i, ((x, y), t)) in mids.iter().zip(items).enumerate() {
        b.dashed(pt(*x, *y), pt(250., *y));
        b.plain(K::Stadium, rect(250., y - 18., 230., 36.), t);
        b.last().stroke = Col::rgb([YELLOW, GREEN, PURPLE, BLUE][i].0);
    }
    b.d
}

pub(super) fn funnel(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = ["展现 10,000", "点击 4,000", "访问 2,400", "咨询 800", "成交 200"];
    let widths = [520., 400., 300., 210., 140., 140.];
    for (i, t) in items.iter().enumerate() {
        let (y0, y1) = (i as f32 * 74., i as f32 * 74. + 70.);
        let (a, z) = (widths[i] / 2., widths[i + 1] / 2.);
        b.poly(&[pt(-a, y0), pt(a, y0), pt(z, y1), pt(-z, y1)], [GREEN, PURPLE, BLUE, YELLOW, RED][i], true);
        b.label(rect(-80., y0 + 19., 160., 32.), t, 15.);
    }
    b.d
}

pub(super) fn brand_house(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.poly(&[pt(0., 80.), pt(280., 0.), pt(560., 80.)], GREY, true);
    b.label(rect(180., 38., 200., 36.), "品牌愿景", 16.);
    b.last().bold = true;
    b.block(K::Rect, rect(0., 80., 560., 50.), "品牌使命：让协作更简单", BLUE);
    b.block(K::Rect, rect(0., 130., 560., 50.), "核心价值：用户第一 · 坦诚清晰 · 追求极致", PURPLE);
    for (i, t) in ["产品力", "服务力", "渠道力", "传播力"].iter().enumerate() {
        b.block(K::Rect, rect(i as f32 * 140., 180., 140., 150.), t, YELLOW);
    }
    b.block(K::Rect, rect(0., 330., 560., 50.), "品牌基础：企业文化与组织能力", GREEN);
    b.d
}

pub(super) fn hierarchy(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let rows: [(&str, &[&str], (u32, u32)); 3] =
        [("决策层", &["董事会"], BLUE), ("管理层", &["产品总监", "技术总监"], GREEN), ("执行层", &["产品经理", "研发工程师", "测试工程师"], ORANGE)];
    for (i, (t, boxes, c)) in rows.iter().enumerate() {
        let y = i as f32 * 110.;
        b.block(K::Rect, rect(0., y, 680., 90.), "", *c);
        let e = b.last();
        e.dash = true;
        e.fill = None;
        b.note(pt(20., y + 30.), 100., t, 15.);
        b.last().bold = true;
        let n = boxes.len() as f32;
        let w = 150.;
        let x0 = 140. + (520. - n * w - (n - 1.) * 16.) / 2.;
        for (k, s) in boxes.iter().enumerate() {
            b.block(K::Rect, rect(x0 + k as f32 * (w + 16.), y + 22., w, 46.), s, *c);
        }
    }
    b.d
}

pub(super) fn group_list(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let groups = [
        ("前端", "· React / Vue\n· 组件库\n· 工程化", PURPLE),
        ("后端", "· Java / Go\n· 微服务\n· 消息队列", GREEN),
        ("数据", "· MySQL\n· Redis\n· 数据仓库", YELLOW),
    ];
    for (i, (t, d, c)) in groups.iter().enumerate() {
        let x = i as f32 * 250.;
        b.plain(K::Rect, rect(x, 0., 220., 240.), "");
        b.note(pt(x + 24., 40.), 180., d, 15.);
        b.block(K::Stadium, rect(x + 50., -18., 120., 36.), t, *c);
        b.last().bold = true;
    }
    b.d
}

fn pie_like(b: &mut B, r0: f32) {
    let items = [("产品 45%", 0.45), ("研发 25%", 0.25), ("市场 18%", 0.18), ("运营 12%", 0.12)];
    let cols = [GREEN, YELLOW, RED, PURPLE];
    let c = pt(0., 0.);
    let mut a = -90.;
    for (i, (t, v)) in items.iter().enumerate() {
        let a1 = a + 360. * v;
        b.poly(&band(c, r0, 180., deg(a), deg(a1), 0.), cols[i], true);
        a = a1;
        let y = -80. + i as f32 * 44.;
        b.block(K::Rect, rect(240., y, 18., 18.), "", cols[i]);
        b.note(pt(266., y - 7.), 140., t, 14.);
    }
}

pub(super) fn pie(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    pie_like(&mut b, 0.);
    b.d
}

pub(super) fn donut(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    pie_like(&mut b, 100.);
    b.label(rect(-80., -30., 160., 60.), "预算分配\n100%", 16.);
    b.d
}

pub(super) fn circle_matrix(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let quads = [
        (180., "01", RED, "目标", "明确要达成的结果"),
        (270., "02", GREEN, "现状", "梳理当前的问题"),
        (90., "03", YELLOW, "方案", "列出可选的解法"),
        (0., "04", BLUE, "行动", "拆解为具体任务"),
    ];
    for (a, n, c, t, d) in quads {
        let off = polar(pt(0., 0.), 5., deg(a + 45.));
        b.poly(&band(off, 0., 80., deg(a), deg(a + 90.), 0.), c, true);
        let p = polar(off, 44., deg(a + 45.));
        b.label(rect(p.x - 20., p.y - 14., 40., 28.), n, 14.);
        let (x, y) = (if off.x < 0. { -340. } else { 100. }, if off.y < 0. { -170. } else { 40. });
        b.card(rect(x, y, 240., 130.), c, t, d);
    }
    b.d
}

pub(super) fn honeycomb(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let (w, h) = (170., 170. * 0.866);
    b.block(K::Hexagon, rect(-w / 2., -h / 2., w, h), "核心能力", GREY);
    b.last().bold = true;
    let cells = [(0., -1., "产品"), (0.75, -0.5, "技术"), (0.75, 0.5, "运营"), (0., 1., "市场"), (-0.75, 0.5, "销售"), (-0.75, -0.5, "服务")];
    for (i, (dx, dy, t)) in cells.iter().enumerate() {
        let (cx, cy) = (dx * w * 1.04, dy * h * 1.04);
        b.block(K::Hexagon, rect(cx - w / 2., cy - h / 2., w, h), t, [BLUE, PURPLE, RED, YELLOW, GREEN, ORANGE][i]);
    }
    b.d
}

pub(super) fn curve_list(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let c = pt(-220., 0.);
    let arc = arc_pts(c, 260., deg(-58.), deg(58.));
    let s = b.line_col();
    b.path(polyline(&arc, false), Some(s), None);
    b.last().dash = true;
    let items = [("发现问题", "从数据和反馈中找到痛点"), ("分析原因", "定位根因，评估影响"), ("解决问题", "制定方案并跟踪落地")];
    for (i, (t, d)) in items.iter().enumerate() {
        let p = polar(c, 260., deg(-40. + i as f32 * 40.));
        b.block(K::Circle, rect(p.x - 24., p.y - 24., 48., 48.), &format!("{}", i + 1), [PURPLE, GREEN, YELLOW][i]);
        b.titled(pt(p.x + 40., p.y - 26.), 240., t, d);
    }
    b.d
}

pub(super) fn venn(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let r = 130.;
    let sets = [(0., -75., 0xe06c6c, "擅长的事", 0., -60.), (-87., 75., 0x5b7fd6, "喜欢的事", -50., 30.), (87., 75., 0xe0b85c, "有价值的事", 50., 30.)];
    for (x, y, c, t, dx, dy) in sets {
        b.shape(K::Circle, rect(x - r, y - r, 2. * r, 2. * r), "");
        let e = b.last();
        e.stroke = Col::rgb(c);
        e.fill = Some(Col::rgb(c).with_alpha(0.35));
        b.label(rect(x + dx - 60., y + dy - 16., 120., 32.), t, 15.);
    }
    b.label(rect(-40., 0., 80., 30.), "理想", 15.);
    b.last().bold = true;
    b.d
}

pub(super) fn stacked_venn(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let items = [(200., "潜在市场 TAM", RED), (155., "可服务市场 SAM", YELLOW), (110., "可获得市场 SOM", GREEN), (62., "当前份额", BLUE)];
    for (r, t, c) in items {
        b.block(K::Circle, rect(-r, 400. - 2. * r, 2. * r, 2. * r), "", c);
        let y = if r < 70. { 400. - r - 16. } else { 400. - 2. * r + 12. };
        b.label(rect(-80., y, 160., 32.), t, 14.);
    }
    b.d
}

pub(super) fn target(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    for (i, r) in [180., 135., 90., 45.].iter().enumerate() {
        b.shape(K::Circle, rect(-r, -r, 2. * r, 2. * r), "");
        let f = if i % 2 == 0 { b.tint(RED) } else { b.surface() };
        let e = b.last();
        e.no_stroke = true;
        e.fill = Some(f);
    }
    b.solid(K::Circle, rect(-14., -14., 28., 28.), "", RED);
    let items = [("长期目标", "三年成为行业领先", -110.), ("年度目标", "营收增长 50%", 0.), ("季度目标", "完成核心功能上线", 110.)];
    for (t, d, y) in items {
        b.dashed(pt(0., 0.), pt(240., y));
        b.titled(pt(250., y - 26.), 200., t, d);
    }
    b.d
}

fn trend(b: &mut B, up: bool) {
    let (a0, a1) = if up { (90., 0.) } else { (-90., 0.) };
    let c = pt(0., 0.);
    b.poly(&band(c, 238., 282., deg(a0), deg(a1), deg(12.)), BLUE, false);
    let stages = ["第一阶段", "第二阶段", "第三阶段", "第四阶段"];
    for (i, t) in stages.iter().enumerate() {
        let a = if up { 75. - i as f32 * 20. } else { -75. + i as f32 * 20. };
        let p = polar(c, 260., deg(a));
        b.dashed(pt(-220., p.y), pt(p.x - 24., p.y));
        b.block(K::Stadium, rect(p.x - 22., p.y - 10., 44., 20.), "", GREEN);
        b.note(pt(-220., p.y - 32.), 200., t, 14.);
    }
}

pub(super) fn trend_up(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    trend(&mut b, true);
    b.d
}

pub(super) fn trend_down(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    trend(&mut b, false);
    b.d
}

pub(super) fn axis2d(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.line(pt(0., 400.), pt(0., 0.), Arrow::Filled);
    b.line(pt(0., 400.), pt(540., 400.), Arrow::Filled);
    for (p0, p1, t) in [(pt(30., 380.), pt(500., 40.), "供给"), (pt(30., 40.), pt(500., 380.), "需求")] {
        b.dashed(p0, p1);
        b.last().stroke = Col::rgb(RED.0);
        b.note(pt(p1.x + 6., p1.y - 12.), 60., t, 14.);
    }
    let m = pt(265., 210.);
    b.dashed(pt(0., m.y), m);
    b.dashed(pt(m.x, 400.), m);
    b.solid(K::Circle, rect(m.x - 8., m.y - 8., 16., 16.), "", BLUE);
    b.note(pt(m.x + 14., m.y - 34.), 80., "均衡点", 14.);
    b.label(rect(-60., -36., 120., 30.), "价格", 14.);
    b.label(rect(540., 386., 80., 30.), "数量", 14.);
    b.d
}

pub(super) fn bubble_quad(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let (w, h) = (560., 400.);
    b.shape(K::Rect, rect(0., 0., w, h), "");
    let s = b.line_col();
    let e = b.last();
    (e.dash, e.fill, e.stroke) = (true, None, s);
    b.dashed(pt(w / 2., 0.), pt(w / 2., h));
    b.dashed(pt(0., h / 2.), pt(w, h / 2.));
    let bubbles = [
        (80., 90., 22., GREEN, "A"),
        (210., 70., 34., PURPLE, "B"),
        (250., 160., 16., YELLOW, "C"),
        (440., 70., 30., PURPLE, "D"),
        (420., 170., 18., GREEN, "E"),
        (170., 270., 56., BLUE, "F"),
        (410., 300., 48., PURPLE, "G"),
        (80., 340., 20., GREEN, "H"),
    ];
    for (x, y, r, c, t) in bubbles {
        b.block(K::Circle, rect(x - r, y - r, 2. * r, 2. * r), t, c);
        b.last().dash = true;
    }
    b.label(rect(w / 2. - 60., -36., 120., 30.), "市场增长率 高", 13.);
    b.label(rect(w / 2. - 60., h + 6., 120., 30.), "市场增长率 低", 13.);
    b.label(rect(-110., h / 2. - 15., 100., 30.), "份额 低", 13.);
    b.label(rect(w + 10., h / 2. - 15., 100., 30.), "份额 高", 13.);
    b.d
}

pub(super) fn grid_matrix(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let cells = [
        (-1., -1., RED, "高价值 · 高难度\n战略投入"),
        (1., -1., PURPLE, "高价值 · 低难度\n优先去做"),
        (-1., 1., GREEN, "低价值 · 高难度\n尽量避免"),
        (1., 1., YELLOW, "低价值 · 低难度\n有空再做"),
    ];
    for (sx, sy, c, t) in cells {
        let (x, y) = (if sx < 0. { -262. } else { 12. }, if sy < 0. { -202. } else { 12. });
        b.flat(K::Rect, rect(x, y, 250., 190.), t, c);
    }
    b.line(pt(-300., 0.), pt(300., 0.), Arrow::Filled);
    b.line(pt(0., 240.), pt(0., -240.), Arrow::Filled);
    b.label(rect(-50., -280., 100., 30.), "价值", 14.);
    b.label(rect(304., -15., 80., 30.), "易实现", 14.);
    b.d
}

// ───────────── 矩阵 / 分析 ─────────────

pub(super) fn priority_matrix(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.quads(
        [
            ("重大项目（高价值 · 高投入）", GREEN),
            ("快速见效（高价值 · 低投入）", BLUE),
            ("费力不讨好（低价值 · 高投入）", ORANGE),
            ("填空事项（低价值 · 低投入）", RED),
        ],
        [["新平台建设", "国际化"], ["优化注册流程", "首页提速"], ["旧系统重构", ""], ["改文案", "换图标"]],
    );
    b.axes(664., 492., "投入", "价值");
    b.d.els.retain(|e| !(matches!(e.kind, ElKind::Sticky) && e.text.is_empty()));
    b.d
}

pub(super) fn eisenhower(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.quads(
        [("重要且紧急 · 立即做", GREEN), ("重要不紧急 · 计划做", BLUE), ("紧急不重要 · 授权做", ORANGE), ("不重要不紧急 · 不做", RED)],
        [["线上故障处理", "客户投诉"], ["学习提升", "制定规划"], ["临时会议", "部分邮件"], ["刷短视频", "无效社交"]],
    );
    b.d
}

pub(super) fn talent_review(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.quads(
        [
            ("潜力员工（高潜力 · 低绩效）", BLUE),
            ("明星员工（高潜力 · 高绩效）", BLUE),
            ("待改进（低潜力 · 低绩效）", GREEN),
            ("骨干员工（低潜力 · 高绩效）", GREEN),
        ],
        [["张三", "李四"], ["王五", "赵六"], ["孙七", "周八"], ["吴九", "郑十"]],
    );
    b.axes(664., 492., "绩效", "潜力");
    b.d
}

pub(super) fn quick_retro(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.quads(
        [("👍 做得好的", GREEN), ("💪 待改进的", ORANGE), ("💡 新的想法", BLUE), ("🤔 疑问", PURPLE)],
        [["按时交付", "沟通顺畅"], ["需求变更多", "测试不足"], ["引入自动化测试", "每日站会"], ["排期如何更准？", ""]],
    );
    b.d.els.retain(|e| !(matches!(e.kind, ElKind::Sticky) && e.text.is_empty()));
    b.d
}

pub(super) fn fishbone(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.line(pt(0., 0.), pt(660., 0.), Arrow::None);
    b.last().stroke_w = 3.;
    b.poly(&[pt(660., -50.), pt(740., 0.), pt(660., 50.)], GREY, false);
    b.label(rect(750., -18., 140., 36.), "交付延期", 18.);
    b.last().bold = true;
    let bones = [
        (220., -1., "人员", RED, ["人手不足", "技能欠缺"]),
        (480., -1., "方法", PURPLE, ["流程繁琐", "缺少评审"]),
        (220., 1., "工具", GREEN, ["环境不稳定", "缺少自动化"]),
        (480., 1., "需求", BLUE, ["需求变更", "描述不清"]),
    ];
    for (x, s, t, c, causes) in bones {
        let top = pt(x - 120., 150. * s);
        b.solid(K::Stadium, rect(top.x - 50., top.y - 16. + 14. * s, 100., 32.), t, c);
        b.line(pt(top.x, top.y), pt(x, 0.), Arrow::None);
        for (k, n) in causes.iter().enumerate() {
            let f = 0.38 + 0.32 * k as f32;
            let p = pt(top.x + (x - top.x) * f, top.y * (1. - f));
            b.line(pt(p.x - 90., p.y), p, Arrow::None);
            b.last().dash = true;
            b.flat(K::Stadium, rect(p.x - 104., p.y - 24., 96., 22.), n, c);
            b.last().font_size = 12.;
        }
    }
    b.d
}

pub(super) fn empathy_map(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let (w, h) = (680., 460.);
    b.plain(K::Rect, rect(0., 0., w, h), "");
    let (cx, cy, r) = (w / 2., h / 2., 80.);
    for (x, y) in [(0., 0.), (w, 0.), (0., h), (w, h)] {
        let d = ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt();
        b.line(pt(x, y), pt(cx + (x - cx) * r / d, cy + (y - cy) * r / d), Arrow::None);
        b.last().stroke = b.line_col();
    }
    b.plain(K::Circle, rect(cx - r, cy - r, 2. * r, 2. * r), "用户");
    let regions = [
        (cx - 80., 16., "想法和感受", 0, "担心效率太低"),
        (20., cy - 16., "听到", 5, "同事推荐"),
        (w - 140., cy - 16., "看到", 7, "竞品功能更全"),
        (cx - 80., h - 150., "说和做", 2, "频繁切换工具"),
    ];
    for (x, y, t, c, n) in regions {
        b.note(pt(x, y), 160., t, 15.);
        b.last().bold = true;
        let (sx, sy) = if y < 50. {
            (x + 20., y + 40.)
        } else if y > cy {
            (x + 20., y + 34.)
        } else {
            (x, y + 34.)
        };
        b.sticky(rect(sx, sy, 120., 90.), c, n);
    }
    for (i, (t, c)) in [("痛点", RED), ("收获", GREEN)].iter().enumerate() {
        b.block(K::Rect, rect(i as f32 * (w / 2. + 8.), h + 16., w / 2. - 8., 120.), t, *c);
        let e = b.last();
        e.text_align = 1;
        e.v_align = 1;
        e.bold = true;
    }
    b.d
}

pub(super) fn service_blueprint(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.text(pt(0., -56.), "用户服务蓝图", 22.);
    let data = [
        ["", "预约", "到店", "服务", "离店"],
        ["用户行为", "线上选择时间", "签到取号", "体验服务", "支付评价"],
        ["前台接触", "预约页面", "前台接待", "服务人员", "收银台"],
        ["后台行为", "排班确认", "调取档案", "记录服务", "生成回访"],
        ["支持过程", "预约系统", "会员系统", "工单系统", "支付系统"],
    ];
    let mut t = b.d.new_table(5, 5, pt(0., 0.), dark);
    if let ElKind::Table { cols, rows, head_col, fills, .. } = &mut t.kind {
        *cols = vec![110., 160., 160., 160., 160.];
        *rows = vec![44., 70., 70., 70., 70.];
        *head_col = true;
        fills.resize(25, 0);
        for c in 1..5 {
            fills[c] = Col::rgb(if dark { 0x3b2f5c } else { 0xece2fe }).0;
            fills[5 + c] = Col::rgb(if dark { 0x4a4020 } else { 0xfff5cc }).0;
        }
    }
    t.x = 0.;
    t.y = 0.;
    t.sync_grid_size();
    for (r, row) in data.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            t.set_part_text(Part::Cell(r * 5 + c), v.to_string());
        }
    }
    b.d.els.push(t);
    b.d
}

/// 画布格子：左上角标题 + 一张便签
fn canvas_cell(b: &mut B, r: Rect, title: &str, note: &str) {
    b.plain(K::Rect, r, title);
    let e = b.last();
    e.text_align = 1;
    e.v_align = 1;
    e.bold = true;
    b.sticky(rect(r.x + 14., r.y + 44., (r.w - 28.).min(130.), 90.), 0, note);
}

pub(super) fn business_canvas(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let (w, h) = (190., 340.);
    let cells = [
        (0., 0., 1., 1., "关键合作", "供应商、渠道伙伴"),
        (1., 0., 1., 0.5, "关键业务", "研发、运营"),
        (1., 0.5, 1., 0.5, "核心资源", "技术、品牌"),
        (2., 0., 1., 1., "价值主张", "为用户解决什么问题"),
        (3., 0., 1., 0.5, "客户关系", "社群、专属服务"),
        (3., 0.5, 1., 0.5, "渠道通路", "官网、应用商店"),
        (4., 0., 1., 1., "客户细分", "中小企业团队"),
    ];
    for (cx, cy, cw, ch, t, n) in cells {
        canvas_cell(&mut b, rect(cx * w, cy * h, cw * w, ch * h), t, n);
    }
    canvas_cell(&mut b, rect(0., h, 2.5 * w, 180.), "成本结构", "人力、服务器");
    canvas_cell(&mut b, rect(2.5 * w, h, 2.5 * w, 180.), "收入来源", "订阅费、增值服务");
    b.d
}

pub(super) fn lean_canvas(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let (w, h) = (190., 340.);
    let cells = [
        (0., 0., 0.5, "问题", "最重要的三个问题"),
        (1., 0., 0.5, "解决方案", "对应的解决办法"),
        (1., 0.5, 0.5, "关键指标", "活跃、留存"),
        (2., 0., 1., "独特卖点", "一句话说清价值"),
        (3., 0., 0.5, "门槛优势", "难以被复制的优势"),
        (3., 0.5, 0.5, "渠道", "触达用户的路径"),
        (4., 0., 1., "客户群体", "目标用户与早期用户"),
    ];
    for (cx, cy, ch, t, n) in cells {
        canvas_cell(&mut b, rect(cx * w, cy * h, w, if t == "问题" { h } else { ch * h }), t, n);
    }
    canvas_cell(&mut b, rect(0., h, 2.5 * w, 180.), "成本分析", "获客、开发、运营成本");
    canvas_cell(&mut b, rect(2.5 * w, h, 2.5 * w, 180.), "收入分析", "定价、毛利、回本周期");
    b.d
}

// ───────────── 流程 / 架构 ─────────────

pub(super) fn product_flow(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let lane = b.lane(0, "产品研发流程", &["产品", "研发", "测试"]);
    let g = lane.grid().unwrap();
    let cx = |c: usize| (g.xs[c] + g.xs[c + 1]) / 2.;
    let y0 = g.ys[1];
    let start = b.colored(K::Stadium, rect(cx(0) - 65., y0 + 24., 130., 44.), "需求提出", 0x34c724, 0xd9f5d6);
    let review = b.colored(K::Diamond, rect(cx(0) - 75., y0 + 100., 150., 76.), "需求评审", 0xffc60a, 0xfff5cc);
    let design = b.shape(K::RoundRect, rect(cx(1) - 70., y0 + 200., 140., 50.), "技术方案");
    let dev = b.shape(K::RoundRect, rect(cx(1) - 70., y0 + 280., 140., 50.), "编码开发");
    let test = b.colored(K::Diamond, rect(cx(2) - 75., y0 + 350., 150., 76.), "测试验收", 0xffc60a, 0xfff5cc);
    let launch = b.colored(K::Stadium, rect(cx(0) - 65., y0 + 366., 130., 44.), "上线发布", 0xf54a45, 0xfde2e2);
    b.conn(start, review, "");
    b.conn(review, design, "通过");
    b.conn_sides(review, (0., 0.5), start, (0., 0.5), "修改");
    b.conn(design, dev, "");
    b.conn(dev, test, "");
    b.conn(test, launch, "通过");
    b.conn_sides(test, (0.5, 0.), dev, (1., 0.5), "缺陷");
    b.d
}

pub(super) fn data_flow(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let user = b.plain(K::Rect, rect(0., 102., 120., 56.), "顾客");
    let p1 = b.block(K::Circle, rect(220., 80., 100., 100.), "1.0\n下单", PURPLE);
    let p2 = b.block(K::Circle, rect(420., 80., 100., 100.), "2.0\n支付", PURPLE);
    let p3 = b.block(K::Circle, rect(620., 80., 100., 100.), "3.0\n发货", PURPLE);
    let wh = b.plain(K::Rect, rect(820., 102., 120., 56.), "仓库");
    let d1 = b.shape(K::DataStore, rect(200., 280., 140., 48.), "D1 订单");
    let d2 = b.shape(K::DataStore, rect(600., 280., 140., 48.), "D2 库存");
    for (a, z, t) in [(user, p1, "订单信息"), (p1, p2, "待支付订单"), (p2, p3, "已支付订单"), (p3, wh, "发货单"), (p1, d1, "写入"), (d2, p3, "库存数据")]
    {
        b.conn_route(a, z, Route::Straight, t);
    }
    b.conn_route(d1, p2, Route::Curve, "读取");
    b.d
}

pub(super) fn sys_arch(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let layers: [(&str, f32, f32, (u32, u32)); 4] =
        [("接入层", 0., 90., YELLOW), ("业务层", 106., 170., PURPLE), ("基础服务", 292., 90., GREEN), ("数据层", 398., 110., BLUE)];
    for (t, y, h, c) in layers {
        b.block(K::Rect, rect(0., y, 760., h), t, c);
        let e = b.last();
        e.text_align = 1;
        e.v_align = 1;
        e.bold = true;
    }
    let row = |b: &mut B, y: f32, items: &[&str], k: K| {
        let w = (640. - (items.len() as f32 - 1.) * 14.) / items.len() as f32;
        for (i, t) in items.iter().enumerate() {
            b.plain(k, rect(104. + i as f32 * (w + 14.), y, w, if k == K::Cylinder { 64. } else { 46. }), t);
        }
    };
    row(&mut b, 22., &["Web", "App", "小程序", "开放 API"], K::RoundRect);
    for (i, (g, items)) in [("用户中心", ["注册登录", "权限"]), ("订单中心", ["下单", "售后"]), ("支付中心", ["收银", "对账"])].iter().enumerate()
    {
        let x = 104. + i as f32 * 218.;
        b.block(K::Rect, rect(x, 120., 204., 140.), g, PURPLE);
        let f = b.surface();
        let e = b.last();
        e.v_align = 1;
        e.fill = Some(f);
        for (k, t) in items.iter().enumerate() {
            b.plain(K::RoundRect, rect(x + 14., 160. + k as f32 * 50., 176., 40.), t);
        }
    }
    row(&mut b, 314., &["消息队列", "缓存", "搜索", "文件存储"], K::RoundRect);
    row(&mut b, 420., &["MySQL", "Redis", "Elasticsearch", "OSS"], K::Cylinder);
    b.block(K::Rect, rect(776., 0., 120., 508.), "运维监控\n\n日志\n告警\n配置中心\n链路追踪", GREY);
    b.d
}

fn topo_node(b: &mut B, icon: &str, x: f32, y: f32, s: f32, name: &str, c: u32) -> u64 {
    let id = b.icon(icon, rect(x - s / 2., y - s / 2., s, s), c);
    b.label(rect(x - 70., y + s / 2. + 2., 140., 26.), name, 13.);
    id
}

pub(super) fn network(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let blue = BLUE.0;
    let net = topo_node(&mut b, "cloud", 0., 0., 56., "互联网", blue);
    let fw = topo_node(&mut b, "shield", 0., 120., 48., "防火墙", 0xf54a45);
    let rt = topo_node(&mut b, "router", 0., 240., 48., "核心路由器", blue);
    let sw: Vec<u64> = [(-260., "办公区交换机"), (0., "服务器区交换机"), (260., "无线 AP")]
        .iter()
        .map(|(x, t)| topo_node(&mut b, if *t == "无线 AP" { "wifi" } else { "network" }, *x, 380., 48., t, blue))
        .collect();
    b.conn_route(net, fw, Route::Straight, "");
    b.conn_route(fw, rt, Route::Straight, "");
    for s in &sw {
        b.conn_sides(rt, (0.5, 1.), *s, (0.5, 0.), "");
    }
    let leaves = [
        (0, -340., "monitor", "PC"),
        (0, -180., "printer", "打印机"),
        (1, -80., "server", "应用服务器"),
        (1, 80., "database", "数据库"),
        (2, 200., "laptop", "笔记本"),
        (2, 320., "smartphone", "手机"),
    ];
    for (p, x, ic, t) in leaves {
        let id = topo_node(&mut b, ic, x, 520., 44., t, 0x646a73);
        b.conn_sides(sw[p], (0.5, 1.), id, (0.5, 0.), "");
    }
    for e in b.d.els.iter_mut() {
        if let ElKind::Connector { end, .. } = &mut e.kind {
            *end = Arrow::None;
        }
    }
    b.d
}

pub(super) fn network_3d(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let net = topo_node(&mut b, "fluent:globe-with-meridians", 0., 0., 72., "互联网", 0);
    let fw = topo_node(&mut b, "fluent:shield", 0., 140., 64., "防火墙", 0);
    let ap = topo_node(&mut b, "fluent:satellite-antenna", 0., 280., 64., "网关", 0);
    b.conn_route(net, fw, Route::Straight, "");
    b.conn_route(fw, ap, Route::Straight, "");
    let leaves = [
        ("fluent:file-cabinet", "文件服务器"),
        ("fluent:desktop-computer", "台式机"),
        ("fluent:laptop", "笔记本"),
        ("fluent:mobile-phone", "手机"),
        ("fluent:printer", "打印机"),
    ];
    for (i, (ic, t)) in leaves.iter().enumerate() {
        let id = topo_node(&mut b, ic, (i as f32 - 2.) * 150., 440., 72., t, 0);
        b.conn_sides(ap, (0.5, 1.), id, (0.5, 0.), "");
    }
    for e in b.d.els.iter_mut() {
        if let ElKind::Connector { end, .. } = &mut e.kind {
            *end = Arrow::None;
        }
    }
    b.d
}

// ───────────── UML ─────────────

fn class_box(b: &mut B, k: K, r: Rect, name: &str, cells: &[&str]) -> u64 {
    let id = b.plain(k, r, name);
    let e = b.last();
    e.bold = true;
    for (i, c) in cells.iter().enumerate() {
        e.set_part_text(Part::Cell(i), c.to_string());
    }
    id
}

pub(super) fn class_diagram(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let user = class_box(&mut b, K::ClassBox, rect(0., 0., 220., 170.), "User", &["- id: Long\n- name: String", "+ login()\n+ logout()"]);
    let order = class_box(&mut b, K::ClassBox, rect(340., 0., 220., 170.), "Order", &["- id: Long\n- amount: Decimal", "+ pay()\n+ cancel()"]);
    let pay = class_box(&mut b, K::InterfaceBox, rect(340., 260., 220., 110.), "«interface» Payable", &["+ pay()"]);
    let item = class_box(&mut b, K::ClassBox, rect(660., 0., 200., 150.), "OrderItem", &["- sku: String\n- qty: Int", "+ subtotal()"]);
    b.conn_route(user, order, Route::Straight, "1 .. *");
    b.conn_route(order, item, Route::Straight, "");
    if let ElKind::Connector { start, end, .. } = &mut b.last().kind {
        (*start, *end) = (Arrow::DiamondFilled, Arrow::None);
    }
    b.conn_route(order, pay, Route::Straight, "");
    let e = b.last();
    e.dash = true;
    if let ElKind::Connector { end, .. } = &mut e.kind {
        *end = Arrow::Hollow;
    }
    b.shape(K::Note, rect(0., 260., 220., 90.), "用户可以创建多个订单");
    b.last().fill = Some(b.tint(YELLOW));
    b.d
}

pub(super) fn er_diagram(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let user = class_box(&mut b, K::Entity2Key, rect(0., 0., 240., 150.), "users 用户", &["PK", "id\nname\nemail\ncreated_at"]);
    let order = class_box(&mut b, K::Entity2Key, rect(380., 0., 240., 170.), "orders 订单", &["PK\nFK", "id\nuser_id\namount\nstatus\ncreated_at"]);
    let item = class_box(&mut b, K::Entity2Key, rect(380., 240., 240., 130.), "order_items 明细", &["PK\nFK", "id\norder_id\nsku\nqty"]);
    for (a, z) in [(user, order), (order, item)] {
        b.conn(a, z, "");
        if let ElKind::Connector { start, end, .. } = &mut b.last().kind {
            (*start, *end) = (Arrow::One, Arrow::ZeroOrMany);
        }
    }
    b.d
}

pub(super) fn state_machine(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let s = b.shape(K::StartDot, rect(0., 92., 24., 24.), "");
    let names = ["待支付", "已支付", "已发货", "已完成"];
    let ids: Vec<u64> = names.iter().enumerate().map(|(i, t)| b.plain(K::RoundRect, rect(90. + i as f32 * 180., 80., 120., 48.), t)).collect();
    let cancel = b.plain(K::RoundRect, rect(90., 240., 120., 48.), "已取消");
    let refund = b.plain(K::RoundRect, rect(450., 240., 120., 48.), "退款中");
    let end = b.shape(K::EndDot, rect(830., 92., 24., 24.), "");
    for e in b.d.els.iter_mut().filter(|e| matches!(e.kind, ElKind::Shape(K::StartDot | K::EndDot))) {
        e.fill = Some(e.stroke);
    }
    b.conn_route(s, ids[0], Route::Straight, "");
    for (i, t) in ["支付", "发货", "签收"].iter().enumerate() {
        b.conn_route(ids[i], ids[i + 1], Route::Straight, t);
    }
    b.conn_route(ids[3], end, Route::Straight, "");
    b.conn_route(ids[0], cancel, Route::Straight, "超时");
    b.conn_route(ids[1], refund, Route::Elbow, "申请退款");
    b.conn_sides(refund, (1., 0.5), ids[3], (0.5, 1.), "退款完成");
    b.conn_sides(cancel, (0.5, 1.), end, (0.5, 1.), "");
    b.d
}

pub(super) fn use_case(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.plain(K::Rect, rect(160., 0., 400., 440.), "电商系统");
    let e = b.last();
    e.v_align = 1;
    e.bold = true;
    e.fill = None;
    let buyer = b.shape(K::Actor, rect(0., 160., 60., 100.), "顾客");
    let admin = b.shape(K::Actor, rect(660., 180., 60., 100.), "管理员");
    let cases: Vec<u64> = [("浏览商品", 60.), ("下单", 150.), ("支付", 240.), ("管理商品", 330.)]
        .iter()
        .map(|(t, y)| b.block(K::Ellipse, rect(230., *y, 150., 56.), t, PURPLE))
        .collect();
    let login = b.block(K::Ellipse, rect(420., 150., 120., 56.), "登录", PURPLE);
    for c in &cases[..3] {
        b.conn_route(buyer, *c, Route::Straight, "");
    }
    b.conn_route(admin, cases[3], Route::Straight, "");
    b.conn_route(cases[1], login, Route::Straight, "«include»");
    b.last().dash = true;
    for e in b.d.els.iter_mut() {
        if let ElKind::Connector { end, .. } = &mut e.kind {
            if !e.dash {
                *end = Arrow::None;
            } else {
                *end = Arrow::Open;
            }
        }
    }
    b.d
}

// ───────────── 原型图 ─────────────

/// 灰色说明文字
fn muted(b: &mut B, r: Rect, text: &str, size: f32, align: u8) -> u64 {
    let id = b.label(r, text, size);
    let e = b.last();
    e.text_align = align;
    e.text_color = Some(Col::rgb(0x8f959e));
    id
}

/// 手机外框 + 状态栏 + 标题栏，返回外框 id
fn phone(b: &mut B, x: f32, title: &str) -> u64 {
    let frame = b.plain(K::RoundRect, rect(x, 0., 300., 620.), "");
    b.label(rect(x + 16., 8., 60., 24.), "9:41", 12.);
    b.last().bold = true;
    muted(b, rect(x + 200., 8., 84., 24.), "▂▄▆  100%", 11., 3);
    b.label(rect(x + 16., 38., 268., 32.), title, 16.);
    b.last().bold = true;
    b.line(pt(x, 78.), pt(x + 300., 78.), Arrow::None);
    let c = b.line_col();
    b.last().stroke = c;
    frame
}

pub(super) fn mobile_wireframe(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    // 首页
    phone(&mut b, 0., "首页");
    b.flat(K::Stadium, rect(16., 92., 268., 36.), "🔍 搜索商品", GREY);
    b.last().text_color = Some(Col::rgb(0x8f959e));
    b.flat(K::RoundRect, rect(16., 142., 268., 112.), "Banner 轮播图", GREY);
    for (k, t) in ["分类", "秒杀", "优惠券", "会员"].iter().enumerate() {
        let x = 24. + k as f32 * 66.;
        b.flat(K::Circle, rect(x + 7., 270., 44., 44.), "", PAL[k]);
        b.label(rect(x - 4., 318., 66., 22.), t, 12.);
    }
    let mut first = 0;
    for k in 0..3 {
        let y = 356. + k as f32 * 76.;
        let id = b.plain(K::Rect, rect(16., y, 268., 64.), "");
        b.last().fill = None;
        b.last().no_stroke = true;
        if k == 0 {
            first = id;
        }
        b.flat(K::RoundRect, rect(16., y, 64., 64.), "图片", GREY);
        b.note(pt(88., y), 190., &format!("商品名称 {}", k + 1), 14.);
        muted(&mut b, rect(88., y + 26., 190., 20.), "一句话描述商品卖点", 12., 1);
        b.note(pt(88., y + 40.), 100., "¥ 199", 13.);
        b.last().text_color = Some(Col::rgb(RED.0));
    }
    b.line(pt(0., 572.), pt(300., 572.), Arrow::None);
    let c = b.line_col();
    b.last().stroke = c;
    for (k, t) in ["首页", "分类", "购物车", "我的"].iter().enumerate() {
        b.label(rect(k as f32 * 75., 582., 75., 28.), t, 12.);
        if k == 0 {
            b.last().text_color = Some(Col::rgb(BLUE.0));
        }
    }
    // 详情页
    let x = 420.;
    let detail = phone(&mut b, x, "商品详情");
    muted(&mut b, rect(x + 12., 40., 60., 28.), "‹ 返回", 13., 1);
    b.flat(K::RoundRect, rect(x + 16., 92., 268., 220.), "商品大图", GREY);
    b.note(pt(x + 16., 326.), 268., "商品名称 1", 17.);
    b.last().bold = true;
    b.note(pt(x + 16., 360.), 120., "¥ 199", 18.);
    b.last().text_color = Some(Col::rgb(RED.0));
    muted(&mut b, rect(x + 16., 400., 268., 64.), "商品详情介绍文字，支持多行展示规格、参数和售后说明等信息。", 13., 1);
    b.last().v_align = 1;
    b.plain(K::RoundRect, rect(x + 16., 476., 268., 40.), "选择规格  ›");
    b.solid(K::Stadium, rect(x + 16., 566., 128., 40.), "加入购物车", ORANGE);
    b.solid(K::Stadium, rect(x + 156., 566., 128., 40.), "立即购买", RED);
    b.conn_sides(first, (1., 0.5), detail, (0., 0.5), "点击商品");
    b.d
}
