//! 草图数据模型：与 Excalidraw 官方 `.excalidraw` 结构一一对应。
//!
//! 对象行里存的就是官方 JSON（`{ type, version, source, elements, appState, files }`），
//! 常用字段解析成强类型，其余字段（包括我们不认识的新字段）放在 `extra` 里原样保留，读写往返不丢数据。
//! 字段语义参考 excalidraw/excalidraw `packages/element/src/types.ts`（MIT）。

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const DEFAULT_STROKE: &str = "#1e1e1e";
pub const TRANSPARENT: &str = "transparent";
/// 容器内文字与边框的间距（官方 BOUND_TEXT_PADDING）
pub const BOUND_TEXT_PADDING: f64 = 5.;
pub const DEFAULT_FONT_SIZE: f64 = 20.;
pub const DEFAULT_LINE_HEIGHT: f64 = 1.25;
/// 手绘字体（官方 Excalifont）
pub const FONT_HAND: i64 = 5;
/// 便签（官方 stickynote，2026-09）
pub const STICKY_BG: &str = "#ffdf6b";
pub const STICKY_BGS: [&str; 5] = [STICKY_BG, "#fcc2d7", "#b2f2bb", "#a5d8ff", "#ffd8a8"];
pub const STICKY_PADDING: f64 = 16.;
pub const STICKY_FOOTER_H: f64 = 20.;
pub const STICKY_MIN: f64 = 75.;
pub const STICKY_SIZE: f64 = 250.;
pub const STICKY_MIN_FONT: f64 = 16.;
/// 常规字体（官方 Nunito）
pub const FONT_NORMAL: i64 = 6;
/// 代码字体（官方 Comic Shanns）
pub const FONT_CODE: i64 = 8;

fn d_type() -> String {
    "excalidraw".into()
}
fn d_version() -> i64 {
    2
}
fn d_stroke() -> String {
    DEFAULT_STROKE.into()
}
fn d_bg() -> String {
    TRANSPARENT.into()
}
fn d_solid() -> String {
    "solid".into()
}
fn d_two() -> f64 {
    2.
}
fn d_one() -> f64 {
    1.
}
fn d_hundred() -> f64 {
    100.
}
fn d_ver() -> i64 {
    1
}

/// 整个草图文件。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SketchDoc {
    #[serde(rename = "type", default = "d_type")]
    pub kind: String,
    #[serde(default = "d_version")]
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub elements: Vec<Element>,
    #[serde(default)]
    pub app_state: Map<String, Value>,
    #[serde(default)]
    pub files: BTreeMap<String, FileData>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for SketchDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl SketchDoc {
    pub fn new() -> Self {
        let mut app_state = Map::new();
        app_state.insert("viewBackgroundColor".into(), json!("#ffffff"));
        app_state.insert("gridSize".into(), json!(20));
        SketchDoc {
            kind: d_type(),
            version: 2,
            source: Some("https://stonebrook.app".into()),
            elements: vec![],
            app_state,
            files: BTreeMap::new(),
            extra: Map::new(),
        }
    }

    /// 未删除的元素（官方文件里删除的元素也可能留着 `isDeleted: true`）。
    pub fn live(&self) -> impl DoubleEndedIterator<Item = &Element> {
        self.elements.iter().filter(|e| !e.is_deleted)
    }

    pub fn is_empty(&self) -> bool {
        self.live().next().is_none()
    }

    pub fn index(&self, id: &str) -> Option<usize> {
        self.elements.iter().position(|e| e.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id && !e.is_deleted)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id && !e.is_deleted)
    }

    pub fn bg_color(&self) -> String {
        self.app_state.get("viewBackgroundColor").and_then(|v| v.as_str()).unwrap_or("#ffffff").to_string()
    }

    pub fn grid_size(&self) -> f64 {
        self.app_state.get("gridSize").and_then(|v| v.as_f64()).filter(|g| *g > 1.).unwrap_or(20.)
    }

    /// 本地图片路径（导入时 dataURL 已落盘到资源目录）。
    pub fn file_path(&self, file_id: &str) -> Option<String> {
        let f = self.files.get(file_id)?;
        f.local_path.clone()
    }

    /// 所有文字（导出 Markdown、搜索用）。
    pub fn texts(&self) -> Vec<String> {
        self.live().filter_map(|e| e.text.clone()).filter(|t| !t.trim().is_empty()).map(|t| t.replace('\n', " ")).collect()
    }
}

