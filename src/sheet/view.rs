//! 界面：在线表格式两行工具栏、名称框 + 公式栏、网格、弹出菜单、查找替换、筛选面板、函数补全、
//! 工作表标签栏与状态栏（选区求和 / 平均 / 计数、缩放）。

use super::paint::{self, CellDraw, Frame};
use super::*;
use crate::theme::{theme, Theme};
use crate::ui::{icon, Tooltip};
use gpui::prelude::FluentBuilder;
use ironcalc::base::types::{HorizontalAlignment as H, VerticalAlignment as V};
use std::rc::Rc;

const FORMULA_H: f32 = 34.;

/// 字体菜单：(显示名, 字体族)。空字体族 = 默认字体。
const FONTS: [(&str, &str); 12] = [
    ("默认字体", ""),
    ("苹方", "PingFang SC"),
    ("宋体", "Songti SC"),
    ("黑体", "Heiti SC"),
    ("楷体", "Kaiti SC"),
    ("仿宋", "STFangsong"),
    ("Arial", "Arial"),
    ("Helvetica", "Helvetica"),
    ("Times New Roman", "Times New Roman"),
    ("Georgia", "Georgia"),
    ("Courier New", "Courier New"),
    ("Menlo", "Menlo"),
];

const SIZES: [i32; 14] = [9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 36, 48, 72];

/// 调色板：第一行灰阶，第二行标准色，下面两行浅色。
const PALETTE: [[&str; 8]; 4] = [
    ["#1f2329", "#373c43", "#646a73", "#8f959e", "#bbbfc4", "#dee0e3", "#eff0f1", "#ffffff"],
    ["#f54a45", "#ff8800", "#ffc60a", "#34c724", "#14c0ff", "#3370ff", "#7f3bf5", "#f01d94"],
    ["#fbbfbc", "#fed4a4", "#ffe28f", "#b7edb1", "#a9e6fa", "#bacefd", "#cdb2fa", "#f9b7d9"],
    ["#fde2e2", "#feead2", "#fff5c2", "#d9f5d6", "#d4f3fd", "#e1eaff", "#ece2fe", "#fdddef"],
];

/// 字体族映射：未知字体（如 xlsx 里的 Calibri / IronCalc 默认的 Inter）用苹方。
fn family(name: &str) -> &'static str {
    FONTS.iter().find(|f| !f.1.is_empty() && (f.1 == name || f.0 == name)).map(|f| f.1).unwrap_or("PingFang SC")
}

fn font_label(name: &str) -> &'static str {
    FONTS.iter().find(|f| !f.1.is_empty() && (f.1 == name || f.0 == name)).map(|f| f.0).unwrap_or("默认字体")
}

impl SheetEditor {
    pub(crate) fn cmd(cmd: impl Into<String>, cx: &mut Context<Self>) -> Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static> {
        let c: String = cmd.into();
        Box::new(cx.listener(move |this, _: &ClickEvent, w, cx| this.run(&c, w, cx)))
    }

    /// 弹出层的锚点：按钮下方（工具栏底边）。
    pub(crate) fn pop_at(&self, ev: &ClickEvent) -> Point<Pixels> {
        let y = self.bounds.origin.y - px(FORMULA_H) + px(2.);
        point(ev.position().x - px(14.), if self.bounds.size.height > px(0.) { y } else { ev.position().y + px(16.) })
    }

    pub(crate) fn pop_btn(kind: Pop, cx: &mut Context<Self>) -> Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static> {
        Box::new(cx.listener(move |this, ev: &ClickEvent, _, cx| {
            let at = this.pop_at(ev);
            this.open_pop(kind, at, cx)
        }))
    }

    // ───────────── 帧数据 ─────────────

    fn visible(&self) -> ((i32, i32), (i32, i32)) {
        let (fw, fh) = self.frozen_wh();
        let w = f32::from(self.bounds.size.width).max(1200.) - HEAD_W;
        let h = f32::from(self.bounds.size.height).max(800.) - HEAD_H;
        let c0 = (self.col_x.partition_point(|&v| v <= fw + self.scroll.0).max(1) as i32).max(self.fc + 1);
        let c1 = (self.col_x.partition_point(|&v| v < fw + self.scroll.0 + (w - fw).max(0.)).max(1) as i32).max(c0);
        let r0 = (self.row_y.partition_point(|&v| v <= fh + self.scroll.1).max(1) as i32).max(self.fr + 1);
        let r1 = (self.row_y.partition_point(|&v| v < fh + self.scroll.1 + (h - fh).max(0.)).max(1) as i32).max(r0);
        ((r0, r1.min(self.row_y.len() as i32 - 1)), (c0, c1.min(self.col_x.len() as i32 - 1)))
    }

