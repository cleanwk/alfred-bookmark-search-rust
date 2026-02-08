use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::process;
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

    let db_path = data_dir.join("bookmarks.db");
    let index = BookmarkIndex::new(db_path).map_err(|e| AppError::DatabaseError(e.to_string()))?;
    let bookmark_cache = BookmarkCache::new(&cache_dir);

    let needs_index = matches!(opt.cmd, SubCommand::Search { .. } | SubCommand::Stats);

    if needs_index {
        let bookmarks_path =
            get_chrome_bookmarks_path_cached(&cache_dir).ok_or(AppError::BookmarksNotFound)?;
        ensure_bookmark_index(&index, &bookmark_cache, &bookmarks_path)?;
    }

    match opt.cmd {
        SubCommand::Search {
            query,
            folders,
            fuzzy,
            limit,
        } => {
            handle_search(query, folders, fuzzy, limit, &index)?;
        }
        SubCommand::Refresh => {
            let bookmarks_path =
                get_chrome_bookmarks_path_cached(&cache_dir).ok_or(AppError::BookmarksNotFound)?;
            bookmark_cache.invalidate();
            index
                .clear_bookmarks_index()
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            ensure_bookmark_index(&index, &bookmark_cache, &bookmarks_path)?;
            show_info_alfred("浏览器书签缓存与索引已刷新");
        }
        SubCommand::Stats => {
            handle_stats(&index)?;
        }
    }

    Ok(())
}

fn ensure_bookmark_index(
    index: &BookmarkIndex,
    cache: &BookmarkCache,
    bookmarks_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let fingerprint = compute_bookmarks_fingerprint(bookmarks_path)?;

    if !index
        .bookmarks_need_refresh(&fingerprint)
        .map_err(|e| AppError::DatabaseError(e.to_string()))?
    {
        return Ok(());
    }

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

    let query_str = query.join(" ");

    let folder_filters: Vec<String> = if let Some(folders_str) = folders {
        normalize_csv_terms(folders_str.split(','))
    } else {
        Vec::new()
    };

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

    struct PrecomputedStrings {
        subtitle: String,
        cmd_subtitle: String,
        opt_subtitle: String,
    }

    let precomputed: Vec<PrecomputedStrings> = bookmarks
        .iter()
        .take(limit)
        .map(|bookmark| {
            let domain = extract_domain(&bookmark.url);
            PrecomputedStrings {
                subtitle: build_subtitle(&bookmark.folder_path, &domain),
                cmd_subtitle: format!("复制URL: {}", bookmark.url),
                opt_subtitle: format!("📂 {}", bookmark.folder_path.as_deref().unwrap_or("未分类")),
            }
        })
        .collect();

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    let mut items = Vec::with_capacity(precomputed.len() + 1);

    for (bookmark, strings) in bookmarks.iter().take(limit).zip(precomputed.iter()) {
        let item = alfred::ItemBuilder::new(&bookmark.name)
            .subtitle(&strings.subtitle)
            .arg(&bookmark.url)
            .uid(&bookmark.id)
            .quicklook_url(&bookmark.url)
            .valid(true)
            .modifier(
                alfred::Modifier::Command,
                Some(&strings.cmd_subtitle),
                Some(&bookmark.url),
                true,
                None,
            )
            .modifier(
                alfred::Modifier::Option,
                Some(&strings.opt_subtitle),
                None::<&str>,
                false,
                None,
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

fn build_subtitle(folder_path: &Option<String>, domain: &str) -> String {
    let mut parts = Vec::new();

    if let Some(path) = folder_path {
        let short_path = path
            .split('/')
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/");

        if !short_path.is_empty() {
            parts.push(format!("📂 {}", short_path));
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

fn show_error_alfred<'a, T: Into<Cow<'a, str>>>(s: T) {
    let item = alfred::ItemBuilder::new("错误")
        .subtitle(s)
        .valid(false)
        .into_item();
    let _ = alfred::json::write_items(io::stdout(), &[item]);
}

fn show_info_alfred<'a, T: Into<Cow<'a, str>>>(s: T) {
    let item = alfred::ItemBuilder::new("✓ 操作完成")
        .subtitle(s)
        .valid(false)
        .into_item();
    let _ = alfred::json::write_items(io::stdout(), &[item]);
}

#[cfg(test)]
mod tests {
    use super::normalize_csv_terms;

    #[test]
    fn normalize_csv_terms_dedupes_and_trims() {
        let terms = normalize_csv_terms(vec![" work ", "work", "project", " "]);
        assert_eq!(terms, vec!["work".to_string(), "project".to_string()]);
    }
}
