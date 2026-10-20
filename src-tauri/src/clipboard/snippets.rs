//! Text snippets (spec/input/text-snippets.md): storage and variable expansion. The searchable
//! menu inserts them now; typed triggers come with the input layer in Phase 4.

use std::{collections::HashMap, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub text: String,
    pub group: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SnippetFile {
    pub snippets: Vec<Snippet>,
}

pub fn load(path: &PathBuf) -> SnippetFile {
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(path: &PathBuf, file: &SnippetFile) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Checks a list from the editor: ids present and unique, triggers unique (ignoring empty ones).
pub fn validate(list: &[Snippet]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    let mut triggers = HashMap::new();
    for s in list {
        if s.id.is_empty() || !ids.insert(s.id.as_str()) {
            return Err("Every snippet needs its own id.".into());
        }
        if !s.trigger.is_empty() {
            if let Some(other) = triggers.insert(s.trigger.as_str(), s.name.as_str()) {
                return Err(format!("The trigger {} is used by both {} and {}.", s.trigger, other, s.name));
            }
        }
    }
    Ok(())
}

/// What expansion needs from outside: the time, the clipboard text and zone offsets.
pub struct Context<'a> {
    pub now_ms: i64,
    pub clipboard: &'a str,
    /// Minutes east of UTC for `local` and each named zone, resolved by the UI.
    pub zones: &'a HashMap<String, i32>,
}

/// Expands `{clipboard}`, `{date}`, `{time}`, `{date:FORMAT}` and `{date:FORMAT@ZONE}`.
/// `{{` and `}}` are literal braces; unknown variables are left as written.
pub fn expand(text: &str, ctx: &Context) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with('}') {
            out.push('}');
            rest = &tail[1..];
            continue;
        }
        let Some(end) = tail.find('}') else {
            out.push_str(tail);
            return out;
        };
        let var = &tail[1..end];
        match variable(var, ctx) {
            Some(v) => out.push_str(&v),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn variable(var: &str, ctx: &Context) -> Option<String> {
    let (name, arg) = match var.split_once(':') {
        Some((n, a)) => (n.trim(), Some(a)),
        None => (var.trim(), None),
    };
    match (name, arg) {
        ("clipboard", None) => Some(ctx.clipboard.to_string()),
        ("date", None) => Some(format(ctx.now_ms, offset(ctx, "local")?, "YYYY-MM-DD")),
        ("time", None) => Some(format(ctx.now_ms, offset(ctx, "local")?, "HH:mm")),
        ("date" | "time", Some(arg)) => {
            let (fmt, zone) = match arg.rsplit_once('@') {
                Some((f, z)) => (f, z.trim()),
                None => (arg, "local"),
            };
            Some(format(ctx.now_ms, offset(ctx, zone)?, fmt))
        }
        _ => None,
    }
}

/// Offset of a zone in minutes: `UTC`, a fixed `+05:30`, or one the UI resolved.
fn offset(ctx: &Context, zone: &str) -> Option<i32> {
    if zone.eq_ignore_ascii_case("utc") || zone.eq_ignore_ascii_case("gmt") || zone == "Z" {
        return Some(0);
    }
    if let Some(fixed) = fixed_offset(zone) {
        return Some(fixed);
    }
    ctx.zones.get(zone).copied().or_else(|| (zone == "local").then_some(0))
}

fn fixed_offset(zone: &str) -> Option<i32> {
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let body = &zone[1..];
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h, m),
        None if body.len() == 4 => body.split_at(2),
        None => (body, "0"),
    };
    let (h, m): (i32, i32) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 14 && m < 60).then_some(sign * (h * 60 + m))
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

struct Civil {
    year: i64,
    month: u32,
    day: u32,
    weekday: usize,
    hour: u32,
    minute: u32,
    second: u32,
}

/// Days since 1970-01-01 to a calendar date (proleptic Gregorian).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn civil(now_ms: i64, offset_min: i32) -> Civil {
    let secs = now_ms.div_euclid(1000) + offset_min as i64 * 60;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    Civil {
        year,
        month,
        day,
        weekday: (days + 3).rem_euclid(7) as usize,
        hour: (tod / 3600) as u32,
        minute: (tod % 3600 / 60) as u32,
        second: (tod % 60) as u32,
    }
}

const TOKENS: &[&str] = &["YYYY", "YY", "MMMM", "MMM", "MM", "M", "DD", "D", "dddd", "ddd", "HH", "H", "hh", "h", "mm", "ss", "A", "a", "Z"];

