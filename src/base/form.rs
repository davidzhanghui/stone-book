//! 表单视图：把数据表变成一张问卷，填写提交即新增一条记录（字段的显示与顺序在「表单字段」里配置）。

use super::editor::BaseEditor;
use super::formula;
use super::model::*;
use super::widgets::*;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde_json::Value;

impl BaseEditor {
    fn form_fields(&self) -> Vec<Field> {
        self.visible().into_iter().filter(|f| !f.kind.computed() && !f.kind.is_link() && f.kind != FieldType::Attachment).collect()
    }

    /// 表单里某题当前的值（文本框按字段类型转换）。
    fn form_value(&self, f: &Field, cx: &App) -> Option<Value> {
        let key = format!("form:{}", f.id);
        if self.inputs.contains_key(&key) {
            let text = self.input_text(&key, cx);
            match f.kind {
                FieldType::Text | FieldType::Url | FieldType::Phone | FieldType::Email => (!text.trim().is_empty()).then(|| Value::from(text.trim())),
                _ => super::io::text_to_value(&mut f.clone(), &text),
            }
        } else {
            self.form_vals.get(&f.id).cloned().filter(|v| !is_empty_value(Some(v)) || f.kind == FieldType::Checkbox)
        }
    }

    /// 显示条件：按前面题目的当前答案判断这道题是否出现。
    fn form_visible(&self, f: &Field, v: &View, cx: &App) -> bool {
        let Some(c) = v.form.show_if.get(&f.id) else { return true };
        let Some(src) = self.field(&c.field) else { return true };
        let val = self.form_value(&src, cx);
        match c.op {
            Op::Empty => is_empty_value(val.as_ref()),
            Op::NotEmpty => !is_empty_value(val.as_ref()),
            Op::Checked => val.and_then(|v| v.as_bool()).unwrap_or(false),
            Op::Unchecked => !val.and_then(|v| v.as_bool()).unwrap_or(false),
            _ if src.kind.has_options() => {
                let (have, want) = (str_list(val.as_ref()), str_list(Some(&c.value)));
                let any = have.iter().any(|h| want.contains(h));
                match c.op {
                    Op::IsNot | Op::NotContains => !any,
                    _ => any,
                }
            }
            op => {
                let fv = |v: Option<&Value>| match v {
                    Some(Value::Number(n)) => super::formula::FVal::Num(n.as_f64().unwrap_or(0.)),
                    Some(Value::String(s)) => super::formula::FVal::Str(s.clone()),
                    Some(Value::Bool(b)) => super::formula::FVal::Bool(*b),
                    _ => super::formula::FVal::Null,
                };
                super::calc::lookup_cond_match(&fv(val.as_ref()), op, &fv(Some(&c.value)))
            }
        }
    }

    fn submit_form(&mut self, cx: &mut Context<Self>) {
        let v = self.view();
        let mut missing = vec![];
        for f in self.form_fields() {
            if v.form.required.contains(&f.id) && self.form_visible(&f, &v, cx) && is_empty_value(self.form_value(&f, cx).as_ref()) {
                missing.push(f.id.clone());
            }
        }
        if !missing.is_empty() {
            self.scratch.insert("form_err".into(), missing.join(","));
            self.show_toast(&format!("还有 {} 道必填题没有填写", missing.len()), cx);
            cx.notify();
            return;
        }
        self.scratch.remove("form_err");
        let mut preset = vec![];
        for mut f in self.form_fields() {
            if !self.form_visible(&f, &v, cx) {
                continue;
            }
            let key = format!("form:{}", f.id);
            let v = if self.inputs.contains_key(&key) {
                let text = self.input_text(&key, cx);
                match f.kind {
                    FieldType::Text | FieldType::Url | FieldType::Phone | FieldType::Email => (!text.trim().is_empty()).then(|| Value::from(text.trim())),
                    _ => super::io::text_to_value(&mut f, &text),
                }
            } else {
                self.form_vals.get(&f.id).cloned()
            };
            if let Some(v) = v {
                preset.push((f.id.clone(), v));
            }
        }
        if preset.is_empty() {
            self.show_toast("表单还没有填写内容", cx);
            return;
        }
        let rid = self.add_record(preset, None, cx);
        if let Some(r) = rid {
            self.scratch.insert("form_last".into(), r);
        }
        self.form_vals.clear();
        self.scratch.remove("form_init");
        self.inputs.retain(|k, _| !k.starts_with("form:"));
        self.form_done = true;
        cx.notify();
    }

