//! 状态图：`stateDiagram` / `stateDiagram-v2`。

use super::flowchart::{GCluster, GEdge, Graph};
use super::sugiyama::Dir;
use super::*;
use crate::canvas::{Arrow, ShapeKind};

pub fn render(body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure, offsets: &BTreeMap<String, (f32, f32)>) -> DResult<Scene> {
    let mut g = Graph::new(Dir::TB);
    let mut stack: Vec<usize> = vec![]; // 复合状态 → 子图
    let mut scope_ids: Vec<String> = vec!["root".into()];
    let mut note_n = 0;
    let state_node = |g: &mut Graph, stack: &Vec<usize>, scope: &str, name: &str, as_target: bool| -> usize {
        let name = name.trim();
        if name == "[*]" {
            let id = format!("{scope}__{}", if as_target { "end" } else { "start" });
            let i = g.node(&id, stack.last().copied());
            g.nodes[i].shape = if as_target { ShapeKind::EndDot } else { ShapeKind::StartDot };
            g.nodes[i].fixed = Some(if as_target { (22., 22.) } else { (18., 18.) });
            g.nodes[i].text = String::new();
            return i;
        }
        let i = g.node(name, stack.last().copied());
        if g.nodes[i].shape == ShapeKind::Rect {
            g.nodes[i].shape = ShapeKind::RoundRect;
        }
        i
    };
    for (line, raw) in body {
        let line = *line;
        let s = raw.trim();
        let lower = s.to_lowercase();
        if lower.starts_with("direction") {
            if let Some(d) = s.split_whitespace().nth(1).and_then(Dir::parse) {
                if stack.is_empty() {
                    g.dir = d;
                }
            }
            continue;
        }
        if s == "}" {
            stack.pop();
            scope_ids.pop();
            continue;
        }
        if s == "--" || lower.starts_with("classdef") || lower.starts_with("class ") || lower.starts_with("style") {
            continue;
        }
        if lower.starts_with("note") {
            // note right of X : text
            let (head, text) = match s.split_once(':') {
                Some((h, t)) => (h, clean_text(t)),
                None => (s, String::new()),
            };
            let target = head.split_whitespace().last().unwrap_or("");
            let id = format!("__note{note_n}");
            note_n += 1;
            let ni = g.node(&id, stack.last().copied());
            g.nodes[ni].shape = ShapeKind::Note;
            g.nodes[ni].text = text;
            if let Some(&ti) = g.index.get(target) {
                g.edges.push(GEdge { from: ti, to: ni, dash: true, dotted: true, end: Arrow::None, minlen: 1, ..Default::default() });
            }
            continue;
        }
        if let Some(rest) = s.strip_prefix("state ") {
            let rest = rest.trim();
            // state "描述" as S1 / state X { / state F <<fork>>
            if let Some(stripped) = rest.strip_suffix('{') {
                let name = stripped.trim();
                let (id, title) = match name.split_once(" as ") {
                    Some((t, id)) => (id.trim().to_string(), clean_text(t)),
                    None => (name.to_string(), name.to_string()),
                };
                g.clusters.push(GCluster { title, parent: stack.last().copied() });
                stack.push(g.clusters.len() - 1);
                scope_ids.push(id);
                continue;
            }
            if let Some((t, id)) = rest.split_once(" as ") {
                let i = state_node(&mut g, &stack, scope_ids.last().unwrap(), id, false);
                g.nodes[i].text = clean_text(t);
                continue;
            }
            if let Some((name, kind)) = rest.split_once("<<") {
                let i = state_node(&mut g, &stack, scope_ids.last().unwrap(), name, false);
                match kind.trim_end_matches(">>").trim() {
                    "fork" | "join" => {
                        g.nodes[i].shape = ShapeKind::Bar;
                        g.nodes[i].fixed = Some(if g.dir.horizontal() { (8., 70.) } else { (70., 8.) });
                        g.nodes[i].text = String::new();
                    }
                    "choice" => {
                        g.nodes[i].shape = ShapeKind::Diamond;
                        g.nodes[i].fixed = Some((28., 28.));
                        g.nodes[i].text = String::new();
                    }
                    _ => {}
                }
                continue;
            }
            state_node(&mut g, &stack, scope_ids.last().unwrap(), rest, false);
            continue;
        }
        if let Some(pos) = s.find("-->") {
            let left = &s[..pos];
            let (right, label) = match s[pos + 3..].split_once(':') {
                Some((r, l)) => (r, clean_text(l)),
                None => (&s[pos + 3..], String::new()),
            };
            let scope = scope_ids.last().unwrap().clone();
            let a = state_node(&mut g, &stack, &scope, left, false);
            let b = state_node(&mut g, &stack, &scope, right, true);
            g.edges.push(GEdge { from: a, to: b, label, end: Arrow::Filled, minlen: 1, ..Default::default() });
            continue;
        }
        // S1 : 描述
        if let Some((id, desc)) = s.split_once(':') {
            let i = state_node(&mut g, &stack, scope_ids.last().unwrap(), id, false);
            let d = clean_text(desc);
            g.nodes[i].text = if g.nodes[i].text == g.nodes[i].id { format!("{}\n{}", g.nodes[i].id, d) } else { d };
            continue;
        }
        if !s.is_empty() && !s.contains(char::is_whitespace) {
            state_node(&mut g, &stack, scope_ids.last().unwrap(), s, false);
            continue;
        }
        return Err(DiagramError::new(line, format!("无法解析：{s}")));
    }
    if !stack.is_empty() {
        return Err(DiagramError::new(body.last().map(|b| b.0).unwrap_or(0), "复合状态缺少 }"));
    }
    if g.nodes.is_empty() {
        return Err(DiagramError::new(0, "状态图至少需要一个状态"));
    }
    Ok(g.to_scene(theme, m, offsets))
}
