//! PlantUML → Mermaid（尽力而为）。PlantUML 官方实现依赖 Java，不适合内置；
//! 常用的几类图在语法上和 Mermaid 很接近，转换后交给内置的 Mermaid 渲染与布局：
//! - 时序图 → sequenceDiagram
//! - 活动图（新语法：start / :动作; / if / while / repeat / fork）→ flowchart
//! - 用例图 → flowchart（参与者方框、用例圆角框、rectangle 转子图）
//! - 类图 → classDiagram；状态图 → stateDiagram；ER 图（entity）→ erDiagram
//! - 思维导图（@startmindmap）→ mindmap（画板里直接转成可编辑的思维导图节点）

/// PlantUML 的图类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumlKind {
    Sequence,
    Activity,
    Usecase,
    Component,
    Class,
    State,
    Er,
    Mindmap,
}

impl PumlKind {
    pub fn label(self) -> &'static str {
        match self {
            PumlKind::Sequence => "时序图",
            PumlKind::Activity => "活动图",
            PumlKind::Usecase => "用例图",
            PumlKind::Component => "组件图",
            PumlKind::Class => "类图",
            PumlKind::State => "状态图",
            PumlKind::Er => "ER 图",
            PumlKind::Mindmap => "思维导图",
        }
    }
}

/// 取第一个 `@start…` 行到对应 `@end…` 行之间的内容（容忍前后的 ``` 围栏或说明文字）。
fn clip(src: &str) -> &str {
    let (mut start, mut end, mut off) = (None, src.len(), 0);
    for line in src.split_inclusive('\n') {
        let t = line.trim();
        if start.is_none() && t.starts_with("@start") {
            start = Some(off);
        } else if start.is_some() && t.starts_with("@end") {
            end = off + line.len();
            break;
        }
        off += line.len();
    }
    &src[start.unwrap_or(0)..end]
}

/// 整词匹配：`title` / `title xxx` / `title: xxx`。
fn word_is(low: &str, w: &str) -> bool {
    low == w || low.strip_prefix(w).is_some_and(|r| r.starts_with(|c: char| c.is_whitespace() || c == ':'))
}

/// 有效内容行（去掉 @start/@end、注释、样式、标题、图例等；多行注释合并成一行）。
fn body(src: &str) -> Vec<String> {
    let src = clip(src);
    let mut out = vec![];
    let mut in_comment = false;
    let mut skip_to: Option<&[&str]> = None;
    for raw in src.lines() {
        let s = raw.trim();
        if in_comment {
            if s.ends_with("'/") {
                in_comment = false;
            }
            continue;
        }
        let lower = s.to_lowercase();
        if let Some(ends) = skip_to {
            if ends.iter().any(|e| lower == *e) {
                skip_to = None;
            }
            continue;
        }
        if s.starts_with("/'") {
            in_comment = !s.ends_with("'/");
            continue;
        }
        // 多行块：skinparam xxx { … } / <style> / legend / 多行 title、header、footer
        if lower.starts_with("skinparam") && lower.ends_with('{') {
            skip_to = Some(&["}"]);
            continue;
        }
        if lower == "<style>" {
            skip_to = Some(&["</style>"]);
            continue;
        }
        if word_is(&lower, "legend") {
            skip_to = Some(&["endlegend", "end legend"]);
            continue;
        }
        if matches!(lower.as_str(), "title" | "header" | "footer" | "caption") {
            skip_to = Some(&["end title", "end header", "end footer", "end caption", "endtitle", "endheader", "endfooter"]);
            continue;
        }
        let skip = s.is_empty()
            || s.starts_with('\'')
            || s.starts_with('!')
            || s.starts_with("@start")
            || s.starts_with("@end")
            || ["skinparam", "hide", "show", "title", "caption", "header", "footer", "scale", "set"].iter().any(|w| word_is(&lower, w));
        if skip {
            continue;
        }
        out.push(s.to_string());
    }
    fold_notes(out)
}

/// `note over A, B` … `end note` 之类的多行块合并成单行：`note over A, B : 第一行<br>第二行`。
fn fold_notes(lines: Vec<String>) -> Vec<String> {
    let mut out = vec![];
    let mut it = lines.into_iter();
    while let Some(s) = it.next() {
        let low = s.to_lowercase();
        let block = ["note", "rnote", "hnote", "ref"].iter().any(|k| word_is(&low, k)) && !s.contains(':') && !(s.contains('"') && low.contains(" as "));
        if !block {
            out.push(s);
            continue;
        }
        let mut text = vec![];
        for l in it.by_ref() {
            let ll = l.to_lowercase();
            if ["end note", "endnote", "end rnote", "end hnote", "end ref", "endref"].iter().any(|e| ll == *e) {
                break;
            }
            text.push(l);
        }
        out.push(format!("{s} : {}", text.join("<br>")));
    }
    out
}

/// `title xxx` 指定的标题（支持多行 `title` … `end title`）。
pub fn title(src: &str) -> Option<String> {
    let mut it = clip(src).lines().map(str::trim);
    while let Some(s) = it.next() {
        let lower = s.to_lowercase();
        if lower == "title" {
            let t: Vec<&str> = it.by_ref().take_while(|l| !l.eq_ignore_ascii_case("end title") && !l.eq_ignore_ascii_case("endtitle")).collect();
            return Some(t.join(" "));
        }
        if word_is(&lower, "title") {
            return Some(s[5..].trim().trim_start_matches(':').trim().to_string());
        }
    }
    None
}

pub fn detect(src: &str) -> Option<PumlKind> {
    let t = clip(src).trim_start();
    if t.starts_with("@startmindmap") || t.starts_with("@startwbs") {
        return Some(PumlKind::Mindmap);
    }
    if !t.starts_with("@startuml") {
        return None;
    }
    let lines = body(t);
    let low: Vec<String> = lines.iter().map(|l| l.to_lowercase()).collect();
    let has = |f: &dyn Fn(&str) -> bool| low.iter().any(|l| f(l));
    if has(&|l| l == "start" || l == "stop" || (l.starts_with(':') && l.ends_with(';')) || l.starts_with("if (") || l.starts_with("while (") || l == "repeat") {
        return Some(PumlKind::Activity);
    }
    if has(&|l| l.starts_with("entity ") && l.ends_with('{')) {
        return Some(PumlKind::Er);
    }
    if has(&|l| ["class ", "abstract ", "interface ", "enum "].iter().any(|k| l.starts_with(k))) {
        return Some(PumlKind::Class);
    }
    if has(&|l| l.contains("[*]") || l.starts_with("state ")) {
        return Some(PumlKind::State);
    }
    // 组件 / 部署 / 对象图：声明行里不会有箭头（避免把名叫 node、file 的时序参与者误判）
    const COMP: [&str; 12] = ["component ", "node ", "cloud ", "artifact ", "folder ", "frame ", "storage ", "object ", "agent ", "card ", "stack ", "file "];
    if has(&|l| {
        (COMP.iter().any(|k| l.starts_with(k)) && !l.contains("->") && !l.contains("--"))
            || l.starts_with("() ")
            || (l.starts_with('[')
                && !l.starts_with("[-")
                && !l.starts_with("[<")
                && !l.starts_with("[]")
                && (l.contains("--") || l.contains("..") || l.contains("->")))
    }) {
        return Some(PumlKind::Component);
    }
    if has(&|l| l.starts_with("usecase") || l.starts_with('(') || l.contains("--> (") || l.contains("-> (") || l.contains("-- (") || l.starts_with("rectangle"))
    {
        return Some(PumlKind::Usecase);
    }
    Some(PumlKind::Sequence)
}

