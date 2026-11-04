//! 画板里的表格与泳道（画板「表格」「泳道」）。
//!
//! 两者是同一种网格元素 `ElKind::Table`：
//! - 表格：单元格里写文字，首行可设为表头；
//! - 泳道：首行 / 首列是泳道名，其余单元格是放图形的区域，图形随泳道移动；可加标题栏。
//!
//! 交互：选中后再单击选中单元格（方向键切换、↵ 编辑、⌫ 清空），双击直接编辑；
//! 边线可拖动调整行高 / 列宽；边界上的「+」插入行 / 列；Tab 编辑下一个单元格（末尾自动加一行）。

use super::model::*;
use super::view::*;
use crate::canvas::{pt, rect, Col, Pt, Rect};
use gpui::*;

/// 表格上边 / 左边细轨道到表格边框的距离（屏幕像素）
const RAIL_OFF: f32 = 12.;

impl El {
    fn table_dims(&self) -> (usize, usize) {
        match &self.kind {
            ElKind::Table { cols, rows, .. } => (rows.len(), cols.len()),
            _ => (0, 0),
        }
    }

    /// 在第 at 行之前插入一行（at = 行数表示末尾）。
    pub fn table_insert_row(&mut self, at: usize, h: f32) {
        let (nr, nc) = self.table_dims();
        if let ElKind::Table { rows, cells, fills, fmts, .. } = &mut self.kind {
            let at = at.min(nr);
            fills.resize(nr * nc, 0);
            fmts.resize(nr * nc, CellFmt::default());
            rows.insert(at, h);
            for _ in 0..nc {
                cells.insert(at * nc, String::new());
                fills.insert(at * nc, 0);
                fmts.insert(at * nc, CellFmt::default());
            }
        }
        self.sync_grid_size();
    }

    pub fn table_insert_col(&mut self, at: usize, w: f32) {
        let (nr, nc) = self.table_dims();
        if let ElKind::Table { cols, cells, fills, fmts, .. } = &mut self.kind {
            let at = at.min(nc);
            fills.resize(nr * nc, 0);
            fmts.resize(nr * nc, CellFmt::default());
            cols.insert(at, w);
            for r in (0..nr).rev() {
                cells.insert(r * nc + at, String::new());
                fills.insert(r * nc + at, 0);
                fmts.insert(r * nc + at, CellFmt::default());
            }
        }
        self.sync_grid_size();
    }

    pub fn table_delete_row(&mut self, r: usize) {
        let (nr, nc) = self.table_dims();
        if nr <= 1 || r >= nr {
            return;
        }
        if let ElKind::Table { rows, cells, fills, fmts, .. } = &mut self.kind {
            fills.resize(nr * nc, 0);
            fmts.resize(nr * nc, CellFmt::default());
            rows.remove(r);
            cells.drain(r * nc..(r + 1) * nc);
            fills.drain(r * nc..(r + 1) * nc);
            fmts.drain(r * nc..(r + 1) * nc);
        }
        self.sync_grid_size();
    }

    pub fn table_delete_col(&mut self, c: usize) {
        let (nr, nc) = self.table_dims();
        if nc <= 1 || c >= nc {
            return;
        }
        if let ElKind::Table { cols, cells, fills, fmts, .. } = &mut self.kind {
            fills.resize(nr * nc, 0);
            fmts.resize(nr * nc, CellFmt::default());
            cols.remove(c);
            for r in (0..nr).rev() {
                cells.remove(r * nc + c);
                fills.remove(r * nc + c);
                fmts.remove(r * nc + c);
            }
        }
        self.sync_grid_size();
    }
}

impl BoardDoc {
    /// 新表格：首行为表头。
    pub fn new_table(&mut self, nrows: usize, ncols: usize, at: Pt, dark: bool) -> El {
        let (cw, ch) = (120., 40.);
        let mut el = self.new_el(
            ElKind::Table {
                cols: vec![cw; ncols],
                rows: vec![ch; nrows],
                cells: vec![String::new(); nrows * ncols],
                fills: vec![0; nrows * ncols],
                head_row: true,
                head_col: false,
                lane: false,
                fmts: vec![],
            },
            rect(at.x - cw * ncols as f32 / 2., at.y - ch * nrows as f32 / 2., cw * ncols as f32, ch * nrows as f32),
            dark,
        );
        el.font_size = 14.;
        el.stroke_w = 1.;
        el
    }

