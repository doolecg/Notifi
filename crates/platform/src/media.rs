//! Pure helpers shared by the media backends (SMTC on Windows, MPRIS on Linux).

use std::path::PathBuf;
use std::sync::Mutex;

use crate::traits::PlaybackState;

/// The session id the user picked; `None` means "follow the system's current one".
pub(crate) static SELECTED: Mutex<Option<String>> = Mutex::new(None);

pub(crate) fn selected() -> Option<String> {
    SELECTED.lock().ok().and_then(|g| g.clone())
}

pub(crate) fn select(id: Option<&str>) {
    if let Ok(mut g) = SELECTED.lock() {
        *g = id.map(str::to_owned);
    }
}

/// Position now, from a position reported `age_ms` ago. Only a playing track moves; the result stays within the duration.
pub(crate) fn extrapolate_ms(position_ms: u64, age_ms: u64, playing: bool, rate: f64, duration_ms: Option<u64>) -> u64 {
    let moved = if playing && rate.is_finite() && rate > 0.0 { (age_ms as f64 * rate) as u64 } else { 0 };
    let pos = position_ms.saturating_add(moved);
    match duration_ms {
        Some(d) if d > 0 => pos.min(d),
        _ => pos,
    }
}

/// Readable player name from an app id such as `Spotify.exe`, `Microsoft.ZuneMusic_8wekyb3d8bbwe!App`
/// or an MPRIS bus name `org.mpris.MediaPlayer2.vlc.instance1234`.
pub(crate) fn friendly_app_name(id: &str) -> String {
    let id = id.strip_prefix("org.mpris.MediaPlayer2.").unwrap_or(id);
    // Packaged apps: "<package>!<app>"; the part after '!' is the app.
    let id = match id.split_once('!') {
        Some((_, app)) if !app.eq_ignore_ascii_case("app") => app,
        Some((pkg, _)) => pkg,
        None => id,
    };
    // Drop an MPRIS ".instanceNNN" suffix.
    let id = match id.rfind(".instance") {
        Some(i) if id[i + 9..].chars().all(|c| c.is_ascii_digit()) => &id[..i],
        _ => id,
    };
    let id = id.strip_suffix(".exe").or_else(|| id.strip_suffix(".EXE")).unwrap_or(id);
    // Keep the last dotted segment ("Microsoft.ZuneMusic" -> "ZuneMusic"), but not for plain file names.
    let seg = id.rsplit('.').next().filter(|s| !s.is_empty()).unwrap_or(id);
    let seg = seg.split('_').next().unwrap_or(seg);
    let mut chars = seg.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => id.to_owned(),
    }
}

pub(crate) fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else {
        "application/octet-stream"
    }
}

/// `file:///home/me/a%20b.png` -> `/home/me/a b.png`. Anything that isn't a local file URL is `None`.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // Optional host: only an empty one or "localhost" is local.
    let path = if rest.starts_with('/') {
        rest
    } else {
        let (host, _) = rest.split_once('/')?;
        if !host.eq_ignore_ascii_case("localhost") {
            return None;
        }
        &rest[host.len()..]
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let mut out = Vec::with_capacity(path.len());
    let b = path.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// MPRIS `PlaybackStatus` string.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn parse_playback_status(s: &str) -> PlaybackState {
    match s {
        "Playing" => PlaybackState::Playing,
        "Paused" => PlaybackState::Paused,
        _ => PlaybackState::Stopped,
    }
}

/// Which player to show: the picked one if it is still there, else one that is playing, else the first.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn choose_player<'a>(candidates: &'a [(String, PlaybackState)], selected: Option<&str>) -> Option<&'a str> {
    if let Some(sel) = selected {
        if let Some((id, _)) = candidates.iter().find(|(id, _)| id == sel) {
            return Some(id);
        }
    }
    candidates
        .iter()
        .find(|(_, st)| *st == PlaybackState::Playing)
        .or_else(|| candidates.first())
        .map(|(id, _)| id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrapolation_moves_only_while_playing() {
        assert_eq!(extrapolate_ms(10_000, 2_500, true, 1.0, Some(200_000)), 12_500);
        assert_eq!(extrapolate_ms(10_000, 2_500, false, 1.0, Some(200_000)), 10_000);
        assert_eq!(extrapolate_ms(10_000, 2_000, true, 1.5, None), 13_000);
    }

    #[test]
    fn extrapolation_clamps_to_duration() {
        assert_eq!(extrapolate_ms(199_000, 5_000, true, 1.0, Some(200_000)), 200_000);
        assert_eq!(extrapolate_ms(5, 5, true, f64::NAN, Some(0)), 5);
    }

    #[test]
    fn app_names() {
        assert_eq!(friendly_app_name("Spotify.exe"), "Spotify");
        assert_eq!(friendly_app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "ZuneMusic");
        assert_eq!(friendly_app_name("org.mpris.MediaPlayer2.vlc"), "Vlc");
        assert_eq!(friendly_app_name("org.mpris.MediaPlayer2.firefox.instance4242"), "Firefox");
        assert_eq!(friendly_app_name("Mozilla.Firefox_n80bbvh6b1yt2!App"), "Firefox");
        assert_eq!(friendly_app_name("chrome.exe"), "Chrome");
    }

    #[test]
    fn mime_sniffing() {
        assert_eq!(sniff_mime(&[0x89, b'P', b'N', b'G', 13]), "image/png");
        assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(sniff_mime(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(sniff_mime(b"nope"), "application/octet-stream");
    }

    #[test]
    fn file_urls() {
        assert_eq!(file_url_to_path("file:///home/me/a%20b.png"), Some(PathBuf::from("/home/me/a b.png")));
        assert_eq!(file_url_to_path("file://localhost/tmp/x.jpg"), Some(PathBuf::from("/tmp/x.jpg")));
        assert_eq!(file_url_to_path("file:///tmp/%E2%99%AA.png"), Some(PathBuf::from("/tmp/♪.png")));
        assert_eq!(file_url_to_path("https://example.com/a.png"), None);
        assert_eq!(file_url_to_path("file://otherhost/tmp/x.jpg"), None);
        assert_eq!(file_url_to_path("file:///bad%zz"), None);
    }

    #[test]
    fn playback_status() {
        assert_eq!(parse_playback_status("Playing"), PlaybackState::Playing);
        assert_eq!(parse_playback_status("Paused"), PlaybackState::Paused);
        assert_eq!(parse_playback_status("Stopped"), PlaybackState::Stopped);
    }

    #[test]
    fn player_choice() {
        let c = vec![("a".to_string(), PlaybackState::Paused), ("b".to_string(), PlaybackState::Playing)];
        assert_eq!(choose_player(&c, Some("a")), Some("a"));
        assert_eq!(choose_player(&c, Some("gone")), Some("b"));
        assert_eq!(choose_player(&c, None), Some("b"));
        let paused = vec![("a".to_string(), PlaybackState::Paused)];
        assert_eq!(choose_player(&paused, None), Some("a"));
        assert_eq!(choose_player(&[], None), None);
    }
}
