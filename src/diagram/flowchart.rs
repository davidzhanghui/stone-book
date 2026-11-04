//! 流程图：`flowchart TD` / `graph LR`。
//!
//! 同时提供通用的 `Graph` → `Scene` 构建（类图 / 状态图 / ER 图也用它）。

use super::sugiyama::{self, Dir, LEdge, LNode, Spacing};
use super::*;
use crate::canvas::geom::boundary_point;
use crate::canvas::{rect, Align, Arrow, Item, LineItem, Pt, Section, ShapeItem, ShapeKind};
use std::collections::HashMap;

// ───────────────────────────── 通用图结构 ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct GNode {
    pub id: String,
    pub text: String,
    pub shape: ShapeKind,
    pub style: Style,
    pub sections: Vec<Section>,
    pub cluster: Option<usize>,
    /// 固定尺寸（开始 / 结束点、分叉条等）
    pub fixed: Option<(f32, f32)>,
    pub classes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GEdge {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub start: Arrow,
    pub end: Arrow,
    pub dash: bool,
    pub dotted: bool,
    pub thick: bool,
    pub invisible: bool,
    pub minlen: usize,
    pub start_label: String,
    pub end_label: String,
    pub color: Option<Col>,
}

#[derive(Clone, Debug, Default)]
pub struct GCluster {
    pub title: String,
    pub parent: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Graph {
    pub dir: Dir,
    pub nodes: Vec<GNode>,
    pub edges: Vec<GEdge>,
    pub clusters: Vec<GCluster>,
    pub index: HashMap<String, usize>,
    pub curved: bool,
}

impl Graph {
    pub fn new(dir: Dir) -> Self {
        Graph { dir, nodes: vec![], edges: vec![], clusters: vec![], index: HashMap::new(), curved: true }
    }

    /// 取得（或创建）节点。
    pub fn node(&mut self, id: &str, cluster: Option<usize>) -> usize {
        if let Some(&i) = self.index.get(id) {
            return i;
        }
        let i = self.nodes.len();
        self.nodes.push(GNode { id: id.to_string(), text: id.to_string(), cluster, ..Default::default() });
        self.index.insert(id.to_string(), i);
        i
    }

    fn node_size(&self, n: &GNode, theme: &Theme, m: &mut dyn Measure) -> (f32, f32) {
        if let Some(s) = n.fixed {
            return s;
        }
        let fs = theme.font_size;
        if !n.sections.is_empty() {
            let mut w: f32 = 0.;
            let mut lines = 0;
            for s in &n.sections {
                for l in &s.lines {
                    w = w.max(m.width(l, fs, s.bold));
                    lines += 1;
                }
            }
            let h = lines as f32 * fs * 1.45 + n.sections.len() as f32 * 8. + 6.;
            return ((w + 28.).max(110.), h.max(40.));
        }
        let (tw, th) = text_size(m, &n.text, fs, n.style.bold);
        let (mut w, mut h) = (tw + 32., th + 20.);
        match n.shape {
            ShapeKind::Diamond => {
                w = tw + 56.;
                h = (th + 40.).max(w * 0.55);
            }
            ShapeKind::Circle | ShapeKind::DoubleCircle => {
                let d = (tw + 28.).max(th + 28.);
                w = d;
                h = d;
            }
            ShapeKind::Hexagon | ShapeKind::Parallelogram | ShapeKind::ParallelogramAlt | ShapeKind::Trapezoid | ShapeKind::TrapezoidAlt => w += 30.,
            ShapeKind::Cylinder => h += 18.,
            ShapeKind::Document => h += 12.,
            ShapeKind::Stadium | ShapeKind::Flag => w += 18.,
            ShapeKind::Ellipse => {
                w += 26.;
                h += 10.;
            }
            ShapeKind::Note => w += 6.,
            _ => {}
        }
        (w.max(56.), h.max(38.))
    }

