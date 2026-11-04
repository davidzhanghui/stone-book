//! 用户设置（X3）：主题、正文字号、内容宽度、联网开关。保存在 kv 表中。
//! 渲染时频繁读取，所以用原子变量存一份，读取零开销。

use crate::store::Store;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

static FONT_SIZE: AtomicU32 = AtomicU32::new(15);
static WIDE: AtomicBool = AtomicBool::new(false);
static NETWORK: AtomicBool = AtomicBool::new(true);
/// 评论作者名（默认「我」，可在设置里改）
static AUTHOR: Mutex<Option<String>> = Mutex::new(None);

pub const FONT_SIZES: [u32; 4] = [14, 15, 16, 17];

pub fn load(store: &Store) {
    if let Some(v) = store.get_kv("font_size").and_then(|v| v.parse::<u32>().ok()) {
        FONT_SIZE.store(v.clamp(12, 20), Ordering::Relaxed);
    }
    WIDE.store(store.get_kv("wide").as_deref() == Some("1"), Ordering::Relaxed);
    NETWORK.store(store.get_kv("network").as_deref() != Some("0"), Ordering::Relaxed);
    if let Some(a) = store.get_kv("author") {
        *AUTHOR.lock().unwrap() = Some(a);
    }
}

pub fn font_size() -> f32 {
    FONT_SIZE.load(Ordering::Relaxed) as f32
}

pub fn set_font_size(store: &Store, v: u32) {
    FONT_SIZE.store(v, Ordering::Relaxed);
    store.set_kv("font_size", &v.to_string());
}

pub fn wide() -> bool {
    WIDE.load(Ordering::Relaxed)
}

pub fn set_wide(store: &Store, v: bool) {
    WIDE.store(v, Ordering::Relaxed);
    store.set_kv("wide", if v { "1" } else { "0" });
}

pub fn network() -> bool {
    NETWORK.load(Ordering::Relaxed)
}

pub fn set_network(store: &Store, v: bool) {
    NETWORK.store(v, Ordering::Relaxed);
    store.set_kv("network", if v { "1" } else { "0" });
}

/// 评论作者名，默认「我」。
pub fn author() -> String {
    AUTHOR.lock().unwrap().clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "我".to_string())
}

/// 只更新内存里的作者名（写库由调用方安排）。空字符串表示用默认的「我」。
pub fn set_author_mem(v: &str) {
    *AUTHOR.lock().unwrap() = Some(v.to_string());
}
