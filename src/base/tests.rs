//! 多维表格数据层的全面回归测试：字段类型、筛选、排序、分组、统计、公式、查找引用、
//! 双向关联、类型转换、撤销重做、分数索引、导入导出、持久化。

use super::calc::{self, convert_value};
use super::formula::{self, FVal};
use super::io;
use super::model::*;
use serde_json::{json, Value};

/// 一张包含全部可填字段的表：返回 (base, 表 id, 字段名 → id)。
fn rich() -> (Base, String, std::collections::HashMap<&'static str, String>) {
    let mut b = Base::new_default("t", ViewKind::Grid);
    b.persist = false;
    let tid = b.meta.active.clone();
    let mut ids = std::collections::HashMap::new();
    let specs: [(&str, FieldType); 14] = [
        ("数量", FieldType::Number),
        ("单价", FieldType::Currency),
        ("进度", FieldType::Progress),
        ("评分", FieldType::Rating),
        ("完成", FieldType::Checkbox),
        ("标签", FieldType::MultiSelect),
        ("负责人", FieldType::Person),
        ("截止", FieldType::Date),
        ("网址", FieldType::Url),
        ("阶段", FieldType::Stage),
        ("编号", FieldType::AutoNumber),
        ("金额", FieldType::Formula),
        ("电话", FieldType::Phone),
        ("邮箱", FieldType::Email),
    ];
    for (name, kind) in specs {
        let mut f = Field::new(name, kind);
        if name == "金额" {
            f.prop.expr = "[数量]*[单价]".into();
        }
        if name == "标签" {
            for o in ["红", "绿", "蓝"] {
                f.ensure_opt(o);
            }
        }
        ids.insert(name, f.id.clone());
        let mut m = b.table(&tid).unwrap().meta.clone();
        m.fields.push(f.id.clone());
        b.put_table(m);
        b.put_field(&tid, f);
    }
    let t = b.table(&tid).unwrap();
    ids.insert("文本", t.meta.fields[0].clone());
    ids.insert("单选", t.meta.fields[1].clone());
    ids.insert("日期", t.meta.fields[2].clone());
    b.commit();
    (b, tid, ids)
}

fn rids(b: &Base, tid: &str) -> Vec<String> {
    b.table(tid).unwrap().order.clone()
}

fn set(b: &mut Base, tid: &str, rid: &str, fid: &str, text: &str) {
    let mut f = b.field(tid, fid).unwrap().clone();
    let v = io::text_to_value(&mut f, text).unwrap_or(Value::Null);
    b.put_field(tid, f);
    b.set_cell(tid, rid, fid, v);
}

fn filtered(b: &Base, tid: &str, conds: Vec<Cond>, any: bool) -> usize {
    let mut v = View::new("v", ViewKind::Grid);
    v.filter = Filter { any, conds };
    b.view_records(tid, &v, "").len()
}

fn cond(fid: &str, op: Op, value: Value) -> Cond {
    Cond { field: fid.into(), op, value }
}

// ───────────── 显示格式 ─────────────

#[test]
fn display_formats_for_every_type() {
    let (mut b, tid, f) = rich();
    let r = rids(&b, &tid)[0].clone();
    set(&mut b, &tid, &r, &f["数量"], "1234.5");
    set(&mut b, &tid, &r, &f["单价"], "¥12.5");
    set(&mut b, &tid, &r, &f["进度"], "60%");
    set(&mut b, &tid, &r, &f["评分"], "4");
    set(&mut b, &tid, &r, &f["完成"], "是");
    set(&mut b, &tid, &r, &f["标签"], "红,蓝");
    set(&mut b, &tid, &r, &f["负责人"], "张三、李四");
    set(&mut b, &tid, &r, &f["截止"], "2026-03-05");
    b.commit();
    assert_eq!(b.text(&tid, &r, &f["数量"]), "1234.5");
    assert_eq!(b.text(&tid, &r, &f["单价"]), "¥12.50");
    assert_eq!(b.text(&tid, &r, &f["进度"]), "60%");
    assert_eq!(b.text(&tid, &r, &f["评分"]), "4");
    assert_eq!(b.text(&tid, &r, &f["完成"]), "是");
    assert_eq!(b.text(&tid, &r, &f["标签"]), "红, 蓝");
    assert_eq!(b.text(&tid, &r, &f["负责人"]), "张三, 李四");
    assert_eq!(b.text(&tid, &r, &f["截止"]), "2026/03/05");
    assert_eq!(b.text(&tid, &r, &f["金额"]), "15431.25");
    assert_eq!(b.text(&tid, &r, &f["编号"]), "1");
}

#[test]
fn number_formats() {
    assert_eq!(calc::fmt_number(1234.5678, 1), "1235");
    assert_eq!(calc::fmt_number(1234.5678, 4), "1,235");
    assert_eq!(calc::fmt_number(-1234567.891, 5), "-1,234,567.89");
    assert_eq!(calc::fmt_number(0.256, 6), "25.6%");
    assert_eq!(calc::fmt_number(0.256, 7), "25.60%");
}

// ───────────── 筛选 ─────────────

#[test]
fn filters_by_type() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    let data = [
        ("苹果", "3", "红", "张三", "是", "2026-01-10"),
        ("香蕉", "5", "绿,蓝", "李四", "否", "2026-02-10"),
        ("橙子", "", "蓝", "张三,王五", "", ""),
        ("", "10", "", "", "是", "2026-03-10"),
    ];
    for (i, (t, n, tag, p, done, d)) in data.iter().enumerate() {
        set(&mut b, &tid, &rs[i], &f["文本"], t);
        set(&mut b, &tid, &rs[i], &f["数量"], n);
        set(&mut b, &tid, &rs[i], &f["标签"], tag);
        set(&mut b, &tid, &rs[i], &f["负责人"], p);
        set(&mut b, &tid, &rs[i], &f["完成"], done);
        set(&mut b, &tid, &rs[i], &f["截止"], d);
    }
    b.commit();
    let opt = |b: &Base, n: &str| b.field(&tid, &f["标签"]).unwrap().opt_by_name(n).unwrap().id.clone();
    let (red, blue) = (opt(&b, "红"), opt(&b, "蓝"));
    // 文本
    assert_eq!(filtered(&b, &tid, vec![cond(&f["文本"], Op::Contains, json!("果"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["文本"], Op::Is, json!("香蕉"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["文本"], Op::IsNot, json!("香蕉"))], false), 4);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["文本"], Op::Empty, Value::Null)], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["文本"], Op::NotContains, json!("果"))], false), 4);
    // 数字
    assert_eq!(filtered(&b, &tid, vec![cond(&f["数量"], Op::Gt, json!("3"))], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["数量"], Op::Le, json!("5"))], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["数量"], Op::Empty, Value::Null)], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["数量"], Op::IsNot, json!("3"))], false), 4, "不等于：空值也算不等于");
    // 多选
    assert_eq!(filtered(&b, &tid, vec![cond(&f["标签"], Op::Contains, json!([blue.clone()]))], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["标签"], Op::Is, json!([red.clone()]))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["标签"], Op::NotContains, json!([blue.clone()]))], false), 3);
    // 人员
    assert_eq!(filtered(&b, &tid, vec![cond(&f["负责人"], Op::Contains, json!("张三"))], false), 2);
    // 复选框
    assert_eq!(filtered(&b, &tid, vec![cond(&f["完成"], Op::Is, json!(true))], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["完成"], Op::Is, json!(false))], false), 3);
    // 日期
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Lt, json!("2026-02-10"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Gt, json!("2026-02-10"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Is, json!("2026-02-10"))], false), 1);
    // 组合
    assert_eq!(filtered(&b, &tid, vec![cond(&f["完成"], Op::Is, json!(true)), cond(&f["数量"], Op::Gt, json!("5"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["完成"], Op::Is, json!(true)), cond(&f["数量"], Op::Is, json!("5"))], true), 3);
    // 公式（数字结果）
    assert_eq!(filtered(&b, &tid, vec![cond(&f["金额"], Op::NotEmpty, Value::Null)], false), 5, "未填单价时公式按 0 计算");
}

