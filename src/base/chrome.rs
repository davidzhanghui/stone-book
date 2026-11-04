//! 多维表格全屏界面的外框：顶栏、左侧数据表列表、视图标签、工具栏。

use super::editor::*;
use super::model::*;
use super::widgets::*;
use crate::theme::Theme;
use crate::ui::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;

impl BaseEditor {
    pub(super) fn render_topbar(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.b().meta.name.clone();
        let (can_undo, can_redo) = (self.b().can_undo(), self.b().can_redo());
        let rename = self.rename_input("base");
        let _ = window;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .pl(px(84.))
            .pr(px(12.))
            .h(px(46.))
            .flex_none()
            .border_b_1()
            .border_color(t.border)
            .bg(t.sidebar_bg)
            .child(div().size(px(24.)).rounded(px(6.)).bg(rgb(0x7f3bf5)).flex().items_center().justify_center().child(icon("base", 15., white())))
            .child(match rename {
                Some(i) => div().w(px(220.)).child(input_box(t).child(i)).into_any_element(),
                None => div()
                    .id("base-name")
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .child(name.clone())
                    .tooltip(tip("点击重命名"))
                    .on_click(cx.listener(move |this, _, w, cx| this.start_rename("base", &name, w, cx)))
                    .into_any_element(),
            })
            .child(div().w(px(8.)))
            .child(
                div()
                    .id("b-undo")
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .when(!can_undo, |d| d.opacity(0.4))
                    .child(icon("restore", 15., t.text_secondary))
                    .tooltip(tip("撤销 ⌘Z"))
                    .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
            )
            .child(
                div()
                    .id("b-redo")
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(t.hover))
                    .when(!can_redo, |d| d.opacity(0.4))
                    .child(icon("redo", 15., t.text_secondary))
                    .tooltip(tip("重做 ⇧⌘Z"))
                    .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
            )
            .child(div().flex_1())
            .child(div().text_size(px(12.)).text_color(t.text_muted).child("所有修改自动保存在本机"))
            .child(primary_btn("b-done", "完成", t).ml(px(8.)).on_click(cx.listener(|this, _, _, cx| {
                this.commit_cell_edit(cx);
                cx.emit(BaseEvent::Close);
            })))
    }

    pub(super) fn render_sidebar(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let tables: Vec<(String, String, usize)> = self.b().table_list().iter().map(|t| (t.meta.id.clone(), t.meta.name.clone(), t.records.len())).collect();
        let cur = if self.dash.is_some() { String::new() } else { self.tid.clone() };
        div()
            .w(px(208.))
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(t.border)
            .bg(t.sidebar_bg)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(12.))
                    .h(px(40.))
                    .text_size(px(12.))
                    .text_color(t.text_muted)
                    .child(div().flex_1().child("数据表"))
                    .child(
                        div()
                            .id("tbl-add")
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.hover))
                            .child(icon("plus", 14., t.text_secondary))
                            .tooltip(tip("新建数据表"))
                            .on_click(cx.listener(|this, _, _, cx| this.add_table(cx))),
                    ),
            )
            .child(div().id("tbl-list").flex_1().overflow_y_scroll().px(px(6.)).children(tables.into_iter().map(|(id, name, n)| {
                let on = id == cur;
                let rename = self.rename_input(&format!("table:{id}"));
                let (id1, id2, name2) = (id.clone(), id.clone(), name.clone());
                div()
                    .id(SharedString::from(format!("tbl-{id}")))
                    .group("tbl")
                    .h(px(34.))
                    .px(px(8.))
                    .mb(px(2.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(on, |d| d.bg(t.active).text_color(t.accent))
                    .when(!on, |d| d.hover(|s| s.bg(t.hover)))
                    .child(icon("v-grid", 15., if on { t.accent } else { t.text_secondary }))
                    .child(match rename {
                        Some(i) => div().flex_1().child(input_box(t).h(px(26.)).child(i)).into_any_element(),
                        None => div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name.clone()).into_any_element(),
                    })
                    .child(div().text_size(px(11.)).text_color(t.text_muted).group_hover("tbl", |s| s.invisible()).child(n.to_string()))
                    .on_click(cx.listener(move |this, ev: &ClickEvent, w, cx| {
                        if ev.click_count() >= 2 {
                            this.start_rename(&format!("table:{id1}"), &name2, w, cx);
                        } else {
                            this.dash = None;
                            this.switch_table(&id1, cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| this.open_popup(Popup::TableMenu(id2.clone()), ev.position, cx)),
                    )
            })))
            .child({
                let dashes: Vec<(String, String)> = self.b().meta.dashboards.iter().map(|d| (d.id.clone(), d.name.clone())).collect();
                let cur = self.dash.clone();
                div()
                    .flex()
                    .flex_col()
                    .px(px(6.))
                    .pb(px(6.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .px(px(6.))
                            .h(px(32.))
                            .text_size(px(12.))
                            .text_color(t.text_muted)
                            .child(div().flex_1().child("仪表盘"))
                            .child(
                                div()
                                    .id("dash-add")
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(t.hover))
                                    .child(icon("plus", 14., t.text_secondary))
                                    .tooltip(tip("新建仪表盘"))
                                    .on_click(cx.listener(|this, _, _, cx| this.add_dashboard(cx))),
                            ),
                    )
                    .children(dashes.into_iter().map(|(id, name)| {
                        let on = cur.as_deref() == Some(&id);
                        div()
                            .id(SharedString::from(format!("dsh-{id}")))
                            .h(px(32.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_size(px(13.))
                            .when(on, |d| d.bg(t.active).text_color(t.accent))
                            .when(!on, |d| d.hover(|s| s.bg(t.hover)))
                            .child(icon("dashboard", 15., if on { t.accent } else { t.text_secondary }))
                            .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_popups();
                                this.dash = Some(id.clone());
                                cx.notify();
                            }))
                    }))
            })
            .child(
                div().p(px(10.)).border_t_1().border_color(t.border).flex().flex_col().gap(px(4.)).child(
                    menu_item("tbl-import", Some("upload-file"), "导入 Excel / CSV", t)
                        .mx(px(0.))
                        .on_click(cx.listener(|this, _, _, cx| this.import_file_dialog(cx))),
                ),
            )
    }

    pub(super) fn render_view_tabs(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let views: Vec<(String, String, ViewKind, bool)> = {
            let b = self.b();
            b.table(&self.tid)
                .map(|t| t.meta.views.iter().filter_map(|v| t.views.get(v)).map(|v| (v.id.clone(), v.name.clone(), v.kind, v.locked)).collect())
                .unwrap_or_default()
        };
        let cur = self.vid.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .px(px(12.))
            .h(px(42.))
            .flex_none()
            .border_b_1()
            .border_color(t.border)
            .when(!self.sidebar && !self.embedded, |d| {
                d.child(
                    div()
                        .id("sb-open")
                        .size(px(26.))
                        .mr(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.hover))
                        .child(icon("sidebar", 15., t.text_secondary))
                        .tooltip(tip("显示数据表列表"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar = true;
                            cx.notify();
                        })),
                )
            })
            .when(self.sidebar && !self.embedded, |d| {
                d.child(
                    div()
                        .id("sb-close")
                        .size(px(26.))
                        .mr(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.hover))
                        .child(icon("chevron-left", 15., t.text_secondary))
                        .tooltip(tip("收起数据表列表"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar = false;
                            cx.notify();
                        })),
                )
            })
            .children(views.into_iter().map(|(id, name, kind, locked)| {
                let on = id == cur;
                let rename = self.rename_input(&format!("view:{id}"));
                let (id1, id2, name1) = (id.clone(), id.clone(), name.clone());
                let color: Hsla = rgb(kind.color()).into();
                div()
                    .id(SharedString::from(format!("view-{id}")))
                    .flex_none()
                    .h(px(30.))
                    .px(px(10.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(on, |d| d.bg(t.active).text_color(t.text).font_weight(FontWeight::MEDIUM))
                    .when(!on, |d| d.text_color(t.text_secondary).hover(|s| s.bg(t.hover)))
                    .child(icon(kind.icon(), 15., color))
                    .child(match rename {
                        Some(i) => div().w(px(120.)).child(input_box(t).h(px(24.)).child(i)).into_any_element(),
                        None => div().child(name.clone()).into_any_element(),
                    })
                    .when(locked, |d| d.child(icon("lock", 12., t.text_muted)))
                    .when(on, |d| {
                        d.child(
                            div()
                                .id("view-more")
                                .size(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .hover(|s| s.bg(t.hover))
                                .child(icon("chevron-down", 12., t.text_muted))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        this.open_popup(Popup::ViewMenu(id2.clone()), ev.position, cx);
                                    }),
                                ),
                        )
                    })
                    .on_click(cx.listener(move |this, ev: &ClickEvent, w, cx| {
                        if ev.click_count() >= 2 {
                            this.start_rename(&format!("view:{id1}"), &name1, w, cx);
                        } else {
                            this.switch_view(&id1, cx);
                        }
                    }))
            }))
            .child(
                div()
                    .id("view-add")
                    .flex_none()
                    .h(px(30.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(t.text_secondary)
                    .hover(|s| s.bg(t.hover))
                    .child(icon("plus", 14., t.text_secondary))
                    .child("新建视图")
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| this.open_popup(Popup::NewView, ev.position, cx))),
            )
            .when(self.embedded, |d| {
                d.child(div().flex_1()).child(tool_btn("emb-full", "maximize", "全屏", false, t).on_click(cx.listener(|this, _, _, cx| {
                    this.commit_cell_edit(cx);
                    cx.emit(BaseEvent::Fullscreen);
                })))
            })
    }

    pub(super) fn render_toolbar(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let v = self.view();
        let total = self.b().table(&self.tid).map(|t| t.records.len()).unwrap_or(0);
        let shown = self.rows().len();
        let nf = v.filter.conds.len();
        let ns = v.sorts.len();
        let ng = v.groups.len();
        let hidden = v.hidden.len();
        let search_input = if self.search_open { Some(self.input("search", "搜索记录", &self.search.clone(), false, window, cx)) } else { None };
        let replace_input = (self.replace_open && !self.search.trim().is_empty()).then(|| self.input("cb:keep:replace", "替换为", "", false, window, cx));
        let pop = |p: Popup| cx.listener(move |this: &mut BaseEditor, ev: &MouseDownEvent, _, cx| this.open_popup(p.clone(), ev.position, cx));
        let kind = v.kind;
        let mut bar = div().flex().flex_row().items_center().gap(px(2.)).px(px(12.)).h(px(42.)).flex_none().border_b_1().border_color(t.border);
        if kind != ViewKind::Form {
            bar = bar.child(
                div()
                    .id("tb-add")
                    .flex_none()
                    .h(px(28.))
                    .px(px(10.))
                    .mr(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(6.))
                    .bg(t.accent)
                    .text_color(white())
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .child(icon("plus", 14., white()))
                    .child("添加记录")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(rid) = this.add_record(vec![], None, cx) {
                            this.detail = Some(rid);
                            cx.notify();
                        }
                    })),
            );
        }
        match kind {
            ViewKind::Kanban => {
                let gname = v.group_field.as_ref().and_then(|f| self.field(f)).map(|f| f.name).unwrap_or("未设置".into());
                bar = bar.child(tool_btn("tb-kset", "v-kanban", format!("分组：{gname}"), false, t).on_mouse_down(MouseButton::Left, pop(Popup::KanbanSet)));
            }
            ViewKind::Calendar | ViewKind::Gantt => {
                bar = bar.child(
                    tool_btn(
                        "tb-dset",
                        if kind == ViewKind::Gantt { "gantt" } else { "calendar" },
                        if kind == ViewKind::Gantt { "甘特设置" } else { "日历设置" },
                        false,
                        t,
                    )
                    .on_mouse_down(MouseButton::Left, pop(Popup::DateSet)),
                );
            }
            _ => {}
        }
        let fields_label = match kind {
            ViewKind::Grid | ViewKind::Gantt => {
                if hidden > 0 {
                    format!("{hidden} 个隐藏字段")
                } else {
                    "字段配置".into()
                }
            }
            ViewKind::Form => "表单字段".into(),
            _ => "卡片配置".into(),
        };
        if kind != ViewKind::Calendar {
            bar = bar.child(
                tool_btn("tb-fields", if kind == ViewKind::Grid { "eye" } else { "layout" }, fields_label, hidden > 0 && kind == ViewKind::Grid, t)
                    .on_mouse_down(MouseButton::Left, pop(Popup::Fields)),
            );
        }
        if kind != ViewKind::Form {
            bar = bar.child(
                tool_btn("tb-filter", "filter", if nf > 0 { format!("{nf} 个筛选条件") } else { "筛选".into() }, nf > 0, t)
                    .on_mouse_down(MouseButton::Left, pop(Popup::Filter)),
            );
        }
        if matches!(kind, ViewKind::Grid | ViewKind::Gantt) {
            bar = bar.child(
                tool_btn("tb-group", "group-by", if ng > 0 { format!("{ng} 个分组") } else { "分组".into() }, ng > 0, t)
                    .on_mouse_down(MouseButton::Left, pop(Popup::Group)),
            );
        }
        if !matches!(kind, ViewKind::Calendar | ViewKind::Form) {
            bar = bar.child(
                tool_btn("tb-sort", "sort", if ns > 0 { format!("{ns} 个排序") } else { "排序".into() }, ns > 0, t)
                    .on_mouse_down(MouseButton::Left, pop(Popup::Sort)),
            );
        }
        if kind == ViewKind::Grid {
            bar = bar.child(tool_btn("tb-rowh", "rows", "行高", false, t).on_mouse_down(MouseButton::Left, pop(Popup::RowHeight)));
            let nc = v.colors.len();
            bar = bar.child(
                tool_btn("tb-color", "palette", if nc > 0 { format!("{nc} 条填色") } else { "填色".into() }, nc > 0, t)
                    .on_mouse_down(MouseButton::Left, pop(Popup::Colors)),
            );
        }
        if kind == ViewKind::Form {
            bar = bar.child(tool_btn("tb-formset", "settings", "表单设置", false, t).on_mouse_down(MouseButton::Left, pop(Popup::FormSet)));
        }
        if self.view_draft.as_ref().is_some_and(|d| d.id == self.vid) {
            bar = bar
                .child(div().ml(px(8.)).text_size(px(12.)).text_color(t.text_muted).child("视图配置有临时修改"))
                .child(tool_btn("tb-dsave", "check", "保存", true, t).on_click(cx.listener(|this, _, _, cx| this.save_view_draft(true, cx))))
                .child(tool_btn("tb-dreset", "restore", "还原", false, t).on_click(cx.listener(|this, _, _, cx| this.save_view_draft(false, cx))));
        }
        if matches!(kind, ViewKind::Kanban | ViewKind::Gallery) {
            bar = bar.child(
                tool_btn("tb-compact", "layout", if v.compact { "紧凑模式" } else { "标准模式" }, v.compact, t)
                    .on_click(cx.listener(|this, _, _, cx| this.update_view(cx, |v| v.compact = !v.compact))),
            );
        }
        bar = bar.child(div().flex_1());
        if kind != ViewKind::Form {
            bar = bar.child(div().text_size(px(12.)).text_color(t.text_muted).mr(px(8.)).child(if shown == total {
                format!("共 {total} 条记录")
            } else {
                format!("{shown} / {total} 条记录")
            }));
        }
        if !self.search.trim().is_empty() && kind == ViewKind::Grid {
            let hits = self.find_hits();
            let n = hits.len();
            let pos = if n == 0 { 0 } else { self.find_pos % n + 1 };
            bar = bar
                .child(div().text_size(px(12.)).text_color(t.text_muted).mr(px(4.)).child(format!("{pos} / {n} 处")))
                .child(
                    tool_btn("tb-fprev", "chevron-up", "", false, t).tooltip(tip("上一处 ⇧⏎")).on_click(cx.listener(|this, _, _, cx| this.find_step(-1, cx))),
                )
                .child(
                    tool_btn("tb-fnext", "chevron-down", "", false, t).tooltip(tip("下一处 ⏎")).on_click(cx.listener(|this, _, _, cx| this.find_step(1, cx))),
                )
                .child(tool_btn("tb-repl", "pencil", "替换", self.replace_open, t).on_click(cx.listener(|this, _, _, cx| {
                    this.replace_open = !this.replace_open;
                    cx.notify();
                })));
            if let Some(ri) = replace_input {
                bar = bar
                    .child(div().w(px(140.)).child(input_box(t).h(px(28.)).child(ri)))
                    .child(tool_btn("tb-replall", "check", "全部替换", false, t).on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))));
            }
        }
        bar = bar.child(match search_input {
            Some(i) => div().w(px(200.)).child(input_box(t).h(px(28.)).gap(px(6.)).child(icon("search", 14., t.text_muted)).child(i)).into_any_element(),
            None => tool_btn("tb-search", "search", "", false, t)
                .tooltip(tip("搜索 ⌘F"))
                .on_click(cx.listener(|this, _, w, cx| this.open_search(w, cx)))
                .into_any_element(),
        });
        bar.child(tool_btn("tb-more", "more", "", false, t).on_mouse_down(MouseButton::Left, pop(Popup::More)))
    }
}
