//! 内置模板（画板左侧工具栏的「模板」）：流程图、泳道图、看板、思维导图、组织架构、
//! SWOT、用户旅程、头脑风暴、时序图。点击模板卡片插入到视口中心，插入后全部可编辑。

use super::model::*;
use super::ui::panel;
use super::view::*;
use crate::canvas::paint::{paint_scene, View};
use crate::canvas::{pt, rect, Arrow, Col, Pt, Rect, ShapeKind};
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;

mod more;

pub const CATEGORIES: [&str; 12] = ["全部", "流程图", "泳道图", "思维导图", "组织架构", "时间线", "图示", "矩阵", "分析", "架构图", "UML", "看板"];

pub struct Template {
    pub name: &'static str,
    pub desc: &'static str,
    pub cat: usize,
    pub build: fn(bool) -> BoardDoc,
}

/// 顺序与分类按常见分类（精选在前）。
pub const TEMPLATES: &[Template] = &[
    Template { name: "时间线（新）", desc: "卡片交错排布的时间线", cat: 5, build: more::timeline_new },
    Template { name: "增长飞轮", desc: "环环相扣的增长循环", cat: 6, build: more::flywheel },
    Template { name: "雷达图", desc: "多维度能力对比", cat: 6, build: more::radar },
    Template { name: "金字塔", desc: "自下而上的层级关系", cat: 6, build: more::pyramid },
    Template { name: "商业模式画布", desc: "九宫格梳理商业模式", cat: 8, build: more::business_canvas },
    Template { name: "基础流程图", desc: "开始、步骤、判断与回退", cat: 1, build: flowchart },
    Template { name: "时间线", desc: "沿时间轴排列的事件", cat: 5, build: more::timeline },
    Template { name: "组织架构图", desc: "自上而下的树状结构", cat: 4, build: org_chart },
    Template { name: "里程碑", desc: "按季度划分的目标与任务", cat: 5, build: more::milestone },
    Template { name: "水平时间线", desc: "首尾相接的阶段条", cat: 5, build: more::h_timeline },
    Template { name: "漏斗图", desc: "逐层转化的数据漏斗", cat: 6, build: more::funnel },
    Template { name: "分组列表", desc: "分组罗列要点", cat: 6, build: more::group_list },
    Template { name: "里程碑（新）", desc: "关键节点与日期", cat: 5, build: more::milestone_new },
    Template { name: "品牌屋", desc: "愿景、使命、价值与支柱", cat: 6, build: more::brand_house },
    Template { name: "垂直时间线", desc: "左右交错的纵向时间线", cat: 5, build: more::v_timeline },
    Template { name: "层次结构", desc: "分层排列的结构图", cat: 6, build: more::hierarchy },
    Template { name: "四象限气泡图", desc: "两个维度 + 气泡大小", cat: 7, build: more::bubble_quad },
    Template { name: "饼图", desc: "各部分占比", cat: 6, build: more::pie },
    Template { name: "圆形矩阵", desc: "围绕中心的四个要素", cat: 6, build: more::circle_matrix },
    Template { name: "网格矩阵图", desc: "坐标轴划分的四象限", cat: 7, build: more::grid_matrix },
    Template { name: "循环图", desc: "PDCA 等循环过程", cat: 6, build: more::cycle },
    Template { name: "二维坐标图", desc: "两条曲线与交点", cat: 6, build: more::axis2d },
    Template { name: "上升趋势图", desc: "逐步上升的阶段", cat: 6, build: more::trend_up },
    Template { name: "韦恩图", desc: "三个集合的交集", cat: 6, build: more::venn },
    Template { name: "金字塔列表", desc: "金字塔 + 每层说明", cat: 6, build: more::pyramid_list },
    Template { name: "蜂巢图", desc: "围绕核心的六个要素", cat: 6, build: more::honeycomb },
    Template { name: "曲线列表", desc: "沿弧线排列的步骤", cat: 6, build: more::curve_list },
    Template { name: "堆叠韦恩图", desc: "层层包含的范围", cat: 6, build: more::stacked_venn },
    Template { name: "路线图", desc: "逐级推进的路线", cat: 5, build: more::roadmap },
    Template { name: "目标图", desc: "由远及近的目标", cat: 6, build: more::target },
    Template { name: "环形图", desc: "带中心说明的占比图", cat: 6, build: more::donut },
    Template { name: "下降趋势图", desc: "逐步下降的阶段", cat: 6, build: more::trend_down },
    Template { name: "用户旅程地图", desc: "阶段 × 行为 / 触点 / 情绪 / 机会", cat: 8, build: journey },
    Template { name: "快速复盘", desc: "好的、待改进、新想法、疑问", cat: 8, build: more::quick_retro },
    Template { name: "鱼骨图", desc: "从多个方面分析问题原因", cat: 8, build: more::fishbone },
    Template { name: "SWOT 分析", desc: "优势、劣势、机会、威胁", cat: 7, build: swot },
    Template { name: "优先级矩阵", desc: "价值 × 投入排优先级", cat: 7, build: more::priority_matrix },
    Template { name: "人才盘点", desc: "绩效 × 潜力四象限", cat: 7, build: more::talent_review },
    Template { name: "艾森豪矩阵", desc: "重要 × 紧急时间管理", cat: 7, build: more::eisenhower },
    Template { name: "精益画布", desc: "创业想法的一页纸梳理", cat: 8, build: more::lean_canvas },
    Template { name: "同理心地图", desc: "想、听、看、说做", cat: 8, build: more::empathy_map },
    Template { name: "系统架构图", desc: "接入、业务、服务、数据分层", cat: 9, build: more::sys_arch },
    Template { name: "产品研发流程图", desc: "产品、研发、测试泳道流程", cat: 1, build: more::product_flow },
    Template { name: "垂直泳道图", desc: "按角色分工的审批流程", cat: 2, build: lane_approval },
    Template { name: "时序图", desc: "参与者、生命线与消息", cat: 10, build: sequence },
    Template { name: "水平泳道图", desc: "产品、研发、测试协作", cat: 2, build: lane_dev },
    Template { name: "UML 类图", desc: "类、接口与关系", cat: 10, build: more::class_diagram },
    Template { name: "数据流图", desc: "外部实体、加工与数据存储", cat: 1, build: more::data_flow },
    Template { name: "网络拓扑图", desc: "网络设备与连接关系", cat: 9, build: more::network },
    Template { name: "ER 图", desc: "实体、字段与基数关系", cat: 10, build: more::er_diagram },
    Template { name: "状态机图", desc: "订单状态流转", cat: 10, build: more::state_machine },
    Template { name: "用例图", desc: "参与者与系统用例", cat: 10, build: more::use_case },
    Template { name: "3D 网络拓扑图", desc: "立体设备图标的网络拓扑", cat: 9, build: more::network_3d },
    Template { name: "思维导图", desc: "左右分布的思维导图", cat: 3, build: mind_plan },
    Template { name: "移动端线框原型图", desc: "首页与详情页的手机线框图", cat: 6, build: more::mobile_wireframe },
    Template { name: "用户服务蓝图", desc: "前台、后台与支持过程", cat: 8, build: more::service_blueprint },
    Template { name: "项目看板", desc: "待办 / 进行中 / 已完成", cat: 11, build: kanban },
    Template { name: "头脑风暴", desc: "围绕主题发散想法", cat: 11, build: brainstorm },
];

