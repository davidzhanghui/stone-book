//! 编辑类命令：选择性粘贴（公式 / 数据验证 / 转置）、插入 / 删除单元格并移位、截断、行高列宽与自适应、
//! 复制为图片、自定义数字格式。

use super::*;
use ironcalc::base::types::Style;

impl SheetEditor {
    pub(crate) fn run_edit(&mut self, head: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let a = self.sel_area();
        let (r0, c0, r1, c1) = (a.row, a.column, (a.row + a.height - 1).min(LAST_ROW), (a.column + a.width - 1).min(LAST_COL));
        match head {
            "paste-formulas" => self.paste_special("formulas", cx),
            "paste-dv" => self.paste_special("dv", cx),
            "paste-transpose" => self.paste_special("transpose", cx),
            "del-cells-left" | "del-cells-up" | "ins-cells-right" | "ins-cells-down" => self.shift_cells(head, cx),
            "clip" => {
                // 截断：去掉自动换行，记到扩展数据里
                self.batch(|this| {
                    this.set_align_raw(None, Some(false));
                    this.edit_sx(|x| x.set_clip(r0, c0, r1, c1, true));
                });
                self.changed(cx);
            }
            "row-height" | "col-width" => {
                let Some(v) = arg.trim().parse::<f64>().ok().filter(|v| *v >= 4. && *v <= 2000.) else { return true };
                {
                    let mut m = self.model.borrow_mut();
                    if head == "row-height" {
                        let r1 = if self.kind == SelKind::Cols || self.kind == SelKind::All { r0 } else { r1 };
                        m.set_rows_height(self.sheet, r0, r1, v).ok();
                    } else {
                        let c1 = if self.kind == SelKind::Rows || self.kind == SelKind::All { c0 } else { c1 };
                        m.set_columns_width(self.sheet, c0, c1, v).ok();
                    }
                }
                self.changed(cx);
            }
            "autofit-rows" => {
                let (ur, _) = self.used();
                let r1 = r1.min(ur.max(r0));
                self.batch(|this| {
                    for r in r0..=r1.min(r0 + 2000) {
                        this.fit_row(r);
                    }
                });
                self.changed(cx);
            }
            "autofit-cols" => {
                let (_, uc) = self.used();
                let c1 = c1.min(uc.max(c0));
                self.batch(|this| {
                    for c in c0..=c1.min(c0 + 500) {
                        this.autofit_col_raw(c);
                    }
                });
                self.changed(cx);
            }
            "copy-image" => self.copy_image(cx),
            "num-custom" => {
                let at = point(self.bounds.origin.x + px(200.), self.bounds.origin.y + px(40.));
                self.open_prompt(PromptKind::NumFmt, at, window, cx);
            }
            _ => return false,
        }
        true
    }

    /// 水平对齐 / 换行（不刷新，供批量操作用）。
    pub(crate) fn set_align_raw(&mut self, h: Option<&str>, wrap: Option<bool>) {
        use ironcalc::base::types::{Alignment, HorizontalAlignment as H, VerticalAlignment as V};
        let h = h.map(|h| match h {
            "left" => H::Left,
            "center" => H::Center,
            "right" => H::Right,
            _ => H::General,
        });
        let a = self.sel_used();
        let styles: Vec<Vec<Style>> = {
            let m = self.model.borrow();
            (a.row..a.row + a.height)
                .map(|r| {
                    (a.column..a.column + a.width)
                        .map(|c| {
                            let mut s = m.get_model().get_style_for_cell(self.sheet, r, c).unwrap_or_default();
                            let al = s.alignment.get_or_insert_with(|| Alignment { vertical: V::Center, ..Default::default() });
                            if let Some(h) = &h {
                                al.horizontal = h.clone();
                            }
                            if let Some(w) = wrap {
                                al.wrap_text = w;
                            }
                            s
                        })
                        .collect()
                })
                .collect()
        };
        self.paste_styles(&a, &styles);
    }

