//! Dynamic notifications (spec/apps/dynamic-notifications.md): turns Windows toasts from delivery and
//! ride services into one live progress card for the Island.

use super::Live;

pub struct Provider {
    pub name: &'static str,
    keys: &'static [&'static str],
    color: &'static str,
    icon: &'static str,
    ride: bool,
}

/// Longest names first, so "Uber Eats" is not taken for "Uber".
pub const PROVIDERS: &[Provider] = &[
    Provider { name: "Just Eat", keys: &["just eat", "justeat"], color: "#ff8000", icon: "bag", ride: false },
    Provider { name: "Uber Eats", keys: &["uber eats", "ubereats"], color: "#06c167", icon: "bag", ride: false },
    Provider { name: "Deliveroo", keys: &["deliveroo"], color: "#00ccbc", icon: "bag", ride: false },
    Provider { name: "Uber", keys: &["uber"], color: "#c9cccd", icon: "car", ride: true },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    pub label: &'static str,
    pub progress: f64,
    pub done: bool,
}

const fn stage(label: &'static str, progress: f64) -> Stage {
    Stage { label, progress, done: progress >= 1.0 }
}

/// Words that show a stage, furthest stage first.
type Rules = &'static [(&'static [&'static str], Stage)];

const FOOD: Rules = &[
    (&["delivered", "enjoy your meal", "order complete"], stage("Delivered", 1.0)),
    (&["arriving now", "at your door", "outside", "nearby", "almost there", "arriving"], stage("Arriving", 0.9)),
    (&["on its way", "on the way", "out for delivery", "en route", "heading to you", "picked up", "on their way"], stage("On the way", 0.7)),
    (&["ready for collection", "ready for pickup", "ready to be picked up"], stage("Ready", 0.5)),
    (&["being prepared", "preparing", "is cooking", "being made"], stage("Preparing", 0.3)),
    (&["confirmed", "accepted", "received your order", "order placed", "thanks for your order"], stage("Confirmed", 0.1)),
];

const RIDE: Rules = &[
    (&["trip ended", "you've arrived", "you have arrived", "dropped off"], stage("Trip ended", 1.0)),
    (&["on your trip", "trip started", "heading to your destination"], stage("On the trip", 0.8)),
    (&["has arrived", "is here", "waiting for you"], stage("Driver is here", 0.6)),
    (&["arriving now", "is arriving", "arriving in"], stage("Driver arriving", 0.45)),
    (&["on the way", "on their way", "is coming", "accepted your"], stage("Driver on the way", 0.3)),
    (&["finding a driver", "looking for a driver", "searching for"], stage("Finding a driver", 0.1)),
];

fn has_word(hay: &str, key: &str) -> bool {
    hay.match_indices(key).any(|(i, _)| {
        !hay[..i].chars().next_back().is_some_and(char::is_alphanumeric) && !hay[i + key.len()..].chars().next().is_some_and(char::is_alphanumeric)
    })
}

/// The service a notification is from, by app name or text.
pub fn provider(app: &str, text: &str) -> Option<&'static Provider> {
    let hay = format!("{app} {text}").to_lowercase();
    PROVIDERS.iter().find(|p| p.keys.iter().any(|k| has_word(&hay, k)))
}

/// Minutes left, from a number before "min" ("12 min", "10-15 minutes": the upper end).
pub fn eta_minutes(text: &str) -> Option<u32> {
    let t = text.to_lowercase();
    t.match_indices("min").find_map(|(i, _)| {
        let head = t[..i].trim_end();
        let digits: String = head.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
        digits.parse::<u32>().ok().filter(|&m| m <= 240)
    })
}

pub fn read_stage(p: &Provider, text: &str) -> Option<Stage> {
    let t = text.to_lowercase();
    let found = (if p.ride { RIDE } else { FOOD }).iter().find(|(words, _)| words.iter().any(|w| t.contains(w))).map(|&(_, s)| s)?;
    // "Arriving in 12 minutes" is still on the way.
    Some(if !p.ride && found.label == "Arriving" && eta_minutes(text).is_some_and(|m| m > 3) { stage("On the way", 0.7) } else { found })
}