/// 构建模板用的小工具。
struct B {
    d: BoardDoc,
    dark: bool,
}

impl B {
    fn new(dark: bool) -> Self {
        B { d: BoardDoc::new(false), dark }
    }
    fn shape(&mut self, k: ShapeKind, r: Rect, text: &str) -> u64 {
        let mut el = self.d.new_el(ElKind::Shape(k), r, self.dark);
        el.text = text.into();
        el.font_size = 14.;
        let id = el.id;
        self.d.els.push(el);
        id
    }
    fn colored(&mut self, k: ShapeKind, r: Rect, text: &str, stroke: u32, fill: u32) -> u64 {
        let id = self.shape(k, r, text);
        let e = self.d.els.last_mut().unwrap();
        e.stroke = Col::rgb(stroke);
        e.fill = Some(if self.dark { Col::rgb(stroke).with_alpha(0.22) } else { Col::rgb(fill) });
        id
    }
    fn conn(&mut self, a: u64, b: u64, label: &str) -> u64 {
        let mut el = self.d.new_el(
            ElKind::Connector {
                a: End::Bound { el: a, ax: 0.5, ay: 0.5 },
                b: End::Bound { el: b, ax: 0.5, ay: 0.5 },
                route: Route::Elbow,
                start: Arrow::None,
                end: Arrow::Filled,
            },
            Rect::default(),
            self.dark,
        );
        el.text = label.into();
        el.font_size = 14.;
        let id = el.id;
        self.d.els.push(el);
        id
    }
    /// 指定两端所在的边（回退线从上方 / 下方绕开，避免和正向连线重叠）。
    fn conn_sides(&mut self, a: u64, sa: (f32, f32), b: u64, sb: (f32, f32), label: &str) {
        let id = self.conn(a, b, label);
        if let Some(ElKind::Connector { a, b, .. }) = self.d.els.iter_mut().find(|e| e.id == id).map(|e| &mut e.kind) {
            if let End::Bound { ax, ay, .. } = a {
                (*ax, *ay) = sa;
            }
            if let End::Bound { ax, ay, .. } = b {
                (*ax, *ay) = sb;
            }
        }
    }
    fn msg(&mut self, x0: f32, x1: f32, y: f32, label: &str, back: bool) {
        let mut el = self.d.new_el(
            ElKind::Connector {
                a: End::Free { x: x0, y },
                b: End::Free { x: x1, y },
                route: Route::Straight,
                start: Arrow::None,
                end: if back { Arrow::Open } else { Arrow::Filled },
            },
            Rect::default(),
            self.dark,
        );
        el.text = label.into();
        el.dash = back;
        el.font_size = 14.;
        self.d.els.push(el);
    }
    fn sticky(&mut self, r: Rect, color: usize, text: &str) -> u64 {
        let mut el = self.d.new_el(ElKind::Sticky, r, self.dark);
        el.fill = Some(Col::rgb(STICKY_COLORS[color % STICKY_COLORS.len()]));
        el.text = text.into();
        el.font_size = 15.;
        el.show_author = false;
        let id = el.id;
        self.d.els.push(el);
        id
    }
    fn text(&mut self, at: Pt, text: &str, size: f32) {
        let w = crate::canvas::geom::estimate_width(text, size, true) + 24.;
        let mut el = self.d.new_el(ElKind::Text, rect(at.x, at.y, w, size * 1.38 + 12.), self.dark);
        el.text = text.into();
        el.font_size = size;
        el.bold = true;
        self.d.els.push(el);
    }
    fn section(&mut self, r: Rect, title: &str) -> u64 {
        let mut el = self.d.new_el(ElKind::Section, r, self.dark);
        el.text = title.into();
        let id = el.id;
        self.d.els.insert(0, el);
        id
    }
    /// 泳道里第 col 列（或第 row 行）的中心。
    fn lane(&mut self, kind: u8, title: &str, heads: &[&str]) -> El {
        let mut el = self.d.new_lane(kind, pt(0., 0.), self.dark);
        let mut k = 0;
        for i in 0..match &el.kind {
            ElKind::Table { cells, .. } => cells.len(),
            _ => 0,
        } {
            if el.is_head_cell(i) && !el.part_text(Part::Cell(i)).is_empty() {
                if let Some(h) = heads.get(k) {
                    el.set_part_text(Part::Cell(i), h.to_string());
                }
                k += 1;
            }
        }
        el.text = title.into();
        el.y -= LANE_TITLE_H;
        el.h += LANE_TITLE_H;
        self.d.els.insert(0, el.clone());
        el
    }
}

