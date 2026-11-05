//! PDFKit 宿主：把系统的 `PDFThumbnailView` + `PDFView` 叠在 GPUI 窗口上（写法同 webview.rs 的 Quick Look 覆盖层）。
//!
//! 同一时刻只打开一份 PDF。所有调用都在主线程（GPUI 前台执行器就是主线程）。
//! 批注只显示、不落盘：数据在 [`super::PdfDoc`]，这里按批注 id 记下生成的 `PDFAnnotation`，删除时一并移除。

use super::{Annot, AnnotKind, COLORS};
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSColor, NSView};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSString, NSStringCompareOptions, NSURL};
use objc2_pdf_kit::{
    PDFAnnotation, PDFAnnotationSubtypeHighlight, PDFAnnotationSubtypeStrikeOut, PDFAnnotationSubtypeText, PDFAnnotationSubtypeUnderline, PDFDisplayBox,
    PDFDisplayMode, PDFDocument, PDFOutline, PDFPage, PDFSelection, PDFThumbnailView, PDFView,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

/// 缩略图条宽度（逻辑像素）
pub const THUMBS_W: f64 = 120.;

struct Host {
    obj: String,
    root: Retained<NSView>,
    thumbs: Retained<PDFThumbnailView>,
    pdf: Retained<PDFView>,
    doc: Retained<PDFDocument>,
    parent: Retained<NSView>,
    shown: HashMap<String, Vec<Retained<PDFAnnotation>>>,
    find: Vec<Retained<PDFSelection>>,
    find_ix: usize,
    show_thumbs: bool,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    HOST.with(|h| h.borrow_mut().as_mut().map(f))
}

fn open_doc(path: &Path) -> Option<Retained<PDFDocument>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
}

/// PDF 页数（打不开为 None）。
pub fn page_count(path: &Path) -> Option<usize> {
    open_doc(path).map(|d| unsafe { d.pageCount() })
}

/// 首页缩略图（PNG，最长边 `max` 像素）。
pub fn thumbnail_png(path: &Path, max: f64) -> Option<Vec<u8>> {
    let doc = open_doc(path)?;
    let page = unsafe { doc.pageAtIndex(0) }?;
    let b = unsafe { page.boundsForBox(PDFDisplayBox::CropBox) };
    let s = (max / b.size.width.max(b.size.height).max(1.)).min(4.);
    let img = unsafe { page.thumbnailOfSize_forBox(CGSize::new(b.size.width * s, b.size.height * s), PDFDisplayBox::CropBox) };
    let tiff = img.TIFFRepresentation()?;
    let img = image::load_from_memory(&tiff.to_vec()).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// GPUI 窗口坐标（左上原点）→ AppKit 父视图坐标（左下原点）。
fn frame_for(parent: &NSView, bounds: gpui::Bounds<gpui::Pixels>) -> CGRect {
    let view_h = parent.bounds().size.height;
    CGRect::new(
        CGPoint::new(f64::from(bounds.origin.x), view_h - f64::from(bounds.origin.y) - f64::from(bounds.size.height)),
        CGSize::new(f64::from(bounds.size.width).max(1.), f64::from(bounds.size.height).max(1.)),
    )
}

fn layout(h: &Host, frame: CGRect) {
    h.root.setFrame(frame);
    let tw = if h.show_thumbs { THUMBS_W.min(frame.size.width / 3.) } else { 0. };
    h.thumbs.setHidden(!h.show_thumbs);
    h.thumbs.setFrame(CGRect::new(CGPoint::new(0., 0.), CGSize::new(tw, frame.size.height)));
    h.pdf.setFrame(CGRect::new(CGPoint::new(tw, 0.), CGSize::new((frame.size.width - tw).max(1.), frame.size.height)));
}

/// 显示（或调整位置）。`obj` 不同则换一份文档；返回 false 表示文件打不开。
pub fn show(window: &gpui::Window, obj: &str, path: &Path, bounds: gpui::Bounds<gpui::Pixels>, annots: &[Annot], start_page: usize, dark: bool) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else { return false };
    let RawWindowHandle::AppKit(rh) = handle.as_raw() else { return false };
    let parent: &NSView = unsafe { &*(rh.ns_view.as_ptr() as *const NSView) };
    let frame = frame_for(parent, bounds);
    if with(|h| h.obj == obj).unwrap_or(false) {
        with(|h| layout(h, frame));
        return true;
    }
    hide();
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let Some(doc) = open_doc(path) else { return false };
    let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let pdf = unsafe { PDFView::initWithFrame(PDFView::alloc(mtm), frame) };
    let thumbs = unsafe { PDFThumbnailView::initWithFrame(PDFThumbnailView::alloc(mtm), frame) };
    unsafe {
        pdf.setDocument(Some(&doc));
        pdf.setAutoScales(true);
        pdf.setDisplayMode(PDFDisplayMode::SinglePageContinuous);
        let bg = if dark {
            NSColor::colorWithSRGBRed_green_blue_alpha(0.12, 0.12, 0.13, 1.)
        } else {
            NSColor::colorWithSRGBRed_green_blue_alpha(0.93, 0.94, 0.95, 1.)
        };
        pdf.setBackgroundColor(&bg);
        thumbs.setPDFView(Some(&pdf));
        thumbs.setThumbnailSize(CGSize::new(80., 104.));
        thumbs.setMaximumNumberOfColumns(1);
        thumbs.setBackgroundColor(Some(&bg));
    }
    root.addSubview(&thumbs);
    root.addSubview(&pdf);
    parent.addSubview(&root);
    let host = Host {
        obj: obj.to_string(),
        root,
        thumbs,
        pdf,
        doc,
        parent: unsafe { Retained::retain(parent as *const _ as *mut NSView) }.expect("ns_view"),
        shown: HashMap::new(),
        find: vec![],
        find_ix: 0,
        show_thumbs: true,
    };
    layout(&host, frame);
    HOST.with(|h| *h.borrow_mut() = Some(host));
    for a in annots {
        add_annot(a);
    }
    go_to(start_page, None);
    true
}

