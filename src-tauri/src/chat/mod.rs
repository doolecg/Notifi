//! AI chat on the bar (spec/apps/chat-host.md): a chat host's own web page shown inside the open Island.
//! The page is a child webview of the Island window, so it is a normal first-party page (its sign-in cookie
//! works) and it gets no access to Notifi's commands. Version 1 hosts T3 Code only.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};

use serde::Deserialize;
use serde_json::Value;
use tauri::{webview::WebviewBuilder, AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use tauri_plugin_http::reqwest;

use crate::island;

/// Label of the chat webview inside the Island window.
const WEBVIEW: &str = "chat";
const PAIR_TTL: &str = "2m";
/// Never loaded: the chat page asks for it on Escape and Notifi closes the Island instead.
const CLOSE_URL: &str = "https://notifi.invalid/close";

/// Where a host's chat page is, found from files and the local server, never from the network.
struct Host {
    origin: String,
    /// T3 Code's id for this server; its chat addresses start with it.
    environment: String,
}

/// The origin the chat was last signed in to in this run, so the one-time link is only used once per run.
static PAIRED: Mutex<Option<String>> = Mutex::new(None);

/// A chat to open next, as its address; used when a session button asks for one and the chat has not been started yet.
static WANT: Mutex<Option<String>> = Mutex::new(None);

/// Whether the chat is on screen (not parked), so it only moves in when it was away.
static VISIBLE: AtomicBool = AtomicBool::new(false);

/// Escape and the grow/shrink motion, run in the chat page (it moves itself; see the file).
const PAGE_JS: &str = include_str!("page.js");
/// Length of the plain fade out.
const FADE_OUT_MS: u64 = 140;
/// Waited after the motion out before parking, so the last frame drawn is the empty one.
const SETTLE_MS: u64 = 30;
/// Out of sight, outside the window. The chat is parked here rather than hidden: a hidden view takes about half
/// a second to draw again when shown, so the motion in would be over before it was seen.
const PARKED: i32 = -30_000;

/// How the page asks the chat to grow out of, or shrink into, the resting pill: origin and scale in the chat's own pixels, the menu's spring.
#[derive(Deserialize)]
pub struct Morph {
    ox: f64,
    oy: f64,
    sx: f64,
    sy: f64,
    ms: u64,
    ease: String,
}

/// The page call for a motion in or out; blur only on a graphics card that has power to spare.
fn motion_js(into: bool, morph: Option<&Morph>) -> String {
    static STRONG: OnceLock<bool> = OnceLock::new();
    match morph {
        Some(m) => {
            let blur = if *STRONG.get_or_init(crate::monitor::gpu::strong) { 8 } else { 0 };
            let m = serde_json::json!({ "ox": m.ox, "oy": m.oy, "sx": m.sx, "sy": m.sy, "ms": m.ms, "ease": m.ease, "blur": blur });
            format!("window.__notifi?.morph({into}, {m});")
        }
        None => format!("window.__notifi?.fade({into});"),
    }
}

/// A T3 Code chat behind a Claude session.
#[derive(Clone)]
pub struct T3Chat {
    pub thread: String,
    pub title: String,
}

/// T3 Code's chat for a Claude session id, read from T3's own database (read only). Remembered for 20 seconds.
pub fn t3_chat_of(session: &str) -> Option<T3Chat> {
    use std::{collections::HashMap, time::Instant};
    type Cache = HashMap<String, (Instant, Option<T3Chat>)>;
    static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
    if session.len() < 8 || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some((at, found)) = cache.get(session).filter(|(at, _)| at.elapsed() < Duration::from_secs(20)) {
        let _ = at;
        return found.clone();
    }
    let found = (|| {
        let db = t3_home()?.join("userdata").join("statev2.sqlite");
        let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
        let _ = conn.busy_timeout(Duration::from_millis(300));
        conn.query_row(
            "select t.thread_id, t.title from orchestration_v2_projection_provider_threads p join orchestration_v2_projection_threads t on t.thread_id = p.thread_id where p.payload_json like ?1 and t.deleted_at is null limit 1",
            [format!("%{session}%")],
            |r| Ok(T3Chat { thread: r.get(0)?, title: r.get(1)? }),
        )
        .ok()
    })();
    cache.insert(session.to_string(), (Instant::now(), found.clone()));
    found
}

fn t3_home() -> Option<PathBuf> {
    std::env::var_os("T3CODE_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| island::agents::home().map(|h| h.join(".t3")))
}

/// Only an address on this computer is ever loaded.
fn local(origin: &str) -> bool {
    ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"].iter().any(|p| origin.starts_with(p))
}

fn origin_from_runtime(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let origin = v.get("origin")?.as_str()?.trim_end_matches('/').to_string();
    local(&origin).then_some(origin)
}

async fn t3_host() -> Result<Host, String> {
    let file = t3_home().ok_or("Could not find your home folder.")?.join("userdata").join("server-runtime.json");
    let text = std::fs::read_to_string(&file).map_err(|_| "T3 Code is not installed, or has never been run.".to_string())?;
    let origin = origin_from_runtime(&text).ok_or("T3 Code's address is not on this computer.")?;
    let client = reqwest::Client::builder().timeout(Duration::from_millis(1500)).build().map_err(|e| e.to_string())?;
    let reply = client.get(format!("{origin}/.well-known/t3/environment")).send().await.ok().filter(|r| r.status().is_success());
    let Some(reply) = reply else { return Err("T3 Code is not running. Start it, then try again.".into()) };
    let environment = reply.text().await.ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.get("environmentId")?.as_str().map(str::to_string)).unwrap_or_default();
    Ok(Host { origin, environment })
}

/// A chat's address in T3 Code: `/<environment id>/<thread id>`.
fn chat_path(path: &str, environment: &str) -> bool {
    let mut parts = path.trim_start_matches('/').split('/');
    let (env, thread, rest) = (parts.next(), parts.next(), parts.next());
    !environment.is_empty() && env == Some(environment) && thread.is_some_and(|t| t.len() >= 8 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) && rest.is_none()
}

fn last_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("chat-last-path"))
}

