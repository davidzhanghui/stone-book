# vendor 补丁清单

石渠通过 `Cargo.toml` 的 `[patch.crates-io]` 使用两份本地修改过的依赖。每处改动在源码里都标了 `[stonebrook patch]`，
可以用 `grep -rn "stonebrook patch" vendor` 列出。升级上游版本时，按下表逐项移植。

## vendor/gpui（GPUI 0.2.2）

| 文件 | 改动 | 用途 |
|---|---|---|
| `src/style.rs`、`src/styled.rs`、`src/taffy.rs` | 新增 `grid_col_weights` / `grid_col_weights()`：按权重（`fr`）分配网格列宽，负数表示固定像素 | 文档表格拖动调整列宽 |
| `src/platform/mac/window.rs` | 环境变量 `STONEBROOK_FORCE_DRAW=1` 时窗口被遮挡也继续绘制 | 自动化截图测试 |
| `src/taffy.rs:312` | `0.0_f32` 显式标注字面量类型 | 消除 `float_literal_f32_fallback` 未来不兼容警告 |

## vendor/ironcalc_base（IronCalc 0.9.1）

| 文件 | 改动 | 用途 |
|---|---|---|
| `src/user_model/common.rs`、`history.rs`、`undo_redo.rs` | `UserModel` 的应用扩展数据 `ext`（`get_ext` / `set_ext` / `load_ext`），修改进入撤销历史 | 电子表格的下拉列表、备注、单元格图片等对象随工作簿保存、可撤销 |
| `src/user_model/common.rs` | `begin_batch` / `end_batch`：期间的多次修改合并成一个撤销步骤（可嵌套） | 粘贴、填充、排序等批量操作一次撤销 |
| `src/lib.rs`、`src/functions/mod.rs` | 导出全部函数目录 `(英文名, 分类)` | 电子表格「全部函数」面板 |

## 升级步骤

1. 用新版本源码替换目录，保留本表列出的改动（`grep -rn "stonebrook patch"` 对照旧版本）。
2. `cargo build && cargo test`，再跑一遍脚本测试（`STONEBROOK_SCRIPT`）里的表格与电子表格用例。
3. 更新本文件和 `Cargo.toml` 里的版本注释。
