//! 草图里「更多工具」的实现：便签、油漆桶、套索、激光笔、画完识别形状、图片裁剪、
//! 吸管、浮动输入（链接 / 网址 / 十六进制颜色）、元素链接（跳到画布里的另一个元素）。

use super::geom::{self, v, R, V};
use super::model::Element;
use super::model::*;
use super::view::*;
use crate::line_input::{InputEvent, LineInput};
use gpui::*;
use serde_json::{json, Value};

impl SketchEditor {
    // ───────────── 便签 ─────────────

    /// 便签拖完（或只点了一下）：定尺寸，马上进入写字。
    pub(super) fn finish_special_create(&mut self, id: &str, start: V, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.doc.get(id).cloned() else { return };
        let tiny = e.width < 4. && e.height < 4.;
        let (w, h) = if tiny { (STICKY_SIZE, STICKY_SIZE) } else { (e.width.max(STICKY_MIN), e.height.max(STICKY_MIN)) };
        let (x, y) = if tiny { (start.x - w / 2., start.y - h / 2.) } else { (e.x, e.y) };
        if let Some(el) = self.doc.get_mut(id) {
            el.x = x;
            el.y = y;
            el.width = w;
            el.height = h;
            el.set("baseHeight", json!(h));
            el.bump();
        }
        self.update_frame_membership(&[id.to_string()]);
        self.sel = [id.to_string()].into_iter().collect();
        if !self.tool_lock {
            self.tool = Tool::Select;
        }
        self.edit_bound_text(id, window, cx);
        // 建便签和写第一行字算一步撤销
        self.undo.pop_back();
    }

    // ───────────── 颜色 ─────────────

    /// 改颜色：按便签规则（便签背景不透明、便签与它的文字同色）作用于选区，并记为当前颜色。
    pub(super) fn set_color(&mut self, target: PickTarget, c: &str, cx: &mut Context<Self>) {
        let sticky_ctx = if self.sel.is_empty() { self.tool == Tool::Sticky } else { self.selected().iter().all(|e| e.is_sticky()) };
        let c = c.to_string();
        let c2 = c.clone();
        match target {
            PickTarget::Stroke => self.set_prop(
                move |s| {
                    if sticky_ctx {
                        s.sticky_ink = c.clone()
                    } else {
                        s.stroke = c.clone()
                    }
                },
                move |e| {
                    if e.kind != "image" {
                        e.stroke_color = if e.is_sticky() && c2 == TRANSPARENT { DEFAULT_STROKE.into() } else { c2.clone() };
                    }
                },
                cx,
            ),
            PickTarget::Bg => self.set_prop(
                move |s| {
                    if sticky_ctx {
                        if c != TRANSPARENT {
                            s.sticky_bg = c.clone()
                        }
                    } else {
                        s.bg = c.clone()
                    }
                },
                move |e| {
                    if e.is_sticky() {
                        e.background_color = if c2 == TRANSPARENT { STICKY_BG.into() } else { c2.clone() };
                    } else if !e.is_text() && e.kind != "image" {
                        e.background_color = c2.clone();
                    }
                },
                cx,
            ),
        }
    }

    /// 吸管看到的颜色（对齐官网的「取像素」语义）：点到描边 / 文字取描边色，点到有填充的
    /// 内部取背景色，空白处取画布底色。图片取不到像素，跳过。
    pub(super) fn color_under(&self, w: V) -> String {
        let tol = 3. / self.zoom as f64;
        for e in self.doc.live().rev() {
            if e.kind == "image" || !geom::hit(e, w, tol) {
                continue;
            }
            if e.is_text() || e.is_linear() || e.kind == "freedraw" {
                return e.stroke_color.clone();
            }
            // 去掉填充再测一次：仍命中说明点在描边上
            let mut outline = e.clone();
            outline.background_color = TRANSPARENT.into();
            let on_stroke = geom::hit(&outline, w, tol);
            if e.has_fill() && !on_stroke {
                return e.background_color.clone();
            }
            if e.stroke_color != TRANSPARENT && !e.is_sticky() {
                return e.stroke_color.clone();
            }
            if e.has_fill() {
                return e.background_color.clone();
            }
        }
        self.doc.bg_color()
    }

