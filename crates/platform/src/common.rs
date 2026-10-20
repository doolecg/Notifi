use std::sync::mpsc::Receiver;

use serde::{Deserialize, Serialize};

/// Events pushed by a backend thread; the app drains it on its own thread or task.
pub type EventStream<T> = Receiver<T>;

/// Holds an OS resource (inhibitor, grab, thumbnail) and releases it on drop.
#[must_use = "the resource is released as soon as the guard is dropped"]
pub struct Guard {
    release: Option<Box<dyn FnOnce() + Send>>,
}

impl Guard {
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self { release: Some(Box::new(release)) }
    }

    pub fn release(mut self) {
        if let Some(f) = self.release.take() {
            f();
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(f) = self.release.take() {
            f();
        }
    }
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard").field("held", &self.release.is_some()).finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Straight RGBA8 pixels, row-major, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// On battery the system draws what the battery gives; on AC it is the adapter output minus what charges the battery.
pub(crate) fn system_watts(on_ac: bool, charging: bool, battery_watts: Option<f32>, adapter_watts: Option<f32>) -> Option<f32> {
    if !on_ac {
        return battery_watts;
    }
    let charge = if charging { battery_watts? } else { 0.0 };
    adapter_watts.map(|a| (a - charge).max(0.0))
}