    /// 行高自适应：按该行内容（含自动换行）算出需要的高度，允许变矮。
    pub(crate) fn fit_row(&mut self, r: i32) {
        let need = {
            let m = self.model.borrow();
            let uc = m.get_model().workbook.worksheet(self.sheet).map(|w| w.dimension().max_column).unwrap_or(1);
            let mut need: f32 = 25.;
            for c in 1..=uc {
                let Ok(st) = m.get_cell_style(self.sheet, r, c) else { continue };
                let text = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let fpx = st.font.sz as f32 + 1.;
                let lines = if st.alignment.as_ref().is_some_and(|a| a.wrap_text) {
                    let w = (m.get_column_width(self.sheet, c).unwrap_or(90.) as f32 - 12.).max(10.);
                    text.split('\n').map(|l| (funcs::text_width_est(l, fpx) / w).ceil().max(1.)).sum::<f32>()
                } else {
                    1.
                };
                need = need.max(lines * fpx * 1.35 + 8.);
            }
            if self.sx().cell(r, 1).is_some() || self.sx().cells.iter().any(|x| x.r == r && !x.image.is_empty()) {
                need = need.max(44.);
            }
            need
        };
        self.model.borrow_mut().set_rows_height(self.sheet, r, r, need as f64).ok();
    }

    pub(crate) fn autofit_col_raw(&mut self, c: i32) {
        let w = {
            let m = self.model.borrow();
            let (ur, _) = self.used();
            let mut w: f32 = 40.;
            for r in 1..=ur.min(5000) {
                let t = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
                if t.is_empty() {
                    continue;
                }
                let sz = m.get_cell_style(self.sheet, r, c).map(|s| s.font.sz).unwrap_or(12) as f32 + 1.;
                w = w.max(funcs::text_width_est(&t, sz) + 16.);
            }
            w.min(600.)
        };
        self.model.borrow_mut().set_columns_width(self.sheet, c, c, w as f64).ok();
    }

    /// 选择性粘贴：仅公式（保留目标格式）/ 仅数据验证 / 转置。只对表格内复制的内容有效。
    fn paste_special(&mut self, mode: &str, cx: &mut Context<Self>) {
        let Some(clip) = self.clip.as_ref() else {
            self.toast = Some("请先在表格里复制要粘贴的内容".into());
            cx.notify();
            return;
        };
        let (sheet, range, data, src_sx) = (clip.sheet, clip.range, clip.data.clone(), clip.sx.clone());
        let cur = Cell { r: self.sel_area().row, c: self.sel_area().column };
        let (h, w) = (range.2 - range.0 + 1, range.3 - range.1 + 1);
        match mode {
            "dv" => {
                let (dr, dc) = (cur.r - range.0, cur.c - range.1);
                self.update_sx(
                    |x| {
                        x.clear_dv(cur.r, cur.c, cur.r + h - 1, cur.c + w - 1);
                        let only_dv = ext::SheetX { dv: src_sx.dv.clone(), ..Default::default() };
                        x.copy_block(&only_dv, range.0, range.1, range.2, range.3, dr, dc);
                    },
                    cx,
                );
            }
            "formulas" => {
                let Some(cd) = data.get("data").and_then(|d| serde_json::from_value::<ClipboardData>(d.clone()).ok()) else { return };
                let target = area(self.sheet, cur.r, cur.c, cur.r + h - 1, cur.c + w - 1);
                let old: Vec<Vec<Style>> = {
                    let m = self.model.borrow();
                    (0..h).map(|i| (0..w).map(|j| m.get_model().get_style_for_cell(self.sheet, cur.r + i, cur.c + j).unwrap_or_default()).collect()).collect()
                };
                self.batch(|this| {
                    {
                        let mut m = this.model.borrow_mut();
                        m.set_selected_sheet(this.sheet).ok();
                        m.set_selected_cell(cur.r, cur.c).ok();
                        m.paste_from_clipboard(sheet, range, &cd, false).ok();
                    }
                    this.paste_styles(&target, &old);
                });
                self.changed(cx);
            }
            _ => {
                // 转置：内容按原样（公式不平移）写到行列互换的位置，样式一起转置
                let cell = |r: i32, c: i32, k: &str| data.pointer(&format!("/data/{r}/{c}/{k}")).cloned();
                let styles: Vec<Vec<Style>> = (0..w)
                    .map(|j| (0..h).map(|i| cell(range.0 + i, range.1 + j, "style").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()).collect())
                    .collect();
                self.batch(|this| {
                    {
                        let mut m = this.model.borrow_mut();
                        for i in 0..h {
                            for j in 0..w {
                                let text = cell(range.0 + i, range.1 + j, "text").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
                                m.set_user_input(this.sheet, cur.r + j, cur.c + i, &text).ok();
                            }
                        }
                    }
                    this.paste_styles(&area(this.sheet, cur.r, cur.c, cur.r + w - 1, cur.c + h - 1), &styles);
                });
                self.anchor = cur;
                self.cur = Cell { r: cur.r + w - 1, c: cur.c + h - 1 };
                self.changed(cx);
            }
        }
        self.sync_formula(cx);
    }

