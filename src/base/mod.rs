//! 多维表格（P10）：一个多维表格引擎 + 多种视图（表格 / 看板 / 画册 / 甘特 / 日历 / 表单）。
//!
//! - `model.rs`：数据模型与撤销（多维表格 → 数据表 → 字段 / 记录 / 视图）
//! - `db.rs`：SQLite 逐行存储
//! - `formula.rs`：公式解析与求值
//! - `calc.rs`：计算字段、筛选 / 排序 / 分组、统计

pub mod calc;
pub mod db;
pub mod formula;
pub mod io;
pub mod model;
pub mod stress;

pub use model::{Base, ViewKind};

pub type BaseRef = std::rc::Rc<std::cell::RefCell<Base>>;

mod automation;
mod calendar;
mod cells;
mod chrome;
mod config;
mod dashboard;
mod detail;
pub mod editor;
mod form;
mod gallery;
mod gantt;
pub mod grid;
mod kanban;
mod panels;
#[cfg(feature = "dev")]
mod script;
pub(crate) mod widgets;

#[cfg(test)]
mod tests;