#[test]
fn date_filter_presets() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    let today = formula::today();
    b.set_cell(&tid, &rs[0], &f["截止"], json!(today + 3_600_000));
    b.set_cell(&tid, &rs[1], &f["截止"], json!(today - 86_400_000));
    b.set_cell(&tid, &rs[2], &f["截止"], json!(today + 86_400_000));
    b.commit();
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Is, json!("today"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Is, json!("yesterday"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Is, json!("tomorrow"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Is, json!("last7"))], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Lt, json!("today"))], false), 1);
}

#[test]
fn search_matches_any_visible_field() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    set(&mut b, &tid, &rs[0], &f["负责人"], "王小明");
    b.commit();
    let v = View::new("v", ViewKind::Grid);
    assert_eq!(b.view_records(&tid, &v, "小明").len(), 1);
    assert_eq!(b.view_records(&tid, &v, "  ").len(), 5);
}

// ───────────── 排序 / 分组 / 统计 ─────────────

#[test]
fn sort_numbers_text_empty_last() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    for (i, n) in ["10", "2", "", "33", "2"].iter().enumerate() {
        set(&mut b, &tid, &rs[i], &f["数量"], n);
    }
    b.commit();
    let mut v = View::new("v", ViewKind::Grid);
    v.sorts.push(SortRule { field: f["数量"].clone(), desc: false });
    let out = b.view_records(&tid, &v, "");
    let vals: Vec<String> = out.iter().map(|r| b.text(&tid, r, &f["数量"])).collect();
    assert_eq!(vals, vec!["2", "2", "10", "33", ""]);
    v.sorts[0].desc = true;
    let out = b.view_records(&tid, &v, "");
    let vals: Vec<String> = out.iter().map(|r| b.text(&tid, r, &f["数量"])).collect();
    assert_eq!(vals, vec!["33", "10", "2", "2", ""], "降序时空值仍在最后");
    // 相同值保持手动顺序（稳定排序）
    let twos: Vec<&String> = out.iter().filter(|r| b.text(&tid, r, &f["数量"]) == "2").collect();
    assert_eq!(twos, vec![&rs[1], &rs[4]]);
}

#[test]
fn group_keys_and_order() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    set(&mut b, &tid, &rs[0], &f["标签"], "蓝");
    set(&mut b, &tid, &rs[1], &f["标签"], "红,绿");
    b.commit();
    let mut keys = vec![];
    for r in &rs {
        for k in b.group_keys(&tid, r, &f["标签"], false) {
            if !keys.iter().any(|x: &(String, String, Option<u8>)| x.0 == k.0) {
                keys.push(k);
            }
        }
    }
    b.order_groups(&tid, &f["标签"], &mut keys, false);
    let labels: Vec<&str> = keys.iter().map(|k| k.1.as_str()).collect();
    assert_eq!(labels, vec!["未分组", "红、绿", "蓝"]);
    // 看板拆分多值
    assert_eq!(b.group_keys(&tid, &rs[1], &f["标签"], true).len(), 2);
}

#[test]
fn stats() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    for (i, n) in ["1", "2", "3", "", ""].iter().enumerate() {
        set(&mut b, &tid, &rs[i], &f["单价"], n);
    }
    set(&mut b, &tid, &rs[0], &f["完成"], "是");
    b.commit();
    assert_eq!(b.stat(&tid, &rs, &f["单价"], "sum"), "求和 ¥6.00");
    assert_eq!(b.stat(&tid, &rs, &f["单价"], "avg"), "平均 ¥2.00");
    assert_eq!(b.stat(&tid, &rs, &f["单价"], "filled"), "已填写 3");
    assert_eq!(b.stat(&tid, &rs, &f["单价"], "empty"), "未填写 2");
    assert_eq!(b.stat(&tid, &rs, &f["单价"], "filled_pct"), "已填写 60%");
    assert_eq!(b.stat(&tid, &rs, &f["完成"], "checked"), "已勾选 1");
    assert_eq!(b.stat(&tid, &[], &f["单价"], "max"), "—");
}

// ───────────── 公式 ─────────────

fn run(src: &str) -> FVal {
    let e = formula::parse(src).unwrap();
    formula::eval(&e, &mut |f| match f {
        "日期" => FVal::Date(formula::parse_date("2026-01-31").unwrap()),
        "空" => FVal::Null,
        "文本数字" => FVal::Str("12".into()),
        _ => FVal::Null,
    })
}

