//! 条件格式（「条件格式」完整版）：突出显示（大于 / 小于 / 介于 / 等于 / 文本包含 / 日期为 / 重复值 / 唯一值 / 空值）、
//! 最前 / 最后 N 项或 N%、高于 / 低于平均值、自定义公式、色阶（双色 8 种 + 三色 4 种）、数据条（纯色 / 渐变各 6 色）、
//! 图标集（方向 / 形状 / 标记 / 等级），以及右侧「管理规则」面板。计算全部交给 IronCalc。

use super::*;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use ironcalc::base::cf_types::{CfRule, CfRuleInput, Cfvo, ColorScaleThreshold, Icon, IconThreshold, PeriodType, TextOperator, ValueOperator};
use ironcalc::base::types::{Color, Dxf, DxfFont, Fill};

/// 双色色阶：(最小值颜色, 最大值颜色)；三色色阶：(最小, 中间, 最大)
const SCALES2: [(u32, u32); 8] = [
    (0xffffff, 0x63be7b),
    (0x63be7b, 0xffffff),
    (0xffffff, 0xf8696b),
    (0xf8696b, 0xffffff),
    (0xffffff, 0xffeb84),
    (0xffeb84, 0xffffff),
    (0xffeb84, 0x63be7b),
    (0x63be7b, 0xffeb84),
];
const SCALES3: [(u32, u32, u32); 4] =
    [(0xf8696b, 0xffffff, 0x63be7b), (0x63be7b, 0xffffff, 0xf8696b), (0xf8696b, 0xffeb84, 0x63be7b), (0x63be7b, 0xffeb84, 0xf8696b)];
/// 数据条颜色（蓝 绿 橙 / 黄 红 青）
const BARS: [u32; 6] = [0x638ec6, 0x63be7b, 0xffb628, 0xf2c94c, 0xf8696b, 0x4ac0e8];

/// 图标集预设：(分组, 名称, 图标从低到高, 颜色)
pub(crate) const ICON_SETS: [(&str, &str); 14] = [
    ("方向", "3Arrows"),
    ("方向", "3ArrowsGray"),
    ("方向", "3Triangles"),
    ("方向", "4Arrows"),
    ("方向", "4ArrowsGray"),
    ("方向", "5Arrows"),
    ("方向", "5ArrowsGray"),
    ("形状", "3TrafficLights1"),
    ("形状", "3Signs"),
    ("形状", "4TrafficLights"),
    ("形状", "4RedToBlack"),
    ("标记", "3Symbols"),
    ("标记", "3Flags"),
    ("标记", "2Thumbs"),
];
/// 等级（评分）：(图标, 颜色, 个数)
const RATINGS: [(Icon, u32, u32); 4] = [(Icon::Star, 0xf2b136, 3), (Icon::Heart, 0xf54a45, 3), (Icon::Circle, 0x646a73, 5), (Icon::Star, 0xf2b136, 5)];

fn rgbc(c: u32) -> Color {
    Color::Rgb(format!("#{c:06x}"))
}

fn icons_of(name: &str) -> Vec<(Icon, Color)> {
    if name == "2Thumbs" {
        return vec![(Icon::ThumbsDown, rgbc(0xf8696b)), (Icon::ThumbsUp, rgbc(0x63be7b))];
    }
    ironcalc::base::cf_types::icon_set_icons(name).unwrap_or_default()
}

/// 「突出显示」默认样式：浅红填充 + 深红文字
pub(crate) fn red_dxf() -> Dxf {
    Dxf {
        font: Some(DxfFont { color: Color::Rgb("#9c0006".into()), ..Default::default() }),
        fill: Some(Fill { color: Color::Rgb("#ffc7ce".into()) }),
        ..Default::default()
    }
}

