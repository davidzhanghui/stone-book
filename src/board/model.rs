//! 画板数据模型。
//!
//! 节点划分自行定义的数据结构：基础图形（composite_shape）、文本（text_shape）、
//! 便签（sticky_note）、图片（image）、画笔（paint）、连线（connector）、分区（section，容器）。
//! 新增字段一律带 serde 默认值，旧画板数据可以直接读取。

use crate::canvas::geom::{boundary_point, points_bounds, polyline_mid};
use crate::canvas::{
    pt, rect, Align, Anchor, Arrow, Col, IconItem, ImageItem, Item, LineItem, PathCmd, PathItem, Pt, Rect, Scene, ShapeItem, ShapeKind, TextItem,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum End {
    Free {
        x: f32,
        y: f32,
    },
    /// 绑定在元素上：ax/ay 为元素内的归一化坐标（0..1）。
    /// (0.5, 0.5) 表示「自动」：按另一端的方位选最近的边；四条边的中点表示固定连在那条边上。
    Bound {
        el: u64,
        ax: f32,
        ay: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Route {
    #[default]
    Straight,
    /// 直角折线
    Elbow,
    Curve,
    /// 圆角直角折线（走线同直角，拐角画成圆弧）
    RoundElbow,
}

impl Route {
    /// 直角类走线（直角 / 圆角直角）。
    pub fn is_elbow(self) -> bool {
        matches!(self, Route::Elbow | Route::RoundElbow)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ElKind {
    Shape(ShapeKind),
    Text,
    Sticky,
    Image {
        path: String,
    },
    /// 图标库里的线性图标（名字见 `board::icons`），颜色用 stroke
    Icon {
        name: String,
    },
    /// 自由笔迹：每个点 [x, y, 压力]
    Pen {
        pts: Vec<[f32; 3]>,
    },
    /// 矢量路径（代码绘图里的饼图扇形、思维导图分支线等）：坐标按元素矩形归一化（0~1），
    /// 移动 / 缩放只需改矩形；填充用 fill，描边用 stroke
    Path {
        segs: Vec<PathSeg>,
    },
    Connector {
        a: End,
        b: End,
        route: Route,
        start: Arrow,
        end: Arrow,
    },
    /// 分区：标题存放在 text；落在分区内的元素随分区移动
    Section,
    /// 表格 / 泳道：列宽、行高、单元格文字（按行存放）、单元格底色（0 为无）。
    /// lane 为泳道：首行 / 首列是泳道名，内部可以放图形且随泳道移动；泳道图标题存放在 text。
    Table {
        cols: Vec<f32>,
        rows: Vec<f32>,
        cells: Vec<String>,
        #[serde(default)]
        fills: Vec<u32>,
        #[serde(default)]
        head_row: bool,
        #[serde(default)]
        head_col: bool,
        #[serde(default)]
        lane: bool,
        /// 各单元格单独设置的文字样式（表格：每个单元格可单独设字号、颜色、加粗、对齐）
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fmts: Vec<CellFmt>,
    },
}

/// 单元格文字样式；各项为 None 时使用表格整体的设置。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellFmt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deco: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Col>,
    /// 水平对齐：1 左、2 中、3 右
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<u8>,
    /// 垂直对齐：1 顶、2 中、3 底
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v_align: Option<u8>,
}

/// 思维导图节点（画板的思维导图：每个节点就是一个可单独设置样式的图形）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Mind {
    /// 父节点；0 表示根节点
    pub parent: u64,
    /// 兄弟节点之间的顺序
    pub seq: f32,
    /// 收起了子节点
    #[serde(default)]
    pub collapsed: bool,
    /// 根节点：布局（见 `mind::LAYOUTS`：0 向右、1 左右分布、2 向下、3 向左、4/5/6 树状图、7/8 时间线）
    #[serde(default)]
    pub layout: u8,
    /// 根节点：分支线（0 曲线、1 直角）
    #[serde(default)]
    pub branch: u8,
    /// 一级分支：放在根节点哪一侧（1 右 / 下、-1 左 / 上、0 按布局自动；点根节点左右两个「+」时设定）
    #[serde(default, skip_serializing_if = "is_zero_i8")]
    pub side: i8,
    /// 布局算出的所在侧（不保存；每次重新布局时更新）
    #[serde(skip)]
    pub dir: i8,
}

fn is_zero_i8(v: &i8) -> bool {
    *v == 0
}

/// 元素里的一段可编辑文字：主文字，或表格的某个单元格。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Main,
    Cell(usize),
}

/// 泳道图标题栏高度。
pub const LANE_TITLE_H: f32 = 40.;

/// 表格 / 泳道的网格线位置（世界坐标，各 n+1 个）。
#[derive(Clone, Debug)]
pub struct Grid {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
}

impl Grid {
    pub fn ncols(&self) -> usize {
        self.xs.len() - 1
    }
    pub fn nrows(&self) -> usize {
        self.ys.len() - 1
    }
    pub fn cell(&self, i: usize) -> Rect {
        let (r, c) = (i / self.ncols(), i % self.ncols());
        rect(self.xs[c], self.ys[r], self.xs[c + 1] - self.xs[c], self.ys[r + 1] - self.ys[r])
    }
    pub fn cell_at(&self, p: Pt) -> Option<usize> {
        let c = self.xs.windows(2).position(|w| p.x >= w[0] && p.x <= w[1])?;
        let r = self.ys.windows(2).position(|w| p.y >= w[0] && p.y <= w[1])?;
        Some(r * self.ncols() + c)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct El {
    pub id: u64,
    pub kind: ElKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub text: String,
    pub stroke: Col,
    #[serde(default)]
    pub fill: Option<Col>,
    #[serde(default = "one_five")]
    pub stroke_w: f32,
    #[serde(default)]
    pub dash: bool,
    /// 点状虚线
    #[serde(default)]
    pub dotted: bool,
    /// 无边框
    #[serde(default)]
    pub no_stroke: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "fs16")]
    pub font_size: f32,
    #[serde(default)]
    pub bold: bool,
    /// 文字颜色；None 表示跟随主题（深色模式自动变浅）
    #[serde(default)]
    pub text_color: Option<Col>,
    /// 文字水平对齐：0 默认（图形居中，文本 / 便签靠左）、1 左、2 中、3 右
    #[serde(default)]
    pub text_align: u8,
    /// 文字垂直对齐：0 默认（图形居中，文本 / 便签顶部）、1 顶、2 中、3 底
    #[serde(default)]
    pub v_align: u8,
    /// 斜体 / 下划线 / 删除线（`canvas::DECO_*` 位）
    #[serde(default)]
    pub deco: u8,
    #[serde(default)]
    pub group: Option<u64>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub seed: u64,
    /// 便签作者
    #[serde(default)]
    pub author: String,
    /// 便签是否显示作者
    #[serde(default = "yes")]
    pub show_author: bool,
    /// 便签字号随大小 / 字数自适应
    #[serde(default)]
    pub auto_font: bool,
    /// 画笔：荧光笔（半透明粗笔）
    #[serde(default)]
    pub highlighter: bool,
    /// 思维导图节点
    #[serde(default)]
    pub mind: Option<Mind>,
    /// 代码分区：生成分区内图形的源码（「PlantUML 绘图」/「Mermaid 绘图」）
    #[serde(default)]
    pub code: Option<String>,
    /// 代码分区的语言：0 PlantUML、1 Mermaid
    #[serde(default, skip_serializing_if = "is_zero")]
    pub code_lang: u8,
    /// 代码分区用「经典样式」生成（否则为「画板样式」）
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code_classic: bool,
    /// 选中部分文字设置的样式分段（选中几个字单独加粗 / 变色），为空表示整段同一样式
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<crate::canvas::Span>,
    /// 多格图形（类图 / 实体等）标题以外各格的文字，对应 `Part::Cell(i)`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<String>,
    /// 连线的中间拐点（代码绘图分层布局算出的走线，绕开中间的节点）。
    /// via_ref 记录生成时两端元素的 [x, y, w, h]：两端一起平移时拐点跟着平移，单独移动或缩放后不再使用，回到自动走线
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_ref: Option<[f32; 8]>,
}

fn one() -> f32 {
    1.
}
fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// 归一化路径的一段（见 `ElKind::Path`）。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PathSeg {
    M(f32, f32),
    L(f32, f32),
    C(f32, f32, f32, f32, f32, f32),
    Z,
}

impl PathSeg {
    /// 世界坐标路径 → 相对矩形 r 的归一化路径。
    pub fn normalize(cmds: &[crate::canvas::PathCmd], r: Rect) -> Vec<PathSeg> {
        use crate::canvas::PathCmd;
        let n = |p: Pt| ((p.x - r.x) / r.w.max(0.01), (p.y - r.y) / r.h.max(0.01));
        cmds.iter()
            .map(|c| match c {
                PathCmd::Move(p) => {
                    let (x, y) = n(*p);
                    PathSeg::M(x, y)
                }
                PathCmd::Line(p) => {
                    let (x, y) = n(*p);
                    PathSeg::L(x, y)
                }
                PathCmd::Cubic(a, b, p) => {
                    let ((ax, ay), (bx, by), (x, y)) = (n(*a), n(*b), n(*p));
                    PathSeg::C(ax, ay, bx, by, x, y)
                }
                PathCmd::Close => PathSeg::Z,
            })
            .collect()
    }

    /// 归一化路径 → 矩形 r 内的世界坐标路径。
    pub fn to_cmds(segs: &[PathSeg], r: Rect) -> Vec<crate::canvas::PathCmd> {
        use crate::canvas::PathCmd;
        let p = |x: f32, y: f32| pt(r.x + x * r.w, r.y + y * r.h);
        segs.iter()
            .map(|s| match *s {
                PathSeg::M(x, y) => PathCmd::Move(p(x, y)),
                PathSeg::L(x, y) => PathCmd::Line(p(x, y)),
                PathSeg::C(ax, ay, bx, by, x, y) => PathCmd::Cubic(p(ax, ay), p(bx, by), p(x, y)),
                PathSeg::Z => PathCmd::Close,
            })
            .collect()
    }
}
fn one_five() -> f32 {
    1.8
}
fn fs16() -> f32 {
    16.
}
fn yes() -> bool {
    true
}

/// 连线端点所在的边（出线方向）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub fn dir(self) -> Pt {
        match self {
            Side::Top => pt(0., -1.),
            Side::Right => pt(1., 0.),
            Side::Bottom => pt(0., 1.),
            Side::Left => pt(-1., 0.),
        }
    }
    /// 该边中点的归一化坐标。
    pub fn anchor(self) -> (f32, f32) {
        match self {
            Side::Top => (0.5, 0.),
            Side::Right => (1., 0.5),
            Side::Bottom => (0.5, 1.),
            Side::Left => (0., 0.5),
        }
    }
    pub const ALL: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

    /// 从 from 指向 to 时，from 一侧应当出线的边。
    pub fn facing(from: Pt, to: Pt) -> Side {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        if dx.abs() >= dy.abs() {
            if dx >= 0. {
                Side::Right
            } else {
                Side::Left
            }
        } else if dy >= 0. {
            Side::Bottom
        } else {
            Side::Top
        }
    }

    fn of_anchor(ax: f32, ay: f32) -> Option<Side> {
        if ay <= 0.001 {
            Some(Side::Top)
        } else if ay >= 0.999 {
            Some(Side::Bottom)
        } else if ax <= 0.001 {
            Some(Side::Left)
        } else if ax >= 0.999 {
            Some(Side::Right)
        } else {
            None
        }
    }
}