    /// 吸管：把点到的颜色设给目标（颜色选择器保持打开，和官网一致）
    pub(super) fn pick_color_at(&mut self, w: V, target: PickTarget, cx: &mut Context<Self>) {
        let c = self.color_under(w);
        self.set_color(target, &c, cx);
        self.shade = None;
        self.popover = Some(Popover::Color(target));
        cx.notify();
    }

    /// 油漆桶：形状 / 便签 / 闭合线填背景色；文字和开放的线改描边色。
    pub(super) fn bucket_at(&mut self, w: V, cx: &mut Context<Self>) {
        // 没填充的形状点内部也算（官方油漆桶行为）
        let id = self
            .hit_at(w)
            .or_else(|| self.doc.live().rev().find(|e| !e.locked && (e.is_container() || e.is_frame()) && geom::inside_shape(e, w)).map(|e| e.id.clone()));
        let Some(id) = id else { return };
        let id = self.doc.get(&id).and_then(|e| e.container_id().map(|c| c.to_string())).unwrap_or(id);
        let Some(e) = self.doc.get(&id).cloned() else { return };
        let bg = self.cur.bg.clone();
        let closed = e.points.is_some() && {
            let p = geom::world_points(&e);
            p.len() > 2 && p[0].dist(*p.last().unwrap()) < 8.
        };
        self.snapshot();
        let Some(el) = self.doc.get_mut(&id) else { return };
        if el.is_sticky() {
            el.background_color = if bg == TRANSPARENT { STICKY_BG.into() } else { bg };
        } else if el.is_container() || closed || matches!(el.kind.as_str(), "embeddable" | "frame") {
            el.background_color = bg;
            if el.fill_style.is_empty() {
                el.fill_style = "solid".into();
            }
        } else if bg != TRANSPARENT && el.kind != "image" {
            el.stroke_color = bg;
        } else {
            self.undo.pop_back();
            return;
        }
        el.bump();
        self.commit(cx);
    }

    // ───────────── 套索 / 激光笔 ─────────────

    pub(super) fn lasso_select(&mut self, pts: &[V]) {
        if pts.len() < 3 {
            return;
        }
        let tol = 2. / self.zoom as f64;
        let ids: Vec<String> = self
            .doc
            .live()
            .filter(|e| !e.locked && e.container_id().is_none_or(|c| self.doc.get(c).is_none()))
            .filter(|e| {
                let r = geom::bounds(e);
                let enclosed = [v(r.x0, r.y0), v(r.x1, r.y0), v(r.x1, r.y1), v(r.x0, r.y1)].iter().all(|c| geom::inside_poly(*c, pts));
                enclosed || pts.iter().any(|p| geom::hit(e, *p, tol))
            })
            .map(|e| e.id.clone())
            .collect();
        for id in ids {
            for t in self.select_target(&id) {
                self.sel.insert(t);
            }
        }
    }

    /// 激光笔动画：轨迹点 1 秒后淡出，画面持续刷新直到轨迹消失。
    pub(super) fn start_laser_anim(&mut self, cx: &mut Context<Self>) {
        if self.laser_anim {
            return;
        }
        self.laser_anim = true;
        cx.spawn(async move |this: WeakEntity<SketchEditor>, cx: &mut AsyncApp| loop {
            cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
            let alive = this
                .update(cx, |this, cx| {
                    let now = std::time::Instant::now();
                    this.laser.retain(|(_, t)| now.duration_since(*t).as_millis() < LASER_MS);
                    cx.notify();
                    let alive = !this.laser.is_empty() || matches!(this.drag, Some(Drag::Laser));
                    if !alive {
                        this.laser_anim = false;
                    }
                    alive
                })
                .unwrap_or(false);
            if !alive {
                break;
            }
        })
        .detach();
    }

    // ───────────── 画完识别形状 ─────────────

