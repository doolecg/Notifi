//! Switcher state without any OS calls: ordering, search, selection and grid sizing.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    Previews,
    Icons,
    Simple,
}

impl Layout {
    pub fn parse(s: &str) -> Self {
        match s {
            "icons" => Self::Icons,
            "simple" => Self::Simple,
            _ => Self::Previews,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub title: String,
    /// Display name of the app (executable stem or WM_CLASS).
    pub app: String,
    pub pid: Option<u32>,
    pub minimized: bool,
    pub monitor: Option<String>,
    /// Data URL of the app icon.
    pub icon: Option<String>,
    /// No live thumbnail: minimised, paused for this app, or capture unavailable.
    pub no_thumb: bool,
}

/// Orders windows most recently used first: windows in `mru` (most recent first) keep that order,
/// the rest follow in `zorder` (front to back) order.
pub fn mru_order(zorder: &[u64], mru: &[u64]) -> Vec<u64> {
    let mut out: Vec<u64> = mru.iter().copied().filter(|id| zorder.contains(id)).collect();
    let rest: Vec<u64> = zorder.iter().copied().filter(|id| !out.contains(id)).collect();
    out.extend(rest);
    out
}

/// Moves `id` to the front of the history and trims it.
pub fn touch(mru: &mut Vec<u64>, id: u64) {
    mru.retain(|x| *x != id);
    mru.insert(0, id);
    mru.truncate(256);
}

/// How well `query` matches an entry; `None` when it doesn't. Higher is better.
pub fn score(app: &str, title: &str, query: &str) -> Option<u32> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let app = app.to_lowercase();
    let title = title.to_lowercase();
    let word_start = |s: &str| s.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q));
    if app == q {
        Some(100)
    } else if app.starts_with(&q) {
        Some(90)
    } else if title.starts_with(&q) {
        Some(80)
    } else if word_start(&app) {
        Some(70)
    } else if word_start(&title) {
        Some(60)
    } else if app.contains(&q) {
        Some(50)
    } else if title.contains(&q) {
        Some(40)
    } else if subsequence(&app, &q) {
        Some(20)
    } else if subsequence(&title, &q) {
        Some(10)
    } else {
        None
    }
}

fn subsequence(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().filter(|c| !c.is_whitespace()).all(|c| it.any(|h| h == c))
}

/// Indices of matching entries, best first; ties keep list (MRU) order.
pub fn rank(entries: &[Entry], query: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, u32)> =
        entries.iter().enumerate().filter_map(|(i, e)| score(&e.app, &e.title, query).map(|s| (i, s))).collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(i, _)| i).collect()
}

/// Apps in order of their most recent window, each with its windows' indices.
pub fn groups(entries: &[Entry], visible: &[usize]) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for &i in visible {
        let key = entries[i].app.to_lowercase();
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(i),
            None => out.push((key, vec![i])),
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub layout: Layout,
    pub entries: Vec<Entry>,
    pub query: String,
    /// Indices into `entries` in display order.
    pub visible: Vec<usize>,
    /// Position in `visible` (previews, simple) of the selected window.
    pub selected: usize,
    /// Icons layout: the selected app (index into `groups`) and window within it.
    pub group: usize,
    pub group_window: usize,
    #[serde(skip)]
    pub cols: usize,
}

impl Model {
    pub fn new(layout: Layout, entries: Vec<Entry>, cols: usize) -> Self {
        let visible = (0..entries.len()).collect();
        Self { layout, entries, query: String::new(), visible, selected: 0, group: 0, group_window: 0, cols: cols.max(1) }
    }

    pub fn groups(&self) -> Vec<(String, Vec<usize>)> {
        groups(&self.entries, &self.visible)
    }

    /// The entry that Enter or releasing the trigger would pick.
    pub fn current(&self) -> Option<&Entry> {
        let i = match self.layout {
            Layout::Icons => *self.groups().get(self.group)?.1.get(self.group_window)?,
            _ => *self.visible.get(self.selected)?,
        };
        self.entries.get(i)
    }