impl El {
    /// 以 (ox, oy) 为原点、按 (sx, sy) 缩放后平移到 (nx, ny) 的副本：位置、大小、字号、线宽、笔迹点、表格行列、连线自由端一起缩放。
    pub fn scaled(&self, ox: f32, oy: f32, nx: f32, ny: f32, sx: f32, sy: f32) -> El {
        let k = sx.min(sy);
        let mut e = self.clone();
        e.x = nx + (self.x - ox) * sx;
        e.y = ny + (self.y - oy) * sy;
        e.w = self.w * sx;
        e.h = self.h * sy;
        e.font_size = (self.font_size * k).max(1.);
        e.stroke_w = self.stroke_w * k;
        match &mut e.kind {
            ElKind::Pen { pts } => pts.iter_mut().for_each(|p| {
                p[0] = nx + (p[0] - ox) * sx;
                p[1] = ny + (p[1] - oy) * sy;
            }),
            ElKind::Connector { a, b, .. } => {
                for end in [a, b] {
                    if let End::Free { x, y } = end {
                        *x = nx + (*x - ox) * sx;
                        *y = ny + (*y - oy) * sy;
                    }
                }
            }
            ElKind::Table { cols, rows, .. } => {
                cols.iter_mut().for_each(|c| *c *= sx);
                rows.iter_mut().for_each(|r| *r *= sy);
            }
            _ => {}
        }
        e
    }
    pub fn rect(&self) -> Rect {
        rect(self.x, self.y, self.w, self.h)
    }
    pub fn is_connector(&self) -> bool {
        matches!(self.kind, ElKind::Connector { .. })
    }
    pub fn is_section(&self) -> bool {
        matches!(self.kind, ElKind::Section)
    }
    pub fn is_table(&self) -> bool {
        matches!(self.kind, ElKind::Table { .. })
    }
    pub fn is_lane(&self) -> bool {
        matches!(self.kind, ElKind::Table { lane: true, .. })
    }
    /// 容器：分区与泳道。画在最底层，里面的元素随它移动。
    pub fn is_container(&self) -> bool {
        self.is_section() || self.is_lane()
    }
    pub fn is_mind(&self) -> bool {
        self.mind.is_some()
    }
    pub fn mind_parent(&self) -> Option<u64> {
        self.mind.as_ref().map(|m| m.parent).filter(|p| *p != 0)
    }
    /// 可以作为连线端点、可以快捷创建的「块状」元素。
    pub fn is_box(&self) -> bool {
        matches!(self.kind, ElKind::Shape(_) | ElKind::Text | ElKind::Sticky | ElKind::Image { .. } | ElKind::Icon { .. } | ElKind::Table { lane: false, .. })
    }

    /// 泳道图标题栏的高度（没有标题时为 0）。
    pub fn title_h(&self) -> f32 {
        if self.is_lane() && !self.text.is_empty() {
            LANE_TITLE_H.min(self.h / 2.)
        } else {
            0.
        }
    }

    /// 表格 / 泳道的网格（泳道去掉标题栏）。
    pub fn grid(&self) -> Option<Grid> {
        let ElKind::Table { cols, rows, .. } = &self.kind else { return None };
        if cols.is_empty() || rows.is_empty() {
            return None;
        }
        let mut xs = vec![self.x];
        for w in cols {
            xs.push(xs.last().unwrap() + w);
        }
        let mut ys = vec![self.y + self.title_h()];
        for h in rows {
            ys.push(ys.last().unwrap() + h);
        }
        Some(Grid { xs, ys })
    }

    /// 按列宽 / 行高之和更新元素尺寸（增删行列、拖动网格线后）。
    pub fn sync_grid_size(&mut self) {
        let th = self.title_h();
        if let ElKind::Table { cols, rows, .. } = &self.kind {
            self.w = cols.iter().sum();
            self.h = rows.iter().sum::<f32>() + th;
        }
    }

    /// 是否为表头单元格。
    pub fn is_head_cell(&self, i: usize) -> bool {
        match &self.kind {
            ElKind::Table { cols, head_row, head_col, .. } => (*head_row && i < cols.len()) || (*head_col && i.is_multiple_of(cols.len().max(1))),
            _ => false,
        }
    }

    pub fn part_text(&self, p: Part) -> &str {
        match (p, &self.kind) {
            (Part::Cell(i), ElKind::Table { cells, .. }) => cells.get(i).map(|s| s.as_str()).unwrap_or(""),
            (Part::Cell(i), ElKind::Shape(_)) => self.rows.get(i).map(|s| s.as_str()).unwrap_or(""),
            _ => &self.text,
        }
    }

    pub fn set_part_text(&mut self, p: Part, t: String) {
        match (p, &mut self.kind) {
            (Part::Cell(i), ElKind::Table { cells, .. }) => {
                if let Some(c) = cells.get_mut(i) {
                    *c = t;
                }
            }
            (Part::Cell(i), ElKind::Shape(_)) => {
                if self.rows.len() <= i {
                    self.rows.resize(i + 1, String::new());
                }
                self.rows[i] = t;
                while self.rows.last().is_some_and(|r| r.is_empty()) {
                    self.rows.pop();
                }
            }
            _ => self.text = t,
        }
    }

    /// 元素里所有可编辑的文字（搜索替换用）。
    pub fn parts(&self) -> Vec<Part> {
        match &self.kind {
            ElKind::Pen { .. } | ElKind::Image { .. } | ElKind::Icon { .. } => vec![],
            ElKind::Table { cells, .. } => std::iter::once(Part::Main).chain((0..cells.len()).map(Part::Cell)).collect(),
            ElKind::Shape(_) => std::iter::once(Part::Main).chain((0..self.compartments().len()).map(Part::Cell)).collect(),
            _ => vec![Part::Main],
        }
    }

    /// 多格图形标题以外的各格（世界坐标）；普通图形为空。
    pub fn compartments(&self) -> Vec<Rect> {
        match self.kind {
            ElKind::Shape(k) => crate::canvas::geom::compartments(k, self.rect()),
            _ => vec![],
        }
    }

    /// 点 `p` 落在第几格（标题栏或普通图形返回 None）。
    pub fn compartment_at(&self, p: Pt) -> Option<usize> {
        self.compartments().iter().position(|c| c.contains(p))
    }

    /// 元素整体的文字样式（样式分段以它为基准）。
    pub fn base_fmt(&self) -> crate::canvas::Fmt {
        crate::canvas::Fmt { bold: self.bold, deco: self.deco, color: None }
    }

