//! 纯函数工具：函数目录（自动补全 / 参数提示）、公式里的引用解析、地址解析、中文序列、CSV。

/// 函数目录：名称、参数签名、中文说明（写法）。
pub struct Func {
    pub name: &'static str,
    pub args: &'static str,
    pub desc: &'static str,
}

macro_rules! funcs {
    ($( $n:literal, $a:literal, $d:literal; )*) => {
        pub const FUNCS: &[Func] = &[ $( Func { name: $n, args: $a, desc: $d } ),* ];
    };
}

funcs! {
    "SUM", "数值1, [数值2, ...]", "求和：返回所有参数之和";
    "AVERAGE", "数值1, [数值2, ...]", "求平均值";
    "COUNT", "值1, [值2, ...]", "统计包含数字的单元格个数";
    "COUNTA", "值1, [值2, ...]", "统计非空单元格个数";
    "COUNTBLANK", "区域", "统计空单元格个数";
    "COUNTIF", "区域, 条件", "统计满足条件的单元格个数";
    "COUNTIFS", "区域1, 条件1, [区域2, 条件2, ...]", "统计同时满足多个条件的单元格个数";
    "SUMIF", "区域, 条件, [求和区域]", "对满足条件的单元格求和";
    "SUMIFS", "求和区域, 区域1, 条件1, ...", "对同时满足多个条件的单元格求和";
    "AVERAGEIF", "区域, 条件, [平均值区域]", "对满足条件的单元格求平均";
    "AVERAGEIFS", "平均值区域, 区域1, 条件1, ...", "对同时满足多个条件的单元格求平均";
    "MAX", "数值1, [数值2, ...]", "返回最大值";
    "MIN", "数值1, [数值2, ...]", "返回最小值";
    "MAXIFS", "最大值区域, 区域1, 条件1, ...", "返回满足条件的最大值";
    "MINIFS", "最小值区域, 区域1, 条件1, ...", "返回满足条件的最小值";
    "MEDIAN", "数值1, [数值2, ...]", "返回中位数";
    "LARGE", "区域, k", "返回第 k 大的值";
    "SMALL", "区域, k", "返回第 k 小的值";
    "STDEV", "数值1, [数值2, ...]", "估算样本标准差";
    "ROUND", "数值, 小数位数", "四舍五入到指定位数";
    "ROUNDUP", "数值, 小数位数", "向上舍入";
    "ROUNDDOWN", "数值, 小数位数", "向下舍入";
    "INT", "数值", "向下取整";
    "TRUNC", "数值, [小数位数]", "截尾取整";
    "CEILING", "数值, 基数", "向上舍入到基数的倍数";
    "FLOOR", "数值, 基数", "向下舍入到基数的倍数";
    "ABS", "数值", "返回绝对值";
    "MOD", "被除数, 除数", "返回两数相除的余数";
    "POWER", "底数, 指数", "返回乘幂";
    "SQRT", "数值", "返回平方根";
    "PRODUCT", "数值1, [数值2, ...]", "返回所有参数的乘积";
    "SUMPRODUCT", "数组1, [数组2, ...]", "对应元素相乘后求和";
    "RAND", "", "返回 0 到 1 之间的随机数";
    "RANDBETWEEN", "下限, 上限", "返回两数之间的随机整数";
    "SUBTOTAL", "函数编号, 区域1, ...", "分类汇总（忽略已隐藏的行）";
    "IF", "条件, 为真时的值, [为假时的值]", "按条件返回不同的值";
    "IFS", "条件1, 值1, [条件2, 值2, ...]", "依次判断多个条件，返回第一个为真的值";
    "IFERROR", "值, 出错时的值", "公式出错时返回指定值";
    "IFNA", "值, 为 #N/A 时的值", "值为 #N/A 时返回指定值";
    "AND", "逻辑值1, [逻辑值2, ...]", "所有参数都为真时返回 TRUE";
    "OR", "逻辑值1, [逻辑值2, ...]", "任一参数为真时返回 TRUE";
    "NOT", "逻辑值", "对逻辑值取反";
    "XOR", "逻辑值1, [逻辑值2, ...]", "异或";
    "SWITCH", "表达式, 值1, 结果1, [默认值]", "按表达式的值返回对应结果";
    "VLOOKUP", "查找值, 区域, 列序号, [是否近似匹配]", "在区域首列查找，返回同一行指定列的值";
    "HLOOKUP", "查找值, 区域, 行序号, [是否近似匹配]", "在区域首行查找，返回同一列指定行的值";
    "XLOOKUP", "查找值, 查找区域, 返回区域, [未找到时], [匹配模式]", "在区域中查找并返回对应项";
    "LOOKUP", "查找值, 查找区域, [结果区域]", "在单行或单列中查找";
    "INDEX", "区域, 行号, [列号]", "返回区域中指定行列的值";
    "MATCH", "查找值, 区域, [匹配类型]", "返回查找值在区域中的位置";
    "XMATCH", "查找值, 区域, [匹配模式]", "返回查找值在区域中的位置";
    "CHOOSE", "序号, 值1, [值2, ...]", "按序号从参数中选一个值";
    "ROW", "[引用]", "返回行号";
    "COLUMN", "[引用]", "返回列号";
    "ROWS", "区域", "返回区域的行数";
    "COLUMNS", "区域", "返回区域的列数";
    "UNIQUE", "区域", "返回区域中的唯一值";
    "SORT", "区域, [排序列], [升降序]", "对区域排序";
    "FILTER", "区域, 条件, [为空时]", "按条件筛选区域";
    "SEQUENCE", "行数, [列数], [起始值], [步长]", "生成数字序列";
    "CONCATENATE", "文本1, [文本2, ...]", "连接多个文本";
    "CONCAT", "文本1, [文本2, ...]", "连接多个文本或区域";
    "TEXTJOIN", "分隔符, 忽略空值, 文本1, ...", "用分隔符连接多个文本";
    "LEFT", "文本, [字符数]", "从左侧截取字符";
    "RIGHT", "文本, [字符数]", "从右侧截取字符";
    "MID", "文本, 起始位置, 字符数", "从中间截取字符";
    "LEN", "文本", "返回文本长度";
    "FIND", "要找的文本, 文本, [起始位置]", "查找文本位置（区分大小写）";
    "SEARCH", "要找的文本, 文本, [起始位置]", "查找文本位置（不区分大小写）";
    "SUBSTITUTE", "文本, 旧文本, 新文本, [第几次]", "替换文本中的指定内容";
    "REPLACE", "文本, 起始位置, 字符数, 新文本", "按位置替换文本";
    "UPPER", "文本", "转为大写";
    "LOWER", "文本", "转为小写";
    "PROPER", "文本", "单词首字母大写";
    "TRIM", "文本", "去掉多余空格";
    "TEXT", "数值, 格式", "按格式把数值转为文本";
    "VALUE", "文本", "把文本转为数字";
    "REPT", "文本, 次数", "重复文本";
    "EXACT", "文本1, 文本2", "比较两个文本是否完全相同";
    "TODAY", "", "返回今天的日期";
    "NOW", "", "返回当前日期和时间";
    "DATE", "年, 月, 日", "由年月日组成日期";
    "YEAR", "日期", "返回年份";
    "MONTH", "日期", "返回月份";
    "DAY", "日期", "返回日";
    "HOUR", "时间", "返回小时";
    "MINUTE", "时间", "返回分钟";
    "SECOND", "时间", "返回秒";
    "WEEKDAY", "日期, [类型]", "返回星期几";
    "WEEKNUM", "日期, [类型]", "返回一年中的第几周";
    "EDATE", "开始日期, 月数", "返回几个月之前或之后的日期";
    "EOMONTH", "开始日期, 月数", "返回几个月之前或之后的月末日期";
    "DATEDIF", "开始日期, 结束日期, 单位", "计算两个日期的间隔";
    "NETWORKDAYS", "开始日期, 结束日期, [假期]", "计算两个日期间的工作日数";
    "WORKDAY", "开始日期, 天数, [假期]", "返回若干工作日之后的日期";
    "ISBLANK", "值", "是否为空";
    "ISNUMBER", "值", "是否为数字";
    "ISTEXT", "值", "是否为文本";
    "ISERROR", "值", "是否为错误值";
    "PMT", "利率, 期数, 现值, [终值], [类型]", "计算贷款每期还款额";
    "FV", "利率, 期数, 每期付款, [现值], [类型]", "计算投资的终值";
    "PV", "利率, 期数, 每期付款, [终值], [类型]", "计算投资的现值";
}

