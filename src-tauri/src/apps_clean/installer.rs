//! Installer helper (Windows, spec/clipboard/installer-helper.md) and AppImage installer
//! (Linux, spec/clipboard/appimage-installer.md): watch Downloads and offer to install.

use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::Serialize;

use super::fsx;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub path: String,
    pub name: String,
    /// msi, exe, zip or appimage.
    pub kind: &'static str,
    pub size: u64,
}

pub fn kind_of(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    if n.ends_with(".msi") {
        Some("msi")
    } else if n.ends_with(".exe") {
        Some("exe")
    } else if n.ends_with(".zip") {
        Some("zip")
    } else if n.ends_with(".appimage") {
        Some("appimage")
    } else {
        None
    }
}

pub fn applications_dir() -> PathBuf {
    fsx::home().join("Applications")
}

/// Installable files sitting in `downloads`, settled for a few seconds, on this platform.
pub fn candidates(downloads: &Path, windows: bool, applications: &Path) -> Vec<Offer> {
    let Ok(read) = fs::read_dir(downloads) else { return vec![] };
    let mut out = vec![];
    for e in read.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(kind) = kind_of(&name) else { continue };
        if windows != (kind != "appimage") {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let settled = meta.modified().ok().is_some_and(|m| SystemTime::now().duration_since(m).unwrap_or_default() > Duration::from_secs(4));
        if !meta.is_file() || !settled || e.path().starts_with(applications) {
            continue;
        }
        out.push(Offer { path: e.path().to_string_lossy().into_owned(), name, kind, size: meta.len() });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The silent switches of installer frameworks we recognise from their own markers.
pub fn silent_switch(head: &[u8]) -> Option<&'static str> {
    let has = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
    if has(b"Inno Setup") {
        Some("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-")
    } else if has(b"Nullsoft Install System") || has(b"NSIS Error") {
        Some("/S")
    } else if has(b".wixburn") {
        Some("/quiet /norestart")
    } else {
        None
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspect {
    pub kind: &'static str,
    /// Switches used for a silent install, when this installer is known to support it.
    pub silent: Option<&'static str>,
    /// Valid, NotSigned, HashMismatch and so on; empty when it could not be checked.
    pub signature: String,
}

pub fn inspect(path: &Path) -> Result<Inspect, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let kind = kind_of(&name).ok_or("This is not an installer Notifi knows")?;
    let mut head = vec![];
    if kind == "exe" {
        fs::File::open(path).and_then(|f| f.take(8 << 20).read_to_end(&mut head)).map_err(|e| e.to_string())?;
    }
    let signature = if cfg!(windows) && matches!(kind, "exe" | "msi") {
        let script = format!("(Get-AuthenticodeSignature -LiteralPath '{}').Status", path.to_string_lossy().replace('\'', "''"));
        fsx::output("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", &script]).map(|s| s.trim().to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    Ok(Inspect { kind, silent: if kind == "msi" { Some("/qn /norestart") } else { silent_switch(&head) }, signature })
}

/// Exit codes in plain words.
pub fn exit_words(code: i32) -> String {
    match code {
        0 => "Installed".into(),
        3010 | 1641 => "Installed. A restart is needed to finish".into(),
        1602 | 1223 => "The installation was cancelled".into(),
        1618 => "Another installation is already running".into(),
        1619 | 1620 => "The installer file is damaged or cannot be opened".into(),
        1625 => "This installation is blocked by a system policy".into(),
        1638 => "A newer version is already installed".into(),
        1925 | 5 => "Administrator rights are needed".into(),
        1603 => "The installer hit a fatal error".into(),
        c => format!("The installer stopped with exit code {c}"),
    }
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub ok: bool,
    pub code: Option<i32>,
    pub message: String,
    /// The program to launch or reveal, when known.
    pub program: Option<String>,
    /// For a portable archive with several programs: let the user choose.
    pub choices: Vec<String>,
}

fn run_wait(program: &str, args: &str, elevate: bool) -> Result<i32, String> {
    let q = |s: &str| s.replace('\'', "''");
    let verb = if elevate { " -Verb RunAs" } else { "" };
    let arg_part = if args.is_empty() { String::new() } else { format!(" -ArgumentList '{}'", q(args)) };
    let script = format!("$p = Start-Process -FilePath '{}'{arg_part} -Wait -PassThru{verb}; exit $p.ExitCode", q(program));
    let status = fsx::command("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &script]).status().map_err(|e| e.to_string())?;
    Ok(status.code().unwrap_or(-1))
}

/// Runs an .msi or .exe installer and waits for it.
pub fn install_windows(path: &Path, silent_allowed: bool) -> Installed {
    let info = match inspect(path) {
        Ok(i) => i,
        Err(e) => return Installed { message: e, ..Default::default() },
    };
    let file = path.to_string_lossy().into_owned();
    let result = match (info.kind, silent_allowed.then_some(info.silent).flatten()) {
        ("msi", Some(switches)) => {
            let args = format!("/i \"{file}\" {switches}");
            run_wait("msiexec.exe", &args, false).and_then(|c| if c == 1925 { run_wait("msiexec.exe", &args, true) } else { Ok(c) })
        }
        ("msi", None) => run_wait("msiexec.exe", &format!("/i \"{file}\""), false),
        ("exe", Some(switches)) => run_wait(&file, switches, false),
        _ => run_wait(&file, "", false),
    };
    match result {
        Ok(code) => Installed { ok: matches!(code, 0 | 3010 | 1641), code: Some(code), message: exit_words(code), ..Default::default() },
        Err(e) => Installed { message: e, ..Default::default() },
    }
}

fn find_exes(root: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(root) else { return };
    for e in read.flatten() {
        let p = e.path();
        if fsx::is_link(&p) {
            continue;
        }
        if p.is_dir() && depth > 0 {
            find_exes(&p, depth - 1, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
            let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !n.contains("unins") && !n.contains("uninstall") && !n.contains("crashpad") && !n.contains("update") {
                out.push(p);
            }
        }
    }
}

pub fn extract_portable(zip: &Path, apps_dir: &Path) -> Installed {
    let stem = zip.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "app".into());
    let dest = apps_dir.join(&stem);
    if dest.exists() {
        return Installed { message: format!("{} already exists", dest.display()), ..Default::default() };
    }
    if let Err(e) = fs::create_dir_all(&dest) {
        return Installed { message: e.to_string(), ..Default::default() };
    }
    if let Err(e) = fsx::output("tar", &["-xf", &zip.to_string_lossy(), "-C", &dest.to_string_lossy()]) {
        let _ = fs::remove_dir_all(&dest);
        return Installed { message: format!("Could not unpack the archive. {e}"), ..Default::default() };
    }
    let mut exes = vec![];
    find_exes(&dest, 3, &mut exes);
    exes.sort_by_key(|p| p.components().count());
    let choices: Vec<String> = exes.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    match choices.len() {
        0 => Installed { ok: true, message: format!("Unpacked to {}. No program was found inside", dest.display()), program: Some(dest.to_string_lossy().into_owned()), ..Default::default() },
        1 => Installed { ok: true, message: format!("Unpacked to {}", dest.display()), program: choices.first().cloned(), ..Default::default() },
        _ => Installed { ok: true, message: format!("Unpacked to {}. Choose the program to add", dest.display()), program: Some(dest.to_string_lossy().into_owned()), choices, ..Default::default() },
    }
}

/// A Start menu shortcut, and optionally a desktop one.
pub fn make_shortcuts(exe: &Path, desktop: bool) -> Result<(), String> {
    let name = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let appdata = fsx::env_path("APPDATA").ok_or("no APPDATA")?;
    let mut places = vec![appdata.join("Microsoft\\Windows\\Start Menu\\Programs")];
    if desktop {
        places.push(fsx::home().join("Desktop"));
    }
    let q = |s: &str| s.replace('\'', "''");
    for dir in places {
        let lnk = dir.join(format!("{name}.lnk"));
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath='{}'; $s.WorkingDirectory='{}'; $s.Save()",
            q(&lnk.to_string_lossy()),
            q(&exe.to_string_lossy()),
            q(&exe.parent().unwrap_or(exe).to_string_lossy())
        );
        fsx::output("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", &script])?;
    }
    Ok(())
}

/// The menu name for an AppImage file: version and architecture dropped, words capitalised.
pub fn app_name(file: &str) -> String {
    let stem = file.strip_suffix(".AppImage").or_else(|| file.strip_suffix(".appimage")).unwrap_or(file);
    let is_noise = |p: &str| {
        let l = p.to_lowercase();
        matches!(l.as_str(), "x86" | "x86_64" | "x64" | "amd64" | "arm64" | "aarch64" | "linux" | "appimage" | "i386")
            || l.strip_prefix('v').unwrap_or(&l).chars().next().is_some_and(|c| c.is_ascii_digit())
    };
    let words: Vec<String> = stem
        .split(['-', '_', ' '])
        .filter(|p| !p.is_empty())
        .take_while(|p| !is_noise(p))
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect();
    if words.is_empty() { stem.to_string() } else { words.join(" ") }
}

fn slug(name: &str) -> String {
    name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>().trim_matches('-').to_string()
}

/// Text of the menu entry that launches an installed AppImage.
pub fn desktop_entry(name: &str, exec: &Path, icon: &str) -> String {
    let exec = exec.to_string_lossy();
    let quoted = if exec.contains(' ') { format!("\"{exec}\"") } else { exec.into_owned() };
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec={quoted} %U\nIcon={icon}\nTerminal=false\nCategories=Utility;\nComment=Installed by Notifi\nX-AppImage-Integrate=false\n")
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppImageInstalled {
    pub name: String,
    pub path: String,
    pub desktop: String,
}

/// Moves an AppImage into ~/Applications, makes it executable and adds a menu entry.
pub fn install_appimage(path: &Path, applications: &Path, menu_dir: &Path, icon_dir: &Path) -> Result<AppImageInstalled, String> {
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("no file name")?;
    if path.starts_with(applications) {
        return Err("This AppImage is already in the applications folder".into());
    }
    fs::create_dir_all(applications).map_err(|e| e.to_string())?;
    let mut dest = applications.join(&file);
    let mut n = 1;
    while dest.exists() {
        n += 1;
        dest = applications.join(format!("{} ({n}).AppImage", file.trim_end_matches(".AppImage")));
    }
    fs::rename(path, &dest).or_else(|_| fs::copy(path, &dest).and_then(|_| fs::remove_file(path))).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    let name = app_name(&file);
    let id = format!("appimage-{}", slug(&name));
    let icon = extract_icon(&dest, icon_dir, &id).unwrap_or_else(|| "application-x-executable".into());
    fs::create_dir_all(menu_dir).map_err(|e| e.to_string())?;
    let desktop = menu_dir.join(format!("{id}.desktop"));
    fs::write(&desktop, desktop_entry(&name, &dest, &icon)).map_err(|e| e.to_string())?;
    let _ = fsx::command("update-desktop-database").arg(menu_dir).status();
    Ok(AppImageInstalled { name, path: dest.to_string_lossy().into_owned(), desktop: desktop.to_string_lossy().into_owned() })
}

/// Asks the AppImage's own runtime for its icon (the app itself is not started).
fn extract_icon(appimage: &Path, icon_dir: &Path, id: &str) -> Option<String> {
    let work = std::env::temp_dir().join(format!("notifi-appimage-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).ok()?;
    let ok = fsx::command(&appimage.to_string_lossy()).current_dir(&work).args(["--appimage-extract", ".DirIcon"]).output().is_ok_and(|o| o.status.success());
    let icon = work.join("squashfs-root/.DirIcon");
    let result = (ok && fs::read(&icon).is_ok_and(|b| b.starts_with(b"\x89PNG"))).then(|| {
        let target = icon_dir.join(format!("{id}.png"));
        fs::create_dir_all(icon_dir).ok()?;
        fs::copy(&icon, &target).ok()?;
        Some(target.to_string_lossy().into_owned())
    });
    let _ = fs::remove_dir_all(&work);
    result.flatten()
}

/// Keeps what was already offered so nothing is offered twice.
#[derive(Default)]
pub struct Seen(pub HashSet<String>);

impl Seen {
    pub fn fresh(&mut self, offers: Vec<Offer>) -> Vec<Offer> {
        offers.into_iter().filter(|o| self.0.insert(o.path.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notifi-inst-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn settled(path: &Path) {
        fs::File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - Duration::from_secs(60)).unwrap();
    }

    #[test]
    fn menu_entry_launches_the_installed_file() {
        let text = desktop_entry("Cool App", Path::new("/home/me/Applications/Cool App.AppImage"), "cool");
        assert!(text.starts_with("[Desktop Entry]\nType=Application\nName=Cool App\n"));
        assert!(text.contains("Exec=\"/home/me/Applications/Cool App.AppImage\" %U\n"));
        assert!(text.contains("Icon=cool\n") && text.contains("Terminal=false"));
        assert!(desktop_entry("X", Path::new("/a/x"), "i").contains("Exec=/a/x %U"));
    }

    #[test]
    fn names_drop_versions_and_architecture() {
        assert_eq!(app_name("obsidian-1.5.3.AppImage"), "Obsidian");
        assert_eq!(app_name("Cool_App-v2.0-x86_64.AppImage"), "Cool App");
        assert_eq!(app_name("kdenlive-24.02.1-x86_64.appimage"), "Kdenlive");
        assert_eq!(app_name("2048.AppImage"), "2048");
    }

    #[test]
    fn installing_moves_marks_runnable_and_writes_the_entry() {
        let root = temp("appimage");
        let downloads = root.join("Downloads");
        let apps = root.join("Applications");
        fs::create_dir_all(&downloads).unwrap();
        let src = downloads.join("tool-1.0-x86_64.AppImage");
        fs::write(&src, b"not really an appimage").unwrap();
        let done = install_appimage(&src, &apps, &root.join("menu"), &root.join("icons")).unwrap();
        assert!(!src.exists());
        assert!(Path::new(&done.path).starts_with(&apps));
        let entry = fs::read_to_string(&done.desktop).unwrap();
        assert!(entry.contains("Name=Tool") && entry.contains(&done.path));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&done.path).unwrap().permissions().mode() & 0o111, 0o111);
        }
        assert!(install_appimage(Path::new(&done.path), &apps, &root.join("menu"), &root.join("icons")).is_err(), "already installed");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn offers_are_platform_specific_and_skip_installed_and_fresh_files() {
        let root = temp("offers");
        for n in ["a.msi", "b.EXE", "c.zip", "d.AppImage", "e.txt", "fresh.msi"] {
            fs::write(root.join(n), "x").unwrap();
            if n != "fresh.msi" {
                settled(&root.join(n));
            }
        }
        let win: Vec<String> = candidates(&root, true, &root.join("Applications")).into_iter().map(|o| o.name).collect();
        assert_eq!(win, ["a.msi", "b.EXE", "c.zip"]);
        let lin: Vec<String> = candidates(&root, false, &root.join("Applications")).into_iter().map(|o| o.name).collect();
        assert_eq!(lin, ["d.AppImage"]);
        assert!(candidates(&root, false, &root).is_empty(), "files already inside the applications folder are ignored");
        let mut seen = Seen::default();
        assert_eq!(seen.fresh(candidates(&root, true, &root.join("Applications"))).len(), 3);
        assert!(seen.fresh(candidates(&root, true, &root.join("Applications"))).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn silent_switches_come_from_known_frameworks_only() {
        assert_eq!(silent_switch(b"....Inno Setup Setup Data...."), Some("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-"));
        assert_eq!(silent_switch(b"Nullsoft Install System v3"), Some("/S"));
        assert_eq!(silent_switch(b"xx.wixburnxx"), Some("/quiet /norestart"));
        assert_eq!(silent_switch(b"some unknown installer"), None);
    }

    #[test]
    fn exit_codes_in_plain_words() {
        assert_eq!(exit_words(0), "Installed");
        assert!(exit_words(1602).contains("cancelled"));
        assert!(exit_words(1603).contains("fatal"));
        assert!(exit_words(77).contains("77"));
    }

    #[test]
    fn archives_with_several_programs_ask_which() {
        let root = temp("zip");
        fs::create_dir_all(root.join("app/bin")).unwrap();
        for f in ["app/main.exe", "app/bin/helper.exe", "app/unins000.exe", "app/readme.txt"] {
            fs::write(root.join(f), "x").unwrap();
        }
        let mut found = vec![];
        find_exes(&root, 3, &mut found);
        let mut names: Vec<String> = found.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["helper.exe", "main.exe"]);
        let _ = fs::remove_dir_all(&root);
    }
}
