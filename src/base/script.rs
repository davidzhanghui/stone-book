//! 脚本驱动测试的入口（`STONEBROOK_SCRIPT`，仅 dev 构建）。

use super::editor::*;
use super::model::*;
use gpui::*;
use serde_json::Value;

impl BaseEditor {
    /// 测试脚本：add / set 行号 字段名 值 / view 种类 / filter 字段 op 值 / sort 字段 / group 字段 / detail 行号 / kanban 行号 列名 / dump
    #[cfg(feature = "dev")]
    pub fn test(&mut self, cmd: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) {
        let parts: Vec<&str> = arg.splitn(3, ' ').collect();
        let fid_by_name = |this: &Self, n: &str| this.fields().into_iter().find(|f| f.name == n).map(|f| f.id);
        match cmd {
            "add" => {
                let n: usize = arg.parse().unwrap_or(1);
                for _ in 0..n {
                    self.add_record(vec![], None, cx);
                }
            }
            "set" if parts.len() >= 2 => {
                let rows = self.rows();
                let Some(rid) = parts[0].parse::<usize>().ok().and_then(|i| rows.get(i).cloned()) else { return };
                let Some(fid) = fid_by_name(self, parts[1]) else { return };
                let Some(mut f) = self.field(&fid) else { return };
                let raw = parts.get(2).copied().unwrap_or("");
                let v = if f.kind == FieldType::Attachment {
                    let assets = crate::objects::store().map(|s| s.assets_dir()).unwrap_or_default();
                    let files: Vec<Attachment> = raw.split('|').filter_map(|p| import_file(&assets, std::path::Path::new(p))).collect();
                    serde_json::to_value(files).unwrap_or(Value::Null)
                } else if f.kind.is_link() {
                    let tid = f.prop.table.clone();
                    let b = self.b();
                    let ids: Vec<String> =
                        raw.split(',').filter_map(|name| b.table(&tid)?.order.iter().find(|r| b.primary_text(&tid, r) == name.trim()).cloned()).collect();
                    Value::from(ids)
                } else {
                    super::io::text_to_value(&mut f, raw).unwrap_or(Value::Null)
                };
                let tid = self.tid.clone();
                self.bm().put_field(&tid, f);
                self.set_cell(&rid, &fid, v, false, cx);
            }
            "field" => {
                // field 名称 类型序号 [公式 / 关联表名 / 查找 关联字段名.目标字段名.汇总]
                let kinds: Vec<FieldType> = TYPE_MENU.iter().flat_map(|g| g.1.iter().copied()).collect();
                let Some(kind) = parts.get(1).and_then(|k| k.parse::<usize>().ok()).and_then(|i| kinds.get(i).copied()) else { return };
                let mut f = Field::new(parts[0], kind);
                let extra = parts.get(2).copied().unwrap_or("");
                match kind {
                    FieldType::Formula => f.prop.expr = extra.into(),
                    FieldType::Link | FieldType::DuplexLink => {
                        let tid = self.b().table_list().iter().find(|t| t.meta.name == extra).map(|t| t.meta.id.clone()).unwrap_or_default();
                        f.prop.table = tid;
                    }
                    FieldType::Lookup => {
                        let ps: Vec<&str> = extra.split('.').collect();
                        if let Some(link) = ps.first().and_then(|n| fid_by_name(self, n)) {
                            let target_t = self.field(&link).map(|l| l.prop.table).unwrap_or_default();
                            f.prop.link = link;
                            f.prop.target = self
                                .b()
                                .table(&target_t)
                                .and_then(|t| t.field_by_name(ps.get(1).copied().unwrap_or("")).map(|x| x.id.clone()))
                                .unwrap_or_default();
                            f.prop.agg = ps.get(2).copied().unwrap_or("").into();
                        }
                    }
                    FieldType::Currency => f.prop.fmt = 2,
                    _ => {}
                }
                self.draft = Some(Draft { fid: None, kind, prop: f.prop.clone(), insert_at: None, err: None });
                let i = self.input("fd:name", "字段名称", parts[0], false, window, cx);
                i.update(cx, |i, cx| i.set_text(parts[0], cx));
                self.save_draft(window, cx);
            }
            "opts" => {
                // opts 字段名 选项1,选项2,…（设置单选 / 多选选项）
                if let Some(mut f) = fid_by_name(self, parts[0]).and_then(|id| self.field(&id)) {
                    for n in parts.get(1).copied().unwrap_or("").split(',') {
                        f.ensure_opt(n.trim());
                    }
                    let tid = self.tid.clone();
                    self.bm().put_field(&tid, f);
                    self.commit(cx);
                }
            }
            "view" => {
                let k = match arg {
                    "kanban" => ViewKind::Kanban,
                    "gallery" => ViewKind::Gallery,
                    "calendar" => ViewKind::Calendar,
                    "gantt" => ViewKind::Gantt,
                    "form" => ViewKind::Form,
                    _ => ViewKind::Grid,
                };
                let existing = self.b().table(&self.tid).and_then(|t| t.meta.views.iter().find(|v| t.views.get(*v).is_some_and(|x| x.kind == k)).cloned());
                match existing {
                    Some(v) => self.switch_view(&v, cx),
                    None => self.add_view(k, cx),
                }
            }
            "table" => {
                let tid = self.b().table_list().iter().find(|t| t.meta.name == arg).map(|t| t.meta.id.clone());
                match tid {
                    Some(t) => self.switch_table(&t, cx),
                    None => {
                        let tid = self.bm().add_table(arg, ViewKind::Grid, 3);
                        self.commit(cx);
                        self.switch_table(&tid, cx);
                    }
                }
            }
            "filter" if parts.len() >= 2 => {
                let Some(fid) = fid_by_name(self, parts[0]) else { return };
                let op = match parts[1] {
                    "is" => Op::Is,
                    "isnot" => Op::IsNot,
                    "contains" => Op::Contains,
                    "empty" => Op::Empty,
                    "notempty" => Op::NotEmpty,
                    "gt" => Op::Gt,
                    "lt" => Op::Lt,
                    _ => Op::Contains,
                };
                let f = self.field(&fid);
                let raw = parts.get(2).copied().unwrap_or("");
                let value = match f.as_ref().map(|f| f.kind) {
                    Some(k) if k.has_options() => {
                        Value::from(raw.split(',').filter_map(|n| f.as_ref().and_then(|f| f.opt_by_name(n.trim())).map(|o| o.id.clone())).collect::<Vec<_>>())
                    }
                    _ => Value::from(raw),
                };
                self.update_view(cx, |v| v.filter.conds.push(Cond { field: fid, op, value }));
            }
            "clearfilter" => self.update_view(cx, |v| v.filter.conds.clear()),
            "sort" => {
                let (n, desc) = arg.split_once(' ').map(|(a, b)| (a, b == "desc")).unwrap_or((arg, false));
                if let Some(fid) = fid_by_name(self, n) {
                    self.update_view(cx, |v| v.sorts = vec![SortRule { field: fid, desc }]);
                }
            }
            "group" => {
                if let Some(fid) = fid_by_name(self, arg) {
                    self.update_view(cx, |v| v.groups = vec![SortRule { field: fid, desc: false }]);
                } else {
                    self.update_view(cx, |v| v.groups.clear());
                }
            }
            "stat" if parts.len() >= 2 => {
                if let Some(fid) = fid_by_name(self, parts[0]) {
                    let k = parts[1].to_string();
                    self.update_view(cx, |v| {
                        v.stats.insert(fid, k);
                    });
                }
            }
            "hide" => {
                if let Some(fid) = fid_by_name(self, arg) {
                    self.update_view(cx, |v| v.hidden.push(fid));
                }
            }
            "rowh" => {
                let h = arg.parse().unwrap_or(1);
                self.update_view(cx, |v| v.row_h = h);
            }
            "compact" => self.update_view(cx, |v| v.compact = arg != "off"),
            "detail" => {
                let rows = self.rows();
                self.detail = arg.parse::<usize>().ok().and_then(|i| rows.get(i).cloned());
            }
            "close" => {
                self.detail = None;
                self.close_popups();
            }
            "popup" => {
                let at = point(px(400.), px(160.));
                let fid0 = self.visible().get(1).map(|f| f.id.clone()).unwrap_or_default();
                let rid0 = self.rows().first().cloned().unwrap_or_default();
                let p = match parts[0] {
                    "filter" => Popup::Filter,
                    "sort" => Popup::Sort,
                    "group" => Popup::Group,
                    "fields" => Popup::Fields,
                    "newview" => Popup::NewView,
                    "more" => Popup::More,
                    "col" => Popup::ColMenu(fid0),
                    "options" => Popup::Options { rid: rid0, fid: parts.get(1).and_then(|n| fid_by_name(self, n)).unwrap_or(fid0) },
                    "date" => {
                        use chrono::Datelike;
                        let n = chrono::Local::now();
                        Popup::Date { rid: rid0, fid: parts.get(1).and_then(|n| fid_by_name(self, n)).unwrap_or(fid0), ym: (n.year(), n.month()) }
                    }
                    "links" => Popup::Links { rid: rid0, fid: parts.get(1).and_then(|n| fid_by_name(self, n)).unwrap_or(fid0) },
                    "kset" => Popup::KanbanSet,
                    "colors" => Popup::Colors,
                    "batch" => Popup::BatchEdit,
                    "groups" => Popup::FieldGroups,
                    "auto" => Popup::Automations,
                    "formset" => Popup::FormSet,
                    "export" => Popup::Export,
                    "dateset" => Popup::DateSet,
                    "rowmenu" => Popup::RowMenu(rid0.clone()),
                    "delfield" => Popup::DelField(parts.get(1).and_then(|n| fid_by_name(self, n)).unwrap_or(fid0)),
                    "widget" => {
                        let w = self.dash.as_ref().and_then(|d| {
                            self.b().meta.dashboards.iter().find(|x| &x.id == d).and_then(|d| d.widgets.get(1).or(d.widgets.first()).map(|w| w.id.clone()))
                        });
                        match w {
                            Some(w) => Popup::Widget(w),
                            None => return,
                        }
                    }
                    "editfield" => {
                        let fid = parts.get(1).and_then(|n| fid_by_name(self, n)).unwrap_or(fid0);
                        self.popup = None;
                        self.edit_field(Some(fid), None, at, window, cx);
                        return;
                    }
                    "newfield" => {
                        self.popup = None;
                        self.edit_field(None, None, at, window, cx);
                        return;
                    }
                    _ => return,
                };
                self.open_popup(p, at, cx);
            }
            "kanban" if parts.len() >= 2 => {
                // 把第 N 张卡片移到某列（按列名）
                let rows = self.rows();
                let Some(rid) = parts[0].parse::<usize>().ok().and_then(|i| rows.get(i).cloned()) else { return };
                let v = self.view();
                let Some(gf) = v.group_field.clone() else { return };
                let key = self.field(&gf).and_then(|f| f.opt_by_name(parts[1]).map(|o| o.id.clone())).unwrap_or_default();
                self.move_card(&rid, None, &key, None, cx);
            }
            "sel" if parts.len() >= 2 => {
                let rows = self.rows();
                let vis = self.visible();
                if let (Some(r), Some(f)) = (
                    parts[0].parse::<usize>().ok().and_then(|i| rows.get(i).cloned()),
                    parts[1].parse::<usize>().ok().and_then(|i| vis.get(i).map(|f| f.id.clone())),
                ) {
                    self.sel = Some(Sel { rid: r.clone(), fid: f.clone(), arid: r, afid: f });
                }
            }
            "edit" => self.start_cell_edit(None, window, cx),
            "type" => {
                if let Some((i, _)) = self.inputs.get("cell") {
                    i.update(cx, |i, cx| i.set_text(arg, cx));
                }
            }
            "search" => {
                self.search = arg.to_string();
                self.search_open = !arg.is_empty();
                if self.search_open {
                    let i = self.input("search", "搜索记录", arg, false, window, cx);
                    i.update(cx, |i, cx| i.set_text(arg, cx));
                }
            }
            "undo" => self.undo(cx),
            "redo" => self.redo(cx),
            "import" => self.import_path(std::path::Path::new(arg), false, cx),
            "append" => self.import_path(std::path::Path::new(arg), true, cx),
            "lock" => {
                let tid = self.tid.clone();
                let mut v = self.view();
                v.locked = arg != "off";
                self.bm().put_view(&tid, v);
                self.commit(cx);
            }
            // color 字段 运算符 值 [kind]：加一条填色规则
            "color" if parts.len() >= 2 => {
                let Some(fid) = fid_by_name(self, parts[0]) else { return };
                let rest: Vec<&str> = parts.get(2).copied().unwrap_or("").split(' ').collect();
                let op = match parts[1] {
                    "gt" => Op::Gt,
                    "lt" => Op::Lt,
                    "is" => Op::Is,
                    "notempty" => Op::NotEmpty,
                    "col" => Op::Empty,
                    _ => Op::Contains,
                };
                let kind: u8 = rest.get(1).and_then(|k| k.parse().ok()).unwrap_or(0);
                let color = (self.view().colors.len() % OPT_COLORS) as u8 + 1;
                let value = Value::from(rest.first().copied().unwrap_or(""));
                self.update_view(cx, |v| {
                    v.colors.push(ColorRule {
                        id: new_id("clr"),
                        kind: if parts[1] == "col" { 2 } else { kind },
                        field: fid.clone(),
                        cond: (parts[1] != "col").then(|| Cond { field: fid, op, value }),
                        color,
                    })
                });
            }
            "hier" => self.toggle_hierarchy(cx),
            // child 子行号 父行号（按当前显示顺序）
            "child" if parts.len() >= 2 => {
                let rows = self.tree_rows();
                let get = |i: &str| i.parse::<usize>().ok().and_then(|i| rows.get(i).map(|x| x.0.clone()));
                if let (Some(c), Some(p)) = (get(parts[0]), get(parts[1])) {
                    let tid = self.tid.clone();
                    let r = self.bm().set_parent(&tid, &c, Some(&p));
                    if let Err(e) = r {
                        println!("set_parent: {e}");
                    }
                    self.commit(cx);
                }
            }
            "check" => {
                let rows = self.rows();
                self.checked = arg.split_whitespace().filter_map(|i| i.parse::<usize>().ok().and_then(|i| rows.get(i).cloned())).collect();
            }
            "dash" => self.add_dashboard(cx),
            "nodash" => self.dash = None,
            "calmode" => {
                let k: u8 = arg.parse().unwrap_or(0);
                let tid = self.tid.clone();
                let mut v = self.view();
                v.cal_mode = k;
                self.bm().put_view(&tid, v);
                self.bm().commit_silent();
            }
            "scale" => {
                let k: u8 = arg.parse().unwrap_or(0);
                self.gantt_scroll = 0.;
                self.update_view(cx, |v| v.scale = k);
            }
            "workdays" => self.update_view(cx, |v| v.workdays_only = arg != "off"),
            "progress" => {
                if let Some(fid) = fid_by_name(self, arg) {
                    self.update_view(cx, |v| v.progress_field = Some(fid));
                }
            }
            "enddate" => {
                if let Some(fid) = fid_by_name(self, arg) {
                    self.update_view(cx, |v| v.end_field = Some(fid));
                }
            }
            "detailtab" => {
                self.detail_tab = arg.parse().unwrap_or(0);
                self.refresh_comment_counts();
            }
            "comment" => {
                if let (Some(rid), Some(st)) = (self.detail.clone(), crate::objects::store()) {
                    st.add_comment(&format!("base:{}", self.obj_id), 0, &rid, arg, "我");
                    self.refresh_comment_counts();
                }
            }
            "layout" => {
                let tid = self.tid.clone();
                let meta = self.b().table(&tid).map(|t| t.meta.clone());
                if let Some(mut m) = meta {
                    m.detail_layout = arg.parse().unwrap_or(0);
                    self.bm().put_table(m);
                    self.commit(cx);
                }
            }
            // fgroup 名称 字段1,字段2
            "fgroup" if parts.len() >= 2 => {
                let ids: Vec<String> = parts[1].split(',').filter_map(|n| fid_by_name(self, n.trim())).collect();
                let tid = self.tid.clone();
                let meta = self.b().table(&tid).map(|t| t.meta.clone());
                if let Some(mut m) = meta {
                    m.groups.push(FieldGroup { id: new_id("grp"), name: parts[0].into(), fields: ids });
                    self.bm().put_table(m);
                    self.commit(cx);
                }
            }
            // autoadd：示例自动化（勾选「完成」时评分设为 5 并发通知）
            "autoadd" => {
                let tid = self.tid.clone();
                let (Some(done), Some(rate)) = (fid_by_name(self, "完成"), fid_by_name(self, "评分")) else { return };
                let mut m = self.b().meta.clone();
                let id = new_id("auto");
                m.automations.push(Automation {
                    id: id.clone(),
                    name: "完成即满分".into(),
                    on: true,
                    trigger: "match".into(),
                    table: tid,
                    cond: Filter { any: false, conds: vec![Cond { field: done, op: Op::Checked, value: Value::Null }] },
                    actions: vec![
                        AutoAction { kind: "update".into(), sets: vec![FieldSet { field: rate, value: Value::from(5) }], ..Default::default() },
                        AutoAction { kind: "notify".into(), text: "{{[文本]}} 已完成".into(), ..Default::default() },
                    ],
                    ..Default::default()
                });
                self.bm().set_meta(m);
                self.commit(cx);
                self.auto_sel = Some(id);
            }
            "press" if parts.len() >= 2 => {
                let rows = self.rows();
                if let (Some(r), Some(f)) = (parts[0].parse::<usize>().ok().and_then(|i| rows.get(i).cloned()), fid_by_name(self, parts[1])) {
                    self.press_button(&r, &f, cx);
                }
            }
            "stress" => {
                let n: usize = arg.parse().unwrap_or(2000);
                let tid = super::stress::build(&mut self.bm(), n);
                self.commit(cx);
                self.switch_table(&tid, cx);
            }
            "find" => self.find_step(1, cx),
            // lookupf 名称 目标表名 目标字段名 条件字段名：打开「按条件查找」的字段编辑器
            "lookupf" => {
                let p: Vec<&str> = arg.split(' ').collect();
                let (Some(name), Some(tname)) = (p.first(), p.get(1)) else { return };
                let Some(tt) = self.b().table_list().iter().find(|t| t.meta.name == *tname).map(|t| t.meta.id.clone()) else { return };
                let target = p.get(2).and_then(|n| self.b().table(&tt).and_then(|t| t.field_by_name(n)).map(|f| f.id.clone())).unwrap_or_default();
                let cf = p.get(3).and_then(|n| self.b().table(&tt).and_then(|t| t.field_by_name(n)).map(|f| f.id.clone())).unwrap_or_default();
                let rhs = self.b().table(&self.tid).and_then(|t| t.primary().map(|f| f.id.clone())).unwrap_or_default();
                let mut prop = default_prop(FieldType::Lookup);
                prop.lookup_table = tt;
                prop.target = target;
                prop.agg = "sum".into();
                prop.conds = vec![LookupCond { field: cf, op: Op::Is, rhs, value: Value::Null }];
                self.close_popups();
                self.draft = Some(Draft { fid: None, kind: FieldType::Lookup, prop, insert_at: None, err: None });
                let i = self.input("fd:name", "字段名称", name, true, window, cx);
                i.update(cx, |i, cx| i.set_text(name, cx));
                self.popup = Some((Popup::FieldEdit, point(px(500.), px(120.))));
            }
            "savefield" => self.save_draft(window, cx),
            // fexpr 文本：在打开的公式编辑器里输入（看自动补全）
            "fexpr" => {
                if let Some((i, _)) = self.inputs.get("fd:expr").map(|x| (x.0.clone(), ())) {
                    i.update(cx, |i, cx| i.set_text(arg, cx));
                }
                if let Some(d) = self.draft.as_mut() {
                    d.prop.expr = arg.to_string();
                }
            }
            "dump" => {
                let b = self.b();
                let rows = b.view_records(&self.tid, &self.view(), &self.search);
                let t = b.table(&self.tid).unwrap();
                let fields = t.visible_fields(&self.view());
                println!("== {} / {} ({} 条)", t.meta.name, self.view().name, rows.len());
                println!("{}", fields.iter().map(|f| t.fields[f].name.clone()).collect::<Vec<_>>().join(" | "));
                for r in rows.iter().take(30) {
                    println!("{}", fields.iter().map(|f| b.text(&self.tid, r, f)).collect::<Vec<_>>().join(" | "));
                }
            }
            _ => {}
        }
        cx.notify();
    }
}