    /// 连线端点落到节点边上的端口：纵向布局只从上下边进出、横向布局只从左右边进出（同层的边走左右 / 上下），
    /// 同一节点同一条边上的多条连线按另一端的位置从左到右错开（菱形的几个分支分别从左下、底部、右下出线）。
    fn assign_ports(&self, rects: &[crate::canvas::Rect], paths: &mut [Option<Vec<Pt>>]) {
        // (节点, 边) → [(边序号, 是否终点, 排序键)]；边：0 上、1 下、2 左、3 右
        let mut groups: BTreeMap<(usize, u8), Vec<(usize, bool, f32)>> = BTreeMap::new();
        let vertical = !self.dir.horizontal();
        for (k, e) in self.edges.iter().enumerate() {
            let Some(pts) = &paths[k] else { continue };
            if pts.len() < 2 || e.from == e.to {
                continue;
            }
            for (node, end, nb) in [(e.from, false, pts[1]), (e.to, true, pts[pts.len() - 2])] {
                let r = rects[node];
                let side = if vertical {
                    if nb.y >= r.bottom() {
                        1
                    } else if nb.y <= r.y {
                        0
                    } else if nb.x >= r.center().x {
                        3
                    } else {
                        2
                    }
                } else if nb.x >= r.right() {
                    3
                } else if nb.x <= r.x {
                    2
                } else if nb.y >= r.center().y {
                    1
                } else {
                    0
                };
                let key = if side < 2 { nb.x } else { nb.y };
                groups.entry((node, side)).or_default().push((k, end, key));
            }
        }
        for ((node, side), mut list) in groups {
            list.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
            let (r, m) = (rects[node], list.len());
            let step = if m > 1 { (0.6 / (m - 1) as f32).min(0.25) } else { 0. };
            for (i, (k, end, _)) in list.into_iter().enumerate() {
                let f = 0.5 + (i as f32 - (m as f32 - 1.) / 2.) * step;
                let toward = match side {
                    0 => crate::canvas::pt(r.x + f * r.w, r.y),
                    1 => crate::canvas::pt(r.x + f * r.w, r.bottom()),
                    2 => crate::canvas::pt(r.x, r.y + f * r.h),
                    _ => crate::canvas::pt(r.right(), r.y + f * r.h),
                };
                let p = boundary_point(self.nodes[node].shape, r, toward);
                if let Some(pts) = paths[k].as_mut() {
                    let i = if end { pts.len() - 1 } else { 0 };
                    pts[i] = p;
                }
            }
        }
    }

