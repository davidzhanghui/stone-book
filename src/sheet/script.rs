//! 脚本驱动测试的入口（`STONEBROOK_SCRIPT`，仅 dev 构建）。

use super::*;

impl SheetEditor {
    /// 测试脚本：sel A1[:B3] / set A1 值 / fill A1:A2 A10 / run <命令> / type 文本（进入编辑）
    #[cfg(feature = "dev")]
    pub fn test(&mut self, cmd: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) {
        match cmd {
            "sel" => {
                if let Some((r0, c0, r1, c1)) = funcs::parse_range(arg) {
                    let (a, b) =
                        if arg.contains(':') { (Cell { r: r0, c: c0 }, Cell { r: r1, c: c1 }) } else { (Cell { r: r0, c: c0 }, Cell { r: r0, c: c0 }) };
                    self.select(a, b, SelKind::Cells, cx);
                }
            }
            "set" => {
                let (a, v) = arg.split_once(' ').unwrap_or((arg, ""));
                if let Some((r, c)) = funcs::parse_addr(a) {
                    self.set_value(Cell { r, c }, v, cx);
                }
            }
            "fill" => {
                let (src, to) = arg.split_once(' ').unwrap_or((arg, arg));
                if let (Some((r0, c0, r1, c1)), Some((tr, tc))) = (funcs::parse_range(src), funcs::parse_addr(to)) {
                    let a = area(self.sheet, r0, c0, r1, c1);
                    self.auto_fill(&a, Cell { r: tr, c: tc });
                    self.anchor = Cell { r: r0, c: c0 };
                    self.cur = Cell { r: tr.max(r1), c: tc.max(c1) };
                    self.changed(cx);
                }
            }
            "style" => {
                let (p, v) = arg.split_once(' ').unwrap_or((arg, ""));
                self.style_path(p, v, cx);
            }
            "run" => self.run(arg, window, cx),
            "edit" => self.start_edit(Some(arg.to_string()), window, cx),
            // files 图片|附件 路径…：把文件挂到当前格
            "files" => {
                let (kind, rest) = arg.split_once(' ').unwrap_or((arg, ""));
                let paths = rest.split_whitespace().map(std::path::PathBuf::from).collect();
                let c = self.cur;
                self.add_files(c, paths, kind == "image", cx);
            }
            "menupath" => {
                self.menu_path = arg.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                cx.notify();
            }
            // 对话框里点「确认」
            "dlgok" => self.test_dlg_ok(window, cx),
            "cpop" => match arg {
                "pick" => {
                    let c = self.cur;
                    self.open_pick(c, window, cx)
                }
                "date" => {
                    let c = self.cur;
                    self.open_date(c, false, window, cx)
                }
                "note" => {
                    let c = self.cur;
                    self.open_note(c, window, cx)
                }
                "notedone" => self.close_note(window, cx),
                "link" => {
                    let c = self.cur;
                    self.open_link_edit(c, window, cx)
                }
                "comment" => {
                    let c = self.cur;
                    self.open_comment(c, window, cx)
                }
                "hover" => {
                    self.hover_cell = Some(self.cur);
                    cx.notify();
                }
                _ => {
                    self.cpop = None;
                    cx.notify();
                }
            },
            "pop" => {
                let kind = match arg {
                    "menu" => Pop::Menu,
                    "insert" => Pop::Insert,
                    "font" => Pop::Font,
                    "size" => Pop::Size,
                    "border" => Pop::Border,
                    "color" => Pop::Color,
                    "fill" => Pop::Fill,
                    "merge" => Pop::Merge,
                    "num" => Pop::Num,
                    "more" => Pop::More,
                    "freeze" => Pop::Freeze,
                    "cond" => Pop::Cond,
                    "formula" => Pop::Formula,
                    "ctx" => Pop::Ctx,
                    "sheets" => Pop::Sheets,
                    "hl" => Pop::Hl,
                    _ => Pop::FilterCol(self.cur.c),
                };
                let r = self.cell_rect(self.cur);
                self.pop = None;
                self.open_pop(kind, point(r.left(), r.bottom()), cx);
            }
            "rclick" => {
                // rclick X Y：在窗口坐标处模拟右键（测试右键菜单）
                let n: Vec<f32> = arg.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                let ev = MouseDownEvent {
                    button: MouseButton::Right,
                    position: point(px(n[0]), px(n[1])),
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                };
                self.right_down(&ev, window, cx);
            }
            "tabmenu" => {
                let r = self.bounds;
                self.pop = Some(Popup { kind: Pop::Tab(arg.parse().unwrap_or(0)), at: point(r.left() + px(100.), r.bottom() - px(220.)) });
            }
            "filterout" => {
                // filterout <列号> <值>：在筛选中取消勾选某个值
                let (c, v) = arg.split_once(' ').unwrap_or((arg, ""));
                self.filter_toggle_value(c.parse().unwrap_or(1), Some(v.to_string()), cx);
            }
            "find" => {
                self.open_find(true, window, cx);
                if let Some(f) = &self.find {
                    let q = f.q.clone();
                    q.update(cx, |i, cx| i.set_text(arg, cx));
                }
                self.refresh_find(cx);
                self.find_step(1, cx);
            }
            _ => {}
        }
        self.sync_formula(cx);
        cx.notify();
    }
}