/// 公式 / 规则里的值：数字原样，文字加引号
pub(crate) fn quote(s: &str) -> String {
    if s.parse::<f64>().is_ok() || s.starts_with('=') {
        s.trim_start_matches('=').to_string()
    } else {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
}

pub(crate) fn hex_of(c: &Color) -> String {
    match c {
        Color::Rgb(s) => s.clone(),
        _ => String::new(),
    }
}

/// 规则的中文描述（管理规则面板用）。
pub(crate) fn describe(r: &CfRule) -> String {
    let op = |o: &ValueOperator| match o {
        ValueOperator::Equal => "等于",
        ValueOperator::GreaterThan => "大于",
        ValueOperator::GreaterThanOrEqual => "大于或等于",
        ValueOperator::LessThan => "小于",
        ValueOperator::LessThanOrEqual => "小于或等于",
        ValueOperator::NotEqual => "不等于",
        ValueOperator::Between => "介于",
        ValueOperator::NotBetween => "不介于",
    };
    match r {
        CfRule::ColorScale { thresholds } => format!("{}色色阶", if thresholds.len() == 2 { "双" } else { "三" }),
        CfRule::CellIs { operator, formula, formula2, .. } => match formula2 {
            Some(f2) => format!("单元格值{} {} 和 {}", op(operator), formula, f2),
            None => format!("单元格值{} {}", op(operator), formula),
        },
        CfRule::Formula { formula, .. } => format!("自定义公式 ={formula}"),
        CfRule::Text { operator, value, .. } => format!(
            "文本{} \"{value}\"",
            match operator {
                TextOperator::Contains => "包含",
                TextOperator::DoesNotContain => "不包含",
                TextOperator::BeginsWith => "开头是",
                TextOperator::EndsWith => "结尾是",
                TextOperator::Equals => "等于",
            }
        ),
        CfRule::TimePeriod { time_period, .. } => format!("日期为{}", period_name(time_period)),
        CfRule::DuplicateValues { .. } => "重复值".into(),
        CfRule::UniqueValues { .. } => "唯一值".into(),
        CfRule::Blanks { .. } => "空值".into(),
        CfRule::NotBlanks { .. } => "非空值".into(),
        CfRule::Errors { .. } => "错误值".into(),
        CfRule::NoErrors { .. } => "无错误".into(),
        CfRule::AboveAverage { .. } => "高于平均值".into(),
        CfRule::BelowAverage { .. } => "低于平均值".into(),
        CfRule::Top10 { rank, percent, .. } => format!("前 {rank}{}", if *percent { "%" } else { " 项" }),
        CfRule::Bottom10 { rank, percent, .. } => format!("后 {rank}{}", if *percent { "%" } else { " 项" }),
        CfRule::DataBar { is_gradient, .. } => format!("数据条（{}）", if *is_gradient { "渐变" } else { "纯色" }),
        CfRule::IconSet { thresholds, .. } => format!("图标集（{} 个）", thresholds.len()),
        CfRule::IconRating { .. } => "等级".into(),
    }
}

fn period_name(p: &PeriodType) -> &'static str {
    match p {
        PeriodType::Yesterday => "昨天",
        PeriodType::Today => "今天",
        PeriodType::Tomorrow => "明天",
        PeriodType::Last7Days => "最近 7 天",
        PeriodType::Next7Days => "未来 7 天",
        PeriodType::LastWeek => "上周",
        PeriodType::ThisWeek => "本周",
        PeriodType::NextWeek => "下周",
        PeriodType::LastMonth => "上月",
        PeriodType::ThisMonth => "本月",
        PeriodType::NextMonth => "下月",
        PeriodType::LastYear => "去年",
        PeriodType::ThisYear => "今年",
        PeriodType::NextYear => "明年",
        PeriodType::Between => "介于",
        PeriodType::NotBetween => "不介于",
    }
}

pub(crate) fn period_of(s: &str) -> PeriodType {
    match s {
        "yesterday" => PeriodType::Yesterday,
        "tomorrow" => PeriodType::Tomorrow,
        "last7" => PeriodType::Last7Days,
        "lastweek" => PeriodType::LastWeek,
        "thisweek" => PeriodType::ThisWeek,
        "nextweek" => PeriodType::NextWeek,
        "lastmonth" => PeriodType::LastMonth,
        "thismonth" => PeriodType::ThisMonth,
        "nextmonth" => PeriodType::NextMonth,
        _ => PeriodType::Today,
    }
}

