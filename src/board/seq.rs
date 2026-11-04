//! 时序图交互（画板）：
//! - 生命线只有头部和虚线可以点中；选中时只框住头部，虚线末端的圆点拖动调整长度
//! - 悬停生命线虚线时显示激活条（竖条）预览，单击落下；激活条始终居中在虚线上，只能上下移动、拉伸
//! - 悬停激活条左右边时，在光标所在高度出现锚点，拖出水平的消息线；
//!   落在另一条生命线的虚线上时自动在落点新建激活条并连上，落在已有激活条上则直接连到它的边上

use super::model::*;
use super::view::*;
use crate::canvas::geom::{is_lifeline, life_dash_top};
use crate::canvas::{pt, rect, Pt, Rect, ShapeKind};
use gpui::*;

/// 新建激活条的宽度与默认高度。
pub const BAR_W: f32 = 16.;
pub const BAR_H: f32 = 80.;
/// 消息线落点在新建激活条顶部下方的距离。
const BAR_LEAD: f32 = 6.;

/// 拖动消息线时的落点。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MsgSnap {
    /// 连到已有激活条的边上
    Bar { id: u64, ax: f32, ay: f32 },
    /// 落在生命线虚线上：松开后在 y 处新建激活条
    Life { id: u64, y: f32 },
}

pub fn is_bar(e: &El) -> bool {
    matches!(e.kind, ElKind::Shape(ShapeKind::Activation))
}

pub fn is_life(e: &El) -> bool {
    matches!(e.kind, ElKind::Shape(k) if is_lifeline(k))
}

/// 生命线的虚线：(x, 起点 y, 终点 y)。
pub fn dash_of(e: &El) -> Option<(f32, f32, f32)> {
    match e.kind {
        ElKind::Shape(k) if is_lifeline(k) => Some((e.x + e.w / 2., life_dash_top(k, e.rect()), e.y + e.h)),
        _ => None,
    }
}

/// 生命线的头部（图标 + 名字 / 方框），选中框只框这一块。
pub fn head_rect(e: &El) -> Rect {
    match dash_of(e) {
        Some((_, top, _)) => rect(e.x, e.y, e.w, top - e.y),
        None => e.rect(),
    }
}

/// 生命线的命中区域：头部或虚线附近；非生命线返回 None。
pub fn life_contains(e: &El, p: Pt, tol: f32) -> Option<bool> {
    let (x, y0, y1) = dash_of(e)?;
    Some(head_rect(e).inflate(tol).contains(p) || ((p.x - x).abs() <= tol + 2. && p.y >= y0 && p.y <= y1 + tol))
}

/// 选中单个元素时的缩放手柄：(手柄序号, 位置, 是否画成圆点)。序号与 `Drag::Resize` 的 handle 一致。
pub fn handle_pts(e: &El) -> Vec<(usize, Pt, bool)> {
    if let Some((x, top, bottom)) = dash_of(e) {
        // 生命线：头部四角调整大小，虚线末端圆点调整长度
        return vec![
            (0, pt(e.x, e.y), false),
            (2, pt(e.x + e.w, e.y), false),
            (4, pt(e.x + e.w, top), false),
            (6, pt(e.x, top), false),
            (5, pt(x, bottom), true),
        ];
    }
    if is_bar(e) {
        // 激活条只能上下拉伸
        let cx = e.x + e.w / 2.;
        return vec![(1, pt(cx, e.y), false), (5, pt(cx, e.y + e.h), false)];
    }
    BoardEditor::handles(e.rect()).into_iter().enumerate().map(|(i, p)| (i, p, false)).collect()
}

impl BoardDoc {
    /// 激活条所在的生命线：激活条水平居中在虚线上，顶部落在虚线范围内。
    pub fn bar_host(&self, bar: &El) -> Option<&El> {
        if !is_bar(bar) {
            return None;
        }
        let c = bar.x + bar.w / 2.;
        self.els.iter().rev().find(|e| dash_of(e).is_some_and(|(x, y0, y1)| (x - c).abs() < 1.5 && bar.y >= y0 - 1. && bar.y <= y1))
    }