pub fn maybe_convert(src: &str) -> String {
    let src = &clip(src).replace('\r', "");
    match detect(src) {
        None => src.to_string(),
        Some(PumlKind::Mindmap) => mindmap(src),
        Some(PumlKind::Activity) => activity(src),
        Some(PumlKind::Usecase | PumlKind::Component) => usecase(src),
        Some(PumlKind::Class) => class(src),
        Some(PumlKind::State) => state(src),
        Some(PumlKind::Er) => er(src),
        Some(PumlKind::Sequence) => sequence(src),
    }
}

/// 去掉箭头里的方向 / 颜色修饰：`-down->`、`-r->`、`-[#red]->`、`<|-up-` → `-->` / `<|--`。
fn plain_arrows(s: &str) -> String {
    s.split(' ').map(plain_token).collect::<Vec<_>>().join(" ")
}

fn plain_token(t: &str) -> String {
    const HEADS: &str = "<|*o#}+x^";
    let rest = t.trim_start_matches(|c| HEADS.contains(c));
    let l = &t[..t.len() - rest.len()];
    let a_len = rest.len() - rest.trim_start_matches(['-', '.']).len();
    if a_len == 0 {
        return t.to_string();
    }
    let (a, rest) = rest.split_at(a_len);
    let rest = if let Some(r) = rest.strip_prefix('[') {
        match r.find(']') {
            Some(i) => &r[i + 1..],
            None => return t.to_string(),
        }
    } else {
        match ["down", "right", "left", "norank", "up", "do", "ri", "le", "d", "r", "l", "u"].iter().find(|w| rest.starts_with(**w)) {
            Some(w) => &rest[w.len()..],
            None => return t.to_string(),
        }
    };
    let b_len = rest.len() - rest.trim_start_matches(['-', '.']).len();
    let (b, r) = rest.split_at(b_len);
    if !r.chars().all(|c| ">|*o#{+x^".contains(c)) {
        return t.to_string();
    }
    format!("{l}{a}{b}{r}")
}

fn q(s: &str) -> String {
    s.trim().trim_matches('"').replace('"', "'").replace("\\n", "<br>")
}

// ───────────── 思维导图 ─────────────

/// `* 根` / `** 子` / `+ 右` / `- 左`（也支持 OrgMode 的缩进写法）→ (层级, 文字)。
pub fn mind_outline(src: &str) -> Option<Vec<(usize, String)>> {
    let mut out = vec![];
    let lines = body(src);
    let mut it = lines.iter();
    while let Some(s) = it.next() {
        let marks = s.chars().take_while(|c| matches!(c, '*' | '+' | '-' | '#')).count();
        if marks == 0 {
            continue;
        }
        let mut text = s[marks..].trim_start_matches('_').to_string();
        // `*[#Orange] 文字`：去掉颜色
        if text.starts_with('[') {
            if let Some(e) = text.find(']') {
                text = text[e + 1..].to_string();
            }
        }
        // `**:多行\n文字;`：多行文字直到分号
        if text.starts_with(':') {
            text.remove(0);
            while !text.trim_end().ends_with(';') {
                match it.next() {
                    Some(n) => {
                        text.push('\n');
                        text.push_str(n);
                    }
                    None => break,
                }
            }
        }
        let text = text.trim().trim_end_matches(';').trim().to_string();
        if !text.is_empty() {
            out.push((marks, text));
        }
    }
    (!out.is_empty()).then_some(out)
}

/// 有 `-` 开头的节点时为左右分布。
pub fn mind_both_sides(src: &str) -> bool {
    body(src).iter().any(|s| s.starts_with('-'))
}

fn mindmap(src: &str) -> String {
    let mut out = String::from("mindmap\n");
    for (level, text) in mind_outline(src).unwrap_or_default() {
        out.push_str(&"  ".repeat(level));
        out.push_str(&text.replace(['(', ')', '[', ']', '{', '}'], " ").replace('\n', " "));
        out.push('\n');
    }
    out
}

// ───────────── 活动图 → 流程图 ─────────────

struct Flow {
    out: Vec<String>,
    n: usize,
    /// 等待接到下一个节点的出口：(节点, 连线文字)
    prev: Vec<(String, Option<String>)>,
    /// 下一条连线的文字（`-> 文字;`）
    pending: Option<String>,
    /// repeat 循环：记下循环体的第一个节点
    repeat_mark: Vec<Option<String>>,
}

impl Flow {
    fn node(&mut self, shape: (&str, &str), text: &str) -> String {
        self.n += 1;
        let id = format!("a{}", self.n);
        self.out.push(format!("    {id}{}\"{}\"{}", shape.0, q(text), shape.1));
        for m in self.repeat_mark.iter_mut() {
            if m.is_none() {
                *m = Some(id.clone());
            }
        }
        id
    }
    fn edge(&mut self, a: &str, b: &str, label: Option<&str>) {
        match label.filter(|l| !l.is_empty()) {
            Some(l) => self.out.push(format!("    {a} -->|{}| {b}", q(l).replace('|', "/"))),
            None => self.out.push(format!("    {a} --> {b}")),
        }
    }
    /// 把所有待接出口接到 id。
    fn link_to(&mut self, id: &str) {
        let pending = self.pending.take();
        for (p, l) in std::mem::take(&mut self.prev) {
            let l = l.or(pending.clone());
            self.edge(&p, id, l.as_deref());
        }
    }
}

/// 括号里的文字：`if (条件) then (是)` → ("条件", "是")。
fn parens(s: &str) -> Vec<String> {
    let mut v = vec![];
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' => {
                if depth > 0 {
                    cur.push(c);
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    v.push(std::mem::take(&mut cur));
                } else if depth > 0 {
                    cur.push(c);
                }
            }
            _ if depth > 0 => cur.push(c),
            _ => {}
        }
    }
    v
}

