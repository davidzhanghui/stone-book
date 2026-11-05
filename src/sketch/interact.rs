//! 草图鼠标交互。屏幕事件先换成世界坐标，再交给 press / drag_to / release，
//! 脚本测试（autotest）直接调用这三个函数。

use super::geom::{self, v, R, V};
use super::model::Element;
use super::model::*;
use super::view::*;
use gpui::*;

const SNAP_ANGLE: f64 = std::f64::consts::PI / 12.;

fn snap_angle(a: f64) -> f64 {
    (a / SNAP_ANGLE).round() * SNAP_ANGLE
}

fn norm_angle(a: f64) -> f64 {
    let t = std::f64::consts::TAU;
    let r = a.rem_euclid(t);
    if r.abs() < 1e-9 || (t - r).abs() < 1e-9 {
        0.
    } else {
        r
    }
}

impl SketchEditor {
    pub(super) fn mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        // 焦点由我们自己管：不让外层容器在冒泡阶段把焦点抢回去（否则新建文字后的输入框拿不到焦点）
        window.prevent_default();
        self.menu = None;
        self.popover = None;
        self.prompt = None;
        if let Some(target) = self.eyedropper.take() {
            let w = self.to_world(ev.position);
            self.pick_color_at(w, target, cx);
            return;
        }
        if ev.button == MouseButton::Middle || self.space_down || self.tool == Tool::Hand {
            self.drag = Some(Drag::Pan { start: ev.position, orig: self.offset });
            cx.notify();
            return;
        }
        let w = self.to_world(ev.position);
        self.press(w, ev.modifiers.shift, ev.modifiers.alt, ev.click_count, window, cx);
    }

    pub(super) fn right_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.finish_edit(cx);
        let w = self.to_world(ev.position);
        if let Some(id) = self.hit_at(w) {
            if !self.sel.contains(&id) {
                self.sel = self.select_target(&id).into_iter().collect();
            }
        }
        self.popover = None;
        self.menu = Some(ev.position);
        cx.notify();
    }

    pub(super) fn mouse_move(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.to_world(ev.position);
        self.mouse_world = w;
        if let Some(Drag::Pan { start, orig }) = &self.drag {
            let d = ev.position - *start;
            self.offset = crate::canvas::pt(orig.x + f32::from(d.x), orig.y + f32::from(d.y));
            cx.notify();
            return;
        }
        if self.drag.is_some() {
            if ev.pressed_button.is_none() {
                // 松开发生在窗口外：按松开处理
                self.release(w, ev.modifiers.shift, window, cx);
                return;
            }
            self.drag_to(w, ev.modifiers.shift, ev.modifiers.alt, cx);
            return;
        }
        if let Some(id) = self.multi.clone() {
            self.move_floating(&id, w, ev.modifiers.shift);
            cx.notify();
            return;
        }
        let h = if self.tool == Tool::Select { self.handle_at(w) } else { None };
        if h != self.hover_handle {
            self.hover_handle = h;
            cx.notify();
        }
    }

    pub(super) fn mouse_up(&mut self, ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.drag, Some(Drag::Pan { .. }) | Some(Drag::Slider)) {
            self.drag = None;
            cx.notify();
            return;
        }
        let w = self.to_world(ev.position);
        self.release(w, ev.modifiers.shift, window, cx);
    }

    pub(super) fn scroll(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = ev.delta.pixel_delta(px(20.));
        if ev.modifiers.platform || ev.modifiers.control {
            let factor = (1. - f32::from(delta.y) * 0.006).clamp(0.5, 2.);
            self.zoom_at(ev.position, factor);
        } else if ev.modifiers.shift && f32::from(delta.x).abs() < 0.01 {
            self.offset = crate::canvas::pt(self.offset.x + f32::from(delta.y), self.offset.y);
        } else {
            self.offset = crate::canvas::pt(self.offset.x + f32::from(delta.x), self.offset.y + f32::from(delta.y));
        }
        self.menu = None;
        cx.notify();
    }

    // ───────────── 手柄 ─────────────

    /// 选区手柄命中：0..8 缩放、8 旋转、100+i 线的第 i 个点
    pub(super) fn handle_at(&self, p: V) -> Option<usize> {
        let tol = 7. / self.zoom as f64;
        let pad = 4. / self.zoom as f64;
        if self.sel.is_empty() || self.editing.is_some() {
            return None;
        }
        if let Some(e) = self.single() {
            if e.locked {
                return None;
            }
            if self.crop_edit.as_deref() == Some(e.id.as_str()) {
                let q = geom::corners(e);
                return handles_of(&q).iter().position(|h| h.dist(p) <= tol).map(|i| 300 + i);
            }
            if e.is_linear() && e.pts().len() >= 2 {
                let pts = geom::world_points(e);
                let n = pts.len();
                if e.elbowed() {
                    return [0, n - 1].into_iter().find(|&i| pts[i].dist(p) <= tol).map(|i| 100 + i);
                }
                if n > 2 && self.line_edit.as_deref() != Some(e.id.as_str()) {
                    if let Some(i) = pts.iter().position(|q| q.dist(p) <= tol) {
                        return Some(100 + i);
                    }
                }
                if n == 2 || self.line_edit.as_deref() == Some(e.id.as_str()) {
                    if let Some(i) = pts.iter().position(|q| q.dist(p) <= tol) {
                        return Some(100 + i);
                    }
                    return geom::segment_mids(e).iter().position(|q| q.dist(p) <= tol).map(|i| 200 + i);
                }
            }
            if e.kind != "freedraw" && !e.is_linear() {
                let mut e2 = e.clone();
                e2.x -= pad;
                e2.y -= pad;
                e2.width += pad * 2.;
                e2.height += pad * 2.;
                let q = geom::corners(&e2);
                if rot_handle_of(&q, 18. / self.zoom as f64).dist(p) <= tol {
                    return Some(8);
                }
                return handles_of(&q).iter().position(|h| h.dist(p) <= tol);
            }
        }
        if self.selected().iter().all(|e| e.locked) {
            return None;
        }
        let r = self.selection_bounds()?.inflate(pad * if self.sel.len() == 1 { 1. } else { 2. });
        let q = [v(r.x0, r.y0), v(r.x1, r.y0), v(r.x1, r.y1), v(r.x0, r.y1)];
        if rot_handle_of(&q, 18. / self.zoom as f64).dist(p) <= tol {
            return Some(8);
        }
        handles_of(&q).iter().position(|h| h.dist(p) <= tol)
    }

    // ───────────── 按下 ─────────────

    pub fn press(&mut self, w: V, shift: bool, alt: bool, clicks: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_edit(cx);
        match self.tool {
            Tool::Select => self.press_select(w, shift, alt, clicks, window, cx),
            Tool::Hand => {}
            Tool::Rect | Tool::Diamond | Tool::Ellipse | Tool::Frame | Tool::Sticky | Tool::Embed => {
                self.snapshot();
                let p = self.snap(w);
                let mut e = Element::new(self.tool.kind().unwrap(), p.x, p.y);
                if self.tool != Tool::Frame {
                    self.apply_cur(&mut e);
                } else {
                    let n = self.doc.live().filter(|e| e.is_frame()).count() + 1;
                    e.set("name", serde_json::json!(format!("框架 {n}")));
                }
                let id = e.id.clone();
                self.doc.elements.push(e);
                self.sel.clear();
                self.drag = Some(Drag::Create { id, start: p });
            }
            Tool::Arrow | Tool::Line => {
                if let Some(mid) = self.multi.clone() {
                    let Some(e) = self.doc.get(&mid) else {
                        self.multi = None;
                        return;
                    };
                    let pts = geom::abs_points(e);
                    let n = pts.len();
                    let last_fixed = if n >= 2 { pts[n - 2] } else { pts[0] };
                    if clicks >= 2 || last_fixed.dist(w) < 6. / self.zoom as f64 {
                        self.finish_linear(&mid, cx);
                    } else {
                        // 浮动点落定，再加一个新的浮动点
                        let p = self.floating_target(&mid, w, shift);
                        if let Some(e) = self.doc.get_mut(&mid) {
                            let rel = [p.x - e.x, p.y - e.y];
                            if let Some(pts) = e.points.as_mut() {
                                if let Some(l) = pts.last_mut() {
                                    *l = rel;
                                }
                                pts.push(rel);
                            }
                        }
                    }
                    cx.notify();
                    return;
                }
                self.snapshot();
                let p = self.snap(w);
                let mut e = Element::new(self.tool.kind().unwrap(), p.x, p.y);
                self.apply_cur(&mut e);
                e.points = Some(vec![[0., 0.], [0., 0.]]);
                let id = e.id.clone();
                self.doc.elements.push(e);
                self.sel.clear();
                self.drag = Some(Drag::Linear { id, start: p });
            }
            Tool::Lasso => {
                if !shift {
                    self.sel.clear();
                }
                self.drag = Some(Drag::Lasso { pts: vec![w] });
            }
            Tool::Laser => {
                self.laser.push((w, std::time::Instant::now()));
                self.drag = Some(Drag::Laser);
                self.start_laser_anim(cx);
            }
            Tool::Bucket => self.bucket_at(w, cx),
            Tool::Draw | Tool::ShapeDraw => {
                self.snapshot();
                let mut e = Element::new("freedraw", w.x, w.y);
                self.apply_cur(&mut e);
                e.background_color = TRANSPARENT.into();
                let id = e.id.clone();
                self.doc.elements.push(e);
                self.sel.clear();
                self.drag = Some(Drag::Draw { id });
            }
            Tool::Text => {
                let container = self.hit_at(w).filter(|id| self.doc.get(id).is_some_and(|e| e.is_container() || e.is_linear()));
                match container {
                    Some(c) => self.edit_bound_text(&c, window, cx),
                    None => self.new_text_at(w, window, cx),
                }
                if !self.tool_lock {
                    self.tool = Tool::Select;
                }
            }
            Tool::Image => self.pick_image(Some(w), window, cx),
            Tool::Eraser => {
                self.drag = Some(Drag::Erase { last: w });
                self.erase_at(w, w);
            }
        }
        cx.notify();
    }

    fn press_select(&mut self, w: V, shift: bool, alt: bool, clicks: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.handle_at(w).filter(|_| !shift) {
            self.snapshot();
            let id = self.sel.iter().next().cloned().unwrap_or_default();
            if h >= 300 {
                if let Some(orig) = self.doc.get(&id).cloned() {
                    self.drag = Some(Drag::Crop { handle: h - 300, orig });
                }
                return;
            }
            if h >= 200 {
                // 拖段中点：在这里插入一个新节点并拖它（把线弯过去）
                self.bake_rotation(&id);
                let mids = self.doc.get(&id).map(geom::segment_mids).unwrap_or_default();
                let Some(m) = mids.get(h - 200).copied() else { return };
                if let Some(e) = self.doc.get_mut(&id) {
                    let rel = [m.x - e.x, m.y - e.y];
                    if let Some(p) = e.points.as_mut() {
                        p.insert(h - 200 + 1, rel);
                    }
                    e.bump();
                }
                self.line_pt = Some(h - 200 + 1);
                self.drag = Some(Drag::Point { id, idx: h - 200 + 1 });
                return;
            }
            if h >= 100 {
                self.bake_rotation(&id);
                let elbow = self.doc.get(&id).is_some_and(|e| e.elbowed());
                let n = self.doc.get(&id).map(|e| e.pts().len()).unwrap_or(0);
                if elbow {
                    // 折线箭头拖端点：先解开这一端，走线跟着鼠标重算
                    let start = h == 100;
                    let idx = if start { 0 } else { n.saturating_sub(1) };
                    if let Some(b) = self.doc.get(&id).and_then(|e| e.binding(start)) {
                        if let Some(t) = self.doc.get_mut(&b.element_id) {
                            t.remove_bound(&id);
                        }
                        if let Some(e) = self.doc.get_mut(&id) {
                            e.set_binding(start, None);
                        }
                    }
                    self.drag = Some(Drag::Point { id, idx });
                    return;
                }
                self.line_pt = Some(h - 100);
                self.drag = Some(Drag::Point { id, idx: h - 100 });
                return;
            }
            let orig: Vec<Element> = self.transform_set().into_iter().filter_map(|id| self.doc.get(&id).cloned()).collect();
            let bounds = self.selection_bounds().unwrap_or_default();
            let single = orig.len() == 1 && !orig[0].is_linear() && orig[0].kind != "freedraw";
            if h == 8 {
                let c = if single { geom::center(&orig[0]) } else { bounds.center() };
                let start = (w.y - c.y).atan2(w.x - c.x);
                self.drag = Some(Drag::Rotate { center: c, start, orig });
            } else {
                self.drag = Some(Drag::Resize { handle: h, orig, bounds, single });
            }
            return;
        }
        let hit = self.hit_at(w);
        // 点到别处：退出线编辑 / 裁剪
        if self.line_edit.is_some() && self.line_edit != hit {
            self.line_edit = None;
            self.line_pt = None;
        }
        if self.crop_edit.is_some() && self.crop_edit != hit {
            self.crop_edit = None;
        }
        match hit {
            Some(id) => {
                if clicks >= 2 {
                    self.double_click(&id, w, window, cx);
                    return;
                }
                let targets = self.select_target(&id);
                if shift {
                    let all_in = targets.iter().all(|t| self.sel.contains(t));
                    for t in targets {
                        if all_in {
                            self.sel.remove(&t);
                        } else {
                            self.sel.insert(t);
                        }
                    }
                } else if !targets.iter().all(|t| self.sel.contains(t)) {
                    self.sel = targets.into_iter().collect();
                }
                if self.sel.is_empty() {
                    return;
                }
                if alt {
                    // ⌥ 拖动：原位复制一份再拖副本
                    self.snapshot();
                    self.duplicate_offset(0., 0.);
                }
                let orig = self.move_set().into_iter().filter_map(|id| self.doc.get(&id).map(|e| (id.clone(), e.x, e.y))).collect();
                self.drag = Some(Drag::Move { start: w, orig, moved: alt });
            }
            None => {
                if clicks >= 2 {
                    self.new_text_at(w, window, cx);
                    return;
                }
                if !shift {
                    self.sel.clear();
                }
                self.drag = Some(Drag::Marquee { start: w, cur: w });
            }
        }
    }

    /// 双击：文字 → 编辑；容器 / 箭头 → 编辑（或新建）绑定文字；框架 → 改名称
    fn double_click(&mut self, id: &str, w: V, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.doc.get(id).cloned() else { return };
        if e.locked {
            return;
        }
        if e.is_text() {
            self.start_text_edit(id, window, cx);
        } else if e.kind == "line" {
            // 直线：双击进入线编辑（官方行为）；箭头：双击写标签
            self.line_edit = Some(id.to_string());
            self.line_pt = None;
            self.sel = [id.to_string()].into_iter().collect();
        } else if e.is_container() || e.is_linear() {
            self.edit_bound_text(id, window, cx);
        } else if e.kind == "image" {
            self.toggle_crop(cx);
        } else if matches!(e.kind.as_str(), "embeddable" | "iframe") {
            match e.link() {
                Some(l) => cx.open_url(l),
                None => self.open_prompt(PromptKind::Embed(id.to_string()), "", window, cx),
            }
        } else if e.is_frame() {
            self.start_frame_name_edit(id, window, cx);
        } else {
            self.new_text_at(w, window, cx);
        }
    }

    /// 移动时一起动的元素：选区 + 绑定文字 + 选中框架里的元素
    pub(super) fn move_set(&self) -> Vec<String> {
        self.sel_with_bound()
    }

    /// 缩放 / 旋转作用的元素：选区（绑定文字随容器重新排版）
    pub(super) fn transform_set(&self) -> Vec<String> {
        self.selected().iter().filter(|e| e.container_id().is_none() || !self.sel.contains(e.container_id().unwrap())).map(|e| e.id.clone()).collect()
    }

    /// 把线的旋转烘焙进点（拖动单个点前）
    fn bake_rotation(&mut self, id: &str) {
        let Some(e) = self.doc.get_mut(id) else { return };
        if e.angle == 0. || e.points.is_none() {
            return;
        }
        let pts = geom::world_points(e);
        e.angle = 0.;
        e.x = pts[0].x;
        e.y = pts[0].y;
        e.points = Some(pts.iter().map(|p| [p.x - pts[0].x, p.y - pts[0].y]).collect());
        e.normalize_points();
    }

    /// 浮动点位置：网格吸附、⇧ 15° 吸附、靠近目标时吸到目标
    fn floating_target(&self, id: &str, w: V, shift: bool) -> V {
        let Some(e) = self.doc.get(id) else { return w };
        let pts = geom::abs_points(e);
        let mut p = self.snap(w);
        if shift && pts.len() >= 2 {
            let prev = pts[pts.len() - 2];
            let d = p.sub(prev);
            let a = snap_angle(d.y.atan2(d.x));
            let l = d.len();
            p = prev.add(v(a.cos() * l, a.sin() * l));
        }
        p
    }

    fn move_floating(&mut self, id: &str, w: V, shift: bool) {
        let p = self.floating_target(id, w, shift);
        let is_arrow = self.doc.get(id).is_some_and(|e| e.kind == "arrow");
        self.bind_hint = if is_arrow { self.bind_target(p, id) } else { None };
        if let Some(e) = self.doc.get_mut(id) {
            let rel = [p.x - e.x, p.y - e.y];
            if let Some(l) = e.points.as_mut().and_then(|p| p.last_mut()) {
                *l = rel;
            }
        }
    }

    pub(super) fn bind_target(&self, p: V, except: &str) -> Option<String> {
        let tol = (geom::BIND_DIST / self.zoom as f64).max(4.);
        // 不绑定到自己的标签文字上
        geom::find_bindable(&self.doc, p, &[except], tol).map(|e| e.id.clone())
    }

    // ───────────── 拖动 ─────────────

    pub fn drag_to(&mut self, w: V, shift: bool, _alt: bool, cx: &mut Context<Self>) {
        let zoom = self.zoom as f64;
        let Some(drag) = self.drag.take() else { return };
        let drag = match drag {
            Drag::Move { start, orig, moved } => {
                if !moved && start.dist(w) * zoom < 3. {
                    Drag::Move { start, orig, moved }
                } else {
                    if !moved {
                        self.snapshot();
                        self.unbind_moving_arrows(&orig.iter().map(|(id, _, _)| id.clone()).collect::<Vec<_>>());
                    }
                    let mut d = w.sub(start);
                    if self.grid {
                        if let Some((_, x, y)) = orig.first() {
                            let s = self.snap(v(x + d.x, y + d.y));
                            d = v(s.x - x, s.y - y);
                        }
                    }
                    if shift {
                        // ⇧：只沿水平或垂直方向移动
                        if d.x.abs() > d.y.abs() {
                            d.y = 0.;
                        } else {
                            d.x = 0.;
                        }
                    }
                    let ids: Vec<String> = orig.iter().map(|(id, _, _)| id.clone()).collect();
                    for (id, x, y) in &orig {
                        if let Some(e) = self.doc.get_mut(id) {
                            e.x = x + d.x;
                            e.y = y + d.y;
                            e.bump();
                        }
                    }
                    geom::update_bound_arrows(&mut self.doc, &ids);
                    Drag::Move { start, orig, moved: true }
                }
            }
            Drag::Resize { handle, orig, bounds, single } => {
                self.resize(handle, &orig, bounds, single, w, shift);
                Drag::Resize { handle, orig, bounds, single }
            }
            Drag::Rotate { center, start, orig } => {
                let a = (w.y - center.y).atan2(w.x - center.x);
                let mut delta = a - start;
                if shift {
                    let base = if orig.len() == 1 { orig[0].angle } else { 0. };
                    delta = snap_angle(base + delta) - base;
                }
                let mut ids = vec![];
                for o in &orig {
                    let Some(e) = self.doc.get_mut(&o.id) else { continue };
                    let rc = geom::rot_center(o);
                    let nc = rc.rot(delta, center);
                    e.x = o.x + (nc.x - rc.x);
                    e.y = o.y + (nc.y - rc.y);
                    e.angle = norm_angle(o.angle + delta);
                    e.bump();
                    ids.push(o.id.clone());
                }
                for id in &ids {
                    geom::layout_bound_text(&mut self.doc, id);
                }
                geom::update_bound_arrows(&mut self.doc, &ids);
                Drag::Rotate { center, start, orig }
            }
            Drag::Create { id, start } => {
                let mut p = self.snap(w);
                // 便签默认等比（⇧ 放开）；其他形状 ⇧ 等比
                let sticky = self.doc.get(&id).is_some_and(|e| e.is_sticky());
                if shift != sticky {
                    let (dx, dy) = (p.x - start.x, p.y - start.y);
                    let s = dx.abs().max(dy.abs());
                    p = v(start.x + s * dx.signum(), start.y + s * dy.signum());
                }
                let r = R::from_pts([start, p]).unwrap();
                if let Some(e) = self.doc.get_mut(&id) {
                    e.x = r.x0;
                    e.y = r.y0;
                    e.width = r.w();
                    e.height = r.h();
                    e.bump();
                }
                Drag::Create { id, start }
            }
            Drag::Linear { id, start } => {
                let mut p = self.snap(w);
                if shift {
                    let d = p.sub(start);
                    let a = snap_angle(d.y.atan2(d.x));
                    p = start.add(v(a.cos() * d.len(), a.sin() * d.len()));
                }
                let is_arrow = self.doc.get(&id).is_some_and(|e| e.kind == "arrow");
                self.bind_hint = if is_arrow { self.bind_target(p, &id) } else { None };
                let elbow = self.doc.get(&id).is_some_and(|e| e.elbowed());
                if elbow {
                    // 折线箭头：起点（可能吸在形状边上）到鼠标走正交线
                    let st = self.doc.get(&id).and_then(|e| e.binding(true)).and_then(|b| self.doc.get(&b.element_id).cloned());
                    let (a, ha) = match &st {
                        Some(t) => {
                            let (q, h) = geom::elbow_side_point(t, p, 4.);
                            (q, Some(h))
                        }
                        None => (start, None),
                    };
                    let (b, hb) = match self.bind_hint.as_ref().and_then(|t| self.doc.get(t)) {
                        Some(t) => {
                            let (q, h) = geom::elbow_side_point(t, a, 4.);
                            (q, Some(h))
                        }
                        None => (p, None),
                    };
                    let route = geom::elbow_route(a, ha, b, hb);
                    if let Some(e) = self.doc.get_mut(&id) {
                        e.x = route[0].x;
                        e.y = route[0].y;
                        e.points = Some(route.iter().map(|q| [q.x - route[0].x, q.y - route[0].y]).collect());
                        e.normalize_points();
                        e.bump();
                    }
                } else if let Some(e) = self.doc.get_mut(&id) {
                    let rel = [p.x - e.x, p.y - e.y];
                    if let Some(l) = e.points.as_mut().and_then(|p| p.last_mut()) {
                        *l = rel;
                    }
                    e.bump();
                }
                Drag::Linear { id, start }
            }
            Drag::Point { id, idx } => {
                let p = self.snap(w);
                let (n, is_arrow) = self.doc.get(&id).map(|e| (e.pts().len(), e.kind == "arrow")).unwrap_or((0, false));
                let endpoint = idx == 0 || idx + 1 == n;
                self.bind_hint = if is_arrow && endpoint { self.bind_target(p, &id) } else { None };
                let elbow = self.doc.get(&id).is_some_and(|e| e.elbowed());
                if elbow {
                    let pts = self.doc.get(&id).map(geom::world_points).unwrap_or_default();
                    if pts.len() >= 2 {
                        let (a, b) = if idx == 0 { (p, pts[pts.len() - 1]) } else { (pts[0], p) };
                        let other = self.doc.get(&id).and_then(|e| e.binding(idx != 0)).and_then(|b| self.doc.get(&b.element_id).cloned());
                        let hint = self.bind_hint.as_ref().and_then(|t| self.doc.get(t)).cloned();
                        let side = |t: &Element, toward: V| {
                            let (q, h) = geom::elbow_side_point(t, toward, 4.);
                            (q, Some(h))
                        };
                        let (a, ha) = match (idx == 0, &hint, &other) {
                            (true, Some(t), _) => side(t, b),
                            (false, _, Some(t)) => side(t, b),
                            _ => (a, None),
                        };
                        let (b, hb) = match (idx == 0, &hint, &other) {
                            (false, Some(t), _) => side(t, a),
                            (true, _, Some(t)) => side(t, a),
                            _ => (b, None),
                        };
                        let route = geom::elbow_route(a, ha, b, hb);
                        if let Some(e) = self.doc.get_mut(&id) {
                            e.x = route[0].x;
                            e.y = route[0].y;
                            e.points = Some(route.iter().map(|q| [q.x - route[0].x, q.y - route[0].y]).collect());
                            e.normalize_points();
                            e.bump();
                        }
                    }
                } else if let Some(e) = self.doc.get_mut(&id) {
                    let mut pts = geom::abs_points(e);
                    if idx < pts.len() {
                        pts[idx] = p;
                        e.x = pts[0].x;
                        e.y = pts[0].y;
                        e.points = Some(pts.iter().map(|q| [q.x - pts[0].x, q.y - pts[0].y]).collect());
                        e.normalize_points();
                        e.bump();
                    }
                }
                geom::layout_bound_text(&mut self.doc, &id);
                // 以拖动后的末端为准：idx 可能因为折线重算而变
                let idx = if elbow {
                    if idx == 0 {
                        0
                    } else {
                        self.doc.get(&id).map(|e| e.pts().len().saturating_sub(1)).unwrap_or(idx)
                    }
                } else {
                    idx
                };
                Drag::Point { id, idx }
            }
            Drag::Marquee { start, .. } => Drag::Marquee { start, cur: w },
            Drag::Pan { start, orig } => Drag::Pan { start, orig },
            Drag::Draw { id } => {
                if let Some(e) = self.doc.get_mut(&id) {
                    let rel = [w.x - e.x, w.y - e.y];
                    let pts = e.points.get_or_insert_with(Vec::new);
                    let far = pts.last().is_none_or(|l| (l[0] - rel[0]).hypot(l[1] - rel[1]) * zoom > 1.2);
                    if far {
                        pts.push(rel);
                        e.bump();
                    }
                }
                Drag::Draw { id }
            }
            Drag::Erase { last } => {
                self.erase_at(last, w);
                Drag::Erase { last: w }
            }
            Drag::Lasso { mut pts } => {
                if pts.last().is_none_or(|l| l.dist(w) * zoom > 2.) {
                    pts.push(w);
                }
                Drag::Lasso { pts }
            }
            Drag::Laser => {
                self.laser.push((w, std::time::Instant::now()));
                Drag::Laser
            }
            Drag::Crop { handle, orig } => {
                self.crop_drag(handle, &orig, w);
                Drag::Crop { handle, orig }
            }
            Drag::Slider => Drag::Slider,
        };
        self.drag = Some(drag);
        cx.notify();
    }

    /// 拖动的线 / 箭头如果目标没有一起动，就解除绑定（官方行为）
    fn unbind_moving_arrows(&mut self, moving: &[String]) {
        let pairs: Vec<(String, bool, String)> = self
            .doc
            .live()
            .filter(|e| e.is_linear() && moving.contains(&e.id))
            .flat_map(|e| [true, false].into_iter().filter_map(move |s| e.binding(s).map(|b| (e.id.clone(), s, b.element_id))))
            .filter(|(_, _, t)| !moving.contains(t))
            .collect();
        for (id, start, t) in pairs {
            if let Some(e) = self.doc.get_mut(&id) {
                e.set_binding(start, None);
            }
            if let Some(te) = self.doc.get_mut(&t) {
                te.remove_bound(&id);
            }
        }
    }

    fn resize(&mut self, h: usize, orig: &[Element], b0: R, single: bool, w: V, shift: bool) {
        let mut ids = vec![];
        if single {
            let o = &orig[0];
            let a = o.angle;
            let c0 = geom::center(o);
            let q = w.rot(-a, c0);
            let (mut x0, mut y0, mut x1, mut y1) = (o.x, o.y, o.x + o.width, o.y + o.height);
            match h {
                0 => (x0, y0) = (q.x, q.y),
                1 => y0 = q.y,
                2 => (x1, y0) = (q.x, q.y),
                3 => x1 = q.x,
                4 => (x1, y1) = (q.x, q.y),
                5 => y1 = q.y,
                6 => (x0, y1) = (q.x, q.y),
                _ => x0 = q.x,
            }
            // 便签：拖角默认等比（⇧ 放开），拖边自由（⇧ 等比）
            let corner = matches!(h, 0 | 2 | 4 | 6);
            let keep = if o.is_sticky() { shift != corner } else { shift || o.kind == "image" || o.is_text() };
            if keep && o.width > 0. && o.height > 0. {
                let ratio = o.width / o.height;
                let (nw, nh) = ((x1 - x0).abs(), (y1 - y0).abs());
                let (nw, nh) = match h {
                    1 | 5 => (nh * ratio, nh),
                    3 | 7 => (nw, nw / ratio),
                    _ => {
                        if nw / ratio > nh {
                            (nw, nw / ratio)
                        } else {
                            (nh * ratio, nh)
                        }
                    }
                };
                // 以对角（或对边中点）为锚
                let (ax, ay) = match h {
                    0 => (o.x + o.width, o.y + o.height),
                    1 => (o.x + o.width / 2., o.y + o.height),
                    2 => (o.x, o.y + o.height),
                    3 => (o.x, o.y + o.height / 2.),
                    4 => (o.x, o.y),
                    5 => (o.x + o.width / 2., o.y),
                    6 => (o.x + o.width, o.y),
                    _ => (o.x + o.width, o.y + o.height / 2.),
                };
                let sx = if matches!(h, 0 | 6 | 7) { -1. } else { 1. };
                let sy = if matches!(h, 0..=2) { -1. } else { 1. };
                x0 = match h {
                    1 | 5 => ax - nw / 2.,
                    _ => ax,
                };
                x1 = match h {
                    1 | 5 => ax + nw / 2.,
                    _ => ax + sx * nw,
                };
                y0 = match h {
                    3 | 7 => ay - nh / 2.,
                    _ => ay,
                };
                y1 = match h {
                    3 | 7 => ay + nh / 2.,
                    _ => ay + sy * nh,
                };
            }
            let nr = R::from_pts([v(x0, y0), v(x1, y1)]).unwrap();
            let (nw, nh) = if o.is_sticky() { (nr.w().max(STICKY_MIN), nr.h().max(STICKY_MIN)) } else { (nr.w().max(1.), nr.h().max(1.)) };
            if o.is_sticky() {
                // 拖宽度不改用户高度；拖高度 / 等比时以新高度为基准；等比还按比例缩放字号上限
                let base0 = o.get("baseHeight").and_then(|v| v.as_f64()).filter(|h| *h > 0.).unwrap_or(o.height);
                let base = if matches!(h, 3 | 7) && !keep { base0 } else { nh };
                // 等比：字号上限跟着宽度按比例变（相对上一帧，逐帧连乘等于相对起点）
                let cur_w = self.doc.get(&o.id).map(|e| e.width).unwrap_or(o.width);
                let scale = if keep && cur_w > 0. { nw / cur_w } else { 1. };
                if let Some(te) = o.bound_text_id().and_then(|t| self.doc.get_mut(&t)) {
                    let f = te.get("baseFontSize").and_then(|v| v.as_f64()).unwrap_or(te.font_size_or());
                    te.set("baseFontSize", serde_json::json!((f * scale).clamp(4., 512.)));
                }
                if let Some(e) = self.doc.get_mut(&o.id) {
                    e.set("baseHeight", serde_json::json!(base));
                }
            }
            let wc = nr.center().rot(a, c0);
            if let Some(e) = self.doc.get_mut(&o.id) {
                e.x = wc.x - nw / 2.;
                e.y = wc.y - nh / 2.;
                e.width = nw;
                e.height = nh;
                if e.is_text() && o.height > 0. {
                    e.font_size = Some((o.font_size_or() * nh / o.height).max(4.));
                    geom::fit_text(e);
                    // 字号变了宽高会略有出入：保持中心
                    e.x = wc.x - e.width / 2.;
                    e.y = wc.y - e.height / 2.;
                }
                e.bump();
            }
            ids.push(o.id.clone());
        } else {
            let (mut x0, mut y0, mut x1, mut y1) = (b0.x0, b0.y0, b0.x1, b0.y1);
            match h {
                0 => (x0, y0) = (w.x, w.y),
                1 => y0 = w.y,
                2 => (x1, y0) = (w.x, w.y),
                3 => x1 = w.x,
                4 => (x1, y1) = (w.x, w.y),
                5 => y1 = w.y,
                6 => (x0, y1) = (w.x, w.y),
                _ => x0 = w.x,
            }
            let (bw, bh) = (b0.w().max(1e-6), b0.h().max(1e-6));
            let mut sx = (x1 - x0) / bw;
            let mut sy = (y1 - y0) / bh;
            if matches!(h, 1 | 5) {
                sx = if shift { sy.abs() } else { 1. };
            }
            if matches!(h, 3 | 7) {
                sy = if shift { sx.abs() } else { 1. };
            }
            if shift && matches!(h, 0 | 2 | 4 | 6) {
                let s = sx.abs().max(sy.abs());
                sx = s * sx.signum();
                sy = s * sy.signum();
            }
            if sx.abs() < 0.01 {
                sx = 0.01;
            }
            if sy.abs() < 0.01 {
                sy = 0.01;
            }
            // 锚点：被拖手柄的对侧
            let ax = if matches!(h, 0 | 6 | 7) { b0.x1 } else { b0.x0 };
            let ay = if matches!(h, 0..=2) { b0.y1 } else { b0.y0 };
            let (sx, sy) = (sx.abs(), sy.abs());
            let map = |p: V| v(ax + (p.x - ax) * sx, ay + (p.y - ay) * sy);
            for o in orig {
                let Some(e) = self.doc.get_mut(&o.id) else { continue };
                if o.points.is_some() && (o.is_linear() || o.kind == "freedraw") {
                    let p0 = map(v(o.x, o.y));
                    e.x = p0.x;
                    e.y = p0.y;
                    e.points = Some(o.pts().iter().map(|p| [p[0] * sx, p[1] * sy]).collect());
                    e.normalize_points();
                } else {
                    let c = map(geom::center(o));
                    e.width = (o.width * sx).max(1.);
                    e.height = (o.height * sy).max(1.);
                    if e.is_text() {
                        e.font_size = Some((o.font_size_or() * sx.min(sy)).max(4.));
                        geom::fit_text(e);
                    }
                    e.x = c.x - e.width / 2.;
                    e.y = c.y - e.height / 2.;
                }
                e.bump();
                ids.push(o.id.clone());
            }
        }
        for id in &ids {
            geom::layout_bound_text(&mut self.doc, id);
        }
        geom::update_bound_arrows(&mut self.doc, &ids);
    }

    fn erase_at(&mut self, a: V, b: V) {
        let tol = 6. / self.zoom as f64;
        let steps = ((a.dist(b) * self.zoom as f64) / 4.).ceil().max(1.) as usize;
        for i in 0..=steps {
            let p = a.add(b.sub(a).scale(i as f64 / steps as f64));
            let hits: Vec<String> = self.doc.live().filter(|e| !e.locked && geom::hit(e, p, tol)).map(|e| e.id.clone()).collect();
            for id in hits {
                let target = self.doc.get(&id).and_then(|e| e.container_id().map(|s| s.to_string())).unwrap_or(id);
                if !self.erase.contains(&target) {
                    self.erase.push(target);
                }
            }
        }
    }

    // ───────────── 松开 ─────────────

    pub fn release(&mut self, w: V, _shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else { return };
        match drag {
            Drag::Create { id, start } if self.doc.get(&id).is_some_and(|e| e.is_sticky() || e.kind == "embeddable") => {
                self.finish_special_create(&id, start, window, cx);
            }
            Drag::Create { id, .. } => {
                let tiny = self.doc.get(&id).is_none_or(|e| e.width < 2. && e.height < 2.);
                if tiny {
                    // 只是点了一下：不留空图形
                    if let Some(i) = self.doc.index(&id) {
                        self.doc.elements.remove(i);
                    }
                    self.undo.pop_back();
                } else {
                    if self.doc.get(&id).is_some_and(|e| e.is_frame()) {
                        self.adopt_children(&id);
                    } else {
                        self.update_frame_membership(std::slice::from_ref(&id));
                    }
                    self.sel = [id].into_iter().collect();
                    if !self.tool_lock {
                        self.tool = Tool::Select;
                    }
                    self.commit(cx);
                }
            }
            Drag::Linear { id, start } => {
                if start.dist(w) * self.zoom as f64 > 5. {
                    self.finish_linear(&id, cx);
                } else {
                    // 单击：进入逐点模式，后续每次单击加一个点，双击 / ↵ / Esc 结束
                    if let Some(e) = self.doc.get_mut(&id) {
                        e.points = Some(vec![[0., 0.], [w.x - e.x, w.y - e.y]]);
                    }
                    self.multi = Some(id);
                }
            }
            Drag::Point { id, idx } => {
                let n = self.doc.get(&id).map(|e| e.pts().len()).unwrap_or(0);
                if self.doc.get(&id).is_some_and(|e| e.kind == "arrow") && (idx == 0 || idx + 1 == n) {
                    self.rebind(&id, idx == 0);
                }
                if let Some(i) = self.doc.index(&id).filter(|&i| self.doc.elements[i].elbowed()) {
                    geom::reroute_elbow(&mut self.doc, i);
                }
                self.bind_hint = None;
                self.commit(cx);
            }
            Drag::Move { orig, moved, .. } => {
                if moved {
                    let ids: Vec<String> = orig.iter().map(|(id, _, _)| id.clone()).collect();
                    self.update_frame_membership(&ids);
                    self.commit(cx);
                }
            }
            Drag::Resize { .. } | Drag::Rotate { .. } => self.commit(cx),
            Drag::Marquee { start, cur } => {
                if let Some(r) = R::from_pts([start, cur]).filter(|r| r.w() > 2. || r.h() > 2.) {
                    let hits: Vec<String> = self
                        .doc
                        .live()
                        .filter(|e| !e.locked && e.container_id().is_none_or(|c| self.doc.get(c).is_none()))
                        .filter(|e| r.contains_r(&geom::bounds(e)))
                        .map(|e| e.id.clone())
                        .collect();
                    for id in hits {
                        for t in self.select_target(&id) {
                            self.sel.insert(t);
                        }
                    }
                }
            }
            Drag::Draw { id } => {
                if let Some(e) = self.doc.get_mut(&id) {
                    e.normalize_points();
                    let last = e.pts().last().copied();
                    e.set("lastCommittedPoint", last.map(|p| serde_json::json!(p)).unwrap_or(serde_json::Value::Null));
                }
                // 「画完识别形状」：手绘笔迹换成矩形 / 椭圆 / 菱形 / 直线 / 箭头
                let id = if self.tool == Tool::ShapeDraw { self.recognize_stroke(&id).unwrap_or(id) } else { id };
                self.update_frame_membership(std::slice::from_ref(&id));
                if self.tool == Tool::ShapeDraw && self.doc.get(&id).is_some_and(|e| e.kind != "freedraw") {
                    self.sel = [id].into_iter().collect();
                }
                self.commit(cx);
            }
            Drag::Lasso { pts } => self.lasso_select(&pts),
            Drag::Laser | Drag::Slider => {}
            Drag::Crop { .. } => self.commit(cx),
            Drag::Erase { .. } => {
                if !self.erase.is_empty() {
                    self.snapshot();
                    let ids = std::mem::take(&mut self.erase);
                    self.delete_ids(&ids);
                    self.commit(cx);
                }
            }
            Drag::Pan { .. } => {}
        }
        if self.multi.is_none() {
            self.bind_hint = None;
        }
        cx.notify();
    }

    /// 结束一条线 / 箭头：去掉浮动点、绑定两端、选中它。
    pub(super) fn finish_linear(&mut self, id: &str, cx: &mut Context<Self>) {
        let was_multi = self.multi.as_deref() == Some(id);
        self.multi = None;
        self.bind_hint = None;
        let Some(i) = self.doc.index(id) else { return };
        {
            let e = &mut self.doc.elements[i];
            if was_multi {
                if let Some(p) = e.points.as_mut() {
                    if p.len() > 2 {
                        p.pop();
                    }
                }
            }
            e.normalize_points();
        }
        let e = self.doc.elements[i].clone();
        let pts = geom::abs_points(&e);
        let len: f64 = pts.windows(2).map(|w| w[0].dist(w[1])).sum();
        if pts.len() < 2 || len < 2. {
            self.doc.elements.remove(i);
            self.undo.pop_back();
            cx.notify();
            return;
        }
        if e.kind == "arrow" {
            self.rebind(id, true);
            self.rebind(id, false);
        }
        self.update_frame_membership(&[id.to_string()]);
        self.sel = [id.to_string()].into_iter().collect();
        if !self.tool_lock {
            self.tool = Tool::Select;
        }
        self.commit(cx);
    }

    /// 按端点当前位置重新绑定（或解绑）。
    fn rebind(&mut self, id: &str, start: bool) {
        let Some(e) = self.doc.get(id).cloned() else { return };
        let pts = geom::world_points(&e);
        if pts.len() < 2 {
            return;
        }
        let p = if start { pts[0] } else { pts[pts.len() - 1] };
        let target = self.bind_target(p, id);
        // 两端绑同一个元素且只有两个点：不绑（官方也不允许自环直线）
        let other = e.binding(!start).map(|b| b.element_id);
        let target = target.filter(|t| !(pts.len() == 2 && other.as_deref() == Some(t.as_str())));
        if let Some(old) = e.binding(start) {
            if Some(&old.element_id) != target.as_ref() {
                if let Some(o) = self.doc.get_mut(&old.element_id) {
                    o.remove_bound(id);
                }
            }
        }
        if let Some(el) = self.doc.get_mut(id) {
            el.set_binding(start, target.as_deref());
        }
        if let Some(t) = &target {
            if let Some(te) = self.doc.get_mut(t) {
                te.add_bound(id, "arrow");
            }
            geom::update_bound_arrows(&mut self.doc, std::slice::from_ref(t));
        }
    }

    /// 新框架：完全在框里的元素归入
    fn adopt_children(&mut self, frame: &str) {
        let Some(f) = self.doc.get(frame).cloned() else { return };
        let r = R::xywh(f.x, f.y, f.width, f.height);
        for e in self.doc.elements.iter_mut().filter(|e| !e.is_deleted && !e.is_frame() && e.id != frame) {
            if r.contains_r(&geom::bounds(e)) {
                e.set("frameId", serde_json::json!(frame));
            }
        }
    }

    /// 元素移动后更新所属框架：中心落在哪个框架里就归哪个，拖出来就脱离。
    pub(super) fn update_frame_membership(&mut self, ids: &[String]) {
        let frames: Vec<(String, R)> = self.doc.live().filter(|e| e.is_frame()).map(|e| (e.id.clone(), R::xywh(e.x, e.y, e.width, e.height))).collect();
        let moving_frames: Vec<&String> = ids.iter().filter(|id| frames.iter().any(|(f, _)| f == *id)).collect();
        for id in ids {
            let Some(e) = self.doc.get(id) else { continue };
            if e.is_frame() || e.container_id().is_some() {
                continue;
            }
            // 随框架一起移动的子元素不变
            if e.frame_id().is_some_and(|f| moving_frames.iter().any(|m| m.as_str() == f)) {
                continue;
            }
            let c = geom::bounds(e).center();
            let target = frames.iter().rev().find(|(_, r)| r.contains(c)).map(|(f, _)| f.clone());
            let cur = e.frame_id().map(|s| s.to_string());
            if target != cur {
                let bound = e.bound_text_id();
                if let Some(e) = self.doc.get_mut(id) {
                    e.set("frameId", target.clone().map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
                }
                if let Some(t) = bound.and_then(|b| self.doc.get_mut(&b)) {
                    t.set("frameId", target.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
                }
            }
        }
    }
}
