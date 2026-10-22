//! Every settings page, its cards and options. Option values live in the settings store
//! under the option's namespace (the page id, or a card's own feature id).

use serde::Serialize;
use serde_json::{Map, Value};

mod apps;
mod clipboard;
mod energy;
mod essentials;
mod input;
mod monitor;
mod sound;
mod utilities;
mod windows;

#[derive(Serialize, Clone, Copy, Debug)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
}

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Control {
    Switch { default: bool },
    /// Drop-down list.
    Picker { choices: &'static [Choice], default: &'static str },
    /// A row of equal tiles for an exclusive choice.
    Tiles { choices: &'static [Choice], default: &'static str },
    Slider { min: f64, max: f64, step: f64, unit: &'static str, default: f64 },
    Number { min: f64, max: f64, step: f64, unit: &'static str, default: f64 },
    Text { placeholder: &'static str, default: &'static str },
    /// A list of strings (apps, folders, rules).
    List { placeholder: &'static str, default: &'static [&'static str] },
    Shortcut { default: &'static str },
    /// Drawn by the page's own component; listed here so search can find it.
    Custom,
}

#[derive(Serialize, Clone, Copy, Debug)]
pub struct Opt {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub icon: &'static str,
    pub control: Control,
}

#[derive(Serialize, Clone, Copy, Debug)]
pub struct Card {
    pub title: &'static str,
    /// Sub-tab the card sits on; empty when the page has no sub-tabs.
    pub tab: &'static str,
    /// A sub-feature with its own on/off switch in the card header and its own namespace.
    pub feature: &'static str,
    pub options: &'static [Opt],
}

#[derive(Serialize, Clone, Copy, Debug)]
pub struct Page {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub blurb: &'static str,
    /// Has an on/off switch in its header, stored as `<id>.enabled`.
    pub feature: bool,
    /// Something the user opens, listed under Utilities in the panel.
    pub tool: bool,
    /// "windows" or "linux" when the page only exists there.
    pub platform: &'static str,
    pub tabs: &'static [&'static str],
    /// Drawn by its own component instead of from the cards.
    pub custom: bool,
    pub cards: &'static [Card],
    /// Spec files (under spec/) this page covers.
    pub specs: &'static [&'static str],
}

#[derive(Serialize, Clone, Copy, Debug)]
pub struct Group {
    pub id: &'static str,
    pub label: &'static str,
    pub pages: &'static [Page],
}

pub const PAGE: Page = Page {
    id: "",
    label: "",
    icon: "",
    blurb: "",
    feature: true,
    tool: false,
    platform: "",
    tabs: &[],
    custom: false,
    cards: &[],
    specs: &[],
};

pub const CARD: Card = Card { title: "", tab: "", feature: "", options: &[] };

pub const fn card(title: &'static str, options: &'static [Opt]) -> Card {
    Card { title, tab: "", feature: "", options }
}

pub const fn tab_card(tab: &'static str, title: &'static str, options: &'static [Opt]) -> Card {
    Card { title, tab, feature: "", options }
}

const fn opt(id: &'static str, label: &'static str, hint: &'static str, control: Control) -> Opt {
    Opt { id, label, hint, icon: "", control }
}

pub const fn sw(id: &'static str, label: &'static str, hint: &'static str, default: bool) -> Opt {
    opt(id, label, hint, Control::Switch { default })
}

pub const fn pick(id: &'static str, label: &'static str, hint: &'static str, choices: &'static [Choice], default: &'static str) -> Opt {
    opt(id, label, hint, Control::Picker { choices, default })
}

pub const fn tiles(id: &'static str, label: &'static str, hint: &'static str, choices: &'static [Choice], default: &'static str) -> Opt {
    opt(id, label, hint, Control::Tiles { choices, default })
}

#[allow(clippy::too_many_arguments)]
pub const fn slider(id: &'static str, label: &'static str, hint: &'static str, min: f64, max: f64, step: f64, unit: &'static str, default: f64) -> Opt {
    opt(id, label, hint, Control::Slider { min, max, step, unit, default })
}

#[allow(clippy::too_many_arguments)]
pub const fn num(id: &'static str, label: &'static str, hint: &'static str, min: f64, max: f64, step: f64, unit: &'static str, default: f64) -> Opt {
    opt(id, label, hint, Control::Number { min, max, step, unit, default })
}

pub const fn text(id: &'static str, label: &'static str, hint: &'static str, placeholder: &'static str, default: &'static str) -> Opt {
    opt(id, label, hint, Control::Text { placeholder, default })
}

pub const fn list(id: &'static str, label: &'static str, hint: &'static str, placeholder: &'static str) -> Opt {
    opt(id, label, hint, Control::List { placeholder, default: &[] })
}

/// A list that starts with `default` instead of empty.
pub const fn list_of(id: &'static str, label: &'static str, hint: &'static str, placeholder: &'static str, default: &'static [&'static str]) -> Opt {
    opt(id, label, hint, Control::List { placeholder, default })
}

pub const fn key(id: &'static str, label: &'static str, hint: &'static str, default: &'static str) -> Opt {
    opt(id, label, hint, Control::Shortcut { default })
}

pub const fn custom(id: &'static str, label: &'static str, hint: &'static str) -> Opt {
    opt(id, label, hint, Control::Custom)
}

pub const fn c(value: &'static str, label: &'static str) -> Choice {
    Choice { value, label, icon: "" }
}

pub const fn ci(value: &'static str, label: &'static str, icon: &'static str) -> Choice {
    Choice { value, label, icon }
}

pub const GROUPS: &[Group] = &[
    Group { id: "essentials", label: "Essentials", pages: essentials::PAGES },
    Group { id: "utilities", label: "Utilities", pages: utilities::PAGES },
    Group { id: "sound", label: "Sound", pages: sound::PAGES },
    Group { id: "energy", label: "Energy and display", pages: energy::PAGES },
    Group { id: "windows", label: "Windows", pages: windows::PAGES },
    Group { id: "input", label: "Keyboard and mouse", pages: input::PAGES },
    Group { id: "clipboard", label: "Clipboard and files", pages: clipboard::PAGES },
    Group { id: "apps", label: "Apps", pages: apps::PAGES },
    Group { id: "about", label: "About", pages: essentials::ABOUT },
];

/// Sections stored as typed structs rather than in the `features` map.
pub const TYPED: &[&str] = &["general", "panel", "island"];

/// What the first-run chooser ticks for "Recommended": a handful of everyday features, not the whole list.
pub const RECOMMENDED: &[&str] = &["island", "clipboardHistory", "screenshot", "volumeMixer", "commandBar", "scratchpad", "windowLayout"];

pub fn pages() -> impl Iterator<Item = &'static Page> {
    GROUPS.iter().flat_map(|g| g.pages.iter())
}

#[allow(dead_code)]
pub fn page(id: &str) -> Option<&'static Page> {
    pages().find(|p| p.id == id)
}

