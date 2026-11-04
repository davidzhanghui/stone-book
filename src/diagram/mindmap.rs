//! 思维导图：`mindmap`（缩进表示层级），以及独立思维导图对象共用的树布局。

use super::*;
use crate::canvas::{pt, rect, Align, Arrow, Item, LineItem, Pt, Rect, ShapeItem, ShapeKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MNode {
    pub text: String,
    #[serde(default)]
    pub children: Vec<MNode>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    /// Mermaid 节点形状（独立导图不使用）
    #[serde(skip)]
    pub shape: Option<ShapeKind>,
}

impl MNode {
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(|c| c.count()).sum::<usize>()
    }
}

/// 布局结果：每个节点的矩形、深度、所属分支、在树中的路径。
#[derive(Clone, Debug)]
pub struct Placed {
    pub path: Vec<usize>,
    pub rect: Rect,
    pub depth: usize,
    pub branch: usize,
    pub left: bool,
    pub parent: Option<usize>,
    pub has_children: bool,
    pub collapsed: bool,
}

const HGAP: f32 = 46.;
const VGAP: f32 = 12.;

pub fn node_size(m: &mut dyn Measure, n: &MNode, depth: usize, fs: f32) -> (f32, f32) {
    let size = font_for(depth, fs);
    let (w, h) = text_size(m, if n.text.is_empty() { " " } else { &n.text }, size, depth <= 1);
    match depth {
        0 => (w + 44., h + 24.),
        1 => (w + 30., h + 14.),
        _ => (w + 22., h + 8.),
    }
}

pub fn font_for(depth: usize, fs: f32) -> f32 {
    match depth {
        0 => fs + 5.,
        1 => fs + 1.,
        _ => fs,
    }
}

/// 子树高度（含间距）
fn subtree_h(m: &mut dyn Measure, n: &MNode, depth: usize, fs: f32, cache: &mut Vec<(usize, f32)>, id: &mut usize) -> f32 {
    let my = *id;
    *id += 1;
    let (_, h) = node_size(m, n, depth, fs);
    let mut ch = 0.;
    if !n.collapsed {
        for (k, c) in n.children.iter().enumerate() {
            if k > 0 {
                ch += VGAP;
            }
            ch += subtree_h(m, c, depth + 1, fs, cache, id);
        }
    }
    let total = h.max(ch);
    cache.push((my, total));
    total
}

#[allow(clippy::too_many_arguments)]
fn place(
    m: &mut dyn Measure,
    n: &MNode,
    depth: usize,
    fs: f32,
    x: f32,
    y_center: f32,
    left: bool,
    branch: usize,
    path: Vec<usize>,
    parent: Option<usize>,
    out: &mut Vec<Placed>,
) {
    let (w, h) = node_size(m, n, depth, fs);
    let r = if left { rect(x - w, y_center - h / 2., w, h) } else { rect(x, y_center - h / 2., w, h) };
    let me = out.len();
    out.push(Placed { path: path.clone(), rect: r, depth, branch, left, parent, has_children: !n.children.is_empty(), collapsed: n.collapsed });
    if n.collapsed || n.children.is_empty() {
        return;
    }
    let heights: Vec<f32> = n
        .children
        .iter()
        .map(|c| {
            let mut cache = vec![];
            let mut id = 0;
            subtree_h(m, c, depth + 1, fs, &mut cache, &mut id)
        })
        .collect();
    let total: f32 = heights.iter().sum::<f32>() + VGAP * (heights.len() as f32 - 1.);
    let mut y = y_center - total / 2.;
    let nx = if left { x - w - HGAP } else { x + w + HGAP };
    for (k, c) in n.children.iter().enumerate() {
        let mut p = path.clone();
        p.push(k);
        place(m, c, depth + 1, fs, nx, y + heights[k] / 2., left, branch, p, Some(me), out);
        y += heights[k] + VGAP;
    }
}

