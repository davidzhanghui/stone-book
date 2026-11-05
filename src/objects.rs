//! 嵌入对象框架（X1）。
//!
//! 画板（含思维导图）/ 流程图 / 电子表格 / 多维表格这类「大对象」不放进文档 JSON，而是单独存进 objects 表，
//! 文档里只有一个 `Embed { kind, id }` 引用块。内存中有一层缓存，修改时版本号递增，
//! 文档内的预览据此刷新。删除引用块不会立即删除对象（撤销还要用），由资源回收统一清理。
//!
//! 保存：画板是不可变快照（`Arc`），直接交给写线程序列化；电子表格是原地修改的 `Rc<RefCell<_>>`，
//! 只能在界面线程编码，所以先标记为脏、停手 `SHEET_DEBOUNCE` 后由 `flush_dirty` 统一编码入队。

use crate::board::model::BoardDoc;
use crate::store::Store;
use ironcalc::base::UserModel;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub type Sheet = Rc<RefCell<UserModel<'static>>>;

/// 电子表格停止修改多久后落盘
pub const SHEET_DEBOUNCE: Duration = Duration::from_millis(400);

/// 对象种类。磁盘与文档 JSON 里仍是字符串（`as_str`），内存里用枚举避免到处比较字符串。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjKind {
    Board,
    Flow,
    Sheet,
    Base,
    Sketch,
}

impl ObjKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "board" => Self::Board,
            "flow" => Self::Flow,
            "sheet" => Self::Sheet,
            "base" => Self::Base,
            "sketch" => Self::Sketch,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Flow => "flow",
            Self::Sheet => "sheet",
            Self::Base => "base",
            Self::Sketch => "sketch",
        }
    }
}

#[derive(Clone)]
pub enum ObjData {
    Board(Arc<BoardDoc>),
    Sheet(Sheet),
    /// 多维表格：对象行里只存元数据，字段 / 记录 / 视图在 base_* 表里逐行存
    Base(crate::base::BaseRef),
    /// 草图：官方 `.excalidraw` 结构（不可变快照，写线程序列化）
    Sketch(Arc<crate::sketch::SketchDoc>),
}

struct Entry {
    kind: ObjKind,
    data: ObjData,
    version: u64,
    /// 电子表格：最近一次未落盘修改的时间
    dirty_since: Option<Instant>,
}

thread_local! {
    static OBJS: RefCell<HashMap<String, Entry>> = RefCell::new(HashMap::new());
}

pub fn store() -> Option<Store> {
    crate::store::global().cloned()
}

pub fn new_sheet() -> UserModel<'static> {
    let mut m = UserModel::new_empty("Sheet", "en", "UTC", "en").expect("ironcalc");
    m.rename_sheet(0, "Sheet1").ok();
    m
}

/// 电子表格的存储格式：IronCalc 工作簿二进制，后面可以跟一段扩展数据（下拉列表、备注、图片等单元格对象，JSON）。
/// 带扩展数据时以 `EXT_MAGIC` 开头：magic + 工作簿长度（u32 LE）+ 工作簿 + 扩展 JSON；旧数据没有 magic，原样解码。
const EXT_MAGIC: &[u8; 8] = b"\0INKSX1\0";

pub fn sheet_to_bytes(m: &UserModel) -> Vec<u8> {
    let wb = m.to_bytes();
    let ext = m.get_ext();
    if ext.is_empty() {
        return wb;
    }
    let mut out = Vec::with_capacity(wb.len() + ext.len() + 12);
    out.extend_from_slice(EXT_MAGIC);
    out.extend_from_slice(&(wb.len() as u32).to_le_bytes());
    out.extend_from_slice(&wb);
    out.extend_from_slice(ext.as_bytes());
    out
}

pub fn sheet_from_bytes(bytes: &[u8]) -> Option<UserModel<'static>> {
    if bytes.len() >= 12 && &bytes[..8] == EXT_MAGIC {
        let n = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
        let wb = bytes.get(12..12 + n)?;
        let mut m = UserModel::from_bytes(wb, "en").ok()?;
        m.load_ext(String::from_utf8_lossy(&bytes[12 + n..]).into_owned());
        return Some(m);
    }
    UserModel::from_bytes(bytes, "en").ok()
}