/// 收起原生视图，返回关闭前所在的页。
pub fn hide() -> Option<usize> {
    let h = HOST.with(|h| h.borrow_mut().take())?;
    let page = current_page_of(&h);
    h.root.removeFromSuperview();
    if let Some(win) = h.parent.window() {
        let _ = win.makeFirstResponder(Some(&h.parent));
    }
    Some(page)
}

pub fn toggle_thumbs() {
    with(|h| {
        h.show_thumbs = !h.show_thumbs;
        let f = h.root.frame();
        layout(h, f);
    });
}

fn current_page_of(h: &Host) -> usize {
    unsafe { h.pdf.currentPage().map(|p| h.doc.indexForPage(&p)).unwrap_or(0) }
}

/// (当前页, 总页数)
pub fn page_info() -> Option<(usize, usize)> {
    with(|h| (current_page_of(h), unsafe { h.doc.pageCount() }))
}

/// 跳到某页；给了矩形就把它滚进视野。
pub fn go_to(page: usize, rect: Option<(f32, f32, f32, f32)>) {
    with(|h| unsafe {
        let n = h.doc.pageCount();
        if n == 0 {
            return;
        }
        let Some(p) = h.doc.pageAtIndex(page.min(n - 1)) else { return };
        match rect {
            Some((x, y, w, hh)) => {
                let r = CGRect::new(CGPoint::new(x as f64, y as f64 - 60.), CGSize::new(w as f64, hh as f64 + 120.));
                h.pdf.goToRect_onPage(r, &p);
            }
            None => h.pdf.goToPage(&p),
        }
    });
}

pub fn step_page(d: i32) {
    with(|h| unsafe {
        if d > 0 && h.pdf.canGoToNextPage() {
            h.pdf.goToNextPage(None);
        } else if d < 0 && h.pdf.canGoToPreviousPage() {
            h.pdf.goToPreviousPage(None);
        }
    });
}

pub fn zoom(d: i32) {
    with(|h| unsafe {
        match d {
            0 => h.pdf.setAutoScales(true),
            d if d > 0 && h.pdf.canZoomIn() => h.pdf.zoomIn(None),
            d if d < 0 && h.pdf.canZoomOut() => h.pdf.zoomOut(None),
            _ => {}
        }
    });
}

/// 当前选区：(按页分组的每行矩形, 选中文字)。没有选中文字时为 None。
pub fn selection() -> Option<(Vec<(usize, Vec<(f32, f32, f32, f32)>)>, String)> {
    with(|h| unsafe {
        let sel = h.pdf.currentSelection()?;
        let text = sel.string().map(|s| s.to_string()).unwrap_or_default();
        if text.trim().is_empty() {
            return None;
        }
        let mut pages: Vec<(usize, Vec<(f32, f32, f32, f32)>)> = vec![];
        for line in sel.selectionsByLine().iter() {
            for page in line.pages().iter() {
                let r = line.boundsForPage(&page);
                if r.size.width <= 0. || r.size.height <= 0. {
                    continue;
                }
                let ix = h.doc.indexForPage(&page);
                let rect = (r.origin.x as f32, r.origin.y as f32, r.size.width as f32, r.size.height as f32);
                match pages.iter_mut().find(|(p, _)| *p == ix) {
                    Some((_, v)) => v.push(rect),
                    None => pages.push((ix, vec![rect])),
                }
            }
        }
        (!pages.is_empty()).then_some((pages, text.split_whitespace().collect::<Vec<_>>().join(" ")))
    })
    .flatten()
}

pub fn clear_selection() {
    with(|h| unsafe { h.pdf.clearSelection() });
}

/// 当前页可见区域中间偏上的一个位置（无选区时放便签用）。
pub fn note_anchor() -> Option<(usize, (f32, f32, f32, f32))> {
    with(|h| unsafe {
        let page = h.pdf.currentPage()?;
        let b = page.boundsForBox(PDFDisplayBox::CropBox);
        let ix = h.doc.indexForPage(&page);
        Some((ix, ((b.origin.x + b.size.width - 48.) as f32, (b.origin.y + b.size.height - 72.) as f32, 22., 22.)))
    })
    .flatten()
}

