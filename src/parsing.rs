use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::types::{Level, LogRecord, TemplateSummary};

// ── Serilog template rendering ────────────────────────────────────────────────

pub fn render_template(template: &str, obj: &serde_json::Map<String, Value>) -> String {
    let mut out = String::with_capacity(template.len() * 2);
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' if i + 1 < chars.len() && chars[i + 1] == '{' => { out.push('{'); i += 2; }
            '}' if i + 1 < chars.len() && chars[i + 1] == '}' => { out.push('}'); i += 2; }
            '{' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != '}' { j += 1; }
                if j < chars.len() {
                    let token: String = chars[start..j].iter().collect();
                    let name = token.trim_start_matches('@').split(':').next().unwrap_or(&token);
                    let val = obj.get(name)
                        .map(value_to_display)
                        .unwrap_or_else(|| format!("{{{}}}", token));
                    out.push_str(&val);
                    i = j + 1;
                } else {
                    out.push('{');
                    i += 1;
                }
            }
            c => { out.push(c); i += 1; }
        }
    }
    out
}

pub fn value_to_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null      => "null".into(),
        Value::Bool(b)   => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Object(map) => {
            let parts: Vec<String> = map.iter()
                .filter(|(k, _)| k.as_str() != "$type")
                .map(|(k, v)| format!("{}:{}", k, value_to_display_quoted(v)))
                .collect();
            if parts.is_empty() {
                map.get("$type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                format!("{{{}}}", parts.join(", "))
            }
        }
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_display_quoted).collect();
            format!("[{}]", items.join(", "))
        }
    }
}

fn value_to_display_quoted(v: &Value) -> String {
    match v {
        Value::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
        other => value_to_display(other),
    }
}

/// Splits a rendered template into segments: (text, is_dynamic).
/// Fixed template text → false, interpolated values → true.
pub fn template_segments(template: &str, obj: &serde_json::Map<String, Value>) -> Vec<(String, bool)> {
    let mut segs = Vec::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    let mut fixed = String::new();
    while i < chars.len() {
        match chars[i] {
            '{' if i + 1 < chars.len() && chars[i + 1] == '{' => { fixed.push('{'); i += 2; }
            '}' if i + 1 < chars.len() && chars[i + 1] == '}' => { fixed.push('}'); i += 2; }
            '{' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != '}' { j += 1; }
                if j < chars.len() {
                    if !fixed.is_empty() { segs.push((std::mem::take(&mut fixed), false)); }
                    let token: String = chars[start..j].iter().collect();
                    let name = token.trim_start_matches('@').split(':').next().unwrap_or(&token);
                    let val = obj.get(name)
                        .map(value_to_display)
                        .unwrap_or_else(|| format!("{{{}}}", token));
                    segs.push((val, true));
                    i = j + 1;
                } else {
                    fixed.push('{');
                    i += 1;
                }
            }
            c => { fixed.push(c); i += 1; }
        }
    }
    if !fixed.is_empty() { segs.push((fixed, false)); }
    segs
}

// ── Timestamp helpers ─────────────────────────────────────────────────────────

pub fn parse_utc(ts: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) { return Some(dt.with_timezone(&Utc)); }
    for fmt in &["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(ts, fmt) {
            return Some(Utc.from_utc_datetime(&n));
        }
    }
    None
}

pub fn fmt_local(dt: &DateTime<Utc>) -> String {
    dt.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string()
}

// ── CLEF parsing ──────────────────────────────────────────────────────────────

pub fn parse_clef_line(line: &str, line_no: usize) -> Option<LogRecord> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let obj = v.as_object()?;

    let timestamp_utc = obj.get("@t").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let dt_utc = parse_utc(&timestamp_utc);
    let timestamp_local = dt_utc.as_ref().map(fmt_local).unwrap_or_else(|| timestamp_utc.clone());

    let level    = obj.get("@l").and_then(|l| l.as_str()).map(Level::from_str).unwrap_or(Level::Info);
    let template = obj.get("@mt").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let exception = obj.get("@x").and_then(|x| x.as_str()).unwrap_or("").to_string();

    let raw_m = obj.get("@m").and_then(|m| m.as_str()).unwrap_or("");
    let message = if !raw_m.is_empty() {
        raw_m.to_string()
    } else if !template.is_empty() {
        render_template(&template, obj)
    } else {
        String::new()
    };

    Some(LogRecord { timestamp_utc, dt_utc, timestamp_local, level, message, template, exception, raw: v, line_no })
}

pub fn load_file(path: &PathBuf) -> Vec<LogRecord> {
    let content = match fs::read_to_string(path) { Ok(c) => c, Err(_) => return vec![] };
    content.lines().enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .filter_map(|(i, l)| parse_clef_line(l, i + 1))
        .collect()
}

// ── Template aggregation ──────────────────────────────────────────────────────

pub fn build_template_summary(records: &[LogRecord], filtered: &[usize]) -> Vec<TemplateSummary> {
    let mut map: HashMap<String, (usize, HashMap<Level, usize>)> = HashMap::new();
    for &i in filtered {
        let r = &records[i];
        let key = if r.template.is_empty() { r.message.chars().take(80).collect() } else { r.template.clone() };
        let e = map.entry(key).or_insert((0, HashMap::new()));
        e.0 += 1;
        *e.1.entry(r.level).or_insert(0) += 1;
    }
    let mut list: Vec<TemplateSummary> = map.into_iter().map(|(template, (count, lm))| {
        let level = lm.into_iter().max_by_key(|(_, c)| *c).map(|(l, _)| l).unwrap_or(Level::Info);
        TemplateSummary { template, count, level }
    }).collect();
    list.sort_by(|a, b| b.count.cmp(&a.count));
    list
}