#[test]
fn formula_edge_cases() {
    assert_eq!(run("TEXT(EDATE([日期], 1), \"YYYY-MM-DD\")"), FVal::Str("2026-02-28".into()), "EDATE 月末应截到当月最后一天");
    assert_eq!(run("[日期] > \"2026-01-01\""), FVal::Bool(true), "日期与日期文本比较");
    assert_eq!(run("[日期] = DATE(2026,1,31)"), FVal::Bool(true));
    assert_eq!(run("[文本数字] + 1"), FVal::Num(13.));
    assert_eq!(run("ROUND(1234.567, -2)"), FVal::Num(1200.));
    assert_eq!(run("MOD(-7, 3)"), FVal::Num(2.));
    assert_eq!(run("TEXT(0.256, \"0.0%\")"), FVal::Str("25.6%".into()));
    assert_eq!(run("TEXT(1234.5, \"0.00\")"), FVal::Str("1234.50".into()));
    assert_eq!(run("MID(\"abc\", 10, 2)"), FVal::Str("".into()));
    assert_eq!(run("LEFT(\"abc\", -1)"), FVal::Str("".into()));
    assert_eq!(run("IFS(1>2, \"a\", 2>1, \"b\")"), FVal::Str("b".into()));
    assert_eq!(run("SWITCH(5, 1, \"一\")"), FVal::Null);
    assert_eq!(run("DATEDIF(DATE(2024,2,29), DATE(2026,2,28), \"Y\")"), FVal::Num(1.));
    assert_eq!(run("ISBLANK([空])"), FVal::Bool(true));
    assert_eq!(run("[空] & \"x\""), FVal::Str("x".into()));
    assert_eq!(run("\"a\" = \"A\""), FVal::Bool(false));
    assert!(matches!(run("WORKDAY(DATE(2026,1,1), 1000000000)"), FVal::Err(_) | FVal::Date(_)), "超大工作日数不能卡死");
    assert!(matches!(run("SQRT(-1)"), FVal::Err(_)));
    assert!(formula::parse("1 +").is_err());
    assert!(formula::parse("\"abc").is_err());
    assert!(formula::parse("[abc").is_err());
}

#[test]
fn formula_on_lookup_list_and_chain() {
    let (mut b, tid, f) = rich();
    let t2 = b.add_table("订单", ViewKind::Grid, 0);
    let mut link = Field::new("明细", FieldType::Link);
    link.prop.table = tid.clone();
    let mut lk = Field::new("数量们", FieldType::Lookup);
    lk.prop.link = link.id.clone();
    lk.prop.target = f["金额"].clone();
    let mut sum = Field::new("合计", FieldType::Formula);
    sum.prop.expr = "SUM([数量们]) * 2".into();
    let mut lk2 = Field::new("计数", FieldType::Lookup);
    lk2.prop.link = link.id.clone();
    lk2.prop.target = f["金额"].clone();
    lk2.prop.agg = "count".into();
    for x in [&link, &lk, &sum, &lk2] {
        let mut m = b.table(&t2).unwrap().meta.clone();
        m.fields.push(x.id.clone());
        b.put_table(m);
        b.put_field(&t2, x.clone());
    }
    let r = b.new_record(&t2, None).unwrap().id;
    let rs = rids(&b, &tid);
    for (i, r) in rs.iter().enumerate() {
        set(&mut b, &tid, r, &f["数量"], &(i + 1).to_string());
        set(&mut b, &tid, r, &f["单价"], "10");
    }
    b.set_cell(&t2, &r, &link.id, json!([rs[0], rs[2]]));
    b.commit();
    assert_eq!(b.text(&t2, &r, &sum.id), "80");
    assert_eq!(b.text(&t2, &r, &lk2.id), "2");
    // 被引用的记录删除后自动不再计入
    b.del_record(&tid, &rs[2]);
    b.commit();
    assert_eq!(b.text(&t2, &r, &sum.id), "20");
    // 修改源数据后结果随之更新（缓存失效）
    set(&mut b, &tid, &rs[0], &f["数量"], "5");
    b.commit();
    assert_eq!(b.text(&t2, &r, &sum.id), "100");
}

#[test]
fn rename_field_updates_formula_refs() {
    let (mut b, tid, f) = rich();
    let r = rids(&b, &tid)[0].clone();
    set(&mut b, &tid, &r, &f["数量"], "2");
    set(&mut b, &tid, &r, &f["单价"], "3");
    b.commit();
    let mut q = b.field(&tid, &f["数量"]).unwrap().clone();
    q.name = "件数".into();
    b.rename_field(&tid, q);
    b.commit();
    assert_eq!(b.field(&tid, &f["金额"]).unwrap().prop.expr, "[件数]*[单价]");
    assert_eq!(b.text(&tid, &r, &f["金额"]), "6");
    b.undo();
    assert_eq!(b.field(&tid, &f["金额"]).unwrap().prop.expr, "[数量]*[单价]");
}

#[test]
fn cache_sees_uncommitted_changes() {
    // 同一次提交里先改值再读（看板拖动、新建记录预填都这么用）
    let (mut b, tid, f) = rich();
    let r = rids(&b, &tid)[0].clone();
    set(&mut b, &tid, &r, &f["数量"], "2");
    b.commit();
    assert_eq!(b.text(&tid, &r, &f["金额"]), "0");
    set(&mut b, &tid, &r, &f["单价"], "3");
    assert_eq!(b.text(&tid, &r, &f["金额"]), "6");
}

// ───────────── 关联 ─────────────

fn duplex(b: &mut Base, t1: &str, t2: &str) -> (Field, Field) {
    let mut f1 = Field::new("客户", FieldType::DuplexLink);
    let mut f2 = Field::new("订单", FieldType::DuplexLink);
    f1.prop.table = t2.into();
    f1.prop.back = f2.id.clone();
    f2.prop.table = t1.into();
    f2.prop.back = f1.id.clone();
    for (t, f) in [(t1, &f1), (t2, &f2)] {
        let mut m = b.table(t).unwrap().meta.clone();
        m.fields.push(f.id.clone());
        b.put_table(m);
        b.put_field(t, f.clone());
    }
    b.commit();
    (f1, f2)
}

#[test]
fn duplex_link_delete_and_duplicate_keep_both_sides_consistent() {
    let (mut b, t1, _) = rich();
    let t2 = b.add_table("客户", ViewKind::Grid, 2);
    let (f1, f2) = duplex(&mut b, &t1, &t2);
    let r1 = rids(&b, &t1)[0].clone();
    let c1 = rids(&b, &t2)[0].clone();
    b.set_cell(&t1, &r1, &f1.id, json!([c1]));
    b.commit();
    // 复制一条订单：客户那边也要多出这条订单
    let copy = b.duplicate_record(&t1, &r1).unwrap();
    b.commit();
    let back = str_list(b.record(&t2, &c1).unwrap().cells.get(&f2.id));
    assert!(back.contains(&copy), "复制记录后对面回链缺少副本：{back:?}");
    // 删除订单：客户那边的回链也要去掉
    b.delete_record(&t1, &r1);
    b.commit();
    let back = str_list(b.record(&t2, &c1).unwrap().cells.get(&f2.id));
    assert_eq!(back, vec![copy.clone()]);
    b.undo();
    let back = str_list(b.record(&t2, &c1).unwrap().cells.get(&f2.id));
    assert!(back.contains(&r1));
}

#[test]
fn single_link_keeps_last() {
    let (mut b, t1, _) = rich();
    let t2 = b.add_table("客户", ViewKind::Grid, 3);
    b.commit();
    let mut f = Field::new("客户", FieldType::Link);
    f.prop.table = t2.clone();
    f.prop.single = true;
    b.put_field(&t1, f.clone());
    let r = rids(&b, &t1)[0].clone();
    let cs = rids(&b, &t2);
    b.set_cell(&t1, &r, &f.id, json!([cs[0], cs[1]]));
    b.commit();
    assert_eq!(str_list(b.record(&t1, &r).unwrap().cells.get(&f.id)), vec![cs[1].clone()]);
}

