// ─────────────────────────────────────────────────────────────────────────────
// lcs.rs
//
// Subsequence (LCS-style) matching.
//
// A query is a SUBSEQUENCE of a text when every character of the query appears
// in the text IN ORDER, but not necessarily consecutively.
//
// Examples with query "chr":
//   "Chrome"    → c✓ h✓ r✓  → MATCH    (c at 0, h at 1, r at 3)
//   "character" → c✓ h✓ r✓  → MATCH    (c at 0, h at 2, r at 5)
//   "arch"      → looking for c first, then h, then r
//                 a  r  c✓  h✓  → we found c and h but no r after h → NO MATCH
//
// This is O(len(text)) per call — extremely fast even with 500 apps.
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` if every character of `query` appears inside `text`
/// in the same order (case-insensitive).
pub fn is_subsequence(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true; // empty query matches everything
    }

    // Work with lowercase iterators so the match is case-insensitive.
    let mut query_chars = query.chars().flat_map(|c| c.to_lowercase());
    let mut text_chars  = text .chars().flat_map(|c| c.to_lowercase());

    // Walk through text, consuming one query character each time we find a match.
    // If we exhaust the query first, all characters were found in order → match.
    let mut next_query_char = query_chars.next();

    for tc in &mut text_chars {
        match next_query_char {
            None => break,             // all query chars matched — done
            Some(qc) if qc == tc => {
                next_query_char = query_chars.next(); // advance query
            }
            _ => {} // text char doesn't match current query char — keep scanning
        }
    }

    // If next_query_char is None, we consumed every query character → match.
    next_query_char.is_none()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — run with: cargo test
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_match() {
        assert!(is_subsequence("chr", "Chrome"));
        assert!(is_subsequence("chr", "character"));
        assert!(is_subsequence("chr", "Enchanter"));
    }

    #[test]
    fn test_no_match() {
        // "arch" has a-r-c-h; query c-h-r needs c before h before r.
        // In "arch": c is at index 2, h is at index 3, but there's no r after h.
        assert!(!is_subsequence("chr", "arch"));
        assert!(!is_subsequence("xyz", "Chrome"));
    }

    #[test]
    fn test_case_insensitive() {
        assert!(is_subsequence("CHR", "chrome"));
        assert!(is_subsequence("chr", "CHROME"));
    }

    #[test]
    fn test_empty_query() {
        assert!(is_subsequence("", "anything"));
        assert!(is_subsequence("", ""));
    }

    #[test]
    fn test_exact_match() {
        assert!(is_subsequence("firefox", "Firefox"));
    }

    #[test]
    fn test_longer_query_than_text() {
        assert!(!is_subsequence("firefox123", "Firefox"));
    }
}