impl SheetEditor {
    /// 条件格式规则数：`sel` 时只算与选区相交的。
    pub(crate) fn cf_count(&self, sel: bool) -> usize {
        let list = self.model.borrow().get_conditional_formatting_list(self.sheet).unwrap_or_default();
        if !sel {
            return list.len();
        }
        let a = self.sel_area();
        list.iter().filter(|cf| Self::cf_hits(&cf.range, &a)).count()
    }

    fn cf_hits(range: &str, a: &ironcalc::base::expressions::types::Area) -> bool {
        range
            .split(' ')
            .any(|r| funcs::parse_range(r).is_some_and(|(r0, c0, r1, c1)| r0 < a.row + a.height && r1 >= a.row && c0 < a.column + a.width && c1 >= a.column))
    }

    /// 添加一条规则到选区。
    pub(crate) fn add_cf(&mut self, rule: CfRuleInput, cx: &mut Context<Self>) {
        let a = self.sel_used();
        let range = range_text(&a);
        let res = self.model.borrow_mut().add_conditional_formatting(self.sheet, &range, rule);
        self.toast = Some(match res {
            Ok(_) => format!("已为 {range} 添加条件格式").into(),
            Err(e) => format!("条件格式失败：{e}").into(),
        });
        self.changed(cx);
    }