/// `files` 里的一张图。`dataURL` 导入后落盘并清空，`localPath` 是本地文件（导出时再内联回 dataURL）。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(rename = "dataURL", default, skip_serializing_if = "String::is_empty")]
    pub data_url: String,
    #[serde(rename = "stonebrookPath", default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 一个元素。类型相关的可选字段（绑定、箭头头、容器、图片……）放在 `extra` 里，用访问器读写，
/// 这样官方的 `null` 与「缺省」两种状态都能原样保留。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, serialize_with = "ser_f")]
    pub x: f64,
    #[serde(default, serialize_with = "ser_f")]
    pub y: f64,
    #[serde(default, serialize_with = "ser_f")]
    pub width: f64,
    #[serde(default, serialize_with = "ser_f")]
    pub height: f64,
    #[serde(default, serialize_with = "ser_f")]
    pub angle: f64,
    #[serde(default = "d_stroke")]
    pub stroke_color: String,
    #[serde(default = "d_bg")]
    pub background_color: String,
    #[serde(default = "d_solid")]
    pub fill_style: String,
    #[serde(default = "d_two", serialize_with = "ser_f")]
    pub stroke_width: f64,
    #[serde(default = "d_solid")]
    pub stroke_style: String,
    #[serde(default = "d_one", serialize_with = "ser_f")]
    pub roughness: f64,
    #[serde(default = "d_hundred", serialize_with = "ser_f")]
    pub opacity: f64,
    #[serde(default)]
    pub group_ids: Vec<String>,
    #[serde(default)]
    pub seed: i64,
    #[serde(default = "d_ver")]
    pub version: i64,
    #[serde(default)]
    pub version_nonce: i64,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub updated: i64,
    #[serde(default)]
    pub locked: bool,
    /// 线 / 箭头 / 自由画的点，相对 (x, y)
    #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "ser_pts")]
    pub points: Option<Vec<[f64; 2]>>,
    /// 文字（容器文字为换行后的结果，原文在 originalText）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "ser_of")]
    pub font_size: Option<f64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 线 / 箭头端点绑定。
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub element_id: String,
    /// 官方新版：端点在目标包围盒里的相对位置（0..1）
    pub fixed_point: Option<[f64; 2]>,
    pub gap: f64,
}