fn flowchart(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let start = b.colored(ShapeKind::Stadium, rect(0., 0., 140., 48.), "开始", 0x34c724, 0xd9f5d6);
    let s1 = b.shape(ShapeKind::RoundRect, rect(0., 108., 140., 56.), "填写申请");
    let d = b.colored(ShapeKind::Diamond, rect(-10., 224., 160., 96.), "信息是否完整？", 0xffc60a, 0xfff5cc);
    let s2 = b.shape(ShapeKind::RoundRect, rect(0., 380., 140., 56.), "提交审批");
    let end = b.colored(ShapeKind::Stadium, rect(0., 496., 140., 48.), "结束", 0xf54a45, 0xfde2e2);
    let fix = b.shape(ShapeKind::RoundRect, rect(240., 244., 140., 56.), "补充材料");
    b.conn(start, s1, "");
    b.conn(s1, d, "");
    b.conn(d, s2, "是");
    b.conn(s2, end, "");
    b.conn(d, fix, "否");
    b.conn_sides(fix, (0.5, 0.), s1, (1., 0.5), "");
    b.d
}

fn lane_approval(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let lane = b.lane(0, "报销审批流程", &["申请人", "部门经理", "财务"]);
    let g = lane.grid().unwrap();
    let cx = |c: usize| (g.xs[c] + g.xs[c + 1]) / 2.;
    let y0 = g.ys[1];
    let start = b.colored(ShapeKind::Stadium, rect(cx(0) - 60., y0 + 40., 120., 44.), "开始", 0x34c724, 0xd9f5d6);
    let apply = b.shape(ShapeKind::RoundRect, rect(cx(0) - 70., y0 + 130., 140., 56.), "提交报销单");
    let check = b.colored(ShapeKind::Diamond, rect(cx(1) - 75., y0 + 116., 150., 84.), "经理审批", 0xffc60a, 0xfff5cc);
    let pay = b.shape(ShapeKind::RoundRect, rect(cx(2) - 70., y0 + 250., 140., 56.), "财务打款");
    let done = b.shape(ShapeKind::RoundRect, rect(cx(0) - 70., y0 + 340., 140., 56.), "确认收款");
    let end = b.colored(ShapeKind::Stadium, rect(cx(0) - 60., y0 + 420., 120., 44.), "结束", 0xf54a45, 0xfde2e2);
    b.conn(start, apply, "");
    b.conn(apply, check, "");
    b.conn(check, pay, "通过");
    b.conn_sides(check, (0.5, 0.), apply, (0.5, 0.), "驳回");
    b.conn(pay, done, "");
    b.conn(done, end, "");
    b.d
}

