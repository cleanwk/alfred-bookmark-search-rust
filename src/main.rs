use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use thiserror::Error;

mod bookmark;
mod cli;
mod index_db;
mod searcher;

use crate::bookmark::{
    compute_bookmarks_fingerprint, get_chrome_bookmarks_path_cached, BookmarkCache,
};
use crate::cli::{Opt, SubCommand};
use crate::index_db::BookmarkIndex;
use crate::searcher::BookmarkSearcher;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("未找到受支持浏览器的书签文件")]
    BookmarksNotFound,
    #[error("读取书签失败: {0}")]
    BookmarksReadError(String),
    #[error("索引数据库错误: {0}")]
    DatabaseError(String),
    #[error("其他错误: {0}")]
    Other(String),
}

const INDEX_CHECK_TTL_MS: u64 = 2_000;
const INDEX_CHECK_STATE_FILE: &str = "index_check_state.json";
const ICON_ACTION_REFRESH: &str = "icons/refresh.png";
const ICON_ACTION_STATS: &str = "icons/stats.png";
const ICON_ACTION_README: &str = "icons/readme.png";
const ICON_ACTION_GUIDE: &str = "icons/guide.png";
const ICON_ACTION_FOLDERS: &str = "icons/folder.png";
const ICON_ACTION_COPY: &str = "icons/copy.png";
const ICON_BOOKMARK: &str = "icons/bookmark.png";

#[derive(Debug, Deserialize, Serialize)]
struct IndexCheckState {
    last_checked_ms: u64,
}

#[derive(Clone)]
struct WorkflowAction {
    title: &'static str,
    subtitle: &'static str,
    arg: &'static str,
    icon_path: &'static str,
}

fn main() {
    let opt: Opt = Opt::from_args();

    if let Err(e) = run(opt) {
        show_error_alfred(e.to_string());
        process::exit(1);
    }
}

fn run(opt: Opt) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = if let Ok(dir) = std::env::var("alfred_workflow_data") {
        std::path::PathBuf::from(dir)
    } else {
        dirs::home_dir()
            .ok_or_else(|| AppError::Other("无法获取home目录".to_string()))?
            .join(".alfred-chrome-bookmarks")
    };

    let cache_dir = if let Ok(dir) = std::env::var("alfred_workflow_cache") {
        std::path::PathBuf::from(dir)
    } else {
        data_dir.clone()
    };

    std::fs::create_dir_all(&data_dir)?;
    if cache_dir != data_dir {
        std::fs::create_dir_all(&cache_dir)?;
    }

    let bookmark_cache = BookmarkCache::new(&cache_dir);

    let needs_index = !matches!(opt.cmd, SubCommand::Actions { .. });
    let needs_ensure_before_command =
        matches!(opt.cmd, SubCommand::Search { .. } | SubCommand::Stats);
    let index = if needs_index {
        let db_path = data_dir.join("bookmarks.db");
        Some(BookmarkIndex::new(db_path).map_err(|e| AppError::DatabaseError(e.to_string()))?)
    } else {
        None
    };

    if needs_ensure_before_command {
        let bookmarks_path =
            get_chrome_bookmarks_path_cached(&cache_dir).ok_or(AppError::BookmarksNotFound)?;
        ensure_bookmark_index(
            index.as_ref().expect("index initialized"),
            &bookmark_cache,
            &bookmarks_path,
            &cache_dir,
        )?;
    }

    match opt.cmd {
        SubCommand::Search {
            query,
            folders,
            fuzzy,
            limit,
        } => {
            handle_search(
                query,
                folders,
                fuzzy,
                limit,
                index.as_ref().expect("index initialized"),
            )?;
        }
        SubCommand::Refresh => {
            let bookmarks_path =
                get_chrome_bookmarks_path_cached(&cache_dir).ok_or(AppError::BookmarksNotFound)?;
            bookmark_cache.invalidate();
            index
                .as_ref()
                .expect("index initialized")
                .clear_bookmarks_index()
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            refresh_bookmark_index(
                index.as_ref().expect("index initialized"),
                &bookmark_cache,
                &bookmarks_path,
            )?;
            mark_index_checked_recently(&cache_dir);
            show_info_alfred("浏览器书签缓存与索引已刷新");
        }
        SubCommand::Stats => {
            handle_stats(index.as_ref().expect("index initialized"))?;
        }
        SubCommand::Actions { query } => {
            handle_actions(query)?;
        }
    }

    Ok(())
}

