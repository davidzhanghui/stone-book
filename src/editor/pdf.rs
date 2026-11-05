//! 文档里的「PDF 阅读」块：插入（选文件 / 附件转换）、预览卡片、摘录到正文。

use super::*;
use crate::model::BlockKind;
use crate::pdfdoc::{Annot, PdfDoc};
use crate::theme::Theme;
use std::path::{Path, PathBuf};

/// 首页缩略图（缓存成 PNG，按文件路径 + 修改时间）。
fn thumb(path: &str) -> Option<PathBuf> {
    let stamp = std::fs::metadata(path).ok().map(|m| format!("{}-{:?}", m.len(), m.modified().ok()))?;
    let out = crate::webview::cache_dir().join(format!("pdf-{}.png", crate::webview::hash_key(&[path, &stamp, "thumb1"])));
    if !out.exists() {
        let png = crate::pdfdoc::host::thumbnail_png(Path::new(path), 520.)?;
        std::fs::write(&out, png).ok()?;
    }
    Some(out)
}

/// 文档里的预览：左边首页缩略图，右边文件名、页数、批注数与最近几条批注。
pub(super) fn pdf_preview(p: &PdfDoc, w: f32, t: &Theme) -> AnyElement {
    let pages = crate::pdfdoc::host::page_count(Path::new(&p.path));
    let recent: Vec<String> = p.sorted().into_iter().rev().take(3).map(|a| if a.text.is_empty() { a.note.clone() } else { a.text.clone() }).collect();
    div()
        .w(px(w))
        .flex()
        .flex_row()
        .gap(px(16.))
        .p(px(8.))
        .child(match thumb(&p.path) {
            Some(img_path) => div()
                .w(px(130.))
                .h(px(170.))
                .flex_none()
                .rounded(px(4.))
                .border_1()
                .border_color(t.border)
                .bg(gpui::white())
                .overflow_hidden()
                .child(img(img_path).size_full().object_fit(ObjectFit::Contain))
                .into_any_element(),
            None => div()
                .w(px(130.))
                .h(px(170.))
                .flex_none()
                .rounded(px(4.))
                .bg(t.code_bg)
                .flex()
                .items_center()
                .justify_center()
                .text_color(t.text_muted)
                .child("PDF")
                .into_any_element(),
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(15.))
                        .text_color(t.text)
                        .font_weight(FontWeight::SEMIBOLD)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(p.name.clone()),
                )
                .child(div().text_size(px(12.)).text_color(t.text_muted).child(match pages {
                    Some(n) => format!("{n} 页 · {} 条批注 · 读到第 {} 页", p.annots.len(), p.last_page + 1),
                    None => crate::i18n::tr("文件不存在或无法打开").to_string(),
                }))
                .children(
                    recent
                        .into_iter()
                        .map(|s| div().pl(px(8.)).border_l_2().border_color(t.accent).text_size(px(13.)).text_color(t.text_secondary).line_clamp(2).child(s)),
                ),
        )
        .into_any_element()
}

impl Editor {
    /// 斜杠「PDF 阅读」：选一个 PDF。
    pub(super) fn pick_pdf(&mut self, cx: &mut Context<Self>) {
        let rx =
            cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("打开".into()), ..Default::default() });
        cx.spawn(async move |this: WeakEntity<Editor>, cx: &mut AsyncApp| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.into_iter().next() {
                    this.update(cx, |this, cx| this.insert_pdf(&p, None, cx)).ok();
                }
            }
        })
        .detach();
    }

    /// 把 PDF 拷进 assets，建「PDF 阅读」对象插到光标处（`replace` 给了就替换那个附件块）并打开。
    pub(crate) fn insert_pdf(&mut self, src: &Path, replace: Option<usize>, cx: &mut Context<Self>) {
        let is_pdf = src.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        if !is_pdf || crate::pdfdoc::host::page_count(src).is_none() {
            cx.emit(EditorEvent::Toast("请选择可以打开的 PDF 文件".into()));
            return;
        }
        // 附件本来就在 assets 里，直接沿用；外部文件先拷进来
        let assets = self.store.assets_dir();
        let path = if src.starts_with(&assets) { Some(src.to_string_lossy().to_string()) } else { super::clip::copy_asset(src, &assets) };
        let Some(path) = path else {
            cx.emit(EditorEvent::Toast("拷贝 PDF 失败".into()));
            return;
        };
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "PDF".into());
        let doc = PdfDoc { path, name, ..Default::default() };
        let id = crate::objects::create("pdfdoc", Some(crate::objects::ObjData::Pdf(std::sync::Arc::new(doc))));
        let blk = Block::new(BlockKind::Embed { kind: "pdfdoc".into(), id: id.clone() }, RichText::default());
        self.checkpoint(EditKind::Other);
        match replace {
            Some(ix) if ix < self.content.blocks.len() => {
                *self.block_mut(ix) = blk;
                self.set_cursor(Pos::new(ix, 0, 0), false, cx);
            }
            _ => {
                let at = (self.head.block + 1).min(self.content.blocks.len());
                // 光标在空段落上：直接占用这一行
                let empty = self.content.blocks.get(self.head.block).is_some_and(|b| b.kind == BlockKind::Text && b.text.is_empty() && self.head.block > 0);
                if empty {
                    let ix = self.head.block;
                    *self.block_mut(ix) = blk;
                    self.set_cursor(Pos::new(ix, 0, 0), false, cx);
                } else {
                    self.insert_blocks(at, vec![blk]);
                    self.set_cursor(Pos::new(at, 0, 0), false, cx);
                }
            }
        }
        self.changed(cx);
        cx.emit(EditorEvent::OpenEmbed { kind: "pdfdoc".into(), id });
    }

    /// 附件里的 PDF 转为可批注的「PDF 阅读」块。
    pub(super) fn convert_file_to_pdf(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(BlockKind::File { path, .. }) = self.content.blocks.get(ix).map(|b| b.kind.clone()) else { return };
        self.insert_pdf(Path::new(&path), Some(ix), cx);
    }

    /// 摘录：在这份 PDF 的块后面（已有摘录之后）插入引用块，末尾带回跳链接。
    pub fn insert_pdf_excerpt(&mut self, obj: &str, a: &Annot, cx: &mut Context<Self>) {
        let pos = self.content.blocks.iter().position(|b| matches!(&b.kind, BlockKind::Embed { id, .. } if id == obj));
        let mut at = pos.map_or(self.content.blocks.len(), |i| i + 1);
        // 跳过紧跟在后面的已有摘录（引用块且含本 PDF 的链接），新的接在最后
        let prefix = format!("{}{obj}", crate::pdfdoc::PDF_LINK_PREFIX);
        while self
            .content
            .blocks
            .get(at)
            .is_some_and(|b| b.kind == BlockKind::Quote && b.text.runs().any(|(_, m)| m.link.as_ref().is_some_and(|l| l.starts_with(&prefix))))
        {
            at += 1;
        }
        let body = if a.text.is_empty() { a.note.clone() } else { a.text.clone() };
        let mut text = RichText::plain(body);
        if !a.note.is_empty() && !a.text.is_empty() {
            text.insert(text.len(), &format!("\n批注：{}", a.note), Marks { italic: true, ..Default::default() });
        }
        let label = format!(" — 第 {} 页", a.page + 1);
        let link = Marks { link: Some(crate::pdfdoc::pdf_link(obj, a.page, &a.id)), ..Default::default() };
        text.insert(text.len(), &label, link);
        self.checkpoint(EditKind::Other);
        self.insert_blocks(at, vec![Block::new(BlockKind::Quote, text)]);
        self.changed(cx);
    }
}