fn activity(src: &str) -> String {
    enum Frame {
        If { cond: String, ends: Vec<(String, Option<String>)>, has_else: bool, no: Option<String> },
        While { cond: String, no: Option<String> },
        Fork { origin: Vec<(String, Option<String>)>, ends: Vec<(String, Option<String>)> },
        Switch { cond: String, ends: Vec<(String, Option<String>)> },
    }
    let mut f = Flow { out: vec!["flowchart TD".into()], n: 0, prev: vec![], pending: None, repeat_mark: vec![] };
    let mut stack: Vec<Frame> = vec![];
    let lines = body(src);
    let mut i = 0;
    let mut in_note = false;
    while i < lines.len() {
        let mut s = lines[i].clone();
        i += 1;
        // `#pink:动作;` 动作的颜色前缀
        if s.starts_with('#') {
            if let Some(c) = s.find(':') {
                if !s[..c].contains(char::is_whitespace) {
                    s = s[c..].to_string();
                }
            }
        }
        let low = s.to_lowercase();
        if in_note {
            in_note = !low.starts_with("end note") && !low.starts_with("endnote");
            continue;
        }
        if low.starts_with("note") {
            in_note = !s.contains(':');
            continue;
        }
        if low == "start" {
            let id = f.node(("([", "])"), "开始");
            f.link_to(&id);
            f.prev = vec![(id, None)];
        } else if low == "stop" || low == "end" {
            let id = f.node(("([", "])"), "结束");
            f.link_to(&id);
        } else if low == "detach" || low == "kill" {
            f.prev.clear();
        } else if let Some(first) = s.strip_prefix(':') {
            // 动作可以跨多行，直到分号
            let mut text = first.to_string();
            while !text.trim_end().ends_with(';') && !text.trim_end().ends_with(']') && !text.trim_end().ends_with('|') && i < lines.len() {
                text.push('\n');
                text.push_str(&lines[i]);
                i += 1;
            }
            let text = text.trim_end().trim_end_matches([';', ']', '|', '<', '>', '/', '}']).to_string();
            let id = f.node(("[", "]"), &text.replace('\n', "<br>"));
            f.link_to(&id);
            f.prev = vec![(id, None)];
        } else if low.starts_with("->") {
            f.pending = Some(s[2..].trim().trim_end_matches(';').to_string());
        } else if low.starts_with("if ") || low.starts_with("if(") {
            let p = parens(&s);
            let id = f.node(("{", "}"), p.first().map(|x| x.as_str()).unwrap_or("?"));
            f.link_to(&id);
            let yes = p.get(1).cloned();
            f.prev = vec![(id.clone(), yes)];
            stack.push(Frame::If { cond: id, ends: vec![], has_else: false, no: None });
        } else if low.starts_with("elseif") || low.starts_with("else if") {
            let p = parens(&s);
            if let Some(Frame::If { cond, ends, .. }) = stack.last_mut() {
                ends.append(&mut f.prev);
                let last = cond.clone();
                f.n += 1;
                let id = format!("a{}", f.n);
                f.out.push(format!("    {id}{{\"{}\"}}", q(p.first().map(|x| x.as_str()).unwrap_or("?"))));
                f.out.push(format!("    {last} -->|否| {id}"));
                *cond = id.clone();
                f.prev = vec![(id, p.get(1).cloned())];
            }
        } else if low.starts_with("else") {
            let p = parens(&s);
            if let Some(Frame::If { cond, ends, has_else, .. }) = stack.last_mut() {
                ends.append(&mut f.prev);
                *has_else = true;
                f.prev = vec![(cond.clone(), Some(p.first().cloned().unwrap_or_else(|| "否".into())))];
            }
        } else if low.starts_with("endif") || low.starts_with("end if") {
            if let Some(Frame::If { cond, mut ends, has_else, no }) = stack.pop() {
                ends.append(&mut f.prev);
                if !has_else {
                    ends.push((cond, Some(no.unwrap_or_else(|| "否".into()))));
                }
                f.prev = ends;
            }
        } else if low.starts_with("switch") {
            let p = parens(&s);
            let id = f.node(("{", "}"), p.first().map(|x| x.as_str()).unwrap_or("?"));
            f.link_to(&id);
            f.prev.clear();
            stack.push(Frame::Switch { cond: id, ends: vec![] });
        } else if low.starts_with("case") {
            let p = parens(&s);
            if let Some(Frame::Switch { cond, ends }) = stack.last_mut() {
                ends.append(&mut f.prev);
                f.prev = vec![(cond.clone(), Some(p.first().cloned().unwrap_or_default()))];
            }
        } else if low.starts_with("endswitch") || low.starts_with("end switch") {
            if let Some(Frame::Switch { mut ends, .. }) = stack.pop() {
                ends.append(&mut f.prev);
                f.prev = ends;
            }
        } else if low.starts_with("while") {
            let p = parens(&s);
            let id = f.node(("{", "}"), p.first().map(|x| x.as_str()).unwrap_or("?"));
            f.link_to(&id);
            f.prev = vec![(id.clone(), p.get(1).cloned())];
            stack.push(Frame::While { cond: id, no: None });
        } else if low.starts_with("endwhile") || low.starts_with("end while") {
            let p = parens(&s);
            if let Some(Frame::While { cond, no }) = stack.pop() {
                f.link_to(&cond);
                f.prev = vec![(cond, p.first().cloned().or(no).or(Some("否".into())))];
            }
        } else if low == "repeat" {
            f.repeat_mark.push(None);
        } else if low.starts_with("repeat while") || low.starts_with("repeatwhile") {
            let p = parens(&s);
            let id = f.node(("{", "}"), p.first().map(|x| x.as_str()).unwrap_or("?"));
            f.link_to(&id);
            if let Some(Some(start)) = f.repeat_mark.pop() {
                f.edge(&id, &start, Some(p.get(1).map(|x| x.as_str()).unwrap_or("是")));
            }
            f.prev = vec![(id, Some(p.get(2).cloned().unwrap_or_else(|| "否".into())))];
        } else if low == "fork" || low == "split" {
            stack.push(Frame::Fork { origin: f.prev.clone(), ends: vec![] });
        } else if low.starts_with("fork again") || low.starts_with("split again") {
            if let Some(Frame::Fork { origin, ends }) = stack.last_mut() {
                ends.append(&mut f.prev);
                f.prev = origin.clone();
            }
        } else if low.starts_with("end fork") || low.starts_with("end split") || low.starts_with("end merge") {
            if let Some(Frame::Fork { mut ends, .. }) = stack.pop() {
                ends.append(&mut f.prev);
                f.prev = ends;
            }
        }
        // |泳道|、partition、group、注释等忽略
    }
    f.out.join("\n")
}

// ───────────── 用例图 → 流程图 ─────────────

type Ids = std::collections::HashMap<String, String>;

