//! 自动化（R15）配置面板：左边是自动化列表，右边编辑触发条件和动作，底部是运行日志。
//! 执行引擎在 model.rs（提交时同步执行，修改并进同一步撤销）。

use super::config::CondSet;
use super::editor::{BaseEditor, PickItem};
use super::model::*;
use super::widgets::*;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;
use serde_json::Value;
use std::rc::Rc;

pub(super) const TRIGGERS: [(&str, &str); 5] =
    [("create", "新增记录时"), ("match", "记录满足条件时"), ("change", "字段被修改时"), ("schedule", "定时触发"), ("button", "点击按钮时")];
pub(super) const ACTIONS: [(&str, &str); 3] = [("update", "更新记录"), ("create", "新增记录"), ("notify", "发送系统通知")];

impl BaseEditor {
    pub(super) fn update_auto(&mut self, id: &str, cx: &mut Context<Self>, merge: bool, f: impl FnOnce(&mut Automation)) {
        let mut m = self.b().meta.clone();
        if let Some(a) = m.automations.iter_mut().find(|a| a.id == id) {
            f(a);
        }
        self.bm().set_meta(m);
        if merge {
            self.commit_merge(Some("auto"), cx)
        } else {
            self.commit(cx)
        }
    }

    pub(super) fn automations_panel(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let autos = self.b().meta.automations.clone();
        if self.auto_sel.as_ref().is_none_or(|s| !autos.iter().any(|a| &a.id == s)) {
            self.auto_sel = autos.first().map(|a| a.id.clone());
        }
        let sel = self.auto_sel.clone();
        let mut list =
            div().id("au-list").w(px(210.)).flex_none().border_r_1().border_color(t.border).flex().flex_col().gap(px(2.)).p(px(6.)).overflow_y_scroll();
        for a in &autos {
            let on = sel.as_deref() == Some(&a.id);
            let (id1, id2, enabled) = (a.id.clone(), a.id.clone(), a.on);
            list = list.child(
                div()
                    .id(SharedString::from(format!("au-{}", a.id)))
                    .h(px(34.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(t.active))
                    .when(!on, |d| d.hover(|s| s.bg(t.hover)))
                    .child(icon("zap", 14., if enabled { t.accent } else { t.text_muted }))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(13.)).child(a.name.clone()))
                    .child(div().id(SharedString::from(format!("au-on-{}", a.id))).child(switch(enabled, t)).on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.update_auto(&id2, cx, false, |a| a.on = !a.on);
                    })))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.auto_sel = Some(id1.clone());
                        cx.notify();
                    })),
            );
        }
        list = list.child(menu_item("au-new", Some("plus"), "新建自动化", t).mx(px(0.)).text_color(t.accent).on_click(cx.listener(|this, _, _, cx| {
            let tid = this.tid.clone();
            let mut m = this.b().meta.clone();
            let id = new_id("auto");
            let n = m.automations.len() + 1;
            m.automations.push(Automation {
                id: id.clone(),
                name: format!("自动化 {n}"),
                on: true,
                trigger: "create".into(),
                table: tid,
                actions: vec![AutoAction { kind: "notify".into(), text: "新增了记录：{{[".to_string() + "文本]}}", ..Default::default() }],
                ..Default::default()
            });
            this.bm().set_meta(m);
            this.commit(cx);
            this.auto_sel = Some(id);
        })));
        let editor: AnyElement = match sel.and_then(|s| autos.iter().find(|a| a.id == s).cloned()) {
            Some(a) => self.auto_editor(&a, t, window, cx),
            None => div()
                .p(px(24.))
                .text_size(px(13.))
                .text_color(t.text_muted)
                .child("还没有自动化。自动化在本机运行：记录新增、满足条件、字段修改、定时或点击按钮时，自动更新记录、新增记录或发系统通知。")
                .into_any_element(),
        };
        let log: Vec<AutoLog> = self.b().auto_log.iter().rev().take(8).cloned().collect();
        panel(t)
            .w(px(820.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(8.))
                    .border_b_1()
                    .border_color(t.border)
                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("自动化"))
                    .child(div().flex_1())
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("在本机运行 · 自动化产生的修改和触发它的修改是同一步撤销")),
            )
            .child(div().flex().flex_row().h(px(440.)).child(list).child(div().id("au-edit").flex_1().overflow_y_scroll().child(editor)))
            .child(
                div()
                    .border_t_1()
                    .border_color(t.border)
                    .px(px(14.))
                    .py(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("运行日志"))
                    .when(log.is_empty(), |d| d.child(div().text_size(px(12.)).text_color(t.text_muted).child("还没有运行记录")))
                    .children(log.into_iter().map(|l| {
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .child(div().text_color(t.text_muted).child(super::formula::fmt_date(l.at, 1)))
                            .child(div().child(l.name))
                            .child(div().text_color(if l.ok { t.text_secondary } else { t.fg_palette[1] }).child(l.msg))
                    })),
            )
            .into_any_element()
    }

    fn auto_editor(&mut self, a: &Automation, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = a.id.clone();
        let label = |s: &'static str| div().text_size(px(12.)).text_color(t.text_muted).child(s);
        let id_n = id.clone();
        let name =
            self.cb_input(&format!("au:name:{id}"), "名称", &a.name, move |this, text, cx| this.update_auto(&id_n, cx, true, |a| a.name = text), window, cx);
        let tname = self.b().table(&a.table).map(|t| t.meta.name.clone()).unwrap_or("选择数据表".into());
        let trig = TRIGGERS.iter().find(|x| x.0 == a.trigger).map(|x| x.1).unwrap_or("选择触发方式");
        let mut body = div().flex().flex_col().gap(px(8.)).p(px(14.));
        body = body.child(input_box(t).child(name));
        let (id_t, id_tb) = (id.clone(), id.clone());
        body = body.child(label("当")).child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.))
                .child(select_box("au-tb", tname, Some("v-grid"), t).w(px(160.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let items = this.table_items();
                        let id = id_tb.clone();
                        this.open_pick_cb(
                            items,
                            vec![],
                            false,
                            ev.position,
                            move |this, tid, cx| {
                                this.update_auto(&id, cx, false, |a| {
                                    a.table = tid;
                                    a.cond.conds.clear();
                                    a.field.clear();
                                })
                            },
                            cx,
                        );
                    }),
                ))
                .child(select_box("au-tr", trig, Some("zap"), t).w(px(160.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let items = TRIGGERS.iter().map(|(k, l)| PickItem::new(*k, *l)).collect();
                        let id = id_t.clone();
                        this.open_pick_cb(items, vec![], false, ev.position, move |this, k, cx| this.update_auto(&id, cx, false, |a| a.trigger = k), cx);
                    }),
                )),
        );
        if a.trigger == "change" {
            let fname = self.b().field(&a.table, &a.field).map(|f| f.name.clone()).unwrap_or("选择字段".into());
            let (id_f, tb) = (id.clone(), a.table.clone());
            body = body.child(select_box("au-fd", format!("字段：{fname}"), None, t).w(px(220.)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let items = this.field_items_of(&tb, |_| true);
                    let id = id_f.clone();
                    this.open_pick_cb(items, vec![], false, ev.position, move |this, f, cx| this.update_auto(&id, cx, false, |a| a.field = f), cx);
                }),
            ));
        }
        if a.trigger == "schedule" {
            let id_s = id.clone();
            let inp = self.cb_input(
                &format!("au:sch:{id}"),
                "daily 09:00 / weekly 1 09:00 / monthly 1 09:00",
                &a.schedule,
                move |this, text, cx| this.update_auto(&id_s, cx, true, |a| a.schedule = text.trim().to_string()),
                window,
                cx,
            );
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(input_box(t).w(px(260.)).child(inp))
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child("应用打开时按时执行")),
            );
        }
        if a.trigger == "button" {
            body = body.child(div().text_size(px(12.)).text_color(t.text_muted).child("在按钮字段的设置里选择这条自动化；点击按钮时对那条记录执行下面的动作"));
        }
        if a.trigger != "button" {
            body = body.child(label(if a.trigger == "match" {
                "满足以下全部条件（从不满足变为满足时触发一次）"
            } else {
                "并且满足（可选）"
            }));
            for (i, c) in a.cond.conds.iter().enumerate() {
                let id_c = id.clone();
                let set: CondSet = Rc::new(move |this, c, cx| {
                    this.update_auto(&id_c, cx, true, move |a| {
                        if let Some(x) = a.cond.conds.get_mut(i) {
                            *x = c;
                        }
                    })
                });
                let ed = self.cond_editor(&format!("au{id}c{i}"), &a.table.clone(), c, set, t, window, cx);
                let id_d = id.clone();
                body = body.child(div().flex().flex_row().items_center().gap(px(6.)).child(ed).child(
                    div().id(("au-cdel", i)).cursor_pointer().child(icon("x", 12., t.text_muted)).on_click(cx.listener(move |this, _, _, cx| {
                        this.update_auto(&id_d, cx, false, |a| {
                            if i < a.cond.conds.len() {
                                a.cond.conds.remove(i);
                            }
                        })
                    })),
                ));
            }
            let (id_a, tb) = (id.clone(), a.table.clone());
            body = body.child(div().id("au-cadd").text_size(px(12.5)).text_color(t.accent).cursor_pointer().child("+ 添加条件").on_click(cx.listener(
                move |this, _, _, cx| {
                    let Some(f) = this.b().table(&tb).and_then(|t| t.primary().cloned()) else { return };
                    this.update_auto(&id_a, cx, false, |a| a.cond.conds.push(super::config::blank_cond(&f)));
                },
            )));
        }
        body = body.child(div().h(px(4.))).child(label("就执行"));
        for (i, act) in a.actions.iter().enumerate() {
            body = body.child(self.action_editor(&id, a, i, act, t, window, cx));
        }
        let id_add = id.clone();
        let id_del = id.clone();
        body = body
            .child(div().id("au-aadd").text_size(px(12.5)).text_color(t.accent).cursor_pointer().child("+ 添加动作").on_click(cx.listener(
                move |this, _, _, cx| this.update_auto(&id_add, cx, false, |a| a.actions.push(AutoAction { kind: "update".into(), ..Default::default() })),
            )))
            .child(div().pt(px(10.)).child(ghost_btn("au-del", "删除这条自动化", t).text_color(t.fg_palette[1]).on_click(cx.listener(
                move |this, _, _, cx| {
                    let mut m = this.b().meta.clone();
                    m.automations.retain(|a| a.id != id_del);
                    this.bm().set_meta(m);
                    this.commit(cx);
                },
            ))));
        body.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn action_editor(&mut self, id: &str, a: &Automation, i: usize, act: &AutoAction, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let kind = ACTIONS.iter().find(|x| x.0 == act.kind).map(|x| x.1).unwrap_or("选择动作");
        let id_k = id.to_string();
        let mut card = div().flex().flex_col().gap(px(6.)).p(px(8.)).rounded(px(8.)).border_1().border_color(t.border);
        let id_x = id.to_string();
        card = card.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(select_box(("au-ak", i), kind, None, t).w(px(150.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let items = ACTIONS.iter().map(|(k, l)| PickItem::new(*k, *l)).collect();
                        let id = id_k.clone();
                        this.open_pick_cb(
                            items,
                            vec![],
                            false,
                            ev.position,
                            move |this, k, cx| {
                                this.update_auto(&id, cx, false, move |a| {
                                    if let Some(x) = a.actions.get_mut(i) {
                                        x.kind = k;
                                        x.sets.clear();
                                    }
                                })
                            },
                            cx,
                        );
                    }),
                ))
                .child(div().flex_1())
                .child(div().id(("au-adel", i)).cursor_pointer().child(icon("trash", 13., t.text_muted)).on_click(cx.listener(move |this, _, _, cx| {
                    this.update_auto(&id_x, cx, false, |a| {
                        if i < a.actions.len() {
                            a.actions.remove(i);
                        }
                    })
                }))),
        );
        let target = if act.kind == "create" && !act.table.is_empty() { act.table.clone() } else { a.table.clone() };
        match act.kind.as_str() {
            "notify" => {
                let id_n = id.to_string();
                let inp = self.cb_input(
                    &format!("au:{id}:n{i}"),
                    "通知内容，可用 {{[字段名]}} 引用记录的字段",
                    &act.text,
                    move |this, text, cx| {
                        this.update_auto(&id_n, cx, true, move |a| {
                            if let Some(x) = a.actions.get_mut(i) {
                                x.text = text;
                            }
                        })
                    },
                    window,
                    cx,
                );
                card = card.child(input_box(t).child(inp));
            }
            k => {
                if k == "create" {
                    let tname = self.b().table(&target).map(|t| t.meta.name.clone()).unwrap_or_default();
                    let id_t = id.to_string();
                    card = card.child(select_box(("au-at", i), format!("新增到：{tname}"), Some("v-grid"), t).w(px(220.)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            let items = this.table_items();
                            let id = id_t.clone();
                            this.open_pick_cb(
                                items,
                                vec![],
                                false,
                                ev.position,
                                move |this, tb, cx| {
                                    this.update_auto(&id, cx, false, move |a| {
                                        if let Some(x) = a.actions.get_mut(i) {
                                            x.table = tb;
                                            x.sets.clear();
                                        }
                                    })
                                },
                                cx,
                            );
                        }),
                    ));
                }
                for (j, st) in act.sets.iter().enumerate() {
                    let fname = self.b().field(&target, &st.field).map(|f| f.name.clone()).unwrap_or("选择字段".into());
                    let (id_f, id_v, tb) = (id.to_string(), id.to_string(), target.clone());
                    let init = st.value.as_str().map(String::from).unwrap_or_else(|| if st.value.is_null() { String::new() } else { st.value.to_string() });
                    let inp = self.cb_input(
                        &format!("au:{id}:s{i}:{j}"),
                        "值：文本 / today / {{[字段]}}",
                        &init,
                        move |this, text, cx| {
                            this.update_auto(&id_v, cx, true, move |a| {
                                if let Some(x) = a.actions.get_mut(i).and_then(|x| x.sets.get_mut(j)) {
                                    x.value = Value::from(text);
                                }
                            })
                        },
                        window,
                        cx,
                    );
                    card = card.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(select_box(SharedString::from(format!("au-sf-{i}-{j}")), fname, None, t).w(px(140.)).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                    let items = this.field_items_of(&tb, |f| !f.kind.computed() && f.kind != FieldType::Attachment);
                                    let id = id_f.clone();
                                    this.open_pick_cb(
                                        items,
                                        vec![],
                                        false,
                                        ev.position,
                                        move |this, f, cx| {
                                            this.update_auto(&id, cx, false, move |a| {
                                                if let Some(x) = a.actions.get_mut(i).and_then(|x| x.sets.get_mut(j)) {
                                                    x.field = f;
                                                }
                                            })
                                        },
                                        cx,
                                    );
                                }),
                            ))
                            .child(div().text_size(px(12.)).text_color(t.text_muted).child("设为"))
                            .child(input_box(t).flex_1().child(inp)),
                    );
                }
                let id_s = id.to_string();
                card = card.child(div().id(("au-sadd", i)).text_size(px(12.)).text_color(t.accent).cursor_pointer().child("+ 设置字段").on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.update_auto(&id_s, cx, false, move |a| {
                            if let Some(x) = a.actions.get_mut(i) {
                                x.sets.push(FieldSet { field: String::new(), value: Value::Null });
                            }
                        })
                    },
                )));
            }
        }
        card.into_any_element()
    }
}
