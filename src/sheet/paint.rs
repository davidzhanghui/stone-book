//! 网格绘制。思路表格的 Canvas 渲染：只画可视区；冻结时把视口切成四个分区
//! （左上冻结角 / 顶部冻结行 / 左侧冻结列 / 主滚动区），每个分区用自己的偏移和裁剪各画一遍。
//! 绘制顺序：网格线 → 填充 → 合并块 / 溢出文字遮挡网格线 → 数据条 → 文字 → 边框 → 查找高亮 → 引用高亮 → 选区。

use super::{Cell, SelKind, HEAD_H, HEAD_W};
use gpui::*;

pub(crate) struct CellDraw {
    pub r: i32,
    pub c: i32,
    /// 内容坐标（合并单元格为整块）
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
    pub text: SharedString,
    pub color: Hsla,
    pub bg: Option<Hsla>,
    pub font: Font,
    pub px: f32,
    pub underline: bool,
    pub strike: bool,
    /// 0 左 1 中 2 右
    pub h: u8,
    /// 0 上 1 中 2 下
    pub v: u8,
    pub wrap: bool,
    pub merged: bool,
    /// 文字可溢出到的左右边界（内容坐标）
    pub ov: (f32, f32),
    /// 数据条：(起点比例, 长度比例, 颜色)
    pub bar: Option<(f32, f32, Hsla, bool)>,
    /// 上 右 下 左：(颜色, 线宽, 虚线)
    pub borders: [Option<(Hsla, f32, bool)>; 4],
    /// 对角线：(左上→右下, 右上→左下, 颜色, 线宽)
    pub diag: Option<(bool, bool, Hsla, f32)>,
    /// 单元格对象（下拉 / 复选框 / 图片 / 备注…）
    pub obj: Option<Box<ObjDraw>>,
    /// 文本换行「截断」：不溢出到相邻格
    pub clip: bool,
}