fn uc_ensure(name: &str, actor: bool, ids: &mut Ids, decl: &mut Vec<String>, alias: Option<&str>, n: &mut usize) -> String {
    let key = alias.unwrap_or(name).to_string();
    if let Some(id) = ids.get(&key) {
        return id.clone();
    }
    *n += 1;
    let id = format!("u{n}");
    decl.push(if actor { format!("    {id}[\"{}\"]", q(name)) } else { format!("    {id}([\"{}\"])", q(name)) });
    ids.insert(key, id.clone());
    if alias.is_some() {
        ids.insert(name.to_string(), id.clone());
    }
    id
}

/// 用例图 / 组件图里的引用：(用例) / :参与者: / [组件] / 名字或别名。
fn uc_ref(r: &str, ids: &mut Ids, decl: &mut Vec<String>, n: &mut usize) -> String {
    let r = r.trim();
    if let Some(x) = r.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        return uc_ensure(x, false, ids, decl, None, n);
    }
    if let Some(x) = r.strip_prefix(':').and_then(|x| x.strip_suffix(':')) {
        return uc_ensure(x, true, ids, decl, None, n);
    }
    let name = r.strip_prefix('[').and_then(|x| x.strip_suffix(']')).unwrap_or(r).trim_matches('"');
    if let Some(id) = ids.get(name) {
        return id.clone();
    }
    uc_ensure(name, true, ids, decl, None, n)
}

/// `name as ALIAS` / `"长名字" as ALIAS` / `ALIAS as "长名字"` → (显示名, 别名)。
fn split_alias(rest: &str) -> (String, Option<String>) {
    match rest.split_once(" as ") {
        Some((a, b)) => {
            let (a, b) = (a.trim(), b.trim());
            if b.starts_with('"') {
                (b.trim_matches('"').to_string(), Some(a.to_string()))
            } else {
                (a.trim_matches('"').to_string(), Some(b.to_string()))
            }
        }
        None => (rest.trim().trim_matches('"').to_string(), None),
    }
}

/// 去掉声明末尾的构造型 / 颜色 / 排序等修饰：`Foo <<service>> #red order 10`。
fn cut_mods(s: &str) -> &str {
    let mut in_q = false;
    for (i, c) in s.char_indices() {
        if c == '"' {
            in_q = !in_q;
        }
        if !in_q && c == ' ' {
            let r = &s[i..];
            if r.starts_with(" <<") || r.starts_with(" #") || r.starts_with(" order ") || r.starts_with(" [[") {
                return s[..i].trim();
            }
        }
    }
    s.trim()
}

/// 关系两端 `"1"` / `"n"` 之类的基数文字（流程图里没有）。
fn drop_card(l: &str, r: &str) -> (String, String) {
    let (mut l, mut r) = (l.trim().to_string(), r.trim().to_string());
    let short_q = |t: &str| t.len() >= 2 && t.len() <= 8 && t.starts_with('"') && t.ends_with('"');
    if let Some((h, t)) = l.rsplit_once(' ') {
        if short_q(t) && !h.trim().is_empty() {
            l = h.trim().to_string();
        }
    }
    if let Some((h, t)) = r.split_once(' ') {
        if short_q(h) && !t.trim().is_empty() {
            r = t.trim().to_string();
        }
    }
    (l, r)
}

fn usecase(src: &str) -> String {
    const NODES: [&str; 17] = [
        "component",
        "node",
        "cloud",
        "artifact",
        "folder",
        "frame",
        "storage",
        "agent",
        "card",
        "stack",
        "file",
        "database",
        "queue",
        "interface",
        "boundary",
        "control",
        "entity",
    ];
    const BOXES: [&str; 10] = ["rectangle", "package", "node", "cloud", "folder", "frame", "component", "database", "storage", "card"];
    let mut out = vec![];
    let mut dir = "TB";
    let mut ids = Ids::new();
    let mut decl: Vec<String> = vec![];
    let mut n = 0;
    // 对象图：(节点 id, 显示名, 成员行, decl 下标)
    let mut obj: Option<(String, String, Vec<String>, usize)> = None;
    let mut open_boxes = 0;
    for s in body(src) {
        let low = s.to_lowercase();
        if let Some((id, name, members, at)) = obj.as_mut() {
            if s == "}" {
                let text = std::iter::once(name.clone()).chain(members.iter().cloned()).collect::<Vec<_>>().join("<br>");
                decl[*at] = format!("    {id}[\"{}\"]", q(&text));
                obj = None;
            } else {
                members.push(s.clone());
            }
            continue;
        }
        if low.starts_with("left to right") {
            dir = "LR";
            continue;
        }
        if low.starts_with("top to bottom") {
            continue;
        }
        // 容器：rectangle X { / package X { / node X {
        if let Some(kw) = BOXES.iter().find(|k| word_is(&low, k)).filter(|_| s.ends_with('{')) {
            let (name, alias) = split_alias(cut_mods(s[kw.len()..].trim().trim_end_matches('{').trim()));
            decl.push(format!("    subgraph {}", q(&name).replace(' ', "_")));
            if let Some(a) = alias {
                ids.insert(a, q(&name).replace(' ', "_"));
            }
            open_boxes += 1;
            continue;
        }
        if s == "}" {
            if open_boxes > 0 {
                decl.push("    end".into());
                open_boxes -= 1;
            }
            continue;
        }
        // object X { 成员 }
        if word_is(&low, "object") {
            let (name, alias) = split_alias(cut_mods(s[6..].trim().trim_end_matches('{').trim()));
            let id = uc_ensure(&name, true, &mut ids, &mut decl, alias.as_deref(), &mut n);
            if s.ends_with('{') {
                obj = Some((id, name, vec![], decl.len() - 1));
            }
            continue;
        }
        // actor 用户 / actor "用户" as U / usecase "登录" as UC1 / usecase (登录) / component Foo / [组件] as C
        let mut declared = false;
        for (kw, actor) in std::iter::once(("actor", true)).chain(std::iter::once(("usecase", false))).chain(NODES.iter().map(|k| (*k, true))) {
            if word_is(&low, kw) && !s.contains("->") && !s.contains("--") {
                let (name, alias) = split_alias(cut_mods(s[kw.len()..].trim()));
                let name = name.trim_matches(['(', ')', ':', '[', ']']).to_string();
                uc_ensure(&name, actor, &mut ids, &mut decl, alias.as_deref(), &mut n);
                declared = true;
                break;
            }
        }
        if declared {
            continue;
        }
        // (查看) as UC3 / :管理员: as A / [订单] as O / () "接口" as I
        if let Some((l, a)) = s
            .split_once(" as ")
            .filter(|(l, _)| (l.starts_with('(') && l.ends_with(')')) || (l.starts_with(':') && l.ends_with(':')) || (l.starts_with('[') && l.ends_with(']')))
        {
            let actor = !l.starts_with('(');
            let name = l.trim_matches(['(', ')', ':', '[', ']']);
            uc_ensure(name, actor, &mut ids, &mut decl, Some(a.trim()), &mut n);
            continue;
        }
        // 单独一行 `[组件]`
        if s.len() > 2 && s.starts_with('[') && s.ends_with(']') && !s.contains(' ')
            || (s.starts_with('[') && s.ends_with(']') && !s.contains("--") && !s.contains("->"))
        {
            uc_ref(&s, &mut ids, &mut decl, &mut n);
            continue;
        }
        if let Some(rest) = s.strip_prefix("() ") {
            let (name, alias) = split_alias(cut_mods(rest));
            if !rest.contains("--") && !rest.contains("..") {
                uc_ensure(&name, false, &mut ids, &mut decl, alias.as_deref(), &mut n);
                continue;
            }
        }
        let s = plain_arrows(&s);
        let (rel, label) = match s.split_once(" : ") {
            Some((a, b)) => (a.to_string(), Some(b.trim().to_string())),
            None => (s.clone(), None),
        };
        let arrows = ["<|--", "--|>", "<|-", "-|>", "..>", "<..", "-->", "<--", "->", "<-", "--", ".>", ".."];
        if let Some((tok, pos)) = arrows.iter().filter_map(|t| rel.find(t).map(|p| (*t, p))).min_by_key(|x| x.1) {
            let (l, r) = drop_card(&rel[..pos], &rel[pos + tok.len()..]);
            if l.is_empty() || r.is_empty() {
                continue;
            }
            let (a, b) = (uc_ref(&l, &mut ids, &mut decl, &mut n), uc_ref(&r, &mut ids, &mut decl, &mut n));
            let back = tok.starts_with('<');
            let (a, b) = if back { (b, a) } else { (a, b) };
            let op = if tok.contains('.') {
                "-.->"
            } else if tok == "--" {
                "---"
            } else {
                "-->"
            };
            match label {
                Some(l) => out.push(format!("    {a} {op}|{}| {b}", q(&l).replace('|', "/"))),
                None => out.push(format!("    {a} {op} {b}")),
            }
        }
    }
    for _ in 0..open_boxes {
        decl.push("    end".into());
    }
    let mut all = vec![format!("flowchart {dir}")];
    all.extend(decl);
    all.extend(out);
    all.join("\n")
}

