//! 脚本测试驱动（STONEBROOK_SCRIPT 的 `sketch …` 命令），只在 dev 构建里编译。

use super::geom::{self, v};
use super::view::*;
use gpui::*;

impl SketchEditor {
    pub fn test_cmd(&mut self, arg: &str, window: &mut Window, cx: &mut Context<Self>) {
        let parts: Vec<&str> = arg.split_whitespace().collect();
        let n: Vec<f64> = parts.iter().skip(1).filter_map(|s| s.parse().ok()).collect();
        let shift = parts.contains(&"shift");
        match parts.first().copied().unwrap_or("") {
            "tool" => {
                if let Some(t) = Tool::parse(parts.get(1).copied().unwrap_or("")) {
                    self.tool = t;
                    self.sel.clear();
                }
            }
            // 世界坐标拖动：按下 → 分 8 步移动 → 松开
            "drag" if n.len() >= 4 => {
                let (a, b) = (v(n[0], n[1]), v(n[2], n[3]));
                self.press(a, shift, parts.contains(&"alt"), 1, window, cx);
                for i in 1..=8 {
                    let p = a.add(b.sub(a).scale(i as f64 / 8.));
                    self.drag_to(p, shift, false, cx);
                }
                self.release(b, shift, window, cx);
            }
            "click" if n.len() >= 2 => {
                let p = v(n[0], n[1]);
                let clicks = n.get(2).copied().unwrap_or(1.) as usize;
                self.press(p, shift, false, clicks, window, cx);
                self.release(p, shift, window, cx);
            }
            // 在 (x, y) 新建文字（或写进那里的容器）
            "text" if n.len() >= 2 => {
                let text = parts.iter().skip(3).copied().collect::<Vec<_>>().join(" ");
                let p = v(n[0], n[1]);
                let prev = self.tool;
                self.tool = Tool::Text;
                self.press(p, false, false, 1, window, cx);
                if let Some(ed) = self.editing.as_ref() {
                    let a = ed.area.clone();
                    a.update(cx, |a, cx| a.insert(&text, cx));
                }
                self.finish_edit(cx);
                window.focus(&self.focus);
                self.tool = prev;
            }
            "key" => match parts.get(1).copied().unwrap_or("") {
                "escape" => self.escape(window, cx),
                "enter" => self.enter(window, cx),
                "delete" => self.delete_sel(cx),
                "undo" => self.do_undo(cx),
                "redo" => self.do_redo(cx),
                "all" => self.select_all(cx),
                "dup" => self.duplicate(cx),
                "group" => self.group(cx),
                "ungroup" => self.ungroup(cx),
                "front" => self.z_order(2, cx),
                "back" => self.z_order(-2, cx),
                "align-left" => self.align(0, cx),
                "copy" => self.copy(cx),
                "paste" => self.paste(cx),
                _ => {}
            },
            "open" => {
                if let Ok(s) = std::fs::read_to_string(parts.get(1).copied().unwrap_or("")) {
                    if let Ok(d) = super::io::parse(&s) {
                        self.replace_doc(d, cx);
                    }
                }
            }
            "export" => {
                let path = parts.get(1).copied().unwrap_or("/tmp/sketch.png");
                if let Some(png) = super::svg::to_png(&self.doc, &[], super::svg::SvgOpts::default(), 2.) {
                    std::fs::write(path, png).ok();
                }
            }
            "dump" => self.test_dump(),
            // sel KIND [N]：选中第 N 个某种元素（容器文字不算）
            "sel" => {
                let kind = parts.get(1).copied().unwrap_or("");
                let n = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                if let Some(id) = self.doc.live().filter(|e| e.kind == kind && e.container_id().is_none()).nth(n).map(|e| e.id.clone()) {
                    self.tool = Tool::Select;
                    self.sel = [id].into_iter().collect();
                }
            }
            // pop color-stroke | color-bg | tools | head-start | head-end | fonts | none
            "pop" => {
                self.popover = match parts.get(1).copied().unwrap_or("") {
                    "color-stroke" => Some(super::view::Popover::Color(super::view::PickTarget::Stroke)),
                    "color-bg" => Some(super::view::Popover::Color(super::view::PickTarget::Bg)),
                    "tools" => Some(super::view::Popover::Tools),
                    "head-start" => Some(super::view::Popover::Head(true)),
                    "head-end" => Some(super::view::Popover::Head(false)),
                    "fonts" => Some(super::view::Popover::Fonts),
                    _ => None,
                };
            }
            "set" => match (parts.get(1).copied(), parts.get(2).copied()) {
                (Some("arrowtype"), Some(k)) => {
                    self.cur.arrow_type = if k == "elbow" {
                        "elbow"
                    } else if k == "sharp" {
                        "sharp"
                    } else {
                        "round"
                    }
                }
                (Some("bg"), Some(c)) => self.set_color(super::view::PickTarget::Bg, c, cx),
                (Some("stroke"), Some(c)) => self.set_color(super::view::PickTarget::Stroke, c, cx),
                (Some("fill"), Some(f)) => {
                    let f = f.to_string();
                    let f2 = f.clone();
                    self.set_prop(move |s| s.fill = f.clone(), move |e| e.fill_style = f2.clone(), cx)
                }
                _ => {}
            },
            "lineedit" => self.toggle_line_edit(cx),
            "crop" => self.toggle_crop(cx),
            "image" => {
                if let Some(p) = parts.get(1) {
                    self.insert_image(p.to_string(), Some(v(n.first().copied().unwrap_or(0.), n.get(1).copied().unwrap_or(0.))), cx);
                }
            }
            // 在正在编辑的文字框里输入
            "type" => {
                let text = parts.iter().skip(1).copied().collect::<Vec<_>>().join(" ");
                if let Some(ed) = self.editing.as_ref() {
                    let a = ed.area.clone();
                    a.update(cx, |a, cx| a.insert(&text, cx));
                }
            }
            "finish" => {
                self.finish_edit(cx);
                window.focus(&self.focus);
            }
            "mermaid" => self.open_mermaid(window, cx),
            "mermaid-insert" => self.insert_mermaid(window, cx),
            _ => {}
        }
        cx.notify();
    }

    pub fn test_dump(&self) {
        println!("SKETCH tool={:?} sel={} multi={:?} editing={}", self.tool, self.sel.len(), self.multi, self.editing.is_some());
        for e in self.doc.live() {
            let b = geom::bounds(e);
            println!(
                "  {} {} x={:.1} y={:.1} w={:.1} h={:.1} angle={:.3} text={:?} start={:?} end={:?} container={:?} frame={:?} groups={}",
                e.kind,
                &e.id[..6.min(e.id.len())],
                e.x,
                e.y,
                e.width,
                e.height,
                e.angle,
                e.text,
                e.binding(true).map(|b| b.element_id[..6.min(b.element_id.len())].to_string()),
                e.binding(false).map(|b| b.element_id[..6.min(b.element_id.len())].to_string()),
                e.container_id(),
                e.frame_id(),
                e.group_ids.len()
            );
            let _ = b;
        }
    }
}