pub fn find(name: &str) -> Option<&'static Func> {
    FUNCS.iter().find(|f| f.name.eq_ignore_ascii_case(name))
}

fn is_op(c: char) -> bool {
    matches!(c, '=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | ' ' | '%')
}

/// 公式末尾正在输入的函数名前缀：返回 (前缀起点字节偏移, 前缀)。
pub fn completion_prefix(text: &str) -> Option<(usize, &str)> {
    if !text.starts_with('=') || in_string(text) {
        return None;
    }
    let start = text.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_')).map(|i| i + 1).unwrap_or(0);
    let prefix = &text[start..];
    let before = text[..start].chars().last()?;
    (!prefix.is_empty() && prefix.starts_with(|c: char| c.is_ascii_alphabetic()) && is_op(before)).then_some((start, prefix))
}

pub fn candidates(prefix: &str) -> Vec<&'static Func> {
    let p = prefix.to_ascii_uppercase();
    let mut v: Vec<&Func> = FUNCS.iter().filter(|f| f.name.starts_with(&p)).collect();
    v.sort_by_key(|f| (f.name != p, f.name.len()));
    v.truncate(8);
    v
}

/// 是否处在字符串字面量里（双引号个数为奇数）。
fn in_string(text: &str) -> bool {
    text.chars().filter(|&c| c == '"').count() % 2 == 1
}

