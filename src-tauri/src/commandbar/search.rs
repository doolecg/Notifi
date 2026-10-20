//! Turns a query into ranked rows from every source.

use std::{collections::HashMap, path::PathBuf};

use serde::Serialize;

use super::{
    apps::{App, Launch},
    calc,
    emoji_data::EMOJI,
    index::{split_args, FileEntry, Script},
    rank::score,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    App(Launch),
    Window(u64),
    File(PathBuf),
    Clip(u64),
    Copy(String),
    Script { path: PathBuf, args: Vec<String>, complete: Option<String> },
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub kind: &'static str,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
}

pub struct WindowRow {
    pub id: u64,
    pub title: String,
    pub app: String,
}

pub struct ClipRow {
    pub id: u64,
    pub text: String,
}

#[derive(Default)]
pub struct Options {
    pub apps: bool,
    pub windows: bool,
    pub files: bool,
    pub clipboard: bool,
    pub emoji: bool,
    pub calculator: bool,
    pub scripts: bool,
    pub aliases: Vec<(String, String)>,
    pub pins: Vec<String>,
    pub hidden: Vec<String>,
    pub bare_scripts: Vec<String>,
}

pub struct Input<'a> {
    pub apps: &'a [App],
    pub windows: &'a [WindowRow],
    pub files: &'a [FileEntry],
    pub clips: &'a [ClipRow],
    pub scripts: &'a [Script],
    pub usage: &'a HashMap<String, u32>,
    pub learned: Option<&'a str>,
    pub options: &'a Options,
}

pub const LIMIT: usize = 30;

