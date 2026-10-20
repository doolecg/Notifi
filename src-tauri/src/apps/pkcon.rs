//! PackageKit through its `pkcon` client (Linux), distro-neutral. Its lines are
//! `<status> <name-version.arch> (<repo>) <summary>`; the printable id is split into name and version.

use std::time::Duration;

use super::{exec, packages::Pkg};

const STATUSES: &[&str] = &[
    "Low", "Normal", "Important", "Security", "Bugfix", "Enhancement", "Blocked", "Installed", "Available", "Unknown", "Critical", "Moderate",
];
const ARCHES: &[&str] = &["x86_64", "amd64", "i686", "i386", "i586", "noarch", "all", "any", "aarch64", "arm64", "armhf", "armv7hl", "ppc64le", "s390x", "riscv64", "armv7l", "x86"];

fn split_id(token: &str) -> (String, String) {
    let body = match token.rsplit_once('.') {
        Some((rest, arch)) if ARCHES.contains(&arch) => rest,
        _ => token,
    };
    let dashes: Vec<usize> = body.match_indices('-').map(|(i, _)| i).collect();
    let starts_version = |i: usize| body[i + 1..].chars().next().is_some_and(|c| c.is_ascii_digit());
    let cut = if dashes.len() >= 2 && starts_version(dashes[dashes.len() - 2]) {
        Some(dashes[dashes.len() - 2])
    } else {
        dashes.last().copied().filter(|&i| starts_version(i))
    };
    match cut {
        Some(i) => (body[..i].to_string(), body[i + 1..].to_string()),
        None => (body.to_string(), String::new()),
    }
}

pub fn parse(text: &str, as_updates: bool) -> Vec<Pkg> {
    let mut out: Vec<Pkg> = Vec::new();
    for line in text.lines().map(str::trim) {
        let mut words = line.split_whitespace();
        let Some(first) = words.next() else { continue };
        let token = if STATUSES.contains(&first) { words.next() } else { Some(first) };
        let Some(token) = token else { continue };
        if (!token.contains('-') && !token.contains('.')) || token.ends_with(':') {
            continue;
        }
        let origin = words
            .next()
            .filter(|w| w.starts_with('(') && w.ends_with(')'))
            .map(|w| w.trim_matches(['(', ')']).to_string())
            .unwrap_or_default();
        let (name, version) = split_id(token);
        if name.is_empty() || out.iter().any(|p| p.id == name) {
            continue;
        }
        out.push(Pkg {
            manager: "pkcon".into(),
            id: name.clone(),
            name,
            version: if as_updates { String::new() } else { version.clone() },
            available: as_updates.then_some(version),
            origin,
        });
    }
    out
}

fn run(args: &[&str], secs: u64) -> Result<String, String> {
    exec::run("pkcon", args, Duration::from_secs(secs)).map(|o| o.stdout)
}

pub fn installed() -> Result<Vec<Pkg>, String> {
    Ok(parse(&run(&["--plain", "get-packages", "--filter", "installed"], 120)?, false))
}

/// Installed versions are filled in by the caller.
pub fn updates() -> Result<Vec<Pkg>, String> {
    let _ = run(&["--plain", "refresh"], 180);
    Ok(parse(&run(&["--plain", "get-updates"], 180)?, true))
}

pub fn search(query: &str) -> Result<Vec<Pkg>, String> {
    Ok(parse(&run(&["--plain", "search", "name", query], 120)?, false))
}

pub fn install_args(id: &str) -> Vec<String> {
    ["--plain", "-y", "install", id].map(String::from).to_vec()
}

pub fn update_args(id: &str) -> Vec<String> {
    ["--plain", "-y", "update", id].map(String::from).to_vec()
}

pub fn remove_args(id: &str) -> Vec<String> {
    ["--plain", "-y", "remove", id].map(String::from).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_rpm_and_deb_ids() {
        assert_eq!(split_id("bash-5.2.15-1.fc39.x86_64"), ("bash".into(), "5.2.15-1.fc39".into()));
        assert_eq!(split_id("gcc-12-base-12.2.0-3.amd64"), ("gcc-12-base".into(), "12.2.0-3".into()));
        assert_eq!(split_id("openssl-1:3.0.2-0ubuntu1.15.amd64"), ("openssl".into(), "1:3.0.2-0ubuntu1.15".into()));
        assert_eq!(split_id("foo-bar-1.0.all"), ("foo-bar".into(), "1.0".into()));
    }

    #[test]
    fn updates_listing() {
        let text = "Results:\nSecurity\topenssl-1:3.0.2-0ubuntu1.15.amd64 (jammy-security)\tSecure Sockets Layer toolkit\nNormal\tfirefox-120.0-1.fc39.x86_64 (updates)\tMozilla Firefox\n";
        let rows = parse(text, true);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].id.as_str(), rows[0].available.as_deref(), rows[0].origin.as_str()), ("openssl", Some("1:3.0.2-0ubuntu1.15"), "jammy-security"));
    }

    #[test]
    fn installed_listing_skips_noise() {
        let text = "Transaction: Getting packages\nInstalled\tbash-5.2.15-1.x86_64 (installed)\tShell\n";
        let rows = parse(text, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, "5.2.15-1");
    }
}
