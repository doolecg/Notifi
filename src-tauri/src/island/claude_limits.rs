//! Claude plan limits (5 hour and weekly) from Anthropic's usage endpoint, using the login Claude Code
//! already saved on this PC. The token is read for each request and never kept or refreshed here, so
//! Claude Code's own login is left alone.

use std::{path::Path, time::Duration};

use serde::Serialize;
use serde_json::Value;
use tauri_plugin_http::reqwest;

use super::agents::{parse_ts, Limit};

const URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub const SESSION_MS: u64 = 5 * 3600 * 1000;
pub const WEEK_MS: u64 = 7 * 24 * 3600 * 1000;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUsage {
    pub session: Option<Limit>,
    pub week: Option<Limit>,
    /// "off", "waiting", "ok", "noLogin", "expired" or "failed".
    pub status: &'static str,
    /// Unix ms of the last attempt, 0 when none yet.
    pub checked: u64,
}

impl Default for ClaudeUsage {
    fn default() -> Self {
        Self { session: None, week: None, status: "off", checked: 0 }
    }
}

/// The access token from Claude Code's login file, or why there isn't a usable one.
fn token(home: &Path, now: u64) -> Result<String, &'static str> {
    let text = std::fs::read_to_string(home.join(".claude").join(".credentials.json")).map_err(|_| "noLogin")?;
    let v: Value = serde_json::from_str(&text).map_err(|_| "noLogin")?;
    let oauth = v.get("claudeAiOauth").ok_or("noLogin")?;
    let token = oauth.get("accessToken").and_then(Value::as_str).filter(|t| !t.is_empty()).ok_or("noLogin")?;
    match oauth.get("expiresAt").and_then(Value::as_u64) {
        Some(exp) if exp <= now => Err("expired"),
        _ => Ok(token.to_string()),
    }
}

fn bucket(v: Option<&Value>, window_ms: u64) -> Option<Limit> {
    let v = v?;
    let used_percent = v.get("utilization").and_then(Value::as_f64)?;
    let resets_at = v.get("resets_at").and_then(Value::as_str).and_then(parse_ts);
    Some(Limit { used_percent, resets_at, window_ms: Some(window_ms) })
}

/// (5 hour, weekly) from the response body; either can be missing.
pub fn parse(v: &Value) -> (Option<Limit>, Option<Limit>) {
    let mut session = bucket(v.get("five_hour"), SESSION_MS);
    let mut week = bucket(v.get("seven_day"), WEEK_MS);
    // Newer responses carry a list of `{ kind, percent, resets_at }` instead.
    if let Some(list) = v.get("limits").and_then(Value::as_array) {
        for item in list {
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
            let Some(used_percent) = item.get("percent").and_then(Value::as_f64) else { continue };
            let resets_at = item.get("resets_at").and_then(Value::as_str).and_then(parse_ts);
            if session.is_none() && (kind.contains("five") || kind.contains("session")) {
                session = Some(Limit { used_percent, resets_at, window_ms: Some(SESSION_MS) });
            } else if week.is_none() && kind.contains("seven") && !kind.contains("opus") && !kind.contains("sonnet") {
                week = Some(Limit { used_percent, resets_at, window_ms: Some(WEEK_MS) });
            }
        }
    }
    (session, week)
}

/// One check: the new state, keeping the last good limits when the request fails.
pub async fn check(home: &Path, now: u64, last: &ClaudeUsage) -> ClaudeUsage {
    let keep = |status| ClaudeUsage { status, checked: now, ..last.clone() };
    let token = match token(home, now) {
        Ok(t) => t,
        Err(status) => return keep(status),
    };
    let Ok(client) = reqwest::Client::builder()
        .user_agent(concat!("Notifi/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(10))
        .build()
    else {
        return keep("failed");
    };
    let resp = client.get(URL).bearer_auth(token).header("anthropic-beta", "oauth-2025-04-20").send().await;
    let Ok(resp) = resp else { return keep("failed") };
    let code = resp.status();
    if code == reqwest::StatusCode::UNAUTHORIZED || code == reqwest::StatusCode::FORBIDDEN {
        return keep("expired");
    }
    if !code.is_success() {
        return keep("failed");
    }
    let Some(body) = resp.bytes().await.ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
        return keep("failed");
    };
    let (session, week) = parse(&body);
    if session.is_none() && week.is_none() {
        return keep("failed");
    }
    ClaudeUsage { session, week, status: "ok", checked: now }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_buckets() {
        let v: Value = serde_json::from_str(
            r#"{"five_hour":{"utilization":37.0,"resets_at":"2026-10-20T18:00:00.000Z"},"seven_day":{"utilization":12.5,"resets_at":null}}"#,
        )
        .unwrap();
        let (s, w) = parse(&v);
        assert_eq!(s.unwrap().used_percent, 37.0);
        assert!(s.unwrap().resets_at.is_some());
        assert_eq!(w.unwrap().used_percent, 12.5);
        assert_eq!(w.unwrap().resets_at, None);
    }

    #[test]
    fn reads_the_list_form_and_nulls() {
        let v: Value = serde_json::from_str(
            r#"{"five_hour":null,"seven_day":null,"limits":[{"kind":"five_hour","percent":50,"resets_at":"2026-10-20T18:00:00Z"},{"kind":"seven_day_opus","percent":90},{"kind":"seven_day","percent":20}]}"#,
        )
        .unwrap();
        let (s, w) = parse(&v);
        assert_eq!(s.unwrap().used_percent, 50.0);
        assert_eq!(w.unwrap().used_percent, 20.0);
    }

    #[test]
    fn token_needs_a_login_that_has_not_expired() {
        let dir = std::env::temp_dir().join(format!("notifi-cl-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        assert_eq!(token(&dir, 1000), Err("noLogin"));
        std::fs::write(dir.join(".claude").join(".credentials.json"), r#"{"claudeAiOauth":{"accessToken":"t","expiresAt":2000}}"#).unwrap();
        assert_eq!(token(&dir, 1000), Ok("t".to_string()));
        assert_eq!(token(&dir, 3000), Err("expired"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
