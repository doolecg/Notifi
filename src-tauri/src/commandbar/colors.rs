//! Colours typed into the bar: `#3a7`, `#33aa77`, `rgb(51, 170, 119)`, `hsl(150, 54%, 43%)`,
//! optionally followed by `to hex|rgb|hsl|css|qt`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// `#rrggbb`, for the swatch.
    pub swatch: String,
    pub title: String,
    pub subtitle: String,
    /// What Enter copies.
    pub copy: String,
}

fn parse_hex(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    let nibble = |i: usize| u8::from_str_radix(&digits[i..i + 1], 16).ok().map(|n| n * 17);
    match digits.len() {
        3 => Some(Color { r: nibble(0)?, g: nibble(1)?, b: nibble(2)?, a: 255 }),
        4 => Some(Color { r: nibble(0)?, g: nibble(1)?, b: nibble(2)?, a: nibble(3)? }),
        6 => Some(Color { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 255 }),
        8 => Some(Color { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
        _ => None,
    }
}

/// The numbers inside `name( ... )`, split on commas, spaces or a slash; percent signs are dropped.
fn function_args(text: &str, names: &[&str]) -> Option<Vec<f64>> {
    let open = text.find('(')?;
    let name = text[..open].trim();
    if !names.iter().any(|n| name.eq_ignore_ascii_case(n)) || !text.ends_with(')') {
        return None;
    }
    let inner = &text[open + 1..text.len() - 1];
    let args: Option<Vec<f64>> = inner
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.trim_end_matches('%').trim_end_matches("deg").parse::<f64>().ok())
        .collect();
    args.filter(|a| a.len() == 3 || a.len() == 4)
}

fn alpha(args: &[f64], percent_form: bool) -> u8 {
    match args.get(3) {
        Some(a) if percent_form => (a.clamp(0.0, 100.0) * 2.55).round() as u8,
        Some(a) => (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        None => 255,
    }
}

pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let (s, l) = (s.clamp(0.0, 100.0) / 100.0, l.clamp(0.0, 100.0) / 100.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (byte(r), byte(g), byte(b))
}

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u32, u32, u32) {
    let (r, g, b) = (f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let sector = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let h = sector * 60.0;
    ((h.round() as u32) % 360, (s * 100.0).round() as u32, (l * 100.0).round() as u32)
}

pub fn parse_color(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some(c) = parse_hex(text) {
        return Some(c);
    }
    if let Some(a) = function_args(text, &["rgb", "rgba"]) {
        let byte = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        return Some(Color { r: byte(a[0]), g: byte(a[1]), b: byte(a[2]), a: alpha(&a, false) });
    }
    let a = function_args(text, &["hsl", "hsla"])?;
    let (r, g, b) = hsl_to_rgb(a[0], a[1], a[2]);
    Some(Color { r, g, b, a: alpha(&a, false) })
}

fn format_as(c: Color, format: &str) -> Option<String> {
    let opaque = c.a == 255;
    let hex = if opaque { format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b) } else { format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a) };
    let alpha = f64::from(c.a) / 255.0;
    let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
    Some(match format {
        "hex" => hex,
        "rgb" if opaque => format!("rgb({}, {}, {})", c.r, c.g, c.b),
        "rgb" => format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, round2(alpha)),
        "hsl" if opaque => format!("hsl({h}, {s}%, {l}%)"),
        "hsl" => format!("hsla({h}, {s}%, {l}%, {})", round2(alpha)),
        "css" if opaque => format!("rgb({} {} {})", c.r, c.g, c.b),
        "css" => format!("rgb({} {} {} / {})", c.r, c.g, c.b, round2(alpha)),
        "qt" if opaque => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        "qt" => format!("#{:02x}{:02x}{:02x}{:02x}", c.a, c.r, c.g, c.b),
        _ => return None,
    })
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The row for a query that is a colour, with or without a conversion phrase.
pub fn interpret(query: &str) -> Option<Found> {
    let query = query.trim();
    let lower = query.to_ascii_lowercase();
    let (colour, target) = match lower.rfind(" to ").or_else(|| lower.rfind(" in ")) {
        Some(i) => (&query[..i], Some(lower[i + 4..].trim().to_string())),
        None => (query, None),
    };
    let c = parse_color(colour)?;
    let swatch = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    match target {
        Some(format) => {
            let out = format_as(c, &format)?;
            Some(Found { swatch, title: out.clone(), subtitle: format!("{colour} as {format} - Enter copies it"), copy: out })
        }
        None => {
            let hex = format_as(c, "hex")?;
            let rest = [format_as(c, "rgb")?, format_as(c, "hsl")?].join(" - ");
            Some(Found { swatch, title: hex.clone(), subtitle: format!("{rest} - add \"to rgb\" to convert"), copy: hex })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hex_in_every_length() {
        assert_eq!(parse_color("#f00"), Some(Color { r: 255, g: 0, b: 0, a: 255 }));
        assert_eq!(parse_color("#33aa77"), Some(Color { r: 51, g: 170, b: 119, a: 255 }));
        assert_eq!(parse_color("#33aa7780").map(|c| c.a), Some(128));
        assert_eq!(parse_color("#f008").map(|c| c.a), Some(136));
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("#ggg"), None);
        assert_eq!(parse_color("red"), None);
    }

    #[test]
    fn reads_rgb_and_hsl_functions() {
        assert_eq!(parse_color("rgb(51, 170, 119)"), Some(Color { r: 51, g: 170, b: 119, a: 255 }));
        assert_eq!(parse_color("rgb(51 170 119 / 0.5)").map(|c| c.a), Some(128));
        assert_eq!(parse_color("hsl(0, 100%, 50%)"), Some(Color { r: 255, g: 0, b: 0, a: 255 }));
        assert_eq!(parse_color("hsl(120deg 100% 25%)"), Some(Color { r: 0, g: 128, b: 0, a: 255 }));
        assert_eq!(parse_color("rgb(1, 2)"), None);
        assert_eq!(parse_color("foo(1, 2, 3)"), None);
    }

    #[test]
    fn converts_between_models() {
        assert_eq!(rgb_to_hsl(255, 0, 0), (0, 100, 50));
        assert_eq!(rgb_to_hsl(0, 0, 255), (240, 100, 50));
        assert_eq!(rgb_to_hsl(128, 128, 128), (0, 0, 50));
        let c = parse_color("#33aa77").unwrap();
        let (h, s, l) = rgb_to_hsl(c.r, c.g, c.b);
        let (r, g, b) = hsl_to_rgb(f64::from(h), f64::from(s), f64::from(l));
        assert!((i32::from(r) - 51).abs() <= 2 && (i32::from(g) - 170).abs() <= 2 && (i32::from(b) - 119).abs() <= 2);
    }

    #[test]
    fn interpret_previews_and_converts() {
        let preview = interpret("#ff0000").unwrap();
        assert_eq!(preview.copy, "#ff0000");
        assert_eq!(preview.swatch, "#ff0000");
        assert!(preview.subtitle.contains("rgb(255, 0, 0)") && preview.subtitle.contains("hsl(0, 100%, 50%)"));
        assert_eq!(interpret("#f00 to rgb").unwrap().copy, "rgb(255, 0, 0)");
        assert_eq!(interpret("#f00 to hsl").unwrap().copy, "hsl(0, 100%, 50%)");
        assert_eq!(interpret("rgb(255,0,0) to hex").unwrap().copy, "#ff0000");
        assert_eq!(interpret("#ff000080 to css").unwrap().copy, "rgb(255 0 0 / 0.5)");
        assert_eq!(interpret("#ff000080 to qt").unwrap().copy, "#80ff0000");
        assert!(interpret("#f00 to ansi").is_none());
        assert!(interpret("hello").is_none());
        assert!(interpret("2 + 2").is_none());
    }
}