    /// 插入 / 删除单元格并让右侧（或下方）单元格移位：用 IronCalc 的剪切粘贴搬动，公式引用随之更新。
    fn shift_cells(&mut self, what: &str, cx: &mut Context<Self>) {
        let a = self.sel_used();
        let (r0, c0, r1, c1) = (a.row, a.column, a.row + a.height - 1, a.column + a.width - 1);
        let (ur, uc) = self.used();
        let (w, h) = (c1 - c0 + 1, r1 - r0 + 1);
        // (源区域, 目标左上角, 要清空的区域)
        let (src, dst, clear) = match what {
            "del-cells-left" => ((r0, c1 + 1, r1, uc.max(c1 + 1)), (r0, c0), (r0, (uc.max(c1) - w + 1).max(c0), r1, uc.max(c1))),
            "del-cells-up" => ((r1 + 1, c0, ur.max(r1 + 1), c1), (r0, c0), ((ur.max(r1) - h + 1).max(r0), c0, ur.max(r1), c1)),
            "ins-cells-right" => ((r0, c0, r1, uc.max(c0)), (r0, c0 + w), (r0, c0, r1, c1)),
            _ => ((r0, c0, ur.max(r0), c1), (r0 + h, c0), (r0, c0, r1, c1)),
        };
        self.batch(|this| {
            let s = this.sheet;
            let has_src = src.0 <= src.2 && src.1 <= src.3 && !(what.starts_with("del") && (src.1 > uc || src.0 > ur));
            if has_src {
                let data = {
                    let mut m = this.model.borrow_mut();
                    m.set_selected_sheet(s).ok();
                    m.set_selected_cell(src.0, src.1).ok();
                    m.set_selected_range(src.0, src.1, src.2, src.3).ok();
                    m.copy_to_clipboard()
                        .ok()
                        .and_then(|c| serde_json::to_value(&c).ok())
                        .and_then(|v| v.get("data").cloned())
                        .and_then(|d| serde_json::from_value::<ClipboardData>(d).ok())
                };
                if let Some(data) = data {
                    let mut m = this.model.borrow_mut();
                    m.set_selected_cell(dst.0, dst.1).ok();
                    m.paste_from_clipboard(s, (src.0, src.1, src.2, src.3), &data, true).ok();
                }
            }
            // 插入：腾出的区域清空；删除：末尾空出来的一截清空（不会和搬过去的数据重叠）
            this.model.borrow_mut().range_clear_all(&area(s, clear.0, clear.1, clear.2, clear.3)).ok();
            // 单元格对象跟着移
            let (dr, dc) = (dst.0 - src.0, dst.1 - src.1);
            this.edit_sx(|x| {
                let inside = |r: i32, c: i32| r >= src.0 && r <= src.2 && c >= src.1 && c <= src.3;
                let removed = |r: i32, c: i32| r >= r0 && r <= r1 && c >= c0 && c <= c1;
                if what.starts_with("del") {
                    x.cells.retain(|o| !removed(o.r, o.c));
                    x.clear_dv(r0, c0, r1, c1);
                }
                for o in x.cells.iter_mut().filter(|o| inside(o.r, o.c)) {
                    o.r += dr;
                    o.c += dc;
                }
                for d in x.dv.iter_mut().filter(|d| inside(d.r0, d.c0) && inside(d.r1, d.c1)) {
                    d.r0 += dr;
                    d.r1 += dr;
                    d.c0 += dc;
                    d.c1 += dc;
                }
            });
        });
        self.anchor = Cell { r: r0, c: c0 };
        self.cur = self.anchor;
        self.kind = SelKind::Cells;
        self.changed(cx);
        self.sync_formula(cx);
    }