    /// 写入样式分段并规整（与整体样式相同的分段清空）。
    pub fn set_spans(&mut self, mut spans: Vec<crate::canvas::Span>) {
        crate::canvas::rich::normalize(&mut spans, self.text.len(), self.base_fmt());
        self.spans = spans;
    }

    /// 整体改样式后同步到各分段（整段加粗时选中部分的加粗也一起变）。
    pub fn spans_follow(&mut self, f: impl Fn(&mut crate::canvas::Fmt)) {
        let mut spans = std::mem::take(&mut self.spans);
        for s in &mut spans {
            f(&mut s.fmt);
        }
        self.set_spans(spans);
    }
    pub fn shape_kind(&self) -> ShapeKind {
        match &self.kind {
            ElKind::Shape(k) => *k,
            ElKind::Sticky => ShapeKind::Sticky,
            _ => ShapeKind::Rect,
        }
    }
    /// 实际的水平对齐：0 左、1 中、2 右。
    pub fn h_align(&self) -> u8 {
        match self.text_align {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => {
                if matches!(self.kind, ElKind::Shape(_)) {
                    1
                } else {
                    0
                }
            }
        }
    }
    /// 实际的垂直对齐：0 顶、1 中、2 底。
    pub fn v_align_eff(&self) -> u8 {
        match self.v_align {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => {
                if matches!(self.kind, ElKind::Text | ElKind::Sticky) {
                    0
                } else {
                    1
                }
            }
        }
    }
    /// 画布绘制用的对齐方式（默认图形垂直居中，文本 / 便签顶对齐）。
    pub fn canvas_align(&self) -> Align {
        Align::from_hv(self.h_align(), self.v_align_eff())
    }
    /// 单元格文字对齐：单元格单独设置过的优先；否则表头居中，泳道正文靠左上，表格正文按设置（默认靠左、垂直居中）。
    pub fn cell_align(&self, i: usize) -> Align {
        let f = self.cell_fmt(i);
        let (h, v) = if self.is_head_cell(i) {
            (1, 1)
        } else {
            let v = match self.v_align {
                1 => 0,
                3 => 2,
                _ => 1,
            };
            match (self.is_lane(), self.text_align) {
                (true, _) => (0, 0),
                (false, 2) => (1, v),
                (false, 3) => (2, v),
                _ => (0, v),
            }
        };
        let h = f.align.map(|a| a.clamp(1, 3) - 1).unwrap_or(h);
        let v = f.v_align.map(|a| a.clamp(1, 3) - 1).unwrap_or(v);
        Align::from_hv(h, v)
    }
    /// 单元格单独设置的文字样式（没有设置时为默认值）。
    pub fn cell_fmt(&self, i: usize) -> CellFmt {
        match &self.kind {
            ElKind::Table { fmts, .. } => fmts.get(i).copied().unwrap_or_default(),
            _ => CellFmt::default(),
        }
    }
    /// 修改某个单元格的样式。
    pub fn cell_fmt_mut(&mut self, i: usize, f: impl FnOnce(&mut CellFmt)) {
        if let ElKind::Table { fmts, cells, .. } = &mut self.kind {
            if i < cells.len() {
                fmts.resize(cells.len(), CellFmt::default());
                f(&mut fmts[i]);
            }
            if fmts.iter().all(|f| *f == CellFmt::default()) {
                fmts.clear();
            }
        }
    }
    /// 整个表格改样式时清掉各单元格对应的单独设置（选中整个表格设置后全部统一）。
    pub fn clear_cell_fmts(&mut self, f: impl Fn(&mut CellFmt)) {
        if let ElKind::Table { fmts, .. } = &mut self.kind {
            fmts.iter_mut().for_each(&f);
            if fmts.iter().all(|f| *f == CellFmt::default()) {
                fmts.clear();
            }
        }
    }
    /// 单元格实际字号 / 加粗 / 修饰。
    pub fn cell_font(&self, i: usize) -> f32 {
        self.cell_fmt(i).font_size.unwrap_or(self.font_size)
    }
    pub fn cell_bold(&self, i: usize) -> bool {
        self.cell_fmt(i).bold.unwrap_or(self.bold || (self.is_lane() && self.is_head_cell(i)))
    }
    pub fn cell_deco(&self, i: usize) -> u8 {
        self.cell_fmt(i).deco.unwrap_or(self.deco)
    }

    /// 文字放不下时把元素撑高（图形、类图的格子、表格的行随文字自动变高）。
    /// 高度不低于 `floor`（编辑前的高度，删掉文字后缩回去但不比原来小）；返回是否改变了尺寸。
    /// `lines(文字, 字号, 加粗, 宽度)` 返回换行后的行数。
    pub fn fit_text(&mut self, part: Part, floor: f32, lines: &mut dyn FnMut(&str, f32, bool, f32) -> usize) -> bool {
        use crate::canvas::geom::{compartment_text_area, compartments};
        use crate::canvas::paint::text_area;
        if self.is_mind() || self.locked {
            return false;
        }
        match (&self.kind, part) {
            (ElKind::Table { rows, cols, .. }, Part::Cell(i)) => {
                let (nc, r) = (cols.len().max(1), i / cols.len().max(1));
                let Some(&cur) = rows.get(r) else { return false };
                let g = self.grid().unwrap();
                // 一行里所有单元格都要放得下
                let need = (0..nc)
                    .map(|c| {
                        let k = r * nc + c;
                        let t = self.part_text(Part::Cell(k));
                        if t.is_empty() {
                            return 0.;
                        }
                        let cell = g.cell(k);
                        let a = text_area(ShapeKind::Rect, cell, self.cell_align(k));
                        let fs = self.cell_font(k);
                        lines(t, fs, self.cell_bold(k), a.w.max(4.)) as f32 * fs * 1.38 + (cell.h - a.h) + 2.
                    })
                    .fold(0f32, f32::max);
                let h = need.max(floor).max(24.);
                if (h - cur).abs() < 0.5 {
                    return false;
                }
                if let ElKind::Table { rows, .. } = &mut self.kind {
                    rows[r] = h;
                }
                self.sync_grid_size();
                true
            }
            (ElKind::Shape(_) | ElKind::Sticky, _) => {
                if matches!(self.kind, ElKind::Sticky) && self.auto_font {
                    return false;
                }
                let kind = self.shape_kind();
                let fs = self.effective_font();
                let bold = self.bold;
                let text = self.part_text(part).to_string();
                // 当前高度下的文字区域
                let area = |e: &El| -> Option<Rect> {
                    match part {
                        Part::Main => Some(text_area(kind, e.rect(), e.canvas_align())),
                        Part::Cell(i) => compartments(kind, e.rect()).get(i).map(|c| compartment_text_area(*c)),
                    }
                };
                let orig_h = self.h;
                let need_of = |e: &El, lines: &mut dyn FnMut(&str, f32, bool, f32) -> usize| -> Option<(f32, Rect)> {
                    let a = area(e)?;
                    let b = if part == Part::Main { bold } else { false };
                    let n = if text.is_empty() { 0 } else { lines(&text, fs, b, a.w.max(4.)) };
                    // 主文字上下各留一点空白，不贴边
                    let pad = if part == Part::Main { 10. } else { 2. };
                    Some((n as f32 * fs * 1.38 + pad, a))
                };
                // 先回到 floor，再逐步撑高到放得下（区域高度与元素高度不一定成正比，迭代几次）
                self.h = floor;
                for _ in 0..6 {
                    let Some((need, a)) = need_of(self, lines) else { break };
                    if need <= a.h + 0.5 {
                        break;
                    }
                    let before = a.h;
                    self.h += need - a.h;
                    // 文字区域不随高度变大（类图标题栏、生命线头部等）：放弃撑高
                    if area(self).is_some_and(|b| b.h <= before + 0.1) {
                        self.h = floor;
                        break;
                    }
                }
                (self.h - orig_h).abs() > 0.1
            }
            _ => false,
        }
    }
    /// 编辑前用于 [`fit_text`] 的下限：表格单元格为所在行高，其他为元素高度。
    pub fn part_floor(&self, part: Part) -> f32 {
        match (&self.kind, part) {
            (ElKind::Table { rows, cols, .. }, Part::Cell(i)) => rows.get(i / cols.len().max(1)).copied().unwrap_or(40.),
            _ => self.h,
        }
    }

    /// 实际字号：便签开启自适应时按面积与字数估算（便签「自动」字号）。
    pub fn effective_font(&self) -> f32 {
        if !(self.auto_font && matches!(self.kind, ElKind::Sticky)) {
            return self.font_size;
        }
        let chars = self.text.chars().count().max(1) as f32;
        let area = (self.w - 24.).max(20.) * (self.h - 40.).max(20.);
        (area / (chars * 1.9)).sqrt().clamp(12., 40.)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BoardDoc {
    pub els: Vec<El>,
    pub next_id: u64,
    #[serde(default)]
    pub rough: bool,
    /// 流程图模式（左侧显示流程图形状库）
    #[serde(default)]
    pub flow: bool,
}

/// 一次性建好的查找表：按 id 找元素、两元素之间有哪些连线。
/// 画一帧 / 做一次命中测试时先建一次，避免每条连线都线性查找（元素多时是 O(n²)）。
pub struct Lookup<'a> {
    by_id: HashMap<u64, &'a El>,
    /// (起点元素, 终点元素)
    pairs: HashSet<(u64, u64)>,
    /// 被收起的思维导图节点（不画、不参与命中测试）
    pub hidden: HashSet<u64>,
}

impl<'a> Lookup<'a> {
    pub fn get(&self, id: u64) -> Option<&'a El> {
        self.by_id.get(&id).copied()
    }
    pub fn visible(&self, id: u64) -> bool {
        !self.hidden.contains(&id)
    }
}

