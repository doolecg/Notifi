//! On-demand speed test against open LibreSpeed servers. Runs only when the user starts it.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_http::reqwest;

use crate::settings::Store;

const SERVERS: &str = "https://librespeed.org/backend-servers/servers.php";
const PROGRESS_EVENT: &str = "speedtest://progress";
const PHASE: Duration = Duration::from_secs(8);
const KEEP: usize = 5;

#[derive(Default)]
pub struct SpeedTestState {
    running: AtomicBool,
    history: Mutex<Vec<SpeedResult>>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SpeedResult {
    pub server: String,
    /// Bytes per second.
    pub download: f64,
    pub upload: f64,
    pub ping_ms: f64,
    pub jitter_ms: f64,
    /// Unix time in ms.
    pub at: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
    /// Bytes per second for download/upload, ms for ping.
    value: f64,
    server: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Server {
    name: String,
    server: String,
    #[serde(rename = "dlURL", default = "dl")]
    dl_url: String,
    #[serde(rename = "ulURL", default = "ul")]
    ul_url: String,
    #[serde(rename = "pingURL", default = "ul")]
    ping_url: String,
}

fn dl() -> String {
    "garbage.php".into()
}
fn ul() -> String {
    "empty.php".into()
}

impl Server {
    fn custom(base: &str) -> Self {
        Self { name: base.to_string(), server: base.to_string(), dl_url: dl(), ul_url: ul(), ping_url: ul() }
    }

    fn url(&self, path: &str) -> String {
        join(&self.server, path)
    }
}

/// Joins a server base (possibly protocol-relative) and a relative path.
fn join(base: &str, path: &str) -> String {
    let base = if base.starts_with("//") { format!("https:{base}") } else { base.to_string() };
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Mean absolute difference between consecutive pings.
fn jitter(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return 0.0;
    }
    v.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (v.len() - 1) as f64
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("Notifi/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())
}

async fn ping_once(client: &reqwest::Client, url: &str) -> Option<f64> {
    let started = Instant::now();
    let resp = client.get(url).query(&[("r", rand_token())]).timeout(Duration::from_secs(3)).send().await.ok()?;
    resp.status().is_success().then(|| started.elapsed().as_secs_f64() * 1000.0)
}

fn rand_token() -> String {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos().to_string()).unwrap_or_default()
}

async fn pick_server(client: &reqwest::Client) -> Result<Server, String> {
    let text = client
        .get(SERVERS)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the server list: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Couldn't read the server list: {e}"))?;
    let list: Vec<Server> = serde_json::from_str(&text).map_err(|e| format!("The server list was unreadable: {e}"))?;
    if list.is_empty() {
        return Err("The server list was empty.".into());
    }
    let handles: Vec<_> = list
        .into_iter()
        .map(|s| {
            let client = client.clone();
            tauri::async_runtime::spawn(async move {
                let ms = ping_once(&client, &s.url(&s.ping_url)).await;
                (s, ms)
            })
        })
        .collect();
    let mut best: Option<(Server, f64)> = None;
    for h in handles {
        if let Ok((s, Some(ms))) = h.await {
            if best.as_ref().is_none_or(|(_, b)| ms < *b) {
                best = Some((s, ms));
            }
        }
    }
    best.map(|(s, _)| s).ok_or_else(|| "No test server answered. Check the connection.".into())
}

async fn run(app: &AppHandle) -> Result<SpeedResult, String> {
    let client = client()?;
    let custom = app.state::<Store>().get().option("network", "speedTestServer").and_then(Value::as_str).map(str::trim).unwrap_or("").to_string();
    let server = if custom.is_empty() || custom == "https://" { pick_server(&client).await? } else { Server::custom(&custom) };
    let progress = |phase: &'static str, value: f64| {
        let _ = app.emit(PROGRESS_EVENT, Progress { phase, value, server: server.name.clone() });
    };

    let mut pings = Vec::new();
    for _ in 0..6 {
        if let Some(ms) = ping_once(&client, &server.url(&server.ping_url)).await {
            pings.push(ms);
            progress("ping", ms);
        }
    }
    if pings.is_empty() {
        return Err(format!("{} didn't answer.", server.name));
    }

    let started = Instant::now();
    let mut received = 0u64;
    let mut resp = client
        .get(server.url(&server.dl_url))
        .query(&[("ckSize", "100"), ("r", &rand_token())])
        .timeout(PHASE + Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("Download test failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download test failed: the server answered {}.", resp.status()));
    }
    let mut last = Instant::now();
    while started.elapsed() < PHASE {
        match resp.chunk().await {
            Ok(Some(chunk)) => received += chunk.len() as u64,
            Ok(None) => break,
            Err(e) if received == 0 => return Err(format!("Download test failed: {e}")),
            Err(_) => break,
        }
        if last.elapsed() > Duration::from_millis(250) {
            last = Instant::now();
            progress("download", received as f64 / started.elapsed().as_secs_f64());
        }
    }
    drop(resp);
    let download = received as f64 / started.elapsed().as_secs_f64().max(0.001);
    progress("download", download);