/// `"ff = Firefox"` lines become pairs; anything without an equals sign is ignored.
pub fn parse_pairs(lines: &[String]) -> Vec<(String, String)> {
    lines
        .iter()
        .filter_map(|l| l.split_once('='))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .collect()
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn clip_title(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    line.chars().take(90).collect()
}

fn row_key(kind: &str, name: &str) -> String {
    format!("{kind}:{name}")
}

struct Scored {
    score: i32,
    row: Row,
    target: Target,
}

fn boost(input: &Input, key: &str) -> i32 {
    let used = input.usage.get(key).copied().unwrap_or(0).min(20) as i32 * 15;
    let learned = if input.learned == Some(key) { 1500 } else { 0 };
    used + learned
}

pub fn run(query: &str, input: &Input) -> Vec<(Row, Target)> {
    let query = query.trim();
    let o = input.options;
    let mut out: Vec<Scored> = Vec::new();

    if query.is_empty() {
        for app in input.apps.iter().filter(|a| !o.hidden.iter().any(|h| same(h, &a.name))) {
            let key = row_key("app", &app.name);
            let pinned = o.pins.iter().any(|p| same(p, &app.name));
            let used = input.usage.get(&key).copied().unwrap_or(0);
            if pinned || used > 0 {
                out.push(app_row(app, if pinned { 5000 } else { 0 } + used as i32));
            }
        }
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out.truncate(8);
        return finish(out);
    }

    if o.calculator {
        if let Some(value) = calc::evaluate(query) {
            out.push(copy_row("calc", &value, &format!("{query} ="), "calc", 9000, value.clone()));
        } else if let Some((line, value)) = calc::convert(query) {
            out.push(copy_row("unit", &value, &line, "ruler", 9000, value.clone()));
        }
    }

    if o.apps {
        for app in input.apps.iter().filter(|a| !o.hidden.iter().any(|h| same(h, &a.name))) {
            let alias = o.aliases.iter().any(|(a, target)| same(a, query) && same(target, &app.name));
            let base = if alias { Some(3000) } else { score(query, &app.name) };
            if let Some(base) = base {
                let pin = if o.pins.iter().any(|p| same(p, &app.name)) { 120 } else { 0 };
                out.push(app_row(app, base + pin + 60 + boost(input, &row_key("app", &app.name))));
            }
        }
    }

    if o.windows {
        for w in input.windows {
            let best = score(query, &w.title).into_iter().chain(score(query, &w.app)).max();
            if let Some(base) = best {
                out.push(Scored {
                    score: base + 30 + boost(input, &row_key("window", &w.app)),
                    row: Row { id: format!("window:{}", w.id), kind: "window", title: w.title.clone(), subtitle: format!("Window - {}", w.app), icon: "window".into() },
                    target: Target::Window(w.id),
                });
            }
        }
    }

    if o.scripts {
        let (first, rest) = query.split_once(char::is_whitespace).unwrap_or((query, ""));
        for s in input.scripts {
            let exact = same(first, &s.name);
            let Some(base) = (if exact { Some(2500) } else if rest.is_empty() { score(query, &s.name) } else { None }) else { continue };
            let bare = o.bare_scripts.iter().any(|b| same(b, &s.name));
            let args = split_args(rest);
            let complete = (exact && rest.is_empty() && !bare).then(|| format!("{} ", s.name));
            let subtitle = if complete.is_some() {
                "Script - type arguments after the name".to_string()
            } else if args.is_empty() {
                "Script - runs and copies its output".to_string()
            } else {
                format!("Script - {}", args.join(" "))
            };
            out.push(Scored {
                score: base + 40,
                row: Row { id: format!("script:{}", s.name), kind: "script", title: s.name.clone(), subtitle, icon: "script".into() },
                target: Target::Script { path: s.path.clone(), args, complete },
            });
        }
    }

    if o.files && query.chars().count() >= 2 {
        let mut hits: Vec<(i32, &FileEntry)> = input.files.iter().filter_map(|f| score(query, &f.name).map(|s| (s, f))).collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        for (s, f) in hits.into_iter().take(6) {
            out.push(Scored {
                score: s - 20,
                row: Row {
                    id: format!("file:{}", f.path.display()),
                    kind: "file",
                    title: f.name.clone(),
                    subtitle: f.path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
                    icon: if f.dir { "folder" } else { "file" }.into(),
                },
                target: Target::File(f.path.clone()),
            });
        }
    }

    if o.clipboard && query.chars().count() >= 2 {
        let mut hits: Vec<(i32, &ClipRow)> = input.clips.iter().filter_map(|c| score(query, &c.text.chars().take(300).collect::<String>()).map(|s| (s, c))).collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        for (s, c) in hits.into_iter().take(4) {
            out.push(Scored {
                score: s - 60,
                row: Row { id: format!("clip:{}", c.id), kind: "clipboard", title: clip_title(&c.text), subtitle: "Clipboard history".into(), icon: "clipboard".into() },
                target: Target::Clip(c.id),
            });
        }
    }

    if o.emoji && query.chars().count() >= 2 {
        let mut hits: Vec<(i32, &(&str, &str))> = EMOJI.iter().filter_map(|e| score(query, e.1).map(|s| (s, e))).collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        for (s, (glyph, name)) in hits.into_iter().take(6) {
            let key = row_key("emoji", glyph);
            out.push(Scored {
                score: s - 80 + boost(input, &key),
                row: Row { id: key, kind: "emoji", title: (*name).to_string(), subtitle: "Emoji - Enter copies it".into(), icon: (*glyph).to_string() },
                target: Target::Copy((*glyph).to_string()),
            });
        }
    }

    out.sort_by(|a, b| b.score.cmp(&a.score));
    out.truncate(LIMIT);
    finish(out)
}

fn app_row(app: &App, score: i32) -> Scored {
    Scored {
        score,
        row: Row { id: row_key("app", &app.name), kind: "app", title: app.name.clone(), subtitle: if app.detail.is_empty() { "App".into() } else { format!("App - {}", app.detail) }, icon: "app".into() },
        target: Target::App(app.launch.clone()),
    }
}

fn copy_row(kind: &'static str, title: &str, subtitle: &str, icon: &str, score: i32, text: String) -> Scored {
    Scored { score, row: Row { id: format!("{kind}:{title}"), kind, title: title.to_string(), subtitle: subtitle.to_string(), icon: icon.into() }, target: Target::Copy(text) }
}

fn finish(scored: Vec<Scored>) -> Vec<(Row, Target)> {
    scored.into_iter().map(|s| (s.row, s.target)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> App {
        App { name: name.into(), launch: Launch::Exec(name.to_lowercase()), detail: String::new() }
    }

    fn options() -> Options {
        Options { apps: true, windows: true, files: true, clipboard: true, emoji: true, calculator: true, scripts: true, ..Default::default() }
    }

    fn titles(rows: &[(Row, Target)]) -> Vec<String> {
        rows.iter().map(|(r, _)| r.title.clone()).collect()
    }

    #[test]
    fn calculator_row_comes_first() {
        let usage = HashMap::new();
        let o = options();
        let apps = [app("Calculator")];
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], usage: &usage, learned: None, options: &o };
        let rows = run("2*21", &input);
        assert_eq!(rows[0].0.title, "42");
        assert_eq!(rows[0].1, Target::Copy("42".into()));
    }

    #[test]
    fn unit_row_and_emoji() {
        let usage = HashMap::new();
        let o = options();
        let input = Input { apps: &[], windows: &[], files: &[], clips: &[], scripts: &[], usage: &usage, learned: None, options: &o };
        assert_eq!(run("5 km to mi", &input)[0].0.kind, "unit");
        let rows = run("rocket", &input);
        assert_eq!(rows[0].0.icon, "🚀");
    }

    #[test]
    fn learned_choice_wins_for_the_same_query() {
        let usage = HashMap::new();
        let o = options();
        let apps = [app("Chrome"), app("Chromium")];
        let plain = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], usage: &usage, learned: None, options: &o };
        let learned = Input { learned: Some("app:Chromium"), ..plain };
        assert_eq!(titles(&run("chrom", &learned))[0], "Chromium");
    }

    #[test]
    fn usage_orders_equal_matches() {
        let mut usage = HashMap::new();
        usage.insert("app:Chromium".to_string(), 5);
        let o = options();
        let apps = [app("Chrome"), app("Chromium")];
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], usage: &usage, learned: None, options: &o };
        let rows = run("chrom", &input);
        assert_eq!(rows[0].0.title, "Chromium");
    }

    #[test]
    fn alias_and_hidden_and_pins() {
        let usage = HashMap::new();
        let mut o = options();
        o.aliases = parse_pairs(&["ff = Firefox".to_string(), "bad".to_string()]);
        o.hidden = vec!["Secret".into()];
        o.pins = vec!["Zed".into()];
        let apps = [app("Firefox"), app("Secret"), app("Zed")];
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], usage: &usage, learned: None, options: &o };
        assert_eq!(run("ff", &input)[0].0.title, "Firefox");
        assert!(!titles(&run("secret", &input)).contains(&"Secret".to_string()));
        assert_eq!(titles(&run("", &input)), vec!["Zed".to_string()]);
    }

    #[test]
    fn scripts_take_arguments_and_complete_bare_names() {
        let usage = HashMap::new();
        let o = options();
        let scripts = [Script { name: "backup".into(), path: "/s/backup.sh".into() }];
        let input = Input { apps: &[], windows: &[], files: &[], clips: &[], scripts: &scripts, usage: &usage, learned: None, options: &o };
        match &run("backup now \"to disk\"", &input)[0].1 {
            Target::Script { args, complete, .. } => {
                assert_eq!(args, &vec!["now".to_string(), "to disk".to_string()]);
                assert!(complete.is_none());
            }
            other => panic!("{other:?}"),
        }
        match &run("backup", &input)[0].1 {
            Target::Script { complete, .. } => assert_eq!(complete.as_deref(), Some("backup ")),
            other => panic!("{other:?}"),
        }
        let mut bare = options();
        bare.bare_scripts = vec!["backup".into()];
        let input = Input { options: &bare, ..input };
        match &run("backup", &input)[0].1 {
            Target::Script { complete, .. } => assert!(complete.is_none()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn disabled_sources_stay_quiet() {
        let usage = HashMap::new();
        let o = Options::default();
        let apps = [app("Firefox")];
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], usage: &usage, learned: None, options: &o };
        assert!(run("firefox", &input).is_empty());
        assert!(run("1+1", &input).is_empty());
    }
}
