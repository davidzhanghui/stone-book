//! ER 图：`erDiagram`。

use super::flowchart::{GEdge, Graph};
use super::sugiyama::Dir;
use super::*;
use crate::canvas::{Arrow, Section, ShapeKind};

fn card(tok: &str, left: bool) -> Option<Arrow> {
    Some(match (tok, left) {
        ("|o", true) | ("o|", false) => Arrow::ZeroOrOne,
        ("||", _) => Arrow::One,
        ("}o", true) | ("o{", false) => Arrow::ZeroOrMany,
        ("}|", true) | ("|{", false) => Arrow::OneOrMany,
        _ => return None,
    })
}

pub fn render(body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure, offsets: &BTreeMap<String, (f32, f32)>) -> DResult<Scene> {
    let mut g = Graph::new(Dir::TB);
    g.curved = false;
    let mut attrs: Vec<Vec<String>> = vec![];
    let mut open: Option<usize> = None;
    let ensure = |g: &mut Graph, attrs: &mut Vec<Vec<String>>, name: &str| -> usize {
        let i = g.node(name.trim(), None);
        while attrs.len() <= i {
            attrs.push(vec![]);
        }
        i
    };
    for (line, raw) in body {
        let line = *line;
        let s = raw.trim();
        if let Some(cur) = open {
            if s == "}" {
                open = None;
            } else {
                // string name PK "注释"
                let mut parts: Vec<String> = vec![];
                let mut rest = s;
                let mut comment = String::new();
                if let Some(q) = rest.find('"') {
                    comment = rest[q..].trim_matches('"').to_string();
                    rest = &rest[..q];
                }
                parts.extend(rest.split_whitespace().map(|x| x.to_string()));
                let mut text = parts.join("  ");
                if !comment.is_empty() {
                    text = format!("{text}  // {comment}");
                }
                attrs[cur].push(text);
            }
            continue;
        }
        if s.to_lowercase().starts_with("direction") {
            if let Some(d) = s.split_whitespace().nth(1).and_then(Dir::parse) {
                g.dir = d;
            }
            continue;
        }
        if let Some(name) = s.strip_suffix('{') {
            let name = name.trim();
            let (id, label) = match name.find('[') {
                Some(b) => (&name[..b], Some(clean_text(name[b + 1..].trim_end_matches(']')))),
                None => (name, None),
            };
            let i = ensure(&mut g, &mut attrs, id);
            if let Some(l) = label {
                g.nodes[i].text = l;
            }
            open = Some(i);
            continue;
        }
        // CUSTOMER ||--o{ ORDER : places
        let (rel, label) = match s.split_once(':') {
            Some((r, l)) => (r.trim(), clean_text(l)),
            None => (s, String::new()),
        };
        let toks: Vec<&str> = rel.split_whitespace().collect();
        if toks.len() == 3 {
            let op = toks[1];
            let line_pos = op.find("--").or_else(|| op.find(".."));
            if let Some(lp) = line_pos {
                let (l, r) = (&op[..lp], &op[lp + 2..]);
                let (Some(la), Some(ra)) = (card(l, true), card(r, false)) else {
                    return Err(DiagramError::new(line, format!("无法识别的基数写法：{op}")));
                };
                let a = ensure(&mut g, &mut attrs, toks[0]);
                let b = ensure(&mut g, &mut attrs, toks[2]);
                g.edges.push(GEdge { from: a, to: b, label, start: la, end: ra, dash: op.contains(".."), minlen: 1, ..Default::default() });
                continue;
            }
        }
        if toks.len() == 1 && !toks[0].is_empty() {
            ensure(&mut g, &mut attrs, toks[0]);
            continue;
        }
        return Err(DiagramError::new(line, format!("无法解析：{s}")));
    }
    if g.nodes.is_empty() {
        return Err(DiagramError::new(0, "ER 图至少需要一个实体"));
    }
    for (i, n) in g.nodes.iter_mut().enumerate() {
        n.shape = ShapeKind::Rect;
        let mut secs = vec![Section { lines: vec![n.text.clone()], left: false, bold: true }];
        if !attrs[i].is_empty() {
            secs.push(Section { lines: attrs[i].clone(), left: true, bold: false });
        }
        n.sections = secs;
    }
    Ok(g.to_scene(theme, m, offsets))
}
