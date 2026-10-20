//! Media on Linux: MPRIS (`org.mpris.MediaPlayer2.*`) over the session bus.
//! Calls are blocking D-Bus round trips, so call from a worker thread. Cover art is only read from `file://` URLs.

use std::collections::HashMap;
use std::sync::mpsc;

use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::{OwnedValue, Value};
use zbus::{message::Type as MsgType, MatchRule};

use crate::common::EventStream;
use crate::error::{PlatformError, Result};
use crate::linux_common::Linux;
use crate::media::{choose_player, file_url_to_path, friendly_app_name, parse_playback_status, select, selected, sniff_mime};
use crate::traits::*;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const OBJECT: &str = "/org/mpris/MediaPlayer2";
const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const MAX_COVER_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Metadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub length_ms: Option<u64>,
}

fn value_str(v: &Value<'_>) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.as_str().to_owned()),
        Value::Value(inner) => value_str(inner),
        _ => None,
    }
}

/// Strings come as a single string or an array of them (`xesam:artist` is `as`).
fn value_strings(v: &Value<'_>) -> String {
    match v {
        Value::Array(a) => a.iter().filter_map(value_str).collect::<Vec<_>>().join(", "),
        Value::Value(inner) => value_strings(inner),
        other => value_str(other).unwrap_or_default(),
    }
}

/// `mpris:length` is microseconds, but players disagree on the integer type.
fn value_micros(v: &Value<'_>) -> Option<u64> {
    let n = match v {
        Value::I64(n) => *n,
        Value::U64(n) => i64::try_from(*n).ok()?,
        Value::I32(n) => i64::from(*n),
        Value::U32(n) => i64::from(*n),
        Value::Value(inner) => return value_micros(inner),
        _ => return None,
    };
    u64::try_from(n).ok()
}

pub(crate) fn parse_metadata(m: &HashMap<String, OwnedValue>) -> Metadata {
    let get = |k: &str| m.get(k).map(|v| &**v);
    Metadata {
        title: get("xesam:title").map(value_strings).unwrap_or_default(),
        artist: get("xesam:artist").map(value_strings).unwrap_or_default(),
        album: get("xesam:album").map(value_strings).unwrap_or_default(),
        art_url: get("mpris:artUrl").and_then(value_str).filter(|s| !s.is_empty()),
        length_ms: get("mpris:length").and_then(value_micros).filter(|l| *l > 0).map(|us| us / 1000),
    }
}

fn conn() -> Result<Connection> {
    static CONN: std::sync::Mutex<Option<Connection>> = std::sync::Mutex::new(None);
    let mut g = CONN.lock().map_err(|_| PlatformError::backend("dbus", "connection lock poisoned"))?;
    if g.is_none() {
        *g = Some(Connection::session()?);
    }
    Ok(g.as_ref().expect("set above").clone())
}

fn proxy<'a>(conn: &'a Connection, bus: &'a str, iface: &'a str) -> Result<Proxy<'a>> {
    Ok(Proxy::new(conn, bus, OBJECT, iface)?)
}

fn bus_names(conn: &Connection) -> Result<Vec<String>> {
    let mut names: Vec<String> = DBusProxy::new(conn)?
        .list_names()?
        .into_iter()
        .map(|n| n.to_string())
        .filter(|n| n.starts_with(PREFIX))
        .collect();
    names.sort();
    Ok(names)
}

fn state_of(conn: &Connection, bus: &str) -> PlaybackState {
    proxy(conn, bus, PLAYER_IFACE)
        .and_then(|p| Ok(p.get_property::<String>("PlaybackStatus")?))
        .map(|s| parse_playback_status(&s))
        .unwrap_or(PlaybackState::Stopped)
}

fn session_of(conn: &Connection, bus: &str) -> MediaSession {
    let name = proxy(conn, bus, ROOT_IFACE)
        .and_then(|p| Ok(p.get_property::<String>("Identity")?))
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| friendly_app_name(bus));
    MediaSession { id: bus.to_owned(), name }
}

