//! Media on Windows: the system media transport controls (SMTC) session manager.
//! WinRT async calls are awaited with `.join()`, so every method blocks briefly; call from a worker thread.

use std::sync::mpsc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows::Foundation::{IReference, TypedEventHandler};
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status, MediaPropertiesChangedEventArgs,
    PlaybackInfoChangedEventArgs, SessionsChangedEventArgs, TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};

use crate::common::EventStream;
use crate::error::{PlatformError, Result};
use crate::media::{extrapolate_ms, friendly_app_name, select, selected, sniff_mime};
use crate::traits::*;
use crate::win::Win;

/// 100 ns ticks between 1601-01-01 (WinRT DateTime) and 1970-01-01.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;
const MAX_COVER_BYTES: u64 = 16 * 1024 * 1024;

type Subscription = (Session, i64, i64, i64);

fn werr(context: &'static str) -> impl Fn(windows::core::Error) -> PlatformError {
    move |e| PlatformError::backend(context, e)
}

fn manager() -> Result<Manager> {
    Manager::RequestAsync().map_err(werr("SMTC request"))?.join().map_err(werr("SMTC request"))
}

fn session_id(s: &Session) -> String {
    s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
}

fn media_session(s: &Session) -> MediaSession {
    let id = session_id(s);
    MediaSession { name: friendly_app_name(&id), id }
}

fn all_sessions(m: &Manager) -> Vec<Session> {
    m.GetSessions().map(|v| v.into_iter().collect()).unwrap_or_default()
}

/// The picked session if it is still running, else the system's current one.
fn pick(m: &Manager) -> Option<Session> {
    if let Some(sel) = selected() {
        if let Some(s) = all_sessions(m).into_iter().find(|s| session_id(s) == sel) {
            return Some(s);
        }
    }
    m.GetCurrentSession().ok()
}

fn current_session() -> Result<Option<Session>> {
    Ok(pick(&manager()?))
}

fn read_cover(reference: IRandomAccessStreamReference) -> Option<Cover> {
    let stream = reference.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > MAX_COVER_BYTES {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    let loaded = reader.LoadAsync(size as u32).ok()?.join().ok()?;
    let mut bytes = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    let mime = stream
        .ContentType()
        .ok()
        .map(|h| h.to_string())
        .filter(|m| m.starts_with("image/"))
        .unwrap_or_else(|| sniff_mime(&bytes).to_string());
    Some(Cover { bytes, mime })
}

fn ticks_to_ms(ticks: i64) -> u64 {
    (ticks.max(0) / 10_000) as u64
}

fn now_playing_of(s: &Session) -> Result<NowPlaying> {
    let props = s
        .TryGetMediaPropertiesAsync()
        .map_err(werr("SMTC media properties"))?
        .join()
        .map_err(werr("SMTC media properties"))?;
    let text = |r: windows::core::Result<windows::core::HSTRING>| r.map(|h| h.to_string()).unwrap_or_default();
    let cover = props.Thumbnail().ok().and_then(read_cover);

    let info = s.GetPlaybackInfo().map_err(werr("SMTC playback info"))?;
    let state = match info.PlaybackStatus() {
        Ok(Status::Playing) => PlaybackState::Playing,
        Ok(Status::Paused) => PlaybackState::Paused,
        _ => PlaybackState::Stopped,
    };
    let controls = info.Controls().map_err(werr("SMTC controls"))?;
    let rate = info
        .PlaybackRate()
        .ok()
        .and_then(|r: IReference<f64>| r.Value().ok())
        .filter(|r| *r > 0.0)
        .unwrap_or(1.0);

    let (mut duration_ms, mut position_ms) = (None, 0);
    if let Ok(t) = s.GetTimelineProperties() {
        let start = t.StartTime().map(|d| d.Duration).unwrap_or(0);
        let end = t.EndTime().map(|d| d.Duration).unwrap_or(0);
        let len = ticks_to_ms(end - start);
        duration_ms = (len > 0).then_some(len);
        let pos = ticks_to_ms(t.Position().map(|d| d.Duration).unwrap_or(0) - start);
        let now_unix_ticks = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| (d.as_nanos() / 100) as i64).unwrap_or(0);
        let updated_unix_ticks = t.LastUpdatedTime().map(|d| d.UniversalTime - EPOCH_DIFF_TICKS).unwrap_or(now_unix_ticks);
        let age_ms = ticks_to_ms(now_unix_ticks - updated_unix_ticks);
        position_ms = extrapolate_ms(pos, age_ms, state == PlaybackState::Playing, rate, duration_ms);
    }

    let flag = |r: windows::core::Result<bool>| r.unwrap_or(false);
    Ok(NowPlaying {
        session: media_session(s),
        title: text(props.Title()),
        artist: text(props.Artist()),
        album: text(props.AlbumTitle()),
        cover,
        duration_ms,
        position_ms,
        state,
        can_play_pause: flag(controls.IsPlayEnabled()) || flag(controls.IsPauseEnabled()),
        can_prev: flag(controls.IsPreviousEnabled()),
        can_next: flag(controls.IsNextEnabled()),
        can_seek: flag(controls.IsPlaybackPositionEnabled()),
    })
}