    fn len(&self) -> usize {
        match self.layout {
            Layout::Icons => self.groups().len(),
            _ => self.visible.len(),
        }
    }

    /// Moves the main selection by `delta`, wrapping around.
    pub fn step(&mut self, delta: isize) {
        let n = self.len() as isize;
        if n == 0 {
            return;
        }
        match self.layout {
            Layout::Icons => {
                self.group = (self.group as isize + delta).rem_euclid(n) as usize;
                self.group_window = 0;
            }
            _ => self.selected = (self.selected as isize + delta).rem_euclid(n) as usize,
        }
    }

    /// Arrow keys: left/right step by one, up/down by a row (no wrap past the ends).
    pub fn arrow(&mut self, dx: isize, dy: isize) {
        if self.layout == Layout::Icons && dy != 0 {
            self.step_window(dy.signum());
            return;
        }
        if dy == 0 {
            self.step(dx);
            return;
        }
        let n = self.visible.len();
        let cols = if self.layout == Layout::Simple { 1 } else { self.cols };
        let next = self.selected as isize + dy * cols as isize;
        if next >= 0 && (next as usize) < n {
            self.selected = next as usize;
        }
    }

    /// Moves between windows of the selected app.
    pub fn step_window(&mut self, delta: isize) {
        match self.layout {
            Layout::Icons => {
                let Some((_, list)) = self.groups().get(self.group).cloned() else { return };
                self.group_window = (self.group_window as isize + delta).rem_euclid(list.len() as isize) as usize;
            }
            _ => {
                let Some(app) = self.current().map(|e| e.app.to_lowercase()) else { return };
                let n = self.visible.len() as isize;
                for k in 1..n {
                    let at = (self.selected as isize + delta * k).rem_euclid(n) as usize;
                    if self.entries[self.visible[at]].app.to_lowercase() == app {
                        self.selected = at;
                        return;
                    }
                }
            }
        }
    }

    /// Selects the second most recent window (the Alt+Tab flip), or the last one when reversing.
    pub fn select_initial(&mut self, reverse: bool) {
        let n = self.len();
        self.selected = 0;
        self.group = 0;
        self.group_window = 0;
        if n > 1 {
            if reverse {
                self.step(-1);
            } else {
                self.step(1);
            }
        }
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.visible = rank(&self.entries, &self.query);
        self.selected = 0;
        self.group = 0;
        self.group_window = 0;
    }

    /// Selects the entry with this id (a click or hover in the UI).
    pub fn select_id(&mut self, id: u64) -> bool {
        match self.layout {
            Layout::Icons => {
                for (g, (_, list)) in self.groups().iter().enumerate() {
                    if let Some(w) = list.iter().position(|&i| self.entries[i].id == id) {
                        self.group = g;
                        self.group_window = w;
                        return true;
                    }
                }
                false
            }
            _ => match self.visible.iter().position(|&i| self.entries[i].id == id) {
                Some(p) => {
                    self.selected = p;
                    true
                }
                None => false,
            },
        }
    }

    /// Drops windows that closed; keeps the selection in range.
    pub fn remove(&mut self, ids: &[u64]) {
        let keep_id = self.current().map(|e| e.id);
        self.entries.retain(|e| !ids.contains(&e.id));
        self.visible = rank(&self.entries, &self.query);
        if !keep_id.is_some_and(|id| !ids.contains(&id) && self.select_id(id)) {
            let n = self.len();
            self.selected = self.selected.min(n.saturating_sub(1));
            self.group = self.group.min(n.saturating_sub(1));
            self.group_window = 0;
        }
    }
}

/// Card and panel sizes, all in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub card_w: f64,
    pub card_h: f64,
    pub thumb_h: f64,
    pub width: f64,
    pub height: f64,
}

pub const PAD: f64 = 18.0;
pub const GAP: f64 = 12.0;
pub const LABEL: f64 = 30.0;
pub const SEARCH: f64 = 34.0;

/// Base thumbnail size for a size setting; minimal cards have no thumbnail.
pub fn thumb_size(size: &str) -> (f64, f64) {
    match size {
        "large" => (300.0, 188.0),
        "extraLarge" => (400.0, 250.0),
        "minimal" => (180.0, 0.0),
        _ => (220.0, 138.0),
    }
}

