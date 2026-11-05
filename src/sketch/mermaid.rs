//! Mermaid 至草图（对应官网「Mermaid 至 Excalidraw」）。
//!
//! 流程图（flowchart / graph）转成可编辑的草图元素：节点 → 矩形 / 圆角矩形 / 菱形 / 椭圆 + 容器文字，
//! 连线 → 两端绑定的箭头（带标签），子图 → 框架。解析与 Sugiyama 布局复用 `crate::diagram`。
//! 其他图（时序图、类图……）和官网一样，渲染成一张图片插入。

use super::geom;
use super::model::*;
use crate::canvas::{Arrow, EstimateMeasure, Item, Measure, ShapeKind};

/// 用草图手写体量字宽，节点大小与最终显示一致
struct HandMeasure;

impl Measure for HandMeasure {
    fn width(&mut self, text: &str, size: f32, _bold: bool) -> f32 {
        geom::text_width(text, size as f64, FONT_HAND) as f32
    }
}

const NODE_FONT: f64 = 20.;
const EDGE_FONT: f64 = 16.;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

pub enum Converted {
    Doc(SketchDoc),
    /// 非流程图：PNG 字节
    Png(Vec<u8>),
}

fn head_of(a: Arrow) -> Value {
    match a {
        Arrow::None => Value::Null,
        Arrow::Open => json!("arrow"),
        Arrow::Hollow => json!("triangle_outline"),
        Arrow::Circle => json!("circle"),
        Arrow::Cross => json!("bar"),
        Arrow::DiamondFilled => json!("diamond"),
        Arrow::DiamondHollow => json!("diamond_outline"),
        _ => json!("arrow"),
    }
}

pub fn convert(src: &str) -> Result<Converted, String> {
    let (lines, _) = crate::diagram::preprocess(src);
    let Some(first) = lines.first() else { return Err("请输入 Mermaid 代码，例如 flowchart LR".into()) };
    let head = first.1.trim().to_lowercase();
    if !(head.starts_with("flowchart") || head.starts_with("graph")) {
        // 其他类型：渲染成图片
        let scene = crate::diagram::render(src, false, None, &mut EstimateMeasure, &BTreeMap::new()).map_err(|e| format!("第 {} 行：{}", e.line, e.msg))?;
        let svg = crate::canvas::svg::to_svg(&scene);
        let png = crate::canvas::svg::svg_to_png(&svg, 2.).ok_or("渲染图片失败")?;
        return Ok(Converted::Png(png));
    }
    let mut g = crate::diagram::flowchart::parse(&first.1, &lines[1..]).map_err(|e| format!("第 {} 行：{}", e.line, e.msg))?;
    g.curved = false;
    let mut theme = crate::diagram::Theme::default_light();
    theme.font_size = NODE_FONT as f32;
    let scene = g.to_scene(&theme, &mut HandMeasure, &BTreeMap::new());
    let mut d = SketchDoc::new();
    // 子图 → 框架（先放，层级在底）
    for item in &scene.items {
        if let Item::Shape(s) = item {
            if s.id.is_empty() && !s.text.is_empty() {
                let mut f = Element::new("frame", s.rect.x as f64, s.rect.y as f64);
                f.width = s.rect.w as f64;
                f.height = s.rect.h as f64;
                f.set("name", json!(s.text));
                d.elements.push(f);
            }
        }
    }
    let mut ids: HashMap<String, String> = HashMap::new();
    for item in &scene.items {
        let Item::Shape(s) = item else { continue };
        if s.id.is_empty() {
            continue;
        }
        let kind = match s.kind {
            ShapeKind::Diamond | ShapeKind::Hexagon => "diamond",
            ShapeKind::Circle | ShapeKind::DoubleCircle | ShapeKind::Ellipse => "ellipse",
            _ => "rectangle",
        };
        let mut e = Element::new(kind, s.rect.x as f64, s.rect.y as f64);
        e.width = s.rect.w as f64;
        e.height = s.rect.h as f64;
        // 菱形 / 椭圆的可写区域比外框小：按文字放大外框（中心不变），免得字被挤成多行
        let longest = s.text.split('\n').map(|l| geom::text_width(l, NODE_FONT, FONT_HAND)).fold(0., f64::max);
        let need = match kind {
            "diamond" => (longest + BOUND_TEXT_PADDING * 2.) * 2. + 8.,
            "ellipse" => (longest + BOUND_TEXT_PADDING * 2.) * std::f64::consts::SQRT_2 + 8.,
            _ => longest + BOUND_TEXT_PADDING * 2. + 16.,
        };
        if need > e.width {
            e.x -= (need - e.width) / 2.;
            e.width = need;
        }
        if kind == "rectangle" && matches!(s.kind, ShapeKind::Rect) {
            e.set("roundness", Value::Null);
        }
        if let Some(f) = g.index.get(&s.id).and_then(|&i| g.nodes[i].style.fill) {
            e.background_color = f.hex();
        }
        // 节点在哪个子图（框架）里
        if let Some(fr) = d.elements.iter().rev().find(|f| f.is_frame() && geom::bounds(f).contains_r(&geom::bounds(&e))) {
            let fid = fr.id.clone();
            e.set("frameId", json!(fid));
        }
        let eid = e.id.clone();
        let fid = e.extra.get("frameId").cloned().unwrap_or(Value::Null);
        d.elements.push(e);
        if !s.text.trim().is_empty() {
            add_label(&mut d, &eid, &s.text, NODE_FONT, fid);
        }
        ids.insert(s.id.clone(), eid);
    }
    let routes: HashMap<String, Vec<crate::canvas::Pt>> =
        scene.items.iter().filter_map(|i| if let Item::Line(l) = i { Some((l.id.clone(), l.points.clone())) } else { None }).collect();
    for (k, ge) in g.edges.iter().enumerate() {
        if ge.invisible {
            continue;
        }
        let (Some(a), Some(b)) = (ids.get(&g.nodes[ge.from].id).cloned(), ids.get(&g.nodes[ge.to].id).cloned()) else { continue };
        let pts: Vec<geom::V> = match routes.get(&format!("edge:{k}")) {
            Some(p) if p.len() >= 2 => p.iter().map(|p| geom::v(p.x as f64, p.y as f64)).collect(),
            _ => {
                let (ea, eb) = (d.get(&a).unwrap(), d.get(&b).unwrap());
                vec![geom::center(ea), geom::center(eb)]
            }
        };
        let mut arr = Element::new("arrow", pts[0].x, pts[0].y);
        arr.points = Some(pts.iter().map(|p| [p.x - pts[0].x, p.y - pts[0].y]).collect());
        arr.normalize_points();
        arr.set("roundness", if pts.len() > 2 { json!({ "type": 2 }) } else { Value::Null });
        arr.set("startArrowhead", head_of(ge.start));
        arr.set("endArrowhead", head_of(ge.end));
        if ge.dash || ge.dotted {
            arr.stroke_style = if ge.dotted { "dotted".into() } else { "dashed".into() };
        }
        if ge.thick {
            arr.stroke_width = 4.;
        }
        arr.set_binding(true, Some(&a));
        arr.set_binding(false, Some(&b));
        let aid = arr.id.clone();
        d.elements.push(arr);
        for t in [&a, &b] {
            if let Some(te) = d.get_mut(t) {
                te.add_bound(&aid, "arrow");
            }
        }
        if !ge.label.trim().is_empty() {
            add_label(&mut d, &aid, &ge.label, EDGE_FONT, Value::Null);
        }
    }
    geom::update_bound_arrows(&mut d, &[]);
    Ok(Converted::Doc(d))
}