    pub fn to_scene(&self, theme: &Theme, m: &mut dyn Measure, offsets: &BTreeMap<String, (f32, f32)>) -> Scene {
        let sizes: Vec<(f32, f32)> = self.nodes.iter().map(|n| self.node_size(n, theme, m)).collect();
        let lnodes: Vec<LNode> = self.nodes.iter().zip(&sizes).map(|(n, &(w, h))| LNode { w, h, cluster: n.cluster }).collect();
        let ledges: Vec<LEdge> = self.edges.iter().map(|e| LEdge { from: e.from, to: e.to, minlen: e.minlen.max(1) }).collect();
        let labeled = self.edges.iter().any(|e| !e.label.is_empty() || !e.start_label.is_empty());
        let sp = Spacing { node: if self.clusters.is_empty() { 36. } else { 40. }, rank: if !self.clusters.is_empty() || labeled { 70. } else { 54. } };
        // 同一对节点之间的多条边（含往返）：记录序号，绘制时左右错开
        let mut pair_count: HashMap<(usize, usize), usize> = HashMap::new();
        for e in &self.edges {
            *pair_count.entry((e.from.min(e.to), e.from.max(e.to))).or_default() += 1;
        }
        let mut pair_seen: HashMap<(usize, usize), usize> = HashMap::new();
        let lay = sugiyama::layout(&lnodes, &ledges, self.dir, &sp);
        let rects: Vec<crate::canvas::Rect> = lay.pos.iter().zip(&sizes).map(|(c, &(w, h))| rect(c.x - w / 2., c.y - h / 2., w, h)).collect();
        let mut scene = Scene::default();

        // 子图边框：从最内层开始，包住成员节点与子子图
        let mut cbox: Vec<Option<crate::canvas::Rect>> = vec![None; self.clusters.len()];
        for ci in (0..self.clusters.len()).rev() {
            let mut b: Option<crate::canvas::Rect> = None;
            for (ni, n) in self.nodes.iter().enumerate() {
                if n.cluster == Some(ci) {
                    let r = rects[ni];
                    b = Some(b.map_or(r, |x| x.union(&r)));
                }
            }
            for (cj, c) in self.clusters.iter().enumerate() {
                if c.parent == Some(ci) {
                    if let Some(r) = cbox[cj] {
                        b = Some(b.map_or(r, |x| x.union(&r)));
                    }
                }
            }
            if let Some(r) = b {
                let title_h = if self.clusters[ci].title.is_empty() { 8. } else { theme.font_size * 1.9 };
                cbox[ci] = Some(crate::canvas::rect(r.x - 16., r.y - 12. - title_h, r.w + 32., r.h + 24. + title_h));
            }
        }
        for (ci, c) in self.clusters.iter().enumerate() {
            if let Some(r) = cbox[ci] {
                let mut s = ShapeItem::new(ShapeKind::RoundRect, r);
                s.radius = 8.;
                s.fill = Some(theme.cluster_fill.with_alpha(if theme.dark { 0.9 } else { 0.85 }));
                s.stroke = Some(theme.cluster_stroke);
                s.stroke_w = 1.2;
                s.text = c.title.clone();
                s.text_color = theme.text;
                s.font_size = theme.font_size - 1.;
                s.bold = true;
                s.align = Align::TopCenter;
                s.seed = 900 + ci as u64;
                scene.items.push(Item::Shape(s));
            }
        }

        // 连线：先算出每条边的折线，再统一分配两端在节点边上的端口
        let mut paths: Vec<Option<Vec<Pt>>> = vec![None; self.edges.len()];
        for (k, e) in self.edges.iter().enumerate() {
            if e.invisible {
                continue;
            }
            let mut pts: Vec<Pt> = lay.edges[k].clone();
            let key = (e.from.min(e.to), e.from.max(e.to));
            let total = pair_count[&key];
            let nth = {
                let c = pair_seen.entry(key).or_default();
                *c += 1;
                *c - 1
            };
            if total > 1 && pts.len() == 2 && e.from != e.to {
                // 两点直线 → 加一个偏移的中点，变成弧线
                let (a, b) = (pts[0], pts[1]);
                let d = b.sub(a);
                let len = (d.x * d.x + d.y * d.y).sqrt().max(1.);
                // 统一以「小序号节点 → 大序号节点」为基准方向，保证往返两条边分居两侧
                let sign = if e.from < e.to { 1. } else { -1. };
                let off = (nth as f32 - (total as f32 - 1.) / 2.) * 40. * sign;
                let n = crate::canvas::pt(-d.y / len, d.x / len);
                pts = vec![a, a.lerp(b, 0.5).add(n.scale(off)), b];
            }
            paths[k] = Some(pts);
        }
        self.assign_ports(&rects, &mut paths);
        let mut edge_ends = vec![];
        for (k, e) in self.edges.iter().enumerate() {
            let Some(pts) = paths[k].take() else { continue };
            let tangents =
                (self.curved && pts.len() >= 2 && e.from != e.to).then(|| (port_normal(&rects[e.from], pts[0]), port_normal(&rects[e.to], pts[pts.len() - 1])));
            let mut l = LineItem::new(pts);
            l.id = format!("edge:{k}");
            l.curved = self.curved;
            l.tangents = tangents;
            l.stroke = e.color.unwrap_or(theme.line);
            l.width = if e.thick { 3. } else { 1.5 };
            l.dash = e.dash;
            l.dotted = e.dotted;
            l.start = e.start;
            l.end = e.end;
            l.label = e.label.clone();
            l.start_label = e.start_label.clone();
            l.end_label = e.end_label.clone();
            l.label_color = theme.text;
            l.label_bg = Some(theme.label_bg);
            l.font_size = theme.font_size - 2.;
            l.seed = 300 + k as u64;
            scene.items.push(Item::Line(l));
            edge_ends.push((self.nodes[e.from].id.clone(), self.nodes[e.to].id.clone()));
        }

        // 节点
        for (i, n) in self.nodes.iter().enumerate() {
            let mut s = ShapeItem::new(n.shape, rects[i]);
            s.id = n.id.clone();
            s.fill = Some(n.style.fill.unwrap_or(theme.node_fill));
            s.stroke = Some(n.style.stroke.unwrap_or(theme.node_stroke));
            s.stroke_w = n.style.stroke_width.unwrap_or(1.5);
            s.dash = n.style.dash;
            s.text = n.text.clone();
            s.text_color = n.style.color.unwrap_or(theme.text);
            s.font_size = theme.font_size;
            s.bold = n.style.bold;
            s.sections = n.sections.clone();
            s.radius = 8.;
            s.seed = 10 + i as u64;
            match n.shape {
                ShapeKind::Note => {
                    s.fill = Some(n.style.fill.unwrap_or(theme.note_fill));
                    s.stroke = Some(n.style.stroke.unwrap_or(theme.note_stroke));
                }
                ShapeKind::StartDot | ShapeKind::EndDot | ShapeKind::Bar => {
                    s.stroke = Some(theme.text);
                    s.fill = Some(theme.text);
                }
                _ => {}
            }
            scene.items.push(Item::Shape(s));
        }
        apply_offsets(&mut scene, offsets, &edge_ends);
        scene.fit(16.);
        scene
    }
}

/// 端口所在边的朝外法向（曲线从节点边上垂直伸出）。
fn port_normal(r: &crate::canvas::Rect, p: Pt) -> Pt {
    let (dx, dy) = ((p.x - r.center().x) / r.w.max(1.), (p.y - r.center().y) / r.h.max(1.));
    if dy.abs() >= dx.abs() {
        crate::canvas::pt(0., dy.signum())
    } else {
        crate::canvas::pt(dx.signum(), 0.)
    }
}

// ───────────────────────────── 流程图解析 ─────────────────────────────

struct Parser<'a> {
    s: &'a [char],
    i: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self, k: usize) -> Option<char> {
        self.s.get(self.i + k).copied()
    }
    fn rest(&self) -> String {
        self.s[self.i.min(self.s.len())..].iter().collect()
    }
    fn starts(&self, pat: &str) -> bool {
        let p: Vec<char> = pat.chars().collect();
        self.s.len() >= self.i + p.len() && self.s[self.i..self.i + p.len()] == p[..]
    }
    fn skip_ws(&mut self) {
        while self.peek(0).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }
    fn eof(&self) -> bool {
        self.i >= self.s.len()
    }
}