/// 把对象交给写线程保存。画板在写线程里序列化；表格 / 多维表格元数据在这里编码（必须在界面线程）。
fn persist(store: &Store, id: &str, kind: ObjKind, data: &ObjData) {
    match data {
        ObjData::Board(b) => {
            let b = b.clone();
            store.save_object_with(id, kind.as_str(), move || serde_json::to_vec(&*b).unwrap_or_default());
        }
        ObjData::Sheet(s) => store.save_object(id, kind.as_str(), sheet_to_bytes(&s.borrow())),
        ObjData::Base(b) => store.save_object(id, kind.as_str(), serde_json::to_vec(&b.borrow().meta).unwrap_or_default()),
        ObjData::Sketch(d) => {
            let d = d.clone();
            store.save_object_with(id, kind.as_str(), move || serde_json::to_vec(&*d).unwrap_or_default());
        }
    }
}

fn decode(id: &str, kind: ObjKind, bytes: &[u8]) -> Option<ObjData> {
    Some(match kind {
        ObjKind::Base => ObjData::Base(Rc::new(RefCell::new(crate::base::db::load(id, serde_json::from_slice(bytes).ok()?)))),
        ObjKind::Board | ObjKind::Flow => ObjData::Board(Arc::new(serde_json::from_slice(bytes).ok()?)),
        ObjKind::Sheet => ObjData::Sheet(Rc::new(RefCell::new(sheet_from_bytes(bytes)?))),
        ObjKind::Sketch => ObjData::Sketch(Arc::new(serde_json::from_slice(bytes).ok()?)),
    })
}

fn default_data(kind: ObjKind) -> ObjData {
    match kind {
        ObjKind::Sheet => ObjData::Sheet(Rc::new(RefCell::new(new_sheet()))),
        ObjKind::Flow => ObjData::Board(Arc::new(BoardDoc::new(true))),
        ObjKind::Sketch => ObjData::Sketch(Arc::new(crate::sketch::SketchDoc::new())),
        ObjKind::Board | ObjKind::Base => ObjData::Board(Arc::new(BoardDoc::new(false))),
    }
}

fn insert(id: &str, kind: ObjKind, data: ObjData) {
    if let Some(s) = store() {
        persist(&s, id, kind, &data);
    }
    OBJS.with(|o| o.borrow_mut().insert(id.to_string(), Entry { kind, data, version: 1, dirty_since: None }));
}

/// 新建对象并立即排队落盘。未知的 `kind` 按画板处理。
pub fn create(kind: &str, data: Option<ObjData>) -> String {
    let kind = ObjKind::parse(kind).unwrap_or(ObjKind::Board);
    let id = crate::store::new_uuid();
    let data = data.unwrap_or_else(|| default_data(kind));
    insert(&id, kind, data);
    id
}

/// 取对象（必要时从数据库加载）。
pub fn get(id: &str) -> Option<ObjData> {
    let hit = OBJS.with(|o| o.borrow().get(id).map(|e| e.data.clone()));
    if hit.is_some() {
        return hit;
    }
    let (kind, bytes) = store()?.load_object(id)?;
    let kind = ObjKind::parse(&kind)?;
    let data = decode(id, kind, &bytes)?;
    OBJS.with(|o| o.borrow_mut().insert(id.to_string(), Entry { kind, data: data.clone(), version: 1, dirty_since: None }));
    Some(data)
}

pub fn version(id: &str) -> u64 {
    OBJS.with(|o| o.borrow().get(id).map(|e| e.version).unwrap_or(0))
}

pub fn sketch(id: &str) -> Option<Arc<crate::sketch::SketchDoc>> {
    match get(id)? {
        ObjData::Sketch(d) => Some(d),
        _ => None,
    }
}

pub fn board(id: &str) -> Option<Arc<BoardDoc>> {
    match get(id)? {
        ObjData::Board(b) => Some(b),
        _ => None,
    }
}