fn color(i: u8, alpha: f64) -> Retained<NSColor> {
    let c = COLORS[i as usize % COLORS.len()].1;
    let f = |s: u32| ((c >> s) & 0xff) as f64 / 255.;
    NSColor::colorWithSRGBRed_green_blue_alpha(f(16), f(8), f(0), alpha)
}

/// 显示一条批注。
pub fn add_annot(a: &Annot) {
    with(|h| unsafe {
        let Some(page) = h.doc.pageAtIndex(a.page) else { return };
        let subtype = match a.kind {
            AnnotKind::Highlight => PDFAnnotationSubtypeHighlight,
            AnnotKind::Underline => PDFAnnotationSubtypeUnderline,
            AnnotKind::Strike => PDFAnnotationSubtypeStrikeOut,
            AnnotKind::Note => PDFAnnotationSubtypeText,
        };
        let rects: Vec<(f32, f32, f32, f32)> = if a.kind == AnnotKind::Note { a.rects.iter().take(1).copied().collect() } else { a.rects.clone() };
        let mut made = vec![];
        for (x, y, w, hh) in rects {
            let r = CGRect::new(CGPoint::new(x as f64, y as f64), CGSize::new(w as f64, hh as f64));
            let ann = PDFAnnotation::initWithBounds_forType_withProperties(PDFAnnotation::alloc(), r, subtype, None);
            ann.setColor(&color(a.color, if a.kind == AnnotKind::Highlight { 0.5 } else { 1. }));
            if !a.note.is_empty() {
                ann.setContents(Some(&NSString::from_str(&a.note)));
            }
            ann.setUserName(Some(&NSString::from_str(&a.id)));
            page.addAnnotation(&ann);
            made.push(ann);
        }
        h.shown.insert(a.id.clone(), made);
    });
}

pub fn remove_annot(id: &str) {
    with(|h| unsafe {
        for ann in h.shown.remove(id).unwrap_or_default() {
            if let Some(p) = ann.page() {
                p.removeAnnotation(&ann);
            }
        }
    });
}

/// 重新显示一条批注（改了颜色 / 便签内容后）。
pub fn refresh_annot(a: &Annot) {
    remove_annot(&a.id);
    add_annot(a);
}

/// 全文搜索：高亮所有命中并跳到第一个，返回命中数。
pub fn find(q: &str) -> usize {
    with(|h| unsafe {
        h.find.clear();
        h.find_ix = 0;
        if q.trim().is_empty() {
            h.pdf.setHighlightedSelections(None);
            return 0;
        }
        let found = h.doc.findString_withOptions(&NSString::from_str(q), NSStringCompareOptions::CaseInsensitiveSearch);
        h.find = found.to_vec();
        h.pdf.setHighlightedSelections(if h.find.is_empty() { None } else { Some(&found) });
        if let Some(first) = h.find.first() {
            h.pdf.setCurrentSelection(Some(first));
            h.pdf.goToSelection(first);
        }
        h.find.len()
    })
    .unwrap_or(0)
}

/// 跳到下一个 / 上一个搜索结果，返回 (当前序号, 总数)。
pub fn find_step(d: i32) -> (usize, usize) {
    with(|h| unsafe {
        let n = h.find.len();
        if n == 0 {
            return (0, 0);
        }
        h.find_ix = (h.find_ix as i32 + d).rem_euclid(n as i32) as usize;
        let s = &h.find[h.find_ix];
        h.pdf.setCurrentSelection(Some(s));
        h.pdf.goToSelection(s);
        (h.find_ix, n)
    })
    .unwrap_or((0, 0))
}

/// 目录：(层级, 标题, 页)
pub fn outline() -> Vec<(usize, String, usize)> {
    fn walk(doc: &PDFDocument, node: &PDFOutline, depth: usize, out: &mut Vec<(usize, String, usize)>) {
        unsafe {
            for i in 0..node.numberOfChildren() {
                let Some(c) = node.childAtIndex(i) else { continue };
                let label = c.label().map(|s| s.to_string()).unwrap_or_default();
                let page = c.destination().and_then(|d| d.page()).map(|p: Retained<PDFPage>| doc.indexForPage(&p)).unwrap_or(0);
                out.push((depth, label, page));
                if out.len() < 2000 {
                    walk(doc, &c, depth + 1, out);
                }
            }
        }
    }
    with(|h| {
        let mut out = vec![];
        if let Some(root) = unsafe { h.doc.outlineRoot() } {
            walk(&h.doc, &root, 0, &mut out);
        }
        out
    })
    .unwrap_or_default()
}

/// 把当前文档（含显示中的批注）写成一份新 PDF。
pub fn write_to(path: &Path) -> bool {
    with(|h| unsafe { h.doc.writeToFile(&NSString::from_str(&path.to_string_lossy())) }).unwrap_or(false)
}