struct NodeRef {
    id: String,
    shape: Option<(ShapeKind, String)>,
    class: Option<String>,
}

const OPENERS: &[(&str, &str, ShapeKind)] = &[
    ("(((", ")))", ShapeKind::DoubleCircle),
    ("((", "))", ShapeKind::Circle),
    ("([", "])", ShapeKind::Stadium),
    ("[[", "]]", ShapeKind::Subroutine),
    ("[(", ")]", ShapeKind::Cylinder),
    ("{{", "}}", ShapeKind::Hexagon),
    ("[/", "/]", ShapeKind::Parallelogram),
    ("[/", "\\]", ShapeKind::Trapezoid),
    ("[\\", "\\]", ShapeKind::ParallelogramAlt),
    ("[\\", "/]", ShapeKind::TrapezoidAlt),
    ("(", ")", ShapeKind::RoundRect),
    ("[", "]", ShapeKind::Rect),
    ("{", "}", ShapeKind::Diamond),
    (">", "]", ShapeKind::Flag),
];

fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '-' || c == '#' || (!c.is_ascii() && !c.is_whitespace())
}

fn parse_node(p: &mut Parser, line: usize) -> DResult<Option<NodeRef>> {
    p.skip_ws();
    let start = p.i;
    while let Some(c) = p.peek(0) {
        // 连字符：如果后面是边的写法（--、-.、->）就停止
        if c == '-' && matches!(p.peek(1), Some('-') | Some('.') | Some('>')) {
            break;
        }
        if c == '.' && p.peek(1) == Some('-') {
            break;
        }
        if c == '=' || c == '~' || c == '&' || c == ':' || c == ';' {
            break;
        }
        if !is_id_char(c) {
            break;
        }
        p.i += 1;
    }
    let id: String = p.s[start..p.i].iter().collect();
    if id.is_empty() {
        return Ok(None);
    }
    let mut shape = None;
    for (open, close, kind) in OPENERS {
        if p.starts(open) {
            // 找到匹配的结束符（文本可以用引号包住）
            let after = p.i + open.chars().count();
            let rest: String = p.s[after..].iter().collect();
            let mut text_end = None;
            if rest.trim_start().starts_with('"') {
                if let Some(q1) = rest.find('"') {
                    if let Some(q2) = rest[q1 + 1..].find('"') {
                        let tail = &rest[q1 + 1 + q2 + 1..];
                        if tail.trim_start().starts_with(close) {
                            let off = q1 + 1 + q2 + 1 + (tail.len() - tail.trim_start().len());
                            text_end = Some((rest[..q1 + 1 + q2 + 1].to_string(), off));
                        }
                    }
                }
            }
            if text_end.is_none() {
                if let Some(pos) = rest.find(close) {
                    text_end = Some((rest[..pos].to_string(), pos));
                }
            }
            if let Some((text, off)) = text_end {
                // 位置是字节偏移，转换成字符数
                let consumed_chars = rest[..off].chars().count() + close.chars().count();
                p.i = after + consumed_chars;
                shape = Some((*kind, clean_text(&text)));
                break;
            } else if !matches!(*open, "[/" | "[\\") {
                return Err(DiagramError::new(line, format!("节点 {id} 的「{open}」没有闭合")));
            }
        }
    }
    let mut class = None;
    if p.starts(":::") {
        p.i += 3;
        let s = p.i;
        while p.peek(0).is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            p.i += 1;
        }
        class = Some(p.s[s..p.i].iter().collect());
    }
    Ok(Some(NodeRef { id, shape, class }))
}