// ───────────── 类图 / 状态图 / ER 图 ─────────────

fn class(src: &str) -> String {
    let mut out = vec!["classDiagram".to_string()];
    // 块的类型：true 表示类（需要输出右括号），false 表示 package 等容器
    let mut blocks: Vec<bool> = vec![];
    // `class B extends A` 产生的继承关系，放在最后输出
    let mut later: Vec<String> = vec![];
    for s in body(src) {
        let low = s.to_lowercase();
        if blocks.last() != Some(&true) {
            if low.starts_with("left to right") {
                out.push("  direction LR".into());
                continue;
            }
            if low.starts_with("top to bottom") {
                continue;
            }
            if word_is(&low, "note") && s.contains(':') {
                // `note right of Foo : 说明` → 独立注释节点
                out.push(format!("  note \"{}\"", s.split_once(':').map(|x| x.1.trim()).unwrap_or("").replace('"', "'")));
                continue;
            }
        }
        if low.starts_with("package") || low.starts_with("namespace") || low.starts_with("together") {
            if s.ends_with('{') {
                blocks.push(false);
            }
            continue;
        }
        if s.starts_with('}') {
            if blocks.pop().unwrap_or(true) {
                out.push("  }".into());
            }
            continue;
        }
        let mut decl = None;
        for (kw, ann) in [
            ("abstract class", Some("abstract")),
            ("abstract", Some("abstract")),
            ("interface", Some("interface")),
            ("enum", Some("enumeration")),
            ("class", None),
        ] {
            if low.starts_with(kw) && s[kw.len()..].starts_with(char::is_whitespace) {
                let rest = s[kw.len()..].trim();
                let open = rest.ends_with('{');
                let name = rest.trim_end_matches('{').trim();
                // `class B extends A implements C, D`
                let mut name = name;
                for (word, op) in [(" extends ", "<|--"), (" implements ", "<|..")] {
                    if let Some((head, tail)) = name.split_once(word) {
                        let own = head.trim().trim_matches('"').replace(' ', "_");
                        let tail = tail.split(" implements ").next().unwrap_or(tail).split(" extends ").next().unwrap_or(tail);
                        later.extend(tail.split(',').map(|p| format!("  {} {op} {own}", p.trim().replace(' ', "_"))));
                        name = head;
                    }
                }
                // 去掉泛型约束、颜色、构造型
                let name = name.split(['<', '#']).next().unwrap_or(name).trim();
                let (name, label) = match name.split_once(" as ") {
                    Some((a, b)) => (b.trim().to_string(), Some(a.trim().trim_matches('"').to_string())),
                    None => (name.trim_matches('"').to_string(), None),
                };
                decl = Some((name, label, ann, open));
                break;
            }
        }
        if let Some((name, label, ann, open)) = decl {
            let name = name.replace(' ', "_");
            match label {
                Some(l) => out.push(format!("  class {name}[\"{l}\"] {{")),
                None => out.push(format!("  class {name} {{")),
            }
            if let Some(a) = ann {
                out.push(format!("    <<{a}>>"));
            }
            if open {
                blocks.push(true);
            } else {
                out.push("  }".into());
            }
            continue;
        }
        if blocks.last() == Some(&true) {
            // 成员：去掉 {static} / {abstract}，分隔线忽略
            if s.starts_with("--") || s.starts_with("==") || s.starts_with("..") || s.starts_with("__") {
                continue;
            }
            out.push(format!("    {}", s.replace("{static}", "").replace("{abstract}", "").replace("{field}", "").replace("{method}", "").trim()));
            continue;
        }
        let s = plain_arrows(&s).replace(" -> ", " --> ").replace(" <- ", " <-- ");
        out.push(format!("  {s}"));
    }
    out.extend(later);
    out.join("\n")
}

fn state(src: &str) -> String {
    let mut out = vec!["stateDiagram-v2".to_string()];
    for s in body(src) {
        let low = s.to_lowercase();
        if low.starts_with("left to right") {
            out.push("  direction LR".into());
            continue;
        }
        if low.starts_with("top to bottom") {
            continue;
        }
        let s = plain_arrows(&s).replace(" -> ", " --> ");
        // 状态的颜色、构造型等修饰去掉
        let s = s.split(" #").next().unwrap_or(&s).to_string();
        out.push(format!("  {s}"));
    }
    out.join("\n")
}

