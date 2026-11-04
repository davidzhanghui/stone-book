//! 「菜单」：在线表格的五大类级联菜单（编辑 / 插入 / 格式 / 数据 / 查看），石渠另加「文件」。
//! 菜单是一棵 `MI` 树，渲染成并排的面板；悬停在带 ▸ 的项上展开下一级（`menu_path` 记录展开路径）。

use super::*;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;

pub(crate) enum MI {
    Item {
        label: SharedString,
        icon: Option<&'static str>,
        hint: SharedString,
        cmd: String,
        on: bool,
        enabled: bool,
    },
    /// 带数字输入框的项：「向上插入 [1] 行」「行高设为 [27] 像素」，执行 `cmd:数字`
    Input {
        pre: &'static str,
        post: &'static str,
        icon: Option<&'static str>,
        input: Entity<LineInput>,
        cmd: &'static str,
        enabled: bool,
    },
    Sub {
        label: SharedString,
        icon: Option<&'static str>,
        w: f32,
        items: Vec<MI>,
        enabled: bool,
    },
    Sep,
    /// 自绘内容（条件格式的色阶 / 数据条 / 图标集缩略图）
    Custom {
        key: &'static str,
        h: f32,
    },
}

const ROW_H: f32 = 32.;
const SEP_H: f32 = 9.;

fn it(label: impl Into<SharedString>, ic: Option<&'static str>, hint: &'static str, cmd: impl Into<String>) -> MI {
    MI::Item { label: label.into(), icon: ic, hint: hint.into(), cmd: cmd.into(), on: false, enabled: true }
}

fn sub(label: impl Into<SharedString>, ic: Option<&'static str>, w: f32, items: Vec<MI>) -> MI {
    MI::Sub { label: label.into(), icon: ic, w, items, enabled: true }
}

impl MI {
    fn on(mut self, v: bool) -> Self {
        if let MI::Item { on, .. } = &mut self {
            *on = v;
        }
        self
    }
    fn enabled(mut self, v: bool) -> Self {
        match &mut self {
            MI::Item { enabled, .. } | MI::Sub { enabled, .. } | MI::Input { enabled, .. } => *enabled = v,
            MI::Sep | MI::Custom { .. } => {}
        }
        self
    }
    fn height(&self) -> f32 {
        match self {
            MI::Sep => SEP_H,
            MI::Custom { h, .. } => *h,
            _ => ROW_H,
        }
    }
}

/// 菜单里的数字输入框（插入行列数、行高、列宽）。
pub(crate) struct MenuInputs {
    pub ins: [Entity<LineInput>; 4],
    /// 条件格式「前 / 后 N 项 / %」
    pub cf: [Entity<LineInput>; 4],
    pub rh: Entity<LineInput>,
    pub cw: Entity<LineInput>,
}

impl MenuInputs {
    pub fn new(cx: &mut Context<SheetEditor>) -> Self {
        let mk = |v: &str, cx: &mut Context<SheetEditor>| {
            cx.new(|cx| {
                let mut i = LineInput::new("", cx);
                i.set_text(v, cx);
                i
            })
        };
        MenuInputs {
            ins: [mk("1", cx), mk("1", cx), mk("1", cx), mk("1", cx)],
            cf: [mk("10", cx), mk("10", cx), mk("10", cx), mk("10", cx)],
            rh: mk("25", cx),
            cw: mk("90", cx),
        }
    }
}

/// 数字格式（「常规 ▾」列表）：(名称, 格式, 示例)
pub(crate) const NUM_FMTS: [(&str, &str, &str); 17] = [
    ("常规", "general", ""),
    ("纯文本", "@", ""),
    ("数字", "0", "1024"),
    ("数字（千分位）", "#,##0", "1,024"),
    ("数字（千分位，小数点）", "#,##0.00", "1,024.56"),
    ("百分比", "0%", "10%"),
    ("百分比（小数点）", "0.00%", "10.24%"),
    ("科学计数", "0.00E+00", "1.02E+03"),
    ("人民币", "\"¥\"#,##0", "¥1,024"),
    ("人民币（小数点）", "\"¥\"#,##0.00", "¥1,024.56"),
    ("美元", "\"$\"#,##0", "$1,024"),
    ("美元（小数点）", "\"$\"#,##0.00", "$1,024.56"),
    ("日期", "yyyy/mm/dd", "2017/08/01"),
    ("日期", "yyyy-mm-dd", "2017-08-01"),
    ("时间", "h:mm:ss", "23:24:25"),
    ("日期时间", "yyyy/mm/dd h:mm:ss", "2017/08/01 23:24:25"),
    ("会计专用", "_(\"¥\"* #,##0.00_)", "¥ 1,024.56"),
];

