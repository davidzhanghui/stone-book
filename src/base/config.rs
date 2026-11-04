//! 新增的配置面板与通用组件：回调式下拉 / 输入框、条件编辑器（筛选、填色、按条件查找、自动化、
//! 表单显示条件共用），以及填色、批量修改、删除字段确认、字段编组、表单设置、导入导出面板。

use super::calc::{self, DATE_PRESETS};
use super::editor::{BaseEditor, Pick, PickItem, PickTarget};
use super::model::*;
use super::widgets::*;
use crate::line_input::LineInput;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde_json::Value;
use std::rc::Rc;

pub(super) type CondSet = Rc<dyn Fn(&mut BaseEditor, Cond, &mut Context<BaseEditor>)>;

pub(super) const COLOR_KINDS: [&str; 3] = ["单元格", "整行", "整列"];

/// 条件的初始值（换字段 / 运算符时）。
pub(super) fn blank_cond(f: &Field) -> Cond {
    Cond { field: f.id.clone(), op: ops_for(f.kind)[0], value: Value::Null }
}

impl BaseEditor {
    // ───────────── 通用组件 ─────────────

    /// 打开二级下拉，选中后调用 `cb`（多选时传入以换行分隔的全部选中项）。
    pub(super) fn open_pick_cb(
        &mut self,
        items: Vec<PickItem>,
        selected: Vec<String>,
        multi: bool,
        at: Point<Pixels>,
        cb: impl Fn(&mut BaseEditor, String, &mut Context<BaseEditor>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.pick = Some(Pick { target: PickTarget::Cb, items, selected, multi, at });
        self.pick_cb = Some(Rc::new(cb));
        cx.notify();
    }

    /// 内容一变就调用 `cb` 的输入框（每次渲染刷新回调，闭包里的数据总是最新的）。
    pub(super) fn cb_input(
        &mut self,
        key: &str,
        ph: &str,
        init: &str,
        cb: impl Fn(&mut BaseEditor, String, &mut Context<BaseEditor>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<LineInput> {
        let k = format!("cb:{key}");
        let i = self.input(&k, ph, init, false, window, cx);
        self.input_cbs.insert(k, Rc::new(cb));
        i
    }

    pub(super) fn field_items_of(&self, tid: &str, filter: impl Fn(&Field) -> bool) -> Vec<PickItem> {
        self.b()
            .table(tid)
            .map(|t| t.field_list().into_iter().filter(|f| filter(f)).map(|f| PickItem::new(f.id.clone(), f.name.clone()).icon(f.kind.icon())).collect())
            .unwrap_or_default()
    }

    pub(super) fn table_items(&self) -> Vec<PickItem> {
        self.b().table_list().iter().map(|t| PickItem::new(t.meta.id.clone(), t.meta.name.clone()).icon("v-grid")).collect()
    }

    /// 条件编辑器：字段 · 运算符 · 值。`key` 区分输入框，`tid` 是字段所在的数据表。
    pub(super) fn cond_editor(&mut self, key: &str, tid: &str, c: &Cond, set: CondSet, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let f = self.b().field(tid, &c.field).cloned();
        let tid2 = tid.to_string();
        let (set1, c1) = (set.clone(), c.clone());
        let fsel = select_box(
            SharedString::from(format!("ce-f-{key}")),
            f.as_ref().map(|f| f.name.clone()).unwrap_or("选择字段".into()),
            f.as_ref().map(|f| f.kind.icon()),
            t,
        )
        .w(px(130.))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                let items = this.field_items_of(&tid2, |_| true);
                let (set, tid) = (set1.clone(), tid2.clone());
                this.open_pick_cb(
                    items,
                    vec![c1.field.clone()],
                    false,
                    ev.position,
                    move |this, id, cx| {
                        let f = this.b().field(&tid, &id).cloned();
                        if let Some(f) = f {
                            set(this, blank_cond(&f), cx);
                        }
                    },
                    cx,
                );
            }),
        );
        let mut row = div().flex().flex_row().items_center().gap(px(6.)).child(fsel);
        let Some(f) = f else { return row.into_any_element() };
        let kind = f.kind;
        let (set2, c2) = (set.clone(), c.clone());
        row = row.child(select_box(SharedString::from(format!("ce-o-{key}")), c.op.label(kind), None, t).w(px(92.)).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                let items = ops_for(kind).iter().map(|o| PickItem::new(format!("{o:?}"), o.label(kind))).collect();
                let (set, c) = (set2.clone(), c2.clone());
                this.open_pick_cb(
                    items,
                    vec![format!("{:?}", c2.op)],
                    false,
                    ev.position,
                    move |this, id, cx| {
                        if let Some(op) = Op::ALL.into_iter().find(|o| format!("{o:?}") == id) {
                            let mut c = c.clone();
                            if (op == Op::Between) != (c.op == Op::Between) {
                                c.value = Value::Null;
                            }
                            c.op = op;
                            set(this, c, cx);
                        }
                    },
                    cx,
                );
            }),
        ));
        if !c.op.needs_value() {
            return row.into_any_element();
        }
        let ik = format!("{key}:{}:{:?}", c.field, c.op);
        if c.op == Op::Between {
            let (a, b) = calc::pair(&c.value);
            for (j, init) in [(0usize, a.clone()), (1, b.clone())] {
                let (set, c, a, b) = (set.clone(), c.clone(), a.clone(), b.clone());
                let inp = self.cb_input(
                    &format!("{ik}:{j}"),
                    if j == 0 { "下限" } else { "上限" },
                    &init,
                    move |this, text, cx| {
                        let mut c = c.clone();
                        c.value = if j == 0 { Value::from(vec![text, b.clone()]) } else { Value::from(vec![a.clone(), text]) };
                        set(this, c, cx);
                    },
                    window,
                    cx,
                );
                row = row.child(input_box(t).w(px(96.)).child(inp));
            }
            return row.into_any_element();
        }
        if kind.has_options() {
            let names: Vec<String> = str_list(Some(&c.value)).iter().filter_map(|id| f.opt(id).map(|o| o.name.clone())).collect();
            let (set, c, f2) = (set.clone(), c.clone(), f.clone());
            row = row.child(
                select_box(SharedString::from(format!("ce-v-{key}")), if names.is_empty() { "请选择".into() } else { names.join("、") }, None, t)
                    .w(px(170.))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            let items = f2
                                .prop
                                .options
                                .iter()
                                .map(|o| PickItem { id: o.id.clone(), label: o.name.clone(), icon: None, color: Some(o.color), group: None })
                                .collect();
                            let (set, c) = (set.clone(), c.clone());
                            this.open_pick_cb(
                                items,
                                str_list(Some(&c.value)),
                                true,
                                ev.position,
                                move |this, ids, cx| {
                                    let mut c = c.clone();
                                    c.value = Value::from(ids.split('\n').filter(|s| !s.is_empty()).map(String::from).collect::<Vec<_>>());
                                    set(this, c, cx);
                                },
                                cx,
                            );
                        }),
                    ),
            );
            return row.into_any_element();
        }
        if kind == FieldType::Checkbox {
            let on = c.value.as_bool().unwrap_or(true);
            let (set, c) = (set.clone(), c.clone());
            row = row.child(select_box(SharedString::from(format!("ce-v-{key}")), if on { "已勾选" } else { "未勾选" }, None, t).w(px(110.)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let (set, c) = (set.clone(), c.clone());
                    this.open_pick_cb(
                        vec![PickItem::new("true", "已勾选"), PickItem::new("false", "未勾选")],
                        vec![],
                        false,
                        ev.position,
                        move |this, id, cx| {
                            let mut c = c.clone();
                            c.value = Value::Bool(id == "true");
                            set(this, c, cx);
                        },
                        cx,
                    );
                }),
            ));
            return row.into_any_element();
        }
        if kind.is_date() {
            let cur = c.value.as_str().unwrap_or("").to_string();
            let preset = DATE_PRESETS.iter().find(|p| p.0 == cur && !cur.is_empty()).map(|p| p.1.to_string());
            let label = preset.clone().unwrap_or_else(|| {
                if let Some(n) = cur.strip_prefix("past:") {
                    format!("过去 {n} 天")
                } else if let Some(n) = cur.strip_prefix("next:") {
                    format!("未来 {n} 天")
                } else {
                    "指定日期".into()
                }
            });
            let (set1, c1) = (set.clone(), c.clone());
            row = row.child(select_box(SharedString::from(format!("ce-v-{key}")), label, None, t).w(px(104.)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let mut items: Vec<PickItem> = DATE_PRESETS.iter().map(|(k, l)| PickItem::new(if k.is_empty() { "date" } else { k }, *l)).collect();
                    items.push(PickItem::new("past:", "过去 N 天"));
                    items.push(PickItem::new("next:", "未来 N 天"));
                    let (set, c) = (set1.clone(), c1.clone());
                    this.open_pick_cb(
                        items,
                        vec![],
                        false,
                        ev.position,
                        move |this, id, cx| {
                            let mut c = c.clone();
                            c.value = Value::from(match id.as_str() {
                                "date" => String::new(),
                                "past:" | "next:" => format!("{id}14"),
                                x => x.to_string(),
                            });
                            set(this, c, cx);
                        },
                        cx,
                    );
                }),
            ));
            if preset.is_none() {
                let n_mode = cur.starts_with("past:") || cur.starts_with("next:");
                let init = if n_mode { cur[5..].to_string() } else { cur.clone() };
                let prefix = if n_mode { cur[..5].to_string() } else { String::new() };
                let (set, c) = (set.clone(), c.clone());
                let inp = self.cb_input(
                    &format!("{ik}:{prefix}"),
                    if n_mode { "天数" } else { "2026-10-28" },
                    &init,
                    move |this, text, cx| {
                        let mut c = c.clone();
                        c.value = Value::from(format!("{prefix}{}", text.trim()));
                        set(this, c, cx);
                    },
                    window,
                    cx,
                );
                row = row.child(input_box(t).w(px(if n_mode { 60. } else { 110. })).child(inp));
            }
            return row.into_any_element();
        }
        let init = c.value.as_str().map(String::from).or_else(|| c.value.as_f64().map(super::formula::fmt_num)).unwrap_or_default();
        let (set, c) = (set.clone(), c.clone());
        let inp = self.cb_input(
            &ik,
            "输入值",
            &init,
            move |this, text, cx| {
                let mut c = c.clone();
                c.value = Value::from(text);
                set(this, c, cx);
            },
            window,
            cx,
        );
        row.child(input_box(t).w(px(170.)).child(inp)).into_any_element()
    }

    /// 颜色选择（12 色标签色）。
    pub(super) fn color_swatch(&self, id: impl Into<ElementId>, color: u8, t: &Theme) -> Stateful<Div> {
        let (bg, fg) = tag_color(color, t.dark);
        div().id(id).size(px(22.)).flex_none().rounded(px(5.)).bg(bg).border_1().border_color(fg.opacity(0.5)).cursor_pointer()
    }

    pub(super) fn open_color_pick(
        &mut self,
        cur: u8,
        at: Point<Pixels>,
        cb: impl Fn(&mut BaseEditor, u8, &mut Context<BaseEditor>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let names = ["蓝", "绿", "橙", "青", "紫", "红", "黄", "粉", "靛", "棕", "浅绿", "灰"];
        let items = (0..OPT_COLORS as u8)
            .map(|c| PickItem { id: c.to_string(), label: names[c as usize % names.len()].to_string(), icon: None, color: Some(c), group: None })
            .collect();
        self.open_pick_cb(items, vec![cur.to_string()], false, at, move |this, id, cx| cb(this, id.parse().unwrap_or(0), cx), cx);
    }

    // ───────────── 填色（R2） ─────────────

    pub(super) fn colors_panel(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let v = self.view();
        let tid = self.tid.clone();
        let n = v.colors.len();
        let mut rows = div().id("clr-list").max_h(px(420.)).overflow_y_scroll().flex().flex_col().gap(px(8.)).px(px(12.));
        for (i, r) in v.colors.iter().enumerate() {
            let rid = r.id.clone();
            let upd = move |rid: String, f: Rc<dyn Fn(&mut ColorRule)>| {
                move |this: &mut BaseEditor, cx: &mut Context<BaseEditor>| {
                    let (rid, f) = (rid.clone(), f.clone());
                    this.update_view(cx, move |v| {
                        if let Some(r) = v.colors.iter_mut().find(|x| x.id == rid) {
                            f(r);
                        }
                    })
                }
            };
            let fname = self.field(&r.field).map(|f| f.name).unwrap_or("选择字段".into());
            let mut line = div().flex().flex_row().items_center().gap(px(6.));
            line = line.child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().id(("clr-up", i)).cursor_pointer().when(i > 0, |d| d.child(icon("chevron-up", 11., t.text_muted))).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.update_view(cx, |v| {
                                if i > 0 {
                                    v.colors.swap(i, i - 1)
                                }
                            })
                        },
                    )))
                    .child(div().id(("clr-dn", i)).cursor_pointer().when(i + 1 < n, |d| d.child(icon("chevron-down", 11., t.text_muted))).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.update_view(cx, |v| {
                                if i + 1 < v.colors.len() {
                                    v.colors.swap(i, i + 1)
                                }
                            })
                        }),
                    )),
            );
            let up_kind = upd;
            let rid_k = rid.clone();
            line = line.child(select_box(("clr-k", i), COLOR_KINDS[r.kind.min(2) as usize], None, t).w(px(84.)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let items = COLOR_KINDS.iter().enumerate().map(|(k, l)| PickItem::new(k.to_string(), *l)).collect();
                    let up = up_kind;
                    let rid = rid_k.clone();
                    this.open_pick_cb(
                        items,
                        vec![],
                        false,
                        ev.position,
                        move |this, id, cx| {
                            let k: u8 = id.parse().unwrap_or(0);
                            up(
                                rid.clone(),
                                Rc::new(move |r: &mut ColorRule| {
                                    r.kind = k;
                                    if k == 2 {
                                        r.cond = None;
                                    } else if r.cond.is_none() {
                                        r.cond = Some(Cond { field: r.field.clone(), op: Op::NotEmpty, value: Value::Null });
                                    }
                                }),
                            )(this, cx)
                        },
                        cx,
                    );
                }),
            ));
            if r.kind == 2 {
                let up_f = upd;
                let rid_f = rid.clone();
                let tid2 = tid.clone();
                line = line.child(select_box(("clr-f", i), fname, None, t).w(px(120.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let items = this.field_items_of(&tid2, |_| true);
                        let up = up_f;
                        let rid = rid_f.clone();
                        let tid = tid2.clone();
                        this.open_pick_cb(
                            items,
                            vec![],
                            false,
                            ev.position,
                            move |this, id, cx| {
                                let f = this.b().field(&tid, &id).cloned();
                                up(
                                    rid.clone(),
                                    Rc::new(move |r: &mut ColorRule| {
                                        r.field = id.clone();
                                        if r.kind == 0 {
                                            r.cond = f.as_ref().map(blank_cond);
                                        }
                                    }),
                                )(this, cx)
                            },
                            cx,
                        );
                    }),
                ));
            }
            if let (Some(c), true) = (r.cond.clone(), r.kind != 2) {
                let rid_c = rid.clone();
                let up_c = upd;
                let kind = r.kind;
                let _ = kind;
                let set: CondSet = Rc::new(move |this, c, cx| {
                    let c2 = c.clone();
                    up_c(
                        rid_c.clone(),
                        Rc::new(move |r: &mut ColorRule| {
                            // 单元格规则：条件字段就是被填色的字段
                            r.field = c2.field.clone();
                            r.cond = Some(c2.clone());
                        }),
                    )(this, cx)
                });
                let shown = if r.kind == 0 { Cond { field: r.field.clone(), ..c } } else { c };
                line = line.child(self.cond_editor(&format!("clr{i}"), &tid, &shown, set, t, window, cx));
            }
            let up_col = upd;
            let rid_col = rid.clone();
            let cur = r.color;
            line = line.child(self.color_swatch(("clr-c", i), r.color, t).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let up = up_col;
                    let rid = rid_col.clone();
                    this.open_color_pick(cur, ev.position, move |this, c, cx| up(rid.clone(), Rc::new(move |r: &mut ColorRule| r.color = c))(this, cx), cx);
                }),
            ));
            let rid_d = rid.clone();
            line = line.child(
                div()
                    .id(("clr-del", i))
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(icon("trash", 13., t.text_muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let rid = rid_d.clone();
                        this.update_view(cx, move |v| v.colors.retain(|x| x.id != rid));
                    })),
            );
            rows = rows.child(line);
        }
        let full = n >= MAX_COLOR_RULES;
        panel(t)
            .min_w(px(560.))
            .pb(px(8.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(12.))
                    .pt(px(8.))
                    .pb(px(8.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("填色"))
                    .child(div().flex_1())
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("{n} / {MAX_COLOR_RULES} 条 · 越靠上优先级越高"))),
            )
            .when(n == 0, |d| {
                d.child(div().px(px(12.)).pb(px(8.)).text_size(px(12.5)).text_color(t.text_muted).child("按条件给单元格、整行或整列填色，只在表格视图生效"))
            })
            .child(rows)
            .child(div().px(px(8.)).pt(px(6.)).child(
                menu_item("clr-add", Some("plus"), "添加条件", t).text_color(if full { t.text_muted } else { t.accent }).w(px(120.)).on_click(cx.listener(
                    move |this, _, _, cx| {
                        if full {
                            this.show_toast(&format!("最多 {MAX_COLOR_RULES} 条填色规则"), cx);
                            return;
                        }
                        let Some(f) = this.visible().into_iter().nth(1).or_else(|| this.visible().into_iter().next()) else { return };
                        let color = (this.view().colors.len() % OPT_COLORS) as u8;
                        this.update_view(cx, |v| {
                            v.colors.push(ColorRule {
                                id: new_id("clr"),
                                kind: 0,
                                field: f.id.clone(),
                                cond: Some(Cond { field: f.id.clone(), op: Op::NotEmpty, value: Value::Null }),
                                color,
                            })
                        });
                    },
                )),
            ))
            .into_any_element()
    }

    // ───────────── 批量修改（R11） ─────────────

    pub(super) fn batch_panel(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let n = self.checked.len();
        let fid = self.scratch.get("batch_field").cloned().unwrap_or_default();
        let f = self.field(&fid);
        let mut body = div().flex().flex_col().gap(px(10.)).px(px(14.)).pb(px(12.));
        body = body.child(div().text_size(px(12.)).text_color(t.text_muted).child("要修改的字段")).child(
            select_box("bt-f", f.as_ref().map(|f| f.name.clone()).unwrap_or("选择字段".into()), f.as_ref().map(|f| f.kind.icon()), t).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    let tid = this.tid.clone();
                    let items = this.field_items_of(&tid, |f| !f.kind.computed() && f.kind != FieldType::Attachment);
                    this.open_pick_cb(
                        items,
                        vec![],
                        false,
                        ev.position,
                        |this, id, cx| {
                            this.scratch.insert("batch_field".into(), id);
                            this.scratch.remove("batch_value");
                            cx.notify();
                        },
                        cx,
                    );
                }),
            ),
        );
        if let Some(f) = &f {
            body = body.child(div().text_size(px(12.)).text_color(t.text_muted).child("改为"));
            if f.kind.has_options() {
                let cur = self.scratch.get("batch_value").cloned().unwrap_or_default();
                let names: Vec<String> = cur.split('\n').filter_map(|id| f.opt(id).map(|o| o.name.clone())).collect();
                let f2 = f.clone();
                body = body.child(select_box("bt-v", if names.is_empty() { "清空".into() } else { names.join("、") }, None, t).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let items = f2
                            .prop
                            .options
                            .iter()
                            .map(|o| PickItem { id: o.id.clone(), label: o.name.clone(), icon: None, color: Some(o.color), group: None })
                            .collect();
                        let multi = f2.kind == FieldType::MultiSelect;
                        this.open_pick_cb(
                            items,
                            vec![],
                            multi,
                            ev.position,
                            |this, ids, cx| {
                                this.scratch.insert("batch_value".into(), ids);
                                cx.notify();
                            },
                            cx,
                        );
                    }),
                ));
            } else if f.kind == FieldType::Checkbox {
                let on = self.scratch.get("batch_value").map(|v| v == "1").unwrap_or(false);
                body = body.child(div().id("bt-cb").flex().items_center().gap(px(8.)).cursor_pointer().child(check_box(on, t)).child("勾选").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.scratch.insert("batch_value".into(), if on { "0" } else { "1" }.into());
                        cx.notify();
                    }),
                ));
            } else {
                let inp = self.cb_input(
                    "batch",
                    "新的值（留空为清空）",
                    "",
                    |this, text, _| {
                        this.scratch.insert("batch_value".into(), text);
                    },
                    window,
                    cx,
                );
                body = body.child(input_box(t).child(inp));
            }
        }
        panel(t)
            .w(px(320.))
            .child(div().px(px(14.)).pt(px(10.)).pb(px(8.)).text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(format!("批量修改 {n} 条记录")))
            .child(body)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .px(px(14.))
                    .pt(px(6.))
                    .pb(px(10.))
                    .border_t_1()
                    .border_color(t.border)
                    .child(div().flex_1())
                    .child(ghost_btn("bt-cancel", "取消", t).on_click(cx.listener(|this, _, _, cx| {
                        this.close_popups();
                        cx.notify();
                    })))
                    .child(primary_btn("bt-ok", "应用", t).on_click(cx.listener(|this, _, _, cx| this.apply_batch(cx)))),
            )
            .into_any_element()
    }

    fn apply_batch(&mut self, cx: &mut Context<Self>) {
        let Some(mut f) = self.scratch.get("batch_field").and_then(|id| self.field(id)) else { return };
        let raw = self.scratch.get("batch_value").cloned().unwrap_or_default();
        let v = match f.kind {
            k if k.has_options() => {
                let ids: Vec<String> = raw.split('\n').filter(|s| !s.is_empty()).map(String::from).collect();
                if k == FieldType::MultiSelect {
                    Value::from(ids)
                } else {
                    ids.first().map(|s| Value::from(s.clone())).unwrap_or(Value::Null)
                }
            }
            FieldType::Checkbox => Value::Bool(raw == "1"),
            FieldType::Text | FieldType::Url | FieldType::Phone | FieldType::Email => Value::from(raw.trim()),
            FieldType::Link | FieldType::DuplexLink => {
                let target = f.prop.table.clone();
                let b = self.b();
                Value::from(
                    raw.split([',', '，', '、'])
                        .filter_map(|n| b.table(&target)?.order.iter().find(|r| b.primary_text(&target, r) == n.trim()).cloned())
                        .collect::<Vec<_>>(),
                )
            }
            _ => super::io::text_to_value(&mut f, &raw).unwrap_or(Value::Null),
        };
        let tid = self.tid.clone();
        let rids: Vec<String> = self.checked.iter().cloned().collect();
        for r in &rids {
            self.bm().set_cell(&tid, r, &f.id, v.clone());
        }
        self.commit(cx);
        self.close_popups();
        self.show_toast(&format!("已修改 {} 条记录的「{}」", rids.len(), f.name), cx);
    }

    // ───────────── 删除字段确认（R6） ─────────────

    pub(super) fn del_field_panel(&mut self, fid: String, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let name = self.field(&fid).map(|f| f.name).unwrap_or_default();
        let uses = self.b().field_usages(&self.tid, &fid);
        panel(t)
            .w(px(360.))
            .child(div().px(px(14.)).pt(px(10.)).pb(px(6.)).text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(format!("删除字段「{name}」？")))
            .child(div().px(px(14.)).pb(px(8.)).text_size(px(12.5)).text_color(t.text_secondary).child(if uses.is_empty() {
                "没有其他地方用到这个字段，可以放心删除（⌘Z 可撤销）".to_string()
            } else {
                "以下地方用到了这个字段，删除后会失效：".to_string()
            }))
            .child(div().id("df-list").max_h(px(200.)).overflow_y_scroll().px(px(14.)).flex().flex_col().gap(px(4.)).children(
                uses.into_iter().map(|u| div().flex().items_center().gap(px(6.)).text_size(px(12.5)).child(icon("info", 12., t.fg_palette[1])).child(u)),
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(10.))
                    .child(div().flex_1())
                    .child(ghost_btn("df-cancel", "取消", t).on_click(cx.listener(|this, _, _, cx| {
                        this.close_popups();
                        cx.notify();
                    })))
                    .child(primary_btn("df-ok", "删除", t).bg(t.fg_palette[1]).on_click(cx.listener(move |this, _, _, cx| {
                        this.close_popups();
                        this.delete_field(&fid, cx);
                    }))),
            )
            .into_any_element()
    }

    // ───────────── 字段编组（R6） ─────────────

    fn put_groups(&mut self, cx: &mut Context<Self>, merge: bool, f: impl FnOnce(&mut Vec<FieldGroup>)) {
        let tid = self.tid.clone();
        let Some(mut m) = self.b().table(&tid).map(|t| t.meta.clone()) else { return };
        f(&mut m.groups);
        m.groups.iter_mut().for_each(|g| g.fields.retain(|x| m.fields.contains(x)));
        // 编组内的字段在字段顺序里保持相邻
        let mut order: Vec<String> = vec![];
        for x in &m.fields {
            if order.contains(x) {
                continue;
            }
            match m.groups.iter().find(|g| g.fields.contains(x)) {
                Some(g) if x != &m.fields[0] => order.extend(g.fields.iter().filter(|y| *y != &m.fields[0]).cloned()),
                _ => order.push(x.clone()),
            }
        }
        m.fields = order;
        self.bm().put_table(m);
        if merge {
            self.commit_merge(Some("groups"), cx)
        } else {
            self.commit(cx)
        }
    }

    pub(super) fn field_groups_panel(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let groups = self.b().table(&self.tid).map(|t| t.meta.groups.clone()).unwrap_or_default();
        let mut list = div().id("fg-list").max_h(px(420.)).overflow_y_scroll().flex().flex_col().gap(px(10.)).px(px(12.));
        for (i, g) in groups.iter().enumerate() {
            let gid = g.id.clone();
            let gid2 = gid.clone();
            let inp = self.cb_input(
                &format!("fg:{gid}"),
                "编组名称",
                &g.name,
                move |this, text, cx| {
                    let gid = gid2.clone();
                    this.put_groups(cx, true, move |gs| {
                        if let Some(g) = gs.iter_mut().find(|g| g.id == gid) {
                            g.name = text;
                        }
                    });
                },
                window,
                cx,
            );
            let names: Vec<(String, String)> = g.fields.iter().filter_map(|f| self.field(f).map(|x| (x.id, x.name))).collect();
            let (gid3, gid4) = (gid.clone(), gid.clone());
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.border)
                    .child(
                        div().flex().flex_row().items_center().gap(px(6.)).child(input_box(t).flex_1().h(px(28.)).child(inp)).child(
                            div()
                                .id(("fg-del", i))
                                .size(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .cursor_pointer()
                                .hover(|s| s.bg(t.hover))
                                .child(icon("trash", 13., t.text_muted))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let gid = gid3.clone();
                                    this.put_groups(cx, false, move |gs| gs.retain(|g| g.id != gid));
                                })),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(4.))
                            .children(
                                names
                                    .into_iter()
                                    .map(|(_, n)| div().px(px(6.)).h(px(22.)).flex().items_center().rounded(px(4.)).bg(t.hover).text_size(px(12.)).child(n)),
                            )
                            .child(
                                div()
                                    .id(("fg-add", i))
                                    .px(px(6.))
                                    .h(px(22.))
                                    .flex()
                                    .items_center()
                                    .gap(px(2.))
                                    .text_size(px(12.))
                                    .text_color(t.accent)
                                    .cursor_pointer()
                                    .child(icon("plus", 12., t.accent))
                                    .child("选择字段")
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                            let tid = this.tid.clone();
                                            let groups = this.b().table(&tid).map(|t| t.meta.groups.clone()).unwrap_or_default();
                                            let primary = this.b().table(&tid).and_then(|t| t.meta.fields.first().cloned()).unwrap_or_default();
                                            let mine = groups.iter().find(|g| g.id == gid4).map(|g| g.fields.clone()).unwrap_or_default();
                                            let items = this.field_items_of(&tid, |f| {
                                                f.id != primary && (mine.contains(&f.id) || !groups.iter().any(|g| g.fields.contains(&f.id)))
                                            });
                                            let gid = gid4.clone();
                                            this.open_pick_cb(
                                                items,
                                                mine,
                                                true,
                                                ev.position,
                                                move |this, ids, cx| {
                                                    let gid = gid.clone();
                                                    this.put_groups(cx, false, move |gs| {
                                                        if let Some(g) = gs.iter_mut().find(|g| g.id == gid) {
                                                            g.fields = ids.split('\n').filter(|s| !s.is_empty()).map(String::from).collect();
                                                        }
                                                    });
                                                },
                                                cx,
                                            );
                                        }),
                                    ),
                            ),
                    ),
            );
        }
        let full = groups.len() >= MAX_FIELD_GROUPS;
        panel(t)
            .w(px(380.))
            .pb(px(8.))
            .child(div().px(px(12.)).pt(px(8.)).pb(px(4.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("字段编组"))
            .child(div().px(px(12.)).pb(px(8.)).text_size(px(12.)).text_color(t.text_muted).child("编组内的字段始终相邻，表头上方显示编组名，所有视图共用"))
            .child(list)
            .child(div().px(px(8.)).pt(px(6.)).child(menu_item("fg-new", Some("plus"), "新建编组", t).text_color(t.accent).w(px(120.)).on_click(cx.listener(
                move |this, _, _, cx| {
                    if full {
                        return;
                    }
                    this.put_groups(cx, false, |gs| {
                        let n = gs.len() + 1;
                        gs.push(FieldGroup { id: new_id("grp"), name: format!("编组 {n}"), fields: vec![] });
                    });
                },
            ))))
            .into_any_element()
    }

    // ───────────── 导入 / 导出（R13） ─────────────

    pub(super) fn import_panel(&mut self, path: String, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let tname = self.b().table(&self.tid).map(|t| t.meta.name.clone()).unwrap_or_default();
        let fname = std::path::Path::new(&path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let (p1, p2) = (path.clone(), path.clone());
        panel(t)
            .w(px(320.))
            .child(section_label(format!("导入「{fname}」"), t))
            .child(menu_item("im-new", Some("plus"), "新建数据表（xlsx 每个工作表一张表）", t).on_click(cx.listener(move |this, _, _, cx| {
                this.close_popups();
                this.import_path(std::path::Path::new(&p1), false, cx);
            })))
            .child(menu_item("im-append", Some("row-below"), format!("追加到「{tname}」（按列名匹配字段）"), t).on_click(cx.listener(move |this, _, _, cx| {
                this.close_popups();
                this.import_path(std::path::Path::new(&p2), true, cx);
            })))
            .into_any_element()
    }

    pub(super) fn export_menu(&mut self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        panel(t)
            .w(px(260.))
            .child(menu_item("ex-csv", Some("download"), "当前视图 → CSV", t).on_click(cx.listener(|this, _, _, cx| this.export_csv(cx))))
            .child(menu_item("ex-xlsx", Some("download"), "当前视图 → Excel", t).on_click(cx.listener(|this, _, _, cx| this.export_xlsx(false, cx))))
            .child(
                menu_item("ex-all", Some("download"), "整个多维表格 → Excel（每表一个工作表）", t)
                    .on_click(cx.listener(|this, _, _, cx| this.export_xlsx(true, cx))),
            )
            .into_any_element()
    }

    pub(super) fn export_xlsx(&mut self, all: bool, cx: &mut Context<Self>) {
        self.close_popups();
        let sheets: Vec<(String, View)> = if all {
            let b = self.b();
            b.table_list()
                .iter()
                .filter_map(|t| {
                    t.views.get(&t.meta.active_view).or_else(|| t.meta.views.first().and_then(|v| t.views.get(v))).map(|v| (t.meta.id.clone(), v.clone()))
                })
                .collect()
        } else {
            vec![(self.tid.clone(), self.view())]
        };
        let name = if all {
            self.b().meta.name.clone()
        } else {
            format!("{}-{}", self.b().table(&self.tid).map(|t| t.meta.name.clone()).unwrap_or_default(), self.view().name)
        };
        let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
        let tmp = std::env::temp_dir().join(format!("stonebrook-export-{}.xlsx", crate::store::new_uuid()));
        let res = super::io::export_xlsx(&self.b(), &sheets, &tmp.to_string_lossy());
        if let Err(e) = res {
            self.show_toast(&format!("导出失败：{e}"), cx);
            return;
        }
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{name}.xlsx")));
        cx.spawn(async move |_: WeakEntity<BaseEditor>, cx: &mut AsyncApp| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            if std::fs::copy(&tmp, &path).is_ok() {
                cx.update(|cx| cx.reveal_path(&path)).ok();
            }
            std::fs::remove_file(&tmp).ok();
        })
        .detach();
    }

    // ───────────── 表单设置（R8） ─────────────

    pub(super) fn form_set_panel(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let v = self.view();
        let fc = v.form.clone();
        let fields = self.visible();
        let title = self.cb_input("form:title", "表单标题（默认为数据表名）", &fc.title, |this, text, cx| this.update_form(cx, |f| f.title = text), window, cx);
        let desc = self.cb_input("form:desc", "表单描述", &fc.desc, |this, text, cx| this.update_form(cx, |f| f.desc = text), window, cx);
        let done = self.cb_input("form:done", "提交成功后的提示", &fc.done_text, |this, text, cx| this.update_form(cx, |f| f.done_text = text), window, cx);
        let mut list = div().id("fs-list").max_h(px(360.)).overflow_y_scroll().flex().flex_col().gap(px(8.));
        let tid = self.tid.clone();
        for (i, f) in fields.iter().enumerate().filter(|(_, f)| !f.kind.computed() && f.kind != FieldType::Attachment && !f.kind.is_link()) {
            let req = fc.required.contains(&f.id);
            let (fid, fid2, fid3, fid4) = (f.id.clone(), f.id.clone(), f.id.clone(), f.id.clone());
            let note = self.cb_input(
                &format!("form:note:{}", f.id),
                "填写说明（默认用字段描述）",
                fc.notes.get(&f.id).map(|s| s.as_str()).unwrap_or(""),
                move |this, text, cx| {
                    let fid = fid2.clone();
                    this.update_form(cx, move |c| {
                        if text.trim().is_empty() {
                            c.notes.remove(&fid);
                        } else {
                            c.notes.insert(fid, text);
                        }
                    })
                },
                window,
                cx,
            );
            let def_init = fc.defaults.get(&f.id).map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).unwrap_or_default();
            let fk = f.clone();
            let def = self.cb_input(
                &format!("form:def:{}", f.id),
                "默认值",
                &def_init,
                move |this, text, cx| {
                    let mut fk = fk.clone();
                    let fid = fid3.clone();
                    let v = super::io::text_to_value(&mut fk, &text);
                    if fk.kind.has_options() {
                        let tid = this.tid.clone();
                        this.bm().put_field(&tid, fk.clone());
                    }
                    this.update_form(cx, move |c| match v {
                        Some(v) => {
                            c.defaults.insert(fid, v);
                        }
                        None => {
                            c.defaults.remove(&fid);
                        }
                    })
                },
                window,
                cx,
            );
            let cond = fc.show_if.get(&f.id).cloned();
            let prev: Vec<String> = fields[..i].iter().map(|x| x.id.clone()).collect();
            let mut row = div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(t.border)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .child(icon(f.kind.icon(), 14., t.text_muted))
                        .child(div().flex_1().text_size(px(13.)).child(f.name.clone()))
                        .child(
                            div()
                                .id(SharedString::from(format!("fs-req-{fid}")))
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(12.))
                                .cursor_pointer()
                                .child("必填")
                                .child(switch(req, t))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let fid = fid.clone();
                                    this.update_form(cx, move |c| {
                                        if c.required.contains(&fid) {
                                            c.required.retain(|x| x != &fid);
                                        } else {
                                            c.required.push(fid);
                                        }
                                    })
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(6.))
                        .child(input_box(t).flex_1().h(px(28.)).child(note))
                        .child(input_box(t).w(px(110.)).h(px(28.)).child(def)),
                );
            match cond {
                Some(c) => {
                    let fid = fid4.clone();
                    let set: CondSet = Rc::new(move |this, c, cx| {
                        let fid = fid.clone();
                        this.update_form(cx, move |f| {
                            f.show_if.insert(fid, c);
                        })
                    });
                    let fid5 = fid4.clone();
                    row = row.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(div().text_size(px(12.)).text_color(t.text_muted).child("仅当"))
                            .child(self.cond_editor(&format!("fsc{i}"), &tid, &c, set, t, window, cx))
                            .child(div().id(SharedString::from(format!("fs-cx-{fid5}"))).cursor_pointer().child(icon("x", 12., t.text_muted)).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    let fid = fid5.clone();
                                    this.update_form(cx, move |f| {
                                        f.show_if.remove(&fid);
                                    })
                                }),
                            )),
                    );
                }
                None if !prev.is_empty() => {
                    let fid = fid4.clone();
                    let first = prev[0].clone();
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("fs-c-{fid}")))
                            .text_size(px(12.))
                            .text_color(t.accent)
                            .cursor_pointer()
                            .child("+ 设置显示条件（满足前面某题的答案时才显示）")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let Some(pf) = this.field(&first) else { return };
                                let fid = fid.clone();
                                this.update_form(cx, move |f| {
                                    f.show_if.insert(fid, Cond { field: pf.id.clone(), op: Op::NotEmpty, value: Value::Null });
                                })
                            })),
                    );
                }
                None => {}
            }
            list = list.child(row);
        }
        panel(t)
            .w(px(520.))
            .pb(px(10.))
            .child(div().px(px(14.)).pt(px(10.)).pb(px(8.)).text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("表单设置"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .px(px(14.))
                    .child(input_box(t).child(title))
                    .child(input_box(t).child(desc))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.))
                            .items_center()
                            .child(input_box(t).flex_1().child(done))
                            .child(
                                ghost_btn("fs-cover", if fc.cover.is_empty() { "添加封面图" } else { "更换封面图" }, t)
                                    .on_click(cx.listener(|this, _, _, cx| this.pick_form_cover(cx))),
                            )
                            .when(!fc.cover.is_empty(), |d| {
                                d.child(
                                    ghost_btn("fs-cover-x", "去掉封面", t).on_click(cx.listener(|this, _, _, cx| this.update_form(cx, |f| f.cover.clear()))),
                                )
                            }),
                    )
                    .child(div().pt(px(4.)).text_size(px(12.)).text_color(t.text_muted).child("每道题：必填 · 填写说明 · 默认值 · 显示条件"))
                    .child(list),
            )
            .into_any_element()
    }

    pub(super) fn update_form(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut FormCfg)) {
        let tid = self.tid.clone();
        let mut v = self.view();
        f(&mut v.form);
        self.bm().put_view(&tid, v);
        self.commit_merge(Some("form"), cx);
    }

    fn pick_form_cover(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择封面图".into()),
            ..Default::default()
        });
        cx.spawn(async move |this: WeakEntity<BaseEditor>, cx: &mut AsyncApp| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(p) = paths.first().cloned() else { return };
            let Some(assets) = cx.update(|_| crate::objects::store().map(|s| s.assets_dir())).ok().flatten() else { return };
            let att = cx.background_executor().spawn(async move { super::editor::import_file(&assets, &p) }).await;
            this.update(cx, |this, cx| {
                if let Some(a) = att {
                    let path = a.preview().to_string();
                    this.update_form(cx, |f| f.cover = path);
                }
            })
            .ok();
        })
        .detach();
    }
}