    fn cells(&mut self, rows: (i32, i32), cols: (i32, i32), t: &Theme, window: &mut Window) -> Rc<Vec<CellDraw>> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (
            self.ver,
            crate::objects::version(&self.obj_id),
            self.sheet,
            self.zoom.to_bits(),
            self.show_formulas,
            rows,
            cols,
            self.fr,
            self.fc,
            self.filter_ver_key(),
            t.dark,
            (self.geo_key, crate::canvas::paint::image_generation()),
        )
            .hash(&mut h);
        let key = h.finish();
        if let Some((k, c)) = &self.cache {
            if *k == key {
                return c.clone();
            }
        }
        let z = self.zoom;
        let merged = self.merged();
        let mut want: Vec<(i32, i32)> = vec![];
        let row_list: Vec<i32> = (1..=self.fr).chain(rows.0..=rows.1).collect();
        let col_list: Vec<i32> = (1..=self.fc).chain(cols.0..=cols.1).collect();
        for &r in &row_list {
            for &c in &col_list {
                want.push((r, c));
            }
        }
        // 合并块的左上角不在可视区时也要画
        for m in &merged {
            let vis_r = row_list.iter().any(|&r| r >= m.0 && r < m.0 + m.3);
            let vis_c = col_list.iter().any(|&c| c >= m.1 && c < m.1 + m.2);
            if vis_r && vis_c && !want.contains(&(m.0, m.1)) {
                want.push((m.0, m.1));
            }
        }
        let inside = |r: i32, c: i32| merged.iter().find(|m| r >= m.0 && r < m.0 + m.3 && c >= m.1 && c < m.1 + m.2).copied();
        // 单元格对象：有对象的格子即使没有文字也要画
        let sx = self.sx();
        for x in &sx.cells {
            if (row_list.contains(&x.r) && col_list.contains(&x.c)) && !want.contains(&(x.r, x.c)) {
                want.push((x.r, x.c));
            }
        }
        let has_dv = !sx.dv.is_empty();
        let m = self.model.borrow();
        let mut out: Vec<CellDraw> = vec![];
        for (r, c) in want {
            let mut mg = false;
            let (mut r1, mut c1) = (r, c);
            if let Some(mm) = inside(r, c) {
                if (mm.0, mm.1) != (r, c) {
                    continue;
                }
                mg = true;
                r1 = mm.0 + mm.3 - 1;
                c1 = mm.1 + mm.2 - 1;
            }
            let Ok(ext) = m.get_extended_cell_style(self.sheet, r, c) else { continue };
            let st = &ext.style;
            let mut text = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
            if self.show_formulas {
                let content = m.get_cell_content(self.sheet, r, c).unwrap_or_default();
                if content.starts_with('=') {
                    text = content;
                }
            }
            if ext.data_bar.as_ref().is_some_and(|d| !d.show_value) {
                text.clear();
            }
            let bg = hex_to_hsla(&m.resolve_color(&st.fill.color));
            let bc = |b: &Option<ironcalc::base::types::BorderItem>| {
                b.as_ref().map(|b| {
                    use ironcalc::base::types::BorderStyle as B;
                    let color = hex_to_hsla(&m.resolve_color(&b.color)).unwrap_or(if t.dark { t.text_secondary } else { rgb(0x1f2329).into() });
                    let (w, dashed) = match b.style {
                        B::Thin => (1., false),
                        B::Medium => (2., false),
                        B::Thick | B::Double => (3., false),
                        B::MediumDashed | B::MediumDashDot | B::MediumDashDotDot => (2., true),
                        _ => (1., true),
                    };
                    (color, w * z.max(1.), dashed)
                })
            };
            let borders = [bc(&st.border.top), bc(&st.border.right), bc(&st.border.bottom), bc(&st.border.left)];
            let diag = (st.border.diagonal_down || st.border.diagonal_up)
                .then(|| bc(&st.border.diagonal).map(|(c, w, _)| (st.border.diagonal_down, st.border.diagonal_up, c, w)))
                .flatten();
            let mut obj = self.cell_obj(&sx, has_dv, r, c, &mut text, &m, t);
            // 条件格式的图标集 / 等级
            if ext.icon.is_some() || ext.rating.is_some() {
                let o = obj.get_or_insert_with(Default::default);
                if let Some(ic) = &ext.icon {
                    o.cf_icon = Some((super::cf::icon_name(&ic.icon), hex_to_hsla(&m.resolve_color(&ic.color)).unwrap_or(rgb(0x808080).into())));
                    if !ic.show_value {
                        text.clear();
                    }
                }
                if let Some(rt) = &ext.rating {
                    o.rating =
                        Some((super::cf::icon_name(&rt.icon), rt.count, rt.max, hex_to_hsla(&m.resolve_color(&rt.color)).unwrap_or(rgb(0xf2b136).into())));
                    if !rt.show_value {
                        text.clear();
                    }
                }
            }
            let bar = ext.data_bar.as_ref().and_then(|d| {
                let col = hex_to_hsla(&m.resolve_color(&d.positive_color))?;
                let v = d.value.clamp(0., 1.) as f32;
                let axis = d.axis_position.clamp(0., 1.) as f32;
                Some((axis, v * (1. - axis), col, d.is_gradient))
            });
            if text.is_empty() && bg.is_none() && borders.iter().all(|b| b.is_none()) && bar.is_none() && !mg && obj.is_none() && diag.is_none() {
                continue;
            }
            let ty = m.get_cell_type(self.sheet, r, c).ok();
            let al = st.alignment.as_ref();
            let h = match al.map(|a| &a.horizontal) {
                Some(H::Left) => 0,
                Some(H::Center) | Some(H::CenterContinuous) => 1,
                Some(H::Right) => 2,
                _ => match ty {
                    Some(CellType::Number) if !self.show_formulas || !text.starts_with('=') => 2,
                    Some(CellType::LogicalValue) | Some(CellType::ErrorValue) => 1,
                    _ => 0,
                },
            };
            // 没有对齐信息时垂直居中（默认）
            let v = match al.map(|a| &a.vertical) {
                None => 1,
                Some(V::Top) => 0,
                Some(V::Bottom) => 2,
                _ => 1,
            };
            let mut f = font(family(&st.font.name));
            if st.font.b {
                f.weight = FontWeight::BOLD;
            }
            if st.font.i {
                f.style = FontStyle::Italic;
            }
            let fc = hex_to_hsla(&m.resolve_color(&st.font.color)).filter(|c| !(t.dark && c.l < 0.3) && !(!t.dark && c.l > 0.97 && bg.is_none()));
            let fc = if obj.as_ref().is_some_and(|o| o.remind) { Some(rgb(0x3370ff).into()) } else { fc };
            // 日期提醒（时钟图标在前）、图片、附件：没有显式对齐时靠左
            let h = if obj.as_ref().is_some_and(|o| o.remind || o.image.is_some() || !o.files.is_empty())
                && matches!(al.map(|a| &a.horizontal), None | Some(H::General))
            {
                0
            } else {
                h
            };
            out.push(CellDraw {
                r,
                c,
                x0: self.col_left(c),
                x1: self.col_left(c1 + 1),
                y0: self.row_top(r),
                y1: self.row_top(r1 + 1),
                text: text.into(),
                color: fc.unwrap_or(t.text),
                bg,
                font: f,
                px: (st.font.sz as f32 + 1.) * z,
                underline: st.font.u,
                strike: st.font.strike,
                h,
                v,
                wrap: al.is_some_and(|a| a.wrap_text),
                merged: mg,
                ov: (0., 0.),
                bar,
                borders,
                diag,
                obj,
                clip: sx.is_clip(r, c),
            });
        }
        drop(m);
        // 文字溢出：左对齐向右、右对齐向左、居中向两侧，经过的格子必须为空
        let filled: std::collections::HashSet<(i32, i32)> = out.iter().filter(|d| !d.text.is_empty() || d.merged).map(|d| (d.r, d.c)).collect();
        let pad = 5. * z;
        for i in 0..out.len() {
            out[i].ov = (out[i].x0, out[i].x1);
            let d = &out[i];
            if d.wrap || d.merged || d.text.is_empty() || d.clip {
                continue;
            }
            let run = TextRun { len: d.text.len(), font: d.font.clone(), color: d.color, background_color: None, underline: None, strikethrough: None };
            let need = f32::from(window.text_system().shape_line(d.text.clone(), px(d.px), &[run], None).width) + pad * 2.;
            let own = d.x1 - d.x0;
            if need <= own {
                continue;
            }
            let (lo, hi) = if d.c <= self.fc { (1, self.fc) } else { (self.fc + 1, cols.1) };
            let free = |c: i32| c >= lo && c <= hi && !filled.contains(&(d.r, c)) && inside(d.r, c).is_none();
            let (mut l, mut rr) = (d.c, d.c);
            match d.h {
                0 => {
                    while self.col_left(rr + 1) - d.x0 < need && free(rr + 1) {
                        rr += 1;
                    }
                }
                2 => {
                    while d.x1 - self.col_left(l) < need && free(l - 1) {
                        l -= 1;
                    }
                }
                _ => {
                    let extra = (need - own) / 2.;
                    while d.x0 - self.col_left(l) < extra && free(l - 1) {
                        l -= 1;
                    }
                    while self.col_left(rr + 1) - d.x1 < extra && free(rr + 1) {
                        rr += 1;
                    }
                }
            }
            out[i].ov = (self.col_left(l), self.col_left(rr + 1));
        }
        let rc = Rc::new(out);
        self.cache = Some((key, rc.clone()));
        // 单元格图片还在后台解码：下一帧再来（解码完成会改变缓存键）
        if crate::canvas::paint::image_pending() {
            window.request_animation_frame();
        }
        rc
    }

    fn filter_ver_key(&self) -> u64 {
        self.filter.as_ref().map(|f| f.hidden.len() as u64 + f.excl.len() as u64 * 7919).unwrap_or(0)
    }

    fn build_frame(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> Frame {
        self.rebuild_geometry();
        let (rows, cols) = self.visible();
        let cells = self.cells(rows, cols, t, window);
        let a = self.sel_area();
        let cur_m = self.merged().into_iter().find(|m| self.cur.r >= m.0 && self.cur.r < m.0 + m.3 && self.cur.c >= m.1 && self.cur.c < m.1 + m.2);
        let cur = cur_m.map(|m| (m.0, m.1, m.0 + m.3 - 1, m.1 + m.2 - 1)).unwrap_or((self.cur.r, self.cur.c, self.cur.r, self.cur.c));
        let refs = self
            .editing_text(cx)
            .map(|t| {
                let mut seen: Vec<String> = vec![];
                funcs::parse_refs(&t)
                    .into_iter()
                    .map(|(r0, c0, r1, c1, s, e)| {
                        let key = t[s..e].replace('$', "").to_uppercase();
                        let i = seen.iter().position(|k| *k == key).unwrap_or_else(|| {
                            seen.push(key);
                            seen.len() - 1
                        });
                        (r0, c0, r1, c1, rgb(funcs::REF_COLORS[i % funcs::REF_COLORS.len()]).into())
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                // 选中迷你图且面板打开：数据范围标红
                match self.side {
                    Some(super::dialogs::Side::Spark(c)) if c == self.cur => self
                        .sx()
                        .cell(c.r, c.c)
                        .and_then(|x| x.spark.as_ref().and_then(|s| funcs::parse_range(&s.src)))
                        .map(|(r0, c0, r1, c1)| vec![(r0, c0, r1, c1, rgb(0xf54a45).into())])
                        .unwrap_or_default(),
                    _ => vec![],
                }
            });
        let (finds, find_cur) = self
            .find
            .as_ref()
            .map(|f| {
                let vis: Vec<Cell> = f
                    .matches
                    .iter()
                    .filter(|(s, _)| *s == self.sheet)
                    .map(|(_, m)| *m)
                    .filter(|m| (m.r <= self.fr || (m.r >= rows.0 && m.r <= rows.1)) && (m.c <= self.fc || (m.c >= cols.0 && m.c <= cols.1)))
                    .collect();
                (vis, f.matches.get(f.idx).filter(|(s, _)| *s == self.sheet).map(|(_, m)| *m))
            })
            .unwrap_or_default();
        Frame {
            cells,
            col_x: self.col_x.clone(),
            row_y: self.row_y.clone(),
            fr: self.fr,
            fc: self.fc,
            scroll: self.scroll,
            rows,
            cols,
            sel: (a.row, a.column, a.row + a.height - 1, a.column + a.width - 1),
            cur,
            kind: self.kind,
            fill_to: match &self.drag {
                Some(super::SDrag::Fill { to }) => Some(*to),
                _ => None,
            },
            refs,
            finds,
            find_cur,
            filter: self.filter.as_ref().filter(|f| f.sheet == self.sheet).map(|f| (f.head, f.c0, f.c1, f.excl.keys().copied().collect())),
            grid_on: self.model.borrow().get_show_grid_lines(self.sheet).unwrap_or(true),
            cut: self.clip.as_ref().filter(|c| c.sheet == self.sheet && c.shown).map(|c| c.range),
            resize: self.resize_state().map(|(col, i, dragging)| (col, self.resize_edge(col, i), dragging)),
            editing: self.editing.is_some(),
            zoom: self.zoom,
            bg: t.bg,
            grid: if t.dark { hsla(0., 0., 0.26, 1.) } else { rgb(0xdee0e3).into() },
            head_bg: if t.dark { hsla(0., 0., 0.16, 1.) } else { rgb(0xf5f6f7).into() },
            head_fg: t.text_secondary,
            accent: t.accent,
            dark: t.dark,
            hl: super::view_prefs(&self.obj_id).hl.as_deref().and_then(hex_to_hsla).map(|c| c.opacity(0.35)),
            date_btn: self.kind == SelKind::Cells
                && a.width == 1
                && a.height == 1
                && matches!(self.sx().dv_at(self.cur.r, self.cur.c).map(|d| &d.rule), Some(super::ext::Rule::Date { .. })),
        }
    }

    /// 单元格对象的绘制数据；会改写要显示的文字（复选框、下拉胶囊、图片、附件不显示原文字）。
    fn cell_obj(
        &self,
        sx: &super::ext::SheetX,
        has_dv: bool,
        r: i32,
        c: i32,
        text: &mut String,
        m: &ironcalc::base::UserModel,
        t: &Theme,
    ) -> Option<Box<paint::ObjDraw>> {
        use super::ext::Rule;
        let x = sx.cell(r, c);
        let dv = if has_dv { sx.dv_at(r, c) } else { None };
        if x.is_none() && dv.is_none() {
            return None;
        }
        let mut o = paint::ObjDraw::default();
        if let Some(dv) = dv {
            match &dv.rule {
                Rule::Check => {
                    let on = matches!(m.get_model().get_cell_value_by_index(self.sheet, r, c), Ok(ironcalc::base::cell::CellValue::Boolean(true)));
                    o.check = Some(on);
                    text.clear();
                }
                Rule::List { opts, multi, colored, .. } => {
                    let vals = if *multi {
                        super::ext::split_multi(text)
                    } else if text.is_empty() {
                        vec![]
                    } else {
                        vec![text.clone()]
                    };
                    o.chips = vals
                        .into_iter()
                        .map(|v| {
                            let (bg, fg) = match opts.iter().find(|o| o.text == v) {
                                Some(op) if *colored => op.colors(),
                                _ => (0xdee0e3, 0x1f2329),
                            };
                            let (bg, fg): (Hsla, Hsla) = (rgb(bg).into(), rgb(fg).into());
                            (v.into(), if t.dark { bg.opacity(0.75) } else { bg }, fg)
                        })
                        .collect();
                    o.arrow = true;
                    text.clear();
                }
                _ => {}
            }
            if !text.is_empty() || o.check.is_some() || !o.chips.is_empty() {
                o.invalid = !self.valid_at(dv, r, c);
            }
        }
        if let Some(x) = x {
            o.note = !x.note.is_empty();
            o.comment = !x.comments.is_empty();
            o.remind = x.remind.is_some();
            if !x.image.is_empty() {
                o.image = crate::canvas::paint::try_load_image(&x.image);
                text.clear();
            }
            if !x.files.is_empty() {
                o.files = x.files.iter().map(|f| SharedString::from(f.name.clone())).collect();
                text.clear();
            }
            if let Some(sp) = &x.spark {
                let mut vals: Vec<Option<f64>> = funcs::parse_range(&sp.src)
                    .map(|(r0, c0, r1, c1)| {
                        (r0..=r1)
                            .flat_map(|r| (c0..=c1).map(move |c| (r, c)))
                            .take(2000)
                            .filter_map(|(r, c)| match m.get_model().get_cell_value_by_index(self.sheet, r, c) {
                                Ok(ironcalc::base::cell::CellValue::Number(v)) => Some(Some(v)),
                                // 空单元格：空距 / 零值 / 直线连接（跳过）
                                Ok(ironcalc::base::cell::CellValue::None) => match sp.empty {
                                    0 => Some(None),
                                    1 => Some(Some(0.)),
                                    _ => None,
                                },
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if sp.rtl {
                    vals.reverse();
                }
                let th = super::ext::SPARK_THEMES[sp.theme as usize % super::ext::SPARK_THEMES.len()];
                let accent: Hsla = rgb(th.2).into();
                let mark_cols = std::array::from_fn(|i| sp.mark_color(i).and_then(hex_to_hsla).unwrap_or(accent));
                o.spark = Some(paint::SparkDraw {
                    vals,
                    kind: sp.kind,
                    main: hex_to_hsla(&sp.color).unwrap_or(rgb(th.1).into()),
                    mark_cols,
                    fancy: sp.fancy,
                    marks: sp.marks,
                    lw: sp.lw.max(1) as f32 * 0.75,
                    axis: sp.axis,
                });
                text.clear();
            }
        }
        Some(Box::new(o))
    }

    // ───────────── 小部件 ─────────────

    /// 大按钮：上图标下文字（「菜单」「撤销」「格式刷」…）
    pub(crate) fn big(id: &'static str, ic: &'static str, label: &'static str, drop: bool, on: bool, t: &Theme) -> Stateful<Div> {
        let fg = if on { t.accent } else { t.text };
        div()
            .id(id)
            .flex_none()
            .h(px(52.))
            .px(px(7.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(3.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(on, |d| d.bg(t.accent_soft))
            .hover(|s| s.bg(t.hover))
            .child(div().flex().flex_row().items_center().gap(px(2.)).child(icon(ic, 18., fg)).when(drop, |d| d.child(icon("sh-chevron", 10., t.text_muted))))
            .child(div().text_size(px(12.)).text_color(fg).whitespace_nowrap().child(label))
    }

    /// 小图标按钮（28×28）
    pub(crate) fn ib(id: impl Into<ElementId>, ic: &'static str, tip: &'static str, on: bool, t: &Theme) -> Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .when(on, |d| d.bg(t.accent_soft))
            .hover(|s| s.bg(t.hover))
            .child(icon(ic, 16., if on { t.accent } else { t.text }))
            .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
    }

    pub(crate) fn vsep(t: &Theme) -> Div {
        div().flex_none().w(px(1.)).h(px(40.)).mx(px(6.)).bg(t.border)
    }

    pub(crate) fn menu_row(
        id: impl Into<ElementId>,
        ic: Option<&'static str>,
        label: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        t: &Theme,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .mx(px(4.))
            .px(px(8.))
            .h(px(32.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .rounded(px(6.))
            .text_size(px(13.))
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .child(div().w(px(16.)).flex_none().when_some(ic, |d, i| d.child(icon(i, 15., t.text_secondary))))
            .child(div().flex_1().whitespace_nowrap().child(label.into()))
            .child(div().pl(px(16.)).text_size(px(12.)).text_color(t.text_muted).whitespace_nowrap().child(hint.into()))
    }

    pub(crate) fn item(
        &self,
        id: &'static str,
        ic: Option<&'static str>,
        label: impl Into<SharedString>,
        hint: &'static str,
        cmd: impl Into<String>,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        Self::menu_row(id, ic, label, hint, t).on_click(Self::cmd(cmd, cx))
    }

    pub(crate) fn sep(t: &Theme) -> Div {
        div().my(px(4.)).mx(px(8.)).h(px(1.)).bg(t.border)
    }

    pub(crate) fn section(s: &'static str, t: &Theme) -> Div {
        div().px(px(12.)).pt(px(6.)).pb(px(2.)).text_size(px(12.)).text_color(t.text_muted).child(s)
    }

    pub(crate) fn panel(t: &Theme) -> Div {
        div()
            .occlude()
            .flex()
            .flex_col()
            .bg(t.panel_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(8.))
            .shadow_lg()
            .py(px(4.))
            .text_size(px(13.))
            .text_color(t.text)
            .font_family("PingFang SC")
    }

    // ───────────── 工具栏 ─────────────

    fn toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let st = self.cur_style();
        let al = st.alignment.clone();
        let hz = al.as_ref().map(|a| a.horizontal.clone()).unwrap_or_default();
        let vt = al.as_ref().map(|a| a.vertical.clone());
        let wrap = al.as_ref().is_some_and(|a| a.wrap_text);
        let merged = self.merged().iter().any(|m| self.cur.r >= m.0 && self.cur.r < m.0 + m.3 && self.cur.c >= m.1 && self.cur.c < m.1 + m.2);
        let sel = self.sel_area();
        let can_merge = self.kind == SelKind::Cells && (sel.width > 1 || sel.height > 1) || merged;
        let (can_undo, can_redo) = {
            let m = self.model.borrow();
            (m.can_undo(), m.can_redo())
        };
        let painter_on = self.painter.is_some();
        let clip = self.sx().is_clip(self.cur.r, self.cur.c);
        let wide = self.bounds.size.width > px(1560.);
        let font_color = hex_to_hsla(&self.model.borrow().resolve_color(&st.font.color)).unwrap_or(rgb(0xf54a45).into());
        let fill_color = hex_to_hsla(&self.model.borrow().resolve_color(&st.fill.color)).unwrap_or(rgb(0xffc60a).into());
        let disabled = |d: Stateful<Div>, ok: bool| if ok { d } else { d.opacity(0.4) };
        if self.collapsed {
            return div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(2.))
                .pl(px(84.))
                .pr(px(10.))
                .h(px(40.))
                .border_b_1()
                .border_color(t.border)
                .bg(t.bg)
                .child(Self::ib("c-exit", "sh-exit", "退出", false, t).on_click(Self::cmd("close", cx)))
                .child(Self::vsep(t).h(px(20.)))
                .child(Self::ib("c-undo", "sh-undo", "撤销 ⌘Z", false, t).on_click(Self::cmd("undo", cx)))
                .child(Self::ib("c-redo", "sh-redo", "重做 ⇧⌘Z", false, t).on_click(Self::cmd("redo", cx)))
                .child(Self::ib("c-painter", "sh-painter", "格式刷", painter_on, t).on_click(Self::cmd("painter", cx)))
                .child(Self::vsep(t).h(px(20.)))
                .child(Self::ib("c-b", "bold", "加粗 ⌘B", st.font.b, t).on_click(Self::cmd("bold", cx)))
                .child(Self::ib("c-i", "italic", "斜体 ⌘I", st.font.i, t).on_click(Self::cmd("italic", cx)))
                .child(Self::ib("c-u", "underline", "下划线 ⌘U", st.font.u, t).on_click(Self::cmd("underline", cx)))
                .child(Self::ib("c-s", "strikethrough", "删除线 ⇧⌘X", st.font.strike, t).on_click(Self::cmd("strike", cx)))
                .child(Self::vsep(t).h(px(20.)))
                .child(Self::ib("c-hl", "sh-h-left", "左对齐", matches!(hz, H::Left), t).on_click(Self::cmd("h-left", cx)))
                .child(Self::ib("c-hc", "sh-h-center", "水平居中", matches!(hz, H::Center), t).on_click(Self::cmd("h-center", cx)))
                .child(Self::ib("c-hr", "sh-h-right", "右对齐", matches!(hz, H::Right), t).on_click(Self::cmd("h-right", cx)))
                .child(Self::ib("c-find", "sh-find", "查找和替换 ⌘F", false, t).on_click(Self::cmd("find", cx)))
                .child(div().flex_1())
                .child(Self::ib("c-expand", "sh-down", "展开工具栏", false, t).on_click(cx.listener(|this, _, _, cx| {
                    this.collapsed = false;
                    cx.notify();
                })));
        }
        div()
            .flex()
            .flex_row()
            .items_center()
            .pl(px(80.))
            .pr(px(10.))
            .h(px(68.))
            .border_b_1()
            .border_color(t.border)
            .bg(t.bg)
            .child(
                div()
                    .id("s-exit")
                    .flex_none()
                    .px(px(8.))
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(icon("sh-exit", 18., t.text))
                    .child(div().text_size(px(15.)).child("退出"))
                    .on_click(Self::cmd("close", cx)),
            )
            .child(Self::vsep(t))
            .child(
                Self::big("s-menu", "sh-menu", "菜单", true, false, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("菜单".into())).into())
                    .on_click(Self::pop_btn(Pop::Menu, cx)),
            )
            .child(Self::vsep(t))
            .child(
                disabled(Self::big("s-undo", "sh-undo", "撤销", false, false, t), can_undo)
                    .on_click(Self::cmd("undo", cx))
                    .tooltip(|_, cx| cx.new(|_| Tooltip("撤销 ⌘Z".into())).into()),
            )
            .child(
                disabled(Self::big("s-redo", "sh-redo", "重做", false, false, t), can_redo)
                    .on_click(Self::cmd("redo", cx))
                    .tooltip(|_, cx| cx.new(|_| Tooltip("重做 ⇧⌘Z".into())).into()),
            )
            .child(
                Self::big("s-painter", "sh-painter", "格式刷", false, painter_on, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("格式刷：单击使用一次，双击连续使用".into())).into())
                    .on_click(cx.listener(|this, ev: &ClickEvent, w, cx| {
                        if ev.click_count() >= 2 {
                            this.painter = None;
                            this.run("painter-sticky", w, cx)
                        } else {
                            this.run("painter", w, cx)
                        }
                    })),
            )
            .child(
                Self::big("s-clear", "sh-clear", "清除格式", false, false, t)
                    .on_click(Self::cmd("clear-format", cx))
                    .tooltip(|_, cx| cx.new(|_| Tooltip("清除格式 ⌘\\".into())).into()),
            )
            .child(Self::vsep(t))
            .child(
                Self::big("s-insert", "sh-insert", "插入", true, false, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("插入".into())).into())
                    .on_click(Self::pop_btn(Pop::Insert, cx)),
            )
            .child(Self::vsep(t))
            // 字体 / 字号 / 边框 + B S I U A 填充
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.))
                            .child(
                                Self::select_box("s-font", font_label(&st.font.name), 132., t)
                                    .tooltip(|_, cx| cx.new(|_| Tooltip("字体".into())).into())
                                    .on_click(Self::pop_btn(Pop::Font, cx)),
                            )
                            .child(
                                Self::select_box("s-size", SharedString::from(st.font.sz.to_string()), 52., t)
                                    .tooltip(|_, cx| cx.new(|_| Tooltip("字号".into())).into())
                                    .on_click(Self::pop_btn(Pop::Size, cx)),
                            )
                            .child(
                                div()
                                    .id("s-border")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .h(px(28.))
                                    .px(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(t.hover))
                                    .child(icon("sh-border", 16., t.text))
                                    .child(icon("sh-chevron", 10., t.text_muted))
                                    .tooltip(|_, cx| cx.new(|_| Tooltip("边框".into())).into())
                                    .on_click(Self::pop_btn(Pop::Border, cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(2.))
                            .child(Self::ib("s-b", "bold", "加粗 ⌘B", st.font.b, t).on_click(Self::cmd("bold", cx)))
                            .child(Self::ib("s-s", "strikethrough", "删除线 ⇧⌘X", st.font.strike, t).on_click(Self::cmd("strike", cx)))
                            .child(Self::ib("s-i", "italic", "斜体 ⌘I", st.font.i, t).on_click(Self::cmd("italic", cx)))
                            .child(Self::ib("s-u", "underline", "下划线 ⌘U", st.font.u, t).on_click(Self::cmd("underline", cx)))
                            .child(Self::color_btn("s-fc", "sh-a", font_color, "文字颜色", Pop::Color, t, cx))
                            .child(Self::color_btn("s-bg", "sh-bucket", fill_color, "填充颜色", Pop::Fill, t, cx)),
                    ),
            )
            .child(Self::vsep(t))
            // 垂直对齐 / 水平对齐
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.))
                            .child(Self::ib("s-vt", "sh-v-top", "顶部对齐", matches!(vt, Some(V::Top)), t).on_click(Self::cmd("v-top", cx)))
                            .child(
                                Self::ib("s-vm", "sh-v-mid", "垂直居中", vt.is_none() || matches!(vt, Some(V::Center)), t).on_click(Self::cmd("v-middle", cx)),
                            )
                            .child(Self::ib("s-vb", "sh-v-bottom", "底部对齐", matches!(vt, Some(V::Bottom)), t).on_click(Self::cmd("v-bottom", cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.))
                            .child(
                                Self::ib("s-hl", "sh-h-left", "左对齐 ⇧⌘L", matches!(hz, H::Left) || matches!(hz, H::General), t)
                                    .on_click(Self::cmd("h-left", cx)),
                            )
                            .child(Self::ib("s-hc", "sh-h-center", "水平居中 ⇧⌘E", matches!(hz, H::Center), t).on_click(Self::cmd("h-center", cx)))
                            .child(Self::ib("s-hr", "sh-h-right", "右对齐 ⇧⌘R", matches!(hz, H::Right), t).on_click(Self::cmd("h-right", cx))),
                    ),
            )
            .child(Self::vsep(t))
            // 换行方式 + 合并单元格
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.))
                            .child(Self::ib("s-ov", "sh-overflow", "溢出", !wrap && !clip, t).on_click(Self::cmd("overflow", cx)))
                            .child(Self::ib("s-clip", "sh-clip", "截断", clip, t).on_click(Self::cmd("clip", cx)))
                            .child(Self::ib("s-wrap", "sh-wrap", "自动换行", wrap, t).on_click(Self::cmd("wrap", cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.))
                            .child(
                                div()
                                    .id("s-merge")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(4.))
                                    .h(px(28.))
                                    .px(px(5.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .when(merged, |d| d.bg(t.accent_soft))
                                    .when(!can_merge, |d| d.opacity(0.4))
                                    .hover(|s| s.bg(t.hover))
                                    .child(icon("sh-merge", 16., if merged { t.accent } else { t.text }))
                                    .child(div().text_size(px(12.5)).text_color(if merged { t.accent } else { t.text }).child(if merged {
                                        "取消合并"
                                    } else {
                                        "合并单元格"
                                    }))
                                    .on_click(Self::cmd("merge-toggle", cx)),
                            )
                            .child(Self::ib("s-merge-more", "sh-chevron", "合并方式", false, t).w(px(16.)).on_click(Self::pop_btn(Pop::Merge, cx))),
                    ),
            )
            .child(Self::vsep(t))
            // 窗口够宽时「更多」里的按钮直接平铺（同类响应式）
            .when(wide, |d| d.child(self.more_group(true, t, cx)).child(Self::vsep(t)))
            .when(!wide, |d| {
                d.child(
                    Self::big("s-more", "sh-more", "更多", false, matches!(self.pop.as_ref().map(|p| p.kind), Some(Pop::More)), t)
                        .tooltip(|_, cx| cx.new(|_| Tooltip("更多".into())).into())
                        .on_click(Self::pop_btn(Pop::More, cx)),
                )
            })
            .child(
                Self::big("s-find", "sh-find", "查找和替换", false, self.find.is_some(), t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("查找和替换 ⇧⌘H".into())).into())
                    .on_click(Self::cmd("replace", cx)),
            )
            .child(
                Self::big("s-comment", "sh-comment", "评论", false, false, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("评论 ⌥⌘M".into())).into())
                    .on_click(Self::cmd("comment", cx)),
            )
            .child(div().flex_1())
            .child(Self::ib("s-collapse", "sh-collapse", "收起工具栏", false, t).on_click(cx.listener(|this, _, _, cx| {
                this.collapsed = true;
                cx.notify();
            })))
    }

    /// 文档内嵌表格的浮动工具栏（选中表格后在上方浮出）。
    fn float_toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let st = self.cur_style();
        let num_label =
            super::menu::NUM_FMTS.iter().find(|f| f.1 == st.num_fmt).map(|f| f.0).unwrap_or(if st.num_fmt == "general" { "常规" } else { "自定义" });
        let font_color = hex_to_hsla(&self.model.borrow().resolve_color(&st.font.color)).unwrap_or(rgb(0xf54a45).into());
        let fill_color = hex_to_hsla(&self.model.borrow().resolve_color(&st.fill.color)).unwrap_or(rgb(0xffc60a).into());
        let pop = |kind: Pop, cx: &mut Context<Self>| -> Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static> {
            Box::new(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                let at = point(ev.position().x - px(14.), this.bounds.origin.y - px(4.));
                this.open_pop(kind, at, cx)
            }))
        };
        let drop = |id: &'static str, ic: &'static str, tip: &'static str, kind: Pop, cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .h(px(28.))
                .px(px(3.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .child(icon(ic, 16., t.text))
                .child(icon("sh-chevron", 10., t.text_muted))
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                .on_click(pop(kind, cx))
        };
        let sep = || div().w(px(1.)).h(px(18.)).mx(px(4.)).bg(t.border);
        let filter_on = self.filter.as_ref().is_some_and(|f| f.sheet == self.sheet);
        Self::panel(t)
            .flex_row()
            .items_center()
            .gap(px(2.))
            .px(px(6.))
            .py(px(4.))
            .child(Self::ib("ft-painter", "sh-painter", "格式刷", self.painter.is_some(), t).on_click(Self::cmd("painter", cx)))
            .child(sep())
            .child(Self::select_box("ft-num", num_label, 86., t).tooltip(|_, cx| cx.new(|_| Tooltip("数字格式".into())).into()).on_click(pop(Pop::Num, cx)))
            .child(sep())
            .child(
                Self::select_box("ft-font", font_label(&st.font.name), 86., t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("字体".into())).into())
                    .on_click(pop(Pop::Font, cx)),
            )
            .child(
                Self::select_box("ft-size", SharedString::from(st.font.sz.to_string()), 46., t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("字号".into())).into())
                    .on_click(pop(Pop::Size, cx)),
            )
            .child(sep())
            .child(Self::ib("ft-b", "bold", "加粗 ⌘B", st.font.b, t).on_click(Self::cmd("bold", cx)))
            .child(Self::color_btn_raw("ft-fc", "sh-a", font_color, "文字颜色", t).on_click(pop(Pop::Color, cx)))
            .child(Self::color_btn_raw("ft-bg", "sh-bucket", fill_color, "填充颜色", t).on_click(pop(Pop::Fill, cx)))
            .child(drop("ft-border", "sh-border", "边框", Pop::Border, cx))
            .child(sep())
            .child(Self::ib("ft-merge", "sh-merge", "合并单元格 ⌥⇧D", false, t).on_click(Self::cmd("merge-toggle", cx)))
            .child(drop("ft-align", "sh-h-left", "对齐", Pop::Align, cx))
            .child(drop("ft-wrap", "sh-wrap", "文本换行", Pop::Wrap, cx))
            .child(sep())
            .child(Self::ib("ft-list", "sh-dropdown", "下拉列表", false, t).on_click(Self::cmd("ins-list", cx)))
            .child(Self::ib("ft-filter", "sh-filter", "筛选 ⌃⇧L", filter_on, t).on_click(Self::cmd("filter", cx)))
            .child(Self::ib("ft-more", "sh-more", "更多（完整菜单）", false, t).on_click(pop(Pop::Menu, cx)))
            .child(sep())
            .child(Self::ib("ft-insert", "sh-insert", "插入", false, t).on_click(pop(Pop::Insert, cx)))
            .child(Self::ib("ft-comment", "sh-comment", "评论", false, t).on_click(Self::cmd("comment", cx)))
            .child(Self::ib("ft-full", "expand", "全屏", false, t).on_click(Self::cmd("fullscreen", cx)))
    }

    /// 「更多」里的一组按钮（窗口宽时平铺在工具栏，窄时收进「更多」面板）：
    /// 常规▾ / ¥ % .00 .0 | 冻结▾ 筛选▾ 排序 条件格式▾ 下拉列表 公式▾
    fn more_group(&self, inline: bool, t: &Theme, cx: &mut Context<Self>) -> Div {
        let st = self.cur_style();
        let num_label =
            super::menu::NUM_FMTS.iter().find(|f| f.1 == st.num_fmt).map(|f| f.0).unwrap_or(if st.num_fmt == "general" { "常规" } else { "自定义" });
        let (ur, uc) = self.used();
        let has_data = ur > 1 || uc > 1 || !self.is_empty(1, 1);
        let frozen = self.fr > 0 || self.fc > 0;
        let filter_on = self.filter.as_ref().is_some_and(|f| f.sheet == self.sheet);
        let pop = |kind: Pop, cx: &mut Context<Self>| -> Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static> {
            Box::new(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                // 面板里的按钮：下拉出现在按钮正下方
                let at = point(ev.position().x - px(14.), ev.position().y + px(24.));
                let at = if inline { this.pop_at(ev) } else { at };
                this.open_pop(kind, at, cx)
            }))
        };
        let small = |id: &'static str, label: &'static str, tip: &'static str, on: bool, cx: &mut Context<Self>, cmd: &'static str| {
            div()
                .id(id)
                .h(px(26.))
                .min_w(px(26.))
                .px(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(on, |d| d.bg(t.accent_soft).text_color(t.accent))
                .hover(|s| s.bg(t.hover))
                .child(label)
                .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
                .on_click(Self::cmd(cmd, cx))
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        Self::select_box("mo-num", num_label, 110., t)
                            .tooltip(|_, cx| cx.new(|_| Tooltip("数字格式".into())).into())
                            .on_click(pop(Pop::Num, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.))
                            .child(small("mo-cny", "¥", "人民币", st.num_fmt.contains('¥'), cx, "num:\"¥\"#,##0.00"))
                            .child(small("mo-pct", "%", "百分比 ⌃⇧5", st.num_fmt.contains('%'), cx, "num:0.00%"))
                            .child(Self::ib("mo-inc", "sh-dec-inc", "增加小数位数", false, t).size(px(26.)).on_click(Self::cmd("dec-inc", cx)))
                            .child(Self::ib("mo-dec", "sh-dec-dec", "减少小数位数", false, t).size(px(26.)).on_click(Self::cmd("dec-dec", cx))),
                    ),
            )
            .child(Self::vsep(t))
            .child(
                Self::big("mo-freeze", "sh-freeze", "冻结", true, frozen, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("冻结".into())).into())
                    .on_click(pop(Pop::Freeze, cx)),
            )
            .child(
                Self::big("mo-filter", "sh-filter", "筛选", true, filter_on, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("筛选 ⌃⇧L".into())).into())
                    .on_click(pop(Pop::FilterMenu, cx)),
            )
            .child(
                Self::big("mo-sort", "sh-sort-asc", "排序", false, false, t)
                    .when(!has_data, |d| d.opacity(0.4))
                    .tooltip(|_, cx| cx.new(|_| Tooltip("排序 ⌥⇧F".into())).into())
                    .when(has_data, |d| d.on_click(pop(Pop::SortMenu, cx))),
            )
            .child(
                Self::big("mo-cond", "sh-cond", "条件格式", true, false, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("条件格式".into())).into())
                    .on_click(pop(Pop::Cond, cx)),
            )
            .child(
                Self::big("mo-list", "sh-dropdown", "下拉列表", false, false, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("下拉列表".into())).into())
                    .on_click(Self::cmd("ins-list", cx)),
            )
            .child(
                Self::big("mo-fx", "sh-sigma", "公式", true, false, t)
                    .tooltip(|_, cx| cx.new(|_| Tooltip("公式".into())).into())
                    .on_click(pop(Pop::Formula, cx)),
            )
    }

    pub(crate) fn select_box(id: &'static str, label: impl Into<SharedString>, w: f32, t: &Theme) -> Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .w(px(w))
            .h(px(28.))
            .px(px(8.))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .text_size(px(13.))
            .child(div().overflow_hidden().whitespace_nowrap().child(label.into()))
            .child(icon("sh-chevron", 12., t.text_muted))
    }

    pub(crate) fn color_btn(id: &'static str, ic: &'static str, color: Hsla, tip: &'static str, pop: Pop, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        Self::color_btn_raw(id, ic, color, tip, t).on_click(Self::pop_btn(pop, cx))
    }

    pub(crate) fn color_btn_raw(id: &'static str, ic: &'static str, color: Hsla, tip: &'static str, t: &Theme) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .h(px(28.))
            .px(px(3.))
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .child(div().flex().flex_col().items_center().child(icon(ic, 15., t.text)).child(div().w(px(16.)).h(px(3.)).mt(px(1.)).rounded(px(1.)).bg(color)))
            .child(icon("sh-chevron", 10., t.text_muted))
            .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
    }

    // ───────────── 弹出层内容 ─────────────

    fn pop_body(&mut self, kind: Pop, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let st = self.cur_style();
        // 级联菜单每级最高多少：右键菜单可整体上移，用整个窗口高；工具栏下拉从锚点往下
        let vh = f32::from(window.viewport_size().height);
        let at_y = self.pop.as_ref().map_or(0., |p| f32::from(p.at.y));
        let max_h = |ctx: bool| if ctx { vh - 16. } else { (vh - at_y - 8.).max(240.) };
        match kind {
            Pop::Menu => {
                let root = self.menu_root();
                let w = if self.menu_kind == super::menu::MenuKind::Main { 150. } else { 260. };
                self.cascade(root, w, max_h(false), t, cx)
            }
            Pop::Insert => Self::panel(t)
                .w(px(196.))
                .child(self.item("i-check", Some("sh-checkbox"), "复选框", "", "ins-check", t, cx))
                .child(self.item("i-list", Some("sh-dropdown"), "下拉列表", "", "ins-list", t, cx))
                .child(self.item("i-date", Some("sh-date"), "日期选项", "", "ins-date", t, cx))
                .child(self.item("i-remind", Some("sh-remind"), "日期提醒", "", "ins-remind", t, cx))
                .child(Self::sep(t))
                .child(self.item("i-image", Some("sh-image"), "图片", "", "ins-image", t, cx))
                .child(self.item("i-spark", Some("sh-spark"), "迷你图", "", "ins-spark", t, cx))
                .child(Self::sep(t))
                .child(self.item("i-link", Some("sh-link"), "链接", "⌘K", "ins-link", t, cx))
                .child(self.item("i-file", Some("sh-attach"), "附件", "", "ins-file", t, cx))
                .child(Self::sep(t))
                .child(self.item("i-note", Some("sh-note"), "备注", "", "ins-note", t, cx))
                .into_any_element(),
            Pop::Font => {
                let cur = font_label(&st.font.name);
                Self::panel(t)
                    .w(px(200.))
                    .children(FONTS.iter().enumerate().map(|(i, (label, fam))| {
                        let fam_s = if fam.is_empty() { "PingFang SC" } else { fam };
                        Self::menu_row(("font", i), (*label == cur).then_some("sh-check"), *label, "", t)
                            .font_family(fam_s)
                            .on_click(Self::cmd(format!("font:{fam}"), cx))
                    }))
                    .into_any_element()
            }
            Pop::Size => Self::panel(t)
                .w(px(90.))
                .children(SIZES.iter().map(|s| {
                    Self::menu_row(("size", *s as usize), (*s == st.font.sz).then_some("sh-check"), s.to_string(), "", t)
                        .on_click(Self::cmd(format!("size:{s}"), cx))
                }))
                .into_any_element(),
            Pop::Border => self.border_panel(t, cx).into_any_element(),
            Pop::Color => self.color_panel("color", t, cx).into_any_element(),
            Pop::Fill => self.color_panel("fill", t, cx).into_any_element(),
            Pop::Merge => Self::panel(t)
                .w(px(180.))
                .child(self.item("mg-all", Some("sh-merge"), "合并单元格", "⌥⇧D", "merge", t, cx))
                .child(self.item("mg-rows", None, "按行合并", "", "merge-rows", t, cx))
                .child(self.item("mg-cols", None, "按列合并", "", "merge-cols", t, cx))
                .child(Self::sep(t))
                .child(self.item("mg-un", None, "取消合并", "", "unmerge", t, cx))
                .into_any_element(),
            Pop::Num | Pop::Freeze | Pop::Cond | Pop::Formula | Pop::FilterMenu => {
                let root = self.menu_root();
                let w = match kind {
                    Pop::Num => 300.,
                    Pop::Freeze => 270.,
                    Pop::Formula => 220.,
                    Pop::FilterMenu => 180.,
                    _ => 190.,
                };
                self.cascade(root, w, max_h(false), t, cx)
            }
            Pop::More => Self::panel(t).px(px(8.)).py(px(6.)).child(self.more_group(false, t, cx)).into_any_element(),
            Pop::SortMenu => Self::panel(t)
                .w(px(180.))
                .child(self.item("so-asc", Some("sh-sort-asc"), "升序 A→Z", "⌥⇧F", "sort-asc", t, cx))
                .child(self.item("so-desc", Some("sh-sort-desc"), "降序 Z→A", "", "sort-desc", t, cx))
                .child(Self::sep(t))
                .child(self.item("so-custom", Some("sh-list"), "自定义排序", "", "sort-custom", t, cx))
                .into_any_element(),
            Pop::Ctx | Pop::RowCtx | Pop::ColCtx => {
                let root = self.ctx_items(kind);
                self.cascade(root, 240., max_h(true), t, cx)
            }
            Pop::Tab(i) => {
                let n = self.model.borrow().get_worksheets_properties().len() as u32;
                Self::panel(t)
                    .w(px(200.))
                    .child(self.item("t-rename", Some("sh-rename"), "重命名", "", format!("rename-sheet:{i}"), t, cx))
                    .child(self.item("t-dup", Some("sh-dup"), "创建副本", "", format!("dup-sheet:{i}"), t, cx))
                    .when(i > 0, |d| d.child(self.item("t-left", Some("sh-left"), "左移", "", format!("move-sheet:{i},{}", i as i64 - 1), t, cx)))
                    .when(i + 1 < n, |d| d.child(self.item("t-right", Some("sh-right"), "右移", "", format!("move-sheet:{i},{}", i + 1), t, cx)))
                    .child(Self::section("标签颜色", t))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(5.))
                            .px(px(12.))
                            .py(px(4.))
                            .children(PALETTE[1].iter().enumerate().map(|(k, hex)| {
                                div()
                                    .id(("tc", k))
                                    .size(px(18.))
                                    .rounded_full()
                                    .bg(hex_to_hsla(hex).unwrap())
                                    .cursor_pointer()
                                    .on_click(Self::cmd(format!("tab-color:{i},{hex}"), cx))
                            }))
                            .child(
                                div()
                                    .id("tc-none")
                                    .size(px(18.))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(t.border_strong)
                                    .cursor_pointer()
                                    .on_click(Self::cmd(format!("tab-color:{i},"), cx)),
                            ),
                    )
                    .when(n > 1, |d| d.child(Self::sep(t)).child(self.item("t-del", Some("sh-trash"), "删除", "", format!("del-sheet:{i}"), t, cx)))
                    .into_any_element()
            }
            Pop::Sheets => {
                let sheets = self.model.borrow().get_worksheets_properties();
                Self::panel(t)
                    .w(px(200.))
                    .children(sheets.iter().enumerate().map(|(i, s)| {
                        Self::menu_row(("sl", i), (i as u32 == self.sheet).then_some("sh-check"), s.name.clone(), "", t)
                            .on_click(Self::cmd(format!("sheet:{i}"), cx))
                    }))
                    .into_any_element()
            }
            Pop::FilterCol(c) => self.filter_panel(c, t, cx).into_any_element(),
            Pop::Zoom => Self::panel(t)
                .w(px(100.))
                .children([50, 75, 90, 100, 125, 150, 200, 300].into_iter().map(|z| {
                    let on = (self.zoom * 100.).round() as i32 == z;
                    Self::menu_row(("zm", z as usize), on.then_some("sh-check"), format!("{z}%"), "", t).on_click(Self::cmd(format!("zoom:{z}"), cx))
                }))
                .into_any_element(),
            Pop::Align => {
                let st = self.cur_style();
                let al = st.alignment.clone();
                let hz = al.as_ref().map(|a| a.horizontal.clone()).unwrap_or_default();
                let vt = al.as_ref().map(|a| a.vertical.clone());
                Self::panel(t)
                    .p(px(6.))
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.))
                            .child(Self::ib("al-hl", "sh-h-left", "左对齐 ⇧⌘L", matches!(hz, H::Left | H::General), t).on_click(Self::cmd("h-left", cx)))
                            .child(Self::ib("al-hc", "sh-h-center", "水平居中 ⇧⌘E", matches!(hz, H::Center), t).on_click(Self::cmd("h-center", cx)))
                            .child(Self::ib("al-hr", "sh-h-right", "右对齐 ⇧⌘R", matches!(hz, H::Right), t).on_click(Self::cmd("h-right", cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.))
                            .child(Self::ib("al-vt", "sh-v-top", "顶部对齐", matches!(vt, Some(V::Top)), t).on_click(Self::cmd("v-top", cx)))
                            .child(
                                Self::ib("al-vm", "sh-v-mid", "垂直居中", vt.is_none() || matches!(vt, Some(V::Center)), t).on_click(Self::cmd("v-middle", cx)),
                            )
                            .child(Self::ib("al-vb", "sh-v-bottom", "底部对齐", matches!(vt, Some(V::Bottom)), t).on_click(Self::cmd("v-bottom", cx))),
                    )
                    .into_any_element()
            }
            Pop::Wrap => {
                let wrap = self.cur_style().alignment.is_some_and(|a| a.wrap_text);
                let clip = self.sx().is_clip(self.cur.r, self.cur.c);
                Self::panel(t)
                    .w(px(140.))
                    .child(self.item("wr-ov", Some("sh-overflow"), "溢出", "", "overflow", t, cx).when(!wrap && !clip, |d| d.text_color(t.accent)))
                    .child(self.item("wr-cl", Some("sh-clip"), "截断", "", "clip", t, cx).when(clip, |d| d.text_color(t.accent)))
                    .child(self.item("wr-wr", Some("sh-wrap"), "自动换行", "", "wrap", t, cx).when(wrap, |d| d.text_color(t.accent)))
                    .into_any_element()
            }
            Pop::Hl => {
                // 高亮所在行列：第一格「无」，其后浅色
                const HL: [(&str, &str); 12] = [
                    ("", "无"),
                    ("#bacefd", "浅蓝色"),
                    ("#a9e6fa", "浅青色"),
                    ("#b7edb1", "浅绿色"),
                    ("#ffe28f", "浅黄色"),
                    ("#fed4a4", "浅橙色"),
                    ("#fbbfbc", "浅红色"),
                    ("#cdb2fa", "浅紫色"),
                    ("#dee0e3", "浅灰色"),
                    ("#3370ff", "蓝色"),
                    ("#34c724", "绿色"),
                    ("#f54a45", "红色"),
                ];
                let cur = super::view_prefs(&self.obj_id).hl.unwrap_or_default();
                Self::panel(t)
                    .p(px(8.))
                    .flex_row()
                    .flex_wrap()
                    .w(px(176.))
                    .gap(px(6.))
                    .children(HL.iter().enumerate().map(|(i, (hex, name))| {
                        let on = cur == *hex;
                        let (hex, name) = (*hex, *name);
                        div()
                            .id(("hl", i))
                            .size(px(22.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(if on { t.accent } else { t.border })
                            .when(!hex.is_empty(), |d| d.bg(hex_to_hsla(hex).unwrap().opacity(0.6)))
                            .when(hex.is_empty(), |d| {
                                d.child(div().absolute().w(px(28.)).h(px(1.)).bg(rgb(0xf54a45)).top(px(10.)).left(px(-3.))).relative().overflow_hidden()
                            })
                            .cursor_pointer()
                            .tooltip(move |_, cx| cx.new(|_| Tooltip(name.into())).into())
                            .on_click(Self::cmd(format!("hl:{hex}"), cx))
                    }))
                    .into_any_element()
            }
            Pop::Shortcuts => div().into_any_element(),
        }
    }

    pub(crate) fn check(on: bool, t: &Theme) -> Div {
        div()
            .size(px(14.))
            .flex_none()
            .rounded(px(3.))
            .border_1()
            .border_color(if on { t.accent } else { t.border_strong })
            .when(on, |d| d.bg(t.accent))
            .flex()
            .items_center()
            .justify_center()
            .when(on, |d| d.child(icon("sh-check", 11., white())))
    }

    fn filter_panel(&mut self, c: i32, t: &Theme, cx: &mut Context<Self>) -> Div {
        let vals = self.filter_values(c);
        let excl = self.filter.as_ref().and_then(|f| f.excl.get(&c)).cloned().unwrap_or_default();
        let all_on = excl.is_empty();
        Self::panel(t)
            .w(px(250.))
            .child(self.item("fl-asc", Some("sh-sort-asc"), "升序", "", format!("sort-asc:{c}"), t, cx))
            .child(self.item("fl-desc", Some("sh-sort-desc"), "降序", "", format!("sort-desc:{c}"), t, cx))
            .child(Self::sep(t))
            .child(Self::section("按值筛选", t))
            .child(
                div()
                    .id("fl-all")
                    .mx(px(4.))
                    .px(px(8.))
                    .h(px(30.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(Self::check(all_on, t))
                    .child("全选")
                    .on_click(cx.listener(move |this, _, _, cx| this.filter_toggle_value(c, None, cx))),
            )
            .child(div().id("fl-list").max_h(px(260.)).overflow_y_scroll().children(vals.into_iter().enumerate().map(|(i, (v, n))| {
                let on = !excl.contains(&v);
                let label: SharedString = if v.is_empty() { "（空白）".into() } else { v.clone().into() };
                div()
                    .id(("fv", i))
                    .mx(px(4.))
                    .px(px(8.))
                    .h(px(30.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(Self::check(on, t))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().child(label))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child(n.to_string()))
                    .on_click(cx.listener(move |this, _, _, cx| this.filter_toggle_value(c, Some(v.clone()), cx)))
            })))
            .child(Self::sep(t))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .px(px(10.))
                    .py(px(4.))
                    .child(
                        div()
                            .id("fl-clear")
                            .flex_1()
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(t.border)
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .child("清除条件")
                            .on_click(Self::cmd(format!("filter-clear:{c}"), cx)),
                    )
                    .child(
                        div()
                            .id("fl-ok")
                            .flex_1()
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(t.accent)
                            .text_color(white())
                            .cursor_pointer()
                            .child("确定")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pop = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    fn shortcuts_modal(&self, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        const GROUPS: [(&str, &[(&str, &str)]); 5] = [
            (
                "编辑",
                &[
                    ("撤销 / 重做", "⌘Z / ⇧⌘Z"),
                    ("复制 / 剪切 / 粘贴", "⌘C / ⌘X / ⌘V"),
                    ("仅粘贴值 / 格式", "⇧⌘V / ⌥⌘V"),
                    ("编辑单元格", "F2 / ↵"),
                    ("向下 / 向右填充", "⌘D / ⌘R"),
                    ("合并 / 拆分单元格", "⌥⇧D"),
                    ("插入当天日期 / 时刻", "⌘; / ⇧⌘;"),
                    ("向上 / 向下插入行", "⌃⇧= / ⌃⇧]"),
                    ("删除行列", "⌘⌥-"),
                    ("隐藏行 / 列", "⌘⌥9 / ⌘⌥0"),
                    ("取消隐藏行 / 列", "⇧⌘9 / ⇧⌘0"),
                ],
            ),
            (
                "格式",
                &[
                    ("加粗 / 斜体 / 下划线", "⌘B / ⌘I / ⌘U"),
                    ("删除线", "⇧⌘X"),
                    ("左 / 中 / 右对齐", "⇧⌘L / ⇧⌘E / ⇧⌘R"),
                    ("格式刷", "⌃⌘C"),
                    ("清除格式", "⌘\\"),
                    ("数字格式", "⌃⇧1 ~ 6"),
                    ("上 / 右 / 下 / 左边框", "⌥⇧1 / 2 / 3 / 4"),
                    ("取消边框", "⌥⇧5"),
                ],
            ),
            (
                "选择与导航",
                &[
                    ("全选", "⌘A"),
                    ("选中整行 / 整列", "⇧空格 / ⌥空格"),
                    ("移到数据区边缘", "⌘ + 方向键"),
                    ("扩展选区至边缘", "⇧⌘ + 方向键"),
                    ("行首 / 行尾", "Home / End"),
                    ("表头 / 表尾", "⌘Home / ⌘End"),
                    ("上 / 下一个工作表", "⌘⌥↑ / ⌘⌥↓"),
                    ("滚动到活动格", "⌘⌫"),
                ],
            ),
            ("公式", &[("自动求和", "⌘⌥="), ("切换绝对 / 相对引用", "F4"), ("显示所有公式", "⌃`"), ("补全函数", "Tab / ↵")]),
            ("查看", &[("查找", "⌘F"), ("查找和替换", "⇧⌘H"), ("筛选", "⌃⇧L"), ("排序", "⌥⇧F"), ("放大 / 缩小 / 还原", "⌘+ / ⌘- / ⌘0"), ("快捷键", "⌘/")]),
        ];
        div()
            .id("keys-mask")
            .absolute()
            .inset_0()
            .bg(t.overlay)
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| {
                this.pop = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("keys")
                    .w(px(760.))
                    .max_h(relative(0.85))
                    .overflow_y_scroll()
                    .bg(t.panel_bg)
                    .rounded(px(12.))
                    .shadow_lg()
                    .p(px(20.))
                    .text_size(px(13.))
                    .text_color(t.text)
                    .font_family("PingFang SC")
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).mb(px(12.)).child("表格快捷键"))
                    .child(div().flex().flex_row().flex_wrap().gap(px(16.)).children(GROUPS.iter().map(|(g, list)| {
                        div().w(px(345.)).flex().flex_col().gap(px(4.)).child(div().text_color(t.text_muted).text_size(px(12.)).mb(px(2.)).child(*g)).children(
                            list.iter().map(|(k, v)| {
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .py(px(3.))
                                    .border_b_1()
                                    .border_color(t.border)
                                    .child(*k)
                                    .child(div().font_family("Menlo").text_size(px(12.)).text_color(t.text_secondary).child(*v))
                            }),
                        )
                    }))),
            )
    }

    /// 查找和替换（右上浮动面板；查找 / 替换为 / 范围 / 搜索范围包括公式；可收起成紧凑条）。
    fn find_panel(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let f = self.find.as_ref()?;
        let count = if f.matches.is_empty() { "0/0".to_string() } else { format!("{}/{}", f.idx + 1, f.matches.len()) };
        let has_q = !f.q.read(cx).text().is_empty();
        let opt = |id: &'static str, label: &'static str, on: bool, which: u8, cx: &mut Context<Self>| {
            div().id(id).flex().flex_row().items_center().gap(px(6.)).cursor_pointer().child(Self::check(on, t)).child(label).on_click(cx.listener(
                move |this, _, _, cx| {
                    if let Some(f) = &mut this.find {
                        match which {
                            0 => f.case = !f.case,
                            1 => f.whole = !f.whole,
                            _ => f.formulas = !f.formulas,
                        }
                    }
                    this.refresh_find(cx);
                },
            ))
        };
        let btn = |id: &'static str, label: &'static str, primary: bool, enabled: bool| {
            div()
                .id(id)
                .px(px(14.))
                .h(px(30.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(if primary { t.accent } else { t.border_strong })
                .when(primary, |d| d.bg(t.accent).text_color(white()))
                .when(!enabled, |d| d.opacity(0.4))
                .cursor_pointer()
                .child(label)
        };
        let qbox = div()
            .flex_1()
            .h(px(32.))
            .px(px(8.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(t.accent)
            .child(icon("sh-search", 14., t.text_muted))
            .child(div().flex_1().line_height(px(30.)).overflow_hidden().child(f.q.clone()))
            .child(Self::ib("f-prev", "chevron-left", "上一个 ⇧↵", false, t).size(px(20.)).on_click(cx.listener(|this, _, _, cx| this.find_step(-1, cx))))
            .child(div().text_size(px(12.)).text_color(t.text_secondary).child(count))
            .child(Self::ib("f-next", "chevron-right", "下一个 ↵", false, t).size(px(20.)).on_click(cx.listener(|this, _, _, cx| this.find_step(1, cx))));
        let head = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .when(!f.compact, |d| d.child(div().w(px(52.)).child("查找")))
            .child(qbox)
            .child(Self::ib("f-compact", if f.compact { "expand" } else { "fold-vertical" }, if f.compact { "展开" } else { "收起" }, false, t).on_click(
                cx.listener(|this, _, _, cx| {
                    if let Some(f) = &mut this.find {
                        f.compact = !f.compact;
                    }
                    cx.notify();
                }),
            ))
            .child(Self::ib("f-close", "sh-x", "关闭 Esc", false, t).on_click(cx.listener(|this, _, w, cx| this.close_find(w, cx))));
        const SCOPES: [&str; 3] = ["当前工作表", "全部工作表", "选区"];
        let input = |e: &Entity<LineInput>| {
            div()
                .flex_1()
                .h(px(32.))
                .px(px(8.))
                .rounded(px(6.))
                .border_1()
                .border_color(t.border_strong)
                .line_height(px(30.))
                .overflow_hidden()
                .child(e.clone())
        };
        let panel = div()
            .absolute()
            .top(px(HEAD_H + 8.))
            .right(px(18.))
            .w(px(if f.compact { 360. } else { 470. }))
            .occlude()
            .bg(t.panel_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(10.))
            .shadow_lg()
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .text_size(px(13.))
            .text_color(t.text)
            .child(head);
        if f.compact {
            return Some(panel);
        }
        let scope = f.scope;
        Some(
            panel
                .child(div().flex().flex_row().items_center().gap(px(6.)).child(div().w(px(52.)).child("替换为")).child(input(&f.rep)))
                .child(
                    div().flex().flex_row().items_start().gap(px(6.)).child(div().w(px(52.)).mt(px(6.)).child("范围")).child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(crate::base::widgets::select_box("f-scope", SCOPES[scope as usize], None, t).h(px(32.)).on_click(cx.listener(
                                |this, _, _, cx| {
                                    if let Some(f) = &mut this.find {
                                        f.scope_open = !f.scope_open;
                                    }
                                    cx.notify();
                                },
                            )))
                            .when(f.scope_open, |d| {
                                d.child(div().border_1().border_color(t.border).rounded(px(6.)).children(SCOPES.iter().enumerate().map(|(i, name)| {
                                    div()
                                        .id(("f-sc", i))
                                        .px(px(10.))
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(t.hover))
                                        .when(i == scope as usize, |d| d.text_color(t.accent))
                                        .child(*name)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(f) = &mut this.find {
                                                f.scope = i as u8;
                                                f.scope_open = false;
                                            }
                                            this.refresh_find(cx);
                                        }))
                                })))
                            }),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(16.))
                        .pl(px(58.))
                        .child(opt("fo-fx", "搜索范围包括公式", f.formulas, 2, cx))
                        .child(opt("fo-case", "区分大小写", f.case, 0, cx))
                        .child(opt("fo-whole", "单元格完全匹配", f.whole, 1, cx)),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap(px(8.))
                        .child(btn("f-find", "查找", false, has_q).on_click(cx.listener(|this, _, _, cx| this.find_step(1, cx))))
                        .child(btn("f-rep", "替换", false, has_q).on_click(cx.listener(|this, _, _, cx| this.replace_one(cx))))
                        .child(btn("f-repall", "全部替换", true, has_q).on_click(cx.listener(|this, _, _, cx| this.replace_all(cx)))),
                ),
        )
    }

    /// 编辑框下方：函数补全列表或参数提示。
    fn formula_help(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let text = self.editing_text(cx)?;
        if !text.starts_with('=') {
            return None;
        }
        let list = self.completions(cx);
        if !list.is_empty() {
            let sel = self.comp_sel.min(list.len() - 1);
            return Some(
                Self::panel(t)
                    .w(px(340.))
                    .children(list.into_iter().enumerate().map(|(i, f)| {
                        let on = i == sel;
                        div()
                            .id(("comp", i))
                            .mx(px(4.))
                            .px(px(8.))
                            .py(px(5.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(on, |d| d.bg(t.accent_soft))
                            .hover(|s| s.bg(t.hover))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap(px(6.))
                                    .child(div().font_weight(FontWeight::SEMIBOLD).text_color(if on { t.accent } else { t.text }).child(f.name))
                                    .child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("({})", f.args))),
                            )
                            .when(on, |d| d.child(div().text_size(px(12.)).text_color(t.text_secondary).child(f.desc)))
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.comp_sel = i;
                                this.accept_completion(w, cx);
                            }))
                    }))
                    .child(div().px(px(12.)).py(px(3.)).text_size(px(11.)).text_color(t.text_muted).child("↑↓ 选择 · Tab / ↵ 插入")),
            );
        }
        let (f, arg) = funcs::current_call(&text)?;
        let args: Vec<&str> = f.args.split(", ").collect();
        let cur = arg.min(args.len().saturating_sub(1));
        Some(
            Self::panel(t)
                .px(px(12.))
                .py(px(8.))
                .gap(px(4.))
                .max_w(px(420.))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(format!("{}(", f.name)))
                        .children(args.iter().enumerate().map(|(i, a)| {
                            let mut d = div().child(format!("{a}{}", if i + 1 < args.len() { ", " } else { "" }));
                            if i == cur && args.len() > 1 || (args[cur].contains("...") && i == args.len() - 1 && arg >= args.len()) {
                                d = d.text_color(t.accent).font_weight(FontWeight::SEMIBOLD);
                            }
                            d
                        }))
                        .child(")"),
                )
                .child(div().text_size(px(12.)).text_color(t.text_secondary).child(f.desc)),
        )
    }

    fn status_text(&self) -> Option<String> {
        let a = self.sel_used();
        if a.width * a.height <= 1 {
            return None;
        }
        let m = self.model.borrow();
        let (mut sum, mut n, mut cnt) = (0f64, 0usize, 0usize);
        let (mut mn, mut mx) = (f64::MAX, f64::MIN);
        let mut seen = 0usize;
        'outer: for r in a.row..a.row + a.height {
            if self.is_hidden_row(r) {
                continue;
            }
            for c in a.column..a.column + a.width {
                seen += 1;
                if seen > 200_000 {
                    break 'outer;
                }
                match m.get_model().get_cell_value_by_index(self.sheet, r, c) {
                    Ok(ironcalc::base::cell::CellValue::Number(v)) => {
                        sum += v;
                        n += 1;
                        cnt += 1;
                        mn = mn.min(v);
                        mx = mx.max(v);
                    }
                    Ok(ironcalc::base::cell::CellValue::None) | Err(_) => {}
                    Ok(_) => cnt += 1,
                }
            }
        }
        if cnt == 0 {
            return None;
        }
        if n == 0 {
            return Some(format!("计数：{cnt}"));
        }
        Some(format!(
            "求和：{}　平均值：{}　计数：{cnt}　最小值：{}　最大值：{}",
            funcs::fmt_num(sum),
            funcs::fmt_num(sum / n as f64),
            funcs::fmt_num(mn),
            funcs::fmt_num(mx)
        ))
    }

    fn tabs_bar(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let sheets = self.model.borrow().get_worksheets_properties();
        let colors: Vec<Option<Hsla>> = sheets.iter().map(|s| hex_to_hsla(&self.model.borrow().resolve_color(&s.color))).collect();
        let status = self.status_text();
        let zoom = (self.zoom * 100.).round() as i32;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .px(px(8.))
            .h(px(36.))
            .border_t_1()
            .border_color(t.border)
            .bg(t.sidebar_bg)
            .child(Self::ib("tab-list", "sh-list", "工作表列表", false, t).on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                let p = ev.position();
                let n = this.model.borrow().get_worksheets_properties().len() as f32;
                this.open_pop(Pop::Sheets, point(p.x, p.y - px(8.) - px(32. * n + 10.)), cx)
            })))
            .child(Self::ib("tab-add", "sh-plus", "新建工作表 ⇧F11", false, t).on_click(Self::cmd("new-sheet", cx)))
            .child(div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(t.border))
            .children(sheets.iter().enumerate().map(|(i, sp)| {
                let on = i as u32 == self.sheet;
                let renaming = self.rename.as_ref().filter(|r| r.0 == i as u32).map(|r| r.1.clone());
                let color = colors[i];
                div()
                    .id(("tab", i))
                    .relative()
                    .px(px(14.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(on, |d| d.bg(t.bg).text_color(t.accent).font_weight(FontWeight::MEDIUM).shadow_sm())
                    .when(!on, |d| d.text_color(t.text_secondary).hover(|s| s.bg(t.hover)))
                    .when_some(renaming.clone(), |d, inp| d.min_w(px(90.)).child(inp))
                    .when(renaming.is_none(), |d| d.child(sp.name.clone()))
                    .when_some(color, |d, c| d.child(div().absolute().bottom(px(1.)).left(px(8.)).right(px(8.)).h(px(3.)).rounded(px(2.)).bg(c)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            this.switch_sheet(i as u32, cx);
                            this.pop = Some(Popup { kind: Pop::Tab(i as u32), at: point(ev.position.x, ev.position.y - px(250.)) });
                            cx.notify();
                        }),
                    )
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        if this.sheet != i as u32 {
                            this.switch_sheet(i as u32, cx);
                        }
                        if ev.click_count() >= 2 {
                            this.start_rename(i as u32, window, cx);
                        }
                    }))
            }))
            .child(div().flex_1())
            .when_some(status, |d, s| d.child(div().text_size(px(12.)).text_color(t.text_secondary).mr(px(16.)).child(s)))
            .when_some(self.painter.as_ref(), |d, p| {
                d.child(div().text_size(px(12.)).text_color(t.accent).mr(px(12.)).child(if p.1 { "格式刷（连续）" } else { "格式刷" }))
            })
            .child(
                Self::ib(
                    "bb-grid",
                    "sh-grid",
                    if self.model.borrow().get_show_grid_lines(self.sheet).unwrap_or(true) { "隐藏网格线" } else { "显示网格线" },
                    !self.model.borrow().get_show_grid_lines(self.sheet).unwrap_or(true),
                    t,
                )
                .size(px(24.))
                .on_click(Self::cmd("grid", cx)),
            )
            .child(
                div()
                    .id("bb-hl")
                    .flex()
                    .flex_row()
                    .items_center()
                    .h(px(24.))
                    .px(px(3.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .when(super::view_prefs(&self.obj_id).hl.is_some(), |d| d.bg(t.accent_soft))
                    .hover(|s| s.bg(t.hover))
                    .child(icon("sh-highlight", 16., if super::view_prefs(&self.obj_id).hl.is_some() { t.accent } else { t.text }))
                    .child(icon("sh-chevron", 10., t.text_muted))
                    .tooltip(|_, cx| cx.new(|_| Tooltip("高亮所在行列".into())).into())
                    .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                        let p = ev.position();
                        this.open_pop(Pop::Hl, point(p.x - px(150.), p.y - px(90.)), cx)
                    })),
            )
            .child(div().w(px(1.)).h(px(16.)).mx(px(6.)).bg(t.border))
            .child(Self::ib("z-out", "sh-minus", "缩小 ⌘-", false, t).size(px(22.)).on_click(Self::cmd("zoom-out", cx)))
            .child(
                div()
                    .id("z-val")
                    .w(px(46.))
                    .flex()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(t.text_secondary)
                    .cursor_pointer()
                    .child(format!("{zoom}%"))
                    .tooltip(|_, cx| cx.new(|_| Tooltip("缩放比例".into())).into())
                    .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                        let p = ev.position();
                        this.open_pop(Pop::Zoom, point(p.x - px(40.), p.y - px(250.)), cx)
                    })),
            )
            .child(Self::ib("z-in", "sh-plus", "放大 ⌘+", false, t).size(px(22.)).on_click(Self::cmd("zoom-in", cx)))
    }
}

