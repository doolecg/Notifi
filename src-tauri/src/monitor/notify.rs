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

#[cfg_attr(not(windows), allow(dead_code))]
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_xml() {
        assert_eq!(super::escape("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }
}