// ───────────── 字段编辑器的扩展部分 ─────────────

pub(super) const OUT_FMTS: [(&str, &str); 5] = [("", "自动"), ("num", "数字"), ("cur", "货币"), ("pct", "百分比"), ("date", "日期")];

impl BaseEditor {
    pub(super) fn draft_set(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Prop)) {
        if let Some(d) = self.draft.as_mut() {
            f(&mut d.prop);
            d.err = None;
        }
        cx.notify();
    }

    fn label(s: &'static str, t: &Theme) -> Div {
        div().text_size(px(12.)).text_color(t.text_muted).child(s)
    }

    /// 查找引用：经由关联字段 / 按条件查找（R3）。
    pub(super) fn lookup_editor(&mut self, prop: &Prop, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let filter_mode = !prop.lookup_table.is_empty();
        let seg = |id: &'static str, label: &'static str, on: bool, cx: &mut Context<Self>, to_filter: bool| {
            div()
                .id(id)
                .flex_1()
                .h(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.5))
                .cursor_pointer()
                .when(on, |d| d.bg(t.accent_soft).text_color(t.accent))
                .when(!on, |d| d.hover(|s| s.bg(t.hover)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let cur = this.tid.clone();
                    this.draft_set(cx, |p| {
                        p.target.clear();
                        p.conds.clear();
                        if to_filter {
                            p.link.clear();
                            p.lookup_table = cur;
                        } else {
                            p.lookup_table.clear();
                        }
                    })
                }))
        };
        let mut body = div().flex().flex_col().gap(px(8.));
        body = body.child(
            div()
                .flex()
                .flex_row()
                .rounded(px(6.))
                .border_1()
                .border_color(t.border_strong)
                .overflow_hidden()
                .child(seg("lk-m0", "通过关联字段", !filter_mode, cx, false))
                .child(seg("lk-m1", "按条件查找", filter_mode, cx, true)),
        );
        let target_table = if filter_mode { prop.lookup_table.clone() } else { self.field(&prop.link).map(|l| l.prop.table).unwrap_or_default() };
        if filter_mode {
            let tname = self.b().table(&prop.lookup_table).map(|t| t.meta.name.clone()).unwrap_or("选择数据表".into());
            body = body.child(Self::label("从哪张数据表查找", t)).child(select_box("lk-tb", tname, Some("v-grid"), t).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    let items = this.table_items();
                    this.open_pick_cb(
                        items,
                        vec![],
                        false,
                        ev.position,
                        |this, tid, cx| {
                            this.draft_set(cx, |p| {
                                p.lookup_table = tid;
                                p.target.clear();
                                p.conds.clear();
                            })
                        },
                        cx,
                    );
                }),
            ));
        } else {
            let link = self.field(&prop.link);
            body = body.child(Self::label("通过哪个关联字段", t)).child(
                select_box("fe-ll", link.as_ref().map(|l| l.name.clone()).unwrap_or("请选择关联字段".into()), Some("f-link"), t).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                        let tid = this.tid.clone();
                        let items = this.field_items_of(&tid, |f| f.kind.is_link());
                        if items.is_empty() {
                            this.show_toast("请先新建一个单向 / 双向关联字段，或改用「按条件查找」", cx);
                            return;
                        }
                        this.open_pick_cb(
                            items,
                            vec![],
                            false,
                            ev.position,
                            |this, id, cx| {
                                this.draft_set(cx, |p| {
                                    p.link = id;
                                    p.target.clear();
                                })
                            },
                            cx,
                        );
                    }),
                ),
            );
        }
        let target = self.b().field(&target_table, &prop.target).cloned();
        let tt = target_table.clone();
        body = body.child(Self::label("引用哪个字段", t)).child(
            select_box("fe-lf", target.as_ref().map(|f| f.name.clone()).unwrap_or("请选择字段".into()), target.as_ref().map(|f| f.kind.icon()), t)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let items = this.field_items_of(&tt, |_| true);
                        this.open_pick_cb(items, vec![], false, ev.position, |this, id, cx| this.draft_set(cx, |p| p.target = id), cx);
                    }),
                ),
        );
        if filter_mode {
            body = body.child(Self::label("查找条件（全部满足，最多 5 条）", t));
            for (i, c) in prop.conds.iter().enumerate() {
                body = body.child(self.lookup_cond_row(i, c, &target_table, t, window, cx));
            }
            if prop.conds.len() < 5 {
                let tt = target_table.clone();
                body = body.child(div().id("lk-cadd").text_size(px(12.5)).text_color(t.accent).cursor_pointer().child("+ 添加条件").on_click(cx.listener(
                    move |this, _, _, cx| {
                        let Some(f) = this.b().table(&tt).and_then(|t| t.primary().cloned()) else { return };
                        let cur = this.b().table(&this.tid).and_then(|t| t.primary().map(|p| p.id.clone())).unwrap_or_default();
                        this.draft_set(cx, |p| p.conds.push(LookupCond { field: f.id, op: Op::Is, rhs: cur, value: Value::Null }));
                    },
                )));
            }
        }
        let agg = calc::LOOKUP_AGGS.iter().find(|a| a.0 == prop.agg).map(|a| a.1).unwrap_or("原样引用");
        body = body.child(Self::label("汇总方式", t)).child(select_box("fe-la", agg, None, t).on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                let items = calc::LOOKUP_AGGS.iter().map(|(k, l)| PickItem::new(*k, *l)).collect();
                this.open_pick_cb(items, vec![], false, ev.position, |this, id, cx| this.draft_set(cx, |p| p.agg = id), cx);
            }),
        ));
        if matches!(prop.agg.as_str(), "sum" | "avg" | "max" | "min" | "count" | "count_unique") {
            body = body.child(self.out_format_editor(prop, t, cx));
        }
        body.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn lookup_cond_row(&mut self, i: usize, c: &LookupCond, target_table: &str, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tf = self.b().field(target_table, &c.field).cloned();
        let kind = tf.as_ref().map(|f| f.kind).unwrap_or(FieldType::Text);
        let tt = target_table.to_string();
        let ops: Vec<Op> = [Op::Is, Op::IsNot, Op::Contains, Op::Empty, Op::NotEmpty, Op::Gt, Op::Lt].into_iter().collect();
        let mut row = div().flex().flex_row().items_center().gap(px(6.));
        row = row.child(
            select_box(("lkc-f", i), tf.as_ref().map(|f| f.name.clone()).unwrap_or("对方字段".into()), tf.as_ref().map(|f| f.kind.icon()), t)
                .w(px(120.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let items = this.field_items_of(&tt, |_| true);
                        this.open_pick_cb(
                            items,
                            vec![],
                            false,
                            ev.position,
                            move |this, id, cx| {
                                this.draft_set(cx, |p| {
                                    if let Some(c) = p.conds.get_mut(i) {
                                        c.field = id;
                                    }
                                })
                            },
                            cx,
                        );
                    }),
                ),
        );
        row = row.child(select_box(("lkc-o", i), c.op.label(kind), None, t).w(px(84.)).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                let items = ops.iter().map(|o| PickItem::new(format!("{o:?}"), o.label(kind))).collect();
                this.open_pick_cb(
                    items,
                    vec![],
                    false,
                    ev.position,
                    move |this, id, cx| {
                        if let Some(op) = Op::ALL.into_iter().find(|o| format!("{o:?}") == id) {
                            this.draft_set(cx, |p| {
                                if let Some(c) = p.conds.get_mut(i) {
                                    c.op = op;
                                }
                            })
                        }
                    },
                    cx,
                );
            }),
        ));
        if c.op.needs_value() {
            let cur = self.field(&c.rhs);
            row = row.child(
                select_box(("lkc-r", i), cur.as_ref().map(|f| format!("当前表·{}", f.name)).unwrap_or("固定值".into()), None, t).w(px(120.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let tid = this.tid.clone();
                        let mut items = vec![PickItem::new("", "固定值")];
                        items.extend(this.field_items_of(&tid, |_| true));
                        this.open_pick_cb(
                            items,
                            vec![],
                            false,
                            ev.position,
                            move |this, id, cx| {
                                this.draft_set(cx, |p| {
                                    if let Some(c) = p.conds.get_mut(i) {
                                        c.rhs = id;
                                    }
                                })
                            },
                            cx,
                        );
                    }),
                ),
            );
            if cur.is_none() {
                let init = c.value.as_str().map(String::from).unwrap_or_else(|| if c.value.is_null() { String::new() } else { c.value.to_string() });
                let inp = self.cb_input(
                    &format!("fd:lkv{i}"),
                    "值",
                    &init,
                    move |this, text, cx| {
                        this.draft_set(cx, |p| {
                            if let Some(c) = p.conds.get_mut(i) {
                                c.value =
                                    text.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::from(text));
                            }
                        })
                    },
                    window,
                    cx,
                );
                row = row.child(input_box(t).w(px(90.)).child(inp));
            }
        }
        row.child(div().id(("lkc-x", i)).cursor_pointer().child(icon("x", 12., t.text_muted)).on_click(cx.listener(move |this, _, _, cx| {
            this.draft_set(cx, |p| {
                if i < p.conds.len() {
                    p.conds.remove(i);
                }
            })
        })))
        .into_any_element()
    }

    /// 结果格式（公式、查找引用的汇总值）。
    pub(super) fn out_format_editor(&mut self, prop: &Prop, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let label = OUT_FMTS.iter().find(|x| x.0 == prop.out).map(|x| x.1).unwrap_or("自动");
        let out = prop.out.clone();
        let sub: Option<(String, Vec<PickItem>)> = match prop.out.as_str() {
            "num" => Some((
                calc::NUM_FMTS[prop.fmt as usize % calc::NUM_FMTS.len()].to_string(),
                calc::NUM_FMTS.iter().enumerate().map(|(i, l)| PickItem::new(i.to_string(), *l)).collect(),
            )),
            "cur" | "pct" => Some((format!("{} 位小数", prop.fmt.min(4)), (0..5).map(|i| PickItem::new(i.to_string(), format!("{i} 位小数"))).collect())),
            "date" => Some((
                super::formula::DATE_FMTS[prop.fmt as usize % super::formula::DATE_FMTS.len()].to_string(),
                super::formula::DATE_FMTS.iter().enumerate().map(|(i, l)| PickItem::new(i.to_string(), *l)).collect(),
            )),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(Self::label("结果格式", t))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(select_box("fe-out", label, None, t).flex_1().on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                            let items = OUT_FMTS.iter().map(|(k, l)| PickItem::new(*k, *l)).collect();
                            this.open_pick_cb(
                                items,
                                vec![],
                                false,
                                ev.position,
                                |this, k, cx| {
                                    this.draft_set(cx, |p| {
                                        if k == "cur" && p.symbol.is_empty() {
                                            p.symbol = "¥".into();
                                            p.fmt = 2;
                                        } else {
                                            p.fmt = 0;
                                        }
                                        p.out = k;
                                    })
                                },
                                cx,
                            );
                        }),
                    ))
                    .when_some(sub, |d, (l, items)| {
                        d.child(select_box("fe-out2", l, None, t).flex_1().on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                this.open_pick_cb(
                                    items.clone(),
                                    vec![],
                                    false,
                                    ev.position,
                                    |this, k, cx| this.draft_set(cx, |p| p.fmt = k.parse().unwrap_or(0)),
                                    cx,
                                );
                            }),
                        ))
                    })
                    .when(out == "cur", |d| {
                        d.child(select_box("fe-out3", prop.symbol.clone(), None, t).w(px(70.)).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                                let items = calc::CURRENCIES.iter().map(|c| PickItem::new(*c, *c)).collect();
                                this.open_pick_cb(items, vec![], false, ev.position, |this, k, cx| this.draft_set(cx, |p| p.symbol = k), cx);
                            }),
                        ))
                    }),
            )
            .into_any_element()
    }

    /// 字段类型通用的扩展配置：默认值、选项引用 / 联动 / 排序 / 批量添加、数字单位、按钮。
    pub(super) fn field_extra(&mut self, kind: FieldType, prop: &Prop, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = vec![];
        use FieldType::*;
        if kind == Number {
            let inp = self.cb_input(
                "fd:unit",
                "例如 kg、件（可选）",
                &prop.unit,
                |this, text, cx| this.draft_set(cx, |p| p.unit = text.trim().to_string()),
                window,
                cx,
            );
            out.push(div().flex().flex_col().gap(px(6.)).child(Self::label("单位", t)).child(input_box(t).child(inp)).into_any_element());
        }
        if kind.has_options() && kind != Stage {
            // 选项管理：按字母排序、批量添加
            let batch = self.cb_input(
                "fd:optbatch",
                "批量添加，逗号/空格分隔",
                "",
                |this, text, _| {
                    this.scratch.insert("optbatch".into(), text);
                },
                window,
                cx,
            );
            out.push(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(6.))
                    .items_center()
                    .child(input_box(t).flex_1().h(px(28.)).child(batch))
                    .child(ghost_btn("fe-optb", "添加", t).h(px(28.)).on_click(cx.listener(|this, _, _, cx| {
                        let text = this.scratch.remove("optbatch").unwrap_or_default();
                        this.inputs.remove("cb:fd:optbatch");
                        this.draft_set(cx, |p| {
                            for n in text.split([',', '，', '、', ' ', '\n', ';', '；']).map(|x| x.trim()).filter(|x| !x.is_empty()) {
                                if !p.options.iter().any(|o| o.name == n) {
                                    let c = (p.options.len() % OPT_COLORS) as u8;
                                    p.options.push(SelOpt { id: new_id("opt"), name: n.into(), color: c });
                                }
                            }
                        })
                    })))
                    .child(
                        ghost_btn("fe-optsort", "A→Z", t)
                            .h(px(28.))
                            .on_click(cx.listener(|this, _, _, cx| this.draft_set(cx, |p| p.options.sort_by(|a, b| a.name.cmp(&b.name))))),
                    )
                    .into_any_element(),
            );
            // 选项引用：来自某表某字段的去重值，可加联动条件（R6）
            let tname = self.b().table(&prop.opt_table).map(|t| t.meta.name.clone());
            let fname = self.b().field(&prop.opt_table, &prop.opt_field).map(|f| f.name.clone());
            let ot = prop.opt_table.clone();
            let mut sec = div().flex().flex_col().gap(px(6.)).child(Self::label("选项引用（可选）：选项来自另一张表的某个字段", t)).child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(select_box("fe-ot", tname.unwrap_or("不引用".into()), Some("v-grid"), t).flex_1().on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                            let mut items = vec![PickItem::new("", "不引用（用本字段的选项）")];
                            items.extend(this.table_items());
                            this.open_pick_cb(
                                items,
                                vec![],
                                false,
                                ev.position,
                                |this, id, cx| {
                                    this.draft_set(cx, |p| {
                                        p.opt_table = id;
                                        p.opt_field.clear();
                                        p.conds.clear();
                                    })
                                },
                                cx,
                            );
                        }),
                    ))
                    .when(!prop.opt_table.is_empty(), |d| {
                        d.child(select_box("fe-of", fname.unwrap_or("选择字段".into()), None, t).flex_1().on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                let items = this.field_items_of(&ot, |_| true);
                                this.open_pick_cb(items, vec![], false, ev.position, |this, id, cx| this.draft_set(cx, |p| p.opt_field = id), cx);
                            }),
                        ))
                    }),
            );
            if !prop.opt_table.is_empty() {
                sec = sec.child(Self::label("联动条件（可选，最多 5 条）：编辑时只显示满足条件的选项", t));
                let target = prop.opt_table.clone();
                for (i, c) in prop.conds.iter().enumerate() {
                    sec = sec.child(self.lookup_cond_row(i, c, &target, t, window, cx));
                }
                if prop.conds.len() < 5 {
                    sec = sec.child(div().id("fe-ocadd").text_size(px(12.5)).text_color(t.accent).cursor_pointer().child("+ 添加联动条件").on_click(
                        cx.listener(move |this, _, _, cx| {
                            let Some(f) = this.b().table(&target).and_then(|t| t.primary().cloned()) else { return };
                            this.draft_set(cx, |p| p.conds.push(LookupCond { field: f.id, op: Op::Is, rhs: String::new(), value: Value::Null }));
                        }),
                    ));
                }
            }
            out.push(sec.into_any_element());
        }
        // 默认值（R6）
        if matches!(kind, Text | Number | Currency | Select | MultiSelect | Date | Checkbox | Person | Url | Phone | Email | Rating | Progress) {
            let el: AnyElement = match kind {
                Date => {
                    let on = prop.default.as_str() == Some("today");
                    div()
                        .id("fe-def")
                        .flex()
                        .flex_row()
                        .items_center()
                        .cursor_pointer()
                        .child(div().flex_1().text_size(px(13.)).child("默认值：今天"))
                        .child(switch(on, t))
                        .on_click(cx.listener(move |this, _, _, cx| this.draft_set(cx, |p| p.default = if on { Value::Null } else { Value::from("today") })))
                        .into_any_element()
                }
                Checkbox => {
                    let on = prop.default.as_bool().unwrap_or(false);
                    div()
                        .id("fe-def")
                        .flex()
                        .flex_row()
                        .items_center()
                        .cursor_pointer()
                        .child(div().flex_1().text_size(px(13.)).child("默认勾选"))
                        .child(switch(on, t))
                        .on_click(cx.listener(move |this, _, _, cx| this.draft_set(cx, |p| p.default = if on { Value::Null } else { Value::Bool(true) })))
                        .into_any_element()
                }
                Select | MultiSelect => {
                    let names: Vec<String> =
                        str_list(Some(&prop.default)).iter().filter_map(|id| prop.options.iter().find(|o| &o.id == id).map(|o| o.name.clone())).collect();
                    let opts = prop.options.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(Self::label("默认值", t))
                        .child(select_box("fe-def", if names.is_empty() { "无".into() } else { names.join("、") }, None, t).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                let mut items = vec![PickItem::new("", "无")];
                                items.extend(opts.iter().map(|o| PickItem {
                                    id: o.id.clone(),
                                    label: o.name.clone(),
                                    icon: None,
                                    color: Some(o.color),
                                    group: None,
                                }));
                                let multi = kind == MultiSelect;
                                this.open_pick_cb(
                                    items,
                                    vec![],
                                    multi,
                                    ev.position,
                                    move |this, ids, cx| {
                                        this.draft_set(cx, |p| {
                                            let ids: Vec<String> = ids.split('\n').filter(|s| !s.is_empty()).map(String::from).collect();
                                            p.default = if ids.is_empty() {
                                                Value::Null
                                            } else if multi {
                                                Value::from(ids)
                                            } else {
                                                Value::from(ids[0].clone())
                                            };
                                        })
                                    },
                                    cx,
                                );
                            }),
                        ))
                        .into_any_element()
                }
                _ => {
                    let init = match &prop.default {
                        Value::Null => String::new(),
                        Value::String(s) => s.clone(),
                        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("、"),
                        Value::Number(n) if kind == Progress => super::formula::fmt_num(n.as_f64().unwrap_or(0.) * 100.) + "%",
                        v => v.to_string(),
                    };
                    let inp = self.cb_input(
                        "fd:default",
                        "新记录自动填入（可选）",
                        &init,
                        move |this, text, cx| {
                            let mut f = Field::new("", kind);
                            let v = super::io::text_to_value(&mut f, &text).unwrap_or(Value::Null);
                            this.draft_set(cx, |p| p.default = v)
                        },
                        window,
                        cx,
                    );
                    div().flex().flex_col().gap(px(6.)).child(Self::label("默认值", t)).child(input_box(t).child(inp)).into_any_element()
                }
            };
            out.push(el);
        }
        if kind == Button {
            out.push(self.button_editor(prop, t, window, cx));
        }
        out
    }

    /// 按钮字段（R16）：文字、颜色、打开链接、修改字段、触发自动化。
    fn button_editor(&mut self, prop: &Prop, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let label = self.cb_input("fd:btn:label", "按钮文字", &prop.label, |this, text, cx| this.draft_set(cx, |p| p.label = text), window, cx);
        let url =
            self.cb_input("fd:btn:url", "点击打开的链接，可用 {{[字段名]}}", &prop.url, |this, text, cx| this.draft_set(cx, |p| p.url = text), window, cx);
        let cur = prop.color;
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(Self::label("按钮文字与颜色", t))
            .child(div().flex().flex_row().gap(px(8.)).items_center().child(input_box(t).flex_1().child(label)).child(
                self.color_swatch("fe-bc", prop.color, t).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        this.open_color_pick(cur, ev.position, |this, c, cx| this.draft_set(cx, |p| p.color = c), cx)
                    }),
                ),
            ))
            .child(Self::label("点击后：打开链接", t))
            .child(input_box(t).child(url))
            .child(Self::label("点击后：修改当前记录的字段", t));
        let tid = self.tid.clone();
        for (i, st) in prop.sets.iter().enumerate() {
            let fname = self.field(&st.field).map(|f| f.name).unwrap_or("选择字段".into());
            let init = st.value.as_str().map(String::from).unwrap_or_else(|| if st.value.is_null() { String::new() } else { st.value.to_string() });
            let inp = self.cb_input(
                &format!("fd:btn:s{i}"),
                "值：文本 / today / now / {{[字段]}}",
                &init,
                move |this, text, cx| {
                    this.draft_set(cx, |p| {
                        if let Some(x) = p.sets.get_mut(i) {
                            x.value = match text.trim() {
                                "true" | "是" => Value::Bool(true),
                                "false" | "否" => Value::Bool(false),
                                _ => Value::from(text),
                            };
                        }
                    })
                },
                window,
                cx,
            );
            let tid2 = tid.clone();
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(6.))
                    .items_center()
                    .child(select_box(("fe-bsf", i), fname, None, t).w(px(130.)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            let items = this.field_items_of(&tid2, |f| !f.kind.computed() && f.kind != FieldType::Attachment);
                            this.open_pick_cb(
                                items,
                                vec![],
                                false,
                                ev.position,
                                move |this, id, cx| {
                                    this.draft_set(cx, |p| {
                                        if let Some(x) = p.sets.get_mut(i) {
                                            x.field = id;
                                        }
                                    })
                                },
                                cx,
                            );
                        }),
                    ))
                    .child(input_box(t).flex_1().child(inp))
                    .child(div().id(("fe-bsx", i)).cursor_pointer().child(icon("x", 12., t.text_muted)).on_click(cx.listener(move |this, _, _, cx| {
                        this.draft_set(cx, |p| {
                            if i < p.sets.len() {
                                p.sets.remove(i);
                            }
                        })
                    }))),
            );
        }
        body = body.child(
            div()
                .id("fe-bsadd")
                .text_size(px(12.5))
                .text_color(t.accent)
                .cursor_pointer()
                .child("+ 修改字段")
                .on_click(cx.listener(|this, _, _, cx| this.draft_set(cx, |p| p.sets.push(FieldSet { field: String::new(), value: Value::Null })))),
        );
        let autos: Vec<(String, String)> = self.b().meta.automations.iter().filter(|a| a.trigger == "button").map(|a| (a.id.clone(), a.name.clone())).collect();
        let aname = autos.iter().find(|a| a.0 == prop.auto).map(|a| a.1.clone()).unwrap_or("不触发".into());
        body.child(Self::label("点击后：触发自动化（触发方式为「点击按钮时」的自动化）", t))
            .child(select_box("fe-bauto", aname, Some("zap"), t).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let mut items = vec![PickItem::new("", "不触发")];
                    items.extend(autos.iter().map(|(id, n)| PickItem::new(id.clone(), n.clone()).icon("zap")));
                    this.open_pick_cb(items, vec![], false, ev.position, |this, id, cx| this.draft_set(cx, |p| p.auto = id), cx);
                }),
            ))
            .into_any_element()
    }
}
