//! 本地存储：SQLite（~/Library/Application Support/Stonebrook/stonebrook.db）。
//! 文档内容以 JSON 存一列，另存一份纯文本用于全文搜索。
//!
//! 写入模型：所有写操作都进同一个后台写线程的有序队列（单写者，顺序与提交顺序一致）。
//! 带 key 的任务（文档 / 嵌入对象保存）在队列里按 key 合并，只写最新快照；
//! 界面线程用另一条只读连接读数据（WAL 下读写互不阻塞）。
//! 需要「写完立刻读到」的地方用 `write_sync`，退出前用 `flush` 等队列写完。

use crate::model::DocContent;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Params, Row};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// 用户可见的品牌名。
pub const APP_NAME: &str = "石渠";
pub const APP_NAME_EN: &str = "Stonebrook";
/// 数据目录名与数据库文件名。
pub const DATA_DIR_NAME: &str = "Stonebrook";
pub const DB_FILE: &str = "stonebrook.db";
pub const DATA_DIR_ENV: &str = "STONEBROOK_DATA_DIR";

#[derive(Clone, Debug)]
pub struct DocMeta {
    pub id: String,
    pub parent: Option<String>,
    pub title: String,
    pub updated: i64,
    pub deleted: bool,
}

/// 一条评论：挂在某篇文档的某个块上。
#[derive(Clone, Debug)]
pub struct Comment {
    pub id: String,
    /// 块在文档中的序号（保存时按 `block_id` 回写，块被插入 / 删除 / 移动后依然跟着原块）
    pub block: usize,
    /// 运行期的块 id（`Block::id` 不落盘，加载时按序号映射，编辑过程中靠它跟踪块）
    pub block_id: u64,
    /// 选中文字的引用快照，便于定位
    pub anchor: String,
    pub body: String,
    pub author: String,
    pub created: i64,
    pub resolved: bool,
}

/// 网页卡片缓存的一行。
#[derive(Clone, Debug, Default)]
pub struct LinkCardRow {
    pub title: String,
    pub descr: String,
    pub image: String,
    pub favicon: String,
    pub site: String,
    pub ok: bool,
}

/// 文档 JSON 解析失败：原始数据已备份到 `backup`。
#[derive(Clone, Debug)]
pub struct Corrupt {
    pub backup: PathBuf,
}

type Job = Box<dyn FnOnce(&mut Connection) -> rusqlite::Result<()> + Send>;

#[derive(Default)]
struct Queue {
    jobs: VecDeque<(Option<String>, Job)>,
    busy: bool,
}

/// 后台写线程。
struct Writer {
    q: Mutex<Queue>,
    cv: Condvar,
}

#[derive(Clone)]
pub struct Store {
    /// 界面线程的只读连接
    conn: Arc<Mutex<Connection>>,
    writer: Arc<Writer>,
    /// 最近一次写库失败的原因（界面层取走后提示一次）
    error: Arc<Mutex<Option<String>>>,
    pub data_dir: PathBuf,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// 数据目录：默认 ~/Library/Application Support/Stonebrook，可用 STONEBROOK_DATA_DIR 覆盖。
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return PathBuf::from(dir);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(DATA_DIR_NAME)
}

static GLOBAL: OnceLock<Store> = OnceLock::new();

/// 进程级的存储实例（启动时 `install` 一次；单元测试里可能没有）。
pub fn global() -> Option<&'static Store> {
    GLOBAL.get()
}

/// 安装进程级存储；已安装过则忽略。
pub fn install(store: Store) {
    let _ = GLOBAL.set(store);
}

