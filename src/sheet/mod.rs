//! 电子表格：交互与界面。
//!
//! IronCalc 负责公式 / 依赖重算 / 样式 / 条件格式 / 撤销 / xlsx；这里是 GPUI 界面层：
//! - `mod.rs`   状态与命令：工具栏按钮、菜单、右键菜单、快捷键全部归一到 `run(cmd)`
//! - `paint.rs` 网格绘制：冻结四分区、文字溢出、自动换行、边框、数据条、引用高亮、选区
//! - `view.rs`  界面：两行工具栏、名称框 + 公式栏、弹出菜单、查找替换、筛选面板、工作表标签、状态栏
//! - `funcs.rs` 函数目录（补全 / 参数提示）、引用解析、中文序列
//! - `icons.rs` 工具栏图标
//! - `ext.rs`   单元格对象的数据（数据验证 / 下拉 / 复选框 / 提醒 / 备注 / 评论 / 图片 / 附件 / 迷你图 / 筛选视图 / 截断），
//!   存在 IronCalc `UserModel::ext`，随撤销恢复
//! - `objs.rs`  「插入」菜单的单元格对象交互：选择器、日历、链接、备注、评论、提醒
//! - `menu.rs`  同类五大类级联菜单
//! - `edit.rs`  选择性粘贴、单元格移位、截断、行高列宽、复制为图片
//! - `cf.rs`    条件格式（预设缩略图、图标集、管理规则面板）
//! - `data.rs`  数据验证、自定义排序、分列、清洗、去重、跨表引用、全部函数、筛选视图
//! - `dialogs.rs` 下拉列表 / 迷你图对话框与迷你图面板
//! - `color.rs` 11×6 调色板、更多颜色取色器、边框面板

mod cf;
mod color;
mod commands;
mod data;
mod dialogs;
mod edit;
mod ext;
mod funcs;
pub mod icons;
mod menu;
mod mouse;
mod objs;
mod paint;
#[cfg(feature = "dev")]
mod script;
mod view;

use crate::line_input::{InputEvent, LineInput};
use crate::objects::Sheet;
use gpui::*;
use ironcalc::base::expressions::types::Area;
use ironcalc::base::types::{CellType, Style};
use ironcalc::base::{BorderArea, ClipboardData};
use std::collections::{HashMap, HashSet};

pub(crate) const HEAD_W: f32 = 46.;
pub(crate) const HEAD_H: f32 = 24.;
const LAST_ROW: i32 = 1_048_576;
const LAST_COL: i32 = 16_384;

/// 声明 Action、默认按键与对应的命令字符串（命令由 `SheetEditor::run` 统一执行）。
macro_rules! sheet_keys {
    ($( $act:ident $key:literal => $cmd:literal ),* $(,)?) => {
        actions!(sheet, [$($act),*]);
        pub fn bind_keys(cx: &mut App) {
            let c = Some("Sheet");
            cx.bind_keys([$(KeyBinding::new($key, $act, c)),*]);
        }
        fn with_actions(d: Div, cx: &mut Context<SheetEditor>) -> Div {
            d$(.on_action(cx.listener(|this, _: &$act, w, cx| this.run($cmd, w, cx))))*
        }
    };
}

// 快捷键参见《在表格中使用快捷键》（Mac）。
sheet_keys! {
    SUp "up" => "up", SDown "down" => "down", SLeft "left" => "left", SRight "right" => "right",
    SExtUp "shift-up" => "ext-up", SExtDown "shift-down" => "ext-down", SExtLeft "shift-left" => "ext-left", SExtRight "shift-right" => "ext-right",
    SEdgeUp "cmd-up" => "edge-up", SEdgeDown "cmd-down" => "edge-down", SEdgeLeft "cmd-left" => "edge-left", SEdgeRight "cmd-right" => "edge-right",
    SXEdgeUp "cmd-shift-up" => "xedge-up", SXEdgeDown "cmd-shift-down" => "xedge-down", SXEdgeLeft "cmd-shift-left" => "xedge-left", SXEdgeRight "cmd-shift-right" => "xedge-right",
    STab "tab" => "tab", SShiftTab "shift-tab" => "stab", SEnter "enter" => "enter", SShiftEnter "shift-enter" => "senter",
    SEdit "f2" => "edit", SDelete "backspace" => "clear-content", SDelete2 "delete" => "clear-content", SEscape "escape" => "esc",
    SPageDown "pagedown" => "pgdn", SPageUp "pageup" => "pgup", SHome "home" => "home", SEnd "end" => "end",
    SToTop "cmd-home" => "top", SToEnd "cmd-end" => "bottom",
    SUndo "cmd-z" => "undo", SRedo "cmd-shift-z" => "redo", SRedo2 "cmd-y" => "redo",
    SCopy "cmd-c" => "copy", SCut "cmd-x" => "cut", SPaste "cmd-v" => "paste", SPasteValues "cmd-shift-v" => "paste-values", SPasteFormat "cmd-alt-v" => "paste-format",
    SBold "cmd-b" => "bold", SItalic "cmd-i" => "italic", SUnderline "cmd-u" => "underline", SStrike "cmd-shift-x" => "strike",
    SAlignC "cmd-shift-e" => "h-center", SAlignL "cmd-shift-l" => "h-left", SAlignR "cmd-shift-r" => "h-right",
    SClearFmt "cmd-\\" => "clear-format", SPainter "cmd-ctrl-c" => "painter", SMerge "alt-shift-d" => "merge-toggle",
    SFillDown "cmd-d" => "fill-down", SFillRight "cmd-r" => "fill-right",
    SDate "cmd-;" => "insert-date", STime "cmd-shift-;" => "insert-time", STime2 "cmd-:" => "insert-time",
    SAutoSum "cmd-alt-=" => "autosum", SFind "cmd-f" => "find", SLink "cmd-k" => "ins-link", SComment "cmd-alt-m" => "comment", SReplace "cmd-shift-h" => "replace", SKeys "cmd-/" => "shortcuts",
    SZoomIn "cmd-=" => "zoom-in", SZoomIn2 "cmd-+" => "zoom-in", SZoomIn3 "cmd-shift-=" => "zoom-in", SZoomOut "cmd--" => "zoom-out", SZoomReset "cmd-0" => "zoom-reset",
    SInsBefore "ctrl-shift-=" => "insert-before", SInsAfter "ctrl-shift-]" => "insert-after", SDelSel "cmd-alt--" => "delete-sel",
    SHideRows "cmd-alt-9" => "hide-rows", SHideCols "cmd-alt-0" => "hide-cols", SShowRows "cmd-shift-9" => "unhide-rows", SShowCols "cmd-shift-0" => "unhide-cols",
    SBTop "alt-shift-1" => "border:top", SBRight "alt-shift-2" => "border:right", SBBottom "alt-shift-3" => "border:bottom", SBLeft "alt-shift-4" => "border:left", SBNone "alt-shift-5" => "border:none",
    SFilter "ctrl-shift-l" => "filter", SSort "alt-shift-f" => "sort-asc", SShowFx "ctrl-`" => "show-formulas", SShowFx2 "ctrl-~" => "show-formulas",
    SNum1 "ctrl-shift-1" => "num:#,##0.00", SNum2 "ctrl-shift-2" => "num:hh:mm:ss", SNum3 "ctrl-shift-3" => "num:yyyy-mm-dd", SNum4 "ctrl-shift-4" => "num:\"¥\"#,##0.00", SNum5 "ctrl-shift-5" => "num:0.00%", SNum6 "ctrl-shift-6" => "num:0.00E+00",
    SF4 "f4" => "f4", SSelectAll "cmd-a" => "select-all", SNewSheet "shift-f11" => "new-sheet",
    SNextSheet "cmd-alt-down" => "next-sheet", SPrevSheet "cmd-alt-up" => "prev-sheet", SScrollCur "cmd-backspace" => "scroll-to-cursor",
}

pub enum SheetEvent {
    Close,
    /// 文档内嵌的表格点「全屏」
    Fullscreen,
}

impl EventEmitter<SheetEvent> for SheetEditor {}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) struct Cell {
    pub r: i32,
    pub c: i32,
}

/// 选区类型：普通区域 / 整行 / 整列 / 全表（点行头、列头、左上角）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SelKind {
    Cells,
    Rows,
    Cols,
    All,
}

enum SDrag {
    Select,
    Rows,
    Cols,
    ColResize {
        col: i32,
        start: f32,
        orig: f32,
    },
    RowResize {
        row: i32,
        start: f32,
        orig: f32,
    },
    Fill {
        to: Cell,
    },
    /// 编辑公式时拖选区域插入引用
    RefPick,
}

/// 弹出层种类。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Pop {
    Menu,
    Insert,
    Font,
    Size,
    Border,
    Color,
    Fill,
    Merge,
    Num,
    More,
    Freeze,
    Cond,
    Ctx,
    RowCtx,
    ColCtx,
    Tab(u32),
    FilterCol(i32),
    Sheets,
    Shortcuts,
    /// 高亮所在行列的颜色
    Hl,
    /// 缩放比例
    Zoom,
    /// 内嵌浮动工具栏的对齐 / 换行下拉
    Align,
    Wrap,
    /// 「更多」里的公式 / 筛选下拉
    Formula,
    FilterMenu,
    SortMenu,
}