    /// 把一笔手绘识别成形状：闭合 → 矩形 / 菱形 / 椭圆，开放且够直 → 直线。认不出来就保留手绘。
    pub(super) fn recognize_stroke(&mut self, id: &str) -> Option<String> {
        let e = self.doc.get(id)?.clone();
        let pts = geom::world_points(&e);
        if pts.len() < 4 {
            return None;
        }
        let r = R::from_pts(pts.iter().copied())?;
        let diag = r.w().hypot(r.h()).max(1.);
        let len: f64 = pts.windows(2).map(|w| w[0].dist(w[1])).sum();
        let (a, b) = (pts[0], *pts.last().unwrap());
        let kind = recognize(&pts, r, diag, len, a, b)?;
        let mut n = Element::new(kind, r.x0, r.y0);
        self.apply_cur(&mut n);
        n.stroke_color = e.stroke_color.clone();
        n.stroke_width = e.stroke_width;
        n.opacity = e.opacity;
        n.group_ids = e.group_ids.clone();
        if kind == "line" {
            n.x = a.x;
            n.y = a.y;
            n.points = Some(vec![[0., 0.], [b.x - a.x, b.y - a.y]]);
            n.normalize_points();
        } else {
            n.width = r.w().max(4.);
            n.height = r.h().max(4.);
        }
        let nid = n.id.clone();
        let i = self.doc.index(id)?;
        self.doc.elements[i] = n;
        Some(nid)
    }

    // ───────────── 图片裁剪 ─────────────

    pub(super) fn toggle_crop(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.single().filter(|e| e.kind == "image" && !e.locked).cloned() else { return };
        self.crop_edit = if self.crop_edit.as_deref() == Some(e.id.as_str()) { None } else { Some(e.id.clone()) };
        if self.crop_edit.is_some() {
            self.show_toast("拖动图片四周的手柄裁剪，Esc 或点空白处完成", cx);
        }
        cx.notify();
    }

    /// 原图尺寸（像素）与当前裁剪区（原图像素）
    pub(super) fn crop_of(&self, e: &Element) -> ([f64; 4], f64, f64) {
        let c = e.get("crop");
        let g = |k: &str| c.and_then(|c| c.get(k)).and_then(|v| v.as_f64());
        let natural = e.file_id().and_then(|f| self.doc.file_path(f)).and_then(|p| image::image_dimensions(p).ok()).map(|(w, h)| (w as f64, h as f64));
        let (nw, nh) = match (g("naturalWidth"), g("naturalHeight")) {
            (Some(w), Some(h)) => (w, h),
            _ => natural.unwrap_or((e.width.max(1.), e.height.max(1.))),
        };
        let crop = match (g("x"), g("y"), g("width"), g("height")) {
            (Some(x), Some(y), Some(w), Some(h)) if w > 0. && h > 0. => [x, y, w, h],
            _ => [0., 0., nw, nh],
        };
        (crop, nw, nh)
    }

    /// 整张原图在世界坐标里的四角（裁剪模式下画虚线框）
    pub(super) fn crop_full_quad(&self, e: &Element) -> [V; 4] {
        let ([cx0, cy0, cw, ch], nw, nh) = self.crop_of(e);
        let (sx, sy) = (e.width / cw, e.height / ch);
        let (fx0, fy0) = (e.x - cx0 * sx, e.y - cy0 * sy);
        let c = geom::center(e);
        [v(fx0, fy0), v(fx0 + nw * sx, fy0), v(fx0 + nw * sx, fy0 + nh * sy), v(fx0, fy0 + nh * sy)].map(|p| p.rot(e.angle, c))
    }