fn er(src: &str) -> String {
    let mut out = vec!["erDiagram".to_string()];
    let mut in_entity = false;
    for s in body(src) {
        let low = s.to_lowercase();
        if low.starts_with("entity") {
            let name = s[6..].trim().trim_end_matches('{').trim();
            // `entity "显示名" as ALIAS` → `ALIAS[显示名] {`；只有名字时直接当 ID
            let head = match name.split_once(" as ") {
                Some((title, alias)) => format!("{}[{}]", alias.trim().replace(' ', "_"), title.trim().trim_matches('"').replace(['[', ']'], "")),
                None => name.trim_matches('"').replace(' ', "_"),
            };
            out.push(format!("  {head} {{"));
            in_entity = s.ends_with('{');
            if !in_entity {
                out.push("  }".into());
            }
            continue;
        }
        if in_entity {
            if s.starts_with('}') {
                out.push("  }".into());
                in_entity = false;
                continue;
            }
            if s.starts_with("--") || s.starts_with("==") || s.starts_with("..") {
                continue;
            }
            // `* id : int <<PK>>` → `int id PK`
            let key = s.starts_with('*') || s.contains("<<PK>>") || s.contains("<<pk>>");
            let fk = s.contains("<<FK>>") || s.contains("<<fk>>");
            let t = s.trim_start_matches(['*', '+', '-', '#', '~']).replace("<<PK>>", "").replace("<<FK>>", "").replace("<<pk>>", "").replace("<<fk>>", "");
            let (name, ty) = match t.split_once(':') {
                Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
                None => (t.trim().to_string(), "string".to_string()),
            };
            let ty = if ty.is_empty() { "string".to_string() } else { ty.split_whitespace().next().unwrap_or("string").to_string() };
            let mut line = format!("    {ty} {}", name.replace(' ', "_"));
            if key {
                line.push_str(" PK");
            } else if fk {
                line.push_str(" FK");
            }
            out.push(line);
            continue;
        }
        let s = plain_arrows(&s);
        if s.contains("--") || s.contains("..") {
            // 关系：`A ||--o{ B : 标签`；没有标签时补一个，Mermaid 需要
            // 去掉两端的 `"1"` / `"n"` 基数文字（Mermaid 写法里没有）
            let (rel, label) = match s.split_once(':') {
                Some((r, l)) => (r, Some(l.trim())),
                None => (s.as_str(), None),
            };
            let rel = rel.split_whitespace().filter(|t| !(t.len() >= 2 && t.starts_with('"') && t.ends_with('"'))).collect::<Vec<_>>().join(" ");
            let line = format!("{rel} : {}", label.filter(|l| !l.is_empty()).unwrap_or("\"\""));
            out.push(format!("  {line}"));
        }
    }
    out.join("\n")
}

// ───────────── 时序图 ─────────────

/// 一条时序消息：(发送方, 接收方, Mermaid 箭头, 激活前缀, 文字)。
struct Msg {
    from: String,
    to: String,
    op: &'static str,
    act: &'static str,
    text: String,
}

/// 解析 `A -> B : 文字`，支持 `-->` `->>` `->x` `->o` `<-` `<->` `-[#red]>` `\\` `/` 半箭头、`++` `--` 激活、`[->` `->]` 外部端点。
fn parse_msg(s: &str) -> Option<Msg> {
    let (head, text) = match split_unquoted(s, ':') {
        Some((h, t)) => (h, t.trim().to_string()),
        None => (s, String::new()),
    };
    let chars: Vec<(usize, char)> = head.char_indices().collect();
    // 第一个不在引号里、能起头的箭头位置
    let mut in_q = false;
    let mut start = None;
    for (k, &(i, c)) in chars.iter().enumerate() {
        if c == '"' {
            in_q = !in_q;
        }
        if in_q {
            continue;
        }
        let next = chars.get(k + 1).map(|x| x.1);
        let prev_space = k == 0 || chars[k - 1].1.is_whitespace() || chars[k - 1].1 == '[';
        let is_start = match c {
            '-' => true,
            '<' => matches!(next, Some('-') | Some('<')),
            '\\' | '/' => matches!(next, Some('-') | Some('\\') | Some('/')) && prev_space,
            'x' | 'o' => prev_space && matches!(next, Some('-') | Some('<')),
            _ => false,
        };
        if is_start {
            start = Some(i);
            break;
        }
    }
    let p = start?;
    let from = head[..p].trim();
    let arrow = &head[p..];
    let mut it = arrow.chars().peekable();
    let (mut left, mut right) = (String::new(), String::new());
    let mut dashes = 0;
    // 左端头
    while let Some(&c) = it.peek() {
        if matches!(c, '<' | '\\' | '/' | 'x' | 'o') {
            left.push(c);
            it.next();
        } else {
            break;
        }
    }
    // 线身（可夹 [#color,dashed] 修饰）
    while let Some(&c) = it.peek() {
        if c == '-' {
            dashes += 1;
            it.next();
        } else if c == '[' {
            for c in it.by_ref() {
                if c == ']' {
                    break;
                }
            }
        } else {
            break;
        }
    }
    if dashes == 0 {
        return None;
    }
    // 右端头
    while let Some(&c) = it.peek() {
        if matches!(c, '>' | '\\' | '/' | 'x' | 'o' | ')') {
            right.push(c);
            it.next();
        } else {
            break;
        }
    }
    let rest: String = it.collect();
    let (mut a, mut b) = (from.to_string(), rest.trim().to_string());
    // 接收方后面的激活标记：`B ++`、`B --`、`B ++ #red`
    let mut act = "";
    for mark in ["++", "--", "**", "!!"] {
        if let Some(r) = b.strip_suffix(mark).or_else(|| b.split(&format!(" {mark}")).next().filter(|x| x.len() < b.len())) {
            act = match mark {
                "++" => "+",
                "--" => "-",
                _ => "",
            };
            b = r.trim().to_string();
            break;
        }
    }
    for mark in ["++", "--", "**", "!!"] {
        if let Some(r) = a.strip_suffix(mark) {
            a = r.trim().to_string();
        }
    }
    let ext = |x: String| if x == "[" || x == "]" || x == "?" || x.is_empty() { "外部".to_string() } else { x.trim_matches('"').to_string() };
    let (mut from, mut to) = (ext(a), ext(b));
    let dashed = dashes >= 2;
    let both = left.starts_with('<') && !right.is_empty();
    let reversed = left.starts_with('<') && right.is_empty();
    if reversed {
        std::mem::swap(&mut from, &mut to);
        // 反向时箭头头部在左端
        right = left.chars().map(|c| if c == '<' { '>' } else { c }).collect();
    }
    let kind = if right.contains('x') {
        "x"
    } else if right.starts_with(">>") || right.contains(')') {
        "open"
    } else {
        "filled"
    };
    let op = match (both, kind, dashed) {
        (true, _, false) => "<<->>",
        (true, _, true) => "<<-->>",
        (_, "x", false) => "-x",
        (_, "x", true) => "--x",
        (_, "open", false) => "-)",
        (_, "open", true) => "--)",
        (_, _, false) => "->>",
        (_, _, true) => "-->>",
    };
    if from.is_empty() || to.is_empty() {
        return None;
    }
    Some(Msg { from, to, op, act, text })
}

