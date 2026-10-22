//! Island options, read from the `island` namespace of the settings store with defaults for
//! anything missing, so the Island works before every field exists in the settings page.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::RwLock,
};

use super::agents::Kind;

/// Modules shown in the launcher grid, in default order. `ready` is false until the feature behind it ships.
pub const MODULES: &[(&str, &str, bool)] = &[
    ("controls", "Controls", true),
    ("mixer", "Volume mixer", true),
    ("nowPlaying", "Now Playing", true),
    ("clipboard", "Clipboard", true),
    ("captures", "Recent captures", true),
    ("shelf", "Files", true),
    ("system", "System", true),
    ("tools", "Tools", true),
    ("calendar", "Calendar", true),
    ("notifications", "Notifications", true),
    ("timer", "Timer", true),
    ("camera", "Camera mirror", true),
    ("downloads", "Downloads", true),
    ("scratchpad", "Scratchpad", true),
    ("agents", "AI Agents", true),
];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct IslandConfig {
    pub enabled: bool,
    /// "click" or "hover".
    pub open_on: String,
    pub hover_delay_ms: u32,
    pub collapse_delay_ms: u32,
    /// "primary", "pointer" or a monitor name.
    pub display: String,
    pub hide_in_fullscreen: bool,
    /// "compact", "spacious" or "custom".
    pub size: String,
    pub custom_width: f64,
    pub custom_height: f64,
    pub translucent: bool,
    pub default_module: String,
    /// Grid modules in order; ids not listed are hidden.
    pub modules: Vec<String>,
    pub orbit_left: Vec<String>,
    pub orbit_right: Vec<String>,
    pub orbit_bottom: Vec<String>,
    pub controls_buttons: Vec<String>,
    pub show_lyrics: bool,
    pub equaliser: bool,
    pub preferred_player: String,
    pub hide_timer_countdown: bool,
    pub activity_track: bool,
    pub activity_timer: bool,
    pub activity_agent: bool,
    pub activity_ms: u32,
    /// Agent turns at least this long raise a "finished" notice.
    pub agent_finish_minutes: u32,
    /// Claude plan limit per 5-hour session in API value (USD); 0 means not set.
    pub claude_plan_limit_usd: f64,
    /// AI Agents spending period: "today", "7d" or "30d".
    pub agents_period: String,
    /// What the resting pill shows on its ends when nothing live is showing.
    pub idle_battery: bool,
    pub idle_cpu_ram: bool,
    pub idle_ai_usage: bool,
    /// How the resting pill looks: "calm", "chips", "native", "agents", "field" or "rings".
    pub pill_style: String,
    /// Notifications waiting in the list, as a count on the pill.
    pub idle_notifications: bool,
    /// The city whose weather shows on the pill; empty means no weather and nothing is sent anywhere.
    pub weather_city: String,
    /// Buttons on the pill that copy a prompt and open the AI chat.
    pub ai_prompts: Vec<String>,
    /// When idle the pill shrinks to a thin bar and grows back on hover.
    pub idle_thin_bar: bool,
    /// "top" (hangs from the top centre) or "bottomLeft" (sits on the taskbar's left, grows upward).
    pub dock: String,
    /// With the Island at the top, keep the AI Agents bar on its own at the taskbar's left.
    pub agents_taskbar: bool,
    /// Read Windows notifications for delivery and ride progress (off until the user turns it on).
    pub dynamic_notifications: bool,
    /// Also flash other notifications on the pill.
    pub dynamic_other: bool,
    /// Services followed as live progress, by name.
    pub dynamic_apps: Vec<String>,
    /// A chat button on the Island and the AI Agents bar that opens the AI chat window.
    pub chat_enabled: bool,
    /// Notifi's notifications make a deep click.
    pub activity_sound: bool,
    /// Notifi's own alerts show on the Island instead of as Windows toasts.
    pub alerts_in_island: bool,
    /// Stops Notifi's notifications while on.
    pub privacy_mode: bool,
    /// Read Claude's 5 hour and weekly limits from Anthropic using Claude Code's saved login.
    pub claude_usage_connect: bool,
    /// How much darker than the taskbar the docked bar is, in percent.
    /// From the AI agents settings page (`features.aiAgents`).
    pub agents_follow: Vec<String>,
    pub log_folders: Vec<String>,
    /// "closest", "session" or "week": which limit the closed pill shows.
    pub limit_shown: String,
    pub agents_show_limits: bool,
    pub agents_show_resets: bool,
    pub agents_show_models: bool,
    pub agents_show_projects: bool,
    pub agents_show_activity: bool,
    pub limit_warning: u32,
    pub daily_budget_usd: f64,
    /// Show questions and permission requests from Claude Code in the Island.
    pub ask_in_island: bool,
}

