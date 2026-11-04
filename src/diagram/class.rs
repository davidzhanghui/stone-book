//! 类图：`classDiagram`。

use super::flowchart::{GEdge, Graph};
use super::sugiyama::Dir;
use super::*;
use crate::canvas::{Arrow, Section, ShapeKind};

struct ClassInfo {
    annotation: Option<String>,
    label: Option<String>,
    attrs: Vec<String>,
    methods: Vec<String>,
}

fn generic(s: &str) -> String {
    // Square~Shape~ → Square<Shape>
    let mut out = String::new();
    let mut open = true;
    for c in s.chars() {
        if c == '~' {
            out.push(if open { '<' } else { '>' });
            open = !open;
        } else {
            out.push(c);
        }
    }
    out
}

fn add_member(info: &mut ClassInfo, m: &str) {
    let m = generic(m.trim());
    if m.is_empty() {
        return;
    }
    if let Some(a) = m.strip_prefix("<<").and_then(|x| x.strip_suffix(">>")) {
        info.annotation = Some(a.to_string());
        return;
    }
    if m.contains('(') {
        info.methods.push(m)
    } else {
        info.attrs.push(m)
    }
}

/// 关系写法：左端标记 + 线型 + 右端标记
fn parse_relation(s: &str) -> Option<(usize, usize, Arrow, Arrow, bool)> {
    let lefts = [("<|", Arrow::Hollow), ("*", Arrow::DiamondFilled), ("o", Arrow::DiamondHollow), ("<", Arrow::Open), ("", Arrow::None)];
    let rights = [("|>", Arrow::Hollow), ("*", Arrow::DiamondFilled), ("o", Arrow::DiamondHollow), (">", Arrow::Open), ("", Arrow::None)];
    for (i, _) in s.char_indices() {
        for line in ["--", ".."] {
            for (lt, la) in lefts {
                if !s[i..].starts_with(lt) || !s[i + lt.len()..].starts_with(line) {
                    continue;
                }
                let after = i + lt.len() + line.len();
                for (rt, ra) in rights {
                    if s[after..].starts_with(rt) {
                        let end = after + rt.len();
                        // 两侧必须是空白或引号，避免把类名里的字符当成关系
                        let before_ok = i == 0 || s[..i].ends_with(char::is_whitespace) || s[..i].ends_with('"');
                        let after_ok = end >= s.len() || s[end..].starts_with(char::is_whitespace) || s[end..].starts_with('"');
                        if before_ok && after_ok && (lt.len() + rt.len() > 0 || true) {
                            return Some((i, end, la, ra, line == ".."));
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn render(body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure, offsets: &BTreeMap<String, (f32, f32)>) -> DResult<Scene> {
    let mut g = Graph::new(Dir::TB);
    g.curved = false;
    let mut infos: Vec<ClassInfo> = vec![];
    let ensure = |g: &mut Graph, infos: &mut Vec<ClassInfo>, name: &str| -> usize {
        let name = generic(name.trim());
        let i = g.node(&name, None);
        while infos.len() <= i {
            infos.push(ClassInfo { annotation: None, label: None, attrs: vec![], methods: vec![] });
        }
        i
    };
    let mut open: Option<usize> = None;
    let mut notes = vec![];
    for (line, raw) in body {
        let line = *line;
        let s = raw.trim();
        if let Some(cur) = open {
            if s == "}" {
                open = None;
            } else {
                add_member(&mut infos[cur], s);
            }
            continue;
        }
        let lower = s.to_lowercase();
        if lower.starts_with("direction") {
            if let Some(d) = s.split_whitespace().nth(1).and_then(Dir::parse) {
                g.dir = d;
            }
            continue;
        }
        if lower.starts_with("note") {
            notes.push(clean_text(s.split_once('"').map(|x| x.1.trim_end_matches('"')).unwrap_or(&s[4..])));
            continue;
        }
        if lower.starts_with("classdef")
            || lower.starts_with("style")
            || lower.starts_with("cssclass")
            || lower.starts_with("click")
            || lower.starts_with("callback")
            || lower.starts_with("link")
        {
            continue;
        }
        if let Some(rest) = s.strip_prefix("<<") {
            // <<interface>> Animal
            if let Some((ann, name)) = rest.split_once(">>") {
                let i = ensure(&mut g, &mut infos, name);
                infos[i].annotation = Some(ann.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = s.strip_prefix("class ") {
            let rest = rest.trim();
            let (head, has_body) = match rest.strip_suffix('{') {
                Some(h) => (h.trim(), true),
                None => (rest, false),
            };
            // class Animal["动物"]
            let (name, label) = match head.find('[') {
                Some(b) => (&head[..b], Some(clean_text(head[b + 1..].trim_end_matches(']')))),
                None => (head, None),
            };
            let name = name.split(":::").next().unwrap_or(name);
            let i = ensure(&mut g, &mut infos, name);
            if label.is_some() {
                infos[i].label = label;
            }
            if has_body {
                open = Some(i);
            }
            continue;
        }
        if let Some((a, b, la, ra, dashed)) = parse_relation(s) {
            let left = s[..a].trim();
            let right_full = s[b..].trim();
            let (right, label) = match right_full.split_once(':') {
                Some((r, l)) => (r.trim(), clean_text(l)),
                None => (right_full, String::new()),
            };
            // 多重性："1" --> "*"
            let (left, lcard) = split_card(left, true);
            let (right, rcard) = split_card(right, false);
            let li = ensure(&mut g, &mut infos, left);
            let ri = ensure(&mut g, &mut infos, right);
            // 边始终按书写方向从左到右（继承 / 实现时箭头画在父类一端）
            let (from, to, start, end, sl, el) = (li, ri, la, ra, lcard, rcard);
            g.edges.push(GEdge { from, to, start, end, dash: dashed, label, start_label: sl, end_label: el, minlen: 1, ..Default::default() });
            continue;
        }
        // Animal : +int age
        if let Some((name, member)) = s.split_once(':') {
            let i = ensure(&mut g, &mut infos, name);
            add_member(&mut infos[i], member);
            continue;
        }
        if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '~') {
            ensure(&mut g, &mut infos, s);
            continue;
        }
        return Err(DiagramError::new(line, format!("无法解析：{s}")));
    }
    if open.is_some() {
        return Err(DiagramError::new(body.last().map(|b| b.0).unwrap_or(0), "类定义缺少 }"));
    }
    if g.nodes.is_empty() {
        return Err(DiagramError::new(0, "类图至少需要一个类"));
    }
    // 布局时让「父类」在上：继承边（左端是空心三角）的布局方向是 父 → 子，已满足
    for (i, n) in g.nodes.iter_mut().enumerate() {
        let info = &infos[i];
        n.shape = ShapeKind::Rect;
        let mut head = vec![];
        if let Some(a) = &info.annotation {
            head.push(format!("«{a}»"));
        }
        head.push(info.label.clone().unwrap_or_else(|| n.id.clone()));
        n.sections = vec![
            Section { lines: head, left: false, bold: true },
            Section { lines: info.attrs.clone(), left: true, bold: false },
            Section { lines: info.methods.clone(), left: true, bold: false },
        ];
        // 空栏保留一点高度
        for s in n.sections.iter_mut().skip(1) {
            if s.lines.is_empty() {
                s.lines.push(String::new());
            }
        }
    }
    for (k, note) in notes.into_iter().enumerate() {
        let i = g.node(&format!("__note{k}"), None);
        g.nodes[i].shape = ShapeKind::Note;
        g.nodes[i].text = note;
    }
    // 关系方向：B 继承 A 写作 A <|-- B，布局边 A→B，使 A 在上
    Ok(g.to_scene(theme, m, offsets))
}

/// 拆出引号中的多重性。left=true 时引号在末尾，否则在开头。
fn split_card(s: &str, left: bool) -> (&str, String) {
    let s = s.trim();
    if left {
        if let Some(body) = s.strip_suffix('"') {
            if let Some(q) = body.rfind('"') {
                return (s[..q].trim(), body[q + 1..].to_string());
            }
        }
    } else if let Some(rest) = s.strip_prefix('"') {
        if let Some(q) = rest.find('"') {
            return (rest[q + 1..].trim(), rest[..q].to_string());
        }
    }
    (s, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relations_and_members() {
        let src = "classDiagram\n Animal <|-- Duck\n Animal \"1\" *-- \"many\" Leg : has\n Duck ..|> Swimmer\n class Animal {\n  <<abstract>>\n  +int age\n  +isMammal() bool\n }\n Duck : +swim()\n class List~T~";
        let s = super::super::render(src, false, None, &mut crate::canvas::EstimateMeasure, &BTreeMap::new()).unwrap();
        assert!(s.items.len() >= 8);
        let (a, b, la, ra, dash) = parse_relation("A ..|> B").unwrap();
        assert_eq!((la, ra, dash), (Arrow::None, Arrow::Hollow, true));
        assert!(a < b);
        assert_eq!(split_card("A \"1\"", true), ("A", "1".to_string()));
    }
}
