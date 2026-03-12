use crate::providers::MediaEntry;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

pub fn normalize(s: &str) -> String {
    s.to_lowercase()
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}

pub fn score_name(name: &str, query: &str) -> u16 {
    if name == query {
        1000
    } else if name.starts_with(query) {
        800
    } else if let Some(pos) = name.find(query) {
        600 - (pos.min(100) as u16)
    } else {
        0
    }
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let n = b.len();
    let mut row: Vec<usize> = (0..=n).collect();
    for ca in a.chars() {
        let mut prev = row[0];
        row[0] += 1;
        for (j, &cb) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = if ca == cb {
                prev
            } else {
                prev.min(row[j]).min(row[j + 1]) + 1
            };
            prev = old;
        }
    }
    row[n]
}

pub fn fuzzy_word_match(word: &str, text: &str) -> bool {
    if text.contains(word) {
        return true;
    }
    word.len() >= 4
        && text
            .split_whitespace()
            .any(|w| w.len().abs_diff(word.len()) <= 1 && edit_distance(w, word) <= 1)
}

pub fn rank_results(entries: Vec<MediaEntry>, query: &str) -> Vec<MediaEntry> {
    let q_norm = normalize(query);
    let q_words: Vec<String> = q_norm.split_whitespace().map(str::to_owned).collect();

    let mut scored: Vec<(u16, MediaEntry)> = entries
        .into_iter()
        .map(|e| {
            let names: Vec<String> = std::iter::once(&e.name)
                .chain(e.alternative_names.iter())
                .map(|n| normalize(n))
                .collect();

            let best_name_score = names
                .iter()
                .map(|t| score_name(t, &q_norm))
                .max()
                .unwrap_or(0);

            let score = if best_name_score > 0 {
                best_name_score
            } else {
                let d = e.description.as_ref().map(|s| normalize(s));
                if d.as_ref().is_some_and(|d| d.contains(q_norm.as_str())) {
                    450
                } else {
                    let total = q_words.len() as u16;
                    let matches = q_words
                        .iter()
                        .filter(|w| {
                            names.iter().any(|t| fuzzy_word_match(w, t))
                                || d.as_ref().is_some_and(|d| d.contains(w.as_str()))
                        })
                        .count() as u16;
                    if total > 0 {
                        matches * 500 / total
                    } else {
                        0
                    }
                }
            };
            (score, e)
        })
        .collect();

    scored.sort_unstable_by(|(sa, ea), (sb, eb)| {
        sb.cmp(sa)
            .then(ea.name.len().cmp(&eb.name.len()))
            .then(ea.name.cmp(&eb.name))
    });

    scored.into_iter().map(|(_, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_accents() {
        assert_eq!(normalize("Café"), "cafe");
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize("HELLO"), "hello");
    }

    #[test]
    fn score_exact_match() {
        assert_eq!(score_name("foo", "foo"), 1000);
    }

    #[test]
    fn score_prefix() {
        assert_eq!(score_name("foobar", "foo"), 800);
    }

    #[test]
    fn score_contains() {
        let s = score_name("bazfoo", "foo");
        assert!(s > 0 && s < 600);
    }

    #[test]
    fn score_no_match() {
        assert_eq!(score_name("abc", "xyz"), 0);
    }

    #[test]
    fn edit_distance_identical() {
        assert_eq!(edit_distance("kitten", "kitten"), 0);
    }

    #[test]
    fn edit_distance_known() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn fuzzy_exact_substring() {
        assert!(fuzzy_word_match("hello", "say hello world"));
    }

    #[test]
    fn fuzzy_close_word() {
        assert!(fuzzy_word_match("hell", "say helo world"));
    }

    #[test]
    fn fuzzy_short_word_no_fuzzy() {
        assert!(!fuzzy_word_match("ab", "cd ef"));
    }
}