/// The `{...}` in a command's output; T3's command can print other lines around it.
fn json_in(out: &str) -> Option<Value> {
    serde_json::from_str(out.get(out.find('{')?..=out.rfind('}')?)?).ok()
}

/// A one-time link that signs the chat in, from T3 Code's own command.
fn pair_url(origin: &str) -> Result<String, String> {
    let cli = std::env::var_os("T3CODE_CLI_PATH")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| t3_home().map(|h| h.join("bin").join(if cfg!(windows) { "t3.cmd" } else { "t3" })))
        .ok_or("Could not find the T3 Code command.")?;
    let mut cmd = std::process::Command::new(cli);
    cmd.args(["auth", "pairing", "create", "--base-url", origin, "--json", "--ttl", PAIR_TTL, "--label", "Notifi"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|_| "Could not run the T3 Code command.".to_string())?;
    let v = json_in(&String::from_utf8_lossy(&out.stdout)).ok_or("T3 Code did not give a pairing link.")?;
    let link = v.get("pairUrl").and_then(Value::as_str).ok_or("T3 Code did not give a pairing link.")?;
    link.starts_with(origin).then(|| link.to_string()).ok_or_else(|| "T3 Code gave a pairing link for another address.".into())
}

/// Shows the chat at this rectangle of the Island window (logical pixels), starting it the first time.
/// When it was hidden it comes in with `morph`, else fades in.
pub async fn show(app: &AppHandle, x: f64, y: f64, w: f64, h: f64, morph: Option<Morph>) -> Result<(), String> {
    if !island::config(app).chat_enabled {
        return Err("Turn on the AI chat button in the Island settings first.".into());
    }
    let (pos, size) = (LogicalPosition::new(x, y), LogicalSize::new(w.max(1.0), h.max(1.0)));
    if let Some(view) = app.get_webview(WEBVIEW) {
        if !VISIBLE.swap(true, Ordering::SeqCst) {
            crate::keybinds::give_way(app, &t3_shortcuts());
            // A frame on, once it is in place.
            let _ = view.eval(format!("window.__notifi?.hidden(); requestAnimationFrame(() => {{ {} }});", motion_js(true, morph.as_ref())));
        }
        let _ = view.set_position(pos);
        let _ = view.set_size(size);
        return Ok(());
    }
    let window = app.get_window(island::ISLAND).ok_or("The Island is not running.")?;
    let host = t3_host().await?;
    let paired = PAIRED.lock().unwrap().as_deref() == Some(host.origin.as_str());
    let first = if paired { format!("{}/", host.origin) } else { pair_url(&host.origin)? };
    let url = first.parse::<tauri::Url>().map_err(|e| e.to_string())?;
    // Escape in the page asks for CLOSE_URL (T3 marks every Escape as handled, so the page checks for its popups instead).
    let closer = app.clone();
    let builder = WebviewBuilder::new(WEBVIEW, WebviewUrl::External(url))
        .initialization_script(PAGE_JS.replace("__CLOSE_URL__", CLOSE_URL))
        .on_navigation(move |u| {
            if u.as_str() != CLOSE_URL {
                return true;
            }
            let _ = closer.emit_to(island::ISLAND, "island://command", "close");
            false
        });
    let view = window.add_child(builder.transparent(true), pos, size).map_err(|e| e.to_string())?;
    VISIBLE.store(true, Ordering::SeqCst);
    crate::keybinds::give_way(app, &t3_shortcuts());
    *PAIRED.lock().unwrap() = Some(host.origin.clone());
    // The chat a session button asked for, else the chat that was open last, once the sign-in has landed on its start page.
    let wanted = WANT.lock().unwrap().take();
    let saved = wanted.or_else(|| last_file(app).and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string())).filter(|p| chat_path(p, &host.environment));
    if let Some(path) = saved {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(2500));
            let at_start = view.url().is_ok_and(|u| u.path() == "/" || u.path().starts_with("/draft"));
            if let (true, Ok(url)) = (at_start, format!("{}{path}", host.origin).parse::<tauri::Url>()) {
                let _ = view.navigate(url);
            }
        });
    }
    Ok(())
}