/// 单元格上的对象。
#[derive(Default)]
pub(crate) struct ObjDraw {
    /// 复选框
    pub check: Option<bool>,
    /// 下拉选项胶囊：(文字, 底色, 文字色)
    pub chips: Vec<(SharedString, Hsla, Hsla)>,
    /// 下拉箭头
    pub arrow: bool,
    /// 日期提醒：文字前画时钟、文字变蓝
    pub remind: bool,
    pub image: Option<std::sync::Arc<RenderImage>>,
    /// 备注（右上角红三角）
    pub note: bool,
    /// 评论（右上角黄三角）
    pub comment: bool,
    /// 附件文件名
    pub files: Vec<SharedString>,
    pub spark: Option<SparkDraw>,
    /// 不符合数据验证（左上角红三角）
    pub invalid: bool,
    /// 条件格式图标集：(图标名, 颜色)
    pub cf_icon: Option<(&'static str, Hsla)>,
    /// 条件格式等级：(图标名, 点亮个数, 总数, 颜色)
    pub rating: Option<(&'static str, u32, u32, Hsla)>,
}

pub(crate) struct SparkDraw {
    /// 数据（None = 空单元格，按「空距」断开）
    pub vals: Vec<Option<f64>>,
    /// 0 折线 1 柱状 2 盈亏
    pub kind: u8,
    pub main: Hsla,
    /// 各标记颜色（高点 低点 负点 首点 尾点 所有点）
    pub mark_cols: [Hsla; 6],
    pub fancy: bool,
    pub marks: u8,
    /// 折线粗细
    pub lw: f32,
    /// 显示横坐标轴
    pub axis: bool,
}

pub(crate) struct Frame {
    pub cells: std::rc::Rc<Vec<CellDraw>>,
    pub col_x: Vec<f32>,
    pub row_y: Vec<f32>,
    pub fr: i32,
    pub fc: i32,
    pub scroll: (f32, f32),
    /// 主滚动区的可视行列范围
    pub rows: (i32, i32),
    pub cols: (i32, i32),
    pub sel: (i32, i32, i32, i32),
    pub cur: (i32, i32, i32, i32),
    pub kind: SelKind,
    pub fill_to: Option<Cell>,
    pub refs: Vec<(i32, i32, i32, i32, Hsla)>,
    pub finds: Vec<Cell>,
    pub find_cur: Option<Cell>,
    /// 筛选：(表头行, 起列, 止列, 已设置条件的列)
    pub filter: Option<(i32, i32, i32, Vec<i32>)>,
    pub grid_on: bool,
    pub cut: Option<(i32, i32, i32, i32)>,
    /// 列头 / 行头分隔线：(是否列, 网格局部坐标, 是否拖动中)
    pub resize: Option<(bool, f32, bool)>,
    pub editing: bool,
    pub zoom: f32,
    pub bg: Hsla,
    pub grid: Hsla,
    pub head_bg: Hsla,
    pub head_fg: Hsla,
    pub accent: Hsla,
    pub dark: bool,
    /// 高亮所在行列的颜色
    pub hl: Option<Hsla>,
    /// 日期选项：活动格右侧画日历按钮
    pub date_btn: bool,
}

impl Frame {
    fn cl(&self, c: i32) -> f32 {
        let i = (c - 1).max(0) as usize;
        self.col_x.get(i).or(self.col_x.last()).copied().unwrap_or(0.)
    }
    fn rt(&self, r: i32) -> f32 {
        let i = (r - 1).max(0) as usize;
        self.row_y.get(i).or(self.row_y.last()).copied().unwrap_or(0.)
    }
}

/// 一个分区：行列范围、偏移、裁剪框。
struct Pane {
    rows: (i32, i32),
    cols: (i32, i32),
    ox: Pixels,
    oy: Pixels,
    mask: Bounds<Pixels>,
    frozen_r: bool,
    frozen_c: bool,
}

impl Pane {
    fn x(&self, f: &Frame, c: i32) -> Pixels {
        self.ox + px(f.cl(c))
    }
    fn y(&self, f: &Frame, r: i32) -> Pixels {
        self.oy + px(f.rt(r))
    }
    fn rect(&self, f: &Frame, r0: i32, c0: i32, r1: i32, c1: i32) -> Bounds<Pixels> {
        Bounds::from_corners(point(self.x(f, c0), self.y(f, r0)), point(self.x(f, c1 + 1), self.y(f, r1 + 1)))
    }
    fn xr(&self, x: f32) -> Pixels {
        self.ox + px(x)
    }
    fn yr(&self, y: f32) -> Pixels {
        self.oy + px(y)
    }
}

fn hline(window: &mut Window, x0: Pixels, x1: Pixels, y: Pixels, w: f32, color: Hsla, dashed: bool) {
    if dashed {
        let mut x = x0;
        while x < x1 {
            let e = (x + px(3.)).min(x1);
            window.paint_quad(fill(Bounds::from_corners(point(x, y), point(e, y + px(w))), color));
            x += px(5.);
        }
    } else {
        window.paint_quad(fill(Bounds::from_corners(point(x0, y), point(x1, y + px(w))), color));
    }
}

fn vline(window: &mut Window, x: Pixels, y0: Pixels, y1: Pixels, w: f32, color: Hsla, dashed: bool) {
    if dashed {
        let mut y = y0;
        while y < y1 {
            let e = (y + px(3.)).min(y1);
            window.paint_quad(fill(Bounds::from_corners(point(x, y), point(x + px(w), e)), color));
            y += px(5.);
        }
    } else {
        window.paint_quad(fill(Bounds::from_corners(point(x, y0), point(x + px(w), y1)), color));
    }
}

fn label(window: &mut Window, cx: &mut App, text: String, size: f32, color: Hsla, bold: bool) -> ShapedLine {
    let mut f = font("PingFang SC");
    if bold {
        f.weight = FontWeight::SEMIBOLD;
    }
    let run = TextRun { len: text.len(), font: f, color, background_color: None, underline: None, strikethrough: None };
    let _ = cx;
    window.text_system().shape_line(text.into(), px(size), &[run], None)
}

pub(crate) fn paint(f: &Frame, b: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let o = b.origin;
    window.paint_quad(fill(b, f.bg));
    let (fw, fh) = (f.cl(f.fc + 1), f.rt(f.fr + 1));
    let gx0 = o.x + px(HEAD_W);
    let gy0 = o.y + px(HEAD_H);
    let mut panes = vec![];
    for frozen_r in [true, false] {
        for frozen_c in [true, false] {
            let rows = if frozen_r { (1, f.fr) } else { f.rows };
            let cols = if frozen_c { (1, f.fc) } else { f.cols };
            if rows.1 < rows.0 || cols.1 < cols.0 {
                continue;
            }
            let (mx0, mx1) = if frozen_c { (gx0, gx0 + px(fw)) } else { (gx0 + px(fw), b.right()) };
            let (my0, my1) = if frozen_r { (gy0, gy0 + px(fh)) } else { (gy0 + px(fh), b.bottom()) };
            panes.push(Pane {
                rows,
                cols,
                ox: gx0 - px(if frozen_c { 0. } else { f.scroll.0 }),
                oy: gy0 - px(if frozen_r { 0. } else { f.scroll.1 }),
                mask: Bounds::from_corners(point(mx0, my0), point(mx1.max(mx0), my1.max(my0))),
                frozen_r,
                frozen_c,
            });
        }
    }
    for p in &panes {
        window.with_content_mask(Some(ContentMask { bounds: p.mask }), |window| paint_pane(f, p, window, cx));
    }
    paint_headers(f, b, &panes, window, cx);
    // 调整列宽 / 行高（悬停时表头分隔线变蓝；拖动时再拉一条贯穿网格的参考线）
    if let Some((col, at, dragging)) = f.resize {
        if col {
            let x = o.x + px(at);
            window.paint_quad(fill(Bounds::new(point(x - px(1.5), o.y + px(2.)), size(px(3.), px(HEAD_H - 4.))), f.accent).corner_radii(px(1.5)));
            if dragging {
                window.paint_quad(fill(Bounds::from_corners(point(x - px(0.5), gy0), point(x + px(0.5), b.bottom())), f.accent));
            }
        } else {
            let y = o.y + px(at);
            window.paint_quad(fill(Bounds::new(point(o.x + px(2.), y - px(1.5)), size(px(HEAD_W - 4.), px(3.))), f.accent).corner_radii(px(1.5)));
            if dragging {
                window.paint_quad(fill(Bounds::from_corners(point(gx0, y - px(0.5)), point(b.right(), y + px(0.5))), f.accent));
            }
        }
    }
}

fn paint_pane(f: &Frame, p: &Pane, window: &mut Window, cx: &mut App) {
    let in_pane = |cd: &CellDraw| (cd.r <= f.fr) == p.frozen_r && (cd.c <= f.fc) == p.frozen_c;
    // 网格线
    if f.grid_on {
        for c in p.cols.0..=p.cols.1 + 1 {
            vline(window, p.x(f, c) - px(0.5), p.mask.top(), p.mask.bottom(), 1., f.grid, false);
        }
        for r in p.rows.0..=p.rows.1 + 1 {
            hline(window, p.mask.left(), p.mask.right(), p.y(f, r) - px(0.5), 1., f.grid, false);
        }
    }
    // 填充（合并块用底色盖住内部网格线）
    for cd in f.cells.iter().filter(|cd| in_pane(cd)) {
        let rect = Bounds::from_corners(point(p.xr(cd.x0), p.yr(cd.y0)), point(p.xr(cd.x1), p.yr(cd.y1)));
        if let Some(bg) = cd.bg {
            window.paint_quad(fill(Bounds::from_corners(rect.origin - point(px(0.5), px(0.5)), rect.bottom_right() - point(px(0.5), px(0.5))), bg));
        } else if cd.merged {
            window.paint_quad(fill(Bounds::from_corners(rect.origin + point(px(0.5), px(0.5)), rect.bottom_right() - point(px(0.5), px(0.5))), f.bg));
        }
    }
    // 高亮所在行列（底部「高亮所在行列」）
    if let Some(hl) = f.hl {
        let (r0, c0, r1, c1) = f.cur;
        let rr = Bounds::from_corners(point(p.mask.left(), p.y(f, r0)), point(p.mask.right(), p.y(f, r1 + 1)));
        let cc = Bounds::from_corners(point(p.x(f, c0), p.mask.top()), point(p.x(f, c1 + 1), p.mask.bottom()));
        window.paint_quad(fill(rr, hl));
        window.paint_quad(fill(cc, hl));
    }
    // 文字溢出到相邻空格：盖住经过的竖线
    for cd in f.cells.iter().filter(|cd| in_pane(cd) && (cd.ov.0 < cd.x0 || cd.ov.1 > cd.x1)) {
        let (y0, y1) = (p.yr(cd.y0) + px(0.5), p.yr(cd.y1) - px(0.5));
        if cd.ov.1 > cd.x1 {
            window.paint_quad(fill(Bounds::from_corners(point(p.xr(cd.x1) - px(0.5), y0), point(p.xr(cd.ov.1) - px(1.), y1)), f.bg));
        }
        if cd.ov.0 < cd.x0 {
            window.paint_quad(fill(Bounds::from_corners(point(p.xr(cd.ov.0) + px(0.5), y0), point(p.xr(cd.x0) + px(0.5), y1)), f.bg));
        }
    }
    // 数据条
    for cd in f.cells.iter().filter(|cd| in_pane(cd) && cd.bar.is_some()) {
        let (start, len, color, grad) = cd.bar.unwrap();
        let w = cd.x1 - cd.x0 - 4.;
        let r = Bounds::from_corners(point(p.xr(cd.x0 + 2. + w * start), p.yr(cd.y0 + 3.)), point(p.xr(cd.x0 + 2. + w * (start + len)), p.yr(cd.y1 - 3.)));
        if grad {
            // 渐变填充：从颜色渐隐到白
            window.paint_quad(
                fill(r, linear_gradient(90., linear_color_stop(color.opacity(0.75), 0.), linear_color_stop(color.opacity(0.08), 1.))).corner_radii(px(2.)),
            );
        } else {
            window.paint_quad(fill(r, color.opacity(0.6)).corner_radii(px(2.)));
        }
    }
    // 文字
    let pad = 5. * f.zoom;
    for cd in f.cells.iter().filter(|cd| in_pane(cd) && !cd.text.is_empty()) {
        let rect = Bounds::from_corners(point(p.xr(cd.x0), p.yr(cd.y0)), point(p.xr(cd.x1), p.yr(cd.y1)));
        let mut font = cd.font.clone();
        let fsize = px(cd.px);
        let lh = px(cd.px * 1.35);
        let run = |len: usize, font: &Font| TextRun {
            len,
            font: font.clone(),
            color: cd.color,
            background_color: None,
            underline: cd.underline.then(|| UnderlineStyle { color: Some(cd.color), thickness: px(1.), wavy: false }),
            strikethrough: cd.strike.then(|| StrikethroughStyle { color: Some(cd.color), thickness: px(1.) }),
        };
        if cd.wrap {
            let inner = Bounds::from_corners(rect.origin + point(px(pad), px(2.)), rect.bottom_right() - point(px(pad), px(2.)));
            let Ok(lines) = window.text_system().shape_text(cd.text.clone(), fsize, &[run(cd.text.len(), &font)], Some(inner.size.width.max(px(4.))), None)
            else {
                continue;
            };
            let total: Pixels = lines.iter().map(|l| l.size(lh).height).fold(px(0.), |a, b| a + b);
            let mut y = match cd.v {
                0 => inner.top(),
                2 => inner.bottom() - total,
                _ => inner.top() + (inner.size.height - total) / 2.,
            };
            let align = match cd.h {
                1 => TextAlign::Center,
                2 => TextAlign::Right,
                _ => TextAlign::Left,
            };
            window.with_content_mask(Some(ContentMask { bounds: rect }), |window| {
                for l in &lines {
                    l.paint(point(inner.left(), y), lh, align, Some(inner), window, cx).ok();
                    y += l.size(lh).height;
                }
            });
            continue;
        }
        let text: SharedString = if cd.text.contains('\n') { cd.text.replace('\n', " ").into() } else { cd.text.clone() };
        if cd.font.style == FontStyle::Italic && cd.font.family.as_ref() == "PingFang SC" {
            font.family = ".SystemUIFont".into();
        }
        let line = window.text_system().shape_line(text.clone(), fsize, &[run(text.len(), &font)], None);
        let icon_w = if cd.obj.as_ref().is_some_and(|o| o.remind) { px(14. * f.zoom) } else { px(0.) };
        let x = match cd.h {
            1 => rect.left() + (rect.size.width - line.width) / 2.,
            2 => rect.right() - line.width - px(pad),
            _ => rect.left() + px(pad) + icon_w,
        };
        let y = match cd.v {
            0 => rect.top() + px(2.),
            2 => rect.bottom() - lh - px(2.),
            _ => rect.top() + (rect.size.height - lh) / 2.,
        };
        let clip = Bounds::from_corners(point(p.xr(cd.ov.0.min(cd.x0)), rect.top()), point(p.xr(cd.ov.1.max(cd.x1)) - px(1.), rect.bottom()));
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            line.paint(point(x, y), lh, window, cx).ok();
        });
    }
    // 边框
    for cd in f.cells.iter().filter(|cd| in_pane(cd)) {
        let (x0, x1, y0, y1) = (p.xr(cd.x0), p.xr(cd.x1), p.yr(cd.y0), p.yr(cd.y1));
        let [t, r, btm, l] = cd.borders;
        if let Some((c, w, d)) = t {
            hline(window, x0 - px(w / 2.), x1 + px(w / 2.), y0 - px(w / 2.), w, c, d);
        }
        if let Some((c, w, d)) = btm {
            hline(window, x0 - px(w / 2.), x1 + px(w / 2.), y1 - px(w / 2.), w, c, d);
        }
        if let Some((c, w, d)) = l {
            vline(window, x0 - px(w / 2.), y0 - px(w / 2.), y1 + px(w / 2.), w, c, d);
        }
        if let Some((c, w, d)) = r {
            vline(window, x1 - px(w / 2.), y0 - px(w / 2.), y1 + px(w / 2.), w, c, d);
        }
    }
    // 对角线边框
    for cd in f.cells.iter().filter(|cd| in_pane(cd) && cd.diag.is_some()) {
        let (down, up, c, w) = cd.diag.unwrap();
        let (x0, x1, y0, y1) = (p.xr(cd.x0), p.xr(cd.x1), p.yr(cd.y0), p.yr(cd.y1));
        for (a, b) in [(down, (point(x0, y0), point(x1, y1))), (up, (point(x0, y1), point(x1, y0)))].into_iter().filter(|(on, _)| *on).map(|(_, l)| (l.0, l.1))
        {
            let mut pb = PathBuilder::stroke(px(w));
            pb.move_to(a);
            pb.line_to(b);
            if let Ok(path) = pb.build() {
                window.paint_path(path, c);
            }
        }
    }
    // 单元格对象
    for cd in f.cells.iter().filter(|cd| in_pane(cd) && cd.obj.is_some()) {
        let rect = Bounds::from_corners(point(p.xr(cd.x0), p.yr(cd.y0)), point(p.xr(cd.x1), p.yr(cd.y1)));
        window.with_content_mask(Some(ContentMask { bounds: rect }), |window| paint_obj(f, cd, cd.obj.as_ref().unwrap(), rect, window, cx));
    }
    // 筛选按钮（表头行每格右侧）
    if let Some((head, c0, c1, active)) = &f.filter {
        if (*head <= f.fr) == p.frozen_r {
            for c in (*c0).max(p.cols.0)..=(*c1).min(p.cols.1) {
                let r = p.rect(f, *head, c, *head, c);
                let s = px(16.);
                let bx = Bounds::new(point(r.right() - s - px(4.), r.top() + (r.size.height - s) / 2.), size(s, s));
                let on = active.contains(&c);
                window.paint_quad(fill(bx, if on { f.accent } else { f.bg }).corner_radii(px(3.)).border_widths(px(1.)).border_color(if on {
                    f.accent
                } else {
                    f.grid
                }));
                // 小漏斗 / 三角
                let col = if on { white() } else { f.head_fg };
                let mut pb = PathBuilder::fill();
                let (cx0, cy0) = (bx.left() + px(4.), bx.top() + px(5.5));
                pb.move_to(point(cx0, cy0));
                pb.line_to(point(cx0 + px(8.), cy0));
                pb.line_to(point(cx0 + px(4.), cy0 + px(5.)));
                pb.close();
                if let Ok(path) = pb.build() {
                    window.paint_path(path, col);
                }
            }
        }
    }
    // 查找命中
    for m in &f.finds {
        let r = p.rect(f, m.r, m.c, m.r, m.c);
        let cur = f.find_cur == Some(*m);
        window.paint_quad(fill(r, if cur { hsla(0.09, 1., 0.55, 0.45) } else { hsla(0.14, 1., 0.55, 0.30) }));
    }
    // 剪切 / 复制的蚂蚁线
    // 与当前选区重合时只画选区的实线框，离开后才显示虚线
    if let Some((r0, c0, r1, c1)) = f.cut.filter(|c| *c != f.sel) {
        let r = p.rect(f, r0, c0, r1, c1);
        window.paint_quad(outline(r, f.accent, BorderStyle::Dashed).border_widths(px(1.5)));
    }
    // 编辑公式时的引用高亮（每个引用一种颜色，区域加浅色底）
    for (r0, c0, r1, c1, col) in &f.refs {
        let r = p.rect(f, *r0, *c0, *r1, *c1);
        window.paint_quad(fill(r, col.opacity(0.10)));
        window.paint_quad(outline(r, *col, BorderStyle::default()).border_widths(px(1.5)));
    }
    // 选区
    let (r0, c0, r1, c1) = f.sel;
    let s = p.rect(f, r0, c0, r1.min(f.row_y.len() as i32), c1.min(f.col_x.len() as i32));
    let (cr0, cc0, cr1, cc1) = f.cur;
    let cr = p.rect(f, cr0, cc0, cr1, cc1);
    let tint = f.accent.opacity(0.10);
    if (r0, c0, r1, c1) != (cr0, cc0, cr1, cc1) {
        // 选区底色（活动格保持白底）
        for q in [
            Bounds::from_corners(s.origin, point(s.right(), cr.top())),
            Bounds::from_corners(point(s.left(), cr.bottom()), s.bottom_right()),
            Bounds::from_corners(point(s.left(), cr.top()), point(cr.left(), cr.bottom())),
            Bounds::from_corners(point(cr.right(), cr.top()), point(s.right(), cr.bottom())),
        ] {
            if q.size.width > px(0.) && q.size.height > px(0.) {
                window.paint_quad(fill(q, tint));
            }
        }
    }
    if !f.editing {
        window.paint_quad(
            outline(Bounds::from_corners(s.origin - point(px(1.), px(1.)), s.bottom_right()), f.accent, BorderStyle::default()).border_widths(px(2.)),
        );
        if f.kind == SelKind::Cells {
            let hb = Bounds::new(point(s.right() - px(4.), s.bottom() - px(4.)), size(px(7.), px(7.)));
            window.paint_quad(fill(hb, f.accent).border_widths(px(1.)).border_color(white()));
        }
    }
    if f.date_btn && !f.editing {
        // 日期选项：活动格右侧的日历按钮
        let b = Bounds::new(point(cr.right() + px(3.), cr.top() + (cr.size.height - px(18.)) / 2.), size(px(18.), px(18.)));
        window.paint_quad(fill(b, f.bg).corner_radii(px(4.)).border_widths(px(1.)).border_color(f.grid));
        let i = Bounds::new(point(b.left() + px(4.), b.top() + px(5.)), size(px(10.), px(9.)));
        window.paint_quad(outline(i, f.head_fg, BorderStyle::default()).border_widths(px(1.)).corner_radii(px(1.5)));
        window.paint_quad(fill(Bounds::new(point(i.left(), i.top() + px(2.)), size(px(10.), px(1.))), f.head_fg));
    }
    if let Some(to) = f.fill_to {
        let (or, oc) = (to.r - r1, to.c - c1);
        let fr = if or > 0 && or >= oc {
            p.rect(f, r0, c0, to.r, c1)
        } else if oc > 0 {
            p.rect(f, r0, c0, r1, to.c)
        } else {
            s
        };
        window.paint_quad(outline(fr, f.accent, BorderStyle::Dashed).border_widths(px(1.5)));
    }
}

