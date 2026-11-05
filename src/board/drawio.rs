//! draw.io（diagrams.net）导入：`.drawio` / `.xml` → 可编辑画板（每页一个）。
//!
//! 不复刻 mxGraph 编辑器，只把常见元素映射到画板：
//! - 顶点：矩形 / 圆角矩形 / 椭圆 / 菱形 / 圆柱 / 文档 / 六边形 / 平行四边形 / 云 / 便签 / 人形等常用形状，
//!   文本节点 → 文本，泳道（swimlane）/ 容器 → 分区；不认识的形状一律降级成带文字的矩形，并计数提示；
//! - 连线：绑定到两端元素（exitX/exitY、entryX/entryY 作为锚点），直角 / 曲线走线、虚线、箭头、拐点；
//!   挂在连线上的文字标签并入连线说明；
//! - 样式：填充、描边、文字颜色、字号、粗体、斜体、虚线、线宽、对齐、透明度。
//!
//! 文件格式参考 jgraph/drawio：`<mxfile><diagram>` 里要么直接是 `<mxGraphModel>`，
//! 要么是压缩串（base64 → raw deflate → URL 编码的 XML）。子元素的坐标相对父元素（组 / 泳道），这里换算成绝对坐标。

use super::model::{BoardDoc, ElKind, End, Route};
use crate::canvas::{pt, rect, Arrow, Col, ShapeKind};
use std::collections::HashMap;

/// 一页导入结果。
pub struct Page {
    pub name: String,
    pub doc: BoardDoc,
    /// 降级成矩形的形状数
    pub simplified: usize,
}

/// 是不是 draw.io 文件内容。
pub fn is_drawio(text: &str) -> bool {
    let head = &text[..text.len().min(4096)];
    head.contains("<mxfile") || head.contains("<mxGraphModel")
}

/// 解析整个文件（可能多页）。
pub fn import(text: &str, dark: bool) -> Result<Vec<Page>, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("不是有效的 XML：{e}"))?;
    let root = doc.root_element();
    let mut pages = vec![];
    match root.tag_name().name() {
        "mxGraphModel" => pages.push(("第 1 页".to_string(), root.document().input_text()[root.range()].to_string())),
        "mxfile" => {
            for (i, d) in root.children().filter(|n| n.has_tag_name("diagram")).enumerate() {
                let name = d.attribute("name").map(str::to_string).unwrap_or_else(|| format!("第 {} 页", i + 1));
                let xml = match d.children().find(|n| n.has_tag_name("mxGraphModel")) {
                    Some(m) => text[m.range()].to_string(),
                    None => {
                        let raw = d.text().unwrap_or("").trim();
                        if raw.is_empty() {
                            continue;
                        }
                        inflate(raw).ok_or_else(|| format!("第 {} 页解压失败", i + 1))?
                    }
                };
                pages.push((name, xml));
            }
        }
        other => return Err(format!("不是 draw.io 文件（根元素是 <{other}>）")),
    }
    if pages.is_empty() {
        return Err("文件里没有图".into());
    }
    pages.into_iter().map(|(name, xml)| convert(&xml, dark).map(|(doc, simplified)| Page { name, doc, simplified })).collect()
}

/// 压缩页：base64 → raw deflate → URL 解码。
fn inflate(s: &str) -> Option<String> {
    use base64::Engine;
    use std::io::Read;
    let bytes = base64::engine::general_purpose::STANDARD.decode(s.split_whitespace().collect::<String>()).ok()?;
    let mut out = String::new();
    flate2::read::DeflateDecoder::new(&bytes[..]).read_to_string(&mut out).ok()?;
    Some(url_decode(&out))
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `key=value;key=value;shapeName;` → (第一个不带 = 的词, 键值表)
fn parse_style(s: &str) -> (String, HashMap<String, String>) {
    let mut first = String::new();
    let mut map = HashMap::new();
    for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('=') {
            Some((k, v)) => {
                map.insert(k.to_string(), v.to_string());
            }
            None if first.is_empty() => first = part.to_string(),
            None => {
                map.insert(part.to_string(), "1".into());
            }
        }
    }
    (first, map)
}