fn add_label(d: &mut SketchDoc, container: &str, text: &str, size: f64, frame: Value) {
    let Some(c) = d.get(container).cloned() else { return };
    let mut t = Element::new("text", c.x, c.y);
    let text = text.replace("<br>", "\n").replace("<br/>", "\n");
    t.text = Some(text.clone());
    t.set("originalText", json!(text));
    t.font_size = Some(size);
    t.set("textAlign", json!("center"));
    t.set("verticalAlign", json!("middle"));
    t.set("containerId", json!(container));
    t.set("frameId", frame);
    let tid = t.id.clone();
    d.elements.push(t);
    if let Some(ce) = d.get_mut(container) {
        ce.add_bound(&tid, "text");
    }
    geom::layout_bound_text(d, container);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowchart_becomes_bound_elements() {
        let Ok(Converted::Doc(d)) = convert("flowchart TD\n  A[开始] --> B{判断}\n  B -->|是| C((完成))\n  B -.->|否| A") else { panic!() };
        assert_eq!(d.live().filter(|e| e.kind == "rectangle").count(), 1);
        assert_eq!(d.live().filter(|e| e.kind == "diamond").count(), 1);
        assert_eq!(d.live().filter(|e| e.kind == "ellipse").count(), 1);
        let arrows: Vec<_> = d.live().filter(|e| e.kind == "arrow").collect();
        assert_eq!(arrows.len(), 3);
        assert!(arrows.iter().all(|a| a.binding(true).is_some() && a.binding(false).is_some()));
        assert!(arrows.iter().any(|a| a.stroke_style == "dashed"));
        // 节点文字 3 个 + 连线标签 2 个
        assert_eq!(d.live().filter(|e| e.is_text()).count(), 5);
        assert!(d.live().filter(|e| e.is_text()).all(|t| t.container_id().is_some()));
    }

    #[test]
    fn other_diagrams_render_as_image() {
        let r = convert("sequenceDiagram\n  A->>B: hi");
        assert!(matches!(r, Ok(Converted::Png(p)) if p.len() > 100));
        assert!(convert("").is_err());
    }
}