impl Default for IslandConfig {
    fn default() -> Self {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect();
        Self {
            enabled: false,
            open_on: "click".into(),
            hover_delay_ms: 250,
            collapse_delay_ms: 600,
            display: "primary".into(),
            hide_in_fullscreen: true,
            size: "compact".into(),
            custom_width: 1200.0,
            custom_height: 270.0,
            translucent: false,
            default_module: "nowPlaying".into(),
            modules: MODULES.iter().map(|m| m.0.to_string()).collect(),
            orbit_left: s(&["grid", "timer"]),
            orbit_right: s(&["settings", "volume"]),
            orbit_bottom: s(&["nowPlaying"]),
            controls_buttons: s(&["mixer", "keepAwake", "timer", "calendar"]),
            show_lyrics: true,
            equaliser: true,
            preferred_player: String::new(),
            hide_timer_countdown: false,
            activity_track: true,
            activity_timer: true,
            activity_agent: true,
            activity_ms: 3000,
            agent_finish_minutes: 2,
            claude_plan_limit_usd: 0.0,
            agents_period: "7d".into(),
            idle_battery: true,
            idle_cpu_ram: true,
            idle_ai_usage: true,
            pill_style: "calm".into(),
            idle_notifications: true,
            weather_city: String::new(),
            ai_prompts: s(&["Explain this", "Summarize this", "Fix this"]),
            idle_thin_bar: true,
            dock: "top".into(),
            agents_taskbar: false,
            dynamic_notifications: false,
            dynamic_other: false,
            dynamic_apps: ["Just Eat", "Uber Eats", "Uber", "Deliveroo"].map(String::from).to_vec(),
            chat_enabled: false,
            activity_sound: true,
            alerts_in_island: true,
            privacy_mode: false,
            claude_usage_connect: false,
            agents_follow: Vec::new(),
            log_folders: Vec::new(),
            limit_shown: "closest".into(),
            agents_show_limits: true,
            agents_show_resets: true,
            agents_show_models: true,
            agents_show_projects: true,
            agents_show_activity: true,
            limit_warning: 80,
            daily_budget_usd: 0.0,
            ask_in_island: false,
        }
    }
}

impl IslandConfig {
    /// From the whole settings document. Without an `island` section the on/off switch falls back
    /// to the older `features.everyday.topPill` flag.
    pub fn from_settings(settings: &Value) -> Self {
        let mut c = Self::island_section(settings);
        c.apply_agent_options(settings.pointer("/features/aiAgents"));
        c
    }