fn ensure_bookmark_index(
    index: &BookmarkIndex,
    cache: &BookmarkCache,
    bookmarks_path: &Path,
    cache_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if is_index_check_recent(cache_dir, now_ms()) {
        return Ok(());
    }

    let fingerprint = compute_bookmarks_fingerprint(bookmarks_path)?;

    if !index
        .bookmarks_need_refresh(&fingerprint)
        .map_err(|e| AppError::DatabaseError(e.to_string()))?
    {
        mark_index_checked_recently(cache_dir);
        return Ok(());
    }

    refresh_bookmark_index(index, cache, bookmarks_path)?;
    mark_index_checked_recently(cache_dir);

    Ok(())
}

fn refresh_bookmark_index(
    index: &BookmarkIndex,
    cache: &BookmarkCache,
    bookmarks_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let fingerprint = compute_bookmarks_fingerprint(bookmarks_path)?;
    let bookmarks = cache
        .load(bookmarks_path)
        .map_err(|e| AppError::BookmarksReadError(e.to_string()))?;

    index
        .replace_bookmarks(&bookmarks, &fingerprint)
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(())
}

fn handle_search(
    query: Vec<String>,
    folders: Option<String>,
    fuzzy: bool,
    limit: usize,
    index: &BookmarkIndex,
) -> Result<(), Box<dyn std::error::Error>> {
    let searcher = BookmarkSearcher::new();

    let raw_query = query.join(" ");
    let (query_str, inline_folder_filters) = parse_query_and_folder_filters(&raw_query);

    let mut folder_filters: Vec<String> = if let Some(folders_str) = folders {
        normalize_csv_terms(folders_str.split(','))
    } else {
        Vec::new()
    };

    for folder in inline_folder_filters {
        if !folder_filters
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&folder))
        {
            folder_filters.push(folder);
        }
    }

    let fallback_exact =
        || -> Result<Vec<crate::bookmark::ChromeBookmark>, Box<dyn std::error::Error>> {
            let bookmarks = index
                .load_all_bookmarks()
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            let results = searcher.search(&bookmarks, &query_str, &folder_filters, false, limit);
            Ok(results.into_iter().map(|item| item.bookmark).collect())
        };

    let bookmarks = if fuzzy {
        let all = index
            .load_all_bookmarks()
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        searcher
            .search(&all, &query_str, &folder_filters, true, limit)
            .into_iter()
            .map(|item| item.bookmark)
            .collect()
    } else if query_str.is_empty() {
        if folder_filters.is_empty() {
            index
                .list_bookmarks(limit)
                .map_err(|e| AppError::DatabaseError(e.to_string()))?
        } else {
            index
                .list_bookmarks_by_folder_filters(&folder_filters, limit)
                .map_err(|e| AppError::DatabaseError(e.to_string()))?
        }
    } else if folder_filters.is_empty() {
        match index
            .search_bookmarks_fts(&query_str, limit)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?
        {
            Some(results) => results,
            None => fallback_exact()?,
        }
    } else {
        match index
            .search_bookmarks_fts_with_folders(&query_str, &folder_filters, limit)
            .map_err(|e| AppError::DatabaseError(e.to_string()))?
        {
            Some(results) => results,
            None => fallback_exact()?,
        }
    };

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    let mut items = Vec::with_capacity(bookmarks.len());

    for bookmark in bookmarks.iter().take(limit) {
        let domain = extract_domain(&bookmark.url);
        let subtitle = build_subtitle(&bookmark.folder_path, &domain);
        let cmd_subtitle = format!("复制URL: {}", bookmark.url);
        let opt_subtitle = format!("#{}", bookmark.folder_path.as_deref().unwrap_or("未分类"));
        let open_arg = format!("open:{}", bookmark.url);
        let copy_arg = format!("copy:{}", bookmark.url);
        let item = alfred::ItemBuilder::new(&bookmark.name)
            .subtitle(subtitle)
            .arg(open_arg)
            .uid(&bookmark.id)
            .quicklook_url(&bookmark.url)
            .icon_path(ICON_BOOKMARK)
            .valid(true)
            .modifier(
                alfred::Modifier::Command,
                Some(cmd_subtitle),
                Some(copy_arg),
                true,
                Some(alfred::Icon::Path(Cow::Borrowed(ICON_ACTION_COPY))),
            )
            .modifier(
                alfred::Modifier::Option,
                Some(opt_subtitle),
                None::<&str>,
                false,
                Some(alfred::Icon::Path(Cow::Borrowed(ICON_ACTION_FOLDERS))),
            )
            .text_copy(&bookmark.url)
            .text_large_type(&bookmark.name)
            .into_item();

        items.push(item);
    }

    let empty_subtitle = if folder_filters.is_empty() {
        "尝试使用不同的关键词".to_string()
    } else {
        format!(
            "当前目录过滤: {} | 尝试使用不同关键词",
            folder_filters.join(", ")
        )
    };

    if items.is_empty() {
        items.push(
            alfred::ItemBuilder::new("未找到书签")
                .subtitle(&empty_subtitle)
                .valid(false)
                .into_item(),
        );
    }

    alfred::json::write_items(&mut writer, &items)?;
    writer.flush()?;
    Ok(())
}

