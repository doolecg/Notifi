use std::{
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use notifi_platform::{Feature, Guard, KeepAwake, Platform, Support};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

const CHANGED_EVENT: &str = "keepawake://changed";

#[derive(Default)]
pub struct KeepAwakeState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    guard: Option<Guard>,
    ends_at: Option<u64>,
    generation: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    supported: bool,
    active: bool,
    /// Unix time in ms when it turns itself off; `None` while it runs until turned off.
    ends_at: Option<u64>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn status(app: &AppHandle) -> Status {
    let supported = *app.state::<Platform>().capabilities().get(Feature::KeepAwake) == Support::Supported;
    let state = app.state::<KeepAwakeState>();
    let inner = state.inner.lock().unwrap();
    Status { supported, active: inner.guard.is_some(), ends_at: inner.ends_at }
}

fn changed(app: &AppHandle) -> Status {
    let status = status(app);
    let _ = app.emit(CHANGED_EVENT, &status);
    status
}

#[tauri::command]
pub fn keep_awake_status(app: AppHandle) -> Status {
    status(&app)
}

/// Turns keep awake on for `minutes` (0 = until turned off) or off.
#[tauri::command]
pub async fn set_keep_awake(app: AppHandle, on: bool, minutes: u32) -> Result<Status, String> {
    let state: State<KeepAwakeState> = app.state();
    let old = {
        let mut inner = state.inner.lock().unwrap();
        inner.generation += 1;
        inner.ends_at = None;
        inner.guard.take()
    };
    drop(old);
    if !on {
        return Ok(changed(&app));
    }
    let platform = app.state::<Platform>().inner().clone();
    let request = KeepAwake { keep_display_on: true, reason: "Keep awake is on".into() };
    let guard = platform.power().keep_awake(request).await.map_err(|e| e.to_string())?;
    let generation = {
        let mut inner = state.inner.lock().unwrap();
        inner.guard = Some(guard);
        if minutes > 0 {
            inner.ends_at = Some(now_ms() + u64::from(minutes) * 60_000);
        }
        inner.generation
    };
    if minutes > 0 {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(u64::from(minutes) * 60));
            let state = app.state::<KeepAwakeState>();
            let expired = {
                let mut inner = state.inner.lock().unwrap();
                (inner.generation == generation).then(|| {
                    inner.ends_at = None;
                    inner.guard.take()
                })
            };
            if let Some(guard) = expired {
                drop(guard);
                changed(&app);
            }
        });
    }
    Ok(changed(&app))
}
