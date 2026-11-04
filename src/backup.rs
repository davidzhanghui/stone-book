//! 完整备份 / 恢复：把数据库与 assets 目录打成一个 zip，换机器或出问题时可整体还原。
//!
//! 包内容：`manifest.json`（格式版本、库版本、导出时的 assets 绝对路径）、`stonebrook.db`（`VACUUM INTO` 的一致快照）、`assets/**`。
//! 恢复分两步，避免在运行中替换正在使用的数据库：
//! 1. `stage_restore`：校验并解压到数据目录下的 `restore-pending/`；
//! 2. 下次启动时 `apply_pending`（先于 `Store::open`）：当前的库与 assets 移到 `recovery/pre-restore-<时间>/`（不删除），
//!    换入备份内容，并把文档里写死的旧 assets 绝对路径改成当前路径。

use crate::store::{data_dir, DB_FILE};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const FORMAT: u32 = 1;
const PENDING: &str = "restore-pending";

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: u32,
    /// 导出时数据库的 schema 版本（`PRAGMA user_version`）
    schema: usize,
    /// 导出时 assets 目录的绝对路径，恢复时用来改写文档里的图片路径
    assets_dir: String,
    created_ms: i64,
    app_version: String,
}

pub struct ExportStats {
    pub files: usize,
    pub bytes: u64,
}

fn zip_err(e: zip::result::ZipError) -> anyhow::Error {
    anyhow!("{e}")
}

/// 已压缩过的格式直接存储，省 CPU。
fn stored(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "mp3" | "mp4" | "m4a" | "mov" | "zip" | "pdf" | "aac" | "ogg")
}

fn add_dir(zw: &mut ZipWriter<File>, root: &Path, dir: &Path, stats: &mut ExportStats) -> Result<()> {
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            add_dir(zw, root, &path, stats)?;
        } else if ft.is_file() {
            let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            let method = if stored(&path) { CompressionMethod::Stored } else { CompressionMethod::Deflated };
            zw.start_file(format!("assets/{rel}"), SimpleFileOptions::default().compression_method(method).large_file(true)).map_err(zip_err)?;
            stats.bytes += io::copy(&mut File::open(&path)?, zw)?;
            stats.files += 1;
        }
    }
    Ok(())
}