// ───────────── 类型转换 ─────────────

#[test]
fn convert_between_types() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    set(&mut b, &tid, &rs[0], &f["文本"], "2026-05-06");
    set(&mut b, &tid, &rs[1], &f["文本"], "12.5");
    set(&mut b, &tid, &rs[2], &f["标签"], "红,绿");
    set(&mut b, &tid, &rs[3], &f["完成"], "是");
    set(&mut b, &tid, &rs[4], &f["进度"], "35%");
    b.commit();
    let conv = |b: &Base, r: &str, fid: &str, kind: FieldType| {
        let old = b.field(&tid, fid).unwrap().clone();
        let mut new = old.clone();
        new.kind = kind;
        new.prop = default_prop(kind);
        let v = convert_value(b, &tid, r, &old, &mut new);
        (v, new)
    };
    let (v, _) = conv(&b, &rs[0], &f["文本"], FieldType::Date);
    assert_eq!(v.and_then(|v| v.as_i64()), formula::parse_date("2026-05-06"));
    let (v, _) = conv(&b, &rs[1], &f["文本"], FieldType::Number);
    assert_eq!(v, Some(json!(12.5)));
    let (v, new) = conv(&b, &rs[2], &f["标签"], FieldType::Select);
    assert_eq!(new.opt(v.unwrap().as_str().unwrap()).unwrap().name, "红");
    let (v, _) = conv(&b, &rs[2], &f["标签"], FieldType::Text);
    assert_eq!(v, Some(json!("红, 绿")));
    let (v, _) = conv(&b, &rs[3], &f["完成"], FieldType::Text);
    assert_eq!(v, Some(json!("是")));
    let (v, _) = conv(&b, &rs[4], &f["进度"], FieldType::Number);
    assert_eq!(v, Some(json!(0.35)));
}

// ───────────── 撤销 / 顺序 ─────────────

#[test]
fn switching_view_is_not_an_undo_step() {
    let (mut b, tid, f) = rich();
    let r = rids(&b, &tid)[0].clone();
    set(&mut b, &tid, &r, &f["数量"], "7");
    b.commit();
    b.undo();
    assert!(b.can_redo());
    // 切换视图（记住当前视图）不能进撤销栈，也不能清掉重做
    let mut m = b.table(&tid).unwrap().meta.clone();
    m.active_view = "other".into();
    b.put_table(m);
    b.commit_silent();
    assert!(b.can_redo(), "切换视图把重做清掉了");
    b.redo();
    assert_eq!(b.text(&tid, &r, &f["数量"]), "7");
}

#[test]
fn fractional_rank_survives_many_inserts() {
    let (mut b, tid, _) = rich();
    let rs = rids(&b, &tid);
    let (a, z) = (rs[0].clone(), rs[1].clone());
    // 反复插在第一条之后，100 次后顺序仍然唯一且可预测
    let mut last = a.clone();
    let mut inserted = vec![];
    for _ in 0..100 {
        let t = b.table(&tid).unwrap();
        let ra = t.records[&a].rank;
        let i = t.order.iter().position(|x| x == &a).unwrap();
        let next = t.records[&t.order[i + 1]].rank;
        let r = b.new_record(&tid, Some(rank_between(Some(ra), Some(next)))).unwrap().id;
        b.commit();
        inserted.push(r.clone());
        last = r;
    }
    let order = rids(&b, &tid);
    let pos = |x: &String| order.iter().position(|y| y == x).unwrap();
    assert_eq!(pos(&last), 1, "最后插入的应紧跟第一条");
    assert!(pos(&z) > pos(&inserted[0]));
    let ranks: Vec<f64> = order.iter().map(|r| b.record(&tid, r).unwrap().rank).collect();
    assert!(ranks.windows(2).all(|w| w[0] < w[1]), "rank 出现重复：精度耗尽后没有重排");
}

#[test]
fn undo_redo_add_delete_field_and_records() {
    let (mut b, tid, f) = rich();
    let n0 = b.table(&tid).unwrap().fields.len();
    let mut m = b.table(&tid).unwrap().meta.clone();
    m.fields.retain(|x| x != &f["数量"]);
    b.put_table(m);
    b.del_field(&tid, &f["数量"]);
    b.commit();
    assert_eq!(b.table(&tid).unwrap().fields.len(), n0 - 1);
    b.undo();
    assert_eq!(b.table(&tid).unwrap().fields.len(), n0);
    assert!(b.table(&tid).unwrap().meta.fields.contains(&f["数量"]));
    let r = b.new_record(&tid, None).unwrap().id;
    b.commit();
    b.undo();
    assert!(b.record(&tid, &r).is_none());
    b.redo();
    assert!(b.record(&tid, &r).is_some());
    assert_eq!(rids(&b, &tid).last(), Some(&r));
}

// ───────────── 导入导出 ─────────────

#[test]
fn csv_quotes_newlines_and_bom() {
    let rows = io::parse_csv("\u{feff}名称,备注\r\n\"甲\",\"第一行\n第二行\"\r\n乙,\"含\"\"引号\"\"\"\r\n");
    assert_eq!(rows, vec![vec!["名称", "备注"], vec!["甲", "第一行\n第二行"], vec!["乙", "含\"引号\""]]);
    assert_eq!(io::csv_escape("a\"b"), "\"a\"\"b\"");
    let rows = io::parse_csv("a\tb\n1\t2\n");
    assert_eq!(rows[1], vec!["1", "2"]);
}

#[test]
fn import_infers_types_and_progress() {
    let rows = io::parse_csv("名称,进度,金额,网址\n甲,50%,$3,https://a.com\n乙,20%,$4.5,https://b.com\n");
    let mut b = Base::new_default("t", ViewKind::Grid);
    b.persist = false;
    let tid = io::import_rows(&mut b, "导入", rows).unwrap();
    b.commit();
    let t = b.table(&tid).unwrap();
    let kinds: Vec<FieldType> = t.field_list().iter().map(|f| f.kind).collect();
    assert_eq!(kinds, vec![FieldType::Text, FieldType::Progress, FieldType::Currency, FieldType::Url]);
    let r = t.order[0].clone();
    let fid = t.meta.fields[1].clone();
    assert_eq!(b.text(&tid, &r, &fid), "50%");
}