struct EdgeTok {
    start: Arrow,
    end: Arrow,
    dash: bool,
    thick: bool,
    invisible: bool,
    label: String,
    minlen: usize,
}

fn head(c: Option<char>) -> Option<Arrow> {
    match c {
        Some('>') => Some(Arrow::Filled),
        Some('x') => Some(Arrow::Cross),
        Some('o') => Some(Arrow::Circle),
        _ => None,
    }
}

fn parse_edge(p: &mut Parser) -> Option<EdgeTok> {
    p.skip_ws();
    let save = p.i;
    let mut start = Arrow::None;
    if matches!(p.peek(0), Some('<') | Some('x') | Some('o')) && matches!(p.peek(1), Some('-') | Some('=') | Some('.')) {
        start = head(Some(p.peek(0).unwrap())).unwrap_or(Arrow::Filled);
        if p.peek(0) == Some('<') {
            start = Arrow::Filled;
        }
        p.i += 1;
    }
    let mut tok = EdgeTok { start, end: Arrow::None, dash: false, thick: false, invisible: false, label: String::new(), minlen: 1 };
    match p.peek(0) {
        Some('~') if p.starts("~~~") => {
            while p.peek(0) == Some('~') {
                p.i += 1;
            }
            tok.invisible = true;
            return Some(tok);
        }
        Some('-') if p.peek(1) == Some('.') => {
            tok.dash = true;
            p.i += 1;
            let mut dots = 0;
            while p.peek(0) == Some('.') {
                p.i += 1;
                dots += 1;
            }
            // 「-. 文本 .->」
            if dots == 1 && p.peek(0) == Some(' ') {
                let rest = p.rest();
                for term in [".->", ".-"] {
                    if let Some(pos) = rest.find(term) {
                        tok.label = clean_text(&rest[..pos]);
                        p.i += rest[..pos].chars().count() + 1;
                        while p.peek(0) == Some('.') {
                            p.i += 1;
                        }
                        break;
                    }
                }
            }
            tok.minlen = dots.max(1);
            if p.peek(0) == Some('-') {
                p.i += 1;
            }
            if let Some(h) = head(p.peek(0)) {
                tok.end = h;
                p.i += 1;
            }
        }
        Some(c @ ('-' | '=')) => {
            let mut n: usize = 0;
            while p.peek(0) == Some(c) {
                p.i += 1;
                n += 1;
            }
            if n < 2 && head(p.peek(0)).is_none() {
                p.i = save;
                return None;
            }
            tok.thick = c == '=';
            // 「-- 文本 -->」「== 文本 ==>」
            if n == 2 && head(p.peek(0)).is_none() && p.peek(0) == Some(' ') {
                let rest = p.rest();
                let terms: &[&str] = if c == '-' { &["-->", "---", "--x", "--o"] } else { &["==>", "===", "==x", "==o"] };
                if let Some((pos, term)) = terms.iter().filter_map(|t| rest.find(t).map(|p| (p, *t))).min_by_key(|x| x.0) {
                    tok.label = clean_text(&rest[..pos]);
                    p.i += rest[..pos].chars().count();
                    let mut m = 0;
                    while p.peek(0) == Some(c) {
                        p.i += 1;
                        m += 1;
                    }
                    let _ = (term, m);
                }
            } else {
                tok.minlen = n.saturating_sub(1).max(1);
            }
            if let Some(h) = head(p.peek(0)) {
                // 'o'/'x' 作为箭头时后面不能紧跟标识符字符（否则是节点名）
                let is_head = p.peek(0) == Some('>') || !p.peek(1).is_some_and(|c| c.is_alphanumeric());
                if is_head {
                    tok.end = h;
                    p.i += 1;
                }
            }
        }
        _ => {
            p.i = save;
            return None;
        }
    }
    // 「-->|文本|」
    p.skip_ws();
    if p.peek(0) == Some('|') {
        let rest = p.rest();
        if let Some(pos) = rest[1..].find('|') {
            tok.label = clean_text(&rest[1..1 + pos]);
            p.i += rest[..pos + 2].chars().count();
        }
    }
    Some(tok)
}