/// 在引号之外按第一个 `sep` 拆开。
fn split_unquoted(s: &str, sep: char) -> Option<(&str, &str)> {
    let mut in_q = false;
    for (i, c) in s.char_indices() {
        if c == '"' {
            in_q = !in_q;
        } else if c == sep && !in_q {
            return Some((&s[..i], &s[i + 1..]));
        }
    }
    None
}

fn sequence(src: &str) -> String {
    let mut out = vec!["sequenceDiagram".to_string()];
    if let Some(t) = title(src).filter(|t| !t.is_empty()) {
        out.push(format!("  title {}", t.replace('\n', " ")));
    }
    // 显示名 → 别名：`participant "API 网关" as G` 之后，`"API 网关" -> G` 里的引号名也指向 G
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut last: Option<(String, String)> = None;
    let mut last_p = String::new();
    // 块栈：true 表示需要输出 end（box 不是时序块）
    let mut blocks: Vec<bool> = vec![];
    for s in body(src) {
        let lower = s.to_lowercase();
        let word = lower.split_whitespace().next().unwrap_or("");
        match word {
            "participant" | "actor" | "boundary" | "control" | "entity" | "database" | "collections" | "queue" => {
                let (name, alias) = split_alias(cut_mods(s[word.len()..].trim()));
                let id = alias.clone().unwrap_or_else(|| name.clone());
                names.insert(name.clone(), id.clone());
                last_p = id.clone();
                let kw = if word == "actor" { "actor" } else { "participant" };
                out.push(if id == name { format!("  {kw} {id}") } else { format!("  {kw} {id} as {name}") });
                continue;
            }
            "autonumber" => {
                out.push("  autonumber".into());
                continue;
            }
            "activate" | "deactivate" => {
                let who = cut_mods(s[word.len()..].trim()).trim_matches('"').to_string();
                let who = names.get(&who).cloned().unwrap_or(who);
                out.push(format!("  {word} {who}"));
                continue;
            }
            "alt" | "loop" | "opt" | "par" | "critical" | "break" => {
                blocks.push(true);
                out.push(format!("  {s}"));
                continue;
            }
            "group" => {
                blocks.push(true);
                out.push("  rect rgb(240,244,255)".into());
                continue;
            }
            "box" => {
                blocks.push(false);
                continue;
            }
            "else" | "and" => {
                out.push(format!("  {s}"));
                continue;
            }
            "end" => {
                if blocks.pop().unwrap_or(true) {
                    out.push("  end".into());
                }
                continue;
            }
            "return" => {
                if let Some((from, to)) = &last {
                    out.push(format!("  {to} -->> {from}: {}", s[6..].trim()));
                }
                continue;
            }
            "note" | "rnote" | "hnote" | "ref" => {
                let (head, text) = split_unquoted(&s, ':').unwrap_or((&s, ""));
                let head = cut_mods(head).split_whitespace().collect::<Vec<_>>().join(" ").replace(", ", ",").replace(" ,", ",");
                let hl = head.to_lowercase();
                let who = |rest: &str| {
                    rest.split(',')
                        .map(|x| names.get(x.trim().trim_matches('"')).cloned().unwrap_or_else(|| x.trim().trim_matches('"').to_string()))
                        .collect::<Vec<_>>()
                        .join(",")
                };
                let pos = if let Some(r) = hl.find(" over ").map(|i| &head[i + 6..]).or_else(|| hl.strip_suffix(" over").map(|_| "")) {
                    Some(format!("over {}", who(r)))
                } else if let Some(i) = hl.find(" left of ") {
                    Some(format!("left of {}", who(&head[i + 9..])))
                } else if let Some(i) = hl.find(" right of ") {
                    Some(format!("right of {}", who(&head[i + 10..])))
                } else if hl.ends_with(" left") || hl.ends_with(" right") {
                    // 没写对象：挂在最近出现的参与者上
                    Some(format!("{} {last_p}", if hl.ends_with(" left") { "left of" } else { "right of" }))
                } else {
                    None
                };
                if let Some(pos) = pos {
                    let text = if word == "ref" { format!("ref: {}", text.trim()) } else { text.trim().to_string() };
                    out.push(format!("  Note {pos}: {text}"));
                } else {
                    out.push(format!("  %% {s}"));
                }
                continue;
            }
            "create" | "destroy" | "newpage" | "hide" | "show" => continue,
            _ => {}
        }
        // 分隔线 == x == / 延时 ... / 空白 ||| 没有对应写法，忽略
        if s.starts_with("==") || s.starts_with("...") || s.starts_with("|||") || s.starts_with("||") && s.ends_with("||") {
            continue;
        }
        // 引号里的显示名换成别名
        let s = names.iter().fold(s.clone(), |acc, (name, id)| if name == id { acc } else { acc.replace(&format!("\"{name}\""), id) });
        match parse_msg(&s) {
            Some(m) => {
                last_p = m.to.clone();
                out.push(format!("  {} {} {}{}: {}", m.from, m.op, m.act, m.to, m.text));
                last = Some((m.from, m.to));
            }
            None => out.push(format!("  %% {s}")),
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_basic_sequence() {
        let m = maybe_convert("@startuml\nactor User\nUser -> Server : 请求\nServer --> User : 响应\n@enduml");
        assert!(m.starts_with("sequenceDiagram"));
        assert!(m.contains("User ->> Server: 请求"));
        assert!(m.contains("Server -->> User: 响应"));
    }

    #[test]
    fn activity_becomes_flowchart_with_branches() {
        let src = "@startuml\nstart\n:提交申请;\nif (金额 > 1000?) then (是)\n  :总监审批;\nelse (否)\n  :经理审批;\nendif\n:归档;\nstop\n@enduml";
        assert_eq!(detect(src), Some(PumlKind::Activity));
        let m = maybe_convert(src);
        assert!(m.starts_with("flowchart TD"), "{m}");
        assert!(m.contains("|是|") && m.contains("|否|"), "{m}");
        // 两个分支都汇到「归档」
        let archive = m.lines().find(|l| l.contains("归档")).unwrap().trim().split('[').next().unwrap().to_string();
        assert_eq!(m.lines().filter(|l| l.trim_end().ends_with(&format!(" {archive}"))).count(), 2, "{m}");
        let b = crate::board::from_mermaid(&m, false).unwrap();
        assert!(b.els.len() > 6);
    }

    #[test]
    fn class_state_er_usecase_mindmap() {
        let c = maybe_convert("@startuml\ninterface Shape {\n +area() : double\n}\nclass Circle {\n -r : double\n}\nShape <|.. Circle\n@enduml");
        assert!(c.starts_with("classDiagram") && c.contains("<<interface>>") && c.contains("Shape <|.. Circle"), "{c}");
        let s = maybe_convert("@startuml\n[*] -down-> 待支付\n待支付 -> 已支付 : 支付\n@enduml");
        assert!(s.starts_with("stateDiagram") && s.contains("[*] --> 待支付"), "{s}");
        let e = maybe_convert("@startuml\nentity 用户 {\n * id : int\n --\n 名字 : varchar\n}\n用户 ||--o{ 订单\n@enduml");
        assert!(e.starts_with("erDiagram") && e.contains("int id PK") && e.contains("||--o{"), "{e}");
        let u = maybe_convert("@startuml\nleft to right direction\nactor 用户\n用户 --> (登录)\n(登录) .> (验证码) : include\n@enduml");
        assert!(u.starts_with("flowchart LR") && u.contains("-.->"), "{u}");
        let o = mind_outline("@startmindmap\n* 根\n** 甲\n*** 甲1\n-- 乙\n@endmindmap").unwrap();
        assert_eq!(o, vec![(1, "根".to_string()), (2, "甲".into()), (3, "甲1".into()), (2, "乙".into())]);
        for src in [&c, &s, &e, &u] {
            assert!(crate::diagram::render(src, false, None, &mut crate::canvas::EstimateMeasure, &Default::default()).is_ok(), "{src}");
        }
        assert!(crate::diagram::render(
            &maybe_convert("@startmindmap\n* 根\n** 甲\n@endmindmap"),
            false,
            None,
            &mut crate::canvas::EstimateMeasure,
            &Default::default()
        )
        .is_ok());
    }

    #[test]
    fn er_with_quoted_cardinality_and_titles() {
        let src = "@startuml 商品\nentity \"类目 CATEGORY\" as CATEGORY {\n  * id : bigint <<PK>>\n  --\n  name : varchar \"备注\"\n}\nCATEGORY \"1\" ||--o{ \"n\" CATEGORY : 父子类目\n@enduml";
        let m = maybe_convert(src);
        assert!(m.contains("CATEGORY[类目 CATEGORY] {") && m.contains("CATEGORY ||--o{ CATEGORY : 父子类目"), "{m}");
        assert!(crate::diagram::render(&m, false, None, &mut crate::canvas::EstimateMeasure, &Default::default()).is_ok(), "{m}");
    }

    #[test]
    fn sequence_multiline_note_and_aliases() {
        // 样例是本地文件，不在仓库里；没有时跳过
        let Ok(src) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/task/test-plantuml.md")) else { return };
        let src = src.split("```text\n").nth(1).unwrap_or("").split("```").next().unwrap_or("");
        let m = maybe_convert(src);
        assert!(m.contains("Note over MQ,P: 失败重试"), "{m}");
        assert!(crate::diagram::render(&m, false, None, &mut crate::canvas::EstimateMeasure, &Default::default()).is_ok(), "{m}");
    }

    fn renders(src: &str) -> String {
        let m = maybe_convert(src);
        let r = crate::diagram::render(&m, false, None, &mut crate::canvas::EstimateMeasure, &Default::default());
        assert!(r.is_ok(), "{:?}\n{m}", r.err().map(|e| e.msg));
        m
    }

    #[test]
    fn sequence_variants() {
        // 显示名 / 别名顺序、box、block 注释、skinparam 块、箭头修饰、return
        let m = renders("@startuml\nskinparam sequence {\n ArrowColor red\n}\nparticipant \"API 网关\" as G\nactor Bob as B\nbox \"内部\" #LightBlue\nparticipant Z\nend box\n/' c\nG -> Z : x '/\nB -[#red]> G : 重试\nG ->x B : 超时\nB <- G : 反向\nG --> B ++ : 激活\nnote right of G\n多行\nend note\nrnote over G : r\nref over G, B : 引用\nreturn 结果\n== 分隔 ==\n...\n@enduml");
        assert!(m.contains("participant G as API 网关") && m.contains("actor B as Bob"), "{m}");
        assert!(m.contains("B ->> G: 重试") && m.contains("G -x B: 超时") && m.contains("G ->> B: 反向") && m.contains("G -->> +B: 激活"), "{m}");
        assert!(m.contains("Note right of G: 多行") && m.contains("Note over G: r") && m.contains("Note over G,B: ref: 引用"), "{m}");
        assert!(!m.contains("%%"), "{m}");
        // ``` 围栏与前后说明文字不影响
        renders("说明\n```plantuml\n@startuml\nA -> B\n@enduml\n```\n结尾");
        // 外部端点与双向箭头
        renders("@startuml\n[-> A : in\nA ->] : out\nA <-> B : both\n@enduml");
    }

    #[test]
    fn class_state_variants() {
        let m = renders("@startuml\nleft to right direction\nclass B extends A\nA <|-down- B\nnote right of A : 注\nlegend\n图例\nend legend\n@enduml");
        assert!(m.contains("A <|-- B") && !m.contains("down") && m.contains("direction LR") && !m.contains("图例"), "{m}");
        let m = renders("@startuml\n[*] -down-> Idle\nIdle -[#red]-> Busy : go\nBusy --> [*]\n@enduml");
        assert!(m.contains("[*] --> Idle") && m.contains("Idle --> Busy : go"), "{m}");
    }

    #[test]
    fn component_object_usecase_variants() {
        let m = renders("@startuml\nnode N {\n  [C]\n}\ndatabase DB\n[B] --> [C] : 用\n[C] ..> DB\nobject O {\n id = 1\n}\nO --> DB\n@enduml");
        assert_eq!(detect(&m), None);
        let m = renders("@startuml\nactor Admin\nactor U\n(查看) as UC3\nAdmin --> UC3\nAdmin -|> U\n@enduml");
        assert!(m.contains("查看") && m.matches("-->").count() >= 2, "{m}");
    }

    #[test]
    fn activity_color_and_switch() {
        let m = renders("@startuml\nstart\n#pink:粉色;\nswitch (类型?)\ncase (A)\n  :a;\ncase (B)\n  :b;\nendswitch\n:end;\nstop\n@enduml");
        assert!(m.contains("粉色") && m.contains("|A|") && m.contains("|B|"), "{m}");
    }

    #[test]
    fn mindmap_and_wbs() {
        let o = mind_outline("@startwbs\n* 根\n**[#Orange] 彩色\n**:多行\n文字;\n@endwbs").unwrap();
        assert_eq!(o, vec![(1, "根".to_string()), (2, "彩色".into()), (2, "多行\n文字".into())]);
        assert_eq!(detect("@startwbs\n* a\n@endwbs"), Some(PumlKind::Mindmap));
    }
}