fn lane_dev(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let lane = b.lane(1, "需求研发流程", &["产品", "研发", "测试"]);
    let g = lane.grid().unwrap();
    let cy = |r: usize| (g.ys[r] + g.ys[r + 1]) / 2.;
    let x0 = g.xs[1];
    let n1 = b.shape(ShapeKind::RoundRect, rect(x0 + 30., cy(0) - 28., 120., 56.), "需求评审");
    let n2 = b.shape(ShapeKind::RoundRect, rect(x0 + 180., cy(1) - 28., 120., 56.), "技术方案");
    let n3 = b.shape(ShapeKind::RoundRect, rect(x0 + 330., cy(1) - 28., 120., 56.), "开发联调");
    let n4 = b.colored(ShapeKind::Diamond, rect(x0 + 470., cy(2) - 40., 140., 80.), "测试验收", 0xffc60a, 0xfff5cc);
    let n5 = b.colored(ShapeKind::Stadium, rect(x0 + 480., cy(0) - 24., 120., 48.), "上线发布", 0x34c724, 0xd9f5d6);
    b.conn(n1, n2, "");
    b.conn(n2, n3, "");
    b.conn(n3, n4, "");
    b.conn(n4, n5, "通过");
    b.conn_sides(n4, (0.5, 1.), n3, (0.5, 1.), "缺陷");
    b.d
}

fn kanban(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let cols = [
        ("待办", 0, ["整理用户反馈", "设计新版首页", "准备周会材料"].as_slice()),
        ("进行中", 5, ["接入支付渠道", "性能优化"].as_slice()),
        ("已完成", 7, ["登录改版", "埋点上报", "数据看板"].as_slice()),
    ];
    for (i, (title, color, items)) in cols.iter().enumerate() {
        let x = i as f32 * 320.;
        b.section(rect(x, 0., 296., 560.), title);
        for (k, t) in items.iter().enumerate() {
            b.sticky(rect(x + 22., 24. + k as f32 * 132., 252., 112.), *color, t);
        }
    }
    b.d
}

