//! 「更多图形」图形库（画板：工具栏「图形」面板底部的「更多图形 M」打开左侧停靠的面板）。
//!
//! 分类：基础 / 直线 / 泳道 / 流程图 / 类图 / 时序图 / 数据流图 / 实体关系图 / 组件图 / 状态图。
//! 单击选择工具后在画布上单击或拖出；也可以直接把图形拖到画布上。

use super::model::*;
use super::ui::panel;
use super::view::*;
use crate::canvas::paint::{paint_scene, View};
use crate::canvas::{pt, rect, Arrow, Col, Item, LineItem, Pt, Rect, Scene, ShapeItem, ShapeKind};
use crate::theme::Theme;
use crate::ui::{icon, Tooltip};
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;

/// 图形库里的一项。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibItem {
    Shape(ShapeKind),
    /// 线条：路线、起点箭头、终点箭头、虚线
    Line(Route, Arrow, Arrow, bool),
    /// 泳道：0 垂直、1 水平、2 矩阵
    Lane(u8),
    /// 图标库里的线性图标
    Icon(&'static str),
    /// 表情（插入为文字）
    Emoji(&'static str),
    /// 线框控件（见 wire.rs）
    Wire(u8),
}

/// 拖到画布上的图形库项。
#[derive(Clone, Copy, Debug)]
pub struct LibDrag(pub LibItem);

use LibItem::{Emoji, Icon, Lane, Line, Shape as S, Wire};
use ShapeKind as K;

const BASIC: &[(LibItem, &str)] = &[
    (S(K::RoundRect), "圆角矩形 R"),
    (S(K::Stadium), "全圆角矩形"),
    (S(K::Diamond), "菱形 D"),
    (S(K::Rect), "矩形 S"),
    (S(K::Circle), "圆形 O"),
    (S(K::Cylinder), "圆柱体"),
    (S(K::Chevron), "燕尾形"),
    (S(K::HomePlate), "五边形箭头"),
    (S(K::Parallelogram), "平行四边形"),
    (S(K::Trapezoid), "梯形"),
    (S(K::BubbleRound), "椭圆气泡"),
    (S(K::Bubble), "矩形气泡"),
    (S(K::RightTriangle), "直角三角形"),
    (S(K::Triangle), "三角形"),
    (S(K::Star), "五角星"),
    (S(K::Hexagon), "六边形"),
    (S(K::Pentagon), "五边形"),
    (S(K::Octagon), "八边形"),
    (S(K::LeftArrow), "左箭头"),
    (S(K::BlockArrow), "右箭头"),
    (S(K::DoubleArrow), "双向箭头"),
    (S(K::Cloud), "云朵"),
    (S(K::BraceLeft), "左大括号"),
    (S(K::BraceRight), "右大括号"),
    (S(K::Cross), "十字形"),
    (S(K::BlockArc), "弧形"),
    (S(K::Pie), "扇形"),
    (S(K::Cube), "立方体"),
    (S(K::Ellipse), "椭圆"),
    (S(K::Note), "注释"),
];

const LINES: &[(LibItem, &str)] = &[
    (Line(Route::Straight, Arrow::None, Arrow::None, false), "直线"),
    (Line(Route::Straight, Arrow::None, Arrow::None, true), "虚线"),
    (Line(Route::Straight, Arrow::None, Arrow::Filled, false), "箭头"),
    (Line(Route::Straight, Arrow::Filled, Arrow::Filled, false), "双向箭头"),
];

const LANES: &[(LibItem, &str)] = &[(Lane(0), "垂直泳道"), (Lane(1), "水平泳道"), (Lane(2), "矩阵泳道（泳道 × 阶段）")];

const FLOW: &[(LibItem, &str)] = &[
    (S(K::Rect), "流程"),
    (S(K::Stadium), "开始 / 结束"),
    (S(K::Diamond), "判定"),
    (S(K::Document), "文档"),
    (S(K::Parallelogram), "数据"),
    (S(K::Cylinder), "数据库"),
    (S(K::HCylinder), "直接数据"),
    (S(K::Subroutine), "子流程"),
    (S(K::ManualInput), "手动输入"),
    (S(K::TrapezoidAlt), "手动操作"),
    (S(K::Delay), "延迟"),
    (S(K::Hexagon), "准备"),
    (S(K::OffPage), "跨页引用"),
    (S(K::Tape), "顺序数据"),
    (S(K::Card), "卡片"),
    (S(K::PunchedTape), "纸带"),
    (S(K::Display), "显示"),
    (S(K::InternalStorage), "内部存储"),
    (S(K::LoopLimit), "循环界限"),
    (S(K::StoredData), "存储数据"),
    (S(K::MultiDoc), "多文档"),
    (S(K::OrJunction), "或"),
    (S(K::SumJunction), "汇总连接"),
    (S(K::Collate), "对照"),
    (S(K::Sort), "排序"),
    (S(K::Merge), "合并"),
    (S(K::Circle), "连接点"),
    (S(K::Note), "注释"),
];

const CLASS: &[(LibItem, &str)] = &[
    (S(K::ClassBox), "类"),
    (S(K::InterfaceBox), "接口"),
    (S(K::RoundRect), "对象"),
    (S(K::Subroutine), "枚举"),
    (S(K::Note), "注释"),
    (Line(Route::Straight, Arrow::None, Arrow::DiamondHollow, false), "聚合"),
    (Line(Route::Straight, Arrow::None, Arrow::DiamondFilled, false), "组合"),
    (Line(Route::Straight, Arrow::None, Arrow::Open, false), "关联"),
    (Line(Route::Straight, Arrow::None, Arrow::Open, true), "依赖"),
    (Line(Route::Straight, Arrow::None, Arrow::Hollow, true), "实现"),
    (Line(Route::Straight, Arrow::None, Arrow::Hollow, false), "泛化"),
];

const SEQUENCE: &[(LibItem, &str)] = &[
    (S(K::LifeActor), "参与者"),
    (S(K::LifeBox), "对象"),
    (S(K::LifeDb), "数据库"),
    (S(K::LifeQueue), "队列"),
    (S(K::LifeBoundary), "边界"),
    (S(K::LifeControl), "控制"),
    (S(K::LifeEntity), "实体"),
    (S(K::LifeCollections), "集合"),
    (S(K::Activation), "激活条"),
    (S(K::Frame), "组合片段"),
    (S(K::FrameAlt), "条件片段（alt）"),
    (Line(Route::Straight, Arrow::None, Arrow::Filled, false), "同步消息"),
    (Line(Route::Straight, Arrow::None, Arrow::Open, true), "返回消息"),
];

const DFD: &[(LibItem, &str)] = &[
    (S(K::Rect), "外部实体"),
    (S(K::DfdProcess), "加工"),
    (S(K::Circle), "加工（圆）"),
    (S(K::DataStore), "数据存储"),
    (S(K::DataStoreId), "数据存储（编号）"),
    (S(K::OpenRect), "数据存储（开口）"),
];

const ER: &[(LibItem, &str)] = &[
    (S(K::Entity), "实体"),
    (S(K::Entity2Key), "实体（主键）"),
    (S(K::Entity2), "实体（两列）"),
    (S(K::Entity3), "实体（三列）"),
    (Line(Route::Elbow, Arrow::One, Arrow::ZeroOrMany, false), "一对多"),
    (Line(Route::Elbow, Arrow::One, Arrow::One, false), "一对一"),
    (Line(Route::Elbow, Arrow::ZeroOrMany, Arrow::ZeroOrMany, false), "多对多"),
];

const COMPONENT: &[(LibItem, &str)] = &[
    (S(K::Component), "组件"),
    (S(K::Component2), "组件（图标）"),
    (S(K::ProvidedIface), "提供接口"),
    (S(K::AssemblyIface), "组装连接"),
    (S(K::RequiredIface), "需求接口"),
];

const WIRE: &[(LibItem, &str)] = &[
    (Wire(0), "按钮"),
    (Wire(1), "输入框"),
    (Wire(2), "下拉框"),
    (Wire(3), "复选框"),
    (Wire(4), "单选框"),
    (Wire(5), "开关"),
    (Wire(6), "搜索框"),
    (Wire(7), "Tab 栏"),
    (Wire(8), "导航栏"),
    (Wire(9), "列表"),
    (Wire(10), "卡片"),
    (Wire(11), "图片占位"),
    (Wire(12), "头像"),
    (Wire(13), "文本段落"),
    (Wire(14), "手机外框"),
    (Wire(15), "浏览器外框"),
];

const STATE: &[(LibItem, &str)] = &[(S(K::StartDot), "初始状态"), (S(K::EndDot), "终止状态"), (S(K::Bar), "分叉 / 汇合"), (S(K::RoundRect), "状态")];

pub const CATEGORIES: &[(&str, &[(LibItem, &str)])] = &[
    ("基础", BASIC),
    ("直线", LINES),
    ("泳道", LANES),
    ("流程图", FLOW),
    ("类图", CLASS),
    ("时序图", SEQUENCE),
    ("数据流图", DFD),
    ("实体关系图", ER),
    ("组件图", COMPONENT),
    ("状态图", STATE),
    ("线框", WIRE),
];

/// 工具栏「图形」面板里的基础图形（5 × 5）。
pub fn basic_shapes() -> impl Iterator<Item = (ShapeKind, &'static str)> {
    BASIC.iter().take(25).filter_map(|(i, n)| match i {
        S(k) => Some((*k, *n)),
        _ => None,
    })
}

/// 单击画布时新图形的默认尺寸。
pub fn default_size(k: ShapeKind) -> (f32, f32) {
    match k {
        K::Circle | K::Star | K::Pentagon | K::Octagon | K::BlockArc | K::Pie => (100., 100.),
        K::Ellipse => (130., 84.),
        K::Diamond | K::Sort => (140., 100.),
        K::Triangle | K::RightTriangle => (110., 100.),
        K::Merge | K::Collate => (90., 80.),
        K::Cloud => (160., 104.),
        K::BubbleRound | K::Bubble => (150., 100.),
        K::BraceLeft | K::BraceRight => (150., 120.),
        K::Cube => (120., 110.),
        K::LifeActor | K::LifeBox | K::LifeDb | K::LifeQueue | K::LifeBoundary | K::LifeControl | K::LifeEntity | K::LifeCollections => (110., 320.),
        K::Activation => (16., 120.),
        K::Frame | K::FrameAlt => (320., 200.),
        K::ClassBox => (170., 150.),
        K::InterfaceBox => (170., 110.),
        K::Entity | K::Entity2Key | K::Entity2 | K::Entity3 => (190., 140.),
        K::StartDot => (32., 32.),
        K::EndDot => (36., 36.),
        K::Bar => (160., 8.),
        K::OrJunction | K::SumJunction | K::Tape => (64., 64.),
        K::ProvidedIface | K::RequiredIface => (80., 30.),
        K::AssemblyIface => (110., 40.),
        K::Component | K::Component2 => (170., 90.),
        K::DataStore | K::DataStoreId | K::OpenRect => (160., 56.),
        _ => (140., 72.),
    }
}

/// 图标预览的尺寸（瘦高的生命线、扁平的分叉条等单独处理）。
fn preview_size(k: ShapeKind) -> (f32, f32) {
    match k {
        K::LifeActor | K::LifeBox | K::LifeDb | K::LifeQueue | K::LifeBoundary | K::LifeControl | K::LifeEntity | K::LifeCollections => (22., 30.),
        K::Activation => (8., 28.),
        K::Bar => (28., 6.),
        K::StartDot | K::EndDot | K::Circle | K::OrJunction | K::SumJunction | K::Tape | K::Star | K::Pentagon | K::Octagon | K::BlockArc | K::Pie => {
            (24., 24.)
        }
        _ => (28., 22.),
    }
}

/// 图形库项的小图标。
pub fn item_preview(item: LibItem, stroke: Hsla) -> AnyElement {
    let col = Col::from_hsla(stroke);
    let (w, h, scene) = match item {
        Icon(name) if super::iconlib::is_color(name) => {
            return match super::iconlib::raster(name, 64) {
                Some(r) => img(ImageSource::Render(r)).size(px(26.)).flex_none().into_any_element(),
                None => div().size(px(26.)).into_any_element(),
            }
        }
        Icon(name) => return svg().path(format!("icons/lib/{name}.svg")).size(px(22.)).flex_none().text_color(stroke).into_any_element(),
        Emoji(e) => return div().text_size(px(22.)).line_height(px(26.)).child(e).into_any_element(),
        Wire(kind) => {
            // 生成一份缩小的控件当预览
            let mut d = BoardDoc::new(false);
            d.els = super::wire::build(&mut d, kind, pt(0., 0.), false);
            let mut scene = d.to_scene(false);
            scene.fit(0.);
            let s = (28. / scene.width.max(1.)).min(24. / scene.height.max(1.));
            let (w, h) = (scene.width * s, scene.height * s);
            let scene = Arc::new(scene);
            return div()
                .w(px(w.max(4.)))
                .h(px(h.max(4.)))
                .flex_none()
                .child(canvas(|b, _, _| b, move |b, _, win, cx| paint_scene(&scene, View { origin: b.origin, scale: s }, b, win, cx)).size_full())
                .into_any_element();
        }
        S(k) => {
            let (w, h) = preview_size(k);
            let mut s = ShapeItem::new(k, rect(1.5, 1.5, w - 3., h - 3.));
            s.fill = None;
            s.stroke = Some(col);
            s.stroke_w = 1.4;
            s.radius = (h / 5.).min(4.);
            (w, h, Scene { items: vec![Item::Shape(s)], width: w, height: h, ..Default::default() })
        }
        Line(_, start, end, dash) => {
            let mut l = LineItem::new(vec![pt(4., 24.), pt(26., 4.)]);
            l.stroke = col;
            l.width = 1.5;
            l.start = start;
            l.end = end;
            l.dash = dash;
            l.label_bg = Some(Col::rgb(0xffffff).with_alpha(0.));
            (30., 28., Scene { items: vec![Item::Line(l)], width: 30., height: 28., ..Default::default() })
        }
        Lane(kind) => {
            // 小网格示意
            let (cols, rows): (Vec<f32>, Vec<f32>) = match kind {
                1 => (vec![0., 6., 26.], vec![0., 8., 16., 24.]),
                2 => (vec![0., 6., 16., 26.], vec![0., 6., 15., 24.]),
                _ => (vec![0., 9., 17., 26.], vec![0., 6., 24.]),
            };
            let mut items = vec![];
            let mut seg = |a: (f32, f32), b: (f32, f32)| {
                let mut l = LineItem::new(vec![pt(a.0 + 2., a.1 + 2.), pt(b.0 + 2., b.1 + 2.)]);
                l.stroke = col;
                l.width = 1.3;
                l.end = Arrow::None;
                l.label_bg = None;
                items.push(Item::Line(l));
            };
            let (x1, y1) = (*cols.last().unwrap(), *rows.last().unwrap());
            for x in &cols {
                seg((*x, 0.), (*x, y1));
            }
            for y in &rows {
                seg((0., *y), (x1, *y));
            }
            (30., 28., Scene { items, width: 30., height: 28., ..Default::default() })
        }
    };
    let scene = Arc::new(scene);
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .child(canvas(|b, _, _| b, move |b, _, win, cx| paint_scene(&scene, View { origin: b.origin, scale: 1. }, b, win, cx)).size_full())
        .into_any_element()
}

/// 拖动时跟着鼠标的预览。
struct LibGhost(LibItem);

impl Render for LibGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::theme::theme(cx);
        div()
            .size(px(44.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .bg(t.panel_bg.opacity(0.9))
            .border_1()
            .border_color(t.accent)
            .child(item_preview(self.0, t.accent))
    }
}

impl BoardEditor {
    /// 选中图形库里的一项：设置对应的工具。
    pub(super) fn pick_library(&mut self, item: LibItem, cx: &mut Context<Self>) {
        match item {
            S(k) => self.set_tool(Tool::Shape(k), cx),
            Line(route, start, end, dash) => {
                self.set_tool(Tool::Connector(route, end != Arrow::None), cx);
                self.conn_preset = Some((start, end, dash));
            }
            Lane(k) => self.set_tool(Tool::Lane(k), cx),
            // 图标 / 表情 / 线框控件：单击直接插到视口中心
            Icon(_) | Emoji(_) | Wire(_) => {
                let at = self.to_world(self.canvas_bounds.center());
                self.insert_icon(item, at, cx);
            }
        }
    }

    /// 在 at（世界坐标，中心点）插入图标或表情并选中。
    pub(super) fn insert_icon(&mut self, item: LibItem, at: Pt, cx: &mut Context<Self>) {
        let dark = Self::dark(cx);
        self.snapshot();
        let el = match item {
            Icon(name) => {
                super::iconlib::push_recent(name);
                let (w, h) = super::iconlib::insert_size(name);
                let mut el = self.doc.new_el(ElKind::Icon { name: name.to_string() }, rect(at.x - w / 2., at.y - h / 2., w, h), dark);
                el.stroke = if dark { Col::rgb(0xe3e4e6) } else { Col::rgb(0x1f2329) };
                el.fill = None;
                el
            }
            Emoji(e) => {
                super::iconlib::push_recent(&format!("emoji:{e}"));
                let mut el = self.doc.new_el(ElKind::Text, rect(at.x - 32., at.y - 32., 64., 64.), dark);
                el.text = e.to_string();
                el.font_size = 40.;
                el.fill = None;
                el.text_align = 2;
                el
            }
            Wire(kind) => {
                let (w, h) = super::wire::size(kind);
                let els = super::wire::build(&mut self.doc, kind, pt(at.x - w / 2., at.y - h / 2.), dark);
                self.sel = els.iter().map(|e| e.id).collect();
                self.doc.els.extend(els);
                self.tool = Tool::Select;
                self.commit(cx);
                return;
            }
            _ => return,
        };
        let id = el.id;
        self.doc.els.push(el);
        self.sel = [id].into();
        self.tool = Tool::Select;
        self.commit(cx);
    }

    fn lib_is_on(&self, item: LibItem) -> bool {
        match item {
            S(k) => self.tool == Tool::Shape(k),
            Line(route, start, end, dash) => matches!(self.tool, Tool::Connector(r, _) if r == route) && self.conn_preset == Some((start, end, dash)),
            Lane(k) => self.tool == Tool::Lane(k),
            Icon(_) | Emoji(_) | Wire(_) => false,
        }
    }

    /// 从图形库拖到画布上：在松开处创建。
    pub(super) fn drop_library_item(&mut self, d: LibDrag, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.to_world(window.mouse_position());
        let dark = Self::dark(cx);
        self.snapshot();
        let id = match d.0 {
            S(k) => {
                let (w, h) = default_size(k);
                let mut el = self.doc.new_el(ElKind::Shape(k), rect(p.x - w / 2., p.y - h / 2., w, h), dark);
                self.style_new(&mut el);
                let id = el.id;
                self.doc.els.push(el);
                id
            }
            Line(route, start, end, dash) => {
                let mut el = self.doc.new_el(
                    ElKind::Connector { a: End::Free { x: p.x - 80., y: p.y }, b: End::Free { x: p.x + 80., y: p.y }, route, start, end },
                    Rect::default(),
                    dark,
                );
                el.dash = dash;
                let id = el.id;
                self.doc.els.push(el);
                id
            }
            Lane(k) => {
                let el = self.doc.new_lane(k, p, dark);
                let id = el.id;
                self.doc.els.insert(0, el);
                id
            }
            Icon(_) | Emoji(_) | Wire(_) => {
                self.undo.pop_back();
                self.insert_icon(d.0, p, cx);
                return;
            }
        };
        self.sel = [id].into();
        self.tool = Tool::Select;
        self.commit(cx);
    }

    /// 图形库的一个格子（工具栏面板与左侧图形库共用）。
    pub(super) fn lib_cell(&self, key: (&'static str, usize), item: LibItem, tip: &'static str, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let on = self.lib_is_on(item);
        div()
            .id(key)
            .size(px(40.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .when(on, |d| d.bg(t.accent_soft))
            .hover(|s| s.bg(t.hover))
            .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
            .child(item_preview(item, if on { t.accent } else { t.text }))
            .on_click(cx.listener(move |this, _, _, cx| this.pick_library(item, cx)))
            .on_drag(LibDrag(item), |d, _, _, cx| cx.new(|_| LibGhost(d.0)))
    }

    /// 左侧停靠的「更多图形」面板。
    pub(super) fn render_library(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.library {
            return None;
        }
        let mut list = div().flex().flex_col().pb(px(12.));
        for (ci, (name, items)) in CATEGORIES.iter().enumerate() {
            let folded = self.lib_folded.contains(name);
            let name: &'static str = name;
            list = list.child(
                div()
                    .id(("lib-cat", ci))
                    .h(px(30.))
                    .mt(px(4.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(t.text_secondary)
                    .hover(|s| s.bg(t.hover))
                    .child(icon(if folded { "chevron-right" } else { "chevron-down" }, 13., t.text_muted))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.lib_folded.remove(name) {
                            this.lib_folded.insert(name);
                        }
                        cx.notify();
                    })),
            );
            if !folded {
                list = list.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(2.))
                        .px(px(2.))
                        .children(items.iter().enumerate().map(|(i, (item, tip))| self.lib_cell(("lib-item", ci * 100 + i), *item, tip, t, cx))),
                );
            }
        }
        Some(
            div()
                .absolute()
                .left(px(16.))
                .top(px(64.))
                .bottom(px(16.))
                .w(px(5. * 42. + 26.))
                .child(
                    panel(t)
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .h(px(42.))
                                .px(px(12.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_b_1()
                                .border_color(t.border)
                                .child(div().text_size(px(14.)).child("更多图形"))
                                .child(
                                    div()
                                        .id("lib-close")
                                        .size(px(26.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(t.hover))
                                        .tooltip(|_, cx| cx.new(|_| Tooltip("收起 M".into())).into())
                                        .child(icon("x", 15., t.text_muted))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.library = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(div().id("lib-scroll").flex_1().overflow_y_scroll().px(px(8.)).child(list)),
                )
                .into_any_element(),
        )
    }

    /// 打开 / 收起左侧的「素材」面板（画板工具栏的「素材」）。
    pub(super) fn toggle_icons(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flyout = None;
        if self.icons.take().is_some() {
            window.focus(&self.focus);
            cx.notify();
            return;
        }
        self.library = false;
        self.icon_pack = None;
        let q = cx.new(|cx| crate::line_input::LineInput::new("搜索素材，如：用户、云、数据库", cx));
        let sub = cx.subscribe_in(&q, window, |this, _, ev: &crate::line_input::InputEvent, window, cx| {
            if matches!(ev, crate::line_input::InputEvent::Cancel) {
                // 在素材集里按 Esc 先回到素材首页，再按才收起
                if this.icon_pack.take().is_none() {
                    this.icons = None;
                    window.focus(&this.focus);
                }
            }
            this.icon_scroll.scroll_to_item(0, ScrollStrategy::Top);
            cx.notify();
        });
        self.icons = Some((q, sub));
        cx.notify();
    }

    /// 进入某个素材集（None 回到素材首页），清空搜索词。
    pub(super) fn open_icon_pack(&mut self, pack: Option<&'static str>, cx: &mut Context<Self>) {
        self.icon_pack = pack;
        if let Some((q, _)) = self.icons.as_ref() {
            q.clone().update(cx, |q, cx| q.set_text("", cx));
        }
        self.icon_scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// 素材格子：单击插入到视口中心，也可以拖到画布上。
    fn icon_cell(&self, key: impl Into<ElementId>, item: LibItem, tip: &'static str, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id(key.into())
            .size(px(ICON_CELL))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .tooltip(move |_, cx| cx.new(|_| Tooltip(tip.into())).into())
            .child(item_preview(item, t.text))
            .on_click(cx.listener(move |this, _, _, cx| this.pick_library(item, cx)))
            .on_drag(LibDrag(item), |d, _, _, cx| cx.new(|_| LibGhost(d.0)))
    }

    /// 搜索结果 / 素材集详情的行：分组标题行和图标行一样高，好用 uniform_list 只渲染可见部分。
    fn icon_rows(&self, words: &[&str]) -> Vec<IconRow> {
        let mut rows = vec![];
        let push = |rows: &mut Vec<IconRow>, title: SharedString, open: Option<&'static str>, items: Vec<(LibItem, &'static str)>| {
            if items.is_empty() {
                return;
            }
            rows.push(IconRow::Head(title, items.len(), open));
            rows.extend(items.chunks(ICON_COLS).map(|c| IconRow::Cells(c.to_vec())));
        };
        let emoji = || super::icons::EMOJIS.iter().map(|e| (Emoji(e), *e)).collect::<Vec<_>>();
        match self.icon_pack {
            Some(EMOJI_PACK) => push(&mut rows, "系统表情".into(), None, if words.is_empty() { emoji() } else { vec![] }),
            Some(id) => {
                for c in super::iconlib::pack(id).map(|p| p.cats.as_slice()).unwrap_or_default() {
                    let items = c.icons.iter().filter(|d| d.matches(words)).map(|d| (Icon(d.id.as_str()), d.title.as_str())).collect();
                    push(&mut rows, c.name.as_str().into(), None, items);
                }
            }
            // 首页搜索：按素材集分组，标题可点进素材集
            None => {
                for p in super::iconlib::packs() {
                    let items = p.icons().filter(|d| d.matches(words)).map(|d| (Icon(d.id.as_str()), d.title.as_str())).collect();
                    push(&mut rows, p.title.as_str().into(), Some(p.id.as_str()), items);
                }
            }
        }
        rows
    }

    fn render_icon_row(&self, i: usize, row: &IconRow, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        match row {
            IconRow::Head(title, n, open) => {
                let head = div()
                    .w_full()
                    .h(px(ICON_ROW_H))
                    .flex()
                    .items_end()
                    .justify_between()
                    .pb(px(6.))
                    .px(px(2.))
                    .child(div().text_size(px(13.)).text_color(t.text).child(title.clone()));
                match *open {
                    Some(id) => head
                        .child(
                            div()
                                .id(("icon-more", i))
                                .flex()
                                .items_center()
                                .gap(px(2.))
                                .cursor_pointer()
                                .text_size(px(12.))
                                .text_color(t.text_muted)
                                .hover(|s| s.text_color(t.accent))
                                .child(format!("查看全部 {n}"))
                                .child(icon("chevron-right", 12., t.text_muted))
                                .on_click(cx.listener(move |this, _, _, cx| this.open_icon_pack(Some(id), cx))),
                        )
                        .into_any_element(),
                    None => head.child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("{n} 个"))).into_any_element(),
                }
            }
            IconRow::Cells(items) => div()
                .h(px(ICON_ROW_H))
                .flex()
                .items_center()
                .gap(px(ICON_GAP))
                .children(items.iter().enumerate().map(|(j, (item, tip))| self.icon_cell(("icon-row", i * ICON_COLS + j), *item, tip, t, cx)))
                .into_any_element(),
        }
    }

    /// 素材首页：最近使用 + 各素材集的卡片（卡片标题可点进素材集看全部）。
    fn render_icon_home(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mut list = div().flex().flex_col().pb(px(12.));
        let recent: Vec<(LibItem, &'static str)> = super::iconlib::recent()
            .iter()
            .filter_map(|k| match k.strip_prefix("emoji:") {
                Some(e) => super::icons::EMOJIS.iter().find(|x| **x == e).map(|e| (Emoji(e), *e)),
                None => super::iconlib::get(k).map(|(_, d)| (Icon(d.id.as_str()), d.title.as_str())),
            })
            .collect();
        if !recent.is_empty() {
            list = list.child(div().pt(px(12.)).pb(px(4.)).px(px(2.)).text_size(px(13.)).text_color(t.text).child("最近使用")).child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(ICON_GAP))
                    .children(recent.into_iter().enumerate().map(|(i, (item, tip))| self.icon_cell(("icon-recent", i), item, tip, t, cx))),
            );
        }
        let packs = super::iconlib::packs().iter().map(|p| {
            let items: Vec<_> = p.preview().take(ICON_COLS * 2).map(|d| (Icon(d.id.as_str()), d.title.as_str())).collect();
            (p.id.as_str(), p.title.as_str(), p.len(), items)
        });
        let emoji = (EMOJI_PACK, "系统表情", super::icons::EMOJIS.len(), super::icons::EMOJIS.iter().take(ICON_COLS * 2).map(|e| (Emoji(e), *e)).collect());
        for (ci, (id, title, n, items)) in packs.chain([emoji]).enumerate() {
            list = list.child(
                div()
                    .id(("icon-pack", ci))
                    .group("icon-pack-head")
                    .mt(px(12.))
                    .mb(px(6.))
                    .px(px(2.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .child(div().text_size(px(13.)).text_color(t.text).group_hover("icon-pack-head", |s| s.text_color(t.accent)).child(title))
                    .child(div().flex().items_center().gap(px(2.)).text_size(px(12.)).text_color(t.text_muted).child(format!("{n} 个素材")).child(icon(
                        "chevron-right",
                        12.,
                        t.text_muted,
                    )))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_icon_pack(Some(id), cx))),
            );
            list = list.child(
                div()
                    .p(px(3.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.border)
                    .flex()
                    .flex_wrap()
                    .children(items.into_iter().enumerate().map(|(i, (item, tip))| self.icon_cell(("icon-card", ci * 100 + i), item, tip, t, cx))),
            );
        }
        list
    }

    /// 左侧停靠的「素材」面板：首页（最近使用 + 素材集卡片）、搜索结果、素材集详情。
    pub(super) fn render_icons(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (q, _) = self.icons.as_ref()?;
        let query = q.read(cx).text().trim().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        let pack = self.icon_pack.and_then(super::iconlib::pack);
        let close = div()
            .id("icons-close")
            .size(px(26.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .tooltip(|_, cx| cx.new(|_| Tooltip("收起".into())).into())
            .child(icon("x", 15., t.text_muted))
            .on_click(cx.listener(|this, _, window, cx| this.toggle_icons(window, cx)));
        let search = div()
            .flex_1()
            .h(px(32.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(t.border_strong)
            .text_size(px(13.))
            .child(icon("search", 14., t.text_muted))
            .child(div().flex_1().child(q.clone()));
        let mut top = div().flex().flex_col().border_b_1().border_color(t.border);
        let detail = self.icon_pack.is_some();
        if detail {
            let (title, n): (SharedString, usize) = match pack {
                Some(p) => (p.title.clone().into(), p.len()),
                None => ("系统表情".into(), super::icons::EMOJIS.len()),
            };
            top = top.child(
                div()
                    .h(px(40.))
                    .pl(px(4.))
                    .pr(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("icons-back")
                            .h(px(28.))
                            .pl(px(4.))
                            .pr(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .tooltip(|_, cx| cx.new(|_| Tooltip("返回素材首页 Esc".into())).into())
                            .child(icon("chevron-left", 16., t.text))
                            .child(div().text_size(px(14.)).text_color(t.text).child(title))
                            .on_click(cx.listener(|this, _, _, cx| this.open_icon_pack(None, cx))),
                    )
                    .child(div().flex_1().text_size(px(12.)).text_color(t.text_muted).child(format!("{n} 个素材")))
                    .child(close),
            );
            top = top.child(div().px(px(10.)).pb(px(10.)).flex().child(search));
        } else {
            top = top.child(div().p(px(10.)).flex().items_center().gap(px(8.)).child(search).child(close));
        }
        let body = if !detail && words.is_empty() {
            div().id("icons-scroll").flex_1().overflow_y_scroll().px(px(10.)).child(self.render_icon_home(t, cx)).into_any_element()
        } else {
            let rows = std::rc::Rc::new(self.icon_rows(&words));
            if rows.is_empty() {
                div().flex_1().pt(px(40.)).flex().justify_center().text_size(px(13.)).text_color(t.text_muted).child("没有找到相关素材").into_any_element()
            } else {
                uniform_list(
                    "icons-list",
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let t = crate::theme::theme(cx).clone();
                        range.map(|i| this.render_icon_row(i, &rows[i], &t, cx)).collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.icon_scroll.clone())
                .flex_1()
                .px(px(10.))
                .pb(px(8.))
                .into_any_element()
            }
        };
        // 素材集详情底部注明来源与许可
        let footer = pack.filter(|p| !p.license.is_empty()).map(|p| {
            let url = p.url.as_str();
            div()
                .id("icons-license")
                .px(px(12.))
                .py(px(6.))
                .border_t_1()
                .border_color(t.border)
                .text_size(px(11.))
                .text_color(t.text_muted)
                .child(format!("来源与许可：{}", p.license))
                .when(!url.is_empty(), |d| {
                    d.cursor_pointer()
                        .hover(|s| s.text_color(t.accent))
                        .tooltip(move |_, cx| cx.new(|_| Tooltip(url.into())).into())
                        .on_click(move |_, _, cx| cx.open_url(url))
                })
        });
        Some(
            div()
                .absolute()
                .left(px(16.))
                .top(px(64.))
                .bottom(px(16.))
                .w(px(ICON_PANEL_W))
                .child(panel(t).size_full().flex().flex_col().child(top).child(body).children(footer))
                .into_any_element(),
        )
    }
}

/// 素材面板里的一行（搜索结果 / 素材集详情）
enum IconRow {
    /// 分组标题：(标题, 数量, 可点进的素材集)
    Head(SharedString, usize, Option<&'static str>),
    Cells(Vec<(LibItem, &'static str)>),
}

/// 「系统表情」素材集（插入为文字）
const EMOJI_PACK: &str = "emoji";
const ICON_COLS: usize = 8;
const ICON_CELL: f32 = 36.;
const ICON_GAP: f32 = 2.;
const ICON_ROW_H: f32 = 38.;

/// 素材面板宽度（每行 8 个）
pub(super) const ICON_PANEL_W: f32 = ICON_COLS as f32 * (ICON_CELL + ICON_GAP) + 22.;