    /// 由命令参数生成规则：`rule[=值]`
    pub(crate) fn cf_rule(rule: &str, arg: &str) -> Option<CfRuleInput> {
        let red = red_dxf;
        Some(match rule {
            "gt" | "lt" | "eq" | "ne" | "ge" | "le" => CfRuleInput::CellIs {
                operator: match rule {
                    "gt" => ValueOperator::GreaterThan,
                    "lt" => ValueOperator::LessThan,
                    "ge" => ValueOperator::GreaterThanOrEqual,
                    "le" => ValueOperator::LessThanOrEqual,
                    "ne" => ValueOperator::NotEqual,
                    _ => ValueOperator::Equal,
                },
                formula: quote(arg),
                formula2: None,
                format: red(),
                stop_if_true: false,
            },
            "between" | "notbetween" => {
                let (a, b) = arg.split_once([',', '，']).unwrap_or((arg, arg));
                CfRuleInput::CellIs {
                    operator: if rule == "between" { ValueOperator::Between } else { ValueOperator::NotBetween },
                    formula: quote(a.trim()),
                    formula2: Some(quote(b.trim())),
                    format: red(),
                    stop_if_true: false,
                }
            }
            "contains" | "notcontains" | "begins" | "ends" => CfRuleInput::Text {
                operator: match rule {
                    "notcontains" => TextOperator::DoesNotContain,
                    "begins" => TextOperator::BeginsWith,
                    "ends" => TextOperator::EndsWith,
                    _ => TextOperator::Contains,
                },
                value: arg.to_string(),
                format: red(),
                stop_if_true: false,
            },
            "date" => CfRuleInput::TimePeriod { time_period: period_of(arg), date1: None, date2: None, format: red(), stop_if_true: false },
            "formula" => CfRuleInput::Formula { formula: arg.trim_start_matches('=').to_string(), format: red(), stop_if_true: false },
            "dup" => CfRuleInput::DuplicateValues { format: red(), stop_if_true: false },
            "unique" => CfRuleInput::UniqueValues { format: red(), stop_if_true: false },
            "blank" => CfRuleInput::Blanks { format: red(), stop_if_true: false },
            "notblank" => CfRuleInput::NotBlanks { format: red(), stop_if_true: false },
            "above" => CfRuleInput::AboveAverage { format: red(), stop_if_true: false },
            "below" => CfRuleInput::BelowAverage { format: red(), stop_if_true: false },
            "top10" => CfRuleInput::Top10 { rank: 10, percent: false, format: red(), stop_if_true: false },
            "top" | "toppct" => CfRuleInput::Top10 { rank: arg.trim().parse().unwrap_or(10), percent: rule == "toppct", format: red(), stop_if_true: false },
            "bottom" | "bottompct" => {
                CfRuleInput::Bottom10 { rank: arg.trim().parse().unwrap_or(10), percent: rule == "bottompct", format: red(), stop_if_true: false }
            }
            "scale2" => {
                let (lo, hi) = SCALES2[arg.parse::<usize>().unwrap_or(0) % SCALES2.len()];
                CfRuleInput::ColorScale {
                    thresholds: vec![ColorScaleThreshold { cfvo: Cfvo::Min, color: rgbc(lo) }, ColorScaleThreshold { cfvo: Cfvo::Max, color: rgbc(hi) }],
                }
            }
            "scale3" => {
                let (lo, mid, hi) = SCALES3[arg.parse::<usize>().unwrap_or(0) % SCALES3.len()];
                CfRuleInput::ColorScale {
                    thresholds: vec![
                        ColorScaleThreshold { cfvo: Cfvo::Min, color: rgbc(lo) },
                        ColorScaleThreshold { cfvo: Cfvo::Percentile(50.), color: rgbc(mid) },
                        ColorScaleThreshold { cfvo: Cfvo::Max, color: rgbc(hi) },
                    ],
                }
            }
            // 旧命令（工具栏三种色阶 / 数据条）兼容
            "scale-ryg" => return Self::cf_rule("scale3", "0"),
            "scale-gyr" => return Self::cf_rule("scale3", "3"),
            "scale-wb" => CfRuleInput::ColorScale {
                thresholds: vec![
                    ColorScaleThreshold { cfvo: Cfvo::Min, color: rgbc(0xffffff) },
                    ColorScaleThreshold { cfvo: Cfvo::Max, color: rgbc(0x3370ff) },
                ],
            },
            "bar-blue" => return Self::cf_rule("bar", "1,0"),
            "bar-green" => return Self::cf_rule("bar", "1,1"),
            "bar-red" => return Self::cf_rule("bar", "1,4"),
            "bar" => {
                // bar:是否渐变,颜色序号
                let (g, i) = arg.split_once(',').unwrap_or(("0", arg));
                CfRuleInput::DataBar {
                    min: Some(Cfvo::Min),
                    max: Some(Cfvo::Max),
                    positive_color: rgbc(BARS[i.parse::<usize>().unwrap_or(0) % BARS.len()]),
                    negative_color: rgbc(0xf8696b),
                    is_gradient: g == "1",
                    show_value: true,
                }
            }
            "icons" => {
                let icons = icons_of(arg);
                if icons.is_empty() {
                    return None;
                }
                let n = icons.len();
                CfRuleInput::IconSet {
                    thresholds: icons
                        .into_iter()
                        .enumerate()
                        .map(|(i, (icon, color))| IconThreshold { icon, cfvo: Cfvo::Percent((i * 100 / n) as f64), color, is_strict: true })
                        .collect(),
                    show_value: true,
                }
            }
            "rating" => {
                let (icon, color, n) = RATINGS[arg.parse::<usize>().unwrap_or(0) % RATINGS.len()].clone();
                CfRuleInput::IconRating {
                    icon,
                    color: rgbc(color),
                    thresholds: (0..n).map(|i| (Cfvo::Percent((i * 100 / n) as f64), true)).collect(),
                    show_value: true,
                }
            }
            _ => return None,
        })
    }