pub fn render(header: &str, body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure, offsets: &BTreeMap<String, (f32, f32)>) -> DResult<Scene> {
    let g = parse(header, body)?;
    Ok(g.to_scene(theme, m, offsets))
}

pub fn parse(header: &str, body: &[(usize, String)]) -> DResult<Graph> {
    let dir = header.split_whitespace().nth(1).and_then(Dir::parse).unwrap_or(Dir::TB);
    let mut g = Graph::new(dir);
    let mut class_defs: HashMap<String, Style> = HashMap::new();
    let mut node_styles: Vec<(String, Style)> = vec![];
    let mut link_styles: Vec<(Option<usize>, Style)> = vec![];
    let mut stack: Vec<usize> = vec![];
    for (line, raw) in body {
        let line = *line;
        for stmt in raw.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            let lower = stmt.to_lowercase();
            let word = lower.split_whitespace().next().unwrap_or("");
            match word {
                "subgraph" => {
                    let rest = stmt[8..].trim();
                    let title = match rest.find('[') {
                        Some(b) => clean_text(rest[b + 1..].trim_end_matches(']')),
                        None => clean_text(rest),
                    };
                    g.clusters.push(GCluster { title, parent: stack.last().copied() });
                    stack.push(g.clusters.len() - 1);
                    continue;
                }
                "end" => {
                    if stack.pop().is_none() {
                        return Err(DiagramError::new(line, "多余的 end"));
                    }
                    continue;
                }
                "direction" => continue,
                "classdef" => {
                    let rest = stmt[8..].trim();
                    if let Some((names, style)) = rest.split_once(char::is_whitespace) {
                        for n in names.split(',') {
                            class_defs.insert(n.trim().to_string(), Style::parse(style));
                        }
                    }
                    continue;
                }
                "class" => {
                    let rest = stmt[5..].trim();
                    if let Some((ids, cls)) = rest.rsplit_once(char::is_whitespace) {
                        for id in ids.split(',') {
                            let i = g.node(id.trim(), stack.last().copied());
                            g.nodes[i].classes.push(cls.trim().to_string());
                        }
                    }
                    continue;
                }
                "style" => {
                    let rest = stmt[5..].trim();
                    if let Some((id, style)) = rest.split_once(char::is_whitespace) {
                        node_styles.push((id.trim().to_string(), Style::parse(style)));
                    }
                    continue;
                }
                "linkstyle" => {
                    let rest = stmt[9..].trim();
                    if let Some((ids, style)) = rest.split_once(char::is_whitespace) {
                        let st = Style::parse(style);
                        for id in ids.split(',') {
                            link_styles.push((id.trim().parse().ok(), st.clone()));
                        }
                    }
                    continue;
                }
                "click" | "accTitle" | "accDescr" => continue,
                _ => {}
            }
            // 链式语句：A & B --> C -->|x| D
            let chars: Vec<char> = stmt.chars().collect();
            let mut p = Parser { s: &chars, i: 0 };
            let mut prev: Vec<usize> = vec![];
            let mut pending: Option<EdgeTok> = None;
            loop {
                let mut group = vec![];
                loop {
                    let Some(nr) = parse_node(&mut p, line)? else { break };
                    let cl = stack.last().copied();
                    let idx = g.node(&nr.id, cl);
                    if let Some((shape, text)) = nr.shape {
                        g.nodes[idx].shape = shape;
                        g.nodes[idx].text = text;
                    }
                    if let Some(c) = nr.class {
                        g.nodes[idx].classes.push(c);
                    }
                    group.push(idx);
                    p.skip_ws();
                    if p.peek(0) == Some('&') {
                        p.i += 1;
                        continue;
                    }
                    break;
                }
                if group.is_empty() {
                    if pending.is_some() || !p.eof() {
                        return Err(DiagramError::new(line, format!("连线缺少目标节点：{}", p.rest())));
                    }
                    break;
                }
                if let Some(tok) = pending.take() {
                    for &a in &prev {
                        for &b in &group {
                            g.edges.push(GEdge {
                                from: a,
                                to: b,
                                label: tok.label.clone(),
                                start: tok.start,
                                end: tok.end,
                                dash: tok.dash,
                                thick: tok.thick,
                                invisible: tok.invisible,
                                minlen: tok.minlen,
                                ..Default::default()
                            });
                        }
                    }
                }
                p.skip_ws();
                if p.eof() {
                    break;
                }
                let Some(tok) = parse_edge(&mut p) else {
                    return Err(DiagramError::new(line, format!("无法解析连线：{}", p.rest())));
                };
                pending = Some(tok);
                prev = group;
            }
        }
    }
    if !stack.is_empty() {
        return Err(DiagramError::new(body.last().map(|b| b.0).unwrap_or(0), "subgraph 缺少 end"));
    }
    // 样式
    for n in g.nodes.iter_mut() {
        let mut st = class_defs.get("default").cloned().unwrap_or_default();
        for c in &n.classes {
            if let Some(cs) = class_defs.get(c) {
                st.merge(cs);
            }
        }
        n.style = st;
    }
    for (id, st) in node_styles {
        if let Some(&i) = g.index.get(&id) {
            g.nodes[i].style.merge(&st);
        }
    }
    for (ix, st) in link_styles {
        match ix {
            Some(i) => {
                if let Some(e) = g.edges.get_mut(i) {
                    e.color = st.stroke;
                    e.dash |= st.dash;
                }
            }
            None => g.edges.iter_mut().for_each(|e| e.color = st.stroke),
        }
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> Graph {
        let (lines, _) = preprocess(src);
        parse(&lines[0].1, &lines[1..]).unwrap_or_else(|e| panic!("line {}: {}", e.line, e.msg))
    }

    #[test]
    fn shapes_and_edges() {
        let g = p("flowchart LR\n A[方形] --> B(圆角)\n B -.-> C{判断}\n C ==>|粗| D((圆))\n C -- 文字 --> E[(库)]\n E --- F>旗]\n F --x G[[子程序]]\n G <--> H{{六边形}}\n H ~~~ I[/平行/]");
        assert_eq!(g.nodes.len(), 9);
        let s = |id: &str| g.nodes[g.index[id]].shape;
        assert_eq!(s("A"), ShapeKind::Rect);
        assert_eq!(s("B"), ShapeKind::RoundRect);
        assert_eq!(s("C"), ShapeKind::Diamond);
        assert_eq!(s("D"), ShapeKind::Circle);
        assert_eq!(s("E"), ShapeKind::Cylinder);
        assert_eq!(s("F"), ShapeKind::Flag);
        assert_eq!(s("G"), ShapeKind::Subroutine);
        assert_eq!(s("H"), ShapeKind::Hexagon);
        assert_eq!(s("I"), ShapeKind::Parallelogram);
        assert_eq!(g.edges.len(), 8);
        assert!(g.edges[1].dash);
        assert!(g.edges[2].thick && g.edges[2].label == "粗");
        assert_eq!(g.edges[3].label, "文字");
        assert_eq!(g.edges[4].end, Arrow::None);
        assert_eq!(g.edges[5].end, Arrow::Cross);
        assert_eq!(g.edges[6].start, Arrow::Filled);
        assert!(g.edges[7].invisible);
    }

    #[test]
    fn chains_groups_subgraphs_styles() {
        let g = p("graph TD\n a & b --> c --> d\n subgraph one [第一组]\n  x --> y\n  subgraph two\n   z\n  end\n end\n classDef hot fill:#f96,stroke:#333\n class a hot\n y:::hot\n style d fill:#ff0");
        assert_eq!(g.edges.len(), 4);
        assert_eq!(g.clusters.len(), 2);
        assert_eq!(g.clusters[1].parent, Some(0));
        assert_eq!(g.nodes[g.index["x"]].cluster, Some(0));
        assert_eq!(g.nodes[g.index["z"]].cluster, Some(1));
        assert!(g.nodes[g.index["a"]].style.fill.is_some());
        assert!(g.nodes[g.index["y"]].style.fill.is_some());
        assert!(g.nodes[g.index["d"]].style.fill.is_some());
    }

    #[test]
    fn quoted_text_with_brackets() {
        let g = p("flowchart TD\n A[\"包含 [方括号] 的文本\"] --> B[\"第一行<br>第二行\"]");
        assert_eq!(g.nodes[0].text, "包含 [方括号] 的文本");
        assert_eq!(g.nodes[1].text, "第一行\n第二行");
    }
}
