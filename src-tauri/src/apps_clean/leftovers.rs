//! Finding what a removed app left behind, conservatively (spec/apps/uninstaller.md).
//! Only paths tied to the app's own identifiers are "verified"; a similar name alone is not.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

use super::fsx;

/// What identifies the app being removed.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    pub name: String,
    pub publisher: String,
    /// Registry key name, package name, desktop id, winget-style `Vendor.Product`.
    pub ids: Vec<String>,
    pub location: Option<PathBuf>,
}

/// Apps that stay installed, whose data must never be offered.
#[derive(Clone, Debug, Default)]
pub struct Others {
    pub names: Vec<String>,
    pub locations: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Verified,
    Name,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    File,
    Registry,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub path: String,
    pub kind: Kind,
    pub confidence: Confidence,
    pub size: u64,
    pub why: String,
}

/// Lower-case letters and digits only, so "Foo-Bar_2" and "foo bar 2" compare equal.
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

const COMPANY_WORDS: &[&str] = &["inc", "ltd", "llc", "corp", "corporation", "gmbh", "co", "limited", "foundation", "sa", "ab"];

fn norm_publisher(s: &str) -> String {
    let words: Vec<&str> = s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let kept: Vec<&str> = words.iter().copied().filter(|w| !COMPANY_WORDS.contains(&w.to_lowercase().as_str())).collect();
    norm(&if kept.is_empty() { words.join("") } else { kept.join("") })
}

/// Folders that belong to the system or to many apps at once.
const SHARED: &[&str] = &[
    "microsoft", "windows", "common", "commonfiles", "packages", "temp", "tmp", "cache", "caches", "config", "data", "local", "locallow",
    "programs", "program", "apps", "app", "bin", "lib", "share", "fonts", "icons", "themes", "plugins", "extensions", "update", "updates",
    "crashdumps", "logs", "log", "downloads", "desktop", "documents", "mozilla", "google", "chrome", "intel", "nvidia", "amd", "adobe",
    "applications", "autostart", "systemd", "dconf", "pulse", "gnome", "kde", "mime", "recent", "trash", "dbus", "gtk3", "gtk4", "qt5ct",
    "cache", "state", "var", "nodejs", "python", "java", "npm", "pip", "cargo", "rustup", "gradle", "nuget", "dotnet",
];

fn looks_like_version(n: &str) -> bool {
    let rest = n.strip_prefix('v').unwrap_or(n);
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) || matches!(n, "x64" | "x86" | "arm64" | "win32" | "win64" | "amd64")
}