/// 导出完整备份到 `dest`。调用前须已把内存里的修改落盘（`objects::flush_dirty(true)`）。可在后台线程执行。
pub fn export(store: &crate::store::Store, dest: &Path) -> Result<ExportStats> {
    store.flush();
    let tmp_db = data_dir().join("backup-snapshot.db");
    let _ = fs::remove_file(&tmp_db);
    let snap = tmp_db.to_string_lossy().to_string();
    store.with_conn(|c| c.execute("VACUUM INTO ?1", [&snap])).context("生成数据库快照失败")?;

    let result = (|| -> Result<ExportStats> {
        let assets = store.assets_dir();
        let mut zw = ZipWriter::new(File::create(dest)?);
        let mut stats = ExportStats { files: 0, bytes: 0 };
        let manifest = Manifest {
            format: FORMAT,
            schema: crate::store::schema_version(),
            assets_dir: assets.to_string_lossy().to_string(),
            created_ms: crate::store::now_ms(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        zw.start_file("manifest.json", SimpleFileOptions::default()).map_err(zip_err)?;
        zw.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        zw.start_file(DB_FILE, SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).large_file(true)).map_err(zip_err)?;
        stats.bytes += io::copy(&mut File::open(&tmp_db)?, &mut zw)?;
        stats.files += 1;
        if assets.is_dir() {
            add_dir(&mut zw, &assets, &assets, &mut stats)?;
        }
        zw.finish().map_err(zip_err)?;
        Ok(stats)
    })();
    let _ = fs::remove_file(&tmp_db);
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    result
}

/// 校验备份并解压到待恢复目录；下次启动生效。可在后台线程执行。
pub fn stage_restore(zip_path: &Path) -> Result<()> {
    let mut ar = ZipArchive::new(File::open(zip_path)?).map_err(|_| anyhow!("不是有效的石渠备份文件"))?;
    let manifest: Manifest = {
        let f = ar.by_name("manifest.json").map_err(|_| anyhow!("不是石渠备份文件（缺少 manifest.json）"))?;
        serde_json::from_reader(f).map_err(|_| anyhow!("备份信息损坏"))?
    };
    if manifest.format != FORMAT {
        bail!("备份格式版本 {} 不受支持", manifest.format);
    }
    if manifest.schema > crate::store::schema_version() {
        bail!("备份来自更新版本的石渠（库版本 {} > {}），请先升级应用", manifest.schema, crate::store::schema_version());
    }
    ar.by_name(DB_FILE).map_err(|_| anyhow!("备份里没有数据库"))?;

    let pending = data_dir().join(PENDING);
    let _ = fs::remove_dir_all(&pending);
    fs::create_dir_all(&pending)?;
    let res = (|| -> Result<()> {
        for i in 0..ar.len() {
            let mut f = ar.by_index(i).map_err(zip_err)?;
            // enclosed_name 拒绝 `..` 与绝对路径，防止解压逃出目录
            let Some(rel) = f.enclosed_name() else { bail!("备份含非法路径：{}", f.name()) };
            let out = pending.join(rel);
            if f.is_dir() {
                fs::create_dir_all(&out)?;
                continue;
            }
            if let Some(p) = out.parent() {
                fs::create_dir_all(p)?;
            }
            io::copy(&mut f, &mut File::create(&out)?)?;
        }
        // 解压出来的库必须能打开且完整
        let c = rusqlite::Connection::open_with_flags(pending.join(DB_FILE), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok != "ok" {
            bail!("备份中的数据库已损坏：{ok}");
        }
        Ok(())
    })();
    if res.is_err() {
        let _ = fs::remove_dir_all(&pending);
    }
    res
}

/// 启动时（打开数据库之前）调用：若有待恢复的备份，就换入。返回给用户看的说明。
pub fn apply_pending() -> Option<String> {
    let dir = data_dir();
    let pending = dir.join(PENDING);
    let manifest: Manifest = serde_json::from_slice(&fs::read(pending.join("manifest.json")).ok()?).ok()?;
    match swap_in(&dir, &pending, &manifest) {
        Ok(old) => Some(format!("已从备份恢复，原数据保留在 {}", old.display())),
        Err(e) => {
            eprintln!("恢复备份失败：{e:#}");
            None
        }
    }
}

fn swap_in(dir: &Path, pending: &Path, m: &Manifest) -> Result<PathBuf> {
    let old = dir.join("recovery").join(format!("pre-restore-{}", crate::store::now_ms()));
    fs::create_dir_all(&old)?;
    let names: Vec<String> = vec![DB_FILE.into(), format!("{DB_FILE}-wal"), format!("{DB_FILE}-shm"), "assets".into()];
    for name in &names {
        let p = dir.join(name);
        if p.exists() {
            fs::rename(&p, old.join(name))?;
        }
    }
    let moved = (|| -> Result<()> {
        fs::rename(pending.join(DB_FILE), dir.join(DB_FILE))?;
        if pending.join("assets").is_dir() {
            fs::rename(pending.join("assets"), dir.join("assets"))?;
        } else {
            fs::create_dir_all(dir.join("assets"))?;
        }
        Ok(())
    })();
    if let Err(e) = moved {
        // 换入失败：把旧数据放回去
        let _ = fs::remove_file(dir.join(DB_FILE));
        for name in &names {
            if old.join(name).exists() {
                let _ = fs::remove_dir_all(dir.join(name));
                let _ = fs::rename(old.join(name), dir.join(name));
            }
        }
        return Err(e);
    }
    let _ = fs::remove_dir_all(pending);
    let new_assets = dir.join("assets").to_string_lossy().to_string();
    if m.assets_dir != new_assets {
        if let Err(e) = rewrite_paths(&dir.join(DB_FILE), &m.assets_dir, &new_assets) {
            eprintln!("改写图片路径失败：{e:#}");
        }
    }
    Ok(old)
}

/// 把数据里写死的旧 assets 路径前缀换成新的。
/// 只处理文本形式的数据（文档、模板、同步块、多维表格、画板 / 流程图 / 导图）；
/// 电子表格是长度前缀的二进制，不能改长度，其中的单元格图片需要路径一致才能显示。
fn rewrite_paths(db: &Path, old: &str, new: &str) -> Result<()> {
    let mut c = rusqlite::Connection::open(db)?;
    let tx = c.transaction()?;
    let p = rusqlite::params![old, new];
    for sql in [
        "UPDATE docs SET content = replace(content, ?1, ?2) WHERE instr(content, ?1) > 0",
        "UPDATE templates SET content = replace(content, ?1, ?2) WHERE instr(content, ?1) > 0",
        "UPDATE sync_blocks SET blocks = replace(blocks, ?1, ?2) WHERE instr(blocks, ?1) > 0",
        "UPDATE base_tables SET data = replace(data, ?1, ?2) WHERE instr(data, ?1) > 0",
        "UPDATE base_fields SET data = replace(data, ?1, ?2) WHERE instr(data, ?1) > 0",
        "UPDATE base_records SET data = replace(data, ?1, ?2) WHERE instr(data, ?1) > 0",
        "UPDATE base_views SET data = replace(data, ?1, ?2) WHERE instr(data, ?1) > 0",
        "UPDATE link_cards SET image = replace(image, ?1, ?2), favicon = replace(favicon, ?1, ?2) WHERE instr(image, ?1) > 0 OR instr(favicon, ?1) > 0",
        "UPDATE objects SET data = CAST(replace(CAST(data AS TEXT), ?1, ?2) AS BLOB)
         WHERE kind IN ('board', 'flow', 'mindmap', 'base') AND instr(CAST(data AS TEXT), ?1) > 0",
    ] {
        tx.execute(sql, p)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_replaces_prefix_in_docs_and_json_objects() {
        let dir = std::env::temp_dir().join(format!("stonebrook-bk-{}", crate::store::new_uuid()));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.db");
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch(
            "CREATE TABLE docs (content TEXT); CREATE TABLE templates (content TEXT); CREATE TABLE sync_blocks (blocks TEXT);
             CREATE TABLE base_tables (data TEXT); CREATE TABLE base_fields (data TEXT); CREATE TABLE base_records (data TEXT); CREATE TABLE base_views (data TEXT);
             CREATE TABLE link_cards (image TEXT, favicon TEXT); CREATE TABLE objects (kind TEXT, data BLOB);
             INSERT INTO docs VALUES ('{\"p\":\"/old/assets/a.png\"}');
             INSERT INTO objects VALUES ('board', CAST('{\"p\":\"/old/assets/b.png\"}' AS BLOB)), ('sheet', X'00FF');",
        )
        .unwrap();
        drop(c);
        rewrite_paths(&db, "/old/assets", "/new/assets").unwrap();
        let c = rusqlite::Connection::open(&db).unwrap();
        let d: String = c.query_row("SELECT content FROM docs", [], |r| r.get(0)).unwrap();
        assert!(d.contains("/new/assets/a.png"));
        let b: Vec<u8> = c.query_row("SELECT data FROM objects WHERE kind='board'", [], |r| r.get(0)).unwrap();
        assert!(String::from_utf8(b).unwrap().contains("/new/assets/b.png"));
        let s: Vec<u8> = c.query_row("SELECT data FROM objects WHERE kind='sheet'", [], |r| r.get(0)).unwrap();
        assert_eq!(s, vec![0, 0xFF]);
        fs::remove_dir_all(dir).ok();
    }

    /// 导出 → 暂存 → 换入，且 assets 路径变化时文档里的旧路径被改写；旧数据保留在 recovery/。
    #[test]
    fn export_stage_apply_roundtrip() {
        let _env = crate::store::TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("stonebrook-rt-{}", crate::store::new_uuid()));
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(&dst).unwrap();
        std::env::set_var(crate::store::DATA_DIR_ENV, &src);
        let store = crate::store::Store::open().unwrap();
        let id = store.create(None, &crate::model::DocContent::from_blocks(vec![]));
        let old_assets = store.assets_dir().to_string_lossy().to_string();
        store.write_sync({
            let (id, p) = (id.clone(), format!("{old_assets}/pic.png"));
            move |c| c.execute("UPDATE docs SET content = ?2 WHERE id = ?1", rusqlite::params![id, format!("{{\"img\":\"{p}\"}}")]).map(|_| ())
        });
        fs::write(store.assets_dir().join("pic.png"), b"png-bytes").unwrap();
        let zip = root.join("b.zip");
        let st = export(&store, &zip).unwrap();
        assert_eq!(st.files, 2);

        // 在另一个数据目录里先放一份「旧数据」，再恢复
        std::env::set_var(crate::store::DATA_DIR_ENV, &dst);
        fs::create_dir_all(dst.join("assets")).unwrap();
        fs::write(dst.join(DB_FILE), b"old").unwrap();
        stage_restore(&zip).unwrap();
        assert!(apply_pending().is_some());
        assert_eq!(fs::read(dst.join("assets/pic.png")).unwrap(), b"png-bytes");
        let c = rusqlite::Connection::open(dst.join(DB_FILE)).unwrap();
        let content: String = c.query_row("SELECT content FROM docs WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert!(content.contains(&format!("{}/pic.png", dst.join("assets").display())), "{content}");
        let pre: Vec<_> = fs::read_dir(dst.join("recovery")).unwrap().flatten().collect();
        assert_eq!(pre.len(), 1);
        assert_eq!(fs::read(pre[0].path().join(DB_FILE)).unwrap(), b"old");
        assert!(!dst.join(PENDING).exists());
        fs::remove_dir_all(root).ok();
    }
}
