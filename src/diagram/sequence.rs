//! 时序图：`sequenceDiagram`。

use super::*;
use crate::canvas::{pt, rect, Align, Arrow, Item, LineItem, ShapeItem, ShapeKind};
use std::collections::HashMap;

struct Participant {
    id: String,
    label: String,
    actor: bool,
}

#[derive(Clone)]
enum Ev {
    Msg { from: usize, to: usize, text: String, dash: bool, end: Arrow, activate: Option<bool>, line: usize },
    Note { a: usize, b: usize, side: i8, text: String },
    BlockStart { kind: String, label: String },
    BlockElse { label: String },
    BlockEnd { line: usize },
    Activate(usize, bool),
}

fn arrow_of(op: &str) -> Option<(bool, Arrow)> {
    Some(match op {
        "->>" => (false, Arrow::Filled),
        "-->>" => (true, Arrow::Filled),
        "->" => (false, Arrow::None),
        "-->" => (true, Arrow::None),
        "-x" => (false, Arrow::Cross),
        "--x" => (true, Arrow::Cross),
        "-)" => (false, Arrow::Open),
        "--)" => (true, Arrow::Open),
        "<<->>" => (false, Arrow::Filled),
        "<<-->>" => (true, Arrow::Filled),
        _ => return None,
    })
}

