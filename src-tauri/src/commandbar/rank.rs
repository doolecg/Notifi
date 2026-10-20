//! Typo-tolerant matching for the Command Bar.

fn fold(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).collect()
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev2: Vec<usize> = Vec::new();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(prev2[j - 2] + 1);
            }
        }
        prev2 = prev;
        prev = cur;
    }
    prev[b.len()]
}

fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower && !cur.is_empty() {
                out.push(fold(&cur));
                cur.clear();
            }
            prev_lower = c.is_lowercase();
            cur.push(c);
        } else {
            if !cur.is_empty() {
                out.push(fold(&cur));
                cur.clear();
            }
            prev_lower = false;
        }
    }
    if !cur.is_empty() {
        out.push(fold(&cur));
    }
    out
}

/// Scores one query word against the text; `None` when it does not match at all.
fn score_word(word: &[char], full: &str, text_words: &[String]) -> Option<i32> {
    let w: String = word.iter().collect();
    if full == w {
        return Some(1000);
    }
    if full.starts_with(&w) {
        return Some(850);
    }
    if text_words.iter().any(|t| *t == w) {
        return Some(700);
    }
    if text_words.iter().any(|t| t.starts_with(&w)) {
        return Some(600);
    }
    if let Some(pos) = full.find(&w) {
        return Some(400 - (full[..pos].chars().count() as i32).min(100));
    }
    if word.len() >= 3 {
        let allowed = if word.len() >= 7 { 2 } else { 1 };
        let best = text_words
            .iter()
            .map(|t| {
                let tc: Vec<char> = t.chars().collect();
                let cut = &tc[..tc.len().min(word.len())];
                edit_distance(word, &tc).min(edit_distance(word, cut) + usize::from(tc.len() > word.len()))
            })
            .min()?;
        if best <= allowed {
            return Some(300 - 80 * best as i32);
        }
    }
    subsequence(word, full)
}

fn subsequence(word: &[char], full: &str) -> Option<i32> {
    if word.len() < 2 {
        return None;
    }
    let mut it = full.chars().enumerate();
    let mut last: Option<usize> = None;
    let mut gaps = 0i32;
    for &c in word {
        let (idx, _) = it.find(|(_, f)| *f == c)?;
        if let Some(l) = last {
            gaps += (idx - l - 1) as i32;
        }
        last = Some(idx);
    }
    Some((200 - gaps * 8).max(40))
}

/// How well `query` matches `text`; every query word must match somewhere.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let query_words = words(query);
    if query_words.is_empty() {
        return None;
    }
    let full = fold(text);
    let text_words = words(text);
    let mut total = 0;
    for w in &query_words {
        let chars: Vec<char> = w.chars().collect();
        total += score_word(&chars, &full, &text_words)?;
    }
    let mut score = total / query_words.len() as i32;
    let squashed: String = fold(query).split_whitespace().collect::<Vec<_>>().join(" ");
    if full == squashed {
        score += 200;
    } else if full.starts_with(&squashed) {
        score += 80;
    }
    Some(score - (full.chars().count() as i32 / 8).min(40))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_beats_prefix_beats_substring() {
        let exact = score("firefox", "Firefox").unwrap();
        let prefix = score("fire", "Firefox").unwrap();
        let sub = score("fox", "Firefox").unwrap();
        assert!(exact > prefix && prefix > sub, "{exact} {prefix} {sub}");
    }

    #[test]
    fn word_prefix_beats_inner_substring() {
        let word = score("stu", "Visual Studio Code").unwrap();
        let inner = score("udi", "Visual Studio Code").unwrap();
        assert!(word > inner);
    }

    #[test]
    fn tolerates_typos() {
        assert!(score("fierfox", "Firefox").is_some());
        assert!(score("chorme", "Google Chrome").is_some());
        assert!(score("notepda", "Notepad").is_some());
        assert!(score("zzzz", "Firefox").is_none());
    }

    #[test]
    fn typo_ranks_below_real_match() {
        let real = score("chrome", "Google Chrome").unwrap();
        let typo = score("chorme", "Google Chrome").unwrap();
        assert!(real > typo);
    }

    #[test]
    fn every_word_must_match() {
        assert!(score("visual code", "Visual Studio Code").is_some());
        assert!(score("visual banana", "Visual Studio Code").is_none());
    }

    #[test]
    fn camel_case_words_split() {
        assert!(score("code", "VSCode").is_some());
        assert_eq!(words("PowerShell core"), vec!["power", "shell", "core"]);
    }

    #[test]
    fn subsequence_fallback() {
        assert!(score("vsc", "Visual Studio Code").is_some());
        assert!(score("v", "Visual Studio Code").is_some());
    }

    #[test]
    fn edit_distance_counts_transposition_once() {
        let a: Vec<char> = "ab".chars().collect();
        let b: Vec<char> = "ba".chars().collect();
        assert_eq!(edit_distance(&a, &b), 1);
    }
}
