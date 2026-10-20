use crate::error::Result;
use crate::linux_common::{config_dir, portal_dark_mode};

pub(crate) async fn dark_mode() -> Result<bool> {
    let text = config_dir().and_then(|d| std::fs::read_to_string(d.join("kdeglobals")).ok());
    match text.as_deref().and_then(kdeglobals_is_dark) {
        Some(dark) => Ok(dark),
        None => portal_dark_mode().await,
    }
}

/// Judges by the window background colour, falling back to the colour scheme's name.
pub(crate) fn kdeglobals_is_dark(text: &str) -> Option<bool> {
    let mut section = "";
    let mut scheme_name = None;
    let mut background = None;
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        match (section, key.trim()) {
            ("General", "ColorScheme") => scheme_name = Some(value.trim().to_string()),
            ("Colors:Window", "BackgroundNormal") => background = parse_rgb(value),
            _ => {}
        }
    }
    if let Some((r, g, b)) = background {
        let luma = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
        return Some(luma < 128.0);
    }
    scheme_name.map(|n| n.to_ascii_lowercase().contains("dark"))
}

fn parse_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let mut parts = value.split(',').map(|p| p.trim().parse::<u8>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

#[cfg(test)]
mod tests {
    use super::kdeglobals_is_dark;

    #[test]
    fn background_colour_wins() {
        let breeze_dark = "[General]\nColorScheme=MyTheme\n\n[Colors:Window]\nBackgroundNormal=32,35,38\n";
        assert_eq!(kdeglobals_is_dark(breeze_dark), Some(true));
        let light = "[Colors:Window]\nBackgroundNormal=239,240,241\n[General]\nColorScheme=SomethingDark\n";
        assert_eq!(kdeglobals_is_dark(light), Some(false));
    }

    #[test]
    fn scheme_name_fallback() {
        assert_eq!(kdeglobals_is_dark("[General]\nColorScheme=BreezeDark\n"), Some(true));
        assert_eq!(kdeglobals_is_dark("[General]\nColorScheme=BreezeLight\n"), Some(false));
        assert_eq!(kdeglobals_is_dark("[KDE]\nSingleClick=false\n"), None);
    }
}
