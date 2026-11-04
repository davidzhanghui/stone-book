//! 画板里的思维导图（画板「思维导图」）。
//!
//! 每个节点都是一个普通图形元素（`El.mind` 记录父节点与顺序），所以选中、样式、原位编辑、
//! 复制粘贴都复用画板已有的逻辑；这里负责：
//! - 自动布局：向右 / 左右分布 / 向下，节点尺寸随文字变化，每次提交后重新排布
//! - 分支线：曲线或直角，颜色跟随一级分支
//! - 收起 / 展开子节点
//! - 编辑命令：Enter 兄弟节点、Tab 子节点、⇧Tab 父节点、拖动节点换父节点

use super::model::*;
use super::view::*;
use crate::canvas::geom::estimate_width;
use crate::canvas::{pt, rect, Arrow, Col, Item, LineItem, Pt, Rect, Scene, ShapeItem, ShapeKind};
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::{HashMap, HashSet};

/// 一级分支的颜色（描边色；填充用同色浅透明）。
pub const BRANCH_COLORS: [u32; 6] = [0x3370ff, 0x14c0ff, 0x34c724, 0xff8800, 0x7f3bf5, 0xf14ba9];

/// 节点文字最宽多少就换行。
const MAX_TEXT_W: f32 = 260.;

/// 布局：(编号, 分组, 名称)。插入面板按分组排列（思维导图 / 树状图 / 时间线）。
pub const LAYOUTS: [(u8, &str, &str); 9] = [
    (0, "思维导图", "向右"),
    (3, "思维导图", "向左"),
    (2, "思维导图", "向下（组织架构）"),
    (1, "思维导图", "左右分布"),
    (4, "树状图", "向右下"),
    (5, "树状图", "向左下"),
    (6, "树状图", "左右交替"),
    (7, "时间线", "横向"),
    (8, "时间线", "纵向"),
];

/// 树状图：子节点相对主干缩进的距离 / 上下相邻子树的间距。
const IND: f32 = 22.;
const IND_GAP: f32 = 12.;
/// 横向时间线：一级分支离挂点的距离。
const TL_IND: f32 = 20.;

/// 一条分支线的走法。
#[derive(Clone, Copy, PartialEq, Debug)]
enum Edge {
    /// 左右展开的思维导图
    Horiz,
    /// 向下的组织架构图
    Down,
    /// 树状图：父节点底部伸出竖直主干，子节点挂在主干右侧（1）或左侧（-1）
    Indent(f32),
    /// 横向时间线：从主干竖着伸出再横向接到一级分支
    TimeH,
    /// 纵向时间线：从主干横向接到一级分支
    TimeV,
}

/// 父节点到子节点的分支线走法。`dir` 是子节点所在侧。
fn edge_kind(layout: u8, parent_root: bool, dir: i8) -> Edge {
    let d = if dir < 0 { -1. } else { 1. };
    match layout {
        2 => Edge::Down,
        4 => Edge::Indent(1.),
        5 => Edge::Indent(-1.),
        6 => Edge::Indent(d),
        7 if parent_root => Edge::TimeH,
        7 => Edge::Indent(1.),
        8 if parent_root => Edge::TimeV,
        8 => Edge::Indent(d),
        _ => Edge::Horiz,
    }
}

/// 树状图主干的 x：根节点从底部中间伸出，其他节点从靠近子节点一侧的边缘附近伸出。
fn indent_trunk(pr: Rect, parent_root: bool, d: f32) -> f32 {
    if parent_root {
        pr.center().x
    } else if d > 0. {
        pr.x + (pr.w / 2.).min(14.)
    } else {
        pr.right() - (pr.w / 2.).min(14.)
    }
}

/// 直角折线 a → b → c；`round` 时拐角画成圆角。
fn elbow(a: Pt, b: Pt, c: Pt, round: bool) -> Vec<Pt> {
    let r = 8f32.min((b.y - a.y).abs().max((b.x - a.x).abs())).min((c.x - b.x).abs().max((c.y - b.y).abs()));
    if !round || r < 1. {
        return vec![a, b, c];
    }
    let toward = |from: Pt, to: Pt| {
        let d = from.dist(to).max(0.001);
        pt(from.x + (to.x - from.x) / d * r, from.y + (to.y - from.y) / d * r)
    };
    let (p0, p1) = (toward(b, a), toward(b, c));
    let mut v = vec![a];
    for i in 0..=8 {
        let t = i as f32 / 8.;
        let u = 1. - t;
        v.push(pt(u * u * p0.x + 2. * u * t * b.x + t * t * p1.x, u * u * p0.y + 2. * u * t * b.y + t * t * p1.y));
    }
    v.push(c);
    v
}

/// 布局时用的只读结构：子节点表、收起的节点、节点尺寸。
struct Tree<'a> {
    kids: &'a HashMap<u64, Vec<(f32, u64)>>,
    collapsed: &'a HashSet<u64>,
    sizes: &'a HashMap<u64, (f32, f32)>,
}

impl Tree<'_> {
    fn size(&self, id: u64) -> (f32, f32) {
        self.sizes.get(&id).copied().unwrap_or((0., 0.))
    }

    /// 展开着的子节点。
    fn kids_of(&self, id: u64) -> Vec<u64> {
        if self.collapsed.contains(&id) {
            return vec![];
        }
        self.kids.get(&id).map(|k| k.iter().map(|x| x.1).collect()).unwrap_or_default()
    }

    fn subtree(&self, id: u64) -> Vec<u64> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() && out.len() < 100_000 {
            if let Some(k) = self.kids.get(&out[i]) {
                out.extend(k.iter().map(|x| x.1));
            }
            i += 1;
        }
        out
    }

    /// 左右展开时子树占的高度。
    fn span_h(&self, id: u64, depth: usize) -> f32 {
        let own = self.size(id).1;
        let k = self.kids_of(id);
        if k.is_empty() || depth > 200 {
            return own;
        }
        let gap = if depth == 0 { 28. } else { 14. };
        own.max(k.iter().map(|c| self.span_h(*c, depth + 1)).sum::<f32>() + gap * (k.len() - 1) as f32)
    }

    /// 缩进树占的高度：节点自身 + 下面依次排开的子树。
    fn height(&self, id: u64, depth: usize) -> f32 {
        let h = self.size(id).1;
        if depth > 200 {
            return h;
        }
        h + self.kids_of(id).iter().map(|k| IND_GAP + self.height(*k, depth + 1)).sum::<f32>()
    }

    /// 缩进树占的宽度。
    fn width(&self, id: u64, depth: usize) -> f32 {
        let w = self.size(id).0;
        if depth > 200 {
            return w;
        }
        self.kids_of(id).iter().map(|k| (w / 2.).min(14.) + IND + self.width(*k, depth + 1)).fold(w, f32::max)
    }

    /// 把节点放在 (x, y)（左上角），子节点在它下面往 d 一侧缩进排开。
    fn place(&self, id: u64, x: f32, y: f32, d: f32, pos: &mut HashMap<u64, (f32, f32)>, depth: usize) {
        pos.insert(id, (x, y));
        if depth > 200 {
            return;
        }
        let (w, h) = self.size(id);
        let r = rect(x, y, w, h);
        let tx = indent_trunk(r, false, d);
        let mut cy = y + h + IND_GAP;
        for k in self.kids_of(id) {
            let kw = self.size(k).0;
            let kx = if d > 0. { tx + IND } else { tx - IND - kw };
            self.place(k, kx, cy, d, pos, depth + 1);
            cy += self.height(k, depth + 1) + IND_GAP;
        }
    }
}