    pub(super) fn render_form(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tname = self.b().table(&self.tid).map(|t| t.meta.name.clone()).unwrap_or_default();
        let v = self.view();
        if self.form_done {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                .child(div().size(px(56.)).rounded_full().bg(rgb(0x34c724)).flex().items_center().justify_center().child(icon("check", 30., white())))
                .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child("提交成功"))
                .child(div().text_size(px(13.)).text_color(t.text_muted).child(if v.form.done_text.trim().is_empty() {
                    format!("已新增一条记录到「{tname}」")
                } else {
                    v.form.done_text.clone()
                }))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(10.))
                        .child(primary_btn("form-again", "再填一份", t).on_click(cx.listener(|this, _, _, cx| {
                            this.form_done = false;
                            cx.notify();
                        })))
                        .child(ghost_btn("form-view", "查看已提交的记录", t).on_click(cx.listener(|this, _, _, cx| {
                            this.form_done = false;
                            this.detail = this.scratch.get("form_last").cloned();
                            cx.notify();
                        }))),
                )
                .into_any_element();
        }
        let all_fields = self.form_fields();
        // 默认值预填（表单里设置的优先，其次字段默认值）；每次「再填一份」重新预填
        if !self.scratch.contains_key("form_init") {
            self.scratch.insert("form_init".into(), "1".into());
            for f in &all_fields {
                let dv = v.form.defaults.get(&f.id).cloned().or_else(|| default_value(f));
                let Some(dv) = dv else { continue };
                match f.kind {
                    FieldType::Select | FieldType::Stage | FieldType::MultiSelect | FieldType::Checkbox | FieldType::Rating => {
                        self.form_vals.insert(f.id.clone(), dv);
                    }
                    _ => {
                        let text = match (&f.kind, &dv) {
                            (FieldType::Date, Value::Number(n)) => formula::fmt_date(n.as_i64().unwrap_or(0), 2),
                            (FieldType::Progress, Value::Number(n)) => format!("{}%", formula::fmt_num(n.as_f64().unwrap_or(0.) * 100.)),
                            (_, Value::String(s)) => s.clone(),
                            (_, Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "),
                            (_, Value::Number(n)) => formula::fmt_num(n.as_f64().unwrap_or(0.)),
                            _ => String::new(),
                        };
                        let key = format!("form:{}", f.id);
                        self.inputs.remove(&key);
                        let i = self.input(&key, "", &text, false, window, cx);
                        i.update(cx, |i, cx| i.set_text(&text, cx));
                    }
                }
            }
        }
        let fields: Vec<Field> = all_fields.into_iter().filter(|f| self.form_visible(f, &v, cx)).collect();
        let errs: Vec<String> = self.scratch.get("form_err").map(|s| s.split(',').map(String::from).collect()).unwrap_or_default();
        let mut card =
            div().w(px(640.)).flex().flex_col().gap(px(22.)).p(px(32.)).rounded(px(12.)).bg(t.panel_bg).border_1().border_color(t.border).shadow_md();
        card = card.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .pb(px(8.))
                .border_b_1()
                .border_color(t.border)
                .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child(if !v.form.title.trim().is_empty() {
                    v.form.title.clone()
                } else if v.name.ends_with("表单视图") {
                    tname.clone()
                } else {
                    v.name.clone()
                }))
                .child(div().text_size(px(13.)).text_color(t.text_muted).child(if v.form.desc.trim().is_empty() {
                    "填写后提交，将在数据表中新增一条记录".to_string()
                } else {
                    v.form.desc.clone()
                })),
        );
        if !v.form.cover.is_empty() {
            card = card.child(
                div()
                    .h(px(160.))
                    .w_full()
                    .rounded(px(8.))
                    .overflow_hidden()
                    .child(img(std::path::PathBuf::from(v.form.cover.clone())).size_full().object_fit(ObjectFit::Cover)),
            );
        }
        for (i, f) in fields.iter().enumerate() {
            let req = v.form.required.contains(&f.id);
            let note = v.form.notes.get(&f.id).cloned().unwrap_or_else(|| f.desc.clone());
            let err = errs.contains(&f.id);
            let label = div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{}. {}", i + 1, f.name))
                        .when(req, |d| d.child(div().text_color(t.fg_palette[1]).child("*")))
                        .child(div().text_size(px(12.)).text_color(t.text_muted).font_weight(FontWeight::NORMAL).child(f.kind.label())),
                )
                .when(!note.trim().is_empty(), |d| d.child(div().text_size(px(12.5)).text_color(t.text_muted).child(note.clone())));
            let fid = f.id.clone();
            let input: AnyElement = match f.kind {
                FieldType::Select | FieldType::Stage | FieldType::MultiSelect => {
                    let multi = f.kind == FieldType::MultiSelect;
                    let cur = str_list(self.form_vals.get(&f.id));
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.))
                        .children(f.prop.options.iter().map(|o| {
                            let on = cur.contains(&o.id);
                            let (oid, fid) = (o.id.clone(), fid.clone());
                            div()
                                .id(SharedString::from(format!("fo-{}", o.id)))
                                .cursor_pointer()
                                .rounded(px(12.))
                                .border_2()
                                .border_color(if on { t.accent } else { transparent_black() })
                                .child(tag(o.name.clone(), o.color, t))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let mut cur = str_list(this.form_vals.get(&fid));
                                    if cur.contains(&oid) {
                                        cur.retain(|x| x != &oid);
                                    } else if multi {
                                        cur.push(oid.clone());
                                    } else {
                                        cur = vec![oid.clone()];
                                    }
                                    let v = if multi { Value::from(cur) } else { cur.first().map(|s| Value::from(s.clone())).unwrap_or(Value::Null) };
                                    this.form_vals.insert(fid.clone(), v);
                                    cx.notify();
                                }))
                        }))
                        .into_any_element()
                }
                FieldType::Checkbox => {
                    let on = self.form_vals.get(&f.id).and_then(|v| v.as_bool()).unwrap_or(false);
                    div()
                        .id(SharedString::from(format!("fcb-{fid}")))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .child(check_box(on, t))
                        .child("是")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.form_vals.insert(fid.clone(), Value::Bool(!on));
                            cx.notify();
                        }))
                        .into_any_element()
                }
                FieldType::Rating => {
                    let n = self.form_vals.get(&f.id).and_then(|v| v.as_f64()).unwrap_or(0.) as i32;
                    let max = if f.prop.max > 0. { f.prop.max as i32 } else { 5 };
                    let (ic, col) = (rating_icon(&f.prop.symbol), rating_color(&f.prop.symbol));
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(6.))
                        .children((1..=max).map(|k| {
                            let fid = fid.clone();
                            div()
                                .id(SharedString::from(format!("frt-{fid}-{k}")))
                                .cursor_pointer()
                                .child(icon(if k <= n { rating_icon_on(&f.prop.symbol) } else { ic }, 24., if k <= n { col } else { t.border_strong }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.form_vals.insert(fid.clone(), Value::from(k));
                                    cx.notify();
                                }))
                        }))
                        .into_any_element()
                }
                FieldType::Date => {
                    let i = self.input(&format!("form:{fid}"), "例如 2026-10-28", "", false, window, cx);
                    let fid2 = fid.clone();
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(input_box(t).w(px(220.)).child(i))
                        .child(ghost_btn(SharedString::from(format!("fd-today-{fid2}")), "今天", t).on_click(cx.listener(move |this, _, _, cx| {
                            let s = formula::fmt_date(formula::today(), 2);
                            if let Some((i, _)) = this.inputs.get(&format!("form:{fid2}")) {
                                i.update(cx, |i, cx| i.set_text(&s, cx));
                            }
                        })))
                        .into_any_element()
                }
                _ => {
                    let ph = match f.kind {
                        FieldType::Number | FieldType::Currency => "请输入数字",
                        FieldType::Progress => "请输入百分比，例如 60%",
                        FieldType::Person => "输入名字，多人用逗号分隔",
                        FieldType::Url => "https://",
                        _ => "请输入",
                    };
                    let i = self.input(&format!("form:{fid}"), ph, "", false, window, cx);
                    input_box(t).h(px(36.)).child(i).into_any_element()
                }
            };
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label)
                    .child(input)
                    .when(err, |d| d.child(div().text_size(px(12.)).text_color(t.fg_palette[1]).child("这道题必须填写"))),
            );
        }
        if fields.is_empty() {
            card = card.child(div().text_color(t.text_muted).child("在「表单字段」里勾选要收集的字段"));
        }
        card = card.child(
            div()
                .flex()
                .flex_row()
                .child(primary_btn("form-submit", "提交", t).w(px(120.)).h(px(36.)).on_click(cx.listener(|this, _, _, cx| this.submit_form(cx)))),
        );
        div()
            .id("form-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(t.sidebar_bg)
            .child(div().w_full().flex().justify_center().py(px(32.)).child(card))
            .into_any_element()
    }
}
