//! 饼图：`pie [showData] [title xxx]`。

use super::*;
use crate::canvas::{pt, rect, Anchor, Item, PathCmd, PathItem, ShapeItem, ShapeKind, TextItem};

pub fn render(header: &str, body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure) -> DResult<Scene> {
    let mut title = String::new();
    let mut show_data = header.contains("showData");
    if let Some(p) = header.find("title") {
        title = header[p + 5..].trim().to_string();
    }
    let mut data: Vec<(String, f64)> = vec![];
    for (line, raw) in body {
        let s = raw.trim();
        if let Some(t) = s.strip_prefix("title") {
            title = t.trim().to_string();
            continue;
        }
        if s == "showData" {
            show_data = true;
            continue;
        }
        let Some((k, v)) = s.rsplit_once(':') else {
            return Err(DiagramError::new(*line, "饼图数据应为 \"名称\" : 数值"));
        };
        let v: f64 = v.trim().parse().map_err(|_| DiagramError::new(*line, format!("不是数字：{}", v.trim())))?;
        if v < 0. {
            return Err(DiagramError::new(*line, "数值不能为负"));
        }
        data.push((clean_text(k), v));
    }
    let total: f64 = data.iter().map(|d| d.1).sum();
    if data.is_empty() || total <= 0. {
        return Err(DiagramError::new(0, "饼图至少需要一个正数"));
    }
    let r = 130.;
    let top = if title.is_empty() { 10. } else { 48. };
    let (cx, cy) = (r + 20., top + r + 10.);
    let mut scene = Scene::default();
    let mut a0 = -std::f64::consts::FRAC_PI_2;
    for (i, (label, v)) in data.iter().enumerate() {
        let frac = v / total;
        let a1 = a0 + frac * std::f64::consts::TAU;
        let mut cmds = vec![PathCmd::Move(pt(cx, cy))];
        let steps = ((frac * 72.).ceil() as usize).max(2);
        for k in 0..=steps {
            let a = a0 + (a1 - a0) * k as f64 / steps as f64;
            cmds.push(PathCmd::Line(pt(cx + r * a.cos() as f32, cy + r * a.sin() as f32)));
        }
        cmds.push(PathCmd::Close);
        let col = theme.palette[i % 8];
        scene.items.push(Item::Path(PathItem { cmds, fill: Some(col.with_alpha(0.88)), stroke: Some(theme.label_bg), width: 2., dash: false }));
        if frac > 0.04 {
            let am = (a0 + a1) / 2.;
            let lr = r * 0.66;
            scene.items.push(Item::Text(TextItem {
                pos: pt(cx + lr * am.cos() as f32, cy + lr * am.sin() as f32 + 5.),
                text: format!("{:.0}%", frac * 100.),
                size: theme.font_size - 1.,
                color: Col::rgb(0xffffff),
                bold: true,
                anchor: Anchor::Middle,
            }));
        }
        // 图例
        let ly = top + 20. + i as f32 * 26.;
        let lx = cx + r + 40.;
        let mut sw = ShapeItem::new(ShapeKind::RoundRect, rect(lx, ly - 7., 14., 14.));
        sw.radius = 3.;
        sw.fill = Some(col);
        sw.stroke = None;
        scene.items.push(Item::Shape(sw));
        let text = if show_data { format!("{label}  [{v}]") } else { label.clone() };
        let _ = m.width(&text, theme.font_size, false);
        scene.items.push(Item::Text(TextItem {
            pos: pt(lx + 22., ly + 5.),
            text,
            size: theme.font_size,
            color: theme.text,
            bold: false,
            anchor: Anchor::Start,
        }));
        a0 = a1;
    }
    if !title.is_empty() {
        scene.items.push(Item::Text(TextItem {
            pos: pt(cx, 30.),
            text: title,
            size: theme.font_size + 3.,
            color: theme.text,
            bold: true,
            anchor: Anchor::Middle,
        }));
    }
    scene.fit(16.);
    Ok(scene)
}
