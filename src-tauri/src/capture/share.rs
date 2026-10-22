//! Temporary links (spec/capture/share-links.md): uploads a capture or a finished recording to the
//! user's own share server and copies the link.
//!
//! Protocol the server must speak:
//! - `POST <server>/upload`, `multipart/form-data` with the text field `expires` (requested lifetime in
//!   seconds) and the file field `file`. `Authorization: Bearer <token>` is sent only when a token is set.
//! - A 2xx JSON reply `{"url": "...", "expires": <seconds, optional>, "delete": "<url, optional>"}`.
//!   `url` and `delete` may be absolute or start with `/` (joined to the server address).
//! - `DELETE <delete url>` with the same bearer header revokes a link that came back too late.

use std::{
    sync::{atomic::Ordering, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_http::reqwest;

use super::{encode, export, notice, number, text, CaptureState, Last, Opts};
use crate::settings::Store;

pub const NS: &str = "shareLinks";
/// Recordings are offered 1 or 6 hours; screenshots 1, 6 or 24.
const SHOT_MAX_HOURS: u32 = 24;
const RECORDING_MAX_HOURS: u32 = 6;

#[derive(Clone)]
pub struct Config {
    server: String,
    token: String,
    hours: u32,
    limit: u64,
}

#[derive(Debug, PartialEq)]
pub struct Link {
    pub url: String,
    pub expires: Option<u64>,
    pub delete: Option<String>,
}

#[derive(Deserialize)]
struct Reply {
    url: String,
    expires: Option<u64>,
    delete: Option<String>,
}

// ---- Pure parts ----

fn is_local(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "[::1]"
}

/// The server address without a trailing slash. Needs `https://`; plain `http://` only for this machine.
pub fn server_base(server: &str) -> Option<String> {
    let s = server.trim().trim_end_matches('/');
    let (secure, rest) = match s.strip_prefix("https://") {
        Some(r) => (true, r),
        None => (false, s.strip_prefix("http://")?),
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let name = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let bare = if name.starts_with('[') { name.split_once(']').map_or(name, |(h, _)| &name[..=h.len()]) } else { name.split(':').next().unwrap_or("") };
    if bare.is_empty() || host.contains('@') || (!secure && !is_local(bare)) {
        return None;
    }
    Some(s.to_string())
}

/// `scheme://host[:port]` of a server address, for links the server gives as paths.
fn origin(base: &str) -> &str {
    let after = base.find("://").map_or(0, |i| i + 3);
    base[after..].find('/').map_or(base, |i| &base[..after + i])
}

pub fn upload_url(base: &str) -> String {
    format!("{base}/upload")
}

/// Lifetime in seconds for the chosen hours, kept to what the kind of file is offered.
pub fn expiry_secs(pick: &str, max_hours: u32) -> u64 {
    u64::from(pick.trim().parse::<u32>().unwrap_or(1).clamp(1, max_hours)) * 3600
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    if cleaned.is_empty() {
        "capture".into()
    } else {
        cleaned
    }
}

pub fn content_type(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

pub fn multipart(boundary: &str, filename: &str, mime: &str, bytes: &[u8], expires: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 512);
    out.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"expires\"\r\n\r\n{expires}\r\n").as_bytes());
    out.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {mime}\r\n\r\n", safe_name(filename)).as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    out
}

fn absolute(base: &str, link: &str) -> Result<String, String> {
    if link.starts_with("https://") || link.starts_with("http://") {
        Ok(link.to_string())
    } else if link.starts_with('/') {
        Ok(format!("{}{link}", origin(base)))
    } else {
        Err("The server sent a link that is not a web address.".into())
    }
}

pub fn parse_reply(body: &[u8], base: &str) -> Result<Link, String> {
    let reply: Reply = serde_json::from_slice(body).map_err(|_| "The server's reply was not what Notifi expects (JSON with a url).".to_string())?;
    // The delete link carries the user's token, so it must stay on the server the file went to.
    let delete = match reply.delete.as_deref() {
        Some(d) if !d.is_empty() => absolute(base, d).ok().filter(|d| origin(d) == origin(base)),
        _ => None,
    };
    Ok(Link { url: absolute(base, &reply.url)?, expires: reply.expires, delete })
}

fn mime_of(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// "in 1 hour", "in 6 hours" for the notice.
fn lifetime_label(secs: u64) -> String {
    let h = (secs / 3600).max(1);
    format!("{h} hour{}", if h == 1 { "" } else { "s" })
}

fn boundary() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("notifi-{n:x}")
}

// ---- Settings ----

/// The sharing settings, or None while links are off or no usable server is set.
fn config(app: &AppHandle) -> Option<Config> {
    let s = app.state::<Store>().get();
    if !s.enabled(NS) {
        return None;
    }
    Some(Config {
        server: server_base(&text(&s, NS, "server", ""))?,
        token: text(&s, NS, "token", "").trim().to_string(),
        hours: number(&s, NS, "expiry", 1.0) as u32,
        limit: (number(&s, NS, "recordingLimit", 100.0).max(1.0) as u64) * 1024 * 1024,
    })
}

/// Whether the share actions are shown: links are on and the server address is usable.
pub(super) fn available(app: &AppHandle) -> bool {
    config(app).is_some()
}

// ---- Network ----

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("Notifi/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())
}

async fn post(cfg: &Config, filename: &str, mime: &str, bytes: &[u8], expires: u64) -> Result<Link, String> {
    let boundary = boundary();
    let mut req = client()?
        .post(upload_url(&cfg.server))
        .header("Content-Type", content_type(&boundary))
        .body(multipart(&boundary, filename, mime, bytes, expires));
    if !cfg.token.is_empty() {
        req = req.bearer_auth(&cfg.token);
    }
    let resp = req.send().await.map_err(|_| "Could not reach the share server.".to_string())?;
    let code = resp.status();
    if code == reqwest::StatusCode::UNAUTHORIZED || code == reqwest::StatusCode::FORBIDDEN {
        return Err("The share server refused the access token.".into());
    }
    if code == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        return Err("The share server says the file is too big.".into());
    }
    if !code.is_success() {
        return Err(format!("The share server answered {}.", code.as_u16()));
    }
    let body = resp.bytes().await.map_err(|_| "The share server's reply was cut off.".to_string())?;
    parse_reply(&body, &cfg.server)
}

