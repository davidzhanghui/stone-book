//! 数据菜单与条件格式规则编辑：新建 / 编辑条件格式规则、数据验证、自定义排序、分列、删除重复项、清洗建议、
//! 跨表引用、全部函数面板、筛选视图。

use super::cf::hex_of;
use super::ext::{Rule, View};
use super::*;
use crate::base::widgets::{check_box, ghost_btn, input_box, primary_btn};
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use ironcalc::base::cf_types::{CfRule, CfRuleInput, PeriodType, TextOperator, ValueOperator};
use ironcalc::base::types::{Color, Dxf, DxfFont, Fill};

/// 条件格式规则编辑器的条件类型：(键, 名称, 需要几个值)
const CF_TYPES: [(&str, &str, u8); 24] = [
    ("gt", "大于", 1),
    ("lt", "小于", 1),
    ("ge", "大于或等于", 1),
    ("le", "小于或等于", 1),
    ("eq", "等于", 1),
    ("ne", "不等于", 1),
    ("between", "介于", 2),
    ("notbetween", "不介于", 2),
    ("contains", "文本包含", 1),
    ("notcontains", "文本不包含", 1),
    ("begins", "文本开头是", 1),
    ("ends", "文本结尾是", 1),
    ("date=today", "日期为今天", 0),
    ("date=yesterday", "日期为昨天", 0),
    ("date=tomorrow", "日期为明天", 0),
    ("date=last7", "日期为最近 7 天", 0),
    ("date=thisweek", "日期为本周", 0),
    ("date=thismonth", "日期为本月", 0),
    ("dup", "重复值", 0),
    ("unique", "唯一值", 0),
    ("blank", "空值", 0),
    ("notblank", "非空值", 0),
    ("above", "高于平均值", 0),
    ("formula", "自定义公式", 1),
];

/// 规则样式里可选的颜色
const FMT_FILLS: [&str; 9] = ["", "#ffc7ce", "#ffeb9c", "#c6efce", "#d4f3fd", "#e1eaff", "#ece2fe", "#dee0e3", "#fde2e2"];
const FMT_FONTS: [&str; 8] = ["", "#9c0006", "#9c5700", "#006100", "#1f3a93", "#7f3bf5", "#f54a45", "#1f2329"];

/// 数据验证类型：(名称, 规则)
const DV_TYPES: [&str; 5] = ["下拉列表", "复选框", "日期", "数字", "文本长度"];
const DV_OPS: [(&str, &str); 8] = [
    ("between", "介于"),
    ("notBetween", "不介于"),
    ("eq", "等于"),
    ("ne", "不等于"),
    ("gt", "大于"),
    ("lt", "小于"),
    ("ge", "大于或等于"),
    ("le", "小于或等于"),
];

pub(crate) enum DataDlg {
    /// 条件格式规则：(编辑的规则序号)
    Cf {
        index: Option<u32>,
        range: Entity<LineInput>,
        ty: usize,
        a: Entity<LineInput>,
        b: Entity<LineInput>,
        fill: String,
        font: String,
        bold: bool,
        italic: bool,
        strike: bool,
        open: bool,
    },
    Dv {
        range: Entity<LineInput>,
        ty: usize,
        op: usize,
        a: Entity<LineInput>,
        b: Entity<LineInput>,
        reject: bool,
        hint: Entity<LineInput>,
        open: u8,
    },
    /// 自定义排序：区域、首行为标题、排序条件 (列, 升序)
    Sort {
        area: (i32, i32, i32, i32),
        header: bool,
        keys: Vec<(i32, bool)>,
        open: Option<usize>,
    },
    /// 分列：分隔符 0 逗号 1 空格 2 分号 3 制表符 4 自定义
    Split {
        area: (i32, i32, i32, i32),
        sep: u8,
        custom: Entity<LineInput>,
    },
    Dedupe {
        area: (i32, i32, i32, i32),
        header: bool,
        cols: Vec<(i32, bool)>,
    },
    Clean {
        items: Vec<(String, Vec<(i32, i32, String)>, bool)>,
    },
    XRef {
        list: Vec<(String, String)>,
        sel: Option<usize>,
        range: Entity<LineInput>,
    },
}

impl SheetEditor {
    fn small_input(&mut self, text: &str, ph: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<LineInput> {
        let i = cx.new(|cx| LineInput::new(ph, cx));
        i.update(cx, |i, cx| i.set_text(text, cx));
        let s = cx.subscribe_in(&i, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => this.data_confirm(window, cx),
            InputEvent::Cancel => this.close_dlg(window, cx),
            InputEvent::Changed => cx.notify(),
            _ => {}
        });
        self.dlg_subs.push(s);
        i
    }

    fn open_data(&mut self, d: DataDlg, cx: &mut Context<Self>) {
        self.pop = None;
        self.dlg = Some(super::dialogs::Dlg::Data(Box::new(d)));
        cx.notify();
    }

    fn data_mut(&mut self) -> Option<&mut DataDlg> {
        match &mut self.dlg {
            Some(super::dialogs::Dlg::Data(d)) => Some(d),
            _ => None,
        }
    }

    /// 当前格所在的连续数据区（选区多行时用选区）。
    pub(crate) fn data_region(&self) -> (i32, i32, i32, i32) {
        let a = self.sel_area();
        if self.kind == SelKind::Cells && (a.height > 1 || a.width > 1) {
            return (a.row, a.column, a.row + a.height - 1, a.column + a.width - 1);
        }
        let (ur, uc) = self.used();
        if self.kind != SelKind::Cells {
            let s = self.sel_used();
            return (s.row, s.column, s.row + s.height - 1, s.column + s.width - 1);
        }
        let (mut r0, mut r1, mut c0, mut c1) = (self.cur.r, self.cur.r, self.cur.c, self.cur.c);
        loop {
            let mut grew = false;
            if r0 > 1 && (c0..=c1).any(|c| !self.is_empty(r0 - 1, c)) {
                r0 -= 1;
                grew = true;
            }
            if r1 < ur && (c0..=c1).any(|c| !self.is_empty(r1 + 1, c)) {
                r1 += 1;
                grew = true;
            }
            if c0 > 1 && (r0..=r1).any(|r| !self.is_empty(r, c0 - 1)) {
                c0 -= 1;
                grew = true;
            }
            if c1 < uc && (r0..=r1).any(|r| !self.is_empty(r, c1 + 1)) {
                c1 += 1;
                grew = true;
            }
            if !grew {
                break;
            }
        }
        (r0, c0, r1, c1)
    }