impl BoardDoc {
    /// 按顺序排好的子节点。
    pub fn mind_children(&self, id: u64) -> Vec<u64> {
        let mut v: Vec<(f32, u64)> = self.els.iter().filter(|e| e.mind_parent() == Some(id)).map(|e| (e.mind.as_ref().unwrap().seq, e.id)).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|x| x.1).collect()
    }

    fn mind_kids_map(&self) -> HashMap<u64, Vec<(f32, u64)>> {
        let mut kids: HashMap<u64, Vec<(f32, u64)>> = HashMap::new();
        for e in &self.els {
            if let (Some(p), Some(m)) = (e.mind_parent(), e.mind.as_ref()) {
                kids.entry(p).or_default().push((m.seq, e.id));
            }
        }
        for v in kids.values_mut() {
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        kids
    }

    pub fn mind_root(&self, mut id: u64) -> u64 {
        for _ in 0..256 {
            match self.get(id).and_then(|e| e.mind_parent()) {
                Some(p) if self.get(p).is_some() => id = p,
                _ => break,
            }
        }
        id
    }

    pub fn mind_level(&self, mut id: u64) -> usize {
        let mut n = 0;
        while let Some(p) = self.get(id).and_then(|e| e.mind_parent()).filter(|p| self.get(*p).is_some()) {
            id = p;
            n += 1;
            if n > 256 {
                break;
            }
        }
        n
    }

    /// 节点及其所有后代。
    pub fn mind_subtree(&self, id: u64) -> Vec<u64> {
        let kids = self.mind_kids_map();
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            if let Some(k) = kids.get(&out[i]) {
                out.extend(k.iter().map(|x| x.1));
            }
            i += 1;
        }
        out
    }

    /// 被收起的节点的所有后代。
    pub(super) fn mind_hidden(&self) -> HashSet<u64> {
        let mut hidden = HashSet::new();
        if !self.els.iter().any(|e| e.mind.as_ref().is_some_and(|m| m.collapsed)) {
            return hidden;
        }
        let kids = self.mind_kids_map();
        let mut stack: Vec<u64> = self
            .els
            .iter()
            .filter(|e| e.mind.as_ref().is_some_and(|m| m.collapsed))
            .flat_map(|e| kids.get(&e.id).map(|k| k.iter().map(|x| x.1).collect::<Vec<_>>()).unwrap_or_default())
            .collect();
        while let Some(id) = stack.pop() {
            if hidden.insert(id) {
                if let Some(k) = kids.get(&id) {
                    stack.extend(k.iter().map(|x| x.1));
                }
            }
        }
        hidden
    }

    /// 根节点的布局与分支线样式。
    fn mind_root_style(&self, id: u64) -> (u8, u8) {
        let r = self.mind_root(id);
        self.get(r).and_then(|e| e.mind.as_ref()).map(|m| (m.layout, m.branch)).unwrap_or((0, 0))
    }

    /// 分支线（画在节点下面）。
    pub(super) fn mind_edges(&self, lk: &Lookup, scene: &mut Scene) {
        // 树状图 / 时间线的主干：根节点伸出的一条长线（根节点颜色），一级分支挂在上面
        for root in self.els.iter().filter(|e| e.mind.as_ref().is_some_and(|m| m.parent == 0 && m.layout >= 4 && !m.collapsed) && lk.visible(e.id)) {
            let rr = root.rect();
            let kids: Vec<Rect> = self.mind_children(root.id).iter().filter_map(|k| lk.get(*k)).map(|e| e.rect()).collect();
            if kids.is_empty() {
                continue;
            }
            let layout = root.mind.as_ref().map(|m| m.layout).unwrap_or(7);
            let pts = if layout == 7 {
                let end = kids.iter().map(|r| r.x - TL_IND).fold(rr.right(), f32::max) + 28.;
                vec![pt(rr.right(), rr.center().y), pt(end, rr.center().y)]
            } else {
                // 纵向时间线的主干伸出最后一个分支一截，树状图停在最后一个分支
                let tail = if layout == 8 { 28. } else { 0. };
                let end = kids.iter().map(|r| r.center().y).fold(rr.bottom(), f32::max) + tail;
                vec![pt(rr.center().x, rr.bottom()), pt(rr.center().x, end)]
            };
            let mut l = LineItem::new(pts);
            l.stroke = root.stroke.with_alpha(0.9 * root.opacity);
            l.width = 2.4;
            l.end = Arrow::None;
            l.label_bg = None;
            l.seed = root.seed;
            scene.items.push(Item::Line(l));
        }
        for el in self.els.iter().filter(|e| lk.visible(e.id)) {
            let Some(pid) = el.mind_parent() else { continue };
            let Some(p) = lk.get(pid) else { continue };
            let (layout, branch) = self.mind_root_style(el.id);
            let (pr, cr) = (p.rect(), el.rect());
            let level1 = p.mind_parent().is_none();
            let dir = el.mind.as_ref().map(|m| m.dir).unwrap_or(1);
            let kind = edge_kind(layout, level1, dir);
            if kind != Edge::Horiz && kind != Edge::Down {
                // 树状图 / 时间线：竖直主干 + 横向短线（曲线风格时拐角画成圆角）
                let pts = match kind {
                    Edge::Indent(d) if level1 => {
                        // 根节点的竖直主干单独画过了，这里只画横向短线
                        let ex = if d > 0. { cr.x } else { cr.right() };
                        vec![pt(pr.center().x, cr.center().y), pt(ex, cr.center().y)]
                    }
                    Edge::Indent(d) => {
                        let tx = indent_trunk(pr, level1, d);
                        let ex = if d > 0. { cr.x } else { cr.right() };
                        elbow(pt(tx, pr.bottom()), pt(tx, cr.center().y), pt(ex, cr.center().y), branch == 0)
                    }
                    Edge::TimeH => {
                        let ax = cr.x - TL_IND;
                        elbow(pt(ax, pr.center().y), pt(ax, cr.center().y), pt(cr.x, cr.center().y), branch == 0)
                    }
                    _ => {
                        let ex = if dir > 0 { cr.x } else { cr.right() };
                        vec![pt(pr.center().x, cr.center().y), pt(ex, cr.center().y)]
                    }
                };
                let mut l = LineItem::new(pts);
                l.stroke = el.stroke.with_alpha(0.9 * el.opacity);
                l.width = if level1 { 2.2 } else { 1.6 };
                l.end = Arrow::None;
                l.label_bg = None;
                l.seed = el.seed;
                scene.items.push(Item::Line(l));
                continue;
            }
            let (a, b, vertical) = if kind == Edge::Down {
                (pt(pr.center().x, pr.bottom()), pt(cr.center().x, cr.y), true)
            } else if dir >= 0 {
                (pt(pr.right(), pr.center().y), pt(cr.x, cr.center().y), false)
            } else {
                (pt(pr.x, pr.center().y), pt(cr.right(), cr.center().y), false)
            };
            // 根节点的分支从中心附近伸出，显得更饱满
            let a = if level1 && !vertical { pt(a.x - (a.x - pr.center().x) * 0.15, a.y) } else { a };
            let pts = if branch == 1 {
                if vertical {
                    let my = (a.y + b.y) / 2.;
                    vec![a, pt(a.x, my), pt(b.x, my), b]
                } else {
                    let mx = (a.x + b.x) / 2.;
                    vec![a, pt(mx, a.y), pt(mx, b.y), b]
                }
            } else {
                let (c1, c2) =
                    if vertical { (pt(a.x, (a.y + b.y) / 2.), pt(b.x, (a.y + b.y) / 2.)) } else { (pt((a.x + b.x) / 2., a.y), pt((a.x + b.x) / 2., b.y)) };
                (0..=20)
                    .map(|i| {
                        let t = i as f32 / 20.;
                        let u = 1. - t;
                        pt(
                            u * u * u * a.x + 3. * u * u * t * c1.x + 3. * u * t * t * c2.x + t * t * t * b.x,
                            u * u * u * a.y + 3. * u * u * t * c1.y + 3. * u * t * t * c2.y + t * t * t * b.y,
                        )
                    })
                    .collect()
            };
            let mut l = LineItem::new(pts);
            l.stroke = el.stroke.with_alpha(0.9 * el.opacity);
            l.width = if level1 { 2.4 } else { 1.8 };
            l.end = Arrow::None;
            l.label_bg = None;
            l.seed = el.seed;
            scene.items.push(Item::Line(l));
        }
    }

    /// 节点尺寸：随文字自动变化，超过最大宽度换行。
    fn mind_size(e: &El) -> (f32, f32) {
        let fs = e.font_size;
        let lh = fs * 1.38;
        let plain = e.no_stroke && e.fill.is_none();
        let root = e.mind_parent().is_none();
        let (px, py) = if plain {
            (6., 4.)
        } else if root {
            (24., 14.)
        } else {
            (16., 9.)
        };
        let text = if e.text.is_empty() { "　　　" } else { e.text.as_str() };
        let mut widest = 0f32;
        let mut rows = 0f32;
        for line in text.split('\n') {
            let w = estimate_width(line, fs, e.bold);
            widest = widest.max(w.min(MAX_TEXT_W));
            rows += (w / MAX_TEXT_W).ceil().max(1.);
        }
        ((widest + 2. * px + 4.).max(fs * 2.5), rows * lh + 2. * py)
    }

    /// 重新排布所有思维导图（每次提交后调用）。根节点中心保持不动。
    pub fn layout_minds(&mut self) {
        // 父节点已被删除的孤儿节点升级为根
        let ids: HashSet<u64> = self.els.iter().map(|e| e.id).collect();
        for e in self.els.iter_mut() {
            if let Some(m) = e.mind.as_mut() {
                if m.parent != 0 && !ids.contains(&m.parent) {
                    m.parent = 0;
                }
            }
        }
        let roots: Vec<u64> = self.els.iter().filter(|e| e.mind.is_some() && e.mind_parent().is_none()).map(|e| e.id).collect();
        if roots.is_empty() {
            return;
        }
        let kids = self.mind_kids_map();
        let idx: HashMap<u64, usize> = self.els.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        // 先按文字更新尺寸
        for e in self.els.iter_mut().filter(|e| e.mind.is_some()) {
            let (w, h) = Self::mind_size(e);
            let c = e.rect().center();
            e.w = w;
            e.h = h;
            if e.mind_parent().is_none() {
                e.x = c.x - w / 2.;
                e.y = c.y - h / 2.;
            }
        }
        let collapsed: HashSet<u64> = self.els.iter().filter(|e| e.mind.as_ref().is_some_and(|m| m.collapsed)).map(|e| e.id).collect();
        let size = |els: &Vec<El>, id: u64| idx.get(&id).map(|&i| (els[i].w, els[i].h)).unwrap_or((0., 0.));
        let sizes: HashMap<u64, (f32, f32)> = self.els.iter().filter(|e| e.mind.is_some()).map(|e| (e.id, (e.w, e.h))).collect();
        let tree = Tree { kids: &kids, collapsed: &collapsed, sizes: &sizes };
        let mut dirs: HashMap<u64, i8> = HashMap::new();
        for root in roots {
            let (layout, rr) = match self.get(root) {
                Some(e) => (e.mind.as_ref().map(|m| m.layout).unwrap_or(0), e.rect()),
                None => continue,
            };
            let level1 = tree.kids_of(root);
            let sides = self.mind_sides(layout, &level1, &tree);
            for (k, s) in level1.iter().zip(&sides) {
                for n in tree.subtree(*k) {
                    dirs.insert(n, *s);
                }
            }
            if layout >= 4 {
                // 树状图 / 时间线：一级分支各自是一棵向下缩进的树
                let mut pos: HashMap<u64, (f32, f32)> = HashMap::new();
                pos.insert(root, (rr.x, rr.y));
                match layout {
                    7 => {
                        // 横向时间线：一级分支沿主干上下交替，上下两排各自往右排，挂点之间至少隔开一段
                        let ty = rr.center().y;
                        let mut cur = [rr.right() + 28.; 2];
                        let mut last = f32::MIN;
                        for (k, s) in level1.iter().zip(&sides) {
                            let i = (*s > 0) as usize;
                            let attach = cur[i].max(last + 56.);
                            let h = tree.height(*k, 0);
                            let ky = if *s < 0 { ty - 26. - h } else { ty + 26. };
                            tree.place(*k, attach + TL_IND, ky, 1., &mut pos, 0);
                            cur[i] = attach + TL_IND + tree.width(*k, 0) + 28.;
                            last = attach;
                        }
                    }
                    8 => {
                        // 纵向时间线：一级分支沿主干左右交替
                        let tx = rr.center().x;
                        let mut cur = [rr.bottom() + 28.; 2];
                        let mut last = f32::MIN;
                        for (k, s) in level1.iter().zip(&sides) {
                            let i = (*s > 0) as usize;
                            let y = cur[i].max(last + 44.);
                            let kw = tree.size(*k).0;
                            let kx = if *s > 0 { tx + 28. } else { tx - 28. - kw };
                            tree.place(*k, kx, y, *s as f32, &mut pos, 0);
                            cur[i] = y + tree.height(*k, 0) + 24.;
                            last = y;
                        }
                    }
                    _ => {
                        // 树状图：从根节点底部中间伸出主干，一级分支依次往下排
                        let tx = rr.center().x;
                        let mut y = rr.bottom() + 22.;
                        for (k, s) in level1.iter().zip(&sides) {
                            let kw = tree.size(*k).0;
                            let kx = if *s > 0 { tx + IND } else { tx - IND - kw };
                            tree.place(*k, kx, y, *s as f32, &mut pos, 0);
                            y += tree.height(*k, 0) + IND_GAP;
                        }
                    }
                }
                for (id, (x, y)) in pos {
                    if let Some(&i) = idx.get(&id) {
                        self.els[i].x = x;
                        self.els[i].y = y;
                    }
                }
                continue;
            }
            let vertical = layout == 2;
            // 子树在「排列方向」上占的长度（向右布局为高度，向下布局为宽度）
            let mut span: HashMap<u64, f32> = HashMap::new();
            fn measure(
                id: u64,
                kids: &HashMap<u64, Vec<(f32, u64)>>,
                collapsed: &HashSet<u64>,
                vertical: bool,
                size: &dyn Fn(u64) -> (f32, f32),
                span: &mut HashMap<u64, f32>,
                depth: usize,
            ) -> f32 {
                let (w, h) = size(id);
                let own = if vertical { w } else { h };
                let gap = if vertical {
                    24.
                } else if depth == 0 {
                    28.
                } else {
                    14.
                };
                let total = match kids.get(&id).filter(|_| !collapsed.contains(&id) && depth < 200) {
                    Some(k) if !k.is_empty() => {
                        let s: f32 = k.iter().map(|c| measure(c.1, kids, collapsed, vertical, size, span, depth + 1)).sum();
                        s + gap * (k.len() - 1) as f32
                    }
                    _ => 0.,
                };
                let v = own.max(total);
                span.insert(id, v);
                v
            }
            let els_ref = &self.els;
            let sz = |id: u64| size(els_ref, id);
            measure(root, &kids, &collapsed, vertical, &sz, &mut span, 0);
            // 再从根往下放置
            let mut pos: HashMap<u64, (f32, f32)> = HashMap::new();
            let rr = self.get(root).unwrap().rect();
            pos.insert(root, (rr.x, rr.y));
            // (节点, 方向：1 向右 / -1 向左 / 0 向下)
            let mut queue: Vec<(u64, f32)> = vec![];
            if !level1.is_empty() {
                let groups: Vec<(Vec<u64>, f32)> = if vertical {
                    vec![(level1.clone(), 0.)]
                } else {
                    let pick = |want: i8| level1.iter().zip(&sides).filter(|(_, s)| **s == want).map(|(k, _)| *k).collect::<Vec<u64>>();
                    vec![(pick(1), 1.), (pick(-1), -1.)]
                };
                for (g, dir) in groups {
                    self.mind_place_children(root, &g, dir, &span, &mut pos, &idx, 0);
                    queue.extend(g.into_iter().map(|c| (c, dir)));
                }
            }
            while let Some((id, dir)) = queue.pop() {
                if collapsed.contains(&id) {
                    continue;
                }
                if let Some(k) = kids.get(&id) {
                    let g: Vec<u64> = k.iter().map(|x| x.1).collect();
                    self.mind_place_children(id, &g, dir, &span, &mut pos, &idx, 1);
                    queue.extend(g.into_iter().map(|c| (c, dir)));
                }
            }
            for (id, (x, y)) in pos {
                if let Some(&i) = idx.get(&id) {
                    self.els[i].x = x;
                    self.els[i].y = y;
                }
            }
        }
        for e in self.els.iter_mut() {
            if let Some(m) = e.mind.as_mut() {
                m.dir = dirs.get(&e.id).copied().unwrap_or(1);
            }
        }
    }

    /// 一级分支各放在哪一侧（1 右 / 下、-1 左 / 上）：手动指定的优先，其余按布局自动分配。
    fn mind_sides(&self, layout: u8, level1: &[u64], tree: &Tree) -> Vec<i8> {
        let fixed: Vec<i8> = level1.iter().map(|k| self.get(*k).and_then(|e| e.mind.as_ref()).map(|m| m.side.signum()).unwrap_or(0)).collect();
        match layout {
            1 => {
                // 左右分布：按子树高度尽量平均，前一半放右边
                let span = |k: u64| tree.span_h(k, 0);
                let total: f32 = level1.iter().map(|k| span(*k)).sum();
                let mut acc: f32 = level1.iter().zip(&fixed).filter(|(_, s)| **s > 0).map(|(k, _)| span(*k)).sum();
                let mut any_right = fixed.iter().any(|s| *s > 0);
                level1
                    .iter()
                    .zip(&fixed)
                    .map(|(k, s)| match *s {
                        0 if acc < total / 2. || !any_right => {
                            acc += span(*k);
                            any_right = true;
                            1
                        }
                        0 => -1,
                        s => s,
                    })
                    .collect()
            }
            // 左右交替：树状图左右 / 纵向时间线从右边开始，横向时间线从上面开始
            6..=8 => {
                let first = if layout == 7 { -1 } else { 1 };
                fixed
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        if *s != 0 {
                            *s
                        } else if i % 2 == 0 {
                            first
                        } else {
                            -first
                        }
                    })
                    .collect()
            }
            2 | 4 => vec![1; level1.len()],
            5 => vec![-1; level1.len()],
            3 => fixed.iter().map(|s| if *s == 0 { -1 } else { *s }).collect(),
            _ => fixed.iter().map(|s| if *s == 0 { 1 } else { *s }).collect(),
        }
    }

    /// 把一组子节点排在 parent 的 dir 一侧（dir = 0 表示向下），整组相对父节点居中。
    #[allow(clippy::too_many_arguments)]
    fn mind_place_children(
        &self,
        parent: u64,
        kids: &[u64],
        dir: f32,
        span: &HashMap<u64, f32>,
        pos: &mut HashMap<u64, (f32, f32)>,
        idx: &HashMap<u64, usize>,
        depth: usize,
    ) {
        if kids.is_empty() {
            return;
        }
        let Some(&(px, py)) = pos.get(&parent) else { return };
        let pe = &self.els[idx[&parent]];
        let (pw, ph) = (pe.w, pe.h);
        let vertical = dir == 0.;
        let gap = if vertical {
            24.
        } else if depth == 0 {
            28.
        } else {
            14.
        };
        let total: f32 = kids.iter().map(|k| span[k]).sum::<f32>() + gap * (kids.len() - 1) as f32;
        let level_gap = if depth == 0 { 64. } else { 40. };
        if vertical {
            let mut x = px + pw / 2. - total / 2.;
            for k in kids {
                let e = &self.els[idx[k]];
                let s = span[k];
                pos.insert(*k, (x + s / 2. - e.w / 2., py + ph + level_gap));
                x += s + gap;
            }
        } else {
            let mut y = py + ph / 2. - total / 2.;
            for k in kids {
                let e = &self.els[idx[k]];
                let s = span[k];
                let nx = if dir > 0. { px + pw + level_gap } else { px - level_gap - e.w };
                pos.insert(*k, (nx, y + s / 2. - e.h / 2.));
                y += s + gap;
            }
        }
    }

    /// 新建一个思维导图节点，样式按层级（根 / 一级分支 / 其他）。
    pub fn new_mind_node(&mut self, parent: u64, seq: f32, text: &str, at: Pt, dark: bool) -> El {
        let mut el = self.new_el(ElKind::Shape(ShapeKind::RoundRect), rect(at.x, at.y, 80., 40.), dark);
        el.text = text.to_string();
        el.mind = Some(Mind { parent, seq, ..Default::default() });
        let level = if parent == 0 { 0 } else { self.mind_level(parent) + 1 };
        let branch = if level >= 1 {
            // 一级分支按顺序轮换颜色；更深的沿用所属一级分支的颜色
            if level == 1 {
                let n = self.mind_children(parent).len();
                Some(Col::rgb(BRANCH_COLORS[n % BRANCH_COLORS.len()]))
            } else {
                self.get(parent).map(|p| p.stroke)
            }
        } else {
            None
        };
        Self::style_mind_level(&mut el, level, branch);
        el
    }

    /// 层级默认样式：根节点蓝底白字、一级分支浅色底、更深的只有文字。
    pub fn style_mind_level(el: &mut El, level: usize, branch: Option<Col>) {
        let c = branch.unwrap_or(Col::rgb(0x3370ff));
        match level {
            0 => {
                el.kind = ElKind::Shape(ShapeKind::RoundRect);
                el.fill = Some(Col::rgb(0x3370ff));
                el.stroke = Col::rgb(0x3370ff);
                el.text_color = Some(Col::rgb(0xffffff));
                el.font_size = 20.;
                el.bold = true;
                el.no_stroke = false;
            }
            1 => {
                el.kind = ElKind::Shape(ShapeKind::RoundRect);
                el.fill = Some(c.with_alpha(0.16));
                el.stroke = c;
                el.text_color = None;
                el.font_size = 16.;
                el.bold = false;
                el.no_stroke = false;
            }
            _ => {
                el.kind = ElKind::Shape(ShapeKind::Rect);
                el.fill = None;
                el.stroke = c;
                el.no_stroke = true;
                el.text_color = None;
                el.font_size = 14.;
                el.bold = false;
            }
        }
        el.stroke_w = 1.6;
    }

    /// 结构变化后，按新层级重设一棵子树的默认样式。
    pub fn restyle_mind_subtree(&mut self, id: u64) {
        for n in self.mind_subtree(id) {
            let level = self.mind_level(n);
            let branch = if level >= 2 {
                // 找到所属的一级分支
                let mut cur = n;
                while self.mind_level(cur) > 1 {
                    cur = self.get(cur).and_then(|e| e.mind_parent()).unwrap_or(cur);
                }
                self.get(cur).map(|e| e.stroke)
            } else if level == 1 {
                let p = self.get(n).and_then(|e| e.mind_parent()).unwrap_or(0);
                let k = self.mind_children(p).iter().position(|x| *x == n).unwrap_or(0);
                Some(Col::rgb(BRANCH_COLORS[k % BRANCH_COLORS.len()]))
            } else {
                None
            };
            if let Some(i) = self.index(n) {
                Self::style_mind_level(&mut self.els[i], level, branch);
            }
        }
    }

    /// 插入一个新的思维导图（中心主题 + 三个分支），返回根节点 id。
    pub fn insert_mind_map(&mut self, center: Pt, layout: u8, dark: bool) -> u64 {
        let mut root = self.new_mind_node(0, 0., "中心主题", center, dark);
        if let Some(m) = root.mind.as_mut() {
            m.layout = layout;
        }
        let rid = root.id;
        self.els.push(root);
        for i in 0..3 {
            let el = self.new_mind_node(rid, i as f32, &format!("分支主题 {}", i + 1), center, dark);
            self.els.push(el);
        }
        // 先排一次，再把根节点挪到落点中心
        self.layout_minds();
        if let Some(i) = self.index(rid) {
            let e = &mut self.els[i];
            e.x = center.x - e.w / 2.;
            e.y = center.y - e.h / 2.;
        }
        self.layout_minds();
        rid
    }

    /// 由缩进大纲（每行前面的 `*` / `+` / `-` 或两个空格表示层级）生成思维导图。
    pub fn mind_from_outline(&mut self, lines: &[(usize, String)], center: Pt, layout: u8, dark: bool) -> Option<u64> {
        let mut stack: Vec<(usize, u64)> = vec![];
        let mut root = None;
        let mut seq = 0.;
        for (level, text) in lines {
            while stack.last().is_some_and(|(l, _)| *l >= *level) {
                stack.pop();
            }
            let parent = stack.last().map(|x| x.1).unwrap_or(0);
            if let (0, Some(r)) = (parent, root) {
                // 多个根：挂到第一个根下
                let el = self.new_mind_node(r, seq, text, center, dark);
                seq += 1.;
                stack.push((*level, el.id));
                self.els.push(el);
                continue;
            }
            let mut el = self.new_mind_node(parent, seq, text, center, dark);
            seq += 1.;
            if parent == 0 {
                if let Some(m) = el.mind.as_mut() {
                    m.layout = layout;
                }
                root = Some(el.id);
            }
            stack.push((*level, el.id));
            self.els.push(el);
        }
        let rid = root?;
        self.layout_minds();
        if let Some(i) = self.index(rid) {
            let e = &mut self.els[i];
            e.x = center.x - e.w / 2.;
            e.y = center.y - e.h / 2.;
        }
        self.layout_minds();
        Some(rid)
    }

    /// 只有一个思维导图的新画板（文档里斜杠「思维导图」、以思维导图查看文档）；没给大纲时是默认的中心主题 + 三个分支。
    pub fn with_mind(outline: Option<&[(usize, String)]>, layout: u8, dark: bool) -> BoardDoc {
        let mut doc = BoardDoc::new(false);
        let at = pt(0., 0.);
        if outline.and_then(|lines| doc.mind_from_outline(lines, at, layout, dark)).is_none() {
            doc.insert_mind_map(at, layout, dark);
        }
        doc
    }

    /// 画板里只有思维导图（和关联线）时导出为缩进大纲（Markdown 列表），否则 None。
    pub fn mind_outline_md(&self) -> Option<String> {
        if !self.els.iter().any(|e| e.is_mind()) || !self.els.iter().all(|e| e.is_mind() || e.is_connector()) {
            return None;
        }
        fn walk(doc: &BoardDoc, id: u64, depth: usize, out: &mut String) {
            let Some(e) = doc.get(id) else { return };
            out.push_str(&format!("{}- {}\n", "    ".repeat(depth), e.text.replace('\n', " ")));
            for c in doc.mind_children(id) {
                walk(doc, c, depth + 1, out);
            }
        }
        let mut out = String::new();
        for e in self.els.iter().filter(|e| e.mind.as_ref().is_some_and(|m| m.parent == 0)) {
            walk(self, e.id, 0, &mut out);
        }
        Some(out)
    }
}