    /// 新泳道：0 垂直泳道、1 水平泳道、2 矩阵泳道（泳道 × 阶段）。
    pub fn new_lane(&mut self, kind: u8, at: Pt, dark: bool) -> El {
        let head = 44.;
        let (cols, rows, head_row, head_col): (Vec<f32>, Vec<f32>, bool, bool) = match kind {
            1 => (vec![head, 640.], vec![160.; 3], false, true),
            2 => (vec![head, 220., 220., 220.], vec![head, 160., 160., 160.], true, true),
            _ => (vec![220.; 3], vec![head, 480.], true, false),
        };
        let (nr, nc) = (rows.len(), cols.len());
        let mut cells = vec![String::new(); nr * nc];
        let mut lane_no = 0;
        let mut stage_no = 0;
        for i in 0..nr * nc {
            let (r, c) = (i / nc, i % nc);
            let in_head_row = head_row && r == 0;
            let in_head_col = head_col && c == 0;
            match (in_head_row, in_head_col) {
                (true, true) => {}
                (true, false) => {
                    lane_no += 1;
                    cells[i] = format!("泳道 {lane_no}");
                }
                (false, true) => {
                    stage_no += 1;
                    cells[i] = if kind == 2 { format!("阶段 {stage_no}") } else { format!("泳道 {stage_no}") };
                }
                _ => {}
            }
        }
        let (w, h): (f32, f32) = (cols.iter().sum(), rows.iter().sum());
        let mut el = self.new_el(
            ElKind::Table { cols, rows, cells, fills: vec![0; nr * nc], head_row, head_col, lane: true, fmts: vec![] },
            rect(at.x - w / 2., at.y - h / 2., w, h),
            dark,
        );
        el.font_size = 14.;
        el.stroke_w = 1.;
        el.fill = Some(if dark { Col::rgb(0x1f2023) } else { Col::rgb(0xffffff) });
        el
    }
}

impl BoardEditor {
    /// 选中的单个表格 / 泳道。
    pub(super) fn single_table(&self) -> Option<&El> {
        self.single().filter(|e| e.is_table())
    }

    /// 当前选中的单元格（行, 列）。
    pub(super) fn cur_cell(&self) -> Option<(u64, usize, usize)> {
        let (id, i) = self.cell_sel?;
        let e = self.single_table().filter(|e| e.id == id)?;
        let nc = e.grid()?.ncols();
        Some((id, i / nc, i % nc))
    }

    fn mutate_table(&mut self, id: u64, cx: &mut Context<Self>, f: impl FnOnce(&mut El)) {
        let Some(i) = self.doc.index(id) else { return };
        self.snapshot();
        f(&mut self.doc.els[i]);
        self.commit(cx);
    }

    /// 插入行 / 列：0 上方插入行、1 下方插入行、2 左侧插入列、3 右侧插入列（相对选中单元格，没有选中时加在末尾）。
    pub(super) fn table_insert(&mut self, how: u8, cx: &mut Context<Self>) {
        let Some(e) = self.single_table() else { return };
        let id = e.id;
        let g = e.grid().unwrap();
        let (r, c) = self.cur_cell().map(|x| (x.1, x.2)).unwrap_or((g.nrows() - 1, g.ncols() - 1));
        let (rh, cw) = (g.ys[r + 1] - g.ys[r], g.xs[c + 1] - g.xs[c]);
        self.popover = None;
        self.mutate_table(id, cx, |e| match how {
            0 => e.table_insert_row(r, rh),
            1 => e.table_insert_row(r + 1, rh),
            2 => e.table_insert_col(c, cw),
            _ => e.table_insert_col(c + 1, cw),
        });
        // 选中单元格跟着原来的内容走
        if let Some((_, i)) = self.cell_sel.as_mut() {
            let nc = g.ncols() + usize::from(how >= 2);
            *i = match how {
                0 => (r + 1) * nc + c,
                2 => r * nc + c + 1,
                1 => r * nc + c,
                _ => r * nc + c,
            };
        }
    }

