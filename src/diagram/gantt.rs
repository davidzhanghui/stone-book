//! 甘特图：`gantt`。

use super::*;
use crate::canvas::{pt, rect, Anchor, Arrow, Item, LineItem, ShapeItem, ShapeKind, TextItem};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use std::collections::HashMap;

struct Task {
    name: String,
    section: usize,
    start: NaiveDateTime,
    end: NaiveDateTime,
    done: bool,
    active: bool,
    crit: bool,
    milestone: bool,
}

fn parse_dt(s: &str, fmt: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    let f = fmt.replace("YYYY", "%Y").replace("MM", "%m").replace("DD", "%d").replace("HH", "%H").replace("mm", "%M").replace("ss", "%S");
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, &f) {
        return Some(dt);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, &f) {
        return d.and_hms_opt(0, 0, 0);
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0))
}

fn parse_dur(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.find(|c: char| c.is_alphabetic())?);
    let n: f64 = num.trim().parse().ok()?;
    Some(match unit {
        "d" => Duration::minutes((n * 1440.) as i64),
        "w" => Duration::minutes((n * 7. * 1440.) as i64),
        "h" => Duration::minutes((n * 60.) as i64),
        "m" => Duration::minutes(n as i64),
        "s" => Duration::seconds(n as i64),
        _ => return None,
    })
}

