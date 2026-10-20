//! OS notifications for alerts: Windows toasts, freedesktop notifications over D-Bus on Linux.
//! Do Not Disturb is honoured by the OS on both (Focus Assist, the notification daemon).

pub fn send(title: String, body: String, sound: bool) {
    std::thread::spawn(move || {
        if let Err(e) = show(&title, &body, sound) {
            eprintln!("notifi: notification failed: {e}");
        }
    });
}

#[cfg(windows)]
fn show(title: &str, body: &str, sound: bool) -> windows::core::Result<()> {
    use windows::{
        core::HSTRING,
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
    };
    // Installed builds carry the app id on their Start menu shortcut; dev runs borrow PowerShell's.
    let app_id = if cfg!(debug_assertions) {
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe"
    } else {
        "io.notifi.app"
    };
    let audio = if sound { "" } else { "<audio silent=\"true\"/>" };
    let xml = format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>{audio}</toast>",
        escape(title),
        escape(body)
    );
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?.Show(&toast)
}

#[cfg(target_os = "linux")]
fn show(title: &str, body: &str, sound: bool) -> zbus::Result<()> {
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    let conn = zbus::blocking::Connection::session()?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("suppress-sound", Value::from(!sound));
    hints.insert("category", Value::from("device"));
    conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &("Notifi", 0u32, "dialog-warning", title, body, Vec::<&str>::new(), hints, -1i32),
    )?;
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn show(_title: &str, _body: &str, _sound: bool) -> Result<(), String> {
    Err("notifications are not available on this platform".into())
}

/// Whether the system's Do Not Disturb switch is on: toast banners off on Windows, banners off on GNOME.
pub fn do_not_disturb() -> bool {
    use std::process::Command;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings", "/v", "NOC_GLOBAL_SETTING_TOASTS_ENABLED"])
            .creation_flags(0x0800_0000)
            .output();
        out.ok().and_then(|o| reg_dword(&String::from_utf8_lossy(&o.stdout))) == Some(0)
    }
    #[cfg(not(windows))]
    {
        let out = Command::new("gsettings").args(["get", "org.gnome.desktop.notifications", "show-banners"]).output();
        out.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "false")
    }
}

/// The number in a `reg query` line such as `    NAME    REG_DWORD    0x0`.
#[cfg_attr(not(windows), allow(dead_code))]
fn reg_dword(output: &str) -> Option<u32> {
    let line = output.lines().find(|l| l.contains("REG_DWORD"))?;
    let hex = line.split_whitespace().last()?.strip_prefix("0x")?;
    u32::from_str_radix(hex, 16).ok()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_a_registry_dword() {
        assert_eq!(super::reg_dword("\r\nHKEY_CURRENT_USER\\X\r\n    NOC    REG_DWORD    0x0\r\n"), Some(0));
        assert_eq!(super::reg_dword("    NOC    REG_DWORD    0x1f"), Some(31));
        assert_eq!(super::reg_dword("ERROR: The system was unable to find the specified registry key or value."), None);
    }

    #[test]
    fn escapes_xml() {
        assert_eq!(super::escape("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }
}