#[test]
fn text_to_value_rules() {
    let mut f = Field::new("n", FieldType::Number);
    assert_eq!(io::text_to_value(&mut f, "1,234"), Some(json!(1234.0)));
    assert_eq!(io::text_to_value(&mut f, "abc"), None);
    let mut f = Field::new("r", FieldType::Rating);
    assert_eq!(io::text_to_value(&mut f, "9"), Some(json!(5.0)), "评分不能超过上限");
    let mut f = Field::new("c", FieldType::Checkbox);
    assert_eq!(io::text_to_value(&mut f, "否"), Some(json!(false)));
}

// ───────────── 持久化 ─────────────

#[test]
fn persist_roundtrip_through_sqlite() {
    let _env = crate::store::TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("stonebrook-base-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::env::set_var(crate::store::DATA_DIR_ENV, &dir);
    crate::store::install(crate::store::Store::open().unwrap());
    let (mut b, tid, f) = rich();
    b.persist = true;
    b.save_all();
    let r = rids(&b, &tid)[0].clone();
    set(&mut b, &tid, &r, &f["数量"], "42");
    b.commit();
    let back = super::db::load("t", b.meta.clone());
    assert_eq!(back.text(&tid, &r, &f["数量"]), "42");
    assert_eq!(back.table(&tid).unwrap().order, b.table(&tid).unwrap().order);
    assert_eq!(back.table(&tid).unwrap().meta.fields, b.table(&tid).unwrap().meta.fields);
    let _ = std::fs::remove_dir_all(&dir);
}

// ═════════════ P0–P3 新功能 ═════════════

/// 精确失效与全量重算结果一致（T1）：随机修改后逐格比较。
#[test]
fn precise_invalidation_matches_full_recompute() {
    let (mut b, t1, f) = rich();
    let t2 = b.add_table("汇总", ViewKind::Grid, 3);
    b.commit();
    let mut link = Field::new("明细", FieldType::Link);
    link.prop.table = t1.clone();
    let mut lk = Field::new("金额合计", FieldType::Lookup);
    lk.prop.link = link.id.clone();
    lk.prop.target = f["金额"].clone();
    lk.prop.agg = "sum".into();
    let mut fx = Field::new("翻倍", FieldType::Formula);
    fx.prop.expr = "[金额合计]*2 + [数据表].FILTER(CurrentValue.[完成]).[数量].SUM()".into();
    for x in [&link, &lk, &fx] {
        let mut m = b.table(&t2).unwrap().meta.clone();
        m.fields.push(x.id.clone());
        b.put_table(m);
        b.put_field(&t2, x.clone());
    }
    b.commit();
    let r1 = rids(&b, &t1);
    let r2 = rids(&b, &t2);
    let mut seed = 7u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    for step in 0..60 {
        match rnd(4) {
            0 => {
                let r = r1[rnd(r1.len())].clone();
                set(&mut b, &t1, &r, &f["数量"], &rnd(20).to_string());
            }
            1 => {
                let r = r1[rnd(r1.len())].clone();
                set(&mut b, &t1, &r, &f["完成"], if rnd(2) == 0 { "是" } else { "否" });
            }
            2 => {
                let r = r2[rnd(r2.len())].clone();
                let pick: Vec<String> = r1.iter().filter(|_| rnd(2) == 0).cloned().collect();
                b.set_cell(&t2, &r, &link.id, json!(pick));
            }
            _ => {
                let r = r1[rnd(r1.len())].clone();
                set(&mut b, &t1, &r, &f["单价"], &rnd(9).to_string());
            }
        }
        if step % 3 == 0 {
            b.commit();
        }
        // 先读一遍（填充缓存），再和全新计算比较
        let all = |b: &Base| -> Vec<String> {
            [(&t1, &r1, vec![f["金额"].clone()]), (&t2, &r2, vec![lk.id.clone(), fx.id.clone()])]
                .iter()
                .flat_map(|(t, rs, fs)| rs.iter().flat_map(move |r| fs.iter().map(move |x| b.text(t, r, x))))
                .collect()
        };
        let cached = all(&b);
        b.cache.borrow_mut().clear();
        assert_eq!(cached, all(&b), "第 {step} 步：精确失效与全量重算不一致");
    }
}

#[test]
fn conditional_lookup_without_link() {
    let (mut b, t1, f) = rich();
    let t2 = b.add_table("客户", ViewKind::Grid, 2);
    b.commit();
    let rs = rids(&b, &t1);
    for (i, who) in ["甲", "乙", "甲", "丙", "甲"].iter().enumerate() {
        set(&mut b, &t1, &rs[i], &f["负责人"], who);
        set(&mut b, &t1, &rs[i], &f["数量"], &(i + 1).to_string());
    }
    let cs = rids(&b, &t2);
    let name = b.table(&t2).unwrap().meta.fields[0].clone();
    b.set_cell(&t2, &cs[0], &name, json!("甲"));
    b.set_cell(&t2, &cs[1], &name, json!("丙"));
    let mut lk = Field::new("数量合计", FieldType::Lookup);
    lk.prop.lookup_table = t1.clone();
    lk.prop.target = f["数量"].clone();
    lk.prop.agg = "sum".into();
    lk.prop.conds = vec![LookupCond { field: f["负责人"].clone(), op: Op::Is, rhs: name.clone(), value: Value::Null }];
    let mut cnt = lk.clone();
    cnt.id = new_id("fld");
    cnt.name = "大单数".into();
    cnt.prop.agg = "count".into();
    cnt.prop.conds.push(LookupCond { field: f["数量"].clone(), op: Op::Gt, rhs: String::new(), value: json!(2) });
    b.put_field(&t2, lk.clone());
    b.put_field(&t2, cnt.clone());
    b.commit();
    assert_eq!(b.text(&t2, &cs[0], &lk.id), "9");
    assert_eq!(b.text(&t2, &cs[1], &lk.id), "4");
    assert_eq!(b.text(&t2, &cs[0], &cnt.id), "2");
    // 源表改动后索引与缓存都要失效
    set(&mut b, &t1, &rs[1], &f["负责人"], "甲");
    b.commit();
    assert_eq!(b.text(&t2, &cs[0], &lk.id), "11");
}

#[test]
fn cross_table_filter_and_list_functions() {
    let (mut b, t1, f) = rich();
    let rs = rids(&b, &t1);
    for (i, r) in rs.iter().enumerate() {
        set(&mut b, &t1, r, &f["数量"], &((i + 1) * 10).to_string());
        set(&mut b, &t1, r, &f["负责人"], if i % 2 == 0 { "甲" } else { "乙" });
    }
    let t2 = b.add_table("人员", ViewKind::Grid, 1);
    b.commit();
    let c = rids(&b, &t2)[0].clone();
    let name = b.table(&t2).unwrap().meta.fields[0].clone();
    b.set_cell(&t2, &c, &name, json!("甲"));
    let mut fx = Field::new("业绩", FieldType::Formula);
    fx.prop.expr = "[数据表].FILTER(CurrentValue.[负责人] = [文本]).[数量].SUM()".into();
    let mut g = Field::new("明细", FieldType::Formula);
    g.prop.expr = "[数据表].FILTER(CurrentValue.[负责人] = [文本]).[数量].SORT(FALSE).ARRAYJOIN(\"/\")".into();
    let mut m = Field::new("映射", FieldType::Formula);
    m.prop.expr = "LIST(1,2,3).MAP(CurrentValue * 2).ARRAYJOIN(\",\")".into();
    for x in [&fx, &g, &m] {
        b.put_field(&t2, x.clone());
    }
    b.commit();
    assert_eq!(b.text(&t2, &c, &fx.id), "90");
    assert_eq!(b.text(&t2, &c, &g.id), "50/30/10");
    assert_eq!(b.text(&t2, &c, &m.id), "2,4,6");
}

#[test]
fn new_formula_functions() {
    let r = |s: &str| formula::eval(&formula::parse(s).unwrap(), &mut |_| FVal::Null);
    assert_eq!(r("MEDIAN(3, 1, 4, 2)"), FVal::Num(2.5));
    assert_eq!(r("RANK(30, LIST(10, 30, 20))"), FVal::Num(1.));
    assert_eq!(r("RANK(30, LIST(10, 30, 20), TRUE)"), FVal::Num(3.));
    assert_eq!(r("ISNUMBER(1)"), FVal::Bool(true));
    assert_eq!(r("ISERROR(1/0)"), FVal::Bool(true));
    assert_eq!(r("ROUND(LOG(1000), 6)"), FVal::Num(3.));
    assert_eq!(r("REGEXEXTRACT(\"订单 A-1023 已发货\", \"A-(\\d+)\")"), FVal::Str("1023".into()));
    assert_eq!(r("REGEXREPLACE(\"a1b22\", \"[0-9]+\", \"#\")"), FVal::Str("a#b#".into()));
    assert_eq!(r("REGEXMATCH(\"abc\", \"^a\")"), FVal::Bool(true));
    assert_eq!(r("NTH(LIST(\"x\",\"y\",\"z\"), 2)"), FVal::Str("y".into()));
    assert_eq!(r("LAST(LIST(1,2,3))"), FVal::Num(3.));
    assert_eq!(r("SORTBY(LIST(\"a\",\"b\",\"c\"), LIST(3,1,2)).ARRAYJOIN()"), FVal::Str("b,c,a".into()));
    assert_eq!(r("LIST(5,8,1).FILTER(CurrentValue > 3).SUM()"), FVal::Num(13.));
    assert_eq!(r("TEXT(WORKDAY.INTL(DATE(2026,10,30), 1, \"0000011\"), \"YYYY-MM-DD\")"), FVal::Str("2026-11-02".into()));
    assert!(formula::parse("LIST(1).NOPE()").is_err());
}

#[test]
fn filter_new_operators() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    for (i, n) in ["1", "5", "9", "", "12"].iter().enumerate() {
        set(&mut b, &tid, &rs[i], &f["数量"], n);
    }
    set(&mut b, &tid, &rs[0], &f["完成"], "是");
    set(&mut b, &tid, &rs[0], &f["截止"], "2026-03-01");
    set(&mut b, &tid, &rs[1], &f["截止"], "2026-03-15");
    set(&mut b, &tid, &rs[2], &f["截止"], "2026-04-02");
    b.commit();
    assert_eq!(filtered(&b, &tid, vec![cond(&f["数量"], Op::Between, json!(["5", "10"]))], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["数量"], Op::Between, json!("2~"))], false), 3);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Between, json!(["2026-03-01", "2026-03-31"]))], false), 2);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["完成"], Op::Checked, Value::Null)], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["完成"], Op::Unchecked, Value::Null)], false), 4);
    let today = formula::today();
    b.set_cell(&tid, &rs[3], &f["截止"], json!(today - 10 * 86_400_000));
    b.commit();
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Is, json!("past:14"))], false), 1);
    assert_eq!(filtered(&b, &tid, vec![cond(&f["截止"], Op::Is, json!("past:7"))], false), 0);
}