    pub(crate) fn run_cf(&mut self, head: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match head {
            "cf" => {
                let (rule, v) = arg.split_once('=').unwrap_or((arg, ""));
                if let Some(r) = Self::cf_rule(rule, v) {
                    self.add_cf(r, cx);
                }
            }
            "cf-top" | "cf-toppct" | "cf-bottom" | "cf-bottompct" => {
                if let Some(r) = Self::cf_rule(head.trim_start_matches("cf-"), arg) {
                    self.add_cf(r, cx);
                }
            }
            "cf-ask" => {
                // 突出显示：弹出输入框填值（介于填「最小值, 最大值」）
                let rule: &'static str = match arg {
                    "gt" => "gt",
                    "lt" => "lt",
                    "between" => "between",
                    "eq" => "eq",
                    _ => "contains",
                };
                let at = point(self.bounds.origin.x + px(260.), self.bounds.origin.y + px(40.));
                self.open_prompt(PromptKind::Cf(rule), at, window, cx);
            }
            "cf-clear" => {
                let a = self.sel_area();
                self.cf_delete(|range, _| Self::cf_hits(range, &a), cx);
            }
            "cf-clear-all" => self.cf_delete(|_, _| true, cx),
            "cf-clear-dup" => {
                let a = self.sel_area();
                self.cf_delete(|range, r| Self::cf_hits(range, &a) && matches!(r, CfRule::DuplicateValues { .. }), cx);
            }
            "cf-clear-dup-all" => self.cf_delete(|_, r| matches!(r, CfRule::DuplicateValues { .. }), cx),
            "cf-del" => {
                if let Ok(i) = arg.parse::<u32>() {
                    self.model.borrow_mut().delete_conditional_formatting(self.sheet, i).ok();
                    self.changed(cx);
                }
            }
            "cf-up" | "cf-down" => {
                if let Ok(i) = arg.parse::<u32>() {
                    let mut m = self.model.borrow_mut();
                    if head == "cf-up" {
                        m.raise_conditional_formatting_priority(self.sheet, i).ok();
                    } else {
                        m.lower_conditional_formatting_priority(self.sheet, i).ok();
                    }
                }
                self.changed(cx);
            }
            "cf-panel" => {
                self.side = Some(super::dialogs::Side::Cf);
                cx.notify();
            }
            "cf-new" => self.open_cf_dlg(None, arg == "formula", window, cx),
            "cf-edit" => self.open_cf_dlg(arg.parse().ok(), false, window, cx),
            _ => return false,
        }
        true
    }

    fn cf_delete(&mut self, keep_out: impl Fn(&str, &CfRule) -> bool, cx: &mut Context<Self>) {
        let list = self.model.borrow().get_conditional_formatting_list(self.sheet).unwrap_or_default();
        let mut idx: Vec<usize> = list.iter().filter(|cf| keep_out(&cf.range, &cf.cf_rule)).map(|cf| cf.index).collect();
        idx.sort_unstable_by(|a, b| b.cmp(a));
        self.batch(|this| {
            for i in &idx {
                this.model.borrow_mut().delete_conditional_formatting(this.sheet, *i as u32).ok();
            }
        });
        self.toast = Some(format!("已清除 {} 条条件格式", idx.len()).into());
        self.changed(cx);
    }

    /// 色阶 / 数据条 / 图标集的缩略图选择面板（嵌在级联菜单里）。
    pub(crate) fn cf_gallery(&self, key: &str, t: &Theme, cx: &mut Context<Self>) -> Div {
        let tile = |id: SharedString, cmd: String, body: Div, cx: &mut Context<Self>| {
            div()
                .id(id)
                .p(px(3.))
                .rounded(px(5.))
                .border_1()
                .border_color(gpui::transparent_black())
                .cursor_pointer()
                .hover(|s| s.border_color(t.accent).bg(t.hover))
                .child(body)
                .on_click(Self::cmd(cmd, cx))
        };
        let cell_rows = |colors: Vec<u32>| {
            div()
                .w(px(30.))
                .border_1()
                .border_color(t.border)
                .flex()
                .flex_col()
                .children(colors.into_iter().map(|c| div().h(px(7.)).bg(rgb(c)).border_b_1().border_color(hsla(0., 0., 0.85, 0.6))))
        };
        let foot = |cx: &mut Context<Self>| {
            div()
                .id("cf-foot")
                .mt(px(6.))
                .pt(px(6.))
                .border_t_1()
                .border_color(t.border)
                .px(px(4.))
                .h(px(26.))
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .child("自定义规则")
                .on_click(Self::cmd("cf-new", cx))
        };
        match key {
            "scales" => {
                let blend = |a: u32, b: u32, k: f32| {
                    let ch = |s: u32| (((a >> s) & 255) as f32 * (1. - k) + ((b >> s) & 255) as f32 * k) as u32;
                    (ch(16) << 16) | (ch(8) << 8) | ch(0)
                };
                div()
                    .p(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("双色色阶"))
                    .child(div().flex().flex_row().flex_wrap().gap(px(4.)).children(SCALES2.iter().enumerate().map(|(i, (lo, hi))| {
                        tile(format!("sc2-{i}").into(), format!("cf:scale2={i}"), cell_rows((0..5).map(|k| blend(*hi, *lo, k as f32 / 4.)).collect()), cx)
                    })))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("三色色阶"))
                    .child(div().flex().flex_row().flex_wrap().gap(px(4.)).children(SCALES3.iter().enumerate().map(|(i, (lo, mid, hi))| {
                        tile(
                            format!("sc3-{i}").into(),
                            format!("cf:scale3={i}"),
                            cell_rows(vec![*hi, blend(*hi, *mid, 0.5), *mid, blend(*mid, *lo, 0.5), *lo]),
                            cx,
                        )
                    })))
                    .child(foot(cx))
            }
            "bars" => {
                let bar = |c: u32, grad: bool| {
                    div().w(px(30.)).border_1().border_color(t.border).flex().flex_col().gap(px(1.)).py(px(1.)).children([0.9, 0.5, 0.7, 0.3].map(|w| {
                        div()
                            .h(px(7.))
                            .w(relative(w))
                            .when(!grad, |d| d.bg(rgb(c)))
                            .when(grad, |d| d.bg(linear_gradient(90., linear_color_stop(rgb(c), 0.), linear_color_stop(rgb(0xffffff), 1.))))
                    }))
                };
                div()
                    .p(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("纯色填充"))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(4.))
                            .children(BARS.iter().enumerate().map(|(i, c)| tile(format!("bs-{i}").into(), format!("cf:bar=0,{i}"), bar(*c, false), cx))),
                    )
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("渐变填充"))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(4.))
                            .children(BARS.iter().enumerate().map(|(i, c)| tile(format!("bg-{i}").into(), format!("cf:bar=1,{i}"), bar(*c, true), cx))),
                    )
                    .child(foot(cx))
            }
            _ => {
                let mut d = div().p(px(8.)).flex().flex_col().gap(px(4.));
                for g in ["方向", "形状", "标记"] {
                    d = d.child(div().text_size(px(12.)).text_color(t.text_muted).child(g)).child(div().flex().flex_row().flex_wrap().gap(px(2.)).children(
                        ICON_SETS.iter().filter(|s| s.0 == g).map(|(_, name)| {
                            let icons = icons_of(name);
                            tile(
                                format!("is-{name}").into(),
                                format!("cf:icons={name}"),
                                div()
                                    .w(px(138.))
                                    .h(px(18.))
                                    .flex()
                                    .flex_row()
                                    .gap(px(3.))
                                    .children(icons.into_iter().rev().map(|(ic, c)| icon_el(&ic, &hex_of(&c), 15.))),
                                cx,
                            )
                        }),
                    ));
                }
                d = d.child(div().text_size(px(12.)).text_color(t.text_muted).child("等级")).child(div().flex().flex_row().flex_wrap().gap(px(2.)).children(
                    RATINGS.iter().enumerate().map(|(i, (ic, c, n))| {
                        tile(
                            format!("rt-{i}").into(),
                            format!("cf:rating={i}"),
                            div()
                                .w(px(138.))
                                .h(px(18.))
                                .flex()
                                .flex_row()
                                .gap(px(2.))
                                .children((0..*n).map(|k| icon_el(ic, &format!("#{:06x}", if k + 1 < *n { *c } else { 0xdee0e3 }), 14.))),
                            cx,
                        )
                    }),
                ));
                d.child(foot(cx))
            }
        }
    }

    /// 右侧「条件格式 · 管理规则」面板。
    pub(crate) fn cf_panel(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let list = self.model.borrow().get_conditional_formatting_list(self.sheet).unwrap_or_default();
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
            .child(div().flex().flex_row().items_center().child(div().flex_1().text_size(px(15.)).font_weight(FontWeight::MEDIUM).child("条件格式")).child(
                Self::ib("cfp-x", "x", "关闭", false, t).on_click(cx.listener(|this, _, _, cx| {
                    this.side = None;
                    cx.notify();
                })),
            ))
            .child(crate::base::widgets::primary_btn("cfp-new", "+ 新建规则", t).on_click(Self::cmd("cf-new", cx)))
            .when(n == 0, |d| d.child(div().py(px(30.)).flex().justify_center().text_color(t.text_muted).child("当前工作表还没有条件格式")))
            .child(div().id("cfp-list").flex_1().overflow_y_scroll().flex().flex_col().gap(px(6.)).children(list.into_iter().enumerate().map(|(k, cf)| {
                let idx = cf.index;
                let fmt = self.model.borrow().get_dxf_for_conditional_formatting(self.sheet, idx as u32).ok().flatten();
                let (bg, fg) = fmt
                    .map(|d| (d.fill.map(|f| hex_of(&f.color)).unwrap_or_default(), d.font.map(|f| hex_of(&f.color)).unwrap_or_default()))
                    .unwrap_or_default();
                div()
                    .id(("cfp", k))
                    .group("cfp")
                    .p(px(8.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(t.border)
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .size(px(28.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(t.border)
                            .bg(hex_to_hsla(&bg).unwrap_or(t.bg))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .text_color(hex_to_hsla(&fg).unwrap_or(t.text))
                            .child("Aa"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(describe(&cf.cf_rule))
                            .child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("应用于 {}", cf.range))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .invisible()
                            .group_hover("cfp", |s| s.visible())
                            .child(Self::ib(("cfp-up", k), "sh-up", "提高优先级", false, t).size(px(22.)).on_click(Self::cmd(format!("cf-up:{idx}"), cx)))
                            .child(Self::ib(("cfp-dn", k), "sh-down", "降低优先级", false, t).size(px(22.)).on_click(Self::cmd(format!("cf-down:{idx}"), cx)))
                            .child(Self::ib(("cfp-ed", k), "pencil", "编辑", false, t).size(px(22.)).on_click(Self::cmd(format!("cf-edit:{idx}"), cx)))
                            .child(Self::ib(("cfp-del", k), "trash", "删除", false, t).size(px(22.)).on_click(Self::cmd(format!("cf-del:{idx}"), cx))),
                    )
            })))
            .child(
                crate::base::widgets::ghost_btn("cfp-clear", "清除整张工作表的规则", t)
                    .when(n == 0, |d| d.opacity(0.4))
                    .on_click(Self::cmd("cf-clear-all", cx)),
            )
    }
}

/// 图标集里的一个图标（画成 SVG 小图）。
pub(crate) fn icon_el(ic: &Icon, color: &str, size: f32) -> Div {
    div().size(px(size)).flex().items_center().justify_center().child(icon(icon_name(ic), size, hex_to_hsla(color).unwrap_or(rgb(0x808080).into())))
}

pub(crate) fn icon_name(ic: &Icon) -> &'static str {
    match ic {
        Icon::ArrowUp => "sh-i-up",
        Icon::ArrowRight => "sh-i-right",
        Icon::ArrowDown => "sh-i-down",
        Icon::ArrowAngleUp => "sh-i-angle-up",
        Icon::ArrowAngleDown => "sh-i-angle-down",
        Icon::Circle => "sh-i-circle",
        Icon::TriangleUp | Icon::TriangleUpFilled => "sh-i-tri-up",
        Icon::TriangleDown | Icon::TriangleDownFilled => "sh-i-tri-down",
        Icon::FlatRectangle => "sh-i-flat",
        Icon::Rhombus => "sh-i-rhombus",
        Icon::Flag => "sh-i-flag",
        Icon::Check => "sh-i-check",
        Icon::Cross => "sh-i-cross",
        Icon::Exclamation => "sh-i-excl",
        Icon::Star => "sh-i-star",
        Icon::Heart => "sh-i-heart",
        Icon::ThumbsUp => "sh-i-thumb-up",
        Icon::ThumbsDown => "sh-i-thumb-down",
    }
}
