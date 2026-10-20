//! Turns a query into ranked rows from every source.

use std::{collections::HashMap, path::PathBuf};

use serde::Serialize;

use super::{
    apps::{App, Launch},
    calc, colors,
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
    Snippet(String),
    /// A keybind action id, run the way its shortcut would.
    Action(String),
    /// A settings page id.
    Page(String),
    /// A command in the menus of a window: the window id and the menu path.
    Menu { window: u64, path: Vec<String> },
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

pub struct SnippetRow {
    pub id: String,
    pub name: String,
    pub trigger: String,
}

pub struct MenuRow {
    pub window: u64,
    pub path: Vec<String>,
}

pub struct SystemRow {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub target: Target,
}

#[derive(Default)]
pub struct Options {
    pub apps: bool,
    pub windows: bool,
    pub files: bool,
    pub clipboard: bool,
    pub emoji: bool,
    pub calculator: bool,
    pub colors: bool,
    pub snippets: bool,
    pub system: bool,
    pub menus: bool,
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
    pub snippets: &'a [SnippetRow],
    pub system: &'a [SystemRow],
    pub menus: &'a [MenuRow],
    pub usage: &'a HashMap<String, u32>,
    pub learned: Option<&'a str>,
    pub options: &'a Options,
}

pub const LIMIT: usize = 30;
const MENU_LIMIT: usize = 6;

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

    if o.colors {
        if let Some(found) = colors::interpret(query) {
            out.push(Scored {
                score: 9500,
                row: Row { id: format!("color:{}", found.title), kind: "color", title: found.title, subtitle: found.subtitle, icon: found.swatch },
                target: Target::Copy(found.copy),
            });
        }
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

    if o.snippets {
        for s in input.snippets {
            let by_trigger = !s.trigger.is_empty() && same(&s.trigger, query);
            let base = if by_trigger { Some(2800) } else { score(query, &s.name) };
            if let Some(base) = base {
                out.push(Scored {
                    score: base - 10,
                    row: Row { id: format!("snippet:{}", s.id), kind: "snippet", title: s.name.clone(), subtitle: "Snippet - Enter pastes it, Ctrl+Enter copies it".into(), icon: "snippet".into() },
                    target: Target::Snippet(s.id.clone()),
                });
            }
        }
    }

    if o.menus && query.chars().count() >= 2 {
        out.extend(menu_rows(query, input.menus));
    }

    if o.system && query.chars().count() >= 2 {
        for r in input.system {
            if let Some(base) = score(query, &r.title) {
                out.push(Scored {
                    score: base - 40,
                    row: Row { id: r.id.clone(), kind: "system", title: r.title.clone(), subtitle: r.subtitle.clone(), icon: "system".into() },
                    target: r.target.clone(),
                });
            }
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

/// Menu commands that match the query, best first, at most [`MENU_LIMIT`]. A match on the command
/// name beats one on the menus above it.
fn menu_rows(query: &str, menus: &[MenuRow]) -> Vec<Scored> {
    let mut hits: Vec<(i32, &MenuRow)> = menus
        .iter()
        .filter_map(|m| {
            let title = m.path.last()?;
            let trail = m.path[..m.path.len() - 1].join(" ");
            let by_title = score(query, title);
            let by_trail = score(query, &format!("{trail} {title}")).map(|s| s - 150);
            by_title.into_iter().chain(by_trail).max().map(|s| (s, m))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    hits.into_iter()
        .take(MENU_LIMIT)
        .map(|(s, m)| Scored {
            score: s - 30,
            row: Row {
                id: format!("menu:{}", m.path.join(">")),
                kind: "menu",
                title: m.path.last().cloned().unwrap_or_default(),
                subtitle: format!("Menu - {}", m.path[..m.path.len() - 1].join(" > ")),
                icon: "menu".into(),
            },
            target: Target::Menu { window: m.window, path: m.path.clone() },
        })
        .collect()
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
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], snippets: &[], system: &[], menus: &[], usage: &usage, learned: None, options: &o };
        let rows = run("2*21", &input);
        assert_eq!(rows[0].0.title, "42");
        assert_eq!(rows[0].1, Target::Copy("42".into()));
    }

    #[test]
    fn unit_row_and_emoji() {
        let usage = HashMap::new();
        let o = options();
        let input = Input { apps: &[], windows: &[], files: &[], clips: &[], scripts: &[], snippets: &[], system: &[], menus: &[], usage: &usage, learned: None, options: &o };
        assert_eq!(run("5 km to mi", &input)[0].0.kind, "unit");
        let rows = run("rocket", &input);
        assert_eq!(rows[0].0.icon, "🚀");
    }

    #[test]
    fn learned_choice_wins_for_the_same_query() {
        let usage = HashMap::new();
        let o = options();
        let apps = [app("Chrome"), app("Chromium")];
        let plain = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], snippets: &[], system: &[], menus: &[], usage: &usage, learned: None, options: &o };
        let learned = Input { learned: Some("app:Chromium"), ..plain };
        assert_eq!(titles(&run("chrom", &learned))[0], "Chromium");
    }

    #[test]
    fn usage_orders_equal_matches() {
        let mut usage = HashMap::new();
        usage.insert("app:Chromium".to_string(), 5);
        let o = options();
        let apps = [app("Chrome"), app("Chromium")];
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], snippets: &[], system: &[], menus: &[], usage: &usage, learned: None, options: &o };
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
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], snippets: &[], system: &[], menus: &[], usage: &usage, learned: None, options: &o };
        assert_eq!(run("ff", &input)[0].0.title, "Firefox");
        assert!(!titles(&run("secret", &input)).contains(&"Secret".to_string()));
        assert_eq!(titles(&run("", &input)), vec!["Zed".to_string()]);
    }

    #[test]
    fn scripts_take_arguments_and_complete_bare_names() {
        let usage = HashMap::new();
        let o = options();
        let scripts = [Script { name: "backup".into(), path: "/s/backup.sh".into() }];
        let input = Input { apps: &[], windows: &[], files: &[], clips: &[], scripts: &scripts, snippets: &[], system: &[], menus: &[], usage: &usage, learned: None, options: &o };
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
    fn colours_snippets_and_system_rows() {
        let usage = HashMap::new();
        let o = Options { colors: true, snippets: true, system: true, ..Default::default() };
        let snippets = [SnippetRow { id: "s1".into(), name: "Email signature".into(), trigger: ";sig".into() }];
        let system = [SystemRow { id: "system:openSettings".into(), title: "Open Settings".into(), subtitle: "Notifi".into(), target: Target::Action("openSettings".into()) }];
        let input = Input { apps: &[], windows: &[], files: &[], clips: &[], scripts: &[], snippets: &snippets, system: &system, menus: &[], usage: &usage, learned: None, options: &o };
        let colour = run("#33aa77", &input);
        assert_eq!((colour[0].0.kind, colour[0].0.icon.as_str()), ("color", "#33aa77"));
        assert_eq!(run("#f00 to rgb", &input)[0].1, Target::Copy("rgb(255, 0, 0)".into()));
        assert_eq!(run(";sig", &input)[0].1, Target::Snippet("s1".into()));
        assert_eq!(run("signature", &input)[0].0.kind, "snippet");
        assert_eq!(run("open settings", &input)[0].1, Target::Action("openSettings".into()));
        let quiet = Options::default();
        let off = Input { options: &quiet, ..input };
        assert!(run("#33aa77", &off).is_empty() && run(";sig", &off).is_empty() && run("open settings", &off).is_empty());
    }

    #[test]
    fn menu_commands_rank_by_name_then_by_menu() {
        let usage = HashMap::new();
        let o = Options { menus: true, ..Default::default() };
        let menus = [
            MenuRow { window: 7, path: vec!["File".into(), "Save As".into()] },
            MenuRow { window: 7, path: vec!["File".into(), "Export".into(), "Save copy".into()] },
            MenuRow { window: 7, path: vec!["Help".into(), "About".into()] },
        ];
        let input = Input { apps: &[], windows: &[], files: &[], clips: &[], scripts: &[], snippets: &[], system: &[], menus: &menus, usage: &usage, learned: None, options: &o };
        let rows = run("save", &input);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].0.kind, rows[0].0.title.as_str(), rows[0].0.subtitle.as_str()), ("menu", "Save As", "Menu - File"));
        assert_eq!(rows[0].1, Target::Menu { window: 7, path: vec!["File".into(), "Save As".into()] });
        assert_eq!(rows[1].0.subtitle, "Menu - File > Export");
        assert!(run("sa", &Input { options: &Options::default(), ..input }).is_empty());
    }

    #[test]
    fn disabled_sources_stay_quiet() {
        let usage = HashMap::new();
        let o = Options::default();
        let apps = [app("Firefox")];
        let input = Input { apps: &apps, windows: &[], files: &[], clips: &[], scripts: &[], snippets: &[], system: &[], menus: &[], usage: &usage, learned: None, options: &o };
        assert!(run("firefox", &input).is_empty());
        assert!(run("1+1", &input).is_empty());
    }
}