/// 新建多维表格（斜杠菜单「表格 / 看板 / 画册」：只是默认视图不同）。
pub fn create_base(kind: crate::base::ViewKind) -> String {
    let id = crate::store::new_uuid();
    let mut b = crate::base::Base::new_default(&id, kind);
    b.persist = store().is_some();
    b.save_all();
    insert(&id, ObjKind::Base, ObjData::Base(Rc::new(RefCell::new(b))));
    id
}

/// 按预设新建多维表格（「任务看板」「日程」）。
pub fn create_base_preset(preset: &str) -> String {
    let id = crate::store::new_uuid();
    let mut b = crate::base::Base::new_preset(&id, preset);
    b.persist = store().is_some();
    b.save_all();
    insert(&id, ObjKind::Base, ObjData::Base(Rc::new(RefCell::new(b))));
    id
}

pub fn base(id: &str) -> Option<crate::base::BaseRef> {
    match get(id)? {
        ObjData::Base(b) => Some(b),
        _ => None,
    }
}

/// 所有多维表格（「关联已有多维表格」用）：(id, 名称, 数据表数)。
/// 只解析对象行里的元数据，不把记录加载进内存（已打开的用内存里的最新名称）。
pub fn list_bases() -> Vec<(String, String, usize)> {
    let Some(s) = store() else { return vec![] };
    s.objects_of_kind("base")
        .into_iter()
        .filter_map(|(id, bytes)| {
            if let Some(ObjData::Base(b)) = OBJS.with(|o| o.borrow().get(&id).map(|e| e.data.clone())) {
                let b = b.borrow();
                return Some((id, b.meta.name.clone(), b.meta.tables.len()));
            }
            let meta: crate::base::model::BaseMeta = serde_json::from_slice(&bytes).ok()?;
            Some((id, meta.name, meta.tables.len()))
        })
        .collect()
}

pub fn sheet(id: &str) -> Option<Sheet> {
    match get(id)? {
        ObjData::Sheet(s) => Some(s),
        _ => None,
    }
}

/// 更新对象并排队保存（电子表格只标脏，停手后再编码）。
pub fn update(id: &str, data: ObjData) {
    let kind = OBJS.with(|o| {
        let mut o = o.borrow_mut();
        let e = o.get_mut(id)?;
        e.data = data.clone();
        e.version += 1;
        if e.kind == ObjKind::Sheet {
            e.dirty_since = Some(Instant::now());
            return None;
        }
        Some(e.kind)
    });
    if let (Some(kind), Some(store)) = (kind, store()) {
        persist(&store, id, kind, &data);
    }
}

/// 把停手超过 `SHEET_DEBOUNCE` 的电子表格编码入队；`force` 时不论时间全部入队（退出前用）。
pub fn flush_dirty(force: bool) {
    let Some(store) = store() else { return };
    let due: Vec<(String, ObjKind, ObjData)> = OBJS.with(|o| {
        let mut o = o.borrow_mut();
        o.iter_mut()
            .filter(|(_, e)| e.dirty_since.is_some_and(|t| force || t.elapsed() >= SHEET_DEBOUNCE))
            .map(|(id, e)| {
                e.dirty_since = None;
                (id.clone(), e.kind, e.data.clone())
            })
            .collect()
    });
    for (id, kind, data) in due {
        persist(&store, &id, kind, &data);
    }
}

/// 删除对象（用完即弃的临时对象，如「以思维导图查看」生成的画板）。
pub fn remove(id: &str) {
    OBJS.with(|o| o.borrow_mut().remove(id));
    if let Some(s) = store() {
        s.delete_object(id);
    }
}

/// 电子表格 / 多维表格在原地修改（RefCell），调用此函数递增版本并保存。
pub fn touch(id: &str) {
    if let Some(d) = get(id) {
        update(id, d);
    }
}

/// 复制对象（粘贴嵌入块时使用），返回新 id。
pub fn duplicate(id: &str) -> Option<String> {
    let data = get(id)?;
    let kind = OBJS.with(|o| o.borrow().get(id).map(|e| e.kind))?;
    let copy = match &data {
        ObjData::Board(b) => ObjData::Board(Arc::new((**b).clone())),
        ObjData::Sketch(d) => ObjData::Sketch(Arc::new((**d).clone())),
        ObjData::Sheet(s) => ObjData::Sheet(Rc::new(RefCell::new(sheet_from_bytes(&sheet_to_bytes(&s.borrow()))?))),
        ObjData::Base(b) => {
            let new_id = crate::store::new_uuid();
            let copy = b.borrow().duplicate(&new_id);
            copy.save_all();
            insert(&new_id, ObjKind::Base, ObjData::Base(Rc::new(RefCell::new(copy))));
            return Some(new_id);
        }
    };
    Some(create(kind.as_str(), Some(copy)))
}

