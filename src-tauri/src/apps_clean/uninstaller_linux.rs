//! Linux side of the uninstaller: `.desktop` entries, then the package manager that owns each.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use super::{
    fsx,
    leftovers::{self, Identity, Leftover, Others},
    uninstaller::{App, Remove},
};

#[derive(Debug, Default, PartialEq)]
pub struct Desktop {
    pub name: String,
    pub exec: String,
    pub hidden: bool,
}

/// Reads the `[Desktop Entry]` group of a `.desktop` file.
pub fn parse_desktop(text: &str) -> Option<Desktop> {
    let mut entry = Desktop::default();
    let mut in_group = false;
    let mut is_app = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if !in_group {
            continue;
        }
        match k {
            "Name" => entry.name = v.to_string(),
            "Exec" => entry.exec = v.to_string(),
            "Type" => is_app = v == "Application",
            "NoDisplay" | "Hidden" => entry.hidden |= v.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    (is_app && !entry.name.is_empty()).then_some(entry)
}

/// The program path from an `Exec=` line, without field codes.
pub fn exec_program(exec: &str) -> String {
    let exec = exec.trim();
    if let Some(rest) = exec.strip_prefix('"') {
        return rest.split('"').next().unwrap_or_default().to_string();
    }
    exec.split_whitespace().next().unwrap_or_default().to_string()
}

fn application_dirs() -> Vec<PathBuf> {
    let home = fsx::home();
    vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        home.join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ]
}

fn has(tool: &str) -> bool {
    fsx::command(tool).arg("--version").output().is_ok()
}

/// Package owners for desktop files, asked once per package manager.
fn owners(files: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if files.is_empty() {
        return map;
    }
    let args = |first: &[&str]| first.iter().map(|s| s.to_string()).chain(files.iter().cloned()).collect::<Vec<_>>();
    if has("dpkg") {
        if let Ok(out) = fsx::command("dpkg").args(args(&["-S"])).output() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some((pkg, path)) = line.split_once(": ") {
                    map.insert(path.trim().to_string(), format!("apt:{}", pkg.split(':').next().unwrap_or(pkg)));
                }
            }
        }
    } else if has("rpm") {
        if let Ok(out) = fsx::command("rpm").args(args(&["-qf", "--qf", "%{NAME}\\n"])).output() {
            for (path, line) in files.iter().zip(String::from_utf8_lossy(&out.stdout).lines()) {
                if !line.contains("not owned") && !line.contains(' ') {
                    map.insert(path.clone(), format!("dnf:{line}"));
                }
            }
        }
    } else if has("pacman") {
        if let Ok(out) = fsx::command("pacman").args(args(&["-Qo"])).output() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some((path, rest)) = line.split_once(" is owned by ") {
                    map.insert(path.trim().to_string(), format!("pacman:{}", rest.split_whitespace().next().unwrap_or_default()));
                }
            }
        }
    }
    map
}

fn package_sizes() -> HashMap<String, u64> {
    let mut map = HashMap::new();
    if has("dpkg-query") {
        if let Ok(out) = fsx::output("dpkg-query", &["-W", "-f=${Package}\t${Installed-Size}\n"]) {
            for line in out.lines() {
                if let Some((n, kb)) = line.split_once('\t') {
                    map.insert(n.to_string(), kb.trim().parse::<u64>().unwrap_or(0) * 1024);
                }
            }
        }
    }
    map
}