async fn revoke(cfg: &Config, link: &Link) {
    let Some(url) = &link.delete else { return };
    let Ok(client) = client() else { return };
    let mut req = client.delete(url);
    if !cfg.token.is_empty() {
        req = req.bearer_auth(&cfg.token);
    }
    let _ = req.send().await;
}

/// Uploads, then copies the link, unless the capture was edited or replaced meanwhile (`generation`
/// moved on), in which case the late link is revoked and never copied.
fn spawn_upload(app: &AppHandle, cfg: Config, filename: String, mime: &'static str, bytes: Vec<u8>, expires: u64) {
    let generation = app.state::<CaptureState>().share_gen.load(Ordering::SeqCst);
    let app = app.clone();
    notice(&app, "Uploading the link...");
    tauri::async_runtime::spawn(async move {
        match post(&cfg, &filename, mime, &bytes, expires).await {
            Ok(link) => {
                if app.state::<CaptureState>().share_gen.load(Ordering::SeqCst) != generation {
                    revoke(&cfg, &link).await;
                    return;
                }
                match crate::clipboard::set_text(&link.url) {
                    Ok(()) => notice(&app, format!("Link copied, gone in {}", lifetime_label(link.expires.unwrap_or(expires)))),
                    Err(e) => notice(&app, format!("Could not copy the link: {e}")),
                }
            }
            Err(e) => notice(&app, format!("{e} The capture is kept; try again.")),
        }
    });
}

/// Stops a pending upload from copying its link (the capture was edited, replaced or discarded).
pub(super) fn cancel_pending(app: &AppHandle) {
    app.state::<CaptureState>().share_gen.fetch_add(1, Ordering::SeqCst);
}

/// Uploads a capture in the user's chosen file format and copies the link.
pub(super) fn share_shot(app: &AppHandle, last: &Arc<Last>) {
    let Some(cfg) = config(app) else {
        notice(app, "Turn on temporary links and set a server in the Sharing tab first.");
        return;
    };
    let o = Opts::read(app);
    let (bytes, ext) = match encode(&export(&o, &last.img, last.scale, true), &o.format, o.quality) {
        Ok(v) => v,
        Err(e) => return notice(app, e),
    };
    let expires = expiry_secs(&cfg.hours.to_string(), SHOT_MAX_HOURS);
    spawn_upload(app, cfg, format!("capture.{ext}"), mime_of(ext), bytes, expires);
}

