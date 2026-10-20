use crate::error::Result;
use crate::linux_common::portal_dark_mode;
use crate::{gnome, kde};

/// X11 sessions can run any desktop, so ask the portal first, then the desktop's own setting.
pub(crate) async fn dark_mode() -> Result<bool> {
    match portal_dark_mode().await {
        Ok(dark) => Ok(dark),
        Err(e) => {
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_uppercase();
            if desktop.split(':').any(|d| d == "KDE") {
                kde::dark_mode().await
            } else if desktop.contains("GNOME") {
                gnome::dark_mode().await
            } else {
                Err(e)
            }
        }
    }
}
