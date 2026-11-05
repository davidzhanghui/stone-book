//! 附件全屏预览浮层（顶栏「× 退出 | 图标 文件名 / 大小 | 用其他应用打开 | 下载」，下方铺满预览内容）。
//! 电子表格的附件、多维表格的附件字段共用；内容复用文档「视频或文件」：图片查看器（缩放 / 旋转 / 适应页面）；
//! 视频 / 音频播放器；Markdown / 代码 / HTML / PDF / Office / webm 叠加原生交互预览；其余（svg 等、Quick Look 缩略图）显示静态预览。

use super::*;
use crate::base::widgets::primary_btn;
use crate::theme::Theme;
use crate::ui::icon;
use std::rc::Rc;

/// 宿主提供预览状态的存放处（播放器 / 原生预览视图 id、图片尺寸缓存）。
pub(crate) trait FileViewHost: ImgHost {
    fn fv_media(&mut self) -> &mut Option<u64>;
    fn image_sizes(&mut self) -> &mut HashMap<String, Option<(u32, u32)>>;
}

/// 打开预览前调用：视频打开即播放（同文档里的「放大预览」），清空上次的图片查看状态。
pub(crate) fn file_view_opening<V: FileViewHost>(this: &mut V, path: &str, name: &str) {
    if preview_kind(name) == Preview::Video {
        let id = crate::media::external_id(path);
        crate::media::pause_others(id);
        crate::media::with(id, path, |p| p.play());
    }
    this.img_views().clear();
}

/// 预览关闭后：停掉播放器、收起叠加的原生预览视图。
pub(crate) fn release_file_view<V: FileViewHost>(this: &mut V) {
    if let Some(id) = this.fv_media().take() {
        crate::media::drop_player(id);
        if crate::webview::live_block() == Some(id) {
            crate::webview::hide_live();
        }
    }
}