fn extract_domain(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn index_check_state_path(cache_dir: &Path) -> std::path::PathBuf {
    cache_dir.join(INDEX_CHECK_STATE_FILE)
}

fn is_index_check_recent(cache_dir: &Path, now: u64) -> bool {
    let path = index_check_state_path(cache_dir);
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let state = match serde_json::from_slice::<IndexCheckState>(&bytes) {
        Ok(state) => state,
        Err(_) => return false,
    };

    now.saturating_sub(state.last_checked_ms) <= INDEX_CHECK_TTL_MS
}

fn mark_index_checked_recently(cache_dir: &Path) {
    let path = index_check_state_path(cache_dir);
    let state = IndexCheckState {
        last_checked_ms: now_ms(),
    };

    if let Ok(bytes) = serde_json::to_vec(&state) {
        let _ = std::fs::write(path, bytes);
    }
}

fn normalize_csv_terms<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for value in values {
        let trimmed = value.as_ref().trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            normalized.push(trimmed.to_string());
        }
    }

    normalized
}

fn parse_query_and_folder_filters(raw_query: &str) -> (String, Vec<String>) {
    let mut query_tokens = Vec::new();
    let mut folder_filters: Vec<String> = Vec::new();

    for token in raw_query.split_whitespace() {
        if let Some(value) = token.strip_prefix('#') {
            if value.is_empty() {
                continue;
            }
            let values = normalize_csv_terms(value.split(','));
            append_unique_case_insensitive(&mut folder_filters, values);
            continue;
        }

        if let Some(value) = token
            .strip_prefix("dir:")
            .or_else(|| token.strip_prefix("folder:"))
            .or_else(|| token.strip_prefix("path:"))
            .or_else(|| token.strip_prefix("in:"))
        {
            let values = normalize_csv_terms(value.split(','));
            append_unique_case_insensitive(&mut folder_filters, values);
            continue;
        }

        query_tokens.push(token.to_string());
    }

    (query_tokens.join(" "), folder_filters)
}

fn append_unique_case_insensitive(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
        {
            target.push(value);
        }
    }
}

fn workflow_actions() -> Vec<WorkflowAction> {
    vec![
        WorkflowAction {
            title: "Refresh Index",
            subtitle: "重新扫描书签并重建索引",
            arg: "action:refresh",
            icon_path: ICON_ACTION_REFRESH,
        },
        WorkflowAction {
            title: "Show Stats",
            subtitle: "显示当前书签总数",
            arg: "action:stats",
            icon_path: ICON_ACTION_STATS,
        },
        WorkflowAction {
            title: "Open Workflow Guide",
            subtitle: "打开本地 ALFRED_WORKFLOW_GUIDE.md",
            arg: "action:open_guide",
            icon_path: ICON_ACTION_GUIDE,
        },
        WorkflowAction {
            title: "Open README",
            subtitle: "打开本地 README.md",
            arg: "action:open_readme",
            icon_path: ICON_ACTION_README,
        },
    ]
}

fn build_subtitle(folder_path: &Option<String>, domain: &str) -> String {
    let mut parts = Vec::new();

    if let Some(path) = folder_path {
        let folder_display = path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");

        if !folder_display.is_empty() {
            parts.push(folder_display);
        }
    }

    parts.push(domain.to_string());
    parts.join("  ·  ")
}

fn handle_stats(index: &BookmarkIndex) -> Result<(), Box<dyn std::error::Error>> {
    let total_bookmarks = index
        .get_total_bookmarks()
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    show_info_alfred(format!("书签总数: {}", total_bookmarks));
    Ok(())
}

fn handle_actions(query: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let keyword = query.join(" ").trim().to_lowercase();
    let mut items = Vec::new();

    for action in workflow_actions() {
        if !keyword.is_empty() {
            let title_match = action.title.to_lowercase().contains(&keyword);
            let subtitle_match = action.subtitle.to_lowercase().contains(&keyword);
            if !title_match && !subtitle_match {
                continue;
            }
        }

        items.push(
            alfred::ItemBuilder::new(action.title)
                .subtitle(action.subtitle)
                .arg(action.arg)
                .valid(true)
                .icon_path(action.icon_path)
                .into_item(),
        );
    }

    if items.is_empty() {
        items.push(
            alfred::ItemBuilder::new("未找到动作")
                .subtitle("尝试输入 refresh / stats / guide / readme")
                .valid(false)
                .into_item(),
        );
    }

    alfred::json::write_items(io::stdout(), &items)?;
    Ok(())
}

