use crate::bookmark::ChromeBookmark;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Debug)]
pub struct SearchResult {
    pub bookmark: ChromeBookmark,
}

pub struct BookmarkSearcher {
    fuzzy_matcher: SkimMatcherV2,
}

impl BookmarkSearcher {
    pub fn new() -> Self {
        Self {
            fuzzy_matcher: SkimMatcherV2::default(),
        }
    }

    pub fn search(
        &self,
        bookmarks: &[ChromeBookmark],
        query: &str,
        folder_filters: &[String],
        fuzzy: bool,
        limit: usize,
    ) -> Vec<SearchResult> {
        if limit == 0 {
            return Vec::new();
        }

        let normalized_folder_filters = normalize_folder_filters(folder_filters);
        let query_lower = query.to_lowercase();

        if query.is_empty() {
            return bookmarks
                .iter()
                .filter(|bookmark| matches_folder_filters(bookmark, &normalized_folder_filters))
                .take(limit)
                .cloned()
                .map(|bookmark| SearchResult { bookmark })
                .collect();
        }

        #[derive(Debug)]
        struct HeapItem {
            score: i64,
            idx: usize,
            bookmark: ChromeBookmark,
        }

        impl PartialEq for HeapItem {
            fn eq(&self, other: &Self) -> bool {
                self.score == other.score && self.idx == other.idx
            }
        }

        impl Eq for HeapItem {}

        impl Ord for HeapItem {
            fn cmp(&self, other: &Self) -> Ordering {
                match self.score.cmp(&other.score) {
                    Ordering::Equal => other.idx.cmp(&self.idx),
                    order => order,
                }
            }
        }

        impl PartialOrd for HeapItem {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut heap: BinaryHeap<std::cmp::Reverse<HeapItem>> = BinaryHeap::new();

        for (idx, bookmark) in bookmarks.iter().enumerate() {
            if !matches_folder_filters(bookmark, &normalized_folder_filters) {
                continue;
            }

            let score = if fuzzy {
                self.fuzzy_search(bookmark, query)
            } else {
                self.exact_search(bookmark, &query_lower)
            };

            if score <= 0 {
                continue;
            }

            let candidate = HeapItem {
                score,
                idx,
                bookmark: bookmark.clone(),
            };

            if heap.len() < limit {
                heap.push(std::cmp::Reverse(candidate));
                continue;
            }

            if let Some(smallest) = heap.peek() {
                let better =
                    score > smallest.0.score || (score == smallest.0.score && idx < smallest.0.idx);
                if better {
                    heap.pop();
                    heap.push(std::cmp::Reverse(candidate));
                }
            }
        }

        let mut items: Vec<HeapItem> = heap.into_iter().map(|item| item.0).collect();
        items.sort_unstable_by(|a, b| b.score.cmp(&a.score).then_with(|| a.idx.cmp(&b.idx)));

        items
            .into_iter()
            .map(|item| SearchResult {
                bookmark: item.bookmark,
            })
            .collect()
    }

    fn fuzzy_search(&self, bookmark: &ChromeBookmark, query: &str) -> i64 {
        let mut max_score = 0i64;

        if let Some(score) = self.fuzzy_matcher.fuzzy_match(&bookmark.name, query) {
            max_score = max_score.max(score * 2);
        }

        if let Some(score) = self.fuzzy_matcher.fuzzy_match(&bookmark.url, query) {
            max_score = max_score.max(score);
        }

        if let Some(ref folder_path) = bookmark.folder_path {
            if let Some(score) = self.fuzzy_matcher.fuzzy_match(folder_path, query) {
                max_score = max_score.max(score / 2);
            }
        }

        max_score
    }

    fn exact_search(&self, bookmark: &ChromeBookmark, query_lower: &str) -> i64 {
        let mut score = 0i64;

        if bookmark.name_lower.contains(query_lower) {
            score += 200;
            if bookmark.name_lower == query_lower {
                score += 100;
            }
            if bookmark.name_lower.starts_with(query_lower) {
                score += 50;
            }
        }

        if bookmark.url_lower.contains(query_lower) {
            score += 100;
        }

        if let Some(ref folder_lower) = bookmark.folder_path_lower {
            if folder_lower.contains(query_lower) {
                score += 50;
            }
        }

        score
    }
}

impl Default for BookmarkSearcher {
    fn default() -> Self {
        Self::new()
    }
}

pub fn normalize_folder_filters(raw_filters: &[String]) -> Vec<Vec<String>> {
    raw_filters
        .iter()
        .filter_map(|raw| normalize_folder_filter(raw))
        .collect()
}

pub fn normalize_folder_filter(raw: &str) -> Option<Vec<String>> {
    let cleaned = raw
        .trim()
        .to_lowercase()
        .replace('\\', "/")
        .replace('>', "/")
        .replace('|', "/");

    let segments: Vec<String> = cleaned
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect();

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

pub fn matches_folder_filters(bookmark: &ChromeBookmark, folder_filters: &[Vec<String>]) -> bool {
    if folder_filters.is_empty() {
        return true;
    }

    let Some(folder_lower) = bookmark.folder_path_lower.as_deref() else {
        return false;
    };

    let folder_segments: Vec<&str> = folder_lower
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();

    folder_filters
        .iter()
        .all(|filter| folder_matches_hierarchy(&folder_segments, filter))
}

pub fn folder_filter_to_like_pattern(raw_filter: &str) -> Option<String> {
    let segments = normalize_folder_filter(raw_filter)?;

    if segments.len() == 1 {
        return Some(format!("%{}%", escape_like_value(&segments[0])));
    }

    let joined = segments
        .iter()
        .map(|segment| format!("%{}%", escape_like_value(segment)))
        .collect::<Vec<_>>()
        .join("/");

    Some(format!("%{}%", joined))
}

fn folder_matches_hierarchy(folder_segments: &[&str], filter_segments: &[String]) -> bool {
    if filter_segments.is_empty() {
        return true;
    }

    if filter_segments.len() == 1 {
        let wanted = &filter_segments[0];
        return folder_segments
            .iter()
            .any(|segment| segment.contains(wanted));
    }

    let mut cursor = 0usize;
    for wanted in filter_segments {
        let mut found = false;
        while cursor < folder_segments.len() {
            if folder_segments[cursor].contains(wanted) {
                found = true;
                cursor += 1;
                break;
            }
            cursor += 1;
        }
        if !found {
            return false;
        }
    }

    true
}

fn escape_like_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmark(id: &str, name: &str, url: &str, folder: Option<&str>) -> ChromeBookmark {
        ChromeBookmark {
            id: id.to_string(),
            name: name.to_string(),
            url: url.to_string(),
            date_added: "0".to_string(),
            folder_path: folder.map(ToString::to_string),
            name_lower: name.to_lowercase(),
            url_lower: url.to_lowercase(),
            folder_path_lower: folder.map(|p| p.to_lowercase()),
        }
    }

    #[test]
    fn exact_search_ranks_full_match_first() {
        let searcher = BookmarkSearcher::new();
        let bookmarks = vec![
            bookmark("1", "rust", "https://rust-lang.org", None),
            bookmark("2", "rust-lang", "https://example.com", None),
            bookmark("3", "other", "https://other.com", None),
        ];

        let results = searcher.search(&bookmarks, "rust", &[], false, 10);
        assert_eq!(results.first().expect("first").bookmark.id, "1");
    }

    #[test]
    fn empty_query_returns_first_n_in_order() {
        let searcher = BookmarkSearcher::new();
        let bookmarks = vec![
            bookmark("1", "one", "https://1.com", None),
            bookmark("2", "two", "https://2.com", None),
            bookmark("3", "three", "https://3.com", None),
        ];

        let results = searcher.search(&bookmarks, "", &[], false, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].bookmark.id, "1");
        assert_eq!(results[1].bookmark.id, "2");
    }