/// Lays `n` cards out in the space available. Two or fewer windows get wider previews when
/// there is room; crowded lists shrink cards down to half size before adding rows past the screen.
pub fn grid(n: usize, size: (f64, f64), max_cols: usize, avail: (f64, f64)) -> Grid {
    let n = n.max(1);
    let (avail_w, avail_h) = (avail.0 - 2.0 * PAD, avail.1 - 2.0 * PAD - SEARCH);
    let fit_cols = |w: f64| (((avail_w + GAP) / (w + GAP)).floor() as usize).max(1);
    let mut scale = if n <= 2 && size.1 > 0.0 { 1.5 } else { 1.0 };
    loop {
        let (w, h) = (size.0 * scale, size.1 * scale);
        let mut cols = fit_cols(w).min(n);
        if max_cols > 0 {
            cols = cols.min(max_cols);
        }
        let rows = n.div_ceil(cols);
        let card_h = h + LABEL;
        let height = rows as f64 * card_h + (rows - 1) as f64 * GAP;
        let shrink_ok = scale > 0.5 + f64::EPSILON;
        let fits = height <= avail_h && cols as f64 * w + (cols - 1) as f64 * GAP <= avail_w;
        if fits || !shrink_ok {
            let width = cols as f64 * w + (cols - 1) as f64 * GAP + 2.0 * PAD;
            let total_h = height.min(avail_h.max(card_h)) + 2.0 * PAD + SEARCH;
            return Grid { cols, rows, card_w: w, card_h, thumb_h: h, width, height: total_h };
        }
        scale = if scale > 1.0 { 1.0 } else { (scale - 0.1).max(0.5) };
    }
}