/// The shortcuts T3 Code uses (its keybindings.json), so Notifi can let go of the same keys while the chat is in front.
fn t3_shortcuts() -> Vec<tauri_plugin_global_shortcut::Shortcut> {
    let Some(text) = t3_home().and_then(|h| std::fs::read_to_string(h.join("userdata").join("keybindings.json")).ok()) else { return Vec::new() };
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    list.iter().filter_map(|b| b.get("key")?.as_str().and_then(t3_accel)).filter_map(|a| crate::keybinds::parse(&a).ok()).collect()
}

/// T3's `mod+shift+n` as Notifi's `Ctrl+Shift+N`; keys without a Ctrl, Alt or Win key are left out (Notifi binds none).
fn t3_accel(key: &str) -> Option<String> {
    let parts: Vec<&str> = key.split('+').collect();
    // `mod++` is mod and the plus key.
    let (mods, last) = if key.ends_with("++") { (&parts[..parts.len() - 2], "=") } else { parts.split_last().map(|(l, m)| (m, *l))? };
    let mods: Vec<&str> = mods
        .iter()
        .map(|m| match *m {
            "mod" | "ctrl" => Some("Ctrl"),
            "shift" => Some("Shift"),
            "alt" => Some("Alt"),
            "meta" | "win" => Some("Super"),
            _ => None,
        })
        .collect::<Option<_>>()?;
    if !mods.iter().any(|m| *m != "Shift") {
        return None;
    }
    let mut key = last.to_string();
    if let Some(first) = key.get(..1) {
        key = first.to_uppercase() + &key[1..];
    }
    Some(format!("{}+{key}", mods.join("+")))
}

/// Moves the chat within the Island window by this many physical pixels.
pub fn shift(app: &AppHandle, dx: i32, dy: i32) {
    if (dx, dy) == (0, 0) || !VISIBLE.load(Ordering::SeqCst) {
        return;
    }
    let Some(view) = app.get_webview(WEBVIEW) else { return };
    if let Ok(p) = view.position() {
        let _ = view.set_position(tauri::PhysicalPosition::new(p.x + dx, p.y + dy));
    }
}

/// Hides the chat, remembering which chat was open so it comes back to it; `fade` first moves it out with `morph`, else fades it.
pub async fn hide(app: &AppHandle, fade: bool, morph: Option<Morph>) {
    let Some(view) = app.get_webview(WEBVIEW) else { return };
    if !VISIBLE.swap(false, Ordering::SeqCst) {
        return;
    }
    crate::keybinds::take_back(app);
    if let (Ok(url), Some(file)) = (view.url(), last_file(app)) {
        let path = url.path().to_string();
        // A chat's address has two parts (`/<environment>/<thread>`); the check on the way back in does the full test.
        if path.matches('/').count() == 2 && !path.starts_with("/draft/") {
            let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
            let _ = std::fs::write(file, path);
        }
    }
    if fade {
        let ms = morph.as_ref().map_or(FADE_OUT_MS, |m| m.ms) + SETTLE_MS;
        let _ = view.eval(motion_js(false, morph.as_ref()));
        let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(Duration::from_millis(ms))).await;
        // Opened again while it moved out: the motion in has taken over.
        if VISIBLE.load(Ordering::SeqCst) {
            return;
        }
    } else {
        // Parked empty, so nothing old shows for a frame when it comes back.
        let _ = view.eval("window.__notifi?.hidden();");
    }
    let _ = view.set_position(tauri::PhysicalPosition::new(PARKED, PARKED));
}