#[test]
fn hierarchy_tree_cycle_depth_and_delete() {
    let (mut b, tid, _) = rich();
    let pf = b.enable_hierarchy(&tid).unwrap();
    b.commit();
    let rs = rids(&b, &tid);
    // rs0 ─ rs1 ─ rs2 ；rs3 顶级；rs4 → rs0
    b.set_parent(&tid, &rs[1], Some(&rs[0])).unwrap();
    b.set_parent(&tid, &rs[2], Some(&rs[1])).unwrap();
    b.set_parent(&tid, &rs[4], Some(&rs[0])).unwrap();
    b.commit();
    assert!(b.set_parent(&tid, &rs[0], Some(&rs[2])).is_err(), "成环");
    assert!(b.set_parent(&tid, &rs[0], Some(&rs[0])).is_err());
    let tree = b.tree(&tid, &rs, &[]);
    let shape: Vec<(String, usize)> = tree.iter().map(|x| (x.0.clone(), x.1)).collect();
    assert_eq!(shape, vec![(rs[0].clone(), 0), (rs[1].clone(), 1), (rs[2].clone(), 2), (rs[4].clone(), 1), (rs[3].clone(), 0)]);
    // 只命中第 3 级时补出祖先
    let only = b.tree(&tid, &[rs[2].clone()], &[]);
    assert_eq!(only.iter().map(|x| (x.0.clone(), x.2)).collect::<Vec<_>>(), vec![(rs[0].clone(), false), (rs[1].clone(), false), (rs[2].clone(), true)]);
    // 折叠
    assert_eq!(b.tree(&tid, &rs, &[format!("t:{}", rs[0])]).len(), 2);
    // 深度上限：再往下挂 3 层后第 6 层被拒绝
    let mut last = rs[2].clone();
    for _ in 0..2 {
        let n = b.new_record(&tid, None).unwrap().id;
        b.set_parent(&tid, &n, Some(&last)).unwrap();
        last = n;
    }
    let n = b.new_record(&tid, None).unwrap().id;
    assert!(b.set_parent(&tid, &n, Some(&last)).is_err());
    b.commit();
    // 删除中间节点：子记录挂到祖父
    b.delete_record(&tid, &rs[1]);
    b.commit();
    assert_eq!(b.parent_of(&tid, &rs[2]).as_deref(), Some(rs[0].as_str()));
    b.undo();
    assert_eq!(b.parent_of(&tid, &rs[2]).as_deref(), Some(rs[1].as_str()));
    assert_eq!(b.table(&tid).unwrap().meta.parent_field.as_deref(), Some(pf.as_str()));
}