impl BoardEditor {
    /// 选中的单个思维导图节点。
    pub(super) fn single_mind(&self) -> Option<&El> {
        self.single().filter(|e| e.is_mind())
    }

    fn mind_new_and_edit(&mut self, el: El, window: &mut Window, cx: &mut Context<Self>) {
        let id = el.id;
        self.doc.els.push(el);
        self.doc.layout_minds();
        self.sel = [id].into();
        self.commit(cx);
        self.start_edit(id, window, cx);
        if let Some(ed) = self.editing.as_ref() {
            ed.area.update(cx, |a, _| a.select_all_text());
        }
    }

    /// Tab / 点「+」：添加子节点（收起的节点先展开）。`side` 只对根节点有效：点左侧「+」加到左边（-1），右侧加到右边（1），0 按布局自动。
    pub(super) fn mind_add_child(&mut self, id: u64, side: i8, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_edit(cx);
        self.snapshot();
        if let Some(m) = self.doc.index(id).and_then(|i| self.doc.els[i].mind.as_mut()) {
            m.collapsed = false;
        }
        let kids = self.doc.mind_children(id);
        let level = self.doc.mind_level(id) + 1;
        // 往某一侧加时排在这一侧最后一个节点之后，否则排在最后
        let same_side = kids.iter().rfind(|k| self.doc.get(**k).and_then(|e| e.mind.as_ref()).is_some_and(|m| m.dir == side)).copied();
        let seq = match same_side.filter(|_| level == 1 && side != 0) {
            Some(last) => {
                let s = self.doc.get(last).and_then(|e| e.mind.as_ref()).map(|m| m.seq).unwrap_or(0.);
                for e in self.doc.els.iter_mut() {
                    if let Some(m) = e.mind.as_mut().filter(|m| m.parent == id && m.seq > s) {
                        m.seq += 1.;
                    }
                }
                s + 1.
            }
            None => kids.iter().filter_map(|k| self.doc.get(*k).and_then(|e| e.mind.as_ref()).map(|m| m.seq)).fold(-1f32, f32::max) + 1.,
        };
        let at = self.doc.get(id).map(|e| e.rect().center()).unwrap_or_default();
        let mut el = self.doc.new_mind_node(id, seq, if level == 1 { "分支主题" } else { "子主题" }, at, Self::dark(cx));
        if let Some(m) = el.mind.as_mut().filter(|_| level == 1) {
            m.side = side;
            m.dir = if side == 0 { 1 } else { side };
        }
        self.mind_new_and_edit(el, window, cx);
    }