fn mind_plan(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let lines: Vec<(usize, String)> = [
        (1, "项目规划"),
        (2, "目标"),
        (3, "提升转化率 20%"),
        (3, "优化新用户体验"),
        (2, "里程碑"),
        (3, "需求评审"),
        (3, "开发完成"),
        (3, "正式上线"),
        (2, "团队"),
        (3, "产品 · 设计"),
        (3, "研发 · 测试"),
        (2, "风险"),
        (3, "排期紧张"),
        (3, "第三方依赖"),
    ]
    .iter()
    .map(|(l, t)| (*l, t.to_string()))
    .collect();
    b.d.mind_from_outline(&lines, pt(0., 0.), 1, dark);
    b.d
}

fn org_chart(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let lines: Vec<(usize, String)> = [
        (1, "CEO"),
        (2, "技术部"),
        (3, "前端组"),
        (3, "后端组"),
        (3, "测试组"),
        (2, "产品部"),
        (3, "产品经理"),
        (3, "设计"),
        (2, "市场部"),
        (3, "品牌"),
        (3, "销售"),
    ]
    .iter()
    .map(|(l, t)| (*l, t.to_string()))
    .collect();
    if let Some(root) = b.d.mind_from_outline(&lines, pt(0., 0.), 2, dark) {
        if let Some(m) = b.d.index(root).and_then(|i| b.d.els[i].mind.as_mut()) {
            m.branch = 1;
        }
        // 组织架构的末级节点也用方框
        for e in b.d.els.iter_mut().filter(|e| e.is_mind() && e.no_stroke) {
            e.no_stroke = false;
            e.kind = ElKind::Shape(ShapeKind::RoundRect);
            e.fill = Some(if dark { Col::rgb(0x25272b) } else { Col::rgb(0xffffff) });
        }
        b.d.layout_minds();
    }
    b.d
}

fn swot(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.text(pt(0., -56.), "SWOT 分析", 22.);
    let cells = [
        (0., 0., 7, "S 优势\n· 技术积累深厚\n· 用户口碑好"),
        (316., 0., 2, "W 劣势\n· 品牌知名度低\n· 渠道单一"),
        (0., 236., 5, "O 机会\n· 市场需求增长\n· 政策支持"),
        (316., 236., 1, "T 威胁\n· 竞品加速追赶\n· 获客成本上升"),
    ];
    for (x, y, c, t) in cells {
        b.sticky(rect(x, y, 300., 220.), c, t);
    }
    b.d
}

fn journey(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    b.text(pt(0., -56.), "用户旅程图", 22.);
    let mut t = b.d.new_table(5, 6, pt(0., 0.), dark);
    if let ElKind::Table { cols, rows, head_col, .. } = &mut t.kind {
        *cols = vec![110., 160., 160., 160., 160., 160.];
        *rows = vec![44., 76., 76., 76., 76.];
        *head_col = true;
    }
    t.x = 0.;
    t.y = 0.;
    t.sync_grid_size();
    let data = [
        ["阶段", "认知", "考虑", "购买", "使用", "推荐"],
        ["用户行为", "看到广告", "对比竞品", "下单支付", "日常使用", "分享给朋友"],
        ["接触点", "社交媒体", "官网 / 评测", "App", "App / 客服", "朋友圈"],
        ["情绪", "好奇 🙂", "犹豫 😐", "期待 🙂", "满意 😀", "自豪 😀"],
        ["机会点", "精准投放", "突出差异", "简化流程", "新手引导", "邀请奖励"],
    ];
    for (r, row) in data.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            t.set_part_text(Part::Cell(r * 6 + c), v.to_string());
        }
    }
    b.d.els.push(t);
    b.d
}

fn brainstorm(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let c = b.colored(ShapeKind::Circle, rect(-80., -80., 160., 160.), "如何提升\n用户留存？", 0x3370ff, 0xe1eaff);
    let _ = c;
    if let Some(e) = b.d.els.last_mut() {
        e.font_size = 18.;
        e.bold = true;
    }
    let ideas = ["签到奖励", "个性化推荐", "社群运营", "会员体系", "推送召回", "新手任务", "内容更新", "积分商城"];
    for (i, t) in ideas.iter().enumerate() {
        let a = i as f32 / ideas.len() as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let (x, y) = (a.cos() * 300., a.sin() * 230.);
        b.sticky(rect(x - 75., y - 60., 150., 120.), i, t);
    }
    b.d
}