fn color(v: Option<&String>) -> Option<Option<Col>> {
    let v = v?.trim();
    if v == "none" || v.is_empty() {
        return Some(None);
    }
    let hex = v.strip_prefix('#')?;
    let hex = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect::<String>() } else { hex.to_string() };
    u32::from_str_radix(&hex[..6.min(hex.len())], 16).ok().map(|c| Some(Col::rgb(c)))
}

/// 标签文字：html=1 时去掉标签、换行标签转换行、解实体。
fn label(v: &str, html: bool) -> String {
    if !html {
        return v.to_string();
    }
    let mut s = v.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n").replace("</div>", "\n").replace("</p>", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.drain(..) {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&");
    out.trim_end_matches('\n').trim().to_string()
}

/// draw.io 形状 → 画板形状。None 表示不认识（降级为矩形）。
fn shape_of(first: &str, st: &HashMap<String, String>) -> Option<ShapeKind> {
    let name = st.get("shape").map(String::as_str).unwrap_or(first);
    Some(match name {
        "" | "rect" | "rectangle" | "label" => {
            if st.get("rounded").is_some_and(|v| v == "1") {
                ShapeKind::RoundRect
            } else {
                ShapeKind::Rect
            }
        }
        "ellipse" => {
            if st.get("aspect").is_some_and(|v| v == "fixed") {
                ShapeKind::Circle
            } else {
                ShapeKind::Ellipse
            }
        }
        "doubleEllipse" => ShapeKind::DoubleCircle,
        "rhombus" => ShapeKind::Diamond,
        "hexagon" => ShapeKind::Hexagon,
        "parallelogram" => ShapeKind::Parallelogram,
        "trapezoid" => ShapeKind::Trapezoid,
        "triangle" => ShapeKind::Triangle,
        "cylinder" | "cylinder2" | "cylinder3" | "datastore" => ShapeKind::Cylinder,
        "document" => ShapeKind::Document,
        "process" => ShapeKind::Subroutine,
        "note" | "note2" => ShapeKind::Note,
        "cloud" => ShapeKind::Cloud,
        "step" => ShapeKind::Chevron,
        "cube" => ShapeKind::Cube,
        "umlActor" => ShapeKind::Actor,
        "card" => ShapeKind::Card,
        "tape" => ShapeKind::Tape,
        "delay" => ShapeKind::Delay,
        "offPageConnector" => ShapeKind::OffPage,
        "manualInput" => ShapeKind::ManualInput,
        "display" => ShapeKind::Display,
        "internalStorage" => ShapeKind::InternalStorage,
        "singleArrow" => ShapeKind::BlockArrow,
        "doubleArrow" => ShapeKind::DoubleArrow,
        "callout" => ShapeKind::Bubble,
        "cross" => ShapeKind::Cross,
        "mxgraph.flowchart.start_1" | "mxgraph.flowchart.terminator" => ShapeKind::Stadium,
        "mxgraph.flowchart.decision" => ShapeKind::Diamond,
        "mxgraph.flowchart.process" => ShapeKind::Rect,
        "mxgraph.flowchart.document" => ShapeKind::Document,
        "mxgraph.flowchart.database" => ShapeKind::Cylinder,
        _ => return None,
    })
}