/// 盖住整个窗口的预览浮层。`on_close`：点「退出」；`on_download`：点「下载」（顶栏和图片工具栏共用）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn file_view_el<V: FileViewHost>(
    this: &mut V,
    name: &str,
    path: &str,
    size: u64,
    t: &Theme,
    window: &mut Window,
    cx: &mut Context<V>,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    on_download: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> AnyElement {
    let on_download = Rc::new(on_download);
    let vp = window.viewport_size();
    let doc_w = (f32::from(vp.width) - 80.).clamp(320., 900.);
    let msg =
        |s: &'static str| div().size_full().flex().items_center().justify_center().text_size(px(13.)).text_color(t.text_muted).child(s).into_any_element();
    let mid = crate::media::external_id(path);
    let exists = std::path::Path::new(path).exists();
    let kind = preview_kind(name);
    let live = if exists { file_live_src(path, name, t.dark) } else { None };
    // 换了文件：先释放上一个
    if this.fv_media().is_some_and(|id| id != mid) {
        release_file_view(this);
    }
    *this.fv_media() = Some(mid);
    let (area_w, area_h) = (f32::from(vp.width) - 48., f32::from(vp.height) - 56. - 48.);
    let is_video = exists && kind == Preview::Video;
    let live_bg = live.is_some();
    let raw =
        if exists && img_viewable(name) { *this.image_sizes().entry(path.to_string()).or_insert_with(|| image::image_dimensions(path).ok()) } else { None };
    let content: AnyElement = if let Some(raw) = raw {
        // 图片：与文档「视频或文件」的全屏预览同一个查看器（适应页面 / 缩放 / 旋转 / 原始尺寸 / 拖动）
        let (w, h) = (f32::from(vp.width), f32::from(vp.height) - 57.);
        let dl = on_download.clone();
        img_viewer(this, mid | IMG_FULL_KEY, path, Some(raw), w, h, true, t, cx, |_, _, _, _| false, move |this: &mut V, cx| dl(this, cx))
    } else if is_video {
        let (vw, vh) = video_meta(path, window).map(|m| (m.1 as f32, m.2 as f32)).unwrap_or((1280., 720.));
        let k = (area_w / vw.max(1.)).min(area_h / vh.max(1.));
        let player = video_player(mid, path, name, vw * k, vh * k, &PlayerOpts::default(), t, window);
        div().size_full().flex().items_center().justify_center().child(player).into_any_element()
    } else if exists && kind == Preview::Audio {
        let player = audio_player(mid, path, name, area_w.min(640.), false, None, None, t, cx);
        div().size_full().flex().items_center().justify_center().child(player).into_any_element()
    } else if let Some(src) = live {
        canvas(|_, _, _| (), move |b, _, window, _| crate::webview::show_live(window, mid, &src, b)).size_full().into_any_element()
    } else {
        match static_preview(path, name, doc_w, t.dark, cx) {
            StaticPreview::Image(p) => div().size_full().p(px(24.)).child(img(p).size_full().object_fit(ObjectFit::Contain)).into_any_element(),
            StaticPreview::Snap(p, w, h) => div()
                .id("fv-scroll")
                .size_full()
                .overflow_y_scroll()
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py(px(20.))
                        .gap(px(12.))
                        .child(div().w(px(w)).h(px(h)).flex_none().bg(t.bg).rounded(px(4.)).shadow_sm().overflow_hidden().child(img(p).size_full()))
                        .when(h >= STATIC_MAX_H - 1., |d| {
                            d.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.text_muted)
                                    .child(crate::i18n::tr("内容较长，仅预览前面部分；完整内容请「用其他应用打开」")),
                            )
                        }),
                )
                .into_any_element(),
            StaticPreview::Pending => msg("加载中…"),
            StaticPreview::Unsupported => {
                let p = path.to_string();
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .child(file_icon(name, 64.))
                    .child(div().text_size(px(15.)).child(name.to_string()))
                    .child(div().text_size(px(13.)).text_color(t.text_muted).child(if exists {
                        "暂不支持预览该类型的文件"
                    } else {
                        "文件不存在"
                    }))
                    .child(primary_btn("fv-ext", "用其他应用打开", t).on_click(move |_, _, cx| cx.open_with_system(std::path::Path::new(&p))))
                    .into_any_element()
            }
        }
    };
    let content_bg = if is_video {
        gpui::black()
    } else if live_bg {
        t.bg
    } else {
        t.code_bg
    };
    let head_btn = |id: &'static str, ic: &'static str, label: &'static str| {
        div()
            .id(id)
            .h(px(30.))
            .px(px(10.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(5.))
            .rounded(px(6.))
            .text_size(px(13.))
            .text_color(t.text)
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .child(icon(ic, 14., t.text_secondary))
            .child(crate::i18n::tr(label))
    };
    let path2 = path.to_string();
    let panel = div()
        .id("file-view")
        .w(vp.width)
        .h(vp.height)
        .occlude()
        .flex()
        .flex_col()
        .bg(t.bg)
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .h(px(56.))
                .flex_none()
                // 左侧让出透明标题栏的红绿灯
                .pl(px(80.))
                .pr(px(16.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.))
                .border_b_1()
                .border_color(t.border)
                .child(head_btn("fv-close", "x", "退出").on_click(cx.listener(move |this, _, w, cx| on_close(this, w, cx))))
                .child(div().w(px(1.)).h(px(20.)).bg(t.border))
                .child(file_icon(name, 28.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(14.)).whitespace_nowrap().overflow_hidden().text_ellipsis().child(name.to_string()))
                        .child(div().text_size(px(11.5)).text_color(t.text_muted).child(crate::convert::human_size(size))),
                )
                .child(head_btn("fv-open", "external", "用其他应用打开").on_click(move |_, _, cx| cx.open_with_system(std::path::Path::new(&path2))))
                .child(
                    head_btn("fv-dl", "download", "下载").border_1().border_color(t.border).on_click(cx.listener(move |this, _, _, cx| on_download(this, cx))),
                ),
        )
        .child(div().flex_1().min_h(px(0.)).bg(content_bg).child(content));
    deferred(anchored().position_mode(AnchoredPositionMode::Window).position(point(px(0.), px(0.))).child(panel)).with_priority(10).into_any_element()
}

/// 另存为（下载）附件，结果交给 `done(ok, 目标路径)`。
pub(crate) fn save_file_as<V: 'static>(
    path: &str,
    name: &str,
    cx: &mut Context<V>,
    done: impl FnOnce(&mut V, Option<std::path::PathBuf>, &mut Context<V>) + 'static,
) {
    let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
    let rx = cx.prompt_for_new_path(&dir, Some(name));
    let src = path.to_string();
    cx.spawn(async move |this: WeakEntity<V>, cx: &mut AsyncApp| {
        if let Ok(Ok(Some(dst))) = rx.await {
            let ok = std::fs::copy(&src, &dst).is_ok();
            this.update(cx, |this, cx| {
                done(this, ok.then_some(dst), cx);
                cx.notify();
            })
            .ok();
        }
    })
    .detach();
}