    pub(super) fn crop_drag(&mut self, handle: usize, orig: &Element, w: V) {
        let a = orig.angle;
        let c0 = geom::center(orig);
        let q = w.rot(-a, c0);
        let ([cx0, cy0, cw, ch], nw, nh) = self.crop_of(orig);
        let (sx, sy) = (orig.width / cw, orig.height / ch);
        let (fx0, fy0) = (orig.x - cx0 * sx, orig.y - cy0 * sy);
        let (fx1, fy1) = (fx0 + nw * sx, fy0 + nh * sy);
        let (mut x0, mut y0, mut x1, mut y1) = (orig.x, orig.y, orig.x + orig.width, orig.y + orig.height);
        match handle {
            0 => (x0, y0) = (q.x, q.y),
            1 => y0 = q.y,
            2 => (x1, y0) = (q.x, q.y),
            3 => x1 = q.x,
            4 => (x1, y1) = (q.x, q.y),
            5 => y1 = q.y,
            6 => (x0, y1) = (q.x, q.y),
            _ => x0 = q.x,
        }
        let min = 10.;
        x0 = x0.clamp(fx0, (x1 - min).max(fx0));
        y0 = y0.clamp(fy0, (y1 - min).max(fy0));
        x1 = x1.clamp((x0 + min).min(fx1), fx1);
        y1 = y1.clamp((y0 + min).min(fy1), fy1);
        let nc = v((x0 + x1) / 2., (y0 + y1) / 2.).rot(a, c0);
        let (w2, h2) = (x1 - x0, y1 - y0);
        let crop = json!({
            "x": (x0 - fx0) / sx, "y": (y0 - fy0) / sy, "width": w2 / sx, "height": h2 / sy,
            "naturalWidth": nw, "naturalHeight": nh,
        });
        if let Some(e) = self.doc.get_mut(&orig.id) {
            e.width = w2;
            e.height = h2;
            e.x = nc.x - w2 / 2.;
            e.y = nc.y - h2 / 2.;
            e.set("crop", crop);
            e.bump();
        }
        geom::update_bound_arrows(&mut self.doc, std::slice::from_ref(&orig.id));
    }

    // ───────────── 链接 ─────────────

    /// 「复制对象链接」的目标：单个元素用它自己的 id（绑定文字换成容器），
    /// 多选且同属一个最外层分组时用分组 id（与官网 getLinkIdAndTypeFromSelection 一致）
    pub(super) fn link_target_of_selection(&self) -> Option<String> {
        let sel = self.selected();
        match sel.as_slice() {
            [] => None,
            [e] => Some(self.doc.get(e.container_id().unwrap_or(&e.id)).map_or(e.id.clone(), |c| c.id.clone())),
            [first, ..] => {
                let g = first.group_ids.last()?;
                sel.iter().all(|e| e.group_ids.last() == Some(g)).then(|| g.clone())
            }
        }
    }

    pub(super) fn copy_element_link(&mut self, cx: &mut Context<Self>) {
        match self.link_target_of_selection() {
            Some(id) => {
                cx.write_to_clipboard(ClipboardItem::new_string(element_link(&id)));
                self.show_toast("已复制对象链接：给另一个元素设「链接」时粘贴进去，点击即跳到这里", cx);
            }
            None => self.show_toast("先选中单个元素或一个分组", cx),
        }
    }

    /// 点击链接图标：元素链接 → 画布定位并选中目标；其余当网址用浏览器打开
    pub(super) fn follow_link(&mut self, link: &str, cx: &mut Context<Self>) {
        if let Some(id) = element_link_id(link) {
            let hits: Vec<String> = self.doc.live().filter(|e| e.id == id || e.group_ids.iter().any(|g| g == id)).map(|e| e.id.clone()).collect();
            if hits.is_empty() {
                self.show_toast("链接指向的元素已不存在", cx);
                return;
            }
            let r = geom::bounds_of(self.doc.live().filter(|e| hits.contains(&e.id)));
            // 查看模式只定位不选中（没有选区可言）
            if !self.view_mode {
                self.sel = hits.iter().flat_map(|h| self.select_target(h)).collect();
                self.line_edit = None;
                self.crop_edit = None;
            }
            self.zoom_to(r, self.zoom.max(1.));
            cx.notify();
            return;
        }
        cx.open_url(&normalize_link(link));
    }

