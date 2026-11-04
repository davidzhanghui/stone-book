//! 命令分发：菜单、工具栏、右键菜单、快捷键都汇总到 `run(命令名)`。

use super::*;

impl SheetEditor {
    /// 统一命令入口：工具栏、菜单、右键菜单、快捷键、自动化测试都走这里。
    pub(crate) fn run(&mut self, cmd: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (head, arg) = cmd.split_once(':').unwrap_or((cmd, ""));
        if !matches!(head, "painter" | "esc" | "border-color") {
            self.pop = None;
        }
        self.toast = None;
        match head {
            "up" => self.move_by(-1, 0, false, cx),
            "down" => self.move_by(1, 0, false, cx),
            "left" => self.move_by(0, -1, false, cx),
            "right" => self.move_by(0, 1, false, cx),
            "ext-up" => self.move_by(-1, 0, true, cx),
            "ext-down" => self.move_by(1, 0, true, cx),
            "ext-left" => self.move_by(0, -1, true, cx),
            "ext-right" => self.move_by(0, 1, true, cx),
            "edge-up" => self.edge(-1, 0, false, cx),
            "edge-down" => self.edge(1, 0, false, cx),
            "edge-left" => self.edge(0, -1, false, cx),
            "edge-right" => self.edge(0, 1, false, cx),
            "xedge-up" => self.edge(-1, 0, true, cx),
            "xedge-down" => self.edge(1, 0, true, cx),
            "xedge-left" => self.edge(0, -1, true, cx),
            "xedge-right" => self.edge(0, 1, true, cx),
            "tab" => {
                if self.accept_completion(window, cx) || self.pop_tab(false, window, cx) {
                    return;
                }
                self.move_by(0, 1, false, cx);
                window.focus(&self.focus);
            }
            "stab" => {
                if self.pop_tab(true, window, cx) {
                    return;
                }
                self.move_by(0, -1, false, cx);
                window.focus(&self.focus);
            }
            "enter" => {
                // 日期弹层（含日期提醒）：回车保存并关闭
                if matches!(self.cpop, Some(objs::CellPop::Date(_))) {
                    self.date_close(true, window, cx);
                    return;
                }
                self.start_edit(None, window, cx)
            }
            "senter" => self.move_by(-1, 0, false, cx),
            "edit" => self.start_edit(None, window, cx),
            "pgdn" => {
                let n = ((f32::from(self.bounds.size.height) - HEAD_H) / (25. * self.zoom)).max(1.) as i32;
                self.move_by(n, 0, false, cx)
            }
            "pgup" => {
                let n = ((f32::from(self.bounds.size.height) - HEAD_H) / (25. * self.zoom)).max(1.) as i32;
                self.move_by(-n, 0, false, cx)
            }
            "home" => {
                let c = self.cur.c;
                self.move_by(0, 1 - c, false, cx)
            }
            "end" => {
                let (_, uc) = self.used();
                let c = self.cur.c;
                self.move_by(0, uc - c, false, cx)
            }
            "top" => {
                let (r, c) = (self.cur.r, self.cur.c);
                self.move_by(1 - r, 1 - c, false, cx)
            }
            "bottom" => {
                let (ur, uc) = self.used();
                let (r, c) = (self.cur.r, self.cur.c);
                self.move_by(ur - r, uc - c, false, cx)
            }
            "scroll-to-cursor" => {
                self.ensure_visible();
                cx.notify();
            }
            "select-all" => self.select(Cell { r: 1, c: 1 }, Cell { r: 1, c: 1 }, SelKind::All, cx),
            "select-row" => {
                let (a, c) = (self.anchor, self.cur);
                self.select(a, c, SelKind::Rows, cx)
            }
            "select-col" => {
                let (a, c) = (self.anchor, self.cur);
                self.select(a, c, SelKind::Cols, cx)
            }
            "clear-content" => {
                let a = self.sel_area();
                let (r0, c0, r1, c1) = (a.row, a.column, a.row + a.height - 1, a.column + a.width - 1);
                let inside = |r: i32, c: i32| r >= r0 && r <= r1 && c >= c0 && c <= c1;
                // 单元格对象（图片 / 附件 / 迷你图 / 日期提醒）和链接也算内容，退格一起清掉
                let objs =
                    self.sx().cells.iter().any(|x| inside(x.r, x.c) && (!x.image.is_empty() || !x.files.is_empty() || x.spark.is_some() || x.remind.is_some()));
                let links: Vec<(i32, i32)> =
                    self.model.borrow().get_links(self.sheet).map(|l| l.keys().copied().filter(|&(r, c)| inside(r, c)).collect()).unwrap_or_default();
                self.batch(|this| {
                    let mut m = this.model.borrow_mut();
                    m.range_clear_contents(&a).ok();
                    for (r, c) in links {
                        m.delete_cell_link(this.sheet, r, c).ok();
                    }
                    drop(m);
                    if objs {
                        this.edit_sx(|x| {
                            for cx_ in x.cells.iter_mut().filter(|x| inside(x.r, x.c)) {
                                cx_.image.clear();
                                cx_.files.clear();
                                cx_.spark = None;
                                cx_.remind = None;
                            }
                        });
                    }
                });
                self.hover_file = None;
                if self.side.is_some_and(|s| matches!(s, dialogs::Side::Spark(c) if inside(c.r, c.c))) {
                    self.side = None;
                }
                self.changed(cx);
                self.sync_formula(cx);
            }
            "clear-all" => {
                let a = self.sel_area();
                self.model.borrow_mut().range_clear_all(&a).ok();
                self.changed(cx);
                self.sync_formula(cx);
            }
            "esc" => {
                if self.pop.take().is_some() || self.prompt.take().is_some() || self.cpop.take().is_some() || self.dlg.take().is_some() {
                } else if self.editing.take().is_some() {
                    self.sync_formula(cx);
                } else if self.painter.take().is_some() {
                } else if self.find.is_some() {
                    self.close_find(window, cx);
                } else if self.clip.as_ref().is_some_and(|c| c.cut) {
                    self.clip = None;
                } else if self.side.take().is_some() {
                } else {
                    cx.emit(SheetEvent::Close);
                }
                cx.notify();
            }
            "close" => {
                self.commit_edit(cx);
                self.cpop = None;
                self.release_file_view();
                cx.emit(SheetEvent::Close);
            }
            "fullscreen" => {
                self.commit_edit(cx);
                self.pop = None;
                cx.emit(SheetEvent::Fullscreen);
            }
            "undo" | "redo" => {
                self.editing = None;
                {
                    let mut m = self.model.borrow_mut();
                    if head == "undo" {
                        m.undo().ok()
                    } else {
                        m.redo().ok()
                    };
                }
                self.apply_filter();
                self.changed(cx);
                self.sync_formula(cx);
            }
            "copy" => self.copy(false, cx),
            "cut" => self.copy(true, cx),
            "paste" => self.paste("normal", cx),
            "paste-values" => self.paste("values", cx),
            "paste-format" => self.paste("format", cx),
            "bold" => self.toggle_style("font.b", cx),
            "italic" => self.toggle_style("font.i", cx),
            "underline" => self.toggle_style("font.u", cx),
            "strike" => self.toggle_style("font.strike", cx),
            "h-left" => self.set_align(Some("left"), None, cx),
            "h-center" => self.set_align(Some("center"), None, cx),
            "h-right" => self.set_align(Some("right"), None, cx),
            "v-top" => self.style_path("alignment.vertical", "top", cx),
            "v-middle" => self.style_path("alignment.vertical", "center", cx),
            "v-bottom" => self.style_path("alignment.vertical", "bottom", cx),
            "wrap" | "overflow" => {
                let a = self.sel_area();
                let on = head == "wrap";
                self.batch(|this| {
                    this.set_align_raw(None, Some(on));
                    this.edit_sx(|x| x.set_clip(a.row, a.column, a.row + a.height - 1, a.column + a.width - 1, false));
                });
                self.changed(cx);
            }
            "font" => {
                let name = arg.to_string();
                self.restyle(move |s| s.font.name = if name.is_empty() { "Inter".into() } else { name.clone() }, cx)
            }
            "size" => self.style_path("font.size", arg, cx),
            "font-inc" => self.style_path("font.size_delta", "1", cx),
            "font-dec" => self.style_path("font.size_delta", "-1", cx),
            "color" | "fill" => {
                SheetEditor::remember_color(arg);
                self.style_path(if head == "color" { "font.color" } else { "fill.color" }, arg, cx)
            }
            "opt-color" | "spark-color" | "spark-mark" => self.apply_obj_color(head, arg, cx),
            "border-color" => {
                SheetEditor::remember_color(arg);
                self.border_color = if arg.is_empty() { "#1f2329".into() } else { arg.to_string() };
                cx.notify();
            }
            "num" => self.style_path("num_fmt", arg, cx),
            "dec-inc" => self.decimals(true, cx),
            "dec-dec" => self.decimals(false, cx),
            "border" => self.set_border(arg, cx),
            "clear-format" => {
                let a = self.sel_area();
                self.model.borrow_mut().range_clear_formatting(&a).ok();
                self.changed(cx);
            }
            "painter" | "painter-sticky" => {
                if self.painter.is_some() {
                    self.painter = None;
                } else {
                    let a = self.sel_used();
                    let m = self.model.borrow();
                    let styles = (a.row..a.row + a.height)
                        .map(|r| (a.column..a.column + a.width).map(|c| m.get_model().get_style_for_cell(self.sheet, r, c).unwrap_or_default()).collect())
                        .collect();
                    drop(m);
                    self.painter = Some((styles, head == "painter-sticky"));
                    self.toast = Some(if head == "painter-sticky" {
                        "格式刷（连续使用，Esc 退出）：选择要应用格式的单元格".into()
                    } else {
                        "格式刷：选择要应用格式的单元格".into()
                    });
                }
                cx.notify();
            }
            "merge" | "merge-rows" | "merge-cols" | "unmerge" | "merge-toggle" => {
                let a = self.sel_used();
                let merged = self.merged_at(self.cur).is_some();
                let unmerging = head == "unmerge" || (head == "merge-toggle" && merged);
                {
                    let mut m = self.model.borrow_mut();
                    if !unmerging {
                        // 区域内多格有内容时只保留（每个合并块）左上方第一个非空值，其余清空后再合并；
                        // ironcalc 遇到多个非空格会直接拒绝合并
                        let parts = match head {
                            "merge-rows" => (a.row..a.row + a.height).map(|r| area(a.sheet, r, a.column, r, a.column + a.width - 1)).collect(),
                            "merge-cols" => (a.column..a.column + a.width).map(|c| area(a.sheet, a.row, c, a.row + a.height - 1, c)).collect(),
                            _ => vec![area(a.sheet, a.row, a.column, a.row + a.height - 1, a.column + a.width - 1)],
                        };
                        let mut dropped = false;
                        for p in parts {
                            let filled: Vec<(i32, i32)> = (p.row..p.row + p.height)
                                .flat_map(|r| (p.column..p.column + p.width).map(move |c| (r, c)))
                                .filter(|&(r, c)| m.get_cell_content(p.sheet, r, c).map(|s| !s.is_empty()).unwrap_or(false))
                                .collect();
                            for &(r, c) in filled.iter().skip(1) {
                                m.range_clear_contents(&area(p.sheet, r, c, r, c)).ok();
                                dropped = true;
                            }
                        }
                        if dropped {
                            self.toast = Some("合并后仅保留左上角单元格的内容".into());
                        }
                    }
                    match head {
                        "merge-rows" => m.merge_cells_across(&a).ok(),
                        "merge-cols" => m.merge_cells_down(&a).ok(),
                        "unmerge" => m.unmerge_cells(&a).ok(),
                        _ if head == "merge-toggle" && merged => m.unmerge_cells(&a).ok(),
                        _ => m.merge_cells(&a).ok(),
                    };
                }
                self.changed(cx);
            }
            "fill-down" => self.fill_dir(true, cx),
            "fill-right" => self.fill_dir(false, cx),
            "insert-date" | "insert-time" => {
                let now = chrono::Local::now();
                let v = if head == "insert-date" { now.format("%Y-%m-%d").to_string() } else { now.format("%H:%M:%S").to_string() };
                if self.editing.is_some() {
                    let t = format!("{}{v}", self.editing_text(cx).unwrap_or_default());
                    self.set_edit_text(&t, cx);
                } else {
                    let c = self.cur;
                    self.set_value(c, &v, cx);
                    self.sync_formula(cx);
                }
            }
            "insert-fn" => {
                // 插入函数：进入编辑并写好 `=NAME(`
                self.start_edit(Some(format!("={arg}(")), window, cx);
            }
            "autosum" => self.autosum(window, cx),
            "find" => self.open_find(false, window, cx),
            "replace" => self.open_find(true, window, cx),
            "shortcuts" => {
                self.pop = Some(Popup { kind: Pop::Shortcuts, at: point(px(0.), px(0.)) });
                cx.notify();
            }
            "zoom-in" | "zoom-out" | "zoom-reset" | "zoom" => {
                const STEPS: [f32; 9] = [0.5, 0.75, 0.9, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0];
                let z = self.zoom;
                self.zoom = match head {
                    "zoom-in" => STEPS.iter().copied().find(|s| *s > z + 0.01).unwrap_or(3.),
                    "zoom-out" => STEPS.iter().rev().copied().find(|s| *s < z - 0.01).unwrap_or(0.5),
                    "zoom" => arg.parse::<f32>().map(|v| v / 100.).unwrap_or(1.).clamp(0.5, 3.),
                    _ => 1.,
                };
                self.scroll = (self.scroll.0 * self.zoom / z, self.scroll.1 * self.zoom / z);
                self.ensure_visible();
                cx.notify();
            }
            "insert-before" => {
                let w = if self.kind == SelKind::Cols { "col-left" } else { "row-above" };
                self.rows_op(w, 0, cx)
            }
            "insert-after" => {
                let w = if self.kind == SelKind::Cols { "col-right" } else { "row-below" };
                self.rows_op(w, 0, cx)
            }
            "delete-sel" => {
                let w = if self.kind == SelKind::Cols { "del-cols" } else { "del-rows" };
                self.rows_op(w, 0, cx)
            }
            "row-above" | "row-below" | "col-left" | "col-right" | "del-rows" | "del-cols" | "hide-rows" | "hide-cols" | "unhide-rows" | "unhide-cols" => {
                self.rows_op(head, arg.parse().unwrap_or(0), cx)
            }
            "freeze-none" => self.freeze(Some(0), Some(0), cx),
            "freeze-rows" => self.freeze(Some(arg.parse().unwrap_or(1)), None, cx),
            "freeze-cols" => self.freeze(None, Some(arg.parse().unwrap_or(1)), cx),
            "freeze-here" => {
                // 冻结至当前格：上方的行与左侧的列（「冻结至第 N 行 X 列」）
                let (r, c) = (self.cur.r, self.cur.c);
                self.freeze(Some(r), Some(c), cx)
            }
            "freeze-row-here" => {
                let r = self.cur.r;
                self.freeze(Some(r), None, cx)
            }
            "freeze-col-here" => {
                let c = self.cur.c;
                self.freeze(None, Some(c), cx)
            }
            "sort-asc" => self.sort(true, arg.parse().ok(), cx),
            "sort-desc" => self.sort(false, arg.parse().ok(), cx),
            "filter" => self.toggle_filter(cx),
            "filter-clear" => {
                if let Some(f) = &mut self.filter {
                    if let Ok(c) = arg.parse::<i32>() {
                        f.excl.remove(&c);
                    } else {
                        f.excl.clear();
                    }
                }
                self.apply_filter();
                self.save_view(cx);
                cx.notify();
            }
            "toolbar" => {
                self.collapsed = !self.collapsed;
                cx.notify();
            }
            "show-formulas" => {
                self.show_formulas = !self.show_formulas;
                cx.notify();
            }
            "grid" => {
                let on = self.model.borrow().get_show_grid_lines(self.sheet).unwrap_or(true);
                self.model.borrow_mut().set_show_grid_lines(self.sheet, !on).ok();
                self.changed(cx);
            }
            "f4" => {
                if let Some(t) = self.editing_text(cx) {
                    let n = t.chars().count();
                    let res = self.model.borrow().cycle_reference(&t, n, n);
                    if let Ok((new, ..)) = res {
                        self.set_edit_text(&new, cx);
                    }
                }
            }
            "new-sheet" => {
                self.model.borrow_mut().new_sheet().ok();
                let n = self.model.borrow().get_worksheets_properties().len() as u32;
                self.switch_sheet(n.saturating_sub(1), cx);
                self.changed(cx);
            }
            "next-sheet" | "prev-sheet" => {
                let n = self.model.borrow().get_worksheets_properties().len() as u32;
                let s = if head == "next-sheet" { (self.sheet + 1) % n } else { (self.sheet + n - 1) % n };
                self.switch_sheet(s, cx);
            }
            "sheet" => self.switch_sheet(arg.parse().unwrap_or(0), cx),
            "dup-sheet" => {
                let s: u32 = arg.parse().unwrap_or(self.sheet);
                self.model.borrow_mut().duplicate_sheet(s).ok();
                self.changed(cx);
            }
            "del-sheet" => {
                let s: u32 = arg.parse().unwrap_or(self.sheet);
                if self.model.borrow().get_worksheets_properties().len() > 1 {
                    self.model.borrow_mut().delete_sheet(s).ok();
                    self.switch_sheet(s.saturating_sub(1), cx);
                    self.changed(cx);
                }
            }
            "move-sheet" => {
                // move-sheet:<索引>,<新位置>
                let (a, b) = arg.split_once(',').unwrap_or(("0", "0"));
                let (a, b) = (a.parse::<u32>().unwrap_or(0), b.parse::<i64>().unwrap_or(0));
                let n = self.model.borrow().get_worksheets_properties().len() as i64;
                if b >= 0 && b < n {
                    self.model.borrow_mut().move_sheet(a, b as u32).ok();
                    self.switch_sheet(b as u32, cx);
                    self.changed(cx);
                }
            }
            "tab-color" => {
                let (s, c) = arg.split_once(',').unwrap_or(("0", ""));
                let color = ironcalc::base::types::Color::from_param(c).unwrap_or_default();
                self.model.borrow_mut().set_sheet_color(s.parse().unwrap_or(0), &color).ok();
                self.changed(cx);
            }
            "rename-sheet" => {
                let s: u32 = arg.parse().unwrap_or(self.sheet);
                self.start_rename(s, window, cx);
            }
            "hl" => {
                let v = arg.to_string();
                set_view_prefs(&self.obj_id, |p| p.hl = (!v.is_empty()).then_some(v));
                cx.notify();
            }
            "add-rows" => {
                let n = self.add_rows.read(cx).text().trim().parse::<i32>().unwrap_or(20).clamp(1, 10000);
                let (ur, _) = self.used();
                self.extra_rows += n;
                self.geo_key.0 = u64::MAX;
                let c = self.cur.c;
                self.select(Cell { r: ur + 1, c }, Cell { r: ur + 1, c }, SelKind::Cells, cx);
                self.toast = Some(format!("已在末尾添加 {n} 行").into());
            }
            "import" => self.import(cx),
            "export-xlsx" => self.export(true, cx),
            "export-csv" => self.export(false, cx),
            _ => {
                if !self.run_obj(head, arg, window, cx) && !self.run_edit(head, arg, window, cx) && !self.run_cf(head, arg, window, cx) {
                    self.run_data(head, arg, window, cx);
                }
            }
        }
    }
}