impl Card {
    /// The namespace this card's options are stored under.
    pub fn namespace(&self, page: &Page) -> &'static str {
        if self.feature.is_empty() {
            page.id
        } else {
            self.feature
        }
    }
}

impl Control {
    pub fn default_value(&self) -> Option<Value> {
        Some(match *self {
            Control::Switch { default } => Value::Bool(default),
            Control::Picker { default, .. } | Control::Tiles { default, .. } => default.into(),
            Control::Slider { default, .. } | Control::Number { default, .. } => number(default),
            Control::Text { default, .. } | Control::Shortcut { default } => default.into(),
            Control::List { default, .. } => Value::Array(default.iter().map(|s| Value::from(*s)).collect()),
            Control::Custom => return None,
        })
    }

    /// The saved value when it fits this control (numbers clamped), otherwise `None`.
    pub fn accept(&self, value: &Value) -> Option<Value> {
        match *self {
            Control::Switch { .. } => value.as_bool().map(Value::Bool),
            Control::Picker { choices, .. } | Control::Tiles { choices, .. } => {
                value.as_str().filter(|v| choices.iter().any(|c| c.value == *v)).map(Value::from)
            }
            Control::Slider { min, max, .. } | Control::Number { min, max, .. } => {
                value.as_f64().map(|v| number(v.clamp(min, max)))
            }
            Control::Text { .. } | Control::Shortcut { .. } => value.as_str().map(Value::from),
            Control::List { .. } => {
                let items = value.as_array()?;
                items.iter().all(Value::is_string).then(|| value.clone())
            }
            Control::Custom => None,
        }
    }
}

fn number(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        Value::from(v as i64)
    } else {
        Value::from(v)
    }
}

/// One stored option: its namespace, its key in that namespace, and its control.
pub struct Field {
    pub namespace: &'static str,
    pub key: &'static str,
    pub control: Control,
}

const ENABLED: Control = Control::Switch { default: false };

/// Every stored option, including the implied `enabled` switch of each feature.
pub fn fields() -> impl Iterator<Item = Field> {
    pages().flat_map(|page| {
        let header = page.feature.then_some(Field { namespace: page.id, key: "enabled", control: ENABLED });
        let cards = page.cards.iter().flat_map(move |card| {
            let ns = card.namespace(page);
            let switch = (!card.feature.is_empty()).then_some(Field { namespace: ns, key: "enabled", control: ENABLED });
            let options = card
                .options
                .iter()
                .filter(|o| !matches!(o.control, Control::Custom))
                .map(move |o| Field { namespace: ns, key: o.id, control: o.control });
            switch.into_iter().chain(options)
        });
        header.into_iter().chain(cards)
    })
}