const DONE_LINGER_MS: u64 = 90_000;
const STALE_MS: u64 = 3 * 60 * 60_000;
const LATE_MS: u64 = 20 * 60_000;

struct Card {
    provider: &'static Provider,
    stage: Stage,
    eta_at: Option<u64>,
    eta_total: u64,
    updated: u64,
}

#[derive(Default)]
pub struct Orders {
    cards: Vec<Card>,
}

impl Orders {
    /// Takes a notification from a followed service; false when it shows no stage.
    pub fn update(&mut self, p: &'static Provider, text: &str, now: u64) -> bool {
        let Some(stage) = read_stage(p, text) else { return false };
        let eta = eta_minutes(text).map(|m| u64::from(m) * 60_000);
        let at = self.cards.iter().position(|c| std::ptr::eq(c.provider, p)).unwrap_or_else(|| {
            self.cards.push(Card { provider: p, stage, eta_at: None, eta_total: 0, updated: now });
            self.cards.len() - 1
        });
        let c = &mut self.cards[at];
        c.stage = stage;
        c.updated = now;
        if let Some(ms) = eta {
            c.eta_at = Some(now + ms);
            c.eta_total = c.eta_total.max(ms);
        }
        if stage.done {
            c.eta_at = None;
        }
        true
    }

    /// The newest card as an Island entry; finished and forgotten cards are dropped first.
    pub fn live(&mut self, now: u64) -> Vec<Live> {
        self.cards.retain(|c| now.saturating_sub(c.updated) < if c.stage.done { DONE_LINGER_MS } else { STALE_MS });
        let Some(c) = self.cards.iter().max_by_key(|c| c.updated) else { return Vec::new() };
        let left = c.eta_at.filter(|&at| now < at + LATE_MS).map(|at| at.saturating_sub(now));
        let value = left.map_or(String::new(), |ms| if ms == 0 { "Due".into() } else { format!("{} min", ms.div_ceil(60_000)) });
        let mut progress = c.stage.progress;
        if let (Some(ms), true) = (left, c.eta_total > 0 && !c.stage.done) {
            progress = progress.max((1.0 - ms as f64 / c.eta_total as f64).min(0.95));
        }
        vec![Live {
            id: "order",
            label: c.provider.name,
            icon: c.provider.icon,
            color: c.provider.color,
            value,
            status: if c.stage.done { "done" } else { "" },
            count: 0,
            todos: None,
            tokens: 0,
            session_pct: None,
            week_pct: None,
            project: None,
            terminals: 0,
            subagents: 0,
            progress: Some(progress),
            detail: Some(c.stage.label.to_string()),
        }]
    }
}

#[cfg(windows)]
mod listener {
    use std::{collections::HashSet, time::Duration};

    use tauri::{AppHandle, Manager};
    use windows::{
        Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
        UI::Notifications::{
            Management::{UserNotificationListener, UserNotificationListenerAccessStatus},
            NotificationKinds, UserNotification,
        },
    };

    use super::{provider, Provider};
    use crate::island::{activity, config, config::IslandConfig, now_ms, notice, IslandState, AGENT_COLOR};

    fn read(n: &UserNotification) -> Option<(String, Vec<String>)> {
        let who = n.AppInfo().ok()?.DisplayInfo().ok()?.DisplayName().ok()?.to_string();
        let mut texts = Vec::new();
        for binding in n.Notification().ok()?.Visual().ok()?.Bindings().ok()? {
            for t in binding.GetTextElements().ok()? {
                let s = t.Text().ok()?.to_string();
                if !s.is_empty() {
                    texts.push(s);
                }
            }
        }
        Some((who, texts))
    }

    fn handle(app: &AppHandle, c: &IslandConfig, who: &str, texts: &[String], backlog: bool) {
        let text = texts.join(" \u{b7} ");
        let followed = |p: &&Provider| c.dynamic_apps.iter().any(|a| a.eq_ignore_ascii_case(p.name));
        let state = app.state::<IslandState>();
        if let Some(p) = provider(who, &text).filter(followed) {
            if state.orders.lock().unwrap().update(p, &text, now_ms()) {
                state.wake();
                return;
            }
        }
        if c.dynamic_other && !backlog {
            activity(app, c, "bell", AGENT_COLOR, format!("{who} \u{b7} {}", texts.first().map_or("", String::as_str)));
        }
    }