/// 分隔线位置（在这些序号前画线）
pub(crate) const NUM_SEPS: [usize; 5] = [2, 5, 8, 12, 16];

pub(crate) const MENU_SIZES: [i32; 9] = [9, 10, 11, 12, 14, 18, 24, 30, 36];

impl SheetEditor {
    /// 数字格式子菜单（菜单「格式 › 数字格式」与「更多 › 常规 ▾」共用）。
    pub(crate) fn num_items(&self) -> Vec<MI> {
        let cur = self.cur_style().num_fmt;
        let mut v = vec![];
        for (i, (label, fmt, ex)) in NUM_FMTS.iter().enumerate() {
            if NUM_SEPS.contains(&i) {
                v.push(MI::Sep);
            }
            v.push(MI::Item { label: (*label).into(), icon: None, hint: (*ex).into(), cmd: format!("num:{fmt}"), on: cur == *fmt, enabled: true });
        }
        v.push(MI::Sep);
        v.push(it("更多格式…", None, "", "num-custom"));
        v
    }

    /// 函数分类子菜单（菜单「数据 › 公式」与「更多 › 公式」共用）。
    pub(crate) fn formula_items(&self) -> Vec<MI> {
        let mut v = vec![
            it("SUM（求和）", None, "", "insert-fn:SUM"),
            it("AVERAGE（平均值）", None, "", "insert-fn:AVERAGE"),
            it("COUNT（计数）", None, "", "insert-fn:COUNT"),
            it("MAX（最大值）", None, "", "insert-fn:MAX"),
            it("MIN（最小值）", None, "", "insert-fn:MIN"),
            MI::Sep,
            it("全部函数", None, "", "all-funcs"),
        ];
        let cat = ironcalc::base::function_catalog();
        for c in ["日期", "数据库", "信息", "逻辑", "查找", "数学", "统计", "文本", "财务", "工程"] {
            let mut names: Vec<&String> = cat.iter().filter(|x| x.1 == c).map(|x| &x.0).collect();
            names.sort();
            let items = names.into_iter().map(|n| it(n.clone(), None, "", format!("insert-fn:{n}"))).collect();
            v.push(sub(c, None, 200., items));
        }
        v
    }

