//! 计算层：公式 / 查找引用 / 自动编号等计算字段，显示文本，以及视图管线（筛选 → 排序 → 分组）与底部统计。
//!
//! 计算结果按 记录 → 字段 两级缓存（查询不分配字符串）。修改时按依赖精确失效：
//! 改一条记录只清这条记录和「依赖本表的其他表」；改字段定义清整张表和下游表。
//! 公式之间循环引用时返回错误而不是死循环。

use super::formula::{self, fmt_date, fmt_num, FVal};
use super::model::*;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// FxHash：计算缓存的键是短字符串 id，比默认的 SipHash 快很多（T2）。
#[derive(Default, Clone, Copy)]
pub struct FxHasher(u64);

impl std::hash::Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b[..chunk.len()].copy_from_slice(chunk);
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(b)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    fn write_u8(&mut self, i: u8) {
        self.0 = (self.0.rotate_left(5) ^ i as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn write_usize(&mut self, i: usize) {
        self.0 = (self.0.rotate_left(5) ^ i as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

pub type FxMap<K, V> = HashMap<K, V, std::hash::BuildHasherDefault<FxHasher>>;

#[derive(Default)]
pub struct DepGraph {
    same: HashMap<(String, String), Vec<String>>,
    cross: HashMap<(String, String), Vec<(String, String)>>,
    wild: HashMap<String, Vec<(String, String)>>,
}

#[derive(Default)]
pub struct Cache {
    /// 记录 → 字段 → 值
    vals: FxMap<String, FxMap<String, FVal>>,
    /// 数据表 → 有缓存的记录（整表失效用）
    by_table: HashMap<String, HashSet<String>>,
    visiting: HashSet<(String, String)>,
    exprs: HashMap<String, Result<formula::Expr, String>>,
    /// 字段级依赖图；字段定义变化时重建
    graph: Option<std::rc::Rc<DepGraph>>,
    /// 按条件查找的等值索引：(表, 字段) → 值文本 → 记录
    idx: HashMap<(String, String), HashMap<String, Vec<String>>>,
    /// 数据表的数据版本（视图结果缓存据此失效）
    pub tver: HashMap<String, u64>,
    /// 本次提交里被整列失效的 (表, 字段)，提交时移进 `Base::log`
    pub dropped: HashSet<(String, String)>,
    /// 命中 / 未命中计数（性能面板）
    pub hits: u64,
    pub misses: u64,
}

impl Cache {
    /// 全量清空（测试里用来和精确失效对比）
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.vals.clear();
        self.by_table.clear();
        self.exprs.clear();
        self.graph = None;
        self.idx.clear();
        for v in self.tver.values_mut() {
            *v += 1;
        }
    }
    fn drop_table(&mut self, tid: &str) {
        if let Some(rs) = self.by_table.remove(tid) {
            for r in rs {
                self.vals.remove(&r);
            }
        }
        self.idx.retain(|k, _| k.0 != tid);
        *self.tver.entry(tid.to_string()).or_default() += 1;
    }
}

pub const NUM_FMTS: [&str; 8] = ["自动", "整数", "保留 1 位小数", "保留 2 位小数", "千分位", "千分位（2 位小数）", "百分比", "百分比（2 位小数）"];
pub const CURRENCIES: [&str; 6] = ["¥", "$", "€", "£", "HK$", "₩"];
pub const LOOKUP_AGGS: [(&str, &str); 9] = [
    ("", "原样引用"),
    ("unique", "去重引用"),
    ("count", "计数"),
    ("count_unique", "去重计数"),
    ("sum", "求和"),
    ("avg", "平均值"),
    ("max", "最大值"),
    ("min", "最小值"),
    ("concat", "拼接文本"),
];
pub const STATS: [(&str, &str); 11] = [
    ("", "不展示"),
    ("count", "记录总数"),
    ("filled", "已填写"),
    ("empty", "未填写"),
    ("filled_pct", "已填写百分比"),
    ("unique", "唯一值"),
    ("sum", "求和"),
    ("avg", "平均值"),
    ("max", "最大值"),
    ("min", "最小值"),
    ("checked", "已勾选"),
];

fn thousands(n: f64, dec: usize) -> String {
    let s = format!("{:.*}", dec, n.abs());
    let (int, frac) = s.split_once('.').map(|(a, b)| (a.to_string(), format!(".{b}"))).unwrap_or((s.clone(), String::new()));
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{}{out}{frac}", if n < 0. { "-" } else { "" })
}

/// 按数字格式显示。
pub fn fmt_number(n: f64, fmt: u8) -> String {
    match fmt {
        1 => format!("{:.0}", n),
        2 => format!("{:.1}", n),
        3 => format!("{:.2}", n),
        4 => thousands(n, 0),
        5 => thousands(n, 2),
        6 => format!("{}%", fmt_num((n * 100. * 1e6).round() / 1e6)),
        7 => format!("{:.2}%", n * 100.),
        _ => fmt_num(n),
    }
}

impl Base {
    // ───────────── 缓存失效（T1） ─────────────

    /// 字段级依赖图：
    /// - `same`：(表, 字段) → 本表里引用它的公式（同一条记录内传递，清这条记录就够了）；
    /// - `cross`：(表, 字段) → 其他记录 / 其他表里会受影响的计算字段（整列失效）；
    /// - `wild`：表 → 该表任意变化都会影响的计算字段（跨表 `[表名]` 公式、主字段是公式时的关联显示）。
    fn graph(&self) -> std::rc::Rc<DepGraph> {
        if let Some(g) = &self.cache.borrow().graph {
            return g.clone();
        }
        let mut g = DepGraph::default();
        let names: HashMap<&str, &str> = self.tables.values().map(|t| (t.meta.name.as_str(), t.meta.id.as_str())).collect();
        // 某表某字段是否可以精确依赖（计算字段的值随本记录其他字段变，按通配处理）
        let exact = |tid: &str, fid: &str| self.field(tid, fid).is_some_and(|f| !f.kind.computed() && !f.kind.is_link());
        for t in self.tables.values() {
            let a = t.meta.id.clone();
            for x in t.fields.values() {
                let me = (a.clone(), x.id.clone());
                let mut src: Vec<(String, String)> = vec![];
                let on = |tid: &str, fid: &str, src: &mut Vec<(String, String)>| {
                    if exact(tid, fid) {
                        src.push((tid.to_string(), fid.to_string()));
                    } else {
                        src.push((tid.to_string(), "*".into()));
                    }
                };
                match x.kind {
                    FieldType::Link | FieldType::DuplexLink => {
                        if let Some(p) = self.table(&x.prop.table).and_then(|tt| tt.primary()) {
                            on(&x.prop.table, &p.id, &mut src);
                        }
                    }
                    FieldType::Lookup => {
                        let target = if x.prop.lookup_table.is_empty() {
                            t.fields.get(&x.prop.link).map(|l| l.prop.table.clone()).unwrap_or_default()
                        } else {
                            x.prop.lookup_table.clone()
                        };
                        on(&target, &x.prop.target, &mut src);
                        for c in &x.prop.conds {
                            on(&target, &c.field, &mut src);
                            if !c.rhs.is_empty() {
                                g.same.entry((a.clone(), c.rhs.clone())).or_default().push(x.id.clone());
                            }
                        }
                        if !x.prop.link.is_empty() {
                            g.same.entry((a.clone(), x.prop.link.clone())).or_default().push(x.id.clone());
                        }
                    }
                    FieldType::Formula => {
                        if let Ok(e) = formula::parse(&x.prop.expr) {
                            let mut refs = vec![];
                            formula::refs(&e, &mut refs);
                            for r in refs {
                                match t.field_by_name(&r) {
                                    Some(f) => g.same.entry((a.clone(), f.id.clone())).or_default().push(x.id.clone()),
                                    None => {
                                        if let Some(id) = names.get(r.as_str()) {
                                            src.push((id.to_string(), "*".into()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                for (bt, bf) in src {
                    if bf == "*" {
                        g.wild.entry(bt).or_default().push(me.clone());
                    } else {
                        g.cross.entry((bt, bf)).or_default().push(me.clone());
                    }
                }
            }
        }
        let g = std::rc::Rc::new(g);
        self.cache.borrow_mut().graph = Some(g.clone());
        g
    }

    /// 同一条记录内：改了 `fids` 之后会变的字段（含公式的传递引用、最后更新时间）。
    fn same_closure(&self, g: &DepGraph, tid: &str, fids: Vec<String>) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        let mut stack = fids;
        while let Some(f) = stack.pop() {
            if out.contains(&f) {
                continue;
            }
            if let Some(next) = g.same.get(&(tid.to_string(), f.clone())) {
                stack.extend(next.iter().cloned());
            }
            out.push(f);
        }
        if let Some(t) = self.table(tid) {
            out.extend(t.fields.values().filter(|f| f.kind == FieldType::ModifiedTime).map(|f| f.id.clone()));
        }
        out
    }

    /// 整列失效，并沿依赖图传到下游的列。
    fn drop_columns(&self, g: &DepGraph, start: Vec<(String, String)>) {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut stack = start;
        let mut c = self.cache.borrow_mut();
        while let Some((t, f)) = stack.pop() {
            if !seen.insert((t.clone(), f.clone())) {
                continue;
            }
            if let Some(rs) = c.by_table.get(&t).cloned() {
                for r in rs {
                    if let Some(m) = c.vals.get_mut(&r) {
                        m.remove(&f);
                    }
                }
            }
            c.idx.retain(|k, _| k.0 != t);
            *c.tver.entry(t.clone()).or_default() += 1;
            c.dropped.insert((t.clone(), f.clone()));
            for x in g.same.get(&(t.clone(), f.clone())).into_iter().flatten() {
                stack.push((t.clone(), x.clone()));
            }
            for x in g.cross.get(&(t.clone(), f.clone())).into_iter().flatten().chain(g.wild.get(&t).into_iter().flatten()) {
                stack.push(x.clone());
            }
        }
    }

    /// 一条记录变了（`changed` 为改动的字段；None 表示新增 / 删除，按全部字段算）：
    /// 清这条记录自己的缓存，再把受影响的其他记录、其他表的计算列清掉。
    pub(crate) fn invalidate_record(&self, tid: &str, rid: &str, changed: Option<Vec<String>>) {
        let g = self.graph();
        let changed = match changed {
            Some(c) => c,
            None => self.table(tid).map(|t| t.fields.keys().cloned().collect()).unwrap_or_default(),
        };
        let all = self.same_closure(&g, tid, changed);
        {
            let mut c = self.cache.borrow_mut();
            c.vals.remove(rid);
            c.idx.retain(|k, _| k.0 != tid);
            *c.tver.entry(tid.to_string()).or_default() += 1;
        }
        let mut cols: Vec<(String, String)> = vec![];
        for f in &all {
            cols.extend(g.cross.get(&(tid.to_string(), f.clone())).into_iter().flatten().cloned());
        }
        cols.extend(g.wild.get(tid).into_iter().flatten().cloned());
        if !cols.is_empty() {
            self.drop_columns(&g, cols);
        }
    }

    /// 字段定义 / 数据表结构变了：清整张表和所有下游表，依赖图重建。
    pub(crate) fn invalidate_table(&self, tid: &str) {
        let g = self.graph();
        let mut tables: HashSet<String> = HashSet::new();
        tables.insert(tid.to_string());
        let mut stack = vec![tid.to_string()];
        while let Some(t) = stack.pop() {
            let fids: Vec<String> = self.table(&t).map(|x| x.fields.keys().cloned().collect()).unwrap_or_default();
            for f in fids.iter().map(|f| (t.clone(), f.clone())) {
                for x in g.cross.get(&f).into_iter().flatten() {
                    if tables.insert(x.0.clone()) {
                        stack.push(x.0.clone());
                    }
                }
            }
            for x in g.wild.get(&t).into_iter().flatten() {
                if tables.insert(x.0.clone()) {
                    stack.push(x.0.clone());
                }
            }
        }
        let mut c = self.cache.borrow_mut();
        for t in tables {
            c.drop_table(&t);
        }
        c.graph = None;
        if let Some(t) = self.tables.get(tid) {
            for f in t.fields.keys() {
                c.exprs.remove(f);
            }
        }
    }

    // ───────────── 取值 ─────────────

    /// 任意字段的值（计算字段在这里求值）。
    pub fn fval(&self, tid: &str, rid: &str, fid: &str) -> FVal {
        {
            let mut c = self.cache.borrow_mut();
            if let Some(v) = c.vals.get(rid).and_then(|m| m.get(fid)).cloned() {
                c.hits += 1;
                return v;
            }
            c.misses += 1;
        }
        let key = (rid.to_string(), fid.to_string());
        let (Some(t), Some(r)) = (self.table(tid), self.record(tid, rid)) else { return FVal::Null };
        let Some(f) = t.fields.get(fid) else { return FVal::Null };
        if !self.cache.borrow_mut().visiting.insert(key.clone()) {
            return FVal::Err("循环引用".into());
        }
        let v = match f.kind {
            FieldType::Formula => self.eval_formula(t, rid, f),
            FieldType::Lookup => self.eval_lookup(tid, r, f),
            FieldType::AutoNumber => FVal::Str(auto_number(f, r.seq)),
            FieldType::CreatedTime => FVal::Date(r.created),
            FieldType::ModifiedTime => FVal::Date(r.updated),
            _ => self.stored_fval(f, r.cells.get(fid)),
        };
        let mut c = self.cache.borrow_mut();
        c.visiting.remove(&key);
        c.by_table.entry(tid.to_string()).or_default().insert(key.0.clone());
        c.vals.entry(key.0).or_default().insert(key.1, v.clone());
        v
    }

    fn stored_fval(&self, f: &Field, v: Option<&Value>) -> FVal {
        let Some(v) = v else {
            return if f.kind == FieldType::Checkbox { FVal::Bool(false) } else { FVal::Null };
        };
        use FieldType::*;
        match f.kind {
            Number | Progress | Currency | Rating => v.as_f64().map(FVal::Num).unwrap_or(FVal::Null),
            Date => v.as_i64().map(FVal::Date).unwrap_or(FVal::Null),
            Checkbox => FVal::Bool(v.as_bool().unwrap_or(false)),
            Select | Stage => v.as_str().and_then(|id| f.opt(id)).map(|o| FVal::Str(o.name.clone())).unwrap_or(FVal::Null),
            MultiSelect => FVal::List(str_list(Some(v)).iter().filter_map(|id| f.opt(id)).map(|o| FVal::Str(o.name.clone())).collect()),
            Person => FVal::List(str_list(Some(v)).into_iter().map(FVal::Str).collect()),
            Attachment => FVal::List(attachments(Some(v)).into_iter().map(|a| FVal::Str(a.name)).collect()),
            Link | DuplexLink => FVal::List(
                str_list(Some(v)).iter().filter(|r| self.record(&f.prop.table, r).is_some()).map(|r| FVal::Str(self.primary_text(&f.prop.table, r))).collect(),
            ),
            _ => match v {
                Value::String(s) => FVal::Str(s.clone()),
                Value::Number(n) => FVal::Num(n.as_f64().unwrap_or(0.)),
                Value::Bool(b) => FVal::Bool(*b),
                _ => FVal::Str(v.to_string()),
            },
        }
    }

    fn eval_formula(&self, t: &Table, rid: &str, f: &Field) -> FVal {
        let parsed = {
            let mut c = self.cache.borrow_mut();
            c.exprs.entry(f.id.clone()).or_insert_with(|| formula::parse(&f.prop.expr)).clone()
        };
        let e = match parsed {
            Ok(e) => e,
            Err(msg) => return if f.prop.expr.trim().is_empty() { FVal::Null } else { FVal::Err(msg) },
        };
        let mut env = BaseEnv { b: self, tid: t.meta.id.clone(), rid: rid.to_string() };
        self.rec_titles(formula::eval_env(&e, &mut env))
    }

    /// 公式结果里的记录换成标题文本（跨表 FILTER 直接返回记录集时）。
    fn rec_titles(&self, v: FVal) -> FVal {
        match v {
            FVal::Rec(t, r) => FVal::Str(self.title(&t, &r)),
            FVal::List(l) => FVal::List(l.into_iter().map(|x| self.rec_titles(x)).collect()),
            v => v,
        }
    }

    /// 按条件查找：目标表里满足全部条件的记录（第一条等值条件走哈希索引）。
    pub fn lookup_matches(&self, tid: &str, rid: &str, target: &str, conds: &[LookupCond]) -> Vec<String> {
        let Some(tt) = self.table(target) else { return vec![] };
        let rhs = |c: &LookupCond| -> FVal {
            if c.rhs.is_empty() {
                match &c.value {
                    Value::String(s) => FVal::Str(s.clone()),
                    Value::Number(n) => FVal::Num(n.as_f64().unwrap_or(0.)),
                    Value::Bool(b) => FVal::Bool(*b),
                    Value::Null => FVal::Null,
                    v => FVal::Str(v.to_string()),
                }
            } else {
                self.fval(tid, rid, &c.rhs)
            }
        };
        let conds: Vec<(&LookupCond, FVal)> = conds.iter().filter(|c| tt.fields.contains_key(&c.field)).map(|c| (c, rhs(c))).collect();
        // 候选集：第一条「等于」条件用索引缩小范围
        let cands: Vec<String> = match conds.iter().find(|(c, v)| c.op == Op::Is && !v.is_null()) {
            Some((c, v)) => {
                let key = (target.to_string(), c.field.clone());
                if !self.cache.borrow().idx.contains_key(&key) {
                    let mut m: HashMap<String, Vec<String>> = HashMap::new();
                    for r in &tt.order {
                        for k in lookup_keys(&self.fval(target, r, &c.field)) {
                            m.entry(k).or_default().push(r.clone());
                        }
                    }
                    self.cache.borrow_mut().idx.insert(key.clone(), m);
                }
                let mut out: Vec<String> = vec![];
                let c = self.cache.borrow();
                for k in lookup_keys(v) {
                    for r in c.idx[&key].get(&k).into_iter().flatten() {
                        if !out.contains(r) {
                            out.push(r.clone());
                        }
                    }
                }
                // 保持目标表的顺序
                let pos: HashMap<&String, usize> = tt.order.iter().enumerate().map(|(i, r)| (r, i)).collect();
                out.sort_by_key(|r| pos.get(r).copied().unwrap_or(usize::MAX));
                out
            }
            None => tt.order.clone(),
        };
        cands.into_iter().filter(|r| conds.iter().all(|(c, v)| lookup_cond_match(&self.fval(target, r, &c.field), c.op, v))).collect()
    }

    fn eval_lookup(&self, tid: &str, r: &Record, f: &Field) -> FVal {
        let (target_t, rids) = if !f.prop.lookup_table.is_empty() {
            (f.prop.lookup_table.clone(), self.lookup_matches(tid, &r.id, &f.prop.lookup_table, &f.prop.conds))
        } else {
            let Some(link) = self.field(tid, &f.prop.link) else { return FVal::Null };
            (link.prop.table.clone(), str_list(r.cells.get(&link.id)))
        };
        if self.field(&target_t, &f.prop.target).is_none() {
            return FVal::Null;
        }
        let mut vals = vec![];
        for lr in rids {
            if self.record(&target_t, &lr).is_some() {
                match self.fval(&target_t, &lr, &f.prop.target) {
                    FVal::List(l) => vals.extend(l),
                    FVal::Err(e) => return FVal::Err(e),
                    v if !v.is_null() => vals.push(v),
                    _ => {}
                }
            }
        }
        let nums = || vals.iter().filter_map(|v| v.num()).collect::<Vec<f64>>();
        match f.prop.agg.as_str() {
            "count" => FVal::Num(vals.len() as f64),
            "sum" => FVal::Num(nums().iter().sum()),
            "avg" => {
                let n = nums();
                if n.is_empty() {
                    FVal::Null
                } else {
                    FVal::Num(n.iter().sum::<f64>() / n.len() as f64)
                }
            }
            "max" => nums().into_iter().fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b)))).map(FVal::Num).unwrap_or(FVal::Null),
            "min" => nums().into_iter().fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.min(b)))).map(FVal::Num).unwrap_or(FVal::Null),
            "concat" => FVal::Str(vals.iter().map(|v| v.text()).collect::<Vec<_>>().join("")),
            "count_unique" => FVal::Num(vals.iter().map(|v| v.text()).collect::<HashSet<_>>().len() as f64),
            "unique" => {
                let mut seen = HashSet::new();
                FVal::List(vals.into_iter().filter(|v| seen.insert(v.text())).collect())
            }
            _ => FVal::List(vals),
        }
    }

    /// 主字段文本（卡片标题、关联显示）。
    pub fn primary_text(&self, tid: &str, rid: &str) -> String {
        let Some(p) = self.table(tid).and_then(|t| t.primary()) else { return String::new() };
        let pid = p.id.clone();
        self.text(tid, rid, &pid)
    }

    pub fn title(&self, tid: &str, rid: &str) -> String {
        let s = self.primary_text(tid, rid);
        if s.trim().is_empty() {
            "未命名记录".into()
        } else {
            s
        }
    }

    /// 单元格显示文本（搜索、导出、复制都用它）。
    pub fn text(&self, tid: &str, rid: &str, fid: &str) -> String {
        let Some(f) = self.field(tid, fid) else { return String::new() };
        let v = self.fval(tid, rid, fid);
        format_fval(f, &v)
    }

    // ───────────── 筛选 ─────────────

    pub fn cond_match(&self, tid: &str, rid: &str, c: &Cond) -> bool {
        let Some(f) = self.field(tid, &c.field) else { return true };
        let v = self.fval(tid, rid, &c.field);
        let empty = v.is_null() || (f.kind == FieldType::Checkbox && !v.truthy());
        match c.op {
            Op::Empty => return empty,
            Op::NotEmpty => return !empty,
            Op::Checked => return v.truthy(),
            Op::Unchecked => return !v.truthy(),
            _ => {}
        }
        let raw = self.record(tid, rid).and_then(|r| r.cells.get(&c.field));
        match f.kind {
            FieldType::Checkbox => v.truthy() == c.value.as_bool().unwrap_or(true),
            FieldType::Select | FieldType::Stage | FieldType::MultiSelect | FieldType::Person => {
                let want: Vec<String> = str_list(Some(&c.value));
                if want.is_empty() {
                    return true;
                }
                let have: Vec<String> = str_list(raw);
                let any = have.iter().any(|h| want.contains(h));
                match c.op {
                    Op::Is if f.kind.has_options() && f.kind != FieldType::MultiSelect => any,
                    Op::Is => {
                        let (a, b): (HashSet<_>, HashSet<_>) = (have.iter().collect(), want.iter().collect());
                        a == b
                    }
                    Op::IsNot | Op::NotContains => !any,
                    _ => any,
                }
            }
            k if k.is_date() => {
                let Some(d) = (match v {
                    FVal::Date(d) => Some(d),
                    _ => None,
                }) else {
                    return false;
                };
                if c.op == Op::Between {
                    let (a, b) = pair(&c.value);
                    let lo = date_range(&a).map(|x| x.0).unwrap_or(i64::MIN);
                    let hi = date_range(&b).map(|x| x.1).unwrap_or(i64::MAX);
                    return d >= lo && d < hi;
                }
                let Some((lo, hi)) = date_range(c.value.as_str().unwrap_or("today")) else { return true };
                match c.op {
                    Op::Lt => d < lo,
                    Op::Gt => d >= hi,
                    _ => d >= lo && d < hi,
                }
            }
            k if c.op == Op::Between && (k.numeric() || matches!(v, FVal::Num(_))) => {
                let (a, b) = pair(&c.value);
                let Some(n) = (if v.is_null() { None } else { v.num() }) else { return false };
                a.trim().parse::<f64>().map_or(true, |a| n >= a) && b.trim().parse::<f64>().map_or(true, |b| n <= b)
            }
            k if k.numeric() || matches!(v, FVal::Num(_)) && !matches!(c.op, Op::Contains | Op::NotContains) => {
                let want = c.value.as_str().and_then(|s| s.trim().parse::<f64>().ok()).or_else(|| c.value.as_f64());
                let Some(want) = want else { return true };
                // 空值：只有「不等于」成立（与文本、选项的不等于一致）
                let Some(n) = (if v.is_null() { None } else { v.num() }) else { return c.op == Op::IsNot };
                match c.op {
                    Op::Is => (n - want).abs() < 1e-9,
                    Op::IsNot => (n - want).abs() >= 1e-9,
                    Op::Gt => n > want,
                    Op::Ge => n >= want,
                    Op::Lt => n < want,
                    Op::Le => n <= want,
                    _ => true,
                }
            }
            _ => {
                let want = c.value.as_str().unwrap_or("").trim().to_lowercase();
                if want.is_empty() {
                    return true;
                }
                let have = format_fval(f, &v).to_lowercase();
                match c.op {
                    Op::Is => have == want,
                    Op::IsNot => have != want,
                    Op::Contains => have.contains(&want),
                    Op::NotContains => !have.contains(&want),
                    _ => true,
                }
            }
        }
    }

    /// 视图管线用的筛选：条件只解析一次（大表上逐行重建条件列表很费时）。
    fn matcher<'a>(&self, tid: &str, filter: &'a Filter) -> Vec<&'a Cond> {
        filter.conds.iter().filter(|c| self.field(tid, &c.field).is_some()).collect()
    }

    pub fn record_matches(&self, tid: &str, rid: &str, filter: &Filter) -> bool {
        let conds: Vec<&Cond> = filter.conds.iter().filter(|c| self.field(tid, &c.field).is_some()).collect();
        if conds.is_empty() {
            return true;
        }
        if filter.any {
            conds.iter().any(|c| self.cond_match(tid, rid, c))
        } else {
            conds.iter().all(|c| self.cond_match(tid, rid, c))
        }
    }

    // ───────────── 排序 ─────────────

    fn sort_key(&self, tid: &str, rid: &str, f: &Field) -> SortKey {
        let v = self.fval(tid, rid, &f.id);
        if v.is_null() && f.kind != FieldType::Checkbox {
            return SortKey::Empty;
        }
        let raw = self.record(tid, rid).and_then(|r| r.cells.get(&f.id));
        match f.kind {
            FieldType::Select | FieldType::Stage | FieldType::MultiSelect => {
                let first = str_list(raw).into_iter().next().unwrap_or_default();
                SortKey::Num(f.prop.options.iter().position(|o| o.id == first).unwrap_or(usize::MAX) as f64)
            }
            FieldType::Checkbox => SortKey::Num(if v.truthy() { 1. } else { 0. }),
            _ => match v {
                FVal::Num(n) => SortKey::Num(n),
                FVal::Date(d) => SortKey::Num(d as f64),
                FVal::Bool(b) => SortKey::Num(b as u8 as f64),
                v => SortKey::Text(v.text()),
            },
        }
    }

    pub fn sort_records(&self, tid: &str, rids: &mut [String], rules: &[SortRule]) {
        let Some(t) = self.table(tid) else { return };
        let rules: Vec<(&Field, bool)> = rules.iter().filter_map(|r| t.fields.get(&r.field).map(|f| (f, r.desc))).collect();
        if rules.is_empty() {
            return;
        }
        // 先算好每行的排序键再排（每行只求值一次，T3）
        let mut keyed: Vec<(Vec<SortKey>, String)> =
            rids.iter().map(|r| (rules.iter().map(|(f, _)| self.sort_key(tid, r, f)).collect(), std::mem::take(&mut r.clone()))).collect();
        keyed.sort_by(|a, b| {
            for (i, (_, desc)) in rules.iter().enumerate() {
                let o = cmp_key(&a.0[i], &b.0[i], *desc);
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        });
        for (slot, (_, r)) in rids.iter_mut().zip(keyed) {
            *slot = r;
        }
    }

    /// 视图管线：筛选 → 搜索 → 排序，返回记录 id（未排序时按手动顺序）。
    pub fn view_records(&self, tid: &str, v: &View, search: &str) -> Vec<String> {
        let Some(t) = self.table(tid) else { return vec![] };
        let t0 = std::time::Instant::now();
        let fields = t.visible_fields(v);
        let conds = self.matcher(tid, &v.filter);
        let q = search.trim().to_lowercase();
        let any = v.filter.any;
        let mut out: Vec<String> = t
            .order
            .iter()
            .filter(|r| {
                conds.is_empty() || if any { conds.iter().any(|c| self.cond_match(tid, r, c)) } else { conds.iter().all(|c| self.cond_match(tid, r, c)) }
            })
            .filter(|r| q.is_empty() || fields.iter().any(|f| self.text(tid, r, f).to_lowercase().contains(&q)))
            .cloned()
            .collect();
        self.sort_records(tid, &mut out, &v.sorts);
        super::stress::record(&super::stress::PIPE_US, t0);
        out
    }

    /// 增量更新视图结果：从 `from_ver` 之后的提交日志判断能否只挪动改过的记录。
    /// 能则原地更新 `rows` 并返回 true；不能（改了字段定义、视图用到的计算列整列失效、日志不全）返回 false。
    pub fn refresh_rows(&self, tid: &str, v: &View, search: &str, rows: &mut Vec<String>, from_ver: u64) -> bool {
        let Some(t) = self.table(tid) else { return false };
        let entries: Vec<&CommitInfo> = self.log.iter().filter(|c| c.ver > from_ver).collect();
        if entries.len() as u64 != self.ver - from_ver {
            return false;
        }
        let used: HashSet<&String> = v.filter.conds.iter().map(|c| &c.field).chain(v.sorts.iter().map(|s| &s.field)).collect();
        let mut changed: HashSet<String> = HashSet::new();
        for e in entries {
            if e.structural {
                return false;
            }
            // 视图用到的计算列整列失效：不知道哪些记录变了，只能重算
            if e.cols.iter().any(|(ct, cf)| ct == tid && (used.contains(cf) || !search.trim().is_empty())) {
                return false;
            }
            changed.extend(e.recs.get(tid).into_iter().flatten().cloned());
        }
        if changed.is_empty() {
            return true;
        }
        if changed.len() > 2000 {
            return false;
        }
        rows.retain(|r| !changed.contains(r));
        let conds = self.matcher(tid, &v.filter);
        let q = search.trim().to_lowercase();
        let fields = t.visible_fields(v);
        let rules: Vec<(&Field, bool)> = v.sorts.iter().filter_map(|r| t.fields.get(&r.field).map(|f| (f, r.desc))).collect();
        let rank = |r: &str| t.records.get(r).map(|x| x.rank).unwrap_or(0.);
        // 手动顺序：按 (rank, id) 比较；有排序时按排序键，相同时按手动顺序
        let cmp = |a: &str, ka: &[SortKey], b: &str, kb: &[SortKey]| -> Ordering {
            for (i, (_, desc)) in rules.iter().enumerate() {
                let o = cmp_key(&ka[i], &kb[i], *desc);
                if o != Ordering::Equal {
                    return o;
                }
            }
            rank(a).partial_cmp(&rank(b)).unwrap_or(Ordering::Equal).then(a.cmp(b))
        };
        let mut add: Vec<String> = changed.into_iter().filter(|r| t.records.contains_key(r)).collect();
        add.sort_by(|a, b| rank(a).partial_cmp(&rank(b)).unwrap_or(Ordering::Equal).then(a.cmp(b)));
        for r in add {
            let ok = (conds.is_empty()
                || if v.filter.any { conds.iter().any(|c| self.cond_match(tid, &r, c)) } else { conds.iter().all(|c| self.cond_match(tid, &r, c)) })
                && (q.is_empty() || fields.iter().any(|f| self.text(tid, &r, f).to_lowercase().contains(&q)));
            if !ok {
                continue;
            }
            let kr: Vec<SortKey> = rules.iter().map(|(f, _)| self.sort_key(tid, &r, f)).collect();
            let pos = rows.partition_point(|x| {
                let kx: Vec<SortKey> = rules.iter().map(|(f, _)| self.sort_key(tid, x, f)).collect();
                cmp(x, &kx, &r, &kr) == Ordering::Less
            });
            rows.insert(pos, r);
        }
        true
    }

    // ───────────── 记录层级（R1） ─────────────

    /// 记录的父记录（层级未开启或父记录不存在时为 None）。
    pub fn parent_of(&self, tid: &str, rid: &str) -> Option<String> {
        let t = self.table(tid)?;
        let pf = t.meta.parent_field.as_ref()?;
        let p = str_list(t.records.get(rid)?.cells.get(pf)).into_iter().next()?;
        (p != rid && t.records.contains_key(&p)).then_some(p)
    }

    /// 记录的层级深度（顶级为 0）。
    pub fn depth_of(&self, tid: &str, rid: &str) -> usize {
        let mut d = 0;
        let mut cur = rid.to_string();
        while let Some(p) = self.parent_of(tid, &cur) {
            d += 1;
            cur = p;
            if d > 64 {
                break;
            }
        }
        d
    }

    /// 子记录（按表内顺序）。
    pub fn children_of(&self, tid: &str, rid: &str) -> Vec<String> {
        let Some(t) = self.table(tid) else { return vec![] };
        t.order.iter().filter(|r| self.parent_of(tid, r).as_deref() == Some(rid)).cloned().collect()
    }

    /// 子树高度（只有自己为 0）。
    pub fn subtree_height(&self, tid: &str, rid: &str) -> usize {
        self.children_of(tid, rid).iter().map(|c| 1 + self.subtree_height(tid, c)).max().unwrap_or(0)
    }

    /// 视图结果树化：同级保持传入顺序（已筛选 + 排序），被命中的子记录补出祖先。
    /// 返回 (记录, 深度, 是否命中筛选)；`collapsed` 里有 "t:记录" 的节点不展开。
    pub fn tree(&self, tid: &str, rows: &[String], collapsed: &[String]) -> Vec<(String, usize, bool)> {
        let Some(t) = self.table(tid) else { return vec![] };
        if t.meta.parent_field.is_none() {
            return rows.iter().map(|r| (r.clone(), 0, true)).collect();
        }
        let matched: HashSet<&String> = rows.iter().collect();
        // 位置：命中的按视图顺序，补出的祖先排在它第一个后代之前
        let mut pos: HashMap<String, usize> = HashMap::new();
        let mut parent: HashMap<String, Option<String>> = HashMap::new();
        for (i, r) in rows.iter().enumerate() {
            pos.entry(r.clone()).or_insert(i * 2 + 1);
            let mut cur = r.clone();
            let mut guard = 0;
            loop {
                let p = self.parent_of(tid, &cur);
                parent.insert(cur.clone(), p.clone());
                let Some(p) = p else { break };
                pos.entry(p.clone()).or_insert(i * 2);
                cur = p;
                guard += 1;
                if guard > MAX_DEPTH * 4 {
                    break;
                }
            }
        }
        let mut kids: HashMap<Option<String>, Vec<String>> = HashMap::new();
        let mut all: Vec<&String> = pos.keys().collect();
        all.sort_by_key(|r| pos[*r]);
        for r in all {
            kids.entry(parent.get(r).cloned().flatten()).or_default().push(r.clone());
        }
        let mut out = vec![];
        fn walk(
            n: &Option<String>,
            d: usize,
            kids: &HashMap<Option<String>, Vec<String>>,
            matched: &HashSet<&String>,
            collapsed: &[String],
            out: &mut Vec<(String, usize, bool)>,
        ) {
            for c in kids.get(n).into_iter().flatten() {
                out.push((c.clone(), d, matched.contains(c)));
                if !collapsed.contains(&format!("t:{c}")) {
                    walk(&Some(c.clone()), d + 1, kids, matched, collapsed, out);
                }
            }
        }
        walk(&None, 0, &kids, &matched, collapsed, &mut out);
        out
    }

    // ───────────── 填色（R2） ─────────────

    /// 按填色规则算出可见记录的颜色：整列、整行、单元格（同类规则取第一条命中的）。
    pub fn colorize(&self, tid: &str, v: &View, rids: &[String]) -> Colors {
        let mut out = Colors::default();
        for rule in &v.colors {
            if self.field(tid, &rule.field).is_none() {
                continue;
            }
            match rule.kind {
                2 => {
                    out.col.entry(rule.field.clone()).or_insert(rule.color);
                }
                k => {
                    for r in rids {
                        let hit = rule.cond.as_ref().is_none_or(|c| self.field(tid, &c.field).is_some() && self.cond_match(tid, r, c));
                        if !hit {
                            continue;
                        }
                        if k == 1 {
                            out.row.entry(r.clone()).or_insert(rule.color);
                        } else {
                            out.cell.entry((r.clone(), rule.field.clone())).or_insert(rule.color);
                        }
                    }
                }
            }
        }
        out
    }

    // ───────────── 依赖检查（R6 删除前提示） ─────────────

    /// 用到某字段的地方（公式、查找引用、视图配置、层级、仪表盘、自动化、按钮）。
    pub fn field_usages(&self, tid: &str, fid: &str) -> Vec<String> {
        let mut out = vec![];
        let Some(f) = self.field(tid, fid) else { return out };
        let name = f.name.clone();
        for t in self.table_list() {
            for x in t.field_list() {
                if x.id == fid {
                    continue;
                }
                let used = match x.kind {
                    FieldType::Formula if t.meta.id == tid => formula::parse(&x.prop.expr).ok().is_some_and(|e| {
                        let mut r = vec![];
                        formula::refs(&e, &mut r);
                        r.contains(&name)
                    }),
                    FieldType::Lookup => x.prop.link == fid || x.prop.target == fid || x.prop.conds.iter().any(|c| c.field == fid || c.rhs == fid),
                    FieldType::Button => x.prop.sets.iter().any(|s| s.field == fid) || x.prop.url.contains(&format!("[{name}]")),
                    _ => false,
                };
                if used {
                    out.push(format!("「{}」表的{}字段「{}」", t.meta.name, x.kind.label(), x.name));
                }
            }
            if t.meta.id != tid {
                continue;
            }
            if t.meta.parent_field.as_deref() == Some(fid) {
                out.push("记录层级（父记录字段）".into());
            }
            for vid in &t.meta.views {
                let Some(v) = t.views.get(vid) else { continue };
                let mut what = vec![];
                if v.filter.conds.iter().any(|c| c.field == fid) {
                    what.push("筛选");
                }
                if v.sorts.iter().any(|c| c.field == fid) {
                    what.push("排序");
                }
                if v.groups.iter().any(|c| c.field == fid) || v.group_field.as_deref() == Some(fid) {
                    what.push("分组");
                }
                if v.date_field.as_deref() == Some(fid) || v.end_field.as_deref() == Some(fid) {
                    what.push("日期字段");
                }
                if v.colors.iter().any(|c| c.field == fid || c.cond.as_ref().is_some_and(|x| x.field == fid)) || v.color_field.as_deref() == Some(fid) {
                    what.push("填色");
                }
                if v.cover.as_deref() == Some(fid) {
                    what.push("封面");
                }
                if v.progress_field.as_deref() == Some(fid) {
                    what.push("进度");
                }
                if !what.is_empty() {
                    out.push(format!("视图「{}」的{}", v.name, what.join("、")));
                }
            }
        }
        for d in &self.meta.dashboards {
            if d.widgets.iter().any(|w| w.table == tid && (w.dim == fid || w.dim2 == fid || w.field == fid)) {
                out.push(format!("仪表盘「{}」", d.name));
            }
        }
        for a in &self.meta.automations {
            if a.table == tid
                && (a.field == fid || a.cond.conds.iter().any(|c| c.field == fid) || a.actions.iter().any(|x| x.sets.iter().any(|s| s.field == fid)))
            {
                out.push(format!("自动化「{}」", a.name));
            }
        }
        out
    }

    // ───────────── 分组 ─────────────

    /// 一条记录在某字段上的分组键：(键, 显示名, 选项颜色)。`split` 为 true 时多值字段拆成多组（看板）。
    pub fn group_keys(&self, tid: &str, rid: &str, fid: &str, split: bool) -> Vec<(String, String, Option<u8>)> {
        let Some(f) = self.field(tid, fid) else { return vec![] };
        let raw = self.record(tid, rid).and_then(|r| r.cells.get(fid));
        let none = || vec![(String::new(), "未分组".to_string(), None)];
        match f.kind {
            FieldType::Select | FieldType::Stage | FieldType::MultiSelect => {
                let opts: Vec<&SelOpt> = str_list(raw).iter().filter_map(|id| f.opt(id)).collect();
                if opts.is_empty() {
                    return none();
                }
                if split || f.kind != FieldType::MultiSelect {
                    opts.iter().map(|o| (o.id.clone(), o.name.clone(), Some(o.color))).collect()
                } else {
                    vec![(
                        opts.iter().map(|o| o.id.as_str()).collect::<Vec<_>>().join(","),
                        opts.iter().map(|o| o.name.as_str()).collect::<Vec<_>>().join("、"),
                        opts.first().map(|o| o.color),
                    )]
                }
            }
            FieldType::Person | FieldType::Link | FieldType::DuplexLink => {
                let ids = str_list(raw);
                let label = |id: &String| if f.kind == FieldType::Person { id.clone() } else { self.title(&f.prop.table, id) };
                let ids: Vec<String> = ids.into_iter().filter(|id| f.kind == FieldType::Person || self.record(&f.prop.table, id).is_some()).collect();
                if ids.is_empty() {
                    return none();
                }
                if split {
                    ids.iter().map(|id| (id.clone(), label(id), None)).collect()
                } else {
                    vec![(ids.join(","), ids.iter().map(label).collect::<Vec<_>>().join("、"), None)]
                }
            }
            FieldType::Checkbox => {
                let on = raw.and_then(|v| v.as_bool()).unwrap_or(false);
                vec![(if on { "1" } else { "0" }.into(), if on { "已勾选" } else { "未勾选" }.into(), None)]
            }
            k if k.is_date() => match self.fval(tid, rid, fid) {
                FVal::Date(d) => vec![(fmt_date(d, 2), fmt_date(d, 0), None)],
                _ => none(),
            },
            _ => {
                let s = self.text(tid, rid, fid);
                if s.is_empty() {
                    none()
                } else {
                    vec![(s.clone(), s, None)]
                }
            }
        }
    }

    /// 分组的显示顺序：选项按选项顺序，其余按键排序；「未分组」在最前。
    pub fn order_groups(&self, tid: &str, fid: &str, keys: &mut [(String, String, Option<u8>)], desc: bool) {
        let Some(f) = self.field(tid, fid) else { return };
        let pos = |k: &str| -> f64 {
            if k.is_empty() {
                return -1.;
            }
            match f.kind {
                FieldType::Select | FieldType::Stage | FieldType::MultiSelect => {
                    k.split(',').next().and_then(|id| f.prop.options.iter().position(|o| o.id == id)).map(|p| p as f64).unwrap_or(1e9)
                }
                FieldType::Rating | FieldType::Number | FieldType::Progress | FieldType::Currency => k.replace(['%', ',', '¥', '$'], "").parse().unwrap_or(1e9),
                _ => f64::NAN,
            }
        };
        keys.sort_by(|a, b| {
            let (pa, pb) = (pos(&a.0), pos(&b.0));
            if a.0.is_empty() || b.0.is_empty() {
                // 「未分组」总在最前
                return b.0.is_empty().cmp(&a.0.is_empty());
            }
            let o = if pa.is_nan() || pb.is_nan() { a.1.cmp(&b.1) } else { pa.partial_cmp(&pb).unwrap_or(Ordering::Equal) };
            if desc {
                o.reverse()
            } else {
                o
            }
        });
    }

    // ───────────── 统计 ─────────────

    pub fn stat(&self, tid: &str, rids: &[String], fid: &str, kind: &str) -> String {
        let Some(f) = self.field(tid, fid) else { return String::new() };
        let vals: Vec<FVal> = rids.iter().map(|r| self.fval(tid, r, fid)).collect();
        let filled = vals.iter().filter(|v| !v.is_null() && !(f.kind == FieldType::Checkbox && !v.truthy())).count();
        let nums: Vec<f64> = vals
            .iter()
            .filter(|v| !v.is_null())
            .filter_map(|v| match v {
                FVal::List(l) => Some(l.iter().filter_map(|x| x.num()).sum()),
                v => v.num(),
            })
            .collect();
        let fmtn = |n: f64| match f.kind {
            FieldType::Currency => format!("{}{}", f.prop.symbol, thousands(n, f.prop.fmt as usize)),
            FieldType::Progress => format!("{}%", fmt_num((n * 100.).round())),
            FieldType::Number if f.prop.fmt > 0 => fmt_number(n, f.prop.fmt),
            _ => fmt_num((n * 100.).round() / 100.),
        };
        match kind {
            "count" => format!("{} 条记录", rids.len()),
            "filled" => format!("已填写 {filled}"),
            "empty" => format!("未填写 {}", rids.len() - filled),
            "filled_pct" => format!("已填写 {}%", if rids.is_empty() { 0 } else { filled * 100 / rids.len() }),
            "checked" => format!("已勾选 {filled}"),
            "unique" => {
                let set: HashSet<String> = vals.iter().filter(|v| !v.is_null()).map(|v| v.text()).collect();
                format!("唯一值 {}", set.len())
            }
            "sum" => format!("求和 {}", fmtn(nums.iter().sum())),
            "avg" if !nums.is_empty() => format!("平均 {}", fmtn(nums.iter().sum::<f64>() / nums.len() as f64)),
            "max" if !nums.is_empty() => format!("最大 {}", fmtn(nums.iter().cloned().fold(f64::MIN, f64::max))),
            "min" if !nums.is_empty() => format!("最小 {}", fmtn(nums.iter().cloned().fold(f64::MAX, f64::min))),
            "avg" | "max" | "min" => "—".into(),
            _ => String::new(),
        }
    }
}

/// 填色结果：整行 / 单元格 / 整列。
#[derive(Clone, Debug, Default)]
pub struct Colors {
    pub row: HashMap<String, u8>,
    pub cell: HashMap<(String, String), u8>,
    pub col: HashMap<String, u8>,
}

impl Colors {
    /// 单元格最终颜色：单元格 > 整行 > 整列。
    pub fn at(&self, rid: &str, fid: &str) -> Option<u8> {
        self.cell.get(&(rid.to_string(), fid.to_string())).or_else(|| self.row.get(rid)).or_else(|| self.col.get(fid)).copied()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum SortKey {
    Empty,
    Num(f64),
    Text(String),
}

fn cmp_key(a: &SortKey, b: &SortKey, desc: bool) -> Ordering {
    // 空值总在最后
    let o = match (a, b) {
        (SortKey::Empty, SortKey::Empty) => return Ordering::Equal,
        (SortKey::Empty, _) => return Ordering::Greater,
        (_, SortKey::Empty) => return Ordering::Less,
        (SortKey::Num(x), SortKey::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (SortKey::Text(x), SortKey::Text(y)) => x.cmp(y),
        (SortKey::Num(_), SortKey::Text(_)) => Ordering::Less,
        (SortKey::Text(_), SortKey::Num(_)) => Ordering::Greater,
    };
    if desc {
        o.reverse()
    } else {
        o
    }
}

pub fn auto_number(f: &Field, seq: u64) -> String {
    let n = if f.prop.digits > 0 { format!("{:0width$}", seq, width = f.prop.digits as usize) } else { seq.to_string() };
    format!("{}{n}", f.prop.prefix)
}

/// 按字段类型把值格式化成显示文本。
pub fn format_fval(f: &Field, v: &FVal) -> String {
    // 公式 / 查找引用的结果格式（R4）
    if matches!(f.kind, FieldType::Formula | FieldType::Lookup) && !f.prop.out.is_empty() {
        let n = match v {
            FVal::Num(n) => Some(*n),
            FVal::List(l) if l.len() == 1 => l[0].num(),
            _ => None,
        };
        match (f.prop.out.as_str(), n, v) {
            ("num", Some(n), _) => return fmt_number(n, f.prop.fmt),
            ("cur", Some(n), _) => {
                return format!("{}{}", if f.prop.symbol.is_empty() { "¥" } else { &f.prop.symbol }, thousands(n, f.prop.fmt.min(4) as usize))
            }
            ("pct", Some(n), _) => return format!("{:.*}%", f.prop.fmt.min(4) as usize, n * 100.),
            ("date", _, FVal::Date(d)) => return fmt_date(*d, f.prop.fmt.min(5)),
            _ => {}
        }
    }
    match (f.kind, v) {
        (_, FVal::Null) => String::new(),
        (FieldType::Number, FVal::Num(n)) if !f.prop.unit.is_empty() => format!("{} {}", fmt_number(*n, f.prop.fmt), f.prop.unit),
        (FieldType::Number, FVal::Num(n)) => fmt_number(*n, f.prop.fmt),
        (FieldType::Formula, FVal::Num(n)) => fmt_number(*n, f.prop.fmt),
        (FieldType::Currency, FVal::Num(n)) => format!("{}{}", f.prop.symbol, thousands(*n, f.prop.fmt as usize)),
        (FieldType::Progress, FVal::Num(n)) => format!("{}%", fmt_num((n * 100. * 10f64.powi(f.prop.fmt as i32)).round() / 10f64.powi(f.prop.fmt as i32))),
        (FieldType::Checkbox, FVal::Bool(b)) => {
            if *b {
                "是".into()
            } else {
                String::new()
            }
        }
        (k, FVal::Date(d)) if k == FieldType::Date || k.is_date() => fmt_date(*d, f.prop.fmt),
        (FieldType::Formula, FVal::Date(d)) => fmt_date(*d, f.prop.fmt.min(5)),
        _ => v.text(),
    }
}

/// 日期筛选值 → [起, 止) 毫秒区间。
pub fn date_range(v: &str) -> Option<(i64, i64)> {
    use chrono::{Datelike, Local, TimeZone};
    const DAY: i64 = 86_400_000;
    let t = formula::today();
    let wd = Local.timestamp_millis_opt(t).single()?.weekday().num_days_from_monday() as i64;
    Some(match v {
        "today" => (t, t + DAY),
        "tomorrow" => (t + DAY, t + 2 * DAY),
        "yesterday" => (t - DAY, t),
        "week" => (t - wd * DAY, t + (7 - wd) * DAY),
        "lastweek" => (t - (wd + 7) * DAY, t - wd * DAY),
        "last7" => (t - 6 * DAY, t + DAY),
        "next7" => (t, t + 7 * DAY),
        "last30" => (t - 29 * DAY, t + DAY),
        "next30" => (t, t + 30 * DAY),
        "nextweek" => (t + (7 - wd) * DAY, t + (14 - wd) * DAY),
        "lastmonth" | "nextmonth" => {
            let d = Local.timestamp_millis_opt(t).single()?;
            let m0 = d.year() * 12 + d.month() as i32 - 1 + if v == "lastmonth" { -1 } else { 1 };
            let start = Local.with_ymd_and_hms(m0.div_euclid(12), m0.rem_euclid(12) as u32 + 1, 1, 0, 0, 0).single()?.timestamp_millis();
            let m1 = m0 + 1;
            (start, Local.with_ymd_and_hms(m1.div_euclid(12), m1.rem_euclid(12) as u32 + 1, 1, 0, 0, 0).single()?.timestamp_millis())
        }
        s if s.starts_with("past:") || s.starts_with("next:") => {
            let n: i64 = s[5..].parse().ok()?;
            if s.starts_with("past:") {
                (t - (n - 1).max(0) * DAY, t + DAY)
            } else {
                (t, t + n * DAY)
            }
        }
        "month" => {
            let d = Local.timestamp_millis_opt(t).single()?;
            let start = Local.with_ymd_and_hms(d.year(), d.month(), 1, 0, 0, 0).single()?.timestamp_millis();
            let (ny, nm) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
            (start, Local.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).single()?.timestamp_millis())
        }
        s => {
            let d = formula::parse_date(s)?;
            let d = formula::day_start(d);
            (d, d + DAY)
        }
    })
}

pub const DATE_PRESETS: [(&str, &str); 14] = [
    ("today", "今天"),
    ("tomorrow", "明天"),
    ("yesterday", "昨天"),
    ("week", "本周"),
    ("lastweek", "上周"),
    ("nextweek", "下周"),
    ("month", "本月"),
    ("lastmonth", "上月"),
    ("nextmonth", "下月"),
    ("last7", "过去 7 天"),
    ("next7", "未来 7 天"),
    ("last30", "过去 30 天"),
    ("next30", "未来 30 天"),
    ("", "指定日期"),
];

/// 「介于」的两个值（数组 [a, b] 或 "a~b"）。
pub fn pair(v: &Value) -> (String, String) {
    let s = |x: &Value| x.as_str().map(String::from).or_else(|| x.as_f64().map(fmt_num)).unwrap_or_default();
    match v {
        Value::Array(a) => (a.first().map(s).unwrap_or_default(), a.get(1).map(s).unwrap_or_default()),
        Value::String(x) => x.split_once('~').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or((x.clone(), String::new())),
        _ => (String::new(), String::new()),
    }
}

/// 公式环境：当前记录的字段、同名数据表（跨表 FILTER）、记录的字段。
struct BaseEnv<'a> {
    b: &'a Base,
    tid: String,
    rid: String,
}

impl formula::Env for BaseEnv<'_> {
    fn field(&mut self, name: &str) -> FVal {
        let Some(t) = self.b.table(&self.tid) else { return FVal::Null };
        if let Some(x) = t.field_by_name(name) {
            return self.b.fval(&self.tid, &self.rid, &x.id);
        }
        match self.b.table_list().into_iter().find(|t| t.meta.name == name) {
            Some(tt) => FVal::List(tt.order.iter().map(|r| FVal::Rec(tt.meta.id.clone(), r.clone())).collect()),
            None => FVal::Err(format!("字段「{name}」不存在")),
        }
    }
    fn rec_field(&mut self, tid: &str, rid: &str, name: &str) -> FVal {
        match self.b.table(tid).and_then(|t| t.field_by_name(name)) {
            Some(f) => self.b.fval(tid, rid, &f.id),
            None => FVal::Err(format!("字段「{name}」不存在")),
        }
    }
    fn record_id(&mut self) -> FVal {
        FVal::Str(self.rid.clone())
    }
    fn created(&mut self) -> FVal {
        self.b.record(&self.tid, &self.rid).map(|r| FVal::Date(r.created)).unwrap_or(FVal::Null)
    }
}

/// 值的比较键（多值逐个；数字统一格式，日期按天）。
fn lookup_keys(v: &FVal) -> Vec<String> {
    match v {
        FVal::List(l) => l.iter().flat_map(lookup_keys).collect(),
        FVal::Null => vec![],
        FVal::Date(d) => vec![fmt_date(*d, 2)],
        FVal::Num(n) => vec![fmt_num(*n)],
        v => vec![v.text().trim().to_lowercase()],
    }
}

/// 查找引用条件：目标表的值 `lhs` 与比较值 `rhs`。
pub fn lookup_cond_match(lhs: &FVal, op: Op, rhs: &FVal) -> bool {
    let eq = || {
        let (a, b) = (lookup_keys(lhs), lookup_keys(rhs));
        a.iter().any(|x| b.contains(x))
    };
    match op {
        Op::Empty => lhs.is_null(),
        Op::NotEmpty => !lhs.is_null(),
        Op::Is => eq(),
        Op::IsNot => !eq(),
        Op::Contains => {
            let b = rhs.text().to_lowercase();
            !b.is_empty() && lhs.text().to_lowercase().contains(&b)
        }
        Op::NotContains => !lhs.text().to_lowercase().contains(&rhs.text().to_lowercase()),
        Op::Checked => lhs.truthy(),
        Op::Unchecked => !lhs.truthy(),
        _ => {
            if lhs.is_null() || rhs.is_null() {
                return false;
            }
            let rhs = match (lhs, rhs) {
                (FVal::Date(_), FVal::Str(s)) => formula::parse_date(s).map(FVal::Date).unwrap_or(rhs.clone()),
                _ => rhs.clone(),
            };
            let o = formula::compare(&lhs.clone(), &rhs);
            match op {
                Op::Gt => o == Ordering::Greater,
                Op::Ge => o != Ordering::Less,
                Op::Lt => o == Ordering::Less,
                Op::Le => o != Ordering::Greater,
                _ => false,
            }
        }
    }
}

/// 把一种类型的值转换为另一种类型（修改字段类型时用）。`opts` 是新字段（会按需新增选项）。
pub fn convert_value(base: &Base, tid: &str, rid: &str, old: &Field, new: &mut Field) -> Option<Value> {
    let raw = base.record(tid, rid).and_then(|r| r.cells.get(&old.id)).cloned();
    raw.as_ref()?;
    if old.kind == new.kind || (old.kind.is_link() && new.kind.is_link() && old.prop.table == new.prop.table) {
        return raw;
    }
    let fv = base.fval(tid, rid, &old.id);
    let text = format_fval(old, &fv);
    let parts: Vec<String> = match &fv {
        FVal::List(l) => l.iter().map(|x| x.text()).filter(|s| !s.is_empty()).collect(),
        _ => text.split([',', '，', '、', ';', '；']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
    };
    use FieldType::*;
    match new.kind {
        Text | Url | Phone | Email => (!text.is_empty()).then(|| Value::from(text)),
        Number | Currency | Rating | Progress => {
            let n = fv.num().filter(|_| !fv.is_null()).or_else(|| text.replace([',', '¥', '$', '%'], "").trim().parse().ok())?;
            let n = if new.kind == Progress && text.ends_with('%') { n / 100. } else { n };
            serde_json::Number::from_f64(n).map(Value::Number)
        }
        Checkbox => Some(Value::Bool(fv.truthy() && !matches!(text.as_str(), "否" | "0" | "false" | "FALSE" | "未完成"))),
        Date => match fv {
            FVal::Date(d) => Some(Value::from(d)),
            _ => formula::parse_date(&text).map(Value::from),
        },
        Select | Stage => parts.first().map(|p| Value::from(new.ensure_opt(p))),
        MultiSelect => (!parts.is_empty()).then(|| Value::from(parts.iter().map(|p| new.ensure_opt(p)).collect::<Vec<_>>())),
        Person => (!parts.is_empty()).then(|| Value::from(parts)),
        Link | DuplexLink => {
            let target = base.table(&new.prop.table)?;
            let pid = target.primary()?.id.clone();
            let ids: Vec<String> = parts.iter().filter_map(|p| target.order.iter().find(|r| &base.text(&new.prop.table, r, &pid) == p).cloned()).collect();
            (!ids.is_empty()).then(|| Value::from(ids))
        }
        Attachment if old.kind == Attachment => raw,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::*;
    use serde_json::Value;

    fn sample() -> (Base, String) {
        let mut b = Base::new_default("t", ViewKind::Grid);
        b.persist = false;
        let tid = b.meta.active.clone();
        let mut qty = Field::new("数量", FieldType::Number);
        qty.id = "qty".into();
        let mut price = Field::new("单价", FieldType::Number);
        price.id = "price".into();
        let mut total = Field::new("金额", FieldType::Formula);
        total.id = "total".into();
        total.prop.expr = "[数量]*[单价]".into();
        for f in [qty, price, total] {
            let mut m = b.table(&tid).unwrap().meta.clone();
            m.fields.push(f.id.clone());
            b.put_table(m);
            b.put_field(&tid, f);
        }
        let rids = b.table(&tid).unwrap().order.clone();
        for (i, r) in rids.iter().enumerate() {
            b.set_cell(&tid, r, "qty", Value::from(i as f64 + 1.));
            b.set_cell(&tid, r, "price", Value::from(10.));
        }
        b.commit();
        (b, tid)
    }

    #[test]
    fn formula_field_and_sort() {
        let (b, tid) = sample();
        let rids = b.table(&tid).unwrap().order.clone();
        assert_eq!(b.text(&tid, &rids[2], "total"), "30");
        let mut v = View::new("v", ViewKind::Grid);
        v.sorts.push(SortRule { field: "total".into(), desc: true });
        let out = b.view_records(&tid, &v, "");
        assert_eq!(out[0], rids[4]);
    }

    #[test]
    fn filter_numbers() {
        let (b, tid) = sample();
        let mut v = View::new("v", ViewKind::Grid);
        v.filter.conds.push(Cond { field: "total".into(), op: Op::Ge, value: Value::from("30") });
        assert_eq!(b.view_records(&tid, &v, "").len(), 3);
        v.filter.any = true;
        v.filter.conds.push(Cond { field: "qty".into(), op: Op::Is, value: Value::from("1") });
        assert_eq!(b.view_records(&tid, &v, "").len(), 4);
    }

    #[test]
    fn formula_cycle_is_error() {
        let (mut b, tid) = sample();
        let mut f = b.field(&tid, "qty").unwrap().clone();
        f.kind = FieldType::Formula;
        f.prop.expr = "[金额]+1".into();
        b.put_field(&tid, f);
        b.commit();
        let r = b.table(&tid).unwrap().order[0].clone();
        assert!(b.text(&tid, &r, "total").contains("循环引用"));
    }

    #[test]
    fn lookup_sums_linked_records() {
        let (mut b, tid) = sample();
        let t2 = b.add_table("汇总", ViewKind::Grid, 1);
        let mut link = Field::new("明细", FieldType::Link);
        link.prop.table = tid.clone();
        let mut lk = Field::new("总额", FieldType::Lookup);
        lk.prop.link = link.id.clone();
        lk.prop.target = "total".into();
        lk.prop.agg = "sum".into();
        b.put_field(&t2, link.clone());
        b.put_field(&t2, lk.clone());
        b.commit();
        let r = b.table(&t2).unwrap().order[0].clone();
        let rids = b.table(&tid).unwrap().order.clone();
        b.set_cell(&t2, &r, &link.id, Value::from(vec![rids[0].clone(), rids[1].clone()]));
        b.commit();
        assert_eq!(b.text(&t2, &r, &lk.id), "30");
    }

    #[test]
    fn convert_text_to_select_creates_options() {
        let (mut b, tid) = sample();
        let fid = b.table(&tid).unwrap().meta.fields[0].clone();
        let rids = b.table(&tid).unwrap().order.clone();
        b.set_cell(&tid, &rids[0], &fid, Value::from("红"));
        b.set_cell(&tid, &rids[1], &fid, Value::from("绿"));
        b.commit();
        let old = b.field(&tid, &fid).unwrap().clone();
        let mut new = old.clone();
        new.kind = FieldType::Select;
        let v = super::convert_value(&b, &tid, &rids[1], &old, &mut new).unwrap();
        assert_eq!(new.prop.options.len(), 1);
        assert_eq!(v.as_str().unwrap(), new.prop.options[0].id);
    }
}