fn sequence(dark: bool) -> BoardDoc {
    let mut b = B::new(dark);
    let names = [(ShapeKind::LifeActor, "用户"), (ShapeKind::LifeBox, "前端"), (ShapeKind::LifeBox, "服务端"), (ShapeKind::LifeDb, "数据库")];
    for (i, (k, n)) in names.iter().enumerate() {
        b.shape(*k, rect(i as f32 * 210., 0., 110., 440.), n);
    }
    let cx = |i: usize| i as f32 * 210. + 55.;
    b.msg(cx(0), cx(1), 110., "输入账号密码", false);
    b.msg(cx(1), cx(2), 160., "登录请求", false);
    b.msg(cx(2), cx(3), 210., "查询用户", false);
    b.msg(cx(3), cx(2), 260., "用户信息", true);
    b.msg(cx(2), cx(1), 310., "返回 Token", true);
    b.msg(cx(1), cx(0), 360., "登录成功", true);
    b.d
}

/// 空画板右侧「从模板创建」推荐的常用模板：(列表名, 模板名, 图标, 图标色)。
const QUICK: [(&str, &str, &str, u32); 8] = [
    ("时间线", "时间线（新）", "timeline", 0x34c724),
    ("组织架构图", "组织架构图", "sitemap", 0x7f3bf5),
    ("系统架构图", "系统架构图", "layout", 0x34c724),
    ("思维导图", "思维导图", "mindmap", 0xff8800),
    ("增长飞轮", "增长飞轮", "sync", 0x34c724),
    ("金字塔", "金字塔", "triangle", 0xff8800),
    ("移动端线框原型图", "移动端线框原型图", "smartphone", 0x3370ff),
    ("快速复盘", "快速复盘", "lightbulb", 0xffc60a),
];

fn quick_index(k: usize) -> Option<usize> {
    let name = QUICK.get(k)?.1;
    TEMPLATES.iter().position(|t| t.name == name)
}

thread_local! {
    static PREVIEWS: std::cell::RefCell<std::collections::HashMap<(usize, bool), (Arc<crate::canvas::Scene>, Rect)>> = Default::default();
}

/// 模板卡片的预览场景（按明暗主题缓存，模板很多，避免每帧重新生成）。
fn preview(i: usize, dark: bool) -> (Arc<crate::canvas::Scene>, Rect) {
    PREVIEWS.with(|m| {
        m.borrow_mut()
            .entry((i, dark))
            .or_insert_with(|| {
                let mut scene = (TEMPLATES[i].build)(dark).to_scene(dark);
                scene.rough = false;
                let b = scene.items.iter().map(|it| it.bounds()).reduce(|a, b| a.union(&b)).unwrap_or_default();
                (Arc::new(scene), b)
            })
            .clone()
    })
}

impl BoardEditor {
    pub(super) fn open_templates(&mut self, cx: &mut Context<Self>) {
        self.finish_edit(cx);
        self.close_overlays();
        self.templates = Some(0);
        cx.notify();
    }

    /// 插入模板：整体放到视口中心并选中。
    pub(super) fn insert_template(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(t) = TEMPLATES.get(i) else { return };
        let doc = (t.build)(Self::dark(cx));
        let Some(b) = doc.bounds() else { return };
        let c = self.to_world(self.canvas_bounds.center());
        self.snapshot();
        self.insert_copies_at(doc.els, c.x - b.center().x, c.y - b.center().y);
        self.remove_dangling();
        self.templates = None;
        self.tool = Tool::Select;
        self.commit(cx);
        // 模板比视口大时缩放到能看全
        let vw = f32::from(self.canvas_bounds.size.width) / self.zoom;
        let vh = f32::from(self.canvas_bounds.size.height) / self.zoom;
        if b.w + 160. > vw || b.h + 160. > vh {
            let r = self.selection_bounds();
            self.zoom_to(r);
        }
    }