/// Default values of every namespace kept in the `features` map.
pub fn feature_defaults() -> std::collections::BTreeMap<String, Map<String, Value>> {
    let mut out: std::collections::BTreeMap<String, Map<String, Value>> = Default::default();
    for field in fields().filter(|f| !TYPED.contains(&f.namespace)) {
        if let Some(value) = field.control.default_value() {
            out.entry(field.namespace.to_string()).or_default().insert(field.key.to_string(), value);
        }
    }
    out
}

/// The control for `namespace.key`, if the catalog has one.
pub fn control(namespace: &str, key: &str) -> Option<Control> {
    fields().find(|f| f.namespace == namespace && f.key == key).map(|f| f.control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ids_are_unique() {
        let mut pages_seen = BTreeSet::new();
        for page in pages() {
            assert!(pages_seen.insert(page.id), "duplicate page {}", page.id);
        }
        let mut seen = BTreeSet::new();
        for f in fields() {
            assert!(seen.insert((f.namespace, f.key)), "duplicate option {}.{}", f.namespace, f.key);
            assert!(!pages_seen.contains(f.namespace) || page(f.namespace).is_some());
        }
    }

    #[test]
    fn recommended_features_exist() {
        for id in RECOMMENDED {
            assert!(page(id).is_some_and(|p| p.feature), "{id} is not a feature page");
        }
    }

    #[test]
    fn defaults_fit_their_controls() {
        for f in fields() {
            let value = f.control.default_value().unwrap();
            assert_eq!(f.control.accept(&value), Some(value.clone()), "{}.{}", f.namespace, f.key);
            if let Control::Picker { choices, .. } | Control::Tiles { choices, .. } = f.control {
                assert!(!choices.is_empty(), "{}.{} has no choices", f.namespace, f.key);
            }
        }
    }

    #[test]
    fn sub_tabs_are_used() {
        for page in pages() {
            for card in page.cards {
                if page.tabs.is_empty() {
                    assert!(card.tab.is_empty(), "{}: card {} names a tab", page.id, card.title);
                } else {
                    assert!(page.tabs.contains(&card.tab), "{}: card {} has unknown tab {}", page.id, card.title, card.tab);
                }
            }
        }
    }

    fn spec_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec")
    }

    /// Every page named in spec/settings.md exists in the catalog under its group.
    #[test]
    fn every_settings_md_page_exists() {
        let text = std::fs::read_to_string(spec_dir().join("settings.md")).unwrap();
        let section = text.split("## Sidebar groups and pages").nth(1).unwrap().split("\n## ").next().unwrap();
        let norm = |s: &str| s.to_lowercase().replace('&', "and").replace(" (linux only)", "").replace(" (linux)", "");
        let labels: BTreeSet<String> = pages().map(|p| norm(p.label)).collect();
        let held_back = ["app switcher", "port manager"];
        let mut missing = Vec::new();
        for line in section.lines() {
            let names: Vec<String> = if let Some(rest) = line.strip_prefix("- **") {
                let (head, tail) = rest.split_once(":**").unwrap();
                match head {
                    "Essentials" => continue,
                    "About" => vec![norm(head)],
                    _ => tail.trim().trim_end_matches('.').split(", ").map(norm).collect(),
                }
            } else if let Some(rest) = line.strip_prefix("  - ") {
                vec![norm(rest.split([':', '.']).next().unwrap())]
            } else {
                continue;
            };
            for name in names {
                if !labels.contains(&name) && !held_back.contains(&name.as_str()) {
                    missing.push(name);
                }
            }
        }
        assert!(missing.is_empty(), "pages missing from the catalog: {missing:?}");
    }

    /// Every feature spec file is covered by some page.
    #[test]
    fn every_spec_feature_has_a_page() {
        let covered: BTreeSet<&str> = pages().flat_map(|p| p.specs.iter().copied()).collect();
        let held_back = ["windows/app-switcher.md", "windows/switcher-options.md", "apps/port-manager.md", "apps/kill-process.md"];
        let mut missing = Vec::new();
        for dir in std::fs::read_dir(spec_dir()).unwrap().flatten().filter(|e| e.path().is_dir()) {
            for file in std::fs::read_dir(dir.path()).unwrap().flatten() {
                let name = format!("{}/{}", dir.file_name().to_string_lossy(), file.file_name().to_string_lossy());
                if name.ends_with(".md") && !covered.contains(name.as_str()) && !held_back.contains(&name.as_str()) {
                    missing.push(name);
                }
            }
        }
        assert!(missing.is_empty(), "spec files without a settings page: {missing:?}");
    }
}