    fn menu_tree(&self) -> Vec<MI> {
        let st = self.cur_style();
        let a = self.sel_area();
        let single = self.kind == SelKind::Cells && a.width == 1 && a.height == 1;
        let merged = self.merged().iter().any(|m| self.cur.r >= m.0 && self.cur.r < m.0 + m.3 && self.cur.c >= m.1 && self.cur.c < m.1 + m.2);
        let (ur, uc) = self.used();
        let has_data = ur > 1 || uc > 1 || !self.is_empty(1, 1);
        let one_col = a.width == 1 && self.kind != SelKind::Rows && self.kind != SelKind::All;
        let grid_on = self.model.borrow().get_show_grid_lines(self.sheet).unwrap_or(true);
        let al = st.alignment.clone();
        let hz = al.as_ref().map(|a| a.horizontal.clone()).unwrap_or_default();
        let vt = al.as_ref().map(|a| a.vertical.clone());
        let wrap = al.as_ref().is_some_and(|a| a.wrap_text);
        let clip = self.sx().is_clip(self.cur.r, self.cur.c);
        use ironcalc::base::types::{HorizontalAlignment as H, VerticalAlignment as V};
        let mi = &self.menu_inputs;
        let views = self.sx().views.clone();
        let mut view_items = vec![it("新建视图", Some("sh-view"), "", "new-view")];
        if !views.is_empty() {
            view_items.push(MI::Sep);
            for (i, v) in views.iter().enumerate() {
                view_items.push(it(v.name.clone(), None, "", format!("view:{i}")).on(self.view_on == Some(i)));
            }
            view_items.push(MI::Sep);
            view_items.push(it("关闭视图", None, "", "view-close").enabled(self.view_on.is_some()));
        }
        vec![
            sub(
                "文件",
                Some("sh-import"),
                220.,
                vec![
                    it("导入 Excel / CSV", Some("sh-import"), "", "import"),
                    it("导出为 Excel（.xlsx）", Some("sh-export"), "", "export-xlsx"),
                    it("导出为 CSV", Some("sh-export"), "", "export-csv"),
                    MI::Sep,
                    it("快捷键", Some("sh-keyboard"), "⌘/", "shortcuts"),
                ],
            ),
            MI::Sep,
            sub(
                "编辑",
                None,
                290.,
                vec![
                    it("撤销", Some("sh-undo"), "⌘Z", "undo").enabled(self.model.borrow().can_undo()),
                    it("重做", Some("sh-redo"), "⌘Y", "redo").enabled(self.model.borrow().can_redo()),
                    MI::Sep,
                    it("剪切", Some("sh-cut"), "⌘X", "cut"),
                    it("复制", Some("sh-copy"), "⌘C", "copy"),
                    it("复制为图片", Some("sh-snap"), "", "copy-image"),
                    it("粘贴", Some("sh-paste"), "⌘V", "paste"),
                    sub(
                        "选择性粘贴",
                        None,
                        230.,
                        vec![
                            it("仅粘贴值", None, "⇧⌘V", "paste-values"),
                            it("仅粘贴格式", None, "⌥⌘V", "paste-format"),
                            it("仅粘贴公式", None, "", "paste-formulas"),
                            it("仅粘贴数据验证", None, "", "paste-dv"),
                            MI::Sep,
                            it("将行与列转置", None, "", "paste-transpose"),
                        ],
                    ),
                    MI::Sep,
                    it("删除行", None, "⌘⌥-", "del-rows"),
                    it("删除列", None, "", "del-cols"),
                    it("删除单元格，右侧单元格左移", None, "", "del-cells-left"),
                    it("删除单元格，下方单元格上移", None, "", "del-cells-up"),
                    MI::Sep,
                    it("查找和替换", Some("sh-find"), "⇧⌘H", "replace"),
                ],
            ),
            sub(
                "插入",
                None,
                290.,
                vec![
                    MI::Input { pre: "向上插入", post: "行", icon: Some("sh-row-above"), input: mi.ins[0].clone(), cmd: "row-above", enabled: true },
                    MI::Input { pre: "向下插入", post: "行", icon: Some("sh-row-below"), input: mi.ins[1].clone(), cmd: "row-below", enabled: true },
                    MI::Sep,
                    MI::Input { pre: "向左插入", post: "列", icon: Some("sh-col-left"), input: mi.ins[2].clone(), cmd: "col-left", enabled: true },
                    MI::Input { pre: "向右插入", post: "列", icon: Some("sh-col-right"), input: mi.ins[3].clone(), cmd: "col-right", enabled: true },
                    MI::Sep,
                    it("插入单元格，现有单元格右移", Some("sh-col-right"), "", "ins-cells-right"),
                    it("插入单元格，现有单元格下移", Some("sh-row-below"), "", "ins-cells-down"),
                    MI::Sep,
                    it("复选框", Some("sh-checkbox"), "", "ins-check"),
                    it("下拉列表", Some("sh-dropdown"), "", "ins-list"),
                    it("日期选项", Some("sh-date"), "", "ins-date"),
                    it("日期提醒", Some("sh-remind"), "", "ins-remind"),
                    MI::Sep,
                    it("图片", Some("sh-image"), "", "ins-image"),
                    it("迷你图", Some("sh-spark"), "", "ins-spark"),
                    MI::Sep,
                    it("链接", Some("sh-link"), "⌘K", "ins-link"),
                    it("附件", Some("sh-attach"), "", "ins-file"),
                    MI::Sep,
                    it("评论", Some("sh-comment"), "⌥⌘M", "comment"),
                    it("备注", Some("sh-note"), "", "ins-note"),
                ],
            ),
            sub(
                "格式",
                None,
                270.,
                vec![
                    sub("数字格式", None, 300., self.num_items()),
                    MI::Sep,
                    it("加粗", Some("bold"), "⌘B", "bold").on(st.font.b),
                    it("斜体", Some("italic"), "⌘I", "italic").on(st.font.i),
                    it("下划线", Some("underline"), "⌘U", "underline").on(st.font.u),
                    it("删除线", Some("strikethrough"), "⇧⌘X", "strike").on(st.font.strike),
                    MI::Sep,
                    sub("字号", None, 120., MENU_SIZES.iter().map(|s| it(s.to_string(), None, "", format!("size:{s}")).on(st.font.sz == *s)).collect()),
                    MI::Sep,
                    sub(
                        "水平对齐",
                        Some("sh-h-left"),
                        160.,
                        vec![
                            it("向左对齐", Some("sh-h-left"), "⇧⌘L", "h-left").on(matches!(hz, H::Left | H::General)),
                            it("中心对齐", Some("sh-h-center"), "⇧⌘E", "h-center").on(matches!(hz, H::Center)),
                            it("向右对齐", Some("sh-h-right"), "⇧⌘R", "h-right").on(matches!(hz, H::Right)),
                        ],
                    ),
                    sub(
                        "垂直对齐",
                        Some("sh-v-mid"),
                        160.,
                        vec![
                            it("顶部对齐", Some("sh-v-top"), "", "v-top").on(matches!(vt, Some(V::Top))),
                            it("居中对齐", Some("sh-v-mid"), "", "v-middle").on(vt.is_none() || matches!(vt, Some(V::Center))),
                            it("底部对齐", Some("sh-v-bottom"), "", "v-bottom").on(matches!(vt, Some(V::Bottom))),
                        ],
                    ),
                    sub(
                        "文本换行",
                        Some("sh-wrap"),
                        150.,
                        vec![
                            it("溢出", Some("sh-overflow"), "", "overflow").on(!wrap && !clip),
                            it("截断", Some("sh-clip"), "", "clip").on(clip),
                            it("自动换行", Some("sh-wrap"), "", "wrap").on(wrap),
                        ],
                    ),
                    it(if merged { "取消合并" } else { "合并单元格" }, Some("sh-merge"), "⌥⇧D", "merge-toggle").enabled(!single || merged),
                    MI::Sep,
                    MI::Input { pre: "行高设为", post: "像素", icon: Some("sh-height"), input: mi.rh.clone(), cmd: "row-height", enabled: true },
                    it("行高自适应", None, "", "autofit-rows"),
                    MI::Input { pre: "列宽设为", post: "像素", icon: Some("sh-width"), input: mi.cw.clone(), cmd: "col-width", enabled: true },
                    it("列宽自适应", None, "", "autofit-cols"),
                    MI::Sep,
                    sub("条件格式", Some("sh-cond"), 200., self.cf_items()),
                    sub(
                        "高亮重复值",
                        None,
                        230.,
                        vec![
                            it("高亮重复值", None, "", "cf:dup"),
                            it("清除所选单元格的高亮", None, "", "cf-clear-dup"),
                            it("清除整个工作表的高亮", None, "", "cf-clear-dup-all"),
                        ],
                    ),
                    MI::Sep,
                    it("格式刷", Some("sh-painter"), "⌃⌘C", "painter"),
                    it("清除格式", Some("sh-clear"), "⌘\\", "clear-format"),
                ],
            ),
            sub(
                "数据",
                None,
                220.,
                vec![
                    it("升序 A→Z", Some("sh-sort-asc"), "⌥⇧F", "sort-asc").enabled(has_data),
                    it("降序 Z→A", Some("sh-sort-desc"), "", "sort-desc").enabled(has_data),
                    it("自定义排序", Some("sh-list"), "", "sort-custom").enabled(has_data),
                    MI::Sep,
                    sub(
                        "筛选",
                        Some("sh-filter"),
                        200.,
                        vec![it(if self.filter.is_some() { "取消筛选" } else { "筛选" }, None, "⌃⇧L", "filter"), it("新建视图以筛选", None, "", "new-view")],
                    ),
                    MI::Sep,
                    it("分列", Some("sh-split"), "", "split").enabled(one_col && has_data),
                    MI::Sep,
                    it("数据验证", Some("sh-validate"), "", "dv"),
                    sub("数据清洗", Some("sh-clean"), 180., vec![it("清洗建议", None, "", "clean"), it("删除重复项", None, "", "dedupe")]).enabled(has_data),
                    MI::Sep,
                    sub("跨表引用", Some("sh-xref"), 200., vec![it("新建跨表引用数据", None, "", "xref")]),
                    sub("公式", Some("sh-sigma"), 220., self.formula_items()),
                ],
            ),
            sub(
                "查看",
                None,
                200.,
                vec![
                    sub("冻结", Some("sh-freeze"), 270., self.freeze_items()),
                    sub("视图", Some("sh-view"), 200., view_items),
                    it("网格线", Some("sh-grid"), "", "grid").on(grid_on),
                    MI::Sep,
                    it("显示公式", Some("sh-fx"), "⌃`", "show-formulas").on(self.show_formulas),
                    it("放大", Some("zoom-in"), "⌘+", "zoom-in"),
                    it("缩小", Some("zoom-out"), "⌘-", "zoom-out"),
                ],
            ),
        ]
    }