pub(crate) struct Popup {
    pub kind: Pop,
    pub at: Point<Pixels>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PromptKind {
    /// 条件格式：大于 / 小于 / 等于 / 文本包含
    Cf(&'static str),
    /// 自定义数字格式
    NumFmt,
}

pub(crate) struct Prompt {
    pub kind: PromptKind,
    pub input: Entity<LineInput>,
    pub at: Point<Pixels>,
    _sub: Subscription,
}

pub(crate) struct Edit {
    pub input: Entity<LineInput>,
    _sub: Subscription,
    /// 直接打字进入（「输入模式」）：方向键结束编辑并移动；F2 / 双击进入的是「编辑模式」
    pub typed: bool,
}

pub(crate) struct Find {
    pub q: Entity<LineInput>,
    pub rep: Entity<LineInput>,
    pub replace: bool,
    pub case: bool,
    pub whole: bool,
    /// 命中项：(工作表, 单元格)
    pub matches: Vec<(u32, Cell)>,
    pub idx: usize,
    _subs: Vec<Subscription>,
    /// 范围：0 当前工作表 1 全部工作表 2 选区
    pub scope: u8,
    pub scope_open: bool,
    /// 搜索范围包括公式
    pub formulas: bool,
    /// 收起成只有查找框的紧凑条
    pub compact: bool,
    /// 打开时的选区（范围为「选区」时用）
    pub area: (i32, i32, i32, i32),
}

/// 筛选（表头出现筛选按钮，按值勾选；被筛掉的行隐藏）。只在本次打开期间有效，不写入工作簿。
pub(crate) struct Filter {
    pub sheet: u32,
    pub head: i32,
    pub last: i32,
    pub c0: i32,
    pub c1: i32,
    pub excl: HashMap<i32, HashSet<String>>,
    pub hidden: HashSet<i32>,
}

/// 站内剪贴板：保留公式（相对引用平移）与样式；系统剪贴板同时写 TSV + HTML。
struct Clip {
    tsv: String,
    sheet: u32,
    range: (i32, i32, i32, i32),
    data: serde_json::Value,
    cut: bool,
    /// 蚂蚁线是否还显示：点选别的单元格后隐藏，但内部粘贴（含样式）仍可用
    shown: bool,
    /// 复制时源工作表的单元格对象（下拉列表、备注、图片…随粘贴一起复制）
    sx: ext::SheetX,
}

pub struct SheetEditor {
    pub obj_id: String,
    pub(crate) model: Sheet,
    pub(crate) sheet: u32,
    pub(crate) cur: Cell,
    pub(crate) anchor: Cell,
    pub(crate) kind: SelKind,
    pub(crate) scroll: (f32, f32),
    pub(crate) zoom: f32,
    pub(crate) editing: Option<Edit>,
    pub(crate) formula: Entity<LineInput>,
    pub(crate) name_box: Entity<LineInput>,
    _subs: Vec<Subscription>,
    pub(crate) focus: FocusHandle,
    pub(crate) bounds: Bounds<Pixels>,
    drag: Option<SDrag>,
    pub(crate) rename: Option<(u32, Entity<LineInput>, Subscription)>,
    geo_key: (u64, u32, u32, u64, i32, i32),
    pub(crate) col_x: Vec<f32>,
    pub(crate) row_y: Vec<f32>,
    pub(crate) fr: i32,
    pub(crate) fc: i32,
    pub(crate) ver: u64,
    pub(crate) pop: Option<Popup>,
    pub(crate) prompt: Option<Prompt>,
    /// 格式刷：(源样式矩阵, 是否连续使用)
    pub(crate) painter: Option<(Vec<Vec<Style>>, bool)>,
    pub(crate) find: Option<Find>,
    pub(crate) filter: Option<Filter>,
    filter_ver: u64,
    pub(crate) collapsed: bool,
    pub(crate) comp_sel: usize,
    ref_pick: Option<(usize, Cell)>,
    clip: Option<Clip>,
    /// 鼠标悬停在列头 / 行头分隔线上：(是否列, 列号或行号)
    pub(crate) resize_hover: Option<(bool, i32)>,
    pub(crate) show_formulas: bool,
    pub(crate) toast: Option<SharedString>,
    pub(crate) toast_timer: bool,
    /// 可视区单元格绘制数据的缓存（数据 / 视口 / 缩放不变时复用）
    pub(crate) cache: Option<(u64, std::rc::Rc<Vec<paint::CellDraw>>)>,
    /// 扩展数据解析缓存：(原始 JSON, 解析结果)
    pub(crate) ext_cache: std::cell::RefCell<(String, std::rc::Rc<ext::Ext>)>,
    /// 贴在单元格上的弹出层（下拉选择、日期、链接、备注、评论）
    pub(crate) cpop: Option<objs::CellPop>,
    /// 对话框
    pub(crate) dlg: Option<dialogs::Dlg>,
    /// 右侧面板
    pub(crate) side: Option<dialogs::Side>,
    /// 鼠标悬停的单元格（显示备注）
    pub(crate) hover_cell: Option<Cell>,
    /// 悬停的附件（格子, 第几个）：显示浮动工具栏
    pub(crate) hover_file: Option<(Cell, usize)>,
    /// 附件全屏预览正在用的播放器 / 原生预览视图 id（media::external_id）：关闭预览时释放
    pub(crate) fv_media: Option<u64>,
    /// 附件全屏预览里图片查看器的状态（缩放 / 旋转 / 平移），每次打开预览清空
    pub(crate) img_views: HashMap<u64, crate::editor::ImgView>,
    /// 图片原始尺寸缓存
    pub(crate) image_sizes: HashMap<String, Option<(u32, u32)>>,
    /// 迷你图面板里展开的下拉：0 无 1 主题 2 颜色 3 折线粗细 8+k 第 k 个标记的颜色
    pub(crate) spark_open: u8,
    /// 迷你图面板在「高级配置」页
    pub(crate) spark_adv: bool,
    /// 底部「+ N 行」追加的空行数（表格是无限行，这里只是把显示区域撑长并滚过去）
    pub(crate) extra_rows: i32,
    pub(crate) add_rows: Entity<LineInput>,
    /// 级联菜单展开路径
    pub(crate) menu_path: Vec<usize>,
    pub(crate) menu_kind: menu::MenuKind,
    pub(crate) menu_inputs: menu::MenuInputs,
    /// 当前打开的筛选视图
    pub(crate) view_on: Option<usize>,
    /// 对话框里输入框的订阅
    pub(crate) dlg_subs: Vec<Subscription>,
    /// 「全部函数」面板的搜索框
    pub(crate) func_q: Entity<LineInput>,
    /// 更多颜色取色器
    pub(crate) picker: Option<color::Picker>,
    /// 边框：线条颜色、线型、面板里展开的子项（1 颜色 2 线型）
    pub(crate) border_color: String,
    pub(crate) border_style: String,
    pub(crate) border_sub: u8,
    /// 嵌在文档里（只显示网格 + 浮动工具栏）
    pub embedded: bool,
    /// 文档给的最大宽度
    pub embed_max_w: f32,
}

impl Focusable for SheetEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

pub(crate) fn area(sheet: u32, r0: i32, c0: i32, r1: i32, c1: i32) -> Area {
    Area { sheet, row: r0.min(r1), column: c0.min(c1), width: (c1 - c0).abs() + 1, height: (r1 - r0).abs() + 1 }
}

/// 每个表格的查看偏好（高亮所在行列），只在本次运行期间有效，不进撤销历史。
#[derive(Clone, Default)]
pub(crate) struct ViewPrefs {
    pub hl: Option<String>,
}

thread_local! {
    static PREFS: std::cell::RefCell<HashMap<String, ViewPrefs>> = std::cell::RefCell::new(HashMap::new());
    /// 颜色面板的「最近使用」
    pub(crate) static RECENT_COLORS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub(crate) fn view_prefs(id: &str) -> ViewPrefs {
    PREFS.with(|p| p.borrow().get(id).cloned().unwrap_or_default())
}

pub(crate) fn set_view_prefs(id: &str, f: impl FnOnce(&mut ViewPrefs)) {
    PREFS.with(|p| f(p.borrow_mut().entry(id.to_string()).or_default()));
}

pub(crate) fn hex_to_hsla(s: &str) -> Option<Hsla> {
    let h = s.trim_start_matches('#');
    u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6).map(|v| rgb(v).into())
}

pub(crate) fn col_name(c: i32) -> String {
    crate::editor::col_name(c)
}

fn addr(r: i32, c: i32) -> String {
    format!("{}{}", col_name(c), r)
}

fn range_text(a: &Area) -> String {
    if a.width == 1 && a.height == 1 {
        addr(a.row, a.column)
    } else {
        format!("{}:{}", addr(a.row, a.column), addr(a.row + a.height - 1, a.column + a.width - 1))
    }
}

impl SheetEditor {
    pub fn new(obj_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = crate::objects::sheet(&obj_id).unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(crate::objects::new_sheet())));
        let formula = cx.new(|cx| LineInput::new("", cx));
        let name_box = cx.new(|cx| LineInput::new("", cx));
        let s1 = cx.subscribe_in(&formula, window, |this, input, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { shift } => {
                let v = input.read(cx).text();
                this.editing = None;
                this.set_value(this.cur, &v, cx);
                this.move_by(if *shift { -1 } else { 1 }, 0, false, cx);
                window.focus(&this.focus);
            }
            InputEvent::Cancel => {
                this.sync_formula(cx);
                window.focus(&this.focus);
            }
            _ => {}
        });
        // 名称框：输入 B12 或 A1:C5 回车跳转（同类）
        let s2 = cx.subscribe_in(&name_box, window, |this, input, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => {
                let v = input.read(cx).text();
                if let Some((r0, c0, r1, c1)) = funcs::parse_range(&v) {
                    this.select(Cell { r: r0, c: c0 }, Cell { r: r1, c: c1 }, SelKind::Cells, cx);
                }
                window.focus(&this.focus);
                this.sync_formula(cx);
            }
            InputEvent::Cancel => {
                window.focus(&this.focus);
                this.sync_formula(cx);
            }
            _ => {}
        });
        let mut s = SheetEditor {
            obj_id,
            model,
            sheet: 0,
            cur: Cell { r: 1, c: 1 },
            anchor: Cell { r: 1, c: 1 },
            kind: SelKind::Cells,
            scroll: (0., 0.),
            zoom: 1.,
            editing: None,
            formula,
            name_box,
            _subs: vec![s1, s2],
            focus: cx.focus_handle(),
            bounds: Bounds::default(),
            drag: None,
            rename: None,
            geo_key: (u64::MAX, 0, 0, 0, 0, 0),
            col_x: vec![],
            row_y: vec![],
            fr: 0,
            fc: 0,
            ver: 0,
            pop: None,
            prompt: None,
            painter: None,
            find: None,
            filter: None,
            filter_ver: 0,
            collapsed: false,
            comp_sel: 0,
            ref_pick: None,
            clip: None,
            resize_hover: None,
            show_formulas: false,
            toast: None,
            toast_timer: false,
            cache: None,
            ext_cache: Default::default(),
            cpop: None,
            dlg: None,
            side: None,
            hover_cell: None,
            hover_file: None,
            fv_media: None,
            img_views: HashMap::new(),
            image_sizes: HashMap::new(),
            spark_open: 0,
            spark_adv: false,
            extra_rows: 0,
            menu_path: vec![],
            menu_kind: menu::MenuKind::Main,
            menu_inputs: menu::MenuInputs::new(cx),
            view_on: None,
            dlg_subs: vec![],
            picker: None,
            border_color: "#1f2329".into(),
            border_style: "thin".into(),
            border_sub: 0,
            embedded: false,
            embed_max_w: 720.,
            func_q: cx.new(|cx| LineInput::new("搜索函数名或说明", cx)),
            add_rows: cx.new(|cx| {
                let mut i = LineInput::new("", cx);
                i.set_text("20", cx);
                i
            }),
        };
        // 日期提醒：每 30 秒检查一次
        cx.spawn(async move |this: WeakEntity<SheetEditor>, cx: &mut AsyncApp| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(30)).await;
            if this.update(cx, |this, cx| this.check_reminders(cx)).is_err() {
                break;
            }
        })
        .detach();
        let fq = s.func_q.clone();
        s._subs.push(cx.subscribe(&fq, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Changed) {
                cx.notify();
            }
        }));
        s.sync_formula(cx);
        s
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.ver += 1;
        crate::objects::touch(&self.obj_id);
        if self.find.is_some() {
            self.refresh_find(cx);
        }
        cx.notify();
    }

    pub(crate) fn sync_formula(&mut self, cx: &mut Context<Self>) {
        let v = self.model.borrow().get_cell_content(self.sheet, self.cur.r, self.cur.c).unwrap_or_default();
        self.formula.update(cx, |f, cx| {
            f.set_text(&v, cx);
            f.collapse_to_end(cx);
        });
        let a = self.sel_area();
        let name = match self.kind {
            SelKind::Cells => range_text(&a),
            SelKind::Rows => format!("{}:{}", a.row, a.row + a.height - 1),
            SelKind::Cols => format!("{}:{}", col_name(a.column), col_name(a.column + a.width - 1)),
            SelKind::All => "全选".into(),
        };
        self.name_box.update(cx, |f, cx| {
            f.set_text(&name, cx);
            f.collapse_to_end(cx);
        });
    }

    fn set_value(&mut self, cell: Cell, v: &str, cx: &mut Context<Self>) {
        let old = self.model.borrow().get_cell_content(self.sheet, cell.r, cell.c).unwrap_or_default();
        if old == v {
            return;
        }
        self.model.borrow_mut().set_user_input(self.sheet, cell.r, cell.c, v).ok();
        // 数据验证：拒绝输入的规则直接撤回，否则只提示（格子右上角标红）
        if let Some(dv) = self.sx().dv_at(cell.r, cell.c).cloned() {
            if !self.valid_at(&dv, cell.r, cell.c) {
                if dv.reject {
                    self.model.borrow_mut().undo().ok();
                    self.toast = Some(if dv.hint.is_empty() { dv.describe() } else { dv.hint.clone() }.into());
                    cx.notify();
                    return;
                }
                self.toast = Some(format!("输入的值不符合数据验证：{}", if dv.hint.is_empty() { dv.describe() } else { dv.hint.clone() }).into());
            }
        }
        self.autofit_row(cell.r);
        self.changed(cx);
    }

    /// 该格的值是否符合数据验证。
    pub(crate) fn valid_at(&self, dv: &ext::Dv, r: i32, c: i32) -> bool {
        let m = self.model.borrow();
        let shown = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
        let num = match m.get_model().get_cell_value_by_index(self.sheet, r, c) {
            Ok(ironcalc::base::cell::CellValue::Number(v)) => Some(v),
            _ => None,
        };
        let shown = match m.get_model().get_cell_value_by_index(self.sheet, r, c) {
            Ok(ironcalc::base::cell::CellValue::Boolean(b)) => {
                if b {
                    "TRUE".into()
                } else {
                    "FALSE".into()
                }
            }
            _ => shown,
        };
        dv.check(&shown, num)
    }

    // ───────────── 几何 ─────────────

    /// 已使用区域的最大行列。
    pub(crate) fn used(&self) -> (i32, i32) {
        let m = self.model.borrow();
        m.get_model()
            .workbook
            .worksheet(self.sheet)
            .map(|ws| {
                let d = ws.dimension();
                (d.max_row, d.max_column)
            })
            .unwrap_or((1, 1))
    }

    /// 需要建几何缓存的行列数：已用区域 + 余量，并随滚动无限延伸。
    fn extent(&self) -> (i32, i32) {
        let (ur, uc) = self.used();
        let vh = f32::from(self.bounds.size.height).max(800.);
        let vw = f32::from(self.bounds.size.width).max(1200.);
        let rows = (ur + 50 + self.extra_rows).max(self.cur.r.max(self.anchor.r) + 30).max(((self.scroll.1 + vh * 2.) / (20. * self.zoom)) as i32 + 1).max(100);
        let cols = (uc + 8).max(self.cur.c.max(self.anchor.c) + 6).max(((self.scroll.0 + vw * 2.) / (60. * self.zoom)) as i32 + 1).max(26);
        (rows.min(LAST_ROW - 1), cols.min(LAST_COL - 1))
    }

    pub(crate) fn rebuild_geometry(&mut self) {
        let (rows, cols) = self.extent();
        let key = (self.ver ^ (crate::objects::version(&self.obj_id) << 32), self.sheet, self.zoom.to_bits(), self.filter_ver, rows, cols);
        if self.geo_key == key {
            return;
        }
        self.geo_key = key;
        let z = self.zoom;
        let m = self.model.borrow();
        let ws = m.get_model().workbook.worksheet(self.sheet).ok();
        self.fr = m.get_frozen_rows_count(self.sheet).unwrap_or(0);
        self.fc = m.get_frozen_columns_count(self.sheet).unwrap_or(0);
        self.col_x.clear();
        self.col_x.push(0.);
        let mut x = 0.;
        for c in 1..=cols + 1 {
            let hidden = ws.is_some_and(|w| w.is_column_hidden(c).unwrap_or(false));
            if !hidden {
                x += m.get_column_width(self.sheet, c).unwrap_or(90.) as f32 * z;
            }
            self.col_x.push(x);
        }
        let fhid = self.filter.as_ref().filter(|f| f.sheet == self.sheet).map(|f| &f.hidden);
        self.row_y.clear();
        self.row_y.push(0.);
        let mut y = 0.;
        for r in 1..=rows + 1 {
            let hidden = ws.is_some_and(|w| w.is_row_hidden(r).unwrap_or(false)) || fhid.is_some_and(|h| h.contains(&r));
            if !hidden {
                y += (m.get_row_height(self.sheet, r).unwrap_or(25.) as f32).max(16.) * z;
            }
            self.row_y.push(y);
        }
    }

    pub(crate) fn col_left(&self, c: i32) -> f32 {
        let i = (c - 1).max(0) as usize;
        self.col_x.get(i).or(self.col_x.last()).copied().unwrap_or(0.)
    }
    pub(crate) fn row_top(&self, r: i32) -> f32 {
        let i = (r - 1).max(0) as usize;
        self.row_y.get(i).or(self.row_y.last()).copied().unwrap_or(0.)
    }
    pub(crate) fn col_w(&self, c: i32) -> f32 {
        self.col_left(c + 1) - self.col_left(c)
    }
    pub(crate) fn row_h(&self, r: i32) -> f32 {
        self.row_top(r + 1) - self.row_top(r)
    }
    /// 冻结区宽 / 高
    pub(crate) fn frozen_wh(&self) -> (f32, f32) {
        (self.col_left(self.fc + 1), self.row_top(self.fr + 1))
    }

    /// 内容坐标 → 网格区域内的局部坐标（考虑冻结与滚动）。
    pub(crate) fn lx(&self, x: f32, frozen: bool) -> f32 {
        HEAD_W + x - if frozen { 0. } else { self.scroll.0 }
    }
    pub(crate) fn ly(&self, y: f32, frozen: bool) -> f32 {
        HEAD_H + y - if frozen { 0. } else { self.scroll.1 }
    }

    /// 局部坐标 → 单元格（二分查找前缀和）。
    fn cell_at_local(&self, lx: f32, ly: f32) -> Cell {
        let (fw, fh) = self.frozen_wh();
        let x = lx - HEAD_W;
        let y = ly - HEAD_H;
        let x = if x < fw { x.max(0.) } else { x + self.scroll.0 };
        let y = if y < fh { y.max(0.) } else { y + self.scroll.1 };
        let c = self.col_x.partition_point(|&v| v <= x).max(1) as i32;
        let r = self.row_y.partition_point(|&v| v <= y).max(1) as i32;
        Cell { r, c }
    }

    fn local(&self, p: Point<Pixels>) -> (f32, f32) {
        (f32::from(p.x - self.bounds.origin.x), f32::from(p.y - self.bounds.origin.y))
    }

    /// 单元格（合并单元格取整块）在窗口中的矩形。
    pub(crate) fn cell_rect(&self, c: Cell) -> Bounds<Pixels> {
        let (r0, c0, r1, c1) = self.merged_at(c).map(|m| (m.0, m.1, m.0 + m.3 - 1, m.1 + m.2 - 1)).unwrap_or((c.r, c.c, c.r, c.c));
        let x0 = self.lx(self.col_left(c0), c0 <= self.fc);
        let x1 = self.lx(self.col_left(c1 + 1), c0 <= self.fc);
        let y0 = self.ly(self.row_top(r0), r0 <= self.fr);
        let y1 = self.ly(self.row_top(r1 + 1), r0 <= self.fr);
        Bounds::from_corners(
            point(self.bounds.origin.x + px(x0), self.bounds.origin.y + px(y0)),
            point(self.bounds.origin.x + px(x1), self.bounds.origin.y + px(y1)),
        )
    }

    pub(crate) fn merged(&self) -> Vec<(i32, i32, i32, i32)> {
        self.model.borrow().get_merged_cells(self.sheet).unwrap_or_default().iter().map(|m| (m.row, m.column, m.width, m.height)).collect()
    }

    /// 包含该格的合并区域 (行, 列, 宽, 高)。
    fn merged_at(&self, c: Cell) -> Option<(i32, i32, i32, i32)> {
        self.merged().into_iter().find(|m| c.r >= m.0 && c.r < m.0 + m.3 && c.c >= m.1 && c.c < m.1 + m.2)
    }

    fn ensure_visible(&mut self) {
        self.rebuild_geometry();
        let (fw, fh) = self.frozen_wh();
        let margin = if self.embedded { 0. } else { 12. };
        let vw = f32::from(self.bounds.size.width) - HEAD_W - fw - margin;
        let vh = f32::from(self.bounds.size.height) - HEAD_H - fh - margin;
        if vw <= 0. || vh <= 0. {
            return;
        }
        if self.cur.c > self.fc {
            let (x0, x1) = (self.col_left(self.cur.c) - fw, self.col_left(self.cur.c + 1) - fw);
            if x0 < self.scroll.0 {
                self.scroll.0 = x0;
            } else if x1 > self.scroll.0 + vw {
                self.scroll.0 = x1 - vw;
            }
        }
        if self.cur.r > self.fr {
            let (y0, y1) = (self.row_top(self.cur.r) - fh, self.row_top(self.cur.r + 1) - fh);
            if y0 < self.scroll.1 {
                self.scroll.1 = y0;
            } else if y1 > self.scroll.1 + vh {
                self.scroll.1 = y1 - vh;
            }
        }
        self.scroll = (self.scroll.0.max(0.), self.scroll.1.max(0.));
    }

    // ───────────── 选区 ─────────────

    /// 当前选区（普通区域会扩展到完整覆盖合并单元格）。
    pub(crate) fn sel_area(&self) -> Area {
        let (mut r0, mut r1) = (self.anchor.r.min(self.cur.r), self.anchor.r.max(self.cur.r));
        let (mut c0, mut c1) = (self.anchor.c.min(self.cur.c), self.anchor.c.max(self.cur.c));
        match self.kind {
            SelKind::Rows => return area(self.sheet, r0, 1, r1, LAST_COL),
            SelKind::Cols => return area(self.sheet, 1, c0, LAST_ROW, c1),
            SelKind::All => return area(self.sheet, 1, 1, LAST_ROW, LAST_COL),
            SelKind::Cells => {}
        }
        let merged = self.merged();
        loop {
            let mut grew = false;
            for m in &merged {
                let (mr1, mc1) = (m.0 + m.3 - 1, m.1 + m.2 - 1);
                let hit = m.0 <= r1 && mr1 >= r0 && m.1 <= c1 && mc1 >= c0;
                if hit && (m.0 < r0 || mr1 > r1 || m.1 < c0 || mc1 > c1) {
                    r0 = r0.min(m.0);
                    r1 = r1.max(mr1);
                    c0 = c0.min(m.1);
                    c1 = c1.max(mc1);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        area(self.sheet, r0, c0, r1, c1)
    }

    /// 选区与已用区域求交（用于逐格遍历的操作：复制、统计、格式刷）。
    pub(crate) fn sel_used(&self) -> Area {
        let a = self.sel_area();
        if self.kind == SelKind::Cells {
            return a;
        }
        let (ur, uc) = self.used();
        let r1 = (a.row + a.height - 1).min(ur.max(a.row));
        let c1 = (a.column + a.width - 1).min(uc.max(a.column));
        area(self.sheet, a.row, a.column, r1, c1)
    }

    fn select(&mut self, a: Cell, b: Cell, kind: SelKind, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        self.anchor = a;
        self.cur = b;
        self.kind = kind;
        self.ensure_visible();
        self.sync_formula(cx);
        cx.notify();
    }

    pub(crate) fn is_hidden_row(&self, r: i32) -> bool {
        (r as usize) < self.row_y.len() && self.row_h(r) <= 0.
    }
    pub(crate) fn is_hidden_col(&self, c: i32) -> bool {
        (c as usize) < self.col_x.len() && self.col_w(c) <= 0.
    }

    fn move_by(&mut self, dr: i32, dc: i32, extend: bool, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        self.rebuild_geometry();
        let mut from = self.cur;
        // 从合并单元格出发：跳过整块
        if !extend {
            if let Some(m) = self.merged_at(from) {
                if dr > 0 {
                    from.r = m.0 + m.3 - 1;
                }
                if dc > 0 {
                    from.c = m.1 + m.2 - 1;
                }
                if dr < 0 {
                    from.r = m.0;
                }
                if dc < 0 {
                    from.c = m.1;
                }
            }
        }
        let mut next = Cell { r: (from.r + dr).max(1), c: (from.c + dc).max(1) };
        // 跳过隐藏的行列
        let mut guard = 0;
        while dr != 0 && self.is_hidden_row(next.r) && next.r > 1 && guard < 10000 {
            next.r += dr.signum();
            guard += 1;
        }
        while dc != 0 && self.is_hidden_col(next.c) && next.c > 1 && guard < 20000 {
            next.c += dc.signum();
            guard += 1;
        }
        next.r = next.r.clamp(1, LAST_ROW);
        next.c = next.c.clamp(1, LAST_COL);
        if !extend {
            if let Some(m) = self.merged_at(next) {
                next = Cell { r: m.0, c: m.1 };
            }
        }
        if !extend {
            self.anchor = next;
            self.kind = SelKind::Cells;
        }
        self.cur = next;
        self.ref_pick = None;
        self.ensure_visible();
        self.sync_formula(cx);
        cx.notify();
    }

    fn is_empty(&self, r: i32, c: i32) -> bool {
        self.model.borrow().get_cell_content(self.sheet, r, c).map(|s| s.is_empty()).unwrap_or(true)
    }

    /// ⌘ + 方向键：移到当前数据区边缘（Excel的规则）。
    fn edge(&mut self, dr: i32, dc: i32, extend: bool, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        let (ur, uc) = self.used();
        let (maxr, maxc) = (ur + 1, uc + 1);
        let mut p = self.cur;
        let step = |p: Cell| Cell { r: p.r + dr, c: p.c + dc };
        let inb = |p: Cell| p.r >= 1 && p.c >= 1 && p.r <= maxr && p.c <= maxc;
        let n = step(p);
        if !inb(n) {
            p = Cell {
                r: if dr < 0 {
                    1
                } else if dr > 0 {
                    maxr
                } else {
                    p.r
                },
                c: if dc < 0 {
                    1
                } else if dc > 0 {
                    maxc
                } else {
                    p.c
                },
            };
        } else if !self.is_empty(p.r, p.c) && !self.is_empty(n.r, n.c) {
            while inb(step(p)) && !self.is_empty(step(p).r, step(p).c) {
                p = step(p);
            }
        } else {
            p = n;
            while inb(step(p)) && self.is_empty(p.r, p.c) {
                p = step(p);
            }
        }
        let p = Cell { r: p.r.max(1), c: p.c.max(1) };
        let (dr2, dc2) = (p.r - self.cur.r, p.c - self.cur.c);
        if extend {
            self.cur = p;
            self.ensure_visible();
            self.sync_formula(cx);
            cx.notify();
        } else {
            self.move_by(dr2, dc2, false, cx);
        }
    }

    // ───────────── 编辑 ─────────────

    pub(crate) fn editing_text(&self, cx: &App) -> Option<String> {
        self.editing.as_ref().map(|e| e.input.read(cx).text())
    }

    fn start_edit(&mut self, initial: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.pop = None;
        let typed = initial.is_some();
        let text = initial.unwrap_or_else(|| self.model.borrow().get_cell_content(self.sheet, self.cur.r, self.cur.c).unwrap_or_default());
        let input = cx.new(|cx| LineInput::new("", cx));
        input.update(cx, |i, cx| {
            i.set_text(&text, cx);
            i.collapse_to_end(cx);
        });
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { shift } => {
                if !*shift && this.accept_completion(window, cx) {
                    return;
                }
                this.commit_edit(cx);
                this.move_by(if *shift { -1 } else { 1 }, 0, false, cx);
                window.focus(&this.focus);
            }
            InputEvent::Cancel => {
                this.editing = None;
                this.ref_pick = None;
                window.focus(&this.focus);
                this.sync_formula(cx);
                cx.notify();
            }
            InputEvent::Up | InputEvent::Down => {
                let n = this.completions(cx).len();
                if n > 0 {
                    let d = if matches!(ev, InputEvent::Up) { n - 1 } else { 1 };
                    this.comp_sel = (this.comp_sel + d) % n;
                    cx.notify();
                    return;
                }
                this.commit_edit(cx);
                this.move_by(if matches!(ev, InputEvent::Up) { -1 } else { 1 }, 0, false, cx);
                window.focus(&this.focus);
            }
            InputEvent::Changed => {
                if let Some(e) = &this.editing {
                    let t = e.input.read(cx).text();
                    this.formula.update(cx, |f, cx| {
                        f.set_text(&t, cx);
                        f.collapse_to_end(cx);
                    });
                }
                this.comp_sel = 0;
                cx.notify();
            }
        });
        window.focus(&input.focus_handle(cx));
        self.formula.update(cx, |f, cx| {
            f.set_text(&text, cx);
            f.collapse_to_end(cx);
        });
        self.editing = Some(Edit { input, _sub: sub, typed });
        self.comp_sel = 0;
        self.ref_pick = None;
        cx.notify();
    }

    fn commit_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(e) = self.editing.take() {
            let v = e.input.read(cx).text();
            self.ref_pick = None;
            // 公式自动补齐右括号（同类）
            let v = if v.starts_with('=') {
                let open = v.matches('(').count() as i32 - v.matches(')').count() as i32;
                format!("{v}{}", ")".repeat(open.max(0) as usize))
            } else {
                v
            };
            self.set_value(self.cur, &v, cx);
            self.sync_formula(cx);
        }
    }

    pub(crate) fn completions(&self, cx: &App) -> Vec<&'static funcs::Func> {
        self.editing_text(cx).and_then(|t| funcs::completion_prefix(&t).map(|(_, p)| funcs::candidates(p))).unwrap_or_default()
    }

    /// 接受当前补全项：把函数名前缀替换成 `NAME(`。
    pub(crate) fn accept_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(e) = &self.editing else { return false };
        let text = e.input.read(cx).text();
        let Some((start, _)) = funcs::completion_prefix(&text) else { return false };
        let list = self.completions(cx);
        let Some(f) = list.get(self.comp_sel.min(list.len().saturating_sub(1))) else { return false };
        let new = format!("{}{}(", &text[..start], f.name);
        self.set_edit_text(&new, cx);
        window.focus(&self.editing.as_ref().unwrap().input.focus_handle(cx));
        true
    }

    fn set_edit_text(&mut self, s: &str, cx: &mut Context<Self>) {
        if let Some(e) = &self.editing {
            e.input.update(cx, |i, cx| {
                i.set_text(s, cx);
                i.collapse_to_end(cx);
            });
            self.formula.update(cx, |f, cx| {
                f.set_text(s, cx);
                f.collapse_to_end(cx);
            });
        }
        cx.notify();
    }

    /// 编辑公式时点选 / 拖选单元格：在末尾插入或替换刚插入的引用。
    fn pick_ref(&mut self, a: Cell, b: Cell, cx: &mut Context<Self>) {
        let Some(text) = self.editing_text(cx) else { return };
        let (start, _) = match self.ref_pick {
            Some(p) => p,
            None => (text.len(), a),
        };
        let start = start.min(text.len());
        let r = if a == b { addr(a.r, a.c) } else { range_text(&area(self.sheet, a.r, a.c, b.r, b.c)) };
        let new = format!("{}{}", &text[..start], r);
        self.ref_pick = Some((start, a));
        self.set_edit_text(&new, cx);
    }

    /// 自动换行的格子：按内容估算并撑高行高（开启自动换行后行高自适应）。
    fn autofit_row(&mut self, r: i32) {
        let mut need: f32 = 0.;
        {
            let m = self.model.borrow();
            let (_, uc) = (0, m.get_model().workbook.worksheet(self.sheet).map(|w| w.dimension().max_column).unwrap_or(1));
            for c in 1..=uc {
                let Ok(st) = m.get_cell_style(self.sheet, r, c) else { continue };
                if !st.alignment.as_ref().is_some_and(|a| a.wrap_text) {
                    continue;
                }
                let text = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let fpx = st.font.sz as f32 + 1.;
                let w = (m.get_column_width(self.sheet, c).unwrap_or(90.) as f32 - 12.).max(10.);
                let lines = text.split('\n').map(|l| (funcs::text_width_est(l, fpx) / w).ceil().max(1.)).sum::<f32>();
                need = need.max(lines * fpx * 1.35 + 8.);
            }
        }
        let cur = self.model.borrow().get_row_height(self.sheet, r).unwrap_or(25.) as f32;
        if need > cur + 0.5 {
            self.model.borrow_mut().set_rows_height(self.sheet, r, r, need as f64).ok();
        }
    }

    // ───────────── 样式 ─────────────

    fn style_path(&mut self, path: &str, value: &str, cx: &mut Context<Self>) {
        let a = self.sel_area();
        self.model.borrow_mut().update_range_style(&a, path, value).ok();
        self.changed(cx);
    }

    pub(crate) fn cur_style(&self) -> Style {
        self.model.borrow().get_cell_style(self.sheet, self.cur.r, self.cur.c).unwrap_or_default()
    }

    fn toggle_style(&mut self, path: &str, cx: &mut Context<Self>) {
        let s = self.cur_style();
        let on = match path {
            "font.b" => s.font.b,
            "font.i" => s.font.i,
            "font.u" => s.font.u,
            _ => s.font.strike,
        };
        self.style_path(path, if on { "false" } else { "true" }, cx);
    }

    /// 逐格改样式后作为一次撤销步骤写回（借 IronCalc 的「粘贴样式」）。
    fn restyle(&mut self, f: impl Fn(&mut Style), cx: &mut Context<Self>) {
        self.restyle_raw(f);
        let a = self.sel_used();
        for r in a.row..a.row + a.height.min(500) {
            self.autofit_row(r);
        }
        self.changed(cx);
    }

    /// 逐格改样式（不刷新）
    fn restyle_raw(&mut self, f: impl Fn(&mut Style)) {
        let a = self.sel_used();
        let styles: Vec<Vec<Style>> = {
            let m = self.model.borrow();
            (a.row..a.row + a.height)
                .map(|r| {
                    (a.column..a.column + a.width)
                        .map(|c| {
                            let mut s = m.get_model().get_style_for_cell(self.sheet, r, c).unwrap_or_default();
                            f(&mut s);
                            s
                        })
                        .collect()
                })
                .collect()
        };
        self.paste_styles(&a, &styles);
    }

    fn paste_styles(&mut self, a: &Area, styles: &[Vec<Style>]) {
        if styles.is_empty() || styles[0].is_empty() {
            return;
        }
        let mut m = self.model.borrow_mut();
        m.set_selected_sheet(self.sheet).ok();
        m.set_selected_cell(a.row, a.column).ok();
        m.set_selected_range(a.row, a.column, a.row + a.height - 1, a.column + a.width - 1).ok();
        m.on_paste_styles(styles).ok();
    }

    /// 水平对齐 / 换行：没有对齐信息的格子默认垂直居中（默认），写入时一并带上，避免变成底端对齐。
    fn set_align(&mut self, h: Option<&str>, wrap: Option<bool>, cx: &mut Context<Self>) {
        use ironcalc::base::types::{Alignment, HorizontalAlignment as H, VerticalAlignment as V};
        let h = h.map(|h| match h {
            "left" => H::Left,
            "center" => H::Center,
            "right" => H::Right,
            _ => H::General,
        });
        self.restyle(
            |s| {
                let a = s.alignment.get_or_insert_with(|| Alignment { vertical: V::Center, ..Default::default() });
                if let Some(h) = &h {
                    a.horizontal = h.clone();
                }
                if let Some(w) = wrap {
                    a.wrap_text = w;
                }
            },
            cx,
        );
    }

    fn set_border(&mut self, kind: &str, cx: &mut Context<Self>) {
        // 对角线：IronCalc 的 Border 有 diagonal_up / diagonal_down，逐格写样式
        if kind == "diag-down" || kind == "diag-up" {
            let item = serde_json::from_value::<ironcalc::base::types::BorderItem>(
                serde_json::json!({ "style": self.border_style_key(), "color": self.border_color }),
            )
            .ok();
            let down = kind == "diag-down";
            self.restyle(
                move |s| {
                    if down {
                        s.border.diagonal_down = true;
                    } else {
                        s.border.diagonal_up = true;
                    }
                    s.border.diagonal = item.clone();
                },
                cx,
            );
            return;
        }
        let ty = match kind {
            "all" => "All",
            "outer" | "thick" => "Outer",
            "inner" => "Inner",
            "center-h" => "CenterH",
            "center-v" => "CenterV",
            "top" => "Top",
            "bottom" => "Bottom",
            "left" => "Left",
            "right" => "Right",
            _ => "None",
        };
        let style = if kind == "thick" { "medium".to_string() } else { self.border_style_key() };
        let json = serde_json::json!({ "item": { "style": style, "color": self.border_color }, "type": ty });
        if let Ok(ba) = serde_json::from_value::<BorderArea>(json) {
            let a = self.sel_area();
            self.batch(|this| {
                this.model.borrow_mut().set_area_with_border(&a, &ba).ok();
                if ty == "None" {
                    this.restyle_raw(|s| {
                        s.border.diagonal_down = false;
                        s.border.diagonal_up = false;
                        s.border.diagonal = None;
                    });
                }
            });
            self.changed(cx);
        }
    }

    fn border_style_key(&self) -> String {
        match self.border_style.as_str() {
            "dashed" => "mediumdashed",
            "medium" | "thick" | "dotted" | "double" => self.border_style.as_str(),
            _ => "thin",
        }
        .to_string()
    }

    /// 小数位数 +1 / -1：在当前数字格式上增删一个 0。
    fn decimals(&mut self, inc: bool, cx: &mut Context<Self>) {
        let fmt = self.cur_style().num_fmt;
        let base = if fmt == "general" { "0".to_string() } else { fmt };
        let new = if inc {
            if let Some(i) = base.find('.') {
                format!("{}0{}", &base[..i + 1], &base[i + 1..])
            } else if let Some(i) = base.rfind('0') {
                format!("{}.0{}", &base[..=i], &base[i + 1..])
            } else {
                "0.0".into()
            }
        } else if let Some(i) = base.find('.') {
            let tail = &base[i + 1..];
            let zeros = tail.chars().take_while(|c| *c == '0').count();
            if zeros <= 1 {
                format!("{}{}", &base[..i], &tail[zeros..])
            } else {
                format!("{}.{}", &base[..i], &tail[1..])
            }
        } else {
            base
        };
        self.style_path("num_fmt", &new, cx);
    }

    // ───────────── 剪贴板 ─────────────

    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        let a = self.sel_used();
        let (tsv, html) = {
            let m = self.model.borrow();
            let mut tsv = String::new();
            let mut html = String::from("<meta charset=\"utf-8\"><table style=\"border-collapse:collapse\">");
            for r in a.row..a.row + a.height {
                html.push_str("<tr>");
                let mut row = vec![];
                for c in a.column..a.column + a.width {
                    let v = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
                    let st = m.get_cell_style(self.sheet, r, c).unwrap_or_default();
                    let mut css = String::new();
                    if st.font.b {
                        css.push_str("font-weight:bold;");
                    }
                    if st.font.i {
                        css.push_str("font-style:italic;");
                    }
                    let fc = m.resolve_color(&st.font.color);
                    if fc.len() == 7 {
                        css.push_str(&format!("color:{fc};"));
                    }
                    let bg = m.resolve_color(&st.fill.color);
                    if bg.len() == 7 {
                        css.push_str(&format!("background:{bg};"));
                    }
                    html.push_str(&format!("<td style=\"{css}\">{}</td>", v.replace('&', "&amp;").replace('<', "&lt;")));
                    row.push(v);
                }
                html.push_str("</tr>");
                tsv.push_str(&row.join("\t"));
                tsv.push('\n');
            }
            html.push_str("</table>");
            (tsv, html)
        };
        crate::clipboard::write(&tsv, Some(&html), None);
        // 站内剪贴板（保留公式与样式）
        let data = {
            let mut m = self.model.borrow_mut();
            m.set_selected_sheet(self.sheet).ok();
            m.set_selected_cell(a.row, a.column).ok();
            m.set_selected_range(a.row, a.column, a.row + a.height - 1, a.column + a.width - 1).ok();
            m.copy_to_clipboard().ok().and_then(|c| serde_json::to_value(&c).ok())
        };
        if let Some(v) = data {
            self.clip = Some(Clip {
                tsv: tsv.clone(),
                sheet: self.sheet,
                range: (a.row, a.column, a.row + a.height - 1, a.column + a.width - 1),
                data: v,
                cut,
                shown: true,
                sx: self.sx(),
            });
        }
        self.toast = Some(if cut { "已剪切".into() } else { "已复制".into() });
        cx.notify();
    }

    fn paste(&mut self, mode: &str, cx: &mut Context<Self>) {
        let sys = crate::clipboard::read();
        let internal = self.clip.as_ref().filter(|c| sys.text.as_deref().map(|t| t.trim_end() == c.tsv.trim_end()).unwrap_or(false));
        let cur = Cell { r: self.sel_area().row, c: self.sel_area().column };
        if let Some(clip) = internal {
            let data: Option<ClipboardData> = clip.data.get("data").and_then(|d| serde_json::from_value(d.clone()).ok());
            if mode == "format" {
                let styles: Vec<Vec<Style>> = (clip.range.0..=clip.range.2)
                    .map(|r| {
                        (clip.range.1..=clip.range.3)
                            .map(|c| {
                                clip.data.pointer(&format!("/data/{r}/{c}/style")).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
                            })
                            .collect()
                    })
                    .collect();
                let a = area(self.sheet, cur.r, cur.c, cur.r + styles.len() as i32 - 1, cur.c + styles[0].len() as i32 - 1);
                self.paste_styles(&a, &styles);
                self.changed(cx);
                return;
            }
            if mode == "normal" {
                if let Some(data) = data {
                    let (sheet, range, cut) = (clip.sheet, clip.range, clip.cut);
                    let src_sx = clip.sx.clone();
                    self.batch(|this| {
                        {
                            let mut m = this.model.borrow_mut();
                            m.set_selected_sheet(this.sheet).ok();
                            m.set_selected_cell(cur.r, cur.c).ok();
                            m.paste_from_clipboard(sheet, range, &data, cut).ok();
                        }
                        // 单元格对象（下拉 / 复选框 / 备注 / 图片…）一起粘贴；剪切时从原处移走
                        let (dr, dc) = (cur.r - range.0, cur.c - range.1);
                        let same = sheet == this.sheet;
                        this.edit_sx(|x| {
                            if cut && same {
                                x.cells.retain(|c| !(c.r >= range.0 && c.r <= range.2 && c.c >= range.1 && c.c <= range.3));
                                x.clear_dv(range.0, range.1, range.2, range.3);
                            }
                            x.copy_block(&src_sx, range.0, range.1, range.2, range.3, dr, dc);
                        });
                    });
                    if cut {
                        self.clip = None;
                    }
                    self.anchor = cur;
                    self.cur = Cell { r: cur.r + range.2 - range.0, c: cur.c + range.3 - range.1 };
                    self.changed(cx);
                    self.sync_formula(cx);
                    return;
                }
            }
        }
        // 外部内容：HTML 表格（Excel / 网页）优先转成 TSV
        let tsv = sys
            .html
            .as_deref()
            .and_then(|h| {
                let doc = scraper::Html::parse_document(h);
                let tr = scraper::Selector::parse("tr").ok()?;
                let td = scraper::Selector::parse("td, th").ok()?;
                let rows: Vec<String> = doc
                    .select(&tr)
                    .map(|r| r.select(&td).map(|c| c.text().collect::<String>().trim().replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t"))
                    .collect();
                (!rows.is_empty()).then(|| rows.join("\n"))
            })
            .or(sys.text);
        let Some(tsv) = tsv else { return };
        let tsv = tsv.trim_end_matches('\n');
        if tsv.contains('\t') || tsv.contains('\n') {
            let a = area(self.sheet, cur.r, cur.c, cur.r, cur.c);
            self.model.borrow_mut().paste_csv_string(&a, tsv).ok();
            let rows = tsv.lines().count() as i32;
            let cols = tsv.lines().map(|l| l.split('\t').count()).max().unwrap_or(1) as i32;
            self.anchor = cur;
            self.cur = Cell { r: cur.r + rows - 1, c: cur.c + cols - 1 };
        } else {
            // 单个值粘贴到整个选区（同类）
            let a = self.sel_used();
            let mut m = self.model.borrow_mut();
            for r in a.row..a.row + a.height {
                for c in a.column..a.column + a.width {
                    m.set_user_input(self.sheet, r, c, tsv).ok();
                }
            }
        }
        self.changed(cx);
        self.sync_formula(cx);
    }

    // ───────────── 填充 / 排序 / 求和 ─────────────

    /// 自动填充；单个源格里的中文序列由我们处理，其他交给 IronCalc（数字等差、日期、公式平移）。
    fn auto_fill(&mut self, a: &Area, to: Cell) {
        // 数据验证（下拉 / 复选框 / 日期）与单元格对象跟着填充，和填充值合成一步撤销
        let a2 = a;
        self.batch(|this| {
            this.auto_fill_values(a2, to);
            let down = to.r > a2.row + a2.height - 1;
            let src = this.sx();
            let (r0, c0, r1, c1) = (a2.row, a2.column, a2.row + a2.height - 1, a2.column + a2.width - 1);
            let srcs: Vec<ext::Dv> = src.dv.iter().filter(|d| d.r1 >= r0 && d.r0 <= r1 && d.c1 >= c0 && d.c0 <= c1).cloned().collect();
            if !srcs.is_empty() {
                // 填充目标区域（不含源区域）
                let (t0, t1) = if down { ((r1 + 1, c0), (to.r, c1)) } else { ((r0, c1 + 1), (r1, to.c)) };
                let (step, end) = if down { (a2.height, to.r) } else { (a2.width, to.c) };
                let mut copies = vec![];
                let mut k = step;
                while (if down { r0 } else { c0 }) + k <= end {
                    for d in &srcs {
                        // 源规则与源区域的交集，平移后再与目标区域求交
                        let (a0, b0, a1, b1) = (d.r0.max(r0), d.c0.max(c0), d.r1.min(r1), d.c1.min(c1));
                        let (dr, dc) = if down { (k, 0) } else { (0, k) };
                        let (n0, m0, n1, m1) = ((a0 + dr).max(t0.0), (b0 + dc).max(t0.1), (a1 + dr).min(t1.0), (b1 + dc).min(t1.1));
                        if n0 <= n1 && m0 <= m1 {
                            copies.push(ext::Dv { r0: n0, c0: m0, r1: n1, c1: m1, ..d.clone() });
                        }
                    }
                    k += step;
                }
                this.edit_sx(|x| {
                    x.clear_dv(t0.0, t0.1, t1.0, t1.1);
                    x.dv.extend(copies);
                });
            }
        });
    }

    fn auto_fill_values(&mut self, a: &Area, to: Cell) {
        let down = to.r > a.row + a.height - 1;
        let first = self.model.borrow().get_cell_content(self.sheet, a.row, a.column).unwrap_or_default();
        let mut m = self.model.borrow_mut();
        if a.width == 1 && a.height == 1 && funcs::series_next(&first, 1).is_some() {
            let count = if down { to.r - a.row } else { to.c - a.column };
            for k in 1..=count {
                let v = funcs::series_next(&first, k as usize).unwrap();
                let (r, c) = if down { (a.row + k, a.column) } else { (a.row, a.column + k) };
                m.set_user_input(self.sheet, r, c, &v).ok();
            }
            return;
        }
        if down {
            m.auto_fill_rows(a, to.r).ok();
        } else {
            m.auto_fill_columns(a, to.c).ok();
        }
    }

    fn fill_dir(&mut self, down: bool, cx: &mut Context<Self>) {
        let a = self.sel_used();
        if down && a.height > 1 {
            let src = area(self.sheet, a.row, a.column, a.row, a.column + a.width - 1);
            self.model.borrow_mut().auto_fill_rows(&src, a.row + a.height - 1).ok();
        } else if !down && a.width > 1 {
            let src = area(self.sheet, a.row, a.column, a.row + a.height - 1, a.column);
            self.model.borrow_mut().auto_fill_columns(&src, a.column + a.width - 1).ok();
        }
        self.changed(cx);
    }

    /// 排序：选区（多行时）或当前数据区，按当前列；首行有文字表头时视为表头（「数据含标题行」）。
    pub(crate) fn sort(&mut self, asc: bool, col: Option<i32>, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        let a = self.sel_area();
        let key_col = col.unwrap_or(self.cur.c);
        let (rows, cols) = if let Some(f) = self.filter.as_ref().filter(|f| f.sheet == self.sheet && key_col >= f.c0 && key_col <= f.c1) {
            ((f.head + 1, f.last), (f.c0, f.c1))
        } else if a.height > 1 && self.kind == SelKind::Cells {
            ((a.row, a.row + a.height - 1), (a.column, a.column + a.width - 1))
        } else {
            let (ur, uc) = self.used();
            let head_text = self.model.borrow().get_cell_type(self.sheet, 1, key_col).map(|t| matches!(t, CellType::Text)).unwrap_or(false);
            ((if head_text { 2 } else { 1 }, ur), (1, uc))
        };
        if rows.1 <= rows.0 {
            return;
        }
        let data: Vec<(Vec<String>, String, Option<f64>)> = {
            let m = self.model.borrow();
            (rows.0..=rows.1)
                .map(|r| {
                    let contents: Vec<String> = (cols.0..=cols.1).map(|c| m.get_cell_content(self.sheet, r, c).unwrap_or_default()).collect();
                    let shown = m.get_formatted_cell_value(self.sheet, r, key_col).unwrap_or_default();
                    let num = matches!(m.get_cell_type(self.sheet, r, key_col), Ok(CellType::Number))
                        .then(|| shown.replace([',', '¥', '$', '%'], "").parse::<f64>().ok())
                        .flatten();
                    (contents, shown, num)
                })
                .collect()
        };
        let mut data = data;
        data.sort_by(|x, y| {
            // 空值始终排在最后；数字在文字前
            let ord = match ((x.1.is_empty(), x.2, &x.1), (y.1.is_empty(), y.2, &y.1)) {
                ((true, ..), (true, ..)) => std::cmp::Ordering::Equal,
                ((true, ..), _) => return std::cmp::Ordering::Greater,
                (_, (true, ..)) => return std::cmp::Ordering::Less,
                ((_, Some(p), _), (_, Some(q), _)) => p.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal),
                ((_, Some(_), _), _) => std::cmp::Ordering::Less,
                (_, (_, Some(_), _)) => std::cmp::Ordering::Greater,
                ((_, _, a), (_, _, b)) => a.cmp(b),
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
        {
            let mut m = self.model.borrow_mut();
            m.pause_evaluation();
            for (i, row) in data.iter().enumerate() {
                for (j, v) in row.0.iter().enumerate() {
                    m.set_user_input(self.sheet, rows.0 + i as i32, cols.0 + j as i32, v).ok();
                }
            }
            m.resume_evaluation();
            m.evaluate();
        }
        self.apply_filter();
        self.toast = Some(format!("已按 {} 列{}排序", col_name(key_col), if asc { "升序" } else { "降序" }).into());
        self.changed(cx);
    }

    /// 自动求和（⌘⌥=）：在当前格写入对上方（或左侧）连续数字的 SUM，并进入编辑让你确认。
    fn autosum(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let numeric = |s: &Self, r: i32, c: i32| matches!(s.model.borrow().get_cell_type(s.sheet, r, c), Ok(CellType::Number)) && !s.is_empty(r, c);
        let a = self.sel_area();
        if self.kind == SelKind::Cells && a.height > 1 {
            // 选中一片数字：在下方一行逐列求和
            let r = a.row + a.height;
            for c in a.column..a.column + a.width {
                let f = format!("=SUM({}:{})", addr(a.row, c), addr(r - 1, c));
                self.model.borrow_mut().set_user_input(self.sheet, r, c, &f).ok();
            }
            self.changed(cx);
            return;
        }
        let Cell { r, c } = self.cur;
        let mut top = r;
        while top > 1 && numeric(self, top - 1, c) {
            top -= 1;
        }
        let f = if top < r {
            format!("=SUM({}:{})", addr(top, c), addr(r - 1, c))
        } else {
            let mut left = c;
            while left > 1 && numeric(self, r, left - 1) {
                left -= 1;
            }
            if left < c {
                format!("=SUM({}:{})", addr(r, left), addr(r, c - 1))
            } else {
                "=SUM(".into()
            }
        };
        self.start_edit(Some(f), window, cx);
        if let Some(e) = &mut self.editing {
            e.typed = false;
        }
    }

    // ───────────── 行列 ─────────────

    fn sel_rows(&self) -> (i32, i32) {
        let a = self.sel_area();
        (a.row, (a.row + a.height - 1).min(LAST_ROW))
    }
    fn sel_cols(&self) -> (i32, i32) {
        let a = self.sel_area();
        (a.column, (a.column + a.width - 1).min(LAST_COL))
    }

    fn rows_op(&mut self, what: &str, n: i32, cx: &mut Context<Self>) {
        let (r0, r1) = self.sel_rows();
        let (c0, c1) = self.sel_cols();
        let cnt_r = if self.kind == SelKind::Cols || self.kind == SelKind::All { 1 } else { r1 - r0 + 1 };
        let cnt_c = if self.kind == SelKind::Rows || self.kind == SelKind::All { 1 } else { c1 - c0 + 1 };
        let n_r = if n > 0 { n } else { cnt_r };
        let n_c = if n > 0 { n } else { cnt_c };
        let s = self.sheet;
        self.model.borrow_mut().begin_batch();
        // 扩展对象（下拉列表 / 备注 / 图片…）随行列插入删除平移
        match what {
            "row-above" => self.edit_sx(|x| x.shift(true, r0, n_r)),
            "row-below" => self.edit_sx(|x| x.shift(true, r1 + 1, n_r)),
            "col-left" => self.edit_sx(|x| x.shift(false, c0, n_c)),
            "col-right" => self.edit_sx(|x| x.shift(false, c1 + 1, n_c)),
            "del-rows" => self.edit_sx(|x| x.shift(true, r0, -cnt_r)),
            "del-cols" => self.edit_sx(|x| x.shift(false, c0, -cnt_c)),
            _ => {}
        }
        {
            let mut m = self.model.borrow_mut();
            match what {
                "row-above" => m.insert_rows(s, r0, n_r).ok(),
                "row-below" => m.insert_rows(s, r1 + 1, n_r).ok(),
                "col-left" => m.insert_columns(s, c0, n_c).ok(),
                "col-right" => m.insert_columns(s, c1 + 1, n_c).ok(),
                "del-rows" => m.delete_rows(s, r0, cnt_r).ok(),
                "del-cols" => m.delete_columns(s, c0, cnt_c).ok(),
                "hide-rows" => m.set_rows_hidden(s, r0, r0 + cnt_r - 1, true).ok(),
                "hide-cols" => m.set_columns_hidden(s, c0, c0 + cnt_c - 1, true).ok(),
                "unhide-rows" => {
                    // 选中跨过隐藏行的区域后取消隐藏；只选一行时取消它上下相邻的隐藏行
                    let (a, b) = if cnt_r > 1 { (r0, r0 + cnt_r - 1) } else { ((r0 - 50).max(1), r0 + 50) };
                    m.set_rows_hidden(s, a, b, false).ok()
                }
                "unhide-cols" => {
                    let (a, b) = if cnt_c > 1 { (c0, c0 + cnt_c - 1) } else { ((c0 - 20).max(1), c0 + 20) };
                    m.set_columns_hidden(s, a, b, false).ok()
                }
                _ => None,
            };
            m.end_batch();
        }
        match what {
            "row-below" => {
                self.anchor = Cell { r: r1 + 1, c: self.anchor.c };
                self.cur = Cell { r: r1 + n_r, c: self.cur.c };
            }
            "col-right" => {
                self.anchor = Cell { r: self.anchor.r, c: c1 + 1 };
                self.cur = Cell { r: self.cur.r, c: c1 + n_c };
            }
            "del-rows" | "del-cols" | "hide-rows" | "hide-cols" => {
                self.kind = SelKind::Cells;
                self.cur = Cell { r: r0.min(LAST_ROW), c: c0.min(LAST_COL) };
                self.anchor = self.cur;
            }
            _ => {}
        }
        self.changed(cx);
        self.sync_formula(cx);
    }

    /// 冻结：`rows`/`cols` 为冻结的行数 / 列数。
    fn freeze(&mut self, rows: Option<i32>, cols: Option<i32>, cx: &mut Context<Self>) {
        {
            let mut m = self.model.borrow_mut();
            if let Some(r) = rows {
                m.set_frozen_rows_count(self.sheet, r.max(0)).ok();
            }
            if let Some(c) = cols {
                m.set_frozen_columns_count(self.sheet, c.max(0)).ok();
            }
        }
        self.scroll = (0., 0.);
        self.changed(cx);
    }

    /// 列宽自适应（双击列边框）。
    fn autofit_col(&mut self, c: i32, cx: &mut Context<Self>) {
        let w = {
            let m = self.model.borrow();
            let (ur, _) = self.used();
            let mut w: f32 = 40.;
            for r in 1..=ur.min(5000) {
                let t = m.get_formatted_cell_value(self.sheet, r, c).unwrap_or_default();
                if t.is_empty() {
                    continue;
                }
                let sz = m.get_cell_style(self.sheet, r, c).map(|s| s.font.sz).unwrap_or(12) as f32 + 1.;
                w = w.max(funcs::text_width_est(&t, sz) + 16.);
            }
            w.min(600.)
        };
        self.model.borrow_mut().set_columns_width(self.sheet, c, c, w as f64).ok();
        self.changed(cx);
    }

    // ───────────── 筛选 ─────────────

    /// 打开 / 关闭筛选：范围是选区（多行时）或当前格所在的连续数据区。
    fn toggle_filter(&mut self, cx: &mut Context<Self>) {
        if self.filter.as_ref().is_some_and(|f| f.sheet == self.sheet) {
            self.filter = None;
            self.filter_ver += 1;
            self.toast = Some("已取消筛选".into());
            cx.notify();
            return;
        }
        let a = self.sel_area();
        let (ur, uc) = self.used();
        let (head, last, c0, c1) = if self.kind == SelKind::Cells && a.height > 1 {
            (a.row, a.row + a.height - 1, a.column, a.column + a.width - 1)
        } else {
            // 当前数据区：从当前格向四周扩展到空行 / 空列
            let (mut r0, mut r1, mut c0, mut c1) = (self.cur.r, self.cur.r, self.cur.c, self.cur.c);
            let row_empty = |s: &Self, r: i32, c0: i32, c1: i32| (c0..=c1).all(|c| s.is_empty(r, c));
            let col_empty = |s: &Self, c: i32, r0: i32, r1: i32| (r0..=r1).all(|r| s.is_empty(r, c));
            loop {
                let mut grew = false;
                if r0 > 1 && !row_empty(self, r0 - 1, c0, c1) {
                    r0 -= 1;
                    grew = true;
                }
                if r1 < ur && !row_empty(self, r1 + 1, c0, c1) {
                    r1 += 1;
                    grew = true;
                }
                if c0 > 1 && !col_empty(self, c0 - 1, r0, r1) {
                    c0 -= 1;
                    grew = true;
                }
                if c1 < uc && !col_empty(self, c1 + 1, r0, r1) {
                    c1 += 1;
                    grew = true;
                }
                if !grew {
                    break;
                }
            }
            (r0, r1.max(r0 + 1), c0, c1)
        };
        self.filter = Some(Filter { sheet: self.sheet, head, last, c0, c1, excl: HashMap::new(), hidden: HashSet::new() });
        self.filter_ver += 1;
        self.toast = Some(format!("已为 {} 开启筛选", range_text(&area(self.sheet, head, c0, last, c1))).into());
        cx.notify();
    }

    pub(crate) fn filter_values(&self, col: i32) -> Vec<(String, usize)> {
        let Some(f) = &self.filter else { return vec![] };
        let m = self.model.borrow();
        let mut counts: Vec<(String, usize)> = vec![];
        let mut idx: HashMap<String, usize> = HashMap::new();
        for r in f.head + 1..=f.last {
            let v = m.get_formatted_cell_value(self.sheet, r, col).unwrap_or_default();
            match idx.get(&v) {
                Some(&i) => counts[i].1 += 1,
                None => {
                    idx.insert(v.clone(), counts.len());
                    counts.push((v, 1));
                }
            }
        }
        counts.sort_by(|a, b| match (a.0.parse::<f64>(), b.0.parse::<f64>()) {
            (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            _ => (a.0.is_empty(), &a.0).cmp(&(b.0.is_empty(), &b.0)),
        });
        counts
    }

    fn apply_filter(&mut self) {
        let Some(f) = &self.filter else { return };
        let mut hidden = HashSet::new();
        {
            let m = self.model.borrow();
            for r in f.head + 1..=f.last {
                for (c, ex) in &f.excl {
                    if ex.contains(&m.get_formatted_cell_value(f.sheet, r, *c).unwrap_or_default()) {
                        hidden.insert(r);
                        break;
                    }
                }
            }
        }
        if let Some(f) = &mut self.filter {
            f.hidden = hidden;
        }
        self.filter_ver += 1;
    }

    pub(crate) fn filter_toggle_value(&mut self, col: i32, v: Option<String>, cx: &mut Context<Self>) {
        let all: Vec<String> = self.filter_values(col).into_iter().map(|x| x.0).collect();
        if let Some(f) = &mut self.filter {
            let ex = f.excl.entry(col).or_default();
            match v {
                // None：全选 / 取消全选
                None => {
                    if ex.is_empty() {
                        ex.extend(all);
                    } else {
                        ex.clear();
                    }
                }
                Some(v) => {
                    if !ex.remove(&v) {
                        ex.insert(v);
                    }
                }
            }
            if ex.is_empty() {
                f.excl.remove(&col);
            }
        }
        self.apply_filter();
        self.save_view(cx);
        cx.notify();
    }

    // ───────────── 查找替换 ─────────────

    fn open_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.pop = None;
        if let Some(f) = &mut self.find {
            f.replace = f.replace || replace;
            f.compact = false;
            let q = f.q.clone();
            window.focus(&q.focus_handle(cx));
            cx.notify();
            return;
        }
        let q = cx.new(|cx| LineInput::new("在当前工作表中查找", cx));
        let rep = cx.new(|cx| LineInput::new("", cx));
        let s1 = cx.subscribe_in(&q, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Changed => this.refresh_find(cx),
            InputEvent::Submit { shift } => this.find_step(if *shift { -1 } else { 1 }, cx),
            InputEvent::Cancel => this.close_find(window, cx),
            _ => {}
        });
        let s2 = cx.subscribe_in(&rep, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => this.replace_one(cx),
            InputEvent::Cancel => this.close_find(window, cx),
            _ => {}
        });
        window.focus(&q.focus_handle(cx));
        // 打开时如果选中了一片区域，范围默认为选区
        let a = self.sel_area();
        let scope = if self.kind == SelKind::Cells && (a.width > 1 || a.height > 1) { 2 } else { 0 };
        self.find = Some(Find {
            q,
            rep,
            replace,
            case: false,
            whole: false,
            matches: vec![],
            idx: 0,
            _subs: vec![s1, s2],
            scope,
            formulas: false,
            compact: false,
            scope_open: false,
            area: (a.row, a.column, a.row + a.height - 1, a.column + a.width - 1),
        });
        cx.notify();
    }

    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn find_hit(f: &Find, q: &str, v: &str) -> bool {
        if q.is_empty() {
            return false;
        }
        if f.case {
            if f.whole {
                v == q
            } else {
                v.contains(q)
            }
        } else {
            let (v, q) = (v.to_lowercase(), q.to_lowercase());
            if f.whole {
                v == q
            } else {
                v.contains(&q)
            }
        }
    }

    /// 重新查找：范围为当前工作表 / 全部工作表 / 选区；勾选「搜索范围包括公式」时也匹配公式文本。
    pub(crate) fn refresh_find(&mut self, cx: &mut Context<Self>) {
        self.rebuild_geometry();
        let Some(f) = &self.find else { return };
        let q = f.q.read(cx).text();
        let sheets: Vec<u32> = if f.scope == 1 { (0..self.model.borrow().get_worksheets_properties().len() as u32).collect() } else { vec![self.sheet] };
        let mut matches = vec![];
        {
            let m = self.model.borrow();
            for s in sheets {
                let (ur, uc) = m.get_model().workbook.worksheet(s).map(|w| (w.dimension().max_row, w.dimension().max_column)).unwrap_or((1, 1));
                let (r0, c0, r1, c1) = if f.scope == 2 { f.area } else { (1, 1, ur, uc) };
                for r in r0..=r1.min(ur) {
                    if s == self.sheet && self.is_hidden_row(r) {
                        continue;
                    }
                    for c in c0..=c1.min(uc) {
                        if s == self.sheet && self.is_hidden_col(c) {
                            continue;
                        }
                        let v = m.get_formatted_cell_value(s, r, c).unwrap_or_default();
                        let hit = Self::find_hit(f, &q, &v)
                            || (f.formulas && m.get_cell_content(s, r, c).is_ok_and(|t| t.starts_with('=') && Self::find_hit(f, &q, &t)));
                        if hit {
                            matches.push((s, Cell { r, c }));
                        }
                    }
                }
            }
        }
        let (cur, sh) = (self.cur, self.sheet);
        if let Some(f) = &mut self.find {
            f.idx = matches.iter().position(|(s, m)| (*s, m.r, m.c) >= (sh, cur.r, cur.c)).unwrap_or(0);
            f.matches = matches;
        }
        cx.notify();
    }

    pub(crate) fn find_step(&mut self, d: i32, cx: &mut Context<Self>) {
        let Some(f) = &mut self.find else { return };
        if f.matches.is_empty() {
            return;
        }
        let n = f.matches.len() as i32;
        let at = f.matches.get(f.idx).copied();
        // 第一次回车停在当前命中项上
        f.idx = if at == Some((self.sheet, self.cur)) { ((f.idx as i32 + d).rem_euclid(n)) as usize } else { f.idx };
        let (s, c) = f.matches[f.idx];
        if s != self.sheet {
            // 跨工作表跳转（不重新查找，保留当前序号）
            self.commit_edit(cx);
            self.sheet = s;
            self.scroll = (0., 0.);
            self.geo_key.0 = u64::MAX;
            self.rebuild_geometry();
        }
        self.anchor = c;
        self.cur = c;
        self.kind = SelKind::Cells;
        self.ensure_visible();
        self.sync_formula(cx);
        cx.notify();
    }

    pub(crate) fn replace_one(&mut self, cx: &mut Context<Self>) {
        let Some(f) = &self.find else { return };
        let (q, rep) = (f.q.read(cx).text(), f.rep.read(cx).text());
        if q.is_empty() {
            return;
        }
        let Some(&(s, c)) = f.matches.get(f.idx) else { return };
        if c != self.cur || s != self.sheet {
            self.find_step(1, cx);
            return;
        }
        let content = self.model.borrow().get_cell_content(self.sheet, c.r, c.c).unwrap_or_default();
        let new = Self::replace_str(&content, &q, &rep, f.case, f.whole);
        self.set_value(c, &new, cx);
        self.refresh_find(cx);
        self.find_step(1, cx);
    }

    pub(crate) fn replace_all(&mut self, cx: &mut Context<Self>) {
        let Some(f) = &self.find else { return };
        let (q, rep) = (f.q.read(cx).text(), f.rep.read(cx).text());
        let (case, whole) = (f.case, f.whole);
        let list = f.matches.clone();
        if q.is_empty() || list.is_empty() {
            return;
        }
        self.batch(|this| {
            let mut m = this.model.borrow_mut();
            m.pause_evaluation();
            for (s, c) in &list {
                let content = m.get_cell_content(*s, c.r, c.c).unwrap_or_default();
                m.set_user_input(*s, c.r, c.c, &Self::replace_str(&content, &q, &rep, case, whole)).ok();
            }
            m.resume_evaluation();
            m.evaluate();
        });
        self.toast = Some(format!("已替换 {} 处", list.len()).into());
        self.changed(cx);
    }

    fn replace_str(s: &str, q: &str, rep: &str, case: bool, whole: bool) -> String {
        if whole {
            return rep.to_string();
        }
        if case {
            return s.replace(q, rep);
        }
        // 不区分大小写替换
        let (ls, lq) = (s.to_lowercase(), q.to_lowercase());
        if ls.len() != s.len() {
            return s.replace(q, rep);
        }
        let mut out = String::new();
        let mut i = 0;
        while let Some(p) = ls[i..].find(&lq) {
            out.push_str(&s[i..i + p]);
            out.push_str(rep);
            i += p + q.len();
        }
        out.push_str(&s[i..]);
        out
    }

    // ───────────── 弹出层 / 输入框 ─────────────

    pub(crate) fn open_pop(&mut self, kind: Pop, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.pop.as_ref().is_some_and(|p| p.kind == kind) {
            self.pop = None;
        } else {
            self.commit_edit(cx);
            self.menu_path.clear();
            self.picker = None;
            self.border_sub = 0;
            self.menu_kind = match kind {
                Pop::Num => menu::MenuKind::Num,
                Pop::Cond => menu::MenuKind::Cond,
                Pop::Freeze => menu::MenuKind::Freeze,
                Pop::Formula => menu::MenuKind::Formula,
                Pop::FilterMenu => menu::MenuKind::Filter,
                Pop::Ctx | Pop::RowCtx | Pop::ColCtx => menu::MenuKind::Ctx(kind),
                _ => menu::MenuKind::Main,
            };
            if kind == Pop::Menu {
                // 菜单里的行高 / 列宽输入框显示当前值
                let (rh, cw) = {
                    let m = self.model.borrow();
                    (m.get_row_height(self.sheet, self.cur.r).unwrap_or(25.).round(), m.get_column_width(self.sheet, self.cur.c).unwrap_or(90.).round())
                };
                self.menu_inputs.rh.update(cx, |i, cx| i.set_text(&rh.to_string(), cx));
                self.menu_inputs.cw.update(cx, |i, cx| i.set_text(&cw.to_string(), cx));
            }
            self.pop = Some(Popup { kind, at });
        }
        cx.notify();
    }

    pub(crate) fn open_prompt(&mut self, kind: PromptKind, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.pop = None;
        let init = match kind {
            PromptKind::Cf(_) => String::new(),
            PromptKind::NumFmt => self.cur_style().num_fmt,
        };
        let input = cx.new(|cx| LineInput::new("", cx));
        input.update(cx, |i, cx| i.set_text(&init, cx));
        let sub = cx.subscribe_in(&input, window, |this, input, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } => {
                let v = input.read(cx).text();
                this.submit_prompt(&v, cx);
                window.focus(&this.focus);
            }
            InputEvent::Cancel => {
                this.prompt = None;
                window.focus(&this.focus);
                cx.notify();
            }
            _ => {}
        });
        window.focus(&input.focus_handle(cx));
        self.prompt = Some(Prompt { kind, input, at, _sub: sub });
        cx.notify();
    }

    pub(crate) fn submit_prompt(&mut self, v: &str, cx: &mut Context<Self>) {
        let Some(p) = self.prompt.take() else { return };
        match p.kind {
            PromptKind::Cf(rule) => {
                if let Some(r) = Self::cf_rule(rule, v.trim()).filter(|_| !v.trim().is_empty()) {
                    self.add_cf(r, cx);
                }
            }
            PromptKind::NumFmt => {
                if !v.trim().is_empty() {
                    self.style_path("num_fmt", v.trim(), cx);
                }
            }
        }
        self.changed(cx);
    }

    // ───────────── 命令 ─────────────

    /// 内嵌模式：数据区（至少 2 行 3 列，新建表格的默认大小）的像素尺寸，不含表头。
    pub(crate) fn embed_content(&mut self) -> (f32, f32) {
        self.rebuild_geometry();
        let (ur, uc) = self.used();
        let rows = ur.max(self.cur.r).max(self.anchor.r).max(2);
        let cols = uc.max(self.cur.c).max(self.anchor.c).max(3);
        (self.col_left(cols + 1), self.row_top(rows + 1))
    }

    pub fn new_embedded(obj_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut s = Self::new(obj_id, window, cx);
        s.embedded = true;
        s
    }

    pub(crate) fn switch_sheet(&mut self, s: u32, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        self.sheet = s;
        self.cur = Cell { r: 1, c: 1 };
        self.anchor = self.cur;
        self.kind = SelKind::Cells;
        self.scroll = (0., 0.);
        self.geo_key.0 = u64::MAX;
        if self.find.is_some() {
            self.refresh_find(cx);
        }
        self.sync_formula(cx);
        cx.notify();
    }

    pub(crate) fn start_rename(&mut self, i: u32, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.model.borrow().get_worksheets_properties().get(i as usize).map(|s| s.name.clone()).unwrap_or_default();
        let input = cx.new(|cx| LineInput::new("工作表名", cx));
        input.update(cx, |inp, cx| inp.set_text(&name, cx));
        let sub = cx.subscribe_in(&input, window, move |this, input, ev: &InputEvent, window, cx| match ev {
            InputEvent::Submit { .. } | InputEvent::Cancel => {
                let v = input.read(cx).text();
                if !v.trim().is_empty() {
                    this.model.borrow_mut().rename_sheet(i, v.trim()).ok();
                    this.changed(cx);
                }
                this.rename = None;
                window.focus(&this.focus);
                cx.notify();
            }
            _ => {}
        });
        window.focus(&input.focus_handle(cx));
        self.rename = Some((i, input, sub));
        cx.notify();
    }

    fn import(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("导入 xlsx / CSV".into()),
            ..Default::default()
        });
        cx.spawn(async move |this: WeakEntity<SheetEditor>, cx: &mut AsyncApp| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(p) = paths.first().cloned() else { return };
            this.update(cx, |this, cx| {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if ext == "xlsx" {
                    if let Ok(model) = ironcalc::import::load_from_xlsx(&p.to_string_lossy(), "en", "UTC", "en") {
                        *this.model.borrow_mut() = ironcalc::base::UserModel::from_model(model);
                        this.sheet = 0;
                        this.filter = None;
                    }
                } else if let Ok(text) = std::fs::read_to_string(&p) {
                    let tsv = if ext == "csv" { funcs::csv_to_tsv(&text) } else { text };
                    let a = area(this.sheet, this.cur.r, this.cur.c, this.cur.r, this.cur.c);
                    this.model.borrow_mut().paste_csv_string(&a, &tsv).ok();
                }
                this.changed(cx);
            })
            .ok();
        })
        .detach();
    }

    fn export(&mut self, xlsx: bool, cx: &mut Context<Self>) {
        let name = if xlsx { "表格.xlsx" } else { "表格.csv" };
        let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
        let rx = cx.prompt_for_new_path(&dir, Some(name));
        let model = self.model.clone();
        let sheet = self.sheet;
        let (rows, cols) = self.used();
        cx.spawn(async move |_: WeakEntity<SheetEditor>, cx: &mut AsyncApp| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let ok = if xlsx {
                let _ = std::fs::remove_file(&path);
                ironcalc::export::save_to_xlsx(model.borrow().get_model(), &path.to_string_lossy()).is_ok()
            } else {
                let m = model.borrow();
                let mut out = String::new();
                for r in 1..=rows {
                    let mut row: Vec<String> = (1..=cols).map(|c| m.get_formatted_cell_value(sheet, r, c).unwrap_or_default()).collect();
                    while row.last().is_some_and(|v| v.is_empty()) {
                        row.pop();
                    }
                    out.push_str(
                        &row.iter()
                            .map(|v| if v.contains([',', '"', '\n']) { format!("\"{}\"", v.replace('"', "\"\"")) } else { v.clone() })
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                    out.push('\n');
                }
                std::fs::write(&path, out).is_ok()
            };
            if ok {
                cx.update(|cx| cx.reveal_path(&path)).ok();
            }
        })
        .detach();
    }

    // ───────────── 鼠标 ─────────────

    /// 列头 / 行头分隔线命中：(是否列, 分隔线左侧的列号 / 上方的行号)。
    fn resize_hit(&self, lx: f32, ly: f32) -> Option<(bool, i32)> {
        if ly < HEAD_H && lx >= HEAD_W {
            let cell = self.cell_at_local(lx, ly + HEAD_H);
            return [cell.c - 1, cell.c]
                .into_iter()
                .filter(|c| *c >= 1)
                .find(|&c| (self.lx(self.col_left(c + 1), c <= self.fc) - lx).abs() < 5.)
                .map(|c| (true, c));
        }
        if lx < HEAD_W && ly >= HEAD_H {
            let cell = self.cell_at_local(lx + HEAD_W, ly);
            return [cell.r - 1, cell.r]
                .into_iter()
                .filter(|r| *r >= 1)
                .find(|&r| (self.ly(self.row_top(r + 1), r <= self.fr) - ly).abs() < 4.)
                .map(|r| (false, r));
        }
        None
    }

    /// 分隔线在网格局部坐标中的位置（列：x；行：y）。
    pub(crate) fn resize_edge(&self, col: bool, i: i32) -> f32 {
        if col {
            self.lx(self.col_left(i + 1), i <= self.fc)
        } else {
            self.ly(self.row_top(i + 1), i <= self.fr)
        }
    }

    /// 正在拖动或悬停的分隔线：(是否列, 列号 / 行号, 是否拖动中)。
    pub(crate) fn resize_state(&self) -> Option<(bool, i32, bool)> {
        match &self.drag {
            Some(SDrag::ColResize { col, .. }) => Some((true, *col, true)),
            Some(SDrag::RowResize { row, .. }) => Some((false, *row, true)),
            _ => self.resize_hover.map(|(c, i)| (c, i, false)),
        }
    }

    /// 点选别处后收起复制的蚂蚁线；剪切则直接取消（与常见表格产品一致）。
    fn hide_marquee(&mut self) {
        if self.clip.as_ref().is_some_and(|c| c.cut) {
            self.clip = None;
        } else if let Some(c) = &mut self.clip {
            c.shown = false;
        }
    }

    fn right_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.commit_edit(cx);
        let (lx, ly) = self.local(ev.position);
        self.rebuild_geometry();
        let a = self.sel_area();
        let inside = |r: i32, c: i32| r >= a.row && r < a.row + a.height && c >= a.column && c < a.column + a.width;
        let kind = if ly < HEAD_H && lx >= HEAD_W {
            let cell = self.cell_at_local(lx, ly + HEAD_H);
            if !(self.kind == SelKind::Cols && inside(1, cell.c)) {
                self.select(Cell { r: 1, c: cell.c }, Cell { r: 1, c: cell.c }, SelKind::Cols, cx);
            }
            Pop::ColCtx
        } else if lx < HEAD_W && ly >= HEAD_H {
            let cell = self.cell_at_local(lx + HEAD_W, ly);
            if !(self.kind == SelKind::Rows && inside(cell.r, 1)) {
                self.select(Cell { r: cell.r, c: 1 }, Cell { r: cell.r, c: 1 }, SelKind::Rows, cx);
            }
            Pop::RowCtx
        } else {
            let cell = self.cell_at_local(lx, ly);
            if !inside(cell.r, cell.c) {
                self.select(cell, cell, SelKind::Cells, cx);
            }
            Pop::Ctx
        };
        self.pop = Some(Popup { kind, at: ev.position });
        self.menu_path.clear();
        self.menu_kind = menu::MenuKind::Ctx(kind);
        // 行头 / 列头菜单里的行高 / 列宽输入框显示当前值
        let (rh, cw) = {
            let m = self.model.borrow();
            (m.get_row_height(self.sheet, self.cur.r).unwrap_or(25.).round(), m.get_column_width(self.sheet, self.cur.c).unwrap_or(90.).round())
        };
        self.menu_inputs.rh.update(cx, |i, cx| i.set_text(&rh.to_string(), cx));
        self.menu_inputs.cw.update(cx, |i, cx| i.set_text(&cw.to_string(), cx));
        cx.notify();
    }
}