pub fn list() -> Vec<App> {
    let home = fsx::home();
    let mut found: Vec<(PathBuf, Desktop)> = vec![];
    for dir in application_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "desktop") {
                if let Some(d) = fs::read_to_string(&p).ok().and_then(|t| parse_desktop(&t)).filter(|d| !d.hidden) {
                    found.push((p, d));
                }
            }
        }
    }
    let system_files: Vec<String> = found
        .iter()
        .filter(|(p, _)| p.starts_with("/usr/"))
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    let owned = owners(&system_files);
    let sizes = package_sizes();
    let mut apps = vec![];
    for (path, d) in found {
        let shown = path.to_string_lossy().into_owned();
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let program = exec_program(&d.exec);
        let (source, remove, size, ids, location): (&str, Remove, u64, Vec<String>, Option<String>) = if shown.contains("/flatpak/exports/") {
            let bin = home.join(".var/app").join(&stem);
            ("flatpak", Remove { kind: "flatpak".into(), target: stem.clone() }, 0, vec![stem.clone()], Some(bin.to_string_lossy().into_owned()).filter(|b| Path::new(b).is_dir()))
        } else if shown.starts_with("/var/lib/snapd/") {
            let snap = stem.split('_').next().unwrap_or(&stem).to_string();
            ("snap", Remove { kind: "snap".into(), target: snap.clone() }, 0, vec![snap, stem.clone()], None)
        } else if path.starts_with(home.join(".local/share/applications")) {
            if program.to_lowercase().ends_with(".appimage") && Path::new(&program).exists() {
                let size = fs::metadata(&program).map(|m| m.len()).unwrap_or(0);
                ("appimage", Remove { kind: "appimage".into(), target: format!("{program}\n{shown}") }, size, vec![stem.clone()], Some(program.clone()))
            } else {
                continue;
            }
        } else if let Some(owner) = owned.get(&shown) {
            let (kind, pkg) = owner.split_once(':').unwrap_or(("", owner));
            let size = sizes.get(pkg).copied().unwrap_or(0);
            let bin = Path::new(&program).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (kind_label(kind), Remove { kind: kind.into(), target: pkg.into() }, size, vec![pkg.to_string(), stem.clone(), bin], None)
        } else {
            continue;
        };
        apps.push(
            App { id: format!("{source}:{stem}"), name: d.name, source: source.into(), size, location, ids, remove, ..Default::default() }.seal(),
        );
    }
    apps
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "apt" => "apt",
        "dnf" => "dnf",
        "pacman" => "pacman",
        _ => "package",
    }
}

pub fn remove(app: &App) -> Result<String, String> {
    let t = &app.remove.target;
    match app.remove.kind.as_str() {
        "apt" => fsx::output("pkexec", &["apt-get", "remove", "-y", t]).map(|_| "Removed with apt".into()),
        "dnf" => fsx::output("pkexec", &["dnf", "remove", "-y", t]).map(|_| "Removed with dnf".into()),
        "pacman" => fsx::output("pkexec", &["pacman", "-Rns", "--noconfirm", t]).map(|_| "Removed with pacman".into()),
        "flatpak" => fsx::output("flatpak", &["uninstall", "-y", "--noninteractive", t]).map(|_| "Removed with Flatpak".into()),
        "snap" => fsx::output("pkexec", &["snap", "remove", t]).map(|_| "Removed with Snap".into()),
        "appimage" => {
            for file in t.split('\n') {
                fsx::trash(Path::new(file))?;
            }
            Ok("The AppImage and its menu entry were moved to the trash".into())
        }
        other => Err(format!("Removing {other} apps is not supported")),
    }
}

fn roots(deep: bool) -> Vec<PathBuf> {
    let home = fsx::home();
    let mut r = vec![
        home.join(".config"),
        home.join(".cache"),
        home.join(".local/share"),
        home.join(".local/state"),
        home.join(".var/app"),
        home.join(".config/autostart"),
    ];
    if deep {
        r.extend([PathBuf::from("/opt"), PathBuf::from("/var/lib")]);
    }
    r
}

pub fn scan(id: &Identity, others: &Others, deep: bool, cancel: &AtomicBool) -> Vec<Leftover> {
    leftovers::scan_roots(id, others, &roots(deep), cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_desktop_entry_group_only() {
        let text = "[Desktop Entry]\nType=Application\nName=Firefox\nExec=/usr/lib/firefox/firefox %u\n[Desktop Action new]\nName=New window\nNoDisplay=true\n";
        let d = parse_desktop(text).unwrap();
        assert_eq!(d.name, "Firefox");
        assert!(!d.hidden);
        assert_eq!(exec_program(&d.exec), "/usr/lib/firefox/firefox");
    }

    #[test]
    fn hidden_and_non_app_entries() {
        assert!(parse_desktop("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n").unwrap().hidden);
        assert!(parse_desktop("[Desktop Entry]\nType=Link\nName=X\n").is_none());
    }

    #[test]
    fn quoted_exec_paths() {
        assert_eq!(exec_program("\"/home/a b/App.AppImage\" --no-sandbox %U"), "/home/a b/App.AppImage");
    }
}