/// 有序的建表 / 升级脚本：第 i 条把库从版本 i 升到 i+1（版本号存在 `PRAGMA user_version`）。
/// 只能在末尾追加，不能修改已发布的条目。
const MIGRATIONS: &[&str] = &[
    // 1：基线（老库里这些表可能已存在，所以全部 IF NOT EXISTS）
    "CREATE TABLE IF NOT EXISTS docs (
        id TEXT PRIMARY KEY,
        parent TEXT,
        title TEXT NOT NULL DEFAULT '',
        sort REAL NOT NULL DEFAULT 0,
        created INTEGER NOT NULL,
        updated INTEGER NOT NULL,
        deleted INTEGER NOT NULL DEFAULT 0,
        content TEXT NOT NULL,
        plain TEXT NOT NULL DEFAULT ''
     );
     CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS objects (id TEXT PRIMARY KEY, kind TEXT NOT NULL, data BLOB NOT NULL, updated INTEGER NOT NULL);
     CREATE TABLE IF NOT EXISTS sync_blocks (id TEXT PRIMARY KEY, blocks TEXT NOT NULL, source TEXT, updated INTEGER NOT NULL);
     CREATE TABLE IF NOT EXISTS link_cards (url TEXT PRIMARY KEY, title TEXT, descr TEXT, image TEXT, favicon TEXT, site TEXT, fetched_at INTEGER NOT NULL, ok INTEGER NOT NULL);
     CREATE TABLE IF NOT EXISTS templates (id TEXT PRIMARY KEY, name TEXT NOT NULL, content TEXT NOT NULL, created INTEGER NOT NULL);
     CREATE TABLE IF NOT EXISTS comments (
        id TEXT PRIMARY KEY,
        doc TEXT NOT NULL,
        block INTEGER NOT NULL,
        anchor TEXT NOT NULL DEFAULT '',
        body TEXT NOT NULL,
        author TEXT NOT NULL DEFAULT '',
        created INTEGER NOT NULL,
        resolved INTEGER NOT NULL DEFAULT 0
     );
     CREATE INDEX IF NOT EXISTS comments_doc ON comments(doc, resolved);
     CREATE TABLE IF NOT EXISTS base_tables  (base_id TEXT NOT NULL, table_id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(base_id, table_id));
     CREATE TABLE IF NOT EXISTS base_fields  (base_id TEXT NOT NULL, table_id TEXT NOT NULL, field_id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(base_id, field_id));
     CREATE TABLE IF NOT EXISTS base_records (base_id TEXT NOT NULL, table_id TEXT NOT NULL, record_id TEXT NOT NULL, data TEXT NOT NULL,
                                              created INTEGER NOT NULL DEFAULT 0, updated INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(base_id, record_id));
     CREATE TABLE IF NOT EXISTS base_views   (base_id TEXT NOT NULL, table_id TEXT NOT NULL, view_id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(base_id, view_id));
     CREATE INDEX IF NOT EXISTS base_records_table ON base_records(base_id, table_id);
     CREATE TABLE IF NOT EXISTS base_history (base_id TEXT NOT NULL, record_id TEXT NOT NULL, field_id TEXT NOT NULL,
                                              before TEXT, after TEXT, at INTEGER NOT NULL);
     CREATE INDEX IF NOT EXISTS base_history_rec ON base_history(base_id, record_id, at);",
    // 2：全文索引（trigram 分词，中英文都能做子串匹配；少于 3 个字的查询回退到扫描）
    "CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(id UNINDEXED, title, plain, tokenize = 'trigram');
     INSERT INTO docs_fts (id, title, plain) SELECT id, title, plain FROM docs;",
];