/// 便签的 9 种颜色（暖色到冷色排列）。
pub const STICKY_COLORS: [u32; 9] = [0xfff3b0, 0xffe0b2, 0xffd1d1, 0xfcd6e8, 0xe8dcff, 0xd6e8ff, 0xcdf3ee, 0xd8f5d0, 0xeceef1];

impl BoardDoc {
    pub fn new(flow: bool) -> Self {
        BoardDoc { els: vec![], next_id: 1, rough: false, flow }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    pub fn get(&self, id: u64) -> Option<&El> {
        self.els.iter().find(|e| e.id == id)
    }

    pub fn lookup(&self) -> Lookup<'_> {
        let mut pairs = HashSet::new();
        for e in &self.els {
            if let ElKind::Connector { a: End::Bound { el: x, .. }, b: End::Bound { el: y, .. }, .. } = &e.kind {
                pairs.insert((*x, *y));
            }
        }
        Lookup { by_id: self.els.iter().map(|e| (e.id, e)).collect(), pairs, hidden: self.mind_hidden() }
    }

    pub fn index(&self, id: u64) -> Option<usize> {
        self.els.iter().position(|e| e.id == id)
    }

    pub fn new_el(&mut self, kind: ElKind, r: Rect, dark: bool) -> El {
        let id = self.alloc_id();
        let ink = if dark { Col::rgb(0xe3e4e6) } else { Col::rgb(0x1f2329) };
        let (stroke, fill) = match &kind {
            ElKind::Shape(_) => (if dark { Col::rgb(0x5b8cff) } else { Col::rgb(0x3370ff) }, Some(if dark { Col::rgb(0x22304d) } else { Col::rgb(0xeef3ff) })),
            ElKind::Sticky => (Col::rgb(0xd9b84a), Some(Col::rgb(STICKY_COLORS[0]))),
            ElKind::Connector { .. } => (if dark { Col::rgb(0xa6a9ae) } else { Col::rgb(0x646a73) }, None),
            ElKind::Section => (if dark { Col::rgb(0x4a4d55) } else { Col::rgb(0xdee0e3) }, Some(if dark { Col::rgb(0x25272b) } else { Col::rgb(0xf5f6f7) })),
            ElKind::Table { .. } => (if dark { Col::rgb(0x4a4d55) } else { Col::rgb(0xc9cdd4) }, None),
            _ => (ink, None),
        };
        El {
            id,
            kind,
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            text: String::new(),
            stroke,
            fill,
            stroke_w: 1.8,
            dash: false,
            dotted: false,
            no_stroke: false,
            opacity: 1.,
            font_size: 16.,
            bold: false,
            text_color: None,
            text_align: 0,
            v_align: 0,
            deco: 0,
            group: None,
            locked: false,
            seed: id * 7919 % 100_000,
            author: String::new(),
            show_author: true,
            auto_font: false,
            highlighter: false,
            mind: None,
            code: None,
            code_lang: 0,
            code_classic: false,
            spans: vec![],
            rows: vec![],
            via: vec![],
            via_ref: None,
        }
    }

    /// 端点位置与出线方向。`toward` 为另一端的大致位置（自动选边用）。
    fn end_info(lk: &Lookup, e: &End, toward: Pt, route: Route) -> (Pt, Option<Pt>) {
        match e {
            End::Free { x, y } => (pt(*x, *y), None),
            End::Bound { el, ax, ay } => match lk.get(*el) {
                Some(t) => {
                    let r = t.rect();
                    if let Some(side) = Side::of_anchor(*ax, *ay) {
                        return (pt(r.x + ax * r.w, r.y + ay * r.h), Some(side.dir()));
                    }
                    let auto = (*ax - 0.5).abs() < 0.01 && (*ay - 0.5).abs() < 0.01;
                    if !auto {
                        return (pt(r.x + ax * r.w, r.y + ay * r.h), None);
                    }
                    match route {
                        // 直线：沿中心连线贴到形状边界
                        Route::Straight => (boundary_point(t.shape_kind(), r, toward), None),
                        // 折线 / 曲线：从朝向另一端的那条边的中点垂直出线（默认效果）
                        _ => {
                            let side = Side::facing(r.center(), toward);
                            let (sx, sy) = side.anchor();
                            (pt(r.x + sx * r.w, r.y + sy * r.h), Some(side.dir()))
                        }
                    }
                }
                None => (toward, None),
            },
        }
    }

    /// 连线经过的点（曲线已采样成折线）。
    pub fn connector_points(&self, el: &El) -> Vec<Pt> {
        Self::connector_points_in(&self.lookup(), el)
    }

    /// 同上，复用已建好的查找表（批量计算时用）。
    pub fn connector_points_in(lk: &Lookup, el: &El) -> Vec<Pt> {
        let ElKind::Connector { route, .. } = &el.kind else { return vec![] };
        let (pts, da, db, via) = Self::skeleton_in(lk, el);
        match route {
            Route::Straight | Route::Elbow => pts,
            Route::RoundElbow => round_corners(&pts, 10.),
            Route::Curve if via => sample_cmds(&crate::canvas::geom::smooth_cmds_dir(&pts, da, db)),
            Route::Curve => {
                let (pa, pb) = (pts[0], pts[1]);
                let k = (pa.dist(pb) * 0.4).clamp(30., 160.);
                let (c1, c2) = (pa.add(da.scale(k)), pb.add(db.scale(k)));
                (0..=24)
                    .map(|i| {
                        let t = i as f32 / 24.;
                        let u = 1. - t;
                        pt(
                            u * u * u * pa.x + 3. * u * u * t * c1.x + 3. * u * t * t * c2.x + t * t * t * pb.x,
                            u * u * u * pa.y + 3. * u * u * t * c1.y + 3. * u * t * t * c2.y + t * t * t * pb.y,
                        )
                    })
                    .collect()
            }
        }
    }

    /// 连线的骨架：折线为未倒圆角的直角路径；直线 / 曲线为 [起点, 拐点…, 终点]。
    /// 另返回曲线两端的出线方向，以及是否沿用了保存的拐点。
    pub fn connector_skeleton(&self, el: &El) -> Vec<Pt> {
        Self::skeleton_in(&self.lookup(), el).0
    }