    /// 生命线上的激活条。
    pub fn bars_on(&self, life: u64) -> Vec<u64> {
        let Some(l) = self.get(life) else { return vec![] };
        let Some((x, y0, y1)) = dash_of(l) else { return vec![] };
        self.els.iter().filter(|e| is_bar(e) && (e.x + e.w / 2. - x).abs() < 1.5 && e.y >= y0 - 1. && e.y <= y1).map(|e| e.id).collect()
    }
}

impl BoardEditor {
    /// 光标所在的生命线虚线（不在激活条上、上面也没有压着别的元素）：(生命线, 预览激活条的顶部 y)。
    pub(super) fn dash_hover(&self, p: Pt) -> Option<(u64, f32)> {
        let id = self.hit(p)?;
        let e = self.doc.get(id).filter(|e| !e.locked)?;
        let (x, y0, y1) = dash_of(e)?;
        if (p.x - x).abs() * self.zoom > 6. || p.y < y0 || p.y > y1 {
            return None;
        }
        // 光标在预览条上方一点
        Some((id, (p.y - 10.).clamp(y0, (y1 - 20.).max(y0))))
    }

    /// 光标所在的激活条边缘锚点：(激活条, ax, ay)。ax 为 0（左边）或 1（右边），ay 跟随光标高度。
    pub(super) fn bar_port_at(&self, p: Pt) -> Option<(u64, f32, f32)> {
        let reach = 7. / self.zoom;
        let lk = self.doc.lookup();
        let e = self.doc.els.iter().rev().filter(|e| lk.visible(e.id)).find(|e| {
            let r = e.rect();
            p.y >= r.y && p.y <= r.bottom() && p.x >= r.x - reach && p.x <= r.right() + reach && !e.is_container()
        })?;
        if !is_bar(e) || e.locked {
            return None;
        }
        let r = e.rect();
        // 靠近哪条边就用哪条边；激活条中间留一段用来拖动激活条本身
        let inner = (3. / self.zoom).min(r.w / 4.);
        let ax = if p.x >= r.right() - inner {
            1.
        } else if p.x <= r.x + inner {
            0.
        } else {
            return None;
        };
        Some((e.id, ax, ((p.y - r.y) / r.h.max(1.)).clamp(0.02, 0.98)))
    }

    /// 连线某一端绑定的激活条及其锚点位置。
    pub(super) fn msg_end(&self, conn: u64, which: usize) -> Option<(u64, Pt)> {
        let ElKind::Connector { a, b, .. } = &self.doc.get(conn)?.kind else { return None };
        let End::Bound { el, ax, ay } = (if which == 0 { a } else { b }) else { return None };
        let e = self.doc.get(*el).filter(|e| is_bar(e))?;
        Some((e.id, pt(e.x + ax * e.w, e.y + ay * e.h)))
    }

    /// 消息线落点：q 附近的激活条（另一端所在的除外），其次是生命线虚线（另一端所在的生命线除外）。
    fn msg_target(&self, q: Pt, from_bar: u64, from: Pt) -> Option<MsgSnap> {
        let host = self.doc.get(from_bar).and_then(|b| self.doc.bar_host(b)).map(|l| l.id);
        let lk = self.doc.lookup();
        let reach = 12. / self.zoom;
        let bar = self.doc.els.iter().rev().filter(|e| is_bar(e) && e.id != from_bar && lk.visible(e.id)).find(|e| {
            let r = e.rect();
            let same_life = host.is_some() && self.doc.bar_host(e).map(|l| l.id) == host;
            !same_life && q.y >= r.y && q.y <= r.bottom() && q.x >= r.x - reach && q.x <= r.right() + reach
        });
        if let Some(e) = bar {
            let ax = if e.x + e.w / 2. >= from.x { 0. } else { 1. };
            // 不贴在上下两端：端点仍算在左右边上（换成直角连线时从侧边出线）
            return Some(MsgSnap::Bar { id: e.id, ax, ay: ((q.y - e.y) / e.h.max(1.)).clamp(0.02, 0.98) });
        }
        let life = self
            .doc
            .els
            .iter()
            .rev()
            .filter(|e| Some(e.id) != host && lk.visible(e.id) && !e.locked)
            .find(|e| dash_of(e).is_some_and(|(x, y0, y1)| (q.x - x).abs() * self.zoom < 16. && q.y >= y0 && q.y <= y1))?;
        Some(MsgSnap::Life { id: life.id, y: q.y })
    }