/// 当前代码支持的库 schema 版本（备份恢复时用来拒绝来自更新版本的备份）。
pub fn schema_version() -> usize {
    MIGRATIONS.len()
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let ver: usize = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(ver) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

fn open_conn(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}

impl Writer {
    fn run(&self, mut conn: Connection, error: Arc<Mutex<Option<String>>>) {
        loop {
            let job = {
                let mut q = self.q.lock().unwrap_or_else(|e| e.into_inner());
                while q.jobs.is_empty() {
                    q = self.cv.wait(q).unwrap_or_else(|e| e.into_inner());
                }
                q.busy = true;
                q.jobs.pop_front().map(|(_, j)| j)
            };
            if let Some(job) = job {
                if let Err(e) = job(&mut conn) {
                    eprintln!("写库失败：{e}");
                    *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
                }
            }
            self.q.lock().unwrap_or_else(|e| e.into_inner()).busy = false;
            self.cv.notify_all();
        }
    }
}

impl Store {
    pub fn open() -> Result<Self> {
        let data_dir = data_dir();
        std::fs::create_dir_all(data_dir.join("assets"))?;
        let path = data_dir.join(DB_FILE);
        let mut wconn = open_conn(&path)?;
        migrate(&mut wconn)?;
        let conn = open_conn(&path)?;
        let writer = Arc::new(Writer { q: Mutex::new(Queue::default()), cv: Condvar::new() });
        let error = Arc::new(Mutex::new(None));
        {
            let (w, e) = (writer.clone(), error.clone());
            std::thread::Builder::new().name("stonebrook-writer".into()).spawn(move || w.run(wconn, e))?;
        }
        Ok(Store { conn: Arc::new(Mutex::new(conn)), writer, error, data_dir })
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }

    // ───────────── 基础设施 ─────────────

    /// 只读连接（界面线程查询用）。写操作请走 `write` / `write_sync`。
    pub fn with_conn<R>(&self, f: impl FnOnce(&mut Connection) -> R) -> R {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut conn)
    }

    /// 提交一个写任务（后台按顺序执行）。带 `key` 的任务若队列里已有同 key 的未执行任务，就地替换（只写最新的）。
    pub fn write(&self, key: Option<String>, job: impl FnOnce(&mut Connection) -> rusqlite::Result<()> + Send + 'static) {
        let mut q = self.writer.q.lock().unwrap_or_else(|e| e.into_inner());
        let job: Job = Box::new(job);
        if let Some(k) = &key {
            if let Some(slot) = q.jobs.iter_mut().find(|(x, _)| x.as_ref() == Some(k)) {
                slot.1 = job;
                return;
            }
        }
        q.jobs.push_back((key, job));
        drop(q);
        self.writer.cv.notify_all();
    }

    /// 提交写任务并等它（以及之前排队的任务）写完。
    pub fn write_sync(&self, job: impl FnOnce(&mut Connection) -> rusqlite::Result<()> + Send + 'static) {
        self.write(None, job);
        self.flush();
    }

    /// 等待写队列清空（退出前、需要读到刚写入的数据前调用）。
    pub fn flush(&self) {
        let mut q = self.writer.q.lock().unwrap_or_else(|e| e.into_inner());
        while !q.jobs.is_empty() || q.busy {
            q = self.writer.cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// 取走最近一次写库失败的原因。
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    /// 查询多行（出错时返回空并打印原因）。
    fn rows<T>(&self, sql: &str, p: impl Params, f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>) -> Vec<T> {
        self.with_conn(|c| {
            let res = c.prepare_cached(sql).and_then(|mut st| st.query_map(p, f).map(|it| it.filter_map(|r| r.ok()).collect()));
            res.unwrap_or_else(|e| {
                eprintln!("查询失败：{e}");
                vec![]
            })
        })
    }

    /// 查询一行。
    fn row<T>(&self, sql: &str, p: impl Params, f: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>) -> Option<T> {
        self.with_conn(|c| {
            c.query_row(sql, p, f).optional().unwrap_or_else(|e| {
                eprintln!("查询失败：{e}");
                None
            })
        })
    }

    // ───────────── 文档 ─────────────

    pub fn list(&self) -> Vec<DocMeta> {
        self.rows("SELECT id, parent, title, updated, deleted FROM docs ORDER BY sort, created", [], |r| {
            Ok(DocMeta { id: r.get(0)?, parent: r.get(1)?, title: r.get(2)?, updated: r.get(3)?, deleted: r.get::<_, i64>(4)? != 0 })
        })
    }

    /// 读取文档。JSON 损坏时把原文备份到 `recovery/` 目录，返回一篇说明情况的文档和 `Corrupt`。
    pub fn load(&self, id: &str) -> Option<(DocContent, Option<Corrupt>)> {
        let raw: String = self.row("SELECT content FROM docs WHERE id = ?1", [id], |r| r.get(0))?;
        match DocContent::try_from_json(&raw) {
            Ok(c) => Some((c, None)),
            Err(e) => {
                let dir = self.data_dir.join("recovery");
                let backup = dir.join(format!("{id}-{}.json", now_ms()));
                let saved = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&backup, &raw));
                if let Err(io) = saved {
                    eprintln!("备份损坏文档失败：{io}");
                    return None; // 备份不了就不打开，避免后续保存覆盖原数据
                }
                eprintln!("文档 {id} 解析失败（{e}），原文已备份到 {}", backup.display());
                Some((DocContent::corrupt_notice(&backup.to_string_lossy()), Some(Corrupt { backup })))
            }
        }
    }

    pub fn create(&self, parent: Option<&str>, content: &DocContent) -> String {
        let id = new_uuid();
        let now = now_ms();
        let (pid, title, json, plain) = (parent.map(str::to_string), content.title(), content.to_json(), content.plain_text());
        let doc = id.clone();
        self.write_sync(move |c| {
            let tx = c.transaction()?;
            let sort: f64 = tx.query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM docs", [], |r| r.get(0))?;
            tx.execute(
                "INSERT INTO docs (id, parent, title, sort, created, updated, content, plain) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7)",
                params![doc, pid, title, sort, now, json, plain],
            )?;
            tx.execute("INSERT INTO docs_fts (id, title, plain) VALUES (?1, ?2, ?3)", params![doc, title, plain])?;
            tx.commit()
        });
        id
    }

    /// 保存文档：放进写队列，同一文档未写的旧快照会被替换。序列化在写线程里做。
    pub fn save(&self, id: &str, content: &DocContent) {
        let (doc, content) = (id.to_string(), content.clone());
        self.write(Some(format!("doc:{id}")), move |c| {
            let (title, json, plain) = (content.title(), content.to_json(), content.plain_text());
            let tx = c.transaction()?;
            tx.execute("UPDATE docs SET title = ?2, content = ?3, plain = ?4, updated = ?5 WHERE id = ?1", params![doc, title, json, plain, now_ms()])?;
            tx.execute("DELETE FROM docs_fts WHERE id = ?1", [&doc])?;
            tx.execute("INSERT INTO docs_fts (id, title, plain) VALUES (?1, ?2, ?3)", params![doc, title, plain])?;
            tx.commit()
        });
    }

    pub fn set_deleted(&self, id: &str, deleted: bool) {
        let id = id.to_string();
        // 连同子文档一起移入 / 移出回收站
        self.write_sync(move |c| {
            c.execute(
                "WITH RECURSIVE sub(id) AS (SELECT ?1 UNION ALL SELECT d.id FROM docs d JOIN sub ON d.parent = sub.id)
                 UPDATE docs SET deleted = ?2 WHERE id IN sub",
                params![id, deleted as i64],
            )
            .map(|_| ())
        });
    }

    /// 永久删除文档（连同子文档与它们的评论）。
    pub fn purge(&self, id: &str) {
        let id = id.to_string();
        self.write_sync(move |c| {
            const SUB: &str = "WITH RECURSIVE sub(id) AS (SELECT ?1 UNION ALL SELECT d.id FROM docs d JOIN sub ON d.parent = sub.id)";
            let tx = c.transaction()?;
            // 先删评论和索引（依赖 docs 里的父子关系），最后删文档本身
            tx.execute(&format!("{SUB} DELETE FROM comments WHERE doc IN sub"), [&id])?;
            tx.execute(&format!("{SUB} DELETE FROM docs_fts WHERE id IN sub"), [&id])?;
            tx.execute(&format!("{SUB} DELETE FROM docs WHERE id IN sub"), [&id])?;
            tx.commit()
        });
    }

    /// 全文搜索：标题 / 正文子串匹配，返回 (id, title, 片段)。
    /// 3 个字及以上走 FTS5 trigram 索引；更短的查询（如两个汉字）回退到逐篇扫描。
    pub fn search(&self, q: &str, limit: usize) -> Vec<(String, String, String)> {
        let q = q.trim();
        if q.is_empty() {
            return vec![];
        }
        let map = |r: &Row<'_>| {
            let plain: String = r.get(2)?;
            Ok((r.get(0)?, r.get(1)?, snippet(&plain, q)))
        };
        if q.chars().count() >= 3 {
            // 整体作为一个短语，转义双引号
            let phrase = format!("\"{}\"", q.replace('"', "\"\""));
            return self.rows(
                "SELECT d.id, d.title, d.plain FROM docs_fts f JOIN docs d ON d.id = f.id
                 WHERE docs_fts MATCH ?1 AND d.deleted = 0
                 ORDER BY (instr(lower(d.title), lower(?2)) > 0) DESC, d.updated DESC LIMIT ?3",
                params![phrase, q, limit as i64],
                map,
            );
        }
        self.rows(
            "SELECT id, title, plain, instr(lower(title), lower(?1)) AS t, instr(lower(plain), lower(?1)) AS p
             FROM docs WHERE deleted = 0 AND (t > 0 OR p > 0)
             ORDER BY (t > 0) DESC, updated DESC LIMIT ?2",
            params![q, limit as i64],
            map,
        )
    }

    /// 可能引用嵌入对象的全部 JSON：文档（含回收站）、用户模板、同步块（资源回收时扫描引用用）。
    pub fn reference_texts(&self) -> Vec<String> {
        self.flush();
        self.rows("SELECT content FROM docs UNION ALL SELECT content FROM templates UNION ALL SELECT blocks FROM sync_blocks", [], |r| r.get(0))
    }

    // ───────────── 嵌入对象 ─────────────

    /// 保存对象：同一对象未写的旧版本会被替换。`encode` 在写线程里执行。
    pub fn save_object_with(&self, id: &str, kind: &str, encode: impl FnOnce() -> Vec<u8> + Send + 'static) {
        let (oid, kind) = (id.to_string(), kind.to_string());
        self.write(Some(format!("obj:{id}")), move |c| {
            let data = encode();
            c.execute("INSERT OR REPLACE INTO objects (id, kind, data, updated) VALUES (?1, ?2, ?3, ?4)", params![oid, kind, data, now_ms()]).map(|_| ())
        });
    }

    pub fn save_object(&self, id: &str, kind: &str, data: Vec<u8>) {
        self.save_object_with(id, kind, move || data);
    }

    pub fn load_object(&self, id: &str) -> Option<(String, Vec<u8>)> {
        self.flush();
        self.row("SELECT kind, data FROM objects WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
    }

    /// 对象的 (id, kind, 更新时间)。
    pub fn object_ids(&self) -> Vec<(String, String, i64)> {
        self.rows("SELECT id, kind, updated FROM objects", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    }

    /// 对象的原始字节（不经过缓存，元数据浏览用）。
    pub fn objects_of_kind(&self, kind: &str) -> Vec<(String, Vec<u8>)> {
        self.flush();
        self.rows("SELECT id, data FROM objects WHERE kind = ?1 ORDER BY updated DESC", [kind], |r| Ok((r.get(0)?, r.get(1)?)))
    }

    pub fn delete_object(&self, id: &str) {
        let (oid, key) = (id.to_string(), format!("obj:{id}"));
        // 先撤掉还没写的保存，否则删完又被写回来
        self.writer.q.lock().unwrap_or_else(|e| e.into_inner()).jobs.retain(|(k, _)| k.as_ref() != Some(&key));
        self.write(None, move |c| c.execute("DELETE FROM objects WHERE id = ?1", [oid]).map(|_| ()));
    }

    // ───────────── 同步块 ─────────────

    pub fn save_sync(&self, id: &str, blocks_json: &str, source: &str) {
        let (sid, json, src) = (id.to_string(), blocks_json.to_string(), source.to_string());
        self.write(Some(format!("sync:{id}")), move |c| {
            c.execute("INSERT OR REPLACE INTO sync_blocks (id, blocks, source, updated) VALUES (?1, ?2, ?3, ?4)", params![sid, json, src, now_ms()]).map(|_| ())
        });
    }

    pub fn load_sync(&self, id: &str) -> Option<(String, String)> {
        self.flush();
        self.row("SELECT blocks, source FROM sync_blocks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))
    }

    /// 同步块列表：(id, 来源文档 id, 预览文字, 更新时间)
    pub fn list_sync(&self) -> Vec<(String, String, String, i64)> {
        self.flush();
        self.rows("SELECT id, source, blocks, updated FROM sync_blocks ORDER BY updated DESC", [], |r| {
            let json: String = r.get(2)?;
            let preview = crate::model::blocks_from_json(&json).map(|b| b.iter().map(|x| x.plain_text()).collect::<Vec<_>>().join(" ")).unwrap_or_default();
            Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default(), preview.chars().take(60).collect(), r.get(3)?))
        })
    }

    // ───────────── 网页卡片缓存 ─────────────

    pub fn save_link_card(&self, url: &str, card: &LinkCardRow) {
        let (url, c) = (url.to_string(), card.clone());
        self.write(Some(format!("card:{url}")), move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO link_cards (url, title, descr, image, favicon, site, fetched_at, ok) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![url, c.title, c.descr, c.image, c.favicon, c.site, now_ms(), c.ok as i64],
            )
            .map(|_| ())
        });
    }

    pub fn load_link_card(&self, url: &str) -> Option<LinkCardRow> {
        self.row("SELECT title, descr, image, favicon, site, ok FROM link_cards WHERE url = ?1", [url], |r| {
            let s = |i: usize| r.get::<_, Option<String>>(i).map(Option::unwrap_or_default);
            Ok(LinkCardRow { title: s(0)?, descr: s(1)?, image: s(2)?, favicon: s(3)?, site: s(4)?, ok: r.get::<_, i64>(5)? != 0 })
        })
    }

    // ───────────── 模板 ─────────────

    pub fn save_template(&self, name: &str, content_json: &str) -> String {
        let id = new_uuid();
        let (tid, name, json) = (id.clone(), name.to_string(), content_json.to_string());
        self.write_sync(move |c| {
            c.execute("INSERT INTO templates (id, name, content, created) VALUES (?1, ?2, ?3, ?4)", params![tid, name, json, now_ms()]).map(|_| ())
        });
        id
    }

    pub fn list_templates(&self) -> Vec<(String, String, String)> {
        self.rows("SELECT id, name, content FROM templates ORDER BY created DESC", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    }

    pub fn delete_template(&self, id: &str) {
        let id = id.to_string();
        self.write_sync(move |c| c.execute("DELETE FROM templates WHERE id = ?1", [id]).map(|_| ()));
    }

    // ───────────── 评论 ─────────────

    /// 某文档的评论（未解决的在前，按创建时间）。
    pub fn comments(&self, doc: &str) -> Vec<Comment> {
        self.flush();
        self.rows("SELECT id, block, anchor, body, author, created, resolved FROM comments WHERE doc = ?1 ORDER BY resolved, created", [doc], |r| {
            Ok(Comment {
                id: r.get(0)?,
                block: r.get::<_, i64>(1)?.max(0) as usize,
                block_id: 0,
                anchor: r.get(2)?,
                body: r.get(3)?,
                author: r.get(4)?,
                created: r.get(5)?,
                resolved: r.get::<_, i64>(6)? != 0,
            })
        })
    }

    pub fn add_comment(&self, doc: &str, block: usize, anchor: &str, body: &str, author: &str) -> Comment {
        let c = Comment {
            id: new_uuid(),
            block,
            block_id: 0,
            anchor: anchor.to_string(),
            body: body.to_string(),
            author: author.to_string(),
            created: now_ms(),
            resolved: false,
        };
        let (x, doc) = (c.clone(), doc.to_string());
        self.write(None, move |conn| {
            conn.execute(
                "INSERT INTO comments (id, doc, block, anchor, body, author, created, resolved) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                params![x.id, doc, block as i64, x.anchor, x.body, x.author, x.created],
            )
            .map(|_| ())
        });
        c
    }

    pub fn set_comment_resolved(&self, id: &str, resolved: bool) {
        let id = id.to_string();
        self.write(None, move |c| c.execute("UPDATE comments SET resolved = ?2 WHERE id = ?1", params![id, resolved as i64]).map(|_| ()));
    }

    /// 评论所在块的序号变了（块被移动 / 前面插入删除了块）。
    pub fn set_comment_block(&self, id: &str, block: usize) {
        let id = id.to_string();
        self.write(None, move |c| c.execute("UPDATE comments SET block = ?2 WHERE id = ?1", params![id, block as i64]).map(|_| ()));
    }

    pub fn delete_comment(&self, id: &str) {
        let id = id.to_string();
        self.write(None, move |c| c.execute("DELETE FROM comments WHERE id = ?1", [id]).map(|_| ()));
    }

    // ───────────── 键值 ─────────────

    pub fn get_kv(&self, k: &str) -> Option<String> {
        self.row("SELECT v FROM kv WHERE k = ?1", [k], |r| r.get(0))
    }

    pub fn set_kv(&self, k: &str, v: &str) {
        let (k, v) = (k.to_string(), v.to_string());
        self.write(Some(format!("kv:{k}")), move |c| c.execute("INSERT OR REPLACE INTO kv (k, v) VALUES (?1, ?2)", params![k, v]).map(|_| ()));
    }

    /// 在写线程里原子地给整数值加上 `delta`（读改写不会因为排队而丢计数）。
    pub fn add_kv_int(&self, k: &str, delta: i64) {
        let k = k.to_string();
        self.write(None, move |c| {
            c.execute("INSERT INTO kv (k, v) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET v = CAST(CAST(v AS INTEGER) + ?2 AS TEXT)", params![k, delta])
                .map(|_| ())
        });
    }
}

