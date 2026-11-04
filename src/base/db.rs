//! 多维表格的 SQLite 存储：字段 / 记录 / 视图逐行存，改一个格子只写一行。
//!
//! objects 表里仍有一条 kind='base' 的对象，内容只是 `BaseMeta`（名称、数据表顺序），
//! 文档里的 `Embed { kind: "base", id }` 引用方式与画板、电子表格一致。

use super::model::*;
use crate::store::Store;
use rusqlite::params;

/// 修改历史保留天数
const HISTORY_DAYS: i64 = 180;

fn store() -> Option<&'static Store> {
    crate::store::global()
}

/// 把一批修改（取前 / 后状态）放进写队列，在一个事务里写入（表结构见 `store::MIGRATIONS`）。
/// 失败原因由 `Store::take_error` 统一上报。
pub fn write(base: &str, changes: &[Change], after: bool) {
    let Some(store) = store() else { return };
    let (base, changes) = (base.to_string(), changes.to_vec());
    store.write(None, move |c| {
        let base = base.as_str();
        let tx = c.transaction()?;
        for ch in &changes {
            match ch {
                Change::Meta(..) => {} // 由 objects 表保存
                Change::Table(tid, a, b) => match if after { b } else { a } {
                    Some(m) => tx
                        .execute("INSERT OR REPLACE INTO base_tables VALUES (?1, ?2, ?3)", params![base, tid, serde_json::to_string(m).unwrap_or_default()])
                        .map(|_| ())?,
                    None => tx.execute("DELETE FROM base_tables WHERE base_id = ?1 AND table_id = ?2", params![base, tid]).map(|_| ())?,
                },
                Change::Field(tid, fid, a, b) => match if after { b } else { a } {
                    Some(f) => tx
                        .execute(
                            "INSERT OR REPLACE INTO base_fields VALUES (?1, ?2, ?3, ?4)",
                            params![base, tid, fid, serde_json::to_string(f).unwrap_or_default()],
                        )
                        .map(|_| ())?,
                    None => tx.execute("DELETE FROM base_fields WHERE base_id = ?1 AND field_id = ?2", params![base, fid]).map(|_| ())?,
                },
                Change::Record(tid, rid, a, b) => {
                    let (old, new) = if after { (a, b) } else { (b, a) };
                    log_history(&tx, base, rid, old.as_ref(), new.as_ref());
                    match new {
                        Some(r) => tx
                            .prepare_cached("INSERT OR REPLACE INTO base_records VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?
                            .execute(params![base, tid, rid, serde_json::to_string(r).unwrap_or_default(), r.created, r.updated])
                            .map(|_| ())?,
                        None => tx.prepare_cached("DELETE FROM base_records WHERE base_id = ?1 AND record_id = ?2")?.execute(params![base, rid]).map(|_| ())?,
                    }
                }
                Change::View(tid, vid, a, b) => match if after { b } else { a } {
                    Some(v) => tx
                        .execute(
                            "INSERT OR REPLACE INTO base_views VALUES (?1, ?2, ?3, ?4)",
                            params![base, tid, vid, serde_json::to_string(v).unwrap_or_default()],
                        )
                        .map(|_| ())?,
                    None => tx.execute("DELETE FROM base_views WHERE base_id = ?1 AND view_id = ?2", params![base, vid]).map(|_| ())?,
                },
            }
        }
        tx.commit()
    });
}

/// 记录修改历史：逐字段写前 / 后值（新建、删除记为字段 ""）。
fn log_history(tx: &rusqlite::Transaction, base: &str, rid: &str, old: Option<&Record>, new: Option<&Record>) {
    let at = now();
    let put = |fid: &str, a: Option<&serde_json::Value>, b: Option<&serde_json::Value>| {
        if let Ok(mut st) = tx.prepare_cached("INSERT INTO base_history VALUES (?1, ?2, ?3, ?4, ?5, ?6)") {
            st.execute(params![base, rid, fid, a.map(|v| v.to_string()), b.map(|v| v.to_string()), at]).ok();
        }
    };
    match (old, new) {
        (None, Some(_)) => put("", None, Some(&serde_json::Value::from("created"))),
        (Some(_), None) => put("", Some(&serde_json::Value::from("deleted")), None),
        (Some(a), Some(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.cells.keys().chain(b.cells.keys()).collect();
            for k in keys {
                if a.cells.get(k) != b.cells.get(k) {
                    put(k, a.cells.get(k), b.cells.get(k));
                }
            }
        }
        _ => {}
    }
}

/// 一条记录的修改历史（新的在前）：(字段 id, 前值, 后值, 时间)。
pub fn history(base: &str, rid: &str) -> Vec<(String, Option<serde_json::Value>, Option<serde_json::Value>, i64)> {
    let Some(store) = store() else { return vec![] };
    let cutoff = now() - HISTORY_DAYS * 86_400_000;
    store.write_sync(move |c| c.execute("DELETE FROM base_history WHERE at < ?1", [cutoff]).map(|_| ()));
    store.with_conn(|c| {
        let Ok(mut st) =
            c.prepare("SELECT field_id, before, after, at FROM base_history WHERE base_id = ?1 AND record_id = ?2 ORDER BY at DESC, rowid DESC LIMIT 200")
        else {
            return vec![];
        };
        st.query_map(params![base, rid], |r| {
            let p = |s: Option<String>| s.and_then(|s| serde_json::from_str(&s).ok());
            Ok((r.get::<_, String>(0)?, p(r.get(1)?), p(r.get(2)?), r.get::<_, i64>(3)?))
        })
        .map(|it| it.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
    })
}

/// 清空一个多维表格的修改历史。
pub fn clear_history(base: &str) {
    if let Some(store) = store() {
        let base = base.to_string();
        store.write(None, move |c| c.execute("DELETE FROM base_history WHERE base_id = ?1", [base]).map(|_| ()));
    }
}

fn rows<T: serde::de::DeserializeOwned>(c: &rusqlite::Connection, sql: &str, base: &str, tid: &str) -> Vec<T> {
    let Ok(mut st) = c.prepare(sql) else { return vec![] };
    st.query_map(params![base, tid], |r| r.get::<_, String>(0))
        .map(|it| it.filter_map(|s| s.ok()).filter_map(|s| serde_json::from_str(&s).ok()).collect())
        .unwrap_or_default()
}

/// 从数据库装载整个多维表格。
pub fn load(id: &str, meta: BaseMeta) -> Base {
    let t0 = std::time::Instant::now();
    let b = load_inner(id, meta);
    super::stress::record(&super::stress::LOAD_US, t0);
    b
}

fn load_inner(id: &str, meta: BaseMeta) -> Base {
    let Some(store) = store() else { return Base::from_parts(id, meta, vec![]) };
    store.flush();
    let tables = store.with_conn(|c| {
        let mut out = vec![];
        for tid in &meta.tables {
            let tm: Option<TableMeta> = c
                .query_row("SELECT data FROM base_tables WHERE base_id = ?1 AND table_id = ?2", params![id, tid], |r| r.get::<_, String>(0))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            let Some(tm) = tm else { continue };
            let fields: Vec<Field> = rows(c, "SELECT data FROM base_fields WHERE base_id = ?1 AND table_id = ?2", id, tid);
            let records: Vec<Record> = rows(c, "SELECT data FROM base_records WHERE base_id = ?1 AND table_id = ?2", id, tid);
            let views: Vec<View> = rows(c, "SELECT data FROM base_views WHERE base_id = ?1 AND table_id = ?2", id, tid);
            out.push(Table::assemble(tm, fields, records, views));
        }
        out
    });
    let mut meta = meta;
    meta.tables.retain(|t| tables.iter().any(|x| &x.meta.id == t));
    Base::from_parts(id, meta, tables)
}

/// 删除一个多维表格的全部行（资源回收用）。
pub fn delete(store: &Store, id: &str) {
    let id = id.to_string();
    store.write(None, move |c| {
        let tx = c.transaction()?;
        for t in ["base_tables", "base_fields", "base_records", "base_views", "base_history"] {
            tx.execute(&format!("DELETE FROM {t} WHERE base_id = ?1"), [&id])?;
        }
        tx.commit()
    });
}