impl Element {
    pub fn new(kind: &str, x: f64, y: f64) -> Element {
        let mut e = Element {
            id: new_id(),
            kind: kind.into(),
            x,
            y,
            width: 0.,
            height: 0.,
            angle: 0.,
            stroke_color: d_stroke(),
            background_color: d_bg(),
            fill_style: d_solid(),
            stroke_width: 2.,
            stroke_style: d_solid(),
            roughness: 1.,
            opacity: 100.,
            group_ids: vec![],
            seed: rand_i32() as i64,
            version: 1,
            version_nonce: rand_i32() as i64,
            is_deleted: false,
            updated: now_ms(),
            locked: false,
            points: None,
            text: None,
            font_size: None,
            extra: Map::new(),
        };
        e.set("frameId", Value::Null);
        e.set("boundElements", Value::Null);
        e.set("link", Value::Null);
        match kind {
            "rectangle" | "stickynote" => e.set("roundness", json!({ "type": 3 })),
            "diamond" => e.set("roundness", json!({ "type": 2 })),
            "ellipse" | "freedraw" | "text" | "image" | "frame" => e.set("roundness", Value::Null),
            "arrow" | "line" => {
                e.set("roundness", json!({ "type": 2 }));
                e.points = Some(vec![[0., 0.]]);
                e.set("lastCommittedPoint", Value::Null);
                e.set("startBinding", Value::Null);
                e.set("endBinding", Value::Null);
                e.set("startArrowhead", Value::Null);
                e.set("endArrowhead", if kind == "arrow" { json!("arrow") } else { Value::Null });
                if kind == "arrow" {
                    e.set("elbowed", json!(false));
                }
            }
            _ => {}
        }
        match kind {
            "freedraw" => {
                e.points = Some(vec![[0., 0.]]);
                e.set("pressures", json!([]));
                e.set("simulatePressure", json!(true));
                e.set("lastCommittedPoint", Value::Null);
            }
            "text" => {
                e.text = Some(String::new());
                e.font_size = Some(DEFAULT_FONT_SIZE);
                e.set("fontFamily", json!(FONT_HAND));
                e.set("textAlign", json!("left"));
                e.set("verticalAlign", json!("top"));
                e.set("containerId", Value::Null);
                e.set("originalText", json!(""));
                e.set("autoResize", json!(true));
                e.set("lineHeight", json!(DEFAULT_LINE_HEIGHT));
            }
            "image" => {
                e.set("fileId", Value::Null);
                e.set("status", json!("saved"));
                e.set("scale", json!([1, 1]));
                e.set("crop", Value::Null);
            }
            "stickynote" => {
                e.background_color = STICKY_BG.into();
                e.fill_style = "solid".into();
                e.set("baseHeight", json!(0));
                e.set("created", json!(now_ms()));
            }
            "embeddable" => {
                e.set("roundness", json!({ "type": 3 }));
                e.set("validated", json!(true));
            }
            "frame" => {
                e.set("name", Value::Null);
                e.stroke_color = "#bbb".into();
                e.roughness = 0.;
            }
            _ => {}
        }
        e
    }

    // ── extra 字段访问 ──

    pub fn get(&self, k: &str) -> Option<&Value> {
        self.extra.get(k).filter(|v| !v.is_null())
    }
    pub fn set(&mut self, k: &str, v: Value) {
        self.extra.insert(k.into(), v);
    }
    pub fn str_of(&self, k: &str) -> Option<&str> {
        self.get(k).and_then(|v| v.as_str())
    }

    pub fn is_linear(&self) -> bool {
        matches!(self.kind.as_str(), "arrow" | "line")
    }
    pub fn is_text(&self) -> bool {
        self.kind == "text"
    }
    pub fn is_frame(&self) -> bool {
        matches!(self.kind.as_str(), "frame" | "magicframe")
    }
    /// 能放文字的容器
    pub fn is_container(&self) -> bool {
        matches!(self.kind.as_str(), "rectangle" | "diamond" | "ellipse" | "stickynote")
    }
    pub fn is_sticky(&self) -> bool {
        self.kind == "stickynote"
    }
    /// 能被箭头绑定
    pub fn is_bindable(&self) -> bool {
        matches!(self.kind.as_str(), "rectangle" | "diamond" | "ellipse" | "stickynote" | "text" | "image" | "frame" | "magicframe" | "iframe" | "embeddable")
    }

    pub fn has_fill(&self) -> bool {
        self.background_color != TRANSPARENT && !self.background_color.is_empty()
    }