/// 从一堆文本里收集所有形如 32 位十六进制的 id（对象 id 都是无连字符的 UUID）。
pub fn collect_ids(texts: &[String]) -> HashSet<String> {
    let mut out = HashSet::new();
    for t in texts {
        let b = t.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_hexdigit() {
                let s = i;
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if i - s == 32 {
                    out.insert(t[s..i].to_string());
                }
            } else {
                i += 1;
            }
        }
    }
    out
}

/// 取匹配位置附近的一段文字作为搜索结果摘要（按字符逐个比较，避免大小写转换改变字节长度后错位）。
fn snippet(plain: &str, q: &str) -> String {
    let hay: Vec<char> = plain.chars().collect();
    let needle: Vec<char> = q.chars().flat_map(char::to_lowercase).collect();
    let lower: Vec<char> = hay.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let pos = (!needle.is_empty() && needle.len() <= lower.len()).then(|| lower.windows(needle.len()).position(|w| w == needle.as_slice())).flatten();
    let start = pos.map_or(0, |p| p.saturating_sub(20));
    let take = if pos.is_some() { 70 } else { 60 };
    hay[start..].iter().take(take).collect::<String>().replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_ids_finds_only_32_hex_tokens() {
        let a = "a".repeat(32);
        let ids = collect_ids(&[format!(r#"{{"kind":"board","id":"{a}"}} 123 {}"#, "b".repeat(33))]);
        assert!(ids.contains(&a));
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn snippet_centers_on_match() {
        let s = format!("{}关键字{}", "前".repeat(40), "后".repeat(40));
        let sn = snippet(&s, "关键字");
        assert!(sn.starts_with(&"前".repeat(20)));
        assert!(sn.contains("关键字"));
    }

    #[test]
    fn writer_coalesces_and_flushes() {
        let dir = std::env::temp_dir().join(format!("stonebrook-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = open_conn(&dir.join("t.db")).unwrap();
        migrate(&mut w).unwrap();
        let conn = open_conn(&dir.join("t.db")).unwrap();
        let writer = Arc::new(Writer { q: Mutex::new(Queue::default()), cv: Condvar::new() });
        let error = Arc::new(Mutex::new(None));
        let (wc, ec) = (writer.clone(), error.clone());
        std::thread::spawn(move || wc.run(w, ec));
        let s = Store { conn: Arc::new(Mutex::new(conn)), writer, error, data_dir: dir.clone() };
        for i in 0..50 {
            s.set_kv("k", &i.to_string());
        }
        s.flush();
        assert_eq!(s.get_kv("k").as_deref(), Some("49"));
        let id = s.create(None, &DocContent::new_empty("测试文档标题"));
        assert_eq!(s.search("文档标", 10).len(), 1, "FTS 命中");
        assert_eq!(s.search("文档", 10).len(), 1, "短查询回退扫描");
        s.purge(&id);
        assert!(s.search("文档标", 10).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