    /// Enter：添加兄弟节点（根节点没有兄弟，改为添加子节点）。
    pub(super) fn mind_add_sibling(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some((parent, seq)) = self.doc.get(id).and_then(|e| e.mind.as_ref()).map(|m| (m.parent, m.seq)) else { return };
        if parent == 0 || self.doc.get(parent).is_none() {
            return self.mind_add_child(id, 0, window, cx);
        }
        self.finish_edit(cx);
        self.snapshot();
        // 插在当前节点之后：后面的兄弟顺延
        for e in self.doc.els.iter_mut() {
            if let Some(m) = e.mind.as_mut().filter(|m| m.parent == parent && m.seq > seq) {
                m.seq += 1.;
            }
        }
        let at = self.doc.get(id).map(|e| e.rect().center()).unwrap_or_default();
        let mut el = self.doc.new_mind_node(parent, seq + 1., "", at, Self::dark(cx));
        // 兄弟节点沿用当前节点的样式
        if let Some(src) = self.doc.get(id) {
            el.kind = src.kind.clone();
            el.fill = src.fill;
            el.stroke = src.stroke;
            el.no_stroke = src.no_stroke;
            el.font_size = src.font_size;
            el.text_color = src.text_color;
            // 一级分支的兄弟留在同一侧
            if let (Some(m), Some(sm)) = (el.mind.as_mut(), src.mind.as_ref()) {
                m.side = sm.side;
                m.dir = sm.dir;
            }
        }
        el.text = if self.doc.mind_level(parent) == 0 { "分支主题".into() } else { "子主题".into() };
        self.mind_new_and_edit(el, window, cx);
    }