/// Uploads a file from disk (a finished recording). Files over the size limit are refused with a message:
/// Notifi has no way yet to re-encode a finished video smaller.
#[tauri::command]
pub fn capture_share_file(app: AppHandle, path: String) {
    let Some(cfg) = config(&app) else {
        notice(&app, "Turn on temporary links and set a server in the Sharing tab first.");
        return;
    };
    // Only a capture or recording Notifi made and lists, never any file the page names.
    if super::recent::listed(&app, &path).is_err() {
        notice(&app, "That file is not one of your recent captures, so it was not uploaded.");
        return;
    }
    let path = std::path::PathBuf::from(path);
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if size > cfg.limit {
        notice(&app, format!("That file is {} MB, over the {} MB limit set in the Sharing tab.", size.div_ceil(1024 * 1024), cfg.limit / (1024 * 1024)));
        return;
    }
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return notice(&app, format!("Could not read the file: {e}")),
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let expires = expiry_secs(&cfg.hours.to_string(), RECORDING_MAX_HOURS);
    spawn_upload(&app, cfg, name, mime_of(&ext), bytes, expires);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_needs_https_except_on_this_machine() {
        assert_eq!(server_base(" https://share.example.com/ ").as_deref(), Some("https://share.example.com"));
        assert_eq!(server_base("https://example.com:8443/s").as_deref(), Some("https://example.com:8443/s"));
        assert_eq!(server_base("http://localhost:3000").as_deref(), Some("http://localhost:3000"));
        assert_eq!(server_base("http://127.0.0.1").as_deref(), Some("http://127.0.0.1"));
        assert_eq!(server_base("http://share.example.com"), None);
        assert_eq!(server_base("share.example.com"), None);
        assert_eq!(server_base("https://"), None);
        assert_eq!(server_base("https://user:pw@example.com"), None);
        assert_eq!(server_base(""), None);
    }

    #[test]
    fn upload_url_and_origin() {
        assert_eq!(upload_url("https://a.b/s"), "https://a.b/s/upload");
        assert_eq!(origin("https://a.b:8443/s"), "https://a.b:8443");
        assert_eq!(origin("https://a.b"), "https://a.b");
    }

    #[test]
    fn expiry_maps_to_seconds_within_the_offer() {
        assert_eq!(expiry_secs("1", 24), 3600);
        assert_eq!(expiry_secs("6", 24), 21600);
        assert_eq!(expiry_secs("24", 24), 86400);
        assert_eq!(expiry_secs("24", 6), 21600);
        assert_eq!(expiry_secs("junk", 24), 3600);
        assert_eq!(expiry_secs("0", 24), 3600);
    }

    #[test]
    fn multipart_has_the_expiry_then_the_file() {
        let body = multipart("BND", "my shot\".png", "image/png", b"\x89PNG", 3600);
        let text = String::from_utf8_lossy(&body).into_owned();
        assert!(text.starts_with("--BND\r\nContent-Disposition: form-data; name=\"expires\"\r\n\r\n3600\r\n--BND\r\n"));
        assert!(text.contains("name=\"file\"; filename=\"my_shot_.png\"\r\nContent-Type: image/png\r\n\r\n\u{FFFD}PNG\r\n--BND--\r\n"));
        assert!(body.windows(4).any(|w| w == b"\x89PNG"));
        assert_eq!(content_type("BND"), "multipart/form-data; boundary=BND");
    }

    #[test]
    fn reply_links_can_be_absolute_or_paths() {
        let l = parse_reply(br#"{"url":"https://x.io/f/1","expires":3600,"delete":"/d/1"}"#, "https://x.io/s").unwrap();
        assert_eq!(l, Link { url: "https://x.io/f/1".into(), expires: Some(3600), delete: Some("https://x.io/d/1".into()) });
        let p = parse_reply(br#"{"url":"/f/2"}"#, "https://x.io/s").unwrap();
        assert_eq!(p, Link { url: "https://x.io/f/2".into(), expires: None, delete: None });
    }

    #[test]
    fn a_delete_link_on_another_server_is_dropped() {
        let l = parse_reply(br#"{"url":"https://x.io/f/1","delete":"https://evil.example/d/1"}"#, "https://x.io/s").unwrap();
        assert_eq!(l.delete, None);
    }

    #[test]
    fn bad_replies_are_errors() {
        assert!(parse_reply(b"not json", "https://x.io").is_err());
        assert!(parse_reply(br#"{"nope":1}"#, "https://x.io").is_err());
        assert!(parse_reply(br#"{"url":"javascript:alert(1)"}"#, "https://x.io").is_err());
    }

    #[test]
    fn labels_and_types() {
        assert_eq!(lifetime_label(3600), "1 hour");
        assert_eq!(lifetime_label(21600), "6 hours");
        assert_eq!(mime_of("PNG"), "image/png");
        assert_eq!(mime_of("mp4"), "video/mp4");
        assert_eq!(mime_of("zip"), "application/octet-stream");
    }
}