fn show_error_alfred<'a, T: Into<Cow<'a, str>>>(s: T) {
    let item = alfred::ItemBuilder::new("✗ 操作失败")
        .subtitle(s)
        .icon_path("icons/error.png")
        .valid(false)
        .into_item();
    let _ = alfred::json::write_items(io::stdout(), &[item]);
}

fn show_info_alfred<'a, T: Into<Cow<'a, str>>>(s: T) {
    let item = alfred::ItemBuilder::new("✓ 操作完成")
        .subtitle(s)
        .icon_path("icons/info.png")
        .valid(false)
        .into_item();
    let _ = alfred::json::write_items(io::stdout(), &[item]);
}

#[cfg(test)]
mod tests {
    use super::{
        is_index_check_recent, normalize_csv_terms, now_ms, parse_query_and_folder_filters,
        workflow_actions, IndexCheckState, INDEX_CHECK_STATE_FILE,
    };
    use tempfile::TempDir;

    #[test]
    fn parse_query_extracts_inline_folder_filters() {
        let (query, folders) = parse_query_and_folder_filters("rust dir:work/project folder:tech");
        assert_eq!(query, "rust");
        assert_eq!(
            folders,
            vec!["work/project".to_string(), "tech".to_string()]
        );
    }

    #[test]
    fn parse_query_keeps_regular_terms() {
        let (query, folders) = parse_query_and_folder_filters("rust async tokio");
        assert_eq!(query, "rust async tokio");
        assert!(folders.is_empty());
    }

    #[test]
    fn normalize_csv_terms_dedupes_and_trims() {
        let terms = normalize_csv_terms(vec![" work ", "work", "project", " "]);
        assert_eq!(terms, vec!["work".to_string(), "project".to_string()]);
    }

    #[test]
    fn parse_query_extracts_hash_folder_filters() {
        let (query, folders) = parse_query_and_folder_filters("#work #project rust");
        assert_eq!(query, "rust");
        assert_eq!(folders, vec!["work".to_string(), "project".to_string()]);
    }

    #[test]
    fn parse_query_supports_mixed_hash_and_plain_keywords() {
        let (query, folders) = parse_query_and_folder_filters("tokio #backend #docs async");
        assert_eq!(query, "tokio async");
        assert_eq!(folders, vec!["backend".to_string(), "docs".to_string()]);
    }

    #[test]
    fn parse_query_merges_hash_and_inline_folder_filters() {
        let (query, folders) =
            parse_query_and_folder_filters("rust #work dir:project folder:docs #WORK");
        assert_eq!(query, "rust");
        assert_eq!(
            folders,
            vec![
                "work".to_string(),
                "project".to_string(),
                "docs".to_string()
            ]
        );
    }

    #[test]
    fn parse_query_ignores_empty_hash_token() {
        let (query, folders) = parse_query_and_folder_filters("# rust #");
        assert_eq!(query, "rust");
        assert!(folders.is_empty());
    }

    #[test]
    fn parse_query_accepts_hash_comma_separated_folders() {
        let (query, folders) = parse_query_and_folder_filters("#work,project rust");
        assert_eq!(query, "rust");
        assert_eq!(folders, vec!["work".to_string(), "project".to_string()]);
    }

    #[test]
    fn workflow_actions_contains_core_entries() {
        let actions = workflow_actions();
        assert_eq!(actions.len(), 4);
        assert!(actions.iter().any(|action| action.arg == "action:refresh"));
        assert!(actions.iter().any(|action| action.arg == "action:stats"));
    }

    #[test]
    fn index_check_recent_respects_ttl() {
        let tmp = TempDir::new().expect("tempdir");
        let state_path = tmp.path().join(INDEX_CHECK_STATE_FILE);

        let now = now_ms();
        let state = IndexCheckState {
            last_checked_ms: now.saturating_sub(500),
        };
        let bytes = serde_json::to_vec(&state).expect("serialize state");
        std::fs::write(&state_path, bytes).expect("write state");
        assert!(is_index_check_recent(tmp.path(), now));

        let old_state = IndexCheckState {
            last_checked_ms: now.saturating_sub(10_000),
        };
        let bytes = serde_json::to_vec(&old_state).expect("serialize stale state");
        std::fs::write(state_path, bytes).expect("write stale state");
        assert!(!is_index_check_recent(tmp.path(), now));
    }
}