    /// ⇧Tab：在当前节点与父节点之间插入一个新的父节点。
    pub(super) fn mind_add_parent(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some((parent, seq)) = self.doc.get(id).and_then(|e| e.mind.as_ref()).map(|m| (m.parent, m.seq)) else { return };
        if parent == 0 {
            return;
        }
        self.finish_edit(cx);
        self.snapshot();
        let at = self.doc.get(id).map(|e| e.rect().center()).unwrap_or_default();
        let mut el = self.doc.new_mind_node(parent, seq, "新主题", at, Self::dark(cx));
        let nid = el.id;
        if let Some(i) = self.doc.index(id) {
            if let Some(m) = self.doc.els[i].mind.as_mut() {
                m.parent = nid;
                m.seq = 0.;
                // 新父节点接替原来所在的一侧
                if let Some(nm) = el.mind.as_mut() {
                    nm.side = std::mem::take(&mut m.side);
                }
            }
        }
        self.doc.els.push(el);
        self.doc.restyle_mind_subtree(nid);
        self.doc.layout_minds();
        self.sel = [nid].into();
        self.commit(cx);
        self.start_edit(nid, window, cx);
        if let Some(ed) = self.editing.as_ref() {
            ed.area.update(cx, |a, _| a.select_all_text());
        }
    }

