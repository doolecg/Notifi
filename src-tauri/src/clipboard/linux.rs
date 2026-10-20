//! Linux clipboard through the desktop's command-line tools: `wl-paste --watch` / `wl-copy` on
//! Wayland (data-control, so KDE yes and GNOME only with its extension), `xclip` polling on X11.
//! Paste goes through `wtype`, `ydotool` or `xdotool`, whichever is installed.

use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

use super::{history::{Capture, Image}, Event, Payload};

pub struct Snapshot {
    payload: Payload,
}

static SEQ: AtomicU32 = AtomicU32::new(1);
const TIMEOUT: Duration = Duration::from_secs(5);

fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn has(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()))
        .unwrap_or(false)
}

/// Runs a tool with optional stdin, giving up after the timeout so a slow source app can't hang us.
fn run(cmd: &str, args: &[&str], input: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(data);
    }
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().ok()?;
                return status.success().then_some(out);
            }
            Ok(None) if start.elapsed() < TIMEOUT => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

fn types() -> Vec<String> {
    let out = if wayland() {
        run("wl-paste", &["--list-types"], None)
    } else {
        run("xclip", &["-selection", "clipboard", "-o", "-t", "TARGETS"], None)
    };
    out.map(|b| String::from_utf8_lossy(&b).lines().map(|l| l.trim().to_string()).collect()).unwrap_or_default()
}

fn get(mime: &str) -> Option<Vec<u8>> {
    if wayland() {
        run("wl-paste", &["--no-newline", "--type", mime], None)
    } else {
        run("xclip", &["-selection", "clipboard", "-o", "-t", mime], None)
    }
}

fn set(mime: &str, data: &[u8]) -> Result<(), String> {
    let ok = if wayland() {
        run("wl-copy", &["--type", mime], Some(data)).is_some()
    } else {
        run("xclip", &["-selection", "clipboard", "-i", "-t", mime], Some(data)).is_some()
    };
    if ok {
        Ok(())
    } else {
        Err("Couldn't write the clipboard (needs wl-clipboard on Wayland or xclip on X11).".into())
    }
}

