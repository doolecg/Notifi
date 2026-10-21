//! AI chat on the bar (spec/apps/chat-host.md): a chat host's own web page shown inside the open Island.
//! The page is a child webview of the Island window, so it is a normal first-party page (its sign-in cookie
//! works) and it gets no access to Notifi's commands. Version 1 hosts T3 Code only.

use std::{path::PathBuf, sync::Mutex, time::Duration};

use serde_json::Value;
use tauri::{webview::WebviewBuilder, AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use tauri_plugin_http::reqwest;

use crate::island;

/// Label of the chat webview inside the Island window.
const WEBVIEW: &str = "chat";
const PAIR_TTL: &str = "2m";

/// Where a host's chat page is, found from files and the local server, never from the network.
struct Host {
    origin: String,
    /// T3 Code's id for this server; its chat addresses start with it.
    environment: String,
}

/// The origin the chat was last signed in to in this run, so the one-time link is only used once per run.
static PAIRED: Mutex<Option<String>> = Mutex::new(None);

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
pub async fn show(app: &AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    if !island::config(app).chat_enabled {
        return Err("Turn on the AI chat button in the Island settings first.".into());
    }
    let (pos, size) = (LogicalPosition::new(x, y), LogicalSize::new(w.max(1.0), h.max(1.0)));
    if let Some(view) = app.get_webview(WEBVIEW) {
        let _ = view.set_position(pos);
        let _ = view.set_size(size);
        let _ = view.show();
        return Ok(());
    }
    let window = app.get_window(island::ISLAND).ok_or("The Island is not running.")?;
    let host = t3_host().await?;
    let paired = PAIRED.lock().unwrap().as_deref() == Some(host.origin.as_str());
    let first = if paired { format!("{}/", host.origin) } else { pair_url(&host.origin)? };
    let url = first.parse::<tauri::Url>().map_err(|e| e.to_string())?;
    let view = window.add_child(WebviewBuilder::new(WEBVIEW, WebviewUrl::External(url)), pos, size).map_err(|e| e.to_string())?;
    *PAIRED.lock().unwrap() = Some(host.origin.clone());
    // Back to the chat that was open last, once the sign-in has landed on its start page.
    let saved = last_file(app).and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string()).filter(|p| chat_path(p, &host.environment));
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

/// Hides the chat, remembering which chat was open so it comes back to it.
pub fn hide(app: &AppHandle) {
    let Some(view) = app.get_webview(WEBVIEW) else { return };
    if let (Ok(url), Some(file)) = (view.url(), last_file(app)) {
        let path = url.path().to_string();
        // A chat's address has two parts (`/<environment>/<thread>`); the check on the way back in does the full test.
        if path.matches('/').count() == 2 && !path.starts_with("/draft/") {
            let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
            let _ = std::fs::write(file, path);
        }
    }
    let _ = view.hide();
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
pub async fn chat_show(app: AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    show(&app, x, y, w, h).await
}

#[tauri::command]
pub fn chat_hide(app: AppHandle) {
    hide(&app);
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
    fn the_command_json_is_found_among_other_output() {
        let v = json_in("[1021/203054:ERROR:electron] noise\n{\"pairUrl\":\"http://127.0.0.1:3773/pair#token=x\"}\n").unwrap();
        assert_eq!(v["pairUrl"], "http://127.0.0.1:3773/pair#token=x");
        assert!(json_in("no json here").is_none());
    }
}