    /// ⌘.：收起 / 展开选中节点的子节点。
    pub(super) fn mind_toggle_collapse(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self.sel.iter().copied().filter(|id| !self.doc.mind_children(*id).is_empty()).collect();
        if ids.is_empty() {
            return;
        }
        let collapse = !ids.iter().all(|id| self.doc.get(*id).and_then(|e| e.mind.as_ref()).is_some_and(|m| m.collapsed));
        self.snapshot();
        for id in ids {
            if let Some(m) = self.doc.index(id).and_then(|i| self.doc.els[i].mind.as_mut()) {
                m.collapsed = collapse;
            }
        }
        self.commit(cx);
    }

    pub(super) fn mind_set_layout(&mut self, layout: u8, cx: &mut Context<Self>) {
        let roots: HashSet<u64> = self.sel.iter().filter(|id| self.doc.get(**id).is_some_and(|e| e.is_mind())).map(|id| self.doc.mind_root(*id)).collect();
        if roots.is_empty() {
            return;
        }
        self.snapshot();
        for r in roots {
            if let Some(m) = self.doc.index(r).and_then(|i| self.doc.els[i].mind.as_mut()) {
                m.layout = layout;
            }
        }
        self.commit(cx);
    }

    pub(super) fn mind_set_branch(&mut self, branch: u8, cx: &mut Context<Self>) {
        let roots: HashSet<u64> = self.sel.iter().filter(|id| self.doc.get(**id).is_some_and(|e| e.is_mind())).map(|id| self.doc.mind_root(*id)).collect();
        if roots.is_empty() {
            return;
        }
        self.snapshot();
        for r in roots {
            if let Some(m) = self.doc.index(r).and_then(|i| self.doc.els[i].mind.as_mut()) {
                m.branch = branch;
            }
        }
        self.commit(cx);
    }

    /// 拖动节点松开：落在另一个节点上就挂到它下面（不能挂到自己的后代上）。
    pub(super) fn mind_drop(&mut self, id: u64, target: Option<u64>) {
        let Some(t) = target else { return };
        if self.doc.mind_subtree(id).contains(&t) {
            return;
        }
        let seq = self.doc.mind_children(t).len() as f32;
        if let Some(i) = self.doc.index(id) {
            if let Some(m) = self.doc.els[i].mind.as_mut() {
                m.parent = t;
                m.seq = seq;
                m.side = 0;
            }
        }
        if let Some(m) = self.doc.index(t).and_then(|i| self.doc.els[i].mind.as_mut()) {
            m.collapsed = false;
        }
        self.doc.restyle_mind_subtree(id);
    }

