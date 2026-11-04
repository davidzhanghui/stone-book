# 截图回归（tools/harness）

用 `STONEBROOK_SCRIPT` 回放一份逐行脚本操作窗口，在进程内截图（不需要「屏幕录制」权限）。
每次运行都用输出目录下全新的 `data/` 作为数据目录（`STONEBROOK_DATA_DIR`），**不会碰 `~/Stonebrook`**。

Python 一律用项目根目录的 venv（需要 Pillow）：

```sh
python3 -m venv .venv && .venv/bin/pip install Pillow   # 只需一次
cargo build --release                                     # 带 dev 功能（脚本驱动需要）
```

## 跑一次

```sh
.venv/bin/python tools/harness/run.py tools/harness/examples/basic.txt            # 结果在 /tmp/stonebrook-run
.venv/bin/python tools/harness/run.py my.txt /tmp/my-run
STONEBROOK_BIN=/other/stonebrook .venv/bin/python tools/harness/run.py my.txt
```

- 截图：`<输出目录>/<名字>.png`；日志：`<输出目录>/log`。
- 光标常亮（`STONEBROOK_NOBLINK`），窗口被遮挡也继续绘制（`STONEBROOK_FORCE_DRAW`）。

## 前后对比

```sh
.venv/bin/python tools/harness/compare.py tools/harness/examples/basic.txt <旧二进制> target/release/stonebrook
```

- 同一份脚本分别跑两个版本，逐张逐像素比较；有差异时输出差异像素数和范围，并把差异图写到 `/tmp/stonebrook-compare/diff-<名字>.png`，以状态 1 退出。
- **只移动代码、不改外观的重构（拆模块、换 token、抽公共组件），每张都应当是「一致」。**
- 脚本开头先 `rmove` 把指针移到空白处，否则真实鼠标位置会以 hover 的形式出现在截图里。
- 旧版二进制的编法（不影响当前目录的 target）：

  ```sh
  git worktree add /tmp/sb-old <提交或分支>
  (cd /tmp/sb-old && CARGO_TARGET_DIR=/tmp/sb-old-target cargo build --release)
  cp /tmp/sb-old-target/release/stonebrook /tmp/sb-old-bin
  git worktree remove /tmp/sb-old
  ```

## 脚本

命令的完整列表见 `src/autotest.rs` 开头的注释，常用的：

```text
rmove X Y                # 真实鼠标移动（停放指针）
key cmd-n                # 按键
type 你好 **粗体**        # 逐字输入（触发 Markdown 快捷语法）
click X Y [N] [shift]    # 点击
wait 毫秒 / shot 名字     # 等待 / 截图
```

`examples/basic.txt`：新建文档、各类块与行内样式、一段检验中文避头尾的长段落、斜杠菜单、命令面板。

其他用例：`numbering.txt`（有序列表编号菜单）、`reading.txt`（阅读模式）、`shortcuts.txt`（快捷键面板与改键）、`links.txt`（@ 链接与反向链接）、`themes.txt`（配色方案）、`english.txt`（英文界面）。
坐标是窗口坐标（pt），截图是 2x 像素：截图上的坐标除以 2 即可。`click` 直接走编辑器命中测试；需要触发元素自己的鼠标事件（菜单、按钮）时用 `rclick`。