// ───────────── 导出 ─────────────

/// 电子表格已使用区域的显示值（最多 rows × cols）。
pub fn sheet_values(s: &Sheet, max_rows: i32, max_cols: i32) -> Vec<Vec<String>> {
    let m = s.borrow();
    let mut out = vec![];
    let mut last_row = 0;
    let mut last_col = 0;
    for r in 1..=max_rows {
        let mut row = vec![];
        for c in 1..=max_cols {
            let v = m.get_formatted_cell_value(0, r, c).unwrap_or_default();
            if !v.is_empty() {
                last_row = r;
                last_col = last_col.max(c);
            }
            row.push(v);
        }
        out.push(row);
    }
    out.truncate(last_row as usize);
    for row in out.iter_mut() {
        row.truncate(last_col.max(1) as usize);
    }
    out
}

pub fn to_markdown(kind: &str, id: &str) -> String {
    match get(id) {
        Some(ObjData::Sheet(s)) => {
            let vals = sheet_values(&s, 200, 26);
            if vals.is_empty() {
                return "（空表格）\n".into();
            }
            let mut out = String::new();
            for (i, row) in vals.iter().enumerate() {
                out.push_str(&format!("| {} |\n", row.iter().map(|c| c.replace('|', "\\|")).collect::<Vec<_>>().join(" | ")));
                if i == 0 {
                    out.push_str(&format!("|{}\n", " --- |".repeat(row.len())));
                }
            }
            out
        }
        Some(ObjData::Base(b)) => crate::base::io::to_markdown(&b.borrow()),
        Some(ObjData::Sketch(d)) => crate::sketch::to_markdown(&d),
        Some(ObjData::Board(b)) => {
            if let Some(md) = b.mind_outline_md() {
                return md;
            }
            let texts: Vec<String> = b.els.iter().filter(|e| !e.text.is_empty()).map(|e| e.text.replace('\n', " ")).collect();
            format!("[{}：{}]\n", if kind == "flow" { "流程图" } else { "画板" }, texts.join(" / "))
        }
        None => String::new(),
    }
}

pub fn to_html(_kind: &str, id: &str) -> String {
    match get(id) {
        Some(ObjData::Board(b)) => {
            let mut scene = b.to_scene(false);
            scene.fit(16.);
            scene.bg = Some(crate::canvas::Col::rgb(0xffffff));
            format!("<div class=\"board\">{}</div>", crate::canvas::svg::to_svg(&scene))
        }
        Some(ObjData::Sheet(s)) => {
            let vals = sheet_values(&s, 500, 26);
            let mut out = String::from("<table border=\"1\" style=\"border-collapse:collapse\">");
            for row in vals {
                out.push_str("<tr>");
                for c in row {
                    out.push_str(&format!("<td>{}</td>", c.replace('&', "&amp;").replace('<', "&lt;")));
                }
                out.push_str("</tr>");
            }
            out.push_str("</table>");
            out
        }
        Some(ObjData::Base(b)) => crate::base::io::to_html(&b.borrow()),
        Some(ObjData::Sketch(d)) => crate::sketch::to_html(&d),
        None => String::new(),
    }
}

/// 资源回收：删除未被任何文档（含回收站）、用户模板、同步块引用，且超过 7 天未修改的对象。
pub fn gc(store: &Store) -> usize {
    let refs = crate::store::collect_ids(&store.reference_texts());
    let week = 7 * 24 * 3600 * 1000;
    let now = crate::store::now_ms();
    let mut n = 0;
    for (id, kind, updated) in store.object_ids() {
        if now - updated > week && !refs.contains(&id) {
            store.delete_object(&id);
            if kind == "base" {
                crate::base::db::delete(store, &id);
            }
            n += 1;
        }
    }
    n
}