    /// 拖动节点时鼠标下方可以作为新父节点的节点。
    pub(super) fn mind_drop_target(&self, id: u64, p: Pt) -> Option<u64> {
        let sub: HashSet<u64> = self.doc.mind_subtree(id).into_iter().collect();
        let lk = self.doc.lookup();
        self.doc
            .els
            .iter()
            .rev()
            .find(|e| e.is_mind() && !sub.contains(&e.id) && lk.visible(e.id) && e.rect().inflate(6. / self.zoom).contains(p))
            .map(|e| e.id)
    }

    /// 节点的「+」在哪些边，以及从那里加的子节点放在根节点哪一侧（0 自动）。
    /// 根节点左右各一个（往两边都能加），其他节点只在子节点展开的那一侧。
    fn mind_plus_sides(&self, e: &El) -> Vec<(Side, i8)> {
        let (layout, _) = self.doc.mind_root_style_pub(e.id);
        let dir = e.mind.as_ref().map(|m| m.dir).unwrap_or(1);
        if e.mind_parent().is_none() {
            return match layout {
                0 | 1 | 3 | 6 | 8 => vec![(Side::Left, -1), (Side::Right, 1)],
                7 => vec![(Side::Right, 0)],
                _ => vec![(Side::Bottom, 0)],
            };
        }
        match layout {
            0 | 1 | 3 => vec![(if dir < 0 { Side::Left } else { Side::Right }, 0)],
            _ => vec![(Side::Bottom, 0)],
        }
    }

    /// 分支线从哪条边接进来（根节点没有）。
    fn mind_in_side(&self, e: &El) -> Option<Side> {
        let pid = e.mind_parent()?;
        let (layout, _) = self.doc.mind_root_style_pub(e.id);
        let dir = e.mind.as_ref().map(|m| m.dir).unwrap_or(1);
        let parent_root = self.doc.get(pid).is_some_and(|p| p.mind_parent().is_none());
        Some(match edge_kind(layout, parent_root, dir) {
            Edge::Down => Side::Top,
            Edge::TimeH => Side::Left,
            Edge::Indent(d) if d < 0. => Side::Right,
            Edge::Indent(_) => Side::Left,
            _ if dir < 0 => Side::Right,
            _ => Side::Left,
        })
    }

    /// 思维导图节点上可以拖出连线的边：除去分支线和「+」占用的边（向右的导图是上下两个圆点）。
    pub(super) fn mind_link_sides(&self, e: &El) -> Vec<Side> {
        let used: Vec<Side> = self.mind_plus_sides(e).into_iter().map(|x| x.0).chain(self.mind_in_side(e)).collect();
        let (layout, _) = self.doc.mind_root_style_pub(e.id);
        Side::ALL.into_iter().filter(|s| !used.contains(s) && !(e.mind_parent().is_none() && layout >= 6 && *s == Side::Bottom)).collect()
    }

    fn mind_side_pt(&self, e: &El, side: Side) -> Pt {
        let r = e.rect();
        let (ax, ay) = side.anchor();
        pt(r.x + ax * r.w, r.y + ay * r.h).add(side.dir().scale(14. / self.zoom))
    }

    /// 选中节点旁的「+」按钮：(节点, 位置, 新子节点放哪一侧)。收起时徽标占用的那个不显示。
    pub(super) fn mind_buttons(&self) -> Vec<(u64, Pt, i8)> {
        let Some(e) = self.single_mind() else { return vec![] };
        if self.editing.is_some() || self.drag.is_some() || e.locked {
            return vec![];
        }
        let sides = self.mind_plus_sides(e);
        let collapsed = e.mind.as_ref().is_some_and(|m| m.collapsed);
        let n = sides.len();
        sides.into_iter().enumerate().filter(|(i, _)| !(collapsed && *i == n - 1)).map(|(_, (s, side))| (e.id, self.mind_side_pt(e, s), side)).collect()
    }

    /// 收起的节点旁边一直显示的数量徽标（不必选中）。
    pub(super) fn mind_badges(&self) -> Vec<(u64, Pt, usize)> {
        let lk = self.doc.lookup();
        self.doc
            .els
            .iter()
            .filter(|e| e.mind.as_ref().is_some_and(|m| m.collapsed) && lk.visible(e.id))
            .filter_map(|e| {
                let (s, _) = *self.mind_plus_sides(e).last()?;
                Some((e.id, self.mind_side_pt(e, s), self.doc.mind_subtree(e.id).len() - 1))
            })
            .collect()
    }

    pub(super) fn insert_mind(&mut self, at: Pt, layout: u8, window: &mut Window, cx: &mut Context<Self>) {
        self.snapshot();
        let rid = self.doc.insert_mind_map(at, layout, Self::dark(cx));
        self.sel = [rid].into();
        self.tool = Tool::Select;
        self.commit(cx);
        let _ = window;
    }
}

impl BoardDoc {
    pub fn mind_root_style_pub(&self, id: u64) -> (u8, u8) {
        self.mind_root_style(id)
    }
}

/// 布局缩略图（插入面板）：按真实布局排一个小导图，根节点画成描边胶囊、其他节点画成浅灰短条。
pub fn mind_thumb(layout: u8, ink: Hsla, w: f32, h: f32) -> AnyElement {
    let mut d = BoardDoc::new(false);
    let root = d.insert_mind_map(pt(0., 0.), layout, false);
    // 的缩略图：根节点 + 三个分支，直角分支线
    if let Some(m) = d.index(root).and_then(|i| d.els[i].mind.as_mut()) {
        m.branch = 1;
    }
    for e in d.els.iter_mut() {
        e.text.clear();
        e.font_size = if e.mind_parent().is_none() { 18. } else { 13. };
        e.bold = false;
    }
    d.layout_minds();
    let Some(bounds) = d.els.iter().map(|e| e.rect()).reduce(|a, b| a.union(&b)) else { return div().into_any_element() };
    let lk = d.lookup();
    let mut scene = Scene::default();
    d.mind_edges(&lk, &mut scene);
    let s = ((w - 12.) / bounds.w).min((h - 12.) / bounds.h).min(0.5);
    let col = Col::from_hsla(ink);
    let bar = Col::from_hsla(ink.opacity(0.16));
    for it in scene.items.iter_mut() {
        if let Item::Line(l) = it {
            l.stroke = col;
            l.width = 1.3 / s;
        }
    }
    for e in d.els.iter() {
        let r = e.rect();
        let root = e.mind_parent().is_none();
        let r = if root { r } else { rect(r.x + 2., r.center().y - r.h * 0.22, r.w - 4., r.h * 0.44) };
        let mut sh = ShapeItem::new(ShapeKind::RoundRect, r);
        sh.radius = r.h / 2.;
        if root {
            sh.fill = Some(Col::from_hsla(ink.opacity(0.28)));
            sh.stroke = Some(col);
            sh.stroke_w = 1.6 / s;
        } else {
            sh.fill = Some(bar);
            sh.stroke = None;
        }
        scene.items.push(Item::Shape(sh));
    }
    let off = pt((w - bounds.w * s) / 2. - bounds.x * s, (h - bounds.h * s) / 2. - bounds.y * s);
    let scene = std::sync::Arc::new(scene);
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .child(
            canvas(
                |b, _, _| b,
                move |b, _, win, cx| {
                    crate::canvas::paint::paint_scene(
                        &scene,
                        crate::canvas::paint::View { origin: point(b.origin.x + px(off.x), b.origin.y + px(off.y)), scale: s },
                        b,
                        win,
                        cx,
                    )
                },
            )
            .size_full(),
        )
        .into_any_element()
}

