//! The clipboard history store (spec/clipboard/clipboard-history.md, history-options.md):
//! dedupe, pins, retention and the optional sensitive-text filter. No OS calls in here.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cleanurl;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Rich,
    Image,
    Files,
    Url,
    Color,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    /// Plain text; for files the paths, one per line.
    pub text: String,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub files: Vec<String>,
    /// Image file stem under the images folder.
    pub image: Option<String>,
    pub width: u32,
    pub height: u32,
    pub app: String,
    /// Unix ms of the copy, or of the last paste from the history.
    pub created: i64,
    pub pinned: bool,
    pub hash: String,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            id: 0,
            kind: Kind::Text,
            text: String::new(),
            html: None,
            rtf: None,
            files: Vec::new(),
            image: None,
            width: 0,
            height: 0,
            app: String::new(),
            created: 0,
            pinned: false,
            hash: String::new(),
        }
    }
}

/// What the watcher read from the clipboard.
#[derive(Default, Debug, Clone)]
pub struct Capture {
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub files: Vec<String>,
    pub image: Option<Image>,
    pub app: String,
    /// The source app asked clipboard managers to leave this copy out.
    pub concealed: bool,
}

#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Longest text kept per entry; bigger copies are cut here.
const MAX_TEXT: usize = 2 * 1024 * 1024;