    fn skeleton_in(lk: &Lookup, el: &El) -> (Vec<Pt>, Pt, Pt, bool) {
        let ElKind::Connector { a, b, route, .. } = &el.kind else { return (vec![], Pt::default(), Pt::default(), false) };
        let center = |e: &End| match e {
            End::Free { x, y } => pt(*x, *y),
            End::Bound { el, .. } => lk.get(*el).map(|t| t.rect().center()).unwrap_or_default(),
        };
        let (ca, cb) = (center(a), center(b));
        if let Some(via) = Self::live_via(lk, el) {
            let (pa, da) = Self::end_info(lk, a, via[0], *route);
            let (pb, db) = Self::end_info(lk, b, *via.last().unwrap(), *route);
            let mut pts = vec![pa];
            pts.extend(via);
            pts.push(pb);
            let n = pts.len();
            let da = da.unwrap_or_else(|| anchor_dir(a, pa, pts[1]));
            let db = db.unwrap_or_else(|| anchor_dir(b, pb, pts[n - 2]));
            let pts = if route.is_elbow() { orthogonalize(&pts) } else { pts };
            return (pts, da, db, true);
        }
        let (mut pa, mut da) = Self::end_info(lk, a, cb, *route);
        let (mut pb, mut db) = Self::end_info(lk, b, ca, *route);
        if route.is_elbow() {
            let auto = |e: &End| matches!(e, End::Bound { ax, ay, .. } if (*ax - 0.5).abs() < 0.01 && (*ay - 0.5).abs() < 0.01);
            let rect_of = |e: &End| match e {
                End::Bound { el, .. } => lk.get(*el).map(|t| t.rect()),
                _ => None,
            };
            // 只有边是平的元素（矩形类）才能把连线挪离边的中点；菱形、圆形等挪开后会和轮廓脱开
            let flat = |e: &End| match e {
                End::Bound { el, .. } => lk.get(*el).is_some_and(|t| {
                    !matches!(
                        t.shape_kind(),
                        ShapeKind::Diamond
                            | ShapeKind::Ellipse
                            | ShapeKind::Circle
                            | ShapeKind::DoubleCircle
                            | ShapeKind::Triangle
                            | ShapeKind::RightTriangle
                            | ShapeKind::Star
                            | ShapeKind::Pentagon
                            | ShapeKind::Octagon
                            | ShapeKind::Cross
                            | ShapeKind::BlockArrow
                            | ShapeKind::DoubleArrow
                            | ShapeKind::StartDot
                            | ShapeKind::EndDot
                    )
                }),
                _ => false,
            };
            let can_slide = flat(a) && flat(b);
            if let (true, true, Some(ra), Some(rb)) = (auto(a), auto(b), rect_of(a), rect_of(b)) {
                // 回边：两个元素之间还有一条反向连线时，这条从右侧绕回，避免两条线重叠
                let reverse = match (a, b) {
                    (End::Bound { el: p, .. }, End::Bound { el: q, .. }) => p != q && lk.pairs.contains(&(*q, *p)),
                    _ => false,
                };
                let backward = if (ca.y - cb.y).abs() > (ca.x - cb.x).abs() { ca.y > cb.y } else { ca.x > cb.x };
                if reverse && backward {
                    pa = pt(ra.right(), ra.center().y);
                    pb = pt(rb.right(), rb.center().y);
                    da = Some(pt(1., 0.));
                    db = Some(pt(1., 0.));
                } else if let (Some(d1), Some(d2)) = (da, db) {
                    // 两端正对且在横向（或纵向）上有重叠：取重叠区中点拉成直线，去掉小台阶
                    if !can_slide {
                    } else if d1.x == 0. && d2.x == 0. && d1.y == -d2.y {
                        let (lo, hi) = (ra.x.max(rb.x), ra.right().min(rb.right()));
                        if hi - lo > 12. {
                            let x = ((lo + hi) / 2.).clamp(lo, hi);
                            let x = if (ca.x - cb.x).abs() < 1. { ca.x } else { x };
                            pa.x = x;
                            pb.x = x;
                        }
                    } else if d1.y == 0. && d2.y == 0. && d1.x == -d2.x {
                        let (lo, hi) = (ra.y.max(rb.y), ra.bottom().min(rb.bottom()));
                        if hi - lo > 12. {
                            let y = if (ca.y - cb.y).abs() < 1. { ca.y } else { (lo + hi) / 2. };
                            pa.y = y;
                            pb.y = y;
                        }
                    }
                }
            }
        }
        let pts = if route.is_elbow() { orth_route(pa, da, pb, db) } else { vec![pa, pb] };
        let da2 = da.unwrap_or_else(|| anchor_dir(a, pa, pb));
        let db2 = db.unwrap_or_else(|| anchor_dir(b, pb, pa));
        (pts, da2, db2, false)
    }

    /// 端点元素的 [x, y, w, h]（悬空端点为其坐标）。
    fn end_ref(lk: &Lookup, e: &End) -> [f32; 4] {
        match e {
            End::Free { x, y } => [*x, *y, 0., 0.],
            End::Bound { el, .. } => lk.get(*el).map(|t| [t.x, t.y, t.w, t.h]).unwrap_or_default(),
        }
    }

    /// 仍然有效的中间拐点：两端元素与生成时相比尺寸不变、平移量相同，拐点随之平移；否则为 None。
    fn live_via(lk: &Lookup, el: &El) -> Option<Vec<Pt>> {
        let ElKind::Connector { a, b, .. } = &el.kind else { return None };
        let r = el.via_ref.filter(|_| !el.via.is_empty())?;
        let (ra, rb) = (Self::end_ref(lk, a), Self::end_ref(lk, b));
        let close = |x: f32, y: f32| (x - y).abs() < 0.5;
        let (dx, dy) = (ra[0] - r[0], ra[1] - r[1]);
        let ok = close(ra[2], r[2]) && close(ra[3], r[3]) && close(rb[2], r[6]) && close(rb[3], r[7]) && close(rb[0] - r[4], dx) && close(rb[1] - r[5], dy);
        ok.then(|| el.via.iter().map(|p| pt(p[0] + dx, p[1] + dy)).collect())
    }

    /// 设置连线的中间拐点，并记下两端元素当前的位置尺寸。
    pub fn set_via(&mut self, id: u64, via: &[Pt]) {
        let Some(el) = self.get(id) else { return };
        let ElKind::Connector { a, b, .. } = &el.kind else { return };
        let lk = self.lookup();
        let (ra, rb) = (Self::end_ref(&lk, a), Self::end_ref(&lk, b));
        let r = [ra[0], ra[1], ra[2], ra[3], rb[0], rb[1], rb[2], rb[3]];
        if let Some(i) = self.index(id) {
            self.els[i].via = via.iter().map(|p| [p.x, p.y]).collect();
            self.els[i].via_ref = (!via.is_empty()).then_some(r);
        }
    }

    /// 手动调整走线：把骨架 [起点, 拐点…, 终点] 存成拐点，两端改成固定在当前位置的锚点（避免自动选边后跳到别的边）。
    pub fn set_route(&mut self, id: u64, pts: &[Pt]) {
        let Some(i) = self.index(id) else { return };
        let ElKind::Connector { route, .. } = self.els[i].kind else { return };
        let pts = if route.is_elbow() { simplify(pts.to_vec()) } else { pts.to_vec() };
        if pts.len() < 2 {
            return;
        }
        let pin = |e: &End, p: Pt| match e {
            End::Bound { el, .. } => match self.get(*el) {
                Some(t) => {
                    let (ax, ay) = (((p.x - t.x) / t.w.max(1.)).clamp(0., 1.), ((p.y - t.y) / t.h.max(1.)).clamp(0., 1.));
                    let ax = if (ax - 0.5).abs() < 0.01 && (ay - 0.5).abs() < 0.01 { 0.511 } else { ax };
                    End::Bound { el: *el, ax, ay }
                }
                None => e.clone(),
            },
            End::Free { .. } => End::Free { x: p.x, y: p.y },
        };
        let ElKind::Connector { a, b, .. } = &self.els[i].kind else { return };
        let (na, nb) = (pin(a, pts[0]), pin(b, *pts.last().unwrap()));
        if let ElKind::Connector { a, b, .. } = &mut self.els[i].kind {
            *a = na;
            *b = nb;
        }
        self.set_via(id, &pts[1..pts.len() - 1]);
    }

    /// 连线说明的位置（连线中点）。
    pub fn connector_mid(&self, el: &El) -> Pt {
        polyline_mid(&self.connector_points(el))
    }

    /// 元素的包围盒（连线与笔迹按实际点计算）。
    pub fn bounds_of(&self, el: &El) -> Rect {
        Self::bounds_in(&self.lookup(), el)
    }

    pub fn bounds_in(lk: &Lookup, el: &El) -> Rect {
        match &el.kind {
            ElKind::Connector { .. } => points_bounds(&Self::connector_points_in(lk, el)).inflate(4.),
            ElKind::Pen { pts } => {
                let p: Vec<Pt> = pts.iter().map(|p| pt(p[0], p[1])).collect();
                points_bounds(&p).inflate(el.stroke_w * if el.highlighter { 6. } else { 2. })
            }
            _ => el.rect(),
        }
    }

    pub fn bounds(&self) -> Option<Rect> {
        let lk = self.lookup();
        self.els.iter().filter(|e| lk.visible(e.id)).map(|e| Self::bounds_in(&lk, e)).reduce(|a, b| a.union(&b))
    }

    /// 元素的放置区域：泳道是网格部分，分区是整个矩形。
    fn container_body(e: &El) -> Rect {
        match e.grid() {
            Some(g) if e.is_lane() => rect(g.xs[0], g.ys[0], g.xs[g.xs.len() - 1] - g.xs[0], g.ys[g.ys.len() - 1] - g.ys[0]),
            _ => e.rect(),
        }
    }

    /// 容器（分区 / 泳道）内的元素：包围盒完全落在容器里的元素（较小的容器也算），以及两端都在容器内的连线。
    pub fn section_children(&self, id: u64) -> Vec<u64> {
        let Some(sec) = self.get(id).filter(|e| e.is_container()) else { return vec![] };
        let r = Self::container_body(sec);
        let area = sec.w * sec.h;
        let inside = |b: Rect| b.x >= r.x - 0.5 && b.y >= r.y - 0.5 && b.right() <= r.right() + 0.5 && b.bottom() <= r.bottom() + 0.5;
        let lk = self.lookup();
        self.els
            .iter()
            .filter(|e| e.id != id)
            .filter(|e| !e.is_container() || e.w * e.h < area)
            .filter(|e| inside(Self::bounds_in(&lk, e)))
            .map(|e| e.id)
            .collect()
    }

    /// 转为可绘制的场景。
    pub fn to_scene(&self, dark: bool) -> Scene {
        self.to_scene_opts(dark, None)
    }