fn tri(window: &mut Window, a: Point<Pixels>, b: Point<Pixels>, c: Point<Pixels>, color: Hsla) {
    let mut pb = PathBuilder::fill();
    pb.move_to(a);
    pb.line_to(b);
    pb.line_to(c);
    pb.close();
    if let Ok(path) = pb.build() {
        window.paint_path(path, color);
    }
}

fn stroke(window: &mut Window, pts: &[Point<Pixels>], w: f32, color: Hsla) {
    if pts.len() < 2 {
        return;
    }
    let mut pb = PathBuilder::stroke(px(w));
    pb.move_to(pts[0]);
    for q in &pts[1..] {
        pb.line_to(*q);
    }
    if let Ok(path) = pb.build() {
        window.paint_path(path, color);
    }
}

/// 小时钟（日期提醒）
pub(crate) fn paint_clock(window: &mut Window, c: Point<Pixels>, r: f32, color: Hsla) {
    window.paint_quad(fill(Bounds::new(point(c.x - px(r), c.y - px(r)), size(px(r * 2.), px(r * 2.))), color).corner_radii(px(r)));
    stroke(window, &[point(c.x, c.y - px(r * 0.6)), c, point(c.x + px(r * 0.5), c.y + px(r * 0.2))], 1.2, white());
}