    pub(crate) fn run_data(&mut self, head: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match head {
            "sort-custom" => {
                let area = self.data_region();
                let header =
                    self.model.borrow().get_cell_type(self.sheet, area.0, area.1).map(|t| matches!(t, CellType::Text)).unwrap_or(false) && area.2 > area.0;
                self.open_data(DataDlg::Sort { area, header, keys: vec![(self.cur.c.clamp(area.1, area.3), true)], open: None }, cx);
            }
            "split" => {
                let a = self.sel_used();
                let (ur, _) = self.used();
                let r1 = if self.kind == SelKind::Cols { ur } else { a.row + a.height - 1 };
                let custom = self.small_input("", "自定义分隔符", window, cx);
                let txt = self.model.borrow().get_formatted_cell_value(self.sheet, a.row, a.column).unwrap_or_default();
                let sep = if txt.contains(',') || txt.contains('，') {
                    0
                } else if txt.contains(';') {
                    2
                } else if txt.contains('\t') {
                    3
                } else {
                    1
                };
                self.open_data(DataDlg::Split { area: (a.row, a.column, r1, a.column), sep, custom }, cx);
            }
            "dedupe" => {
                let area = self.data_region();
                let header =
                    self.model.borrow().get_cell_type(self.sheet, area.0, area.1).map(|t| matches!(t, CellType::Text)).unwrap_or(false) && area.2 > area.0;
                self.open_data(DataDlg::Dedupe { area, header, cols: (area.1..=area.3).map(|c| (c, true)).collect() }, cx);
            }
            "clean" => {
                let items = self.clean_scan();
                self.open_data(DataDlg::Clean { items }, cx);
            }
            "dv" => self.open_dv(window, cx),
            "xref" => {
                let list = self.sheet_list();
                let range = self.small_input("", "例如 A1:D20（留空为整个数据区）", window, cx);
                self.open_data(DataDlg::XRef { list, sel: None, range }, cx);
            }
            "all-funcs" => {
                self.side = Some(super::dialogs::Side::Funcs);
                let q = self.func_q.clone();
                window.focus(&q.focus_handle(cx));
                cx.notify();
            }
            "new-view" => self.new_view(cx),
            "view" => {
                if let Some(v) = self.sx().views.get(arg.parse::<usize>().unwrap_or(usize::MAX)).cloned() {
                    self.view_on = arg.parse().ok();
                    self.filter = Some(Filter {
                        sheet: self.sheet,
                        head: v.head,
                        last: v.last,
                        c0: v.c0,
                        c1: v.c1,
                        excl: v.excl.into_iter().map(|(c, e)| (c, e.into_iter().collect())).collect(),
                        hidden: Default::default(),
                    });
                    self.apply_filter();
                    self.toast = Some(format!("已切换到视图「{}」", v.name).into());
                    cx.notify();
                }
            }
            "view-close" => {
                self.view_on = None;
                self.filter = None;
                self.filter_ver += 1;
                cx.notify();
            }
            "view-del" => {
                if let Ok(i) = arg.parse::<usize>() {
                    self.update_sx(
                        |x| {
                            if i < x.views.len() {
                                x.views.remove(i);
                            }
                        },
                        cx,
                    );
                    self.view_on = None;
                    self.filter = None;
                    self.filter_ver += 1;
                }
            }
            "xref-refresh" => self.xref_refresh(cx),
            _ => return false,
        }
        true
    }

    // ───────────── 筛选视图 ─────────────

    fn new_view(&mut self, cx: &mut Context<Self>) {
        if self.filter.as_ref().is_none_or(|f| f.sheet != self.sheet) {
            self.toggle_filter(cx);
        }
        let Some(f) = &self.filter else { return };
        let n = self.sx().views.len() + 1;
        let v = View {
            name: format!("视图 {n}"),
            head: f.head,
            last: f.last,
            c0: f.c0,
            c1: f.c1,
            excl: f.excl.iter().map(|(c, e)| (*c, e.iter().cloned().collect())).collect(),
        };
        self.update_sx(|x| x.views.push(v), cx);
        self.view_on = Some(n - 1);
        self.toast = Some(format!("已新建「视图 {n}」：在视图里筛选不影响其他视图").into());
    }

    /// 筛选条件变化时写回当前视图。
    pub(crate) fn save_view(&mut self, cx: &mut Context<Self>) {
        let (Some(i), Some(f)) = (self.view_on, self.filter.as_ref()) else { return };
        let excl: std::collections::BTreeMap<i32, Vec<String>> = f.excl.iter().map(|(c, e)| (*c, e.iter().cloned().collect())).collect();
        self.update_sx(
            |x| {
                if let Some(v) = x.views.get_mut(i) {
                    v.excl = excl;
                }
            },
            cx,
        );
    }

    // ───────────── 数据验证 ─────────────