    /// 在网格边界 k 处插入（边界上的「+」）。
    pub(super) fn table_insert_at(&mut self, id: u64, row: bool, k: usize, cx: &mut Context<Self>) {
        let Some(g) = self.doc.get(id).and_then(|e| e.grid()) else { return };
        self.cell_sel = None;
        if row {
            let src = k.min(g.nrows() - 1);
            let h = g.ys[src + 1] - g.ys[src];
            // 插在表头之前时，新行用正文行的高度
            let h = if k == 0 && g.nrows() > 1 { g.ys[2] - g.ys[1] } else { h };
            self.mutate_table(id, cx, |e| e.table_insert_row(k, h));
        } else {
            let src = if k >= g.ncols() { g.ncols() - 1 } else { k };
            let w = g.xs[src + 1] - g.xs[src];
            let w = if k == 0 && g.ncols() > 1 { g.xs[2] - g.xs[1] } else { w };
            self.mutate_table(id, cx, |e| e.table_insert_col(k, w));
        }
    }

    pub(super) fn table_delete(&mut self, row: bool, cx: &mut Context<Self>) {
        let Some((id, r, c)) = self.cur_cell() else { return };
        self.popover = None;
        self.cell_sel = None;
        self.mutate_table(id, cx, |e| if row { e.table_delete_row(r) } else { e.table_delete_col(c) });
    }

    pub(super) fn table_toggle_head(&mut self, row: bool, cx: &mut Context<Self>) {
        let Some(id) = self.single_table().map(|e| e.id) else { return };
        self.mutate_table(id, cx, |e| {
            if let ElKind::Table { head_row, head_col, .. } = &mut e.kind {
                if row {
                    *head_row = !*head_row;
                } else {
                    *head_col = !*head_col;
                }
            }
        });
    }

