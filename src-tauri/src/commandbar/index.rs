//! The file index, user scripts and the helpers that open things.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_FILES: usize = 60_000;
const MAX_DEPTH: usize = 5;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
}

const SKIP_DIRS: &[&str] = &["node_modules", "target", "AppData", "$RECYCLE.BIN", "System Volume Information", "__pycache__", "venv"];

fn skip(name: &str) -> bool {
    name.starts_with('.') || SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// A capped recursive scan; hidden folders and build output are left out.
pub fn scan_files(roots: &[PathBuf]) -> Vec<FileEntry> {
    let mut out = Vec::new();
    for root in roots {
        walk(root, 0, &mut out);
        if out.len() >= MAX_FILES {
            break;
        }
    }
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<FileEntry>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        if out.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if skip(&name) {
            continue;
        }
        let path = entry.path();
        let dir_entry = entry.file_type().is_ok_and(|t| t.is_dir());
        out.push(FileEntry { name, path: path.clone(), dir: dir_entry });
        if dir_entry && depth < MAX_DEPTH {
            walk(&path, depth + 1, out);
        }
    }
}

pub fn default_folders() -> Vec<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    home.map(|h| ["Documents", "Desktop", "Downloads"].iter().map(|f| h.join(f)).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub name: String,
    pub path: PathBuf,
}

fn runnable(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    #[cfg(windows)]
    {
        matches!(ext.as_str(), "ps1" | "bat" | "cmd" | "exe" | "py" | "sh")
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        matches!(ext.as_str(), "sh" | "py" | "ps1") || fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
}

pub fn list_scripts(folder: &Path) -> Vec<Script> {
    let Ok(read) = fs::read_dir(folder) else { return Vec::new() };
    let mut out: Vec<Script> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && runnable(p))
        .filter_map(|p| Some(Script { name: p.file_stem()?.to_str()?.to_string(), path: p }))
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Splits typed arguments on spaces, keeping "double quoted" parts together.
pub fn split_args(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut has) = (Vec::new(), String::new(), false, false);
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.stdin(Stdio::null())
}

/// Runs a script with `args` and returns what it printed.
pub fn run_script(path: &Path, args: &[String]) -> Result<String, String> {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    let mut cmd = match ext.as_str() {
        "ps1" => {
            let mut c = Command::new(if cfg!(windows) { "powershell" } else { "pwsh" });
            c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(path);
            c
        }
        "bat" | "cmd" => {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(path);
            c
        }
        "py" => {
            let mut c = Command::new(if cfg!(windows) { "python" } else { "python3" });
            c.arg(path);
            c
        }
        "sh" => {
            let mut c = Command::new("sh");
            c.arg(path);
            c
        }
        _ => Command::new(path),
    };
    let out = quiet(cmd.args(args)).output().map_err(|e| format!("Could not start the script: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    if out.status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("The script failed: {}", err.lines().next().unwrap_or("no message")))
    }
}

pub fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    quiet(&mut cmd).spawn().map(|_| ()).map_err(|e| format!("Could not open it: {e}"))
}

pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut c = Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        c.spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        open_path(path.parent().unwrap_or(path))
    }
}

pub fn open_appx(id: &str) -> Result<(), String> {
    Command::new("explorer").arg(format!("shell:AppsFolder\\{id}")).spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn spawn_exec(exec: &str) -> Result<(), String> {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", &format!("exec {exec}")]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_keep_quoted_parts() {
        assert_eq!(split_args(r#"one "two words" three"#), vec!["one", "two words", "three"]);
        assert_eq!(split_args("  a   b "), vec!["a", "b"]);
        assert_eq!(split_args(r#""" x"#), vec!["", "x"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn scan_skips_hidden_and_build_folders() {
        let dir = std::env::temp_dir().join(format!("notifi-cb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::create_dir_all(dir.join("node_modules")).unwrap();
        fs::write(dir.join("a/b/deep.txt"), "x").unwrap();
        fs::write(dir.join(".git/config"), "x").unwrap();
        fs::write(dir.join("node_modules/pkg.js"), "x").unwrap();
        let names: Vec<String> = scan_files(&[dir.clone()]).into_iter().map(|f| f.name).collect();
        let _ = fs::remove_dir_all(&dir);
        assert!(names.contains(&"deep.txt".to_string()));
        assert!(!names.contains(&"config".to_string()));
        assert!(!names.contains(&"pkg.js".to_string()));
    }
}