    /// 某点上最上层元素的链接（查看模式点击用；锁定的也算，绑定文字算它的容器）
    pub(super) fn link_at(&self, p: V) -> Option<String> {
        let tol = 6. / self.zoom as f64;
        let e = self.doc.live().rev().find(|e| geom::hit(e, p, tol))?;
        let e = e.container_id().and_then(|c| self.doc.get(c)).unwrap_or(e);
        e.link().filter(|l| !l.trim().is_empty()).map(|l| l.to_string())
    }

    /// 进出查看模式：进入时收起所有编辑状态
    pub(super) fn toggle_view_mode(&mut self, cx: &mut Context<Self>) {
        self.finish_edit(cx);
        if let Some(id) = self.multi.clone() {
            self.finish_linear(&id, cx);
        }
        self.view_mode = !self.view_mode;
        self.sel.clear();
        self.drag = None;
        self.line_edit = None;
        self.line_pt = None;
        self.crop_edit = None;
        self.popover = None;
        self.menu = None;
        self.prompt = None;
        self.eyedropper = None;
        self.hover_link = false;
        self.hover_move = false;
        self.hover_handle = None;
        self.bind_hint = None;
        self.show_toast(
            if self.view_mode {
                "已进入查看模式：拖动平移，点带链接的元素打开链接；⌥R 或 Esc 退出"
            } else {
                "已退出查看模式"
            },
            cx,
        );
    }

    // ───────────── 浮动输入 ─────────────