    /// 泳道图标题：没有时加上（默认「泳道图」并进入编辑），有时进入编辑。
    pub(super) fn lane_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.single_table().filter(|e| e.is_lane()) else { return };
        let id = e.id;
        if e.text.is_empty() {
            self.mutate_table(id, cx, |e| {
                e.text = "泳道图".into();
                e.y -= LANE_TITLE_H;
                e.h += LANE_TITLE_H;
            });
        }
        self.start_edit_part(id, Part::Main, window, cx);
    }

    /// 单元格底色（选中了单元格时只改该单元格，否则改整个表格的底色）。
    pub(super) fn set_cell_fill(&mut self, c: Option<Col>, cx: &mut Context<Self>) {
        match self.cell_sel {
            Some((id, i)) if self.single_table().is_some_and(|e| e.id == id) => self.mutate_table(id, cx, |e| {
                if let ElKind::Table { fills, cells, .. } = &mut e.kind {
                    fills.resize(cells.len(), 0);
                    if let Some(f) = fills.get_mut(i) {
                        *f = c.map(|c| c.0).unwrap_or(0);
                    }
                }
            }),
            _ => self.set_fill(c, cx),
        }
    }

    /// 方向键在单元格之间移动。返回 false 表示没有选中单元格（交给默认的微移）。
    pub(super) fn move_cell(&mut self, dx: i32, dy: i32, cx: &mut Context<Self>) -> bool {
        let Some((id, r, c)) = self.cur_cell() else { return false };
        let Some(g) = self.doc.get(id).and_then(|e| e.grid()) else { return false };
        let r = (r as i32 + dy).clamp(0, g.nrows() as i32 - 1) as usize;
        let c = (c as i32 + dx).clamp(0, g.ncols() as i32 - 1) as usize;
        self.cell_sel = Some((id, r * g.ncols() + c));
        cx.notify();
        true
    }

    /// 编辑单元格时按 Tab：编辑下一个单元格，最后一个单元格之后自动加一行（表格）。
    pub(super) fn table_tab(&mut self, id: u64, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_edit(cx);
        let Some(g) = self.doc.get(id).and_then(|e| e.grid()) else { return };
        let n = g.ncols() * g.nrows();
        if i + 1 >= n {
            let h = g.ys[g.nrows()] - g.ys[g.nrows() - 1];
            self.mutate_table(id, cx, |e| e.table_insert_row(g.nrows(), h));
        }
        self.cell_sel = Some((id, i + 1));
        self.start_edit_part(id, Part::Cell(i + 1), window, cx);
    }

    /// 选中表格时边界上的圆点（悬停变「+」）：(是否插入行, 边界序号, 世界坐标)。
    pub(super) fn table_plus(&self) -> Vec<(bool, usize, Pt)> {
        if self.editing.is_some() || self.drag.is_some() || self.tool != Tool::Select {
            return vec![];
        }
        let Some(e) = self.single_table().filter(|e| !e.locked) else { return vec![] };
        let Some(g) = e.grid() else { return vec![] };
        let off = RAIL_OFF / self.zoom;
        let mut v = vec![];
        for (k, x) in g.xs.iter().enumerate() {
            v.push((false, k, pt(*x, g.ys[0] - off)));
        }
        for (k, y) in g.ys.iter().enumerate() {
            v.push((true, k, pt(g.xs[0] - off, *y)));
        }
        v
    }

    /// 选中表格时上边 / 左边的细轨道，以及悬停「+」对应的边界线（世界坐标）。
    pub(super) fn table_rails(&self) -> (Vec<Rect>, Option<(Pt, Pt)>) {
        if self.table_plus().is_empty() {
            return (vec![], None);
        }
        let Some(g) = self.single_table().and_then(|e| e.grid()) else { return (vec![], None) };
        let (x0, x1, y0, y1) = (g.xs[0], g.xs[g.xs.len() - 1], g.ys[0], g.ys[g.ys.len() - 1]);
        let (off, w) = (RAIL_OFF / self.zoom, 6. / self.zoom);
        let rails = vec![rect(x0, y0 - off - w / 2., x1 - x0, w), rect(x0 - off - w / 2., y0, w, y1 - y0)];
        let hot =
            self.table_hot.and_then(|(row, k)| if row { g.ys.get(k).map(|y| (pt(x0, *y), pt(x1, *y))) } else { g.xs.get(k).map(|x| (pt(*x, y0), pt(*x, y1))) });
        (rails, hot)
    }

    /// 鼠标附近可以拖动的网格线：(表格, 是否竖线, 边界序号)。
    pub(super) fn grid_line_at(&self, p: Pt) -> Option<(u64, bool, usize)> {
        let tol = 4. / self.zoom;
        // 只在选中或悬停的表格上响应
        let cands = self.sel.iter().chain(self.hover.iter()).filter_map(|id| self.doc.get(*id)).filter(|e| e.is_table() && !e.locked);
        for e in cands {
            let Some(g) = e.grid() else { continue };
            let (y0, y1) = (g.ys[0], g.ys[g.ys.len() - 1]);
            let (x0, x1) = (g.xs[0], g.xs[g.xs.len() - 1]);
            if p.y >= y0 && p.y <= y1 {
                for (k, x) in g.xs.iter().enumerate().skip(1) {
                    if (p.x - x).abs() < tol {
                        return Some((e.id, true, k));
                    }
                }
            }
            if p.x >= x0 && p.x <= x1 {
                for (k, y) in g.ys.iter().enumerate().skip(1) {
                    if (p.y - y).abs() < tol {
                        return Some((e.id, false, k));
                    }
                }
            }
        }
        None
    }

    /// 拖动网格线：改变它左侧（上方）那一列（行）的尺寸，表格随之变大变小。
    pub(super) fn drag_grid_line(&mut self, id: u64, vertical: bool, k: usize, orig: f32, delta: f32) {
        let Some(i) = self.doc.index(id) else { return };
        let e = &mut self.doc.els[i];
        if let ElKind::Table { cols, rows, .. } = &mut e.kind {
            let v = if vertical { cols.get_mut(k - 1) } else { rows.get_mut(k - 1) };
            if let Some(v) = v {
                *v = (orig + delta).max(24.);
            }
        }
        e.sync_grid_size();
    }

    /// 插入表格（放在视口中心并选中）。
    pub(super) fn insert_table(&mut self, nrows: usize, ncols: usize, cx: &mut Context<Self>) {
        let at = self.to_world(self.canvas_bounds.center());
        self.snapshot();
        let el = self.doc.new_table(nrows, ncols, at, Self::dark(cx));
        self.sel = [el.id].into();
        self.cell_sel = Some((el.id, 0));
        self.doc.els.push(el);
        self.flyout = None;
        self.tool = Tool::Select;
        self.commit(cx);
    }
}

#[cfg(test)]
mod tests {
    // 别用 `super::*`：会把 gpui 的 `test` 宏引进作用域
    use super::super::model::{BoardDoc, ElKind, Part, LANE_TITLE_H};

    /// 测试用换行：每个字符宽 fs，按宽度折行
    fn lines(t: &str, fs: f32, _: bool, w: f32) -> usize {
        let per = (w / fs).floor().max(1.) as usize;
        t.split('\n').map(|p| p.chars().count().div_ceil(per).max(1)).sum()
    }