    /// 转为可绘制的场景；`hide_text` 指定的文字不画（正在原位编辑时由输入框负责显示）。
    pub fn to_scene_opts(&self, dark: bool, hide_text: Option<(u64, Part)>) -> Scene {
        let mut scene = Scene { rough: self.rough, ..Default::default() };
        let lk = self.lookup();
        let ink = if dark { Col::rgb(0xe3e4e6) } else { Col::rgb(0x1f2329) };
        let bg = if dark { Col::rgb(0x1b1c1f) } else { Col::rgb(0xffffff) };
        let text_of = |el: &El| if hide_text == Some((el.id, Part::Main)) { String::new() } else { el.text.clone() };
        // 多格图形各格文字，正在编辑的那一格不画（由输入框显示）
        let rows_of = |el: &El| {
            el.rows.iter().enumerate().map(|(i, r)| if hide_text == Some((el.id, Part::Cell(i))) { String::new() } else { r.clone() }).collect::<Vec<_>>()
        };
        // 默认墨色在深色模式下自动反白；用户选过的颜色保持原样
        // 默认墨色在深色模式下反白；但浅色底（浅色模式下建的图形、便签色）上仍用深色字，保证看得清
        let text_col = |el: &El| match el.text_color {
            Some(c) if c != Col::rgb(0x1f2329) => c,
            _ if dark && el.fill.is_some_and(|f| f.alpha() > 0.5 && luminance(f) > 0.6) => Col::rgb(0x1f2329),
            _ => ink,
        };
        // 容器（分区 / 泳道）永远画在最底层，其上是思维导图的分支线，再上面是其他元素
        let n_containers = self.els.iter().filter(|e| e.is_container()).count();
        let ordered = self.els.iter().filter(|e| e.is_container()).chain(self.els.iter().filter(|e| !e.is_container())).filter(|e| lk.visible(e.id));
        for (k, el) in ordered.enumerate() {
            if k == n_containers {
                self.mind_edges(&lk, &mut scene);
            }
            let a = el.opacity;
            match &el.kind {
                ElKind::Shape(k) => {
                    let mut s = ShapeItem::new(*k, el.rect());
                    s.id = format!("el:{}", el.id);
                    s.fill = el.fill;
                    s.stroke = (!el.no_stroke).then_some(el.stroke);
                    s.stroke_w = el.stroke_w;
                    s.dash = el.dash;
                    s.dotted = el.dotted;
                    s.radius = 10.;
                    s.text = text_of(el);
                    s.text_color = text_col(el);
                    s.font_size = el.font_size;
                    s.bold = el.bold;
                    s.deco = el.deco;
                    s.spans = el.spans.clone();
                    s.rows = rows_of(el);
                    s.align = el.canvas_align();
                    s.opacity = a;
                    s.seed = el.seed;
                    scene.items.push(Item::Shape(s));
                }
                ElKind::Sticky => {
                    let mut s = ShapeItem::new(ShapeKind::Sticky, el.rect());
                    s.id = format!("el:{}", el.id);
                    s.fill = el.fill;
                    s.stroke = None;
                    s.radius = 4.;
                    s.shadow = true;
                    s.text = text_of(el);
                    // 便签底色固定偏浅，文字始终用深色
                    s.text_color = el.text_color.unwrap_or(Col::rgb(0x1f2329));
                    s.font_size = el.effective_font();
                    s.bold = el.bold;
                    s.deco = el.deco;
                    s.spans = el.spans.clone();
                    s.align = el.canvas_align();
                    s.opacity = a;
                    s.seed = el.seed;
                    scene.items.push(Item::Shape(s));
                    if el.show_author && !el.author.is_empty() && el.h > 60. {
                        scene.items.push(Item::Text(TextItem {
                            pos: pt(el.x + el.w - 10., el.y + el.h - 12.),
                            text: el.author.clone(),
                            size: 11.,
                            color: Col::rgb(0x1f2329).with_alpha(0.45),
                            bold: false,
                            anchor: Anchor::End,
                        }));
                    }
                }
                ElKind::Text => {
                    let mut s = ShapeItem::new(ShapeKind::Rect, el.rect());
                    s.id = format!("el:{}", el.id);
                    s.fill = el.fill;
                    s.stroke = None;
                    s.text = text_of(el);
                    s.text_color = match el.text_color {
                        Some(c) => text_col(el).with_alpha(c.alpha()),
                        // 旧数据把文字颜色放在 stroke 里
                        None if el.stroke != Col::rgb(0x1f2329) && el.stroke != Col::rgb(0xe3e4e6) => el.stroke,
                        None => ink,
                    };
                    s.font_size = el.font_size;
                    s.bold = el.bold;
                    s.deco = el.deco;
                    s.spans = el.spans.clone();
                    s.align = el.canvas_align();
                    s.opacity = a;
                    scene.items.push(Item::Shape(s));
                }
                ElKind::Image { path } => scene.items.push(Item::Image(ImageItem { rect: el.rect(), path: path.clone() })),
                ElKind::Path { segs } => scene.items.push(Item::Path(PathItem {
                    cmds: PathSeg::to_cmds(segs, el.rect()),
                    fill: el.fill.map(|f| f.with_alpha(f.alpha() * a)),
                    stroke: (!el.no_stroke).then(|| el.stroke.with_alpha(el.stroke.alpha() * a)),
                    width: el.stroke_w,
                    dash: el.dash,
                })),
                ElKind::Icon { name } => {
                    scene.items.push(Item::Icon(IconItem { rect: el.rect(), name: name.clone(), color: el.stroke.with_alpha(el.stroke.alpha() * a) }))
                }
                ElKind::Pen { pts } => {
                    let (size, alpha) = if el.highlighter { (el.stroke_w * 6., 0.38) } else { (el.stroke_w * 2.2, 1.) };
                    let cmds = freehand_outline(pts, size);
                    scene.items.push(Item::Path(PathItem { cmds, fill: Some(el.stroke.with_alpha(a * alpha)), stroke: None, width: 0., dash: false }));
                }
                ElKind::Connector { start, end, .. } => {
                    let mut l = LineItem::new(Self::connector_points_in(&lk, el));
                    l.id = format!("el:{}", el.id);
                    l.stroke = el.stroke.with_alpha(a);
                    l.width = el.stroke_w;
                    l.dash = el.dash;
                    l.dotted = el.dotted;
                    l.start = *start;
                    l.end = *end;
                    l.label = text_of(el);
                    l.label_color = text_col(el);
                    l.label_bg = Some(bg);
                    l.font_size = (el.font_size - 2.).max(10.);
                    l.seed = el.seed;
                    scene.items.push(Item::Line(l));
                }
                ElKind::Table { fills, lane, .. } => {
                    let Some(g) = el.grid() else { continue };
                    let line_col = if el.no_stroke { None } else { Some(el.stroke.with_alpha(a)) };
                    let head_fill = if *lane {
                        if dark {
                            Col::rgb(0x2a3142)
                        } else {
                            Col::rgb(0xeef3ff)
                        }
                    } else if dark {
                        Col::rgb(0x2b2d31)
                    } else {
                        Col::rgb(0xf5f6f7)
                    };
                    let mut bgs = ShapeItem::new(ShapeKind::Rect, el.rect());
                    bgs.id = format!("el:{}", el.id);
                    bgs.fill = el.fill;
                    bgs.stroke = None;
                    bgs.opacity = a;
                    scene.items.push(Item::Shape(bgs));
                    // 泳道图标题栏
                    let th = el.title_h();
                    if th > 0. {
                        let mut t = ShapeItem::new(ShapeKind::Rect, rect(el.x, el.y, el.w, th));
                        t.fill = Some(head_fill);
                        t.stroke = line_col;
                        t.stroke_w = 1.;
                        t.text = text_of(el);
                        t.text_color = text_col(el);
                        t.font_size = el.font_size + 2.;
                        t.bold = true;
                        t.opacity = a;
                        scene.items.push(Item::Shape(t));
                    }
                    for i in 0..g.ncols() * g.nrows() {
                        let r = g.cell(i);
                        let head = el.is_head_cell(i);
                        let fill = match fills.get(i).copied().unwrap_or(0) {
                            0 if head => Some(head_fill),
                            0 => None,
                            c => Some(Col(c)),
                        };
                        let text = if hide_text == Some((el.id, Part::Cell(i))) { String::new() } else { el.part_text(Part::Cell(i)).to_string() };
                        if fill.is_none() && text.is_empty() {
                            continue;
                        }
                        let cf = el.cell_fmt(i);
                        let mut c = ShapeItem::new(ShapeKind::Rect, r);
                        c.fill = fill;
                        c.stroke = None;
                        c.text = text;
                        c.text_color = match cf.color {
                            Some(col) if col != Col::rgb(0x1f2329) => col,
                            _ => text_col(el),
                        };
                        c.font_size = el.cell_font(i);
                        c.bold = el.cell_bold(i);
                        c.deco = el.cell_deco(i);
                        c.align = el.cell_align(i);
                        c.opacity = a;
                        scene.items.push(Item::Shape(c));
                    }
                    if let Some(lc) = line_col {
                        let mut line = |p: Pt, q: Pt, w: f32| {
                            let mut l = LineItem::new(vec![p, q]);
                            l.stroke = lc;
                            l.width = w;
                            l.end = Arrow::None;
                            l.label_bg = None;
                            scene.items.push(Item::Line(l));
                        };
                        let (x0, x1, y0, y1) = (g.xs[0], g.xs[g.xs.len() - 1], g.ys[0], g.ys[g.ys.len() - 1]);
                        for x in &g.xs {
                            line(pt(*x, y0), pt(*x, y1), el.stroke_w);
                        }
                        for y in &g.ys {
                            line(pt(x0, *y), pt(x1, *y), el.stroke_w);
                        }
                    }
                }
                ElKind::Section => {
                    let mut s = ShapeItem::new(ShapeKind::RoundRect, el.rect());
                    s.id = format!("sec:{}", el.id);
                    s.fill = el.fill;
                    s.stroke = (!el.no_stroke).then_some(el.stroke);
                    s.stroke_w = 1.;
                    s.radius = 8.;
                    s.opacity = a;
                    s.seed = el.seed;
                    scene.items.push(Item::Shape(s));
                    let title = text_of(el);
                    if el.code.is_some() {
                        // 代码绘图的标题：左上角外侧的橙色标签
                        if !title.is_empty() {
                            let w = crate::canvas::geom::estimate_width(&title, 13., true) + 16.;
                            let mut tag = ShapeItem::new(ShapeKind::RoundRect, code_title_rect(el, w));
                            tag.fill = Some(code_tag());
                            tag.stroke = None;
                            tag.radius = 4.;
                            tag.text = title;
                            tag.text_color = Col::rgb(0xffffff);
                            tag.font_size = 13.;
                            tag.bold = true;
                            tag.opacity = a;
                            scene.items.push(Item::Shape(tag));
                        }
                    } else if !title.is_empty() {
                        scene.items.push(Item::Text(TextItem {
                            pos: pt(el.x + 2., el.y - 12.),
                            text: title,
                            size: 13.,
                            color: if dark { Col::rgb(0xa6a9ae) } else { Col::rgb(0x646a73) },
                            bold: true,
                            anchor: Anchor::Start,
                        }));
                    }
                }
            }
        }
        scene
    }
}