    pub fn container_id(&self) -> Option<&str> {
        self.str_of("containerId")
    }
    pub fn frame_id(&self) -> Option<&str> {
        self.str_of("frameId")
    }
    pub fn file_id(&self) -> Option<&str> {
        self.str_of("fileId")
    }
    pub fn font_family(&self) -> i64 {
        self.get("fontFamily").and_then(|v| v.as_i64()).unwrap_or(FONT_HAND)
    }
    pub fn text_align(&self) -> &str {
        self.str_of("textAlign").unwrap_or("left")
    }
    pub fn vertical_align(&self) -> &str {
        self.str_of("verticalAlign").unwrap_or("top")
    }
    pub fn line_height(&self) -> f64 {
        self.get("lineHeight").and_then(|v| v.as_f64()).filter(|v| *v > 0.3).unwrap_or(DEFAULT_LINE_HEIGHT)
    }
    pub fn font_size_or(&self) -> f64 {
        self.font_size.unwrap_or(DEFAULT_FONT_SIZE)
    }
    pub fn original_text(&self) -> String {
        self.str_of("originalText").map(|s| s.to_string()).or_else(|| self.text.clone()).unwrap_or_default()
    }
    pub fn name(&self) -> Option<&str> {
        self.str_of("name")
    }
    pub fn link(&self) -> Option<&str> {
        self.str_of("link")
    }
    pub fn arrowhead(&self, start: bool) -> Option<&str> {
        self.str_of(if start { "startArrowhead" } else { "endArrowhead" })
    }
    pub fn elbowed(&self) -> bool {
        self.get("elbowed").and_then(|v| v.as_bool()).unwrap_or(false)
    }

    /// 圆角：None 为直角；Some(type) 为官方 roundness.type（2 比例、3 自适应）
    pub fn roundness(&self) -> Option<i64> {
        self.get("roundness").map(|v| v.get("type").and_then(|t| t.as_i64()).unwrap_or(3))
    }
    pub fn roundness_value(&self) -> Option<f64> {
        self.get("roundness").and_then(|v| v.get("value")).and_then(|v| v.as_f64())
    }

    pub fn binding(&self, start: bool) -> Option<Binding> {
        let v = self.get(if start { "startBinding" } else { "endBinding" })?;
        let id = v.get("elementId")?.as_str()?.to_string();
        let fixed_point = v.get("fixedPoint").and_then(|f| {
            let a = f.as_array()?;
            Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
        });
        let gap = v.get("gap").and_then(|g| g.as_f64()).unwrap_or(4.);
        Some(Binding { element_id: id, fixed_point, gap })
    }
    pub fn set_binding(&mut self, start: bool, target: Option<&str>) {
        let k = if start { "startBinding" } else { "endBinding" };
        match target {
            Some(id) => self.set(k, json!({ "elementId": id, "focus": 0, "gap": 4 })),
            None => self.set(k, Value::Null),
        }
    }

    /// boundElements：[(id, type)]
    pub fn bound_elements(&self) -> Vec<(String, String)> {
        self.get("boundElements")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|b| Some((b.get("id")?.as_str()?.to_string(), b.get("type").and_then(|t| t.as_str()).unwrap_or("arrow").to_string())))
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn add_bound(&mut self, id: &str, kind: &str) {
        let mut v = self.bound_elements();
        if v.iter().any(|(i, _)| i == id) {
            return;
        }
        v.push((id.into(), kind.into()));
        self.set_bound(v);
    }
    pub fn remove_bound(&mut self, id: &str) {
        let mut v = self.bound_elements();
        let n = v.len();
        v.retain(|(i, _)| i != id);
        if v.len() != n {
            self.set_bound(v);
        }
    }
    fn set_bound(&mut self, v: Vec<(String, String)>) {
        if v.is_empty() {
            self.set("boundElements", Value::Null);
        } else {
            self.set("boundElements", Value::Array(v.into_iter().map(|(id, t)| json!({ "id": id, "type": t })).collect()));
        }
    }
    /// 绑定在本元素上的文字（容器文字 / 箭头标签）
    pub fn bound_text_id(&self) -> Option<String> {
        self.bound_elements().into_iter().find(|(_, t)| t == "text").map(|(id, _)| id)
    }

    pub fn pts(&self) -> &[[f64; 2]] {
        self.points.as_deref().unwrap_or(&[])
    }