fn arrow(v: Option<&String>, fill: Option<&String>, default: Arrow) -> Arrow {
    let hollow = fill.is_some_and(|f| f == "0");
    match v.map(String::as_str) {
        None => default,
        Some("none") | Some("") => Arrow::None,
        Some("open") | Some("openThin") => Arrow::Open,
        Some("oval") | Some("circle") => {
            if hollow {
                Arrow::Hollow
            } else {
                Arrow::Circle
            }
        }
        Some("diamond") | Some("diamondThin") => {
            if hollow {
                Arrow::DiamondHollow
            } else {
                Arrow::DiamondFilled
            }
        }
        Some("cross") => Arrow::Cross,
        Some("ERone") | Some("ERmandOne") => Arrow::One,
        Some("ERzeroToOne") => Arrow::ZeroOrOne,
        Some("ERoneToMany") => Arrow::OneOrMany,
        Some("ERzeroToMany") | Some("ERmany") => Arrow::ZeroOrMany,
        Some(_) => {
            if hollow {
                Arrow::Hollow
            } else {
                Arrow::Filled
            }
        }
    }
}

struct Cell {
    id: String,
    parent: String,
    value: String,
    style: String,
    vertex: bool,
    edge: bool,
    source: Option<String>,
    target: Option<String>,
    /// 几何：x, y, w, h（相对父元素）
    geo: (f32, f32, f32, f32),
    relative: bool,
    points: Vec<(f32, f32)>,
    src_pt: Option<(f32, f32)>,
    tgt_pt: Option<(f32, f32)>,
}

fn num(n: roxmltree::Node, k: &str) -> f32 {
    n.attribute(k).and_then(|v| v.parse().ok()).unwrap_or(0.)
}

fn read_cells(model: &roxmltree::Document) -> Vec<Cell> {
    let mut out = vec![];
    for n in model.descendants().filter(|n| n.has_tag_name("mxCell")) {
        // UserObject / object 包装：id 与标签在外层
        let wrap = n.parent().filter(|p| p.has_tag_name("UserObject") || p.has_tag_name("object"));
        let id = wrap.and_then(|w| w.attribute("id")).or(n.attribute("id")).unwrap_or("").to_string();
        let value = wrap.and_then(|w| w.attribute("label")).or(n.attribute("value")).unwrap_or("").to_string();
        let geo_node = n.children().find(|c| c.has_tag_name("mxGeometry"));
        let mut cell = Cell {
            id,
            parent: n.attribute("parent").unwrap_or("").to_string(),
            value,
            style: n.attribute("style").unwrap_or("").to_string(),
            vertex: n.attribute("vertex") == Some("1"),
            edge: n.attribute("edge") == Some("1"),
            source: n.attribute("source").map(str::to_string),
            target: n.attribute("target").map(str::to_string),
            geo: (0., 0., 0., 0.),
            relative: false,
            points: vec![],
            src_pt: None,
            tgt_pt: None,
        };
        if let Some(g) = geo_node {
            cell.geo = (num(g, "x"), num(g, "y"), num(g, "width"), num(g, "height"));
            cell.relative = g.attribute("relative") == Some("1");
            for c in g.children() {
                if c.has_tag_name("Array") && c.attribute("as") == Some("points") {
                    cell.points = c.children().filter(|p| p.has_tag_name("mxPoint")).map(|p| (num(p, "x"), num(p, "y"))).collect();
                } else if c.has_tag_name("mxPoint") {
                    match c.attribute("as") {
                        Some("sourcePoint") => cell.src_pt = Some((num(c, "x"), num(c, "y"))),
                        Some("targetPoint") => cell.tgt_pt = Some((num(c, "x"), num(c, "y"))),
                        _ => {}
                    }
                }
            }
        }
        out.push(cell);
    }
    out
}