    pub(super) fn open_prompt(&mut self, kind: PromptKind, init: &str, window: &mut Window, cx: &mut Context<Self>) {
        let ph = match &kind {
            PromptKind::Link(_) => "网址，或粘贴右键「复制对象链接」得到的链接",
            PromptKind::Hex(_) => "十六进制颜色",
        };
        let input = cx.new(|cx| {
            let mut i = LineInput::new(ph, cx);
            i.set_text(init, cx);
            i
        });
        let k2 = kind.clone();
        let sub = cx.subscribe_in(&input, window, move |this, input, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => {
                let text = input.read(cx).text();
                this.apply_prompt(&k2, text.trim(), cx);
                this.prompt = None;
                window.focus(&this.focus);
                cx.notify();
            }
            InputEvent::Cancel => {
                this.prompt = None;
                window.focus(&this.focus);
                cx.notify();
            }
            _ => {}
        });
        window.focus(&input.focus_handle(cx));
        self.prompt = Some(InputPrompt { kind, input, _sub: sub });
        cx.notify();
    }

    pub(super) fn apply_prompt(&mut self, kind: &PromptKind, text: &str, cx: &mut Context<Self>) {
        match kind {
            PromptKind::Link(ids) => {
                self.snapshot();
                for id in ids {
                    if let Some(e) = self.doc.get_mut(id) {
                        e.set("link", if text.is_empty() { Value::Null } else { json!(text) });
                        e.bump();
                    }
                }
                self.commit(cx);
            }
            PromptKind::Hex(t) => {
                let h = text.trim().trim_start_matches('#');
                let valid = (h.len() == 3 || h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit());
                if valid {
                    self.set_color(*t, &format!("#{}", h.to_lowercase()), cx);
                } else if h.eq_ignore_ascii_case("transparent") {
                    self.set_color(*t, TRANSPARENT, cx);
                } else {
                    self.show_toast("不是有效的十六进制颜色", cx);
                }
            }
        }
    }

    // ───────────── Mermaid 至草图 ─────────────

    pub(super) fn open_mermaid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.popover = None;
        self.eyedropper = None;
        let sample = "flowchart LR\n  A[开始] --> B{是否通过?}\n  B -->|是| C[发布]\n  B -->|否| D[修改]\n  D --> A";
        let style =
            crate::text_area::TextStyle { font_size: 13., line_height: 19., color: crate::theme::theme(cx).text, bold: false, align: 0, v_align: 0, deco: 0 };
        let area = cx.new(|cx| crate::text_area::TextArea::new(sample, "粘贴 Mermaid 代码", style, cx));
        area.update(cx, |a, _| a.select_all_text());
        window.focus(&area.focus_handle(cx));
        let sub = cx.observe(&area, |this, _, cx| this.schedule_mermaid_preview(cx));
        self.mermaid = Some(MermaidDlg { area, error: None, preview: None, preview_src: String::new(), preview_gen: 0, _sub: sub });
        self.refresh_mermaid_preview(cx);
        cx.notify();
    }

    /// 输入停顿 250ms 后再刷新预览（非流程图要渲染 PNG，逐键重算太浪费）
    fn schedule_mermaid_preview(&mut self, cx: &mut Context<Self>) {
        let Some(dlg) = self.mermaid.as_mut() else { return };
        if dlg.area.read(cx).text() == dlg.preview_src {
            return;
        }
        dlg.preview_gen += 1;
        let g = dlg.preview_gen;
        cx.spawn(async move |this: WeakEntity<SketchEditor>, cx: &mut AsyncApp| {
            cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
            this.update(cx, |this, cx| {
                if this.mermaid.as_ref().is_some_and(|d| d.preview_gen == g) {
                    this.refresh_mermaid_preview(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn refresh_mermaid_preview(&mut self, cx: &mut Context<Self>) {
        let Some(dlg) = self.mermaid.as_mut() else { return };
        let src = dlg.area.read(cx).text().to_string();
        if src == dlg.preview_src && dlg.preview.is_some() {
            return;
        }
        dlg.preview = Some(if src.trim().is_empty() {
            MermaidPreview::Error(String::new())
        } else {
            match super::mermaid::convert(&src) {
                Ok(super::mermaid::Converted::Doc(d)) => MermaidPreview::Doc(std::sync::Arc::new(d)),
                Ok(super::mermaid::Converted::Png(png)) => {
                    // GPUI 按路径缓存图片：文件名带内容哈希
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    png.hash(&mut h);
                    let path = std::env::temp_dir().join(format!("stonebrook-mermaid-{:016x}.png", h.finish()));
                    match std::fs::write(&path, &png) {
                        Ok(()) => MermaidPreview::Image(path),
                        Err(e) => MermaidPreview::Error(format!("预览失败：{e}")),
                    }
                }
                Err(msg) => MermaidPreview::Error(msg),
            }
        });
        dlg.preview_src = src;
        dlg.error = None;
        cx.notify();
    }

    pub(super) fn insert_mermaid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dlg) = self.mermaid.as_mut() else { return };
        let src = dlg.area.read(cx).text().to_string();
        match super::mermaid::convert(&src) {
            Ok(conv) => {
                self.mermaid = None;
                let at = self.to_world(self.canvas_bounds.center());
                match conv {
                    super::mermaid::Converted::Doc(d) => {
                        // 放在已有内容右边，不压住别的东西；插入后把视图移过去
                        let at = match (geom::bounds_of(self.doc.live()), geom::bounds_of(d.live())) {
                            (Some(old), Some(new)) => v(old.x1 + 120. + new.w() / 2., old.center().y),
                            _ => at,
                        };
                        self.insert_doc(d, at, cx);
                        let r = self.selection_bounds();
                        self.zoom_to(r, 1.);
                    }
                    super::mermaid::Converted::Png(png) => match crate::convert::save_image_bytes(&png, "png", &super::ops::assets_dir()) {
                        Some(path) => self.insert_image(path, Some(at), cx),
                        None => self.show_toast("保存图片失败", cx),
                    },
                }
                window.focus(&self.focus);
            }
            Err(msg) => {
                dlg.error = Some(msg);
                cx.notify();
            }
        }
    }
}

pub(super) const LASER_MS: u128 = 1000;

/// 笔迹 → 形状种类
fn recognize(pts: &[V], r: R, diag: f64, len: f64, a: V, b: V) -> Option<&'static str> {
    let closed = a.dist(b) < diag * 0.2 && len > diag * 1.5;
    if !closed {
        // 够直：弦长接近路径长度
        return (a.dist(b) / len.max(1.) > 0.92 && len > 20.).then_some("line");
    }
    if r.w() < 10. || r.h() < 10. {
        return None;
    }
    // 闭合多边形面积 / 包围盒面积：矩形≈1，椭圆≈0.785，菱形≈0.5
    let mut area = 0.;
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        area += p.x * q.y - q.x * p.y;
    }
    let ratio = (area / 2.).abs() / (r.w() * r.h());
    // 经过包围盒角落附近的次数：矩形四角都会经过
    let near = r.w().min(r.h()) * 0.18;
    let corners = [v(r.x0, r.y0), v(r.x1, r.y0), v(r.x1, r.y1), v(r.x0, r.y1)];
    let hit_corners = corners.iter().filter(|c| pts.iter().any(|p| p.dist(**c) < near)).count();
    Some(if hit_corners >= 3 && ratio > 0.8 {
        "rectangle"
    } else if ratio < 0.64 {
        "diamond"
    } else {
        "ellipse"
    })
}

/// 元素链接：沿用官网格式 `?element=<id>`，在 excalidraw.com 打开同一文件也能跳转
pub(super) fn element_link(id: &str) -> String {
    format!("https://excalidraw.com/?element={id}")
}

/// 从链接里取出 `element=` 参数（不管域名，只要带这个参数就当元素链接）
pub(super) fn element_link_id(link: &str) -> Option<&str> {
    let query = link.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|kv| kv.strip_prefix("element=")).filter(|id| !id.is_empty())
}

/// 没写协议的网址补成 https（「example.com」→「https://example.com」）
pub(super) fn normalize_link(link: &str) -> String {
    let l = link.trim();
    if l.contains("://") || l.starts_with("mailto:") || l.starts_with("tel:") {
        l.to_string()
    } else {
        format!("https://{l}")
    }
}

#[cfg(test)]
mod tests {
    use super::recognize;
    use crate::sketch::geom::{v, R, V};

    fn ring(f: impl Fn(f64) -> V) -> Vec<V> {
        (0..=64).map(|i| f(i as f64 / 64. * std::f64::consts::TAU)).collect()
    }

    fn rec(pts: &[V]) -> Option<&'static str> {
        let r = R::from_pts(pts.iter().copied()).unwrap();
        let len: f64 = pts.windows(2).map(|w| w[0].dist(w[1])).sum();
        recognize(pts, r, r.w().hypot(r.h()), len, pts[0], *pts.last().unwrap())
    }

    #[test]
    fn recognizes_basic_shapes() {
        let ellipse = ring(|t| v(100. + 80. * t.cos(), 100. + 50. * t.sin()));
        assert_eq!(rec(&ellipse), Some("ellipse"));
        let mut rect = vec![];
        for (a, b) in [(v(0., 0.), v(200., 0.)), (v(200., 0.), v(200., 120.)), (v(200., 120.), v(0., 120.)), (v(0., 120.), v(0., 0.))] {
            for k in 0..16 {
                rect.push(a.add(b.sub(a).scale(k as f64 / 16.)));
            }
        }
        rect.push(v(0., 2.));
        assert_eq!(rec(&rect), Some("rectangle"));
        let mut dia = vec![];
        for (a, b) in [(v(100., 0.), v(200., 80.)), (v(200., 80.), v(100., 160.)), (v(100., 160.), v(0., 80.)), (v(0., 80.), v(100., 0.))] {
            for k in 0..16 {
                dia.push(a.add(b.sub(a).scale(k as f64 / 16.)));
            }
        }
        dia.push(v(101., 1.));
        assert_eq!(rec(&dia), Some("diamond"));
        let line: Vec<V> = (0..20).map(|i| v(i as f64 * 10., i as f64 * 3. + (i % 2) as f64)).collect();
        assert_eq!(rec(&line), Some("line"));
        let squiggle: Vec<V> = (0..40).map(|i| v(i as f64 * 5., ((i as f64) * 0.8).sin() * 40.)).collect();
        assert_eq!(rec(&squiggle), None);
    }
}