    /// The AI agents page's options win over the Island's older `agentFinishMinutes`.
    fn apply_agent_options(&mut self, page: Option<&Value>) {
        let Some(page) = page.and_then(Value::as_object) else { return };
        let strings = |k: &str| -> Option<Vec<String>> {
            let list = page.get(k)?.as_array()?;
            Some(list.iter().filter_map(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        };
        let flag = |k: &str, slot: &mut bool| {
            if let Some(v) = page.get(k).and_then(Value::as_bool) {
                *slot = v;
            }
        };
        if let Some(v) = strings("agents") {
            self.agents_follow = v;
        }
        if let Some(v) = strings("logFolders") {
            self.log_folders = v;
        }
        if let Some(v) = page.get("limitShown").and_then(Value::as_str).filter(|v| matches!(*v, "closest" | "session" | "week")) {
            self.limit_shown = v.into();
        }
        flag("showLimits", &mut self.agents_show_limits);
        flag("showResets", &mut self.agents_show_resets);
        flag("showModels", &mut self.agents_show_models);
        flag("showProjects", &mut self.agents_show_projects);
        flag("showActivity", &mut self.agents_show_activity);
        if let Some(v) = page.get("finishedAfter").and_then(Value::as_f64).filter(|v| v.is_finite()) {
            self.agent_finish_minutes = v.clamp(1.0, 60.0) as u32;
        }
        if let Some(v) = page.get("limitWarning").and_then(Value::as_f64).filter(|v| v.is_finite()) {
            self.limit_warning = v.clamp(1.0, 100.0) as u32;
        }
        if let Some(v) = page.get("dailyBudget").and_then(Value::as_f64).filter(|v| v.is_finite()) {
            self.daily_budget_usd = v.max(0.0);
        }
    }

    fn island_section(settings: &Value) -> Self {
        let Some(section) = settings.get("island").filter(|v| v.is_object()) else {
            let enabled = settings.pointer("/features/everyday/topPill").and_then(Value::as_bool).unwrap_or(false);
            return Self { enabled, ..Self::default() };
        };
        // Field by field, so one bad value doesn't throw the rest away.
        let mut out = serde_json::to_value(Self::default()).unwrap_or_default();
        if let (Some(out), Some(saved)) = (out.as_object_mut(), section.as_object()) {
            for (k, v) in saved {
                if let Some(slot) = out.get_mut(k) {
                    if same_kind(slot, v) {
                        *slot = v.clone();
                    }
                }
            }
        }
        let mut c: Self = serde_json::from_value(out).unwrap_or_default();
        c.hover_delay_ms = c.hover_delay_ms.min(3000);
        c.collapse_delay_ms = c.collapse_delay_ms.min(5000);
        c.activity_ms = c.activity_ms.clamp(1000, 15000);
        if !c.claude_plan_limit_usd.is_finite() || c.claude_plan_limit_usd < 0.0 {
            c.claude_plan_limit_usd = 0.0;
        }
        if !matches!(c.dock.as_str(), "top" | "bottomLeft") {
            c.dock = "top".into();
        }
        if !matches!(c.agents_period.as_str(), "today" | "7d" | "30d") {
            c.agents_period = "7d".into();
        }
        if !matches!(c.pill_style.as_str(), "calm" | "chips" | "native" | "agents" | "field" | "rings") {
            c.pill_style = "calm".into();
        }
        c.weather_city = c.weather_city.trim().chars().take(80).collect();
        // Up to four prompts fit on the pill.
        c.ai_prompts = c.ai_prompts.iter().map(|p| p.trim().chars().take(2000).collect::<String>()).filter(|p| !p.is_empty()).take(4).collect();
        c
    }
}

/// Whether an agent is in the page's "Agents to follow" list (an empty list follows all).
pub fn follows(list: &[String], kind: Kind) -> bool {
    let name = kind.name().to_lowercase();
    let first = name.split_whitespace().next().unwrap_or_default().to_string();
    list.is_empty() || list.iter().any(|s| {
        let s = s.to_lowercase();
        s == name || s == first
    })
}

/// The percentage the closed pill shows, from a session and a week limit.
pub fn pick_limit(shown: &str, session: Option<f64>, week: Option<f64>) -> Option<f64> {
    match shown {
        "session" => session,
        "week" => week,
        _ => match (session, week) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        },
    }
}

/// 0 below 80% of the budget, 1 from 80%, 2 once it is used up.
pub fn budget_level(spent: f64, budget: f64) -> u8 {
    if budget <= 0.0 {
        0
    } else if spent >= budget {
        2
    } else if spent >= budget * 0.8 {
        1
    } else {
        0
    }
}

/// Log folders from the settings page, read on top of each tool's usual ones.
static EXTRA_ROOTS: RwLock<Vec<(Kind, PathBuf)>> = RwLock::new(Vec::new());

pub fn set_log_folders(folders: &[String]) {
    let roots: Vec<_> = folders.iter().flat_map(|f| roots_in(Path::new(f))).collect();
    if *EXTRA_ROOTS.read().unwrap() != roots {
        *EXTRA_ROOTS.write().unwrap() = roots;
    }
}

pub fn extra_roots(kind: Kind) -> Vec<PathBuf> {
    EXTRA_ROOTS.read().unwrap().iter().filter(|(k, _)| *k == kind).map(|(_, p)| p.clone()).collect()
}

/// Which tool's logs a chosen folder holds: a tool's config folder, its `projects` or `sessions`
/// folder, or the folder with OpenCode's database.
fn roots_in(dir: &Path) -> Vec<(Kind, PathBuf)> {
    let mut out = Vec::new();
    let mut add = |kind: Kind, p: PathBuf| {
        if p.is_dir() {
            out.push((kind, p));
        }
    };
    match dir.file_name().and_then(|n| n.to_str()).unwrap_or_default() {
        "projects" => add(Kind::Claude, dir.to_path_buf()),
        "sessions" | "archived_sessions" => add(Kind::Codex, dir.to_path_buf()),
        _ => {}
    }
    add(Kind::Claude, dir.join("projects"));
    add(Kind::Codex, dir.join("sessions"));
    add(Kind::Codex, dir.join("archived_sessions"));
    if dir.join("opencode.db").is_file() {
        out.push((Kind::OpenCode, dir.to_path_buf()));
    }
    out
}

fn same_kind(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(n)) => !a.is_u64() || n.is_u64(),
        (Value::Array(_), Value::Array(items)) => items.iter().all(Value::is_string),
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn falls_back_to_the_old_feature_flag() {
        let c = IslandConfig::from_settings(&json!({ "features": { "everyday": { "topPill": true } } }));
        assert!(c.enabled);
        assert_eq!(c.size, "compact");
        assert!(!IslandConfig::from_settings(&json!({})).enabled);
    }

    #[test]
    fn keeps_good_fields_and_drops_bad_ones() {
        let c = IslandConfig::from_settings(&json!({ "island": {
            "enabled": true, "size": "spacious", "hoverDelayMs": "slow", "orbitLeft": ["timer"],
            "orbitRight": [1, 2], "activityMs": 50, "unknown": 1, "collapseDelayMs": -5
        } }));
        assert!(c.enabled);
        assert_eq!(c.size, "spacious");
        assert_eq!(c.hover_delay_ms, 250);
        assert_eq!(c.orbit_left, vec!["timer"]);
        assert_eq!(c.orbit_right, IslandConfig::default().orbit_right);
        assert_eq!(c.activity_ms, 1000);
        assert_eq!(c.collapse_delay_ms, 600);
    }