/// 光标（末尾）所在的最内层函数调用：返回函数与当前是第几个参数（从 0 开始）。
pub fn current_call(text: &str) -> Option<(&'static Func, usize)> {
    if !text.starts_with('=') {
        return None;
    }
    let mut depth = 0i32;
    let mut arg = 0usize;
    let mut quote = false;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    for &(i, c) in bytes.iter().rev() {
        match c {
            '"' => quote = !quote,
            _ if quote => {}
            ')' => depth += 1,
            ',' if depth == 0 => arg += 1,
            '(' if depth == 0 => {
                let head = &text[..i];
                let s = head.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_')).map(|i| i + 1).unwrap_or(0);
                return find(&head[s..]).map(|f| (f, arg));
            }
            '(' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// 末尾是否可以插入一个引用（输入 `=`、运算符、`(`、`,` 后点单元格会插入引用）。
pub fn expects_ref(text: &str) -> bool {
    text.starts_with('=')
        && !in_string(text)
        && text.trim_end().chars().last().is_some_and(|c| matches!(c, '=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':'))
}

/// "B12" → (行 12, 列 2)；支持 `$`。
pub fn parse_addr(s: &str) -> Option<(i32, i32)> {
    let s = s.trim().replace('$', "");
    let col: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if col.is_empty() || col.len() > 3 {
        return None;
    }
    let row: i32 = s[col.len()..].parse().ok()?;
    let c = col.to_ascii_uppercase().bytes().fold(0i32, |acc, b| acc * 26 + (b - b'A' + 1) as i32);
    (row >= 1 && c >= 1).then_some((row, c))
}

/// "A1:C5" / "B2" → (r0, c0, r1, c1)
pub fn parse_range(s: &str) -> Option<(i32, i32, i32, i32)> {
    let (a, b) = s.split_once(':').unwrap_or((s, s));
    let (r0, c0) = parse_addr(a)?;
    let (r1, c1) = parse_addr(b)?;
    Some((r0.min(r1), c0.min(c1), r0.max(r1), c0.max(c1)))
}

/// 公式里本表的单元格 / 区域引用（跳过字符串、跨表引用和函数名），按出现顺序。
/// 返回 (r0, c0, r1, c1, 起始字节, 结束字节)。
pub fn parse_refs(text: &str) -> Vec<(i32, i32, i32, i32, usize, usize)> {
    let mut out = vec![];
    if !text.starts_with('=') {
        return out;
    }
    let b = text.as_bytes();
    let mut i = 1;
    let mut quote = false;
    let token = |mut j: usize| -> Option<usize> {
        // [$]字母{1,3}[$]数字+
        if j < b.len() && b[j] == b'$' {
            j += 1;
        }
        let s = j;
        while j < b.len() && b[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j == s || j - s > 3 {
            return None;
        }
        if j < b.len() && b[j] == b'$' {
            j += 1;
        }
        let d = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        (j > d).then_some(j)
    };
    while i < b.len() {
        let ch = b[i];
        if ch == b'"' {
            quote = !quote;
            i += 1;
            continue;
        }
        let prev_ok = !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'!' || b[i - 1] == b'.' || b[i - 1] == b'$');
        if !quote && prev_ok && (ch == b'$' || ch.is_ascii_alphabetic()) {
            if let Some(mut e) = token(i) {
                // 后面紧跟 `(` 的是函数名（如 LOG10()），跟字母的是更长的名字
                if e < b.len() && (b[e] == b'(' || b[e].is_ascii_alphabetic() || b[e] == b'_') {
                    i = e;
                    continue;
                }
                let mut end = e;
                if e < b.len() && b[e] == b':' {
                    if let Some(e2) = token(e + 1) {
                        end = e2;
                        e = e2;
                    }
                }
                if let Some((r0, c0, r1, c1)) = parse_range(&text[i..end]) {
                    out.push((r0, c0, r1, c1, i, end));
                }
                i = e;
                continue;
            }
            // 跳过整个标识符
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    out
}

/// 引用高亮的配色：蓝、红、紫、绿、橙、青、粉。
pub const REF_COLORS: [u32; 7] = [0x3370ff, 0xf54a45, 0x7f3bf5, 0x2ea121, 0xff8800, 0x14c0ff, 0xf01d94];

const SERIES: [&[&str]; 7] = [
    &["一月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "十一月", "十二月"],
    &["星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日"],
    &["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
    &["第一季度", "第二季度", "第三季度", "第四季度"],
    &["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"],
    &["子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥"],
    &["一", "二", "三", "四", "五", "六", "七", "八", "九", "十"],
];

/// 中文序列（月份 / 星期 / 季度 / 天干地支）填充：返回源值之后的第 n 项。
pub fn series_next(v: &str, n: usize) -> Option<String> {
    for s in SERIES {
        if let Some(i) = s.iter().position(|x| *x == v) {
            return Some(s[(i + n) % s.len()].to_string());
        }
    }
    None
}

pub fn csv_to_tsv(csv: &str) -> String {
    let mut out = String::new();
    let mut in_q = false;
    let mut chars = csv.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q && chars.peek() == Some(&'"') => {
                chars.next();
                out.push('"');
            }
            '"' => in_q = !in_q,
            ',' if !in_q => out.push('\t'),
            '\t' => out.push(' '),
            '\n' if in_q => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// 估算文字宽度（用于自适应列宽 / 行高）：汉字约 1 个字号宽，ASCII 约 0.58 个。
pub fn text_width_est(s: &str, px: f32) -> f32 {
    s.chars().map(|c| if c.is_ascii() { 0.58 } else { 1.0 }).sum::<f32>() * px
}

/// 把数值格式化成状态栏里的样子：去掉多余的 0。
pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0. && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_skip_functions_and_strings() {
        let r = parse_refs("=SUM(A1:B3)+LOG10(C2)+\"D4\"+$E$5");
        let got: Vec<_> = r.iter().map(|x| (x.0, x.1, x.2, x.3)).collect();
        assert_eq!(got, vec![(1, 1, 3, 2), (2, 3, 2, 3), (5, 5, 5, 5)]);
    }

    #[test]
    fn completion_and_call() {
        assert_eq!(completion_prefix("=SU"), Some((1, "SU")));
        assert_eq!(completion_prefix("=A1+vl"), Some((4, "vl")));
        assert_eq!(completion_prefix("abc"), None);
        assert_eq!(candidates("su")[0].name, "SUM");
        let (f, arg) = current_call("=IF(A1>2,SUM(B1,").unwrap();
        assert_eq!((f.name, arg), ("SUM", 1));
        let (f, arg) = current_call("=IF(A1>2,SUM(B1),").unwrap();
        assert_eq!((f.name, arg), ("IF", 2));
        assert!(expects_ref("=SUM("));
        assert!(!expects_ref("=SUM(A1"));
    }

    #[test]
    fn addr() {
        assert_eq!(parse_addr("b12"), Some((12, 2)));
        assert_eq!(parse_addr("$AA$3"), Some((3, 27)));
        assert_eq!(parse_range("C5:A1"), Some((1, 1, 5, 3)));
        assert_eq!(series_next("周六", 2), Some("周一".into()));
    }
}