    /// 拖动消息线的一端（另一端连在激活条上）：保持水平，吸附到激活条或生命线虚线上。返回是否按消息线处理。
    pub(super) fn drag_msg_end(&mut self, id: u64, which: usize, p: Pt) -> bool {
        let Some((from_bar, from)) = self.msg_end(id, 1 - which) else { return false };
        let q = pt(p.x, from.y);
        self.msg_snap = self.msg_target(q, from_bar, from);
        self.snap_anchor = None;
        let end = match self.msg_snap {
            Some(MsgSnap::Bar { id: bar, ax, ay }) => End::Bound { el: bar, ax, ay },
            Some(MsgSnap::Life { id: life, y }) => {
                // 预览：线头停在将要新建的激活条的边上
                let x = self.doc.get(life).and_then(dash_of).map(|d| d.0).unwrap_or(q.x);
                End::Free { x: if x >= from.x { x - BAR_W / 2. } else { x + BAR_W / 2. }, y }
            }
            None => End::Free { x: q.x, y: q.y },
        };
        // 起点激活条从朝向落点的那条边出线
        let src = self.doc.get(from_bar).map(|e| e.x + e.w / 2.).unwrap_or(from.x);
        let side = if q.x >= src { 1. } else { 0. };
        if let Some(i) = self.doc.index(id) {
            if let ElKind::Connector { a, b, .. } = &mut self.doc.els[i].kind {
                if let End::Bound { ax, .. } = if which == 0 { &mut *b } else { &mut *a } {
                    *ax = side;
                }
                *(if which == 0 { a } else { b }) = end;
            }
        }
        true
    }

    /// 松开消息线：落在生命线上时新建激活条并连上。返回新建的激活条。
    pub(super) fn finish_msg(&mut self, id: u64, which: usize) -> Option<u64> {
        let snap = self.msg_snap.take()?;
        let MsgSnap::Life { id: life, y } = snap else { return None };
        let (from_bar, from) = self.msg_end(id, 1 - which)?;
        let (x, _, y1) = self.doc.get(life).and_then(dash_of)?;
        let top = y - BAR_LEAD;
        // 虚线不够长时把生命线拉长，放得下整根激活条
        if top + BAR_H > y1 {
            if let Some(i) = self.doc.index(life) {
                self.doc.els[i].h += top + BAR_H + 20. - y1;
            }
        }
        let src = self.doc.get(from_bar)?.clone();
        let mut bar = self.doc.new_el(ElKind::Shape(ShapeKind::Activation), rect(x - BAR_W / 2., top, BAR_W, BAR_H), false);
        // 样式与起点的激活条一致
        bar.stroke = src.stroke;
        bar.fill = src.fill;
        bar.stroke_w = src.stroke_w;
        bar.no_stroke = src.no_stroke;
        let bid = bar.id;
        let ax = if x >= from.x { 0. } else { 1. };
        if let Some(i) = self.doc.index(id) {
            if let ElKind::Connector { a, b, .. } = &mut self.doc.els[i].kind {
                *(if which == 0 { a } else { b }) = End::Bound { el: bid, ax, ay: BAR_LEAD / BAR_H };
            }
        }
        // 激活条放在连线下面，不压住箭头
        let at = self.doc.index(id).unwrap_or(self.doc.els.len());
        self.doc.els.insert(at, bar);
        Some(bid)
    }

    /// 在生命线上 top 处落一根激活条并选中。
    pub(super) fn place_bar(&mut self, life: u64, top: f32, cx: &mut Context<Self>) {
        let Some((x, _, y1)) = self.doc.get(life).and_then(dash_of) else { return };
        self.snapshot();
        let mut bar = self.doc.new_el(ElKind::Shape(ShapeKind::Activation), rect(x - BAR_W / 2., top, BAR_W, BAR_H.min(y1 - top).max(20.)), Self::dark(cx));
        self.style_new(&mut bar);
        let id = bar.id;
        self.doc.els.push(bar);
        self.sel = [id].into();
        self.bar_ghost = None;
        self.commit(cx);
    }