impl BoardEditor {
    /// 布局卡片网格（插入面板 / 切换布局共用）：按「思维导图 / 树状图 / 时间线」分组，每组两列。
    pub(super) fn mind_layout_cards(
        &self,
        id: &'static str,
        current: Option<u8>,
        t: &crate::theme::Theme,
        cx: &mut Context<Self>,
        pick: fn(&mut Self, u8, &mut Context<Self>),
    ) -> Div {
        let mut col = div().flex().flex_col();
        let mut group = "";
        let mut grid = div();
        for (k, g, name) in LAYOUTS {
            if g != group {
                if !group.is_empty() {
                    col = col.child(grid);
                }
                group = g;
                col = col.child(div().px(px(4.)).pt(px(if col_is_first(k) { 0. } else { 8. })).pb(px(6.)).text_size(px(12.)).text_color(t.text_muted).child(g));
                grid = div().flex().flex_wrap().gap(px(8.));
            }
            let on = current == Some(k);
            grid = grid.child(
                div()
                    .id((id, k as usize))
                    .w(px(92.))
                    .h(px(64.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if on { t.accent } else { t.border })
                    .when(on, |d| d.bg(t.accent_soft))
                    .hover(|s| s.bg(t.hover))
                    .cursor_pointer()
                    .tooltip(move |_, cx| cx.new(|_| crate::ui::Tooltip(format!("{g} · {name}").into())).into())
                    .child(mind_thumb(k, t.text, 88., 60.))
                    .on_click(cx.listener(move |this, _, _, cx| pick(this, k, cx))),
            );
        }
        col.child(grid)
    }
}

/// 是否是第一个分组的第一项（标题上方不留空）。
fn col_is_first(layout: u8) -> bool {
    LAYOUTS[0].0 == layout
}

/// 节点在世界坐标中的矩形（测试用）。
#[allow(dead_code)]
pub fn mind_rects(doc: &BoardDoc) -> Vec<(u64, Rect)> {
    doc.els.iter().filter(|e| e.is_mind()).map(|e| (e.id, e.rect())).collect()
}

#[cfg(test)]
mod tests {
    // 别用 `super::*`：会把 gpui 的 `test` 宏引进作用域
    use super::super::model::BoardDoc;
    use crate::canvas::{pt, Rect};

    #[test]
    fn mind_layout_right_and_collapse() {
        let mut b = BoardDoc::new(false);
        let root = b.insert_mind_map(pt(0., 0.), 0, false);
        let kids = b.mind_children(root);
        assert_eq!(kids.len(), 3);
        let r = b.get(root).unwrap().rect();
        // 子节点都在根节点右侧，且自上而下排列、互不重叠
        let rects: Vec<Rect> = kids.iter().map(|k| b.get(*k).unwrap().rect()).collect();
        assert!(rects.iter().all(|c| c.x > r.right()));
        for w in rects.windows(2) {
            assert!(w[1].y >= w[0].bottom());
        }
        // 根节点中心保持在落点
        assert!((r.center().x).abs() < 0.5 && (r.center().y).abs() < 0.5);
        // 收起后子节点隐藏
        let i = b.index(root).unwrap();
        b.els[i].mind.as_mut().unwrap().collapsed = true;
        assert_eq!(b.lookup().hidden.len(), 3);
    }

    /// 每种布局下节点互不重叠（再挂几层子节点）。
    #[test]
    fn mind_all_layouts_no_overlap() {
        for (layout, _, _) in super::LAYOUTS {
            let mut b = BoardDoc::new(false);
            let root = b.insert_mind_map(pt(0., 0.), layout, false);
            let kids = b.mind_children(root);
            for (i, k) in kids.iter().enumerate() {
                for j in 0..=i {
                    let el = b.new_mind_node(*k, j as f32, &format!("子主题 {i}-{j}"), pt(0., 0.), false);
                    let gid = el.id;
                    b.els.push(el);
                    if j == 0 {
                        let g = b.new_mind_node(gid, 0., "孙", pt(0., 0.), false);
                        b.els.push(g);
                    }
                }
            }
            b.layout_minds();
            let rects: Vec<Rect> = b.els.iter().map(|e| e.rect()).collect();
            for (i, a) in rects.iter().enumerate() {
                for c in &rects[i + 1..] {
                    assert!(!a.inflate(-1.).intersects(c), "布局 {layout} 节点重叠：{a:?} {c:?}");
                }
            }
        }
    }

    /// 根节点左侧「+」：子节点放到左边；横向时间线上下交替。
    #[test]
    fn mind_explicit_side_and_timeline() {
        let mut b = BoardDoc::new(false);
        let root = b.insert_mind_map(pt(0., 0.), 0, false);
        let mut el = b.new_mind_node(root, 9., "左", pt(0., 0.), false);
        el.mind.as_mut().unwrap().side = -1;
        let lid = el.id;
        b.els.push(el);
        b.layout_minds();
        let rc = b.get(root).unwrap().rect();
        assert!(b.get(lid).unwrap().rect().right() < rc.x);
        assert!(b.mind_children(root).iter().filter(|k| **k != lid).all(|k| b.get(*k).unwrap().rect().x > rc.right()));

        let mut b = BoardDoc::new(false);
        let root = b.insert_mind_map(pt(0., 0.), 7, false);
        let ty = b.get(root).unwrap().rect().center().y;
        let kids = b.mind_children(root);
        assert!(b.get(kids[0]).unwrap().rect().bottom() < ty);
        assert!(b.get(kids[1]).unwrap().rect().y > ty);
        assert!(b.get(kids[2]).unwrap().rect().bottom() < ty);
    }

    #[test]
    fn mind_both_sides_and_outline() {
        let mut b = BoardDoc::new(false);
        let lines: Vec<(usize, String)> =
            [(1, "根"), (2, "甲"), (3, "甲1"), (2, "乙"), (2, "丙"), (2, "丁")].iter().map(|(l, t)| (*l, t.to_string())).collect();
        let root = b.mind_from_outline(&lines, pt(0., 0.), 1, false).unwrap();
        let rc = b.get(root).unwrap().rect().center();
        let kids = b.mind_children(root);
        assert_eq!(kids.len(), 4);
        let left = kids.iter().filter(|k| b.get(**k).unwrap().rect().center().x < rc.x).count();
        assert!((1..=3).contains(&left));
        assert_eq!(b.mind_level(b.mind_children(kids[0])[0]), 2);
    }
}
