//! Flatpak (Linux). `--columns` output is tab-separated when piped.

use std::time::Duration;

use super::{exec, packages::Pkg};

fn run(args: &[&str], secs: u64) -> Result<String, String> {
    exec::run("flatpak", args, Duration::from_secs(secs)).map(|o| o.stdout)
}

/// Tab-separated rows: application id, name, version, origin.
pub fn parse(text: &str, as_updates: bool) -> Vec<Pkg> {
    text.lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('\t').map(str::trim).collect();
            let id = cells.first().filter(|id| id.contains('.') && !id.contains(' '))?;
            let version = cells.get(2).copied().unwrap_or_default().to_string();
            Some(Pkg {
                manager: "flatpak".into(),
                id: id.to_string(),
                name: cells.get(1).filter(|n| !n.is_empty()).unwrap_or(id).to_string(),
                version: if as_updates { String::new() } else { version.clone() },
                available: as_updates.then_some(version).filter(|v| !v.is_empty()),
                origin: cells.get(3).copied().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

pub fn installed() -> Result<Vec<Pkg>, String> {
    Ok(parse(&run(&["list", "--app", "--columns=application,name,version,origin"], 60)?, false))
}

/// Installed versions are filled in by the caller; the remote list only knows the new version.
pub fn updates() -> Result<Vec<Pkg>, String> {
    Ok(parse(&run(&["remote-ls", "--updates", "--app", "--columns=application,name,version,origin"], 180)?, true))
}

pub fn search(query: &str) -> Result<Vec<Pkg>, String> {
    Ok(parse(&run(&["search", "--columns=application,name,version,remotes", query], 120)?, false))
}

pub fn install_args(id: &str, remote: &str) -> Vec<String> {
    let remote = if remote.is_empty() { "flathub" } else { remote };
    ["install", "-y", "--noninteractive", remote, id].map(String::from).to_vec()
}

pub fn update_args(id: &str) -> Vec<String> {
    ["update", "-y", "--noninteractive", id].map(String::from).to_vec()
}

pub fn remove_args(id: &str) -> Vec<String> {
    ["uninstall", "-y", "--noninteractive", id].map(String::from).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_rows() {
        let text = "org.mozilla.firefox\tFirefox\t120.0\tflathub\ncom.valvesoftware.Steam\tSteam\t\tflathub\nWarning: something\n";
        let rows = parse(text, false);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].id.as_str(), rows[0].name.as_str(), rows[0].version.as_str(), rows[0].origin.as_str()), ("org.mozilla.firefox", "Firefox", "120.0", "flathub"));
    }

    #[test]
    fn update_rows_carry_the_new_version() {
        let rows = parse("org.gimp.GIMP\tGIMP\t2.10.38\tflathub\n", true);
        assert_eq!(rows[0].available.as_deref(), Some("2.10.38"));
        assert_eq!(rows[0].version, "");
    }
}
