//! 轻量代码高亮：一个通用的单遍扫描器，按语言切换关键字表与注释语法。
//! 文档代码块（每帧，带缓存）、文本编辑框、附件里的代码文件预览（HTML）共用这一套。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    /// 连线箭头（`-->`、`->>`、`<|--` 等，Mermaid / PlantUML）
    Arrow,
}

pub const LANGS: &[&str] = &[
    "Plain Text",
    "Rust",
    "Python",
    "JavaScript",
    "TypeScript",
    "Go",
    "Java",
    "C",
    "C++",
    "Shell",
    "SQL",
    "JSON",
    "YAML",
    "HTML",
    "CSS",
    "Markdown",
    "Kotlin",
    "Swift",
    "Mermaid",
    "LaTeX",
];

struct Syntax {
    keywords: &'static [&'static str],
    line_comment: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
}

fn syntax_for(lang: &str) -> Option<Syntax> {
    let l = lang.to_ascii_lowercase();
    let c_like_comments = (&["//"][..], Some(("/*", "*/")));
    let (keywords, (line_comment, block_comment)): (&[&str], (&[&str], Option<(&str, &str)>)) = match l.as_str() {
        "rust" | "rs" => (
            &[
                "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "use", "mod", "match", "if", "else", "for", "while", "loop", "return", "self",
                "Self", "crate", "super", "as", "in", "where", "async", "await", "move", "ref", "const", "static", "type", "dyn", "unsafe", "true", "false",
                "break", "continue",
            ],
            c_like_comments,
        ),
        "python" | "py" => (
            &[
                "def", "class", "import", "from", "as", "if", "elif", "else", "for", "while", "return", "in", "not", "and", "or", "is", "None", "True",
                "False", "with", "try", "except", "finally", "raise", "lambda", "yield", "async", "await", "pass", "break", "continue", "global", "self",
            ],
            (&["#"][..], None),
        ),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => (
            &[
                "function",
                "const",
                "let",
                "var",
                "if",
                "else",
                "for",
                "while",
                "return",
                "class",
                "extends",
                "new",
                "this",
                "import",
                "from",
                "export",
                "default",
                "async",
                "await",
                "try",
                "catch",
                "finally",
                "throw",
                "typeof",
                "instanceof",
                "in",
                "of",
                "true",
                "false",
                "null",
                "undefined",
                "interface",
                "type",
                "enum",
                "implements",
                "public",
                "private",
                "readonly",
                "switch",
                "case",
                "break",
            ],
            c_like_comments,
        ),
        "go" | "golang" => (
            &[
                "func",
                "package",
                "import",
                "var",
                "const",
                "type",
                "struct",
                "interface",
                "map",
                "chan",
                "go",
                "defer",
                "if",
                "else",
                "for",
                "range",
                "return",
                "switch",
                "case",
                "default",
                "select",
                "nil",
                "true",
                "false",
                "break",
                "continue",
            ],
            c_like_comments,
        ),
        "java" | "kotlin" | "kt" | "swift" | "c" | "c++" | "cpp" | "h" => (
            &[
                "class",
                "public",
                "private",
                "protected",
                "static",
                "final",
                "void",
                "int",
                "long",
                "double",
                "float",
                "char",
                "bool",
                "boolean",
                "return",
                "if",
                "else",
                "for",
                "while",
                "new",
                "this",
                "import",
                "package",
                "extends",
                "implements",
                "interface",
                "try",
                "catch",
                "throw",
                "throws",
                "null",
                "true",
                "false",
                "const",
                "struct",
                "include",
                "namespace",
                "template",
                "auto",
                "fun",
                "val",
                "var",
                "func",
                "let",
                "nil",
                "override",
                "switch",
                "case",
                "break",
            ],
            c_like_comments,
        ),
        "shell" | "bash" | "sh" | "zsh" => (
            &[
                "if", "then", "else", "elif", "fi", "for", "do", "done", "while", "case", "esac", "function", "return", "export", "local", "echo", "cd", "in",
                "source",
            ],
            (&["#"][..], None),
        ),
        "sql" => (
            &[
                "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create", "table", "drop", "alter", "join", "left", "right",
                "inner", "on", "group", "by", "order", "limit", "and", "or", "not", "null", "as", "primary", "key", "index", "SELECT", "FROM", "WHERE",
                "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "JOIN", "LEFT", "ON", "GROUP", "BY", "ORDER", "LIMIT", "AND", "OR",
                "NOT", "NULL", "AS",
            ],
            (&["--"][..], Some(("/*", "*/"))),
        ),
        "json" => (&["true", "false", "null"], (&[][..], None)),
        "yaml" | "yml" => (&["true", "false", "null"], (&["#"][..], None)),
        "css" => (&["important"], (&[][..], Some(("/*", "*/")))),
        "mermaid" => (
            &[
                "flowchart",
                "graph",
                "subgraph",
                "end",
                "direction",
                "sequenceDiagram",
                "participant",
                "actor",
                "loop",
                "alt",
                "else",
                "opt",
                "par",
                "and",
                "note",
                "Note",
                "over",
                "classDiagram",
                "class",
                "stateDiagram",
                "state",
                "erDiagram",
                "gantt",
                "section",
                "title",
                "dateFormat",
                "pie",
                "mindmap",
                "timeline",
                "classDef",
                "style",
                "linkStyle",
                "activate",
                "deactivate",
                "autonumber",
                "TD",
                "TB",
                "LR",
                "RL",
                "BT",
                "of",
                "left",
                "right",
                "as",
                "critical",
                "break",
                "rect",
            ],
            (&["%%"][..], None),
        ),
        "plantuml" | "puml" => (
            &[
                "startuml",
                "enduml",
                "startmindmap",
                "endmindmap",
                "startwbs",
                "endwbs",
                "actor",
                "participant",
                "boundary",
                "control",
                "entity",
                "database",
                "collections",
                "queue",
                "as",
                "activate",
                "deactivate",
                "destroy",
                "autonumber",
                "alt",
                "else",
                "opt",
                "loop",
                "par",
                "break",
                "critical",
                "group",
                "end",
                "note",
                "over",
                "of",
                "left",
                "right",
                "top",
                "bottom",
                "ref",
                "title",
                "start",
                "stop",
                "if",
                "then",
                "elseif",
                "endif",
                "while",
                "endwhile",
                "repeat",
                "is",
                "not",
                "fork",
                "again",
                "split",
                "partition",
                "class",
                "abstract",
                "interface",
                "enum",
                "package",
                "namespace",
                "usecase",
                "rectangle",
                "component",
                "node",
                "cloud",
                "frame",
                "folder",
                "state",
                "skinparam",
                "direction",
                "to",
                "hide",
                "show",
                "return",
                "box",
                "detach",
                "kill",
            ],
            (&["'"][..], Some(("/'", "'/"))),
        ),
        "latex" | "tex" => (&[], (&["%"][..], None)),
        "html" | "xml" => (&[], (&[][..], Some(("<!--", "-->")))),
        // 认不出的代码文件：只标字符串 / 数字 / 常见注释
        "generic" => (&[], (&["#", "//"][..], Some(("/*", "*/")))),
        _ => return None,
    };
    Some(Syntax { keywords, line_comment, block_comment })
}