pub fn render(body: &[(usize, String)], theme: &Theme, m: &mut dyn Measure) -> DResult<Scene> {
    let mut fmt = "YYYY-MM-DD".to_string();
    let mut title = String::new();
    let mut sections: Vec<String> = vec![];
    let mut tasks: Vec<Task> = vec![];
    let mut ids: HashMap<String, usize> = HashMap::new();
    for (line, raw) in body {
        let line = *line;
        let s = raw.trim();
        let lower = s.to_lowercase();
        if let Some(t) = s.strip_prefix("title") {
            title = t.trim().to_string();
            continue;
        }
        if lower.starts_with("dateformat") {
            fmt = s[10..].trim().to_string();
            continue;
        }
        if lower.starts_with("axisformat")
            || lower.starts_with("excludes")
            || lower.starts_with("todaymarker")
            || lower.starts_with("tickinterval")
            || lower.starts_with("weekday")
            || lower.starts_with("inclusiveenddates")
        {
            continue;
        }
        if let Some(sec) = s.strip_prefix("section") {
            sections.push(sec.trim().to_string());
            continue;
        }
        let Some((name, spec)) = s.split_once(':') else {
            return Err(DiagramError::new(line, format!("任务应为「名称 : 参数」：{s}")));
        };
        if sections.is_empty() {
            sections.push(String::new());
        }
        let mut parts: Vec<&str> = spec.split(',').map(|p| p.trim()).collect();
        let mut t = Task {
            name: name.trim().to_string(),
            section: sections.len() - 1,
            start: NaiveDateTime::default(),
            end: NaiveDateTime::default(),
            done: false,
            active: false,
            crit: false,
            milestone: false,
        };
        // 状态标签在最前面
        while let Some(&p) = parts.first() {
            match p {
                "done" => t.done = true,
                "active" => t.active = true,
                "crit" => t.crit = true,
                "milestone" => t.milestone = true,
                _ => break,
            }
            parts.remove(0);
        }
        // 剩余：[id,] start, end|duration  或  [id,] duration（接在上一个任务之后）
        let mut id = None;
        if parts.len() == 3 || (parts.len() == 2 && parse_dur(parts[1]).is_some() && parse_dt(parts[0], &fmt).is_none() && !parts[0].starts_with("after")) {
            id = Some(parts.remove(0).to_string());
        }
        let prev_end = tasks.last().map(|t| t.end);
        let start = match parts.first() {
            Some(p) if parts.len() == 2 => {
                if let Some(dep) = p.strip_prefix("after") {
                    let mut end = None;
                    for d in dep.split_whitespace() {
                        let e = ids.get(d).map(|&i| tasks[i].end).ok_or_else(|| DiagramError::new(line, format!("找不到任务 {d}")))?;
                        end = Some(end.map_or(e, |x: NaiveDateTime| x.max(e)));
                    }
                    end
                } else {
                    parse_dt(p, &fmt)
                }
            }
            _ => prev_end,
        };
        let start = start.ok_or_else(|| DiagramError::new(line, "无法确定开始时间（第一个任务需要写日期）"))?;
        let last = parts.last().copied().unwrap_or("1d");
        let end = parse_dur(last)
            .map(|d| start + d)
            .or_else(|| parse_dt(last, &fmt))
            .ok_or_else(|| DiagramError::new(line, format!("无法解析时长或结束日期：{last}")))?;
        t.start = start;
        t.end = end.max(start);
        if let Some(id) = id {
            ids.insert(id, tasks.len());
        }
        tasks.push(t);
    }
    if tasks.is_empty() {
        return Err(DiagramError::new(0, "甘特图至少需要一个任务"));
    }
    let fs = theme.font_size - 1.;
    let t0 = tasks.iter().map(|t| t.start).min().unwrap();
    let t1 = tasks.iter().map(|t| t.end).max().unwrap();
    let span_days = ((t1 - t0).num_minutes() as f32 / 1440.).max(1.);
    let chart_w = (span_days * 28.).clamp(480., 1100.);
    let px_per_day = chart_w / span_days;
    let label_w = sections.iter().map(|s| m.width(s, fs, true)).fold(0., f32::max).max(60.) + 24.;
    let row_h = 30.;
    let top = if title.is_empty() { 34. } else { 64. };
    let x_of = |d: NaiveDateTime| label_w + ((d - t0).num_minutes() as f32 / 1440.) * px_per_day;
    let mut scene = Scene::default();
    let total_h = tasks.len() as f32 * row_h;
    // 分区色带
    let mut row = 0usize;
    for (si, name) in sections.iter().enumerate() {
        let n = tasks.iter().filter(|t| t.section == si).count();
        if n == 0 {
            continue;
        }
        let mut band = ShapeItem::new(ShapeKind::Rect, rect(0., top + row as f32 * row_h, label_w + chart_w, n as f32 * row_h));
        band.fill = Some(theme.palette[si % 8].with_alpha(if theme.dark { 0.12 } else { 0.07 }));
        band.stroke = None;
        scene.items.push(Item::Shape(band));
        scene.items.push(Item::Text(TextItem {
            pos: pt(10., top + row as f32 * row_h + n as f32 * row_h / 2. + fs * 0.35),
            text: name.clone(),
            size: fs,
            color: theme.text,
            bold: true,
            anchor: Anchor::Start,
        }));
        row += n;
    }
    // 刻度
    let step_days = if span_days <= 14. {
        1
    } else if span_days <= 70. {
        7
    } else {
        30
    };
    let mut d = t0.date();
    if step_days == 7 {
        d -= Duration::days(d.weekday().num_days_from_monday() as i64);
    } else if step_days == 30 {
        d = d.with_day(1).unwrap_or(d);
    }
    while d.and_hms_opt(0, 0, 0).unwrap() <= t1 {
        let x = x_of(d.and_hms_opt(0, 0, 0).unwrap());
        if x >= label_w - 0.5 {
            let mut l = LineItem::new(vec![pt(x, top - 6.), pt(x, top + total_h)]);
            l.stroke = theme.muted.with_alpha(0.35);
            l.width = 1.;
            l.end = Arrow::None;
            scene.items.push(Item::Line(l));
            let label = if step_days == 30 { d.format("%Y-%m").to_string() } else { d.format("%m-%d").to_string() };
            scene.items.push(Item::Text(TextItem {
                pos: pt(x, top - 12.),
                text: label,
                size: fs - 2.,
                color: theme.muted,
                bold: false,
                anchor: Anchor::Middle,
            }));
        }
        d = if step_days == 30 {
            let (y, mo) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
            NaiveDate::from_ymd_opt(y, mo, 1).unwrap()
        } else {
            d + Duration::days(step_days)
        };
    }
    // 今天
    let today = chrono::Local::now().naive_local();
    if today >= t0 && today <= t1 {
        let x = x_of(today);
        let mut l = LineItem::new(vec![pt(x, top - 6.), pt(x, top + total_h)]);
        l.stroke = theme.palette[3];
        l.width = 1.5;
        l.end = Arrow::None;
        scene.items.push(Item::Line(l));
    }
    // 任务条（按分区顺序排列）
    let mut order: Vec<usize> = (0..tasks.len()).collect();
    order.sort_by_key(|&i| tasks[i].section);
    for (r, &i) in order.iter().enumerate() {
        let t = &tasks[i];
        let y = top + r as f32 * row_h;
        let (x0, x1) = (x_of(t.start), x_of(t.end));
        let color = if t.crit {
            theme.palette[3]
        } else if t.done {
            theme.muted
        } else if t.active {
            theme.palette[0]
        } else {
            theme.palette[t.section % 8]
        };
        if t.milestone {
            let mut s = ShapeItem::new(ShapeKind::Diamond, rect(x0 - 9., y + 6., 18., 18.));
            s.fill = Some(color);
            s.stroke = Some(color);
            scene.items.push(Item::Shape(s));
            scene.items.push(Item::Text(TextItem {
                pos: pt(x0 + 14., y + row_h / 2. + fs * 0.35),
                text: t.name.clone(),
                size: fs,
                color: theme.text,
                bold: false,
                anchor: Anchor::Start,
            }));
            continue;
        }
        let w = (x1 - x0).max(4.);
        let mut s = ShapeItem::new(ShapeKind::RoundRect, rect(x0, y + 5., w, row_h - 10.));
        s.radius = 4.;
        s.fill = Some(if t.done { color.with_alpha(0.45) } else { color.with_alpha(if t.active { 0.95 } else { 0.8 }) });
        s.stroke = if t.active || t.crit { Some(color) } else { None };
        s.seed = 500 + i as u64;
        scene.items.push(Item::Shape(s));
        let tw = m.width(&t.name, fs, false);
        let (tx, anchor, col) = if tw + 12. < w { (x0 + w / 2., Anchor::Middle, Col::rgb(0xffffff)) } else { (x0 + w + 6., Anchor::Start, theme.text) };
        scene.items.push(Item::Text(TextItem { pos: pt(tx, y + row_h / 2. + fs * 0.35), text: t.name.clone(), size: fs, color: col, bold: false, anchor }));
    }
    if !title.is_empty() {
        scene.items.push(Item::Text(TextItem {
            pos: pt((label_w + chart_w) / 2., 26.),
            text: title,
            size: fs + 4.,
            color: theme.text,
            bold: true,
            anchor: Anchor::Middle,
        }));
    }
    scene.width = label_w + chart_w + 120.;
    scene.height = top + total_h + 16.;
    Ok(scene)
}
