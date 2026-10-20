use std::process::Command;

use crate::error::{PlatformError, Result};
use crate::linux_common::portal_dark_mode;

pub(crate) async fn dark_mode() -> Result<bool> {
    match (gsettings("color-scheme"), gsettings("gtk-theme")) {
        (Ok(scheme), theme) => Ok(is_dark(&scheme, theme.as_deref().unwrap_or(""))),
        (Err(e), _) => {
            tracing::debug!(error = %e, "gsettings unavailable, trying the portal");
            portal_dark_mode().await
        }
    }
}

fn gsettings(key: &str) -> Result<String> {
    let out = Command::new("gsettings").args(["get", "org.gnome.desktop.interface", key]).output()?;
    if !out.status.success() {
        return Err(PlatformError::backend("gsettings", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_string())
}

/// Older GNOME has no color-scheme key, only a "-dark" GTK theme.
pub(crate) fn is_dark(color_scheme: &str, gtk_theme: &str) -> bool {
    match color_scheme {
        "prefer-dark" => true,
        "prefer-light" => false,
        _ => gtk_theme.to_ascii_lowercase().ends_with("-dark"),
    }
}

#[cfg(test)]
mod tests {
    use super::is_dark;

    #[test]
    fn color_scheme() {
        assert!(is_dark("prefer-dark", "Adwaita"));
        assert!(!is_dark("prefer-light", "Adwaita-dark"));
        assert!(!is_dark("default", "Adwaita"));
        assert!(is_dark("default", "Yaru-dark"));
        assert!(is_dark("", "Adwaita-Dark"));
    }
}