pub fn highlight(lang: &str, text: &str) -> Vec<(Range<usize>, Tok)> {
    let Some(syn) = syntax_for(lang) else {
        return vec![];
    };
    let l = lang.to_ascii_lowercase();
    let arrows = matches!(l.as_str(), "mermaid" | "plantuml" | "puml");
    // PlantUML 的单引号是注释，不是字符串
    let quote_comment = matches!(l.as_str(), "plantuml" | "puml");
    let keywords: HashSet<&str> = syn.keywords.iter().copied().collect();
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        if let Some(lc) = syn.line_comment.iter().find(|lc| rest.starts_with(**lc)) {
            let _ = lc;
            let end = rest.find('\n').map(|e| i + e).unwrap_or(text.len());
            out.push((i..end, Tok::Comment));
            i = end;
            continue;
        }
        if let Some((open, close)) = syn.block_comment {
            if let Some(after) = rest.strip_prefix(open) {
                let end = after.find(close).map(|e| i + open.len() + e + close.len()).unwrap_or(text.len());
                out.push((i..end, Tok::Comment));
                i = end;
                continue;
            }
        }
        let b = bytes[i];
        if arrows && matches!(b, b'-' | b'.' | b'=' | b'<' | b'>' | b'|' | b'*' | b'o' | b'x' | b'}' | b'{') {
            if let Some(n) = arrow_len(rest) {
                out.push((i..i + n, Tok::Arrow));
                i += n;
                continue;
            }
        }
        if b == b'"' || (b == b'\'' && !quote_comment) || b == b'`' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b && bytes[j] != b'\n' {
                if bytes[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(bytes.len());
            out.push((i..end, Tok::String));
            i = end;
            continue;
        }
        if b.is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || bytes[j] == b'_') {
                j += 1;
            }
            out.push((i..j, Tok::Number));
            i = j;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let word = &text[i..j];
            if keywords.contains(word) {
                out.push((i..j, Tok::Keyword));
            } else if arrows {
                // 图表代码里的节点名 / 别名不着色
            } else if bytes.get(j) == Some(&b'(') || bytes.get(j) == Some(&b'!') {
                out.push((i..j, Tok::Function));
            } else if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && word.len() > 1 {
                out.push((i..j, Tok::Type));
            }
            i = j;
            continue;
        }
        // 跳过一个完整字符（可能是多字节）
        i += rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

