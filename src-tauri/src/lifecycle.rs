//! Windows exist only for installed features. A WebView2 window costs tens of megabytes even while it
//! is hidden, so a feature that is not installed never gets one; installing builds its windows and
//! uninstalling closes them. Threads and hooks are driven by each feature's own settings.

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::settings::{Settings, Store};

struct Windowed {
    /// The windows are needed while any of these features is installed.
    features: &'static [&'static str],
    /// Their labels.
    labels: &'static [&'static str],
    /// Builds whichever of the windows are missing.
    build: fn(&AppHandle) -> tauri::Result<()>,
}

const WINDOWED: &[Windowed] = &[
    Windowed { features: &["island"], labels: &[crate::island::ISLAND, crate::island::BAR], build: crate::island::build_windows },
    Windowed { features: &["commandBar"], labels: &[crate::commandbar::WINDOW], build: crate::commandbar::build_window },
    Windowed { features: &["quickPanel", "quickToggles"], labels: &[crate::quick::WINDOW], build: crate::quick::create },
    Windowed { features: &["scratchpad"], labels: &[crate::scratchpad::WINDOW], build: crate::scratchpad::create },
    Windowed { features: &["colorPicker"], labels: &[crate::picker::WINDOW], build: crate::picker::create },
    Windowed { features: &["radialMenu"], labels: &[crate::radial::WINDOW], build: crate::radial::build_window },
    Windowed { features: &["screenRecording"], labels: &[crate::recording::BAR, crate::recording::PICK], build: crate::recording::create },
    Windowed {
        features: &["screenshot", "textFromScreen", "recentCaptures", "shareLinks"],
        labels: &[crate::capture::OVERLAY, crate::capture::PREVIEW],
        build: crate::capture::create,
    },
    Windowed {
        features: &["volumeMixer", "perAppOutput", "outputSwitcher", "mediaKeyBlocker", "microphone", "extraDimming", "displays"],
        labels: crate::sound::osd::LABELS,
        build: crate::sound::osd::create,
    },
    Windowed { features: &["clipboardHistory"], labels: &[crate::clipboard::HISTORY_WINDOW], build: crate::clipboard::build_history_window },
    Windowed { features: &["textSnippets"], labels: &[crate::clipboard::SNIPPETS_WINDOW], build: crate::clipboard::build_snippets_window },
    Windowed { features: &["shelf"], labels: &[crate::clipboard::shelf::WINDOW], build: crate::clipboard::shelf::build_window },
    Windowed { features: &["windowLayout"], labels: &[crate::windows_mgmt::overlay::LABEL], build: crate::windows_mgmt::overlay::create },
    Windowed { features: &["appSwitcher"], labels: &[crate::switcher::WINDOW], build: crate::switcher::build_window },
    Windowed {
        features: &["taskbarPreview", "taskbarClicks"],
        labels: &[crate::switcher::taskbar::WINDOW],
        build: crate::switcher::taskbar::build_window,
    },
];

fn installed(s: &Settings, feature: &str) -> bool {
    if feature == "island" {
        s.island.enabled
    } else {
        s.enabled(feature)
    }
}

static BUSY: Mutex<()> = Mutex::new(());

/// Builds the windows the installed features need and closes the ones no installed feature needs.
/// Always works from the latest settings, so two quick changes end up in the right place.
pub fn sync(app: &AppHandle) {
    let _one_at_a_time = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = app.try_state::<Store>() else { return };
    let s = store.get();
    for w in WINDOWED {
        let wanted = w.features.iter().any(|f| installed(&s, f));
        let missing = w.labels.iter().any(|l| app.get_webview_window(l).is_none());
        let present = w.labels.iter().any(|l| app.get_webview_window(l).is_some());
        if wanted && missing {
            if let Err(e) = (w.build)(app) {
                eprintln!("Could not build the windows {:?}: {e}", w.labels);
            }
        } else if !wanted && present {
            for label in w.labels {
                if let Some(window) = app.get_webview_window(label) {
                    let _ = window.destroy();
                }
            }
        }
    }
}

/// After a settings change: building a webview from the thread that runs commands can deadlock on
/// Windows, so the work happens on its own thread.
pub fn sync_later(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || sync(&app));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every feature the table names is a real feature page or sub-feature, so a typo cannot leave a window never built.
    #[test]
    fn features_are_real() {
        let known: BTreeSet<&str> = crate::catalog::fields().filter(|f| f.key == "enabled").map(|f| f.namespace).collect();
        for w in WINDOWED {
            for f in w.features {
                assert!(known.contains(f) || *f == "island", "{f} is not a feature");
            }
        }
    }

    #[test]
    fn each_window_label_is_listed_once() {
        let mut seen = BTreeSet::new();
        for w in WINDOWED {
            for l in w.labels {
                assert!(seen.insert(*l), "{l} is in the table twice");
            }
        }
    }
}