    let block = vec![0u8; 1 << 20];
    let started = Instant::now();
    let mut sent = 0u64;
    while started.elapsed() < PHASE {
        let res = client
            .post(server.url(&server.ul_url))
            .query(&[("r", rand_token())])
            .header("Content-Type", "application/octet-stream")
            .body(block.clone())
            .timeout(PHASE)
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => sent += block.len() as u64,
            Ok(r) if sent == 0 => return Err(format!("Upload test failed: the server answered {}.", r.status())),
            Err(e) if sent == 0 => return Err(format!("Upload test failed: {e}")),
            _ => break,
        }
        progress("upload", sent as f64 / started.elapsed().as_secs_f64());
    }
    let upload = sent as f64 / started.elapsed().as_secs_f64().max(0.001);

    let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    Ok(SpeedResult { server: server.name.clone(), download, upload, ping_ms: median(pings.clone()), jitter_ms: jitter(&pings), at })
}

/// The error `speed_test_run` returns when "Ask before running a speed test" is on and the caller has not asked yet.
pub const NEEDS_CONFIRMATION: &str = "confirm";

fn needs_confirmation(ask: bool, confirmed: Option<bool>) -> bool {
    ask && confirmed != Some(true)
}

/// Runs the test. With "Ask before running a speed test" on, the caller must ask first and pass `confirmed: true`.
#[tauri::command]
pub async fn speed_test_run(app: AppHandle, confirmed: Option<bool>) -> Result<SpeedResult, String> {
    let ask = app.state::<Store>().get().option("network", "askBeforeTest").and_then(Value::as_bool).unwrap_or(false);
    if needs_confirmation(ask, confirmed) {
        return Err(NEEDS_CONFIRMATION.into());
    }
    let state = app.state::<SpeedTestState>();
    if state.running.swap(true, Ordering::SeqCst) {
        return Err("A speed test is already running.".into());
    }
    let result = run(&app).await;
    state.running.store(false, Ordering::SeqCst);
    let result = result?;
    let mut history = state.history.lock().unwrap();
    history.insert(0, result.clone());
    history.truncate(KEEP);
    Ok(result)
}

#[tauri::command]
pub fn speed_test_history(app: AppHandle) -> Vec<SpeedResult> {
    app.state::<SpeedTestState>().history.lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_first_only_blocks_unconfirmed_runs() {
        assert!(!needs_confirmation(false, None));
        assert!(needs_confirmation(true, None));
        assert!(needs_confirmation(true, Some(false)));
        assert!(!needs_confirmation(true, Some(true)));
    }

    #[test]
    fn joins_server_urls() {
        assert_eq!(join("https://a.example/backend", "garbage.php"), "https://a.example/backend/garbage.php");
        assert_eq!(join("https://a.example/", "backend/empty.php"), "https://a.example/backend/empty.php");
        assert_eq!(join("//b.example", "/empty.php"), "https://b.example/empty.php");
    }

    #[test]
    fn parses_the_server_list() {
        let list: Vec<Server> = serde_json::from_str(
            r#"[{"name":"A","server":"https:\/\/a.example\/backend","id":1,"dlURL":"garbage.php","ulURL":"empty.php","pingURL":"empty.php"},{"name":"B","server":"https://b.example"}]"#,
        )
        .unwrap();
        assert_eq!(list[0].url(&list[0].dl_url), "https://a.example/backend/garbage.php");
        assert_eq!(list[1].ping_url, "empty.php");
    }

    #[test]
    fn ping_stats() {
        assert_eq!(median(vec![30.0, 10.0, 20.0]), 20.0);
        assert_eq!(jitter(&[10.0, 20.0, 15.0]), 7.5);
        assert_eq!(jitter(&[10.0]), 0.0);
    }
}