type Tokens = Rc<Vec<(Range<usize>, Tok)>>;

thread_local! {
    /// (语言, 文本哈希, 文本长度) → 高亮结果。代码块每帧都要高亮，文字不变就直接复用。
    static CACHE: RefCell<HashMap<(String, u64, usize), Tokens>> = RefCell::new(HashMap::new());
}

const CACHE_CAP: usize = 512;

/// 带缓存的 `highlight`（渲染路径用）。
pub fn highlight_cached(lang: &str, text: &str) -> Tokens {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    let key = (lang.to_string(), h.finish(), text.len());
    CACHE.with(|c| {
        if let Some(t) = c.borrow().get(&key) {
            return t.clone();
        }
        let t: Tokens = Rc::new(highlight(lang, text));
        let mut c = c.borrow_mut();
        if c.len() >= CACHE_CAP {
            c.clear();
        }
        c.insert(key, t.clone());
        t
    })
}

/// 文件扩展名 → 高亮语言（附件代码预览用）。纯文本类返回 None，其余认不出的按通用规则。
pub fn lang_for_ext(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "txt" | "log" | "csv" | "tsv" | "env" | "md" | "markdown" => return None,
        "rs" => "rust",
        "py" | "pyw" => "python",
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "vue" => "javascript",
        "go" => "go",
        "java" | "kt" | "kts" | "scala" | "dart" | "gradle" | "swift" | "c" | "h" | "cc" | "cpp" | "hpp" | "cxx" | "m" | "mm" | "cs" => "java",
        "sh" | "bash" | "zsh" | "fish" => "shell",
        "sql" => "sql",
        "json" | "jsonc" => "json",
        "yaml" | "yml" | "toml" | "ini" | "conf" | "properties" => "yaml",
        "css" | "scss" | "less" => "css",
        "html" | "htm" | "xml" | "svg" | "plist" => "html",
        "mmd" | "mermaid" => "mermaid",
        "puml" | "plantuml" => "plantuml",
        "tex" => "latex",
        _ => "generic",
    })
}

