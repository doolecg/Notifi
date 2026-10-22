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
/// The Ed25519 public key (hex) whose private half signs every installer (`scripts/sign-update.mjs`).
/// An installer is only used when its `.sig` beside it on the release checks out against this key,
/// so a changed or swapped file is refused even if it came from GitHub.
const UPDATE_KEY: &str = "c2f83aa02f15a99b44433938ddcf3617ac59727d09bcb2fe1baf6a11ace6a799";
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
    /// The release notes, as Markdown.
    #[serde(default)]
    body: Option<String>,
}

/// The newest release's notes, for the settings page.
#[derive(Serialize, Clone)]
pub struct Notes {
    version: String,
    markdown: String,
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
    notes: Mutex<Option<Notes>>,
    busy: AtomicBool,
    installing: AtomicBool,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            status: Mutex::new(Status::Idle),
            ready: Mutex::new(None),
            notes: Mutex::new(None),
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

fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

/// Whether `sig` (hex) is the update key's signature of `data`.
fn signed(data: &[u8], sig: &str) -> bool {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let (Some(key), Some(sig)) = (unhex(UPDATE_KEY), unhex(sig)) else { return false };
    UnparsedPublicKey::new(&ED25519, key).verify(data, &sig).is_ok()
}

/// Only files on this project's own GitHub releases are fetched.
fn from_release(url: &str) -> bool {
    url.to_ascii_lowercase().starts_with(&format!("https://github.com/{REPO}/releases/download/"))
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

/// Downloads the installer and keeps it only when its digest matches and its signature checks out.
async fn download(app: &AppHandle, client: &reqwest::Client, asset: &Asset, sig: &Asset, version: &str) -> Result<PathBuf, String> {
    let Some(expected) = expected_sha256(asset.digest.as_deref()) else {
        return Err("The release has no SHA-256 digest, so it was not downloaded".into());
    };
    if !from_release(&asset.browser_download_url) || !from_release(&sig.browser_download_url) {
        return Err("The update is not on Notifi's own release page, so it was not downloaded".into());
    }
    let signature = fetch_signature(client, sig).await?;
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("updates");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = safe_name(&asset.name);
    let final_path = dir.join(&name);
    let part_path = dir.join(format!("{name}.part"));

    if hash_file(&final_path).is_ok_and(|h| h == expected) {
        return check_signature(final_path, &signature);
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
    check_signature(final_path, &signature)
}

async fn fetch_signature(client: &reqwest::Client, sig: &Asset) -> Result<String, String> {
    // A hex Ed25519 signature is 128 characters; anything much bigger is not one.
    if sig.size > 1024 {
        return Err("The update's signature file is the wrong size, so it was not downloaded".into());
    }
    let response = client.get(&sig.browser_download_url).timeout(Duration::from_secs(30)).send().await.map_err(|e| format!("Download failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Download failed: GitHub answered {}", response.status().as_u16()));
    }
    response.text().await.map_err(|e| format!("Download failed: {e}"))
}

/// Keeps the file only when it is signed with the update key; otherwise it is deleted.
fn check_signature(path: PathBuf, signature: &str) -> Result<PathBuf, String> {
    let data = fs::read(&path).map_err(|e| e.to_string())?;
    if signed(&data, signature) {
        // Kept beside the installer so it can be checked again just before it runs.
        fs::write(signature_file(&path), signature).map_err(|e| e.to_string())?;
        Ok(path)
    } else {
        let _ = fs::remove_file(&path);
        Err("The update's signature did not check out, so it was discarded".into())
    }
}

fn signature_file(installer: &Path) -> PathBuf {
    let mut name = installer.file_name().unwrap_or_default().to_os_string();
    name.push(".sig");
    installer.with_file_name(name)
}

/// The installer sits in a folder anything running as the user can write to, hours after it was checked, so it is checked again before it runs.
fn verify_again(path: &Path) -> Result<(), String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let signature = fs::read_to_string(signature_file(path)).map_err(|e| e.to_string())?;
    if signed(&data, signature.trim()) {
        Ok(())
    } else {
        let _ = fs::remove_file(path);
        Err("The update's signature no longer checked out, so it was discarded".into())
    }
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
    *app.state::<Updater>().notes.lock().unwrap() = Some(Notes { version: version.clone(), markdown: release.body.clone().unwrap_or_default() });
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
    let sig_name = format!("{}.sig", asset.name);
    let Some(sig) = release.assets.iter().find(|a| a.name == sig_name) else {
        return Status::Error { message: format!("Version {version} is not signed, so it was not downloaded") };
    };
    set_status(app, Status::Downloading { version: version.clone(), percent: 0 });
    match download(app, &client, asset, sig, &version).await {
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

/// Runs the installer once Notifi has quit; with `restart`, starts the updated Notifi when it is done.
#[cfg(windows)]
fn launch_installer(path: &Path, restart: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Both paths come from Windows (the cache folder and this program), never from the release, and are quoted for cmd.
    let quoted = |p: &Path| format!("\"{}\"", p.display());
    let mut line = format!("/c start \"\" /wait msiexec /i {} /passive /norestart", quoted(path));
    if restart {
        line += &format!(" & start \"\" {}", quoted(&exe));
    }
    std::process::Command::new("cmd")
        .raw_arg(line)
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start the installer: {e}"))
}

#[cfg(not(windows))]
fn launch_installer(path: &Path, _restart: bool) -> Result<(), String> {
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

fn install(app: &AppHandle, restart: bool) -> Result<(), String> {
    let updater = app.state::<Updater>();
    let Some(path) = updater.ready.lock().unwrap().clone() else {
        return Err("No update is ready".into());
    };
    if updater.installing.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    verify_again(&path).and_then(|()| launch_installer(&path, restart)).inspect_err(|_| updater.installing.store(false, Ordering::SeqCst))
}

/// Runs on app exit: installs a ready update when the user chose install-on-quit (Notifi stays closed after).
pub fn on_exit(app: &AppHandle) {
    let on_quit = app.state::<settings::Store>().get().general.install_on_quit;
    let ready = matches!(*app.state::<Updater>().status.lock().unwrap(), Status::Ready { .. });
    if on_quit && ready {
        let _ = install(app, false);
    }
}

/// The Install button: install, then start the new version.
pub fn install_and_exit(app: &AppHandle) {
    match install(app, true) {
        Ok(()) => {
            #[cfg(not(windows))]
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

/// The newest release's notes; fetched now when no check has run yet.
#[tauri::command]
pub async fn update_notes(app: AppHandle) -> Result<Notes, String> {
    if let Some(notes) = app.state::<Updater>().notes.lock().unwrap().clone() {
        return Ok(notes);
    }
    let release = fetch_latest(&client()?).await?;
    let notes = Notes { version: release.tag_name.trim().trim_start_matches(['v', 'V']).to_string(), markdown: release.body.unwrap_or_default() };
    *app.state::<Updater>().notes.lock().unwrap() = Some(notes.clone());
    Ok(notes)
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
    fn only_signed_files_from_our_releases_pass() {
        // Signed with a throwaway Ed25519 key (RFC 8032 test 1); the real key would reject it.
        const TEST_KEY: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
        const TEST_SIG: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";
        use ring::signature::{UnparsedPublicKey, ED25519};
        assert!(UnparsedPublicKey::new(&ED25519, unhex(TEST_KEY).unwrap()).verify(b"", &unhex(TEST_SIG).unwrap()).is_ok());
        assert!(!signed(b"", TEST_SIG));
        // Made by scripts/sign-update.mjs with the real key: the script and UPDATE_KEY belong together.
        let real = "ae66cdf2f78e36658ab0f5c2d386e7aa496048630d44b9e758a9e43c044137304ac2ad499339c9a1e1680c44fa19bf84b6acc3271e69458d434817e6284fab09";
        assert!(signed(b"notifi", real));
        assert!(!signed(b"notifj", real));
        assert!(!signed(b"", "not hex"));
        assert_eq!(unhex("0aFf"), Some(vec![0x0a, 0xff]));
        assert_eq!(unhex("abc"), None);
        assert!(from_release("https://github.com/doolecg/Notifi/releases/download/0.3.0/Notifi_0.3.0_x64_en-US.msi"));
        assert!(!from_release("https://github.com/someone/notifi/releases/download/0.3.0/x.msi"));
        assert!(!from_release("http://github.com/doolecg/notifi/releases/download/0.3.0/x.msi"));
        assert!(!from_release("https://github.com.evil.example/doolecg/notifi/releases/download/x.msi"));
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