pub fn watch(tx: Sender<Event>) {
    thread::Builder::new()
        .name("clipboard-listener".into())
        .spawn(move || {
            if wayland() && has("wl-paste") {
                if let Ok(mut child) = Command::new("wl-paste")
                    .args(["--watch", "echo", "changed"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    if let Some(out) = child.stdout.take() {
                        for _ in BufReader::new(out).lines().map_while(Result::ok) {
                            SEQ.fetch_add(1, Ordering::SeqCst);
                            if tx.send(Event::Changed).is_err() {
                                break;
                            }
                        }
                    }
                    let _ = child.kill();
                    return;
                }
            }
            let mut last: Option<Vec<u8>> = None;
            loop {
                thread::sleep(Duration::from_millis(700));
                let now = types().into_iter().find(|t| t.starts_with("text/plain") || t == "UTF8_STRING" || t.starts_with("image/")).and_then(|t| get(&t));
                if now.is_some() && now != last {
                    if last.is_some() {
                        SEQ.fetch_add(1, Ordering::SeqCst);
                        if tx.send(Event::Changed).is_err() {
                            return;
                        }
                    }
                    last = now;
                }
            }
        })
        .ok();
}

pub fn sequence() -> u32 {
    SEQ.load(Ordering::SeqCst)
}

pub fn foreground_app() -> String {
    if wayland() {
        return String::new();
    }
    run("xdotool", &["getactivewindow", "getwindowpid"], None)
        .and_then(|pid| std::fs::read_to_string(format!("/proc/{}/comm", String::from_utf8_lossy(&pid).trim())).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn read() -> Result<Capture, String> {
    let list = types();
    if list.is_empty() {
        return Err("Clipboard tools not found or clipboard empty.".into());
    }
    let mut cap = Capture { app: foreground_app(), ..Default::default() };
    if list.iter().any(|t| t == "x-kde-passwordManagerHint") {
        cap.concealed = get("x-kde-passwordManagerHint").is_some_and(|v| v.starts_with(b"secret"));
        if cap.concealed {
            return Ok(cap);
        }
    }
    if list.iter().any(|t| t == "text/uri-list") {
        cap.files = get("text/uri-list")
            .map(|b| String::from_utf8_lossy(&b).lines().filter_map(|l| l.trim().strip_prefix("file://").map(percent_decode)).collect())
            .unwrap_or_default();
    }
    if let Some(t) = list.iter().find(|t| t.starts_with("text/plain;charset=utf-8") || *t == "UTF8_STRING" || *t == "text/plain") {
        cap.text = get(t).map(|b| String::from_utf8_lossy(&b).into_owned());
    }
    if list.iter().any(|t| t == "text/html") {
        cap.html = get("text/html").map(|b| String::from_utf8_lossy(&b).into_owned());
    }
    if cap.files.is_empty() && cap.text.is_none() && list.iter().any(|t| t == "image/png") {
        cap.image = get("image/png").and_then(|png| image::load_from_memory_with_format(&png, image::ImageFormat::Png).ok()).map(|img| {
            let rgba = img.to_rgba8();
            Image { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
        });
    }
    Ok(cap)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The copied image, even when text came with it.
pub fn read_image() -> Option<Image> {
    let png = get("image/png")?;
    let rgba = image::load_from_memory_with_format(&png, image::ImageFormat::Png).ok()?.to_rgba8();
    Some(Image { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

pub fn read_text() -> Option<String> {
    let list = types();
    let t = list.iter().find(|t| t.starts_with("text/plain") || *t == "UTF8_STRING")?;
    get(t).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// The tools hold one type at a time, so files win over images, images over text.
pub fn write(p: &Payload) -> Result<u32, String> {
    let next = sequence() + 1;
    if !p.files.is_empty() {
        let uris: String = p.files.iter().map(|f| format!("file://{f}\n")).collect();
        set("text/uri-list", uris.as_bytes())?;
    } else if let Some(png) = &p.png {
        set("image/png", png)?;
    } else if let Some(text) = &p.text {
        set("text/plain;charset=utf-8", text.as_bytes())?;
    }
    Ok(next)
}

pub fn write_if(seq: u32, p: &Payload) -> Result<Option<u32>, String> {
    if sequence() != seq {
        return Ok(None);
    }
    write(p).map(Some)
}

pub fn clear() -> Result<u32, String> {
    let next = sequence() + 1;
    let ok = if wayland() { run("wl-copy", &["--clear"], None).is_some() } else { run("xclip", &["-selection", "clipboard", "-i"], Some(b"")).is_some() };
    ok.then_some(next).ok_or_else(|| "Couldn't clear the clipboard.".into())
}

pub fn snapshot() -> Result<Snapshot, String> {
    let cap = read()?;
    let png = cap.image.as_ref().and_then(|img| super::encode_png(img).ok());
    Ok(Snapshot { payload: Payload { text: cap.text, html: None, rtf: None, files: cap.files, png } })
}

pub fn restore(s: &Snapshot) -> Result<u32, String> {
    write(&s.payload)
}

pub fn foreground() -> Option<isize> {
    if wayland() {
        return None;
    }
    run("xdotool", &["getactivewindow"], None).and_then(|b| String::from_utf8_lossy(&b).trim().parse().ok())
}

pub fn focus(window: isize) {
    let _ = run("xdotool", &["windowactivate", "--sync", &window.to_string()], None);
}

pub fn release_modifiers() {
    thread::sleep(Duration::from_millis(150));
}

pub fn send_paste() {
    if wayland() && has("wtype") {
        let _ = run("wtype", &["-M", "ctrl", "v", "-m", "ctrl"], None);
    } else if has("ydotool") {
        let _ = run("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"], None);
    } else {
        let _ = run("xdotool", &["key", "--clearmodifiers", "ctrl+v"], None);
    }
}

/// Presses Backspace `count` times.
pub fn erase(count: usize) {
    if wayland() && has("wtype") {
        let args: Vec<&str> = (0..count).flat_map(|_| ["-k", "BackSpace"]).collect();
        let _ = run("wtype", &args, None);
    } else if has("ydotool") {
        let args: Vec<&str> = std::iter::once("key").chain((0..count).flat_map(|_| ["14:1", "14:0"])).collect();
        let _ = run("ydotool", &args, None);
    } else {
        let args: Vec<&str> = ["key", "--clearmodifiers"].into_iter().chain((0..count).map(|_| "BackSpace")).collect();
        let _ = run("xdotool", &args, None);
    }
}

pub fn type_text(text: &str) {
    if wayland() && has("wtype") {
        let _ = run("wtype", &["--", text], None);
    } else if has("ydotool") {
        let _ = run("ydotool", &["type", "--", text], None);
    } else {
        let _ = run("xdotool", &["type", "--clearmodifiers", "--", text], None);
    }
}
