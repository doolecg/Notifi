//! Column-aligned console tables (winget, Scoop) cut by display column, not by byte.

fn width(c: char) -> usize {
    match c as u32 {
        0x300..=0x36F | 0x200B..=0x200F => 0,
        0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE6F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F300..=0x1FAFF | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// The text of `line` between display columns `from` and `to` (exclusive), trimmed.
fn slice(line: &str, from: usize, to: usize) -> String {
    let mut col = 0;
    let mut out = String::new();
    for c in line.chars() {
        let w = width(c);
        if col >= from && col < to {
            out.push(c);
        }
        col += w;
        if col >= to {
            break;
        }
    }
    out.trim().to_string()
}

pub struct Columns(Vec<usize>);

impl Columns {
    pub fn cells(&self, line: &str) -> Vec<String> {
        self.0
            .iter()
            .enumerate()
            .map(|(i, &start)| slice(line, start, self.0.get(i + 1).copied().unwrap_or(usize::MAX)))
            .collect()
    }
}

/// Columns starting at each of `names` (single-word headers) found in `line`; `None` unless all are present.
pub fn header_columns(line: &str, names: &[&str]) -> Option<Columns> {
    let mut starts = Vec::new();
    let mut col = 0;
    let mut word = String::new();
    let mut word_start = 0;
    let mut found: Vec<(String, usize)> = Vec::new();
    for c in line.chars().chain(std::iter::once(' ')) {
        if c.is_whitespace() {
            if !word.is_empty() {
                found.push((std::mem::take(&mut word), word_start));
            }
        } else {
            if word.is_empty() {
                word_start = col;
            }
            word.push(c);
        }
        col += width(c);
    }
    for name in names {
        starts.push(found.iter().find(|(w, _)| w == name)?.1);
    }
    (starts.first() == Some(&0) && starts.windows(2).all(|w| w[0] < w[1])).then_some(Columns(starts))
}

/// Columns taken from the runs of dashes in `rule` (Scoop's `Format-Table` output).
pub fn rule_columns(rule: &str) -> Option<Columns> {
    let mut starts = Vec::new();
    let mut prev_dash = false;
    for (i, c) in rule.chars().enumerate() {
        let dash = c == '-';
        if dash && !prev_dash {
            starts.push(i);
        }
        prev_dash = dash;
    }
    (!starts.is_empty() && rule.chars().all(|c| c == '-' || c == ' ')).then_some(Columns(starts))
}

/// Strips progress-spinner output: keeps what follows the last carriage return.
pub fn clean_line(line: &str) -> &str {
    line.rsplit('\r').next().unwrap_or(line).trim_end()
}

/// Rows of a table whose header row contains all `names`; a repeated header starts a new block.
pub fn by_headers(text: &str, names: &[&str]) -> Vec<Vec<String>> {
    let mut cols: Option<Columns> = None;
    let mut rows = Vec::new();
    let mut after_header = false;
    for raw in text.lines() {
        let line = clean_line(raw);
        if let Some(c) = header_columns(line, names) {
            cols = Some(c);
            after_header = true;
            continue;
        }
        if after_header {
            after_header = false;
            if line.chars().all(|c| c == '-') && !line.is_empty() {
                continue;
            }
        }
        let Some(c) = &cols else { continue };
        if line.trim().is_empty() {
            cols = None;
            continue;
        }
        let cells = c.cells(line);
        if cells.len() == names.len() && cells.iter().take(2).all(|c| !c.is_empty()) {
            rows.push(cells);
        }
    }
    rows
}

/// Rows of a table whose columns are given by a dashed rule under the header.
pub fn by_rule(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let lines: Vec<&str> = text.lines().map(clean_line).collect();
    for (i, line) in lines.iter().enumerate().skip(1) {
        let Some(cols) = rule_columns(line) else { continue };
        if !line.starts_with('-') || line.len() < 3 {
            continue;
        }
        let header = cols.cells(lines[i - 1]);
        let rows = lines[i + 1..]
            .iter()
            .take_while(|l| !l.trim().is_empty())
            .map(|l| cols.cells(l))
            .collect();
        return (header, rows);
    }
    (Vec::new(), Vec::new())
}

/// Removes ANSI colour sequences some tools print even when piped.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