    #[test]
    fn agent_dashboard_fields() {
        let c = IslandConfig::from_settings(&json!({ "island": { "claudePlanLimitUsd": 25, "agentsPeriod": "30d" } }));
        assert_eq!(c.claude_plan_limit_usd, 25.0);
        assert_eq!(c.agents_period, "30d");
        let c = IslandConfig::from_settings(&json!({ "island": { "claudePlanLimitUsd": -3, "agentsPeriod": "year" } }));
        assert_eq!(c.claude_plan_limit_usd, 0.0);
        assert_eq!(c.agents_period, "7d");
    }

    #[test]
    fn agent_page_options_apply() {
        let c = IslandConfig::from_settings(&json!({ "features": { "aiAgents": {
            "agents": ["Codex", " "], "limitShown": "week", "showModels": false, "finishedAfter": 5,
            "limitWarning": 90, "dailyBudget": 12.5, "logFolders": ["D:/logs"]
        } } }));
        assert_eq!(c.agents_follow, vec!["Codex"]);
        assert_eq!(c.limit_shown, "week");
        assert!(!c.agents_show_models && c.agents_show_limits);
        assert_eq!((c.agent_finish_minutes, c.limit_warning, c.daily_budget_usd), (5, 90, 12.5));
        assert_eq!(c.log_folders.len(), 1);
        let d = IslandConfig::from_settings(&json!({ "features": { "aiAgents": { "limitShown": "x", "dailyBudget": -4 } } }));
        assert_eq!((d.limit_shown.as_str(), d.daily_budget_usd), ("closest", 0.0));
    }

    #[test]
    fn follow_list_and_limit_choice() {
        assert!(follows(&[], Kind::Codex));
        assert!(follows(&["claude code".into()], Kind::Claude));
        assert!(follows(&["Claude".into()], Kind::Claude));
        assert!(!follows(&["Claude".into()], Kind::Codex));
        assert_eq!(pick_limit("closest", Some(30.0), Some(70.0)), Some(70.0));
        assert_eq!(pick_limit("session", Some(30.0), Some(70.0)), Some(30.0));
        assert_eq!(pick_limit("week", Some(30.0), None), None);
        assert_eq!(pick_limit("closest", None, Some(5.0)), Some(5.0));
    }

    #[test]
    fn budget_levels() {
        assert_eq!(budget_level(5.0, 0.0), 0);
        assert_eq!(budget_level(7.9, 10.0), 0);
        assert_eq!(budget_level(8.0, 10.0), 1);
        assert_eq!(budget_level(10.0, 10.0), 2);
    }

    #[test]
    fn log_folders_are_sorted_by_tool() {
        let dir = std::env::temp_dir().join(format!("notifi-logroots-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("projects")).unwrap();
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        let roots = roots_in(&dir);
        assert!(roots.contains(&(Kind::Claude, dir.join("projects"))));
        assert!(roots.contains(&(Kind::Codex, dir.join("sessions"))));
        assert!(roots_in(&dir.join("nothing")).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn idle_pill_fields() {
        let d = IslandConfig::default();
        assert!(d.idle_battery && d.idle_cpu_ram && d.idle_ai_usage && d.idle_thin_bar);
        let c = IslandConfig::from_settings(&json!({ "island": { "idleBattery": false, "idleCpuRam": "no", "idleThinBar": false } }));
        assert!(!c.idle_battery);
        assert!(c.idle_cpu_ram);
        assert!(!c.idle_thin_bar);
    }

    #[test]
    fn weather_city_and_ai_prompts() {
        let d = IslandConfig::default();
        assert!(d.idle_notifications && d.weather_city.is_empty() && d.ai_prompts.len() == 3);
        let c = IslandConfig::from_settings(&json!({ "island": { "weatherCity": "  Leeds ", "aiPrompts": [" a ", "", "b", "c", "d", "e"] } }));
        assert_eq!(c.weather_city, "Leeds");
        assert_eq!(c.ai_prompts, ["a", "b", "c", "d"]);
    }

    #[test]
    fn pill_style_is_one_of_the_six() {
        assert_eq!(IslandConfig::default().pill_style, "calm");
        let style = |v: &str| IslandConfig::from_settings(&json!({ "island": { "pillStyle": v } })).pill_style;
        for ok in ["calm", "chips", "native", "agents", "field", "rings"] {
            assert_eq!(style(ok), ok);
        }
        assert_eq!(style("neon"), "calm");
    }
}
