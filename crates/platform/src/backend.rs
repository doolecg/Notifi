use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Windows,
    X11,
    KdeWayland,
    GnomeWayland,
    OtherWayland,
    /// Any OS without a backend; every capability is unsupported.
    Other,
}

/// The environment variables backend detection looks at, so it can be tested without touching the process env.
#[derive(Debug, Clone, Default)]
pub struct SessionEnv {
    pub session_type: Option<String>,
    pub current_desktop: Option<String>,
    pub wayland_display: Option<String>,
    pub display: Option<String>,
}

impl SessionEnv {
    pub fn from_process() -> Self {
        let var = |k| std::env::var(k).ok().filter(|v: &String| !v.is_empty());
        Self {
            session_type: var("XDG_SESSION_TYPE"),
            current_desktop: var("XDG_CURRENT_DESKTOP"),
            wayland_display: var("WAYLAND_DISPLAY"),
            display: var("DISPLAY"),
        }
    }
}

impl Backend {
    pub fn detect() -> Self {
        if cfg!(windows) {
            Backend::Windows
        } else if cfg!(target_os = "linux") {
            Self::detect_linux(&SessionEnv::from_process())
        } else {
            Backend::Other
        }
    }

    pub fn detect_linux(env: &SessionEnv) -> Self {
        let session = env.session_type.as_deref().unwrap_or("").to_ascii_lowercase();
        let wayland = match session.as_str() {
            "wayland" => true,
            "x11" => false,
            // tty, unspecified or missing: fall back to which display server is reachable.
            _ => env.wayland_display.is_some() || env.display.is_none(),
        };
        if !wayland {
            return Backend::X11;
        }
        let desktops: Vec<String> = env
            .current_desktop
            .as_deref()
            .unwrap_or("")
            .split(':')
            .map(|d| d.trim().to_ascii_uppercase())
            .collect();
        if desktops.iter().any(|d| d == "KDE") {
            Backend::KdeWayland
        } else if desktops.iter().any(|d| d == "GNOME" || d.starts_with("GNOME-")) {
            Backend::GnomeWayland
        } else {
            Backend::OtherWayland
        }
    }

    pub fn is_wayland(self) -> bool {
        matches!(self, Backend::KdeWayland | Backend::GnomeWayland | Backend::OtherWayland)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(session: Option<&str>, desktop: Option<&str>, wl: Option<&str>, x: Option<&str>) -> SessionEnv {
        SessionEnv {
            session_type: session.map(Into::into),
            current_desktop: desktop.map(Into::into),
            wayland_display: wl.map(Into::into),
            display: x.map(Into::into),
        }
    }

    #[test]
    fn x11_session_is_x11_whatever_the_desktop() {
        assert_eq!(Backend::detect_linux(&env(Some("x11"), Some("KDE"), None, Some(":0"))), Backend::X11);
        assert_eq!(Backend::detect_linux(&env(Some("x11"), Some("ubuntu:GNOME"), None, Some(":0"))), Backend::X11);
        assert_eq!(Backend::detect_linux(&env(Some("X11"), None, None, None)), Backend::X11);
    }

    #[test]
    fn wayland_desktops() {
        assert_eq!(Backend::detect_linux(&env(Some("wayland"), Some("KDE"), Some("wayland-0"), None)), Backend::KdeWayland);
        assert_eq!(Backend::detect_linux(&env(Some("wayland"), Some("ubuntu:GNOME"), None, None)), Backend::GnomeWayland);
        assert_eq!(Backend::detect_linux(&env(Some("wayland"), Some("GNOME-Classic:GNOME"), None, None)), Backend::GnomeWayland);
        assert_eq!(Backend::detect_linux(&env(Some("wayland"), Some("sway"), None, None)), Backend::OtherWayland);
        assert_eq!(Backend::detect_linux(&env(Some("wayland"), Some("Hyprland"), None, None)), Backend::OtherWayland);
        assert_eq!(Backend::detect_linux(&env(Some("wayland"), None, None, None)), Backend::OtherWayland);
    }

    #[test]
    fn unknown_session_falls_back_to_display_vars() {
        assert_eq!(Backend::detect_linux(&env(Some("tty"), Some("KDE"), Some("wayland-0"), Some(":0"))), Backend::KdeWayland);
        assert_eq!(Backend::detect_linux(&env(None, Some("GNOME"), None, Some(":1"))), Backend::X11);
        assert_eq!(Backend::detect_linux(&env(None, None, None, None)), Backend::OtherWayland);
    }

    #[test]
    fn is_wayland() {
        assert!(Backend::GnomeWayland.is_wayland());
        assert!(!Backend::X11.is_wayland());
        assert!(!Backend::Windows.is_wayland());
    }
}
