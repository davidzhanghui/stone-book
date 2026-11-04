//! 快捷键面板（⌘/）：按分组列出可改的动作和当前按键；点一行录制新按键，冲突时拒绝并提示。

use super::*;
use crate::shortcuts;

pub struct ShortcutsPanel {
    focus: FocusHandle,
    /// 正在录制的动作
    recording: Option<&'static str>,
    message: Option<String>,
}

impl Workspace {
    pub fn show_shortcuts(&mut self, _: &ShowShortcuts, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcuts_panel.take().is_some() {
            window.focus(&self.editor.focus_handle(cx));
            cx.notify();
            return;
        }
        let focus = cx.focus_handle();
        window.focus(&focus);
        self.shortcuts_panel = Some(ShortcutsPanel { focus, recording: None, message: None });
        self.more_menu = false;
        self.palette = None;
        cx.notify();
    }

    fn close_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts_panel = None;
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }

    /// 录制中的按键：在捕获阶段拦下，不让它触发任何动作。
    fn shortcuts_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.shortcuts_panel.as_mut() else { return };
        let k = &ev.keystroke;
        let Some(action) = panel.recording else {
            if k.key == "escape" && k.modifiers == Modifiers::none() {
                cx.stop_propagation();
                self.close_shortcuts(window, cx);
            }
            return;
        };
        cx.stop_propagation();
        if k.key == "escape" && k.modifiers == Modifiers::none() {
            panel.recording = None;
            panel.message = None;
            cx.notify();
            return;
        }
        if !shortcuts::recordable(k) {
            panel.message = Some("请按带 ⌘ / ⌃ / ⌥ 的组合键或 F1–F12；Esc 取消".into());
            cx.notify();
            return;
        }
        let key = k.unparse();
        if let Some(other) = shortcuts::conflict(action, &key, cx) {
            panel.message = Some(if crate::i18n::english() {
                format!("{} is already used by “{}”", shortcuts::display(&key), crate::i18n::tr(other))
            } else {
                format!("{} 已被「{other}」使用，请换一个", shortcuts::display(&key))
            });
            cx.notify();
            return;
        }
        let result = shortcuts::set(action, Some(key.clone()), cx);
        if let Some(panel) = self.shortcuts_panel.as_mut() {
            panel.recording = None;
            panel.message = Some(match result {
                Ok(()) => format!("已设为 {}", shortcuts::display(&key)),
                Err(e) => e,
            });
        }
        crate::refresh_menus(cx);
        cx.notify();
    }

    pub(super) fn render_shortcuts_panel(&self, t: &Theme, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let panel = self.shortcuts_panel.as_ref()?;
        let mut groups: Vec<(&str, Vec<(&'static str, &'static str)>)> = Vec::new();
        for (action, group, label) in shortcuts::ITEMS {
            match groups.iter_mut().find(|(g, _)| g == group) {
                Some((_, items)) => items.push((action, label)),
                None => groups.push((group, vec![(action, label)])),
            }
        }
        let key_pill = |text: String, t: &Theme| {
            div()
                .px(px(6.))
                .py(px(1.))
                .rounded(px(4.))
                .bg(t.hover)
                .border_1()
                .border_color(t.border)
                .text_size(px(12.))
                .text_color(t.text_secondary)
                .child(text)
        };
        let any_custom = shortcuts::ITEMS.iter().any(|(a, _, _)| shortcuts::is_custom(a, cx));
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_center()
                .pt(px(70.))
                .bg(hsla(0., 0., 0., 0.12))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.close_shortcuts(window, cx)))
                .child(
                    div()
                        .id("shortcuts-panel")
                        .track_focus(&panel.focus)
                        .capture_key_down(cx.listener(Self::shortcuts_key))
                        .w(px(620.))
                        .max_h(px(680.))
                        .flex()
                        .flex_col()
                        .bg(t.panel_bg)
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(12.))
                        .shadow_lg()
                        .overflow_hidden()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .px_4()
                                .h(px(52.))
                                .border_b_1()
                                .border_color(t.border)
                                .child(icon("command", 16., t.text_muted))
                                .child(div().flex_1().text_size(px(15.)).text_color(t.text).child(crate::i18n::tr("快捷键")))
                                .child(div().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("点一行即可修改")))
                                .when(any_custom, |d| {
                                    d.child(
                                        div()
                                            .id("sc-reset-all")
                                            .ml_2()
                                            .px_2()
                                            .py(px(3.))
                                            .rounded(px(6.))
                                            .text_size(px(12.))
                                            .text_color(t.accent)
                                            .cursor_pointer()
                                            .hover(|s| s.bg(t.hover))
                                            .child(crate::i18n::tr("全部恢复默认"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let r = shortcuts::reset_all(cx);
                                                crate::refresh_menus(cx);
                                                if let Some(p) = this.shortcuts_panel.as_mut() {
                                                    p.recording = None;
                                                    p.message = Some(r.map(|_| "已全部恢复默认".to_string()).unwrap_or_else(|e| e));
                                                }
                                                cx.notify();
                                            })),
                                    )
                                }),
                        )
                        .when_some(panel.message.clone(), |d, m| {
                            d.child(div().px_4().py(px(8.)).text_size(px(12.)).text_color(t.text_secondary).bg(t.hover).child(m))
                        })
                        .child(div().id("shortcuts-list").flex_1().overflow_y_scroll().px(px(10.)).pb(px(10.)).children(groups.into_iter().map(
                            |(group, items)| {
                                div()
                                    .child(div().px_2().pt(px(14.)).pb(px(4.)).text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr(group)))
                                    .children(items.into_iter().map(|(action, label)| {
                                        let recording = panel.recording == Some(action);
                                        let keys = shortcuts::keys_for(action, cx);
                                        let custom = shortcuts::is_custom(action, cx);
                                        div()
                                            .id(SharedString::from(format!("sc-{action}")))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .px_2()
                                            .h(px(34.))
                                            .rounded(px(6.))
                                            .cursor_pointer()
                                            .when(recording, |d| d.bg(t.active))
                                            .hover(|s| s.bg(t.hover))
                                            .child(div().flex_1().text_size(px(13.5)).text_color(t.text).child(crate::i18n::tr(label)))
                                            .when(recording, |d| {
                                                d.child(div().text_size(px(12.)).text_color(t.accent).child(crate::i18n::tr("按下新的快捷键…  Esc 取消")))
                                            })
                                            .when(!recording, |d| {
                                                d.when(keys.is_empty(), |d| {
                                                    d.child(div().text_size(px(12.)).text_color(t.text_muted).child(crate::i18n::tr("未设置")))
                                                })
                                                .children(keys.iter().map(|k| key_pill(shortcuts::display(k), t)))
                                            })
                                            .when(custom && !recording, |d| {
                                                d.child(
                                                    div()
                                                        .id(SharedString::from(format!("sc-reset-{action}")))
                                                        .px(px(6.))
                                                        .text_size(px(12.))
                                                        .text_color(t.accent)
                                                        .hover(|s| s.underline())
                                                        .child(crate::i18n::tr("恢复默认"))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            let r = shortcuts::set(action, None, cx);
                                                            crate::refresh_menus(cx);
                                                            if let Some(p) = this.shortcuts_panel.as_mut() {
                                                                p.message = r.err();
                                                            }
                                                            cx.notify();
                                                        })),
                                                )
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if let Some(p) = this.shortcuts_panel.as_mut() {
                                                    p.recording = if p.recording == Some(action) { None } else { Some(action) };
                                                    p.message = None;
                                                    window.focus(&p.focus);
                                                }
                                                cx.notify();
                                            }))
                                    }))
                            },
                        ))),
                ),
        )
    }
}