/// 左右平衡布局：根节点居中，一级分支按子树高度交替分到两侧。
pub fn layout(m: &mut dyn Measure, root: &MNode, fs: f32, balanced: bool) -> Vec<Placed> {
    let mut out = vec![];
    let (rw, rh) = node_size(m, root, 0, fs);
    out.push(Placed {
        path: vec![],
        rect: rect(-rw / 2., -rh / 2., rw, rh),
        depth: 0,
        branch: 0,
        left: false,
        parent: None,
        has_children: !root.children.is_empty(),
        collapsed: root.collapsed,
    });
    if root.collapsed {
        return out;
    }
    let heights: Vec<f32> = root
        .children
        .iter()
        .map(|c| {
            let mut cache = vec![];
            let mut id = 0;
            subtree_h(m, c, 1, fs, &mut cache, &mut id)
        })
        .collect();
    // 分边：贪心地放到当前较矮的一侧（保持原顺序）
    let mut sides = vec![false; root.children.len()];
    if balanced && root.children.len() > 1 {
        let (mut hl, mut hr) = (0f32, 0f32);
        for (k, h) in heights.iter().enumerate() {
            if hr <= hl {
                hr += h + VGAP;
            } else {
                sides[k] = true;
                hl += h + VGAP;
            }
        }
    }
    for left in [false, true] {
        let ks: Vec<usize> = (0..root.children.len()).filter(|&k| sides[k] == left).collect();
        let total: f32 = ks.iter().map(|&k| heights[k]).sum::<f32>() + VGAP * (ks.len().max(1) as f32 - 1.);
        let mut y = -total / 2.;
        let x = if left { -rw / 2. - HGAP } else { rw / 2. + HGAP };
        for &k in &ks {
            place(m, &root.children[k], 1, fs, x, y + heights[k] / 2., left, k, vec![k], Some(0), &mut out);
            y += heights[k] + VGAP;
        }
    }
    out
}

/// 由布局生成场景。
pub fn to_scene(root: &MNode, placed: &[Placed], theme: &Theme, selected: Option<&[usize]>) -> Scene {
    let mut scene = Scene::default();
    let get = |path: &[usize]| -> &MNode {
        let mut n = root;
        for &k in path {
            n = &n.children[k];
        }
        n
    };
    // 连线（在节点下面）
    for p in placed.iter().skip(1) {
        let par = &placed[p.parent.unwrap_or(0)];
        let col = theme.palette[p.branch % 8];
        let (a, b): (Pt, Pt) = if p.left {
            (pt(par.rect.x + if par.depth == 0 { par.rect.w / 2. } else { 0. }, par.rect.center().y), pt(p.rect.right(), anchor_y(p)))
        } else {
            (pt(par.rect.right() - if par.depth == 0 { par.rect.w / 2. } else { 0. }, par.rect.center().y), pt(p.rect.x, anchor_y(p)))
        };
        let a = if par.depth >= 2 { pt(a.x, anchor_y(par)) } else { a };
        let mx = (a.x + b.x) / 2.;
        // 标准三次贝塞尔：水平出、水平入
        scene.items.push(Item::Path(crate::canvas::PathItem {
            cmds: vec![crate::canvas::PathCmd::Move(a), crate::canvas::PathCmd::Cubic(pt(mx, a.y), pt(mx, b.y), b)],
            fill: None,
            stroke: Some(col),
            width: if p.depth == 1 { 2.6 } else { 1.6 },
            dash: false,
        }));
        let _ = (Arrow::None, LineItem::new(vec![]));
        // 二级以下：节点底部画一条分支色的下划线
        if p.depth >= 2 {
            let mut u = LineItem::new(vec![pt(p.rect.x, p.rect.bottom()), pt(p.rect.right(), p.rect.bottom())]);
            u.stroke = col;
            u.width = 1.6;
            u.end = Arrow::None;
            scene.items.push(Item::Line(u));
        }
    }
    for (i, p) in placed.iter().enumerate() {
        let n = get(&p.path);
        let col = theme.palette[p.branch % 8];
        let kind = n.shape.unwrap_or(match p.depth {
            0 => ShapeKind::RoundRect,
            1 => ShapeKind::RoundRect,
            _ => ShapeKind::Rect,
        });
        let mut s = ShapeItem::new(kind, p.rect);
        s.id = format!("mm:{}", p.path.iter().map(|k| k.to_string()).collect::<Vec<_>>().join("."));
        s.radius = if p.depth == 0 { p.rect.h / 2. } else { 8. };
        s.font_size = font_for(p.depth, theme.font_size);
        s.text = if n.text.is_empty() { " ".into() } else { n.text.clone() };
        s.seed = 60 + i as u64;
        s.align = Align::Center;
        match p.depth {
            0 => {
                s.fill = Some(theme.palette[0]);
                s.stroke = None;
                s.text_color = Col::rgb(0xffffff);
                s.bold = true;
            }
            1 => {
                s.fill = Some(col.with_alpha(if theme.dark { 0.25 } else { 0.12 }));
                s.stroke = Some(col);
                s.stroke_w = 1.6;
                s.text_color = theme.text;
                s.bold = true;
            }
            _ => {
                s.fill = None;
                s.stroke = None;
                s.text_color = theme.text;
            }
        }
        if n.shape.is_some() && p.depth >= 2 {
            s.stroke = Some(col);
        }
        if selected.is_some_and(|sel| sel == p.path.as_slice()) {
            s.stroke = Some(theme.node_stroke);
            s.stroke_w = 2.5;
            if p.depth >= 2 {
                s.fill = Some(theme.node_stroke.with_alpha(0.1));
            }
        }
        scene.items.push(Item::Shape(s));
        // 折叠标记：显示被折叠的子节点数
        if p.collapsed && p.has_children {
            let cx = if p.left { p.rect.x - 12. } else { p.rect.right() + 12. };
            let mut c = ShapeItem::new(ShapeKind::Circle, rect(cx - 9., anchor_y(p) - 9., 18., 18.));
            c.fill = Some(theme.label_bg);
            c.stroke = Some(col);
            c.stroke_w = 1.4;
            c.text = format!("{}", n.count() - 1);
            c.font_size = 10.;
            c.text_color = col;
            scene.items.push(Item::Shape(c));
        }
    }
    scene
}