/// 代码绘图标题标签的底色。
pub fn code_tag() -> Col {
    Col::rgb(0xd9773d)
}

/// 代码绘图标题标签的位置（宽度 w）：贴在分区左上角的外侧。
pub fn code_title_rect(el: &El, w: f32) -> Rect {
    rect(el.x, el.y - 30., w.min(el.w.max(60.)), 24.)
}

/// 颜色亮度（0 黑 ~ 1 白）。
fn luminance(c: Col) -> f32 {
    let v = c.0;
    let (r, g, b) = (((v >> 24) & 0xff) as f32, ((v >> 16) & 0xff) as f32, ((v >> 8) & 0xff) as f32);
    (0.299 * r + 0.587 * g + 0.114 * b) / 255.
}

/// 主方向（水平或垂直的单位向量）。
/// 端点的出线方向：绑定在元素上时取锚点相对元素中心的主方向（菱形左下边上的锚点向下），悬空时朝向相邻点。
fn anchor_dir(e: &End, p: Pt, next: Pt) -> Pt {
    match e {
        End::Bound { ax, ay, .. } if (ax - 0.5).abs() > 0.01 || (ay - 0.5).abs() > 0.01 => {
            if (ay - 0.5).abs() >= (ax - 0.5).abs() {
                pt(0., (ay - 0.5).signum())
            } else {
                pt((ax - 0.5).signum(), 0.)
            }
        }
        _ => main_dir(next.sub(p)),
    }
}

/// 把曲线路径采样成折线。
fn sample_cmds(cmds: &[PathCmd]) -> Vec<Pt> {
    let mut out = vec![];
    let mut cur = pt(0., 0.);
    for &c in cmds {
        match c {
            PathCmd::Move(p) | PathCmd::Line(p) => {
                out.push(p);
                cur = p;
            }
            PathCmd::Cubic(c1, c2, p) => {
                for i in 1..=12 {
                    let t = i as f32 / 12.;
                    let u = 1. - t;
                    out.push(pt(
                        u * u * u * cur.x + 3. * u * u * t * c1.x + 3. * u * t * t * c2.x + t * t * t * p.x,
                        u * u * u * cur.y + 3. * u * u * t * c1.y + 3. * u * t * t * c2.y + t * t * t * p.y,
                    ));
                }
                cur = p;
            }
            PathCmd::Close => {}
        }
    }
    out
}

/// 把经过各点的折线改成横平竖直：不在同一水平 / 竖直线上的相邻两点之间按主方向走 Z 形。
fn orthogonalize(pts: &[Pt]) -> Vec<Pt> {
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let (p, q) = (w[0], w[1]);
        if (p.x - q.x).abs() >= 0.5 && (p.y - q.y).abs() >= 0.5 {
            if (q.y - p.y).abs() >= (q.x - p.x).abs() {
                let m = (p.y + q.y) / 2.;
                out.extend([pt(p.x, m), pt(q.x, m)]);
            } else {
                let m = (p.x + q.x) / 2.;
                out.extend([pt(m, p.y), pt(m, q.y)]);
            }
        }
        out.push(q);
    }
    // 去掉共线的中间点
    let mut res: Vec<Pt> = vec![];
    for p in out {
        if res.last().is_some_and(|l: &Pt| l.dist(p) < 0.5) {
            continue;
        }
        if res.len() >= 2 {
            let (a, b) = (res[res.len() - 2], res[res.len() - 1]);
            if ((a.x - b.x).abs() < 0.5 && (b.x - p.x).abs() < 0.5) || ((a.y - b.y).abs() < 0.5 && (b.y - p.y).abs() < 0.5) {
                res.pop();
            }
        }
        res.push(p);
    }
    res
}

fn main_dir(d: Pt) -> Pt {
    if d.x.abs() >= d.y.abs() {
        pt(d.x.signum(), 0.)
    } else {
        pt(0., d.y.signum())
    }
}

/// 直角连线路由：两端按出线方向先伸出一小段，再用 L / Z 形折线相连，最后去掉多余的拐点。
pub fn orth_route(pa: Pt, da: Option<Pt>, pb: Pt, db: Option<Pt>) -> Vec<Pt> {
    const STUB: f32 = 24.;
    let da = da.unwrap_or_else(|| main_dir(pb.sub(pa)));
    let db = db.unwrap_or_else(|| main_dir(pa.sub(pb)));
    let sa = pa.add(da.scale(STUB));
    let sb = pb.add(db.scale(STUB));
    let ha = da.y == 0.;
    let hb = db.y == 0.;
    let mut pts = vec![pa, sa];
    if da == db {
        // 两端朝同一方向（回边从同一侧绕回）：先走到更外侧，再折回来
        if ha {
            let x = if da.x > 0. { sa.x.max(sb.x) } else { sa.x.min(sb.x) };
            pts.extend([pt(x, sa.y), pt(x, sb.y)]);
        } else {
            let y = if da.y > 0. { sa.y.max(sb.y) } else { sa.y.min(sb.y) };
            pts.extend([pt(sa.x, y), pt(sb.x, y)]);
        }
    } else if ha && hb {
        // 两端都横向出线：中间竖着走
        let mx = (sa.x + sb.x) / 2.;
        let facing = (sb.x - sa.x) * da.x >= 0.;
        if facing {
            pts.extend([pt(mx, sa.y), pt(mx, sb.y)]);
        } else {
            let my = (sa.y + sb.y) / 2.;
            pts.extend([pt(sa.x, my), pt(sb.x, my)]);
        }
    } else if !ha && !hb {
        let my = (sa.y + sb.y) / 2.;
        let facing = (sb.y - sa.y) * da.y >= 0.;
        if facing {
            pts.extend([pt(sa.x, my), pt(sb.x, my)]);
        } else {
            let mx = (sa.x + sb.x) / 2.;
            pts.extend([pt(mx, sa.y), pt(mx, sb.y)]);
        }
    } else if ha {
        // a 横出、b 竖出：优先在 (sb.x, sa.y) 拐一次
        pts.push(pt(sb.x, sa.y));
    } else {
        pts.push(pt(sa.x, sb.y));
    }
    pts.extend([sb, pb]);
    simplify(pts)
}

/// 折线的拐角改成圆弧（采样成短线段），半径不超过相邻两段各自的一半。
pub fn round_corners(pts: &[Pt], radius: f32) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let r = radius.min(a.dist(b) / 2.).min(b.dist(c) / 2.);
        if r < 0.5 {
            out.push(b);
            continue;
        }
        let p0 = b.add(a.sub(b).scale(r / a.dist(b)));
        let p2 = b.add(c.sub(b).scale(r / b.dist(c)));
        // 以拐点为控制点的二次贝塞尔
        for i in 0..=6 {
            let t = i as f32 / 6.;
            let u = 1. - t;
            out.push(pt(u * u * p0.x + 2. * u * t * b.x + t * t * p2.x, u * u * p0.y + 2. * u * t * b.y + t * t * p2.y));
        }
    }
    out.push(pts[pts.len() - 1]);
    out
}