    /// 只拖动生命线上的激活条：只能上下移动，且不离开虚线。返回修正后的 dy；不是这种情况返回 None。
    pub(super) fn bar_move(&self, orig: &[(u64, f32, f32, Option<Vec<[f32; 3]>>, Option<(End, End)>)], dy: f32) -> Option<f32> {
        let mut dy = dy;
        let mut any = false;
        for (id, _, y, _, _) in orig {
            let e = self.doc.get(*id)?;
            if e.is_connector() {
                continue;
            }
            let mut probe = e.clone();
            probe.y = *y;
            let (_, y0, y1) = dash_of(self.doc.bar_host(&probe)?).unwrap();
            dy = dy.clamp(y0 - y, (y1 - 10. - y).max(y0 - y));
            any = true;
        }
        any.then_some(dy)
    }

    /// 预览激活条（悬停虚线 / 消息线将要落下的位置）。
    pub(super) fn ghost_bar(&self) -> Option<Rect> {
        let (life, top) = match (self.msg_snap, self.bar_ghost) {
            (Some(MsgSnap::Life { id, y }), _) => (id, y - BAR_LEAD),
            (_, Some(g)) if self.drag.is_none() => g,
            _ => return None,
        };
        let (x, _, y1) = self.doc.get(life).and_then(dash_of)?;
        let h = if self.msg_snap.is_some() { BAR_H } else { BAR_H.min(y1 - top).max(20.) };
        Some(rect(x - BAR_W / 2., top, BAR_W, h))
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_pts, head_rect, life_contains, BAR_H, BAR_W};
    use crate::board::model::{round_corners, BoardDoc, ElKind};
    use crate::canvas::{pt, rect, ShapeKind};

    fn doc() -> (BoardDoc, u64, u64) {
        let mut d = BoardDoc::new(false);
        let life = d.new_el(ElKind::Shape(ShapeKind::LifeBox), rect(0., 0., 110., 320.), false);
        let bar = d.new_el(ElKind::Shape(ShapeKind::Activation), rect(55. - BAR_W / 2., 100., BAR_W, BAR_H), false);
        let (l, b) = (life.id, bar.id);
        d.els.extend([life, bar]);
        (d, l, b)
    }

    #[test]
    fn bar_attaches_to_lifeline_dash() {
        let (mut d, l, b) = doc();
        assert_eq!(d.bars_on(l), vec![b]);
        assert_eq!(d.bar_host(d.get(b).unwrap()).map(|e| e.id), Some(l));
        // 偏离虚线就不再属于这条生命线
        d.els[1].x += 30.;
        assert!(d.bars_on(l).is_empty());
    }

    #[test]
    fn lifeline_hits_only_head_and_dash() {
        let (d, l, _) = doc();
        let e = d.get(l).unwrap();
        assert_eq!(life_contains(e, pt(20., 20.), 3.), Some(true));
        assert_eq!(life_contains(e, pt(55., 200.), 3.), Some(true));
        assert_eq!(life_contains(e, pt(20., 200.), 3.), Some(false));
        // 选中框只框头部，末端是调长度的圆点
        assert_eq!(head_rect(e).h, crate::canvas::geom::life_head(e.rect()));
        assert!(handle_pts(e).iter().any(|(i, p, round)| *i == 5 && *round && (p.y - 320.).abs() < 0.1));
    }

    #[test]
    fn round_elbow_keeps_ends_and_softens_corners() {
        let pts = vec![pt(0., 0.), pt(100., 0.), pt(100., 100.)];
        let r = round_corners(&pts, 10.);
        assert_eq!(r.first(), Some(&pt(0., 0.)));
        assert_eq!(r.last(), Some(&pt(100., 100.)));
        assert!(r.len() > 3);
        assert!(!r.contains(&pt(100., 0.)));
    }
}
