//! PDF 批注阅读（`ObjKind::PdfDoc`，磁盘串 `"pdfdoc"`）。
//!
//! - PDF 文件拷进 assets 目录，对象行里存 [`PdfDoc`]（文件路径、上次页码、批注列表）；
//! - 批注的权威数据在 SQLite，**不写回原 PDF**；打开时由 [`host`] 转成 PDFKit 的临时 `PDFAnnotation` 显示，
//!   只有「导出带批注的 PDF」才会把它们写进一份新文件；
//! - 阅读界面是原生 `PDFView`（选字、搜索、目录、缩略图都用系统能力），叠在 GPUI 窗口上，见 [`view`]。

pub mod host;
pub mod view;

use serde::{Deserialize, Serialize};

/// 高亮颜色（黄、绿、蓝、粉）：(名称, 色值)
pub const COLORS: [(&str, u32); 4] = [("黄色", 0xffd400), ("绿色", 0x5fd068), ("蓝色", 0x5ab0ff), ("粉色", 0xff7eb6)];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotKind {
    #[default]
    Highlight,
    Underline,
    Strike,
    /// 页内便签（只有一个矩形，正文在 note 里）
    Note,
}

impl AnnotKind {
    pub fn label(self) -> &'static str {
        match self {
            AnnotKind::Highlight => "高亮",
            AnnotKind::Underline => "下划线",
            AnnotKind::Strike => "删除线",
            AnnotKind::Note => "便签",
        }
    }
}

/// 一条批注。锚点 = 页码 + 页内矩形（PDF 坐标，左下原点）+ 摘录文本。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Annot {
    pub id: String,
    /// 从 0 开始的页序号
    pub page: usize,
    #[serde(default)]
    pub kind: AnnotKind,
    /// 颜色序号，见 [`COLORS`]
    #[serde(default)]
    pub color: u8,
    /// 每行一个矩形 (x, y, w, h)
    #[serde(default)]
    pub rects: Vec<(f32, f32, f32, f32)>,
    /// 选中的原文
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// 批注 / 便签内容
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// 创建时间（毫秒）
    #[serde(default)]
    pub created: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PdfDoc {
    /// assets 目录里的 PDF 路径
    pub path: String,
    /// 显示名（原文件名）
    pub name: String,
    /// 上次读到的页（从 0 开始）
    #[serde(default)]
    pub last_page: usize,
    #[serde(default)]
    pub annots: Vec<Annot>,
}

impl PdfDoc {
    /// 按页码、再按页内从上到下排序的批注（侧栏列表 / 导出用）。
    pub fn sorted(&self) -> Vec<&Annot> {
        let mut v: Vec<&Annot> = self.annots.iter().collect();
        v.sort_by(|a, b| {
            let top = |x: &Annot| x.rects.iter().map(|r| r.1 + r.3).fold(f32::MIN, f32::max);
            a.page.cmp(&b.page).then(top(b).partial_cmp(&top(a)).unwrap_or(std::cmp::Ordering::Equal))
        });
        v
    }

    pub fn annot(&self, id: &str) -> Option<&Annot> {
        self.annots.iter().find(|a| a.id == id)
    }

    /// 批注导出为 Markdown：按页分组，高亮为引用，便签 / 批注内容跟在后面。
    pub fn to_markdown(&self) -> String {
        let mut out = format!("# {} · 批注\n", self.name);
        let mut page = usize::MAX;
        for a in self.sorted() {
            if a.page != page {
                page = a.page;
                out.push_str(&format!("\n## 第 {} 页\n\n", page + 1));
            }
            if !a.text.is_empty() {
                for line in a.text.lines() {
                    out.push_str(&format!("> {line}\n"));
                }
                out.push_str(&format!("> — {}（{}）\n", a.kind.label(), COLORS[(a.color as usize) % COLORS.len()].0));
            }
            if !a.note.is_empty() {
                if !a.text.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("- {}\n", a.note.replace('\n', " ")));
            }
            out.push('\n');
        }
        out
    }
}

/// 摘录链接：`stonebrook://pdf/<对象 id>?page=<页>&annot=<批注 id>`，点击打开阅读器并定位。
pub const PDF_LINK_PREFIX: &str = "stonebrook://pdf/";

pub fn pdf_link(obj: &str, page: usize, annot: &str) -> String {
    format!("{PDF_LINK_PREFIX}{obj}?page={page}&annot={annot}")
}

/// 解析摘录链接：(对象 id, 页, 批注 id)。
pub fn pdf_link_target(url: &str) -> Option<(String, usize, String)> {
    let rest = url.strip_prefix(PDF_LINK_PREFIX)?;
    let (obj, q) = rest.split_once('?').unwrap_or((rest, ""));
    if obj.is_empty() {
        return None;
    }
    let mut page = 0;
    let mut annot = String::new();
    for kv in q.split('&') {
        match kv.split_once('=') {
            Some(("page", v)) => page = v.parse().unwrap_or(0),
            Some(("annot", v)) => annot = v.to_string(),
            _ => {}
        }
    }
    Some((obj.to_string(), page, annot))
}

pub fn new_annot_id() -> String {
    crate::store::new_uuid()[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(page: usize, y: f32, text: &str) -> Annot {
        Annot { id: format!("{page}-{y}"), page, rects: vec![(10., y, 100., 12.)], text: text.into(), ..Default::default() }
    }

    #[test]
    fn sorted_by_page_then_top_down() {
        let d = PdfDoc { annots: vec![a(1, 500., "c"), a(0, 100., "b"), a(0, 700., "a")], ..Default::default() };
        let order: Vec<&str> = d.sorted().iter().map(|x| x.text.as_str()).collect();
        assert_eq!(order, ["a", "b", "c"]);
    }

    #[test]
    fn markdown_groups_by_page() {
        let mut n = a(2, 300., "");
        n.kind = AnnotKind::Note;
        n.note = "便签内容".into();
        let d = PdfDoc { name: "论文.pdf".into(), annots: vec![a(0, 100., "第一句"), n], ..Default::default() };
        let md = d.to_markdown();
        assert!(md.starts_with("# 论文.pdf · 批注"));
        assert!(md.contains("## 第 1 页\n\n> 第一句\n> — 高亮（黄色）"));
        assert!(md.contains("## 第 3 页\n\n- 便签内容"));
    }

    #[test]
    fn links_round_trip() {
        let url = pdf_link("abc", 12, "x1");
        assert_eq!(pdf_link_target(&url), Some(("abc".into(), 12, "x1".into())));
        assert_eq!(pdf_link_target("stonebrook://pdf/abc"), Some(("abc".into(), 0, String::new())));
        assert_eq!(pdf_link_target("stonebrook://pdf/"), None);
    }

    #[test]
    fn json_round_trip_and_defaults() {
        let d = PdfDoc { path: "/a.pdf".into(), name: "a.pdf".into(), last_page: 3, annots: vec![a(0, 1., "t")] };
        let back: PdfDoc = serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
        assert_eq!(back, d);
        let old: PdfDoc = serde_json::from_str(r#"{"path":"/b.pdf","name":"b.pdf"}"#).unwrap();
        assert!(old.annots.is_empty());
    }
}