fn convert(xml: &str, dark: bool) -> Result<(BoardDoc, usize), String> {
    let model = roxmltree::Document::parse(xml).map_err(|e| format!("图数据解析失败：{e}"))?;
    let cells = read_cells(&model);
    let by_id: HashMap<&str, &Cell> = cells.iter().map(|c| (c.id.as_str(), c)).collect();
    // 绝对坐标：沿父链累加顶点的 x / y
    let offset = |c: &Cell| -> (f32, f32) {
        let (mut x, mut y) = (0., 0.);
        let mut p = c.parent.as_str();
        for _ in 0..64 {
            match by_id.get(p) {
                Some(pc) if pc.vertex => {
                    x += pc.geo.0;
                    y += pc.geo.1;
                    p = pc.parent.as_str();
                }
                _ => break,
            }
        }
        (x, y)
    };
    let mut doc = BoardDoc::new(false);
    let mut ids: HashMap<String, u64> = HashMap::new();
    let mut simplified = 0;
    let mut edge_labels: HashMap<String, Vec<String>> = HashMap::new();
    // 先放容器（泳道 / 组），再放普通顶点，最后连线：画板按数组顺序绘制，容器要在下面
    let mut verts: Vec<&Cell> = cells.iter().filter(|c| c.vertex).collect();
    verts.sort_by_key(|c| !parse_style(&c.style).0.starts_with("swimlane") && !parse_style(&c.style).1.contains_key("container"));
    for c in verts {
        let (first, st) = parse_style(&c.style);
        let html = st.get("html").is_some_and(|v| v == "1");
        let text = label(&c.value, html);
        // 挂在连线上的标签
        if by_id.get(c.parent.as_str()).is_some_and(|p| p.edge) {
            if !text.is_empty() {
                edge_labels.entry(c.parent.clone()).or_default().push(text);
            }
            continue;
        }
        if first == "group" && text.is_empty() {
            continue;
        }
        let (ox, oy) = offset(c);
        let r = rect(c.geo.0 + ox, c.geo.1 + oy, c.geo.2.max(1.), c.geo.3.max(1.));
        let (kind, unknown) = if first == "text" || st.contains_key("text") {
            (ElKind::Text, false)
        } else if first.starts_with("swimlane") || st.contains_key("swimlane") {
            (ElKind::Section, false)
        } else if first == "line" {
            (ElKind::Shape(ShapeKind::Rect), false)
        } else if let Some(s) = shape_of(&first, &st) {
            (ElKind::Shape(s), false)
        } else {
            // 图片与不认识的形状：带文字的矩形
            (ElKind::Shape(ShapeKind::Rect), true)
        };
        if unknown {
            simplified += 1;
        }
        let mut el = doc.new_el(kind, r, dark);
        el.text = text;
        if let Some(f) = color(st.get("fillColor")) {
            el.fill = f;
        }
        match color(st.get("strokeColor")) {
            Some(Some(s)) => el.stroke = s,
            Some(None) => el.no_stroke = true,
            None => {}
        }
        if let Some(Some(fc)) = color(st.get("fontColor")) {
            el.text_color = Some(fc);
        }
        if let Some(fs) = st.get("fontSize").and_then(|v| v.parse::<f32>().ok()) {
            el.font_size = fs.clamp(8., 96.);
        }
        let fstyle: u8 = st.get("fontStyle").and_then(|v| v.parse().ok()).unwrap_or(0);
        el.bold = fstyle & 1 != 0;
        el.deco = if fstyle & 2 != 0 { crate::canvas::DECO_ITALIC } else { 0 } | if fstyle & 4 != 0 { crate::canvas::DECO_UNDERLINE } else { 0 };
        el.dash = st.get("dashed").is_some_and(|v| v == "1");
        if let Some(w) = st.get("strokeWidth").and_then(|v| v.parse::<f32>().ok()) {
            el.stroke_w = w.clamp(0.5, 12.);
        }
        if let Some(o) = st.get("opacity").and_then(|v| v.parse::<f32>().ok()) {
            el.opacity = (o / 100.).clamp(0.05, 1.);
        }
        el.text_align = match st.get("align").map(String::as_str) {
            Some("left") => 1,
            Some("center") => 2,
            Some("right") => 3,
            _ => 0,
        };
        el.v_align = match st.get("verticalAlign").map(String::as_str) {
            Some("top") => 1,
            Some("middle") => 2,
            Some("bottom") => 3,
            _ => 0,
        };
        ids.insert(c.id.clone(), el.id);
        doc.els.push(el);
    }
    // 连线
    let mut vias: Vec<(u64, Vec<crate::canvas::Pt>)> = vec![];
    for c in cells.iter().filter(|c| c.edge) {
        let (_, st) = parse_style(&c.style);
        let (ox, oy) = offset(c);
        let end = |id: &Option<String>, pt_: Option<(f32, f32)>, kx: &str, ky: &str| -> End {
            match id.as_ref().and_then(|i| ids.get(i)) {
                Some(&el) => {
                    let ax = st.get(kx).and_then(|v| v.parse().ok()).unwrap_or(0.5f32).clamp(0., 1.);
                    let ay = st.get(ky).and_then(|v| v.parse().ok()).unwrap_or(0.5f32).clamp(0., 1.);
                    End::Bound { el, ax, ay }
                }
                None => {
                    let (x, y) = pt_.unwrap_or((c.geo.0, c.geo.1));
                    End::Free { x: x + ox, y: y + oy }
                }
            }
        };
        let a = end(&c.source, c.src_pt, "exitX", "exitY");
        let b = end(&c.target, c.tgt_pt, "entryX", "entryY");
        let es = st.get("edgeStyle").map(String::as_str).unwrap_or("");
        let route = if st.get("curved").is_some_and(|v| v == "1") {
            Route::Curve
        } else if es.contains("orthogonal") || es.contains("elbow") || es.contains("entityRelation") {
            if st.get("rounded").is_some_and(|v| v == "1") {
                Route::RoundElbow
            } else {
                Route::Elbow
            }
        } else {
            Route::Straight
        };
        let start = arrow(st.get("startArrow"), st.get("startFill"), Arrow::None);
        let endw = arrow(st.get("endArrow"), st.get("endFill"), Arrow::Filled);
        let mut el = doc.new_el(ElKind::Connector { a, b, route, start, end: endw }, rect(0., 0., 0., 0.), dark);
        let html = st.get("html").is_some_and(|v| v == "1");
        let mut text = label(&c.value, html);
        if let Some(extra) = edge_labels.remove(&c.id) {
            for t in extra {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&t);
            }
        }
        el.text = text;
        if let Some(Some(s)) = color(st.get("strokeColor")) {
            el.stroke = s;
        }
        if let Some(Some(fc)) = color(st.get("fontColor")) {
            el.text_color = Some(fc);
        }
        el.dash = st.get("dashed").is_some_and(|v| v == "1");
        if let Some(w) = st.get("strokeWidth").and_then(|v| v.parse::<f32>().ok()) {
            el.stroke_w = w.clamp(0.5, 12.);
        }
        if !c.points.is_empty() {
            vias.push((el.id, c.points.iter().map(|(x, y)| pt(x + ox, y + oy)).collect()));
        }
        doc.els.push(el);
    }
    for (id, v) in vias {
        doc.set_via(id, &v);
    }
    if doc.els.is_empty() {
        return Err("这一页是空的".into());
    }
    Ok((doc, simplified))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = r##"<mxfile><diagram name="流程" id="a"><mxGraphModel><root>
<mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="s" value="开始" style="ellipse;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="40" y="40" width="120" height="60" as="geometry"/></mxCell>
<mxCell id="d" value="是否&lt;br&gt;通过？" style="rhombus;whiteSpace=wrap;html=1;fontStyle=1;fontSize=14" vertex="1" parent="1"><mxGeometry x="40" y="160" width="120" height="80" as="geometry"/></mxCell>
<mxCell id="x" value="奇怪形状" style="shape=mxgraph.aws4.lambda;" vertex="1" parent="1"><mxGeometry x="240" y="160" width="60" height="60" as="geometry"/></mxCell>
<mxCell id="lane" value="泳道" style="swimlane;" vertex="1" parent="1"><mxGeometry x="400" y="0" width="200" height="200" as="geometry"/></mxCell>
<mxCell id="in" value="里面" style="rounded=1;" vertex="1" parent="lane"><mxGeometry x="20" y="40" width="80" height="40" as="geometry"/></mxCell>
<mxCell id="e1" value="" style="edgeStyle=orthogonalEdgeStyle;rounded=0;dashed=1;endArrow=open;" edge="1" parent="1" source="s" target="d"><mxGeometry relative="1" as="geometry"><Array as="points"><mxPoint x="100" y="130"/></Array></mxGeometry></mxCell>
<mxCell id="lbl" value="是" style="edgeLabel;html=1;" vertex="1" connectable="0" parent="e1"><mxGeometry x="-0.2" relative="1" as="geometry"/></mxCell>
<mxCell id="e2" style="endArrow=none;" edge="1" parent="1" source="d"><mxGeometry relative="1" as="geometry"><mxPoint x="300" y="300" as="targetPoint"/></mxGeometry></mxCell>
</root></mxGraphModel></diagram></mxfile>"##;

    #[test]
    fn maps_shapes_edges_and_labels() {
        let pages = import(PLAIN, false).unwrap();
        assert_eq!(pages.len(), 1);
        let p = &pages[0];
        assert_eq!(p.name, "流程");
        assert_eq!(p.simplified, 1);
        let els = &p.doc.els;
        let find = |t: &str| els.iter().find(|e| e.text == t).unwrap_or_else(|| panic!("找不到 {t}"));
        assert_eq!(find("开始").kind, ElKind::Shape(ShapeKind::Ellipse));
        assert_eq!(find("开始").fill, Some(Col::rgb(0xdae8fc)));
        let d = find("是否\n通过？");
        assert_eq!(d.kind, ElKind::Shape(ShapeKind::Diamond));
        assert!(d.bold);
        assert_eq!(d.font_size, 14.);
        assert!(matches!(find("泳道").kind, ElKind::Section));
        // 泳道里的子元素换算成绝对坐标
        let inner = find("里面");
        assert_eq!((inner.x, inner.y), (420., 40.));
        assert_eq!(inner.kind, ElKind::Shape(ShapeKind::RoundRect));
        let edges: Vec<_> = els.iter().filter(|e| e.is_connector()).collect();
        assert_eq!(edges.len(), 2);
        let e1 = edges.iter().find(|e| e.text == "是").expect("连线标签并入连线");
        assert!(e1.dash);
        assert_eq!(e1.via, vec![[100., 130.]]);
        match &e1.kind {
            ElKind::Connector { a: End::Bound { .. }, b: End::Bound { .. }, route: Route::Elbow, end: Arrow::Open, .. } => {}
            k => panic!("连线不对：{k:?}"),
        }
        let e2 = edges.iter().find(|e| e.text.is_empty()).unwrap();
        assert!(matches!(&e2.kind, ElKind::Connector { b: End::Free { x, y }, end: Arrow::None, .. } if *x == 300. && *y == 300.));
        // 泳道在最下层
        assert!(els.iter().position(|e| e.text == "泳道").unwrap() < els.iter().position(|e| e.text == "里面").unwrap());
    }

    #[test]
    fn compressed_pages() {
        use base64::Engine;
        use std::io::Write;
        let inner = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="a" value="压缩页" style="" vertex="1" parent="1"><mxGeometry x="0" y="0" width="80" height="40" as="geometry"/></mxCell></root></mxGraphModel>"#;
        // draw.io：encodeURIComponent → raw deflate → base64
        let enc: String =
            inner.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(enc.as_bytes()).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(z.finish().unwrap());
        let file = format!(
            r#"<mxfile><diagram name="一">{b64}</diagram><diagram name="二"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="b" value="第二页" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell></root></mxGraphModel></diagram></mxfile>"#
        );
        let pages = import(&file, true).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].doc.els[0].text, "压缩页");
        assert_eq!(pages[1].name, "二");
        assert!(is_drawio(&file));
        assert!(import("<root/>", false).is_err());
    }
}