/// Places a source of `src` size inside `cell`, keeping its aspect ratio and centring it.
pub fn fit_rect(cell: (f64, f64, f64, f64), src: (u32, u32)) -> (f64, f64, f64, f64) {
    let (x, y, w, h) = cell;
    if src.0 == 0 || src.1 == 0 || w <= 0.0 || h <= 0.0 {
        return cell;
    }
    let scale = (w / src.0 as f64).min(h / src.1 as f64);
    let (fw, fh) = (src.0 as f64 * scale, src.1 as f64 * scale);
    (x + (w - fw) / 2.0, y + (h - fh) / 2.0, fw, fh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: u64, app: &str, title: &str) -> Entry {
        Entry { id, title: title.into(), app: app.into(), pid: None, minimized: false, monitor: None, icon: None, no_thumb: false }
    }

    #[test]
    fn mru_keeps_history_then_zorder() {
        assert_eq!(mru_order(&[1, 2, 3, 4], &[3, 9, 1]), vec![3, 1, 2, 4]);
        assert_eq!(mru_order(&[5, 6], &[]), vec![5, 6]);
        let mut h = vec![1, 2, 3];
        touch(&mut h, 3);
        assert_eq!(h, vec![3, 1, 2]);
    }

    #[test]
    fn initial_selection_flips_to_previous() {
        let mut m = Model::new(Layout::Previews, vec![e(1, "a", ""), e(2, "b", ""), e(3, "c", "")], 3);
        m.select_initial(false);
        assert_eq!(m.current().unwrap().id, 2);
        m.select_initial(true);
        assert_eq!(m.current().unwrap().id, 3);
        m.step(1);
        assert_eq!(m.current().unwrap().id, 1);
        let mut one = Model::new(Layout::Previews, vec![e(1, "a", "")], 3);
        one.select_initial(false);
        assert_eq!(one.current().unwrap().id, 1);
    }

    #[test]
    fn search_ranking() {
        assert_eq!(score("notepad", "x", "note"), Some(90));
        assert!(score("chrome", "Notepad notes", "note").unwrap() < score("notepad", "", "note").unwrap());
        assert_eq!(score("code", "main.rs - notifi", "notifi"), Some(60));
        assert_eq!(score("explorer", "Downloads", "xyz"), None);
        assert_eq!(score("explorer", "Downloads", "epr"), Some(20));
        let list = vec![e(1, "chrome", "Notepad tips"), e(2, "notepad", "Untitled"), e(3, "explorer", "Docs")];
        assert_eq!(rank(&list, "note"), vec![1, 0]);
        assert_eq!(rank(&list, ""), vec![0, 1, 2]);
    }

    #[test]
    fn query_resets_selection() {
        let mut m = Model::new(Layout::Previews, vec![e(1, "chrome", ""), e(2, "notepad", "")], 2);
        m.select_initial(false);
        m.set_query("note".into());
        assert_eq!(m.current().unwrap().id, 2);
        m.set_query("zzz".into());
        assert!(m.current().is_none());
    }

    #[test]
    fn arrows_move_by_rows() {
        let mut m = Model::new(Layout::Previews, (1..=7).map(|i| e(i, "a", "")).collect(), 3);
        m.arrow(0, 1);
        assert_eq!(m.selected, 3);
        m.arrow(0, 1);
        assert_eq!(m.selected, 6);
        m.arrow(0, 1);
        assert_eq!(m.selected, 6);
        m.arrow(-1, 0);
        assert_eq!(m.selected, 5);
    }

    #[test]
    fn icon_layout_groups_apps() {
        let list = vec![e(1, "code", ""), e(2, "chrome", ""), e(3, "Code", ""), e(4, "chrome", "")];
        let mut m = Model::new(Layout::Icons, list, 4);
        assert_eq!(m.groups().len(), 2);
        m.step_window(1);
        assert_eq!(m.current().unwrap().id, 3);
        m.step(1);
        assert_eq!(m.current().unwrap().id, 2);
        assert!(m.select_id(4));
        assert_eq!((m.group, m.group_window), (1, 1));
    }

    #[test]
    fn same_app_cycling_in_previews() {
        let list = vec![e(1, "code", ""), e(2, "chrome", ""), e(3, "code", "")];
        let mut m = Model::new(Layout::Previews, list, 3);
        m.step_window(1);
        assert_eq!(m.current().unwrap().id, 3);
        m.step_window(1);
        assert_eq!(m.current().unwrap().id, 1);
    }

    #[test]
    fn removing_keeps_selection() {
        let mut m = Model::new(Layout::Previews, vec![e(1, "a", ""), e(2, "b", ""), e(3, "c", "")], 3);
        m.select_id(3);
        m.remove(&[1]);
        assert_eq!(m.current().unwrap().id, 3);
        m.remove(&[3]);
        assert_eq!(m.current().unwrap().id, 2);
    }

    #[test]
    fn grid_sizing_and_columns() {
        let g = grid(6, thumb_size("normal"), 0, (1920.0, 1080.0));
        assert_eq!((g.cols, g.rows), (6, 1));
        assert_eq!(g.card_w, 220.0);
        assert_eq!(g.width, 6.0 * 220.0 + 5.0 * GAP + 2.0 * PAD);
        let limited = grid(6, thumb_size("normal"), 4, (1920.0, 1080.0));
        assert_eq!((limited.cols, limited.rows), (4, 2));
        let pair = grid(2, thumb_size("normal"), 0, (1920.0, 1080.0));
        assert_eq!(pair.card_w, 330.0);
        let crowded = grid(60, thumb_size("extraLarge"), 0, (1280.0, 720.0));
        assert!(crowded.card_w < 400.0 && crowded.card_w >= 200.0);
        let minimal = grid(3, thumb_size("minimal"), 0, (1920.0, 1080.0));
        assert_eq!((minimal.thumb_h, minimal.card_w), (0.0, 180.0));
        let narrow = grid(3, thumb_size("normal"), 0, (300.0, 800.0));
        assert_eq!(narrow.cols, 1);
    }

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit_rect((0.0, 0.0, 200.0, 100.0), (400, 400)), (50.0, 0.0, 100.0, 100.0));
        assert_eq!(fit_rect((10.0, 10.0, 200.0, 100.0), (0, 0)), (10.0, 10.0, 200.0, 100.0));
    }
}
