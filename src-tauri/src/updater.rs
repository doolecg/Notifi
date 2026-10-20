use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_http::reqwest;

use crate::{settings, tray};

const REPO: &str = "doolecg/notifi";
const STATUS_EVENT: &str = "updater://status";
const STARTUP_DELAY: Duration = Duration::from_secs(8);

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Downloading { version: String, percent: u8 },
    Ready { version: String },
    Manual { version: String, url: String },
    Error { message: String },
}

#[derive(Deserialize, Clone, Debug)]
struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

#[derive(Deserialize, Debug)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Msi,
    AppImage,
    /// deb/rpm installs and other platforms: only point at the release page.
    Manual,
}

fn platform_kind() -> Kind {
    if cfg!(windows) {
        Kind::Msi
    } else if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some() {
        Kind::AppImage
    } else {
        Kind::Manual
    }
}

pub struct Updater {
    status: Mutex<Status>,
    ready: Mutex<Option<PathBuf>>,
    busy: AtomicBool,
    installing: AtomicBool,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            status: Mutex::new(Status::Idle),
            ready: Mutex::new(None),
            busy: AtomicBool::new(false),
            installing: AtomicBool::new(false),
        }
    }
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let mut parts = text.split('.').map(|p| {
        let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().ok()
    });
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn pick_asset(assets: &[Asset], kind: Kind) -> Option<&Asset> {
    assets.iter().find(|a| {
        let name = a.name.to_ascii_lowercase();
        match kind {
            Kind::Msi => name.ends_with(".msi") && name.contains("x64"),
            Kind::AppImage => name.ends_with(".appimage") && name.contains("amd64"),
            Kind::Manual => false,
        }
    })
}

fn expected_sha256(digest: Option<&str>) -> Option<String> {
    let hex = digest?.strip_prefix("sha256:")?.to_ascii_lowercase();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

fn safe_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect()
}