    /// 渲染级联菜单：`items` 为根；每一级面板最高 `max_h`，超出可滚动。
    pub(crate) fn cascade(&mut self, root: Vec<MI>, root_w: f32, max_h: f32, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        // 面板高度：各项 + 上下内边距 4 + 边框 1
        let panel_h = |items: &[MI]| (items.iter().map(|m| m.height()).sum::<f32>() + 10.).min(max_h);
        let path = self.menu_path.clone();
        let mut panels: Vec<(Vec<MI>, f32, f32)> = vec![];
        let mut items = root;
        let mut w = root_w;
        let mut top = 0.;
        let mut level = 0;
        loop {
            let next = path.get(level).copied().and_then(|i| match items.get(i) {
                Some(MI::Sub { enabled: true, .. }) => {
                    let y = 4. + items[..i].iter().map(|m| m.height()).sum::<f32>();
                    Some((i, y))
                }
                _ => None,
            });
            match next {
                Some((i, y)) => {
                    let MI::Sub { items: sub_items, w: sub_w, icon: ic, .. } = items.remove(i) else { unreachable!() };
                    // 占位：保持下标（渲染时还要用到父级的这一项）
                    items.insert(i, MI::Sub { label: "".into(), icon: ic, w: 0., items: vec![], enabled: true });
                    let ph = panel_h(&items);
                    panels.push((std::mem::take(&mut items), w, top));
                    // 子菜单与悬停项对齐，但不超出父面板底边（否则整个菜单会被挤着往上跳）
                    top = (top + y - 4.).min(top + ph - panel_h(&sub_items)).max(0.);
                    items = sub_items;
                    w = sub_w;
                    level += 1;
                }
                None => {
                    panels.push((items, w, top));
                    break;
                }
            }
        }
        // 父级被占位的项要显示原来的文字：重新从树上取
        let labels = self.path_labels(&path);
        let mut row = div().flex().flex_row().items_start();
        for (lv, (items, w, top)) in panels.into_iter().enumerate() {
            let open = path.get(lv).copied();
            let mut p = Self::panel(t).w(px(w)).mt(px(top)).max_h(px(max_h)).id(("mp", lv)).overflow_y_scroll();
            for (i, m) in items.into_iter().enumerate() {
                let hover_path: Vec<usize> = path.iter().take(lv).copied().chain(std::iter::once(i)).collect();
                let hp = hover_path.clone();
                let track = cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                    if this.menu_path != hp {
                        this.menu_path = hp.clone();
                        cx.notify();
                    }
                });
                p = p.child(match m {
                    MI::Sep => Self::sep(t).into_any_element(),
                    MI::Custom { key, .. } => self.cf_gallery(key, t, cx).into_any_element(),
                    MI::Item { label, icon: ic, hint, cmd, on, enabled } => {
                        let hp = path.iter().take(lv).copied().collect::<Vec<_>>();
                        Self::menu_row(("mi", lv * 1000 + i), ic, label, hint, t)
                            .when(on, |d| d.text_color(t.accent).child(icon("sh-check", 14., t.accent)))
                            .when(!enabled, |d| d.opacity(0.4))
                            .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                                if this.menu_path != hp {
                                    this.menu_path = hp.clone();
                                    cx.notify();
                                }
                            }))
                            .when(enabled, |d| d.on_click(Self::cmd(cmd, cx)))
                            .into_any_element()
                    }
                    MI::Input { pre, post, icon: ic, input, cmd, enabled } => {
                        let inp = input.clone();
                        div()
                            .id(("mi", lv * 1000 + i))
                            .mx(px(4.))
                            .px(px(8.))
                            .h(px(ROW_H))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .when(!enabled, |d| d.opacity(0.4))
                            .child(div().w(px(16.)).flex_none().when_some(ic, |d, i| d.child(icon(i, 15., t.text_secondary))))
                            .child(pre)
                            .child(
                                div()
                                    .id(("mi-in", lv * 1000 + i))
                                    .w(px(52.))
                                    .h(px(24.))
                                    .px(px(6.))
                                    .rounded(px(5.))
                                    .border_1()
                                    .border_color(t.border_strong)
                                    .line_height(px(22.))
                                    .text_size(px(12.5))
                                    .child(input.clone())
                                    .on_click(|_, _, cx| cx.stop_propagation()),
                            )
                            .child(post)
                            .on_mouse_move(track)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                let n = inp.read(cx).text().trim().to_string();
                                this.run(&format!("{cmd}:{n}"), w, cx)
                            }))
                            .into_any_element()
                    }
                    MI::Sub { label, icon: ic, enabled, .. } => {
                        let label = if label.is_empty() { labels.get(lv).cloned().unwrap_or_default() } else { label };
                        let is_open = open == Some(i);
                        Self::menu_row(("mi", lv * 1000 + i), ic, label, "", t)
                            .when(is_open, |d| d.bg(t.hover))
                            .when(!enabled, |d| d.opacity(0.4))
                            .child(icon("chevron-right", 13., t.text_muted))
                            .on_mouse_move(track)
                            .into_any_element()
                    }
                });
            }
            row = row.child(p);
        }
        row.into_any_element()
    }

    fn path_labels(&self, path: &[usize]) -> Vec<SharedString> {
        let mut out = vec![];
        let mut items = self.menu_root();
        for &i in path {
            match items.into_iter().nth(i) {
                Some(MI::Sub { label, items: sub, .. }) => {
                    out.push(label);
                    items = sub;
                }
                _ => break,
            }
        }
        out
    }

    /// 右键菜单（单元格 / 行头 / 列头）。低频项收进二级菜单，免得菜单比窗口还高。
    pub(crate) fn ctx_items(&self, kind: Pop) -> Vec<MI> {
        let a = self.sel_area();
        let rows_n = if self.kind == SelKind::Cols || self.kind == SelKind::All { 1 } else { a.height.min(9999) };
        let cols_n = if self.kind == SelKind::Rows || self.kind == SelKind::All { 1 } else { a.width.min(999) };
        let mi = &self.menu_inputs;
        let mut v = vec![
            it("剪切", Some("sh-cut"), "⌘X", "cut"),
            it("复制", Some("sh-copy"), "⌘C", "copy"),
            it("粘贴", Some("sh-paste"), "⌘V", "paste"),
            sub(
                "选择性粘贴",
                None,
                200.,
                vec![
                    it("仅粘贴值", None, "⇧⌘V", "paste-values"),
                    it("仅粘贴格式", None, "⌥⌘V", "paste-format"),
                    it("仅粘贴公式", None, "", "paste-formulas"),
                    MI::Sep,
                    it("将行与列转置", None, "", "paste-transpose"),
                ],
            ),
            MI::Sep,
        ];
        if kind != Pop::ColCtx {
            v.push(it(format!("向上插入 {rows_n} 行"), Some("sh-row-above"), "", "row-above"));
            v.push(it(format!("向下插入 {rows_n} 行"), Some("sh-row-below"), "", "row-below"));
        }
        if kind != Pop::RowCtx {
            v.push(it(format!("向左插入 {cols_n} 列"), Some("sh-col-left"), "", "col-left"));
            v.push(it(format!("向右插入 {cols_n} 列"), Some("sh-col-right"), "", "col-right"));
        }
        v.push(MI::Sep);
        if kind != Pop::ColCtx {
            v.push(it(format!("删除 {rows_n} 行"), Some("sh-trash"), "⌘⌥-", "del-rows"));
        }
        if kind != Pop::RowCtx {
            v.push(it(format!("删除 {cols_n} 列"), Some("sh-trash"), "", "del-cols"));
        }
        if kind == Pop::Ctx {
            v.push(sub("删除单元格", None, 200., vec![it("右侧单元格左移", None, "", "del-cells-left"), it("下方单元格上移", None, "", "del-cells-up")]));
            v.push(sub("插入单元格", None, 200., vec![it("现有单元格右移", None, "", "ins-cells-right"), it("现有单元格下移", None, "", "ins-cells-down")]));
        }
        v.push(it("清除内容", Some("sh-clear"), "⌫", "clear-content"));
        v.push(MI::Sep);
        match kind {
            Pop::RowCtx => v.extend([
                MI::Input { pre: "行高设为", post: "像素", icon: Some("sh-height"), input: mi.rh.clone(), cmd: "row-height", enabled: true },
                it("隐藏行", Some("sh-eye-off"), "⌘⌥9", "hide-rows"),
                it("取消隐藏行", Some("sh-eye"), "⇧⌘9", "unhide-rows"),
                it(format!("冻结至第 {} 行", self.cur.r), Some("sh-freeze"), "", "freeze-row-here"),
            ]),
            Pop::ColCtx => {
                let c = self.cur.c;
                v.extend([
                    MI::Input { pre: "列宽设为", post: "像素", icon: Some("sh-width"), input: mi.cw.clone(), cmd: "col-width", enabled: true },
                    it("隐藏列", Some("sh-eye-off"), "⌘⌥0", "hide-cols"),
                    it("取消隐藏列", Some("sh-eye"), "⇧⌘0", "unhide-cols"),
                    it(format!("冻结至第 {} 列", col_name(c)), Some("sh-freeze"), "", "freeze-col-here"),
                    MI::Sep,
                    it("按此列升序排序", Some("sh-sort-asc"), "", format!("sort-asc:{c}")),
                    it("按此列降序排序", Some("sh-sort-desc"), "", format!("sort-desc:{c}")),
                ]);
            }
            _ => {
                let x = self.sx().cell(self.cur.r, self.cur.c).cloned().unwrap_or_default();
                let has_link = self.cell_link(self.cur.r, self.cur.c).is_some();
                v.push(it(if has_link { "编辑链接" } else { "插入链接" }, Some("sh-link"), "⌘K", "ins-link"));
                if has_link {
                    v.push(it("取消链接", Some("sh-unlink"), "", "unlink"));
                }
                v.push(it("评论", Some("sh-comment"), "⌥⌘M", "comment"));
                if x.note.is_empty() {
                    v.push(it("插入备注", Some("sh-note"), "", "ins-note"));
                } else {
                    v.push(sub("备注", Some("sh-note"), 140., vec![it("编辑备注", None, "", "ins-note"), it("删除备注", None, "", "del-note")]));
                }
                if !x.image.is_empty() {
                    v.push(sub("图片", Some("sh-image"), 140., vec![it("查看图片", None, "", "view-image"), it("删除图片", None, "", "del-image")]));
                }
                if !x.files.is_empty() {
                    v.push(it("删除附件", Some("sh-attach"), "", "del-files"));
                }
                if x.spark.is_some() {
                    v.push(sub("迷你图", Some("sh-spark"), 140., vec![it("编辑迷你图", None, "", "edit-spark"), it("删除迷你图", None, "", "del-spark")]));
                }
                v.push(it("下拉列表", Some("sh-dropdown"), "", "ins-list"));
                v.push(MI::Sep);
                v.push(sub(
                    "排序",
                    Some("sh-sort-asc"),
                    160.,
                    vec![it("升序排序", Some("sh-sort-asc"), "", "sort-asc"), it("降序排序", Some("sh-sort-desc"), "", "sort-desc")],
                ));
                v.push(it(if self.filter.is_some() { "取消筛选" } else { "筛选" }, Some("sh-filter"), "⌃⇧L", "filter"));
                v.push(it("合并 / 取消合并", Some("sh-merge"), "⌥⇧D", "merge-toggle"));
                v.push(it(format!("冻结至第 {} 行 {} 列", self.cur.r, col_name(self.cur.c)), Some("sh-freeze"), "", "freeze-here"));
            }
        }
        v
    }

    pub(crate) fn menu_root(&self) -> Vec<MI> {
        match self.menu_kind {
            MenuKind::Ctx(k) => self.ctx_items(k),
            MenuKind::Main => self.menu_tree(),
            MenuKind::Num => self.num_items(),
            MenuKind::Formula => self.formula_items(),
            MenuKind::Cond => self.cf_items(),
            MenuKind::Freeze => self.freeze_items(),
            MenuKind::Filter => {
                vec![it(if self.filter.is_some() { "取消筛选" } else { "筛选" }, None, "⌃⇧L", "filter"), it("新建视图以筛选", None, "", "new-view")]
            }
        }
    }

    pub(crate) fn freeze_items(&self) -> Vec<MI> {
        vec![
            it("冻结首行", Some("sh-row-above"), "", "freeze-rows:1").on(self.fr == 1 && self.fc == 0),
            it("冻结首列", Some("sh-col-left"), "", "freeze-cols:1").on(self.fc == 1 && self.fr == 0),
            it(format!("冻结至当前行列（{} 行 {} 列）", self.cur.r, col_name(self.cur.c)), Some("sh-freeze"), "", "freeze-here"),
            MI::Sep,
            it("取消冻结", None, "", "freeze-none").enabled(self.fr > 0 || self.fc > 0),
        ]
    }

    pub(crate) fn cf_items(&self) -> Vec<MI> {
        let mi = &self.menu_inputs;
        vec![
            sub(
                "突出显示单元格",
                None,
                150.,
                vec![
                    it("大于", None, "", "cf-ask:gt"),
                    it("小于", None, "", "cf-ask:lt"),
                    it("介于", None, "", "cf-ask:between"),
                    it("等于", None, "", "cf-ask:eq"),
                    it("文本包含", None, "", "cf-ask:contains"),
                    sub(
                        "日期为",
                        None,
                        120.,
                        vec![
                            it("昨天", None, "", "cf:date=yesterday"),
                            it("今天", None, "", "cf:date=today"),
                            it("明天", None, "", "cf:date=tomorrow"),
                            it("最近 7 天", None, "", "cf:date=last7"),
                            it("上周", None, "", "cf:date=lastweek"),
                            it("本周", None, "", "cf:date=thisweek"),
                            it("下周", None, "", "cf:date=nextweek"),
                            it("上月", None, "", "cf:date=lastmonth"),
                            it("本月", None, "", "cf:date=thismonth"),
                            it("下月", None, "", "cf:date=nextmonth"),
                        ],
                    ),
                    it("重复值", None, "", "cf:dup"),
                    it("唯一值", None, "", "cf:unique"),
                    it("空值", None, "", "cf:blank"),
                    it("其他规则", None, "", "cf-new"),
                ],
            ),
            sub(
                "最前/最后/平均值",
                None,
                170.,
                vec![
                    MI::Input { pre: "前", post: "项", icon: None, input: mi.cf[0].clone(), cmd: "cf-top", enabled: true },
                    MI::Input { pre: "前", post: "%", icon: None, input: mi.cf[1].clone(), cmd: "cf-toppct", enabled: true },
                    MI::Input { pre: "后", post: "项", icon: None, input: mi.cf[2].clone(), cmd: "cf-bottom", enabled: true },
                    MI::Input { pre: "后", post: "%", icon: None, input: mi.cf[3].clone(), cmd: "cf-bottompct", enabled: true },
                    it("高于平均值", None, "", "cf:above"),
                    it("低于平均值", None, "", "cf:below"),
                ],
            ),
            it("自定义公式", None, "", "cf-new:formula"),
            MI::Sep,
            sub("色阶", None, 230., vec![MI::Custom { key: "scales", h: 196. }]),
            sub("数据条", None, 210., vec![MI::Custom { key: "bars", h: 196. }]),
            sub("图标集", None, 330., vec![MI::Custom { key: "icons", h: 330. }]),
            MI::Sep,
            it("新建规则", None, "", "cf-new"),
            it("管理规则", None, "", "cf-panel"),
            MI::Sep,
            it("清除所选单元格的规则", None, "", "cf-clear").enabled(self.cf_count(true) > 0),
            it("清除整张工作表的规则", None, "", "cf-clear-all").enabled(self.cf_count(false) > 0),
        ]
    }
}

/// 当前打开的是哪一棵级联菜单。
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum MenuKind {
    Main,
    Num,
    Formula,
    Cond,
    Freeze,
    Filter,
    /// 右键菜单（Pop::Ctx / RowCtx / ColCtx）
    Ctx(Pop),
}
