//! Menu commands of another app's window, for the Command Bar's "App menu commands" source.
//!
//! Windows reads them through UI Automation (`win_menus`): the window's menu bar is walked and each
//! menu is opened briefly to read its items. Other systems report the feature as unsupported.

use crate::error::Result;

/// A menu tree as read from the app: a name and the items under it (empty for a command).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    pub name: String,
    pub children: Vec<Node>,
}

/// One runnable command with the menus leading to it, e.g. `["File", "Save As"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub path: Vec<String>,
}

impl MenuItem {
    pub fn title(&self) -> &str {
        self.path.last().map(String::as_str).unwrap_or_default()
    }

    /// The menus above the command, `File > Export`.
    pub fn trail(&self) -> String {
        self.path[..self.path.len().saturating_sub(1)].join(" > ")
    }
}

/// Most commands one app contributes; big IDEs have thousands and the rest would only add noise.
pub const MAX_ITEMS: usize = 400;

/// A menu entry's label without the `&` mnemonic marker, the accelerator after a tab and a trailing
/// ellipsis. Separators and blank labels come out empty.
pub fn clean(raw: &str) -> String {
    let label = raw.split('\t').next().unwrap_or_default();
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    let out = out.trim().trim_end_matches(['.', '\u{2026}']).trim().to_string();
    if out.chars().all(|c| c == '-' || c == '_') { String::new() } else { out }
}

/// Every command in the trees, depth first in menu order. Names are cleaned, blank ones dropped
/// (their children are kept under the parent), repeats of the same path are listed once, and the
/// list stops at [`MAX_ITEMS`].
pub fn flatten(roots: &[Node]) -> Vec<MenuItem> {
    fn walk(nodes: &[Node], trail: &mut Vec<String>, out: &mut Vec<MenuItem>) {
        for node in nodes {
            if out.len() >= MAX_ITEMS {
                return;
            }
            let name = clean(&node.name);
            let named = !name.is_empty();
            if named {
                trail.push(name);
            }
            if node.children.is_empty() {
                if named && !out.iter().any(|m| m.path == *trail) {
                    out.push(MenuItem { path: trail.clone() });
                }
            } else {
                walk(&node.children, trail, out);
            }
            if named {
                trail.pop();
            }
        }
    }
    let mut out = Vec::new();
    walk(roots, &mut Vec::new(), &mut out);
    out
}

/// Reads the menu commands of the window `window` (a window id from the window list).
pub fn list(window: u64) -> Result<Vec<MenuItem>> {
    #[cfg(windows)]
    {
        Ok(flatten(&crate::win_menus::read(window)?))
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Err(crate::error::PlatformError::unsupported(crate::Feature::WindowList, "menu commands need Windows UI Automation"))
    }
}

/// Opens the menus along `path` in the window and runs the command at the end.
pub fn invoke(window: u64, path: &[String]) -> Result<()> {
    #[cfg(windows)]
    {
        crate::win_menus::invoke(window, path)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, path);
        Err(crate::error::PlatformError::unsupported(crate::Feature::WindowList, "menu commands need Windows UI Automation"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str) -> Node {
        Node { name: name.into(), children: Vec::new() }
    }

    fn menu(name: &str, children: Vec<Node>) -> Node {
        Node { name: name.into(), children }
    }

    fn paths(items: &[MenuItem]) -> Vec<String> {
        items.iter().map(|m| m.path.join(">")).collect()
    }

    #[test]
    fn labels_lose_mnemonics_accelerators_and_ellipses() {
        assert_eq!(clean("&Save As...\tCtrl+Shift+S"), "Save As");
        assert_eq!(clean("Save \u{2026}"), "Save");
        assert_eq!(clean("Tom && Jerry"), "Tom & Jerry");
        assert_eq!(clean("-"), "");
        assert_eq!(clean("  "), "");
    }

    #[test]
    fn commands_keep_their_menu_trail() {
        let tree = [menu("&File", vec![leaf("&New"), menu("Export", vec![leaf("PDF"), leaf("PNG...")])]), menu("Help", vec![leaf("About")])];
        let items = flatten(&tree);
        assert_eq!(paths(&items), ["File>New", "File>Export>PDF", "File>Export>PNG", "Help>About"]);
        assert_eq!(items[1].title(), "PDF");
        assert_eq!(items[1].trail(), "File > Export");
    }

    #[test]
    fn separators_blanks_and_repeats_are_dropped() {
        let tree = [menu("Edit", vec![leaf("Cut"), leaf("-"), leaf(""), leaf("Cut"), menu("", vec![leaf("Paste")])])];
        assert_eq!(paths(&flatten(&tree)), ["Edit>Cut", "Edit>Paste"]);
    }

    #[test]
    fn an_empty_menu_is_not_a_command_but_a_bar_item_alone_is() {
        assert_eq!(paths(&flatten(&[leaf("Window")])), ["Window"]);
    }

    #[test]
    fn the_list_stops_at_the_limit() {
        let many: Vec<Node> = (0..MAX_ITEMS + 50).map(|i| leaf(&format!("Item {i}"))).collect();
        assert_eq!(flatten(&[menu("Big", many)]).len(), MAX_ITEMS);
    }
}