#[test]
fn color_rules_priority() {
    let (mut b, tid, f) = rich();
    let rs = rids(&b, &tid);
    set(&mut b, &tid, &rs[0], &f["数量"], "100");
    set(&mut b, &tid, &rs[1], &f["数量"], "5");
    b.commit();
    let mut v = View::new("v", ViewKind::Grid);
    let rule = |kind, field: &str, cond: Option<Cond>, color| ColorRule { id: new_id("clr"), kind, field: field.into(), cond, color };
    v.colors = vec![
        rule(0, &f["数量"], Some(cond(&f["数量"], Op::Gt, json!("50"))), 1),
        rule(0, &f["数量"], Some(cond(&f["数量"], Op::Gt, json!("1"))), 2),
        rule(1, &f["文本"], Some(cond(&f["数量"], Op::Lt, json!("10"))), 3),
        rule(2, &f["单价"], None, 4),
    ];
    let c = b.colorize(&tid, &v, &rs);
    assert_eq!(c.at(&rs[0], &f["数量"]), Some(1), "同类规则取第一条命中的");
    assert_eq!(c.at(&rs[1], &f["数量"]), Some(2), "单元格规则优先于整行");
    assert_eq!(c.at(&rs[1], &f["单价"]), Some(3), "整行优先于整列");
    assert_eq!(c.at(&rs[2], &f["单价"]), Some(4));
    assert_eq!(c.at(&rs[2], &f["数量"]), None);
}

#[test]
fn defaults_and_field_usages() {
    let (mut b, tid, f) = rich();
    let mut q = b.field(&tid, &f["数量"]).unwrap().clone();
    q.prop.default = json!(3);
    b.put_field(&tid, q);
    let mut d = b.field(&tid, &f["截止"]).unwrap().clone();
    d.prop.default = json!("today");
    b.put_field(&tid, d);
    let r = b.new_record(&tid, None).unwrap().id;
    b.commit();
    assert_eq!(b.text(&tid, &r, &f["数量"]), "3");
    assert_eq!(b.record(&tid, &r).unwrap().cells[&f["截止"]], json!(formula::today()));
    let mut v = b.table(&tid).unwrap().views.values().next().unwrap().clone();
    v.sorts.push(SortRule { field: f["数量"].clone(), desc: false });
    b.put_view(&tid, v);
    b.commit();
    let u = b.field_usages(&tid, &f["数量"]);
    assert!(u.iter().any(|x| x.contains("金额")), "{u:?}");
    assert!(u.iter().any(|x| x.contains("排序")), "{u:?}");
}

#[test]
fn delete_record_clears_one_way_links() {
    let (mut b, t1, _) = rich();
    let t2 = b.add_table("客户", ViewKind::Grid, 2);
    b.commit();
    let mut l = Field::new("客户", FieldType::Link);
    l.prop.table = t2.clone();
    b.put_field(&t1, l.clone());
    let r = rids(&b, &t1)[0].clone();
    let cs = rids(&b, &t2);
    b.set_cell(&t1, &r, &l.id, json!([cs[0], cs[1]]));
    b.commit();
    b.delete_record(&t2, &cs[0]);
    b.commit();
    assert_eq!(str_list(b.record(&t1, &r).unwrap().cells.get(&l.id)), vec![cs[1].clone()]);
    b.undo();
    assert_eq!(str_list(b.record(&t1, &r).unwrap().cells.get(&l.id)).len(), 2);
}

#[test]
fn automations_create_change_match_cascade_and_undo() {
    let (mut b, tid, f) = rich();
    let log = b.add_table("日志", ViewKind::Grid, 0);
    b.commit();
    let log_name = b.table(&log).unwrap().meta.fields[0].clone();
    let mut m = b.meta.clone();
    m.automations = vec![
        Automation {
            id: "a1".into(),
            name: "完成即满分".into(),
            on: true,
            trigger: "match".into(),
            table: tid.clone(),
            cond: Filter { any: false, conds: vec![cond(&f["完成"], Op::Checked, Value::Null)] },
            actions: vec![AutoAction { kind: "update".into(), sets: vec![FieldSet { field: f["评分"].clone(), value: json!(5) }], ..Default::default() }],
            ..Default::default()
        },
        Automation {
            id: "a2".into(),
            name: "评分变化记日志".into(),
            on: true,
            trigger: "change".into(),
            table: tid.clone(),
            field: f["评分"].clone(),
            actions: vec![
                AutoAction {
                    kind: "create".into(),
                    table: log.clone(),
                    sets: vec![FieldSet { field: log_name.clone(), value: json!("{{[文本]}} 得了 {{[评分]}} 分") }],
                    ..Default::default()
                },
                AutoAction { kind: "notify".into(), text: "{{[文本]}} 满分".into(), ..Default::default() },
            ],
            ..Default::default()
        },
    ];
    b.set_meta(m);
    b.commit();
    let r = rids(&b, &tid)[0].clone();
    set(&mut b, &tid, &r, &f["文本"], "苹果");
    b.commit();
    set(&mut b, &tid, &r, &f["完成"], "是");
    b.commit();
    assert_eq!(b.text(&tid, &r, &f["评分"]), "5");
    let lr = rids(&b, &log);
    assert_eq!(lr.len(), 1);
    assert_eq!(b.text(&log, &lr[0], &log_name), "苹果 得了 5 分");
    assert_eq!(b.notices.len(), 1);
    // 再改一次别的字段不会重复触发「满足条件」
    set(&mut b, &tid, &r, &f["数量"], "3");
    b.commit();
    assert_eq!(rids(&b, &log).len(), 1);
    // 自动化的修改和触发它的修改是同一步撤销
    b.undo();
    b.undo();
    assert_eq!(b.text(&tid, &r, &f["评分"]), "");
    assert!(rids(&b, &log).is_empty());
}

#[test]
fn button_sets_fields_and_builds_url() {
    let (mut b, tid, f) = rich();
    let mut btn = Field::new("标记完成", FieldType::Button);
    btn.prop.sets = vec![FieldSet { field: f["完成"].clone(), value: json!(true) }, FieldSet { field: f["截止"].clone(), value: json!("today") }];
    btn.prop.url = "example.com/item/{{[编号]}}".into();
    b.put_field(&tid, btn.clone());
    b.commit();
    let r = rids(&b, &tid)[1].clone();
    let url = b.press_button(&tid, &r, &btn.id);
    b.commit();
    assert_eq!(url.as_deref(), Some("https://example.com/item/2"));
    assert_eq!(b.text(&tid, &r, &f["完成"]), "是");
    assert_eq!(b.record(&tid, &r).unwrap().cells[&f["截止"]], json!(formula::today()));
}