/// 回形针（附件）
/// 文件类型图标（与文档「视频或文件」的 file_icon 同色同图形）：彩色文档底 + 白色图形 / 字母。
fn paint_file_icon(window: &mut Window, cx: &mut App, o: Point<Pixels>, s: f32, name: &str) {
    use crate::editor::Glyph;
    let (col, glyph) = crate::editor::file_style(name);
    window.paint_svg(Bounds::new(o, size(px(s), px(s))), "icons/file-shape.svg".into(), TransformationMatrix::unit(), rgb(col).into(), cx).ok();
    // 图形放在文档底偏下的位置（让出右上角的折角）
    let c = point(o.x + px(s * 0.54), o.y + px(s * 0.6));
    match glyph {
        Glyph::Svg(n) => {
            let g = s * 0.5;
            window
                .paint_svg(
                    Bounds::new(point(c.x - px(g / 2.), c.y - px(g / 2.)), size(px(g), px(g))),
                    format!("icons/{n}.svg").into(),
                    TransformationMatrix::unit(),
                    white(),
                    cx,
                )
                .ok();
        }
        Glyph::Text(t) => {
            let fs = s * if t.len() > 1 { 0.26 } else { 0.4 };
            let run = TextRun {
                len: t.len(),
                font: Font { weight: FontWeight::BOLD, ..font("PingFang SC") },
                color: white(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(t.into(), px(fs), &[run], None);
            line.paint(point(c.x - line.width / 2., c.y - px(fs * 1.2 / 2.)), px(fs * 1.2), window, cx).ok();
        }
    }
}

fn paint_obj(f: &Frame, cd: &CellDraw, o: &ObjDraw, rect: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let z = f.zoom;
    let pad = px(5. * z);
    let mid_y = rect.top() + rect.size.height / 2.;
    let blue: Hsla = rgb(0x3370ff).into();
    if let Some(img) = &o.image {
        // 单元格图片：等比缩放放进格子（留 2px 边）
        let sz = img.size(0);
        let (iw, ih) = (sz.width.0 as f32, sz.height.0 as f32);
        let (bw, bh) = (f32::from(rect.size.width) - 4., f32::from(rect.size.height) - 4.);
        if iw > 0. && ih > 0. && bw > 2. && bh > 2. {
            let k = (bw / iw).min(bh / ih);
            let (w, h) = (iw * k, ih * k);
            let x = match cd.h {
                1 => rect.left() + (rect.size.width - px(w)) / 2.,
                2 => rect.right() - px(w + 2.),
                _ => rect.left() + px(2.),
            };
            window.paint_image(Bounds::new(point(x, mid_y - px(h / 2.)), size(px(w), px(h))), Corners::all(px(2.)), img.clone(), 0, false).ok();
        }
    }
    if let Some(on) = o.check {
        let s = (12. * z).max(8.);
        let x = match cd.h {
            0 => rect.left() + pad,
            2 => rect.right() - pad - px(s),
            _ => rect.left() + (rect.size.width - px(s)) / 2.,
        };
        let b = Bounds::new(point(x, mid_y - px(s / 2.)), size(px(s), px(s)));
        if on {
            window.paint_quad(fill(b, blue).corner_radii(px(2.5 * z)));
            stroke(
                window,
                &[
                    point(b.left() + px(s * 0.22), b.top() + px(s * 0.52)),
                    point(b.left() + px(s * 0.42), b.top() + px(s * 0.72)),
                    point(b.left() + px(s * 0.78), b.top() + px(s * 0.3)),
                ],
                1.6 * z.max(0.8),
                white(),
            );
        } else {
            window.paint_quad(fill(b, f.bg).corner_radii(px(2.5 * z)).border_widths(px(1.)).border_color(if f.dark {
                hsla(0., 0., 0.5, 1.)
            } else {
                rgb(0xbbbfc4).into()
            }));
        }
    }
    if !o.chips.is_empty() {
        let fs = 12. * z;
        let h = px(fs * 1.55);
        let mut x = rect.left() + pad;
        let right = rect.right() - if o.arrow { px(16. * z) } else { pad };
        for (text, bg, fg) in &o.chips {
            let line = label(window, cx, text.to_string(), fs, *fg, false);
            let w = line.width + px(fs * 1.1);
            let b = Bounds::new(point(x, mid_y - h / 2.), size(w.min((right - x).max(px(0.))), h));
            if b.size.width <= px(4.) {
                break;
            }
            window.paint_quad(fill(b, *bg).corner_radii(h / 2.));
            window.with_content_mask(Some(ContentMask { bounds: b }), |window| {
                line.paint(point(x + px(fs * 0.55), mid_y - px(fs * 1.4) / 2.), px(fs * 1.4), window, cx).ok();
            });
            x += w + px(4.);
        }
    }
    if o.arrow {
        let (ax, ay) = (rect.right() - px(12. * z), mid_y - px(2. * z));
        tri(window, point(ax, ay), point(ax + px(7. * z), ay), point(ax + px(3.5 * z), ay + px(4. * z)), f.head_fg);
    }
    if o.remind {
        paint_clock(window, point(rect.left() + pad + px(5. * z), mid_y), 5. * z, blue);
    }
    if !o.files.is_empty() {
        let fs = 12. * z;
        let lh = fs * 1.35;
        let total = lh * o.files.len() as f32;
        let mut y = mid_y - px(total / 2.);
        for name in &o.files {
            let s = 14. * z;
            paint_file_icon(window, cx, point(rect.left() + pad, y + px((lh - s) / 2.)), s, name);
            let line = window.text_system().shape_line(
                name.clone(),
                px(fs),
                &[TextRun {
                    len: name.len(),
                    font: font("PingFang SC"),
                    color: blue,
                    background_color: None,
                    underline: Some(UnderlineStyle { color: Some(blue), thickness: px(1.), wavy: false }),
                    strikethrough: None,
                }],
                None,
            );
            line.paint(point(rect.left() + pad + px(14. * z + 4. * z), y), px(lh), window, cx).ok();
            y += px(lh);
        }
    }
    if let Some((name, col)) = o.cf_icon {
        let s = 14. * z;
        window
            .paint_svg(
                Bounds::new(point(rect.left() + pad * 0.6, mid_y - px(s / 2.)), size(px(s), px(s))),
                format!("icons/{name}.svg").into(),
                TransformationMatrix::unit(),
                col,
                cx,
            )
            .ok();
    }
    if let Some((name, n, max, col)) = o.rating {
        let s = 13. * z;
        for k in 0..max {
            let c = if k < n { col } else { hsla(0., 0., 0.85, 1.) };
            window
                .paint_svg(
                    Bounds::new(point(rect.left() + pad * 0.6 + px((s + 2.) * k as f32), mid_y - px(s / 2.)), size(px(s), px(s))),
                    format!("icons/{name}.svg").into(),
                    TransformationMatrix::unit(),
                    c,
                    cx,
                )
                .ok();
        }
    }
    if let Some(sp) = &o.spark {
        let pad = px(3. * z.max(0.6));
        paint_spark(sp, Bounds::from_corners(rect.origin + point(pad + px(1.), pad), rect.bottom_right() - point(pad + px(1.), pad)), window);
    }
    if o.note || o.comment {
        let s = px(7. * z.max(0.8));
        let col = if o.comment { hsla(0.12, 0.95, 0.5, 1.) } else { rgb(0xf54a45).into() };
        tri(window, point(rect.right() - s, rect.top()), point(rect.right(), rect.top()), point(rect.right(), rect.top() + s), col);
    }
    if o.invalid {
        let s = px(7. * z.max(0.8));
        tri(window, point(rect.left(), rect.top()), point(rect.left() + s, rect.top()), point(rect.left(), rect.top() + s), rgb(0xf54a45).into());
    }
}

/// 迷你图：折线（可渐变填充、空距断开）/ 柱状（可圆角）/ 盈亏，标记高点、低点、负点、首尾点，可画横坐标轴。
pub(crate) fn paint_spark(sp: &SparkDraw, b: Bounds<Pixels>, window: &mut Window) {
    let n = sp.vals.len();
    let nums: Vec<(usize, f64)> = sp.vals.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, v))).collect();
    if nums.is_empty() || b.size.width <= px(2.) || b.size.height <= px(2.) {
        return;
    }
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let (mut lo, mut hi) = nums.iter().fold((f64::MAX, f64::MIN), |(a, z), (_, v)| (a.min(*v), z.max(*v)));
    if sp.kind == 2 {
        (lo, hi) = (-1., 1.);
    } else if sp.kind == 1 || sp.axis {
        lo = lo.min(0.);
        hi = hi.max(0.);
    }
    if (hi - lo).abs() < 1e-12 {
        hi = lo + 1.;
    }
    let yv = |v: f64| b.top() + px(h - ((v - lo) / (hi - lo)) as f32 * h);
    let imax = nums.iter().fold(nums[0], |m, x| if x.1 > m.1 { *x } else { m }).0;
    let imin = nums.iter().fold(nums[0], |m, x| if x.1 < m.1 { *x } else { m }).0;
    let (first, last) = (nums[0].0, nums[nums.len() - 1].0);
    // 标记颜色：按优先级 高点 > 低点 > 首点 > 尾点 > 负点
    let mark = |i: usize, v: f64| -> Option<Hsla> {
        let m = sp.marks;
        if m & 1 != 0 && i == imax {
            Some(sp.mark_cols[0])
        } else if m & 2 != 0 && i == imin {
            Some(sp.mark_cols[1])
        } else if m & 8 != 0 && i == first {
            Some(sp.mark_cols[3])
        } else if m & 16 != 0 && i == last {
            Some(sp.mark_cols[4])
        } else if m & 4 != 0 && v < 0. {
            Some(sp.mark_cols[2])
        } else {
            None
        }
    };
    let zero = yv(0.);
    if sp.axis {
        window.paint_quad(fill(Bounds::new(point(b.left(), zero - px(0.5)), size(b.size.width, px(1.))), hsla(0., 0., 0.55, 0.6)));
    }
    if sp.kind >= 1 {
        let slot = w / n as f32;
        let bw = (slot * 0.72).max(1.);
        for &(i, v) in &nums {
            let x = b.left() + px(slot * i as f32 + (slot - bw) / 2.);
            let r = if sp.kind == 2 {
                // 盈亏：正值向上、负值向下，等高
                let hh = px(h / 2. - 1.);
                if v >= 0. {
                    Bounds::new(point(x, zero - hh), size(px(bw), hh))
                } else {
                    Bounds::new(point(x, zero), size(px(bw), hh))
                }
            } else {
                let y = yv(v);
                Bounds::from_corners(point(x, y.min(zero)), point(x + px(bw), y.max(zero) + if (y - zero).abs() < px(1.) { px(1.) } else { px(0.) }))
            };
            let col = mark(i, v).unwrap_or(if sp.kind == 2 && v < 0. { sp.mark_cols[2] } else { sp.main });
            window.paint_quad(fill(r, col).corner_radii(if sp.fancy { px(bw.min(6.) / 3.) } else { px(0.) }));
        }
        return;
    }
    let xs = |i: usize| b.left() + if n == 1 { px(w / 2.) } else { px(w * i as f32 / (n - 1) as f32) };
    // 按空距断成若干段
    let mut segs: Vec<Vec<Point<Pixels>>> = vec![vec![]];
    for (i, v) in sp.vals.iter().enumerate() {
        match v {
            Some(v) => segs.last_mut().unwrap().push(point(xs(i), yv(*v))),
            None if !segs.last().unwrap().is_empty() => segs.push(vec![]),
            None => {}
        }
    }
    for pts in segs.iter().filter(|p| !p.is_empty()) {
        if sp.fancy && pts.len() > 1 {
            // 渐变填充：上浓下淡
            let mut pb = PathBuilder::fill();
            pb.move_to(point(pts[0].x, b.bottom()));
            for q in pts {
                pb.line_to(*q);
            }
            pb.line_to(point(pts[pts.len() - 1].x, b.bottom()));
            pb.close();
            if let Ok(path) = pb.build() {
                window.paint_path(path, linear_gradient(180., linear_color_stop(sp.main.opacity(0.32), 0.), linear_color_stop(sp.main.opacity(0.02), 1.)));
            }
        }
        stroke(window, pts, sp.lw, sp.main);
    }
    for &(i, v) in &nums {
        let q = point(xs(i), yv(v));
        let col = mark(i, v).or((sp.marks & 32 != 0).then_some(sp.mark_cols[5]));
        if let Some(col) = col {
            let r = sp.lw.max(2.) + 1.;
            window.paint_quad(fill(Bounds::new(point(q.x - px(r), q.y - px(r)), size(px(r * 2.), px(r * 2.))), col).corner_radii(px(r)));
        }
    }
}