impl Capture {
    /// The entry this copy becomes, before it gets an id; `None` when there is nothing to keep.
    pub fn to_entry(&self, keep_media: bool, now: i64) -> Option<Entry> {
        let text = self.text.as_deref().filter(|t| !t.trim().is_empty());
        let mut e = Entry { app: self.app.clone(), created: now, ..Default::default() };
        if !self.files.is_empty() {
            if !keep_media {
                return None;
            }
            e.kind = Kind::Files;
            e.files = self.files.clone();
            e.text = self.files.join("\n");
        } else if let Some(text) = text {
            e.text = truncate(text, MAX_TEXT).to_string();
            e.html = self.html.clone();
            e.rtf = self.rtf.clone();
            e.kind = if cleanurl::lone_link(text).is_some() {
                Kind::Url
            } else if is_color(text) {
                Kind::Color
            } else if e.html.is_some() || e.rtf.is_some() {
                Kind::Rich
            } else {
                Kind::Text
            };
        } else if let Some(img) = &self.image {
            if !keep_media || img.width == 0 || img.height == 0 {
                return None;
            }
            e.kind = Kind::Image;
            e.width = img.width;
            e.height = img.height;
        } else {
            return None;
        }
        let mut h = Sha256::new();
        h.update(match e.kind {
            Kind::Image => b"i\0",
            Kind::Files => b"f\0",
            _ => b"t\0",
        });
        match (&self.image, e.kind) {
            (Some(img), Kind::Image) => {
                h.update(img.width.to_le_bytes());
                h.update(img.height.to_le_bytes());
                h.update(&img.rgba);
            }
            _ => h.update(e.text.as_bytes()),
        }
        e.hash = h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect();
        if e.kind == Kind::Image {
            e.image = Some(e.hash.clone());
        }
        Some(e)
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)` on their own.
pub fn is_color(text: &str) -> bool {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let lower = t.to_ascii_lowercase();
    ["rgb(", "rgba(", "hsl(", "hsla("].iter().any(|p| lower.starts_with(p))
        && lower.ends_with(')')
        && lower.len() < 64
        && lower[lower.find('(').unwrap() + 1..lower.len() - 1].chars().all(|c| c.is_ascii_digit() || " ,.%/deg".contains(c))
}

/// The optional filter: one token that looks like a password, key or token. Normal commands,
/// sentences and links pass even when they mention words like "password".
pub fn looks_sensitive(text: &str) -> bool {
    let t = text.trim();
    if t.contains("-----BEGIN") && t.contains("PRIVATE KEY") {
        return true;
    }
    if t.is_empty() || t.chars().any(char::is_whitespace) || cleanurl::lone_link(t).is_some() {
        return false;
    }
    const PREFIXES: &[&str] = &["sk-", "sk_live_", "rk_live_", "ghp_", "gho_", "ghu_", "ghs_", "github_pat_", "glpat-", "xoxb-", "xoxp-", "AKIA", "ASIA", "AIza", "ya29.", "eyJ"];
    if t.len() >= 20 && PREFIXES.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    if !(10..=64).contains(&t.len()) || t.contains(['/', '\\']) || t.starts_with(['-', '.', '$', '~']) {
        return false;
    }
    let classes = [
        t.chars().any(|c| c.is_ascii_lowercase()),
        t.chars().any(|c| c.is_ascii_uppercase()),
        t.chars().any(|c| c.is_ascii_digit()),
        t.chars().any(|c| c.is_ascii_punctuation()),
    ];
    let n = classes.iter().filter(|c| **c).count();
    let looks_like_word = t.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_');
    n >= 4 || (n == 3 && t.len() >= 16 && !looks_like_word)
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct History {
    pub next_id: u64,
    pub entries: Vec<Entry>,
    /// The list window's last size, logical px.
    pub window: Option<(f64, f64)>,
}

/// What an insert did.
#[derive(Debug, PartialEq)]
pub enum Inserted {
    New(u64),
    /// Same content was already there and moved to the top.
    Moved(u64),
    /// Same content is pinned; it stays where it is.
    Pinned(u64),
}

impl History {
    /// Adds a copy at the top. A copy that matches an existing entry replaces it at the top,
    /// unless that entry is pinned.
    pub fn insert(&mut self, mut entry: Entry) -> Inserted {
        if let Some(pos) = self.entries.iter().position(|e| e.hash == entry.hash) {
            let old = &mut self.entries[pos];
            if old.pinned {
                return Inserted::Pinned(old.id);
            }
            let mut old = self.entries.remove(pos);
            old.created = entry.created;
            if !entry.app.is_empty() {
                old.app = std::mem::take(&mut entry.app);
            }
            if entry.html.is_some() || entry.rtf.is_some() {
                old.html = entry.html.take();
                old.rtf = entry.rtf.take();
            }
            let id = old.id;
            self.entries.insert(0, old);
            return Inserted::Moved(id);
        }
        self.next_id += 1;
        entry.id = self.next_id;
        let id = entry.id;
        self.entries.insert(0, entry);
        Inserted::New(id)
    }

    /// Moves an entry that was pasted or copied from the history to the top (pinned stay put).
    pub fn touch(&mut self, id: u64, now: i64) {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id && !e.pinned) {
            let mut e = self.entries.remove(pos);
            e.created = now;
            self.entries.insert(0, e);
        }
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn set_pinned(&mut self, id: u64, pinned: bool) -> bool {
        match self.entries.iter_mut().find(|e| e.id == id) {
            Some(e) if e.pinned != pinned => {
                e.pinned = pinned;
                true
            }
            _ => false,
        }
    }

    pub fn delete(&mut self, ids: &[u64]) -> Vec<Entry> {
        self.take(|e| ids.contains(&e.id))
    }

    /// Unpinned entries copied at or before `before` (ms): what "Clear unpinned" removes.
    pub fn clear_count(&self, before: i64) -> usize {
        self.entries.iter().filter(|e| !e.pinned && e.created <= before).count()
    }

    pub fn clear_unpinned(&mut self, before: i64) -> Vec<Entry> {
        self.take(|e| !e.pinned && e.created <= before)
    }

    /// Applies retention: drops unpinned entries older than `keep_days` (0 = no age limit), then
    /// the oldest unpinned ones past `max_items` (0 = no limit).
    pub fn prune(&mut self, now: i64, max_items: usize, keep_days: u32) -> Vec<Entry> {
        let mut removed = Vec::new();
        if keep_days > 0 {
            let cutoff = now - keep_days as i64 * 86_400_000;
            removed.extend(self.take(|e| !e.pinned && e.created < cutoff));
        }
        if max_items > 0 {
            let mut unpinned = 0;
            let mut over = Vec::new();
            for e in &self.entries {
                if !e.pinned {
                    unpinned += 1;
                    if unpinned > max_items {
                        over.push(e.id);
                    }
                }
            }
            removed.extend(self.take(|e| over.contains(&e.id)));
        }
        removed
    }

    fn take(&mut self, pred: impl Fn(&Entry) -> bool) -> Vec<Entry> {
        let (gone, kept): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| pred(e));
        self.entries = kept;
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    fn text(t: &str, now: i64) -> Entry {
        Capture { text: Some(t.into()), app: "Editor".into(), ..Default::default() }.to_entry(true, now).unwrap()
    }

    #[test]
    fn kinds() {
        assert_eq!(text("hello", 0).kind, Kind::Text);
        assert_eq!(text("https://example.com/a", 0).kind, Kind::Url);
        assert_eq!(text("#0f9483", 0).kind, Kind::Color);
        assert_eq!(text("rgb(10, 20, 30)", 0).kind, Kind::Color);
        let rich = Capture { text: Some("bold".into()), html: Some("<b>bold</b>".into()), ..Default::default() };
        assert_eq!(rich.to_entry(true, 0).unwrap().kind, Kind::Rich);
        let files = Capture { files: vec!["C:\\a.txt".into()], text: Some("ignored".into()), ..Default::default() };
        assert_eq!(files.to_entry(true, 0).unwrap().kind, Kind::Files);
        assert!(files.to_entry(false, 0).is_none());
        let img = Capture { image: Some(Image { width: 1, height: 1, rgba: vec![1, 2, 3, 4] }), ..Default::default() };
        let e = img.to_entry(true, 0).unwrap();
        assert_eq!((e.kind, e.image.as_deref()), (Kind::Image, Some(e.hash.as_str())));
        assert!(img.to_entry(false, 0).is_none());
        assert!(Capture { text: Some("   ".into()), ..Default::default() }.to_entry(true, 0).is_none());
    }

    #[test]
    fn dedupe_moves_to_top_but_pins_stay() {
        let mut h = History::default();
        let Inserted::New(a) = h.insert(text("a", 1)) else { panic!() };
        let Inserted::New(b) = h.insert(text("b", 2)) else { panic!() };
        assert_eq!(h.insert(text("a", 3)), Inserted::Moved(a));
        assert_eq!(h.entries.iter().map(|e| e.id).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(h.entries[0].created, 3);
        assert_eq!(h.entries.len(), 2);

        h.set_pinned(b, true);
        assert_eq!(h.insert(text("b", 4)), Inserted::Pinned(b));
        assert_eq!(h.entries[1].id, b);
        h.touch(b, 5);
        assert_eq!(h.entries[1].id, b);
        h.touch(a, 6);
        assert_eq!(h.entries[0].id, a);
    }

    #[test]
    fn same_text_with_new_formatting_keeps_one_entry() {
        let mut h = History::default();
        h.insert(text("x", 1));
        let rich = Capture { text: Some("x".into()), html: Some("<i>x</i>".into()), ..Default::default() }.to_entry(true, 2).unwrap();
        h.insert(rich);
        assert_eq!(h.entries.len(), 1);
        assert_eq!(h.entries[0].html.as_deref(), Some("<i>x</i>"));
    }

    #[test]
    fn retention_by_age_and_count() {
        let now = 100 * DAY;
        let mut h = History::default();
        for i in 0..10 {
            h.insert(text(&format!("t{i}"), now - (10 - i) * DAY));
        }
        let oldest = h.entries.last().unwrap().id;
        h.set_pinned(oldest, true);
        let gone = h.prune(now, 0, 5);
        assert_eq!(gone.len(), 4, "entries older than 5 days, except the pinned one");
        assert!(h.get(oldest).is_some());
        let gone = h.prune(now, 3, 0);
        assert_eq!(gone.len(), 2);
        assert_eq!(h.entries.iter().filter(|e| !e.pinned).count(), 3);
        assert!(h.get(oldest).is_some());
        assert!(h.prune(now, 0, 0).is_empty());
    }

    #[test]
    fn clear_keeps_pins_and_newer_copies() {
        let mut h = History::default();
        h.insert(text("a", 1));
        h.insert(text("b", 2));
        let pinned = h.entries[0].id;
        h.set_pinned(pinned, true);
        h.insert(text("c", 3));
        assert_eq!(h.clear_count(2), 1);
        h.insert(text("d", 9));
        assert_eq!(h.clear_unpinned(2).len(), 1);
        let left: Vec<_> = h.entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(left, vec!["d", "c", "b"]);
    }

    #[test]
    fn sensitive_filter() {
        for s in ["Tr0ub4dor&3xyz", "sk-abcdefghijklmnopqrstuvwxyz", "ghp_1234567890abcdefghijABCDEFGHIJ", "-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----", "aB3$kL9!mQ2@"] {
            assert!(looks_sensitive(s), "{s}");
        }
        for s in [
            "sudo passwd root",
            "Remember to change your password",
            "https://example.com/reset-password?token=abc",
            "git commit -m \"fix password check\"",
            "hello",
            "C:\\Users\\Me\\Documents\\Passwords2024.txt",
            "README.md",
            "some_function_name",
            "#0f9483",
            "--password-store=basic",
        ] {
            assert!(!looks_sensitive(s), "{s}");
        }
    }
}
