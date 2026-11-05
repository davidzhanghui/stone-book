//! 文档图谱：节点 = 文档，边 = 文档链接（`stonebrook://doc/…`）。默认显示当前文档两跳以内的邻居，可切到全部文档。
//! 布局用 Fruchterman–Reingold 力导向（确定性初值，打开时算一次），渲染时等比适配到窗口。

use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

struct Node {
    id: String,
    title: String,
    x: f32,
    y: f32,
    deg: usize,
}

pub struct Graph {
    nodes: Vec<Node>,
    edges: Vec<(usize, usize)>,
    all: bool,
    center: String,
    focus: FocusHandle,
}

/// 两跳以内的邻居（无向）。
fn neighborhood(center: &str, edges: &[(String, String)], hops: usize) -> HashSet<String> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    let mut seen: HashSet<String> = HashSet::from([center.to_string()]);
    let mut q = VecDeque::from([(center, 0usize)]);
    while let Some((n, d)) = q.pop_front() {
        if d == hops {
            continue;
        }
        for &m in adj.get(n).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(m.to_string()) {
                q.push_back((m, d + 1));
            }
        }
    }
    seen
}

/// Fruchterman–Reingold：返回每个节点的 (x, y)，范围大致在 [-1, 1]。
fn layout(n: usize, edges: &[(usize, usize)]) -> Vec<(f32, f32)> {
    if n == 0 {
        return vec![];
    }
    let mut pos: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let a = i as f32 * 2.399_963; // 黄金角，避免初始重叠
            let r = (i as f32 + 1.).sqrt() / (n as f32).sqrt();
            (r * a.cos(), r * a.sin())
        })
        .collect();
    let k = (4. / n as f32).sqrt().max(0.05);
    let mut temp = 0.2f32;
    for _ in 0..300 {
        let mut disp = vec![(0f32, 0f32); n];
        for i in 0..n {
            for j in i + 1..n {
                let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                let d = (dx * dx + dy * dy).sqrt().max(0.01);
                let f = k * k / d;
                let (ux, uy) = (dx / d * f, dy / d * f);
                disp[i].0 += ux;
                disp[i].1 += uy;
                disp[j].0 -= ux;
                disp[j].1 -= uy;
            }
        }
        for &(a, b) in edges {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let f = d * d / k;
            let (ux, uy) = (dx / d * f, dy / d * f);
            disp[a].0 -= ux;
            disp[a].1 -= uy;
            disp[b].0 += ux;
            disp[b].1 += uy;
        }
        for i in 0..n {
            // 轻微向心力：孤立节点不会飘太远
            disp[i].0 -= pos[i].0 * 0.05;
            disp[i].1 -= pos[i].1 * 0.05;
            let d = (disp[i].0 * disp[i].0 + disp[i].1 * disp[i].1).sqrt().max(1e-6);
            let m = d.min(temp);
            pos[i].0 += disp[i].0 / d * m;
            pos[i].1 += disp[i].1 / d * m;
        }
        temp = (temp * 0.97).max(0.002);
    }
    pos
}

