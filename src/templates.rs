//! 文档模板：内置模板（Markdown 书写）+ 用户「另存为模板」的文档。
//! 支持变量：{{今天}} {{本周}} {{标题}} {{时间}} {{年}} {{月}}。

use crate::model::{Block, BlockKind, DocContent, RichText};
use chrono::{Datelike, Local};
use std::sync::Arc;

pub struct Builtin {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: u32,
    pub desc: &'static str,
    pub md: &'static str,
}

pub const BUILTIN: &[Builtin] = &[
    Builtin {
        name: "周报",
        icon: "calendar",
        color: 0x3b6cf6,
        desc: "本周进展、下周计划、风险与求助",
        md: "# {{本周}} 周报\n\n## 本周进展\n\n- [ ] \n\n## 关键数据\n\n| 指标 | 本周 | 上周 | 变化 |\n| --- | --- | --- | --- |\n| | | | |\n\n## 下周计划\n\n1. \n\n## 风险与求助\n\n- \n",
    },
    Builtin {
        name: "会议纪要",
        icon: "users",
        color: 0x2ea121,
        desc: "议题、结论、待办与负责人",
        md: "# 会议纪要 {{今天}}\n\n**时间**：{{今天}} {{时间}}　**地点**：　**参会人**：\n\n## 议题\n\n1. \n\n## 讨论记录\n\n- \n\n## 结论\n\n- \n\n## 待办事项\n\n- [ ] 事项 — 负责人 — 截止日期\n",
    },
    Builtin {
        name: "OKR",
        icon: "target",
        color: 0xe5484d,
        desc: "目标与关键结果，附进度评分",
        md: "# {{年}} 年 OKR\n\n## O1：\n\n| 关键结果 | 负责人 | 进度 | 评分 |\n| --- | --- | --- | --- |\n| KR1： | | 0% | |\n| KR2： | | 0% | |\n| KR3： | | 0% | |\n\n## O2：\n\n| 关键结果 | 负责人 | 进度 | 评分 |\n| --- | --- | --- | --- |\n| KR1： | | 0% | |\n| KR2： | | 0% | |\n\n## 复盘\n\n- 做得好的：\n- 待改进的：\n",
    },
    Builtin {
        name: "读书笔记",
        icon: "book",
        color: 0xf5a339,
        desc: "书籍信息、摘录、思考与行动",
        md: "# 《》读书笔记\n\n**作者**：　**读完日期**：{{今天}}　**评分**：⭐⭐⭐⭐⭐\n\n## 一句话总结\n\n> \n\n## 精彩摘录\n\n> \n\n## 我的思考\n\n- \n\n## 可以马上行动的\n\n- [ ] \n",
    },
    Builtin {
        name: "项目计划",
        icon: "workflow",
        color: 0x8b5cf6,
        desc: "背景目标、里程碑甘特图、分工",
        md: "# 项目计划\n\n## 背景与目标\n\n\n## 里程碑\n\n```mermaid\ngantt\n    title 项目里程碑\n    dateFormat YYYY-MM-DD\n    section 设计\n    需求评审 :a1, {{今天}}, 5d\n    方案设计 :a2, after a1, 7d\n    section 开发\n    开发实现 :b1, after a2, 14d\n    联调测试 :b2, after b1, 7d\n    section 发布\n    上线 :milestone, after b2, 0d\n```\n\n## 分工\n\n| 模块 | 负责人 | 状态 |\n| --- | --- | --- |\n| | | 未开始 |\n\n## 风险\n\n- \n",
    },
    Builtin {
        name: "Bug 报告",
        icon: "bug",
        color: 0xec4899,
        desc: "环境、复现步骤、期望与实际",
        md: "# Bug：\n\n**发现日期**：{{今天}}　**严重程度**：P2　**状态**：待处理\n\n## 环境\n\n- 版本：\n- 系统：\n\n## 复现步骤\n\n1. \n2. \n3. \n\n## 期望结果\n\n\n## 实际结果\n\n\n## 日志 / 截图\n\n```\n\n```\n",
    },
    Builtin {
        name: "日记",
        icon: "feather",
        color: 0x14b8a6,
        desc: "今日三件事、感恩与明日计划",
        md: "# {{今天}} 日记\n\n## 今天最重要的三件事\n\n- [ ] \n- [ ] \n- [ ] \n\n## 记录\n\n\n## 感恩的事\n\n- \n\n## 明天\n\n- \n",
    },
];

/// 替换模板变量。
pub fn fill_vars(s: &str, title: &str) -> String {
    let now = Local::now();
    let monday = now.date_naive() - chrono::Duration::days(now.weekday().num_days_from_monday() as i64);
    let week = format!("{}-{:02}-{:02} ~ {}", monday.year(), monday.month(), monday.day(), (monday + chrono::Duration::days(6)).format("%m-%d"));
    s.replace("{{今天}}", &now.format("%Y-%m-%d").to_string())
        .replace("{{本周}}", &week)
        .replace("{{时间}}", &now.format("%H:%M").to_string())
        .replace("{{年}}", &now.year().to_string())
        .replace("{{月}}", &now.month().to_string())
        .replace("{{标题}}", title)
}

/// 内置模板 → (标题, 正文块)。Markdown 第一行 `# xxx` 作为标题。
pub fn builtin_blocks(t: &Builtin, cur_title: &str) -> (String, Vec<Block>) {
    let md = fill_vars(t.md, cur_title);
    let (first, rest) = md.split_once('\n').unwrap_or((&md, ""));
    let title = first.trim_start_matches('#').trim().to_string();
    // 标题后面紧跟标题（或位于末尾）时补一个空段落，方便直接填写
    let mut fixed: Vec<Block> = vec![];
    for b in crate::convert::md_to_blocks(rest) {
        let prev_heading = fixed.last().is_some_and(|p: &Block| matches!(p.kind, BlockKind::Heading { .. }));
        if prev_heading && matches!(b.kind, BlockKind::Heading { .. }) {
            fixed.push(Block::text(""));
        }
        fixed.push(b);
    }
    if fixed.last().is_some_and(|p| matches!(p.kind, BlockKind::Heading { .. })) {
        fixed.push(Block::text(""));
    }
    (title, fixed)
}

/// 用户模板（存的是整篇文档 JSON，块 0 为标题）→ (标题, 正文块)。
pub fn user_blocks(json: &str, cur_title: &str) -> (String, Vec<Block>) {
    let filled = fill_vars(json, cur_title);
    let mut blocks = crate::model::blocks_from_json(&filled).unwrap_or_default();
    let title = if blocks.first().is_some_and(|b| matches!(b.kind, BlockKind::Title)) { blocks.remove(0).text.text } else { String::new() };
    (title, blocks)
}

pub fn to_doc(title: String, mut blocks: Vec<Block>) -> DocContent {
    if blocks.is_empty() {
        blocks.push(Block::text(""));
    }
    let mut all = vec![Arc::new(Block::new(BlockKind::Title, RichText::plain(title)))];
    all.extend(blocks.into_iter().map(Arc::new));
    DocContent { blocks: all }
}