/// 去掉重复点与共线的中间点。
fn simplify(pts: Vec<Pt>) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_some_and(|q| q.dist(p) < 0.5) {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let collinear = ((a.x - b.x).abs() < 0.5 && (b.x - p.x).abs() < 0.5) || ((a.y - b.y).abs() < 0.5 && (b.y - p.y).abs() < 0.5);
            if collinear {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// 简化版 perfect-freehand：根据压力 / 速度生成笔迹轮廓（闭合多边形），再平滑。
pub fn freehand_outline(pts: &[[f32; 3]], size: f32) -> Vec<PathCmd> {
    if pts.is_empty() {
        return vec![];
    }
    // 先做一次平滑，去掉触控板的抖动
    let mut p: Vec<[f32; 3]> = Vec::with_capacity(pts.len());
    for (i, q) in pts.iter().enumerate() {
        if i == 0 || i + 1 == pts.len() {
            p.push(*q);
        } else {
            let a = pts[i - 1];
            let b = pts[i + 1];
            p.push([(a[0] + 2. * q[0] + b[0]) / 4., (a[1] + 2. * q[1] + b[1]) / 4., q[2]]);
        }
    }
    if p.len() == 1 {
        let r = size / 2.;
        return crate::canvas::geom::ellipse_cmds(rect(p[0][0] - r, p[0][1] - r, 2. * r, 2. * r));
    }
    let mut left = vec![];
    let mut right = vec![];
    let n = p.len();
    for i in 0..n {
        let prev = p[if i == 0 { 0 } else { i - 1 }];
        let next = p[if i + 1 < n { i + 1 } else { n - 1 }];
        let (dx, dy) = (next[0] - prev[0], next[1] - prev[1]);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (nx, ny) = (-dy / len, dx / len);
        // 起笔 / 收笔变细
        let taper = ((i as f32 + 1.) / 4.).min(1.).min(((n - i) as f32) / 4.).max(0.35);
        let pressure = if p[i][2] > 0. { p[i][2] } else { 0.5 };
        let r = size / 2. * (0.55 + pressure * 0.6) * taper;
        left.push(pt(p[i][0] + nx * r, p[i][1] + ny * r));
        right.push(pt(p[i][0] - nx * r, p[i][1] - ny * r));
    }
    right.reverse();
    let mut ring = left;
    ring.extend(right);
    let mut cmds = crate::canvas::geom::smooth_cmds(&ring);
    cmds.push(PathCmd::Close);
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_two() -> (BoardDoc, u64, u64) {
        let mut b = BoardDoc::new(false);
        let a = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(0., 0., 100., 50.), false);
        let c = b.new_el(ElKind::Shape(ShapeKind::Ellipse), rect(300., 0., 100., 50.), false);
        let (ida, idc) = (a.id, c.id);
        b.els.push(a);
        b.els.push(c);
        (b, ida, idc)
    }

    #[test]
    fn connector_follows_shapes() {
        let (mut b, ida, idc) = doc_with_two();
        let l = b.new_el(
            ElKind::Connector {
                a: End::Bound { el: ida, ax: 0.5, ay: 0.5 },
                b: End::Bound { el: idc, ax: 0.5, ay: 0.5 },
                route: Route::Straight,
                start: Arrow::None,
                end: Arrow::Filled,
            },
            Rect::default(),
            false,
        );
        let pts = b.connector_points(&l);
        assert!((pts[0].x - 100.).abs() < 0.1 && (pts[1].x - 300.).abs() < 0.1);
        let json = serde_json::to_string(&b).unwrap();
        let back: BoardDoc = serde_json::from_str(&json).unwrap();
        assert_eq!(back.els.len(), 2);
        assert!(!freehand_outline(&[[0., 0., 0.5], [10., 5., 0.6], [20., 0., 0.5]], 4.).is_empty());
    }

    #[test]
    fn elbow_connector_is_orthogonal_and_leaves_from_facing_sides() {
        let (mut b, ida, idc) = doc_with_two();
        // 第二个形状挪到右下方
        b.els[1].y = 200.;
        let l = b.new_el(
            ElKind::Connector {
                a: End::Bound { el: ida, ax: 0.5, ay: 0.5 },
                b: End::Bound { el: idc, ax: 0.5, ay: 0.5 },
                route: Route::Elbow,
                start: Arrow::None,
                end: Arrow::Filled,
            },
            Rect::default(),
            false,
        );
        let pts = b.connector_points(&l);
        // 起点在 a 的右边中点
        assert_eq!(pts[0], pt(100., 25.));
        // 每一段都是水平或竖直
        for w in pts.windows(2) {
            assert!((w[0].x - w[1].x).abs() < 0.01 || (w[0].y - w[1].y).abs() < 0.01, "{pts:?}");
        }
    }

    #[test]
    fn manual_elbow_route_keeps_shape_until_one_end_moves() {
        let mut b = BoardDoc::new(true);
        let x = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(0., 0., 100., 40.), false);
        let y = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(0., 160., 100., 40.), false);
        let (ix, iy) = (x.id, y.id);
        let l = b.new_el(
            ElKind::Connector {
                a: End::Bound { el: ix, ax: 0.5, ay: 0.5 },
                b: End::Bound { el: iy, ax: 0.5, ay: 0.5 },
                route: Route::Elbow,
                start: Arrow::None,
                end: Arrow::Filled,
            },
            Rect::default(),
            false,
        );
        let id = l.id;
        b.els.extend([x, y, l]);
        let s = b.connector_skeleton(b.get(id).unwrap());
        assert_eq!(s, vec![pt(50., 40.), pt(50., 160.)]);
        // 中段往右挪 80：下伸 20 → 右 → 下 → 左 → 下 20 进入
        let detour = [pt(50., 40.), pt(50., 60.), pt(130., 60.), pt(130., 140.), pt(50., 140.), pt(50., 160.)];
        b.set_route(id, &detour);
        let got = b.connector_points(b.get(id).unwrap());
        assert_eq!(got, detour.to_vec());
        // 两端一起平移：走线跟着平移
        for e in b.els.iter_mut().filter(|e| !e.is_connector()) {
            e.x += 10.;
        }
        assert_eq!(b.connector_points(b.get(id).unwrap())[2], pt(140., 60.));
        // 只挪一端：回到自动走线，端点仍在下边 / 上边中点
        b.els[1].x += 200.;
        let got = b.connector_points(b.get(id).unwrap());
        assert_eq!((got[0], *got.last().unwrap()), (pt(60., 40.), pt(260., 160.)));
    }

    #[test]
    fn back_edge_goes_around_and_aligned_edge_is_straight() {
        let mut b = BoardDoc::new(true);
        let x = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(0., 0., 100., 40.), false);
        let y = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(8., 120., 100., 40.), false);
        let (ix, iy) = (x.id, y.id);
        b.els.extend([x, y]);
        let conn = |b: &mut BoardDoc, p: u64, q: u64| {
            b.new_el(
                ElKind::Connector {
                    a: End::Bound { el: p, ax: 0.5, ay: 0.5 },
                    b: End::Bound { el: q, ax: 0.5, ay: 0.5 },
                    route: Route::Elbow,
                    start: Arrow::None,
                    end: Arrow::Filled,
                },
                Rect::default(),
                false,
            )
        };
        let down = conn(&mut b, ix, iy);
        let up = conn(&mut b, iy, ix);
        b.els.extend([down.clone(), up.clone()]);
        // 正向：横向有重叠，直接一条竖线
        let d = b.connector_points(&down);
        assert_eq!(d.len(), 2, "{d:?}");
        assert!((d[0].x - d[1].x).abs() < 0.01);
        // 反向：从右侧绕回，和正向不重叠
        let u = b.connector_points(&up);
        assert!(u.iter().all(|p| p.x >= 100.), "{u:?}");
    }

    #[test]
    fn old_board_json_still_loads() {
        let json = r#"{"els":[{"id":1,"kind":{"Shape":"Rect"},"x":0,"y":0,"w":10,"h":10,"stroke":4278190335}],"next_id":2}"#;
        let b: BoardDoc = serde_json::from_str(json).unwrap();
        assert!(b.els[0].show_author && !b.els[0].dotted && b.els[0].text_align == 0);
    }

    #[test]
    fn section_children_are_fully_inside() {
        let mut b = BoardDoc::new(false);
        let s = b.new_el(ElKind::Section, rect(0., 0., 400., 300.), false);
        let inner = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(20., 20., 100., 50.), false);
        let outer = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(350., 20., 100., 50.), false);
        let (sid, iid) = (s.id, inner.id);
        b.els.extend([s, inner, outer]);
        assert_eq!(b.section_children(sid), vec![iid]);
    }

    #[test]
    fn scaled_scales_geometry_and_text() {
        let mut b = BoardDoc::new(false);
        let mut e = b.new_el(ElKind::Shape(ShapeKind::Rect), rect(100., 50., 80., 40.), false);
        e.font_size = 16.;
        let s = e.scaled(0., 0., 10., 20., 0.5, 0.5);
        assert_eq!((s.x, s.y, s.w, s.h, s.font_size), (60., 45., 40., 20., 8.));
    }
}