    #[test]
    fn cell_text_grows_row_and_shrinks_back() {
        let mut b = BoardDoc::new(false);
        let mut t = b.new_table(2, 3, pt(0., 0.), false);
        t.set_part_text(Part::Cell(3), "大幅度\n打卡咖啡店\n看到咖啡店".into());
        assert!(t.fit_text(Part::Cell(3), 40., &mut lines));
        let g = t.grid().unwrap();
        let rh = g.ys[2] - g.ys[1];
        assert!(rh >= 3. * 14. * 1.38, "行高 {rh}");
        assert!((t.h - 40. - rh).abs() < 0.1);
        // 删掉文字后缩回原来的行高
        t.set_part_text(Part::Cell(3), "短".into());
        t.fit_text(Part::Cell(3), 40., &mut lines);
        assert!((t.grid().unwrap().ys[2] - t.grid().unwrap().ys[1] - 40.).abs() < 0.1);
    }

    #[test]
    fn shape_and_class_compartment_grow() {
        use crate::canvas::ShapeKind;
        let mut b = BoardDoc::new(false);
        let mut s = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(0., 0., 120., 60.), false);
        s.text = "一\n二\n三\n四\n五".into();
        assert!(s.fit_text(Part::Main, 60., &mut lines));
        assert!(s.h >= 5. * s.font_size * 1.38);
        let mut c = b.new_el(ElKind::Shape(ShapeKind::ClassBox), rect(0., 0., 160., 120.), false);
        c.set_part_text(Part::Cell(1), "+a()\n+b()\n+c()\n+d()\n+e()".into());
        assert!(c.fit_text(Part::Cell(1), 120., &mut lines));
        let area = crate::canvas::geom::compartment_text_area(c.compartments()[1]);
        assert!(area.h + 0.5 >= 5. * c.font_size * 1.38);
        // 类图标题栏高度有上限：多行标题不会无限撑高
        let h = c.h;
        c.text = "一\n二\n三".into();
        c.fit_text(Part::Main, h, &mut lines);
        assert!((c.h - h).abs() < 0.1);
    }

    #[test]
    fn cell_fmt_follows_rows_and_cols() {
        let mut b = BoardDoc::new(false);
        let mut t = b.new_table(2, 2, pt(0., 0.), false);
        t.cell_fmt_mut(3, |f| f.font_size = Some(20.));
        assert_eq!(t.cell_font(3), 20.);
        assert_eq!(t.cell_font(2), t.font_size);
        t.table_insert_row(0, 40.);
        t.table_insert_col(0, 120.);
        assert_eq!(t.cell_font(8), 20.);
        t.table_delete_row(0);
        assert_eq!(t.cell_font(5), 20.);
        t.clear_cell_fmts(|f| f.font_size = None);
        assert!(matches!(&t.kind, ElKind::Table { fmts, .. } if fmts.is_empty()));
    }
    use crate::canvas::{pt, rect};

    #[test]
    fn insert_and_delete_rows_cols() {
        let mut b = BoardDoc::new(false);
        let mut t = b.new_table(2, 3, pt(0., 0.), false);
        t.set_part_text(Part::Cell(4), "中".into());
        t.table_insert_row(1, 40.);
        assert_eq!(t.part_text(Part::Cell(7)), "中");
        t.table_insert_col(0, 120.);
        assert_eq!(t.part_text(Part::Cell(10)), "中");
        assert!((t.w - 480.).abs() < 0.1 && (t.h - 120.).abs() < 0.1);
        t.table_delete_col(0);
        t.table_delete_row(1);
        assert_eq!(t.part_text(Part::Cell(4)), "中");
        let g = t.grid().unwrap();
        assert_eq!((g.nrows(), g.ncols()), (2, 3));
        assert_eq!(g.cell_at(pt(t.x + 130., t.y + 50.)), Some(4));
    }

    #[test]
    fn lane_children_follow_body() {
        let mut b = BoardDoc::new(false);
        let lane = b.new_lane(0, pt(0., 0.), false);
        let r = lane.rect();
        let inside = b.new_el(ElKind::Shape(crate::canvas::ShapeKind::Rect), rect(r.x + 20., r.y + 80., 100., 50.), false);
        let (lid, iid) = (lane.id, inside.id);
        b.els.extend([lane, inside]);
        assert_eq!(b.section_children(lid), vec![iid]);
        // 加标题栏后网格整体下移，尺寸随之更新
        let i = b.index(lid).unwrap();
        b.els[i].text = "标题".into();
        b.els[i].sync_grid_size();
        let g = b.els[i].grid().unwrap();
        assert!((g.ys[0] - b.els[i].y - LANE_TITLE_H).abs() < 0.1);
    }
}
