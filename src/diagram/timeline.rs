//! 时间线：`timeline`。

use super::*;
use crate::canvas::{pt, rect, Anchor, Arrow, Item, LineItem, ShapeItem, ShapeKind, TextItem};

struct Period {
    label: String,
    events: Vec<String>,
    section: usize,
}

pub fn render(body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure) -> DResult<Scene> {
    let mut title = String::new();
    let mut sections: Vec<String> = vec![];
    let mut periods: Vec<Period> = vec![];
    for (line, raw) in body {
        let s = raw.trim();
        if let Some(t) = s.strip_prefix("title") {
            title = t.trim().to_string();
            continue;
        }
        if let Some(t) = s.strip_prefix("section") {
            sections.push(t.trim().to_string());
            continue;
        }
        let mut parts = s.split(':').map(clean_text);
        let first = parts.next().unwrap_or_default();
        if s.starts_with(':') {
            // 接续上一个时期的事件
            let Some(p) = periods.last_mut() else {
                return Err(DiagramError::new(*line, "事件前面缺少时期"));
            };
            p.events.extend(parts.filter(|e| !e.is_empty()));
            continue;
        }
        periods.push(Period { label: first, events: parts.filter(|e| !e.is_empty()).collect(), section: sections.len().saturating_sub(1) });
    }
    if periods.is_empty() {
        return Err(DiagramError::new(0, "时间线至少需要一个时期"));
    }
    let fs = theme.font_size;
    let col_w = periods
        .iter()
        .map(|p| p.events.iter().chain(std::iter::once(&p.label)).map(|e| m.width(e, fs - 1., false)).fold(0., f32::max))
        .fold(0., f32::max)
        .clamp(90., 200.)
        + 24.;
    let top = if title.is_empty() { 10. } else { 52. };
    let sec_h = if sections.is_empty() { 0. } else { 38. };
    let axis_y = top + sec_h + 60.;
    let mut scene = Scene::default();
    // 分区
    if !sections.is_empty() {
        for (si, name) in sections.iter().enumerate() {
            let idx: Vec<usize> = periods.iter().enumerate().filter(|(_, p)| p.section == si).map(|(i, _)| i).collect();
            if let (Some(&a), Some(&b)) = (idx.first(), idx.last()) {
                let r = rect(20. + a as f32 * (col_w + 16.), top, (b - a + 1) as f32 * (col_w + 16.) - 16., 30.);
                let mut s = ShapeItem::new(ShapeKind::RoundRect, r);
                s.fill = Some(theme.palette[si % 8].with_alpha(0.85));
                s.stroke = None;
                s.text = name.clone();
                s.text_color = Col::rgb(0xffffff);
                s.bold = true;
                s.font_size = fs - 1.;
                scene.items.push(Item::Shape(s));
            }
        }
    }
    let total_w = periods.len() as f32 * (col_w + 16.) + 24.;
    let mut axis = LineItem::new(vec![pt(10., axis_y), pt(total_w, axis_y)]);
    axis.stroke = theme.muted;
    axis.width = 2.;
    axis.end = Arrow::Filled;
    scene.items.push(Item::Line(axis));
    let mut max_y = axis_y;
    for (i, p) in periods.iter().enumerate() {
        let x = 20. + i as f32 * (col_w + 16.);
        let col = theme.palette[if sections.is_empty() { i } else { p.section } % 8];
        let mut head = ShapeItem::new(ShapeKind::RoundRect, rect(x, axis_y - 50., col_w, 34.));
        head.fill = Some(col.with_alpha(if theme.dark { 0.3 } else { 0.15 }));
        head.stroke = Some(col);
        head.text = p.label.clone();
        head.text_color = theme.text;
        head.bold = true;
        head.font_size = fs;
        head.seed = 70 + i as u64;
        scene.items.push(Item::Shape(head));
        let mut dot = ShapeItem::new(ShapeKind::Circle, rect(x + col_w / 2. - 6., axis_y - 6., 12., 12.));
        dot.fill = Some(col);
        dot.stroke = Some(theme.label_bg);
        dot.stroke_w = 2.;
        scene.items.push(Item::Shape(dot));
        let mut y = axis_y + 22.;
        for (k, e) in p.events.iter().enumerate() {
            let (_, th) = text_size(m, e, fs - 1., false);
            let h = th + 16.;
            let mut l = LineItem::new(vec![pt(x + col_w / 2., y - 22. + if k == 0 { 6. } else { 0. }), pt(x + col_w / 2., y)]);
            l.stroke = col.with_alpha(0.6);
            l.width = 1.2;
            l.end = Arrow::None;
            l.dash = true;
            scene.items.push(Item::Line(l));
            let mut s = ShapeItem::new(ShapeKind::RoundRect, rect(x, y, col_w, h));
            s.fill = Some(theme.label_bg);
            s.stroke = Some(col.with_alpha(0.7));
            s.text = e.clone();
            s.text_color = theme.text;
            s.font_size = fs - 1.;
            s.seed = 90 + (i * 10 + k) as u64;
            scene.items.push(Item::Shape(s));
            y += h + 14.;
        }
        max_y = max_y.max(y);
    }
    if !title.is_empty() {
        scene.items.push(Item::Text(TextItem {
            pos: pt(total_w / 2., 28.),
            text: title,
            size: fs + 4.,
            color: theme.text,
            bold: true,
            anchor: Anchor::Middle,
        }));
    }
    scene.fit(16.);
    Ok(scene)
}
