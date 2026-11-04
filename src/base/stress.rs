//! 压测（T6）：生成「N 行 × 20 字段（5 个公式、2 个查找引用、1 个双向关联）」的多维表格，
//! 以及性能面板用的计时（视图管线、提交写库、载入）。

use super::model::*;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// 最近一次视图管线（筛选 + 排序 + 树化）耗时，微秒
pub static PIPE_US: AtomicU64 = AtomicU64::new(0);
/// 最近一次提交（含写库）耗时，微秒
pub static COMMIT_US: AtomicU64 = AtomicU64::new(0);
/// 最近一次打开多维表格（从 SQLite 载入）耗时，微秒
pub static LOAD_US: AtomicU64 = AtomicU64::new(0);
/// 性能面板是否打开（⌥⌘P，多维表格全屏时也显示自己的指标）
pub static SHOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn record(slot: &AtomicU64, t0: std::time::Instant) {
    slot.store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
}

pub fn ms(slot: &AtomicU64) -> f64 {
    slot.load(Ordering::Relaxed) as f64 / 1000.
}

/// 往多维表格里加一张压测数据表 + 一张客户表，返回压测表 id（不提交）。
pub fn build(b: &mut Base, rows: usize) -> String {
    let customers = b.add_table("压测-客户", ViewKind::Grid, 0);
    let main = b.add_table("压测-订单", ViewKind::Grid, 0);
    for t in [&customers, &main] {
        let defaults = b.table(t).map(|x| x.meta.fields.clone()).unwrap_or_default();
        let mut m = b.table(t).unwrap().meta.clone();
        m.fields.clear();
        b.put_table(m);
        for f in defaults {
            b.del_field(t, &f);
        }
    }
    let add = |b: &mut Base, t: &str, f: Field| -> String {
        let id = f.id.clone();
        let mut m = b.table(t).unwrap().meta.clone();
        m.fields.push(id.clone());
        b.put_table(m);
        b.put_field(t, f);
        id
    };
    let cname = add(b, &customers, Field::new("客户", FieldType::Text));
    let level = {
        let mut f = Field::new("等级", FieldType::Select);
        for n in ["A", "B", "C"] {
            f.ensure_opt(n);
        }
        add(b, &customers, f)
    };
    let mut cust_ids = vec![];
    let levels = b.field(&customers, &level).unwrap().prop.options.iter().map(|o| o.id.clone()).collect::<Vec<_>>();
    for i in 0..200 {
        let r = b.new_record(&customers, None).unwrap();
        let mut r2 = r.clone();
        r2.cells.insert(cname.clone(), Value::from(format!("客户{i:03}")));
        r2.cells.insert(level.clone(), Value::from(levels[i % 3].clone()));
        b.put_record(&customers, r2);
        cust_ids.push(r.id);
    }
    let title = add(b, &main, Field::new("订单号", FieldType::Text));
    let qty = add(b, &main, Field::new("数量", FieldType::Number));
    let price = add(b, &main, {
        let mut f = Field::new("单价", FieldType::Currency);
        f.prop.fmt = 2;
        f
    });
    let date = add(b, &main, Field::new("下单日期", FieldType::Date));
    let done = add(b, &main, Field::new("完成", FieldType::Checkbox));
    let status = add(b, &main, {
        let mut f = Field::new("状态", FieldType::Select);
        for n in ["待处理", "进行中", "已完成", "已取消"] {
            f.ensure_opt(n);
        }
        f
    });
    let tags = add(b, &main, {
        let mut f = Field::new("标签", FieldType::MultiSelect);
        for n in ["加急", "大客户", "复购", "海外"] {
            f.ensure_opt(n);
        }
        f
    });
    let owner = add(b, &main, Field::new("负责人", FieldType::Person));
    let rate = add(b, &main, Field::new("评分", FieldType::Rating));
    let prog = add(b, &main, Field::new("进度", FieldType::Progress));
    // 双向关联
    let mut l1 = Field::new("客户", FieldType::DuplexLink);
    let mut l2 = Field::new("订单", FieldType::DuplexLink);
    l1.prop.table = customers.clone();
    l1.prop.back = l2.id.clone();
    l1.prop.single = true;
    l2.prop.table = main.clone();
    l2.prop.back = l1.id.clone();
    let link = add(b, &main, l1);
    add(b, &customers, l2);
    // 查找引用 ×2
    add(b, &main, {
        let mut f = Field::new("客户等级", FieldType::Lookup);
        f.prop.link = link.clone();
        f.prop.target = level.clone();
        f
    });
    add(b, &main, {
        let mut f = Field::new("客户名", FieldType::Lookup);
        f.prop.link = link.clone();
        f.prop.target = cname.clone();
        f
    });
    // 公式 ×5
    for (n, e) in [
        ("金额", "[数量]*[单价]"),
        ("含税", "ROUND([金额]*1.13, 2)"),
        ("账龄", "DAYS(TODAY(), [下单日期])"),
        ("大单", "IF([金额]>5000, \"是\", \"否\")"),
        ("摘要", "[订单号] & \"-\" & [状态]"),
    ] {
        add(b, &main, {
            let mut f = Field::new(n, FieldType::Formula);
            f.prop.expr = e.into();
            f
        });
    }
    add(b, &main, Field::new("编号", FieldType::AutoNumber));
    let st_opts: Vec<String> = b.field(&main, &status).unwrap().prop.options.iter().map(|o| o.id.clone()).collect();
    let tag_opts: Vec<String> = b.field(&main, &tags).unwrap().prop.options.iter().map(|o| o.id.clone()).collect();
    let today = super::formula::today();
    let people = ["张三", "李四", "王五", "赵六", "钱七"];
    let mut back: std::collections::HashMap<String, Vec<String>> = Default::default();
    for i in 0..rows {
        let mut r = b.new_record(&main, None).unwrap();
        let x = (i as u64).wrapping_mul(2654435761) % 1000;
        r.cells.insert(title.clone(), Value::from(format!("SO-{i:06}")));
        r.cells.insert(qty.clone(), Value::from((x % 50 + 1) as f64));
        r.cells.insert(price.clone(), Value::from((x % 300) as f64 + 9.9));
        r.cells.insert(date.clone(), Value::from(today - (x as i64 % 365) * 86_400_000));
        r.cells.insert(done.clone(), Value::Bool(x.is_multiple_of(3)));
        r.cells.insert(status.clone(), Value::from(st_opts[(x % 4) as usize].clone()));
        r.cells.insert(tags.clone(), Value::from(vec![tag_opts[(x % 4) as usize].clone()]));
        r.cells.insert(owner.clone(), Value::from(vec![people[(x % 5) as usize]]));
        r.cells.insert(rate.clone(), Value::from((x % 5 + 1) as f64));
        r.cells.insert(prog.clone(), Value::from((x % 100) as f64 / 100.));
        let c = cust_ids[(x % cust_ids.len() as u64) as usize].clone();
        r.cells.insert(link.clone(), Value::from(vec![c.clone()]));
        back.entry(c).or_default().push(r.id.clone());
        b.put_record(&main, r);
    }
    let back_field = b.field(&main, &link).unwrap().prop.back.clone();
    for (c, list) in back {
        if let Some(mut r) = b.record(&customers, &c).cloned() {
            r.cells.insert(back_field.clone(), Value::from(list));
            b.put_record(&customers, r);
        }
    }
    let mut m = b.meta.clone();
    m.active = main.clone();
    b.set_meta(m);
    main
}