fn pick_bus(conn: &Connection) -> Result<Option<String>> {
    let candidates: Vec<(String, PlaybackState)> =
        bus_names(conn)?.into_iter().map(|b| { let s = state_of(conn, &b); (b, s) }).collect();
    Ok(choose_player(&candidates, selected().as_deref()).map(str::to_owned))
}

fn need_bus(conn: &Connection) -> Result<String> {
    pick_bus(conn)?.ok_or_else(|| PlatformError::NotFound("no media player".into()))
}

fn read_cover(url: &str) -> Option<Cover> {
    let path = file_url_to_path(url)?;
    if std::fs::metadata(&path).ok()?.len() > MAX_COVER_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mime = sniff_mime(&bytes).to_owned();
    Some(Cover { bytes, mime })
}

fn now_playing_of(conn: &Connection, bus: &str) -> Result<NowPlaying> {
    let player = proxy(conn, bus, PLAYER_IFACE)?;
    let meta = player
        .get_property::<HashMap<String, OwnedValue>>("Metadata")
        .map(|m| parse_metadata(&m))
        .unwrap_or_default();
    let flag = |name: &str, default: bool| player.get_property::<bool>(name).unwrap_or(default);
    let position_ms = player.get_property::<i64>("Position").ok().and_then(|us| u64::try_from(us).ok()).map_or(0, |us| us / 1000);
    Ok(NowPlaying {
        session: session_of(conn, bus),
        title: meta.title,
        artist: meta.artist,
        album: meta.album,
        cover: meta.art_url.as_deref().and_then(read_cover),
        duration_ms: meta.length_ms,
        position_ms: meta.length_ms.map_or(position_ms, |d| position_ms.min(d)),
        state: state_of(conn, bus),
        can_play_pause: flag("CanPlay", true) || flag("CanPause", true),
        can_prev: flag("CanGoPrevious", false),
        can_next: flag("CanGoNext", false),
        can_seek: flag("CanSeek", false),
    })
}

fn call(method: &'static str) -> Result<()> {
    let conn = conn()?;
    let bus = need_bus(&conn)?;
    proxy(&conn, &bus, PLAYER_IFACE)?.call::<_, _, ()>(method, &())?;
    Ok(())
}

impl Media for Linux {
    fn sessions(&self) -> Result<Vec<MediaSession>> {
        let conn = conn()?;
        Ok(bus_names(&conn)?.iter().map(|b| session_of(&conn, b)).collect())
    }

    fn current(&self) -> Result<Option<MediaSession>> {
        let conn = conn()?;
        Ok(pick_bus(&conn)?.map(|b| session_of(&conn, &b)))
    }

    fn set_current(&self, session_id: Option<&str>) -> Result<()> {
        select(session_id);
        Ok(())
    }

    fn now_playing(&self) -> Result<Option<NowPlaying>> {
        let conn = conn()?;
        match pick_bus(&conn)? {
            Some(bus) => now_playing_of(&conn, &bus).map(Some),
            None => Ok(None),
        }
    }

    fn play(&self) -> Result<()> {
        call("Play")
    }
    fn pause(&self) -> Result<()> {
        call("Pause")
    }
    fn toggle(&self) -> Result<()> {
        call("PlayPause")
    }
    fn next(&self) -> Result<()> {
        call("Next")
    }
    fn previous(&self) -> Result<()> {
        call("Previous")
    }

    fn seek(&self, position_ms: u64) -> Result<()> {
        let conn = conn()?;
        let bus = need_bus(&conn)?;
        let player = proxy(&conn, &bus, PLAYER_IFACE)?;
        // Seek takes a relative offset, which needs no track id.
        let current_us: i64 = player.get_property("Position")?;
        let offset_us = (position_ms as i64).saturating_mul(1000).saturating_sub(current_us);
        player.call::<_, _, ()>("Seek", &offset_us)?;
        Ok(())
    }