#[test]
fn append_import_and_xlsx_roundtrip() {
    let (mut b, tid, f) = rich();
    let rows = io::parse_csv("文本,数量,标签,新列\n梨,7,红,x\n桃,8,紫,y\n");
    let n0 = rids(&b, &tid).len();
    assert_eq!(io::append_rows(&mut b, &tid, rows), 2);
    b.commit();
    let rs = rids(&b, &tid);
    assert_eq!(rs.len(), n0 + 2);
    let last = rs.last().unwrap();
    assert_eq!(b.text(&tid, last, &f["数量"]), "8");
    assert_eq!(b.text(&tid, last, &f["标签"]), "紫", "不存在的选项自动新建");
    assert!(b.table(&tid).unwrap().field_by_name("新列").is_some());
    let path = std::env::temp_dir().join(format!("stonebrook-x-{}.xlsx", std::process::id()));
    let v = b.table(&tid).unwrap().views.values().next().unwrap().clone();
    io::export_xlsx(&b, &[(tid.clone(), v)], path.to_str().unwrap()).unwrap();
    let sheets = io::read_xlsx_sheets(path.to_str().unwrap());
    let _ = std::fs::remove_file(&path);
    assert_eq!(sheets.len(), 1);
    let (_, rows) = &sheets[0];
    assert_eq!(rows[0][0], "文本");
    assert!(rows.iter().any(|r| r[0] == "桃"));
}

/// 性能基线（T6）：cargo test --release perf_baseline -- --ignored --nocapture
#[test]
#[ignore]
fn perf_baseline() {
    let n: usize = std::env::var("BASE_ROWS").ok().and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let mut b = Base::new_default("perf", ViewKind::Grid);
    b.persist = false;
    let t0 = std::time::Instant::now();
    let tid = super::stress::build(&mut b, n);
    b.commit();
    println!("生成 {n} 行：{:?}", t0.elapsed());
    let t = b.table(&tid).unwrap();
    let f = |name: &str| t.field_by_name(name).unwrap().id.clone();
    let (amt, status, qty) = (f("金额"), f("状态"), f("数量"));
    let mut v = View::new("v", ViewKind::Grid);
    let t0 = std::time::Instant::now();
    let all = b.view_records(&tid, &v, "");
    println!("无筛选管线：{:?}（{} 行）", t0.elapsed(), all.len());
    v.sorts.push(SortRule { field: amt.clone(), desc: true });
    v.filter.conds.push(Cond { field: status.clone(), op: Op::IsNot, value: json!([b.field(&tid, &status).unwrap().prop.options[3].id.clone()]) });
    let t0 = std::time::Instant::now();
    let out = b.view_records(&tid, &v, "");
    println!("首次 筛选 + 按公式排序（含计算）：{:?}（{} 行）", t0.elapsed(), out.len());
    let t0 = std::time::Instant::now();
    let _ = b.view_records(&tid, &v, "");
    println!("缓存命中后再跑管线：{:?}", t0.elapsed());
    // 改一个格子 → 提交 → 管线
    let r = out[10].clone();
    let t0 = std::time::Instant::now();
    b.set_cell(&tid, &r, &qty, json!(999));
    b.commit();
    let c1 = t0.elapsed();
    let _ = b.view_records(&tid, &v, "");
    println!("改一个格子：提交 {c1:?}，提交 + 管线 {:?}", t0.elapsed());
    let c = b.cache.borrow();
    println!("缓存命中 {} / 未命中 {}", c.hits, c.misses);
}

/// 增量视图更新与全量重算一致：改格子、新增、删除、改排序字段。
#[test]
fn incremental_rows_match_full() {
    let (mut b, tid, f) = rich();
    for i in 0..30 {
        let r = b.new_record(&tid, None).unwrap().id;
        b.set_cell(&tid, &r, &f["数量"], json!((i * 7 % 13) as f64));
    }
    b.commit();
    let mut v = View::new("v", ViewKind::Grid);
    v.sorts.push(SortRule { field: f["金额"].clone(), desc: true });
    v.filter.conds.push(cond(&f["数量"], Op::Gt, json!("2")));
    let mut rows = b.view_records(&tid, &v, "");
    let mut ver = b.ver;
    let all = rids(&b, &tid);
    for step in 0..40 {
        let r = all[(step * 11) % all.len()].clone();
        match step % 4 {
            0 => set(&mut b, &tid, &r, &f["数量"], &((step * 5) % 17).to_string()),
            1 => set(&mut b, &tid, &r, &f["单价"], &((step * 3) % 7).to_string()),
            2 => {
                let n = b.new_record(&tid, None).unwrap().id;
                b.set_cell(&tid, &n, &f["数量"], json!(step as f64));
            }
            _ => set(&mut b, &tid, &r, &f["文本"], "x"),
        }
        b.commit();
        assert!(b.refresh_rows(&tid, &v, "", &mut rows, ver), "第 {step} 步不应退回全量重算");
        ver = b.ver;
        assert_eq!(rows, b.view_records(&tid, &v, ""), "第 {step} 步增量结果不一致");
    }
    // 改字段定义 → 必须全量
    let mut q = b.field(&tid, &f["数量"]).unwrap().clone();
    q.name = "件数".into();
    b.rename_field(&tid, q);
    b.commit();
    assert!(!b.refresh_rows(&tid, &v, "", &mut rows, ver));
}

/// 性能：10 万行改一个格子后增量刷新视图（cargo test --release perf_incremental -- --ignored --nocapture）
#[test]
#[ignore]
fn perf_incremental() {
    let mut b = Base::new_default("perf", ViewKind::Grid);
    b.persist = false;
    let tid = super::stress::build(&mut b, 100_000);
    b.commit();
    let t = b.table(&tid).unwrap();
    let id = |n: &str| t.field_by_name(n).unwrap().id.clone();
    let (amt, qty, status, title) = (id("金额"), id("数量"), id("状态"), id("订单号"));
    let mut v = View::new("v", ViewKind::Grid);
    v.sorts.push(SortRule { field: amt, desc: true });
    v.filter.conds.push(Cond { field: status.clone(), op: Op::IsNot, value: json!([b.field(&tid, &status).unwrap().prop.options[3].id.clone()]) });
    let mut rows = b.view_records(&tid, &v, "");
    let mut ver = b.ver;
    for (name, fid, val) in [("排序字段相关（数量 → 金额）", qty.clone(), json!(999)), ("无关文本字段", title.clone(), json!("改名"))] {
        let r = rows[100].clone();
        let t0 = std::time::Instant::now();
        b.set_cell(&tid, &r, &fid, val);
        b.commit();
        let c = t0.elapsed();
        let ok = b.refresh_rows(&tid, &v, "", &mut rows, ver);
        ver = b.ver;
        println!("改一个格子（{name}）：提交 {c:?}，提交 + 增量刷新 {:?}，增量={ok}", t0.elapsed());
    }
}