    pub fn run(app: AppHandle) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut seen: HashSet<u32> = HashSet::new();
        let mut first = true;
        let mut told = false;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let c = config(&app);
            if !c.enabled || !c.dynamic_notifications {
                seen.clear();
                first = true;
                told = false;
                continue;
            }
            let Ok(listener) = UserNotificationListener::Current() else { continue };
            let mut status = listener.GetAccessStatus().unwrap_or(UserNotificationListenerAccessStatus::Denied);
            if status == UserNotificationListenerAccessStatus::Unspecified {
                status = listener.RequestAccessAsync().and_then(|a| a.join()).unwrap_or(UserNotificationListenerAccessStatus::Denied);
            }
            if status != UserNotificationListenerAccessStatus::Allowed {
                if !std::mem::replace(&mut told, true) {
                    notice(&app, "Windows blocked notification access. Allow it in Settings > Privacy & security > Notifications.".into());
                }
                continue;
            }
            let Ok(list) = listener.GetNotificationsAsync(NotificationKinds::Toast).and_then(|a| a.join()) else { continue };
            let mut now_ids = HashSet::new();
            for n in list {
                let Ok(id) = n.Id() else { continue };
                now_ids.insert(id);
                if seen.insert(id) {
                    if let Some((who, texts)) = read(&n) {
                        handle(&app, &c, &who, &texts, first);
                    }
                }
            }
            seen.retain(|id| now_ids.contains(id));
            first = false;
        }
    }
}

#[cfg(windows)]
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::Builder::new().name("notifi-dynamic".into()).spawn(move || listener::run(app)).expect("spawn dynamic notifications thread");
}

#[cfg(not(windows))]
pub fn start(_app: &tauri::AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> &'static Provider {
        PROVIDERS.iter().find(|p| p.name == name).unwrap()
    }

    #[test]
    fn provider_is_found_by_whole_word_in_app_or_text() {
        assert_eq!(provider("Just Eat", "Your order").unwrap().name, "Just Eat");
        assert_eq!(provider("Google Chrome", "Uber Eats: your food is on the way").unwrap().name, "Uber Eats");
        assert_eq!(provider("Uber", "Your driver is arriving").unwrap().name, "Uber");
        assert!(provider("Chrome", "Kubernetes is hubert").is_none());
    }

    #[test]
    fn eta_is_the_number_before_min() {
        assert_eq!(eta_minutes("Arriving in 12 min"), Some(12));
        assert_eq!(eta_minutes("10-15 minutes away"), Some(15));
        assert_eq!(eta_minutes("No time given"), None);
        assert_eq!(eta_minutes("999 min"), None);
    }

    #[test]
    fn stage_takes_the_furthest_match_and_ignores_offers() {
        assert_eq!(read_stage(p("Just Eat"), "Your order has been delivered").unwrap().label, "Delivered");
        assert_eq!(read_stage(p("Just Eat"), "Your order is being prepared").unwrap().progress, 0.3);
        assert_eq!(read_stage(p("Uber Eats"), "Arriving in 12 min").unwrap().label, "On the way");
        assert_eq!(read_stage(p("Uber Eats"), "Your courier is arriving, 2 min").unwrap().label, "Arriving");
        assert_eq!(read_stage(p("Uber"), "Your driver has arrived").unwrap().label, "Driver is here");
        assert!(read_stage(p("Deliveroo"), "20% off your next order!").is_none());
    }

    #[test]
    fn card_counts_down_and_finishes() {
        let mut o = Orders::default();
        assert!(!o.update(p("Just Eat"), "Free delivery this weekend", 0));
        assert!(o.update(p("Just Eat"), "Your order is on its way, 10 min", 0));
        let l = o.live(5 * 60_000);
        assert_eq!(l[0].value, "5 min");
        assert_eq!(l[0].progress, Some(0.7));
        assert!(o.update(p("Just Eat"), "Your order has been delivered", 6 * 60_000));
        assert_eq!(o.live(6 * 60_000)[0].status, "done");
        assert!(o.live(6 * 60_000 + 91_000).is_empty());
    }
}