fn control(context: &'static str, f: impl FnOnce(&Session) -> windows::core::Result<bool>) -> Result<()> {
    let session = current_session()?.ok_or_else(|| PlatformError::NotFound("no media session".into()))?;
    let ok = f(&session).map_err(werr(context))?;
    if ok {
        Ok(())
    } else {
        Err(PlatformError::backend(context, "the player refused the command"))
    }
}

#[derive(Clone, Copy)]
enum Raw {
    Sessions,
    Current,
    Track,
    Playback,
    Timeline,
}

fn notifier(tx: &mpsc::Sender<Raw>, raw: Raw) -> impl Fn() + Send + 'static {
    let tx = tx.clone();
    move || {
        let _ = tx.send(raw);
    }
}

fn unsubscribe(sub: Option<Subscription>) {
    if let Some((s, a, b, c)) = sub {
        let _ = s.RemoveMediaPropertiesChanged(a);
        let _ = s.RemovePlaybackInfoChanged(b);
        let _ = s.RemoveTimelinePropertiesChanged(c);
    }
}

/// Handlers on the session being shown; called again whenever that session may have changed.
fn resubscribe(mgr: &Manager, raw_tx: &mpsc::Sender<Raw>, subscribed: &mut Option<Subscription>) {
    unsubscribe(subscribed.take());
    let Some(s) = pick(mgr) else { return };
    let f = notifier(raw_tx, Raw::Track);
    let a = s.MediaPropertiesChanged(&TypedEventHandler::<Session, MediaPropertiesChangedEventArgs>::new(move |_, _| {
        f();
        Ok(())
    }));
    let f = notifier(raw_tx, Raw::Playback);
    let b = s.PlaybackInfoChanged(&TypedEventHandler::<Session, PlaybackInfoChangedEventArgs>::new(move |_, _| {
        f();
        Ok(())
    }));
    let f = notifier(raw_tx, Raw::Timeline);
    let c = s.TimelinePropertiesChanged(&TypedEventHandler::<Session, TimelinePropertiesChangedEventArgs>::new(move |_, _| {
        f();
        Ok(())
    }));
    if let (Ok(a), Ok(b), Ok(c)) = (a, b, c) {
        *subscribed = Some((s, a, b, c));
    }
}