/// A name that cannot safely be matched to one app.
pub fn ambiguous(raw: &str) -> bool {
    let n = norm(raw);
    n.len() < 3 || SHARED.contains(&n.as_str()) || looks_like_version(&n) || raw.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn id_matches(id: &Identity, n: &str) -> bool {
    id.ids.iter().any(|i| {
        let k = norm(i);
        k.len() >= 4 && k == n && !is_guid(i)
    })
}

fn is_guid(s: &str) -> bool {
    s.starts_with('{') && s.ends_with('}') && s.len() == 38
}

/// Vendor/product pairs from two-part identifiers such as `Mozilla.Firefox`, and from publisher and name.
fn pairs(id: &Identity) -> Vec<(String, String)> {
    let mut out = vec![];
    for i in &id.ids {
        let parts: Vec<&str> = i.split('.').filter(|p| !p.is_empty()).collect();
        if let [vendor, product] = parts[..] {
            out.push((norm(vendor), norm(product)));
        }
    }
    if !id.publisher.is_empty() {
        out.push((norm_publisher(&id.publisher), norm(&id.name)));
    }
    out
}

fn name_matches(id: &Identity, n: &str) -> bool {
    let name = norm(&id.name);
    name.len() >= 4 && (n == name || (name.len() >= 5 && n.contains(&name)))
}

/// How a single folder or file name relates to the app, ignoring where it sits.
pub fn classify(id: &Identity, raw: &str) -> Option<Confidence> {
    if ambiguous(raw) {
        return None;
    }
    let n = norm(raw);
    if id_matches(id, &n) {
        Some(Confidence::Verified)
    } else if name_matches(id, &n) {
        Some(Confidence::Name)
    } else {
        None
    }
}

/// Whether `child` inside a folder named `vendor` is this app's own product folder.
pub fn classify_pair(id: &Identity, vendor: &str, child: &str) -> Option<Confidence> {
    if ambiguous(child) {
        return None;
    }
    let (v, c) = (norm_publisher(vendor), norm(child));
    pairs(id).iter().any(|(pv, pc)| *pv == v && *pc == c && pc.len() >= 3).then_some(Confidence::Verified)
}

fn still_installed(others: &Others, path: &Path, raw: &str) -> bool {
    let n = norm(raw);
    others.names.iter().any(|o| *o == n) || others.locations.iter().any(|l| path.starts_with(l) || l.starts_with(path))
}

fn entry_name(path: &Path) -> String {
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_file() {
        path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or(file)
    } else {
        file
    }
}

fn better(a: Confidence, b: Confidence) -> Confidence {
    if a == Confidence::Verified || b == Confidence::Verified {
        Confidence::Verified
    } else {
        Confidence::Name
    }
}

/// Scans `roots` (one level, plus one vendor level) for items that belong to the app.
pub fn scan_roots(id: &Identity, others: &Others, roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Leftover> {
    let mut found: Vec<Leftover> = vec![];
    fn push(found: &mut Vec<Leftover>, path: &Path, confidence: Confidence, why: &str) {
        if !found.iter().any(|f| f.path == path.to_string_lossy()) {
            found.push(Leftover { path: path.to_string_lossy().into_owned(), kind: Kind::File, confidence, size: 0, why: why.into() });
        }
    }
    if let Some(location) = &id.location {
        if location.exists() && !fsx::is_link(location) && !ambiguous(&location.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) {
            push(&mut found, location, Confidence::Verified, "The folder the app was installed in");
        }
    }
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return found;
            }
            let path = entry.path();
            if fsx::is_link(&path) {
                continue;
            }
            let raw = entry_name(&path);
            if still_installed(others, &path, &raw) {
                continue;
            }
            if let Some(c) = classify(id, &raw) {
                let why = if c == Confidence::Verified { "Named after the app's own identifier" } else { "Similar to the app's name; check before ticking" };
                push(&mut found, &path, c, why);
            }
            if path.is_dir() && !ambiguous(&raw) || path.is_dir() && norm_publisher(&raw) == norm_publisher(&id.publisher) {
                let Ok(children) = fs::read_dir(&path) else { continue };
                for child in children.flatten() {
                    let cp = child.path();
                    if fsx::is_link(&cp) || !cp.is_dir() {
                        continue;
                    }
                    let cn = entry_name(&cp);
                    if still_installed(others, &cp, &cn) {
                        continue;
                    }
                    if let Some(c) = classify_pair(id, &raw, &cn) {
                        push(&mut found, &cp, better(c, Confidence::Verified), "Vendor and product folder of the app");
                    }
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn firefox() -> Identity {
        Identity {
            name: "Mozilla Firefox".into(),
            publisher: "Mozilla Corporation".into(),
            ids: vec!["Mozilla.Firefox".into(), "{3E9A1F2B-0000-0000-0000-000000000000}".into()],
            location: None,
        }
    }

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("notifi-left-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn identifier_is_verified_and_name_is_not() {
        let id = Identity { name: "Super Editor".into(), ids: vec!["SuperEditor".into()], ..Default::default() };
        assert_eq!(classify(&id, "SuperEditor"), Some(Confidence::Verified));
        assert_eq!(classify(&id, "super-editor"), Some(Confidence::Verified));
        assert_eq!(classify(&id, "Super Editor Backup"), Some(Confidence::Name));
        assert_eq!(classify(&id, "Other"), None);
    }

    #[test]
    fn guids_never_verify_and_shared_or_version_folders_are_refused() {
        let id = Identity { name: "Thing".into(), ids: vec!["{3E9A1F2B-0000-0000-0000-000000000000}".into(), "Common".into()], ..Default::default() };
        assert_eq!(classify(&id, "{3E9A1F2B-0000-0000-0000-000000000000}"), None);
        assert_eq!(classify(&id, "Common"), None);
        assert!(ambiguous("Microsoft"));
        assert!(ambiguous("1.2.3"));
        assert!(ambiguous("v2"));
        assert!(ambiguous("x64"));
        assert!(!ambiguous("Firefox"));
    }

    #[test]
    fn two_part_vendor_identifiers_pair_up() {
        let id = firefox();
        assert_eq!(classify_pair(&id, "Mozilla", "Firefox"), Some(Confidence::Verified));
        assert_eq!(classify_pair(&id, "Mozilla Corporation", "MozillaFirefox"), Some(Confidence::Verified));
        assert_eq!(classify_pair(&id, "Mozilla", "Thunderbird"), None);
        assert_eq!(classify_pair(&id, "Other", "Firefox"), None);
    }

    #[test]
    fn scan_finds_product_folder_but_not_the_vendor_folder_or_other_apps() {
        let root = fixture("scan");
        for dir in ["Mozilla/Firefox", "Mozilla/Thunderbird", "Unrelated", "Mozilla Firefox Notes", "Microsoft/Firefox"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        let found = scan_roots(&firefox(), &Others::default(), &[root.clone()], &AtomicBool::new(false));
        let paths: Vec<String> = found.iter().map(|f| f.path.replace('\\', "/")).collect();
        assert!(paths.iter().any(|p| p.ends_with("Mozilla/Firefox")), "{paths:?}");
        assert!(!paths.iter().any(|p| p.ends_with("Mozilla/Thunderbird")));
        assert!(!paths.iter().any(|p| p.ends_with("/Mozilla")));
        assert!(!paths.iter().any(|p| p.ends_with("Unrelated")));
        assert!(!paths.iter().any(|p| p.ends_with("Microsoft/Firefox")));
        let notes = found.iter().find(|f| f.path.ends_with("Mozilla Firefox Notes")).unwrap();
        assert_eq!(notes.confidence, Confidence::Name);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn data_of_apps_still_installed_is_never_offered() {
        let root = fixture("others");
        fs::create_dir_all(root.join("SuperEditor")).unwrap();
        fs::create_dir_all(root.join("SuperEditorPro")).unwrap();
        let id = Identity { name: "Super Editor".into(), ids: vec!["SuperEditor".into()], ..Default::default() };
        let others = Others { names: vec![norm("Super Editor Pro")], locations: vec![] };
        let found = scan_roots(&id, &others, &[root.clone()], &AtomicBool::new(false));
        assert!(found.iter().any(|f| f.path.ends_with("SuperEditor")));
        assert!(!found.iter().any(|f| f.path.ends_with("SuperEditorPro")));
        let blocked = Others { names: vec![], locations: vec![root.join("SuperEditor")] };
        assert!(!scan_roots(&id, &blocked, &[root.clone()], &AtomicBool::new(false)).iter().any(|f| f.path.ends_with("SuperEditor")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancelling_stops_the_scan() {
        let root = fixture("cancel");
        fs::create_dir_all(root.join("SuperEditor")).unwrap();
        let id = Identity { name: "Super Editor".into(), ids: vec!["SuperEditor".into()], ..Default::default() };
        assert!(scan_roots(&id, &Others::default(), &[root.clone()], &AtomicBool::new(true)).is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