    /// 修改后递增版本（官方协作 / 合并依赖它；本地只用来区分改动）
    pub fn bump(&mut self) {
        self.version += 1;
        self.version_nonce = rand_i32() as i64;
        self.updated = now_ms();
    }

    /// 线 / 自由画：把第一个点平移到 (0,0)，并按点重新计算宽高（官方约定）。
    pub fn normalize_points(&mut self) {
        let Some(pts) = self.points.as_mut() else { return };
        if pts.is_empty() {
            return;
        }
        let [ox, oy] = pts[0];
        if ox != 0. || oy != 0. {
            for p in pts.iter_mut() {
                p[0] -= ox;
                p[1] -= oy;
            }
            self.x += ox;
            self.y += oy;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in pts.iter() {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        self.width = x1 - x0;
        self.height = y1 - y0;
    }
}

// ───────────── 工具 ─────────────

/// 数字按 JS `JSON.stringify` 的习惯输出：整数不带 `.0`（与官方文件逐字段等价）。
fn ser_f<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if v.fract() == 0. && v.abs() < 9.0e15 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}

fn ser_of<S: serde::Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(v) => ser_f(v, s),
        None => s.serialize_none(),
    }
}

fn ser_pts<S: serde::Serializer>(v: &Option<Vec<[f64; 2]>>, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let Some(pts) = v else { return s.serialize_none() };
    let n = |x: f64| if x.fract() == 0. && x.abs() < 9.0e15 { serde_json::Value::from(x as i64) } else { serde_json::Value::from(x) };
    let mut seq = s.serialize_seq(Some(pts.len()))?;
    for p in pts {
        seq.serialize_element(&[n(p[0]), n(p[1])])?;
    }
    seq.end()
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 非加密随机数（seed / versionNonce 用）
pub fn rand_u64() -> u64 {
    use std::cell::Cell;
    thread_local! {
        static S: Cell<u64> = Cell::new({
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9e37);
            t ^ 0x9e37_79b9_7f4a_7c15
        });
    }
    S.with(|s| {
        // xorshift64*
        let mut x = s.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    })
}

pub fn rand_i32() -> i32 {
    (rand_u64() >> 33) as i32
}

/// 官方元素 id 是 21 位 nanoid；这里用同样长度的随机串。
pub fn new_id() -> String {
    const A: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_-";
    (0..21).map(|_| A[(rand_u64() % 64) as usize] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_doc_roundtrip() {
        let d = SketchDoc::new();
        let s = serde_json::to_string(&d).unwrap();
        let back: SketchDoc = serde_json::from_str(&s).unwrap();
        assert_eq!(d, back);
        assert!(s.contains("\"type\":\"excalidraw\""));
    }

    #[test]
    fn unknown_fields_are_kept() {
        let src = r#"{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":1,"y":2,"width":3,"height":4,
            "futureField":{"k":[1,2]},"startBinding":null,"index":"a0"}],"appState":{"gridSize":null,"newThing":true},"files":{},"topLevelNew":1}"#;
        let d: SketchDoc = serde_json::from_str(src).unwrap();
        let out = serde_json::to_value(&d).unwrap();
        assert_eq!(out["topLevelNew"], 1);
        assert_eq!(out["appState"]["newThing"], true);
        let e = &out["elements"][0];
        assert_eq!(e["futureField"]["k"][1], 2);
        assert_eq!(e["index"], "a0");
        assert!(e.get("startBinding").is_some_and(|v| v.is_null()));
    }

    #[test]
    fn normalize_moves_origin() {
        let mut e = Element::new("line", 10., 10.);
        e.points = Some(vec![[5., 5.], [15., 25.]]);
        e.normalize_points();
        assert_eq!((e.x, e.y, e.width, e.height), (15., 15., 10., 20.));
        assert_eq!(e.pts()[0], [0., 0.]);
    }
}