    fn events(&self) -> Result<EventStream<MediaEvent>> {
        let (tx, rx) = mpsc::channel();

        // Player signals: PropertiesChanged and Seeked, both emitted on the MPRIS object path.
        let conn_a = conn()?;
        let rule_a = MatchRule::builder().msg_type(MsgType::Signal).path(OBJECT)?.build();
        let iter_a = MessageIterator::for_match_rule(rule_a, &conn_a, None)?;
        let tx_a = tx.clone();
        std::thread::Builder::new().name("notifi-mpris-props".into()).spawn(move || {
            let mut last_pick = pick_bus(&conn_a).ok().flatten();
            for msg in iter_a {
                let Ok(msg) = msg else { continue };
                let header = msg.header();
                let (iface, member) = (header.interface().map(|i| i.to_string()), header.member().map(|m| m.to_string()));
                let sender = header.sender().map(|s| s.to_string());
                let mut out = Vec::new();
                match (iface.as_deref(), member.as_deref()) {
                    (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
                        let Ok((target, changed, _)) = msg.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>() else {
                            continue;
                        };
                        if target != PLAYER_IFACE {
                            continue;
                        }
                        let pick = pick_bus(&conn_a).ok().flatten();
                        let pick_changed = pick != last_pick;
                        last_pick = pick.clone();
                        let from_current = match (&pick, &sender) {
                            (Some(bus), Some(s)) => owner_of(&conn_a, bus).as_deref() == Some(s.as_str()),
                            _ => false,
                        };
                        if pick_changed {
                            out.push(MediaEvent::TrackChanged);
                            out.push(MediaEvent::PlaybackChanged);
                        } else if from_current {
                            if changed.contains_key("Metadata") {
                                out.push(MediaEvent::TrackChanged);
                            }
                            if changed.contains_key("PlaybackStatus") {
                                out.push(MediaEvent::PlaybackChanged);
                            }
                        }
                    }
                    (Some(PLAYER_IFACE), Some("Seeked")) => out.push(MediaEvent::Seeked),
                    _ => {}
                }
                if out.into_iter().any(|e| tx_a.send(e).is_err()) {
                    break;
                }
            }
        })?;

        // Players appearing and disappearing.
        let conn_b = conn()?;
        let rule_b = MatchRule::builder()
            .msg_type(MsgType::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg0ns(ROOT_IFACE)?
            .build();
        let iter_b = MessageIterator::for_match_rule(rule_b, &conn_b, None)?;
        std::thread::Builder::new().name("notifi-mpris-names".into()).spawn(move || {
            for msg in iter_b {
                let Ok(msg) = msg else { continue };
                let Ok((name, _, _)) = msg.body().deserialize::<(String, String, String)>() else { continue };
                if name.starts_with(PREFIX) && tx.send(MediaEvent::SessionsChanged).is_err() {
                    break;
                }
            }
        })?;
        Ok(rx)
    }
}

fn owner_of(conn: &Connection, bus: &str) -> Option<String> {
    DBusProxy::new(conn).ok()?.get_name_owner(bus.try_into().ok()?).ok().map(|o| o.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ov(v: impl Into<Value<'static>>) -> OwnedValue {
        OwnedValue::try_from(v.into()).unwrap()
    }

    #[test]
    fn parses_typical_metadata() {
        let mut m = HashMap::new();
        m.insert("xesam:title".to_string(), ov("Song"));
        m.insert("xesam:artist".to_string(), ov(vec!["A", "B"]));
        m.insert("xesam:album".to_string(), ov("Album"));
        m.insert("mpris:artUrl".to_string(), ov("file:///tmp/c.png"));
        m.insert("mpris:length".to_string(), ov(215_500_000i64));
        let meta = parse_metadata(&m);
        assert_eq!(meta.title, "Song");
        assert_eq!(meta.artist, "A, B");
        assert_eq!(meta.album, "Album");
        assert_eq!(meta.art_url.as_deref(), Some("file:///tmp/c.png"));
        assert_eq!(meta.length_ms, Some(215_500));
    }

    #[test]
    fn tolerates_odd_players() {
        let mut m = HashMap::new();
        m.insert("xesam:artist".to_string(), ov("Solo"));
        m.insert("mpris:length".to_string(), ov(1_000_000u64));
        let meta = parse_metadata(&m);
        assert_eq!(meta.artist, "Solo");
        assert_eq!(meta.length_ms, Some(1000));
        assert_eq!(parse_metadata(&HashMap::new()), Metadata::default());
    }
}
