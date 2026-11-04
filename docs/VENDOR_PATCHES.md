# vendor 补丁清单

石渠通过 `Cargo.toml` 的 `[patch.crates-io]` 使用两份本地修改过的依赖。每处改动在源码里都标了 `[stonebrook patch]`，
可以用 `grep -rn "stonebrook patch" vendor` 列出。升级上游版本时，按下表逐项移植。

## vendor/gpui（GPUI 0.2.2）

| 文件 | 改动 | 用途 |
|---|---|---|
| `src/style.rs`、`src/styled.rs`、`src/taffy.rs` | 新增 `grid_col_weights` / `grid_col_weights()`：按权重（`fr`）分配网格列宽，负数表示固定像素 | 文档表格拖动调整列宽 |
| `src/platform/mac/window.rs` | 环境变量 `STONEBROOK_FORCE_DRAW=1` 时窗口被遮挡也继续绘制 | 自动化截图测试 |
| `src/taffy.rs:312` | `0.0_f32` 显式标注字面量类型 | 消除 `float_literal_f32_fallback` 未来不兼容警告 |
| `src/text_system/line_wrapper.rs`、`line_layout.rs` | 中文避头尾：`is_no_break_before`（闭合标点不在行首）、`is_no_break_after`（开标点不在行尾），两处换行算法都用；测试 `test_wrap_line_kinsoku` | 正文、标题、表格里的中文换行符合排版习惯（移植自 quiet.md） |
| `src/platform/mac/text_system.rs` | 合成斜体：字体没有斜体字面（如苹方）时配一个倾斜绘制的孪生 FontId（`oblique_twins`），逐字形按原文位置决定是否倾斜 | 中文斜体像浏览器一样倾斜 |
| `src/platform/mac/text_system.rs` | `font_ids_by_postscript_name` 一个名字对应多个 FontId，CoreText 回退来的字体按字形数挑出同一份 | 系统回退的苹方与 PingFang.ttc 同名但字形表不同，原来会画成错字（移植自 quiet.md） |
| `src/platform/mac/text_system.rs` | `raster_bounds` 改用与光栅化相同字号的 CTFont 测量字形边界 | 系统字体随字号切换光学尺寸，大标题字形左侧原来会被切掉一条（移植自 quiet.md） |
| `src/platform/mac/text_system.rs` | 光栅化时打开字体平滑（`set_allows_font_smoothing` / `set_should_smooth_fonts`） | 笔画粗细与原生 AppKit 文字一致（移植自 quiet.md） |

## vendor/ironcalc_base（IronCalc 0.9.1）

| 文件 | 改动 | 用途 |
|---|---|---|
| `src/user_model/common.rs`、`history.rs`、`undo_redo.rs` | `UserModel` 的应用扩展数据 `ext`（`get_ext` / `set_ext` / `load_ext`），修改进入撤销历史 | 电子表格的下拉列表、备注、单元格图片等对象随工作簿保存、可撤销 |
| `src/user_model/common.rs` | `begin_batch` / `end_batch`：期间的多次修改合并成一个撤销步骤（可嵌套） | 粘贴、填充、排序等批量操作一次撤销 |
| `src/lib.rs`、`src/functions/mod.rs` | 导出全部函数目录 `(英文名, 分类)` | 电子表格「全部函数」面板 |

## 升级步骤

1. 用新版本源码替换目录，保留本表列出的改动（`grep -rn "stonebrook patch"` 对照旧版本）。
2. `scripts/check.sh`（含 vendored GPUI 的换行测试），再用 `tools/harness/compare.py` 对比升级前后的截图。
3. 更新本文件和 `Cargo.toml` 里的版本注释。