fn event_loop(tx: mpsc::Sender<MediaEvent>, ready: mpsc::Sender<Result<()>>) {
    let (raw_tx, raw_rx) = mpsc::channel::<Raw>();
    let mgr = match manager() {
        Ok(m) => m,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let f = notifier(&raw_tx, Raw::Sessions);
    let s1 = mgr.SessionsChanged(&TypedEventHandler::<Manager, SessionsChangedEventArgs>::new(move |_, _| {
        f();
        Ok(())
    }));
    let f = notifier(&raw_tx, Raw::Current);
    let s2 = mgr.CurrentSessionChanged(&TypedEventHandler::<Manager, CurrentSessionChangedEventArgs>::new(move |_, _| {
        f();
        Ok(())
    }));
    let (Ok(tok_sessions), Ok(tok_current)) = (s1, s2) else {
        let _ = ready.send(Err(PlatformError::backend("SMTC events", "could not subscribe")));
        return;
    };
    let _ = ready.send(Ok(()));

    let mut subscribed = None;
    resubscribe(&mgr, &raw_tx, &mut subscribed);

    // Timeline events also fire as playback advances; only a jump away from the expected position is a seek.
    let mut last_position: Option<(u64, Instant)> = None;
    while let Ok(raw) = raw_rx.recv() {
        let out = match raw {
            Raw::Sessions => {
                resubscribe(&mgr, &raw_tx, &mut subscribed);
                vec![MediaEvent::SessionsChanged]
            }
            Raw::Current => {
                resubscribe(&mgr, &raw_tx, &mut subscribed);
                vec![MediaEvent::TrackChanged, MediaEvent::PlaybackChanged]
            }
            Raw::Track => vec![MediaEvent::TrackChanged],
            Raw::Playback => vec![MediaEvent::PlaybackChanged],
            Raw::Timeline => {
                let np = subscribed.as_ref().and_then(|(s, ..)| now_playing_of(s).ok());
                match np {
                    Some(np) => {
                        let jumped = match last_position {
                            Some((p, at)) => {
                                let expect = if np.state == PlaybackState::Playing { p + at.elapsed().as_millis() as u64 } else { p };
                                np.position_ms.abs_diff(expect) > 1500
                            }
                            None => false,
                        };
                        last_position = Some((np.position_ms, Instant::now()));
                        if jumped {
                            vec![MediaEvent::Seeked]
                        } else {
                            vec![]
                        }
                    }
                    None => vec![],
                }
            }
        };
        if out.into_iter().any(|e| tx.send(e).is_err()) {
            break;
        }
    }
    unsubscribe(subscribed.take());
    let _ = mgr.RemoveSessionsChanged(tok_sessions);
    let _ = mgr.RemoveCurrentSessionChanged(tok_current);
}

impl Media for Win {
    fn sessions(&self) -> Result<Vec<MediaSession>> {
        Ok(all_sessions(&manager()?).iter().map(media_session).collect())
    }

    fn current(&self) -> Result<Option<MediaSession>> {
        Ok(current_session()?.as_ref().map(media_session))
    }

    fn set_current(&self, session_id: Option<&str>) -> Result<()> {
        select(session_id);
        Ok(())
    }

    fn now_playing(&self) -> Result<Option<NowPlaying>> {
        match current_session()? {
            Some(s) => now_playing_of(&s).map(Some),
            None => Ok(None),
        }
    }

    fn play(&self) -> Result<()> {
        control("SMTC play", |s| s.TryPlayAsync()?.join())
    }
    fn pause(&self) -> Result<()> {
        control("SMTC pause", |s| s.TryPauseAsync()?.join())
    }
    fn toggle(&self) -> Result<()> {
        control("SMTC toggle", |s| s.TryTogglePlayPauseAsync()?.join())
    }
    fn next(&self) -> Result<()> {
        control("SMTC next", |s| s.TrySkipNextAsync()?.join())
    }
    fn previous(&self) -> Result<()> {
        control("SMTC previous", |s| s.TrySkipPreviousAsync()?.join())
    }
    fn seek(&self, position_ms: u64) -> Result<()> {
        // The player's own timeline may start after zero.
        let start = current_session()?
            .and_then(|s| s.GetTimelineProperties().ok())
            .and_then(|t| t.StartTime().ok())
            .map(|d| d.Duration)
            .unwrap_or(0);
        let ticks = start + (position_ms as i64).saturating_mul(10_000);
        control("SMTC seek", |s| s.TryChangePlaybackPositionAsync(ticks)?.join())
    }

    fn events(&self) -> Result<EventStream<MediaEvent>> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::Builder::new().name("notifi-media-events".into()).spawn(move || event_loop(tx, ready_tx))?;
        ready_rx.recv().map_err(|_| PlatformError::backend("SMTC events", "event thread died"))??;
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints the session playing right now, if any.
    /// Run: cargo test -p notifi-platform -- --ignored --nocapture prints_current_session
    #[test]
    #[ignore = "needs a running media player"]
    fn prints_current_session() {
        println!("sessions: {:?}", Media::sessions(&Win).unwrap());
        match Media::now_playing(&Win).unwrap() {
            Some(np) => println!(
                "now playing: {} - {} [{}] from {} ({:?}), {}/{:?} ms, cover: {:?}, prev/next/seek: {}/{}/{}",
                np.artist,
                np.title,
                np.album,
                np.session.name,
                np.state,
                np.position_ms,
                np.duration_ms,
                np.cover.as_ref().map(|c| (c.mime.clone(), c.bytes.len())),
                np.can_prev,
                np.can_next,
                np.can_seek
            ),
            None => println!("no media session is running"),
        }
    }
}
