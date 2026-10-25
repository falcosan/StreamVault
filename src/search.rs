use crate::providers::MediaEntry;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

const EXACT: usize = 1000;
const PREFIX: usize = 800;
const CONTAINS: usize = 600;
const DESCRIPTION: usize = 450;
const WORDS: usize = 500;

pub fn rank(entries: Vec<MediaEntry>, query: &str) -> Vec<MediaEntry> {
    let query = normalize(query);
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut scored: Vec<(usize, MediaEntry)> = entries
        .into_iter()
        .map(|entry| (score(&entry, &query, &words), entry))
        .collect();
    scored.sort_by(|(sa, a), (sb, b)| {
        sb.cmp(sa)
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.name.cmp(&b.name))
    });
    scored.into_iter().map(|(_, entry)| entry).collect()
}

fn score(entry: &MediaEntry, query: &str, words: &[&str]) -> usize {
    let names: Vec<String> = std::iter::once(&entry.name)
        .chain(&entry.alternative_names)
        .map(|n| normalize(n))
        .collect();
    let best = names
        .iter()
        .map(|n| name_score(n, query))
        .max()
        .unwrap_or(0);
    if best > 0 {
        return best;
    }
    let description = entry.description.as_deref().map(normalize);
    let described = |text: &str| description.as_deref().is_some_and(|d| d.contains(text));
    if described(query) {
        return DESCRIPTION;
    }
    let matched = words
        .iter()
        .filter(|w| names.iter().any(|n| fuzzy_contains(n, w)) || described(w))
        .count();
    (WORDS * matched).checked_div(words.len()).unwrap_or(0)
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}

fn name_score(name: &str, query: &str) -> usize {
    if name == query {
        EXACT
    } else if name.starts_with(query) {
        PREFIX
    } else if let Some(position) = name.find(query) {
        CONTAINS - position.min(100)
    } else {
        0
    }
}

fn fuzzy_contains(text: &str, word: &str) -> bool {
    if text.contains(word) {
        return true;
    }
    let length = word.chars().count();
    length >= 4
        && text.split_whitespace().any(|candidate| {
            candidate.chars().count().abs_diff(length) <= 1 && edit_distance(candidate, word) <= 1
        })
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for ca in a.chars() {
        let mut diagonal = row[0];
        row[0] += 1;
        for (j, &cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == cb {
                diagonal
            } else {
                diagonal.min(row[j]).min(above) + 1
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, description: Option<&str>) -> MediaEntry {
        MediaEntry {
            name: name.into(),
            description: description.map(String::from),
            ..Default::default()
        }
    }

    fn names(entries: &[MediaEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn normalize_strips_accents_and_case() {
        assert_eq!(normalize("CAFÉ"), "cafe");
    }

    #[test]
    fn name_score_orders_exact_prefix_and_substring() {
        assert_eq!(name_score("foo", "foo"), EXACT);
        assert_eq!(name_score("foobar", "foo"), PREFIX);
        assert_eq!(name_score("bazfoo", "foo"), CONTAINS - 3);
        assert_eq!(name_score("abc", "xyz"), 0);
    }

    #[test]
    fn edit_distance_matches_levenshtein() {
        assert_eq!(edit_distance("kitten", "kitten"), 0);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
    }

    #[test]
    fn fuzzy_contains_tolerates_one_typo_in_long_words() {
        assert!(fuzzy_contains("say hello world", "hello"));
        assert!(fuzzy_contains("say helo world", "hell"));
        assert!(!fuzzy_contains("cd ef", "ab"));
        assert!(fuzzy_contains("ちびまる", "ちびまる子"));
    }

    #[test]
    fn rank_orders_by_relevance_then_shorter_name() {
        let ranked = rank(
            vec![
                entry("The Office", None),
                entry("Office Space", None),
                entry("Office", None),
                entry("Unrelated", Some("an office drama")),
                entry("Nothing", None),
            ],
            "Office",
        );
        assert_eq!(
            names(&ranked),
            [
                "Office",
                "Office Space",
                "The Office",
                "Unrelated",
                "Nothing"
            ]
        );
    }

    #[test]
    fn rank_scores_partial_word_matches() {
        let ranked = rank(
            vec![entry("Breaking Bad", None), entry("Bad Day", None)],
            "breakin badd",
        );
        assert_eq!(names(&ranked), ["Breaking Bad", "Bad Day"]);
    }

    #[test]
    fn rank_handles_long_queries() {
        let query = "word ".repeat(200);
        assert_eq!(rank(vec![entry("word", None)], &query).len(), 1);
    }
}