fn anchor_y(p: &Placed) -> f32 {
    if p.depth >= 2 {
        p.rect.bottom()
    } else {
        p.rect.center().y
    }
}

// ───────────── Mermaid mindmap 语法 ─────────────

fn parse_node_text(s: &str) -> (String, Option<ShapeKind>) {
    let s = s.trim();
    // 去掉图标 / 类名声明
    let s = s.split("::icon").next().unwrap_or(s).trim();
    let shapes: [(&str, &str, ShapeKind); 6] = [
        ("((", "))", ShapeKind::Circle),
        ("))", "((", ShapeKind::Hexagon),
        ("{{", "}}", ShapeKind::Hexagon),
        ("(", ")", ShapeKind::Stadium),
        (")", "(", ShapeKind::Ellipse),
        ("[", "]", ShapeKind::Rect),
    ];
    for (o, c, k) in shapes {
        if let Some(start) = s.find(o) {
            if s.ends_with(c) && start + o.len() <= s.len() - c.len() {
                return (clean_text(&s[start + o.len()..s.len() - c.len()]), Some(k));
            }
        }
    }
    (clean_text(s), None)
}

pub fn parse(body: &[(usize, String)]) -> DResult<MNode> {
    let mut stack: Vec<(usize, MNode)> = vec![];
    let mut root: Option<MNode> = None;
    for (line, raw) in body {
        let indent = raw.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 4 } else { 1 }).sum::<usize>();
        let t = raw.trim();
        if t.starts_with(":::") || t.starts_with("::icon") {
            continue;
        }
        let (text, shape) = parse_node_text(t);
        let node = MNode { text, shape, ..Default::default() };
        // 弹出缩进 >= 当前的节点，挂到父节点
        while let Some((ind, _)) = stack.last() {
            if *ind >= indent {
                let (_, n) = stack.pop().unwrap();
                match stack.last_mut() {
                    Some((_, p)) => p.children.push(n),
                    None => {
                        if root.is_some() {
                            return Err(DiagramError::new(*line, "思维导图只能有一个根节点"));
                        }
                        root = Some(n);
                    }
                }
            } else {
                break;
            }
        }
        if stack.is_empty() && root.is_some() {
            return Err(DiagramError::new(*line, "思维导图只能有一个根节点（检查缩进）"));
        }
        stack.push((indent, node));
    }
    while let Some((_, n)) = stack.pop() {
        match stack.last_mut() {
            Some((_, p)) => p.children.push(n),
            None => root = Some(n),
        }
    }
    root.ok_or_else(|| DiagramError::new(0, "思维导图为空"))
}

pub fn render(body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure) -> DResult<Scene> {
    let root = parse(body)?;
    let placed = layout(m, &root, theme.font_size, true);
    let mut scene = to_scene(&root, &placed, theme, None);
    scene.fit(20.);
    Ok(scene)
}