fn set_status(app: &AppHandle, status: Status) {
    let updater = app.state::<Updater>();
    *updater.status.lock().unwrap() = status.clone();
    let _ = app.emit(STATUS_EVENT, &status);
    let ready = match &status {
        Status::Ready { version } => Some(version.as_str()),
        _ => None,
    };
    tray::set_update_ready(app, ready);
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("Notifi/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())
}

async fn fetch_latest(client: &reqwest::Client) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Could not reach GitHub: {e}"))?;
    match response.status().as_u16() {
        200 => {
            let text = response.text().await.map_err(|e| format!("Could not read GitHub's reply: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("Unexpected reply from GitHub: {e}"))
        }
        404 => Err("No releases published yet".into()),
        code => Err(format!("GitHub answered {code}")),
    }
}

async fn download(app: &AppHandle, client: &reqwest::Client, asset: &Asset, version: &str) -> Result<PathBuf, String> {
    let Some(expected) = expected_sha256(asset.digest.as_deref()) else {
        return Err("The release has no SHA-256 digest, so it was not downloaded".into());
    };
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("updates");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = safe_name(&asset.name);
    let final_path = dir.join(&name);
    let part_path = dir.join(format!("{name}.part"));

    if hash_file(&final_path).is_ok_and(|h| h == expected) {
        return Ok(final_path);
    }
    let _ = fs::remove_file(&final_path);

    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Download failed: GitHub answered {}", response.status().as_u16()));
    }
    let mut file = fs::File::create(&part_path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut last_percent = None;
    let result: Result<(), String> = async {
        while let Some(chunk) = response.chunk().await.map_err(|e| format!("Download failed: {e}"))? {
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            hasher.update(&chunk);
            received += chunk.len() as u64;
            let percent = if asset.size > 0 { (received * 100 / asset.size).min(100) as u8 } else { 0 };
            if last_percent != Some(percent) {
                last_percent = Some(percent);
                set_status(app, Status::Downloading { version: version.to_string(), percent });
            }
        }
        Ok(())
    }
    .await;
    drop(file);
    if let Err(e) = result {
        let _ = fs::remove_file(&part_path);
        return Err(e);
    }
    if received != asset.size {
        let _ = fs::remove_file(&part_path);
        return Err(format!("Download was incomplete ({received} of {} bytes)", asset.size));
    }
    if hex(&hasher.finalize()) != expected {
        let _ = fs::remove_file(&part_path);
        return Err("The download did not match its SHA-256 digest and was discarded".into());
    }
    fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
    Ok(final_path)
}

async fn run_check(app: &AppHandle) -> Status {
    let current = app.package_info().version.to_string();
    let client = match client() {
        Ok(c) => c,
        Err(message) => return Status::Error { message },
    };
    let release = match fetch_latest(&client).await {
        Ok(r) => r,
        Err(message) => return Status::Error { message },
    };
    let version = release.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    if !is_newer(&version, &current) {
        return Status::UpToDate;
    }
    let kind = platform_kind();
    let Some(asset) = pick_asset(&release.assets, kind) else {
        return if kind == Kind::Manual {
            Status::Manual { version, url: release.html_url }
        } else {
            Status::Error { message: format!("Version {version} has no installer for this system") }
        };
    };
    set_status(app, Status::Downloading { version: version.clone(), percent: 0 });
    match download(app, &client, asset, &version).await {
        Ok(path) => {
            *app.state::<Updater>().ready.lock().unwrap() = Some(path);
            Status::Ready { version }
        }
        Err(message) => Status::Error { message },
    }
}

async fn check(app: &AppHandle) {
    let updater = app.state::<Updater>();
    if matches!(*updater.status.lock().unwrap(), Status::Ready { .. }) {
        return;
    }
    if updater.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    set_status(app, Status::Checking);
    let status = run_check(app).await;
    set_status(app, status);
    updater.busy.store(false, Ordering::SeqCst);
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(STARTUP_DELAY);
        let mut last = None::<Instant>;
        loop {
            let general = app.state::<settings::Store>().get().general;
            let due = match last {
                None => true,
                Some(at) => {
                    general.update_check_hours > 0
                        && at.elapsed() >= Duration::from_secs(u64::from(general.update_check_hours) * 3600)
                }
            };
            if due && general.auto_update {
                tauri::async_runtime::block_on(check(&app));
                last = Some(Instant::now());
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}

#[cfg(windows)]
fn launch_installer(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("msiexec")
        .arg("/i")
        .arg(path)
        .arg("/passive")
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start the installer: {e}"))
}

#[cfg(not(windows))]
fn launch_installer(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let target = PathBuf::from(std::env::var_os("APPIMAGE").ok_or("Not running as an AppImage")?);
    let mut staged = target.clone().into_os_string();
    staged.push(".new");
    let staged = PathBuf::from(staged);
    fs::copy(path, &staged).map_err(|e| format!("Could not write the new AppImage: {e}"))?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    fs::rename(&staged, &target).map_err(|e| format!("Could not replace the AppImage: {e}"))
}

#[cfg(not(windows))]
fn relaunch() {
    if let Some(target) = std::env::var_os("APPIMAGE") {
        let _ = std::process::Command::new(target).spawn();
    }
}

#[cfg(windows)]
fn relaunch() {}

fn install(app: &AppHandle) -> Result<(), String> {
    let updater = app.state::<Updater>();
    let Some(path) = updater.ready.lock().unwrap().clone() else {
        return Err("No update is ready".into());
    };
    if updater.installing.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    launch_installer(&path).inspect_err(|_| updater.installing.store(false, Ordering::SeqCst))
}

/// Runs on app exit: installs a ready update when the user chose install-on-quit.
pub fn on_exit(app: &AppHandle) {
    let on_quit = app.state::<settings::Store>().get().general.install_on_quit;
    let ready = matches!(*app.state::<Updater>().status.lock().unwrap(), Status::Ready { .. });
    if on_quit && ready {
        let _ = install(app);
    }
}

pub fn install_and_exit(app: &AppHandle) {
    match install(app) {
        Ok(()) => {
            relaunch();
            app.exit(0);
        }
        Err(message) => set_status(app, Status::Error { message }),
    }
}

#[tauri::command]
pub fn get_update_status(updater: State<Updater>) -> Status {
    updater.status.lock().unwrap().clone()
}

#[tauri::command]
pub fn check_update_now(app: AppHandle) {
    std::thread::spawn(move || tauri::async_runtime::block_on(check(&app)));
}

#[tauri::command]
pub fn install_update(app: AppHandle) {
    install_and_exit(&app);
}

#[tauri::command]
pub fn open_release_page(updater: State<Updater>) -> Result<(), String> {
    let Status::Manual { url, .. } = updater.status.lock().unwrap().clone() else {
        return Err("No release page to open".into());
    };
    open_url(&url)
}

#[tauri::command]
pub fn open_release_notes() -> Result<(), String> {
    open_url(&format!("https://github.com/{REPO}/releases/latest"))
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/c", "start", ""]).arg(url);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset { name: name.into(), size: 1, digest: None, browser_download_url: String::new() }
    }

    #[test]
    fn version_compare() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert_eq!(parse_version("v1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(parse_version("2"), Some((2, 0, 0)));
    }

    #[test]
    fn asset_pick() {
        let assets = [
            asset("Notifi_0.2.0_x64.sha"),
            asset("Notifi_0.2.0_amd64.deb"),
            asset("Notifi_0.2.0_amd64.AppImage"),
            asset("Notifi_0.2.0_x64_en-US.msi"),
        ];
        assert_eq!(pick_asset(&assets, Kind::Msi).unwrap().name, "Notifi_0.2.0_x64_en-US.msi");
        assert_eq!(pick_asset(&assets, Kind::AppImage).unwrap().name, "Notifi_0.2.0_amd64.AppImage");
        assert!(pick_asset(&assets, Kind::Manual).is_none());
        assert!(pick_asset(&assets[..2], Kind::Msi).is_none());
    }

    #[test]
    fn digest_check() {
        let good = "sha256:".to_string() + &"AB".repeat(32);
        assert_eq!(expected_sha256(Some(&good)), Some("ab".repeat(32)));
        assert_eq!(expected_sha256(None), None);
        assert_eq!(expected_sha256(Some("sha1:abc")), None);
        assert_eq!(expected_sha256(Some("sha256:abc")), None);
        let empty = hex(&Sha256::digest(b""));
        assert_eq!(empty, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn names_are_sanitised() {
        assert_eq!(safe_name("../evil name.msi"), ".._evil_name.msi");
    }
}