fn paint_headers(f: &Frame, b: Bounds<Pixels>, panes: &[Pane], window: &mut Window, cx: &mut App) {
    let o = b.origin;
    let (r0, c0, r1, c1) = f.sel;
    let full_cols = matches!(f.kind, SelKind::Cols | SelKind::All);
    let full_rows = matches!(f.kind, SelKind::Rows | SelKind::All);
    let hsize = 12. * f.zoom.clamp(0.8, 1.3);
    // 列头
    window.paint_quad(fill(Bounds::new(o, size(b.size.width, px(HEAD_H))), f.head_bg));
    for p in panes.iter().filter(|p| p.frozen_r || f.fr == 0) {
        let mask = Bounds::from_corners(point(p.mask.left(), o.y), point(p.mask.right(), o.y + px(HEAD_H)));
        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
            for c in p.cols.0..=p.cols.1 {
                let (x0, x1) = (p.x(f, c), p.x(f, c + 1));
                if x1 - x0 <= px(0.) {
                    // 隐藏列：画一个小标记
                    window.paint_quad(
                        fill(Bounds::new(point(x0 - px(2.), o.y + px(6.)), size(px(4.), px(HEAD_H - 12.))), f.head_fg.opacity(0.5)).corner_radii(px(1.)),
                    );
                    continue;
                }
                let on = c >= c0 && c <= c1;
                let full = on && full_cols;
                if full {
                    window.paint_quad(fill(Bounds::from_corners(point(x0, o.y), point(x1, o.y + px(HEAD_H))), f.accent));
                } else if on {
                    window.paint_quad(fill(Bounds::from_corners(point(x0, o.y), point(x1, o.y + px(HEAD_H))), f.accent.opacity(0.14)));
                    window.paint_quad(fill(Bounds::from_corners(point(x0, o.y + px(HEAD_H - 2.)), point(x1, o.y + px(HEAD_H))), f.accent));
                }
                let color = if full {
                    white()
                } else if on {
                    f.accent
                } else {
                    f.head_fg
                };
                let line = label(window, cx, super::col_name(c), hsize, color, on);
                line.paint(point(x0 + (x1 - x0 - line.width) / 2., o.y + (px(HEAD_H) - px(hsize * 1.4)) / 2.), px(hsize * 1.4), window, cx).ok();
                window.paint_quad(fill(Bounds::new(point(x1 - px(0.5), o.y), size(px(1.), px(HEAD_H))), f.grid));
            }
        });
    }
    // 行头
    window.paint_quad(fill(Bounds::new(o, size(px(HEAD_W), b.size.height)), f.head_bg));
    for p in panes.iter().filter(|p| p.frozen_c || f.fc == 0) {
        let mask = Bounds::from_corners(point(o.x, p.mask.top()), point(o.x + px(HEAD_W), p.mask.bottom()));
        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
            for r in p.rows.0..=p.rows.1 {
                let (y0, y1) = (p.y(f, r), p.y(f, r + 1));
                if y1 - y0 <= px(0.) {
                    window.paint_quad(
                        fill(Bounds::new(point(o.x + px(8.), y0 - px(2.)), size(px(HEAD_W - 16.), px(4.))), f.head_fg.opacity(0.5)).corner_radii(px(1.)),
                    );
                    continue;
                }
                let on = r >= r0 && r <= r1;
                let full = on && full_rows;
                if full {
                    window.paint_quad(fill(Bounds::from_corners(point(o.x, y0), point(o.x + px(HEAD_W), y1)), f.accent));
                } else if on {
                    window.paint_quad(fill(Bounds::from_corners(point(o.x, y0), point(o.x + px(HEAD_W), y1)), f.accent.opacity(0.14)));
                    window.paint_quad(fill(Bounds::from_corners(point(o.x + px(HEAD_W - 2.), y0), point(o.x + px(HEAD_W), y1)), f.accent));
                }
                let color = if full {
                    white()
                } else if on {
                    f.accent
                } else {
                    f.head_fg
                };
                let line = label(window, cx, r.to_string(), hsize - 0.5, color, on);
                let lh = px(hsize * 1.4);
                line.paint(point(o.x + (px(HEAD_W) - line.width) / 2., y0 + (y1 - y0 - lh) / 2.), lh, window, cx).ok();
                window.paint_quad(fill(Bounds::new(point(o.x, y1 - px(0.5)), size(px(HEAD_W), px(1.))), f.grid));
            }
        });
    }
    // 左上角：全选三角
    let corner = Bounds::new(o, size(px(HEAD_W), px(HEAD_H)));
    window.paint_quad(fill(corner, f.head_bg));
    let mut pb = PathBuilder::fill();
    pb.move_to(point(corner.right() - px(5.), corner.bottom() - px(15.)));
    pb.line_to(point(corner.right() - px(5.), corner.bottom() - px(5.)));
    pb.line_to(point(corner.right() - px(15.), corner.bottom() - px(5.)));
    pb.close();
    if let Ok(path) = pb.build() {
        window.paint_path(path, if f.kind == SelKind::All { f.accent } else { f.head_fg.opacity(0.45) });
    }
    // 表头分隔线
    window.paint_quad(fill(Bounds::new(point(o.x, o.y + px(HEAD_H - 1.)), size(b.size.width, px(1.))), f.grid));
    window.paint_quad(fill(Bounds::new(point(o.x + px(HEAD_W - 1.), o.y), size(px(1.), b.size.height)), f.grid));
    // 冻结线（灰色粗线）
    let freeze = if f.dark { hsla(0., 0., 0.45, 1.) } else { hsla(0.6, 0.06, 0.72, 1.) };
    if f.fr > 0 {
        let y = o.y + px(HEAD_H + f.rt(f.fr + 1)) - px(1.5);
        window.paint_quad(fill(Bounds::new(point(o.x, y), size(b.size.width, px(3.))), freeze));
    }
    if f.fc > 0 {
        let x = o.x + px(HEAD_W + f.cl(f.fc + 1)) - px(1.5);
        window.paint_quad(fill(Bounds::new(point(x, o.y), size(px(3.), b.size.height)), freeze));
    }
}
