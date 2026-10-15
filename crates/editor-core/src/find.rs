use ropey::Rope;
use std::ops::Range;

pub fn find_matches(rope: &Rope, query: &str, limit: usize) -> Vec<Range<usize>> {
    let needle = fold_case(query);
    if needle.is_empty() || needle.contains('\n') {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for (index, line) in rope.lines().enumerate() {
        let line_start = rope.line_to_char(index);
        let folded = fold_case(&line.to_string());
        let remaining = limit - matches.len();
        let hits = line_matches(&folded, &needle).take(remaining);
        matches.extend(hits.map(|hit| hit.start + line_start..hit.end + line_start));
        if matches.len() >= limit {
            break;
        }
    }
    matches
}

fn line_matches<'a>(folded: &'a str, needle: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
    let needle_chars = needle.chars().count();
    let mut counted_bytes = 0;
    let mut counted_chars = 0;
    folded.match_indices(needle).map(move |(byte, _)| {
        counted_chars += folded[counted_bytes..byte].chars().count();
        counted_bytes = byte;
        counted_chars..counted_chars + needle_chars
    })
}

fn fold_case(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

fn fold_char(ch: char) -> char {
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => ch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, query: &str) -> Vec<Range<usize>> {
        find_matches(&Rope::from_str(text), query, usize::MAX)
    }

    #[test]
    fn matches_iterate_top_down_ignoring_case() {
        let text = "let side = Side::Right;\nfn side() {}\n";
        assert_eq!(found(text, "side"), vec![4..8, 11..15, 27..31]);
    }

    #[test]
    fn matches_do_not_overlap_and_count_chars_not_bytes() {
        assert_eq!(found("aaaa", "aa"), vec![0..2, 2..4]);
        assert_eq!(found("é side", "SIDE"), vec![2..6]);
    }

    #[test]
    fn empty_and_multiline_queries_find_nothing() {
        assert!(found("text", "").is_empty());
        assert!(found("a\nb", "a\nb").is_empty());
    }

    #[test]
    fn the_limit_caps_the_matches() {
        assert_eq!(find_matches(&Rope::from_str("x x x\nx"), "x", 2), vec![0..1, 2..3]);
    }
}
