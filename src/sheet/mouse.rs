//! 鼠标交互：点击选择、拖选、拖动填充柄 / 行列边线、滚动。

use super::*;

impl SheetEditor {
    pub(super) fn mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.embedded {
            // 文档里的表格：别让文档的块选择抢走点击
            cx.stop_propagation();
        }
        self.pop = None;
        self.toast = None;
        let (lx, ly) = self.local(ev.position);
        let editing_formula = self.editing_text(cx).is_some_and(|t| t.starts_with('='));
        if !editing_formula {
            window.focus(&self.focus);
        }
        self.rebuild_geometry();
        // 左上角：全选
        if lx < HEAD_W && ly < HEAD_H {
            self.hide_marquee();
            self.select(Cell { r: 1, c: 1 }, Cell { r: 1, c: 1 }, SelKind::All, cx);
            return;
        }
        // 列头
        if ly < HEAD_H {
            let cell = self.cell_at_local(lx, ly + HEAD_H);
            if let Some((_, c)) = self.resize_hit(lx, ly) {
                if ev.click_count >= 2 {
                    self.autofit_col(c, cx);
                    return;
                }
                self.drag = Some(SDrag::ColResize { col: c, start: lx, orig: self.col_w(c) / self.zoom });
                cx.notify();
                return;
            }
            self.hide_marquee();
            let a = if ev.modifiers.shift { self.anchor } else { Cell { r: 1, c: cell.c } };
            self.select(a, Cell { r: 1, c: cell.c }, SelKind::Cols, cx);
            self.drag = Some(SDrag::Cols);
            return;
        }
        // 行头
        if lx < HEAD_W {
            let cell = self.cell_at_local(lx + HEAD_W, ly);
            if let Some((_, r)) = self.resize_hit(lx, ly) {
                self.drag = Some(SDrag::RowResize { row: r, start: ly, orig: self.row_h(r) / self.zoom });
                cx.notify();
                return;
            }
            self.hide_marquee();
            let a = if ev.modifiers.shift { self.anchor } else { Cell { r: cell.r, c: 1 } };
            self.select(a, Cell { r: cell.r, c: 1 }, SelKind::Rows, cx);
            self.drag = Some(SDrag::Rows);
            return;
        }
        let cell = self.cell_at_local(lx, ly);
        // 编辑公式时点选单元格插入引用
        if editing_formula {
            let t = self.editing_text(cx).unwrap_or_default();
            let keep = self.ref_pick.is_some_and(|(s, _)| s <= t.len() && funcs::parse_refs(&t).last().is_some_and(|r| r.5 == t.len() && r.4 == s));
            if funcs::expects_ref(&t) || keep {
                if !keep {
                    self.ref_pick = None;
                }
                self.pick_ref(cell, cell, cx);
                self.drag = Some(SDrag::RefPick);
                if let Some(e) = &self.editing {
                    window.focus(&e.input.focus_handle(cx));
                    // 根节点 track_focus 会在冒泡阶段把焦点抢回表格，阻止它
                    window.prevent_default();
                }
                return;
            }
            window.focus(&self.focus);
        }
        // 筛选按钮
        if let Some(f) = self.filter.as_ref().filter(|f| f.sheet == self.sheet) {
            if cell.r == f.head && cell.c >= f.c0 && cell.c <= f.c1 {
                let r = self.cell_rect(cell);
                if ev.position.x > r.right() - px(22.) {
                    self.commit_edit(cx);
                    self.pop = Some(Popup { kind: Pop::FilterCol(cell.c), at: point(r.right() - px(22.), r.bottom() + px(2.)) });
                    cx.notify();
                    return;
                }
            }
        }
        // 单元格对象：复选框、下拉箭头、日期按钮、链接、附件、图片、迷你图
        if !ev.modifiers.shift && self.painter.is_none() && self.obj_click(cell, ev.position, ev.click_count, ev.modifiers.platform, window, cx) {
            return;
        }
        // 填充柄
        let a = self.sel_area();
        if self.kind == SelKind::Cells {
            let sel = self.cell_rect(Cell { r: a.row + a.height - 1, c: a.column + a.width - 1 });
            let handle = point(sel.right(), sel.bottom());
            if (ev.position.x - handle.x).abs() < px(6.) && (ev.position.y - handle.y).abs() < px(6.) {
                self.commit_edit(cx);
                self.drag = Some(SDrag::Fill { to: self.cur });
                return;
            }
        }
        if ev.click_count >= 2 && self.painter.is_none() {
            self.select(cell, cell, SelKind::Cells, cx);
            self.start_edit(None, window, cx);
            // 根节点 track_focus 会在冒泡阶段把焦点抢回表格：输入法拿不到编辑框（候选框跑到窗口角落、字也进不去）
            window.prevent_default();
            return;
        }
        let a = if ev.modifiers.shift { self.anchor } else { cell };
        self.hide_marquee();
        self.select(a, cell, SelKind::Cells, cx);
        self.drag = Some(SDrag::Select);
    }

    pub(super) fn mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let (lx, ly) = self.local(ev.position);
        if ev.pressed_button != Some(MouseButton::Left) {
            // 悬停在列头 / 行头分隔线上：换成调整光标并高亮分隔线
            let hit = self.resize_hit(lx, ly);
            if hit != self.resize_hover {
                self.resize_hover = hit;
                cx.notify();
            }
            // 悬停在有备注的格子上显示备注
            let hc = (lx > HEAD_W && ly > HEAD_H).then(|| self.cell_at_local(lx, ly)).filter(|c| self.sx().cell(c.r, c.c).is_some_and(|x| !x.note.is_empty()));
            if hc != self.hover_cell {
                self.hover_cell = hc;
                cx.notify();
            }
            // 悬停在附件文件名上：浮出附件工具栏
            let hf = (lx > HEAD_W && ly > HEAD_H).then(|| self.cell_at_local(lx, ly)).and_then(|c| {
                let n = self.sx().cell(c.r, c.c).map(|x| x.files.len()).filter(|n| *n > 0)?;
                let r = self.cell_rect(c);
                let lh = 12. * self.zoom * 1.35;
                let top = r.top() + (r.size.height - px(lh * n as f32)) / 2.;
                let i = ((ev.position.y - top) / px(lh)).floor() as i64;
                (i >= 0 && (i as usize) < n).then_some((c, i as usize))
            });
            if hf != self.hover_file {
                self.hover_file = hf;
                cx.notify();
            }
            return;
        }
        // 拖到边缘时自动滚动
        let (w, h) = (f32::from(self.bounds.size.width), f32::from(self.bounds.size.height));
        if matches!(self.drag, Some(SDrag::Select) | Some(SDrag::Fill { .. }) | Some(SDrag::RefPick) | Some(SDrag::Rows) | Some(SDrag::Cols)) {
            if ly > h - 10. {
                self.scroll.1 += 20.;
            } else if ly < HEAD_H && !matches!(self.drag, Some(SDrag::Cols)) && self.scroll.1 > 0. {
                self.scroll.1 = (self.scroll.1 - 20.).max(0.);
            }
            if lx > w - 10. {
                self.scroll.0 += 20.;
            } else if lx < HEAD_W && !matches!(self.drag, Some(SDrag::Rows)) && self.scroll.0 > 0. {
                self.scroll.0 = (self.scroll.0 - 20.).max(0.);
            }
        }
        let cell = self.cell_at_local(lx.max(HEAD_W + 1.), ly.max(HEAD_H + 1.));
        match &mut self.drag {
            Some(SDrag::Select) => {
                if cell != self.cur {
                    self.cur = cell;
                    self.sync_formula(cx);
                    cx.notify();
                }
            }
            Some(SDrag::Rows) => {
                let c = Cell { r: cell.r, c: 1 };
                if c != self.cur {
                    self.cur = c;
                    self.sync_formula(cx);
                    cx.notify();
                }
            }
            Some(SDrag::Cols) => {
                let c = Cell { r: 1, c: cell.c };
                if c != self.cur {
                    self.cur = c;
                    self.sync_formula(cx);
                    cx.notify();
                }
            }
            Some(SDrag::RefPick) => {
                if let Some((_, a)) = self.ref_pick {
                    self.pick_ref(a, cell, cx);
                }
            }
            Some(SDrag::ColResize { col, start, orig }) => {
                let w = (*orig + (lx - *start) / self.zoom).max(8.);
                let col = *col;
                let (c0, c1) = if self.kind == SelKind::Cols && col >= self.sel_area().column && col < self.sel_area().column + self.sel_area().width {
                    self.sel_cols()
                } else {
                    (col, col)
                };
                self.model.borrow_mut().set_columns_width(self.sheet, c0, c1, w as f64).ok();
                // 只改 geo_key 时重建后的键与原来相同，单元格绘制缓存不会失效（文字停在旧位置）
                self.ver += 1;
                cx.notify();
            }
            Some(SDrag::RowResize { row, start, orig }) => {
                let h = (*orig + (ly - *start) / self.zoom).max(8.);
                let row = *row;
                let (r0, r1) = if self.kind == SelKind::Rows && row >= self.sel_area().row && row < self.sel_area().row + self.sel_area().height {
                    self.sel_rows()
                } else {
                    (row, row)
                };
                self.model.borrow_mut().set_rows_height(self.sheet, r0, r1, h as f64).ok();
                self.ver += 1;
                cx.notify();
            }
            Some(SDrag::Fill { .. }) => {
                self.drag = Some(SDrag::Fill { to: cell });
                cx.notify();
            }
            None => {}
        }
    }

    pub(super) fn mouse_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        match self.drag.take() {
            Some(SDrag::Fill { to }) => {
                let a = self.sel_area();
                // 只沿一个方向填充：取超出更多的方向
                let (over_r, over_c) = (to.r - (a.row + a.height - 1), to.c - (a.column + a.width - 1));
                if over_r > 0 && over_r >= over_c {
                    self.auto_fill(&a, Cell { r: to.r, c: a.column });
                    self.cur = Cell { r: to.r, c: a.column + a.width - 1 };
                } else if over_c > 0 {
                    self.auto_fill(&a, Cell { r: a.row, c: to.c });
                    self.cur = Cell { r: a.row + a.height - 1, c: to.c };
                }
                self.anchor = Cell { r: a.row, c: a.column };
                self.changed(cx);
            }
            Some(SDrag::ColResize { .. }) | Some(SDrag::RowResize { .. }) => self.changed(cx),
            Some(SDrag::RefPick) => {
                if let Some(e) = &self.editing {
                    window.focus(&e.input.focus_handle(cx));
                }
            }
            Some(SDrag::Select) | Some(SDrag::Rows) | Some(SDrag::Cols) => {
                if matches!(&self.dlg, Some(dialogs::Dlg::Spark(d)) if d.picking.is_some()) {
                    self.spark_pick_sync(cx);
                }
                // 格式刷：松手时把样式刷到选区（按源区域平铺）
                if let Some((styles, sticky)) = self.painter.take() {
                    // 普通选区整块平铺；整行 / 整列只刷到已用区域
                    let a = self.sel_used();
                    self.paste_styles(&a, &styles);
                    if sticky {
                        self.painter = Some((styles, true));
                    }
                    self.changed(cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn scroll_wheel(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let d = ev.delta.pixel_delta(px(24.));
        if self.embedded {
            // 内嵌表格：内容放得下时让文档自己滚
            let (w, h) = self.embed_content();
            let fits_x = w <= f32::from(self.bounds.size.width) + 1.;
            let fits_y = h <= f32::from(self.bounds.size.height) + 1.;
            let horiz = f32::from(d.x).abs() > f32::from(d.y).abs();
            if (horiz && fits_x) || (!horiz && fits_y) {
                return;
            }
            cx.stop_propagation();
        }
        if ev.modifiers.platform {
            // ⌘ + 滚轮缩放
            let z = (self.zoom * if f32::from(d.y) > 0. { 1.05 } else { 0.95 }).clamp(0.5, 3.);
            self.zoom = (z * 100.).round() / 100.;
            cx.notify();
            return;
        }
        self.scroll.0 = (self.scroll.0 - f32::from(d.x)).max(0.);
        self.scroll.1 = (self.scroll.1 - f32::from(d.y)).max(0.);
        cx.notify();
    }
}