    /// 「从模板创建」面板是否显示：画板为空、没有别的弹窗、没在画图或编辑。
    pub(super) fn quick_visible(&self) -> bool {
        self.quick_tpl && self.doc.els.is_empty() && self.templates.is_none() && self.code_dlg.is_none() && self.tool == Tool::Select && self.editing.is_none()
    }

    /// ↑↓ 切换预览的模板。
    pub(super) fn quick_step(&mut self, d: i32, cx: &mut Context<Self>) {
        let n = QUICK.len() as i32;
        let k = match self.quick_pick {
            Some(k) => (k as i32 + d).rem_euclid(n),
            None if d > 0 => 0,
            None => n - 1,
        };
        self.quick_pick = Some(k as usize);
        cx.notify();
    }

    pub(super) fn quick_insert(&mut self, k: usize, cx: &mut Context<Self>) {
        if let Some(i) = quick_index(k) {
            self.quick_tpl = false;
            self.quick_pick = None;
            self.insert_template(i, cx);
        }
    }

    /// 预览：把当前选中的模板半透明地画在视口中心（插入后的位置与大小）。
    pub(super) fn render_quick_preview(&self, dark: bool) -> Option<AnyElement> {
        let i = quick_index(self.quick_pick?)?;
        let (scene, b) = preview(i, dark);
        let vw = f32::from(self.canvas_bounds.size.width);
        let vh = f32::from(self.canvas_bounds.size.height);
        let mut s = self.zoom;
        if (b.w + 160.) * s > vw || (b.h + 160.) * s > vh {
            s = ((vw - 160.) / b.w.max(1.)).min((vh - 160.) / b.h.max(1.)).max(0.05);
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .opacity(0.55)
                .child(
                    canvas(
                        |bb, _, _| bb,
                        move |bb, _, window, cx| {
                            let c = bb.center();
                            let origin = point(c.x - px(b.center().x * s), c.y - px(b.center().y * s));
                            paint_scene(&scene, View { origin, scale: s }, bb, window, cx);
                        },
                    )
                    .size_full(),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_quick_templates(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.quick_visible() {
            return None;
        }
        let pick = self.quick_pick;
        let items = QUICK.iter().enumerate().map(|(k, (name, _, ic, c))| {
            let on = pick == Some(k);
            div()
                .id(("quick-tpl", k))
                .h(px(36.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(14.))
                .when(on, |d| d.bg(t.hover))
                .child(icon(ic, 18., gpui::rgb(*c).into()))
                .child(*name)
                .on_hover(cx.listener(move |this, h: &bool, _, cx| {
                    if *h && this.quick_pick != Some(k) {
                        this.quick_pick = Some(k);
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.quick_insert(k, cx)))
        });
        Some(
            div()
                .absolute()
                .right(px(24.))
                .top(px(0.))
                .bottom(px(0.))
                .flex()
                .items_center()
                .child(
                    panel(t)
                        .id("quick-tpl-panel")
                        .w(px(280.))
                        .p(px(14.))
                        .flex()
                        .flex_col()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_hover(cx.listener(|this, h: &bool, _, cx| {
                            if !*h && this.quick_pick.take().is_some() {
                                cx.notify();
                            }
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .px(px(10.))
                                .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("从模板创建"))
                                .child(
                                    div()
                                        .id("quick-tpl-close")
                                        .size(px(24.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(t.hover))
                                        .child(icon("x", 16., t.text_muted))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.quick_tpl = false;
                                            this.quick_pick = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(div().px(px(10.)).mt(px(4.)).mb(px(8.)).text_size(px(12.)).text_color(t.text_muted).child("用 ↑↓ 键切换预览模板，点击插入模板"))
                        .children(items)
                        .child(
                            div()
                                .id("quick-tpl-more")
                                .mt(px(6.))
                                .h(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(13.5))
                                .text_color(t.accent)
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.8))
                                .child("更多模板")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.quick_pick = None;
                                    this.open_templates(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_templates(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let cat = self.templates?;
        let dark = t.dark;
        let cats = div().w(px(140.)).flex_none().py(px(8.)).flex().flex_col().gap(px(2.)).border_r_1().border_color(t.border).children(
            CATEGORIES.iter().enumerate().map(|(i, name)| {
                let on = i == cat;
                div()
                    .id(("tpl-cat", i))
                    .h(px(34.))
                    .mx(px(8.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.5))
                    .when(on, |d| d.bg(t.accent_soft).text_color(t.accent))
                    .hover(|s| s.bg(t.hover))
                    .child(*name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.templates = Some(i);
                        cx.notify();
                    }))
            }),
        );
        let cards = TEMPLATES.iter().enumerate().filter(|(_, tp)| cat == 0 || tp.cat == cat).map(|(i, tp)| {
            let (scene, b) = preview(i, dark);
            let (cw, ch) = (220f32, 132f32);
            div()
                .id(("tpl", i))
                .w(px(cw + 16.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(8.))
                .rounded(px(10.))
                .cursor_pointer()
                .border_1()
                .border_color(t.border)
                .hover(|s| s.border_color(t.accent).shadow_md())
                .child(
                    div().w(px(cw)).h(px(ch)).rounded(px(6.)).bg(t.bg).overflow_hidden().child(
                        canvas(
                            |bb, _, _| bb,
                            move |bb, _, window, cx| {
                                let s = ((cw - 16.) / b.w.max(1.)).min((ch - 16.) / b.h.max(1.)).min(0.6);
                                let origin = point(bb.origin.x + px(cw / 2. - b.center().x * s), bb.origin.y + px(ch / 2. - b.center().y * s));
                                paint_scene(&scene, View { origin, scale: s }, bb, window, cx);
                            },
                        )
                        .size_full(),
                    ),
                )
                .child(div().text_size(px(13.5)).child(tp.name))
                .child(div().text_size(px(12.)).text_color(t.text_muted).child(tp.desc))
                .on_click(cx.listener(move |this, _, _, cx| this.insert_template(i, cx)))
        });
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::hsla(0., 0., 0., 0.25))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.templates = None;
                        cx.notify();
                    }),
                )
                .child(
                    panel(t)
                        .id("tpl-dlg")
                        .w(px(940.))
                        .h(px(600.))
                        .flex()
                        .flex_col()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .h(px(52.))
                                .px(px(18.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_b_1()
                                .border_color(t.border)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(icon("layout-template", 18., t.accent))
                                        .child(div().text_size(px(16.)).child("模板")),
                                )
                                .child(
                                    div()
                                        .id("tpl-close")
                                        .size(px(28.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(t.hover))
                                        .child(icon("x", 16., t.text_muted))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.templates = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(div().flex_1().flex().overflow_hidden().child(cats).child(
                            div().id("tpl-scroll").flex_1().overflow_y_scroll().p(px(16.)).child(div().flex().flex_wrap().gap(px(14.)).children(cards)),
                        )),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_templates_build_and_have_content() {
        for t in super::TEMPLATES {
            for dark in [false, true] {
                let d = (t.build)(dark);
                assert!(d.els.len() >= 2, "{}", t.name);
                assert!(d.bounds().is_some_and(|b| b.w > 100. && b.h > 100.), "{}", t.name);
                // 连线两端都能找到
                for e in &d.els {
                    if let super::ElKind::Connector { a: super::End::Bound { el: x, .. }, b: super::End::Bound { el: y, .. }, .. } = &e.kind {
                        assert!(d.get(*x).is_some() && d.get(*y).is_some(), "{}", t.name);
                    }
                }
            }
        }
    }

    /// TPL_EXPORT_DIR=目录 cargo test export_template_previews 导出所有模板的 PNG，肉眼核对。
    #[test]
    fn export_template_previews() {
        let Ok(dir) = std::env::var("TPL_EXPORT_DIR") else { return };
        for (i, t) in super::TEMPLATES.iter().enumerate() {
            let mut scene = (t.build)(false).to_scene(false);
            scene.fit(24.);
            scene.bg = Some(crate::canvas::Col::rgb(0xffffff));
            let png = crate::canvas::svg::svg_to_png(&crate::canvas::svg::to_svg(&scene), 1.).unwrap();
            std::fs::write(format!("{dir}/{i:02}-{}.png", t.name), png).unwrap();
        }
    }
}
