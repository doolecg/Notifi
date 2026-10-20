//! Typed snippet triggers (spec/input/text-snippets.md): watches what is typed and swaps a
//! finished trigger for its snippet, by typing or pasting the same way the snippet menu does.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};

use notifi_platform::{Observed, Watch};
use tauri::{AppHandle, Manager};

use super::{ignored, opt_bool, opt_list, opt_str, paste_temporarily, settings, snippets::Snippet, sys, ClipState, Payload};
use crate::settings::Settings;

/// Characters kept from what was typed, more than any sensible trigger needs.
const KEEP: usize = 64;

struct Config {
    on: bool,
    prefix: String,
    case: bool,
}

static CONFIG: Mutex<Config> = Mutex::new(Config { on: false, prefix: String::new(), case: false });
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Starts or stops watching to match the settings.
pub fn sync(app: &AppHandle, s: &Settings) {
    let on = s.enabled("textSnippets");
    *CONFIG.lock().unwrap() = Config {
        on,
        prefix: opt_str(s, "textSnippets", "prefix", "").to_string(),
        case: opt_bool(s, "textSnippets", "caseSensitive", false),
    };
    if !on || RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let rx = match notifi_platform::observe(Watch { keys: true, buttons: true, motion: false }) {
        Ok(rx) => rx,
        Err(e) => {
            eprintln!("text snippets: typed triggers are off: {e}");
            RUNNING.store(false, Ordering::SeqCst);
            return;
        }
    };
    let app = app.clone();
    thread::Builder::new()
        .name("snippet-triggers".into())
        .spawn(move || {
            let mut typed = String::new();
            while let Ok(event) = rx.recv() {
                let (prefix, case) = {
                    let c = CONFIG.lock().unwrap();
                    if !c.on {
                        break;
                    }
                    (c.prefix.clone(), c.case)
                };
                match event {
                    Observed::Text(c) => typed.push(c),
                    Observed::Backspace => {
                        typed.pop();
                        continue;
                    }
                    Observed::Break | Observed::Button { down: true, .. } => {
                        typed.clear();
                        continue;
                    }
                    _ => continue,
                }
                if let Some((start, _)) = typed.char_indices().rev().nth(KEEP) {
                    typed.drain(..start);
                }
                let found = {
                    let clip = app.state::<ClipState>();
                    let file = clip.snippets.lock().unwrap();
                    find(&typed, &file.snippets, &prefix, case).map(|(s, n)| (s.clone(), n))
                };
                if let Some((snippet, len)) = found {
                    replace(&app, &snippet, len);
                    // Skip what our own keystrokes produced, then start a new word.
                    thread::sleep(Duration::from_millis(60));
                    while rx.try_recv().is_ok() {}
                    typed.clear();
                }
            }
            RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

/// The snippet whose prefix and trigger end `typed`, longest first, and how many characters that
/// is. A letter or digit just before it means it is part of a longer word, so it doesn't count.
fn find<'a>(typed: &str, list: &'a [Snippet], prefix: &str, case: bool) -> Option<(&'a Snippet, usize)> {
    let typed: Vec<char> = typed.chars().collect();
    let same = |a: char, b: char| a == b || (!case && a.to_lowercase().eq(b.to_lowercase()));
    list.iter()
        .filter(|s| !s.trigger.is_empty())
        .filter_map(|s| {
            let word: Vec<char> = prefix.chars().chain(s.trigger.chars()).collect();
            let start = typed.len().checked_sub(word.len())?;
            let ends = typed[start..].iter().zip(&word).all(|(a, b)| same(*a, *b));
            let alone = start == 0 || !typed[start - 1].is_alphanumeric();
            (ends && alone).then_some((s, word.len()))
        })
        .max_by_key(|(_, n)| *n)
}

fn replace(app: &AppHandle, snippet: &Snippet, len: usize) {
    let s = settings(app);
    let mut leave_alone = opt_list(&s, "textSnippets", "excludedApps");
    leave_alone.extend(opt_list(&s, "appExclusions", "apps"));
    if ignored(&sys::foreground_app(), &leave_alone) {
        return;
    }
    // Let the app take the last key first, so an address bar has shown its completion.
    thread::sleep(Duration::from_millis(40));
    let field = notifi_platform::focused_field();
    if field.password {
        return;
    }
    let text = super::expand_typed(app, &snippet.text);
    // A selected completion after the caret takes one Backspace of its own.
    sys::erase(len + field.selection as usize);
    if opt_str(&s, "textSnippets", "insertMethod", "paste") == "type" {
        sys::type_text(&text);
    } else {
        let _ = paste_temporarily(app, &Payload::text(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str, trigger: &str) -> Snippet {
        Snippet { id: id.into(), trigger: trigger.into(), ..Default::default() }
    }

    #[test]
    fn finds_the_trigger_that_ends_the_text() {
        let list = [s("a", ";s"), s("b", ";sig"), s("c", "addr"), s("d", "")];
        assert_eq!(find("hello ;sig", &list, "", false).map(|(s, n)| (s.id.as_str(), n)), Some(("b", 4)));
        assert_eq!(find("x;s", &list, "", false).map(|(s, n)| (s.id.as_str(), n)), None);
        assert_eq!(find("addr", &list, "", false).map(|(s, n)| (s.id.as_str(), n)), Some(("c", 4)));
        assert!(find("paddr", &list, "", false).is_none());
        assert!(find("ADDR", &list, "", true).is_none());
        assert_eq!(find("ADDR", &list, "", false).map(|(_, n)| n), Some(4));
        assert!(find("addr", &list, "!", false).is_none());
        assert_eq!(find("go !addr", &list, "!", false).map(|(_, n)| n), Some(5));
    }
}