impl Workspace {
    fn build_graph(&self, all: bool, focus: FocusHandle) -> Graph {
        let docs: Vec<&DocMeta> = self.docs.iter().filter(|d| !d.deleted).collect();
        let alive: HashSet<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        let raw: Vec<(String, String)> =
            self.store.link_edges().into_iter().filter(|(a, b)| alive.contains(a.as_str()) && alive.contains(b.as_str())).collect();
        let keep = (!all).then(|| neighborhood(&self.current, &raw, 2));
        let nodes: Vec<&DocMeta> = docs.into_iter().filter(|d| keep.as_ref().is_none_or(|k| k.contains(&d.id))).collect();
        let ix: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, d)| (d.id.as_str(), i)).collect();
        let mut edges: Vec<(usize, usize)> = raw.iter().filter_map(|(a, b)| Some((*ix.get(a.as_str())?, *ix.get(b.as_str())?))).collect();
        edges.sort_unstable();
        edges.dedup();
        let pos = layout(nodes.len(), &edges);
        let mut deg = vec![0; nodes.len()];
        for &(a, b) in &edges {
            deg[a] += 1;
            deg[b] += 1;
        }
        Graph {
            nodes: nodes.iter().zip(pos).enumerate().map(|(i, (d, (x, y)))| Node { id: d.id.clone(), title: d.title.clone(), x, y, deg: deg[i] }).collect(),
            edges,
            all,
            center: self.current.clone(),
            focus,
        }
    }

    pub fn open_graph(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |e, cx| e.save_now(cx));
        self.reload_docs();
        let focus = cx.focus_handle();
        window.focus(&focus);
        self.graph = Some(self.build_graph(false, focus));
        cx.notify();
    }

    fn close_graph(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.graph = None;
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }

    pub(super) fn render_graph(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let g = self.graph.as_ref()?;
        let t = theme(cx).clone();
        let vp = window.viewport_size();
        let (w, h) = (f32::from(vp.width), f32::from(vp.height) - 52.);
        let (minx, maxx) = g.nodes.iter().fold((f32::MAX, f32::MIN), |(a, b), n| (a.min(n.x), b.max(n.x)));
        let (miny, maxy) = g.nodes.iter().fold((f32::MAX, f32::MIN), |(a, b), n| (a.min(n.y), b.max(n.y)));
        let pad = 80.;
        let s = ((w - pad * 2.) / (maxx - minx).max(0.01)).min((h - pad * 2.) / (maxy - miny).max(0.01)).min(400.);
        let (cx0, cy0) = ((minx + maxx) / 2., (miny + maxy) / 2.);
        let place = move |x: f32, y: f32| (w / 2. + (x - cx0) * s, h / 2. + (y - cy0) * s);
        let pts: Vec<(f32, f32)> = g.nodes.iter().map(|n| place(n.x, n.y)).collect();
        let segs: Vec<((f32, f32), (f32, f32))> = g.edges.iter().map(|&(a, b)| (pts[a], pts[b])).collect();
        let edge_col = t.text_muted.opacity(0.35);
        let all = g.all;
        let tab = |id: &'static str, label: &'static str, on: bool, to_all: bool| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(4.))
                .rounded(px(6.))
                .text_size(px(13.))
                .cursor_pointer()
                .when(on, |d| d.bg(t.active).text_color(t.text))
                .when(!on, |d| d.text_color(t.text_muted).hover(|s| s.bg(t.hover)))
                .child(crate::i18n::tr(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(f) = this.graph.as_ref().map(|g| g.focus.clone()) {
                        this.graph = Some(this.build_graph(to_all, f));
                    }
                    cx.notify();
                }))
        };
        Some(
            div()
                .id("graph")
                .absolute()
                .inset_0()
                .occlude()
                .track_focus(&g.focus)
                .bg(t.bg)
                .flex()
                .flex_col()
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key == "escape" {
                        this.close_graph(window, cx);
                    }
                }))
                .child(
                    div()
                        .h(px(52.))
                        .flex_none()
                        .pl(px(80.))
                        .pr(px(16.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .border_b_1()
                        .border_color(t.border)
                        .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(t.text).child(crate::i18n::tr("文档图谱")))
                        .child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("{} 篇 · {} 条链接", g.nodes.len(), g.edges.len())))
                        .child(div().w(px(16.)))
                        .child(tab("g-near", "当前文档周边", !all, false))
                        .child(tab("g-all", "全部文档", all, true))
                        .child(div().flex_1())
                        .child(div().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("点击节点打开文档 · Esc 关闭")))
                        .child(
                            div()
                                .id("g-close")
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.bg(t.hover))
                                .child(icon("x", 16., t.text_secondary))
                                .on_click(cx.listener(|this, _, window, cx| this.close_graph(window, cx))),
                        ),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            canvas(
                                |b, _, _| b,
                                move |b, _, window, _| {
                                    for ((x1, y1), (x2, y2)) in &segs {
                                        let mut pb = PathBuilder::stroke(px(1.2));
                                        pb.move_to(point(b.origin.x + px(*x1), b.origin.y + px(*y1)));
                                        pb.line_to(point(b.origin.x + px(*x2), b.origin.y + px(*y2)));
                                        if let Ok(p) = pb.build() {
                                            window.paint_path(p, edge_col);
                                        }
                                    }
                                },
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .children(g.nodes.iter().enumerate().map(|(i, n)| {
                            let (x, y) = pts[i];
                            let r = (6. + (n.deg as f32).sqrt() * 3.).min(18.);
                            let center = n.id == g.center;
                            let id = n.id.clone();
                            let title = if n.title.trim().is_empty() { "未命名文档".to_string() } else { n.title.clone() };
                            div()
                                .id(("g-node", i))
                                .absolute()
                                .left(px(x - 70.))
                                .top(px(y - r))
                                .w(px(140.))
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(4.))
                                .cursor_pointer()
                                .group("gnode")
                                .child(
                                    div()
                                        .size(px(r * 2.))
                                        .rounded_full()
                                        .bg(if center {
                                            t.accent
                                        } else if n.deg == 0 {
                                            t.text_muted.opacity(0.5)
                                        } else {
                                            t.accent.opacity(0.55)
                                        })
                                        .border_2()
                                        .border_color(t.bg)
                                        .group_hover("gnode", |s| s.bg(t.accent)),
                                )
                                .child(
                                    div()
                                        .max_w(px(140.))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(12.))
                                        .text_color(if center { t.text } else { t.text_secondary })
                                        .when(center, |d| d.font_weight(FontWeight::SEMIBOLD))
                                        .child(title),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.close_graph(window, cx);
                                    this.open_doc(&id, window, cx);
                                }))
                        })),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{layout, neighborhood};

    #[test]
    fn two_hop_neighborhood() {
        let e: Vec<(String, String)> = [("a", "b"), ("b", "c"), ("c", "d"), ("x", "y")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let n = neighborhood("a", &e, 2);
        assert!(n.contains("a") && n.contains("b") && n.contains("c"));
        assert!(!n.contains("d") && !n.contains("x"));
    }

    #[test]
    fn layout_separates_nodes() {
        let pos = layout(5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        assert_eq!(pos.len(), 5);
        for i in 0..5 {
            for j in i + 1..5 {
                let d = ((pos[i].0 - pos[j].0).powi(2) + (pos[i].1 - pos[j].1).powi(2)).sqrt();
                assert!(d > 0.05, "节点 {i} 和 {j} 重叠");
            }
            assert!(pos[i].0.is_finite() && pos[i].1.is_finite());
        }
    }
}