/// Formats a moment with the tokens listed in TOKENS; `[text]` is literal.
pub fn format(now_ms: i64, offset_min: i32, fmt: &str) -> String {
    let t = civil(now_ms, offset_min);
    let h12 = if t.hour % 12 == 0 { 12 } else { t.hour % 12 };
    let mut out = String::new();
    let mut rest = fmt;
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('[') {
            let end = inner.find(']').unwrap_or(inner.len());
            out.push_str(&inner[..end]);
            rest = inner.get(end + 1..).unwrap_or("");
            continue;
        }
        let Some(token) = TOKENS.iter().find(|tok| rest.starts_with(**tok)) else {
            let ch = rest.chars().next().unwrap();
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
            continue;
        };
        let piece = match *token {
            "YYYY" => format!("{:04}", t.year),
            "YY" => format!("{:02}", t.year.rem_euclid(100)),
            "MMMM" => MONTHS[t.month as usize - 1].to_string(),
            "MMM" => MONTHS[t.month as usize - 1][..3].to_string(),
            "MM" => format!("{:02}", t.month),
            "M" => t.month.to_string(),
            "DD" => format!("{:02}", t.day),
            "D" => t.day.to_string(),
            "dddd" => DAYS[t.weekday].to_string(),
            "ddd" => DAYS[t.weekday][..3].to_string(),
            "HH" => format!("{:02}", t.hour),
            "H" => t.hour.to_string(),
            "hh" => format!("{h12:02}"),
            "h" => h12.to_string(),
            "mm" => format!("{:02}", t.minute),
            "ss" => format!("{:02}", t.second),
            "A" => if t.hour < 12 { "AM" } else { "PM" }.to_string(),
            "a" => if t.hour < 12 { "am" } else { "pm" }.to_string(),
            "Z" => {
                let sign = if offset_min < 0 { '-' } else { '+' };
                format!("{sign}{:02}:{:02}", offset_min.abs() / 60, offset_min.abs() % 60)
            }
            _ => unreachable!(),
        };
        out.push_str(&piece);
        rest = &rest[token.len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-20 13:05:09 UTC, a Tuesday.
    const NOW: i64 = 1_792_501_509_000;

    fn ctx<'a>(zones: &'a HashMap<String, i32>) -> Context<'a> {
        Context { now_ms: NOW, clipboard: "copied", zones }
    }

    #[test]
    fn plain_variables() {
        let zones = HashMap::from([("local".to_string(), 60)]);
        assert_eq!(expand("Hi {clipboard}!", &ctx(&zones)), "Hi copied!");
        assert_eq!(expand("{date} {time}", &ctx(&zones)), "2026-10-20 14:05");
    }

    #[test]
    fn formats_and_zones() {
        let zones = HashMap::from([("local".to_string(), 0), ("Asia/Tokyo".to_string(), 540)]);
        let c = ctx(&zones);
        assert_eq!(expand("{date:dddd D MMMM YYYY}", &c), "Tuesday 20 October 2026");
        assert_eq!(expand("{date:ddd DD/MM/YY h:mm:ss A}", &c), "Tue 20/10/26 1:05:09 PM");
        assert_eq!(expand("{time:HH:mm@Asia/Tokyo}", &c), "22:05");
        assert_eq!(expand("{date:HH:mm Z@+05:30}", &c), "18:35 +05:30");
        assert_eq!(expand("{date:HH:mm Z@-0330}", &c), "09:35 -03:30");
        assert_eq!(expand("{date:YYYY-MM-DD@UTC}", &c), "2026-10-20");
        assert_eq!(expand("{date:[Week of] MMM D}", &c), "Week of Oct 20");
    }

    #[test]
    fn crossing_midnight_and_year() {
        let zones = HashMap::new();
        let c = Context { now_ms: 1_798_761_000_000, clipboard: "", zones: &zones }; // 2026-12-31 23:50 UTC
        assert_eq!(expand("{date:YYYY-MM-DD HH:mm@+01:00}", &c), "2027-01-01 00:50");
        assert_eq!(expand("{date:YYYY-MM-DD dddd@UTC}", &c), "2026-12-31 Thursday");
    }

    #[test]
    fn braces_and_unknowns() {
        let zones = HashMap::new();
        let c = ctx(&zones);
        assert_eq!(expand("{{clipboard}} {nope} {date:YYYY@Mars/Base}", &c), "{clipboard} {nope} {date:YYYY@Mars/Base}");
        assert_eq!(expand("open { without end", &c), "open { without end");
        assert_eq!(expand("a } b", &c), "a } b");
    }

    #[test]
    fn validation() {
        let s = |id: &str, trigger: &str| Snippet { id: id.into(), name: id.into(), trigger: trigger.into(), ..Default::default() };
        assert!(validate(&[s("a", ";sig"), s("b", ""), s("c", "")]).is_ok());
        assert!(validate(&[s("a", ";sig"), s("b", ";sig")]).is_err());
        assert!(validate(&[s("a", ""), s("a", "")]).is_err());
    }
}
