//! Lyrics for Now Playing from LRCLIB (https://lrclib.net), a free open lyrics database.
//! Time-synced lyrics are preferred; plain lyrics come back unsynced.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri_plugin_http::reqwest;

const API: &str = "https://lrclib.net/api";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Line {
    /// Start time; `None` in unsynced lyrics.
    ms: Option<u64>,
    text: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Lyrics {
    synced: bool,
    lines: Vec<Line>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

/// `[mm:ss.xx]` stamps at the start of a line; a line can carry several.
fn parse_lrc(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(tail) = rest.strip_prefix('[') {
            let Some((tag, after)) = tail.split_once(']') else { break };
            let Some((min, sec)) = tag.split_once(':') else { break };
            let (Ok(min), Ok(sec)) = (min.parse::<u64>(), sec.parse::<f64>()) else { break };
            stamps.push(min * 60_000 + (sec * 1000.0) as u64);
            rest = after.trim_start();
        }
        for ms in stamps {
            out.push(Line { ms: Some(ms), text: rest.to_string() });
        }
    }
    out.sort_by_key(|l| l.ms);
    out
}

fn from_record(r: Record) -> Option<Lyrics> {
    if let Some(lines) = r.synced_lyrics.as_deref().map(parse_lrc).filter(|l| !l.is_empty()) {
        return Some(Lyrics { synced: true, lines });
    }
    let lines: Vec<Line> = r.plain_lyrics?.lines().map(|t| Line { ms: None, text: t.trim().to_string() }).collect();
    (!lines.iter().all(|l| l.text.is_empty())).then_some(Lyrics { synced: false, lines })
}

async fn get<T: serde::de::DeserializeOwned>(client: &reqwest::Client, path: &str, query: &[(&str, String)]) -> Option<T> {
    let resp = client.get(format!("{API}/{path}")).query(query).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    serde_json::from_slice(&resp.bytes().await.ok()?).ok()
}

#[tauri::command]
pub async fn island_lyrics(title: String, artist: String, album: String, duration_ms: Option<u64>) -> Option<Lyrics> {
    if title.is_empty() {
        return None;
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("Notifi/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let mut exact = vec![("track_name", title.clone()), ("artist_name", artist.clone())];
    if !album.is_empty() {
        exact.push(("album_name", album));
    }
    if let Some(d) = duration_ms {
        exact.push(("duration", (d / 1000).to_string()));
    }
    if let Some(found) = get::<Record>(&client, "get", &exact).await.and_then(from_record) {
        return Some(found);
    }
    let search = [("track_name", title), ("artist_name", artist)];
    get::<Vec<Record>>(&client, "search", &search).await?.into_iter().find_map(from_record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stamps_and_multiple_stamps() {
        let l = parse_lrc("[00:01.50] one\n[00:03.00][00:02.00]two\nnot a line");
        let got: Vec<_> = l.iter().map(|l| (l.ms, l.text.as_str())).collect();
        assert_eq!(got, vec![(Some(1500), "one"), (Some(2000), "two"), (Some(3000), "two")]);
    }
}