/// Starts Windows voice typing in the chat (Win+H).
pub fn dictate(app: &AppHandle) -> Result<(), String> {
    let view = app.get_webview(WEBVIEW).ok_or("Open the chat first.")?;
    let _ = view.set_focus();
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_H, VK_LWIN};
        let key = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        };
        // Give the chat a moment to take the keyboard before the shortcut goes to it.
        std::thread::sleep(Duration::from_millis(150));
        let keys = [key(VK_LWIN, KEYBD_EVENT_FLAGS(0)), key(VK_H, KEYBD_EVENT_FLAGS(0)), key(VK_H, KEYEVENTF_KEYUP), key(VK_LWIN, KEYEVENTF_KEYUP)];
        unsafe { SendInput(&keys, std::mem::size_of::<INPUT>() as i32) };
        Ok(())
    }
    #[cfg(not(windows))]
    Err("Dictation uses Windows voice typing and is not available here.".into())
}

/// The Island's chat page asks for the chat where its empty area is; a problem comes back as text to show there.
#[tauri::command]
pub async fn chat_show(app: AppHandle, x: f64, y: f64, w: f64, h: f64, morph: Option<Morph>) -> Result<(), String> {
    show(&app, x, y, w, h, morph).await
}

/// A session button: open the Island on this T3 Code chat.
#[tauri::command]
pub async fn chat_open_thread(app: AppHandle, thread: String) {
    if !island::config(&app).chat_enabled || !thread.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return;
    }
    match t3_host().await {
        Ok(host) => {
            let path = format!("/{}/{thread}", host.environment);
            match (app.get_webview(WEBVIEW), format!("{}{path}", host.origin).parse::<tauri::Url>()) {
                // Already running: go to it now. Otherwise it is opened on this chat when it starts.
                (Some(view), Ok(url)) => {
                    let _ = view.navigate(url);
                }
                _ => *WANT.lock().unwrap() = Some(path),
            }
            island::open_chat(&app);
        }
        Err(e) => island::notice(&app, e),
    }
}

#[tauri::command]
pub async fn chat_hide(app: AppHandle, fade: Option<bool>, morph: Option<Morph>) {
    hide(&app, fade.unwrap_or(false), morph).await;
}

#[tauri::command]
pub fn chat_dictate(app: AppHandle) {
    if let Err(e) = dictate(&app) {
        island::notice(&app, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_origins_are_loaded() {
        let runtime = |o: &str| format!(r#"{{"version":1,"origin":"{o}"}}"#);
        assert_eq!(origin_from_runtime(&runtime("http://127.0.0.1:3773/")), Some("http://127.0.0.1:3773".into()));
        assert_eq!(origin_from_runtime(&runtime("http://localhost:8080")), Some("http://localhost:8080".into()));
        assert_eq!(origin_from_runtime(&runtime("https://evil.example")), None);
        assert_eq!(origin_from_runtime(&runtime("http://127.0.0.1.evil.example:80")), None);
        assert_eq!(origin_from_runtime("not json"), None);
    }

    #[test]
    fn chat_addresses_need_the_server_id_and_a_thread() {
        assert!(chat_path("/env-1/5e43a3e8-5d91-458a-98eb", "env-1"));
        assert!(!chat_path("/draft/5e43a3e8-5d91-458a-98eb", "env-1"));
        assert!(!chat_path("/env-1/5e43a3e8-5d91-458a-98eb/extra", "env-1"));
        assert!(!chat_path("/env-1/short", "env-1"));
        assert!(!chat_path("/env-1/5e43a3e8-5d91-458a-98eb", ""));
    }

    #[test]
    fn t3_keys_read_as_notifi_shortcuts() {
        assert_eq!(t3_accel("mod+shift+n").as_deref(), Some("Ctrl+Shift+N"));
        assert_eq!(t3_accel("mod+alt+enter").as_deref(), Some("Ctrl+Alt+Enter"));
        assert_eq!(t3_accel("mod++").as_deref(), Some("Ctrl+="));
        assert_eq!(t3_accel("shift+x"), None);
        assert_eq!(t3_accel("c"), None);
        let notifi = crate::keybinds::parse("Ctrl+Shift+N").unwrap();
        assert_eq!(crate::keybinds::parse(&t3_accel("mod+shift+n").unwrap()).unwrap().id(), notifi.id());
    }

    #[test]
    fn the_command_json_is_found_among_other_output() {
        let v = json_in("[1021/203054:ERROR:electron] noise\n{\"pairUrl\":\"http://127.0.0.1:3773/pair#token=x\"}\n").unwrap();
        assert_eq!(v["pairUrl"], "http://127.0.0.1:3773/pair#token=x");
        assert!(json_in("no json here").is_none());
    }
}