    fn open_dv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let a = self.sel_used();
        let existing = self.sx().dv_at(a.row, a.column).cloned();
        let (ty, op, av, bv) = match existing.as_ref().map(|d| &d.rule) {
            Some(Rule::Check) => (1, 0, String::new(), String::new()),
            Some(Rule::Date { .. }) => (2, 0, String::new(), String::new()),
            Some(Rule::Num { op, a, b }) => (3, DV_OPS.iter().position(|o| o.0 == op).unwrap_or(0), funcs::fmt_num(*a), funcs::fmt_num(*b)),
            Some(Rule::Len { op, a, b }) => (4, DV_OPS.iter().position(|o| o.0 == op).unwrap_or(0), funcs::fmt_num(*a), funcs::fmt_num(*b)),
            _ => (3, 0, String::new(), String::new()),
        };
        let range = self.small_input(&range_text(&a), "应用范围", window, cx);
        let ai = self.small_input(&av, "最小值", window, cx);
        let bi = self.small_input(&bv, "最大值", window, cx);
        let hint = self.small_input(&existing.as_ref().map(|d| d.hint.clone()).unwrap_or_default(), "输入不合法时的提示（可选）", window, cx);
        self.open_data(DataDlg::Dv { range, ty, op, a: ai, b: bi, reject: existing.as_ref().is_some_and(|d| d.reject), hint, open: 0 }, cx);
    }

    // ───────────── 条件格式规则 ─────────────

    pub(crate) fn open_cf_dlg(&mut self, index: Option<u32>, formula: bool, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.model.borrow().get_conditional_formatting_list(self.sheet).unwrap_or_default();
        let cur = index.and_then(|i| list.iter().find(|c| c.index as u32 == i).cloned());
        let (mut ty, mut av, mut bv) = (0, String::new(), String::new());
        let range_s = cur.as_ref().map(|c| c.range.clone()).unwrap_or_else(|| range_text(&self.sel_used()));
        if let Some(c) = &cur {
            let key = |k: &str| CF_TYPES.iter().position(|t| t.0 == k).unwrap_or(0);
            let unq = |s: &str| s.trim_matches('"').to_string();
            (ty, av, bv) = match &c.cf_rule {
                CfRule::CellIs { operator, formula, formula2, .. } => (
                    key(match operator {
                        ValueOperator::GreaterThan => "gt",
                        ValueOperator::LessThan => "lt",
                        ValueOperator::GreaterThanOrEqual => "ge",
                        ValueOperator::LessThanOrEqual => "le",
                        ValueOperator::Equal => "eq",
                        ValueOperator::NotEqual => "ne",
                        ValueOperator::Between => "between",
                        ValueOperator::NotBetween => "notbetween",
                    }),
                    unq(formula),
                    formula2.as_deref().map(unq).unwrap_or_default(),
                ),
                CfRule::Text { operator, value, .. } => (
                    key(match operator {
                        TextOperator::DoesNotContain => "notcontains",
                        TextOperator::BeginsWith => "begins",
                        TextOperator::EndsWith => "ends",
                        _ => "contains",
                    }),
                    value.clone(),
                    String::new(),
                ),
                CfRule::TimePeriod { time_period, .. } => (
                    key(match time_period {
                        PeriodType::Yesterday => "date=yesterday",
                        PeriodType::Tomorrow => "date=tomorrow",
                        PeriodType::Last7Days => "date=last7",
                        PeriodType::ThisWeek => "date=thisweek",
                        PeriodType::ThisMonth => "date=thismonth",
                        _ => "date=today",
                    }),
                    String::new(),
                    String::new(),
                ),
                CfRule::Formula { formula, .. } => (key("formula"), format!("={formula}"), String::new()),
                CfRule::DuplicateValues { .. } => (key("dup"), String::new(), String::new()),
                CfRule::UniqueValues { .. } => (key("unique"), String::new(), String::new()),
                CfRule::Blanks { .. } => (key("blank"), String::new(), String::new()),
                CfRule::NotBlanks { .. } => (key("notblank"), String::new(), String::new()),
                CfRule::AboveAverage { .. } => (key("above"), String::new(), String::new()),
                _ => {
                    self.toast = Some("这类规则（色阶 / 数据条 / 图标集）请删除后重新选择预设".into());
                    cx.notify();
                    return;
                }
            };
        } else if formula {
            ty = CF_TYPES.len() - 1;
            av = "=".into();
        }
        let dxf = index.and_then(|i| self.model.borrow().get_dxf_for_conditional_formatting(self.sheet, i).ok().flatten());
        let (fill, font, bold, italic, strike) = match &dxf {
            Some(d) => (
                d.fill.as_ref().map(|f| hex_of(&f.color)).unwrap_or_default(),
                d.font.as_ref().map(|f| hex_of(&f.color)).unwrap_or_default(),
                d.font.as_ref().and_then(|f| f.b).unwrap_or(false),
                d.font.as_ref().and_then(|f| f.i).unwrap_or(false),
                d.font.as_ref().and_then(|f| f.strike).unwrap_or(false),
            ),
            None => ("#ffc7ce".into(), "#9c0006".into(), false, false, false),
        };
        let range = self.small_input(&range_s, "应用范围", window, cx);
        let a = self.small_input(&av, "值", window, cx);
        let b = self.small_input(&bv, "值", window, cx);
        window.focus(&a.focus_handle(cx));
        self.open_data(DataDlg::Cf { index, range, ty, a, b, fill, font, bold, italic, strike, open: false }, cx);
    }

    // ───────────── 确认 ─────────────

    pub(crate) fn data_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(super::dialogs::Dlg::Data(d)) = self.dlg.take() else { return };
        self.dlg_subs.clear();
        match *d {
            DataDlg::Cf { index, range, ty, a, b, fill, font, bold, italic, strike, .. } => {
                let (key, _, nv) = CF_TYPES[ty];
                let (av, bv) = (a.read(cx).text(), b.read(cx).text());
                let arg = if nv == 2 { format!("{av},{bv}") } else { av.clone() };
                let (rule, v) = key.split_once('=').unwrap_or((key, arg.as_str()));
                let Some(mut r) = Self::cf_rule(rule, v) else { return };
                let dxf = Dxf {
                    font: Some(DxfFont {
                        color: if font.is_empty() { Color::None } else { Color::Rgb(font) },
                        b: bold.then_some(true),
                        i: italic.then_some(true),
                        strike: strike.then_some(true),
                        ..Default::default()
                    }),
                    fill: (!fill.is_empty()).then_some(Fill { color: Color::Rgb(fill) }),
                    ..Default::default()
                };
                set_format(&mut r, dxf);
                let range = range.read(cx).text().replace('$', "").trim().to_uppercase();
                let res = match index {
                    Some(i) => self.model.borrow_mut().update_conditional_formatting(self.sheet, i, &range, r),
                    None => self.model.borrow_mut().add_conditional_formatting(self.sheet, &range, r),
                };
                self.toast = Some(match res {
                    Ok(_) => {
                        if index.is_some() {
                            "已更新规则".into()
                        } else {
                            format!("已为 {range} 添加条件格式").into()
                        }
                    }
                    Err(e) => format!("条件格式失败：{e}").into(),
                });
                self.changed(cx);
            }
            DataDlg::Dv { range, ty, op, a, b, reject, hint, .. } => {
                let Some((r0, c0, r1, c1)) = funcs::parse_range(&range.read(cx).text().replace('$', "")) else { return };
                if ty == 0 {
                    // 下拉列表走专门的设置框
                    self.anchor = Cell { r: r0, c: c0 };
                    self.cur = Cell { r: r1, c: c1 };
                    self.open_list_dlg(window, cx);
                    return;
                }
                let num = |e: &Entity<LineInput>| e.read(cx).text().trim().parse::<f64>().unwrap_or(0.);
                let rule = match ty {
                    1 => Rule::Check,
                    2 => Rule::Date { time: false },
                    3 => Rule::Num { op: DV_OPS[op].0.into(), a: num(&a), b: num(&b) },
                    _ => Rule::Len { op: DV_OPS[op].0.into(), a: num(&a), b: num(&b) },
                };
                let hint = hint.read(cx).text();
                let dv = ext::Dv { r0, c0, r1, c1, rule, reject, hint };
                self.update_sx(
                    |x| {
                        x.clear_dv(r0, c0, r1, c1);
                        x.dv.push(dv);
                    },
                    cx,
                );
                self.toast = Some("已设置数据验证".into());
            }
            DataDlg::Sort { area, header, keys, .. } => self.sort_by(area, header, &keys, cx),
            DataDlg::Split { area, sep, custom } => {
                let sep = match sep {
                    0 => ",".to_string(),
                    1 => " ".into(),
                    2 => ";".into(),
                    3 => "\t".into(),
                    _ => custom.read(cx).text(),
                };
                self.split_col(area, &sep, cx);
            }
            DataDlg::Dedupe { area, header, cols } => self.dedupe(area, header, &cols.iter().filter(|c| c.1).map(|c| c.0).collect::<Vec<_>>(), cx),
            DataDlg::Clean { items } => {
                let fixes: Vec<(i32, i32, String)> = items.into_iter().filter(|i| i.2).flat_map(|i| i.1).collect();
                let n = fixes.len();
                self.batch(|this| {
                    let mut m = this.model.borrow_mut();
                    for (r, c, v) in fixes {
                        m.set_user_input(this.sheet, r, c, &v).ok();
                    }
                });
                self.toast = Some(format!("已清洗 {n} 个单元格").into());
                self.changed(cx);
            }
            DataDlg::XRef { list, sel, range } => {
                let Some((id, _)) = sel.and_then(|i| list.get(i).cloned()) else { return };
                let rng = range.read(cx).text().trim().replace('$', "").to_uppercase();
                let at = self.cur;
                self.update_sx(|x| x.xrefs.push(ext::XRef { r: at.r, c: at.c, obj: id, range: rng }), cx);
                self.xref_refresh(cx);
            }
        }
        window.focus(&self.focus);
        cx.notify();
    }

    // ───────────── 排序 / 分列 / 去重 / 清洗 ─────────────

    /// 多条件排序：整行一起移动（包括格式），一次撤销。
    pub(crate) fn sort_by(&mut self, area: (i32, i32, i32, i32), header: bool, keys: &[(i32, bool)], cx: &mut Context<Self>) {
        let (r0, c0, r1, c1) = area;
        let r0 = if header { r0 + 1 } else { r0 };
        if r1 <= r0 || keys.is_empty() {
            return;
        }
        let rows: Vec<(Vec<String>, Vec<ironcalc::base::types::Style>, Vec<(String, Option<f64>)>)> = {
            let m = self.model.borrow();
            (r0..=r1)
                .map(|r| {
                    let contents = (c0..=c1).map(|c| m.get_cell_content(self.sheet, r, c).unwrap_or_default()).collect();
                    let styles = (c0..=c1).map(|c| m.get_model().get_style_for_cell(self.sheet, r, c).unwrap_or_default()).collect();
                    let ks = keys
                        .iter()
                        .map(|(k, _)| {
                            let shown = m.get_formatted_cell_value(self.sheet, r, *k).unwrap_or_default();
                            let num = match m.get_model().get_cell_value_by_index(self.sheet, r, *k) {
                                Ok(ironcalc::base::cell::CellValue::Number(v)) => Some(v),
                                _ => None,
                            };
                            (shown, num)
                        })
                        .collect();
                    (contents, styles, ks)
                })
                .collect()
        };
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|&x, &y| {
            for (ki, (_, asc)) in keys.iter().enumerate() {
                let (a, b) = (&rows[x].2[ki], &rows[y].2[ki]);
                let o = match (a.0.is_empty(), b.0.is_empty()) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => return std::cmp::Ordering::Greater,
                    (false, true) => return std::cmp::Ordering::Less,
                    _ => match (a.1, b.1) {
                        (Some(p), Some(q)) => p.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        _ => a.0.cmp(&b.0),
                    },
                };
                let o = if *asc { o } else { o.reverse() };
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
        let styles: Vec<Vec<ironcalc::base::types::Style>> = order.iter().map(|&i| rows[i].1.clone()).collect();
        // 单元格对象随行移动
        let moves: Vec<(i32, i32)> = order.iter().enumerate().map(|(new, &old)| (r0 + old as i32, r0 + new as i32)).collect();
        self.batch(|this| {
            {
                let mut m = this.model.borrow_mut();
                m.pause_evaluation();
                for (i, &o) in order.iter().enumerate() {
                    for (j, v) in rows[o].0.iter().enumerate() {
                        m.set_user_input(this.sheet, r0 + i as i32, c0 + j as i32, v).ok();
                    }
                }
                m.resume_evaluation();
                m.evaluate();
            }
            this.paste_styles(&super::area(this.sheet, r0, c0, r1, c1), &styles);
            this.edit_sx(|x| {
                for o in x.cells.iter_mut().filter(|o| o.r >= r0 && o.r <= r1 && o.c >= c0 && o.c <= c1) {
                    if let Some((_, to)) = moves.iter().find(|(from, _)| *from == o.r) {
                        o.r = *to;
                    }
                }
            });
        });
        self.apply_filter();
        self.toast = Some(format!("已按 {} 个条件排序 {} 行", keys.len(), r1 - r0 + 1).into());
        self.changed(cx);
    }

    fn split_col(&mut self, area: (i32, i32, i32, i32), sep: &str, cx: &mut Context<Self>) {
        if sep.is_empty() {
            return;
        }
        let (r0, c, r1, _) = area;
        let parts: Vec<(i32, Vec<String>)> = {
            let m = self.model.borrow();
            (r0..=r1)
                .map(|r| {
                    let v = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
                    let v = if sep == "," { v.replace('，', ",") } else { v };
                    (r, v.split(sep).map(|s| s.trim().to_string()).collect())
                })
                .collect()
        };
        let maxn = parts.iter().map(|p| p.1.len()).max().unwrap_or(1);
        self.batch(|this| {
            let mut m = this.model.borrow_mut();
            for (r, ps) in &parts {
                if ps.len() <= 1 {
                    continue;
                }
                for (k, v) in ps.iter().enumerate() {
                    m.set_user_input(this.sheet, *r, c + k as i32, v).ok();
                }
            }
        });
        self.toast = Some(format!("已分成 {maxn} 列").into());
        self.changed(cx);
    }

    fn dedupe(&mut self, area: (i32, i32, i32, i32), header: bool, cols: &[i32], cx: &mut Context<Self>) {
        let (r0, c0, r1, c1) = area;
        let r0 = if header { r0 + 1 } else { r0 };
        if cols.is_empty() || r1 < r0 {
            return;
        }
        let (rows, styles): (Vec<Vec<String>>, Vec<Vec<ironcalc::base::types::Style>>) = {
            let m = self.model.borrow();
            let mut seen = std::collections::HashSet::new();
            let mut keep = vec![];
            let mut st = vec![];
            for r in r0..=r1 {
                let key: Vec<String> = cols.iter().map(|c| m.get_formatted_cell_value(self.sheet, r, *c).unwrap_or_default()).collect();
                if seen.insert(key) {
                    keep.push((c0..=c1).map(|c| m.get_cell_content(self.sheet, r, c).unwrap_or_default()).collect());
                    st.push((c0..=c1).map(|c| m.get_model().get_style_for_cell(self.sheet, r, c).unwrap_or_default()).collect());
                }
            }
            (keep, st)
        };
        let removed = (r1 - r0 + 1) as usize - rows.len();
        if removed == 0 {
            self.toast = Some("没有发现重复项".into());
            cx.notify();
            return;
        }
        self.batch(|this| {
            {
                let mut m = this.model.borrow_mut();
                m.range_clear_all(&super::area(this.sheet, r0, c0, r1, c1)).ok();
                for (i, row) in rows.iter().enumerate() {
                    for (j, v) in row.iter().enumerate() {
                        m.set_user_input(this.sheet, r0 + i as i32, c0 + j as i32, v).ok();
                    }
                }
            }
            this.paste_styles(&super::area(this.sheet, r0, c0, r0 + rows.len() as i32 - 1, c1), &styles);
        });
        self.toast = Some(format!("已删除 {removed} 行重复项，保留 {} 行", rows.len()).into());
        self.changed(cx);
    }

    /// 清洗建议：扫描数据区，给出可一键修复的问题。
    fn clean_scan(&self) -> Vec<(String, Vec<(i32, i32, String)>, bool)> {
        let (r0, c0, r1, c1) = {
            let a = self.sel_area();
            if self.kind == SelKind::Cells && (a.width > 1 || a.height > 1) {
                (a.row, a.column, a.row + a.height - 1, a.column + a.width - 1)
            } else {
                let (ur, uc) = self.used();
                (1, 1, ur, uc)
            }
        };
        let m = self.model.borrow();
        let (mut trim, mut spaces, mut full, mut textnum) = (vec![], vec![], vec![], vec![]);
        let half = |s: &str| {
            s.chars()
                .map(|ch| match ch {
                    '\u{FF01}'..='\u{FF5E}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
                    '\u{3000}' => ' ',
                    _ => ch,
                })
                .collect::<String>()
        };
        for r in r0..=r1.min(r0 + 20000) {
            for c in c0..=c1.min(c0 + 200) {
                let v = m.get_cell_content(self.sheet, r, c).unwrap_or_default();
                if v.is_empty() || v.starts_with('=') {
                    continue;
                }
                if v.trim() != v {
                    trim.push((r, c, v.trim().to_string()));
                } else if v.contains("  ") {
                    let mut out = String::new();
                    for w in v.split(' ').filter(|w| !w.is_empty()) {
                        if !out.is_empty() {
                            out.push(' ');
                        }
                        out.push_str(w);
                    }
                    spaces.push((r, c, out));
                } else if v.chars().any(|ch| ('\u{FF10}'..='\u{FF5A}').contains(&ch) || ch == '\u{3000}')
                    && !v.chars().any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
                {
                    full.push((r, c, half(&v)));
                } else if matches!(m.get_cell_type(self.sheet, r, c), Ok(CellType::Text)) && v.trim_start_matches('\'').replace(',', "").parse::<f64>().is_ok()
                {
                    textnum.push((r, c, v.trim_start_matches('\'').replace(',', "")));
                }
            }
        }
        let mut out = vec![];
        for (name, list) in [("去掉首尾空格", trim), ("合并连续空格", spaces), ("全角字符转半角", full), ("文本型数字转为数字", textnum)]
        {
            if !list.is_empty() {
                out.push((format!("{name}（{} 个单元格）", list.len()), list, true));
            }
        }
        out
    }

    // ───────────── 跨表引用 ─────────────

    /// 本机其他电子表格：(对象 id, 所在文档标题)
    fn sheet_list(&self) -> Vec<(String, String)> {
        let Some(store) = crate::objects::store() else { return vec![] };
        let ids: Vec<String> = store.with_conn(|c| {
            let Ok(mut st) = c.prepare("SELECT id FROM objects WHERE kind = 'sheet' ORDER BY updated DESC") else { return vec![] };
            st.query_map([], |r| r.get(0)).map(|it| it.filter_map(|x| x.ok()).collect()).unwrap_or_default()
        });
        ids.into_iter()
            .filter(|id| *id != self.obj_id)
            .map(|id| {
                let title: String = store.with_conn(|c| {
                    c.query_row("SELECT title FROM docs WHERE content LIKE ?1 AND deleted = 0 LIMIT 1", [format!("%{id}%")], |r| r.get(0)).unwrap_or_default()
                });
                let preview = crate::objects::sheet(&id)
                    .map(|s| crate::objects::sheet_values(&s, 1, 4).first().map(|r| r.join(" · ")).unwrap_or_default())
                    .unwrap_or_default();
                let label = format!(
                    "{}{}",
                    if title.is_empty() { "未命名文档".to_string() } else { title },
                    if preview.is_empty() { String::new() } else { format!("　{preview}") }
                );
                (id, label)
            })
            .collect()
    }

    /// 重新拉取所有跨表引用的数据（只读快照）。
    pub(crate) fn xref_refresh(&mut self, cx: &mut Context<Self>) {
        let refs = self.sx().xrefs.clone();
        if refs.is_empty() {
            self.toast = Some("当前工作表没有跨表引用".into());
            cx.notify();
            return;
        }
        let mut cells = 0;
        self.batch(|this| {
            for x in &refs {
                let Some(src) = crate::objects::sheet(&x.obj) else { continue };
                let vals = {
                    let s = src.borrow();
                    let (r0, c0, r1, c1) = funcs::parse_range(&x.range).unwrap_or_else(|| {
                        let d = s.get_model().workbook.worksheet(0).map(|w| w.dimension()).ok();
                        (1, 1, d.as_ref().map(|d| d.max_row).unwrap_or(1), d.as_ref().map(|d| d.max_column).unwrap_or(1))
                    });
                    (r0..=r1.min(r0 + 5000))
                        .map(|r| (c0..=c1.min(c0 + 200)).map(|c| s.get_formatted_cell_value(0, r, c).unwrap_or_default()).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                };
                let mut m = this.model.borrow_mut();
                for (i, row) in vals.iter().enumerate() {
                    for (j, v) in row.iter().enumerate() {
                        m.set_user_input(this.sheet, x.r + i as i32, x.c + j as i32, v).ok();
                        cells += 1;
                    }
                }
            }
        });
        self.toast = Some(format!("已更新跨表引用数据（{cells} 个单元格）").into());
        self.changed(cx);
    }

    // ───────────── 界面 ─────────────

    pub(crate) fn data_dlg_body(&mut self, t: &Theme, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some(super::dialogs::Dlg::Data(d)) = &self.dlg else { return None };
        let footer = |ok: &'static str, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap(px(8.))
                .child(ghost_btn("dd-c", "取消", t).on_click(cx.listener(|this, _, w, cx| this.close_dlg(w, cx))))
                .child(primary_btn("dd-ok", ok, t).on_click(cx.listener(|this, _, w, cx| this.data_confirm(w, cx))))
        };
        let label = |s: &'static str| div().w(px(76.)).flex_none().text_color(t.text_secondary).child(s);
        let row = || div().flex().flex_row().items_center().gap(px(8.));
        let chk = |id: &'static str, label: &'static str, on: bool, f: fn(&mut DataDlg), cx: &mut Context<Self>| {
            div().id(id).flex().flex_row().items_center().gap(px(6.)).cursor_pointer().child(check_box(on, t)).child(label).on_click(cx.listener(
                move |this, _, _, cx| {
                    if let Some(d) = this.data_mut() {
                        f(d);
                    }
                    cx.notify();
                },
            ))
        };
        Some(match d.as_ref() {
            DataDlg::Cf { index, range, ty, a, b, fill, font, bold, italic, strike, open } => {
                let (ty, open, nv) = (*ty, *open, CF_TYPES[*ty].2);
                let (fill, font, bold, italic, strike) = (fill.clone(), font.clone(), *bold, *italic, *strike);
                let swatch = |id: String, hex: &'static str, on: bool, is_font: bool, cx: &mut Context<Self>| {
                    div()
                        .id(SharedString::from(id))
                        .size(px(22.))
                        .rounded(px(4.))
                        .border_2()
                        .border_color(if on { t.accent } else { t.border })
                        .when(!hex.is_empty() && !is_font, |d| d.bg(hex_to_hsla(hex).unwrap()))
                        .when(is_font, |d| d.flex().items_center().justify_center().text_color(hex_to_hsla(hex).unwrap_or(t.text)).child("A"))
                        .when(hex.is_empty() && !is_font, |d| {
                            d.child(div().text_size(px(10.)).text_color(t.text_muted).child("无")).flex().items_center().justify_center()
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(DataDlg::Cf { fill, font, .. }) = this.data_mut() {
                                if is_font {
                                    *font = hex.to_string();
                                } else {
                                    *fill = hex.to_string();
                                }
                            }
                            cx.notify();
                        }))
                };
                Self::dlg_frame2(if index.is_some() { "编辑条件格式规则" } else { "新建条件格式规则" }, 460., t, cx)
                    .child(row().child(label("应用范围")).child(input_box(t).flex_1().child(range.clone())))
                    .child(
                        row().items_start().child(label("条件").mt(px(6.))).child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(crate::base::widgets::select_box("cf-ty", CF_TYPES[ty].1, None, t).on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(DataDlg::Cf { open, .. }) = this.data_mut() {
                                        *open = !*open;
                                    }
                                    cx.notify();
                                })))
                                .when(open, |d| {
                                    d.child(div().id("cf-tys").max_h(px(200.)).overflow_y_scroll().border_1().border_color(t.border).rounded(px(6.)).children(
                                        CF_TYPES.iter().enumerate().map(|(i, (_, name, _))| {
                                            div()
                                                .id(("cft", i))
                                                .px(px(10.))
                                                .h(px(28.))
                                                .flex()
                                                .items_center()
                                                .cursor_pointer()
                                                .hover(|s| s.bg(t.hover))
                                                .when(i == ty, |d| d.text_color(t.accent))
                                                .child(*name)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if let Some(DataDlg::Cf { ty, open, .. }) = this.data_mut() {
                                                        *ty = i;
                                                        *open = false;
                                                    }
                                                    cx.notify();
                                                }))
                                        }),
                                    ))
                                })
                                .when(nv >= 1, |d| {
                                    d.child(
                                        row()
                                            .child(input_box(t).flex_1().child(a.clone()))
                                            .when(nv == 2, |d| d.child("和").child(input_box(t).flex_1().child(b.clone()))),
                                    )
                                }),
                        ),
                    )
                    .child(
                        row()
                            .child(label("填充颜色"))
                            .children(FMT_FILLS.iter().enumerate().map(|(i, h)| swatch(format!("cff-{i}"), h, fill == *h, false, cx))),
                    )
                    .child(
                        row().child(label("文字颜色")).children(FMT_FONTS.iter().enumerate().map(|(i, h)| swatch(format!("cfc-{i}"), h, font == *h, true, cx))),
                    )
                    .child(
                        row()
                            .child(label("字形"))
                            .child(chk(
                                "cf-b",
                                "加粗",
                                bold,
                                |d| {
                                    if let DataDlg::Cf { bold, .. } = d {
                                        *bold = !*bold
                                    }
                                },
                                cx,
                            ))
                            .child(chk(
                                "cf-i",
                                "斜体",
                                italic,
                                |d| {
                                    if let DataDlg::Cf { italic, .. } = d {
                                        *italic = !*italic
                                    }
                                },
                                cx,
                            ))
                            .child(chk(
                                "cf-s",
                                "删除线",
                                strike,
                                |d| {
                                    if let DataDlg::Cf { strike, .. } = d {
                                        *strike = !*strike
                                    }
                                },
                                cx,
                            )),
                    )
                    .child(
                        row().child(label("预览")).child(
                            div()
                                .px(px(12.))
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .border_1()
                                .border_color(t.border)
                                .bg(hex_to_hsla(&fill).unwrap_or(t.bg))
                                .text_color(hex_to_hsla(&font).unwrap_or(t.text))
                                .when(bold, |d| d.font_weight(FontWeight::BOLD))
                                .when(italic, |d| d.italic())
                                .when(strike, |d| d.line_through())
                                .child("示例文字 123"),
                        ),
                    )
                    .child(footer("确定", cx))
                    .into_any_element()
            }
            DataDlg::Dv { range, ty, op, a, b, reject, hint, open } => {
                let (ty, op, reject, open) = (*ty, *op, *reject, *open);
                let picker = |id: &'static str, items: Vec<&'static str>, cur: usize, which: u8, cx: &mut Context<Self>| {
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(crate::base::widgets::select_box(id, items[cur], None, t).on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(DataDlg::Dv { open, .. }) = this.data_mut() {
                                *open = if *open == which { 0 } else { which };
                            }
                            cx.notify();
                        })))
                        .when(open == which, |d| {
                            d.child(div().border_1().border_color(t.border).rounded(px(6.)).children(items.iter().enumerate().map(|(i, name)| {
                                div()
                                    .id((id, i))
                                    .px(px(10.))
                                    .h(px(28.))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(t.hover))
                                    .when(i == cur, |d| d.text_color(t.accent))
                                    .child(*name)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(DataDlg::Dv { ty, op, open, .. }) = this.data_mut() {
                                            if which == 1 {
                                                *ty = i;
                                            } else {
                                                *op = i;
                                            }
                                            *open = 0;
                                        }
                                        cx.notify();
                                    }))
                            })))
                        })
                };
                let radio = |id: &'static str, label: &'static str, on: bool, v: bool, cx: &mut Context<Self>| {
                    div()
                        .id(id)
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .cursor_pointer()
                        .child(
                            div()
                                .size(px(16.))
                                .rounded_full()
                                .border_1()
                                .border_color(if on { t.accent } else { t.border_strong })
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(on, |d| d.child(div().size(px(8.)).rounded_full().bg(t.accent))),
                        )
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(DataDlg::Dv { reject, .. }) = this.data_mut() {
                                *reject = v;
                            }
                            cx.notify();
                        }))
                };
                let need_b = matches!(DV_OPS[op].0, "between" | "notBetween");
                Self::dlg_frame2("数据验证", 460., t, cx)
                    .child(row().child(label("应用范围")).child(input_box(t).flex_1().child(range.clone())))
                    .child(row().items_start().child(label("验证条件").mt(px(6.))).child(picker("dv-ty", DV_TYPES.to_vec(), ty, 1, cx)))
                    .when(ty >= 3, |d| {
                        d.child(row().items_start().child(label("").mt(px(6.))).child(picker("dv-op", DV_OPS.iter().map(|o| o.1).collect(), op, 2, cx))).child(
                            row()
                                .child(label(""))
                                .child(input_box(t).flex_1().child(a.clone()))
                                .when(need_b, |d| d.child("和").child(input_box(t).flex_1().child(b.clone()))),
                        )
                    })
                    .when(ty == 0, |d| d.child(row().child(label("")).child(div().text_color(t.text_muted).child("点「确定」后设置下拉选项"))))
                    .child(row().child(label("非法输入时")).child(radio("dv-warn", "显示警告", !reject, false, cx)).child(radio(
                        "dv-rej",
                        "拒绝输入",
                        reject,
                        true,
                        cx,
                    )))
                    .child(row().child(label("提示文字")).child(input_box(t).flex_1().child(hint.clone())))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .child(ghost_btn("dv-del", "删除验证", t).on_click(cx.listener(|this, _, w, cx| {
                                this.close_dlg(w, cx);
                                this.run("del-dv", w, cx);
                            })))
                            .child(div().flex_1())
                            .child(footer("确定", cx)),
                    )
                    .into_any_element()
            }
            DataDlg::Sort { area, header, keys, open } => {
                let (area, header, keys, open) = (*area, *header, keys.clone(), *open);
                let col_label = |c: i32| {
                    let h = if header { self.model.borrow().get_formatted_cell_value(self.sheet, area.0, c).unwrap_or_default() } else { String::new() };
                    if h.is_empty() {
                        format!("{} 列", col_name(c))
                    } else {
                        format!("{} 列（{h}）", col_name(c))
                    }
                };
                let cols: Vec<(i32, String)> = (area.1..=area.3).map(|c| (c, col_label(c))).collect();
                Self::dlg_frame2("自定义排序", 480., t, cx)
                    .child(
                        div().text_color(t.text_secondary).child(format!("排序范围：{}", range_text(&super::area(self.sheet, area.0, area.1, area.2, area.3)))),
                    )
                    .child(chk(
                        "st-h",
                        "数据包含标题行",
                        header,
                        |d| {
                            if let DataDlg::Sort { header, .. } = d {
                                *header = !*header
                            }
                        },
                        cx,
                    ))
                    .children({
                        let mut out = vec![];
                        for (i, (c, asc)) in keys.iter().enumerate() {
                            let (c, asc) = (*c, *asc);
                            let mut d = div().flex().flex_col().gap(px(4.)).child(
                                row()
                                    .child(div().w(px(56.)).text_color(t.text_secondary).child(if i == 0 { "主要条件" } else { "次要条件" }))
                                    .child(
                                        crate::base::widgets::select_box(
                                            ("st-c", i),
                                            cols.iter().find(|x| x.0 == c).map(|x| x.1.clone()).unwrap_or_default(),
                                            None,
                                            t,
                                        )
                                        .flex_1()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(DataDlg::Sort { open, .. }) = this.data_mut() {
                                                *open = if *open == Some(i) { None } else { Some(i) };
                                            }
                                            cx.notify();
                                        })),
                                    )
                                    .child(
                                        div()
                                            .id(("st-o", i))
                                            .px(px(10.))
                                            .h(px(30.))
                                            .flex()
                                            .items_center()
                                            .rounded(px(6.))
                                            .border_1()
                                            .border_color(t.border_strong)
                                            .cursor_pointer()
                                            .child(if asc { "升序 A→Z" } else { "降序 Z→A" })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(DataDlg::Sort { keys, .. }) = this.data_mut() {
                                                    keys[i].1 = !keys[i].1;
                                                }
                                                cx.notify();
                                            })),
                                    )
                                    .child(Self::ib(("st-d", i), "trash", "删除条件", false, t).on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(DataDlg::Sort { keys, .. }) = this.data_mut() {
                                            if keys.len() > 1 {
                                                keys.remove(i);
                                            }
                                        }
                                        cx.notify();
                                    }))),
                            );
                            if open == Some(i) {
                                let opts: Vec<_> = cols
                                    .iter()
                                    .map(|(cc, name)| {
                                        let cc = *cc;
                                        div()
                                            .id(SharedString::from(format!("st-co{i}-{cc}")))
                                            .px(px(10.))
                                            .h(px(28.))
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(t.hover))
                                            .child(name.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(DataDlg::Sort { keys, open, .. }) = this.data_mut() {
                                                    keys[i].0 = cc;
                                                    *open = None;
                                                }
                                                cx.notify();
                                            }))
                                    })
                                    .collect();
                                d = d.child(div().ml(px(64.)).border_1().border_color(t.border).rounded(px(6.)).children(opts));
                            }
                            out.push(d);
                        }
                        out
                    })
                    .child(
                        div()
                            .id("st-add")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .text_color(t.accent)
                            .cursor_pointer()
                            .child(icon("plus", 14., t.accent))
                            .child("添加条件")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(DataDlg::Sort { keys, area, .. }) = this.data_mut() {
                                    let next = (area.1..=area.3).find(|c| !keys.iter().any(|k| k.0 == *c)).unwrap_or(area.1);
                                    keys.push((next, true));
                                }
                                cx.notify();
                            })),
                    )
                    .child(footer("排序", cx))
                    .into_any_element()
            }
            DataDlg::Split { area, sep, custom } => {
                let (area, sep) = (*area, *sep);
                let sample = self.model.borrow().get_formatted_cell_value(self.sheet, area.0, area.1).unwrap_or_default();
                let sep_s = match sep {
                    0 => ",".to_string(),
                    1 => " ".into(),
                    2 => ";".into(),
                    3 => "\t".into(),
                    _ => custom.read(cx).text(),
                };
                let preview: Vec<String> = if sep_s.is_empty() {
                    vec![sample.clone()]
                } else {
                    sample.replace('，', if sep == 0 { "," } else { "，" }).split(sep_s.as_str()).map(|s| s.trim().to_string()).collect()
                };
                let opt = |id: &'static str, label: &'static str, v: u8, cx: &mut Context<Self>| {
                    div()
                        .id(id)
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .cursor_pointer()
                        .child(
                            div()
                                .size(px(16.))
                                .rounded_full()
                                .border_1()
                                .border_color(if sep == v { t.accent } else { t.border_strong })
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(sep == v, |d| d.child(div().size(px(8.)).rounded_full().bg(t.accent))),
                        )
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(DataDlg::Split { sep, .. }) = this.data_mut() {
                                *sep = v;
                            }
                            cx.notify();
                        }))
                };
                Self::dlg_frame2("分列", 440., t, cx)
                    .child(div().text_color(t.text_secondary).child(format!("将 {} 列第 {}–{} 行按分隔符拆到右侧各列", col_name(area.1), area.0, area.2)))
                    .child(
                        row()
                            .gap(px(14.))
                            .child(opt("sp-0", "逗号", 0, cx))
                            .child(opt("sp-1", "空格", 1, cx))
                            .child(opt("sp-2", "分号", 2, cx))
                            .child(opt("sp-3", "制表符", 3, cx))
                            .child(opt("sp-4", "自定义", 4, cx)),
                    )
                    .when(sep == 4, |d| d.child(input_box(t).child(custom.clone())))
                    .child(div().text_color(t.text_secondary).child("预览（第一行）"))
                    .child(row().flex_wrap().gap(px(4.)).children(preview.into_iter().map(|p| {
                        div().px(px(8.)).h(px(24.)).flex().items_center().rounded(px(4.)).bg(t.sidebar_bg).border_1().border_color(t.border).child(p)
                    })))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("右侧已有内容会被覆盖"))
                    .child(footer("分列", cx))
                    .into_any_element()
            }
            DataDlg::Dedupe { area, header, cols } => {
                let (area, header, cols) = (*area, *header, cols.clone());
                Self::dlg_frame2("删除重复项", 420., t, cx)
                    .child(div().text_color(t.text_secondary).child(format!("范围：{}", range_text(&super::area(self.sheet, area.0, area.1, area.2, area.3)))))
                    .child(chk(
                        "dd-h",
                        "数据包含标题行",
                        header,
                        |d| {
                            if let DataDlg::Dedupe { header, .. } = d {
                                *header = !*header
                            }
                        },
                        cx,
                    ))
                    .child(div().text_color(t.text_secondary).child("按以下列判断是否重复："))
                    .child(div().id("dd-cols").max_h(px(220.)).overflow_y_scroll().flex().flex_col().gap(px(6.)).children(cols.into_iter().enumerate().map(
                        |(i, (c, on))| {
                            let name =
                                if header { self.model.borrow().get_formatted_cell_value(self.sheet, area.0, c).unwrap_or_default() } else { String::new() };
                            div()
                                .id(("dd-c", i))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .cursor_pointer()
                                .child(check_box(on, t))
                                .child(format!("{} 列{}", col_name(c), if name.is_empty() { String::new() } else { format!("（{name}）") }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(DataDlg::Dedupe { cols, .. }) = this.data_mut() {
                                        cols[i].1 = !cols[i].1;
                                    }
                                    cx.notify();
                                }))
                        },
                    )))
                    .child(footer("删除重复项", cx))
                    .into_any_element()
            }
            DataDlg::Clean { items } => {
                let items: Vec<(String, bool)> = items.iter().map(|i| (i.0.clone(), i.2)).collect();
                let empty = items.is_empty();
                Self::dlg_frame2("清洗建议", 420., t, cx)
                    .when(empty, |d| d.child(div().py(px(20.)).flex().justify_center().text_color(t.text_muted).child("数据很干净，没有发现需要清洗的内容")))
                    .children(items.into_iter().enumerate().map(|(i, (name, on))| {
                        div()
                            .id(("cl", i))
                            .p(px(10.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(t.border)
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .child(check_box(on, t))
                            .child(name)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(DataDlg::Clean { items }) = this.data_mut() {
                                    items[i].2 = !items[i].2;
                                }
                                cx.notify();
                            }))
                    }))
                    .child(footer(if empty { "好的" } else { "一键清洗" }, cx))
                    .into_any_element()
            }
            DataDlg::XRef { list, sel, range } => {
                let (list, sel) = (list.clone(), *sel);
                Self::dlg_frame2("新建跨表引用数据", 480., t, cx)
                    .child(
                        div().text_color(t.text_secondary).child("从本机其他电子表格引用数据，放到当前单元格开始的位置；之后可在「数据 › 跨表引用」里刷新。"),
                    )
                    .when(list.is_empty(), |d| d.child(div().py(px(16.)).flex().justify_center().text_color(t.text_muted).child("还没有其他电子表格")))
                    .child(div().id("xr-list").max_h(px(240.)).overflow_y_scroll().flex().flex_col().gap(px(4.)).children(list.into_iter().enumerate().map(
                        |(i, (_, label))| {
                            div()
                                .id(("xr", i))
                                .px(px(10.))
                                .h(px(32.))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(8.))
                                .rounded(px(6.))
                                .cursor_pointer()
                                .when(sel == Some(i), |d| d.bg(t.accent_soft).text_color(t.accent))
                                .hover(|s| s.bg(t.hover))
                                .child(icon("sheet", 14., t.text_secondary))
                                .child(div().overflow_hidden().whitespace_nowrap().child(label))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(DataDlg::XRef { sel, .. }) = this.data_mut() {
                                        *sel = Some(i);
                                    }
                                    cx.notify();
                                }))
                        },
                    )))
                    .child(row().child(label("引用范围")).child(input_box(t).flex_1().child(range.clone())))
                    .child(footer("引用", cx))
                    .into_any_element()
            }
        })
    }

    fn dlg_frame2(title: &'static str, w: f32, t: &Theme, cx: &mut Context<Self>) -> Div {
        Self::panel(t).w(px(w)).px(px(20.)).py(px(16.)).gap(px(12.)).child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::MEDIUM).child(title))
                .child(Self::ib("dd-x", "x", "关闭", false, t).on_click(cx.listener(|this, _, w, cx| this.close_dlg(w, cx)))),
        )
    }

    /// 右侧面板（条件格式 / 全部函数）。
    pub(crate) fn side_panel(&mut self, t: &Theme, _window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        match self.side? {
            super::dialogs::Side::Cf => Some(self.cf_panel(t, cx)),
            super::dialogs::Side::Funcs => Some(self.funcs_panel(t, cx)),
            _ => None,
        }
    }

    fn funcs_panel(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let q = self.func_q.read(cx).text().to_uppercase();
        let cat = ironcalc::base::function_catalog();
        let mut list: Vec<(String, &'static str)> = cat
            .into_iter()
            .filter(|(n, c)| q.is_empty() || n.contains(&q) || c.contains(q.as_str()) || funcs::find(n).is_some_and(|f| f.desc.contains(q.as_str())))
            .collect();
        list.sort();
        let n = list.len();
        div()
            .w(px(300.))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(t.border)
            .bg(t.bg)
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(14.))
            .py(px(12.))
            .text_size(px(13.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(div().flex_1().text_size(px(15.)).font_weight(FontWeight::MEDIUM).child(format!("全部函数（{n}）")))
                    .child(Self::ib("fn-x", "x", "关闭", false, t).on_click(cx.listener(|this, _, _, cx| {
                        this.side = None;
                        cx.notify();
                    }))),
            )
            .child(input_box(t).gap(px(6.)).child(icon("search", 14., t.text_muted)).child(div().flex_1().child(self.func_q.clone())))
            .child(div().id("fn-list").flex_1().overflow_y_scroll().flex().flex_col().children(list.into_iter().enumerate().map(|(i, (name, c))| {
                let f = funcs::find(&name);
                let nm = name.clone();
                div()
                    .id(("fn", i))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(6.))
                            .items_center()
                            .child(div().font_weight(FontWeight::MEDIUM).child(name.clone()))
                            .child(div().text_size(px(11.)).px(px(5.)).rounded(px(3.)).bg(t.sidebar_bg).text_color(t.text_muted).child(c)),
                    )
                    .when_some(f, |d, f| d.child(div().text_size(px(12.)).text_color(t.text_secondary).child(format!("{}({})　{}", f.name, f.args, f.desc))))
                    .on_click(cx.listener(move |this, _, w, cx| this.run(&format!("insert-fn:{nm}"), w, cx)))
            })))
    }
}

/// 替换规则里的格式（新建规则对话框选的样式）。
fn set_format(r: &mut CfRuleInput, dxf: Dxf) {
    match r {
        CfRuleInput::CellIs { format, .. }
        | CfRuleInput::Text { format, .. }
        | CfRuleInput::Formula { format, .. }
        | CfRuleInput::TimePeriod { format, .. }
        | CfRuleInput::DuplicateValues { format, .. }
        | CfRuleInput::UniqueValues { format, .. }
        | CfRuleInput::Blanks { format, .. }
        | CfRuleInput::NotBlanks { format, .. }
        | CfRuleInput::Errors { format, .. }
        | CfRuleInput::NoErrors { format, .. }
        | CfRuleInput::AboveAverage { format, .. }
        | CfRuleInput::BelowAverage { format, .. }
        | CfRuleInput::Top10 { format, .. }
        | CfRuleInput::Bottom10 { format, .. } => *format = dxf,
        _ => {}
    }
}