pub fn render(body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure) -> DResult<Scene> {
    let mut parts: Vec<Participant> = vec![];
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut evs: Vec<Ev> = vec![];
    let mut autonumber = false;
    let mut title = String::new();
    let get = |parts: &mut Vec<Participant>, index: &mut HashMap<String, usize>, id: &str| -> usize {
        let id = id.trim();
        if let Some(&i) = index.get(id) {
            return i;
        }
        parts.push(Participant { id: id.to_string(), label: id.to_string(), actor: false });
        index.insert(id.to_string(), parts.len() - 1);
        parts.len() - 1
    };
    for (line, raw) in body {
        let line = *line;
        let s = raw.trim();
        let lower = s.to_lowercase();
        let word = lower.split_whitespace().next().unwrap_or("");
        match word {
            "participant" | "actor" => {
                let rest = s[word.len()..].trim();
                let (id, label) = match rest.split_once(" as ") {
                    Some((a, b)) => (a.trim(), clean_text(b)),
                    None => (rest, clean_text(rest)),
                };
                let i = get(&mut parts, &mut index, id);
                parts[i].label = label;
                parts[i].actor = word == "actor";
                continue;
            }
            "autonumber" => {
                autonumber = true;
                continue;
            }
            "title" => {
                title = clean_text(s[5..].trim_start_matches(':'));
                continue;
            }
            "activate" | "deactivate" => {
                let i = get(&mut parts, &mut index, s[word.len()..].trim());
                evs.push(Ev::Activate(i, word == "activate"));
                continue;
            }
            "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect" => {
                evs.push(Ev::BlockStart { kind: word.to_string(), label: clean_text(s[word.len()..].trim()) });
                continue;
            }
            "else" | "and" | "option" => {
                evs.push(Ev::BlockElse { label: clean_text(s[word.len()..].trim()) });
                continue;
            }
            "end" => {
                evs.push(Ev::BlockEnd { line });
                continue;
            }
            "note" => {
                // Note right of A: text / Note over A,B: text
                let (head, text) = s.split_once(':').ok_or_else(|| DiagramError::new(line, "注释缺少冒号：Note right of A: 文本"))?;
                let h = head[4..].trim();
                let hl = h.to_lowercase();
                let (side, who) = if let Some(r) = hl.strip_prefix("right of") {
                    (1, &h[h.len() - r.len()..])
                } else if let Some(r) = hl.strip_prefix("left of") {
                    (-1, &h[h.len() - r.len()..])
                } else if let Some(r) = hl.strip_prefix("over") {
                    (0, &h[h.len() - r.len()..])
                } else {
                    return Err(DiagramError::new(line, "注释位置应为 left of / right of / over"));
                };
                let mut ids = who.split(',');
                let a = get(&mut parts, &mut index, ids.next().unwrap_or(""));
                let b = ids.next().map(|x| get(&mut parts, &mut index, x)).unwrap_or(a);
                evs.push(Ev::Note { a, b, side, text: clean_text(text) });
                continue;
            }
            "box" | "create" | "destroy" | "links" | "link" | "properties" | "details" => continue,
            _ => {}
        }
        // 消息：A->>+B: text
        let (head, text) = match s.split_once(':') {
            Some((h, t)) => (h, clean_text(t)),
            None => (s, String::new()),
        };
        let ops = ["<<-->>", "<<->>", "-->>", "->>", "-->", "--x", "--)", "->", "-x", "-)"];
        let Some((pos, op)) = ops.iter().filter_map(|op| head.find(op).map(|p| (p, *op))).min_by_key(|(p, op)| (*p, std::cmp::Reverse(op.len()))) else {
            return Err(DiagramError::new(line, format!("无法解析：{s}")));
        };
        let from = head[..pos].trim();
        let mut to = head[pos + op.len()..].trim();
        let mut activate = None;
        if let Some(t) = to.strip_prefix('+') {
            activate = Some(true);
            to = t.trim();
        } else if let Some(t) = to.strip_prefix('-') {
            activate = Some(false);
            to = t.trim();
        }
        if from.is_empty() || to.is_empty() {
            return Err(DiagramError::new(line, "消息缺少发送方或接收方"));
        }
        let (dash, end) = arrow_of(op).unwrap();
        let f = get(&mut parts, &mut index, from);
        let t = get(&mut parts, &mut index, to);
        evs.push(Ev::Msg { from: f, to: t, text, dash, end, activate, line });
    }
    if parts.is_empty() {
        return Err(DiagramError::new(0, "时序图至少需要一条消息"));
    }

    // ── 布局 ──
    let fs = theme.font_size;
    let n = parts.len();
    let box_w: Vec<f32> = parts.iter().map(|p| (m.width(&p.label, fs, true) + 30.).max(96.)).collect();
    let mut gap = vec![70f32; n.saturating_sub(1)];
    for ev in &evs {
        if let Ev::Msg { from, to, text, .. } = ev {
            let (a, b) = ((*from).min(*to), (*from).max(*to));
            let w = text_size(m, text, fs - 1., false).0 + 36.;
            if a == b {
                if a < n - 1 {
                    gap[a] = gap[a].max(w + 20.);
                }
            } else {
                let span = (b - a) as f32;
                for g in gap.iter_mut().take(b).skip(a) {
                    *g = g.max(w / span);
                }
            }
        }
        if let Ev::Note { a, side, text, .. } = ev {
            let w = text_size(m, text, fs - 1., false).0 + 30.;
            if *side == 1 && *a < n - 1 {
                gap[*a] = gap[*a].max(w + 20.);
            }
            if *side == -1 && *a > 0 {
                gap[a - 1] = gap[a - 1].max(w + 20.);
            }
        }
    }
    let mut xs = vec![0f32; n];
    let mut x = box_w[0] / 2. + 20.;
    for i in 0..n {
        if i > 0 {
            x += (box_w[i - 1] / 2. + box_w[i] / 2.).max(gap[i - 1] + 20.);
        }
        xs[i] = x;
    }
    let head_h = 42.;
    let top = if title.is_empty() { 10. } else { 46. };
    let mut scene = Scene::default();
    let mut y = top + head_h + 24.;
    let mut lines_out: Vec<Item> = vec![];
    let mut boxes: Vec<Item> = vec![];
    let mut frames: Vec<Item> = vec![];
    let mut active: Vec<Vec<f32>> = vec![vec![]; n];
    let mut acts: Vec<Item> = vec![];
    struct Open {
        kind: String,
        label: String,
        y0: f32,
        min: usize,
        max: usize,
        elses: Vec<(f32, String)>,
    }
    let mut blocks: Vec<Open> = vec![];
    let mut counter = 0;
    let act_x = |active: &Vec<Vec<f32>>, i: usize| -> f32 { active[i].len() as f32 * 5. };
    let close_act = |active: &mut Vec<Vec<f32>>, acts: &mut Vec<Item>, i: usize, y: f32| {
        if let Some(y0) = active[i].pop() {
            let depth = active[i].len() as f32;
            let mut s = ShapeItem::new(ShapeKind::Rect, rect(xs[i] - 5. + depth * 5., y0, 10., (y - y0).max(10.)));
            s.fill = Some(theme.bar_fill.unwrap_or(theme.node_fill));
            s.stroke = Some(theme.bar_fill.unwrap_or(theme.node_stroke));
            s.stroke_w = 1.2;
            acts.push(Item::Shape(s));
        }
    };
    for ev in &evs {
        match ev {
            Ev::Msg { from, to, text, dash, end, activate, line } => {
                let label = if autonumber {
                    counter += 1;
                    if text.is_empty() {
                        format!("{counter}")
                    } else {
                        format!("{counter}. {text}")
                    }
                } else {
                    text.clone()
                };
                let lh = text_size(m, &label, fs - 1., false).1;
                y += lh + 6.;
                for b in blocks.iter_mut() {
                    b.min = b.min.min((*from).min(*to));
                    b.max = b.max.max((*from).max(*to));
                }
                let mut l;
                if from == to {
                    let x0 = xs[*from] + act_x(&active, *from) + 5.;
                    l = LineItem::new(vec![pt(x0, y - 8.), pt(x0 + 44., y - 8.), pt(x0 + 44., y + 14.), pt(x0, y + 14.)]);
                    let mut t = crate::canvas::TextItem {
                        pos: pt(x0 + 52., y + 6.),
                        text: label.clone(),
                        size: fs - 1.,
                        color: theme.text,
                        bold: false,
                        anchor: crate::canvas::Anchor::Start,
                    };
                    t.pos.y -= 6.;
                    lines_out.push(Item::Text(t));
                    y += 24.;
                } else {
                    let dir = if to > from { 1. } else { -1. };
                    let xa = xs[*from] + dir * (5. + act_x(&active, *from) * 0.);
                    let xb = xs[*to] - dir * 5.;
                    l = LineItem::new(vec![pt(xa, y), pt(xb, y)]);
                    let mid = (xa + xb) / 2.;
                    lines_out.push(Item::Text(crate::canvas::TextItem {
                        pos: pt(mid, y - 6. - (lh - fs * 1.38)),
                        text: label.clone(),
                        size: fs - 1.,
                        color: theme.text,
                        bold: false,
                        anchor: crate::canvas::Anchor::Middle,
                    }));
                }
                l.stroke = theme.line;
                l.dash = *dash;
                l.end = *end;
                l.label_bg = Some(theme.label_bg);
                l.seed = 50 + *line as u64;
                lines_out.push(Item::Line(l));
                match activate {
                    Some(true) => active[*to].push(y),
                    Some(false) => close_act(&mut active, &mut acts, *from, y),
                    None => {}
                }
                y += 14.;
            }
            Ev::Activate(i, on) => {
                if *on {
                    active[*i].push(y - 4.);
                } else {
                    close_act(&mut active, &mut acts, *i, y);
                }
            }
            Ev::Note { a, b, side, text } => {
                let (tw, th) = text_size(m, text, fs - 1., false);
                let w = tw + 24.;
                let h = th + 14.;
                y += 6.;
                let r = match side {
                    1 => rect(xs[*a] + 14., y, w, h),
                    -1 => rect(xs[*a] - 14. - w, y, w, h),
                    _ => {
                        let (x0, x1) = (xs[*a].min(xs[*b]), xs[*a].max(xs[*b]));
                        let ww = w.max(x1 - x0 + 60.);
                        rect((x0 + x1) / 2. - ww / 2., y, ww, h)
                    }
                };
                let mut s = ShapeItem::new(ShapeKind::Note, r);
                s.fill = Some(theme.note_fill);
                s.stroke = Some(theme.note_stroke);
                s.text = text.clone();
                s.text_color = theme.text;
                s.font_size = fs - 1.;
                s.seed = 77;
                boxes.push(Item::Shape(s));
                y += h + 12.;
            }
            Ev::BlockStart { kind, label } => {
                y += 10.;
                blocks.push(Open { kind: kind.clone(), label: label.clone(), y0: y, min: usize::MAX, max: 0, elses: vec![] });
                y += fs * 1.9;
            }
            Ev::BlockElse { label } => {
                if let Some(b) = blocks.last_mut() {
                    y += 8.;
                    b.elses.push((y, label.clone()));
                    y += fs * 1.6;
                }
            }
            Ev::BlockEnd { line } => {
                let Some(b) = blocks.pop() else { return Err(DiagramError::new(*line, "多余的 end")) };
                y += 10.;
                let (lo, hi) = if b.min == usize::MAX { (0, n - 1) } else { (b.min, b.max) };
                let pad = 30. + blocks.len() as f32 * 8.;
                let x0 = xs[lo] - pad - if lo == hi { 50. } else { 0. };
                let x1 = xs[hi] + pad + if lo == hi { 50. } else { 0. };
                let r = rect(x0, b.y0, x1 - x0, y - b.y0);
                if b.kind == "rect" {
                    let mut s = ShapeItem::new(ShapeKind::Rect, r);
                    s.fill = Some(Col::parse(&b.label).unwrap_or(theme.cluster_fill).with_alpha(0.5));
                    s.stroke = None;
                    frames.insert(0, Item::Shape(s));
                } else {
                    let mut s = ShapeItem::new(ShapeKind::Frame, r);
                    s.fill = None;
                    s.stroke = Some(theme.muted);
                    s.stroke_w = 1.2;
                    s.text = b.kind.clone();
                    s.text_color = theme.text;
                    s.font_size = fs - 2.;
                    frames.push(Item::Shape(s));
                    if !b.label.is_empty() {
                        frames.push(Item::Text(crate::canvas::TextItem {
                            pos: pt(x0 + crate::canvas::geom::estimate_width(&b.kind, fs - 2., true) + 26., b.y0 + fs * 1.15),
                            text: format!("[{}]", b.label),
                            size: fs - 2.,
                            color: theme.text,
                            bold: true,
                            anchor: crate::canvas::Anchor::Start,
                        }));
                    }
                    for (ey, el) in b.elses {
                        let mut l = LineItem::new(vec![pt(x0, ey), pt(x1, ey)]);
                        l.stroke = theme.muted;
                        l.dash = true;
                        l.end = Arrow::None;
                        l.width = 1.;
                        frames.push(Item::Line(l));
                        if !el.is_empty() {
                            frames.push(Item::Text(crate::canvas::TextItem {
                                pos: pt((x0 + x1) / 2., ey + fs * 1.1),
                                text: format!("[{el}]"),
                                size: fs - 2.,
                                color: theme.text,
                                bold: true,
                                anchor: crate::canvas::Anchor::Middle,
                            }));
                        }
                    }
                }
            }
        }
    }
    if let Some(_b) = blocks.last() {
        return Err(DiagramError::new(body.last().map(|b| b.0).unwrap_or(0), "loop / alt / opt 缺少 end"));
    }
    for i in 0..n {
        while !active[i].is_empty() {
            close_act(&mut active, &mut acts, i, y);
        }
    }
    y += 16.;
    // 参与者（上下各一份）与生命线
    let mut heads = vec![];
    for (i, p) in parts.iter().enumerate() {
        let mut life = LineItem::new(vec![pt(xs[i], top + head_h), pt(xs[i], y)]);
        life.stroke = theme.muted;
        life.dash = true;
        life.width = 1.;
        life.end = Arrow::None;
        heads.push(Item::Line(life));
        for (k, yy) in [top, y].into_iter().enumerate() {
            let r = rect(xs[i] - box_w[i] / 2., yy, box_w[i], head_h);
            if p.actor {
                let mut s = ShapeItem::new(ShapeKind::Actor, rect(xs[i] - 12., yy - 4., 24., head_h - 14.));
                s.stroke = Some(theme.node_stroke);
                s.text = p.label.clone();
                s.text_color = theme.text;
                s.font_size = fs;
                heads.push(Item::Shape(s));
            } else {
                let mut s = ShapeItem::new(ShapeKind::RoundRect, r);
                s.id = if k == 0 { p.id.clone() } else { String::new() };
                s.fill = Some(theme.node_fill);
                s.stroke = Some(theme.node_stroke);
                s.text = p.label.clone();
                s.text_color = theme.text;
                s.font_size = fs;
                s.bold = true;
                s.radius = 6.;
                s.seed = 20 + i as u64 * 2 + k as u64;
                heads.push(Item::Shape(s));
            }
        }
    }
    let _ = Align::Center;
    if !title.is_empty() {
        scene.items.push(Item::Text(crate::canvas::TextItem {
            pos: pt((xs[0] + xs[n - 1]) / 2., 26.),
            text: title,
            size: fs + 3.,
            color: theme.text,
            bold: true,
            anchor: crate::canvas::Anchor::Middle,
        }));
    }
    scene.items.extend(frames);
    scene.items.extend(heads);
    scene.items.extend(acts);
    scene.items.extend(boxes);
    scene.items.extend(lines_out);
    scene.fit(16.);
    Ok(scene)
}