    #[test]
    fn folder_filter_supports_hierarchy_matching() {
        let searcher = BookmarkSearcher::new();
        let bookmarks = vec![
            bookmark(
                "1",
                "rust doc",
                "https://doc.rust-lang.org",
                Some("书签栏/Work/Project/Rust"),
            ),
            bookmark("2", "music", "https://music.example", Some("书签栏/Play")),
        ];

        let results = searcher.search(&bookmarks, "", &vec!["work/project".into()], false, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].bookmark.id, "1");
    }

    #[test]
    fn folder_filter_accepts_partial_segment() {
        let searcher = BookmarkSearcher::new();
        let bookmarks = vec![
            bookmark(
                "1",
                "rust doc",
                "https://doc.rust-lang.org",
                Some("书签栏/Work/Project/Rust"),
            ),
            bookmark("2", "music", "https://music.example", Some("书签栏/Play")),
        ];

        let results = searcher.search(&bookmarks, "", &vec!["proj".into()], false, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].bookmark.id, "1");
    }

    #[test]
    fn like_pattern_escapes_special_chars() {
        let pattern = folder_filter_to_like_pattern("100%/a_b").expect("pattern");
        assert!(pattern.contains("100\\%"));
        assert!(pattern.contains("a\\_b"));
    }

    #[test]
    fn normalize_folder_filter_handles_separators() {
        let normalized = normalize_folder_filter("Work>Project|Rust\\Docs").expect("normalized");
        assert_eq!(normalized, vec!["work", "project", "rust", "docs"]);
    }
}
