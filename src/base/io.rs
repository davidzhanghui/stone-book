//! 导入导出：文档导出时的 Markdown / HTML 表格，CSV 导出，CSV / Excel 导入（自动识别字段类型）。

use super::formula;
use super::model::*;
use serde_json::Value;

/// 当前数据表当前视图的 (可见字段名, 每行文本)。
fn active_grid(b: &Base, limit: usize) -> Option<(String, Vec<String>, Vec<Vec<String>>)> {
    let t = b.table(&b.meta.active).or_else(|| b.table_list().into_iter().next())?;
    let tid = t.meta.id.clone();
    let v = t.views.get(&t.meta.active_view).or_else(|| t.meta.views.first().and_then(|v| t.views.get(v)))?;
    let fields = t.visible_fields(v);
    let names = fields.iter().filter_map(|f| t.fields.get(f).map(|x| x.name.clone())).collect();
    let rows = b.view_records(&tid, v, "").iter().take(limit).map(|r| fields.iter().map(|f| b.text(&tid, r, f)).collect()).collect();
    Some((t.meta.name.clone(), names, rows))
}

pub fn to_markdown(b: &Base) -> String {
    let Some((_, names, rows)) = active_grid(b, 500) else { return String::new() };
    let esc = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let mut out = format!("| {} |\n|{}\n", names.iter().map(|n| esc(n)).collect::<Vec<_>>().join(" | "), " --- |".repeat(names.len()));
    for r in rows {
        out.push_str(&format!("| {} |\n", r.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")));
    }
    out
}

pub fn to_html(b: &Base) -> String {
    let Some((name, names, rows)) = active_grid(b, 2000) else { return String::new() };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;");
    let mut out = format!("<table border=\"1\" style=\"border-collapse:collapse\"><caption>{}</caption><tr>", esc(&name));
    for n in names {
        out.push_str(&format!("<th>{}</th>", esc(&n)));
    }
    out.push_str("</tr>");
    for r in rows {
        out.push_str("<tr>");
        for c in r {
            out.push_str(&format!("<td>{}</td>", esc(&c)));
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

pub fn csv_escape(v: &str) -> String {
    if v.contains([',', '"', '\n']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

/// 视图导出为 CSV（带 BOM，Excel 直接打开不乱码）。
pub fn view_csv(b: &Base, tid: &str, v: &View) -> String {
    let Some(t) = b.table(tid) else { return String::new() };
    let fields = t.visible_fields(v);
    let mut out = String::from("\u{feff}");
    out.push_str(&fields.iter().filter_map(|f| t.fields.get(f)).map(|f| csv_escape(&f.name)).collect::<Vec<_>>().join(","));
    out.push('\n');
    for r in b.view_records(tid, v, "") {
        out.push_str(&fields.iter().map(|f| csv_escape(&b.text(tid, &r, f))).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let sep = if text.lines().next().is_some_and(|l| l.contains('\t') && !l.contains(',')) { '\t' } else { ',' };
    let mut rows = vec![];
    let mut row = vec![];
    let mut cell = String::new();
    let mut q = false;
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if q && it.peek() == Some(&'"') => {
                it.next();
                cell.push('"');
            }
            '"' if cell.is_empty() || q => q = !q,
            c if c == sep && !q => row.push(std::mem::take(&mut cell)),
            '\r' if !q => {}
            '\n' if !q => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    rows
}

/// 根据一列的取值猜字段类型。
pub fn infer_type(vals: &[&str]) -> FieldType {
    let vals: Vec<&str> = vals.iter().map(|v| v.trim()).filter(|v| !v.is_empty()).collect();
    if vals.is_empty() {
        return FieldType::Text;
    }
    let all = |f: &dyn Fn(&str) -> bool| vals.iter().all(|v| f(v));
    if all(&|v| matches!(v, "true" | "false" | "TRUE" | "FALSE" | "是" | "否" | "✓" | "✔" | "√")) {
        return FieldType::Checkbox;
    }
    if all(&|v| v.replace([',', '¥', '$'], "").parse::<f64>().is_ok()) {
        return if all(&|v| v.starts_with('¥') || v.starts_with('$')) { FieldType::Currency } else { FieldType::Number };
    }
    if all(&|v| v.ends_with('%') && v.trim_end_matches('%').parse::<f64>().is_ok()) {
        return FieldType::Progress;
    }
    if all(&|v| v.chars().any(|c| c.is_ascii_digit()) && (v.contains('-') || v.contains('/') || v.contains('年')) && formula::parse_date(v).is_some()) {
        return FieldType::Date;
    }
    if all(&|v| v.starts_with("http://") || v.starts_with("https://")) {
        return FieldType::Url;
    }
    if all(&|v| v.contains('@') && v.contains('.') && !v.contains(' ')) {
        return FieldType::Email;
    }
    let distinct: std::collections::HashSet<&str> = vals.iter().copied().collect();
    let avg = vals.iter().map(|v| v.chars().count()).sum::<usize>() / vals.len();
    // 有重复取值、种类不多的短文本 → 单选
    if vals.len() >= 4 && distinct.len() < vals.len() && distinct.len() <= (vals.len() / 2).clamp(2, 30) && avg <= 12 {
        return FieldType::Select;
    }
    FieldType::Text
}

/// 把一个二维文本表格（首行为表头）导入为新的数据表，返回数据表 id。
pub fn import_rows(b: &mut Base, name: &str, rows: Vec<Vec<String>>) -> Option<String> {
    let header = rows.first()?.clone();
    let body = &rows[1..];
    let ncol = header.len().max(body.iter().map(|r| r.len()).max().unwrap_or(0));
    if ncol == 0 {
        return None;
    }
    let tid = b.add_table(name, ViewKind::Grid, 0);
    // 换掉默认字段
    let defaults = b.table(&tid)?.meta.fields.clone();
    for f in &defaults {
        b.del_field(&tid, f);
    }
    let mut fields = vec![];
    for c in 0..ncol {
        let col: Vec<&str> = body.iter().map(|r| r.get(c).map(|s| s.as_str()).unwrap_or("")).collect();
        let fname = header.get(c).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("字段 {}", c + 1));
        let kind = if c == 0 && infer_type(&col) != FieldType::Number { FieldType::Text } else { infer_type(&col) };
        let mut f = Field::new(&fname, kind);
        if kind == FieldType::Currency {
            f.prop.symbol = col.iter().find(|v| !v.is_empty()).map(|v| if v.starts_with('$') { "$" } else { "¥" }).unwrap_or("¥").into();
        }
        fields.push(f);
    }
    let mut cells: Vec<Vec<Option<Value>>> = vec![];
    for r in body {
        let mut row = vec![];
        for (c, f) in fields.iter_mut().enumerate() {
            let s = r.get(c).map(|s| s.trim()).unwrap_or("");
            row.push(if s.is_empty() { None } else { text_to_value(f, s) });
        }
        cells.push(row);
    }
    let mut meta = b.table(&tid)?.meta.clone();
    meta.fields = fields.iter().map(|f| f.id.clone()).collect();
    b.put_table(meta);
    for f in &fields {
        b.put_field(&tid, f.clone());
    }
    for row in cells {
        let Some(mut r) = b.new_record(&tid, None) else { continue };
        for (c, v) in row.into_iter().enumerate() {
            if let Some(v) = v {
                r.cells.insert(fields[c].id.clone(), v);
            }
        }
        b.put_record(&tid, r);
    }
    Some(tid)
}

/// 把用户输入 / 粘贴的文本转换为某字段的值（选项不存在会新建）。
pub fn text_to_value(f: &mut Field, s: &str) -> Option<Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    use FieldType::*;
    match f.kind {
        Number | Currency => s.replace([',', '¥', '$', '€', '£'], "").trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number),
        Rating => {
            let max = if f.prop.max > 0. { f.prop.max } else { 5. };
            let n: f64 = s.trim().parse().ok()?;
            serde_json::Number::from_f64(n.round().clamp(0., max)).map(Value::Number)
        }
        Progress => {
            let n: f64 = s.trim_end_matches('%').trim().parse().ok()?;
            serde_json::Number::from_f64(if s.ends_with('%') || n > 1. { n / 100. } else { n }).map(Value::Number)
        }
        Checkbox => Some(Value::Bool(matches!(s, "true" | "TRUE" | "是" | "✓" | "✔" | "√" | "1" | "已完成"))),
        Date => formula::parse_date(s).map(Value::from),
        Select | Stage => Some(Value::from(f.ensure_opt(s))),
        MultiSelect => Some(Value::from(s.split([',', '，', '、']).map(|p| p.trim()).filter(|p| !p.is_empty()).map(|p| f.ensure_opt(p)).collect::<Vec<_>>())),
        Person => Some(Value::from(s.split([',', '，', '、']).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect::<Vec<_>>())),
        Attachment | Link | DuplexLink | Lookup | Formula | AutoNumber | CreatedTime | ModifiedTime => None,
        _ => Some(Value::from(s)),
    }
}

/// 把二维文本表格（首行为表头）追加到已有数据表：按列名匹配字段，没有的列新建字段（类型自动识别），
/// 值按目标字段类型转换（关联字段按主字段文本匹配）。返回追加的记录数。
pub fn append_rows(b: &mut Base, tid: &str, rows: Vec<Vec<String>>) -> usize {
    let Some(header) = rows.first().cloned() else { return 0 };
    let body = &rows[1..];
    let mut cols: Vec<Option<Field>> = vec![];
    for (c, name) in header.iter().enumerate() {
        let name = name.trim();
        let existing = b.table(tid).and_then(|t| t.field_by_name(name)).cloned();
        let f = match existing {
            Some(f) if f.kind.computed() || f.kind == FieldType::Attachment => None,
            Some(f) => Some(f),
            None if name.is_empty() => None,
            None => {
                let col: Vec<&str> = body.iter().map(|r| r.get(c).map(|s| s.as_str()).unwrap_or("")).collect();
                let f = Field::new(name, infer_type(&col));
                let Some(mut m) = b.table(tid).map(|t| t.meta.clone()) else { return 0 };
                m.fields.push(f.id.clone());
                b.put_table(m);
                b.put_field(tid, f.clone());
                Some(f)
            }
        };
        cols.push(f);
    }
    let mut n = 0;
    for r in body {
        let Some(rec) = b.new_record(tid, None) else { break };
        n += 1;
        for (c, f) in cols.iter_mut().enumerate() {
            let (Some(f), Some(text)) = (f.as_mut(), r.get(c)) else { continue };
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let v = if f.kind.is_link() {
                let target = f.prop.table.clone();
                let ids: Vec<String> = text
                    .split([',', '，', '、'])
                    .filter_map(|n| b.table(&target)?.order.iter().find(|x| b.primary_text(&target, x) == n.trim()).cloned())
                    .collect();
                Value::from(ids)
            } else {
                text_to_value(f, text).unwrap_or(Value::Null)
            };
            b.set_cell(tid, &rec.id, &f.id, v);
        }
    }
    for f in cols.into_iter().flatten() {
        if f.kind.has_options() {
            b.put_field(tid, f);
        }
    }
    n
}

/// 读取 xlsx 的全部工作表：(工作表名, 二维文本)。
pub fn read_xlsx_sheets(path: &str) -> Vec<(String, Vec<Vec<String>>)> {
    let Ok(model) = ironcalc::import::load_from_xlsx(path, "en", "UTC", "en") else { return vec![] };
    let user = ironcalc::base::UserModel::from_model(model);
    let names = user.get_model().workbook.get_worksheet_names();
    let mut out = vec![];
    for (i, name) in names.into_iter().enumerate() {
        let Ok(ws) = user.get_model().workbook.worksheet(i as u32) else { continue };
        let dim = ws.dimension();
        let mut rows = vec![];
        for r in 1..=dim.max_row.min(20000) {
            rows.push((1..=dim.max_column.min(300)).map(|c| user.get_formatted_cell_value(i as u32, r, c).unwrap_or_default()).collect::<Vec<String>>());
        }
        rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
        if !rows.is_empty() {
            out.push((name, rows));
        }
    }
    out
}

/// 导出 xlsx：每个 (数据表, 视图) 一个工作表；数字、货币、进度、评分写成数字，其余写显示文本。
pub fn export_xlsx(b: &Base, sheets: &[(String, View)], path: &str) -> Result<(), String> {
    let mut model = ironcalc::base::Model::new_empty("Sheet1", "en", "UTC", "en")?;
    let mut used: Vec<String> = vec![];
    for (i, (tid, v)) in sheets.iter().enumerate() {
        let Some(t) = b.table(tid) else { continue };
        let mut name: String = t.meta.name.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(28).collect();
        if name.is_empty() || used.contains(&name) {
            name = format!("{name}{}", i + 1);
        }
        used.push(name.clone());
        if i == 0 {
            model.rename_sheet_by_index(0, &name)?;
        } else {
            model.add_sheet(&name)?;
        }
        let sheet = i as u32;
        let fields = t.visible_fields(v);
        for (c, f) in fields.iter().enumerate() {
            model.update_cell_with_text(sheet, 1, c as i32 + 1, &t.fields[f].name)?;
        }
        for (r, rid) in b.view_records(tid, v, "").iter().enumerate() {
            for (c, fid) in fields.iter().enumerate() {
                let f = &t.fields[fid];
                let (row, col) = (r as i32 + 2, c as i32 + 1);
                let num = match (f.kind.numeric() || matches!(f.kind, FieldType::Formula | FieldType::Lookup), b.fval(tid, rid, fid)) {
                    (true, super::formula::FVal::Num(n)) => Some(n),
                    _ => None,
                };
                match num {
                    Some(n) if f.kind != FieldType::AutoNumber || f.prop.prefix.is_empty() => model.update_cell_with_number(sheet, row, col, n)?,
                    _ => {
                        let text = b.text(tid, rid, fid);
                        if !text.is_empty() {
                            model.update_cell_with_text(sheet, row, col, &text)?;
                        }
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_file(path);
    ironcalc::export::save_to_xlsx(&model, path).map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_roundtrip_and_inference() {
        let rows = parse_csv(
            "姓名,年龄,入职,部门,完成\n张三,30,2024-01-02,研发,是\n李四,28,2023/5/6,研发,否\n\"王,五\",35,2022-03-04,市场,是\n赵六,41,2020-07-08,研发,否\n",
        );
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[3][0], "王,五");
        let mut b = Base::new_default("t", ViewKind::Grid);
        b.persist = false;
        let tid = import_rows(&mut b, "导入", rows).unwrap();
        b.commit();
        let t = b.table(&tid).unwrap();
        let kinds: Vec<FieldType> = t.field_list().iter().map(|f| f.kind).collect();
        assert_eq!(kinds, vec![FieldType::Text, FieldType::Number, FieldType::Date, FieldType::Select, FieldType::Checkbox]);
        assert_eq!(t.records.len(), 4);
        let v = t.views.values().next().unwrap().clone();
        let csv = view_csv(&b, &tid, &v);
        assert!(csv.contains("\"王,五\",35,2022/03/04,市场,是"));
    }
}