    /// 复制为图片：把选区画成 SVG（填充、文字、边框、网格线），栅格化为 PNG 放进剪贴板。
    fn copy_image(&mut self, cx: &mut Context<Self>) {
        let a = self.sel_used();
        let (rows, cols) = (a.height.min(200), a.width.min(50));
        let m = self.model.borrow();
        let xs: Vec<f32> = std::iter::once(0.)
            .chain((0..cols).scan(0., |x, j| {
                *x += m.get_column_width(self.sheet, a.column + j).unwrap_or(90.) as f32;
                Some(*x)
            }))
            .collect();
        let ys: Vec<f32> = std::iter::once(0.)
            .chain((0..rows).scan(0., |y, i| {
                *y += m.get_row_height(self.sheet, a.row + i).unwrap_or(25.) as f32;
                Some(*y)
            }))
            .collect();
        let (w, h) = (xs[cols as usize], ys[rows as usize]);
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>");
        for i in 0..rows {
            for j in 0..cols {
                let (r, c) = (a.row + i, a.column + j);
                let (x0, y0, x1, y1) = (xs[j as usize], ys[i as usize], xs[j as usize + 1], ys[i as usize + 1]);
                let st = m.get_cell_style(self.sheet, r, c).unwrap_or_default();
                let bg = m.resolve_color(&st.fill.color);
                if bg.len() == 7 {
                    svg.push_str(&format!("<rect x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" fill=\"{bg}\"/>", x1 - x0, y1 - y0));
                }
                svg.push_str(&format!(
                    "<rect x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#dee0e3\" stroke-width=\"1\"/>",
                    x1 - x0,
                    y1 - y0
                ));
                let text = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
                if !text.is_empty() {
                    let fc = m.resolve_color(&st.font.color);
                    let num = matches!(m.get_cell_type(self.sheet, r, c), Ok(CellType::Number));
                    use ironcalc::base::types::HorizontalAlignment as H;
                    let (x, anchor) = match st.alignment.as_ref().map(|a| &a.horizontal) {
                        Some(H::Center) => ((x0 + x1) / 2., "middle"),
                        Some(H::Right) => (x1 - 5., "end"),
                        Some(H::Left) => (x0 + 5., "start"),
                        _ if num => (x1 - 5., "end"),
                        _ => (x0 + 5., "start"),
                    };
                    let sz = st.font.sz as f32 + 1.;
                    svg.push_str(&format!(
                        "<text x=\"{x}\" y=\"{}\" font-family=\"PingFang SC\" font-size=\"{sz}\" text-anchor=\"{anchor}\" fill=\"{}\"{}{}>{}</text>",
                        (y0 + y1) / 2. + sz * 0.36,
                        if fc.len() == 7 { fc } else { "#1f2329".into() },
                        if st.font.b { " font-weight=\"bold\"" } else { "" },
                        if st.font.i { " font-style=\"italic\"" } else { "" },
                        esc(&text)
                    ));
                }
            }
        }
        svg.push_str("</svg>");
        drop(m);
        match crate::canvas::svg::svg_to_png(&svg, 2.) {
            Some(png) => {
                crate::clipboard::write_png(&png, "");
                self.toast = Some("已复制为图片".into());
            }
            None => self.toast = Some("复制为图片失败".into()),
        }
        cx.notify();
    }
}