impl Render for SheetEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx).clone();
        if !matches!(self.cpop, Some(super::objs::CellPop::FileView { .. })) {
            self.release_file_view();
        }
        if self.embedded && self.bounds.size.width > px(0.) {
            // 内嵌：内容放得下就不滚动
            let (cw, ch) = self.embed_content();
            let (vw, vh) = (f32::from(self.bounds.size.width) - HEAD_W, f32::from(self.bounds.size.height) - HEAD_H);
            self.scroll = (self.scroll.0.clamp(0., (cw - vw).max(0.)), self.scroll.1.clamp(0., (ch - vh).max(0.)));
        }
        let frame = self.build_frame(&t, window, cx);
        let entity = cx.entity();
        // 提示条 2.5 秒后自动消失
        if self.toast.is_some() && !self.toast_timer {
            self.toast_timer = true;
            let msg = self.toast.clone();
            cx.spawn(async move |this: WeakEntity<SheetEditor>, cx: &mut AsyncApp| {
                cx.background_executor().timer(std::time::Duration::from_millis(2500)).await;
                this.update(cx, |this, cx| {
                    this.toast_timer = false;
                    if this.toast == msg {
                        this.toast = None;
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        let edit_overlay = self.editing.as_ref().map(|e| {
            let r = self.cell_rect(self.cur);
            let text = e.input.read(cx).text();
            let est = funcs::text_width_est(&text, 13. * self.zoom) + 24.;
            let typed = e.typed;
            div()
                .absolute()
                .left(r.origin.x - self.bounds.origin.x - px(1.))
                .top(r.origin.y - self.bounds.origin.y - px(1.))
                .min_w(r.size.width + px(2.))
                .w(px(est).max(r.size.width + px(2.)))
                .max_w(px(640.))
                .h(r.size.height.max(px(24.)) + px(2.))
                .px(px(5.))
                .flex()
                .items_center()
                .bg(t.panel_bg)
                .border_2()
                .border_color(t.accent)
                .shadow_md()
                .text_size(px(13. * self.zoom))
                .occlude()
                // 「输入模式」下左右方向键结束编辑并移动（Excel / 同类）
                .capture_key_down(cx.listener(move |this, ev: &KeyDownEvent, w, cx| {
                    let k = ev.keystroke.key.as_str();
                    let m = &ev.keystroke.modifiers;
                    if typed && (k == "left" || k == "right") && !m.shift && !m.platform && !m.alt && !m.control {
                        cx.stop_propagation();
                        this.commit_edit(cx);
                        this.move_by(0, if k == "left" { -1 } else { 1 }, false, cx);
                        w.focus(&this.focus);
                    }
                }))
                .child(e.input.clone())
        });
        let help = self.formula_help(&t, cx).map(|h| {
            let r = self.cell_rect(self.cur);
            div()
                .absolute()
                .left(r.origin.x - self.bounds.origin.x - px(1.))
                .top(r.origin.y - self.bounds.origin.y + r.size.height.max(px(24.)) + px(4.))
                .child(h)
        });
        let pop = self.pop.as_ref().map(|p| (p.kind, p.at));
        let pop_el = pop.filter(|(k, _)| *k != Pop::Shortcuts).map(|(k, at)| {
            let body = self.pop_body(k, &t, window, cx);
            deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(div().id("pop").child(body).on_mouse_down_out(cx.listener(
                |this, _, _, cx| {
                    this.pop = None;
                    cx.notify();
                },
            ))))
            .with_priority(5)
        });
        let prompt_el = self.prompt.as_ref().map(|p| {
            let title = match p.kind {
                PromptKind::NumFmt => "自定义数字格式（如 0.00、#,##0、yyyy-mm-dd）",
                PromptKind::Cf("gt") => "突出显示：大于",
                PromptKind::Cf("lt") => "突出显示：小于",
                PromptKind::Cf("eq") => "突出显示：等于",
                PromptKind::Cf("between") => "突出显示：介于（最小值, 最大值）",
                PromptKind::Cf(_) => "突出显示：文本包含",
            };
            let input = p.input.clone();
            deferred(
                anchored().position(p.at).snap_to_window_with_margin(px(8.)).child(
                    Self::panel(&t)
                        .id("prompt")
                        .w(px(240.))
                        .p(px(12.))
                        .gap(px(8.))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(title))
                        .child(div().h(px(30.)).px(px(8.)).rounded(px(6.)).border_1().border_color(t.accent).line_height(px(28.)).child(input.clone()))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .id("pr-cancel")
                                        .px(px(12.))
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(t.border)
                                        .cursor_pointer()
                                        .child("取消")
                                        .on_click(cx.listener(|this, _, w, cx| {
                                            this.prompt = None;
                                            w.focus(&this.focus);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .id("pr-ok")
                                        .px(px(12.))
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.))
                                        .bg(t.accent)
                                        .text_color(white())
                                        .cursor_pointer()
                                        .child("确定")
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            let v = input.read(cx).text();
                                            this.submit_prompt(&v, cx);
                                            w.focus(&this.focus);
                                        })),
                                ),
                        )
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.prompt = None;
                            cx.notify();
                        })),
                ),
            )
            .with_priority(6)
        });
        let find_panel = self.find_panel(&t, cx);
        let cell_pop = self.cell_pop_el(&t, window, cx);
        let dlg = self.dlg_el(&t, window, cx);
        let side = match self.side {
            Some(super::dialogs::Side::Spark(c)) => self.spark_panel(c, &t, cx),
            Some(_) => self.side_panel(&t, window, cx),
            None => None,
        };
        // 底部左侧「+ [20] 行」
        let add_rows = (!self.embedded).then(|| {
            div()
                .absolute()
                .left(px(HEAD_W + 6.))
                .bottom(px(8.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .px(px(6.))
                .py(px(3.))
                .rounded(px(6.))
                .bg(t.bg.opacity(0.92))
                .text_size(px(12.))
                .text_color(t.text_secondary)
                .child(Self::ib("add-rows", "sh-plus", "在末尾添加行", false, &t).size(px(22.)).on_click(Self::cmd("add-rows", cx)))
                .child(
                    div().w(px(40.)).h(px(22.)).px(px(6.)).rounded(px(5.)).border_1().border_color(t.border).line_height(px(20.)).child(self.add_rows.clone()),
                )
                .child("行")
        });
        // 悬停显示备注
        let hover_note = self.hover_cell.filter(|c| !matches!(&self.cpop, Some(super::objs::CellPop::Note { cell, .. }) if cell == c)).and_then(|c| {
            let note = self.sx().cell(c.r, c.c)?.note.clone();
            let r = self.cell_rect(c);
            Some(
                div()
                    .absolute()
                    .left(r.right() - self.bounds.origin.x + px(4.))
                    .top(r.top() - self.bounds.origin.y)
                    .w(px(200.))
                    .min_h(px(60.))
                    .p(px(8.))
                    .bg(rgb(0xfffbd1))
                    .border_1()
                    .border_color(rgb(0xf0e6a0))
                    .shadow_md()
                    .text_size(px(13.))
                    .text_color(rgb(0x1f2329))
                    .child(note),
            )
        });
        let file_tb = self.file_toolbar(&t, cx);
        let toast = self.toast.clone();
        let painter = self.painter.is_some();
        let resize_cursor = self.resize_state().map(|(col, ..)| col);
        let cur_style = self.cur_style();
        let _ = cur_style;
        let embedded = self.embedded;
        let (emb_w, emb_h) = if embedded {
            let (w, h) = self.embed_content();
            ((HEAD_W + w + 2.).min(self.embed_max_w), (HEAD_H + h + 2.).min(480.))
        } else {
            (0., 0.)
        };
        // 内嵌：聚焦（或开着弹出层）时在表格上方浮出工具栏
        let float_tb = (embedded && (self.focus.contains_focused(window, cx) || self.pop.is_some()) && self.bounds.size.width > px(0.)).then(|| {
            let at = point(self.bounds.origin.x, self.bounds.origin.y - px(50.));
            deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(div().id("float-tb").child(self.float_toolbar(&t, cx)))).with_priority(4)
        });
        let toolbar = (!embedded).then(|| self.toolbar(&t, cx));
        let tabs = (!embedded).then(|| self.tabs_bar(&t, cx));
        let root = div()
            .key_context("Sheet")
            .track_focus(&self.focus)
            .when(!embedded, |d| d.size_full())
            .when(embedded, |d| d.w(px(emb_w)).h(px(emb_h)))
            .relative()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.text)
            .font_family("PingFang SC");
        with_actions(root, cx)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if this.editing.is_some()
                    || this.find.is_some()
                    || this.prompt.is_some()
                    || this.rename.is_some()
                    || this.dlg.is_some()
                    || this.cpop.as_ref().is_some_and(|p| !matches!(p, super::objs::CellPop::LinkCard { .. }))
                {
                    return;
                }
                let k = &ev.keystroke;
                if k.modifiers.platform || k.modifiers.control {
                    return;
                }
                this.cpop = None;
                // 空格切换复选框（同类）
                if k.key == "space" && !k.modifiers.shift && !k.modifiers.alt && this.toggle_checks_in_sel(cx) {
                    cx.stop_propagation();
                    return;
                }
                if k.key == "space" && k.modifiers.shift {
                    this.run("select-row", window, cx);
                    cx.stop_propagation();
                    return;
                }
                if k.key == "space" && k.modifiers.alt {
                    this.run("select-col", window, cx);
                    cx.stop_propagation();
                    return;
                }
                if k.modifiers.alt {
                    return;
                }
                if let Some(ch) = &k.key_char {
                    if !ch.is_empty() && !ch.chars().all(|c| c.is_control()) {
                        this.pop = None;
                        this.start_edit(Some(ch.clone()), window, cx);
                        cx.stop_propagation();
                    }
                }
            }))
            .children(toolbar)
            // ── 名称框 + 公式栏 ──
            .when(!embedded, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(px(FORMULA_H))
                        .border_b_1()
                        .border_color(t.border)
                        .child(
                            div()
                                .w(px(110.))
                                .h_full()
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .border_r_1()
                                .border_color(t.border)
                                .text_size(px(13.))
                                .font_family("Menlo")
                                .child(self.name_box.clone()),
                        )
                        .child(div().px(px(10.)).flex().items_center().child(icon("sh-fx", 16., t.text_muted)))
                        .child(div().w(px(1.)).h(px(18.)).bg(t.border))
                        .child(div().flex_1().h_full().px(px(10.)).flex().items_center().text_size(px(13.)).child(self.formula.clone())),
                )
            })
            // ── 网格（+ 右侧面板） ──
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .id("sheet-grid")
                            .flex_1()
                            .min_w(px(0.))
                            .min_h(px(0.))
                            .relative()
                            .overflow_hidden()
                            .when(painter, |d| d.cursor(CursorStyle::PointingHand))
                            .when_some(resize_cursor, |d, col| d.cursor(if col { CursorStyle::ResizeLeftRight } else { CursorStyle::ResizeUpDown }))
                            .child(
                                canvas(
                                    move |b, _, cx| {
                                        entity.update(cx, |this, _| this.bounds = b);
                                        b
                                    },
                                    move |b, _, window, cx| paint::paint(&frame, b, window, cx),
                                )
                                .size_full(),
                            )
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                            .on_mouse_down(MouseButton::Right, cx.listener(Self::right_down))
                            .on_mouse_move(cx.listener(Self::mouse_move))
                            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                if !*hovered && this.resize_hover.take().is_some() {
                                    cx.notify();
                                }
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
                            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
                            .children(edit_overlay)
                            .children(help)
                            .children(find_panel)
                            .children(add_rows)
                            .children(hover_note)
                            .children(file_tb)
                            .when_some(toast, |d, msg| {
                                d.child(
                                    div().absolute().bottom(px(18.)).left_0().right_0().flex().justify_center().child(
                                        div()
                                            .px(px(14.))
                                            .py(px(7.))
                                            .rounded(px(8.))
                                            .bg(hsla(0., 0., 0.12, 0.88))
                                            .text_color(white())
                                            .text_size(px(13.))
                                            .shadow_lg()
                                            .child(msg),
                                    ),
                                )
                            }),
                    )
                    .children(side),
            )
            .children(tabs)
            .children(float_tb)
            .children(pop_el)
            .children(prompt_el)
            .children(cell_pop)
            .children(dlg)
            .when(pop.is_some_and(|(k, _)| k == Pop::Shortcuts), |d| d.child(self.shortcuts_modal(&t, cx)))
    }
}