/// 首行是 shebang 时猜语言（`#!/bin/bash`、`#!/usr/bin/env python3`）。
pub fn lang_for_shebang(first: &str) -> Option<&'static str> {
    let f = first.strip_prefix("#!")?;
    Some(if f.contains("python") {
        "python"
    } else if f.contains("node") {
        "javascript"
    } else if f.contains("sh") {
        "shell"
    } else {
        "generic"
    })
}

/// 记号的 CSS 颜色（HTML 预览用，与编辑器主题一致）。
pub fn tok_css(t: Tok, dark: bool) -> &'static str {
    match (t, dark) {
        (Tok::Keyword | Tok::Arrow, false) => "#a626a4",
        (Tok::String, false) => "#50a14f",
        (Tok::Comment, false) => "#a0a1a7",
        (Tok::Number, false) => "#986801",
        (Tok::Function, false) => "#4078f2",
        (Tok::Type, false) => "#c18401",
        (Tok::Keyword | Tok::Arrow, true) => "#c678dd",
        (Tok::String, true) => "#98c379",
        (Tok::Comment, true) => "#7f848e",
        (Tok::Number, true) => "#d19a66",
        (Tok::Function, true) => "#61afef",
        (Tok::Type, true) => "#e5c07b",
    }
}

/// 从开头起的一段连线箭头长度：由 `- . = < > | * o x { }` 组成、至少含两个 `-`/`.`/`=` 或一个 `>`，
/// 且 `o` / `x` / `{ }` 只能出现在两端（避免把单词里的字母当箭头）。
fn arrow_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let n = b.iter().take_while(|c| matches!(c, b'-' | b'.' | b'=' | b'<' | b'>' | b'|' | b'*' | b'o' | b'x' | b'}' | b'{')).count();
    if n < 2 {
        return None;
    }
    let seg = &b[..n];
    let body = seg.iter().filter(|c| matches!(c, b'-' | b'.' | b'=')).count();
    if body == 0 || (body < 2 && !seg.contains(&b'>')) {
        return None;
    }
    // 字母只允许在两端：中间出现说明是普通单词
    let inner = &seg[1..n - 1];
    if inner.iter().any(|c| matches!(c, b'o' | b'x')) {
        return None;
    }
    if b.get(n).is_some_and(|c| c.is_ascii_alphanumeric()) && matches!(seg[n - 1], b'o' | b'x') {
        return None;
    }
    if seg[0] == b'.' && body < 2 {
        return None;
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_tokens() {
        let toks = highlight("rust", "fn main() { let s = \"hi\"; // 注释\n}");
        assert!(toks.iter().any(|(_, t)| *t == Tok::Keyword));
        assert!(toks.iter().any(|(_, t)| *t == Tok::String));
        assert!(toks.iter().any(|(_, t)| *t == Tok::Comment));
        assert!(toks.iter().any(|(_, t)| *t == Tok::Function));
    }

    #[test]
    fn cached_matches_uncached() {
        let src = "fn a() {}\n/* x */";
        assert_eq!(*highlight_cached("rust", src), highlight("rust", src));
        assert!(Rc::ptr_eq(&highlight_cached("rust", src), &highlight_cached("rust", src)));
    }

    #[test]
    fn diagram_arrows() {
        let src = "A --> B\nB ->> C: hi\nfoo-bar";
        let toks = highlight("mermaid", src);
        let arrows: Vec<&str> = toks.iter().filter(|(_, t)| *t == Tok::Arrow).map(|(r, _)| &src[r.clone()]).collect();
        assert_eq!(arrows, ["-->", "->>"]);
        let p = "@startuml\n' 注释\nUser -> Gateway: 提交\n@enduml";
        let toks = highlight("plantuml", p);
        assert!(toks.iter().any(|(r, t)| *t == Tok::Comment && p[r.clone()].contains("注释")));
        assert!(toks.iter().any(|(r, t)| *t == Tok::Keyword && &p[r.clone()] == "startuml"));
    }
}
