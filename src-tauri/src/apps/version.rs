//! Tolerant version comparison (spec/apps/app-updates.md, edge cases).

use std::cmp::Ordering;

#[derive(Debug, PartialEq, Eq)]
enum Tok {
    Num(u64),
    Word(String),
}

fn tokens(version: &str) -> Vec<Tok> {
    let lower = version.trim().to_lowercase();
    let lower = lower.strip_prefix('v').filter(|r| r.starts_with(|c: char| c.is_ascii_digit())).unwrap_or(&lower);
    let mut out = Vec::new();
    let mut chars = lower.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut n = String::new();
            while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                n.push(d);
                chars.next();
            }
            out.push(Tok::Num(n.parse().unwrap_or(u64::MAX)));
        } else if c.is_alphabetic() {
            let mut w = String::new();
            while let Some(&d) = chars.peek().filter(|d| d.is_alphabetic()) {
                w.push(d);
                chars.next();
            }
            out.push(Tok::Word(w));
        } else {
            chars.next();
        }
    }
    out
}

fn pre_rank(word: &str) -> Option<i32> {
    Some(match word {
        "dev" | "snapshot" | "nightly" => 0,
        "alpha" | "a" => 1,
        "beta" | "b" => 2,
        "pre" | "preview" => 3,
        "rc" | "candidate" => 4,
        _ => return None,
    })
}

pub fn is_prerelease(version: &str) -> bool {
    tokens(version).iter().any(|t| matches!(t, Tok::Word(w) if pre_rank(w).is_some()))
}

/// `None` when either side has no number to compare.
pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    let (ta, tb) = (tokens(a), tokens(b));
    if !ta.iter().any(|t| matches!(t, Tok::Num(_))) || !tb.iter().any(|t| matches!(t, Tok::Num(_))) {
        return None;
    }
    for i in 0..ta.len().max(tb.len()) {
        let ord = match (ta.get(i), tb.get(i)) {
            (Some(Tok::Num(x)), Some(Tok::Num(y))) => x.cmp(y),
            (Some(Tok::Word(x)), Some(Tok::Word(y))) => match (pre_rank(x), pre_rank(y)) {
                (Some(p), Some(q)) => p.cmp(&q),
                _ => x.cmp(y),
            },
            (Some(Tok::Num(_)), Some(Tok::Word(w))) => if pre_rank(w).is_some() { Ordering::Greater } else { Ordering::Less },
            (Some(Tok::Word(w)), Some(Tok::Num(_))) => if pre_rank(w).is_some() { Ordering::Less } else { Ordering::Greater },
            (Some(Tok::Num(n)), None) => if *n == 0 { Ordering::Equal } else { Ordering::Greater },
            (None, Some(Tok::Num(n))) => if *n == 0 { Ordering::Equal } else { Ordering::Less },
            (Some(Tok::Word(w)), None) => if pre_rank(w).is_some() { Ordering::Less } else { Ordering::Greater },
            (None, Some(Tok::Word(w))) => if pre_rank(w).is_some() { Ordering::Greater } else { Ordering::Less },
            (None, None) => Ordering::Equal,
        };
        if ord != Ordering::Equal {
            return Some(ord);
        }
    }
    Some(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_v_and_case() {
        assert_eq!(compare("v1.2.0", "1.2.0"), Some(Ordering::Equal));
        assert_eq!(compare("1.0.0-RC1", "1.0.0-rc1"), Some(Ordering::Equal));
    }

    #[test]
    fn numeric_prerelease_numbering() {
        assert_eq!(compare("1.0-beta9", "1.0-beta10"), Some(Ordering::Less));
        assert_eq!(compare("2.0rc10", "2.0rc9"), Some(Ordering::Greater));
    }

    #[test]
    fn release_beats_prerelease() {
        assert_eq!(compare("1.0.0-rc2", "1.0.0"), Some(Ordering::Less));
        assert_eq!(compare("1.0.0", "1.0.0-beta1"), Some(Ordering::Greater));
        assert!(is_prerelease("3.1-beta2") && !is_prerelease("3.1"));
    }

    #[test]
    fn plain_numbers_and_trailing_zero() {
        assert_eq!(compare("1.10", "1.9"), Some(Ordering::Greater));
        assert_eq!(compare("1.2", "1.2.0"), Some(Ordering::Equal));
    }

    #[test]
    fn unusable_versions_are_not_compared() {
        assert_eq!(compare("Unknown", "1.0"), None);
        assert_eq!(compare("", "1.0"), None);
    }
}
